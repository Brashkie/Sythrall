// ══════════════════════════════════════════
//  Sythrall — App Component
// ══════════════════════════════════════════
// src/components/app.ts

import { api } from '../api/client'
import { renderFileAnalysis, renderMetrics } from '../panels/analysis'
import { renderAPICards, renderIssuesList } from '../panels/apis'
import { generateCodeGraph, generateProjectGraph } from '../panels/graph'
import { clientMLAnalysis, renderMLResults } from '../panels/ml'
import { clearSession, restoreSession, saveSession } from '../panels/problems'
import { loadPersistedActiveProject, setActiveProject, state } from '../store/state'
import type { TabId } from '../types'
import { appendLog, delay, fmtBytes, getExt, nowStr, setProgress, toast, uniqueId } from '../utils/helpers'
import { createCollapseToggle, createResizer } from '../utils/resizer'
import { initCharts, renderComplexityBars, renderDistChart, renderRTChart, updateHistChart } from './charts'
import { applyMarkers, getEditorValue, initEditor, loadFileInEditor } from './editor'
import { explorerAddFile, explorerRemoveFile, initExplorer } from './explorer'
import { renderFlow, setStep, updateRunMeta } from './flow'
import { initMermaid, renderDiagram } from './mermaid'

// ── Tab system — 'upload' agregado
const TABS: TabId[] = [
  'dashboard',
  'editor',
  'apis',
  'issues',
  'diagram',
  'ml',
  'metrics',
  'diff',
  'logs',
  'upload',
  'static',
]

export function switchTab(name: TabId): void {
  TABS.forEach((t) => {
    document.getElementById('t-' + t)?.classList.toggle('active', t === name)
    document.getElementById('panel-' + t)?.classList.toggle('active', t === name)
  })

  // Monaco necesita relayout después de que el panel sea visible
  if (name === 'editor') {
    setTimeout(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const fn = (window as any)['editorRelayout'] as (() => void) | undefined
      fn?.()
    }, 50)
    setTimeout(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const fn = (window as any)['editorRelayout'] as (() => void) | undefined
      fn?.()
    }, 300)
  }

  // Renderizar panel Upload al activarlo
  if (name === 'upload') {
    import('../panels/upload').then(({ renderUploadPanel, loadRecentProjects }) => {
      renderUploadPanel()
      loadRecentProjects()
    })
  }
  if (name === 'static') {
    import('../panels/static').then(({ renderStaticPanel }) => {
      renderStaticPanel()
    })
  }
}

// ── Right panel tabs
export function rpTab(name: 'flow' | 'analysis' | 'server' | 'problems'): void {
  document.querySelectorAll<HTMLElement>('.rp-tab').forEach((el, i) => {
    el.classList.toggle('active', ['flow', 'analysis', 'server', 'problems'][i] === name)
  })
  document.querySelectorAll<HTMLElement>('.rp-panel').forEach((el) => {
    el.classList.remove('active')
  })
  document.getElementById('rpp-' + name)?.classList.add('active')
}

// ── Global pill
function updateGlobalPill(): void {
  const pill = document.getElementById('global-pill')!
  const st = overallStatus()
  const map = {
    ok: { cls: 'pill-ok', dot: 'dot-ok', lbl: 'TODO OK' },
    warning: { cls: 'pill-warn', dot: 'dot-warn', lbl: 'ADVERTENCIAS' },
    down: { cls: 'pill-err', dot: 'dot-err', lbl: 'ERRORES' },
  }
  const m = map[st]
  pill.className = `pill ${m.cls}`
  pill.innerHTML = `<span class="dot ${m.dot}"></span><span>${m.lbl}</span>`
}

function overallStatus(): 'ok' | 'warning' | 'down' {
  if (state.results.apis.some((a) => a.status === 'down') || state.results.issues.some((i) => i.severity === 'error'))
    return 'down'
  if (state.results.issues.some((i) => i.severity === 'warning') || state.results.logErrors.length) return 'warning'
  return 'ok'
}

// ── Stats
export function updateStats(): void {
  const ok = state.results.apis.filter((a) => a.status === 'ok').length
  const n = state.results.issues.length

  setEl('sv-api', state.urls.length ? `${ok}/${state.urls.length}` : '—')
  setEl('ss-api', `${ok} activo(s)`)
  setEl('sv-files', state.files.length || '—')
  setEl('sv-issues', String(n || '0'))
  const issEl = document.getElementById('sv-issues')
  if (issEl) issEl.style.color = n ? 'var(--err)' : 'var(--ok)'
  setEl('ss-issues', `${state.results.issues.filter((i) => i.severity === 'error').length} errores`)

  const scores = state.files.filter((f) => f.metrics?.pylint_score != null).map((f) => f.metrics.pylint_score!)
  const avg = scores.length ? scores.reduce((a, b) => a + b, 0) / scores.length : null
  const scoreEl = document.getElementById('sv-score')!
  scoreEl.textContent = avg != null ? avg.toFixed(1) : '—'
  scoreEl.style.color = avg == null ? 'var(--muted)' : avg >= 8 ? 'var(--ok)' : avg >= 5 ? 'var(--warn)' : 'var(--err)'
}

