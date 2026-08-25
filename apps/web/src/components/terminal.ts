// ══════════════════════════════════════════
//  Sythrall — Terminal integrada
//  Cliente WebSocket del sidecar Rust (terminal-server): PTY real vía
//  portable-pty, protegido con un token de sesión (JWT) de vida corta que
//  apps/api emite (GET /auth/terminal-token, solo a pedidos locales) — ver
//  services/terminal/src/auth.rs. Sin login real todavía: el token es para
//  un usuario implícito "local", pero la cañería ya está lista para cuando
//  lo haya.
//  Multi-sesión: cada pestaña es su propia conexión WebSocket → su propio
//  PtySession del lado del servidor (ya soportado ahí, una sesión por
//  socket) — el frontend solo necesita abrir N sockets en vez de reusar uno.
// ══════════════════════════════════════════
// components/terminal.ts

import { FitAddon } from '@xterm/addon-fit'
import { SearchAddon } from '@xterm/addon-search'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { Terminal } from '@xterm/xterm'
import '@xterm/xterm/css/xterm.css'
import { getApiBase } from '../store/state'
import type { LogEntry } from '../utils/helpers'
import { LOG_ENTRIES, logRowHtml, subscribeToLogs, toast } from '../utils/helpers'
import { icon } from '../utils/icons'
import { createResizer } from '../utils/resizer'
import { terminalWelcomeBanner } from '../utils/terminalBanner'

const TOKEN_KEY = 'cw_terminal_token'
type TerminalView = 'terminal' | 'logs'

const TERM_THEME = {
  background: '#0a0d1a',
  foreground: '#c8d4f0',
  cursor: '#3d9eff',
  cursorAccent: '#0a0d1a',
  selectionBackground: 'rgba(61,158,255,.35)',
  black: '#0a0d1a',
  red: '#ff3366',
  green: '#00f5a0',
  yellow: '#ffb627',
  blue: '#3d9eff',
  magenta: '#b87dff',
  cyan: '#4ad9e0',
  white: '#c8d4f0',
  brightBlack: '#4a5880',
  brightRed: '#ff5c85',
  brightGreen: '#4dffc2',
  brightYellow: '#ffcb66',
  brightBlue: '#6cb4ff',
  brightMagenta: '#cba3ff',
  brightCyan: '#7de8ee',
  brightWhite: '#ffffff',
}

interface ShellOption {
  id: string
  label: string
}

interface TermSession {
  id: string
  name: string
  shellId?: string
  shellLabel: string
  term: Terminal
  fitAddon: FitAddon
  searchAddon: SearchAddon
  socket: WebSocket | null
  container: HTMLElement
  intentionalClose: boolean
  reconnectAttempts: number
  reconnectTimer: ReturnType<typeof setTimeout> | null
}

const sessions: TermSession[] = []
let activeId: string | null = null
let nextSessionNum = 1
let availableShells: ShellOption[] = []
let shellsFetched = false

let resizerWired = false
let viewTabsWired = false
let searchWired = false
let logsUnsubscribe: (() => void) | null = null

interface StoredToken {
  token: string
  expiresAt: number
}

// Margen antes del vencimiento real: evita usar un token que expira a mitad
// de la conexión (el JWT dura 1h — ver apps/api/routers/auth.py).
const TOKEN_EXPIRY_MARGIN_MS = 30_000

function getToken(): string | null {
  const raw = sessionStorage.getItem(TOKEN_KEY)
  if (!raw) return null
  try {
    const stored = JSON.parse(raw) as StoredToken
    if (Date.now() >= stored.expiresAt - TOKEN_EXPIRY_MARGIN_MS) {
      clearToken()
      return null
    }
    return stored.token
  } catch {
    // Token viejo guardado como string pelado (formato pre-JWT) — descartarlo.
    clearToken()
    return null
  }
}

function setToken(token: string, expiresInSeconds: number): void {
  const stored: StoredToken = { token, expiresAt: Date.now() + expiresInSeconds * 1000 }
  sessionStorage.setItem(TOKEN_KEY, JSON.stringify(stored))
}

function clearToken(): void {
  sessionStorage.removeItem(TOKEN_KEY)
}

