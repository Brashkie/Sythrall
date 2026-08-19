"""
Tests — Editor Intelligence Router
pytest tests/test_intelligence.py -v
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from fastapi.testclient import TestClient
from main import app

client = TestClient(app)

# ─── Fixtures ────────────────────────────────────────────────────────────────

PY_CLEAN = """
def add(a: int, b: int) -> int:
    return a + b

def greet(name: str) -> str:
    return f"Hello {name}"
"""

PY_ISSUES = """
import os
import unused_lib

def bubble_sort(arr):
    n = len(arr)
    for i in range(n):
        for j in range(n - i - 1):
            if arr[j] > arr[j+1]:
                arr[j], arr[j+1] = arr[j+1], arr[j]

def binary_search(arr, target):
    lo, hi = 0, len(arr)
    while lo < hi:
        mid = (lo + hi) // 2
        if arr[mid] == target: return mid
        elif arr[mid] < target: lo = mid + 1
        else: hi = mid
    return -1

def constant(x):
    return x * 2

x = print("debug leak")

def bad_except():
    try:
        pass
    except:
        pass
"""

PY_SYNTAX_ERROR = """
def broken(
    pass
"""

TS_ISSUES = """
import { readFileSync } from 'fs'
import { unusedThing } from './unused'

function nestedLoops(n: number): void {
    for (let i = 0; i < n; i++) {
        for (let j = 0; j < n; j++) {
            console.log(i, j)
        }
    }
}

var oldStyle = 42
const x = (a: any) => a
debugger
"""

TS_CLEAN = """
export function add(a: number, b: number): number {
    return a + b
}

export const greet = (name: string): string => {
    return `Hello ${name}`
}
"""


# ─── /intel/lint — Python ────────────────────────────────────────────────────


class TestFastLintPython:
    def test_lint_ok(self):
        r = client.post("/intel/lint", json={"filename": "test.py", "content": PY_ISSUES})
        assert r.status_code == 200

    def test_lint_returns_markers(self):
        data = client.post("/intel/lint", json={"filename": "test.py", "content": PY_ISSUES}).json()
        assert "markers" in data
        assert isinstance(data["markers"], list)

    def test_lint_detects_print(self):
        data = client.post("/intel/lint", json={"filename": "test.py", "content": PY_ISSUES}).json()
        codes = [m["code"] for m in data["markers"]]
        assert "C002" in codes

    def test_lint_detects_bare_except(self):
        data = client.post("/intel/lint", json={"filename": "test.py", "content": PY_ISSUES}).json()
        codes = [m["code"] for m in data["markers"]]
        assert "W002" in codes

    def test_lint_syntax_error(self):
        data = client.post("/intel/lint", json={"filename": "test.py", "content": PY_SYNTAX_ERROR}).json()
        assert any(m["severity"] == 8 for m in data["markers"])
        assert any("SyntaxError" in m["message"] for m in data["markers"])

    def test_lint_clean_code(self):
        data = client.post("/intel/lint", json={"filename": "test.py", "content": PY_CLEAN}).json()
        errors = [m for m in data["markers"] if m["severity"] == 8]
        assert len(errors) == 0

    def test_lint_marker_structure(self):
        data = client.post("/intel/lint", json={"filename": "test.py", "content": PY_ISSUES}).json()
        for m in data["markers"]:
            assert "startLineNumber" in m
            assert "startColumn" in m
            assert "endLineNumber" in m
            assert "endColumn" in m
            assert "message" in m
            assert "severity" in m
            assert m["severity"] in (1, 2, 4, 8)

    def test_lint_severity_values(self):
        data = client.post("/intel/lint", json={"filename": "test.py", "content": PY_ISSUES}).json()
        for m in data["markers"]:
            assert m["severity"] in (1, 2, 4, 8), "Severity debe ser compatible con Monaco"

    def test_lint_ms_field(self):
        data = client.post("/intel/lint", json={"filename": "test.py", "content": PY_ISSUES}).json()
        assert "ms" in data
        assert data["ms"] < 500, "Fast lint debe ser < 500ms"

    def test_lint_empty_content(self):
        r = client.post("/intel/lint", json={"filename": "test.py", "content": ""})
        assert r.status_code == 200
        assert r.json()["markers"] == []

    def test_lint_todo_hint(self):
        code = "# TODO: fix this\nx = 1"
        data = client.post("/intel/lint", json={"filename": "test.py", "content": code}).json()
        hints = [m for m in data["markers"] if m["severity"] == 1]
        assert len(hints) >= 1

    def test_lint_long_line(self):
        code = "x = " + "a" * 130
        data = client.post("/intel/lint", json={"filename": "test.py", "content": code}).json()
        codes = [m["code"] for m in data["markers"]]
        assert "E501" in codes


# ─── /intel/lint — TypeScript ────────────────────────────────────────────────


class TestFastLintTypeScript:
    def test_lint_ts_ok(self):
        r = client.post("/intel/lint", json={"filename": "app.ts", "content": TS_ISSUES})
        assert r.status_code == 200

    def test_lint_ts_detects_console(self):
        data = client.post("/intel/lint", json={"filename": "app.ts", "content": TS_ISSUES}).json()
        codes = [m["code"] for m in data["markers"]]
        assert "TS001" in codes

    def test_lint_ts_detects_any(self):
        data = client.post("/intel/lint", json={"filename": "app.ts", "content": TS_ISSUES}).json()
        codes = [m["code"] for m in data["markers"]]
        assert "TS002" in codes

    def test_lint_ts_detects_debugger(self):
        data = client.post("/intel/lint", json={"filename": "app.ts", "content": TS_ISSUES}).json()
        codes = [m["code"] for m in data["markers"]]
        assert "TS003" in codes

    def test_lint_ts_detects_var(self):
        data = client.post("/intel/lint", json={"filename": "app.ts", "content": TS_ISSUES}).json()
        codes = [m["code"] for m in data["markers"]]
        assert "TS004" in codes

    def test_lint_ts_clean_code(self):
        data = client.post("/intel/lint", json={"filename": "app.ts", "content": TS_CLEAN}).json()
        errors = [m for m in data["markers"] if m["severity"] == 8]
        assert len(errors) == 0

    def test_lint_ts_marker_structure(self):
        data = client.post("/intel/lint", json={"filename": "app.ts", "content": TS_ISSUES}).json()
        for m in data["markers"]:
            assert m["severity"] in (1, 2, 4, 8)
            assert m["startLineNumber"] >= 1

    def test_lint_tsx_extension(self):
        r = client.post("/intel/lint", json={"filename": "Component.tsx", "content": TS_ISSUES})
        assert r.status_code == 200

    def test_lint_js_extension(self):
        r = client.post("/intel/lint", json={"filename": "script.js", "content": "var x = 1\nconsole.log(x)"})
        assert r.status_code == 200
        codes = [m["code"] for m in r.json()["markers"]]
        assert "TS004" in codes or "TS001" in codes


# ─── /intel/analyze — heavy path ─────────────────────────────────────────────


class TestHeavyAnalyze:
    def test_analyze_ok(self):
        r = client.post(
            "/intel/analyze", json={"filename": "test.py", "content": PY_ISSUES, "tools": ["ast", "complexity"]}
        )
        assert r.status_code == 200

    def test_analyze_returns_big_o(self):
        data = client.post(
            "/intel/analyze", json={"filename": "test.py", "content": PY_ISSUES, "tools": ["ast", "complexity"]}
        ).json()
        assert "big_o" in data
        assert len(data["big_o"]) >= 2

    def test_analyze_halstead_key_present_without_sidecar(self):
        """Fase 22: sin sidecar (no corre en pytest), `halstead` tiene que
        seguir presente en metrics, en None — no faltar la key."""
        data = client.post(
            "/intel/analyze", json={"filename": "test.py", "content": PY_ISSUES, "tools": ["ast", "complexity"]}
        ).json()
        assert "halstead" in data["metrics"]
        assert data["metrics"]["halstead"] is None

    def test_analyze_big_o_bubble(self):
        data = client.post(
            "/intel/analyze", json={"filename": "test.py", "content": PY_ISSUES, "tools": ["ast"]}
        ).json()
        fn = next((f for f in data["big_o"] if f["name"] == "bubble_sort"), None)
        assert fn is not None
        assert fn["big_o"] == "O(n²)"

    def test_analyze_big_o_binary(self):
        data = client.post(
            "/intel/analyze", json={"filename": "test.py", "content": PY_ISSUES, "tools": ["ast"]}
        ).json()
        fn = next((f for f in data["big_o"] if f["name"] == "binary_search"), None)
        assert fn is not None
        assert fn["big_o"] == "O(log n)"

    def test_analyze_big_o_constant(self):
        data = client.post(
            "/intel/analyze", json={"filename": "test.py", "content": PY_ISSUES, "tools": ["ast"]}
        ).json()
        fn = next((f for f in data["big_o"] if f["name"] == "constant"), None)
        assert fn is not None
        assert fn["big_o"] == "O(1)"

    def test_analyze_returns_markers(self):
        data = client.post(
            "/intel/analyze", json={"filename": "test.py", "content": PY_ISSUES, "tools": ["ast"]}
        ).json()
        assert "markers" in data

    def test_analyze_returns_ms(self):
        data = client.post(
            "/intel/analyze", json={"filename": "test.py", "content": PY_ISSUES, "tools": ["ast"]}
        ).json()
        assert "ms" in data

    def test_analyze_source_heavy(self):
        data = client.post(
            "/intel/analyze", json={"filename": "test.py", "content": PY_ISSUES, "tools": ["ast"]}
        ).json()
        assert data["source"] == "heavy"

    def test_analyze_ts_big_o(self):
        data = client.post("/intel/analyze", json={"filename": "app.ts", "content": TS_ISSUES, "tools": []}).json()
        assert "big_o" in data
        fn = next((f for f in data["big_o"] if f["name"] == "nestedLoops"), None)
        if fn:
            assert fn["big_o"] == "O(n²)"

    def test_analyze_empty_content(self):
        r = client.post("/intel/analyze", json={"filename": "test.py", "content": "", "tools": ["ast"]})
        assert r.status_code == 200
        assert r.json()["big_o"] == []

    def test_analyze_complexity_metrics(self):
        # El sidecar Rust (services/complexity) es una capacidad opcional — en CI
        # no está corriendo, así que esto ejercita el wiring (request → shape
        # de respuesta) y la degradación con gracia, no los valores en sí
        # (eso lo cubre `cargo test` sobre services/complexity/src/*.rs).
        data = client.post(
            "/intel/analyze",
            json={
                "filename": "test.py",
                "content": PY_ISSUES,
                "tools": ["complexity"],
            },
        ).json()
        assert "metrics" in data
        if data["metrics"].get("complexity"):
            for fn in data["metrics"]["complexity"]:
                assert "name" in fn
                assert "complexity" in fn
                assert "rank" in fn


# ─── /intel/analyze — clasificadores CS Engine (regex/grammar/graph) ────────


CS_CLASSIFIERS_PY = """
import re
from collections import deque

def has_email(s):
    return re.search(r"[\\w.]+@[\\w.]+", s) is not None

def parse_expr(tokens, pos):
    stack = []
    stack.append(tokens[pos])
    if pos < len(tokens):
        return parse_expr(tokens, pos + 1)
    return stack.pop()

def factorial(n):
    if n <= 1:
        return 1
    return n * factorial(n - 1)

def bfs(graph, start):
    visited = set([start])
    queue = deque([start])
    while queue:
        node = queue.popleft()
        for n in graph[node]:
            if n not in visited:
                visited.add(n)
                queue.append(n)
    return visited

def topo_sort(graph):
    in_degree = {}
    for node in graph:
        in_degree[node] = 0
    return in_degree

def plain(x):
    return x + 1
"""


class TestCsEngineClassifiers:
    def _functions(self):
        data = client.post(
            "/intel/analyze", json={"filename": "cs.py", "content": CS_CLASSIFIERS_PY, "tools": ["ast"]}
        ).json()
        return {f["name"]: f for f in data["big_o"]}

    def test_regex_classified_as_type3(self):
        fn = self._functions()["has_email"]
        assert fn["regex_class"] == "Type-3 (Regular)"
        assert fn["regex_note"]

    def test_regex_not_classified_without_re_call(self):
        fn = self._functions()["plain"]
        assert fn["regex_class"] is None
        assert fn["regex_note"] is None

    def test_grammar_classified_by_name_and_shape(self):
        fn = self._functions()["parse_expr"]
        assert fn["grammar_class"] == "Type-2 (Context-Free)"
        assert fn["grammar_note"]

    def test_grammar_requires_name_signal_not_just_recursion(self):
        # Recursiva pero sin nombre de parser — la señal de nombre es obligatoria.
        fn = self._functions()["factorial"]
        assert fn["grammar_class"] is None

    def test_graph_traversal_bfs(self):
        fn = self._functions()["bfs"]
        assert fn["graph_traversal"] == "BFS"
        assert fn["graph_traversal_note"]

    def test_graph_traversal_topological_sort(self):
        fn = self._functions()["topo_sort"]
        assert fn["graph_traversal"] == "Topological Sort (Kahn's algorithm)"

    def test_graph_traversal_none_for_plain_function(self):
        fn = self._functions()["plain"]
        assert fn["graph_traversal"] is None


# ─── /intel/hover — Python ───────────────────────────────────────────────────


class TestHoverPython:
    def test_hover_ok(self):
        r = client.post(
            "/intel/hover",
            json={
                "filename": "test.py",
                "content": PY_ISSUES,
                "line": 5,
                "column": 1,
                "symbol_name": "bubble_sort",
            },
        )
        assert r.status_code == 200

    def test_hover_returns_markdown(self):
        data = client.post(
            "/intel/hover",
            json={
                "filename": "test.py",
                "content": PY_ISSUES,
                "line": 5,
                "column": 1,
                "symbol_name": "bubble_sort",
            },
        ).json()
        assert "markdown" in data
        assert len(data["markdown"]) > 0

    def test_hover_contains_signature(self):
        data = client.post(
            "/intel/hover",
            json={
                "filename": "test.py",
                "content": PY_ISSUES,
                "line": 5,
                "column": 1,
                "symbol_name": "bubble_sort",
            },
        ).json()
        assert "bubble_sort" in data["markdown"]

    def test_hover_contains_big_o(self):
        data = client.post(
            "/intel/hover",
            json={
                "filename": "test.py",
                "content": PY_ISSUES,
                "line": 5,
                "column": 1,
                "symbol_name": "bubble_sort",
            },
        ).json()
        assert "O(n²)" in data["markdown"]

    def test_hover_contains_cc(self):
        data = client.post(
            "/intel/hover",
            json={
                "filename": "test.py",
                "content": PY_ISSUES,
                "line": 5,
                "column": 1,
                "symbol_name": "bubble_sort",
            },
        ).json()
        assert "CC" in data["markdown"] or "Cyclomatic" in data["markdown"]

    def test_hover_hot_path_warning(self):
        data = client.post(
            "/intel/hover",
            json={
                "filename": "test.py",
                "content": PY_ISSUES,
                "line": 5,
                "column": 1,
                "symbol_name": "bubble_sort",
            },
        ).json()
        assert "Hot Path" in data["markdown"] or "Cython" in data["markdown"]

    def test_hover_docstring_included(self):
        data = client.post(
            "/intel/hover",
            json={
                "filename": "test.py",
                "content": PY_ISSUES,
                "line": 5,
                "column": 1,
                "symbol_name": "bubble_sort",
            },
        ).json()
        # bubble_sort no tiene docstring en PY_ISSUES, solo verifica que no crashea
        assert "markdown" in data

    def test_hover_binary_search_log_n(self):
        data = client.post(
            "/intel/hover",
            json={
                "filename": "test.py",
                "content": PY_ISSUES,
                "line": 12,
                "column": 1,
                "symbol_name": "binary_search",
            },
        ).json()
        assert "O(log n)" in data["markdown"]

    def test_hover_bigo_emoji(self):
        data = client.post(
            "/intel/hover",
            json={
                "filename": "test.py",
                "content": PY_ISSUES,
                "line": 5,
                "column": 1,
                "symbol_name": "bubble_sort",
            },
        ).json()
        # Debe tener uno de los emojis de severity
        assert any(e in data["markdown"] for e in ["🟢", "🟡", "🔴", "⚪"])

    def test_hover_notation_reference_footnote(self):
        data = client.post(
            "/intel/hover",
            json={
                "filename": "test.py",
                "content": PY_ISSUES,
                "line": 5,
                "column": 1,
                "symbol_name": "bubble_sort",
            },
        ).json()
        # Fase 13: pie de nota con la referencia de notación asintótica (O/Θ/Ω/o/ω)
        assert "cota superior" in data["markdown"].lower()
        assert "o (cota superior estricta)" in data["markdown"].lower()

    def test_hover_security_finding_shown(self):
        code = """
def get_user(request):
    username = request.args["username"]
    db.execute("SELECT * FROM users WHERE name=" + username)
"""
        data = client.post(
            "/intel/hover",
            json={"filename": "test.py", "content": code, "line": 2, "column": 1, "symbol_name": "get_user"},
        ).json()
        assert "CWE-89" in data["markdown"]
        assert "SQL Injection" in data["markdown"]

    def test_hover_no_security_section_when_clean(self):
        data = client.post(
            "/intel/hover",
            json={
                "filename": "test.py",
                "content": PY_ISSUES,
                "line": 5,
                "column": 1,
                "symbol_name": "bubble_sort",
            },
        ).json()
        assert "CWE-" not in data["markdown"]

    def test_hover_range_present(self):
        data = client.post(
            "/intel/hover",
            json={
                "filename": "test.py",
                "content": PY_ISSUES,
                "line": 5,
                "column": 1,
                "symbol_name": "bubble_sort",
            },
        ).json()
        assert "range" in data
        if data["range"]:
            assert "startLineNumber" in data["range"]
            assert "endLineNumber" in data["range"]

    def test_hover_unknown_line_returns_empty(self):
        data = client.post(
            "/intel/hover",
            json={
                "filename": "test.py",
                "content": PY_ISSUES,
                "line": 999,
                "column": 1,
                "symbol_name": "",
            },
        ).json()
        assert data["markdown"] == ""

    def test_hover_syntax_error_no_crash(self):
        r = client.post(
            "/intel/hover",
            json={
                "filename": "test.py",
                "content": PY_SYNTAX_ERROR,
                "line": 1,
                "column": 1,
                "symbol_name": "",
            },
        )
        assert r.status_code == 200


# ─── /intel/hover — TypeScript ───────────────────────────────────────────────


class TestHoverTypeScript:
    def test_hover_ts_ok(self):
        r = client.post(
            "/intel/hover",
            json={
                "filename": "app.ts",
                "content": TS_ISSUES,
                "line": 4,
                "column": 1,
                "symbol_name": "nestedLoops",
            },
        )
        assert r.status_code == 200

    def test_hover_ts_contains_function_name(self):
        data = client.post(
            "/intel/hover",
            json={
                "filename": "app.ts",
                "content": TS_ISSUES,
                "line": 4,
                "column": 1,
                "symbol_name": "nestedLoops",
            },
        ).json()
        if data["markdown"]:
            assert "nestedLoops" in data["markdown"]

    def test_hover_ts_contains_big_o(self):
        data = client.post(
            "/intel/hover",
            json={
                "filename": "app.ts",
                "content": TS_ISSUES,
                "line": 4,
                "column": 1,
                "symbol_name": "nestedLoops",
            },
        ).json()
        if data["markdown"]:
            assert "O(" in data["markdown"]

    def test_hover_ts_no_crash_unknown(self):
        r = client.post(
            "/intel/hover",
            json={
                "filename": "app.ts",
                "content": TS_ISSUES,
                "line": 999,
                "column": 1,
                "symbol_name": "nonexistent",
            },
        )
        assert r.status_code == 200
        assert r.json()["markdown"] == ""


# ─── Fixtures adicionales ─────────────────────────────────────────────────────

PY_DEFS = '''
import os

class DataProcessor:
    """Procesa datos."""
    def __init__(self, data):
        self.data = data

    def process(self):
        return [self._transform(x) for x in self.data]

    def _transform(self, x):
        return x * 2

def bubble_sort(arr):
    n = len(arr)
    for i in range(n):
        for j in range(n - i - 1):
            if arr[j] > arr[j+1]:
                arr[j], arr[j+1] = arr[j+1], arr[j]

result = bubble_sort([3,1,2])
dp = DataProcessor([1,2,3])
dp.process()
'''

TS_DEFS = """
interface User { id: number; name: string }

class UserService {
  findById(id: number): User | undefined { return undefined }
  addUser(user: User): void {}
}

function processUsers(service: UserService): void {
  service.findById(1)
}

const svc = new UserService()
processUsers(svc)
"""


# ─── /intel/definition — Python ──────────────────────────────────────────────


class TestGoToDefinitionPython:
    def test_definition_ok(self):
        r = client.post(
            "/intel/definition",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "line": 22,
                "column": 10,
                "symbol_name": "bubble_sort",
            },
        )
        assert r.status_code == 200

    def test_definition_found_function(self):
        data = client.post(
            "/intel/definition",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "line": 22,
                "column": 10,
                "symbol_name": "bubble_sort",
            },
        ).json()
        assert data["found"] is True
        assert len(data["definitions"]) >= 1
        assert data["definitions"][0]["line"] == 15

    def test_definition_found_class(self):
        data = client.post(
            "/intel/definition",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "line": 23,
                "column": 6,
                "symbol_name": "DataProcessor",
            },
        ).json()
        assert data["found"] is True
        defn = data["definitions"][0]
        assert defn["kind"] == "class"
        assert defn["line"] == 4

    def test_definition_kind_function(self):
        data = client.post(
            "/intel/definition",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "line": 22,
                "column": 10,
                "symbol_name": "bubble_sort",
            },
        ).json()
        assert data["definitions"][0]["kind"] in ("function", "method")

    def test_definition_signature_present(self):
        data = client.post(
            "/intel/definition",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "line": 22,
                "column": 10,
                "symbol_name": "bubble_sort",
            },
        ).json()
        assert "bubble_sort" in data["definitions"][0]["signature"]

    def test_definition_docstring(self):
        data = client.post(
            "/intel/definition",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "line": 23,
                "column": 6,
                "symbol_name": "DataProcessor",
            },
        ).json()
        assert "Procesa datos" in data["definitions"][0]["docstring"]

    def test_definition_not_found(self):
        data = client.post(
            "/intel/definition",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "line": 1,
                "column": 1,
                "symbol_name": "nonexistent_xyz",
            },
        ).json()
        assert data["found"] is False
        assert data["definitions"] == []

    def test_definition_empty_symbol(self):
        data = client.post(
            "/intel/definition",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "line": 1,
                "column": 1,
                "symbol_name": "",
            },
        ).json()
        assert data["found"] is False

    def test_definition_method(self):
        data = client.post(
            "/intel/definition",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "line": 24,
                "column": 4,
                "symbol_name": "process",
            },
        ).json()
        assert data["found"] is True
        assert data["definitions"][0]["kind"] == "method"

    def test_definition_response_structure(self):
        data = client.post(
            "/intel/definition",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "line": 22,
                "column": 10,
                "symbol_name": "bubble_sort",
            },
        ).json()
        assert "found" in data
        assert "symbol" in data
        assert "definitions" in data
        for d in data["definitions"]:
            assert "line" in d
            assert "column" in d
            assert "kind" in d
            assert "signature" in d
            assert "docstring" in d

    def test_definition_syntax_error_no_crash(self):
        r = client.post(
            "/intel/definition",
            json={
                "filename": "test.py",
                "content": "def broken(\n  pass",
                "line": 1,
                "column": 1,
                "symbol_name": "broken",
            },
        )
        assert r.status_code == 200


# ─── /intel/definition — TypeScript ──────────────────────────────────────────


class TestGoToDefinitionTS:
    def test_definition_ts_ok(self):
        r = client.post(
            "/intel/definition",
            json={
                "filename": "service.ts",
                "content": TS_DEFS,
                "line": 9,
                "column": 10,
                "symbol_name": "processUsers",
            },
        )
        assert r.status_code == 200

    def test_definition_ts_function(self):
        data = client.post(
            "/intel/definition",
            json={
                "filename": "service.ts",
                "content": TS_DEFS,
                "line": 13,
                "column": 1,
                "symbol_name": "processUsers",
            },
        ).json()
        assert data["found"] is True
        assert data["definitions"][0]["kind"] == "function"

    def test_definition_ts_class(self):
        data = client.post(
            "/intel/definition",
            json={
                "filename": "service.ts",
                "content": TS_DEFS,
                "line": 12,
                "column": 16,
                "symbol_name": "UserService",
            },
        ).json()
        assert data["found"] is True
        assert data["definitions"][0]["kind"] == "class"

    def test_definition_ts_interface(self):
        data = client.post(
            "/intel/definition",
            json={
                "filename": "service.ts",
                "content": TS_DEFS,
                "line": 8,
                "column": 20,
                "symbol_name": "User",
            },
        ).json()
        assert data["found"] is True
        assert data["definitions"][0]["kind"] in ("interface", "class")

    def test_definition_ts_not_found(self):
        data = client.post(
            "/intel/definition",
            json={
                "filename": "service.ts",
                "content": TS_DEFS,
                "line": 1,
                "column": 1,
                "symbol_name": "NonExistentXYZ",
            },
        ).json()
        assert data["found"] is False


# ─── /intel/references — Python ──────────────────────────────────────────────


class TestFindReferencesPython:
    def test_references_ok(self):
        r = client.post(
            "/intel/references",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "symbol_name": "bubble_sort",
            },
        )
        assert r.status_code == 200

    def test_references_finds_definition(self):
        data = client.post(
            "/intel/references",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "symbol_name": "bubble_sort",
            },
        ).json()
        kinds = [r["kind"] for r in data["references"]]
        assert "definition" in kinds

    def test_references_finds_usage(self):
        data = client.post(
            "/intel/references",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "symbol_name": "bubble_sort",
            },
        ).json()
        # Debe encontrar al menos 2: definición + uso
        assert data["total"] >= 2

    def test_references_definition_line(self):
        data = client.post(
            "/intel/references",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "symbol_name": "bubble_sort",
            },
        ).json()
        assert data["definition_line"] == 15

    def test_references_method_call(self):
        data = client.post(
            "/intel/references",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "symbol_name": "process",
            },
        ).json()
        kinds = [r["kind"] for r in data["references"]]
        assert "definition" in kinds
        assert "call" in kinds

    def test_references_preview_present(self):
        data = client.post(
            "/intel/references",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "symbol_name": "bubble_sort",
            },
        ).json()
        for ref in data["references"]:
            assert "preview" in ref
            assert len(ref["preview"]) > 0

    def test_references_sorted_by_line(self):
        data = client.post(
            "/intel/references",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "symbol_name": "bubble_sort",
            },
        ).json()
        lines = [r["line"] for r in data["references"]]
        assert lines == sorted(lines)

    def test_references_response_structure(self):
        data = client.post(
            "/intel/references",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "symbol_name": "bubble_sort",
            },
        ).json()
        assert "symbol" in data
        assert "references" in data
        assert "definition_line" in data
        assert "total" in data
        for ref in data["references"]:
            assert "line" in ref
            assert "column" in ref
            assert "kind" in ref
            assert "preview" in ref

    def test_references_empty_symbol(self):
        data = client.post(
            "/intel/references",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "symbol_name": "",
            },
        ).json()
        assert data["total"] == 0

    def test_references_not_found(self):
        data = client.post(
            "/intel/references",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "symbol_name": "nonexistent_xyz_abc",
            },
        ).json()
        assert data["total"] == 0

    def test_references_class(self):
        data = client.post(
            "/intel/references",
            json={
                "filename": "test.py",
                "content": PY_DEFS,
                "symbol_name": "DataProcessor",
            },
        ).json()
        assert data["total"] >= 2  # definición + uso en dp = DataProcessor(...)
        assert data["definition_line"] == 4


# ─── /intel/references — TypeScript ──────────────────────────────────────────


class TestFindReferencesTS:
    def test_references_ts_ok(self):
        r = client.post(
            "/intel/references",
            json={
                "filename": "service.ts",
                "content": TS_DEFS,
                "symbol_name": "processUsers",
            },
        )
        assert r.status_code == 200

    def test_references_ts_finds_usage(self):
        data = client.post(
            "/intel/references",
            json={
                "filename": "service.ts",
                "content": TS_DEFS,
                "symbol_name": "processUsers",
            },
        ).json()
        assert data["total"] >= 2

    def test_references_ts_definition_line(self):
        data = client.post(
            "/intel/references",
            json={
                "filename": "service.ts",
                "content": TS_DEFS,
                "symbol_name": "processUsers",
            },
        ).json()
        assert data["definition_line"] is not None

    def test_references_ts_empty(self):
        data = client.post(
            "/intel/references",
            json={
                "filename": "service.ts",
                "content": TS_DEFS,
                "symbol_name": "nonExistentSymbol123",
            },
        ).json()
        assert data["total"] == 0


# ─── Fixtures Fase 3 ─────────────────────────────────────────────────────────

PY_COMPLETE = '''
import os
from typing import List, Dict

LIMIT = 100
MAX_SIZE = 200

class DataProcessor:
    """Procesa datos."""
    def __init__(self, data: List[int]):
        self.data = data
    def process(self) -> List[int]:
        return [x * 2 for x in self.data]
    def filter_big(self, threshold: int) -> List[int]:
        return [x for x in self.data if x > threshold]

def bubble_sort(arr: List[int]) -> List[int]:
    pass

def binary_search(arr: List[int], target: int) -> int:
    pass
'''

PY_RENAME = """
def bubble_sort(arr):
    n = len(arr)
    for i in range(n):
        for j in range(n - i - 1):
            if arr[j] > arr[j+1]:
                arr[j], arr[j+1] = arr[j+1], arr[j]

result = bubble_sort([3,1,2])
again  = bubble_sort([5,4,3])
data   = bubble_sort([9,8,7])
"""

TS_COMPLETE = """
import { readFile } from 'fs'

interface User { id: number; name: string }
type UserId = number

class UserService {
  findById(id: number): User | undefined { return undefined }
  addUser(user: User): void {}
}

function processUsers(service: UserService): void {}
function validateUser(user: User): boolean { return true }
"""


# ─── /intel/completions — Python ─────────────────────────────────────────────


class TestCompletionsPython:
    def test_completions_ok(self):
        r = client.post("/intel/completions", json={"filename": "test.py", "content": PY_COMPLETE, "prefix": ""})
        assert r.status_code == 200

    def test_completions_returns_symbols(self):
        data = client.post(
            "/intel/completions", json={"filename": "test.py", "content": PY_COMPLETE, "prefix": ""}
        ).json()
        assert "symbols" in data
        assert data["total"] >= 5

    def test_completions_finds_functions(self):
        data = client.post(
            "/intel/completions", json={"filename": "test.py", "content": PY_COMPLETE, "prefix": ""}
        ).json()
        labels = [s["label"] for s in data["symbols"]]
        assert "bubble_sort" in labels
        assert "binary_search" in labels

    def test_completions_finds_classes(self):
        data = client.post(
            "/intel/completions", json={"filename": "test.py", "content": PY_COMPLETE, "prefix": ""}
        ).json()
        labels = [s["label"] for s in data["symbols"]]
        assert "DataProcessor" in labels

    def test_completions_finds_imports(self):
        data = client.post(
            "/intel/completions", json={"filename": "test.py", "content": PY_COMPLETE, "prefix": ""}
        ).json()
        labels = [s["label"] for s in data["symbols"]]
        assert "os" in labels or "List" in labels

    def test_completions_finds_constants(self):
        data = client.post(
            "/intel/completions", json={"filename": "test.py", "content": PY_COMPLETE, "prefix": ""}
        ).json()
        labels = [s["label"] for s in data["symbols"]]
        assert "LIMIT" in labels or "MAX_SIZE" in labels

    def test_completions_prefix_filter(self):
        data = client.post(
            "/intel/completions", json={"filename": "test.py", "content": PY_COMPLETE, "prefix": "bu"}
        ).json()
        assert data["total"] >= 1
        for s in data["symbols"]:
            assert s["label"].lower().startswith("bu")

    def test_completions_prefix_no_match(self):
        data = client.post(
            "/intel/completions", json={"filename": "test.py", "content": PY_COMPLETE, "prefix": "zzznomatch"}
        ).json()
        assert data["total"] == 0

    def test_completions_symbol_structure(self):
        data = client.post(
            "/intel/completions", json={"filename": "test.py", "content": PY_COMPLETE, "prefix": ""}
        ).json()
        for s in data["symbols"]:
            assert "label" in s
            assert "kind" in s
            assert "detail" in s
            assert "insert_text" in s
            assert "line" in s
            assert s["kind"] in ("function", "method", "class", "variable", "import", "interface", "type")

    def test_completions_detail_has_signature(self):
        data = client.post(
            "/intel/completions", json={"filename": "test.py", "content": PY_COMPLETE, "prefix": "bubble"}
        ).json()
        if data["symbols"]:
            assert "bubble_sort" in data["symbols"][0]["detail"]

    def test_completions_empty_content(self):
        data = client.post("/intel/completions", json={"filename": "test.py", "content": "", "prefix": ""}).json()
        assert data["total"] == 0

    def test_completions_sorted_functions_first(self):
        data = client.post(
            "/intel/completions", json={"filename": "test.py", "content": PY_COMPLETE, "prefix": ""}
        ).json()
        kinds = [s["kind"] for s in data["symbols"]]
        # Funciones y métodos deben aparecer antes que imports
        func_indices = [i for i, k in enumerate(kinds) if k in ("function", "method")]
        import_indices = [i for i, k in enumerate(kinds) if k == "import"]
        if func_indices and import_indices:
            assert min(func_indices) < max(import_indices)


# ─── /intel/completions — TypeScript ─────────────────────────────────────────


class TestCompletionsTS:
    def test_completions_ts_ok(self):
        r = client.post("/intel/completions", json={"filename": "service.ts", "content": TS_COMPLETE, "prefix": ""})
        assert r.status_code == 200

    def test_completions_ts_finds_functions(self):
        data = client.post(
            "/intel/completions", json={"filename": "service.ts", "content": TS_COMPLETE, "prefix": ""}
        ).json()
        labels = [s["label"] for s in data["symbols"]]
        assert "processUsers" in labels or "validateUser" in labels

    def test_completions_ts_finds_classes(self):
        data = client.post(
            "/intel/completions", json={"filename": "service.ts", "content": TS_COMPLETE, "prefix": ""}
        ).json()
        labels = [s["label"] for s in data["symbols"]]
        assert "UserService" in labels

    def test_completions_ts_finds_interfaces(self):
        data = client.post(
            "/intel/completions", json={"filename": "service.ts", "content": TS_COMPLETE, "prefix": ""}
        ).json()
        labels = [s["label"] for s in data["symbols"]]
        assert "User" in labels

    def test_completions_ts_prefix(self):
        data = client.post(
            "/intel/completions", json={"filename": "service.ts", "content": TS_COMPLETE, "prefix": "User"}
        ).json()
        assert data["total"] >= 1
        for s in data["symbols"]:
            assert s["label"].lower().startswith("user")


# ─── /intel/rename — Python ──────────────────────────────────────────────────


class TestRenamePython:
    def test_rename_ok(self):
        r = client.post(
            "/intel/rename",
            json={
                "filename": "test.py",
                "content": PY_RENAME,
                "symbol_name": "bubble_sort",
                "new_name": "optimized_sort",
            },
        )
        assert r.status_code == 200

    def test_rename_valid(self):
        data = client.post(
            "/intel/rename",
            json={
                "filename": "test.py",
                "content": PY_RENAME,
                "symbol_name": "bubble_sort",
                "new_name": "optimized_sort",
            },
        ).json()
        assert data["valid"] is True

    def test_rename_returns_edits(self):
        data = client.post(
            "/intel/rename",
            json={
                "filename": "test.py",
                "content": PY_RENAME,
                "symbol_name": "bubble_sort",
                "new_name": "optimized_sort",
            },
        ).json()
        assert len(data["edits"]) >= 3  # def + 3 usos

    def test_rename_edit_structure(self):
        data = client.post(
            "/intel/rename",
            json={
                "filename": "test.py",
                "content": PY_RENAME,
                "symbol_name": "bubble_sort",
                "new_name": "optimized_sort",
            },
        ).json()
        for e in data["edits"]:
            assert "range" in e
            assert "newText" in e
            assert "kind" in e
            assert "preview" in e
            rng = e["range"]
            assert "startLineNumber" in rng
            assert "startColumn" in rng
            assert "endLineNumber" in rng
            assert "endColumn" in rng

    def test_rename_new_text_correct(self):
        data = client.post(
            "/intel/rename",
            json={
                "filename": "test.py",
                "content": PY_RENAME,
                "symbol_name": "bubble_sort",
                "new_name": "optimized_sort",
            },
        ).json()
        for e in data["edits"]:
            assert e["newText"] == "optimized_sort"

    def test_rename_includes_definition(self):
        data = client.post(
            "/intel/rename",
            json={
                "filename": "test.py",
                "content": PY_RENAME,
                "symbol_name": "bubble_sort",
                "new_name": "optimized_sort",
            },
        ).json()
        kinds = [e["kind"] for e in data["edits"]]
        assert "definition" in kinds

    def test_rename_sorted_by_line(self):
        data = client.post(
            "/intel/rename",
            json={
                "filename": "test.py",
                "content": PY_RENAME,
                "symbol_name": "bubble_sort",
                "new_name": "optimized_sort",
            },
        ).json()
        lines = [e["range"]["startLineNumber"] for e in data["edits"]]
        assert lines == sorted(lines)

    def test_rename_invalid_new_name_number(self):
        data = client.post(
            "/intel/rename",
            json={
                "filename": "test.py",
                "content": PY_RENAME,
                "symbol_name": "bubble_sort",
                "new_name": "123invalid",
            },
        ).json()
        assert data["valid"] is False
        assert "error" in data

    def test_rename_invalid_new_name_spaces(self):
        data = client.post(
            "/intel/rename",
            json={
                "filename": "test.py",
                "content": PY_RENAME,
                "symbol_name": "bubble_sort",
                "new_name": "my sort",
            },
        ).json()
        assert data["valid"] is False

    def test_rename_same_name(self):
        data = client.post(
            "/intel/rename",
            json={
                "filename": "test.py",
                "content": PY_RENAME,
                "symbol_name": "bubble_sort",
                "new_name": "bubble_sort",
            },
        ).json()
        assert data["valid"] is False

    def test_rename_not_found(self):
        data = client.post(
            "/intel/rename",
            json={
                "filename": "test.py",
                "content": PY_RENAME,
                "symbol_name": "nonexistent_xyz",
                "new_name": "new_name",
            },
        ).json()
        assert data["valid"] is False

    def test_rename_empty_symbol(self):
        data = client.post(
            "/intel/rename",
            json={
                "filename": "test.py",
                "content": PY_RENAME,
                "symbol_name": "",
                "new_name": "new_name",
            },
        ).json()
        assert data["valid"] is False

    def test_rename_total_count(self):
        data = client.post(
            "/intel/rename",
            json={
                "filename": "test.py",
                "content": PY_RENAME,
                "symbol_name": "bubble_sort",
                "new_name": "fast_sort",
            },
        ).json()
        assert data["total"] == len(data["edits"])
        assert data["old_name"] == "bubble_sort"
        assert data["new_name"] == "fast_sort"
