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
import symtable
from pathlib import Path
from typing import Any

# ── tree-sitter (C/C++) ───────────────────────────────────────────────────────
try:
    from tree_sitter import Language, Parser as TSParser
    import tree_sitter_c   as tsc
    import tree_sitter_cpp as tscpp
    _C_LANG   = Language(tsc.language())
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
        return {"filename": filename, "language": ext, "error": str(e),
                "functions": [], "classes": [], "imports": [], "exports": [],
                "complexity": [], "big_o": [], "dead_code": [], "wasm_hints": []}


# ══════════════════════════════════════════════════════════════════════════════
#  PYTHON PARSER  (ast + symtable)
# ══════════════════════════════════════════════════════════════════════════════

def _parse_python(filename: str, content: str) -> dict:
    tree  = ast.parse(content, filename=filename)
    lines = content.splitlines()

    functions: list[dict] = []
    classes:   list[dict] = []
    imports:   list[dict] = []

    # ── Imports ───────────────────────────────────────────────────────────────
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                imports.append({
                    "module": alias.name,
                    "alias":  alias.asname,
                    "type":   "import",
                    "line":   node.lineno,
                })
        elif isinstance(node, ast.ImportFrom):
            mod = node.module or ""
            for alias in node.names:
                imports.append({
                    "module": mod,
                    "name":   alias.name,
                    "alias":  alias.asname,
                    "type":   "from_import",
                    "line":   node.lineno,
                })

    # ── Funciones y clases ────────────────────────────────────────────────────
    for node in ast.walk(tree):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            end   = _end_line(node)
            loc   = end - node.lineno + 1
            cc    = _cyclomatic_python(node)
            bigo, bigo_reason = _infer_big_o_python(node)
            decorators = [_decorator_name(d) for d in node.decorator_list]

            functions.append({
                "name":       node.name,
                "line":       node.lineno,
                "end_line":   end,
                "loc":        loc,
                "args":       [a.arg for a in node.args.args],
                "is_async":   isinstance(node, ast.AsyncFunctionDef),
                "decorators": decorators,
                "docstring":  ast.get_docstring(node),
                "complexity": cc,
                "big_o":      bigo,
                "big_o_reason": bigo_reason,
                "calls":      _extract_calls_python(node),
                "returns_annotated": node.returns is not None,
            })

        elif isinstance(node, ast.ClassDef):
            methods = []
            for item in node.body:
                if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    methods.append({
                        "name": item.name,
                        "line": item.lineno,
                        "args": [a.arg for a in item.args.args],
                        "is_async": isinstance(item, ast.AsyncFunctionDef),
                    })
            bases = [_node_name(b) for b in node.bases]
            classes.append({
                "name":       node.name,
                "line":       node.lineno,
                "bases":      bases,
                "methods":    methods,
                "decorators": [_decorator_name(d) for d in node.decorator_list],
                "docstring":  ast.get_docstring(node),
            })

    # ── Imports no usados (heurística) ───────────────────────────────────────
    used_names = {n.id for n in ast.walk(tree) if isinstance(n, ast.Name)}
    used_attrs = {n.attr for n in ast.walk(tree) if isinstance(n, ast.Attribute)}
    dead_imports: list[dict] = []
    for imp in imports:
        alias = imp.get("alias") or imp.get("name") or imp["module"].split(".")[0]
        if alias and alias != "*":
            if alias not in used_names and alias not in used_attrs:
                dead_imports.append({
                    "type":   "unused_import",
                    "name":   alias,
                    "module": imp["module"],
                    "line":   imp["line"],
                })

    # ── WASM / hot path hints ─────────────────────────────────────────────────
    wasm_hints = _wasm_hints_python(functions, content)

    # ── Call graph (dentro del mismo archivo) ─────────────────────────────────
    call_graph = _build_call_graph(functions)

    # ── Dependencias circulares entre imports ─────────────────────────────────
    circular = _detect_circular_imports(imports)

    return {
        "filename":    filename,
        "language":    "python",
        "functions":   functions,
        "classes":     classes,
        "imports":     imports,
        "exports":     [],           # Python no tiene exports explícitos
        "dead_code":   dead_imports,
        "call_graph":  call_graph,
        "circular_deps": circular,
        "wasm_hints":  wasm_hints,
        "summary": {
            "total_functions": len(functions),
            "total_classes":   len(classes),
            "total_imports":   len(imports),
            "unused_imports":  len(dead_imports),
            "avg_complexity":  round(
                sum(f["complexity"] for f in functions) / len(functions), 2
            ) if functions else 0,
            "max_loc_function": max((f["loc"] for f in functions), default=0),
        },
    }


