"""
complexity_client.py — Cliente HTTP del sidecar Rust `complexity-engine`
(services/complexity), que reemplaza a `radon` (complejidad ciclomática,
Maintainability Index, métricas raw de líneas).

Mismo espíritu de degradación con gracia que ya existía para flake8/pylint/
radon vía LIB_FLAGS: si el sidecar no está corriendo, se devuelve el shape
vacío por default en vez de propagar la excepción — el caller no necesita
saber si el sidecar está arriba o no.
"""

import os

import httpx

COMPLEXITY_ENGINE_URL = os.environ.get("COMPLEXITY_ENGINE_URL", "http://127.0.0.1:7682")

_EMPTY_RESULT = {"functions": [], "mi": None, "halstead": None, "raw": {}, "error": None}


async def analyze_complexity(filename: str, content: str) -> dict:
    """POSTea el archivo al sidecar y devuelve su respuesta tal cual (shape:
    functions/mi/halstead/raw/error). Ante cualquier falla de red devuelve el
    shape vacío — el sidecar es una capacidad opcional, no una dependencia
    dura. `halstead` (Fase 22) es Rust-only, sin fallback Python — mismo
    límite que ya tenía `mi`/`functions`/`raw` desde la Fase 11
    (`complexity-engine` reemplazó a `radon` sin dejar un cálculo Python
    paralelo; degrada a `None`, no a una versión más lenta calculada acá)."""
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


async def parse_python_rich(filename: str, content: str) -> dict | None:
    """Fase 1 de la migración de `static_parser.py` a Rust: functions/classes/
    imports/call_graph/summary vía el sidecar (`POST /parse/python`), mismo
    shape que `_parse_python()` salvo wasm_hints/dead_code (todavía no
    calculados en Rust — quedan en el path Python). `None` en
    cualquier falla, para que el caller decida el fallback (no hay un shape
    "vacío" razonable acá como en `analyze_complexity`, porque el caller
    necesita saber si tiene que recurrir a `_parse_python()` entero)."""
    try:
        async with httpx.AsyncClient(timeout=5.0) as client:
            resp = await client.post(
                f"{COMPLEXITY_ENGINE_URL}/parse/python",
                json={"filename": filename, "content": content},
            )
            resp.raise_for_status()
            data = resp.json()
            return None if data.get("error") else data
    except (httpx.HTTPError, ValueError):
        return None


async def build_import_graph_rust(files_summary: list[dict]) -> dict | None:
    """Fase 18, primer slice del Graph Engine: Import Graph vía el sidecar
    (`POST /graph/import`). `files_summary` es el resumen ya parseado de cada
    archivo del proyecto (filename/language/functions/imports/dead_code) —
    no archivos crudos, el sidecar no parsea. `None` en cualquier falla,
    mismo criterio que `parse_python_rich`: el caller (`graph.py`) decide
    el fallback."""
    try:
        async with httpx.AsyncClient(timeout=5.0) as client:
            resp = await client.post(
                f"{COMPLEXITY_ENGINE_URL}/graph/import",
                json=files_summary,
            )
            resp.raise_for_status()
            data = resp.json()
            return None if data.get("error") else data
    except (httpx.HTTPError, ValueError):
        return None


async def build_centrality_graph_rust(files_summary: list[dict]) -> dict | None:
    """Fase 18, segunda porción del Graph Engine: Centrality vía el sidecar
    (`POST /graph/centrality`). Mismo `files_summary` que `build_import_graph_rust`
    y mismo criterio de falla (`None`, el caller decide)."""
    try:
        async with httpx.AsyncClient(timeout=5.0) as client:
            resp = await client.post(
                f"{COMPLEXITY_ENGINE_URL}/graph/centrality",
                json=files_summary,
            )
            resp.raise_for_status()
            data = resp.json()
            return None if data.get("error") else data
    except (httpx.HTTPError, ValueError):
        return None


async def build_call_graph_rust(files_payload: list[dict]) -> dict | None:
    """Fase 18, tercera porción del Graph Engine: Call Graph vía el sidecar
    (`POST /graph/call`). `files_payload` es distinto del `files_summary` de
    Import/Centrality — Call Graph necesita detalle por función (name/big_o/
    complexity/line) y el `call_graph` ya calculado por archivo, no imports/
    dead_code. Mismo criterio de falla (`None`, el caller decide)."""
    try:
        async with httpx.AsyncClient(timeout=5.0) as client:
            resp = await client.post(
                f"{COMPLEXITY_ENGINE_URL}/graph/call",
                json=files_payload,
            )
            resp.raise_for_status()
            data = resp.json()
            return None if data.get("error") else data
    except (httpx.HTTPError, ValueError):
        return None


async def build_circular_graph_rust(files_summary: list[dict]) -> dict | None:
    """Fase 18, cuarta y última porción del Graph Engine: Circular Deps vía
    el sidecar (`POST /graph/circular`). Mismo `files_summary` que
    `build_import_graph_rust`/`build_centrality_graph_rust` y mismo criterio
    de falla (`None`, el caller decide)."""
    try:
        async with httpx.AsyncClient(timeout=5.0) as client:
            resp = await client.post(
                f"{COMPLEXITY_ENGINE_URL}/graph/circular",
                json=files_summary,
            )
            resp.raise_for_status()
            data = resp.json()
            return None if data.get("error") else data
    except (httpx.HTTPError, ValueError):
        return None


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
