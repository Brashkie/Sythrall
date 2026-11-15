"""
Router: Execution Intelligence
Fase 23 del ROADMAP — a diferencia de todo lo demás en apps/api/routers
(análisis estático puro: parseo de texto, nunca ejecución), este router
dispara cómputo real vía el sidecar Rust: compila y corre kernels que
Sythrall mismo escribe (nunca código del usuario) para validar
empíricamente predicciones de Big-O. Merece su propio archivo en vez de
vivir en static_analysis.py, cuyo propio docstring dice "análisis
estático... SIN IA" — esto ejecuta código, es una categoría distinta.

Fase 26 (Algorithm Validation Engine) generalizó el primer kernel (Fortran,
matmul, O(n³)) a 9 más: Zig (bubble sort, O(n²)), Assembly x86 real (suma de
cuadrados, O(n)), Zig otra vez pero forma distinta (recorrido de grafos BFS,
O(V+E)), Fortran otra vez pero la primera forma NO polinomial (Fibonacci
recursivo ingenuo, exponencial Θ(φⁿ)), Assembly otra vez pero la primera
forma O(n log n) (mergesort bottom-up iterativo), Zig otra vez para
agregación por categoría (O(n)), Fortran otra vez para regresión lineal
(O(n)), Zig una cuarta vez para quicksort recursivo (O(n log n) por una
técnica distinta a mergesort — divide y vencerás recursivo, no fusión
iterativa), y Assembly una tercera vez para búsqueda binaria (la primera
forma SUB-LINEAL, O(log n)) — mismo patrón, mismo criterio de degradación
cada vez.

"Migrar de numpy/pandas/scikit-learn" (pedido explícito del usuario,
2026-08-31): antes de proponer reemplazar una llamada a una de esas
librerías por un kernel nativo, hay que medir HONESTAMENTE contra la
librería real, no asumir que "nativo" es automáticamente más rápido.
Pieza 1, `/validate-matmul-vs-numpy`: kernel Fortran vs. numpy real, mismos
tamaños y mismos datos — numpy gana por mucho (BLAS/LAPACK), reportado tal
cual. Pieza 2, `/validate-aggregation-vs-pandas`: kernel Zig (agregación por
categoría) vs. pandas real `groupby().agg()` — acá gana el kernel nativo por
~20×, con la salvedad explícita de que asume categorías fijas conocidas de
antemano. Pieza 3 (y última), `/validate-linreg-vs-sklearn`: kernel Fortran
(regresión lineal univariada, forma cerrada) vs. scikit-learn real — gana el
kernel nativo por ~25×, mismo tipo de salvedad que la pieza 2 (asume una
sola variable con solución cerrada conocida, sklearn no puede asumir eso).
Ninguna pieza concluye "nativo siempre gana" ni "librería siempre gana" —
cada una mide su propio caso y lo reporta tal cual sale."""

import time

from fastapi import APIRouter
from starlette.concurrency import run_in_threadpool

from services.complexity_client import (
    comparar_metaprogramacion_rust,
    verificar_spark_rust,
    validate_aggregation_rust,
    validate_binary_search_rust,
    validate_bubble_sort_rust,
    validate_fibonacci_rust,
    validate_graph_bfs_rust,
    validate_linreg_rust,
    validate_matmul_bigo_rust,
    validate_template_cost_rust,
    validate_mergesort_rust,
    validate_quicksort_rust,
    validate_sum_squares_rust,
)

try:
    import numpy as np

    _HAS_NUMPY = True
except ImportError:
    _HAS_NUMPY = False

try:
    import pandas as pd

    _HAS_PANDAS = True
except ImportError:
    _HAS_PANDAS = False

try:
    from sklearn.linear_model import LinearRegression

    _HAS_SKLEARN = True
except ImportError:
    _HAS_SKLEARN = False

router = APIRouter()

