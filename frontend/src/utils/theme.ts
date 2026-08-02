// ══════════════════════════════════════════
//  CodeWatch PRO — Tema claro/oscuro
// ══════════════════════════════════════════
// utils/theme.ts

const THEME_KEY = 'theme'
type Theme = 'dark' | 'light'

function applyTheme(theme: Theme): void {
  document.documentElement.dataset['theme'] = theme
  localStorage.setItem(THEME_KEY, theme)
  const btn = document.getElementById('theme-toggle-btn')
  if (btn) btn.textContent = theme === 'light' ? '☀️' : '🌙'
}

/** Llamar lo antes posible (antes de initApp) para evitar un flash del tema incorrecto. */
export function initTheme(): void {
  const saved = localStorage.getItem(THEME_KEY)
  applyTheme(saved === 'light' ? 'light' : 'dark')
}

export function toggleTheme(): void {
  const current = document.documentElement.dataset['theme'] === 'light' ? 'light' : 'dark'
  applyTheme(current === 'light' ? 'dark' : 'light')
}
