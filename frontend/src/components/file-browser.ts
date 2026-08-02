// ══════════════════════════════════════════
//  CodeWatch PRO — Explorador de carpetas (cross-browser, estilo VSCode)
//  Arma y renderiza el árbol de una carpeta elegida vía <input webkitdirectory>
//  (funciona en Chrome/Edge/Firefox/Safari — a diferencia de la File System
//  Access API, que es solo Chromium). Al abrir un archivo del árbol se
//  reusa el mismo pipeline de análisis que "+ Código" (selectFile/state.files).
// ══════════════════════════════════════════
// components/file-browser.ts

import { EXT_ICONS, MAX_RENDERED_CHILDREN } from '../panels/upload'
import { state } from '../store/state'
import type { CodeFile } from '../types'
import { buildTreeFromFileList, type FolderTreeNode } from '../utils/file-tree'
import { appendLog, esc, fmtBytes, getExt, toast, uniqueId } from '../utils/helpers'
import { selectFile, updateSelectors, updateStats } from './app'
import { explorerAddFile } from './explorer'

const expanded = new Set<string>()
let selectedPath: string | null = null
let currentRoot: FolderTreeNode | null = null

export function handleFolderPick(files: FileList | null): void {
  if (!files || files.length === 0) return
  expanded.clear()
  selectedPath = null
  currentRoot = buildTreeFromFileList(files)
  renderFolderTree()
  appendLog('info', `📂 Carpeta explorada: ${currentRoot.name} (${files.length} archivos)`, 'fe')
}

function findNode(node: FolderTreeNode, path: string): FolderTreeNode | null {
  if (node.path === path) return node
  for (const child of node.children ?? []) {
    const found = findNode(child, path)
    if (found) return found
  }
  return null
}

function openTreeFile(path: string): void {
  if (!currentRoot) return
  const node = findNode(currentRoot, path)
  if (node?.type !== 'file' || !node.file) return

  selectedPath = path
  const existing = state.files.find((f) => f.path === path)
  if (existing) {
    // selectFile() internamente llama a updateFileTree() (la lista plana de
    // app.ts), que pisa #file-tree — se vuelve a renderizar el árbol justo
    // después, en el mismo tick síncrono, así que no hay parpadeo visible.
    selectFile(existing.id)
    renderFolderTree()
    return
  }

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
    updateSelectors()
    updateStats()
    selectFile(file.id)
    renderFolderTree()
    appendLog('info', `📁 ${path} (${fmtBytes(f.size)})`, 'fe')
    toast('✅ ' + f.name, 'ok')
  }
  reader.readAsText(f)
}

function renderNode(node: FolderTreeNode, depth: number): string {
  if (depth === 0 && !expanded.has(node.path)) expanded.add(node.path)

  if (node.type === 'directory') {
    const isOpen = expanded.has(node.path)
    const pad = depth * 14
    const allChildren = node.children ?? []
    const visibleChildren = allChildren.slice(0, MAX_RENDERED_CHILDREN)
    const hiddenCount = allChildren.length - visibleChildren.length
    const childrenHtml = isOpen ? visibleChildren.map((c) => renderNode(c, depth + 1)).join('') : ''

    return `
      <div class="tree-dir">
        <div class="tree-row dir-row" style="padding-left:${pad + 6}px" data-tree-toggle="${esc(node.path)}">
          <span class="tree-expand">${isOpen ? '▾' : '▸'}</span>
          <span>${isOpen ? '📂' : '📁'}</span>
          <span class="tree-name">${esc(node.name)}</span>
          ${node.children?.length ? `<span class="tree-count">${node.children.length}</span>` : ''}
        </div>
        <div class="tree-children" ${isOpen ? '' : 'style="display:none"'}>
          ${childrenHtml}
          ${isOpen && hiddenCount > 0 ? `<div class="tree-truncated" style="padding-left:${(depth + 1) * 14 + 20}px">… +${hiddenCount} más (carpeta muy grande, no se muestran todos)</div>` : ''}
        </div>
      </div>`
  }

  const pad = depth * 14 + 20
  const ext = `.${node.name.split('.').pop()}`
  const icon = EXT_ICONS[ext] ?? '📄'
  const isActive = selectedPath === node.path

  return `
    <div class="tree-row file-row ${isActive ? 'active' : ''}"
      style="padding-left:${pad}px"
      data-tree-file="${esc(node.path)}"
      title="${esc(node.path)}">
      <span>${icon}</span>
      <span class="tree-name">${esc(node.name)}</span>
      <span class="tree-size">${node.file ? fmtBytes(node.file.size) : ''}</span>
    </div>`
}

function renderFolderTree(): void {
  const el = document.getElementById('file-tree')
  if (!el || !currentRoot) return

  el.innerHTML = `<div class="dz-tree">${renderNode(currentRoot, 0)}</div>`

  // Asignación directa (no addEventListener) — mismo patrón que updateFileTree()
  // en app.ts: cualquiera de los dos renders que corra último queda como
  // dueño exclusivo de #file-tree, sin listeners duplicados/huérfanos.
  el.onclick = (e: MouseEvent) => {
    const target = e.target as HTMLElement

    const toggle = target.closest<HTMLElement>('[data-tree-toggle]')
    if (toggle) {
      e.stopPropagation()
      const path = toggle.dataset['treeToggle']
      if (!path) return
      if (expanded.has(path)) expanded.delete(path)
      else expanded.add(path)
      renderFolderTree()
      return
    }

    const fileRow = target.closest<HTMLElement>('[data-tree-file]')
    if (fileRow) {
      e.stopPropagation()
      const path = fileRow.dataset['treeFile']
      if (path) openTreeFile(path)
    }
  }
}
