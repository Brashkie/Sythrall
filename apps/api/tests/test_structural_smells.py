"""
Tests — Structural Smells (Fase 22, segundo ítem)
pytest tests/test_structural_smells.py -v

Ejercita `services/complexity/src/smells.rs` (Rust-only ahora, ver
`static_parser.py::_parse_python`) vía `/static/parse` — `conftest.py` levanta
el sidecar real para toda la sesión de pytest.
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from fastapi.testclient import TestClient

from main import app

client = TestClient(app)


def _smells(code: str) -> list[dict]:
    r = client.post("/static/parse", json={"filename": "test.py", "content": code})
    assert r.status_code == 200
    return r.json()["structural_smells"]


class TestLongFunction:
    def test_long_function_flagged(self):
        code = "def f():\n" + "    x = 1\n" * 55 + "    return x\n"
        smells = _smells(code)
        assert any(s["kind"] == "long_function" for s in smells)

    def test_short_function_not_flagged(self):
        code = "def f():\n    return 1\n"
        smells = _smells(code)
        assert not any(s["kind"] == "long_function" for s in smells)


class TestExcessiveParameters:
    def test_many_params_flagged(self):
        code = "def f(a, b, c, d, e, f):\n    return a\n"
        smells = _smells(code)
        assert any(s["kind"] == "excessive_parameters" for s in smells)

    def test_five_params_not_flagged(self):
        code = "def f(a, b, c, d, e):\n    return a\n"
        smells = _smells(code)
        assert not any(s["kind"] == "excessive_parameters" for s in smells)


class TestDeepNesting:
    def test_five_levels_flagged(self):
        code = (
            "def f(a, b, c, d, e):\n"
            "    if a:\n"
            "        if b:\n"
            "            if c:\n"
            "                if d:\n"
            "                    if e:\n"
            "                        return 1\n"
            "    return 0\n"
        )
        smells = _smells(code)
        assert any(s["kind"] == "deep_nesting" for s in smells)

    def test_two_levels_not_flagged(self):
        code = "def f(x):\n    if x:\n        if x > 1:\n            return 1\n    return 0\n"
        smells = _smells(code)
        assert not any(s["kind"] == "deep_nesting" for s in smells)


class TestLargeClass:
    def test_many_methods_flagged(self):
        code = "class C:\n" + "\n".join(f"    def m{i}(self):\n        pass" for i in range(16))
        smells = _smells(code)
        assert any(s["kind"] == "large_class" for s in smells)

    def test_few_methods_not_flagged(self):
        code = "class C:\n    def a(self):\n        pass\n    def b(self):\n        pass\n"
        smells = _smells(code)
        assert not any(s["kind"] == "large_class" for s in smells)


class TestGodObject:
    def test_many_methods_and_attrs_flagged(self):
        attrs = "    def __init__(self):\n" + "\n".join(f"        self.a{i} = {i}" for i in range(12))
        methods = "\n".join(f"    def m{i}(self):\n        pass" for i in range(25))
        code = f"class C:\n{attrs}\n{methods}\n"
        smells = _smells(code)
        assert any(s["kind"] == "god_object" for s in smells)

    def test_many_methods_but_few_attrs_not_god_object(self):
        # Muchos métodos (dispara large_class) pero pocos atributos propios —
        # no debería ser god_object, la señal necesita ambas cosas.
        methods = "\n".join(f"    def m{i}(self):\n        pass" for i in range(25))
        code = f"class C:\n{methods}\n"
        smells = _smells(code)
        assert not any(s["kind"] == "god_object" for s in smells)
        assert any(s["kind"] == "large_class" for s in smells)


class TestSmellShape:
    def test_clean_code_has_no_smells(self):
        code = "def add(a, b):\n    return a + b\n"
        assert _smells(code) == []

    def test_smell_has_required_fields(self):
        code = "def f(a, b, c, d, e, f):\n    return a\n"
        for s in _smells(code):
            for key in ("kind", "name", "line", "message"):
                assert key in s

    def test_smells_sorted_by_line(self):
        code = "def big(a, b, c, d, e, f):\n" "    return a\n\n\n" "def small():\n" "    return 1\n"
        smells = _smells(code)
        lines = [s["line"] for s in smells]
        assert lines == sorted(lines)

    def test_non_python_file_returns_empty_list_not_error(self):
        r = client.post("/static/parse", json={"filename": "test.ts", "content": "function f() {}"})
        assert r.status_code == 200
        assert r.json()["structural_smells"] == []
