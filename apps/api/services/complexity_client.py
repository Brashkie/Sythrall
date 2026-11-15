"""
complexity_client.py — Cliente HTTP del sidecar Rust `complexity-engine`
(services/complexity), que reemplaza a `radon` (complejidad ciclomática,
Maintainability Index, métricas raw de líneas).

Mismo espíritu de degradación con gracia que ya existía para flake8/pylint/
radon vía LIB_FLAGS: si el sidecar no está corriendo, se devuelve el shape
vacío por default en vez de propagar la excepción — el caller no necesita
saber si el sidecar está arriba o no.
"""

import asyncio
import os

import httpx

COMPLEXITY_ENGINE_URL = os.environ.get("COMPLEXITY_ENGINE_URL", "http://127.0.0.1:7682")

# ─── Cliente compartido ──────────────────────────────────────────────────────
#
# **Medido: crear un `AsyncClient` por llamada costaba 166 ms; reutilizarlo,
# 1,3 ms — 123x.** El análisis en Rust de un archivo tarda 4,5 ms, así que el
# 98% del tiempo de una request se iba en abrir y cerrar la conexión, no en
# analizar. Con 500 archivos eran 83 s contra ~5 s.
#
# El motivo es que un `AsyncClient` nuevo estrena su pool: cada llamada pagaba
# un handshake TCP completo contra el sidecar, y encima dejaba el socket en
# TIME_WAIT. Reutilizarlo mantiene la conexión viva (keep-alive), que es para
# lo que httpx tiene un pool.
#
# **Se guarda por event loop, no en una global pelada.** Un `AsyncClient` se
# ata al loop en el que se creó; pytest crea y destruye un loop por test, y
# `uvicorn --reload` levanta uno nuevo en cada recarga. Una global sola daría
# "Event loop is closed" en la segunda vuelta. Con la clave por loop, cada
# contexto tiene el suyo y el de un loop muerto simplemente deja de usarse.
_clients: dict[object, httpx.AsyncClient] = {}


def _get_client() -> httpx.AsyncClient:
    loop = asyncio.get_running_loop()
    # Purgar los loops muertos: sin esto el dict crece una entrada por cada
    # recarga de uvicorn y por cada test de pytest.
    for viejo in [x for x in _clients if x.is_closed()]:
        _clients.pop(viejo, None)
    cli = _clients.get(loop)
    if cli is None or cli.is_closed:
        # `limits` explícito: sin esto el default de httpx (100 conexiones)
        # puede abrir más sockets contra el sidecar que los que éste atiende
        # con provecho, y el análisis de un proyecto grande dispara decenas de
        # llamadas concurrentes vía `asyncio.gather`.
        cli = httpx.AsyncClient(
            limits=httpx.Limits(max_keepalive_connections=32, max_connections=64),
        )
        _clients[loop] = cli
    return cli


async def close_clients() -> None:
    """Cierra los clientes vivos. La llama el `lifespan` de `main.py` al
    apagar; sin esto httpx avisa por conexiones sin cerrar al terminar.

    **Cerrar un cliente atado a OTRO loop tira `RuntimeError`**, y eso no
    puede hacer fallar el apagado: el cliente de un loop ya muerto no tiene
    nada que cerrar —sus sockets se fueron con el loop—, así que se descarta
    la entrada y se sigue. Lo encontró su propio test.
    """
    for loop, cli in list(_clients.items()):
        try:
            if not cli.is_closed:
                await cli.aclose()
        except RuntimeError:
            pass
        _clients.pop(loop, None)
    _clients.clear()


_EMPTY_RESULT = {"functions": [], "mi": None, "halstead": None, "raw": {}, "error": None}


