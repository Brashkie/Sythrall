"""
Tests — Static Analysis Router (FastAPI)
pytest tests/test_static_analysis.py -v

`conftest.py` levanta el sidecar Rust (`complexity-engine`) para toda la
sesión de pytest — Big-O/complejidad/space/recursión/security/smells para
`.py` son Rust-only (ver `static_parser.py::_parse_python`), así que estos
tests ejercitan el motor real vía HTTP, no un fallback Python ni un mock.
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


class TestRecurrenceRelations:
    """Fase 13: reconocimiento de recurrencias divide-and-conquer
    (T(n) = aT(n/b) + f(n)), resuelto vía el Teorema Maestro — Rust-only
    (`services/complexity/src/bigo.rs::resolve_master_theorem`), sin
    duplicar la lógica en el fallback puro-Python: Rust es el camino
    principal (el sidecar corre siempre que está disponible, y `conftest.py`
    lo levanta para toda la sesión de pytest) y el costo de mantener dos
    implementaciones completas en paridad no se justifica para algo que solo
    se necesitaría en el fallback degradado. Los mismos 4 casos ya están
    cubiertos en `rich::tests` vía `cargo test` — acá se confirma que llegan
    igual vía HTTP end-to-end (`/static/parse`, `/intel/hover`)."""

    MERGE_SORT = (
        "def merge(left, right):\n"
        "    result = []\n"
        "    i = 0\n"
        "    for x in left:\n"
        "        result.append(x)\n"
        "    return result\n"
        "\n"
        "def merge_sort(arr):\n"
        "    if len(arr) <= 1:\n"
        "        return arr\n"
        "    mid = len(arr) // 2\n"
        "    left = merge_sort(arr[:mid])\n"
        "    right = merge_sort(arr[mid:])\n"
        "    return merge(left, right)\n"
    )

    def _fn(self, src: str, name: str) -> dict:
        r = client.post("/static/parse", json={"filename": "test.py", "content": src})
        assert r.status_code == 200
        fn = next((f for f in r.json()["functions"] if f["name"] == name), None)
        assert fn is not None
        return fn

    def test_merge_sort_es_onlogn_via_helper_interprocedural(self):
        fn = self._fn(self.MERGE_SORT, "merge_sort")
        assert fn["big_o"] == "O(n log n)"
        assert fn["recurrence"] == "T(n) = 2T(n/2) + Θ(n)"

    def test_non_divide_and_conquer_recursion_has_no_recurrence(self):
        src = "def factorial(n):\n    if n <= 1:\n        return 1\n    return n * factorial(n - 1)\n"
        fn = self._fn(src, "factorial")
        assert fn["recurrence"] is None

    def test_iterative_function_has_no_recurrence(self):
        src = "def total_de(arr):\n    total = 0\n    for x in arr:\n        total += x\n    return total\n"
        fn = self._fn(src, "total_de")
        assert fn["recurrence"] is None

    def test_hover_markdown_includes_recurrence_row(self):
        r = client.post(
            "/intel/hover",
            json={"filename": "test.py", "content": self.MERGE_SORT, "line": 8, "column": 5},
        )
        assert r.status_code == 200
        md = r.json().get("markdown", "")
        assert "T(n) = 2T(n/2)" in md


class TestProjectHealth:
    """Fase 2 del rediseño UX: /static/parse-project agrega security_findings/
    structural_smells a nivel de proyecto (antes solo per-archivo) y computa
    4 scores de Project Health, cada uno con sus números crudos al lado."""

    SQLI_FILE = (
        "def run_query(request):\n"
        "    username = request.args.get('username')\n"
        '    query = "SELECT * FROM users WHERE name = \'" + username + "\'"\n'
        "    cursor.execute(query)\n"
    )
    LONG_FN_FILE = "def big(a, b, c, d, e, f):\n" + "    x = 1\n" * 55 + "    return x\n"

    def _parse_project(self, files: list[dict]) -> dict:
        r = client.post("/static/parse-project", json={"files": files})
        assert r.status_code == 200
        return r.json()

    def test_clean_project_has_all_scores_at_100(self):
        data = self._parse_project([{"filename": "clean.py", "content": "def add(a, b):\n    return a + b\n"}])
        h = data["health"]
        assert h["security"]["score"] == 100
        assert h["quality"]["score"] == 100
        assert h["complexity"]["score"] == 100
        assert h["architecture"]["score"] == 100

    def test_empty_project_does_not_crash_and_scores_100(self):
        data = self._parse_project([])
        assert data["health"]["complexity"]["avg_complexity"] == 0.0
        assert data["health"]["complexity"]["score"] == 100

    def test_security_findings_aggregated_with_file(self):
        data = self._parse_project(
            [
                {"filename": "sqli.py", "content": self.SQLI_FILE},
                {"filename": "clean.py", "content": "def add(a, b):\n    return a + b\n"},
            ]
        )
        findings = data["security_findings"]
        assert len(findings) == 1
        assert findings[0]["file"] == "sqli.py"
        assert findings[0]["severity"] == "High"
        assert data["health"]["security"]["high"] == 1
        assert data["health"]["security"]["score"] == 85  # 100 - 15
        assert data["summary"]["security_findings"] == 1

    def test_structural_smells_aggregated_with_file(self):
        data = self._parse_project([{"filename": "long.py", "content": self.LONG_FN_FILE}])
        smells = data["structural_smells"]
        assert {s["kind"] for s in smells} == {"long_function", "excessive_parameters"}
        assert all(s["file"] == "long.py" for s in smells)
        assert data["health"]["quality"]["smells"] == len(smells)
        assert data["health"]["quality"]["score"] < 100
        assert data["summary"]["structural_smells"] == len(smells)

    def test_naming_smells_aggregated_with_file(self):
        # `x` de una sola letra, asignada muchas veces dentro de una función
        # larga — el mismo archivo que ya dispara structural_smells también
        # dispara naming_smells, y ambos se cuentan por separado.
        data = self._parse_project([{"filename": "long.py", "content": self.LONG_FN_FILE}])
        naming = data["naming_smells"]
        assert {s["kind"] for s in naming} == {"single_letter_name"}
        assert all(s["file"] == "long.py" for s in naming)
        assert data["health"]["quality"]["naming"] == len(naming)
        assert data["summary"]["naming_smells"] == len(naming)

    def test_circular_dependency_penalizes_architecture_score(self):
        data = self._parse_project(
            [
                {"filename": "circular_a.py", "content": "import circular_b\n"},
                {"filename": "circular_b.py", "content": "import circular_a\n"},
            ]
        )
        assert data["health"]["architecture"]["cycles"] >= 1
        assert data["health"]["architecture"]["score"] < 100

    def test_architecture_circular_dependency_produces_smell(self):
        # Mismo fixture que el test de arriba, pero verificando la Fase 22:
        # el ciclo ahora TAMBIÉN aparece reencuadrado como architecture_smell
        # (antes solo vivía en el grafo `circular`). health.architecture.smells
        # tiene que quedar en 0 acá — ya está penalizado por `cycles`, contarlo
        # de nuevo sería un doble castigo por el mismo hallazgo.
        data = self._parse_project(
            [
                {"filename": "circular_a.py", "content": "import circular_b\n"},
                {"filename": "circular_b.py", "content": "import circular_a\n"},
            ]
        )
        smells = data["architecture_smells"]
        assert any(s["kind"] == "circular_dependency" for s in smells)
        assert data["summary"]["architecture_smells"] >= 1
        assert data["health"]["architecture"]["smells"] == 0

    def test_architecture_high_efferent_coupling_detected(self):
        # hub_importer.py importa 16 archivos del proyecto (> el umbral de 15,
        # calibrado arriba de apps/api/main.py que hoy importa 11 como
        # composition root legítimo).
        targets = [{"filename": f"target_{i}.py", "content": f"x = {i}\n"} for i in range(16)]
        hub_content = "".join(f"import target_{i}\n" for i in range(16))
        data = self._parse_project([{"filename": "hub_importer.py", "content": hub_content}, *targets])
        smells = [s for s in data["architecture_smells"] if s["kind"] == "high_efferent_coupling"]
        assert len(smells) == 1
        assert smells[0]["name"] == "hub_importer.py"
        assert smells[0]["line"] == 0
        assert data["health"]["architecture"]["smells"] >= 1

    def test_architecture_unstable_dependency_detected(self):
        # core.py: 3 dependientes (Ca=3) que a la vez importa 4 helpers
        # (Ce=4) → inestabilidad 4/(3+4) ≈ 0.57 > 0.5 — un módulo muy usado
        # que además es frágil (depende de más de lo que debería para algo
        # tan central).
        helpers = [{"filename": f"helper_{i}.py", "content": f"x = {i}\n"} for i in range(4)]
        core_content = "".join(f"import helper_{i}\n" for i in range(4))
        dependents = [{"filename": f"dep_{c}.py", "content": "import core\n"} for c in ("a", "b", "c")]
        data = self._parse_project([{"filename": "core.py", "content": core_content}, *helpers, *dependents])
        smells = [s for s in data["architecture_smells"] if s["kind"] == "unstable_dependency"]
        assert len(smells) == 1
        assert smells[0]["name"] == "core.py"
        assert data["health"]["architecture"]["smells"] >= 1

    def test_architecture_smells_have_no_file_field(self):
        # A diferencia de structural_smells/naming_smells (que se agregan por
        # archivo, con un campo `file` tackeado), architecture_smells ya son
        # globales — `name` lleva la ruta completa por sí solo.
        targets = [{"filename": f"target_{i}.py", "content": f"x = {i}\n"} for i in range(16)]
        hub_content = "".join(f"import target_{i}\n" for i in range(16))
        data = self._parse_project([{"filename": "hub_importer.py", "content": hub_content}, *targets])
        assert data["architecture_smells"]
        assert all("file" not in s for s in data["architecture_smells"])

    def test_project_wide_avg_complexity_flattens_all_functions(self):
        # 1 archivo con 1 función O(1) + 1 archivo con función de loop anidado
        # (CC mayor) — el promedio debe combinar TODAS las funciones, no
        # promediar primero por archivo.
        data = self._parse_project(
            [
                {"filename": "simple.py", "content": "def f():\n    return 1\n"},
                {"filename": "loops.py", "content": PY_NESTED_LOOPS},
            ]
        )
        assert data["health"]["complexity"]["avg_complexity"] > 1.0

    def test_top_complex_functions_sorted_desc_across_files(self):
        # Fase 2 del rediseño UX — widget "Complexity by Function" del
        # Dashboard: aplanado entre archivos, ordenado desc por complejidad.
        data = self._parse_project(
            [
                {"filename": "a.py", "content": PY_NESTED_LOOPS},
                {"filename": "b.py", "content": "def simple():\n    return 1\n"},
            ]
        )
        top = data["top_complex_functions"]
        assert top
        assert all(top[i]["complexity"] >= top[i + 1]["complexity"] for i in range(len(top) - 1))
        assert top[0]["name"] == "bubble_sort"
        assert top[0]["file"] == "a.py"
        assert {"file", "name", "line", "complexity", "big_o"} <= set(top[0].keys())

    def test_top_complex_functions_capped_at_ten(self):
        files = [{"filename": f"f{i}.py", "content": f"def fn{i}():\n    return {i}\n"} for i in range(15)]
        data = self._parse_project(files)
        assert len(data["top_complex_functions"]) == 10

    def test_language_distribution_is_real_loc_not_estimated(self):
        # Fase 2 del rediseño UX — widget "Languages" del Dashboard: LOC real
        # contado sobre el contenido, no una estimación.
        py_content = "def a():\n    return 1\n"  # 2 líneas (2 \n, criterio wc -l)
        ts_content = "function b() {\n  return 2\n}\n"  # 3 líneas
        data = self._parse_project(
            [
                {"filename": "a.py", "content": py_content},
                {"filename": "b.ts", "content": ts_content},
            ]
        )
        dist = data["language_distribution"]
        assert dist["python"] == {"files": 1, "loc": 2, "functions": 1}
        assert dist["typescript"] == {"files": 1, "loc": 3, "functions": 1}
        assert data["summary"]["total_loc"] == 5

    def test_language_distribution_groups_multiple_files_same_language(self):
        data = self._parse_project(
            [
                {"filename": "a.py", "content": "def a():\n    return 1\n"},
                {"filename": "b.py", "content": "def b():\n    return 2\n"},
            ]
        )
        assert data["language_distribution"]["python"]["files"] == 2
        assert data["language_distribution"]["python"]["functions"] == 2

    def test_health_scores_never_negative(self):
        # Muchos findings High + muchos smells — el clamp a 0 no debe fallar.
        files = [{"filename": f"vuln{i}.py", "content": self.SQLI_FILE} for i in range(10)]
        data = self._parse_project(files)
        for key in ("security", "quality", "complexity", "architecture"):
            assert data["health"][key]["score"] >= 0
