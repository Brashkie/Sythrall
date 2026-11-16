# Lint de todo el proyecto: biome (frontend) + clippy (Rust).
#
# **Fase 35** - ya no hay 'ruff check apps/api': no hay Python.
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root
$failed = 0

Write-Host "[1/2] Frontend (biome)..."
npm run lint; if (-not $?) { $failed = 1 }

Write-Host "[2/2] Rust (clippy)..."
cargo clippy --all-targets -- -D warnings; if (-not $?) { $failed = 1 }

exit $failed
