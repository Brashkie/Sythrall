"""
Router: Code Graph Visual
Endpoint unificado para los 4 tipos de grafo del panel Diagrama.

POST /analyze/graph   → Import Graph, Call Graph, Circular Deps, Complexity Heatmap
GET  /analyze/graph/types → tipos disponibles
"""

from __future__ import annotations

import asyncio
import re
from typing import Any
from fastapi import APIRouter
from pydantic import BaseModel

from shared import add_log, UPLOADS_DIR
from services.complexity_client import (
    build_call_graph_rust,
    build_centrality_graph_rust,
    build_circular_graph_rust,
    build_import_graph_rust,
)
from services.static_parser import find_cycles_capped
from services.project_service import read_project_files

router = APIRouter()

try:
    import networkx as nx

    HAS_NX = True
except ImportError:
    HAS_NX = False


# ── Schemas ───────────────────────────────────────────────────────────────────


class GraphRequest(BaseModel):
    files: list[dict]  # [{"filename": "...", "content": "..."}]
    graph_type: str = "import"  # import | call | circular | heatmap


# ── Colores heatmap ───────────────────────────────────────────────────────────


def _cc_color(cc: int) -> str:
    if cc <= 5:
        return "#00f5a0"  # verde
    if cc <= 10:
        return "#ffb627"  # amarillo
    if cc <= 20:
        return "#ff8a00"  # naranja
    return "#ff3366"  # rojo


def _cc_level(cc: int) -> str:
    if cc <= 5:
        return "low"
    if cc <= 10:
        return "medium"
    if cc <= 20:
        return "high"
    return "critical"


def _bigo_color(bigo: str) -> str:
    table = {
        "O(1)": "#00f5a0",
        "O(log n)": "#8ef5c0",
        "O(n)": "#ffb627",
        "O(n log n)": "#ff8a00",
        "O(n²)": "#ff3366",
        "O(n³)": "#ff3366",
        "O(2^n)": "#ff3366",
    }
    return table.get(bigo, "#4a5880")


def _bigo_level(bigo: str) -> str:
    if bigo in ("O(1)", "O(log n)"):
        return "efficient"
    if bigo in ("O(n)", "O(n log n)"):
        return "moderate"
    return "expensive"


# ── Endpoints ─────────────────────────────────────────────────────────────────


@router.get("/graph/types")
async def graph_types():
    return {
        "types": [
            {
                "id": "import",
                "label": "Import Graph",
                "description": "Dependencias entre archivos vía imports/requires",
            },
            {
                "id": "call",
                "label": "Call Graph",
                "description": "Qué función llama a cuál dentro de cada archivo",
            },
            {
                "id": "circular",
                "label": "Circular Dependencies",
                "description": "Ciclos en el grafo de dependencias entre módulos",
            },
            {
                "id": "heatmap",
                "label": "Complexity Heatmap",
                "description": "Mapa de calor por complejidad ciclomática y Big-O",
            },
            {
                "id": "centrality",
                "label": "Centrality / Hubs",
                "description": "Archivos más conectados del proyecto — mismo concepto que centralidad en redes sociales, aplicado a dependencias",
            },
        ]
    }


@router.post("/graph")
async def generate_graph(req: GraphRequest) -> dict[str, Any]:
    """
    Genera el grafo visual solicitado a partir de múltiples archivos.
    Retorna tanto datos estructurados como Mermaid para el renderer.
    """
    if not req.files:
        return _empty_response(req.graph_type)

    # Parsear todos los archivos
    parsed_files = await _parse_all(req.files)

    try:
        if req.graph_type == "import":
            return await _build_import_graph(parsed_files)
        elif req.graph_type == "call":
            return await _build_call_graph(parsed_files)
        elif req.graph_type == "circular":
            return await _build_circular_graph(parsed_files)
        elif req.graph_type == "heatmap":
            return _build_heatmap(parsed_files)
        elif req.graph_type == "centrality":
            return await _build_centrality_graph(parsed_files)
        else:
            return {"error": f"Tipo de grafo desconocido: {req.graph_type}"}
    except Exception as e:
        add_log("warn", f"graph error ({req.graph_type}): {e}")
        return {"error": str(e), "mermaid": "", "nodes": [], "edges": []}


