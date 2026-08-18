"""
Tests — cliente del sidecar Rust `complexity-engine` (services/complexity_client.py)

El sidecar no corre durante `pytest` (ver .github/workflows/ci.yml — el job
`backend` no tiene toolchain de Rust ni lo levanta), así que estos tests
ejercitan la degradación con gracia real (sin mocks): el sidecar está
genuinamente no-disponible en este entorno, igual que le pasaría a un usuario
que corre `pytest` sin haber hecho `npm run dev`. La corrección de los
números (complejidad/MI/raw) se prueba aparte con `cargo test` sobre
services/complexity/src/*.rs, que sí corre el código real.
"""

import asyncio

from services.complexity_client import (
    analyze_complexity,
    check_complexity_engine_sync,
    check_complexity_engine,
    parse_python_rich,
)


class TestComplexityClientUnavailable:
    def test_analyze_complexity_returns_empty_shape_when_unreachable(self):
        result = asyncio.run(analyze_complexity("f.py", "def f():\n    return 1\n"))
        assert result == {"functions": [], "mi": None, "raw": {}, "error": None}

    def test_check_complexity_engine_sync_false_when_unreachable(self):
        assert check_complexity_engine_sync() is False

    def test_check_complexity_engine_async_false_when_unreachable(self):
        assert asyncio.run(check_complexity_engine()) is False

    def test_analyze_complexity_does_not_raise_on_empty_content(self):
        result = asyncio.run(analyze_complexity("empty.py", ""))
        assert result["functions"] == []


class TestParsePythonRichUnavailable:
    """Fase 1 de la migración a Rust (services/complexity_client.py::parse_python_rich).
    Mismo espíritu que TestComplexityClientUnavailable — sin el sidecar corriendo
    en este entorno, `None` es el resultado correcto (el caller decide caer en
    `_parse_python()`), no una excepción."""

    def test_returns_none_when_unreachable(self):
        result = asyncio.run(parse_python_rich("f.py", "def f():\n    return 1\n"))
        assert result is None

    def test_does_not_raise_on_empty_content(self):
        result = asyncio.run(parse_python_rich("empty.py", ""))
        assert result is None
