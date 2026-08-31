"""
Tests — Analysis, ML, Diagram routers
pytest tests/test_analysis.py -v
"""

import pytest
from fastapi.testclient import TestClient

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

import services.complexity_client as complexity_client
from main import app

client = TestClient(app)

_UNREACHABLE_URL = "http://127.0.0.1:1"


# ─── /analyze/code ────────────────────────────────────────────────────────────


class TestAnalyzeCode:
    SIMPLE_PY = "def hello(world):\n    print(world)\n"
    SYNTAX_ERR = "def bad(\n    pass\n"
    TYPED_PY = '''
def add(a: int, b: int) -> int:
    """Suma dos enteros."""
    return a + b

x = add(1, 2)
'''

    def test_analyze_valid_python(self):
        res = client.post(
            "/analyze/code",
            json={
                "filename": "hello.py",
                "content": self.SIMPLE_PY,
                "tools": ["ast"],
            },
        )
        assert res.status_code == 200
        data = res.json()
        assert data["filename"] == "hello.py"
        assert isinstance(data["issues"], list)
        assert "ast" in data["tools_used"]

    def test_analyze_detects_print(self):
        res = client.post(
            "/analyze/code",
            json={
                "filename": "hello.py",
                "content": self.SIMPLE_PY,
                "tools": ["ast"],
            },
        )
        issues = res.json()["issues"]
        print_issues = [i for i in issues if i["code"] == "C002"]
        assert len(print_issues) >= 1

    def test_analyze_detects_missing_docstring(self):
        res = client.post(
            "/analyze/code",
            json={
                "filename": "hello.py",
                "content": self.SIMPLE_PY,
                "tools": ["ast"],
            },
        )
        issues = res.json()["issues"]
        doc_issues = [i for i in issues if i["code"] == "C001"]
        assert len(doc_issues) >= 1

    def test_analyze_no_docstring_warning_for_documented(self):
        res = client.post(
            "/analyze/code",
            json={
                "filename": "good.py",
                "content": self.TYPED_PY,
                "tools": ["ast"],
            },
        )
        issues = res.json()["issues"]
        # add() tiene docstring → no debe generar C001 para esa función
        doc_issues = [i for i in issues if i["code"] == "C001" and "add" in i.get("message", "")]
        assert len(doc_issues) == 0

    def test_analyze_syntax_error(self):
        res = client.post(
            "/analyze/code",
            json={
                "filename": "bad.py",
                "content": self.SYNTAX_ERR,
                "tools": ["ast"],
            },
        )
        assert res.status_code == 200
        issues = res.json()["issues"]
        error_issues = [i for i in issues if i["severity"] == "error"]
        assert len(error_issues) >= 1
        assert error_issues[0]["code"] == "E001"

    def test_analyze_generic_except(self):
        code = "try:\n    pass\nexcept:\n    pass\n"
        res = client.post(
            "/analyze/code",
            json={
                "filename": "bad.py",
                "content": code,
                "tools": ["ast"],
            },
        )
        issues = res.json()["issues"]
        assert any(i["code"] == "W002" for i in issues)

    def test_analyze_global_variable(self):
        code = "global x\nx = 1\n"
        res = client.post(
            "/analyze/code",
            json={
                "filename": "bad.py",
                "content": code,
                "tools": ["ast"],
            },
        )
        issues = res.json()["issues"]
        assert any(i["code"] == "W003" for i in issues)

    def test_analyze_long_line(self):
        long_line = "x = " + "a" * 130 + "\n"
        res = client.post(
            "/analyze/code",
            json={
                "filename": "bad.py",
                "content": long_line,
                "tools": ["ast"],
            },
        )
        issues = res.json()["issues"]
        assert any(i["code"] == "E501" for i in issues)

    def test_analyze_json_valid(self):
        res = client.post(
            "/analyze/code",
            json={
                "filename": "data.json",
                "content": '{"key": "value"}',
                "tools": ["ast"],
            },
        )
        assert res.status_code == 200
        assert res.json()["issues"] == []

    def test_analyze_json_invalid(self):
        res = client.post(
            "/analyze/code",
            json={
                "filename": "bad.json",
                "content": "{bad json}",
                "tools": ["ast"],
            },
        )
        assert res.status_code == 200
        issues = res.json()["issues"]
        assert any(i["tool"] == "json" for i in issues)

    def test_analyze_issues_sorted_by_line(self):
        code = "\n".join(["def f():", "    print(1)", "    print(2)", "    print(3)"])
        res = client.post(
            "/analyze/code",
            json={
                "filename": "f.py",
                "content": code,
                "tools": ["ast"],
            },
        )
        issues = [i for i in res.json()["issues"] if i.get("line")]
        lines = [i["line"] for i in issues]
        assert lines == sorted(lines)

    def test_analyze_result_has_all_keys(self):
        res = client.post(
            "/analyze/code",
            json={
                "filename": "x.py",
                "content": "x = 1\n",
                "tools": ["ast"],
            },
        )
        data = res.json()
        for key in (
            "filename",
            "ts",
            "issues",
            "metrics",
            "complexity",
            "maintainability",
            "halstead",
            "raw_stats",
            "tools_used",
        ):
            assert key in data, f"Falta clave: {key}"

    def test_analyze_empty_content(self):
        res = client.post(
            "/analyze/code",
            json={
                "filename": "empty.py",
                "content": "",
                "tools": ["ast"],
            },
        )
        assert res.status_code == 200
        assert res.json()["issues"] == []