function updateBadges(): void {
  const nDown = state.results.apis.filter((a) => a.status === 'down' || a.status === 'error').length
  const ta = document.getElementById('tb-apis')
  if (ta) ta.style.display = nDown ? '' : 'none'

  const n = state.results.issues.filter((i) => i.severity === 'error').length
  const ti = document.getElementById('tb-issues')!
  ti.textContent = String(n)
  ti.style.display = n ? '' : 'none'
  const bni = document.getElementById('bn-badge-issues')
  if (bni) {
    bni.textContent = String(n)
    bni.style.display = n ? '' : 'none'
  }
  const tf = document.getElementById('tb-files')!
  const nf = state.files.length
  tf.textContent = String(nf)
  tf.style.display = nf ? '' : 'none'
  const bnf = document.getElementById('bn-badge-files')
  if (bnf) {
    bnf.textContent = String(nf)
    bnf.style.display = nf ? '' : 'none'
  }
}

function setEl(id: string, val: unknown): void {
  const el = document.getElementById(id)
  if (el) el.textContent = String(val)
}

// ── Backend check
export async function checkBackend(): Promise<void> {
  const badge = document.getElementById('be-badge')!
  const txt = document.getElementById('be-txt')!
  badge.className = 'be-badge be-loading'
  txt.textContent = 'Conectando...'
  try {
    const d = await api.capabilities()
    state.backendOk = true
    badge.className = 'be-badge be-ok'
    txt.textContent = 'Backend OK'
    const allCaps = [
      'flake8',
      'pylint',
      'complexity',
      'numpy',
      'pandas',
      'polars',
      'sklearn',
      'lightgbm',
      'torch',
      'tensorflow',
      'scipy',
      'opencv',
      'plotly',
      'spacy',
      'icecream',
    ]
    const capsRow = document.getElementById('caps-row')!
    capsRow.innerHTML = allCaps
      .map((k) => `<span class="cap-chip ${d[k] ? 'cap-on' : 'cap-off'}">${d[k] ? '✓' : '✗'} ${k}</span>`)
      .join('')
    const serverInfo = document.getElementById('server-info')!
    serverInfo.innerHTML = `
      <div class="metric-section">
        <div class="ms-title">Servidor</div>
        ${mr('Python', d.python?.toString().split(' ')[0])}
        ${mr('flake8', d.flake8 ? '✓' : '✗')}
        ${mr('pylint', d.pylint ? '✓' : '✗')}
        ${mr('complexity', d.complexity ? '✓' : '✗')}
        ${mr('PyTorch', d.torch ? '✓' : '✗')}
        ${mr('TensorFlow', d.tensorflow ? '✓' : '✗')}
        ${mr('Polars', d.polars ? '✓' : '✗')}
        ${mr('LightGBM', d.lightgbm ? '✓' : '✗')}
        ${mr('spaCy', d.spacy ? '✓' : '✗')}
      </div>`
    appendLog('ok', '🖥 Backend OK — ' + d.server, 'be')
  } catch (e) {
    state.backendOk = false
    badge.className = 'be-badge be-err'
    txt.textContent = 'Sin backend'
    document.getElementById('caps-row')!.innerHTML =
      '<span style="font-size:.62rem;color:var(--err);font-family:var(--mono)">docker compose up</span>'
    appendLog('err', 'Backend no disponible: ' + (e as Error).message, 'fe')
  }
}

function mr(k: string, v: unknown, color?: string): string {
  return `<div class="metric-row"><span class="mr-k">${k}</span><span class="mr-v"${color ? ` style="color:${color}"` : ''}>${v ?? '—'}</span></div>`
}

// ── Session restore: reabre el proyecto activo y el último archivo, si había,
// al volver a la app. Solo tiene sentido con backend real (necesita traer
// contenido de un proyecto persistido) — sin backend, la sesión previa vivía
// solo en memoria de todos modos.
async function tryRestoreSession(): Promise<void> {
  if (!state.backendOk) return
  const projectId = loadPersistedActiveProject()
  if (!projectId) return

  try {
    await api.getProjectTree(projectId) // confirma que el proyecto sigue existiendo
    setActiveProject(projectId)
    appendLog('ok', `📂 Proyecto activo restaurado (${projectId.slice(0, 8)}…)`, 'be')

    const lastFile = restoreSession()
    if (lastFile) {
      const { openProjectFile } = await import('../panels/upload')
      await openProjectFile(projectId, lastFile)
      toast('↺ Sesión restaurada', 'ok')
    }
  } catch {
    // El proyecto guardado ya no existe (borrado, servidor reiniciado en otro
    // lado, etc.) — se limpia la referencia en vez de quedar en un estado roto.
    setActiveProject(null)
  }
}

