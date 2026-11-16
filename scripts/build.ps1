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

# **Fase 35** - aca habia un 'compileall' del backend de Python. Ya no hay
# backend de Python: el chequeo equivalente es 'cargo build', que el bloque de
# abajo ya hace.

if (Get-Command cargo -ErrorAction SilentlyContinue) {
    Write-Host "`n[2/2] Rust sidecars (terminal + complexity) (cargo build --release)..." -ForegroundColor Yellow
    Push-Location $root
    try {
        cargo build --release
    } finally {
        Pop-Location
    }
} else {
    Write-Host "`n[2/2] Rust sidecars (terminal + complexity) - omitido (instala Rust: https://rustup.rs)." -ForegroundColor DarkYellow
}

Write-Host "`nBuild completo. Salida frontend en dist\" -ForegroundColor Green
