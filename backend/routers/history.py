"""Router: Historial de API checks"""
from fastapi import APIRouter

router = APIRouter()
_api_history: list[dict] = []


@router.get("/history")
async def get_history(limit: int = 50):
    """Historial de verificaciones de APIs."""
    return {"history": _api_history[-limit:], "total": len(_api_history)}