// ══════════════════════════════════════════
//  Sythrall — Panel Resizer
//  Drag-to-resize + collapse con botones flecha
// ══════════════════════════════════════════
// utils/resizer.ts
export interface ResizerOptions {
  minSize?: number
  maxSize?: number
  direction?: 'horizontal' | 'vertical'
  /** 'left' = sidebar (handle on right edge, drag right = grow)
   *  'right' = right panel (handle on left edge, drag left = grow) */
  side?: 'left' | 'right'
  onChange?: (size: number) => void
}

export function createResizer(panelEl: HTMLElement, handleEl: HTMLElement, options: ResizerOptions = {}): () => void {
  const { minSize = 200, maxSize = 560, direction = 'horizontal', side = 'left', onChange } = options

  let dragging = false
  let startPos = 0
  let startSize = 0

  const getSize = () =>
    direction === 'horizontal' ? panelEl.getBoundingClientRect().width : panelEl.getBoundingClientRect().height

  const applySize = (size: number) => {
    const clamped = Math.max(minSize, Math.min(maxSize, size))
    if (direction === 'horizontal') {
      panelEl.style.width = `${clamped}px`
      panelEl.style.minWidth = `${clamped}px`
    } else {
      panelEl.style.height = `${clamped}px`
      panelEl.style.minHeight = `${clamped}px`
    }
    onChange?.(clamped)
    return clamped
  }

  const onMouseDown = (e: MouseEvent) => {
    e.preventDefault()
    dragging = true
    startPos = direction === 'horizontal' ? e.clientX : e.clientY
    startSize = getSize()
    document.body.style.cursor = direction === 'horizontal' ? 'col-resize' : 'row-resize'
    document.body.style.userSelect = 'none'
    handleEl.classList.add('resizing')
  }

  const onMouseMove = (e: MouseEvent) => {
    if (!dragging) return
    const current = direction === 'horizontal' ? e.clientX : e.clientY

    let delta: number
    if (side === 'right') {
      // Handle is on LEFT edge of right panel:
      // mouse moves LEFT  → current < startPos → delta > 0 → panel grows ✓
      // mouse moves RIGHT → current > startPos → delta < 0 → panel shrinks ✓
      delta = startPos - current
    } else {
      // Handle is on RIGHT edge of sidebar:
      // mouse moves RIGHT → current > startPos → delta > 0 → panel grows ✓
      // mouse moves LEFT  → current < startPos → delta < 0 → panel shrinks ✓
      delta = current - startPos
    }

    applySize(startSize + delta)
  }

  const onMouseUp = () => {
    if (!dragging) return
    dragging = false
    document.body.style.cursor = ''
    document.body.style.userSelect = ''
    handleEl.classList.remove('resizing')
    // Guardar tamaño en localStorage (ignorar si está colapsado)
    const currentSize = getSize()
    if (currentSize > minSize - 10) {
      localStorage.setItem(`panel-size-${panelEl.id}`, String(currentSize))
    }
  }

  handleEl.addEventListener('mousedown', onMouseDown)
  document.addEventListener('mousemove', onMouseMove)
  document.addEventListener('mouseup', onMouseUp)

  // Restaurar tamaño guardado (solo si es razonable)
  const saved = localStorage.getItem(`panel-size-${panelEl.id}`)
  if (saved) {
    const savedNum = Number(saved)
    if (savedNum >= minSize && savedNum <= maxSize) {
      applySize(savedNum)
    }
  }

  return () => {
    handleEl.removeEventListener('mousedown', onMouseDown)
    document.removeEventListener('mousemove', onMouseMove)
    document.removeEventListener('mouseup', onMouseUp)
  }
}

// ══════════════════════════════════════════
//  Collapse Toggle
// ══════════════════════════════════════════
export function createCollapseToggle(
  panelEl: HTMLElement,
  btnEl: HTMLElement,
  direction: 'left' | 'right' = 'left',
  expandSize = 300,
): void {
  let collapsed = false

  // left panel:  ‹ when open (click to collapse left), › when closed (click to expand right)
  // right panel: › when open (click to collapse right), ‹ when closed (click to expand left)
  const openIcon = direction === 'left' ? '‹' : '›'
  const closedIcon = direction === 'left' ? '›' : '‹'

  const setCollapsed = (val: boolean) => {
    collapsed = val
    if (collapsed) {
      panelEl.style.transition = 'width .22s ease, min-width .22s ease'
      panelEl.style.width = '0px'
      panelEl.style.minWidth = '0px'
      panelEl.classList.add('collapsed')
      btnEl.innerHTML = closedIcon
      btnEl.title = 'Expandir panel'
      btnEl.style.background = 'var(--info)'
      btnEl.style.color = '#fff'
      btnEl.style.borderColor = 'var(--info)'
      btnEl.style.opacity = '1'
    } else {
      const saved = localStorage.getItem(`panel-size-${panelEl.id}`)
      const size = saved && Number(saved) > 100 ? Number(saved) : expandSize
      panelEl.style.transition = 'width .22s ease, min-width .22s ease'
      panelEl.style.width = `${size}px`
      panelEl.style.minWidth = `${size}px`
      panelEl.classList.remove('collapsed')
      btnEl.innerHTML = openIcon
      btnEl.title = 'Colapsar panel'
      btnEl.style.background = ''
      btnEl.style.color = ''
      btnEl.style.borderColor = ''
      btnEl.style.opacity = ''
    }
  }

  btnEl.addEventListener('click', () => setCollapsed(!collapsed))

  // Estado inicial
  btnEl.innerHTML = openIcon
  btnEl.title = 'Colapsar panel'
}
