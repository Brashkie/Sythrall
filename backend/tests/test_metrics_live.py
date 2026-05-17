"""
Tests — Live Metrics + Validate + Safe Mode
pytest tests/test_metrics_live.py -v
"""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent.parent))

from fastapi.testclient import TestClient

# Importar el router directamente para test aislado
from fastapi import FastAPI
from routers.metrics_live import router

app = FastAPI()
app.include_router(router, prefix="/metrics")
client = TestClient(app)

# ─── Fixtures ────────────────────────────────────────────────────────────────

PY_SIMPLE = """
import os
from typing import List

LIMIT = 100

class DataProcessor:
    def __init__(self, data: List[int]):
        self.data = data
    def process(self) -> List[int]:
        return [x * 2 for x in self.data]

def bubble_sort(arr: List[int]) -> List[int]:
    n = len(arr)
    for i in range(n):
        for j in range(n - i - 1):
            if arr[j] > arr[j+1]:
                arr[j], arr[j+1] = arr[j+1], arr[j]
    return arr

def binary_search(arr: List[int], target: int) -> int:
    lo, hi = 0, len(arr)
    while lo < hi:
        mid = (lo + hi) // 2
        if arr[mid] == target: return mid
        elif arr[mid] < target: lo = mid + 1
        else: hi = mid
    return -1

def constant(x: int) -> int:
    return x * 2
"""

PY_SYNTAX_ERROR = """
def broken(
    x = 1
    pass
"""

TS_CODE = """
import { readFile } from 'fs'
import { join } from 'path'

class UserService {
  findAll(): void {}
  findById(id: number): void {}
}

function processUsers(service: UserService): void {
  for (let i = 0; i < 100; i++) {
    for (let j = 0; j < 100; j++) {
      console.log(i, j)
    }
  }
}

const helper = (x: number) => x * 2
"""

EMPTY_CODE = ""
BINARY_CODE = "def test():\n    pass\x00\x00\x00binary data here\x00\x00"


# ─── /metrics/live — Python ──────────────────────────────────────────────────

class TestLiveMetricsPython:
    def test_live_ok(self):
        r = client.post("/metrics/live", json={"filename": "test.py", "content": PY_SIMPLE})
        assert r.status_code == 200

    def test_live_loc(self):
        data = client.post("/metrics/live", json={"filename": "test.py", "content": PY_SIMPLE}).json()
        assert data["loc"] > 0

    def test_live_functions_count(self):
        data = client.post("/metrics/live", json={"filename": "test.py", "content": PY_SIMPLE}).json()
        assert data["functions"] >= 4  # __init__, process, bubble_sort, binary_search, constant

    def test_live_classes_count(self):
        data = client.post("/metrics/live", json={"filename": "test.py", "content": PY_SIMPLE}).json()
        assert data["classes"] >= 1

    def test_live_imports_count(self):
        data = client.post("/metrics/live", json={"filename": "test.py", "content": PY_SIMPLE}).json()
        assert data["imports"] >= 2

    def test_live_big_o_worst_bubble(self):
        data = client.post("/metrics/live", json={"filename": "test.py", "content": PY_SIMPLE}).json()
        # bubble_sort tiene O(n²), debe ser el worst
        assert data["big_o_worst"] in ("O(n²)", "O(n³)", "O(2^n)")

    def test_live_avg_cc(self):
        data = client.post("/metrics/live", json={"filename": "test.py", "content": PY_SIMPLE}).json()
        assert isinstance(data["avg_cc"], float)
        assert data["avg_cc"] >= 1.0

    def test_live_language_detected(self):
        data = client.post("/metrics/live", json={"filename": "test.py", "content": PY_SIMPLE}).json()
        assert data["language"] == "python"

    def test_live_parse_ok(self):
        data = client.post("/metrics/live", json={"filename": "test.py", "content": PY_SIMPLE}).json()
        assert data["parse_ok"] is True
        assert data["safe_mode"] is False

    def test_live_ms_field(self):
        data = client.post("/metrics/live", json={"filename": "test.py", "content": PY_SIMPLE}).json()
        assert "ms" in data
        assert data["ms"] < 500

    def test_live_empty_content(self):
        data = client.post("/metrics/live", json={"filename": "test.py", "content": ""}).json()
        assert data["loc"] == 0
        assert data["functions"] == 0

    def test_live_syntax_error_safe_mode(self):
        data = client.post("/metrics/live", json={"filename": "test.py", "content": PY_SYNTAX_ERROR}).json()
        assert data["parse_ok"] is False
        assert data["safe_mode"] is True
        assert data["parse_error"] is not None

    def test_live_safe_mode_still_returns_metrics(self):
        data = client.post("/metrics/live", json={"filename": "test.py", "content": PY_SYNTAX_ERROR}).json()
        # Debe retornar métricas básicas aunque falle el AST
        assert "loc" in data
        assert "functions" in data

    def test_live_big_o_distribution(self):
        data = client.post("/metrics/live", json={"filename": "test.py", "content": PY_SIMPLE}).json()
        assert isinstance(data["big_o_dist"], dict)
        assert len(data["big_o_dist"]) > 0

    def test_live_binary_search_log_n(self):
        code = "def binary_search(arr, target):\n    lo, hi = 0, len(arr)\n    while lo < hi:\n        mid = (lo + hi) // 2\n        if arr[mid] == target: return mid\n        elif arr[mid] < target: lo = mid + 1\n        else: hi = mid\n    return -1\n"
        data = client.post("/metrics/live", json={"filename": "test.py", "content": code}).json()
        assert data["big_o_worst"] in ("O(log n)", "O(n)", "O(1)")


