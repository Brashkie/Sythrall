// ══════════════════════════════════════════
//  CodeWatch PRO — API Client v4.2
//  Actualizado para FastAPI backend
// ══════════════════════════════════════════

import { getApiBase } from '../store/state'
import type { AnalysisResult, ApiResult, Capabilities, MLAnalysisResult } from '../types'

const TIMEOUT = 60_000

async function post<T>(path: string, body: unknown): Promise<T> {
  const controller = new AbortController()
  const tid = setTimeout(() => controller.abort(), TIMEOUT)
  try {
    const res = await fetch(getApiBase() + path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
      signal: controller.signal,
    })
    if (!res.ok) {
      let msg = `HTTP ${res.status}`
      try {
        const b = await res.json()
        msg = b.detail ?? b.message ?? msg
      } catch {
        /* noop */
      }
      throw new Error(msg)
    }
    return res.json() as Promise<T>
  } finally {
    clearTimeout(tid)
  }
}

async function get<T>(path: string): Promise<T> {
  const res = await fetch(getApiBase() + path, {
    signal: AbortSignal.timeout(8000),
  })
  if (!res.ok) throw new Error(`HTTP ${res.status}`)
  return res.json() as Promise<T>
}

// ─── Upload helpers (XHR para progress real) ──────────────────────────────────

export interface UploadProgress {
  loaded: number
  total: number
  percent: number
}

function xhrPost<T>(path: string, formData: FormData, onProgress?: (p: UploadProgress) => void): Promise<T> {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest()

    xhr.upload.addEventListener('progress', (e) => {
      if (e.lengthComputable && onProgress) {
        onProgress({ loaded: e.loaded, total: e.total, percent: Math.round((e.loaded / e.total) * 100) })
      }
    })

    xhr.addEventListener('load', () => {
      if (xhr.status >= 200 && xhr.status < 300) {
        try {
          resolve(JSON.parse(xhr.responseText))
        } catch {
          reject(new Error('Respuesta inválida del servidor.'))
        }
      } else {
        try {
          const b = JSON.parse(xhr.responseText)
          reject(new Error(b.detail ?? `Error ${xhr.status}`))
        } catch {
          reject(new Error(`Error del servidor: ${xhr.status}`))
        }
      }
    })

    xhr.addEventListener('error', () => reject(new Error('Error de red.')))
    xhr.addEventListener('abort', () => reject(new Error('Subida cancelada.')))
    xhr.open('POST', getApiBase() + path)
    xhr.send(formData)
  })
}

// ─── Tipos de proyecto/upload ──────────────────────────────────────────────────

export interface ProjectTreeNode {
  name: string
  type: 'file' | 'directory'
  path: string
  size?: number
  size_fmt?: string
  extension?: string
  is_code?: boolean
  modified?: string
  children?: ProjectTreeNode[]
  truncated?: boolean
  error?: string
}

export interface ProjectInfo {
  total_files: number
  total_size: number
  total_size_fmt: string
  code_files: number
  by_extension: Record<string, number>
  created_at: string
}

export interface UploadResult {
  project_id: string
  project_name: string
  type: 'files' | 'folder' | 'zip'
  total_files: number
  tree: ProjectTreeNode
  info?: ProjectInfo
  errors?: Array<{ file: string; reason: string }>
  extracted?: number
  skipped?: number
}

export interface ProjectSummary {
  project_id: string
  total_files: number
  total_size: number
  total_size_fmt: string
  code_files: number
  by_extension: Record<string, number>
  created_at: string
}

export interface FileContent {
  path: string
  content: string
  size: number
  extension: string
}

// ─── Tipos Static Analysis ────────────────────────────────────────────────────

export interface StaticFunction {
  name: string
  line: number
  end_line?: number
  loc?: number
  complexity: number
  big_o: string
  big_o_reason: string
  big_o_theta?: string
  big_o_omega?: string
  calls?: string[]
  is_async?: boolean
  args?: string[]
  decorators?: string[]
}

export interface StaticClass {
  name: string
  line: number
  bases?: string[]
  methods?: Array<{ name: string; line: number }>
  kind?: string
}

export interface StaticImport {
  module: string
  type: string
  line: number
  name?: string
  alias?: string
}

export interface WasmHint {
  function: string
  line: number
  priority: number
  reasons: string[]
  recommendation: string
  estimated_speedup: string
}

export interface StaticParseResult {
  filename: string
  language: string
  functions: StaticFunction[]
  classes: StaticClass[]
  imports: StaticImport[]
  exports: Array<{ name: string; line: number }>
  interfaces?: Array<{ name: string; line: number }>
  types?: Array<{ name: string; line: number }>
  dead_code: Array<{ type: string; name?: string; module: string; line: number }>
  call_graph: Array<{ from: string; to: string }>
  wasm_hints: WasmHint[]
  summary: Record<string, number | string>
  error?: string
}

export interface BigOResult {
  filename: string
  language: string
  functions: Array<{ function: string; line: number; big_o: string; reason: string; complexity: number; loc: number }>
  distribution: Record<string, number>
  hot_paths: Array<{ function: string; big_o: string }>
  total: number
}

