// ══════════════════════════════════════════
//  Sythrall — Project Explorer v1.0
//  Árbol expandible + File tabs + Búsqueda global + Outline
//
//  API pública (consumida por app.ts / events.ts / file-browser.ts):
//    initExplorer({ onFileOpen: (f) => ... })  — engancha la apertura de archivos
//    explorerAddFile(file) / explorerRemoveFile(id)
//    openSearch()
// ══════════════════════════════════════════

import { state } from '../store/state'
import type { CodeFile } from '../types'
import { icon, languageBadge } from '../utils/icons'

// ─── Tipos ────────────────────────────────────────────────────────────────────

interface ExplorerOptions {
  onFileOpen: (file: CodeFile) => void
}

interface Tab {
  id: string
  name: string
  modified: boolean
}

// ─── Estado interno ───────────────────────────────────────────────────────────

let _opts: ExplorerOptions | null = null
const _tabs: Tab[] = []
let _activeTabId: string = ''
let _searchOpen = false
let _searchQuery = ''

// ─── Iconos ───────────────────────────────────────────────────────────────────

function extIcon(ext: string): string {
  return languageBadge(ext)
}

// ══════════════════════════════════════════
//  INIT
// ══════════════════════════════════════════

export function initExplorer(opts: ExplorerOptions): void {
  _opts = opts
  _upgradeFileTree()
  _injectFileTabs()
  _injectSearchOverlay()
  _wireGlobalShortcuts()
}

// ══════════════════════════════════════════
//  ÁRBOL DE ARCHIVOS (reemplaza #file-tree)
// ══════════════════════════════════════════

function _upgradeFileTree(): void {
  const container = document.getElementById('file-tree')
  if (!container) return

  // Reemplazar sb-head con versión mejorada
  const sbHead = container.previousElementSibling as HTMLElement | null
  if (sbHead?.classList.contains('sb-head')) {
    sbHead.innerHTML = `
      <span>Archivos</span>
      <div style="display:flex;gap:3px">
        <button class="btn btn-ghost btn-sm" id="exp-search-btn" title="Buscar (Ctrl+Shift+F)" style="padding:2px 6px">${icon('search', 13)}</button>
        <button class="btn btn-ghost btn-sm" id="btn-add-code">+ Código</button>
        <button class="btn btn-ghost btn-sm" id="btn-add-log">+ Log</button>
      </div>
    `
    document.getElementById('exp-search-btn')?.addEventListener('click', toggleSearch)
  }

  _renderFileTree()
}

function _renderFileTree(): void {
  const container = document.getElementById('file-tree')
  if (!container) return

  if (!state.files.length) {
    container.innerHTML = `<div class="empty">
      Sin archivos cargados
      <button class="btn btn-ghost btn-sm" id="exp-empty-cta">+ Código</button>
    </div>`
    document.getElementById('exp-empty-cta')?.addEventListener('click', () => {
      document.getElementById('btn-add-code')?.click()
    })
    return
  }

  // Agrupar por extensión (pseudo-árbol)
  const groups: Record<string, CodeFile[]> = {}
  for (const f of state.files) {
    const ext = f.ext || '.txt'
    if (!groups[ext]) groups[ext] = []
    groups[ext].push(f)
  }

  // Renderizar como lista plana con grupos colapsables
  let html = ''

  if (Object.keys(groups).length > 1) {
    // Múltiples tipos: agrupar
    for (const [ext, files] of Object.entries(groups)) {
      const icon = extIcon(ext)
      html += `
        <div class="exp-group" data-ext="${ext}">
          <div class="exp-group-head" data-toggle-group="${ext}">
            <span class="exp-arrow">▾</span>
            <span>${icon} ${ext.toUpperCase().replace('.', '')}</span>
            <span class="exp-count">${files.length}</span>
          </div>
          <div class="exp-group-body" data-group-body="${ext}">
            ${files.map((f) => _fileNodeHtml(f)).join('')}
          </div>
        </div>
      `
    }
  } else {
    // Un solo tipo: lista plana
    html = state.files.map((f) => _fileNodeHtml(f)).join('')
  }

  container.innerHTML = html
  _attachTreeEvents(container)
}