# ── Parser helper ─────────────────────────────────────────────────────────────


async def _parse_all(files: list[dict]) -> list[dict]:
    """Parsea todos los archivos con el static parser. `asyncio.gather` en
    vez de awaits secuenciales — `parse_file` consulta el sidecar Rust por
    HTTP para `.py`, no vale la pena serializar esos round-trips."""
    from services.static_parser import parse_file

    to_parse = [(f.get("filename", "unknown"), f.get("content", "")) for f in files if f.get("content", "").strip()]
    parsed_list = await asyncio.gather(*(parse_file(fname, content) for fname, content in to_parse))

    results = []
    for (fname, content), parsed in zip(to_parse, parsed_list, strict=True):
        parsed["_filename"] = fname
        parsed["_content"] = content
        results.append(parsed)
    return results


# ══════════════════════════════════════════════════════════════════════════════
#  EDGES / DIGRAPH COMPARTIDOS (Fase 22 — Architecture smells)
#  `_build_import_graph`/`_build_circular_graph`/`_build_centrality_graph`
#  reconstruían el mismo loop archivo→archivo 3 veces, cada una casi
#  idéntica. Extraído acá para que `_build_architecture_smells` (abajo) no
#  sea una 4ta copia — un solo shape de edge, un solo builder de DiGraph.
# ══════════════════════════════════════════════════════════════════════════════


def _build_project_edges(parsed_files: list[dict]) -> list[dict]:
    """Edges archivo→archivo, solo entre archivos que el proyecto ya trae
    (imports a librerías externas no generan edge, no hay archivo del
    proyecto que resolver). Shape único `{"from", "to", "via", "line"}` —
    antes `_build_import_graph` incluía `"line"` y los otros dos builders no;
    unificado porque ningún test compara el dict completo, solo hace
    `.get()`/`in` sobre keys puntuales."""
    file_names = {p["_filename"] for p in parsed_files}
    edges: list[dict] = []
    seen: set[str] = set()

    for p in parsed_files:
        src = p["_filename"]
        for imp in p.get("imports", []):
            mod = imp.get("module", "")
            for candidate in _module_to_candidates(mod, src):
                if candidate in file_names and candidate != src:
                    key = f"{src}→{candidate}"
                    if key not in seen:
                        seen.add(key)
                        edges.append({"from": src, "to": candidate, "via": mod, "line": imp.get("line", 0)})
                    break

    return edges


def _build_project_digraph(parsed_files: list[dict], edges: list[dict]) -> nx.DiGraph:
    """DiGraph con TODOS los archivos como nodos (incluso sin edges) antes de
    agregar las edges — así un archivo aislado aparece con in/out-degree 0
    en vez de estar ausente del grafo. `nx.simple_cycles` ignora nodos
    aislados de todos modos, así que esto no cambia el resultado de
    `find_cycles_capped` para `_build_architecture_smells` — su único caller
    ahora que Import/Centrality/Call/Circular Graph ya se portaron todos a
    Rust (Fase 18)."""
    G = nx.DiGraph()
    for p in parsed_files:
        G.add_node(p["_filename"])
    for e in edges:
        G.add_edge(e["from"], e["to"])
    return G


# ══════════════════════════════════════════════════════════════════════════════
#  IMPORT GRAPH
# ══════════════════════════════════════════════════════════════════════════════