async def post_to_sidecar(path: str, payload, timeout: float = 5.0) -> dict | list | None:
    """POST genérico al sidecar Rust — toda la lógica de red (cliente
    compartido con keep-alive, degradación con gracia) vive UNA vez acá en vez de copiada
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
        # El timeout va por request, no por cliente: el cliente es compartido
        # y cada endpoint conserva el suyo.
        resp = await _get_client().post(f"{COMPLEXITY_ENGINE_URL}{path}", json=payload, timeout=timeout)
        resp.raise_for_status()
        data = resp.json()
        return None if isinstance(data, dict) and data.get("error") else data
    except (httpx.HTTPError, ValueError, RuntimeError):
        # `RuntimeError` cubre el loop cerrado bajo el cliente entre medio —
        # misma degradacion con gracia que el resto: `None`, y el caller decide.
        return None


async def parse_executable_rust(content: bytes) -> dict | None:
    """Fase 19 (Machine Intelligence) — segundo bullet, cierra la fase:
    analizador de ejecutables reales (PE/ELF/Mach-O/COFF/wasm,
    `services/complexity/src/binparse.rs`, crate `object`). NO reusa
    `post_to_sidecar` — ese helper siempre manda `json=payload`, pero acá el
    payload es el binario crudo (`Content-Type: application/octet-stream`),
    no JSON. Timeout más largo que el default (15s, no 5s) porque un
    ejecutable real puede pesar varios MB, más que cualquier otro payload
    que este cliente envía hoy."""
    try:
        resp = await _get_client().post(
            f"{COMPLEXITY_ENGINE_URL}/parse/executable",
            content=content,
            headers={"Content-Type": "application/octet-stream"},
            timeout=15.0,
        )
        resp.raise_for_status()
        data = resp.json()
        return None if isinstance(data, dict) and data.get("error") else data
    except (httpx.HTTPError, ValueError, RuntimeError):
        return None


async def get_from_sidecar(path: str, params: dict | None = None, timeout: float = 5.0) -> dict | list | None:
    """GET genérico al sidecar Rust — hermano de `post_to_sidecar` para el
    único endpoint que usa GET en vez de POST (`GET /log`, ver
    `log_client.py::fetch_logs`). Mismo criterio de degradación."""
    try:
        resp = await _get_client().get(f"{COMPLEXITY_ENGINE_URL}{path}", params=params, timeout=timeout)
        resp.raise_for_status()
        data = resp.json()
        return None if isinstance(data, dict) and data.get("error") else data
    except (httpx.HTTPError, ValueError, RuntimeError):
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
        resp = await _get_client().post(
            f"{COMPLEXITY_ENGINE_URL}/metrics/complexity",
            json={"filename": filename, "content": content},
            timeout=5.0,
        )
        resp.raise_for_status()
        return resp.json()
    except (httpx.HTTPError, ValueError, RuntimeError):
        return dict(_EMPTY_RESULT)


async def parse_python_rich(filename: str, content: str) -> dict | None:
    """Fase 1 de la migración de `static_parser.py` a Rust: functions/classes/
    imports/call_graph/summary vía el sidecar (`POST /parse/python`), mismo
    shape que `_parse_python()` salvo dead_code (todavía no calculado en
    Rust — queda en el path Python; `wasm_hints` sí viaja acá desde
    `wasm.rs`)."""
    return await post_to_sidecar("/parse/python", {"filename": filename, "content": content})


async def compilar_asm_rust(
    language: str, content: str, opt_levels: list[str], features: list[str] | None = None
) -> list | None:
    """El puente fuente -> codigo maquina (`compilar.rs`, `POST /compile/asm`).

    Invoca el compilador del lenguaje en modo "emitir ensamblador" y le pasa
    el resultado a `asmparse.rs`. Timeout mas largo que el default: compilar
    de verdad tarda, y un `rustc -O2` sobre algo no trivial pasa facil de 5s.

    Relevo puro — toda la validacion (lenguaje soportado, nivel de
    optimizacion contra una lista fija) vive en Rust, que es donde tiene que
    estar: aca no se decide nada.
    """
    return await post_to_sidecar(
        "/compile/asm",
        {
            "language": language,
            "content": content,
            "opt_levels": opt_levels,
            "features": features or [],
        },
        timeout=120.0,
    )


async def comparar_metaprogramacion_rust() -> dict | None:
    """Fase 34, Linea C: mide la curva de costo de metaprogramacion en C++ y
    en D y las compara (`dcost.rs`, `POST /execution/metaprogramming-cost`).

    Timeout largo: son ocho compilaciones reales, cuatro por lenguaje.
    """
    return await post_to_sidecar("/execution/metaprogramming-cost", {}, timeout=300.0)


async def salud_scores_rust(entrada: dict) -> dict | None:
    """Los cuatro scores de Project Health (`salud.rs`, `POST /health/scores`).

    Eran las ultimas formulas de AUTORIDAD que vivian en Python. Aca ya no se
    calcula nada: se juntan los conteos (que esta capa ya tiene, porque agrega
    los resultados por archivo) y se los manda. Verificado con 4000 entradas
    aleatorias contra la implementacion Python que reemplaza: 0 diferencias.
    """
    return await post_to_sidecar("/health/scores", entrada)


async def css_atomico_rust(filename: str, content: str) -> dict | None:
    """Atomic CSS (`cssatomico.rs`, `POST /analyze/css-atomic`): genera la hoja
    atomica y mide que se gana y que se paga. Relevo puro — no reescribe nada,
    devuelve la hoja y el mapa para leer y comparar.
    """
    return await post_to_sidecar("/analyze/css-atomic", {"filename": filename, "content": content})


async def pareto_compilacion_rust(language: str, content: str, configs: list[dict] | None = None) -> dict | None:
    """Busqueda multi-objetivo sobre la CONFIGURACION de compilacion
    (`compilar.rs::buscar_pareto`, `POST /compile/pareto`).

    Timeout largo: cada punto del frente es una invocacion real del
    compilador, asi que el costo crece con el tamanio de la grilla. Relevo
    puro — la grilla por defecto y la relacion de dominancia viven en Rust.
    """
    return await post_to_sidecar(
        "/compile/pareto",
        {"language": language, "content": content, "configs": configs or []},
        timeout=300.0,
    )


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


async def parse_css_rust(filename: str, content: str) -> dict | None:
    """CSS, via el sidecar (`POST /parse/css`,
    `services/complexity/src/cssparse.rs`) — Sythrall CSS Engine. Como
    Fortran/Assembly/Notebook, no reemplaza ningun parser Python previo: CSS
    no se analizaba en absoluto antes de esto."""
    return await post_to_sidecar("/parse/css", {"filename": filename, "content": content})


async def parse_manifest_rust(filename: str, content: str) -> dict | None:
    """Manifests de dependencias, via el sidecar (`POST /parse/manifest`,
    `services/complexity/src/manifest.rs`). A diferencia de los parsers de
    lenguaje, este SI usa el `filename` del lado Rust: elige el parser por
    nombre de archivo, no por extension."""
    return await post_to_sidecar("/parse/manifest", {"filename": filename, "content": content})


async def css_usage_rust(project_dir: str, extensions: list[str], ignored_dirs: list[str]) -> dict | None:
    """Referencias cruzadas CSS <-> marcado/scripts de un proyecto entero
    (`POST /analyze/css-usage`, `services/complexity/src/cssusage.rs`).
    Recibe el mismo shape que `/scan/project` porque necesita leer el arbol
    completo: cruzar archivos ES el analisis. Timeout mas alto por el mismo
    motivo que el scanner — lee y recorre todo el proyecto."""
    return await post_to_sidecar(
        "/analyze/css-usage",
        {"project_dir": project_dir, "extensions": extensions, "ignored_dirs": ignored_dirs},
        timeout=30.0,
    )


async def parse_html_rust(filename: str, content: str) -> dict | None:
    """HTML, via el sidecar (`POST /parse/html`,
    `services/complexity/src/htmlparse.rs`) — accesibilidad y estructura del
    documento. Como CSS, no analiza codigo: un HTML no tiene funciones."""
    return await post_to_sidecar("/parse/html", {"filename": filename, "content": content})


async def parse_fortran_rust(filename: str, content: str) -> dict | None:
    """Fortran, vía el sidecar (`POST /parse/fortran`,
    `services/complexity/src/fparse.rs`) — Fase 20 (Scientific Intelligence).
    A diferencia de C/C++/JS/TS, no reemplaza ningún parser Python previo:
    Sythrall no tenía soporte Fortran antes de esta fase."""
    return await post_to_sidecar("/parse/fortran", {"filename": filename, "content": content})


async def verificar_spark_rust(content: str) -> dict | None:
    """SPARK/GNATprove sobre un fuente Ada, vía el sidecar
    (`POST /verify/spark`, `services/complexity/src/teoria/spark.rs`) —
    Fase 34, Línea E, paso 3.

    **Es la tercera categoría de evidencia del motor**: lo que vuelve marcado
    como probado no lo probó Sythrall leyendo el código, lo cerró un probador
    externo (Alt-Ergo, Z3) resolviendo una fórmula. Y lo que vuelve sin probar
    tampoco es un hallazgo: es una obligación que el prover no pudo cerrar.

    **Timeout propio y más largo que el del resto.** Un probador no es un
    parser: puede tardar minutos donde un parseo tarda milisegundos."""
    return await post_to_sidecar("/verify/spark", {"filename": "fuente.adb", "content": content}, timeout=300.0)


async def parse_d_rust(filename: str, content: str) -> dict | None:
    """D, vía el sidecar (`POST /parse/d`,
    `services/complexity/src/lenguajes/dparse.rs`) — Fase 34, Línea C, paso 1.

    Lo que trae y ningún otro de los doce: la superficie de confianza en
    **tres** niveles (`@safe`/`@trusted`/`@system`), con el del medio nombrado
    por el lenguaje. En Rust la superficie de revisión es una convención de
    comentario (`// SAFETY:`); en D es una palabra, así que se puede enumerar
    exacta en vez de leer comentarios."""
    return await post_to_sidecar("/parse/d", {"filename": filename, "content": content})


async def parse_ada_rust(filename: str, content: str) -> dict | None:
    """Ada, vía el sidecar (`POST /parse/ada`,
    `services/complexity/src/adaparse.rs`) — Fase 34, Línea E, paso 1.

    Paso 1 de tres: Ada como lenguaje *analizado*. Los pasos 2 (código máquina
    vía GNAT) y 3 (SPARK como tercera categoría de evidencia) están bloqueados
    por el toolchain ausente, no por el diseño — y este paso no lo está porque
    la gramática parsea desde el texto, igual que los otros seis lenguajes."""
    return await post_to_sidecar("/parse/ada", {"filename": filename, "content": content})


async def parse_rust_rust(filename: str, content: str) -> dict | None:
    """Rust como TARGET de análisis, vía el sidecar (`POST /parse/rust`,
    `services/complexity/src/rsparse.rs`) — Fase 34, Línea B. Trae funciones,
    `use`s y la superficie de confianza (`unsafe` con y sin justificación),
    que es el punto de la línea: medir dónde un lenguaje NOMBRA lo que
    confía en vez de dejarlo implícito."""
    return await post_to_sidecar("/parse/rust", {"filename": filename, "content": content})


async def parse_asm_rust(filename: str, content: str) -> dict | None:
    """Assembly x86-64, vía el sidecar (`POST /parse/asm`,
    `services/complexity/src/asmparse.rs`) — Fase 19 (Machine Intelligence),
    primer bullet. Como Fortran, no reemplaza ningún parser Python previo:
    Sythrall no tenía soporte Assembly antes de esta fase. A diferencia de
    los demás `/parse/*`, `asmparse::parse` nunca devuelve `None` del lado
    Rust — acá sigue pudiendo dar `None` solo si el sidecar mismo no
    responde (`post_to_sidecar`), no por un fallo de parseo interno."""
    return await post_to_sidecar("/parse/asm", {"filename": filename, "content": content})


async def parse_notebook_rust(filename: str, content: str) -> dict | None:
    """Jupyter Notebook (`.ipynb`), vía el sidecar (`POST /parse/notebook`,
    `services/complexity/src/notebook.rs`) — Fase 27 (AI Intelligence),
    único bullet implementable hoy sin el `AIProvider`. `content` acá es el
    JSON crudo del notebook, no código Python — el sidecar extrae las
    celdas de código y corre el mismo motor que `/parse/python` sobre el
    texto concatenado. Como Fortran/Assembly, no reemplaza ningún parser
    Python previo: Sythrall no tenía soporte de notebooks antes de esta
    fase."""
    return await post_to_sidecar("/parse/notebook", {"filename": filename, "content": content})


async def get_plugin_manifests_rust() -> list[dict] | None:
    """Fase 24 (Extensibility Platform) — los 8 manifests built-in
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


