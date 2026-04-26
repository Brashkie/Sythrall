// ══════════════════════════════════════════
//  CodeWatch PRO — Types
// ══════════════════════════════════════════
export type Severity  = 'error' | 'warning' | 'info'
export type Status    = 'ok' | 'warning' | 'down' | 'unknown' | 'error'
export type StepState = 'idle' | 'run' | 'ok' | 'err' | 'warn'

// 'upload' agregado para el nuevo panel de proyectos
export type TabId =
  | 'dashboard' | 'editor' | 'apis'    | 'issues'
  | 'diagram'   | 'ml'     | 'metrics' | 'diff'
  | 'logs'      | 'upload' | 'static'

export interface CodeFile {
  id:       string
  name:     string
  ext:      string
  size:     number
  content:  string
  issues:   Issue[]
  metrics:  FileMetrics
  analyzed: boolean
}

export interface LogFile {
  name:    string
  size:    number
  content: string
}

export interface Issue {
  tool:        string
  line?:       number
  col?:        number
  severity:    Severity
  code?:       string
  message:     string
  preview?:    string
  file?:       string
  symbol?:     string
  suggestion?: string
  category?:   string
}

export interface ApiResult {
  url:           string
  status:        Status
  code:          string | number | null
  ms:            number | null
  error:         string | null
  ts:            string | null
  history:       HistoryEntry[]
  headers?:      Record<string, string>
  content_type?: string
  json_preview?: string
}

export interface HistoryEntry {
  ts:     string
  status: Status
  ms:     number | null
  code?:  string | number | null
}

export interface FileMetrics {
  pylint_score?: number
  complexity?:   ComplexityEntry[]
  mi?:           number
  raw?:          RawStats
  tools_used?:   string[]
}

export interface ComplexityEntry {
  name:       string
  type:       string
  line:       number
  complexity: number
  rank:       string
}

export interface RawStats {
  loc:      number
  lloc:     number
  sloc:     number
  comments: number
  blank:    number
  multi:    number
}

export interface MLLibrary {
  name:     string
  category: string
  color:    string
  import:   string
  alias?:   string | null
  version?: string | null
}

export interface PipelineStep {
  id:          string
  description: string
  icon:        string
  line:        number
  count:       number
}

export interface MLModel {
  name:      string
  type:      string
  family:    string
  framework: string
  line:      number
}

export interface MLMetric {
  found: boolean
  count: number
  value: number | null
}

export interface MLAnalysisResult {
  filename:    string
  ts:          string
  libraries:   MLLibrary[]
  pipeline:    PipelineStep[]
  models:      MLModel[]
  metrics:     Record<string, MLMetric>
  issues:      Issue[]
  diagram:     string
  score:       number
  suggestions: string[]
}

export interface AnalysisResult {
  filename:        string
  ts:              string
  issues:          Issue[]
  metrics:         { pylint_score?: number }
  complexity:      ComplexityEntry[]
  maintainability: number | null
  raw_stats:       RawStats
  tools_used:      string[]
}

export interface RunHistoryEntry {
  ts:     string
  issues: number
  apiOk:  number
  ms:     number
}

export interface AppState {
  files:      CodeFile[]
  logFiles:   LogFile[]
  urls:       string[]
  results: {
    apis:      ApiResult[]
    issues:    Issue[]
    logErrors: LogError[]
  }
  running:        boolean
  autoOn:         boolean
  autoTimer:      ReturnType<typeof setInterval> | null
  history:        RunHistoryEntry[]
  steps:          Record<string, StepState>
  currentFile:    CodeFile | null
  backendOk:      boolean
  currentMermaid: string
}

export interface LogError {
  file:   string
  lineNo: number
  level:  string
  line:   string
}

export interface Capabilities {
  python:           string
  server:           string
  ts:               string
  flake8:           boolean
  pylint:           boolean
  radon:            boolean
  numpy:            boolean
  pandas:           boolean
  polars:           boolean
  sklearn:          boolean
  lightgbm:         boolean
  torch:            boolean
  tensorflow:       boolean
  scipy:            boolean
  opencv:           boolean
  plotly:           boolean
  spacy:            boolean
  icecream:         boolean
  cython:           boolean
  cython_version?:  string
  [key: string]:    unknown
}

export type NotebookEnvId = 'thebe' | 'pyodide' | 'starboard'

export interface NotebookEnv {
  id:          NotebookEnvId
  name:        string
  description: string
  npmPackage:  string
  version:     string
  color:       string
  icon:        string
  useCases:    string[]
}
