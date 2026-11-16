# Sythrall - Dev (sin Docker)
#
# Wrapper fino: la orquestacion real vive en "npm run dev" (package.json), que
# usa concurrently para levantar el frontend (vite) y los dos sidecars de Rust.
#
# **Fase 35** - aca se verificaba que existiera un .venv con el backend de
# Python. Ya no hay backend de Python: el motor en Rust sirve el HTTP y el
# analisis, asi que lo unico que hace falta es node_modules y cargo.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)

if (-not (Test-Path (Join-Path $root "node_modules"))) {
    Write-Host "No se encontro node_modules en la raiz - ejecuta scripts\setup.ps1 primero." -ForegroundColor Red
    exit 1
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "No se encontro 'cargo'. Los dos sidecars de Rust son el backend." -ForegroundColor Red
    Write-Host "Instala Rust: https://rustup.rs" -ForegroundColor Red
    exit 1
}

Write-Host "== Sythrall - Dev (sin Docker) =="
Write-Host "  Frontend: http://localhost:5173"
Write-Host "  Motor:    http://localhost:7682"
Write-Host "  Terminal: http://localhost:7681"
Write-Host ""
Set-Location $root
npm run dev
