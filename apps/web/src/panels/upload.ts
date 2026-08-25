// ══════════════════════════════════════════
//  Sythrall — Upload Panel
//  Mismo estilo que analysis.ts / apis.ts
// ══════════════════════════════════════════
// panels/upload.ts

import type { ProjectSummary, ProjectTreeNode, UploadProgress, UploadResult } from '../api/client'
import { api } from '../api/client'
import { addFile, setActiveProject, state } from '../store/state'
import type { CodeFile } from '../types'
import { healthTier } from '../utils/health'
import { appendLog, toast } from '../utils/helpers'
import { icon, languageBadge } from '../utils/icons'

// ─── Filtro de carpetas del sistema en uploads de carpeta ─────────────────────
//
// Mismo IGNORED_DIRS que ya usa el backend (services/project_service.py) para
// ZIPs y para construir el árbol — acá se aplica ANTES de mandar nada, no solo
// para descartar en el servidor: elegir la carpeta raíz de un proyecto JS/TS
// normal (con node_modules presente, algo casi inevitable con el picker de
// carpeta del browser) mandaba decenas de miles de archivos innecesarios al
// backend. Caso real reportado por el usuario: 27614 archivos, 513 MB, la
// subida terminaba con la conexión cortada a mitad de camino en vez de un
// error claro. Filtrar acá evita leer/subir esos bytes desde el vamos, no solo
// que el backend los descarte después de recibirlos.
const IGNORED_DIR_NAMES = new Set([
  '__pycache__',
  '.git',
  '.svn',
  'node_modules',
  '.venv',
  'venv',
  '.idea',
  '.vscode',
  'dist',
  'build',
  '.next',
  '.nuxt',
  'coverage',
  '.pytest_cache',
  '.mypy_cache',
])

function _filterIgnoredDirs(files: File[]): { kept: File[]; skipped: number } {
  const kept: File[] = []
  let skipped = 0
  for (const f of files) {
    const relPath = (f as File & { webkitRelativePath?: string }).webkitRelativePath
    const parts = relPath ? relPath.split('/').slice(0, -1) : []
    if (parts.some((p) => IGNORED_DIR_NAMES.has(p))) skipped++
    else kept.push(f)
  }
  return { kept, skipped }
}

// ─── Estado del panel ─────────────────────────────────────────────────────────

type UploadTab = 'files' | 'folder' | 'zip' | 'empty'

interface UploadPanelState {
  activeTab: UploadTab
  pendingFiles: File[]
  pendingZip: File | null
  projectName: string
  isUploading: boolean
  uploadPct: number
  projects: ProjectSummary[]
  activeResult: UploadResult | null
  /** El formulario de subida (tabs/dropzone/nombre/botón) queda oculto por
   * defecto — Proyectos es un grid limpio con un solo "+ Nuevo proyecto"
   * (estilo Vercel), no un formulario siempre visible compitiendo con el
   * grid por atención. Se muestra al tocar ese botón, y vuelve a ocultarse
   * al cancelar o terminar de subir. */
  showCreateForm: boolean
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
  showCreateForm: false,
}

// ─── Tab config ───────────────────────────────────────────────────────────────

const TABS: Record<UploadTab, { label: string; hint: string; dropText: string }> = {
  files: {
    label: 'Archivos',
    hint: 'Cualquier extensión · Máx 50 MB c/u',
    dropText: 'Arrastra archivos aquí',
  },
  folder: {
    label: 'Carpeta',
    hint: 'Se preserva la estructura de carpetas',
    dropText: 'Arrastra una carpeta aquí',
  },
  zip: {
    label: 'ZIP',
    hint: 'Se descomprime automáticamente · Máx 200 MB',
    dropText: 'Arrastra un .zip aquí',
  },
  // Sin dropzone/archivos — crea el proyecto directo y queda listo para
  // escribir código desde cero con "+ Nuevo archivo" (ver renderResult()).
  // Pedido explícito del usuario: poder crear proyectos "sin subir carpetas
  // o nada" para trabajar codificando ahí mismo.
  empty: {
    label: 'Vacío',
    hint: 'Sin archivos — los agregás después con + Nuevo archivo',
    dropText: '',
  },
}

