"""
Tests — Router History (Fase 28, Enterprise Intelligence, primera porción).

Usa el flujo real de upload (mismo patrón que test_upload.py) para tener un
project_id real en el `UPLOADS_DIR` real — solo `history_store.DB_PATH` se
aísla en `tmp_path` por test, así los snapshots de una corrida de tests no
ensucian el `data/history.db` real ni a otros tests.
"""

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from fastapi.testclient import TestClient

from main import app
from services import history_store

client = TestClient(app)


@pytest.fixture(autouse=True)
def _isolated_history_db(monkeypatch, tmp_path):
    monkeypatch.setattr(history_store, "DB_PATH", tmp_path / "history.db")
    history_store.init_db()


def _upload_project(filename: str = "app.py", content: bytes = b"def f():\n    return 1\n") -> str:
    res = client.post("/api/upload/files", files=[("files", (filename, content, "text/plain"))])
    return res.json()["project_id"]


def _health(score: int) -> dict:
    """Health con el MISMO score en las 4 categorías — para los tests de
    orden/promedio del panorama, donde lo que importa es el `overall`
    resultante, no distinguir una categoría de otra."""
    return {
        "security": {"score": score, "high": 0, "medium": 0, "low": 0},
        "quality": {"score": score, "smells": 0, "naming": 0},
        "complexity": {"score": score, "avg_complexity": 1.0},
        "architecture": {"score": score, "cycles": 0, "smells": 0},
    }


class TestCreateSnapshot:
    def test_returns_404_for_missing_project(self):
        res = client.post("/history/does-not-exist/snapshot")
        assert res.status_code == 404

    def test_creates_snapshot_with_health_and_summary(self):
        project_id = _upload_project()
        try:
            res = client.post(f"/history/{project_id}/snapshot")
            assert res.status_code == 200
            data = res.json()
            assert data["project_id"] == project_id
            assert "health" in data and "security" in data["health"]
            assert "summary" in data and "total_files" in data["summary"]
            assert data["id"] is not None
        finally:
            client.delete(f"/api/upload/projects/{project_id}")


class TestListSnapshots:
    def test_lists_saved_snapshots_newest_first(self):
        project_id = _upload_project()
        try:
            first = client.post(f"/history/{project_id}/snapshot").json()
            second = client.post(f"/history/{project_id}/snapshot").json()

            res = client.get(f"/history/{project_id}")
            assert res.status_code == 200
            ids = [s["id"] for s in res.json()]
            assert ids == [second["id"], first["id"]]
        finally:
            client.delete(f"/api/upload/projects/{project_id}")

    def test_empty_list_for_project_with_no_snapshots(self):
        project_id = _upload_project()
        try:
            res = client.get(f"/history/{project_id}")
            assert res.status_code == 200
            assert res.json() == []
        finally:
            client.delete(f"/api/upload/projects/{project_id}")


class TestCompareSnapshots:
    def test_compares_two_snapshots_of_unchanged_project(self):
        # Mismo proyecto, sin cambios entre las 2 corridas -- todos los
        # deltas deben dar 0.
        project_id = _upload_project()
        try:
            first = client.post(f"/history/{project_id}/snapshot").json()
            second = client.post(f"/history/{project_id}/snapshot").json()

            res = client.get(f"/history/compare/{first['id']}/{second['id']}")
            assert res.status_code == 200
            data = res.json()
            assert data["health_diff"]["security"]["delta"] == 0
            assert data["summary_diff"]["total_functions"]["delta"] == 0
            assert data["from"]["id"] == first["id"]
            assert data["to"]["id"] == second["id"]
        finally:
            client.delete(f"/api/upload/projects/{project_id}")

    def test_returns_404_when_either_snapshot_missing(self):
        project_id = _upload_project()
        try:
            snap = client.post(f"/history/{project_id}/snapshot").json()
            res = client.get(f"/history/compare/{snap['id']}/999999")
            assert res.status_code == 404
        finally:
            client.delete(f"/api/upload/projects/{project_id}")


class TestDeleteSnapshot:
    def test_deletes_and_then_404s_on_repeat(self):
        project_id = _upload_project()
        try:
            snap = client.post(f"/history/{project_id}/snapshot").json()
            res = client.delete(f"/history/snapshot/{snap['id']}")
            assert res.status_code == 200
            assert res.json()["deleted"] is True

            res2 = client.delete(f"/history/snapshot/{snap['id']}")
            assert res2.status_code == 404
        finally:
            client.delete(f"/api/upload/projects/{project_id}")

    def test_deleted_snapshot_disappears_from_list(self):
        project_id = _upload_project()
        try:
            snap = client.post(f"/history/{project_id}/snapshot").json()
            client.delete(f"/history/snapshot/{snap['id']}")
            res = client.get(f"/history/{project_id}")
            assert res.json() == []
        finally:
            client.delete(f"/api/upload/projects/{project_id}")


