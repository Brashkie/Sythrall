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


async def post_to_sidecar(path: str, payload, timeout: float = 5.0) -> dict | list | None:
    """POST genérico al sidecar Rust — toda la lógica de red (cliente de
    corta vida, degradación con gracia) vive UNA vez acá en vez de copiada
    en cada función que necesita hablarle al sidecar. Cada endpoint nuevo
    (y ya van 10+) necesita, del lado Python, una línea que llame a esto —
    no otra función de ~10 líneas clonada de la anterior. Esto es lo que
    "Rust es el principal" pide también del lado Python: la lógica real
    vive en Rust, acá solo el mínimo indispensable para llamarlo. Compartido
    también por `services/log_client.py` (`persist_log`), no solo por este
    archivo — es EL punto de entrada HTTP hacia el sidecar, no "el de
    complexity_client.py nada más".

    `None` en cualquier falla de red, o si el sidecar responde con un shape
    de error (`{"error": ...}`) — el `isinstance` guard es necesario porque
    algunos endpoints devuelven un dict de éxito y otros (Architecture
    Smells) devuelven una lista plana; sin el guard, `.get("error")` sobre
    una lista rompería. El caller decide el fallback. `analyze_complexity`
    NO usa este helper — su semántica de falla es distinta (un shape vacío
    específico, no `None`), no vale la pena forzarla acá."""
    try:
        async with httpx.AsyncClient(timeout=timeout) as client:
            resp = await client.post(f"{COMPLEXITY_ENGINE_URL}{path}", json=payload)
            resp.raise_for_status()
            data = resp.json()
            return None if isinstance(data, dict) and data.get("error") else data
    except (httpx.HTTPError, ValueError):
        return None