export interface StaticProjectResult {
  files: StaticParseResult[]
  dependency_graph: Array<{ from: string; to: string; via: string }>
  summary: {
    total_files: number
    total_functions: number
    total_classes: number
    total_imports: number
    unused_imports: number
    big_o_distribution: Record<string, number>
    wasm_candidates: number
  }
  wasm_candidates: Array<{ file: string; hints: WasmHint[] }>
}

// ─── Tipos Intelligence ───────────────────────────────────────────────────────

export interface IntelMarker {
  startLineNumber: number
  startColumn: number
  endLineNumber: number
  endColumn: number
  message: string
  severity: number // 1=Hint 2=Info 4=Warning 8=Error (Monaco compatible)
  source: string
  code: string
}

export interface IntelLintResult {
  markers: IntelMarker[]
  ms: number
  source: 'fast'
}

export interface IntelAnalyzeResult {
  markers: IntelMarker[]
  metrics: {
    pylint_score?: number
    complexity?: Array<{ name: string; line: number; complexity: number; rank: string }>
    maintainability?: number
  }
  big_o: Array<{
    name: string
    line: number
    big_o: string
    reason: string
    complexity: number
  }>
  ms: number
  source: 'heavy'
}

export interface IntelHoverResult {
  markdown: string
  range: {
    startLineNumber: number
    startColumn?: number
    endLineNumber: number
    endColumn?: number
  } | null
}

export interface IntelDefinitionResult {
  found: boolean
  symbol: string
  filename: string
  definitions: Array<{
    line: number
    column: number
    end_line?: number
    kind: string
    signature: string
    docstring: string
  }>
}

export interface IntelReferencesResult {
  symbol: string
  filename: string
  references: Array<{
    line: number
    column: number
    kind: string
    preview: string
  }>
  definition_line: number | null
  total: number
}

// ─── Tipos Code Graph ─────────────────────────────────────────────────────────

export interface GraphNode {
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
  name?: string
  file?: string
  file_short?: string
  cc_color?: string
  cc_level?: string
  bigo_color?: string
  bigo_level?: string
  loc?: number
}

export interface GraphEdge {
  from: string
  to: string
  via?: string
  is_cycle?: boolean
}

export interface GraphResult {
  graph_type: string
  nodes: GraphNode[]
  edges: GraphEdge[]
  functions?: GraphNode[]
  mermaid: string
  summary?: Record<string, unknown>
  cycles?: string[][]
  has_cycles?: boolean
  entry_points?: string[]
  error?: string
}

export interface GraphType {
  id: string
  label: string
  icon: string
  description: string
}

