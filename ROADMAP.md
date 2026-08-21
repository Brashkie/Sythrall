# Roadmap

Full phase-by-phase detail — scope, rationale, what shipped, what's still open. See
[README.md](README.md) for the product overview, or [CHANGELOG.md](CHANGELOG.md) for
the version-by-version release history (a different axis — a phase groups a
coherent chunk of work across the project's lifetime, a release/tag carries its own
semantic version number). Same convention as
[`ansimax`](https://github.com/Brashkie/ansimax)'s own roadmap.

[English](./ROADMAP.md) · [Español](./ROADMAP.es.md)

**Status**: ✅ Complete · 🟡 Partial · 🔴 Planned

## Language philosophy

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

## Where this is going: Computer Science Intelligence, not a linter

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

## Phases

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

### ✅ Phase 10 — Rebrand + `apps/` restructure + large-project scaling + enterprise-style shell

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
- [x] Rust unit tests (`cargo test`) for complexity/MI/raw-metrics against hand-computed values, run by the same CI job that already built `terminal-server`.

### ✅ Phase 12 — Closed the last 3 CS Engine classifiers + Problems Panel placement

Closes out the Phase 8 CS Engine roadmap items and the open Problems Panel decision from Phase 7/Phase 10:

- [x] **Regex → Chomsky Type-3 (Regular)**: detects direct `re.compile/match/search/findall/...` calls per function. Honest about its limit — doesn't trace a `re.Pattern` saved in a variable, only direct `re.XXX(...)` calls.
- [x] **Grammar/parser-shaped code → Chomsky Type-2 (Context-Free)**: heuristic requires *both* a name signal (`parse`/`grammar`/`tokenize`/`lexer`/...) *and* a shape signal (recursion or an explicit append/pop stack pattern) — either signal alone produced too many false positives in testing (a plain recursive `factorial` isn't a parser just because it's recursive).
- [x] **Graph traversal → BFS/DFS/Topological Sort, O(V+E)**: heuristic on variable names (`visited`/`seen`/`explored`, `in_degree`) plus a queue (`.popleft()`) vs. stack/recursion shape to tell BFS from DFS. Same explicitly-heuristic, not-semantic-analysis style as the existing WASM-hint detector (`_wasm_hints_python`) — the new code follows its exact conventions.
- [x] **Problems Panel got its own home**: a 4th right-panel sub-tab (`Flujo · Análisis · Servidor · Problems`, `#rpp-problems`/`#problems-content`) instead of trying to merge it into the existing file-analysis view — resolves the DOM-container conflict documented since Phase 7 without touching that view's richer content (Pylint score, MI, per-function table). Wired into `editor.ts::applyMarkers()`, exactly where `panels/problems.ts` had documented the intended integration point since it was written.
- [x] Removed 3 confirmed-dead exports with zero callers, re-verified twice (`editor.ts::copyEditorContent`, `explorer.ts::explorerMarkModified`/`explorerRefresh`). A longer list of exports with no *visible* caller was found in the same pass but deliberately left alone — not enough certainty they aren't API surface for a feature that hasn't landed yet (same situation Force Graph was in before it got wired).

---

The phases below are the 11 conceptual pillars of "Computer Science Intelligence" (see above) plus two closing phases that sit outside that theory — one architectural, one productization — grouped by how well-founded they are, not by how far in the future they sit. Phases 13–14 are the most direct continuation of what Phase 8/12 already shipped; 15–17 are genuinely new theoretical ground; 18–20 build out the language-philosophy table; 21–22 fill a gap the original 9-pillar pass missed entirely (security, code quality) — same heuristic, confidence-scored style as everything else here, not a new methodology; 23 is architecturally different from everything above it (needs a running process, not source text); 24–25 close the loop — 24 is the API boundary that would let Phases 13–23 grow as plugins instead of every analyzer landing directly in `apps/api`, 25 is how the product actually reaches people once there's something to ship.

### ✅ Phase 13 — Algorithmic Intelligence *(extends Phase 8's O/Θ/Ω engine, not a new architecture)*

Phase 8 already computes worst-case O, best-case Ω, and tight-bound Θ per function — a specific, standard application of asymptotic notation in algorithm analysis (matching the convention used by CLRS and most algorithms texts). Worth making explicit rather than implicit, since the general definitions are broader than that one use:

- [x] **Asymptotic notation reference**, surfaced wherever a Big-O result already appears: a collapsible `<details>` reference table above the Static panel's Big-O table (`panels/static.ts::_renderBigOTable`) covers all five symbols — O (upper bound), Ω (lower bound), Θ (tight bound), o (strict upper bound), ω (strict lower bound) — general definition first, honest that `o`/`ω` aren't computed (no reliable static heuristic distinguishes "strict" from the tight case). The Python hover tooltip (`_build_hover_python`) gets the same explanation as a one-line footnote under its O/Θ/Ω table. JS/TS hover intentionally untouched — it only computes O, not Θ/Ω, so the fuller legend doesn't apply there yet.
- [x] **Space complexity** alongside time complexity, built in Rust and Python from the same pass this time (`services/complexity/src/space.rs` + `static_parser.py::_infer_space_python`, both wired the same day) — same heuristic AST-based approach as the time engine (no execution), but the signal is *what auxiliary structure gets built*, not *how many times a loop runs*: a loop that only accumulates into a scalar (`total += x`) is O(1) space even though it's O(n) time. Detects growing collections (`.append`/`.add`/`.update`/`.insert`/`.extend`, or `d[k] = v` subscript-assignment) at loop-nesting depth (O(n) at depth 1, O(n²) at depth 2 — a matrix built in nested loops), nested comprehensions (`[[0 for _ in range(n)] for _ in range(n)]`), and recursion-stack depth (O(log n) for binary-split recursion reusing `bigo.rs`'s existing split detector, O(n) for linear recursion, since Python doesn't optimize tail calls). Byte-for-byte parity confirmed between the Rust and Python implementations on all 8 cases by hand. Wired into `/intel/hover` and `/intel/analyze` (Rust-first, Python fallback, same gate as Big-O) and a new **Space** column in the Static panel's Big-O table. Benchmarked with Criterion vs. real `_parse_python()` timing on the same synthetic files: 10 functions — 0.77ms vs 18.0ms (23.4×); 100 — 9.17ms vs 212.6ms (23.2×); 1000 — 254ms vs 2083ms (8.2×) — and reported honestly that adding this pass slowed `analyze_rich()` itself by 30–59% against its own prior baseline (two more full-body walks per function stack up), not just the win against Python.
- [x] **Recurrence relation recognition** for divide-and-conquer functions, **Rust-only** (`services/complexity/src/bigo.rs::resolve_master_theorem`) — deliberately not mirrored into the Python fallback (see below). Before this, a recursive function with no loop of its own (`merge_sort`, a recursive binary search) fell straight through every branch of the existing loop-depth heuristic to its generic default (`O(n)`, "base case"), because that heuristic only reasoned about loop nesting and a single `is_recursive` bool. Recognizes `T(n) = a·T(n/2) + f(n)` and resolves it via the Master Theorem's three cases, showing the recurrence itself (`T(n) = 2T(n/2) + Θ(n)`) alongside the resolved `Θ(n log n)` — not just the answer. `a` (subproblems per call) is its own small piece of reasoning, not a reused counter: it treats `if/elif/else` branches as mutually-exclusive alternatives (`max` across branches) and sequential statements as additive (`sum`), so `merge_sort`'s two unconditional recursive calls give `a=2` while a recursive binary search's two calls in mutually-exclusive `elif`/`else` branches correctly give `a=1` (`T(n)=T(n/2)+O(1)` → `Θ(log n)`, not `T(n)=2T(n/2)+O(1)` → `Θ(n)`). `f(n)`'s degree comes from whichever is larger: the function's own loop nesting, or — via a second pass over the file's already-analyzed functions — the Big-O of a helper it calls (`merge_sort` calling `merge()`, already resolved to `O(n)` on its own). 5 Rust tests (`rich::tests`) cover the real cases (`merge_sort`, recursive binary search, non-divide-and-conquer recursion). **Not ported to `static_parser.py`**: an initial pass did build full parity, but with the Rust sidecar as the primary path (running whenever available) and Python only a degraded fallback, maintaining two full implementations of this specific piece wasn't worth it — the pure-Python fallback now always reports `recurrence: None` for every function (confirmed by 4 Python tests) and keeps answering with the pre-existing generic loop/recursion heuristic, same as before this phase. Wired into `/intel/hover` (Rust-rich branch only — the row simply doesn't appear when the sidecar is down) and the Static panel's existing recursion badge tooltip — no new UI surface, reuses the hover that already existed for `recursion_note`.

