# 🛰 CodeWatch PRO

> **Enterprise-grade code intelligence platform** — static analysis, ML/DL inspection, real-time editor intelligence, code graph visualization, and API monitoring. Built with TypeScript (no frameworks) + FastAPI + Monaco Editor.

<div align="center">

![Version](https://img.shields.io/badge/version-4.4.0-blue?style=flat-square)
[![CI](https://github.com/Brashkie/codewatch-pro/actions/workflows/ci.yml/badge.svg)](https://github.com/Brashkie/codewatch-pro/actions/workflows/ci.yml)
![Frontend](https://img.shields.io/badge/Frontend-Vite%20%2B%20TypeScript-646cff?style=flat-square)
![Backend](https://img.shields.io/badge/Backend-FastAPI%20%2B%20Python-009688?style=flat-square)
![Deploy](https://img.shields.io/badge/Deploy-Docker%20%2B%20Nginx-2496ed?style=flat-square)
![Tests](https://img.shields.io/badge/Tests-316%20passing-00f5a0?style=flat-square)
![Author](https://img.shields.io/badge/Author-Hepein%20Oficial-b87dff?style=flat-square)
![License](https://img.shields.io/badge/License-GPL%203.0-orange?style=flat-square)

[English](./README.md) · [Español](./README.es.md)

</div>

---

## ✨ What is CodeWatch PRO?

CodeWatch PRO is a professional code intelligence platform built as both an educational project and a production-grade demonstration. It combines a modern TypeScript frontend (zero runtime frameworks), a Python/FastAPI backend with full ML/DL support, a multi-language static analysis engine, and a Monaco Editor integration that rivals VS Code's developer experience.

### Core capabilities

| Module | Description |
|---|---|
| **📂 Project Explorer** | Expandable file tree, multi-file tabs, global search (`Ctrl+Shift+F`), symbol outline |
| **📝 Editor Intelligence** | Real-time linting, inline diagnostics, Big-O hover, Go to Definition, Find References, Rename Symbol, semantic autocomplete |
| **🔬 Static Analysis** | AST-based multi-language parser (Python, TypeScript, C/C++) — Big-O estimation, cyclomatic complexity, WASM/Cython hints, call graph, dead code |
| **🕸 Code Graph Visual** | Import graph, Call graph, Circular dependency detection, Complexity heatmap — Tree View + interactive Force Graph |
| **📂 Projects** | Upload files, folders and ZIPs with interactive tree and Monaco-powered file viewer |
| **🔍 Analysis** | pylint · flake8 · radon · AST — issues, complexity, maintainability index |
| **🤖 ML/DL** | Detection of 23 libraries, 23 pipeline patterns, 25 models, 20+ issue rules |
| **🔀 Diagrams** | Flowchart · Callgraph · Classes · Sequence — generated with Mermaid.js + zoom/pan |
| **📡 APIs** | External endpoint verification with history and response metrics |
| **📊 Dashboard** | Distribution charts, response times, execution history |
| **🔁 Diff** | Visual file comparison with highlighted changes |
| **🖥 Logs** | Real-time server log stream |

---

## 📁 Project structure

All manifests/configs live at the repo root; `backend/` and `frontend/` hold source code only. `scripts/` is the single entry point for everything (see [scripts/README.md](scripts/README.md)).

```
codewatch-pro/
├── package.json                   ← npm manifest (frontend deps + lint/format/build scripts)
├── package-lock.json
├── vite.config.ts                 ← root: 'frontend', build.outDir: '../dist'
├── tsconfig.json                  ← include: frontend/src
├── biome.json                     ← Biome (lint/format) config
├── requirements.txt                ← backend runtime deps
├── requirements-dev.txt            ← Ruff (lint/format), dev-only
├── pyproject.toml                  ← Ruff config
├── pytest.ini                      ← testpaths: backend/tests
├── docker-compose.yml
├── .dockerignore
├── START.bat / STOP.bat
├── scripts/                       ← Dev workflow without Docker (setup/dev/build/test/lint/format, .ps1 + .sh)
├── backend/
│   ├── main.py                    ← FastAPI v4.2 (32+ routes)
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
│   │   └── graph.py               ← GET /analyze/graph/types, POST /analyze/graph{,/project}
│   ├── services/
│   │   ├── project_service.py
│   │   └── static_parser.py       ← Multi-language parser: Python/C/C++/JS/TS
│   └── tests/
│       ├── test_upload.py         ← 26 tests
│       ├── test_analysis.py       ← 57 tests
│       ├── test_static_analysis.py← 53 tests
│       ├── test_intelligence.py   ← 109 tests (Phases 1–3)
│       ├── test_graph.py          ← 46 tests (Phase 1)
│       └── test_graph_phase2.py   ← 25 tests (Phase 2 — projects)
├── frontend/
│   ├── index.html
│   ├── Dockerfile.frontend
│   └── src/
│       ├── api/client.ts          ← Full API client v4.2
│       ├── components/
│       │   ├── app.ts             ← App shell + file management
│       │   ├── editor.ts          ← Monaco Editor integration
│       │   ├── editor-intelligence.ts ← Linting + hover + autocomplete (Phases 1–3)
│       │   ├── explorer.ts        ← Project Explorer (tree + tabs + search + outline)
│       │   ├── events.ts          ← Global event wiring
│       │   ├── charts.ts          ← Chart.js integration
│       │   ├── mermaid.ts         ← Mermaid + zoom/pan engine
│       │   └── flow.ts            ← Execution flow diagram
│       ├── panels/
│       │   ├── analysis.ts
│       │   ├── apis.ts
│       │   ├── ml.ts
│       │   ├── upload.ts
│       │   ├── static.ts          ← Static Analysis panel
│       │   └── graph.ts           ← Code Graph Visual (Force Graph + Dir Tree)
│       ├── store/state.ts
│       ├── styles/
│       │   ├── main.css
│       │   ├── upload.css
│       │   ├── static-addon.css
│       │   └── explorer.css
│       ├── types/index.ts
│       └── utils/
└── README.md
```

---

## ⚡ Prerequisites

| Tool | Min version | Download |
|---|---|---|
| **Docker Desktop** | 4.x | [docker.com](https://www.docker.com/products/docker-desktop) |
| **Node.js** | 20.x | [nodejs.org](https://nodejs.org) |
| **Python** | 3.11+ | [python.org](https://www.python.org) |
| **Git** | any | [git-scm.com](https://git-scm.com) |

---

## 🚀 Installation

### 1 — Clone

```bash
git clone https://github.com/Brashkie/codewatch-pro.git
cd codewatch-pro
```

### 2 — Start with Docker (recommended)

**Windows:**
1. Open Docker Desktop and wait until it's running
2. Double-click **`START.bat`**
3. Browser opens at `http://localhost:8080`

**Any OS:**
```bash
docker compose up --build
```

### 3 — Development mode (without Docker)

The `scripts/` folder wraps the whole no-Docker workflow in one command each — no need to juggle `cd`/`pip`/`npm` manually:

```powershell
# Windows (PowerShell)
.\scripts\setup.ps1   # installs backend venv + frontend node_modules
.\scripts\dev.ps1     # runs backend (uvicorn --reload) + frontend (vite) together
```

```bash
# macOS / Linux / Git Bash / WSL
./scripts/setup.sh
./scripts/dev.sh
```

See [scripts/README.md](scripts/README.md) for the full list (`build`, `test`, `lint`, `format`). Under the hood, `dev` is just `npm run dev` — `concurrently` runs Vite and uvicorn as one process (`[web]`/`[api]` prefixed logs), so `npm run dev` works the same directly if you'd rather skip the wrapper scripts.

<details>
<summary>Manual commands (equivalent, no script)</summary>

**Backend** (from repo root — `requirements.txt` lives there, app code is in `backend/`):
```bash
pip install -r requirements.txt
cd backend
uvicorn main:app --host 0.0.0.0 --port 8000 --reload
```

**Frontend** (from repo root — `package.json`/`vite.config.ts` live there, source is in `frontend/src`):
```bash
npm install
npm run dev
# http://localhost:5173
```

</details>

---

## 🌐 Service URLs

| Service | URL |
|---|---|
| **App (Docker)** | http://localhost:8080 |
| **App (dev)** | http://localhost:5173 |
| **Backend API** | http://localhost:8000 |
| **Swagger UI** | http://localhost:8000/docs |
| **Health** | http://localhost:8000/health |

---

## 🔌 API Reference

### System
```http
GET /health          → Server status
GET /capabilities    → All installed library versions
GET /logs            → Server logs
```

### Static Analysis (no AI, multi-language)
```http
POST /static/parse          → Full AST: Big-O, call graph, WASM hints
POST /static/parse-project  → Multi-file: dependency graph
POST /static/bigO           → Big-O table per function
POST /static/wasm           → Cython/WASM hotpath recommendations
GET  /static/languages      → Supported languages
```

### Editor Intelligence
```http
POST /intel/lint         → Fast lint ~1ms
POST /intel/analyze      → Heavy analyze ~80ms (pylint + radon + Big-O)
POST /intel/hover        → Signature + Big-O + CC + docs
POST /intel/definition   → Go to Definition
POST /intel/references   → Find References
POST /intel/completions  → Semantic autocomplete
POST /intel/rename       → Rename Symbol (WorkspaceEdits)
```

### Code Graph
```http
GET  /analyze/graph/types           → Available types
POST /analyze/graph                 → From sidebar files
POST /analyze/graph/project         → From uploaded ZIP project
```

Graph types: `import` · `call` · `circular` · `heatmap`

### Project Upload
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

## 🐍 Python stack

| Library | Version | Use |
|---|---|---|
| FastAPI | 0.115.5 | Async REST server |
| pylint / flake8 / radon | latest | Code quality |
| tree-sitter | 0.23.2 | Multi-language AST |
| networkx | 3.6.1 | Graph algorithms |
| numpy / pandas / polars | latest | Data processing |
| torch / tensorflow-cpu | latest | Deep learning |
| scikit-learn / lightgbm | latest | Classic ML |
| spacy / opencv / scipy | latest | NLP / Vision / Science |
| **Cython** | 3.2.4 | Python → C compilation |

## 🟦 TypeScript stack

| Library | Version | Use |
|---|---|---|
| Vite | 5.3.1 | Bundler |
| TypeScript | 5.4.5 | Static typing |
| monaco-editor | 0.45.0 | Code editor |
| mermaid | 11.4.0 | Diagrams |
| chart.js | 4.4.3 | Charts |

---

## 🧪 Tests

```bash
cd backend && pytest
# pytest.ini lives at the repo root and is auto-discovered; testpaths = backend/tests
```

```
test_upload.py            26 ✅
test_analysis.py          57 ✅
test_static_analysis.py   53 ✅
test_intelligence.py     109 ✅
test_graph.py             46 ✅
test_graph_phase2.py      25 ✅
─────────────────────────────
Total: 316 passed
```

---

## 🧹 Tooling (lint & format)

[Biome](https://biomejs.dev) formats/lints the TypeScript frontend; [Ruff](https://docs.astral.sh/ruff/) does the same for the Python backend. Both run through `scripts/`:

```bash
./scripts/lint.sh      # or .\scripts\lint.ps1   — check only, no writes
./scripts/format.sh    # or .\scripts\format.ps1 — writes fixes
```

Equivalent direct commands (from repo root): `npm run lint` / `npm run format`, `ruff check backend` / `ruff format backend`.

---

## 🔄 Docker commands

```bash
docker compose logs -f                         # Live logs
docker compose build --no-cache && docker compose up -d  # Full rebuild
docker compose ps                              # Active containers
docker exec -it codewatch-backend bash         # Backend shell
docker compose down                            # Stop (keep volumes)
docker compose down -v                         # Stop + delete volumes
```

---

## 🗺️ Roadmap

### ✅ v1.0 — Foundation
- [x] Flask backend + pylint, flake8, radon
- [x] TypeScript + Vite frontend (zero frameworks)
- [x] Monaco Editor + Chart.js + Docker

### ✅ v2.0 — ML/DL Inspector
- [x] 23 library detections, 23 pipeline patterns, 25 models
- [x] 20+ issue rules (data leakage, reproducibility, framework-specific)
- [x] Score 0–100 + Mermaid pipeline diagram

### ✅ v3.0 — Zoom/Pan + Responsive
- [x] Diagram zoom + pan, responsive layout, mobile bottom navigation

### ✅ v4.0 — FastAPI + Project Upload
- [x] Flask → FastAPI migration, file/folder/ZIP upload, 83 tests

### ✅ v4.1 — Static Analysis + Editor Intelligence
- [x] Multi-language AST parser (Python · TypeScript · C/C++) — no AI
- [x] Big-O estimation per function, cyclomatic complexity, WASM/Cython hints
- [x] Real-time linting (fast ~1ms, heavy ~80ms), inline diagnostics
- [x] Hover: signature + Big-O + CC + docstring
- [x] Go to Definition · Find References · Semantic autocomplete · Rename Symbol
- [x] 162 automated tests

### ✅ v4.2 — Code Graph + Project Explorer
- [x] Import Graph · Call Graph · Circular Dependencies · Complexity Heatmap
- [x] Phase 1: graphs from sidebar; Phase 2: graphs from uploaded ZIP projects
- [x] Cross-folder dependency resolution, NetworkX circular detection
- [x] Interactive Force Graph (custom physics engine, no D3)
- [x] Project Explorer: file tree + multi-file tabs + global search + outline
- [x] 316 automated tests

### ✅ v4.3 — Problems Panel + Live Metrics
- [x] **Problems panel** (VSCode-style): errors · warnings · Big-O · complexity · security findings
- [x] **Live metrics bar** in editor: LOC · functions · imports · complexity score · Big-O worst · parse time (ms)
- [x] Auto-recovery if parser fails (safe mode + regex fallback)
- [x] Corrupt file detection, session restore

---

The items below are grouped by how well-founded they are, not by version number — the goal is to be honest about scope before committing to it.

### 🔜 v4.4 — Computer Science Engine ⭐ *(top priority — direct extension of the existing analysis engine, no new architecture)*

Not just "what" the code does — *why* it behaves that way. Built entirely on data `static_parser.py` and the Big-O engine already compute:

- [x] Full complexity picture per function: Θ (tight bound), Ω (best case), O (worst case) — not just worst-case O *(Python only for now; C/C++/JS/TS still show O only)*
- [x] "Why" explanation attached to every Big-O result (e.g. *"2 nested loops — inner loop runs n times per outer iteration"*)
- [ ] Regex detected → classify as Finite Automaton / Chomsky Type-3 (Regular)
- [ ] Grammar/parser-shaped code detected → Context-Free Grammar / Pushdown Automaton / Chomsky Type-2
- [ ] Recursion detected → tail-call detection, recursion-depth estimate, "Lambda Calculus" framing
- [ ] Graph traversal detected → label as DFS/BFS/topological sort, O(V+E)

### 🔜 v4.5 — Data Structure Detector *(same heuristic-pattern style as the existing WASM-hint/dead-code detectors)*

- [ ] Detect AVL / Red-Black Tree / Trie / Heap / Segment Tree / Fenwick Tree / Bloom Filter / B-Tree / HashMap / Skip List from AST shape
- [ ] For each match: complexity, typical use case, tradeoffs

### 🔜 v4.6 — Multi-language Expansion
- [ ] **C/C++** full support (tree-sitter already integrated — complete pipeline)
- [ ] **Java** — AST + complexity analysis
- [ ] **Go** — imports, goroutine detection
- [ ] **Rust** — ownership patterns, unsafe block warnings
- [ ] **PHP** — deprecated function detection
- [ ] **SQL** — query complexity estimation
- [ ] Language-specific lint rules per extension

### 🔜 v4.7 — Cython & WASM Integration
- [ ] Auto-detect Cython candidates from Big-O analysis (O(n²)+ functions)
- [ ] Generate `.pyx` stubs from Python function signatures
- [ ] Compile Cython in Docker (MSVC on Windows / GCC on Linux)
- [ ] Side-by-side Python vs Cython benchmark display
- [ ] Estimated speedup shown in hover provider
- [ ] WASM compilation path via Emscripten

### 🔜 v4.8 — Execution Path Simulator
- [ ] Animated circuit-board execution flow:
  `Input → Parser → AST → Dependency Resolver → Metrics → Report`
- [ ] Step-by-step trace with timing per stage
- [ ] Export as animated SVG

### 🔬 Research spikes — integrate existing tools, don't rebuild them

Real ideas, but each is its own serious project already solved well by dedicated open-source tools — the honest move is to link/embed those, not reinvent them:

- [ ] Compiler pipeline visualization (Lexer → AST → IR → Assembly) — embed [Compiler Explorer](https://godbolt.org) (open source) instead of building a teaching compiler from scratch
- [ ] Executable analyzer (PE / ELF / Mach-O, sections, imports/exports, symbols) — wrap [Capstone](https://www.capstone-engine.org)/[LIEF](https://lief-project.github.io)/`objdump`, don't hand-write a disassembler
- [ ] Graph centrality / hub detection (NetworkX is already a dependency) — surface the most-connected files/functions in the existing Code Graph, the same idea social networks use to find "influencers"
- [ ] Standalone desktop build (PyInstaller/Nuitka + Tauri, or Zig for tiny static binaries) — a portable binary with no Docker/Node/Python required, as an alternative to `scripts/` and Docker
- [ ] Rust extension (PyO3) for the static parser's hottest path — only if profiling `static_parser.py` on large real files actually shows it's needed; not a rewrite, same pattern as adopting Ruff (Rust) over a pure-Python linter

### 🧭 Long-term / different tool category — not committed

These need runtime instrumentation (ptrace/eBPF), a running process, or live packet capture — architecturally a different kind of tool than static analysis, so they stay here as ideas rather than roadmap commitments:

- [ ] Memory visualizer (stack/heap/data/bss) — requires a running process to inspect, not source text
- [ ] Concurrency analyzer (races, deadlocks, mutex/atomic misuse) — needs real execution or tools like ThreadSanitizer, not AST inspection
- [ ] OS engine (threads, paging, scheduling, IPC) — needs kernel-level tracing
- [ ] Network analyzer (TCP/TLS/QUIC/WebSocket) — needs packet capture; this is a Wireshark-shaped tool, not a static analyzer
- [ ] Security analyzer beyond pattern detection (ROP, heap spray, use-after-free exploitation) — competes directly with mature SAST tools (Semgrep, CodeQL, Bandit); the realistic version folds into the CS Engine above as "detect the pattern + explain the CWE," not a full exploit-analysis engine

### 🔜 v5.0 — Enterprise Persistence
- [ ] PostgreSQL + Delta Lake
- [ ] Analysis history, metric comparison between versions
- [ ] WebSockets for real-time streaming
- [ ] JWT authentication, public API with rate limiting
- [x] GitHub Action for CI/CD — `.github/workflows/ci.yml` (typecheck/lint/build/test on every push/PR) + `release.yml` (tag → GitHub Release with CHANGELOG notes + frontend build artifact)

### 💡 Future
- [ ] VS Code extension
- [ ] Jupyter Notebook analysis (`.ipynb`)
- [ ] ApexVision integration (`/analyze/image` with OpenCV + YOLOv11)
- [ ] Team dashboard with aggregated metrics

---

## 📝 Changelog

See [CHANGELOG.md](CHANGELOG.md) for the full version history.

---

## 👤 Author

**Brashkie** · [Hepein Oficial](https://github.com/Brashkie) · Lima, Perú

---

## 📄 License

GPL-3.0 — see [LICENSE](LICENSE)
