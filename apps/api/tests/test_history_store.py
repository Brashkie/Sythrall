"""
Tests — services/history_store.py (Fase 28, Enterprise Intelligence,
primera porción: persistencia local de snapshots de Project Health).

Cada test monkeypatchea `history_store.DB_PATH` a un archivo dentro de
`tmp_path` (fixture de pytest) — aislado del `data/history.db` real que usa
el proceso en desarrollo, y de los demás tests entre sí.
"""

import sys
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from services import history_store

FAKE_HEALTH = {
    "security": {"score": 100, "high": 0, "medium": 0, "low": 0},
    "quality": {"score": 95, "smells": 1, "naming": 0},
    "complexity": {"score": 90, "avg_complexity": 3.2},
    "architecture": {"score": 100, "cycles": 0, "smells": 0},
}
FAKE_SUMMARY = {"total_files": 3, "total_functions": 10, "big_o_distribution": {"O(1)": 8, "O(n)": 2}}


def _use_temp_db(monkeypatch, tmp_path):
    monkeypatch.setattr(history_store, "DB_PATH", tmp_path / "history.db")
    history_store.init_db()


def test_init_db_creates_table_and_starts_empty(monkeypatch, tmp_path):
    _use_temp_db(monkeypatch, tmp_path)
    assert history_store.list_snapshots("proj-1") == []


def test_save_and_get_snapshot_roundtrips_health_and_summary(monkeypatch, tmp_path):
    _use_temp_db(monkeypatch, tmp_path)
    saved = history_store.save_snapshot("proj-1", FAKE_HEALTH, FAKE_SUMMARY)
    assert saved["id"] is not None
    assert saved["project_id"] == "proj-1"
    assert saved["created_at"]  # ISO 8601 string no vacío

    fetched = history_store.get_snapshot(saved["id"])
    assert fetched is not None
    assert fetched["health"] == FAKE_HEALTH
    assert fetched["summary"] == FAKE_SUMMARY


def test_get_snapshot_returns_none_for_missing_id(monkeypatch, tmp_path):
    _use_temp_db(monkeypatch, tmp_path)
    assert history_store.get_snapshot(999999) is None


def test_list_snapshots_orders_newest_first(monkeypatch, tmp_path):
    _use_temp_db(monkeypatch, tmp_path)
    first = history_store.save_snapshot("proj-1", FAKE_HEALTH, FAKE_SUMMARY)
    second = history_store.save_snapshot("proj-1", FAKE_HEALTH, FAKE_SUMMARY)
    snapshots = history_store.list_snapshots("proj-1")
    assert [s["id"] for s in snapshots] == [second["id"], first["id"]]


def test_list_snapshots_orders_by_id_when_timestamps_collide(monkeypatch, tmp_path):
    """Regresión — bug real encontrado en vivo por el test de arriba, que
    falló de forma intermitente: `created_at` sale de `datetime.now()`, y con
    la granularidad del reloj de Windows dos snapshots guardados en el mismo
    tick quedan con el MISMO timestamp. Sin el desempate por `id DESC`,
    `list_snapshots` los devolvía en orden arbitrario. Acá el empate se
    fuerza (reloj congelado) en vez de esperar a que la suerte lo reproduzca."""
    _use_temp_db(monkeypatch, tmp_path)

    class _FrozenClock:
        @staticmethod
        def now(_tz=None):
            return datetime(2026, 9, 2, 12, 0, 0, tzinfo=UTC)

    monkeypatch.setattr(history_store, "datetime", _FrozenClock)
    first = history_store.save_snapshot("proj-1", FAKE_HEALTH, FAKE_SUMMARY)
    second = history_store.save_snapshot("proj-1", FAKE_HEALTH, FAKE_SUMMARY)

    snapshots = history_store.list_snapshots("proj-1")
    assert snapshots[0]["created_at"] == snapshots[1]["created_at"]  # el empate realmente ocurrió
    assert [s["id"] for s in snapshots] == [second["id"], first["id"]]

    # `project_rollup` comparte el mismo criterio de orden — mismo empate,
    # mismo resultado, o el panorama mostraría el snapshot viejo como "el
    # último".
    entry = history_store.project_rollup()[0]
    assert entry["latest"]["id"] == second["id"]
    assert entry["previous"]["id"] == first["id"]


