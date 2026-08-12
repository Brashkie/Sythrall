#!/usr/bin/env bash
# Sythrall - Setup (sin Docker)
# Manifiestos y paquetes viven en la raiz del repo (package.json, requirements.txt, .venv/, node_modules/).
set -e
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# En Windows, `python3` suele ser un stub roto de la Microsoft Store que
# "existe" pero falla al ejecutarse — probar que corra de verdad, no solo
# que el comando exista.
PYTHON=""
for cand in python3 python; do
  if command -v "$cand" >/dev/null 2>&1 && "$cand" -c "" >/dev/null 2>&1; then
    PYTHON="$cand"
    break
  fi
done
if [ -z "$PYTHON" ]; then
  echo "No se encontró un Python funcional (python3/python) en el PATH." >&2
  exit 1
fi

echo "== Sythrall - Setup =="

echo ""
echo "[1/2] Backend (Python venv, raiz del repo)..."
cd "$root"
if [ ! -d ".venv" ]; then
  "$PYTHON" -m venv .venv
fi
VENV_PYTHON=".venv/bin/python"
[ -f "$VENV_PYTHON" ] || VENV_PYTHON=".venv/Scripts/python.exe"
"$VENV_PYTHON" -m pip install --upgrade pip -q
"$VENV_PYTHON" -m pip install -q -r requirements.txt -r requirements-dev.txt

echo ""
echo "[2/3] Frontend (npm, raiz del repo)..."
cd "$root"
npm install

echo ""
echo "[3/3] Terminal sidecar (Rust)..."
if command -v cargo >/dev/null 2>&1; then
  echo "cargo $(cargo --version) encontrado."
else
  echo "AVISO: no se encontró 'cargo'. La terminal integrada (npm run dev) no va a" >&2
  echo "funcionar hasta instalar el toolchain de Rust: https://rustup.rs" >&2
fi

echo ""
echo "Listo. Usa scripts/dev.sh para iniciar el entorno de desarrollo."
