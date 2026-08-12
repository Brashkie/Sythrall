// ══════════════════════════════════════════
//  Sythrall — Upload Panel
//  Mismo estilo que analysis.ts / apis.ts
// ══════════════════════════════════════════
// panels/upload.ts

import type { ProjectSummary, ProjectTreeNode, UploadProgress, UploadResult } from '../api/client'
import { api } from '../api/client'
import { addFile, setActiveProject, state } from '../store/state'
import type { CodeFile } from '../types'
import { appendLog, toast } from '../utils/helpers'
import { languageBadge } from '../utils/icons'

// ─── Estado del panel ─────────────────────────────────────────────────────────

type UploadTab = 'files' | 'folder' | 'zip'

interface UploadPanelState {
  activeTab: UploadTab
  pendingFiles: File[]
  pendingZip: File | null
  projectName: string
  isUploading: boolean
  uploadPct: number
  projects: ProjectSummary[]
  activeResult: UploadResult | null
}

const st: UploadPanelState = {
  activeTab: 'files',
  pendingFiles: [],
  pendingZip: null,
  projectName: '',
  isUploading: false,
  uploadPct: 0,
  projects: [],
  activeResult: null,
}

// ─── Tab config ───────────────────────────────────────────────────────────────

const TABS: Record<UploadTab, { icon: string; label: string; hint: string; dropText: string }> = {
  files: {
    icon: '📄',
    label: 'Archivos',
    hint: 'Cualquier extensión · Máx 50 MB c/u',
    dropText: 'Arrastra archivos aquí',
  },
  folder: {
    icon: '📁',
    label: 'Carpeta',
    hint: 'Se preserva la estructura de carpetas',
    dropText: 'Arrastra una carpeta aquí',
  },
  zip: {
    icon: '🗜️',
    label: 'ZIP',
    hint: 'Se descomprime automáticamente · Máx 200 MB',
    dropText: 'Arrastra un .zip aquí',
  },
}

// ─── Render principal ─────────────────────────────────────────────────────────

export function renderUploadPanel(): void {
  const el = document.getElementById('upload-content')
  if (!el) return

  const tab = TABS[st.activeTab]
  const hasPending = st.pendingFiles.length > 0 || st.pendingZip !== null
  const totalSize = fmtSize(st.pendingFiles.reduce((s, f) => s + f.size, 0) + (st.pendingZip?.size ?? 0))

  el.innerHTML = `
    <!-- Header del hub -->
    <div class="up-hub-header">
      <div class="up-hub-title">Proyectos</div>
      <div class="up-hub-sub">Un proyecto activo es el contexto que usan Editor, Issues, Diagrama, Static y ML/DL — subí uno, o elegí uno de los recientes, para trabajar sobre él.</div>
    </div>

    <!-- Tabs -->
    <div class="up-tabs">
      ${(Object.entries(TABS) as [UploadTab, typeof tab][])
        .map(
          ([id, t]) => `
        <button class="up-tab ${st.activeTab === id ? 'active' : ''}" data-up-tab="${id}">
          ${t.icon} ${t.label}
        </button>`,
        )
        .join('')}
    </div>

    <!-- Drop zone -->
    <div class="up-dropzone ${hasPending ? 'has-files' : ''}" id="up-dropzone">
      ${
        !hasPending
          ? `
        <div class="up-drop-content">
          <div class="up-drop-icon">${tab.icon}</div>
          <div class="up-drop-primary">${tab.dropText}</div>
          <div class="up-drop-secondary">o haz clic para seleccionar</div>
          <div class="up-drop-hint">${tab.hint}</div>
        </div>
      `
          : `
        <div class="up-pending">
          <div class="up-drop-icon">${tab.icon}</div>
          <div class="up-pending-count">
            <strong>${st.pendingFiles.length || (st.pendingZip ? 1 : 0)}</strong>
            archivo${(st.pendingFiles.length || 1) !== 1 ? 's' : ''} listo${(st.pendingFiles.length || 1) !== 1 ? 's' : ''}
          </div>
          <div class="up-pending-size">Tamaño total: ${totalSize}</div>
          <button class="btn btn-danger btn-sm" id="up-clear-btn">✕ Limpiar</button>
        </div>
      `
      }
    </div>

    <!-- Input oculto -->
    <input type="file" id="up-file-input" style="display:none" />

    <!-- Nombre del proyecto -->
    ${
      hasPending
        ? `
      <div class="up-name-field">
        <label for="up-project-name">Nombre del proyecto (opcional)</label>
        <input type="text" id="up-project-name" class="up-text-input"
          placeholder="mi-proyecto" value="${esc(st.projectName)}" />
      </div>
    `
        : ''
    }

    <!-- Botón de subida -->
    <button class="btn btn-primary" id="up-submit-btn" style="width:100%;justify-content:center"
      ${!hasPending || st.isUploading ? 'disabled' : ''}>
      ${st.isUploading ? `<span class="up-spinner"></span> Subiendo... ${st.uploadPct}%` : '🚀 Subir Proyecto'}
    </button>

    <!-- Progress bar -->
    ${
      st.isUploading
        ? `
      <div class="up-progress-wrap">
        <div class="up-progress-bar" style="width:${st.uploadPct}%"></div>
      </div>
    `
        : ''
    }

    <!-- Resultado -->
    ${st.activeResult ? renderResult(st.activeResult) : ''}

    <!-- Proyectos recientes -->
    ${renderRecentProjects()}
  `

  attachUploadEvents(el)
}

