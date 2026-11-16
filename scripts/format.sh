#!/usr/bin/env bash
# Formatea todo: biome (frontend) + rustfmt (Rust).
#
# **Fase 35** — ya no hay `ruff format apps/api`: no hay Python.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

echo "[1/2] Frontend (biome)..."
npm run format

echo "[2/2] Rust (rustfmt)..."
cargo fmt
