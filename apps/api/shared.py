"""
shared.py — Estado global y helpers compartidos entre main y todos los routers.
Evita el circular import: routers importan de shared, no de main.
"""

import asyncio
import os
import sys
import tempfile
from collections.abc import Callable
from datetime import datetime
from pathlib import Path
from typing import TypeVar

T = TypeVar("T")

# ── Directorio de proyectos subidos ───────────────────────────────────────────
# Absoluto y anclado a la raíz del repo (no relativo al cwd del proceso, que en
# dev es apps/api/ — ver scripts/run-backend.mjs) y, sobre todo, AFUERA del
# árbol que uvicorn --reload vigila (ese mismo apps/api/). Antes vivía en
# apps/api/uploads/projects/: cada archivo que un upload escribía (y cada
# archivo que un delete borraba) caía dentro del propio directorio observado,
# disparando un reload del proceso a mitad de la request — el pedido en curso
# veía "socket hang up"/500 (encontrado en vivo subiendo un ZIP: la extracción
# escribe varios archivos seguidos, dispara el reload con más probabilidad que
# un upload de un solo archivo, aunque el bug afecta a upload/delete por igual).
UPLOADS_DIR = Path(__file__).resolve().parent.parent.parent / "uploads" / "projects"

# En consolas Windows con codepage no-UTF8 (cp1252, etc.), imprimir emojis
# revienta con UnicodeEncodeError y tumba el proceso. Forzar UTF-8 en stdout/
# stderr evita el crash sin depender de que el usuario configure `chcp 65001`.
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        _stream.reconfigure(encoding="utf-8", errors="replace")

# ── Estado global ─────────────────────────────────────────────────────────────
LOG_HISTORY: list[dict] = []
API_HISTORY: dict[str, list] = {}

# ── Flags de libs (se rellenan en main.py al importar) ───────────────────────
# Los routers los leen desde aquí para no volver a importar main.
LIB_FLAGS: dict[str, bool] = {
    "HAS_FLAKE8": False,
    "HAS_PYLINT": False,
    "HAS_COMPLEXITY_ENGINE": False,
    "HAS_NUMPY": False,
    "HAS_PANDAS": False,
    "HAS_SKLEARN": False,
    "HAS_TORCH": False,
    "HAS_TF": False,
    "HAS_SCIPY": False,
    "HAS_CV2": False,
    "HAS_PLOTLY": False,
    "HAS_POLARS": False,
    "HAS_LGB": False,
    "HAS_SPACY": False,
    "HAS_ICECREAM": False,
    "HAS_CYTHON": False,
}

# ── Helpers ───────────────────────────────────────────────────────────────────


def now() -> str:
    return datetime.now().strftime("%Y-%m-%d %H:%M:%S")


def add_log(level: str, msg: str) -> None:
    entry = {"ts": now(), "level": level, "msg": msg}
    LOG_HISTORY.append(entry)
    if len(LOG_HISTORY) > 200:
        LOG_HISTORY.pop(0)
    print(f"[{level.upper()}] {msg}")
    # Persistencia en el sidecar (CBOR, ver services/log_client.py) — best
    # effort, en una tarea de fondo: nunca debe agregarle latencia a este
    # caller ni romper nada si el sidecar está caído. Los 29 call sites de
    # add_log() en el proyecto no cambian, todos siguen siendo síncronos.
    from services.log_client import persist_log

    try:
        asyncio.get_running_loop().create_task(persist_log(entry))
    except RuntimeError:
        pass  # no hay event loop corriendo todavía (no debería pasar en la práctica)


def save_temp(content: str, suffix: str) -> str:
    tmp = tempfile.NamedTemporaryFile(mode="w", suffix=suffix, delete=False, encoding="utf-8")
    tmp.write(content)
    tmp.close()
    return tmp.name


def safe_remove(path: str) -> None:
    try:
        os.unlink(path)
    except Exception:
        pass


def dedup_by_key(items: list[T], key_fn: Callable[[T], object]) -> list[T]:
    """Deduplica preservando el orden de la primera aparición — el mismo
    `seen = set(); unique = []; for x in items: ... if key not in seen: ...`
    aparecía copiado 6 veces (`routers/analysis.py` ×2, `routers/intelligence.py`
    ×4), cada uno solo con una `key_fn` distinta. `key_fn` decide qué hace
    "iguales" a dos items — line+message truncado, line+column, label+kind,
    lo que el caller necesite."""
    seen: set = set()
    unique: list[T] = []
    for item in items:
        key = key_fn(item)
        if key not in seen:
            seen.add(key)
            unique.append(item)
    return unique


def _get_lib_version(lib_name: str) -> str | None:
    """Retorna la versión de una librería ML/DL instalada."""
    try:
        import importlib

        # Mapeo de nombres lógicos a módulos reales
        _MOD_MAP = {
            "numpy": "numpy",
            "pandas": "pandas",
            "torch": "torch",
            "tensorflow": "tensorflow",
            "scipy": "scipy",
            "cv2": "cv2",
            "plotly": "plotly",
            "icecream": "icecream",
            "polars": "polars",
            "lightgbm": "lightgbm",
            "spacy": "spacy",
            "sklearn": "sklearn",
            "cython": "Cython",
        }
        mod_name = _MOD_MAP.get(lib_name, lib_name)
        mod = importlib.import_module(mod_name)
        return getattr(mod, "__version__", None)
    except Exception:
        return None
