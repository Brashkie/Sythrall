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
    | 'unreachable_code'
    | 'condition_always_true'
    | 'condition_always_false'
    | 'division_by_zero'
    | 'possible_division_by_zero'
    | 'index_out_of_range'
    | 'possible_index_out_of_range'
    | 'none_dereference'
    | 'possible_none_dereference'
    | 'redundant_condition'
    | 'contradictory_condition'
    | 'tautological_condition'
    | 'duplicated_condition'
    | 'absorbable_condition'
    | 'unbounded_queue'
    | 'unbounded_cache'
    | 'unbounded_task_spawn'
    | 'unbounded_convergence_loop'
    | 'catastrophic_cancellation_step'
    | 'float_equality_comparison'
    | 'undefined_state_transition'
    | 'unreachable_state'
    | 'unhandled_state'
    | 'recursive_template_no_base_case'
    | 'deep_template_recursion'
    | 'linear_op_in_loop'
    | 'unwrap_in_fallible_fn'
    | 'ts_ignore_instead_of_expect_error'
    | 'double_type_assertion'
    | 'bare_type_ignore'
    | 'dead_store'
    | 'overwritten_store'
    | 'potential_false_sharing'
    | 'sfinae_instead_of_concepts'
    | 'signed_unsigned_comparison'
    | 'implicit_precision_loss'
    | 'int_pointer_cast'
    | 'missing_implicit_none'
    | 'single_precision_literal_in_double'
    | 'integer_division_to_real'
    | 'char_signedness_assumption'
    | 'factorial_brute_force'
    | 'undefined_grammar_rule'
    | 'unreferenced_grammar_rule'
    | 'unsafe_block_without_safety_comment'
    | 'unsafe_fn_without_safety_doc'
    | 'mutable_default_argument'
  name: string
  line: number
  message: string
  /** El motor lo deriva de `kind` al serializar — ningun emisor lo escribe,
   * asi que esta siempre presente. `unclassified` es un kind que todavia no
   * entro en la tabla de `decidibilidad.rs`, y se dice en vez de callarse. */
  decidability?: 'decidable' | 'approximate' | 'unclassified'
  /** Por que ese lado. Ausente cuando el kind no esta clasificado: no se
   * inventa una explicacion para algo que nadie decidio. */
  decidability_reason?: string
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

// Fase 28 (Enterprise Intelligence) — snapshot local de Project Health por
// proyecto (`services/history_store.py`, SQLite, sin auth todavía). `health`
// es el mismo `ProjectHealth` de arriba; `summary` es un subconjunto de
// `StaticProjectResult['summary']` (sin `big_o_distribution`, que no tiene
// un delta aritmético con sentido — ver `routers/history.py`).
export interface HistorySnapshot {
  id: number
  project_id: string
  created_at: string
  health: ProjectHealth
  summary: Record<string, number>
}

export interface HistoryDiffEntry {
  from: number
  to: number
  delta: number
}

export interface HistoryComparison {
  from: HistorySnapshot
  to: HistorySnapshot
  health_diff: Record<keyof ProjectHealth, HistoryDiffEntry>
  summary_diff: Record<string, HistoryDiffEntry>
  /** Qué cambió en el ANALIZADOR entre las dos mediciones (Fase 28,
   * 2026-09-05). Es la diferencia entre "tu código empeoró" y "Sythrall
   * aprendió a ver algo que antes no veía": cuando un lenguaje pasa de
   * devolver `[]` fijo a emitir hallazgos reales, el score baja sin que se
   * haya tocado una línea. `known: false` = uno de los snapshots es
   * anterior a que esto se guardara, así que no se puede distinguir. */
  capabilities_diff: {
    known: boolean
    comparable: boolean
    added: string[]
    removed: string[]
    note: string
  }
}

// Panorama entre proyectos (Fase 28, segundo bullet) — el último snapshot
// guardado de CADA proyecto, más su delta contra el anterior. `project_name`
// es `null` y `exists` es `false` cuando el proyecto ya se borró del disco
// pero su historial sigue en la base (la UI lo muestra como tal, no lo
// esconde). `delta` es `null` cuando el proyecto tiene un solo snapshot —
// todavía no hay contra qué comparar.
export interface HistoryOverviewProject {
  project_id: string
  project_name: string | null
  exists: boolean
  snapshots: number
  created_at: string
  health: ProjectHealth
  summary: Record<string, number>
  overall: number
  delta: Record<keyof ProjectHealth, number> | null
}

