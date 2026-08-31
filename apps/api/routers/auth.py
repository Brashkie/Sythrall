"""
Router: Auth — emisor de tokens de sesión (JWT) para la terminal.

apps/api es el único emisor hoy (services/terminal solo verifica — ver
services/terminal/src/auth.rs). El usuario es implícito ("local") porque
todavía no hay login real, pero la cañería ya tiene la forma que va a tener
el día que lo haya: ese día cambia quién se firma en `sub`, no cómo se
verifica el token en el sidecar Rust.
"""

import ipaddress
import os
import time

import jwt
from fastapi import APIRouter, Request
from fastapi.responses import JSONResponse

router = APIRouter()

_SECRET = os.environ.get("SYTHRALL_AUTH_SECRET")
if not _SECRET:
    raise RuntimeError(
        "SYTHRALL_AUTH_SECRET no está seteada. Corré `npm run dev` "
        "(scripts/dev-banner.mjs la genera sola) o exportala a mano antes "
        "de levantar este backend."
    )

_SCOPE = "terminal"
_TTL_SECONDS = 3600


def _is_loopback_ip(ip_str: str) -> bool:
    try:
        return ipaddress.ip_address(ip_str).is_loopback
    except ValueError:
        return False


def _is_loopback_request(request: Request) -> bool:
    """Mismo criterio que `is_loopback_request` en services/terminal/src/main.rs:
    el ÚLTIMO valor de X-Forwarded-For (el que estampa Vite con la IP real,
    no falsificable por el cliente) si existe, si no la IP del peer directo.
    """
    xff = request.headers.get("x-forwarded-for")
    if xff:
        return _is_loopback_ip(xff.rsplit(",", 1)[-1].strip())
    client = request.client
    if client is None:
        return False
    return _is_loopback_ip(client.host)


@router.get("/terminal-token", tags=["Auth"])
async def terminal_token(request: Request):
    """Conveniencia para uso 100% local: si el pedido viene de esta misma
    máquina, firma y sirve un token de sesión sin que el usuario tenga que
    pedirlo a mano. Si alguna vez se expone esto más allá de localhost, esto
    se corta hasta que exista login real.
    """
    if not _is_loopback_request(request):
        return JSONResponse(status_code=403, content={"detail": "forbidden"})

    now = int(time.time())
    claims = {"sub": "local", "scope": _SCOPE, "iat": now, "exp": now + _TTL_SECONDS}
    token = jwt.encode(claims, _SECRET, algorithm="HS256")
    return {"token": token, "expires_in": _TTL_SECONDS}
