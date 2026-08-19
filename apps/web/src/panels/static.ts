// ══════════════════════════════════════════
//  Sythrall — Panel Análisis Estático
//  Mismo patrón que panels/ml.ts y panels/upload.ts
//  Sin IA. Parsers: Python AST, tree-sitter C/C++, regex TS/JS
// ══════════════════════════════════════════

import type { SecurityFinding, StaticProjectResult, StructuralSmell } from '../api/client'
import { api } from '../api/client'
import { state } from '../store/state'
import { renderHealthCards } from '../utils/health'
import { appendLog, toast } from '../utils/helpers'
import { icon, languageBadgeByName } from '../utils/icons'

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
  space_complexity?: string
  space_reason?: string
  is_recursive?: boolean
  is_tail_recursive?: boolean
  recursion_note?: string | null
  regex_class?: string | null
  regex_note?: string | null
  grammar_class?: string | null
  grammar_note?: string | null
  graph_traversal?: string | null
  graph_traversal_note?: string | null
  calls?: string[]
  is_async?: boolean
  args?: string[]
  decorators?: string[]
}

interface ParsedClass {
  name: string
  line: number
  bases?: string[]
  methods?: Array<{ name: string; line: number }>
  kind?: string
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
  functions: ParsedFunction[]
  classes: ParsedClass[]
  imports: ParsedImport[]
  exports: Array<{ name: string; line: number }>
  interfaces?: Array<{ name: string; line: number }>
  types?: Array<{ name: string; line: number }>
  dead_code: Array<{ type: string; name?: string; module: string; line: number }>
  call_graph: Array<{ from: string; to: string }>
  wasm_hints: WasmHint[]
  security_findings: SecurityFinding[]
  structural_smells: StructuralSmell[]
  summary: Record<string, number | string>
  error?: string
}

// ─── Colores Big O ────────────────────────────────────────────────────────────

const BIG_O_COLOR: Record<string, string> = {
  'O(1)': 'var(--ok)',
  'O(log n)': '#8ef5c0',
  'O(n)': 'var(--info)',
  'O(n log n)': 'var(--warn)',
  'O(n²)': '#ff8a00',
  'O(n³)': 'var(--err)',
  'O(2^n)': 'var(--err)',
}
// ─── Estado del panel ─────────────────────────────────────────────────────────

let _result: StaticResult | null = null
let _loading = false

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
            Selecciona un archivo y haz clic en Analizar
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
}

// ─── Eventos ──────────────────────────────────────────────────────────────────

