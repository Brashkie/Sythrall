// ══════════════════════════════════════════
//  CodeWatch PRO — Utilities
// ══════════════════════════════════════════

export function debounce<T extends (...args: unknown[]) => void>(fn: T, ms: number): (...args: Parameters<T>) => void {
  let timer: ReturnType<typeof setTimeout>
  return (...args: Parameters<T>) => {
    clearTimeout(timer)
    timer = setTimeout(() => fn(...args), ms)
  }
}

export function delay(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms))
}

export function nowStr(): string {
  return new Date().toLocaleTimeString('es-PE')
}

export function nowTs(): string {
  const d = new Date()
  return [d.getHours(), d.getMinutes(), d.getSeconds()].map((n) => String(n).padStart(2, '0')).join(':')
}

export function fmtBytes(b: number): string {
  if (b < 1024) return b + 'B'
  if (b < 1048576) return (b / 1024).toFixed(1) + 'KB'
  return (b / 1048576).toFixed(1) + 'MB'
}

export function esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

export function getExt(name: string): string {
  return '.' + name.split('.').pop()!.toLowerCase()
}

export function uniqueId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2)
}

export function clamp(val: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, val))
}

// Toast notifications
let _toastContainer: HTMLElement | null = null

function getToastContainer(): HTMLElement {
  if (!_toastContainer) {
    _toastContainer = document.createElement('div')
    _toastContainer.id = 'toast-container'
    _toastContainer.style.cssText = `
      position:fixed;bottom:20px;right:20px;z-index:9999;
      display:flex;flex-direction:column;gap:8px;pointer-events:none;
    `
    document.body.appendChild(_toastContainer)
  }
  return _toastContainer
}

export type ToastType = 'ok' | 'err' | 'warn' | 'info'

export function toast(msg: string, type: ToastType = 'info'): void {
  const colors: Record<ToastType, string> = {
    ok: '#00f5a0',
    err: '#ff3366',
    warn: '#ffb627',
    info: '#3d9eff',
  }
  const el = document.createElement('div')
  el.style.cssText = `
    background:#141830;border:1px solid ${colors[type]};border-radius:9px;
    padding:10px 16px;font-size:13px;color:${colors[type]};
    font-family:'DM Mono',monospace;
    animation:toast-in .25s ease;pointer-events:auto;
    max-width:320px;box-shadow:0 6px 24px rgba(0,0,0,.5);
  `
  el.textContent = msg
  getToastContainer().appendChild(el)
  setTimeout(() => {
    el.style.animation = 'toast-out .25s ease forwards'
    setTimeout(() => el.remove(), 300)
  }, 2800)
}

// Progress bar
export function setProgress(pct: number): void {
  const el = document.getElementById('pbar')
  if (el) el.style.width = pct + '%'
}

// Log stream
export type LogLevel = 'ok' | 'err' | 'warn' | 'info'

export interface LogEntry {
  ts: string
  type: LogLevel
  msg: string
  src: 'fe' | 'be'
}

const LOG_COLORS: Record<LogLevel, string> = { ok: '#00f5a0', err: '#ff3366', warn: '#ffb627', info: '#c8d4f0' }
const LOG_ICONS: Record<LogLevel, string> = { ok: '✓', err: '✕', warn: '⚠', info: '·' }

// Buffer compartido (capado) + suscriptores — permite que más de una vista
// (el tab "Logs" de siempre y el selector Terminal/Logs del panel inferior)
// muestren el mismo stream sin acoplarse al id de un único elemento del DOM.
const LOG_CAP = 300
export const LOG_ENTRIES: LogEntry[] = []
const logSubscribers = new Set<(entry: LogEntry) => void>()

export function subscribeToLogs(cb: (entry: LogEntry) => void): () => void {
  logSubscribers.add(cb)
  return () => logSubscribers.delete(cb)
}

export function logRowHtml(entry: LogEntry): string {
  return `
    <span class="ls-ts">${entry.ts}</span>
    <span class="ls-src src-${entry.src}">${entry.src}</span>
    <span style="width:12px;flex-shrink:0;font-size:10px;color:${LOG_COLORS[entry.type]}">${LOG_ICONS[entry.type]}</span>
    <span style="color:${LOG_COLORS[entry.type]}">${esc(entry.msg)}</span>
  `
}

export function appendLog(type: LogLevel, msg: string, src: 'fe' | 'be' = 'fe'): void {
  const entry: LogEntry = { ts: nowTs(), type, msg, src }
  LOG_ENTRIES.push(entry)
  if (LOG_ENTRIES.length > LOG_CAP) LOG_ENTRIES.shift()

  const el = document.getElementById('log-stream')
  if (el) {
    const row = document.createElement('div')
    row.className = 'ls-row'
    row.innerHTML = logRowHtml(entry)
    el.appendChild(row)
    const autoScroll = document.getElementById('autoscroll') as HTMLInputElement | null
    if (autoScroll?.checked) el.scrollTop = el.scrollHeight
  }

  for (const cb of logSubscribers) cb(entry)
}
