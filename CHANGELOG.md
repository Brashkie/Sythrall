# Changelog

Todos los cambios notables de CodeWatch PRO se documentan acá. Formato basado en
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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
