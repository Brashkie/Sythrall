// ══════════════════════════════════════════
//  Algorithm Validation Engine — el panel que compila y corre kernels reales
//  para medir su exponente de crecimiento empírico.
//
//  Vivía dentro de `panels/static.ts`, que llegó a 3.585 líneas. El ROADMAP
//  decía que partirlo "significa mover el bloque de validación empírica con
//  sus ~30 variables de estado de módulo" y lo marcaba como el obstáculo.
//  Medido, era al revés: son 20 variables, **todas suyas**, y el bloque sólo
//  dependía de UNA función del panel. Era la parte más fácil de sacar, no la
//  más difícil.
//
//  Y sacarlo permitió arreglar lo que el propio código señalaba como
//  riesgoso. Había 18 variables sueltas (`_bubbleSortValidation`,
//  `_bubbleSortValidating`, … ×9 kernels) y 9 handlers con el mismo cuerpo,
//  más un comentario que admitía el peligro textual: *"con 13 copias, agregar
//  el próximo kernel significaba copiar 13 líneas más para cambiar 3 tokens,
//  con riesgo real de tocar la variable de validación equivocada en el
//  `finally` por un typo"*. La factoría de handlers resolvió la mitad; lo que
//  quedaba era que el ESTADO siguiera siendo 18 variables nombradas a mano.
//
//  Acá los 9 kernels son **una tabla de datos** y el estado **un Map**
//  indexado por la misma clave. Agregar el décimo kernel es agregar una fila.
//  No hay ninguna variable que se pueda confundir con otra porque no hay
//  variables: hay filas.
// ══════════════════════════════════════════

import type { EmpiricalValidationResult } from '../api/client'
import { api } from '../api/client'
import { esc, toast } from '../utils/helpers'

/** Lo único que este módulo necesita saber de una función analizada.
 *
 * **No importa `ParsedFunction` de `panels/static.ts` a propósito**: ese tipo
 * vive dentro del panel y no está exportado, así que traerlo crearía una
 * dependencia circular (el panel importa este módulo). Y tampoco hace falta:
 * de las ~40 propiedades de `ParsedFunction`, acá se usan **dos**. Declarar
 * el subconjunto real deja escrito cuál es la dependencia de verdad, y
 * cualquier objeto con esos dos campos encaja. */
interface FuncionConBigO {
  name: string
  big_o: string
}

/** Los atributos `data-*` con los que el panel marca cada botón. Son la
 * **clave de todo**: identifican la fila de la tabla, la entrada del estado y
 * el botón en el DOM, los tres con el mismo string. */
export type AtributoDeValidacion =
  | 'data-validate-bubble-sort'
  | 'data-validate-sum-squares'
  | 'data-validate-graph-bfs'
  | 'data-validate-fibonacci'
  | 'data-validate-mergesort'
  | 'data-validate-aggregation'
  | 'data-validate-linreg'
  | 'data-validate-quicksort'
  | 'data-validate-binary-search'

interface Kernel {
  /** La clave única: atributo del botón, índice del estado. */
  attr: AtributoDeValidacion
  /** Lo que se lee en la fila. */
  titulo: string
  /** El tooltip del botón, que dice qué va a pasar al apretarlo. */
  tooltip: string
  /** El exponente que la teoría predice. Lo medido se compara contra esto. */
  exponenteEsperado: number
  /** El `big_o` con el que una función del archivo "coincide" con el kernel.
   *
   * `undefined` para BFS a propósito: el motor de Big-O nunca emite `O(V+E)`
   * como valor real de una función del usuario — ese string sólo existe en la
   * etiqueta de este kernel, así que el match sería código inalcanzable. */
  bigO?: string
  /** Cómo se escribe lo medido. El default (`n^X.XX`) vale para los
   * polinomiales; Fibonacci mide la BASE de un crecimiento exponencial y la
   * búsqueda binaria mide contra `log2(n)`, así que mentirían con el default. */
  formato?: (v: number) => string
  /** El endpoint que compila y corre el kernel. */
  correr: () => Promise<EmpiricalValidationResult>
}

/** **Los nueve kernels, como datos.** Cada fila es un lenguaje demostrando
 * una forma de crecimiento sobre código que Sythrall escribió — nunca el del
 * usuario. */
