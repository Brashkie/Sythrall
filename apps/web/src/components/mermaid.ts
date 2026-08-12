// ══════════════════════════════════════════
//  Sythrall — Mermaid Diagrams + Zoom/Pan
// ══════════════════════════════════════════
// src/components/mermaid.ts
import mermaid from 'mermaid'

let initialized = false

// ── Zoom state
interface ZoomState {
  scale: number
  offsetX: number
  offsetY: number
}

const zoom: ZoomState = { scale: 1, offsetX: 0, offsetY: 0 }
const ZOOM_MIN = 0.15
const ZOOM_MAX = 5
const ZOOM_STEP = 0.15

let viewport: HTMLElement | null = null
let canvas: HTMLElement | null = null
let zoomLevelEl: HTMLElement | null = null
let toastEl: HTMLElement | null = null
let toastTimer: ReturnType<typeof setTimeout> | null = null

// ── Pan state
let isPanning = false
let panStartX = 0
let panStartY = 0
let panOriginX = 0
let panOriginY = 0

// ════════════════════════════════════════════════
//  MERMAID INIT
// ════════════════════════════════════════════════

export function initMermaid(): void {
  mermaid.initialize({
    startOnLoad: false,
    theme: 'dark',
    darkMode: true,
    themeVariables: {
      primaryColor: '#1a2040',
      primaryTextColor: '#c8d4f0',
      primaryBorderColor: '#2d3768',
      lineColor: '#4a5880',
      secondaryColor: '#0e1225',
      background: '#060810',
      mainBkg: '#0e1225',
      fontSize: '13px',
    },
    flowchart: { curve: 'basis', padding: 18 },
    sequence: { actorMargin: 70 },
  })
  initialized = true
}

export async function renderDiagram(code: string): Promise<string> {
  if (!initialized) initMermaid()
  const id = 'mermaid-' + Date.now()
  const result = await mermaid.render(id, code)
  return result.svg
}

// ════════════════════════════════════════════════
//  ZOOM / PAN — PUBLIC API
// ════════════════════════════════════════════════

export function initDiagramZoom(): void {
  viewport = document.getElementById('diag-viewport')
  canvas = document.getElementById('diag-canvas')
  zoomLevelEl = document.getElementById('zoom-level')

  if (!viewport || !canvas) return

  // Crear toast de zoom
  toastEl = document.createElement('div')
  toastEl.className = 'zoom-toast'
  viewport.appendChild(toastEl)

  // Wheel → zoom centrado en cursor
  viewport.addEventListener('wheel', onWheel, { passive: false })

  // Mouse drag → pan
  viewport.addEventListener('mousedown', onMouseDown)
  window.addEventListener('mousemove', onMouseMove)
  window.addEventListener('mouseup', onMouseUp)

  // Touch → pinch zoom + pan
  viewport.addEventListener('touchstart', onTouchStart, { passive: false })
  viewport.addEventListener('touchmove', onTouchMove, { passive: false })
  viewport.addEventListener('touchend', onTouchEnd)

  // Keyboard shortcuts
  document.addEventListener('keydown', onKeyDown)

  resetZoom(false)
}

export function zoomIn(): void {
  applyZoom(zoom.scale + ZOOM_STEP, getCenterPoint())
}

export function zoomOut(): void {
  applyZoom(zoom.scale - ZOOM_STEP, getCenterPoint())
}

export function resetZoom(animate = true): void {
  setZoom(1, 0, 0, animate)
}

export function fitDiagram(animate = true): void {
  if (!viewport || !canvas) return
  const svg = canvas.querySelector('svg')
  if (!svg) {
    resetZoom(animate)
    return
  }

  const vw = viewport.clientWidth
  const vh = viewport.clientHeight
  const sw = svg.scrollWidth || parseInt(svg.getAttribute('width') ?? '800', 10)
  const sh = svg.scrollHeight || parseInt(svg.getAttribute('height') ?? '600', 10)

  const scaleX = (vw - 48) / sw
  const scaleY = (vh - 48) / sh
  const fit = Math.min(scaleX, scaleY, 1) // nunca agrandar más de 1:1

  // Centrar
  const ox = (vw - sw * fit) / 2
  const oy = (vh - sh * fit) / 2

  setZoom(fit, ox, oy, animate)
}

// ════════════════════════════════════════════════
//  INTERNAL
// ════════════════════════════════════════════════

function applyZoom(newScale: number, pivot: { x: number; y: number }): void {
  const s = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, newScale))
  const ds = s / zoom.scale

  // Mantener el punto bajo el cursor fijo
  const ox = pivot.x - ds * (pivot.x - zoom.offsetX)
  const oy = pivot.y - ds * (pivot.y - zoom.offsetY)

  setZoom(s, ox, oy, false)
}

function setZoom(scale: number, ox: number, oy: number, animate: boolean): void {
  zoom.scale = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, scale))
  zoom.offsetX = ox
  zoom.offsetY = oy

  if (!canvas) return

  if (animate) {
    canvas.classList.add('zoom-animating')
    setTimeout(() => canvas!.classList.remove('zoom-animating'), 240)
  }

  canvas.style.transform = `translate(${zoom.offsetX}px, ${zoom.offsetY}px) scale(${zoom.scale})`

  // Actualizar indicador
  const pct = Math.round(zoom.scale * 100) + '%'
  if (zoomLevelEl) zoomLevelEl.textContent = pct
  showZoomToast(pct)
}

