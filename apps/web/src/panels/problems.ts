// ══════════════════════════════════════════
//  Sythrall — Problems Panel + Live Metrics Bar
//  v4.3: VSCode-style problems + editor metrics
//  Wireado en editor.ts (loadFileInEditor → initLiveMetrics, applyMarkers →
//  updateProblems). Renderiza en su propio sub-tab del panel derecho
//  (#problems-content, rp-tab "problems") — no comparte contenedor con el
//  sub-tab "Análisis" (#analysis-content, panels/analysis.ts).
// ══════════════════════════════════════════

import type { NamingSmell, SecurityFinding, StructuralSmell } from '../api/client'
import { getApiBase } from '../store/state'
import type { CodeFile, Issue } from '../types'
import { icon, languageBadge } from '../utils/icons'
import { NAMING_SMELL_LABEL, SMELL_LABEL, securitySeverityColor } from './static'

// ─── Estado ───────────────────────────────────────────────────────────────────

let _metricsTimer: ReturnType<typeof setTimeout> | null = null
let _currentFile = ''
const _initialized = false

const METRICS_DEBOUNCE_MS = 600

// ── Colores Big-O ─────────────────────────────────────────────────────────────
const BIGO_COLOR: Record<string, string> = {
  'O(1)': 'var(--ok)',
  'O(log n)': 'var(--ok)',
  'O(n)': 'var(--warn)',
  'O(n log n)': 'var(--warn)',
  'O(n²)': 'var(--err)',
  'O(n³)': 'var(--err)',
  'O(2^n)': 'var(--err)',
  '?': 'var(--muted)',
}

// ── CC color ─────────────────────────────────────────────────────────────────
function ccColor(cc: number): string {
  if (cc <= 5) return 'var(--ok)'
  if (cc <= 10) return 'var(--warn)'
  if (cc <= 20) return 'var(--orange)'
  return 'var(--err)'
}

// ══════════════════════════════════════════
//  LIVE METRICS BAR
//  Se inyecta encima de .editor-bar
// ══════════════════════════════════════════

export function initLiveMetrics(filename: string, content: string): void {
  _currentFile = filename
  _ensureMetricsBar()
  _scheduleMetrics(filename, content)
}

export function updateLiveMetricsContent(filename: string, content: string): void {
  _currentFile = filename
  _scheduleMetrics(filename, content)
}

function _scheduleMetrics(filename: string, content: string): void {
  if (_metricsTimer) clearTimeout(_metricsTimer)
  _metricsTimer = setTimeout(() => _fetchMetrics(filename, content), METRICS_DEBOUNCE_MS)
}

function _ensureMetricsBar(): void {
  if (document.getElementById('live-metrics-bar')) return

  const bar = document.createElement('div')
  bar.id = 'live-metrics-bar'
  bar.className = 'lm-bar'
  bar.innerHTML = `
    <span class="lm-item" id="lm-loc"       title="Lines of code">—</span>
    <span class="lm-sep">·</span>
    <span class="lm-item" id="lm-functions" title="Functions">—</span>
    <span class="lm-sep">·</span>
    <span class="lm-item" id="lm-imports"   title="Imports">—</span>
    <span class="lm-sep">·</span>
    <span class="lm-item" id="lm-cc"        title="Avg cyclomatic complexity">CC —</span>
    <span class="lm-sep">·</span>
    <span class="lm-item" id="lm-bigo"      title="Big-O worst case">—</span>
    <span class="lm-sep">·</span>
    <span class="lm-item lm-parse" id="lm-parse" title="Parse time">—ms</span>
    <span class="lm-sep">·</span>
    <span class="lm-item lm-lang" id="lm-lang">—</span>
    <span class="lm-safe" id="lm-safe" style="display:none" title="Safe mode: AST failed, using regex fallback">${icon('warning', 11)} safe</span>
  `

  // Insertar ANTES del panel de editor
  const editorPanel = document.getElementById('panel-editor')
  const tabsBar = document.getElementById('file-tabs-bar')
  const insertBefore = tabsBar ?? editorPanel?.querySelector('.editor-bar') ?? editorPanel
  insertBefore?.parentElement?.insertBefore(bar, insertBefore)
}

