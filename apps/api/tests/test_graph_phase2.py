"""
Tests — Code Graph Fase 2: proyectos subidos + dir tree + cross-module deps
pytest tests/test_graph_phase2.py -v
"""

import sys
import io
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from fastapi.testclient import TestClient
from main import app

client = TestClient(app)


# ─── Helpers ─────────────────────────────────────────────────────────────────


def _make_zip(files: dict[str, str]) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as zf:
        for path, content in files.items():
            zf.writestr(path, content)
    buf.seek(0)
    return buf.read()


def _upload_project(files: dict[str, str], name: str = "test") -> str:
    z = _make_zip(files)
    res = client.post(
        "/api/upload/zip",
        files={"file": ("project.zip", z, "application/zip")},
        data={"project_name": name},
    )
    assert res.status_code == 200
    return res.json()["project_id"]


# ─── Fixtures ────────────────────────────────────────────────────────────────

FULLSTACK = {
    "frontend/app.ts": "import { api } from './api'\nimport { router } from './router'\nexport function main() { return api(router()) }\n",
    "frontend/api.ts": "import { fetch } from './utils'\nexport function api(d: unknown) { return fetch(d) }\n",
    "frontend/router.ts": "export function router() { return {} }\n",
    "frontend/utils.ts": "export function fetch(u: unknown) { return u }\n",
    "backend/main.py": "from parser import parse\nfrom analyzer import analyze\ndef run(): return analyze(parse('x'))\n",
    "backend/parser.py": "from lexer import tokenize\ndef parse(s): return tokenize(s)\n",
    "backend/analyzer.py": "from metrics import compute\ndef analyze(d):\n    for i in range(len(d)):\n        for j in range(len(d)): pass\n    return compute(d)\n",
    "backend/lexer.py": "def tokenize(s): return s.split()\n",
    "backend/metrics.py": "def compute(d): return len(d)\n",
}

CIRCULAR_PROJECT = {
    "src/a.py": "from b import b_func\ndef a_func(): return b_func()\n",
    "src/b.py": "from c import c_func\ndef b_func(): return c_func()\n",
    "src/c.py": "from a import a_func\ndef c_func(): return a_func()\n",
}

SINGLE_FOLDER = {
    "main.py": "from utils import helper\ndef main(): return helper()\n",
    "utils.py": "def helper(): return 42\n",
    "database.py": "def query(sql): return sql\n",
}


# ─── /analyze/graph/project — básico ─────────────────────────────────────────


class TestProjectGraphBasic:
    def test_endpoint_ok(self):
        pid = _upload_project(SINGLE_FOLDER, "basic")
        r = client.post("/analyze/graph/project", json={"project_id": pid, "graph_type": "import"})
        assert r.status_code == 200

    def test_unknown_project(self):
        r = client.post("/analyze/graph/project", json={"project_id": "nonexistent-id", "graph_type": "import"})
        assert r.status_code == 200
        assert "error" in r.json()

    def test_returns_graph_type(self):
        pid = _upload_project(SINGLE_FOLDER, "t1")
        data = client.post("/analyze/graph/project", json={"project_id": pid, "graph_type": "import"}).json()
        assert data["graph_type"] == "import"

    def test_returns_total_files(self):
        pid = _upload_project(SINGLE_FOLDER, "t2")
        data = client.post("/analyze/graph/project", json={"project_id": pid, "graph_type": "import"}).json()
        assert data["total_files"] == 3

    def test_returns_file_list(self):
        pid = _upload_project(SINGLE_FOLDER, "t3")
        data = client.post("/analyze/graph/project", json={"project_id": pid, "graph_type": "import"}).json()
        assert "file_list" in data
        assert len(data["file_list"]) == 3

    def test_returns_mermaid(self):
        pid = _upload_project(SINGLE_FOLDER, "t4")
        data = client.post("/analyze/graph/project", json={"project_id": pid, "graph_type": "import"}).json()
        assert "flowchart" in data.get("mermaid", "")

    def test_all_4_types_work(self):
        pid = _upload_project(SINGLE_FOLDER, "t5")
        for gtype in ["import", "call", "circular", "heatmap"]:
            r = client.post("/analyze/graph/project", json={"project_id": pid, "graph_type": gtype})
            assert r.status_code == 200, f"Falló para tipo {gtype}"
            assert r.json().get("graph_type") == gtype


# ─── Cross-folder deps y dir tree ────────────────────────────────────────────


