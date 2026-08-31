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
matmul, O(n³)) a 5 más: Zig (bubble sort, O(n²)), Assembly x86 real (suma de
cuadrados, O(n)), Zig otra vez pero forma distinta (recorrido de grafos BFS,
O(V+E)), Fortran otra vez pero la primera forma NO polinomial (Fibonacci
recursivo ingenuo, exponencial Θ(φⁿ)), y Assembly otra vez pero la primera
forma O(n log n) (mergesort bottom-up iterativo) — mismo patrón, mismo
criterio de degradación cada vez.

Primera pieza de "migrar de numpy/pandas/scikit-learn" (pedido explícito del
usuario, 2026-08-31): antes de proponer reemplazar una llamada a una de esas
librerías por un kernel nativo, hay que medir HONESTAMENTE contra la
librería real, no asumir que "nativo" es automáticamente más rápido.
`/validate-matmul-vs-numpy` corre el kernel Fortran que ya existe (arriba)
Y numpy real, mismos tamaños y mismos datos, side by side — ver el
comentario de esa función para el resultado (numpy gana por mucho acá, y
eso se reporta tal cual, no se esconde)."""

import time

from fastapi import APIRouter

from services.complexity_client import (
    validate_bubble_sort_rust,
    validate_fibonacci_rust,
    validate_graph_bfs_rust,
    validate_matmul_bigo_rust,
    validate_mergesort_rust,
    validate_sum_squares_rust,
)

try:
    import numpy as np

    _HAS_NUMPY = True
except ImportError:
    _HAS_NUMPY = False

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
    numpy_timings = _numpy_matmul_timings()

    if not numpy_timings["available"] and not fortran.get("measurements"):
        comparison_note = (
            "Ni numpy ni el sidecar Rust (kernel Fortran) están disponibles en este entorno — no se puede comparar."
        )
    elif not numpy_timings["available"]:
        comparison_note = "numpy no está instalado en este entorno — no se puede comparar."
    elif not fortran.get("measurements"):
        comparison_note = "El sidecar Rust (kernel Fortran) no está disponible en este entorno — no se puede comparar."
    else:
        comparison_note = "No se pudieron obtener tamaños en común entre ambas mediciones."
    if numpy_timings["available"] and fortran.get("measurements"):
        fortran_by_n = {m["n"]: m["seconds"] for m in fortran["measurements"]}
        numpy_by_n = {m["n"]: m["seconds"] for m in numpy_timings["measurements"]}
        common_sizes = sorted(set(fortran_by_n) & set(numpy_by_n))
        if common_sizes:
            n = common_sizes[-1]
            fortran_s, numpy_s = fortran_by_n[n], numpy_by_n[n]
            if numpy_s > 0:
                ratio = fortran_s / numpy_s
                comparison_note = (
                    f"En n={n}: Fortran (triple loop, escrito para validar O(n³), no para competir en velocidad) tardó "
                    f"{fortran_s:.4f}s; numpy (BLAS/LAPACK real) tardó {numpy_s:.6f}s — numpy es ≈{ratio:.0f}× más rápido "
                    "en esta máquina. Conclusión honesta: para multiplicación de matrices específicamente, todavía NO "
                    "hay un reemplazo nativo que le gane a numpy sin bloqueo por cachés/SIMD/paralelismo real — "
                    "'migrar' acá significaría escribir eso, no solo un triple loop en otro lenguaje."
                )

    return {"fortran": fortran, "numpy": numpy_timings, "comparison_note": comparison_note}
