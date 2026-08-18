# 🛰 Sythrall

> **Plataforma de inteligencia de código empresarial** — análisis estático, inspección ML/DL, inteligencia de editor en tiempo real, visualización de grafos de código, terminal integrada y monitoreo de APIs. Construida con TypeScript (sin frameworks) + FastAPI + Rust + Monaco Editor.

<div align="center">

![Version](https://img.shields.io/badge/versión-4.8.0-blue?style=flat-square)
[![CI](https://github.com/Brashkie/Sythrall/actions/workflows/ci.yml/badge.svg)](https://github.com/Brashkie/Sythrall/actions/workflows/ci.yml)
![Frontend](https://img.shields.io/badge/Frontend-Vite%20%2B%20TypeScript-646cff?style=flat-square)
![Backend](https://img.shields.io/badge/Backend-FastAPI%20%2B%20Python-009688?style=flat-square)
![Terminal](https://img.shields.io/badge/Terminal-Rust%20%2B%20axum-dea584?style=flat-square)
![Deploy](https://img.shields.io/badge/Deploy-Docker%20%2B%20Nginx-2496ed?style=flat-square)
![Tests](https://img.shields.io/badge/Tests-373%20pasando-00f5a0?style=flat-square)
![Author](https://img.shields.io/badge/Autor-Hepein%20Oficial-b87dff?style=flat-square)
![License](https://img.shields.io/badge/Licencia-GPL%203.0-orange?style=flat-square)

[English](./README.md) · [Español](./README.es.md)

</div>

---

## ✨ ¿Qué es Sythrall?

Sythrall es una plataforma profesional de inteligencia de código construida como proyecto educativo y de demostración empresarial. Combina un frontend moderno en TypeScript puro (sin frameworks en runtime), un backend Python/FastAPI con soporte completo de ML/DL, un motor de análisis estático multi-lenguaje, y una integración con Monaco Editor que rivaliza con la experiencia de desarrollo de VS Code.

### Capacidades principales

| Módulo | Descripción |
|---|---|
| **📂 Project Explorer** | Árbol de archivos expandible, pestañas multi-archivo, búsqueda global (`Ctrl+Shift+F`), outline de símbolos |
| **📁 Explorador de carpetas** | Abrí una carpeta real del disco ("+ Carpeta") y navegala como árbol expandible estilo VSCode — funciona en cualquier navegador moderno, sin APIs exclusivas de Chromium. Crea o suma al proyecto activo, no es una vista local aislada |
| **🖥 Terminal integrada** | Shell interactiva real (PowerShell/bash) en un panel inferior redimensionable, con un sidecar en Rust (`portable-pty` + `axum`) — protegida por token, sin fricción para uso local |
| **📝 Editor Intelligence** | Linting en tiempo real, diagnósticos inline, hover con Big-O, Go to Definition, Find References, Rename Symbol, autocompletado semántico |
| **🔬 Análisis Estático** | Parser AST multi-lenguaje (Python, TypeScript, C/C++) — estimación Big-O, complejidad ciclomática, hints WASM/Cython, call graph, dead code |
| **🕸 Code Graph Visual** | Import Graph, Call Graph, detección de dependencias circulares — Tree View con Mermaid, sobre archivos sueltos o un proyecto completo *(el Force Graph interactivo y el dir-tree de Complexity Heatmap existen y están testeados, pero ninguno de los dos tiene todavía un control de UI que lo dispare)* |
| **📂 Proyectos** | Subí archivos, una carpeta o un ZIP — el resultado puede quedar como **proyecto activo**, que Editor · Issues · Diagrama · Static · Métricas leen directo, sin volver a subir nada por panel |
| **🔍 Análisis** | pylint · flake8 · AST · `complexity-engine` (Rust) — issues, complejidad, maintainability index |
| **🤖 ML/DL** | Detección de 23 librerías, 23 patrones de pipeline, 25 modelos, 20+ reglas |
| **🔀 Diagramas** | Flowchart · Callgraph · Clases · Secuencia — Mermaid.js con zoom/pan |
| **📡 APIs** | Verificación de endpoints externos con historial y métricas |
| **📊 Dashboard** | Charts de distribución, tiempos de respuesta, historial de ejecuciones |
| **🔁 Diff** | Comparación visual de archivos con resaltado de cambios |
| **🖥 Logs** | Stream de logs del servidor en tiempo real — también disponible como vista alternable dentro del panel de terminal |
| **🎨 Tema claro/oscuro** | Toggle en el topbar, oscuro por defecto, persiste entre sesiones |

---

## 📁 Estructura del proyecto

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
│   │   ├── main.py                 ← FastAPI v4.8 (35+ rutas)
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
│   │   └── tests/                  ← 329 tests en total (ver sección Tests más abajo)
│   └── web/                        ← Frontend TypeScript (Vite, sin frameworks)
│       ├── index.html
│       ├── Dockerfile.frontend
│       └── src/
│           ├── api/client.ts          ← API client completo
│           ├── components/
│           │   ├── app.ts             ← Shell de la app + manejo de archivos
│           │   ├── editor.ts          ← Integración Monaco Editor
│           │   ├── editor-intelligence.ts ← Linting + hover + autocompletado (Fases 1–3)
│           │   ├── explorer.ts        ← Project Explorer (árbol + tabs + búsqueda + outline)
│           │   ├── file-browser.ts    ← Árbol de carpetas desde <input webkitdirectory>, cross-browser
│           │   ├── terminal.ts        ← Cliente xterm.js para el sidecar de terminal en Rust
│           │   ├── events.ts          ← Conexión global de eventos
│           │   ├── charts.ts          ← Integración Chart.js
│           │   ├── mermaid.ts         ← Mermaid + motor zoom/pan
│           │   └── flow.ts            ← Diagrama de flujo de ejecución
│           ├── panels/
│           │   ├── analysis.ts        ← Render de Issues/Métricas (+ modo proyecto activo)
│           │   ├── apis.ts
│           │   ├── ml.ts
│           │   ├── upload.ts          ← Hub de Proyectos: subida, recientes, proyecto activo
│           │   ├── static.ts          ← Panel de Análisis Estático (+ modo proyecto activo)
│           │   ├── problems.ts        ← Live Metrics Bar + Session Restore + Problems Panel (los 3 wireados, cada uno con su propio home)
│           │   └── graph.ts           ← Code Graph Visual — Tree View con Mermaid conectado; Force Graph/Dir Tree implementados, sin UI todavía
│           ├── store/state.ts         ← activeProjectId persiste entre refrescos (localStorage)
│           ├── styles/
│           │   ├── main.css
│           │   ├── upload.css
│           │   ├── static-addon.css
│           │   └── explorer.css
│           ├── types/index.ts
│           └── utils/
│               ├── icons.ts           ← Set de iconos SVG inline + badges de lenguaje — sin emoji, sin librería
│               ├── file-tree.ts       ← FileList → árbol anidado (para file-browser.ts)
│               └── theme.ts           ← Toggle de tema claro/oscuro + persistencia
├── services/                       ← procesos Rust independientes que `apps/` llama por HTTP — nadie los lanza directo
│   ├── terminal/                   ← shell interactiva real sobre WebSocket
│   │   ├── Dockerfile
│   │   └── src/
│   │       ├── main.rs             ← handler WS de axum, auth por token, bridging del PTY
│   │       ├── pty_session.rs      ← wrapper de portable-pty (ConPTY/PTY Unix, una sola implementación)
│   │       └── auth.rs             ← generación de token + comparación en tiempo constante
│   └── complexity/                 ← complejidad ciclomática + MI + métricas raw (reemplaza radon) + Fase 18 análisis Python rico
│       ├── Dockerfile
│       ├── benches/
│       │   ├── complexity_bench.rs ← Criterion — medido 9-21x más rápido que radon
│       │   └── parse_bench.rs      ← Criterion — analyze_rich() vs. _parse_python(), 5.6-20.6x más rápido
│       └── src/
│           ├── main.rs             ← servidor HTTP axum (GET /health, POST /metrics/complexity, POST /parse/python)
│           ├── lib.rs              ← entrypoints analyze()/analyze_rich(), los usan main.rs y los benchmarks
│           ├── parser.rs           ← wrapper de rustpython-parser + resolución offset de bytes→línea
│           ├── complexity.rs       ← complejidad ciclomática de McCabe (lógica propia, no de radon)
│           ├── maintainability.rs  ← Maintainability Index (fórmula Coleman-Oman, conteo Halstead propio)
│           ├── raw.rs              ← loc/lloc/sloc/comments/blank/multi
│           ├── walk.rs             ← walker exhaustivo genérico del AST (equivalente a ast.walk()), compartido por los módulos de abajo
│           ├── bigo.rs             ← heurística Big-O/Θ/Ω — puerto de _infer_big_o_python/_theta_omega_python
│           ├── recursion.rs        ← detección de tail-call — puerto de _recursion_info_python
│           ├── classifiers.rs      ← clasificadores CS Engine regex/grammar/graph-traversal — puerto de las versiones Python de la Fase 12
│           ├── structure.rs        ← extracción de clases/imports + helpers de AST (decorators, docstrings, calls)
│           └── rich.rs             ← orquestador analyze_rich() — mismo shape functions/classes/imports/summary que _parse_python()
└── README.md
```

---

## ⚡ Requisitos previos

| Herramienta | Versión mínima | Descarga |
|---|---|---|
| **Docker Desktop** | 4.x | [docker.com](https://www.docker.com/products/docker-desktop) |
| **Node.js** | 20.x | [nodejs.org](https://nodejs.org) |
| **Python** | 3.11+ | [python.org](https://www.python.org) |
| **Rust** | stable | [rustup.rs](https://rustup.rs) — solo hace falta para correr la terminal integrada en modo dev; el resto de la app funciona sin ella |
| **Git** | cualquiera | [git-scm.com](https://git-scm.com) |

---

## 🚀 Instalación

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

El proceso `[term]` imprime un token aleatorio al arrancar (`🔑 Terminal token: ...`). Para uso local normal no lo necesitás — el panel de terminal se conecta solo. Solo importa si alguna vez configurás `TERMINAL_HOST` a algo distinto de `127.0.0.1` (ver la nota de seguridad de la terminal más abajo).

<details>
<summary>Comandos manuales (equivalentes, sin script)</summary>

**Backend** (desde la raíz — ahí vive `requirements.txt`, el código está en `apps/api/`):
```bash
pip install -r requirements.txt
cd apps/api
uvicorn main:app --host 0.0.0.0 --port 8000 --reload
```

**Frontend** (desde la raíz — ahí viven `package.json`/`vite.config.ts`, el código fuente está en `apps/web/src`):
```bash
npm install
npm run dev
# App en http://localhost:5173
```

</details>

---

## 🌐 URLs del sistema

| Servicio | URL |
|---|---|
| **App (Docker)** | http://localhost:8080 |
| **App (dev)** | http://localhost:5173 |
| **Backend API** | http://localhost:8000 |
| **Swagger UI** | http://localhost:8000/docs |
| **Health** | http://localhost:8000/health |
| **Sidecar de terminal (Rust)** | ws://127.0.0.1:7681 (proxeado a través de `/terminal` en dev — no está pensado para abrirse directo) |
| **Sidecar de complejidad (Rust)** | http://127.0.0.1:7682 (lo llama el backend, no el navegador — no está pensado para abrirse directo) |

### 🔐 Nota de seguridad de la terminal

El sidecar de terminal bindea a `127.0.0.1` por defecto — a diferencia del resto de la app (que bindea `0.0.0.0`), deliberadamente **no** acepta conexiones de otras máquinas de entrada, porque otorga acceso real a una shell. Además está protegido por un token aleatorio por corrida (comparación en tiempo constante), servido automáticamente solo a pedidos que verificablemente vienen de la misma máquina. Si alguna vez configurás `TERMINAL_HOST=0.0.0.0` (o exponés el puerto 7681 de otra forma) para alcanzarlo desde otro dispositivo en tu red, la auto-conexión deja de funcionar para pedidos remotos y hay que pegar el token a mano — copiándolo de la consola donde corre `[term]`. No expongas este puerto en una red no confiable sin un proxy reverso real + TLS delante.

---

## 🔌 Referencia de la API

### Sistema
```http
GET /health          → Estado del servidor y capacidades
GET /capabilities    → Versiones de todas las librerías instaladas
GET /logs            → Historial de logs
```

### Análisis estático (sin IA, multi-lenguaje)
```http
POST /static/parse          → Parse completo: Big-O, call graph, hints WASM
POST /static/parse-project  → Multi-archivo: grafo de dependencias
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

Tipos: `import` · `call` · `circular` · `heatmap`

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

## 🐍 Stack Python (backend)

| Librería | Versión | Uso |
|---|---|---|
| FastAPI | 0.115.5 | Servidor REST async |
| pylint / flake8 | latest | Calidad de código (complejidad/MI se movió al sidecar Rust `complexity-engine` — ver 🦀 Stack Rust más abajo) |
| tree-sitter | 0.23.2 | Parser AST multi-lenguaje |
| networkx | 3.6.1 | Algoritmos de grafos |
| numpy / pandas / polars | latest | Procesamiento de datos |
| torch / tensorflow-cpu | latest | Deep learning |
| scikit-learn / lightgbm | latest | ML clásico |
| spacy / opencv / scipy | latest | NLP / Visión / Ciencia |
| **Cython** | 3.2.4 | Compilación Python → C |

## 🟦 Stack TypeScript (frontend)

| Librería | Versión | Uso |
|---|---|---|
| Vite | 5.3.1 | Bundler y dev server |
| TypeScript | 5.4.5 | Tipado estático |
| monaco-editor | 0.45.0 | Editor de código |
| mermaid | 11.4.0 | Diagramas |
| chart.js | 4.4.3 | Gráficas |
| @xterm/xterm | 6.0.0 | Emulador de terminal (lado cliente del sidecar en Rust) |
| [ansimax](https://github.com/Brashkie/ansimax) | 1.5.0 | Rendering ANSI/CLI para el banner de arranque de `npm run dev` |

## 🦀 Stack Rust — dos sidecars, un solo `Cargo.toml`

`terminal-server` (primer uso de Rust en el proyecto — un sidecar chico para la terminal integrada, no un reemplazo de código Python existente) y `complexity-engine` (reemplaza la dependencia pip `radon` — complejidad ciclomática, Maintainability Index y métricas raw de líneas, calculadas en proceso propio en vez de importadas de una librería de terceros cuyo interior el proyecto no controla). Los dos bins comparten un solo `Cargo.toml` en la raíz — `services/{terminal,complexity}/` solo tienen código fuente, sin manifiesto propio.

| Crate | Lo usa | Uso |
|---|---|---|
| axum | ambos | Servidor HTTP/WebSocket + ruteo |
| tokio | ambos | Runtime async |
| portable-pty | terminal-server | ConPTY (Windows) / PTY (Unix) — una sola implementación para ambos |
| subtle | terminal-server | Comparación de token en tiempo constante |
| rand | terminal-server | CSPRNG para el token de cada corrida |
| rustpython-parser | complexity-engine | Python source → AST (la lógica de complejidad/MI/raw-metrics arriba de eso es código propio del proyecto, no de radon) |
| criterion | complexity-engine (dev) | Benchmarks — `cargo bench --bench complexity_bench` |

`complexity-engine` bindea a `127.0.0.1:7682` por defecto (`COMPLEXITY_HOST`/`COMPLEXITY_PORT` overrideables, misma convención que el sidecar de terminal) y expone `GET /health` + `POST /metrics/complexity`. Sin token de auth — a diferencia de la terminal, es un endpoint de cómputo puro sin acceso a shell/filesystem, así que el riesgo que un token mitiga ahí no aplica acá. El backend Python le pega vía `apps/api/services/complexity_client.py` y degrada con gracia (complejidad/MI vacíos, sin crash) si el sidecar no está corriendo — mismo patrón que flake8/pylint siendo opcionales.

---

## 🧪 Tests

```bash
cd apps/api && pytest
# pytest.ini vive en la raíz y se autodetecta; testpaths = apps/api/tests
```

```
test_upload.py             29 ✅
test_analysis.py           62 ✅
test_intelligence.py      116 ✅
test_graph.py              46 ✅
test_graph_phase2.py       31 ✅
test_metrics_live.py       34 ✅
test_complexity_client.py   6 ✅
test_static_analysis.py     5 ✅
──────────────────────────────
Total: 329 pasando
```

Los dos sidecars en Rust tienen sus propios checks — `cargo build --release`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (29 tests unitarios para la lógica de complejidad/MI/raw-metrics/Big-O/recursión/clasificadores CS-Engine de `complexity-engine`, contra valores calculados a mano y con paridad testeada contra la implementación Python) — corridos vía el job `terminal` en [`ci.yml`](.github/workflows/ci.yml), aparte del suite de Python de arriba (el mismo job compila/testea los dos bins, ya que comparten un `Cargo.toml`).

---

## 🧹 Herramientas (lint & format)

[Biome](https://biomejs.dev) formatea y lintea el frontend TypeScript; [Ruff](https://docs.astral.sh/ruff/) hace lo mismo para el backend Python. Ambos corren vía `scripts/`:

```bash
./scripts/lint.sh      # o .\scripts\lint.ps1   — solo revisa, no escribe
./scripts/format.sh    # o .\scripts\format.ps1 — aplica los cambios
```

Comandos directos equivalentes (desde la raíz): `npm run lint` / `npm run format`, `ruff check apps/api` / `ruff format apps/api`.

---

## 🔄 Comandos Docker útiles

```bash
docker compose logs -f                                   # Logs en vivo
docker compose build --no-cache && docker compose up -d  # Rebuild completo
docker compose ps                                        # Contenedores activos
docker exec -it sythrall-backend bash                    # Shell del backend
docker compose down                                      # Detener (conserva volúmenes)
docker compose down -v                                   # Detener + borrar volúmenes
```

---

## ⚙️ Configuración

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
      - "8000:8000"
  frontend:
    ports:
      - "8080:80"
```

---

## 🐛 Solución de problemas

**Docker no inicia** → Abre Docker Desktop y espera a que el ícono deje de mostrar "Starting".

**Puerto 8000 ocupado**
```bash
netstat -ano | findstr :8000   # Windows
lsof -i :8000                  # Linux / Mac
```

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

**No aparece la terminal / botón "🖥 Terminal" no conecta** → Falta el toolchain de Rust. Instalalo desde [rustup.rs](https://rustup.rs), reiniciá la terminal/consola para que el PATH se actualice, y volvé a correr `npm run dev` — el proceso `[term]` debería compilar y arrancar solo. El resto de la app funciona igual sin esto.

**La terminal pide el token a cada rato** → Normalmente no debería pasar en uso local (se auto-conecta). Si pasa seguido, confirmá que estás abriendo la app en `http://localhost:5173` (no por IP de red) — el chequeo de "pedido local" depende de eso.

---

## 🗺️ Roadmap

Organizado por fase, no por número de versión — una fase agrupa un bloque coherente
de trabajo a lo largo de la vida del proyecto; los releases/tags siguen teniendo su
propio número semántico (ver [`CHANGELOG.md`](CHANGELOG.md)), son dos ejes distintos.
Misma convención que usa el roadmap propio de [`ansimax`](https://github.com/Brashkie/ansimax).

**✅ Completa · 🟡 Parcial · 🔴 Planeada**

### Filosofía de lenguajes

Sythrall no está "construido con seis lenguajes" — cada lenguaje ocupa la capa donde
realmente aporta valor, y las Fases 13+ de abajo están organizadas según esa división
en vez de una lista plana de "agregar más lenguajes":

| Capa | Lenguaje | Rol | Evidencia hasta ahora |
|---|---|---|---|
| Interacción | **TypeScript** | UI, integración con Monaco, editor intelligence, diagramas — la única capa que el usuario toca directamente | Vite + Monaco + Mermaid + Chart.js + xterm, todo shippeado (Fases 1–9) |
| Intelligence & Science | **Python** | IA/ML, orquestación, cargas científicas — **no** el motor de análisis estático a largo plazo: ese rol se está retirando hacia Rust (Native Analysis Core de la Fase 18), una porción medida a la vez | 23 librerías detectadas, ML/DL Inspector (Fase 2). `static_parser.py` en sí es la pieza legacy que la Fase 18 está migrando fuera de Python, no un fixture permanente |
| Native Analysis | **Rust** | El motor de análisis estático, destino comprometido — parsing, AST, resolución de símbolos, complejidad, seguridad, calidad, grafos. Cada porción se sigue perfilando/benchmarkeando antes del swap (eso es el *cómo*, no el *si*) | `terminal-server` (manejo de PTY), `complexity-engine` (9–21× más rápido que `radon` — Fase 11), análisis Python rico portado (Fase 18) |
| Scientific/HPC | **Fortran** | *Objetivo* de análisis, no lenguaje de implementación — Sythrall ya tiene rendimiento numérico nivel Fortran gratis vía los backends LAPACK/BLAS compilados de numpy/scipy | Planeado (Fase 20) |
| Nivel máquina | **Assembly** | Objetivo de análisis para desglose de instrucciones/registros/control-flow — envuelve Capstone/LIEF en vez de escribir un disassembler a mano | Planeado (Fase 19) |
| Native tooling | **Zig** | Build, cross-compilación, distribución standalone — una preocupación distinta al rol de motor de análisis de Rust, no compite con él | Planeado (Fase 25) |

La regla para mover cualquier cosa a Rust (o cualquier lenguaje nativo) solía ser
un either/or de verdad: perfilar primero, benchmarkear el reemplazo, y quedarse con
él solo si los números lo justifican — la investigación de proyectos gigantes de la
Fase 10 encontró que el costo real O(n²) eran tres bugs de Python comunes, no el
parser, y se arregló sin lenguaje nuevo; el `complexity-engine` de la Fase 11
encontró una ganancia real, medida, de 9–21× y adoptó Rust. Los dos resultados
salieron del mismo proceso; ninguno se asumió de entrada.

Para el motor de análisis estático puntualmente, esa pregunta ya está resuelta: se
muda a Rust, completo, no solo donde un benchmark lo favorezca — ver el Native
Analysis Core de la Fase 18 más abajo. Lo que sobrevive de la regla vieja es el
*método*, no la *decisión*: cada porción se sigue portando, testeando por paridad
contra el Python que reemplaza, y benchmarkeando antes de que los call sites
cambien — la migración nunca cambia correctitud por velocidad de llegar a la meta.
El framing either/or todavía aplica a cualquier *otra* adopción futura de lenguaje
nativo fuera de esta migración puntual — esto es una excepción explícita y única,
no una reversión de la regla general.

### Hacia dónde va esto: Computer Science Intelligence, no un linter

La descripción honesta de Sythrall hoy es "lee código, calcula Big-O." Las Fases
13–23 de abajo son el plan para hacer crecer eso hacia algo más específico:
*Sythrall analiza software desde las estructuras matemáticas/algorítmicas de abajo,
hasta el compilador, el código máquina, y el hardware sobre el que corre.* No
convirtiéndose en un solver matemático ni en un compilador — conectando la teoría de
CS que ya explica *por qué* funcionan las heurísticas que Sythrall ya tiene (Big-O,
los clasificadores de la jerarquía de Chomsky de la Fase 8/12, el framing de Cálculo
Lambda sobre recursión tail-call) con el resto de la teoría de la que salen esas
ideas, y construyendo detectores para eso con el mismo método heurístico y
benchmark-primero con el que se construyó todo lo demás de este roadmap:

```
                    Computer Science Engine
                              │
        ┌─────────────────────┼─────────────────────┐
        │                     │                      │
   Algoritmos             Matemática              Lenguajes
        │                     │                      │
    Big-O/Θ/Ω            Matemática Discreta    Lenguajes Formales
    Estructuras de Datos  Lógica                Gramáticas
    Grafos                Recurrencias          Teoría de Parsing
        │                     │                      │
        └─────────────────────┼──────────────────────┘
                              │
                       Teoría de Compiladores
                              │
                    Lexer → AST → IR → Codegen
                              │
                        Código Máquina
                              │
                             CPU
```

Cada capa de abajo sigue gateada por las mismas reglas ya establecidas en este
roadmap: heurística y explícita sobre serlo (mismo estilo que los detectores de
WASM-hints/clasificadores CS Engine, no afirmando prueba semántica), y cualquier
adopción de un lenguaje nativo sigue necesitando su propio benchmark antes/después
(la regla de la Fase 18, ya probada dos veces).

### ✅ Fase 1 — Base
- [x] Backend Flask + pylint, flake8, radon
- [x] Frontend TypeScript + Vite sin frameworks
- [x] Monaco Editor + Chart.js + Docker

### ✅ Fase 2 — Inspector ML/DL
- [x] 23 librerías detectadas, 23 patrones de pipeline, 25 modelos
- [x] 20+ reglas de issues (data leakage, reproducibilidad, frameworks)
- [x] Score 0–100 + diagrama Mermaid del pipeline

### ✅ Fase 3 — Zoom/Pan + Responsive
- [x] Zoom y pan en diagramas, layout responsive completo, bottom navigation móvil

### ✅ Fase 4 — FastAPI + Upload de proyectos
- [x] Migración Flask → FastAPI, upload de archivos/carpetas/ZIPs, 83 tests

### ✅ Fase 5 — Análisis Estático + Editor Intelligence
- [x] Parser AST multi-lenguaje (Python · TypeScript · C/C++) sin IA
- [x] Estimación Big-O por función, complejidad ciclomática, hints Cython/WASM
- [x] Linting en tiempo real (fast ~1ms, heavy ~80ms), diagnósticos inline
- [x] Hover: firma + Big-O + CC + docstring
- [x] Go to Definition · Find References · Autocompletado semántico · Rename Symbol
- [x] 162 tests automatizados

### ✅ Fase 6 — Code Graph + Project Explorer
- [x] Import Graph · Call Graph · Dependencias Circulares
- [x] Sub-etapa A: grafos desde sidebar; Sub-etapa B: desde proyectos ZIP completos
- [x] Resolución cross-folder, detección circular con NetworkX
- [x] Force Graph interactivo *y* dir-tree de Complexity Heatmap (`renderForceGraph`/`renderDirTree` en `panels/graph.ts`, motor de física propio, sin D3) *(los dos implementados y testeados — un audit posterior encontró que todo el módulo de grafos no tenía ni un caller en la app; el Tree View con Mermaid quedó conectado en la Fase 10 más abajo, pero `generateWholeProjectDiagram` en `app.ts` todavía pasa `onForce`/`onDirTree` como no-ops (documentado en su propio docstring) — ninguno de los dos tiene control de UI todavía)*
- [x] Project Explorer: árbol + tabs múltiples + búsqueda global + outline
- [x] 316 tests automatizados

### ✅ Fase 7 — Panel de Problemas + Métricas en Vivo *(código shippeado, wiring incompleto encontrado después — ver Fases 10 y 12)*
- [x] **Panel de problemas** (estilo VSCode): errores · warnings · Big-O · complejidad · hallazgos de seguridad — *originalmente compartía contenedor DOM con la vista de análisis de archivo y la hubiera pisado; resuelto dándole su propio sub-tab del panel derecho (`#rpp-problems`) en vez de fusionar las dos vistas — ver Fase 12 más abajo*
- [x] **Barra de métricas en vivo** en el editor: LOC · funciones · imports · complexity score · Big-O peor caso · parse time (ms) *(conectado en la Fase 10 más abajo — el módulo existía desde esta fase pero `editor.ts` nunca lo llamaba)*
- [x] Auto-recovery si el parser falla (safe mode + fallback regex)
- [x] Detección de archivos corruptos
- [x] Restauración de sesión *(conectado en la Fase 10 más abajo, junto con persistir el proyecto activo — restaurar "qué archivo estaba abierto" recién tuvo sentido real cuando hubo un proyecto de verdad del cual recuperar el contenido)*

### ✅ Fase 8 — Computer Science Engine *(extensión directa del motor de análisis existente, sin arquitectura nueva)*

No solo "qué" hace el código — *por qué* se comporta así. Construido enteramente sobre datos que `static_parser.py` y el motor de Big-O ya calculan:

- [x] Complejidad completa por función: Θ (cota ajustada), Ω (mejor caso), O (peor caso) — no solo el O del peor caso *(solo Python por ahora; C/C++/JS/TS todavía muestran solo O)*
- [x] Explicación del "por qué" en cada resultado de Big-O (ej. *"2 loops anidados — el loop interno corre n veces por cada iteración del externo"*)
- [x] Recursión detectada → detección de tail-call + marco de "Cálculo Lambda" *(estimación de profundidad omitida — depende del input en runtime, no se puede calcular de forma estática confiable)*
- [x] Regex detectada → clasificar como Autómata Finito / Chomsky Tipo-3 (Regular) *(solo llamadas directas `re.XXX(...)` — no rastrea un `re.Pattern` guardado en variable)*
- [x] Código con forma de gramática/parser detectado → Gramática Libre de Contexto / Autómata con Pila / Chomsky Tipo-2 *(heurística: nombre + forma de recursión/pila explícita — se exigen las dos señales para no generar demasiados falsos positivos)*
- [x] Recorrido de grafo detectado → etiquetar como DFS/BFS/orden topológico, O(V+E) *(heurística: señales de nombre de variable — `visited`/`seen`/`explored`, `in_degree`, una cola con `.popleft()` — no un análisis de flujo de datos real)*

### ✅ Fase 9 — Terminal Integrada + Explorador de Carpetas + Tema

No estaba originalmente en este roadmap — salió directo de feedback del usuario a mitad de desarrollo, se sumó porque cada pieza era chica y estaba bien acotada por separado:

- [x] **Terminal integrada**, shell interactiva real sobre WebSocket — primer uso de **Rust** en el proyecto (sidecar `terminal-server`: `portable-pty` + `axum`), protegida por token, auto-conexión sin fricción para uso local, selector de panel entre la shell y una vista de Logs en vivo
- [x] **Explorador de carpetas** en el sidebar ("+ Carpeta") — árbol expandible estilo VSCode desde una carpeta real del disco, cross-browser vía `webkitdirectory` (deliberadamente *no* la File System Access API, que es solo Chromium)
- [x] Toggle de **tema claro/oscuro**, persistido, oscuro por defecto
- [x] [`ansimax`](https://github.com/Brashkie/ansimax) (librería propia) para el banner de arranque de `npm run dev`

### 🟡 Fase 10 — Rebrand + reestructuración a `apps/` + escalado a proyectos gigantes + shell estilo enterprise

No estaba originalmente en este roadmap — salió de querer que el proyecto aguante código real a gran escala y se vea/comporte como las herramientas de referencia mostradas durante el desarrollo (Aikido, Datadog, DeepSource), no de un plan de versiones:

- [x] Renombrado **CodeWatch PRO → Sythrall** en todo el proyecto (nombre del paquete, servicios Docker, identificadores internos, remote de git)
- [x] Reestructurado a `apps/api` · `apps/web` · `apps/terminal` — un directorio por servicio, los manifiestos de cada herramienta se quedan en la raíz (layout estilo Turborepo/Nx) *(`apps/terminal` se mudó a `services/terminal` después, separando productos de cara al usuario de procesos independientes — ver [`CHANGELOG.md`](CHANGELOG.md))*
- [x] **Benchmark de proyectos gigantes**: se armó un harness reproducible con proyectos sintéticos (hasta 4003 archivos, hasta 1600 funciones por archivo) en vez de asumir que hacía falta una reescritura. Encontró y arregló tres bugs reales O(n²) — dos escondidos dentro de comprehensions de una línea, uno un cálculo muerto que el frontend nunca leía. La generación del Import Graph con 4003 archivos pasó de 3.9s a 0.128s (30×) sin agregar ningún lenguaje nuevo. Detalle en [`CHANGELOG.md`](CHANGELOG.md#460). El parser propio (`static_parser.py`) ya era lineal y no necesitó cambios — el hallazgo anterior sobre PyO3 sigue vigente.
- [x] **Nav vertical reemplaza la tabbar horizontal** — nav de iconos persistente (`apps/web/src/utils/icons.ts`, SVGs inline, `stroke="currentColor"` para seguir el tema activo sin código extra), mismo patrón que usan las herramientas de referencia. `switchTab()` no cambió — los items del nav nuevo mantuvieron la convención `class="tab"`/`data-tab`/`id="t-*"`, así que fue un cambio puramente de HTML/CSS.
- [x] **Un solo proyecto activo, no cuatro entradas separadas.** Antes: "+ Código"/"+ Carpeta"/"+ Log" del sidebar eran efímeros (se perdían al refrescar, nunca tocaban el backend) mientras que Proyectos era el único camino persistente — dos modelos mentales para la misma idea. Ahora "+ Código"/"+ Carpeta" crean o suman al **proyecto activo** (mismos endpoints del backend que ya usaba Proyectos, `project_id` ahora opcional en `/api/upload/{files,folder}` para soportar el append), y Editor · Issues · Diagrama · Static · Métricas leen todos del proyecto que esté activo — elegís un proyecto una vez, trabajás en todos los paneles.
- [x] **Arreglos encontrados por audit**, con el mismo método de "¿esto tiene algún caller de verdad?" que encontró el hueco del Force Graph arriba: se reconectaron la Live Metrics Bar y Session Restore (`panels/problems.ts`, escrito para la Fase 7, nunca llamado desde `editor.ts`); el proyecto activo ahora persiste en `localStorage` así que ambos se restauran solos al recargar; se arregló el badge de la pestaña APIs (nunca se actualizaba); el panel de Métricas ganó un modo de proyecto activo igual que Issues/Diagrama/Static.
- [x] Ubicación del **panel de problemas** — todavía necesita una decisión (ver nota de la Fase 7 arriba) antes de poder conectarse sin pisar la vista de análisis de archivo existente. *(resuelto en la Fase 12 más abajo — sub-tab propio del panel derecho en vez de compartir contenedor)*

### ✅ Fase 11 — `radon` reemplazado por un sidecar Rust propio (`complexity-engine`)

No fue una reescritura por rendimiento — el motivo fue no querer depender de la lógica interna de una librería de terceros para algo que el proyecto puede tener propio, siendo un codebase que mantiene una sola persona. Medido antes de afirmar cualquier ganancia de velocidad, siguiendo el mismo criterio benchmark-primero que el trabajo de escalado a proyectos gigantes de arriba:

- [x] **Nuevo sidecar Rust `apps/complexity`** *(mudado a `services/complexity` después — ver [`CHANGELOG.md`](CHANGELOG.md))*, misma arquitectura que `terminal-server` (proceso persistente, HTTP, no subprocess por llamada ni extensión nativa PyO3) — `rustpython-parser` para el AST, código propio del proyecto para complejidad ciclomática (McCabe), Maintainability Index (fórmula Coleman-Oman) y métricas raw de líneas. Los dos sidecars ahora comparten un solo `Cargo.toml` en la raíz (2 `[[bin]]`, 1 `[lib]`).
- [x] **Benchmarkeado con Criterion contra `radon` real**, no asumido: 10 funciones — 0.42ms vs 8.97ms; 100 funciones — 4.7ms vs 89ms; 1000 funciones — 102ms vs 899ms. 9–21× más rápido, medido sobre los mismos archivos sintéticos en ambos lados.
- [x] `radon==6.0.1` eliminado de `requirements.txt`; `services/complexity_client.py` le pega al sidecar por HTTP y degrada con gracia (complejidad/MI vacíos, sin crash) si no está corriendo — mismo patrón de capacidad opcional que flake8/pylint.
- [x] Se arregló un bug real de condición de carrera encontrado al conectar esto: el diseño viejo cacheaba "¿está disponible la herramienta?" una sola vez al arrancar el backend, así que un primer `cargo build` lento podía dejar la capacidad trabada en `false` el resto de la sesión aunque el sidecar ya estuviera arriba. Los call sites reales del análisis ya no dependen de ese flag cacheado — le pegan al sidecar en vivo y degradan con gracia por pedido en vez de por sesión.
- [x] 15 tests unitarios en Rust (`cargo test`) para complejidad/MI/raw-metrics contra valores calculados a mano, corridos por el mismo job de CI que ya compilaba `terminal-server`.

### ✅ Fase 12 — Cerrados los últimos 3 clasificadores del CS Engine + ubicación del Problems Panel

Cierra los ítems de roadmap del CS Engine de la Fase 8 y la decisión pendiente del Problems Panel de la Fase 7/Fase 10:

- [x] **Regex → Chomsky Tipo-3 (Regular)**: detecta llamadas directas `re.compile/match/search/findall/...` por función. Honesto sobre su límite — no rastrea un `re.Pattern` guardado en variable, solo llamadas directas `re.XXX(...)`.
- [x] **Código con forma de gramática/parser → Chomsky Tipo-2 (Context-Free)**: la heurística exige *ambas* señales — nombre (`parse`/`grammar`/`tokenize`/`lexer`/...) *y* forma (recursión o patrón explícito de pila append/pop) — cualquiera de las dos sola generaba demasiados falsos positivos en las pruebas (un `factorial` recursivo plano no es un parser solo por ser recursivo).
- [x] **Recorrido de grafo → BFS/DFS/Orden Topológico, O(V+E)**: heurística sobre nombres de variable (`visited`/`seen`/`explored`, `in_degree`) más una cola (`.popleft()`) vs. forma de pila/recursión para distinguir BFS de DFS. Mismo estilo explícitamente heurístico, no-análisis-semántico, que el detector de WASM-hints ya existente (`_wasm_hints_python`) — el código nuevo sigue exactamente sus convenciones.
- [x] **El Problems Panel consiguió su propio lugar**: un 4to sub-tab del panel derecho (`Flujo · Análisis · Servidor · Problems`, `#rpp-problems`/`#problems-content`) en vez de intentar fusionarlo con la vista de análisis de archivo existente — resuelve el conflicto de contenedor DOM documentado desde la Fase 7 sin tocar el contenido más rico de esa vista (Pylint score, MI, tabla por función). Conectado en `editor.ts::applyMarkers()`, exactamente donde `panels/problems.ts` documentaba el punto de integración pensado desde que se escribió.
- [x] Eliminados 3 exports confirmados muertos, sin callers, re-verificados dos veces (`editor.ts::copyEditorContent`, `explorer.ts::explorerMarkModified`/`explorerRefresh`). Una lista más larga de exports sin caller *visible* se encontró en la misma pasada pero se dejó a propósito — no hay suficiente certeza de que no sean superficie de API para una feature que todavía no aterrizó (misma situación que tuvo el Force Graph antes de conectarse).

---

Las fases de abajo son los 11 pilares conceptuales de "Computer Science Intelligence" (ver arriba) más dos fases finales que quedan fuera de esa teoría — una arquitectónica, una de productización — agrupadas según qué tan fundamentadas están, no según qué tan lejos en el futuro caen. Las Fases 13–14 son la continuación más directa de lo que la Fase 8/12 ya shippeó; 15–17 son terreno teórico genuinamente nuevo; 18–20 construyen sobre la tabla de filosofía de lenguajes; 21–22 llenan un hueco que la pasada original de 9 pilares no vio (seguridad, calidad de código) — mismo estilo heurístico con confianza puntuada que todo lo demás acá, no una metodología nueva; 23 es arquitectónicamente distinta a todo lo de arriba (necesita un proceso corriendo, no solo texto fuente); 24–25 cierran el círculo — 24 es el límite de API que dejaría crecer las Fases 13–23 como plugins en vez de que cada analizador termine directo en `apps/api`, 25 es cómo el producto realmente llega a la gente una vez que hay algo para mostrar.

### 🟡 Fase 13 — Algorithmic Intelligence *(extiende el motor O/Θ/Ω de la Fase 8, sin arquitectura nueva)*

La Fase 8 ya calcula O del peor caso, Ω del mejor caso, y Θ de cota ajustada por función — una aplicación específica y estándar de la notación asintótica en análisis de algoritmos (misma convención que usan CLRS y la mayoría de los textos de algoritmos). Vale la pena hacerlo explícito en vez de implícito, ya que las definiciones generales son más amplias que ese uso puntual:

- [x] **Referencia de notación asintótica**, mostrada donde ya aparece un resultado Big-O: una tabla `<details>` colapsable arriba de la tabla Big-O del panel Static (`panels/static.ts::_renderBigOTable`) cubre los cinco símbolos — O (cota superior), Ω (cota inferior), Θ (cota ajustada), o (cota superior estricta), ω (cota inferior estricta) — definición general primero, honesto en que `o`/`ω` no se calculan (no hay heurística estática confiable que distinga "estricto" del caso ajustado). El tooltip de hover en Python (`_build_hover_python`) recibe la misma explicación como nota de una línea debajo de su tabla O/Θ/Ω. El hover de JS/TS queda sin tocar a propósito — solo calcula O, no Θ/Ω, así que la referencia completa todavía no aplica ahí.
- [x] **Complejidad de espacio** junto a la de tiempo, construida en Rust y Python desde la misma pasada esta vez (`services/complexity/src/space.rs` + `static_parser.py::_infer_space_python`, las dos cableadas el mismo día) — mismo enfoque heurístico basado en AST que el motor de tiempo (sin ejecución), pero la señal es *qué estructura auxiliar se construye*, no *cuántas veces corre un loop*: un loop que solo acumula en un escalar (`total += x`) es O(1) de espacio aunque sea O(n) de tiempo. Detecta colecciones que crecen (`.append`/`.add`/`.update`/`.insert`/`.extend`, o asignación por subscript `d[k] = v`) según la profundidad de anidamiento de loop (O(n) en profundidad 1, O(n²) en profundidad 2 — una matriz construida en loops anidados), comprehensions anidadas (`[[0 for _ in range(n)] for _ in range(n)]`), y profundidad del call stack por recursión (O(log n) para recursión con división binaria, reusando el detector de split que ya tiene `bigo.rs`; O(n) para recursión lineal, porque Python no optimiza tail calls). 8 tests unitarios en Rust + 8 en Python (`TestSpaceComplexity`), paridad byte a byte confirmada a mano en los 8 casos. Cableado en `/intel/hover` y `/intel/analyze` (Rust primero, fallback a Python, mismo gate que Big-O) y una columna **Space** nueva en la tabla Big-O del panel Static. Benchmarkeado con Criterion contra el tiempo real de `_parse_python()` sobre los mismos archivos sintéticos: 10 funciones — 0.77ms vs 18.0ms (23.4×); 100 — 9.17ms vs 212.6ms (23.2×); 1000 — 254ms vs 2083ms (8.2×) — y reportado con honestidad que agregar este pase hizo a `analyze_rich()` mismo 30–59% más lento contra su propio baseline previo (dos recorridos completos más por función se acumulan), no solo la ganancia contra Python.
- [ ] **Reconocimiento de relaciones de recurrencia** para funciones divide-and-conquer — ej. el patrón `T(n) = 2T(n/2) + Θ(n)` de `merge_sort` matcheado contra los tres casos del Teorema Maestro en vez de caer en la heurística genérica de loop/recursión ya existente, llegando a `Θ(n log n)` mostrando la recurrencia, no solo la respuesta

### 🟡 Fase 14 — Data Structures & Graph Intelligence *(mismo estilo heurístico que los detectores de WASM-hints/dead-code ya existentes)*

Continuación directa del CS Engine (Fase 8/12) — mismo enfoque de análisis estático, sin arquitectura nueva. No solo nombrar la estructura, explicarla:

- [ ] Detectar AVL / Red-Black Tree / Trie / Heap / Segment Tree / Fenwick Tree / Bloom Filter / B-Tree / HashMap / Skip List a partir de la forma del AST
- [ ] Por cada detección: complejidad (tiempo *y* espacio), operaciones típicas, casos de uso, ventajas/desventajas contra las alternativas — el mismo estándar de "por qué", no solo "qué", que ya se exige el motor de Big-O
- [x] **Algoritmos de grafo como operaciones de primera clase, primera porción: centralidad/detección de hubs** (`routers/graph.py::_build_centrality_graph`) — nuevo tipo de grafo `centrality` que reusa el import graph que ya se construye para los otros 4 (Import/Call/Circular/Heatmap), puntuado con `nx.degree_centrality`/`in_degree`/`out_degree` sobre el mismo `DiGraph` de NetworkX que el detector de dependencias circulares ya construye — no una librería nueva, la misma que ya se ganaba su lugar. Un archivo se marca como "hub" cuando está entre los 5 con mayor in-degree *y* tiene al menos 2 dependientes (evita marcar como hub un archivo con una sola import entrante). Cableado en el dropdown de proyecto del panel Diagrama (`index.html`, `PROJECT_GRAPH_TYPES` en `app.ts`) — alcanzable por un usuario, no solo una respuesta de API, la misma disciplina de "¿esto tiene un control de UI?" que enseñó el hueco del Force Graph. 8 tests de backend (`TestCentralityGraph`). **BFS/DFS/camino más corto/componentes conexas/orden topológico sin empezar todavía** — esta porción es el ítem puntual que el texto del roadmap ya nombraba ("centralidad/detección de hubs... mostrar los archivos más conectados"); el resto de la lista de algoritmos de grafo sigue abierto. Deliberadamente todavía Python (`networkx`, no Rust) — el Graph Engine de la Fase 18 necesita la *construcción* del import/call graph en Rust primero, que todavía no existe; portar solo los algoritmos antes de eso significaría mantener dos representaciones de grafo separadas.
- [ ] **Detección de interacción entre estructuras anidadas** — ej. una hash table recorrida dentro de un loop que también toca esa misma hash table, marcada como un posible recorrido O(n²); análisis de complejidad que mira cómo las estructuras *se combinan*, no solo complejidad por función aislada

### 🔴 Fase 15 — Mathematical Intelligence *(matemática discreta usada para explicar programas, no un solver matemático)*

No es "Sythrall demuestra teoremas." Los conceptos de matemática discreta que ya sostienen el resto de este roadmap, hechos explícitos donde ya están implícitos en lo que Sythrall detecta:

- [ ] Conjuntos y relaciones — las operaciones de `dict`/`set` ya detectadas (el clasificador de recorrido de grafo de la Fase 8 lee `visited`/`seen` como conjuntos) enmarcadas explícitamente como operaciones de conjuntos, no solo heurísticas de nombre de variable
- [ ] Funciones — clasificación pura vs. con efectos secundarios (¿la función solo lee sus argumentos y retorna, o muta estado externo?), una propiedad estática real, no una heurística adivinada
- [ ] Combinatoria — la cardinalidad de loops anidados ya calculada para Big-O (la señal de profundidad de loop de la Fase 8) reencuadrada explícitamente como el conteo combinatorio que en realidad es
- [ ] Álgebra de Boole — hints de simplificación de De Morgan sobre condicionales complejos (`not (a and b)` ⟷ `(not a) or (not b)`), mostrados como nota de legibilidad, no como reescritura automática
- [ ] Framing de demostración por inducción sobre funciones recursivas que ya tienen caso base + paso inductivo detectados (el análisis de recursión de la Fase 8 ya encuentra ambos) — una nota explicativa, no una prueba generada

### 🔴 Fase 16 — Formal Language Intelligence *(completa la jerarquía de Chomsky que la Fase 8/12 ya empezó)*

La Fase 8/12 ya shippea dos niveles de esto: regex → Chomsky Tipo-3 (Regular), código con forma de gramática/parser → Chomsky Tipo-2 (Context-Free). Esta fase completa la jerarquía como referencia educativa y de clasificación:

- [ ] **Tipo-1 (Context-Sensitive) / Autómata Linealmente Acotado** y **Tipo-0 (Recursivamente Enumerable) / Máquina de Turing** — los dos niveles que faltan, agregados donde Sythrall encuentre una señal AST concreta para ellos, documentados con honestidad donde todavía no pueda
- [ ] **Panel de referencia de la Jerarquía de Chomsky** — cada tipo de gramática emparejado con el autómata que la reconoce (Regular → Autómata Finito, Context-Free → Autómata de Pila, Context-Sensitive → Autómata Linealmente Acotado, Tipo-0 → Máquina de Turing), enlazado desde donde ya dispara una clasificación regex/gramática hoy

### 🔬 Fase 17 — Compiler Intelligence *(integrar herramientas maduras, no reconstruirlas — todavía un spike de investigación, no comprometido)*

El framing de Cálculo Lambda ya existe sobre la recursión tail-call (Fase 8) — esta fase es donde esa teoría se conecta con un pipeline de compilador real, no un compilador propio de Sythrall:

- [ ] Visualización del pipeline del compilador (Lexer → AST → IR → optimización → codegen) — integrar [Compiler Explorer](https://godbolt.org) (open source) en vez de construir un compilador educativo desde cero
- [ ] Vista a nivel IR de la reescritura tail-call que la Fase 8 ya explica en prosa — mostrar cómo se ve el IR de una función tail-recursive reducida a un loop, en vez de solo afirmar que "podría" serlo

### 🟡 Fase 18 — Native Analysis Core *(migración comprometida: el rol de `static_parser.py` se muda a Rust, completo — no solo donde un benchmark lo favorezca)*

`complexity-engine` (Fase 11) fue la prueba de concepto; esta fase es la decisión que esa prueba justificó. **El motor de análisis estático se muda a Rust, completo** — parsing, construcción de AST, resolución de símbolos, complejidad, seguridad, grafos, métricas de calidad, todo, consolidado progresivamente en un core nativo modular. El rol de Python se acota a lo que el ML/DL Inspector de la Fase 2 ya hace bien: IA, ML, cargas científicas — no parsear código fuente. El estado final **no tiene capa de compatibilidad permanente ni doble implementación**: una vez que una pieza se porta y se prueba correcta, la versión Python que reemplazó se elimina, no se queda "por las dudas". `static_parser.py` en sí es el objetivo de la migración, no un fixture que la sobrevive.

Lo que no cambia: *cómo* se mueve cada porción sigue siendo disciplinado — se porta, se testea por paridad contra el Python que reemplaza sobre el mismo input, se benchmarkea con Criterion, y solo entonces se conecta a los call sites en vivo, exactamente el proceso que ya usó la primera porción de abajo. El compromiso es con el destino; el rigor está en cómo se llega a cada paso, para que migrar rápido nunca signifique migrar con descuido.

- [x] **Análisis Python rico, portado a `complexity-engine`** — el trabajo por función/clase/import de `_parse_python()` (Big-O, Θ/Ω, complejidad ciclomática, recursión tail-call, los 3 clasificadores del CS Engine de la Fase 12) ahora también corre en Rust, expuesto como `POST /parse/python`, con paridad byte a byte contra la implementación Python sobre el mismo archivo (solo difiere el idioma del texto de razón — inglés en Rust, español en Python, a propósito). Benchmarkeado con Criterion contra el tiempo real de `_parse_python()` sobre los mismos archivos sintéticos: 10 funciones — 0.48ms vs 9.94ms (20.6×); 100 — 5.95ms vs 100.7ms (16.9×); 1000 — 187ms vs 1038ms (5.6×) — el margen se achica a mayor escala, reportado tal cual se midió, no redondeado para arriba. Conectado en `/static/bigO`, `/intel/analyze` (el "heavy path" que corre a los ~2s de inactividad mientras se tipea) y `/intel/hover` (solo la porción de clasificación CS Engine — el texto de firma del hover sigue siendo AST en Python, porque `RichFunction` de Rust solo trae nombres de argumento planos, sin anotaciones de tipo), cada uno chequeado en vivo por pedido sin gate de flag cacheado, misma lección del arreglo de condición de carrera de la Fase 11. **Todavía no conectado en `/static/parse`**: ese endpoint devuelve el shape legacy completo (`dead_code`, `call_graph`, `circular_deps`, `wasm_hints`, `exports`) al frontend, nada de lo cual esta porción calcula todavía — los siguientes cuatro ítems son exactamente lo que falta antes de que pueda mudarse.
- [ ] **Graph Engine** — construcción y recorrido de import/call graph para proyectos grandes, hoy Python puro en `graph.py`; la pieza que tanto `/static/parse` como el trabajo de algoritmos de grafo de la Fase 14 necesitan en Rust antes de poder dejar Python atrás
- [ ] **Dependency Engine** — detección de dependencias circulares y resolución cross-file a escala de proyecto — la otra mitad que `/static/parse` necesita, junto con WASM hints y detección de dead-code, para retirar del todo su path en Python
- [ ] **Symbol Engine** — go-to-definition / find-references sobre codebases grandes, hoy basado en regex/AST por archivo
- [ ] **Project Scanner** — el fan-out de recorrer+parsear archivos para análisis de proyecto completo (`read_project_files` y afines)
- [x] **Security, primera porción portada** — `security.rs`: taint tracking + SQL/Command Injection + credenciales hardcodeadas, portado el mismo día que se shippeó la versión Python (Fase 21), misma disciplina de benchmarkear-y-después-swap que cada otra fila de acá. Path Traversal/Deserialización insegura (todavía solo Python) y Code Quality (Fase 22, sin empezar en ningún lenguaje todavía) quedan en cola — el paso de "escribir en Python primero" sigue siendo legítimo para una heurística genuinamente nueva que todavía itera sobre falsos positivos, ya no es el lugar de descanso por default
- [ ] **`static_parser.py` eliminado** — la meta real: una vez que cada endpoint que lo lee hoy (`/static/parse`, `/static/bigO`, `/intel/*`, `graph.py`) lea del core Rust en su lugar, el archivo Python se elimina, no se deja deprecado-pero-vivo

```
                    Sythrall
                       │
                 Native Analysis Core (Rust)
                       │
       ┌───────────────┼────────────────┐
       │               │                │
    Parsing          Analysis         Graph
   AST/símbolos  Complexity/Security  CFG/DFG
                       │
              Deep Analysis
       ┌───────────────┼───────────────┐
       │               │               │
    Security        Quality       Performance
```

Una cosa que esto deliberadamente NO es: `static_parser.py` → un solo `static_parser.rs` gigante. La división modular de arriba — módulos Rust separados por responsabilidad, el mismo patrón que ya usan `bigo.rs`/`classifiers.rs`/`recursion.rs`/`structure.rs` dentro de `rich.rs` hoy — es a donde porta cada ítem de arriba, no un port monolítico único. Y una vez que exista la Fase 24 (Extensibility Platform), este core tampoco tiene que cargar con todo para siempre — catálogos de CWE, reglas adicionales, soporte de lenguajes nuevos, y modelos de explicación de IA son exactamente el tipo de cosa que pertenece como plugin *encima* de estos primitives, no hardcodeado para siempre dentro del core mismo.

### 🔬 Fase 19 — Machine Intelligence *(el lado Assembly de la tabla de filosofía de lenguajes — integrar herramientas maduras, no reconstruirlas)*

Análisis de en qué *se convierte* el código, no solo qué dice. Cada uno de estos es su propio proyecto serio ya resuelto bien por herramientas open-source dedicadas — lo honesto es integrarlas, no reinventarlas:

- [ ] Soporte **Assembly (x86-64)** como lenguaje-objetivo — desglose de instrucciones/registros/control-flow a partir de snippets `.s`/asm inline pegados por el usuario *(pattern-matching sobre texto, no un disassembler)*
- [ ] Analizador de ejecutables (PE / ELF / Mach-O, secciones, imports/exports, símbolos) — envolver [Capstone](https://www.capstone-engine.org)/[LIEF](https://lief-project.github.io)/`objdump`, no escribir un disassembler a mano
- [ ] Explicadores de calling-convention y stack-frame ligados a la vista de Assembly una vez que exista — conecta la teoría (por qué el prólogo de una función se ve como se ve) con los bytes reales, cerrando el círculo desde la vista IR de la Fase 17 hasta el código máquina real

### 🔴 Fase 20 — Scientific Intelligence *(Fortran, más allá de una sola bala)*

Fortran como lenguaje-objetivo, conectado al stack numérico que Sythrall ya trae (los backends LAPACK/BLAS compilados de numpy/scipy) — no un lenguaje en el que el motor propio de Sythrall necesite estar escrito:

- [ ] Detección de loops `DO`/operaciones con arrays, candidatos a vectorización y SIMD
- [ ] Reconocimiento de algoritmos numéricos (operaciones con matrices, descomposiciones) con framing específico de dominio, ej. *"Multiplicación de matrices — O(n³), candidatos: SIMD, blocking, paralelización — dominio: HPC/Computación Numérica"* en vez de una etiqueta Big-O pelada
- [ ] Detección de uso de BLAS/LAPACK — marcar dónde un proyecto ya se apoya en backends numéricos compilados en vez de reimplementar algo que ya proveen

### 🟡 Fase 21 — Security & Taint Intelligence *(análisis de flujo de datos basado en patrones, con confianza puntuada — no un reemplazo de SAST)*

Un hueco que la pasada original de 9 pilares no vio: nada en las Fases 13–20 mira seguridad. Continuación directa del estilo del CS Engine — los clasificadores de regex/grammar de la Fase 8/12 ya demuestran que "patrón heurístico + confianza etiquetada con honestidad" funciona — aplicado a flujo de datos source→sink en vez de forma algorítmica. Nada acá debería presentarse jamás como "esto ES una vulnerabilidad", solo como un patrón que merece la atención de una persona, con la evidencia mostrada para que la afirmación sea auditable. Primera porción hecha:

- [x] **Taint tracking dentro de una sola función** — un recorrido recursivo que arrastra procedencia resuelve un valor hasta una fuente no confiable (`request.args`/`form`/`values`/`json`/`GET`/`POST`/`COOKIES`/`headers`, `input()`, `sys.argv`, `os.environ`/`os.getenv`) a través de asignaciones, concatenación de strings, f-strings y `.format()`, rastreando además *si* el taint se armó por construcción de string (la señal real de riesgo de SQLi/command injection) o solo pasó de largo. **Deliberadamente no cross-function** — el taint interprocedural necesita el call graph que el Dependency Engine de abajo todavía no construye. **Portado a Rust el mismo día que se shippeó en Python** (`services/complexity/src/security.rs`), no dejado como un "después" — ver la nota de Native Analysis Core más abajo sobre por qué. 19 tests unitarios en Python (`test_security_findings.py`) + 9 en Rust (`security::tests`), ambos cubriendo la forma vulnerable y la forma segura/parametrizada de cada check, paridad byte a byte entre los dos confirmada a mano sobre los mismos inputs.
- [x] **Catálogo CWE v1, 3 de 5 shippeados**: SQL Injection (CWE-89) — dispara solo cuando el query se *arma* por concatenación/f-string/`.format()`, así que `cursor.execute("...%s...", (val,))` correctamente no produce finding; Command Injection (CWE-78) — `os.system`/`os.popen` (siempre corren en shell) o `subprocess.*(..., shell=True)`, `subprocess.run([...])` sin `shell=True` correctamente no produce finding; credenciales hardcodeadas (CWE-798) — heurística de nombre+forma (variable con nombre de credencial asignada a un literal string no-placeholder), a nivel de archivo en vez de por función porque es genuinamente ahí donde viven. **Path Traversal (CWE-22) y Deserialización insegura (CWE-502) sin empezar** — diferidos, no descartados en silencio
- [x] **Findings con confianza puntuada** (por ahora Alta/Media, ningún caso dispara Baja todavía), nunca un sí/no binario — refleja el precedente de "ambas señales requeridas" del clasificador de grammar de la Fase 12: una fuente sola no es un finding, fuente *y* la señal de concatenación/sink disparando juntas sí lo es
- [x] **Schema de finding en árbol de evidencia** — categoría → CWE → severidad → confianza → fuente → sink → línea → recomendación, una sola forma compartida para cada finding. Mostrado en el **panel Static** (`panels/static.ts::_renderSecurityFindings`, una sección nueva arriba de la tabla Big-O) y en el **hover** de Python (`_build_hover_python`, Rust primero vía `parse_python_rich` con el mismo fallback a Python por función que ya usa cada otro clasificador acá) — **todavía no en el Problems Panel**: ese tab se alimenta de markers de lint por cada tecla, y los findings de seguridad corren hoy sobre el mismo pase pesado `/static/parse` que ya usan los WASM hints, que viven en Static, no en Problems
- [x] **2 bugs reales encontrados y corregidos la misma semana que esto se shippeó**, ambos por una auditoría independiente, no autodetectados: (1) reasignar una variable tainted a un literal seguro (`cmd = request.args.get("x"); cmd = "ls -la"`) no limpiaba su taint, produciendo un falso positivo de confianza Alta sobre código defensivo ordinario; (2) una función anidada que reusaba el nombre de una variable externa filtraba taint entre scopes. Los dos se corrigieron haciendo que el recorrido de taint sea scope-aware (`_own_scope_nodes` en Python, `walk_stmts_own_scope` en Rust — una variante del walker genérico de AST que se detiene en los límites de `FunctionDef`/`Lambda` anidados en vez de descender a ellos), con tests de regresión en ambos lenguajes.
- [ ] Explícitamente fuera de alcance, permanentemente: análisis de exploits completo (ROP, heap spray, use-after-free) — ya correctamente cercado bajo Execution Intelligence (Fase 23 de abajo) como "compite directamente con herramientas SAST maduras"; esta fase se queda dentro de ese mismo límite, solo que estática y más acotada

**Native Analysis Core, aplicado de verdad**: esta es la primera porción de la Fase 18 portada a Rust la misma semana en que se escribió en Python, no puesta en cola para "después" — una respuesta directa a hacer la migración de verdad en vez de solo planearla. Benchmarkeado con Criterion contra el tiempo real de Python (`_parse_python()`, que ahora también corre el mismo pase de seguridad) sobre los mismos archivos sintéticos usados en cada benchmark previo de este proyecto: 10 funciones — 0.52ms vs 11.97ms (23×); 100 — 6.37ms vs 129.0ms (20.3×); 1000 — 191.7ms vs 1316.9ms (6.9×). Agregar el pase de taint hizo a `analyze_rich()` mismo medible más lento (Criterion marcó una regresión de 2.7–8% contra su propio baseline previo) — reportado con honestidad en vez de mostrar solo la ganancia contra Python.

### 🔴 Fase 22 — Code Quality Intelligence *(el Maintainability Index que la Fase 11 ya shippea, desglosado en sus partes auditables, más los smells que un solo número de MI no puede expresar)*

El Maintainability Index de `complexity-engine` (Fase 11) ya calcula un Volumen de Halstead internamente (`maintainability.rs::Halstead::volume()`) pero solo lo expone precocinado dentro de una fórmula. Esta fase muestra los componentes de los que esa fórmula está hecha, y agrega los smells estructurales/de nombres/arquitectura que un solo número de MI no puede capturar por sí solo:

- [ ] **Métricas de Halstead, desglosadas**: Vocabulario (η1+η2), Longitud (N1+N2), Volumen (ya calculado para el MI), Dificultad, Esfuerzo — su propia tabla junto al score de MI existente en vez de quedarse como un input opaco de una sola fórmula
- [ ] **Smells estructurales**: función larga, clase grande, anidamiento profundo, exceso de parámetros, lógica duplicada (comparación de forma de AST, no diff textual), god object — mismo estándar de "por qué, no solo qué" que ya se exige el motor de Big-O, cada smell trayendo su umbral y su razonamiento, no una etiqueta sola
- [ ] **Smells de nombres**: nombres de una sola letra fuera de scopes de loop ajustado, casing inconsistente dentro de un mismo archivo, shadowing de un nombre de scope externo — intencionalmente conservador, marca solo casos mecánicamente verificables en vez de juicios de "nombre poco claro" que necesitarían un LLM para arbitrar
- [ ] **Smells de arquitectura**: coupling/cohesion por módulo, construido sobre el import graph que ya existe (Fase 6/14) — conteos de acoplamiento aferente/eferente, inestabilidad, el detector de dependencias circulares ya shippeado reformulado como una instancia de un chequeo general de violación de capas
- [ ] **Dashboard de calidad** — una vista de resumen (Security/Quality/Performance/Architecture) que junta números de las Fases 21/22/13/14, no un modelo de scoring nuevo inventado solo para el dashboard

### 🔴 Fase 23 — Execution Intelligence *(instrumentación en runtime — un tipo de herramienta distinto a todo lo de arriba)*

Todo en las Fases 1–22 es análisis estático: texto fuente entra, hechos salen, sin necesidad de ejecución. Esta fase es arquitectónicamente distinta — necesita un proceso corriendo, ptrace/eBPF, o captura de paquetes en vivo, por eso se quedó como idea "de largo plazo" no comprometida durante mucho tiempo. Numerada acá para ser honestos de que es un destino real, no para afirmar que está cerca:

- [ ] Visualizador de memoria (stack/heap/data/bss) — requiere un proceso corriendo para inspeccionar, no texto fuente
- [ ] Analizador de concurrencia (race conditions, deadlocks, mal uso de mutex/atomic) — necesita ejecución real o herramientas como ThreadSanitizer, no inspección de AST
- [ ] Motor de SO (threads, paging, scheduling, IPC) — necesita tracing a nivel de kernel
- [ ] Analizador de redes (TCP/TLS/QUIC/WebSocket) — necesita captura de paquetes; esto es una herramienta con forma de Wireshark, no un analizador estático
- [ ] Analizador de seguridad más allá de detección de patrones (ROP, heap spray, use-after-free) — compite directamente con herramientas SAST maduras (Semgrep, CodeQL, Bandit); la versión realista se integra a la Fase 21 de arriba como "detectar el patrón + explicar el CWE", no un motor de análisis de exploits completo

### 🔴 Fase 24 — Extensibility Platform *(el límite de API que dejaría crecer las Fases 13–23 sin que cada feature termine en `apps/api` — una herramienta interna, no un marketplace público, hasta que haya demanda real de terceros)*

Cada fase de arriba se agrega directamente al código propio de Sythrall — razonable mientras lo mantiene una sola persona, pero cada una de las Fases 13–23 es realistamente del tamaño de un plugin por sí sola. Esta fase es el límite que dejaría que ese trabajo pase afuera del core sin que cada analizador termine siendo un archivo de `routers/` que Sythrall tiene que mantener para siempre. Deliberadamente acotada para una primera porción — sin marketplace, sin sandboxing, sin modelo de confianza para terceros — porque nada de eso tiene razón de existir antes de que un segundo plugin real, más allá de los que Sythrall mismo shippea, lo necesite de verdad:

- [ ] **Manifest de plugin + interfaz de capability** — un plugin declara qué analiza (`language`, `security`, `performance`, ...) y qué necesita (`ast`, `metrics`, `source`) en un manifest chico y tipado; los propios parsers de Python/JS/TS de Sythrall se vuelven las primeras implementaciones "built-in" de esa misma interfaz, probando que el límite es real antes de que un tercero lo toque
- [ ] **Un tipo de plugin shippeado de punta a punta** — la prueba concreta de si la interfaz es realmente usable, no un segundo sistema paralelo construido al lado. El trabajo de Fortran de la Fase 20 es el candidato natural: análisis numérico/científico como "plugin de lenguaje" en vez de otra rama hardcodeada en `static_parser.py`
- [ ] **Separación extension vs. plugin**, siguiendo una distinción que ya está implícita en cómo está organizado `apps/web` hoy: un *plugin* agrega un analizador (nuevos tipos de finding, nuevo lenguaje, nueva regla) y solo necesita la interfaz de capability de arriba; una *extension* agrega UI (un panel nuevo, de la misma forma en que el tab de Problems de la Fase 12 ya es uno) y consume la salida de un plugin sobre la misma forma JSON que ya lee cada panel — sin arquitectura nueva para las extensions, solo un nombre documentado para una costura que ya existe de manera informal
- [ ] **IA como capa de explicación opcional, nunca como el detector** — una interfaz `AIProvider` (ONNX/GGUF local, API remota, o ninguna configurada) que un plugin puede llamar para convertir `Evidence` (la ruta de flujo de datos de la Fase 21, el razonamiento de Big-O de la Fase 13) en prosa, con el finding determinista producido y completamente usable con cero IA configurada — la misma forma que ya tiene hoy el campo `reason` del Big-O, solo que opcionalmente pasado a un modelo en vez de solo strings de template
- [ ] Explícitamente diferido, sin fecha: registry/marketplace público, ejecución sandboxed/WASM de terceros, un SDK de plugins multi-lenguaje más allá de lo que el propio core de Sythrall ya usa — compromisos de infraestructura real que solo tienen sentido una vez que existan plugins construidos *para* Sythrall por alguien que no sea su autor, para justificarlos

La regla que sobrevive a todo lo de arriba: `apps/api` sigue funcionando con cero plugins instalados — siempre lo hizo, esta fase solo agrega una forma documentada de construir encima, nunca un requisito para hacerlo.

### 🔴 Fase 25 — Sythrall Platform *(cómo el producto llega a la gente, una vez que el CS Engine tiene algo para mostrar)*

Todo acá es ortogonal a las fases de teoría de arriba — trabajo de ingeniería/distribución que no depende de que las Fases 13–24 aterricen primero, cerrando el roadmap con cómo se usa Sythrall en vez de qué sabe:

- [ ] **Native Toolchain (Zig)** — build standalone (Zig, o PyInstaller/Nuitka + Tauri), un binario portable sin depender de Docker/Node/Python; cross-compilación para los binarios nativos que este proyecto ya shippea (`terminal-server`, `complexity-engine`), un solo toolchain en vez de matrices de CI por plataforma *(deliberadamente no compite con el rol de Rust en la Fase 18 — el trabajo de Zig es llevar a Sythrall hacia una máquina, no analizar lo que hay en ella)*
- [ ] **Integración Cython & WASM** — detección automática de candidatos Cython desde el análisis Big-O (funciones O(n²)+), generación de stubs `.pyx` desde firmas Python, compilación en Docker (MSVC/GCC), benchmark lado a lado Python-vs-Cython, speedup estimado en el hover provider, ruta de compilación WASM vía Emscripten
- [ ] **Execution Path Simulator** — vista animada tipo circuito del propio pipeline de análisis de Sythrall (`Input → Parser → AST → Dependency Resolver → Metrics → Report`), traza paso a paso con timing por etapa, exportable como SVG animado
- [ ] **Persistencia empresarial** — PostgreSQL + Delta Lake, historial de análisis, comparación de métricas entre versiones, autenticación JWT, API pública con rate limiting
- [ ] Extensión para VS Code, servidor LSP (el cliente natural para los hechos de las Fases 13–19 una vez que haya un protocolo estándar sirviéndolos), análisis de Jupyter Notebooks (`.ipynb`), integración ApexVision (`/analyze/image` con OpenCV + YOLOv11), dashboard de equipo con métricas agregadas
- [x] GitHub Action para CI/CD — `.github/workflows/ci.yml` (typecheck/lint/build/test en cada push/PR) + `release.yml` (tag → GitHub Release con notas del CHANGELOG + artefacto del build frontend)

### 🔬 Spikes de investigación — ideas sueltas, no comprometidas

- [x] ~~Extensión en Rust (PyO3) para el hot-path del parser estático~~ — **investigado dos veces con benchmarks reales, no adoptado ninguna de las dos**: la primera pasada perfiló `static_parser.py` con archivos *individuales* grandes (250+ funciones, 3000+ sintéticas) y encontró que la consolidación de recorridos AST ya hecha fue neutra (dentro del ruido de medición), con el parser ya rápido donde importa (~160ms para tamaños realistas). La segunda pasada (Fase 10, arriba) probó el otro eje — miles de *archivos* en un proyecto, no un archivo gigante — y encontró que el parser en sí seguía escalando lineal; el costo O(n²) real eran tres bugs de Python comunes en el código *alrededor* del parser, arreglados sin ningún lenguaje nuevo. Rust *sí* terminó entrando al proyecto (`terminal-server` de la Fase 9), pero para un problema genuinamente pensado para Rust — manejo de PTY cross-platform — no como reescritura de Python que ya funcionaba bien. Si algún día aparece un cuello de botella real en `parse_file` mismo, el modelo de integración sería un sidecar Axum (mismo patrón que `terminal-server`) hablando HTTP con FastAPI, no embeber vía PyO3 — más simple de mantener en solitario, sin matriz de builds de bindings nativos.

---

## 📝 Changelog

Ver [CHANGELOG.md](CHANGELOG.md) para el historial completo de versiones.

---

## 👤 Autor

**Brashkie** · [Hepein Oficial](https://github.com/Brashkie) · Lima, Perú

---

## 📄 Licencia

GPL-3.0 — ver [LICENSE](LICENSE)
