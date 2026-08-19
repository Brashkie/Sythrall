// ══════════════════════════════════════════
//  Sythrall — Editor Intelligence
//  Fase 1: Real-time linting + Big-O hover
//  Fase 2: Go to Definition + Find References
//  Fase 3: Autocomplete + Rename + Call Graph
//
//  Fast path  (debounce 300ms):  /intel/lint
//  Heavy path (idle 2000ms):     /intel/analyze
//  Hover:                        /intel/hover
//  Definition:                   /intel/definition  (F12)
//  References:                   /intel/references  (Shift+F12)
//  Completions:                  /intel/completions (Ctrl+Space)
//  Rename:                       /intel/rename      (F2)
//  Call Graph:                   /static/parse
// ══════════════════════════════════════════

import * as monaco from 'monaco-editor'
import { getApiBase } from '../store/state'
import { appendLog } from '../utils/helpers'

// ─── Estado ───────────────────────────────────────────────────────────────────

let _editor: monaco.editor.IStandaloneCodeEditor | null = null
let _model: monaco.editor.ITextModel | null = null
let _fastTimer: ReturnType<typeof setTimeout> | null = null
let _heavyTimer: ReturnType<typeof setTimeout> | null = null
let _currentFile: string = ''
let _initialized = false

// Todos los disposables en un array — se limpian en teardown
const _disposables: monaco.IDisposable[] = []

// Markers por fuente (se mergean antes de aplicar)
const _markers: Record<'fast' | 'heavy', monaco.editor.IMarkerData[]> = {
  fast: [],
  heavy: [],
}

// ─── Tiempos ──────────────────────────────────────────────────────────────────

const FAST_DEBOUNCE_MS = 300
const HEAVY_IDLE_MS = 2000
const FAST_TIMEOUT_MS = 5_000
const HEAVY_TIMEOUT_MS = 15_000

// ══════════════════════════════════════════
//  INIT / TEARDOWN
// ══════════════════════════════════════════

/**
 * Conecta toda la inteligencia del editor a una instancia Monaco.
 * Llamar desde editor.ts después de monaco.editor.create()
 */
export function initEditorIntelligence(editor: monaco.editor.IStandaloneCodeEditor): void {
  if (_initialized) teardownIntelligence()

  _editor = editor
  _initialized = true

  _disposables.push(editor.onDidChangeModelContent(() => _scheduleAnalysis()))
  _disposables.push(
    editor.onDidChangeModel(() => {
      _model = editor.getModel()
      _currentFile = _model?.uri.path.split('/').pop() ?? 'script.py'
      _clearAllMarkers()
      _scheduleAnalysis()
    }),
  )

  _model = editor.getModel()
  _currentFile = _model?.uri.path.split('/').pop() ?? 'script.py'

  // Registrar providers de las 3 fases
  _registerHoverProviders()
  _registerDefinitionProviders()
  _registerReferenceProviders()
  _registerCompletionProvider()
  _registerRenameProvider()

  if (_model?.getValue().trim()) {
    _scheduleAnalysis()
  }
}

/**
 * Llamar cuando se carga un nuevo archivo en el editor.
 */
export function notifyFileChanged(filename: string): void {
  _currentFile = filename
  _clearAllMarkers()
  _scheduleAnalysis()
}

function teardownIntelligence(): void {
  if (_fastTimer) clearTimeout(_fastTimer)
  if (_heavyTimer) clearTimeout(_heavyTimer)
  _disposables.forEach((d) => {
    d.dispose()
  })
  _disposables.length = 0
  _fastTimer = null
  _heavyTimer = null
  _initialized = false
  _markers.fast = []
  _markers.heavy = []
}

// ══════════════════════════════════════════
//  FASE 1 — Linting en tiempo real
// ══════════════════════════════════════════

function _scheduleAnalysis(): void {
  if (_fastTimer) clearTimeout(_fastTimer)
  if (_heavyTimer) clearTimeout(_heavyTimer)
  _fastTimer = setTimeout(_runFastLint, FAST_DEBOUNCE_MS)
  _heavyTimer = setTimeout(_runHeavyAnalyze, HEAVY_IDLE_MS)
}

