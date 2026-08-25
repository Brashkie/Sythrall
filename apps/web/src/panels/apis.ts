// ══════════════════════════════════════════
//  Sythrall — APIs Panel
// ══════════════════════════════════════════
// panels/apis.ts

import type { StaticProjectResult } from '../api/client'
import { api } from '../api/client'
import { state } from '../store/state'
import { toast } from '../utils/helpers'
import { icon } from '../utils/icons'
import { renderProjectContextBanner, wireProjectContextBanner } from '../utils/projectHeader'
import { ARCH_SMELL_LABEL, NAMING_SMELL_LABEL, SMELL_LABEL, smellColorSeverity } from './static'

export function renderAPICards(): void {
  const el = document.getElementById('api-cards')
  if (!el) return
  if (!state.results.apis.length) {
    el.innerHTML = '<div class="empty">Agrega URLs</div>'
    return
  }
  el.innerHTML = state.results.apis
    .map((a) => {
      const colorMap: Record<string, string> = { ok: 'var(--ok)', warning: 'var(--warn)', down: 'var(--err)' }
      const c = colorMap[a.status] ?? 'var(--muted)'
      const hist = (a.history ?? []).slice(-10)
      const bars = hist
        .map((h) => {
          const hc = h.status === 'ok' ? 'var(--ok)' : h.status === 'warning' ? 'var(--warn)' : 'var(--err)'
          const hh = Math.min(22, Math.max(3, h.ms ? Math.round(h.ms / 25) : 3))
          return `<div style="width:5px;height:${hh}px;background:${hc};border-radius:2px;align-self:flex-end"></div>`
        })
        .join('')

      return `<div class="api-card">
      <div style="display:flex;align-items:center;gap:8px;margin-bottom:8px">
        <span style="width:8px;height:8px;border-radius:50%;background:${c};flex-shrink:0"></span>
        <span class="ac-url" style="color:${c}" title="${a.url}">${a.url}</span>
        <button class="btn btn-ghost btn-sm" data-recheck="${a.url}">↺</button>
      </div>
      <div class="ac-metrics">
        <div class="acm"><div class="acm-k">Estado</div><div class="acm-v" style="color:${c}">${a.status.toUpperCase()}</div></div>
        <div class="acm"><div class="acm-k">Respuesta</div><div class="acm-v" style="color:var(--info)">${a.ms != null ? a.ms + 'ms' : '—'}</div></div>
        <div class="acm"><div class="acm-k">HTTP</div><div class="acm-v">${a.code ?? '—'}</div></div>
      </div>
      ${a.error ? `<div style="margin-top:7px;font-family:var(--mono);font-size:.65rem;color:var(--err);background:rgba(255,51,102,.07);padding:6px;border-radius:5px">${icon('warning', 12)} ${a.error}</div>` : ''}
      ${hist.length ? `<div style="display:flex;align-items:flex-end;gap:2px;margin-top:8px;height:24px">${bars}</div>` : ''}
    </div>`
    })
    .join('')

  // Event delegation for re-check buttons
  el.onclick = async (e: MouseEvent) => {
    const url = (e.target as HTMLElement).dataset['recheck']
    if (url) {
      e.stopPropagation()
      await recheckAPI(url)
    }
  }
}

export async function recheckAPI(url: string): Promise<void> {
  try {
    const res = await api.checkUrls([url])
    const idx = state.results.apis.findIndex((a) => a.url === url)
    if (idx >= 0 && res.results[0]) state.results.apis[idx] = res.results[0]
    renderAPICards()
    toast(`↺ ${url}`, 'ok')
  } catch (e) {
    toast('Error: ' + (e as Error).message, 'err')
  }
}

export function filterAPIs(q: string): void {
  document.querySelectorAll<HTMLElement>('.api-card').forEach((el) => {
    el.style.display = el.textContent?.toLowerCase().includes(q.toLowerCase()) ? '' : 'none'
  })
}

// ══════════════════════════════════════════
//  Issues Panel
// ══════════════════════════════════════════
import type { Issue } from '../types'

interface IssueFilter {
  sev: string
  tool: string
}
const issueFilter: IssueFilter = { sev: 'all', tool: '' }

const _SEV_MAP: Record<'High' | 'Medium' | 'Low', Issue['severity']> = { High: 'error', Medium: 'warning', Low: 'info' }

/** Convierte los hallazgos ricos del proyecto (security/structural/naming/
 * architecture — los mismos que ya muestra Static/Dashboard) a la forma
 * `Issue[]` que esta lista ya sabía renderizar, en vez de construir un
 * segundo pipeline de render paralelo. Sin esto, "Hallazgos" con un proyecto
 * activo solo mostraba lint de flake8/pylint por archivo — una fuente
 * mucho más pobre que la que Static/Dashboard ya calculan para el mismo
 * proyecto en el mismo momento. */
function _projectFindingsToIssues(data: StaticProjectResult): Issue[] {
  const out: Issue[] = []
  for (const f of data.security_findings ?? []) {
    out.push({
      tool: 'security',
      severity: _SEV_MAP[f.severity],
      message: `${f.category}: ${f.recommendation}`,
      file: f.file,
      line: f.line,
      code: f.cwe,
    })
  }
  for (const s of data.structural_smells ?? []) {
    const [label, color] = SMELL_LABEL[s.kind] ?? [s.kind, 'var(--muted)']
    out.push({
      tool: 'structural',
      severity: _SEV_MAP[smellColorSeverity(color)],
      message: `${label}: ${s.message}`,
      file: s.file,
      line: s.line,
    })
  }
  for (const s of data.naming_smells ?? []) {
    const [label, color] = NAMING_SMELL_LABEL[s.kind] ?? [s.kind, 'var(--muted)']
    out.push({
      tool: 'naming',
      severity: _SEV_MAP[smellColorSeverity(color)],
      message: `${label}: ${s.message}`,
      file: s.file,
      line: s.line,
    })
  }
  for (const s of data.architecture_smells ?? []) {
    const [label, color] = ARCH_SMELL_LABEL[s.kind] ?? [s.kind, 'var(--muted)']
    out.push({
      tool: 'architecture',
      severity: _SEV_MAP[smellColorSeverity(color)],
      message: `${label}: ${s.message}`,
      file: s.name,
      line: s.line,
    })
  }
  return out
}