export interface HistoryOverview {
  projects: HistoryOverviewProject[]
  aggregate: {
    projects: number
    snapshots: number
    avg: Record<keyof ProjectHealth, number> & { overall: number }
    totals: Record<string, number>
  }
  note: string
}

/** Reporte de referencias cruzadas CSS. `unreferenced` NO significa "sin
 * usar": ver `note`, que trae el hedge completo del motor. */
export interface CssUsageReport {
  available: boolean
  reason?: string
  css_files?: number
  scanned_files?: number
  defined_classes?: number
  referenced_classes?: number
  unreferenced?: Array<{ name: string; file: string; line: number }>
  dynamic_sites?: Array<{ file: string; line: number; snippet: string }>
  note?: string
}

/** Fase 19 — uso de SIMD de un procedimiento de ensamblador.
 *
 * `vectorized` mira SOLO las instrucciones packed: usar un registro vectorial
 * no alcanza. `addss` opera sobre un elemento con un registro de 128 bits y es
 * lo que emite cualquier compilador para un `float`; llamar a eso
 * "vectorizado" daría un sí para casi todo programa. */
export interface SimdInfo {
  isa: 'sse' | 'avx' | 'avx2' | 'avx512'
  max_width_bits: number
  packed_instructions: number
  scalar_simd_instructions: number
  vector_registers: string[]
  vectorized: boolean
}

export interface AsmInstruction {
  line: number
  mnemonic: string
  operands: string[]
  category: string
  explanation: string
}

export interface AsmProcedure {
  name: string
  line: number
  loc: number
  big_o: string
  big_o_reason: string
  instructions: AsmInstruction[]
  registers_used: string[]
  calls: string[]
  simd: SimdInfo | null
}

/** Una compilación a ensamblador, por nivel de optimización. */
export interface CompilacionAsm {
  available: boolean
  language: string
  toolchain: string
  toolchain_version: string
  opt_level: string
  features: string[]
  assembly: string
  assembly_lines: number
  assembly_truncated: boolean
  analysis: { syntax: string; procedures: AsmProcedure[] } | null
  error: string | null
}

export interface CompileAsmResponse {
  available: boolean
  results: CompilacionAsm[]
  error: string | null
}

/** Un punto de la búsqueda de configuración. `dominated_by` vacío = está en
 *  el frente. */
export interface PuntoPareto {
  label: string
  config: { opt_level: string; features: string[] }
  instructions: number
  packed: number
  assembly_lines: number
  vectorized: boolean
  dominated_by: string[]
  error: string | null
}

export interface BusquedaPareto {
  language: string
  toolchain: string
  toolchain_version: string
  points: PuntoPareto[]
  front: string[]
  note: string
  error: string | null
}

export interface ParetoResponse {
  available: boolean
  result: BusquedaPareto | null
  error: string | null
}

/** Fase 34, Línea C — la curva de costo de metaprogramación en D. Mismo
 *  shape que `TemplateCostResult`, a propósito: comparar dos curvas con
 *  formas distintas no compararía nada. */
export interface DCostResult {
  available: boolean
  compiler: string | null
  points: Array<{ depth: number; compile_ms: number; object_bytes: number; symbols: number }>
  symbols_per_level: number | null
  bytes_per_level: number | null
  note: string
}

export interface ComparacionMetaprogramacion {
  cpp: TemplateCostResult
  d: DCostResult
  comparable: boolean
  cpp_symbols_per_level: number | null
  d_symbols_per_level: number | null
  ambas_lineales: boolean | null
  note: string
}

export interface AtomoCss {
  class: string
  property: string
  value: string
  important: boolean
  at_context: string | null
  uses: number
  selectors: string[]
}

export interface AtomizacionResult {
  atoms: AtomoCss[]
  stylesheet: string
  mapping: Array<{ selector: string; at_context: string | null; classes: string[] }>
  summary: {
    declarations_before: number
    atoms_after: number
    bytes_before: number
    bytes_after: number
    reuse_ratio: number
    single_use_atoms: number
    top_atom: string | null
    top_atom_uses: number
  }
  note: string
}

export interface AtomizacionResponse {
  available: boolean
  result: AtomizacionResult | null
  error: string | null
}

