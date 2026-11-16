#!/usr/bin/env bash
# Lint de todo el proyecto: biome (frontend) + clippy (Rust).
#
# **Fase 35** — ya no hay `ruff check apps/api`: no hay Python.
set -uo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
failed=0

echo "[1/2] Frontend (biome)..."
npm run lint || failed=1

echo "[2/2] Rust (clippy)..."
cargo clippy --all-targets -- -D warnings || failed=1

exit $failed
