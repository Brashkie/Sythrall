# Changelog

Todos los cambios notables de Sythrall se documentan acá. Formato basado en
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [4.7.1] — 2026-08-17

### Changed
- **Roadmap reorganizado alrededor de "Computer Science Intelligence"**: las Fases 13-22 de `README.md`/`README.es.md` pasaron de una lista de features sueltas (Data Structure Detector, Language Intelligence, Cython & WASM, etc.) a 9 pilares conceptuales explícitos — Algorithmic Intelligence, Data Structures & Graph Intelligence, Mathematical Intelligence, Formal Language Intelligence, Compiler Intelligence, Native Intelligence, Machine Intelligence, Scientific Intelligence, Execution Intelligence — más una Fase 22 "Sythrall Platform" que junta todo lo que era puramente de producto/distribución (Zig, Cython/WASM, Execution Path Simulator, persistencia empresarial, VS Code/LSP/Jupyter/ApexVision). Motivo: la descripción honesta de Sythrall hoy es "lee código, calcula Big-O" — el objetivo pasa a ser conectar esa heurística con la teoría de CS que ya la explica (la jerarquía de Chomsky que los clasificadores de la Fase 8/12 ya usan, el framing de Cálculo Lambda que ya tiene la recursión tail-call), no agregar features sueltas. Fases 1-12 (historial ya shippeado) no se tocaron — solo se corrigieron las referencias cruzadas de número de fase que quedaron desactualizadas por el corrimiento (`Fase 15`→`18`, `17`→`19`, `18`→`20`, `19`→`22`).

### Added
- **Fase 1 de la migración de `static_parser.py` a Rust**: `services/complexity` gana 5 módulos nuevos (`walk.rs`, `bigo.rs`, `recursion.rs`, `classifiers.rs`, `structure.rs`, orquestados por `rich.rs`) que portan 1:1 todo lo que `_parse_python()` calcula por función/clase/import — Big-O, Θ/Ω, complejidad ciclomática, recursión tail-call, y los 3 clasificadores del CS Engine (regex/grammar/graph traversal) de la Fase 12. Expuesto como `POST /parse/python` en el sidecar existente (no uno nuevo). `static_parser.py` no se tocó — Python y Rust corren en paralelo, exactamente como pidió el usuario ("no reemplazaría static_parser.py de golpe").
- `walk.rs`: walker genérico y exhaustivo del AST (equivalente real a `ast.walk()` de Python — visita cada statement/expresión sin importar la profundidad de anidamiento), compartido por los 4 módulos nuevos en vez de que cada uno reinvente su propio recorrido parcial (el primer intento de `recursion.rs` sí lo hacía, con walkers incompletos que se descartaron antes de mergear).
- `services/complexity_client.py::parse_python_rich()`: cliente del endpoint nuevo, `None` en cualquier falla (el caller decide caer a `_parse_python()`).
- **Benchmarkeado con Criterion, no asumido**: `services/complexity/benches/parse_bench.rs` vs. `_parse_python()` real cronometrado con `timeit` sobre los mismos archivos sintéticos (10/100/1000 funciones, ahora con regex + recursión + loop, no solo el caso simple). Resultado: 0.48ms vs 9.94ms (10 funciones, 20.6×), 5.95ms vs 100.7ms (100, 16.9×), 187ms vs 1038ms (1000, 5.6×) — el margen se achica a mayor escala, reportado tal cual salió.
- **Test de paridad manual** (`scratchpad/parity_test.py`, no forma parte del repo): confirma que `POST /parse/python` y `_parse_python()` devuelven exactamente los mismos valores para todos los campos estructurales/numéricos sobre el mismo archivo de prueba (clase con método async, regex, recursión tail-call y no-tail, BFS) — la única diferencia es el idioma del texto de razón (inglés en Rust, español en Python, a propósito).
- 14 tests unitarios nuevos en Rust (`rich::tests`, mismos casos que ya existían en `test_intelligence.py`) + 2 en Python (`test_complexity_client.py`, degradación sin sidecar) + 5 en Python (`test_static_analysis.py`, nuevo — no existía cobertura para `/static/*` antes de esta pasada).

