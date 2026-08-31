"""
Router: Code Graph Visual
Endpoint unificado para los 5 tipos de grafo del panel Diagrama — todos
Rust-only ahora (Fase 18, Graph Engine): Import, Call, Circular, Centrality
y Heatmap (la última pieza en migrar) viven en
`services/complexity/src/graph.rs`; este archivo solo parsea/orquesta y
degrada con gracia si el sidecar no responde.

POST /analyze/graph   → Import Graph, Call Graph, Circular Deps, Complexity Heatmap, Centrality
GET  /analyze/graph/types → tipos disponibles
"""

from __future__ import annotations

import asyncio
from typing import Any
from fastapi import APIRouter
from pydantic import BaseModel

from shared import add_log, UPLOADS_DIR
from services.complexity_client import (
    build_architecture_smells_rust,
    build_call_graph_rust,
    build_centrality_graph_rust,
    build_circular_graph_rust,
    build_heatmap_rust,
    build_import_graph_rust,
)
from services.static_parser import parse_project_files

router = APIRouter()


# ── Schemas ───────────────────────────────────────────────────────────────────


class GraphRequest(BaseModel):
    files: list[dict]  # [{"filename": "...", "content": "..."}]
    graph_type: str = "import"  # import | call | circular | heatmap


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
            return await _build_heatmap(parsed_files)
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
    for (fname, _content), parsed in zip(to_parse, parsed_list, strict=True):
        parsed["_filename"] = fname
        results.append(parsed)
    return results


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


_SIDECAR_DOWN_MSG = "complexity-engine sidecar no disponible"


def _graph_degraded(graph_type: str, summary: dict, **extra) -> dict:
    """Shape de degradación compartido por los builders de grafo cuando el
    sidecar no responde — antes cada uno tenía su propia copia casi idéntica
    de este dict más su propio `add_log`. `summary` varía por tipo de grafo
    (cada uno tiene sus propias keys); `extra` cubre los campos puntuales que
    solo Circular Deps necesita (`cycles`/`has_cycles`) o Import Graph
    (`entry_points`)."""
    add_log("warn", f"graph error ({graph_type}): {_SIDECAR_DOWN_MSG}")
    return {
        "graph_type": graph_type,
        "nodes": [],
        "edges": [],
        "mermaid": f"flowchart TD\n    A[{_SIDECAR_DOWN_MSG}]",
        "summary": summary,
        "error": _SIDECAR_DOWN_MSG,
        **extra,
    }


async def _build_import_graph(parsed_files: list[dict]) -> dict[str, Any]:
    """
    Construye el grafo de dependencias entre archivos.
    Nodo = archivo, Edge = import entre archivos del proyecto.

    Fase 18, primer slice del Graph Engine: el cómputo (nodos/edges/mermaid/
    entry_points/summary) vive ahora en el sidecar Rust
    (`services/complexity/src/graph.rs::build_import_graph`, expuesto vía
    `POST /graph/import`) — mismo criterio Rust-only que Halstead/seguridad/
    smells en esta misma fase: sin fallback Python, degradación explícita si
    el sidecar no responde. `_build_project_edges`/`_module_to_candidates` se
    eliminaron del todo — ya viven en `graph.rs`
    (`build_project_edges`/`module_to_candidates`) y Architecture Smells
    (Fase 18, "Dependency Engine") era su único otro caller Python. Heatmap
    también se portó del todo — `_short_name`/`_safe_id` ya no viven acá,
    quedaron como `short_name`/`safe_id` en `graph.rs`.
    """
    result = await build_import_graph_rust(_files_summary_for_sidecar(parsed_files))
    if result is None:
        return _graph_degraded("import", {"total_files": 0, "total_imports": 0, "isolated": 0}, entry_points=[])
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
    """
    result = await build_call_graph_rust(_call_graph_payload(parsed_files))
    if result is None:
        return _graph_degraded("call", {"total_functions": 0, "total_calls": 0, "hot_paths": []})
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
    algoritmo). `find_cycles_capped` (Python, en `static_parser.py`) y el
    import de `networkx` en este archivo se eliminaron del todo — su último
    caller (`_build_architecture_smells`) se cortó a Rust en la misma pasada
    (Fase 18, "Dependency Engine").
    """
    result = await build_circular_graph_rust(_files_summary_for_sidecar(parsed_files))
    if result is None:
        return _graph_degraded(
            "circular",
            {"total_files": 0, "total_cycles": 0, "affected_files": 0, "cycle_descriptions": []},
            cycles=[],
            has_cycles=False,
        )
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
        return _graph_degraded("centrality", {"total_files": 0, "hubs": [], "max_in_degree": 0})
    return result


# ══════════════════════════════════════════════════════════════════════════════
#  ARCHITECTURE SMELLS (Fase 18 — "Dependency Engine", último ítem del Graph
#  Engine portado)
# ══════════════════════════════════════════════════════════════════════════════