# ─── /analyze/project ─────────────────────────────────────────────────────────


class TestAnalyzeProject:
    """Versión batch de /analyze/code — flake8/pylint corren una vez para todos
    los archivos en vez de un subprocess por archivo. Ver docstring del endpoint."""

    def test_returns_one_entry_per_file(self):
        res = client.post(
            "/analyze/project",
            json={
                "files": [
                    {"filename": "a.py", "content": "x = 1\n"},
                    {"filename": "b.py", "content": "y = 2\n"},
                ],
                "tools": ["ast"],
            },
        )
        assert res.status_code == 200
        data = res.json()["files"]
        assert set(data.keys()) == {"a.py", "b.py"}
        assert data["a.py"]["filename"] == "a.py"
        assert data["b.py"]["filename"] == "b.py"

    def test_ast_issues_attributed_to_correct_file(self):
        res = client.post(
            "/analyze/project",
            json={
                "files": [
                    {"filename": "clean.py", "content": "def f(x: int) -> int:\n    return x\n"},
                    {"filename": "prints.py", "content": "def f(x):\n    print(x)\n"},
                ],
                "tools": ["ast"],
            },
        )
        data = res.json()["files"]
        clean_prints = [i for i in data["clean.py"]["issues"] if i["code"] == "C002"]
        dirty_prints = [i for i in data["prints.py"]["issues"] if i["code"] == "C002"]
        assert clean_prints == []
        assert len(dirty_prints) >= 1

    def test_flake8_issues_attributed_to_correct_file(self):
        res = client.post(
            "/analyze/project",
            json={
                "files": [
                    {"filename": "ok.py", "content": "x = 1\n"},
                    {"filename": "bad.py", "content": "import os\nx=1\n"},  # F401 + E225
                ],
                "tools": ["flake8"],
            },
        )
        data = res.json()["files"]
        assert data["bad.py"]["issues"], "bad.py debería tener issues de flake8"
        assert all(i["tool"] == "flake8" for i in data["bad.py"]["issues"])

    def test_non_python_files_get_empty_result_without_crashing(self):
        res = client.post(
            "/analyze/project",
            json={
                "files": [
                    {"filename": "app.ts", "content": "export const x = 1\n"},
                    {"filename": "real.py", "content": "x = 1\n"},
                ],
                "tools": ["ast", "flake8", "pylint", "complexity"],
            },
        )
        assert res.status_code == 200
        data = res.json()["files"]
        assert data["app.ts"]["issues"] == []
        assert data["app.ts"]["tools_used"] == []

    def test_empty_project(self):
        res = client.post("/analyze/project", json={"files": [], "tools": ["ast"]})
        assert res.status_code == 200
        assert res.json()["files"] == {}


