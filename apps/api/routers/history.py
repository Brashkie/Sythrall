"""
Router: History — Fase 28 (Enterprise Intelligence), primera porción:
snapshots locales de Project Health por proyecto (SQLite, sin auth todavía
— ver services/history_store.py para el porqué del alcance angosto).

Endpoints:
  GET    /history/overview                   → panorama entre TODOS los proyectos con snapshots
  POST   /history/{project_id}/snapshot      → corre el análisis actual del proyecto y lo guarda
  GET    /history/{project_id}               → lista de snapshots guardados (más nuevo primero)
  GET    /history/compare/{from_id}/{to_id}  → diff de scores/summary entre 2 snapshots
  DELETE /history/snapshot/{snapshot_id}     → borra uno
"""

from __future__ import annotations

from typing import Any

from fastapi import APIRouter, HTTPException
from starlette.concurrency import run_in_threadpool

from routers.static_analysis import compute_project_analysis
from services import history_store, plugin_registry
from services.project_service import list_projects
from shared import UPLOADS_DIR

router = APIRouter()

_HEALTH_CATEGORIES = ("security", "quality", "complexity", "architecture")
# Solo claves de `summary` cuyo valor es un número — `big_o_distribution`
# (un dict) queda deliberadamente afuera del diff, no tiene un "delta"
# aritmético con sentido.
_SUMMARY_KEYS = (
    "total_files",
    "total_functions",
    "total_classes",
    "total_imports",
    "unused_imports",
    "wasm_candidates",
    "security_findings",
    "structural_smells",
    "naming_smells",
    "modernization_candidates",
    "architecture_smells",
    "total_loc",
)


@router.get("/overview")
async def history_overview() -> dict[str, Any]:
    """Panorama entre TODOS los proyectos con snapshots guardados — Fase 28,
    segundo bullet ("dashboard de equipo con métricas agregadas entre
    proyectos/usuarios"), en la mitad que hoy es real: **entre proyectos, no
    entre usuarios**. La parte de "usuarios" sigue sin construirse porque
    Sythrall todavía no tiene cuentas de usuario de ninguna clase (ver el
    docstring de `services/history_store.py`): agrupar por un `user_id` que
    siempre sería el mismo valor inventado no agregaría información, solo una
    columna falsa. Cuando existan cuentas reales, este mismo endpoint es
    donde entra ese `GROUP BY`.

    **Tiene que declararse ANTES que `GET /{project_id}`**: FastAPI resuelve
    rutas en orden de declaración y ambas tienen un solo segmento, así que
    declarado después, `/history/overview` entraría por el path param y
    devolvería "los snapshots del proyecto llamado 'overview'" (una lista
    vacía) en vez de este panorama. No es teórico — es exactamente el orden
    en el que estas rutas se leen en este archivo.

    Nada acá recalcula un análisis: todo sale de snapshots ya guardados. Un
    proyecto que nunca guardó uno no aparece — eso es correcto, no un hueco:
    el panorama compara historial, y un proyecto sin historial no tiene nada
    que comparar.
    """
    rollup = await run_in_threadpool(history_store.project_rollup)
    # Los nombres viven en la metadata cacheada de cada proyecto en disco
    # (`project_service.list_projects`), no en la base de snapshots — a
    # propósito: renombrar un proyecto no debería reescribir su historial.
    # Sync + toca disco, así que va al threadpool como el resto.
    known = {p["project_id"]: p.get("project_name") for p in await run_in_threadpool(list_projects, UPLOADS_DIR)}

    projects: list[dict[str, Any]] = []
    for entry in rollup:
        latest, previous = entry["latest"], entry["previous"]
        scores = {cat: latest["health"][cat]["score"] for cat in _HEALTH_CATEGORIES}
        projects.append(
            {
                "project_id": entry["project_id"],
                # `None` cuando el proyecto ya se borró del disco pero su
                # historial sigue en la base — se muestra como tal en la UI,
                # nunca se esconde la fila: un score histórico de un proyecto
                # que ya no está sigue siendo un dato real.
                "project_name": known.get(entry["project_id"]),
                "exists": entry["project_id"] in known,
                "snapshots": entry["snapshots"],
                "created_at": latest["created_at"],
                "health": latest["health"],
                "summary": latest["summary"],
                "overall": round(sum(scores.values()) / len(scores)),
                "delta": (
                    {cat: scores[cat] - previous["health"][cat]["score"] for cat in _HEALTH_CATEGORIES}
                    if previous
                    else None
                ),
            }
        )

    # Peor `overall` primero — el orden útil para "dónde hay que mirar", no
    # el alfabético ni el cronológico.
    projects.sort(key=lambda p: p["overall"])

    aggregate: dict[str, Any] = {
        "projects": len(projects),
        "snapshots": sum(p["snapshots"] for p in projects),
        # Promedio simple entre proyectos, NO ponderado por tamaño: un
        # proyecto chico con seguridad rota cuenta igual que uno grande sano.
        # Ponderar por LOC escondería exactamente el caso que este panorama
        # existe para mostrar.
        "avg": {
            cat: round(sum(p["health"][cat]["score"] for p in projects) / len(projects)) if projects else 0
            for cat in _HEALTH_CATEGORIES
        },
        "totals": {key: sum(p["summary"].get(key, 0) for p in projects) for key in _SUMMARY_KEYS},
    }
    aggregate["avg"]["overall"] = round(sum(aggregate["avg"][c] for c in _HEALTH_CATEGORIES) / len(_HEALTH_CATEGORIES))

    return {
        "projects": projects,
        "aggregate": aggregate,
        "note": (
            "Agregado sobre el último snapshot guardado de cada proyecto, no sobre un análisis en vivo — "
            "un proyecto que cambió desde su último snapshot se muestra como estaba entonces."
        ),
    }


