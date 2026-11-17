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
  //
  // **Fase 35** — los 9 endpoints pasaron al motor (`servicio/subidas.rs`).
  // Es I/O y lógica de producto, no análisis: árbol de archivos, metadata
  // cacheada, extracción de ZIP y poda de proyectos viejos. El lector de ZIP
  // se escribió a mano (`servicio/zip.rs`) usando el DEFLATE de `flate2`, que
  // ya estaba en el árbol de dependencias.
  '/api/upload': {
    target: 'http://localhost:7682',
    changeOrigin: true,
    // Sin rewrite: /api/upload/files → localhost:7682/api/upload/files ✓
  },

  // ── Análisis de código, ML, diagramas, logs y chequeo de APIs
  //
  // **Fase 35** — una sola entrada. Esto llegó a tener OCHO, porque los
  // endpoints de `/analyze` fueron pasando al motor de a uno y cada uno
  // necesitaba su entrada específica declarada ANTES del `/analyze` genérico
  // que todavía iba a FastAPI: Vite resuelve por prefijo en orden de
  // declaración, así que el general se comía al específico. Ya no hay
  // FastAPI, así que todas colapsan en esta.
  '/analyze': {
    target: 'http://localhost:7682',
    changeOrigin: true,
  },

  // ── Logs del servidor FastAPI
  // **Fase 35** — `/logs` lo servia un router de Python que solo reenviaba al
  // `/log` del motor. Mismo shape, un salto menos.
  '/logs': {
    target: 'http://localhost:7682',
    changeOrigin: true,
  },

  // **Fase 35** — aca habia una entrada `/api/history` (el historial de API
  // checks). Se fue con el router de logs: ningun cliente la pedia y el
  // endpoint ya no existe en ningun lado, asi que la entrada del proxy
  // apuntaba al vacio. Se borra en vez de reapuntarla.

  // ── Sistema: capabilities, health y el token de la terminal
  //
  // **Fase 35, paso 6** — lo último que servía Python. `/capabilities` cambió
  // de significado: reportaba qué librerías de Python tenía instaladas el
  // backend (`HAS_FLAKE8`, `HAS_TORCH`…), y eso describía al intérprete que
  // servía la API, no al producto. Ahora reporta los lenguajes del manifest y
  // los toolchains que el motor encuentra.
  '/capabilities': {
    target: 'http://localhost:7682',
    changeOrigin: true,
  },
  '/health': {
    target: 'http://localhost:7682',
    changeOrigin: true,
  },
  // **Fase 35** — los nueve endpoints de `/static` pasaron al motor. El mas
  // grande (`parse-project`) era orquestacion, no analisis: encadenaba cuatro
  // cosas que el motor ya hacia.
  '/static': {
    target: 'http://localhost:7682',
    changeOrigin: true,
  },
  // **Fase 35** — la inteligencia del editor (lint, hover, definición,
  // referencias, autocompletado, renombrar) pasó al motor
  // (`servicio/editor.rs`). Los fallbacks de definición/referencias se
  // borraron: `ast::symbols` ya tenía las cuatro funciones.
  '/intel': {
    target: 'http://localhost:7682',
    changeOrigin: true,
  },
  // **Fase 35** — las métricas en vivo del editor pasaron al motor
  // (`servicio/metricas.rs`). El prefijo entero va al motor: `metrics_live.py`
  // era el único router de Python bajo `/metrics`, y `/metrics/complexity` ya
  // lo servía el motor desde antes.
  //
  // No fue un port: el router traía su propio análisis por lenguaje (`ast` de
  // Python, expresiones regulares para JS/TS/C/Fortran) en paralelo a los
  // parsers reales, y era medidamente peor — contaba bucles totales en vez de
  // anidados, así que tres bucles seguidos daban O(n³).
  '/metrics': {
    target: 'http://localhost:7682',
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
    target: 'http://localhost:7682',
    changeOrigin: true,
    xfwd: true,
  },

  // ── Execution Intelligence (Fase 23/26 — Algorithm Validation Engine):
  // faltaba esta entrada desde que `execution.py` se creó — invisible hasta
  // ahora porque nunca se había probado el panel Static sirviendo desde
  // `vite dev` en un navegador real contra este router específico (las
  // verificaciones previas de matmul-vs-numpy/agregación-vs-pandas fueron
  // por curl directo al backend, no a través de este proxy). Sin esto,
  // cualquier botón de "Validar empíricamente"/"Comparar con numpy/pandas"
  // se cuelga con un 404 silencioso en dev.
  // **Fase 35** — de los 16 endpoints de `/execution`, 12 ya vivian identicos
  // en el motor y el router de Python solo los reenviaba. Apunta al motor.
  '/execution': {
    target: 'http://localhost:7682',
    changeOrigin: true,
  },

  // ── Binary/Executable Analysis (Fase 19 — Machine Intelligence, segundo
  // bullet): mismo patrón, agregado junto con el router.
  // **Fase 35 — el primer prefijo que ya no pasa por Python.** `/binary`
  // vivia en el router `binary_analysis` de Python, que leia el upload y lo
  // reenviaba por HTTP al motor. Ahora apunta al motor directo: `binparse`
  // esta en proceso ahi, asi que ese salto desaparecio en vez de portarse.
  '/binary': {
    target: 'http://localhost:7682',
    changeOrigin: true,
  },

  // ── History (Fase 28 — Enterprise Intelligence, primera porción):
  // snapshots locales de Project Health.
  //
  // **Fase 35** — pasó al motor. El router de Python guardaba en SQLite y
  // para el snapshot le pedía el análisis al motor por HTTP; ahora las dos
  // mitades viven en el mismo proceso y el almacenamiento es el CBOR
  // append-only de `logstore`, sin dependencia nueva.
  '/history': {
    target: 'http://localhost:7682',
    changeOrigin: true,
  },
}

export default defineConfig({
  // Manifiestos (package.json, tsconfig.json, etc.) viven en la raíz del repo;
  // el código fuente del frontend vive en apps/web/.
  root: 'apps/web',
  envDir: '../..',

  build: {
    outDir: '../../dist',
    // **Sin esto, `dist/` crece para siempre.** Vite vacía el directorio de
    // salida solo cuando está adentro de `root`, y acá `root` es `apps/web`
    // mientras la salida va a la raíz del repo — así que se negaba a
    // limpiarlo y cada build dejaba sus chunks con hash nuevo al lado de los
    // de todos los builds anteriores. Medido antes de tocarlo: **671
    // archivos, 185 MB, 584 de ellos `.js`**, de los cuales la build vigente
    // usaba 15. El resto era basura que `index.html` ya no referencia.
    emptyOutDir: true,
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