def test_list_snapshots_filters_by_project(monkeypatch, tmp_path):
    _use_temp_db(monkeypatch, tmp_path)
    history_store.save_snapshot("proj-a", FAKE_HEALTH, FAKE_SUMMARY)
    history_store.save_snapshot("proj-b", FAKE_HEALTH, FAKE_SUMMARY)
    snapshots_a = history_store.list_snapshots("proj-a")
    assert len(snapshots_a) == 1
    assert snapshots_a[0]["project_id"] == "proj-a"


def test_delete_snapshot_removes_it(monkeypatch, tmp_path):
    _use_temp_db(monkeypatch, tmp_path)
    saved = history_store.save_snapshot("proj-1", FAKE_HEALTH, FAKE_SUMMARY)
    assert history_store.delete_snapshot(saved["id"]) is True
    assert history_store.get_snapshot(saved["id"]) is None


def test_delete_snapshot_returns_false_for_missing_id(monkeypatch, tmp_path):
    _use_temp_db(monkeypatch, tmp_path)
    assert history_store.delete_snapshot(999999) is False


# ── project_rollup (Fase 28, 2do bullet: panorama entre proyectos) ───────────


def test_project_rollup_is_empty_without_snapshots(monkeypatch, tmp_path):
    _use_temp_db(monkeypatch, tmp_path)
    assert history_store.project_rollup() == []


def test_project_rollup_returns_latest_and_previous_per_project(monkeypatch, tmp_path):
    _use_temp_db(monkeypatch, tmp_path)
    older = history_store.save_snapshot("proj-1", FAKE_HEALTH, FAKE_SUMMARY)
    newer = history_store.save_snapshot("proj-1", FAKE_HEALTH, FAKE_SUMMARY)

    rollup = history_store.project_rollup()
    assert len(rollup) == 1
    entry = rollup[0]
    assert entry["project_id"] == "proj-1"
    assert entry["snapshots"] == 2
    assert entry["latest"]["id"] == newer["id"]
    assert entry["previous"]["id"] == older["id"]
    # El shape completo viaja, no solo los ids — es lo que el panorama
    # necesita para mostrar scores sin volver a pedir cada snapshot.
    assert entry["latest"]["health"] == FAKE_HEALTH


def test_project_rollup_previous_is_none_with_a_single_snapshot(monkeypatch, tmp_path):
    _use_temp_db(monkeypatch, tmp_path)
    history_store.save_snapshot("proj-1", FAKE_HEALTH, FAKE_SUMMARY)
    entry = history_store.project_rollup()[0]
    assert entry["snapshots"] == 1
    assert entry["previous"] is None


def test_project_rollup_ignores_all_but_the_2_newest_per_project(monkeypatch, tmp_path):
    _use_temp_db(monkeypatch, tmp_path)
    ids = [history_store.save_snapshot("proj-1", FAKE_HEALTH, FAKE_SUMMARY)["id"] for _ in range(5)]
    entry = history_store.project_rollup()[0]
    # `snapshots` cuenta los 5, pero solo se traen los 2 más nuevos.
    assert entry["snapshots"] == 5
    assert entry["latest"]["id"] == ids[-1]
    assert entry["previous"]["id"] == ids[-2]


def test_project_rollup_separates_projects(monkeypatch, tmp_path):
    _use_temp_db(monkeypatch, tmp_path)
    a1 = history_store.save_snapshot("proj-a", FAKE_HEALTH, FAKE_SUMMARY)
    b1 = history_store.save_snapshot("proj-b", FAKE_HEALTH, FAKE_SUMMARY)
    b2 = history_store.save_snapshot("proj-b", FAKE_HEALTH, FAKE_SUMMARY)

    by_id = {e["project_id"]: e for e in history_store.project_rollup()}
    assert set(by_id) == {"proj-a", "proj-b"}
    assert by_id["proj-a"]["latest"]["id"] == a1["id"]
    assert by_id["proj-a"]["previous"] is None
    assert by_id["proj-b"]["latest"]["id"] == b2["id"]
    assert by_id["proj-b"]["previous"]["id"] == b1["id"]
