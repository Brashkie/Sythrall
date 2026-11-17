// ══════════════════════════════════════════
//  Sythrall — Panel Análisis Estático
//  Mismo patrón que panels/ml.ts y panels/upload.ts
//  Sin IA. Parsers: Python/tree-sitter C/C++/regex TS/JS/tree-sitter Fortran
//    (los 7 plugins de lenguaje que describe services/complexity/src/plugin.rs)
//
//  Fase 24 (Extensibility Platform): este panel es una "extension" en el
//  sentido que documenta plugin.rs — consume la salida de los 7 plugins de
//  lenguaje por el mismo shape JSON de siempre, no implementa la interfaz
//  de manifest/capability él mismo.
// ══════════════════════════════════════════

import type {
  ArchitectureSmell,
  AtomizacionResult,
  BusquedaPareto,
  ComparacionMetaprogramacion,
  CssUsageReport,
  DCostResult,
  EmpiricalValidationResult,
  ExecutableParseResult,
  MemoryLayoutResult,
  NamingSmell,
  PuntoPareto,
  SecurityFinding,
  SparkObligation,
  SparkResult,
  StaticProjectResult,
  StructuralSmell,
  TemplateComplexity,
  TemplateCostResult,
} from '../api/client'
import { api } from '../api/client'
import { guardarUltimoAnalisis, state } from '../store/state'
import { BIG_O_COLOR, renderBigODistribution } from '../utils/bigo'
import { ARCH_SMELL_LABEL, NAMING_SMELL_LABEL, SMELL_LABEL, securitySeverityColor } from '../utils/etiquetas'
import { renderHealthCards } from '../utils/health'
import { appendLog, esc, fileOptionsHtml, fmtBytes, toast } from '../utils/helpers'
import { icon, languageBadgeByName } from '../utils/icons'
import { renderProjectContextBanner, wireProjectContextBanner } from '../utils/projectHeader'
import { type ContextoDeMaquina, lenguajeCompilable, manejarClickDeMaquina, renderCodigoMaquina } from './maquina'
import { manejarClickDeValidacion, renderMotorDeValidacion } from './validacion'

// ─── Tipos ────────────────────────────────────────────────────────────────────

interface ParsedFunction {
  name: string
  line: number
  end_line?: number
  loc?: number
  /** Opcional a propósito: este dato viene de un motor en otro lenguaje,
   * así que TypeScript no puede garantizarlo — el tipo no se chequea en
   * el borde del `fetch`. Declararlo obligatorio hizo que Fortran, que
   * no lo emitía, pintara `undefined` en la columna CC de la tabla de
   * Big-O. Mismo caso que `ParsedImport.type`. */
  complexity?: number
  big_o: string
  big_o_reason: string
  big_o_theta?: string
  big_o_omega?: string
  combinatorics_note?: string | null
  space_complexity?: string
  space_reason?: string
  is_recursive?: boolean
  is_tail_recursive?: boolean
  recursion_note?: string | null
  induction_note?: string | null
  recurrence?: string | null
  regex_class?: string | null
  regex_note?: string | null
  grammar_class?: string | null
  grammar_note?: string | null
  graph_traversal?: string | null
  graph_traversal_note?: string | null
  semantic_analysis_class?: string | null
  semantic_analysis_note?: string | null
  data_structure?: string | null
  data_structure_note?: string | null
  sorting_algorithm?: string | null
  sorting_algorithm_note?: string | null
  search_algorithm?: string | null
  search_algorithm_note?: string | null
  recursion_shape_algorithm?: string | null
  recursion_shape_algorithm_note?: string | null
  /** Fase 32 — `'grover_search'` o `'shor_factoring'`. Forma declarada, no
   * verificacion del circuito. */
  quantum_algorithm?: string | null
  quantum_algorithm_note?: string | null
  memoization?: string | null
  memoization_note?: string | null
  calls?: string[]
  is_async?: boolean
  args?: string[]
  decorators?: string[]
  is_pure?: boolean
  purity_note?: string
  kind?: string
  do_loop_depth?: number
  vectorization_note?: string | null
  numerical_algorithm_note?: string | null
  blas_lapack_calls?: string[]
  registers_used?: string[]
  instructions?: AsmInstruction[]
  stack_frame?: StackFrameInfo
  context_switch?: ContextSwitchBlock[]
}

/** Fase 32 — un bloque de guardado/restauracion de contexto: una sola
 * instruccion que toca muchos registros de golpe (`PUSHA`/`POPA` en x86,
 * `STM`/`LDM` en ARM). Vacio para casi todo procedimiento: son
 * instrucciones de codigo escrito a mano y de manejadores de interrupcion,
 * no de salida de compilador. */
interface ContextSwitchBlock {
  line: number
  mnemonic: string
  direction: 'save' | 'restore'
  registers: string[]
  register_count: number
  /** ARM: restaurar dentro de `pc` ES retornar. */
  is_return: boolean
  /** `PUSHA`/`POPA` no existen en x86-64. */
  invalid_in_x86_64: boolean
  explanation: string
}

// Fase 19 (Machine Intelligence) — una instrucción de Assembly ya
// clasificada por `asmparse.rs` (pattern-matching, no un disassembler).
interface AsmInstruction {
  line: number
  mnemonic: string
  operands: string[]
  category: 'data_movement' | 'arithmetic' | 'logic' | 'comparison' | 'control_flow' | 'stack' | 'other'
  explanation: string
}

// Fase 19, 3er bullet — explicador de calling-convention/stack-frame
// (`callingconv.rs`), reinterpretando las `instructions` que ya vienen en
// cada procedimiento de Assembly. `local_stack_bytes` solo aparece cuando
// el prólogo estándar se detectó Y le sigue un `sub` inmediato sobre el
// stack pointer.
interface StackFrameInfo {
  has_standard_prologue: boolean
  has_standard_epilogue: boolean
  is_leaf_function: boolean
  local_stack_bytes: number | null
  explanation: string
}

// Fase 25 (Modernization Intelligence) — candidato derivado de los
// allocation sites que `memlayout.rs` ya calcula, nunca una conversión
// automática de código.
interface ModernizationCandidate {
  variable: string
  pattern:
    | 'manual_memory_raii'
    | 'unmatched_allocation'
    | 'double_release'
    | 'unsafe_realloc_reassignment'
    | 'use_after_free'
    | 'dangling_local_reference'
    | 'sizeof_pointer_allocation'
    | 'unchecked_malloc_dereference'
    | 'c_style_cast'
    | 'format_string_vulnerability'
    | 'unsafe_string_function'
    | 'unbounded_scanf'
  line: number
  current: string
  suggested_target: 'raii_smart_pointer' | 'rust_ownership' | 'cpp_named_cast' | 'rust_format_macro'
  reasoning: string
  confidence: 'medium' | 'high'
}

interface ModernizationReport {
  candidates: ModernizationCandidate[]
  /** Los `candidates` tienen el mismo shape en todos los lenguajes, pero los
   * conteos del resumen son los patrones de CADA motor: `modernization.rs`
   * (C/C++) y `pymodern.rs` (Python) tienen listas de patrones distintas.
   * Enumerar acá los de C/C++ como si fueran los unicos seria mentir sobre
   * lo que llega para un archivo Python. `total` si es comun a los dos. */
  summary: { total: number } & Record<string, number>
  note: string
}

interface ParsedClass {
  name: string
  line: number
  bases?: string[]
  methods?: Array<{ name: string; line: number }>
  kind?: string
  data_structure?: string | null
  data_structure_note?: string | null
}

interface ParsedImport {
  module: string
  /** Opcional a propósito: este dato viene de un motor en otro lenguaje,
   * así que TypeScript no puede garantizarlo. Declararlo obligatorio
   * hacía que un lenguaje que lo omitiera rompiera el render entero en
   * vez de degradar — pasó con Rust, y lo encontró el navegador. */
  type?: string
  line: number
  name?: string
  alias?: string
}

interface WasmHint {
  function: string
  line: number
  priority: number
  reasons: string[]
  recommendation: string
  estimated_speedup: string
}

/** Fase 34, Línea B — el mapa de dónde el código renuncia a las garantías
 * del compilador, y si lo justificó. No es una métrica de calidad: un
 * `unsafe` documentado es legítimo. Es el inventario de lo que hay que
 * revisar a mano. Ver `rsparse.rs::TrustSurface`. */
interface TrustSurface {
  unsafe_blocks: number
  documented_blocks: number
  unsafe_fns: number
  documented_fns: number
  unsafe_impls: number
}

/** Fase 34, Línea D — dónde el programa tenía una prueba y la tiró.
 * Dual de `TrustSurface`. Ver `rsparse.rs::ProofSurface`. */
/** Fase 34, Línea D — la misma idea en TypeScript. Ver
 * `jscosto.rs::ProofSurfaceTs`. */
/** Fase 34, Línea D — la variante de Python, que completa el trío.
 * Ver `pruebapy.rs::ProofSurfacePy`. */
interface ProofSurfacePy {
  bare_type_ignores: number
  coded_type_ignores: number
  casts: number
  any_annotations: number
}

interface ProofSurfaceTs {
  non_null_assertions: number
  type_assertions: number
  double_assertions: number
  any_annotations: number
  ts_ignore: number
  ts_expect_error: number
}

/** **La superficie de confianza de D: tres niveles donde Rust tiene dos.**
 *
 * `trusted` es el del medio y el que importa — codigo que el compilador NO
 * chequea porque el autor afirma que es seguro llamarlo desde `@safe`. La
 * diferencia con Rust no es de cantidad: en Rust esa superficie se marca con
 * un comentario (`// SAFETY:`) que el compilador no conoce, asi que
 * enumerarla significa leer comentarios; en D es una palabra del lenguaje y
 * `review_surface` la trae completa, con nombre y linea. */
interface TrustSurfaceD {
  safe_fns: number
  trusted_fns: number
  system_fns: number
  /** Sin atributo propio ni heredado. **No es lo mismo que `@system`**: el
   * autor no afirmo nada. En codigo con templates D infiere el atributo, asi
   * que "sin marcar" no quiere decir "sin chequear". */
  unmarked_fns: number
  /** Con el nivel heredado de un `@safe:` o de un bloque, no escrito en la
   * firma. Importa para revisar: es la parte mas facil de pasar por alto. */
  inherited: number
  trusted_with_comment: number
  review_surface: Array<{ name: string; line: number; inherited: boolean; has_comment: boolean }>
}

interface ProofSurface {
  unwrap_calls: number
  expect_calls: number
  unchecked_casts: number
  fallible_fns: number
  unwraps_in_fallible_fns: number
}

interface StaticResult {
  filename: string
  language: string
  /** Fase 32 — `'arm'` entro aca porque reportar `'intel'` para un archivo
   * de ARM era una afirmacion falsa, no un hueco. */
  asm_syntax?: 'att' | 'intel' | 'arm'
  /** Fase 32 — localidad espacial de acceso a matrices. Presente para C/C++
   * desde la 1a porcion y para Fortran desde la 2a. */
  locality_findings?: LocalityFinding[]
  // Fase 27 (AI Intelligence) — solo presentes para `language === 'notebook'`.
  // `cell_ranges` es `[línea_inicio, línea_fin]` (1-based, inclusive) por
  // celda de código, en el mismo orden que aparecen en el .ipynb — todo lo
  // demás (`functions`/`security_findings`/etc.) reporta líneas relativas al
  // pseudo-archivo concatenado, no a la celda individual (ver `notebook.rs`).
  code_cells?: number
  cell_ranges?: Array<[number, number]>
  /** `execution_count` de cada celda de código (el `[N]` de Jupyter);
   * `null` = nunca ejecutada. Mismo orden que `cell_ranges`. */
  execution_counts?: Array<number | null>
  /** Reproducibilidad del notebook — sale del formato del .ipynb, no de
   * analizar el código. `out_of_order_cells` son índices 1-based. */
  reproducibility?: {
    executed_cells: number
    unexecuted_cells: number
    out_of_order_cells: number[]
    note: string
  }
  // Sythrall CSS Engine — solo presentes para `language === 'css'`. CSS no
  // tiene funciones ni complejidad algorítmica, así que en vez de fingir el
  // shape genérico trae el suyo (ver `cssparse.rs`).
  css_findings?: Array<{
    kind: string
    selector: string
    line: number
    severity: string
    confidence: string
    message: string
  }>
  css_note?: string
  // Dependency Intelligence — solo para `language === 'manifest'`.
  manifest_findings?: Array<{
    kind: string
    dependency: string
    line: number
    severity: string
    confidence: string
    message: string
  }>
  manifest_note?: string
  // HTML — accesibilidad y estructura. Solo para `language === 'html'`.
  html_findings?: Array<{
    kind: string
    element: string
    line: number
    severity: string
    confidence: string
    message: string
  }>
  html_note?: string
  /** Fase 15 (Mathematical Intelligence) — precondiciones de control óptimo
   * violadas de forma demostrable. Solo para Python/notebook. */
  control_findings?: Array<{
    kind: string
    function: string
    line: number
    severity: string
    confidence: string
    message: string
  }>
  functions: ParsedFunction[]
  classes: ParsedClass[]
  imports: ParsedImport[]
  /** JS/TS traen `{ name, line }`; Ada agrega `kind` (declaracion o entry) y
   * `params`. Opcionales porque los otros lenguajes no los mandan. */
  exports: Array<{ name: string; line: number; kind?: string; params?: number }>
  interfaces?: Array<{ name: string; line: number }>
  types?: Array<{ name: string; line: number }>
  dead_code: Array<{ type: string; name?: string; module: string; line: number }>
  call_graph: Array<{ from: string; to: string }>
  wasm_hints: WasmHint[]
  memory_layout?: MemoryLayoutResult
  /** Fase 34, Línea A — solo C++; ver `_renderTemplateComplexity`. */
  template_complexity?: TemplateComplexity
  /** Fase 34 — Rust (Línea B, dos niveles) o D (Línea C, tres); ver
   * `_renderTrustSurfaceAny`. */
  trust_surface?: TrustSurface | TrustSurfaceD
  /** Fase 34, Línea D — su dual; ver `_renderProofSurface`. */
  proof_surface?: ProofSurface | ProofSurfaceTs | ProofSurfacePy
  modernization?: ModernizationReport
  security_findings: SecurityFinding[]
  structural_smells: StructuralSmell[]
  naming_smells: NamingSmell[]
  summary: Record<string, number | string>
  error?: string
}

/** Color de una complejidad ciclomática. Un valor ausente es `—` en gris:
 * distinto de un 0, que sería una afirmación. */
/** **El estado bueno NO lleva color, y ese es el punto.**
 *
 * Un tablero donde cada numero esta pintado de algo le pide atencion al ojo
 * en seis lugares a la vez, o sea en ninguno. El color se reserva para lo
 * que cruza un umbral: si todo esta bien, la fila se lee de un vistazo como
 * texto normal y lo que salta es lo unico que hay que mirar. */
function _ccColor(cc?: number): string {
  if (cc === undefined) return 'var(--muted)'
  if (cc >= 10) return 'var(--err)'
  if (cc >= 5) return 'var(--warn)'
  return 'var(--txt)'
}

// ─── Colores Big O ────────────────────────────────────────────────────────────

// ─── Estado del panel ─────────────────────────────────────────────────────────

let _result: StaticResult | null = null
// Fase 17 — el backend nunca devuelve el código fuente de vuelta (solo la
// estructura parseada), así que el visualizador de Compiler Explorer necesita
// su propia copia guardada al momento del análisis. Junto a `_result`, no
// como parámetro de `_renderResult`, porque ese re-renderiza sin el archivo a
// mano después de una validación empírica (`_makeValidateHandler`).
let _resultSource = ''
let _loading = false

// Fase 23 — resultado de la validación empírica de matmul, compartido entre
// todas las funciones con `numerical_algorithm_note` (el kernel es genérico,
// no por-función), así que una sola corrida vale para toda la sesión del panel.
let _matmulValidation: EmpiricalValidationResult | null = null
let _matmulValidating = false

// Fase 34, Línea A — la mitad MEDIDA del costo de la metaprogramación. Mismo
// criterio que `_matmulValidation`: la sonda es genérica (no depende del
// archivo abierto), así que una corrida vale para toda la sesión del panel.
let _tplCostResult: TemplateCostResult | null = null
// Fase 34, Linea C — el segundo punto de datos.
let _sparkRunning = false
let _sparkResult: SparkResult | null = null
let _metaCompRunning = false
let _metaCompResult: ComparacionMetaprogramacion | null = null
let _tplCostRunning = false

// Fase 26 — mismo criterio que `_matmulValidation`, generalizado a los otros
// 2 kernels que el motor ya sabe correr (Zig/bubble-sort, Assembly/suma de
// cuadrados). A diferencia de matmul, estos no cuelgan de un badge detectado
// por-función (no existe un detector "esto parece bubble sort" todavía) — se
// muestran en una sección propia, siempre visible para lenguajes nativos.

// Fase 26 — primera pieza de "migrar de numpy/pandas/scikit-learn" (pedido
// explícito del usuario): comparación real matmul Fortran vs. numpy, no un
// exponente medido más — por eso vive con su propio estado/render, no
// reusa `_renderKernelValidationRow`.

// Fase 26 — segunda pieza de "migrar de numpy/pandas/scikit-learn": mismo
// criterio que `_matmulVsNumpyComparison` de arriba, esta vez agregación
// (Zig) vs. pandas real.

// Fase 26 — tercera y última pieza de "migrar de numpy/pandas/scikit-learn":
// mismo criterio, esta vez regresión lineal (Fortran) vs. scikit-learn real.

// Fase 19 (Machine Intelligence) — analizador de ejecutables. Independiente
// del pipeline de código fuente de arriba (no hay "archivo seleccionado":
// el usuario sube un binario aparte, no algo ya cargado como texto en
// `state.files`), así que vive en su propio estado y su propio `<div>`
// (`#st-exe-result`), no dentro de `_result`/`_renderResult`.
let _exeResult: ExecutableParseResult | null = null
let _exeFileName: string | null = null
let _exeAnalyzing = false

// ─── Repintado y fuente analizada ─────────────────────────────────────────────

/** Repinta el resultado sin tirar el análisis.
 *
 * **`renderStaticPanel()` no sirve acá**: reescribe el panel entero —barra de
 * herramientas y estado vacío incluidos— y se lleva puesto el resultado del
 * análisis, que es justo donde viven las secciones interactivas. Marcar un
 * chip hacía desaparecer todo. `_renderResult` escribe solo en `#st-body`.
 *
 * **Se llamaba `_repintarMaquina`, y el nombre era falso**: lo usan seis
 * features distintas (código máquina, comparación de metaprogramación, SPARK,
 * Atomic CSS, el frente de Pareto y el costo de templates) porque es *el*
 * repintado del panel, no el de una sección. Con ese nombre, cada feature
 * nueva que lo necesitaba parecía estar tomando prestado algo ajeno. */
function _repintarResultado(): void {
  if (_result) _renderResult(_result)
}

/** El fuente que produjo el resultado que se está viendo.
 *
 * **No es `state.currentFile`.** Eso lo setea el explorador al abrir un
 * archivo, pero analizar desde el selector de este panel no lo toca — así que
 * leer de ahí daba vacío justo en el camino normal de uso. `_resultSource` es
 * lo que `_runSingle` guardó, o sea exactamente lo que produjo el resultado.
 *
 * **Esta expresión estaba escrita tres veces idéntica** (código máquina,
 * Pareto y Atomic CSS) y solo una de las tres traía la explicación de por qué
 * el `||` está ahí. Las otras dos la repetían por copia. */
