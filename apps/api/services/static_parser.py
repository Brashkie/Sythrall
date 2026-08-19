"""
services/static_parser.py
Parser multi-lenguaje SIN IA.
- Python  → ast + symtable nativo
- C/C++   → tree-sitter
- JS/TS   → regex + estructura AST-like (sin dependencias externas en runtime)
- Big O   → heurística por patrones AST
- WASM    → detección de hot paths y recomendaciones
"""

from __future__ import annotations

import ast
import re
from pathlib import Path
from typing import Any

# ── tree-sitter (C/C++) ───────────────────────────────────────────────────────
try:
    from tree_sitter import Language, Parser as TSParser
    import tree_sitter_c as tsc
    import tree_sitter_cpp as tscpp

    _C_LANG = Language(tsc.language())
    _CPP_LANG = Language(tscpp.language())
    HAS_TREESITTER = True
except Exception:
    HAS_TREESITTER = False

# ── networkx (grafos) ─────────────────────────────────────────────────────────
try:
    import networkx as nx

    HAS_NX = True
except ImportError:
    HAS_NX = False


# ══════════════════════════════════════════════════════════════════════════════
#  ENTRADA PRINCIPAL
# ══════════════════════════════════════════════════════════════════════════════


def parse_file(filename: str, content: str) -> dict[str, Any]:
    """Despacha al parser correcto según extensión."""
    ext = Path(filename).suffix.lower()
    try:
        if ext == ".py":
            return _parse_python(filename, content)
        elif ext in (".c",):
            return _parse_c(filename, content)
        elif ext in (".cpp", ".cc", ".cxx", ".hpp", ".h"):
            return _parse_cpp(filename, content)
        elif ext in (".js", ".jsx"):
            return _parse_js(filename, content)
        elif ext in (".ts", ".tsx"):
            return _parse_ts(filename, content)
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
        }


# ══════════════════════════════════════════════════════════════════════════════
#  PYTHON PARSER  (ast + symtable)
# ══════════════════════════════════════════════════════════════════════════════


def _parse_python(filename: str, content: str) -> dict:
    tree = ast.parse(content, filename=filename)

    functions: list[dict] = []
    classes: list[dict] = []
    imports: list[dict] = []
    security_findings: list[dict] = []
    structural_smells: list[dict] = []

    # ── Imports ───────────────────────────────────────────────────────────────
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                imports.append(
                    {
                        "module": alias.name,
                        "alias": alias.asname,
                        "type": "import",
                        "line": node.lineno,
                    }
                )
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

    # ── Funciones y clases ────────────────────────────────────────────────────
    for node in ast.walk(tree):
        if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef):
            end = _end_line(node)
            loc = end - node.lineno + 1
            cc = _cyclomatic_python(node)
            recursion = _recursion_info_python(node)
            depth, has_early_exit = _loop_analysis_python(node)
            bigo, bigo_reason = _infer_big_o_python(node, recursion["is_recursive"], depth)
            bigo_theta, bigo_omega = _theta_omega_python(has_early_exit, bigo)
            space, space_reason = _infer_space_python(node, recursion["is_recursive"])
            decorators = [_decorator_name(d) for d in node.decorator_list]

            regex_info = _regex_info_python(node)
            grammar_info = _grammar_info_python(node, recursion["is_recursive"])
            graph_info = _graph_traversal_info_python(node, recursion["is_recursive"])
            security_findings.extend(_security_findings_python(node))

            structural_smells.extend(
                f
                for f in (
                    _check_long_function(node.name, node.lineno, loc),
                    _check_excessive_parameters(node.name, node.lineno, len(node.args.args)),
                    _check_deep_nesting(node.name, node.lineno, node),
                )
                if f is not None
            )

            functions.append(
                {
                    "name": node.name,
                    "line": node.lineno,
                    "end_line": end,
                    "loc": loc,
                    "args": [a.arg for a in node.args.args],
                    "is_async": isinstance(node, ast.AsyncFunctionDef),
                    "decorators": decorators,
                    "docstring": ast.get_docstring(node),
                    "complexity": cc,
                    "big_o": bigo,
                    "big_o_reason": bigo_reason,
                    "big_o_theta": bigo_theta,
                    "big_o_omega": bigo_omega,
                    "space_complexity": space,
                    "space_reason": space_reason,
                    "is_recursive": recursion["is_recursive"],
                    "is_tail_recursive": recursion["is_tail_recursive"],
                    "recursion_note": _recursion_note(recursion),
                    "regex_class": "Type-3 (Regular)" if regex_info["uses_regex"] else None,
                    "regex_note": _regex_note(regex_info),
                    "grammar_class": "Type-2 (Context-Free)" if grammar_info["is_grammar_shaped"] else None,
                    "grammar_note": _grammar_note(grammar_info),
                    "graph_traversal": graph_info["traversal_kind"],
                    "graph_traversal_note": _graph_traversal_note(graph_info),
                    "calls": _extract_calls_python(node),
                    "returns_annotated": node.returns is not None,
                }
            )

        elif isinstance(node, ast.ClassDef):
            methods = []
            for item in node.body:
                if isinstance(item, ast.FunctionDef | ast.AsyncFunctionDef):
                    methods.append(
                        {
                            "name": item.name,
                            "line": item.lineno,
                            "args": [a.arg for a in item.args.args],
                            "is_async": isinstance(item, ast.AsyncFunctionDef),
                        }
                    )
            bases = [_node_name(b) for b in node.bases]
            class_end = _end_line(node)
            class_loc = class_end - node.lineno + 1
            attribute_count = _count_self_attributes_python(node)
            classes.append(
                {
                    "name": node.name,
                    "line": node.lineno,
                    "end_line": class_end,
                    "loc": class_loc,
                    "bases": bases,
                    "methods": methods,
                    "decorators": [_decorator_name(d) for d in node.decorator_list],
                    "docstring": ast.get_docstring(node),
                    "attribute_count": attribute_count,
                }
            )
            structural_smells.extend(
                f
                for f in (
                    _check_large_class(node.name, node.lineno, len(methods), class_loc),
                    _check_god_object(node.name, node.lineno, len(methods), attribute_count),
                )
                if f is not None
            )

    # ── Imports no usados (heurística) ───────────────────────────────────────
    used_names = {n.id for n in ast.walk(tree) if isinstance(n, ast.Name)}
    used_attrs = {n.attr for n in ast.walk(tree) if isinstance(n, ast.Attribute)}
    dead_imports: list[dict] = []
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

    # ── WASM / hot path hints ─────────────────────────────────────────────────
    wasm_hints = _wasm_hints_python(functions, content)

    # ── Call graph (dentro del mismo archivo) ─────────────────────────────────
    call_graph = _build_call_graph(functions)

    # ── Dependencias circulares entre imports ─────────────────────────────────
    circular = _detect_circular_imports(imports)

    # ── Security & Taint (Fase 21 v1) ─────────────────────────────────────────
    security_findings.extend(_hardcoded_credentials_python(tree))
    security_findings.sort(key=lambda f: f["line"])

    # ── Structural Smells (Fase 22) ────────────────────────────────────────────
    structural_smells.sort(key=lambda s: s["line"])

    return {
        "filename": filename,
        "language": "python",
        "functions": functions,
        "classes": classes,
        "imports": imports,
        "exports": [],  # Python no tiene exports explícitos
        "dead_code": dead_imports,
        "call_graph": call_graph,
        "circular_deps": circular,
        "wasm_hints": wasm_hints,
        "security_findings": security_findings,
        "structural_smells": structural_smells,
        "summary": {
            "total_functions": len(functions),
            "total_classes": len(classes),
            "total_imports": len(imports),
            "unused_imports": len(dead_imports),
            "avg_complexity": round(sum(f["complexity"] for f in functions) / len(functions), 2) if functions else 0,
            "max_loc_function": max((f["loc"] for f in functions), default=0),
            "security_findings": len(security_findings),
            "structural_smells": len(structural_smells),
        },
    }


