#!/usr/bin/env bash
# Sythrall — Format
# Biome --write (frontend) + Ruff format (backend). Escribe cambios en disco.
set -e
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "== Sythrall — Format =="

echo ""
echo "[1/2] Frontend (biome format --write)..."
cd "$root"
npm run format

VENV_RUFF="$root/.venv/bin/ruff"
[ -f "$VENV_RUFF" ] || VENV_RUFF="$root/.venv/Scripts/ruff.exe"
echo ""
echo "[2/2] Backend (ruff format)..."
cd "$root"
if [ -f "$VENV_RUFF" ]; then "$VENV_RUFF" format apps/api; else ruff format apps/api; fi

echo ""
echo "Formato aplicado."