function _fuenteAnalizada(): string {
  return _resultSource || (state.currentFile?.content ?? '')
}

/** Lo que el explorador de código máquina necesita del panel.
 *
 * Funciones y no valores: el resultado que se está viendo cambia con cada
 * análisis, y el handler se registra una sola vez. */
const _contextoDeMaquina: ContextoDeMaquina = {
  resultado: () => _result,
  fuente: _fuenteAnalizada,
  repintar: _repintarResultado,
}

// ─── Render principal ─────────────────────────────────────────────────────────

export function renderStaticPanel(): void {
  const el = document.getElementById('static-content')
  if (!el) return

  const f = state.currentFile
  const fileOpts = fileOptionsHtml(state.files)

  el.innerHTML = `
    <!-- Toolbar -->
    <div class="st-toolbar">
      <select id="st-file-sel" aria-label="Archivo a analizar" class="diag-sel" style="min-width:200px">
        <option value="">— Selecciona un archivo —</option>
        ${fileOpts}
      </select>
      <button class="btn btn-run btn-sm" id="st-run-btn">Analizar</button>
      <button class="btn btn-ghost btn-sm" id="st-run-project-btn" title="Analizar todos los archivos cargados">Analizar proyecto</button>
      <button class="btn btn-ghost btn-sm" id="st-exe-btn" title="Fase 19 (Machine Intelligence) — analiza la estructura real de un ejecutable/librería ya compilado (.exe/.dll/.so/.dylib/.o): secciones, símbolos, imports/exports">Analizar ejecutable</button>
      <input type="file" id="st-exe-input" class="hidden" />
      <div class="flex-1"></div>
      <span id="st-lang-badge"></span>
    </div>

    <!-- Fase 19 — Analizador de ejecutables (independiente del pipeline de código fuente de arriba) -->
    <div id="st-exe-result">${_renderExeResult()}</div>

    <!-- Contenido -->
    <div class="st-body" id="st-body">
      ${
        !state.files.length
          ? `<div class="empty">
            ${
              state.activeProjectId
                ? 'Hay un proyecto activo — click "Analizar proyecto" arriba'
                : 'Carga archivos .py .ts .js .c .cpp, o elegí un proyecto activo en Proyectos'
            }
           </div>`
          : `<div class="empty">
            ${
              state.currentFile
                ? `"${esc(state.currentFile.name)}" cargado — click "Analizar" arriba para ver Big-O, smells y seguridad`
                : 'Selecciona un archivo y haz clic en Analizar'
            }
           </div>`
      }
    </div>
  `

  // Preseleccionar archivo actual
  if (f) {
    const sel = document.getElementById('st-file-sel') as HTMLSelectElement
    if (sel) sel.value = f.id
  }

  _attachStaticEvents(el)

  // Re-hidratar desde el análisis de proyecto ya calculado — sin esto, salir
  // de Static y volver (o volver a activar el tab) pisaba `#st-body` con el
  // prompt "click Analizar proyecto" de arriba aunque `state.results.
  // projectDashboard` ya tuviera el resultado completo, mostrando un estado
  // "sin analizar" para un proyecto que en realidad ya estaba analizado.
  if (!state.files.length && state.activeProjectId && state.results.projectDashboard) {
    _renderProjectResult(state.results.projectDashboard)
  }
}

// ─── Eventos ──────────────────────────────────────────────────────────────────

function _attachStaticEvents(el: HTMLElement): void {
  el.querySelector('#st-run-btn')?.addEventListener('click', () => _runSingle())
  el.querySelector('#st-run-project-btn')?.addEventListener('click', () => _runProject())
  // Fase 19 — el botón solo abre el file input; no hay filtro de extensión
  // (`accept`) a propósito, el formato se detecta por firma binaria del
  // lado Rust, no por nombre de archivo.
  const exeInput = el.querySelector('#st-exe-input') as HTMLInputElement
  el.querySelector('#st-exe-btn')?.addEventListener('click', () => exeInput?.click())
  exeInput?.addEventListener('change', () => {
    const file = exeInput.files?.[0]
    if (file) void _onExeFileSelected(file)
    exeInput.value = ''
  })
  // Fase 23/26 — delegado porque los botones viven dentro de #st-body, que
  // se reescribe entero en cada _renderResult (no existen todavía al momento
  // de este addEventListener si se ataran directo a cada botón).
  //
  // **Y se ata UNA sola vez.** Los listeners de arriba van sobre elementos que
  // `renderStaticPanel()` reescribe en cada render, así que se van con ellos;
  // éste va sobre `#static-content`, que PERSISTE — se acumulaba uno nuevo por
  // cada render del panel. Nadie lo notó porque todos los handlers de esta
  // lista son idempotentes: validar dos veces solo valida dos veces. El
  // primero que alterna (los chips de Código máquina) lo destapó enseguida:
  // con dos listeners, un chip se prendía y se apagaba en el mismo click y el
  // resultado era que no pasaba nada.
  if (el.dataset['clickDelegado'] === '1') return
  el.dataset['clickDelegado'] = '1'
  el.addEventListener('click', (e) => {
    const t = e.target as HTMLElement
    if (t.dataset['validateMatmul'] !== undefined) void _onValidateMatmulClick()
    // Los 9 kernels del Algorithm Validation Engine viven en
    // `panels/validacion.ts`: eran 9 líneas idénticas acá, con su estado y
    // sus handlers duplicados. Ahora el módulo resuelve cuál es por su
    // atributo y le pasamos cómo repintar.
    if (
      manejarClickDeValidacion(t, () => {
        if (_result) _renderResult(_result)
      })
    )
      return
    if (t.dataset['validateTemplateCost'] !== undefined) void _onValidateTemplateCostClick()
    if (t.dataset['compareMeta'] !== undefined) void _compararMeta()
    if (t.dataset['verifySpark'] !== undefined) void _verificarSpark()
    if (t.dataset['loadGodbolt'] !== undefined) _onLoadGodboltClick(t)

    // Los 6 controles del explorador de código máquina viven en
    // `panels/maquina.ts`. Estaban acá, y la rama de `mc-cerrar` había
    // quedado DESPUÉS de las dos de abajo, separada de las otras cinco que le
    // corresponden — el tipo de cosa que pasa en un dispatch de 80 líneas.
    if (manejarClickDeMaquina(t, _contextoDeMaquina)) return
    if (t.dataset['paretoBuscar'] !== undefined && _result) void _buscarPareto(_result)
    if (t.dataset['cssAtomizar'] !== undefined && _result) void _atomizarCss(_result)
  })
}

// ─── Análisis de un archivo ───────────────────────────────────────────────────

async function _runSingle(): Promise<void> {
  const sel = document.getElementById('st-file-sel') as HTMLSelectElement
  const f = state.files.find((x) => x.id === sel?.value)
  if (!f) {
    toast('Selecciona un archivo', 'warn')
    return
  }

  _setLoading(true)
  try {
    const data = await api.staticParse(f.name, f.content)
    _result = data as StaticResult
    _resultSource = f.content
    _renderResult(_result)
    appendLog('ok', `Static: ${f.name} — ${data.functions?.length ?? 0} funciones`, 'be')
  } catch (e) {
    _showError((e as Error).message)
    toast('Error: ' + (e as Error).message, 'err')
  } finally {
    _setLoading(false)
  }
}

// ─── Análisis del proyecto completo ──────────────────────────────────────────

/** El Dashboard muestra el mismo análisis de proyecto (Project Health,
 * Findings, distribución Big-O, funciones más complejas) — un solo fetch acá
 * lo actualiza ahí también, sin pedir el análisis dos veces. */
function _syncDashboard(data: StaticProjectResult): void {
  state.results.projectDashboard = data
  state.lastAnalysisAt = Date.now()
  if (state.activeProjectId) guardarUltimoAnalisis(state.activeProjectId, data)
  import('../components/app').then((m) => m.updateGlobalPill())
  import('./dashboard').then((m) => m.renderDashboard())
}

async function _runProject(): Promise<void> {
  // Sin archivos cargados a mano — si hay proyecto activo, se analiza directo
  // del disco (mismo patrón que Issues, ver analyzeAllFiles en components/app.ts).
  if (!state.files.length && state.activeProjectId) {
    _setLoading(true)
    state.projectAnalysisRunning = true
    const { renderFlow } = await import('../components/flow')
    renderFlow()
    try {
      const data = await api.staticParseProjectById(state.activeProjectId)
      _renderProjectResult(data)
      _syncDashboard(data)
      // Después del render: la sección de referencias cruzadas se rellena
      // sola cuando llega, sin bloquear el resto del análisis (recorre el
      // árbol entero, así que tarda más que el parseo por archivo).
      void loadCssUsage(state.activeProjectId)
      appendLog('ok', `Proyecto activo: ${data.summary.total_files} archivos analizados`, 'be')
    } catch (e) {
      _showError((e as Error).message)
    } finally {
      _setLoading(false)
      state.projectAnalysisRunning = false
      renderFlow()
    }
    return
  }

  if (!state.files.length) {
    toast('Carga archivos primero, o elegí un proyecto activo en Proyectos', 'warn')
    return
  }

  _setLoading(true)
  try {
    const files = state.files.map((f) => ({ filename: f.name, content: f.content }))
    const data = await api.staticParseProject(files)
    _renderProjectResult(data)
    // Deliberadamente NO se sincroniza con el Dashboard: esto es análisis
    // ad-hoc de archivos sueltos del Editor, no del proyecto activo — el
    // Dashboard es project-centric por diseño (ver panels/dashboard.ts) y
    // mezclar ambas fuentes acá reintroduciría exactamente la ambigüedad
    // "¿estos archivos son del proyecto o los cargué a mano?" que se corrigió.
    appendLog('ok', `Proyecto: ${files.length} archivos analizados`, 'be')
  } catch (e) {
    _showError((e as Error).message)
  } finally {
    _setLoading(false)
  }
}

// ─── Render resultado de un archivo ──────────────────────────────────────────

function _renderResult(r: StaticResult): void {
  const body = document.getElementById('st-body')!

  // Badge de lenguaje
  const badge = document.getElementById('st-lang-badge')!
  badge.innerHTML = `${languageBadgeByName(r.language)} <span style="font-family:var(--mono);font-size:.68rem;color:var(--muted)">${esc(r.language)}</span>`

  if (r.error) {
    body.innerHTML = `<div class="st-error">${icon('warning', 14)} ${esc(r.error)}</div>`
    return
  }

  body.innerHTML = `
    <!-- Resumen -->
    ${_renderSummaryCards(r)}

    <!-- Celdas de código del notebook (Fase 27) -->
    ${r.language === 'notebook' && r.cell_ranges?.length ? _renderNotebookCells(r.cell_ranges, r.execution_counts, r.reproducibility) : ''}

    <!-- Sythrall CSS Engine -->
    ${r.language === 'css' ? _renderCssAnalysis(r) : ''}

    <!-- Dependency Intelligence (manifests) -->
    ${r.language === 'manifest' ? _renderManifestAnalysis(r) : ''}

    <!-- HTML: accesibilidad y estructura -->
    ${r.language === 'html' ? _renderHtmlAnalysis(r) : ''}

    <!-- Control óptimo: precondiciones (Fase 15) -->
    ${r.control_findings?.length ? _renderControlFindings(r.control_findings) : ''}

    <!-- Security findings -->
    ${r.security_findings?.length ? _renderSecurityFindings(r.security_findings) : ''}

    <!-- Structural smells -->
    ${r.structural_smells?.length ? _renderStructuralSmells(r.structural_smells) : ''}

    <!-- Naming smells -->
    ${r.naming_smells?.length ? _renderNamingSmells(r.naming_smells) : ''}

    <!-- Big O table -->
    ${_renderBigOTable(r.functions)}

    <!-- Algorithm Validation Engine — kernels Zig/Assembly (Fase 26) -->
    ${['c', 'cpp', 'fortran', 'assembly'].includes(r.language) ? renderMotorDeValidacion(r.functions) : ''}

    <!-- Compiler pipeline — Compiler Explorer embebido (Fase 17) -->
    ${
      GODBOLT_LANG_MAP[r.language]
        ? _renderCompilerExplorer(
            r.language,
            _resultSource,
            r.functions.some((f) => f.is_tail_recursive),
          )
        : ''
    }

    <!-- Funciones -->
    ${_renderFunctions(r.functions)}

    <!-- Clases -->
    ${r.classes.length ? _renderClasses(r.classes, r.language) : ''}

    <!-- Firmas sin cuerpo (Ada .ads) / exports (JS/TS) -->
    ${r.exports.length ? _renderExports(r.exports, r.language) : ''}

    <!-- Imports / Dead code -->
    ${_renderImports(r.imports, r.dead_code)}

    <!-- Localidad espacial (C/C++ y Fortran) -->
    ${_renderLocality(r.locality_findings ?? [], r.language)}

    <!-- Interfaces / Types (TS) -->
    ${r.interfaces?.length ? _renderInterfaces(r.interfaces, r.types ?? []) : ''}

    <!-- Call graph -->
    ${r.call_graph.length ? _renderCallGraph(r.call_graph) : ''}

    <!-- WASM hints -->
    ${r.wasm_hints.length ? _renderWasmHints(r.wasm_hints) : ''}

    <!-- Memory layout (C/C++, Fase 23) -->
    ${r.memory_layout?.variables.length ? _renderMemoryLayout(r.memory_layout) : ''}

    <!-- Metaprogramación C++ (Fase 34, Línea A) -->
    ${r.template_complexity?.templates ? _renderTemplateComplexity(r.template_complexity) : ''}

    <!-- Modernization Intelligence (C/C++, Fase 25) -->
    ${r.modernization?.candidates.length ? _renderModernization(r.modernization) : ''}

    <!-- Codigo maquina: fuente -> ensamblador (Fase 19) -->
    ${renderCodigoMaquina(r)}

    <!-- Busqueda de configuracion, frente de Pareto (Fase 32) -->
    ${lenguajeCompilable(r) ? _renderPareto() : ''}

    <!-- Atomic CSS (Fase 31) -->
    ${_renderCssAtomico(r)}

    <!-- Assembly breakdown (Fase 19) -->
    ${r.language === 'assembly' ? _renderAsmBreakdown(r.functions, r.asm_syntax) : ''}

    <!-- Verificacion formal, solo Ada (Fase 34 Linea E paso 3) -->
    ${r.language === 'ada' ? _renderSpark() : ''}

    <!-- Trust Surface (Rust, Fase 34 Línea B) -->
    ${r.trust_surface ? _renderTrustSurfaceAny(r.trust_surface) : ''}

    <!-- Proof Surface (Rust, Fase 34 Línea D) -->
    ${r.proof_surface ? _renderProofSurfaceAny(r.proof_surface) : ''}
  `
}

// ─── Secciones de render ──────────────────────────────────────────────────────

function _renderSummaryCards(r: StaticResult): string {
  const s = r.summary as Record<string, number>
  const avg = typeof s.avg_complexity === 'number' ? s.avg_complexity : 0
  // Usa `_ccColor` en vez de repetir la escalera de umbrales: era la SEGUNDA
  // copia de las mismas dos cotas (5 y 10), y al neutralizar el estado bueno
  // en una quedo verde en la otra. Una regla, un lugar.
  const avgColor = _ccColor(avg)

  return `
  <div class="st-summary-row">
    ${sc(String(s.total_functions ?? r.functions.length), 'Funciones')}
    ${sc(String(s.total_classes ?? r.classes.length), 'Clases')}
    ${sc(String(s.total_imports ?? r.imports.length), 'Imports')}
    ${sc(String(s.unused_imports ?? r.dead_code.length), 'No usados', s.unused_imports ? 'var(--warn)' : undefined)}
    ${sc(avg.toFixed(1), 'CC promedio', avgColor)}
    ${sc(String(s.max_loc_function ?? 0), 'Max LOC/fn')}
  </div>`
}

/** Fase 27 (AI Intelligence) — mapa "dónde mirar" para un notebook: las
 * líneas que `functions`/`security_findings`/etc. reportan son relativas al
 * pseudo-archivo de texto concatenado (`notebook.rs`), no a la celda
 * individual del `.ipynb` — este bloque deja al usuario cruzar manualmente
 * una línea reportada contra la celda a la que pertenece, en vez de intentar
 * (todavía) reescribir cada line number del resto del panel. */
// Sythrall CSS Engine — referencias cruzadas. Cacheado por proyecto igual
// que el resto de los datos del panel: se pide una vez al analizar y se
// invalida al cambiar de proyecto.
let _cssUsage: CssUsageReport | null = null
let _cssUsageProject: string | null = null

// Sin `export`: no lo importa nadie, se usa solo desde este panel. Estaba
// exportado por inercia, y un export sin consumidores es API publica que
// nadie puede quitar despues sin preguntarse a quien rompe.
async function loadCssUsage(projectId: string): Promise<void> {
  if (_cssUsageProject === projectId && _cssUsage) return
  _cssUsageProject = projectId
  try {
    _cssUsage = await api.cssUsage(projectId)
  } catch {
    _cssUsage = { available: false, reason: 'no se pudo consultar' }
  }
  const host = document.getElementById('st-css-usage')
  if (host) host.innerHTML = _renderCssUsage()
}

/** El orden de esta sección es deliberado: primero los sitios que construyen
 * nombres en runtime, después la lista de "sin referencia". Al revés, alguien
 * leería la lista como si fuera una lista de borrado — y el dato que la
 * relativiza quedaría abajo, donde no se lee. */
function _renderCssUsage(): string {
  const r = _cssUsage
  if (!r?.available || !r.css_files) return ''

  const dyn = r.dynamic_sites ?? []
  const unref = r.unreferenced ?? []
  const stats = [
    sc(String(r.defined_classes ?? 0), 'CLASES DEFINIDAS'),
    sc(String(r.referenced_classes ?? 0), 'CON REFERENCIA', 'var(--ok)'),
    sc(String(unref.length), 'SIN REFERENCIA', unref.length ? 'var(--warn)' : undefined),
    sc(String(dyn.length), 'CONSTRUIDAS EN RUNTIME', dyn.length ? 'var(--err)' : undefined),
    sc(String(r.css_files ?? 0), 'ARCHIVOS CSS'),
  ].join('')

  const dynList = dyn.length
    ? `<div class="ms-title mt-3">Nombres construidos en runtime (${dyn.length})</div>
       ${dyn
         .slice(0, 20)
         .map(
           (d) => `<div class="dash-lang-support-row">
        <span class="sec-cwe">${esc(d.file)}:${d.line}</span>
        <span style="font-family:var(--mono);font-size:.7rem;color:var(--muted)">${esc(d.snippet)}</span>
      </div>`,
         )
         .join('')}
       ${dyn.length > 20 ? `<div class="st-mem-note">…y ${dyn.length - 20} más</div>` : ''}`
    : ''

  const unrefList = unref.length
    ? `<div class="ms-title mt-3">Sin referencia estática encontrada (${unref.length})</div>
       <div style="display:flex;flex-wrap:wrap;gap:5px">
       ${unref
         .slice(0, 60)
         .map(
           (u) =>
             `<span title="${esc(u.file)}:${u.line}" style="font-size:.65rem;padding:2px 8px;border-radius:3px;background:var(--ghost-tint);color:var(--muted);font-family:var(--mono)">.${esc(u.name)}</span>`,
         )
         .join('')}
       </div>
       ${unref.length > 60 ? `<div class="st-mem-note">…y ${unref.length - 60} más</div>` : ''}`
    : ''

  return `
  <div class="st-summary-row">${stats}</div>
  <div class="metric-section">
    <div class="ms-title">CSS — referencias cruzadas</div>
    ${dynList}
    ${unrefList}
    ${r.note ? `<div class="st-mem-note mt-2">${esc(r.note)}</div>` : ''}
  </div>`
}

