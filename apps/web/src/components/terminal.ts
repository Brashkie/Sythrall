// ══════════════════════════════════════════
//  Sythrall — Terminal integrada
//  Cliente WebSocket del sidecar Rust (terminal-server): PTY real vía
//  portable-pty, protegido con un token que el usuario pega una sola vez.
// ══════════════════════════════════════════
// components/terminal.ts

import { FitAddon } from '@xterm/addon-fit'
import { Terminal } from '@xterm/xterm'
import '@xterm/xterm/css/xterm.css'
import type { LogEntry } from '../utils/helpers'
import { LOG_ENTRIES, logRowHtml, subscribeToLogs, toast } from '../utils/helpers'
import { createResizer } from '../utils/resizer'

const TOKEN_KEY = 'cw_terminal_token'
type TerminalView = 'terminal' | 'logs'

let term: Terminal | null = null
let fitAddon: FitAddon | null = null
let socket: WebSocket | null = null
let resizerWired = false
let viewTabsWired = false
let logsUnsubscribe: (() => void) | null = null

function getToken(): string | null {
  return sessionStorage.getItem(TOKEN_KEY)
}

function setToken(token: string): void {
  sessionStorage.setItem(TOKEN_KEY, token)
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
        <div style="font-weight:700;margin-bottom:6px">🔑 Token de la terminal</div>
        <div style="font-size:.75rem;color:var(--muted);margin-bottom:12px;line-height:1.5">
          Mirá la consola donde corriste <code>npm run dev</code> (línea con prefijo "term") y pegá el token que imprimió al arrancar.
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

function ensureTerminalInstance(): void {
  if (term) return
  term = new Terminal({
    fontFamily: 'DM Mono, monospace',
    fontSize: 13,
    cursorBlink: true,
    theme: { background: '#0a0d1a', foreground: '#c8d4f0' },
  })
  fitAddon = new FitAddon()
  term.loadAddon(fitAddon)

  const body = document.getElementById('terminal-body')
  if (body) {
    term.open(body)
    fitAddon.fit()
  }

  term.onData((data) => {
    if (socket?.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify({ type: 'input', data }))
    }
  })
  term.onResize(({ cols, rows }) => {
    if (socket?.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify({ type: 'resize', cols, rows }))
    }
  })
}

function connect(token: string): void {
  const proto = location.protocol === 'https:' ? 'wss' : 'ws'
  const ws = new WebSocket(`${proto}://${location.host}/terminal/ws?token=${encodeURIComponent(token)}`)
  socket = ws

  ws.onopen = () => {
    setToken(token)
    fitAddon?.fit()
  }
  ws.onmessage = (ev) => {
    try {
      const msg = JSON.parse(ev.data)
      if (msg.type === 'output') term?.write(msg.data)
    } catch {
      /* frame no-JSON, ignorar */
    }
  }
  ws.onclose = (ev) => {
    // 4401 = nuestro código de "token inválido" (server responde 401 antes del upgrade,
    // pero algunos navegadores exponen el cierre igual con code 1006 genérico).
    if (ev.code === 1006 || ev.code === 4401) clearToken()
    term?.writeln('\r\n\x1b[90m[conexión cerrada]\x1b[0m')
    socket = null
  }
  ws.onerror = () => {
    toast('No se pudo conectar a la terminal — ¿está corriendo el sidecar Rust (npm run dev)?', 'err')
  }
}

async function fetchLocalToken(): Promise<string | null> {
  try {
    const res = await fetch('/terminal/token')
    if (!res.ok) return null // 403 = pedido no-local, el sidecar lo rechaza a propósito
    const data = await res.json()
    return typeof data.token === 'string' ? data.token : null
  } catch {
    return null // sidecar no está corriendo todavía, etc.
  }
}

async function connectWithToken(): Promise<void> {
  let token = getToken()
  if (!token) {
    // Uso local normal: el sidecar sirve el token solo a pedidos que vienen
    // de esta misma máquina — sin eso, no hace falta pegarlo a mano.
    token = await fetchLocalToken()
  }
  if (!token) {
    const entered = await promptForToken()
    if (!entered) return
    token = entered
  }
  connect(token)
}

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
  const tabTerm = document.getElementById('tp-view-terminal')
  const tabLogs = document.getElementById('tp-view-logs')

  if (termBody) termBody.style.display = view === 'terminal' ? 'block' : 'none'
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
    requestAnimationFrame(() => fitAddon?.fit())
  }
}

export function openTerminalPanel(): void {
  const panel = document.getElementById('terminal-panel')
  if (!panel) return
  panel.style.display = 'flex'
  ensureTerminalInstance()

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
        onChange: () => fitAddon?.fit(),
      })
    }
  }

  if (!viewTabsWired) {
    viewTabsWired = true
    document.getElementById('tp-view-terminal')?.addEventListener('click', () => showTerminalView('terminal'))
    document.getElementById('tp-view-logs')?.addEventListener('click', () => showTerminalView('logs'))
  }

  requestAnimationFrame(() => fitAddon?.fit())
  if (!socket) void connectWithToken()
}

export function closeTerminalPanel(): void {
  const panel = document.getElementById('terminal-panel')
  if (panel) panel.style.display = 'none'
  socket?.close()
  socket = null
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
  socket?.close()
  socket = null
  term?.reset()
  void connectWithToken()
}