### Changed
- `routers/static_analysis.py::analyze_big_o` (`POST /static/bigO`) intenta primero `parse_python_rich()` para archivos `.py` — es un endpoint que genuinamente solo necesita `functions`, así que no pierde nada si el sidecar responde. Sin gate de flag cacheado: se intenta en vivo en cada pedido, misma lección de la condición de carrera de `complexity-engine` (v4.7.0). Cae a `parse_file()` (Python) si el sidecar no responde.
- **Deliberadamente NO conectado en `/static/parse`**: ese endpoint devuelve el shape legacy completo al frontend (`dead_code`, `call_graph`, `circular_deps`, `wasm_hints`, `exports`), ninguno de los cuales esta fase calcula todavía en Rust — conectarlo hubiera descartado en silencio campos que el panel Static renderiza. Queda documentado como el motivo concreto por el que `Dependency Engine`/WASM-hints/dead-code son la próxima porción lógica, no una elección arbitraria.
- `complexity.rs` gana una función pública `cyclomatic(body)` factorizada de `function_entry` (mismo comportamiento, cero cambio funcional) — para que `rich.rs` la reuse sin duplicar la lógica de McCabe una segunda vez.
- **`apps/terminal` y `apps/complexity` se mudan a `services/`**: distinción explícita entre productos que el usuario ejecuta directamente (`apps/api`, `apps/web`) y procesos/servidores independientes que esos productos consumen (`services/terminal`, `services/complexity`) — `apps/` dejaba de significar algo preciso apenas dejó de ser "todo lo que no es raíz". Movido con `git mv` (preserva historial). Sin cambios de comportamiento — actualizadas las rutas en `Cargo.toml` (5 `path =`), ambos `Dockerfile` (`COPY services/...`), `docker-compose.yml` (`dockerfile:`) y los comentarios de `scripts/run-{terminal,complexity}.mjs`; `scripts/build.{sh,ps1}` y CI no referenciaban paths directos, sin cambios ahí.

## [4.7.0] — 2026-08-17

### Added
- **Sidecar Rust `apps/complexity` (`complexity-engine`)**: reemplaza a `radon` — complejidad ciclomática (McCabe), Maintainability Index (fórmula Coleman-Oman) y métricas raw de líneas (loc/lloc/sloc/comments/blank/multi), calculadas con código propio sobre el AST de `rustpython-parser` en vez de importadas de una librería de terceros. Mismo patrón arquitectónico que `terminal-server` (v4.5): proceso persistente con servidor HTTP (axum), no subprocess-por-llamada ni extensión nativa PyO3 — descartado explícitamente porque el path de análisis se llama en cada pausa de tipeo del editor, y un subprocess ahí reintroduciría el costo de arranque de proceso que se optimizó para flake8/pylint en v4.6.0. Los dos sidecars ahora comparten un solo `Cargo.toml` en la raíz (2 `[[bin]]`, 1 `[lib]` nuevo — `complexity_core`, usado tanto por `main.rs` como por el benchmark).
- **`apps/api/services/complexity_client.py`** (nuevo): cliente HTTP async del sidecar (`analyze_complexity`, `check_complexity_engine`/`_sync`), con degradación con gracia (shape vacío, sin excepción) si el sidecar no está corriendo — mismo patrón de capacidad opcional que ya existía para flake8/pylint/radon vía `LIB_FLAGS`.
- **Benchmark con Criterion, no asumido**: `apps/complexity/benches/complexity_bench.rs` mide `complexity_core::analyze()` directo (sin HTTP) sobre archivos sintéticos de 10/100/1000 funciones, comparado contra `radon` real cronometrado con `timeit` sobre los mismos archivos. Resultado: 0.42ms vs 8.97ms (10 funciones), 4.69ms vs 89.2ms (100), 102ms vs 899ms (1000) — 9 a 21× más rápido. El motivo del cambio no fue rendimiento (era ownership de la lógica, no depender de una librería de terceros que no se controla) pero se midió igual antes de decidir qué decir acá, en vez de asumir.
- 15 tests unitarios en Rust (`cargo test`, corridos por el job `terminal` de CI, ahora renombrado porque compila/testea los dos sidecars) contra valores de complejidad/MI/raw calculados a mano sobre snippets canónicos. 4 tests nuevos en Python (`test_complexity_client.py`) que ejercitan la degradación con gracia real (el sidecar no corre durante `pytest` en CI — sin mocks, es el mismo comportamiento que vería un usuario que corre `pytest` sin haber hecho `npm run dev`).
- **Los 3 clasificadores pendientes del CS Engine (roadmap v4.4)**: Regex → Chomsky Tipo-3 (detecta llamadas directas `re.compile/match/search/...`), código con forma de gramática/parser → Chomsky Tipo-2 (heurística de nombre + recursión/pila explícita, ambas señales exigidas), y recorrido de grafo → BFS/DFS/Orden Topológico O(V+E) (heurística de nombres de variable `visited`/`in_degree` + forma de cola/pila). Mismo estilo de código que ya usan Big-O/Θ/Ω/recursión en `static_parser.py` — helpers de un solo `ast.walk` que devuelven señales, clasificadores que arman `(label, nota)` a partir de esas señales sin volver a recorrer el AST. Wireados en `/intel/analyze`, `/intel/hover` y la tabla de Big-O del panel Static (3 badges nuevos: 🔤 Regex, 🌳 CFG, 🕸️ BFS/DFS/Topo). 7 tests nuevos en `test_intelligence.py`, incluyendo guards de falso-positivo (una función recursiva sin nombre de parser no se clasifica como grammar-shaped).
- **Problems Panel conectado**, resolviendo la decisión de ubicación abierta desde v4.3/v4.6.0: nuevo 4to sub-tab del panel derecho (`#rpp-problems`/`#problems-content`) en vez de compartir contenedor con la vista de Análisis existente — sin ese cambio, wirear `updateProblems()` hubiera pisado contenido que esa vista tiene y Problems no (Pylint score, Maintainability Index, tabla de complejidad por función). Conectado en `editor.ts::applyMarkers()`, el punto de integración que el propio `panels/problems.ts` documentaba desde que se escribió.