// ══════════════════════════════════════════
//  FILE MANAGEMENT
// ══════════════════════════════════════════
export function handleCodeFiles(files: FileList | null): void {
  if (!files?.length) return
  const fileArray = Array.from(files)

  fileArray.forEach((f) => {
    if (state.files.find((x) => x.name === f.name)) return
    const reader = new FileReader()
    reader.onload = (e) => {
      const ext = getExt(f.name)
      const file = {
        id: uniqueId(),
        name: f.name,
        ext,
        size: f.size,
        content: e.target!.result as string,
        issues: [],
        metrics: {},
        analyzed: false,
      }
      state.files.push(file)
      explorerAddFile(file)
      updateFileTree()
      updateSelectors()
      updateStats()
      appendLog('info', `📁 ${f.name} (${fmtBytes(f.size)})`, 'fe')
      toast('✅ ' + f.name, 'ok')
    }
    reader.readAsText(f)
  })

  // "+ Código" ya no deja archivos sueltos efímeros: se guardan en el
  // proyecto activo (o crean uno nuevo) — mismo camino que Proyectos, un
  // solo lugar donde vive el trabajo en vez de dos modelos separados.
  if (state.backendOk) void persistFilesToProject(fileArray)
}

export async function persistFilesToProject(files: File[], mode: 'files' | 'folder' = 'files'): Promise<void> {
  const upload = mode === 'folder' ? api.uploadFolder : api.uploadFiles
  try {
    if (state.activeProjectId) {
      await upload(files, '', undefined, state.activeProjectId)
      appendLog('ok', `📦 ${files.length} archivo(s) guardado(s) en el proyecto activo`, 'be')
      return
    }
    const name = window.prompt('Nombre del proyecto nuevo (cancelar = no guardar, solo en esta sesión):', '')
    if (name === null) return
    const result = await upload(files, name)
    setActiveProject(result.project_id)
    appendLog('ok', `📦 Proyecto "${result.project_name}" creado y activo`, 'be')
    toast('📂 Proyecto creado — activo', 'ok')
  } catch (e) {
    appendLog('err', `Error guardando en el proyecto: ${(e as Error).message}`, 'fe')
  }
}

export function handleLogFiles(files: FileList | null): void {
  if (!files) return
  Array.from(files).forEach((f) => {
    const reader = new FileReader()
    reader.onload = (e) => {
      state.logFiles.push({
        name: f.name,
        size: f.size,
        content: e.target!.result as string,
        projectId: state.activeProjectId,
      })
      appendLog('info', '📋 Log: ' + f.name, 'fe')
    }
    reader.readAsText(f)
  })
}

export function updateFileTree(): void {
  const el = document.getElementById('file-tree')!
  const tb = document.getElementById('tb-files')!
  if (!state.files.length) {
    el.innerHTML = ''
    tb.style.display = 'none'
    return
  }
  tb.textContent = String(state.files.length)
  tb.style.display = ''
  el.innerHTML = state.files
    .map((f) => {
      const n = f.issues.length
      const cls = n ? 'has-err' : f.analyzed ? 'analyzed' : ''
      const badge = n
        ? `<span class="fn-badge fn-err">${n}</span>`
        : f.analyzed
          ? `<span class="fn-badge fn-ok">✓</span>`
          : `<span class="fn-badge fn-pending">—</span>`
      const icons: Record<string, string> = {
        '.py': '🐍',
        '.js': '🟨',
        '.ts': '🔷',
        '.json': '📋',
        '.yaml': '⚙️',
        '.html': '🌐',
        '.css': '🎨',
        '.go': '🐹',
        '.sh': '💻',
        '.java': '☕',
        '.txt': '📄',
        '.log': '📜',
      }
      return `<div class="file-node ${cls}${f === state.currentFile ? ' active' : ''}" data-id="${f.id}">
      <span style="font-size:13px">${icons[f.ext] ?? '📄'}</span>
      <span class="fn-name" title="${f.name}">${f.name}</span>
      ${badge}
      <button class="btn btn-danger btn-sm" style="padding:2px 4px" data-remove="${f.id}">✕</button>
    </div>`
    })
    .join('')
  el.onclick = (e: MouseEvent) => {
    const target = e.target as HTMLElement
    const removeId = target.dataset['remove']
    const nodeEl = target.closest<HTMLElement>('.file-node')
    if (removeId) {
      e.stopPropagation()
      removeFile(removeId)
    } else if (nodeEl?.dataset['id']) selectFile(nodeEl.dataset['id'])
  }
}

function removeFile(id: string): void {
  state.files = state.files.filter((f) => f.id !== id)
  if (state.currentFile?.id === id) state.currentFile = null
  explorerRemoveFile(id)
  updateFileTree()
  updateSelectors()
  updateStats()
}

