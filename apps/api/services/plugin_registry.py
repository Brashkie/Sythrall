"""
services/plugin_registry.py
Fase 24 (Extensibility Platform) — cachea al arranque los manifests que
`services/complexity/src/plugin.rs` expone (`GET /plugins/manifests`) y
deriva de ahí lo que antes vivían como dos cosas hardcodeadas a mano y
mantenidas en paralelo: el mapeo extensión→lenguaje que usa
`static_parser.py::parse_file` para despachar, y el dict de
`routers/static_analysis.py::/languages`.

Mismo criterio de "una sola vez al arranque, se degrada con gracia" que
`HAS_COMPLEXITY_ENGINE` (`main.py`, `check_complexity_engine_sync()`) — si el
sidecar no responde en ese instante exacto, `_MANIFESTS` queda vacío por el
resto de esta corrida del proceso y `manifests()`/`extension_map()` caen al
respaldo de abajo, no reintentan más tarde.
"""

from __future__ import annotations

from services.complexity_client import get_plugin_manifests_sync

_MANIFESTS: list[dict] = []

# Respaldo si el sidecar no respondió al arranque — mismo contenido que
# `plugin.rs::builtin_manifests()`, no una lista independiente a mantener
# sincronizada "cuando alguien se acuerde": deja de importar en cuanto el
# sidecar responde una sola vez, es un piso mínimo, no el camino feliz.
_FALLBACK_MANIFESTS: list[dict] = [
    {
        "id": "python",
        "category": "language",
        "needs": ["source", "ast"],
        "extensions": [".py"],
        "parser": "rustpython-parser (Rust sidecar) — Python ast (stdlib) como fallback sin sidecar",
        "features": [
            "functions",
            "classes",
            "imports",
            "big_o",
            "cyclomatic_complexity",
            "dead_code",
            "call_graph",
            "wasm_hints",
        ],
        "builtin": True,
    },
    {
        "id": "c",
        "category": "language",
        "needs": ["source", "ast"],
        "extensions": [".c"],
        "parser": "tree-sitter-c (Rust sidecar)",
        "features": ["functions", "structs", "includes", "macros", "big_o", "call_graph", "wasm_hints"],
        "builtin": True,
    },
    {
        "id": "cpp",
        "category": "language",
        "needs": ["source", "ast"],
        "extensions": [".cpp", ".cc", ".cxx", ".hpp", ".h"],
        "parser": "tree-sitter-cpp (Rust sidecar)",
        "features": ["functions", "classes", "includes", "macros", "big_o", "call_graph", "wasm_hints"],
        "builtin": True,
    },
    {
        "id": "javascript",
        "category": "language",
        "needs": ["source"],
        "extensions": [".js", ".jsx"],
        "parser": "regex + AST-like (Rust sidecar)",
        "features": ["functions", "classes", "imports", "exports", "big_o", "dead_code", "call_graph", "wasm_hints"],
        "builtin": True,
    },
    {
        "id": "typescript",
        "category": "language",
        "needs": ["source"],
        "extensions": [".ts", ".tsx"],
        "parser": "regex + AST-like (Rust sidecar)",
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
        "builtin": True,
    },
    {
        "id": "fortran",
        "category": "language",
        "needs": ["source", "ast", "call_graph"],
        "extensions": [".f", ".f90", ".f95", ".f03", ".f08", ".for"],
        "parser": "tree-sitter-fortran (Rust sidecar)",
        "features": [
            "functions",
            "subroutines",
            "do_loop_depth",
            "vectorization_candidates",
            "numerical_algorithm_shape",
            "blas_lapack_usage",
            "big_o",
            "call_graph",
        ],
        "builtin": True,
    },
    {
        "id": "assembly",
        "category": "language",
        "needs": ["source"],
        "extensions": [".s", ".asm"],
        "parser": "pattern-matching AT&T/Intel (Rust sidecar) — no es un disassembler real, ver Fase 19",
        "features": ["procedures", "instructions", "registers_used", "big_o", "call_graph"],
        "builtin": True,
    },
]


def load_plugin_manifests_sync() -> None:
    """Llamado una vez desde `main.py` en el mismo bloque de arranque donde
    ya se calcula `HAS_COMPLEXITY_ENGINE`. No hace nada si el sidecar no
    respondió — `_MANIFESTS` se queda vacío y `manifests()` cae al respaldo."""
    global _MANIFESTS
    result = get_plugin_manifests_sync()
    if result:
        _MANIFESTS = result


def manifests() -> list[dict]:
    return _MANIFESTS or _FALLBACK_MANIFESTS


def manifest_by_id(lang_id: str) -> dict | None:
    return next((m for m in manifests() if m["id"] == lang_id), None)


def extension_map() -> dict[str, str]:
    """`{".py": "python", ...}` — derivado de `manifests()` (sidecar o
    respaldo, según cuál esté activo)."""
    return {ext: m["id"] for m in manifests() for ext in m["extensions"]}
