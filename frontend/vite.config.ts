import { defineConfig } from 'vite'

export default defineConfig({
  root: '.',
  build: {
    outDir:    'dist',
    sourcemap: false,
    rollupOptions: {
      output: {
        manualChunks(id: string) {
          if (id.includes('monaco-editor'))  return 'vendor-monaco'
          if (id.includes('chart.js'))       return 'vendor-charts'
          if (id.includes('mermaid'))        return 'vendor-mermaid'
          if (id.includes('jszip') || id.includes('node_modules/diff')) return 'vendor-misc'
        },
      },
    },
  },
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target:       'http://localhost:5000',
        rewrite:      (p: string) => p.replace(/^\/api/, ''),
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
