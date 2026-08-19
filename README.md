<div align="center">
<img src="apps/web/public/sythrall-logo.png" alt="Sythrall" width="96" />

# Sythrall

</div>

> **Enterprise-grade code intelligence platform** — static analysis, ML/DL inspection, real-time editor intelligence, code graph visualization, an integrated terminal, and API monitoring. Built with TypeScript (no frameworks) + FastAPI + Rust + Monaco Editor.

<div align="center">

![Version](https://img.shields.io/badge/version-4.9.0-blue?style=flat-square)
[![CI](https://github.com/Brashkie/Sythrall/actions/workflows/ci.yml/badge.svg)](https://github.com/Brashkie/Sythrall/actions/workflows/ci.yml)
![Frontend](https://img.shields.io/badge/Frontend-Vite%20%2B%20TypeScript-646cff?style=flat-square)
![Backend](https://img.shields.io/badge/Backend-FastAPI%20%2B%20Python-009688?style=flat-square)
![Terminal](https://img.shields.io/badge/Terminal-Rust%20%2B%20axum-dea584?style=flat-square)
![Deploy](https://img.shields.io/badge/Deploy-Docker%20%2B%20Nginx-2496ed?style=flat-square)
![Tests](https://img.shields.io/badge/Tests-405%20passing-00f5a0?style=flat-square)
![Author](https://img.shields.io/badge/Author-Hepein%20Oficial-b87dff?style=flat-square)
![License](https://img.shields.io/badge/License-GPL%203.0-orange?style=flat-square)

[English](./README.md) · [Español](./README.es.md) · [Roadmap](./ROADMAP.md) · [Changelog](./CHANGELOG.md)

</div>

---

## What is Sythrall?

Sythrall is a professional code intelligence platform built as both an educational project and a production-grade demonstration. It combines a modern TypeScript frontend (zero runtime frameworks), a Python/FastAPI backend with full ML/DL support, a multi-language static analysis engine, and a Monaco Editor integration that rivals VS Code's developer experience.

### Core capabilities

| Module | Description |
|---|---|
| **Project Explorer** | Expandable file tree, multi-file tabs, global search (`Ctrl+Shift+F`), symbol outline |
| **Folder Browser** | Open a real folder from disk ("+ Folder") and browse it as a VSCode-style expandable tree — works in every modern browser, no Chromium-only APIs. Creates or appends to the active project, so it's not a one-off local-only view |
| **Integrated Terminal** | Real interactive shell (PowerShell/bash) in a resizable bottom panel, powered by a Rust sidecar (`portable-pty` + `axum`) — token-protected, zero-friction for local use |
| **Editor Intelligence** | Real-time linting, inline diagnostics, Big-O hover, Go to Definition, Find References, Rename Symbol, semantic autocomplete |
| **Static Analysis** | AST-based multi-language parser (Python, TypeScript, C/C++) — Big-O estimation, cyclomatic complexity, security findings (CWE catalog), structural smells, WASM/Cython hints, call graph, dead code |
| **Project Health** | 4 aggregated scores (Security, Quality, Complexity, Architecture) computed across an entire project, each with its formula and raw numbers shown — never a bare number |
| **Code Graph Visual** | Import graph, Call graph, Circular dependency detection, Centrality/hub detection — Mermaid Tree View, scoped to loose files or a whole uploaded project *(the interactive Force Graph and the Complexity Heatmap dir-tree renderers both exist and are tested, but neither is wired to a UI control yet)* |
| **Projects** | Upload files, a folder or a ZIP — the result can become the **active project**, which Editor · Issues · Diagram · Static · Metrics then read directly, without re-uploading anything per panel |
| **Analysis** | pylint · flake8 · AST · `complexity-engine` (Rust) — issues, complexity, maintainability index, Halstead metrics |
| **ML/DL** | Detection of 23 libraries, 23 pipeline patterns, 25 models, 20+ issue rules |
| **Diagrams** | Flowchart · Callgraph · Classes · Sequence — generated with Mermaid.js + zoom/pan |
| **APIs** | External endpoint verification with history and response metrics |
| **Dashboard** | Project Health scores, distribution charts, response times, execution history |
| **Diff** | Visual file comparison with highlighted changes |
| **Logs** | Real-time server log stream — also available as a switchable view inside the terminal panel |
| **Light/Dark theme** | Toggle in the topbar, dark by default, persisted across sessions |

---

## Project structure

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
│   │   ├── main.py                 ← FastAPI (35+ routes)
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
│   │   └── tests/                  ← 405 tests total (see Tests section below)
│   └── web/                        ← TypeScript frontend (Vite, zero frameworks)
│       ├── index.html
│       ├── Dockerfile.frontend
│       └── src/
│           ├── api/client.ts          ← Full API client
│           ├── components/
│           │   ├── app.ts             ← App shell + file management
│           │   ├── editor.ts          ← Monaco Editor integration
│           │   ├── editor-intelligence.ts ← Linting + hover + autocomplete
│           │   ├── explorer.ts        ← Project Explorer (tree + tabs + search + outline)
│           │   ├── file-browser.ts    ← Folder tree from <input webkitdirectory>, cross-browser
│           │   ├── terminal.ts        ← xterm.js client for the Rust terminal sidecar
│           │   ├── events.ts          ← Global event wiring
│           │   ├── charts.ts          ← Chart.js integration
│           │   ├── mermaid.ts         ← Mermaid + zoom/pan engine
│           │   └── flow.ts            ← Execution flow diagram
│           ├── panels/
│           │   ├── dashboard.ts       ← Project Health scores
│           │   ├── analysis.ts        ← Issues/Metrics rendering (+ active-project mode)
│           │   ├── apis.ts
│           │   ├── ml.ts
│           │   ├── upload.ts          ← Projects hub: upload, recent list, active project
│           │   ├── static.ts          ← Static Analysis panel (+ active-project mode)
│           │   ├── problems.ts        ← Live Metrics Bar + Session Restore + Problems Panel
│           │   └── graph.ts           ← Code Graph Visual — Mermaid Tree View wired; Force Graph/Dir Tree implemented, not yet wired to UI
│           ├── store/state.ts         ← activeProjectId persists across reloads (localStorage)
│           ├── styles/
│           │   ├── main.css
│           │   ├── upload.css
│           │   ├── static-addon.css
│           │   ├── explorer.css
│           │   └── problems.css
│           ├── types/index.ts
│           └── utils/
│               ├── icons.ts           ← Inline SVG icon set + language badges — no emoji, no icon library
│               ├── health.ts          ← Project Health score cards, shared by Dashboard and Static
│               ├── file-tree.ts       ← FileList → nested tree (for file-browser.ts)
│               └── theme.ts           ← Light/dark toggle + persistence
├── services/                       ← independent Rust processes `apps/` calls over HTTP — no one launches these directly
│   ├── terminal/                   ← real interactive shell over WebSocket
│   │   ├── Dockerfile
│   │   └── src/
│   │       ├── main.rs             ← axum WS handler, token auth, PTY bridging
│   │       ├── pty_session.rs      ← portable-pty wrapper (ConPTY/Unix PTY, one impl)
│   │       └── auth.rs             ← token generation + constant-time comparison
│   └── complexity/                 ← cyclomatic complexity + MI + raw metrics (replaces radon) + rich Python analysis
│       ├── Dockerfile
│       ├── benches/
│       │   ├── complexity_bench.rs ← Criterion — measured 9-21x faster than radon
│       │   └── parse_bench.rs      ← Criterion — analyze_rich() vs. _parse_python(), 5.6-20.6x faster
│       └── src/
│           ├── main.rs             ← axum HTTP server (GET /health, POST /metrics/complexity, POST /parse/python)
│           ├── lib.rs              ← analyze()/analyze_rich() entrypoints, shared by main.rs and the benchmarks
│           ├── parser.rs           ← rustpython-parser wrapper + byte-offset→line resolution
│           ├── complexity.rs       ← McCabe cyclomatic complexity (own logic, not radon's)
│           ├── maintainability.rs  ← Maintainability Index + Halstead metrics (Coleman-Oman formula)
│           ├── raw.rs              ← loc/lloc/sloc/comments/blank/multi
│           ├── walk.rs             ← generic exhaustive AST walker (ast.walk() equivalent), shared by the modules below
│           ├── bigo.rs             ← Big-O/Θ/Ω heuristic
│           ├── space.rs            ← Space complexity heuristic
│           ├── recursion.rs        ← tail-call detection
│           ├── classifiers.rs      ← regex/grammar/graph-traversal CS Engine classifiers
│           ├── security.rs         ← taint tracking + CWE catalog (SQLi, command injection, path traversal, deserialization, hardcoded credentials)
│           ├── smells.rs           ← structural code smells (long function, god object, etc.)
│           ├── structure.rs        ← class/import extraction + AST helpers (decorators, docstrings, calls)
│           └── rich.rs             ← analyze_rich() orchestrator — same functions/classes/imports/summary shape as _parse_python()
└── README.md
```

---

## Prerequisites

| Tool | Min version | Download |
|---|---|---|
| **Docker Desktop** | 4.x | [docker.com](https://www.docker.com/products/docker-desktop) |
| **Node.js** | 20.x | [nodejs.org](https://nodejs.org) |
| **Python** | 3.11+ | [python.org](https://www.python.org) |
| **Rust** | stable | [rustup.rs](https://rustup.rs) — only needed to run the integrated terminal in dev mode; the rest of the app works without it |
| **Git** | any | [git-scm.com](https://git-scm.com) |

---

## Installation

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

The `[term]` process prints a random token on startup (`Terminal token: ...`). For normal local use you don't need it — the terminal panel auto-connects. It only matters if you ever set `TERMINAL_HOST` to something other than `127.0.0.1` (see the Terminal security note below).

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

## Service URLs

| Service | URL |
|---|---|
| **App (Docker)** | http://localhost:8080 |
| **App (dev)** | http://localhost:5173 |
| **Backend API** | http://localhost:8000 |
| **Swagger UI** | http://localhost:8000/docs |
| **Health** | http://localhost:8000/health |
| **Terminal sidecar (Rust)** | ws://127.0.0.1:7681 (proxied through `/terminal` in dev — not meant to be opened directly) |
| **Complexity sidecar (Rust)** | http://127.0.0.1:7682 (called by the backend, not the browser — not meant to be opened directly) |

### Terminal security note

The terminal sidecar binds to `127.0.0.1` by default — unlike the rest of the app (which binds `0.0.0.0`), it deliberately does **not** accept connections from other machines out of the box, because it grants real shell access. It's also protected by a per-run random token (constant-time comparison), auto-served only to requests that verifiably originate from the same machine. If you ever set `TERMINAL_HOST=0.0.0.0` (or otherwise expose port 7681) to reach it from another device on your network, auto-connect stops working for remote requests and the token must be entered manually — copy it from the `[term]` console output. Don't expose this port over an untrusted network without a real reverse proxy + TLS in front of it.

---

## API Reference

### System
```http
GET /health          → Server status
GET /capabilities    → All installed library versions
GET /logs            → Server logs
```

### Static Analysis (no AI, multi-language)
```http
POST /static/parse          → Full AST: Big-O, security findings, structural smells, call graph, WASM hints
POST /static/parse-project  → Multi-file: dependency graph, aggregated findings, Project Health scores
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

Graph types: `import` · `call` · `circular` · `heatmap` · `centrality`

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

## Python stack

| Library | Version | Use |
|---|---|---|
| FastAPI | 0.115.5 | Async REST server |
| pylint / flake8 | latest | Code quality (complexity/MI moved to the `complexity-engine` Rust sidecar — see Rust stack below) |
| tree-sitter | 0.23.2 | Multi-language AST |
| networkx | 3.6.1 | Graph algorithms |
| numpy / pandas / polars | latest | Data processing |
| torch / tensorflow-cpu | latest | Deep learning |
| scikit-learn / lightgbm | latest | Classic ML |
| spacy / opencv / scipy | latest | NLP / Vision / Science |
| **Cython** | 3.2.4 | Python → C compilation |

## TypeScript stack

| Library | Version | Use |
|---|---|---|
| Vite | 5.3.1 | Bundler |
| TypeScript | 5.4.5 | Static typing |
| monaco-editor | 0.45.0 | Code editor |
| mermaid | 11.4.0 | Diagrams |
| chart.js | 4.4.3 | Charts |
| @xterm/xterm | 6.0.0 | Terminal emulator (client side of the Rust sidecar) |
| [ansimax](https://github.com/Brashkie/ansimax) | 1.5.0 | ANSI/CLI rendering for the `npm run dev` startup banner |

## Rust stack — two sidecars, one `Cargo.toml`

`terminal-server` (a small sidecar for the integrated terminal) and `complexity-engine` (replaces the `radon` pip dependency — cyclomatic complexity, Maintainability Index, raw line metrics, security/taint analysis, and structural smells, computed in-process instead of imported from a third-party library). Both bins share one root `Cargo.toml` — `services/{terminal,complexity}/` hold source only, no per-service manifest.

| Crate | Used by | Use |
|---|---|---|
| axum | both | HTTP/WebSocket server + routing |
| tokio | both | Async runtime |
| portable-pty | terminal-server | ConPTY (Windows) / PTY (Unix) — one implementation for both |
| subtle | terminal-server | Constant-time token comparison |
| rand | terminal-server | CSPRNG for the per-run token |
| rustpython-parser | complexity-engine | Python source → AST |
| criterion | complexity-engine (dev) | Benchmarks — `cargo bench --bench complexity_bench` |

`complexity-engine` binds to `127.0.0.1:7682` by default (`COMPLEXITY_HOST`/`COMPLEXITY_PORT` overridable, same convention as the terminal sidecar) and exposes `GET /health` + `POST /metrics/complexity` + `POST /parse/python`. No auth token — unlike the terminal, it's a pure computation endpoint with no shell/filesystem access. The Python backend calls it via `apps/api/services/complexity_client.py` and degrades gracefully (empty complexity/MI, no crash) if the sidecar isn't running — same optional-capability pattern as flake8/pylint.

---

## Tests

```bash
cd apps/api && pytest
# pytest.ini lives at the repo root and is auto-discovered; testpaths = apps/api/tests
```

```
test_intelligence.py      120 ✓
test_analysis.py           62 ✓
test_graph.py               54 ✓
test_graph_phase2.py        31 ✓
test_metrics_live.py        34 ✓
test_upload.py              29 ✓
test_security_findings.py   30 ✓
test_static_analysis.py     24 ✓
test_structural_smells.py   14 ✓
test_complexity_client.py    7 ✓
──────────────────────────────
Total: 405 passed
```

Both Rust sidecars have their own checks — `cargo build --release`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (69 unit tests across complexity/MI/raw-metrics/Big-O/space/recursion/CS-Engine-classifiers/security/structural-smells, hand-computed values and parity-tested against the Python implementation) — run via the `terminal` job in [`ci.yml`](.github/workflows/ci.yml), separate from the Python suite above (same job builds/tests both bins, since they share one `Cargo.toml`).

---

## Tooling (lint & format)

[Biome](https://biomejs.dev) formats/lints the TypeScript frontend; [Ruff](https://docs.astral.sh/ruff/) does the same for the Python backend. Both run through `scripts/`:

```bash
./scripts/lint.sh      # or .\scripts\lint.ps1   — check only, no writes
./scripts/format.sh    # or .\scripts\format.ps1 — writes fixes
```

Equivalent direct commands (from repo root): `npm run lint` / `npm run format`, `ruff check apps/api` / `ruff format apps/api`.

---

## Docker commands

```bash
docker compose logs -f                         # Live logs
docker compose build --no-cache && docker compose up -d  # Full rebuild
docker compose ps                              # Active containers
docker exec -it sythrall-backend bash          # Backend shell
docker compose down                            # Stop (keep volumes)
docker compose down -v                         # Stop + delete volumes
```

---

## Configuration

`apps/api/.env`:
```env
PYTHONUNBUFFERED=1
```

`apps/web/.env`:
```env
VITE_SILENCE_SOURCEMAP_WARNINGS=true
```

Terminal sidecar environment variables (optional — default `127.0.0.1:7681`, safe for local use):
```env
TERMINAL_HOST=127.0.0.1
TERMINAL_PORT=7681
```

Complexity sidecar environment variables (optional — default `127.0.0.1:7682`):
```env
COMPLEXITY_HOST=127.0.0.1
COMPLEXITY_PORT=7682
```
The backend finds it via `COMPLEXITY_ENGINE_URL` (default `http://127.0.0.1:7682`; overridden to `http://complexity:7682` in `docker-compose.yml`, the Docker network service name).

Changing ports in `docker-compose.yml`:
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

## Troubleshooting

**Docker won't start** → Open Docker Desktop and wait until the icon stops showing "Starting."

**Port 8000 already in use**
```bash
netstat -ano | findstr :8000   # Windows
lsof -i :8000                  # Linux / Mac
```

**Backend shows "No backend"**
```bash
docker compose ps
docker compose logs backend
```
The first run can take ~25s (downloading PyTorch).

**ZIP rejected** → 200 MB limit. Upload it as a folder if it's larger.

**Module not found**
```bash
docker compose build --no-cache && docker compose up -d
```

**Terminal button doesn't connect** → The Rust toolchain is missing. Install it from [rustup.rs](https://rustup.rs), restart your terminal/console so `PATH` picks it up, and run `npm run dev` again — the `[term]` process should build and start on its own. The rest of the app works the same without it.

**Terminal keeps asking for the token** → Shouldn't normally happen for local use (it auto-connects). If it does, confirm you're opening the app at `http://localhost:5173` (not by network IP) — the "local request" check depends on that.

---

## Roadmap

Organized by phase, not version number — a phase tracks one coherent chunk of work across the project's lifetime; actual releases/tags still carry semantic version numbers (see [CHANGELOG.md](CHANGELOG.md)). Full phase-by-phase detail, including rationale and what's still open, lives in **[ROADMAP.md](ROADMAP.md)**.

**Status**: ✅ Complete · 🟡 Partial · 🔴 Planned

### Language philosophy

Sythrall isn't "built with six languages" — each language sits at the layer where it actually earns its place:

| Layer | Language | Role |
|---|---|---|
| Interaction | **TypeScript** | UI, Monaco integration, editor intelligence, diagrams |
| Intelligence & Science | **Python** | AI/ML detection, orchestration, scientific workloads |
| Native Analysis | **Rust** | The static-analysis engine — parsing, AST, complexity, security, quality, graphs |
| Scientific/HPC | **Fortran** | Analysis *target*, not an implementation language (planned) |
| Machine-level | **Assembly** | Analysis target for instruction/register/control-flow (planned) |
| Native tooling | **Zig** | Build, cross-compilation, standalone distribution (planned) |

### Phases at a glance

| # | Phase | Status |
|---|---|---|
| 1 | Foundation | ✅ |
| 2 | ML/DL Inspector | ✅ |
| 3 | Zoom/Pan + Responsive | ✅ |
| 4 | FastAPI + Project Upload | ✅ |
| 5 | Static Analysis + Editor Intelligence | ✅ |
| 6 | Code Graph + Project Explorer | ✅ |
| 7 | Problems Panel + Live Metrics | ✅ |
| 8 | Computer Science Engine | ✅ |
| 9 | Integrated Terminal + Folder Explorer + Theme | ✅ |
| 10 | Rebrand + `apps/` restructure + large-project scaling | 🟡 |
| 11 | `radon` replaced by `complexity-engine` (Rust) | ✅ |
| 12 | Closed the CS Engine classifiers + Problems Panel placement | ✅ |
| 13 | Algorithmic Intelligence | 🟡 |
| 14 | Data Structures & Graph Intelligence | 🟡 |
| 15 | Mathematical Intelligence | 🔴 |
| 16 | Formal Language Intelligence | 🔴 |
| 17 | Compiler Intelligence | 🔬 research spike |
| 18 | Native Analysis Core (`static_parser.py` → Rust) | 🟡 |
| 19 | Machine Intelligence | 🔬 research spike |
| 20 | Scientific Intelligence | 🔴 |
| 21 | Security & Taint Intelligence | ✅ |
| 22 | Code Quality Intelligence | 🟡 |
| 23 | Execution Intelligence | 🔴 |
| 24 | Extensibility Platform | 🔴 |
| 25 | Sythrall Platform | 🔴 |

Full write-up for every phase — scope, rationale, what shipped, what's still open — → **[ROADMAP.md](ROADMAP.md)**.

---

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for the full version history.

---

## Author

**Brashkie** · [Hepein Oficial](https://github.com/Brashkie) · Lima, Perú

---

## License

GPL-3.0 — see [LICENSE](LICENSE)