function _attachStaticEvents(el: HTMLElement): void {
  el.querySelector('#st-run-btn')?.addEventListener('click', () => _runSingle())
  el.querySelector('#st-run-project-btn')?.addEventListener('click', () => _runProject())
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

/** El Dashboard muestra las mismas 4 tarjetas de Project Health — un solo
 * fetch acá las actualiza ahí también, sin pedir el análisis dos veces. */
function _syncDashboardHealth(health: StaticProjectResult['health']): void {
  state.results.projectHealth = health
  import('./dashboard').then((m) => m.renderProjectHealth())
}

async function _runProject(): Promise<void> {
  // Sin archivos cargados a mano — si hay proyecto activo, se analiza directo
  // del disco (mismo patrón que Issues, ver analyzeAllFiles en components/app.ts).
  if (!state.files.length && state.activeProjectId) {
    _setLoading(true)
    try {
      const data = await api.staticParseProjectById(state.activeProjectId)
      _renderProjectResult(data)
      _syncDashboardHealth(data.health)
      appendLog('ok', `Proyecto activo: ${data.summary.total_files} archivos analizados`, 'be')
    } catch (e) {
      _showError((e as Error).message)
    } finally {
      _setLoading(false)
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
    _syncDashboardHealth(data.health)
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

    <!-- Big O table -->
    ${_renderBigOTable(r.functions)}

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
            ? `<span class="bigo-recursion ${f.is_tail_recursive ? 'bigo-recursion-tail' : 'bigo-recursion-notail'}" title="${esc(f.recursion_note ?? '')}">${f.is_tail_recursive ? 'tail-call' : 'recursión'}</span>`
            : ''
        }
        ${f.regex_class ? `<span class="bigo-cs-badge bigo-regex" title="${esc(f.regex_note ?? '')}">Regex</span>` : ''}
        ${f.grammar_class ? `<span class="bigo-cs-badge bigo-grammar" title="${esc(f.grammar_note ?? '')}">CFG</span>` : ''}
        ${f.graph_traversal ? `<span class="bigo-cs-badge bigo-graph" title="${esc(f.graph_traversal_note ?? '')}">${esc(f.graph_traversal)}</span>` : ''}
      </td>
      <td><span class="bigo-badge" style="color:${color};border-color:${color}">${esc(f.big_o)}</span></td>
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
    p >= 5 ? ['Crítico', 'var(--err)'] : p >= 3 ? ['Alto', '#ff8a00'] : ['Medio', 'var(--warn)']

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

function _renderSecurityFindings(findings: SecurityFinding[]): string {
  const sevColor = (s: string) => (s === 'High' ? 'var(--err)' : s === 'Medium' ? 'var(--warn)' : 'var(--muted)')
  const confLabel = (c: string) => (c === 'High' ? 'Alta' : c === 'Medium' ? 'Media' : 'Baja')

  return `
  <div class="metric-section">
    <div class="ms-title">${icon('shield', 14)} Security & Taint (${findings.length}) <span class="sec-disclaimer">— patrones heurísticos, no un reemplazo de SAST</span></div>
    ${findings
      .map((f) => {
        const color = sevColor(f.severity)
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

const SMELL_LABEL: Record<StructuralSmell['kind'], [string, string]> = {
  long_function: ['Función larga', 'var(--warn)'],
  excessive_parameters: ['Exceso de parámetros', 'var(--warn)'],
  deep_nesting: ['Anidamiento profundo', 'var(--warn)'],
  large_class: ['Clase grande', '#ff8a00'],
  god_object: ['God object', 'var(--err)'],
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

const SEC_SEV_ORDER: Record<string, number> = { High: 0, Medium: 1, Low: 2 }
const SMELL_KIND_ORDER: Record<StructuralSmell['kind'], number> = {
  god_object: 0,
  large_class: 1,
  deep_nesting: 2,
  long_function: 3,
  excessive_parameters: 4,
}

// ─── Render proyecto ──────────────────────────────────────────────────────────

function _renderProjectResult(data: StaticProjectResult): void {
  const body = document.getElementById('st-body')!
  const s = data.summary
  const dist = s.big_o_distribution ?? {}
  const projectFindings = [...(data.security_findings ?? [])].sort(
    (a, b) => SEC_SEV_ORDER[a.severity] - SEC_SEV_ORDER[b.severity],
  )
  const projectSmells = [...(data.structural_smells ?? [])].sort(
    (a, b) => SMELL_KIND_ORDER[a.kind] - SMELL_KIND_ORDER[b.kind],
  )

  const distRows = Object.entries(dist)
    .sort((a, b) => {
      const order = ['O(1)', 'O(log n)', 'O(n)', 'O(n log n)', 'O(n²)', 'O(n³)', 'O(2^n)']
      return order.indexOf(a[0]) - order.indexOf(b[0])
    })
    .map(([bigo, count]) => {
      const color = BIG_O_COLOR[bigo] ?? 'var(--muted)'
      const max = Math.max(...Object.values(dist))
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

  const candidates = data.wasm_candidates ?? []

  body.innerHTML = `
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

    ${
      distRows
        ? `
    <div class="metric-section">
      <div class="ms-title">Distribución Big O del proyecto</div>
      <div style="padding:4px 0">${distRows}</div>
    </div>`
        : ''
    }

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