def _files_summary_for_sidecar(parsed_files: list[dict]) -> list[dict]:
    """Resumen ya parseado de cada archivo (filename/language/functions-count/
    imports/dead-code-count) — el shape que espera el sidecar Rust para
    construir cualquier grafo a nivel de proyecto (`FileSummary` en
    `services/complexity/src/graph.rs`). Compartido por `_build_import_graph`
    y `_build_centrality_graph` (Fase 18, Graph Engine) — ambos necesitan
    exactamente el mismo payload, solo cambia a qué endpoint del sidecar se
    postea."""
    return [
        {
            "filename": p["_filename"],
            "language": p.get("language", "?"),
            "functions": len(p.get("functions", [])),
            "imports": [{"module": imp.get("module", ""), "line": imp.get("line", 0)} for imp in p.get("imports", [])],
            "dead_code": len(p.get("dead_code", [])),
        }
        for p in parsed_files
    ]


async def _build_import_graph(parsed_files: list[dict]) -> dict[str, Any]:
    """
    Construye el grafo de dependencias entre archivos.
    Nodo = archivo, Edge = import entre archivos del proyecto.

    Fase 18, primer slice del Graph Engine: el cómputo (nodos/edges/mermaid/
    entry_points/summary) vive ahora en el sidecar Rust
    (`services/complexity/src/graph.rs::build_import_graph`, expuesto vía
    `POST /graph/import`) — mismo criterio Rust-only que Halstead/seguridad/
    smells en esta misma fase: sin fallback Python, degradación explícita si
    el sidecar no responde. `_build_project_edges`/`_short_name`/`_safe_id`/
    `_module_to_candidates` siguen viviendo acá porque Call/Circular Graph
    todavía los usan (slices siguientes de esta misma fase).
    """
    result = await build_import_graph_rust(_files_summary_for_sidecar(parsed_files))
    if result is None:
        add_log("warn", "graph error (import): complexity-engine sidecar no disponible")
        return {
            "graph_type": "import",
            "nodes": [],
            "edges": [],
            "mermaid": "flowchart TD\n    A[complexity-engine no disponible]",
            "entry_points": [],
            "summary": {"total_files": 0, "total_imports": 0, "isolated": 0},
            "error": "complexity-engine sidecar no disponible",
        }
    return result


# ══════════════════════════════════════════════════════════════════════════════
#  CALL GRAPH
# ══════════════════════════════════════════════════════════════════════════════


def _call_graph_payload(parsed_files: list[dict]) -> list[dict]:
    """Shape que espera el sidecar Rust para Call Graph (`CallGraphFileInput`
    en `services/complexity/src/graph.rs`) — distinto de
    `_files_summary_for_sidecar`: acá hace falta detalle por función
    (name/big_o/complexity/line) y el `call_graph` ya calculado por archivo,
    no imports/dead_code."""
    return [
        {
            "filename": p["_filename"],
            "functions": [
                {
                    "name": fn["name"],
                    "big_o": fn.get("big_o", "?"),
                    "complexity": fn.get("complexity", 1),
                    "line": fn.get("line", 0),
                }
                for fn in p.get("functions", [])
            ],
            "call_graph": [{"from": e["from"], "to": e["to"]} for e in p.get("call_graph", [])],
        }
        for p in parsed_files
    ]


async def _build_call_graph(parsed_files: list[dict]) -> dict[str, Any]:
    """
    Call graph: qué función llama a cuál.
    Agrupa por archivo, muestra las conexiones intra e inter archivo.

    Fase 18, tercera porción del Graph Engine: el cómputo (pura agregación
    del `call_graph` que cada archivo ya trae precalculado — no construye
    nada nuevo) vive ahora en el sidecar Rust
    (`services/complexity/src/graph.rs::build_call_graph`, expuesto vía
    `POST /graph/call`) — mismo criterio Rust-only que Import Graph/Centrality:
    sin fallback Python, degradación explícita si el sidecar no responde.
    `_bigo_color`/`_bigo_level` no se tocan — `_build_heatmap` (todavía
    Python) también los usa.
    """
    result = await build_call_graph_rust(_call_graph_payload(parsed_files))
    if result is None:
        add_log("warn", "graph error (call): complexity-engine sidecar no disponible")
        return {
            "graph_type": "call",
            "nodes": [],
            "edges": [],
            "mermaid": "flowchart TD\n    A[complexity-engine no disponible]",
            "summary": {"total_functions": 0, "total_calls": 0, "hot_paths": []},
            "error": "complexity-engine sidecar no disponible",
        }
    return result


