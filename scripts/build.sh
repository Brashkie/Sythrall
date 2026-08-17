#!/usr/bin/env bash
# Sythrall — Build
# Compila el frontend (Vite) y valida la sintaxis del backend.
set -e
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "== Sythrall — Build =="

echo ""
echo "[1/3] Frontend (typecheck + build)..."
cd "$root"
npm run typecheck
npm run build

VENV_PYTHON="$root/.venv/bin/python"
[ -f "$VENV_PYTHON" ] || VENV_PYTHON="$root/.venv/Scripts/python.exe"
if [ -f "$VENV_PYTHON" ]; then
  echo ""
  echo "[2/3] Backend (sanity check)..."
  cd "$root"
  "$VENV_PYTHON" -m compileall -q apps/api/main.py apps/api/shared.py apps/api/routers apps/api/services
else
  echo ""
  echo "[2/3] Backend — omitido (ejecuta scripts/setup.sh para validar también el backend)."
fi

if command -v cargo >/dev/null 2>&1; then
  echo ""
  echo "[3/3] Rust sidecars (terminal + complexity) (cargo build --release)..."
  cd "$root"
  cargo build --release
else
  echo ""
  echo "[3/3] Rust sidecars (terminal + complexity) — omitido (instala Rust: https://rustup.rs)."
fi

echo ""
echo "Build completo. Salida frontend en dist/"
