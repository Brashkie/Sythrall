"""
Tests — Analysis, ML, Diagram routers
pytest tests/test_analysis.py -v
"""

import pytest
from fastapi.testclient import TestClient

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from main import app

client = TestClient(app)


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
        for key in ("filename", "ts", "issues", "metrics", "complexity", "maintainability", "raw_stats", "tools_used"):
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
                "tools": ["ast", "flake8", "pylint", "radon"],
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

    TORCH_CODE = """
import torch
import torch.nn as nn

torch.manual_seed(42)

class MyNet(nn.Module):
    def __init__(self):
        super().__init__()
        self.fc = nn.Linear(10, 2)
    def forward(self, x):
        return self.fc(x)

model = MyNet()
optimizer = torch.optim.Adam(model.parameters())
loss_fn = nn.CrossEntropyLoss()

for epoch in range(10):
    optimizer.zero_grad()
    out = model(torch.randn(32, 10))
    loss = loss_fn(out, torch.zeros(32, dtype=torch.long))
    loss.backward()
    optimizer.step()
"""

    LEAKAGE_CODE = """
import pandas as pd
from sklearn.preprocessing import StandardScaler
from sklearn.model_selection import train_test_split

scaler = StandardScaler()
X = scaler.fit_transform(X_raw)   # BUG: antes del split
X_train, X_test = train_test_split(X)
"""

    def test_ml_detects_libraries(self):
        res = client.post(
            "/analyze/ml",
            json={
                "filename": "model.py",
                "content": self.SKLEARN_CODE,
            },
        )
        assert res.status_code == 200
        data = res.json()
        lib_names = [lib["name"] for lib in data["libraries"]]
        assert "NumPy" in lib_names
        assert "Pandas" in lib_names
        assert "Scikit-learn" in lib_names

    def test_ml_detects_pipeline_stages(self):
        res = client.post(
            "/analyze/ml",
            json={
                "filename": "model.py",
                "content": self.SKLEARN_CODE,
            },
        )
        stage_ids = [s["id"] for s in res.json()["pipeline"]]
        assert "carga_datos" in stage_ids
        assert "split" in stage_ids
        assert "escalado" in stage_ids
        assert "entrenamiento" in stage_ids
        assert "prediccion" in stage_ids
        assert "evaluacion" in stage_ids

    def test_ml_detects_random_forest_model(self):
        res = client.post(
            "/analyze/ml",
            json={
                "filename": "model.py",
                "content": self.SKLEARN_CODE,
            },
        )
        model_names = [m["name"] for m in res.json()["models"]]
        assert "RandomForestClassifier" in model_names

    def test_ml_detects_metrics(self):
        res = client.post(
            "/analyze/ml",
            json={
                "filename": "model.py",
                "content": self.SKLEARN_CODE,
            },
        )
        metrics = res.json()["metrics"]
        assert "accuracy" in metrics

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

    def test_ml_detects_data_leakage(self):
        res = client.post(
            "/analyze/ml",
            json={
                "filename": "bad.py",
                "content": self.LEAKAGE_CODE,
            },
        )
        issues = res.json()["issues"]
        leakage = [i for i in issues if i.get("category") == "data_leakage"]
        assert len(leakage) >= 1
        assert leakage[0]["severity"] == "error"

    def test_ml_pytorch_no_zero_grad_issue(self):
        bad_torch = self.TORCH_CODE.replace("optimizer.zero_grad()", "")
        res = client.post(
            "/analyze/ml",
            json={
                "filename": "net.py",
                "content": bad_torch,
            },
        )
        issues = res.json()["issues"]
        pytorch_issues = [i for i in issues if i.get("category") == "pytorch"]
        assert any("zero_grad" in i["message"] for i in pytorch_issues)

    def test_ml_pytorch_with_zero_grad_no_issue(self):
        res = client.post(
            "/analyze/ml",
            json={
                "filename": "net.py",
                "content": self.TORCH_CODE,
            },
        )
        issues = res.json()["issues"]
        zero_grad_issues = [i for i in issues if i.get("category") == "pytorch" and "zero_grad" in i.get("message", "")]
        assert len(zero_grad_issues) == 0

    def test_ml_no_random_state_warning(self):
        code = "from sklearn.model_selection import train_test_split\nX_train, X_test = train_test_split(X)\n"
        res = client.post("/analyze/ml", json={"filename": "m.py", "content": code})
        issues = res.json()["issues"]
        rs_issues = [i for i in issues if "random_state" in i.get("message", "")]
        assert len(rs_issues) >= 1

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

    def test_ml_icecream_warning(self):
        code = "from icecream import ic\n" + "\n".join([f"ic(x{i})" for i in range(8)])
        res = client.post("/analyze/ml", json={"filename": "debug.py", "content": code})
        issues = res.json()["issues"]
        ic_issues = [i for i in issues if i.get("category") == "icecream"]
        assert len(ic_issues) >= 1
        assert ic_issues[0]["severity"] == "warning"

    def test_ml_lgbm_without_early_stopping(self):
        code = "import lightgbm as lgb\nmodel = lgb.LGBMClassifier()\nmodel.fit(X, y)\n"
        res = client.post("/analyze/ml", json={"filename": "lgbm.py", "content": code})
        issues = res.json()["issues"]
        lgbm_issues = [i for i in issues if i.get("category") == "lightgbm"]
        assert any("early_stopping" in i["message"] for i in lgbm_issues)

    def test_ml_opencv_no_none_check(self):
        code = "import cv2\nimg = cv2.imread('file.jpg')\nprint(img.shape)\n"
        res = client.post("/analyze/ml", json={"filename": "cv.py", "content": code})
        issues = res.json()["issues"]
        cv_issues = [i for i in issues if i.get("category") == "opencv"]
        assert any("None" in i["message"] for i in cv_issues)

    def test_ml_plotly_no_show(self):
        code = "import plotly.express as px\nfig = px.scatter(df, x='a', y='b')\n"
        res = client.post("/analyze/ml", json={"filename": "plot.py", "content": code})
        issues = res.json()["issues"]
        plot_issues = [i for i in issues if i.get("category") == "plotly"]
        assert len(plot_issues) >= 1

    def test_ml_polars_pandas_mix_warning(self):
        code = "import polars as pl\nimport pandas as pd\ndf = pd.read_csv('f.csv')\ndf2 = pl.DataFrame()\n"
        res = client.post("/analyze/ml", json={"filename": "mix.py", "content": code})
        issues = res.json()["issues"]
        polars_issues = [i for i in issues if i.get("category") == "polars"]
        assert len(polars_issues) >= 1

    def test_ml_pipeline_sorted_by_line(self):
        res = client.post(
            "/analyze/ml",
            json={
                "filename": "model.py",
                "content": self.SKLEARN_CODE,
            },
        )
        lines = [s["line"] for s in res.json()["pipeline"]]
        assert lines == sorted(lines)

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