const KERNELS: readonly Kernel[] = [
  {
    attr: 'data-validate-bubble-sort',
    titulo: 'Bubble sort (Zig) — predicho O(n²)',
    tooltip:
      'Compila y corre un bubble sort real en Zig (escrito por Sythrall, no tu código) para medir el exponente de crecimiento real',
    exponenteEsperado: 2,
    bigO: 'O(n²)',
    correr: api.validateBubbleSortBigO,
  },
  {
    attr: 'data-validate-sum-squares',
    titulo: 'Suma de cuadrados (Assembly x86) — predicho O(n)',
    tooltip:
      'Ensambla y corre una suma de cuadrados escrita a mano en Assembly x86 (AT&T, cdecl) para medir el exponente de crecimiento real',
    exponenteEsperado: 1,
    bigO: 'O(n)',
    correr: api.validateSumSquaresBigO,
  },
  {
    attr: 'data-validate-graph-bfs',
    titulo: 'Recorrido de grafos BFS (Zig) — predicho O(V+E)',
    tooltip:
      'Compila y corre un BFS real en Zig sobre un grafo disperso de grado fijo (escrito por Sythrall, no tu código) para medir el exponente de crecimiento real',
    exponenteEsperado: 1,
    correr: api.validateGraphBfsBigO,
  },
  {
    attr: 'data-validate-fibonacci',
    titulo: 'Fibonacci recursivo ingenuo (Fortran) — predicho exponencial Θ(φⁿ)',
    tooltip:
      'Compila y corre un Fibonacci recursivo sin memoizar en Fortran (escrito por Sythrall, no tu código) para medir la base real de crecimiento exponencial',
    exponenteEsperado: 1.618,
    bigO: 'O(2^n)',
    formato: (v) => `medido: base≈${v.toFixed(2)} (φ≈1.618)`,
    correr: api.validateFibonacciExponential,
  },
  {
    attr: 'data-validate-mergesort',
    titulo: 'Mergesort bottom-up (Assembly x86) — predicho O(n log n)',
    tooltip:
      'Ensambla y corre un mergesort bottom-up iterativo escrito a mano en Assembly x86 (escrito por Sythrall, no tu código) para medir el exponente de crecimiento real',
    exponenteEsperado: 1,
    bigO: 'O(n log n)',
    correr: api.validateMergesortBigO,
  },
  {
    attr: 'data-validate-aggregation',
    titulo: 'Agregación por categoría (Zig) — predicho O(n)',
    tooltip:
      'Compila y corre una agregación por categoría (suma/promedio, la forma detrás de un groupby().agg()) real en Zig (escrito por Sythrall, no tu código) para medir el exponente de crecimiento real',
    exponenteEsperado: 1,
    bigO: 'O(n)',
    correr: api.validateAggregationBigO,
  },
  {
    attr: 'data-validate-linreg',
    titulo: 'Regresión lineal, forma cerrada (Fortran) — predicho O(n)',
    tooltip:
      'Compila y corre una regresión lineal univariada por mínimos cuadrados (forma cerrada) real en Fortran (escrito por Sythrall, no tu código) para medir el exponente de crecimiento real',
    exponenteEsperado: 1,
    bigO: 'O(n)',
    correr: api.validateLinregBigO,
  },
  {
    attr: 'data-validate-quicksort',
    titulo: 'Quicksort recursivo (Zig) — predicho O(n log n)',
    tooltip:
      'Compila y corre un quicksort recursivo real en Zig (partición de Lomuto, escrito por Sythrall, no tu código) para medir el exponente de crecimiento real — misma forma que el mergesort de arriba, por una técnica distinta',
    exponenteEsperado: 1,
    bigO: 'O(n log n)',
    correr: api.validateQuicksortBigO,
  },
  {
    attr: 'data-validate-binary-search',
    titulo: 'Búsqueda binaria (Assembly x86) — predicho O(log n)',
    tooltip:
      'Ensambla y corre una búsqueda binaria iterativa escrita a mano en Assembly x86 (AT&T, cdecl, escrita por Sythrall, no tu código) para medir cómo escala el tiempo contra log2(tamaño del arreglo) — la primera forma sub-lineal de este motor',
    exponenteEsperado: 1,
    bigO: 'O(log n)',
    formato: (v) => `medido: log2(n)^${v.toFixed(2)}`,
    correr: api.validateBinarySearchBigO,
  },
]

/** Cuánto puede alejarse lo medido de lo predicho y seguir contando como
 * confirmación. Es el valor que tenía el código antes de extraerse: no se
 * cambió, sólo dejó de estar repetido en nueve lugares. */
const TOLERANCIA_DEL_EXPONENTE = 0.3

interface EstadoDeKernel {
  resultado: EmpiricalValidationResult | null
  corriendo: boolean
}

/** **El estado, indexado por la misma clave que la tabla.** Antes eran 18
 * variables sueltas de módulo; el riesgo que el código viejo documentaba —
 * tocar la del kernel equivocado— deja de ser posible porque la clave sale
 * del mismo lugar en la lectura y en la escritura. */
const estado = new Map<AtributoDeValidacion, EstadoDeKernel>()

function estadoDe(attr: AtributoDeValidacion): EstadoDeKernel {
  let e = estado.get(attr)
  if (!e) {
    e = { resultado: null, corriendo: false }
    estado.set(attr, e)
  }
  return e
}

/** Los nombres de las funciones del archivo que comparten el Big-O predicho
 * del kernel.
 *
 * **Es coincidencia de string, no detección de forma**, y el badge lo dice:
 * que una función declare `O(n log n)` no significa que sea un mergesort. */
