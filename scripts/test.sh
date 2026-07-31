#!/usr/bin/env bash
# CodeWatch PRO — Tests
# Corre pytest (backend) y typecheck (frontend).
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
failed=0

echo "== CodeWatch PRO — Tests =="

VENV_PYTHON="$root/.venv/bin/python"
[ -f "$VENV_PYTHON" ] || VENV_PYTHON="$root/.venv/Scripts/python.exe"
if [ ! -f "$VENV_PYTHON" ]; then
  echo "No se encontró .venv en la raíz — ejecuta scripts/setup.sh primero." >&2
  exit 1
fi

echo ""
echo "[1/2] Backend (pytest)..."
cd "$root/backend"
"$VENV_PYTHON" -m pytest || failed=1

echo ""
echo "[2/2] Frontend (typecheck)..."
cd "$root"
npm run typecheck || failed=1

if [ "$failed" -ne 0 ]; then
  echo ""
  echo "Hay fallos — revisa la salida arriba." >&2
  exit 1
fi
echo ""
echo "Todo OK."
