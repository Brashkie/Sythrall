#!/usr/bin/env bash
# Sythrall — Dev (sin Docker)
#
# Wrapper fino: la orquestación real vive en "npm run dev" (package.json), que
# usa concurrently para levantar el frontend (vite) y los dos sidecars de Rust
# en una sola consola, con logs [web]/[term]/[cx].
#
# **Fase 35** — acá se verificaba que existiera un `.venv` con el backend de
# Python. Ya no hay backend de Python: el motor en Rust sirve el HTTP y el
# análisis, así que lo único que hace falta es node_modules y cargo.
set -e
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ ! -d "$root/node_modules" ]; then
  echo "No se encontró node_modules en la raíz — ejecuta scripts/setup.sh primero." >&2
  exit 1
fi
if ! command -v cargo >/dev/null 2>&1; then
  echo "No se encontró 'cargo'. Los dos sidecars de Rust son el backend: sin" >&2
  echo "ellos la app no tiene qué servir. Instalá Rust: https://rustup.rs" >&2
  exit 1
fi

echo "== Sythrall — Dev (sin Docker) =="
echo "  Frontend: http://localhost:5173"
echo "  Motor:    http://localhost:7682"
echo "  Terminal: http://localhost:7681"
echo
cd "$root"
npm run dev