/** Fase 31/32 — los 5 tipos de hallazgo que nacieron después del agregador
 * de proyecto (CSS, HTML, manifests, precondiciones de control y localidad
 * espacial) se calculaban por archivo y ahí morían. Acá se muestran juntos, con su archivo de
 * origen. **No entran en el score de Project Health**: ver el comentario en
 * `compute_project_analysis` para el porqué. */
function _renderProjectExtraFindings(data: StaticProjectResult): string {
  const d = data as unknown as {
    css_findings?: Array<{ kind: string; selector?: string; line: number; message: string; file?: string }>
    html_findings?: Array<{ kind: string; element?: string; line: number; message: string; file?: string }>
    manifest_findings?: Array<{ kind: string; dependency?: string; line: number; message: string; file?: string }>
    control_findings?: Array<{ kind: string; function?: string; line: number; message: string; file?: string }>
    locality_findings?: Array<{ kind: string; array?: string; line: number; message: string; file?: string }>
  }
  const grupos: Array<[string, Array<{ kind: string; line: number; message: string; file?: string }>, string]> = [
    ['CSS', d.css_findings ?? [], 'var(--purple)'],
    ['HTML / accesibilidad', d.html_findings ?? [], 'var(--info)'],
    ['Dependencias', d.manifest_findings ?? [], 'var(--warn)'],
    ['Control óptimo', d.control_findings ?? [], 'var(--err)'],
    ['Localidad espacial', d.locality_findings ?? [], 'var(--orange)'],
  ].filter(([, list]) => list.length) as never
  if (!grupos.length) return ''

  return `
  <div class="metric-section">
    <div class="ms-title">Otros hallazgos del proyecto (${grupos.reduce((n, [, l]) => n + l.length, 0)})</div>
    <div class="st-mem-note">CSS, HTML, dependencias, precondiciones de control y localidad espacial de acceso. No entran en el score de Project Health: sus dimensiones están calibradas sobre conteos de funciones/clases, y sumarlos ahí cambiaría en silencio el significado de un número que se compara entre corridas.</div>
    ${grupos
      .map(
        ([titulo, list, color]) => `
      <div class="ms-title" style="margin-top:10px;color:${color}">${esc(titulo)} (${list.length})</div>
      ${list
        .slice(0, 25)
        .map(
          (f) => `<div class="dash-lang-support-row">
        <span class="sec-cwe">${esc(f.file ?? '')}:${f.line}</span>
        <span style="font-size:.72rem">${esc(f.message.slice(0, 140))}</span>
      </div>`,
        )
        .join('')}
      ${list.length > 25 ? `<div class="st-mem-note">…y ${list.length - 25} más</div>` : ''}`,
      )
      .join('')}
  </div>`
}

const HTML_FINDING_LABEL: Record<string, [string, string]> = {
  img_without_alt: ['Imagen sin texto alternativo', 'var(--warn)'],
  duplicate_id: ['id duplicado', 'var(--warn)'],
  missing_lang: ['Documento sin idioma', 'var(--warn)'],
  heading_level_skip: ['Salto de nivel de encabezado', 'var(--info)'],
  control_without_label: ['Control sin etiqueta', 'var(--warn)'],
  link_without_href: ['Enlace sin href', 'var(--info)'],
  positive_tabindex: ['tabindex positivo', 'var(--info)'],
}

/** HTML — accesibilidad y estructura. Todo lo que llega acá se verifica
 * leyendo la línea indicada; no es un validador conforme al estándar ni una
 * auditoría de accesibilidad completa (ver la nota del motor). */
function _renderHtmlAnalysis(r: StaticResult): string {
  const s = r.summary as unknown as {
    total_elements?: number
    total_ids?: number
    images?: number
    links?: number
    form_controls?: number
    headings?: number
    bytes?: number
  }
  const findings = r.html_findings ?? []
  const stats = [
    sc(String(s.total_elements ?? 0), 'ELEMENTOS'),
    sc(String(s.headings ?? 0), 'ENCABEZADOS'),
    sc(String(s.images ?? 0), 'IMÁGENES'),
    sc(String(s.links ?? 0), 'ENLACES'),
    sc(String(s.form_controls ?? 0), 'CONTROLES'),
    sc(String(s.total_ids ?? 0), 'IDs ÚNICOS'),
    sc(`${((s.bytes ?? 0) / 1024).toFixed(1)}KB`, 'TAMAÑO'),
  ].join('')

  const list = findings.length
    ? findings
        .map((f) => {
          const [label, color] = HTML_FINDING_LABEL[f.kind] ?? [f.kind, 'var(--muted)']
          return `<div class="st-wasm-item">
        <div class="st-fn-head">
          <span class="st-fn-name">&lt;${esc(f.element)}&gt;</span>
          <span class="badge badge-tinted" style="--tag-color:${color}">${label}</span>
          <span class="st-fn-line ml-auto">línea ${f.line}</span>
        </div>
        <div class="st-wasm-rec">${esc(f.message)}</div>
      </div>`
        })
        .join('')
    : `<div class="empty">Sin hallazgos demostrables en este documento</div>`

  return `
  <div class="st-summary-row">${stats}</div>
  <div class="metric-section">
    <div class="ms-title">HTML — accesibilidad y estructura (${findings.length})</div>
    ${list}
    ${r.html_note ? `<div class="st-mem-note mt-2">${esc(r.html_note)}</div>` : ''}
  </div>`
}

const MANIFEST_FINDING_LABEL: Record<string, [string, string]> = {
  unbounded_dependency: ['Versión sin acotar', 'var(--warn)'],
  external_source_dependency: ['Fuente externa al registro', 'var(--warn)'],
  duplicate_dependency: ['Declarada dos veces', 'var(--info)'],
}

/** Dependency Intelligence — primera Format Extension. Analiza el manifest,
 * no el código: no hay funciones ni Big-O que mostrar, así que trae su
 * propio resumen en vez de fingir el shape genérico. */
function _renderManifestAnalysis(r: StaticResult): string {
  const s = r.summary as unknown as {
    ecosystem?: string
    total?: number
    production?: number
    development?: number
    exact?: number
    ranged?: number
    unbounded?: number
  }
  const stats = [
    sc(String(s.total ?? 0), 'DEPENDENCIAS'),
    sc(String(s.production ?? 0), 'PRODUCCIÓN'),
    sc(String(s.development ?? 0), 'DESARROLLO'),
    sc(String(s.exact ?? 0), 'VERSIÓN EXACTA', 'var(--ok)'),
    sc(String(s.ranged ?? 0), 'RANGO ACOTADO'),
    sc(String(s.unbounded ?? 0), 'SIN ACOTAR', (s.unbounded ?? 0) > 0 ? 'var(--warn)' : undefined),
    sc(String(s.ecosystem ?? '?'), 'ECOSISTEMA'),
  ].join('')

  const findings = r.manifest_findings ?? []
  const list = findings.length
    ? findings
        .map((f) => {
          const [label, color] = MANIFEST_FINDING_LABEL[f.kind] ?? [f.kind, 'var(--muted)']
          return `<div class="st-wasm-item">
        <div class="st-fn-head">
          <span class="st-fn-name">${esc(f.dependency)}</span>
          <span class="badge badge-tinted" style="--tag-color:${color}">${label}</span>
          <span class="st-fn-line ml-auto">línea ${f.line}</span>
        </div>
        <div class="st-wasm-rec">${esc(f.message)}</div>
      </div>`
        })
        .join('')
    : `<div class="empty">Sin hallazgos demostrables en este manifest</div>`

  return `
  <div class="st-summary-row">${stats}</div>
  <div class="metric-section">
    <div class="ms-title">Dependency Intelligence (${findings.length})</div>
    ${list}
    ${r.manifest_note ? `<div class="st-mem-note mt-2">${esc(r.manifest_note)}</div>` : ''}
  </div>`
}

const CONTROL_FINDING_LABEL: Record<string, [string, string]> = {
  lqr_dimension_mismatch: ['Dimensiones incompatibles', 'var(--err)'],
  lqr_weight_not_positive_definite: ['Matriz de peso inválida', 'var(--err)'],
  lqr_non_symmetric_weight: ['Matriz de peso no simétrica', 'var(--warn)'],
  gibbs_nonpositive_temperature: ['Temperatura ε no positiva', 'var(--err)'],
  random_choice_bad_probabilities: ['Probabilidades imposibles', 'var(--err)'],
  softmax_unstable: ['Softmax sin estabilizar', 'var(--warn)'],
}

/** Precondiciones de control óptimo (Fase 15). Todo lo que llega acá es una
 * violación DEMOSTRADA leyendo el código — de ahí que no haya un estado
 * "todo bien": este análisis prueba errores, no certifica correctitud. */
function _renderControlFindings(findings: NonNullable<StaticResult['control_findings']>): string {
  return `
  <div class="metric-section">
    <div class="ms-title">Control óptimo — precondiciones (${findings.length})</div>
    <div class="st-mem-note">Violaciones demostrables de lo que un solver de Riccati/LQR asume sobre sus matrices, y de lo que una selección de Gibbs (<code>exp(-V/ε)</code> normalizado) asume sobre su temperatura y sus probabilidades. No certifica que el resto esté bien: solo reporta lo que se puede probar leyendo el código, y nunca opina sobre si la política elegida es buena.</div>
    ${findings
      .map((f) => {
        const [label, color] = CONTROL_FINDING_LABEL[f.kind] ?? [f.kind, 'var(--muted)']
        return `<div class="st-wasm-item">
      <div class="st-fn-head">
        <span class="st-fn-name">${esc(f.function)}()</span>
        <span style="font-size:.65rem;color:${color};font-family:var(--mono)">${label}</span>
        <span class="st-fn-line ml-auto">línea ${f.line}</span>
      </div>
      <div class="st-wasm-rec">${esc(f.message)}</div>
    </div>`
      })
      .join('')}
  </div>`
}

const CSS_FINDING_LABEL: Record<string, [string, string]> = {
  duplicate_declaration: ['Declaración repetida', 'var(--warn)'],
  duplicate_selector: ['Selector duplicado', 'var(--warn)'],
  high_specificity: ['Especificidad alta', 'var(--info)'],
}

/** Sythrall CSS Engine — resumen medido + hallazgos demostrables. La nota
 * del motor (que dice explícitamente qué NO reporta y por qué) se muestra
 * siempre, incluso sin hallazgos: es parte del resultado, no un pie de
 * página opcional. */
function _renderCssAnalysis(r: StaticResult): string {
  const s = r.summary as unknown as {
    total_rules?: number
    total_selectors?: number
    total_declarations?: number
    total_at_rules?: number
    important_count?: number
    bytes?: number
    max_specificity?: [number, number, number]
  }
  const spec = s.max_specificity ?? [0, 0, 0]
  const stats = [
    sc(String(s.total_rules ?? 0), 'REGLAS'),
    sc(String(s.total_selectors ?? 0), 'SELECTORES'),
    sc(String(s.total_declarations ?? 0), 'DECLARACIONES'),
    sc(String(s.total_at_rules ?? 0), 'AT-RULES'),
    sc(String(s.important_count ?? 0), '!IMPORTANT', (s.important_count ?? 0) > 0 ? 'var(--warn)' : undefined),
    sc(`${((s.bytes ?? 0) / 1024).toFixed(1)}KB`, 'TAMAÑO'),
    sc(spec.join(','), 'ESPECIFICIDAD MÁX'),
  ].join('')

  const findings = r.css_findings ?? []
  const list = findings.length
    ? findings
        .map((f) => {
          const [label, color] = CSS_FINDING_LABEL[f.kind] ?? [f.kind, 'var(--muted)']
          return `<div class="st-wasm-item">
        <div class="st-fn-head">
          <span class="st-fn-name">${esc(f.selector)}</span>
          <span class="badge badge-tinted" style="--tag-color:${color}">${label}</span>
          <span class="st-fn-line ml-auto">línea ${f.line}</span>
        </div>
        <div class="st-wasm-rec">${esc(f.message)}</div>
      </div>`
        })
        .join('')
    : `<div class="empty">Sin hallazgos demostrables en este archivo</div>`

  return `
  <div class="st-summary-row">${stats}</div>
  <div class="metric-section">
    <div class="ms-title">Sythrall CSS Engine (${findings.length})</div>
    ${list}
    ${r.css_note ? `<div class="st-mem-note mt-2">${esc(r.css_note)}</div>` : ''}
  </div>`
}

function _renderNotebookCells(
  cellRanges: Array<[number, number]>,
  executionCounts?: Array<number | null>,
  repro?: StaticResult['reproducibility'],
): string {
  const outOfOrder = new Set(repro?.out_of_order_cells ?? [])
  // Un chip por celda: rango de líneas + su `[N]` real de Jupyter. Rojo si
  // esa celda se ejecutó antes que alguna de más arriba, gris si nunca se
  // ejecutó — el mismo dato que el resumen de abajo, pero ubicable celda a
  // celda en vez de solo contado.
  const chips = cellRanges
    .map(([start, end], i) => {
      const count = executionCounts?.[i]
      const bad = outOfOrder.has(i + 1)
      const color = bad ? 'var(--err)' : count == null ? 'var(--muted)' : 'var(--txt)'
      const exec = count == null ? '[ ]' : `[${count}]`
      const title = bad
        ? 'Esta celda se ejecutó antes que alguna celda que está más arriba en el notebook'
        : count == null
          ? 'Celda sin ejecutar en el estado guardado'
          : `Contador de ejecución de Jupyter: ${count}`
      return `<span title="${esc(title)}" style="font-size:.65rem;padding:2px 8px;border-radius:3px;background:var(--ghost-tint);color:${color};font-family:var(--mono)">celda ${i + 1} ${exec}: L${start}-${end}</span>`
    })
    .join('')

  const issues = (repro?.out_of_order_cells.length ?? 0) + (repro?.unexecuted_cells ?? 0)
  const reproBlock = repro
    ? `<div class="st-mem-note" style="margin-top:6px;color:${issues ? 'var(--warn)' : 'var(--muted)'}">
        Reproducibilidad: ${repro.executed_cells} ejecutada(s), ${repro.unexecuted_cells} sin ejecutar, ${repro.out_of_order_cells.length} fuera de orden — ${esc(repro.note)}
      </div>`
    : ''

  return `
  <div class="metric-section">
    <div class="ms-title">Celdas de código (${cellRanges.length})</div>
    <div class="st-mem-note">Las líneas del resto de este panel son relativas al texto concatenado de estas celdas, no al notebook original — usá este mapa para ubicar a qué celda pertenece una línea reportada</div>
    <div style="display:flex;flex-wrap:wrap;gap:5px;margin-top:4px">${chips}</div>
    ${reproBlock}
  </div>`
}

function sc(val: string, label: string, color?: string): string {
  return `<div class="st-stat">
    <div class="st-stat-val"${color ? ` style="color:${color}"` : ''}>${esc(val)}</div>
    <div class="st-stat-lbl">${label}</div>
  </div>`
}