class TestOverview:
    """Panorama entre proyectos — Fase 28, 2do bullet."""

    def test_empty_overview_without_any_snapshot(self):
        res = client.get("/history/overview")
        assert res.status_code == 200
        data = res.json()
        assert data["projects"] == []
        assert data["aggregate"]["projects"] == 0
        assert data["aggregate"]["snapshots"] == 0

    def test_overview_is_not_swallowed_by_the_project_id_route(self):
        """`GET /history/overview` y `GET /history/{project_id}` tienen la
        misma forma (1 segmento) — si `/overview` se declarara DESPUÉS,
        FastAPI lo resolvería como "los snapshots del proyecto llamado
        'overview'" y devolvería una lista vacía en vez del panorama. Este
        test falla (una lista, no un dict) si alguien reordena las rutas."""
        data = client.get("/history/overview").json()
        assert isinstance(data, dict)
        assert "aggregate" in data

    def test_overview_reports_the_latest_snapshot_of_each_project(self):
        project_a = _upload_project()
        project_b = _upload_project(filename="other.py", content=b"x = 1\n")
        try:
            client.post(f"/history/{project_a}/snapshot")
            client.post(f"/history/{project_b}/snapshot")
            latest_b = client.post(f"/history/{project_b}/snapshot").json()

            data = client.get("/history/overview").json()
            by_id = {p["project_id"]: p for p in data["projects"]}
            assert set(by_id) == {project_a, project_b}
            assert by_id[project_a]["snapshots"] == 1
            assert by_id[project_b]["snapshots"] == 2
            assert by_id[project_b]["created_at"] == latest_b["created_at"]
            assert data["aggregate"]["projects"] == 2
            assert data["aggregate"]["snapshots"] == 3
        finally:
            client.delete(f"/api/upload/projects/{project_a}")
            client.delete(f"/api/upload/projects/{project_b}")

    def test_delta_is_null_with_one_snapshot_and_computed_with_two(self):
        project_id = _upload_project()
        try:
            client.post(f"/history/{project_id}/snapshot")
            assert client.get("/history/overview").json()["projects"][0]["delta"] is None

            client.post(f"/history/{project_id}/snapshot")
            entry = client.get("/history/overview").json()["projects"][0]
            # Mismo proyecto sin cambios entre las 2 corridas: delta 0 en
            # las 4 categorías, no `None`.
            assert entry["delta"] == {"security": 0, "quality": 0, "complexity": 0, "architecture": 0}
        finally:
            client.delete(f"/api/upload/projects/{project_id}")

    def test_project_name_and_exists_survive_project_deletion(self):
        """Borrar el proyecto del disco NO borra su historial — la fila sigue
        apareciendo, marcada `exists: false` y sin nombre, en vez de
        desaparecer en silencio."""
        project_id = _upload_project()
        client.post(f"/history/{project_id}/snapshot")

        entry = client.get("/history/overview").json()["projects"][0]
        assert entry["exists"] is True
        assert entry["project_name"]

        client.delete(f"/api/upload/projects/{project_id}")
        entry_after = client.get("/history/overview").json()["projects"][0]
        assert entry_after["project_id"] == project_id
        assert entry_after["exists"] is False
        assert entry_after["project_name"] is None

    def test_projects_are_ordered_worst_health_first(self):
        """El proyecto con peor `overall` va primero — el orden útil para
        "dónde hay que mirar". Se fabrica el escenario escribiendo snapshots
        directamente (no hace falta un proyecto realmente malo en disco)."""
        history_store.save_snapshot("sano", _health(95), {"total_files": 1})
        history_store.save_snapshot("roto", _health(20), {"total_files": 1})

        projects = client.get("/history/overview").json()["projects"]
        assert [p["project_id"] for p in projects] == ["roto", "sano"]
        assert projects[0]["overall"] == 20
        # Promedio simple entre proyectos, sin ponderar por tamaño.
        assert client.get("/history/overview").json()["aggregate"]["avg"]["overall"] == 58