class TestProjectGraphFullstack:
    def setup_method(self):
        self.pid = _upload_project(FULLSTACK, "fullstack")

    def test_import_graph_nodes_count(self):
        data = client.post("/analyze/graph/project", json={"project_id": self.pid, "graph_type": "import"}).json()
        assert len(data["nodes"]) == 9

    def test_import_graph_has_edges(self):
        data = client.post("/analyze/graph/project", json={"project_id": self.pid, "graph_type": "import"}).json()
        assert len(data["edges"]) >= 5

    def test_cross_folder_frontend_edges(self):
        """frontend/app.ts → frontend/api.ts, frontend/router.ts"""
        data = client.post("/analyze/graph/project", json={"project_id": self.pid, "graph_type": "import"}).json()
        froms = [e["from"] for e in data["edges"]]
        assert "frontend/app.ts" in froms

    def test_cross_folder_backend_edges(self):
        """backend/main.py → backend/parser.py, backend/analyzer.py"""
        data = client.post("/analyze/graph/project", json={"project_id": self.pid, "graph_type": "import"}).json()
        froms = [e["from"] for e in data["edges"]]
        assert "backend/main.py" in froms

    def test_entry_points_detected(self):
        data = client.post("/analyze/graph/project", json={"project_id": self.pid, "graph_type": "import"}).json()
        eps = data.get("entry_points", [])
        assert len(eps) >= 1
        # Los entry points son los que nadie importa
        assert "frontend/app.ts" in eps or "backend/main.py" in eps

    def test_dir_tree_present(self):
        data = client.post("/analyze/graph/project", json={"project_id": self.pid, "graph_type": "import"}).json()
        assert "dir_tree" in data

    def test_dir_tree_has_folders(self):
        data = client.post("/analyze/graph/project", json={"project_id": self.pid, "graph_type": "import"}).json()
        tree = data["dir_tree"]
        folders = [c["name"] for c in tree.get("children", []) if c["type"] == "directory"]
        assert "frontend" in folders
        assert "backend" in folders

    def test_dir_tree_file_stats(self):
        data = client.post("/analyze/graph/project", json={"project_id": self.pid, "graph_type": "import"}).json()
        tree = data["dir_tree"]

        # Buscar un archivo de código en el árbol
        def find_files(node):
            if node["type"] == "file" and node.get("stats"):
                return [node]
            result = []
            for child in node.get("children", []):
                result.extend(find_files(child))
            return result

        files = find_files(tree)
        assert len(files) >= 1
        for f in files:
            s = f["stats"]
            assert "functions" in s
            assert "language" in s

    def test_dir_tree_sorted(self):
        """Directorios antes que archivos."""
        data = client.post("/analyze/graph/project", json={"project_id": self.pid, "graph_type": "import"}).json()
        tree = data["dir_tree"]
        if len(tree.get("children", [])) >= 2:
            types = [c["type"] for c in tree["children"]]
            dirs = [i for i, t in enumerate(types) if t == "directory"]
            files = [i for i, t in enumerate(types) if t == "file"]
            if dirs and files:
                assert max(dirs) < min(files)

    def test_heatmap_uses_full_paths(self):
        data = client.post("/analyze/graph/project", json={"project_id": self.pid, "graph_type": "heatmap"}).json()
        fns = data.get("functions", [])
        # Los paths deben incluir la carpeta
        paths = [f["file"] for f in fns]
        assert any("/" in p or "\\" in p for p in paths)


# ─── Circular deps en proyecto ────────────────────────────────────────────────


class TestProjectCircular:
    def setup_method(self):
        self.pid = _upload_project(CIRCULAR_PROJECT, "circular-test")

    def test_circular_detected(self):
        data = client.post("/analyze/graph/project", json={"project_id": self.pid, "graph_type": "circular"}).json()
        assert data.get("has_cycles") is True

    def test_circular_cycle_contains_project_files(self):
        data = client.post("/analyze/graph/project", json={"project_id": self.pid, "graph_type": "circular"}).json()
        cycle_files = set()
        for c in data.get("cycles", []):
            cycle_files.update(c)
        assert len(cycle_files) >= 2

    def test_circular_affected_nodes_marked(self):
        data = client.post("/analyze/graph/project", json={"project_id": self.pid, "graph_type": "circular"}).json()
        in_cycle = [n for n in data["nodes"] if n.get("in_cycle")]
        assert len(in_cycle) >= 2

    def test_no_circular_project(self):
        pid = _upload_project(FULLSTACK, "no-circ")
        data = client.post("/analyze/graph/project", json={"project_id": pid, "graph_type": "circular"}).json()
        assert data.get("has_cycles") is False

    def test_circular_summary(self):
        data = client.post("/analyze/graph/project", json={"project_id": self.pid, "graph_type": "circular"}).json()
        s = data.get("summary", {})
        assert s.get("total_cycles", 0) >= 1
        assert len(s.get("cycle_descriptions", [])) >= 1


