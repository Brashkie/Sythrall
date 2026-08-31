// ══════════════════════════════════════════
//  Sythrall — Event Wiring
// ══════════════════════════════════════════
// src/components/events.ts

import { api } from '../api/client'
// biome-ignore lint/correctness/noUnusedImports: filterIssues is used below in the issue-search listener (false positive)
import { filterAPIs, filterIssues, renderIssuesList, setIssueFilter } from '../panels/apis'
import { state } from '../store/state'
import type { TabId } from '../types'
import { appendLog, debounce } from '../utils/helpers'
import { icon } from '../utils/icons'
import { toggleTheme } from '../utils/theme'
import type { DiagView } from './app'
import {
  addURL,
  analyzeCurrentFile,
  clearAll,
  exportZip,
  generateDiagram,
  handleCodeFiles,
  handleLogFiles,
  rpTab,
  runAll,
  runDiff,
  runMLAnalysis,
  switchDiagView,
  switchTab,
  toggleAuto,
} from './app'
import { getEditorValue } from './editor'
import { toggleSearch } from './explorer'
import { fitDiagram, initDiagramZoom, resetZoom, zoomIn, zoomOut } from './mermaid'

export function wireAllEvents(): void {
  document.getElementById('exp-search-btn')?.addEventListener('click', toggleSearch)

  // ── Tabs
  document.querySelectorAll<HTMLElement>('.tab[data-tab]').forEach((el) => {
    el.addEventListener('click', () => switchTab(el.dataset['tab'] as TabId))
  })

  // ── Indicador de proyecto activo (nav-rail) — no es un ".tab" (estilo
  // propio, no debe heredar el hover/active de las pestañas) así que se
  // wirea aparte en vez de sumarlo al selector de arriba.
  document.getElementById('nav-rail-project')?.addEventListener('click', () => switchTab('upload'))

  // ── Colapsar/expandir el nav-rail — colapsado significa DESAPARECE del
  // todo (solo el botón de abajo queda visible, ver .nav-rail.collapsed en
  // main.css), no una versión angosta con íconos como antes — pedido
  // explícito del usuario, con la sidebar de Claude como referencia: al
  // pasar el mouse por encima (sin clickear) aparece como preview flotante
  // ("fantasma") sin comprometerse a expandir de verdad; recién clickear el
  // botón cambia la preferencia persistida. No reusa createResizer/
  // createCollapseToggle de utils/resizer.ts (esas colapsan a 0px liso, sin
  // ningún handle para volver — acá siempre queda el botón accesible).
  const navRail = document.getElementById('nav-rail')
  const navRailCollapseBtn = document.getElementById('nav-rail-collapse-btn')
  if (navRail && navRailCollapseBtn) {
    const NAV_RAIL_COLLAPSED_KEY = 'sythrall:nav-rail-collapsed'
    let collapsed = false
    let peeking = false
    let leaveTimer: ReturnType<typeof setTimeout> | null = null

    const applyCollapsed = (next: boolean) => {
      collapsed = next
      peeking = false
      navRail.classList.remove('peeking')
      navRail.classList.toggle('collapsed', collapsed)
      navRailCollapseBtn.title = collapsed ? 'Expandir sidebar' : 'Colapsar sidebar'
    }
    try {
      collapsed = localStorage.getItem(NAV_RAIL_COLLAPSED_KEY) === '1'
    } catch {
      /* localStorage puede no estar disponible */
    }
    applyCollapsed(collapsed)

    navRailCollapseBtn.addEventListener('click', () => {
      applyCollapsed(!collapsed)
      try {
        localStorage.setItem(NAV_RAIL_COLLAPSED_KEY, collapsed ? '1' : '0')
      } catch {
        /* localStorage puede no estar disponible */
      }
    })

    // Preview "fantasma" al pasar el mouse — solo tiene sentido si la
    // preferencia real es colapsado (si ya está expandido, hover no hace
    // nada). Saca `.collapsed` y pone `.peeking` en vez de sumar reglas CSS
    // que dupliquen "cómo se ve expandido": sin `.collapsed`, el rail vuelve
    // solo a verse exactamente como expandido — `.peeking` solo agrega el
    // posicionamiento flotante (position:absolute, sombra) por encima del
    // contenido, sin empujarlo ni correr el layout de `.app-main`.
    navRail.addEventListener('mouseenter', () => {
      if (!collapsed) return
      if (leaveTimer) {
        clearTimeout(leaveTimer)
        leaveTimer = null
      }
      peeking = true
      navRail.classList.remove('collapsed')
      navRail.classList.add('peeking')
    })
    navRail.addEventListener('mouseleave', () => {
      if (!peeking) return
      // Pequeño delay para que mover el mouse hacia un ítem puntual no cierre
      // el preview a mitad de camino por un instante fuera del área.
      leaveTimer = setTimeout(() => {
        peeking = false
        navRail.classList.remove('peeking')
        navRail.classList.add('collapsed')
      }, 200)
    })
  }

  // ── Right panel tabs
  document.querySelectorAll<HTMLElement>('.rp-tab[data-rptab]').forEach((el) => {
    el.addEventListener('click', () => rpTab(el.dataset['rptab'] as 'flow' | 'analysis' | 'server' | 'problems'))
  })

  // ── URL input
  const urlInput = document.getElementById('url-main') as HTMLInputElement
  urlInput?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') addURL()
  })
  document.getElementById('btn-add-url')?.addEventListener('click', () => addURL())

  // ── Quick URL buttons
  document.querySelectorAll<HTMLElement>('[data-quick-url]').forEach((el) => {
    el.addEventListener('click', () => addURL(el.dataset['quickUrl']))
  })

  // ── File inputs
  const fiCode = document.getElementById('fi-code') as HTMLInputElement
  const fiLog = document.getElementById('fi-log') as HTMLInputElement
  const fiFolder = document.getElementById('fi-folder') as HTMLInputElement
  document.getElementById('btn-add-code')?.addEventListener('click', () => fiCode.click())
  document.getElementById('btn-add-log')?.addEventListener('click', () => fiLog.click())
  document.getElementById('btn-add-folder')?.addEventListener('click', () => fiFolder.click())
  document.getElementById('dz')?.addEventListener('click', () => fiCode.click())
  fiCode?.addEventListener('change', () => {
    handleCodeFiles(fiCode.files)
    fiCode.value = ''
  })
  fiLog?.addEventListener('change', () => {
    handleLogFiles(fiLog.files)
    fiLog.value = ''
  })
  fiFolder?.addEventListener('change', async () => {
    const { handleFolderPick } = await import('./file-browser')
    handleFolderPick(fiFolder.files)
    fiFolder.value = ''
  })

  // ── Main run button
  document.getElementById('run-btn')?.addEventListener('click', runAll)

  // ── Auto analysis
  document.getElementById('auto-btn')?.addEventListener('click', toggleAuto)

  // ── Terminal (sidecar Rust) — import dinámico: xterm.js (~330kB) no debe
  // sumarse al bundle inicial, solo se carga si el usuario abre la terminal.
  document.getElementById('terminal-toggle-btn')?.addEventListener('click', async () => {
    const { toggleTerminalPanel } = await import('./terminal')
    toggleTerminalPanel()
  })
  document.getElementById('terminal-close-btn')?.addEventListener('click', async () => {
    const { closeTerminalPanel } = await import('./terminal')
    closeTerminalPanel()
  })
  document.getElementById('terminal-new-btn')?.addEventListener('click', async () => {
    const { newTerminalSession } = await import('./terminal')
    newTerminalSession()
  })

  // ── Tema
  document.getElementById('theme-toggle-btn')?.addEventListener('click', toggleTheme)

  // ── Export
  document.getElementById('btn-export')?.addEventListener('click', exportZip)

  // ── Clear
  document.getElementById('btn-clear')?.addEventListener('click', clearAll)

  // ── Editor buttons
  document.getElementById('btn-analyze-file')?.addEventListener('click', analyzeCurrentFile)
  document.getElementById('btn-copy-editor')?.addEventListener('click', () => {
    const val = getEditorValue()
    if (val) {
      navigator.clipboard.writeText(val)
    }
  })
  document.getElementById('btn-diagram-file')?.addEventListener('click', () => {
    if (state.currentFile) {
      const sel = document.getElementById('diag-file-sel') as HTMLSelectElement
      if (sel) sel.value = state.currentFile.id
      switchTab('diagram')
      generateDiagram()
    }
  })

  // ── Listen for editor content changes
  document.addEventListener('editor:change', (e: Event) => {
    const content = (e as CustomEvent<string>).detail
    if (state.currentFile) state.currentFile.content = content
  })

  // ── API search
  document
    .getElementById('api-search')
    ?.addEventListener('input', (e) => filterAPIs((e.target as HTMLInputElement).value))

  // ── Re-check all APIs
  document.getElementById('btn-recheck-all')?.addEventListener('click', async () => {
    if (!state.urls.length) return
    try {
      const r = await api.checkUrls(state.urls)
      r.results.forEach((res) => {
        const idx = state.results.apis.findIndex((a) => a.url === res.url)
        if (idx >= 0) state.results.apis[idx] = res
      })
      const { renderAPICards } = await import('../panels/apis')
      renderAPICards()
    } catch (e) {
      appendLog('err', 'Error: ' + (e as Error).message, 'fe')
    }
  })

  // ── Issue filters
  document.querySelectorAll<HTMLElement>('[data-sev]').forEach((el) => {
    el.addEventListener('click', () => setIssueFilter(el.dataset['sev'] ?? 'all'))
  })
  // Debounced — filterIssues reconstruye la lista completa de hallazgos en
  // cada llamada, no vale la pena hacerlo en cada tecla.
  const issueSearchInput = document.getElementById('issue-search') as HTMLInputElement | null
  const filterIssuesDebounced = debounce(() => filterIssues(issueSearchInput?.value ?? ''), 200)
  issueSearchInput?.addEventListener('input', filterIssuesDebounced)
  document
    .getElementById('tool-filter')
    ?.addEventListener('change', (e) => setIssueFilter(null, (e.target as HTMLSelectElement).value))

  // ── Diagram generate
  document.getElementById('btn-gen-diagram')?.addEventListener('click', generateDiagram)

  // ── Diagram export SVG
  document.getElementById('btn-export-svg')?.addEventListener('click', () => {
    const svgEl = document.querySelector<SVGElement>('#mermaid-output svg')
    if (!svgEl) return
    const blob = new Blob([svgEl.outerHTML], { type: 'image/svg+xml' })
    const a = document.createElement('a')
    a.href = URL.createObjectURL(blob)
    a.download = `diagram-${Date.now()}.svg`
    a.click()
  })

  // ── Diagram copy mermaid code
  document.getElementById('btn-copy-mermaid')?.addEventListener('click', () => {
    if (state.currentMermaid) navigator.clipboard.writeText(state.currentMermaid)
  })

  // ── Zoom controls ──────────────────────────────
  document.getElementById('btn-zoom-in')?.addEventListener('click', () => zoomIn())
  document.getElementById('btn-zoom-out')?.addEventListener('click', () => zoomOut())
  document.getElementById('btn-zoom-reset')?.addEventListener('click', () => resetZoom(true))
  document.getElementById('btn-zoom-fit')?.addEventListener('click', () => fitDiagram(true))

  // ── Diagram: toggle Árbol / Interactivo / Directorio (mismo resultado,
  // 3 vistas — ver switchDiagView en app.ts)
  document.getElementById('diag-view-toggle')?.addEventListener('click', (e) => {
    const btn = (e.target as HTMLElement).closest<HTMLButtonElement>('[data-diag-view]')
    if (btn) switchDiagView(btn.dataset.diagView as DiagView)
  })

  // Inicializar zoom engine (attach wheel + drag listeners al viewport)
  initDiagramZoom()
  // ──────────────────────────────────────────────

  // ── ML/DL
  document.getElementById('btn-run-ml')?.addEventListener('click', runMLAnalysis)

  // ── ML diagram button (delegated)
  document.getElementById('ml-content')?.addEventListener('click', async (e) => {
    const btn = (e.target as HTMLElement).closest<HTMLElement>('[data-ml-diagram]')
    if (!btn) return
    const code = btn.dataset['mlDiagram']
    if (!code) return
    state.currentMermaid = code
    switchTab('diagram')
    const { renderDiagram, resetDiagramView } = await import('./mermaid')
    const outEl = document.getElementById('mermaid-output')!
    try {
      const svg = await renderDiagram(code)
      outEl.innerHTML = svg
      const svgEl = outEl.querySelector('svg')
      if (svgEl) {
        svgEl.style.maxWidth = 'none'
        svgEl.style.height = 'auto'
      }
      document.getElementById('mermaid-raw-code')!.textContent = code
      document.getElementById('mermaid-code-container')!.style.display = ''
      resetDiagramView(true)
    } catch (err) {
      outEl.innerHTML = `<div class="empty">Error: ${(err as Error).message}</div>`
    }
  })

  // ── Diff
  document.getElementById('btn-diff')?.addEventListener('click', runDiff)

  // ── Logs
  document.getElementById('btn-fetch-logs')?.addEventListener('click', async () => {
    try {
      const d = await api.getLogs()
      ;(d.logs as Array<{ level: string; msg: string }>).forEach((l) => {
        appendLog(l.level as 'ok' | 'err' | 'warn' | 'info', l.msg, 'be')
      })
    } catch {
      appendLog('err', 'Backend no disponible', 'fe')
    }
  })
  document.getElementById('btn-clear-log')?.addEventListener('click', () => {
    const el = document.getElementById('log-stream')
    if (el) el.innerHTML = ''
  })

  // ━━━ RESPONSIVE / MOBILE ━━━

  const overlay = document.getElementById('drawer-overlay')

  function closeAllDrawers() {
    document.getElementById('sidebar')?.classList.remove('drawer-open')
    document.getElementById('right-panel')?.classList.remove('drawer-open')
    overlay?.classList.remove('active')
    const fab = document.getElementById('rp-fab')
    if (fab) fab.innerHTML = icon('metrics', 18)
  }

  overlay?.addEventListener('click', () => closeAllDrawers())

  document.getElementById('mobile-toggle')?.addEventListener('click', (e) => {
    e.stopPropagation()
    const sidebar = document.getElementById('sidebar')!
    const isOpen = sidebar.classList.toggle('drawer-open')
    overlay?.classList.toggle('active', isOpen)
    if (isOpen) document.getElementById('right-panel')?.classList.remove('drawer-open')
  })

  document.getElementById('rp-fab')?.addEventListener('click', (e) => {
    e.stopPropagation()
    const rp = document.getElementById('right-panel')!
    const fab = e.currentTarget as HTMLElement
    const isOpen = rp.classList.toggle('drawer-open')
    overlay?.classList.toggle('active', isOpen)
    fab.innerHTML = isOpen ? icon('close', 18) : icon('metrics', 18)
    if (isOpen) document.getElementById('sidebar')?.classList.remove('drawer-open')
  })

  function updateResponsiveUI() {
    const w = window.innerWidth
    const fab = document.getElementById('rp-fab')
    const nav = document.getElementById('bottom-nav')
    if (fab) fab.style.display = w <= 900 && w > 480 ? 'flex' : 'none'
    if (nav) nav.style.display = w <= 480 ? 'flex' : 'none'
    if (w > 900) closeAllDrawers()
  }
  updateResponsiveUI()
  window.addEventListener('resize', updateResponsiveUI)

  // ── Bottom Navigation
  document.querySelectorAll<HTMLElement>('[data-bn-tab]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const tabId = btn.dataset['bnTab'] as import('../types').TabId
      if (!tabId) return
      switchTab(tabId)
      document.querySelectorAll<HTMLElement>('.bn-item').forEach((b) => {
        b.classList.toggle('active', b.dataset['bnTab'] === tabId)
      })
      closeBnMore()
    })
  })

  const bnMoreBtn = document.getElementById('bn-more-btn')
  const bnMoreMenu = document.getElementById('bn-more-menu')
  function closeBnMore() {
    if (bnMoreMenu) bnMoreMenu.style.display = 'none'
  }
  bnMoreBtn?.addEventListener('click', (e) => {
    e.stopPropagation()
    if (!bnMoreMenu) return
    bnMoreMenu.style.display = bnMoreMenu.style.display === 'block' ? 'none' : 'block'
  })
  document.addEventListener('click', (e: MouseEvent) => {
    const t = e.target as HTMLElement
    if (!t.closest('#bn-more-btn') && !t.closest('#bn-more-menu')) closeBnMore()
  })

  document.querySelectorAll<HTMLElement>('.tab[data-tab]').forEach((tab) => {
    tab.addEventListener('click', () => {
      const tabId = tab.dataset['tab']
      document.querySelectorAll<HTMLElement>('.bn-item[data-bn-tab]').forEach((btn) => {
        btn.classList.toggle('active', btn.dataset['bnTab'] === tabId)
      })
    })
  })

  // ── File selector change
  document.getElementById('file-sel')?.addEventListener('change', (e) => {
    const id = (e.target as HTMLSelectElement).value
    if (id) {
      import('./app').then(({ selectFile }) => selectFile(id))
    }
  })

  // ── Diag file selector
  document.getElementById('diag-file-sel')?.addEventListener('change', (e) => {
    const id = (e.target as HTMLSelectElement).value
    const sel = document.getElementById('file-sel') as HTMLSelectElement
    if (sel && id) sel.value = id
  })

  // ── Drag & drop
  const dz = document.getElementById('dz')!
  document.body.addEventListener('dragover', (e) => {
    e.preventDefault()
    dz.classList.add('over')
  })
  document.body.addEventListener('dragleave', () => dz.classList.remove('over'))
  document.body.addEventListener('drop', (e) => {
    e.preventDefault()
    dz.classList.remove('over')
    handleCodeFiles(e.dataTransfer?.files ?? null)
  })

  // ── Issue item click → open file
  document.getElementById('issues-list')?.addEventListener('click', async (e) => {
    const item = (e.target as HTMLElement).closest<HTMLElement>('.issue-item')
    const fileName = item?.dataset['file']
    if (!fileName) return
    const f = state.files.find((f) => f.name === fileName)
    if (f) {
      const { selectFile } = await import('./app')
      selectFile(f.id)
    }
  })
}