# ══════════════════════════════════════════════════════════════════════════════
#  CIRCULAR DEPENDENCIES
# ══════════════════════════════════════════════════════════════════════════════


async def _build_circular_graph(parsed_files: list[dict]) -> dict[str, Any]:
    """
    Detecta y visualiza ciclos en el grafo de imports.

    Fase 18, cuarta y última porción del Graph Engine: el cómputo (incluyendo
    la enumeración de ciclos, antes vía `find_cycles_capped`/`nx.simple_cycles`)
    vive ahora en el sidecar Rust
    (`services/complexity/src/graph.rs::build_circular_graph`, expuesto vía
    `POST /graph/circular`) — mismo criterio Rust-only que el resto de esta
    fila: sin fallback Python, degradación explícita si el sidecar no
    responde. El buscador de ciclos en Rust es un DFS acotado, no un puerto
    de Johnson's algorithm (`nx.simple_cycles`) — ningún test depende del
    orden/contenido exacto de la enumeración de NetworkX, solo de cuenta/
    pertenencia, así que no hacía falta replicarlo ni agregar `petgraph`
    (ver el comentario de sección de `graph.rs` para el detalle del
    algoritmo). `find_cycles_capped`/`_build_project_digraph`/`HAS_NX`/`nx`
    no se tocan — `_build_architecture_smells` (Fase 22) los sigue usando.
    """
    result = await build_circular_graph_rust(_files_summary_for_sidecar(parsed_files))
    if result is None:
        add_log("warn", "graph error (circular): complexity-engine sidecar no disponible")
        return {
            "graph_type": "circular",
            "nodes": [],
            "edges": [],
            "cycles": [],
            "mermaid": "flowchart TD\n    A[complexity-engine no disponible]",
            "has_cycles": False,
            "summary": {"total_files": 0, "total_cycles": 0, "affected_files": 0, "cycle_descriptions": []},
            "error": "complexity-engine sidecar no disponible",
        }
    return result


# ══════════════════════════════════════════════════════════════════════════════
#  CENTRALITY / HUB DETECTION (Fase 14)
# ══════════════════════════════════════════════════════════════════════════════


async def _build_centrality_graph(parsed_files: list[dict]) -> dict[str, Any]:
    """
    Centralidad/hub detection sobre el import graph: qué archivos son más
    "influyentes" en el proyecto — mismo concepto que centralidad en redes
    sociales (grado de conexión), aplicado a dependencias entre módulos.

    Fase 18, segunda porción del Graph Engine: el cómputo vive ahora en el
    sidecar Rust (`services/complexity/src/graph.rs::build_centrality_graph`,
    expuesto vía `POST /graph/centrality`) — mismo criterio Rust-only que
    Import Graph (slice anterior de esta misma fase): sin fallback Python,
    degradación explícita si el sidecar no responde. Esto resuelve además la
    nota que dejó Fase 14 al shippear esto por primera vez ("deliberately
    still Python — Phase 18's Graph Engine needs the import/call graph
    construction itself in Rust first, which doesn't exist yet") — esa
    construcción ya está en Rust desde el slice de Import Graph.

    "Hub" = entre los 5 archivos con más in-degree (más otros archivos
    dependen de él — cambiarlo tiene el radio de impacto más grande) Y con
    al menos 2 dependientes, para no marcar como hub un archivo con una sola
    import entrante.
    """
    result = await build_centrality_graph_rust(_files_summary_for_sidecar(parsed_files))
    if result is None:
        add_log("warn", "graph error (centrality): complexity-engine sidecar no disponible")
        return {
            "graph_type": "centrality",
            "nodes": [],
            "edges": [],
            "mermaid": "flowchart TD\n    A[complexity-engine no disponible]",
            "summary": {"total_files": 0, "hubs": [], "max_in_degree": 0},
            "error": "complexity-engine sidecar no disponible",
        }
    return result