# ─── Resolución de módulos mejorada ──────────────────────────────────────────


class TestModuleResolution:
    def test_same_folder_resolution(self):
        """frontend/app.ts → import './api' → debe resolver a frontend/api.ts"""
        pid = _upload_project(FULLSTACK, "res-test")
        data = client.post("/analyze/graph/project", json={"project_id": pid, "graph_type": "import"}).json()
        edges = [(e["from"], e["to"]) for e in data["edges"]]
        assert ("frontend/app.ts", "frontend/api.ts") in edges

    def test_backend_same_folder(self):
        """backend/main.py → from parser import → debe resolver a backend/parser.py"""
        pid = _upload_project(FULLSTACK, "res-test2")
        data = client.post("/analyze/graph/project", json={"project_id": pid, "graph_type": "import"}).json()
        edges = [(e["from"], e["to"]) for e in data["edges"]]
        assert ("backend/main.py", "backend/parser.py") in edges

    def test_no_false_cross_folder_edges(self):
        """frontend/app.ts NO debe conectar con backend/ si no hay import explícito."""
        pid = _upload_project(FULLSTACK, "res-test3")
        data = client.post("/analyze/graph/project", json={"project_id": pid, "graph_type": "import"}).json()
        cross = [
            (e["from"], e["to"])
            for e in data["edges"]
            if "frontend" in e["from"] and "backend" in e["to"] or "backend" in e["from"] and "frontend" in e["to"]
        ]
        assert len(cross) == 0  # sin cross-folder deps en este proyecto


# ─── /analyze/project y /static/parse-project con project_id ─────────────────
# Mismo patrón que /analyze/graph/project: en vez de que el frontend mande el
# contenido de cada archivo, se le pasa un project_id y el backend lee del
# disco (services/project_service.py:read_project_files). Ver "concepto de
# proyecto activo" en CHANGELOG.md.


class TestAnalyzeProjectById:
    def test_project_id_reads_from_disk(self):
        pid = _upload_project(CIRCULAR_PROJECT, "ap-test1")
        res = client.post("/analyze/project", json={"project_id": pid, "tools": ["ast"]})
        assert res.status_code == 200
        data = res.json()["files"]
        assert set(data.keys()) == {"src/a.py", "src/b.py", "src/c.py"}

    def test_project_id_takes_precedence_over_files(self):
        """Si vienen los dos, gana project_id (files se ignora) — mismo criterio que /analyze/graph/project."""
        pid = _upload_project(CIRCULAR_PROJECT, "ap-test2")
        res = client.post(
            "/analyze/project",
            json={"project_id": pid, "files": [{"filename": "ignored.py", "content": "x = 1\n"}], "tools": ["ast"]},
        )
        data = res.json()["files"]
        assert "ignored.py" not in data
        assert "src/a.py" in data

    def test_nonexistent_project_id_returns_empty(self):
        res = client.post("/analyze/project", json={"project_id": "nonexistent-id", "tools": ["ast"]})
        assert res.status_code == 200
        assert res.json()["files"] == {}

    def test_flake8_issues_attributed_correctly_from_disk(self):
        pid = _upload_project({"bad.py": "import os\nx=1\n"}, "ap-test3")
        res = client.post("/analyze/project", json={"project_id": pid, "tools": ["flake8"]})
        data = res.json()["files"]
        assert data["bad.py"]["issues"], "bad.py debería tener issues de flake8 leído del disco"


class TestParseProjectById:
    def test_project_id_reads_from_disk(self):
        pid = _upload_project(CIRCULAR_PROJECT, "pp-test1")
        res = client.post("/static/parse-project", json={"project_id": pid})
        assert res.status_code == 200
        data = res.json()
        assert data["summary"]["total_files"] == 3

    def test_nonexistent_project_id_returns_empty_summary(self):
        res = client.post("/static/parse-project", json={"project_id": "nonexistent-id"})
        assert res.status_code == 200
        assert res.json()["summary"]["total_files"] == 0