async def get_from_sidecar(path: str, params: dict | None = None, timeout: float = 5.0) -> dict | list | None:
    """GET genérico al sidecar Rust — hermano de `post_to_sidecar` para el
    único endpoint que usa GET en vez de POST (`GET /log`, ver
    `log_client.py::fetch_logs`). Mismo criterio de degradación."""
    try:
        async with httpx.AsyncClient(timeout=timeout) as client:
            resp = await client.get(f"{COMPLEXITY_ENGINE_URL}{path}", params=params)
            resp.raise_for_status()
            data = resp.json()
            return None if isinstance(data, dict) and data.get("error") else data
    except (httpx.HTTPError, ValueError):
        return None


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
    shape que `_parse_python()` salvo dead_code (todavía no calculado en
    Rust — queda en el path Python; `wasm_hints` sí viaja acá desde
    `wasm.rs`)."""
    return await post_to_sidecar("/parse/python", {"filename": filename, "content": content})


async def parse_c_rust(filename: str, content: str) -> dict | None:
    """C, vía el sidecar (`POST /parse/c`, `services/complexity/src/cparse.rs`
    — tree-sitter, mismas gramáticas que el binding Python que reemplaza)."""
    return await post_to_sidecar("/parse/c", {"filename": filename, "content": content})


async def parse_cpp_rust(filename: str, content: str) -> dict | None:
    """C++, vía el sidecar (`POST /parse/cpp`)."""
    return await post_to_sidecar("/parse/cpp", {"filename": filename, "content": content})


async def parse_js_rust(filename: str, content: str) -> dict | None:
    """JavaScript, vía el sidecar (`POST /parse/js`,
    `services/complexity/src/jsts.rs` — mismo regex+heurística que el Python
    que reemplaza)."""
    return await post_to_sidecar("/parse/js", {"filename": filename, "content": content})


async def parse_ts_rust(filename: str, content: str) -> dict | None:
    """TypeScript, vía el sidecar (`POST /parse/ts`)."""
    return await post_to_sidecar("/parse/ts", {"filename": filename, "content": content})


async def parse_fortran_rust(filename: str, content: str) -> dict | None:
    """Fortran, vía el sidecar (`POST /parse/fortran`,
    `services/complexity/src/fparse.rs`) — Fase 20 (Scientific Intelligence).
    A diferencia de C/C++/JS/TS, no reemplaza ningún parser Python previo:
    Sythrall no tenía soporte Fortran antes de esta fase."""
    return await post_to_sidecar("/parse/fortran", {"filename": filename, "content": content})


async def parse_asm_rust(filename: str, content: str) -> dict | None:
    """Assembly x86-64, vía el sidecar (`POST /parse/asm`,
    `services/complexity/src/asmparse.rs`) — Fase 19 (Machine Intelligence),
    primer bullet. Como Fortran, no reemplaza ningún parser Python previo:
    Sythrall no tenía soporte Assembly antes de esta fase. A diferencia de
    los demás `/parse/*`, `asmparse::parse` nunca devuelve `None` del lado
    Rust — acá sigue pudiendo dar `None` solo si el sidecar mismo no
    responde (`post_to_sidecar`), no por un fallo de parseo interno."""
    return await post_to_sidecar("/parse/asm", {"filename": filename, "content": content})


async def get_plugin_manifests_rust() -> list[dict] | None:
    """Fase 24 (Extensibility Platform) — los 7 manifests built-in
    (`GET /plugins/manifests`, `services/complexity/src/plugin.rs`), fuente
    de verdad que reemplaza el dict hardcodeado que antes vivía en
    `routers/static_analysis.py::/languages`. `None` cuando el sidecar no
    responde — el caller (`services/plugin_registry.py`) cae al mapeo de
    extensiones hardcodeado como respaldo, mismo criterio que el resto."""
    result = await get_from_sidecar("/plugins/manifests")
    return result if isinstance(result, list) else None


async def validate_matmul_bigo_rust() -> dict | None:
    """Fase 23 (Execution Intelligence) — validación empírica del O(n³) que
    `numerical_algorithm_note` predice por forma (Fase 20). Vía el sidecar
    (`POST /execution/validate-matmul`, `services/complexity/src/
    fortran_bench.rs`): compila y corre un kernel Fortran que Sythrall mismo
    escribe (nunca código del usuario) a varios tamaños, mide el tiempo real.
    Sin payload — el kernel y los tamaños son fijos en esta primera versión.
    Timeout más largo que el default (5s): compilar + correr 4 tamaños toma
    unos segundos, no una fracción — mismo criterio que el scan de proyecto
    completo (60s)."""
    return await post_to_sidecar("/execution/validate-matmul", {}, timeout=30.0)


async def validate_bubble_sort_rust() -> dict | None:
    """Fase 26 (Algorithm Validation Engine) — generaliza
    `validate_matmul_bigo_rust` más allá de Fortran/matmul: compila y corre
    un bubble sort escrito a mano en Zig (`POST /execution/validate-bubble-sort`,
    `services/complexity/src/zig_bench.rs`) para validar empíricamente O(n²).
    Mismo timeout largo, mismo motivo (compilar + correr 4 tamaños toma
    segundos, no una fracción)."""
    return await post_to_sidecar("/execution/validate-bubble-sort", {}, timeout=30.0)


async def validate_sum_squares_rust() -> dict | None:
    """Fase 26 (Algorithm Validation Engine) — tercer kernel de validación,
    esta vez en Assembly x86 real (`POST /execution/validate-sum-squares`,
    `services/complexity/src/asm_bench.rs`) para validar empíricamente O(n).
    Mismo timeout largo que los otros 2 — el `clock()` de resolución gruesa
    en Windows obliga a tamaños de N mucho más grandes, así que estas
    corridas tardan más que las de Fortran/Zig."""
    return await post_to_sidecar("/execution/validate-sum-squares", {}, timeout=30.0)


async def validate_graph_bfs_rust() -> dict | None:
    """Fase 26 (Algorithm Validation Engine) — cuarto kernel, segunda vez en
    Zig pero una forma algorítmica distinta a `validate_bubble_sort_rust`:
    recorrido de grafos (`POST /execution/validate-graph-bfs`,
    `services/complexity/src/bfs_bench.rs`) para validar empíricamente
    O(V+E) sobre un grafo disperso de grado fijo. Mismo timeout largo."""
    return await post_to_sidecar("/execution/validate-graph-bfs", {}, timeout=30.0)


async def validate_fibonacci_rust() -> dict | None:
    """Fase 26 (Algorithm Validation Engine) — quinto kernel, y el primero
    en validar una forma NO polinomial: profundidad de recursión (Fibonacci
    recursivo ingenuo en Fortran, `POST /execution/validate-fibonacci`,
    `services/complexity/src/fib_bench.rs`) para confirmar que crece
    exponencialmente (Θ(φⁿ)), no O(n^k). Mismo timeout largo."""
    return await post_to_sidecar("/execution/validate-fibonacci", {}, timeout=30.0)


async def validate_mergesort_rust() -> dict | None:
    """Fase 26 (Algorithm Validation Engine) — sexto kernel, y el primero en
    validar O(n log n): mergesort bottom-up iterativo en Assembly x86
    (`POST /execution/validate-mergesort`,
    `services/complexity/src/mergesort_bench.rs`). Mismo timeout largo."""
    return await post_to_sidecar("/execution/validate-mergesort", {}, timeout=30.0)


async def build_architecture_smells_rust(files_summary: list[dict]) -> list[dict] | None:
    """Fase 18, "Dependency Engine": Architecture Smells vía el sidecar
    (`POST /graph/architecture`) — pura orquestación sobre lo que Import/
    Centrality/Circular Graph ya construyen, no un payload nuevo. Devuelve
    la lista de smells directamente (no un dict de grafo con nodos/edges)."""
    return await post_to_sidecar("/graph/architecture", files_summary)


async def build_import_graph_rust(files_summary: list[dict]) -> dict | None:
    """Fase 18, primer slice del Graph Engine: Import Graph vía el sidecar
    (`POST /graph/import`). `files_summary` es el resumen ya parseado de cada
    archivo del proyecto (filename/language/functions/imports/dead_code) —
    no archivos crudos, el sidecar no parsea."""
    return await post_to_sidecar("/graph/import", files_summary)


async def build_centrality_graph_rust(files_summary: list[dict]) -> dict | None:
    """Fase 18, segunda porción del Graph Engine: Centrality vía el sidecar
    (`POST /graph/centrality`). Mismo `files_summary` que `build_import_graph_rust`."""
    return await post_to_sidecar("/graph/centrality", files_summary)


async def build_call_graph_rust(files_payload: list[dict]) -> dict | None:
    """Fase 18, tercera porción del Graph Engine: Call Graph vía el sidecar
    (`POST /graph/call`). `files_payload` es distinto del `files_summary` de
    Import/Centrality — Call Graph necesita detalle por función (name/big_o/
    complexity/line) y el `call_graph` ya calculado por archivo, no imports/
    dead_code."""
    return await post_to_sidecar("/graph/call", files_payload)


async def build_heatmap_rust(files_payload: list[dict]) -> dict | None:
    """Complexity Heatmap vía el sidecar (`POST /graph/heatmap`) — la última
    pieza del Graph Engine que quedaba Python-only. `files_payload` es su
    propio shape (name/big_o/complexity/line/loc por función), no el
    `files_summary` de Import/Centrality ni el de Call Graph."""
    return await post_to_sidecar("/graph/heatmap", files_payload)


async def detect_ml_rust(content: str) -> dict | None:
    """Detección estructurada de librerías/pipeline/modelos/métricas ML vía
    el sidecar (`POST /ml/detect`, `services/complexity/src/ml.rs`) —
    primera mitad de `routers/ml.py` en portarse a Rust. `None` cuando el
    sidecar no responde — `ml.py` cae a sus 4 detectores `_fallback`
    (idénticos al Python que existía antes de este puerto)."""
    return await post_to_sidecar("/ml/detect", {"content": content})


async def scan_project_rust(project_dir: str, extensions: list[str], ignored_dirs: list[str]) -> list[dict] | None:
    """Fase 18, "Project Scanner": escanea Y parsea un proyecto entero en una
    sola llamada (`POST /scan/project`, `services/complexity/src/scanner.rs`)
    — reemplaza el patrón anterior de N llamadas HTTP (una por archivo,
    `read_project_files` + `asyncio.gather(parse_file(...) for f in files)`)
    por 1 sola por proyecto. `project_dir` ya viene resuelto/validado por el
    caller (nunca un path crudo del cliente HTTP) — Rust no hace ninguna
    validación de seguridad nueva, solo lee un directorio de confianza.
    `extensions`/`ignored_dirs` vienen de `project_service.py` (única fuente
    de verdad de esas listas, para que no diverjan entre las dos
    implementaciones). Timeout más largo que el resto de los endpoints: acá
    hay I/O de disco + parseo de un proyecto entero, no un archivo suelto."""
    result = await post_to_sidecar(
        "/scan/project",
        {"project_dir": project_dir, "extensions": extensions, "ignored_dirs": ignored_dirs},
        timeout=60.0,
    )
    return result["files"] if result is not None else None


async def build_circular_graph_rust(files_summary: list[dict]) -> dict | None:
    """Fase 18, cuarta porción del Graph Engine: Circular Deps vía el
    sidecar (`POST /graph/circular`). Mismo `files_summary` que
    `build_import_graph_rust`/`build_centrality_graph_rust`."""
    return await post_to_sidecar("/graph/circular", files_summary)


async def find_definitions_python_rust(content: str, symbol: str) -> list[dict] | None:
    """Fase 18, "Symbol Engine": go-to-definition para Python vía el sidecar
    (`POST /symbols/definition/python`, `services/complexity/src/symbols.rs`)
    — antes 100% Python (`ast.walk` propio en `routers/intelligence.py`,
    duplicando lo que `rich.rs`/`structure.rs` ya recorren para `/parse/python`).
    Mismo alcance de siempre: por archivo, no a nivel de proyecto entero."""
    return await post_to_sidecar("/symbols/definition/python", {"content": content, "symbol": symbol})


async def find_definitions_jsts_rust(content: str, is_typescript: bool, symbol: str) -> list[dict] | None:
    """JS/TS, vía el sidecar (`POST /symbols/definition/js` o `/ts` según
    `is_typescript` — reusa `jsts::parse_js_ts`, ya calculado para `/parse/js`/
    `/parse/ts`, no una segunda pasada de parseo)."""
    path = "/symbols/definition/ts" if is_typescript else "/symbols/definition/js"
    return await post_to_sidecar(path, {"content": content, "symbol": symbol})


async def find_references_python_rust(content: str, symbol: str) -> dict | None:
    """Find-references para Python vía el sidecar (`POST /symbols/references/python`)
    — shape `{"references": [...], "definition_line": int | None}`."""
    return await post_to_sidecar("/symbols/references/python", {"content": content, "symbol": symbol})


async def find_references_jsts_rust(content: str, is_typescript: bool, symbol: str) -> dict | None:
    """JS/TS, vía el sidecar (`POST /symbols/references/js` o `/ts`)."""
    path = "/symbols/references/ts" if is_typescript else "/symbols/references/js"
    return await post_to_sidecar(path, {"content": content, "symbol": symbol})


def check_complexity_engine_sync() -> bool:
    """Chequeo de arranque (síncrono, timeout corto) — mismo lugar donde antes
    se hacía `import radon` envuelto en try/except."""
    try:
        resp = httpx.get(f"{COMPLEXITY_ENGINE_URL}/health", timeout=0.5)
        return resp.status_code == 200
    except httpx.HTTPError:
        return False


def get_plugin_manifests_sync() -> list[dict] | None:
    """Fase 24 — versión síncrona de `get_plugin_manifests_rust`, para el
    mismo momento de arranque (import de `main.py`) donde ya se llama
    `check_complexity_engine_sync()`. `None` si el sidecar no responde en
    ese instante — `plugin_registry.py` cae al mapeo hardcodeado como
    respaldo, no reintenta más tarde en esta misma corrida del proceso
    (mismo criterio de "una sola vez al boot" que `HAS_COMPLEXITY_ENGINE`)."""
    try:
        resp = httpx.get(f"{COMPLEXITY_ENGINE_URL}/plugins/manifests", timeout=2.0)
        resp.raise_for_status()
        data = resp.json()
        return data if isinstance(data, list) else None
    except (httpx.HTTPError, ValueError):
        return None


async def check_complexity_engine() -> bool:
    """Versión async del chequeo de salud, para endpoints que ya corren en un
    handler async (evita bloquear el event loop con la llamada síncrona)."""
    try:
        async with httpx.AsyncClient(timeout=0.5) as client:
            resp = await client.get(f"{COMPLEXITY_ENGINE_URL}/health")
            return resp.status_code == 200
    except httpx.HTTPError:
        return False
