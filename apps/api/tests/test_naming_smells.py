"""
Tests — Naming Smells (Fase 22, tercer ítem)
pytest tests/test_naming_smells.py -v

Ejercita `services/complexity/src/naming.rs` (Rust-only ahora, ver
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
    return r.json()["naming_smells"]


class TestSingleLetterName:
    def test_single_letter_variable_flagged(self):
        code = "def f():\n    x = compute()\n    return x\n"
        smells = _smells(code)
        assert any(s["kind"] == "single_letter_name" and s["name"] == "x" for s in smells)

    def test_for_loop_target_not_flagged(self):
        code = "def f(arr):\n    for i in range(len(arr)):\n        arr[i] += 1\n    return arr\n"
        smells = _smells(code)
        assert not any(s["kind"] == "single_letter_name" for s in smells)

    def test_comprehension_target_not_flagged(self):
        code = "def f(arr):\n    return [x * 2 for x in arr]\n"
        smells = _smells(code)
        assert not any(s["kind"] == "single_letter_name" for s in smells)

    def test_single_letter_parameter_not_flagged(self):
        # `def add(a, b)` es idiomático — solo variables asignadas en el
        # cuerpo se marcan, no parámetros.
        code = "def add(a, b):\n    return a + b\n"
        smells = _smells(code)
        assert not any(s["kind"] == "single_letter_name" for s in smells)

    def test_multi_letter_variable_not_flagged(self):
        code = "def f():\n    total = compute()\n    return total\n"
        smells = _smells(code)
        assert not any(s["kind"] == "single_letter_name" for s in smells)


class TestInconsistentCasing:
    def test_mixed_snake_and_camel_flagged(self):
        code = "def do_thing(value):\n    return value\n\ndef doOtherThing(value):\n    return value\n"
        smells = _smells(code)
        assert any(s["kind"] == "inconsistent_casing" for s in smells)

    def test_only_snake_case_not_flagged(self):
        code = "def do_thing(value):\n    return value\n\ndef do_other_thing(value):\n    return value\n"
        smells = _smells(code)
        assert not any(s["kind"] == "inconsistent_casing" for s in smells)

    def test_only_camel_case_not_flagged(self):
        code = "def doThing(value):\n    return value\n\ndef doOtherThing(value):\n    return value\n"
        smells = _smells(code)
        assert not any(s["kind"] == "inconsistent_casing" for s in smells)


class TestShadowedName:
    def test_nested_function_shadows_outer_local_flagged(self):
        code = (
            "def outer():\n"
            "    total = 0\n"
            "    def inner():\n"
            "        total = 1\n"
            "        return total\n"
            "    return inner()\n"
        )
        smells = _smells(code)
        assert any(s["kind"] == "shadowed_name" and s["name"] == "inner" for s in smells)

    def test_nested_function_shadows_module_global_flagged(self):
        code = "config = {}\n\ndef load(config):\n    return config\n"
        smells = _smells(code)
        assert any(s["kind"] == "shadowed_name" and s["name"] == "load" for s in smells)

    def test_no_shadowing_not_flagged(self):
        code = "def outer():\n    total = 0\n    def inner():\n        count = 1\n        return count\n    return inner()\n"
        smells = _smells(code)
        assert not any(s["kind"] == "shadowed_name" for s in smells)

    def test_sibling_functions_do_not_shadow_each_other(self):
        code = "def a():\n    value = 1\n    return value\n\ndef b():\n    value = 2\n    return value\n"
        smells = _smells(code)
        assert not any(s["kind"] == "shadowed_name" for s in smells)


class TestNamingSmellShape:
    def test_clean_code_has_no_smells(self):
        code = "def add(a, b):\n    return a + b\n"
        assert _smells(code) == []

    def test_smell_has_required_fields(self):
        code = "def f():\n    x = 1\n    return x\n"
        for s in _smells(code):
            for key in ("kind", "name", "line", "message"):
                assert key in s

    def test_smells_sorted_by_line(self):
        code = "def f():\n    y = 1\n    return y\n\n\ndef g():\n    x = 1\n    return x\n"
        smells = _smells(code)
        lines = [s["line"] for s in smells]
        assert lines == sorted(lines)

    def test_non_python_file_returns_empty_list_not_error(self):
        r = client.post("/static/parse", json={"filename": "test.ts", "content": "function f() {}"})
        assert r.status_code == 200
        assert r.json()["naming_smells"] == []