function _fileNodeHtml(f: CodeFile): string {
  const isActive = f === state.currentFile
  const n = f.issues.length
  const errCount = f.issues.filter((i) => i.severity === 'error').length
  const warnCount = f.issues.filter((i) => i.severity === 'warning').length

  let badge = ''
  if (errCount) badge = `<span class="exp-badge exp-badge-err">${errCount}</span>`
  else if (warnCount) badge = `<span class="exp-badge exp-badge-warn">${warnCount}</span>`
  else if (f.analyzed) badge = `<span class="exp-badge exp-badge-ok">✓</span>`

  const cls = [
    'exp-file',
    isActive ? 'exp-file-active' : '',
    errCount ? 'exp-file-err' : '',
    warnCount && !errCount ? 'exp-file-warn' : '',
    f.analyzed && !n ? 'exp-file-ok' : '',
  ]
    .filter(Boolean)
    .join(' ')

  return `
    <div class="${cls}" data-file-id="${f.id}" title="${f.name}">
      <span class="exp-file-icon">${extIcon(f.ext)}</span>
      <span class="exp-file-name">${f.name}</span>
      ${badge}
      <button class="exp-file-close" data-remove-id="${f.id}" title="Cerrar">✕</button>
    </div>
  `
}

function _attachTreeEvents(container: HTMLElement): void {
  // Click en archivo
  container.addEventListener('click', (e) => {
    const target = e.target as HTMLElement

    // Botón eliminar
    const removeId = target.closest<HTMLElement>('[data-remove-id]')?.dataset['removeId']
    if (removeId) {
      e.stopPropagation()
      _removeFileFromExplorer(removeId)
      return
    }

    // Seleccionar archivo
    const fileNode = target.closest<HTMLElement>('[data-file-id]')
    const fileId = fileNode?.dataset['fileId']
    if (fileId) {
      explorerSelectFile(fileId)
      return
    }

    // Toggle grupo
    const groupHead = target.closest<HTMLElement>('[data-toggle-group]')
    const groupExt = groupHead?.dataset['toggleGroup']
    if (groupExt) {
      const body = container.querySelector<HTMLElement>(`[data-group-body="${groupExt}"]`)
      const arrow = groupHead.querySelector('.exp-arrow') as HTMLElement
      if (body) {
        const isOpen = body.style.display !== 'none'
        body.style.display = isOpen ? 'none' : ''
        if (arrow) arrow.textContent = isOpen ? '▸' : '▾'
      }
    }
  })
}

function _removeFileFromExplorer(id: string): void {
  state.files = state.files.filter((f) => f.id !== id)
  if (state.currentFile?.id === id) state.currentFile = null
  _closeTab(id)
  _renderFileTree()
  _renderFileTabs()

  // Delegar al sistema existente
  import('./app').then((m) => {
    m.updateFileTree?.()
    m.updateSelectors?.()
    m.updateStats?.()
  })
}

// ══════════════════════════════════════════
//  FILE TABS (multi-archivo en editor bar)
// ══════════════════════════════════════════

function _injectFileTabs(): void {
  const editorBar = document.querySelector('.editor-bar')
  if (!editorBar) return

  // Insertar container de tabs ANTES de la editor bar
  const tabsContainer = document.createElement('div')
  tabsContainer.id = 'file-tabs-bar'
  tabsContainer.className = 'exp-tabs-bar'
  editorBar.parentElement?.insertBefore(tabsContainer, editorBar)

  _renderFileTabs()
}

