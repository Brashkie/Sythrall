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
// ══════════════════════════════════════════

import type {
  NamingSmell,
  SecurityFinding,
  StaticLanguagesResult,
  StaticProjectResult,
  StructuralSmell,
} from '../api/client'
import { api } from '../api/client'
import { state } from '../store/state'
import type { TabId } from '../types'
import { renderHealthCards } from '../utils/health'
import { esc, toast } from '../utils/helpers'
import { icon } from '../utils/icons'
import type { IconName } from '../utils/icons'
import {
  NAMING_SMELL_LABEL,
  renderBigODistribution,
  SMELL_LABEL,
  securitySeverityColor,
  smellColorSeverity,
} from './static'

// Capacidad real del engine por lenguaje (tree-sitter instalado o no, etc.)
// — cambia solo con el deploy, no por proyecto, así que se pide una vez y
// se cachea acá en vez de repetir el fetch en cada análisis.
let _engineLanguages: StaticLanguagesResult | null = null

// Momento del último análisis exitoso EN ESTA SESIÓN del navegador — no hay
// persistencia server-side de corridas pasadas todavía, así que esto se
// resetea al recargar la página; se muestra como tal, no como un historial
// real (ver Project Health Trend, todavía sin implementar por la misma razón).
let _lastAnalyzedAt: number | null = null

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
  try {
    const [data] = await Promise.all([
      api.staticParseProjectById(state.activeProjectId),
      _engineLanguages ? Promise.resolve(_engineLanguages) : api.staticLanguages().then((r) => (_engineLanguages = r)),
    ])
    state.results.projectDashboard = data
    // El grid de Proyectos (panels/upload.ts) lee este cache para mostrar
    // badges de health reales sin tener que disparar su propio análisis —
    // nunca un score inventado, solo lo que ya se calculó de verdad acá.
    if (state.activeProjectId) state.projectHealthCache[state.activeProjectId] = data.health
    _lastAnalyzedAt = Date.now()
  } catch (e) {
    toast('Error: ' + (e as Error).message, 'err')
  }
  state.projectAnalysisRunning = false
  renderFlow()
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

function _renderStatsRow(data: StaticProjectResult): string {
  const s = data.summary
  const findingsTotal = (s.security_findings ?? 0) + (s.structural_smells ?? 0) + (s.naming_smells ?? 0)
  const cells: Array<[string, string]> = [
    [String(s.total_files ?? 0), 'Files'],
    [_fmtLoc(s.total_loc ?? 0), 'LOC'],
    [String(findingsTotal), 'Findings'],
    [_lastAnalyzedAt ? _fmtRelativeTime(_lastAnalyzedAt) : '—', 'Last Analysis'],
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
      <div class="dash-sev-row"><span style="color:var(--err)">High</span>${bar(high, 'var(--err)')}<span>${high}</span></div>
      <div class="dash-sev-row"><span style="color:var(--warn)">Medium</span>${bar(medium, 'var(--warn)')}<span>${medium}</span></div>
      <div class="dash-sev-row"><span style="color:var(--muted)">Low</span>${bar(low, 'var(--muted)')}<span>${low}</span></div>
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
  <div class="chart-card wide" style="background:var(--s1);border:1px solid var(--b0);border-radius:var(--radius);padding:14px">
    <div class="cc-head"><span>COMPLEXITY BY FUNCTION</span><span style="color:var(--muted);font-size:.58rem;font-family:var(--mono)">cyclomatic · Big-O</span></div>
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
  <div class="chart-card wide" style="background:var(--s1);border:1px solid var(--b0);border-radius:var(--radius);padding:14px">
    <div class="cc-head"><span>LANGUAGES</span></div>
    <div style="padding:4px 0">${distRows}</div>
    ${
      support
        ? `<div class="cc-head" style="margin-top:10px"><span>LANGUAGE SUPPORT</span><span style="color:var(--muted);font-size:.58rem;font-family:var(--mono)">capacidad real del engine</span></div>
    <div class="dash-lang-support-grid">${support}</div>`
        : ''
    }
  </div>`
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
      <button class="btn btn-ghost btn-sm" id="dash-goto-projects-adhoc" style="margin-left:auto">Proyectos</button>
    </div>
    <div class="st-summary-row">
      ${cells.map(([val, lbl]) => `<div class="stat-card"><div class="sc-lbl">${lbl}</div><div class="sc-val">${esc(val)}</div></div>`).join('')}
    </div>
    <div class="chart-card wide" style="background:var(--s1);border:1px solid var(--b0);border-radius:var(--radius);padding:14px">
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
  { icon: 'diagram', title: 'Deep Analysis', desc: 'Arquitectura, complejidad, calidad y dependencias.', tab: 'diagram' },
  { icon: 'shield', title: 'Project Security', desc: 'Vulnerabilidades, secretos y dependencias riesgosas.', tab: 'static' },
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
  const cards = [...EXPLORE_AVAILABLE.map((c) => _exploreCard(c, true)), ...EXPLORE_SOON.map((c) => _exploreCard(c, false))].join('')
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
    } else {
      root.innerHTML = `
      <div class="empty">
Proyecto activo, todavía sin analizar
        <button class="btn btn-ghost btn-sm" id="dash-health-run">Analizar salud del proyecto</button>
      </div>`
      document.getElementById('dash-health-run')?.addEventListener('click', () => {
        loadProjectHealth()
      })
    }
    return
  }

  const findings = _mergedFindings(data)

  root.innerHTML = `
    <div class="dash-project-head">
      <span class="dash-project-lbl">Project</span>
      <span class="dash-project-name">${esc(_projectLabel())}</span>
      <button class="btn btn-ghost btn-sm" id="dash-health-refresh" style="margin-left:auto">↻ Actualizar</button>
      <button class="btn btn-ghost btn-sm" id="dash-goto-projects">Cambiar</button>
    </div>

    ${_renderStatsRow(data)}

    <div class="chart-card wide" style="background:var(--s1);border:1px solid var(--b0);border-radius:var(--radius);padding:14px">
      <div class="cc-head">PROJECT HEALTH <span class="cc-head-sub" style="color:var(--muted);font-size:.58rem;font-family:var(--mono)">security · quality · complexity · architecture</span></div>
      ${renderHealthCards(data.health, { clickable: true })}
    </div>

    <div class="charts-row">
      ${_renderRecentFindings(findings)}
      ${_renderFindingsBySeverity(findings)}
    </div>

    ${renderBigODistribution(data.summary.big_o_distribution ?? {}, 'Complexity Distribution (Big-O)')}

    ${_renderComplexityByFunction(data)}

    ${_renderLanguages(data)}
  `
  document.getElementById('dash-health-refresh')?.addEventListener('click', () => loadProjectHealth())
  document.getElementById('dash-goto-projects')?.addEventListener('click', () => _goto('upload'))
  _wireInteractivity(root)
}
