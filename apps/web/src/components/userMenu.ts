// ══════════════════════════════════════════
//  Sythrall — Menú de usuario + Modal de Configuración
// ══════════════════════════════════════════
// src/components/userMenu.ts
//
// Footer del nav-rail (avatar + nombre) con un menú desplegable — mismo
// patrón que claude.ai (pedido explícito del usuario, con esa UI como
// referencia directa). "Configuración" abre un modal aparte con una
// sidebar de secciones, no otro dropdown.
//
// Sythrall hoy NO tiene sistema de cuentas real (solo un token de terminal
// restringido a loopback, ver routers/auth.py) — el nombre/avatar acá son
// un placeholder estático, y "Cerrar sesión"/"Obtener ayuda" son stubs
// honestos (un toast, no una acción fingida) hasta que exista login real.
// Las secciones "Plugins"/"Conectores"/"Habilidades" son shells vacíos a
// propósito — adelantan la forma de la visión de Sythrall Ecosystem
// (Fase 24/30) sin inventar contenido que todavía no existe.

import { state } from '../store/state'
import { isDevMode, setDevMode } from '../utils/devMode'
import { esc, toast } from '../utils/helpers'
import { icon } from '../utils/icons'
import { getTheme, setTheme } from '../utils/theme'

type SettingsSection = 'general' | 'apariencia' | 'desarrollador' | 'plugins' | 'conectores' | 'habilidades'

let _menuOpen = false
let _modalOpen = false
let _activeSection: SettingsSection = 'general'
// Foco a restaurar al cerrar el modal — sin esto, cerrar con Escape/click
// afuera deja el foco del teclado tirado en el `<body>` en vez de volver al
// botón que lo abrió, rompiendo la navegación por teclado para quien venía
// usándola.
let _modalTrigger: HTMLElement | null = null

function _closeMenu(): void {
  _menuOpen = false
  document.getElementById('nav-user-menu')?.classList.remove('open')
  document.getElementById('nav-user-btn')?.setAttribute('aria-expanded', 'false')
}

function _toggleMenu(): void {
  _menuOpen = !_menuOpen
  document.getElementById('nav-user-menu')?.classList.toggle('open', _menuOpen)
  document.getElementById('nav-user-btn')?.setAttribute('aria-expanded', String(_menuOpen))
}

function _openModal(): void {
  _modalOpen = true
  _modalTrigger = document.activeElement as HTMLElement | null
  document.getElementById('settings-modal-overlay')?.classList.add('open')
  _renderSection(_activeSection)
  // Primer item de navegación en vez del botón de cerrar — coincide con
  // dónde cae la mirada al abrir un modal con sidebar (mismo criterio que
  // claude.ai, la referencia explícita de este componente).
  document.querySelector<HTMLElement>('.settings-modal-nav-item')?.focus()
}

function _closeModal(): void {
  _modalOpen = false
  document.getElementById('settings-modal-overlay')?.classList.remove('open')
  _modalTrigger?.focus()
  _modalTrigger = null
}

/** Tab-trap del modal de Configuración: mientras está abierto, Tab/Shift+Tab
 * nunca deben poder salir de sus elementos enfocables — sin esto el foco se
 * escapa al nav-rail/topbar de atrás, que queda visualmente tapado por el
 * overlay pero sigue siendo alcanzable por teclado. */
function _trapModalTab(e: KeyboardEvent): void {
  if (!_modalOpen || e.key !== 'Tab') return
  const modal = document.querySelector<HTMLElement>('.settings-modal')
  if (!modal) return
  const focusable = Array.from(
    modal.querySelectorAll<HTMLElement>('button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'),
  ).filter((el) => !el.hasAttribute('disabled'))
  if (!focusable.length) return
  const first = focusable[0]!
  const last = focusable[focusable.length - 1]!
  if (e.shiftKey && document.activeElement === first) {
    e.preventDefault()
    last.focus()
  } else if (!e.shiftKey && document.activeElement === last) {
    e.preventDefault()
    first.focus()
  }
}

/** Flechas arriba/abajo entre items del menú de usuario — mismo patrón de
 * navegación que cualquier `role="menu"` nativo (ver también el tab-trap del
 * modal, arriba). */
function _navigateMenu(e: KeyboardEvent): void {
  if (!_menuOpen || (e.key !== 'ArrowDown' && e.key !== 'ArrowUp')) return
  const items = Array.from(document.querySelectorAll<HTMLElement>('#nav-user-menu [role="menuitem"]'))
  if (!items.length) return
  e.preventDefault()
  const idx = items.indexOf(document.activeElement as HTMLElement)
  const next = e.key === 'ArrowDown' ? (idx + 1) % items.length : (idx - 1 + items.length) % items.length
  items[next]?.focus()
}