# ══════════════════════════════════════════════════════════════════════════════
#  C PARSER  (tree-sitter)
# ══════════════════════════════════════════════════════════════════════════════

def _parse_c(filename: str, content: str) -> dict:
    if not HAS_TREESITTER:
        return _unsupported(filename, ".c", reason="tree-sitter no instalado")

    parser = TSParser(_C_LANG)
    tree   = parser.parse(content.encode())
    root   = tree.root_node

    functions = _ts_extract_functions(root, content, "c")
    includes  = _ts_extract_includes(root, content)
    structs   = _ts_extract_structs(root, content)
    macros    = _ts_extract_macros(root, content)
    wasm_hints = _wasm_hints_c(functions, content)

    return {
        "filename":  filename,
        "language":  "c",
        "functions": functions,
        "classes":   structs,
        "imports":   includes,
        "exports":   [],
        "dead_code": [],
        "macros":    macros,
        "call_graph": _build_call_graph(functions),
        "wasm_hints": wasm_hints,
        "summary": {
            "total_functions": len(functions),
            "total_structs":   len(structs),
            "total_includes":  len(includes),
            "total_macros":    len(macros),
        },
    }


def _parse_cpp(filename: str, content: str) -> dict:
    if not HAS_TREESITTER:
        return _unsupported(filename, ".cpp", reason="tree-sitter no instalado")

    parser = TSParser(_CPP_LANG)
    tree   = parser.parse(content.encode())
    root   = tree.root_node

    functions = _ts_extract_functions(root, content, "cpp")
    includes  = _ts_extract_includes(root, content)
    classes   = _ts_extract_classes_cpp(root, content)
    macros    = _ts_extract_macros(root, content)
    wasm_hints = _wasm_hints_c(functions, content)

    return {
        "filename":  filename,
        "language":  "cpp",
        "functions": functions,
        "classes":   classes,
        "imports":   includes,
        "exports":   [],
        "dead_code": [],
        "macros":    macros,
        "call_graph": _build_call_graph(functions),
        "wasm_hints": wasm_hints,
        "summary": {
            "total_functions": len(functions),
            "total_classes":   len(classes),
            "total_includes":  len(includes),
        },
    }


# ── tree-sitter helpers ───────────────────────────────────────────────────────

def _ts_extract_functions(root, content: str, lang: str) -> list[dict]:
    functions = []
    lines     = content.splitlines()

    def walk(node):
        if node.type == "function_definition":
            name  = _ts_func_name(node)
            start = node.start_point[0] + 1
            end   = node.end_point[0]   + 1
            loc   = end - start + 1
            body_src = "\n".join(lines[start-1:end])

            # Complejidad ciclomática básica por conteo de branch keywords
            cc = 1 + body_src.count(" if ") + body_src.count(" else if ") + \
                 body_src.count(" for ") + body_src.count(" while ") + \
                 body_src.count(" case ") + body_src.count(" && ") + \
                 body_src.count(" || ")

            bigo, reason = _infer_big_o_c(body_src)

            functions.append({
                "name":       name,
                "line":       start,
                "end_line":   end,
                "loc":        loc,
                "complexity": cc,
                "big_o":      bigo,
                "big_o_reason": reason,
                "calls":      _extract_calls_c(node),
            })
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
            path_node = next((c for c in node.children if c.type in
                              ("string_literal", "system_lib_string")), None)
            if path_node:
                raw = path_node.text.decode("utf-8", errors="replace").strip('"<>')
                includes.append({
                    "module": raw,
                    "type":   "include",
                    "line":   node.start_point[0] + 1,
                })
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
                structs.append({
                    "name": name_node.text.decode("utf-8", errors="replace"),
                    "line": node.start_point[0] + 1,
                    "kind": node.type.replace("_specifier",""),
                })
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
                classes.append({
                    "name": name_node.text.decode("utf-8", errors="replace"),
                    "line": node.start_point[0] + 1,
                    "kind": "class",
                })
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
                macros.append({
                    "name": name_node.text.decode("utf-8", errors="replace"),
                    "line": node.start_point[0] + 1,
                })
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
    r'(?:export\s+)?(?:async\s+)?function\s+(\w+)\s*\(([^)]*)\)'
    r'|const\s+(\w+)\s*=\s*(?:async\s*)?\(([^)]*)\)\s*=>'
    r'|(?:export\s+)?(?:async\s+)?function\s*\(([^)]*)\)',
    re.MULTILINE,
)
_JS_CLASS_RE  = re.compile(r'(?:export\s+)?class\s+(\w+)(?:\s+extends\s+(\w+))?', re.MULTILINE)
_JS_IMPORT_RE = re.compile(r"import\s+(?:{[^}]+}|[\w*]+)?\s*(?:,\s*{[^}]+})?\s*from\s+['\"]([^'\"]+)['\"]", re.MULTILINE)
_JS_EXPORT_RE = re.compile(r'export\s+(?:default\s+)?(?:function|class|const|let|var)\s+(\w+)', re.MULTILINE)
_TS_IFACE_RE  = re.compile(r'(?:export\s+)?interface\s+(\w+)', re.MULTILINE)
_TS_TYPE_RE   = re.compile(r'(?:export\s+)?type\s+(\w+)\s*=', re.MULTILINE)


