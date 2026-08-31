"""
Tests — Code Graph Visual Router
pytest tests/test_graph.py -v

La mayoría de lo que este archivo probaba antes (nodos/edges/mermaid/cycles/
hubs/heatmap de Import/Call/Circular/Heatmap/Centrality graph) era relay
puro a `services/complexity/src/graph.rs` — `_build_import_graph`/
`_build_call_graph`/etc. en `routers/graph.py` devuelven el resultado de
`build_*_graph_rust()` verbatim, incluyendo el string `mermaid` (también
generado en Rust, no en Python). Esos tests se eliminaron (2026-08-31,
pedido explícito del usuario de aplicar "cero tests de Python para código
Rust-only" a toda la carpeta) — `graph.rs` tiene su propio `cargo test`
cubriendo cycles/hubs/heatmap/mermaid con más detalle del que estos tests
alcanzaban a verificar por HTTP. Lo que queda: `TestGraphTypes` (un dict
Python estático, no un relay) y el camino "sin archivos" de cada tipo de
grafo (`generate_graph` corta antes de llamar a Rust cuando `files=[]` —
comportamiento 100% Python).
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from fastapi.testclient import TestClient
from main import app

client = TestClient(app)

FILES_EMPTY = []


class TestGraphTypes:
    def test_types_ok(self):
        r = client.get("/analyze/graph/types")
        assert r.status_code == 200

    def test_types_returns_5(self):
        data = client.get("/analyze/graph/types").json()
        assert len(data["types"]) == 5

    def test_types_ids(self):
        data = client.get("/analyze/graph/types").json()
        ids = [t["id"] for t in data["types"]]
        assert "import" in ids
        assert "call" in ids
        assert "circular" in ids
        assert "heatmap" in ids
        assert "centrality" in ids

    def test_types_structure(self):
        data = client.get("/analyze/graph/types").json()
        for t in data["types"]:
            assert "id" in t
            assert "label" in t
            assert "description" in t

    def test_centrality_appears_in_graph_types(self):
        data = client.get("/analyze/graph/types").json()
        ids = [t["id"] for t in data["types"]]
        assert "centrality" in ids


class TestEmptyFilesShortCircuitsBeforeRust:
    """`generate_graph` (routers/graph.py) devuelve `_empty_response()` sin
    llamar al sidecar cuando `files=[]` — comportamiento Python-only, no
    duplica ningún test de `graph.rs`."""

    def test_import_empty_files(self):
        data = client.post("/analyze/graph", json={"files": FILES_EMPTY, "graph_type": "import"}).json()
        assert data["nodes"] == []
        assert "mermaid" in data

    def test_call_empty(self):
        data = client.post("/analyze/graph", json={"files": FILES_EMPTY, "graph_type": "call"}).json()
        assert data["nodes"] == []

    def test_circular_empty(self):
        data = client.post("/analyze/graph", json={"files": FILES_EMPTY, "graph_type": "circular"}).json()
        assert data["has_cycles"] is False

    def test_heatmap_empty(self):
        data = client.post("/analyze/graph", json={"files": FILES_EMPTY, "graph_type": "heatmap"}).json()
        assert data["functions"] == []

    def test_centrality_empty(self):
        data = client.post("/analyze/graph", json={"files": FILES_EMPTY, "graph_type": "centrality"}).json()
        assert data["summary"]["hubs"] == []
        assert data["nodes"] == []