async function _fetchMetrics(filename: string, content: string): Promise<void> {
  if (!content.trim()) {
    _clearMetrics()
    return
  }

  try {
    const res = await fetch(getApiBase() + '/metrics/live', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ filename, content }),
      signal: AbortSignal.timeout(3000),
    })
    if (!res.ok) return
    const d = (await res.json()) as LiveMetricsData
    _renderMetrics(d)
  } catch {
    // Silencioso — no interrumpir al usuario
  }
}

function _renderMetrics(d: LiveMetricsData): void {
  _setMetric('lm-loc', `${d.loc} líneas`)
  _setMetric('lm-functions', `${d.functions} fn`)
  _setMetric('lm-imports', `${d.imports} imp`)
  _setMetric('lm-cc', `CC ${d.avg_cc}`, ccColor(d.avg_cc))
  _setMetric('lm-bigo', d.big_o_worst, BIGO_COLOR[d.big_o_worst] ?? 'var(--muted)')
  _setMetric('lm-parse', `${d.ms}ms`)
  _setMetric('lm-lang', d.language)

  const safe = document.getElementById('lm-safe')
  if (safe) safe.style.display = d.safe_mode ? '' : 'none'
}

function _clearMetrics(): void {
  ;['lm-loc', 'lm-functions', 'lm-imports', 'lm-cc', 'lm-bigo', 'lm-parse', 'lm-lang'].forEach((id) => {
    _setMetric(id, '—')
  })
}

function _setMetric(id: string, text: string, color?: string): void {
  const el = document.getElementById(id)
  if (!el) return
  el.textContent = text
  if (color) el.style.color = color
}

// ══════════════════════════════════════════
//  PROBLEMS PANEL
//  Se monta en #rpp-analysis del panel derecho
//  Muestra errores, warnings, Big-O, complejidad, security
// ══════════════════════════════════════════

let _lastFile: CodeFile | null = null
let _lastLiveMetrics: LiveMetricsData | undefined
let _fileFindings: { security: SecurityFinding[]; structural: StructuralSmell[]; naming: NamingSmell[] } = {
  security: [],
  structural: [],
  naming: [],
}

/** Findings del CS Engine (Fases 21/22 — security/structural/naming) para el
 * archivo abierto en el Editor, empujados desde `editor-intelligence.ts`
 * cuando resuelve el heavy path (`/intel/analyze`). Llegan en un timing
 * distinto al de `updateProblems` (2s idle vs. cada cambio de markers), así
 * que se cachean acá y disparan un re-render sobre el último file conocido
 * en vez de requerir que el caller los pase siempre juntos. */
export function updateFileFindings(
  security: SecurityFinding[],
  structural: StructuralSmell[],
  naming: NamingSmell[],
): void {
  _fileFindings = { security, structural, naming }
  if (_lastFile) updateProblems(_lastFile, _lastLiveMetrics)
}

