# CodeWatch PRO - Format
# Biome --write (frontend) + Ruff format (backend). Escribe cambios en disco.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

Write-Host "== CodeWatch PRO - Format ==" -ForegroundColor Cyan

Write-Host "`n[1/2] Frontend (biome format --write)..." -ForegroundColor Yellow
Push-Location $root
try {
    npm run format
} finally {
    Pop-Location
}

$venvRuff = Join-Path $root ".venv\Scripts\ruff.exe"
Write-Host "`n[2/2] Backend (ruff format)..." -ForegroundColor Yellow
Push-Location $root
try {
    if (Test-Path $venvRuff) { & $venvRuff format backend } else { ruff format backend }
} finally {
    Pop-Location
}

Write-Host "`nFormato aplicado." -ForegroundColor Green
