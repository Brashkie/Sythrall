"""
Router: Static Analysis
Análisis estático multi-lenguaje SIN IA.
Endpoints:
  POST /static/parse          → AST de un archivo
  POST /static/parse-project  → AST de múltiples archivos + resumen del proyecto
  POST /static/bigO           → solo Big O de un archivo
  POST /static/wasm           → solo WASM hints de un archivo
  GET  /static/languages      → lenguajes soportados
"""

from __future__ import annotations

import asyncio
from typing import Any
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from services.complexity_client import (
    compilar_asm_rust,
    css_atomico_rust,
    css_usage_rust,
    pareto_compilacion_rust,
    salud_scores_rust,
)
from services.plugin_registry import manifests as plugin_manifests
from services.project_service import IGNORED_DIRS, PARSEABLE_EXTENSIONS
from services.static_parser import parse_file, parse_project_files
from shared import LIB_FLAGS, UPLOADS_DIR
from routers.graph import _build_circular_graph, _build_architecture_smells

router = APIRouter()

# La version de las formulas la define ahora `salud.rs` y viaja en la
# respuesta del motor (`health.scoring_version`): tenerla tambien aca
# seria una segunda fuente de verdad que puede desincronizarse.


# ── Schemas ───────────────────────────────────────────────────────────────────


class ParseRequest(BaseModel):
    filename: str
    content: str


class CssUsageRequest(BaseModel):
    project_id: str


class ParseProjectRequest(BaseModel):
    files: list[dict] = []  # [{"filename": "...", "content": "..."}] — ignorado si viene project_id
    project_id: str | None = None  # si viene, se lee del disco en vez de usar `files`


class BigORequest(BaseModel):
    filename: str
    content: str


class WasmRequest(BaseModel):
    filename: str
    content: str


# ── Endpoints ─────────────────────────────────────────────────────────────────


@router.get("/languages")
async def supported_languages():
    """Lenguajes soportados y herramientas disponibles.

    Fase 24 (Extensibility Platform): este shape ya no es un dict hardcodeado
    a mano acá — se arma desde `plugin_registry.manifests()`, que a su vez
    viene del manifest que expone `services/complexity/src/plugin.rs`
    (`GET /plugins/manifests`), o del respaldo local si el sidecar no
    respondió al arranque. `available` sigue siendo información específica
    de este backend (si el sidecar está arriba), no algo que el manifest en
    sí describa — por eso se calcula acá: `True` siempre para "python" (tiene
    su propio esqueleto de fallback sin sidecar), `HAS_COMPLEXITY_ENGINE`
    para el resto (sin fallback propio)."""
    languages = {
        m["id"]: {
            "extensions": m["extensions"],
            "parser": m["parser"],
            "features": m["features"],
            "available": True if m["id"] == "python" else LIB_FLAGS["HAS_COMPLEXITY_ENGINE"],
        }
        for m in plugin_manifests()
    }
    return {
        "languages": languages,
        "capabilities": {
            "complexity_engine": LIB_FLAGS["HAS_COMPLEXITY_ENGINE"],
        },
    }


@router.post("/parse")
async def parse_single(req: ParseRequest) -> dict[str, Any]:
    """
    Analiza un archivo con el parser correspondiente a su extensión.
    Retorna funciones, clases, imports, Big O, call graph y WASM hints.
    """
    return await parse_file(req.filename, req.content)


class CompilarAsmRequest(BaseModel):
    language: str
    content: str
    # Dos niveles por default: un numero suelto no dice nada, la comparacion si.
    opt_levels: list[str] = ["0", "2"]
    # Extensiones de CPU (`avx2`, `avx512f`, ...). Sin esto no se puede
    # preguntar "ise vectorizo?": gcc -O3 a secas no emite una sola
    # instruccion SIMD donde con -mavx2 da AVX2 de 256 bits.
    features: list[str] = []


@router.post("/compile-asm")
async def compile_asm(req: CompilarAsmRequest):
    """Compila el fuente a ensamblador y lo analiza (Fase 19 + Fase 26).

    **Va bajo `/static` a proposito y no bajo un prefijo nuevo**: `/static` ya
    esta en el `BACKEND_PROXY` de `vite.config.ts`, asi que no hace falta
    tocarlo. Un prefijo nuevo sin su entrada da 404 silencioso en el navegador
    aunque curl contra el backend ande — ya paso antes.
    """
    result = await compilar_asm_rust(req.language, req.content, req.opt_levels, req.features)
    if result is None:
        return {"available": False, "results": [], "error": "El motor de analisis no esta disponible."}
    return {"available": True, "results": result, "error": None}


