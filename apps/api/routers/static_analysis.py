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

from pathlib import Path
from typing import Any
from fastapi import APIRouter
from pydantic import BaseModel

from services.static_parser import parse_file, HAS_TREESITTER, HAS_NX
from services.project_service import read_project_files
from services.complexity_client import parse_python_rich

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
    """Lenguajes soportados y herramientas disponibles."""
    return {
        "languages": {
            "python": {
                "extensions": [".py"],
                "parser": "Python ast (stdlib)",
                "features": [
                    "functions",
                    "classes",
                    "imports",
                    "big_o",
                    "cyclomatic_complexity",
                    "dead_code",
                    "call_graph",
                    "circular_deps",
                    "wasm_hints",
                ],
                "available": True,
            },
            "c": {
                "extensions": [".c"],
                "parser": "tree-sitter-c",
                "features": ["functions", "structs", "includes", "macros", "big_o", "call_graph", "wasm_hints"],
                "available": HAS_TREESITTER,
            },
            "cpp": {
                "extensions": [".cpp", ".cc", ".cxx", ".hpp", ".h"],
                "parser": "tree-sitter-cpp",
                "features": ["functions", "classes", "includes", "macros", "big_o", "call_graph", "wasm_hints"],
                "available": HAS_TREESITTER,
            },
            "javascript": {
                "extensions": [".js", ".jsx"],
                "parser": "regex + AST-like",
                "features": [
                    "functions",
                    "classes",
                    "imports",
                    "exports",
                    "big_o",
                    "dead_code",
                    "call_graph",
                    "wasm_hints",
                ],
                "available": True,
            },
            "typescript": {
                "extensions": [".ts", ".tsx"],
                "parser": "regex + AST-like",
                "features": [
                    "functions",
                    "classes",
                    "imports",
                    "exports",
                    "interfaces",
                    "types",
                    "big_o",
                    "dead_code",
                    "call_graph",
                    "wasm_hints",
                ],
                "available": True,
            },
        },
        "capabilities": {
            "tree_sitter": HAS_TREESITTER,
            "networkx": HAS_NX,
        },
    }


@router.post("/parse")
async def parse_single(req: ParseRequest) -> dict[str, Any]:
    """
    Analiza un archivo con el parser correspondiente a su extensión.
    Retorna funciones, clases, imports, Big O, call graph y WASM hints.
    """
    return parse_file(req.filename, req.content)


@router.post("/parse-project")
async def parse_project(req: ParseProjectRequest) -> dict[str, Any]:
    """
    Analiza múltiples archivos y arma el resumen global del proyecto.
    """
    if req.project_id:
        project_dir = Path(f"uploads/projects/{req.project_id}")
        files = read_project_files(project_dir) if project_dir.exists() else []
    else:
        files = req.files

    results: list[dict] = [parse_file(f.get("filename", "unknown"), f.get("content", "")) for f in files]

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
        },
        "wasm_candidates": wasm_candidates,
    }


@router.post("/bigO")
async def analyze_big_o(req: BigORequest) -> dict[str, Any]:
    """Extrae solo el análisis Big O de un archivo.

    Fase 1 de la migración a Rust (ver CHANGELOG): para `.py`, primero intenta
    `parse_python_rich()` (sidecar `complexity-engine`, 6-20x más rápido medido
    con Criterion vs. `_parse_python()`) — este endpoint es un subset genuino
    (solo lee `functions`), así que no pierde nada si el sidecar responde. Si
    no está disponible, cae en el `parse_file()` de siempre — sin gate de flag
    cacheado (misma lección de la condición de carrera del sidecar anterior):
    se intenta en vivo cada vez, no se asume disponibilidad de una foto vieja.
    """
    parsed = None
    if Path(req.filename).suffix.lower() == ".py":
        rich = await parse_python_rich(req.filename, req.content)
        if rich is not None:
            parsed = {"language": "python", "functions": rich["functions"]}
    if parsed is None:
        parsed = parse_file(req.filename, req.content)
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
    parsed = parse_file(req.filename, req.content)
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
