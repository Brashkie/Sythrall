// ══════════════════════════════════════════
//  CodeWatch PRO — Store (estado global)
// ══════════════════════════════════════════
import type { AppState, CodeFile } from '../types'

export const state: AppState = {
  files:      [],
  logFiles:   [],
  urls:       [],
  results: {
    apis:      [],
    issues:    [],
    logErrors: [],
  },
  running:        false,
  autoOn:         false,
  autoTimer:      null,
  history:        [],
  steps:          {},
  currentFile:    null,
  backendOk:      false,
  currentMermaid: '',
}

// ── Helpers para los archivos
export function addFile(file: CodeFile): void {
  if (!state.files.find(f => f.name === file.name)) {
    state.files.push(file)
  }
}

export function removeFile(id: string): void {
  state.files = state.files.filter(f => f.id !== id)
  if (state.currentFile?.id === id) state.currentFile = null
}

export function findFile(id: string): CodeFile | undefined {
  return state.files.find(f => f.id === id)
}

export function getApiBase(): string {
  return window.location.port === '8080' ? '/api' : 'http://localhost:5000'
}