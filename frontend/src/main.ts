// ══════════════════════════════════════════
//  CodeWatch PRO — Main Entry Point
// ══════════════════════════════════════════

// CSS side-effect import — handled by Vite bundler
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore
import './styles/main.css'

import { initApp } from './components/app'
import { wireAllEvents } from './components/events'

document.addEventListener('DOMContentLoaded', () => {
  initApp()
  wireAllEvents()
})
