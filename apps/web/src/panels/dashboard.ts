// ══════════════════════════════════════════
//  Sythrall — Panel Dashboard: Project Health (Fase 2 del rediseño UX)
//  4 scores agregados a nivel de proyecto (Security/Quality/Complexity/
//  Architecture), cada uno con su porqué visible — no un número mágico.
// ══════════════════════════════════════════

import { api } from '../api/client'
import { state } from '../store/state'
import { renderHealthCards } from '../utils/health'
import { toast } from '../utils/helpers'

export async function loadProjectHealth(): Promise<void> {
  if (!state.activeProjectId) {
    toast('Elegí un proyecto activo en Proyectos primero', 'warn')
    return
  }

  const root = document.getElementById('dash-health')
  if (root) {
    root.innerHTML = `<div class="empty">Analizando salud del proyecto...</div>`
  }

  try {
    const data = await api.staticParseProjectById(state.activeProjectId)
    state.results.projectHealth = data.health
  } catch (e) {
    toast('Error: ' + (e as Error).message, 'err')
  }
  renderProjectHealth()
}

export function renderProjectHealth(): void {
  const root = document.getElementById('dash-health')
  if (!root) return

  const health = state.results.projectHealth
  if (!health) {
    if (!state.activeProjectId) {
      root.innerHTML = `
      <div class="empty">
Elegí un proyecto activo para ver su Project Health
        <button class="btn btn-ghost btn-sm" id="dash-health-goto-projects">Proyectos</button>
      </div>`
      document.getElementById('dash-health-goto-projects')?.addEventListener('click', () => {
        import('../components/app').then((m) => m.switchTab?.('upload'))
      })
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

  root.innerHTML = `
    ${renderHealthCards(health)}
    <div style="text-align:right;margin-top:6px">
      <button class="btn btn-ghost btn-sm" id="dash-health-refresh">↻ Actualizar</button>
    </div>`
  document.getElementById('dash-health-refresh')?.addEventListener('click', () => {
    loadProjectHealth()
  })
}
