// ══════════════════════════════════════════
//  Sythrall — Project Explorer v1.0
//  Árbol expandible + File tabs + Búsqueda global + Outline
//
//  API pública (consumida por app.ts / events.ts / file-browser.ts):
//    initExplorer({ onFileOpen: (f) => ... })  — engancha la apertura de archivos
//    explorerAddFile(file) / explorerRefreshTree() / explorerClearAll()
//    explorerSetFolderRoot(root)  — árbol pendiente de "+ Carpeta"
//    openSearch() / toggleSearch()
//
//  Dueño único de #file-tree: árbol jerárquico real (carpetas + archivos
//  sueltos conviven, ver utils/file-tree.ts::buildMergedTree). La delegación
//  de click se ata UNA sola vez en initExplorer() — _renderFileTree() nunca
//  vuelve a tocar listeners, solo innerHTML.
// ══════════════════════════════════════════

import { MAX_RENDERED_CHILDREN } from '../panels/upload'
import { state } from '../store/state'
import type { CodeFile } from '../types'
import { buildMergedTree, type FolderTreeNode } from '../utils/file-tree'
import { appendLog, fmtBytes, getExt, toast, uniqueId } from '../utils/helpers'
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
let _folderRoot: FolderTreeNode | null = null
const _expandedDirs = new Set<string>()

// ─── Iconos ───────────────────────────────────────────────────────────────────

function extIcon(ext: string): string {
  return languageBadge(ext)
}

// ══════════════════════════════════════════
//  INIT
// ══════════════════════════════════════════

export function initExplorer(opts: ExplorerOptions): void {
  _opts = opts
  _renderFileTree()
  _wireTreeEvents()
  _injectFileTabs()
  _injectSearchOverlay()
  _wireGlobalShortcuts()
}

// ══════════════════════════════════════════
//  ÁRBOL DE ARCHIVOS (#file-tree)
// ══════════════════════════════════════════

function _renderFileTree(): void {
  const container = document.getElementById('file-tree')
  if (!container) return

  if (!state.files.length && !_folderRoot) {
    container.innerHTML = `<div class="empty">
      Sin archivos cargados
      <button class="btn btn-ghost btn-sm" id="exp-empty-cta">+ Código</button>
    </div>`
    return
  }

  const merged = buildMergedTree(state.files, _folderRoot)
  container.innerHTML = `<div class="dz-tree">${(merged.children ?? []).map((c) => _renderTreeNode(c, 0)).join('')}</div>`
}

function _renderTreeNode(node: FolderTreeNode, depth: number): string {
  const pad = depth * 14

  if (node.type === 'directory') {
    if (depth === 0 && !_expandedDirs.has(node.path)) _expandedDirs.add(node.path)
    const isOpen = _expandedDirs.has(node.path)
    const allChildren = node.children ?? []
    const visibleChildren = allChildren.slice(0, MAX_RENDERED_CHILDREN)
    const hiddenCount = allChildren.length - visibleChildren.length
    const childrenHtml = isOpen ? visibleChildren.map((c) => _renderTreeNode(c, depth + 1)).join('') : ''

    return `
      <div class="tree-dir">
        <div class="tree-row dir-row" style="padding-left:${pad + 6}px" data-tree-toggle="${_esc(node.path)}">
          <span class="tree-expand">${isOpen ? '▾' : '▸'}</span>
          <span class="tree-name">${_esc(node.name)}</span>
          ${node.children?.length ? `<span class="tree-count">${node.children.length}</span>` : ''}
        </div>
        <div class="tree-children" ${isOpen ? '' : 'style="display:none"'}>
          ${childrenHtml}
          ${isOpen && hiddenCount > 0 ? `<div class="tree-truncated" style="padding-left:${(depth + 1) * 14 + 20}px">… +${hiddenCount} más (carpeta muy grande, no se muestran todos)</div>` : ''}
        </div>
      </div>`
  }

  const cf = node.codeFileId ? state.files.find((f) => f.id === node.codeFileId) : undefined
  const isActive = !!cf && cf === state.currentFile
  const badge = cf ? _fileBadgeHtml(cf) : ''
  const ext = `.${node.name.split('.').pop()}`

  return `
    <div class="tree-row file-row ${isActive ? 'active' : ''}"
      style="padding-left:${pad + 20}px"
      data-tree-file="${_esc(node.path)}"
      ${cf ? `data-file-id="${cf.id}"` : ''}
      title="${_esc(node.path)}">
      <span>${extIcon(ext)}</span>
      <span class="tree-name">${_esc(node.name)}</span>
      ${badge}
      ${cf ? `<button class="exp-file-close" data-remove-id="${cf.id}" title="Cerrar">✕</button>` : ''}
    </div>`
}

