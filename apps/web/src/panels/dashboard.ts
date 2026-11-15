// ══════════════════════════════════════════
//  Sythrall — Panel Dashboard: Project Health (Fase 2 del rediseño UX)
//
//  El Dashboard es project-centric por diseño: Project Health/Findings/
//  Big-O SIEMPRE representan el proyecto activo (state.activeProjectId),
//  nunca se mezclan con archivos sueltos cargados a mano ni con URLs de
//  APIs pegadas manualmente — mezclar esas fuentes en la MISMA vista
//  producía una ambigüedad real ("¿estos archivos son del proyecto o los
//  que cargué yo?"). Lo que si cambió (UX Audit): sin proyecto activo pero
//  CON archivos sueltos cargados, la pantalla ya no se queda en un mensaje
//  genérico — muestra un resumen de esos archivos, con su propio header
//  claramente rotulado como "Archivos sueltos" para no reintroducir esa
//  ambigüedad (ver _renderAdHocSummary). El health-check de APIs sigue
//  exclusivamente en APIs.
//
//  Única excepción a "todo acá es del proyecto activo", agregada en la Fase
//  28: `_renderOverviewSection` — el panorama entre TODOS los proyectos con
//  snapshots guardados. No rompe la regla de arriba por 2 motivos
//  deliberados: va siempre al final (nunca mezclado con las tarjetas del
//  proyecto activo) y lleva su propio rótulo explícito ("Panorama entre
//  proyectos"), que es exactamente el remedio que _renderAdHocSummary ya
//  usó para el mismo riesgo.
// ══════════════════════════════════════════

import type {
  HistoryComparison,
  HistoryOverview,
  HistorySnapshot,
  NamingSmell,
  SecurityFinding,
  StaticLanguagesResult,
  StaticProjectResult,
  StructuralSmell,
} from '../api/client'
import { api } from '../api/client'
import { guardarUltimoAnalisis, leerUltimoAnalisis, setActiveProject, state } from '../store/state'
import type { TabId } from '../types'
import { renderBigODistribution } from '../utils/bigo'
import { NAMING_SMELL_LABEL, SMELL_LABEL, securitySeverityColor, smellColorSeverity } from '../utils/etiquetas'
import { healthTier, renderHealthCards } from '../utils/health'
import { esc, toast } from '../utils/helpers'
import type { IconName } from '../utils/icons'
import { icon } from '../utils/icons'

// Capacidad real del engine por lenguaje (tree-sitter instalado o no, etc.)
// — cambia solo con el deploy, no por proyecto, así que se pide una vez y
// se cachea acá en vez de repetir el fetch en cada análisis.
let _engineLanguages: StaticLanguagesResult | null = null

// Momento del último análisis exitoso EN ESTA SESIÓN del navegador — no hay
// persistencia server-side de corridas pasadas todavía, así que esto se
// resetea al recargar la página; se muestra como tal, no como un historial
// real. La Fase 28 (Enterprise Intelligence) cierra esa brecha para
// snapshots GUARDADOS explícitamente (ver `_renderHistorySection` más abajo)
// — esto (`_lastAnalyzedAt`) sigue siendo solo "cuándo fue el último análisis
// de ESTA pestaña", no reemplazado por el historial persistente.
let _lastAnalyzedAt: number | null = null

// Fase 28 (Enterprise Intelligence, primera porción) — snapshots guardados
// por proyecto, cacheados igual que `_engineLanguages` de arriba (se pide
// una vez, se invalida explícitamente al guardar/borrar uno nuevo, nunca se
// vuelve a pedir "por las dudas" en cada render).
const _historyCache: Record<string, HistorySnapshot[]> = {}
const _historyLoading = new Set<string>()

async function _ensureHistoryLoaded(projectId: string): Promise<void> {
  if (_historyCache[projectId] || _historyLoading.has(projectId)) return
  _historyLoading.add(projectId)
  try {
    _historyCache[projectId] = await api.listSnapshots(projectId)
  } catch {
    _historyCache[projectId] = []
  } finally {
    _historyLoading.delete(projectId)
    renderDashboard()
  }
}

// Fase 28, segundo bullet — panorama entre proyectos. Mismo patrón de cache
// que `_historyCache` de arriba, pero UNA sola entrada (no por proyecto:
// esto es global por definición). Se invalida en los mismos 2 puntos que el
// historial por proyecto — guardar y borrar un snapshot — porque son
// exactamente los eventos que cambian lo que este panorama muestra.
let _overviewCache: HistoryOverview | null = null
let _overviewLoading = false

const _EMPTY_OVERVIEW: HistoryOverview = {
  projects: [],
  aggregate: {
    projects: 0,
    snapshots: 0,
    avg: { security: 0, quality: 0, complexity: 0, architecture: 0, overall: 0 },
    totals: {},
  },
  note: '',
}

async function _ensureOverviewLoaded(): Promise<void> {
  if (_overviewCache || _overviewLoading) return
  _overviewLoading = true
  try {
    _overviewCache = await api.historyOverview()
  } catch {
    _overviewCache = _EMPTY_OVERVIEW
  } finally {
    _overviewLoading = false
    renderDashboard()
  }
}