function _renderBigOTable(functions: ParsedFunction[]): string {
  if (!functions.length) return ''

  const rows = functions
    .map((f) => {
      const color = BIG_O_COLOR[f.big_o] ?? 'var(--muted)'
      const isHot = ['O(n²)', 'O(n³)', 'O(2^n)'].includes(f.big_o)
      return `<tr class="${isHot ? 'bigo-hot' : ''}">
      <td class="bigo-fn">${esc(f.name)}
        ${f.is_async ? '<span class="bigo-async">async</span>' : ''}
        ${
          f.is_recursive || f.is_tail_recursive
            ? // `is_recursive` es Python-only (recursion.rs) -- C/C++ (Fase 17,
              // 2026-09-02) solo calcula `is_tail_recursive` (cparse.rs), nunca
              // recursión general, así que el `||` deja que este mismo badge
              // sirva para ambos sin duplicar markup.
              `<span class="bigo-recursion ${f.is_tail_recursive ? 'bigo-recursion-tail' : 'bigo-recursion-notail'}" title="${esc([f.recursion_note, f.induction_note, f.recurrence].filter(Boolean).join('\n\n'))}">${f.is_tail_recursive ? 'tail-call' : 'recursión'}</span>`
            : ''
        }
        ${f.regex_class ? `<span class="bigo-cs-badge bigo-regex" title="${esc(f.regex_note ?? '')}">Regex</span>` : ''}
        ${f.grammar_class ? `<span class="bigo-cs-badge bigo-grammar" title="${esc(f.grammar_note ?? '')}">CFG</span>` : ''}
        ${f.graph_traversal ? `<span class="bigo-cs-badge bigo-graph" title="${esc(f.graph_traversal_note ?? '')}">${esc(f.graph_traversal)}</span>` : ''}
        ${f.semantic_analysis_class ? `<span class="bigo-cs-badge bigo-semantic" title="${esc(f.semantic_analysis_note ?? '')}">Type-1?</span>` : ''}
        ${f.data_structure ? `<span class="bigo-cs-badge bigo-datastruct" title="${esc(f.data_structure_note ?? '')}">${esc(f.data_structure)}</span>` : ''}
        ${f.sorting_algorithm === 'bubble_sort' ? `<span class="bigo-cs-badge bigo-numerical" title="${esc(f.sorting_algorithm_note ?? '')}">Bubble sort?</span>` : ''}
        ${f.search_algorithm === 'binary_search' ? `<span class="bigo-cs-badge bigo-numerical" title="${esc(f.search_algorithm_note ?? '')}">Binary search?</span>` : ''}
        ${f.quantum_algorithm ? `<span class="bigo-cs-badge bigo-numerical" title="${esc(f.quantum_algorithm_note ?? '')}">${f.quantum_algorithm === 'grover_search' ? 'Grover?' : 'Shor?'}</span>` : ''}
        ${f.recursion_shape_algorithm === 'naive_fibonacci' ? `<span class="bigo-cs-badge bigo-numerical" title="${esc(f.recursion_shape_algorithm_note ?? '')}">Fibonacci?</span>` : ''}
        ${f.recursion_shape_algorithm === 'memoized_fibonacci' ? `<span class="bigo-cs-badge bigo-numerical" title="${esc(f.recursion_shape_algorithm_note ?? '')}">Fibonacci memoizado?</span>` : ''}
        ${f.memoization ? `<span class="bigo-cs-badge bigo-purity" title="${esc(f.memoization_note ?? '')}">memoizado</span>` : ''}
        ${f.is_pure ? `<span class="bigo-cs-badge bigo-purity" title="${esc(f.purity_note ?? '')}">pure</span>` : ''}
        ${f.vectorization_note ? `<span class="bigo-cs-badge bigo-vectorization" title="${esc(f.vectorization_note)}">SIMD?</span>` : ''}
        ${f.numerical_algorithm_note ? `<span class="bigo-cs-badge bigo-numerical" title="${esc(f.numerical_algorithm_note)}">Matrix?</span> ${_renderMatmulValidationControl()}` : ''}
        ${f.blas_lapack_calls?.length ? `<span class="bigo-cs-badge bigo-blas" title="${esc(`Detected BLAS/LAPACK calls: ${f.blas_lapack_calls.join(', ')}`)}">BLAS/LAPACK</span>` : ''}
      </td>
      <td><span class="bigo-badge" style="color:${color};border-color:${color}"${f.combinatorics_note ? ` title="${esc(f.combinatorics_note)}"` : ''}>${esc(f.big_o)}</span></td>
      <td class="bigo-thetaomega">${f.big_o_theta ? esc(f.big_o_theta) : '—'} / ${f.big_o_omega ? esc(f.big_o_omega) : '—'}</td>
      <td class="bigo-reason">${esc(f.big_o_reason)}</td>
      <td class="bigo-space">${f.space_complexity ? `<span class="bigo-badge" style="color:${BIG_O_COLOR[f.space_complexity] ?? 'var(--muted)'};border-color:${BIG_O_COLOR[f.space_complexity] ?? 'var(--muted)'}" title="${esc(f.space_reason ?? '')}">${esc(f.space_complexity)}</span>` : '—'}</td>
      <td class="bigo-cc" style="color:${_ccColor(f.complexity)}">${f.complexity ?? '—'}</td>
      <td class="bigo-loc">${f.loc ?? '—'}</td>
      <td class="bigo-line">:${f.line}</td>
    </tr>`
    })
    .join('')

  return `
  <div class="metric-section">
    <div class="ms-title">Algorithm Complexity — Big O</div>
    <details class="bigo-notation-ref">
      <summary>Notación asintótica — referencia</summary>
      <table class="bigo-notation-table">
        <tbody>
          <tr><td><code>O</code></td><td>Cota superior (peor caso)</td><td>El caso más lento que Sythrall detecta</td></tr>
          <tr><td><code>Θ</code></td><td>Cota ajustada — mismo orden en el mejor y el peor caso</td><td>Se infiere comparando O y Ω</td></tr>
          <tr><td><code>Ω</code></td><td>Cota inferior (mejor caso)</td><td>El caso más rápido que Sythrall detecta (ej. salida temprana)</td></tr>
          <tr><td><code>o</code></td><td>Cota superior <em>estricta</em> — crece más rápido, nunca al mismo orden</td><td>No calculado — sin heurística estática confiable para distinguirlo de O</td></tr>
          <tr><td><code>ω</code></td><td>Cota inferior <em>estricta</em></td><td>No calculado — mismo motivo</td></tr>
        </tbody>
      </table>
    </details>
    <details class="ref-details">
      <summary>Jerarquía de Chomsky — referencia</summary>
      <table class="ref-table">
        <thead><tr><th>Tipo</th><th>Autómata</th><th>¿Sythrall lo detecta?</th></tr></thead>
        <tbody>
          <tr><td><code>Type-3</code> Regular</td><td>Autómata Finito</td><td><span class="bigo-cs-badge bigo-regex">Regex</span> — uso de <code>re</code> en el cuerpo</td></tr>
          <tr><td><code>Type-2</code> Libre de Contexto</td><td>Autómata de Pila</td><td><span class="bigo-cs-badge bigo-grammar">CFG</span> — forma de parser/gramática (pila + recursión)</td></tr>
          <tr><td><code>Type-1</code> Sensible al Contexto</td><td>Autómata Limitado Lineal</td><td><span class="bigo-cs-badge bigo-semantic">Type-1?</span> — informal: patrón clásico de análisis semántico (tabla de símbolos que crece + rechazo por contexto). No es una clasificación formal probada.</td></tr>
          <tr><td><code>Type-0</code> Recursivamente Enumerable</td><td>Máquina de Turing</td><td>Sin badge por función — cómputo sin restricciones es el caso por defecto de cualquier lenguaje de propósito general (Python ya es Turing-completo), no un patrón puntual que detectar.</td></tr>
        </tbody>
      </table>
    </details>
    <div class="table-scroll">
      <table class="bigo-table">
        <thead><tr>
          <th>Función</th><th title="Peor caso">Big O</th>
          <th title="Cota ajustada (Θ) / Mejor caso (Ω) — ver referencia arriba">Θ / Ω</th>
          <th>Razón</th>
          <th class="bigo-space" title="Espacio auxiliar — estructuras creadas, no solo tiempo de ejecución (Fase 13)">Space</th>
          <th title="Complejidad ciclomática McCabe">CC</th>
          <th title="Líneas de código">LOC</th>
          <th>Línea</th>
        </tr></thead>
        <tbody>${rows}</tbody>
      </table>
    </div>
  </div>`
}

/** Fase 23 — control inline junto al badge "Matrix?": botón para disparar la
 * validación empírica, spinner mientras corre, o el exponente medido una vez
 * que hay resultado. Compartido entre todas las filas con
 * `numerical_algorithm_note` — el kernel es genérico (Sythrall lo escribe,
 * no el código del usuario), así que una sola corrida sirve para todas. */
function _renderMatmulValidationControl(): string {
  if (_matmulValidating) {
    return `<span class="bigo-cs-badge bigo-empirical"><span class="up-spinner"></span> Validando...</span>`
  }
  if (_matmulValidation) {
    const v = _matmulValidation
    if (!v.available || v.estimated_exponent == null) {
      return `<span class="bigo-cs-badge bigo-empirical bigo-empirical-off" title="${esc(v.note)}">sin validar</span>`
    }
    const close = Math.abs(v.estimated_exponent - 3) < 0.3
    return `<span class="bigo-cs-badge bigo-empirical ${close ? 'bigo-empirical-ok' : 'bigo-empirical-off'}" title="${esc(v.note)}">medido: n^${v.estimated_exponent.toFixed(2)}</span> <button class="btn btn-ghost btn-sm" data-validate-matmul title="Volver a correr la validación">↺</button>`
  }
  return `<button class="btn btn-ghost btn-sm" data-validate-matmul title="Compila y corre Fortran real (escrito por Sythrall, no tu código) para medir el exponente de crecimiento real">Validar empíricamente</button>`
}

// ─── Compiler pipeline — Compiler Explorer embebido (Fase 17) ─────────────────
//
// Primer bullet de la Fase 17 (Compiler Intelligence, hasta ahora un
// research spike sin comprometer): en vez de construir un compilador
// educativo propio (Lexer→AST→IR→optimización→codegen), se embebe
// Compiler Explorer (godbolt.org, open source) — la misma herramienta que
// cualquier ingeniero de sistemas ya usa para esto. Confirmado que
// `godbolt.org` no envía `X-Frame-Options`/`frame-ancestors` (permite ser
// embebido).
//
// **Ruta elegida tras un intento fallido, no adivinado**: la ruta
// documentada `/clientstate/<base64 de {sessions:[{language, source,
// compilers:[{id}]}]}>` SÍ precarga el código real — confirmado en vivo
// cargándola con un nombre de función distintivo y comparando contra el
// HTML devuelto. La ruta "sin chrome" `/e#clientstate:<mismo base64>` (sin
// barra superior/sponsors/modal de privacidad, la que este comentario
// documentaba originalmente) se probó primero por dar una UI más liviana,
// pero un segundo chequeo con un nombre de función distintivo reveló que
// IGNORA el `source` del payload y siempre muestra su ejemplo por defecto
// (`int square(int num)`) — el chequeo inicial ("¿aparece 'square' en la
// página?") daba un falso positivo porque ese es justo el nombre del
// ejemplo por defecto. Corregido antes de shippear: se usa `/clientstate/`
// (con el chrome completo del sitio, incluyendo el modal de política de
// privacidad la primera vez que se visita desde un perfil de navegador
// nuevo) porque cargar el código equivocado en silencio es peor que un
// chrome más pesado.
//
// Deliberadamente limitado a `c`/`cpp`/`fortran` — los 3 lenguajes que este
// motor ya compila de verdad en otras fases (kernels de Fase 23/26) — no a
// `assembly`/Python/JS/TS, donde "pipeline de COMPILADOR" no aplica de la
// misma forma (Assembly ya es el resultado, Python/JS no tienen un
// `codegen` nativo comparable). IDs de compilador fijados a los alias
// "snapshot"/"nightly" de cada lenguaje (siempre resuelven a la versión más
// reciente disponible) en vez de una versión numerada, para no quedar
// apuntando a un compilador ya removido del servicio con el tiempo.
//
// **Consentimiento explícito, no automático**: a diferencia de cualquier
// otra sección de este panel, cargar esto envía el código fuente COMPLETO
// del archivo a un servicio de terceros (godbolt.org) — el primer caso de
// esta naturaleza en todo el motor, que hasta acá es 100% local (Rust
// sidecar) o Sythrall-authored (kernels de Fase 23/26, nunca código del
// usuario). Por eso el iframe NO se arma en el HTML inicial — solo un botón
// con la URL ya calculada en un `data-*`, y un solo click la inyecta.
const GODBOLT_LANG_MAP: Record<string, { lang: string; compiler: string }> = {
  c: { lang: 'c', compiler: 'cgsnapshot' },
  cpp: { lang: 'c++', compiler: 'gsnapshot' },
  fortran: { lang: 'fortran', compiler: 'gfortransnapshot' },
}

// Límite arbitrario pero generoso — un archivo real de este tamaño ya sería
// inusual, y evita armar una URL de varios cientos de KB para un caso límite
// que además Compiler Explorer probablemente tardaría mucho en compilar.
const GODBOLT_MAX_SOURCE_CHARS = 20_000

/** Base64 seguro para UTF-8 -- btoa solo entiende Latin1 y rompe (o
 * corrompe en silencio) con cualquier caracter fuera de ese rango, ej. un
 * comentario en espanol con tildes/enies dentro del codigo fuente -- primero
 * se codifica a bytes UTF-8 con TextEncoder. */
function _utf8ToBase64(s: string): string {
  const bytes = new TextEncoder().encode(s)
  let binary = ''
  for (const b of bytes) binary += String.fromCharCode(b)
  return btoa(binary)
}

function _buildGodboltUrl(language: string, source: string, options: string): string | null {
  const mapping = GODBOLT_LANG_MAP[language]
  if (!mapping) return null
  const clientState = {
    sessions: [{ id: 1, language: mapping.lang, source, compilers: [{ id: mapping.compiler, options }] }],
  }
  const b64 = _utf8ToBase64(JSON.stringify(clientState))
  return `https://godbolt.org/clientstate/${b64}`
}

/** `hasTailRecursive` (Fase 17, segundo bullet, 2026-09-02): cuando el
 * archivo tiene al menos una función `is_tail_recursive` (`cparse.rs`), el
 * botón pide `-O2` en vez de dejar el nivel de optimización por defecto del
 * compilador (típicamente `-O0`, que NUNCA hace sibling call optimization)
 * — sin esto, el usuario clickearía esperando confirmar la reescritura a
 * loop que `recursion_note` menciona y no vería nada, pareciendo que la
 * nota mentía cuando en realidad faltaba pedir la optimización que la
 * habilita. Mismo criterio "mostrar, no solo afirmar" que motiva este
 * bullet completo. */
function _renderCompilerExplorer(language: string, source: string, hasTailRecursive: boolean): string {
  if (!source) return ''
  if (source.length > GODBOLT_MAX_SOURCE_CHARS) {
    return `
    <div class="metric-section">
      <div class="ms-title">Compiler pipeline (Compiler Explorer) — Fase 17</div>
      <div class="st-mem-note">Archivo demasiado grande (${source.length.toLocaleString('es')} caracteres, límite ${GODBOLT_MAX_SOURCE_CHARS.toLocaleString('es')}) para enviar a este visualizador externo.</div>
    </div>`
  }
  const options = hasTailRecursive ? '-O2' : ''
  const url = _buildGodboltUrl(language, source, options)
  if (!url) return ''
  const tailHint = hasTailRecursive
    ? ' Este archivo tiene al menos una función tail-call (ver el badge "tail-call" en la tabla Big-O) — cargado con <code>-O2</code> para poder confirmar en el ensamblador si el compilador la redujo a un loop (sin instrucción <code>call</code> a sí misma) o no.'
    : ''
  const label = hasTailRecursive ? 'Cargar Compiler Explorer (-O2, confirmar tail-call)' : 'Cargar Compiler Explorer'
  return `
  <div class="metric-section st-godbolt-section">
    <div class="ms-title">Compiler pipeline (Compiler Explorer) — Fase 17</div>
    <div class="st-mem-note">Herramienta externa (<a href="https://godbolt.org" target="_blank" rel="noopener noreferrer">godbolt.org</a>, no de Sythrall) — Lexer→AST→IR→optimización→codegen real para este archivo. Al cargarla, el código fuente completo se envía a ese servicio para compilarlo — no se carga sola.${tailHint}</div>
    <button class="btn btn-sm" type="button" data-load-godbolt data-godbolt-url="${esc(url)}">${label}</button>
  </div>`
}

/** Reemplaza el botón de consentimiento por el iframe real — recién en este
 * click se construye el `<iframe>` (nunca antes, ver el hedge de arriba
 * sobre no cargarlo solo). La URL ya viene calculada en el `data-*` del
 * botón, así que este handler no necesita `_resultSource` de nuevo. Un link
 * "abrir en una pestaña nueva" queda como respaldo si el iframe no carga
 * (red restringida, bloqueador de contenido, etc.). */
function _onLoadGodboltClick(target: HTMLElement): void {
  const url = target.dataset['godboltUrl']
  const container = target.closest('.st-godbolt-section')
  if (!url || !container) return
  container.innerHTML = `
    <div class="ms-title">Compiler pipeline (Compiler Explorer) — Fase 17</div>
    <iframe src="${esc(url)}" style="width:100%;height:640px;border:none;border-radius:6px;background:#1e1e1e" loading="lazy" title="Compiler Explorer"></iframe>
    <div class="st-mem-note mt-1"><a href="${esc(url)}" target="_blank" rel="noopener noreferrer">Abrir en una pestaña nueva ↗</a> si no carga acá</div>`
}

/** Fábrica de handlers de click para "Validar/Comparar empíricamente" —
 * auditoría (2026-09-01): las 13 variantes (10 kernels + 3 comparaciones vs.
 * numpy/pandas/scikit-learn) tenían el mismo cuerpo de 11 líneas copiado y
 * pegado, cambiando solo qué variable de estado tocan y qué función de
 * `api.*` llaman — con 13 copias, agregar el próximo kernel significaba
 * copiar 13 líneas más para cambiar 3 tokens, con riesgo real de tocar la
 * variable de validación equivocada en el `finally` por un typo. `T`
 * cubre ambos shapes que existen hoy (`EmpiricalValidationResult` para los
 * 10 kernels, `MatmulVsNumpyResult`/`AggregationVsPandasResult`/
 * `LinregVsSklearnResult` para las 3 comparaciones) sin necesitar un tipo
 * unión — el control de flujo es idéntico en los 13 casos, solo cambia el
 * tipo de dato que viaja. */
function _makeValidateHandler<T>(
  getRunning: () => boolean,
  setRunning: (running: boolean) => void,
  setResult: (result: T) => void,
  apiCall: () => Promise<T>,
): () => Promise<void> {
  return async () => {
    if (getRunning()) return
    setRunning(true)
    if (_result) _renderResult(_result)
    try {
      setResult(await apiCall())
    } catch (e) {
      toast('Error: ' + (e as Error).message, 'err')
    } finally {
      setRunning(false)
      if (_result) _renderResult(_result)
    }
  }
}

const _onValidateMatmulClick = _makeValidateHandler(
  () => _matmulValidating,
  (v) => {
    _matmulValidating = v
  },
  (v) => {
    _matmulValidation = v
  },
  api.validateMatmulBigO,
)
const _onValidateTemplateCostClick = _makeValidateHandler(
  () => _tplCostRunning,
  (v) => {
    _tplCostRunning = v
  },
  (v) => {
    _tplCostResult = v
  },
  api.validateTemplateCost,
)

// ─── Fase 19 (Machine Intelligence) — Analizador de ejecutables ──────────────

/** Re-renderiza SOLO `#st-exe-result` — a diferencia de `_renderResult`
 * (que reescribe `#st-body` entero atado a un archivo de código fuente
 * seleccionado), esto no depende de ningún archivo cargado, así que tiene
 * su propio target de render en vez de mezclarse con ese flujo. */
function _repaintExeSection(): void {
  const el = document.getElementById('st-exe-result')
  if (el) el.innerHTML = _renderExeResult()
}

async function _onExeFileSelected(file: File): Promise<void> {
  _exeFileName = file.name
  _exeAnalyzing = true
  _exeResult = null
  _repaintExeSection()
  try {
    _exeResult = await api.parseExecutable(file)
    if (!_exeResult.available) toast(_exeResult.note, 'warn')
  } catch (e) {
    toast('Error: ' + (e as Error).message, 'err')
  } finally {
    _exeAnalyzing = false
    _repaintExeSection()
  }
}

function _renderExeResult(): string {
  if (_exeAnalyzing) {
    return `<div class="metric-section"><div class="st-fn-head"><span class="bigo-cs-badge bigo-empirical"><span class="up-spinner"></span> Analizando "${esc(_exeFileName ?? '')}"...</span></div></div>`
  }
  if (!_exeResult) return ''
  const r = _exeResult
  if (!r.available) {
    return `<div class="metric-section"><div class="st-error">${icon('warning', 14)} ${esc(r.note)}</div></div>`
  }

  const importsByLib = new Map<string, string[]>()
  for (const imp of r.imports) {
    const list = importsByLib.get(imp.library) ?? []
    list.push(imp.name)
    importsByLib.set(imp.library, list)
  }

  return `
  <div class="metric-section">
    <div class="ms-title">Analizador de ejecutables — "${esc(_exeFileName ?? '')}" (Fase 19)</div>
    <div class="st-mem-note" title="${esc(r.note)}">Estructura binaria real (crate \`object\`, misma librería que rustc/lld/wasmtime) — no es disassembly de instrucciones.</div>
    <div class="st-summary-row">
      ${sc(r.format, 'Formato')}
      ${sc(r.architecture, 'Arquitectura')}
      ${sc(r.is_64_bit ? '64 bits' : '32 bits', 'Bits')}
      ${sc('0x' + r.entry_point.toString(16), 'Entry point')}
      ${sc(String(r.sections.length), 'Secciones')}
      ${sc(String(r.imports.length), 'Imports')}
    </div>
    ${
      r.sections.length
        ? `<div class="st-mem-region-title" style="margin-top:.6rem">Secciones</div>
    <div class="st-exe-scroll">
      ${r.sections.map((s) => `<div class="st-mem-var"><span class="st-mem-var-name">${esc(s.name)}</span><span class="st-mem-var-type">${esc(s.kind)}</span><span class="st-mem-var-scope">${s.size.toLocaleString()} bytes @ 0x${s.address.toString(16)}</span></div>`).join('')}
    </div>`
        : ''
    }
    ${
      importsByLib.size
        ? `<div class="st-mem-region-title" style="margin-top:.6rem">Imports, agrupados por librería (${importsByLib.size})</div>
    <div class="st-exe-scroll">
      ${[...importsByLib.entries()]
        .map(
          ([lib, names]) =>
            `<div class="st-mem-var"><span class="st-mem-var-name">${esc(lib)}</span><span class="st-mem-var-type">${names.length} símbolo${names.length === 1 ? '' : 's'}</span><span class="st-mem-var-scope" style="flex-basis:100%">${names.map(esc).join(', ')}</span></div>`,
        )
        .join('')}
    </div>`
        : ''
    }
    ${
      r.exports.length
        ? `<div class="st-mem-region-title" style="margin-top:.6rem">Exports (${r.exports.length})</div>
    <div class="st-exe-scroll">
      ${r.exports.map((e) => `<div class="st-mem-var"><span class="st-mem-var-name">${esc(e.name)}</span><span class="st-mem-var-scope">@ 0x${e.address.toString(16)}</span></div>`).join('')}
    </div>`
        : ''
    }
    ${
      r.symbols.length
        ? `<div class="st-mem-region-title" style="margin-top:.6rem">Símbolos (${r.symbols.length})</div>
    <div class="st-exe-scroll">
      ${r.symbols
        .map(
          (s) =>
            `<div class="st-mem-var"><span class="st-mem-var-name">${esc(s.name)}</span><span class="st-mem-var-type">${esc(s.kind)}</span><span class="st-mem-var-scope">${s.is_undefined ? 'indefinido' : 'definido'} · ${s.is_global ? 'global' : 'local'} @ 0x${s.address.toString(16)}</span></div>`,
        )
        .join('')}
    </div>`
        : '<div class="empty" style="padding:.5rem 0">Sin símbolos — normal en un binario release/stripped.</div>'
    }
    ${r.truncated ? `<div class="st-wasm-rec" style="margin-top:.5rem">Algunas listas se cortaron — ver la nota completa para el tope exacto.</div>` : ''}
  </div>`
}