// Dir tree node para Fase 2 (proyectos subidos)
export interface DirTreeNode {
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

// Respuesta extendida de graph/project (Fase 2)
export interface ProjectGraphResult extends GraphResult {
  project_id?: string
  total_files?: number
  file_list?: string[]
  dir_tree?: DirTreeNode
}

// ─── API pública ──────────────────────────────────────────────────────────────

export const api = {
  // ── Sistema
  capabilities: () => get<Capabilities>('/capabilities'),

  // ── APIs externas
  checkUrls: (urls: string[], timeout = 10) => post<{ results: ApiResult[] }>('/analyze/api', { urls, timeout }),

  // ── Análisis de código
  analyzeCode: (filename: string, content: string) =>
    post<AnalysisResult>('/analyze/code', {
      filename,
      content,
      tools: ['ast', 'flake8', 'pylint', 'radon'],
    }),

  // ── ML/DL
  analyzeML: (filename: string, content: string) => post<MLAnalysisResult>('/analyze/ml', { filename, content }),

  // ── Logs
  analyzeLogs: (files: { name: string; content: string }[]) =>
    post<{ errors: unknown[]; warnings: unknown[] }>('/analyze/logs-analyze', { files }),

  // ── Diagramas Mermaid
  generateDiagram: (filename: string, content: string, diagram_type: string) =>
    post<{ mermaid: string }>('/analyze/diagram', { filename, content, diagram_type }),

  // ── Code Graph Visual (v4.2) ──────────────────────────────────────────────

  /** Fase 1: graph desde archivos del sidebar */
  analyzeGraph: (files: Array<{ filename: string; content: string }>, graph_type: string) =>
    post<GraphResult>('/analyze/graph', { files, graph_type }),

  /** Fase 2: graph desde proyecto ZIP/subido — incluye dir_tree + metadata */
  analyzeProjectGraph: (project_id: string, graph_type: string) =>
    post<ProjectGraphResult>('/analyze/graph/project', { project_id, graph_type }),

  /** Tipos de grafo disponibles */
  graphTypes: () => get<{ types: GraphType[] }>('/analyze/graph/types'),

  // ── Logs del servidor
  getLogs: (limit = 50) => get<{ logs: unknown[] }>(`/logs?limit=${limit}`),

  // ── Upload de proyectos
  uploadFiles: (files: File[], projectName = '', onProgress?: (p: UploadProgress) => void) => {
    const form = new FormData()
    for (const f of files) form.append('files', f, f.name)
    if (projectName) form.append('project_name', projectName)
    return xhrPost<UploadResult>('/api/upload/files', form, onProgress)
  },

  uploadFolder: (files: File[], projectName = '', onProgress?: (p: UploadProgress) => void) => {
    const form = new FormData()
    for (const f of files) {
      const path = (f as File & { webkitRelativePath?: string }).webkitRelativePath ?? f.name
      form.append('files', new Blob([f], { type: f.type }), path)
    }
    if (projectName) form.append('project_name', projectName)
    return xhrPost<UploadResult>('/api/upload/folder', form, onProgress)
  },

  uploadZip: (file: File, projectName = '', onProgress?: (p: UploadProgress) => void) => {
    const form = new FormData()
    form.append('file', file, file.name)
    if (projectName) form.append('project_name', projectName)
    return xhrPost<UploadResult>('/api/upload/zip', form, onProgress)
  },

  listProjects: () => get<{ projects: ProjectSummary[]; total: number }>('/api/upload/projects'),

  getProjectTree: (projectId: string) =>
    get<{ project_id: string; tree: ProjectTreeNode; info: ProjectInfo }>(`/api/upload/projects/${projectId}/tree`),

  getFileContent: (projectId: string, filePath: string) =>
    get<FileContent>(`/api/upload/projects/${projectId}/file?path=${encodeURIComponent(filePath)}`),

  deleteProject: (projectId: string) =>
    fetch(getApiBase() + `/api/upload/projects/${projectId}`, { method: 'DELETE' }).then((r) => {
      if (!r.ok) throw new Error(`HTTP ${r.status}`)
      return r.json()
    }),

  // ── Fallback: ping desde browser sin CORS
  browserPing: async (url: string): Promise<ApiResult> => {
    const t0 = Date.now()
    const r: ApiResult = {
      url,
      status: 'unknown',
      code: null,
      ms: null,
      error: null,
      ts: new Date().toLocaleTimeString(),
      history: [],
    }
    try {
      await fetch(url, { mode: 'no-cors', signal: AbortSignal.timeout(6000) })
      r.ms = Date.now() - t0
      r.status = 'ok'
      r.code = '2xx'
    } catch (e: unknown) {
      r.ms = Date.now() - t0
      const err = e as Error
      r.status = err.name === 'AbortError' ? 'down' : 'warning'
      r.error = err.name === 'AbortError' ? 'Timeout' : err.message
    }
    return r
  },

  // ── Análisis estático multi-lenguaje SIN IA (v4.1) ────────────────────────

  staticParse: (filename: string, content: string) => post<StaticParseResult>('/static/parse', { filename, content }),

  staticParseProject: (files: Array<{ filename: string; content: string }>) =>
    post<StaticProjectResult>('/static/parse-project', { files }),

  staticBigO: (filename: string, content: string) => post<BigOResult>('/static/bigO', { filename, content }),

  staticWasm: (filename: string, content: string) =>
    post<{
      filename: string
      language: string
      hints: WasmHint[]
      total: number
      critical: WasmHint[]
      summary: string
    }>('/static/wasm', { filename, content }),

  staticLanguages: () =>
    get<{
      languages: Record<string, { extensions: string[]; parser: string; features: string[]; available: boolean }>
      capabilities: Record<string, boolean>
    }>('/static/languages'),

  // ── Editor Intelligence (v4.1) ────────────────────────────────────────────

  intelLint: (filename: string, content: string) => post<IntelLintResult>('/intel/lint', { filename, content }),

  intelAnalyze: (filename: string, content: string, tools = ['ast', 'flake8', 'radon']) =>
    post<IntelAnalyzeResult>('/intel/analyze', { filename, content, tools }),

  intelHover: (filename: string, content: string, line: number, column: number, symbol_name: string) =>
    post<IntelHoverResult>('/intel/hover', { filename, content, line, column, symbol_name }),

  intelDefinition: (filename: string, content: string, line: number, column: number, symbol_name: string) =>
    post<IntelDefinitionResult>('/intel/definition', { filename, content, line, column, symbol_name }),

  intelReferences: (filename: string, content: string, symbol_name: string) =>
    post<IntelReferencesResult>('/intel/references', { filename, content, symbol_name }),

  intelCompletions: (filename: string, content: string, prefix = '') =>
    post<{
      symbols: Array<{
        label: string
        kind: string
        detail: string
        documentation: string | null
        insert_text: string
        sort_text: string
        line: number
      }>
      total: number
      filename: string
      prefix: string
    }>('/intel/completions', { filename, content, prefix }),

  intelRename: (filename: string, content: string, symbol_name: string, new_name: string) =>
    post<{
      valid: boolean
      edits: Array<{
        range: { startLineNumber: number; startColumn: number; endLineNumber: number; endColumn: number }
        newText: string
        kind: string
        preview: string
      }>
      total: number
      old_name: string
      new_name: string
      filename: string
      error?: string
    }>('/intel/rename', { filename, content, symbol_name, new_name }),
}
