@echo off
title CodeWatch PRO - Iniciando...
color 0A

echo.
echo  ===========================================
echo   CodeWatch PRO  v4.0  -  Hepein Oficial
echo   FastAPI + flake8 + pylint + radon + ML/DL
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
echo  [1/3] Construyendo imagenes...
docker compose build --no-cache
IF %ERRORLEVEL% NEQ 0 (
    echo  [ERROR] Fallo al construir las imagenes.
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
echo  [3/3] Esperando que el backend inicie (25 seg)...
timeout /t 25 /nobreak >nul

echo.
echo  ===========================================
echo   Sistema listo!
echo.
echo   Dashboard:    http://localhost:8080
echo   Backend API:  http://localhost:8000
echo   Swagger UI:   http://localhost:8000/docs
echo   Health:       http://localhost:8000/health
echo.
echo   Para detener: ejecuta STOP.bat
echo  ===========================================
echo.

start http://localhost:8080

echo  Mostrando logs en tiempo real (Ctrl+C para salir):
echo.
docker compose logs -f