# ══════════════════════════════════════════════════════════════════════════════
#  ARCHITECTURE SMELLS (Fase 22 — último ítem de Code Quality Intelligence)
#  Python-only por ahora: el import graph cross-file (_module_to_candidates,
#  arriba) solo existe acá — no fue portado a Rust. Portarlo ("Graph Engine")
#  es un ítem propio y más grande de la Fase 18, deliberadamente no abordado
#  en esta pasada. Caso espejo de Halstead (Rust-only, sin fallback Python
#  porque sus datos solo existen en Rust): acá los datos (el grafo cross-
#  file) solo existen en Python, así que el feature es Python-only por la
#  razón inversa — no una excepción al mandato Rust-first, su contraparte.
# ══════════════════════════════════════════════════════════════════════════════

_HIGH_EFFERENT_COUPLING = 15  # sin número de literatura Fowler/Martin para
# conteos crudos de efferent coupling (a diferencia de LOC/métodos de los
# structural smells, que sí tienen precedente). Calibrado contra
# apps/api/main.py, que hoy importa 11 módulos del proyecto como composition
# root legítimo — el umbral queda arriba de eso para no marcar el propio
# entrypoint como smell.
_UNSTABLE_MIN_CA = 3  # un archivo más estricto que el piso de "hub" de
# _build_centrality_graph (≥2) — una segunda señal de coupling independiente
# debería ser más exigente, no repetir el mismo umbral.
_UNSTABLE_INSTABILITY = 0.5  # punto medio natural de la métrica de
# inestabilidad de Robert Martin (I = Ce/(Ca+Ce)), no un número arbitrario.


def _build_architecture_smells(parsed_files: list[dict], cycles: list[list[str]] | None = None) -> list[dict]:
    """
    Smells de arquitectura: acoplamiento eferente alto, dependencia inestable
    (afferent alto + inestabilidad alta — un módulo muy usado por otros que a
    la vez es frágil, un cambio se propaga en las dos direcciones), y
    dependencia circular reencuadrada como un smell más (antes solo vivía
    como su propio grafo en _build_circular_graph) — la "violación de capas
    general" que pide la Fase 22 del roadmap.

    Mismo shape {kind, name, line, message} que structural_smells/
    naming_smells, con dos diferencias deliberadas: `line` siempre es 0 acá
    (estos smells son de archivo/grafo, no de línea) y `name` lleva la ruta
    completa del archivo en vez de un basename corto — no hay un campo
    `file` separado como en los otros dos smells (ya son globales, no se
    agregan por archivo), así que `name` tiene que bastar por sí solo para
    distinguir dos archivos con el mismo basename en carpetas distintas.

    `cycles`, si se pasa (ej. desde parse_project, que ya corrió
    _build_circular_graph), evita recalcular find_cycles_capped una segunda
    vez sobre el mismo grafo.
    """
    edges = _build_project_edges(parsed_files)
    file_names = [p["_filename"] for p in parsed_files]
    smells: list[dict] = []

    if HAS_NX:
        G = _build_project_digraph(parsed_files, edges)
        in_degree = dict(G.in_degree())
        out_degree = dict(G.out_degree())
        if cycles is None:
            cycles = find_cycles_capped(G, max_cycles=20)
    else:
        in_degree = {f: 0 for f in file_names}
        out_degree = {f: 0 for f in file_names}
        cycles = cycles or []

    for fname in file_names:
        ca, ce = in_degree.get(fname, 0), out_degree.get(fname, 0)
        if ce > _HIGH_EFFERENT_COUPLING:
            smells.append(
                {
                    "kind": "high_efferent_coupling",
                    "name": fname,
                    "line": 0,
                    "message": (
                        f"{ce} imports internos del proyecto (> {_HIGH_EFFERENT_COUPLING}) — si no es "
                        f"un punto de composición intencional (entrypoint, registro de routers), "
                        f"considerar dividir responsabilidades."
                    ),
                }
            )
        instability = ce / (ca + ce) if (ca + ce) else 0.0
        if ca >= _UNSTABLE_MIN_CA and instability > _UNSTABLE_INSTABILITY:
            smells.append(
                {
                    "kind": "unstable_dependency",
                    "name": fname,
                    "line": 0,
                    "message": (
                        f"{ca} archivo(s) dependen de este módulo, pero él mismo depende de {ce} — "
                        f"inestabilidad {instability:.2f} (umbral {_UNSTABLE_INSTABILITY}): un cambio "
                        f"acá se propaga tanto hacia arriba como hacia abajo."
                    ),
                }
            )

    for cycle in cycles or []:
        smells.append(
            {
                "kind": "circular_dependency",
                "name": " → ".join(cycle) + f" → {cycle[0]}",  # mismo formato que cycle_descriptions
                "line": 0,
                "message": (
                    f"Ciclo de imports entre {len(cycle)} archivo(s) — ninguno puede entenderse/"
                    f"testearse en aislamiento del otro."
                ),
            }
        )

    return smells


