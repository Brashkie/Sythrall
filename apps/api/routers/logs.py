"""
Router: Logs & History
Migración exacta de GET /logs y GET /api/history del app.py Flask v3.0.
"""

from fastapi import APIRouter, Query

from services.log_client import fetch_logs
from shared import LOG_HISTORY, API_HISTORY

router = APIRouter()


@router.get("/logs", tags=["Logs"])
async def get_logs(limit: int = Query(default=100, ge=1, le=500)):
    """Vista unificada (backend + ambos sidecars Rust) vía el log persistido
    en CBOR (`services/complexity/src/logstore.rs`, servido por
    `GET /log` del sidecar `complexity-engine`) — si el sidecar está caído,
    cae al `LOG_HISTORY` en memoria de siempre (solo backend, sin persistir),
    mismo criterio de degradación con gracia que el resto del proyecto."""
    unified = await fetch_logs(limit)
    if unified is not None:
        return unified
    return {"logs": LOG_HISTORY[-limit:], "total": len(LOG_HISTORY)}


@router.get("/api/history", tags=["Logs"])
async def get_api_history():
    """Equivalente a GET /api/history del Flask original."""
    return {"history": API_HISTORY}
