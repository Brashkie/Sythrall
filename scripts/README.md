# scripts/

Flujo de desarrollo local **sin Docker**: mismos comandos, en dos versiones —
`.ps1` (PowerShell, Windows) y `.sh` (bash — Git Bash, WSL, macOS, Linux).
Docker sigue siendo una alternativa válida (ver `docker-compose.yml` en la raíz);
estos scripts son un camino más directo para desarrollo día a día.

Todos los manifiestos (`package.json`, `requirements.txt`, `pyproject.toml`, `biome.json`, `pytest.ini`, `Cargo.toml`) viven en la **raíz del repo**; `apps/` (web, api, terminal) y `services/` (terminal, complexity — sidecars Rust) solo contienen código fuente. Estos scripts son el único punto de entrada — no necesitas saber en qué carpeta vive cada cosa.

| Script | Qué hace |
|---|---|
| `setup` | Crea `.venv` en la raíz, instala dependencias de Python (`requirements.txt` + `requirements-dev.txt`), corre `npm install` y verifica que `cargo` esté disponible (solo avisa si falta — no lo instala). Ejecútalo una vez (o cuando cambien las dependencias). |
| `dev` | Wrapper de `npm run dev` — `concurrently` levanta backend (`uvicorn --reload`, puerto 8420), frontend (`vite`, puerto 5173) y los sidecars Rust de terminal (`cargo run`, puerto 7681) y complejidad (`cargo run`) juntos en una sola consola, con logs `[web]`/`[api]`/`[term]`/`[cx]`. Si falta `cargo`, todo lo demás funciona igual — solo no arrancan los sidecars nativos (la terminal integrada y el análisis de complejidad caen a su equivalente Python/JS). |
| `build` | `tsc --noEmit` + `vite build` en el frontend; valida sintaxis del backend con `compileall`; `cargo build --release` para el sidecar de terminal (si `cargo` está disponible). |
| `test` | `pytest` (backend) + `tsc --noEmit` (frontend). |
| `lint` | `biome check` (frontend) + `ruff check` (backend) — solo reporta, no modifica archivos. |
| `format` | `biome format --write` (frontend) + `ruff format` (backend) — escribe los cambios. |

## Uso

**Windows (PowerShell):**
```powershell
.\scripts\setup.ps1
.\scripts\dev.ps1
```

**macOS / Linux / Git Bash / WSL:**
```bash
./scripts/setup.sh
./scripts/dev.sh
```

## Notas

- `dev` necesita que `setup` se haya ejecutado antes (busca `.venv` y `node_modules` en la raíz).
- `dev.ps1`/`dev.sh` son wrappers finos — la orquestación real es `npm run dev`
  (`package.json`), que usa `concurrently` para levantar `vite`, `uvicorn` y los
  dos sidecars Rust como un solo proceso padre. Un Ctrl+C detiene los cuatro.
  También puedes correr `npm run dev` directo, o por separado con
  `npm run dev:web` / `npm run dev:api` / `npm run dev:term` / `npm run dev:cx`.
- `scripts/run-backend.mjs` es el que arranca uvicorn: resuelve la ruta del
  Python del venv (`.venv/bin/python` vs `.venv/Scripts/python.exe`) según el SO,
  y lo lanza con cwd=`apps/api/` (rutas relativas como `uploads/projects` dependen de eso).
- `scripts/run-terminal.mjs` arranca el sidecar de terminal (`cargo run --bin
  terminal-server`, código en `services/terminal`, con cwd=raíz donde vive
  `Cargo.toml` del workspace). Imprime un token aleatorio al arrancar — no hace
  falta copiarlo a mano para uso local, el panel de Terminal se auto-conecta
  (ver README, sección de seguridad de la terminal).
- `scripts/run-complexity.mjs` arranca el sidecar de complejidad (`cargo run
  --bin complexity-engine`, código en `services/complexity`, mismo cwd=raíz).
  Si no está disponible, `/intel/analyze`, `/intel/hover` y `/static/bigO`
  caen automáticamente a sus equivalentes en Python (ver Fase 18 en el README).
- `scripts/dev-banner.mjs` imprime el banner de `npm run dev` con
  [`ansimax`](https://github.com/Brashkie/ansimax) antes de levantar los tres procesos.
- `lint`/`format` usan el Ruff instalado en `.venv` si existe; si no,
  caen al `ruff` del PATH.
