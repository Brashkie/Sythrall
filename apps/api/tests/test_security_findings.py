"""
Tests — Security & Taint Intelligence (Fase 21 v1)
pytest tests/test_security_findings.py -v

Ejercita services/static_parser.py::_security_findings_python /
_hardcoded_credentials_python directo (via /static/parse), sin sidecar —
esta pasada es 100% Python, no toca Rust.
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from fastapi.testclient import TestClient

from main import app

client = TestClient(app)


def _findings(code: str) -> list[dict]:
    r = client.post("/static/parse", json={"filename": "test.py", "content": code})
    assert r.status_code == 200
    return r.json()["security_findings"]


class TestSQLInjection:
    def test_concatenated_query_flagged(self):
        code = """
def get_user(request):
    username = request.args["username"]
    query = "SELECT * FROM users WHERE name=" + username
    db.execute(query)
"""
        findings = _findings(code)
        sqli = [f for f in findings if f["cwe"] == "CWE-89"]
        assert len(sqli) == 1
        assert sqli[0]["confidence"] == "High"
        assert sqli[0]["function"] == "get_user"
        assert sqli[0]["source"] == "solicitud HTTP"

    def test_fstring_query_flagged(self):
        code = """
def get_user(request):
    username = request.args["username"]
    db.execute(f"SELECT * FROM users WHERE name={username}")
"""
        findings = _findings(code)
        assert any(f["cwe"] == "CWE-89" for f in findings)

    def test_parameterized_query_not_flagged(self):
        code = """
def get_user(request):
    username = request.args["username"]
    db.execute("SELECT * FROM users WHERE name=%s", (username,))
"""
        findings = _findings(code)
        assert not any(f["cwe"] == "CWE-89" for f in findings)

    def test_static_query_not_flagged(self):
        code = """
def list_users():
    db.execute("SELECT * FROM users")
"""
        findings = _findings(code)
        assert not any(f["cwe"] == "CWE-89" for f in findings)

    def test_untainted_concatenation_not_flagged(self):
        code = """
def get_table(table_name):
    query = "SELECT * FROM " + table_name
    db.execute(query)
"""
        findings = _findings(code)
        assert not any(f["cwe"] == "CWE-89" for f in findings)


class TestCommandInjection:
    def test_os_system_with_tainted_input_flagged(self):
        code = """
import os

def run(request):
    cmd = request.args["cmd"]
    os.system(cmd)
"""
        findings = _findings(code)
        cmdi = [f for f in findings if f["cwe"] == "CWE-78"]
        assert len(cmdi) == 1
        assert cmdi[0]["sink"] == "os.system(...)"

    def test_subprocess_shell_true_flagged(self):
        code = """
import subprocess

def run(request):
    cmd = request.args["cmd"]
    subprocess.run(cmd, shell=True)
"""
        findings = _findings(code)
        assert any(f["cwe"] == "CWE-78" for f in findings)

    def test_subprocess_list_args_not_flagged(self):
        code = """
import subprocess

def run(user_input):
    subprocess.run(["ls", user_input])
"""
        findings = _findings(code)
        assert not any(f["cwe"] == "CWE-78" for f in findings)

    def test_subprocess_without_shell_true_not_flagged(self):
        code = """
import subprocess

def run(request):
    cmd = request.args["cmd"]
    subprocess.run(cmd)
"""
        findings = _findings(code)
        assert not any(f["cwe"] == "CWE-78" for f in findings)

    def test_reassignment_to_safe_value_clears_taint(self):
        """Regresión: una reasignación a un literal seguro debe limpiar el
        taint previo, no dejarlo pegado por el resto de la función."""
        code = """
import os

def run(request):
    cmd = request.args.get("x")
    cmd = "ls -la"
    os.system(cmd)
"""
        findings = _findings(code)
        assert findings == []

    def test_nested_function_same_name_does_not_leak_taint(self):
        """Regresión: una función anidada que reusa el nombre de una variable
        del scope externo no debe contaminar el taint del scope externo."""
        code = """
import os

def outer(request):
    cmd = "ls -la"
    def inner():
        cmd = request.args.get("x")
        return cmd
    os.system(cmd)
"""
        findings = _findings(code)
        assert not any(f["function"] == "outer" for f in findings)


class TestHardcodedCredentials:
    def test_hardcoded_api_key_flagged(self):
        code = 'API_KEY = "sk-live-abc123def456"\n'
        findings = _findings(code)
        cwe798 = [f for f in findings if f["cwe"] == "CWE-798"]
        assert len(cwe798) == 1
        assert cwe798[0]["confidence"] == "Medium"

    def test_empty_placeholder_not_flagged(self):
        code = 'DB_PASSWORD = ""\n'
        findings = _findings(code)
        assert not any(f["cwe"] == "CWE-798" for f in findings)

    def test_changeme_placeholder_not_flagged(self):
        code = 'SECRET_KEY = "changeme"\n'
        findings = _findings(code)
        assert not any(f["cwe"] == "CWE-798" for f in findings)

    def test_angle_bracket_placeholder_not_flagged(self):
        code = 'API_KEY = "<your-api-key>"\n'
        findings = _findings(code)
        assert not any(f["cwe"] == "CWE-798" for f in findings)

    def test_unrelated_variable_name_not_flagged(self):
        code = 'GREETING = "hello world"\n'
        findings = _findings(code)
        assert not any(f["cwe"] == "CWE-798" for f in findings)

    def test_computed_value_not_flagged(self):
        code = """
def make_token():
    password_hash = hash_function(input())
"""
        findings = _findings(code)
        assert not any(f["cwe"] == "CWE-798" for f in findings)


class TestFindingShape:
    def test_findings_sorted_by_line(self):
        code = """
API_KEY = "sk-live-abc123"

def get_user(request):
    username = request.args["username"]
    db.execute("SELECT * FROM users WHERE name=" + username)
"""
        findings = _findings(code)
        lines = [f["line"] for f in findings]
        assert lines == sorted(lines)

    def test_every_finding_has_evidence_fields(self):
        code = """
def get_user(request):
    username = request.args["username"]
    db.execute("SELECT * FROM users WHERE name=" + username)
"""
        for f in _findings(code):
            for key in ("cwe", "category", "severity", "confidence", "source", "line", "recommendation"):
                assert key in f

    def test_clean_file_has_no_findings(self):
        code = """
def add(a, b):
    return a + b
"""
        assert _findings(code) == []

    def test_non_python_file_returns_empty_list_not_error(self):
        r = client.post("/static/parse", json={"filename": "test.ts", "content": "function f() {}"})
        assert r.status_code == 200
        assert r.json()["security_findings"] == []