def _parse_js(filename: str, content: str) -> dict:
    return _parse_js_ts(filename, content, "javascript")


def _parse_ts(filename: str, content: str) -> dict:
    return _parse_js_ts(filename, content, "typescript")


def _parse_js_ts(filename: str, content: str, lang: str) -> dict:
    lines    = content.splitlines()
    n_lines  = len(lines)

    # ── Funciones ─────────────────────────────────────────────────────────────
    functions: list[dict] = []
    seen_funcs: set[str]  = set()

    for m in _JS_FUNC_RE.finditer(content):
        name = m.group(1) or m.group(3) or "<anonymous>"
        if name in seen_funcs:
            continue
        seen_funcs.add(name)
        line_no = content[:m.start()].count("\n") + 1
        # Calcular LOC aproximado buscando la llave de cierre
        loc  = _estimate_js_func_loc(content, m.start())
        body = "\n".join(lines[line_no-1: line_no-1+loc])
        cc   = _cyclomatic_js(body)
        bigo, reason = _infer_big_o_js(body)

        functions.append({
            "name":       name,
            "line":       line_no,
            "end_line":   min(line_no + loc, n_lines),
            "loc":        loc,
            "complexity": cc,
            "big_o":      bigo,
            "big_o_reason": reason,
            "calls":      _extract_calls_js(body),
            "is_async":   "async" in content[max(0,m.start()-10):m.start()+5],
        })

    # ── Clases ────────────────────────────────────────────────────────────────
    classes = []
    for m in _JS_CLASS_RE.finditer(content):
        line_no = content[:m.start()].count("\n") + 1
        classes.append({
            "name":    m.group(1),
            "extends": m.group(2),
            "line":    line_no,
        })

    # ── Imports ───────────────────────────────────────────────────────────────
    imports = []
    for m in _JS_IMPORT_RE.finditer(content):
        line_no = content[:m.start()].count("\n") + 1
        imports.append({
            "module": m.group(1),
            "type":   "esm_import",
            "line":   line_no,
        })

    # ── Exports ───────────────────────────────────────────────────────────────
    exports = []
    for m in _JS_EXPORT_RE.finditer(content):
        line_no = content[:m.start()].count("\n") + 1
        exports.append({"name": m.group(1), "line": line_no})

    # ── TypeScript extras ─────────────────────────────────────────────────────
    interfaces, types_list = [], []
    if lang == "typescript":
        for m in _TS_IFACE_RE.finditer(content):
            line_no = content[:m.start()].count("\n") + 1
            interfaces.append({"name": m.group(1), "line": line_no})
        for m in _TS_TYPE_RE.finditer(content):
            line_no = content[:m.start()].count("\n") + 1
            types_list.append({"name": m.group(1), "line": line_no})

    # ── Imports no usados ─────────────────────────────────────────────────────
    dead_imports = _dead_js_imports(imports, content)

    # ── WASM hints ────────────────────────────────────────────────────────────
    wasm_hints = _wasm_hints_js(functions, content)

    return {
        "filename":   filename,
        "language":   lang,
        "functions":  functions,
        "classes":    classes,
        "imports":    imports,
        "exports":    exports,
        "interfaces": interfaces,
        "types":      types_list,
        "dead_code":  dead_imports,
        "call_graph": _build_call_graph(functions),
        "wasm_hints": wasm_hints,
        "summary": {
            "total_functions":  len(functions),
            "total_classes":    len(classes),
            "total_imports":    len(imports),
            "total_exports":    len(exports),
            "total_interfaces": len(interfaces),
            "unused_imports":   len(dead_imports),
            "avg_complexity":   round(
                sum(f["complexity"] for f in functions) / len(functions), 2
            ) if functions else 0,
        },
    }


