# CodeWatch PRO - Tests
# Corre pytest (backend) y typecheck (frontend).
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$failed = $false

Write-Host "== CodeWatch PRO - Tests ==" -ForegroundColor Cyan

$venvPython = Join-Path $root ".venv\Scripts\python.exe"
if (-not (Test-Path $venvPython)) {
    Write-Host "No se encontro .venv en la raiz - ejecuta scripts\setup.ps1 primero." -ForegroundColor Red
    exit 1
}

Write-Host "`n[1/2] Backend (pytest)..." -ForegroundColor Yellow
Push-Location (Join-Path $root "backend")
try {
    & $venvPython -m pytest
    if ($LASTEXITCODE -ne 0) { $failed = $true }
} finally {
    Pop-Location
}

Write-Host "`n[2/2] Frontend (typecheck)..." -ForegroundColor Yellow
Push-Location $root
try {
    npm run typecheck
    if ($LASTEXITCODE -ne 0) { $failed = $true }
} finally {
    Pop-Location
}

if ($failed) {
    Write-Host "`nHay fallos - revisa la salida arriba." -ForegroundColor Red
    exit 1
}
Write-Host "`nTodo OK." -ForegroundColor Green
