#!/usr/bin/env bash
# Sythrall — Lint
# Biome (frontend) + Ruff (backend), sin escribir cambios.
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
failed=0

echo "== Sythrall — Lint =="

echo ""
echo "[1/2] Frontend (biome check)..."
cd "$root"
npm run lint || failed=1

VENV_RUFF="$root/.venv/bin/ruff"
[ -f "$VENV_RUFF" ] || VENV_RUFF="$root/.venv/Scripts/ruff.exe"
echo ""
echo "[2/2] Backend (ruff check)..."
cd "$root"
if [ -f "$VENV_RUFF" ]; then "$VENV_RUFF" check apps/api; else ruff check apps/api; fi || failed=1

if [ "$failed" -ne 0 ]; then
  echo ""
  echo "Hay problemas de lint — revisa la salida arriba." >&2
  exit 1
fi
echo ""
echo "Sin problemas de lint."
