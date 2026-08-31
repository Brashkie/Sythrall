"""
Service: ProjectService
Lógica de negocio para manejo de proyectos subidos.
"""

import io
import json
import zipfile
import shutil
import logging
from pathlib import Path
from datetime import datetime
from typing import Any

logger = logging.getLogger("sythrall.project_service")

# Metadata cacheada por proyecto — evita recorrer todo el disco (rglob + stat
# de cada archivo) en cada GET /projects. Se calcula una vez al subir.
META_FILENAME = ".sythrall-meta.json"

# Extensiones bloqueadas en ZIPs
BLOCKED_EXTENSIONS = {".exe", ".dll", ".so", ".bin", ".sh", ".bat", ".cmd", ".ps1"}

# Carpetas a ignorar en el árbol
IGNORED_DIRS = {
    "__pycache__",
    ".git",
    ".svn",
    "node_modules",
    ".venv",
    "venv",
    ".idea",
    ".vscode",
    "dist",
    "build",
    ".next",
    ".nuxt",
    "coverage",
    ".pytest_cache",
    ".mypy_cache",
    ".tox",
    "*.egg-info",
}

# Extensiones de código para estadísticas
CODE_EXTENSIONS = {
    ".py",
    ".ts",
    ".tsx",
    ".js",
    ".jsx",
    ".java",
    ".cpp",
    ".c",
    ".cs",
    ".go",
    ".rs",
    ".rb",
    ".php",
    ".swift",
    ".kt",
    ".scala",
    ".r",
    ".html",
    ".css",
    ".scss",
    ".sass",
    ".less",
    ".vue",
    ".svelte",
    ".json",
    ".yaml",
    ".yml",
    ".toml",
    ".ini",
    ".env",
    ".md",
    ".txt",
    ".sql",
    ".graphql",
    ".sh",
    ".bash",
    ".zsh",
    ".f",
    ".f90",
    ".f95",
    ".f03",
    ".f08",
    ".for",
    ".s",
    ".asm",
}


# ─── Árbol de archivos ────────────────────────────────────────────────────────


def build_tree(directory: Path, max_depth: int = 10, _depth: int = 0, _root: Path | None = None) -> dict:
    """
    Construye un árbol JSON de la estructura de archivos.

    Retorna:
    {
        "name": "src",
        "type": "directory",
        "path": "src",
        "children": [
            {"name": "app.ts", "type": "file", "size": 1024, "extension": ".ts", "path": "src/app.ts"},
            ...
        ]
    }
    """
    # `_root` es SIEMPRE el directorio del proyecto (el primer `directory` con
    # el que se llamó, sin importar cuántos niveles de recursión bajemos) —
    # todo "path" del árbol se calcula relativo a él. Antes se calculaba
    # relativo al padre inmediato solo en `_depth == 0` y con `base=None` (→
    # "path" = solo el nombre del archivo, sin ningún directorio) en
    # cualquier nivel más profundo: un archivo en `src/main.py` quedaba con
    # "path": "main.py" en vez de "path": "src/main.py". Bug real, encontrado
    # subiendo un ZIP con una subcarpeta y pegándole directo al endpoint con
    # curl — el frontend usa ese "path" tal cual para pedir el contenido del
    # archivo (`/file?path=...`) al clickearlo en el árbol, así que cualquier
    # archivo que no estuviera en la raíz del proyecto daba 404 al abrirlo.
    if _root is None:
        _root = directory

    # El nodo raíz es un caso especial: su "path" es su propio nombre de
    # directorio (no hay nada "relativo a sí mismo" que tenga sentido acá),
    # igual que antes. Todo lo que cuelga de él sí es relativo a `_root`.
    own_path = str(directory.relative_to(directory.parent)) if directory == _root else _relative_path(directory, _root)

    if _depth > max_depth:
        return {
            "name": directory.name,
            "type": "directory",
            "path": own_path,
            "children": [],
            "truncated": True,
        }

    node: dict[str, Any] = {
        "name": directory.name,
        "type": "directory",
        "path": own_path,
        "children": [],
    }

    try:
        entries = sorted(directory.iterdir(), key=lambda p: (p.is_file(), p.name.lower()))
    except PermissionError:
        node["error"] = "Sin permisos de lectura"
        return node

    dirs = [e for e in entries if e.is_dir() and e.name not in IGNORED_DIRS]
    files = [e for e in entries if e.is_file() and e.name != META_FILENAME]

    for subdir in dirs:
        child = build_tree(subdir, max_depth, _depth + 1, _root)
        node["children"].append(child)

    for file in files:
        stat = file.stat()
        ext = file.suffix.lower()
        node["children"].append(
            {
                "name": file.name,
                "type": "file",
                "path": _relative_path(file, _root),
                "size": stat.st_size,
                "size_fmt": _fmt_size(stat.st_size),
                "extension": ext,
                "is_code": ext in CODE_EXTENSIONS,
                "modified": datetime.fromtimestamp(stat.st_mtime).isoformat(),
            }
        )

    return node


