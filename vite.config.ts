import type { ProxyOptions } from 'vite'
import { defineConfig } from 'vite'

// ──────────────────────────────────────────────────────────────────────────────
//  Sythrall — Vite Config v4.1
//
//  Mapa de rutas FastAPI (real, en :8420 — ver scripts/run-backend.mjs):
//    /api/upload/*    → subida de proyectos
//    /analyze/*       → análisis de código, ML, diagramas, logs
//    /check/*         → (no usado en v4, fusionado en /analyze/api)
//    /logs            → historial de logs del servidor
//    /capabilities    → capacidades del backend
//    /health          → health check
//
//  El proxy ANTERIOR hacía rewrite /api → '' (quitaba el prefijo).
//  El proxy NUEVO NO hace rewrite: cada ruta se mapea directamente.
//
//  **Bug real de fondo que esto corrige**: todas estas entradas apuntaban a
//  un `:8000` que no corre nada — deuda de la migración Flask→FastAPI (el
//  backend real vive en :8420, ver scripts/run-backend.mjs) que quedó
//  invisible porque `getApiBase()` (store/state.ts) las esquivaba con fetches
//  absolutos a :8420 cuando detectaba el puerto 5173/4173 de Vite por
//  hardcode. Esa detección por puerto es en sí misma frágil — apenas Vite
//  cae a otro puerto (5173 ocupado, algo tan común como tener otro proyecto
//  corriendo) el fallback de `getApiBase()` pasa a same-origin, todas las
//  llamadas van por ESTE proxy roto, y la app entera deja de poder hablar
//  con el backend (confirmado en vivo: exactamente el bug reportado de "el
//  ZIP no me deja entrar" — no era el ZIP, era cualquier request cuando Vite
//  no caía en el puerto esperado). Arreglado en las dos puntas: acá, target
//  correcto (:8420); en `getApiBase()`, ya no hace falta ninguna detección
//  de puerto — same-origin siempre, dev y prod por igual, con este proxy (y
//  el de `preview`, abajo) haciendo el trabajo de infraestructura que le
//  corresponde. El frontend no necesita saber en qué puerto vive el backend.
// ──────────────────────────────────────────────────────────────────────────────

const BACKEND_PROXY: Record<string, ProxyOptions> = {
  // ── Upload de proyectos
  '/api/upload': {
    target: 'http://localhost:8420',
    changeOrigin: true,
    // Sin rewrite: /api/upload/files → localhost:8420/api/upload/files ✓
  },

  // ── Análisis de código, ML, diagramas, logs
  '/analyze': {
    target: 'http://localhost:8420',
    changeOrigin: true,
    // /analyze/code    → localhost:8420/analyze/code    ✓
    // /analyze/ml      → localhost:8420/analyze/ml      ✓
    // /analyze/diagram → localhost:8420/analyze/diagram ✓
    // /analyze/api     → localhost:8420/analyze/api     ✓  (antes /check/api)
    // /analyze/logs-analyze → localhost:8420/analyze/logs-analyze ✓ (antes /analyze/logs)
  },

  // ── Logs del servidor FastAPI
  '/logs': {
    target: 'http://localhost:8420',
    changeOrigin: true,
  },

  // ── Historial de API checks
  '/api/history': {
    target: 'http://localhost:8420',
    changeOrigin: true,
  },

  // ── Sistema: capabilities y health
  '/capabilities': {
    target: 'http://localhost:8420',
    changeOrigin: true,
  },
  '/health': {
    target: 'http://localhost:8420',
    changeOrigin: true,
  },
  '/static': {
    target: 'http://localhost:8420',
    changeOrigin: true,
  },
  '/intel': {
    target: 'http://localhost:8420',
    changeOrigin: true,
  },
  '/metrics': {
    target: 'http://localhost:8420',
    changeOrigin: true,
  },

  // ── Terminal interactiva (sidecar Rust, WebSocket con PTY real)
  '/terminal': {
    target: 'http://localhost:7681',
    ws: true,
    changeOrigin: true,
    // xfwd: Vite estampa la IP real del socket entrante en X-Forwarded-For
    // (no algo que el cliente pueda falsificar).
    xfwd: true,
  },

  // ── Auth: emisión de tokens de sesión (JWT)
  '/auth': {
    target: 'http://localhost:8420',
    changeOrigin: true,
    xfwd: true,
  },
}

export default defineConfig({
  // Manifiestos (package.json, tsconfig.json, etc.) viven en la raíz del repo;
  // el código fuente del frontend vive en apps/web/.
  root: 'apps/web',
  envDir: '../..',

  build: {
    outDir: '../../dist',
    sourcemap: false,
    rollupOptions: {
      output: {
        manualChunks(id: string) {
          if (id.includes('monaco-editor')) return 'vendor-monaco'
          if (id.includes('mermaid')) return 'vendor-mermaid'
          if (id.includes('@xterm')) return 'vendor-xterm'
          if (id.includes('jszip') || id.includes('node_modules/diff')) return 'vendor-misc'
        },
      },
    },
  },

  server: {
    port: 5173,
    proxy: BACKEND_PROXY,
  },

  // Mismo proxy que `server` — `vite preview` sirve el build de producción
  // en un servidor estático propio, sin esto cualquier request relativa a
  // la API se quedaría sin destino (a diferencia del dev server, no lo
  // llevaba antes porque `getApiBase()` ya no distingue por puerto).
  preview: {
    proxy: BACKEND_PROXY,
  },

  optimizeDeps: {
    exclude: ['monaco-editor'],
  },

  worker: {
    format: 'es',
  },
})
