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
from fastapi import APIRouter
from pydantic import BaseModel

from services.plugin_registry import manifests as plugin_manifests
from services.static_parser import parse_file, parse_project_files
from shared import LIB_FLAGS, UPLOADS_DIR
from routers.graph import _build_circular_graph, _build_architecture_smells

router = APIRouter()


# ── Schemas ───────────────────────────────────────────────────────────────────


class ParseRequest(BaseModel):
    filename: str
    content: str


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


@router.post("/parse-project")
async def parse_project(req: ParseProjectRequest) -> dict[str, Any]:
    """
    Analiza múltiples archivos y arma el resumen global del proyecto.
    """
    if req.project_id:
        # Fase 18, "Project Scanner" — Rust lee del disco y parsea el
        # proyecto entero en 1 sola llamada (`parse_project_files`,
        # `scanner.rs`), en vez del patrón anterior de N llamadas HTTP (una
        # por archivo). Cada resultado ya trae `loc` (Rust lo calcula al
        # leer el archivo — ver más abajo, `language_distribution`).
        project_dir = UPLOADS_DIR / req.project_id
        results: list[dict] = await parse_project_files(project_dir) if project_dir.exists() else []
    else:
        # `asyncio.gather` en vez de awaits secuenciales — `parse_file` ahora
        # consulta el sidecar Rust para `.py`, y con proyectos grandes (el
        # benchmark de 4003 archivos de la Fase 10) N round-trips HTTP en
        # serie sí se notarían.
        results = list(
            await asyncio.gather(*(parse_file(f.get("filename", "unknown"), f.get("content", "")) for f in req.files))
        )
        for r, f in zip(results, req.files, strict=False):
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
    security_score = max(0, 100 - (sec_high * 15 + sec_medium * 5 + sec_low * 1))

    # Normalizado por tamaño del proyecto — un proyecto grande con muchos
    # archivos chicos no debe castigarse igual que uno chico con la misma
    # cantidad absoluta de smells. `smells` (estructurales) y `naming` pesan
    # distinto en el score — naming es más ruidoso/menos grave por
    # ocurrencia (ver umbral 300 vs 100 abajo), así que se cuentan aparte en
    # vez de sumarlos 1:1 en el mismo balde.
    smells_count = len(structural_smells)
    naming_count = len(naming_smells)
    quality_denom = max(1, total_funcs + total_classes)
    quality_penalty = smells_count / quality_denom * 300 + naming_count / quality_denom * 100
    quality_score = max(0, 100 - min(100, round(quality_penalty)))

    # 5 = umbral "bueno" de CC, mismas bandas que radon usaba antes de la Fase 11.
    complexity_score = max(0, min(100, round(100 - max(0.0, avg_complexity - 5) * 8)))

    # Igual que quality_penalty normaliza por tamaño del proyecto (comentario
    # arriba), el término de coupling se normaliza por cantidad de archivos —
    # sin esto, un proyecto de 200 archivos con una docena de hubs legítimos
    # se castigaría desproporcionadamente vs. uno chico con la misma cuenta
    # absoluta. Cycles NO se normaliza (ya funcionaba así antes de esta fase).
    architecture_denom = max(1, len(results))
    architecture_penalty = min(100, total_cycles * 15 + round(coupling_smells_count / architecture_denom * 100))
    architecture_score = max(0, 100 - architecture_penalty)

    health = {
        "security": {"score": security_score, "high": sec_high, "medium": sec_medium, "low": sec_low},
        "quality": {"score": quality_score, "smells": smells_count, "naming": naming_count},
        "complexity": {"score": complexity_score, "avg_complexity": avg_complexity},
        "architecture": {"score": architecture_score, "cycles": total_cycles, "smells": coupling_smells_count},
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
            # Incluye las entradas de ciclo — a diferencia de
            # health.architecture.smells, que las excluye para no penalizar
            # el score dos veces. Este total es "cuántos hallazgos de
            # arquitectura hay para mostrar en la lista", no un input de
            # scoring, así que no tiene la misma exclusión.
            "architecture_smells": len(architecture_smells),
            "total_loc": sum(v["loc"] for v in language_distribution.values()),
        },
        "wasm_candidates": wasm_candidates,
        "security_findings": security_findings,
        "structural_smells": structural_smells,
        "naming_smells": naming_smells,
        "architecture_smells": architecture_smells,
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
