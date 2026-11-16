#!/usr/bin/env bash
# Corre los tests: cargo test (Rust) + typecheck (frontend).
#
# **Fase 35** — ya no hay pytest: toda la logica vive en Rust y sus tests son
# `cargo test`. Ver la regla del proyecto: nada de tests de Python para codigo
# que solo existe en Rust.
set -uo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
failed=0

echo "[1/2] Rust (cargo test)..."
cargo test || failed=1

echo "[2/2] Frontend (typecheck)..."
npm run typecheck || failed=1

exit $failed
