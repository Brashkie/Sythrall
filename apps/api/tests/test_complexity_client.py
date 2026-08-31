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
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from fastapi.testclient import TestClient

import services.complexity_client as complexity_client
import services.log_client as log_client
from main import app
from shared import now
from services.complexity_client import (
    analyze_complexity,
    build_architecture_smells_rust,
    build_call_graph_rust,
    build_centrality_graph_rust,
    build_circular_graph_rust,
    build_import_graph_rust,
    check_complexity_engine_sync,
    check_complexity_engine,
    find_definitions_python_rust,
    find_definitions_jsts_rust,
    find_references_python_rust,
    find_references_jsts_rust,
    parse_c_rust,
    parse_cpp_rust,
    get_plugin_manifests_rust,
    parse_fortran_rust,
    parse_js_rust,
    parse_python_rich,
    parse_ts_rust,
)

client = TestClient(app)

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


class TestBuildArchitectureSmellsRustUnavailable:
    """Fase 18, "Dependency Engine" — el último ítem del Graph Engine
    (services/complexity_client.py::build_architecture_smells_rust). Mismo
    criterio `None`-en-falla, salvo que el shape de éxito es una lista
    plana de smells, no un dict de grafo — por eso el chequeo de error en
    la función es `isinstance(data, dict) and data.get("error")` en vez de
    `data.get("error")` a secas."""

    def test_returns_none_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        files_summary = [{"filename": "a.py", "language": "python", "functions": 1, "imports": [], "dead_code": 0}]
        result = asyncio.run(build_architecture_smells_rust(files_summary))
        assert result is None

    def test_does_not_raise_on_empty_list(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(build_architecture_smells_rust([]))
        assert result is None

    def test_returns_smells_against_real_sidecar(self):
        files_summary = [
            {
                "filename": "a.py",
                "language": "python",
                "functions": 1,
                "imports": [{"module": "b", "line": 1}],
                "dead_code": 0,
            },
            {
                "filename": "b.py",
                "language": "python",
                "functions": 1,
                "imports": [{"module": "a", "line": 1}],
                "dead_code": 0,
            },
        ]
        result = asyncio.run(build_architecture_smells_rust(files_summary))
        assert result is not None
        assert any(s["kind"] == "circular_dependency" for s in result)


class TestParseCCppJsTsRustUnavailable:
    """Fase 18: C/C++/JS/TS son Rust-only ahora (`services/complexity/src/
    {cparse,jsts}.rs`) — sin fallback Python propio (a diferencia de
    Python, ninguno tenía un esqueleto liviano previo), así que `None` es
    el resultado correcto cuando el sidecar no está disponible, mismo
    criterio que el resto de esta clase de funciones."""

    def test_parse_c_returns_none_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(parse_c_rust("f.c", "int main() { return 0; }"))
        assert result is None

    def test_parse_cpp_returns_none_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(parse_cpp_rust("f.cpp", "class C {};"))
        assert result is None

    def test_parse_js_returns_none_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(parse_js_rust("f.js", "function f() {}"))
        assert result is None

    def test_parse_ts_returns_none_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(parse_ts_rust("f.ts", "interface Foo {}"))
        assert result is None

    def test_parse_fortran_returns_none_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(parse_fortran_rust("f.f90", "SUBROUTINE F()\nEND SUBROUTINE F\n"))
        assert result is None


class TestParseCCppJsTsRustAgainstRealSidecar:
    """Contra el sidecar real que `conftest.py` ya deja corriendo — confirma
    que el shape de respuesta es el esperado, no solo que no explota."""

    def test_parse_c_finds_function(self):
        result = asyncio.run(parse_c_rust("f.c", "int add(int a, int b) { return a + b; }"))
        assert result is not None
        assert any(f["name"] == "add" for f in result["functions"])

    def test_parse_cpp_finds_class(self):
        result = asyncio.run(parse_cpp_rust("f.cpp", "class Animal {\npublic:\n    void speak();\n};\n"))
        assert result is not None
        assert any(c["name"] == "Animal" for c in result["classes"])

    def test_parse_js_finds_function(self):
        result = asyncio.run(parse_js_rust("f.js", "function add(a, b) { return a + b; }"))
        assert result is not None
        assert any(f["name"] == "add" for f in result["functions"])

    def test_parse_ts_finds_interface(self):
        result = asyncio.run(parse_ts_rust("f.ts", "interface Foo {}"))
        assert result is not None
        assert any(i["name"] == "Foo" for i in result["interfaces"])

    def test_parse_fortran_finds_subroutine(self):
        result = asyncio.run(
            parse_fortran_rust("f.f90", "SUBROUTINE ADD(A, B, C)\n  REAL :: A, B, C\n  C = A + B\nEND SUBROUTINE ADD\n")
        )
        assert result is not None
        assert any(f["name"] == "ADD" for f in result["functions"])


class TestPluginManifestsRustUnavailable:
    """Fase 24 (Extensibility Platform) — `None` es el resultado correcto
    cuando el sidecar no responde, mismo criterio que el resto de esta clase."""

    def test_returns_none_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(get_plugin_manifests_rust())
        assert result is None


class TestPluginManifestsRustAgainstRealSidecar:
    """Contra el sidecar real — confirma que `GET /plugins/manifests` trae
    los 7 plugins built-in con el shape que `plugin_registry.py` espera."""

    def test_returns_seven_builtin_language_plugins(self):
        result = asyncio.run(get_plugin_manifests_rust())
        assert result is not None
        assert len(result) == 7
        ids = {m["id"] for m in result}
        assert ids == {"python", "c", "cpp", "javascript", "typescript", "fortran", "assembly"}
        for m in result:
            assert m["builtin"] is True
            assert m["category"] == "language"
            assert m["extensions"]


class TestSymbolEngineRustUnavailable:
    """Fase 18, "Symbol Engine": go-to-definition/find-references, portado a
    Rust (`services/complexity/src/symbols.rs`). `None` cuando el sidecar no
    responde es el resultado correcto — `routers/intelligence.py` decide el
    fallback (Python conserva el suyo propio para `.py`, ver
    `_find_definitions_python_fallback`/`_find_references_python_fallback`;
    JS/TS no tiene fallback, igual que el resto de ese lenguaje desde que su
    parser se portó del todo)."""

    def test_find_definitions_python_returns_none_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(find_definitions_python_rust("def f():\n    pass\n", "f"))
        assert result is None

    def test_find_definitions_jsts_returns_none_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(find_definitions_jsts_rust("function f() {}", False, "f"))
        assert result is None

    def test_find_references_python_returns_none_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(find_references_python_rust("def f():\n    pass\nf()\n", "f"))
        assert result is None

    def test_find_references_jsts_returns_none_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        result = asyncio.run(find_references_jsts_rust("function f() {}\nf();\n", True, "f"))
        assert result is None


class TestSymbolEngineRustAgainstRealSidecar:
    """Contra el sidecar real que `conftest.py` ya deja corriendo."""

    def test_find_definitions_python_finds_function(self):
        result = asyncio.run(find_definitions_python_rust("def bubble_sort(arr):\n    return arr\n", "bubble_sort"))
        assert result is not None
        assert result[0]["kind"] == "function"
        assert result[0]["line"] == 1

    def test_find_definitions_jsts_finds_class(self):
        result = asyncio.run(find_definitions_jsts_rust("class UserService {}\n", True, "UserService"))
        assert result is not None
        assert result[0]["kind"] == "class"

    def test_find_references_python_finds_definition_and_use(self):
        result = asyncio.run(find_references_python_rust("def f():\n    pass\n\nf()\n", "f"))
        assert result is not None
        assert result["definition_line"] == 1
        kinds = [r["kind"] for r in result["references"]]
        assert "definition" in kinds

    def test_find_references_jsts_finds_definition_and_use(self):
        result = asyncio.run(find_references_jsts_rust("function f() {}\nf();\n", False, "f"))
        assert result is not None
        assert result["definition_line"] == 1
        assert result["references"]


class TestLogClient:
    """Log unificado (backend + ambos sidecars Rust), persistido en CBOR vía
    el sidecar (services/log_client.py + services/complexity/src/logstore.rs).
    `persist_log` nunca debe levantar ni bloquear cuando el sidecar está
    caído; `fetch_logs` sigue el mismo criterio `None`-en-falla que el resto
    de este archivo. Los casos de éxito (contra el sidecar real que
    `conftest.py` ya dejó corriendo) prueban el round-trip completo:
    persistir y después leerlo de vuelta, ya decodificado a texto real."""

    def test_persist_log_does_not_raise_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        asyncio.run(log_client.persist_log({"ts": "2026-01-01 00:00:00", "level": "info", "msg": "x"}))

    def test_fetch_logs_returns_none_when_unreachable(self, monkeypatch):
        monkeypatch.setattr(complexity_client, "COMPLEXITY_ENGINE_URL", _UNREACHABLE_URL)
        assert asyncio.run(log_client.fetch_logs(10)) is None

    def test_persist_then_fetch_roundtrips_against_real_sidecar(self):
        # `ts` real (no un placeholder viejo hardcodeado) — `GET /log` ordena
        # por timestamp y devuelve los últimos `limit`; con un `ts` antiguo,
        # este marker terminaba fuera de esa ventana apenas los archivos
        # `.cbor` acumulaban suficientes entradas reales más nuevas (bug real
        # encontrado en vivo: pasaba después de una sesión larga de pytest).
        marker = f"pytest-log-marker-{id(self)}"
        asyncio.run(log_client.persist_log({"ts": now(), "level": "info", "msg": marker}, source="api"))
        result = asyncio.run(log_client.fetch_logs(500))
        assert result is not None
        assert any(entry["msg"] == marker for entry in result["logs"])

    def test_get_logs_endpoint_returns_unified_shape(self):
        r = client.get("/logs?limit=5")
        assert r.status_code == 200
        data = r.json()
        assert "logs" in data and "total" in data
