// ══════════════════════════════════════════
//  Sythrall — Main Entry Point v4.0
// ══════════════════════════════════════════

// @ts-ignore
import './styles/main.css'

import { initApp } from './components/app'
import { initCommandPalette } from './components/commandPalette'
import { wireAllEvents } from './components/events'
import { applyDevMode } from './utils/devMode'
import { initTheme } from './utils/theme'

// Antes de DOMContentLoaded para no flashear el tema incorrecto al cargar.
initTheme()
// Mismo motivo: el atributo tiene que estar puesto antes del primer pintado,
// o los atajos de puerto aparecen un instante y desaparecen.
applyDevMode()

document.addEventListener('DOMContentLoaded', () => {
  initApp()
  wireAllEvents()
  // Ctrl/Cmd+K. Va despues de initApp() porque la paleta indexa
  // `state.files`, y antes de eso el estado todavia no se restauro.
  initCommandPalette()
  // El panel Upload se inicializa lazy en switchTab('upload')
  // cuando el usuario hace click en el tab Proyectos por primera vez
})
