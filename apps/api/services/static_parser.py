"""
services/static_parser.py
Parser multi-lenguaje SIN IA.
- Python  → Rust (sidecar), esqueleto AST liviano en Python si no responde
- C/C++   → Rust (sidecar, tree-sitter vía `services/complexity/src/cparse.rs`)
- JS/TS   → Rust (sidecar, regex + AST-like vía `services/complexity/src/jsts.rs`)
- Fortran → Rust (sidecar, tree-sitter vía `services/complexity/src/fparse.rs`) —
  Fase 20 (Scientific Intelligence): DO-loops/vectorización, algoritmos
  numéricos, uso de BLAS/LAPACK
- Big O   → heurística por patrones AST/regex, calculada en Rust para los 5 lenguajes
- WASM    → detección de hot paths y recomendaciones, calculada en Rust (no aplica a
  Fortran — ya compila a nativo, WASM no es un target relevante ahí)

Ninguno de los parsers no-Python tiene ya una implementación Python duplicada
— todos dependen del sidecar Rust (`complexity-engine`); sin él, degradan a
`_unsupported()` (C/C++/JS/TS/Fortran, que nunca tuvieron un fallback propio)
o al esqueleto AST liviano (Python, que sí lo tenía desde antes).

Fase 24 (Extensibility Platform): el despacho por extensión en `parse_file`
ya no es una cadena `if/elif` hardcodeada acá — resuelve vía
`services/plugin_registry.py::extension_map()`, que a su vez viene del
manifest de `services/complexity/src/plugin.rs` (`GET /plugins/manifests`).
Fortran ya no es una rama especial: despacha exactamente igual que los otros
5 lenguajes.
"""

from __future__ import annotations

import ast
import asyncio
import re
from collections.abc import Awaitable, Callable
from pathlib import Path
from typing import Any

from services.complexity_client import (
    parse_asm_rust,
    parse_c_rust,
    parse_cpp_rust,
    parse_fortran_rust,
    parse_js_rust,
    parse_python_rich,
    parse_ts_rust,
    scan_project_rust,
)
from services.plugin_registry import extension_map
from services.project_service import IGNORED_DIRS, PARSEABLE_EXTENSIONS, read_project_files


# ══════════════════════════════════════════════════════════════════════════════
#  ENTRADA PRINCIPAL
# ══════════════════════════════════════════════════════════════════════════════


async def parse_file(filename: str, content: str) -> dict[str, Any]:
    """Despacha al parser correcto según extensión — resuelto vía
    `plugin_registry.extension_map()` (Fase 24), no una cadena `if/elif` de
    extensiones hardcodeadas acá. Async porque los 7 lenguajes ahora
    consultan el sidecar Rust — Python (`parse_python_rich`), C/C++
    (`parse_c_rust`/`parse_cpp_rust`, tree-sitter), JS/TS
    (`parse_js_rust`/`parse_ts_rust`, regex+heurística), Fortran
    (`parse_fortran_rust`, tree-sitter), Assembly (`parse_asm_rust`,
    pattern-matching AT&T/Intel — Fase 19)."""
    ext = Path(filename).suffix.lower()
    try:
        lang_id = extension_map().get(ext)
        handler = _LANGUAGE_HANDLERS.get(lang_id) if lang_id else None
        if handler is not None:
            return await handler(filename, content)
        else:
            return _unsupported(filename, ext)
    except Exception as e:
        return {
            "filename": filename,
            "language": ext,
            "error": str(e),
            "functions": [],
            "classes": [],
            "imports": [],
            "exports": [],
            "complexity": [],
            "big_o": [],
            "dead_code": [],
            "wasm_hints": [],
            "security_findings": [],
            "structural_smells": [],
            "naming_smells": [],
        }


