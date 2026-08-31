# Sythrall - Cross-compile con Zig
# Fase 29 (Sythrall Platform, antes Fase 25) - Zig como toolchain de
# cross-compilacion para los binarios nativos que este proyecto ya
# shippea (complexity-engine, terminal-server), sin mantener una matriz
# de CI por plataforma. Usa cargo-zigbuild (zig como linker C para el
# target elegido) - probado de punta a punta en esta maquina de
# desarrollo (Windows -> Linux x86_64), no solo documentado.
#
# Uso: scripts\cross-compile.ps1 [target]
#   target por default: x86_64-unknown-linux-gnu
param(
    [string]$Target = "x86_64-unknown-linux-gnu"
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

Write-Host "== Sythrall - Cross-compile ($Target) via Zig ==" -ForegroundColor Cyan

if (-not (Get-Command zig -ErrorAction SilentlyContinue)) {
    Write-Host "Zig no esta instalado - https://ziglang.org/download/" -ForegroundColor Red
    exit 1
}
Write-Host "Zig: $(zig version)"

if (-not (Get-Command cargo-zigbuild -ErrorAction SilentlyContinue)) {
    Write-Host "`ncargo-zigbuild no esta instalado - instalando (cargo install cargo-zigbuild)..." -ForegroundColor Yellow
    cargo install cargo-zigbuild
}

$installedTargets = rustup target list --installed
if (-not ($installedTargets -contains $Target)) {
    Write-Host "`nTarget $Target no instalado - agregando (rustup target add $Target)..." -ForegroundColor Yellow
    rustup target add $Target
}

Write-Host "`nCompilando complexity-engine + terminal-server para $Target..." -ForegroundColor Yellow
Push-Location $root
try {
    cargo zigbuild --target $Target --release --bin complexity-engine --bin terminal-server
} finally {
    Pop-Location
}

$outDir = Join-Path $root "target\$Target\release"
Write-Host "`nListo - binarios en $outDir`:" -ForegroundColor Green
Get-ChildItem -Path $outDir -Filter "complexity-engine*" -ErrorAction SilentlyContinue
Get-ChildItem -Path $outDir -Filter "terminal-server*" -ErrorAction SilentlyContinue