export function updateSelectors(): void {
  const opts =
    '<option value="">— Selecciona —</option>' +
    state.files.map((f) => `<option value="${f.id}">${f.name}</option>`).join('')
  ;['file-sel', 'diff-a', 'diff-b', 'diag-file-sel', 'ml-file-sel'].forEach((id) => {
    const el = document.getElementById(id) as HTMLSelectElement | null
    if (el) el.innerHTML = opts
  })
}

export function selectFile(id: string): void {
  const f = state.files.find((x) => x.id === id)
  if (!f) return
  state.currentFile = f
  loadFileInEditor(f)
  renderFileAnalysis(f)
  rpTab('analysis')
  updateFileTree()
  saveSession(f.name)
  ;(document.getElementById('file-sel') as HTMLSelectElement).value = id
  ;(document.getElementById('diag-file-sel') as HTMLSelectElement).value = id
  ;(document.getElementById('ml-file-sel') as HTMLSelectElement).value = id
  switchTab('editor')
}

// ══════════════════════════════════════════
//  URL MANAGEMENT
// ══════════════════════════════════════════
export function addURL(url?: string): void {
  const inp = document.getElementById('url-main') as HTMLInputElement
  const v = (url ?? inp.value).trim()
  if (!v) return
  if (!v.startsWith('http')) {
    toast('❌ URL debe comenzar con http://', 'err')
    return
  }
  if (state.urls.includes(v)) {
    toast('Ya existe', 'warn')
    return
  }
  state.urls.push(v)
  state.results.apis.push({ url: v, status: 'unknown', code: null, ms: null, error: null, ts: null, history: [] })
  inp.value = ''
  renderURLList()
  appendLog('info', '🌐 ' + v, 'fe')
}

function renderURLList(): void {
  const el = document.getElementById('url-list')!
  if (!state.urls.length) {
    el.innerHTML = '<div class="empty" style="padding:10px;font-size:.68rem">Sin URLs</div>'
    return
  }
  el.innerHTML = state.urls
    .map((url) => {
      const r = state.results.apis.find((a) => a.url === url)
      const c =
        ({ ok: 'var(--ok)', warning: 'var(--warn)', down: 'var(--err)' } as Record<string, string>)[r?.status ?? ''] ??
        'var(--muted)'
      return `<div class="url-item">
      <div class="ui-dot" style="background:${c}"></div>
      <span class="ui-url" title="${url}">${url}</span>
      ${r?.ms ? `<span class="ui-ms">${r.ms}ms</span>` : ''}
      ${r?.code ? `<span class="ui-code">HTTP ${r.code}</span>` : ''}
      <button class="btn btn-danger btn-sm" style="padding:2px 4px" data-remove-url="${url}">✕</button>
    </div>`
    })
    .join('')
  el.onclick = (e: MouseEvent) => {
    const url = (e.target as HTMLElement).dataset['removeUrl']
    if (url) {
      state.urls = state.urls.filter((u) => u !== url)
      state.results.apis = state.results.apis.filter((a) => a.url !== url)
      renderURLList()
    }
  }
}

// ══════════════════════════════════════════
//  MAIN ANALYSIS
// ══════════════════════════════════════════
export async function runAll(): Promise<void> {
  if (state.running) return
  state.running = true
  document.getElementById('run-btn')!.setAttribute('disabled', '')
  state.results.issues = []
  ;['api', 'upload', 'analyze', 'logs', 'report'].forEach((id) => {
    state.steps[id] = 'idle'
  })
  renderFlow()
  setProgress(5)
  const t0 = Date.now()
  appendLog('info', '━━━━━━━━━━━━━━━━━', 'fe')
  appendLog('info', '▶ Análisis iniciado', 'fe')

  setStep('api', 'run')
  await runAPIChecks()
  setStep('api', state.results.apis.some((a) => a.status === 'down') ? 'err' : 'ok')
  setProgress(20)

  setStep('upload', 'run')
  setStep('analyze', 'run')
  await analyzeAllFiles()
  setStep('upload', 'ok')
  setStep('analyze', state.results.issues.filter((i) => i.severity === 'error').length ? 'err' : 'ok')
  setProgress(65)

  setStep('logs', 'run')
  await analyzeAllLogs()
  setStep('logs', state.results.logErrors.length ? 'warn' : 'ok')
  setProgress(85)

  setStep('report', 'run')
  await delay(100)
  renderAllResults()
  setStep('report', 'ok')
  setProgress(100)
  setTimeout(() => setProgress(0), 700)

  const ms = Date.now() - t0
  const entry = {
    ts: nowStr(),
    issues: state.results.issues.length,
    apiOk: state.results.apis.filter((a) => a.status === 'ok').length,
    ms,
  }
  state.history.push(entry)
  updateHistChart()
  updateRunMeta(entry)
  updateGlobalPill()
  updateStats()
  updateBadges()
  appendLog('info', `✔ Completo en ${ms}ms — ${overallStatus().toUpperCase()}`, 'fe')
  state.running = false
  document.getElementById('run-btn')!.removeAttribute('disabled')
  toast(`✅ Listo — ${state.results.issues.length} problemas`, state.results.issues.length ? 'warn' : 'ok')
}

