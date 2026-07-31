"""
Tests — Upload Router (FastAPI)
pytest tests/test_upload.py -v
"""

import io
import json
import zipfile
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

# Importar la app
import sys

sys.path.insert(0, str(Path(__file__).parent.parent))

from main import app

client = TestClient(app)


# ─── Fixtures ─────────────────────────────────────────────────────────────────


def _make_zip(files: dict[str, str]) -> bytes:
    """Crea un ZIP en memoria con los archivos especificados."""
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as zf:
        for name, content in files.items():
            zf.writestr(name, content)
    return buf.getvalue()


# ─── Health ───────────────────────────────────────────────────────────────────


class TestHealth:
    def test_health_ok(self):
        res = client.get("/health")
        assert res.status_code == 200
        data = res.json()
        assert data["status"] == "ok"
        assert "capabilities" in data
        assert "capabilities" in data
        assert "flake8" in data["capabilities"]
        assert "pylint" in data["capabilities"]

    def test_capabilities(self):
        res = client.get("/capabilities")
        assert res.status_code == 200
        data = res.json()
        # El nuevo /capabilities retorna flags directamente (HAS_*) y versiones
        assert "python" in data
        assert "server" in data
        assert any(k.startswith("HAS_") for k in data)


# ─── Upload files ─────────────────────────────────────────────────────────────


class TestUploadFiles:
    def test_upload_single_file(self):
        res = client.post(
            "/api/upload/files",
            files=[("files", ("hello.py", b"print('hola')", "text/plain"))],
        )
        assert res.status_code == 200
        data = res.json()
        assert data["project_id"]
        assert data["total_files"] == 1
        assert data["type"] == "files"
        assert data["tree"]["type"] == "directory"

    def test_upload_multiple_files(self):
        res = client.post(
            "/api/upload/files",
            files=[
                ("files", ("app.py", b"x = 1", "text/plain")),
                ("files", ("main.ts", b"const x = 1;", "text/plain")),
            ],
        )
        assert res.status_code == 200
        data = res.json()
        assert data["total_files"] == 2
        children_names = {c["name"] for c in data["tree"]["children"]}
        assert "app.py" in children_names
        assert "main.ts" in children_names

    def test_upload_with_project_name(self):
        res = client.post(
            "/api/upload/files",
            data={"project_name": "mi-proyecto-test"},
            files=[("files", ("readme.md", b"# Hola", "text/plain"))],
        )
        assert res.status_code == 200
        assert res.json()["project_name"] == "mi-proyecto-test"

    def test_upload_no_files_returns_4xx(self):
        # FastAPI retorna 422 cuando falta el campo requerido 'files'
        res = client.post("/api/upload/files", files=[])
        assert res.status_code in (400, 422)

    def test_upload_blocked_extension(self):
        res = client.post(
            "/api/upload/files",
            files=[("files", ("malware.exe", b"\x4d\x5a", "application/octet-stream"))],
        )
        assert res.status_code == 200
        data = res.json()
        # El archivo debe ir a errors, no a saved
        assert data["total_files"] == 0
        assert len(data["errors"]) == 1
        assert "Extensión" in data["errors"][0]["reason"]


# ─── Upload folder ────────────────────────────────────────────────────────────


class TestUploadFolder:
    def test_upload_folder_structure(self):
        res = client.post(
            "/api/upload/folder",
            files=[
                ("files", ("src/app.ts", b"export {}", "text/plain")),
                ("files", ("src/components/btn.ts", b"export {}", "text/plain")),
                ("files", ("package.json", b"{}", "text/plain")),
            ],
        )
        assert res.status_code == 200
        data = res.json()
        assert data["type"] == "folder"
        assert data["total_files"] == 3
        # El árbol debe tener subdirectorio src
        children_names = {c["name"] for c in data["tree"]["children"]}
        assert "src" in children_names
        assert "package.json" in children_names

    def test_upload_empty_folder_returns_4xx(self):
        # FastAPI retorna 422 cuando falta el campo requerido 'files'
        res = client.post("/api/upload/folder", files=[])
        assert res.status_code in (400, 422)


# ─── Upload ZIP ───────────────────────────────────────────────────────────────


class TestUploadZip:
    def test_upload_valid_zip(self):
        zip_bytes = _make_zip(
            {
                "app.py": "print('hello')",
                "src/utils.py": "def helper(): pass",
                "requirements.txt": "fastapi\nuvicorn",
            }
        )
        res = client.post(
            "/api/upload/zip",
            files=[("file", ("project.zip", zip_bytes, "application/zip"))],
        )
        assert res.status_code == 200
        data = res.json()
        assert data["type"] == "zip"
        assert data["extracted"] == 3
        assert data["skipped"] == 0
        assert data["tree"]["type"] == "directory"

    def test_upload_zip_with_project_name(self):
        zip_bytes = _make_zip({"readme.md": "# Test"})
        res = client.post(
            "/api/upload/zip",
            data={"project_name": "my-zip-project"},
            files=[("file", ("test.zip", zip_bytes, "application/zip"))],
        )
        assert res.status_code == 200
        assert res.json()["project_name"] == "my-zip-project"

    def test_upload_non_zip_returns_400(self):
        res = client.post(
            "/api/upload/zip",
            files=[("file", ("notazip.txt", b"texto plano", "text/plain"))],
        )
        assert res.status_code == 400

    def test_upload_zip_skips_blocked_extensions(self):
        zip_bytes = _make_zip(
            {
                "good.py": "print('ok')",
                "bad.exe": "\x4d\x5a",
            }
        )
        res = client.post(
            "/api/upload/zip",
            files=[("file", ("mixed.zip", zip_bytes, "application/zip"))],
        )
        assert res.status_code == 200
        data = res.json()
        assert data["extracted"] == 1  # Solo good.py
        assert data["skipped"] >= 1  # bad.exe fue bloqueado

    def test_upload_zip_invalid_content_returns_400(self):
        res = client.post(
            "/api/upload/zip",
            files=[("file", ("fake.zip", b"not a zip at all", "application/zip"))],
        )
        assert res.status_code == 400