async def validate_template_cost_rust() -> dict | None:
    """Fase 34, Línea A — cierra la promesa de `plantillas.rs`: ese módulo
    cuenta lo ESCRITO y dice que el costo real exige compilar; esto es ese
    "exige compilar". Vía el sidecar (`POST /execution/validate-template-cost`,
    `services/complexity/src/tplcost.rs`): compila una sonda que Sythrall
    escribe (nunca código del usuario, mismo criterio que `validate_matmul`) a
    varias profundidades de instanciación y mide cómo escala el costo en esta
    máquina. Sin payload. Timeout largo por el mismo motivo que los otros
    validadores: compilar cuatro veces toma segundos."""
    return await post_to_sidecar("/execution/validate-template-cost", {}, timeout=30.0)


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


async def validate_linreg_rust() -> dict | None:
    """Fase 26 (Algorithm Validation Engine) — octavo kernel, segunda vez en
    Fortran: regresión lineal univariada por mínimos cuadrados (`POST
    /execution/validate-linreg`, `services/complexity/src/linreg_bench.rs`)
    para validar empíricamente O(n) — la pieza 3 (y última) de "migrar de
    numpy/pandas/scikit-learn" (ver
    `routers/execution.py::validate_linreg_vs_sklearn`). Mismo timeout
    largo."""
    return await post_to_sidecar("/execution/validate-linreg", {}, timeout=30.0)