# ══════════════════════════════════════════════════════════════════════════════
#  COMPLEXITY HEATMAP
# ══════════════════════════════════════════════════════════════════════════════


def _build_heatmap(parsed_files: list[dict]) -> dict[str, Any]:
    """
    Mapa de calor de complejidad: todos los archivos × todas las funciones.
    Colorea por CC y/o Big-O.
    """
    functions: list[dict] = []

    for p in parsed_files:
        fname = p["_filename"]
        for fn in p.get("functions", []):
            cc = fn.get("complexity", 1)
            bigo = fn.get("big_o", "?")
            functions.append(
                {
                    "file": fname,
                    "file_short": _short_name(fname),
                    "name": fn["name"],
                    "line": fn.get("line", 0),
                    "cc": cc,
                    "cc_color": _cc_color(cc),
                    "cc_level": _cc_level(cc),
                    "big_o": bigo,
                    "bigo_color": _bigo_color(bigo),
                    "bigo_level": _bigo_level(bigo),
                    "loc": fn.get("loc", 0),
                }
            )

    # Ordenar: más problemáticas primero
    level_order = {"critical": 0, "expensive": 0, "high": 1, "medium": 1, "moderate": 2, "low": 3, "efficient": 3}
    functions.sort(key=lambda f: (level_order.get(f["cc_level"], 5) + level_order.get(f["bigo_level"], 5), -f["cc"]))

    mermaid = _heatmap_to_mermaid(functions)

    # Stats
    critical = [f for f in functions if f["cc_level"] in ("critical", "high")]
    hot = [f for f in functions if f["bigo_level"] == "expensive"]
    avg_cc = round(sum(f["cc"] for f in functions) / len(functions), 2) if functions else 0

    return {
        "graph_type": "heatmap",
        "functions": functions,
        "mermaid": mermaid,
        "summary": {
            "total_functions": len(functions),
            "avg_cc": avg_cc,
            "critical_count": len(critical),
            "hot_paths": len(hot),
            "by_level": {
                "low": sum(1 for f in functions if f["cc_level"] == "low"),
                "medium": sum(1 for f in functions if f["cc_level"] == "medium"),
                "high": sum(1 for f in functions if f["cc_level"] == "high"),
                "critical": sum(1 for f in functions if f["cc_level"] == "critical"),
            },
        },
    }