async function runAPIChecks(): Promise<void> {
  if (!state.urls.length) {
    appendLog('warn', 'Sin URLs', 'fe')
    return
  }
  try {
    const r = await api.checkUrls(state.urls)
    r.results.forEach((res) => {
      const idx = state.results.apis.findIndex((a) => a.url === res.url)
      if (idx >= 0) state.results.apis[idx] = res
    })
    appendLog('info', `📡 ${state.results.apis.length} endpoints verificados`, 'be')
  } catch {
    appendLog('warn', 'Backend no disponible — fetch del browser', 'fe')
    for (const url of state.urls) {
      const r = await api.browserPing(url)
      const idx = state.results.apis.findIndex((a) => a.url === url)
      if (idx >= 0) state.results.apis[idx] = { ...state.results.apis[idx], ...r }
    }
  }
  renderURLList()
  renderAPICards()
  renderRTChart()
}

async function analyzeAllFiles(): Promise<void> {
  state.results.issues = []

  if (!state.files.length) {
    // Sin archivos cargados a mano — si hay un proyecto activo, se analiza
    // directo del disco (el backend lee los archivos, no hace falta bajar el
    // contenido de cada uno al navegador). Ver /analyze/project?project_id=.
    if (state.activeProjectId && state.backendOk) {
      try {
        const res = await api.analyzeProjectById(state.activeProjectId)
        for (const [filename, r] of Object.entries(res.files)) {
          state.results.issues.push(...r.issues.map((i) => ({ ...i, file: filename })))
        }
        renderIssuesList()
        appendLog('ok', `🔍 Proyecto activo: ${state.results.issues.length} issue(s)`, 'be')
      } catch (e) {
        appendLog('err', `Error analizando proyecto activo: ${(e as Error).message}`, 'fe')
      }
    }
    return
  }

  if (state.backendOk) {
    // Un solo request para todo el proyecto — flake8/pylint corren una vez en
    // vez de un subprocess por archivo (antes: ~0.9s/archivo, ~15min con 1000
    // archivos; ahora: proyectado a segundos). Ver /analyze/project.
    try {
      const res = await api.analyzeProject(state.files.map((f) => ({ filename: f.name, content: f.content })))
      for (const f of state.files) {
        const r = res.files[f.name]
        if (!r) continue
        f.issues = r.issues
        f.metrics = {
          pylint_score: r.metrics?.pylint_score,
          complexity: r.complexity ?? [],
          mi: r.maintainability ?? undefined,
          raw: r.raw_stats,
          tools_used: r.tools_used ?? [],
        }
        f.analyzed = true
        state.results.issues.push(...f.issues.map((i) => ({ ...i, file: f.name })))
        appendLog('ok', `🔍 ${f.name}: ${f.issues.length} issue(s)`, 'be')
      }
    } catch (e) {
      appendLog('err', `Error analizando proyecto: ${(e as Error).message}`, 'fe')
    }
  } else {
    for (const f of state.files) {
      f.issues = clientAnalyze(f)
      f.analyzed = true
      state.results.issues.push(...f.issues.map((i) => ({ ...i, file: f.name })))
      appendLog('warn', `🔍 ${f.name}: análisis básico`, 'fe')
    }
  }
  updateFileTree()
}

function clientAnalyze(f: { content: string; ext: string }): import('../types').Issue[] {
  const issues: import('../types').Issue[] = []
  f.content.split('\n').forEach((raw, i) => {
    if (raw.length > 120)
      issues.push({
        tool: 'ast',
        line: i + 1,
        col: 121,
        severity: 'warning',
        code: 'E501',
        message: `Línea larga (${raw.length})`,
      })
    if (/debugger/.test(raw))
      issues.push({ tool: 'ast', line: i + 1, col: 0, severity: 'error', code: 'E001', message: 'debugger encontrado' })
    if (/(TODO|FIXME|HACK):/i.test(raw))
      issues.push({ tool: 'ast', line: i + 1, col: 0, severity: 'info', code: 'W001', message: 'Comentario pendiente' })
  })
  return issues
}

async function analyzeAllLogs(): Promise<void> {
  state.results.logErrors = []
  if (!state.logFiles.length) return
  try {
    if (state.backendOk) {
      const res = await api.analyzeLogs(state.logFiles.map((f) => ({ name: f.name, content: f.content })))
      state.results.logErrors = [
        ...(res.errors as import('../types').LogError[]),
        ...(res.warnings as import('../types').LogError[]),
      ]
      appendLog('ok', `📋 Logs: ${res.errors.length} errores`, 'be')
    }
  } catch (e) {
    appendLog('err', 'Error logs: ' + (e as Error).message, 'fe')
  }
}

