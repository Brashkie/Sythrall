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
  projectAnalysisRunning: false,
  autoOn: false,
  autoTimer: null,
  history: [],
  currentFile: null,
  backendOk: false,
  backendChecked: false,
  capabilities: null,
  currentMermaid: '',
  activeProjectId: null,
  activeProjectName: null,
  projectHealthCache: {},
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
const ACTIVE_PROJECT_NAME_KEY = 'sythrall:active-project-name'

/** La Terminal solo tiene sentido con un proyecto activo — sin uno, no hay
 * ningún directorio real al que apuntarla ("¿la terminal de qué?"). Pedido
 * explícito del usuario: sacarla como algo siempre disponible, habilitarla
 * recién al crear/abrir un proyecto. Deshabilitada (no oculta) en vez de
 * quitada del todo — sigue siendo visible dónde está, con un título que
 * explica por qué está apagada, en vez de un botón que desaparece sin aviso. */
export function updateTerminalAvailability(hasActiveProject: boolean): void {
  const btn = document.getElementById('terminal-toggle-btn')
  if (!(btn instanceof HTMLButtonElement)) return
  btn.disabled = !hasActiveProject
  btn.title = hasActiveProject ? 'Terminal' : 'Creá o abrí un proyecto para usar la terminal'
}

/** `name` es opcional a propósito — no todos los call sites lo conocen
 * (ej. limpiar la selección al eliminar un proyecto) — en ese caso el
 * indicador del nav-rail cae al id truncado (ver `_projectLabel`-style
 * fallback en cada lugar que lee `activeProjectName`). El propio setter
 * sincroniza el DOM del nav-rail acá (en vez de en cada uno de los ~7
 * call sites) para que sea imposible que un caller nuevo se olvide de
 * actualizar el indicador. */
export function setActiveProject(id: string | null, name?: string | null): void {
  state.activeProjectId = id
  state.activeProjectName = id ? (name ?? null) : null
  try {
    if (id) {
      localStorage.setItem(ACTIVE_PROJECT_KEY, id)
      if (name) localStorage.setItem(ACTIVE_PROJECT_NAME_KEY, name)
      else localStorage.removeItem(ACTIVE_PROJECT_NAME_KEY)
    } else {
      localStorage.removeItem(ACTIVE_PROJECT_KEY)
      localStorage.removeItem(ACTIVE_PROJECT_NAME_KEY)
    }
  } catch {
    /* localStorage puede no estar disponible */
  }

  const label = document.getElementById('nav-rail-project-name')
  if (label) label.textContent = id ? (name ?? id.slice(0, 8) + '…') : 'Sin proyecto'

  updateTerminalAvailability(!!id)

  // El dropzone grande ("Arrastra archivos aquí") es ruido una vez que hay
  // un proyecto activo con su propio árbol ya visible debajo — los botones
  // +Código/+Log/+Carpeta de arriba siguen ahí para sumar archivos sueltos,
  // no hace falta la caja grande empujando el árbol real hacia abajo.
  // Pedido explícito del usuario, con captura del dropzone conviviendo con
  // un proyecto ya cargado.
  const dz = document.getElementById('dz')
  if (dz) dz.style.display = id ? 'none' : ''

  // La Archivos sidebar pasa a mostrar el árbol completo del proyecto activo
  // (estilo VS Code: el explorador vive junto al Editor, no en una pantalla
  // aparte) — import dinámico por el mismo motivo que el de flow.ts abajo
  // (explorer.ts ya importa de panels/upload.ts, que a su vez importa de
  // acá — evita profundizar ese ciclo con una dependencia estática nueva).
  import('../components/explorer').then((m) => m.explorerSetActiveProject(id))

  // Import dinámico — igual que el resto de los `_goto()` de la app — para
  // no crear un ciclo estático con components/flow.ts (que a su vez importa
  // este módulo). El widget Flujo del panel derecho deriva sus 2 primeras
  // etapas ("Proyecto"/"Archivos") de `activeProjectId`, así que necesita
  // refrescarse apenas cambia, no solo cuando corre un análisis.
  import('../components/flow').then((m) => m.renderFlow())
}

export function loadPersistedActiveProject(): string | null {
  try {
    return localStorage.getItem(ACTIVE_PROJECT_KEY)
  } catch {
    return null
  }
}

export function loadPersistedActiveProjectName(): string | null {
  try {
    return localStorage.getItem(ACTIVE_PROJECT_NAME_KEY)
  } catch {
    return null
  }
}

export function getApiBase(): string {
  // Siempre same-origin — el proxy de turno (Vite en dev vía `server.proxy`/
  // `preview.proxy` en vite.config.ts, nginx en producción) es quien sabe en
  // qué puerto vive el backend real, no el frontend. Antes esto detectaba
  // "modo dev" mirando si `window.location.port` era exactamente 5173/4173
  // para pegarle directo a `http://localhost:8420` — frágil de verdad: apenas
  // Vite caía a otro puerto (5173 ocupado por cualquier motivo, algo común)
  // esa detección fallaba, la app pasaba a same-origin igual, pero el proxy
  // de Vite en ese entonces apuntaba a un `:8000` muerto (deuda de la
  // migración Flask→FastAPI) — cada fetch a la API se colgaba en silencio.
  // Arreglado en la raíz en vite.config.ts (proxy corregido a :8420 + mismo
  // proxy agregado a `preview`), así que ya no hace falta ninguna detección
  // de puerto acá: same-origin funciona siempre, dev y prod por igual.
  return ''
}