# ══════════════════════════════════════════════════════════════════════════════
#  C PARSER  (tree-sitter)
# ══════════════════════════════════════════════════════════════════════════════


def _parse_c(filename: str, content: str) -> dict:
    if not HAS_TREESITTER:
        return _unsupported(filename, ".c", reason="tree-sitter no instalado")

    parser = TSParser(_C_LANG)
    tree = parser.parse(content.encode())
    root = tree.root_node

    functions = _ts_extract_functions(root, content, "c")
    includes = _ts_extract_includes(root, content)
    structs = _ts_extract_structs(root, content)
    macros = _ts_extract_macros(root, content)
    wasm_hints = _wasm_hints_c(functions, content)

    return {
        "filename": filename,
        "language": "c",
        "functions": functions,
        "classes": structs,
        "imports": includes,
        "exports": [],
        "dead_code": [],
        "macros": macros,
        "call_graph": _build_call_graph(functions),
        "wasm_hints": wasm_hints,
        "security_findings": [],
        "structural_smells": [],
        "summary": {
            "total_functions": len(functions),
            "total_structs": len(structs),
            "total_includes": len(includes),
            "total_macros": len(macros),
        },
    }


def _parse_cpp(filename: str, content: str) -> dict:
    if not HAS_TREESITTER:
        return _unsupported(filename, ".cpp", reason="tree-sitter no instalado")

    parser = TSParser(_CPP_LANG)
    tree = parser.parse(content.encode())
    root = tree.root_node

    functions = _ts_extract_functions(root, content, "cpp")
    includes = _ts_extract_includes(root, content)
    classes = _ts_extract_classes_cpp(root, content)
    macros = _ts_extract_macros(root, content)
    wasm_hints = _wasm_hints_c(functions, content)

    return {
        "filename": filename,
        "language": "cpp",
        "functions": functions,
        "classes": classes,
        "imports": includes,
        "exports": [],
        "dead_code": [],
        "macros": macros,
        "call_graph": _build_call_graph(functions),
        "wasm_hints": wasm_hints,
        "security_findings": [],
        "structural_smells": [],
        "summary": {
            "total_functions": len(functions),
            "total_classes": len(classes),
            "total_includes": len(includes),
        },
    }


# ── tree-sitter helpers ───────────────────────────────────────────────────────


def _ts_extract_functions(root, content: str, lang: str) -> list[dict]:
    functions = []
    lines = content.splitlines()

    def walk(node):
        if node.type == "function_definition":
            name = _ts_func_name(node)
            start = node.start_point[0] + 1
            end = node.end_point[0] + 1
            loc = end - start + 1
            body_src = "\n".join(lines[start - 1 : end])

            # Complejidad ciclomática básica por conteo de branch keywords.
            # No sumar " else if " aparte: " else if " ya contiene " if " como
            # substring, así que cada `else if` se contaba dos veces (CC=6 en
            # vez de 4 para un if + 2 else-if) — " if " solo ya cubre ambos.
            cc = (
                1
                + body_src.count(" if ")
                + body_src.count(" for ")
                + body_src.count(" while ")
                + body_src.count(" case ")
                + body_src.count(" && ")
                + body_src.count(" || ")
            )

            bigo, reason = _infer_big_o_c(body_src)

            functions.append(
                {
                    "name": name,
                    "line": start,
                    "end_line": end,
                    "loc": loc,
                    "complexity": cc,
                    "big_o": bigo,
                    "big_o_reason": reason,
                    "calls": _extract_calls_c(node),
                }
            )
        for child in node.children:
            walk(child)

    walk(root)
    return functions


def _ts_func_name(node) -> str:
    """Extrae el nombre de una función de un nodo tree-sitter."""
    for child in node.children:
        if child.type == "function_declarator":
            for sub in child.children:
                if sub.type == "identifier":
                    return sub.text.decode("utf-8", errors="replace")
        if child.type in ("identifier", "qualified_identifier"):
            return child.text.decode("utf-8", errors="replace")
    return "<anonymous>"


def _ts_extract_includes(root, content: str) -> list[dict]:
    includes = []

    def walk(node):
        if node.type == "preproc_include":
            path_node = next((c for c in node.children if c.type in ("string_literal", "system_lib_string")), None)
            if path_node:
                raw = path_node.text.decode("utf-8", errors="replace").strip('"<>')
                includes.append(
                    {
                        "module": raw,
                        "type": "include",
                        "line": node.start_point[0] + 1,
                    }
                )
        for child in node.children:
            walk(child)

    walk(root)
    return includes


def _ts_extract_structs(root, content: str) -> list[dict]:
    structs = []

    def walk(node):
        if node.type in ("struct_specifier", "union_specifier"):
            name_node = next((c for c in node.children if c.type == "type_identifier"), None)
            if name_node:
                structs.append(
                    {
                        "name": name_node.text.decode("utf-8", errors="replace"),
                        "line": node.start_point[0] + 1,
                        "kind": node.type.replace("_specifier", ""),
                    }
                )
        for child in node.children:
            walk(child)

    walk(root)
    return structs


def _ts_extract_classes_cpp(root, content: str) -> list[dict]:
    classes = []

    def walk(node):
        if node.type == "class_specifier":
            name_node = next((c for c in node.children if c.type == "type_identifier"), None)
            if name_node:
                classes.append(
                    {
                        "name": name_node.text.decode("utf-8", errors="replace"),
                        "line": node.start_point[0] + 1,
                        "kind": "class",
                    }
                )
        for child in node.children:
            walk(child)

    walk(root)
    return classes


def _ts_extract_macros(root, content: str) -> list[dict]:
    macros = []

    def walk(node):
        if node.type == "preproc_def":
            name_node = next((c for c in node.children if c.type == "identifier"), None)
            if name_node:
                macros.append(
                    {
                        "name": name_node.text.decode("utf-8", errors="replace"),
                        "line": node.start_point[0] + 1,
                    }
                )
        for child in node.children:
            walk(child)

    walk(root)
    return macros


def _extract_calls_c(func_node) -> list[str]:
    calls = []

    def walk(node):
        if node.type == "call_expression":
            fn = next((c for c in node.children if c.type == "identifier"), None)
            if fn:
                calls.append(fn.text.decode("utf-8", errors="replace"))
        for child in node.children:
            walk(child)

    walk(func_node)
    return list(set(calls))


# ══════════════════════════════════════════════════════════════════════════════
#  JS/TS PARSER  (regex + AST-like sin deps externas)
# ══════════════════════════════════════════════════════════════════════════════

_JS_FUNC_RE = re.compile(
    r"(?:export\s+)?(?:async\s+)?function\s+(\w+)\s*\(([^)]*)\)"
    r"|const\s+(\w+)\s*=\s*(?:async\s*)?\(([^)]*)\)\s*=>"
    r"|(?:export\s+)?(?:async\s+)?function\s*\(([^)]*)\)",
    re.MULTILINE,
)
_JS_CLASS_RE = re.compile(r"(?:export\s+)?class\s+(\w+)(?:\s+extends\s+(\w+))?", re.MULTILINE)
_JS_IMPORT_RE = re.compile(
    r"import\s+(?:{[^}]+}|[\w*]+)?\s*(?:,\s*{[^}]+})?\s*from\s+['\"]([^'\"]+)['\"]", re.MULTILINE
)
_JS_EXPORT_RE = re.compile(r"export\s+(?:default\s+)?(?:function|class|const|let|var)\s+(\w+)", re.MULTILINE)
_TS_IFACE_RE = re.compile(r"(?:export\s+)?interface\s+(\w+)", re.MULTILINE)
_TS_TYPE_RE = re.compile(r"(?:export\s+)?type\s+(\w+)\s*=", re.MULTILINE)


def _parse_js(filename: str, content: str) -> dict:
    return _parse_js_ts(filename, content, "javascript")