# Mismos tamaños Y mismos datos que `kernels/validate_matmul.f90`
# (A[i,j]=(i+j) mod 7, B[i,j]=(i*j) mod 5, ambos float32/REAL) — para que la
# comparación sea realmente sobre el mismo problema, no dos problemas
# distintos que casualmente comparten un nombre.
_NUMPY_MATMUL_SIZES = (300, 450, 600, 800)


def _numpy_matmul_timings() -> dict:
    if not _HAS_NUMPY:
        return {
            "available": False,
            "measurements": [],
            "note": "numpy no está instalado en este entorno.",
        }
    measurements = []
    for n in _NUMPY_MATMUL_SIZES:
        i_idx, j_idx = np.meshgrid(np.arange(1, n + 1), np.arange(1, n + 1), indexing="ij")
        a = ((i_idx + j_idx) % 7).astype(np.float32)
        b = ((i_idx * j_idx) % 5).astype(np.float32)
        best = None
        for _ in range(3):
            start = time.perf_counter()
            a @ b
            elapsed = time.perf_counter() - start
            best = elapsed if best is None else min(best, elapsed)
        measurements.append({"n": n, "seconds": best})
    return {
        "available": True,
        "measurements": measurements,
        "note": (
            "Tiempos reales de numpy (no una estimación) para los mismos tamaños y los mismos "
            "valores que el kernel Fortran de arriba usa — misma multiplicación de matrices, "
            "medida dos veces con dos implementaciones distintas."
        ),
    }


# Mismos tamaños Y mismos datos que `kernels/validate_aggregation.zig`
# (10 categorías fijas = i % 10, value = (i*37 + 11) % 1000) — igual criterio
# que `_NUMPY_MATMUL_SIZES`: comparar el mismo problema, no dos que solo
# comparten nombre.
_PANDAS_AGG_SIZES = (2_000_000, 4_000_000, 8_000_000, 16_000_000)


def _pandas_aggregation_timings() -> dict:
    if not _HAS_PANDAS:
        return {
            "available": False,
            "measurements": [],
            "note": "pandas no está instalado en este entorno.",
        }
    measurements = []
    for n in _PANDAS_AGG_SIZES:
        idx = np.arange(n) if _HAS_NUMPY else None
        if idx is None:
            # numpy es una dependencia de pandas, así que en la práctica
            # nunca falta si pandas está disponible — igual, sin asumirlo.
            return {
                "available": False,
                "measurements": [],
                "note": "numpy (dependencia de pandas) no está instalado en este entorno.",
            }
        df = pd.DataFrame({"category": idx % 10, "value": (idx * 37 + 11) % 1000})
        best = None
        for _ in range(3):
            start = time.perf_counter()
            df.groupby("category")["value"].agg(["sum", "mean"])
            elapsed = time.perf_counter() - start
            best = elapsed if best is None else min(best, elapsed)
        measurements.append({"n": n, "seconds": best})
    return {
        "available": True,
        "measurements": measurements,
        "note": (
            "Tiempos reales de pandas (no una estimación) para los mismos tamaños y los mismos "
            "valores que el kernel Zig de arriba usa — misma agregación por categoría (suma y "
            "promedio), medida dos veces con dos implementaciones distintas."
        ),
    }


# Mismos tamaños Y mismos datos que `kernels/validate_linreg.f90`
# (x_i = i, y_i = 3x_i + 7 + ruido entero chico) — igual criterio que
# `_NUMPY_MATMUL_SIZES`/`_PANDAS_AGG_SIZES`: comparar el mismo problema.
_SKLEARN_LINREG_SIZES = (40_000_000, 80_000_000, 120_000_000, 160_000_000)