async function _runFastLint(): Promise<void> {
  const content = _model?.getValue() ?? ''
  if (!content.trim() || !_currentFile) return
  try {
    const ctrl = new AbortController()
    const tid = setTimeout(() => ctrl.abort(), FAST_TIMEOUT_MS)
    const res = await fetch(getApiBase() + '/intel/lint', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ filename: _currentFile, content }),
      signal: ctrl.signal,
    })
    clearTimeout(tid)
    if (!res.ok) return
    const data = (await res.json()) as { markers: RawMarker[]; ms: number }
    _markers.fast = data.markers.map(_toMonacoMarker)
    _applyMarkers()
  } catch {
    /* silencioso */
  }
}

async function _runHeavyAnalyze(): Promise<void> {
  const content = _model?.getValue() ?? ''
  if (!content.trim() || !_currentFile) return
  try {
    const ctrl = new AbortController()
    const tid = setTimeout(() => ctrl.abort(), HEAVY_TIMEOUT_MS)
    const res = await fetch(getApiBase() + '/intel/analyze', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ filename: _currentFile, content, tools: ['ast', 'flake8', 'complexity'] }),
      signal: ctrl.signal,
    })
    clearTimeout(tid)
    if (!res.ok) return
    const data = (await res.json()) as HeavyResult
    _markers.heavy = data.markers.map(_toMonacoMarker)
    _applyMarkers()
    const hot = data.big_o.filter((f) => ['O(n²)', 'O(n³)', 'O(2^n)'].includes(f.big_o))
    if (hot.length) {
      appendLog('warn', `Hot paths en ${_currentFile}: ${hot.map((f) => `${f.name}(${f.big_o})`).join(', ')}`, 'be')
    }
  } catch (e) {
    if ((e as Error).name !== 'AbortError') {
      appendLog('warn', `heavy analyze: ${(e as Error).message}`, 'fe')
    }
  }
}

function _applyMarkers(): void {
  if (!_model) return
  const all = [..._markers.fast, ..._markers.heavy]
  const seen = new Set<string>()
  const dedup: monaco.editor.IMarkerData[] = []
  for (const m of all) {
    const key = `${m.startLineNumber}:${m.code}`
    if (!seen.has(key)) {
      seen.add(key)
      dedup.push(m)
    }
  }
  monaco.editor.setModelMarkers(_model, 'sythrall-intel', dedup)
}

function _clearAllMarkers(): void {
  _markers.fast = []
  _markers.heavy = []
  if (_model) monaco.editor.setModelMarkers(_model, 'sythrall-intel', [])
}

function _toMonacoMarker(raw: RawMarker): monaco.editor.IMarkerData {
  const sev = ((): monaco.MarkerSeverity => {
    switch (raw.severity) {
      case 8:
        return monaco.MarkerSeverity.Error
      case 4:
        return monaco.MarkerSeverity.Warning
      case 2:
        return monaco.MarkerSeverity.Info
      default:
        return monaco.MarkerSeverity.Hint
    }
  })()
  return {
    startLineNumber: raw.startLineNumber,
    startColumn: raw.startColumn,
    endLineNumber: raw.endLineNumber,
    endColumn: raw.endColumn,
    message: raw.message,
    severity: sev,
    source: raw.source,
    code: raw.code,
  }
}

// ── Hover ─────────────────────────────────────────────────────────────────────

function _registerHoverProviders(): void {
  for (const lang of ['python', 'typescript', 'javascript']) {
    _disposables.push(
      monaco.languages.registerHoverProvider(lang, {
        provideHover: (model, position) => _fetchHover(model, position),
      }),
    )
  }
}

async function _fetchHover(
  model: monaco.editor.ITextModel,
  position: monaco.Position,
): Promise<monaco.languages.Hover | null> {
  const word = model.getWordAtPosition(position)
  if (!word || word.word.length < 2) return null
  const content = model.getValue()
  const filename = _currentFile || model.uri.path.split('/').pop() || 'script.py'
  try {
    const ctrl = new AbortController()
    const tid = setTimeout(() => ctrl.abort(), 3000)
    const res = await fetch(getApiBase() + '/intel/hover', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        filename,
        content,
        line: position.lineNumber,
        column: position.column,
        symbol_name: word.word,
      }),
      signal: ctrl.signal,
    })
    clearTimeout(tid)
    if (!res.ok) return null
    const data = (await res.json()) as HoverResponse
    if (!data.markdown) return null
    const hover: monaco.languages.Hover = {
      contents: [{ value: data.markdown, isTrusted: true, supportThemeIcons: true }],
    }
    if (data.range) {
      hover.range = new monaco.Range(
        data.range.startLineNumber,
        data.range.startColumn ?? 1,
        data.range.endLineNumber,
        data.range.endColumn ?? 999,
      )
    }
    return hover
  } catch {
    return null
  }
}

