// ══════════════════════════════════════════
//  Sythrall — Monaco Editor
//  Workers con ?worker de Vite (sin plugins)
// ══════════════════════════════════════════
// src/components/editor.ts

// Importar workers con la sintaxis ?worker de Vite
import EditorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker'
import CssWorker from 'monaco-editor/esm/vs/language/css/css.worker?worker'
import HtmlWorker from 'monaco-editor/esm/vs/language/html/html.worker?worker'
import JsonWorker from 'monaco-editor/esm/vs/language/json/json.worker?worker'
import TsWorker from 'monaco-editor/esm/vs/language/typescript/ts.worker?worker'

import { initLiveMetrics, updateLiveMetricsContent, updateProblems } from '../panels/problems'
import type { CodeFile } from '../types'
import { debounce } from '../utils/helpers'
import { initEditorIntelligence, notifyFileChanged } from './editor-intelligence'

;(self as any).MonacoEnvironment = {
  getWorker(_: unknown, label: string) {
    if (label === 'json') return new JsonWorker()
    if (label === 'css' || label === 'scss' || label === 'less') return new CssWorker()
    if (label === 'html' || label === 'handlebars') return new HtmlWorker()
    if (label === 'typescript' || label === 'javascript') return new TsWorker()
    return new EditorWorker()
  },
}

import * as monaco from 'monaco-editor'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let editorInstance: any = null
let _currentFileName = ''

const EXT_LANG: Record<string, string> = {
  '.py': 'python',
  '.js': 'javascript',
  '.ts': 'typescript',
  '.json': 'json',
  '.yaml': 'yaml',
  '.yml': 'yaml',
  '.html': 'html',
  '.css': 'css',
  '.sh': 'shell',
  '.java': 'java',
  '.go': 'go',
  '.txt': 'plaintext',
}

export function initEditor(): void {
  monaco.editor.defineTheme('sythrall', {
    base: 'vs-dark',
    inherit: true,
    rules: [
      { token: 'comment', foreground: '4a5880', fontStyle: 'italic' },
      { token: 'keyword', foreground: 'b87dff' },
      { token: 'string', foreground: '00c07a' },
      { token: 'number', foreground: 'ffb627' },
      { token: 'type', foreground: '3d9eff' },
    ],
    colors: {
      'editor.background': '#0e1225',
      'editor.foreground': '#c8d4f0',
      'editorLineNumber.foreground': '#2d3768',
      'editor.lineHighlightBackground': '#141830',
      'editorCursor.foreground': '#3d9eff',
      'editorGutter.background': '#0e1225',
      'editor.selectionBackground': '#1a2040',
    },
  })

  const container = document.getElementById('editor-container')
  if (!container) {
    console.error('[Sythrall] editor-container no encontrado')
    return
  }

  editorInstance = monaco.editor.create(container, {
    theme: 'sythrall',
    language: 'python',
    value: '# Sythrall\n# Sube archivos o escribe aquí\n',
    fontSize: 13,
    fontFamily: "'DM Mono', 'Cascadia Code', monospace",
    fontLigatures: true,
    lineNumbers: 'on',
    minimap: { enabled: true },
    scrollBeyondLastLine: false,
    wordWrap: 'on',
    bracketPairColorization: { enabled: true },
    smoothScrolling: true,
    cursorBlinking: 'expand',
    padding: { top: 12, bottom: 12 },
    tabSize: 4,
    automaticLayout: true,
  })

  // Exponer helpers globales para el Explorer y otros módulos
  ;(window as any)['editorRelayout'] = () => editorInstance?.layout()
  ;(window as any)['editorGoToLine'] = (line: number) => {
    editorInstance?.revealLineInCenter(line)
    editorInstance?.setPosition({ lineNumber: line, column: 1 })
    editorInstance?.focus()
  }

  // Editor Intelligence (linting en tiempo real + hover + autocomplete)
  initEditorIntelligence(editorInstance)

  editorInstance.onDidChangeModelContent(
    debounce(() => {
      const content = editorInstance?.getValue()
      if (content === undefined) return
      document.dispatchEvent(new CustomEvent('editor:change', { detail: content }))
      if (_currentFileName) updateLiveMetricsContent(_currentFileName, content)
    }, 800),
  )

  console.log('[Sythrall] Monaco Editor ✓')
}

export function loadFileInEditor(file: CodeFile): void {
  if (!editorInstance) return
  _currentFileName = file.name
  const lang = EXT_LANG[file.ext] ?? 'plaintext'
  monaco.editor.setModelLanguage(editorInstance.getModel(), lang)
  editorInstance.setValue(file.content)
  const fnEl = document.getElementById('ed-fname')
  if (fnEl) {
    fnEl.textContent = file.name
    fnEl.style.display = ''
  }
  applyMarkers(file)
  initLiveMetrics(file.name, file.content)

  // Notificar al sistema de inteligencia que cambió el archivo
  notifyFileChanged(file.name)
}

export function applyMarkers(file: CodeFile): void {
  if (!editorInstance) return
  const markers: monaco.editor.IMarkerData[] = file.issues
    .filter((i) => i.line)
    .map((i) => ({
      severity: i.severity === 'error' ? monaco.MarkerSeverity.Error : monaco.MarkerSeverity.Warning,
      message: `[${i.tool}] ${i.message}`,
      startLineNumber: i.line!,
      endLineNumber: i.line!,
      startColumn: i.col ?? 1,
      endColumn: 999,
    }))
  monaco.editor.setModelMarkers(editorInstance.getModel(), 'sythrall', markers)
  const el = document.getElementById('ed-errs')
  if (el) {
    el.textContent = markers.length ? `⚠ ${markers.length} error(es)` : ''
    el.style.color = markers.length ? 'var(--err)' : 'var(--ok)'
  }
  updateProblems(file)
}

export function getEditorValue(): string | null {
  return editorInstance?.getValue() ?? null
}