def _estimate_js_func_loc(content: str, start: int) -> int:
    """Estima líneas de una función JS buscando la llave de cierre balanceada."""
    depth, i, max_loc = 0, start, 0
    in_func = False
    while i < len(content) and max_loc < 500:
        ch = content[i]
        if ch == "{":
            depth += 1
            in_func = True
        elif ch == "}" and in_func:
            depth -= 1
            if depth == 0:
                return content[start:i+1].count("\n") + 1
        elif ch == "\n":
            max_loc += 1
        i += 1
    return max(1, max_loc)


def _extract_calls_js(body: str) -> list[str]:
    return list(set(re.findall(r'(\w+)\s*\(', body)))


def _dead_js_imports(imports: list[dict], content: str) -> list[dict]:
    dead = []
    for imp in imports:
        mod = imp["module"].split("/")[-1].replace("-","_").replace(".","_")
        # Buscar si el módulo o algo que de él se importa está en el contenido
        # Heurística simple: buscar el nombre base del módulo
        base = re.sub(r'[^a-zA-Z0-9_]','', mod)
        if base and len(base) > 1 and content.count(base) <= 1:
            dead.append({
                "type":   "possibly_unused_import",
                "module": imp["module"],
                "line":   imp["line"],
            })
    return dead


# ══════════════════════════════════════════════════════════════════════════════
#  BIG O HEURÍSTICA
# ══════════════════════════════════════════════════════════════════════════════

def _count_loop_depth_python(node: ast.AST) -> int:
    """Profundidad máxima de loops anidados en un nodo Python AST."""
    LOOP_TYPES = (ast.For, ast.While)
    max_depth  = [0]

    def walk(n, depth):
        for child in ast.iter_child_nodes(n):
            d = depth + 1 if isinstance(child, LOOP_TYPES) else depth
            max_depth[0] = max(max_depth[0], d)
            walk(child, d)

    walk(node, 0)
    return max_depth[0]


def _has_binary_split_python(node: ast.AST) -> bool:
    for n in ast.walk(node):
        if isinstance(n, ast.BinOp):
            if isinstance(n.op, ast.FloorDiv) and isinstance(n.right, ast.Constant) and n.right.value == 2:
                return True
            if isinstance(n.op, ast.RShift)   and isinstance(n.right, ast.Constant) and n.right.value == 1:
                return True
    return False


def _has_recursion_python(func: ast.FunctionDef) -> bool:
    for n in ast.walk(func):
        if isinstance(n, ast.Call) and isinstance(n.func, ast.Name) and n.func.id == func.name:
            return True
    return False


def _infer_big_o_python(func: ast.FunctionDef) -> tuple[str, str]:
    depth     = _count_loop_depth_python(func)
    recursive = _has_recursion_python(func)
    binary    = _has_binary_split_python(func)

    if depth == 0 and not recursive:
        return "O(1)", "sin loops ni recursión"
    if depth == 1 and binary:
        return "O(log n)", "loop con división binaria"
    if depth == 1 and recursive:
        return "O(n log n)", "loop + recursión"
    if depth == 1:
        return "O(n)", "un loop"
    if depth == 2:
        return "O(n²)", "loops anidados dobles"
    if depth == 3:
        return "O(n³)", "loops anidados triples"
    if depth >= 4:
        return f"O(n^{depth})", f"{depth} loops anidados"
    if recursive and not binary:
        return "O(2^n)", "recursión sin división"
    return "O(n)", "caso base"


def _infer_big_o_c(body: str) -> tuple[str, str]:
    """Heurística Big O para C/C++ basada en keywords del cuerpo."""
    loops = body.count(" for ") + body.count("\tfor ") + \
            body.count(" while ") + body.count("\twhile ")
    has_binary = "/ 2" in body or ">> 1" in body or "mid" in body
    has_recurse = False  # Simplificado para C

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
    loops = len(re.findall(r'\b(for|while|forEach|map|filter|reduce)\b', body))
    has_binary = "/ 2" in body or ">> 1" in body or "Math.floor" in body
    nested = bool(re.search(
        r'(for|while|forEach|map)[^{]*\{[^}]*(for|while|forEach|map)',
        body, re.DOTALL
    ))

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
        if isinstance(node, (ast.If, ast.While, ast.For, ast.ExceptHandler,
                              ast.With, ast.Assert, ast.comprehension)):
            cc += 1
        elif isinstance(node, ast.BoolOp):
            cc += len(node.values) - 1
    return cc


