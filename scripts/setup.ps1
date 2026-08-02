# CodeWatch PRO - Setup (sin Docker)
# Manifiestos y paquetes viven en la raiz del repo (package.json, requirements.txt, .venv/, node_modules/).
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

Write-Host "== CodeWatch PRO - Setup ==" -ForegroundColor Cyan

Write-Host "`n[1/2] Backend (Python venv, raiz del repo)..." -ForegroundColor Yellow
Push-Location $root
try {
    if (-not (Test-Path ".venv")) {
        python -m venv .venv
    }
    & ".venv\Scripts\python.exe" -m pip install --upgrade pip -q
    & ".venv\Scripts\python.exe" -m pip install -q -r requirements.txt -r requirements-dev.txt
} finally {
    Pop-Location
}

Write-Host "`n[2/3] Frontend (npm, raiz del repo)..." -ForegroundColor Yellow
Push-Location $root
try {
    npm install
} finally {
    Pop-Location
}

Write-Host "`n[3/3] Terminal sidecar (Rust)..." -ForegroundColor Yellow
if (Get-Command cargo -ErrorAction SilentlyContinue) {
    Write-Host "cargo $(cargo --version) encontrado." -ForegroundColor Green
} else {
    Write-Host "AVISO: no se encontro 'cargo'. La terminal integrada (npm run dev) no va a" -ForegroundColor Red
    Write-Host "funcionar hasta instalar el toolchain de Rust: https://rustup.rs" -ForegroundColor Red
}

Write-Host "`nListo. Usa scripts\dev.ps1 para iniciar el entorno de desarrollo." -ForegroundColor Green