export function updateProblems(file: CodeFile, liveMetrics?: LiveMetricsData): void {
  const container = document.getElementById('problems-content')
  if (!container) return

  // Los findings del CS Engine son de otro archivo — todavía no llegaron los
  // del que se acaba de abrir (el heavy path tarda ~2s idle), así que se
  // limpian en vez de mostrar findings de un archivo distinto al de la vista.
  if (file.name !== _lastFile?.name) {
    _fileFindings = { security: [], structural: [], naming: [] }
  }
  _lastFile = file
  _lastLiveMetrics = liveMetrics

  const errors = file.issues.filter((i) => i.severity === 'error')
  const warnings = file.issues.filter((i) => i.severity === 'warning')
  const infos = file.issues.filter((i) => i.severity === 'info')
  const security = file.issues.filter(
    (i) =>
      i.message?.toLowerCase().includes('security') ||
      i.message?.toLowerCase().includes('injection') ||
      i.message?.toLowerCase().includes('eval') ||
      i.code === 'S001',
  )

  // Header con counts
  let html = `
    <div class="pb-header">
      <span class="pb-filename">${_icon(file.ext)} ${file.name}</span>
      <div class="pb-counts">
        ${errors.length ? `<span class="pb-count pb-err">${icon('warning', 11)} ${errors.length} error${errors.length !== 1 ? 'es' : ''}</span>` : ''}
        ${warnings.length ? `<span class="pb-count pb-warn">△ ${warnings.length} warn</span>` : ''}
        ${infos.length ? `<span class="pb-count pb-info">ℹ ${infos.length} info</span>` : ''}
        ${!file.issues.length && file.analyzed ? `<span class="pb-count pb-ok">✓ Sin problemas</span>` : ''}
        ${!file.analyzed ? `<span class="pb-count" style="color:var(--muted)">Sin analizar todavía</span>` : ''}
      </div>
    </div>
  `

  // Live metrics summary si está disponible
  if (liveMetrics) {
    html += `
      <div class="pb-metrics-summary">
        <div class="pb-metric-row">
          <span class="pb-metric-k">Big-O worst</span>
          <span class="pb-metric-v" style="color:${BIGO_COLOR[liveMetrics.big_o_worst] ?? 'var(--muted)'}">${liveMetrics.big_o_worst}</span>
        </div>
        <div class="pb-metric-row">
          <span class="pb-metric-k">CC promedio</span>
          <span class="pb-metric-v" style="color:${ccColor(liveMetrics.avg_cc)}">${liveMetrics.avg_cc}</span>
        </div>
        <div class="pb-metric-row">
          <span class="pb-metric-k">LOC / SLOC</span>
          <span class="pb-metric-v">${liveMetrics.loc} / ${liveMetrics.sloc}</span>
        </div>
        <div class="pb-metric-row">
          <span class="pb-metric-k">Funciones</span>
          <span class="pb-metric-v">${liveMetrics.functions}</span>
        </div>
        ${
          liveMetrics.safe_mode
            ? `
          <div class="pb-safe-mode">${icon('warning', 12)} Safe mode activo — AST falló, usando regex fallback</div>
        `
            : ''
        }
      </div>
    `
  }

  // Sección por categoría
  if (security.length) {
    html += _problemSection(`${icon('shield', 12)} Security`, security, 'pb-security')
  }
  if (errors.length) {
    html += _problemSection('Errors', errors, 'pb-err-section')
  }
  if (warnings.length) {
    html += _problemSection('Warnings', warnings, 'pb-warn-section')
  }
  if (infos.length) {
    html += _problemSection('ℹ Info', infos, 'pb-info-section')
  }

  // Big-O por función (desde métricas)
  if (liveMetrics?.big_o_dist && Object.keys(liveMetrics.big_o_dist).length) {
    html += `
      <div class="pb-section">
        <div class="pb-section-head">Big-O Distribution</div>
        <div class="pb-bigo-dist">
          ${Object.entries(liveMetrics.big_o_dist)
            .sort(([a], [b]) => _bigoRank(b) - _bigoRank(a))
            .map(
              ([bigo, count]) => `
              <div class="pb-bigo-row">
                <span class="pb-bigo-label" style="color:${BIGO_COLOR[bigo] ?? 'var(--muted)'}">${bigo}</span>
                <div class="pb-bigo-bar-wrap">
                  <div class="pb-bigo-bar" style="width:${Math.min(100, (count as number) * 20)}%;background:${BIGO_COLOR[bigo] ?? 'var(--muted)'}"></div>
                </div>
                <span class="pb-bigo-count">${count} fn</span>
              </div>
            `,
            )
            .join('')}
        </div>
      </div>
    `
  }

  // Findings del CS Engine (Security/Structural/Naming — Fases 21/22),
  // Rust-first vía /intel/analyze. Distinto de los `issues` de flake8/pylint
  // de arriba — son señales del motor propio de Sythrall, no de linters
  // externos, así que se muestran en su propia sección.
  html += _renderFileFindings()

  container.innerHTML = html

  // Click en issue → ir a la línea
  container.querySelectorAll<HTMLElement>('[data-pb-line]').forEach((item) => {
    item.addEventListener('click', () => {
      const line = parseInt(item.dataset['pbLine'] ?? '1', 10)
      const goTo = (window as any)['editorGoToLine'] as ((l: number) => void) | undefined
      goTo?.(line)
      import('../components/app').then((m) => m.switchTab?.('editor'))
    })
  })
}

/** Security/Structural/Naming findings del CS Engine para el archivo abierto
 * — mismo patrón visual que `_problemSection` (fila clickeable con
 * `data-pb-line`, reusa el delegado de click ya wireado al final de
 * `updateProblems`), pero coloreado con los mismos mapas kind→color que ya
 * usa `panels/static.ts`, no una paleta nueva inventada acá. */
