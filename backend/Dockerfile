FROM python:3.11-slim

WORKDIR /app

# Dependencias del sistema para pylint/radon
RUN apt-get update && apt-get install -y --no-install-recommends \
    gcc \
    && rm -rf /var/lib/apt/lists/*

# El build context es la raíz del repo (requirements.txt vive ahí, no en backend/)
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY backend/ .

# Directorio para proyectos subidos
RUN mkdir -p uploads/projects

EXPOSE 8000

# FastAPI con uvicorn (reemplaza Flask)
CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8000", "--reload"]
