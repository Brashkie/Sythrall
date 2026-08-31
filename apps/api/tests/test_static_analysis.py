"""
Tests — Project Health scoring (routers/static_analysis.py::parse_project)

Todo lo que este archivo probaba antes (Big-O, purity, WASM hints, memory
layout, modernization, Assembly, Fortran, etc.) era puro relay a Rust — cada
campo llegaba verbatim desde `parse_python_rich`/`parse_c_rust`/etc. sin
ningún cómputo propio de Python, ya cubierto por el `cargo test` del módulo
Rust correspondiente. Esos tests se eliminaron (2026-08-31, pedido explícito
del usuario de aplicar la política "cero tests de Python para código
Rust-only" a TODO apps/api/tests/, no solo a un archivo). Lo único que queda
acá es `TestProjectHealth`: los 4 scores de Project Health (security/
quality/complexity/architecture), `top_complex_functions`, y
`language_distribution` son aritmética y agregación real de Python sobre
datos ya parseados por archivo — Rust no tiene ningún equivalente de esto,
así que es la única parte de este archivo que de verdad necesitaba un test
en Python.
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
