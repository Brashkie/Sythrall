// ══════════════════════════════════════════
//  Sythrall — Store (estado global)
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
    projectDashboard: null,
  },
  running: false,
  autoOn: false,
  autoTimer: null,
  history: [],
  steps: {},
  currentFile: null,
  backendOk: false,
  currentMermaid: '',
  activeProjectId: null,
}

export function addFile(file: CodeFile): void {
  if (!state.files.find((f) => f.name === file.name)) {
    state.files.push(file)
  }
}

// ─── Proyecto activo — persiste entre refrescos ───────────────────────────────
// state.activeProjectId solo (sin esto) vive en memoria y se pierde al
// refrescar. Junto con Session Restore (panels/problems.ts), esto es lo que
// permite reabrir donde quedaste al volver a la app.

const ACTIVE_PROJECT_KEY = 'sythrall:active-project'

export function setActiveProject(id: string | null): void {
  state.activeProjectId = id
  try {
    if (id) localStorage.setItem(ACTIVE_PROJECT_KEY, id)
    else localStorage.removeItem(ACTIVE_PROJECT_KEY)
  } catch {
    /* localStorage puede no estar disponible */
  }
}

export function loadPersistedActiveProject(): string | null {
  try {
    return localStorage.getItem(ACTIVE_PROJECT_KEY)
  } catch {
    return null
  }
}

export function getApiBase(): string {
  // FastAPI corre en :8420 — en dev Vite proxea /api → :8420
  // En producción con nginx el proxy maneja todo en el mismo origen
  if (window.location.port === '5173' || window.location.port === '4173') {
    // Modo dev Vite — FastAPI directo
    return 'http://localhost:8420'
  }
  if (window.location.port === '8080') {
    // Docker / nginx — usa el proxy del mismo origen (sin puerto)
    return ''
  }
  // Fallback: mismo origen
  return ''
}
