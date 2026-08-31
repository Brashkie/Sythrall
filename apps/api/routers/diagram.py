"""
Router: Diagram
Migración exacta de /analyze/diagram del app.py Flask v3.0.
Preserva: _py_flowchart_fallback, _py_callgraph_fallback, _py_classes_fallback,
_py_sequence_fallback, _generic_flowchart.

Reducción de Python (siguiendo el mandato "Rust es lo principal"): los 4
builders `.py` ya no re-parsean con su propio `ast.walk` cuando el sidecar
Rust responde — `parse_python_rich` (el mismo que ya usa
`static_parser.py::_parse_python`) trae funciones/clases/call_graph ya
calculados, y este router solo arma el string Mermaid a partir de eso
(`_py_*_from_rich`). Los 4 originales (`_py_*_fallback`, sin ningún cambio de
cuerpo) se preservan como el camino sin sidecar — antes de esta migración
estos builders eran 100% independientes del sidecar, así que borrarlos sería
una regresión real, no una limpieza, mismo criterio que
`static_parser.py::_parse_python` ya estableció para su propio esqueleto.

`ml.py` sigue siendo el candidato grande de esta misma auditoría (~500
líneas de heurística AST/regex sin ningún equivalente Rust todavía) — no se
toca en esta pasada, es un puerto propio, no una tarde.
"""

import ast
import re
from collections.abc import Callable
from pathlib import Path

from fastapi import APIRouter
from pydantic import BaseModel
from starlette.concurrency import run_in_threadpool

from services.complexity_client import parse_python_rich
from shared import add_log, now

router = APIRouter()


# ── Schema ────────────────────────────────────────────────────────────────────


class DiagramRequest(BaseModel):
    filename: str = "script.py"
    content: str = ""
    diagram_type: str = "flowchart"  # flowchart | callgraph | classes | sequence


# ── Endpoint ──────────────────────────────────────────────────────────────────

_SYNTAX_ERROR_MERMAID: dict[str, Callable[[SyntaxError], str]] = {
    "flowchart": lambda e: f"flowchart TD\n    ERR[SyntaxError línea {e.lineno}]",
    "callgraph": lambda e: "graph LR\n    ERR[Error de sintaxis]",
    "classes": lambda e: "classDiagram\n    class Error",
    "sequence": lambda e: "sequenceDiagram\n    participant Error",
}


def _analyze_diagram_sync(filename: str, content: str, diag_type: str, rich: dict | None) -> dict:
    """Arma el string Mermaid — bloqueante (aunque liviano), corrido en
    threadpool desde el handler async de abajo, mismo criterio que
    `analysis.py::_run_flake8`/`_run_pylint`. `rich` ya viene resuelto (o
    `None`) desde `analyze_diagram` — la llamada de red al sidecar es
    responsabilidad de esa función async, no de acá adentro.

    El `ast.parse()` local de gate (antes de decidir rich-vs-fallback) no es
    redundancia desperdiciada: es el mismo gate rápido que
    `static_parser.py::_parse_python` ya acepta, y es lo que le permite a
    cada `diagram_type` seguir mostrando su propio texto de error de
    sintaxis exacto (`_SYNTAX_ERROR_MERMAID`), en vez de heredar lo que sea
    que el sidecar Rust hubiera devuelto para contenido inválido."""
    ext = Path(filename).suffix.lower()
    result = {"filename": filename, "diagram_type": diag_type, "mermaid": "", "ts": now()}

    try:
        if ext == ".py":
            try:
                ast.parse(content)
            except SyntaxError as e:
                result["mermaid"] = _SYNTAX_ERROR_MERMAID.get(diag_type, _SYNTAX_ERROR_MERMAID["flowchart"])(e)
                add_log("info", f"Diagrama '{diag_type}' para {filename}")
                return result

            if rich is not None:
                if diag_type == "callgraph":
                    result["mermaid"] = _py_callgraph_from_rich(rich["functions"], rich["call_graph"])
                elif diag_type == "classes":
                    result["mermaid"] = _py_classes_from_rich(rich["classes"])
                elif diag_type == "sequence":
                    result["mermaid"] = _py_sequence_from_rich(rich["functions"])
                else:
                    result["mermaid"] = _py_flowchart_from_rich(rich["functions"], filename)
            else:
                fns = {
                    "flowchart": _py_flowchart_fallback,
                    "callgraph": _py_callgraph_fallback,
                    "classes": _py_classes_fallback,
                    "sequence": _py_sequence_fallback,
                }
                fn = fns.get(diag_type, _py_flowchart_fallback)
                # flowchart recibe filename; los demás solo content
                if diag_type == "flowchart":
                    result["mermaid"] = fn(content, filename)
                else:
                    result["mermaid"] = fn(content)
        else:
            result["mermaid"] = _generic_flowchart(content, filename, ext)

        add_log("info", f"Diagrama '{diag_type}' para {filename}")

    except Exception as e:
        result["mermaid"] = f"flowchart TD\n    ERR[Error: {str(e)[:60]}]"

    return result


