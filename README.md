# 🛰 CodeWatch PRO

> Monitor profesional de APIs y código — análisis estático, métricas de complejidad, ML/DL y más.

![Stack](https://img.shields.io/badge/Frontend-Angular%2018%20%2B%20TypeScript-blue)
![Stack](https://img.shields.io/badge/Backend-Flask%20%2B%20Python-green)
![Stack](https://img.shields.io/badge/Deploy-Docker%20%2B%20Nginx-informational)
![Author](https://img.shields.io/badge/Author-Hepein%20Oficial-purple)

---

## 📁 Estructura del proyecto

```
codewatch-pro/
├── backend/
│   ├── app.py              ← Flask API (análisis con pylint/flake8/radon + ML)
│   ├── requirements.txt    ← Dependencias Python
│   └── Dockerfile          ← Imagen Docker del backend
├── frontend/
│   ├── src/                ← Código fuente Angular 18
│   ├── index.html
│   ├── package.json
│   ├── vite.config.ts
│   └── Dockerfile.frontend ← Imagen Docker del frontend
├── docker/
│   └── nginx.conf          ← Configuración Nginx (reverse proxy)
├── docker-compose.yml      ← Orquestación completa
├── START.bat               ← Iniciar todo (Windows)
├── STOP.bat                ← Detener todo (Windows)
└── README.md
```

---

## ⚡ Requisitos previos

Antes de correr el proyecto asegúrate de tener instalado:

| Herramienta | Versión mínima | Descarga |
|---|---|---|
| **Docker Desktop** | 4.x | [docker.com](https://www.docker.com/products/docker-desktop) |
| **Node.js** | 20.x | [nodejs.org](https://nodejs.org) |
| **Python** | 3.10+ | [python.org](https://www.python.org) |
| **Git** | cualquiera | [git-scm.com](https://git-scm.com) |

---

## 🚀 Instalación y primer uso

### 1 — Clonar el repositorio

```bash
git clone https://github.com/Brashkie/codewatch-pro.git
cd codewatch-pro
```

### 2 — Iniciar con Docker (recomendado)

**Opción A — Doble clic (Windows)**
1. Abre **Docker Desktop** y espera a que esté corriendo
2. Haz doble clic en **`START.bat`**
3. El sistema abre automáticamente `http://localhost:8080`

**Opción B — Terminal**
```bash
docker compose up --build
```
Luego abre: `http://localhost:8080`

### 3 — Modo desarrollo (sin Docker)

**Backend:**
```bash
cd backend
pip install -r requirements.txt
python app.py
# Servidor en http://localhost:5000
```

**Frontend:**
```bash
cd frontend
npm install
npm run dev
# App en http://localhost:5173
```

---

## 🌐 URLs del sistema

| Servicio | URL | Descripción |
|---|---|---|
| **Dashboard** | http://localhost:8080 | App principal |
| **Backend API** | http://localhost:5000 | Flask REST API |
| **Health check** | http://localhost:5000/health | Estado del servidor |
| **Capacidades** | http://localhost:5000/capabilities | Herramientas disponibles |

---

## 🔌 API Reference

### `GET /health`
```json
{ "status": "ok", "capabilities": { "flake8": true, "pylint": true, "radon": true } }
```

### `POST /analyze/code`
Analiza código con pylint, flake8 y radon.

**Request:**
```json
{
  "filename": "mi_script.py",
  "content": "def hello():\n    print('hola')\n",
  "tools": ["ast", "flake8", "pylint", "radon"]
}
```
**Response:**
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
Verifica URLs externas y retorna tiempos de respuesta.
```json
{ "urls": ["https://api.ejemplo.com"], "timeout": 10 }
```

### `POST /analyze/logs`
Busca errores y warnings en archivos de log.
```json
{ "files": [{ "name": "app.log", "content": "..." }] }
```

### `GET /logs`
Historial de logs del servidor.

### `GET /api/history`
Historial de verificaciones de APIs.

---

## 🐍 Stack Python (backend)

| Librería | Versión | Uso |
|---|---|---|
| **Flask** | 3.0.3 | Servidor REST |
| **flask-cors** | 4.0.1 | CORS para el browser |
| **requests** | 2.31.0 | Verificar APIs externas |
| **flake8** | 7.1.0 | Estilo PEP8 |
| **pylint** | 3.2.6 | Análisis estático profundo |
| **radon** | 6.0.1 | Complejidad ciclomática |
| **numpy** | 1.26.4 | Operaciones numéricas |
| **pandas** | 2.2.2 | Análisis de datos |
| **scikit-learn** | 1.5.1 | ML clásico |
| **torch** | 2.3.1+cpu | Deep learning (CPU) |
| **tensorflow-cpu** | 2.16.1 | Redes neuronales |
| **spacy** | 3.7.5 | NLP |
| **python-dotenv** | 1.0.1 | Variables de entorno |

## 🟦 Stack TypeScript (frontend)

| Librería | Versión | Uso |
|---|---|---|
| **Angular** | 18 | Framework principal |
| **chart.js** | 4.4.3 | Gráficas y métricas |
| **monaco-editor** | 0.45.0 | Editor de código |
| **mermaid** | 11.4.0 | Diagramas |
| **diff** | 5.2.0 | Comparación de código |
| **jszip** | 3.10.1 | Compresión |

---

## ⚙️ Configuración

### Variables de entorno
Crea `backend/.env`:
```env
FLASK_DEBUG=1
PORT=5000
```

### Cambiar puertos
En `docker-compose.yml`:
```yaml
ports:
  - "TUPORT:5000"   # backend
  - "TUPORT:80"     # frontend
```

---

## 🔄 Comandos Docker útiles

```bash
# Ver logs en tiempo real
docker compose logs -f

# Logs solo del backend
docker compose logs -f backend

# Reconstruir después de cambios
docker compose build --no-cache && docker compose up -d

# Ver contenedores activos
docker compose ps

# Entrar al contenedor del backend
docker exec -it codewatch-backend bash

# Detener y limpiar todo
docker compose down -v
```

---

## 🐛 Solución de problemas

**Docker no inicia**
- Abre Docker Desktop y espera a que el ícono deje de mostrar "Starting"

**Puerto 5000 ocupado**
```bash
netstat -ano | findstr :5000
```
Luego cambia el puerto en `docker-compose.yml`

**Backend aparece como "Sin backend"**
```bash
docker ps                        # verificar que corre
docker compose logs backend      # ver errores
```
El backend puede tardar ~15s en iniciar la primera vez.

**Módulo no encontrado**
```bash
docker compose build --no-cache
docker compose up -d
```

---

## 👤 Autor

**Brashkie** — [Hepein Oficial](https://github.com/Brashkie)

---

## 📄 Licencia

MIT — libre para uso personal y comercial.