// ─── Render principal ─────────────────────────────────────────────────────────

// El formulario de subida (tabs/dropzone/nombre/submit) aparece solo
// mientras se está armando una subida nueva y todavía no hay resultado —
// una vez que hay un `activeResult` (recién subido, o un proyecto existente
// abierto desde el grid) pasa a la vista de explorador (`renderResult()`,
// más abajo), nunca ambas cosas a la vez. Compartida con
// `renderRecentProjects()` (que oculta su propio "+ Nuevo proyecto" acá).
function _formVisible(): boolean {
  return (
    !st.activeResult && (st.showCreateForm || st.pendingFiles.length > 0 || st.pendingZip !== null || st.isUploading)
  )
}

export function renderUploadPanel(): void {
  const el = document.getElementById('upload-content')
  if (!el) return

  const tab = TABS[st.activeTab]
  const hasPending = st.pendingFiles.length > 0 || st.pendingZip !== null
  const totalSize = fmtSize(st.pendingFiles.reduce((s, f) => s + f.size, 0) + (st.pendingZip?.size ?? 0))
  const showForm = _formVisible()

  el.innerHTML = `
    <!-- Header del hub -->
    <div class="up-hub-header">
      <div class="up-hub-title">Proyectos</div>
      <div class="up-hub-sub">Un proyecto activo es el contexto que usan Static, Hallazgos, Arquitectura, Métricas y ML/DL — creá uno nuevo, o elegí uno de los existentes, para explorarlo y abrir sus archivos.</div>
    </div>

    ${
      st.activeResult
        ? // Vista de explorador — proyecto recién subido o existente abierto
          // desde el grid, mismo caso: nombre + stats + árbol clickeable.
          renderResult(st.activeResult)
        : !showForm
          ? renderRecentProjects()
          : `
    <div class="up-form-head">
      <div class="ms-title">Nuevo proyecto</div>
      <button class="btn btn-ghost btn-sm" id="up-cancel-create-btn">Cancelar</button>
    </div>

    <!-- Tabs -->
    <div class="up-tabs">
      ${(Object.entries(TABS) as [UploadTab, typeof tab][])
        .map(
          ([id, t]) => `
        <button class="up-tab ${st.activeTab === id ? 'active' : ''}" data-up-tab="${id}">
          ${t.label}
        </button>`,
        )
        .join('')}
    </div>

    <!-- Drop zone — el tab "Vacío" no tiene nada que arrastrar/seleccionar,
    se salta del todo (ver TABS.empty más arriba). -->
    ${
      st.activeTab === 'empty'
        ? ''
        : `
    <div class="up-dropzone ${hasPending ? 'has-files' : ''}" id="up-dropzone">
      ${
        !hasPending
          ? `
        <div class="up-drop-content">
          <div class="up-drop-primary">${tab.dropText}</div>
          <div class="up-drop-secondary">o haz clic para seleccionar</div>
          <div class="up-drop-hint">${tab.hint}</div>
        </div>
      `
          : `
        <div class="up-pending">
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
    `
    }

    <!-- Input oculto -->
    <input type="file" id="up-file-input" style="display:none" />

    <!-- Nombre del proyecto — siempre visible en "Vacío" (no depende de
    tener archivos pendientes, no hay ninguno en ese modo) -->
    ${
      hasPending || st.activeTab === 'empty'
        ? `
      <div class="up-name-field">
        <label for="up-project-name">Nombre del proyecto (opcional)</label>
        <input type="text" id="up-project-name" class="up-text-input"
          placeholder="mi-proyecto" value="${esc(st.projectName)}" />
      </div>
    `
        : ''
    }

    <!-- Botón de subida / creación -->
    <button class="btn btn-primary" id="up-submit-btn" style="width:100%;justify-content:center"
      ${(!hasPending && st.activeTab !== 'empty') || st.isUploading ? 'disabled' : ''}>
      ${
        st.isUploading
          ? `<span class="up-spinner"></span> Subiendo... ${st.uploadPct}%`
          : st.activeTab === 'empty'
            ? 'Crear proyecto'
            : 'Subir Proyecto'
      }
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
    `
    }
  `

  attachUploadEvents(el)
}

// ─── Explorador de proyecto (recién subido, o existente abierto del grid) ─────
//
// Mismo componente para los dos casos — al llegar acá el proyecto YA está
// activo (`doUpload()`/`loadProject()` ya llamaron `setActiveProject()`),
// así que no hace falta un botón "Usar en editor" aparte: clickear un
// archivo del árbol ya lo abre directo en el Editor (`data-tree-file` →
// `openProjectFile()` → `selectFile()` → `switchTab('editor')`, sin pasar
// por Proyectos como un paso intermedio — ver la nota sobre sacar "Editor"
// del nav-rail en CHANGELOG.md).

function renderResult(r: UploadResult): string {
  const errHtml =
    (r.errors?.length ?? 0) > 0
      ? `<div class="up-partial-errors">
        <div class="up-partial-title">${icon('warning', 12)} ${r.errors!.length} archivo(s) con problemas:</div>
        <ul>${r.errors!.map((e) => `<li>${esc(e.file)}: ${esc(e.reason)}</li>`).join('')}</ul>
       </div>`
      : ''

  // `r.tree` en sí es siempre un objeto real, incluso para un proyecto vacío
  // (la raíz existe, solo con `children: []`) — el fallback de "no tiene
  // archivos" de abajo nunca disparaba por chequear la verdad de `r.tree`
  // en vez de si tiene contenido real. Encontrado creando un proyecto vacío
  // y viendo la raíz pelada (sin ningún indicio de "todavía no hay nada").
  const treeHtml = r.tree?.children?.length ? renderTree(r.tree, 0) : ''
  const info = r.info

  return `
    <div class="up-result">
      <div class="up-result-header" style="color:var(--ok)">
        <div>
          <strong>${esc(r.project_name)}</strong>
          <span class="up-result-meta">${r.total_files} archivos · tipo: ${r.type}${info ? ' · ' + info.total_size_fmt : ''}</span>
        </div>
        <button class="btn btn-ghost btn-sm" id="up-cancel-create-btn" style="margin-left:auto">← Volver a Proyectos</button>
      </div>
      ${errHtml}
      ${
        info
          ? `
        <div class="up-result-stats">
          ${statChip(info.total_files + ' archivos')}
          ${statChip(info.code_files + ' código')}
          ${statChip(info.total_size_fmt)}
          ${Object.entries(info.by_extension)
            .slice(0, 3)
            .map(([ext, n]) => statChip(`${ext} ×${n}`))
            .join('')}
        </div>
      `
          : ''
      }
      <div class="up-tree-actions">
        <button class="btn btn-ghost btn-sm" id="up-new-file-btn">+ Nuevo archivo</button>
      </div>
      ${treeHtml ? `<div class="up-tree">${treeHtml}</div>` : '<div class="empty">Este proyecto no tiene archivos todavía — creá uno con "+ Nuevo archivo".</div>'}
    </div>
  `
}

function statChip(label: string): string {
  return `<span class="up-stat-chip">${esc(label)}</span>`
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

// ─── Proyectos recientes (grid de tarjetas) ───────────────────────────────────

// Extensiones que no aportan como "lenguaje del proyecto" en la fila de badges
// de la tarjeta — quedan afuera del top-3 aunque tengan muchos archivos.
const _NON_LANG_EXT = new Set(['(sin extensión)', '.md', '.txt', '.json', '.lock', '.yml', '.yaml', '.gitignore'])

function _topLanguageBadges(byExtension: Record<string, number>): string {
  const langs = Object.entries(byExtension)
    .filter(([ext]) => !_NON_LANG_EXT.has(ext))
    .sort((a, b) => b[1] - a[1])
    .slice(0, 3)
  if (!langs.length) return ''
  return langs.map(([ext]) => languageBadge(ext)).join('')
}

function _relativeTime(iso: string): string {
  const ms = Date.now() - new Date(iso).getTime()
  const min = Math.floor(ms / 60_000)
  if (min < 1) return 'recién'
  if (min < 60) return `hace ${min}min`
  const hr = Math.floor(min / 60)
  if (hr < 24) return `hace ${hr}h`
  const days = Math.floor(hr / 24)
  return `hace ${days}d`
}

/** Health cacheado de este proyecto (ver `state.projectHealthCache`, poblado
 * por `loadProjectHealth()` en dashboard.ts) — SOLO se muestra si ya se
 * calculó de verdad esta sesión. Nunca se dispara un análisis acá solo para
 * poder mostrar un score en la tarjeta (sería caro — un `parse-project`
 * completo por proyecto — y un progreso inventado si se mostrara estimado). */
function _healthBadgeRow(projectId: string): string {
  const h = state.projectHealthCache[projectId]
  if (!h) return `<span class="up-card-health-pill">Sin analizar</span>`
  const cells: Array<[string, number]> = [
    ['S', h.security.score],
    ['Q', h.quality.score],
    ['C', h.complexity.score],
    ['A', h.architecture.score],
  ]
  return cells
    .map(
      ([label, score]) =>
        `<span class="up-card-health-cell" style="color:${healthTier(score)}">${label} ${score}</span>`,
    )
    .join('')
}

function renderRecentProjects(): string {
  const formOpen = _formVisible()
  if (!st.projects.length) {
    return `
      <div class="up-empty-hub">
        <div class="up-empty-title">Todavía no creaste ningún proyecto</div>
        <div class="up-empty-sub">Subí archivos, una carpeta o un ZIP para crear el primero — queda activo automáticamente.</div>
        ${!formOpen ? '<button class="btn btn-run btn-sm" id="up-new-project-btn">+ Nuevo proyecto</button>' : ''}
      </div>`
  }
  return `
    <div class="up-project-grid-head">
      <div class="ms-title">Proyectos</div>
      ${!formOpen ? '<button class="btn btn-run btn-sm" id="up-new-project-btn">+ Nuevo proyecto</button>' : ''}
    </div>
    <div class="up-project-grid">
      ${st.projects
        .map((p) => {
          const isActive = p.project_id === state.activeProjectId
          const name = p.project_name || `${p.project_id.slice(0, 8)}…`
          return `
        <div class="up-project-card${isActive ? ' active' : ''}" data-open-project="${esc(p.project_id)}" title="${esc(p.project_id)}">
          <div class="up-card-head">
            <span class="up-card-name">${esc(name)}</span>
            <button class="up-card-del" data-del-project="${esc(p.project_id)}" title="Eliminar">${icon('trash', 13)}</button>
          </div>
          ${isActive ? '<span class="pill pill-ok up-active-badge">Activo</span>' : ''}
          <div class="up-card-langs">${_topLanguageBadges(p.by_extension)}</div>
          <div class="up-card-health">${_healthBadgeRow(p.project_id)}</div>
          <div class="up-card-meta">${p.total_files} archivos · ${p.total_size_fmt} · ${_relativeTime(p.created_at)}</div>
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
    if (st.activeTab === 'zip') {
      st.pendingZip = files[0] ?? null
    } else if (st.activeTab === 'folder') {
      const { kept, skipped } = _filterIgnoredDirs(files)
      st.pendingFiles = kept
      if (skipped) toast(`${skipped} archivo(s) de node_modules/.git/etc. excluidos`, 'warn')
    } else {
      st.pendingFiles = files
    }
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
  el.querySelector('#up-submit-btn')?.addEventListener('click', () => {
    if (st.activeTab === 'empty') void doCreateEmpty()
    else doUpload()
  })

  // ── Nuevo archivo (dentro del explorador de un proyecto ya activo)
  el.querySelector('#up-new-file-btn')?.addEventListener('click', () => void doCreateFile())

  // ── Árbol: toggle dirs + selección de archivos
  //
  // Delegado directo sobre `el` (#upload-content), a diferencia de todos los
  // demás listeners de esta función — esos van sobre nodos hijos que
  // `el.innerHTML = ...` recrea en cada render (el nodo viejo con su
  // listener viejo se descarta solo), pero `el` en sí es el MISMO elemento
  // en cada render de `renderUploadPanel()`. Sin el guard de abajo, cada
  // render agregaba OTRO listener más sobre ese mismo `el`, para siempre —
  // un solo click terminaba disparando N handlers apilados a la vez (ej.
  // "Eliminar proyecto" llamando a `removeProject()` N veces concurrentes
  // para el mismo id, o un toggle de carpeta que necesitaba un número par/
  // impar de clicks según cuántos renders hubiera pasado antes, en vez de
  // uno solo) — bug real, encontrado en vivo: un click nativo despachado a
  // mano sobre el toggle de una carpeta no la expandía en absoluto tras
  // varios renders previos (upload→zip tab→seleccionar archivo→submit ya
  // habían disparado varios renders, un número par de listeners apilados se
  // cancelaba entre sí). Guardado con un flag en el propio nodo para que
  // sobreviva sin importar cuántas veces se vuelva a llamar esta función.
  if (!el.dataset['treeEventsWired']) {
    el.dataset['treeEventsWired'] = '1'
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

      // Abrir proyecto (click en la tarjeta del grid) — activa y muestra su
      // árbol acá mismo (loadProject ya llama a renderUploadPanel()), en vez
      // de saltar a otro tab: elegir el proyecto y explorarlo son el mismo
      // paso, coherente con "Proyecto > archivos" en vez de un tab Editor
      // aparte (ver nota en CHANGELOG.md).
      const openCard = target.closest<HTMLElement>('[data-open-project]')
      if (openCard) {
        e.stopPropagation()
        await loadProject(openCard.dataset['openProject']!)
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
    })
  }

  // ── Proyectos (grid)
  el.querySelectorAll<HTMLElement>('[data-open-project]').forEach((card) => {
    card.addEventListener('click', async (e) => {
      e.stopPropagation()
      await loadProject(card.dataset['openProject']!)
    })
  })
  el.querySelectorAll<HTMLElement>('[data-del-project]').forEach((btn) => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation()
      if (!confirm('¿Eliminar este proyecto?')) return
      await removeProject(btn.dataset['delProject']!)
    })
  })
  el.querySelector('#up-new-project-btn')?.addEventListener('click', (e) => {
    e.stopPropagation()
    st.showCreateForm = true
    renderUploadPanel()
  })
  el.querySelector('#up-cancel-create-btn')?.addEventListener('click', (e) => {
    e.stopPropagation()
    st.showCreateForm = false
    st.pendingFiles = []
    st.pendingZip = null
    st.activeResult = null
    renderUploadPanel()
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
      toast('Solo se acepta un archivo .zip en este modo', 'err')
      return
    }
  } else if (st.activeTab === 'folder') {
    const { kept, skipped } = _filterIgnoredDirs(files)
    st.pendingFiles = kept
    if (skipped) toast(`${skipped} archivo(s) de node_modules/.git/etc. excluidos`, 'warn')
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
    if (label) label.textContent = `Subiendo... ${p.percent}%`
  }

  try {
    let result: UploadResult

    if (st.activeTab === 'zip') {
      if (!st.pendingZip) throw new Error('No hay ZIP seleccionado.')
      result = await api.uploadZip(st.pendingZip, st.projectName, onProgress)
      appendLog(
        'ok',
        `ZIP subido: ${result.project_name} — ${result.extracted ?? result.total_files} archivos extraídos`,
        'be',
      )
    } else if (st.activeTab === 'folder') {
      if (!st.pendingFiles.length) throw new Error('No hay archivos.')
      result = await api.uploadFolder(st.pendingFiles, st.projectName, onProgress)
      appendLog('ok', `Carpeta subida: ${result.project_name} — ${result.total_files} archivos`, 'be')
    } else {
      if (!st.pendingFiles.length) throw new Error('No hay archivos.')
      result = await api.uploadFiles(st.pendingFiles, st.projectName, onProgress)
      appendLog('ok', `${result.total_files} archivo(s) subido(s) al proyecto ${result.project_name}`, 'be')
    }

    st.activeResult = result
    setActiveProject(result.project_id, result.project_name)
    st.pendingFiles = []
    st.pendingZip = null
    st.projectName = ''
    toast(`${result.total_files} archivos subidos — proyecto activo`, 'ok')

    // Refrescar lista de proyectos
    await loadRecentProjects()
  } catch (err) {
    const msg = err instanceof Error ? err.message : 'Error al subir.'
    toast(msg, 'err')
    appendLog('err', 'Upload error: ' + msg, 'fe')
  } finally {
    st.isUploading = false
    renderUploadPanel()
  }
}

// Crea un proyecto sin ningún archivo (tab "Vacío") — para poder trabajar
// escribiendo código desde cero con "+ Nuevo archivo" en vez de partir
// siempre de algo ya subido. Mismo tramo final que doUpload() (activar el
// resultado, marcar el proyecto activo, refrescar el grid).
async function doCreateEmpty(): Promise<void> {
  if (st.isUploading) return
  st.isUploading = true
  renderUploadPanel()

  try {
    const result = await api.createEmptyProject(st.projectName)
    appendLog('ok', `Proyecto vacío creado: ${result.project_name}`, 'be')

    st.activeResult = result
    setActiveProject(result.project_id, result.project_name)
    st.projectName = ''
    toast('Proyecto vacío creado — activo', 'ok')

    await loadRecentProjects()
  } catch (err) {
    const msg = err instanceof Error ? err.message : 'Error al crear el proyecto.'
    toast(msg, 'err')
    appendLog('err', 'Create empty project error: ' + msg, 'fe')
  } finally {
    st.isUploading = false
    renderUploadPanel()
  }
}

// "+ Nuevo archivo" — crea un archivo (vacío) dentro del proyecto activo y lo
// abre directo en el Editor, mismo camino que clickear un archivo del árbol
// (openProjectFile). Necesario para poder codificar desde cero (un proyecto
// vacío, o sumarle algo nuevo a cualquier otro) en vez de depender siempre
// de subir algo ya escrito.
async function doCreateFile(): Promise<void> {
  if (!st.activeResult) return
  const path = prompt('Nombre del archivo (con ruta si querés, ej. src/app.py):')?.trim()
  if (!path) return

  try {
    await api.createProjectFile(st.activeResult.project_id, path, '')

    // Refrescar el árbol del proyecto activo sin perder qué carpetas
    // estaban expandidas (a diferencia de loadProject(), que resetea todo
    // eso porque ahí el usuario recién está entrando al proyecto).
    const data = await api.getProjectTree(st.activeResult.project_id)
    st.activeResult = {
      ...st.activeResult,
      total_files: data.info.total_files,
      tree: data.tree,
      info: data.info,
    }
    renderUploadPanel()

    await openProjectFile(st.activeResult.project_id, path)
    toast(`${path} creado`, 'ok')
  } catch (err) {
    const msg = err instanceof Error ? err.message : 'Error al crear el archivo.'
    toast(msg, 'err')
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
      const [{ explorerAddFile }, { updateSelectors }] = await Promise.all([
        import('../components/explorer'),
        import('../components/app'),
      ])
      explorerAddFile(file)
      updateSelectors()
    }

    const { selectFile } = await import('../components/app')
    selectFile(id)

    appendLog('ok', `${filePath} abierto en editor`, 'be')
  } catch (err) {
    toast('Error al abrir archivo: ' + (err instanceof Error ? err.message : ''), 'err')
  }
}

// ─── Cargar proyecto reciente ─────────────────────────────────────────────────

async function loadProject(projectId: string): Promise<void> {
  try {
    const data = await api.getProjectTree(projectId)
    st.activeResult = {
      project_id: data.project_id,
      project_name: data.info.project_name || data.project_id.slice(0, 8),
      type: 'files',
      total_files: data.info.total_files,
      tree: data.tree,
      info: data.info,
    }
    setActiveProject(projectId, st.activeResult.project_name)
    expanded.clear()
    selectedPath = null
    renderUploadPanel()
    toast('Proyecto cargado — activo', 'ok')
  } catch (err) {
    toast('Error al cargar: ' + (err instanceof Error ? err.message : ''), 'err')
  }
}

// ─── Eliminar proyecto ────────────────────────────────────────────────────────

async function removeProject(projectId: string): Promise<void> {
  try {
    await api.deleteProject(projectId)
    if (st.activeResult?.project_id === projectId) st.activeResult = null
    if (state.activeProjectId === projectId) setActiveProject(null)
    toast('Proyecto eliminado', 'warn')
    appendLog('info', `Proyecto ${projectId.slice(0, 8)} eliminado`, 'fe')
    await loadRecentProjects()
  } catch (err) {
    toast('Error: ' + (err instanceof Error ? err.message : ''), 'err')
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