def _sklearn_linreg_timings() -> dict:
    if not _HAS_SKLEARN:
        return {
            "available": False,
            "measurements": [],
            "note": "scikit-learn no está instalado en este entorno.",
        }
    if not _HAS_NUMPY:
        return {
            "available": False,
            "measurements": [],
            "note": "numpy (dependencia de scikit-learn) no está instalado en este entorno.",
        }
    measurements = []
    for n in _SKLEARN_LINREG_SIZES:
        idx = np.arange(1, n + 1, dtype=np.float64)
        noise = (idx.astype(np.int64) * 37 + 11) % 21 - 10
        x = idx.reshape(-1, 1)
        y = 3.0 * idx + 7.0 + noise
        best = None
        for _ in range(3):
            start = time.perf_counter()
            LinearRegression().fit(x, y)
            elapsed = time.perf_counter() - start
            best = elapsed if best is None else min(best, elapsed)
        measurements.append({"n": n, "seconds": best})
    return {
        "available": True,
        "measurements": measurements,
        "note": (
            "Tiempos reales de scikit-learn (no una estimación) para los mismos tamaños y los mismos "
            "valores que el kernel Fortran de arriba usa — misma regresión lineal, medida dos veces con "
            "dos implementaciones distintas."
        ),
    }


def _build_comparison_note(
    kernel_label: str,
    kernel_result: dict,
    lib_label: str,
    lib_timings: dict,
    format_success,
) -> str:
    """Arma el `comparison_note` compartido por las 3 piezas de la migración
    numpy/pandas/scikit-learn — antes copiado casi textual 3 veces (matmul-
    vs-numpy, agregación-vs-pandas, linreg-vs-sklearn): mismo chequeo de qué
    lado falta, mismo cruce de tamaños en común, solo cambiaban los nombres
    y el texto final. `format_success(n, kernel_seconds, lib_seconds)` arma
    la frase de comparación honesta de cada pieza (incluida su propia
    salvedad) — la única parte que de verdad difiere entre las 3."""
    kernel_available = bool(kernel_result.get("measurements"))
    lib_available = lib_timings["available"]

    if not lib_available and not kernel_available:
        return f"Ni {lib_label} ni el sidecar Rust ({kernel_label}) están disponibles en este entorno — no se puede comparar."
    if not lib_available:
        return f"{lib_label} no está instalado en este entorno — no se puede comparar."
    if not kernel_available:
        return f"El sidecar Rust ({kernel_label}) no está disponible en este entorno — no se puede comparar."

    kernel_by_n = {m["n"]: m["seconds"] for m in kernel_result["measurements"]}
    lib_by_n = {m["n"]: m["seconds"] for m in lib_timings["measurements"]}
    common_sizes = sorted(set(kernel_by_n) & set(lib_by_n))
    if not common_sizes:
        return "No se pudieron obtener tamaños en común entre ambas mediciones."

    n = common_sizes[-1]
    kernel_s, lib_s = kernel_by_n[n], lib_by_n[n]
    if kernel_s <= 0 or lib_s <= 0:
        return "No se pudieron obtener tamaños en común entre ambas mediciones."
    return format_success(n, kernel_s, lib_s)


def _degraded(predicted_big_o: str) -> dict:
    return {
        "available": False,
        "predicted_big_o": predicted_big_o,
        "measurements": [],
        "estimated_exponent": None,
        "note": "sidecar Rust no disponible",
    }


@router.post("/validate-matmul")
async def validate_matmul():
    """Compila y corre un kernel de multiplicación de matrices (Fortran,
    escrito por Sythrall) a varios tamaños de `n`, mide el tiempo real, y
    devuelve el exponente de crecimiento empírico junto a la predicción
    estática de O(n³) (`fparse.rs::numerical_algorithm_note`, Fase 20). Sin
    payload — el kernel y los tamaños son fijos en esta primera versión.
    Si el sidecar Rust no está disponible, degrada con gracia (mismo
    criterio que el resto del proyecto) en vez de devolver un error crudo."""
    result = await validate_matmul_bigo_rust()
    return result if result is not None else _degraded("O(n³)")