function coincidencias(funciones: FuncionConBigO[], kernel: Kernel): string[] {
  if (!kernel.bigO) return []
  return funciones.filter((f) => f.big_o === kernel.bigO).map((f) => f.name)
}

function renderFila(kernel: Kernel, funciones: FuncionConBigO[]): string {
  const { resultado, corriendo } = estadoDe(kernel.attr)
  const formato = kernel.formato ?? ((v: number) => `medido: n^${v.toFixed(2)}`)

  let control: string
  if (corriendo) {
    control = `<span class="bigo-cs-badge bigo-empirical"><span class="up-spinner"></span> Validando...</span>`
  } else if (resultado) {
    if (!resultado.available || resultado.estimated_exponent == null) {
      control = `<span class="bigo-cs-badge bigo-empirical bigo-empirical-off" title="${esc(resultado.note)}">sin validar</span>`
    } else {
      const cerca = Math.abs(resultado.estimated_exponent - kernel.exponenteEsperado) < TOLERANCIA_DEL_EXPONENTE
      control =
        `<span class="bigo-cs-badge bigo-empirical ${cerca ? 'bigo-empirical-ok' : 'bigo-empirical-off'}" title="${esc(resultado.note)}">${esc(formato(resultado.estimated_exponent))}</span>` +
        ` <button class="btn btn-ghost btn-sm" ${kernel.attr} title="Volver a correr la validación">↺</button>`
    }
  } else {
    control = `<button class="btn btn-ghost btn-sm" ${kernel.attr} title="${esc(kernel.tooltip)}">Validar empíricamente</button>`
  }

  const nombres = coincidencias(funciones, kernel)
  const badge = nombres.length
    ? `<span class="bigo-cs-badge" style="background:var(--ghost-tint);color:var(--info)" title="Al menos una función de este archivo tiene esta misma complejidad Big-O predicha estáticamente — no significa que ESTA función tenga la misma forma que el kernel, solo el mismo exponente reclamado">↳ coincide con ${nombres.map((n) => `<code>${esc(n)}</code>`).join(', ')}</span>`
    : ''

  return `<div class="st-fn-head"><span class="st-fn-name">${esc(kernel.titulo)}</span> ${badge} ${control}</div>`
}

/** La sección entera del motor de validación.
 *
 * **Y acá se fueron tres títulos que quedaron sin contenido.** El render
 * terminaba con `<div class="ms-title">Migración numpy/pandas/scikit-learn —
 * pieza 1/2/3</div>` y nada debajo: las tres comparaciones contra
 * numpy/pandas/sklearn se borraron cuando los puertos nativos ganaron la
 * medición, pero sus encabezados se quedaron. Se veían en la UI como tres
 * subtítulos vacíos. */
export function renderMotorDeValidacion(funciones: FuncionConBigO[]): string {
  return `
  <div class="metric-section">
    <div class="ms-title">Algorithm Validation Engine</div>
    <div class="st-mem-note">Compila y corre kernels reales que Sythrall mismo escribe (nunca tu código) a varios tamaños para medir el exponente de crecimiento empírico. Cuando alguna función de este archivo comparte la misma complejidad Big-O predicha estáticamente que un kernel, se señala con "↳ coincide con" — pura coincidencia de string contra el Big-O table de arriba, no un detector de que tu función realmente TIENE esa forma.</div>
    ${KERNELS.map((k) => renderFila(k, funciones)).join('\n    ')}
  </div>`
}

/** Atiende el click de un botón de validación, si el elemento es uno.
 *
 * Devuelve `true` cuando lo atendió, para que el panel sepa que no tiene que
 * seguir buscando. `repintar` es cómo el panel vuelve a dibujarse: el módulo
 * no sabe nada de cómo se renderiza el resto, sólo avisa que algo cambió —
 * eso es lo que permitió sacarlo de `static.ts` sin arrastrar `_result` ni
 * `_renderResult`.
 */
export function manejarClickDeValidacion(elemento: HTMLElement, repintar: () => void): boolean {
  const kernel = KERNELS.find((k) => elemento.hasAttribute(k.attr))
  if (!kernel) return false
  void correrKernel(kernel, repintar)
  return true
}

async function correrKernel(kernel: Kernel, repintar: () => void): Promise<void> {
  const e = estadoDe(kernel.attr)
  // Dos clicks seguidos no disparan dos compilaciones del mismo kernel.
  if (e.corriendo) return
  e.corriendo = true
  repintar()
  try {
    e.resultado = await kernel.correr()
  } catch (error) {
    toast('Error: ' + (error as Error).message, 'err')
  } finally {
    e.corriendo = false
    repintar()
  }
}

/** Para los tests y para el panel: cuántos kernels hay. Que sea derivado de
 * la tabla y no un número escrito a mano es justamente el punto. */
export const CANTIDAD_DE_KERNELS = KERNELS.length