def _relative_path(path: Path, base: Path | None) -> str:
    if base is None:
        return path.name
    try:
        # .as_posix(), no str(): en Windows str(Path(...)) usa "\" — el
        # frontend (openProjectFile) hace filePath.split('/').pop() para
        # sacar el nombre del archivo, así que un separador "\" rompía esa
        # lógica (devolvía el path entero como "nombre") además de ser
        # inconsistente si este mismo backend corre en Linux en producción.
        return path.relative_to(base).as_posix()
    except ValueError:
        return path.name


def _fmt_size(size: int) -> str:
    for unit in ("B", "KB", "MB", "GB"):
        if size < 1024:
            return f"{size:.1f} {unit}"
        size /= 1024  # type: ignore
    return f"{size:.1f} TB"


# ─── Leer archivos de un proyecto ya subido ───────────────────────────────────

# Extensiones que static_parser.py realmente sabe parsear (Python/C/C++/JS/TS).
# Mismo set que usaba generate_project_graph antes de factorizarse acá — no
# tiene sentido correr análisis/lint sobre .json/.md/etc. que parse_file
# igual descarta como _unsupported.
PARSEABLE_EXTENSIONS = {
    ".py",
    ".ts",
    ".tsx",
    ".js",
    ".jsx",
    ".c",
    ".cpp",
    ".h",
    ".hpp",
    ".f",
    ".f90",
    ".f95",
    ".f03",
    ".f08",
    ".for",
    ".s",
    ".asm",
}


def read_project_files(project_dir: Path, code_exts: set[str] | None = None) -> list[dict]:
    """Lee del disco todos los archivos de código de un proyecto ya subido,
    como [{"filename": "src/app.ts", "content": "..."}]. Usado por los
    endpoints que analizan un proyecto entero dado su project_id, sin que el
    frontend tenga que mandar el contenido de cada archivo."""
    exts = code_exts or PARSEABLE_EXTENSIONS
    files: list[dict] = []
    for path in project_dir.rglob("*"):
        if not path.is_file() or path.suffix.lower() not in exts:
            continue
        if any(part in IGNORED_DIRS for part in path.relative_to(project_dir).parts):
            continue
        try:
            content = path.read_text(encoding="utf-8", errors="replace")
        except Exception:
            continue
        if content.strip():
            rel_path = path.relative_to(project_dir).as_posix()
            files.append({"filename": rel_path, "content": content})
    return files


# ─── Extraer ZIP ──────────────────────────────────────────────────────────────


def extract_zip(content: bytes, dest_dir: Path) -> dict:
    """
    Descomprime un ZIP en dest_dir con validaciones de seguridad.
    Retorna estadísticas de la extracción.
    """
    extracted = 0
    skipped = 0
    errors: list[dict] = []

    with zipfile.ZipFile(io.BytesIO(content)) as zf:
        for member in zf.infolist():
            # Ignorar directorios vacíos
            if member.filename.endswith("/"):
                continue

            # Path traversal check
            member_path = Path(member.filename)
            safe_parts = [p for p in member_path.parts if p not in (".", "..") and p != ""]
            if not safe_parts:
                skipped += 1
                continue

            # Extensión bloqueada
            ext = Path(safe_parts[-1]).suffix.lower()
            if ext in BLOCKED_EXTENSIONS:
                errors.append({"file": member.filename, "reason": f"Extensión bloqueada: {ext}"})
                skipped += 1
                continue

            # Ignorar carpetas del sistema
            if any(part in IGNORED_DIRS for part in safe_parts[:-1]):
                skipped += 1
                continue

            # Tamaño descomprimido
            if member.file_size > 50 * 1024 * 1024:
                errors.append({"file": member.filename, "reason": "Archivo muy grande (>50 MB descomprimido)"})
                skipped += 1
                continue

            dest_path = dest_dir.joinpath(*safe_parts)
            dest_path.parent.mkdir(parents=True, exist_ok=True)

            try:
                with zf.open(member) as src, open(dest_path, "wb") as dst:
                    shutil.copyfileobj(src, dst)
                extracted += 1
            except Exception as e:
                errors.append({"file": member.filename, "reason": str(e)})

    return {"extracted": extracted, "skipped": skipped, "errors": errors}


# ─── Info del proyecto ────────────────────────────────────────────────────────