// El badge muestra el TOTAL de issues (no solo la severidad dominante) —
// antes mostraba errCount||warnCount, que para un archivo con 0 errores pero
// muchos warnings + algunos infos daba un número distinto al que Métricas/
// Logs muestran para el mismo archivo (f.issues.length), sin ninguna etiqueta
// que explicara la diferencia. Mismo dato en todos lados; el color sigue
// codificando severidad (rojo si hay al menos un error).
function _fileBadgeHtml(f: CodeFile): string {
  const n = f.issues.length
  if (!n) return f.analyzed ? `<span class="exp-badge exp-badge-ok">✓</span>` : ''
  const hasErr = f.issues.some((i) => i.severity === 'error')
  return `<span class="exp-badge ${hasErr ? 'exp-badge-err' : 'exp-badge-warn'}">${n}</span>`
}

// Delegación de click atada UNA sola vez (en initExplorer) — _renderFileTree()
// solo escribe innerHTML, nunca vuelve a atar listeners. Antes, cada render
// agregaba otro addEventListener sobre el mismo contenedor persistente, y se
// iban acumulando sin límite (cada click disparaba N aperturas/cambios de tab).
function _wireTreeEvents(): void {
  const container = document.getElementById('file-tree')
  if (!container) return

  container.addEventListener('click', (e) => {
    const target = e.target as HTMLElement

    if (target.closest('#exp-empty-cta')) {
      document.getElementById('btn-add-code')?.click()
      return
    }

    const removeId = target.closest<HTMLElement>('[data-remove-id]')?.dataset['removeId']
    if (removeId) {
      e.stopPropagation()
      _removeFileFromExplorer(removeId)
      return
    }

    const toggle = target.closest<HTMLElement>('[data-tree-toggle]')
    if (toggle) {
      e.stopPropagation()
      const path = toggle.dataset['treeToggle']
      if (!path) return
      if (_expandedDirs.has(path)) _expandedDirs.delete(path)
      else _expandedDirs.add(path)
      _renderFileTree()
      return
    }

    const fileRow = target.closest<HTMLElement>('[data-tree-file]')
    if (fileRow) {
      e.stopPropagation()
      const fileId = fileRow.dataset['fileId']
      if (fileId) {
        explorerSelectFile(fileId)
        return
      }
      const path = fileRow.dataset['treeFile']
      if (path) _openLazyFile(path)
    }
  })
}

// ══════════════════════════════════════════
//  "+ Carpeta" — árbol pendiente (archivos aún no leídos)
// ══════════════════════════════════════════

export function explorerSetFolderRoot(root: FolderTreeNode | null): void {
  _folderRoot = root
  if (root) {
    for (const c of root.children ?? []) {
      if (c.type === 'directory') _expandedDirs.add(c.path)
    }
  }
  _renderFileTree()
}

function _findLazyNode(node: FolderTreeNode, path: string): FolderTreeNode | null {
  if (node.path === path) return node
  for (const child of node.children ?? []) {
    const found = _findLazyNode(child, path)
    if (found) return found
  }
  return null
}

function _openLazyFile(path: string): void {
  if (!_folderRoot) return
  const node = _findLazyNode(_folderRoot, path)
  if (node?.type !== 'file' || !node.file) return

  const f = node.file
  const reader = new FileReader()
  reader.onload = (e) => {
    const file: CodeFile = {
      id: uniqueId(),
      name: f.name,
      ext: getExt(f.name),
      size: f.size,
      content: e.target!.result as string,
      issues: [],
      metrics: {},
      analyzed: false,
      path,
    }
    state.files.push(file)
    explorerAddFile(file)
    import('./app').then((m) => {
      m.updateSelectors()
      m.updateBadges()
    })
    explorerSelectFile(file.id)
    appendLog('info', `${path} (${fmtBytes(f.size)})`, 'fe')
    toast(f.name, 'ok')
  }
  reader.readAsText(f)
}

function _removeFileFromExplorer(id: string): void {
  state.files = state.files.filter((f) => f.id !== id)
  if (state.currentFile?.id === id) state.currentFile = null
  _closeTab(id)
  _renderFileTree()
  _renderFileTabs()

  // Delegar al sistema existente
  import('./app').then((m) => {
    m.updateSelectors?.()
    m.updateBadges?.()
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

  _wireTabsEvents(tabsContainer)
  _renderFileTabs()
}

// Delegación atada una sola vez, igual que _wireTreeEvents() — el contenedor
// se crea una única vez en _injectFileTabs(); _renderFileTabs() corre en
// cada apertura/cierre de tab y antes volvía a atar un listener cada vez.
function _wireTabsEvents(bar: HTMLElement): void {
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

export function toggleSearch(): void {
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

/** Refresca el árbol (badges, resaltado de archivo activo) sin tocar tabs/carpeta pendiente. */
export function explorerRefreshTree(): void {
  _renderFileTree()
}

/** Limpieza total tras "Limpiar" — árbol, tabs y carpeta pendiente. */
export function explorerClearAll(): void {
  _tabs.length = 0
  _activeTabId = ''
  _folderRoot = null
  _expandedDirs.clear()
  _renderFileTree()
  _renderFileTabs()
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