function promptForToken(): Promise<string | null> {
  return new Promise((resolve) => {
    const overlay = document.createElement('div')
    overlay.style.cssText =
      'position:fixed;inset:0;background:rgba(6,8,16,.7);z-index:2000;display:flex;align-items:center;justify-content:center'
    overlay.innerHTML = `
      <div style="background:var(--s1);border:1px solid var(--b1);border-radius:9px;padding:20px;width:min(420px,90vw);box-shadow:0 12px 40px rgba(0,0,0,.5)">
        <div style="font-weight:700;margin-bottom:6px">Token de sesión de la terminal</div>
        <div style="font-size:.75rem;color:var(--muted);margin-bottom:12px;line-height:1.5">
          No se pudo obtener un token automáticamente (¿estás accediendo desde otra máquina?). Pegá un token de sesión (JWT) válido para conectar.
        </div>
        <input id="cw-term-token-input" type="text" placeholder="Token..." style="width:100%;padding:8px 10px;background:var(--s0);border:1px solid var(--b1);border-radius:6px;color:var(--txt);font-family:var(--mono);font-size:.75rem;margin-bottom:12px" />
        <div style="display:flex;gap:8px;justify-content:flex-end">
          <button class="btn btn-ghost btn-sm" id="cw-term-token-cancel">Cancelar</button>
          <button class="btn btn-run btn-sm" id="cw-term-token-ok">Conectar</button>
        </div>
      </div>`
    document.body.appendChild(overlay)

    const input = overlay.querySelector<HTMLInputElement>('#cw-term-token-input')
    input?.focus()

    const finish = (value: string | null) => {
      overlay.remove()
      resolve(value)
    }

    overlay.querySelector('#cw-term-token-cancel')?.addEventListener('click', () => finish(null))
    overlay.querySelector('#cw-term-token-ok')?.addEventListener('click', () => finish(input?.value.trim() || null))
    input?.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') finish(input.value.trim() || null)
      if (e.key === 'Escape') finish(null)
    })
  })
}

async function fetchLocalToken(): Promise<string | null> {
  try {
    const res = await fetch(getApiBase() + '/auth/terminal-token')
    if (!res.ok) return null // 403 = pedido no-local, apps/api lo rechaza a propósito
    const data = await res.json()
    return typeof data.token === 'string' ? data.token : null
  } catch {
    return null // el backend no está corriendo todavía, etc.
  }
}

// Lee el claim `exp` del JWT sin verificar la firma (solo para saber cuánto
// cachearlo client-side — la verificación real la hace services/terminal al
// aceptar la conexión). Si el token no es un JWT parseable (no debería pasar,
// pero `promptForToken` acepta texto libre), cachea por poco tiempo nomás.
function tokenTtlSeconds(token: string): number {
  try {
    const payload = token.split('.')[1]
    const json = atob(payload.replace(/-/g, '+').replace(/_/g, '/'))
    const claims = JSON.parse(json) as { exp?: number }
    if (typeof claims.exp === 'number') return Math.max(0, claims.exp - Date.now() / 1000)
  } catch {
    /* no era un JWT decodificable */
  }
  return 60
}

async function resolveToken(): Promise<string | null> {
  let token = getToken()
  if (!token) {
    // Uso local normal: el sidecar sirve el token solo a pedidos que vienen
    // de esta misma máquina — sin eso, no hace falta pegarlo a mano.
    token = await fetchLocalToken()
  }
  if (!token) {
    const entered = await promptForToken()
    if (!entered) return null
    token = entered
  }
  return token
}

async function fetchAvailableShells(): Promise<ShellOption[]> {
  if (shellsFetched) return availableShells
  shellsFetched = true
  try {
    const res = await fetch('/terminal/shells')
    if (!res.ok) return availableShells
    const data = await res.json()
    if (Array.isArray(data.shells)) availableShells = data.shells
  } catch {
    /* sidecar no disponible todavía — el picker simplemente no aparece */
  }
  return availableShells
}

function genId(): string {
  return `t${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`
}

// ── Ciclo de vida de una sesión ────────────────────────────────────────────

