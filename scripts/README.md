# scripts/

Flujo de desarrollo local **sin Docker**: mismos comandos, en dos versiones —
`.ps1` (PowerShell, Windows) y `.sh` (bash — Git Bash, WSL, macOS, Linux).
Docker sigue siendo una alternativa válida (ver `docker-compose.yml` en la raíz);
estos scripts son un camino más directo para desarrollo día a día.

Todos los manifiestos (`package.json`, `requirements.txt`, `pyproject.toml`, `biome.json`, `pytest.ini`) viven en la **raíz del repo**; `backend/` y `frontend/` solo contienen código fuente. Estos scripts son el único punto de entrada — no necesitas saber en qué carpeta vive cada cosa.

| Script | Qué hace |
|---|---|
| `setup` | Crea `.venv` en la raíz, instala dependencias de Python (`requirements.txt` + `requirements-dev.txt`) y corre `npm install`. Ejecútalo una vez (o cuando cambien las dependencias). |
| `dev` | Wrapper de `npm run dev` — `concurrently` levanta backend (`uvicorn --reload`, puerto 8000) y frontend (`vite`, puerto 5173) juntos en una sola consola, con logs `[web]`/`[api]`. |
| `build` | `tsc --noEmit` + `vite build` en el frontend; valida sintaxis del backend con `compileall`. |
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
  (`package.json`), que usa `concurrently` para levantar `vite` y `uvicorn` como
  un solo proceso padre. Un Ctrl+C detiene ambos. También puedes correr
  `npm run dev` directo, o por separado con `npm run dev:web` / `npm run dev:api`.
- `scripts/run-backend.mjs` es el que arranca uvicorn: resuelve la ruta del
  Python del venv (`.venv/bin/python` vs `.venv/Scripts/python.exe`) según el SO,
  y lo lanza con cwd=`backend/` (rutas relativas como `uploads/projects` dependen de eso).
- `lint`/`format` usan el Ruff instalado en `.venv` si existe; si no,
  caen al `ruff` del PATH.