function _renderFunctions(functions: ParsedFunction[]): string {
  if (!functions.length) return ''

  return `
  <div class="metric-section">
    <div class="ms-title">Funciones (${functions.length})</div>
    ${functions
      .map((f) => {
        const color = BIG_O_COLOR[f.big_o] ?? 'var(--muted)'
        return `<div class="st-fn-item">
        <div class="st-fn-head">
          ${f.is_async ? '<span class="st-async-badge">async</span>' : ''}
          <span class="st-fn-name">${esc(f.name)}</span>
          ${f.args?.length ? `<span class="st-fn-args">(${f.args.map(esc).join(', ')})</span>` : '<span class="st-fn-args">()</span>'}
          <span class="st-fn-line">línea ${f.line}${f.end_line ? `–${f.end_line}` : ''}</span>
          <span style="margin-left:auto;font-family:var(--mono);font-size:.65rem;color:${color}">${esc(f.big_o)}</span>
        </div>
        <div class="st-fn-meta">
          <span>CC: <b style="color:${_ccColor(f.complexity)}">${f.complexity ?? '—'}</b></span>
          ${f.loc ? `<span>LOC: ${f.loc}</span>` : ''}
          ${f.decorators?.length ? `<span>@${f.decorators.map(esc).join(' @')}</span>` : ''}
          ${f.calls?.length ? `<span>llama: ${f.calls.slice(0, 4).map(esc).join(', ')}${f.calls.length > 4 ? '…' : ''}</span>` : ''}
        </div>
      </div>`
      })
      .join('')}
  </div>`
}

/** **Las firmas sin cuerpo, que hasta hoy no se mostraban en ningun lado.**
 *
 * Un `.ads` de Ada es pura interfaz: cero cuerpos y N firmas. Si esta seccion
 * no existiera, ese archivo se veria como "0 funciones" y nada mas — que es
 * justo lo contrario de lo que dice: declara toda la interfaz del paquete.
 *
 * El campo `exports` existia en el shape y lo llenaban Ada **y JS/TS**, y el
 * panel no lo leia: los exports de un modulo JS tampoco se veian. Se arregla
 * para los dos a la vez, no solo para el que lo destapo. */
function _renderExports(exports: StaticResult['exports'], lang: string): string {
  const title =
    lang === 'ada' ? `Firmas sin cuerpo \u2014 interfaz del paquete (${exports.length})` : `Exports (${exports.length})`
  return `
  <div class="metric-section">
    <div class="ms-title">${title}</div>
    <div class="st-import-grid">
      ${exports
        .map(
          (e) => `<div class="st-import-chip">
          ${e.kind ? `<span class="st-import-type">${esc(e.kind)}</span>` : ''}
          <span>${esc(e.name)}</span>
          <span class="st-fn-line">l\u00ednea ${e.line}</span>
          ${e.params ? `<span class="meta">${e.params} par\u00e1m.</span>` : ''}
        </div>`,
        )
        .join('')}
    </div>
  </div>`
}

function _renderClasses(classes: ParsedClass[], lang: string): string {
  // Rust no tiene clases: tiene tipos, y los metodos se los agregan bloques
  // `impl` aparte. Llamarlos "Clases" seria el mismo error que este panel ya
  // evita con C ("Structs/Unions").
  const title =
    lang === 'c'
      ? 'Structs/Unions'
      : lang === 'cpp'
        ? 'Clases C++'
        : lang === 'rust'
          ? 'Tipos (struct / enum / trait)'
          : lang === 'ada'
            ? 'Unidades (paquetes / tareas / tipos protegidos)'
            : 'Clases'
  return `
  <div class="metric-section">
    <div class="ms-title">${title} (${classes.length})</div>
    ${classes
      .map(
        (c) => `
      <div class="st-class-item">
        <div class="st-fn-head">
          <span class="st-fn-name">${esc(c.name)}</span>
          ${c.bases?.length ? `<span style="font-size:.68rem;color:var(--muted)">extends ${c.bases.map(esc).join(', ')}</span>` : ''}
          <span class="st-fn-line">línea ${c.line}</span>
          ${c.kind ? `<span class="meta">${c.kind}</span>` : ''}
          ${c.data_structure ? `<span class="bigo-cs-badge bigo-datastruct" title="${esc(c.data_structure_note ?? '')}">${esc(c.data_structure)}</span>` : ''}
        </div>
        ${
          c.methods?.length
            ? `
          <div class="st-fn-meta">
            Métodos: ${c.methods.map((m) => `<span class="st-method-chip">${esc(m.name)}</span>`).join('')}
          </div>`
            : ''
        }
      </div>
    `,
      )
      .join('')}
  </div>`
}

function _renderImports(imports: ParsedImport[], dead: StaticResult['dead_code']): string {
  return `
  <div class="metric-section">
    <div class="ms-title">Imports (${imports.length})
      ${dead.length ? `<span style="color:var(--warn);font-size:.65rem;margin-left:8px">${icon('warning', 12)} ${dead.length} no usados</span>` : ''}
    </div>
    <div class="st-import-grid">
      ${imports
        .map((imp) => {
          const isDeadI = dead.some((d) => d.module === imp.module)
          return `<div class="st-import-chip ${isDeadI ? 'st-import-dead' : ''}">
          <span class="st-import-type">${esc((imp.type ?? 'import').replace('_import', '').replace('esm_', ''))}</span>
          <span>${esc(imp.module)}</span>
          ${imp.name ? `<span class="st-import-name">→ ${esc(imp.name)}</span>` : ''}
          ${imp.alias ? `<span class="st-import-name">as ${esc(imp.alias)}</span>` : ''}
          <span class="st-import-line">:${imp.line}</span>
          ${isDeadI ? '<span class="st-unused-badge">no usado</span>' : ''}
        </div>`
        })
        .join('')}
    </div>
  </div>`
}

function _renderInterfaces(
  interfaces: Array<{ name: string; line: number }>,
  types: Array<{ name: string; line: number }>,
): string {
  return `
  <div class="metric-section">
    <div class="ms-title">TypeScript — Interfaces & Types</div>
    <div class="st-import-grid">
      ${interfaces
        .map(
          (i) =>
            `<div class="st-import-chip" style="border-color:var(--info)">
          <span class="st-import-type">interface</span>
          <span class="text-info">${esc(i.name)}</span>
          <span class="st-import-line">:${i.line}</span>
        </div>`,
        )
        .join('')}
      ${types
        .map(
          (t) =>
            `<div class="st-import-chip" style="border-color:var(--purple)">
          <span class="st-import-type">type</span>
          <span class="text-purple">${esc(t.name)}</span>
          <span class="st-import-line">:${t.line}</span>
        </div>`,
        )
        .join('')}
    </div>
  </div>`
}

function _renderCallGraph(edges: Array<{ from: string; to: string }>): string {
  return `
  <div class="metric-section">
    <div class="ms-title">Call Graph (${edges.length} conexiones)</div>
    <div class="st-callgraph">
      ${edges
        .map(
          (e) =>
            `<div class="st-cg-edge">
          <span class="st-cg-caller">${esc(e.from)}</span>
          <span class="st-cg-arrow">→</span>
          <span class="st-cg-callee">${esc(e.to)}</span>
        </div>`,
        )
        .join('')}
    </div>
  </div>`
}

function _renderWasmHints(hints: WasmHint[]): string {
  const priorityLabel = (p: number) =>
    p >= 5 ? ['Crítico', 'var(--err)'] : p >= 3 ? ['Alto', 'var(--orange)'] : ['Medio', 'var(--warn)']

  return `
  <div class="metric-section">
    <div class="ms-title">WASM / Cython — Hot Paths (${hints.length})</div>
    ${hints
      .map((h) => {
        const [label, color] = priorityLabel(h.priority)
        return `<div class="st-wasm-item">
        <div class="st-fn-head">
          <span class="st-fn-name">${esc(h.function)}</span>
          <span class="badge badge-tinted" style="--tag-color:${color}">${label}</span>
          <span class="st-fn-line">línea ${h.line}</span>
          <span style="margin-left:auto;font-size:.65rem;color:var(--ok)">${esc(h.estimated_speedup)}</span>
        </div>
        <ul class="st-wasm-reasons">
          ${h.reasons.map((r) => `<li>${esc(r)}</li>`).join('')}
        </ul>
        <div class="st-wasm-rec">${esc(h.recommendation)}</div>
      </div>`
      })
      .join('')}
  </div>`
}

const MEM_REGION_COLOR: Record<string, string> = {
  stack: 'var(--info)',
  data: 'var(--ok)',
  bss: 'var(--muted)',
}
const MEM_REGION_LABEL: Record<string, string> = {
  stack: 'Stack',
  data: 'Data',
  bss: 'BSS',
}

/** Fase 23 — vista de lista agrupada por región, no un diagrama de cajas y
 * flechas (ese nivel de visualización es la Fase 25, "Execution Path
 * Simulator", una pieza aparte). `heap` no es una región de variable acá —
 * ver el doc de `memlayout.rs`: un puntero vive en el stack, la memoria a la
 * que apunta se lista en `allocations`. */
/** Fase 34, Línea A — métricas de FORMA de la metaprogramación de C++.
 *
 * **El subtítulo no es decoración: es la mitad del hallazgo.** Estos números
 * dicen cuánta metaprogramación hay ESCRITA, no cuánto trabajo le dan al
 * compilador. Presentarlos sin esa aclaración invitaría exactamente a la
 * lectura equivocada — "1.842 sitios" suena a una predicción de costo y no lo
 * es, porque el compilador deduplica lo repetido y expande lo recursivo. */
/** La mitad MEDIDA, debajo de las cuentas estáticas. Es lo que cierra la
 * promesa del panel: arriba dice que saber el costo real exige compilar, acá
 * está el botón que lo compila.
 *
 * **Las dos columnas se leen distinto y el texto lo dice.** Los bytes de
 * objeto son determinísticos y crecen linealmente con la profundidad (un
 * nivel = una función emitida); los milisegundos, a estas profundidades,
 * están dominados por el arranque del compilador y casi no se mueven. Poner
 * los dos números sin esa aclaración invitaría a leer el tiempo como si
 * midiera los templates. */

/** **El segundo punto de datos, al lado del primero (Fase 34, Línea C).**
 *
 * La curva de C++ sola no puede distinguir *"así cuesta la
 * metaprogramación"* de *"así cuesta en C++"*. Medido, C++ emite 3 símbolos
 * por nivel y D emite 6 — el costo por nivel es **del lenguaje**. Lo que sí
 * generaliza es la forma: las dos curvas son lineales y determinísticas.
 *
 * **El veredicto va arriba de las tablas, no abajo.** Es la conclusión que el
 * módulo existe para producir; dejarla al final la convertiría en un pie de
 * página de dos tablas de números. */
function _renderComparacionMeta(): string {
  if (_metaCompRunning) {
    return `<div class="st-mem-note mt-3"><span class="up-spinner"></span> Compilando las dos sondas a 4 profundidades cada una…</div>`
  }
  const r = _metaCompResult
  if (!r) {
    return `<div class="mt-3"><button class="btn btn-ghost btn-sm" data-compare-meta title="Compila la misma sonda en C++ y en D a las mismas profundidades y compara cuánto cuesta cada nivel. No toca tu código.">Comparar con D</button></div>`
  }
  if (!r.comparable) {
    return `<div class="st-mem-note mt-3">${esc(r.note)}</div>`
  }

  const tabla = (titulo: string, c: TemplateCostResult | DCostResult) => `
    <div class="mc-grupo" style="flex-direction:column;align-items:stretch;flex:1;min-width:220px">
      <div class="settings-row-label mb-1">${esc(titulo)}${c.compiler ? ` · ${esc(c.compiler)}` : ''}</div>
      <table class="st-tpl-cost">
        <thead><tr><th>Prof.</th><th>Símbolos</th><th>Objeto</th><th>Tiempo</th></tr></thead>
        <tbody>${c.points
          .map(
            (pt) => `<tr>
            <td class="mono">${pt.depth}</td>
            <td class="mono">${pt.symbols}</td>
            <td class="mono">${(pt.object_bytes / 1024).toFixed(1)} KB</td>
            <td style="font-family:var(--mono);color:var(--muted)">${pt.compile_ms} ms</td>
          </tr>`,
          )
          .join('')}</tbody>
      </table>
    </div>`

  const cpp = r.cpp_symbols_per_level
  const d = r.d_symbols_per_level
  const difieren = cpp != null && d != null && cpp !== d

  return `
  <div class="mt-3">
    <div class="mc-pareto-resumen">
      ${
        difieren
          ? `C++ emite <strong>${cpp}</strong> símbolo${cpp === 1 ? '' : 's'} por nivel y D emite <strong>${d}</strong>:
             el costo por nivel es <strong>del lenguaje</strong>, no de la metaprogramación en general.
             Lo que sí generaliza es la <strong>forma</strong> — ${r.ambas_lineales ? 'las dos curvas son lineales y determinísticas' : 'ver las curvas abajo'}.`
          : `C++ y D emiten <strong>${cpp}</strong> símbolo${cpp === 1 ? '' : 's'} por nivel: el costo por nivel
             coincide, así que sería del <strong>mecanismo</strong> y no del lenguaje. Vale la pena revisarlo con un tercer punto.`
      }
    </div>
    <div class="mc-controles" style="align-items:flex-start">
      ${tabla('C++', r.cpp)}
      ${tabla('D', r.d)}
    </div>
    <div class="mc-aviso">${esc(r.note)}</div>
    <div class="mt-2"><button class="btn btn-ghost btn-sm" data-compare-meta title="Volver a medir">↺ Volver a medir</button></div>
  </div>`
}

async function _compararMeta(): Promise<void> {
  if (_metaCompRunning) return
  _metaCompRunning = true
  _repintarResultado()
  try {
    const res = await api.compareMetaprogramming()
    _metaCompResult = res
  } catch (e) {
    toast('Error: ' + (e instanceof Error ? e.message : ''), 'err')
  } finally {
    _metaCompRunning = false
    _repintarResultado()
  }
}

function _renderTemplateCost(): string {
  if (_tplCostRunning) {
    return `<div class="st-mem-note mt-3"><span class="up-spinner"></span> Compilando la sonda a 4 profundidades…</div>`
  }
  const r = _tplCostResult
  if (!r) {
    return `<div class="mt-3"><button class="btn btn-ghost btn-sm" data-validate-template-cost title="Compila una sonda que Sythrall escribe (nunca tu código) a varias profundidades y mide cuánto cuesta cada nivel en esta máquina">Medir costo de compilación</button></div>`
  }
  if (!r.available) {
    return `<div class="st-mem-note mt-3">${esc(r.note)}</div>`
  }
  const filas = r.points
    .map(
      (pt) => `<tr>
      <td class="mono">${pt.depth}</td>
      <td class="mono">${pt.symbols}</td>
      <td class="mono">${(pt.object_bytes / 1024).toFixed(1)} KB</td>
      <td style="font-family:var(--mono);color:var(--muted)">${pt.compile_ms} ms</td>
    </tr>`,
    )
    .join('')
  return `
  <div class="mt-3">
    <div class="settings-row-label mb-1">Medido en esta máquina${r.compiler ? ` · ${esc(r.compiler)}` : ''}</div>
    <div class="st-mem-note mb-2">${esc(r.note)}</div>
    <table class="st-tpl-cost">
      <thead><tr><th>Profundidad</th><th title="Símbolos que el compilador emitió de verdad, leídos de la tabla del objeto">Símbolos</th><th>Objeto</th><th>Tiempo</th></tr></thead>
      <tbody>${filas}</tbody>
    </table>
    ${
      r.symbols_per_level
        ? `<div class="st-mem-note mt-2"><strong>Cada nivel de instanciación emite ${r.symbols_per_level} símbolo${r.symbols_per_level === 1 ? '' : 's'}</strong>${r.bytes_per_level ? ` y ~${r.bytes_per_level} bytes de código objeto` : ''} acá. <strong>La cuenta de símbolos NO es estáticamente calculable</strong> —saberla exige instanciar, o sea compilar—, así que está medida, no estimada. Y es la más portable de las dos: un entero chico que significa lo mismo en otra máquina, mientras que los bytes por nivel dependen del ABI. Con esto, la profundidad que el análisis estático encontró se ubica sobre una curva real en vez de quedar como un número suelto.</div>`
        : r.bytes_per_level
          ? `<div class="st-mem-note mt-2">Cada nivel de instanciación cuesta <strong>~${r.bytes_per_level} bytes</strong> de código objeto acá.</div>`
          : ''
    }
    <div class="mt-2"><button class="btn btn-ghost btn-sm" data-validate-template-cost title="Volver a medir">↺ Volver a medir</button></div>
  </div>`
}

function _renderTemplateComplexity(t: TemplateComplexity): string {
  const celda = (valor: number, etiqueta: string, ayuda: string) => `
    <div class="st-stat" title="${esc(ayuda)}">
      <div class="st-stat-val">${valor}</div>
      <div class="st-stat-lbl">${esc(etiqueta)}</div>
    </div>`
  return `
  <div class="metric-section">
    <div class="ms-title">Metaprogramación — forma (${t.templates} plantilla${t.templates === 1 ? '' : 's'})</div>
    <div class="st-mem-note">Cuentas de lo que está <strong>escrito</strong>, no del trabajo del compilador. <code>Sitios</code> sobrecuenta (el compilador deduplica lo repetido) y <code>Distintos</code> subcuenta (una plantilla recursiva es un texto y muchas instanciaciones). Saber el costo real —símbolos, memoria, tiempo de build— exige compilar, y eso es otra cosa que esto no hace.</div>
    <div class="st-summary-row">
      ${celda(t.templates, 'Plantillas', 'Declaraciones de template primarias')}
      ${celda(t.specializations, 'Especializaciones', 'Totales o parciales — cada una es una rama por tipo')}
      ${celda(t.instantiation_sites, 'Sitios', 'Cada `X<...>` escrito en el archivo')}
      ${celda(t.distinct_instantiations, 'Distintos', 'Los sitios deduplicados por texto')}
      ${celda(t.constexpr_branches, 'if constexpr', 'Bifurcaciones resueltas en compilación')}
      ${celda(t.static_asserts, 'static asserts', 'Evaluación obligatoria en compilación')}
      ${celda(t.concepts_defined ?? 0, 'concepts', 'Concepts propios definidos (C++20) — la condición ES la firma')}
      ${celda(t.requires_clauses ?? 0, 'requires', 'Cláusulas `requires` (C++20)')}
      ${celda(t.enable_if_sites ?? 0, 'enable_if', 'La forma pre-C++20: la condición viaja escondida y el error al fallar no la menciona')}
    </div>
    ${
      t.recursive_templates.length
        ? `<div class="st-mem-note mt-2">Se instancian a sí mismas: ${t.recursive_templates
            .map((n: string) => `<code>${esc(n)}</code>`)
            .join(', ')} — ahí es donde lo escrito y lo generado más se separan.</div>`
        : ''
    }
    ${_renderTemplateCost()}

    <!-- Fase 34, Linea C: el segundo punto de datos, al lado del primero -->
    ${_renderComparacionMeta()}
  </div>`
}

