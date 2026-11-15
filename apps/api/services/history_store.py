"""
services/history_store.py — Fase 28 (Enterprise Intelligence), primera
porción, deliberadamente angosta a pedido explícito del usuario: persistencia
LOCAL de historial de análisis, sin auth ni backend multi-tenant todavía. El
bullet completo de la fase (Postgres + Delta Lake, JWT real, API pública con
rate limiting) asume un producto SaaS hospedado con usuarios reales — hoy
Sythrall no tiene ninguno de los dos (el único JWT que existe, `routers/
auth.py`, es un token de sesión para la terminal embebida, loopback-only,
`sub: "local"` hardcodeado). Construir Postgres/JWT/API pública antes de que
exista ese consumidor multi-usuario sería exactamente la trampa de
infraestructura especulativa que este proyecto evita en todos lados (mismo
criterio que ya aplica a `AIProvider`, Fase 27). Lo que SÍ es real y
construible hoy: guardar un snapshot de Project Health (`health`/`summary`,
el mismo shape que `/static/parse-project` ya devuelve) por proyecto y por
fecha, para poder comparar cómo cambiaron los scores entre 2 corridas — el
"analysis history, metric comparison between versions" que la fase nombra,
sin el resto de la infraestructura que ese bullet también menciona.

SQLite puro (`sqlite3` de la stdlib, sin ORM nuevo) — misma disciplina de
dependencias mínimas que el resto de este backend. El archivo de la base
vive AFUERA de `apps/api/` (mismo motivo que `UPLOADS_DIR` en `shared.py`:
uvicorn `--reload` vigila `apps/api/`, y escribir un archivo ahí en medio de
un request dispara un reload a mitad de camino — encontrado en vivo antes
con los uploads, no repetido acá).
"""

from __future__ import annotations

import json
import sqlite3
from collections.abc import Iterator
from contextlib import contextmanager
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

# 4 `.parent` porque este archivo vive un nivel más adentro que `shared.py`
# (`apps/api/services/` vs. `apps/api/`) — mismo destino final, la raíz del repo.
DB_PATH = Path(__file__).resolve().parent.parent.parent.parent / "data" / "history.db"


@contextmanager
def _conn() -> Iterator[sqlite3.Connection]:
    """`with conn:` (no `sqlite3.connect` por sí solo) confirma/revierte la
    transacción automáticamente, pero NUNCA cierra la conexión — hay que
    cerrarla a mano, o cada llamada deja un handle de archivo abierto."""
    conn = sqlite3.connect(DB_PATH)
    conn.row_factory = sqlite3.Row
    try:
        with conn:
            yield conn
    finally:
        conn.close()


def init_db() -> None:
    """Llamado una vez al arranque (`main.py::lifespan`), mismo patrón que
    `os.makedirs(UPLOADS_DIR, exist_ok=True)` ya usa para el directorio de
    proyectos."""
    DB_PATH.parent.mkdir(parents=True, exist_ok=True)
    with _conn() as conn:
        conn.execute(
            """
            CREATE TABLE IF NOT EXISTS snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                health_json TEXT NOT NULL,
                summary_json TEXT NOT NULL
            )
            """
        )
        conn.execute("CREATE INDEX IF NOT EXISTS idx_snapshots_project ON snapshots(project_id)")
        # Migración en vez de recrear: las bases que ya existen tienen
        # snapshots reales, y una medición pasada sigue siendo una medición.
        # Las filas viejas quedan con `NULL`, que `_row_to_dict` traduce a
        # `None` — "no se sabe con qué capacidades se calculó", que es
        # exactamente el caso que `compare` tiene que avisar.
        columnas = {r["name"] for r in conn.execute("PRAGMA table_info(snapshots)")}
        if "capabilities_json" not in columnas:
            conn.execute("ALTER TABLE snapshots ADD COLUMN capabilities_json TEXT")


def _row_to_dict(row: sqlite3.Row) -> dict[str, Any]:
    return {
        "id": row["id"],
        "project_id": row["project_id"],
        "created_at": row["created_at"],
        "health": json.loads(row["health_json"]),
        "summary": json.loads(row["summary_json"]),
        # `None` (no `[]`) para las filas anteriores a esta columna: "no se
        # sabe" y "no producía ningún hallazgo" son cosas distintas, y
        # confundirlas haría que `compare` avise de un cambio que no hubo o
        # se calle uno que sí.
        "capabilities": json.loads(row["capabilities_json"]) if _get(row, "capabilities_json") else None,
    }


def _get(row: sqlite3.Row, key: str) -> Any:
    """`sqlite3.Row` levanta `IndexError` con una clave que no existe, no
    devuelve `None` — hace falta para leer una base creada antes de la
    migración sin que reviente."""
    try:
        return row[key]
    except (IndexError, KeyError):
        return None


