# 🛰 Sythrall

> **Enterprise-grade code intelligence platform** — static analysis, ML/DL inspection, real-time editor intelligence, code graph visualization, an integrated terminal, and API monitoring. Built with TypeScript (no frameworks) + FastAPI + Rust + Monaco Editor.

<div align="center">

![Version](https://img.shields.io/badge/version-4.7.1-blue?style=flat-square)
[![CI](https://github.com/Brashkie/Sythrall/actions/workflows/ci.yml/badge.svg)](https://github.com/Brashkie/Sythrall/actions/workflows/ci.yml)
![Frontend](https://img.shields.io/badge/Frontend-Vite%20%2B%20TypeScript-646cff?style=flat-square)
![Backend](https://img.shields.io/badge/Backend-FastAPI%20%2B%20Python-009688?style=flat-square)
![Terminal](https://img.shields.io/badge/Terminal-Rust%20%2B%20axum-dea584?style=flat-square)
![Deploy](https://img.shields.io/badge/Deploy-Docker%20%2B%20Nginx-2496ed?style=flat-square)
![Tests](https://img.shields.io/badge/Tests-329%20passing-00f5a0?style=flat-square)
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
| **🔍 Analysis** | pylint · flake8 · AST · `complexity-engine` (Rust) — issues, complexity, maintainability index |
| **🤖 ML/DL** | Detection of 23 libraries, 23 pipeline patterns, 25 models, 20+ issue rules |
| **🔀 Diagrams** | Flowchart · Callgraph · Classes · Sequence — generated with Mermaid.js + zoom/pan |
| **📡 APIs** | External endpoint verification with history and response metrics |
| **📊 Dashboard** | Distribution charts, response times, execution history |
| **🔁 Diff** | Visual file comparison with highlighted changes |
| **🖥 Logs** | Real-time server log stream — also available as a switchable view inside the terminal panel |
| **🎨 Light/Dark theme** | Toggle in the topbar, dark by default, persisted across sessions |

---

## 📁 Project structure

All manifests/configs live at the repo root; source lives in two directories with an explicit split — `apps/` is the two products a user actually runs (`apps/api`, `apps/web`), `services/` is the independent Rust processes those products call over HTTP (`services/terminal`, `services/complexity`) but that no one launches directly. `scripts/` is the single entry point for everything (see [scripts/README.md](scripts/README.md)).

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
├── Cargo.toml                      ← Rust manifest — services/terminal + services/complexity sidecars (2 [[bin]], 1 [lib])
├── Cargo.lock
├── docker-compose.yml
├── .dockerignore
├── START.bat / STOP.bat
├── scripts/                        ← Dev workflow without Docker (setup/dev/build/test/lint/format, .ps1 + .sh)
│   ├── run-backend.mjs             ← spawns uvicorn (npm run dev:api)
│   ├── run-terminal.mjs            ← spawns the Rust terminal sidecar (npm run dev:term)
│   ├── run-complexity.mjs          ← spawns the Rust complexity sidecar (npm run dev:cx)
│   └── dev-banner.mjs              ← ansimax startup banner for npm run dev
├── apps/                           ← products a user actually runs — one dir each
│   ├── api/                        ← FastAPI backend
│   │   ├── main.py                 ← FastAPI v4.6 (30+ routes)
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
│   │   │   ├── static_parser.py    ← Multi-language parser: Python/C/C++/JS/TS
│   │   │   └── complexity_client.py ← HTTP client for the Rust complexity-engine sidecar
│   │   └── tests/                  ← 329 tests total (see Tests section below)
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
│           │   ├── problems.ts        ← Live Metrics Bar + Session Restore + Problems Panel (all 3 wired, each with its own home)
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
├── services/                       ← independent Rust processes `apps/` calls over HTTP — no one launches these directly
│   ├── terminal/                   ← real interactive shell over WebSocket
│   │   ├── Dockerfile
│   │   └── src/
│   │       ├── main.rs             ← axum WS handler, token auth, PTY bridging
│   │       ├── pty_session.rs      ← portable-pty wrapper (ConPTY/Unix PTY, one impl)
│   │       └── auth.rs             ← token generation + constant-time comparison
│   └── complexity/                 ← cyclomatic complexity + MI + raw metrics (replaces radon) + Phase 18 rich Python analysis
│       ├── Dockerfile
│       ├── benches/
│       │   ├── complexity_bench.rs ← Criterion — measured 9-21x faster than radon
│       │   └── parse_bench.rs      ← Criterion — analyze_rich() vs. _parse_python(), 5.6-20.6x faster
│       └── src/
│           ├── main.rs             ← axum HTTP server (GET /health, POST /metrics/complexity, POST /parse/python)
│           ├── lib.rs              ← analyze()/analyze_rich() entrypoints, shared by main.rs and the benchmarks
│           ├── parser.rs           ← rustpython-parser wrapper + byte-offset→line resolution
│           ├── complexity.rs       ← McCabe cyclomatic complexity (own logic, not radon's)
│           ├── maintainability.rs  ← Maintainability Index (Coleman-Oman formula, own Halstead count)
│           ├── raw.rs              ← loc/lloc/sloc/comments/blank/multi
│           ├── walk.rs             ← generic exhaustive AST walker (ast.walk() equivalent), shared by the modules below
│           ├── bigo.rs             ← Big-O/Θ/Ω heuristic — port of static_parser.py's _infer_big_o_python/_theta_omega_python
│           ├── recursion.rs        ← tail-call detection — port of _recursion_info_python
│           ├── classifiers.rs      ← regex/grammar/graph-traversal CS Engine classifiers — port of Phase 12's Python versions
│           ├── structure.rs        ← class/import extraction + AST helpers (decorators, docstrings, calls)
│           └── rich.rs             ← analyze_rich() orchestrator — same functions/classes/imports/summary shape as _parse_python()
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
| **Complexity sidecar (Rust)** | http://127.0.0.1:7682 (called by the backend, not the browser — not meant to be opened directly) |

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
POST /intel/analyze      → Heavy analyze ~80ms (pylint + complexity engine + Big-O)
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
| pylint / flake8 | latest | Code quality (complexity/MI moved to the `complexity-engine` Rust sidecar — see 🦀 Rust stack below) |
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

## 🦀 Rust stack — two sidecars, one `Cargo.toml`

`terminal-server` (first use of Rust in the project — a small sidecar for the integrated terminal, not a rewrite of any existing Python code) and `complexity-engine` (replaces the `radon` pip dependency — cyclomatic complexity, Maintainability Index, and raw line metrics, computed in-process instead of imported from a third-party library whose internals the project doesn't own). Both bins share one root `Cargo.toml` — `services/{terminal,complexity}/` hold source only, no per-service manifest.

| Crate | Used by | Use |
|---|---|---|
| axum | both | HTTP/WebSocket server + routing |
| tokio | both | Async runtime |
| portable-pty | terminal-server | ConPTY (Windows) / PTY (Unix) — one implementation for both |
| subtle | terminal-server | Constant-time token comparison |
| rand | terminal-server | CSPRNG for the per-run token |
| rustpython-parser | complexity-engine | Python source → AST (the complexity/MI/raw-metrics logic on top of it is this project's own code, not radon's) |
| criterion | complexity-engine (dev) | Benchmarks — `cargo bench --bench complexity_bench` |

`complexity-engine` binds to `127.0.0.1:7682` by default (`COMPLEXITY_HOST`/`COMPLEXITY_PORT` overridable, same convention as the terminal sidecar) and exposes `GET /health` + `POST /metrics/complexity`. No auth token — unlike the terminal, it's a pure computation endpoint with no shell/filesystem access, so the risk a token mitigates there doesn't apply here. The Python backend calls it via `apps/api/services/complexity_client.py` and degrades gracefully (empty complexity/MI, no crash) if the sidecar isn't running — same pattern as flake8/pylint being optional.

---

## 🧪 Tests

```bash
cd apps/api && pytest
# pytest.ini lives at the repo root and is auto-discovered; testpaths = apps/api/tests
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
Total: 329 passed
```

Both Rust sidecars have their own checks — `cargo build --release`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (29 unit tests for `complexity-engine`'s complexity/MI/raw-metrics/Big-O/recursion/CS-Engine-classifier logic, hand-computed values and parity-tested against the Python implementation) — run via the `terminal` job in [`ci.yml`](.github/workflows/ci.yml), separate from the Python suite above (same job builds/tests both bins, since they share one `Cargo.toml`).

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

Organized by phase, not version number — a phase tracks one coherent chunk of work
across the project's lifetime; actual releases/tags still carry semantic version
numbers (see [`CHANGELOG.md`](CHANGELOG.md)), the two aren't the same axis. Same
convention as [`ansimax`](https://github.com/Brashkie/ansimax)'s own roadmap.

**✅ Complete · 🟡 Partial · 🔴 Planned**

### Language philosophy

Sythrall isn't "built with six languages" — each language sits at the layer where it
actually earns its place, and Phases 13+ below are organized around that split
instead of a flat "add more languages" list:

| Layer | Language | Role | Evidence so far |
|---|---|---|---|
| Interaction | **TypeScript** | UI, Monaco integration, editor intelligence, diagrams — the only layer a user directly touches | Vite + Monaco + Mermaid + Chart.js + xterm, all shipped (Phases 1–9) |
| Intelligence & Science | **Python** | The analysis engine itself, ML/DL detection, orchestration | `static_parser.py`, 23 library detections, ML/DL Inspector (Phase 2) |
| Native Analysis | **Rust** | CPU/memory-bound analysis engines — adopted only where a measured bottleneck justifies it, never assumed | `terminal-server` (PTY handling, a genuinely Rust-shaped problem), `complexity-engine` (9–21× faster than `radon`, benchmarked with Criterion before deciding — Phase 11) |
| Scientific/HPC | **Fortran** | Analysis *target*, not an implementation language — Sythrall already gets Fortran-level numeric performance for free via numpy/scipy's compiled LAPACK/BLAS | Planned (Phase 20) |
| Machine-level | **Assembly** | Analysis target for instruction/register/control-flow — wraps Capstone/LIEF instead of hand-rolling a disassembler | Planned (Phase 19) |
| Native tooling | **Zig** | Build, cross-compilation, standalone distribution — a different concern from Rust's analysis-engine role, not competing with it | Planned (Phase 22) |

The rule for moving anything into Rust (or any native language) is the same one
`complexity-engine` already proved out: profile first, benchmark the replacement,
keep it only if the numbers justify it. Phase 10's large-project investigation found
the real O(n²) cost was three ordinary Python bugs, not the parser — fixed without a
new language. Phase 11's `complexity-engine` found a real, measured 9–21× win —
adopted. Both outcomes came from the same process; neither was assumed going in.
That process, not a language preference, is what Phase 18 below extends.

### Where this is going: Computer Science Intelligence, not a linter

The honest one-line description of Sythrall today is "reads code, computes Big-O."
Phases 13–21 below are the plan for growing that into something more specific:
*Sythrall analyzes software from the mathematical/algorithmic structures underneath
it, through to the compiler, the machine code, and the hardware it runs on.* Not by
becoming a math solver or a compiler — by connecting the CS theory that already
explains *why* Sythrall's existing heuristics work (Big-O, the Chomsky-hierarchy
classifiers from Phase 8/12, the Lambda-Calculus framing on tail recursion) to the
rest of the theory those ideas come from, and building detectors for it the same
heuristic, benchmark-first way everything else in this roadmap got built:

```
                    Computer Science Engine
                              │
        ┌─────────────────────┼─────────────────────┐
        │                     │                      │
   Algorithms             Mathematics             Languages
        │                     │                      │
    Big-O/Θ/Ω            Discrete Math          Formal Languages
    Data Structures       Logic                 Grammars
    Graphs                Recurrences           Parsing Theory
        │                     │                      │
        └─────────────────────┼──────────────────────┘
                              │
                       Compiler Theory
                              │
                    Lexer → AST → IR → Codegen
                              │
                        Machine Code
                              │
                             CPU
```

Each layer below is still gated by the same rules already established in this
roadmap: heuristic and explicit about it (matching the WASM-hint/CS-Engine-classifier
style, not claiming semantic proof), and any native-language adoption still requires
its own before/after benchmark (Phase 18's rule, proven twice already).

### ✅ Phase 1 — Foundation
- [x] Flask backend + pylint, flake8, radon
- [x] TypeScript + Vite frontend (zero frameworks)
- [x] Monaco Editor + Chart.js + Docker

### ✅ Phase 2 — ML/DL Inspector
- [x] 23 library detections, 23 pipeline patterns, 25 models
- [x] 20+ issue rules (data leakage, reproducibility, framework-specific)
- [x] Score 0–100 + Mermaid pipeline diagram

### ✅ Phase 3 — Zoom/Pan + Responsive
- [x] Diagram zoom + pan, responsive layout, mobile bottom navigation

### ✅ Phase 4 — FastAPI + Project Upload
- [x] Flask → FastAPI migration, file/folder/ZIP upload, 83 tests

### ✅ Phase 5 — Static Analysis + Editor Intelligence
- [x] Multi-language AST parser (Python · TypeScript · C/C++) — no AI
- [x] Big-O estimation per function, cyclomatic complexity, WASM/Cython hints
- [x] Real-time linting (fast ~1ms, heavy ~80ms), inline diagnostics
- [x] Hover: signature + Big-O + CC + docstring
- [x] Go to Definition · Find References · Semantic autocomplete · Rename Symbol
- [x] 162 automated tests

### ✅ Phase 6 — Code Graph + Project Explorer
- [x] Import Graph · Call Graph · Circular Dependencies · Complexity Heatmap
- [x] Sub-phase A: graphs from sidebar; Sub-phase B: graphs from uploaded ZIP projects
- [x] Cross-folder dependency resolution, NetworkX circular detection
- [x] Interactive Force Graph (custom physics engine, no D3) *(implemented and tested — a later audit found the whole graph module had zero callers anywhere in the app; Mermaid Tree View got wired in Phase 10 below, Force Graph is still waiting on its UI)*
- [x] Project Explorer: file tree + multi-file tabs + global search + outline
- [x] 316 automated tests

### ✅ Phase 7 — Problems Panel + Live Metrics *(shipped code, wiring found incomplete years later — see Phases 10 & 12)*
- [x] **Problems panel** (VSCode-style): errors · warnings · Big-O · complexity · security findings — *originally shared a DOM container with the file-analysis view and would have clobbered it; resolved by giving it its own right-panel sub-tab (`#rpp-problems`) instead of merging the two views — see Phase 12 below*
- [x] **Live metrics bar** in editor: LOC · functions · imports · complexity score · Big-O worst · parse time (ms) *(connected in Phase 10 below — the module existed since this phase but `editor.ts` never called it)*
- [x] Auto-recovery if parser fails (safe mode + regex fallback)
- [x] Corrupt file detection
- [x] Session restore *(connected in Phase 10 below, alongside persisting the active project — restoring "which file was open" only became meaningful once there was a real project to restore its content from)*

### ✅ Phase 8 — Computer Science Engine *(direct extension of the existing analysis engine, no new architecture)*

Not just "what" the code does — *why* it behaves that way. Built entirely on data `static_parser.py` and the Big-O engine already compute:

- [x] Full complexity picture per function: Θ (tight bound), Ω (best case), O (worst case) — not just worst-case O *(Python only for now; C/C++/JS/TS still show O only)*
- [x] "Why" explanation attached to every Big-O result (e.g. *"2 nested loops — inner loop runs n times per outer iteration"*)
- [x] Recursion detected → tail-call detection + "Lambda Calculus" framing *(recursion-depth estimate skipped — depends on runtime input, not reliably computable statically)*
- [x] Regex detected → classify as Finite Automaton / Chomsky Type-3 (Regular) *(direct `re.XXX(...)` calls only — doesn't trace a `re.Pattern` saved in a variable)*
- [x] Grammar/parser-shaped code detected → Context-Free Grammar / Pushdown Automaton / Chomsky Type-2 *(heuristic: name keyword + recursion/explicit-stack shape — both signals required to keep false positives down)*
- [x] Graph traversal detected → label as DFS/BFS/topological sort, O(V+E) *(heuristic: variable-name signals — `visited`/`seen`/`explored`, `in_degree`, a queue with `.popleft()` — not real data-flow analysis)*

### ✅ Phase 9 — Integrated Terminal + Folder Explorer + Theme

Not originally on this roadmap — came directly from user feedback mid-development, folded in because each piece was small and well-scoped on its own:

- [x] **Integrated terminal**, real interactive shell over WebSocket — first use of **Rust** in the project (`terminal-server` sidecar: `portable-pty` + `axum`), token-protected, zero-friction auto-connect for local use, panel switcher between the shell and a live Logs view
- [x] **Folder explorer** in the sidebar ("+ Folder") — VSCode-style expandable tree from a real disk folder, cross-browser via `webkitdirectory` (deliberately *not* the File System Access API, which is Chromium-only)
- [x] **Light/dark theme** toggle, persisted, dark by default
- [x] [`ansimax`](https://github.com/Brashkie/ansimax) (own library) for the `npm run dev` startup banner

### 🟡 Phase 10 — Rebrand + `apps/` restructure + large-project scaling + enterprise-style shell

Not originally on this roadmap — triggered by wanting the project to scale to real large codebases and to look/behave like the reference tools used during development (Aikido, Datadog, DeepSource), not by a version-number plan:

- [x] Renamed **CodeWatch PRO → Sythrall** across the project (package name, Docker services, internal identifiers, git remote)
- [x] Restructured into `apps/api` · `apps/web` · `apps/terminal` — one directory per service, tool manifests stay at repo root (Turborepo/Nx-style layout) *(`apps/terminal` moved to `services/terminal` later, splitting user-facing products from independent processes — see [`CHANGELOG.md`](CHANGELOG.md))*
- [x] **Large-project benchmark**: built a reproducible synthetic-project harness (up to 4003 files, up to 1600 functions/file) instead of assuming a rewrite was needed. Found and fixed three real O(n²) bugs — two hidden inside one-line comprehensions, one a dead computation the frontend never read. Import Graph generation on the 4003-file case went from 3.9s to 0.128s (30×) with zero new languages. Details in [`CHANGELOG.md`](CHANGELOG.md#460). The parser itself (`static_parser.py`) was already linear and needed no changes — the earlier PyO3 finding below still holds.
- [x] **Nav rail replaces the horizontal tabbar** — persistent vertical icon nav (`apps/web/src/utils/icons.ts`, inline SVGs, `stroke="currentColor"` so they follow the active theme with zero extra code), same pattern the reference tools use. `switchTab()` didn't change — the new nav items kept the same `class="tab"`/`data-tab`/`id="t-*"` convention, so this was purely a markup/CSS change.
- [x] **One active project, not four disconnected upload paths.** Before: "+ Code"/"+ Folder"/"+ Log" in the sidebar were ephemeral (lost on refresh, never touched the backend) while Projects was the only persisted path — two mental models for the same idea. Now "+ Code"/"+ Folder" create or append to the **active project** (same backend endpoints Projects already used, `project_id` now optional on `/api/upload/{files,folder}` to support appending), and Editor · Issues · Diagram · Static · Metrics all read from whichever project is active — pick a project once, work across every panel.
- [x] **Audit-driven fixes**, found using the same "does this actually have a caller" method that caught the Force Graph gap above: reconnected the Live Metrics Bar and Session Restore (`panels/problems.ts`, written for Phase 7, never called from `editor.ts`); active project now persists to `localStorage` so both restore automatically on reload; fixed the APIs tab badge (never updated); Metrics panel gained an active-project mode matching Issues/Diagram/Static.
- [ ] **Problems panel** placement — still needs a decision (see Phase 7 note above) before it can be wired in without clobbering the existing file-analysis view. *(resolved in Phase 12 below — own right-panel sub-tab instead of sharing a container)*

### ✅ Phase 11 — `radon` replaced by an own Rust sidecar (`complexity-engine`)

Not a performance rewrite — the trigger was not wanting to depend on a third-party library's internals for something the project can own outright, for a codebase one person maintains solo. Measured before claiming any speed win, following the same benchmark-first approach as the large-project scaling work above:

- [x] **New Rust sidecar `apps/complexity`** *(moved to `services/complexity` later — see [`CHANGELOG.md`](CHANGELOG.md))*, same architecture as `terminal-server` (persistent process, HTTP, not a subprocess-per-call and not a PyO3 native extension) — `rustpython-parser` for the AST, this project's own code for cyclomatic complexity (McCabe), Maintainability Index (Coleman-Oman formula), and raw line metrics. Both sidecars now share one root `Cargo.toml` (2 `[[bin]]`, 1 `[lib]`).
- [x] **Benchmarked with Criterion against real `radon`**, not assumed: 10 functions — 0.42ms vs 8.97ms; 100 functions — 4.7ms vs 89ms; 1000 functions — 102ms vs 899ms. 9–21× faster, measured on the same synthetic files both ways.
- [x] `radon==6.0.1` removed from `requirements.txt`; `services/complexity_client.py` calls the sidecar over HTTP and degrades gracefully (empty complexity/MI, no crash) if it isn't running — same optional-capability pattern as flake8/pylint.
- [x] Fixed a real startup-race bug found while wiring this in: the old design cached "is the tool available" once at backend startup, which meant a slow first `cargo build` could leave the capability stuck `false` for the rest of the session even after the sidecar came up. The actual analysis call sites no longer gate on that cached flag — they call the sidecar live and fall back gracefully per-request instead.
- [x] 15 Rust unit tests (`cargo test`) for complexity/MI/raw-metrics against hand-computed values, run by the same CI job that already built `terminal-server`.

### ✅ Phase 12 — Closed the last 3 CS Engine classifiers + Problems Panel placement

Closes out the Phase 8 CS Engine roadmap items and the open Problems Panel decision from Phase 7/Phase 10:

- [x] **Regex → Chomsky Type-3 (Regular)**: detects direct `re.compile/match/search/findall/...` calls per function. Honest about its limit — doesn't trace a `re.Pattern` saved in a variable, only direct `re.XXX(...)` calls.
- [x] **Grammar/parser-shaped code → Chomsky Type-2 (Context-Free)**: heuristic requires *both* a name signal (`parse`/`grammar`/`tokenize`/`lexer`/...) *and* a shape signal (recursion or an explicit append/pop stack pattern) — either signal alone produced too many false positives in testing (a plain recursive `factorial` isn't a parser just because it's recursive).
- [x] **Graph traversal → BFS/DFS/Topological Sort, O(V+E)**: heuristic on variable names (`visited`/`seen`/`explored`, `in_degree`) plus a queue (`.popleft()`) vs. stack/recursion shape to tell BFS from DFS. Same explicitly-heuristic, not-semantic-analysis style as the existing WASM-hint detector (`_wasm_hints_python`) — the new code follows its exact conventions.
- [x] **Problems Panel got its own home**: a 4th right-panel sub-tab (`Flujo · Análisis · Servidor · Problems`, `#rpp-problems`/`#problems-content`) instead of trying to merge it into the existing file-analysis view — resolves the DOM-container conflict documented since Phase 7 without touching that view's richer content (Pylint score, MI, per-function table). Wired into `editor.ts::applyMarkers()`, exactly where `panels/problems.ts` had documented the intended integration point since it was written.
- [x] Removed 3 confirmed-dead exports with zero callers, re-verified twice (`editor.ts::copyEditorContent`, `explorer.ts::explorerMarkModified`/`explorerRefresh`). A longer list of exports with no *visible* caller was found in the same pass but deliberately left alone — not enough certainty they aren't API surface for a feature that hasn't landed yet (same situation Force Graph was in before it got wired).

---

The phases below are the 9 conceptual pillars of "Computer Science Intelligence" (see above) plus a closing productization phase — grouped by how well-founded they are, not by how far in the future they sit. Phases 13–14 are the most direct continuation of what Phase 8/12 already shipped; 15–17 are genuinely new theoretical ground; 18–20 build out the language-philosophy table; 21–22 close the loop (runtime + how the product actually reaches people).

### 🔴 Phase 13 — Algorithmic Intelligence *(extends Phase 8's O/Θ/Ω engine, not a new architecture)*

Phase 8 already computes worst-case O, best-case Ω, and tight-bound Θ per function — a specific, standard application of asymptotic notation in algorithm analysis (matching the convention used by CLRS and most algorithms texts). Worth making explicit rather than implicit, since the general definitions are broader than that one use:

- [ ] **Asymptotic notation reference**, surfaced wherever a Big-O result already appears: O (upper bound), Ω (lower bound), Θ (tight bound), o (strict upper bound), ω (strict lower bound) — general definitions first, Sythrall's specific worst/best/tight-case convention second, so the distinction is taught, not assumed
- [ ] **Space complexity** alongside time complexity — auxiliary space vs. input space, same O/Θ/Ω treatment already applied to time, same heuristic AST-based approach (no execution)
- [ ] **Recurrence relation recognition** for divide-and-conquer functions — e.g. `merge_sort`'s `T(n) = 2T(n/2) + Θ(n)` pattern-matched to the Master Theorem's three cases instead of falling back to the generic loop/recursion heuristic already in place, landing on `Θ(n log n)` with the recurrence shown, not just the answer

### 🔴 Phase 14 — Data Structures & Graph Intelligence *(same heuristic-pattern style as the existing WASM-hint/dead-code detectors)*

Direct continuation of the CS Engine (Phase 8/12) — same static-analysis approach, no new architecture. Not just naming the structure, explaining it:

- [ ] Detect AVL / Red-Black Tree / Trie / Heap / Segment Tree / Fenwick Tree / Bloom Filter / B-Tree / HashMap / Skip List from AST shape
- [ ] For each match: complexity (time *and* space), typical operations, use cases, tradeoffs vs. the alternatives — the same "why," not just "what," standard the Big-O engine already holds itself to
- [ ] **Graph algorithms as first-class operations** on the Code Graph that already exists (Phase 6), not just a rendering target — BFS/DFS, shortest path, connected components, cycle detection (the existing circular-import detector is one instance of this, generalized), topological ordering, centrality/hub detection (NetworkX is already a dependency — surfacing the most-connected files/functions, the same idea social networks use to find "influencers")
- [ ] **Nested-structure interaction detection** — e.g. a hash table iterated inside a loop that also touches the same hash table, flagged as a potential O(n²) traversal; complexity analysis that looks at how structures *combine*, not just complexity per function in isolation

### 🔴 Phase 15 — Mathematical Intelligence *(discrete math used to explain programs, not a math solver)*

Not "Sythrall proves theorems." The discrete-math concepts that already underpin the rest of this roadmap, made explicit where they're already implicit in what Sythrall detects:

- [ ] Sets & relations — `dict`/`set` operations already detected (Phase 8's graph-traversal classifier reads `visited`/`seen` as sets) framed explicitly as set operations, not just variable-name heuristics
- [ ] Functions — pure vs. side-effecting classification (does a function only read its arguments and return, or mutate outer state?), a real static property, not a heuristic guess
- [ ] Combinatorics — nested-loop cardinality already computed for Big-O (Phase 8's loop-depth signal) reframed explicitly as the combinatorial count it actually is
- [ ] Boolean algebra — De Morgan simplification hints on complex conditionals (`not (a and b)` ⟷ `(not a) or (not b)`), surfaced as a readability note, not an auto-rewrite
- [ ] Proof-by-induction framing attached to recursive functions that already have a detected base case + inductive step (Phase 8's recursion analysis already finds both) — an explanatory note, not a generated proof

### 🔴 Phase 16 — Formal Language Intelligence *(completes the Chomsky hierarchy Phase 8/12 already started)*

Phase 8/12 already ships two tiers of this: regex → Chomsky Type-3 (Regular), grammar/parser-shaped code → Chomsky Type-2 (Context-Free). This phase completes the hierarchy as an educational and classification reference:

- [ ] **Type-1 (Context-Sensitive) / Linear-Bounded Automaton** and **Type-0 (Recursively Enumerable) / Turing Machine** — the remaining two tiers, added where Sythrall can find a concrete AST signal for them, documented honestly where it can't yet
- [ ] **Chomsky Hierarchy reference panel** — each grammar type paired with the automaton that recognizes it (Regular → Finite Automaton, Context-Free → Pushdown Automaton, Context-Sensitive → Linear-Bounded Automaton, Type-0 → Turing Machine), linked from wherever a regex/grammar classification already fires today

### 🔬 Phase 17 — Compiler Intelligence *(integrate mature tools, don't rebuild them — still a research spike, not committed)*

Lambda-Calculus framing already exists on tail-call recursion (Phase 8) — this phase is where that theory connects to an actual compiler pipeline, not a Sythrall-authored compiler:

- [ ] Compiler pipeline visualization (Lexer → AST → IR → optimization → codegen) — embed [Compiler Explorer](https://godbolt.org) (open source) instead of building a teaching compiler from scratch
- [ ] IR-level view of the tail-call rewrite Phase 8 already explains in prose — show what a tail-recursive function's IR looks like reduced to a loop, instead of only asserting that it "could" be

### 🟡 Phase 18 — Native Intelligence *(grow Rust's role past `complexity-engine`, same benchmark-first rule every time)*

`complexity-engine` (Phase 11) is the proof of concept, not a one-off: a Rust sidecar earns its place only when there's a measured CPU/memory bottleneck a benchmark actually confirms — never assumed, never "Python is slow so let's rewrite it." First slice done, following that exact process:

- [x] **Rich Python analysis, ported to `complexity-engine`** — `_parse_python()`'s per-function/class/import work (Big-O, Θ/Ω, cyclomatic complexity, tail-call recursion, the 3 CS Engine classifiers from Phase 12) now also runs in Rust, exposed as `POST /parse/python`, byte-for-byte parity-tested against the Python implementation on the same file (only the reason-text language differs — English in Rust, Spanish in Python, by design). Benchmarked with Criterion vs. real `_parse_python()` timing on the same synthetic files: 10 functions — 0.48ms vs 9.94ms (20.6×); 100 — 5.95ms vs 100.7ms (16.9×); 1000 — 187ms vs 1038ms (5.6×) — the margin narrows at scale, reported as measured rather than rounded up. Wired into `/static/bigO` only (a genuine subset endpoint — reads nothing else), live-checked per request with no cached-flag gate, same lesson as Phase 11's startup-race fix. **Deliberately not wired into `/static/parse`**: that endpoint returns the full legacy shape (`dead_code`, `call_graph`, `circular_deps`, `wasm_hints`, `exports`) to the frontend, none of which this slice computes yet — swapping it in would silently drop fields the Static panel renders.
- [ ] **Graph Engine** — import/call graph construction and traversal for very large projects, currently pure Python in `graph.py` (the natural home for Phase 14's graph-algorithm work once a project is large enough that Python traversal is the actual bottleneck — not before)
- [ ] **Dependency Engine** — circular-dependency detection and cross-file resolution at project scale (the natural next slice — `/static/parse` needs exactly this plus WASM hints and dead-code detection before it can move off the Python path too)
- [ ] **Symbol Engine** — go-to-definition / find-references over large codebases, currently regex/AST-based per-file
- [ ] **Project Scanner** — the file-walking + parsing fan-out for whole-project analysis (`read_project_files` and friends)

`static_parser.py` is not going away — Python and Rust run side by side, and nothing above is committed as a full rewrite. Each item is the list of "next place to look," following the exact process Phase 10 and Phase 11 already validated twice (Phase 10: looked, found ordinary Python bugs, fixed in Python; Phase 11: looked, found a real 9–21× case, adopted Rust).

### 🔬 Phase 19 — Machine Intelligence *(Assembly side of the language-philosophy table — integrate mature tools, don't rebuild them)*

Analysis of what code *becomes*, not just what it says. Each of these is its own serious project already solved well by dedicated open-source tools — the honest move is to embed those, not reinvent them:

- [ ] **Assembly (x86-64)** analysis-target support — instruction/register/control-flow breakdown from raw `.s`/inline-asm snippets pasted by the user *(pattern-matching on text, not a disassembler)*
- [ ] Executable analyzer (PE / ELF / Mach-O, sections, imports/exports, symbols) — wrap [Capstone](https://www.capstone-engine.org)/[LIEF](https://lief-project.github.io)/`objdump`, don't hand-write a disassembler
- [ ] Calling-convention and stack-frame explainers tied to the Assembly view once it exists — connects the theory (why a function prologue looks the way it does) to the actual bytes, closing the loop from Phase 17's IR view down to real machine code

### 🔴 Phase 20 — Scientific Intelligence *(Fortran, deepened past a single bullet point)*

Fortran as an analysis target, connected to the numeric stack Sythrall already ships with (numpy/scipy's compiled LAPACK/BLAS backends) — not a language Sythrall's own engine needs to be written in:

- [ ] DO-loop / array-operation detection, vectorization & SIMD candidates
- [ ] Numerical algorithm recognition (matrix ops, decompositions) with domain-specific framing, e.g. *"Matrix multiplication — O(n³), candidates: SIMD, blocking, parallelization — domain: HPC/Numerical Computing"* instead of a bare Big-O label
- [ ] BLAS/LAPACK usage detection — flag where a project is already leaning on compiled numeric backends vs. reimplementing something they already provide

### 🔴 Phase 21 — Execution Intelligence *(runtime instrumentation — a different kind of tool than everything above)*

Everything in Phases 1–20 is static analysis: source text in, facts out, no execution required. This phase is architecturally different — it needs a running process, ptrace/eBPF, or live packet capture, which is why it stayed an uncommitted "long-term" idea bucket for a long time. Numbered here to be honest that it's a real destination, not to claim it's close:

- [ ] Memory visualizer (stack/heap/data/bss) — requires a running process to inspect, not source text
- [ ] Concurrency analyzer (races, deadlocks, mutex/atomic misuse) — needs real execution or tools like ThreadSanitizer, not AST inspection
- [ ] OS engine (threads, paging, scheduling, IPC) — needs kernel-level tracing
- [ ] Network analyzer (TCP/TLS/QUIC/WebSocket) — needs packet capture; this is a Wireshark-shaped tool, not a static analyzer
- [ ] Security analyzer beyond pattern detection (ROP, heap spray, use-after-free exploitation) — competes directly with mature SAST tools (Semgrep, CodeQL, Bandit); the realistic version folds into the CS Engine above as "detect the pattern + explain the CWE," not a full exploit-analysis engine

### 🔴 Phase 22 — Sythrall Platform *(how the product reaches people, once the CS Engine has something to ship)*

Everything here is orthogonal to the theory phases above — engineering/distribution work that doesn't depend on Phases 13–21 landing first, closing the roadmap with how Sythrall gets used rather than what it knows:

- [ ] **Native Toolchain (Zig)** — standalone desktop build (Zig, or PyInstaller/Nuitka + Tauri), a portable binary with no Docker/Node/Python required; cross-compilation for the native binaries this project already ships (`terminal-server`, `complexity-engine`), one toolchain instead of per-platform CI matrices *(deliberately not competing with Rust's role in Phase 18 — Zig's job is getting Sythrall onto a machine, not analyzing what's on it)*
- [ ] **Cython & WASM integration** — auto-detect Cython candidates from Big-O analysis (O(n²)+ functions), generate `.pyx` stubs from Python function signatures, compile in Docker (MSVC/GCC), side-by-side Python-vs-Cython benchmark display, estimated speedup in the hover provider, WASM compilation path via Emscripten
- [ ] **Execution Path Simulator** — animated circuit-board view of Sythrall's own analysis pipeline (`Input → Parser → AST → Dependency Resolver → Metrics → Report`), step-by-step trace with timing per stage, exportable as animated SVG
- [ ] **Enterprise persistence** — PostgreSQL + Delta Lake, analysis history, metric comparison between versions, JWT authentication, public API with rate limiting
- [ ] VS Code extension, LSP server (the natural client for Phases 13–19's facts once there's a standard protocol serving them), Jupyter Notebook analysis (`.ipynb`), ApexVision integration (`/analyze/image` with OpenCV + YOLOv11), team dashboard with aggregated metrics
- [x] GitHub Action for CI/CD — `.github/workflows/ci.yml` (typecheck/lint/build/test on every push/PR) + `release.yml` (tag → GitHub Release with CHANGELOG notes + frontend build artifact)

### 🔬 Research spikes — leftover ideas, not committed

- [x] ~~Rust extension (PyO3) for the static parser's hottest path~~ — **investigated twice with real benchmarks, not adopted either time**: first pass profiled `static_parser.py` on large *individual* files (250+ functions, 3000+ synthetic) and found the AST-walk consolidation already done was noise-level neutral (~160ms for realistic file sizes). Second pass (Phase 10, above) tested the other axis — thousands of *files* in one project, not one huge file — and found the parser itself still scaled linearly; the actual O(n²) cost was three ordinary Python bugs in the code *around* the parser, fixed without any new language. Rust *did* end up in the project (Phase 9's `terminal-server`), but for a genuinely Rust-shaped problem — cross-platform PTY handling — not as a speed rewrite of working Python. If a real bottleneck ever shows up in `parse_file` itself, the integration model would be an Axum sidecar (same pattern as `terminal-server`) talking HTTP to FastAPI, not PyO3 embedding — simpler to maintain solo, no native-binding build matrix.

---

## 📝 Changelog

See [CHANGELOG.md](CHANGELOG.md) for the full version history.

---

## 👤 Author

**Brashkie** · [Hepein Oficial](https://github.com/Brashkie) · Lima, Perú

---

## 📄 License

GPL-3.0 — see [LICENSE](LICENSE)