def _parse_ts(filename: str, content: str) -> dict:
    return _parse_js_ts(filename, content, "typescript")


def _parse_js_ts(filename: str, content: str, lang: str) -> dict:
    lines = content.splitlines()
    n_lines = len(lines)

    # ── Funciones ─────────────────────────────────────────────────────────────
    functions: list[dict] = []
    seen_funcs: set[str] = set()

    for m in _JS_FUNC_RE.finditer(content):
        name = m.group(1) or m.group(3) or "<anonymous>"
        if name in seen_funcs:
            continue
        seen_funcs.add(name)
        line_no = content[: m.start()].count("\n") + 1
        # Calcular LOC aproximado buscando la llave de cierre
        loc = _estimate_js_func_loc(content, m.start())
        body = "\n".join(lines[line_no - 1 : line_no - 1 + loc])
        cc = _cyclomatic_js(body)
        bigo, reason = _infer_big_o_js(body)

        functions.append(
            {
                "name": name,
                "line": line_no,
                "end_line": min(line_no + loc, n_lines),
                "loc": loc,
                "complexity": cc,
                "big_o": bigo,
                "big_o_reason": reason,
                "calls": _extract_calls_js(body),
                "is_async": "async" in content[max(0, m.start() - 10) : m.start() + 5],
            }
        )

    # ── Clases ────────────────────────────────────────────────────────────────
    classes = []
    for m in _JS_CLASS_RE.finditer(content):
        line_no = content[: m.start()].count("\n") + 1
        classes.append(
            {
                "name": m.group(1),
                "extends": m.group(2),
                "line": line_no,
            }
        )

    # ── Imports ───────────────────────────────────────────────────────────────
    imports = []
    for m in _JS_IMPORT_RE.finditer(content):
        line_no = content[: m.start()].count("\n") + 1
        imports.append(
            {
                "module": m.group(1),
                "type": "esm_import",
                "line": line_no,
            }
        )

    # ── Exports ───────────────────────────────────────────────────────────────
    exports = []
    for m in _JS_EXPORT_RE.finditer(content):
        line_no = content[: m.start()].count("\n") + 1
        exports.append({"name": m.group(1), "line": line_no})

    # ── TypeScript extras ─────────────────────────────────────────────────────
    interfaces, types_list = [], []
    if lang == "typescript":
        for m in _TS_IFACE_RE.finditer(content):
            line_no = content[: m.start()].count("\n") + 1
            interfaces.append({"name": m.group(1), "line": line_no})
        for m in _TS_TYPE_RE.finditer(content):
            line_no = content[: m.start()].count("\n") + 1
            types_list.append({"name": m.group(1), "line": line_no})

    # ── Imports no usados ─────────────────────────────────────────────────────
    dead_imports = _dead_js_imports(imports, content)

    # ── WASM hints ────────────────────────────────────────────────────────────
    wasm_hints = _wasm_hints_js(functions, content)

    return {
        "filename": filename,
        "language": lang,
        "functions": functions,
        "classes": classes,
        "imports": imports,
        "exports": exports,
        "interfaces": interfaces,
        "types": types_list,
        "dead_code": dead_imports,
        "call_graph": _build_call_graph(functions),
        "wasm_hints": wasm_hints,
        "security_findings": [],
        "structural_smells": [],
        "summary": {
            "total_functions": len(functions),
            "total_classes": len(classes),
            "total_imports": len(imports),
            "total_exports": len(exports),
            "total_interfaces": len(interfaces),
            "unused_imports": len(dead_imports),
            "avg_complexity": round(sum(f["complexity"] for f in functions) / len(functions), 2) if functions else 0,
        },
    }


def _estimate_js_func_loc(content: str, start: int) -> int:
    """Estima líneas de una función JS buscando la llave de cierre
    balanceada. Una arrow function sin `{ }` (cuerpo de una sola expresión,
    ej. `const isEven = (n) => n % 2 === 0;`) nunca abre una llave propia —
    sin este chequeo, el scan seguía de largo por el resto del archivo y
    absorbía la llave de la SIGUIENTE función que sí tuviera bloque,
    heredando su LOC/complejidad/Big-O. Se corta en el primer `;` fuera de
    paréntesis (la lista de parámetros) visto antes de cualquier `{` — no
    cubre el caso sin punto y coma final (ASI), documentado como límite
    aceptado del heurístico."""
    depth, paren_depth, i, max_loc = 0, 0, start, 0
    in_func = False
    while i < len(content) and max_loc < 500:
        ch = content[i]
        if ch == "(":
            paren_depth += 1
        elif ch == ")":
            paren_depth -= 1
        elif ch == "{":
            depth += 1
            in_func = True
        elif ch == "}" and in_func:
            depth -= 1
            if depth == 0:
                return content[start : i + 1].count("\n") + 1
        elif ch == ";" and not in_func and paren_depth <= 0:
            return content[start:i].count("\n") + 1
        elif ch == "\n":
            max_loc += 1
        i += 1
    return max(1, max_loc)


def _extract_calls_js(body: str) -> list[str]:
    return list(set(re.findall(r"(\w+)\s*\(", body)))


def _dead_js_imports(imports: list[dict], content: str) -> list[dict]:
    dead = []
    for imp in imports:
        mod = imp["module"].split("/")[-1].replace("-", "_").replace(".", "_")
        # Buscar si el módulo o algo que de él se importa está en el contenido
        # Heurística simple: buscar el nombre base del módulo
        base = re.sub(r"[^a-zA-Z0-9_]", "", mod)
        if base and len(base) > 1 and content.count(base) <= 1:
            dead.append(
                {
                    "type": "possibly_unused_import",
                    "module": imp["module"],
                    "line": imp["line"],
                }
            )
    return dead


# ══════════════════════════════════════════════════════════════════════════════
#  BIG O HEURÍSTICA
# ══════════════════════════════════════════════════════════════════════════════


def _loop_analysis_python(node: ast.AST) -> tuple[int, bool]:
    """Profundidad máxima de loops anidados y si hay un break/return dentro
    de algún loop (early exit — indica que el mejor caso puede terminar
    antes de recorrer todo, ej. una búsqueda lineal). Un solo recorrido
    recursivo para ambas señales — antes eran dos funciones separadas, la
    segunda con un `ast.walk` anidado extra por cada loop encontrado."""
    LOOP_TYPES = (ast.For, ast.While)
    max_depth = [0]
    has_early_exit = [False]

    def walk(n, depth, inside_loop):
        for child in ast.iter_child_nodes(n):
            is_loop = isinstance(child, LOOP_TYPES)
            d = depth + 1 if is_loop else depth
            max_depth[0] = max(max_depth[0], d)
            child_inside_loop = inside_loop or is_loop
            if child_inside_loop and isinstance(child, ast.Break | ast.Return):
                has_early_exit[0] = True
            walk(child, d, child_inside_loop)

    walk(node, 0, False)
    return max_depth[0], has_early_exit[0]


def _has_binary_split_python(node: ast.AST) -> bool:
    for n in ast.walk(node):
        if isinstance(n, ast.BinOp):
            if isinstance(n.op, ast.FloorDiv) and isinstance(n.right, ast.Constant) and n.right.value == 2:
                return True
            if isinstance(n.op, ast.RShift) and isinstance(n.right, ast.Constant) and n.right.value == 1:
                return True
    return False


