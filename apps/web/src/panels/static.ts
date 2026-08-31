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
  EmpiricalValidationResult,
  MatmulVsNumpyResult,
  MemoryLayoutResult,
  NamingSmell,
  SecurityFinding,
  StaticProjectResult,
  StructuralSmell,
} from '../api/client'
import { api } from '../api/client'
import { state } from '../store/state'
import { renderHealthCards } from '../utils/health'
import { appendLog, toast } from '../utils/helpers'
import { icon, languageBadgeByName } from '../utils/icons'
import { renderProjectContextBanner, wireProjectContextBanner } from '../utils/projectHeader'

// ─── Tipos ────────────────────────────────────────────────────────────────────

interface ParsedFunction {
  name: string
  line: number
  end_line?: number
  loc?: number
  complexity: number
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
  line: number
  current: string
  suggested_target: 'raii_smart_pointer' | 'rust_ownership'
  reasoning: string
  confidence: 'medium' | 'high'
}

interface ModernizationReport {
  candidates: ModernizationCandidate[]
  summary: {
    total: number
    manual_memory_raii: number
    unmatched_allocation: number
    double_release: number
    unsafe_realloc_reassignment: number
    use_after_free: number
  }
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
  type: string
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

interface StaticResult {
  filename: string
  language: string
  asm_syntax?: 'att' | 'intel'
  functions: ParsedFunction[]
  classes: ParsedClass[]
  imports: ParsedImport[]
  exports: Array<{ name: string; line: number }>
  interfaces?: Array<{ name: string; line: number }>
  types?: Array<{ name: string; line: number }>
  dead_code: Array<{ type: string; name?: string; module: string; line: number }>
  call_graph: Array<{ from: string; to: string }>
  wasm_hints: WasmHint[]
  memory_layout?: MemoryLayoutResult
  modernization?: ModernizationReport
  security_findings: SecurityFinding[]
  structural_smells: StructuralSmell[]
  naming_smells: NamingSmell[]
  summary: Record<string, number | string>
  error?: string
}

// ─── Colores Big O ────────────────────────────────────────────────────────────

const BIG_O_COLOR: Record<string, string> = {
  'O(1)': 'var(--ok)',
  'O(log n)': '#8ef5c0',
  'O(n)': 'var(--info)',
  'O(n log n)': 'var(--warn)',
  'O(n²)': 'var(--orange)',
  'O(n³)': 'var(--err)',
  'O(2^n)': 'var(--err)',
}
// ─── Estado del panel ─────────────────────────────────────────────────────────

let _result: StaticResult | null = null
let _loading = false

// Fase 23 — resultado de la validación empírica de matmul, compartido entre
// todas las funciones con `numerical_algorithm_note` (el kernel es genérico,
// no por-función), así que una sola corrida vale para toda la sesión del panel.
let _matmulValidation: EmpiricalValidationResult | null = null
let _matmulValidating = false

// Fase 26 — mismo criterio que `_matmulValidation`, generalizado a los otros
// 2 kernels que el motor ya sabe correr (Zig/bubble-sort, Assembly/suma de
// cuadrados). A diferencia de matmul, estos no cuelgan de un badge detectado
// por-función (no existe un detector "esto parece bubble sort" todavía) — se
// muestran en una sección propia, siempre visible para lenguajes nativos.
let _bubbleSortValidation: EmpiricalValidationResult | null = null
let _bubbleSortValidating = false
let _sumSquaresValidation: EmpiricalValidationResult | null = null
let _sumSquaresValidating = false
let _graphBfsValidation: EmpiricalValidationResult | null = null
let _graphBfsValidating = false
let _fibonacciValidation: EmpiricalValidationResult | null = null
let _fibonacciValidating = false
let _mergesortValidation: EmpiricalValidationResult | null = null
let _mergesortValidating = false

// Fase 26 — primera pieza de "migrar de numpy/pandas/scikit-learn" (pedido
// explícito del usuario): comparación real matmul Fortran vs. numpy, no un
// exponente medido más — por eso vive con su propio estado/render, no
// reusa `_renderKernelValidationRow`.
let _matmulVsNumpyComparison: MatmulVsNumpyResult | null = null
let _matmulVsNumpyComparing = false

// ─── Render principal ─────────────────────────────────────────────────────────

export function renderStaticPanel(): void {
  const el = document.getElementById('static-content')
  if (!el) return

  const f = state.currentFile
  const fileOpts = state.files.map((f) => `<option value="${f.id}">${f.name}</option>`).join('')

  el.innerHTML = `
    <!-- Toolbar -->
    <div class="st-toolbar">
      <select id="st-file-sel" class="diag-sel" style="min-width:200px">
        <option value="">— Selecciona un archivo —</option>
        ${fileOpts}
      </select>
      <button class="btn btn-run btn-sm" id="st-run-btn">Analizar</button>
      <button class="btn btn-ghost btn-sm" id="st-run-project-btn" title="Analizar todos los archivos cargados">Analizar proyecto</button>
      <div style="flex:1"></div>
      <span id="st-lang-badge"></span>
    </div>

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
  // Fase 23/26 — delegado porque los botones viven dentro de #st-body, que
  // se reescribe entero en cada _renderResult (no existen todavía al momento
  // de este addEventListener si se ataran directo a cada botón).
  el.addEventListener('click', (e) => {
    const t = e.target as HTMLElement
    if (t.dataset['validateMatmul'] !== undefined) void _onValidateMatmulClick()
    if (t.dataset['validateBubbleSort'] !== undefined) void _onValidateBubbleSortClick()
    if (t.dataset['validateSumSquares'] !== undefined) void _onValidateSumSquaresClick()
    if (t.dataset['validateGraphBfs'] !== undefined) void _onValidateGraphBfsClick()
    if (t.dataset['validateFibonacci'] !== undefined) void _onValidateFibonacciClick()
    if (t.dataset['validateMergesort'] !== undefined) void _onValidateMergesortClick()
    if (t.dataset['compareMatmulNumpy'] !== undefined) void _onCompareMatmulNumpyClick()
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

    <!-- Security findings -->
    ${r.security_findings?.length ? _renderSecurityFindings(r.security_findings) : ''}

    <!-- Structural smells -->
    ${r.structural_smells?.length ? _renderStructuralSmells(r.structural_smells) : ''}

    <!-- Naming smells -->
    ${r.naming_smells?.length ? _renderNamingSmells(r.naming_smells) : ''}

    <!-- Big O table -->
    ${_renderBigOTable(r.functions)}

    <!-- Algorithm Validation Engine — kernels Zig/Assembly (Fase 26) -->
    ${['c', 'cpp', 'fortran', 'assembly'].includes(r.language) ? _renderAlgorithmValidationEngine() : ''}

    <!-- Funciones -->
    ${_renderFunctions(r.functions)}

    <!-- Clases -->
    ${r.classes.length ? _renderClasses(r.classes, r.language) : ''}

    <!-- Imports / Dead code -->
    ${_renderImports(r.imports, r.dead_code)}

    <!-- Interfaces / Types (TS) -->
    ${r.interfaces?.length ? _renderInterfaces(r.interfaces, r.types ?? []) : ''}

    <!-- Call graph -->
    ${r.call_graph.length ? _renderCallGraph(r.call_graph) : ''}

    <!-- WASM hints -->
    ${r.wasm_hints.length ? _renderWasmHints(r.wasm_hints) : ''}

    <!-- Memory layout (C/C++, Fase 23) -->
    ${r.memory_layout?.variables.length ? _renderMemoryLayout(r.memory_layout) : ''}

    <!-- Modernization Intelligence (C/C++, Fase 25) -->
    ${r.modernization?.candidates.length ? _renderModernization(r.modernization) : ''}

    <!-- Assembly breakdown (Fase 19) -->
    ${r.language === 'assembly' ? _renderAsmBreakdown(r.functions, r.asm_syntax) : ''}
  `
}

// ─── Secciones de render ──────────────────────────────────────────────────────

function _renderSummaryCards(r: StaticResult): string {
  const s = r.summary as Record<string, number>
  const avg = typeof s.avg_complexity === 'number' ? s.avg_complexity : 0
  const avgColor = avg < 5 ? 'var(--ok)' : avg < 10 ? 'var(--warn)' : 'var(--err)'

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
          f.is_recursive
            ? `<span class="bigo-recursion ${f.is_tail_recursive ? 'bigo-recursion-tail' : 'bigo-recursion-notail'}" title="${esc([f.recursion_note, f.induction_note, f.recurrence].filter(Boolean).join('\n\n'))}">${f.is_tail_recursive ? 'tail-call' : 'recursión'}</span>`
            : ''
        }
        ${f.regex_class ? `<span class="bigo-cs-badge bigo-regex" title="${esc(f.regex_note ?? '')}">Regex</span>` : ''}
        ${f.grammar_class ? `<span class="bigo-cs-badge bigo-grammar" title="${esc(f.grammar_note ?? '')}">CFG</span>` : ''}
        ${f.graph_traversal ? `<span class="bigo-cs-badge bigo-graph" title="${esc(f.graph_traversal_note ?? '')}">${esc(f.graph_traversal)}</span>` : ''}
        ${f.semantic_analysis_class ? `<span class="bigo-cs-badge bigo-semantic" title="${esc(f.semantic_analysis_note ?? '')}">Type-1?</span>` : ''}
        ${f.data_structure ? `<span class="bigo-cs-badge bigo-datastruct" title="${esc(f.data_structure_note ?? '')}">${esc(f.data_structure)}</span>` : ''}
        ${f.is_pure ? `<span class="bigo-cs-badge bigo-purity" title="${esc(f.purity_note ?? '')}">pure</span>` : ''}
        ${f.vectorization_note ? `<span class="bigo-cs-badge bigo-vectorization" title="${esc(f.vectorization_note)}">SIMD?</span>` : ''}
        ${f.numerical_algorithm_note ? `<span class="bigo-cs-badge bigo-numerical" title="${esc(f.numerical_algorithm_note)}">Matrix?</span> ${_renderMatmulValidationControl()}` : ''}
        ${f.blas_lapack_calls?.length ? `<span class="bigo-cs-badge bigo-blas" title="${esc(`Detected BLAS/LAPACK calls: ${f.blas_lapack_calls.join(', ')}`)}">BLAS/LAPACK</span>` : ''}
      </td>
      <td><span class="bigo-badge" style="color:${color};border-color:${color}"${f.combinatorics_note ? ` title="${esc(f.combinatorics_note)}"` : ''}>${esc(f.big_o)}</span></td>
      <td class="bigo-thetaomega">${f.big_o_theta ? esc(f.big_o_theta) : '—'} / ${f.big_o_omega ? esc(f.big_o_omega) : '—'}</td>
      <td class="bigo-reason">${esc(f.big_o_reason)}</td>
      <td class="bigo-space">${f.space_complexity ? `<span class="bigo-badge" style="color:${BIG_O_COLOR[f.space_complexity] ?? 'var(--muted)'};border-color:${BIG_O_COLOR[f.space_complexity] ?? 'var(--muted)'}" title="${esc(f.space_reason ?? '')}">${esc(f.space_complexity)}</span>` : '—'}</td>
      <td class="bigo-cc" style="color:${f.complexity >= 10 ? 'var(--err)' : f.complexity >= 5 ? 'var(--warn)' : 'var(--ok)'}">${f.complexity}</td>
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

async function _onValidateMatmulClick(): Promise<void> {
  if (_matmulValidating) return
  _matmulValidating = true
  if (_result) _renderResult(_result)
  try {
    _matmulValidation = await api.validateMatmulBigO()
  } catch (e) {
    toast('Error: ' + (e as Error).message, 'err')
  } finally {
    _matmulValidating = false
    if (_result) _renderResult(_result)
  }
}

type ValidateDataAttr =
  | 'data-validate-bubble-sort'
  | 'data-validate-sum-squares'
  | 'data-validate-graph-bfs'
  | 'data-validate-fibonacci'
  | 'data-validate-mergesort'

/** Fase 26 — misma pieza visual que `_renderMatmulValidationControl`
 * (botón → spinner → badge medido), parametrizada para los kernels que ya
 * corren en Rust pero que hasta ahora no tenían ningún control en el
 * frontend — el backend (endpoint, cliente Python, tests) ya estaba
 * completo y verde, solo faltaba esto.
 *
 * `formatMeasured` es opcional porque el kernel de Fibonacci (`fib_bench.rs`)
 * no mide un exponente `n^k` como los otros 4 — mide la BASE de un
 * crecimiento exponencial (`baseⁿ`), así que necesita su propio formato de
 * badge (`base≈X.XX`, no `n^X.XX`) para no mentir sobre qué significa el
 * número. Default: el formato `n^X.XX` que sí aplica a los 4 kernels
 * polinomiales. */
function _renderKernelValidationRow(
  label: string,
  dataAttr: ValidateDataAttr,
  validating: boolean,
  result: EmpiricalValidationResult | null,
  predictedExponent: number,
  tooltip: string,
  formatMeasured: (v: number) => string = (v) => `medido: n^${v.toFixed(2)}`,
): string {
  let control: string
  if (validating) {
    control = `<span class="bigo-cs-badge bigo-empirical"><span class="up-spinner"></span> Validando...</span>`
  } else if (result) {
    if (!result.available || result.estimated_exponent == null) {
      control = `<span class="bigo-cs-badge bigo-empirical bigo-empirical-off" title="${esc(result.note)}">sin validar</span>`
    } else {
      const close = Math.abs(result.estimated_exponent - predictedExponent) < 0.3
      control = `<span class="bigo-cs-badge bigo-empirical ${close ? 'bigo-empirical-ok' : 'bigo-empirical-off'}" title="${esc(result.note)}">${esc(formatMeasured(result.estimated_exponent))}</span> <button class="btn btn-ghost btn-sm" ${dataAttr} title="Volver a correr la validación">↺</button>`
    }
  } else {
    control = `<button class="btn btn-ghost btn-sm" ${dataAttr} title="${esc(tooltip)}">Validar empíricamente</button>`
  }
  return `<div class="st-fn-head"><span class="st-fn-name">${esc(label)}</span> ${control}</div>`
}

function _renderAlgorithmValidationEngine(): string {
  return `
  <div class="metric-section">
    <div class="ms-title">Algorithm Validation Engine (Fase 26)</div>
    <div class="st-mem-note">Compila y corre kernels reales que Sythrall mismo escribe (nunca tu código) a varios tamaños para medir el exponente de crecimiento empírico — mismo patrón que el botón de matmul del Big-O table, generalizado a 2 lenguajes más.</div>
    ${_renderKernelValidationRow(
      'Bubble sort (Zig) — predicho O(n²)',
      'data-validate-bubble-sort',
      _bubbleSortValidating,
      _bubbleSortValidation,
      2,
      'Compila y corre un bubble sort real en Zig (escrito por Sythrall, no tu código) para medir el exponente de crecimiento real',
    )}
    ${_renderKernelValidationRow(
      'Suma de cuadrados (Assembly x86) — predicho O(n)',
      'data-validate-sum-squares',
      _sumSquaresValidating,
      _sumSquaresValidation,
      1,
      'Ensambla y corre una suma de cuadrados escrita a mano en Assembly x86 (AT&T, cdecl) para medir el exponente de crecimiento real',
    )}
    ${_renderKernelValidationRow(
      'Recorrido de grafos BFS (Zig) — predicho O(V+E)',
      'data-validate-graph-bfs',
      _graphBfsValidating,
      _graphBfsValidation,
      1,
      'Compila y corre un BFS real en Zig sobre un grafo disperso de grado fijo (escrito por Sythrall, no tu código) para medir el exponente de crecimiento real',
    )}
    ${_renderKernelValidationRow(
      'Fibonacci recursivo ingenuo (Fortran) — predicho exponencial Θ(φⁿ)',
      'data-validate-fibonacci',
      _fibonacciValidating,
      _fibonacciValidation,
      1.618,
      'Compila y corre un Fibonacci recursivo sin memoizar en Fortran (escrito por Sythrall, no tu código) para medir la base real de crecimiento exponencial',
      (v) => `medido: base≈${v.toFixed(2)} (φ≈1.618)`,
    )}
    ${_renderKernelValidationRow(
      'Mergesort bottom-up (Assembly x86) — predicho O(n log n)',
      'data-validate-mergesort',
      _mergesortValidating,
      _mergesortValidation,
      1,
      'Ensambla y corre un mergesort bottom-up iterativo escrito a mano en Assembly x86 (escrito por Sythrall, no tu código) para medir el exponente de crecimiento real',
    )}
    <div class="ms-title" style="margin-top:.75rem;font-size:.75rem">Migración numpy/pandas/scikit-learn — pieza 1: matmul</div>
    ${_renderMatmulVsNumpyComparison()}
  </div>`
}

/** Fase 26 — primera pieza de "migrar de numpy/pandas/scikit-learn"
 * (pedido explícito del usuario, 2026-08-31): a diferencia de los kernels
 * de arriba, esto no mide UN exponente — corre el kernel Fortran de matmul
 * (ya validado) Y numpy real, mismos tamaños/datos, y muestra ambos
 * tiempos lado a lado con una nota de comparación honesta (numpy gana por
 * mucho acá, y eso se muestra tal cual, no se esconde). Por eso tiene su
 * propio render en vez de reusar `_renderKernelValidationRow`. */
function _renderMatmulVsNumpyComparison(): string {
  if (_matmulVsNumpyComparing) {
    return `<div class="st-fn-head"><span class="st-fn-name">matmul: Fortran vs. numpy</span> <span class="bigo-cs-badge bigo-empirical"><span class="up-spinner"></span> Comparando...</span></div>`
  }
  if (!_matmulVsNumpyComparison) {
    return `<div class="st-fn-head"><span class="st-fn-name">matmul: Fortran vs. numpy</span> <button class="btn btn-ghost btn-sm" data-compare-matmul-numpy title="Corre el kernel Fortran de matmul Y numpy real (mismos tamaños/datos) y compara los tiempos honestamente">Comparar con numpy</button></div>`
  }
  const c = _matmulVsNumpyComparison
  const rows = c.fortran.measurements
    .map((f) => {
      const n = c.numpy.measurements.find((m) => m.n === f.n)
      return `<tr><td>${f.n}</td><td>${f.seconds.toFixed(6)}s</td><td>${n ? n.seconds.toFixed(6) + 's' : '—'}</td></tr>`
    })
    .join('')
  return `<div class="st-wasm-item">
    <div class="st-fn-head">
      <span class="st-fn-name">matmul: Fortran vs. numpy</span>
      <button class="btn btn-ghost btn-sm" data-compare-matmul-numpy title="Volver a correr la comparación">↺</button>
    </div>
    ${
      c.fortran.available && c.numpy.available
        ? `<table class="bigo-table" style="margin:.4rem 0">
      <thead><tr><th>n</th><th>Fortran</th><th>numpy</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>`
        : ''
    }
    <div class="st-wasm-rec">${esc(c.comparison_note)}</div>
  </div>`
}

async function _onValidateBubbleSortClick(): Promise<void> {
  if (_bubbleSortValidating) return
  _bubbleSortValidating = true
  if (_result) _renderResult(_result)
  try {
    _bubbleSortValidation = await api.validateBubbleSortBigO()
  } catch (e) {
    toast('Error: ' + (e as Error).message, 'err')
  } finally {
    _bubbleSortValidating = false
    if (_result) _renderResult(_result)
  }
}

async function _onValidateSumSquaresClick(): Promise<void> {
  if (_sumSquaresValidating) return
  _sumSquaresValidating = true
  if (_result) _renderResult(_result)
  try {
    _sumSquaresValidation = await api.validateSumSquaresBigO()
  } catch (e) {
    toast('Error: ' + (e as Error).message, 'err')
  } finally {
    _sumSquaresValidating = false
    if (_result) _renderResult(_result)
  }
}

async function _onValidateGraphBfsClick(): Promise<void> {
  if (_graphBfsValidating) return
  _graphBfsValidating = true
  if (_result) _renderResult(_result)
  try {
    _graphBfsValidation = await api.validateGraphBfsBigO()
  } catch (e) {
    toast('Error: ' + (e as Error).message, 'err')
  } finally {
    _graphBfsValidating = false
    if (_result) _renderResult(_result)
  }
}

async function _onValidateFibonacciClick(): Promise<void> {
  if (_fibonacciValidating) return
  _fibonacciValidating = true
  if (_result) _renderResult(_result)
  try {
    _fibonacciValidation = await api.validateFibonacciExponential()
  } catch (e) {
    toast('Error: ' + (e as Error).message, 'err')
  } finally {
    _fibonacciValidating = false
    if (_result) _renderResult(_result)
  }
}

async function _onValidateMergesortClick(): Promise<void> {
  if (_mergesortValidating) return
  _mergesortValidating = true
  if (_result) _renderResult(_result)
  try {
    _mergesortValidation = await api.validateMergesortBigO()
  } catch (e) {
    toast('Error: ' + (e as Error).message, 'err')
  } finally {
    _mergesortValidating = false
    if (_result) _renderResult(_result)
  }
}

async function _onCompareMatmulNumpyClick(): Promise<void> {
  if (_matmulVsNumpyComparing) return
  _matmulVsNumpyComparing = true
  if (_result) _renderResult(_result)
  try {
    _matmulVsNumpyComparison = await api.compareMatmulVsNumpy()
  } catch (e) {
    toast('Error: ' + (e as Error).message, 'err')
  } finally {
    _matmulVsNumpyComparing = false
    if (_result) _renderResult(_result)
  }
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
          <span>CC: <b style="color:${f.complexity >= 10 ? 'var(--err)' : f.complexity >= 5 ? 'var(--warn)' : 'var(--ok)'}">${f.complexity}</b></span>
          ${f.loc ? `<span>LOC: ${f.loc}</span>` : ''}
          ${f.decorators?.length ? `<span>@${f.decorators.map(esc).join(' @')}</span>` : ''}
          ${f.calls?.length ? `<span>llama: ${f.calls.slice(0, 4).map(esc).join(', ')}${f.calls.length > 4 ? '…' : ''}</span>` : ''}
        </div>
      </div>`
      })
      .join('')}
  </div>`
}

function _renderClasses(classes: ParsedClass[], lang: string): string {
  const title = lang === 'c' ? 'Structs/Unions' : lang === 'cpp' ? 'Clases C++' : 'Clases'
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
          ${c.kind ? `<span style="font-size:.6rem;color:var(--muted);font-family:var(--mono)">${c.kind}</span>` : ''}
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
          <span class="st-import-type">${imp.type.replace('_import', '').replace('esm_', '')}</span>
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
          <span style="color:var(--info)">${esc(i.name)}</span>
          <span class="st-import-line">:${i.line}</span>
        </div>`,
        )
        .join('')}
      ${types
        .map(
          (t) =>
            `<div class="st-import-chip" style="border-color:var(--purple)">
          <span class="st-import-type">type</span>
          <span style="color:var(--purple)">${esc(t.name)}</span>
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
          <span style="font-size:.65rem;color:${color};font-family:var(--mono)">${label}</span>
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
        <div class="st-mem-region-title" style="color:var(--orange)">Heap — allocations (${layout.allocations.length})</div>
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
    <div class="st-mem-note" title="${esc(report.note)}">Detectado sobre allocation sites ya calculados — propone, no convierte código automáticamente</div>
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
  other: 'var(--muted)',
}

/** Fase 19, 3er bullet — badge de una línea junto al nombre del
 * procedimiento; el "por qué" completo vive en el tooltip (`title`), mismo
 * convención pedagógica que el resto del motor (badges "Type-1?"/"Matrix?"). */
function _renderStackFrameBadge(frame: StackFrameInfo): string {
  const bytesNote = frame.local_stack_bytes ? ` · ${frame.local_stack_bytes}B locales` : ''
  if (frame.has_standard_prologue && frame.has_standard_epilogue) {
    return `<span class="bigo-cs-badge" style="color:var(--ok)" title="${esc(frame.explanation)}">Frame: estándar${bytesNote}</span>`
  }
  if (frame.is_leaf_function) {
    return `<span class="bigo-cs-badge" style="color:var(--muted)" title="${esc(frame.explanation)}">Frame: omitido (leaf)</span>`
  }
  return `<span class="bigo-cs-badge" style="color:var(--warn)" title="${esc(frame.explanation)}">Frame: no estándar</span>`
}

/** Fase 19 — pattern-matching sobre texto (`asmparse.rs`), no un
 * disassembler real. Un `.metric-section` por procedimiento (label
 * delimitado), con sus instrucciones coloreadas por categoría y sus
 * registros usados como chips — mismo criterio de lista que
 * `_renderMemoryLayout`, sin diagrama animado nuevo. */
function _renderAsmBreakdown(functions: ParsedFunction[], syntax?: 'att' | 'intel'): string {
  const procs = functions.filter((f) => f.instructions?.length)
  if (!procs.length) return ''

  return `
  <div class="metric-section">
    <div class="ms-title">Assembly x86-64 — sintaxis detectada: ${syntax === 'att' ? 'AT&T' : 'Intel'}</div>
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
            ? `<div class="st-import-grid">${fn.registers_used.map((r) => `<span class="st-import-chip"><span class="st-import-name">%${esc(r)}</span></span>`).join('')}</div>`
            : ''
        }
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

export function securitySeverityColor(s: string): string {
  return s === 'High' ? 'var(--err)' : s === 'Medium' ? 'var(--warn)' : 'var(--muted)'
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
          <span class="st-fn-line" style="margin-left:auto">línea ${f.line}</span>
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

export const SMELL_LABEL: Record<StructuralSmell['kind'], [string, string]> = {
  long_function: ['Función larga', 'var(--warn)'],
  excessive_parameters: ['Exceso de parámetros', 'var(--warn)'],
  deep_nesting: ['Anidamiento profundo', 'var(--warn)'],
  large_class: ['Clase grande', 'var(--orange)'],
  god_object: ['God object', 'var(--err)'],
  quadratic_list_membership: ['O(n²) oculto (list membership)', 'var(--orange)'],
  de_morgan_simplifiable: ['Simplificable (De Morgan)', 'var(--info)'],
}

// Ni structural_smells ni naming_smells traen un campo `severity` propio (a
// diferencia de SecurityFinding) — el color ya asignado arriba/abajo por
// `kind` es el único juicio de gravedad que el sistema ya expresa en algún
// lado, así que el Dashboard lo reusa para el widget "Findings by Severity"
// en vez de inventar una escala nueva sin verla reflejada en ningún otro lado.
export function smellColorSeverity(color: string): 'High' | 'Medium' | 'Low' {
  if (color === 'var(--err)') return 'High'
  if (color === 'var(--orange)') return 'Medium'
  return 'Low'
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
          <span style="font-size:.65rem;color:${color};font-family:var(--mono)">${label}</span>
          <span class="st-fn-line" style="margin-left:auto">línea ${s.line}</span>
        </div>
        <div class="st-wasm-rec">${esc(s.message)}</div>
      </div>`
      })
      .join('')}
  </div>`
}

export const NAMING_SMELL_LABEL: Record<NamingSmell['kind'], [string, string]> = {
  shadowed_name: ['Nombre tapado', 'var(--err)'],
  inconsistent_casing: ['Casing inconsistente', 'var(--orange)'],
  single_letter_name: ['Nombre de una letra', 'var(--warn)'],
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
          <span style="font-size:.65rem;color:${color};font-family:var(--mono)">${label}</span>
          <span class="st-fn-line" style="margin-left:auto">línea ${s.line}</span>
        </div>
        <div class="st-wasm-rec">${esc(s.message)}</div>
      </div>`
      })
      .join('')}
  </div>`
}

export const ARCH_SMELL_LABEL: Record<ArchitectureSmell['kind'], [string, string]> = {
  circular_dependency: ['Dependencia circular', 'var(--err)'],
  unstable_dependency: ['Dependencia inestable', 'var(--orange)'],
  high_efferent_coupling: ['Alto acoplamiento eferente', 'var(--warn)'],
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
          <span style="font-size:.65rem;color:${color};font-family:var(--mono)">${label}</span>
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
  deep_nesting: 3,
  long_function: 4,
  excessive_parameters: 5,
  de_morgan_simplifiable: 6,
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

// ─── Big O distribution (reusada por Dashboard) ──────────────────────────────

const BIG_O_ORDER = ['O(1)', 'O(log n)', 'O(n)', 'O(n log n)', 'O(n²)', 'O(n³)', 'O(2^n)']

export function renderBigODistribution(
  dist: Record<string, number>,
  title = 'Distribución Big O del proyecto',
): string {
  const entries = Object.entries(dist)
  if (!entries.length) return ''
  const max = Math.max(...Object.values(dist))
  const rows = entries
    .sort((a, b) => BIG_O_ORDER.indexOf(a[0]) - BIG_O_ORDER.indexOf(b[0]))
    .map(([bigo, count]) => {
      const color = BIG_O_COLOR[bigo] ?? 'var(--muted)'
      const pct = Math.round((count / max) * 100)
      return `<div class="bigo-dist-row">
        <span class="bigo-badge" style="color:${color};border-color:${color};min-width:80px">${esc(bigo)}</span>
        <div class="bigo-dist-bar-wrap">
          <div class="bigo-dist-bar" style="width:${pct}%;background:${color}"></div>
        </div>
        <span style="font-family:var(--mono);font-size:.65rem;color:var(--muted)">${count} fn${count > 1 ? 's' : ''}</span>
      </div>`
    })
    .join('')
  return `
  <div class="metric-section">
    <div class="ms-title">${esc(title)}</div>
    <div style="padding:4px 0">${rows}</div>
  </div>`
}

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

    ${renderBigODistribution(s.big_o_distribution ?? {})}

    ${
      candidates.length
        ? `
    <div class="metric-section">
      <div class="ms-title">WASM / Cython candidates</div>
      ${candidates
        .map(
          (c) => `
        <div style="margin-bottom:8px">
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

function esc(s: string | number | undefined): string {
  if (s == null) return ''
  return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
}
