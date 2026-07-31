# CodeWatch PRO - Dev (sin Docker)
# Wrapper fino: la orquestacion real vive en "npm run dev" (package.json),
# que usa concurrently para levantar frontend (vite) y backend (uvicorn) juntos
# en una sola consola, con logs [web]/[api].
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

$venvPython = Join-Path $root ".venv\Scripts\python.exe"
if (-not (Test-Path $venvPython)) {
    Write-Host "No se encontro .venv en la raiz - ejecuta scripts\setup.ps1 primero." -ForegroundColor Red
    exit 1
}
if (-not (Test-Path (Join-Path $root "node_modules"))) {
    Write-Host "No se encontro node_modules en la raiz - ejecuta scripts\setup.ps1 primero." -ForegroundColor Red
    exit 1
}

Write-Host "== CodeWatch PRO - Dev (sin Docker) ==" -ForegroundColor Cyan
Write-Host "  Backend:  http://localhost:8000  (Swagger: /docs)"
Write-Host "  Frontend: http://localhost:5173"
Write-Host "  Ctrl+C detiene ambos procesos."
Write-Host ""

Push-Location $root
try {
    npm run dev
} finally {
    Pop-Location
}