function _renderFileTabs(): void {
  const bar = document.getElementById('file-tabs-bar')
  if (!bar) return

  if (!_tabs.length) {
    bar.style.display = 'none'
    return
  }

  bar.style.display = 'flex'
  bar.innerHTML = _tabs
    .map(
      (tab) => `
    <div class="exp-tab ${tab.id === _activeTabId ? 'exp-tab-active' : ''}" data-tab-id="${tab.id}">
      <span class="exp-tab-icon">${extIcon(_getExt(tab.name))}</span>
      <span class="exp-tab-name">${tab.name}</span>
      ${tab.modified ? '<span class="exp-tab-dot">●</span>' : ''}
      <button class="exp-tab-close" data-close-tab="${tab.id}">✕</button>
    </div>
  `,
    )
    .join('')

  // Scroll al tab activo
  const activeTab = bar.querySelector<HTMLElement>('.exp-tab-active')
  activeTab?.scrollIntoView({ block: 'nearest', inline: 'nearest' })

  bar.addEventListener('click', (e) => {
    const target = e.target as HTMLElement

    // Cerrar tab
    const closeId = target.closest<HTMLElement>('[data-close-tab]')?.dataset['closeTab']
    if (closeId) {
      e.stopPropagation()
      _closeTab(closeId)
      return
    }

    // Activar tab
    const tabEl = target.closest<HTMLElement>('[data-tab-id]')
    const tabId = tabEl?.dataset['tabId']
    if (tabId) explorerSelectFile(tabId)
  })
}

function _openTab(file: CodeFile): void {
  if (!_tabs.find((t) => t.id === file.id)) {
    _tabs.push({ id: file.id, name: file.name, modified: false })
  }
  _activeTabId = file.id
  _renderFileTabs()
}

function _closeTab(id: string): void {
  const idx = _tabs.findIndex((t) => t.id === id)
  if (idx === -1) return
  _tabs.splice(idx, 1)

  if (_activeTabId === id) {
    // Activar tab anterior o siguiente
    const newActive = _tabs[Math.max(0, idx - 1)]
    if (newActive) {
      explorerSelectFile(newActive.id)
    } else {
      _activeTabId = ''
    }
  }
  _renderFileTabs()
  _renderFileTree()
}

// ══════════════════════════════════════════
//  BÚSQUEDA GLOBAL (Ctrl+Shift+F)
// ══════════════════════════════════════════

function _injectSearchOverlay(): void {
  if (document.getElementById('exp-search-overlay')) return

  const overlay = document.createElement('div')
  overlay.id = 'exp-search-overlay'
  overlay.className = 'exp-search-overlay'
  overlay.innerHTML = `
    <div class="exp-search-modal">
      <div class="exp-search-header">
        <span class="exp-search-icon">${icon('search', 14)}</span>
        <input
          id="exp-search-input"
          class="exp-search-input"
          placeholder="Buscar en todos los archivos..."
          autocomplete="off"
          spellcheck="false"
        />
        <span class="exp-search-hint">ESC para cerrar</span>
      </div>
      <div class="exp-search-filters">
        <label class="exp-search-filter">
          <input type="checkbox" id="exp-search-regex"> Regex
        </label>
        <label class="exp-search-filter">
          <input type="checkbox" id="exp-search-case"> Aa
        </label>
        <span class="exp-search-count" id="exp-search-count"></span>
      </div>
      <div class="exp-search-results" id="exp-search-results">
        <div class="exp-search-empty">Escribe para buscar en todos los archivos cargados</div>
      </div>
    </div>
  `
  document.body.appendChild(overlay)

  // Cerrar al click fuera
  overlay.addEventListener('click', (e) => {
    if (e.target === overlay) closeSearch()
  })

  // Input de búsqueda
  const input = document.getElementById('exp-search-input') as HTMLInputElement
  input?.addEventListener('input', () => {
    _searchQuery = input.value
    _runSearch()
  })

  // Click en resultado
  document.getElementById('exp-search-results')?.addEventListener('click', (e) => {
    const item = (e.target as HTMLElement).closest<HTMLElement>('[data-search-file]')
    if (!item) return
    const fileId = item.dataset['searchFile']
    const line = parseInt(item.dataset['searchLine'] ?? '1', 10)
    if (fileId) {
      explorerSelectFile(fileId)
      closeSearch()
      // Ir a la línea en Monaco
      setTimeout(() => {
        const goTo = (window as any)['editorGoToLine'] as ((l: number) => void) | undefined
        goTo?.(line)
      }, 150)
    }
  })
}