def _recursion_info_python(func: ast.FunctionDef) -> dict:
    """Detecta recursión directa (la función se llama a sí misma) y si es
    tail-recursive: cada llamada recursiva es directamente el valor de un
    `return`, sin ninguna operación pendiente después de que retorne
    (ej. `return f(n-1)` es tail; `return n + f(n-1)` no lo es, porque falta
    sumar `n` después de que la llamada vuelva). Python no optimiza
    tail calls — aun siendo tail-recursive, sigue consumiendo stack —
    pero es información útil: indica que se podría reescribir como loop.

    Un solo recorrido del AST para ambas señales (antes eran dos `ast.walk`
    separados) — con miles de funciones en un archivo grande, cada walk de
    más se nota.
    """
    func_name = func.name
    call_count = 0
    tail_count = 0
    for n in ast.walk(func):
        if isinstance(n, ast.Call) and isinstance(n.func, ast.Name) and n.func.id == func_name:
            call_count += 1
        elif (
            isinstance(n, ast.Return)
            and isinstance(n.value, ast.Call)
            and isinstance(n.value.func, ast.Name)
            and n.value.func.id == func_name
        ):
            tail_count += 1

    if call_count == 0:
        return {"is_recursive": False, "is_tail_recursive": False, "call_count": 0}

    is_tail = tail_count == call_count

    return {"is_recursive": True, "is_tail_recursive": is_tail, "call_count": call_count}


def _recursion_note(info: dict) -> str | None:
    """Framing tipo Cálculo Lambda: una recursión tail-call es equivalente a
    un loop (reducible a iteración), aunque Python no la optimiza — sigue
    gastando stack. Una no-tail necesita que cada llamada "espere" el
    resultado de la siguiente para terminar su propio trabajo."""
    if not info["is_recursive"]:
        return None
    if info["is_tail_recursive"]:
        return (
            "Tail-call — cada llamada recursiva es lo último que hace la función; "
            "equivalente a un loop (Cálculo Lambda: reducible a iteración). "
            "Python no optimiza tail calls, así que igual consume stack."
        )
    return (
        "No es tail-call — queda trabajo pendiente después de la llamada recursiva "
        "(ej. sumar su resultado); cada nivel de recursión consume una entrada de stack."
    )


_REGEX_METHODS = {"compile", "match", "fullmatch", "search", "findall", "finditer", "sub", "subn", "split"}


def _regex_info_python(func: ast.FunctionDef) -> dict:
    """Detecta llamadas directas a `re.XXX(...)` — no sigue un `re.Pattern`
    guardado en variable (`p = re.compile(...); p.match(x)`), esa parte
    necesitaría rastrear asignaciones, no vale la pena para una heurística."""
    call_count = 0
    for n in ast.walk(func):
        if (
            isinstance(n, ast.Call)
            and isinstance(n.func, ast.Attribute)
            and n.func.attr in _REGEX_METHODS
            and isinstance(n.func.value, ast.Name)
            and n.func.value.id == "re"
        ):
            call_count += 1
    return {"uses_regex": call_count > 0, "regex_call_count": call_count}


def _regex_note(info: dict) -> str | None:
    """Chomsky Tipo-3 (lenguaje regular, reconocido por un autómata finito).
    Aclaración honesta: el motor `re` de Python es backtracking, no un DFA
    puro — la mayoría de patrones son lineales, pero patrones ambiguos o muy
    anidados pueden degradar a O(2^n) (catastrophic backtracking)."""
    if not info["uses_regex"]:
        return None
    return (
        "Regex detectado — Chomsky Tipo-3 (lenguaje regular), reconocido por un "
        "autómata finito. El motor `re` de Python es backtracking, no un DFA puro: "
        "la mayoría de patrones son lineales, pero patrones ambiguos/anidados pueden "
        "degradar a O(2^n) en el peor caso (catastrophic backtracking)."
    )


def _names_with_calls_python(func: ast.FunctionDef, method_names: set[str]) -> set[str]:
    """Nombres de variable local sobre los que se llamó alguno de
    `method_names` (ej. {"append","pop"} para detectar un patrón de pila
    explícita, {"popleft"} para detectar una cola). Un solo walk, reusado por
    el clasificador de grammar/parser y el de graph traversal."""
    names: set[str] = set()
    for n in ast.walk(func):
        if (
            isinstance(n, ast.Call)
            and isinstance(n.func, ast.Attribute)
            and n.func.attr in method_names
            and isinstance(n.func.value, ast.Name)
        ):
            names.add(n.func.value.id)
    return names


_GRAMMAR_NAME_KEYWORDS = ("parse", "parser", "grammar", "tokenize", "lexer", "lex_", "ast_")


def _grammar_info_python(func: ast.FunctionDef, is_recursive: bool) -> dict:
    """Heurística de dos señales: nombre sugiere parsing Y (recursión O pila
    explícita — ambas formas típicas de recursive-descent / shift-reduce
    parsing). Ninguna señal sola alcanza, para no generar demasiados falsos
    positivos (`is_recursive` se recibe ya calculado, mismo patrón que
    `_infer_big_o_python`)."""
    name_match = any(kw in func.name.lower() for kw in _GRAMMAR_NAME_KEYWORDS)
    if not name_match:
        return {"is_grammar_shaped": False, "signal": None}

    stack_names = _names_with_calls_python(func, {"append", "pop"})
    has_stack = bool(stack_names)

    if is_recursive and has_stack:
        return {"is_grammar_shaped": True, "signal": "name+recursion+stack"}
    if is_recursive:
        return {"is_grammar_shaped": True, "signal": "name+recursion"}
    if has_stack:
        return {"is_grammar_shaped": True, "signal": "name+stack"}
    return {"is_grammar_shaped": False, "signal": None}


def _grammar_note(info: dict) -> str | None:
    if not info["is_grammar_shaped"]:
        return None
    return (
        "Nombre + patrón de recursión/pila sugieren código de parsing — Chomsky "
        "Tipo-2 (gramática libre de contexto), reconocido por un autómata de pila "
        "(pushdown automaton). Heurística por nombre y forma del código, no un "
        "análisis semántico — puede haber falsos positivos/negativos."
    )


def _graph_traversal_info_python(func: ast.FunctionDef, is_recursive: bool) -> dict:
    """`visited`/`seen`/`explored` + señal de cola (`popleft`, BFS) o pila/
    recursión (DFS); `in_degree`/`indegree` → Kahn's algorithm (topological
    sort). Mismo criterio heurístico que el resto del engine — nombres de
    variable, no un análisis de flujo de datos real."""
    assigned = {
        t.id for n in ast.walk(func) if isinstance(n, ast.Assign) for t in n.targets if isinstance(t, ast.Name)
    } | {n.target.id for n in ast.walk(func) if isinstance(n, ast.AnnAssign) and isinstance(n.target, ast.Name)}
    assigned = {name.lower() for name in assigned}

    has_indegree = bool(assigned & {"in_degree", "indegree", "in_deg"})
    has_visited = bool(assigned & {"visited", "seen", "explored"})

    if has_indegree:
        return {"traversal_kind": "Topological Sort (Kahn's algorithm)"}

    if has_visited:
        queue_names = _names_with_calls_python(func, {"popleft"})
        if queue_names:
            return {"traversal_kind": "BFS"}
        stack_names = _names_with_calls_python(func, {"append", "pop"})
        if is_recursive or stack_names:
            return {"traversal_kind": "DFS"}

    return {"traversal_kind": None}


def _graph_traversal_note(info: dict) -> str | None:
    kind = info["traversal_kind"]
    if not kind:
        return None
    return f"{kind} detectado (heurística por nombres de variable) — O(V+E): cada nodo y arista se visita una cantidad constante de veces."


