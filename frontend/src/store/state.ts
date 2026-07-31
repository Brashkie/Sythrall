// ══════════════════════════════════════════
//  CodeWatch PRO — Store (estado global)
// ══════════════════════════════════════════
// store/state.ts
import type { AppState, CodeFile } from '../types'

export const state: AppState = {
  files: [],
  logFiles: [],
  urls: [],
  results: {
    apis: [],
    issues: [],
    logErrors: [],
  },
  running: false,
  autoOn: false,
  autoTimer: null,
  history: [],
  steps: {},
  currentFile: null,
  backendOk: false,
  currentMermaid: '',
}

export function addFile(file: CodeFile): void {
  if (!state.files.find((f) => f.name === file.name)) {
    state.files.push(file)
  }
}

export function removeFile(id: string): void {
  state.files = state.files.filter((f) => f.id !== id)
  if (state.currentFile?.id === id) state.currentFile = null
}

export function findFile(id: string): CodeFile | undefined {
  return state.files.find((f) => f.id === id)
}

export function getApiBase(): string {
  // FastAPI corre en :8000 — en dev Vite proxea /api → :8000
  // En producción con nginx el proxy maneja todo en el mismo origen
  if (window.location.port === '5173' || window.location.port === '4173') {
    // Modo dev Vite — FastAPI directo
    return 'http://localhost:8000'
  }
  if (window.location.port === '8080') {
    // Docker / nginx — usa el proxy del mismo origen (sin puerto)
    return ''
  }
  // Fallback: mismo origen
  return ''
}
