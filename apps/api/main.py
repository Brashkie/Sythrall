"""
Sythrall — Backend FastAPI v4.1
Migración completa de Flask → FastAPI. Hepein Oficial
"""

import os
import sys
from contextlib import asynccontextmanager
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from shared import add_log, now, LIB_FLAGS

# ── Imports condicionales ────────────────────────────────────────────────────
try:
    import flake8  # noqa: F401

    LIB_FLAGS["HAS_FLAKE8"] = True
except ImportError:
    pass
try:
    import pylint  # noqa: F401

    LIB_FLAGS["HAS_PYLINT"] = True
except ImportError:
    pass
try:
    from radon.complexity import cc_visit, cc_rank  # noqa: F401
    from radon.metrics import mi_visit  # noqa: F401
    from radon.raw import analyze as radon_raw  # noqa: F401

    LIB_FLAGS["HAS_RADON"] = True
except ImportError:
    pass
try:
    import numpy as np

    LIB_FLAGS["HAS_NUMPY"] = True
except ImportError:
    pass
try:
    import pandas as pd

    LIB_FLAGS["HAS_PANDAS"] = True
except ImportError:
    pass
try:
    import sklearn

    LIB_FLAGS["HAS_SKLEARN"] = True
except ImportError:
    pass
try:
    import torch

    LIB_FLAGS["HAS_TORCH"] = True
except ImportError:
    pass
try:
    import tensorflow as tf

    LIB_FLAGS["HAS_TF"] = True
except ImportError:
    pass
try:
    import scipy

    LIB_FLAGS["HAS_SCIPY"] = True
except ImportError:
    pass
try:
    import cv2

    LIB_FLAGS["HAS_CV2"] = True
except ImportError:
    pass
try:
    import plotly

    LIB_FLAGS["HAS_PLOTLY"] = True
except ImportError:
    pass
try:
    import polars as pl

    LIB_FLAGS["HAS_POLARS"] = True
except ImportError:
    pass
try:
    import lightgbm as lgb

    LIB_FLAGS["HAS_LGB"] = True
except ImportError:
    pass
try:
    import spacy

    LIB_FLAGS["HAS_SPACY"] = True
except ImportError:
    pass
try:
    import icecream

    LIB_FLAGS["HAS_ICECREAM"] = True
except ImportError:
    pass
try:
    import Cython

    LIB_FLAGS["HAS_CYTHON"] = True
except ImportError:
    pass

# Shorthand booleans para /health y /capabilities
HAS_FLAKE8 = LIB_FLAGS["HAS_FLAKE8"]
HAS_PYLINT = LIB_FLAGS["HAS_PYLINT"]
HAS_RADON = LIB_FLAGS["HAS_RADON"]
HAS_NUMPY = LIB_FLAGS["HAS_NUMPY"]
HAS_PANDAS = LIB_FLAGS["HAS_PANDAS"]
HAS_SKLEARN = LIB_FLAGS["HAS_SKLEARN"]
HAS_TORCH = LIB_FLAGS["HAS_TORCH"]
HAS_TF = LIB_FLAGS["HAS_TF"]
HAS_SCIPY = LIB_FLAGS["HAS_SCIPY"]
HAS_CV2 = LIB_FLAGS["HAS_CV2"]
HAS_PLOTLY = LIB_FLAGS["HAS_PLOTLY"]
HAS_POLARS = LIB_FLAGS["HAS_POLARS"]
HAS_LGB = LIB_FLAGS["HAS_LGB"]
HAS_SPACY = LIB_FLAGS["HAS_SPACY"]
HAS_ICECREAM = LIB_FLAGS["HAS_ICECREAM"]
HAS_CYTHON = LIB_FLAGS["HAS_CYTHON"]


