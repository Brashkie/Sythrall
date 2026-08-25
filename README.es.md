<div align="center">
<img src="apps/web/public/sythrall-logo.png" alt="Sythrall" width="96" />

# Sythrall

</div>

> **Plataforma de inteligencia de código empresarial** — análisis estático, inspección ML/DL, inteligencia de editor en tiempo real, visualización de grafos de código, terminal integrada y monitoreo de APIs. Construida con TypeScript (sin frameworks) + FastAPI + Rust + Monaco Editor.

<div align="center">

![Version](https://img.shields.io/badge/versión-4.11.0-blue?style=flat-square)
[![CI](https://github.com/Brashkie/Sythrall/actions/workflows/ci.yml/badge.svg)](https://github.com/Brashkie/Sythrall/actions/workflows/ci.yml)
![Frontend](https://img.shields.io/badge/Frontend-Vite%20%2B%20TypeScript-646cff?style=flat-square)
![Backend](https://img.shields.io/badge/Backend-FastAPI%20%2B%20Python-009688?style=flat-square)
![Terminal](https://img.shields.io/badge/Terminal-Rust%20%2B%20axum-dea584?style=flat-square)
![Deploy](https://img.shields.io/badge/Deploy-Docker%20%2B%20Nginx-2496ed?style=flat-square)
![Tests](https://img.shields.io/badge/Tests-436%20pasando-00f5a0?style=flat-square)
![Author](https://img.shields.io/badge/Autor-Hepein%20Oficial-b87dff?style=flat-square)
![License](https://img.shields.io/badge/Licencia-GPL%203.0-orange?style=flat-square)

[English](./README.md) · [Español](./README.es.md) · [Roadmap](./ROADMAP.es.md) · [Changelog](./CHANGELOG.md)

</div>

---

## ¿Qué es Sythrall?

Sythrall es una plataforma profesional de inteligencia de código construida como proyecto educativo y de demostración empresarial. Combina un frontend moderno en TypeScript puro (sin frameworks en runtime), un backend Python/FastAPI con soporte completo de ML/DL, un motor de análisis estático multi-lenguaje, y una integración con Monaco Editor que rivaliza con la experiencia de desarrollo de VS Code.

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

Todos los manifiestos/configs viven en la raíz del repo; el código fuente vive en dos directorios con una división explícita — `apps/` son los dos productos que un usuario corre directamente (`apps/api`, `apps/web`), `services/` son los procesos Rust independientes que esos productos llaman por HTTP (`services/terminal`, `services/complexity`) pero que nadie lanza directo. `scripts/` es el único punto de entrada para todo (ver [scripts/README.md](scripts/README.md)).

```
sythrall/
├── package.json                    ← Manifiesto npm (deps del frontend + scripts lint/format/build)
├── package-lock.json
├── vite.config.ts                  ← root: 'apps/web', build.outDir: '../../dist'
├── tsconfig.json                   ← include: apps/web/src
├── biome.json                      ← Config de Biome (lint/format)
├── requirements.txt                ← Deps runtime del backend
├── requirements-dev.txt            ← Ruff (lint/format), solo dev
├── pyproject.toml                  ← Config de Ruff
├── pytest.ini                      ← testpaths: apps/api/tests
├── Cargo.toml                      ← Manifiesto Rust — sidecars services/terminal + services/complexity (2 [[bin]], 1 [lib])
├── Cargo.lock
├── docker-compose.yml
├── .dockerignore
├── START.bat / STOP.bat
├── scripts/                        ← Flujo de dev sin Docker (setup/dev/build/test/lint/format, .ps1 + .sh)
│   ├── run-backend.mjs             ← levanta uvicorn (npm run dev:api)
│   ├── run-terminal.mjs            ← levanta el sidecar de terminal en Rust (npm run dev:term)
│   ├── run-complexity.mjs          ← levanta el sidecar de complejidad en Rust (npm run dev:cx)
│   └── dev-banner.mjs              ← banner de arranque con ansimax para npm run dev
├── apps/                           ← productos que un usuario corre directamente, un directorio cada uno
│   ├── api/                        ← Backend FastAPI
│   │   ├── main.py                 ← FastAPI (35+ rutas)
│   │   ├── shared.py
│   │   ├── Dockerfile
│   │   ├── routers/
│   │   │   ├── upload.py           ← POST /api/upload/{files,folder,zip} + CRUD
│   │   │   ├── analysis.py         ← POST /analyze/{code,api,logs-analyze}
│   │   │   ├── ml.py               ← POST /analyze/ml
│   │   │   ├── diagram.py          ← POST /analyze/diagram
│   │   │   ├── logs.py             ← GET /logs + GET /api/history
│   │   │   ├── static_analysis.py  ← POST /static/{parse,parse-project,bigO,wasm}
│   │   │   ├── intelligence.py     ← POST /intel/{lint,analyze,hover,definition,references,completions,rename}
│   │   │   ├── graph.py            ← GET /analyze/graph/types, POST /analyze/graph{,/project}
│   │   │   └── metrics_live.py     ← POST /metrics/live — métricas instantáneas por tecla
│   │   ├── services/
│   │   │   ├── project_service.py
│   │   │   ├── static_parser.py    ← Parser multi-lenguaje: Python/C/C++/JS/TS
│   │   │   └── complexity_client.py ← Cliente HTTP del sidecar Rust complexity-engine
│   │   └── tests/                  ← 436 tests en total (ver sección Tests más abajo)
│   └── web/                        ← Frontend TypeScript (Vite, sin frameworks)
│       ├── index.html
│       ├── Dockerfile.frontend
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
│   │   ├── Dockerfile
│   │   └── src/
│   │       ├── main.rs             ← handler WS de axum, auth por token, bridging del PTY
│   │       ├── pty_session.rs      ← wrapper de portable-pty (ConPTY/PTY Unix, una sola implementación)
│   │       └── auth.rs             ← generación de token + comparación en tiempo constante
│   └── complexity/                 ← complejidad ciclomática + MI + métricas raw (reemplaza radon) + análisis Python rico
│       ├── Dockerfile
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
docker compose up --build
```

### 3 — Modo desarrollo (sin Docker)

La carpeta `scripts/` envuelve todo el flujo sin Docker en un solo comando — sin tener que ir manualmente con `cd`/`pip`/`npm`:

```powershell
# Windows (PowerShell)
.\scripts\setup.ps1   # instala venv del backend + node_modules del frontend
.\scripts\dev.ps1     # corre backend (uvicorn --reload) + frontend (vite) juntos
```

```bash
# macOS / Linux / Git Bash / WSL
./scripts/setup.sh
./scripts/dev.sh
```

Ver [scripts/README.md](scripts/README.md) para la lista completa (`build`, `test`, `lint`, `format`). Por debajo, `dev` es simplemente `npm run dev` — `concurrently` corre Vite, uvicorn y el sidecar de terminal en Rust como un solo proceso (logs con prefijo `[web]`/`[api]`/`[term]`), así que `npm run dev` funciona igual directo si prefieres saltarte los scripts wrapper. Si no tenés `cargo` instalado, el resto de la app funciona igual — solo no vas a tener la terminal integrada (`[term]` imprime un aviso y termina, sin afectar a los otros dos procesos).

El proceso `[term]` imprime un token aleatorio al arrancar (`Terminal token: ...`). Para uso local normal no lo necesitás — el panel de terminal se conecta solo. Solo importa si alguna vez configurás `TERMINAL_HOST` a algo distinto de `127.0.0.1` (ver la nota de seguridad de la terminal más abajo).

<details>
<summary>Comandos manuales (equivalentes, sin script)</summary>

**Backend** (desde la raíz — ahí vive `requirements.txt`, el código está en `apps/api/`):
```bash
pip install -r requirements.txt
cd apps/api
uvicorn main:app --host 0.0.0.0 --port 8420 --reload
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

## Stack Python (backend)

| Librería | Versión | Uso |
|---|---|---|
| FastAPI | 0.115.5 | Servidor REST async |
| pylint / flake8 | latest | Calidad de código (complejidad/MI se movió al sidecar Rust `complexity-engine` — ver Stack Rust más abajo) |
| tree-sitter | 0.23.2 | Parser AST multi-lenguaje |
| networkx | 3.6.1 | Algoritmos de grafos |
| numpy / pandas / polars | latest | Procesamiento de datos |
| torch / tensorflow-cpu | latest | Deep learning |
| scikit-learn / lightgbm | latest | ML clásico |
| spacy / opencv / scipy | latest | NLP / Visión / Ciencia |
| **Cython** | 3.2.4 | Compilación Python → C |

## Stack TypeScript (frontend)

| Librería | Versión | Uso |
|---|---|---|
| Vite | 5.3.1 | Bundler y dev server |
| TypeScript | 5.4.5 | Tipado estático |
| monaco-editor | 0.45.0 | Editor de código |
| mermaid | 11.4.0 | Diagramas |
| chart.js | 4.4.3 | Gráficas |
| @xterm/xterm | 6.0.0 | Emulador de terminal (lado cliente del sidecar en Rust) |
| [ansimax](https://github.com/Brashkie/ansimax) | 1.5.0 | Rendering ANSI/CLI para el banner de arranque de `npm run dev` |

## Stack Rust — dos sidecars, un solo `Cargo.toml`

`terminal-server` (un sidecar chico para la terminal integrada) y `complexity-engine` (reemplaza la dependencia pip `radon` — complejidad ciclomática, Maintainability Index, métricas raw de líneas, análisis de seguridad/taint, y smells estructurales, calculados en proceso propio en vez de importados de una librería de terceros). Los dos bins comparten un solo `Cargo.toml` en la raíz — `services/{terminal,complexity}/` solo tienen código fuente, sin manifiesto propio.

| Crate | Lo usa | Uso |
|---|---|---|
| axum | ambos | Servidor HTTP/WebSocket + ruteo |
| tokio | ambos | Runtime async |
| portable-pty | terminal-server | ConPTY (Windows) / PTY (Unix) — una sola implementación para ambos |
| subtle | terminal-server | Comparación de token en tiempo constante |
| rand | terminal-server | CSPRNG para el token de cada corrida |
| rustpython-parser | complexity-engine | Python source → AST |
| criterion | complexity-engine (dev) | Benchmarks — `cargo bench --bench complexity_bench` |

`complexity-engine` bindea a `127.0.0.1:7682` por defecto (`COMPLEXITY_HOST`/`COMPLEXITY_PORT` overrideables, misma convención que el sidecar de terminal) y expone `GET /health` + `POST /metrics/complexity` + `POST /parse/python`. Sin token de auth — a diferencia de la terminal, es un endpoint de cómputo puro sin acceso a shell/filesystem. El backend Python le pega vía `apps/api/services/complexity_client.py` y degrada con gracia (complejidad/MI vacíos, sin crash) si el sidecar no está corriendo — mismo patrón que flake8/pylint siendo opcionales.

---

## Tests

```bash
cd apps/api && pytest
# pytest.ini vive en la raíz y se autodetecta; testpaths = apps/api/tests
```

```
test_intelligence.py       121 ✓
test_analysis.py            62 ✓
test_graph.py                54 ✓
test_static_analysis.py      37 ✓
test_metrics_live.py         34 ✓
test_graph_phase2.py         31 ✓
test_upload.py                30 ✓
test_security_findings.py    30 ✓
test_naming_smells.py        16 ✓
test_structural_smells.py    14 ✓
test_complexity_client.py     7 ✓
──────────────────────────────
Total: 436 pasando
```

`pytest` levanta el sidecar Rust (`complexity-engine`) por su cuenta para toda la sesión — ver `tests/conftest.py` — ya que Big-O/complejidad/space/recursión/security/structural+naming smells para archivos `.py` son Rust-only ahora, sin fallback Python contra el cual testear.

Los dos sidecars en Rust tienen sus propios checks — `cargo build --release`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (82 tests unitarios entre complejidad/MI/raw-metrics/Big-O/espacio/recursión/clasificadores CS-Engine/seguridad/smells estructurales+de nombres, contra valores calculados a mano) — corridos vía el job `terminal` en [`ci.yml`](.github/workflows/ci.yml), aparte del suite de Python de arriba (el mismo job compila/testea los dos bins, ya que comparten un `Cargo.toml`).

---

## Herramientas (lint & format)

[Biome](https://biomejs.dev) formatea y lintea el frontend TypeScript; [Ruff](https://docs.astral.sh/ruff/) hace lo mismo para el backend Python. Ambos corren vía `scripts/`:

```bash
./scripts/lint.sh      # o .\scripts\lint.ps1   — solo revisa, no escribe
./scripts/format.sh    # o .\scripts\format.ps1 — aplica los cambios
```

Comandos directos equivalentes (desde la raíz): `npm run lint` / `npm run format`, `ruff check apps/api` / `ruff format apps/api`.

---

## Comandos Docker útiles

```bash
docker compose logs -f                                   # Logs en vivo
docker compose build --no-cache && docker compose up -d  # Rebuild completo
docker compose ps                                        # Contenedores activos
docker exec -it sythrall-backend bash                    # Shell del backend
docker compose down                                      # Detener (conserva volúmenes)
docker compose down -v                                   # Detener + borrar volúmenes
```

---

## Configuración

`apps/api/.env`:
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
El backend lo encuentra vía `COMPLEXITY_ENGINE_URL` (default `http://127.0.0.1:7682`; en `docker-compose.yml` se pisa a `http://complexity:7682`, el nombre del servicio en la red de Docker).

Cambiar puertos en `docker-compose.yml`:
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
Lo que sea que lo ocupe no es necesariamente un proceso viejo de Sythrall — en Windows en particular, servicios en segundo plano de software instalado sin relación pueden agarrar puertos arbitrarios primero. Si el PID no es `python`/`uvicorn`, no lo mates a ciegas; cambiá el puerto de Sythrall (`scripts/run-backend.mjs` + `getApiBase()` en `apps/web/src/store/state.ts`) o identificá primero qué es ese proceso.

**Backend muestra "Sin backend"**
```bash
docker compose ps
docker compose logs backend
```
Puede tardar ~25s la primera vez (descarga de PyTorch).

**ZIP rechazado** → Límite de 200 MB. Súbelo como carpeta si supera ese tamaño.

**Módulo no encontrado**
```bash
docker compose build --no-cache && docker compose up -d
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
| 4 | FastAPI + Upload de proyectos | ✅ |
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

## Autor

**Brashkie** · [Hepein Oficial](https://github.com/Brashkie) · Lima, Perú

---

## Licencia

GPL-3.0 — ver [LICENSE](LICENSE)
