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
        "filenames": [],
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
        "permissions": [
            {"permission": "read_project", "level": "allowed"},
            {"permission": "analyze_files", "level": "allowed"},
        ],
    },
    {
        "id": "c",
        "category": "language",
        "needs": ["source", "ast"],
        "filenames": [],
        "extensions": [".c"],
        "parser": "tree-sitter-c (Rust sidecar)",
        "features": ["functions", "structs", "includes", "macros", "big_o", "call_graph", "wasm_hints"],
        "builtin": True,
        "permissions": [
            {"permission": "read_project", "level": "allowed"},
            {"permission": "analyze_files", "level": "allowed"},
        ],
    },
    {
        "id": "cpp",
        "category": "language",
        "needs": ["source", "ast"],
        "filenames": [],
        "extensions": [".cpp", ".cc", ".cxx", ".hpp", ".h"],
        "parser": "tree-sitter-cpp (Rust sidecar)",
        "features": ["functions", "classes", "includes", "macros", "big_o", "call_graph", "wasm_hints"],
        "builtin": True,
        "permissions": [
            {"permission": "read_project", "level": "allowed"},
            {"permission": "analyze_files", "level": "allowed"},
        ],
    },
    {
        "id": "javascript",
        "category": "language",
        "needs": ["source"],
        "filenames": [],
        "extensions": [".js", ".jsx"],
        "parser": "regex + AST-like (Rust sidecar)",
        "features": ["functions", "classes", "imports", "exports", "big_o", "dead_code", "call_graph", "wasm_hints"],
        "builtin": True,
        "permissions": [
            {"permission": "read_project", "level": "allowed"},
            {"permission": "analyze_files", "level": "allowed"},
        ],
    },
    {
        "id": "typescript",
        "category": "language",
        "needs": ["source"],
        "filenames": [],
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
        "permissions": [
            {"permission": "read_project", "level": "allowed"},
            {"permission": "analyze_files", "level": "allowed"},
        ],
    },
    {
        "id": "d",
        "category": "language",
        "needs": ["source", "ast", "call_graph"],
        "filenames": [],
        "extensions": [".d"],
        "parser": "tree-sitter-d (Rust sidecar)",
        "features": [
            "functions",
            "trust_surface",
            "review_surface",
            "attribute_scope_resolution",
            "unittests",
            "imports",
            "loop_depth",
            "cyclomatic_complexity",
            "big_o",
            "call_graph",
            "structural_smells",
        ],
        "builtin": True,
        "permissions": [
            {"permission": "read_project", "level": "allowed"},
            {"permission": "analyze_files", "level": "allowed"},
        ],
    },
    {
        "id": "ada",
        "category": "language",
        "needs": ["source", "ast", "call_graph"],
        "filenames": [],
        "extensions": [".adb", ".ads"],
        "parser": "tree-sitter-ada (Rust sidecar)",
        "features": [
            "functions",
            "procedures",
            "expression_functions",
            "specs",
            "packages",
            "generics",
            "tasks",
            "with_clauses",
            "loop_depth",
            "cyclomatic_complexity",
            "big_o",
            "call_graph",
            "structural_smells",
        ],
        "builtin": True,
        "permissions": [
            {"permission": "read_project", "level": "allowed"},
            {"permission": "analyze_files", "level": "allowed"},
        ],
    },
    {
        "id": "fortran",
        "category": "language",
        "needs": ["source", "ast", "call_graph"],
        "filenames": [],
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
        "permissions": [
            {"permission": "read_project", "level": "allowed"},
            {"permission": "analyze_files", "level": "allowed"},
        ],
    },
    {
        "id": "assembly",
        "category": "language",
        "needs": ["source"],
        "filenames": [],
        "extensions": [".s", ".asm"],
        "parser": "pattern-matching AT&T/Intel (Rust sidecar) — no es un disassembler real, ver Fase 19",
        "features": ["procedures", "instructions", "registers_used", "big_o", "call_graph"],
        "builtin": True,
        "permissions": [
            {"permission": "read_project", "level": "allowed"},
            {"permission": "analyze_files", "level": "allowed"},
        ],
    },
    {
        "id": "rust",
        "category": "language",
        "needs": ["source", "ast"],
        "filenames": [],
        "extensions": [".rs"],
        "parser": "tree-sitter-rust (Rust sidecar)",
        "features": ["functions", "uses", "big_o", "trust_surface", "proof_surface", "structural_smells"],
        "builtin": True,
        "permissions": [
            {"permission": "read_project", "level": "allowed"},
            {"permission": "analyze_files", "level": "allowed"},
        ],
    },
    {
        "id": "notebook",
        "category": "language",
        "needs": ["source", "ast"],
        "filenames": [],
        "extensions": [".ipynb"],
        "parser": "extracción de celdas de código (JSON, Rust sidecar) → delega en el mismo motor de análisis de Python (`rich::analyze_rich`) — ver Fase 27",
        "features": [
            "functions",
            "classes",
            "imports",
            "big_o",
            "cyclomatic_complexity",
            "security_findings",
            "code_cells",
            "cell_ranges",
        ],
        "builtin": True,
        "permissions": [
            {"permission": "read_project", "level": "allowed"},
            {"permission": "analyze_files", "level": "allowed"},
        ],
    },
    {
        "id": "css",
        "category": "language",
        # Solo "source": cssparse.rs trae su propio tokenizador, no depende
        # del AST de ningun otro motor (los otros 8 declaran "ast").
        "needs": ["source"],
        "filenames": [],
        "extensions": [".css"],
        "parser": "tokenizador propio escrito a mano (Rust sidecar, sin dependencia nueva) — Sythrall CSS Engine",
        "features": [
            "rules",
            "selectors",
            "specificity",
            "duplicate_declaration",
            "duplicate_selector",
            "high_specificity",
            "css_size",
        ],
        "builtin": True,
        "permissions": [
            {"permission": "read_project", "level": "allowed"},
            {"permission": "analyze_files", "level": "allowed"},
        ],
    },
    {
        "id": "html",
        "category": "language",
        "needs": ["source"],
        "filenames": [],
        "extensions": [".html", ".htm"],
        "parser": "tokenizador propio escrito a mano (Rust sidecar, sin dependencia nueva) — accesibilidad y estructura, ver Fase 31",
        "features": ["accessibility", "document_structure", "duplicate_id", "heading_order", "form_labels"],
        "builtin": True,
        "permissions": [
            {"permission": "read_project", "level": "allowed"},
            {"permission": "analyze_files", "level": "allowed"},
        ],
    },
    {
        "id": "manifest",
        # Unica entrada que no es "language": primera Format Extension.
        "category": "format",
        "needs": ["source"],
        # Sin extensiones: reclama por NOMBRE exacto.
        "extensions": [],
        "filenames": ["package.json", "requirements.txt", "cargo.toml", "pyproject.toml"],
        "parser": "parser propio de manifests de dependencias (Rust sidecar) — primera Format Extension, ver Fase 31",
        "features": [
            "dependencies",
            "version_pinning",
            "duplicate_dependency",
            "external_source_dependency",
        ],
        "builtin": True,
        "permissions": [
            {"permission": "read_project", "level": "allowed"},
            {"permission": "analyze_files", "level": "allowed"},
        ],
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


def analysis_capabilities() -> list[str]:
    """`["c:big_o", "c:modernization", "python:security_findings", ...]` —
    qué sabe producir el motor, derivado de los manifests que el sidecar ya
    publica. Ordenado, para que dos listas se comparen sin ruido.

    Se guarda con cada snapshot del historial. El motivo es concreto: cuando
    un lenguaje empieza a emitir un tipo de hallazgo que antes devolvía `[]`
    —cosa que pasó cuatro veces en este repo— el score de ese proyecto baja
    sin que el código se haya tocado, y comparar contra un snapshot viejo
    muestra una "regresión" que nunca existió. Con esto, `compare` puede
    decir qué cambió en el analizador entre las dos mediciones.

    Se toman TODAS las features, no solo las de tipo hallazgo: una capacidad
    de parseo nueva también mueve los conteos de `summary`. Y mantener acá
    una segunda lista de "cuáles cuentan" sería justo el tipo de tabla
    paralela que se desactualiza en silencio."""
    return sorted(f"{m['id']}:{feat}" for m in manifests() for feat in m.get("features", []))


def extension_map() -> dict[str, str]:
    """`{".py": "python", ...}` — derivado de `manifests()` (sidecar o
    respaldo, según cuál esté activo)."""
    return {ext: m["id"] for m in manifests() for ext in m["extensions"]}


def filename_map() -> dict[str, str]:
    """`{"package.json": "manifest", ...}` — plugins que reclaman un NOMBRE de
    archivo exacto en vez de una extensión (Format Extensions, Fase 24). Se
    consulta ANTES que `extension_map`: `package.json` es un `.json` como
    cualquier otro, así que resolver por extensión mandaría todo JSON del
    proyecto al analizador de manifests.

    `.get("filenames", [])` y no `m["filenames"]`: si el sidecar corriera una
    versión previa a este campo, el registry no debe reventar — degrada a
    "ningún plugin reclama nombres", que es exactamente el comportamiento
    anterior."""
    return {name: m["id"] for m in manifests() for name in m.get("filenames", [])}