function _renderFileFindings(): string {
  const { security, structural, naming } = _fileFindings
  if (!security.length && !structural.length && !naming.length) return ''

  const row = (line: number, label: string, color: string, msg: string): string => `
    <div class="pb-issue" data-pb-line="${line}" title="Clic para ir a la línea">
      <span class="pb-issue-loc">L${line}</span>
      <span class="pb-issue-tool" style="color:${color}">${_esc(label)}</span>
      <span class="pb-issue-msg">${_esc(msg)}</span>
    </div>`

  const secHtml = security
    .map((f) => row(f.line, f.severity, securitySeverityColor(f.severity), `${f.cwe} ${f.category}`))
    .join('')
  const structuralHtml = structural
    .map((s) => {
      const [label, color] = SMELL_LABEL[s.kind] ?? [s.kind, 'var(--muted)']
      return row(s.line, label, color, `${s.name} — ${s.message}`)
    })
    .join('')
  const namingHtml = naming
    .map((s) => {
      const [label, color] = NAMING_SMELL_LABEL[s.kind] ?? [s.kind, 'var(--muted)']
      return row(s.line, label, color, `${s.name} — ${s.message}`)
    })
    .join('')

  return `
    ${security.length ? `<div class="pb-section pb-security"><div class="pb-section-head">${icon('shield', 12)} Security (${security.length})</div>${secHtml}</div>` : ''}
    ${structural.length ? `<div class="pb-section"><div class="pb-section-head">Structural Smells (${structural.length})</div>${structuralHtml}</div>` : ''}
    ${naming.length ? `<div class="pb-section"><div class="pb-section-head">Naming Smells (${naming.length})</div>${namingHtml}</div>` : ''}
  `
}

function _problemSection(title: string, issues: Issue[], cls: string): string {
  return `
    <div class="pb-section ${cls}">
      <div class="pb-section-head">${title} (${issues.length})</div>
      ${issues
        .slice(0, 30)
        .map(
          (issue) => `
        <div class="pb-issue" data-pb-line="${issue.line ?? 0}" title="Clic para ir a la línea">
          <span class="pb-issue-loc">L${issue.line ?? '?'}</span>
          <span class="pb-issue-tool pb-tool-${issue.tool ?? 'ast'}">${issue.tool ?? 'ast'}</span>
          <span class="pb-issue-msg">${_esc(issue.message ?? '')}</span>
          ${issue.code ? `<span class="pb-issue-code">${issue.code}</span>` : ''}
        </div>
      `,
        )
        .join('')}
      ${issues.length > 30 ? `<div class="pb-more">+${issues.length - 30} más...</div>` : ''}
    </div>
  `
}

function _bigoRank(bigo: string): number {
  const order: Record<string, number> = {
    'O(2^n)': 6,
    'O(n³)': 5,
    'O(n²)': 4,
    'O(n log n)': 3,
    'O(n)': 2,
    'O(log n)': 1,
    'O(1)': 0,
  }
  return order[bigo] ?? 0
}

function _icon(ext: string): string {
  return languageBadge(ext)
}

function _esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

// ══════════════════════════════════════════
//  SESSION RESTORE
//  Guarda/restaura el archivo activo en localStorage
// ══════════════════════════════════════════

const SESSION_KEY = 'sythrall:session'

interface SessionData {
  activeFile?: string // filename del archivo activo
  timestamp: number
}

export function saveSession(activeFilename: string): void {
  try {
    const data: SessionData = { activeFile: activeFilename, timestamp: Date.now() }
    localStorage.setItem(SESSION_KEY, JSON.stringify(data))
  } catch {
    /* localStorage puede no estar disponible */
  }
}

export function restoreSession(): string | null {
  try {
    const raw = localStorage.getItem(SESSION_KEY)
    if (!raw) return null
    const data: SessionData = JSON.parse(raw)
    // Solo restaurar si es reciente (< 24h)
    if (Date.now() - data.timestamp > 24 * 60 * 60 * 1000) {
      localStorage.removeItem(SESSION_KEY)
      return null
    }
    return data.activeFile ?? null
  } catch {
    return null
  }
}

export function clearSession(): void {
  try {
    localStorage.removeItem(SESSION_KEY)
  } catch {
    /* noop */
  }
}

// ── Tipos internos ────────────────────────────────────────────────────────────

export interface LiveMetricsData {
  filename: string
  language: string
  loc: number
  sloc: number
  functions: number
  classes: number
  imports: number
  avg_cc: number
  max_cc: number
  big_o_worst: string
  big_o_dist: Record<string, number>
  parse_ok: boolean
  parse_error: string | null
  safe_mode: boolean
  ms: number
}