// ══════════════════════════════════════════
//  FASE 2 — Go to Definition + Find References
// ══════════════════════════════════════════

function _registerDefinitionProviders(): void {
  for (const lang of ['python', 'typescript', 'javascript']) {
    _disposables.push(
      monaco.languages.registerDefinitionProvider(lang, {
        provideDefinition: (model, position) => _fetchDefinition(model, position),
      }),
    )
  }
}

function _registerReferenceProviders(): void {
  for (const lang of ['python', 'typescript', 'javascript']) {
    _disposables.push(
      monaco.languages.registerReferenceProvider(lang, {
        provideReferences: (model, position, _ctx) => _fetchReferences(model, position),
      }),
    )
  }
}

async function _fetchDefinition(
  model: monaco.editor.ITextModel,
  position: monaco.Position,
): Promise<monaco.languages.Definition | null> {
  const word = model.getWordAtPosition(position)
  if (!word || word.word.length < 2) return null
  const content = model.getValue()
  const filename = _currentFile || model.uri.path.split('/').pop() || 'script.py'
  try {
    const ctrl = new AbortController()
    const tid = setTimeout(() => ctrl.abort(), 4000)
    const res = await fetch(getApiBase() + '/intel/definition', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        filename,
        content,
        line: position.lineNumber,
        column: position.column,
        symbol_name: word.word,
      }),
      signal: ctrl.signal,
    })
    clearTimeout(tid)
    if (!res.ok) return null
    const data = (await res.json()) as DefinitionResponse
    if (!data.found || !data.definitions.length) return null
    return data.definitions.map((defn) => ({
      uri: model.uri,
      range: new monaco.Range(defn.line, defn.column, defn.end_line ?? defn.line, 999),
    }))
  } catch {
    return null
  }
}

async function _fetchReferences(
  model: monaco.editor.ITextModel,
  position: monaco.Position,
): Promise<monaco.languages.Location[] | null> {
  const word = model.getWordAtPosition(position)
  if (!word || word.word.length < 2) return null
  const content = model.getValue()
  const filename = _currentFile || model.uri.path.split('/').pop() || 'script.py'
  try {
    const ctrl = new AbortController()
    const tid = setTimeout(() => ctrl.abort(), 5000)
    const res = await fetch(getApiBase() + '/intel/references', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ filename, content, symbol_name: word.word }),
      signal: ctrl.signal,
    })
    clearTimeout(tid)
    if (!res.ok) return null
    const data = (await res.json()) as ReferencesResponse
    if (!data.references.length) return null
    return data.references.map((ref) => ({
      uri: model.uri,
      range: new monaco.Range(ref.line, ref.column, ref.line, ref.column + word.word.length),
    }))
  } catch {
    return null
  }
}

// ══════════════════════════════════════════
//  FASE 3 — Autocomplete + Rename + Call Graph
// ══════════════════════════════════════════

const KIND_MAP: Record<string, monaco.languages.CompletionItemKind> = {
  function: monaco.languages.CompletionItemKind.Function,
  method: monaco.languages.CompletionItemKind.Method,
  class: monaco.languages.CompletionItemKind.Class,
  variable: monaco.languages.CompletionItemKind.Variable,
  import: monaco.languages.CompletionItemKind.Module,
  interface: monaco.languages.CompletionItemKind.Interface,
  type: monaco.languages.CompletionItemKind.TypeParameter,
}

function _registerCompletionProvider(): void {
  for (const lang of ['python', 'typescript', 'javascript']) {
    _disposables.push(
      monaco.languages.registerCompletionItemProvider(lang, {
        triggerCharacters: ['.', ' '],
        provideCompletionItems: async (model, position) => {
          const content = model.getValue()
          const filename = _currentFile || model.uri.path.split('/').pop() || 'script.py'
          const word = model.getWordUntilPosition(position)
          const prefix = word.word
          try {
            const ctrl = new AbortController()
            const tid = setTimeout(() => ctrl.abort(), 3000)
            const res = await fetch(getApiBase() + '/intel/completions', {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ filename, content, prefix }),
              signal: ctrl.signal,
            })
            clearTimeout(tid)
            if (!res.ok) return { suggestions: [] }
            const data = (await res.json()) as CompletionsResponse
            const range = {
              startLineNumber: position.lineNumber,
              endLineNumber: position.lineNumber,
              startColumn: word.startColumn,
              endColumn: word.endColumn,
            }
            const suggestions: monaco.languages.CompletionItem[] = data.symbols.map((sym) => ({
              label: sym.label,
              kind: KIND_MAP[sym.kind] ?? monaco.languages.CompletionItemKind.Text,
              detail: sym.detail,
              documentation: sym.documentation ? { value: sym.documentation, isTrusted: true } : undefined,
              insertText: sym.insert_text,
              insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
              sortText: sym.sort_text,
              range,
            }))
            return { suggestions, incomplete: false }
          } catch {
            return { suggestions: [] }
          }
        },
      }),
    )
  }
}

