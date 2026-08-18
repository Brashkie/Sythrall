"""
Tests — Static Analysis Router (FastAPI)
pytest tests/test_static_analysis.py -v

El sidecar `complexity-engine` no corre durante `pytest` en CI, así que
`/static/bigO` sobre archivos `.py` ejercita el fallback a `parse_file()`
(Python) real — no un mock. Confirma que el wiring de la Fase 1 de migración
a Rust (services/complexity_client.py::parse_python_rich, ver
routers/static_analysis.py::analyze_big_o) no rompió el comportamiento
existente cuando el sidecar no está disponible.
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from fastapi.testclient import TestClient

from main import app

client = TestClient(app)

PY_NESTED_LOOPS = """
def bubble_sort(arr):
    n = len(arr)
    for i in range(n):
        for j in range(n - i - 1):
            if arr[j] > arr[j+1]:
                arr[j], arr[j+1] = arr[j+1], arr[j]
    return arr
"""


class TestBigO:
    def test_bigo_ok(self):
        r = client.post("/static/bigO", json={"filename": "test.py", "content": PY_NESTED_LOOPS})
        assert r.status_code == 200

    def test_bigo_detects_on2(self):
        data = client.post("/static/bigO", json={"filename": "test.py", "content": PY_NESTED_LOOPS}).json()
        fn = next((f for f in data["functions"] if f["function"] == "bubble_sort"), None)
        assert fn is not None
        assert fn["big_o"] == "O(n²)"

    def test_bigo_language_python(self):
        data = client.post("/static/bigO", json={"filename": "test.py", "content": PY_NESTED_LOOPS}).json()
        assert data["language"] == "python"

    def test_bigo_empty_file(self):
        r = client.post("/static/bigO", json={"filename": "empty.py", "content": ""})
        assert r.status_code == 200
        assert r.json()["functions"] == []

    def test_bigo_non_python_still_works(self):
        ts_src = "function bubbleSort(arr) {\n  for (let i = 0; i < arr.length; i++) {\n    for (let j = 0; j < arr.length; j++) {}\n  }\n}\n"
        r = client.post("/static/bigO", json={"filename": "test.ts", "content": ts_src})
        assert r.status_code == 200


class TestCyclomaticComplexityElseIf:
    """Regresión: `else if` no debe contarse dos veces (una vez por " if "
    como substring de "else if", y otra vez aparte) — 1 if + 2 else-if debe
    dar CC=4, no CC=6."""

    def test_js_else_if_not_double_counted(self):
        js_src = """
function classify(x) {
  if (x > 10) { return 1; }
  else if (x > 5) { return 2; }
  else if (x > 0) { return 3; }
  return 0;
}
"""
        r = client.post("/static/parse", json={"filename": "test.js", "content": js_src})
        assert r.status_code == 200
        fn = next((f for f in r.json()["functions"] if f["name"] == "classify"), None)
        assert fn is not None
        assert fn["complexity"] == 4

    def test_c_else_if_not_double_counted(self):
        c_src = """
int classify(int x) {
    if (x > 10) { return 1; }
    else if (x > 5) { return 2; }
    else if (x > 0) { return 3; }
    return 0;
}
"""
        r = client.post("/static/parse", json={"filename": "test.c", "content": c_src})
        assert r.status_code == 200
        fn = next((f for f in r.json()["functions"] if f["name"] == "classify"), None)
        assert fn is not None
        assert fn["complexity"] == 4


class TestJsArrowFunctionLoc:
    """Regresión: una arrow function sin `{ }` (cuerpo de una sola expresión)
    no debe absorber el LOC/Big-O de la SIGUIENTE función del archivo."""

    def test_braceless_arrow_does_not_absorb_next_function(self):
        js_src = (
            "const isEven = (n) => n % 2 === 0;\n"
            "function bigLoop() {\n"
            "  for (let i = 0; i < n; i++) { for (let j = 0; j < n; j++) { doWork(i, j); } }\n"
            "}\n"
        )
        r = client.post("/static/parse", json={"filename": "test.js", "content": js_src})
        assert r.status_code == 200
        functions = {f["name"]: f for f in r.json()["functions"]}
        assert functions["isEven"]["loc"] == 1
        assert functions["isEven"]["big_o"] == "O(1)"
        assert functions["bigLoop"]["big_o"] == "O(n²)"

    def test_multiline_signature_still_measured_correctly(self):
        js_src = "function longSignature(\n  a, b, c\n) {\n  return a + b + c;\n}\n"
        r = client.post("/static/parse", json={"filename": "test.js", "content": js_src})
        assert r.status_code == 200
        fn = next((f for f in r.json()["functions"] if f["name"] == "longSignature"), None)
        assert fn is not None
        assert fn["loc"] == 5


class TestSpaceComplexity:
    """Fase 13: complejidad de espacio, paralela al motor de Big-O de
    tiempo — mismos 8 casos verificados en Rust (`space::tests`), acá contra
    el fallback Python real (sin sidecar)."""

    def _space_of(self, src: str, name: str) -> str:
        r = client.post("/static/parse", json={"filename": "test.py", "content": src})
        assert r.status_code == 200
        fn = next((f for f in r.json()["functions"] if f["name"] == name), None)
        assert fn is not None
        return fn["space_complexity"]

    def test_scalar_accumulator_is_o1(self):
        src = "def total_de(arr):\n    total = 0\n    for x in arr:\n        total += x\n    return total\n"
        assert self._space_of(src, "total_de") == "O(1)"

    def test_append_to_list_is_on(self):
        src = "def copiar(arr):\n    out = []\n    for x in arr:\n        out.append(x)\n    return out\n"
        assert self._space_of(src, "copiar") == "O(n)"

    def test_list_comprehension_is_on(self):
        src = "def duplicar(arr):\n    return [x * 2 for x in arr]\n"
        assert self._space_of(src, "duplicar") == "O(n)"

    def test_matrix_from_nested_loops_is_on2(self):
        src = (
            "def matriz(n):\n"
            "    m = []\n"
            "    for i in range(n):\n"
            "        row = []\n"
            "        for j in range(n):\n"
            "            row.append(0)\n"
            "        m.append(row)\n"
            "    return m\n"
        )
        assert self._space_of(src, "matriz") == "O(n²)"

    def test_nested_comprehension_is_on2(self):
        src = "def matriz_comp(n):\n    return [[0 for _ in range(n)] for _ in range(n)]\n"
        assert self._space_of(src, "matriz_comp") == "O(n²)"

    def test_binary_recursion_is_ologn(self):
        src = (
            "def binary_search(arr, lo, hi):\n"
            "    if lo >= hi:\n"
            "        return -1\n"
            "    mid = (lo + hi) // 2\n"
            "    return binary_search(arr, lo, mid - 1)\n"
        )
        assert self._space_of(src, "binary_search") == "O(log n)"

    def test_linear_recursion_is_on(self):
        src = "def factorial(n):\n    if n <= 1:\n        return 1\n    return n * factorial(n - 1)\n"
        assert self._space_of(src, "factorial") == "O(n)"

    def test_dict_built_by_subscript_is_on(self):
        src = (
            "def contar(items):\n"
            "    counts = {}\n"
            "    for x in items:\n"
            "        counts[x] = counts.get(x, 0) + 1\n"
            "    return counts\n"
        )
        assert self._space_of(src, "contar") == "O(n)"