export async function analyzeCurrentFile(): Promise<void> {
  if (!state.currentFile) {
    toast('Selecciona un archivo', 'warn')
    return
  }
  const f = state.currentFile
  const edVal = getEditorValue()
  if (edVal) f.content = edVal
  setProgress(30)
  try {
    if (state.backendOk) {
      const res = await api.analyzeCode(f.name, f.content)
      f.issues = res.issues
      f.metrics = {
        pylint_score: res.metrics?.pylint_score,
        complexity: res.complexity ?? [],
        mi: res.maintainability ?? undefined,
        raw: res.raw_stats,
        tools_used: res.tools_used ?? [],
      }
      f.analyzed = true
    } else {
      f.issues = clientAnalyze(f)
    }
    applyMarkers(f)
    renderFileAnalysis(f)
    updateFileTree()
    setProgress(100)
    setTimeout(() => setProgress(0), 500)
    toast(`🔍 ${f.name}: ${f.issues.length} issue(s)`, f.issues.length ? 'warn' : 'ok')
  } catch (e) {
    toast('Error: ' + (e as Error).message, 'err')
    setProgress(0)
  }
}

function renderAllResults(): void {
  renderAPICards()
  renderIssuesList()
  renderMetrics()
  renderComplexityBars()
  renderDistChart()
  renderRTChart()
}

// ══════════════════════════════════════════
//  AUTO ANALYSIS
// ══════════════════════════════════════════
export function toggleAuto(): void {
  state.autoOn = !state.autoOn
  const btn = document.getElementById('auto-btn')!
  if (state.autoOn) {
    btn.style.color = 'var(--ok)'
    btn.textContent = '⏹ Auto ON'
    state.autoTimer = setInterval(() => {
      if (!state.running) runAll()
    }, 30000)
    toast('⏱ Auto cada 30s', 'ok')
  } else {
    btn.style.color = ''
    btn.textContent = '⏱ Auto'
    if (state.autoTimer) clearInterval(state.autoTimer)
    toast('⏹ Auto OFF', 'warn')
  }
}

export function clearAll(): void {
  if (!confirm('¿Limpiar todo?')) return
  state.files = []
  state.logFiles = []
  state.urls = []
  state.results = { apis: [], issues: [], logErrors: [] }
  updateFileTree()
  updateSelectors()
  renderURLList()
  updateStats()
  const apiCards = document.getElementById('api-cards')
  if (apiCards) apiCards.innerHTML = ''
  const issuesList = document.getElementById('issues-list')
  if (issuesList) issuesList.innerHTML = ''
  clearSession()
  toast('🗑 Limpiado', 'warn')
}

// ══════════════════════════════════════════
//  ML/DL
// ══════════════════════════════════════════
export async function runMLAnalysis(): Promise<void> {
  const selId = (document.getElementById('ml-file-sel') as HTMLSelectElement).value
  const f = state.files.find((x) => x.id === selId)
  if (!f) {
    toast('Selecciona un archivo .py', 'warn')
    return
  }
  if (f.ext !== '.py') {
    toast('Solo archivos .py', 'warn')
    return
  }
  const el = document.getElementById('ml-content')!
  el.innerHTML = '<div class="empty"><span class="empty-icon">⚙️</span>Analizando ML/DL...</div>'
  try {
    const data = state.backendOk ? await api.analyzeML(f.name, f.content) : clientMLAnalysis(f)
    renderMLResults(data, el)
    document.getElementById('tb-ml')!.style.display = ''
    const badge = document.getElementById('ml-score-badge')!
    badge.style.display = ''
    const sc = data.score ?? 0
    const c = sc >= 80 ? 'var(--ok)' : sc >= 50 ? 'var(--warn)' : 'var(--err)'
    badge.innerHTML = `<span style="color:${c};font-weight:700">Score: ${sc}/100</span>`
    toast(`🤖 ML — score ${sc}/100`, sc >= 60 ? 'ok' : 'warn')
    appendLog('ok', `🤖 ML: ${f.name} — score ${sc}`, 'be')
  } catch (e) {
    el.innerHTML = `<div class="empty"><span class="empty-icon">❌</span>Error: ${(e as Error).message}</div>`
    toast('Error ML: ' + (e as Error).message, 'err')
  }
}

// ══════════════════════════════════════════
//  DIAGRAM
// ══════════════════════════════════════════
const PROJECT_GRAPH_TYPES = new Set(['import', 'call', 'circular', 'heatmap', 'centrality'])