@router.post("/diagram")
async def analyze_diagram(req: DiagramRequest):
    """Equivalente a POST /analyze/diagram del Flask original. Hace el
    `await` al sidecar acá (I/O de red, no pertenece en el threadpool) antes
    de pasarle el resultado ya resuelto a `_analyze_diagram_sync`."""
    ext = Path(req.filename).suffix.lower()
    rich = await parse_python_rich(req.filename, req.content) if ext == ".py" else None
    return await run_in_threadpool(_analyze_diagram_sync, req.filename, req.content, req.diagram_type, rich)


# ── Builders .py — vía Rust (sidecar arriba) ──────────────────────────────────


def _py_flowchart_from_rich(functions: list[dict], filename: str) -> str:
    funcs = sorted(
        [
            {
                "name": fn["name"],
                "line": fn["line"],
                "args": [a for a in fn.get("args", []) if a != "self"],
                "returns": fn.get("returns_value", False),
                "is_async": fn.get("is_async", False),
                "docstring": (fn.get("docstring") or "")[:40].replace('"', "'"),
            }
            for fn in functions
        ],
        key=lambda x: x["line"],
    )

    if not funcs:
        return f"flowchart TD\n" f"    A[{filename}]\n" f"    B[Sin funciones]\n" f"    A --> B"

    lines = ["flowchart TD", f"    START([{filename}])"]
    for i, fn in enumerate(funcs[:12]):
        prefix = "async " if fn["is_async"] else ""
        label = f'{prefix}{fn["name"]}({", ".join(fn["args"][:2])})'
        if fn["docstring"]:
            label += f'\\n{fn["docstring"]}'
        lines.append(f'    F{i}["{label}"]')

    lines += ["    END([Fin])", "    START --> F0"]
    for i in range(min(len(funcs), 12) - 1):
        lines.append(f"    F{i} --> F{i+1}")
    lines.append(f"    F{min(len(funcs)-1, 11)} --> END")

    for i, fn in enumerate(funcs[:12]):
        c = "#300a3a" if fn["is_async"] else ("#0f3020" if fn["returns"] else "#1a2040")
        s = "#b87dff" if fn["is_async"] else ("#00f5a0" if fn["returns"] else "#3d9eff")
        lines.append(f"    style F{i} fill:{c},stroke:{s},color:#c8d4f0")

    lines += [
        "    style START fill:#0a2040,stroke:#3d9eff,color:#c8d4f0",
        "    style END fill:#0a2040,stroke:#3d9eff,color:#c8d4f0",
    ]
    return "\n".join(lines)


def _py_callgraph_from_rich(functions: list[dict], call_graph: list[dict]) -> str:
    func_names = {fn["name"] for fn in functions}
    if not func_names:
        return "graph LR\n    A[Sin funciones]"

    lines = ["graph LR"] + [f'    {name}["{name}"]' for name in list(func_names)[:12]]
    seen: set[str] = set()
    for edge in call_graph:
        caller, callee = edge.get("from"), edge.get("to")
        if not caller or not callee or caller not in func_names or callee not in func_names:
            continue
        k = f"{caller}_{callee}"
        if k not in seen:
            seen.add(k)
            lines.append(f"    {caller} -->|llama| {callee}")

    return "\n".join(lines)


def _py_classes_from_rich(classes: list[dict]) -> str:
    if not classes:
        return "classDiagram\n    class SinClases"

    class_names = {c["name"] for c in classes}
    lines = ["classDiagram"]
    for cls in classes[:8]:
        lines.append(f'    class {cls["name"]} {{')
        for a in cls.get("attributes", [])[:6]:
            lines.append(f"        +{a}")
        for m in cls.get("methods", [])[:8]:
            args = [a for a in m.get("args", []) if a != "self"]
            lines.append(f'        +{m["name"]}({", ".join(args[:3])})')
        lines.append("    }")
        for base in cls.get("bases", []):
            if base in class_names:
                lines.append(f"    {base} <|-- {cls['name']} : hereda")

    return "\n".join(lines)