function _runSearch(): void {
  const results = document.getElementById('exp-search-results')
  const count = document.getElementById('exp-search-count')
  if (!results) return

  const q = _searchQuery.trim()
  const useRegex = (document.getElementById('exp-search-regex') as HTMLInputElement)?.checked
  const matchCase = (document.getElementById('exp-search-case') as HTMLInputElement)?.checked

  if (!q) {
    results.innerHTML = '<div class="exp-search-empty">Escribe para buscar en todos los archivos cargados</div>'
    if (count) count.textContent = ''
    return
  }

  let pattern: RegExp
  try {
    pattern = useRegex
      ? new RegExp(q, matchCase ? 'g' : 'gi')
      : new RegExp(q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), matchCase ? 'g' : 'gi')
  } catch {
    results.innerHTML = '<div class="exp-search-empty" style="color:var(--err)">Regex inválido</div>'
    return
  }

  let totalMatches = 0
  let html = ''

  for (const f of state.files) {
    const lines = f.content.split('\n')
    const fileMatches: Array<{ line: number; text: string; highlighted: string }> = []

    lines.forEach((lineText, i) => {
      pattern.lastIndex = 0
      if (pattern.test(lineText)) {
        // Resaltar match
        pattern.lastIndex = 0
        const highlighted = lineText.replace(pattern, (m) => `<mark class="exp-search-mark">${_esc(m)}</mark>`)
        fileMatches.push({ line: i + 1, text: lineText.trim(), highlighted })
        totalMatches++
      }
    })

    if (!fileMatches.length) continue

    html += `
      <div class="exp-search-file">
        <div class="exp-search-file-head">
          <span>${extIcon(f.ext)}</span>
          <span class="exp-search-file-name">${f.name}</span>
          <span class="exp-search-file-count">${fileMatches.length}</span>
        </div>
        ${fileMatches
          .slice(0, 50)
          .map(
            (m) => `
          <div class="exp-search-result" data-search-file="${f.id}" data-search-line="${m.line}">
            <span class="exp-search-line">${m.line}</span>
            <span class="exp-search-text">${m.highlighted.slice(0, 120)}</span>
          </div>
        `,
          )
          .join('')}
        ${fileMatches.length > 50 ? `<div class="exp-search-more">+${fileMatches.length - 50} más...</div>` : ''}
      </div>
    `
  }

  if (!html) {
    results.innerHTML = '<div class="exp-search-empty">Sin resultados</div>'
    if (count) count.textContent = ''
  } else {
    results.innerHTML = html
    if (count) count.textContent = `${totalMatches} resultado(s)`
  }
}

function toggleSearch(): void {
  _searchOpen ? closeSearch() : openSearch()
}

export function openSearch(): void {
  _searchOpen = true
  const overlay = document.getElementById('exp-search-overlay')
  if (overlay) overlay.classList.add('exp-search-visible')
  setTimeout(() => {
    const input = document.getElementById('exp-search-input') as HTMLInputElement
    input?.focus()
    input?.select()
  }, 50)
}

function closeSearch(): void {
  _searchOpen = false
  document.getElementById('exp-search-overlay')?.classList.remove('exp-search-visible')
}

// ══════════════════════════════════════════
//  OUTLINE (panel derecho #rpp-analysis)
//  Se renderiza cuando se abre un archivo
// ══════════════════════════════════════════

