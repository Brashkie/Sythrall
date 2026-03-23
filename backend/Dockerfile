FROM python:3.12-slim

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    PORT=5000

WORKDIR /app

# Dependencias del sistema
RUN apt-get update && apt-get install -y --no-install-recommends \
    gcc g++ git \
    && rm -rf /var/lib/apt/lists/*

# Instalar en dos pasos:
# 1) Librerías base (rápidas)
COPY requirements.txt .
RUN pip install --no-cache-dir --upgrade pip && \
    pip install --no-cache-dir flask==3.0.3 flask-cors==4.0.1 \
        requests==2.31.0 urllib3==2.2.1 \
        flake8==7.1.0 pylint==3.2.6 radon==6.0.1 \
        python-dotenv==1.0.1 gunicorn

# 2) NumPy, Pandas, Scikit-learn
RUN pip install --no-cache-dir \
        numpy==1.26.4 \
        pandas==2.2.2 \
        scikit-learn==1.5.1

# 3) PyTorch CPU-only
RUN pip install --no-cache-dir \
        torch==2.3.1+cpu \
        torchvision==0.18.1+cpu \
        --extra-index-url https://download.pytorch.org/whl/cpu

# 4) TensorFlow CPU
RUN pip install --no-cache-dir tensorflow-cpu==2.16.1

COPY app.py .

EXPOSE 5000

HEALTHCHECK --interval=30s --timeout=15s --start-period=40s --retries=3 \
    CMD python -c "import urllib.request; urllib.request.urlopen('http://localhost:5000/health')" || exit 1

CMD ["gunicorn", "--bind", "0.0.0.0:5000", "--workers", "2", "--timeout", "120", "app:app"]