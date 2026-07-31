#!/usr/bin/env bash
# CodeWatch PRO — Build
# Compila el frontend (Vite) y valida la sintaxis del backend.
set -e
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "== CodeWatch PRO — Build =="

echo ""
echo "[1/2] Frontend (typecheck + build)..."
cd "$root"
npm run typecheck
npm run build

VENV_PYTHON="$root/.venv/bin/python"
[ -f "$VENV_PYTHON" ] || VENV_PYTHON="$root/.venv/Scripts/python.exe"
if [ -f "$VENV_PYTHON" ]; then
  echo ""
  echo "[2/2] Backend (sanity check)..."
  cd "$root"
  "$VENV_PYTHON" -m compileall -q backend/main.py backend/shared.py backend/routers backend/services
else
  echo ""
  echo "[2/2] Backend — omitido (ejecuta scripts/setup.sh para validar también el backend)."
fi

echo ""
echo "Build completo. Salida frontend en dist/"