function renderOutline(file: CodeFile): void {
  const container = document.getElementById('analysis-content')
  if (!container) return

  const items = _extractOutline(file)

  if (!items.length) {
    container.innerHTML = `
      <div class="exp-outline-empty">
        <div>${file.name}</div>
        <div style="color:var(--muted);font-size:.68rem;margin-top:4px">Sin símbolos detectados</div>
      </div>
    `
    return
  }

  // Agrupar por kind
  const groups: Record<string, typeof items> = {
    function: [],
    class: [],
    import: [],
    interface: [],
    type: [],
    variable: [],
  }
  for (const item of items) {
    const g = groups[item.kind] ?? []
    groups[item.kind] = [...g, item]
  }

  const KIND_LABEL: Record<string, string> = {
    function: 'Functions',
    class: 'Classes',
    import: 'Imports',
    interface: 'Interfaces',
    type: 'Types',
    variable: 'Variables',
  }

  let html = `
    <div class="exp-outline-header">
      <span>${extIcon(file.ext)}</span>
      <span class="exp-outline-filename">${file.name}</span>
    </div>
  `

  for (const [kind, kindItems] of Object.entries(groups)) {
    if (!kindItems.length) continue
    html += `
      <div class="exp-outline-group">
        <div class="exp-outline-group-head" data-outline-toggle="${kind}">
          <span class="exp-arrow">▾</span>
          <span>${KIND_LABEL[kind] ?? kind}</span>
          <span class="exp-count">${kindItems.length}</span>
        </div>
        <div class="exp-outline-group-body" data-outline-body="${kind}">
          ${kindItems
            .map(
              (item) => `
            <div class="exp-outline-item" data-outline-line="${item.line}" title="${item.name}">
              <span class="exp-outline-line">${item.line}</span>
              <span class="exp-outline-name">${item.name}</span>
              ${item.detail ? `<span class="exp-outline-detail">${item.detail}</span>` : ''}
            </div>
          `,
            )
            .join('')}
        </div>
      </div>
    `
  }

  container.innerHTML = html

  // Toggle grupos
  container.querySelectorAll<HTMLElement>('[data-outline-toggle]').forEach((head) => {
    head.addEventListener('click', () => {
      const key = head.dataset['outlineToggle']!
      const body = container.querySelector<HTMLElement>(`[data-outline-body="${key}"]`)
      const arrow = head.querySelector('.exp-arrow') as HTMLElement
      if (!body) return
      const isOpen = body.style.display !== 'none'
      body.style.display = isOpen ? 'none' : ''
      if (arrow) arrow.textContent = isOpen ? '▸' : '▾'
    })
  })

  // Click en símbolo → ir a línea
  container.querySelectorAll<HTMLElement>('[data-outline-line]').forEach((item) => {
    item.addEventListener('click', () => {
      const line = parseInt(item.dataset['outlineLine'] ?? '1', 10)
      const goTo = (window as any)['editorGoToLine'] as ((l: number) => void) | undefined
      goTo?.(line)
      // Cambiar a tab Editor si no está activo
      import('./app').then((m) => m.switchTab?.('editor'))
    })
  })
}