function createSession(shell?: ShellOption): TermSession {
  const id = genId()
  const container = document.createElement('div')
  container.className = 'term-session-body'
  container.style.display = 'none'
  document.getElementById('terminal-body')?.appendChild(container)

  const term = new Terminal({
    fontFamily: 'DM Mono, monospace',
    fontSize: 13,
    cursorBlink: true,
    theme: TERM_THEME,
  })
  const fitAddon = new FitAddon()
  const searchAddon = new SearchAddon()
  term.loadAddon(fitAddon)
  term.loadAddon(searchAddon)
  term.loadAddon(new WebLinksAddon())
  term.open(container)

  const session: TermSession = {
    id,
    name: shell?.label ? `${shell.label} ${nextSessionNum}` : `Terminal ${nextSessionNum}`,
    shellId: shell?.id,
    shellLabel: shell?.label ?? 'default',
    term,
    fitAddon,
    searchAddon,
    socket: null,
    container,
    intentionalClose: false,
    reconnectAttempts: 0,
    reconnectTimer: null,
  }
  nextSessionNum += 1

  term.write(terminalWelcomeBanner(session.shellLabel, term.cols || 80))

  term.onData((data) => {
    if (session.socket?.readyState === WebSocket.OPEN) {
      session.socket.send(JSON.stringify({ type: 'input', data }))
    }
  })
  term.onResize(({ cols, rows }) => {
    if (session.socket?.readyState === WebSocket.OPEN) {
      session.socket.send(JSON.stringify({ type: 'resize', cols, rows }))
    }
  })

  // Copy/paste estilo VSCode / Windows Terminal: Ctrl+C copia si hay
  // selección (y no manda ^C al PTY); sin selección, pasa de largo y sí
  // manda la señal de interrupción como siempre. Ctrl+F abre el buscador
  // en vez de dejar que el browser busque en la página.
  term.attachCustomKeyEventHandler((e) => {
    if (e.type !== 'keydown') return true
    const mod = e.ctrlKey || e.metaKey
    const key = e.key.toLowerCase()
    if (mod && !e.shiftKey && key === 'c' && term.hasSelection()) {
      void navigator.clipboard.writeText(term.getSelection())
      return false
    }
    if (mod && key === 'v') {
      void navigator.clipboard.readText().then((text) => {
        if (text && session.socket?.readyState === WebSocket.OPEN) {
          session.socket.send(JSON.stringify({ type: 'input', data: text }))
        }
      })
      return false
    }
    if (mod && key === 'f') {
      openSearchBar()
      return false
    }
    return true
  })

  sessions.push(session)
  return session
}

function connectSession(session: TermSession, token: string): void {
  const proto = location.protocol === 'https:' ? 'wss' : 'ws'
  const shellParam = session.shellId ? `&shell=${encodeURIComponent(session.shellId)}` : ''
  const ws = new WebSocket(`${proto}://${location.host}/terminal/ws?token=${encodeURIComponent(token)}${shellParam}`)
  session.socket = ws

  ws.onopen = () => {
    setToken(token, tokenTtlSeconds(token))
    session.reconnectAttempts = 0
    if (session.id === activeId) session.fitAddon.fit()
  }
  ws.onmessage = (ev) => {
    try {
      const msg = JSON.parse(ev.data)
      if (msg.type === 'output') session.term.write(msg.data)
    } catch {
      /* frame no-JSON, ignorar */
    }
  }
  ws.onclose = (ev) => {
    session.socket = null
    if (session.intentionalClose) return
    // 4401 = nuestro código de "token inválido" (server responde 401 antes del upgrade,
    // pero algunos navegadores exponen el cierre igual con code 1006 genérico).
    if (ev.code === 1006 || ev.code === 4401) clearToken()
    session.term.writeln('\r\n\x1b[90m[conexión cerrada — reintentando...]\x1b[0m')
    scheduleReconnect(session)
  }
  ws.onerror = () => {
    toast('No se pudo conectar a la terminal — ¿está corriendo el sidecar Rust (npm run dev)?', 'err')
  }
}

function scheduleReconnect(session: TermSession): void {
  if (session.intentionalClose || session.reconnectAttempts >= 5) {
    if (session.reconnectAttempts >= 5) {
      session.term.writeln(
        '\x1b[91m[no se pudo reconectar — abrí una pestaña nueva cuando el sidecar esté arriba]\x1b[0m',
      )
    }
    return
  }
  session.reconnectAttempts += 1
  session.reconnectTimer = setTimeout(() => {
    void resolveToken().then((token) => {
      if (token && !session.intentionalClose) connectSession(session, token)
    })
  }, 1500)
}