// ─── Resultado de subida ──────────────────────────────────────────────────────

function renderResult(r: UploadResult): string {
  const errHtml =
    (r.errors?.length ?? 0) > 0
      ? `<div class="up-partial-errors">
        <div class="up-partial-title">⚠️ ${r.errors!.length} archivo(s) con problemas:</div>
        <ul>${r.errors!.map((e) => `<li>${esc(e.file)}: ${esc(e.reason)}</li>`).join('')}</ul>
       </div>`
      : ''

  const treeHtml = r.tree ? renderTree(r.tree, 0) : ''
  const info = r.info

  return `
    <div class="up-result">
      <div class="up-result-header">
        <span>✅</span>
        <div>
          <strong>${esc(r.project_name)}</strong>
          <span class="up-result-meta">${r.total_files} archivos · tipo: ${r.type}${info ? ' · ' + info.total_size_fmt : ''}</span>
        </div>
        <button class="btn btn-ghost btn-sm" id="up-load-in-editor" data-project-id="${r.project_id}" style="margin-left:auto">
          📂 Usar en editor
        </button>
      </div>
      ${errHtml}
      ${
        info
          ? `
        <div class="up-result-stats">
          ${statChip('📄', info.total_files + ' archivos')}
          ${statChip('💻', info.code_files + ' código')}
          ${statChip('💾', info.total_size_fmt)}
          ${Object.entries(info.by_extension)
            .slice(0, 3)
            .map(([ext, n]) => statChip('', `${ext} ×${n}`))
            .join('')}
        </div>
      `
          : ''
      }
      ${treeHtml ? `<div class="up-tree">${treeHtml}</div>` : ''}
    </div>
  `
}

function statChip(icon: string, label: string): string {
  return `<span class="up-stat-chip">${icon ? icon + ' ' : ''}${esc(label)}</span>`
}

// ─── Árbol de archivos ────────────────────────────────────────────────────────

// Nodos expandidos (persiste entre re-renders)
const expanded = new Set<string>()
let selectedPath: string | null = null

// Tope de hijos renderizados por carpeta — una carpeta con miles de archivos
// (ej. node_modules sin filtrar, o un dataset) puede armar de una un innerHTML
// gigante y trabar el navegador. El backend ya trunca por profundidad; esto
// cubre el caso de una sola carpeta muy ancha.
export const MAX_RENDERED_CHILDREN = 300

