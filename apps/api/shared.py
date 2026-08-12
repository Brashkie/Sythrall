"""
shared.py — Estado global y helpers compartidos entre main y todos los routers.
Evita el circular import: routers importan de shared, no de main.
"""

import os
import sys
import tempfile
from datetime import datetime

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
    "HAS_RADON": False,
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

# Versiones (llenadas en main.py al arrancar)
LIB_VERSIONS: dict[str, str | None] = {}


# ── Helpers ───────────────────────────────────────────────────────────────────


def now() -> str:
    return datetime.now().strftime("%Y-%m-%d %H:%M:%S")


def add_log(level: str, msg: str) -> None:
    entry = {"ts": now(), "level": level, "msg": msg}
    LOG_HISTORY.append(entry)
    if len(LOG_HISTORY) > 200:
        LOG_HISTORY.pop(0)
    print(f"[{level.upper()}] {msg}")


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