### 🟡 Phase 14 — Data Structures & Graph Intelligence *(same heuristic-pattern style as the existing WASM-hint/dead-code detectors)*

Direct continuation of the CS Engine (Phase 8/12) — same static-analysis approach, no new architecture. Not just naming the structure, explaining it:

- [ ] Detect AVL / Red-Black Tree / Trie / Heap / Segment Tree / Fenwick Tree / Bloom Filter / B-Tree / HashMap / Skip List from AST shape
- [ ] For each match: complexity (time *and* space), typical operations, use cases, tradeoffs vs. the alternatives — the same "why," not just "what," standard the Big-O engine already holds itself to
- [x] **Graph algorithms as first-class operations, first slice: centrality/hub detection** (`routers/graph.py::_build_centrality_graph`) — new `centrality` graph type reusing the import graph already built for the other 4 (Import/Call/Circular/Heatmap), scored with `nx.degree_centrality`/`in_degree`/`out_degree` on the same NetworkX `DiGraph` the circular-dependency detector already builds — not a new library, the same one already earning its keep. A file is flagged as a "hub" when it's in the top 5 by in-degree *and* has at least 2 dependents (avoids labeling a file with a single import as a hub). Wired into the Diagram panel's project-graph dropdown — reachable by a user, not just an API response, the same "does this have a UI trigger" discipline the Force Graph gap taught this project. Deliberately still Python (`networkx`, not Rust) — Phase 18's Graph Engine needs the import/call graph *construction* itself in Rust first, which doesn't exist yet; porting the algorithms alone before that would mean building two separate graph representations.
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