async def _build_architecture_smells(parsed_files: list[dict]) -> list[dict]:
    """
    Smells de arquitectura: acoplamiento eferente alto, dependencia inestable
    (afferent alto + inestabilidad alta — un módulo muy usado por otros que a
    la vez es frágil, un cambio se propaga en las dos direcciones), y
    dependencia circular reencuadrada como un smell más — la "violación de
    capas general" que pide la Fase 22 del roadmap.

    Fase 18, "Dependency Engine": el cómputo (edges cross-file, in/out-degree,
    enumeración de ciclos, y los 3 checks de smell con sus umbrales) vive
    ahora en el sidecar Rust (`services/complexity/src/graph.rs::build_architecture_smells`,
    expuesto vía `POST /graph/architecture`) — mismo criterio Rust-only que
    el resto del Graph Engine: sin fallback Python, `[]` explícito si el
    sidecar no responde. Cierra el motivo por el que esto era Python-only
    ("el import graph cross-file solo existe en Python") — `build_project_edges`/
    `module_to_candidates`/`find_cycles_capped` ya viven todos en Rust desde
    las porciones anteriores de esta misma fila; esto es pura orquestación
    sobre lo que esas 3 ya construyeron. `_build_project_edges`/
    `_build_project_digraph`/`_module_to_candidates` (Python) y el import de
    `networkx`/`find_cycles_capped` en este archivo se eliminaron — sin otro
    caller, quedaban muertos apenas esto se cortó a Rust.
    """
    smells = await build_architecture_smells_rust(_files_summary_for_sidecar(parsed_files))
    if smells is None:
        add_log("warn", f"graph error (architecture): {_SIDECAR_DOWN_MSG}")
        return []
    return smells


# ══════════════════════════════════════════════════════════════════════════════
#  COMPLEXITY HEATMAP
# ══════════════════════════════════════════════════════════════════════════════


def _heatmap_payload(parsed_files: list[dict]) -> list[dict]:
    """Shape que espera el sidecar Rust para Heatmap (`HeatmapFileInput` en
    `services/complexity/src/graph.rs`) — detalle por función (name/big_o/
    complexity/line/loc), distinto de `_files_summary_for_sidecar`
    (Import/Centrality) y de `_call_graph_payload` (Call Graph, sin `loc`)."""
    return [
        {
            "filename": p["_filename"],
            "functions": [
                {
                    "name": fn["name"],
                    "big_o": fn.get("big_o", "?"),
                    "complexity": fn.get("complexity", 1),
                    "line": fn.get("line", 0),
                    "loc": fn.get("loc", 0),
                }
                for fn in p.get("functions", [])
            ],
        }
        for p in parsed_files
    ]


async def _build_heatmap(parsed_files: list[dict]) -> dict[str, Any]:
    """
    Mapa de calor de complejidad: todos los archivos × todas las funciones.
    Colorea por CC y/o Big-O.

    Última pieza del Graph Engine (Fase 18) que quedaba Python-only — Import/
    Call/Circular/Centrality/Architecture Smells la nombraban explícitamente
    como "todavía Python, fuera de este slice" cada vez. El cómputo (ordenar
    por severidad, colorear, armar el Mermaid) vive ahora en
    `graph.rs::build_heatmap`, expuesto vía `POST /graph/heatmap` — mismo
    criterio Rust-only que sus 4 hermanos: sin fallback Python, degradación
    explícita si el sidecar no responde. `_cc_color`/`_cc_level`/
    `_bigo_color`/`_bigo_level`/`_short_name`/`_safe_id` se eliminaron del
    todo — su único caller Python era este mismo par de funciones.
    """
    result = await build_heatmap_rust(_heatmap_payload(parsed_files))
    if result is None:
        return _graph_degraded(
            "heatmap",
            {"total_functions": 0, "avg_cc": 0, "critical_count": 0, "hot_paths": 0, "by_level": {}},
            functions=[],
        )
    return result


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

        # Fase 18, "Project Scanner" — Rust lee del disco y parsea el
        # proyecto entero en 1 sola llamada (`scanner.rs`), reemplazando el
        # patrón anterior (`read_project_files` + `_parse_all`, N llamadas
        # HTTP). Ninguno de los builders de grafo de acá abajo necesita el
        # contenido crudo del archivo — solo `filename`/`functions`/`imports`/
        # etc., que ya vienen en el resultado parseado.
        parsed_files = await parse_project_files(project_dir)

        if not parsed_files:
            return _empty_response(req.graph_type)

        for r in parsed_files:
            r["_filename"] = r["filename"]

        if req.graph_type == "import":
            result = await _build_import_graph(parsed_files)
        elif req.graph_type == "call":
            result = await _build_call_graph(parsed_files)
        elif req.graph_type == "circular":
            result = await _build_circular_graph(parsed_files)
        elif req.graph_type == "heatmap":
            result = await _build_heatmap(parsed_files)
        elif req.graph_type == "centrality":
            result = await _build_centrality_graph(parsed_files)
        else:
            return {"error": f"Tipo desconocido: {req.graph_type}"}

        # Agregar metadata del proyecto
        result["project_id"] = req.project_id
        result["total_files"] = len(parsed_files)
        result["file_list"] = [f["filename"] for f in parsed_files]

        # Agregar árbol de directorios para Tree View
        result["dir_tree"] = _build_dir_tree(parsed_files)

        add_log("info", f"graph/project: {req.project_id} ({len(parsed_files)} files, type={req.graph_type})")
        return result

    except Exception as e:
        add_log("warn", f"graph/project error: {e}")
        return {"error": str(e), "mermaid": "", "nodes": [], "edges": []}


def _build_dir_tree(parsed: list[dict]) -> dict:
    """
    Construye el árbol de directorios con metadata de complejidad.
    Estructura: { name, type, path, children, stats }

    Antes tomaba `files` (crudo, solo para `filename`) y `parsed` por
    separado — con el Project Scanner (Fase 18) el único caller ya no lee
    archivos crudos aparte, así que un solo parámetro alcanza: `parsed` ya
    trae `filename` para cada archivo.
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

    for f in parsed:
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
