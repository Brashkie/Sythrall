# Formatea todo: biome (frontend) + rustfmt (Rust).
#
# **Fase 35** - ya no hay 'ruff format apps/api': no hay Python.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root

Write-Host "[1/2] Frontend (biome)..."
npm run format

Write-Host "[2/2] Rust (rustfmt)..."
cargo fmt