def _cyclomatic_js(body: str) -> int:
    cc = 1
    keywords = ["if ", "else if ", "for ", "while ", "case ", "catch ",
                "&&", "||", "? "]
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
    seen:  set[str]   = set()

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
#  CIRCULAR IMPORTS (Python)
# ══════════════════════════════════════════════════════════════════════════════

def _detect_circular_imports(imports: list[dict]) -> list[str]:
    """Detecta posibles ciclos entre módulos importados."""
    if not HAS_NX:
        return []
    G = nx.DiGraph()
    for imp in imports:
        mod  = imp["module"]
        parts = mod.split(".")
        if len(parts) >= 2:
            G.add_edge(parts[0], parts[-1])
    try:
        cycles = list(nx.simple_cycles(G))
        return [" → ".join(c) for c in cycles[:5]]
    except Exception:
        return []


# ══════════════════════════════════════════════════════════════════════════════
#  WASM / PERFORMANCE HINTS
# ══════════════════════════════════════════════════════════════════════════════

# Umbrales para considerar un hot path
_WASM_CC_THRESHOLD  = 5    # Complejidad ciclomática
_WASM_LOC_THRESHOLD = 30   # Líneas de código
_WASM_BIGO_HOT      = {"O(n²)", "O(n³)", "O(2^n)"}  # Big O que se benefician de WASM


def _wasm_hints_python(functions: list[dict], content: str) -> list[dict]:
    hints: list[dict] = []

    # Detectar uso actual de Cython o ctypes
    has_cython  = bool(re.search(r'\bcimport\b|\bcdef\b|\bcpdef\b', content))
    has_ctypes  = "ctypes" in content
    has_cffi    = "cffi" in content

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
        is_numeric = any(kw in func["name"].lower() for kw in
                         ["sort","search","compute","calc","matrix","multiply","fft",
                          "transform","encode","decode","hash","compress","convolve"])
        if is_numeric:
            reasons.append("Nombre sugiere operación numérica intensiva")
            priority += 2

        if reasons:
            rec = _wasm_recommendation_python(func, has_cython)
            hints.append({
                "function":       func["name"],
                "line":           func["line"],
                "priority":       priority,
                "reasons":        reasons,
                "recommendation": rec,
                "estimated_speedup": _estimate_speedup(func),
            })

    # Detección de módulos .wasm en el contenido
    if "import.wasm" in content or ".wasm" in content:
        hints.append({
            "function": "<module>",
            "line":     1,
            "priority": 5,
            "reasons":  ["Archivo usa módulos .wasm directamente"],
            "recommendation": "Asegúrate de que los bindings WASM están tipados correctamente",
            "estimated_speedup": "N/A",
        })

    return sorted(hints, key=lambda x: -x["priority"])


def _wasm_hints_c(functions: list[dict], content: str) -> list[dict]:
    hints: list[dict] = []
    for func in functions:
        if func["big_o"] in _WASM_BIGO_HOT or func["complexity"] >= _WASM_CC_THRESHOLD:
            hints.append({
                "function":  func["name"],
                "line":      func["line"],
                "priority":  3,
                "reasons":   [f"Hot path C — {func['big_o']}, CC={func['complexity']}"],
                "recommendation": "Compilar con Emscripten: emcc -O3 -s WASM=1",
                "estimated_speedup": "2-10x vs JavaScript",
            })
    return hints


def _wasm_hints_js(functions: list[dict], content: str) -> list[dict]:
    hints: list[dict] = []
    for func in functions:
        if func["big_o"] in _WASM_BIGO_HOT:
            hints.append({
                "function":  func["name"],
                "line":      func["line"],
                "priority":  3,
                "reasons":   [f"Hot path JS — {func['big_o']}"],
                "recommendation": "Considera mover esta función a un módulo Rust/C++ compilado a WASM",
                "estimated_speedup": "3-20x para operaciones numéricas",
            })
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
    cc   = func["complexity"]
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
    if isinstance(node, ast.Name):      return node.id
    if isinstance(node, ast.Attribute): return f"{_node_name(node.value)}.{node.attr}"
    if isinstance(node, ast.Call):      return _decorator_name(node.func)
    return "?"


def _node_name(node: ast.expr) -> str:
    if isinstance(node, ast.Name):      return node.id
    if isinstance(node, ast.Attribute): return f"{_node_name(node.value)}.{node.attr}"
    return "?"


def _unsupported(filename: str, ext: str, reason: str = "") -> dict:
    return {
        "filename": filename, "language": ext,
        "error": reason or f"Extensión '{ext}' no soportada",
        "functions": [], "classes": [], "imports": [], "exports": [],
        "dead_code": [], "call_graph": [], "wasm_hints": [],
        "summary": {},
    }