@router.post("/validate-template-cost")
async def validate_template_cost():
    """Fase 34, Línea A — compila una sonda de recursión de templates (escrita
    por Sythrall, nunca código del usuario) a varias profundidades y mide
    cuánto objeto y cuánto tiempo cuesta cada una EN ESTA MÁQUINA. Es la mitad
    medida de lo que `plantillas.rs` solo puede describir. Mismo criterio de
    degradación que el resto: sin sidecar o sin compilador C++, `available:
    false` con el motivo, nunca un error crudo."""
    result = await validate_template_cost_rust()
    return (
        result
        if result is not None
        else {
            "available": False,
            "compiler": None,
            "points": [],
            "bytes_per_level": None,
            "note": "El sidecar de análisis no está disponible; las métricas estáticas de templates no lo necesitan, solo esta mitad medida.",
        }
    )


@router.post("/spark-verify")
async def spark_verify(req: dict):
    """Fase 34, Línea E, paso 3 — corre GNATprove sobre un fuente Ada y
    devuelve sus obligaciones de prueba.

    **La regla que esta fase se puso por escrito antes de empezar**: una
    propiedad probada y una advertencia del analizador no se muestran igual, y
    no se afirma que un proyecto SPARK está verificado sin leer el resultado
    real del probador. Por eso cada obligación viene con su resultado en el
    `kind` (`spark_proved`/`spark_unproved`) y `decidibilidad.rs` la clasifica
    como `machine-proved` o `unproved` — nunca como `decidable`, que sería
    adjudicarle al motor una demostración ajena.

    Sin toolchain degrada diciendo qué falta, jamás devuelve cero obligaciones
    como si el código estuviera probado.
    """
    contenido = (req or {}).get("content") or ""
    if not contenido.strip():
        return {
            "available": False,
            "proved": 0,
            "unproved": 0,
            "obligations": [],
            "note": "No se mandó ningún fuente para verificar.",
        }
    result = await verificar_spark_rust(contenido)
    return (
        result
        if result is not None
        else {
            "available": False,
            "proved": 0,
            "unproved": 0,
            "obligations": [],
            "note": "El sidecar de análisis no está disponible: sin él no hay a quién pedirle la prueba.",
        }
    )


@router.post("/metaprogramming-cost")
async def metaprogramming_cost():
    """Fase 34, Linea C — mide la MISMA curva de costo de metaprogramacion en
    C++ y en D, a las mismas profundidades, y las compara.

    La pregunta que una sola curva no puede responder: el costo por nivel que
    midio C++, ies de la metaprogramacion o de C++? Medido, C++ emite 3
    simbolos por nivel y D emite 6 — es del lenguaje. Lo que si generaliza es
    la forma: las dos curvas son lineales y determinísticas.

    Mismo criterio de degradacion que el resto: si falta cualquiera de los dos
    compiladores, `comparable: false` y cada lado dice cual no encontro.
    """
    result = await comparar_metaprogramacion_rust()
    return (
        result
        if result is not None
        else {
            "comparable": False,
            "cpp_symbols_per_level": None,
            "d_symbols_per_level": None,
            "ambas_lineales": None,
            "note": "El sidecar de analisis no esta disponible: sin el no hay ninguna de las dos curvas.",
        }
    )


@router.post("/validate-bubble-sort")
async def validate_bubble_sort():
    """Fase 26 — compila y corre un bubble sort escrito a mano en Zig, mide
    el tiempo real, valida empíricamente O(n²). Mismo criterio de
    degradación que `validate_matmul`."""
    result = await validate_bubble_sort_rust()
    return result if result is not None else _degraded("O(n²)")


@router.post("/validate-sum-squares")
async def validate_sum_squares():
    """Fase 26 — ensambla (GNU as) y corre una suma de cuadrados escrita a
    mano en Assembly x86 real, mide el tiempo real, valida empíricamente
    O(n). Mismo criterio de degradación que `validate_matmul`."""
    result = await validate_sum_squares_rust()
    return result if result is not None else _degraded("O(n)")


