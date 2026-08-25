// ══════════════════════════════════════════
//  Sythrall — Banner de contexto "proyecto activo"
//  Generaliza ".dash-project-head" (antes solo en panels/dashboard.ts) para
//  paneles project-scoped que no son el Dashboard (Static/Métricas/
//  Arquitectura/Hallazgos) — misma idea: nombre del proyecto + "Cambiar",
//  para orientar sin construir una segunda barra de navegación aparte del
//  nav-rail (ver plan de la restructuración de sidebar/Proyectos).
// ══════════════════════════════════════════
// utils/projectHeader.ts

import { state } from '../store/state'
import { esc } from './helpers'

/** Vacío si no hay proyecto activo — el caller decide si mostrar esto o el
 * modo suelto/ad-hoc habitual de su panel. */
export function renderProjectContextBanner(): string {
  if (!state.activeProjectId) return ''
  const name = state.activeProjectName || `${state.activeProjectId.slice(0, 8)}…`
  return `
    <div class="project-context-banner" data-project-context-banner>
      <span class="project-context-lbl">Proyecto</span>
      <span class="project-context-name">${esc(name)}</span>
      <button class="btn btn-ghost btn-sm" data-project-context-switch style="margin-left:auto">Cambiar</button>
    </div>`
}

/** Wirea el botón "Cambiar" del banner — llamar después de insertar el html
 * de `renderProjectContextBanner()` en el DOM. No-op si el banner no está
 * presente (ej. modo suelto, sin proyecto activo). */
export function wireProjectContextBanner(container: ParentNode): void {
  container.querySelector('[data-project-context-switch]')?.addEventListener('click', () => {
    import('../components/app').then((m) => m.switchTab?.('upload'))
  })
}