def _infer_big_o_python(func: ast.FunctionDef, is_recursive: bool, depth: int) -> tuple[str, str]:
    """`is_recursive` y `depth` se reciben ya calculados (por
    `_recursion_info_python` / `_loop_analysis_python`) en vez de volver a
    recorrer el AST acá — evita walks duplicados."""
    recursive = is_recursive
    binary = _has_binary_split_python(func)

    if depth == 0 and not recursive:
        return "O(1)", "Sin loops ni recursión — el tiempo no depende de n"
    if depth == 1 and binary:
        return "O(log n)", "Loop con división binaria — el rango se reduce a la mitad en cada iteración"
    if depth == 1 and recursive:
        return "O(n log n)", "Loop combinado con recursión — mezcla un paso lineal con reducción logarítmica"
    if depth == 1:
        return "O(n)", "Un loop — recorre los n elementos una vez"
    if depth == 2:
        return "O(n²)", "2 loops anidados — el loop interno se ejecuta n veces por cada iteración del externo"
    if depth == 3:
        return "O(n³)", "3 loops anidados — cada nivel adicional multiplica el trabajo por n"
    if depth >= 4:
        return f"O(n^{depth})", f"{depth} loops anidados — crecimiento polinomial de grado {depth}"
    if recursive and not binary:
        return (
            "O(2^n)",
            "Recursión sin reducir el problema — cada llamada genera nuevas llamadas sin dividir la entrada",
        )
    return "O(n)", "Caso base — comportamiento lineal por defecto"


def _theta_omega_python(has_early_exit: bool, worst: str) -> tuple[str, str]:
    """Cota ajustada (Θ) y mejor caso (Ω) a partir del peor caso (O) ya inferido.

    Heurística: si hay un break/return dentro de un loop (`has_early_exit`,
    calculado por `_loop_analysis_python`), el mejor caso puede terminar en
    la primera iteración (Ω(1)), como en una búsqueda lineal que encuentra
    el elemento de una — en ese caso no hay una cota Θ única, varía entre el
    mejor y el peor caso. Sin salida temprana, el loop siempre corre hasta
    el final sin importar los datos, así que Θ = Ω = O.
    """
    suffix = worst[1:]  # "O(n²)" -> "(n²)"
    if has_early_exit:
        return f"varía entre Ω(1) y O{suffix}", "Ω(1)"
    return f"Θ{suffix}", f"Ω{suffix}"


_GROWABLE_METHODS = {"append", "add", "update", "insert", "extend"}


def _is_growing_call_python(node: ast.expr) -> bool:
    """¿Esta llamada hace crecer una colección (`.append(...)`, `.add(...)`, …)?"""
    return isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute) and node.func.attr in _GROWABLE_METHODS


def _grows_structure_python(stmt: ast.AST) -> bool:
    """¿Este statement crea/hace crecer una estructura auxiliar? Cubre
    `xs.append(...)` (llamada) y `d[key] = value` (asignación por subscript
    — el patrón típico para construir un dict incrementalmente)."""
    if isinstance(stmt, ast.Expr):
        return _is_growing_call_python(stmt.value)
    if isinstance(stmt, ast.Assign):
        return any(isinstance(t, ast.Subscript) for t in stmt.targets)
    if isinstance(stmt, ast.AugAssign):
        return isinstance(stmt.target, ast.Subscript)
    return False


def _aux_structure_depth_python(func: ast.FunctionDef) -> int:
    """Profundidad máxima de anidamiento de loop en la que ocurre una
    operación que hace crecer una estructura — separado de la profundidad de
    loop "para tiempo" de `_loop_analysis_python`, que cuenta iteración sin
    importar si algo se construye (un loop que solo acumula en un escalar es
    O(1) de espacio aunque sea O(n) de tiempo)."""
    LOOP_TYPES = (ast.For, ast.While)
    max_depth = [0]

    def walk(n: ast.AST, depth: int) -> None:
        for child in ast.iter_child_nodes(n):
            is_loop = isinstance(child, LOOP_TYPES)
            d = depth + 1 if is_loop else depth
            if d > 0 and _grows_structure_python(child):
                max_depth[0] = max(max_depth[0], d)
            walk(child, d)

    walk(func, 0)
    return max_depth[0]


def _comprehension_depth_python(node: ast.expr) -> int:
    """Profundidad de anidamiento de comprehensions (`[x for x in y]` cuenta
    1; `[[x for x in row] for row in m]` cuenta 2 porque la interna vive en
    el `elt` de la externa). No distingue múltiples generators en una sola
    comprehension — heurística deliberadamente conservadora."""
    if isinstance(node, ast.ListComp | ast.SetComp | ast.GeneratorExp):
        return 1 + _comprehension_depth_python(node.elt)
    if isinstance(node, ast.DictComp):
        return 1 + max(_comprehension_depth_python(node.key), _comprehension_depth_python(node.value))
    return 0


def _max_comprehension_depth_python(func: ast.FunctionDef) -> int:
    return max((_comprehension_depth_python(n) for n in ast.walk(func) if isinstance(n, ast.expr)), default=0)


def _infer_space_python(func: ast.FunctionDef, is_recursive: bool) -> tuple[str, str]:
    """Complejidad de espacio (Fase 13) — heurística AST paralela al motor de
    Big-O de tiempo arriba: mismo criterio general (forma del loop, forma de
    la recursión), pero mirando qué estructuras auxiliares se crean en vez de
    cuántas veces se itera."""
    structure_depth = max(_aux_structure_depth_python(func), _max_comprehension_depth_python(func))
    has_binary = is_recursive and _has_binary_split_python(func)

    if structure_depth >= 2:
        return (
            "O(n²)",
            "Una estructura auxiliar 2D crece con n² — una matriz construida dentro de loops "
            "anidados, o una comprehension anidada",
        )
    if structure_depth == 1:
        return (
            "O(n)",
            "Una lista/set/dict auxiliar crece con el input — aproximadamente un valor "
            "guardado por elemento procesado",
        )
    if is_recursive and has_binary:
        return (
            "O(log n)",
            "La recursión divide el problema a la mitad en cada llamada — el call stack "
            "nunca crece más de log n frames",
        )
    if is_recursive:
        return (
            "O(n)",
            "Recursión sin dividir el input — el call stack crece un frame por llamada, "
            "hasta n de profundidad (Python no optimiza tail calls)",
        )
    return (
        "O(1)",
        "Ninguna estructura auxiliar crece con n, y no hay recursión — espacio extra "
        "constante más allá del input mismo",
    )


def _infer_big_o_c(body: str) -> tuple[str, str]:
    """Heurística Big O para C/C++ basada en keywords del cuerpo."""
    loops = body.count(" for ") + body.count("\tfor ") + body.count(" while ") + body.count("\twhile ")
    has_binary = "/ 2" in body or ">> 1" in body or "mid" in body

    if loops == 0:
        return "O(1)", "sin loops"
    if loops == 1 and has_binary:
        return "O(log n)", "loop con división binaria"
    if loops == 1:
        return "O(n)", "un loop"
    if loops == 2:
        return "O(n²)", "loops anidados dobles"
    if loops >= 3:
        return "O(n³)", "loops anidados triples o más"
    return "O(n)", "caso base"


def _infer_big_o_js(body: str) -> tuple[str, str]:
    loops = len(re.findall(r"\b(for|while|forEach|map|filter|reduce)\b", body))
    has_binary = "/ 2" in body or ">> 1" in body or "Math.floor" in body
    nested = bool(re.search(r"(for|while|forEach|map)[^{]*\{[^}]*(for|while|forEach|map)", body, re.DOTALL))

    if loops == 0:
        return "O(1)", "sin loops"
    if loops == 1 and has_binary:
        return "O(log n)", "loop con división binaria"
    if loops == 1:
        return "O(n)", "un loop"
    if nested or loops >= 2:
        return "O(n²)", "loops anidados"
    return "O(n)", "caso base"


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


def _cyclomatic_js(body: str) -> int:
    # No contar "else if " aparte: "else if " ya contiene "if " como
    # substring, así que cada `else if` sumaba dos veces — "if " solo ya
    # cubre tanto un `if` normal como la parte "if" de un `else if`.
    cc = 1
    keywords = ["if ", "for ", "while ", "case ", "catch ", "&&", "||", "? "]
    for kw in keywords:
        cc += body.count(kw)
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