@router.post("/validate-graph-bfs")
async def validate_graph_bfs():
    """Fase 26 — compila y corre un BFS sobre un grafo disperso de grado
    fijo escrito a mano en Zig, mide el tiempo real, valida empíricamente
    O(V+E). Mismo criterio de degradación que `validate_matmul`."""
    result = await validate_graph_bfs_rust()
    return result if result is not None else _degraded("O(V+E)")


@router.post("/validate-fibonacci")
async def validate_fibonacci():
    """Fase 26 — compila y corre un Fibonacci recursivo ingenuo (sin
    memoización, a propósito) en Fortran, mide el tiempo real, valida
    empíricamente que crece exponencialmente (Θ(φⁿ)), no polinomialmente.
    Mismo criterio de degradación que `validate_matmul`, salvo que acá
    `estimated_exponent` es la BASE medida del crecimiento, no un
    exponente `n^k` — ver `fib_bench.rs` para la razón estadística."""
    result = await validate_fibonacci_rust()
    return result if result is not None else _degraded("exponencial (Θ(φⁿ))")


@router.post("/validate-mergesort")
async def validate_mergesort():
    """Fase 26 — ensambla (GNU as) y corre un mergesort bottom-up iterativo
    escrito a mano en Assembly x86, mide el tiempo real, valida
    empíricamente O(n log n). Mismo criterio de degradación que
    `validate_matmul`."""
    result = await validate_mergesort_rust()
    return result if result is not None else _degraded("O(n log n)")


@router.post("/validate-aggregation")
async def validate_aggregation():
    """Fase 26 — compila y corre una agregación por categoría (suma y
    promedio) escrita a mano en Zig, mide el tiempo real, valida
    empíricamente O(n). Mismo criterio de degradación que `validate_matmul`."""
    result = await validate_aggregation_rust()
    return result if result is not None else _degraded("O(n)")


@router.post("/validate-linreg")
async def validate_linreg():
    """Fase 26 — compila y corre una regresión lineal por mínimos cuadrados
    (forma cerrada) escrita a mano en Fortran, mide el tiempo real, valida
    empíricamente O(n). Mismo criterio de degradación que `validate_matmul`."""
    result = await validate_linreg_rust()
    return result if result is not None else _degraded("O(n)")


@router.post("/validate-quicksort")
async def validate_quicksort():
    """Fase 26 — compila y corre un quicksort recursivo (partición de
    Lomuto) escrito a mano en Zig, mide el tiempo real, valida empíricamente
    O(n log n) — el "recursive sorting shape" nombrado como pendiente.
    Mismo criterio de degradación que `validate_matmul`."""
    result = await validate_quicksort_rust()
    return result if result is not None else _degraded("O(n log n)")


@router.post("/validate-binary-search")
async def validate_binary_search():
    """Fase 26 — ensambla (GNU as) y corre una búsqueda binaria iterativa
    escrita a mano en Assembly x86, mide el tiempo real, valida
    empíricamente O(log n) — la primera forma SUB-LINEAL de este motor, el
    "searching...shapes beyond BFS" nombrado como pendiente. Mismo criterio
    de degradación que `validate_matmul`, salvo que acá el eje del ajuste
    de exponente es log2(tamaño del arreglo), no el tamaño en sí — ver
    `binsearch_bench.rs` para la razón estadística."""
    result = await validate_binary_search_rust()
    return result if result is not None else _degraded("O(log n)")