/** Shell vacío honesto — mismo criterio que el resto del engine: nunca
 * fingir contenido que no existe. `title`/`hint` describen qué va a vivir
 * acá cuando la Fase correspondiente (24/30, Sythrall Ecosystem) shippee
 * algo real. */
function _renderEmptyState(iconName: Parameters<typeof icon>[0], title: string, hint: string): string {
  return `<div class="settings-empty-state">
    ${icon(iconName, 40)}
    <div style="font-size:.95rem;font-weight:600;color:var(--txt)">${esc(title)}</div>
    <div>${esc(hint)}</div>
  </div>`
}

function _renderGeneral(): string {
  const name = document.getElementById('nav-user-name')?.textContent ?? 'Brashkie'
  return `
    <div class="settings-section-title">General</div>
    <div class="settings-row">
      <div>
        <div class="settings-row-label">Nombre</div>
        <div class="settings-row-hint">Cómo te mostramos en el nav-rail — no hay cuentas reales todavía.</div>
      </div>
      <input class="settings-row-input" id="settings-name-input" value="${esc(name)}" />
    </div>
    <div class="settings-row">
      <div>
        <div class="settings-row-label">Idioma</div>
        <div class="settings-row-hint">Sythrall es español-only por ahora — no hay selector de idioma real todavía.</div>
      </div>
      <button class="settings-theme-btn" id="settings-language-btn">${icon('globe', 14)} Español</button>
    </div>
    <div class="settings-row">
      <div>
        <div class="settings-row-label">Ayuda</div>
        <div class="settings-row-hint">Todavía no hay un centro de ayuda propio.</div>
      </div>
      <button class="settings-theme-btn" id="settings-help-btn">${icon('help', 14)} Obtener ayuda</button>
    </div>`
}

function _renderApariencia(): string {
  const current = getTheme()
  return `
    <div class="settings-section-title">Apariencia</div>
    <div class="settings-row">
      <div>
        <div class="settings-row-label">Tema</div>
        <div class="settings-row-hint">Se guarda en este navegador — no depende de una cuenta.</div>
      </div>
      <div class="settings-theme-options">
        <button class="settings-theme-btn ${current === 'light' ? 'active' : ''}" data-set-theme="light">${icon('sun', 14)} Claro</button>
        <button class="settings-theme-btn ${current === 'dark' ? 'active' : ''}" data-set-theme="dark">${icon('moon', 14)} Oscuro</button>
      </div>
    </div>`
}

/** Runtime real, sin inventar nada.
 *
 * **El frontend NO sabe en qué puerto vive el backend** — `getApiBase()`
 * devuelve cadena vacía a propósito, porque quien lo sabe es el proxy (Vite
 * en dev, nginx en prod). Poner ":8420" acá sería escribir un número que esta
 * capa no tiene forma de verificar, y este panel existe justamente para decir
 * la verdad sobre la infraestructura. Así que se muestra lo que sí se puede
 * afirmar: desde dónde se sirve la app, que la API se alcanza same-origin, y
 * lo que el propio backend reporta de sí mismo en `/capabilities`. */
function _renderRuntime(): string {
  const cap = state.capabilities
  const fila = (k: string, v: string, ok?: boolean) => `
    <div class="settings-runtime-row">
      <span class="settings-runtime-k">${esc(k)}</span>
      <span class="settings-runtime-v">${esc(v)}</span>
      ${ok === undefined ? '' : `<span class="settings-runtime-dot" style="background:${ok ? 'var(--ok)' : 'var(--err)'}"></span>`}
    </div>`
  return `
    <div class="settings-runtime">
      ${fila('Frontend', location.origin)}
      ${fila('API', 'same-origin, vía proxy', state.backendOk)}
      ${fila('Servidor', cap?.server ?? '—')}
      ${fila('Python', cap ? String(cap.python).split(' ')[0] : '—')}
      ${fila(
        'Motor de análisis',
        cap?.HAS_COMPLEXITY_ENGINE ? 'disponible' : 'no respondía al arrancar la API',
        !!cap?.HAS_COMPLEXITY_ENGINE,
      )}
    </div>`
}

function _renderDesarrollador(): string {
  const on = isDevMode()
  return `
    <div class="settings-section-title">Desarrollador</div>
    <div class="settings-row">
      <div>
        <div class="settings-row-label">Modo desarrollador</div>
        <div class="settings-row-hint">Muestra los atajos a los puertos locales de Sythrall en Servicios → APIs, y el detalle de runtime de acá abajo. Apagado por defecto: son detalles de infraestructura, no del producto.</div>
      </div>
      <div class="settings-theme-options">
        <button class="settings-theme-btn ${on ? '' : 'active'}" data-set-devmode="off">Desactivado</button>
        <button class="settings-theme-btn ${on ? 'active' : ''}" data-set-devmode="on">Activado</button>
      </div>
    </div>
    ${
      on
        ? `<div class="settings-row" style="display:block">
      <div class="settings-row-label mb-1">Runtime</div>
      <div class="settings-row-hint mb-2">El motor de análisis se detecta UNA vez, cuando arranca la API. En desarrollo el sidecar Rust suele tardar más en compilar que lo que la API espera, así que puede figurar caído aunque después funcione — reiniciá la API para que lo vuelva a detectar.</div>
      ${_renderRuntime()}
    </div>`
        : ''
    }`
}

