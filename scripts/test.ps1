# Corre los tests: cargo test (Rust) + typecheck (frontend).
#
# **Fase 35** - ya no hay pytest: toda la logica vive en Rust y sus tests son
# 'cargo test'. Ver la regla del proyecto: nada de tests de Python para codigo
# que solo existe en Rust.
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root
$failed = 0

Write-Host "[1/2] Rust (cargo test)..."
cargo test; if (-not $?) { $failed = 1 }

Write-Host "[2/2] Frontend (typecheck)..."
npm run typecheck; if (-not $?) { $failed = 1 }

exit $failed