export interface StaticParseResult {
  filename: string
  language: string
  functions: StaticFunction[]
  classes: StaticClass[]
  imports: StaticImport[]
  /** JS/TS traen `{ name, line }`; Ada agrega `kind` (declaracion o entry)
   * y `params`. Opcionales porque los otros lenguajes no los mandan. */
  exports: Array<{ name: string; line: number; kind?: string; params?: number }>
  interfaces?: Array<{ name: string; line: number }>
  types?: Array<{ name: string; line: number }>
  dead_code: Array<{ type: string; name?: string; module: string; line: number }>
  call_graph: Array<{ from: string; to: string }>
  wasm_hints: WasmHint[]
  memory_layout?: MemoryLayoutResult
  /** Fase 34, Línea A — métricas de FORMA de la metaprogramación de C++
   * (`plantillas.rs`). Ausente o en cero para todo lo que no sea C++.
   *
   * **`instantiation_sites` y `distinct_instantiations` se equivocan en
   * direcciones opuestas, y por eso están los dos**: el primero cuenta usos
   * escritos (`vector<int>` 50 veces son 50 sitios y UNA instanciación real,
   * sobrecuenta) y el segundo deduplica por texto (`Factorial<N - 1>` es un
   * texto y N instanciaciones reales, subcuenta). Ninguno es una predicción
   * del trabajo del compilador: saber eso exige compilar. */
  template_complexity?: TemplateComplexity
  security_findings: SecurityFinding[]
  structural_smells: StructuralSmell[]
  naming_smells: NamingSmell[]
  summary: Record<string, number | string>
  error?: string
}

export interface TemplateComplexity {
  templates: number
  specializations: number
  instantiation_sites: number
  distinct_instantiations: number
  constexpr_branches: number
  static_asserts: number
  /** Fase 32 — cómo se restringen las plantillas de este archivo.
   * Opcionales: un resultado viejo en caché no los trae. Ver `sfinae.rs`. */
  concepts_defined?: number
  requires_clauses?: number
  enable_if_sites?: number
  decltype_returns?: number
  recursive_templates: string[]
}

/** Fase 34, Línea A — la mitad MEDIDA del costo de la metaprogramación.
 * `object_bytes` es determinístico y crece linealmente con la profundidad;
 * `compile_ms` a estas profundidades está dominado por el arranque del
 * compilador, así que es contexto, no la medición de los templates. */
export interface TemplateCostResult {
  available: boolean
  compiler: string | null
  points: Array<{ depth: number; compile_ms: number; object_bytes: number; symbols: number }>
  /** Símbolos que emite cada nivel más de instanciación. Medido, no
   * estimado — es el número que el ROADMAP dejó escrito como NO
   * calculable estáticamente. */
  symbols_per_level: number | null
  bytes_per_level: number | null
  note: string
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
    modernization_candidates: number
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
  /** Fase 25 — rollup por archivo (2026-09-01), deliberadamente NO un
   * veredicto Keep/Refactor/Replace/Rewrite/Migrate: agrupa los candidatos
   * de `modernization.rs` por archivo con conteos por confianza/patrón,
   * sin inventar ninguna categoría que la evidencia no sostenga — ver el
   * comentario en `routers/static_analysis.py`. */
  modernization_candidates: Array<{
    variable: string
    pattern: string
    line: number
    current: string
    suggested_target: string
    reasoning: string
    confidence: 'medium' | 'high'
    file: string
  }>
  modernization_by_file: Array<{
    file: string
    total: number
    high: number
    medium: number
    patterns: Record<string, number>
  }>
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

/** Fase 26 — segunda pieza de "migrar de numpy/pandas/scikit-learn": compara
 * el kernel Zig de agregación por categoría YA validado contra pandas real
 * (`groupby().agg()`), mismos tamaños/datos, side by side. A diferencia de
 * `MatmulVsNumpyResult` (donde numpy gana), acá gana el kernel nativo — el
 * `comparison_note` incluye la salvedad de que asume categorías fijas.
 * `pandas.available` es `false` cuando pandas no está instalado en el
 * entorno del backend, degrada con gracia igual que el resto. */
export interface AggregationVsPandasResult {
  zig: EmpiricalValidationResult
  pandas: {
    available: boolean
    measurements: Array<{ n: number; seconds: number }>
    note: string
  }
  comparison_note: string
}

/** Fase 19 (Machine Intelligence) — segundo bullet, cierra la fase: un
 * ejecutable/librería ya compilado (`.exe`/`.dll`/`.so`/`.dylib`/`.o`), NO
 * código fuente — formato/arquitectura/punto de entrada, secciones,
 * símbolos, imports/exports, vía el crate `object` en el sidecar Rust
 * (`binparse.rs`). `truncated` avisa si alguna lista se cortó en 500
 * elementos (un binario real puede tener muchísimos más). `available:
 * false` cuando el archivo no es un binario reconocido, pesa más de 64MB, o
 * el sidecar no está disponible — degrada con gracia igual que el resto. */
/** Fase 26 — tercera y última pieza de "migrar de numpy/pandas/scikit-learn":
 * compara el kernel Fortran de regresión lineal YA validado contra
 * scikit-learn real (`LinearRegression.fit()`), mismos tamaños/datos, side
 * by side. Igual que `AggregationVsPandasResult`, gana el kernel nativo —
 * el `comparison_note` incluye la salvedad de que asume una sola variable
 * predictora. `sklearn.available` es `false` cuando scikit-learn no está
 * instalado en el entorno del backend, degrada con gracia igual que el resto. */
export interface LinregVsSklearnResult {
  fortran: EmpiricalValidationResult
  sklearn: {
    available: boolean
    measurements: Array<{ n: number; seconds: number }>
    note: string
  }
  comparison_note: string
}

export interface ExecutableParseResult {
  available: boolean
  format: string
  architecture: string
  is_64_bit: boolean
  entry_point: number
  sections: Array<{ name: string; address: number; size: number; kind: string }>
  symbols: Array<{ name: string; address: number; kind: string; is_undefined: boolean; is_global: boolean }>
  imports: Array<{ library: string; name: string }>
  exports: Array<{ name: string; address: number }>
  truncated: boolean
  note: string
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

/** **SPARK: la tercera categoria de evidencia.**
 *
 * Lo que viene como `spark_proved` NO lo probo Sythrall leyendo el codigo: lo
 * cerro un probador externo (Alt-Ergo, Z3) resolviendo una formula. Y lo que
 * viene como `spark_unproved` tampoco es un hallazgo: es una obligacion que
 * el prover no pudo cerrar con el tiempo y los axiomas que tenia. */
export interface SparkObligation {
  kind: 'spark_proved' | 'spark_unproved'
  rule: string
  file: string
  line: number
  message: string
}

export interface SparkResult {
  available: boolean
  prover?: string | null
  proved: number
  unproved: number
  obligations: SparkObligation[]
  note: string
}

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
  /** Sythrall CSS Engine — referencias cruzadas CSS ↔ marcado/scripts del
   * proyecto. `available: false` cuando el sidecar Rust no responde. */
  cssUsage: (projectId: string) => post<CssUsageReport>('/static/css-usage', { project_id: projectId }),

