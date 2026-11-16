# Instala las dependencias del proyecto.
#
# **Fase 35** - antes esto creaba un .venv e instalaba requirements.txt.
# Sythrall ya no tiene backend de Python: el motor en Rust sirve el HTTP y el
# analisis. Quedan npm (frontend) y cargo (los dos sidecars).
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root

Write-Host "[1/2] Frontend (npm)..."
npm ci

Write-Host "[2/2] Sidecars de Rust (cargo)..."
cargo build --release

Write-Host ""
Write-Host "Listo. 'npm run dev' levanta el frontend y los dos sidecars."