def _heatmap_to_mermaid(functions: list[dict]) -> str:
    """Genera un flowchart coloreado por complejidad."""
    if not functions:
        return "flowchart TD\n    A[Sin funciones]"

    lines = ["flowchart LR"]

    # Agrupar por archivo
    by_file: dict[str, list[dict]] = {}
    for fn in functions:
        by_file.setdefault(fn["file_short"], []).append(fn)

    for file_short, fns in by_file.items():
        fid = _safe_id(file_short)
        lines.append(f'    subgraph {fid}["{file_short}"]')
        for fn in fns:
            fnid = _safe_id(f"{file_short}_{fn['name']}")
            cc = fn["cc"]
            bigo = fn["big_o"]
            # El color del nodo (fill/stroke por fn["cc_color"], estilado más
            # abajo) ya ES el heatmap — un ícono de semáforo acá repetiría la
            # misma señal de severidad dos veces.
            lines.append(f'    {fnid}["{fn["name"]}\\nCC={cc} · {bigo}"]')
        lines.append("    end")

    # Estilos por nivel de CC
    for fn in functions:
        fnid = _safe_id(f"{fn['file_short']}_{fn['name']}")
        fill = fn["cc_color"] + "20"
        stroke = fn["cc_color"]
        lines.append(f"    style {fnid} fill:{fill},stroke:{stroke}")

    return "\n".join(lines) + "\n"


# ── Utilidades ────────────────────────────────────────────────────────────────


def _safe_id(s: str) -> str:
    """Convierte un string a ID válido para Mermaid."""
    return re.sub(r"[^a-zA-Z0-9_]", "_", s)


def _short_name(path: str) -> str:
    """Retorna solo el nombre del archivo sin ruta."""
    return path.split("/")[-1].split("\\")[-1]


def _module_to_candidates(module: str, source_file: str = "") -> list[str]:
    """
    Genera posibles nombres de archivo desde un módulo Python/JS/TS.
    Si se proporciona source_file, busca primero en la misma carpeta.
    Esto permite resolver cross-folder deps correctamente.
    """
    import os

    source_dir = os.path.dirname(source_file)  # e.g. "backend" o "frontend"

    # Nombre base del módulo
    if module.startswith("."):
        # Relative import: ./api → api
        short = module.lstrip("./").split("/")[-1]
    else:
        short = module.split("/")[-1].split(".")[-1] if "/" in module or "." in module else module

    candidates: list[str] = []

    # 1. Buscar en la misma carpeta que el archivo fuente
    if source_dir:
        for ext in (".py", ".ts", ".js"):
            candidates.append(f"{source_dir}/{short}{ext}")
        candidates.append(f"{source_dir}/{short}/index.ts")
        candidates.append(f"{source_dir}/{short}/index.js")

    # 2. Buscar como path relativo explícito (e.g. ./utils)
    if module.startswith(".") and source_dir:
        rel = module.lstrip("./")
        for ext in (".py", ".ts", ".js"):
            candidates.append(f"{source_dir}/{rel}{ext}")

    # 3. Fallback: plano sin carpeta
    for ext in (".py", ".ts", ".js"):
        candidates.append(f"{short}{ext}")
    candidates.append(f"{short}/index.ts")
    candidates.append(f"{short}/index.js")

    # Deduplicar preservando orden
    seen: set[str] = set()
    unique: list[str] = []
    for c in candidates:
        if c not in seen:
            seen.add(c)
            unique.append(c)
    return unique


def _empty_response(graph_type: str) -> dict:
    base = {
        "graph_type": graph_type,
        "nodes": [],
        "edges": [],
        "mermaid": "flowchart TD\n    A[Sin archivos cargados]",
        "summary": {},
    }
    if graph_type == "circular":
        base["cycles"] = []
        base["has_cycles"] = False
    if graph_type == "heatmap":
        base["functions"] = []
    if graph_type == "centrality":
        base["summary"] = {"total_files": 0, "hubs": [], "max_in_degree": 0}
    return base


# ══════════════════════════════════════════════════════════════════════════════
#  FASE 2: Graph desde proyectos subidos (ZIP/upload)
#  Endpoint que lee archivos de un project_id ya subido
# ══════════════════════════════════════════════════════════════════════════════