async def parse_project_files(project_dir: Path) -> list[dict[str, Any]]:
    """Fase 18, "Project Scanner": parsea un proyecto entero en una sola
    llamada al sidecar Rust (`scan_project_rust` → `POST /scan/project`,
    `services/complexity/src/scanner.rs`) en vez del patrón anterior — leer
    cada archivo acá (`read_project_files`) y mandarlo por HTTP uno por uno
    (`asyncio.gather(parse_file(...) for f in files)`). Ese patrón sigue
    vivo más abajo, como fallback si el sidecar no responde: degradación con
    gracia, no pérdida de funcionalidad, mismo criterio que el resto de los
    endpoints de este módulo. `IGNORED_DIRS`/`PARSEABLE_EXTENSIONS` vienen de
    `project_service.py` — única fuente de verdad de esas listas, para que
    no diverjan entre las dos implementaciones (Python valida/resuelve el
    path; Rust solo recibe las listas ya armadas, no las duplica)."""
    files = await scan_project_rust(str(project_dir), sorted(PARSEABLE_EXTENSIONS), sorted(IGNORED_DIRS))
    if files is not None:
        return files

    raw_files = read_project_files(project_dir)
    results = list(await asyncio.gather(*(parse_file(f["filename"], f["content"]) for f in raw_files)))
    for r, f in zip(results, raw_files, strict=True):
        r["loc"] = f["content"].count("\n")  # mismo campo que ya trae el camino Rust — ver scanner.rs
    return results


# ══════════════════════════════════════════════════════════════════════════════
#  PYTHON PARSER  (ast + symtable)
# ══════════════════════════════════════════════════════════════════════════════


async def _parse_python(filename: str, content: str) -> dict:
    """Big-O, complejidad ciclomática (salvo la excepción de abajo), space
    complexity, recursión, security/taint, structural smells, naming smells,
    call graph, WASM hints e imports no usados son Rust-only cuando el
    sidecar responde (`services/complexity/src/{bigo,space,recursion,
    security,smells,naming,wasm,rich,structure}.rs`, vía `parse_python_rich`)
    — Python ya no los reimplementa en ese camino; el cálculo de imports no
    usados de acá abajo (`ast.walk` sobre `used_names`/`used_attrs`) es
    puramente el fallback para cuando no hay sidecar (`rich is None`), ya no
    la fuente de verdad. Si el sidecar no responde, degrada a un esqueleto
    liviano (`_skeleton_functions_python`/`_skeleton_classes_python`)
    en vez de reimplementar el motor — big_o/space quedan en "?"/None,
    security/smells quedan vacíos. **Excepción deliberada: call graph y WASM
    hints SÍ siguen funcionando en ese camino degradado** —
    `_skeleton_functions_python` ya incluye `calls`/`complexity`/`big_o`/`loc`
    por diseño (ver su propio docstring), así que `_build_call_graph`/
    `_wasm_hints_python` (Python, más abajo) no se borran: son el mecanismo de
    fallback cuando no hay sidecar, no una segunda implementación paralela
    mantenida "por si acaso"."""
    tree = ast.parse(content, filename=filename)
    rich = await parse_python_rich(filename, content)

    if rich is not None:
        functions = rich["functions"]
        classes = rich["classes"]
        imports = rich["imports"]
        dead_imports = rich["dead_code"]
        call_graph = rich["call_graph"]
        wasm_hints = rich["wasm_hints"]
        security_findings = list(rich["security_findings"])
        structural_smells = list(rich["structural_smells"])
        naming_smells = list(rich["naming_smells"])
    else:
        functions = _skeleton_functions_python(tree)
        classes = _skeleton_classes_python(tree)
        imports = _extract_imports_python(tree)
        call_graph = _build_call_graph(functions)
        wasm_hints = _wasm_hints_python(functions, content)
        security_findings, structural_smells, naming_smells = [], [], []

        # ── Imports no usados (heurística) — solo en el camino sin sidecar ──
        used_names = {n.id for n in ast.walk(tree) if isinstance(n, ast.Name)}
        used_attrs = {n.attr for n in ast.walk(tree) if isinstance(n, ast.Attribute)}
        dead_imports = []
        for imp in imports:
            alias = imp.get("alias") or imp.get("name") or imp["module"].split(".")[0]
            if alias and alias != "*":
                if alias not in used_names and alias not in used_attrs:
                    dead_imports.append(
                        {
                            "type": "unused_import",
                            "name": alias,
                            "module": imp["module"],
                            "line": imp["line"],
                        }
                    )

    security_findings.sort(key=lambda f: f["line"])
    structural_smells.sort(key=lambda s: s["line"])
    naming_smells.sort(key=lambda s: s["line"])

    return {
        "filename": filename,
        "language": "python",
        "functions": functions,
        "classes": classes,
        "imports": imports,
        "exports": [],  # Python no tiene exports explícitos
        "dead_code": dead_imports,
        "call_graph": call_graph,
        "wasm_hints": wasm_hints,
        "security_findings": security_findings,
        "structural_smells": structural_smells,
        "naming_smells": naming_smells,
        "summary": {
            "total_functions": len(functions),
            "total_classes": len(classes),
            "total_imports": len(imports),
            "unused_imports": len(dead_imports),
            "avg_complexity": round(sum(f["complexity"] for f in functions) / len(functions), 2) if functions else 0,
            "max_loc_function": max((f["loc"] for f in functions), default=0),
            "security_findings": len(security_findings),
            "structural_smells": len(structural_smells),
            "naming_smells": len(naming_smells),
        },
    }


