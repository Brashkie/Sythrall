"""
log_client.py — Cliente del sidecar Rust `complexity-engine` para el log
unificado (backend Python + ambos sidecars Rust), persistido en CBOR
(`services/complexity/src/logstore.rs`). Python nunca toca bytes CBOR
directamente: solo manda/recibe JSON plano vía los helpers HTTP genéricos
de `complexity_client.py` (`post_to_sidecar`/`get_from_sidecar`) — no una
segunda implementación de "abrir un cliente, timeout, degradar con gracia".
"""

from services.complexity_client import get_from_sidecar, post_to_sidecar


async def persist_log(entry: dict, source: str = "api") -> None:
    """Best-effort — se llama desde `add_log()` como una tarea de fondo, sin
    bloquear al caller ni propagar la excepción si el sidecar está caído.
    Descarta el resultado de `post_to_sidecar` a propósito: a este caller
    no le importa si se persistió o no, solo que nunca explote."""
    await post_to_sidecar("/log", {**entry, "source": source}, timeout=2.0)


async def fetch_logs(limit: int) -> dict | None:
    """`None` en cualquier falla — el caller (`routers/logs.py`) decide el
    fallback al `LOG_HISTORY` en memoria, mismo criterio que el resto de
    las funciones `dict | None` de `complexity_client.py`."""
    return await get_from_sidecar("/log", params={"limit": limit}, timeout=2.0)