@router.post("/validate-matmul-vs-numpy")
async def validate_matmul_vs_numpy():
    """Primera pieza de la migración numpy/pandas/scikit-learn que el
    usuario pidió (2026-08-31): antes de proponer un reemplazo nativo para
    una llamada a estas librerías, medir contra la librería REAL, no
    asumir. Corre el kernel Fortran de `validate_matmul` de arriba Y numpy
    real (mismos 4 tamaños, mismos valores — `A[i,j]=(i+j) mod 7`,
    `B[i,j]=(i*j) mod 5`, float32 en ambos lados) y devuelve los dos
    resultados lado a lado.

    Resultado honesto, medido en esta máquina, no adivinado: numpy gana por
    un margen enorme (≈100-200×) en cada tamaño — el kernel Fortran es un
    triple loop directo escrito para VALIDAR la forma O(n³), no para
    competir en velocidad; numpy delega en BLAS/LAPACK, típicamente
    vectorizado y multi-hilo. Esto se reporta tal cual: la conclusión de
    esta primera pieza NO es "reemplacen numpy por Fortran", es "medir
    antes de migrar" — el mismo principio que ya rige toda la Fase 26. Un
    reemplazo nativo que realmente compita necesitaría bloqueo por cachés
    (tiling), SIMD o paralelismo — trabajo real todavía no hecho, nombrado
    acá para la próxima porción, no escondido."""
    fortran_result = await validate_matmul_bigo_rust()
    fortran = fortran_result if fortran_result is not None else _degraded("O(n³)")
    # `run_in_threadpool`, no una llamada síncrona directa: este cómputo real
    # de numpy tarda segundos, y sin esto bloquea el único hilo del event
    # loop de asyncio — congelando la app ENTERA (hasta su propio health
    # check) mientras corre. Mismo bug ya corregido antes en
    # `upload.py`/`ml.py`/`diagram.py`, encontrado acá en vivo mientras se
    # verificaba la pieza 3 (scikit-learn, la más lenta) en un navegador
    # real: el badge de estado de la app pasó a "Servicio no disponible"
    # durante la comparación.
    numpy_timings = await run_in_threadpool(_numpy_matmul_timings)

    def _format_success(n: int, fortran_s: float, numpy_s: float) -> str:
        ratio = fortran_s / numpy_s
        return (
            f"En n={n}: Fortran (triple loop, escrito para validar O(n³), no para competir en velocidad) tardó "
            f"{fortran_s:.4f}s; numpy (BLAS/LAPACK real) tardó {numpy_s:.6f}s — numpy es ≈{ratio:.0f}× más rápido "
            "en esta máquina. Conclusión honesta: para multiplicación de matrices específicamente, todavía NO "
            "hay un reemplazo nativo que le gane a numpy sin bloqueo por cachés/SIMD/paralelismo real — "
            "'migrar' acá significaría escribir eso, no solo un triple loop en otro lenguaje."
        )

    comparison_note = _build_comparison_note("kernel Fortran", fortran, "numpy", numpy_timings, _format_success)
    return {"fortran": fortran, "numpy": numpy_timings, "comparison_note": comparison_note}


