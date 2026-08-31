"""
Tests — router Execution Intelligence (apps/api/routers/execution.py)

A diferencia de `test_complexity_client.py` (que testea las funciones cliente
del sidecar directamente), este archivo testea el router HTTP en sí —
ninguno de los endpoints `/execution/*` tenía un test propio hasta ahora, un
hueco preexistente que esta primera adición no se propone cerrar entero,
solo cubrir el endpoint nuevo (`/validate-matmul-vs-numpy`) que sí tiene
lógica propia más allá de delegar al sidecar (el timing de numpy y la nota
de comparación se calculan acá, no en Rust).
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

import pytest
from fastapi.testclient import TestClient

from main import app

client = TestClient(app)


class TestValidateMatmulVsNumpy:
    """Primera pieza de "migrar de numpy/pandas/scikit-learn" (pedido
    explícito del usuario, 2026-08-31): antes de proponer un reemplazo
    nativo, medir contra la librería real. Corre el kernel Fortran ya
    existente Y numpy real, mismos tamaños/datos, y compara."""

    def test_response_has_fortran_and_numpy_sections(self):
        r = client.post("/execution/validate-matmul-vs-numpy")
        assert r.status_code == 200
        data = r.json()
        assert "fortran" in data
        assert "numpy" in data
        assert "comparison_note" in data

    def test_numpy_section_has_same_sizes_as_fortran_when_available(self):
        r = client.post("/execution/validate-matmul-vs-numpy")
        data = r.json()
        if not data["numpy"]["available"]:
            pytest.skip("numpy no disponible en este entorno")
        numpy_sizes = {m["n"] for m in data["numpy"]["measurements"]}
        assert numpy_sizes == {300, 450, 600, 800}
        assert all(m["seconds"] > 0 for m in data["numpy"]["measurements"])

    def test_comparison_note_mentions_numpy_when_both_available(self):
        r = client.post("/execution/validate-matmul-vs-numpy")
        data = r.json()
        if not data["numpy"]["available"] or not data["fortran"].get("available"):
            pytest.skip("numpy o el sidecar Fortran no disponibles en este entorno")
        assert "numpy" in data["comparison_note"]
        assert "más rápido" in data["comparison_note"]
