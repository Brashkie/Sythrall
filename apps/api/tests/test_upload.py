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


# ─── Append a un proyecto existente (project_id) ──────────────────────────────
# Habilita que "+ Código"/"+ Carpeta" del sidebar sumen al proyecto activo en
# vez de crear uno nuevo cada vez. Ver CHANGELOG "Un solo punto de entrada".


class TestUploadAppendToExistingProject:
    def test_second_upload_with_project_id_appends(self):
        first = client.post(
            "/api/upload/files",
            files=[("files", ("a.py", b"x = 1", "text/plain"))],
        )
        pid = first.json()["project_id"]

        second = client.post(
            "/api/upload/files",
            data={"project_id": pid},
            files=[("files", ("b.py", b"y = 2", "text/plain"))],
        )
        assert second.status_code == 200
        data = second.json()
        assert data["project_id"] == pid  # mismo proyecto, no uno nuevo

        tree = client.get(f"/api/upload/projects/{pid}/tree").json()["tree"]
        names = {c["name"] for c in tree["children"]}
        assert names == {"a.py", "b.py"}  # ambos archivos conviven

    def test_append_to_nonexistent_project_returns_404(self):
        res = client.post(
            "/api/upload/files",
            data={"project_id": "nonexistent-id"},
            files=[("files", ("a.py", b"x = 1", "text/plain"))],
        )
        assert res.status_code == 404

    def test_appending_without_a_name_preserves_the_original_name(self):
        # resolve_project_name() existe específicamente para esto: agregar
        # archivos a un proyecto ya nombrado no debe pisarle el nombre con el
        # fallback genérico basado en el id, solo porque la request de "append"
        # no manda project_name (así es como "+ Código" agrega al proyecto
        # activo, ver components/app.ts::persistFilesToProject).
        first = client.post(
            "/api/upload/files",
            data={"project_name": "mi-proyecto-nombrado"},
            files=[("files", ("a.py", b"x = 1", "text/plain"))],
        )
        pid = first.json()["project_id"]
        assert first.json()["project_name"] == "mi-proyecto-nombrado"

        second = client.post(
            "/api/upload/files",
            data={"project_id": pid},
            files=[("files", ("b.py", b"y = 2", "text/plain"))],
        )
        assert second.status_code == 200
        assert second.json()["project_name"] == "mi-proyecto-nombrado"

        # Y el nombre queda persistido de verdad — no solo en la respuesta de
        # esta request — así que "Proyectos recientes" (GET /projects) lo
        # sigue mostrando después.
        projects = client.get("/api/upload/projects").json()["projects"]
        proj = next(p for p in projects if p["project_id"] == pid)
        assert proj["project_name"] == "mi-proyecto-nombrado"

    def test_append_via_folder_endpoint(self):
        first = client.post(
            "/api/upload/folder",
            files=[("files", ("myproject/src/a.py", b"x = 1", "text/plain"))],
        )
        pid = first.json()["project_id"]

        second = client.post(
            "/api/upload/folder",
            data={"project_id": pid},
            files=[("files", ("myproject/src/b.py", b"y = 2", "text/plain"))],
        )
        assert second.status_code == 200
        assert second.json()["project_id"] == pid

        tree = client.get(f"/api/upload/projects/{pid}/tree").json()["tree"]
        src_children = {c["name"] for d in tree["children"] if d["name"] == "src" for c in d["children"]}
        assert src_children == {"a.py", "b.py"}


# ─── Upload folder ────────────────────────────────────────────────────────────