def _py_sequence_from_rich(functions: list[dict]) -> str:
    funcs = sorted(functions, key=lambda x: x["line"])

    if len(funcs) < 2:
        return "sequenceDiagram\n" "    participant main\n" "    main->>main: ejecutar\n" "    main-->>main: fin"

    lines = ["sequenceDiagram", "    participant Usuario"] + [f'    participant {fn["name"]}' for fn in funcs[:6]]
    lines.append(f'    Usuario->>+{funcs[0]["name"]}: invocar')
    for i in range(min(len(funcs), 5) - 1):
        lines.append(f'    {funcs[i]["name"]}->>+{funcs[i+1]["name"]}: llamar')
    for i in range(min(len(funcs), 5) - 1, 0, -1):
        lines.append(f'    {funcs[i]["name"]}-->>-{funcs[i-1]["name"]}: retornar')
    lines.append(f'    {funcs[0]["name"]}-->>-Usuario: resultado')

    return "\n".join(lines)


# ── Builders .py — fallback sin sidecar (idénticos al Flask original) ────────


def _py_flowchart_fallback(content: str, filename: str = "script.py") -> str:
    try:
        tree = ast.parse(content)
    except SyntaxError as e:
        return f"flowchart TD\n    ERR[SyntaxError línea {e.lineno}]"

    funcs = sorted(
        [
            {
                "name": n.name,
                "line": n.lineno,
                "args": [a.arg for a in n.args.args if a.arg != "self"],
                "returns": any(isinstance(x, ast.Return) and x.value for x in ast.walk(n)),
                "is_async": isinstance(n, ast.AsyncFunctionDef),
                "docstring": (ast.get_docstring(n) or "")[:40].replace('"', "'"),
            }
            for n in ast.walk(tree)
            if isinstance(n, ast.FunctionDef | ast.AsyncFunctionDef)
        ],
        key=lambda x: x["line"],
    )

    if not funcs:
        return f"flowchart TD\n" f"    A[{filename}]\n" f"    B[Sin funciones]\n" f"    A --> B"

    lines = ["flowchart TD", f"    START([{filename}])"]
    for i, fn in enumerate(funcs[:12]):
        prefix = "async " if fn["is_async"] else ""
        label = f'{prefix}{fn["name"]}({", ".join(fn["args"][:2])})'
        if fn["docstring"]:
            label += f'\\n{fn["docstring"]}'
        lines.append(f'    F{i}["{label}"]')

    lines += ["    END([Fin])", "    START --> F0"]
    for i in range(min(len(funcs), 12) - 1):
        lines.append(f"    F{i} --> F{i+1}")
    lines.append(f"    F{min(len(funcs)-1, 11)} --> END")

    for i, fn in enumerate(funcs[:12]):
        c = "#300a3a" if fn["is_async"] else ("#0f3020" if fn["returns"] else "#1a2040")
        s = "#b87dff" if fn["is_async"] else ("#00f5a0" if fn["returns"] else "#3d9eff")
        lines.append(f"    style F{i} fill:{c},stroke:{s},color:#c8d4f0")

    lines += [
        "    style START fill:#0a2040,stroke:#3d9eff,color:#c8d4f0",
        "    style END fill:#0a2040,stroke:#3d9eff,color:#c8d4f0",
    ]
    return "\n".join(lines)


def _py_callgraph_fallback(content: str) -> str:
    try:
        tree = ast.parse(content)
    except Exception:
        return "graph LR\n    ERR[Error de sintaxis]"

    # Recopilar funciones top-level y de clase (primer nivel del árbol)
    func_nodes = [n for n in ast.walk(tree) if isinstance(n, ast.FunctionDef | ast.AsyncFunctionDef)]
    func_names = {n.name for n in func_nodes}

    if not func_names:
        return "graph LR\n    A[Sin funciones]"

    # Iterar cada función por separado para asignar correctamente caller
    call_map: dict[str, list] = {fn.name: [] for fn in func_nodes}
    for fn_node in func_nodes:
        caller = fn_node.name
        for node in ast.walk(fn_node):
            # Saltar la función misma (no sus hijos de primer nivel que son otras funciones)
            if node is fn_node:
                continue
            if isinstance(node, ast.Call):
                if isinstance(node.func, ast.Name):
                    callee = node.func.id
                elif isinstance(node.func, ast.Attribute):
                    callee = node.func.attr
                else:
                    callee = None
                if callee and callee in func_names and callee != caller:
                    call_map[caller].append(callee)

    lines = ["graph LR"] + [f'    {fn}["{fn}"]' for fn in list(func_names)[:12]]
    seen: set[str] = set()
    for caller, callees in call_map.items():
        for callee in callees:
            k = f"{caller}_{callee}"
            if k not in seen and callee in func_names:
                seen.add(k)
                lines.append(f"    {caller} -->|llama| {callee}")

    return "\n".join(lines)


