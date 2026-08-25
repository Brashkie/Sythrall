"""
Tests — Auth Router (emisor de tokens de sesión de la terminal)
pytest tests/test_auth.py -v
"""

import os
import sys
import time
from pathlib import Path

import jwt
import pytest
from fastapi.testclient import TestClient

sys.path.insert(0, str(Path(__file__).parent.parent))

from main import app

client = TestClient(app)


class TestTerminalToken:
    def test_devuelve_token_valido_en_pedido_local(self):
        # TestClient no expone un peer IP real (usa un host ASGI-scope
        # sintético que no parsea como IP) — se simula el mismo header que
        # estampa el proxy de Vite en dev para un pedido realmente local.
        resp = client.get("/auth/terminal-token", headers={"X-Forwarded-For": "127.0.0.1"})
        assert resp.status_code == 200
        body = resp.json()
        assert body["expires_in"] == 3600

        claims = jwt.decode(body["token"], os.environ["SYTHRALL_AUTH_SECRET"], algorithms=["HS256"])
        assert claims["sub"] == "local"
        assert claims["scope"] == "terminal"
        assert claims["exp"] - claims["iat"] == 3600

    def test_rechaza_pedido_no_loopback_via_x_forwarded_for(self):
        resp = client.get("/auth/terminal-token", headers={"X-Forwarded-For": "203.0.113.5"})
        assert resp.status_code == 403

    def test_usa_el_ultimo_valor_de_x_forwarded_for(self):
        # El primer valor es falsificable por el cliente; el proxy (Vite)
        # estampa la IP real como el ÚLTIMO valor de la cadena.
        resp = client.get("/auth/terminal-token", headers={"X-Forwarded-For": "203.0.113.5, 127.0.0.1"})
        assert resp.status_code == 200