function closeSession(id: string): void {
  const idx = sessions.findIndex((s) => s.id === id)
  if (idx === -1) return
  const [session] = sessions.splice(idx, 1)
  session.intentionalClose = true
  if (session.reconnectTimer) clearTimeout(session.reconnectTimer)
  session.socket?.close()
  session.term.dispose()
  session.container.remove()

  if (sessions.length === 0) {
    closeTerminalPanel()
    return
  }
  // La fila cerrada siempre tiene que desaparecer de la sidebar — a
  // diferencia de activar una sesión (ver setActiveSession), acá SÍ hace
  // falta un rebuild completo, haya sido la activa o no.
  renderSessionsSidebar()
  if (activeId === id) {
    const next = sessions[Math.max(0, idx - 1)]
    setActiveSession(next.id)
  }
}

/** Activa una sesión sin reconstruir la sidebar entera — solo alterna la
 * clase `.active` sobre las filas que ya existen en el DOM. Antes llamaba a
 * `renderSessionsSidebar()` (rebuild vía `innerHTML`), lo que rompía el
 * rename: un doble-click sobre el nombre de una fila QUE NO ERA la activa
 * dispara primero un solo click (activa esa fila → rebuild → el <span>
 * recién puesto en edición se destruye) antes de que el evento `dblclick`
 * llegue a `startRename`. Un rebuild acá solo hace falta cuando la lista de
 * sesiones cambia (crear/cerrar), no al simplemente cambiar cuál está activa
 * — esos dos casos ya llaman a `renderSessionsSidebar()` por su cuenta. */
function setActiveSession(id: string): void {
  const session = sessions.find((s) => s.id === id)
  if (!session) return
  activeId = id
  for (const s of sessions) s.container.style.display = s.id === id ? 'block' : 'none'
  const bar = document.getElementById('term-sessions-sidebar')
  for (const el of bar?.querySelectorAll<HTMLElement>('.term-session-row') ?? []) {
    el.classList.toggle('active', el.dataset.tabId === id)
  }
  requestAnimationFrame(() => {
    session.fitAddon.fit()
    // Un solo click sobre el nombre de una fila (la primera mitad de un
    // doble-click) ya llega hasta acá y agenda este foco para el frame
    // siguiente — si para entonces el doble-click ya arrancó un rename
    // (`startRename` ya puso el <span> en `contentEditable` y le dio foco),
    // robárselo de vuelta a la terminal termina el rename antes de que el
    // usuario llegue a escribir una letra. Cede el foco solo si no hay un
    // rename en curso.
    if (!document.activeElement?.closest('.term-session-name[contenteditable="true"]')) {
      session.term.focus()
    }
  })
}

async function addNewSession(shell?: ShellOption): Promise<void> {
  const session = createSession(shell)
  renderSessionsSidebar()
  setActiveSession(session.id)
  const token = await resolveToken()
  if (token) connectSession(session, token)
}

// ── Lista de sesiones (columna vertical, estilo VSCode) ──────────────────────

function renderSessionsSidebar(): void {
  const bar = document.getElementById('term-sessions-sidebar')
  if (!bar) return
  bar.innerHTML = sessions
    .map(
      (s) => `
    <div class="term-session-row ${s.id === activeId ? 'active' : ''}" data-tab-id="${s.id}" title="${escapeHtml(s.name)}">
      <span class="term-session-icon">${icon('terminal', 13)}</span>
      <span class="term-session-name" data-name-id="${s.id}">${escapeHtml(s.name)}</span>
      <span class="term-session-close" data-close-id="${s.id}" title="Cerrar">
        <svg class="icon" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18"/></svg>
      </span>
    </div>`,
    )
    .join('')

  for (const el of bar.querySelectorAll<HTMLElement>('.term-session-row')) {
    el.addEventListener('click', (e) => {
      if ((e.target as HTMLElement).closest('[data-close-id]')) return
      const id = el.dataset.tabId
      // Si ya es la sesión activa, no hay nada que activar — evita un
      // re-render (`setActiveSession` → `renderSessionsSidebar` → `innerHTML`
      // nuevo) que destruiría el <span> justo en medio de un doble-click
      // sobre su nombre, cancelando el rename antes de que arranque.
      if (id && id !== activeId) setActiveSession(id)
    })
  }
  for (const el of bar.querySelectorAll<HTMLElement>('[data-close-id]')) {
    el.addEventListener('click', (e) => {
      e.stopPropagation()
      const id = el.dataset.closeId
      if (id) closeSession(id)
    })
  }
  for (const el of bar.querySelectorAll<HTMLElement>('.term-session-name')) {
    el.addEventListener('dblclick', (e) => {
      e.stopPropagation()
      startRename(el)
    })
  }
}