  staticParseProjectById: (projectId: string) =>
    post<StaticProjectResult>('/static/parse-project', { project_id: projectId }),

  // ── History (Fase 28 — Enterprise Intelligence, primera porción):
  // snapshots locales de Project Health por proyecto, sin auth todavía.
  createSnapshot: (projectId: string) => post<HistorySnapshot>(`/history/${projectId}/snapshot`, {}),

  listSnapshots: (projectId: string) => get<HistorySnapshot[]>(`/history/${projectId}`),

  compareSnapshots: (fromId: number, toId: number) => get<HistoryComparison>(`/history/compare/${fromId}/${toId}`),

  deleteSnapshot: (snapshotId: number) =>
    fetch(getApiBase() + `/history/snapshot/${snapshotId}`, { method: 'DELETE' }).then((r) => {
      if (!r.ok) throw new Error(`HTTP ${r.status}`)
      return r.json()
    }),

  /** Panorama entre proyectos (último snapshot de cada uno + agregados). */
  historyOverview: () => get<HistoryOverview>('/history/overview'),

  /** Capacidad real del motor por lenguaje (no una lista estática hardcodeada
   * en el frontend) — para el widget "Languages" del Dashboard. */
  staticLanguages: () => get<StaticLanguagesResult>('/static/languages'),

  /** Fase 19 — compila el fuente a ensamblador y lo analiza. `optLevels` es
   * una lista porque lo útil es la COMPARACIÓN; `features` pide extensiones
   * de CPU (sin `avx2` el compilador no vectoriza y la pregunta no se puede
   * responder). */
  /** Fase 32 — frente de Pareto sobre la configuración de compilación. El
   *  fuente no se toca: solo cambian las opciones, así que no hay nada que
   *  probar sobre equivalencia. */
  /** Fase 31 — Atomic CSS: la hoja atómica y la medición del canje. No
   *  reescribe nada. */
  cssAtomic: (filename: string, content: string) =>
    post<AtomizacionResponse>('/static/css-atomic', { filename, content }),

  compilePareto: (language: string, content: string) =>
    post<ParetoResponse>('/static/compile-pareto', { language, content, configs: [] }),

