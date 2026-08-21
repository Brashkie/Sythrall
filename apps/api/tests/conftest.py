"""
conftest.py — configura el path para que pytest importe correctamente el
backend, y levanta el sidecar Rust real (`complexity-engine`) para toda la
sesión de tests.

Con Rust como núcleo del análisis de `.py` (Big-O, complejidad, space,
recursión, security findings, structural/naming smells — ver
`services/static_parser.py::_parse_python`), los tests que ejercitan esas
heurísticas necesitan el sidecar real corriendo — Python ya no tiene una
reimplementación paralela contra la cual testear en su ausencia.
"""

import subprocess
import sys
import time
from pathlib import Path

import httpx

sys.path.insert(0, str(Path(__file__).parent.parent))

from services.complexity_client import COMPLEXITY_ENGINE_URL  # noqa: E402

_REPO_ROOT = Path(__file__).parent.parent.parent.parent
_DEBUG_BIN_CANDIDATES = [
    _REPO_ROOT / "target" / "debug" / "complexity-engine.exe",
    _REPO_ROOT / "target" / "debug" / "complexity-engine",
]


def _sidecar_healthy() -> bool:
    try:
        return httpx.get(f"{COMPLEXITY_ENGINE_URL}/health", timeout=0.5).status_code == 200
    except httpx.HTTPError:
        return False


def _build_binary() -> Path:
    """Siempre `cargo build` (debug, no `--release` — más rápido de
    compilar) en vez de reusar un binario que ya exista: un binario viejo
    (ej. un `target/release` compilado antes de que se agregara una
    heurística nueva en Rust) respondería con un shape desactualizado sin
    ningún error visible — cargo ya cachea la compilación incremental, así
    que si nada cambió esto es casi instantáneo."""
    subprocess.run(
        ["cargo", "build", "--bin", "complexity-engine"],
        cwd=_REPO_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    for candidate in _DEBUG_BIN_CANDIDATES:
        if candidate.exists():
            return candidate
    raise RuntimeError("cargo build no produjo el binario complexity-engine esperado")


def pytest_configure(config):
    """Session-scoped, sin fixture — corre antes de que se importe/colecte
    ningún módulo de test, así el sidecar ya está arriba cuando los routers
    (que llaman a `parse_python_rich` en el primer request) lo necesiten.

    Si ya hay un sidecar corriendo (dev del usuario, o una sesión previa que
    quedó viva), no se toca ni se mata al final — solo se levanta uno nuevo,
    y solo se lo termina, si este proceso fue quien lo arrancó.
    """
    if _sidecar_healthy():
        config._complexity_engine_proc = None
        return

    binary = _build_binary()
    proc = subprocess.Popen(
        [str(binary)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    deadline = time.monotonic() + 10.0
    while time.monotonic() < deadline:
        if _sidecar_healthy():
            config._complexity_engine_proc = proc
            return
        if proc.poll() is not None:
            raise RuntimeError(f"complexity-engine ({binary}) terminó antes de responder /health")
        time.sleep(0.2)

    proc.terminate()
    raise RuntimeError(f"complexity-engine ({binary}) no respondió /health después de 10s")


def pytest_unconfigure(config):
    proc = getattr(config, "_complexity_engine_proc", None)
    if proc is not None:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