@asynccontextmanager
async def lifespan(app: FastAPI):
    os.makedirs("uploads/projects", exist_ok=True)
    add_log("info", "🚀 Sythrall v4.5 — FastAPI")
    add_log(
        "info",
        f"   flake8={'✓' if HAS_FLAKE8 else '✗'}  pylint={'✓' if HAS_PYLINT else '✗'}  radon={'✓' if HAS_RADON else '✗'}",
    )
    add_log(
        "info",
        f"   numpy={'✓' if HAS_NUMPY else '✗'}  pandas={'✓' if HAS_PANDAS else '✗'}  sklearn={'✓' if HAS_SKLEARN else '✗'}",
    )
    add_log("info", f"   pytorch={'✓' if HAS_TORCH else '✗'}  tensorflow={'✓' if HAS_TF else '✗'}")
    add_log(
        "info",
        f"   polars={'✓' if HAS_POLARS else '✗'}  opencv={'✓' if HAS_CV2 else '✗'}  lightgbm={'✓' if HAS_LGB else '✗'}",
    )
    add_log(
        "info",
        f"   scipy={'✓' if HAS_SCIPY else '✗'}  spacy={'✓' if HAS_SPACY else '✗'}  cython={'✓' if HAS_CYTHON else '✗'}",
    )
    yield
    add_log("info", "🛑 Sythrall detenido.")


app = FastAPI(title="Sythrall", version="4.6.0", lifespan=lifespan)
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

# ── Routers ──────────────────────────────────────────────────────────────────
from routers.upload import router as upload_router
from routers.analysis import router as analysis_router
from routers.ml import router as ml_router
from routers.diagram import router as diagram_router
from routers.logs import router as logs_router
from routers.static_analysis import router as static_router
from routers.intelligence import router as intel_router
from routers.graph import router as graph_router
from routers.metrics_live import router as metrics_router

app.include_router(upload_router, prefix="/api/upload", tags=["📂 Upload"])
app.include_router(analysis_router, prefix="/analyze", tags=["🔍 Analysis"])
app.include_router(ml_router, prefix="/analyze", tags=["🤖 ML"])
app.include_router(diagram_router, prefix="/analyze", tags=["📐 Diagram"])
app.include_router(logs_router, tags=["📋 Logs"])
app.include_router(static_router, prefix="/static", tags=["🔬 Static"])
app.include_router(intel_router, prefix="/intel", tags=["🧠 Intelligence"])
app.include_router(graph_router, prefix="/analyze", tags=["🕸 Graph"])
app.include_router(metrics_router, prefix="/metrics", tags=["📊 Live Metrics"])


# ── System endpoints ─────────────────────────────────────────────────────────


@app.get("/health", tags=["⚙️ System"])
async def health():
    return {
        "status": "ok",
        "ts": now(),
        "capabilities": {
            "flake8": HAS_FLAKE8,
            "pylint": HAS_PYLINT,
            "radon": HAS_RADON,
            "numpy": HAS_NUMPY,
            "pandas": HAS_PANDAS,
            "sklearn": HAS_SKLEARN,
            "torch": HAS_TORCH,
            "tensorflow": HAS_TF,
            "cython": HAS_CYTHON,
        },
    }


@app.get("/capabilities", tags=["⚙️ System"])
async def capabilities():
    import subprocess

    caps: dict = {
        "python": sys.version,
        "server": "Sythrall v4.5",
        "ts": now(),
        **{k: v for k, v in LIB_FLAGS.items()},
    }
    for tool, flag, cmd in [
        ("flake8_version", HAS_FLAKE8, [sys.executable, "-m", "flake8", "--version"]),
        ("pylint_version", HAS_PYLINT, [sys.executable, "-m", "pylint", "--version"]),
    ]:
        if flag:
            try:
                caps[tool] = subprocess.run(cmd, capture_output=True, text=True).stdout.strip().splitlines()[0]
            except Exception:
                pass
    for lib, flag, fn in [
        ("numpy_version", HAS_NUMPY, lambda: np.__version__),
        ("pandas_version", HAS_PANDAS, lambda: pd.__version__),
        ("sklearn_version", HAS_SKLEARN, lambda: sklearn.__version__),
        ("torch_version", HAS_TORCH, lambda: torch.__version__),
        ("tf_version", HAS_TF, lambda: tf.__version__),
        ("scipy_version", HAS_SCIPY, lambda: scipy.__version__),
        ("opencv_version", HAS_CV2, lambda: cv2.__version__),
        ("plotly_version", HAS_PLOTLY, lambda: plotly.__version__),
        ("icecream_version", HAS_ICECREAM, lambda: icecream.__version__),
        ("polars_version", HAS_POLARS, lambda: pl.__version__),
        ("lightgbm_version", HAS_LGB, lambda: lgb.__version__),
        ("spacy_version", HAS_SPACY, lambda: spacy.__version__),
        ("cython_version", HAS_CYTHON, lambda: Cython.__version__),
    ]:
        if flag:
            try:
                caps[lib] = fn()
            except Exception:
                pass
    return caps