- [x] **Rich Python analysis, ported to `complexity-engine`, Python duplicate deleted (not just wired alongside)** — `_parse_python()`'s per-function/class/import work (Big-O, Θ/Ω, cyclomatic complexity, tail-call recursion, space complexity, security/taint, structural + naming smells, the 3 CS Engine classifiers) runs in Rust, exposed as `POST /parse/python`. `_parse_python` itself became `async` and now calls the sidecar first for all of it — **the Python heuristic implementations were deleted, not kept as a parity-tested fallback**: ~30 functions removed from `static_parser.py` (all of `_infer_big_o_python`/`_theta_omega_python`/`_loop_analysis_python`/`_has_binary_split_python`, `_recursion_info_python`, the regex/grammar/graph-traversal classifiers, `_infer_space_python` + helpers, `_security_findings_python` + its taint-tracking helpers, every `_check_*` structural/naming smell function). Without the sidecar, `.py` analysis degrades to a lightweight AST-only skeleton (`_skeleton_functions_python`/`_skeleton_classes_python`: name/line/args/docstring/calls/complexity, real — but `big_o`/`space_complexity` as `"?"`, `security_findings`/`structural_smells`/`naming_smells` as `[]`) instead of a second full implementation — the same call this phase already made for Halstead/MI (Phase 11) and Recurrence Relations (Phase 13), generalized to the whole engine. **Now wired into `/static/parse` and `/static/parse-project` too** (the Static panel's main table and Project Health, previously 100% pure-Python, never even checking the sidecar) — `parse_project`'s per-file loop uses `asyncio.gather` instead of sequential awaits, so N files means N concurrent sidecar round-trips, not N serial ones. `pytest` now starts the real sidecar for the whole session (`tests/conftest.py`, `cargo build --bin complexity-engine` if no debug binary exists yet) instead of exercising a Python fallback that no longer exists — the ~130 tests across `test_security_findings.py`/`test_structural_smells.py`/`test_naming_smells.py`/`test_static_analysis.py` now verify the real Rust engine end-to-end via HTTP, with a handful of assertions updated where Rust's (English) wording differs from the deleted Python (Spanish) heuristic's.
- [ ] **Graph Engine** — import/call graph construction and traversal for large projects, currently pure Python in `graph.py`; this is the piece `/static/parse` and Phase 14's graph-algorithm work both need in Rust before either can leave Python behind
- [ ] **Dependency Engine** — circular-dependency detection and cross-file resolution at project scale — the other half `/static/parse` needs, alongside WASM hints and dead-code detection, to fully retire its Python path
- [ ] **Symbol Engine** — go-to-definition / find-references over large codebases, currently regex/AST-based per-file
- [ ] **Project Scanner** — the file-walking + parsing fan-out for whole-project analysis (`read_project_files` and friends)
- [x] **Security, CWE catalog v1 complete in both languages** — `security.rs`: taint tracking + all 5 CWEs (SQL/Command Injection, Path Traversal, Insecure Deserialization, hardcoded credentials), ported the same day the Python version shipped for every one of them (Phase 21), same benchmark-then-swap discipline as every other row here
- [x] **Code Quality, four slices ported, now all Rust-only** — `maintainability.rs::halstead_metrics()`: the 5 Halstead components (Phase 22), Rust-only by design since Phase 11 replaced `radon` (no Python fallback ever existed); `smells.rs`: 5 structural smell checks, and `naming.rs`: 3 naming smell checks — both were briefly built in both languages, then the Python side was deleted in the same pass that retired Big-O/space/security to Rust-only above, so these three now share one boundary instead of two different ones; Project Health scores (Phase 22) aggregate `security_findings`/`structural_smells`/`naming_smells` to project level, orchestration/arithmetic over results the Rust-first engine already computed, not new analysis logic. Architecture smells (Phase 22) also shipped, but deliberately Python-only in `routers/graph.py` — the cross-file import graph they're built on was never ported to Rust (this phase's own unaddressed "Graph Engine" row above), so there was no Rust side to port them into yet, the same kind of boundary Halstead's Rust-only exception documents in reverse. Per-project trend history remains queued
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

