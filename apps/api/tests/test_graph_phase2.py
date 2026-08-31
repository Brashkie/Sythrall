"""
Tests — Code Graph Fase 2: proyectos subidos + dir tree + cross-module deps
pytest tests/test_graph_phase2.py -v

Los contenidos de nodos/edges/cycles/mermaid/resolución de módulos entre
carpetas (`TestModuleResolution`, la mayoría de `TestProjectGraphFullstack`/
`TestProjectCircular`) eran relay puro a `services/complexity/src/graph.rs`
(`build_project_edges`/`module_to_candidates` viven ahí desde la Fase 18,
ver el propio docstring de `_build_import_graph` en `routers/graph.py`) —
eliminados (2026-08-31, misma política aplicada a toda la carpeta de tests).
Lo que queda es genuinamente Python: `total_files`/`file_list` (calculados
acá con `len(parsed_files)`, no en Rust), `dir_tree` (`_build_dir_tree`, sin
equivalente en Rust), el error-path de proyecto inexistente, y
`test_all_5_types_work` — un smoke test de la rama `if/elif` de
`generate_project_graph` que existe por una razón concreta documentada en su
propio docstring: atrapó un bug real de `async`/`await` que ningún test de
Rust podría haber visto, porque el bug estaba en el lado Python del
dispatch, no en el cómputo.
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
    def test_unknown_project(self):
        r = client.post("/analyze/graph/project", json={"project_id": "nonexistent-id", "graph_type": "import"})
        assert r.status_code == 200
        assert "error" in r.json()

    def test_returns_total_files(self):
        pid = _upload_project(SINGLE_FOLDER, "t2")
        data = client.post("/analyze/graph/project", json={"project_id": pid, "graph_type": "import"}).json()
        assert data["total_files"] == 3

    def test_returns_file_list(self):
        pid = _upload_project(SINGLE_FOLDER, "t3")
        data = client.post("/analyze/graph/project", json={"project_id": pid, "graph_type": "import"}).json()
        assert "file_list" in data
        assert len(data["file_list"]) == 3

    def test_all_5_types_work(self):
        """ "centrality" agregado acá — el test original ("test_all_4_types_work")
        es de antes de que centrality existiera como 5º tipo, así que la rama
        `graph_type == "centrality"` de `generate_project_graph` (a diferencia
        de `generate_graph`, cubierta por TestCentralityGraph en test_graph.py)
        no tenía ningún test que la ejercite — el mismo tipo de bug que se
        encontró en el slice de Import Graph (llamar a una función que pasó a
        `async` sin `await`, devolviendo un coroutine en vez de un dict) no
        tendría quién lo atrape acá."""
        pid = _upload_project(SINGLE_FOLDER, "t5")
        for gtype in ["import", "call", "circular", "heatmap", "centrality"]:
            r = client.post("/analyze/graph/project", json={"project_id": pid, "graph_type": gtype})
            assert r.status_code == 200, f"Falló para tipo {gtype}"
            assert r.json().get("graph_type") == gtype


# ─── Dir tree (Python puro, sin equivalente en Rust) ─────────────────────────


class TestProjectGraphFullstack:
    def setup_method(self):
        self.pid = _upload_project(FULLSTACK, "fullstack")

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


# ─── /analyze/project y /static/parse-project con project_id ─────────────────
# Mismo patrón que /analyze/graph/project: en vez de que el frontend mande el
# contenido de cada archivo, se le pasa un project_id y el backend lee del
# disco (services/project_service.py:read_project_files). Ver "concepto de
# proyecto activo" en CHANGELOG.md. 100% Python (lectura de disco + AST/
# flake8, mismo criterio que TestAnalyzeProject en test_analysis.py).


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
