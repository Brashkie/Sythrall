"""
Router: Logs & History
Migración exacta de GET /logs y GET /api/history del app.py Flask v3.0.
"""

from fastapi import APIRouter, Query

from shared import LOG_HISTORY, API_HISTORY, now

router = APIRouter()


@router.get("/logs", tags=["📋 Logs"])
async def get_logs(limit: int = Query(default=100, ge=1, le=500)):
    """Equivalente a GET /logs del Flask original."""
    return {"logs": LOG_HISTORY[-limit:], "total": len(LOG_HISTORY)}


@router.get("/api/history", tags=["📋 Logs"])
async def get_api_history():
    """Equivalente a GET /api/history del Flask original."""
    return {"history": API_HISTORY}