async def validate_quicksort_rust() -> dict | None:
    """Fase 26 (Algorithm Validation Engine) — noveno kernel, cuarto uso de
    Zig: quicksort recursivo con partición de Lomuto (`POST
    /execution/validate-quicksort`, `services/complexity/src/quicksort_bench.rs`)
    para validar empíricamente O(n log n) por una técnica genuinamente
    distinta a `validate_mergesort_rust` (divide y vencerás recursivo, no
    fusión iterativa) — el "recursive sorting shape" que el ROADMAP nombraba
    como pendiente. Mismo timeout largo."""
    return await post_to_sidecar("/execution/validate-quicksort", {}, timeout=30.0)


async def validate_binary_search_rust() -> dict | None:
    """Fase 26 (Algorithm Validation Engine) — décimo kernel, tercera vez en
    Assembly y el primero en validar una forma SUB-LINEAL: búsqueda binaria
    iterativa (`POST /execution/validate-binary-search`,
    `services/complexity/src/binsearch_bench.rs`) para validar
    empíricamente O(log n) — el "searching...shapes beyond BFS" que el
    ROADMAP nombraba como pendiente. A diferencia de los demás kernels, el
    eje del ajuste de exponente en la respuesta es log2(tamaño del
    arreglo), no el tamaño en sí — ver el doc del módulo Rust. Mismo
    timeout largo."""
    return await post_to_sidecar("/execution/validate-binary-search", {}, timeout=30.0)


