@echo off
title CodeWatch PRO - Deteniendo...
color 0C

echo.
echo  ===========================================
echo   CodeWatch PRO - Deteniendo servicios...
echo  ===========================================
echo.

cd /d "%~dp0"

docker compose down

echo.
echo  [OK] Contenedores detenidos.
echo  Los proyectos subidos se conservan en el volumen uploads_data.
echo.
pause