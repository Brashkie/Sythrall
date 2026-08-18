// ══════════════════════════════════════════
//  Sythrall — Code Graph Visual
//  Integrado en el tab 🔀 Diagrama
//  Fase 1: archivos del sidebar
// ══════════════════════════════════════════

import type { GraphResult } from '../api/client'
import { api } from '../api/client'
import { state } from '../store/state'
import { appendLog, toast } from '../utils/helpers'

// ─── Tipos ────────────────────────────────────────────────────────────────────

interface GraphNode {
  id: string
  label: string
  language?: string
  functions?: number
  imports?: number
  in_cycle?: boolean
  big_o?: string
  cc?: number
  color?: string
  level?: string
  // heatmap
  name?: string
  file?: string
  file_short?: string
  cc_color?: string
  cc_level?: string
  bigo_color?: string
  bigo_level?: string
  loc?: number
}

// GraphResult se importa de client.ts
// Tipos adicionales para Fase 2
interface DirTreeNode {
  name: string
  type: 'file' | 'directory'
  path: string
  stats?: {
    functions: number
    avg_cc: number
    hot_paths: number
    language: string
    imports: number
    dead_code: number
  }
  children?: DirTreeNode[]
}

// ─── Estado del módulo ────────────────────────────────────────────────────────

let _currentType = 'import'
const _forceSimulation: unknown = null // d3 simulation

// ══════════════════════════════════════════
//  API pública — llamada desde diagram panel
// ══════════════════════════════════════════

/**
 * Genera el grafo seleccionado usando los archivos del sidebar.
 * Retorna el Mermaid code para el renderer existente.
 * También inicia el Force Graph si la vista es 'force'.
 */
export async function generateCodeGraph(
  graphType: string,
  onMermaid: (code: string) => void,
  onForce: (result: GraphResult) => void,
  onStatus: (msg: string, ok: boolean) => void,
): Promise<void> {
  if (!state.files.length) {
    toast('Carga archivos primero', 'warn')
    return
  }

  _currentType = graphType
  onStatus('Analizando...', true)

  const files = state.files.map((f) => ({ filename: f.name, content: f.content }))

  try {
    const result = (await api.analyzeGraph(files, graphType)) as GraphResult

    if (result.error) {
      onStatus(`Error: ${result.error}`, false)
      return
    }

    // Siempre pasar Mermaid al renderer existente (Tree View)
    onMermaid(result.mermaid || 'flowchart TD\n    A[Sin datos]')

    // También pasar datos para Force Graph
    onForce(result)

    // Status con info del resultado
    const summary = result.summary ?? {}
    let statusMsg = ''
    if (graphType === 'import') {
      statusMsg = `${summary.total_files ?? 0} archivos · ${summary.total_imports ?? 0} imports`
    } else if (graphType === 'call') {
      statusMsg = `${summary.total_functions ?? 0} funciones · ${summary.total_calls ?? 0} llamadas`
    } else if (graphType === 'circular') {
      const n = result.cycles?.length ?? 0
      statusMsg = n > 0 ? `🔴 ${n} ciclo(s) detectado(s)` : '✅ Sin dependencias circulares'
    } else if (graphType === 'heatmap') {
      statusMsg = `${summary.total_functions ?? 0} fn · ${summary.hot_paths ?? 0} hot paths`
    } else if (graphType === 'centrality') {
      const hubs = Array.isArray(summary.hubs) ? summary.hubs.length : 0
      statusMsg = hubs > 0 ? `🔥 ${hubs} hub(s) detectado(s)` : '✅ Sin hubs (grafo poco conectado)'
    }

    onStatus(statusMsg, !result.has_cycles)
    appendLog('ok', `🕸 Graph ${graphType}: ${statusMsg}`, 'be')
  } catch (e) {
    const msg = (e as Error).message
    onStatus(`Error: ${msg}`, false)
    toast('Error al generar grafo: ' + msg, 'err')
    appendLog('err', `graph error: ${msg}`, 'fe')
  }
}