# ─── /analyze/ml ──────────────────────────────────────────────────────────────


class TestAnalyzeML:
    """La detección propiamente dicha (libraries/pipeline/models/metrics/
    issues) es relay puro a `services/complexity/src/ml.rs` — `_analyze_ml_sync`
    toma `detection` verbatim, solo agrega `version` por librería. Esos tests
    se eliminaron (2026-08-31, política de toda la carpeta) — `ml.rs` tiene
    47 tests propios cubriendo cada detector con más profundidad de la que
    estos alcanzaban por HTTP. Lo que queda es genuinamente Python:
    `_ml_score`/`_ml_diagram` (aritmética y armado de Mermaid sobre datos ya
    detectados, sin equivalente en Rust) y los 2 tests de degradación al
    fallback Python (`_detect_*_fallback`), que Rust no puede probar por
    definición — solo corren cuando el sidecar NO responde."""

    SKLEARN_CODE = """
import numpy as np
import pandas as pd
from sklearn.model_selection import train_test_split
from sklearn.preprocessing import StandardScaler
from sklearn.ensemble import RandomForestClassifier
from sklearn.metrics import accuracy_score

df = pd.read_csv('data.csv')
X = df.drop('target', axis=1)
y = df['target']

X_train, X_test, y_train, y_test = train_test_split(X, y, random_state=42)
scaler = StandardScaler()
X_train = scaler.fit_transform(X_train)
X_test = scaler.transform(X_test)

model = RandomForestClassifier(random_state=42)
model.fit(X_train, y_train)
preds = model.predict(X_test)
score = accuracy_score(y_test, preds)
"""

    def test_ml_score_range(self):
        res = client.post(
            "/analyze/ml",
            json={
                "filename": "model.py",
                "content": self.SKLEARN_CODE,
            },
        )
        score = res.json()["score"]
        assert 0 <= score <= 100

    def test_ml_generates_mermaid_diagram(self):
        res = client.post(
            "/analyze/ml",
            json={
                "filename": "model.py",
                "content": self.SKLEARN_CODE,
            },
        )
        diagram = res.json()["diagram"]
        assert diagram.startswith("flowchart TD")
        assert "Pipeline" in diagram

    def test_ml_empty_content_no_crash(self):
        res = client.post("/analyze/ml", json={"filename": "empty.py", "content": ""})
        assert res.status_code == 200
        data = res.json()
        assert data["libraries"] == []
        assert data["pipeline"] == []

    def test_ml_result_has_all_keys(self):
        res = client.post(
            "/analyze/ml",
            json={
                "filename": "m.py",
                "content": self.SKLEARN_CODE,
            },
        )
        data = res.json()
        for key in (
            "filename",
            "ts",
            "libraries",
            "pipeline",
            "issues",
            "metrics",
            "models",
            "diagram",
            "score",
            "suggestions",
        ):
            assert key in data, f"Falta clave: {key}"

    def test_ml_suggestions_list(self):
        res = client.post(
            "/analyze/ml",
            json={
                "filename": "model.py",
                "content": self.SKLEARN_CODE,
            },
        )
        suggestions = res.json()["suggestions"]
        assert isinstance(suggestions, list)
        assert len(suggestions) <= 8

    def test_ml_degrades_to_fallback_when_sidecar_unreachable(self, monkeypatch):
        """Sin sidecar, los 4 detectores caen a `_detect_*_fallback` (el
        `ast.walk`/regex original) y siguen produciendo el mismo shape —
        mismo criterio que el resto del engine (diagram.py, heatmap)."""
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        res = client.post(
            "/analyze/ml",
            json={"filename": "model.py", "content": self.SKLEARN_CODE},
        )
        assert res.status_code == 200
        data = res.json()
        names = {lib["name"] for lib in data["libraries"]}
        assert {"NumPy", "Pandas", "Scikit-learn"}.issubset(names)
        assert len(data["pipeline"]) > 0

    def test_ml_fallback_still_reports_syntax_error_correctly(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        res = client.post(
            "/analyze/ml",
            json={"filename": "bad.py", "content": "def bad(\n"},
        )
        data = res.json()
        assert data["libraries"] == []
        assert any("SyntaxError" in issue["message"] for issue in data["issues"])


# ─── /analyze/diagram ─────────────────────────────────────────────────────────


class TestAnalyzeDiagram:
    SIMPLE_CODE = """
def load_data(path):
    \"\"\"Carga datos.\"\"\"
    return open(path).read()

def process(data):
    return data.strip()

async def fetch(url):
    return url
"""

    CLASS_CODE = """
class Animal:
    def __init__(self, name):
        self.name = name
    def speak(self):
        pass

class Dog(Animal):
    def speak(self):
        return 'woof'
"""

    def test_flowchart_default(self):
        res = client.post(
            "/analyze/diagram",
            json={
                "filename": "script.py",
                "content": self.SIMPLE_CODE,
                "diagram_type": "flowchart",
            },
        )
        assert res.status_code == 200
        data = res.json()
        assert data["mermaid"].startswith("flowchart TD")
        assert "load_data" in data["mermaid"]
        assert "process" in data["mermaid"]

    def test_flowchart_async_function_styled(self):
        res = client.post(
            "/analyze/diagram",
            json={
                "filename": "script.py",
                "content": self.SIMPLE_CODE,
                "diagram_type": "flowchart",
            },
        )
        mermaid = res.json()["mermaid"]
        # fetch es async → debe tener estilo morado (#b87dff)
        assert "#b87dff" in mermaid

    def test_flowchart_empty_file(self):
        res = client.post(
            "/analyze/diagram",
            json={
                "filename": "empty.py",
                "content": "",
                "diagram_type": "flowchart",
            },
        )
        assert res.status_code == 200
        assert "Sin funciones" in res.json()["mermaid"]

    def test_flowchart_syntax_error(self):
        res = client.post(
            "/analyze/diagram",
            json={
                "filename": "bad.py",
                "content": "def bad(\n",
                "diagram_type": "flowchart",
            },
        )
        assert res.status_code == 200
        assert "SyntaxError" in res.json()["mermaid"]

    def test_callgraph_detects_calls(self):
        code = "def a():\n    b()\n\ndef b():\n    pass\n"
        res = client.post(
            "/analyze/diagram",
            json={
                "filename": "calls.py",
                "content": code,
                "diagram_type": "callgraph",
            },
        )
        mermaid = res.json()["mermaid"]
        assert "graph LR" in mermaid
        assert "llama" in mermaid

    def test_callgraph_no_functions(self):
        res = client.post(
            "/analyze/diagram",
            json={
                "filename": "x.py",
                "content": "x = 1\n",
                "diagram_type": "callgraph",
            },
        )
        assert "Sin funciones" in res.json()["mermaid"]

    def test_classes_diagram(self):
        res = client.post(
            "/analyze/diagram",
            json={
                "filename": "classes.py",
                "content": self.CLASS_CODE,
                "diagram_type": "classes",
            },
        )
        mermaid = res.json()["mermaid"]
        assert "classDiagram" in mermaid
        assert "Animal" in mermaid
        assert "Dog" in mermaid
        assert "hereda" in mermaid

    def test_classes_attributes(self):
        res = client.post(
            "/analyze/diagram",
            json={
                "filename": "classes.py",
                "content": self.CLASS_CODE,
                "diagram_type": "classes",
            },
        )
        mermaid = res.json()["mermaid"]
        assert "name" in mermaid  # atributo de instancia self.name

    def test_sequence_diagram(self):
        res = client.post(
            "/analyze/diagram",
            json={
                "filename": "seq.py",
                "content": self.SIMPLE_CODE,
                "diagram_type": "sequence",
            },
        )
        mermaid = res.json()["mermaid"]
        assert "sequenceDiagram" in mermaid
        assert "Usuario" in mermaid
        assert "invocar" in mermaid

    def test_sequence_single_function(self):
        code = "def only():\n    pass\n"
        res = client.post(
            "/analyze/diagram",
            json={
                "filename": "one.py",
                "content": code,
                "diagram_type": "sequence",
            },
        )
        mermaid = res.json()["mermaid"]
        assert "sequenceDiagram" in mermaid

    def test_generic_js_diagram(self):
        code = "function hello() {}\nconst world = () => {}\n"
        res = client.post(
            "/analyze/diagram",
            json={
                "filename": "app.js",
                "content": code,
                "diagram_type": "flowchart",
            },
        )
        mermaid = res.json()["mermaid"]
        assert "flowchart TD" in mermaid
        assert "hello" in mermaid

    def test_generic_unknown_ext(self):
        res = client.post(
            "/analyze/diagram",
            json={
                "filename": "file.rb",
                "content": "def method; end\n",
                "diagram_type": "flowchart",
            },
        )
        assert res.status_code == 200
        assert "flowchart TD" in res.json()["mermaid"]

    def test_diagram_result_has_all_keys(self):
        res = client.post(
            "/analyze/diagram",
            json={
                "filename": "x.py",
                "content": "x=1\n",
                "diagram_type": "flowchart",
            },
        )
        data = res.json()
        for key in ("filename", "diagram_type", "mermaid", "ts"):
            assert key in data

    def test_classes_diagram_lists_attributes_from_multiple_methods(self):
        # `attributes` en Rust cubre `self.x=` de CUALQUIER método, no solo
        # `__init__` (a diferencia del fallback Python, que solo mira
        # `__init__` + asignaciones sueltas a nivel de clase) — diferencia
        # documentada, no un bug.
        code = "class C:\n    def __init__(self):\n        self.a = 1\n    def other(self):\n        self.b = 2\n"
        res = client.post(
            "/analyze/diagram",
            json={"filename": "c.py", "content": code, "diagram_type": "classes"},
        )
        mermaid = res.json()["mermaid"]
        assert "+a" in mermaid
        assert "+b" in mermaid

    def test_diagram_degrades_to_fallback_when_sidecar_unreachable(self, monkeypatch):
        """Mismo criterio que el resto del engine: sin sidecar, los 4
        builders `.py` caen a `_py_*_fallback` (el `ast.walk` original) y
        siguen produciendo el mismo tipo de diagrama, no un error."""
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        for diag_type, expected in [
            ("flowchart", "flowchart TD"),
            ("callgraph", "graph LR"),
            ("classes", "classDiagram"),
            ("sequence", "sequenceDiagram"),
        ]:
            res = client.post(
                "/analyze/diagram",
                json={"filename": "s.py", "content": self.SIMPLE_CODE, "diagram_type": diag_type},
            )
            assert res.status_code == 200
            assert expected in res.json()["mermaid"], f"fallback de {diag_type} no produjo el shape esperado"

    def test_diagram_fallback_flowchart_finds_functions(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        res = client.post(
            "/analyze/diagram",
            json={"filename": "s.py", "content": self.SIMPLE_CODE, "diagram_type": "flowchart"},
        )
        mermaid = res.json()["mermaid"]
        assert "load_data" in mermaid
        assert "process" in mermaid

    def test_diagram_fallback_still_reports_syntax_error_correctly(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        res = client.post(
            "/analyze/diagram",
            json={"filename": "bad.py", "content": "def bad(\n", "diagram_type": "flowchart"},
        )
        assert "SyntaxError" in res.json()["mermaid"]


# ─── /analyze/api (check externo) ─────────────────────────────────────────────


class TestCheckApi:
    def test_check_invalid_url_returns_down(self):
        res = client.post(
            "/analyze/api",
            json={
                "urls": ["http://localhost:19999/nonexistent"],
                "timeout": 2,
            },
        )
        assert res.status_code == 200
        data = res.json()
        assert len(data["results"]) == 1
        assert data["results"][0]["status"] in ("down", "error", "unknown")

    def test_check_multiple_urls(self):
        res = client.post(
            "/analyze/api",
            json={
                "urls": [
                    "http://localhost:19999/a",
                    "http://localhost:19999/b",
                ],
                "timeout": 1,
            },
        )
        assert res.status_code == 200
        assert len(res.json()["results"]) == 2

    def test_check_result_has_required_fields(self):
        res = client.post(
            "/analyze/api",
            json={
                "urls": ["http://localhost:19999/x"],
                "timeout": 1,
            },
        )
        result = res.json()["results"][0]
        for field in ("url", "status", "code", "ms", "error", "ts"):
            assert field in result


# ─── /analyze/logs-analyze ────────────────────────────────────────────────────


class TestAnalyzeLogs:
    LOG_CONTENT = """2024-01-01 10:00:00 INFO Server started
2024-01-01 10:01:00 ERROR Connection refused
2024-01-01 10:02:00 WARNING Deprecated API used
2024-01-01 10:03:00 ERROR Traceback (most recent call last):
2024-01-01 10:04:00 INFO Request processed
"""

    def test_analyze_logs_detects_errors(self):
        res = client.post(
            "/analyze/logs-analyze",
            json={
                "files": [{"name": "app.log", "content": self.LOG_CONTENT}],
            },
        )
        assert res.status_code == 200
        data = res.json()
        assert len(data["errors"]) >= 2

    def test_analyze_logs_detects_warnings(self):
        res = client.post(
            "/analyze/logs-analyze",
            json={
                "files": [{"name": "app.log", "content": self.LOG_CONTENT}],
            },
        )
        assert len(res.json()["warnings"]) >= 1

    def test_analyze_logs_summary(self):
        res = client.post(
            "/analyze/logs-analyze",
            json={
                "files": [{"name": "app.log", "content": self.LOG_CONTENT}],
            },
        )
        summary = res.json()["summary"]
        assert "app.log" in summary
        assert summary["app.log"]["total_lines"] == 5
        assert summary["app.log"]["counts"]["error"] >= 2

    def test_analyze_multiple_log_files(self):
        res = client.post(
            "/analyze/logs-analyze",
            json={
                "files": [
                    {"name": "a.log", "content": "ERROR boom\n"},
                    {"name": "b.log", "content": "INFO ok\n"},
                ],
            },
        )
        assert res.status_code == 200
        summary = res.json()["summary"]
        assert "a.log" in summary
        assert "b.log" in summary

    def test_analyze_empty_log(self):
        res = client.post(
            "/analyze/logs-analyze",
            json={
                "files": [{"name": "empty.log", "content": ""}],
            },
        )
        assert res.status_code == 200
        assert res.json()["errors"] == []
        assert res.json()["warnings"] == []

    def test_analyze_logs_result_keys(self):
        res = client.post(
            "/analyze/logs-analyze",
            json={
                "files": [{"name": "x.log", "content": "ERROR x\n"}],
            },
        )
        data = res.json()
        for key in ("errors", "warnings", "summary", "ts"):
            assert key in data


# ─── /logs y /api/history ─────────────────────────────────────────────────────


class TestLogsHistory:
    def test_get_logs(self):
        res = client.get("/logs")
        assert res.status_code == 200
        data = res.json()
        assert "logs" in data
        assert "total" in data
        assert isinstance(data["logs"], list)

    def test_get_logs_limit(self):
        res = client.get("/logs?limit=5")
        assert res.status_code == 200
        assert len(res.json()["logs"]) <= 5

    def test_get_api_history(self):
        res = client.get("/api/history")
        assert res.status_code == 200
        assert "history" in res.json()