def get_project_info(project_dir: Path) -> dict:
    """Estadísticas generales de un proyecto (recorre el disco — costoso en
    proyectos grandes). Se llama una vez al subir; para lecturas posteriores
    usar get_project_info_cached()."""
    total_files = 0
    total_size = 0
    by_ext: dict[str, int] = {}
    code_files = 0

    for path in project_dir.rglob("*"):
        if path.is_file() and path.name != META_FILENAME:
            total_files += 1
            size = path.stat().st_size
            total_size += size
            ext = path.suffix.lower() or "(sin extensión)"
            by_ext[ext] = by_ext.get(ext, 0) + 1
            if ext in CODE_EXTENSIONS:
                code_files += 1

    # Top extensiones
    top_ext = sorted(by_ext.items(), key=lambda x: x[1], reverse=True)[:10]

    return {
        "total_files": total_files,
        "total_size": total_size,
        "total_size_fmt": _fmt_size(total_size),
        "code_files": code_files,
        "by_extension": dict(top_ext),
        "created_at": datetime.now().isoformat(),
    }


def save_project_meta(project_dir: Path, info: dict) -> None:
    """Cachea el resultado de get_project_info() en disco para no tener que
    recalcularlo (recorrer todo el árbol) cada vez que se lista el proyecto."""
    try:
        (project_dir / META_FILENAME).write_text(json.dumps(info), encoding="utf-8")
    except Exception:
        logger.warning(f"No se pudo cachear metadata de {project_dir.name}", exc_info=True)


def resolve_project_name(project_dir: Path, info: dict, requested_name: str, fallback: str) -> str:
    """
    Resuelve el nombre final del proyecto y lo escribe en `info` (que el
    caller todavía tiene que persistir con save_project_meta) para que
    list_projects() pueda mostrarlo después — antes se resolvía un nombre
    solo para la respuesta HTTP de creación y se perdía para siempre en la
    próxima carga de "Proyectos recientes", que solo tenía el project_id.

    Si el proyecto ya existía (se le están agregando archivos) y no vino un
    nombre nuevo, se preserva el que ya tenía en vez de pisarlo con el
    fallback genérico basado en el id.
    """
    preserved = None
    meta_file = project_dir / META_FILENAME
    if meta_file.exists():
        try:
            preserved = json.loads(meta_file.read_text(encoding="utf-8")).get("project_name")
        except Exception:
            pass
    resolved = requested_name or preserved or fallback
    info["project_name"] = resolved
    return resolved


def get_project_info_cached(project_dir: Path) -> dict:
    """Lee la metadata cacheada si existe; si no (proyectos subidos antes de
    este cache, o cache corrupto), la calcula y la guarda para la próxima."""
    meta_file = project_dir / META_FILENAME
    if meta_file.exists():
        try:
            return json.loads(meta_file.read_text(encoding="utf-8"))
        except Exception:
            pass
    info = get_project_info(project_dir)
    save_project_meta(project_dir, info)
    return info


# ─── Listar proyectos ─────────────────────────────────────────────────────────


def list_projects(uploads_dir: Path) -> list[dict]:
    """Lista todos los proyectos en uploads_dir. Usa la metadata cacheada de
    cada proyecto — NO recorre el árbol de archivos de cada uno (con muchos
    proyectos acumulados, eso bloqueaba el servidor entero en cada llamada)."""
    if not uploads_dir.exists():
        return []

    projects = []
    for project_dir in sorted(uploads_dir.iterdir(), key=lambda p: p.stat().st_mtime, reverse=True):
        if not project_dir.is_dir():
            continue
        info = get_project_info_cached(project_dir)
        projects.append(
            {
                "project_id": project_dir.name,
                "path": str(project_dir),
                **info,
            }
        )

    return projects


# ─── Eliminar proyecto ────────────────────────────────────────────────────────


def delete_project(project_dir: Path) -> None:
    """Elimina un proyecto y todos sus archivos."""
    shutil.rmtree(project_dir, ignore_errors=True)


# ─── Retención — no dejar que se acumulen indefinidamente ────────────────────

MAX_PROJECTS = 20


def prune_old_projects(uploads_dir: Path, keep: int = MAX_PROJECTS) -> int:
    """Borra los proyectos más viejos cuando hay más de `keep` acumulados.
    Se llama después de cada upload exitoso. Retorna cuántos se borraron."""
    if not uploads_dir.exists():
        return 0

    project_dirs = [p for p in uploads_dir.iterdir() if p.is_dir()]
    if len(project_dirs) <= keep:
        return 0

    project_dirs.sort(key=lambda p: p.stat().st_mtime, reverse=True)
    to_delete = project_dirs[keep:]
    for project_dir in to_delete:
        delete_project(project_dir)
    logger.info(f"Retención: {len(to_delete)} proyecto(s) viejo(s) eliminado(s) (límite: {keep})")
    return len(to_delete)
