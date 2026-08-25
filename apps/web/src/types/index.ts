// ══════════════════════════════════════════
//  Sythrall — Types
// ══════════════════════════════════════════
import type { ProjectHealth, StaticProjectResult } from '../api/client'

export type Severity = 'error' | 'warning' | 'info'
export type Status = 'ok' | 'warning' | 'down' | 'unknown' | 'error'
export type StepState = 'idle' | 'run' | 'ok' | 'err' | 'warn'

// 'upload' agregado para el nuevo panel de proyectos
export type TabId =
  | 'dashboard'
  | 'editor'
  | 'apis'
  | 'issues'
  | 'diagram'
  | 'ml'
  | 'metrics'
  | 'diff'
  | 'logs'
  | 'upload'
  | 'static'

export interface CodeFile {
  id: string
  name: string
  ext: string
  size: number
  content: string
  issues: Issue[]
  metrics: FileMetrics
  analyzed: boolean
  /** Ruta relativa dentro de la carpeta explorada (solo si vino del árbol de carpetas, no de "+ Código"). */
  path?: string
}

export interface LogFile {
  name: string
  size: number
  content: string
  /** project_id activo al momento de cargar el log, si había uno — asocia el
   * log con el contexto en el que se subió, sin requerir almacenamiento
   * nuevo del lado del servidor. */
  projectId?: string | null
}

export interface Issue {
  tool: string
  line?: number
  col?: number
  severity: Severity
  code?: string
  message: string
  preview?: string
  file?: string
  symbol?: string
  suggestion?: string
  category?: string
}

export interface ApiResult {
  url: string
  status: Status
  code: string | number | null
  ms: number | null
  error: string | null
  ts: string | null
  history: HistoryEntry[]
  headers?: Record<string, string>
  content_type?: string
  json_preview?: string
}

export interface HistoryEntry {
  ts: string
  status: Status
  ms: number | null
  code?: string | number | null
}

export interface HalsteadMetrics {
  distinct_operators: number
  distinct_operands: number
  total_operators: number
  total_operands: number
  vocabulary: number
  length: number
  volume: number
  difficulty: number
  effort: number
}

export interface FileMetrics {
  pylint_score?: number
  complexity?: ComplexityEntry[]
  mi?: number
  halstead?: HalsteadMetrics | null
  raw?: RawStats
  tools_used?: string[]
}

export interface ComplexityEntry {
  name: string
  type: string
  line: number
  complexity: number
  rank: string
}

export interface RawStats {
  loc: number
  lloc: number
  sloc: number
  comments: number
  blank: number
  multi: number
}

export interface MLLibrary {
  name: string
  category: string
  color: string
  import: string
  alias?: string | null
  version?: string | null
}

export interface PipelineStep {
  id: string
  description: string
  icon: string
  line: number
  count: number
}

export interface MLModel {
  name: string
  type: string
  family: string
  framework: string
  line: number
}

export interface MLMetric {
  found: boolean
  count: number
  value: number | null
}

export interface MLAnalysisResult {
  filename: string
  ts: string
  libraries: MLLibrary[]
  pipeline: PipelineStep[]
  models: MLModel[]
  metrics: Record<string, MLMetric>
  issues: Issue[]
  diagram: string
  score: number
  suggestions: string[]
}

export interface AnalysisResult {
  filename: string
  ts: string
  issues: Issue[]
  metrics: { pylint_score?: number }
  complexity: ComplexityEntry[]
  maintainability: number | null
  halstead: HalsteadMetrics | null
  raw_stats: RawStats
  tools_used: string[]
}

export interface AnalyzeProjectResult {
  files: Record<string, AnalysisResult>
}

export interface RunHistoryEntry {
  ts: string
  issues: number
  apiOk: number
  ms: number
}

export interface AppState {
  files: CodeFile[]
  logFiles: LogFile[]
  urls: string[]
  results: {
    apis: ApiResult[]
    issues: Issue[]
    logErrors: LogError[]
    /** Último resultado de análisis del proyecto activo (parse-project) —
     * compartido entre Dashboard y la vista de proyecto de Static para no
     * pedir el mismo análisis dos veces. */
    projectDashboard: StaticProjectResult | null
  }
  running: boolean
  /** Un análisis de PROYECTO (parse-project, disparado desde Static o el
   * Dashboard) está en curso — distinto de `running` (el pipeline ad-hoc de
   * "▶ Analizar" del topbar) para no acoplar el guard de ese botón a un
   * análisis de proyecto en curso en otro lado. Señal real para el widget
   * Flujo (components/flow.ts) — nunca un estado "en progreso" decorativo. */
  projectAnalysisRunning: boolean
  autoOn: boolean
  autoTimer: ReturnType<typeof setInterval> | null
  history: RunHistoryEntry[]
  currentFile: CodeFile | null
  backendOk: boolean
  /** Si ya se resolvió (éxito o error) al menos un `checkBackend()` — sin
   * esto, `!backendOk` no distingue "todavía verificando" de "confirmado
   * caído", y el hero vacío del Dashboard mostraría un falso "Offline" por
   * el instante que tarda el primer chequeo al arrancar la app. */
  backendChecked: boolean
  /** Resultado crudo del último `/capabilities` exitoso — reusado por el hero
   * vacío del Dashboard para mostrar qué motores están realmente vivos
   * (nunca un estado inventado), en vez de repetir el fetch. `null` mientras
   * no hay backend confirmado. */
  capabilities: Capabilities | null
  currentMermaid: string
  /** Proyecto elegido en el panel Proyectos — Issues/Diagrama/Static/ML/DL lo
   * usan para pedirle al backend datos de ese proyecto sin necesitar que el
   * usuario cargue cada archivo a mano con "+ Código". */
  activeProjectId: string | null
  /** Nombre humano del proyecto activo (no solo el id) — mostrado en el
   * indicador persistente del nav-rail. `null` si no hay proyecto activo o
   * si por algún motivo no se conoce el nombre (cae al id truncado). */
  activeProjectName: string | null
  /** Cache de `ProjectHealth` por proyecto, poblado por
   * `loadProjectHealth()` (dashboard.ts) cada vez que un análisis completo
   * resuelve para ese proyecto EN ESTA SESIÓN — nunca inventado. El grid de
   * Proyectos lo usa para mostrar badges de score solo cuando ya existen
   * datos reales, y un pill neutro "Sin analizar" en caso contrario, en vez
   * de forzar un re-análisis caro solo para poblar la lista. */
  projectHealthCache: Record<string, ProjectHealth>
}

export interface LogError {
  file: string
  lineNo: number
  level: string
  line: string
}

export interface Capabilities {
  python: string
  server: string
  ts: string
  flake8: boolean
  pylint: boolean
  complexity: boolean
  numpy: boolean
  pandas: boolean
  polars: boolean
  sklearn: boolean
  lightgbm: boolean
  torch: boolean
  tensorflow: boolean
  scipy: boolean
  opencv: boolean
  plotly: boolean
  spacy: boolean
  icecream: boolean
  cython: boolean
  cython_version?: string
  [key: string]: unknown
}

export type NotebookEnvId = 'thebe' | 'pyodide' | 'starboard'

export interface NotebookEnv {
  id: NotebookEnvId
  name: string
  description: string
  npmPackage: string
  version: string
  color: string
  useCases: string[]
}