@router.post("/{project_id}/snapshot")
async def create_snapshot(project_id: str) -> dict[str, Any]:
    """Corre el mismo análisis que `/static/parse-project` con `project_id`
    (`compute_project_analysis`, la función que ese endpoint también usa —
    una sola fuente de verdad para el scoring, no una segunda fórmula acá) y
    guarda su `health`/`summary`."""
    project_dir = UPLOADS_DIR / project_id
    if not project_dir.exists():
        raise HTTPException(status_code=404, detail=f"Proyecto '{project_id}' no encontrado")
    analysis = await compute_project_analysis(project_id, [])
    # Qué sabía producir el motor al momento de medir. Sin esto, dos
    # snapshots se comparan como si el analizador no hubiera cambiado entre
    # medio — ver `plugin_registry.analysis_capabilities`.
    capabilities = plugin_registry.analysis_capabilities()
    return await run_in_threadpool(
        history_store.save_snapshot, project_id, analysis["health"], analysis["summary"], capabilities
    )


@router.get("/{project_id}")
async def get_project_snapshots(project_id: str) -> list[dict[str, Any]]:
    return await run_in_threadpool(history_store.list_snapshots, project_id)


@router.get("/compare/{from_id}/{to_id}")
async def compare_snapshots(from_id: int, to_id: int) -> dict[str, Any]:
    """`from`/`to` no tienen por qué estar en orden cronológico — el caller
    decide qué dirección de comparación le interesa, `delta` siempre es
    `to - from` tal cual se pidió."""
    from_snap = await run_in_threadpool(history_store.get_snapshot, from_id)
    to_snap = await run_in_threadpool(history_store.get_snapshot, to_id)
    if from_snap is None or to_snap is None:
        raise HTTPException(status_code=404, detail="Snapshot no encontrado")

    health_diff = {
        cat: {
            "from": from_snap["health"][cat]["score"],
            "to": to_snap["health"][cat]["score"],
            "delta": to_snap["health"][cat]["score"] - from_snap["health"][cat]["score"],
        }
        for cat in _HEALTH_CATEGORIES
    }
    summary_diff = {
        key: {
            "from": from_snap["summary"].get(key, 0),
            "to": to_snap["summary"].get(key, 0),
            "delta": to_snap["summary"].get(key, 0) - from_snap["summary"].get(key, 0),
        }
        for key in _SUMMARY_KEYS
    }
    return {
        "from": from_snap,
        "to": to_snap,
        "health_diff": health_diff,
        "summary_diff": summary_diff,
        "capabilities_diff": _capabilities_diff(from_snap.get("capabilities"), to_snap.get("capabilities")),
        "scoring_diff": _scoring_diff(from_snap["health"], to_snap["health"]),
    }