### Removed
- 3 exports de TypeScript confirmados sin ningún caller, verificados dos veces (sin `onclick=`, sin `window.X`): `editor.ts::copyEditorContent`, `explorer.ts::explorerMarkModified`, `explorer.ts::explorerRefresh`. Una lista más larga de exports sin caller visible se encontró en la misma auditoría pero se dejó sin tocar a propósito — no hay certeza de que no sean superficie de API para algo que todavía no se conectó (mismo caso que tuvo el Force Graph antes de esta sesión).

### Changed
- `radon==6.0.1` eliminado de `requirements.txt`. Identificador de wire `"radon"` renombrado a `"complexity"` en toda la superficie donde era un string público — `tools: list[str]` de `/analyze/code`, `/analyze/project`, `/intel/analyze`, keys de capabilities (`LIB_FLAGS["HAS_RADON"]` → `HAS_COMPLEXITY_ENGINE`), y en el frontend (`types/index.ts`, `api/client.ts`, `components/app.ts` — ahí "radon" se mostraba literal como texto en el panel de capacidades, dejarlo habría mostrado un nombre que ya no es cierto — y las clases CSS `.t-radon`/`.pb-tool-radon` → `.t-complexity`/`.pb-tool-complexity`).

### Fixed
- **Condición de carrera en el chequeo de capacidad**, encontrada al conectar el sidecar: a diferencia de flake8/pylint (chequeo por `import`, determinístico), el chequeo de `complexity-engine` es de red — si `main.py` arranca antes de que `cargo build` termine de compilar el sidecar por primera vez (típico en un `npm run dev` en frío), el flag cacheado quedaba en `False` el resto de la sesión aunque el sidecar terminara de levantar segundos después. Arreglado quitando el gate `LIB_FLAGS["HAS_COMPLEXITY_ENGINE"]` de los call sites reales en `analysis.py` (`_run_complexity`/`_run_complexity_metrics` ya degradan con gracia por su cuenta vía `complexity_client`) — ahora cada análisis le pega al sidecar en vivo en vez de confiar en una foto de arranque. Verificado manualmente: bajar el sidecar, confirmar degradación sin crash, levantarlo de nuevo sin reiniciar el backend, confirmar que el siguiente análisis ya lo usa.
- De paso, corregidas dos versiones de Sythrall hardcodeadas como `v4.5` en `apps/api/main.py` (el log de arranque y el campo `server` de `/capabilities`), que habían quedado desactualizadas desde la v4.6.0.

