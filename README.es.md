# 🛰 CodeWatch PRO

> **Plataforma de inteligencia de código empresarial** — análisis estático, inspección ML/DL, inteligencia de editor en tiempo real, visualización de grafos de código y monitoreo de APIs. Construida con TypeScript (sin frameworks) + FastAPI + Monaco Editor.

<div align="center">

![Version](https://img.shields.io/badge/versión-4.2.0-blue?style=flat-square)
![Frontend](https://img.shields.io/badge/Frontend-Vite%20%2B%20TypeScript-646cff?style=flat-square)
![Backend](https://img.shields.io/badge/Backend-FastAPI%20%2B%20Python-009688?style=flat-square)
![Deploy](https://img.shields.io/badge/Deploy-Docker%20%2B%20Nginx-2496ed?style=flat-square)
![Tests](https://img.shields.io/badge/Tests-316%20pasando-00f5a0?style=flat-square)
![Author](https://img.shields.io/badge/Autor-Hepein%20Oficial-b87dff?style=flat-square)
![License](https://img.shields.io/badge/Licencia-Apache%202.0-orange?style=flat-square)

[English](./README.md) · [Español](./README.es.md)

</div>

---

## ✨ ¿Qué es CodeWatch PRO?

CodeWatch PRO es una plataforma profesional de inteligencia de código construida como proyecto educativo y de demostración empresarial. Combina un frontend moderno en TypeScript puro (sin frameworks en runtime), un backend Python/FastAPI con soporte completo de ML/DL, un motor de análisis estático multi-lenguaje, y una integración con Monaco Editor que rivaliza con la experiencia de desarrollo de VS Code.

### Capacidades principales

| Módulo | Descripción |
|---|---|
| **📂 Project Explorer** | Árbol de archivos expandible, pestañas multi-archivo, búsqueda global (`Ctrl+Shift+F`), outline de símbolos |
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
| **🖥 Logs** | Stream de logs del servidor en tiempo real |

---

## 📁 Estructura del proyecto

```
codewatch-pro/
├── backend/
│   ├── main.py                    ← FastAPI v4.2 (32+ rutas)
│   ├── shared.py
│   ├── requirements.txt
│   ├── Dockerfile
│   ├── pytest.ini
│   ├── routers/
│   │   ├── upload.py              ← POST /api/upload/{files,folder,zip} + CRUD
│   │   ├── analysis.py            ← POST /analyze/{code,api,logs-analyze}
│   │   ├── ml.py                  ← POST /analyze/ml
│   │   ├── diagram.py             ← POST /analyze/diagram
│   │   ├── logs.py                ← GET /logs + GET /api/history
│   │   ├── static_analysis.py     ← POST /static/{parse,parse-project,bigO,wasm}
│   │   ├── intelligence.py        ← POST /intel/{lint,analyze,hover,definition,references,completions,rename}
│   │   └── graph.py               ← GET /analyze/graph/types, POST /analyze/graph{,/project}
│   ├── services/
│   │   ├── project_service.py
│   │   └── static_parser.py       ← Parser multi-lenguaje: Python/C/C++/JS/TS
│   └── tests/
│       ├── test_upload.py         ← 26 tests
│       ├── test_analysis.py       ← 57 tests
│       ├── test_static_analysis.py← 53 tests
│       ├── test_intelligence.py   ← 109 tests (Fases 1–3)
│       ├── test_graph.py          ← 46 tests (Fase 1)
│       └── test_graph_phase2.py   ← 25 tests (Fase 2 — proyectos)
├── frontend/
│   ├── src/
│   │   ├── api/client.ts          ← API client completo v4.2
│   │   ├── components/
│   │   │   ├── app.ts             ← Shell de la app + manejo de archivos
│   │   │   ├── editor.ts          ← Integración Monaco Editor
│   │   │   ├── editor-intelligence.ts ← Linting + hover + autocompletado (Fases 1–3)
│   │   │   ├── explorer.ts        ← Project Explorer (árbol + tabs + búsqueda + outline)
│   │   │   ├── events.ts          ← Conexión global de eventos
│   │   │   ├── charts.ts          ← Integración Chart.js
│   │   │   ├── mermaid.ts         ← Mermaid + motor zoom/pan
│   │   │   └── flow.ts            ← Diagrama de flujo de ejecución
│   │   ├── panels/
│   │   │   ├── analysis.ts
│   │   │   ├── apis.ts
│   │   │   ├── ml.ts
│   │   │   ├── upload.ts
│   │   │   ├── static.ts          ← Panel de Análisis Estático
│   │   │   └── graph.ts           ← Code Graph Visual (Force Graph + Dir Tree)
│   │   ├── store/state.ts
│   │   ├── styles/
│   │   │   ├── main.css
│   │   │   ├── upload.css
│   │   │   ├── static-addon.css
│   │   │   └── explorer.css
│   │   ├── types/index.ts
│   │   └── utils/
│   ├── index.html
│   ├── package.json
│   ├── vite.config.ts
│   └── Dockerfile.frontend
├── docker-compose.yml
├── START.bat
├── STOP.bat
└── README.md
```

---

## ⚡ Requisitos previos

| Herramienta | Versión mínima | Descarga |
|---|---|---|
| **Docker Desktop** | 4.x | [docker.com](https://www.docker.com/products/docker-desktop) |
| **Node.js** | 20.x | [nodejs.org](https://nodejs.org) |
| **Python** | 3.11+ | [python.org](https://www.python.org) |
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

**Backend:**
```bash
cd backend
pip install -r requirements.txt
uvicorn main:app --host 0.0.0.0 --port 8000 --reload
```

**Frontend:**
```bash
cd frontend
npm install
npm run dev
# App en http://localhost:5173
```

---

## 🌐 URLs del sistema

| Servicio | URL |
|---|---|
| **App (Docker)** | http://localhost:8080 |
| **App (dev)** | http://localhost:5173 |
| **Backend API** | http://localhost:8000 |
| **Swagger UI** | http://localhost:8000/docs |
| **Health** | http://localhost:8000/health |

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

---

## 🧪 Tests

```bash
cd backend && pytest tests/ -v
```

```
test_upload.py            26 ✅
test_analysis.py          57 ✅
test_static_analysis.py   53 ✅
test_intelligence.py     109 ✅
test_graph.py             46 ✅
test_graph_phase2.py      25 ✅
─────────────────────────────
Total: 316 pasando
```

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

### 🔜 v4.3 — Panel de Problemas + Métricas en Vivo
- [ ] **Panel de problemas** (estilo VSCode): errores · warnings · Big-O · complejidad · hallazgos de seguridad
- [ ] **Barra de métricas en vivo** en el editor: LOC · funciones · imports · complexity score · Big-O peor caso · parse time (ms)
- [ ] Auto-recovery si el parser falla (safe mode + fallback regex)
- [ ] Detección de archivos corruptos, restauración de sesión

### 🔜 v4.4 — Expansión Multi-lenguaje
- [ ] **C/C++** soporte completo (tree-sitter ya integrado — pipeline completo)
- [ ] **Java** — AST + análisis de complejidad
- [ ] **Go** — imports, detección de goroutines
- [ ] **Rust** — patrones de ownership, bloques unsafe
- [ ] **PHP** — detección de funciones deprecated
- [ ] **SQL** — estimación de complejidad de queries
- [ ] Reglas de lint específicas por extensión

### 🔜 v4.5 — Integración Cython & WASM
- [ ] Detección automática de candidatos Cython desde el análisis Big-O (funciones O(n²)+)
- [ ] Generación de stubs `.pyx` desde las firmas de funciones Python
- [ ] Compilación Cython en Docker (MSVC en Windows / GCC en Linux)
- [ ] Benchmark lado a lado: Python vs Cython
- [ ] Speedup estimado mostrado en el hover provider
- [ ] Ruta de compilación WASM vía Emscripten

### 🔜 v4.6 — Execution Path Simulator
- [ ] Diagrama de flujo de ejecución animado tipo circuito:
  `Input → Parser → AST → Dependency Resolver → Metrics → Report`
- [ ] Traza paso a paso con timing por etapa
- [ ] Exportable como SVG animado

### 🔜 v5.0 — Persistencia Empresarial
- [ ] PostgreSQL + Delta Lake
- [ ] Historial de análisis, comparación de métricas entre versiones
- [ ] WebSockets para análisis en tiempo real
- [ ] Autenticación JWT, API pública con rate limiting
- [ ] GitHub Action para CI/CD

### 💡 Ideas futuras
- [ ] Extensión para VS Code
- [ ] Análisis de Jupyter Notebooks (`.ipynb`)
- [ ] Integración ApexVision (`/analyze/image` con OpenCV + YOLOv11)
- [ ] Dashboard de equipo con métricas agregadas

---

## 👤 Autor

**Brashkie** · [Hepein Oficial](https://github.com/Brashkie) · Lima, Perú

---

## 📄 Licencia

Apache License 2.0 — ver [LICENSE](LICENSE)