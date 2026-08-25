// ══════════════════════════════════════════
//  Sythrall — Flow Diagram
//  Estados derivados de datos reales en cada render (nunca decorativos ni
//  "en progreso" inventado) — ver `_computeSteps()`. Reemplaza el viejo
//  esquema de `setStep(id, estado)` escrito a mano en cada paso de
//  `runAll()`, que solo cubría el pipeline ad-hoc de "▶ Analizar" y mezclaba
//  "APIs" (una preocupación separada, no una etapa de análisis de código)
//  con el resto — pedido explícito del usuario: "APIs no deberían aparecer
//  como primera etapa del análisis de código".
// ══════════════════════════════════════════
// src/components/flow.ts
import { state } from '../store/state'
import type { RunHistoryEntry, StepState } from '../types'

const STEPS = [
  { id: 'project', name: 'Proyecto' },
  { id: 'files', name: 'Archivos' },
  { id: 'analyze', name: 'Análisis' },
  { id: 'findings', name: 'Findings' },
  { id: 'report', name: 'Reporte' },
]

const ICONS: Record<StepState, string> = { idle: '·', run: '◌', ok: '✓', err: '✗', warn: '⚠' }
const COLORS: Record<StepState, string> = {
  idle: 'var(--muted)',
  run: 'var(--warn)',
  ok: 'var(--ok)',
  err: 'var(--err)',
  warn: 'var(--warn)',
}

/** Deriva el estado de las 5 etapas de datos reales ya existentes en vez de
 * un flag manual por paso — cubre tanto el modo "proyecto activo" (Static/
 * Dashboard, `state.results.projectDashboard` + `projectAnalysisRunning`)
 * como el modo suelto ("▶ Analizar" del topbar sobre `state.files`, ya
 * usaba `state.running`). Sin distinción granular de sub-etapa dentro de
 * "en curso" — el pipeline ad-hoc corre varias cosas (APIs, lint, logs) en
 * una sola pasada sin una señal separada por sub-parte, así que se muestra
 * un único "en curso" honesto para toda la duración en vez de inventar
 * hitos intermedios que no existen. */
function _computeSteps(): Record<string, StepState> {
  const hasProject = !!state.activeProjectId
  const hasFiles = hasProject || state.files.length > 0
  const running = state.running || state.projectAnalysisRunning
  const analyzed = hasProject ? !!state.results.projectDashboard : state.files.some((f) => f.analyzed)

  return {
    project: hasProject ? 'ok' : 'idle',
    files: hasFiles ? 'ok' : 'idle',
    analyze: running ? 'run' : analyzed ? 'ok' : 'idle',
    findings: analyzed ? 'ok' : 'idle',
    report: analyzed ? 'ok' : 'idle',
  }
}

export function renderFlow(): void {
  const el = document.getElementById('flow-diag')
  if (!el) return
  const steps = _computeSteps()
  el.innerHTML = STEPS.map((s, i) => {
    const st: StepState = steps[s.id] ?? 'idle'
    const isLast = i === STEPS.length - 1
    return `<div class="fstep">
      <div class="fconn">
        <div class="fnode fstate-${st}">${ICONS[st] ?? i + 1}</div>
        ${!isLast ? `<div class="fline ${st === 'ok' ? 'ok' : ''}"></div>` : ''}
      </div>
      <div class="fcontent">
        <div class="fname" style="color:${COLORS[st]}">${s.name}</div>
        <div class="fdetail">${getDetail(s.id, st)}</div>
      </div>
    </div>`
  }).join('')
}

function getDetail(id: string, st: StepState): string {
  if (st === 'idle') return 'Esperando...'
  if (st === 'run') return 'Procesando...'
  if (id === 'project') return state.activeProjectName || state.activeProjectId!.slice(0, 8)
  if (id === 'files') return state.activeProjectId ? 'Del proyecto activo' : `${state.files.length} archivo(s)`
  if (id === 'analyze') return `${state.results.issues.length} hallazgo(s)`
  if (id === 'findings') return `${state.results.issues.length} hallazgo(s)`
  if (id === 'report') return 'Listo'
  return ''
}

export function updateRunMeta(entry: RunHistoryEntry): void {
  const el = document.getElementById('run-meta')
  if (!el) return
  const mr = (k: string, v: unknown, color?: string) =>
    `<div class="metric-row"><span class="mr-k">${k}</span><span class="mr-v"${color ? ` style="color:${color}"` : ''}>${v}</span></div>`
  el.innerHTML = `
    <div style="font-size:.58rem;font-weight:700;letter-spacing:1.4px;text-transform:uppercase;color:var(--muted);margin-bottom:9px">ÚLTIMA EJECUCIÓN</div>
    ${mr('Hora', entry.ts)}
    ${mr('Duración', entry.ms + 'ms', 'var(--info)')}
    ${mr('Problemas', entry.issues, entry.issues ? 'var(--err)' : 'var(--ok)')}
    ${mr('APIs OK', entry.apiOk + '/' + state.urls.length, 'var(--ok)')}
  `
}
