@echo off
title Sythrall - Iniciando...
color 0A

echo.
echo  ===========================================
echo   Sythrall  -  Hepein Oficial
echo   Motor en Rust  -  13 lenguajes analizados
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

:: El secreto con el que el motor FIRMA los tokens de sesion de la terminal y
:: el sidecar de terminal los VERIFICA. Los dos contenedores tienen que ver el
:: mismo valor. "npm run dev" lo genera solo; por este camino no pasa nadie por
:: npm, asi que se genera aca si falta. Sin el, todo funciona salvo la terminal
:: integrada.
IF NOT EXIST ".env" (
    echo  [..] Generando secreto de sesion en .env
    powershell -NoProfile -Command "$b=New-Object byte[] 32; [Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($b); 'SYTHRALL_AUTH_SECRET=' + (($b | ForEach-Object { $_.ToString('x2') }) -join '') | Out-File -FilePath .env -Encoding ascii -Append"
)
findstr /b /c:"SYTHRALL_AUTH_SECRET=" .env >nul 2>&1
IF %ERRORLEVEL% NEQ 0 (
    echo  [..] Agregando el secreto de sesion al .env existente
    powershell -NoProfile -Command "$b=New-Object byte[] 32; [Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($b); 'SYTHRALL_AUTH_SECRET=' + (($b | ForEach-Object { $_.ToString('x2') }) -join '') | Out-File -FilePath .env -Encoding ascii -Append"
)
echo  [OK] Secreto de sesion listo

echo.
echo  [1/3] Construyendo imagenes...
docker compose --env-file .env -f docker\docker-compose.yml build --no-cache
IF %ERRORLEVEL% NEQ 0 (
    echo  [ERROR] Fallo al construir las imagenes.
    pause
    exit /b 1
)

echo.
echo  [2/3] Iniciando servicios...
docker compose --env-file .env -f docker\docker-compose.yml up -d
IF %ERRORLEVEL% NEQ 0 (
    echo  [ERROR] Fallo al iniciar los contenedores.
    pause
    exit /b 1
)

echo.
echo  [3/3] Esperando que el motor inicie (15 seg)...
timeout /t 15 /nobreak >nul

echo.
echo  ===========================================
echo   Sistema listo!
echo.
echo   Dashboard:    http://localhost:8080
echo   Motor:        http://localhost:7682
echo   Capacidades:  http://localhost:8080/capabilities
echo.
echo   Para detener: ejecuta STOP.bat
echo  ===========================================
echo.

start http://localhost:8080

echo  Mostrando logs en tiempo real (Ctrl+C para salir):
echo.
docker compose --env-file .env -f docker\docker-compose.yml logs -f
