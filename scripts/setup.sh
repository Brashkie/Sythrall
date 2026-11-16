#!/usr/bin/env bash
# Instala las dependencias del proyecto.
#
# **Fase 35** — antes esto creaba un `.venv` e instalaba requirements.txt.
# Sythrall ya no tiene backend de Python: el motor en Rust sirve el HTTP y el
# analisis. Quedan npm (frontend) y cargo (los dos sidecars).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

echo "[1/2] Frontend (npm)..."
npm ci

echo "[2/2] Sidecars de Rust (cargo)..."
cargo build --release

echo
echo "Listo. 'npm run dev' levanta el frontend y los dos sidecars."