const SECTION_RENDERERS: Record<SettingsSection, () => string> = {
  general: _renderGeneral,
  apariencia: _renderApariencia,
  desarrollador: _renderDesarrollador,
  plugins: () =>
    _renderEmptyState(
      'puzzle',
      'Plugins',
      'Todavía no hay ningún plugin de comunidad instalable — parte de la visión de Sythrall Ecosystem (Fase 24/30).',
    ),
  conectores: () =>
    _renderEmptyState(
      'globe',
      'Conectores',
      'SQL Server, GitHub, S3 y similares — ninguno shippeado todavía (Fase 24, Connectors).',
    ),
  habilidades: () =>
    _renderEmptyState(
      'shield',
      'Habilidades',
      'Capacidades especializadas instalables — ninguna shippeada todavía (Fase 24/30).',
    ),
}

function _renderSection(section: SettingsSection): void {
  const content = document.getElementById('settings-modal-content')
  if (content) content.innerHTML = SECTION_RENDERERS[section]()

  document.querySelectorAll<HTMLElement>('.settings-modal-nav-item').forEach((el) => {
    el.classList.toggle('active', el.dataset['settingsSection'] === section)
  })

  if (section === 'apariencia') {
    content?.querySelectorAll<HTMLElement>('[data-set-theme]').forEach((btn) => {
      btn.addEventListener('click', () => {
        setTheme(btn.dataset['setTheme'] as 'light' | 'dark')
        _renderSection('apariencia')
      })
    })
  }
  if (section === 'desarrollador') {
    content?.querySelectorAll<HTMLElement>('[data-set-devmode]').forEach((btn) => {
      btn.addEventListener('click', () => {
        setDevMode(btn.dataset['setDevmode'] === 'on')
        _renderSection('desarrollador')
      })
    })
  }
  if (section === 'general') {
    document.getElementById('settings-name-input')?.addEventListener('change', (e) => {
      const value = (e.target as HTMLInputElement).value.trim() || 'Brashkie'
      const nameEl = document.getElementById('nav-user-name')
      const avatarEl = document.getElementById('nav-user-avatar')
      if (nameEl) nameEl.textContent = value
      if (avatarEl) avatarEl.textContent = value.charAt(0).toUpperCase()
    })
    document
      .getElementById('settings-language-btn')
      ?.addEventListener('click', () => toast('Selector de idioma: todavía no implementado', 'info'))
    document
      .getElementById('settings-help-btn')
      ?.addEventListener('click', () => toast('Centro de ayuda: todavía no implementado', 'info'))
  }
}

export function initUserMenu(): void {
  const btn = document.getElementById('nav-user-btn')
  const menu = document.getElementById('nav-user-menu')
  const overlay = document.getElementById('settings-modal-overlay')

  btn?.addEventListener('click', (e) => {
    e.stopPropagation()
    _toggleMenu()
    if (_menuOpen) menu?.querySelector<HTMLElement>('[role="menuitem"]')?.focus()
  })

  // Cerrar el dropdown al clickear afuera — mismo criterio que cualquier
  // popup de esta app (ver toggleSearch/explorer.ts).
  document.addEventListener('click', (e) => {
    if (_menuOpen && menu && !menu.contains(e.target as Node) && e.target !== btn) _closeMenu()
  })
  document.addEventListener('keydown', _navigateMenu)
  document.addEventListener('keydown', _trapModalTab)

  menu?.querySelectorAll<HTMLElement>('[data-user-action]').forEach((el) => {
    el.addEventListener('click', () => {
      _closeMenu()
      const action = el.dataset['userAction']
      if (action === 'settings') _openModal()
      else if (action === 'help') toast('Centro de ayuda: todavía no implementado', 'info')
      else if (action === 'logout') toast('Cerrar sesión: Sythrall todavía no tiene cuentas reales', 'info')
    })
  })

  document.getElementById('settings-modal-close')?.addEventListener('click', _closeModal)
  overlay?.addEventListener('click', (e) => {
    if (e.target === overlay) _closeModal()
  })
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      if (_modalOpen) _closeModal()
      else if (_menuOpen) _closeMenu()
    }
  })

  document.querySelectorAll<HTMLElement>('.settings-modal-nav-item').forEach((el) => {
    el.addEventListener('click', () => {
      _activeSection = el.dataset['settingsSection'] as SettingsSection
      _renderSection(_activeSection)
    })
  })
}
