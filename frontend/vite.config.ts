import { defineConfig } from 'vite'

// ──────────────────────────────────────────────────────────────────────────────
//  CodeWatch PRO — Vite Config v4.0
//  Migrado de Flask (:5000) a FastAPI (:8000)
//
//  Mapa de rutas FastAPI:
//    /api/upload/*    → subida de proyectos
//    /analyze/*       → análisis de código, ML, diagramas, logs
//    /check/*         → (no usado en v4, fusionado en /analyze/api)
//    /logs            → historial de logs del servidor
//    /capabilities    → capacidades del backend
//    /health          → health check
//
//  El proxy ANTERIOR hacía rewrite /api → '' (quitaba el prefijo).
//  El proxy NUEVO NO hace rewrite: cada ruta se mapea directamente.
// ──────────────────────────────────────────────────────────────────────────────

export default defineConfig({
  root: '.',

  build: {
    outDir:    'dist',
    sourcemap: false,
    rollupOptions: {
      output: {
        manualChunks(id: string) {
          if (id.includes('monaco-editor'))                            return 'vendor-monaco'
          if (id.includes('chart.js'))                                 return 'vendor-charts'
          if (id.includes('mermaid'))                                  return 'vendor-mermaid'
          if (id.includes('jszip') || id.includes('node_modules/diff')) return 'vendor-misc'
        },
      },
    },
  },

  server: {
    port: 5173,
    proxy: {
      // ── Upload de proyectos (nuevo en v4)
      '/api/upload': {
        target:      'http://localhost:8000',
        changeOrigin: true,
        // Sin rewrite: /api/upload/files → localhost:8000/api/upload/files ✓
      },

      // ── Análisis de código, ML, diagramas, logs de análisis
      '/analyze': {
        target:      'http://localhost:8000',
        changeOrigin: true,
        // /analyze/code    → localhost:8000/analyze/code    ✓
        // /analyze/ml      → localhost:8000/analyze/ml      ✓
        // /analyze/diagram → localhost:8000/analyze/diagram ✓
        // /analyze/api     → localhost:8000/analyze/api     ✓  (antes /check/api)
        // /analyze/logs-analyze → localhost:8000/analyze/logs-analyze ✓ (antes /analyze/logs)
      },

      // ── Logs del servidor FastAPI
      '/logs': {
        target:      'http://localhost:8000',
        changeOrigin: true,
      },

      // ── Historial de API checks
      '/api/history': {
        target:      'http://localhost:8000',
        changeOrigin: true,
      },

      // ── Sistema: capabilities y health
      '/capabilities': {
        target:      'http://localhost:8000',
        changeOrigin: true,
      },
      '/health': {
        target:      'http://localhost:8000',
        changeOrigin: true,
      },
      '/static': {
        target:       'http://localhost:8000',
        changeOrigin: true,
      },
      '/intel': {
        target:       'http://localhost:8000',
        changeOrigin: true,
      },
    },
  },

  optimizeDeps: {
    exclude: ['monaco-editor'],
  },

  worker: {
    format: 'es',
  },
})