export async function loadProjectHealth(): Promise<void> {
  if (!state.activeProjectId) {
    toast('Elegí un proyecto activo en Proyectos primero', 'warn')
    return
  }

  const root = document.getElementById('dash-content')
  if (root) {
    root.innerHTML = `<div class="empty">Analizando proyecto…</div>`
  }

  state.projectAnalysisRunning = true
  const { renderFlow } = await import('../components/flow')
  renderFlow()
  ;(await import('../components/app')).updateGlobalPill()
  try {
    const [data] = await Promise.all([
      api.staticParseProjectById(state.activeProjectId),
      _engineLanguages ? Promise.resolve(_engineLanguages) : api.staticLanguages().then((r) => (_engineLanguages = r)),
    ])
    state.results.projectDashboard = data
    // El grid de Proyectos (panels/upload.ts) lee este cache para mostrar
    // badges de health reales sin tener que disparar su propio análisis —
    // nunca un score inventado, solo lo que ya se calculó de verdad acá.
    if (state.activeProjectId) {
      state.projectHealthCache[state.activeProjectId] = data.health
      guardarUltimoAnalisis(state.activeProjectId, data)
    }
    _lastAnalyzedAt = Date.now()
    state.lastAnalysisAt = _lastAnalyzedAt
  } catch (e) {
    toast('Error: ' + (e as Error).message, 'err')
  }
  state.projectAnalysisRunning = false
  renderFlow()
  const { updateGlobalPill } = await import('../components/app')
  updateGlobalPill()
  renderDashboard()
}

/** Nombre a mostrar del proyecto activo — `state.activeProjectName` (seteado
 * por `setActiveProject()`) si se conoce, si no cae al id truncado, mismo
 * criterio que ya usa panels/upload.ts en sus logs/toasts. */
function _projectLabel(): string {
  if (!state.activeProjectId) return ''
  return state.activeProjectName || state.activeProjectId.slice(0, 8)
}

function _goto(tab: TabId): void {
  import('../components/app').then((m) => m.switchTab?.(tab))
}

type FindingRow = {
  severity: 'High' | 'Medium' | 'Low'
  label: string
  color: string
  file: string
  line: number
  detail: string
}

const SEV_ORDER: Record<FindingRow['severity'], number> = { High: 0, Medium: 1, Low: 2 }

function _mergedFindings(data: StaticProjectResult): FindingRow[] {
  const sec: FindingRow[] = (data.security_findings ?? []).map((f: SecurityFinding) => ({
    severity: f.severity,
    label: `${f.cwe} ${f.category}`,
    color: securitySeverityColor(f.severity),
    file: f.file ?? '',
    line: f.line,
    detail: f.recommendation,
  }))
  const structural: FindingRow[] = (data.structural_smells ?? []).map((s: StructuralSmell) => {
    const [label, color] = SMELL_LABEL[s.kind] ?? [s.kind, 'var(--muted)']
    return { severity: smellColorSeverity(color), label, color, file: s.file ?? '', line: s.line, detail: s.message }
  })
  const naming: FindingRow[] = (data.naming_smells ?? []).map((s: NamingSmell) => {
    const [label, color] = NAMING_SMELL_LABEL[s.kind] ?? [s.kind, 'var(--muted)']
    return { severity: smellColorSeverity(color), label, color, file: s.file ?? '', line: s.line, detail: s.message }
  })
  return [...sec, ...structural, ...naming].sort((a, b) => SEV_ORDER[a.severity] - SEV_ORDER[b.severity])
}

/** Formato compacto de LOC (182431 → "182K") — mismo criterio que cualquier
 * lector esperaría de un conteo de líneas de un proyecto grande. */
