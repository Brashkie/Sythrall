// ══════════════════════════════════════════
//  Sythrall — Tema claro/oscuro
// ══════════════════════════════════════════
// utils/theme.ts

import { icon } from './icons'

const THEME_KEY = 'theme'
type Theme = 'dark' | 'light'

/** Evento que este módulo emite en cada cambio de tema. Lo escuchan los
 * componentes que NO pueden leer variables CSS y por eso necesitan que les
 * avisen: Monaco y xterm definen sus temas con colores literales, así que
 * antes se quedaban en oscuro para siempre aunque toda la app pasara a
 * claro. Un evento del DOM en vez de que este módulo importe Monaco: `initTheme`
 * corre ANTES de que el editor exista, y no tiene por qué arrastrar ese peso. */
export const THEME_EVENT = 'sythrall:theme'

function applyTheme(theme: Theme): void {
  document.documentElement.dataset['theme'] = theme
  try {
    localStorage.setItem(THEME_KEY, theme)
  } catch {
    /* modo privado / storage bloqueado — el tema igual se aplica en esta sesión */
  }
  const btn = document.getElementById('theme-toggle-btn')
  if (btn) {
    btn.innerHTML = theme === 'light' ? icon('sun') : icon('moon')
    btn.setAttribute('aria-label', theme === 'light' ? 'Cambiar a tema oscuro' : 'Cambiar a tema claro')
    btn.setAttribute('aria-pressed', String(theme === 'light'))
  }
  document.dispatchEvent(new CustomEvent<Theme>(THEME_EVENT, { detail: theme }))
}

/** Llamar lo antes posible (antes de initApp) para evitar un flash del tema
 * incorrecto. Sin preferencia guardada respeta la del sistema
 * (`prefers-color-scheme`) en vez de imponer oscuro: alguien con el sistema
 * en claro abría Sythrall por primera vez y recibía una pantalla negra sin
 * haber elegido nada. Con una preferencia guardada, esa gana siempre. */
export function initTheme(): void {
  let saved: string | null = null
  try {
    saved = localStorage.getItem(THEME_KEY)
  } catch {
    /* storage bloqueado — se cae a la preferencia del sistema */
  }
  if (saved === 'light' || saved === 'dark') {
    applyTheme(saved)
    return
  }
  const prefersLight = window.matchMedia?.('(prefers-color-scheme: light)').matches ?? false
  applyTheme(prefersLight ? 'light' : 'dark')
}

export function toggleTheme(): void {
  const current = document.documentElement.dataset['theme'] === 'light' ? 'light' : 'dark'
  applyTheme(current === 'light' ? 'dark' : 'light')
}

/** Lee el tema activo — usado por el modal de Configuración para marcar
 * cuál de los 2 botones (Claro/Oscuro) está seleccionado. */
export function getTheme(): Theme {
  return document.documentElement.dataset['theme'] === 'light' ? 'light' : 'dark'
}

/** A diferencia de `toggleTheme` (alterna entre los 2), esto fija un tema
 * específico — lo necesita el modal de Configuración, donde Claro/Oscuro
 * son 2 botones independientes, no un solo toggle. */
export function setTheme(theme: Theme): void {
  applyTheme(theme)
}
