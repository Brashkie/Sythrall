"""
Router: Upload
Maneja subida de archivos individuales, carpetas y ZIPs.
"""

import io
import uuid
import zipfile
import logging
from pathlib import Path

from fastapi import APIRouter, UploadFile, File, HTTPException, Form
from starlette.concurrency import run_in_threadpool

from services.project_service import (
    build_tree,
    extract_zip,
    get_project_info,
    get_project_info_cached,
    save_project_meta,
    delete_project,
    list_projects,
    prune_old_projects,
)

router = APIRouter()
logger = logging.getLogger("codewatch.upload")

UPLOADS_DIR = Path("uploads/projects")
MAX_FILE_SIZE = 50 * 1024 * 1024  # 50 MB por archivo
MAX_ZIP_SIZE = 200 * 1024 * 1024  # 200 MB por ZIP

BLOCKED_EXTENSIONS = {".exe", ".dll", ".so", ".bin", ".sh", ".bat", ".cmd"}
ALLOWED_ROOTS = {"uploads"}  # Evitar path traversal


def _safe_project_path(project_id: str) -> Path:
    """Valida que el path del proyecto esté dentro de UPLOADS_DIR."""
    path = (UPLOADS_DIR / project_id).resolve()
    if not str(path).startswith(str(UPLOADS_DIR.resolve())):
        raise HTTPException(status_code=400, detail="Path inválido.")
    return path


# ─── Subir archivos individuales ─────────────────────────────────────────────
@router.post("/files")
async def upload_files(
    files: list[UploadFile] = File(...),
    project_name: str = Form(default=""),
):
    """
    Sube uno o varios archivos sueltos y los agrupa en un proyecto.
    """
    if not files:
        raise HTTPException(status_code=400, detail="No se enviaron archivos.")

    project_id = str(uuid.uuid4())
    project_dir = UPLOADS_DIR / project_id
    project_dir.mkdir(parents=True, exist_ok=True)

    saved: list[dict] = []
    errors: list[dict] = []

    for file in files:
        ext = Path(file.filename or "").suffix.lower()
        if ext in BLOCKED_EXTENSIONS:
            errors.append({"file": file.filename, "reason": f"Extensión no permitida: {ext}"})
            continue

        content = await file.read()
        if len(content) > MAX_FILE_SIZE:
            errors.append({"file": file.filename, "reason": "Archivo demasiado grande (máx 50 MB)"})
            continue

        # Preservar rutas relativas (e.g. src/components/app.ts)
        safe_name = Path(file.filename or "file").name  # strip path traversal
        dest = project_dir / safe_name
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(content)

        saved.append({"name": file.filename, "size": len(content), "path": safe_name})

    tree = await run_in_threadpool(build_tree, project_dir)
    info = await run_in_threadpool(get_project_info, project_dir)
    save_project_meta(project_dir, info)
    await run_in_threadpool(prune_old_projects, UPLOADS_DIR)

    logger.info(f"Upload files → project {project_id}: {len(saved)} OK, {len(errors)} errores")

    return {
        "project_id": project_id,
        "project_name": project_name or f"project-{project_id[:8]}",
        "type": "files",
        "saved": saved,
        "errors": errors,
        "tree": tree,
        "total_files": len(saved),
    }


# ─── Subir carpeta (múltiples archivos con rutas relativas) ──────────────────
@router.post("/folder")
async def upload_folder(
    files: list[UploadFile] = File(...),
    project_name: str = Form(default=""),
):
    """
    Sube una carpeta completa. El frontend debe enviar los archivos
    con sus rutas relativas en el filename (e.g. 'src/components/app.ts').
    """
    if not files:
        raise HTTPException(status_code=400, detail="No se enviaron archivos.")

    project_id = str(uuid.uuid4())
    project_dir = UPLOADS_DIR / project_id
    project_dir.mkdir(parents=True, exist_ok=True)

    saved: list[dict] = []
    errors: list[dict] = []

    for file in files:
        filename = file.filename or "unknown"
        ext = Path(filename).suffix.lower()

        if ext in BLOCKED_EXTENSIONS:
            errors.append({"file": filename, "reason": f"Extensión bloqueada: {ext}"})
            continue

        content = await file.read()
        if len(content) > MAX_FILE_SIZE:
            errors.append({"file": filename, "reason": "Muy grande (máx 50 MB)"})
            continue

        # Construir ruta segura (evitar path traversal)
        parts = Path(filename).parts
        safe_parts = [p for p in parts if p not in (".", "..") and p != ""]
        dest = project_dir.joinpath(*safe_parts)
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(content)

        saved.append(
            {
                "name": filename,
                "size": len(content),
                "path": str(Path(*safe_parts)),
            }
        )

    tree = await run_in_threadpool(build_tree, project_dir)
    info = await run_in_threadpool(get_project_info, project_dir)
    save_project_meta(project_dir, info)
    await run_in_threadpool(prune_old_projects, UPLOADS_DIR)

    logger.info(f"Upload folder → project {project_id}: {len(saved)} archivos")

    return {
        "project_id": project_id,
        "project_name": project_name or (Path(files[0].filename or "").parts[0] if files else "folder"),
        "type": "folder",
        "saved": saved,
        "errors": errors,
        "tree": tree,
        "total_files": len(saved),
    }


