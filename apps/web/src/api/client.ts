// ══════════════════════════════════════════
//  Sythrall — API Client v4.2
//  Actualizado para FastAPI backend
// ══════════════════════════════════════════

import { getApiBase } from '../store/state'
import type { AnalysisResult, AnalyzeProjectResult, ApiResult, Capabilities, MLAnalysisResult } from '../types'

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
  /** Persistido desde la Fase de UX Audit — proyectos subidos antes de ese
   * fix no lo tienen, de ahí el opcional. */
  project_name?: string
}

export interface UploadResult {
  project_id: string
  project_name: string
  type: 'files' | 'folder' | 'zip' | 'empty'
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
  /** Persistido desde la Fase de UX Audit — proyectos subidos antes de ese
   * fix no lo tienen, de ahí el opcional. */
  project_name?: string
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
  is_recursive?: boolean
  is_tail_recursive?: boolean
  recursion_note?: string | null
  recurrence?: string | null
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

// Fase 23 — "Memory visualizer": clasificación estática (AST) de cada
// variable en stack/heap/data/bss, no una inspección de proceso corriendo
// (ver `services/complexity/src/memlayout.rs`). `heap` no es una región de
// variable — es un allocation site aparte, en `allocations`.
export interface MemoryVariable {
  name: string
  region: 'stack' | 'data' | 'bss'
  scope: string
  line: number
  type_hint: string
}

export interface AllocationSite {
  kind: 'malloc' | 'calloc' | 'realloc' | 'free' | 'new' | 'delete'
  line: number
  variable: string | null
}

export interface MemoryLayoutResult {
  variables: MemoryVariable[]
  allocations: AllocationSite[]
  summary: { stack: number; heap_allocations: number; data: number; bss: number }
  note: string
}

export interface SecurityFinding {
  cwe: string
  category: string
  severity: 'High' | 'Medium' | 'Low'
  confidence: 'High' | 'Medium' | 'Low'
  source: string
  sink: string | null
  line: number
  function: string | null
  recommendation: string
  /** Solo presente en resultados a nivel de proyecto (parse-project) — el
   * archivo de origen, ausente en resultados de un solo archivo. */
  file?: string
}

export interface StructuralSmell {
  kind:
    | 'long_function'
    | 'excessive_parameters'
    | 'deep_nesting'
    | 'large_class'
    | 'god_object'
    | 'quadratic_list_membership'
    | 'de_morgan_simplifiable'
  name: string
  line: number
  message: string
  /** Solo presente a nivel de proyecto, ver SecurityFinding.file. */
  file?: string
}

export interface NamingSmell {
  kind: 'single_letter_name' | 'inconsistent_casing' | 'shadowed_name'
  name: string
  line: number
  message: string
  /** Solo presente a nivel de proyecto, ver SecurityFinding.file. */
  file?: string
}

/** Fase 22: acoplamiento eferente alto, dependencia inestable, y ciclos de
 * import reencuadrados como un smell más. A diferencia de StructuralSmell/
 * NamingSmell, no hay campo `file` — estos smells ya son globales (nunca
 * existen a nivel de un solo archivo), así que `name` lleva la ruta completa
 * del archivo por sí sola. `line` siempre es 0 (smell de archivo/grafo, no
 * de línea puntual). */
export interface ArchitectureSmell {
  kind: 'circular_dependency' | 'unstable_dependency' | 'high_efferent_coupling'
  name: string
  line: number
  message: string
}

export interface ProjectHealthMetric {
  score: number
}

export interface ProjectHealth {
  security: ProjectHealthMetric & { high: number; medium: number; low: number }
  quality: ProjectHealthMetric & { smells: number; naming: number }
  complexity: ProjectHealthMetric & { avg_complexity: number }
  architecture: ProjectHealthMetric & { cycles: number; smells: number }
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
  memory_layout?: MemoryLayoutResult
  security_findings: SecurityFinding[]
  structural_smells: StructuralSmell[]
  naming_smells: NamingSmell[]
  summary: Record<string, number | string>
  error?: string
}

export interface StaticProjectResult {
  files: StaticParseResult[]
  summary: {
    total_files: number
    total_functions: number
    total_classes: number
    total_imports: number
    unused_imports: number
    big_o_distribution: Record<string, number>
    wasm_candidates: number
    security_findings: number
    structural_smells: number
    naming_smells: number
    /** Incluye las entradas de circular_dependency — a diferencia de
     * health.architecture.smells, que las excluye para no penalizar el
     * score dos veces (ver comentario en el backend). */
    architecture_smells: number
    total_loc: number
  }
  wasm_candidates: Array<{ file: string; hints: WasmHint[] }>
  security_findings: SecurityFinding[]
  structural_smells: StructuralSmell[]
  naming_smells: NamingSmell[]
  architecture_smells: ArchitectureSmell[]
  top_complex_functions: Array<{ file: string; name: string; line: number; complexity: number; big_o: string }>
  language_distribution: Record<string, { files: number; loc: number; functions: number }>
  health: ProjectHealth
}

/** Capacidad real del motor de análisis por lenguaje — `available` refleja
 * si el sidecar Rust (`complexity-engine`) está arriba, no una promesa. Los
 * 5 lenguajes (Python/C/C++/JS/TS/Fortran) dependen de él ahora. */
export interface StaticLanguagesResult {
  languages: Record<string, { extensions: string[]; parser: string; features: string[]; available: boolean }>
  capabilities: { complexity_engine: boolean }
}

/** Fase 23 (Execution Intelligence) — validación empírica del O(n³) que
 * `numerical_algorithm_note` predice por forma (Fase 20). Compila y corre un
 * kernel Fortran que Sythrall mismo escribe (nunca el código del usuario) a
 * varios tamaños, mide el tiempo real. `available: false` cuando `gfortran`
 * o el sidecar no están disponibles — degrada con gracia, no es un error. */
export interface EmpiricalValidationResult {
  available: boolean
  predicted_big_o: string
  measurements: Array<{ n: number; seconds: number }>
  estimated_exponent: number | null
  note: string
}

/** Fase 26 — primera pieza de "migrar de numpy/pandas/scikit-learn" (pedido
 * explícito del usuario): compara el kernel Fortran de matmul YA validado
 * contra numpy real, mismos tamaños/datos, side by side — antes de proponer
 * cualquier reemplazo nativo hay que medir contra la librería real, no
 * asumir. `numpy.available` es `false` cuando numpy no está instalado en el
 * entorno del backend, degrada con gracia igual que el resto. */
export interface MatmulVsNumpyResult {
  fortran: EmpiricalValidationResult
  numpy: {
    available: boolean
    measurements: Array<{ n: number; seconds: number }>
    note: string
  }
  comparison_note: string
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
      tools: ['ast', 'flake8', 'pylint', 'complexity'],
    }),

  // ── Análisis de proyecto completo — flake8/pylint corren UNA vez para todos
  // los archivos en vez de un subprocess por archivo (ver /analyze/project en
  // el backend para el porqué: ~30x más rápido en proyectos grandes)
  analyzeProject: (files: { filename: string; content: string }[]) =>
    post<AnalyzeProjectResult>('/analyze/project', {
      files,
      tools: ['ast', 'flake8', 'pylint', 'complexity'],
    }),

  /** Igual que analyzeProject, pero para un proyecto ya subido (Proyectos) —
   * el backend lee los archivos del disco, no hace falta mandarlos. */
  analyzeProjectById: (projectId: string) =>
    post<AnalyzeProjectResult>('/analyze/project', {
      project_id: projectId,
      tools: ['ast', 'flake8', 'pylint', 'complexity'],
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
  // `projectId`: si viene, los archivos se agregan a ese proyecto ya existente
  // en vez de crear uno nuevo — usado cuando "+ Código"/"+ Carpeta" del
  // sidebar suman al proyecto activo (ver components/app.ts:handleCodeFiles).
  uploadFiles: (files: File[], projectName = '', onProgress?: (p: UploadProgress) => void, projectId?: string) => {
    const form = new FormData()
    for (const f of files) form.append('files', f, f.name)
    if (projectName) form.append('project_name', projectName)
    if (projectId) form.append('project_id', projectId)
    return xhrPost<UploadResult>('/api/upload/files', form, onProgress)
  },

  uploadFolder: (files: File[], projectName = '', onProgress?: (p: UploadProgress) => void, projectId?: string) => {
    const form = new FormData()
    for (const f of files) {
      const path = (f as File & { webkitRelativePath?: string }).webkitRelativePath ?? f.name
      form.append('files', new Blob([f], { type: f.type }), path)
    }
    if (projectName) form.append('project_name', projectName)
    if (projectId) form.append('project_id', projectId)
    return xhrPost<UploadResult>('/api/upload/folder', form, onProgress)
  },

  uploadZip: (file: File, projectName = '', onProgress?: (p: UploadProgress) => void) => {
    const form = new FormData()
    form.append('file', file, file.name)
    if (projectName) form.append('project_name', projectName)
    return xhrPost<UploadResult>('/api/upload/zip', form, onProgress)
  },

  // Proyecto sin ningún archivo — para codificar desde cero (+ Nuevo archivo)
  // en vez de partir siempre de algo ya subido. FormData + fetch plano, no
  // post<T>() (ese manda JSON; el backend espera Form(...) como el resto de
  // los endpoints de este router).
  createEmptyProject: async (projectName = ''): Promise<UploadResult> => {
    const form = new FormData()
    if (projectName) form.append('project_name', projectName)
    const res = await fetch(getApiBase() + '/api/upload/empty', { method: 'POST', body: form })
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    return res.json()
  },

  // Crea un archivo nuevo (vacío o con contenido inicial) dentro de un
  // proyecto ya existente — contraparte de escritura de getFileContent.
  createProjectFile: async (
    projectId: string,
    filePath: string,
    content = '',
  ): Promise<{ path: string; size: number }> => {
    const form = new FormData()
    form.append('path', filePath)
    form.append('content', content)
    const res = await fetch(getApiBase() + `/api/upload/projects/${projectId}/file`, { method: 'POST', body: form })
    if (!res.ok) {
      let msg = `HTTP ${res.status}`
      try {
        msg = (await res.json()).detail ?? msg
      } catch {
        /* noop */
      }
      throw new Error(msg)
    }
    return res.json()
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

  /** Igual que staticParseProject, pero para un proyecto ya subido (Proyectos). */
  staticParseProjectById: (projectId: string) =>
    post<StaticProjectResult>('/static/parse-project', { project_id: projectId }),

  /** Capacidad real del motor por lenguaje (no una lista estática hardcodeada
   * en el frontend) — para el widget "Languages" del Dashboard. */
  staticLanguages: () => get<StaticLanguagesResult>('/static/languages'),

  // ── Execution Intelligence (Fase 23) ──────────────────────────────────────

  /** Compila y corre un kernel Fortran real para validar empíricamente el
   * O(n³) detectado por forma en Fase 20 — puede tardar varios segundos
   * (compila + corre 4 tamaños), a diferencia de todo lo demás en este
   * cliente que es análisis estático instantáneo. */
  validateMatmulBigO: () => post<EmpiricalValidationResult>('/execution/validate-matmul', {}),

  /** Fase 26 — mismo patrón que `validateMatmulBigO`, generalizado a un
   * segundo kernel Sythrall-autor: bubble sort real en Zig, valida O(n²). */
  validateBubbleSortBigO: () => post<EmpiricalValidationResult>('/execution/validate-bubble-sort', {}),

  /** Fase 26 — tercer kernel: suma de cuadrados escrita a mano en Assembly
   * x86 (AT&T, cdecl), valida O(n). */
  validateSumSquaresBigO: () => post<EmpiricalValidationResult>('/execution/validate-sum-squares', {}),

  /** Fase 26 — cuarto kernel: BFS sobre un grafo disperso de grado fijo,
   * segunda vez en Zig pero forma algorítmica distinta (recorrido de
   * grafos, no ordenamiento), valida O(V+E). */
  validateGraphBfsBigO: () => post<EmpiricalValidationResult>('/execution/validate-graph-bfs', {}),

  /** Fase 26 — quinto kernel, el primero en validar una forma NO
   * polinomial: Fibonacci recursivo ingenuo en Fortran, valida crecimiento
   * exponencial (Θ(φⁿ)). `estimated_exponent` acá es la BASE medida del
   * crecimiento, no un exponente `n^k` como en los otros 4 — ver
   * `fib_bench.rs` para la razón estadística. */
  validateFibonacciExponential: () => post<EmpiricalValidationResult>('/execution/validate-fibonacci', {}),

  /** Fase 26 — sexto kernel, el primero en validar O(n log n): mergesort
   * bottom-up iterativo escrito a mano en Assembly x86 (segunda vez en
   * Assembly, forma algorítmica distinta a la suma de cuadrados). */
  validateMergesortBigO: () => post<EmpiricalValidationResult>('/execution/validate-mergesort', {}),

  /** Fase 26 — primera pieza de "migrar de numpy/pandas/scikit-learn":
   * corre el kernel Fortran de matmul Y numpy real, mismos tamaños/datos,
   * y devuelve ambos lado a lado con una nota de comparación honesta. */
  compareMatmulVsNumpy: () => post<MatmulVsNumpyResult>('/execution/validate-matmul-vs-numpy', {}),
}