## [4.6.0] — 2026-08-11

### Added
- **Un solo punto de entrada — Proyectos como hub**: "+ Código"/"+ Carpeta" del sidebar dejaron de crear archivos efímeros que se perdían al refrescar — ahora suman al proyecto activo (o crean uno nuevo, pidiendo nombre) usando el mismo backend que ya usaba el panel Proyectos. Antes había 4 entradas de archivos con 2 modelos mentales distintos (efímero vs persistido); ahora todo confluye en un proyecto real. `apps/api/routers/upload.py` gana `project_id` opcional en `/files` y `/folder` para hacer *append* a un proyecto existente en vez de crear uno nuevo cada vez. "+ Log" queda asociado al proyecto activo (`LogFile.projectId`).
- **Session Restore reconectado**: `panels/problems.ts` tenía `saveSession`/`restoreSession`/`clearSession` completos desde v4.3 pero sin un solo caller — no tenía sentido real hasta que hubo un proyecto persistido del cual recuperar contenido real, no solo un nombre de archivo. Ahora el proyecto activo se persiste en localStorage y, al volver a abrir la app, se restaura junto con el último archivo activo.
- **Live Metrics Bar reconectada**: mismo caso — `initLiveMetrics`/`updateLiveMetricsContent` (v4.3) nunca se llamaban desde `editor.ts`. Conectado siguiendo la integración de 2 líneas que el propio archivo ya documentaba.
- Métricas (`renderMetrics`) y badge de la pestaña APIs (`tb-apis`, nunca se actualizaba) también ganan los mismos modos/arreglos.
- **No conectado a propósito**: `updateProblems` (el "Problems Panel" VSCode-style de v4.3) escribe al mismo contenedor (`#analysis-content`) que `renderFileAnalysis`, que ya está en uso y tiene contenido que `updateProblems` no tiene (Pylint score, Maintainability Index, complejidad por función, botones de acceso a Diagrama/ML). Conectarlo tal cual pisaría esa información — falta decidir dónde vive realmente el Problems Panel antes de wirearlo.
- **Concepto de "proyecto activo"**: elegir o subir un proyecto en el panel Proyectos lo marca como activo (`state.activeProjectId`), y Editor/Issues/Diagrama/Static pasan a poder operar sobre él sin necesidad de cargar archivos a mano con "+ Código". Antes, abrir un archivo desde Proyectos solo pintaba el buffer de Monaco — nunca tocaba `state.files`, así que Issues/Diagrama/Static mostraban "sin archivos" aunque el código estuviera a la vista. El botón "Usar en editor" tampoco hacía nada más que un toast.
  - Backend: `apps/api/services/project_service.py` gana `read_project_files()` (factorizado del `os.walk` que antes vivía inline en `generate_project_graph`), y `POST /analyze/project` / `POST /static/parse-project` ahora aceptan `project_id` además de `files` inline — mismo patrón que ya usaba `POST /analyze/graph/project`. El backend lee del disco, el frontend no necesita mandar contenido de archivos.
  - Frontend: se conectó código de "Fase 2" que ya existía completo en `panels/graph.ts` (`generateProjectGraph`, `renderDirTree`, `openNodeInEditor`) pero no estaba wireado a ningún botón — de hecho, se descubrió que **todo** `panels/graph.ts` (incluyendo Import/Call/Circular/Heatmap graph, "Fase 1") no tenía ni un solo caller en el resto de la app. El panel Diagrama ahora tiene un grupo "Proyecto completo" en el selector de tipo, wireado al mismo pipeline de render Mermaid que ya usaba el flowchart de archivo único.
  - Pendiente, a propósito: el Force Graph interactivo y el árbol de directorios con complejidad por archivo (`renderForceGraph`/`renderDirTree`) tienen la lógica lista pero no la UI — la próxima pasada conecta esas dos piezas.