def _extract_imports_python(tree: ast.AST) -> list[dict]:
    """Mismo shape que `structure::extract_imports` de Rust — se usa solo en
    el camino degradado (`rich is None`), cuando conviene no depender del
    sidecar ni siquiera para esto."""
    imports: list[dict] = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                imports.append({"module": alias.name, "alias": alias.asname, "type": "import", "line": node.lineno})
        elif isinstance(node, ast.ImportFrom):
            mod = node.module or ""
            for alias in node.names:
                imports.append(
                    {
                        "module": mod,
                        "name": alias.name,
                        "alias": alias.asname,
                        "type": "from_import",
                        "line": node.lineno,
                    }
                )
    return imports


def _skeleton_functions_python(tree: ast.AST) -> list[dict]:
    """Estructura básica de funciones para cuando el sidecar Rust no está
    disponible — un solo `ast.walk`, sin heurística: Big-O/space/recursión/
    seguridad/smells/pureza (Fase 15, `purity.rs`) son responsabilidad de
    Rust y, sin sidecar, simplemente no se calculan (no se reimplementan
    acá) — `is_pure` degrada a `False` (nunca se afirma pureza sin poder
    probarla, mismo criterio conservador que el propio `purity.rs`
    documenta). Mantiene lo mínimo
    que WASM hints/call graph necesitan para seguir funcionando: nombre,
    línea, LOC, args, decorators, docstring, calls — más `complexity`, que sí
    se mantiene barato de calcular localmente (`_cyclomatic_python`, la única
    heurística de este módulo que sigue viva, porque también la necesita
    `metrics_live.py` sin red)."""
    functions: list[dict] = []
    for node in ast.walk(tree):
        if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef):
            end = _end_line(node)
            functions.append(
                {
                    "name": node.name,
                    "line": node.lineno,
                    "end_line": end,
                    "loc": end - node.lineno + 1,
                    "args": [a.arg for a in node.args.args],
                    "is_async": isinstance(node, ast.AsyncFunctionDef),
                    "decorators": [_decorator_name(d) for d in node.decorator_list],
                    "docstring": ast.get_docstring(node),
                    "complexity": _cyclomatic_python(node),
                    "big_o": "?",
                    "big_o_reason": "",
                    "big_o_theta": "?",
                    "big_o_omega": "?",
                    "combinatorics_note": None,
                    "space_complexity": "?",
                    "space_reason": "",
                    "is_recursive": False,
                    "is_tail_recursive": False,
                    "recursion_note": None,
                    "induction_note": None,
                    "recurrence": None,
                    "regex_class": None,
                    "regex_note": None,
                    "grammar_class": None,
                    "grammar_note": None,
                    "graph_traversal": None,
                    "graph_traversal_note": None,
                    "semantic_analysis_class": None,
                    "semantic_analysis_note": None,
                    "data_structure": None,
                    "data_structure_note": None,
                    "calls": _extract_calls_python(node),
                    "returns_annotated": node.returns is not None,
                    "is_pure": False,
                    "purity_note": "",
                }
            )
    return functions