# ─── Project tree ─────────────────────────────────────────────────────────────


class TestProjectTree:
    def _create_project(self) -> str:
        res = client.post(
            "/api/upload/files",
            files=[
                ("files", ("app.py", b"x = 1", "text/plain")),
                ("files", ("main.ts", b"y = 2", "text/plain")),
            ],
        )
        return res.json()["project_id"]

    def test_get_project_tree(self):
        project_id = self._create_project()
        res = client.get(f"/api/upload/projects/{project_id}/tree")
        assert res.status_code == 200
        data = res.json()
        assert data["project_id"] == project_id
        assert "tree" in data
        assert "info" in data

    def test_get_nonexistent_project_returns_404(self):
        res = client.get("/api/upload/projects/nonexistent-id-123/tree")
        assert res.status_code == 404

    def test_get_file_content(self):
        project_id = self._create_project()
        res = client.get(f"/api/upload/projects/{project_id}/file?path=app.py")
        assert res.status_code == 200
        data = res.json()
        assert data["content"] == "x = 1"
        assert data["extension"] == ".py"

    def test_path_traversal_blocked(self):
        project_id = self._create_project()
        res = client.get(f"/api/upload/projects/{project_id}/file?path=../../etc/passwd")
        # Debe retornar 400 o 404, nunca el archivo
        assert res.status_code in (400, 404)


# ─── Projects list ────────────────────────────────────────────────────────────


class TestProjectsList:
    def test_list_projects(self):
        # Subir un proyecto primero
        client.post(
            "/api/upload/files",
            files=[("files", ("dummy.py", b"pass", "text/plain"))],
        )
        res = client.get("/api/upload/projects")
        assert res.status_code == 200
        data = res.json()
        assert "projects" in data
        assert "total" in data
        assert isinstance(data["projects"], list)

    def test_delete_project(self):
        # Crear proyecto
        upload_res = client.post(
            "/api/upload/files",
            files=[("files", ("to_delete.py", b"pass", "text/plain"))],
        )
        project_id = upload_res.json()["project_id"]

        # Eliminar
        del_res = client.delete(f"/api/upload/projects/{project_id}")
        assert del_res.status_code == 200
        assert "eliminado" in del_res.json()["message"]

        # Verificar que ya no existe
        tree_res = client.get(f"/api/upload/projects/{project_id}/tree")
        assert tree_res.status_code == 404

    def test_delete_nonexistent_returns_404(self):
        res = client.delete("/api/upload/projects/fake-id-xyz")
        assert res.status_code == 404


# ─── ProjectService unit tests ────────────────────────────────────────────────


class TestProjectService:
    def test_build_tree_structure(self, tmp_path: Path):
        from services.project_service import build_tree

        (tmp_path / "app.py").write_text("print('hello')")
        (tmp_path / "src").mkdir()
        (tmp_path / "src" / "utils.py").write_text("def helper(): pass")

        tree = build_tree(tmp_path)

        assert tree["type"] == "directory"
        children_names = {c["name"] for c in tree["children"]}
        assert "app.py" in children_names
        assert "src" in children_names

    def test_extract_zip_extracts_files(self, tmp_path: Path):
        from services.project_service import extract_zip

        zip_bytes = _make_zip(
            {
                "hello.py": "print('hello')",
                "nested/world.ts": "export {}",
            }
        )

        result = extract_zip(zip_bytes, tmp_path)

        assert result["extracted"] == 2
        assert result["skipped"] == 0
        assert (tmp_path / "hello.py").exists()
        assert (tmp_path / "nested" / "world.ts").exists()

    def test_extract_zip_blocks_executables(self, tmp_path: Path):
        from services.project_service import extract_zip

        zip_bytes = _make_zip(
            {
                "ok.py": "pass",
                "bad.exe": "\x4d\x5a",
            }
        )

        result = extract_zip(zip_bytes, tmp_path)
        assert result["extracted"] == 1
        assert not (tmp_path / "bad.exe").exists()

    def test_get_project_info(self, tmp_path: Path):
        from services.project_service import get_project_info

        (tmp_path / "main.py").write_text("print('ok')")
        (tmp_path / "readme.md").write_text("# Hello")

        info = get_project_info(tmp_path)

        assert info["total_files"] == 2
        assert info["code_files"] == 2
        assert info["total_size"] > 0
        assert ".py" in info["by_extension"]

    def test_fmt_size(self):
        from services.project_service import _fmt_size

        assert "B" in _fmt_size(500)
        assert "KB" in _fmt_size(2048)
        assert "MB" in _fmt_size(2 * 1024 * 1024)
