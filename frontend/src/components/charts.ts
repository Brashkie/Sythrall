// ══════════════════════════════════════════
//  CodeWatch PRO — Charts (Chart.js)
// ══════════════════════════════════════════
// src/components/charts.ts
import { Chart, registerables } from 'chart.js'
import type { ChartConfiguration } from 'chart.js'
import { state } from '../store/state'

Chart.register(...registerables)

const GRID_COLOR = 'rgba(30,37,69,.8)'
const TEXT_COLOR = 'rgba(200,212,240,.6)'
const FONT       = { family: "'DM Mono', monospace", size: 10 }

Chart.defaults.color      = TEXT_COLOR
Chart.defaults.font       = FONT as typeof Chart.defaults.font

const charts: Record<string, Chart> = {}

export function initCharts(): void {
  const rtCtx = (document.getElementById('chart-rt') as HTMLCanvasElement).getContext('2d')!
  charts['rt'] = new Chart(rtCtx, {
    type: 'bar',
    data: { labels: [], datasets: [{ label: 'ms', data: [], backgroundColor: 'rgba(61,158,255,.4)', borderColor: '#3d9eff', borderWidth: 1, borderRadius: 3 }] },
    options: {
      responsive: true, maintainAspectRatio: false, animation: { duration: 400 },
      plugins: { legend: { display: false } },
      scales: {
        x: { grid: { display: false }, ticks: { font: FONT } },
        y: { grid: { color: GRID_COLOR }, ticks: { callback: v => v + 'ms', font: FONT } },
      },
    },
  } as ChartConfiguration)

  const distCtx = (document.getElementById('chart-dist') as HTMLCanvasElement).getContext('2d')!
  charts['dist'] = new Chart(distCtx, {
    type: 'doughnut',
    data: {
      labels: ['Errors','Warnings','Info','APIs OK'],
      datasets: [{ data: [0,0,0,0], backgroundColor: ['#ff3366','#ffb627','#3d9eff','#00f5a0'], borderWidth: 0, hoverOffset: 4 }],
    },
    options: {
      responsive: true, maintainAspectRatio: false, cutout: '65%',
      animation: { duration: 600 },
      plugins: { legend: { position: 'right', labels: { boxWidth: 8, padding: 8, font: FONT } } },
    },
  } as ChartConfiguration)

  const histCtx = (document.getElementById('chart-hist') as HTMLCanvasElement).getContext('2d')!
  charts['hist'] = new Chart(histCtx, {
    type: 'line',
    data: {
      labels: [],
      datasets: [
        { label: 'Issues',  data: [], borderColor: '#ff3366', backgroundColor: 'rgba(255,51,102,.1)', tension: .4, fill: true, pointRadius: 2, borderWidth: 1.5 },
        { label: 'APIs OK', data: [], borderColor: '#00f5a0', backgroundColor: 'rgba(0,245,160,.08)',  tension: .4, fill: true, pointRadius: 2, borderWidth: 1.5 },
      ],
    },
    options: {
      responsive: true, maintainAspectRatio: false, animation: { duration: 300 },
      plugins: { legend: { position: 'top', labels: { boxWidth: 8, padding: 8, font: FONT } } },
      scales: {
        x: { grid: { display: false }, ticks: { maxTicksLimit: 10, font: FONT } },
        y: { grid: { color: GRID_COLOR }, beginAtZero: true, ticks: { font: FONT } },
      },
    },
  } as ChartConfiguration)
}

export function renderDistChart(): void {
  const c = charts['dist']; if (!c) return
  const errs  = state.results.issues.filter(i => i.severity === 'error').length
  const warns = state.results.issues.filter(i => i.severity === 'warning').length
  const infos = state.results.issues.filter(i => i.severity === 'info').length
  const ok    = state.results.apis.filter(a => a.status === 'ok').length
  ;(c.data.datasets[0].data as number[]) = [errs, warns, infos, ok]
  c.update('none')
}

export function renderRTChart(): void {
  const c = charts['rt']; if (!c) return
  const apis = state.results.apis.filter(a => a.ms != null)
  c.data.labels   = apis.map(a => a.url.replace(/https?:\/\//, '').substring(0, 16))
  ;(c.data.datasets[0].data as number[]) = apis.map(a => a.ms!)
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  ;(c.data.datasets[0] as any).backgroundColor = apis.map(a => a.ms! < 200 ? 'rgba(0,245,160,.4)' : a.ms! < 1000 ? 'rgba(255,182,39,.4)' : 'rgba(255,51,102,.4)')
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  ;(c.data.datasets[0] as any).borderColor = apis.map(a => a.ms! < 200 ? '#00f5a0' : a.ms! < 1000 ? '#ffb627' : '#ff3366')
  c.update()
  const avg = apis.length ? Math.round(apis.reduce((s, a) => s + a.ms!, 0) / apis.length) : 0
  const el  = document.getElementById('rt-avg')
  if (el) el.textContent = avg ? `avg ${avg}ms` : ''
}

export function updateHistChart(): void {
  const c = charts['hist']; if (!c) return
  const last = state.history.slice(-15)
  c.data.labels = last.map(h => h.ts.substring(0, 5))
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  ;(c.data.datasets[0].data as any) = last.map(h => h.issues)
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  ;(c.data.datasets[1].data as any) = last.map(h => h.apiOk)
  c.update()
  const el = document.getElementById('hist-n')
  if (el) el.textContent = `${state.history.length} ejecucion(es)`
}

export function renderComplexityBars(): void {
  const el = document.getElementById('complexity-bars')!
  const files = state.files.filter(f => f.metrics?.complexity?.length)
  if (!files.length) { el.innerHTML = '<div style="color:var(--muted);font-size:.72rem">Sin datos</div>'; return }
  const fns   = files.flatMap(f => (f.metrics.complexity ?? []).map(c => ({ ...c, fid: f.id })))
  const maxCC = Math.max(...fns.map(c => c.complexity), 1)
  const rankColor: Record<string,string> = { A:'var(--ok)',B:'#8ef5c0',C:'var(--warn)',D:'#ff8a00',E:'var(--err)',F:'var(--err)' }
  el.innerHTML = fns.slice(0, 30).map(fn => {
    const h = Math.max(6, Math.round(fn.complexity / maxCC * 66))
    const c = rankColor[fn.rank] ?? 'var(--muted)'
    return `<div style="display:flex;flex-direction:column;align-items:center;gap:2px;cursor:pointer;flex-shrink:0" title="${fn.name} CC=${fn.complexity}" data-fid="${fn.fid}">
      <div style="font-family:var(--mono);font-size:.58rem;color:${c}">${fn.complexity}</div>
      <div style="width:18px;height:${h}px;background:${c};border-radius:2px;opacity:.75;transition:opacity .2s"
        onmouseenter="this.style.opacity='1'" onmouseleave="this.style.opacity='.75'"></div>
      <div style="font-family:var(--mono);font-size:.52rem;color:var(--muted);max-width:20px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${fn.name.substring(0,5)}</div>
    </div>`
  }).join('')
}