def _skeleton_classes_python(tree: ast.AST) -> list[dict]:
    """Ídem `_skeleton_functions_python` para clases — sin `attribute_count`
    real (necesitaría el mismo recorrido que god_object, que es de Rust),
    queda en 0. `data_structure`/`data_structure_note` (Fase 14) tampoco se
    recalculan acá — ese clasificador es Rust-only, igual que
    `regex_class`/`grammar_class` en `_skeleton_functions_python`."""
    classes: list[dict] = []
    for node in ast.walk(tree):
        if isinstance(node, ast.ClassDef):
            methods = [
                {
                    "name": item.name,
                    "line": item.lineno,
                    "args": [a.arg for a in item.args.args],
                    "is_async": isinstance(item, ast.AsyncFunctionDef),
                }
                for item in node.body
                if isinstance(item, ast.FunctionDef | ast.AsyncFunctionDef)
            ]
            class_end = _end_line(node)
            classes.append(
                {
                    "name": node.name,
                    "line": node.lineno,
                    "end_line": class_end,
                    "loc": class_end - node.lineno + 1,
                    "bases": [_node_name(b) for b in node.bases],
                    "methods": methods,
                    "decorators": [_decorator_name(d) for d in node.decorator_list],
                    "docstring": ast.get_docstring(node),
                    "attribute_count": 0,
                    "data_structure": None,
                    "data_structure_note": None,
                }
            )
    return classes


# ══════════════════════════════════════════════════════════════════════════════
#  C/C++/JS/TS/FORTRAN PARSERS — Rust-only
#  (`services/complexity/src/{cparse,jsts,fparse}.rs`)
# ══════════════════════════════════════════════════════════════════════════════
#
# Fase 18: portados de tree-sitter-en-Python (C/C++) y regex-en-Python
# (JS/TS) a Rust — mismas gramáticas tree-sitter, mismo regex+heurística,
# ahora vía el sidecar. Fase 20 sumó Fortran, que no porta nada previo (nace
# directo en Rust). A diferencia de `_parse_python`, no hay un fallback
# Python degradado acá: ninguno de estos lenguajes tenía ya un "esqueleto
# liviano" (esa pieza es específica del path Python, ver
# `_skeleton_functions_python`), así que si el sidecar no responde, el
# resultado es el mismo `_unsupported()` que ya se usaba cuando tree-sitter
# no estaba instalado — no una segunda implementación mantenida en paralelo.


async def _parse_c(filename: str, content: str) -> dict:
    return await _parse_c_cpp_via_rust(filename, content, "c")


async def _parse_cpp(filename: str, content: str) -> dict:
    return await _parse_c_cpp_via_rust(filename, content, "cpp")


async def _parse_c_cpp_via_rust(filename: str, content: str, lang: str) -> dict:
    """Mismo shape que `_parse_js_ts_via_rust` — solo cambia qué función Rust
    llamar, la extensión que reporta `_unsupported()`, y las 2 diferencias
    puntuales de `summary` (C reporta `total_structs`/`total_macros`, C++
    reporta `total_classes` sin macros, porque tree-sitter-c no tiene macros
    per-clase que contar de la misma forma).

    `memory_layout` (Fase 23) viaja gratis en el mismo resultado — `memlayout.rs`
    corre sobre el mismo árbol que `cparse.rs` ya parseó para
    funciones/clases/etc., no un segundo viaje al sidecar. `modernization`
    (Fase 25, primer motor real) viaja igual de gratis — `modernization.rs`
    reinterpreta `memory_layout.allocations`, cero AST nuevo. Ninguno de los
    dos se agrega a `_unsupported()`: mismo criterio que `macros` (nunca
    estuvieron en ese shape genérico compartido con Python/JS/TS), el
    frontend los renderiza solo si están presentes."""
    fn = parse_c_rust if lang == "c" else parse_cpp_rust
    result = await fn(filename, content)
    if result is None:
        return _unsupported(filename, f".{lang}", reason="sidecar Rust no disponible")

    summary = {
        "total_functions": len(result["functions"]),
        "total_includes": len(result["imports"]),
    }
    if lang == "c":
        summary["total_structs"] = len(result["classes"])
        summary["total_macros"] = len(result["macros"])
    else:
        summary["total_classes"] = len(result["classes"])

    return {
        "filename": filename,
        "language": lang,
        "functions": result["functions"],
        "classes": result["classes"],
        "imports": result["imports"],
        "exports": [],
        "dead_code": [],
        "macros": result["macros"],
        "call_graph": result["call_graph"],
        "wasm_hints": result["wasm_hints"],
        "memory_layout": result["memory"],
        "modernization": result["modernization"],
        "security_findings": [],
        "structural_smells": [],
        "naming_smells": [],
        "summary": summary,
    }


