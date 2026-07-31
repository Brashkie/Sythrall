# CodeWatch PRO - Lint
# Biome (frontend) + Ruff (backend), sin escribir cambios.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$failed = $false

Write-Host "== CodeWatch PRO - Lint ==" -ForegroundColor Cyan

Write-Host "`n[1/2] Frontend (biome check)..." -ForegroundColor Yellow
Push-Location $root
try {
    npm run lint
    if ($LASTEXITCODE -ne 0) { $failed = $true }
} finally {
    Pop-Location
}

$venvRuff = Join-Path $root ".venv\Scripts\ruff.exe"
Write-Host "`n[2/2] Backend (ruff check)..." -ForegroundColor Yellow
Push-Location $root
try {
    if (Test-Path $venvRuff) { & $venvRuff check backend } else { ruff check backend }
    if ($LASTEXITCODE -ne 0) { $failed = $true }
} finally {
    Pop-Location
}

if ($failed) {
    Write-Host "`nHay problemas de lint - revisa la salida arriba." -ForegroundColor Red
    exit 1
}
Write-Host "`nSin problemas de lint." -ForegroundColor Green
