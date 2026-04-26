"""
Router: Diagram
Migración exacta de /analyze/diagram del app.py Flask v3.0.
Preserva: _py_flowchart, _py_callgraph, _py_classes, _py_sequence, _generic_flowchart.
"""

import ast
import re
from pathlib import Path

from fastapi import APIRouter
from pydantic import BaseModel

from shared import add_log, now

router = APIRouter()


# ── Schema ────────────────────────────────────────────────────────────────────

class DiagramRequest(BaseModel):
    filename:     str = "script.py"
    content:      str = ""
    diagram_type: str = "flowchart"   # flowchart | callgraph | classes | sequence


# ── Endpoint ──────────────────────────────────────────────────────────────────

@router.post("/diagram")
async def analyze_diagram(req: DiagramRequest):
    """Equivalente a POST /analyze/diagram del Flask original."""
    filename  = req.filename
    content   = req.content
    diag_type = req.diagram_type
    ext       = Path(filename).suffix.lower()

    result = {"filename": filename, "diagram_type": diag_type, "mermaid": "", "ts": now()}

    try:
        if ext == ".py":
            fns = {
                "flowchart": _py_flowchart,
                "callgraph": _py_callgraph,
                "classes":   _py_classes,
                "sequence":  _py_sequence,
            }
            fn = fns.get(diag_type, _py_flowchart)
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


# ── Funciones de diagramas (idénticas al Flask original) ─────────────────────

def _py_flowchart(content: str, filename: str = "script.py") -> str:
    try:
        tree = ast.parse(content)
    except SyntaxError as e:
        return f"flowchart TD\n    ERR[SyntaxError línea {e.lineno}]"

    funcs = sorted(
        [
            {
                "name":     n.name,
                "line":     n.lineno,
                "args":     [a.arg for a in n.args.args if a.arg != "self"],
                "returns":  any(
                    isinstance(x, ast.Return) and x.value
                    for x in ast.walk(n)
                ),
                "is_async": isinstance(n, ast.AsyncFunctionDef),
                "docstring":(ast.get_docstring(n) or "")[:40].replace('"', "'"),
            }
            for n in ast.walk(tree)
            if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))
        ],
        key=lambda x: x["line"],
    )

    if not funcs:
        return (
            f"flowchart TD\n"
            f'    A[📄 {filename}]\n'
            f"    B[Sin funciones]\n"
            f"    A --> B"
        )

    lines = ["flowchart TD", f'    START([🚀 {filename}])']
    for i, fn in enumerate(funcs[:12]):
        icon  = "⚡" if fn["is_async"] else "⚙️"
        label = f'{icon} {fn["name"]}({", ".join(fn["args"][:2])})'
        if fn["docstring"]:
            label += f'\\n📝 {fn["docstring"]}'
        lines.append(f'    F{i}["{label}"]')

    lines += ["    END([🏁 Fin])", "    START --> F0"]
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


def _py_callgraph(content: str) -> str:
    try:
        tree = ast.parse(content)
    except Exception:
        return "graph LR\n    ERR[Error de sintaxis]"

    # Recopilar funciones top-level y de clase (primer nivel del árbol)
    func_nodes = [
        n for n in ast.walk(tree)
        if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))
    ]
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

    lines = ["graph LR"] + [
        f'    {fn}["⚙️ {fn}"]' for fn in list(func_names)[:12]
    ]
    seen: set[str] = set()
    for caller, callees in call_map.items():
        for callee in callees:
            k = f"{caller}_{callee}"
            if k not in seen and callee in func_names:
                seen.add(k)
                lines.append(f"    {caller} -->|llama| {callee}")

    return "\n".join(lines)


def _py_classes(content: str) -> str:
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
            if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
                methods.append({
                    "name": item.name,
                    "args": [a.arg for a in item.args.args if a.arg != "self"],
                })
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
                            if (isinstance(t, ast.Attribute)
                                    and isinstance(t.value, ast.Name)
                                    and t.value.id == "self"
                                    and t.attr not in attrs):
                                attrs.append(t.attr)
        classes.append({
            "name":    node.name,
            "methods": methods,
            "attrs":   attrs,
            "bases":   [b.id for b in node.bases if isinstance(b, ast.Name)],
        })

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


def _py_sequence(content: str) -> str:
    try:
        tree = ast.parse(content)
    except Exception:
        return "sequenceDiagram\n    participant Error"

    funcs = sorted(
        [
            {"name": n.name, "line": n.lineno}
            for n in ast.walk(tree)
            if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))
        ],
        key=lambda x: x["line"],
    )

    if len(funcs) < 2:
        return (
            "sequenceDiagram\n"
            "    participant main\n"
            "    main->>main: ejecutar\n"
            "    main-->>main: fin"
        )

    lines = (
        ["sequenceDiagram", "    participant Usuario"]
        + [f'    participant {fn["name"]}' for fn in funcs[:6]]
    )
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
            m = re.search(
                r'(?:function\s+(\w+)|const\s+(\w+)\s*=\s*(?:async\s*)?\()', line
            )
            if m:
                name = m.group(1) or m.group(2)
                if name and name not in ("if", "for", "while"):
                    funcs.append({"name": name, "line": i + 1})

    if not funcs:
        return (
            f"flowchart TD\n"
            f'    A[📄 {filename}]\n'
            f'    B[{len(lines_list)} líneas · {ext.replace(".", "").upper()}]\n'
            f"    A --> B"
        )

    code = f"flowchart TD\n    START([📄 {filename}])\n"
    for i, fn in enumerate(funcs[:10]):
        code += f'    F{i}["⚙️ {fn["name"]}()"]\n'
    code += "    END([🏁])\n    START --> F0\n"
    for i in range(min(len(funcs), 10) - 1):
        code += f"    F{i} --> F{i+1}\n"
    code += f"    F{min(len(funcs)-1, 9)} --> END\n"
    return code