async def _parse_fortran(filename: str, content: str) -> dict:
    """Fase 20 (Scientific Intelligence) — a diferencia de C/C++/JS/TS, no
    porta ningún parser Python previo (Sythrall no tenía soporte Fortran).
    Mismo criterio de degradación sin fallback que el resto de este bloque:
    sin sidecar, `_unsupported()`. `fparse.rs` no calcula `classes`/`macros`/
    `wasm_hints` (Fortran no tiene clases ni macros de preprocesador en el
    sentido C, y WASM no es un target relevante para código ya compilado a
    nativo) — se devuelven vacíos para mantener el shape uniforme que espera
    el frontend, no porque falte implementar algo."""
    result = await parse_fortran_rust(filename, content)
    if result is None:
        return _unsupported(filename, ".f90", reason="sidecar Rust no disponible")

    return {
        "filename": filename,
        "language": "fortran",
        "functions": result["functions"],
        "classes": [],
        "imports": result["imports"],
        "exports": [],
        "dead_code": [],
        "macros": [],
        "call_graph": result["call_graph"],
        "wasm_hints": [],
        "security_findings": [],
        "structural_smells": [],
        "naming_smells": [],
        "summary": {
            "total_functions": len(result["functions"]),
            "total_uses": len(result["imports"]),
        },
    }


async def _parse_asm(filename: str, content: str) -> dict:
    """Fase 19 (Machine Intelligence), primer bullet — como Fortran, nace
    directo en Rust, Sythrall no tenía soporte Assembly antes de esto.
    `asmparse.rs` nunca deja de parsear del lado Rust (pattern-matching
    sobre texto, no tree-sitter) — `result is None` acá solo pasa si el
    sidecar mismo no respondió, mismo criterio de degradación que el resto
    de este bloque: sin sidecar, `_unsupported()`. Los "procedimientos"
    (delimitados por labels) viajan en la clave `functions` — mismo nombre
    que el resto de lenguajes, para que el Big-O table del frontend los
    renderice sin ningún cambio. `asm_syntax` (`"att"`/`"intel"`, detectado
    automáticamente) es el único campo nuevo a nivel raíz."""
    result = await parse_asm_rust(filename, content)
    if result is None:
        return _unsupported(filename, ".s", reason="sidecar Rust no disponible")

    return {
        "filename": filename,
        "language": "assembly",
        "asm_syntax": result["syntax"],
        "functions": result["procedures"],
        "classes": [],
        "imports": [],
        "exports": [],
        "dead_code": [],
        "macros": [],
        "call_graph": result["call_graph"],
        "wasm_hints": [],
        "security_findings": [],
        "structural_smells": [],
        "naming_smells": [],
        "summary": {
            "total_procedures": len(result["procedures"]),
            "syntax": result["syntax"],
        },
    }


async def _parse_js(filename: str, content: str) -> dict:
    return await _parse_js_ts_via_rust(filename, content, "javascript")


async def _parse_ts(filename: str, content: str) -> dict:
    return await _parse_js_ts_via_rust(filename, content, "typescript")


# Fase 24 (Extensibility Platform) — despacho vía el manifest de
# `plugin_registry.py` (a su vez sourced desde `plugin.rs`), no una cadena
# `if/elif` de extensiones hardcodeadas. Fortran ya no es "otra rama" propia:
# resuelve exactamente igual que los otros 5, la prueba concreta que pedía
# el bullet 2 de la fase ("Phase 20's Fortran work is the natural
# candidate... instead of another branch hardcoded into static_parser.py").
_LANGUAGE_HANDLERS: dict[str, Callable[[str, str], Awaitable[dict[str, Any]]]] = {
    "python": _parse_python,
    "c": _parse_c,
    "cpp": _parse_cpp,
    "javascript": _parse_js,
    "typescript": _parse_ts,
    "fortran": _parse_fortran,
    "assembly": _parse_asm,
}