export async function generateDiagram(): Promise<void> {
  const diagType = (document.getElementById('diag-type') as HTMLSelectElement).value

  if (PROJECT_GRAPH_TYPES.has(diagType)) {
    await generateWholeProjectDiagram(diagType)
    return
  }

  const selId = (document.getElementById('diag-file-sel') as HTMLSelectElement).value
  const f = state.files.find((x) => x.id === selId)
  if (!f) {
    toast('Selecciona un archivo', 'warn')
    return
  }
  const outEl = document.getElementById('mermaid-output')!
  const statusEl = document.getElementById('diag-status')!
  statusEl.textContent = 'Generando...'
  outEl.innerHTML = '<div class="empty"><span class="empty-icon">⚙️</span>Analizando...</div>'
  try {
    let code = ''
    if (state.backendOk) {
      const res = await api.generateDiagram(f.name, f.content, diagType)
      code = res.mermaid
    } else {
      code = generateMermaidFallback(f.name, f.content, diagType)
    }
    state.currentMermaid = code
    document.getElementById('mermaid-raw-code')!.textContent = code
    document.getElementById('mermaid-code-container')!.style.display = ''
    const svg = await renderDiagram(code)
    outEl.innerHTML = svg
    const svgEl = outEl.querySelector('svg')
    if (svgEl) {
      svgEl.style.maxWidth = '100%'
      svgEl.style.height = 'auto'
    }
    statusEl.textContent = `✅ ${f.name}`
    statusEl.style.color = 'var(--ok)'
    document.getElementById('tb-diagram')!.style.display = ''
    toast('🔀 Diagrama generado', 'ok')
  } catch (e) {
    outEl.innerHTML = `<div class="empty"><span class="empty-icon">⚠️</span>Error: ${(e as Error).message}</div>`
    statusEl.textContent = 'Error'
    statusEl.style.color = 'var(--err)'
  }
}

/**
 * Import Graph / Call Graph / Circular Deps / Heatmap — sobre el proyecto
 * activo si hay uno (panels/graph.ts:generateProjectGraph, lee del disco en
 * el server), o sobre state.files si no (generateCodeGraph, Fase 1). Reusa
 * el mismo pipeline de render Mermaid que el flowchart de archivo único.
 *
 * No incluye todavía el Force Graph interactivo ni el árbol de directorios
 * con complejidad por archivo (renderForceGraph/renderDirTree en graph.ts) —
 * quedan para una siguiente pasada, esto conecta el Tree View / Mermaid.
 */
async function generateWholeProjectDiagram(graphType: string): Promise<void> {
  const outEl = document.getElementById('mermaid-output')!
  const statusEl = document.getElementById('diag-status')!

  const onMermaid = async (code: string) => {
    state.currentMermaid = code
    document.getElementById('mermaid-raw-code')!.textContent = code
    document.getElementById('mermaid-code-container')!.style.display = ''
    const svg = await renderDiagram(code)
    outEl.innerHTML = svg
    const svgEl = outEl.querySelector('svg')
    if (svgEl) {
      svgEl.style.maxWidth = '100%'
      svgEl.style.height = 'auto'
    }
  }
  const onStatus = (msg: string, ok: boolean) => {
    statusEl.textContent = msg
    statusEl.style.color = ok ? 'var(--ok)' : 'var(--err)'
  }

  outEl.innerHTML = '<div class="empty"><span class="empty-icon">⚙️</span>Analizando proyecto...</div>'

  if (state.activeProjectId) {
    await generateProjectGraph(
      state.activeProjectId,
      graphType,
      (code) => void onMermaid(code),
      () => {}, // Force Graph — pendiente de UI, ver docstring
      () => {}, // Dir Tree — pendiente de UI, ver docstring
      onStatus,
    )
  } else {
    await generateCodeGraph(
      graphType,
      (code) => void onMermaid(code),
      () => {}, // Force Graph — pendiente de UI
      onStatus,
    )
  }
}

function generateMermaidFallback(name: string, content: string, _type: string): string {
  const funcs: string[] = []
  content.split('\n').forEach((l) => {
    const m = l.match(/^def\s+(\w+)/) ?? l.match(/function\s+(\w+)/)
    if (m) funcs.push(m[1])
  })
  if (!funcs.length) return `flowchart TD\n    A[📄 ${name}]\n    B[Sin funciones]\n    A --> B`
  let code = `flowchart TD\n    START([🚀 ${name}])\n`
  funcs.slice(0, 10).forEach((fn, i) => {
    code += `    F${i}["⚙️ ${fn}()"]\n`
  })
  code += `    END([🏁])\n    START --> F0\n`
  for (let i = 0; i < Math.min(funcs.length, 10) - 1; i++) code += `    F${i} --> F${i + 1}\n`
  code += `    F${Math.min(funcs.length - 1, 9)} --> END\n`
  return code
}

// ══════════════════════════════════════════
//  DIFF
// ══════════════════════════════════════════
export async function runDiff(): Promise<void> {
  const { createPatch } = await import('diff')
  const a = state.files.find((f) => f.id === (document.getElementById('diff-a') as HTMLSelectElement).value)
  const b = state.files.find((f) => f.id === (document.getElementById('diff-b') as HTMLSelectElement).value)
  if (!a || !b) {
    toast('Selecciona dos archivos', 'warn')
    return
  }
  const patch = createPatch(a.name, a.content, b.content)
  const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  const html = patch
    .split('\n')
    .map((l) => {
      if (l.startsWith('+++')) return `<span style="color:var(--ok);font-weight:bold">${esc(l)}</span>`
      if (l.startsWith('---')) return `<span style="color:var(--err);font-weight:bold">${esc(l)}</span>`
      if (l.startsWith('+'))
        return `<span style="background:rgba(0,245,160,.08);border-left:2px solid var(--ok);padding-left:6px">${esc(l)}</span>`
      if (l.startsWith('-'))
        return `<span style="background:rgba(255,51,102,.08);border-left:2px solid var(--err);padding-left:6px">${esc(l)}</span>`
      if (l.startsWith('@@')) return `<span style="color:var(--purple)">${esc(l)}</span>`
      return `<span style="color:var(--muted)">${esc(l)}</span>`
    })
    .join('\n')
  document.getElementById('diff-out')!.innerHTML = html || '<span style="color:var(--ok)">✅ Archivos idénticos</span>'
}

