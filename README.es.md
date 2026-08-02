# 🛰 CodeWatch PRO

> **Plataforma de inteligencia de código empresarial** — análisis estático, inspección ML/DL, inteligencia de editor en tiempo real, visualización de grafos de código, terminal integrada y monitoreo de APIs. Construida con TypeScript (sin frameworks) + FastAPI + Rust + Monaco Editor.

<div align="center">

![Version](https://img.shields.io/badge/versión-4.5.0-blue?style=flat-square)
[![CI](https://github.com/Brashkie/codewatch-pro/actions/workflows/ci.yml/badge.svg)](https://github.com/Brashkie/codewatch-pro/actions/workflows/ci.yml)
![Frontend](https://img.shields.io/badge/Frontend-Vite%20%2B%20TypeScript-646cff?style=flat-square)
![Backend](https://img.shields.io/badge/Backend-FastAPI%20%2B%20Python-009688?style=flat-square)
![Terminal](https://img.shields.io/badge/Terminal-Rust%20%2B%20axum-dea584?style=flat-square)
![Deploy](https://img.shields.io/badge/Deploy-Docker%20%2B%20Nginx-2496ed?style=flat-square)
![Tests](https://img.shields.io/badge/Tests-297%20pasando-00f5a0?style=flat-square)
![Author](https://img.shields.io/badge/Autor-Hepein%20Oficial-b87dff?style=flat-square)
![License](https://img.shields.io/badge/Licencia-GPL%203.0-orange?style=flat-square)

[English](./README.md) · [Español](./README.es.md)

</div>

---

## ✨ ¿Qué es CodeWatch PRO?

CodeWatch PRO es una plataforma profesional de inteligencia de código construida como proyecto educativo y de demostración empresarial. Combina un frontend moderno en TypeScript puro (sin frameworks en runtime), un backend Python/FastAPI con soporte completo de ML/DL, un motor de análisis estático multi-lenguaje, y una integración con Monaco Editor que rivaliza con la experiencia de desarrollo de VS Code.

### Capacidades principales

| Módulo | Descripción |
|---|---|
| **📂 Project Explorer** | Árbol de archivos expandible, pestañas multi-archivo, búsqueda global (`Ctrl+Shift+F`), outline de símbolos |
| **📁 Explorador de carpetas** | Abrí una carpeta real del disco ("+ Carpeta") y navegala como árbol expandible estilo VSCode — funciona en cualquier navegador moderno, sin APIs exclusivas de Chromium |
| **🖥 Terminal integrada** | Shell interactiva real (PowerShell/bash) en un panel inferior redimensionable, con un sidecar en Rust (`portable-pty` + `axum`) — protegida por token, sin fricción para uso local |
| **📝 Editor Intelligence** | Linting en tiempo real, diagnósticos inline, hover con Big-O, Go to Definition, Find References, Rename Symbol, autocompletado semántico |
| **🔬 Análisis Estático** | Parser AST multi-lenguaje (Python, TypeScript, C/C++) — estimación Big-O, complejidad ciclomática, hints WASM/Cython, call graph, dead code |
| **🕸 Code Graph Visual** | Import Graph, Call Graph, detección de dependencias circulares, Complexity Heatmap — Tree View + Force Graph interactivo |
| **📂 Proyectos** | Subida de archivos, carpetas y ZIPs con árbol interactivo y visor Monaco |
| **🔍 Análisis** | pylint · flake8 · radon · AST — issues, complejidad, maintainability index |
| **🤖 ML/DL** | Detección de 23 librerías, 23 patrones de pipeline, 25 modelos, 20+ reglas |
| **🔀 Diagramas** | Flowchart · Callgraph · Clases · Secuencia — Mermaid.js con zoom/pan |
| **📡 APIs** | Verificación de endpoints externos con historial y métricas |
| **📊 Dashboard** | Charts de distribución, tiempos de respuesta, historial de ejecuciones |
| **🔁 Diff** | Comparación visual de archivos con resaltado de cambios |
| **🖥 Logs** | Stream de logs del servidor en tiempo real — también disponible como vista alternable dentro del panel de terminal |
| **🎨 Tema claro/oscuro** | Toggle en el topbar, oscuro por defecto, persiste entre sesiones |

---

## 📁 Estructura del proyecto

Todos los manifiestos/configs viven en la raíz del repo; `backend/` y `frontend/` solo contienen código fuente. `scripts/` es el único punto de entrada para todo (ver [scripts/README.md](scripts/README.md)).

```
codewatch-pro/
├── package.json                   ← Manifiesto npm (deps del frontend + scripts lint/format/build)
├── package-lock.json
├── vite.config.ts                 ← root: 'frontend', build.outDir: '../dist'
├── tsconfig.json                  ← include: frontend/src
├── biome.json                     ← Config de Biome (lint/format)
├── requirements.txt                ← Deps runtime del backend
├── requirements-dev.txt            ← Ruff (lint/format), solo dev
├── pyproject.toml                  ← Config de Ruff
├── pytest.ini                      ← testpaths: backend/tests
├── Cargo.toml                       ← Manifiesto Rust — sidecar terminal-server
├── Cargo.lock
├── docker-compose.yml
├── .dockerignore
├── START.bat / STOP.bat
├── scripts/                       ← Flujo de dev sin Docker (setup/dev/build/test/lint/format, .ps1 + .sh)
│   ├── run-backend.mjs            ← levanta uvicorn (npm run dev:api)
│   ├── run-terminal.mjs           ← levanta el sidecar de terminal en Rust (npm run dev:term)
│   └── dev-banner.mjs             ← banner de arranque con ansimax para npm run dev
├── terminal-server/                ← Sidecar en Rust: shell interactiva real sobre WebSocket
│   ├── Dockerfile
│   └── src/
│       ├── main.rs                ← handler WS de axum, auth por token, bridging del PTY
│       ├── pty_session.rs         ← wrapper de portable-pty (ConPTY/PTY Unix, una sola implementación)
│       └── auth.rs                ← generación de token + comparación en tiempo constante
├── backend/
│   ├── main.py                    ← FastAPI v4.5 (30+ rutas)
│   ├── shared.py
│   ├── Dockerfile
│   ├── routers/
│   │   ├── upload.py              ← POST /api/upload/{files,folder,zip} + CRUD
│   │   ├── analysis.py            ← POST /analyze/{code,api,logs-analyze}
│   │   ├── ml.py                  ← POST /analyze/ml
│   │   ├── diagram.py             ← POST /analyze/diagram
│   │   ├── logs.py                ← GET /logs + GET /api/history
│   │   ├── static_analysis.py     ← POST /static/{parse,parse-project,bigO,wasm}
│   │   ├── intelligence.py        ← POST /intel/{lint,analyze,hover,definition,references,completions,rename}
│   │   ├── graph.py               ← GET /analyze/graph/types, POST /analyze/graph{,/project}
│   │   └── metrics_live.py        ← POST /metrics/live — métricas instantáneas por tecla
│   ├── services/
│   │   ├── project_service.py
│   │   └── static_parser.py       ← Parser multi-lenguaje: Python/C/C++/JS/TS
│   └── tests/                      ← 297 tests en total (ver sección Tests más abajo)
├── frontend/
│   ├── index.html
│   ├── Dockerfile.frontend
│   └── src/
│       ├── api/client.ts          ← API client completo
│       ├── components/
│       │   ├── app.ts             ← Shell de la app + manejo de archivos
│       │   ├── editor.ts          ← Integración Monaco Editor
│       │   ├── editor-intelligence.ts ← Linting + hover + autocompletado (Fases 1–3)
│       │   ├── explorer.ts        ← Project Explorer (árbol + tabs + búsqueda + outline)
│       │   ├── file-browser.ts    ← Árbol de carpetas desde <input webkitdirectory>, cross-browser
│       │   ├── terminal.ts        ← Cliente xterm.js para el sidecar de terminal en Rust
│       │   ├── events.ts          ← Conexión global de eventos
│       │   ├── charts.ts          ← Integración Chart.js
│       │   ├── mermaid.ts         ← Mermaid + motor zoom/pan
│       │   └── flow.ts            ← Diagrama de flujo de ejecución
│       ├── panels/
│       │   ├── analysis.ts
│       │   ├── apis.ts
│       │   ├── ml.ts
│       │   ├── upload.ts
│       │   ├── static.ts          ← Panel de Análisis Estático
│       │   └── graph.ts           ← Code Graph Visual (Force Graph + Dir Tree)
│       ├── store/state.ts
│       ├── styles/
│       │   ├── main.css
│       │   ├── upload.css
│       │   ├── static-addon.css
│       │   └── explorer.css
│       ├── types/index.ts
│       └── utils/
│           ├── file-tree.ts       ← FileList → árbol anidado (para file-browser.ts)
│           └── theme.ts           ← Toggle de tema claro/oscuro + persistencia
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
git clone https://github.com/Brashkie/codewatch-pro.git
cd codewatch-pro
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

**Backend** (desde la raíz — ahí vive `requirements.txt`, el código está en `backend/`):
```bash
pip install -r requirements.txt
cd backend
uvicorn main:app --host 0.0.0.0 --port 8000 --reload
```

**Frontend** (desde la raíz — ahí viven `package.json`/`vite.config.ts`, el código fuente está en `frontend/src`):
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
POST /intel/analyze      → Heavy analyze ~80ms (pylint + radon + Big-O)
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
  "tools":    ["ast", "flake8", "pylint", "radon"]
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
| pylint / flake8 / radon | latest | Calidad de código |
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

## 🦀 Stack Rust — `terminal-server`

Primer uso de Rust en el proyecto: un sidecar chico para la terminal integrada, no un reemplazo de código Python existente.

| Crate | Uso |
|---|---|
| axum | Servidor WebSocket + ruteo HTTP |
| tokio | Runtime async |
| portable-pty | ConPTY (Windows) / PTY (Unix) — una sola implementación para ambos |
| subtle | Comparación de token en tiempo constante |
| rand | CSPRNG para el token de cada corrida |

---

## 🧪 Tests

```bash
cd backend && pytest
# pytest.ini vive en la raíz y se autodetecta; testpaths = backend/tests
```

```
test_upload.py            26 ✅
test_analysis.py          57 ✅
test_intelligence.py     109 ✅
test_graph.py             46 ✅
test_graph_phase2.py      25 ✅
test_metrics_live.py      34 ✅
─────────────────────────────
Total: 297 pasando
```

El sidecar en Rust (`terminal-server`) tiene sus propios checks — `cargo build --release`, `cargo clippy -- -D warnings`, `cargo test` — corridos vía el job `terminal` en [`ci.yml`](.github/workflows/ci.yml), aparte del suite de Python de arriba.

---

## 🧹 Herramientas (lint & format)

[Biome](https://biomejs.dev) formatea y lintea el frontend TypeScript; [Ruff](https://docs.astral.sh/ruff/) hace lo mismo para el backend Python. Ambos corren vía `scripts/`:

```bash
./scripts/lint.sh      # o .\scripts\lint.ps1   — solo revisa, no escribe
./scripts/format.sh    # o .\scripts\format.ps1 — aplica los cambios
```

Comandos directos equivalentes (desde la raíz): `npm run lint` / `npm run format`, `ruff check backend` / `ruff format backend`.

---

## 🔄 Comandos Docker útiles

```bash
docker compose logs -f                                   # Logs en vivo
docker compose build --no-cache && docker compose up -d  # Rebuild completo
docker compose ps                                        # Contenedores activos
docker exec -it codewatch-backend bash                   # Shell del backend
docker compose down                                      # Detener (conserva volúmenes)
docker compose down -v                                   # Detener + borrar volúmenes
```

---

## ⚙️ Configuración

`backend/.env`:
```env
PYTHONUNBUFFERED=1
```

`frontend/.env`:
```env
VITE_SILENCE_SOURCEMAP_WARNINGS=true
```

Variables de entorno del sidecar de terminal (opcionales — por defecto `127.0.0.1:7681`, seguro para uso local):
```env
TERMINAL_HOST=127.0.0.1
TERMINAL_PORT=7681
```

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

### ✅ v1.0 — Base
- [x] Backend Flask + pylint, flake8, radon
- [x] Frontend TypeScript + Vite sin frameworks
- [x] Monaco Editor + Chart.js + Docker

### ✅ v2.0 — Inspector ML/DL
- [x] 23 librerías detectadas, 23 patrones de pipeline, 25 modelos
- [x] 20+ reglas de issues (data leakage, reproducibilidad, frameworks)
- [x] Score 0–100 + diagrama Mermaid del pipeline

### ✅ v3.0 — Zoom/Pan + Responsive
- [x] Zoom y pan en diagramas, layout responsive completo, bottom navigation móvil

### ✅ v4.0 — FastAPI + Upload de proyectos
- [x] Migración Flask → FastAPI, upload de archivos/carpetas/ZIPs, 83 tests

### ✅ v4.1 — Análisis Estático + Editor Intelligence
- [x] Parser AST multi-lenguaje (Python · TypeScript · C/C++) sin IA
- [x] Estimación Big-O por función, complejidad ciclomática, hints Cython/WASM
- [x] Linting en tiempo real (fast ~1ms, heavy ~80ms), diagnósticos inline
- [x] Hover: firma + Big-O + CC + docstring
- [x] Go to Definition · Find References · Autocompletado semántico · Rename Symbol
- [x] 162 tests automatizados

### ✅ v4.2 — Code Graph + Project Explorer
- [x] Import Graph · Call Graph · Dependencias Circulares · Complexity Heatmap
- [x] Fase 1: grafos desde sidebar; Fase 2: desde proyectos ZIP completos
- [x] Resolución cross-folder, detección circular con NetworkX
- [x] Force Graph interactivo (motor de física propio, sin D3)
- [x] Project Explorer: árbol + tabs múltiples + búsqueda global + outline
- [x] 316 tests automatizados

### ✅ v4.3 — Panel de Problemas + Métricas en Vivo
- [x] **Panel de problemas** (estilo VSCode): errores · warnings · Big-O · complejidad · hallazgos de seguridad
- [x] **Barra de métricas en vivo** en el editor: LOC · funciones · imports · complexity score · Big-O peor caso · parse time (ms)
- [x] Auto-recovery si el parser falla (safe mode + fallback regex)
- [x] Detección de archivos corruptos, restauración de sesión

### 🔧 v4.4 — Computer Science Engine *(shippeado parcial — extensión directa del motor de análisis existente, sin arquitectura nueva)*

No solo "qué" hace el código — *por qué* se comporta así. Construido enteramente sobre datos que `static_parser.py` y el motor de Big-O ya calculan:

- [x] Complejidad completa por función: Θ (cota ajustada), Ω (mejor caso), O (peor caso) — no solo el O del peor caso *(solo Python por ahora; C/C++/JS/TS todavía muestran solo O)*
- [x] Explicación del "por qué" en cada resultado de Big-O (ej. *"2 loops anidados — el loop interno corre n veces por cada iteración del externo"*)
- [x] Recursión detectada → detección de tail-call + marco de "Cálculo Lambda" *(estimación de profundidad omitida — depende del input en runtime, no se puede calcular de forma estática confiable)*
- [ ] Regex detectada → clasificar como Autómata Finito / Chomsky Tipo-3 (Regular)
- [ ] Código con forma de gramática/parser detectado → Gramática Libre de Contexto / Autómata con Pila / Chomsky Tipo-2
- [ ] Recorrido de grafo detectado → etiquetar como DFS/BFS/orden topológico, O(V+E)

### ✅ v4.5 — Terminal Integrada + Explorador de Carpetas + Tema

No estaba originalmente en este roadmap — salió directo de feedback del usuario a mitad de desarrollo, se sumó porque cada pieza era chica y estaba bien acotada por separado:

- [x] **Terminal integrada**, shell interactiva real sobre WebSocket — primer uso de **Rust** en el proyecto (sidecar `terminal-server`: `portable-pty` + `axum`), protegida por token, auto-conexión sin fricción para uso local, selector de panel entre la shell y una vista de Logs en vivo
- [x] **Explorador de carpetas** en el sidebar ("+ Carpeta") — árbol expandible estilo VSCode desde una carpeta real del disco, cross-browser vía `webkitdirectory` (deliberadamente *no* la File System Access API, que es solo Chromium)
- [x] Toggle de **tema claro/oscuro**, persistido, oscuro por defecto
- [x] [`ansimax`](https://github.com/Brashkie/ansimax) (librería propia) para el banner de arranque de `npm run dev`

---

Los ítems de abajo están agrupados según qué tan fundamentados están, no por número de versión — la idea es ser honestos sobre el alcance antes de comprometernos a él.

### 🔜 v4.6 — Data Structure Detector *(mismo estilo heurístico que los detectores de WASM-hints/dead-code ya existentes)*

- [ ] Detectar AVL / Red-Black Tree / Trie / Heap / Segment Tree / Fenwick Tree / Bloom Filter / B-Tree / HashMap / Skip List a partir de la forma del AST
- [ ] Por cada detección: complejidad, uso típico, ventajas/desventajas

### 🔜 v4.7 — Expansión Multi-lenguaje
- [ ] **C/C++** soporte completo (tree-sitter ya integrado — pipeline completo)
- [ ] **Java** — AST + análisis de complejidad
- [ ] **Go** — imports, detección de goroutines
- [ ] **Rust** — patrones de ownership, bloques unsafe *(Rust como lenguaje-objetivo del analizador — distinto del sidecar `terminal-server` shippeado en v4.5, que es tooling, no un lenguaje que el analizador parsea)*
- [ ] **PHP** — detección de funciones deprecated
- [ ] **SQL** — estimación de complejidad de queries
- [ ] Reglas de lint específicas por extensión

### 🔜 v4.8 — Integración Cython & WASM
- [ ] Detección automática de candidatos Cython desde el análisis Big-O (funciones O(n²)+)
- [ ] Generación de stubs `.pyx` desde las firmas de funciones Python
- [ ] Compilación Cython en Docker (MSVC en Windows / GCC en Linux)
- [ ] Benchmark lado a lado: Python vs Cython
- [ ] Speedup estimado mostrado en el hover provider
- [ ] Ruta de compilación WASM vía Emscripten

### 🔜 v4.9 — Execution Path Simulator
- [ ] Diagrama de flujo de ejecución animado tipo circuito:
  `Input → Parser → AST → Dependency Resolver → Metrics → Report`
- [ ] Traza paso a paso con timing por etapa
- [ ] Exportable como SVG animado

### 🔬 Spikes de investigación — integrar herramientas existentes, no reconstruirlas

Ideas reales, pero cada una es su propio proyecto serio ya resuelto bien por herramientas open-source dedicadas — lo honesto es enlazar/integrar esas, no reinventarlas:

- [ ] Visualización del pipeline del compilador (Lexer → AST → IR → Assembly) — integrar [Compiler Explorer](https://godbolt.org) (open source) en vez de construir un compilador educativo desde cero
- [ ] Analizador de ejecutables (PE / ELF / Mach-O, secciones, imports/exports, símbolos) — envolver [Capstone](https://www.capstone-engine.org)/[LIEF](https://lief-project.github.io)/`objdump`, no escribir un disassembler a mano
- [ ] Centralidad de grafos / detección de "hubs" (NetworkX ya es dependencia) — mostrar los archivos/funciones más conectados del Code Graph existente, la misma idea que usan las redes sociales para detectar influencers
- [ ] Build standalone (PyInstaller/Nuitka + Tauri, o Zig para binarios estáticos chicos) — un binario portable sin depender de Docker/Node/Python, como alternativa a `scripts/` y Docker
- [x] ~~Extensión en Rust (PyO3) para el hot-path del parser estático~~ — **investigado con benchmarks reales, no adoptado para esto**: perfilar `static_parser.py` con archivos grandes reales (250+ funciones, 3000+ sintéticas) mostró que la consolidación de recorridos AST ya hecha fue neutra (dentro del ruido de medición), y el parser ya es suficientemente rápido donde importa (~160ms para tamaños de archivo realistas). Rust *sí* terminó entrando al proyecto (`terminal-server` de v4.5), pero para un problema genuinamente pensado para Rust — manejo de PTY cross-platform — no como reescritura de Python que ya funcionaba bien.

### 🧭 Largo plazo / otra categoría de herramienta — no comprometido

Estos necesitan instrumentación en runtime (ptrace/eBPF), un proceso corriendo, o captura de paquetes en vivo — arquitectónicamente son un tipo de herramienta distinto al análisis estático, así que quedan como ideas, no como compromisos de roadmap:

- [ ] Visualizador de memoria (stack/heap/data/bss) — requiere un proceso corriendo para inspeccionar, no texto fuente
- [ ] Analizador de concurrencia (race conditions, deadlocks, mal uso de mutex/atomic) — necesita ejecución real o herramientas como ThreadSanitizer, no inspección de AST
- [ ] Motor de SO (threads, paging, scheduling, IPC) — necesita tracing a nivel de kernel
- [ ] Analizador de redes (TCP/TLS/QUIC/WebSocket) — necesita captura de paquetes; esto es una herramienta con forma de Wireshark, no un analizador estático
- [ ] Analizador de seguridad más allá de detección de patrones (ROP, heap spray, use-after-free) — compite directamente con herramientas SAST maduras (Semgrep, CodeQL, Bandit); la versión realista se integra al CS Engine de arriba como "detectar el patrón + explicar el CWE", no un motor de análisis de exploits completo

### 🔜 v5.0 — Persistencia Empresarial
- [ ] PostgreSQL + Delta Lake
- [ ] Historial de análisis, comparación de métricas entre versiones
- [ ] WebSockets para streaming de análisis en tiempo real *(el WebSocket de la terminal, shippeado en v4.5, es otra cosa — una shell PTY interactiva, no un canal de streaming de análisis)*
- [ ] Autenticación JWT, API pública con rate limiting
- [x] GitHub Action para CI/CD — `.github/workflows/ci.yml` (typecheck/lint/build/test en cada push/PR) + `release.yml` (tag → GitHub Release con notas del CHANGELOG + artefacto del build frontend)

### 💡 Ideas futuras
- [ ] Extensión para VS Code
- [ ] Análisis de Jupyter Notebooks (`.ipynb`)
- [ ] Integración ApexVision (`/analyze/image` con OpenCV + YOLOv11)
- [ ] Dashboard de equipo con métricas agregadas

---

## 📝 Changelog

Ver [CHANGELOG.md](CHANGELOG.md) para el historial completo de versiones.

---

## 👤 Autor

**Brashkie** · [Hepein Oficial](https://github.com/Brashkie) · Lima, Perú

---

## 📄 Licencia

GPL-3.0 — ver [LICENSE](LICENSE)