# ─── Subir ZIP ───────────────────────────────────────────────────────────────
@router.post("/zip")
async def upload_zip(
    file: UploadFile = File(...),
    project_name: str = Form(default=""),
):
    """
    Sube un archivo ZIP, lo descomprime y devuelve el árbol de estructura.
    """
    if not file.filename or not file.filename.lower().endswith(".zip"):
        raise HTTPException(status_code=400, detail="Solo se aceptan archivos .zip")

    content = await file.read()
    if len(content) > MAX_ZIP_SIZE:
        raise HTTPException(status_code=413, detail="ZIP demasiado grande (máx 200 MB)")

    # Validar que es un ZIP real
    if not zipfile.is_zipfile(io.BytesIO(content)):
        raise HTTPException(status_code=400, detail="El archivo no es un ZIP válido.")

    project_id = str(uuid.uuid4())
    project_dir = UPLOADS_DIR / project_id
    project_dir.mkdir(parents=True, exist_ok=True)

    # ZIPs grandes (cientos de archivos, hasta 200 MB) tardan — correrlos en
    # threadpool evita bloquear el event loop para el resto de las requests.
    result = await run_in_threadpool(extract_zip, content, project_dir)

    if result["errors"]:
        logger.warning(f"ZIP {project_id}: {len(result['errors'])} errores al extraer")

    tree = await run_in_threadpool(build_tree, project_dir)
    info = await run_in_threadpool(get_project_info, project_dir)
    save_project_meta(project_dir, info)
    await run_in_threadpool(prune_old_projects, UPLOADS_DIR)

    logger.info(f"Upload ZIP → project {project_id}: {result['extracted']} archivos extraídos")

    return {
        "project_id": project_id,
        "project_name": project_name or Path(file.filename).stem,
        "type": "zip",
        "original_zip": file.filename,
        "extracted": result["extracted"],
        "skipped": result["skipped"],
        "errors": result["errors"],
        "tree": tree,
        "info": info,
    }


# ─── Listar proyectos subidos ─────────────────────────────────────────────────
@router.get("/projects")
async def get_projects():
    """Lista todos los proyectos subidos con su metadata (desde cache — no
    recorre el disco de cada proyecto acumulado)."""
    projects = await run_in_threadpool(list_projects, UPLOADS_DIR)
    return {"projects": projects, "total": len(projects)}


# ─── Obtener árbol de un proyecto ────────────────────────────────────────────
@router.get("/projects/{project_id}/tree")
async def get_project_tree(project_id: str):
    """Devuelve el árbol de archivos de un proyecto específico."""
    project_dir = _safe_project_path(project_id)
    if not project_dir.exists():
        raise HTTPException(status_code=404, detail="Proyecto no encontrado.")

    tree = await run_in_threadpool(build_tree, project_dir)
    info = await run_in_threadpool(get_project_info_cached, project_dir)

    return {
        "project_id": project_id,
        "tree": tree,
        "info": info,
    }


# ─── Obtener contenido de un archivo ─────────────────────────────────────────
@router.get("/projects/{project_id}/file")
async def get_file_content(project_id: str, path: str):
    """Devuelve el contenido de un archivo dentro del proyecto."""
    project_dir = _safe_project_path(project_id)

    # Validar path del archivo (anti path traversal)
    file_path = (project_dir / path).resolve()
    if not str(file_path).startswith(str(project_dir.resolve())):
        raise HTTPException(status_code=400, detail="Path inválido.")

    if not file_path.exists() or not file_path.is_file():
        raise HTTPException(status_code=404, detail="Archivo no encontrado.")

    # Solo archivos de texto (máx 5 MB para lectura)
    if file_path.stat().st_size > 5 * 1024 * 1024:
        raise HTTPException(status_code=413, detail="Archivo muy grande para leer (máx 5 MB).")

    try:
        content = file_path.read_text(encoding="utf-8", errors="replace")
        return {
            "path": path,
            "content": content,
            "size": file_path.stat().st_size,
            "extension": file_path.suffix,
        }
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Error al leer el archivo: {str(e)}") from e


# ─── Eliminar proyecto ────────────────────────────────────────────────────────
@router.delete("/projects/{project_id}")
async def remove_project(project_id: str):
    """Elimina un proyecto y todos sus archivos."""
    project_dir = _safe_project_path(project_id)
    if not project_dir.exists():
        raise HTTPException(status_code=404, detail="Proyecto no encontrado.")

    await run_in_threadpool(delete_project, project_dir)
    logger.info(f"Proyecto eliminado: {project_id}")
    return {"message": f"Proyecto {project_id} eliminado correctamente."}
