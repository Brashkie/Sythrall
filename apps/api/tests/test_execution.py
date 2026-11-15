"""
Tests — router Execution Intelligence (apps/api/routers/execution.py)

A diferencia de `test_complexity_client.py` (que testea las funciones cliente
del sidecar directamente), este archivo testea el router HTTP en sí —
ninguno de los endpoints `/execution/*` tenía un test propio hasta
`/validate-matmul-vs-numpy` (pieza 1 de la migración numpy/pandas/
scikit-learn). Sigue cubriendo únicamente los endpoints con lógica propia
más allá de delegar al sidecar (el timing de numpy/pandas/scikit-learn y la
nota de comparación se calculan acá, no en Rust) — `/validate-aggregation` y
`/validate-linreg` (piezas 2 y 3, kernels Zig/Fortran puros) son relays sin
lógica Python propia y por eso NO tienen test acá, mismo criterio que el
resto de los kernels de la Fase 26 (`cargo test` en `agg_bench.rs`/
`linreg_bench.rs` es su única verificación).
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


class TestValidateAggregationVsPandas:
    """Segunda pieza de "migrar de numpy/pandas/scikit-learn" (2026-08-31):
    corre el kernel Zig ya existente Y pandas real, mismos tamaños/datos, y
    compara. A diferencia de la pieza 1 (donde numpy ganaba), acá gana el
    kernel nativo — el test no asume cuál lado gana, solo que la comparación
    se hace y se reporta con la salvedad correcta."""

    def test_response_has_zig_and_pandas_sections(self):
        r = client.post("/execution/validate-aggregation-vs-pandas")
        assert r.status_code == 200
        data = r.json()
        assert "zig" in data
        assert "pandas" in data
        assert "comparison_note" in data

    def test_pandas_section_has_same_sizes_as_zig_when_available(self):
        r = client.post("/execution/validate-aggregation-vs-pandas")
        data = r.json()
        if not data["pandas"]["available"]:
            pytest.skip("pandas no disponible en este entorno")
        pandas_sizes = {m["n"] for m in data["pandas"]["measurements"]}
        assert pandas_sizes == {2_000_000, 4_000_000, 8_000_000, 16_000_000}
        assert all(m["seconds"] > 0 for m in data["pandas"]["measurements"])

    def test_comparison_note_mentions_the_fixed_categories_caveat_when_both_available(self):
        r = client.post("/execution/validate-aggregation-vs-pandas")
        data = r.json()
        if not data["pandas"]["available"] or not data["zig"].get("available"):
            pytest.skip("pandas o el sidecar Zig no disponibles en este entorno")
        assert "pandas" in data["comparison_note"]
        assert "categorías fijas" in data["comparison_note"]


class TestValidateLinregVsSklearn:
    """Tercera y última pieza de "migrar de numpy/pandas/scikit-learn"
    (2026-08-31): corre el kernel Fortran ya existente Y scikit-learn real,
    mismos tamaños/datos, y compara. Igual que la pieza 2, gana el kernel
    nativo — el test no asume cuál lado gana, solo que la comparación se
    hace y se reporta con la salvedad correcta."""

    def test_response_has_fortran_and_sklearn_sections(self):
        r = client.post("/execution/validate-linreg-vs-sklearn")
        assert r.status_code == 200
        data = r.json()
        assert "fortran" in data
        assert "sklearn" in data
        assert "comparison_note" in data

    def test_sklearn_section_has_same_sizes_as_fortran_when_available(self):
        r = client.post("/execution/validate-linreg-vs-sklearn")
        data = r.json()
        if not data["sklearn"]["available"]:
            pytest.skip("scikit-learn no disponible en este entorno")
        sklearn_sizes = {m["n"] for m in data["sklearn"]["measurements"]}
        assert sklearn_sizes == {40_000_000, 80_000_000, 120_000_000, 160_000_000}
        assert all(m["seconds"] > 0 for m in data["sklearn"]["measurements"])

    def test_comparison_note_mentions_the_single_predictor_caveat_when_both_available(self):
        r = client.post("/execution/validate-linreg-vs-sklearn")
        data = r.json()
        if not data["sklearn"]["available"] or not data["fortran"].get("available"):
            pytest.skip("scikit-learn o el sidecar Fortran no disponibles en este entorno")
        assert "scikit-learn" in data["comparison_note"]
        assert "sola variable predictora" in data["comparison_note"]