function showZoomToast(label: string): void {
  if (!toastEl) return
  toastEl.textContent = label
  toastEl.classList.add('visible')
  if (toastTimer) clearTimeout(toastTimer)
  toastTimer = setTimeout(() => toastEl!.classList.remove('visible'), 900)
}

function getCenterPoint(): { x: number; y: number } {
  if (!viewport) return { x: 0, y: 0 }
  return { x: viewport.clientWidth / 2, y: viewport.clientHeight / 2 }
}

// ── Wheel handler
function onWheel(e: WheelEvent): void {
  if (!viewport) return
  e.preventDefault()

  // Ctrl/Meta + wheel = zoom; solo wheel = scroll normal → aquí siempre zoom
  const rect = viewport.getBoundingClientRect()
  const pivot = {
    x: e.clientX - rect.left,
    y: e.clientY - rect.top,
  }

  const delta = e.deltaY < 0 ? ZOOM_STEP : -ZOOM_STEP
  applyZoom(zoom.scale + delta, pivot)
}

// ── Mouse pan
function onMouseDown(e: MouseEvent): void {
  if (e.button !== 0) return
  isPanning = true
  panStartX = e.clientX
  panStartY = e.clientY
  panOriginX = zoom.offsetX
  panOriginY = zoom.offsetY
}

function onMouseMove(e: MouseEvent): void {
  if (!isPanning) return
  const dx = e.clientX - panStartX
  const dy = e.clientY - panStartY
  zoom.offsetX = panOriginX + dx
  zoom.offsetY = panOriginY + dy
  if (canvas) canvas.style.transform = `translate(${zoom.offsetX}px, ${zoom.offsetY}px) scale(${zoom.scale})`
}

function onMouseUp(): void {
  isPanning = false
}

// ── Touch (pinch + pan)
let lastTouchDist = 0
let lastTouchMidX = 0
let lastTouchMidY = 0

function onTouchStart(e: TouchEvent): void {
  if (e.touches.length === 2) {
    e.preventDefault()
    lastTouchDist = getTouchDist(e)
    const mid = getTouchMid(e)
    lastTouchMidX = mid.x
    lastTouchMidY = mid.y
  } else if (e.touches.length === 1) {
    isPanning = true
    panStartX = e.touches[0].clientX
    panStartY = e.touches[0].clientY
    panOriginX = zoom.offsetX
    panOriginY = zoom.offsetY
  }
}

function onTouchMove(e: TouchEvent): void {
  e.preventDefault()
  if (e.touches.length === 2) {
    const dist = getTouchDist(e)
    const mid = getTouchMid(e)
    const rect = viewport!.getBoundingClientRect()
    const pivot = { x: mid.x - rect.left, y: mid.y - rect.top }

    if (lastTouchDist > 0) {
      applyZoom(zoom.scale * (dist / lastTouchDist), pivot)
    }

    // Pan simultáneo al pinch
    zoom.offsetX += mid.x - lastTouchMidX
    zoom.offsetY += mid.y - lastTouchMidY
    if (canvas) canvas.style.transform = `translate(${zoom.offsetX}px, ${zoom.offsetY}px) scale(${zoom.scale})`

    lastTouchDist = dist
    lastTouchMidX = mid.x
    lastTouchMidY = mid.y
  } else if (e.touches.length === 1 && isPanning) {
    const dx = e.touches[0].clientX - panStartX
    const dy = e.touches[0].clientY - panStartY
    zoom.offsetX = panOriginX + dx
    zoom.offsetY = panOriginY + dy
    if (canvas) canvas.style.transform = `translate(${zoom.offsetX}px, ${zoom.offsetY}px) scale(${zoom.scale})`
  }
}

function onTouchEnd(e: TouchEvent): void {
  if (e.touches.length < 2) lastTouchDist = 0
  if (e.touches.length === 0) isPanning = false
}

function getTouchDist(e: TouchEvent): number {
  const dx = e.touches[0].clientX - e.touches[1].clientX
  const dy = e.touches[0].clientY - e.touches[1].clientY
  return Math.sqrt(dx * dx + dy * dy)
}

function getTouchMid(e: TouchEvent): { x: number; y: number } {
  return {
    x: (e.touches[0].clientX + e.touches[1].clientX) / 2,
    y: (e.touches[0].clientY + e.touches[1].clientY) / 2,
  }
}

// ── Keyboard shortcuts (solo cuando el panel diagrama está activo)
function onKeyDown(e: KeyboardEvent): void {
  const panel = document.getElementById('panel-diagram')
  if (!panel?.classList.contains('active')) return
  if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return

  if (e.key === '+' || e.key === '=') {
    e.preventDefault()
    zoomIn()
  }
  if (e.key === '-') {
    e.preventDefault()
    zoomOut()
  }
  if (e.key === '0') {
    e.preventDefault()
    resetZoom(true)
  }
  if (e.key === 'f' || e.key === 'F') {
    e.preventDefault()
    fitDiagram(true)
  }
}

// ── Reinicia posición al renderizar nuevo diagrama
export function resetDiagramView(animate = false): void {
  // Pequeño delay para que el SVG tenga dimensiones reales
  setTimeout(() => fitDiagram(animate), 80)
}