async def validate_aggregation_rust() -> dict | None:
    """Fase 26 (Algorithm Validation Engine) — séptimo kernel, tercera vez en
    Zig: agregación por categoría (suma/promedio, `POST
    /execution/validate-aggregation`, `services/complexity/src/agg_bench.rs`)
    para validar empíricamente O(n) — la pieza 2 de "migrar de
    numpy/pandas/scikit-learn" (ver
    `routers/execution.py::validate_aggregation_vs_pandas`). Mismo timeout
    largo."""
    return await post_to_sidecar("/execution/validate-aggregation", {}, timeout=30.0)


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


async def scan_project_rust(
    project_dir: str, extensions: list[str], ignored_dirs: list[str], filenames: list[str] | None = None
) -> list[dict] | None:
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
        {
            "project_dir": project_dir,
            "extensions": extensions,
            "ignored_dirs": ignored_dirs,
            # Nombres exactos (manifests): se reclaman por nombre, no por
            # extension — incluir `.json` entero arrastraria todo JSON del
            # proyecto solo para llegar a `package.json`.
            "filenames": filenames or [],
        },
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
        resp = await _get_client().get(f"{COMPLEXITY_ENGINE_URL}/health", timeout=0.5)
        return resp.status_code == 200
    except (httpx.HTTPError, RuntimeError):
        return False