- **Sistema de iconos propio** (`apps/web/src/utils/icons.ts`): set de iconos SVG inline (`stroke="currentColor"`, heredan el color del tema activo — algo que un emoji no puede hacer) + `languageBadge()`, que reemplaza los mapas de emoji por extensión que estaban duplicados en 4 archivos (`upload.ts`, `explorer.ts`, `analysis.ts`, `problems.ts`, más `file-browser.ts` que los importaba).
- **Nav vertical persistente** reemplaza la tabbar horizontal de 11 pestañas — mismo espíritu que Aikido/Datadog/DeepSource (sidebar con iconos + secciones en vez de tabs de texto arriba). `switchTab()` y el wiring de clicks no cambiaron una línea: los items del nav nuevo mantienen `class="tab"` + `data-tab` + `id="t-*"`, así que es un cambio de HTML/CSS, no de lógica.

### Changed
- **Renombrado de CodeWatch PRO a Sythrall**: nombre nuevo en todo el proyecto — `package.json`, `Cargo.toml` (`sythrall-terminal`), `docker-compose.yml` (`sythrall-backend`/`sythrall-frontend`/`sythrall-terminal`/`sythrall-net`), identificadores internos (tema de Monaco, owner de markers, `SESSION_KEY`, `META_FILENAME` del caché de proyectos, loggers), remote de git. Las entradas viejas del changelog que mencionan `CodeWatch PRO` o `.codewatch-meta.json` quedan como estaban — son registro histórico de cómo se llamaba el proyecto en esas versiones, no se reescriben retroactivamente.
- **Reestructuración a `apps/`**: `backend/` → `apps/api/`, `frontend/` → `apps/web/`, `terminal-server/` → `apps/terminal/`. Los manifiestos de cada herramienta (`package.json`, `pyproject.toml`, `Cargo.toml`, `vite.config.ts`, `tsconfig.json`, `biome.json`, `docker-compose.yml`, CI) se quedan en la raíz — ya funcionaban así antes del cambio (p.ej. `vite.config.ts` ya apuntaba a `frontend/` como `root` en vez de vivir adentro). Solo se agrupó el código de cada app bajo un directorio común, siguiendo la convención de monorepos tipo Turborepo/Nx en vez de tener tres carpetas de app sueltas al tope del repo. No afecta a ninguna funcionalidad ni al motor de análisis — es puramente organizativo, con cada referencia de path (Docker, scripts, CI, `.gitignore`) actualizada en consecuencia.

### Fixed
- **`/analyze/graph` (Import Graph) era O(archivos²) por un bug escondido en un one-liner**: `_build_import_graph` en `apps/api/routers/graph.py` reconstruía `{e["from"] for e in edges}` *adentro* del `sum(...)` que calcula archivos aislados — se reconstruía en cada iteración del loop de nodos en vez de una sola vez. Con 4003 archivos sintéticos (benchmark reproducible, ver más abajo) pasó de 3.9s a 0.128s — 30x — al sacar el set-comprehension del loop, igual que ya estaba hecho con `targets`.
- Los 4 generadores de Mermaid (import graph, call graph, circular dependencies, complexity heatmap) en `apps/api/routers/graph.py` concatenaban el string con `mermaid += f"..."` dentro de loops — 17 ocurrencias — reemplazado por acumular en lista + `"\n".join()` al final.
- `/static/parse-project` calculaba un `dependency_graph` con loop triple O(archivos²) que **el frontend nunca leía** (confirmado: no aparece en ningún componente de `apps/web/src`, ni en tests) — con 4003 archivos tardaba 48s calculando algo que se descartaba. Eliminado junto con `all_imports`/`all_exports`, la misma función, también muertos.

### Performance
- Motivado por la ambición de que Sythrall aguante proyectos gigantes: en vez de asumir que hacía falta reescribir el motor en otro lenguaje, se armó un benchmark reproducible (proyecto sintético de hasta 4003 archivos + archivos individuales de hasta 1600 funciones) que aisló los tres bugs de arriba — los tres eran algorítmicos (O(N²) evitable), ninguno tenía que ver con las limitaciones reales de Python. El motor de parsing propio (`parse_file`) ya escalaba lineal antes de este cambio y sigue igual; el cuello de botella estaba en código alrededor del parser, no en el parser.

## [4.5.0] — 2026-08-01