function renderTree(node: ProjectTreeNode, depth: number): string {
  if (depth === 0) {
    // Auto-expand primer nivel
    if (!expanded.has(node.path)) expanded.add(node.path)
  }

  if (node.type === 'directory') {
    const isOpen = expanded.has(node.path)
    const pad = depth * 14
    const allChildren = node.children ?? []
    const visibleChildren = allChildren.slice(0, MAX_RENDERED_CHILDREN)
    const hiddenCount = allChildren.length - visibleChildren.length

    const childrenHtml = isOpen ? visibleChildren.map((c) => renderTree(c, depth + 1)).join('') : ''

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
          ${node.truncated ? `<div class="tree-truncated" style="padding-left:${(depth + 1) * 14 + 20}px">… árbol truncado</div>` : ''}
        </div>
      </div>`
  }

  // File
  const pad = depth * 14 + 20
  const icon = languageBadge(node.extension ?? '')
  const isActive = selectedPath === node.path

  return `
    <div class="tree-row file-row ${isActive ? 'active' : ''}"
      style="padding-left:${pad}px"
      data-tree-file="${esc(node.path)}"
      title="${esc(node.path)}">
      <span>${icon}</span>
      <span class="tree-name">${esc(node.name)}</span>
      <span class="tree-size">${node.size_fmt ?? ''}</span>
    </div>`
}

// ─── Proyectos recientes ──────────────────────────────────────────────────────

function renderRecentProjects(): string {
  if (!st.projects.length) {
    return `
      <div class="up-empty-hub">
        <div class="up-empty-icon">📂</div>
        <div class="up-empty-title">Todavía no creaste ningún proyecto</div>
        <div class="up-empty-sub">Subí archivos, una carpeta o un ZIP arriba para crear el primero — queda activo automáticamente.</div>
      </div>`
  }
  return `
    <div class="metric-section" style="margin-top:10px">
      <div class="ms-title">📋 Proyectos recientes</div>
      ${st.projects
        .slice(0, 5)
        .map((p) => {
          const isActive = p.project_id === state.activeProjectId
          return `
        <div class="up-recent-item${isActive ? ' up-recent-active' : ''}">
          <span>📁</span>
          <div class="up-recent-info">
            <span class="up-recent-id">${esc(p.project_id.slice(0, 8))}…${isActive ? ' <span class="pill pill-ok up-active-badge">● Activo</span>' : ''}</span>
            <span class="up-recent-meta">${p.total_files} archivos · ${p.total_size_fmt}</span>
          </div>
          <button class="btn btn-ghost btn-sm" data-load-project="${esc(p.project_id)}" title="Usar como proyecto activo">📂</button>
          <button class="btn btn-danger btn-sm" data-del-project="${esc(p.project_id)}" title="Eliminar">🗑️</button>
        </div>
      `
        })
        .join('')}
    </div>`
}

// ─── Eventos ──────────────────────────────────────────────────────────────────

function attachUploadEvents(el: HTMLElement): void {
  // ── Tabs
  el.querySelectorAll<HTMLElement>('[data-up-tab]').forEach((btn) => {
    btn.addEventListener('click', () => {
      st.activeTab = btn.dataset['upTab'] as UploadTab
      st.pendingFiles = []
      st.pendingZip = null
      renderUploadPanel()
    })
  })

  // ── Drop zone click
  el.querySelector('#up-dropzone')?.addEventListener('click', () => triggerInput())

  // ── Drag & drop en dropzone
  const dz = el.querySelector<HTMLElement>('#up-dropzone')
  dz?.addEventListener('dragover', (e) => {
    e.preventDefault()
    e.stopPropagation()
    dz.classList.add('drag-over')
  })
  dz?.addEventListener('dragleave', () => dz.classList.remove('drag-over'))
  dz?.addEventListener('drop', (e) => {
    e.preventDefault()
    e.stopPropagation()
    dz.classList.remove('drag-over')
    handleDrop(e as DragEvent)
  })

  // ── File input change
  const input = el.querySelector<HTMLInputElement>('#up-file-input')
  input?.addEventListener('change', () => {
    if (!input.files) return
    const files = Array.from(input.files)
    if (st.activeTab === 'zip') st.pendingZip = files[0] ?? null
    else st.pendingFiles = files
    input.value = ''
    renderUploadPanel()
  })

  // ── Project name
  el.querySelector<HTMLInputElement>('#up-project-name')?.addEventListener('input', (e) => {
    st.projectName = (e.target as HTMLInputElement).value.trim()
  })

  // ── Clear
  el.querySelector('#up-clear-btn')?.addEventListener('click', (e) => {
    e.stopPropagation()
    st.pendingFiles = []
    st.pendingZip = null
    st.activeResult = null
    renderUploadPanel()
  })

  // ── Submit
  el.querySelector('#up-submit-btn')?.addEventListener('click', () => doUpload())

  // ── Árbol: toggle dirs + selección de archivos
  el.addEventListener('click', async (e) => {
    const target = e.target as HTMLElement

    const toggle = target.closest<HTMLElement>('[data-tree-toggle]')
    if (toggle) {
      e.stopPropagation()
      const path = toggle.dataset['treeToggle']!
      if (expanded.has(path)) expanded.delete(path)
      else expanded.add(path)
      renderUploadPanel()
      return
    }

    const fileRow = target.closest<HTMLElement>('[data-tree-file]')
    if (fileRow && st.activeResult) {
      e.stopPropagation()
      selectedPath = fileRow.dataset['treeFile']!
      renderUploadPanel()
      // Cargar contenido del archivo y abrirlo en el editor
      await openProjectFile(st.activeResult.project_id, selectedPath)
      return
    }

    // Cargar en editor
    const loadBtn = target.closest<HTMLElement>('[data-load-project]')
    if (loadBtn) {
      e.stopPropagation()
      await loadProject(loadBtn.dataset['loadProject']!)
      return
    }

    // Eliminar proyecto
    const delBtn = target.closest<HTMLElement>('[data-del-project]')
    if (delBtn) {
      e.stopPropagation()
      if (!confirm('¿Eliminar este proyecto?')) return
      await removeProject(delBtn.dataset['delProject']!)
      return
    }

    // Usar en editor
    const useBtn = target.closest<HTMLElement>('#up-load-in-editor')
    if (useBtn) {
      e.stopPropagation()
      if (st.activeResult) setActiveProject(st.activeResult.project_id)
      toast(
        '✅ Proyecto activo — Issues/Diagrama/Static/ML/DL ya lo usan. Click un archivo del árbol para abrirlo.',
        'ok',
      )
      return
    }
  })

  // ── Recientes
  el.querySelectorAll<HTMLElement>('[data-load-project]').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation()
      loadProject(btn.dataset['loadProject']!)
    })
  })
  el.querySelectorAll<HTMLElement>('[data-del-project]').forEach((btn) => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation()
      if (!confirm('¿Eliminar este proyecto?')) return
      await removeProject(btn.dataset['delProject']!)
    })
  })
}

// ─── Input trigger ────────────────────────────────────────────────────────────

function triggerInput(): void {
  const input = document.querySelector<HTMLInputElement>('#up-file-input')
  if (!input) return

  input.multiple = st.activeTab !== 'zip'
  input.accept = st.activeTab === 'zip' ? '.zip' : '*'

  if (st.activeTab === 'folder') {
    input.setAttribute('webkitdirectory', '')
    input.setAttribute('mozdirectory', '')
  } else {
    input.removeAttribute('webkitdirectory')
    input.removeAttribute('mozdirectory')
  }

  input.click()
}

// ─── Drop ─────────────────────────────────────────────────────────────────────

function handleDrop(e: DragEvent): void {
  const files = Array.from(e.dataTransfer?.files ?? [])
  if (!files.length) return

  if (st.activeTab === 'zip') {
    if (files[0].name.toLowerCase().endsWith('.zip')) {
      st.pendingZip = files[0]
    } else {
      toast('❌ Solo se acepta un archivo .zip en este modo', 'err')
      return
    }
  } else {
    st.pendingFiles = files
  }
  renderUploadPanel()
}

// ─── Upload ───────────────────────────────────────────────────────────────────

async function doUpload(): Promise<void> {
  if (st.isUploading) return

  st.isUploading = true
  st.uploadPct = 0
  renderUploadPanel()

  const onProgress = (p: UploadProgress) => {
    st.uploadPct = p.percent
    // Actualizar solo la barra sin re-render completo
    const bar = document.querySelector<HTMLElement>('.up-progress-bar')
    const label = document.querySelector<HTMLElement>('#up-submit-btn')
    if (bar) bar.style.width = p.percent + '%'
    if (label) label.textContent = `⏳ Subiendo... ${p.percent}%`
  }

  try {
    let result: UploadResult

    if (st.activeTab === 'zip') {
      if (!st.pendingZip) throw new Error('No hay ZIP seleccionado.')
      result = await api.uploadZip(st.pendingZip, st.projectName, onProgress)
      appendLog(
        'ok',
        `🗜️ ZIP subido: ${result.project_name} — ${result.extracted ?? result.total_files} archivos extraídos`,
        'be',
      )
    } else if (st.activeTab === 'folder') {
      if (!st.pendingFiles.length) throw new Error('No hay archivos.')
      result = await api.uploadFolder(st.pendingFiles, st.projectName, onProgress)
      appendLog('ok', `📁 Carpeta subida: ${result.project_name} — ${result.total_files} archivos`, 'be')
    } else {
      if (!st.pendingFiles.length) throw new Error('No hay archivos.')
      result = await api.uploadFiles(st.pendingFiles, st.projectName, onProgress)
      appendLog('ok', `📄 ${result.total_files} archivo(s) subido(s) al proyecto ${result.project_name}`, 'be')
    }

    st.activeResult = result
    setActiveProject(result.project_id)
    st.pendingFiles = []
    st.pendingZip = null
    st.projectName = ''
    toast(`✅ ${result.total_files} archivos subidos — proyecto activo`, 'ok')

    // Refrescar lista de proyectos
    await loadRecentProjects()
  } catch (err) {
    const msg = err instanceof Error ? err.message : 'Error al subir.'
    toast('❌ ' + msg, 'err')
    appendLog('err', 'Upload error: ' + msg, 'fe')
  } finally {
    st.isUploading = false
    renderUploadPanel()
  }
}

// ─── Cargar archivo del proyecto al working set (mismo camino que "+ Código") ──
//
// Antes esto solo pintaba el buffer de Monaco sin tocar state.files, así que
// Issues/Diagrama/Static/ML/DL (que leen state.files) no veían nada aunque el
// código estuviera a la vista en el Editor. Ahora reusa el mismo pipeline que
// "+ Código" (components/app.ts:228) y "+ Carpeta" (file-browser.ts) — el
// archivo queda disponible para el resto de la app, no solo para mirarlo.

export async function openProjectFile(projectId: string, filePath: string): Promise<void> {
  try {
    const fileContent = await api.getFileContent(projectId, filePath)
    const ext = fileContent.extension ?? filePath.split('.').pop() ?? ''
    const id = `upload-${projectId}-${filePath}`

    const existing = state.files.find((f) => f.id === id)
    if (!existing) {
      const file: CodeFile = {
        id,
        name: filePath.split('/').pop() ?? filePath,
        ext: ext.startsWith('.') ? ext : '.' + ext,
        size: fileContent.size,
        content: fileContent.content,
        issues: [],
        metrics: {},
        analyzed: false,
        path: filePath,
      }
      addFile(file)

      // Importados dinámicamente para no crear dependencia circular con app.ts/explorer.ts
      const [{ explorerAddFile }, { updateSelectors, updateStats }] = await Promise.all([
        import('../components/explorer'),
        import('../components/app'),
      ])
      explorerAddFile(file)
      updateSelectors()
      updateStats()
    }

    const { selectFile } = await import('../components/app')
    selectFile(id)

    appendLog('ok', `📄 ${filePath} abierto en editor`, 'be')
  } catch (err) {
    toast('❌ Error al abrir archivo: ' + (err instanceof Error ? err.message : ''), 'err')
  }
}

// ─── Cargar proyecto reciente ─────────────────────────────────────────────────

async function loadProject(projectId: string): Promise<void> {
  try {
    const data = await api.getProjectTree(projectId)
    st.activeResult = {
      project_id: data.project_id,
      project_name: data.project_id.slice(0, 8),
      type: 'files',
      total_files: data.info.total_files,
      tree: data.tree,
      info: data.info,
    }
    setActiveProject(projectId)
    expanded.clear()
    selectedPath = null
    renderUploadPanel()
    toast('📂 Proyecto cargado — activo', 'ok')
  } catch (err) {
    toast('❌ Error al cargar: ' + (err instanceof Error ? err.message : ''), 'err')
  }
}

// ─── Eliminar proyecto ────────────────────────────────────────────────────────

async function removeProject(projectId: string): Promise<void> {
  try {
    await api.deleteProject(projectId)
    if (st.activeResult?.project_id === projectId) st.activeResult = null
    if (state.activeProjectId === projectId) setActiveProject(null)
    toast('🗑️ Proyecto eliminado', 'warn')
    appendLog('info', `Proyecto ${projectId.slice(0, 8)} eliminado`, 'fe')
    await loadRecentProjects()
  } catch (err) {
    toast('❌ Error: ' + (err instanceof Error ? err.message : ''), 'err')
  }
}

// ─── Cargar proyectos recientes ───────────────────────────────────────────────

export async function loadRecentProjects(): Promise<void> {
  try {
    const { projects } = await api.listProjects()
    st.projects = projects
    renderUploadPanel()
  } catch {
    // Silencioso — no crítico si el backend no responde
  }
}

// ─── Utilidades ───────────────────────────────────────────────────────────────

function fmtSize(bytes: number): string {
  const units = ['B', 'KB', 'MB', 'GB']
  let s = bytes
  for (const u of units) {
    if (s < 1024) return `${s.toFixed(1)} ${u}`
    s /= 1024
  }
  return `${s.toFixed(1)} TB`
}

function esc(str: string): string {
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
}
