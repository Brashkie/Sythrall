# 🛰 CodeWatch PRO

Monitor profesional de APIs y código — Flask + flake8 + pylint + radon + Docker

---

## 📁 Estructura del proyecto

```
codewatch-pro/
├── backend/
│   ├── app.py              ← Flask API (análisis real con pylint/flake8/radon)
│   ├── requirements.txt    ← Dependencias Python
│   └── Dockerfile          ← Imagen Docker del backend
├── frontend/
│   └── index.html          ← App web (Monaco Editor + Chart.js)
├── docker/
│   └── nginx.conf          ← Configuración Nginx
├── docker-compose.yml      ← Orquestación completa
├── START.bat               ← Iniciar todo (Windows)
├── STOP.bat                ← Detener todo (Windows)
└── README.md               ← Este archivo
```

---

## 🚀 Inicio rápido (Windows)

### Opción A — Doble clic (recomendado)
1. Asegúrate de tener **Docker Desktop** abierto y corriendo
2. Haz doble clic en **`START.bat`**
3. El navegador se abrirá automáticamente en `http://localhost:8080`

### Opción B — Terminal
```bash
# En la carpeta del proyecto:
docker compose up --build

# Luego abre: http://localhost:8080
```

---

## 🌐 URLs del sistema

| Servicio | URL |
|---|---|
| **Dashboard (frontend)** | http://localhost:8080 |
| **Backend API** | http://localhost:5000 |
| **Health check** | http://localhost:5000/health |
| **Capacidades** | http://localhost:5000/capabilities |

---

## 🔌 API del Backend (Flask)

### `GET /health`
Verifica que el servidor esté activo.
```json
{ "status": "ok", "capabilities": { "flake8": true, "pylint": true, "radon": true } }
```

### `POST /analyze/code`
Analiza un archivo de código con pylint, flake8 y radon.
```json
{
  "filename": "mi_script.py",
  "content": "def hello():\n    print('hola')\n",
  "tools": ["ast", "flake8", "pylint", "radon"]
}
```
**Respuesta:**
```json
{
  "issues": [
    { "tool": "flake8", "line": 2, "col": 5, "severity": "warning", "code": "T201", "message": "print found" }
  ],
  "complexity": [
    { "name": "hello", "line": 1, "complexity": 1, "rank": "A" }
  ],
  "maintainability": 100.0,
  "raw_stats": { "loc": 2, "sloc": 2, "comments": 0, "blank": 0 },
  "metrics": { "pylint_score": 9.5 }
}
```

### `POST /check/api`
Verifica URLs de APIs y retorna tiempos de respuesta.
```json
{ "urls": ["http://localhost:8000", "https://api.ejemplo.com"], "timeout": 10 }
```

### `POST /analyze/logs`
Busca errores y warnings en archivos de log.
```json
{ "files": [{ "name": "app.log", "content": "..." }] }
```

### `GET /logs`
Obtiene el historial de logs del propio servidor.

### `GET /api/history`
Historial de verificaciones de APIs.

---

## 🐍 Librerías Python incluidas

| Librería | Versión | Para qué |
|---|---|---|
| **Flask** | 3.0.3 | Servidor web y API REST |
| **flask-cors** | 4.0.1 | Permite peticiones desde el browser |
| **requests** | 2.31.0 | Verificar APIs externas con headers, auth, SSL |
| **flake8** | 7.1.0 | Estilo de código PEP8, errores básicos |
| **pylint** | 3.2.6 | Análisis estático profundo, score de calidad |
| **radon** | 6.0.1 | Complejidad ciclomática, Maintainability Index |
| **python-dotenv** | 1.0.1 | Variables de entorno desde .env |

---

## ⚙️ Configuración avanzada

### Cambiar el puerto
En `docker-compose.yml`:
```yaml
ports:
  - "TUPORT:5000"   # cambiar TUPORT
```

### Agregar variables de entorno al backend
Crea un archivo `.env` en la carpeta `backend/`:
```
FLASK_DEBUG=1
PORT=5000
```

### Modo desarrollo (sin Docker, solo backend)
```bash
cd backend
pip install -r requirements.txt
python app.py
```
Luego abre `frontend/index.html` directamente en el navegador.

### Usar solo el frontend (sin Docker)
Abre `frontend/index.html` con doble clic en el explorador de archivos.
El frontend detecta automáticamente si el backend está disponible.
Si no está disponible, el análisis se hace en el navegador (modo básico).

---

## 🐛 Solución de problemas

**Docker no inicia**
- Abre Docker Desktop primero
- Espera a que el ícono deje de mostrar "Starting"

**Puerto 5000 ocupado**
```bash
# Ver qué usa el puerto
netstat -ano | findstr :5000
# Cambiar el puerto en docker-compose.yml
```

**Backend aparece como "Sin backend"**
- Verifica que Docker esté corriendo: `docker ps`
- Revisa los logs: `docker compose logs backend`
- El backend tarda ~15s en iniciar la primera vez

**Módulo no encontrado (pylint/flake8)**
```bash
docker compose build --no-cache
docker compose up -d
```

---

## 🔄 Comandos útiles

```bash
# Ver logs en tiempo real
docker compose logs -f

# Ver logs solo del backend
docker compose logs -f backend

# Reconstruir imagen (después de cambiar requirements.txt)
docker compose build --no-cache && docker compose up -d

# Ver estado de los contenedores
docker compose ps

# Acceder al contenedor del backend
docker exec -it codewatch-backend bash

# Detener y eliminar todo
docker compose down -v
```