  compileAsm: (language: string, content: string, optLevels: string[], features: string[]) =>
    post<CompileAsmResponse>('/static/compile-asm', {
      language,
      content,
      opt_levels: optLevels,
      features,
    }),

  // ── Execution Intelligence (Fase 23) ──────────────────────────────────────

  /** Compila y corre un kernel Fortran real para validar empíricamente el
   * O(n³) detectado por forma en Fase 20 — puede tardar varios segundos
   * (compila + corre 4 tamaños), a diferencia de todo lo demás en este
   * cliente que es análisis estático instantáneo. */
  validateMatmulBigO: () => post<EmpiricalValidationResult>('/execution/validate-matmul', {}),
  /** Fase 34, Línea A — mide el costo de compilación de la recursión de
   * templates con una sonda propia; ver `tplcost.rs`. */
  validateTemplateCost: () => post<TemplateCostResult>('/execution/validate-template-cost', {}),

  /** Fase 34, Línea C — mide la misma curva en C++ y en D y las compara. */
  compareMetaprogramming: () => post<ComparacionMetaprogramacion>('/execution/metaprogramming-cost', {}),

  /** Fase 34, Linea E, paso 3 — corre GNATprove sobre un fuente Ada. Tarda
   * mucho mas que cualquier otro analisis: un probador resolviendo formulas
   * no es un parser. */
  verifySpark: (content: string) => post<SparkResult>('/execution/spark-verify', { content }),

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

  /** Fase 26 — séptimo kernel, tercera vez en Zig: agregación por categoría
   * (suma/promedio, la forma detrás de un `groupby().agg()`), valida O(n). */
  validateAggregationBigO: () => post<EmpiricalValidationResult>('/execution/validate-aggregation', {}),

  /** Fase 26 — segunda pieza de "migrar de numpy/pandas/scikit-learn":
   * corre el kernel Zig de agregación Y pandas real, mismos tamaños/datos,
   * y devuelve ambos lado a lado — acá gana el kernel nativo, a diferencia
   * de la pieza 1, con la salvedad de categorías fijas incluida en la nota. */
  compareAggregationVsPandas: () => post<AggregationVsPandasResult>('/execution/validate-aggregation-vs-pandas', {}),

  /** Fase 26 — octavo kernel, segunda vez en Fortran: regresión lineal
   * univariada por mínimos cuadrados (forma cerrada), valida O(n). */
  validateLinregBigO: () => post<EmpiricalValidationResult>('/execution/validate-linreg', {}),

  /** Fase 26 — tercera y última pieza de "migrar de numpy/pandas/scikit-learn":
   * corre el kernel Fortran de regresión lineal Y scikit-learn real, mismos
   * tamaños/datos, y devuelve ambos lado a lado — gana el kernel nativo,
   * mismo tipo de salvedad que la pieza 2 (pandas). */
  compareLinregVsSklearn: () => post<LinregVsSklearnResult>('/execution/validate-linreg-vs-sklearn', {}),

  /** Fase 26 — noveno kernel, cuarta vez en Zig y la primera en demostrar
   * recursión: quicksort recursivo (partición de Lomuto), valida la misma
   * forma O(n log n) que mergesort pero por una técnica genuinamente
   * distinta (divide y vencerás recursivo, no fusión iterativa). */
  validateQuicksortBigO: () => post<EmpiricalValidationResult>('/execution/validate-quicksort', {}),

  /** Fase 26 — décimo kernel, tercera vez en Assembly y la primera en
   * validar una forma SUB-LINEAL: búsqueda binaria iterativa. A diferencia
   * de todos los demás, `estimated_exponent` acá se ajusta contra
   * log2(tamaño del arreglo), no el tamaño en sí — un exponente ≈1.0
   * significa que el tiempo escala linealmente con log2(n), la firma real
   * de O(log n). Ver `binsearch_bench.rs` para la razón estadística. */
  validateBinarySearchBigO: () => post<EmpiricalValidationResult>('/execution/validate-binary-search', {}),

  /** Fase 19 (Machine Intelligence) — segundo bullet, cierra la fase: sube
   * un ejecutable/librería compilado (sin filtro de extensión, el formato
   * se detecta por firma binaria del lado Rust) y devuelve su estructura
   * real. `xhrPost` en vez de `post()` porque el body es `multipart/form-data`
   * (un archivo binario), no JSON — mismo mecanismo que `uploadZip`. */
  parseExecutable: (file: File) => {
    const form = new FormData()
    form.append('file', file, file.name)
    return xhrPost<ExecutableParseResult>('/binary/parse', form)
  },
}