### Added
- **Terminal integrada, estilo VSCode**: shell interactiva real (PowerShell en Windows, `$SHELL`/bash en Unix) embebida en un panel inferior redimensionable, con botón dedicado en el topbar. Primera vez que el proyecto usa **Rust**: nuevo sidecar `terminal-server` (`portable-pty` + `axum` + `tokio`), corriendo como tercer proceso de `npm run dev` (`[web]`/`[api]`/`[term]`). `portable-pty` da una sola implementación para ConPTY (Windows) y PTY (Unix) — evita mantener dos code paths distintos con sus propias rarezas de encoding/resize.
  - **Seguridad**: token aleatorio de 32 bytes (`OsRng`, comparación en tiempo constante vía `subtle::ConstantTimeEq`) generado por arranque, exigido antes de aceptar el WebSocket. El sidecar bindea a `127.0.0.1` por defecto (a diferencia del resto de la app), overridable por env var (`TERMINAL_HOST`/`TERMINAL_PORT`) solo si se lo expone intencionalmente.
  - **Auto-conexión sin fricción**: para uso local normal no hace falta pegar el token a mano — el sidecar lo sirve automáticamente (`GET /terminal/token`) solo a pedidos que verificablemente vienen de la misma máquina (chequeo de `X-Forwarded-For` a través del proxy de Vite, tomando el último valor de la cadena — el único que el proxy controla y el cliente no puede falsificar). Si el token es inválido o el pedido no es local, cae al modal de pegar el token manualmente.
  - Panel con selector **Terminal / Logs**: alterna entre la shell interactiva y una vista de solo lectura con el mismo stream de eventos del tab "Logs" de siempre, en tiempo real. La sesión de shell sigue viva en segundo plano al cambiar de vista.
