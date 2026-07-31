// ══════════════════════════════════════════
//  CodeWatch PRO — Main Entry Point v4.0
// ══════════════════════════════════════════

// @ts-ignore
import './styles/main.css'

import { initApp } from './components/app'
import { wireAllEvents } from './components/events'

document.addEventListener('DOMContentLoaded', () => {
  initApp()
  wireAllEvents()
  // El panel Upload se inicializa lazy en switchTab('upload')
  // cuando el usuario hace click en el tab Proyectos por primera vez
})
