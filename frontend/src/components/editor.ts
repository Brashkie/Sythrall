// ══════════════════════════════════════════
//  CodeWatch PRO — Monaco Editor
//  Workers con ?worker de Vite (sin plugins)
// ══════════════════════════════════════════
//editor.ts
import type { CodeFile } from '../types'
import { debounce } from '../utils/helpers'

// Importar workers con la sintaxis ?worker de Vite
// Vite los convierte en chunks separados y les da URLs correctas en prod
import EditorWorker  from 'monaco-editor/esm/vs/editor/editor.worker?worker'
import JsonWorker    from 'monaco-editor/esm/vs/language/json/json.worker?worker'
import CssWorker     from 'monaco-editor/esm/vs/language/css/css.worker?worker'
import HtmlWorker    from 'monaco-editor/esm/vs/language/html/html.worker?worker'
import TsWorker      from 'monaco-editor/esm/vs/language/typescript/ts.worker?worker'

// Configurar MonacoEnvironment ANTES de importar monaco
// eslint-disable-next-line @typescript-eslint/no-explicit-any
;(self as any).MonacoEnvironment = {
  getWorker(_: unknown, label: string) {
    if (label === 'json')                          return new JsonWorker()
    if (label === 'css' || label === 'scss' || label === 'less') return new CssWorker()
    if (label === 'html' || label === 'handlebars') return new HtmlWorker()
    if (label === 'typescript' || label === 'javascript') return new TsWorker()
    return new EditorWorker()
  },
}

// Importar monaco DESPUÉS de configurar el environment
import * as monaco from 'monaco-editor'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let editorInstance: any = null

const EXT_LANG: Record<string, string> = {
  '.py':'python', '.js':'javascript', '.ts':'typescript', '.json':'json',
  '.yaml':'yaml', '.yml':'yaml', '.html':'html', '.css':'css',
  '.sh':'shell', '.java':'java', '.go':'go', '.txt':'plaintext',
}

export function initEditor(): void {
  monaco.editor.defineTheme('codewatch', {
    base: 'vs-dark', inherit: true,
    rules: [
      { token: 'comment', foreground: '4a5880', fontStyle: 'italic' },
      { token: 'keyword', foreground: 'b87dff' },
      { token: 'string',  foreground: '00c07a' },
      { token: 'number',  foreground: 'ffb627' },
      { token: 'type',    foreground: '3d9eff' },
    ],
    colors: {
      'editor.background':              '#0e1225',
      'editor.foreground':              '#c8d4f0',
      'editorLineNumber.foreground':    '#2d3768',
      'editor.lineHighlightBackground': '#141830',
      'editorCursor.foreground':        '#3d9eff',
      'editorGutter.background':        '#0e1225',
      'editor.selectionBackground':     '#1a2040',
    },
  })

  const container = document.getElementById('editor-container')
  if (!container) { console.error('[CodeWatch] editor-container no encontrado'); return }

  editorInstance = monaco.editor.create(container, {
    theme:        'codewatch',
    language:     'python',
    value:        '# CodeWatch PRO\n# Sube archivos o escribe aquí\n',
    fontSize:     13,
    fontFamily:   "'DM Mono', 'Cascadia Code', monospace",
    fontLigatures: true,
    lineNumbers:  'on',
    minimap:      { enabled: true },
    scrollBeyondLastLine: false,
    wordWrap:     'on',
    bracketPairColorization: { enabled: true },
    smoothScrolling:   true,
    cursorBlinking:    'expand',
    padding:      { top: 12, bottom: 12 },
    tabSize:      4,
    automaticLayout: true,
  })

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  ;(window as any)['editorRelayout'] = () => editorInstance?.layout()

  editorInstance.onDidChangeModelContent(
    debounce(() => {
      const content = editorInstance?.getValue()
      if (content !== undefined)
        document.dispatchEvent(new CustomEvent('editor:change', { detail: content }))
    }, 800)
  )

  console.log('[CodeWatch] Monaco Editor ✓')
}

export function loadFileInEditor(file: CodeFile): void {
  if (!editorInstance) return
  const lang = EXT_LANG[file.ext] ?? 'plaintext'
  monaco.editor.setModelLanguage(editorInstance.getModel(), lang)
  editorInstance.setValue(file.content)
  const fnEl = document.getElementById('ed-fname')
  if (fnEl) { fnEl.textContent = file.name; fnEl.style.display = '' }
  applyMarkers(file)
}

export function applyMarkers(file: CodeFile): void {
  if (!editorInstance) return
  const markers: monaco.editor.IMarkerData[] = file.issues
    .filter(i => i.line)
    .map(i => ({
      severity:        i.severity === 'error' ? monaco.MarkerSeverity.Error : monaco.MarkerSeverity.Warning,
      message:         `[${i.tool}] ${i.message}`,
      startLineNumber: i.line!,
      endLineNumber:   i.line!,
      startColumn:     i.col ?? 1,
      endColumn:       999,
    }))
  monaco.editor.setModelMarkers(editorInstance.getModel(), 'codewatch', markers)
  const el = document.getElementById('ed-errs')
  if (el) {
    el.textContent = markers.length ? `⚠ ${markers.length} error(es)` : ''
    el.style.color  = markers.length ? 'var(--err)' : 'var(--ok)'
  }
}

export function getEditorValue(): string | null {
  return editorInstance?.getValue() ?? null
}

export function copyEditorContent(): void {
  const val = getEditorValue()
  if (val) navigator.clipboard.writeText(val)
}
