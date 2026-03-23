@echo off
title CodeWatch PRO - Iniciando...
color 0A

echo.
echo  ===========================================
echo   CodeWatch PRO  -  Docker
echo   Flask + flake8 + pylint + radon
echo  ===========================================
echo.

:: Verificar que Docker este corriendo
docker info >nul 2>&1
IF %ERRORLEVEL% NEQ 0 (
    echo  [ERROR] Docker no esta corriendo.
    echo  Abre Docker Desktop y vuelve a intentar.
    echo.
    pause
    exit /b 1
)
echo  [OK] Docker detectado

:: Ir a la carpeta donde esta este .bat
cd /d "%~dp0"

echo.
echo  [1/3] Construyendo imagen del backend...
docker compose build --no-cache
IF %ERRORLEVEL% NEQ 0 (
    echo  [ERROR] Fallo al construir la imagen.
    pause
    exit /b 1
)

echo.
echo  [2/3] Iniciando servicios...
docker compose up -d
IF %ERRORLEVEL% NEQ 0 (
    echo  [ERROR] Fallo al iniciar los contenedores.
    pause
    exit /b 1
)

echo.
echo  [3/3] Esperando que el backend inicie (15 seg)...
timeout /t 15 /nobreak >nul

echo.
echo  ===========================================
echo   Sistema listo!
echo.
echo   Dashboard:   http://localhost:8080
echo   Backend API: http://localhost:5000
echo   Health:      http://localhost:5000/health
echo.
echo   Para detener: ejecuta STOP.bat
echo  ===========================================
echo.

start http://localhost:8080

echo  Mostrando logs en tiempo real (Ctrl+C para salir):
echo.
docker compose logs -f