def save_snapshot(
    project_id: str,
    health: dict[str, Any],
    summary: dict[str, Any],
    capabilities: list[str] | None = None,
) -> dict[str, Any]:
    """Guarda un snapshot con la hora actual (UTC, ISO 8601) — devuelve el
    registro completo, mismo shape que `list_snapshots`/`get_snapshot`, para
    que el caller no tenga que volver a pedirlo.

    `capabilities` es la lista `lenguaje:tipo_de_hallazgo` que el motor sabía
    producir en el momento de medir. Sin eso, dos snapshots del mismo
    proyecto se comparan como si el analizador no hubiera cambiado entre
    medio — y cuando cambia, el score se mueve sin que el código se haya
    tocado."""
    created_at = datetime.now(UTC).isoformat()
    with _conn() as conn:
        cur = conn.execute(
            "INSERT INTO snapshots (project_id, created_at, health_json, summary_json, capabilities_json) "
            "VALUES (?, ?, ?, ?, ?)",
            (
                project_id,
                created_at,
                json.dumps(health),
                json.dumps(summary),
                json.dumps(capabilities) if capabilities is not None else None,
            ),
        )
        snapshot_id = cur.lastrowid
    return {
        "id": snapshot_id,
        "project_id": project_id,
        "created_at": created_at,
        "health": health,
        "summary": summary,
        "capabilities": capabilities,
    }


def list_snapshots(project_id: str) -> list[dict[str, Any]]:
    """Más nuevo primero — el orden que un historial siempre quiere mostrar.

    El desempate por `id DESC` NO es decorativo: `created_at` sale de
    `datetime.now(UTC)`, y en Windows la granularidad real del reloj del
    sistema puede ser de milisegundos enteros — dos snapshots guardados en el
    mismo tick quedan con el MISMO `created_at`, y `ORDER BY created_at DESC`
    a secas los devuelve en un orden arbitrario (encontrado en vivo: el test
    de orden de esta función falló justamente así). Con el `id` (autoincrement,
    estrictamente creciente) como segundo criterio, el orden es siempre el de
    inserción real."""
    with _conn() as conn:
        rows = conn.execute(
            "SELECT * FROM snapshots WHERE project_id = ? ORDER BY created_at DESC, id DESC", (project_id,)
        ).fetchall()
    return [_row_to_dict(r) for r in rows]


def project_rollup() -> list[dict[str, Any]]:
    """Para CADA proyecto que tenga al menos 1 snapshot: el más nuevo, el
    anterior (o `None` si solo hay uno), y cuántos hay en total — la materia
    prima del panorama entre proyectos (`GET /history/overview`, Fase 28,
    segundo bullet).

    Dos queries en vez de una a propósito: la primera pide SOLO `id` y
    `project_id` (nunca los 2 blobs JSON de cada fila), agrupa en Python y se
    queda con los 2 ids más nuevos por proyecto; la segunda trae las filas
    completas solo de esos. Con historial acumulado, un `SELECT *` de todo
    para después descartar el 90% sería caro sin motivo.

    Deliberadamente sin window functions (`ROW_NUMBER() OVER (PARTITION BY
    ...)`, que resolvería esto en una sola query): esas piden SQLite ≥ 3.25 y
    la versión que trae el `sqlite3` de la stdlib depende de cómo se compiló
    el Python de cada máquina — no vale la pena atar el historial a eso por
    ahorrar una query barata.

    Mismo orden que `list_snapshots` (`created_at DESC`, desempate por `id`
    para que dos snapshots del mismo microsegundo nunca queden en un orden
    ambiguo entre las dos funciones)."""
    with _conn() as conn:
        index_rows = conn.execute(
            "SELECT id, project_id FROM snapshots ORDER BY project_id, created_at DESC, id DESC"
        ).fetchall()
        by_project: dict[str, list[int]] = {}
        for r in index_rows:
            by_project.setdefault(r["project_id"], []).append(r["id"])
        wanted = [sid for ids in by_project.values() for sid in ids[:2]]
        if not wanted:
            return []
        placeholders = ",".join("?" * len(wanted))
        full_rows = conn.execute(f"SELECT * FROM snapshots WHERE id IN ({placeholders})", wanted).fetchall()

    full_by_id = {r["id"]: _row_to_dict(r) for r in full_rows}
    return [
        {
            "project_id": project_id,
            "snapshots": len(ids),
            "latest": full_by_id[ids[0]],
            "previous": full_by_id[ids[1]] if len(ids) > 1 else None,
        }
        for project_id, ids in by_project.items()
    ]


def get_snapshot(snapshot_id: int) -> dict[str, Any] | None:
    with _conn() as conn:
        row = conn.execute("SELECT * FROM snapshots WHERE id = ?", (snapshot_id,)).fetchone()
    return _row_to_dict(row) if row else None


def delete_snapshot(snapshot_id: int) -> bool:
    with _conn() as conn:
        cur = conn.execute("DELETE FROM snapshots WHERE id = ?", (snapshot_id,))
    return cur.rowcount > 0