/** Las tres variantes comparten concepto pero no campos: cada lenguaje
 * descarta pruebas con su propia sintaxis. Se discrimina por un campo
 * exclusivo de cada una, no por un campo `language` que habría que mantener
 * sincronizado en tres lugares. */
function _renderProofSurfaceAny(p: ProofSurface | ProofSurfaceTs | ProofSurfacePy): string {
  if ('unwrap_calls' in p) return _renderProofSurface(p)
  if ('bare_type_ignores' in p) return _renderProofSurfacePy(p)
  return _renderProofSurfaceTs(p)
}

function _renderProofSurfacePy(p: ProofSurfacePy): string {
  const total = p.bare_type_ignores + p.casts + p.any_annotations
  const celda = (valor: number, etiqueta: string, ayuda: string, color?: string) => `
    <div class="st-stat" title="${esc(ayuda)}">
      <div class="st-stat-val"${color ? ` style="color:${color}"` : ''}>${valor}</div>
      <div class="st-stat-lbl">${esc(etiqueta)}</div>
    </div>`

  if (total === 0 && p.coded_type_ignores === 0) {
    return `
  <div class="metric-section">
    <div class="ms-title">Superficie de prueba</div>
    <div class="st-mem-note">Este archivo no descarta ninguna prueba: <strong>sin <code>type: ignore</code>, sin <code>cast()</code> y sin <code>Any</code></strong>.</div>
  </div>`
  }

  return `
  <div class="metric-section">
    <div class="ms-title">Superficie de prueba (${total} punto${total === 1 ? '' : 's'})</div>
    <div class="st-mem-note">Dónde el programa descarta lo que el sistema de tipos sabía. <strong>La distinción que más vale es entre <code># type: ignore</code> pelado y <code># type: ignore[código]</code></strong>: el pelado calla <em>cualquier</em> error de esa línea para siempre —incluido uno distinto que aparezca años después por otro motivo—, mientras que el que nombra el código deja que mypy siga reportando todo lo demás. Ninguno está mal; la diferencia es que uno <strong>sigue avisando</strong>. Es el mismo par que <code>.expect("por qué")</code> / <code>.unwrap()</code> en Rust y <code>@ts-expect-error</code> / <code>@ts-ignore</code> en TypeScript.</div>
    <div class="st-summary-row">
      ${celda(p.bare_type_ignores, 'ignore pelado', 'Calla cualquier error de esa línea, para siempre', p.bare_type_ignores > 0 ? 'var(--warn)' : undefined)}
      ${celda(p.coded_type_ignores, 'ignore[código]', 'Nombra cuál calla, así que sigue avisando por los demás', 'var(--ok)')}
      ${celda(p.casts, 'cast()', 'Le dice al verificador "confiá en mí" sin verificar nada en runtime')}
      ${celda(p.any_annotations, 'Any', 'Descarta toda la información de tipo de una vez')}
    </div>
  </div>`
}

function _renderProofSurfaceTs(p: ProofSurfaceTs): string {
  const total = p.non_null_assertions + p.type_assertions + p.any_annotations + p.ts_ignore
  const celda = (valor: number, etiqueta: string, ayuda: string, color?: string) => `
    <div class="st-stat" title="${esc(ayuda)}">
      <div class="st-stat-val"${color ? ` style="color:${color}"` : ''}>${valor}</div>
      <div class="st-stat-lbl">${esc(etiqueta)}</div>
    </div>`

  if (total === 0 && p.ts_expect_error === 0) {
    return `
  <div class="metric-section">
    <div class="ms-title">Superficie de prueba</div>
    <div class="st-mem-note">Este archivo no descarta ninguna prueba: <strong>sin <code>as</code>, sin <code>!</code>, sin <code>any</code> y sin supresiones</strong>. Todo lo que el sistema de tipos sabía sigue disponible.</div>
  </div>`
  }

  return `
  <div class="metric-section">
    <div class="ms-title">Superficie de prueba (${total} punto${total === 1 ? '' : 's'})</div>
    <div class="st-mem-note">Dónde el programa le dice al compilador <em>"confiá en mí"</em> en vez de demostrárselo. Un tipo es una proposición, y cada una de estas formas descarta su prueba. <strong>La diferencia entre <code>@ts-ignore</code> y <code>@ts-expect-error</code> es la que más importa</strong>: el segundo <strong>falla cuando el error desaparece</strong>, así que obliga a sacarlo el día que deja de hacer falta; el primero calla para siempre y nunca avisa que ya sobra. Es la misma relación que hay en Rust entre <code>.expect("por qué")</code> y <code>.unwrap()</code>.</div>
    <div class="st-summary-row">
      ${celda(p.non_null_assertions, 'no-nulo !', 'Descarta la prueba de que podía ser null — el gemelo de `.unwrap()`', p.non_null_assertions > 0 ? 'var(--warn)' : undefined)}
      ${celda(p.type_assertions, 'as', 'Aserciones de tipo escritas')}
      ${celda(p.double_assertions, 'as unknown as', 'La escotilla doble: apaga el chequeo de compatibilidad del `as` normal', p.double_assertions > 0 ? 'var(--err)' : 'var(--ok)')}
      ${celda(p.any_annotations, 'any', 'Descarta toda la información de tipo de una vez')}
      ${celda(p.ts_ignore, '@ts-ignore', 'Calla el error para siempre y nunca avisa que ya sobra', p.ts_ignore > 0 ? 'var(--warn)' : undefined)}
      ${celda(p.ts_expect_error, '@ts-expect-error', 'La forma justificada: falla si el error desaparece', 'var(--ok)')}
    </div>
    ${
      p.double_assertions > 0
        ? `<div class="st-mem-note mt-2"><code>as unknown as T</code> <strong>no es un <code>as</code> más fuerte</strong>: es el que apaga el único chequeo que TypeScript todavía hace sobre una aserción. Un <code>as</code> normal exige que los dos tipos se solapen; pasar por <code>unknown</code> saca ese requisito, así que cualquier cosa puede asertarse a cualquier cosa. Es el equivalente de un <code>transmute</code>.</div>`
        : ''
    }
  </div>`
}

function _renderProofSurface(p: ProofSurface): string {
  const total = p.unwrap_calls + p.expect_calls
  const celda = (valor: number, etiqueta: string, ayuda: string, color?: string) => `
    <div class="st-stat" title="${esc(ayuda)}">
      <div class="st-stat-val"${color ? ` style="color:${color}"` : ''}>${valor}</div>
      <div class="st-stat-lbl">${esc(etiqueta)}</div>
    </div>`

  if (total === 0 && p.unchecked_casts === 0) {
    return `
  <div class="metric-section">
    <div class="ms-title">Superficie de prueba</div>
    <div class="st-mem-note">Este archivo no descarta ninguna prueba: <strong>sin <code>.unwrap()</code>, sin <code>.expect()</code> y sin conversiones <code>as</code></strong>. Todo lo que el sistema de tipos sabía sigue disponible.</div>
  </div>`
  }

  return `
  <div class="metric-section">
    <div class="ms-title">Superficie de prueba (${total} punto${total === 1 ? '' : 's'})</div>
    <div class="st-mem-note">El <strong>dual</strong> de la superficie de confianza. Aquella mide dónde el compilador <em>deja de chequear</em>; ésta, dónde el compilador <strong>sí sabía algo</strong> —que esto puede fallar, que este número puede no entrar— y el programa <strong>descartó esa información</strong>. Un tipo es una proposición y un valor suyo es su demostración: <code>Result</code> dice "esto puede fallar", y <code>.unwrap()</code> tira esa prueba sin dar motivo. <code>.expect("por qué")</code> tira la misma prueba pero deja escrito el razonamiento — la misma relación que hay entre un <code>unsafe</code> con <code>// SAFETY:</code> y uno sin él.</div>
    <div class="st-summary-row">
      ${celda(p.unwrap_calls, 'unwrap', 'Prueba descartada sin razón escrita', p.unwrap_calls > 0 ? 'var(--warn)' : undefined)}
      ${celda(p.expect_calls, 'expect', 'La misma prueba descartada, con el motivo escrito', 'var(--ok)')}
      ${celda(p.unchecked_casts, 'as', 'Conversiones sin chequear: `as` trunca en silencio, `try_into()` devuelve un Result')}
      ${celda(p.fallible_fns, 'fns falibles', 'Funciones que devuelven Result: las que tienen `?` disponible')}
      ${celda(p.unwraps_in_fallible_fns, 'unwrap evitable', 'unwrap adentro de una función que podría haber usado `?`', p.unwraps_in_fallible_fns > 0 ? 'var(--err)' : 'var(--ok)')}
    </div>
    ${
      p.unwraps_in_fallible_fns > 0
        ? `<div class="st-mem-note mt-2">${p.unwraps_in_fallible_fns === 1 ? 'El de la última columna es el que más vale: está' : `Los <code>${p.unwraps_in_fallible_fns}</code> de la última columna son los que más valen: están`} dentro de funciones que <strong>ya devuelven <code>Result</code></strong>, así que el llamador está preparado para recibir el error y <code>?</code> lo propaga en un carácter. Un <code>.unwrap()</code> en una función infalible es otra cosa — ahí el pánico puede ser la única opción, y por eso no se cuenta acá.</div>`
        : ''
    }
  </div>`
}

/** **La superficie de confianza de D, y por que se ve distinta a la de Rust.**
 *
 * Rust tiene dos niveles y marca la revision con un comentario; D tiene tres
 * y el del medio es una palabra del lenguaje. Por eso esta seccion puede
 * mostrar algo que la de Rust no: **la lista** de lo que hay que revisar, no
 * solo cuanto hay. */
/** **Lo probado y lo no probado no se ven igual, y esa es la regla.**
 *
 * La Linea E se la puso por escrito antes de empezar: *"una propiedad probada
 * y una advertencia del analizador no se muestran igual, y no se afirma que
 * un proyecto SPARK esta verificado sin leer el resultado real del
 * probador"*.
 *
 * Por eso lo probado va en verde y **dice quien lo probo** — no fue este
 * motor—, y lo no probado va en naranja con el texto del prover. Un contador
 * solo de "probadas" seria exactamente la afirmacion que la regla prohibe. */
function _renderSpark(): string {
  if (_sparkRunning) {
    return `<div class="metric-section"><div class="ms-title">Verificacion formal (SPARK)</div>
      <div class="st-mem-note"><span class="up-spinner"></span> GNATprove esta corriendo. Un probador resolviendo formulas tarda bastante mas que un parser.</div></div>`
  }
  const r = _sparkResult
  if (!r) {
    return `<div class="metric-section">
      <div class="ms-title">Verificacion formal (SPARK)</div>
      <div class="st-mem-note">GNATprove le pide a un probador externo que demuestre cada obligacion de este archivo. <strong>Lo que salga probado no lo prueba Sythrall</strong>: lo cierra Alt-Ergo o Z3 resolviendo una formula, y por eso se reporta como una categoria propia.</div>
      <div class="mt-2"><button class="btn btn-ghost btn-sm" data-verify-spark title="Corre GNATprove sobre este archivo. Puede tardar minutos.">Verificar con SPARK</button></div>
    </div>`
  }
  if (!r.available) {
    return `<div class="metric-section"><div class="ms-title">Verificacion formal (SPARK)</div>
      <div class="st-mem-note">${esc(r.note)}</div>
      <div class="mt-2"><button class="btn btn-ghost btn-sm" data-verify-spark>Reintentar</button></div></div>`
  }

  const total = r.proved + r.unproved
  const fila = (o: SparkObligation) => {
    const probada = o.kind === 'spark_proved'
    return `<div class="st-import-chip" style="border-left:3px solid ${probada ? 'var(--ok)' : 'var(--warn)'}">
      <span class="st-import-type" style="color:${probada ? 'var(--ok)' : 'var(--warn)'}">${probada ? 'probada' : 'sin probar'}</span>
      <span class="mono">${esc(o.rule)}</span>
      <span class="st-fn-line">linea ${o.line}</span>
      <span class="meta">${esc(o.message)}</span>
    </div>`
  }

  return `<div class="metric-section">
    <div class="ms-title">Verificacion formal (SPARK) — ${total} obligacion${total === 1 ? '' : 'es'}</div>
    <div class="st-mem-note">${esc(r.note)}</div>
    <div class="st-summary-row">
      <div class="st-stat" title="Obligaciones que el probador externo cerro. No las probo Sythrall leyendo el codigo.">
        <div class="st-stat-val" style="color:var(--ok)">${r.proved}</div>
        <div class="st-stat-lbl">Probadas por el prover</div>
      </div>
      <div class="st-stat" title="Obligaciones que el probador no pudo cerrar. No es un hallazgo: es una prueba que no cerro.">
        <div class="st-stat-val"${r.unproved > 0 ? ' style="color:var(--warn)"' : ''}>${r.unproved}</div>
        <div class="st-stat-lbl">Sin probar</div>
      </div>
      <div class="st-stat" title="Quien hizo la demostracion">
        <div class="st-stat-val mono" style="font-size:.8rem">${esc(r.prover ?? '—')}</div>
        <div class="st-stat-lbl">Probador</div>
      </div>
    </div>
    <div class="st-import-grid mt-2">${r.obligations.map(fila).join('')}</div>
    <div class="mt-2"><button class="btn btn-ghost btn-sm" data-verify-spark>Volver a verificar</button></div>
  </div>`
}

async function _verificarSpark(): Promise<void> {
  if (_sparkRunning) return
  _sparkRunning = true
  _repintarResultado()
  try {
    _sparkResult = await api.verifySpark(_resultSource)
  } catch (e) {
    toast('Error: ' + (e instanceof Error ? e.message : ''), 'err')
  } finally {
    _sparkRunning = false
    _repintarResultado()
  }
}

function _renderTrustSurfaceD(t: TrustSurfaceD): string {
  const marcadas = t.safe_fns + t.trusted_fns + t.system_fns
  const celda = (valor: number, etiqueta: string, ayuda: string, color?: string) => `
    <div class="st-stat" title="${esc(ayuda)}">
      <div class="st-stat-val"${color ? ` style="color:${color}"` : ''}>${valor}</div>
      <div class="st-stat-lbl">${esc(etiqueta)}</div>
    </div>`

  if (marcadas === 0) {
    return `
  <div class="metric-section">
    <div class="ms-title">Superficie de confianza</div>
    <div class="st-mem-note">Ninguna función de este archivo declara <code>@safe</code>, <code>@trusted</code> ni <code>@system</code>. <strong>Eso no quiere decir que no esté chequeado</strong>: en D el compilador infiere el atributo de los templates y de las funciones <code>auto</code>. Quiere decir que nadie lo escribió, que es un hecho distinto.</div>
  </div>`
  }

  const pct = Math.round((t.trusted_fns / marcadas) * 100)
  const lista = t.review_surface
    .slice(0, 40)
    .map(
      (
        p,
      ) => `<div class="st-import-chip" title="${esc(p.inherited ? 'nivel heredado de una etiqueta o un bloque, no escrito en esta firma' : 'escrito en la firma')}">
        <span class="st-import-type">${p.inherited ? 'heredada' : '@trusted'}</span>
        <span>${esc(p.name)}</span>
        <span class="st-fn-line">línea ${p.line}</span>
        ${p.has_comment ? '' : '<span class="meta" style="color:var(--warn)">sin comentario</span>'}
      </div>`,
    )
    .join('')

  return `
  <div class="metric-section">
    <div class="ms-title">Superficie de confianza — tres niveles (${marcadas} función${marcadas === 1 ? '' : 'es'} marcada${marcadas === 1 ? '' : 's'})</div>
    <div class="st-mem-note">D gradúa la frontera en tres: <code>@safe</code> lo chequea el compilador, <code>@system</code> no lo chequea nadie, y <code>@trusted</code> es <strong>el del medio</strong> — código sin chequear que el autor declara seguro de llamar desde <code>@safe</code>. <strong>Esa es la superficie de revisión, y acá tiene nombre propio</strong>: en Rust la misma frontera se marca con un <code>// SAFETY:</code> que el compilador no conoce, así que enumerarla es leer comentarios. Acá se le puede preguntar al lenguaje.</div>
    <div class="st-summary-row">
      ${celda(t.safe_fns, '@safe', 'El compilador chequea', 'var(--ok)')}
      ${celda(t.trusted_fns, '@trusted', 'Sin chequeo, con afirmación del autor: esto es lo que hay que revisar', t.trusted_fns > 0 ? 'var(--warn)' : undefined)}
      ${celda(t.system_fns, '@system', 'Sin chequeo y sin afirmación')}
      ${celda(t.unmarked_fns, 'Sin marcar', 'Nadie escribió el atributo. En templates D lo infiere, así que no es lo mismo que @system')}
      ${celda(t.inherited, 'Heredadas', 'El nivel viene de una etiqueta `@safe:` o de un bloque, no de la firma')}
      ${celda(t.trusted_with_comment, 'Con comentario', 'De las @trusted, cuántas tienen una explicación pegada arriba', 'var(--ok)')}
    </div>
    ${
      t.trusted_fns > 0
        ? `<div class="st-mem-note mt-2"><strong>${pct}%</strong> de lo marcado es <code>@trusted</code>: esa fracción es el tamaño real de la revisión manual.</div>
      <div class="st-import-grid mt-2">${lista}</div>
      ${t.review_surface.length > 40 ? `<div class="st-mem-note mt-1">… y ${t.review_surface.length - 40} más.</div>` : ''}`
        : '<div class="st-mem-note mt-2">Ninguna <code>@trusted</code>: no hay nada que revisar a mano en este archivo.</div>'
    }
  </div>`
}

/** Rust y D miden la misma frontera con formas distintas; el discriminante es
 * cuál de los dos vocabularios trae el objeto. */
function _renderTrustSurfaceAny(t: TrustSurface | TrustSurfaceD): string {
  return 'trusted_fns' in t ? _renderTrustSurfaceD(t) : _renderTrustSurface(t)
}

