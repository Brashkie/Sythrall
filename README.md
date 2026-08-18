# 🛰 Sythrall

> **Enterprise-grade code intelligence platform** — static analysis, ML/DL inspection, real-time editor intelligence, code graph visualization, an integrated terminal, and API monitoring. Built with TypeScript (no frameworks) + FastAPI + Rust + Monaco Editor.

<div align="center">

![Version](https://img.shields.io/badge/version-4.8.0-blue?style=flat-square)
[![CI](https://github.com/Brashkie/Sythrall/actions/workflows/ci.yml/badge.svg)](https://github.com/Brashkie/Sythrall/actions/workflows/ci.yml)
![Frontend](https://img.shields.io/badge/Frontend-Vite%20%2B%20TypeScript-646cff?style=flat-square)
![Backend](https://img.shields.io/badge/Backend-FastAPI%20%2B%20Python-009688?style=flat-square)
![Terminal](https://img.shields.io/badge/Terminal-Rust%20%2B%20axum-dea584?style=flat-square)
![Deploy](https://img.shields.io/badge/Deploy-Docker%20%2B%20Nginx-2496ed?style=flat-square)
![Tests](https://img.shields.io/badge/Tests-373%20passing-00f5a0?style=flat-square)
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
| **🕸 Code Graph Visual** | Import graph, Call graph, Circular dependency detection — Mermaid Tree View, scoped to loose files or a whole uploaded project *(the interactive Force Graph and the Complexity Heatmap dir-tree renderers both exist and are tested, but neither is wired to a UI control yet)* |
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
│   │   ├── main.py                 ← FastAPI v4.8 (35+ routes)
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
| Intelligence & Science | **Python** | AI/ML detection, orchestration, scientific workloads — **not** the static-analysis engine long-term: that role is being retired to Rust (Phase 18's Native Analysis Core), one measured slice at a time | 23 library detections, ML/DL Inspector (Phase 2). `static_parser.py` itself is the legacy piece Phase 18 is migrating out of Python, not a permanent fixture |
| Native Analysis | **Rust** | The static-analysis engine, committed destination — parsing, AST, symbol resolution, complexity, security, quality, graphs. Each slice is still profiled/benchmarked before the swap (that's *how*, not *whether*) | `terminal-server` (PTY handling), `complexity-engine` (9–21× faster than `radon` — Phase 11), rich Python analysis ported (Phase 18) |
| Scientific/HPC | **Fortran** | Analysis *target*, not an implementation language — Sythrall already gets Fortran-level numeric performance for free via numpy/scipy's compiled LAPACK/BLAS | Planned (Phase 20) |
| Machine-level | **Assembly** | Analysis target for instruction/register/control-flow — wraps Capstone/LIEF instead of hand-rolling a disassembler | Planned (Phase 19) |
| Native tooling | **Zig** | Build, cross-compilation, standalone distribution — a different concern from Rust's analysis-engine role, not competing with it | Planned (Phase 25) |

The rule for moving anything into Rust (or any native language) used to be a real
either/or: profile first, benchmark the replacement, keep it only if the numbers
justify it — Phase 10's large-project investigation found the real O(n²) cost was
three ordinary Python bugs, not the parser, and fixed it without a new language;
Phase 11's `complexity-engine` found a real, measured 9–21× win and adopted Rust.
Both outcomes came from the same process; neither was assumed going in.

For the static-analysis engine specifically, that question is now settled: it's
moving to Rust, fully, not just wherever a benchmark happens to favor it — see
Phase 18's Native Analysis Core below. What survives from the old rule is the
*method*, not the *decision*: each slice is still ported, parity-tested against
the Python it replaces, and benchmarked before the call sites switch over, so the
migration never trades correctness for the sake of hitting the target faster. The
either/or framing still applies to any *other* future native-language adoption
outside this specific migration — this is a one-time, explicit exception, not a
reversal of the general rule.

### Where this is going: Computer Science Intelligence, not a linter

The honest one-line description of Sythrall today is "reads code, computes Big-O."
Phases 13–23 below are the plan for growing that into something more specific:
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
- [x] Import Graph · Call Graph · Circular Dependencies
- [x] Sub-phase A: graphs from sidebar; Sub-phase B: graphs from uploaded ZIP projects
- [x] Cross-folder dependency resolution, NetworkX circular detection
- [x] Interactive Force Graph *and* Complexity Heatmap dir-tree (`renderForceGraph`/`renderDirTree` in `panels/graph.ts`, custom physics engine, no D3) *(both implemented and tested — a later audit found the whole graph module had zero callers anywhere in the app; Mermaid Tree View got wired in Phase 10 below, but `generateWholeProjectDiagram` in `app.ts` still passes both `onForce`/`onDirTree` as no-ops (documented in its own docstring) — neither has a UI trigger yet)*
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
- [x] **Problems panel** placement — still needs a decision (see Phase 7 note above) before it can be wired in without clobbering the existing file-analysis view. *(resolved in Phase 12 below — own right-panel sub-tab instead of sharing a container)*

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

The phases below are the 11 conceptual pillars of "Computer Science Intelligence" (see above) plus two closing phases that sit outside that theory — one architectural, one productization — grouped by how well-founded they are, not by how far in the future they sit. Phases 13–14 are the most direct continuation of what Phase 8/12 already shipped; 15–17 are genuinely new theoretical ground; 18–20 build out the language-philosophy table; 21–22 fill a gap the original 9-pillar pass missed entirely (security, code quality) — same heuristic, confidence-scored style as everything else here, not a new methodology; 23 is architecturally different from everything above it (needs a running process, not source text); 24–25 close the loop — 24 is the API boundary that would let Phases 13–23 grow as plugins instead of every analyzer landing directly in `apps/api`, 25 is how the product actually reaches people once there's something to ship.

### 🟡 Phase 13 — Algorithmic Intelligence *(extends Phase 8's O/Θ/Ω engine, not a new architecture)*

Phase 8 already computes worst-case O, best-case Ω, and tight-bound Θ per function — a specific, standard application of asymptotic notation in algorithm analysis (matching the convention used by CLRS and most algorithms texts). Worth making explicit rather than implicit, since the general definitions are broader than that one use:

- [x] **Asymptotic notation reference**, surfaced wherever a Big-O result already appears: a collapsible `<details>` reference table above the Static panel's Big-O table (`panels/static.ts::_renderBigOTable`) covers all five symbols — O (upper bound), Ω (lower bound), Θ (tight bound), o (strict upper bound), ω (strict lower bound) — general definition first, honest that `o`/`ω` aren't computed (no reliable static heuristic distinguishes "strict" from the tight case). The Python hover tooltip (`_build_hover_python`) gets the same explanation as a one-line footnote under its O/Θ/Ω table. JS/TS hover intentionally untouched — it only computes O, not Θ/Ω, so the fuller legend doesn't apply there yet.
- [x] **Space complexity** alongside time complexity, built in Rust and Python from the same pass this time (`services/complexity/src/space.rs` + `static_parser.py::_infer_space_python`, both wired the same day) — same heuristic AST-based approach as the time engine (no execution), but the signal is *what auxiliary structure gets built*, not *how many times a loop runs*: a loop that only accumulates into a scalar (`total += x`) is O(1) space even though it's O(n) time. Detects growing collections (`.append`/`.add`/`.update`/`.insert`/`.extend`, or `d[k] = v` subscript-assignment) at loop-nesting depth (O(n) at depth 1, O(n²) at depth 2 — a matrix built in nested loops), nested comprehensions (`[[0 for _ in range(n)] for _ in range(n)]`), and recursion-stack depth (O(log n) for binary-split recursion reusing `bigo.rs`'s existing split detector, O(n) for linear recursion, since Python doesn't optimize tail calls). 8 Rust unit tests + 8 Python tests (`TestSpaceComplexity`), byte-for-byte parity confirmed on all 8 cases by hand. Wired into `/intel/hover` and `/intel/analyze` (Rust-first, Python fallback, same gate as Big-O) and a new **Space** column in the Static panel's Big-O table. Benchmarked with Criterion vs. real `_parse_python()` timing on the same synthetic files: 10 functions — 0.77ms vs 18.0ms (23.4×); 100 — 9.17ms vs 212.6ms (23.2×); 1000 — 254ms vs 2083ms (8.2×) — and reported honestly that adding this pass slowed `analyze_rich()` itself by 30–59% against its own prior baseline (two more full-body walks per function stack up), not just the win against Python.
- [ ] **Recurrence relation recognition** for divide-and-conquer functions — e.g. `merge_sort`'s `T(n) = 2T(n/2) + Θ(n)` pattern-matched to the Master Theorem's three cases instead of falling back to the generic loop/recursion heuristic already in place, landing on `Θ(n log n)` with the recurrence shown, not just the answer

### 🟡 Phase 14 — Data Structures & Graph Intelligence *(same heuristic-pattern style as the existing WASM-hint/dead-code detectors)*

Direct continuation of the CS Engine (Phase 8/12) — same static-analysis approach, no new architecture. Not just naming the structure, explaining it:

- [ ] Detect AVL / Red-Black Tree / Trie / Heap / Segment Tree / Fenwick Tree / Bloom Filter / B-Tree / HashMap / Skip List from AST shape
- [ ] For each match: complexity (time *and* space), typical operations, use cases, tradeoffs vs. the alternatives — the same "why," not just "what," standard the Big-O engine already holds itself to
- [x] **Graph algorithms as first-class operations, first slice: centrality/hub detection** (`routers/graph.py::_build_centrality_graph`) — new `centrality` graph type reusing the import graph already built for the other 4 (Import/Call/Circular/Heatmap), scored with `nx.degree_centrality`/`in_degree`/`out_degree` on the same NetworkX `DiGraph` the circular-dependency detector already builds — not a new library, the same one already earning its keep. A file is flagged as a "hub" when it's in the top 5 by in-degree *and* has at least 2 dependents (avoids labeling a file with a single import as a hub). Wired into the Diagram panel's project-graph dropdown (`index.html`, `PROJECT_GRAPH_TYPES` in `app.ts`) — reachable by a user, not just an API response, the same "does this have a UI trigger" discipline the Force Graph gap taught this project. 8 backend tests (`TestCentralityGraph`). **BFS/DFS/shortest path/connected components/topological ordering not started yet** — this slice is the one item the roadmap text called out by name ("centrality/hub detection... surfacing the most-connected files/functions"); the rest of the graph-algorithm list is still open. Deliberately still Python (`networkx`, not Rust) — Phase 18's Graph Engine needs the import/call graph *construction* itself in Rust first, which doesn't exist yet; porting the algorithms alone before that would mean building two separate graph representations.
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

### 🟡 Phase 18 — Native Analysis Core *(committed migration: `static_parser.py`'s role moves to Rust, fully — not just wherever a benchmark happens to favor it)*

`complexity-engine` (Phase 11) was the proof of concept; this phase is the decision it justified. **The static-analysis engine is moving to Rust, completely** — parsing, AST construction, symbol resolution, complexity, security, graphs, quality metrics, all of it, progressively consolidated into a modular native core. Python's role narrows to what Phase 2's ML/DL Inspector already does well: AI, ML, and scientific workloads — not source-code parsing. The end state has **no permanent compatibility layer and no dual implementation**: once a piece is ported and proven correct, the Python version it replaced is deleted, not kept around "just in case." `static_parser.py` itself is the migration's target, not a fixture that survives it.

What doesn't change: *how* each slice moves is still disciplined — ported, parity-tested against the Python it replaces on the same input, benchmarked with Criterion, and only then swapped into the live call sites, exactly the process the first slice below already used. The commitment is to the destination; the rigor is in how each step gets there, so migrating fast never means migrating carelessly.

- [x] **Rich Python analysis, ported to `complexity-engine`** — `_parse_python()`'s per-function/class/import work (Big-O, Θ/Ω, cyclomatic complexity, tail-call recursion, the 3 CS Engine classifiers from Phase 12) now also runs in Rust, exposed as `POST /parse/python`, byte-for-byte parity-tested against the Python implementation on the same file (only the reason-text language differs — English in Rust, Spanish in Python, by design). Benchmarked with Criterion vs. real `_parse_python()` timing on the same synthetic files: 10 functions — 0.48ms vs 9.94ms (20.6×); 100 — 5.95ms vs 100.7ms (16.9×); 1000 — 187ms vs 1038ms (5.6×) — the margin narrows at scale, reported as measured rather than rounded up. Wired into `/static/bigO`, `/intel/analyze` (the ~2s-idle "heavy path" while typing) and `/intel/hover` (only the CS-Engine classification portion — hover's signature text stays AST-based in Python since Rust's `RichFunction` only carries plain argument names, not type annotations), each live-checked per request with no cached-flag gate, same lesson as Phase 11's startup-race fix. **Not yet wired into `/static/parse`**: that endpoint returns the full legacy shape (`dead_code`, `call_graph`, `circular_deps`, `wasm_hints`, `exports`) to the frontend, none of which this slice computes yet — the next four bullets are exactly what's missing before it can move over.
- [ ] **Graph Engine** — import/call graph construction and traversal for large projects, currently pure Python in `graph.py`; this is the piece `/static/parse` and Phase 14's graph-algorithm work both need in Rust before either can leave Python behind
- [ ] **Dependency Engine** — circular-dependency detection and cross-file resolution at project scale — the other half `/static/parse` needs, alongside WASM hints and dead-code detection, to fully retire its Python path
- [ ] **Symbol Engine** — go-to-definition / find-references over large codebases, currently regex/AST-based per-file
- [ ] **Project Scanner** — the file-walking + parsing fan-out for whole-project analysis (`read_project_files` and friends)
- [x] **Security, first slice ported** — `security.rs`: taint tracking + SQL/Command Injection + hardcoded credentials, ported the same day the Python version shipped (Phase 21), same benchmark-then-swap discipline as every other row here. Path Traversal/Insecure Deserialization (still Python-only) and Code Quality (Phase 22, not started in either language yet) remain queued — the "write in Python first" step is still legitimate for a genuinely new heuristic that's still iterating on false positives, it's just not the default resting place anymore
- [ ] **`static_parser.py` deleted** — the actual finish line: once every endpoint that reads it today (`/static/parse`, `/static/bigO`, `/intel/*`, `graph.py`) reads from the Rust core instead, the Python file is removed, not deprecated-and-kept

```
                    Sythrall
                       │
                 Native Analysis Core (Rust)
                       │
       ┌───────────────┼────────────────┐
       │               │                │
    Parsing          Analysis         Graph
   AST/symbols    Complexity/Security  CFG/DFG
                       │
              Deep Analysis
       ┌───────────────┼───────────────┐
       │               │               │
    Security        Quality       Performance
```

One thing this is deliberately *not*: `static_parser.py` → one giant `static_parser.rs`. The modular split above — separate Rust modules per concern, the same pattern `bigo.rs`/`classifiers.rs`/`recursion.rs`/`structure.rs` already use inside `rich.rs` today — is what each bullet above ports into, not a single monolithic port. And once Phase 24 (Extensibility Platform) exists, this core doesn't have to hold everything forever either — CWE catalogs, additional rules, new language support, and AI-explanation models are exactly the kind of thing that belongs as a plugin *on top* of these primitives instead of hardcoded permanently into the Rust core itself.

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

### 🟡 Phase 21 — Security & Taint Intelligence *(pattern-based data-flow analysis, confidence-scored — not a SAST replacement)*

A gap the original 9-pillar pass missed entirely: nothing in Phases 13–20 looks at security. Direct continuation of the CS Engine's existing style — Phase 8/12's regex/grammar classifiers already prove "heuristic pattern + honestly-labeled confidence" works — applied to source→sink data flow instead of algorithmic shape. Nothing here should ever be presented as "this IS a vulnerability," only as a pattern worth a human's attention, with the evidence shown so the claim is auditable. First slice done:

- [x] **Taint tracking within a single function** — a recursive, provenance-carrying walk resolves a value back to an untrusted source (`request.args`/`form`/`values`/`json`/`GET`/`POST`/`COOKIES`/`headers`, `input()`, `sys.argv`, `os.environ`/`os.getenv`) through assignments, string concatenation, f-strings, and `.format()`, tracking *whether* the taint was built via string construction (the actual SQLi/command-injection signal) vs. passed through raw. **Deliberately not cross-function** — interprocedural taint needs the call graph the Dependency Engine below doesn't build yet. **Ported to Rust the same day it shipped in Python** (`services/complexity/src/security.rs`), not left as a someday-item — see the Native Analysis Core note below for why. 19 Python unit tests (`test_security_findings.py`) + 9 Rust unit tests (`security::tests`), both covering the vulnerable and the safe/parameterized form of every check, byte-for-byte parity between the two confirmed by hand on the same inputs.
- [x] **CWE catalog v1, 3 of 5 shipped**: SQL Injection (CWE-89) — fires only when the query is *built* via concatenation/f-string/`.format()`, so `cursor.execute("...%s...", (val,))` correctly produces no finding; Command Injection (CWE-78) — `os.system`/`os.popen` (always shell) or `subprocess.*(..., shell=True)`, `subprocess.run([...])` without `shell=True` correctly produces no finding; hardcoded credentials (CWE-798) — name+shape heuristic (credential-looking variable name assigned a non-placeholder string literal), file-level rather than per-function since that's genuinely where these live. **Path Traversal (CWE-22) and Insecure Deserialization (CWE-502) not started** — deferred, not silently dropped
- [x] **Confidence-scored findings** (currently High/Medium, no Low case fires yet), never a bare yes/no — mirrors the "both signals required" precedent from Phase 12's grammar classifier: a source alone isn't a finding, source *and* the concatenation/sink signal both firing is
- [x] **Evidence-tree finding schema** — category → CWE → severity → confidence → source → sink → line → recommendation, one shared shape for every finding. Surfaced in the **Static panel** (`panels/static.ts::_renderSecurityFindings`, a new section above the Big-O table) and in the Python **hover tooltip** (`_build_hover_python`, Rust-first via `parse_python_rich` with the same per-function Python fallback every other classifier here uses) — **not yet the Problems Panel**: that tab is fed by per-keystroke lint markers, and security findings currently run on the same heavier `/static/parse` pass as WASM hints, which live in Static too, not Problems
- [x] **Two real bugs found and fixed the same week this shipped**, both via an independent audit, not self-caught: (1) reassigning a tainted variable to a safe literal (`cmd = request.args.get("x"); cmd = "ls -la"`) didn't clear its taint, producing a High-confidence false positive on ordinary defensive code; (2) a nested function reusing an outer variable's name leaked taint across scopes. Both fixed by making the taint walk scope-aware (`_own_scope_nodes` in Python, `walk_stmts_own_scope` in Rust — a generic AST walker variant that stops at nested `FunctionDef`/`Lambda` boundaries instead of descending into them), with regression tests in both languages.
- [ ] Explicitly out of scope, permanently: full exploit analysis (ROP, heap spray, use-after-free) — already correctly fenced off under Execution Intelligence (Phase 23 below) as "competes directly with mature SAST tools"; this phase stays inside that same boundary, just static and narrower

**Native Analysis Core, applied for real**: this is the first Phase 18 slice ported to Rust the same week it was written in Python, not queued for "later" — a direct response to actually doing the migration instead of just planning it. Benchmarked with Criterion vs. real Python timing (`_parse_python()`, now also running the same security pass) on the same synthetic files used for every prior benchmark in this project: 10 functions — 0.52ms vs 11.97ms (23×); 100 — 6.37ms vs 129.0ms (20.3×); 1000 — 191.7ms vs 1316.9ms (6.9×). Adding the taint pass measurably slowed `analyze_rich()` itself (Criterion flagged a 2.7–8% regression against its own prior baseline) — reported honestly rather than only quoting the win against Python.

### 🔴 Phase 22 — Code Quality Intelligence *(the Maintainability Index Phase 11 already ships, broken into its auditable parts, plus the smells one MI number can't express)*

`complexity-engine`'s Maintainability Index (Phase 11) already computes a Halstead Volume internally (`maintainability.rs::Halstead::volume()`) but only exposes it pre-baked into one formula. This phase surfaces the components that formula is built from, and adds the structural/naming/architecture smells a single MI number can't catch on its own:

- [ ] **Halstead metrics, broken out**: Vocabulary (η1+η2), Length (N1+N2), Volume (already computed for MI), Difficulty, Effort — their own table next to the existing MI score instead of staying an opaque input to one formula
- [ ] **Structural smells**: long function, large class, deep nesting, excessive parameters, duplicated logic (AST-shape comparison, not textual diff), god object — same "why, not just what" standard the Big-O engine already holds itself to, each smell shipping its threshold and reasoning, not a bare label
- [ ] **Naming smells**: single-letter names outside tight-loop scope, inconsistent casing within one file, shadowing an outer-scope name — intentionally conservative, flags only mechanically-checkable cases rather than "unclear name" judgment calls that would need an LLM to arbitrate
- [ ] **Architecture smells**: coupling/cohesion per module, built on the import graph that already exists (Phase 6/14) — afferent/efferent coupling counts, instability, the already-shipped circular-dependency detector reframed as one instance of a general layer-violation check
- [ ] **Quality dashboard** — a rollup view (Security/Quality/Performance/Architecture) pulling from Phases 21/22/13/14, not a new scoring model invented just for the dashboard

### 🔴 Phase 23 — Execution Intelligence *(runtime instrumentation — a different kind of tool than everything above)*

Everything in Phases 1–22 is static analysis: source text in, facts out, no execution required. This phase is architecturally different — it needs a running process, ptrace/eBPF, or live packet capture, which is why it stayed an uncommitted "long-term" idea bucket for a long time. Numbered here to be honest that it's a real destination, not to claim it's close:

- [ ] Memory visualizer (stack/heap/data/bss) — requires a running process to inspect, not source text
- [ ] Concurrency analyzer (races, deadlocks, mutex/atomic misuse) — needs real execution or tools like ThreadSanitizer, not AST inspection
- [ ] OS engine (threads, paging, scheduling, IPC) — needs kernel-level tracing
- [ ] Network analyzer (TCP/TLS/QUIC/WebSocket) — needs packet capture; this is a Wireshark-shaped tool, not a static analyzer
- [ ] Security analyzer beyond pattern detection (ROP, heap spray, use-after-free exploitation) — competes directly with mature SAST tools (Semgrep, CodeQL, Bandit); the realistic version folds into Phase 21 above as "detect the pattern + explain the CWE," not a full exploit-analysis engine

### 🔴 Phase 24 — Extensibility Platform *(the API boundary that lets Phases 13–23 grow without every feature landing in `apps/api` — an internal tool, not a public marketplace, until real third-party demand shows up)*

Every phase above adds directly to Sythrall's own codebase — reasonable while one person maintains it, but each of Phases 13–23 is realistically a plugin's worth of scope on its own. This phase is the boundary that would let that work happen outside the core without every analyzer becoming a `routers/` file Sythrall itself has to own forever. Scoped deliberately small for a first slice — no marketplace, no sandboxing, no third-party trust model — because none of that has a reason to exist before a second real plugin, beyond the ones Sythrall ships itself, actually needs it:

- [ ] **Plugin manifest + capability interface** — a plugin declares what it analyzes (`language`, `security`, `performance`, ...) and what it needs (`ast`, `metrics`, `source`) in a small typed manifest; Sythrall's own Python/JS/TS parsers become the first "built-in" implementations of that same interface, proving the boundary is real before any third party touches it
- [ ] **One plugin type shipped end-to-end** — the concrete test of whether the interface is actually usable, not a second parallel system built alongside it. Phase 20's Fortran work is the natural candidate: numeric/scientific analysis as a "language plugin" instead of another branch hardcoded into `static_parser.py`
- [ ] **Extension vs. plugin split**, matching a distinction already implicit in how `apps/web` is organized today: a *plugin* adds an analyzer (new finding types, new language, new rule) and only needs the capability interface above; an *extension* adds UI (a new panel, the way Phase 12's Problems tab already is one) and consumes a plugin's output over the same JSON shape every panel already reads — no new architecture for extensions, just a documented name for a seam that already exists informally
- [ ] **AI as an optional explanation layer, never the detector** — an `AIProvider` interface (local ONNX/GGUF, remote API, or none configured) that a plugin can call to turn `Evidence` (Phase 21's data-flow path, Phase 13's Big-O reasoning) into prose, with the deterministic finding produced and fully usable with zero AI configured — the same shape the Big-O `reason` field already has today, just optionally handed to a model instead of only template strings
- [ ] Explicitly deferred, no timeline: public registry/marketplace, sandboxed/WASM third-party execution, a multi-language plugin SDK beyond what Sythrall's own core already uses — real infrastructure commitments that only make sense once plugins built *for* Sythrall by someone other than its author actually exist to justify them

The one rule that survives contact with everything above: `apps/api` keeps working with zero plugins installed — always has, this phase only adds a documented way to build on it, never a requirement to.

### 🔴 Phase 25 — Sythrall Platform *(how the product reaches people, once the CS Engine has something to ship)*

Everything here is orthogonal to the theory phases above — engineering/distribution work that doesn't depend on Phases 13–24 landing first, closing the roadmap with how Sythrall gets used rather than what it knows:

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
