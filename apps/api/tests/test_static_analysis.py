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