class ParetoRequest(BaseModel):
    language: str
    content: str
    # Grilla vacia = la de `compilar.rs`. Cada punto es una compilacion real,
    # asi que el costo lo decide quien pide, no un default generoso.
    configs: list[dict] = []


@router.post("/css-atomic")
async def css_atomic(req: ParseRequest):
    """Atomic CSS (Fase 31): la hoja atomica y la medicion del canje.

    Bajo `/static` como el resto — ya esta en el proxy de Vite.
    """
    result = await css_atomico_rust(req.filename, req.content)
    if result is None:
        return {"available": False, "result": None, "error": "El motor de analisis no esta disponible."}
    return {"available": True, "result": result, "error": None}


@router.post("/compile-pareto")
async def compile_pareto(req: ParetoRequest):
    """Frente de Pareto sobre la configuracion de compilacion (Fase 32).

    Bajo `/static` por el mismo motivo que `/compile-asm`: ya esta proxeado.
    """
    result = await pareto_compilacion_rust(req.language, req.content, req.configs)
    if result is None:
        return {"available": False, "result": None, "error": "El motor de analisis no esta disponible."}
    return {"available": True, "result": result, "error": None}


@router.post("/css-usage")
async def css_usage(req: CssUsageRequest) -> dict[str, Any]:
    """Sythrall CSS Engine — cruza las clases definidas en los `.css` del
    proyecto contra todo lo escrito en su marcado/scripts.

    **A diferencia del resto de este router, esto NO prueba nada** y el
    propio reporte lo dice: un analisis estatico no puede demostrar que una
    regla CSS no se aplica, porque los nombres se construyen en runtime. Ver
    `cssusage.rs` para el razonamiento completo y para el dato medido que
    justifica el hedge.

    Las listas de extensiones/ignorados salen de `project_service.py` mas
    `.css`/`.html`, para que este analisis mire el mismo conjunto de archivos
    que el resto — con el agregado del marcado, que ningun otro analisis
    necesita."""
    project_dir = UPLOADS_DIR / req.project_id
    if not project_dir.exists():
        raise HTTPException(status_code=404, detail=f"Proyecto '{req.project_id}' no encontrado")

    exts = sorted(PARSEABLE_EXTENSIONS | {".css", ".html", ".htm"})
    result = await css_usage_rust(str(project_dir), exts, sorted(IGNORED_DIRS))
    if result is None:
        return {"available": False, "reason": "sidecar Rust no disponible"}
    return {"available": True, **result}


@router.post("/parse-project")
async def parse_project(req: ParseProjectRequest) -> dict[str, Any]:
    """
    Analiza múltiples archivos y arma el resumen global del proyecto.
    """
    return await compute_project_analysis(req.project_id, req.files)


