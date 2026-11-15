// ══════════════════════════════════════════
//  Sythrall — Utilities
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

function nowTs(): string {
  const d = new Date()
  return [d.getHours(), d.getMinutes(), d.getSeconds()].map((n) => String(n).padStart(2, '0')).join(':')
}

/** "recién" / "hace 12 min" / "hace 3 h" / "hace 2 d" a partir de un epoch
 * en ms. Devuelve cadena vacía con `null`, para que quien lo use pueda
 * concatenar sin chequear: un análisis restaurado de una sesión anterior no
 * tiene marca de tiempo, y ahí es más honesto no decir nada que inventar un
 * "hace 0 min". */
export function haceCuanto(ts: number | null): string {
  if (!ts) return ''
  const segundos = Math.max(0, Math.round((Date.now() - ts) / 1000))
  if (segundos < 60) return 'recién'
  const minutos = Math.round(segundos / 60)
  if (minutos < 60) return `hace ${minutos} min`
  const horas = Math.round(minutos / 60)
  if (horas < 24) return `hace ${horas} h`
  return `hace ${Math.round(horas / 24)} d`
}

export function fmtBytes(b: number): string {
  if (b < 1024) return b + 'B'
  if (b < 1048576) return (b / 1024).toFixed(1) + 'KB'
  return (b / 1048576).toFixed(1) + 'MB'
}

/**
 * Escapa texto para insertarlo en HTML. **Incluye las comillas a propósito**:
 * durante años esta función solo cubría `&`, `<` y `>`, lo que la dejaba
 * segura para contenido de texto pero NO para atributos — y este código la
 * usa en atributos por todos lados (`title="${esc(x)}"`,
 * `data-file="${esc(x)}"`, `value="${esc(x)}"`). Con un valor controlado por
 * el usuario (un nombre de archivo subido, un nombre de proyecto), una
 * comilla cerraba el atributo y permitía agregar otro:
 * `mi" onmouseover="alert(1)` se convertía en un handler real.
 *
 * En contenido de texto, `&quot;`/`&#39;` se renderizan como `"`/`'`, así que
 * cubrir los 5 caracteres no cambia nada visible en los usos existentes.
 */
/** **El único escapador de HTML del frontend.**
 *
 * Habia cuatro, con comportamientos distintos: este, uno en `panels/static.ts`
 * y otro en `panels/upload.ts` que NO escapaban la comilla simple, y un
 * `escapeHtml` en `components/terminal.ts` que escapaba via DOM y por lo tanto
 * **tampoco escapaba las comillas dobles** — aunque su salida iba dentro de un
 * `title="..."` con un nombre de sesion que el usuario puede renombrar. Eso no
 * era un duplicado cosmetico: era un agujero de inyeccion de atributo.
 *
 * Acepta numero y nulo porque los sitios de llamada los pasan: interpolar
 * `${esc(f.line)}` es lo normal en estas plantillas, y obligar a convertir a
 * string en cada uno era el motivo por el que `static.ts` tenia su propia
 * copia mas permisiva. */
export function esc(s: string | number | null | undefined): string {
  if (s == null) return ''
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}

export function getExt(name: string): string {
  return '.' + name.split('.').pop()!.toLowerCase()
}

/**
 * `<option>`s para cualquier selector de archivo cargado. Existía duplicado
 * en `components/app.ts` (5 selectores) y `panels/static.ts` (el suyo), los
 * dos armando el mismo HTML a mano y los dos **sin escapar el nombre del
 * archivo** — que lo elige el usuario al subirlo, así que era una inyección
 * real, no teórica. Una sola función para que no vuelva a divergir.
 *
 * Tipo estructural mínimo (`{id, name}`) en vez de importar `LoadedFile`:
 * este módulo no depende de ningún otro del proyecto y conviene que siga así.
 */
export function fileOptionsHtml(files: Array<{ id: string; name: string }>, placeholder?: string): string {
  const head = placeholder ? `<option value="">${esc(placeholder)}</option>` : ''
  return head + files.map((f) => `<option value="${esc(f.id)}">${esc(f.name)}</option>`).join('')
}

export function uniqueId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2)
}

// Toast notifications
let _toastContainer: HTMLElement | null = null

function getToastContainer(): HTMLElement {
  // `isConnected` y no solo `!_toastContainer`: si algo limpia el <body>
  // (o el contenedor se remueve), la referencia cacheada seguiría viva pero
  // huérfana y los toasts siguientes no se verían en ninguna parte.
  if (!_toastContainer?.isConnected) {
    _toastContainer = document.createElement('div')
    _toastContainer.id = 'toast-container'
    document.body.appendChild(_toastContainer)
  }
  return _toastContainer
}

export type ToastType = 'ok' | 'err' | 'warn' | 'info'

/**
 * Los estilos viven en `main.css` (`.toast`), no acá: la versión anterior
 * armaba un `cssText` inline con los colores del tema oscuro escritos en hex
 * (`#141830` de fondo, `#00f5a0` de texto), así que en tema claro aparecía
 * una caja oscura con verde neón — ilegible. Con clases + tokens, el tema
 * claro lo pisa solo.
 *
 * `role="status"` + `aria-live="polite"`: un lector de pantalla anuncia el
 * mensaje cuando aparece. Antes era un `<div>` mudo, así que quien no ve la
 * pantalla no se enteraba de ningún resultado de acción.
 */
export function toast(msg: string, type: ToastType = 'info'): void {
  const el = document.createElement('div')
  el.className = `toast toast-${type}`
  el.setAttribute('role', 'status')
  el.setAttribute('aria-live', 'polite')
  el.textContent = msg
  getToastContainer().appendChild(el)
  setTimeout(() => {
    el.classList.add('toast-leaving')
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

// Tokens, no hex: los valores anteriores (`#00f5a0`, `#ff3366`, `#c8d4f0`)
// eran los del tema OSCURO escritos a mano, así que en tema claro el stream
// de logs quedaba en verde/rosa neón sobre blanco — sin contraste real. El
// tema claro ya define su propia versión de cada uno de estos tokens.
const LOG_COLORS: Record<LogLevel, string> = {
  ok: 'var(--ok)',
  err: 'var(--err)',
  warn: 'var(--warn)',
  info: 'var(--txt)',
}
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
    // El buffer en memoria estaba capado desde siempre (`LOG_ENTRIES.shift()`
    // arriba), pero el DOM no: las filas se agregaban y no se sacaba ninguna,
    // así que en una sesión larga (la terminal, el polling del backend y cada
    // análisis escriben acá) `#log-stream` acumulaba miles de nodos —
    // memoria que solo sube, y un `scrollTop = scrollHeight` cada vez más
    // caro porque fuerza layout sobre todos ellos. Se recorta al mismo cap
    // que el buffer, así los dos dicen lo mismo.
    while (el.childElementCount > LOG_CAP) el.removeChild(el.firstElementChild!)
    const autoScroll = document.getElementById('autoscroll') as HTMLInputElement | null
    if (autoScroll?.checked) el.scrollTop = el.scrollHeight
  }

  for (const cb of logSubscribers) cb(entry)
}