function escapeHtml(s: string): string {
  const div = document.createElement('div')
  div.textContent = s
  return div.innerHTML
}

function startRename(nameEl: HTMLElement): void {
  const id = nameEl.dataset.nameId
  const session = sessions.find((s) => s.id === id)
  if (!session) return
  nameEl.contentEditable = 'true'
  nameEl.focus()
  const range = document.createRange()
  range.selectNodeContents(nameEl)
  const sel = window.getSelection()
  sel?.removeAllRanges()
  sel?.addRange(range)

  const commit = () => {
    nameEl.contentEditable = 'false'
    const value = nameEl.textContent?.trim()
    session.name = value || session.name
    renderSessionsSidebar()
    // Sin esto el foco queda en el body tras el blur del rename — el
    // atajo Ctrl+F (y cualquier tecla) no llega a ningún lado hasta que el
    // usuario clickea la terminal de nuevo.
    session.term.focus()
  }
  nameEl.addEventListener('blur', commit, { once: true })
  nameEl.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault()
      nameEl.blur()
    }
    if (e.key === 'Escape') {
      nameEl.textContent = session.name
      nameEl.blur()
    }
  })
}

async function showShellMenuOrCreate(anchor: HTMLElement): Promise<void> {
  const shells = await fetchAvailableShells()
  if (shells.length <= 1) {
    void addNewSession(shells[0])
    return
  }

  document.querySelector('.term-shell-menu')?.remove()
  const menu = document.createElement('div')
  menu.className = 'term-shell-menu'
  const rect = anchor.getBoundingClientRect()
  menu.style.top = `${rect.bottom + 4}px`
  menu.style.left = `${Math.max(4, rect.right - 150)}px`
  menu.innerHTML = shells
    .map((s, i) => `<button class="term-shell-menu-item" data-shell-idx="${i}">${escapeHtml(s.label)}</button>`)
    .join('')
  document.body.appendChild(menu)

  const close = () => menu.remove()
  for (const btn of menu.querySelectorAll<HTMLElement>('[data-shell-idx]')) {
    btn.addEventListener('click', () => {
      const idx = Number(btn.dataset.shellIdx)
      void addNewSession(shells[idx])
      close()
    })
  }
  setTimeout(() => document.addEventListener('click', close, { once: true }), 0)
}

// ── Buscador (Ctrl+F) ────────────────────────────────────────────────────────

function openSearchBar(): void {
  const bar = document.getElementById('term-search-bar')
  const input = document.getElementById('term-search-input') as HTMLInputElement | null
  if (!bar || !input) return
  bar.style.display = 'flex'
  input.focus()
  input.select()
  wireSearchBarOnce()
}

function closeSearchBar(): void {
  const bar = document.getElementById('term-search-bar')
  if (bar) bar.style.display = 'none'
  const session = sessions.find((s) => s.id === activeId)
  session?.term.focus()
}

function activeSearchAddon(): SearchAddon | null {
  return sessions.find((s) => s.id === activeId)?.searchAddon ?? null
}

function wireSearchBarOnce(): void {
  if (searchWired) return
  searchWired = true
  const input = document.getElementById('term-search-input') as HTMLInputElement | null
  input?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault()
      if (e.shiftKey) activeSearchAddon()?.findPrevious(input.value)
      else activeSearchAddon()?.findNext(input.value)
    }
    if (e.key === 'Escape') closeSearchBar()
  })
  document
    .getElementById('term-search-next')
    ?.addEventListener('click', () => activeSearchAddon()?.findNext(input?.value ?? ''))
  document
    .getElementById('term-search-prev')
    ?.addEventListener('click', () => activeSearchAddon()?.findPrevious(input?.value ?? ''))
  document.getElementById('term-search-close')?.addEventListener('click', closeSearchBar)
}