# ══════════════════════════════════════════════════════════════════════════════
#  SECURITY & TAINT (Fase 21 v1) — heurística de patrones, no un reemplazo de SAST
# ══════════════════════════════════════════════════════════════════════════════
#  Taint tracking de una sola pasada dentro de cada función: no sigue el flujo
#  de control real (no distingue ramas de if/else ni loops — una variable
#  asignada desde una fuente no confiable queda "tainted" para el resto de la
#  función) y no es interprocedural (no rastrea un valor pasado a otra función
#  definida en el mismo archivo — eso necesita el call graph del Dependency
#  Engine, Fase 18). Cada finding trae fuente + evidencia + confianza; nunca
#  se afirma "esto ES una vulnerabilidad", solo "este patrón lo amerita".

_TAINT_HTTP_ATTRS = {"args", "form", "values", "json", "GET", "POST", "COOKIES", "headers"}
_TAINT_DOTTED_SOURCES = {
    "os.getenv": "variable de entorno",
    "os.environ.get": "variable de entorno",
    "request.get_json": "solicitud HTTP",
}
_SQL_SINK_METHODS = {"execute", "executemany"}
_SHELL_CALL_DOTTED = {"os.system", "os.popen"}
_SUBPROCESS_SHELL_FUNCS = {"call", "run", "Popen", "check_call", "check_output"}
_CREDENTIAL_NAME_RE = re.compile(r"(password|passwd|pwd|secret|api_?key|access_?key|auth_?token|private_?key)", re.I)
_UNSAFE_DESERIALIZE_DOTTED = {"pickle.loads", "pickle.load", "marshal.loads"}
_PLACEHOLDER_VALUE_RE = re.compile(r"^(|changeme|xxx+|todo|your[-_]?\w*|<.*>|\{\{.*\}\}|\$\{.*\})$", re.I)


def _taint_source_kind(node: ast.expr) -> str | None:
    """Fuentes no confiables reconocidas: solicitud HTTP (`request.args/form/
    values/json/GET/POST/COOKIES/headers`), stdin (`input()`), argv
    (`sys.argv`), variables de entorno (`os.environ`/`os.getenv`). No cubre
    sockets/DB/lectura de archivo genérica — necesitarían contexto que un
    análisis por función no tiene."""
    if isinstance(node, ast.Call):
        if isinstance(node.func, ast.Name) and node.func.id in ("input", "raw_input"):
            return "stdin (input())"
        dotted = _node_name(node.func)
        if dotted in _TAINT_DOTTED_SOURCES:
            return _TAINT_DOTTED_SOURCES[dotted]
        if isinstance(node.func, ast.Attribute) and node.func.attr == "get":
            base = _node_name(node.func.value)
            if base == "os.environ":
                return "variable de entorno"
            if base.rsplit(".", 1)[-1] in _TAINT_HTTP_ATTRS:
                return "solicitud HTTP"
        return None
    if isinstance(node, ast.Subscript):
        base_name = _node_name(node.value)
        if base_name == "sys.argv":
            return "argumento de línea de comandos"
        if base_name == "os.environ":
            return "variable de entorno"
        if isinstance(node.value, ast.Attribute) and node.value.attr in _TAINT_HTTP_ATTRS:
            return "solicitud HTTP"
        return None
    if isinstance(node, ast.Attribute) and node.attr in _TAINT_HTTP_ATTRS:
        return "solicitud HTTP"
    return None


def _expr_taint_info(expr: ast.expr, tainted: dict[str, tuple[str, bool]]) -> tuple[str, bool] | None:
    """Retorna (fuente, construido_por_concatenación) si `expr` es o contiene
    un valor tainted. El flag "construido" es la señal real de riesgo para
    SQL/command injection: True si el taint llegó a través de concatenación
    (BinOp, incluye `%`), f-string, o `.format()` — es decir, se está armando
    un string a mano en vez de pasar el valor como parámetro separado."""
    direct = _taint_source_kind(expr)
    if direct:
        return (direct, False)
    if isinstance(expr, ast.Name) and expr.id in tainted:
        return tainted[expr.id]
    if isinstance(expr, ast.BinOp):
        found = _expr_taint_info(expr.left, tainted) or _expr_taint_info(expr.right, tainted)
        return (found[0], True) if found else None
    if isinstance(expr, ast.JoinedStr):  # f-string
        for value in expr.values:
            if isinstance(value, ast.FormattedValue):
                found = _expr_taint_info(value.value, tainted)
                if found:
                    return (found[0], True)
        return None
    if isinstance(expr, ast.Call) and isinstance(expr.func, ast.Attribute) and expr.func.attr == "format":
        for arg in list(expr.args) + [kw.value for kw in expr.keywords]:
            found = _expr_taint_info(arg, tainted)
            if found:
                return (found[0], True)
        return None
    return None


def _own_scope_nodes(func: ast.FunctionDef | ast.AsyncFunctionDef) -> list[ast.AST]:
    """Como `ast.walk(func)` pero sin descender a funciones/lambdas anidadas:
    esas tienen su propio scope (y su propia línea de tiempo — pueden
    llamarse después, varias veces, o nunca) y ya se analizan por separado
    cuando el loop principal de `_parse_python` las visita como su propio
    `FunctionDef`. Mezclarlas con el scope de `func` producía falsos
    positivos de taint cuando reusaban un nombre de variable."""
    nodes: list[ast.AST] = []
    stack = list(ast.iter_child_nodes(func))
    while stack:
        node = stack.pop()
        nodes.append(node)
        if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef | ast.Lambda):
            continue
        stack.extend(ast.iter_child_nodes(node))
    return nodes


def _taint_propagation_python(func: ast.FunctionDef) -> dict[str, tuple[str, bool]]:
    """Pasada única sobre las asignaciones de la función (scope propio, sin
    bajar a funciones anidadas — ver `_own_scope_nodes`), en orden de línea:
    si el lado derecho es o contiene un valor tainted, el nombre del lado
    izquierdo queda tainted con esa misma fuente; si NO lo es, se limpia
    cualquier taint previo de ese nombre — una reasignación a un valor seguro
    (`cmd = "ls -la"`) deja de estar tainted para el resto del análisis
    (aproximación lineal, no sigue ramas — documentado arriba)."""
    tainted: dict[str, tuple[str, bool]] = {}
    assigns = sorted((n for n in _own_scope_nodes(func) if isinstance(n, ast.Assign)), key=lambda n: n.lineno)
    for n in assigns:
        info = _expr_taint_info(n.value, tainted)
        for target in n.targets:
            if not isinstance(target, ast.Name):
                continue
            if info:
                tainted[target.id] = info
            else:
                tainted.pop(target.id, None)
    return tainted


def _check_sql_injection(call: ast.Call, tainted: dict[str, tuple[str, bool]]) -> dict | None:
    """CWE-89. Solo dispara si el query se arma por concatenación/f-string/
    `.format()` con un valor tainted — un query parametrizado real
    (`cursor.execute("...%s...", (val,))`) no lo dispara, porque `args[0]` ahí
    es un literal, no una expresión construida."""
    if not (isinstance(call.func, ast.Attribute) and call.func.attr in _SQL_SINK_METHODS):
        return None
    if not call.args:
        return None
    info = _expr_taint_info(call.args[0], tainted)
    if not info or not info[1]:
        return None
    source, _built = info
    base = _node_name(call.func.value)
    sink = f"{base}.{call.func.attr}(...)" if base != "?" else f"{call.func.attr}(...)"
    return {
        "cwe": "CWE-89",
        "category": "SQL Injection",
        "severity": "High",
        "confidence": "High",
        "source": source,
        "sink": sink,
        "line": call.lineno,
        "recommendation": "Usar query parametrizada (placeholders `?`/`%s` + tupla de parámetros) en vez de construir el SQL por concatenación/f-string.",
    }