# ─── /metrics/live — TypeScript ──────────────────────────────────────────────

class TestLiveMetricsTS:
    def test_live_ts_ok(self):
        r = client.post("/metrics/live", json={"filename": "app.ts", "content": TS_CODE})
        assert r.status_code == 200

    def test_live_ts_language(self):
        data = client.post("/metrics/live", json={"filename": "app.ts", "content": TS_CODE}).json()
        assert data["language"] == "typescript"

    def test_live_ts_functions(self):
        data = client.post("/metrics/live", json={"filename": "app.ts", "content": TS_CODE}).json()
        assert data["functions"] >= 1

    def test_live_ts_imports(self):
        data = client.post("/metrics/live", json={"filename": "app.ts", "content": TS_CODE}).json()
        assert data["imports"] >= 2

    def test_live_ts_nested_loops_big_o(self):
        data = client.post("/metrics/live", json={"filename": "app.ts", "content": TS_CODE}).json()
        assert data["big_o_worst"] in ("O(n²)", "O(n³)", "O(2^n)")

    def test_live_jsx_extension(self):
        r = client.post("/metrics/live", json={"filename": "App.jsx", "content": "function App() { return null }"})
        assert r.status_code == 200
        assert r.json()["language"] == "javascript"


# ─── /metrics/validate ───────────────────────────────────────────────────────

class TestValidate:
    def test_validate_ok(self):
        r = client.post("/metrics/validate", json={"filename": "test.py", "content": PY_SIMPLE})
        assert r.status_code == 200

    def test_validate_valid_python(self):
        data = client.post("/metrics/validate", json={"filename": "test.py", "content": PY_SIMPLE}).json()
        assert data["valid"] is True
        assert data["errors"] == []

    def test_validate_syntax_error_safe_mode(self):
        data = client.post("/metrics/validate", json={"filename": "test.py", "content": PY_SYNTAX_ERROR}).json()
        assert data["safe_mode"] is True
        assert len(data["warnings"]) > 0

    def test_validate_binary_detected(self):
        data = client.post("/metrics/validate", json={"filename": "test.py", "content": BINARY_CODE}).json()
        assert data["is_binary"] is True
        assert data["valid"] is False

    def test_validate_empty_file(self):
        data = client.post("/metrics/validate", json={"filename": "test.py", "content": ""}).json()
        assert len(data["warnings"]) > 0

    def test_validate_truncated_python(self):
        truncated = "def my_func(x):\n    for i in range(x):\n        if x > 0:"
        data = client.post("/metrics/validate", json={"filename": "test.py", "content": truncated}).json()
        assert data["is_truncated"] is True or data["safe_mode"] is True

    def test_validate_unbalanced_braces_ts(self):
        bad_ts = "function foo() {\n  if (x) {\n    return 1\n  // missing close braces"
        data = client.post("/metrics/validate", json={"filename": "app.ts", "content": bad_ts}).json()
        assert len(data["warnings"]) > 0

    def test_validate_size_reported(self):
        data = client.post("/metrics/validate", json={"filename": "test.py", "content": PY_SIMPLE}).json()
        assert data["size_bytes"] > 0

    def test_validate_response_structure(self):
        data = client.post("/metrics/validate", json={"filename": "test.py", "content": PY_SIMPLE}).json()
        required = ["filename", "valid", "warnings", "errors", "safe_mode",
                    "is_binary", "is_truncated", "encoding_ok", "size_bytes"]
        for key in required:
            assert key in data, f"Falta campo: {key}"


# ─── /metrics/health ─────────────────────────────────────────────────────────

class TestMetricsHealth:
    def test_health_ok(self):
        r = client.get("/metrics/health")
        assert r.status_code == 200

    def test_health_python_ast(self):
        data = client.get("/metrics/health").json()
        assert data["capabilities"]["python_ast"] is True

    def test_health_supported_languages(self):
        data = client.get("/metrics/health").json()
        langs = data["supported_languages"]
        assert "python" in langs
        assert "typescript" in langs

    def test_health_safe_mode_available(self):
        data = client.get("/metrics/health").json()
        assert data["safe_mode_available"] is True
        assert data["fallback_parser"] == "regex"