async def compute_project_analysis(project_id: str | None, files: list[dict]) -> dict[str, Any]:
    """Extraído del propio `parse_project` de arriba (Fase 28, 2026-09-02) —
    `routers/history.py` necesita exactamente el mismo `health`/`summary`
    para guardar un snapshot que el endpoint en vivo ya calcula, así que
    esto es la única fuente de verdad para ambos en vez de una segunda
    fórmula de scoring a mantener sincronizada. Sin cambio de comportamiento
    para `parse_project` — mismo cuerpo, ahora reusable."""
    if project_id:
        # Fase 18, "Project Scanner" — Rust lee del disco y parsea el
        # proyecto entero en 1 sola llamada (`parse_project_files`,
        # `scanner.rs`), en vez del patrón anterior de N llamadas HTTP (una
        # por archivo). Cada resultado ya trae `loc` (Rust lo calcula al
        # leer el archivo — ver más abajo, `language_distribution`).
        project_dir = UPLOADS_DIR / project_id
        results: list[dict] = await parse_project_files(project_dir) if project_dir.exists() else []
    else:
        # `asyncio.gather` en vez de awaits secuenciales — `parse_file` ahora
        # consulta el sidecar Rust para `.py`, y con proyectos grandes (el
        # benchmark de 4003 archivos de la Fase 10) N round-trips HTTP en
        # serie sí se notarían.
        results = list(
            await asyncio.gather(*(parse_file(f.get("filename", "unknown"), f.get("content", "")) for f in files))
        )
        for r, f in zip(results, files, strict=False):
            r["loc"] = f.get("content", "").count("\n")  # mismo criterio que `wc -l`

    for r in results:
        r["_filename"] = r["filename"]

    # ── Resumen global del proyecto ───────────────────────────────────────────
    total_funcs = sum(len(r.get("functions", [])) for r in results)
    total_classes = sum(len(r.get("classes", [])) for r in results)
    total_imports = sum(len(r.get("imports", [])) for r in results)
    total_dead = sum(len(r.get("dead_code", [])) for r in results)
    all_big_o: dict[str, int] = {}
    for r in results:
        for fn in r.get("functions", []):
            bigo = fn.get("big_o", "?")
            all_big_o[bigo] = all_big_o.get(bigo, 0) + 1

    wasm_candidates = [
        {"file": r["filename"], "hints": r.get("wasm_hints", [])} for r in results if r.get("wasm_hints")
    ]

    # Top funciones por complejidad ciclomática, aplanado entre archivos — para
    # el widget "Complexity by Function" del Dashboard. Agregación pura sobre
    # `functions` que cada parser ya calcula, no análisis nuevo.
    all_functions = [
        {
            "file": r["filename"],
            "name": fn["name"],
            "line": fn["line"],
            "complexity": fn.get("complexity", 1),
            "big_o": fn.get("big_o", "?"),
        }
        for r in results
        for fn in r.get("functions", [])
    ]
    top_complex_functions = sorted(all_functions, key=lambda f: f["complexity"], reverse=True)[:10]

    # Distribución real de líneas/archivos/funciones por lenguaje — no una
    # estimación: `loc` viaja en cada resultado (Rust lo calcula al leer el
    # archivo, o se calcula acá mismo para el camino sin project_id — ver
    # arriba), así que no hace falta el contenido crudo de nuevo. Para el
    # widget "Languages" del Dashboard.
    language_distribution: dict[str, dict[str, int]] = {}
    for r in results:
        lang = r.get("language", "?")
        entry = language_distribution.setdefault(lang, {"files": 0, "loc": 0, "functions": 0})
        entry["files"] += 1
        entry["loc"] += r.get("loc", 0)
        entry["functions"] += len(r.get("functions", []))

    # ── Project Health (Fase 2 del rediseño UX) ───────────────────────────────
    # Agregación pura sobre resultados ya calculados (Rust-first desde las Fases
    # 21/22) — mismo tipo de glue code que big_o_distribution/wasm_candidates
    # arriba, no lógica de análisis nueva.
    security_findings = [{**f, "file": r["filename"]} for r in results for f in r.get("security_findings", [])]
    structural_smells = [{**s, "file": r["filename"]} for r in results for s in r.get("structural_smells", [])]
    naming_smells = [{**s, "file": r["filename"]} for r in results for s in r.get("naming_smells", [])]

    # Fase 25 — Modernization Report, rollup por archivo (2026-09-01, pedido
    # explícito del usuario: un rollup honesto, NO el grafo de 5 categorías
    # Keep/Refactor/Replace/Rewrite/Migrate que el ROADMAP nombra como
    # visión — ese veredicto necesitaría reglas de mapeo patrón→categoría
    # que hoy serían inventadas, no sostenidas por evidencia, violando el
    # principio de "evidencia, nunca un veredicto" que el resto del motor ya
    # respeta (Fase 21 en particular). Esto solo agrupa los candidatos que
    # `modernization.rs` (Fase 25) ya calcula por archivo, sin agregar
    # ningún juicio nuevo — mismo criterio que `security_findings` de arriba.
    modernization_candidates = [
        {**c, "file": r["filename"]} for r in results for c in r.get("modernization", {}).get("candidates", [])
    ]
    modernization_by_file_map: dict[str, dict[str, Any]] = {}
    for c in modernization_candidates:
        entry = modernization_by_file_map.setdefault(
            c["file"], {"file": c["file"], "total": 0, "high": 0, "medium": 0, "patterns": {}}
        )
        entry["total"] += 1
        entry[c["confidence"]] = entry.get(c["confidence"], 0) + 1
        entry["patterns"][c["pattern"]] = entry["patterns"].get(c["pattern"], 0) + 1
    modernization_by_file = sorted(modernization_by_file_map.values(), key=lambda e: e["total"], reverse=True)

    # Fase 31 (2026-09-02) — los 4 tipos de hallazgo que nacieron despues de
    # que este agregador se escribio (CSS, HTML, manifests, precondiciones de
    # control) quedaban invisibles a nivel de proyecto: se calculaban por
    # archivo y ahi morian. Mismo criterio que `security_findings`: se
    # aplanan con su `file` de origen.
    #
    # **Deliberadamente NO entran en el score de Project Health.** Los 4
    # dimensiones actuales (security/quality/complexity/architecture) tienen
    # formulas calibradas sobre conteos de funciones/clases; meterle
    # hallazgos de accesibilidad o de dependencias cambiaria en silencio el
    # significado de un numero que la gente compara contra corridas
    # anteriores — y en un proyecto de puro frontend, donde el denominador de
    # `quality_penalty` es minimo, lo distorsionaria por completo. Se
    # muestran como conteos propios; que entren al score es una decision de
    # producto aparte, no un efecto colateral de esta pasada.
    css_findings = [{**f, "file": r["filename"]} for r in results for f in r.get("css_findings", [])]
    html_findings = [{**f, "file": r["filename"]} for r in results for f in r.get("html_findings", [])]
    manifest_findings = [{**f, "file": r["filename"]} for r in results for f in r.get("manifest_findings", [])]
    control_findings = [{**f, "file": r["filename"]} for r in results for f in r.get("control_findings", [])]
    locality_findings = [{**f, "file": r["filename"]} for r in results for f in r.get("locality_findings", [])]
    all_complexities = [fn.get("complexity", 1) for r in results for fn in r.get("functions", [])]
    avg_complexity = round(sum(all_complexities) / len(all_complexities), 2) if all_complexities else 0.0

    circular = await _build_circular_graph(results)
    total_cycles = circular["summary"]["total_cycles"]

    # Fase 22 — Architecture smells: acoplamiento eferente alto, dependencia
    # inestable, y las mismas dependencias circulares de arriba reencuadradas
    # como un smell más. Fase 18 ("Dependency Engine") cortó esto a Rust —
    # recalcula los ciclos de nuevo del lado del sidecar (capado a 20, no es
    # costoso) en vez de reusar `circular["cycles"]` como antes: dos llamadas
    # HTTP baratas en vez de threadear un parámetro opcional por el puerto.
    architecture_smells = await _build_architecture_smells(results)
    # Excluye las entradas de ciclo — ya penalizadas por `total_cycles * 15`
    # abajo; contarlas de nuevo acá sería un doble castigo por el mismo hallazgo.
    coupling_smells_count = sum(1 for s in architecture_smells if s["kind"] != "circular_dependency")

    sec_high = sum(1 for f in security_findings if f["severity"] == "High")
    sec_medium = sum(1 for f in security_findings if f["severity"] == "Medium")
    sec_low = sum(1 for f in security_findings if f["severity"] == "Low")

    # ── Project Health: las cuatro formulas se fueron a Rust ─────────────
    #
    # **Eran las ultimas formulas de AUTORIDAD que vivian en esta capa.** El
    # resto del motor ya decidia en Rust y Python relevaba, pero los cuatro
    # numeros que el producto muestra como su resultado principal se
    # calculaban aca: 245 lineas con 42 operaciones aritmeticas. Un score es
    # exactamente una afirmacion de autoridad —es el numero contra el que
    # alguien compara dos versiones de su codigo—, asi que tenerlo en la capa
    # de relevo contradecia la regla del proyecto.
    #
    # Aca ya no se calcula nada: se juntan los conteos (que esta capa ya tiene
    # porque agrega los resultados por archivo) y se los manda. Verificado con
    # 4000 entradas aleatorias contra la implementacion que reemplaza:
    # **0 diferencias**.
    smells_count = len(structural_smells)
    naming_count = len(naming_smells)
    entrada_salud = {
        "security_high": sec_high,
        "security_medium": sec_medium,
        "security_low": sec_low,
        "structural_smells": smells_count,
        "naming_smells": naming_count,
        "total_functions": total_funcs,
        "total_classes": total_classes,
        "avg_complexity": avg_complexity,
        "total_cycles": total_cycles,
        # Sin las entradas de ciclo: ya pesan en `total_cycles`, y contarlas
        # dos veces seria castigar el mismo hallazgo dos veces.
        "coupling_smells": coupling_smells_count,
        "total_files": len(results),
    }
    health = await salud_scores_rust(entrada_salud)
    if health is None:
        # **Degradacion con gracia, igual que el resto del motor**: sin
        # sidecar no hay scores. Devolver numeros calculados aca seria
        # reintroducir la segunda formula que esta migracion vino a eliminar,
        # y una que podria desincronizarse en silencio.
        health = {
            "security": {"score": None, "high": sec_high, "medium": sec_medium, "low": sec_low},
            "quality": {"score": None, "smells": smells_count, "naming": naming_count},
            "complexity": {"score": None, "avg_complexity": avg_complexity},
            "architecture": {"score": None, "cycles": total_cycles, "smells": coupling_smells_count},
            "scoring_version": None,
            "error": "El motor de analisis no esta disponible: no hay scores.",
        }

    return {
        "files": results,
        "summary": {
            "total_files": len(results),
            "total_functions": total_funcs,
            "total_classes": total_classes,
            "total_imports": total_imports,
            "unused_imports": total_dead,
            "big_o_distribution": all_big_o,
            "wasm_candidates": len(wasm_candidates),
            "security_findings": len(security_findings),
            "structural_smells": smells_count,
            "naming_smells": naming_count,
            "modernization_candidates": len(modernization_candidates),
            # Incluye las entradas de ciclo — a diferencia de
            # health.architecture.smells, que las excluye para no penalizar
            # el score dos veces. Este total es "cuántos hallazgos de
            # arquitectura hay para mostrar en la lista", no un input de
            # scoring, así que no tiene la misma exclusión.
            "architecture_smells": len(architecture_smells),
            "css_findings": len(css_findings),
            "html_findings": len(html_findings),
            "manifest_findings": len(manifest_findings),
            "control_findings": len(control_findings),
            "locality_findings": len(locality_findings),
            "total_loc": sum(v["loc"] for v in language_distribution.values()),
        },
        "wasm_candidates": wasm_candidates,
        "security_findings": security_findings,
        "structural_smells": structural_smells,
        "naming_smells": naming_smells,
        "modernization_candidates": modernization_candidates,
        "modernization_by_file": modernization_by_file,
        "architecture_smells": architecture_smells,
        "css_findings": css_findings,
        "html_findings": html_findings,
        "manifest_findings": manifest_findings,
        "control_findings": control_findings,
        "locality_findings": locality_findings,
        "top_complex_functions": top_complex_functions,
        "language_distribution": language_distribution,
        "health": health,
    }