// ══════════════════════════════════════════
//  EXPORT
// ══════════════════════════════════════════
export async function exportZip(): Promise<void> {
  const { default: JSZip } = await import('jszip')
  const report = {
    generated: new Date().toISOString(),
    status: overallStatus(),
    backend: state.backendOk,
    summary: {
      apis: state.results.apis.length,
      apisOk: state.results.apis.filter((a) => a.status === 'ok').length,
      files: state.files.length,
      issues: state.results.issues.length,
    },
    apis: state.results.apis,
    issues: state.results.issues,
    logErrors: state.results.logErrors,
    fileMetrics: state.files.map((f) => ({ name: f.name, issues: f.issues.length, metrics: f.metrics })),
  }
  try {
    const zip = new JSZip()
    zip.file('report.json', JSON.stringify(report, null, 2))
    if (state.currentMermaid) zip.file('diagram.mmd', state.currentMermaid)
    const blob = await zip.generateAsync({ type: 'blob' })
    const a = document.createElement('a')
    a.href = URL.createObjectURL(blob)
    a.download = `sythrall-${Date.now()}.zip`
    a.click()
    toast('⬇ ZIP exportado', 'ok')
  } catch {
    const blob = new Blob([JSON.stringify(report, null, 2)], { type: 'application/json' })
    const a = document.createElement('a')
    a.href = URL.createObjectURL(blob)
    a.download = `sythrall-${Date.now()}.json`
    a.click()
    toast('⬇ JSON exportado', 'ok')
  }
}

// ══════════════════════════════════════════
//  INIT APP
// ══════════════════════════════════════════
export function initApp(): void {
  if (window.innerWidth < 900) {
    localStorage.removeItem('panel-size-sidebar')
    localStorage.removeItem('panel-size-right-panel')
  } else {
    ;['sidebar', 'right-panel'].forEach((id) => {
      const saved = localStorage.getItem(`panel-size-${id}`)
      if (saved && (Number(saved) < 100 || Number(saved) > 600)) {
        localStorage.removeItem(`panel-size-${id}`)
      }
    })
  }

  try {
    initEditor()
  } catch (e) {
    console.error('Editor init failed:', e)
  }
  initExplorer({ onFileOpen: (f) => selectFile(f.id) })
  initMermaid()
  setTimeout(() => {
    try {
      initCharts()
    } catch (e) {
      console.warn('Charts init error:', e)
    }
  }, 100)

  const sidebar = document.getElementById('sidebar') as HTMLElement
  const sideHandle = document.getElementById('sidebar-resize') as HTMLElement
  const rightPanel = document.getElementById('right-panel') as HTMLElement
  const rpHandle = document.getElementById('rp-resize') as HTMLElement

  if (sidebar && sideHandle) {
    createResizer(sidebar, sideHandle, { minSize: 200, maxSize: 480, direction: 'horizontal', side: 'left' })
    const collapseBtn = document.getElementById('sidebar-collapse') as HTMLElement
    if (collapseBtn) createCollapseToggle(sidebar, collapseBtn, 'left', 260)
  }

  if (rightPanel && rpHandle) {
    createResizer(rightPanel, rpHandle, { minSize: 260, maxSize: 540, direction: 'horizontal', side: 'right' })
    const rpCollapseBtn = document.getElementById('rp-collapse') as HTMLElement
    if (rpCollapseBtn) createCollapseToggle(rightPanel, rpCollapseBtn, 'right', 280)
  }

  renderFlow()
  renderURLList()

  // Pre-cargar localhost:8000 (FastAPI)
  state.urls.push('http://localhost:8000')
  state.results.apis.push({
    url: 'http://localhost:8000',
    status: 'unknown',
    code: null,
    ms: null,
    error: null,
    ts: null,
    history: [],
  })
  renderURLList()

  // Wiring de eventos (tabs, drag&drop, inputs de archivo, etc.) vive en
  // events.ts (wireAllEvents, llamado desde main.ts) — no duplicar aquí.

  appendLog('info', '🛰 Sythrall listo', 'fe')
  checkBackend().then(() => void tryRestoreSession())
  setInterval(() => {
    if (state.backendOk && !state.running) checkBackend()
  }, 60000)
}