class TestCapabilitiesTracking:
    """Fase 28 (2026-09-05) — un snapshot guarda qué sabía analizar el motor
    cuando se midió. Sin eso, cuando un lenguaje pasa de devolver `[]` fijo a
    emitir hallazgos reales —cosa que pasó 4 veces en este repo— el score de
    ese proyecto baja sin que el código se haya tocado, y comparar contra un
    snapshot viejo muestra una regresión que nunca existió."""

    def test_un_snapshot_nuevo_guarda_las_capacidades(self):
        pid = _upload_project()
        res = client.post(f"/history/{pid}/snapshot")
        assert res.status_code == 200
        caps = res.json()["capabilities"]
        assert isinstance(caps, list) and caps, "un snapshot nuevo tiene que traer capacidades"
        # Formato `lenguaje:feature`, ordenado.
        assert all(":" in c for c in caps)
        assert caps == sorted(caps)
        assert "python:security_findings" in caps

    def test_dos_snapshots_del_mismo_motor_son_comparables(self):
        pid = _upload_project()
        a = client.post(f"/history/{pid}/snapshot").json()["id"]
        b = client.post(f"/history/{pid}/snapshot").json()["id"]
        diff = client.get(f"/history/compare/{a}/{b}").json()["capabilities_diff"]
        assert diff["known"] is True
        assert diff["comparable"] is True
        assert diff["added"] == [] and diff["removed"] == []

    def test_un_analisis_nuevo_marca_la_comparacion_como_no_confiable(self):
        pid = _upload_project()
        viejo = history_store.save_snapshot(pid, _health(100), {}, ["python:functions"])
        nuevo = history_store.save_snapshot(pid, _health(70), {}, ["python:functions", "python:security_findings"])
        diff = client.get(f"/history/compare/{viejo['id']}/{nuevo['id']}").json()["capabilities_diff"]
        assert diff["known"] is True
        assert diff["comparable"] is False
        assert diff["added"] == ["python:security_findings"]
        assert diff["removed"] == []
        assert "no del código" in diff["note"]

    def test_un_snapshot_anterior_a_esta_columna_se_declara_desconocido(self):
        """Las bases que ya existen tienen filas sin capacidades. `None` (no
        `[]`) es lo correcto: "no se sabe" y "no producía nada" son cosas
        distintas, y confundirlas haría que el aviso salga cuando no
        corresponde o se calle cuando sí."""
        pid = _upload_project()
        viejo = history_store.save_snapshot(pid, _health(100), {})
        nuevo = history_store.save_snapshot(pid, _health(70), {}, ["python:security_findings"])
        assert viejo["capabilities"] is None
        diff = client.get(f"/history/compare/{viejo['id']}/{nuevo['id']}").json()["capabilities_diff"]
        assert diff["known"] is False
        assert diff["comparable"] is False
        assert "anterior" in diff["note"]

    def test_la_migracion_no_pierde_los_snapshots_que_ya_existian(self, tmp_path, monkeypatch):
        """Una medición pasada sigue siendo una medición: la columna se agrega
        con ALTER TABLE, no recreando la tabla."""
        import sqlite3

        db = tmp_path / "vieja.db"
        monkeypatch.setattr(history_store, "DB_PATH", db)
        # Esquema anterior, sin `capabilities_json`.
        conn = sqlite3.connect(db)
        with conn:
            conn.execute(
                "CREATE TABLE snapshots (id INTEGER PRIMARY KEY AUTOINCREMENT, project_id TEXT NOT NULL, "
                "created_at TEXT NOT NULL, health_json TEXT NOT NULL, summary_json TEXT NOT NULL)"
            )
            conn.execute(
                "INSERT INTO snapshots (project_id, created_at, health_json, summary_json) VALUES (?, ?, ?, ?)",
                ("p1", "2026-01-01T00:00:00+00:00", "{}", "{}"),
            )
        conn.close()

        history_store.init_db()
        filas = history_store.list_snapshots("p1")
        assert len(filas) == 1, "la migracion borro un snapshot que ya existia"
        assert filas[0]["capabilities"] is None


class TestScoringVersion:
    """La formula de `quality` cambio el 2026-10-02 (lineal con recorte ->
    caida exponencial). Dos snapshots con formulas distintas NO son
    comparables, y la comparacion tiene que decirlo en vez de mostrar un
    delta que mezcla un cambio de codigo con un cambio de calculo.

    Mismo problema y misma forma que `_capabilities_diff`, que ya existia
    para "el analizador aprendio a ver algo nuevo".
    """

    def _snap(self, project_id: str, score: int, version: int | None) -> int:
        health = _health(score)
        if version is not None:
            health["scoring_version"] = version
        return history_store.save_snapshot(project_id=project_id, health=health, summary={}, capabilities=None)["id"]

    def test_misma_version_se_declara_comparable(self):
        a = self._snap("sv-1", 80, 2)
        b = self._snap("sv-1", 90, 2)
        d = client.get(f"/history/compare/{a}/{b}").json()["scoring_diff"]
        assert d["comparable"] is True
        assert d["note"] == ""

    def test_versiones_distintas_avisan_que_el_delta_no_es_del_codigo(self):
        a = self._snap("sv-2", 80, 1)
        b = self._snap("sv-2", 90, 2)
        d = client.get(f"/history/compare/{a}/{b}").json()["scoring_diff"]
        assert d["comparable"] is False
        assert d["from"] == 1 and d["to"] == 2
        assert "formulas" in d["note"].lower() or "fórmulas" in d["note"].lower()

    def test_un_snapshot_sin_el_campo_cuenta_como_v1(self):
        # Los snapshots guardados ANTES de que el campo existiera son v1 por
        # definicion — que es justo el que no es comparable con el actual.
        a = self._snap("sv-3", 80, None)
        b = self._snap("sv-3", 85, 2)
        d = client.get(f"/history/compare/{a}/{b}").json()["scoring_diff"]
        assert d["from"] == 1
        assert d["comparable"] is False


