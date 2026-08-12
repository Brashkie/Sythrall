# 🛰 Sythrall

> **Enterprise-grade code intelligence platform** — static analysis, ML/DL inspection, real-time editor intelligence, code graph visualization, an integrated terminal, and API monitoring. Built with TypeScript (no frameworks) + FastAPI + Rust + Monaco Editor.

<div align="center">

![Version](https://img.shields.io/badge/version-4.6.0-blue?style=flat-square)
[![CI](https://github.com/Brashkie/Sythrall/actions/workflows/ci.yml/badge.svg)](https://github.com/Brashkie/Sythrall/actions/workflows/ci.yml)
![Frontend](https://img.shields.io/badge/Frontend-Vite%20%2B%20TypeScript-646cff?style=flat-square)
![Backend](https://img.shields.io/badge/Backend-FastAPI%20%2B%20Python-009688?style=flat-square)
![Terminal](https://img.shields.io/badge/Terminal-Rust%20%2B%20axum-dea584?style=flat-square)
![Deploy](https://img.shields.io/badge/Deploy-Docker%20%2B%20Nginx-2496ed?style=flat-square)
![Tests](https://img.shields.io/badge/Tests-311%20passing-00f5a0?style=flat-square)
![Author](https://img.shields.io/badge/Author-Hepein%20Oficial-b87dff?style=flat-square)
![License](https://img.shields.io/badge/License-GPL%203.0-orange?style=flat-square)

[English](./README.md) · [Español](./README.es.md)

</div>

---

## ✨ What is Sythrall?

Sythrall is a professional code intelligence platform built as both an educational project and a production-grade demonstration. It combines a modern TypeScript frontend (zero runtime frameworks), a Python/FastAPI backend with full ML/DL support, a multi-language static analysis engine, and a Monaco Editor integration that rivals VS Code's developer experience.

### Core capabilities

| Module | Description |
|---|---|
| **📂 Project Explorer** | Expandable file tree, multi-file tabs, global search (`Ctrl+Shift+F`), symbol outline |
| **📁 Folder Browser** | Open a real folder from disk ("+ Folder") and browse it as a VSCode-style expandable tree — works in every modern browser, no Chromium-only APIs. Creates or appends to the active project, so it's not a one-off local-only view |
| **🖥 Integrated Terminal** | Real interactive shell (PowerShell/bash) in a resizable bottom panel, powered by a Rust sidecar (`portable-pty` + `axum`) — token-protected, zero-friction for local use |
| **📝 Editor Intelligence** | Real-time linting, inline diagnostics, Big-O hover, Go to Definition, Find References, Rename Symbol, semantic autocomplete |
| **🔬 Static Analysis** | AST-based multi-language parser (Python, TypeScript, C/C++) — Big-O estimation, cyclomatic complexity, WASM/Cython hints, call graph, dead code |
| **🕸 Code Graph Visual** | Import graph, Call graph, Circular dependency detection, Complexity heatmap — Mermaid Tree View, scoped to loose files or a whole uploaded project *(the interactive Force Graph renderer exists and is tested but isn't wired to a UI control yet)* |
| **📂 Projects** | Upload files, a folder or a ZIP — the result can become the **active project**, which Editor · Issues · Diagram · Static · Metrics then read directly, without re-uploading anything per panel |
| **🔍 Analysis** | pylint · flake8 · radon · AST — issues, complexity, maintainability index |
| **🤖 ML/DL** | Detection of 23 libraries, 23 pipeline patterns, 25 models, 20+ issue rules |
| **🔀 Diagrams** | Flowchart · Callgraph · Classes · Sequence — generated with Mermaid.js + zoom/pan |
| **📡 APIs** | External endpoint verification with history and response metrics |
| **📊 Dashboard** | Distribution charts, response times, execution history |
| **🔁 Diff** | Visual file comparison with highlighted changes |
| **🖥 Logs** | Real-time server log stream — also available as a switchable view inside the terminal panel |
| **🎨 Light/Dark theme** | Toggle in the topbar, dark by default, persisted across sessions |

---

## 📁 Project structure

All manifests/configs live at the repo root; `apps/` holds source code only, one directory per app/service (`apps/api`, `apps/web`, `apps/terminal`). `scripts/` is the single entry point for everything (see [scripts/README.md](scripts/README.md)).

```
sythrall/
├── package.json                    ← npm manifest (frontend deps + lint/format/build scripts)
├── package-lock.json
├── vite.config.ts                  ← root: 'apps/web', build.outDir: '../../dist'
├── tsconfig.json                   ← include: apps/web/src
├── biome.json                      ← Biome (lint/format) config
├── requirements.txt                ← backend runtime deps
├── requirements-dev.txt            ← Ruff (lint/format), dev-only
├── pyproject.toml                  ← Ruff config
├── pytest.ini                      ← testpaths: apps/api/tests
├── Cargo.toml                      ← Rust manifest — apps/terminal sidecar
├── Cargo.lock
├── docker-compose.yml
├── .dockerignore
├── START.bat / STOP.bat
├── scripts/                        ← Dev workflow without Docker (setup/dev/build/test/lint/format, .ps1 + .sh)
│   ├── run-backend.mjs             ← spawns uvicorn (npm run dev:api)
│   ├── run-terminal.mjs            ← spawns the Rust terminal sidecar (npm run dev:term)
│   └── dev-banner.mjs              ← ansimax startup banner for npm run dev
├── apps/                           ← every app/service the repo ships, one dir each
│   ├── terminal/                   ← Rust sidecar: real interactive shell over WebSocket
│   │   ├── Dockerfile
│   │   └── src/
│   │       ├── main.rs             ← axum WS handler, token auth, PTY bridging
│   │       ├── pty_session.rs      ← portable-pty wrapper (ConPTY/Unix PTY, one impl)
│   │       └── auth.rs             ← token generation + constant-time comparison
│   ├── api/                        ← FastAPI backend
│   │   ├── main.py                 ← FastAPI v4.5 (30+ routes)
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
│   │   │   └── metrics_live.py     ← POST /metrics/live — instant per-keystroke metrics
│   │   ├── services/
│   │   │   ├── project_service.py
│   │   │   └── static_parser.py    ← Multi-language parser: Python/C/C++/JS/TS
│   │   └── tests/                  ← 311 tests total (see Tests section below)
│   └── web/                        ← TypeScript frontend (Vite, zero frameworks)
│       ├── index.html
│       ├── Dockerfile.frontend
│       └── src/
│           ├── api/client.ts          ← Full API client
│           ├── components/
│           │   ├── app.ts             ← App shell + file management
│           │   ├── editor.ts          ← Monaco Editor integration
│           │   ├── editor-intelligence.ts ← Linting + hover + autocomplete (Phases 1–3)
│           │   ├── explorer.ts        ← Project Explorer (tree + tabs + search + outline)
│           │   ├── file-browser.ts    ← Folder tree from <input webkitdirectory>, cross-browser
│           │   ├── terminal.ts        ← xterm.js client for the Rust terminal sidecar
│           │   ├── events.ts          ← Global event wiring
│           │   ├── charts.ts          ← Chart.js integration
│           │   ├── mermaid.ts         ← Mermaid + zoom/pan engine
│           │   └── flow.ts            ← Execution flow diagram
│           ├── panels/
│           │   ├── analysis.ts        ← Issues/Metrics rendering (+ active-project mode)
│           │   ├── apis.ts
│           │   ├── ml.ts
│           │   ├── upload.ts          ← Projects hub: upload, recent list, active project
│           │   ├── static.ts          ← Static Analysis panel (+ active-project mode)
│           │   ├── problems.ts        ← Live Metrics Bar (session restore implemented, not yet wired — see Roadmap)
│           │   └── graph.ts           ← Code Graph Visual — Mermaid Tree View wired; Force Graph/Dir Tree implemented, not yet wired to UI
│           ├── store/state.ts         ← activeProjectId persists across reloads (localStorage)
│           ├── styles/
│           │   ├── main.css
│           │   ├── upload.css
│           │   ├── static-addon.css
│           │   └── explorer.css
│           ├── types/index.ts
│           └── utils/
│               ├── icons.ts           ← Inline SVG icon set + language badges — no emoji, no icon library
│               ├── file-tree.ts       ← FileList → nested tree (for file-browser.ts)
│               └── theme.ts           ← Light/dark toggle + persistence
└── README.md
```

---

## ⚡ Prerequisites

| Tool | Min version | Download |
|---|---|---|
| **Docker Desktop** | 4.x | [docker.com](https://www.docker.com/products/docker-desktop) |
| **Node.js** | 20.x | [nodejs.org](https://nodejs.org) |
| **Python** | 3.11+ | [python.org](https://www.python.org) |
| **Rust** | stable | [rustup.rs](https://rustup.rs) — only needed to run the integrated terminal in dev mode; the rest of the app works without it |
| **Git** | any | [git-scm.com](https://git-scm.com) |

---

## 🚀 Installation

### 1 — Clone

```bash
git clone https://github.com/Brashkie/Sythrall.git
cd Sythrall
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

See [scripts/README.md](scripts/README.md) for the full list (`build`, `test`, `lint`, `format`). Under the hood, `dev` is just `npm run dev` — `concurrently` runs Vite, uvicorn and the Rust terminal sidecar as one process (`[web]`/`[api]`/`[term]` prefixed logs), so `npm run dev` works the same directly if you'd rather skip the wrapper scripts. If `cargo` isn't installed, everything else still runs — you just won't have the integrated terminal (`[term]` prints a warning and exits, the rest is unaffected).

The `[term]` process prints a random token on startup (`🔑 Terminal token: ...`). For normal local use you don't need it — the terminal panel auto-connects. It only matters if you ever set `TERMINAL_HOST` to something other than `127.0.0.1` (see the Terminal security note below).

<details>
<summary>Manual commands (equivalent, no script)</summary>

**Backend** (from repo root — `requirements.txt` lives there, app code is in `apps/api/`):
```bash
pip install -r requirements.txt
cd apps/api
uvicorn main:app --host 0.0.0.0 --port 8000 --reload
```

**Frontend** (from repo root — `package.json`/`vite.config.ts` live there, source is in `apps/web/src`):
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
| **Terminal sidecar (Rust)** | ws://127.0.0.1:7681 (proxied through `/terminal` in dev — not meant to be opened directly) |

### 🔐 Terminal security note

The terminal sidecar binds to `127.0.0.1` by default — unlike the rest of the app (which binds `0.0.0.0`), it deliberately does **not** accept connections from other machines out of the box, because it grants real shell access. It's also protected by a per-run random token (constant-time comparison), auto-served only to requests that verifiably originate from the same machine. If you ever set `TERMINAL_HOST=0.0.0.0` (or otherwise expose port 7681) to reach it from another device on your network, auto-connect stops working for remote requests and the token must be entered manually — copy it from the `[term]` console output. Don't expose this port over an untrusted network without a real reverse proxy + TLS in front of it.

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
| @xterm/xterm | 6.0.0 | Terminal emulator (client side of the Rust sidecar) |
| [ansimax](https://github.com/Brashkie/ansimax) | 1.5.0 | ANSI/CLI rendering for the `npm run dev` startup banner |

## 🦀 Rust stack — `terminal-server`

First use of Rust in the project: a small sidecar for the integrated terminal, not a rewrite of any existing Python code.

| Crate | Use |
|---|---|
| axum | WebSocket server + HTTP routing |
| tokio | Async runtime |
| portable-pty | ConPTY (Windows) / PTY (Unix) — one implementation for both |
| subtle | Constant-time token comparison |
| rand | CSPRNG for the per-run token |

---

## 🧪 Tests

```bash
cd apps/api && pytest
# pytest.ini lives at the repo root and is auto-discovered; testpaths = apps/api/tests
```

```
test_upload.py            29 ✅
test_analysis.py          62 ✅
test_intelligence.py     109 ✅
test_graph.py             46 ✅
test_graph_phase2.py      31 ✅
test_metrics_live.py      34 ✅
─────────────────────────────
Total: 311 passed
```

The Rust sidecar (`terminal-server`) has its own checks — `cargo build --release`, `cargo clippy -- -D warnings`, `cargo test` — run via the `terminal` job in [`ci.yml`](.github/workflows/ci.yml), separate from the Python suite above.

---

## 🧹 Tooling (lint & format)

[Biome](https://biomejs.dev) formats/lints the TypeScript frontend; [Ruff](https://docs.astral.sh/ruff/) does the same for the Python backend. Both run through `scripts/`:

```bash
./scripts/lint.sh      # or .\scripts\lint.ps1   — check only, no writes
./scripts/format.sh    # or .\scripts\format.ps1 — writes fixes
```

Equivalent direct commands (from repo root): `npm run lint` / `npm run format`, `ruff check apps/api` / `ruff format apps/api`.

---

## 🔄 Docker commands

```bash
docker compose logs -f                         # Live logs
docker compose build --no-cache && docker compose up -d  # Full rebuild
docker compose ps                              # Active containers
docker exec -it sythrall-backend bash          # Backend shell
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
- [x] Interactive Force Graph (custom physics engine, no D3) *(implemented and tested — a later audit found the whole graph module had zero callers anywhere in the app; Mermaid Tree View got wired in Unreleased below, Force Graph is still waiting on its UI)*
- [x] Project Explorer: file tree + multi-file tabs + global search + outline
- [x] 316 automated tests

### 🔧 v4.3 — Problems Panel + Live Metrics *(shipped code, wiring found incomplete years later — see Unreleased)*
- [ ] **Problems panel** (VSCode-style): errors · warnings · Big-O · complexity · security findings — *implemented in `panels/problems.ts`, but it targets the same DOM container as the existing file-analysis view and would clobber content that view has and Problems doesn't (Pylint score, Maintainability Index, per-function complexity); needs a placement decision before wiring it in*
- [x] **Live metrics bar** in editor: LOC · functions · imports · complexity score · Big-O worst · parse time (ms) *(connected in Unreleased below — the module existed since this version but `editor.ts` never called it)*
- [x] Auto-recovery if parser fails (safe mode + regex fallback)
- [x] Corrupt file detection
- [x] Session restore *(connected in Unreleased below, alongside persisting the active project — restoring "which file was open" only became meaningful once there was a real project to restore its content from)*

### 🔧 v4.4 — Computer Science Engine *(shipped partial — direct extension of the existing analysis engine, no new architecture)*

Not just "what" the code does — *why* it behaves that way. Built entirely on data `static_parser.py` and the Big-O engine already compute:

- [x] Full complexity picture per function: Θ (tight bound), Ω (best case), O (worst case) — not just worst-case O *(Python only for now; C/C++/JS/TS still show O only)*
- [x] "Why" explanation attached to every Big-O result (e.g. *"2 nested loops — inner loop runs n times per outer iteration"*)
- [x] Recursion detected → tail-call detection + "Lambda Calculus" framing *(recursion-depth estimate skipped — depends on runtime input, not reliably computable statically)*
- [ ] Regex detected → classify as Finite Automaton / Chomsky Type-3 (Regular)
- [ ] Grammar/parser-shaped code detected → Context-Free Grammar / Pushdown Automaton / Chomsky Type-2
- [ ] Graph traversal detected → label as DFS/BFS/topological sort, O(V+E)

### ✅ v4.5 — Integrated Terminal + Folder Explorer + Theme

Not originally on this roadmap — came directly from user feedback mid-development, folded in because each piece was small and well-scoped on its own:

- [x] **Integrated terminal**, real interactive shell over WebSocket — first use of **Rust** in the project (`terminal-server` sidecar: `portable-pty` + `axum`), token-protected, zero-friction auto-connect for local use, panel switcher between the shell and a live Logs view
- [x] **Folder explorer** in the sidebar ("+ Folder") — VSCode-style expandable tree from a real disk folder, cross-browser via `webkitdirectory` (deliberately *not* the File System Access API, which is Chromium-only)
- [x] **Light/dark theme** toggle, persisted, dark by default
- [x] [`ansimax`](https://github.com/Brashkie/ansimax) (own library) for the `npm run dev` startup banner

### 🔧 v4.6.0 — Rebrand + `apps/` restructure + large-project scaling + enterprise-style shell *(shipped partial — Problems panel placement still open)*

Not originally on this roadmap — triggered by wanting the project to scale to real large codebases and to look/behave like the reference tools used during development (Aikido, Datadog, DeepSource), not by a version-number plan:

- [x] Renamed **CodeWatch PRO → Sythrall** across the project (package name, Docker services, internal identifiers, git remote)
- [x] Restructured into `apps/api` · `apps/web` · `apps/terminal` — one directory per service, tool manifests stay at repo root (Turborepo/Nx-style layout)
- [x] **Large-project benchmark**: built a reproducible synthetic-project harness (up to 4003 files, up to 1600 functions/file) instead of assuming a rewrite was needed. Found and fixed three real O(n²) bugs — two hidden inside one-line comprehensions, one a dead computation the frontend never read. Import Graph generation on the 4003-file case went from 3.9s to 0.128s (30×) with zero new languages. Details in [`CHANGELOG.md`](CHANGELOG.md#460). The parser itself (`static_parser.py`) was already linear and needed no changes — the earlier PyO3 finding below still holds.
- [x] **Nav rail replaces the horizontal tabbar** — persistent vertical icon nav (`apps/web/src/utils/icons.ts`, inline SVGs, `stroke="currentColor"` so they follow the active theme with zero extra code), same pattern the reference tools use. `switchTab()` didn't change — the new nav items kept the same `class="tab"`/`data-tab`/`id="t-*"` convention, so this was purely a markup/CSS change.
- [x] **One active project, not four disconnected upload paths.** Before: "+ Code"/"+ Folder"/"+ Log" in the sidebar were ephemeral (lost on refresh, never touched the backend) while Projects was the only persisted path — two mental models for the same idea. Now "+ Code"/"+ Folder" create or append to the **active project** (same backend endpoints Projects already used, `project_id` now optional on `/api/upload/{files,folder}` to support appending), and Editor · Issues · Diagram · Static · Metrics all read from whichever project is active — pick a project once, work across every panel.
- [x] **Audit-driven fixes**, found using the same "does this actually have a caller" method that caught the Force Graph gap above: reconnected the Live Metrics Bar and Session Restore (`panels/problems.ts`, written for v4.3, never called from `editor.ts`); active project now persists to `localStorage` so both restore automatically on reload; fixed the APIs tab badge (never updated); Metrics panel gained an active-project mode matching Issues/Diagram/Static.
- [ ] **Problems panel** placement — still needs a decision (see v4.3 note above) before it can be wired in without clobbering the existing file-analysis view.

---

The items below are grouped by how well-founded they are, not by version number — the goal is to be honest about scope before committing to it.

### 🔜 v4.7 — Data Structure Detector *(same heuristic-pattern style as the existing WASM-hint/dead-code detectors)*

- [ ] Detect AVL / Red-Black Tree / Trie / Heap / Segment Tree / Fenwick Tree / Bloom Filter / B-Tree / HashMap / Skip List from AST shape
- [ ] For each match: complexity, typical use case, tradeoffs

### 🔜 v4.8 — Multi-language Expansion

Same pattern throughout: these are languages Sythrall can *read and analyze* — a tree-sitter grammar plus rules, same as the existing Python/TS/C/C++ pipeline. None of these require Sythrall's own engine to be written in that language; that's a separate, unproven bet (see Research spikes below).

- [ ] **C/C++** full support (tree-sitter already integrated — complete pipeline)
- [ ] **Java** — AST + complexity analysis
- [ ] **Go** — imports, goroutine detection
- [ ] **Rust** — ownership patterns, unsafe block warnings *(analysis-target Rust support — separate from the `terminal-server` sidecar shipped in v4.5, which is tooling, not a language the analyzer parses)*
- [ ] **PHP** — deprecated function detection
- [ ] **SQL** — query complexity estimation
- [ ] **Fortran** — DO-loop / array-op detection, vectorization/SIMD candidates, scientific-computing domain hints *(analysis target only — Sythrall already gets Fortran-level numerical performance for free via numpy/scipy's compiled LAPACK/BLAS backends, no reason to write Fortran itself)*
- [ ] **Assembly (x86-64)** — instruction/register/control-flow breakdown from raw `.s`/inline-asm snippets *(pattern-matching on text the user pastes, not a disassembler — the PE/ELF/Mach-O binary case below already wraps Capstone/LIEF instead of hand-rolling one)*
- [ ] Language-specific lint rules per extension

### 🔜 v4.9 — Cython & WASM Integration
- [ ] Auto-detect Cython candidates from Big-O analysis (O(n²)+ functions)
- [ ] Generate `.pyx` stubs from Python function signatures
- [ ] Compile Cython in Docker (MSVC on Windows / GCC on Linux)
- [ ] Side-by-side Python vs Cython benchmark display
- [ ] Estimated speedup shown in hover provider
- [ ] WASM compilation path via Emscripten

### 🔜 v4.10 — Execution Path Simulator
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
- [x] ~~Rust extension (PyO3) for the static parser's hottest path~~ — **investigated twice with real benchmarks, not adopted either time**: first pass profiled `static_parser.py` on large *individual* files (250+ functions, 3000+ synthetic) and found the AST-walk consolidation already done was noise-level neutral (~160ms for realistic file sizes). Second pass (Unreleased, above) tested the other axis — thousands of *files* in one project, not one huge file — and found the parser itself still scaled linearly; the actual O(n²) cost was three ordinary Python bugs in the code *around* the parser, fixed without any new language. Rust *did* end up in the project (v4.5's `terminal-server`), but for a genuinely Rust-shaped problem — cross-platform PTY handling — not as a speed rewrite of working Python. If a real bottleneck ever shows up in `parse_file` itself, the integration model would be an Axum sidecar (same pattern as `terminal-server`) talking HTTP to FastAPI, not PyO3 embedding — simpler to maintain solo, no native-binding build matrix.

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
- [ ] WebSockets for real-time analysis streaming *(the terminal's WebSocket, shipped in v4.5, is a different thing — an interactive PTY shell, not a streaming-analysis channel)*
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