@router.post("/validate-aggregation-vs-pandas")
async def validate_aggregation_vs_pandas():
    """Segunda pieza de la migración numpy/pandas/scikit-learn (2026-08-31):
    corre el kernel Zig de `validate_aggregation` de arriba Y pandas real
    (mismos 4 tamaños, mismos datos — 10 categorías fijas = `i % 10`, valor =
    `(i*37 + 11) % 1000`) y devuelve los dos resultados lado a lado.

    Resultado honesto, medido en esta máquina, no adivinado: acá el kernel
    Zig gana por un margen grande (≈20×) — lo opuesto de la pieza 1
    (matmul), donde numpy ganaba. La razón no es "Zig es más rápido que
    Python" en general: es que `DataFrame.groupby(...).agg(...)` paga un
    costo real de propósito general (hashing de claves de agrupación,
    construcción de objetos intermedios) que este kernel evita por completo
    al asumir algo que pandas NO puede asumir — un número fijo y conocido de
    categorías (10) — así que basta un arreglo de acumuladores en vez de una
    tabla hash. Esta ventaja es específica de esa forma del problema, no una
    ley general: un `groupby` con miles de categorías desconocidas de
    antemano no tiene este atajo disponible. Se reporta tal cual, con esa
    salvedad explícita, mismo criterio de honestidad que la pieza 1."""
    zig_result = await validate_aggregation_rust()
    zig = zig_result if zig_result is not None else _degraded("O(n)")
    pandas_timings = await run_in_threadpool(_pandas_aggregation_timings)

    def _format_success(n: int, zig_s: float, pandas_s: float) -> str:
        ratio = pandas_s / zig_s
        return (
            f"En n={n}: Zig (recorrido lineal con 10 acumuladores fijos) tardó {zig_s:.4f}s; "
            f"pandas (groupby().agg real) tardó {pandas_s:.4f}s — el kernel Zig es ≈{ratio:.0f}× más "
            "rápido en esta máquina. A diferencia de la pieza 1 (matmul, donde numpy ganaba), acá gana la "
            "versión nativa — pero con una salvedad real: este kernel asume 10 categorías fijas y conocidas "
            "de antemano, algo que un `groupby` de propósito general no puede asumir. La ventaja viene de esa "
            "simplificación, no de que 'Zig sea más rápido que Python' en general."
        )

    comparison_note = _build_comparison_note("kernel Zig", zig, "pandas", pandas_timings, _format_success)
    return {"zig": zig, "pandas": pandas_timings, "comparison_note": comparison_note}


@router.post("/validate-linreg-vs-sklearn")
async def validate_linreg_vs_sklearn():
    """Tercera y última pieza de la migración numpy/pandas/scikit-learn
    (2026-08-31): corre el kernel Fortran de `validate_linreg` de arriba Y
    scikit-learn real (mismos 4 tamaños, mismos datos — `x_i = i`,
    `y_i = 3x_i + 7 + ruido`) y devuelve los dos resultados lado a lado.

    Resultado honesto, medido en esta máquina, no adivinado: igual que la
    pieza 2 (agregación), acá gana el kernel nativo, por un margen todavía
    más grande (~25×) — `sklearn.LinearRegression.fit()` paga overhead real
    de propósito general (validación de input, soporte multivariado, un
    solver genérico) que el kernel Fortran evita por completo al asumir algo
    que sklearn NO puede asumir: una sola variable predictora con solución
    cerrada conocida de antemano. Se reporta tal cual, con esa salvedad
    explícita — el mismo criterio de honestidad que las piezas 1 y 2."""
    fortran_result = await validate_linreg_rust()
    fortran = fortran_result if fortran_result is not None else _degraded("O(n)")
    # `run_in_threadpool` — este es el más lento de los 3 (~40s reales en
    # esta máquina, ver el módulo docstring), así que es el que expuso el
    # bug: sin esto, la app entera queda sin responder (hasta su propio
    # health check) durante toda la comparación.
    sklearn_timings = await run_in_threadpool(_sklearn_linreg_timings)

    def _format_success(n: int, fortran_s: float, sklearn_s: float) -> str:
        ratio = sklearn_s / fortran_s
        return (
            f"En n={n}: Fortran (forma cerrada, un solo recorrido) tardó {fortran_s:.4f}s; "
            f"scikit-learn (LinearRegression.fit real) tardó {sklearn_s:.4f}s — el kernel Fortran es "
            f"≈{ratio:.0f}× más rápido en esta máquina. Igual que la pieza 2 (agregación), gana la versión "
            "nativa — pero con una salvedad real: este kernel asume una sola variable predictora con "
            "solución cerrada conocida de antemano, algo que un `LinearRegression` de propósito general "
            "(multivariado, con validación de input, varios solvers) no puede asumir. La ventaja viene de "
            "esa simplificación, no de que 'Fortran sea más rápido que Python' en general."
        )

    comparison_note = _build_comparison_note(
        "kernel Fortran", fortran, "scikit-learn", sklearn_timings, _format_success
    )
    return {"fortran": fortran, "sklearn": sklearn_timings, "comparison_note": comparison_note}