class TestQualityNoSatura:
    """**El defecto que motivo el cambio.** La formula vieja llegaba a 0 con
    una densidad de 1/3 smells por unidad y de ahi en adelante no informaba
    nada: densidad 0.34 y densidad 3.0 mostraban el mismo 0, asi que arreglar
    la mitad de los smells no movia el numero.

    Medido sobre 47 paquetes reales de `site-packages`: densidad mediana
    0.14, maxima 0.77, y 6 de los 47 saturados en 0.
    """

    def _quality(self, files: list[dict]) -> dict:
        r = client.post("/static/parse-project", json={"files": files})
        assert r.status_code == 200
        return r.json()["health"]["quality"]

    # Una función larga con demasiados parámetros: dispara `long_function`,
    # `excessive_parameters` y `overwritten_store` (la escritura repetida se
    # pisa antes de leerse). Nombres largos a propósito, para que el término
    # de naming no se mezcle con el que se está midiendo.
    SUCIA = (
        "def procesar_lote_{n}(primero, segundo, tercero, cuarto, quinto, sexto):\n"
        + "    acumulador = 1\n" * 55
        + "    return acumulador\n"
    )

    @staticmethod
    def _limpias(cuantas: int) -> str:
        return "".join(f"def calcular_{i}(primero, segundo):\n    return primero + segundo\n" for i in range(cuantas))

    def test_una_densidad_realista_conserva_resolucion(self):
        """**El defecto, en su forma exacta.** Con la fórmula vieja, una
        densidad de 1/3 smells por unidad daba 0 — y de ahí para arriba todo
        daba 0 igual. Dos proyectos con la mitad y el doble de smells se veían
        idénticos, así que arreglar la mitad no movía el número.

        Diez funciones limpias más una con smells da penalización 82: con la
        fórmula vieja eso era 18, con ésta 44. Medido sobre 47 paquetes reales
        de `site-packages`, la saturación en 0 bajó de 13% a 2%.
        """
        q_limpio = self._quality([{"filename": "a.py", "content": self._limpias(10)}])
        q_sucio = self._quality([{"filename": "b.py", "content": self._limpias(10) + self.SUCIA.format(n=1)}])

        assert q_limpio["score"] == 100
        assert q_sucio["smells"] > 0, "el fixture tiene que disparar smells"
        assert q_sucio["score"] < 100, "con smells el score baja"
        assert q_sucio["score"] > 0, (
            "una densidad realista no puede saturar en 0: ahí el score deja de "
            f"mostrar progreso (smells={q_sucio['smells']}, naming={q_sucio['naming']})"
        )

    def test_mas_smells_baja_mas_el_score(self):
        """Monotonía: el score tiene que distinguir peor de mucho peor, que es
        exactamente lo que la fórmula vieja perdía al recortar en 0."""
        base = self._limpias(20)
        q1 = self._quality([{"filename": "a.py", "content": base + self.SUCIA.format(n=1)}])
        q3 = self._quality(
            [
                {
                    "filename": "b.py",
                    "content": base + "".join(self.SUCIA.format(n=i) for i in range(3)),
                }
            ]
        )
        assert q3["smells"] > q1["smells"]
        assert q3["score"] < q1["score"], f"más smells tiene que puntuar menos: {q1['score']} -> {q3['score']}"
        assert q3["score"] > 0, "ninguna de las dos densidades es extrema"

    def test_codigo_limpio_sigue_dando_100(self):
        q = self._quality([{"filename": "clean.py", "content": "def add(a, b):\n    return a + b\n"}])
        assert q["score"] == 100

    def test_la_version_de_scoring_viaja_en_el_health(self):
        r = client.post(
            "/static/parse-project",
            json={"files": [{"filename": "clean.py", "content": "def add(a, b):\n    return a + b\n"}]},
        )
        assert r.json()["health"]["scoring_version"] >= 2