function _renderTrustSurface(t: TrustSurface): string {
  const total = t.unsafe_blocks + t.unsafe_fns
  const documentados = t.documented_blocks + t.documented_fns
  const celda = (valor: number, etiqueta: string, ayuda: string, color?: string) => `
    <div class="st-stat" title="${esc(ayuda)}">
      <div class="st-stat-val"${color ? ` style="color:${color}"` : ''}>${valor}</div>
      <div class="st-stat-lbl">${esc(etiqueta)}</div>
    </div>`

  // El caso de superficie cero merece decirse, no esconderse: "no hay
  // `unsafe`" es un resultado, y no es lo mismo que "no se midió".
  if (total === 0 && t.unsafe_impls === 0) {
    return `
  <div class="metric-section">
    <div class="ms-title">Superficie de confianza</div>
    <div class="st-mem-note">Este archivo no renuncia a ninguna garantía del compilador: <strong>sin <code>unsafe</code></strong>. Todo lo que hace está dentro de lo que el sistema de tipos puede probar.</div>
  </div>`
  }

  const sinJustificar = total - documentados
  return `
  <div class="metric-section">
    <div class="ms-title">Superficie de confianza (${total} punto${total === 1 ? '' : 's'})</div>
    <div class="st-mem-note">Dónde el código le pide al compilador que confíe en el autor. <strong>No es una falla</strong>: <code>unsafe</code> es legítimo y a veces necesario. Lo que esto mide es cuánto de esa superficie está <strong>justificada por escrito</strong> — un <code>// SAFETY:</code> arriba del bloque, o una sección <code># Safety</code> en el doc de la función. Lo no justificado no está mal: está <em>sin revisar</em>.</div>
    <div class="st-summary-row">
      ${celda(t.unsafe_blocks, 'Bloques unsafe', 'Cada `unsafe { ... }` del archivo')}
      ${celda(t.documented_blocks, 'Con SAFETY', 'Bloques con un comentario `SAFETY` inmediatamente arriba', 'var(--ok)')}
      ${celda(t.unsafe_fns, 'Fns unsafe', 'Funciones que trasladan la obligación a quien las llama')}
      ${celda(t.documented_fns, 'Con # Safety', 'Funciones cuyo doc-comment declara el contrato', 'var(--ok)')}
      ${celda(t.unsafe_impls, 'impls unsafe', 'Cada `unsafe impl` — afirma una propiedad del tipo entero, no de una línea')}
      ${celda(sinJustificar, 'Sin justificar', 'Puntos de la superficie sin SAFETY ni # Safety', sinJustificar > 0 ? 'var(--warn)' : 'var(--ok)')}
    </div>
    ${
      t.unsafe_impls > 0
        ? `<div class="st-mem-note mt-2">Los <code>unsafe impl</code> se cuentan aparte a propósito: un <code>unsafe impl Send for T</code> no afirma nada sobre una línea, afirma una propiedad de <strong>todo uso futuro del tipo</strong>. Es la parte de la superficie más cara de revisar y la más fácil de pasar por alto.</div>`
        : ''
    }
  </div>`
}

function _renderMemoryLayout(layout: MemoryLayoutResult): string {
  const byRegion: Record<string, typeof layout.variables> = { stack: [], data: [], bss: [] }
  for (const v of layout.variables) byRegion[v.region]?.push(v)

  const regionBlock = (region: 'stack' | 'data' | 'bss') => {
    const vars = byRegion[region]
    if (!vars.length) return ''
    const color = MEM_REGION_COLOR[region]
    return `
    <div class="st-mem-region">
      <div class="st-mem-region-title" style="color:${color}">${MEM_REGION_LABEL[region]} (${vars.length})</div>
      ${vars
        .map(
          (v) => `<div class="st-mem-var" style="border-left-color:${color}">
          <span class="st-mem-var-name">${esc(v.name)}</span>
          <span class="st-mem-var-type">${esc(v.type_hint)}</span>
          <span class="st-mem-var-scope">${esc(v.scope)}</span>
          <span class="st-fn-line">línea ${v.line}</span>
        </div>`,
        )
        .join('')}
    </div>`
  }

  const allocBlock = layout.allocations.length
    ? `<div class="st-mem-region">
        <div class="st-mem-region-title text-orange">Heap — allocations (${layout.allocations.length})</div>
        ${layout.allocations
          .map(
            (a) => `<div class="st-mem-var" style="border-left-color:var(--orange)">
            <span class="st-mem-var-name">${esc(a.kind)}(${a.variable ? esc(a.variable) : '?'})</span>
            <span class="st-fn-line">línea ${a.line}</span>
          </div>`,
          )
          .join('')}
      </div>`
    : ''

  return `
  <div class="metric-section">
    <div class="ms-title">Memory Layout — stack/heap/data/bss (${layout.variables.length} variables)</div>
    <div class="st-mem-note" title="${esc(layout.note)}">Clasificación estática (AST), no una medición de un proceso corriendo</div>
    <div class="st-mem-grid">
      ${regionBlock('stack')}${regionBlock('data')}${regionBlock('bss')}${allocBlock}
    </div>
  </div>`
}

const MODERNIZATION_CONFIDENCE_COLOR: Record<string, string> = {
  high: 'var(--err)',
  medium: 'var(--warn)',
}
const MODERNIZATION_TARGET_LABEL: Record<string, string> = {
  raii_smart_pointer: 'RAII / smart pointer',
  rust_ownership: 'Rust ownership',
  cpp_named_cast: 'static_cast / reinterpret_cast / const_cast',
  rust_format_macro: 'Rust format! / println!',
  python_stdlib_replacement: 'Reemplazo en la stdlib',
  python_modern_api: 'API vigente de Python',
  collections_abc: 'collections.abc',
  pep585_builtin_generics: 'Genéricos nativos (PEP 585/604)',
}

/** Fase 25 — rollup por archivo a nivel de PROYECTO (2026-09-01), distinto
 * de `_renderModernization` de abajo (que muestra cada candidato individual
 * con su razonamiento completo, para un solo archivo). Acá, con potencialmente
 * muchos archivos, un resumen por archivo es lo honesto: conteos por
 * confianza y por patrón, sin un veredicto Keep/Refactor/Replace/Rewrite/
 * Migrate — esa categorización necesitaría reglas de mapeo patrón→categoría
 * que hoy serían inventadas, no sostenidas por la evidencia que cada
 * patrón realmente prueba (mismo principio de "evidencia, no un veredicto"
 * que Security & Taint Intelligence ya respeta). Para ver el detalle de un
 * archivo puntual, todavía hay que analizarlo individualmente en este mismo
 * panel — esto es un mapa de "dónde mirar primero", no un reemplazo. */
function _renderModernizationByFile(byFile: StaticProjectResult['modernization_by_file']): string {
  return `
  <div class="metric-section">
    <div class="ms-title">Modernization Intelligence — resumen por archivo (${byFile.length})</div>
    <div class="st-mem-note">Candidatos de C/C++ y de Python (Fase 25) agrupados por archivo — conteos por confianza y patrón, no un veredicto Keep/Refactor/Replace/Rewrite/Migrate</div>
    ${byFile
      .map(
        (f) => `
      <div class="st-wasm-item">
        <div class="st-fn-head">
          <span class="st-fn-name">${esc(f.file)}</span>
          ${f.high ? `<span style="font-size:.65rem;color:var(--err);font-family:var(--mono)">${f.high} alta</span>` : ''}
          ${f.medium ? `<span style="font-size:.65rem;color:var(--warn);font-family:var(--mono)">${f.medium} media</span>` : ''}
          <span style="margin-left:auto;font-size:.65rem;color:var(--muted);font-family:var(--mono)">${f.total} total</span>
        </div>
        <div style="display:flex;flex-wrap:wrap;gap:5px;margin-top:4px">
          ${Object.entries(f.patterns)
            .map(
              ([pattern, count]) =>
                `<span style="font-size:.6rem;padding:2px 7px;border-radius:3px;background:var(--ghost-tint);color:var(--muted);font-family:var(--mono)">${esc(pattern)} × ${count}</span>`,
            )
            .join('')}
        </div>
      </div>`,
      )
      .join('')}
  </div>`
}

/** Fase 25 (Modernization Intelligence) — primer motor real, no una
 * conversión automática: cada candidato lleva su evidencia (línea, patrón,
 * razonamiento), el usuario decide qué hacer con eso. Mismo criterio de
 * lista que `_renderMemoryLayout`/`_renderSecurityFindings`, sin un botón
 * de "migrar" — esta fase entiende y propone, no reescribe código. */
function _renderModernization(report: ModernizationReport): string {
  return `
  <div class="metric-section">
    <div class="ms-title">Modernization Intelligence — candidatos (${report.candidates.length})</div>
    <div class="st-mem-note" title="${esc(report.note)}">C/C++ sobre allocation sites ya calculados; Python sobre APIs deprecadas o removidas de la stdlib — propone, no convierte código automáticamente. El detalle de cada límite está en el tooltip.</div>
    ${report.candidates
      .map((c) => {
        const color = MODERNIZATION_CONFIDENCE_COLOR[c.confidence] ?? 'var(--muted)'
        return `<div class="st-wasm-item">
        <div class="st-fn-head">
          <span class="st-fn-name">${esc(c.variable)}</span>
          <span style="font-size:.65rem;color:${color};font-family:var(--mono)">confianza: ${c.confidence === 'high' ? 'alta' : 'media'}</span>
          <span class="st-fn-line">línea ${c.line}</span>
          <span style="margin-left:auto;font-size:.65rem;color:var(--info)">→ ${esc(MODERNIZATION_TARGET_LABEL[c.suggested_target] ?? c.suggested_target)}</span>
        </div>
        <div class="st-wasm-rec" style="color:var(--txt)">${esc(c.current)}</div>
        <div class="st-wasm-rec">${esc(c.reasoning)}</div>
      </div>`
      })
      .join('')}
  </div>`
}

const ASM_CATEGORY_COLOR: Record<string, string> = {
  data_movement: 'var(--info)',
  arithmetic: 'var(--ok)',
  logic: 'var(--purple)',
  comparison: 'var(--warn)',
  control_flow: 'var(--orange)',
  stack: 'var(--err)',
  context_switch: 'var(--purple)',
  other: 'var(--muted)',
}

/** Fase 19, 3er bullet — badge de una línea junto al nombre del
 * procedimiento; el "por qué" completo vive en el tooltip (`title`), mismo
 * convención pedagógica que el resto del motor (badges "Type-1?"/"Matrix?"). */
function _renderStackFrameBadge(frame: StackFrameInfo): string {
  const bytesNote = frame.local_stack_bytes ? ` · ${frame.local_stack_bytes}B locales` : ''
  if (frame.has_standard_prologue && frame.has_standard_epilogue) {
    return `<span class="bigo-cs-badge text-ok" title="${esc(frame.explanation)}">Frame: estándar${bytesNote}</span>`
  }
  if (frame.is_leaf_function) {
    return `<span class="bigo-cs-badge text-muted" title="${esc(frame.explanation)}">Frame: omitido (leaf)</span>`
  }
  return `<span class="bigo-cs-badge text-warn" title="${esc(frame.explanation)}">Frame: no estándar</span>`
}

/** Fase 19 — pattern-matching sobre texto (`asmparse.rs`), no un
 * disassembler real. Un `.metric-section` por procedimiento (label
 * delimitado), con sus instrucciones coloreadas por categoría y sus
 * registros usados como chips — mismo criterio de lista que
 * `_renderMemoryLayout`, sin diagrama animado nuevo. */
/** Fase 32 — localidad espacial de acceso a matrices.
 *
 * **La regla es OPUESTA entre los dos lenguajes que la reportan**, y por eso
 * el `kind` los distingue en vez de compartir uno solo: C guarda row-major y
 * quiere el indice que varia mas rapido AL FINAL; Fortran guarda
 * column-major y lo quiere AL PRINCIPIO. */
interface LocalityFinding {
  kind: string
  function: string
  array: string
  line: number
  inner_loop_variable: string
  severity: string
  confidence: string
  message: string
}

/** Se calculaba, se relayaba, y solo se veia en la vista de PROYECTO: en un
 * archivo suelto el hallazgo existia y no se mostraba. Era asi desde la 1a
 * porcion, o sea que tambien estaba invisible para C. */
function _renderLocality(findings: LocalityFinding[], language: string): string {
  if (!findings.length) return ''
  const orden =
    language === 'fortran'
      ? 'column-major (el indice que varia mas rapido va PRIMERO)'
      : 'row-major (el indice que varia mas rapido va ULTIMO)'
  return `
  <div class="metric-section">
    <div class="ms-title">Localidad espacial (${findings.length}) — ${esc(orden)}</div>
    ${findings
      .map(
        (f) => `<div class="st-wasm-item">
        <div class="st-fn-head">
          <span class="st-fn-name">${esc(f.array)}</span>
          <span style="font-size:.65rem;color:var(--orange);font-family:var(--mono)">stride grande</span>
          <span style="font-size:.6rem;padding:1px 6px;border-radius:3px;border:1px solid ${f.confidence === 'High' ? 'var(--ok)' : 'var(--warn)'};color:${f.confidence === 'High' ? 'var(--ok)' : 'var(--warn)'};font-family:var(--mono)">confianza ${esc(f.confidence)}</span>
          <span class="st-fn-line ml-auto">${esc(f.function)} · linea ${f.line}</span>
        </div>
        <div class="st-wasm-rec">${esc(f.message)}</div>
      </div>`,
      )
      .join('')}
  </div>`
}

/** Fase 32 — los bloques de cambio de contexto de un procedimiento.
 *
 * **Se muestran como su propia fila y no como una instruccion mas** porque
 * lo que importa de ellos no es el mnemonico sino CUANTOS registros mueven:
 * `stmfd sp!, {r4-r11, lr}` se lee como una linea corta y son nueve
 * registros. El conteo es el dato. */
function _renderContextSwitch(blocks: ContextSwitchBlock[]): string {
  if (!blocks.length) return ''
  return `<div class="st-import-grid mt-2">
    ${blocks
      .map((b) => {
        const color = b.is_return ? 'var(--orange)' : b.direction === 'save' ? 'var(--purple)' : 'var(--info)'
        const verbo = b.direction === 'save' ? 'guarda' : 'restaura'
        const extra = b.is_return ? ' · retorna' : b.invalid_in_x86_64 ? ' · no en x86-64' : ''
        return `<span class="st-import-chip" title="${esc(b.explanation)}" style="border-left:2px solid ${color}">
          <span class="st-import-name">${esc(b.mnemonic)}</span>
          <span class="st-fn-line">${verbo} ${b.register_count} reg${extra} · línea ${b.line}</span>
        </span>`
      })
      .join('')}
  </div>`
}

/** El nombre que se le pone al archivo arriba de todo.
 *
 * Estaba fijo en "Assembly x86-64", que es falso para un archivo de ARM —
 * y la sintaxis caia a "Intel" por descarte, que tambien lo era.
 *
 * **Y "x86-64" tampoco vale si el archivo usa `PUSHA`/`POPA`**: esas
 * instrucciones se eliminaron del modo de 64 bits, asi que su sola
 * presencia prueba que el archivo es de 16 o 32 bits. Es el mismo dato que
 * `callingconv.rs` ya usa para no citar un default de x86-64 que no rige; no
 * tenia sentido arreglarlo en el motor y dejar el titulo mintiendo. */
function _asmTitulo(syntax: 'att' | 'intel' | 'arm' | undefined, es16o32: boolean): string {
  if (syntax === 'arm') return 'Assembly ARM — sintaxis unificada, lista de registros entre llaves'
  const sintaxis = syntax === 'att' ? 'AT&T' : 'Intel'
  if (es16o32) return `Assembly x86 de 16/32 bits (usa PUSHA/POPA) — sintaxis detectada: ${sintaxis}`
  return `Assembly x86-64 — sintaxis detectada: ${sintaxis}`
}

// ─── Frente de Pareto de la configuración de compilación (Fase 32) ───────────
//
// Hermano de la tabla de arriba, con una pregunta distinta: aquella compara
// los niveles que vos elegís, ésta **busca** sobre una grilla y te dice cuáles
// no están dominados por ninguna otra. Sobre un bucle vectorizable, de seis
// configuraciones el frente son dos — `-Os` (el código más chico) y
// `-O3 +avx2` (el único que vectoriza). Las otras cuatro son estrictamente
// peores que alguna, y eso es lo que un número suelto no dice.

let _paretoBuscando = false
let _pareto: BusquedaPareto | null = null

function _filaPareto(p: PuntoPareto, enFrente: boolean): string {
  if (p.error) {
    return `<tr class="mc-pareto-err"><td class="mono">${esc(p.label)}</td><td colspan="4">${esc(p.error.slice(0, 90))}</td></tr>`
  }
  const estado = enFrente
    ? `<span class="mc-badge mc-badge-ok" title="Ninguna otra configuración es mejor o igual en los dos objetivos">frente</span>`
    : `<span class="mc-badge mc-badge-off" title="Hay otra configuración que no es peor en ningún objetivo y es mejor en alguno">dominado por ${esc(p.dominated_by.slice(0, 2).join(', '))}</span>`
  return `<tr${enFrente ? ' class="mc-pareto-frente"' : ''}>
    <td class="mono">${esc(p.label)}</td>
    <td class="num">${p.instructions}</td>
    <td class="num">${p.packed}</td>
    <td>${p.vectorized ? `<span class="mc-badge mc-badge-ok">sí</span>` : `<span class="mc-badge mc-badge-off">no</span>`}</td>
    <td>${estado}</td>
  </tr>`
}

function _renderPareto(): string {
  let cuerpo: string
  if (_paretoBuscando) {
    cuerpo = `<div class="mc-wait"><span class="up-spinner"></span> Compilando cada configuración de la grilla…</div>`
  } else if (_pareto) {
    const r = _pareto
    cuerpo = `
      <div class="mc-table-wrap">
        <table class="mc-table">
          <thead><tr>
            <th>configuración</th>
            <th title="A minimizar">instrucciones ↓</th>
            <th title="A maximizar: instrucciones que operan sobre varios elementos a la vez">packed ↑</th>
            <th>vectoriza</th>
            <th></th>
          </tr></thead>
          <tbody>${r.points.map((p) => _filaPareto(p, r.front.includes(p.label))).join('')}</tbody>
        </table>
      </div>
      <div class="mc-pareto-resumen">
        <strong>${r.front.length} de ${r.points.length}</strong> configuraciones no están dominadas:
        ${r.front.map((f) => `<code>${esc(f)}</code>`).join(', ') || '—'}
      </div>
      <div class="mc-aviso">${esc(r.note)}</div>`
  } else {
    cuerpo = `<div class="empty">Compila una grilla de configuraciones y quedate con las que no están dominadas.</div>`
  }

  return `
  <div class="metric-section">
    <div class="ms-title">BÚSQUEDA DE CONFIGURACIÓN — FRENTE DE PARETO</div>
    <div class="mc-explica">
      Mismo código, distintas opciones del compilador. Dos objetivos en conflicto:
      <strong>menos instrucciones</strong> y <strong>más vectorización</strong>.
      Una configuración está <em>dominada</em> si otra no es peor en ninguno de los dos
      y es mejor en alguno — ésas se pueden descartar sin pensar. Las que quedan son
      el compromiso real. <strong>El fuente no se toca</strong>, así que no hay nada
      que probar sobre equivalencia.
    </div>
    <div class="mc-controles">
      <button class="btn btn-run btn-sm" data-pareto-buscar ${_paretoBuscando ? 'disabled' : ''}>Buscar</button>
    </div>
    ${cuerpo}
  </div>`
}

async function _buscarPareto(r: StaticResult): Promise<void> {
  const lang = lenguajeCompilable(r)
  if (!lang) return
  const contenido = _fuenteAnalizada()
  if (!contenido.trim()) {
    toast('No hay contenido que compilar', 'warn')
    return
  }
  _paretoBuscando = true
  _repintarResultado()
  try {
    const res = await api.compilePareto(lang, contenido)
    _pareto = res.result
    if (!res.available) toast(res.error ?? 'El motor no está disponible', 'err')
  } catch (e) {
    toast('Error: ' + (e instanceof Error ? e.message : ''), 'err')
  } finally {
    _paretoBuscando = false
    _repintarResultado()
  }
}

