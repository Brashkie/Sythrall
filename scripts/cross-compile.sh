#!/usr/bin/env bash
# Sythrall — Cross-compile con Zig
# Fase 29 (Sythrall Platform, antes Fase 25) — Zig como toolchain de
# cross-compilación para los binarios nativos que este proyecto ya
# shippea (`complexity-engine`, `terminal-server`), sin mantener una
# matriz de CI por plataforma. Usa `cargo-zigbuild` (zig como linker C
# para el target elegido) — probado de punta a punta en esta máquina de
# desarrollo (Windows → Linux x86_64), no solo documentado.
#
# Uso: scripts/cross-compile.sh [target]
#   target por default: x86_64-unknown-linux-gnu
#   otros targets probables: aarch64-unknown-linux-gnu, x86_64-apple-darwin
#   (requieren su propio `rustup target add`, no instalado por este script)
set -e
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target="${1:-x86_64-unknown-linux-gnu}"

echo "== Sythrall — Cross-compile ($target) via Zig =="

if ! command -v zig >/dev/null 2>&1; then
  echo "Zig no está instalado — https://ziglang.org/download/"
  exit 1
fi
echo "Zig: $(zig version)"

if ! command -v cargo-zigbuild >/dev/null 2>&1; then
  echo ""
  echo "cargo-zigbuild no está instalado — instalando (cargo install cargo-zigbuild)..."
  cargo install cargo-zigbuild
fi

if ! rustup target list --installed | grep -q "^${target}\$"; then
  echo ""
  echo "Target $target no instalado — agregando (rustup target add $target)..."
  rustup target add "$target"
fi

echo ""
echo "Compilando complexity-engine + terminal-server para $target..."
cd "$root"
cargo zigbuild --target "$target" --release --bin complexity-engine --bin terminal-server

out_dir="$root/target/$target/release"
echo ""
echo "Listo — binarios en $out_dir:"
ls -la "$out_dir/complexity-engine" "$out_dir/terminal-server" 2>/dev/null || \
  ls -la "$out_dir/complexity-engine.exe" "$out_dir/terminal-server.exe" 2>/dev/null
