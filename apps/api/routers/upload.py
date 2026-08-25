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
    resolve_project_name,
    delete_project,
    list_projects,
    prune_old_projects,
    IGNORED_DIRS,
)
from shared import UPLOADS_DIR

router = APIRouter()
logger = logging.getLogger("sythrall.upload")

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


def _write_file_sync(dest: Path, content: bytes) -> None:
    """Escritura a disco — SIEMPRE vía run_in_threadpool en el caller, nunca
    invocada directo desde una ruta async. Antes /files y /folder llamaban a
    `dest.write_bytes()` (bloqueante) directo dentro del loop async: con
    miles de archivos (ej. una carpeta con node_modules — 27614 archivos en
    un caso real) esto bloqueaba el event loop de uvicorn el tiempo suficiente
    como para que el proxy/browser cortara la conexión a mitad de la subida
    (net::ERR_CONNECTION_RESET), sin ningún error claro para el usuario."""
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_bytes(content)


# ─── Subir archivos individuales ─────────────────────────────────────────────
@router.post("/files")
async def upload_files(
    files: list[UploadFile] = File(...),
    project_name: str = Form(default=""),
    project_id: str | None = Form(default=None),
):
    """
    Sube uno o varios archivos sueltos y los agrupa en un proyecto.

    Si viene `project_id`, se agregan los archivos a ese proyecto ya existente
    (mismo directorio, tree/info recalculados) en vez de crear uno nuevo — así
    "+ Código" del sidebar puede sumar a un proyecto activo sin duplicar la
    lógica de creación.
    """
    if not files:
        raise HTTPException(status_code=400, detail="No se enviaron archivos.")

    if project_id:
        project_dir = _safe_project_path(project_id)
        if not project_dir.exists():
            raise HTTPException(status_code=404, detail=f"Proyecto {project_id} no encontrado.")
    else:
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
        await run_in_threadpool(_write_file_sync, dest, content)

        saved.append({"name": file.filename, "size": len(content), "path": safe_name})

    tree = await run_in_threadpool(build_tree, project_dir)
    info = await run_in_threadpool(get_project_info, project_dir)
    resolved_name = resolve_project_name(project_dir, info, project_name, f"project-{project_id[:8]}")
    save_project_meta(project_dir, info)
    await run_in_threadpool(prune_old_projects, UPLOADS_DIR)

    logger.info(f"Upload files → project {project_id}: {len(saved)} OK, {len(errors)} errores")

    return {
        "project_id": project_id,
        "project_name": resolved_name,
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
    project_id: str | None = Form(default=None),
):
    """
    Sube una carpeta completa. El frontend debe enviar los archivos
    con sus rutas relativas en el filename (e.g. 'src/components/app.ts').

    Si viene `project_id`, se agrega al proyecto existente — ver docstring de
    upload_files.
    """
    if not files:
        raise HTTPException(status_code=400, detail="No se enviaron archivos.")

    if project_id:
        project_dir = _safe_project_path(project_id)
        if not project_dir.exists():
            raise HTTPException(status_code=404, detail=f"Proyecto {project_id} no encontrado.")
    else:
        project_id = str(uuid.uuid4())
        project_dir = UPLOADS_DIR / project_id
        project_dir.mkdir(parents=True, exist_ok=True)

    saved: list[dict] = []
    errors: list[dict] = []

    skipped_ignored = 0
    for file in files:
        filename = file.filename or "unknown"
        ext = Path(filename).suffix.lower()

        if ext in BLOCKED_EXTENSIONS:
            errors.append({"file": filename, "reason": f"Extensión bloqueada: {ext}"})
            continue

        # Construir ruta segura (evitar path traversal)
        parts = Path(filename).parts
        safe_parts = [p for p in parts if p not in (".", "..") and p != ""]
        if not safe_parts:
            continue

        # El picker de carpeta del browser (webkitdirectory) antepone SIEMPRE
        # el nombre de la carpeta elegida a cada archivo (ej. "myproject/
        # src/main.ts", garantizado por el propio browser — nunca varía entre
        # archivos de una misma subida). Ese primer segmento ya se usa aparte
        # como sugerencia de nombre del proyecto (`resolve_project_name` más
        # abajo) — descartarlo acá evita anidar todo un nivel de más dentro
        # de una carpeta con el mismo nombre (myproject/src/... en vez de
        # src/... directo en la raíz del proyecto). Reportado por el usuario:
        # subir una carpeta con archivos Y subcarpetas en la raíz mostraba
        # "solo carpetas" al abrir el árbol recién subido — en realidad los
        # archivos estaban ahí, pero un nivel más adentro de lo esperado,
        # detrás de esta carpeta extra que había que expandir primero (un
        # proyecto subido por ZIP no tiene este nivel de más). `len(...) > 1`
        # para no vaciar `safe_parts` en el caso límite de un solo archivo
        # sin ruta relativa (sin pasar por el picker de carpeta real).
        if len(safe_parts) > 1:
            safe_parts = safe_parts[1:]

        # Carpetas del sistema (node_modules, .git, dist, __pycache__, etc.) —
        # mismo IGNORED_DIRS que ya usa extract_zip() para ZIPs. Sin esto acá,
        # subir una carpeta de un proyecto JS normal (con node_modules
        # presente) mandaba decenas de miles de archivos innecesarios al
        # backend — un caso real reportado por el usuario: 27614 archivos,
        # 513 MB, la subida terminaba con la conexión cortada a mitad de
        # camino. No evita el costo de LEER esos archivos en el browser (eso
        # se filtra del lado del cliente, ver upload.ts), pero sí evita
        # escribirlos a disco si un caller no pasa por ese filtro.
        if any(part in IGNORED_DIRS for part in safe_parts[:-1]):
            skipped_ignored += 1
            continue

        content = await file.read()
        if len(content) > MAX_FILE_SIZE:
            errors.append({"file": filename, "reason": "Muy grande (máx 50 MB)"})
            continue

        dest = project_dir.joinpath(*safe_parts)
        await run_in_threadpool(_write_file_sync, dest, content)

        saved.append(
            {
                "name": filename,
                "size": len(content),
                "path": str(Path(*safe_parts)),
            }
        )

    tree = await run_in_threadpool(build_tree, project_dir)
    info = await run_in_threadpool(get_project_info, project_dir)
    resolved_name = resolve_project_name(
        project_dir, info, project_name, Path(files[0].filename or "").parts[0] if files else "folder"
    )
    save_project_meta(project_dir, info)
    await run_in_threadpool(prune_old_projects, UPLOADS_DIR)

    logger.info(
        f"Upload folder → project {project_id}: {len(saved)} archivos"
        + (f", {skipped_ignored} ignorados (node_modules/.git/etc.)" if skipped_ignored else "")
    )

    return {
        "project_id": project_id,
        "project_name": resolved_name,
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
    resolved_name = resolve_project_name(project_dir, info, project_name, Path(file.filename).stem)
    save_project_meta(project_dir, info)
    await run_in_threadpool(prune_old_projects, UPLOADS_DIR)

    logger.info(f"Upload ZIP → project {project_id}: {result['extracted']} archivos extraídos")

    return {
        "project_id": project_id,
        "project_name": resolved_name,
        "type": "zip",
        "original_zip": file.filename,
        "extracted": result["extracted"],
        "skipped": result["skipped"],
        "errors": result["errors"],
        "tree": tree,
        "info": info,
        # A diferencia de /files y /folder, esta respuesta no traía "total_files"
        # (solo dentro de "info") — el frontend lo lee del nivel superior
        # (UploadResult.total_files, requerido) para el header del explorador y
        # el toast de confirmación, mostrando "undefined archivos" para todo
        # proyecto creado por ZIP hasta este fix.
        "total_files": info["total_files"],
    }


# ─── Crear proyecto vacío (sin subir nada) ────────────────────────────────────
@router.post("/empty")
async def create_empty_project(project_name: str = Form(default="")):
    """
    Crea un proyecto sin ningún archivo — para trabajar escribiendo código
    desde cero en vez de partir de algo ya existente (pedido explícito del
    usuario: "crear proyectos sin subir carpetas o nada, para poder trabajar
    ahí codificando o creando nuevos archivos"). Los archivos se agregan
    después con POST /projects/{id}/file (el mismo "+ Nuevo archivo" del
    explorador).
    """
    project_id = str(uuid.uuid4())
    project_dir = UPLOADS_DIR / project_id
    project_dir.mkdir(parents=True, exist_ok=True)

    tree = await run_in_threadpool(build_tree, project_dir)
    info = await run_in_threadpool(get_project_info, project_dir)
    resolved_name = resolve_project_name(project_dir, info, project_name, "proyecto-vacío")
    save_project_meta(project_dir, info)
    await run_in_threadpool(prune_old_projects, UPLOADS_DIR)

    logger.info(f"Proyecto vacío creado: {project_id} ({resolved_name})")

    return {
        "project_id": project_id,
        "project_name": resolved_name,
        "type": "empty",
        "saved": [],
        "errors": [],
        "tree": tree,
        "total_files": 0,
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


# ─── Crear un archivo nuevo dentro de un proyecto ("+ Nuevo archivo") ────────
@router.post("/projects/{project_id}/file")
async def create_project_file(project_id: str, path: str = Form(...), content: str = Form(default="")):
    """
    Crea un archivo nuevo (vacío o con contenido inicial) dentro de un
    proyecto ya existente — contraparte de escritura del GET de arriba.
    Necesario para poder codificar desde cero en un proyecto (subido vacío
    con POST /empty, o cualquier otro) en vez de depender siempre de subir
    algo ya escrito.
    """
    project_dir = _safe_project_path(project_id)
    if not project_dir.exists():
        raise HTTPException(status_code=404, detail="Proyecto no encontrado.")

    # A diferencia de /folder (que filtra ".." en silencio de una lista de
    # archivos ya generada por el propio browser), acá el path lo escribe el
    # usuario a mano en un campo de texto — más directo, se rechaza en vez de
    # reinterpretarlo en silencio hacia otro lado dentro del proyecto.
    parts = Path(path).parts
    if any(p in (".", "..") for p in parts):
        raise HTTPException(status_code=400, detail="Path inválido.")
    safe_parts = [p for p in parts if p != ""]
    if not safe_parts:
        raise HTTPException(status_code=400, detail="Path inválido.")

    dest = project_dir.joinpath(*safe_parts)
    if not str(dest.resolve()).startswith(str(project_dir.resolve())):
        raise HTTPException(status_code=400, detail="Path inválido.")
    if dest.exists():
        raise HTTPException(status_code=409, detail="Ya existe un archivo con ese nombre.")

    await run_in_threadpool(_write_file_sync, dest, content.encode("utf-8"))

    info = await run_in_threadpool(get_project_info, project_dir)
    save_project_meta(project_dir, info)

    resolved_path = Path(*safe_parts).as_posix()
    logger.info(f"Archivo nuevo en proyecto {project_id}: {resolved_path}")

    return {"path": resolved_path, "size": len(content.encode("utf-8"))}


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
