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

# **Fase 35** — acá había un `compileall` del backend de Python. Ya no hay
# backend de Python: el chequeo equivalente es `cargo build`, que el bloque de
# abajo ya hace.

if command -v cargo >/dev/null 2>&1; then
  echo ""
  echo "[2/2] Rust sidecars (terminal + complexity) (cargo build --release)..."
  cd "$root"
  cargo build --release
else
  echo ""
  echo "[2/2] Rust sidecars (terminal + complexity) — omitido (instala Rust: https://rustup.rs)."
fi

echo ""
echo "Build completo. Salida frontend en dist/"