- **Tema claro/oscuro**: toggle en el topbar, oscuro sigue siendo el default, persiste en `localStorage`, sin flash del tema incorrecto al recargar.
- **Explorador de carpetas en el sidebar** ("+ Carpeta", junto a "+ Código"/"+ Log"): árbol expandible/colapsable estilo VSCode a partir de una carpeta real del disco, vía `<input webkitdirectory>` — funciona en Chrome, Edge, Firefox y Safari (11.1+), sin depender de la File System Access API (que es solo Chromium). El árbol se arma 100% en el cliente; abrir un archivo lo integra al mismo pipeline de análisis que "+ Código".
- **[`ansimax`](https://github.com/Brashkie/ansimax)** (librería propia) integrada en los scripts Node de `npm run dev`: banner con gradiente al arrancar y mensajes de error con color/box.

### Performance
- El bundle de `@xterm/xterm` (~330KB) queda con carga diferida — no se descarga hasta que el usuario abre el panel de Terminal por primera vez (`import()` dinámico, vendor chunk propio en `vite.config.ts`). El chunk principal del bundle no creció.

### Fixed
- `backend/tests/test_static_analysis.py` estaba vacío (0 líneas, 0 tests) — resto de una limpieza anterior, nunca se borró. Sus casos ya viven en `test_intelligence.py`/`test_static_parser` de otros archivos; no se perdió cobertura al borrarlo.
- Números de versión inconsistentes dentro de `backend/main.py` (`v4.1`, `4.2.0`, `v4.2` en tres lugares distintos, ninguno coincidía con `package.json`) — unificados a la versión real del proyecto.
- Conteo de tests desactualizado en el README (decía 316, con un archivo vacío incluido en la cuenta) — corregido a los 297 reales.
- `release.yml` (el workflow que corre al pushear un tag) no compilaba ni testeaba el sidecar Rust — un tag se podía publicar aunque `terminal-server` no compilara. Se agregó el mismo job de `cargo build/clippy/test` que ya tiene `ci.yml`.

## [4.4.0] — 2026-07-31

### Added
- **Computer Science Engine (parcial)**: Θ (cota ajustada) y Ω (mejor caso) junto al O ya existente, para funciones Python — se calculan detectando `break`/`return` dentro de un loop (si existe, el mejor caso es Ω(1), como una búsqueda lineal que encuentra el elemento de una). Visible en la tabla Big-O del panel Static Analysis y en el hover del editor.
- Textos de "razón" del Big-O reescritos para ser explicativos ("2 loops anidados — el loop interno se ejecuta n veces por cada iteración del externo") en vez de etiquetas terse.
- Detección de **tail-call recursion** para funciones Python: distingue `return f(n-1)` (tail, equivalente a un loop) de `return n + f(n-1)` (no-tail, cada nivel consume stack) — con framing de Cálculo Lambda. Badge 🔁 en la tabla Big-O y fila extra en el hover del editor cuando la función es recursiva.
- Carpeta `scripts/`: `setup`/`dev`/`build`/`test`/`lint`/`format`, cada uno en `.ps1` (Windows) y `.sh` (bash) — flujo de desarrollo local completo sin depender de Docker.
- `npm run dev` levanta backend (uvicorn) y frontend (vite) juntos vía `concurrently`, con logs `[web]`/`[api]` prefijados en una sola consola.
- Biome (frontend) y Ruff (backend) adoptados para lint/format.
- `.dockerignore`, `.vscode/extensions.json`, `CHANGELOG.md`.
- `.github/workflows/ci.yml` — typecheck/lint/build (frontend) + ruff/pytest (backend) en cada push/PR a `main`.
- `.github/workflows/release.yml` — al pushear un tag `vX.Y.Z`, corre la misma verificación y crea una GitHub Release con las notas extraídas de `CHANGELOG.md` y el build del frontend como artefacto adjunto.

### Performance
- **`GET /api/upload/projects` era el cuello de botella real detrás del lag general de la app**: recorría el disco completo (`rglob` + `stat` de cada archivo) de *cada* proyecto subido acumulado, de forma síncrona, bloqueando el servidor entero mientras corría. Ahora la metadata de cada proyecto se calcula una vez al subirlo y se cachea en `.codewatch-meta.json` (excluido del árbol de archivos que ve el usuario); listar proyectos solo lee esos caches. Con ~480 proyectos acumulados, la respuesta pasó de bloquear el server a ~30ms.
- Subida de ZIPs grandes (extracción + armado del árbol) movida a threadpool (`starlette.concurrency.run_in_threadpool`) — ya no bloquea el event loop para el resto de las requests mientras procesa un ZIP grande. Mismo tratamiento para listar/borrar proyectos.
- Límite de retención: se guardan como máximo 20 proyectos subidos — al subir uno nuevo que supere el tope, se borran automáticamente los más viejos. Antes no había límite: se habían acumulado ~480 proyectos de prueba (595 MB) sin ningún mecanismo de limpieza.
- Frontend: el árbol de archivos ya no renderiza carpetas enteras de una si tienen cientos/miles de hijos (tope de 300 por carpeta, con indicador "+N más") — evita trabar el navegador al ver un proyecto muy grande.
- **Detección de dependencias circulares (`/analyze/graph`, tipo `circular`) podía colgar el servidor**: `nx.simple_cycles()` se convertía a `list()` completo antes de usarlo, forzando enumerar *todos* los ciclos simples del grafo — en un proyecto de ~150 archivos con solo 3% de densidad de imports ya son 200.000+ ciclos posibles. Confirmado con benchmark: ese caso pasó de 1088ms (sin terminar de crecer) a 4.46ms cortando la enumeración apenas se encuentran 20 ciclos (`find_cycles_capped()`, nueva función compartida en `services/static_parser.py`, usada también por `routers/graph.py`) — de cualquier forma solo se muestran los primeros, así que no se pierde nada útil. No fue necesario reemplazar Python/NetworkX por Rust: el problema era enumeración sin límite, no velocidad de lenguaje — cualquier implementación tiene el mismo crecimiento exponencial sin un tope.

### Changed
- Todos los manifiestos/configs (`package.json`, `requirements.txt`, `pyproject.toml`, `biome.json`, `pytest.ini`) se movieron a la raíz del repo; `backend/` y `frontend/` ahora solo contienen código fuente.
- `vite.config.ts`: `root: 'frontend'`, el build ahora escribe en un `dist/` de la raíz.
- Contexto de build de Docker cambiado a la raíz del repo para ambos servicios (`docker-compose.yml` y ambos `Dockerfile` actualizados) — **no probado con un build real** (sin CLI de Docker disponible en el entorno de desarrollo usado); verificar con `docker compose build` antes de confiar en ese camino.
- Badge/footer de licencia corregido de Apache-2.0 a GPL-3.0, para que coincida con el archivo `LICENSE` real.
- Roadmap del README reorganizado: las secciones ahora agrupan por qué tan fundamentada está cada idea (lista para construir vs. "integrar, no reconstruir" vs. largo plazo/otra categoría de herramienta), en vez de una lista plana por versión.

### Fixed
- **Big-O incorrecto para loops anidados en TS/JS** (`/metrics/live`) — la heurística por regex confundía "cantidad de coincidencias del patrón de anidamiento" con "cantidad de loops", dando `O(n)` para código con loops anidados en vez de `O(n²)`. Detectado por el propio CI (`ci.yml`) en su primera corrida.
- **Explorer panel sin estilos** — `main.css` nunca importaba `explorer.css`.
- **Archivos duplicados al subir** — soltar/seleccionar un archivo agregaba dos entradas idénticas; causado por `app.ts` y `events.ts` cableando ambos los mismos listeners de `drop`/`change` (resto de un refactor incompleto).
- **Topbar desbordado en anchos de laptop comunes (~1100–1650px)** — demasiados elementos `flex-shrink:0` para el espacio disponible; se agregó un breakpoint dedicado que oculta chips/atajos secundarios antes de que el botón principal "Analizar" quede fuera de pantalla.
- Crash de consola en Windows al arrancar (`UnicodeEncodeError` al imprimir logs con emojis bajo `cp1252`) — `shared.py` ahora fuerza UTF-8 en stdout/stderr.
- `backend/routers/history.py` eliminado — código muerto, nunca registrado, reemplazado por `routers/logs.py`.
- Código muerto vario eliminado: implementaciones duplicadas de `_get_lib_version`/`_cyclomatic_python`, `safe_remove`/`save_temp` redefinidos tapando su propio import, imports/variables sin uso en ~10 archivos (frontend y backend).
- CSS duplicado/muerto: `main_upload_addon.css` (no usado, tenía las correcciones que a `upload.css` le faltaban) fusionado y eliminado; tres `nginx.conf` muertos eliminados (`./nginx.conf`, `frontend/nginx.conf`, `docker/nginx.conf` — ninguno estaba referenciado en ningún lado).

---

## [4.3.0] — Problems Panel + Live Metrics
### Added
- Panel de problemas (estilo VSCode): errores, warnings, Big-O, complejidad, hallazgos de seguridad.
- Barra de métricas en vivo en el editor: LOC, funciones, imports, complexity score, Big-O peor caso, parse time.
- Safe mode: fallback por regex + auto-recovery cuando el parser AST falla; detección de archivos corruptos, restauración de sesión.
### Changed
- Licencia cambiada a GPL-3.0.

## [4.2.0] — Code Graph + Project Explorer
### Added
- Import Graph, Call Graph, Dependencias Circulares, Complexity Heatmap (Tree View + Force Graph interactivo).
- Project Explorer: árbol de archivos, tabs multi-archivo, búsqueda global, outline de símbolos.
- 316 tests automatizados.

## [4.1.0] — Static Analysis + Editor Intelligence
### Added
- Parser AST multi-lenguaje (Python, TypeScript, C/C++), sin IA.
- Estimación Big-O, complejidad ciclomática, hints WASM/Cython.
- Linting en tiempo real, hover, Go to Definition, Find References, Rename Symbol, autocompletado semántico.

## [4.0.0] — FastAPI + Project Upload
### Changed
- Migración del backend de Flask a FastAPI.
### Added
- Upload de proyectos (archivos/carpetas/ZIP).

## [3.0.0] — Zoom/Pan + Responsive
### Added
- Zoom y pan en diagramas; layout responsive con bottom navigation en móvil.

## [2.0.0] — ML/DL Inspector
### Added
- Detección de 23 librerías, 23 patrones de pipeline, 25 modelos, 20+ reglas de issues.
- Score 0–100 + diagrama Mermaid del pipeline.

## [1.0.0] — Foundation
### Added
- Backend Flask + pylint/flake8/radon.
- Frontend TypeScript + Vite (sin frameworks), Monaco Editor, Chart.js, Docker.