async def _parse_js_ts_via_rust(filename: str, content: str, lang: str) -> dict:
    fn = parse_ts_rust if lang == "typescript" else parse_js_rust
    result = await fn(filename, content)
    if result is None:
        return _unsupported(filename, ".ts" if lang == "typescript" else ".js", reason="sidecar Rust no disponible")
    return {
        "filename": filename,
        "language": lang,
        "functions": result["functions"],
        "classes": result["classes"],
        "imports": result["imports"],
        "exports": result["exports"],
        "interfaces": result["interfaces"],
        "types": result["types"],
        "dead_code": result["dead_code"],
        "call_graph": result["call_graph"],
        "wasm_hints": result["wasm_hints"],
        "security_findings": [],
        "structural_smells": [],
        "naming_smells": [],
        "summary": {
            "total_functions": len(result["functions"]),
            "total_classes": len(result["classes"]),
            "total_imports": len(result["imports"]),
            "total_exports": len(result["exports"]),
            "total_interfaces": len(result["interfaces"]),
            "unused_imports": len(result["dead_code"]),
            "avg_complexity": result["avg_complexity"],
        },
    }


# ══════════════════════════════════════════════════════════════════════════════
#  CYCLOMATIC COMPLEXITY
# ══════════════════════════════════════════════════════════════════════════════


def _cyclomatic_python(func: ast.FunctionDef) -> int:
    """Complejidad ciclomática de McCabe para Python."""
    cc = 1
    for node in ast.walk(func):
        if isinstance(
            node, ast.If | ast.While | ast.For | ast.ExceptHandler | ast.With | ast.Assert | ast.comprehension
        ):
            cc += 1
        elif isinstance(node, ast.BoolOp):
            cc += len(node.values) - 1
    return cc


# ══════════════════════════════════════════════════════════════════════════════
#  CALL GRAPH
# ══════════════════════════════════════════════════════════════════════════════


def _build_call_graph(functions: list[dict]) -> list[dict]:
    """Construye edges del call graph a partir de las llamadas detectadas."""
    func_names = {f["name"] for f in functions}
    edges: list[dict] = []
    seen: set[str] = set()

    for func in functions:
        caller = func["name"]
        for callee in func.get("calls", []):
            if callee in func_names and callee != caller:
                key = f"{caller}→{callee}"
                if key not in seen:
                    seen.add(key)
                    edges.append({"from": caller, "to": callee})

    return edges


def _extract_calls_python(func: ast.FunctionDef) -> list[str]:
    calls = []
    for node in ast.walk(func):
        if isinstance(node, ast.Call):
            if isinstance(node.func, ast.Name):
                calls.append(node.func.id)
            elif isinstance(node.func, ast.Attribute):
                calls.append(node.func.attr)
    return list(set(calls))


# Security/taint (Fase 21), structural smells (Fase 22) y naming smells
# (Fase 22) para archivos Python son Rust-only ahora
# (`services/complexity/src/{security,smells,naming}.rs`, vía
# `parse_python_rich`) — ver el docstring de `_parse_python`.


# ══════════════════════════════════════════════════════════════════════════════
#  WASM / PERFORMANCE HINTS
# ══════════════════════════════════════════════════════════════════════════════

# Umbrales para considerar un hot path
_WASM_CC_THRESHOLD = 5  # Complejidad ciclomática
_WASM_LOC_THRESHOLD = 30  # Líneas de código
_WASM_BIGO_HOT = {"O(n²)", "O(n³)", "O(2^n)"}  # Big O que se benefician de WASM