class ProjectGraphRequest(BaseModel):
    project_id: str
    graph_type: str = "import"


@router.post("/graph/project")
async def generate_project_graph(req: ProjectGraphRequest) -> dict[str, Any]:
    """
    Genera un grafo visual a partir de un proyecto ya subido via upload.
    Lee todos los archivos de código del proyecto, los parsea y genera el grafo.
    Soporta proyectos con estructura de carpetas (frontend/, backend/, etc.)
    """

    try:
        # Obtener árbol del proyecto
        project_dir = UPLOADS_DIR / req.project_id

        if not project_dir.exists():
            return {"error": f"Proyecto {req.project_id} no encontrado", "mermaid": "", "nodes": [], "edges": []}

        files_for_graph = read_project_files(project_dir)

        if not files_for_graph:
            return _empty_response(req.graph_type)

        # Parsear y generar grafo
        parsed_files = await _parse_all(files_for_graph)

        if req.graph_type == "import":
            result = await _build_import_graph(parsed_files)
        elif req.graph_type == "call":
            result = await _build_call_graph(parsed_files)
        elif req.graph_type == "circular":
            result = await _build_circular_graph(parsed_files)
        elif req.graph_type == "heatmap":
            result = _build_heatmap(parsed_files)
        elif req.graph_type == "centrality":
            result = await _build_centrality_graph(parsed_files)
        else:
            return {"error": f"Tipo desconocido: {req.graph_type}"}

        # Agregar metadata del proyecto
        result["project_id"] = req.project_id
        result["total_files"] = len(files_for_graph)
        result["file_list"] = [f["filename"] for f in files_for_graph]

        # Agregar árbol de directorios para Tree View
        result["dir_tree"] = _build_dir_tree(files_for_graph, parsed_files)

        add_log("info", f"graph/project: {req.project_id} ({len(files_for_graph)} files, type={req.graph_type})")
        return result

    except Exception as e:
        add_log("warn", f"graph/project error: {e}")
        return {"error": str(e), "mermaid": "", "nodes": [], "edges": []}


def _build_dir_tree(
    files: list[dict],
    parsed: list[dict],
) -> dict:
    """
    Construye el árbol de directorios con metadata de complejidad.
    Estructura: { name, type, path, children, stats }
    """
    # Crear índice de stats por archivo
    stats_by_file: dict[str, dict] = {}
    for p in parsed:
        fname = p["_filename"]
        funcs = p.get("functions", [])
        stats_by_file[fname] = {
            "functions": len(funcs),
            "avg_cc": round(sum(f.get("complexity", 1) for f in funcs) / len(funcs), 1) if funcs else 0,
            "hot_paths": sum(1 for f in funcs if f.get("big_o", "") in ("O(n²)", "O(n³)", "O(2^n)")),
            "language": p.get("language", "?"),
            "imports": len(p.get("imports", [])),
            "dead_code": len(p.get("dead_code", [])),
        }

    # Construir árbol
    root: dict = {"name": "root", "type": "directory", "path": "", "children": {}, "stats": {}}

    for f in files:
        parts = f["filename"].split("/")
        node = root
        for i, part in enumerate(parts):
            path = "/".join(parts[: i + 1])
            if part not in node["children"]:
                is_file = i == len(parts) - 1
                node["children"][part] = {
                    "name": part,
                    "type": "file" if is_file else "directory",
                    "path": path,
                    "children": {},
                    "stats": stats_by_file.get(path, {}) if is_file else {},
                }
            node = node["children"][part]

    def _to_list(node: dict) -> dict:
        result = {
            "name": node["name"],
            "type": node["type"],
            "path": node["path"],
            "stats": node["stats"],
        }
        if node["children"]:
            result["children"] = sorted(
                [_to_list(child) for child in node["children"].values()],
                key=lambda x: (0 if x["type"] == "directory" else 1, x["name"]),
            )
        return result

    return _to_list(root)
