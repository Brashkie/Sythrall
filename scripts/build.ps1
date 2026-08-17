# Sythrall - Build
# Compila el frontend (Vite) y valida la sintaxis del backend.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

Write-Host "== Sythrall - Build ==" -ForegroundColor Cyan

Write-Host "`n[1/3] Frontend (typecheck + build)..." -ForegroundColor Yellow
Push-Location $root
try {
    npm run typecheck
    npm run build
} finally {
    Pop-Location
}

$venvPython = Join-Path $root ".venv\Scripts\python.exe"
if (Test-Path $venvPython) {
    Write-Host "`n[2/3] Backend (sanity check)..." -ForegroundColor Yellow
    Push-Location $root
    try {
        & $venvPython -m compileall -q apps\api\main.py apps\api\shared.py apps\api\routers apps\api\services
    } finally {
        Pop-Location
    }
} else {
    Write-Host "`n[2/3] Backend - omitido (ejecuta scripts\setup.ps1 para validar tambien el backend)." -ForegroundColor DarkYellow
}

if (Get-Command cargo -ErrorAction SilentlyContinue) {
    Write-Host "`n[3/3] Rust sidecars (terminal + complexity) (cargo build --release)..." -ForegroundColor Yellow
    Push-Location $root
    try {
        cargo build --release
    } finally {
        Pop-Location
    }
} else {
    Write-Host "`n[3/3] Rust sidecars (terminal + complexity) - omitido (instala Rust: https://rustup.rs)." -ForegroundColor DarkYellow
}

Write-Host "`nBuild completo. Salida frontend en dist\" -ForegroundColor Green