function _fmtLoc(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${Math.round(n / 1000)}K`
  return String(n)
}

/** Tiempo relativo desde `_lastAnalyzedAt` — solo de esta sesión del
 * navegador (ver comentario en la declaración), nunca "nunca" cuando no hay
 * timestamp: en ese caso simplemente no se muestra la celda. */
function _fmtRelativeTime(ms: number): string {
  const diff = Date.now() - ms
  const mins = Math.floor(diff / 60000)
  if (mins < 1) return '<1m'
  if (mins < 60) return `${mins}m`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `${hours}h`
  return `${Math.floor(hours / 24)}d`
}

/** Toma `summary` y la fecha por separado, no el `StaticProjectResult`
 * entero: el Dashboard también dibuja esta fila con el último análisis
 * GUARDADO, del que solo se persisten esos dos pedazos. */
function _renderStatsRow(s: StaticProjectResult['summary'], at: number | null): string {
  const findingsTotal = (s.security_findings ?? 0) + (s.structural_smells ?? 0) + (s.naming_smells ?? 0)
  const cells: Array<[string, string]> = [
    [String(s.total_files ?? 0), 'Files'],
    [_fmtLoc(s.total_loc ?? 0), 'LOC'],
    [String(findingsTotal), 'Findings'],
    [at ? _fmtRelativeTime(at) : '—', 'Last Analysis'],
  ]
  return `
  <div class="st-summary-row">
    ${cells.map(([val, lbl]) => `<div class="stat-card"><div class="sc-lbl">${lbl}</div><div class="sc-val">${esc(val)}</div></div>`).join('')}
  </div>`
}

function _renderRecentFindings(rows: FindingRow[]): string {
  const top = rows.slice(0, 5)
  if (!top.length) {
    return `<div class="chart-card"><div class="cc-head"><span>RECENT FINDINGS</span></div><div class="empty">Sin findings — proyecto limpio</div></div>`
  }
  return `
  <div class="chart-card">
    <div class="cc-head">
      <span>RECENT FINDINGS</span>
      <span class="cc-head-link" id="dash-findings-viewall" style="cursor:pointer;color:var(--info);font-size:.62rem">Ver todos →</span>
    </div>
    <div class="dash-findings-list">
      ${top
        .map(
          (f) => `<div class="dash-finding-row">
        <span style="color:${f.color};font-family:var(--mono);font-size:.62rem;min-width:52px">${f.severity.toUpperCase()}</span>
        <div class="dash-finding-body">
          <div class="dash-finding-label">${esc(f.label)}</div>
          <div class="dash-finding-loc">${esc(f.file)}${f.file ? ':' : ''}${f.line}</div>
        </div>
      </div>`,
        )
        .join('')}
    </div>
  </div>`
}

function _renderFindingsBySeverity(rows: FindingRow[]): string {
  const high = rows.filter((r) => r.severity === 'High').length
  const medium = rows.filter((r) => r.severity === 'Medium').length
  const low = rows.filter((r) => r.severity === 'Low').length
  const total = Math.max(1, high + medium + low)
  const bar = (n: number, color: string) =>
    `<div class="dash-sev-bar-wrap"><div class="dash-sev-bar" style="width:${Math.round((n / total) * 100)}%;background:${color}"></div></div>`
  return `
  <div class="chart-card">
    <div class="cc-head"><span>FINDINGS BY SEVERITY</span></div>
    <div class="dash-sev-rows">
      <div class="dash-sev-row"><span class="text-err">High</span>${bar(high, 'var(--err)')}<span>${high}</span></div>
      <div class="dash-sev-row"><span class="text-warn">Medium</span>${bar(medium, 'var(--warn)')}<span>${medium}</span></div>
      <div class="dash-sev-row"><span class="text-muted">Low</span>${bar(low, 'var(--muted)')}<span>${low}</span></div>
    </div>
  </div>`
}

function _renderComplexityByFunction(data: StaticProjectResult): string {
  const fns = data.top_complex_functions ?? []
  if (!fns.length) {
    return `<div class="chart-card wide"><div class="cc-head"><span>COMPLEXITY BY FUNCTION</span></div><div class="empty">Sin funciones analizadas</div></div>`
  }
  const maxCC = Math.max(...fns.map((f) => f.complexity), 1)
  return `
  <div class="chart-card wide card card-pad">
    <div class="cc-head"><span>COMPLEXITY BY FUNCTION</span><span class="meta">cyclomatic · Big-O</span></div>
    <div class="dash-cc-fn-list">
      ${fns
        .map((f) => {
          const pct = Math.round((f.complexity / maxCC) * 100)
          const color = f.complexity <= 5 ? 'var(--ok)' : f.complexity <= 10 ? 'var(--warn)' : 'var(--err)'
          return `<div class="dash-cc-fn-row" data-cc-file="${esc(f.file)}" data-cc-line="${f.line}">
          <span class="dash-cc-fn-name" title="${esc(f.file)}">${esc(f.name)}()</span>
          <div class="dash-cc-fn-bar-wrap"><div class="dash-cc-fn-bar" style="width:${pct}%;background:${color}"></div></div>
          <span class="dash-cc-fn-cc" style="color:${color}">${f.complexity}</span>
          <span class="dash-cc-fn-bigo">${esc(f.big_o)}</span>
        </div>`
        })
        .join('')}
    </div>
  </div>`
}

const LANG_DISPLAY_NAME: Record<string, string> = {
  python: 'Python',
  javascript: 'JavaScript',
  typescript: 'TypeScript',
  c: 'C',
  cpp: 'C++',
  fortran: 'Fortran',
  ada: 'Ada',
  d: 'D',
}

/** Distribución real de LOC por lenguaje del proyecto activo (arriba) +
 * capacidad real del engine (abajo, `/static/languages` — `available`
 * refleja si la dependencia está instalada, no una lista aspiracional). */
function _renderLanguages(data: StaticProjectResult): string {
  const dist = data.language_distribution ?? {}
  const entries = Object.entries(dist).filter(([, v]) => v.loc > 0)
  const totalLoc = entries.reduce((sum, [, v]) => sum + v.loc, 0)

  const distRows = entries.length
    ? entries
        .sort((a, b) => b[1].loc - a[1].loc)
        .map(([lang, v]) => {
          const pct = totalLoc ? Math.round((v.loc / totalLoc) * 100) : 0
          const label = LANG_DISPLAY_NAME[lang] ?? lang
          return `<div class="bigo-dist-row">
        <span class="bigo-badge" style="min-width:90px">${esc(label)}</span>
        <div class="bigo-dist-bar-wrap"><div class="bigo-dist-bar" style="width:${pct}%;background:var(--info)"></div></div>
        <span style="font-family:var(--mono);font-size:.65rem;color:var(--muted)">${pct}% · ${_fmtLoc(v.loc)} LOC</span>
      </div>`
        })
        .join('')
    : `<div class="empty">Sin datos de lenguaje todavía</div>`

  const support = _engineLanguages
    ? Object.entries(_engineLanguages.languages)
        .map(
          ([lang, info]) =>
            `<div class="dash-lang-support-row">
          <span style="color:${info.available ? 'var(--ok)' : 'var(--muted)'}">${info.available ? '✓' : '○'}</span>
          <span>${esc(LANG_DISPLAY_NAME[lang] ?? lang)}</span>
        </div>`,
        )
        .join('')
    : ''

  return `
  <div class="chart-card wide card card-pad">
    <div class="cc-head"><span>LANGUAGES</span></div>
    <div style="padding:4px 0">${distRows}</div>
    ${
      support
        ? `<div class="cc-head mt-3"><span>LANGUAGE SUPPORT</span><span class="meta">capacidad real del engine</span></div>
    <div class="dash-lang-support-grid">${support}</div>`
        : ''
    }
  </div>`
}

const _HISTORY_HEALTH_KEYS: Array<keyof HistoryComparison['health_diff']> = [
  'security',
  'quality',
  'complexity',
  'architecture',
]

function _fmtSnapshotDate(iso: string): string {
  const d = new Date(iso)
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString()
}

/** Signo explícito en el delta (`+3`/`-3`/`0`, nunca un `3` pelado que
 * podría leerse como "el nuevo valor" en vez de "cuánto cambió") — mismo
 * criterio de "nunca un número sin su porqué" que `renderHealthCards` ya
 * aplica arriba. Verde/rojo depende de si el número que sube es "mejor"
 * (un score) o "peor" (un conteo de hallazgos) — invertido a propósito
 * entre los dos grupos. */
function _fmtDelta(delta: number, higherIsBetter: boolean): string {
  const sign = delta > 0 ? '+' : ''
  const good = higherIsBetter ? delta >= 0 : delta <= 0
  const color = delta === 0 ? 'var(--muted)' : good ? 'var(--ok)' : 'var(--err)'
  return `<span style="color:${color}">${sign}${delta}</span>`
}

function _renderComparisonResult(cmp: HistoryComparison): string {
  const healthRows = _HISTORY_HEALTH_KEYS
    .map((key) => {
      const d = cmp.health_diff[key]
      return `<div class="st-stat health-stat">
      <div class="st-stat-val" style="color:${healthTier(d.to)}">${d.to}</div>
      <div class="st-stat-lbl">${key}</div>
      <div class="health-why">${d.from} → ${d.to} (${_fmtDelta(d.delta, true)})</div>
    </div>`
    })
    .join('')

  const summaryEntries = Object.entries(cmp.summary_diff).filter(([, d]) => d.delta !== 0)
  const summaryRows = summaryEntries.length
    ? summaryEntries
        .map(
          ([key, d]) =>
            `<div class="dash-lang-support-row">
          <span>${esc(key)}</span>
          <span style="margin-left:auto;font-family:var(--mono)">${d.from} → ${d.to} (${_fmtDelta(d.delta, !/security|smells|dead|unused/.test(key))})</span>
        </div>`,
        )
        .join('')
    : `<div class="empty">Sin cambios en el resumen entre estos 2 snapshots</div>`

  return `
    ${_renderCapabilitiesWarning(cmp.capabilities_diff)}
    <div class="st-summary-row health-row mt-2">${healthRows}</div>
    <div class="cc-head mt-3"><span>CAMBIOS EN EL RESUMEN</span></div>
    <div class="dash-lang-support-grid">${summaryRows}</div>
  `
}

/** Se muestra ARRIBA de los scores, no debajo, y solo cuando hay algo que
 * advertir: si el motor cambió entre las dos mediciones, leer los deltas sin
 * saberlo lleva a la conclusión equivocada, así que la advertencia tiene que
 * llegar antes que el número. Cuando el motor era el mismo no se dice nada —
 * un cartel verde permanente se vuelve invisible. */
function _renderCapabilitiesWarning(caps: HistoryComparison['capabilities_diff']): string {
  if (caps.comparable) return ''
  const color = caps.known ? 'var(--warn)' : 'var(--muted)'
  const detalle = _capsList('+', caps.added)
  const retirados = _capsList('−', caps.removed)
  return `<div class="st-mem-note" style="border-left:2px solid ${color};padding-left:8px;margin-top:8px">
    ${esc(caps.note)}${detalle}${retirados}
  </div>`
}

/** Acotada a 8: comparar contra un snapshot muy viejo puede listar decenas de
 * capacidades, y una pared de texto tapa el aviso en vez de explicarlo. Se
 * dice cuántas quedaron afuera en vez de recortar en silencio. */
const _CAPS_VISIBLES = 8

function _capsList(signo: string, items: string[]): string {
  if (!items.length) return ''
  const visibles = items.slice(0, _CAPS_VISIBLES).map(esc).join(', ')
  const resto = items.length > _CAPS_VISIBLES ? ` … y ${items.length - _CAPS_VISIBLES} más` : ''
  return `<div style="margin-top:4px;font-family:var(--mono);font-size:.62rem;opacity:.85">${signo} ${visibles}${resto}</div>`
}

/** Historial de análisis (Fase 28, Enterprise Intelligence) — snapshots de
 * Project Health guardados EXPLÍCITAMENTE (nunca automático en cada
 * análisis: guardar solo, no en cada re-análisis, evita que la lista se
 * llene de entradas idénticas por clickear "Actualizar" repetidas veces).
 * Comparación por dropdown (desde/hasta) en vez de selección múltiple por
 * click — más simple de cablear, y 2 snapshots son la comparación que
 * importa, no un rango arbitrario. */
function _renderHistorySection(projectId: string): string {
  const snapshots = _historyCache[projectId]

  const body = !snapshots
    ? `<div class="empty">Cargando historial…</div>`
    : snapshots.length === 0
      ? `<div class="empty">Sin snapshots guardados todavía — "Guardar snapshot actual" para empezar a comparar</div>`
      : `
      <div class="dash-lang-support-grid">
        ${snapshots
          .map(
            (s) => `
          <div class="dash-lang-support-row">
            <span>${esc(_fmtSnapshotDate(s.created_at))}</span>
            <span style="margin-left:auto;font-family:var(--mono);color:${healthTier(s.health.security.score)}">S:${s.health.security.score}</span>
            <span style="font-family:var(--mono);color:${healthTier(s.health.quality.score)}">Q:${s.health.quality.score}</span>
            <span style="font-family:var(--mono);color:${healthTier(s.health.complexity.score)}">C:${s.health.complexity.score}</span>
            <span style="font-family:var(--mono);color:${healthTier(s.health.architecture.score)}">A:${s.health.architecture.score}</span>
            <button class="btn btn-ghost btn-sm" data-history-delete="${s.id}" title="Borrar snapshot">✕</button>
          </div>`,
          )
          .join('')}
      </div>
      ${
        snapshots.length >= 2
          ? `
      <div style="display:flex;gap:6px;align-items:center;margin-top:8px;flex-wrap:wrap">
        <select id="dash-history-from" class="diag-sel">
          ${snapshots.map((s, i) => `<option value="${s.id}"${i === snapshots.length - 1 ? ' selected' : ''}>${esc(_fmtSnapshotDate(s.created_at))}</option>`).join('')}
        </select>
        <span style="color:var(--muted);font-size:.7rem">vs</span>
        <select id="dash-history-to" class="diag-sel">
          ${snapshots.map((s, i) => `<option value="${s.id}"${i === 0 ? ' selected' : ''}>${esc(_fmtSnapshotDate(s.created_at))}</option>`).join('')}
        </select>
        <button class="btn btn-ghost btn-sm" id="dash-history-compare-btn">Comparar</button>
      </div>
      <div id="dash-history-compare-result"></div>`
          : ''
      }`

  return `
  <div class="chart-card wide card card-pad">
    <div class="cc-head"><span>HISTORIAL DE ANÁLISIS</span><span class="cc-head-sub meta">snapshots locales, sin auth todavía (Fase 28)</span></div>
    <button class="btn btn-ghost btn-sm mb-2" id="dash-save-snapshot">Guardar snapshot actual</button>
    ${body}
  </div>`
}

/** Panorama entre proyectos (Fase 28, segundo bullet) — el último snapshot
 * guardado de CADA proyecto, ordenado por peor salud primero. Es la única
 * vista del Dashboard que NO es del proyecto activo (ver el comentario de
 * cabecera de este archivo): va rotulada explícitamente como tal, y separada
 * de todo lo de arriba, para no reintroducir la ambigüedad de fuentes que ese
 * comentario documenta.
 *
 * Devuelve `''` (nada, ni siquiera un card vacío) cuando todavía no hay
 * ningún snapshot guardado: hasta que el usuario guarde el primero desde la
 * sección de arriba, este panorama no tiene nada que decir y un card vacío
 * sería solo ruido. */
function _renderOverviewSection(): string {
  const ov = _overviewCache
  if (!ov || ov.projects.length === 0) return ''

  const agg = ov.aggregate
  const avgCards = (['security', 'quality', 'complexity', 'architecture'] as const)
    .map(
      (cat) => `<div class="st-stat health-stat">
      <div class="st-stat-val" style="color:${healthTier(agg.avg[cat])}">${agg.avg[cat]}</div>
      <div class="st-stat-lbl">${cat}</div>
      <div class="health-why">promedio entre ${agg.projects} proyecto${agg.projects === 1 ? '' : 's'}</div>
    </div>`,
    )
    .join('')

  const rows = ov.projects
    .map((p) => {
      const label = p.project_name ?? p.project_id.slice(0, 8) + '…'
      const scores = (['security', 'quality', 'complexity', 'architecture'] as const)
        .map(
          (cat) =>
            `<span style="color:${healthTier(p.health[cat].score)}" title="${cat}">${cat[0]?.toUpperCase()}:${p.health[cat].score}</span>`,
        )
        .join('')
      // Δ del `overall` contra el snapshot anterior del MISMO proyecto — no
      // contra el promedio de los demás: la pregunta que un panorama así
      // responde es "¿este proyecto mejoró o empeoró?", no "¿está por encima
      // de la media?".
      const deltaOverall = p.delta
        ? Math.round((p.delta.security + p.delta.quality + p.delta.complexity + p.delta.architecture) / 4)
        : null
      return `
      <div class="dash-ov-row${p.exists ? ' clickable' : ''}"${p.exists ? ` data-overview-project="${esc(p.project_id)}" data-overview-name="${esc(label)}" title="Activar este proyecto y analizarlo"` : ' title="El proyecto ya no está en disco — su historial se conserva"'}>
        <span class="dash-ov-name"${p.exists ? '' : ' style="color:var(--muted);text-decoration:line-through"'}>${esc(label)}</span>
        <span class="dash-ov-scores">${scores}</span>
        <span class="dash-ov-meta">${deltaOverall === null ? '1er snapshot' : `Δ ${_fmtDelta(deltaOverall, true)}`}</span>
        <span class="dash-ov-meta">${p.snapshots} snap.</span>
        <span class="dash-ov-overall" style="color:${healthTier(p.overall)}">${p.overall}</span>
      </div>`
    })
    .join('')

  return `
  <div class="chart-card wide card card-pad">
    <div class="cc-head"><span>PANORAMA ENTRE PROYECTOS</span><span class="cc-head-sub meta">último snapshot de cada uno · ${agg.snapshots} en total</span></div>
    <div class="st-summary-row health-row">${avgCards}</div>
    <div class="mt-3">${rows}</div>
    <div style="color:var(--muted);font-size:.62rem;margin-top:8px">${esc(ov.note)}</div>
  </div>`
}

function _wireOverviewInteractivity(root: HTMLElement): void {
  root.querySelectorAll<HTMLElement>('[data-overview-project]').forEach((el) => {
    el.addEventListener('click', () => {
      const id = el.dataset['overviewProject']
      if (!id || id === state.activeProjectId) return
      setActiveProject(id, el.dataset['overviewName'] ?? null)
      // El proyecto cambió: los resultados en pantalla son del anterior.
      // `loadProjectHealth` los reemplaza y vuelve a renderizar.
      void loadProjectHealth()
    })
  })
}

function _wireHistoryInteractivity(root: HTMLElement, projectId: string): void {
  root.querySelector<HTMLElement>('#dash-save-snapshot')?.addEventListener('click', async () => {
    try {
      await api.createSnapshot(projectId)
      delete _historyCache[projectId]
      _overviewCache = null // el panorama de abajo también cambió
      toast('Snapshot guardado', 'ok')
      void _ensureHistoryLoaded(projectId)
    } catch (e) {
      toast('Error: ' + (e as Error).message, 'err')
    }
  })
  root.querySelectorAll<HTMLElement>('[data-history-delete]').forEach((el) => {
    el.addEventListener('click', async () => {
      const id = Number(el.dataset['historyDelete'])
      try {
        await api.deleteSnapshot(id)
        delete _historyCache[projectId]
        _overviewCache = null // idem: borrar un snapshot puede cambiar el panorama
        void _ensureHistoryLoaded(projectId)
      } catch (e) {
        toast('Error: ' + (e as Error).message, 'err')
      }
    })
  })
  root.querySelector<HTMLElement>('#dash-history-compare-btn')?.addEventListener('click', async () => {
    const fromEl = document.getElementById('dash-history-from') as HTMLSelectElement | null
    const toEl = document.getElementById('dash-history-to') as HTMLSelectElement | null
    const resultEl = document.getElementById('dash-history-compare-result')
    if (!fromEl || !toEl || !resultEl) return
    resultEl.innerHTML = `<div class="empty">Comparando…</div>`
    try {
      const cmp = await api.compareSnapshots(Number(fromEl.value), Number(toEl.value))
      resultEl.innerHTML = _renderComparisonResult(cmp)
    } catch (e) {
      resultEl.innerHTML = `<div class="empty">Error: ${esc((e as Error).message)}</div>`
    }
  })
}

function _wireInteractivity(root: HTMLElement): void {
  root.querySelectorAll<HTMLElement>('[data-health-nav]').forEach((el) => {
    el.addEventListener('click', () => _goto(el.dataset['healthNav'] as TabId))
  })
  root.querySelector<HTMLElement>('#dash-findings-viewall')?.addEventListener('click', () => _goto('static'))
  root.querySelectorAll<HTMLElement>('.dash-cc-fn-row').forEach((el) => {
    el.addEventListener('click', async () => {
      const file = el.dataset['ccFile']
      const line = parseInt(el.dataset['ccLine'] ?? '1', 10)
      if (!file || !state.activeProjectId) return
      const { openProjectFile } = await import('./upload')
      await openProjectFile(state.activeProjectId, file)
      _goto('editor')
      setTimeout(() => {
        const goTo = (window as unknown as { editorGoToLine?: (l: number) => void }).editorGoToLine
        goTo?.(line)
      }, 150)
    })
  })
}

/** Sin proyecto activo pero con archivos sueltos cargados (el flujo de
 * "+ Código" invita justo a esto) — antes esta vista se quedaba en un
 * mensaje genérico aunque analizar esos archivos ya dejara números reales
 * disponibles (los mismos que Métricas ya calcula, ver panels/analysis.ts).
 * Header propio y explícitamente rotulado "Archivos sueltos" para no
 * mezclar esta vista con Project Health (ver comentario de arriba). */
function _renderAdHocSummary(): string {
  const files = state.files
  const issues = state.results.issues
  const errors = issues.filter((i) => i.severity === 'error').length
  const warnings = issues.filter((i) => i.severity === 'warning').length
  const analyzed = files.filter((f) => f.analyzed).length

  const cells: Array<[string, string]> = [
    [`${analyzed}/${files.length}`, 'Files'],
    [String(issues.length), 'Issues'],
    [String(errors), 'Errors'],
    [String(warnings), 'Warnings'],
  ]

  return `
    <div class="dash-project-head">
      <span class="dash-project-lbl">Archivos sueltos</span>
      <span class="dash-project-name">sin proyecto activo</span>
      <button class="btn btn-ghost btn-sm ml-auto" id="dash-goto-projects-adhoc">Proyectos</button>
    </div>
    <div class="st-summary-row">
      ${cells.map(([val, lbl]) => `<div class="stat-card"><div class="sc-lbl">${lbl}</div><div class="sc-val">${esc(val)}</div></div>`).join('')}
    </div>
    <div class="chart-card wide card card-pad">
      <div class="cc-head"><span>ARCHIVOS</span></div>
      <div style="display:flex;flex-direction:column;margin-top:6px">
        ${files
          .map((f) => {
            const hasErr = f.issues.some((i) => i.severity === 'error')
            const color = hasErr
              ? 'var(--err)'
              : f.issues.length
                ? 'var(--warn)'
                : f.analyzed
                  ? 'var(--ok)'
                  : 'var(--muted)'
            const val = f.issues.length || (f.analyzed ? 'OK' : '—')
            return `<div class="dash-adhoc-file-row" data-adhoc-file="${f.id}">
              <span class="dash-adhoc-file-name">${esc(f.name)}</span>
              <span style="margin-left:auto;font-family:var(--mono);font-size:.72rem;color:${color}">${val}</span>
            </div>`
          })
          .join('')}
      </div>
    </div>
  `
}

function _wireAdHocInteractivity(root: HTMLElement): void {
  root.querySelectorAll<HTMLElement>('[data-adhoc-file]').forEach((el) => {
    el.addEventListener('click', async () => {
      const id = el.dataset['adhocFile']
      if (!id) return
      const { selectFile } = await import('../components/app')
      selectFile(id)
    })
  })
}

/** Línea de estado del servicio, chica y opcional — deliberadamente NO 3
 * filas separadas de "Backend"/"Rust Engine"/"Linters" con Ready/Offline por
 * cada una (versión anterior de este hero). Ese desglose es exactamente la
 * fuga de arquitectura interna que un producto SaaS no debería mostrarle al
 * usuario — el backend es infraestructura de Sythrall, no algo que el
 * usuario deba percibir como "prendido" o "apagado" pieza por pieza. Una
 * sola línea, de disponibilidad del servicio en general (mismo dato que ya
 * calcula `state.backendOk`/`checkBackend()` en app.ts — no un chequeo
 * nuevo), sin nombrar "backend" ni ningún motor interno por separado. */
function _serviceStatusLine(): string {
  const checked = state.backendChecked
  const ok = checked && state.backendOk
  const dot = !checked ? 'var(--muted)' : ok ? 'var(--ok)' : 'var(--err)'
  const text = !checked ? 'Verificando…' : ok ? 'Todo operativo' : 'Servicio no disponible'
  return `<div class="dash-hero-status">
    <span class="dash-hero-dot" style="background:${dot}"></span>
    <span style="color:${dot}">${text}</span>
  </div>`
}

// ── Descubrimiento de capacidades ("Explora Sythrall") ────────────────────────
// Solo aparece en el hero vacío (primer contacto) — comunica desde el
// segundo 1 que Sythrall es una plataforma, no solo "subí tu código", sin
// mentir sobre qué existe todavía: cada tarjeta lleva un estado explícito
// (Disponible/Próximamente) en vez de mostrar las 6 como si ya funcionaran.
// Las "Disponible" navegan al tab real que ya cubre esa capacidad; las
// "Próximamente" no tienen destino — un click solo confirma que todavía no
// existe, no rompe ni finge llevar a algún lado.
interface ExploreCard {
  icon: IconName
  title: string
  desc: string
  tab?: TabId // presente solo si status es 'available'
}

const EXPLORE_AVAILABLE: ExploreCard[] = [
  {
    icon: 'diagram',
    title: 'Deep Analysis',
    desc: 'Arquitectura, complejidad, calidad y dependencias.',
    tab: 'diagram',
  },
  {
    icon: 'shield',
    title: 'Project Security',
    desc: 'Vulnerabilidades, secretos y dependencias riesgosas.',
    tab: 'static',
  },
]

const EXPLORE_SOON: ExploreCard[] = [
  { icon: 'puzzle', title: 'Plugins & Extensions', desc: 'Analizadores, reglas e integraciones a medida.' },
  { icon: 'ml', title: 'AI Models', desc: 'Conectá modelos de IA para potenciar el análisis.' },
  { icon: 'settings', title: 'Personaliza Sythrall', desc: 'Adaptá reglas, modelos y apariencia a tu flujo.' },
  { icon: 'users', title: 'Sythrall Family', desc: 'Compartí tu espacio de trabajo con tu equipo.' },
]

function _exploreCard(c: ExploreCard, available: boolean): string {
  return `
  <div class="dash-explore-card${available ? '' : ' dash-explore-soon'}"${available ? ` data-explore-tab="${c.tab}"` : ''}>
    <div class="dash-explore-head">
      ${icon(c.icon, 20)}
      <span class="dash-explore-badge ${available ? 'dash-explore-badge-ok' : 'dash-explore-badge-soon'}">${available ? 'Disponible' : 'Próximamente'}</span>
    </div>
    <div class="dash-explore-title">${esc(c.title)}</div>
    <div class="dash-explore-desc">${esc(c.desc)}</div>
  </div>`
}

function _renderExploreGrid(): string {
  const cards = [
    ...EXPLORE_AVAILABLE.map((c) => _exploreCard(c, true)),
    ...EXPLORE_SOON.map((c) => _exploreCard(c, false)),
  ].join('')
  return `
  <div class="dash-explore">
    <div class="dash-explore-heading">Explora Sythrall</div>
    <div class="dash-explore-grid">${cards}</div>
  </div>`
}

function _wireExploreGrid(root: ParentNode): void {
  root.querySelectorAll<HTMLElement>('[data-explore-tab]').forEach((el) => {
    el.addEventListener('click', () => _goto(el.dataset['exploreTab'] as TabId))
  })
  root.querySelectorAll<HTMLElement>('.dash-explore-soon').forEach((el) => {
    el.addEventListener('click', () => toast('Todavía no está disponible — pronto', 'info'))
  })
}

/** Estado vacío real (sin proyecto activo NI archivos sueltos) — el punto de
 * entrada de un usuario que recién abre Sythrall por primera vez. Antes era
 * un `<div class="empty">` con un mensaje de una línea; eso se sentía roto/
 * incompleto en vez de "todavía no hay nada que mostrar acá". El foco de
 * esta vista son las dos acciones (abrir/crear proyecto) — el estado del
 * servicio es una línea chica debajo, no el contenido principal. Debajo, el
 * grid de descubrimiento (ver arriba) — deliberadamente después de las
 * acciones, no antes: abrir/crear proyecto sigue siendo la acción principal
 * del hero, el grid es contexto adicional, no compite por atención. */
function _renderEmptyHero(): string {
  return `
  <div class="dash-hero">
    <div class="dash-hero-mark">SYTHRALL</div>
    <div class="dash-hero-tag">Code Intelligence Engine</div>
    <p class="dash-hero-desc">Analiza arquitectura, seguridad, complejidad y calidad de tu proyecto.</p>
    <div class="dash-hero-actions">
      <button class="btn btn-run" id="dash-hero-open">Abrir proyecto</button>
      <button class="btn btn-ghost" id="dash-hero-create">Crear proyecto</button>
    </div>
    ${_serviceStatusLine()}
  </div>
  ${_renderOverviewSection()}
  ${_renderExploreGrid()}`
}

function _wireEmptyHero(): void {
  document.getElementById('dash-hero-open')?.addEventListener('click', () => _goto('upload'))
  document.getElementById('dash-hero-create')?.addEventListener('click', () => {
    document.getElementById('btn-add-folder')?.click()
  })
  _wireExploreGrid(document)
}

export function renderDashboard(): void {
  const root = document.getElementById('dash-content')
  if (!root) return

  const data = state.results.projectDashboard
  if (!data) {
    if (!state.activeProjectId) {
      if (state.files.length) {
        root.innerHTML = _renderAdHocSummary()
        document.getElementById('dash-goto-projects-adhoc')?.addEventListener('click', () => _goto('upload'))
        _wireAdHocInteractivity(root)
        return
      }
      root.innerHTML = _renderEmptyHero()
      _wireEmptyHero()
      // El panorama entre proyectos (Fase 28) también se muestra acá, no
      // solo con un proyecto ya analizado: sin proyecto activo es
      // justamente cuando "¿cómo están todos mis proyectos?" es la pregunta
      // más útil, y clickear una fila activa ese proyecto directamente.
      void _ensureOverviewLoaded()
      _wireOverviewInteractivity(root)
    } else {
      // Hay proyecto abierto pero nadie lo analizó EN ESTA SESIÓN. Dos casos
      // muy distintos, y antes los dos mostraban lo mismo:
      //
      //   - Ya se analizó antes: hay `health`/`summary` guardados, así que se
      //     muestran con su fecha. Abrir un proyecto y ver sus números al
      //     instante es la diferencia entre un centro de control y una
      //     pantalla que pide permiso para mostrar algo.
      //   - Nunca se analizó: ahí sí no hay nada que mostrar, y la única
      //     acción posible queda como botón principal.
      //
      // Lo guardado va SIEMPRE con su fecha y un aviso de que puede haber
      // quedado viejo: nada acá sabe si los archivos cambiaron desde la
      // medición, así que presentarlo como el estado actual sería mentir.
      const guardado = state.activeProjectId ? leerUltimoAnalisis(state.activeProjectId) : null
      root.innerHTML = `
      <div class="dash-project-head">
        <span class="dash-project-lbl">Project</span>
        <span class="dash-project-name">${esc(_projectLabel())}</span>
        <button class="btn btn-ghost btn-sm ml-auto" id="dash-goto-projects-unrun">Cambiar</button>
      </div>
      ${
        guardado
          ? `
      <div class="dash-stale">
        <span class="dash-stale-txt">Último análisis guardado, ${esc(_fmtRelativeTime(guardado.at))} — puede no reflejar cambios posteriores en los archivos.</span>
        <button class="btn btn-run btn-sm" id="dash-health-run">Analizar de nuevo</button>
      </div>
      ${_renderStatsRow(guardado.summary, guardado.at)}
      <div class="chart-card wide card card-pad">
        <div class="cc-head">PROJECT HEALTH <span class="cc-head-sub meta">security · quality · complexity · architecture</span></div>
        ${renderHealthCards(guardado.health)}
      </div>`
          : `
      <div class="dash-unrun">
        <div class="dash-unrun-title">Todavía sin analizar</div>
        <p class="dash-unrun-desc">El análisis calcula seguridad, calidad, complejidad y arquitectura del proyecto. Hasta que corra, no hay métricas que mostrar acá.</p>
        <button class="btn btn-run" id="dash-health-run">Analizar proyecto</button>
      </div>`
      }
      ${_renderOverviewSection()}`
      document.getElementById('dash-health-run')?.addEventListener('click', () => {
        loadProjectHealth()
      })
      document.getElementById('dash-goto-projects-unrun')?.addEventListener('click', () => _goto('upload'))
      void _ensureOverviewLoaded()
      _wireOverviewInteractivity(root)
    }
    return
  }

  const findings = _mergedFindings(data)

  root.innerHTML = `
    <div class="dash-project-head">
      <span class="dash-project-lbl">Project</span>
      <span class="dash-project-name">${esc(_projectLabel())}</span>
      <button class="btn btn-ghost btn-sm ml-auto" id="dash-health-refresh">↻ Actualizar</button>
      <button class="btn btn-ghost btn-sm" id="dash-goto-projects">Cambiar</button>
    </div>

    ${_renderStatsRow(data.summary, _lastAnalyzedAt)}

    <div class="chart-card wide card card-pad">
      <div class="cc-head">PROJECT HEALTH <span class="cc-head-sub meta">security · quality · complexity · architecture</span></div>
      ${renderHealthCards(data.health, { clickable: true })}
    </div>

    ${state.activeProjectId ? _renderHistorySection(state.activeProjectId) : ''}

    <!-- Grid asimetrico: lo que se mira primero ocupa 2/3, el contexto 1/3.
         Antes eran seis bloques a ancho completo, uno debajo del otro. -->
    <div class="dash-grid">
      <div class="span-2">${_renderRecentFindings(findings)}</div>
      <div>${_renderFindingsBySeverity(findings)}</div>
    </div>

    <div class="dash-grid">
      <div class="span-2">${_renderComplexityByFunction(data)}</div>
      <div>${renderBigODistribution(data.summary.big_o_distribution ?? {}, 'Big-O')}</div>
    </div>

    ${_renderLanguages(data)}

    ${_renderOverviewSection()}
  `
  if (state.activeProjectId) {
    void _ensureHistoryLoaded(state.activeProjectId)
    _wireHistoryInteractivity(root, state.activeProjectId)
  }
  void _ensureOverviewLoaded()
  _wireOverviewInteractivity(root)
  document.getElementById('dash-health-refresh')?.addEventListener('click', () => loadProjectHealth())
  document.getElementById('dash-goto-projects')?.addEventListener('click', () => _goto('upload'))
  _wireInteractivity(root)
}