// ── Vista Terminal / Logs ────────────────────────────────────────────────────

function renderLogsBody(): void {
  const body = document.getElementById('terminal-logs-body')
  if (!body) return
  body.innerHTML = LOG_ENTRIES.map((e) => `<div class="ls-row">${logRowHtml(e)}</div>`).join('')
  body.scrollTop = body.scrollHeight
}

function appendLogRow(entry: LogEntry): void {
  const body = document.getElementById('terminal-logs-body')
  if (!body) return
  const row = document.createElement('div')
  row.className = 'ls-row'
  row.innerHTML = logRowHtml(entry)
  body.appendChild(row)
  body.scrollTop = body.scrollHeight
}

function showTerminalView(view: TerminalView): void {
  const termBody = document.getElementById('terminal-body')
  const logsBody = document.getElementById('terminal-logs-body')
  const tabsBar = document.getElementById('term-sessions-sidebar')
  const tabTerm = document.getElementById('tp-view-terminal')
  const tabLogs = document.getElementById('tp-view-logs')

  if (termBody) termBody.style.display = view === 'terminal' ? 'block' : 'none'
  if (tabsBar) tabsBar.style.display = view === 'terminal' ? 'flex' : 'none'
  if (logsBody) logsBody.style.display = view === 'logs' ? 'block' : 'none'
  tabTerm?.classList.toggle('active', view === 'terminal')
  tabLogs?.classList.toggle('active', view === 'logs')

  logsUnsubscribe?.()
  logsUnsubscribe = null

  if (view === 'logs') {
    renderLogsBody()
    logsUnsubscribe = subscribeToLogs(appendLogRow)
  } else {
    // La sesión de shell sigue viva en segundo plano — solo hace falta
    // volver a ajustar el tamaño de xterm.js, que no renderiza mientras
    // su contenedor está oculto (display:none).
    const session = sessions.find((s) => s.id === activeId)
    requestAnimationFrame(() => session?.fitAddon.fit())
  }
}

// ── Panel ─────────────────────────────────────────────────────────────────

function openTerminalPanel(): void {
  const panel = document.getElementById('terminal-panel')
  if (!panel) return
  panel.style.display = 'flex'

  if (!resizerWired) {
    resizerWired = true
    const handle = document.getElementById('terminal-resize')
    if (handle) {
      createResizer(panel, handle, {
        direction: 'vertical',
        // 'right' da el signo de delta correcto para un panel anclado abajo:
        // arrastrar el handle hacia arriba debe agrandarlo (ver resizer.ts).
        side: 'right',
        minSize: 120,
        maxSize: Math.round(window.innerHeight * 0.7),
        onChange: () => sessions.find((s) => s.id === activeId)?.fitAddon.fit(),
      })
    }
  }

  if (!viewTabsWired) {
    viewTabsWired = true
    document.getElementById('tp-view-terminal')?.addEventListener('click', () => showTerminalView('terminal'))
    document.getElementById('tp-view-logs')?.addEventListener('click', () => showTerminalView('logs'))
  }

  if (sessions.length === 0) {
    void addNewSession()
  } else {
    if (activeId) requestAnimationFrame(() => sessions.find((s) => s.id === activeId)?.fitAddon.fit())
    renderSessionsSidebar()
  }
}

export function closeTerminalPanel(): void {
  const panel = document.getElementById('terminal-panel')
  if (panel) panel.style.display = 'none'
  // Ocultar el panel NO mata las sesiones — igual que VSCode, un proceso
  // corriendo en la terminal (un build, un server) sigue vivo en segundo
  // plano aunque el panel esté oculto. Cerrar una sesión puntual es la ×
  // de su pestaña (closeSession), no esto.
  logsUnsubscribe?.()
  logsUnsubscribe = null
}

export function toggleTerminalPanel(): void {
  const panel = document.getElementById('terminal-panel')
  const isOpen = !!panel && panel.style.display !== 'none'
  if (isOpen) closeTerminalPanel()
  else openTerminalPanel()
}

export function newTerminalSession(): void {
  const btn = document.getElementById('terminal-new-btn')
  if (btn) void showShellMenuOrCreate(btn)
  else void addNewSession()
}
