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
    build_call_graph_rust,
    build_centrality_graph_rust,
    build_circular_graph_rust,
    build_import_graph_rust,
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


class TestBuildImportGraphRustUnavailable:
    """Fase 18, primer slice del Graph Engine
    (services/complexity_client.py::build_import_graph_rust). Mismo espíritu
    que TestParsePythonRichUnavailable — `None` es el resultado correcto
    cuando el sidecar no está disponible, no una excepción."""

    def test_returns_none_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        files_summary = [{"filename": "a.py", "language": "python", "functions": 1, "imports": [], "dead_code": 0}]
        result = asyncio.run(build_import_graph_rust(files_summary))
        assert result is None

    def test_does_not_raise_on_empty_list(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(build_import_graph_rust([]))
        assert result is None


class TestBuildCentralityGraphRustUnavailable:
    """Fase 18, segunda porción del Graph Engine
    (services/complexity_client.py::build_centrality_graph_rust). Mismo
    espíritu que TestBuildImportGraphRustUnavailable — `None` es el resultado
    correcto cuando el sidecar no está disponible, no una excepción."""

    def test_returns_none_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        files_summary = [{"filename": "a.py", "language": "python", "functions": 1, "imports": [], "dead_code": 0}]
        result = asyncio.run(build_centrality_graph_rust(files_summary))
        assert result is None

    def test_does_not_raise_on_empty_list(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(build_centrality_graph_rust([]))
        assert result is None


class TestBuildCallGraphRustUnavailable:
    """Fase 18, tercera porción del Graph Engine
    (services/complexity_client.py::build_call_graph_rust). Mismo espíritu
    que las 2 anteriores — `None` es el resultado correcto cuando el sidecar
    no está disponible, no una excepción."""

    def test_returns_none_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        files_payload = [{"filename": "a.py", "functions": [{"name": "f", "big_o": "O(1)"}], "call_graph": []}]
        result = asyncio.run(build_call_graph_rust(files_payload))
        assert result is None

    def test_does_not_raise_on_empty_list(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(build_call_graph_rust([]))
        assert result is None


class TestBuildCircularGraphRustUnavailable:
    """Fase 18, cuarta y última porción del Graph Engine
    (services/complexity_client.py::build_circular_graph_rust). Mismo
    espíritu que las 3 anteriores — `None` es el resultado correcto cuando
    el sidecar no está disponible, no una excepción."""

    def test_returns_none_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        files_summary = [{"filename": "a.py", "language": "python", "functions": 1, "imports": [], "dead_code": 0}]
        result = asyncio.run(build_circular_graph_rust(files_summary))
        assert result is None

    def test_does_not_raise_on_empty_list(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(build_circular_graph_rust([]))
        assert result is None
