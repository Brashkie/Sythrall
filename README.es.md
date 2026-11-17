<div align="center">
<img src="apps/web/public/sythrall-logo.png" alt="Sythrall" width="96" />

# Sythrall

</div>

> **Plataforma de inteligencia de código empresarial** — análisis estático, inspección ML/DL, inteligencia de editor en tiempo real, visualización de grafos de código, terminal integrada y monitoreo de APIs. Construida con TypeScript (sin frameworks) + Rust + Monaco Editor.

<div align="center">

![Version](https://img.shields.io/badge/versión-4.12.0-blue?style=flat-square)
[![CI](https://github.com/Brashkie/Sythrall/actions/workflows/ci.yml/badge.svg)](https://github.com/Brashkie/Sythrall/actions/workflows/ci.yml)
![Frontend](https://img.shields.io/badge/Frontend-Vite%20%2B%20TypeScript-646cff?style=flat-square)
![Backend](https://img.shields.io/badge/Backend-Rust%20%2B%20axum-CE422B?style=flat-square)
![Terminal](https://img.shields.io/badge/Terminal-Rust%20%2B%20axum-dea584?style=flat-square)
![Deploy](https://img.shields.io/badge/Deploy-Docker%20%2B%20Nginx-2496ed?style=flat-square)
![Tests](https://img.shields.io/badge/Tests-436%20pasando-00f5a0?style=flat-square)
![Author](https://img.shields.io/badge/Autor-Hepein%20Oficial-b87dff?style=flat-square)
![License](https://img.shields.io/badge/Licencia-GPL%203.0-orange?style=flat-square)

[English](./README.md) · [Español](./README.es.md) · [Roadmap](./ROADMAP.es.md) · [Changelog](./CHANGELOG.md)

</div>

> **Sythrall vuelve a ser open source, bajo GPL-3.0.** *(2026-11-14)*
>
> **La historia completa, porque cambió dos veces y conviene que esté dicha y no inferida:**
>
> 1. **Open source desde el principio.** Sythrall nació público en este repositorio, bajo **GPL-3.0**, y así se mantuvo hasta la **v4.12.0**.
> 2. **Privado desde la v4.12.0** *(2026-08-31)*. Ese release llevó el título *"last public update before moving to private SaaS"*: el repositorio dejó de recibir actualizaciones y el desarrollo siguió en privado como producto SaaS. Lo que había quedado publicado siguió disponible bajo su licencia, como una foto del proyecto hasta ese punto.
> 3. **Y ahora vuelve a ser open source, otra vez bajo GPL-3.0.** Es un cambio de idea del autor, no una consecuencia de nada: el trabajo hecho en privado vuelve al repositorio público y el desarrollo continúa a la vista.
>
> **Por qué GPL-3.0 y no una licencia permisiva.** La GPL es *copyleft*: cualquiera puede usar, estudiar, modificar y redistribuir Sythrall, y quien distribuya una versión modificada **tiene que publicar su código fuente bajo la misma licencia**. Una licencia permisiva (MIT, Apache) dejaría que una versión cerrada se construya encima sin devolver nada. GPL-3.0 mantiene abierto lo que se construya a partir de acá.
>
> **Y una corrección sobre el archivo de licencia.** Hasta este cambio, el `LICENSE` de este repositorio era un **resumen con un enlace al texto completo**, no el texto. La propia GPL pide que el programa venga acompañado de *una copia* de la licencia, y un enlace no es una copia. Ahora `LICENSE` trae **el texto íntegro de la GNU GPL versión 3** (674 líneas), que es lo que la licencia exige para distribuir.

---

## ¿Qué es Sythrall?

Sythrall es una plataforma profesional de inteligencia de código construida como proyecto educativo y de demostración empresarial. Combina un frontend moderno en TypeScript puro (sin frameworks en runtime) con un backend en Rust que sirve el HTTP y hace el análisis en proceso, sobre 13 lenguajes, más una integración con Monaco Editor que rivaliza con la experiencia de desarrollo de VS Code. **Tuvo un backend en Python/FastAPI hasta la Fase 35 (2026-11-16), cuando se le quitó la última línea** — el CHANGELOG cuenta por qué y qué costó.

### Capacidades principales

| Módulo | Descripción |
|---|---|
| **Project Explorer** | Árbol de archivos expandible, pestañas multi-archivo, búsqueda global (`Ctrl+Shift+F`), outline de símbolos |
| **Explorador de carpetas** | Abrí una carpeta real del disco ("+ Carpeta") y navegala como árbol expandible estilo VSCode — funciona en cualquier navegador moderno, sin APIs exclusivas de Chromium. Crea o suma al proyecto activo, no es una vista local aislada |
| **Terminal integrada** | Shell interactiva real (PowerShell/bash) en un panel inferior redimensionable, con un sidecar en Rust (`portable-pty` + `axum`) — protegida por token, sin fricción para uso local |
| **Editor Intelligence** | Linting en tiempo real, diagnósticos inline, hover con Big-O, Go to Definition, Find References, Rename Symbol, autocompletado semántico |
| **Análisis Estático** | Parser AST multi-lenguaje (Python, TypeScript, C/C++) — estimación Big-O, complejidad ciclomática, hallazgos de seguridad (catálogo CWE), smells estructurales, hints WASM/Cython, call graph, dead code |
| **Project Health** | 4 scores agregados (Security, Quality, Complexity, Architecture) calculados sobre un proyecto completo, cada uno con su fórmula y sus números crudos al lado — nunca un número pelado |
| **Code Graph Visual** | Import Graph, Call Graph, detección de dependencias circulares, detección de centralidad/hubs — Tree View con Mermaid, sobre archivos sueltos o un proyecto completo *(el Force Graph interactivo y el dir-tree de Complexity Heatmap existen y están testeados, pero ninguno de los dos tiene todavía un control de UI que lo dispare)* |
| **Proyectos** | Subí archivos, una carpeta o un ZIP — el resultado puede quedar como **proyecto activo**, que Editor · Issues · Diagrama · Static · Métricas leen directo, sin volver a subir nada por panel |
| **Análisis** | pylint · flake8 · AST · `complexity-engine` (Rust) — issues, complejidad, maintainability index, métricas de Halstead |
| **ML/DL** | Detección de 23 librerías, 23 patrones de pipeline, 25 modelos, 20+ reglas |
| **Diagramas** | Flowchart · Callgraph · Clases · Secuencia — Mermaid.js con zoom/pan |
| **APIs** | Verificación de endpoints externos con historial y métricas |
| **Dashboard** | Scores de Project Health, charts de distribución, tiempos de respuesta, historial de ejecuciones |
| **Diff** | Comparación visual de archivos con resaltado de cambios |
| **Logs** | Stream de logs del servidor en tiempo real — también disponible como vista alternable dentro del panel de terminal |
| **Tema claro/oscuro** | Toggle en el topbar, oscuro por defecto, persiste entre sesiones |

---

## Estructura del proyecto

Todos los manifiestos/configs viven en la raíz del repo; el código fuente vive en dos directorios — `apps/web` es el frontend, y `services/` tiene los dos procesos de Rust: `services/complexity` es el backend (HTTP + análisis) y `services/terminal` el sidecar de la terminal integrada. `scripts/` es el único punto de entrada para todo (ver [scripts/README.md](scripts/README.md)).

```
sythrall/
├── package.json                    ← Manifiesto npm (deps del frontend + scripts lint/format/build)
├── package-lock.json
├── vite.config.ts                  ← root: 'apps/web', build.outDir: '../../dist'
├── tsconfig.json                   ← include: apps/web/src
├── biome.json                      ← Config de Biome (lint/format)
├── requirements-dev.txt            ← Ruff (lint/format), solo dev
├── pyproject.toml                  ← Config de Ruff
├── Cargo.toml                      ← Manifiesto Rust — sidecars services/terminal + services/complexity (2 [[bin]], 1 [lib])
├── Cargo.lock
├── .dockerignore                   ← queda en la raíz — Docker solo lo lee desde el directorio de build context
├── START.bat / STOP.bat
├── docker/                         ← todos los archivos de Docker viven acá, no dispersos por apps/services
│   ├── docker-compose.yml          ← build context: .. (raíz del repo) para los 4 servicios
│   ├── api.Dockerfile
│   ├── frontend.Dockerfile
│   ├── terminal.Dockerfile
│   └── complexity.Dockerfile
├── scripts/                        ← Flujo de dev sin Docker (setup/dev/build/test/lint/format, .ps1 + .sh)
│   ├── run-terminal.mjs            ← levanta el sidecar de terminal en Rust (npm run dev:term)
│   ├── run-complexity.mjs          ← levanta el sidecar de complejidad en Rust (npm run dev:cx)
│   └── dev-banner.mjs              ← banner de arranque con ansimax para npm run dev
├── apps/                           ← productos que un usuario corre directamente, un directorio cada uno
│   └── web/                        ← Frontend TypeScript (Vite, sin frameworks)
│       ├── index.html
│       └── src/
│           ├── api/client.ts          ← API client completo
│           ├── components/
│           │   ├── app.ts             ← Shell de la app + manejo de archivos
│           │   ├── editor.ts          ← Integración Monaco Editor
│           │   ├── editor-intelligence.ts ← Linting + hover + autocompletado
│           │   ├── explorer.ts        ← Project Explorer (árbol + tabs + búsqueda + outline)
│           │   ├── file-browser.ts    ← Árbol de carpetas desde <input webkitdirectory>, cross-browser
│           │   ├── terminal.ts        ← Cliente xterm.js para el sidecar de terminal en Rust
│           │   ├── events.ts          ← Conexión global de eventos
│           │   ├── charts.ts          ← Integración Chart.js
│           │   ├── mermaid.ts         ← Mermaid + motor zoom/pan
│           │   └── flow.ts            ← Diagrama de flujo de ejecución
│           ├── panels/
│           │   ├── dashboard.ts       ← Scores de Project Health
│           │   ├── analysis.ts        ← Render de Issues/Métricas (+ modo proyecto activo)
│           │   ├── apis.ts
│           │   ├── ml.ts
│           │   ├── upload.ts          ← Hub de Proyectos: subida, recientes, proyecto activo
│           │   ├── static.ts          ← Panel de Análisis Estático (+ modo proyecto activo)
│           │   ├── problems.ts        ← Live Metrics Bar + Session Restore + Problems Panel
│           │   └── graph.ts           ← Code Graph Visual — Tree View con Mermaid conectado; Force Graph/Dir Tree implementados, sin UI todavía
│           ├── store/state.ts         ← activeProjectId persiste entre refrescos (localStorage)
│           ├── styles/
│           │   ├── main.css
│           │   ├── upload.css
│           │   ├── static-addon.css
│           │   ├── explorer.css
│           │   └── problems.css
│           ├── types/index.ts
│           └── utils/
│               ├── icons.ts           ← Set de iconos SVG inline + badges de lenguaje — sin emoji, sin librería
│               ├── health.ts          ← Tarjetas de score de Project Health, compartidas entre Dashboard y Static
│               ├── file-tree.ts       ← FileList → árbol anidado (para file-browser.ts)
│               └── theme.ts           ← Toggle de tema claro/oscuro + persistencia
├── services/                       ← procesos Rust independientes que `apps/` llama por HTTP — nadie los lanza directo
│   ├── terminal/                   ← shell interactiva real sobre WebSocket
│   │   └── src/
│   │       ├── main.rs             ← handler WS de axum, auth por token, bridging del PTY
│   │       ├── pty_session.rs      ← wrapper de portable-pty (ConPTY/PTY Unix, una sola implementación)
│   │       └── auth.rs             ← generación de token + comparación en tiempo constante
│   └── complexity/                 ← complejidad ciclomática + MI + métricas raw (reemplaza radon) + análisis Python rico
│       ├── benches/
│       │   ├── complexity_bench.rs ← Criterion — medido 9-21x más rápido que radon
│       │   └── parse_bench.rs      ← Criterion — analyze_rich() vs. _parse_python(), 5.6-20.6x más rápido
│       └── src/
│           ├── main.rs             ← servidor HTTP axum (GET /health, POST /metrics/complexity, POST /parse/python)
│           ├── lib.rs              ← entrypoints analyze()/analyze_rich(), los usan main.rs y los benchmarks
│           ├── parser.rs           ← wrapper de rustpython-parser + resolución offset de bytes→línea
│           ├── complexity.rs       ← complejidad ciclomática de McCabe (lógica propia, no de radon)
│           ├── maintainability.rs  ← Maintainability Index + métricas de Halstead (fórmula Coleman-Oman)
│           ├── raw.rs              ← loc/lloc/sloc/comments/blank/multi
│           ├── walk.rs             ← walker exhaustivo genérico del AST (equivalente a ast.walk()), compartido por los módulos de abajo
│           ├── bigo.rs             ← heurística Big-O/Θ/Ω
│           ├── space.rs            ← heurística de complejidad de espacio
│           ├── recursion.rs        ← detección de tail-call
│           ├── classifiers.rs      ← clasificadores CS Engine regex/grammar/graph-traversal
│           ├── security.rs         ← taint tracking + catálogo CWE (SQLi, command injection, path traversal, deserialización, credenciales hardcodeadas)
│           ├── smells.rs           ← smells estructurales de código (función larga, god object, etc.)
│           ├── structure.rs        ← extracción de clases/imports + helpers de AST (decorators, docstrings, calls)
│           └── rich.rs             ← orquestador analyze_rich() — mismo shape functions/classes/imports/summary que _parse_python()
└── README.md
```

---

## Requisitos previos

| Herramienta | Versión mínima | Descarga |
|---|---|---|
| **Docker Desktop** | 4.x | [docker.com](https://www.docker.com/products/docker-desktop) |
| **Node.js** | 20.x | [nodejs.org](https://nodejs.org) |
| **Python** | 3.11+ | [python.org](https://www.python.org) |
| **Rust** | stable | [rustup.rs](https://rustup.rs) — solo hace falta para correr la terminal integrada en modo dev; el resto de la app funciona sin ella |
| **Git** | cualquiera | [git-scm.com](https://git-scm.com) |

---

## Instalación

### 1 — Clonar el repositorio

```bash
git clone https://github.com/Brashkie/Sythrall.git
cd Sythrall
```

### 2 — Iniciar con Docker (recomendado)

**Windows:**
1. Abre Docker Desktop y espera a que esté corriendo
2. Doble clic en **`START.bat`**
3. El navegador abre automáticamente en `http://localhost:8080`

**Cualquier OS — terminal:**
```bash
docker compose -f docker/docker-compose.yml up --build
```

### 3 — Modo desarrollo (sin Docker)

La carpeta `scripts/` envuelve todo el flujo sin Docker en un solo comando — sin tener que ir manualmente con `cd`/`pip`/`npm`:

```powershell
# Windows (PowerShell)
.\scripts\setup.ps1   # instala venv del backend + node_modules del frontend
.\scripts\dev.ps1     # corre el frontend (vite) y los dos procesos de Rust juntos
```

```bash
# macOS / Linux / Git Bash / WSL
./scripts/setup.sh
./scripts/dev.sh
```

Ver [scripts/README.md](scripts/README.md) para la lista completa (`build`, `test`, `lint`, `format`). Por debajo, `dev` es simplemente `npm run dev` — `concurrently` corre Vite y los dos procesos de Rust como uno solo (logs con prefijo `[web]`/`[term]`/`[cx]`). **`cargo` es obligatorio**: desde la Fase 35 el motor *es* el backend, así que sin él no hay nada sirviendo la API.

El proceso `[term]` imprime un token aleatorio al arrancar (`Terminal token: ...`). Para uso local normal no lo necesitás — el panel de terminal se conecta solo. Solo importa si alguna vez configurás `TERMINAL_HOST` a algo distinto de `127.0.0.1` (ver la nota de seguridad de la terminal más abajo).

<details>
<summary>Comandos manuales (equivalentes, sin script)</summary>

**Backend** (el motor — desde la raíz, ahí vive `Cargo.toml`):
```bash
cargo run --bin complexity-engine
# http://127.0.0.1:7682
```

**Frontend** (desde la raíz — ahí viven `package.json`/`vite.config.ts`, el código fuente está en `apps/web/src`):
```bash
npm install
npm run dev
# App en http://localhost:5173
```

</details>

---

## URLs del sistema

| Servicio | URL |
|---|---|
| **App (Docker)** | http://localhost:8080 |
| **App (dev)** | http://localhost:5173 |
| **Backend API** | http://localhost:8420 |
| **Swagger UI** | http://localhost:8420/docs |
| **Health** | http://localhost:8420/health |
| **Sidecar de terminal (Rust)** | ws://127.0.0.1:7681 (proxeado a través de `/terminal` en dev — no está pensado para abrirse directo) |
| **Sidecar de complejidad (Rust)** | http://127.0.0.1:7682 (lo llama el backend, no el navegador — no está pensado para abrirse directo) |

### Nota de seguridad de la terminal

El sidecar de terminal bindea a `127.0.0.1` por defecto — a diferencia del resto de la app (que bindea `0.0.0.0`), deliberadamente **no** acepta conexiones de otras máquinas de entrada, porque otorga acceso real a una shell. Además está protegido por un token aleatorio por corrida (comparación en tiempo constante), servido automáticamente solo a pedidos que verificablemente vienen de la misma máquina. Si alguna vez configurás `TERMINAL_HOST=0.0.0.0` (o exponés el puerto 7681 de otra forma) para alcanzarlo desde otro dispositivo en tu red, la auto-conexión deja de funcionar para pedidos remotos y hay que pegar el token a mano — copiándolo de la consola donde corre `[term]`. No expongas este puerto en una red no confiable sin un proxy reverso real + TLS delante.

---

## Referencia de la API

### Sistema
```http
GET /health          → Estado del servidor y capacidades
GET /capabilities    → Versiones de todas las librerías instaladas
GET /logs            → Historial de logs
```

### Análisis estático (sin IA, multi-lenguaje)
```http
POST /static/parse          → Parse completo: Big-O, hallazgos de seguridad, smells estructurales, call graph, hints WASM
POST /static/parse-project  → Multi-archivo: grafo de dependencias, findings agregados, scores de Project Health
POST /static/bigO           → Tabla Big-O por función
POST /static/wasm           → Recomendaciones Cython/WASM por hot paths
GET  /static/languages      → Lenguajes soportados
```

### Editor Intelligence
```http
POST /intel/lint         → Fast lint ~1ms (AST + patrones)
POST /intel/analyze      → Heavy analyze ~80ms (pylint + complexity engine + Big-O)
POST /intel/hover        → Firma + Big-O + CC + docstring
POST /intel/definition   → Go to Definition (F12)
POST /intel/references   → Find References (Shift+F12)
POST /intel/completions  → Autocompletado semántico (Ctrl+Space)
POST /intel/rename       → Rename Symbol con WorkspaceEdits (F2)
```

### Code Graph
```http
GET  /analyze/graph/types           → Tipos disponibles
POST /analyze/graph                 → Desde archivos del sidebar
POST /analyze/graph/project         → Desde proyecto ZIP subido
```

Tipos: `import` · `call` · `circular` · `heatmap` · `centrality`

### Análisis de código
```http
POST /analyze/code
```
```json
{
  "filename": "script.py",
  "content":  "def hello():\n    print('hola')\n",
  "tools":    ["ast", "flake8", "pylint", "complexity"]
}
```

### ML/DL
```http
POST /analyze/ml
```
```json
{ "filename": "modelo.py", "content": "..." }
```

### Upload de proyectos
```http
POST   /api/upload/files
POST   /api/upload/folder
POST   /api/upload/zip
GET    /api/upload/projects
GET    /api/upload/projects/{id}/tree
GET    /api/upload/projects/{id}/file
DELETE /api/upload/projects/{id}
```

---

## Stack TypeScript (frontend)

**Son nueve dependencias de ejecución y seis de desarrollo — la lista completa,
tomada de `package.json`, no de memoria.** Esta tabla declaraba `chart.js`, que
no está instalado ni se importa en ninguna parte, y se olvidaba de cuatro que
sí: `jszip`, `diff` y los tres addons de xterm.

| Librería | Versión | Uso |
|---|---|---|
| monaco-editor | ^0.45.0 | Editor de código (7 imports — el que más se usa) |
| mermaid | ^11.4.0 | Diagramas |
| @xterm/xterm | ^6.0.0 | Emulador de terminal (lado cliente del sidecar en Rust) |
| @xterm/addon-fit | ^0.11.0 | Ajusta la terminal al tamaño del contenedor |
| @xterm/addon-search | ^0.16.0 | Búsqueda dentro de la terminal |
| @xterm/addon-web-links | ^0.12.0 | Hace clickeables las URLs de la terminal |
| jszip | ^3.10.1 | Empaqueta el proyecto para descargar — import dinámico |
| diff | ^5.2.0 | Genera el parche unificado del panel Diff — import dinámico |
| [ansimax](https://github.com/Brashkie/ansimax) | ^1.5.0 | Rendering ANSI/CLI para el banner de arranque de `npm run dev` |

| Desarrollo | Versión | Uso |
|---|---|---|
| Vite | ^5.3.1 | Bundler y dev server |
| TypeScript | ^5.4.5 | Tipado estático (`npm run typecheck`) |
| @biomejs/biome | 2.5.6 | Linter y formateador (`npm run lint`) |
| concurrently | 10.0.4 | Levanta los tres procesos de `npm run dev` |
| @types/node, @types/diff | ^20.14.0, ^5.2.1 | Tipos |

**Lo que se borró el 2026-11-16 y por qué.** `thebe`, `pyodide` y
`starboard-notebook` estaban declaradas como dependencias de producción y el
código no las importaba nunca: aparecían sólo como el **texto** de su nombre en
el catálogo de entornos de notebook del panel ML, que existe para decirte
cuáles usa *tu* archivo — Sythrall no ejecuta ninguno de los tres. `thebe`
además arrastraba `@jupyterlab/testutils`, o sea `jest` y `ts-jest`, adentro
del árbol de producción. Sacarlas llevó `node_modules` de **616 a 148 paquetes
de primer nivel** y de **12 GB a 3,0 GB**.

## Stack de Rust — el motor y la terminal, un solo `Cargo.toml`

**Fase 35 (2026-11-16) — Sythrall no tiene Python.** `complexity-engine` es el
backend: sirve todas las rutas HTTP del producto y hace el análisis en proceso.
`terminal-server` es el sidecar de la terminal integrada. Los dos binarios
comparten un `Cargo.toml` en la raíz — `services/{terminal,complexity}/` tienen
sólo código, sin manifest por servicio.

| Crate | Usado por | Para qué |
|---|---|---|
| axum | ambos | Servidor HTTP/WebSocket + routing |
| tokio | ambos | Runtime asincrónico |
| portable-pty | terminal-server | ConPTY (Windows) / PTY (Unix) — una implementación para los dos |
| subtle | terminal-server | Comparación de tokens en tiempo constante |
| jsonwebtoken | ambos | Tokens de sesión — el motor firma, la terminal verifica |
| rand | ambos | CSPRNG para tokens e ids de proyecto |
| rustpython-parser | motor | Código Python → AST |
| tree-sitter (+ 9 gramáticas) | motor | AST de C/C++/JS/TS/Rust/Fortran/Ada/D |
| object | motor | Leer ejecutables (PE/ELF/Mach-O) |
| flate2 | motor | DEFLATE, para leer los ZIPs subidos |
| hyper-rustls + rustls | motor | Cliente HTTP+TLS del chequeo de APIs externas |
| ciborium | motor | CBOR, para el log y el historial de Project Health |
| toml | motor | Leer manifests `Cargo.toml`/`pyproject.toml` |
| criterion | motor (dev) | Benchmarks — `cargo bench --bench complexity_bench` |

El motor escucha en `127.0.0.1:7682` por defecto (`COMPLEXITY_HOST`/
`COMPLEXITY_PORT` lo cambian). `GET /engine/health` es su propia sonda de vida
(un `{"ok":true}` barato); `GET /health` y `GET /capabilities` son los del
producto, y reportan lo que el motor sabe hacer de verdad — los 13 lenguajes
del manifest de plugins y cuáles toolchains externos (`gcc`, `gnatprove`,
`dmd`, `gfortran`, `zig`) encuentra. El único con control de acceso es
`GET /auth/terminal-token`, y sólo a pedidos de loopback.

---

## Tests

```bash
cargo test          # el motor y el sidecar de la terminal
npm run typecheck   # el frontend
```

**Fase 35** — no hay `pytest`: toda la lógica vive en Rust y sus tests son
`cargo test`. La suite son **1.806 tests** más 11 marcados `#[ignore]`
(benchmarks de wall-clock, se corren a mano con `cargo test -- --ignored` —
miden la máquina tanto como el código, así que en CI darían rojos falsos).

CI corre seis compuertas, en [`ci.yml`](.github/workflows/ci.yml):
`npm run typecheck`, `npm run lint`, `npm run build`, `cargo build --release`,
`cargo clippy --all-targets -- -D warnings` y `cargo test`.

## Herramientas (lint & format)

[Biome](https://biomejs.dev) formatea y lintea el frontend TypeScript; [Ruff](https://docs.astral.sh/ruff/) hace lo mismo para el backend Python. Ambos corren vía `scripts/`:

```bash
./scripts/lint.sh      # o .\scripts\lint.ps1   — solo revisa, no escribe
./scripts/format.sh    # o .\scripts\format.ps1 — aplica los cambios
```

Comandos directos equivalentes (desde la raíz): `npm run lint` / `npm run format`, `cargo clippy --all-targets -- -D warnings` / `cargo fmt`.

---

## Comandos Docker útiles

Todos los archivos de Docker viven en `docker/` (`docker/docker-compose.yml`, `docker/*.Dockerfile`) — `-f docker/docker-compose.yml` corre estos comandos desde la raíz sin tener que hacer `cd` a `docker/` primero. `.dockerignore` se queda en la raíz del repo (Docker lo necesita ahí — ver el comentario en `docker/docker-compose.yml`).

```bash
docker compose -f docker/docker-compose.yml logs -f                                   # Logs en vivo
docker compose -f docker/docker-compose.yml build --no-cache && docker compose -f docker/docker-compose.yml up -d  # Rebuild completo
docker compose -f docker/docker-compose.yml ps                                        # Contenedores activos
docker exec -it sythrall-backend bash                                                 # Shell del backend
docker compose -f docker/docker-compose.yml down                                      # Detener (conserva volúmenes)
docker compose -f docker/docker-compose.yml down -v                                   # Detener + borrar volúmenes
```

---

## Configuración

`.env` en la raíz del repo:
```env
PYTHONUNBUFFERED=1
```

`apps/web/.env`:
```env
VITE_SILENCE_SOURCEMAP_WARNINGS=true
```

Variables de entorno del sidecar de terminal (opcionales — por defecto `127.0.0.1:7681`, seguro para uso local):
```env
TERMINAL_HOST=127.0.0.1
TERMINAL_PORT=7681
```

Variables de entorno del sidecar de complejidad (opcionales — por defecto `127.0.0.1:7682`):
```env
COMPLEXITY_HOST=127.0.0.1
COMPLEXITY_PORT=7682
```
El backend lo encuentra vía `COMPLEXITY_ENGINE_URL` (default `http://127.0.0.1:7682`; en `docker/docker-compose.yml` se pisa a `http://complexity:7682`, el nombre del servicio en la red de Docker).

Cambiar puertos en `docker/docker-compose.yml`:
```yaml
services:
  backend:
    ports:
      - "8420:8000"
  frontend:
    ports:
      - "8080:80"
```

---

## Solución de problemas

**Docker no inicia** → Abre Docker Desktop y espera a que el ícono deje de mostrar "Starting".

**Puerto 8420 ocupado**
```bash
netstat -ano | findstr :8420   # Windows
lsof -i :8420                  # Linux / Mac
```
Lo que sea que lo ocupe no es necesariamente un proceso viejo de Sythrall — en Windows en particular, servicios en segundo plano de software instalado sin relación pueden agarrar puertos arbitrarios primero. Si el PID no es `complexity-engine`, no lo mates a ciegas; cambiá el puerto del motor (`COMPLEXITY_PORT`) o identificá primero qué es ese proceso.

**Backend muestra "Sin backend"**
```bash
docker compose -f docker/docker-compose.yml ps
docker compose -f docker/docker-compose.yml logs backend
```
Puede tardar ~25s la primera vez (descarga de PyTorch).

**ZIP rechazado** → Límite de 200 MB. Súbelo como carpeta si supera ese tamaño.

**Módulo no encontrado**
```bash
docker compose -f docker/docker-compose.yml build --no-cache && docker compose -f docker/docker-compose.yml up -d
```

**No aparece la terminal / el botón no conecta** → Falta el toolchain de Rust. Instalalo desde [rustup.rs](https://rustup.rs), reiniciá la terminal/consola para que el PATH se actualice, y volvé a correr `npm run dev` — el proceso `[term]` debería compilar y arrancar solo. El resto de la app funciona igual sin esto.

**La terminal pide el token a cada rato** → Normalmente no debería pasar en uso local (se auto-conecta). Si pasa seguido, confirmá que estás abriendo la app en `http://localhost:5173` (no por IP de red) — el chequeo de "pedido local" depende de eso.

---

## Roadmap

Organizado por fase, no por número de versión — una fase agrupa un bloque coherente de trabajo a lo largo de la vida del proyecto; los releases/tags siguen teniendo su propio número semántico (ver [CHANGELOG.md](CHANGELOG.md)). El detalle completo fase por fase, con razonamiento y qué sigue abierto, vive en **[ROADMAP.es.md](ROADMAP.es.md)**.

**Estado**: ✅ Completa · 🟡 Parcial · 🔴 Planeada

### Filosofía de lenguajes

Sythrall no está "construido con seis lenguajes" — cada lenguaje ocupa la capa donde realmente aporta valor:

| Capa | Lenguaje | Rol |
|---|---|---|
| Interacción | **TypeScript** | UI, integración con Monaco, editor intelligence, diagramas |
| Intelligence & Science | **Python** | IA/ML, orquestación, cargas científicas |
| Native Analysis | **Rust** | El motor de análisis estático — parsing, AST, complejidad, seguridad, calidad, grafos |
| Scientific/HPC | **Fortran** | *Objetivo* de análisis, no lenguaje de implementación (planeado) |
| Nivel máquina | **Assembly** | Objetivo de análisis para instrucciones/registros/control-flow (planeado) |
| Native tooling | **Zig** | Build, cross-compilación, distribución standalone (planeado) |

### Fases de un vistazo

| # | Fase | Estado |
|---|---|---|
| 1 | Base | ✅ |
| 2 | Inspector ML/DL | ✅ |
| 3 | Zoom/Pan + Responsive | ✅ |
| 4 | Backend REST + Upload de proyectos | ✅ |
| 5 | Análisis Estático + Editor Intelligence | ✅ |
| 6 | Code Graph + Project Explorer | ✅ |
| 7 | Panel de Problemas + Métricas en Vivo | ✅ |
| 8 | Computer Science Engine | ✅ |
| 9 | Terminal Integrada + Explorador de Carpetas + Tema | ✅ |
| 10 | Rebrand + reestructuración a `apps/` + escalado a proyectos gigantes | 🟡 |
| 11 | `radon` reemplazado por `complexity-engine` (Rust) | ✅ |
| 12 | Cerrados los clasificadores del CS Engine + ubicación del Problems Panel | ✅ |
| 13 | Algorithmic Intelligence | 🟡 |
| 14 | Data Structures & Graph Intelligence | 🟡 |
| 15 | Mathematical Intelligence | 🔴 |
| 16 | Formal Language Intelligence | 🔴 |
| 17 | Compiler Intelligence | 🔬 spike de investigación |
| 18 | Native Analysis Core (`static_parser.py` → Rust) | 🟡 |
| 19 | Machine Intelligence | 🔬 spike de investigación |
| 20 | Scientific Intelligence | 🔴 |
| 21 | Security & Taint Intelligence | ✅ |
| 22 | Code Quality Intelligence | 🟡 |
| 23 | Execution Intelligence | 🔴 |
| 24 | Extensibility Platform | 🔴 |
| 25 | Sythrall Platform | 🔴 |

Detalle completo de cada fase — alcance, razonamiento, qué se shippeó, qué sigue abierto — → **[ROADMAP.es.md](ROADMAP.es.md)**.

---

## Changelog

Ver [CHANGELOG.md](CHANGELOG.md) para el historial completo de versiones.

---

## Estructura del motor (`services/complexity/src`)

**105 módulos de Rust en 8 grupos**, en el orden del pipeline. Estuvieron
planos hasta el 2026-10-16; el agrupamiento sale de lo que cada módulo hace, no
de su nombre. Los conteos son de la Fase 35 (2026-11-16), cuando `servicio/`
absorbió todo lo que servía el backend de Python.

| Grupo | Módulos | Qué es |
|---|---|---|
| `lenguajes/` | 13 | El front-end: un módulo por lenguaje. Donde el texto se vuelve árbol. |
| `ast/` | 7 | Infraestructura de árbol compartida, sin saber de ningún lenguaje. |
| `costo/` | 15 | Big-O, ciclomática, espacio, costo amortizado y costo de compilar. |
| `flujo/` | 9 | CFG, SSA, dominadores, grafo de llamadas, y lo que se deduce de recorrerlos. |
| `hallazgos/` | 17 | Las reglas que afirman algo sobre el código del usuario. |
| `teoria/` | 10 | CS formal aplicada: autómatas, lógica, combinatoria, decidibilidad. |
| `servicio/` | 22 | La capa que sirve el producto: escaneo, subidas, historial, editor, diagramas, ML, métricas, APIs externas, sistema y auth. |
| `bench/` | 12 | Kernels que Sythrall compila y corre para validarse contra la realidad. |

En la raíz quedan sólo `lib.rs` (punto de entrada y re-exportaciones) y
`main.rs` (el servidor HTTP, que desde la Fase 35 es **el** backend).

---

## Autor

**Brashkie** · [Hepein Oficial](https://github.com/Brashkie) · Lima, Perú

---

## Licencia

**GPL-3.0-or-later** — el texto completo está en [LICENSE](LICENSE).

El proyecto nació público bajo GPL-3.0, pasó a privado en la v4.12.0 y volvió a
ser público bajo GPL-3.0. Es un cambio de idea del autor, no una
reinterpretación: se eligió copyleft en vez de una licencia permisiva para que
cualquier trabajo derivado siga siendo libre.

**Código de terceros dentro de los binarios:** los ejecutables llevan
compilado adentro C y assembly de otros proyectos (las nueve gramáticas de
tree-sitter, y BoringSSL vía `ring`). Sus licencias piden reproducir el aviso
de copyright al distribuir el binario, y **la mayoría no publica ese aviso en
su paquete de crates.io** — se comprobó uno por uno. Por eso está
[THIRD-PARTY.md](THIRD-PARTY.md), con el inventario completo y los avisos
verificados contra el `LICENSE` de cada repositorio. Los textos largos, en
[licenses/](licenses/).