@router.post("/bigO")
async def analyze_big_o(req: BigORequest) -> dict[str, Any]:
    """Extrae solo el análisis Big O de un archivo.

    `parse_file()` ya consulta el sidecar Rust primero para `.py`
    (`_parse_python`, ver `static_parser.py`) — este endpoint ya no necesita
    su propio gate manual, era una duplicación de ese mismo chequeo.
    """
    parsed = await parse_file(req.filename, req.content)
    functions = parsed.get("functions", [])

    big_o_table = [
        {
            "function": f["name"],
            "line": f["line"],
            "big_o": f.get("big_o", "?"),
            "reason": f.get("big_o_reason", ""),
            "complexity": f.get("complexity", 1),
            "loc": f.get("loc", 0),
        }
        for f in functions
    ]

    # Distribución de Big O
    distribution: dict[str, int] = {}
    for entry in big_o_table:
        bigo = entry["big_o"]
        distribution[bigo] = distribution.get(bigo, 0) + 1

    # Hot paths (O(n²) o peor)
    hot_paths = [e for e in big_o_table if e["big_o"] in ("O(n²)", "O(n³)", "O(2^n)", r"O(n^4)")]

    return {
        "filename": req.filename,
        "language": parsed.get("language", "?"),
        "functions": big_o_table,
        "distribution": distribution,
        "hot_paths": hot_paths,
        "total": len(big_o_table),
    }


@router.post("/wasm")
async def analyze_wasm(req: WasmRequest) -> dict[str, Any]:
    """Extrae solo las recomendaciones WASM/Cython de un archivo."""
    parsed = await parse_file(req.filename, req.content)
    hints = parsed.get("wasm_hints", [])

    return {
        "filename": req.filename,
        "language": parsed.get("language", "?"),
        "hints": hints,
        "total": len(hints),
        "critical": [h for h in hints if h.get("priority", 0) >= 4],
        "summary": (
            f"{len(hints)} función(es) candidata(s) a WASM/Cython. "
            + (
                f"{len([h for h in hints if h.get('priority',0)>=4])} crítica(s)."
                if hints
                else "Sin candidatos detectados."
            )
        ),
    }
