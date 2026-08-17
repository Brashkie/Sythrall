"""
complexity_client.py — Cliente HTTP del sidecar Rust `complexity-engine`
(apps/complexity), que reemplaza a `radon` (complejidad ciclomática,
Maintainability Index, métricas raw de líneas).

Mismo espíritu de degradación con gracia que ya existía para flake8/pylint/
radon vía LIB_FLAGS: si el sidecar no está corriendo, se devuelve el shape
vacío por default en vez de propagar la excepción — el caller no necesita
saber si el sidecar está arriba o no.
"""

import os

import httpx

COMPLEXITY_ENGINE_URL = os.environ.get("COMPLEXITY_ENGINE_URL", "http://127.0.0.1:7682")

_EMPTY_RESULT = {"functions": [], "mi": None, "raw": {}, "error": None}


async def analyze_complexity(filename: str, content: str) -> dict:
    """POSTea el archivo al sidecar y devuelve su respuesta tal cual (shape:
    functions/mi/raw/error). Ante cualquier falla de red devuelve el shape
    vacío — el sidecar es una capacidad opcional, no una dependencia dura."""
    try:
        async with httpx.AsyncClient(timeout=5.0) as client:
            resp = await client.post(
                f"{COMPLEXITY_ENGINE_URL}/metrics/complexity",
                json={"filename": filename, "content": content},
            )
            resp.raise_for_status()
            return resp.json()
    except (httpx.HTTPError, ValueError):
        return dict(_EMPTY_RESULT)


def check_complexity_engine_sync() -> bool:
    """Chequeo de arranque (síncrono, timeout corto) — mismo lugar donde antes
    se hacía `import radon` envuelto en try/except."""
    try:
        resp = httpx.get(f"{COMPLEXITY_ENGINE_URL}/health", timeout=0.5)
        return resp.status_code == 200
    except httpx.HTTPError:
        return False


async def check_complexity_engine() -> bool:
    """Versión async del chequeo de salud, para endpoints que ya corren en un
    handler async (evita bloquear el event loop con la llamada síncrona)."""
    try:
        async with httpx.AsyncClient(timeout=0.5) as client:
            resp = await client.get(f"{COMPLEXITY_ENGINE_URL}/health")
            return resp.status_code == 200
    except httpx.HTTPError:
        return False