class TestUploadFolder:
    def test_upload_folder_structure(self):
        # Nombres realistas de `webkitRelativePath` — el browser SIEMPRE
        # antepone el nombre de la carpeta elegida ("myproject") a cada
        # archivo, nunca manda "src/app.ts" pelado como se simulaba antes acá.
        res = client.post(
            "/api/upload/folder",
            files=[
                ("files", ("myproject/src/app.ts", b"export {}", "text/plain")),
                ("files", ("myproject/src/components/btn.ts", b"export {}", "text/plain")),
                ("files", ("myproject/package.json", b"{}", "text/plain")),
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

    def test_upload_folder_strips_top_level_wrapper(self):
        """Regresión: el picker de carpeta manda "myproject/..." en cada
        archivo — sin descartar ese primer segmento, los archivos quedaban
        anidados un nivel de más (myproject/src/... en vez de src/... en la
        raíz), y el árbol recién subido mostraba solo una carpeta "myproject"
        colapsada — reportado por el usuario como "solo aparece carpetas"."""
        res = client.post(
            "/api/upload/folder",
            files=[
                ("files", ("myproject/src/app.ts", b"export {}", "text/plain")),
                ("files", ("myproject/README.md", b"# demo", "text/plain")),
            ],
        )
        assert res.status_code == 200
        data = res.json()
        children_names = {c["name"] for c in data["tree"]["children"]}
        # "myproject" NO debe aparecer como carpeta — src/ y README.md deben
        # estar directo en la raíz del árbol, sin el wrapper de más.
        assert "myproject" not in children_names
        assert "src" in children_names
        assert "README.md" in children_names

    def test_upload_folder_single_flat_file_not_emptied(self):
        """Caso límite: un solo archivo sin ruta relativa (sin pasar por el
        picker de carpeta real) — `safe_parts` tiene un solo segmento, no debe
        vaciarse al intentar descartar "el primero"."""
        res = client.post(
            "/api/upload/folder",
            files=[("files", ("standalone.py", b"x = 1", "text/plain"))],
        )
        assert res.status_code == 200
        data = res.json()
        assert data["total_files"] == 1
        children_names = {c["name"] for c in data["tree"]["children"]}
        assert "standalone.py" in children_names

    def test_upload_empty_folder_returns_4xx(self):
        # FastAPI retorna 422 cuando falta el campo requerido 'files'
        res = client.post("/api/upload/folder", files=[])
        assert res.status_code in (400, 422)

    def test_upload_folder_skips_ignored_dirs(self):
        """Regresión: subir la carpeta raíz de un proyecto JS típico (con
        node_modules presente, casi inevitable con el picker de carpeta del
        browser) no debía mandar esos archivos a disco — antes se guardaban
        igual, un caso real reportado por el usuario tiró 27614 archivos/
        513 MB en una subida que terminó con la conexión cortada."""
        res = client.post(
            "/api/upload/folder",
            files=[
                ("files", ("myproject/src/app.ts", b"export {}", "text/plain")),
                ("files", ("myproject/node_modules/lodash/index.js", b"module.exports = {}", "text/plain")),
                ("files", ("myproject/.git/HEAD", b"ref: refs/heads/main", "text/plain")),
                ("files", ("myproject/dist/bundle.js", b"//built", "text/plain")),
                ("files", ("myproject/package.json", b"{}", "text/plain")),
            ],
        )
        assert res.status_code == 200
        data = res.json()
        assert data["total_files"] == 2  # solo src/app.ts y package.json
        children_names = {c["name"] for c in data["tree"]["children"]}
        assert "node_modules" not in children_names
        assert ".git" not in children_names
        assert "dist" not in children_names
        assert "src" in children_names
        assert "package.json" in children_names


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


# ─── Crear proyecto vacío + agregar archivos nuevos ───────────────────────────


class TestEmptyProjectAndNewFile:
    def test_create_empty_project(self):
        res = client.post("/api/upload/empty", data={"project_name": "mi-proyecto-vacio"})
        assert res.status_code == 200
        data = res.json()
        assert data["project_name"] == "mi-proyecto-vacio"
        assert data["total_files"] == 0
        assert data["tree"]["children"] == []

    def test_create_empty_project_default_name(self):
        res = client.post("/api/upload/empty")
        assert res.status_code == 200
        assert res.json()["project_name"] == "proyecto-vacío"

    def test_create_file_in_empty_project(self):
        pid = client.post("/api/upload/empty").json()["project_id"]

        res = client.post(
            "/api/upload/projects/{}/file".format(pid),
            data={"path": "src/app.py", "content": "print('hola')"},
        )
        assert res.status_code == 200
        assert res.json()["path"] == "src/app.py"

        # El archivo debe poder leerse de vuelta con el mismo contenido
        read = client.get(f"/api/upload/projects/{pid}/file?path=src/app.py")
        assert read.status_code == 200
        assert read.json()["content"] == "print('hola')"

        # Y debe aparecer en el árbol
        tree = client.get(f"/api/upload/projects/{pid}/tree").json()["tree"]
        src_dir = next(c for c in tree["children"] if c["name"] == "src")
        assert any(c["name"] == "app.py" for c in src_dir["children"])

    def test_create_file_empty_content_allowed(self):
        pid = client.post("/api/upload/empty").json()["project_id"]
        res = client.post(
            "/api/upload/projects/{}/file".format(pid),
            data={"path": "notes.txt"},
        )
        assert res.status_code == 200
        assert res.json()["size"] == 0

    def test_create_file_duplicate_returns_409(self):
        pid = client.post("/api/upload/empty").json()["project_id"]
        client.post("/api/upload/projects/{}/file".format(pid), data={"path": "a.py", "content": "1"})
        res = client.post("/api/upload/projects/{}/file".format(pid), data={"path": "a.py", "content": "2"})
        assert res.status_code == 409

    def test_create_file_nonexistent_project_returns_404(self):
        res = client.post(
            "/api/upload/projects/nonexistent-id-123/file",
            data={"path": "a.py", "content": "x"},
        )
        assert res.status_code == 404

    def test_create_file_path_traversal_blocked(self):
        pid = client.post("/api/upload/empty").json()["project_id"]
        res = client.post(
            "/api/upload/projects/{}/file".format(pid),
            data={"path": "../../etc/passwd", "content": "x"},
        )
        assert res.status_code == 400


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

    def test_build_tree_nested_file_path_includes_parent_dir(self, tmp_path: Path):
        """Regresión: un archivo dentro de una subcarpeta debía traer "path"
        relativo a la raíz del proyecto (ej. "src/utils.py"), no solo su
        nombre ("utils.py") — bug real que rompía abrir ese archivo desde el
        árbol (el frontend usa "path" tal cual contra /file?path=...). Y
        siempre con "/" como separador (incluso en Windows) — el frontend
        hace filePath.split('/').pop() para sacar el nombre del archivo."""
        from services.project_service import build_tree

        (tmp_path / "src").mkdir()
        (tmp_path / "src" / "utils.py").write_text("def helper(): pass")
        (tmp_path / "src" / "nested").mkdir()
        (tmp_path / "src" / "nested" / "deep.py").write_text("x = 1")

        tree = build_tree(tmp_path)
        src_node = next(c for c in tree["children"] if c["name"] == "src")
        assert src_node["path"] == "src"

        utils_node = next(c for c in src_node["children"] if c["name"] == "utils.py")
        assert utils_node["path"] == "src/utils.py"

        nested_dir_node = next(c for c in src_node["children"] if c["name"] == "nested")
        assert nested_dir_node["path"] == "src/nested"

        deep_node = next(c for c in nested_dir_node["children"] if c["name"] == "deep.py")
        assert deep_node["path"] == "src/nested/deep.py"

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