// ══════════════════════════════════════════
//  FORCE GRAPH — SVG con D3-like physics
//  Sin dependencia externa — física simple
// ══════════════════════════════════════════

/**
 * Renderiza un Force Graph interactivo en el elemento SVG dado.
 * Nodos arrastrables, hover info, click para ir al archivo.
 */
export function renderForceGraph(
  container: HTMLElement,
  result: GraphResult,
  onNodeClick?: (nodeId: string) => void,
): void {
  container.innerHTML = ''

  const nodes =
    result.graph_type === 'heatmap'
      ? (result.functions ?? []).map((f) => ({
          id: `${f.file_short}::${f.name}`,
          label: f.name ?? '',
          color: f.cc_color ?? '#4a5880',
          size: Math.max(20, Math.min(50, (f.cc ?? 1) * 5)),
          meta: `${f.file_short} · CC=${f.cc} · ${f.big_o}`,
        }))
      : (result.nodes ?? []).map((n) => ({
          id: n.id,
          label: _shortLabel(n.label || n.id),
          color: n.in_cycle ? '#ff3366' : n.color ? n.color : _langColor(n.language ?? ''),
          size: Math.max(18, Math.min(40, 18 + (n.functions ?? 0) * 2)),
          meta: _nodeMetaText(n, result.graph_type),
        }))

  const edges = result.edges ?? []

  if (!nodes.length) {
    container.innerHTML = '<div class="empty"><span class="empty-icon">🕸</span>Sin nodos para mostrar</div>'
    return
  }

  const W = container.clientWidth || 600
  const H = container.clientHeight || 400

  // Asignar posiciones iniciales en círculo
  nodes.forEach((n, i) => {
    const angle = (2 * Math.PI * i) / nodes.length
    const r = Math.min(W, H) * 0.35
    ;(n as any).x = W / 2 + r * Math.cos(angle)
    ;(n as any).y = H / 2 + r * Math.sin(angle)
    ;(n as any).vx = 0
    ;(n as any).vy = 0
  })

  // Crear SVG
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg')
  svg.setAttribute('width', '100%')
  svg.setAttribute('height', '100%')
  svg.setAttribute('viewBox', `0 0 ${W} ${H}`)
  svg.style.cssText = 'cursor:grab;overflow:hidden;background:var(--s0)'

  // Defs: marcadores de flecha
  const defs = document.createElementNS('http://www.w3.org/2000/svg', 'defs')
  defs.innerHTML = `
    <marker id="arrow-normal" markerWidth="8" markerHeight="8" refX="6" refY="3" orient="auto">
      <path d="M0,0 L0,6 L8,3 z" fill="var(--muted)" opacity="0.6"/>
    </marker>
    <marker id="arrow-cycle" markerWidth="8" markerHeight="8" refX="6" refY="3" orient="auto">
      <path d="M0,0 L0,6 L8,3 z" fill="#ff3366"/>
    </marker>
  `
  svg.appendChild(defs)

  // Grupo principal (para pan/zoom)
  const g = document.createElementNS('http://www.w3.org/2000/svg', 'g')
  svg.appendChild(g)

  // Edges
  const edgeGroup = document.createElementNS('http://www.w3.org/2000/svg', 'g')
  g.appendChild(edgeGroup)

  const edgeEls: SVGLineElement[] = []
  for (const e of edges) {
    const line = document.createElementNS('http://www.w3.org/2000/svg', 'line')
    line.setAttribute('stroke', e.is_cycle ? '#ff3366' : 'var(--b2)')
    line.setAttribute('stroke-width', e.is_cycle ? '2' : '1.5')
    line.setAttribute('stroke-opacity', '0.7')
    line.setAttribute('marker-end', e.is_cycle ? 'url(#arrow-cycle)' : 'url(#arrow-normal)')
    edgeGroup.appendChild(line)
    edgeEls.push(line)
  }

  // Nodo group
  const nodeGroup = document.createElementNS('http://www.w3.org/2000/svg', 'g')
  g.appendChild(nodeGroup)

  // Tooltip
  const tooltip = document.createElement('div')
  tooltip.style.cssText = `
    position:absolute;background:var(--s2);border:1px solid var(--b1);border-radius:7px;
    padding:7px 10px;font-family:var(--mono);font-size:.68rem;color:var(--txt);
    pointer-events:none;display:none;z-index:100;max-width:200px;line-height:1.5;
  `
  container.style.position = 'relative'
  container.appendChild(tooltip)

  // Crear elementos de nodo
  const nodeMap: Record<string, any> = {}
  const nodeEls: Array<{ g: SVGGElement; circle: SVGCircleElement; text: SVGTextElement; data: any }> = []

  for (const n of nodes) {
    const nd = n as any
    nodeMap[nd.id] = nd

    const ng = document.createElementNS('http://www.w3.org/2000/svg', 'g')
    ng.style.cursor = 'pointer'

    const circle = document.createElementNS('http://www.w3.org/2000/svg', 'circle')
    circle.setAttribute('r', String(nd.size / 2))
    circle.setAttribute('fill', nd.color + '30')
    circle.setAttribute('stroke', nd.color)
    circle.setAttribute('stroke-width', '2')

    const text = document.createElementNS('http://www.w3.org/2000/svg', 'text')
    text.setAttribute('text-anchor', 'middle')
    text.setAttribute('dominant-baseline', 'middle')
    text.setAttribute('font-size', '10')
    text.setAttribute('fill', 'var(--txt)')
    text.setAttribute('pointer-events', 'none')
    text.textContent = nd.label.length > 12 ? nd.label.slice(0, 11) + '…' : nd.label

    ng.appendChild(circle)
    ng.appendChild(text)
    nodeGroup.appendChild(ng)
    nodeEls.push({ g: ng, circle, text, data: nd })

    // Hover → tooltip
    ng.addEventListener('mouseenter', (_ev: MouseEvent) => {
      tooltip.innerHTML = `<b>${nd.label}</b><br>${nd.meta ?? ''}`
      tooltip.style.display = 'block'
      circle.setAttribute('stroke-width', '3')
    })
    ng.addEventListener('mouseleave', () => {
      tooltip.style.display = 'none'
      circle.setAttribute('stroke-width', '2')
    })
    ng.addEventListener('mousemove', (ev: MouseEvent) => {
      const rect = container.getBoundingClientRect()
      tooltip.style.left = ev.clientX - rect.left + 12 + 'px'
      tooltip.style.top = ev.clientY - rect.top - 10 + 'px'
    })

    // Click → callback
    ng.addEventListener('click', () => {
      onNodeClick?.(nd.id)
    })

    // Drag
    let dragging = false,
      dx = 0,
      dy = 0
    ng.addEventListener('mousedown', (ev: MouseEvent) => {
      ev.stopPropagation()
      dragging = true
      dx = ev.clientX - nd.x
      dy = ev.clientY - nd.y
      svg.style.cursor = 'grabbing'
    })
    window.addEventListener('mousemove', (ev: MouseEvent) => {
      if (!dragging) return
      nd.x = ev.clientX - dx
      nd.y = ev.clientY - dy
      nd.vx = nd.vy = 0
      _updatePositions()
    })
    window.addEventListener('mouseup', () => {
      if (dragging) {
        dragging = false
        svg.style.cursor = 'grab'
      }
    })
  }

  // Función de actualización de posiciones
  function _updatePositions() {
    nodeEls.forEach(({ g, data }) => {
      g.setAttribute('transform', `translate(${data.x},${data.y})`)
    })

    edges.forEach((e, i) => {
      const from = nodeMap[e.from]
      const to = nodeMap[e.to]
      if (from && to) {
        // Ajustar endpoints para que la flecha no quede sobre el nodo
        const dx = to.x - from.x
        const dy = to.y - from.y
        const len = Math.sqrt(dx * dx + dy * dy) || 1
        const r1 = from.size / 2 + 2
        const r2 = to.size / 2 + 8
        edgeEls[i]?.setAttribute('x1', String(from.x + (dx / len) * r1))
        edgeEls[i]?.setAttribute('y1', String(from.y + (dy / len) * r1))
        edgeEls[i]?.setAttribute('x2', String(to.x - (dx / len) * r2))
        edgeEls[i]?.setAttribute('y2', String(to.y - (dy / len) * r2))
      }
    })
  }

  // Simulación de fuerzas (sin D3 — implementación propia)
  let frame = 0
  const MAX_FRAMES = 300

  function simulate() {
    if (frame++ > MAX_FRAMES) return
    const allNodes = nodes as any[]

    // Repulsión entre nodos
    for (let i = 0; i < allNodes.length; i++) {
      for (let j = i + 1; j < allNodes.length; j++) {
        const a = allNodes[i],
          b = allNodes[j]
        const dx = b.x - a.x,
          dy = b.y - a.y
        const d2 = dx * dx + dy * dy + 1
        const f = 1500 / d2
        a.vx -= dx * f
        a.vy -= dy * f
        b.vx += dx * f
        b.vy += dy * f
      }
    }

    // Atracción por edges
    for (const e of edges) {
      const from = nodeMap[e.from]
      const to = nodeMap[e.to]
      if (!from || !to) continue
      const dx = to.x - from.x,
        dy = to.y - from.y
      const d = Math.sqrt(dx * dx + dy * dy) + 1
      const f = (d - 120) * 0.04
      from.vx += (dx * f) / d
      from.vy += (dy * f) / d
      to.vx -= (dx * f) / d
      to.vy -= (dy * f) / d
    }

    // Gravedad hacia centro
    for (const n of allNodes as any[]) {
      n.vx += (W / 2 - n.x) * 0.01
      n.vy += (H / 2 - n.y) * 0.01
    }

    // Aplicar velocidad + fricción + bounds
    for (const n of allNodes as any[]) {
      n.vx *= 0.85
      n.vy *= 0.85
      n.x = Math.max(30, Math.min(W - 30, n.x + n.vx))
      n.y = Math.max(30, Math.min(H - 30, n.y + n.vy))
    }

    _updatePositions()
    requestAnimationFrame(simulate)
  }

  container.appendChild(svg)
  _updatePositions()
  requestAnimationFrame(simulate)
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function _langColor(lang: string): string {
  const map: Record<string, string> = {
    python: '#3d9eff',
    typescript: '#b87dff',
    javascript: '#ffb627',
    c: '#00f5a0',
    cpp: '#00f5a0',
  }
  return map[lang] ?? '#4a5880'
}

function _shortLabel(s: string): string {
  return s.length > 14 ? s.slice(0, 13) + '…' : s
}

function _nodeMetaText(n: GraphNode, graphType: string): string {
  if (graphType === 'import') {
    return `${n.functions ?? 0} fn · ${n.imports ?? 0} imp · ${n.language ?? ''}`
  }
  if (graphType === 'call') {
    return `${n.big_o ?? ''} · CC=${n.cc ?? 1}`
  }
  if (graphType === 'circular') {
    return n.in_cycle ? '🔴 En ciclo' : '✅ Sin ciclo'
  }
  return n.id
}

// ══════════════════════════════════════════════════════════════════════════════
//  FASE 2 — Graph desde proyectos ZIP/subidos
// ══════════════════════════════════════════════════════════════════════════════

/**
 * Genera el grafo visual desde un proyecto ya subido (ZIP/upload).
 * Llama a /analyze/graph/project en vez de /analyze/graph.
 * También retorna el dir_tree para el renderizado en árbol.
 */
export async function generateProjectGraph(
  projectId: string,
  graphType: string,
  onMermaid: (code: string) => void,
  onForce: (result: GraphResult) => void,
  onDirTree: (tree: DirTreeNode) => void,
  onStatus: (msg: string, ok: boolean) => void,
): Promise<void> {
  if (!projectId) {
    toast('Selecciona un proyecto primero', 'warn')
    return
  }

  onStatus('Analizando proyecto...', true)

  try {
    const result = (await api.analyzeProjectGraph(projectId, graphType)) as GraphResult & {
      dir_tree?: DirTreeNode
      total_files?: number
      file_list?: string[]
      project_id?: string
    }

    if (result.error) {
      onStatus(`Error: ${result.error}`, false)
      return
    }

    // Tree View via Mermaid
    onMermaid(result.mermaid || 'flowchart TD\n    A[Sin datos]')

    // Force Graph
    onForce(result)

    // Dir Tree (nuevo en Fase 2)
    if (result.dir_tree) {
      onDirTree(result.dir_tree)
    }

    // Status
    const summary = result.summary ?? {}
    let msg = ''
    if (graphType === 'import') {
      msg = `${result.total_files ?? 0} archivos · ${(result.edges ?? []).length} deps`
    } else if (graphType === 'call') {
      msg = `${summary.total_functions ?? 0} funciones · ${summary.total_calls ?? 0} llamadas`
    } else if (graphType === 'circular') {
      const n = result.cycles?.length ?? 0
      msg =
        n > 0
          ? `🔴 ${n} ciclo(s) — ${summary.affected_files ?? 0} archivos afectados`
          : '✅ Sin dependencias circulares'
    } else if (graphType === 'heatmap') {
      msg = `${summary.total_functions ?? 0} fn · ${summary.hot_paths ?? 0} hot paths · CC promedio ${summary.avg_cc ?? 0}`
    } else if (graphType === 'centrality') {
      const hubs = Array.isArray(summary.hubs) ? summary.hubs.length : 0
      msg = hubs > 0 ? `🔥 ${hubs} hub(s) detectado(s)` : '✅ Sin hubs (grafo poco conectado)'
    }

    onStatus(msg, !result.has_cycles)
    appendLog('ok', `🕸 Graph proyecto ${graphType}: ${msg}`, 'be')
  } catch (e) {
    const msg = (e as Error).message
    onStatus(`Error: ${msg}`, false)
    toast('Error al generar grafo: ' + msg, 'err')
  }
}

// ── Dir Tree Visual ───────────────────────────────────────────────────────────

/**
 * Renderiza el árbol de directorios del proyecto como HTML colapsable.
 * Muestra metadata de complejidad por archivo.
 * Al hacer click en un archivo llama onFileClick(path).
 */
export function renderDirTree(container: HTMLElement, tree: DirTreeNode, onFileClick: (path: string) => void): void {
  container.innerHTML = ''

  const CC_COLORS: Record<string, string> = {
    low: 'var(--ok)',
    medium: 'var(--warn)',
    high: '#ff8a00',
    critical: 'var(--err)',
  }

  const LANG_ICONS: Record<string, string> = {
    python: '🐍',
    typescript: '🟦',
    javascript: '🟨',
    c: '⚙️',
    cpp: '⚙️',
  }

  function buildNode(node: DirTreeNode, depth: number): HTMLElement {
    const wrapper = document.createElement('div')
    wrapper.style.paddingLeft = depth === 0 ? '0' : '16px'

    const row = document.createElement('div')
    row.className = `dtree-row dtree-${node.type}`
    row.style.cssText = `
      display: flex; align-items: center; gap: 5px;
      padding: 3px 6px; border-radius: 5px; cursor: ${node.type === 'file' ? 'pointer' : 'default'};
      font-family: var(--mono); font-size: .72rem; color: var(--txt);
      transition: background .1s;
    `
    row.addEventListener('mouseenter', () => {
      row.style.background = 'var(--s1)'
    })
    row.addEventListener('mouseleave', () => {
      row.style.background = ''
    })

    if (node.type === 'directory') {
      // Ícono de carpeta + toggle
      let expanded = depth < 2 // expandir las primeras 2 niveles
      const toggle = document.createElement('span')
      toggle.textContent = expanded ? '▾' : '▸'
      toggle.style.cssText = 'font-size:.6rem;color:var(--muted);width:10px;flex-shrink:0'

      const icon = document.createElement('span')
      icon.textContent = '📁'

      const name = document.createElement('span')
      name.textContent = node.name
      name.style.fontWeight = '600'

      row.appendChild(toggle)
      row.appendChild(icon)
      row.appendChild(name)

      // Estadísticas de carpeta (agregadas)
      if (node.children?.length) {
        const count = document.createElement('span')
        count.textContent = `${node.children.length} items`
        count.style.cssText = 'color:var(--muted);font-size:.6rem;margin-left:auto'
        row.appendChild(count)
      }

      wrapper.appendChild(row)

      // Hijos
      const childContainer = document.createElement('div')
      childContainer.style.display = expanded ? 'block' : 'none'

      for (const child of node.children ?? []) {
        childContainer.appendChild(buildNode(child, depth + 1))
      }
      wrapper.appendChild(childContainer)

      // Toggle click
      row.addEventListener('click', () => {
        expanded = !expanded
        toggle.textContent = expanded ? '▾' : '▸'
        childContainer.style.display = expanded ? 'block' : 'none'
      })
    } else {
      // Archivo
      const stats = node.stats
      const lang = stats?.language ?? ''
      const icon = LANG_ICONS[lang] ?? '📄'
      const hotPath = (stats?.hot_paths ?? 0) > 0

      const iconEl = document.createElement('span')
      iconEl.textContent = icon
      iconEl.style.cssText = 'width:14px;flex-shrink:0'

      const nameEl = document.createElement('span')
      nameEl.textContent = node.name
      nameEl.style.cssText = hotPath ? 'color:var(--err);font-weight:600' : 'color:var(--txt)'

      row.appendChild(document.createElement('span')) // spacer para alinear con folders
      row.appendChild(iconEl)
      row.appendChild(nameEl)

      // Stats badge
      if (stats && stats.functions > 0) {
        const badge = document.createElement('span')
        const cc = stats.avg_cc
        const ccLvl = cc <= 5 ? 'low' : cc <= 10 ? 'medium' : cc <= 20 ? 'high' : 'critical'
        badge.style.cssText = `
          margin-left: auto; font-size: .58rem; padding: 1px 5px;
          border-radius: 3px; background: ${CC_COLORS[ccLvl]}20;
          color: ${CC_COLORS[ccLvl]}; border: 1px solid ${CC_COLORS[ccLvl]}40;
          white-space: nowrap;
        `
        badge.textContent = `${stats.functions}fn · CC${cc}`
        if (hotPath) badge.textContent += ' 🔴'
        row.appendChild(badge)
      }

      // Click → abrir archivo en Monaco
      row.addEventListener('click', () => onFileClick(node.path))
      wrapper.appendChild(row)
    }

    return wrapper
  }

  const root = document.createElement('div')
  root.style.cssText = 'padding: 4px 0; overflow-y: auto; height: 100%'

  if (tree.children?.length) {
    for (const child of tree.children) {
      root.appendChild(buildNode(child, 0))
    }
  } else {
    root.innerHTML = '<div class="empty"><span class="empty-icon">📁</span>Sin archivos</div>'
  }

  container.appendChild(root)
}

// ── Click nodo → abrir archivo en Monaco ──────────────────────────────────────

/**
 * Conecta el click en un nodo del Force Graph al editor Monaco.
 * Dado un projectId y el path del nodo, carga el archivo en el editor.
 */
export async function openNodeInEditor(
  projectId: string,
  nodePath: string,
  onContent: (filename: string, content: string, language: string) => void,
): Promise<void> {
  try {
    const fc = await api.getFileContent(projectId, nodePath)
    if (fc?.content) {
      const ext = nodePath.split('.').pop() ?? ''
      const lang = _extToLanguage(ext)
      onContent(nodePath, fc.content, lang)
      appendLog('ok', `📂 Abierto: ${nodePath}`, 'fe')
    }
  } catch (e) {
    toast(`No se pudo abrir ${nodePath}: ${(e as Error).message}`, 'err')
  }
}

function _extToLanguage(ext: string): string {
  const map: Record<string, string> = {
    py: 'python',
    ts: 'typescript',
    tsx: 'typescript',
    js: 'javascript',
    jsx: 'javascript',
    c: 'c',
    cpp: 'cpp',
    h: 'c',
    hpp: 'cpp',
  }
  return map[ext] ?? 'plaintext'
}
