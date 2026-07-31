# CodeWatch PRO - Build
# Compila el frontend (Vite) y valida la sintaxis del backend.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

Write-Host "== CodeWatch PRO - Build ==" -ForegroundColor Cyan

Write-Host "`n[1/2] Frontend (typecheck + build)..." -ForegroundColor Yellow
Push-Location $root
try {
    npm run typecheck
    npm run build
} finally {
    Pop-Location
}

$venvPython = Join-Path $root ".venv\Scripts\python.exe"
if (Test-Path $venvPython) {
    Write-Host "`n[2/2] Backend (sanity check)..." -ForegroundColor Yellow
    Push-Location $root
    try {
        & $venvPython -m compileall -q backend\main.py backend\shared.py backend\routers backend\services
    } finally {
        Pop-Location
    }
} else {
    Write-Host "`n[2/2] Backend - omitido (ejecuta scripts\setup.ps1 para validar tambien el backend)." -ForegroundColor DarkYellow
}

Write-Host "`nBuild completo. Salida frontend en dist\" -ForegroundColor Green
