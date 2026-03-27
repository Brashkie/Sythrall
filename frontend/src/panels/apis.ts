// ══════════════════════════════════════════
//  CodeWatch PRO — APIs Panel
// ══════════════════════════════════════════
// panels/apis.ts
import { state } from '../store/state'
import { api } from '../api/client'
import { toast } from '../utils/helpers'
import { renderRTChart } from '../components/charts'

export function renderAPICards(): void {
  const el = document.getElementById('api-cards')
  if (!el) return
  if (!state.results.apis.length) {
    el.innerHTML = '<div class="empty"><span class="empty-icon">📡</span>Agrega URLs</div>'
    return
  }
  el.innerHTML = state.results.apis.map(a => {
    const colorMap: Record<string, string> = { ok: 'var(--ok)', warning: 'var(--warn)', down: 'var(--err)' }
    const iconMap:  Record<string, string> = { ok: '✅', warning: '⚠️', down: '❌' }
    const c    = colorMap[a.status] ?? 'var(--muted)'
    const icon = iconMap[a.status]  ?? '❓'
    const hist = (a.history ?? []).slice(-10)
    const bars = hist.map(h => {
      const hc = h.status === 'ok' ? 'var(--ok)' : h.status === 'warning' ? 'var(--warn)' : 'var(--err)'
      const hh = Math.min(22, Math.max(3, h.ms ? Math.round(h.ms / 25) : 3))
      return `<div style="width:5px;height:${hh}px;background:${hc};border-radius:2px;align-self:flex-end"></div>`
    }).join('')

    return `<div class="api-card">
      <div style="display:flex;align-items:center;gap:8px;margin-bottom:8px">
        <span style="font-size:15px">${icon}</span>
        <span class="ac-url" style="color:${c}" title="${a.url}">${a.url}</span>
        <button class="btn btn-ghost btn-sm" data-recheck="${a.url}">↺</button>
      </div>
      <div class="ac-metrics">
        <div class="acm"><div class="acm-k">Estado</div><div class="acm-v" style="color:${c}">${a.status.toUpperCase()}</div></div>
        <div class="acm"><div class="acm-k">Respuesta</div><div class="acm-v" style="color:var(--info)">${a.ms != null ? a.ms + 'ms' : '—'}</div></div>
        <div class="acm"><div class="acm-k">HTTP</div><div class="acm-v">${a.code ?? '—'}</div></div>
      </div>
      ${a.error ? `<div style="margin-top:7px;font-family:var(--mono);font-size:.65rem;color:var(--err);background:rgba(255,51,102,.07);padding:6px;border-radius:5px">⚠ ${a.error}</div>` : ''}
      ${hist.length ? `<div style="display:flex;align-items:flex-end;gap:2px;margin-top:8px;height:24px">${bars}</div>` : ''}
    </div>`
  }).join('')

  // Event delegation for re-check buttons
  el.onclick = async (e: MouseEvent) => {
    const url = (e.target as HTMLElement).dataset['recheck']
    if (url) { e.stopPropagation(); await recheckAPI(url) }
  }
}

export async function recheckAPI(url: string): Promise<void> {
  try {
    const res = await api.checkUrls([url])
    const idx = state.results.apis.findIndex(a => a.url === url)
    if (idx >= 0 && res.results[0]) state.results.apis[idx] = res.results[0]
    renderAPICards()
    renderRTChart()
    toast(`↺ ${url}`, 'ok')
  } catch (e) {
    toast('Error: ' + (e as Error).message, 'err')
  }
}

export function filterAPIs(q: string): void {
  document.querySelectorAll<HTMLElement>('.api-card').forEach(el => {
    el.style.display = el.textContent?.toLowerCase().includes(q.toLowerCase()) ? '' : 'none'
  })
}

// ══════════════════════════════════════════
//  Issues Panel
// ══════════════════════════════════════════
import type { Issue } from '../types'

interface IssueFilter { sev: string; tool: string }
const issueFilter: IssueFilter = { sev: 'all', tool: '' }

export function renderIssuesList(list?: Issue[]): void {
  const el = document.getElementById('issues-list')
  if (!el) return
  let items = list ?? state.results.issues
  if (issueFilter.sev !== 'all') items = items.filter(i => i.severity === issueFilter.sev)
  if (issueFilter.tool)          items = items.filter(i => i.tool === issueFilter.tool)
  if (!items.length) {
    el.innerHTML = '<div class="empty"><span class="empty-icon">✅</span>Sin problemas</div>'
    return
  }
  el.innerHTML = items.map((iss, idx) => `
    <div class="issue-item ii-${iss.severity}" data-file="${iss.file ?? ''}" style="animation-delay:${idx * .02}s">
      <div class="ii-head">
        <span class="ii-sev sev-${iss.severity}">${iss.severity.toUpperCase()}</span>
        <span class="ii-tool t-${iss.tool}">${iss.tool}</span>
        ${iss.code ? `<span style="font-family:var(--mono);font-size:.6rem;color:var(--muted)">${iss.code}</span>` : ''}
        <span class="ii-file">${iss.file ?? ''}</span>
        ${iss.line ? `<span style="font-family:var(--mono);font-size:.6rem;color:var(--muted)">:${iss.line}</span>` : ''}
      </div>
      <div class="ii-msg">${iss.message ?? ''}</div>
      ${iss.preview ? `<div class="ii-preview">→ ${iss.preview.substring(0, 70)}</div>` : ''}
    </div>`
  ).join('')
}

export function setIssueFilter(sev: string | null, tool?: string): void {
  if (sev !== null) issueFilter.sev = sev ?? 'all'
  if (tool !== undefined) issueFilter.tool = tool ?? ''
  renderIssuesList()
}

export function filterIssues(q: string): void {
  if (!q) { renderIssuesList(); return }
  const low = q.toLowerCase()
  renderIssuesList(state.results.issues.filter(i =>
    ((i.file ?? '') + (i.message ?? '') + (i.code ?? '')).toLowerCase().includes(low)
  ))
}