def _scoring_diff(before: dict[str, Any], after: dict[str, Any]) -> dict[str, Any]:
    """Si las dos mediciones usaron FORMULAS distintas, decirlo.

    Hermano exacto de `_capabilities_diff`, y por el mismo motivo: ahi el
    analizador aprendio a ver algo nuevo, aca la forma de convertir hallazgos
    en score cambio. En los dos casos el delta que se muestra NO es un cambio
    del codigo, y sin avisarlo se lee como una mejora o una regresion real.

    Un snapshot anterior a este campo no trae `scoring_version`: es v1 por
    definicion, que es justamente el que no es comparable con el actual.
    """
    v_from = before.get("scoring_version", 1)
    v_to = after.get("scoring_version", 1)
    if v_from == v_to:
        return {"from": v_from, "to": v_to, "comparable": True, "note": ""}
    return {
        "from": v_from,
        "to": v_to,
        "comparable": False,
        "note": (
            f"Las dos mediciones usan formulas de score distintas (v{v_from} y v{v_to}): "
            "el delta de abajo mezcla un cambio de tu codigo con un cambio del calculo. "
            "Para comparar de verdad, volve a analizar y guarda un snapshot nuevo."
        ),
    }


def _capabilities_diff(before: list[str] | None, after: list[str] | None) -> dict[str, Any]:
    """Qué cambió en el ANALIZADOR entre las dos mediciones.

    Es la diferencia entre "tu código empeoró" y "Sythrall aprendió a ver
    algo que antes no veía". Pasó cuatro veces en este repo: un lenguaje que
    devolvía `[]` fijo empezó a emitir hallazgos reales, y el score de
    cualquier proyecto de ese lenguaje bajó sin que se tocara una línea.
    Comparado contra un snapshot anterior, eso se lee como una regresión que
    nunca existió.

    `None` en cualquiera de los dos lados significa que ese snapshot es
    anterior a que se guardara esta información — no que no tuviera
    capacidades. Se dice explícitamente en vez de asumir que no cambió
    nada, que es la suposición que hace daño."""
    if before is None or after is None:
        return {
            "known": False,
            "comparable": False,
            "added": [],
            "removed": [],
            "note": (
                "Al menos uno de los dos snapshots es anterior a que se guardara qué sabía analizar el motor. "
                "Una diferencia de score entre ellos puede venir de un cambio en el código o de un análisis "
                "nuevo que antes no existía, y desde acá no hay forma de distinguirlos."
            ),
        }
    added = sorted(set(after) - set(before))
    removed = sorted(set(before) - set(after))
    if not added and not removed:
        return {
            "known": True,
            "comparable": True,
            "added": [],
            "removed": [],
            "note": "El motor analizaba lo mismo en las dos mediciones: la diferencia de score es del código.",
        }
    return {
        "known": True,
        "comparable": False,
        "added": added,
        "removed": removed,
        "note": (
            f"El motor cambió entre las dos mediciones ({len(added)} análisis nuevos, {len(removed)} retirados). "
            "Parte de la diferencia de score puede venir de eso y no del código — un análisis que antes no "
            "existía encuentra problemas que ya estaban ahí."
        ),
    }


@router.delete("/snapshot/{snapshot_id}")
async def remove_snapshot(snapshot_id: int) -> dict[str, Any]:
    deleted = await run_in_threadpool(history_store.delete_snapshot, snapshot_id)
    if not deleted:
        raise HTTPException(status_code=404, detail="Snapshot no encontrado")
    return {"deleted": True, "id": snapshot_id}