def _wasm_hints_python(functions: list[dict], content: str) -> list[dict]:
    hints: list[dict] = []

    # Detectar uso actual de Cython
    has_cython = bool(re.search(r"\bcimport\b|\bcdef\b|\bcpdef\b", content))

    for func in functions:
        reasons: list[str] = []
        priority = 0

        if func["big_o"] in _WASM_BIGO_HOT:
            reasons.append(f"Complejidad {func['big_o']} — candidato a optimización WASM")
            priority += 3

        if func["complexity"] >= _WASM_CC_THRESHOLD:
            reasons.append(f"Complejidad ciclomática alta ({func['complexity']})")
            priority += 2

        if func["loc"] >= _WASM_LOC_THRESHOLD:
            reasons.append(f"Función grande ({func['loc']} líneas)")
            priority += 1

        # Detectar operaciones numéricas intensivas
        is_numeric = any(
            kw in func["name"].lower()
            for kw in [
                "sort",
                "search",
                "compute",
                "calc",
                "matrix",
                "multiply",
                "fft",
                "transform",
                "encode",
                "decode",
                "hash",
                "compress",
                "convolve",
            ]
        )
        if is_numeric:
            reasons.append("Nombre sugiere operación numérica intensiva")
            priority += 2

        if reasons:
            rec = _wasm_recommendation_python(func, has_cython)
            hints.append(
                {
                    "function": func["name"],
                    "line": func["line"],
                    "priority": priority,
                    "reasons": reasons,
                    "recommendation": rec,
                    "estimated_speedup": _estimate_speedup(func),
                }
            )

    # Detección de módulos .wasm en el contenido
    if "import.wasm" in content or ".wasm" in content:
        hints.append(
            {
                "function": "<module>",
                "line": 1,
                "priority": 5,
                "reasons": ["Archivo usa módulos .wasm directamente"],
                "recommendation": "Asegúrate de que los bindings WASM están tipados correctamente",
                "estimated_speedup": "N/A",
            }
        )

    return sorted(hints, key=lambda x: -x["priority"])


def _wasm_recommendation_python(func: dict, has_cython: bool) -> str:
    name = func["name"]
    bigo = func["big_o"]

    if has_cython:
        return f"Ya usas Cython — añade 'cdef' a '{name}' con tipos C para compilar a .so"

    if bigo in ("O(n²)", "O(n³)"):
        return (
            f"'{name}' es un hot path crítico. Opciones:\n"
            f"  1. Cython: cdef double {name}(int n) — compila a .so nativo\n"
            f"  2. NumPy vectorización — elimina loops Python\n"
            f"  3. Emscripten → .wasm si necesitas correrlo en browser"
        )

    if bigo == "O(2^n)":
        return (
            f"'{name}' tiene complejidad exponencial. Antes de WASM, considera:\n"
            f"  1. Memoización con @functools.lru_cache\n"
            f"  2. Programación dinámica\n"
            f"  3. Si aún necesitas velocidad: Cython + tipos estáticos"
        )

    return (
        f"'{name}' puede beneficiarse de Cython:\n"
        f"  Agrega 'cdef' al archivo .pyx y compila con: python setup.py build_ext --inplace"
    )


def _estimate_speedup(func: dict) -> str:
    bigo = func["big_o"]
    cc = func["complexity"]
    if bigo in ("O(n³)", "O(2^n)"):
        return "10-100x (crítico)"
    if bigo == "O(n²)":
        return "5-30x"
    if cc >= 10:
        return "2-10x"
    return "1.5-3x"


# ══════════════════════════════════════════════════════════════════════════════
#  HELPERS GENÉRICOS
# ══════════════════════════════════════════════════════════════════════════════


def _end_line(node: ast.AST) -> int:
    return getattr(node, "end_lineno", getattr(node, "lineno", 0))


def _decorator_name(node: ast.expr) -> str:
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        return f"{_node_name(node.value)}.{node.attr}"
    if isinstance(node, ast.Call):
        return _decorator_name(node.func)
    return "?"


def _node_name(node: ast.expr) -> str:
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        return f"{_node_name(node.value)}.{node.attr}"
    return "?"


def _unsupported(filename: str, ext: str, reason: str = "") -> dict:
    return {
        "filename": filename,
        "language": ext,
        "error": reason or f"Extensión '{ext}' no soportada",
        "functions": [],
        "classes": [],
        "imports": [],
        "exports": [],
        "dead_code": [],
        "call_graph": [],
        "wasm_hints": [],
        "security_findings": [],
        "structural_smells": [],
        "naming_smells": [],
        "summary": {},
    }