function _extractOutline(file: CodeFile): Array<{ kind: string; name: string; line: number; detail?: string }> {
  const items: Array<{ kind: string; name: string; line: number; detail?: string }> = []
  const lines = file.content.split('\n')

  if (file.ext === '.py') {
    lines.forEach((line, i) => {
      const fn = line.match(/^(\s*)(async\s+)?def\s+(\w+)\s*\(([^)]*)\)/)
      const cls = line.match(/^class\s+(\w+)/)
      const imp = line.match(/^(?:import|from)\s+(\S+)/)
      const const_ = line.match(/^([A-Z_][A-Z0-9_]+)\s*=/)

      if (fn) items.push({ kind: 'function', name: fn[3], line: i + 1, detail: fn[4]?.slice(0, 30) })
      if (cls) items.push({ kind: 'class', name: cls[1], line: i + 1 })
      if (imp) items.push({ kind: 'import', name: imp[1], line: i + 1 })
      if (const_) items.push({ kind: 'variable', name: const_[1], line: i + 1 })
    })
  } else if (['.ts', '.tsx', '.js', '.jsx'].includes(file.ext)) {
    lines.forEach((line, i) => {
      const fn1 = line.match(/(?:export\s+)?(?:async\s+)?function\s+(\w+)\s*\(([^)]*)\)/)
      const fn2 = line.match(/(?:export\s+)?(?:const|let)\s+(\w+)\s*=\s*(?:async\s*)?\(([^)]*)\)\s*=>/)
      const cls = line.match(/(?:export\s+)?class\s+(\w+)/)
      const iface = line.match(/(?:export\s+)?interface\s+(\w+)/)
      const type = line.match(/(?:export\s+)?type\s+(\w+)\s*=/)
      const imp = line.match(/^import\s+.*from\s+['"]([^'"]+)['"]/)
      const const_ = line.match(/^(?:export\s+)?const\s+([A-Z_][A-Z0-9_]+)\s*=/)

      if (fn1) items.push({ kind: 'function', name: fn1[1], line: i + 1, detail: fn1[2]?.slice(0, 20) })
      if (fn2) items.push({ kind: 'function', name: fn2[1], line: i + 1 })
      if (cls) items.push({ kind: 'class', name: cls[1], line: i + 1 })
      if (iface) items.push({ kind: 'interface', name: iface[1], line: i + 1 })
      if (type) items.push({ kind: 'type', name: type[1], line: i + 1 })
      if (imp) items.push({ kind: 'import', name: imp[1], line: i + 1 })
      if (const_) items.push({ kind: 'variable', name: const_[1], line: i + 1 })
    })
  }

  return items
}

// ══════════════════════════════════════════
//  API PÚBLICA
// ══════════════════════════════════════════

export function explorerAddFile(file: CodeFile): void {
  _renderFileTree()
  _openTab(file)
}

export function explorerRemoveFile(id: string): void {
  _closeTab(id)
  _renderFileTree()
}

function explorerSelectFile(id: string): void {
  const f = state.files.find((x) => x.id === id)
  if (!f || !_opts) return

  _openTab(f)
  _opts.onFileOpen(f)
  _renderFileTree()

  // Renderizar outline en panel derecho
  renderOutline(f)

  // Activar tab análisis
  import('./app').then((m) => m.rpTab?.('analysis'))
}

// ══════════════════════════════════════════
//  KEYBOARD SHORTCUTS
// ══════════════════════════════════════════

function _wireGlobalShortcuts(): void {
  document.addEventListener('keydown', (e) => {
    // Ctrl+Shift+F — Búsqueda global
    if (e.ctrlKey && e.shiftKey && e.key === 'F') {
      e.preventDefault()
      toggleSearch()
      return
    }

    // ESC — cerrar búsqueda
    if (e.key === 'Escape' && _searchOpen) {
      closeSearch()
      return
    }

    // Ctrl+W — cerrar tab activo
    if (e.ctrlKey && e.key === 'w') {
      e.preventDefault()
      if (_activeTabId) _closeTab(_activeTabId)
      return
    }

    // Ctrl+Tab — siguiente tab
    if (e.ctrlKey && e.key === 'Tab') {
      e.preventDefault()
      const idx = _tabs.findIndex((t) => t.id === _activeTabId)
      if (_tabs.length > 1) {
        const next = _tabs[(idx + 1) % _tabs.length]
        explorerSelectFile(next.id)
      }
      return
    }
  })
}

// ══════════════════════════════════════════
//  ESTILOS
// ══════════════════════════════════════════

// ─── Utils ────────────────────────────────────────────────────────────────────

function _getExt(filename: string): string {
  const m = filename.match(/\.[^.]+$/)
  return m ? m[0] : ''
}

function _esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}
