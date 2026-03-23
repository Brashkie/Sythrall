// ══════════════════════════════════════════
//  CodeWatch PRO — Flow Diagram
// ══════════════════════════════════════════
import { state } from '../store/state'
import type { StepState } from '../types'
import type { RunHistoryEntry } from '../types'

const STEPS = [
  { id: 'api',     name: 'APIs',        icon: '📡' },
  { id: 'upload',  name: 'Archivos',    icon: '📤' },
  { id: 'analyze', name: 'Análisis',    icon: '🔬' },
  { id: 'logs',    name: 'Logs',        icon: '📋' },
  { id: 'report',  name: 'Reporte',     icon: '📄' },
]

const ICONS:  Record<StepState, string> = { idle:'·', run:'◌', ok:'✓', err:'✗', warn:'⚠' }
const COLORS: Record<StepState, string> = {
  idle: 'var(--muted)', run: 'var(--warn)', ok: 'var(--ok)', err: 'var(--err)', warn: 'var(--warn)',
}

export function renderFlow(): void {
  const el = document.getElementById('flow-diag')
  if (!el) return
  el.innerHTML = STEPS.map((s, i) => {
    const st: StepState = (state.steps[s.id] as StepState) ?? 'idle'
    const isLast = i === STEPS.length - 1
    return `<div class="fstep">
      <div class="fconn">
        <div class="fnode fstate-${st}">${ICONS[st] ?? i + 1}</div>
        ${!isLast ? `<div class="fline ${st === 'ok' ? 'ok' : ''}"></div>` : ''}
      </div>
      <div class="fcontent">
        <div class="fname" style="color:${COLORS[st]}">${s.icon} ${s.name}</div>
        <div class="fdetail">${getDetail(s.id, st)}</div>
      </div>
    </div>`
  }).join('')
}

export function setStep(id: string, st: StepState): void {
  state.steps[id] = st
  renderFlow()
}

function getDetail(id: string, st: StepState): string {
  if (st === 'idle') return 'Esperando...'
  if (st === 'run')  return 'Procesando...'
  if (id === 'api')     return `${state.results.apis.filter(a => a.status === 'ok').length}/${state.urls.length} activas`
  if (id === 'upload')  return `${state.files.length} archivos`
  if (id === 'analyze') return `${state.results.issues.length} issue(s)`
  if (id === 'logs')    return `${state.results.logErrors.length} errores`
  if (id === 'report')  return 'Completo ✓'
  return ''
}

export function updateRunMeta(entry: RunHistoryEntry): void {
  const el = document.getElementById('run-meta')
  if (!el) return
  const mr = (k: string, v: unknown, color?: string) =>
    `<div class="metric-row"><span class="mr-k">${k}</span><span class="mr-v"${color ? ` style="color:${color}"` : ''}>${v}</span></div>`
  el.innerHTML = `
    <div style="font-size:.58rem;font-weight:700;letter-spacing:1.4px;text-transform:uppercase;color:var(--muted);margin-bottom:9px">ÚLTIMA EJECUCIÓN</div>
    ${mr('Hora',      entry.ts)}
    ${mr('Duración',  entry.ms + 'ms', 'var(--info)')}
    ${mr('Problemas', entry.issues, entry.issues ? 'var(--err)' : 'var(--ok)')}
    ${mr('APIs OK',   entry.apiOk + '/' + state.urls.length, 'var(--ok)')}
  `
}