def _py_classes_fallback(content: str) -> str:
    try:
        tree = ast.parse(content)
    except Exception:
        return "classDiagram\n    class Error"

    classes = []
    for node in ast.walk(tree):
        if not isinstance(node, ast.ClassDef):
            continue
        methods, attrs = [], []
        for item in node.body:
            if isinstance(item, ast.FunctionDef | ast.AsyncFunctionDef):
                methods.append(
                    {
                        "name": item.name,
                        "args": [a.arg for a in item.args.args if a.arg != "self"],
                    }
                )
            if isinstance(item, ast.Assign):
                for t in item.targets:
                    if isinstance(t, ast.Name):
                        attrs.append(t.id)
        # Atributos de instancia (self.x = ...)
        for item in node.body:
            if isinstance(item, ast.FunctionDef) and item.name == "__init__":
                for n in ast.walk(item):
                    if isinstance(n, ast.Assign):
                        for t in n.targets:
                            if (
                                isinstance(t, ast.Attribute)
                                and isinstance(t.value, ast.Name)
                                and t.value.id == "self"
                                and t.attr not in attrs
                            ):
                                attrs.append(t.attr)
        classes.append(
            {
                "name": node.name,
                "methods": methods,
                "attrs": attrs,
                "bases": [b.id for b in node.bases if isinstance(b, ast.Name)],
            }
        )

    if not classes:
        return "classDiagram\n    class SinClases"

    lines = ["classDiagram"]
    for cls in classes[:8]:
        lines.append(f'    class {cls["name"]} {{')
        for a in cls["attrs"][:6]:
            lines.append(f"        +{a}")
        for m in cls["methods"][:8]:
            lines.append(f'        +{m["name"]}({", ".join(m["args"][:3])})')
        lines.append("    }")
        for base in cls["bases"]:
            if any(c["name"] == base for c in classes):
                lines.append(f"    {base} <|-- {cls['name']} : hereda")

    return "\n".join(lines)


def _py_sequence_fallback(content: str) -> str:
    try:
        tree = ast.parse(content)
    except Exception:
        return "sequenceDiagram\n    participant Error"

    funcs = sorted(
        [
            {"name": n.name, "line": n.lineno}
            for n in ast.walk(tree)
            if isinstance(n, ast.FunctionDef | ast.AsyncFunctionDef)
        ],
        key=lambda x: x["line"],
    )

    if len(funcs) < 2:
        return "sequenceDiagram\n" "    participant main\n" "    main->>main: ejecutar\n" "    main-->>main: fin"

    lines = ["sequenceDiagram", "    participant Usuario"] + [f'    participant {fn["name"]}' for fn in funcs[:6]]
    lines.append(f'    Usuario->>+{funcs[0]["name"]}: invocar')
    for i in range(min(len(funcs), 5) - 1):
        lines.append(f'    {funcs[i]["name"]}->>+{funcs[i+1]["name"]}: llamar')
    for i in range(min(len(funcs), 5) - 1, 0, -1):
        lines.append(f'    {funcs[i]["name"]}-->>-{funcs[i-1]["name"]}: retornar')
    lines.append(f'    {funcs[0]["name"]}-->>-Usuario: resultado')

    return "\n".join(lines)


def _generic_flowchart(content: str, filename: str, ext: str) -> str:
    lines_list = content.splitlines()
    funcs: list[dict] = []

    if ext in (".js", ".ts"):
        for i, line in enumerate(lines_list):
            m = re.search(r"(?:function\s+(\w+)|const\s+(\w+)\s*=\s*(?:async\s*)?\()", line)
            if m:
                name = m.group(1) or m.group(2)
                if name and name not in ("if", "for", "while"):
                    funcs.append({"name": name, "line": i + 1})

    if not funcs:
        return (
            f"flowchart TD\n"
            f'    A[{filename}]\n'
            f'    B[{len(lines_list)} líneas · {ext.replace(".", "").upper()}]\n'
            f"    A --> B"
        )

    code = f"flowchart TD\n    START([{filename}])\n"
    for i, fn in enumerate(funcs[:10]):
        code += f'    F{i}["{fn["name"]}()"]\n'
    code += "    END([Fin])\n    START --> F0\n"
    for i in range(min(len(funcs), 10) - 1):
        code += f"    F{i} --> F{i+1}\n"
    code += f"    F{min(len(funcs)-1, 9)} --> END\n"
    return code
