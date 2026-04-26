"""
Service: ProjectService
Lógica de negocio para manejo de proyectos subidos.
"""

import os
import io
import zipfile
import shutil
import logging
from pathlib import Path
from datetime import datetime
from typing import Any

logger = logging.getLogger("codewatch.project_service")

# Extensiones bloqueadas en ZIPs
BLOCKED_EXTENSIONS = {".exe", ".dll", ".so", ".bin", ".sh", ".bat", ".cmd", ".ps1"}

# Carpetas a ignorar en el árbol
IGNORED_DIRS = {
    "__pycache__", ".git", ".svn", "node_modules", ".venv", "venv",
    ".idea", ".vscode", "dist", "build", ".next", ".nuxt", "coverage",
    ".pytest_cache", ".mypy_cache", ".tox", "*.egg-info",
}

# Extensiones de código para estadísticas
CODE_EXTENSIONS = {
    ".py", ".ts", ".tsx", ".js", ".jsx", ".java", ".cpp", ".c", ".cs",
    ".go", ".rs", ".rb", ".php", ".swift", ".kt", ".scala", ".r",
    ".html", ".css", ".scss", ".sass", ".less", ".vue", ".svelte",
    ".json", ".yaml", ".yml", ".toml", ".ini", ".env", ".md", ".txt",
    ".sql", ".graphql", ".sh", ".bash", ".zsh",
}


# ─── Árbol de archivos ────────────────────────────────────────────────────────

def build_tree(directory: Path, max_depth: int = 10, _depth: int = 0) -> dict:
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
    if _depth > max_depth:
        return {"name": directory.name, "type": "directory", "path": str(directory), "children": [], "truncated": True}

    node: dict[str, Any] = {
        "name":     directory.name,
        "type":     "directory",
        "path":     str(directory.relative_to(directory.parent) if _depth == 0 else directory),
        "children": [],
    }

    try:
        entries = sorted(directory.iterdir(), key=lambda p: (p.is_file(), p.name.lower()))
    except PermissionError:
        node["error"] = "Sin permisos de lectura"
        return node

    dirs  = [e for e in entries if e.is_dir()  and e.name not in IGNORED_DIRS]
    files = [e for e in entries if e.is_file()]

    for subdir in dirs:
        child = build_tree(subdir, max_depth, _depth + 1)
        # Reconstruir path relativo al root del proyecto
        child["path"] = _relative_path(subdir, directory if _depth == 0 else None)
        node["children"].append(child)

    for file in files:
        stat     = file.stat()
        ext      = file.suffix.lower()
        rel_path = _relative_path(file, directory if _depth == 0 else None)
        node["children"].append({
            "name":      file.name,
            "type":      "file",
            "path":      rel_path,
            "size":      stat.st_size,
            "size_fmt":  _fmt_size(stat.st_size),
            "extension": ext,
            "is_code":   ext in CODE_EXTENSIONS,
            "modified":  datetime.fromtimestamp(stat.st_mtime).isoformat(),
        })

    return node


def _relative_path(path: Path, base: Path | None) -> str:
    if base is None:
        return path.name
    try:
        return str(path.relative_to(base))
    except ValueError:
        return path.name


def _fmt_size(size: int) -> str:
    for unit in ("B", "KB", "MB", "GB"):
        if size < 1024:
            return f"{size:.1f} {unit}"
        size /= 1024  # type: ignore
    return f"{size:.1f} TB"


# ─── Extraer ZIP ──────────────────────────────────────────────────────────────

def extract_zip(content: bytes, dest_dir: Path) -> dict:
    """
    Descomprime un ZIP en dest_dir con validaciones de seguridad.
    Retorna estadísticas de la extracción.
    """
    extracted = 0
    skipped   = 0
    errors: list[dict] = []

    with zipfile.ZipFile(io.BytesIO(content)) as zf:
        # Detectar si hay un directorio raíz único (ej: my-project-main/)
        names      = zf.namelist()
        root_parts = set(Path(n).parts[0] for n in names if n)
        single_root = len(root_parts) == 1 and not any(n == list(root_parts)[0] for n in names if "/" not in n)

        for member in zf.infolist():
            # Ignorar directorios vacíos
            if member.filename.endswith("/"):
                continue

            # Path traversal check
            member_path = Path(member.filename)
            safe_parts  = [p for p in member_path.parts if p not in (".", "..") and p != ""]
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


# ─── Guardar archivos desde memoria ──────────────────────────────────────────

def save_files(files_data: list[dict], dest_dir: Path) -> list[dict]:
    """Guarda una lista de {path, content_bytes} en dest_dir."""
    saved = []
    for f in files_data:
        dest = dest_dir / f["path"]
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(f["content"])
        saved.append({"path": f["path"], "size": len(f["content"])})
    return saved


# ─── Info del proyecto ────────────────────────────────────────────────────────

def get_project_info(project_dir: Path) -> dict:
    """Estadísticas generales de un proyecto."""
    total_files = 0
    total_size  = 0
    by_ext: dict[str, int] = {}
    code_files  = 0

    for path in project_dir.rglob("*"):
        if path.is_file():
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
        "total_files":  total_files,
        "total_size":   total_size,
        "total_size_fmt": _fmt_size(total_size),
        "code_files":   code_files,
        "by_extension": dict(top_ext),
        "created_at":   datetime.now().isoformat(),
    }


# ─── Listar proyectos ─────────────────────────────────────────────────────────

def list_projects(uploads_dir: Path) -> list[dict]:
    """Lista todos los proyectos en uploads_dir."""
    if not uploads_dir.exists():
        return []

    projects = []
    for project_dir in sorted(uploads_dir.iterdir(), key=lambda p: p.stat().st_mtime, reverse=True):
        if not project_dir.is_dir():
            continue
        info = get_project_info(project_dir)
        projects.append({
            "project_id": project_dir.name,
            "path":       str(project_dir),
            **info,
        })

    return projects


# ─── Eliminar proyecto ────────────────────────────────────────────────────────

def delete_project(project_dir: Path) -> None:
    """Elimina un proyecto y todos sus archivos."""
    shutil.rmtree(project_dir, ignore_errors=True)
