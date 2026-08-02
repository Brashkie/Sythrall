#!/usr/bin/env bash
# CodeWatch PRO — Dev (sin Docker)
# Wrapper fino: la orquestación real vive en "npm run dev" (package.json),
# que usa concurrently para levantar frontend (vite) y backend (uvicorn) juntos
# en una sola consola, con logs [web]/[api].
set -e
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

VENV_PYTHON="$root/.venv/bin/python"
[ -f "$VENV_PYTHON" ] || VENV_PYTHON="$root/.venv/Scripts/python.exe"
if [ ! -f "$VENV_PYTHON" ]; then
  echo "No se encontró .venv en la raíz — ejecuta scripts/setup.sh primero." >&2
  exit 1
fi
if [ ! -d "$root/node_modules" ]; then
  echo "No se encontró node_modules en la raíz — ejecuta scripts/setup.sh primero." >&2
  exit 1
fi
if ! command -v cargo >/dev/null 2>&1; then
  echo "AVISO: no se encontró 'cargo' — la terminal integrada no va a arrancar" >&2
  echo "(el resto de la app funciona igual). Instalá Rust: https://rustup.rs" >&2
fi

echo "== CodeWatch PRO — Dev (sin Docker) =="
echo "  Backend:  http://localhost:8000  (Swagger: /docs)"
echo "  Frontend: http://localhost:5173"
echo "  Terminal: sidecar Rust en :7681 (token impreso en la consola [term])"
echo "  Ctrl+C detiene los tres procesos."
echo ""

cd "$root"
npm run dev