### ✅ Phase 21 — Security & Taint Intelligence *(pattern-based data-flow analysis, confidence-scored — not a SAST replacement)*

A gap the original 9-pillar pass missed entirely: nothing in Phases 13–20 looks at security. Direct continuation of the CS Engine's existing style — Phase 8/12's regex/grammar classifiers already prove "heuristic pattern + honestly-labeled confidence" works — applied to source→sink data flow instead of algorithmic shape. Nothing here should ever be presented as "this IS a vulnerability," only as a pattern worth a human's attention, with the evidence shown so the claim is auditable. CWE catalog v1 complete:

- [x] **Taint tracking within a single function** — a recursive, provenance-carrying walk resolves a value back to an untrusted source (`request.args`/`form`/`values`/`json`/`GET`/`POST`/`COOKIES`/`headers`, `input()`, `sys.argv`, `os.environ`/`os.getenv`) through assignments, string concatenation, f-strings, and `.format()`, tracking *whether* the taint was built via string construction (the actual SQLi/command-injection signal) vs. passed through raw. **Deliberately not cross-function** — interprocedural taint needs the call graph the Dependency Engine below doesn't build yet. **Ported to Rust the same day it shipped in Python** (`services/complexity/src/security.rs`), not left as a someday-item. Both covering the vulnerable and the safe/parameterized form of every check, byte-for-byte parity between the two confirmed by hand on every case.
- [x] **CWE catalog v1, 5 of 5 shipped**: SQL Injection (CWE-89) — fires only when the query is *built* via concatenation/f-string/`.format()`, so `cursor.execute("...%s...", (val,))` correctly produces no finding; Command Injection (CWE-78) — `os.system`/`os.popen` (always shell) or `subprocess.*(..., shell=True)`, `subprocess.run([...])` without `shell=True` correctly produces no finding; hardcoded credentials (CWE-798) — name+shape heuristic, file-level rather than per-function since that's genuinely where these live; **Path Traversal (CWE-22)** — `open(path)` needs the same "built via concatenation" signal as SQLi (`open(f"uploads/{name}")` fires, `open(config_path)` doesn't), `os.path.join(base, seg)` fires on any tainted non-first segment even unconcatenated, since joining a raw `"../etc/passwd"` is already the traversal; **Insecure Deserialization (CWE-502)** — `pickle.loads`/`pickle.load`/`marshal.loads` are unsafe by construction (no "safe form" exists, unlike SQLi/command injection), so they fire at Medium confidence even *without* provable taint and escalate to High when the argument does trace to a taint source; `yaml.load` fires unless called with an explicit `Loader=yaml.SafeLoader`/`CSafeLoader`.
- [x] **Confidence-scored findings** (High/Medium — Medium introduced by CWE-502's untainted-but-still-risky case, the first real use of that level), never a bare yes/no — mirrors the "both signals required" precedent from Phase 12's grammar classifier: a source alone isn't a finding, source *and* the concatenation/sink signal both firing is
- [x] **Evidence-tree finding schema** — category → CWE → severity → confidence → source → sink → line → recommendation, one shared shape for every finding. Surfaced in the **Static panel** and in the Python **hover tooltip**, Rust-only (`security.rs`, see Phase 18 — the Python taint-tracking implementation was deleted, not kept as a fallback; without the sidecar, findings degrade to an empty list) — **not yet the Problems Panel**: that tab is fed by per-keystroke lint markers, and security findings currently run on the same heavier `/static/parse` pass as WASM hints, which live in Static too, not Problems
- [x] **Two real bugs found and fixed the same week this shipped**, both via an independent audit, not self-caught: (1) reassigning a tainted variable to a safe literal (`cmd = request.args.get("x"); cmd = "ls -la"`) didn't clear its taint, producing a High-confidence false positive on ordinary defensive code; (2) a nested function reusing an outer variable's name leaked taint across scopes. Both fixed by making the taint walk scope-aware (`_own_scope_nodes` in Python, `walk_stmts_own_scope` in Rust — a generic AST walker variant that stops at nested `FunctionDef`/`Lambda` boundaries instead of descending into them), with regression tests in both languages. The Rust port built *after* both fixes never had either bug.
- [x] **Project-level Findings, aggregated** (Phase 22 slice) — `security_findings` across every file in a project, not just one at a time, surfaced in the Static panel's project view with the originating file shown per item, sorted by severity
- [ ] Explicitly out of scope, permanently: full exploit analysis (ROP, heap spray, use-after-free) — already correctly fenced off under Execution Intelligence (Phase 23 below) as "competes directly with mature SAST tools"; this phase stays inside that same boundary, just static and narrower

### ✅ Phase 22 — Code Quality Intelligence *(the Maintainability Index Phase 11 already ships, broken into its auditable parts, plus the smells one MI number can't express)*

`complexity-engine`'s Maintainability Index (Phase 11) already computed a Halstead Volume internally but only exposed it pre-baked into one formula. This phase surfaces the components that formula is built from, and adds the structural/naming/architecture smells a single MI number can't catch on its own:

- [x] **Halstead metrics, broken out** (`maintainability.rs::HalsteadMetrics`/`halstead_metrics()`) — Vocabulary (η1+η2), Length (N1+N2), Volume, Difficulty, Effort, their own struct next to the MI score instead of staying an opaque input to one formula. Refactoring `compute()` to take the pre-computed metrics instead of re-walking the suite itself actually **removed** a duplicate AST walk that existed before this phase — a real measured win (4.6–7.1% faster at 10/100 functions), not just new surface. **Rust-only, no Python fallback** — same boundary MI/CC/raw metrics have had since Phase 11 replaced `radon` outright; the degraded shape is `halstead: null`, not a slower Python recomputation.
- [x] **Structural smells** (`smells.rs`, Rust-only — see Phase 18): long function (LOC>50), excessive parameters (>5), deep nesting (any block type, depth>4), large class (methods>15 or LOC>300), god object (methods≥20 AND self-attributes≥10) — conventional Fowler/Martin thresholds, each smell carries its threshold and reasoning in the message, never a bare label. `duplicated logic` (AST-shape hashing) is explicitly deferred, not silently dropped — needs a normalization/hashing scheme that doesn't exist yet.
- [x] **Project Health dashboard**: 4 scores (Security/Quality/Complexity/Architecture) aggregated from `security_findings`/`structural_smells`/complexity/the circular-dependency detector at project scale — each score carries its formula and raw counts alongside it, never a bare number. Surfaced on the Dashboard and in the Static panel's project view, sharing one renderer between both.
- [x] **Naming smells** (`naming.rs`, Rust-only — see Phase 18): single-letter variable outside a loop/comprehension (parameters deliberately exempt — `def add(a, b)` is idiomatic and the name sits next to the signature, so flagging it would be noise, not signal), inconsistent snake_case/camelCase mix within one file (up to 3 examples of each style quoted in the message), a nested function binding a name already used in an enclosing scope (module global or an outer function's parameter/local) — intentionally conservative, flags only mechanically-checkable cases rather than "unclear name" judgment calls that would need an LLM to arbitrate. Counted separately from structural smells in the Quality score (`health.quality.naming`) since naming issues are lower-severity per occurrence than a god object or deep nesting.
- [x] **Architecture smells** (`routers/graph.py::_build_architecture_smells`, Python-only): coupling/cohesion per module, built on the import graph that already exists (Phase 6/14) — high efferent coupling (>15 internal imports, calibrated above `main.py`'s own 11 legitimate composition-root imports), unstable dependency (afferent ≥3 and instability Ce/(Ca+Ce) >0.5, Martin's metric), and the already-shipped circular-dependency detector reframed as one instance of a general layer-violation check (each cycle now also surfaces as a `circular_dependency` smell entry, not just its own separate graph). Python-only by design — the cross-file import graph these smells need only exists in `graph.py`, never ported to Rust (that's Phase 18's own separate "Graph Engine" item, deliberately not tackled here) — the mirror-image case of Halstead being Rust-only because *its* data only exists in Rust.

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