def _check_command_injection(call: ast.Call, tainted: dict[str, tuple[str, bool]]) -> dict | None:
    """CWE-78. `os.system`/`os.popen` siempre corren en shell, así que
    cualquier taint alcanzando el comando alcanza — construido o no.
    `subprocess.*` solo dispara con `shell=True` explícito (sin eso, pasar
    una lista de argumentos ya es la forma segura y no ejecuta por shell)."""
    dotted = _node_name(call.func)
    is_os_shell = dotted in _SHELL_CALL_DOTTED
    is_subprocess_shell = (
        isinstance(call.func, ast.Attribute)
        and call.func.attr in _SUBPROCESS_SHELL_FUNCS
        and _node_name(call.func.value) == "subprocess"
        and any(
            kw.arg == "shell" and isinstance(kw.value, ast.Constant) and kw.value.value is True for kw in call.keywords
        )
    )
    if not (is_os_shell or is_subprocess_shell):
        return None
    if not call.args:
        return None
    info = _expr_taint_info(call.args[0], tainted)
    if not info:
        return None
    source, _built = info
    sink_name = dotted if dotted != "?" else (call.func.attr if isinstance(call.func, ast.Attribute) else "?")
    return {
        "cwe": "CWE-78",
        "category": "Command Injection",
        "severity": "High",
        "confidence": "High",
        "source": source,
        "sink": f"{sink_name}(...)",
        "line": call.lineno,
        "recommendation": "Pasar el comando como lista de argumentos (`subprocess.run([...])`, sin `shell=True`) en vez de un string construido con datos externos.",
    }


def _check_path_traversal(call: ast.Call, tainted: dict[str, tuple[str, bool]]) -> dict | None:
    """CWE-22. `open(path)` solo dispara si el path se arma por concatenación/
    f-string/`.format()` con un valor tainted — mismo criterio "construido"
    que SQL injection, así que `open(f"uploads/{name}")` dispara pero
    `open(config_path)` (una variable simple, no concatenada) no.
    `os.path.join(base, seg)` dispara con cualquier segmento tainted más allá
    del primero, construido o no — unir un `"../etc/passwd"` tainted crudo ya
    es el traversal, no necesita concatenación para ser peligroso."""
    if isinstance(call.func, ast.Name) and call.func.id == "open":
        if not call.args:
            return None
        info = _expr_taint_info(call.args[0], tainted)
        if not info or not info[1]:
            return None
        source, _built = info
        return {
            "cwe": "CWE-22",
            "category": "Path Traversal",
            "severity": "High",
            "confidence": "High",
            "source": source,
            "sink": "open(...)",
            "line": call.lineno,
            "recommendation": "Validar/normalizar el path contra un directorio base permitido (ej. `Path(base).joinpath(name).resolve()` y chequear que siga bajo `base`) antes de abrirlo.",
        }
    if _node_name(call.func) == "os.path.join" and len(call.args) > 1:
        for arg in call.args[1:]:
            info = _expr_taint_info(arg, tainted)
            if info:
                source, _built = info
                return {
                    "cwe": "CWE-22",
                    "category": "Path Traversal",
                    "severity": "High",
                    "confidence": "High",
                    "source": source,
                    "sink": "os.path.join(...)",
                    "line": call.lineno,
                    "recommendation": "Validar/normalizar el path unido contra un directorio base permitido antes de usarlo — `os.path.join` no elimina segmentos `..`.",
                }
    return None


def _check_insecure_deserialization(call: ast.Call, tainted: dict[str, tuple[str, bool]]) -> dict | None:
    """CWE-502. `pickle.loads`/`pickle.load`/`marshal.loads` son inseguras por
    construcción — pueden ejecutar código arbitrario al deserializar, sin
    importar si el input es provably tainted (a diferencia de SQL/command
    injection, no existe una "forma segura" de estas llamadas). `yaml.load`
    solo es insegura sin un `Loader=yaml.SafeLoader`/`CSafeLoader` explícito.
    Confianza Alta si el argumento traza a una fuente de taint conocida,
    Media si no — usar estas funciones ya es un riesgo real, que además
    reciban input tainted es peor."""
    dotted = _node_name(call.func)
    is_pickle_marshal = dotted in _UNSAFE_DESERIALIZE_DOTTED
    is_unsafe_yaml = dotted == "yaml.load" and not any(
        kw.arg == "Loader" and isinstance(kw.value, ast.Attribute) and "SafeLoader" in kw.value.attr
        for kw in call.keywords
    )
    if not (is_pickle_marshal or is_unsafe_yaml):
        return None
    arg_info = _expr_taint_info(call.args[0], tainted) if call.args else None
    if arg_info:
        confidence, source = "High", arg_info[0]
    else:
        confidence, source = "Medium", "llamada de deserialización sin validar"
    recommendation = (
        "Usar `yaml.safe_load()` (o `Loader=yaml.SafeLoader`) en vez del `yaml.load` por defecto, que puede "
        "construir objetos Python arbitrarios."
        if is_unsafe_yaml
        else "Evitar deserializar datos no confiables con `pickle`/`marshal` — usar un formato seguro (JSON) o "
        "un serializador validado por schema."
    )
    return {
        "cwe": "CWE-502",
        "category": "Insecure Deserialization",
        "severity": "High",
        "confidence": confidence,
        "source": source,
        "sink": f"{dotted}(...)",
        "line": call.lineno,
        "recommendation": recommendation,
    }


def _security_findings_python(func: ast.FunctionDef) -> list[dict]:
    """Taint tracking + catálogo de sinks conocidos, dentro de una sola
    función (scope propio, sin bajar a funciones anidadas — esas se analizan
    por separado cuando el loop principal las visita). Cada finding incluye
    `function` con el nombre de la función donde se detectó. A diferencia de
    v1 (solo SQLi/command injection, ambos requieren taint), ya no corta
    temprano si no hay taint — la deserialización insegura (CWE-502) dispara
    aunque el argumento no sea provably tainted."""
    tainted = _taint_propagation_python(func)
    findings: list[dict] = []
    calls = sorted((c for c in _own_scope_nodes(func) if isinstance(c, ast.Call)), key=lambda c: c.lineno)
    for call in calls:
        finding = (
            _check_sql_injection(call, tainted)
            or _check_command_injection(call, tainted)
            or _check_path_traversal(call, tainted)
            or _check_insecure_deserialization(call, tainted)
        )
        if finding:
            finding["function"] = func.name
            findings.append(finding)
    return findings


def _hardcoded_credentials_python(tree: ast.Module) -> list[dict]:
    """CWE-798, a nivel de archivo (no por función, como el resto de la
    seguridad — una credencial hardcodeada suele vivir en una constante de
    módulo). Dos señales requeridas: el nombre de variable sugiere una
    credencial Y el valor es un string literal no vacío que no parece un
    placeholder (`""`, `"changeme"`, `"<your-key>"`, `"${VAR}"`). Heurística
    de nombre+forma, mismo estilo que el resto del CS Engine — no valida si
    el string realmente funciona como credencial."""
    findings: list[dict] = []
    for n in ast.walk(tree):
        if isinstance(n, ast.Assign):
            targets, value = n.targets, n.value
        elif isinstance(n, ast.AnnAssign) and n.value is not None:
            targets, value = [n.target], n.value
        else:
            continue
        if not (isinstance(value, ast.Constant) and isinstance(value.value, str)):
            continue
        if _PLACEHOLDER_VALUE_RE.match(value.value.strip()):
            continue
        for target in targets:
            if isinstance(target, ast.Name) and _CREDENTIAL_NAME_RE.search(target.id):
                findings.append(
                    {
                        "cwe": "CWE-798",
                        "category": "Hardcoded Credentials",
                        "severity": "Medium",
                        "confidence": "Medium",
                        "source": f"literal en el código (`{target.id}`)",
                        "sink": None,
                        "line": n.lineno,
                        "function": None,
                        "recommendation": "Mover el valor a una variable de entorno o a un gestor de secretos — no dejarlo como literal en el código fuente.",
                    }
                )
    return findings


