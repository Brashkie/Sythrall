# Changelog

Todos los cambios notables de CodeWatch PRO se documentan acá. Formato basado en
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [4.4.0] — 2026-07-31

### Added
- **Computer Science Engine (parcial)**: Θ (cota ajustada) y Ω (mejor caso) junto al O ya existente, para funciones Python — se calculan detectando `break`/`return` dentro de un loop (si existe, el mejor caso es Ω(1), como una búsqueda lineal que encuentra el elemento de una). Visible en la tabla Big-O del panel Static Analysis y en el hover del editor.
- Textos de "razón" del Big-O reescritos para ser explicativos ("2 loops anidados — el loop interno se ejecuta n veces por cada iteración del externo") en vez de etiquetas terse.
- Carpeta `scripts/`: `setup`/`dev`/`build`/`test`/`lint`/`format`, cada uno en `.ps1` (Windows) y `.sh` (bash) — flujo de desarrollo local completo sin depender de Docker.
- `npm run dev` levanta backend (uvicorn) y frontend (vite) juntos vía `concurrently`, con logs `[web]`/`[api]` prefijados en una sola consola.
- Biome (frontend) y Ruff (backend) adoptados para lint/format.
- `.dockerignore`, `.vscode/extensions.json`, `CHANGELOG.md`.
- `.github/workflows/ci.yml` — typecheck/lint/build (frontend) + ruff/pytest (backend) en cada push/PR a `main`.
- `.github/workflows/release.yml` — al pushear un tag `vX.Y.Z`, corre la misma verificación y crea una GitHub Release con las notas extraídas de `CHANGELOG.md` y el build del frontend como artefacto adjunto.

### Changed
- Todos los manifiestos/configs (`package.json`, `requirements.txt`, `pyproject.toml`, `biome.json`, `pytest.ini`) se movieron a la raíz del repo; `backend/` y `frontend/` ahora solo contienen código fuente.
- `vite.config.ts`: `root: 'frontend'`, el build ahora escribe en un `dist/` de la raíz.
- Contexto de build de Docker cambiado a la raíz del repo para ambos servicios (`docker-compose.yml` y ambos `Dockerfile` actualizados) — **no probado con un build real** (sin CLI de Docker disponible en el entorno de desarrollo usado); verificar con `docker compose build` antes de confiar en ese camino.
- Badge/footer de licencia corregido de Apache-2.0 a GPL-3.0, para que coincida con el archivo `LICENSE` real.
- Roadmap del README reorganizado: las secciones ahora agrupan por qué tan fundamentada está cada idea (lista para construir vs. "integrar, no reconstruir" vs. largo plazo/otra categoría de herramienta), en vez de una lista plana por versión.

### Fixed
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
