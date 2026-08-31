@echo off
title Sythrall - Deteniendo...
color 0C

echo.
echo  ===========================================
echo   Sythrall - Deteniendo servicios...
echo  ===========================================
echo.

cd /d "%~dp0"

docker compose -f docker\docker-compose.yml down

echo.
echo  [OK] Contenedores detenidos.
echo  Los proyectos subidos se conservan en el volumen uploads_data.
echo.
pause