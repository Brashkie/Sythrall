"""
Tests — cliente del sidecar Rust `complexity-engine` (services/complexity_client.py)

`conftest.py` levanta el sidecar real para toda la sesión de pytest (Rust es
el núcleo del análisis ahora, no un mock contra el cual testear), así que
estos tests apuntan deliberadamente a un puerto que nadie escucha
(`monkeypatch` sobre `COMPLEXITY_ENGINE_URL`) para seguir ejercitando la
degradación con gracia — el shape vacío/`None` que el caller necesita cuando
el sidecar realmente no está disponible (un usuario sin `cargo build`, o el
proceso cayéndose a mitad de sesión). La corrección de los números cuando SÍ
está disponible se prueba aparte con `cargo test` sobre
services/complexity/src/*.rs (motor real) y con el resto de los tests de
este archivo/`test_security_findings.py`/etc. (vía HTTP, contra el sidecar
que `conftest.py` ya dejó corriendo).
"""

import asyncio

import services.complexity_client as complexity_client
from services.complexity_client import (
    analyze_complexity,
    check_complexity_engine_sync,
    check_complexity_engine,
    parse_python_rich,
)

_UNREACHABLE_URL = "http://127.0.0.1:1"


class TestComplexityClientUnavailable:
    def test_analyze_complexity_returns_empty_shape_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(analyze_complexity("f.py", "def f():\n    return 1\n"))
        assert result == {"functions": [], "mi": None, "halstead": None, "raw": {}, "error": None}

    def test_check_complexity_engine_sync_false_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        assert check_complexity_engine_sync() is False

    def test_check_complexity_engine_async_false_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        assert asyncio.run(check_complexity_engine()) is False

    def test_analyze_complexity_does_not_raise_on_empty_content(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(analyze_complexity("empty.py", ""))
        assert result["functions"] == []

    def test_analyze_complexity_halstead_key_present_when_unreachable(self, monkeypatch):
        """Fase 22: `halstead` es Rust-only (sin fallback Python, mismo límite
        que `mi`) — el shape vacío tiene que traer la key igual, en None."""
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(analyze_complexity("f.py", "def f():\n    return 1\n"))
        assert result["halstead"] is None


class TestParsePythonRichUnavailable:
    """Fase 1 de la migración a Rust (services/complexity_client.py::parse_python_rich).
    Mismo espíritu que TestComplexityClientUnavailable — apuntando a un puerto
    inalcanzable, `None` es el resultado correcto (el caller decide degradar),
    no una excepción."""

    def test_returns_none_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(parse_python_rich("f.py", "def f():\n    return 1\n"))
        assert result is None

    def test_does_not_raise_on_empty_content(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(parse_python_rich("empty.py", ""))
        assert result is None