# ══════════════════════════════════════════════════════════════════════════════
#  STRUCTURAL SMELLS (Fase 22, segundo ítem) — heurísticas de forma de AST
# ══════════════════════════════════════════════════════════════════════════════
#  Mismo estilo que el resto del CS Engine: cada smell trae su umbral y su
#  razonamiento en el mensaje, nunca una etiqueta sola. Umbrales
#  convencionales de la literatura de code smells (Fowler/Martin: long
#  method ~50 LOC, large class ~15 métodos, long parameter list ~5, god
#  object combina tamaño con cantidad de estado) — no configurables todavía.
#
#  Deliberadamente NO incluye "duplicated logic" (comparación de forma de
#  AST entre funciones) — necesita un esquema de normalización/hashing que
#  todavía no existe acá; queda para una porción siguiente, no silenciado.

_LONG_FUNCTION_LOC = 50
_EXCESSIVE_PARAMS = 5
_DEEP_NESTING_DEPTH = 4
_LARGE_CLASS_METHODS = 15
_LARGE_CLASS_LOC = 300
_GOD_OBJECT_METHODS = 20
_GOD_OBJECT_ATTRS = 10


def _check_long_function(name: str, line: int, loc: int) -> dict | None:
    if loc <= _LONG_FUNCTION_LOC:
        return None
    return {
        "kind": "long_function",
        "name": name,
        "line": line,
        "message": f"{loc} líneas (> {_LONG_FUNCTION_LOC}) — considerar dividir en funciones más chicas y enfocadas",
    }


def _check_excessive_parameters(name: str, line: int, arg_count: int) -> dict | None:
    if arg_count <= _EXCESSIVE_PARAMS:
        return None
    return {
        "kind": "excessive_parameters",
        "name": name,
        "line": line,
        "message": f"{arg_count} parámetros (> {_EXCESSIVE_PARAMS}) — considerar agrupar parámetros relacionados en un objeto/dataclass",
    }


def _max_nesting_depth_python(body: ast.FunctionDef) -> int:
    """Profundidad máxima de anidamiento de CUALQUIER bloque (if/for/while/
    with/try) — a diferencia de `_loop_analysis_python`, que solo cuenta
    loops para el Big-O de tiempo, acá un `if` anidado en otro `if` anidado
    en un `try` también cuenta: no afecta el tiempo de ejecución, pero sí
    qué tan difícil es leer la función."""
    BLOCK_TYPES = (ast.If, ast.For, ast.AsyncFor, ast.While, ast.With, ast.AsyncWith, ast.Try)
    max_depth = [0]

    def walk(n: ast.AST, depth: int) -> None:
        for child in ast.iter_child_nodes(n):
            is_block = isinstance(child, BLOCK_TYPES)
            d = depth + 1 if is_block else depth
            max_depth[0] = max(max_depth[0], d)
            walk(child, d)

    walk(body, 0)
    return max_depth[0]


def _check_deep_nesting(name: str, line: int, func: ast.FunctionDef) -> dict | None:
    depth = _max_nesting_depth_python(func)
    if depth <= _DEEP_NESTING_DEPTH:
        return None
    return {
        "kind": "deep_nesting",
        "name": name,
        "line": line,
        "message": f"{depth} niveles de bloques anidados (> {_DEEP_NESTING_DEPTH}) — considerar extraer bloques internos a funciones auxiliares o usar early returns",
    }


def _check_large_class(name: str, line: int, method_count: int, loc: int) -> dict | None:
    if method_count <= _LARGE_CLASS_METHODS and loc <= _LARGE_CLASS_LOC:
        return None
    return {
        "kind": "large_class",
        "name": name,
        "line": line,
        "message": f"{method_count} métodos, {loc} líneas (umbrales: {_LARGE_CLASS_METHODS} métodos / {_LARGE_CLASS_LOC} líneas) — considerar dividir responsabilidades en clases más chicas",
    }


def _count_self_attributes_python(class_node: ast.ClassDef) -> int:
    """Cuenta atributos únicos asignados vía `self.X = ...` en cualquier
    método de la clase — proxy heurístico de cuánto estado mantiene la
    clase. No distingue atributos "reales" de temporales reasignados en
    cada llamada, ni sigue atributos heredados de una clase base."""
    attrs: set[str] = set()
    for item in class_node.body:
        if isinstance(item, ast.FunctionDef | ast.AsyncFunctionDef):
            for n in _own_scope_nodes(item):
                if isinstance(n, ast.Assign):
                    for t in n.targets:
                        if isinstance(t, ast.Attribute) and isinstance(t.value, ast.Name) and t.value.id == "self":
                            attrs.add(t.attr)
    return len(attrs)


def _check_god_object(name: str, line: int, method_count: int, attribute_count: int) -> dict | None:
    if method_count < _GOD_OBJECT_METHODS or attribute_count < _GOD_OBJECT_ATTRS:
        return None
    return {
        "kind": "god_object",
        "name": name,
        "line": line,
        "message": f"{method_count} métodos y {attribute_count} atributos (umbrales: {_GOD_OBJECT_METHODS}/{_GOD_OBJECT_ATTRS}) — esta clase probablemente hace demasiado; considerar dividirla por responsabilidad",
    }


# ══════════════════════════════════════════════════════════════════════════════
#  CIRCULAR IMPORTS (Python)
# ══════════════════════════════════════════════════════════════════════════════


def find_cycles_capped(graph: Any, max_cycles: int = 20) -> list[list[str]]:
    """Enumera ciclos simples de un grafo dirigido, cortando apenas se
    encuentran `max_cycles`.

    `nx.simple_cycles()` es un generador, pero convertirlo a `list()` de una
    fuerza enumerar TODOS los ciclos posibles antes de poder usar ninguno —
    en un grafo denso esa cantidad crece exponencial (un proyecto de ~150
    archivos con solo 3% de densidad de imports ya puede tener cientos de
    miles de ciclos simples). Como de cualquier forma solo se muestran los
    primeros, cortar la iteración temprano evita colgar el servidor sin
    perder nada útil para quien lo usa.
    """
    if not HAS_NX:
        return []
    cycles: list[list[str]] = []
    try:
        for cycle in nx.simple_cycles(graph):
            cycles.append(cycle)
            if len(cycles) >= max_cycles:
                break
    except Exception:
        pass
    return cycles


def _detect_circular_imports(imports: list[dict]) -> list[str]:
    """Detecta posibles ciclos entre módulos importados."""
    if not HAS_NX:
        return []
    G = nx.DiGraph()
    for imp in imports:
        mod = imp["module"]
        parts = mod.split(".")
        if len(parts) >= 2:
            G.add_edge(parts[0], parts[-1])
    cycles = find_cycles_capped(G, max_cycles=5)
    return [" → ".join(c) for c in cycles]


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


def _wasm_hints_c(functions: list[dict], content: str) -> list[dict]:
    hints: list[dict] = []
    for func in functions:
        if func["big_o"] in _WASM_BIGO_HOT or func["complexity"] >= _WASM_CC_THRESHOLD:
            hints.append(
                {
                    "function": func["name"],
                    "line": func["line"],
                    "priority": 3,
                    "reasons": [f"Hot path C — {func['big_o']}, CC={func['complexity']}"],
                    "recommendation": "Compilar con Emscripten: emcc -O3 -s WASM=1",
                    "estimated_speedup": "2-10x vs JavaScript",
                }
            )
    return hints


def _wasm_hints_js(functions: list[dict], content: str) -> list[dict]:
    hints: list[dict] = []
    for func in functions:
        if func["big_o"] in _WASM_BIGO_HOT:
            hints.append(
                {
                    "function": func["name"],
                    "line": func["line"],
                    "priority": 3,
                    "reasons": [f"Hot path JS — {func['big_o']}"],
                    "recommendation": "Considera mover esta función a un módulo Rust/C++ compilado a WASM",
                    "estimated_speedup": "3-20x para operaciones numéricas",
                }
            )
    return hints


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
        "summary": {},
    }
