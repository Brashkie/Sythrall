// ══════════════════════════════════════════
//  Sythrall — Project Health (Fase 2 del rediseño UX)
//  Tarjetas de score compartidas entre panels/dashboard.ts y panels/static.ts
//  (misma vista, dos lugares: el Dashboard y la vista de proyecto de Static).
// ══════════════════════════════════════════

import type { ProjectHealth } from '../api/client'
import type { TabId } from '../types'
import { esc } from './helpers'
import { icon } from './icons'

const HEALTH_META: Array<{
  key: keyof ProjectHealth
  label: string
  why: (h: ProjectHealth) => string
}> = [
  {
    key: 'security',
    label: `${icon('shield', 13)} Security`,
    why: (h) => `${h.security.high} High · ${h.security.medium} Medium · ${h.security.low} Low`,
  },
  {
    key: 'quality',
    label: 'Quality',
    why: (h) =>
      `${h.quality.smells} structural smell${h.quality.smells === 1 ? '' : 's'} · ${h.quality.naming} naming smell${h.quality.naming === 1 ? '' : 's'}`,
  },
  {
    key: 'complexity',
    label: 'Complexity',
    why: (h) => `CC promedio ${h.complexity.avg_complexity}`,
  },
  {
    key: 'architecture',
    label: 'Architecture',
    why: (h) =>
      `${h.architecture.cycles} import cycle${h.architecture.cycles === 1 ? '' : 's'}` +
      (h.architecture.smells
        ? ` · ${h.architecture.smells} coupling smell${h.architecture.smells === 1 ? '' : 's'}`
        : ''),
  },
]

/** verde/amarillo/rojo por rango — mismo tiering de 3 niveles que ya usa
 * el promedio de CC (static.ts). */
export function healthTier(score: number): string {
  return score >= 80 ? 'var(--ok)' : score >= 50 ? 'var(--warn)' : 'var(--err)'
}

/** A qué tab lleva cada score cuando las tarjetas son clickeables (solo el
 * Dashboard las usa así hoy — la misma vista dentro de Static navegando a sí
 * misma no tendría sentido, así que es opt-in vía `clickable`). */
const HEALTH_TAB_TARGET: Record<keyof ProjectHealth, TabId> = {
  security: 'static',
  quality: 'static',
  complexity: 'metrics',
  architecture: 'diagram',
}

/** Fila de 4 tarjetas (Security/Quality/Complexity/Architecture) — cada score
 * muestra el número crudo detrás (ej. "3 High · 1 Medium · 0 Low"), nunca un
 * número sin su porqué. Con `clickable: true`, cada tarjeta gana
 * `data-health-nav` con el tab al que navegar — el caller es responsable de
 * escuchar el click (ver panels/dashboard.ts), acá solo se marca el DOM. */
export function renderHealthCards(health: ProjectHealth, opts: { clickable?: boolean } = {}): string {
  return `
  <div class="st-summary-row health-row">
    ${HEALTH_META.map(({ key, label, why }) => {
      const score = health[key].score
      const navAttr = opts.clickable ? ` data-health-nav="${HEALTH_TAB_TARGET[key]}" style="cursor:pointer"` : ''
      return `
    <div class="st-stat health-stat"${navAttr}>
      <div class="st-stat-val" style="color:${healthTier(score)}">${score}</div>
      <div class="st-stat-lbl">${label}</div>
      <div class="health-why">${esc(why(health))}</div>
    </div>`
    }).join('')}
  </div>`
}