// ─── Atomic CSS (Fase 31) ────────────────────────────────────────────────────
//
// **La vista existe para mostrar el canje, no para venderlo.** Atomizar
// comprime el CSS y engorda el marcado, y Sythrall solo ve un lado — así que
// el número grande (el ratio) va al lado del número que lo matiza (los átomos
// de un solo uso, que no ahorran nada y suman una clase al HTML). Sobre el
// `main.css` de este repo: ratio 2.62, pero 662 de 926 átomos son de un solo
// uso. Mostrar solo el 2.62 sería mentir por omisión.

let _atomizando = false
let _atomico: AtomizacionResult | null = null

function _renderCssAtomico(r: StaticResult): string {
  if (r.language !== 'css') return ''

  let cuerpo: string
  if (_atomizando) {
    cuerpo = `<div class="mc-wait"><span class="up-spinner"></span> Atomizando…</div>`
  } else if (_atomico) {
    const s = _atomico.summary
    const pctSingle = s.atoms_after ? Math.round((s.single_use_atoms / s.atoms_after) * 100) : 0
    const top = _atomico.atoms
      .slice()
      .sort((a, b) => b.uses - a.uses)
      .slice(0, 8)
    cuerpo = `
      <div class="st-summary-row">
        ${sc(String(s.declarations_before), 'DECLARACIONES')}
        ${sc(String(s.atoms_after), 'ÁTOMOS')}
        ${sc(s.reuse_ratio.toFixed(2) + '×', 'REUSO')}
        ${sc(String(s.single_use_atoms), 'DE UN SOLO USO')}
        ${sc(fmtBytes(s.bytes_before), 'CSS ACTUAL')}
        ${sc(fmtBytes(s.bytes_after), 'CSS ATÓMICO')}
      </div>
      <div class="mc-pareto-resumen">
        Cada átomo se reusa <strong>${s.reuse_ratio.toFixed(2)}</strong> veces en promedio —
        pero <strong>${s.single_use_atoms} de ${s.atoms_after} (${pctSingle}%)</strong>
        aparecen una sola vez: ésos no ahorran una línea y suman una clase al marcado.
        El más reusado es <code>.${esc(s.top_atom ?? '')}</code> × ${s.top_atom_uses}.
      </div>
      <div class="ca-atomos">
        ${top
          .map(
            (a) => `<div class="ca-atomo">
              <span class="mono">.${esc(a.class)}</span>
              <span class="meta">${esc(a.property)}: ${esc(a.value)}${a.important ? ' !important' : ''}</span>
              ${a.at_context ? `<span class="mc-badge" title="Los átomos de un @media no se pueden colapsar con los de afuera">${esc(a.at_context)}</span>` : ''}
              <span class="ml-auto mono">× ${a.uses}</span>
            </div>`,
          )
          .join('')}
      </div>
      <details class="ca-hoja">
        <summary>Ver la hoja atómica generada (${_atomico.atoms.length} reglas)</summary>
        <pre class="mc-asm-body">${esc(_atomico.stylesheet)}</pre>
      </details>
      <div class="mc-aviso">${esc(_atomico.note)}</div>`
  } else {
    cuerpo = `<div class="empty">Calculá cuánto comprimiría atomizar esta hoja — y cuánto no.</div>`
  }

  return `
  <div class="metric-section">
    <div class="ms-title">ATOMIC CSS — QUÉ SE GANA Y QUÉ SE PAGA</div>
    <div class="mc-explica">
      Atomizar convierte cada par <code>propiedad: valor</code> en una clase única que los
      elementos acumulan. <strong>El CSS se achica; el marcado crece</strong>, y ese lado
      no se mide acá. <strong>No reescribe nada</strong>: devuelve la hoja y el mapa para
      que los puedas leer y comparar.
    </div>
    <div class="mc-controles">
      <button class="btn btn-run btn-sm" data-css-atomizar ${_atomizando ? 'disabled' : ''}>Calcular</button>
    </div>
    ${cuerpo}
  </div>`
}

async function _atomizarCss(r: StaticResult): Promise<void> {
  const contenido = _fuenteAnalizada()
  if (!contenido.trim()) {
    toast('No hay CSS que atomizar', 'warn')
    return
  }
  _atomizando = true
  _repintarResultado()
  try {
    const res = await api.cssAtomic(r.filename, contenido)
    _atomico = res.result
    if (!res.available) toast(res.error ?? 'El motor no está disponible', 'err')
  } catch (e) {
    toast('Error: ' + (e instanceof Error ? e.message : ''), 'err')
  } finally {
    _atomizando = false
    _repintarResultado()
  }
}

function _renderAsmBreakdown(functions: ParsedFunction[], syntax?: 'att' | 'intel' | 'arm'): string {
  const procs = functions.filter((f) => f.instructions?.length)
  if (!procs.length) return ''

  // El `%` es el sigilo de AT&T. Imprimirlo siempre era falso incluso para
  // un archivo de Intel, donde los registros van pelados.
  const sigilo = syntax === 'att' ? '%' : ''
  const es16o32 = procs.some((f) => f.context_switch?.some((b) => b.invalid_in_x86_64))

  return `
  <div class="metric-section">
    <div class="ms-title">${_asmTitulo(syntax, es16o32)}</div>
    ${procs
      .map(
        (fn) => `<div class="st-fn-item">
        <div class="st-fn-head">
          <span class="st-fn-name">${esc(fn.name)}</span>
          <span class="bigo-badge" style="color:${BIG_O_COLOR[fn.big_o] ?? 'var(--muted)'};border-color:${BIG_O_COLOR[fn.big_o] ?? 'var(--muted)'}" title="${esc(fn.big_o_reason)}">${esc(fn.big_o)}</span>
          <span class="st-fn-line">línea ${fn.line}</span>
          ${fn.stack_frame ? _renderStackFrameBadge(fn.stack_frame) : ''}
        </div>
        ${
          fn.registers_used?.length
            ? `<div class="st-import-grid">${fn.registers_used.map((r) => `<span class="st-import-chip"><span class="st-import-name">${sigilo}${esc(r)}</span></span>`).join('')}</div>`
            : ''
        }
        ${_renderContextSwitch(fn.context_switch ?? [])}
        <div class="st-mem-grid">
          ${(fn.instructions ?? [])
            .map(
              (
                ins,
              ) => `<div class="st-mem-var" style="border-left-color:${ASM_CATEGORY_COLOR[ins.category] ?? 'var(--muted)'}" title="${esc(ins.explanation)}">
              <span class="st-mem-var-name">${esc(ins.mnemonic)}</span>
              <span class="st-mem-var-type">${ins.operands.map(esc).join(', ')}</span>
              <span class="st-fn-line">línea ${ins.line}</span>
            </div>`,
            )
            .join('')}
        </div>
      </div>`,
      )
      .join('')}
  </div>`
}

function _renderSecurityFindings(findings: SecurityFinding[]): string {
  const confLabel = (c: string) => (c === 'High' ? 'Alta' : c === 'Medium' ? 'Media' : 'Baja')

  return `
  <div class="metric-section">
    <div class="ms-title">${icon('shield', 14)} Security & Taint (${findings.length}) <span class="sec-disclaimer">— patrones heurísticos, no un reemplazo de SAST</span></div>
    ${findings
      .map((f) => {
        const color = securitySeverityColor(f.severity)
        return `<div class="st-wasm-item sec-finding">
        <div class="st-fn-head">
          ${f.file ? `<span class="sec-cwe">${esc(f.file)}</span>` : ''}
          <span class="st-fn-name">${esc(f.category)}</span>
          <span class="sec-cwe">${esc(f.cwe)}</span>
          <span style="font-size:.65rem;color:${color};font-family:var(--mono)">${esc(f.severity)}</span>
          <span class="sec-confidence">confianza ${confLabel(f.confidence)}</span>
          ${f.function ? `<span class="st-fn-line">${esc(f.function)}()</span>` : ''}
          <span class="st-fn-line ml-auto">línea ${f.line}</span>
        </div>
        <div class="sec-flow">
          <code>${esc(f.source)}</code>${f.sink ? ` → <code>${esc(f.sink)}</code>` : ''}
        </div>
        <div class="st-wasm-rec">${esc(f.recommendation)}</div>
      </div>`
      })
      .join('')}
  </div>`
}

/** Fase 32 — la etiqueta de decidibilidad de un hallazgo, con su motivo en
 * el tooltip.
 *
 * **Lo que responde es "¿por qué este hallazgo es aproximado?", y la
 * respuesta NO es "el análisis es flojo".** Un hallazgo `approximate` lo es
 * porque la propiedad no es decidible — por Rice, ninguna herramienta puede
 * hacerlo mejor sin ejecutar el programa. Decirlo acá es lo que separa un
 * límite matemático de una excusa, y es la razón por la que se muestra al
 * lado del hallazgo y no en un apéndice que nadie abre.
 *
 * `unclassified` se pinta gris y se muestra igual: un kind que todavía no
 * entró en la tabla se ve, en vez de pasar por decidible sin serlo. */
function _decidabilityBadge(s: StructuralSmell): string {
  if (!s.decidability) return ''
  const [texto, color] =
    s.decidability === 'decidable'
      ? ['decidible', 'var(--ok)']
      : s.decidability === 'approximate'
        ? ['aproximado', 'var(--warn)']
        : ['sin clasificar', 'var(--muted)']
  const motivo = s.decidability_reason
    ? `${texto === 'decidible' ? 'Se prueba en el código' : 'No es decidible'}: ${s.decidability_reason}`
    : 'Este tipo de hallazgo todavía no está clasificado en la tabla de decidibilidad del motor.'
  return `<span class="badge badge-outline" style="--tag-color:${color}" title="${esc(motivo)}">${texto}</span>`
}

function _renderStructuralSmells(smells: StructuralSmell[]): string {
  return `
  <div class="metric-section">
    <div class="ms-title">Structural Smells (${smells.length})</div>
    ${smells
      .map((s) => {
        const [label, color] = SMELL_LABEL[s.kind] ?? [s.kind, 'var(--muted)']
        return `<div class="st-wasm-item">
        <div class="st-fn-head">
          ${s.file ? `<span class="sec-cwe">${esc(s.file)}</span>` : ''}
          <span class="st-fn-name">${esc(s.name)}</span>
          <span class="badge badge-tinted" style="--tag-color:${color}">${label}</span>
          ${_decidabilityBadge(s)}
          <span class="st-fn-line ml-auto">línea ${s.line}</span>
        </div>
        <div class="st-wasm-rec">${esc(s.message)}</div>
      </div>`
      })
      .join('')}
  </div>`
}

function _renderNamingSmells(smells: NamingSmell[]): string {
  return `
  <div class="metric-section">
    <div class="ms-title">Naming Smells (${smells.length})</div>
    ${smells
      .map((s) => {
        const [label, color] = NAMING_SMELL_LABEL[s.kind] ?? [s.kind, 'var(--muted)']
        return `<div class="st-wasm-item">
        <div class="st-fn-head">
          ${s.file ? `<span class="sec-cwe">${esc(s.file)}</span>` : ''}
          <span class="st-fn-name">${esc(s.name)}</span>
          <span class="badge badge-tinted" style="--tag-color:${color}">${label}</span>
          <span class="st-fn-line ml-auto">línea ${s.line}</span>
        </div>
        <div class="st-wasm-rec">${esc(s.message)}</div>
      </div>`
      })
      .join('')}
  </div>`
}

// Función propia, no una reutilización de _renderStructuralSmells: esa
// renderiza incondicionalmente "línea ${s.line}", que acá siempre sería
// "línea 0" (smells de arquitectura son de archivo/grafo, no de línea
// puntual) — se leería como un bug. Tampoco hay badge de archivo (`s.file`):
// no existe ese campo, `s.name` ya es la ruta completa por sí sola.
function _renderArchitectureSmells(smells: ArchitectureSmell[]): string {
  return `
  <div class="metric-section">
    <div class="ms-title">Architecture Smells (${smells.length})</div>
    ${smells
      .map((s) => {
        const [label, color] = ARCH_SMELL_LABEL[s.kind] ?? [s.kind, 'var(--muted)']
        return `<div class="st-wasm-item">
        <div class="st-fn-head">
          <span class="st-fn-name">${esc(s.name)}</span>
          <span class="badge badge-tinted" style="--tag-color:${color}">${label}</span>
        </div>
        <div class="st-wasm-rec">${esc(s.message)}</div>
      </div>`
      })
      .join('')}
  </div>`
}

const SEC_SEV_ORDER: Record<string, number> = { High: 0, Medium: 1, Low: 2 }
const SMELL_KIND_ORDER: Record<StructuralSmell['kind'], number> = {
  god_object: 0,
  large_class: 1,
  quadratic_list_membership: 2,
  mutable_default_argument: 3,
  deep_nesting: 4,
  long_function: 5,
  excessive_parameters: 6,
  de_morgan_simplifiable: 7,
  // Primero de todos: es el unico defecto de la lista, el resto es
  // mantenibilidad.
  unreachable_code: -1,
  condition_always_false: -3,
  condition_always_true: -2,
  division_by_zero: -5,
  possible_division_by_zero: -4,
  index_out_of_range: -7,
  possible_index_out_of_range: -6,
  none_dereference: -9,
  possible_none_dereference: -8,
  contradictory_condition: -14,
  tautological_condition: -13,
  duplicated_condition: -12,
  absorbable_condition: -11,
  redundant_condition: -10,
  unbounded_queue: -17,
  unbounded_cache: -16,
  unbounded_task_spawn: -15,
  float_equality_comparison: -20,
  catastrophic_cancellation_step: -19,
  unbounded_convergence_loop: -18,
  undefined_state_transition: -23,
  unhandled_state: -22,
  unreachable_state: -21,
  recursive_template_no_base_case: -26,
  deep_template_recursion: -25,
  linear_op_in_loop: -24,
  unwrap_in_fallible_fn: -29,
  double_type_assertion: -31,
  bare_type_ignore: -32,
  dead_store: -33,
  overwritten_store: -33,
  potential_false_sharing: -34,
  sfinae_instead_of_concepts: -35,
  signed_unsigned_comparison: -55,
  implicit_precision_loss: -50,
  int_pointer_cast: -40,
  missing_implicit_none: -35,
  single_precision_literal_in_double: -50,
  integer_division_to_real: -55,
  char_signedness_assumption: -55,
  factorial_brute_force: -70,
  undefined_grammar_rule: -60,
  unreferenced_grammar_rule: -25,
  ts_ignore_instead_of_expect_error: -30,
  unsafe_block_without_safety_comment: -28,
  unsafe_fn_without_safety_doc: -27,
}
const NAMING_SMELL_KIND_ORDER: Record<NamingSmell['kind'], number> = {
  shadowed_name: 0,
  inconsistent_casing: 1,
  single_letter_name: 2,
}
const ARCH_SMELL_KIND_ORDER: Record<ArchitectureSmell['kind'], number> = {
  circular_dependency: 0,
  unstable_dependency: 1,
  high_efferent_coupling: 2,
}

// `renderBigODistribution` se mudo a `utils/bigo.ts`: lo comparte con el
// Dashboard, asi que no era del panel Static.

// ─── Render proyecto ──────────────────────────────────────────────────────────

function _renderProjectResult(data: StaticProjectResult): void {
  const body = document.getElementById('st-body')!
  const s = data.summary
  const projectFindings = [...(data.security_findings ?? [])].sort(
    (a, b) => SEC_SEV_ORDER[a.severity] - SEC_SEV_ORDER[b.severity],
  )
  const projectSmells = [...(data.structural_smells ?? [])].sort(
    (a, b) => SMELL_KIND_ORDER[a.kind] - SMELL_KIND_ORDER[b.kind],
  )
  const projectNamingSmells = [...(data.naming_smells ?? [])].sort(
    (a, b) => NAMING_SMELL_KIND_ORDER[a.kind] - NAMING_SMELL_KIND_ORDER[b.kind],
  )
  const projectArchSmells = [...(data.architecture_smells ?? [])].sort(
    (a, b) => ARCH_SMELL_KIND_ORDER[a.kind] - ARCH_SMELL_KIND_ORDER[b.kind],
  )

  const candidates = data.wasm_candidates ?? []
  const modernizationByFile = data.modernization_by_file ?? []

  body.innerHTML = `
    ${renderProjectContextBanner()}
    ${data.health ? renderHealthCards(data.health) : ''}

    <div class="st-summary-row">
      ${sc(String(s.total_files ?? 0), 'Archivos')}
      ${sc(String(s.total_functions ?? 0), 'Funciones')}
      ${sc(String(s.total_classes ?? 0), 'Clases')}
      ${sc(String(s.total_imports ?? 0), 'Imports')}
      ${sc(String(s.unused_imports ?? 0), 'No usados', s.unused_imports ? 'var(--warn)' : undefined)}
      ${sc(String(s.wasm_candidates ?? 0), 'WASM candidates', s.wasm_candidates ? 'var(--warn)' : undefined)}
    </div>

    ${projectFindings.length ? _renderSecurityFindings(projectFindings) : ''}

    ${projectSmells.length ? _renderStructuralSmells(projectSmells) : ''}

    ${projectNamingSmells.length ? _renderNamingSmells(projectNamingSmells) : ''}

    ${projectArchSmells.length ? _renderArchitectureSmells(projectArchSmells) : ''}

    ${modernizationByFile.length ? _renderModernizationByFile(modernizationByFile) : ''}

    ${_renderProjectExtraFindings(data)}

    <div id="st-css-usage">${_renderCssUsage()}</div>

    ${renderBigODistribution(s.big_o_distribution ?? {})}

    ${
      candidates.length
        ? `
    <div class="metric-section">
      <div class="ms-title">WASM / Cython candidates</div>
      ${candidates
        .map(
          (c) => `
        <div class="mb-2">
          <div style="font-size:.72rem;font-weight:600;color:var(--info);margin-bottom:4px">${esc(c.file)}</div>
          ${c.hints
            .map(
              (h) => `
            <div class="st-wasm-item" style="margin-left:12px">
              <div class="st-fn-head">
                <span class="st-fn-name">${esc(h.function)}</span>
                <span style="font-size:.65rem;color:var(--warn)">priority ${h.priority}</span>
                <span style="margin-left:auto;font-size:.65rem;color:var(--ok)">${esc(h.estimated_speedup)}</span>
              </div>
            </div>`,
            )
            .join('')}
        </div>
      `,
        )
        .join('')}
    </div>`
        : ''
    }

    <div style="font-size:.7rem;color:var(--muted);text-align:center;padding:8px 0;font-family:var(--mono)">
      Haz clic en Analizar con un archivo seleccionado para ver el detalle completo
    </div>
  `
  wireProjectContextBanner(body)
}

// ─── Loading / Error helpers ─────────────────────────────────────────────────

function _setLoading(on: boolean): void {
  _loading = on
  const btn = document.getElementById('st-run-btn')
  const body = document.getElementById('st-body')
  if (on) {
    btn?.setAttribute('disabled', '')
    if (btn) btn.textContent = 'Analizando...'
    if (body)
      body.innerHTML = `
      <div class="empty">
        Parseando AST...
      </div>`
  } else {
    btn?.removeAttribute('disabled')
    if (btn) btn.textContent = 'Analizar'
  }
}

function _showError(msg: string): void {
  const body = document.getElementById('st-body')
  if (body)
    body.innerHTML = `
    <div class="st-error">${icon('warning', 14)} ${esc(msg)}</div>`
}