/** Fuente de hallazgos vigente: si hay proyecto activo y ya se analizó (el
 * mismo `StaticProjectResult` que Static/Dashboard ya pidieron — no se
 * dispara un segundo `parse-project` acá), los hallazgos ricos del proyecto;
 * si no, la lista plana de lint por archivo de siempre (`state.results.issues`,
 * modo suelto o proyecto todavía sin analizar). */
export function currentIssueSource(): Issue[] {
  if (state.activeProjectId && state.results.projectDashboard) {
    return _projectFindingsToIssues(state.results.projectDashboard)
  }
  return state.results.issues
}

export function renderIssuesList(list?: Issue[]): void {
  const el = document.getElementById('issues-list')
  if (!el) return
  const projectScoped = !list && !!state.activeProjectId && !!state.results.projectDashboard
  let items = list ?? currentIssueSource()
  if (issueFilter.sev !== 'all') items = items.filter((i) => i.severity === issueFilter.sev)
  if (issueFilter.tool) items = items.filter((i) => i.tool === issueFilter.tool)

  const banner = projectScoped ? renderProjectContextBanner() : ''

  if (!items.length) {
    if (state.activeProjectId && !state.results.projectDashboard) {
      el.innerHTML = `${banner}<div class="empty">Proyecto activo, todavía sin analizar</div>`
    } else if (!state.files.length && !state.activeProjectId) {
      el.innerHTML = `<div class="empty">
        Todavía no cargaste archivos para analizar
        <button class="btn btn-ghost btn-sm" id="issues-empty-cta">+ Código</button>
      </div>`
      document.getElementById('issues-empty-cta')?.addEventListener('click', () => {
        document.getElementById('btn-add-code')?.click()
      })
    } else {
      el.innerHTML = `${banner}<div class="empty" style="color:var(--ok)">Sin problemas detectados</div>`
    }
    wireProjectContextBanner(el)
    return
  }
  el.innerHTML =
    banner +
    _groupByLocation(items)
      .map((group, idx) => {
        const first = group[0]
        const maxSev = group.reduce(
          (sev, i) => (SEV_RANK[i.severity] < SEV_RANK[sev] ? i.severity : sev),
          first.severity,
        )
        return `
    <div class="issue-item ii-${maxSev}" data-file="${first.file ?? ''}" style="animation-delay:${idx * 0.02}s">
      <div class="ii-head">
        <span class="ii-sev sev-${maxSev}">${maxSev.toUpperCase()}</span>
        <span class="ii-file">${first.file ?? ''}</span>
        ${first.line ? `<span style="font-family:var(--mono);font-size:.6rem;color:var(--muted)">:${first.line}</span>` : ''}
      </div>
      ${group
        .map(
          (iss) => `
      <div class="ii-sub">
        <span class="ii-tool t-${iss.tool}">${iss.tool}</span>
        ${iss.code ? `<span style="font-family:var(--mono);font-size:.6rem;color:var(--muted)">${iss.code}</span>` : ''}
        <span class="ii-msg">${iss.message ?? ''}</span>
        ${iss.preview ? `<div class="ii-preview">→ ${iss.preview.substring(0, 70)}</div>` : ''}
      </div>`,
        )
        .join('')}
    </div>`
      })
      .join('')
  wireProjectContextBanner(el)
}

const SEV_RANK: Record<string, number> = { error: 0, warning: 1, info: 2 }

// Agrupa por ubicación (file:line) en vez de intentar detectar "estos dos
// issues son la misma cosa" por contenido (frágil — flake8 y pylint no
// siempre coinciden en el texto exacto para el mismo hallazgo, ej. import no
// usado). Un solo header (severidad máxima del grupo) en vez de repetirlo por
// cada tool que reporta sobre la misma línea — sin ocultar ningún mensaje,
// cada uno sigue listado como su propia sub-fila. Grupos de tamaño 1 (el caso
// común, no hay nada co-ubicado) quedan visualmente idénticos a antes.
function _groupByLocation(items: Issue[]): Issue[][] {
  const map = new Map<string, Issue[]>()
  for (const iss of items) {
    const key = `${iss.file ?? ''}:${iss.line ?? 0}`
    const arr = map.get(key)
    if (arr) arr.push(iss)
    else map.set(key, [iss])
  }
  return [...map.values()]
}

export function setIssueFilter(sev: string | null, tool?: string): void {
  if (sev !== null) issueFilter.sev = sev ?? 'all'
  if (tool !== undefined) issueFilter.tool = tool ?? ''
  renderIssuesList()
}

export function filterIssues(q: string): void {
  if (!q) {
    renderIssuesList()
    return
  }
  const low = q.toLowerCase()
  renderIssuesList(
    currentIssueSource().filter((i) =>
      ((i.file ?? '') + (i.message ?? '') + (i.code ?? '')).toLowerCase().includes(low),
    ),
  )
}