function _registerRenameProvider(): void {
  for (const lang of ['python', 'typescript', 'javascript']) {
    _disposables.push(
      monaco.languages.registerRenameProvider(lang, {
        provideRenameEdits: async (
          model: monaco.editor.ITextModel,
          position: monaco.Position,
          newName: string,
        ): Promise<monaco.languages.WorkspaceEdit & monaco.languages.Rejection> => {
          const word = model.getWordAtPosition(position)
          if (!word) return { edits: [], rejectReason: 'No hay símbolo' }

          const content = model.getValue()
          const filename = _currentFile || model.uri.path.split('/').pop() || 'script.py'

          try {
            const ctrl = new AbortController()
            const tid = setTimeout(() => ctrl.abort(), 5000)
            const res = await fetch(getApiBase() + '/intel/rename', {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ filename, content, symbol_name: word.word, new_name: newName }),
              signal: ctrl.signal,
            })
            clearTimeout(tid)
            if (!res.ok) return { edits: [], rejectReason: 'Error de red' }

            const data = (await res.json()) as RenameResponse
            if (!data.valid) return { edits: [], rejectReason: data.error ?? 'No se puede renombrar' }

            const edits: monaco.languages.IWorkspaceTextEdit[] = data.edits.map((e) => ({
              resource: model.uri,
              versionId: model.getVersionId(),
              textEdit: {
                range: new monaco.Range(
                  e.range.startLineNumber,
                  e.range.startColumn,
                  e.range.endLineNumber,
                  e.range.endColumn,
                ),
                text: data.new_name,
              },
            }))

            return { edits }
          } catch {
            return { edits: [], rejectReason: 'Error inesperado' }
          }
        },

        resolveRenameLocation: async (
          model: monaco.editor.ITextModel,
          position: monaco.Position,
        ): Promise<monaco.languages.RenameLocation & monaco.languages.Rejection> => {
          const word = model.getWordAtPosition(position)
          if (!word || word.word.length < 2) {
            return { rejectReason: 'No hay símbolo bajo el cursor', range: new monaco.Range(1, 1, 1, 1), text: '' }
          }
          return {
            range: new monaco.Range(position.lineNumber, word.startColumn, position.lineNumber, word.endColumn),
            text: word.word,
          }
        },
      }),
    )
  }
}

// ══════════════════════════════════════════
//  Tipos internos
// ══════════════════════════════════════════

interface RawMarker {
  startLineNumber: number
  startColumn: number
  endLineNumber: number
  endColumn: number
  message: string
  severity: number
  source: string
  code: string
}

interface HeavyResult {
  markers: RawMarker[]
  big_o: Array<{ name: string; line: number; big_o: string; complexity: number }>
  metrics: Record<string, unknown>
  ms: number
}

interface HoverResponse {
  markdown: string
  range: { startLineNumber: number; startColumn?: number; endLineNumber: number; endColumn?: number } | null
}

interface DefinitionResponse {
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

interface ReferencesResponse {
  symbol: string
  filename: string
  references: Array<{ line: number; column: number; kind: string; preview: string }>
  definition_line: number | null
  total: number
}

interface CompletionSymbol {
  label: string
  kind: string
  detail: string
  documentation: string | null
  insert_text: string
  sort_text: string
  line: number
}

interface CompletionsResponse {
  symbols: CompletionSymbol[]
  total: number
  filename: string
  prefix: string
}

interface RenameEdit {
  range: { startLineNumber: number; startColumn: number; endLineNumber: number; endColumn: number }
  newText: string
  kind: string
  preview: string
}

interface RenameResponse {
  valid: boolean
  edits: RenameEdit[]
  total: number
  old_name: string
  new_name: string
  filename: string
  error?: string
}
