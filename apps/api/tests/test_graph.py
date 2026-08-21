"""
Tests — Code Graph Visual Router
pytest tests/test_graph.py -v
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from fastapi.testclient import TestClient
from main import app

client = TestClient(app)

# ─── Fixtures ────────────────────────────────────────────────────────────────

FILES_SIMPLE = [
    {
        "filename": "main.py",
        "content": "from parser import parse\nfrom analyzer import analyze\n\ndef main():\n    data = parse('x')\n    return analyze(data)\n",
    },
    {"filename": "parser.py", "content": "from lexer import tokenize\n\ndef parse(src):\n    return tokenize(src)\n"},
    {
        "filename": "analyzer.py",
        "content": "from metrics import compute\n\ndef analyze(data):\n    return compute(data)\n",
    },
    {"filename": "metrics.py", "content": "def compute(data):\n    return len(data)\n"},
    {"filename": "lexer.py", "content": "def tokenize(src):\n    return src.split()\n"},
]

FILES_CIRCULAR = [
    {"filename": "a.py", "content": "from b import b_func\n\ndef a_func():\n    return b_func()\n"},
    {"filename": "b.py", "content": "from c import c_func\n\ndef b_func():\n    return c_func()\n"},
    {"filename": "c.py", "content": "from a import a_func\n\ndef c_func():\n    return a_func()\n"},
]

FILES_HEATMAP = [
    {
        "filename": "hot.py",
        "content": "def critical(n):\n    for i in range(n):\n        for j in range(n):\n            for k in range(n):\n                pass\n\ndef simple(x):\n    return x * 2\n",
    },
    {
        "filename": "cold.py",
        "content": "def fast(x):\n    return x\n\ndef medium(arr):\n    for i in range(len(arr)):\n        pass\n    return arr\n",
    },
]

FILES_HUB = [
    {"filename": "utils.py", "content": "def helper():\n    return 1\n"},
    {"filename": "a.py", "content": "import utils\n"},
    {"filename": "b.py", "content": "import utils\n"},
    {"filename": "c.py", "content": "import utils\nimport a\n"},
]

FILES_EMPTY = []


# ─── /analyze/graph/types ────────────────────────────────────────────────────


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


# ─── /analyze/graph — Import Graph ───────────────────────────────────────────


class TestImportGraph:
    def test_import_ok(self):
        r = client.post("/analyze/graph", json={"files": FILES_SIMPLE, "graph_type": "import"})
        assert r.status_code == 200

    def test_import_graph_type(self):
        data = client.post("/analyze/graph", json={"files": FILES_SIMPLE, "graph_type": "import"}).json()
        assert data["graph_type"] == "import"

    def test_import_nodes_count(self):
        data = client.post("/analyze/graph", json={"files": FILES_SIMPLE, "graph_type": "import"}).json()
        assert len(data["nodes"]) == 5

    def test_import_has_edges(self):
        data = client.post("/analyze/graph", json={"files": FILES_SIMPLE, "graph_type": "import"}).json()
        assert len(data["edges"]) >= 1

    def test_import_node_structure(self):
        data = client.post("/analyze/graph", json={"files": FILES_SIMPLE, "graph_type": "import"}).json()
        for n in data["nodes"]:
            assert "id" in n
            assert "label" in n
            assert "language" in n
            assert "functions" in n
            assert "imports" in n

    def test_import_edge_structure(self):
        data = client.post("/analyze/graph", json={"files": FILES_SIMPLE, "graph_type": "import"}).json()
        for e in data["edges"]:
            assert "from" in e
            assert "to" in e
            assert "via" in e

    def test_import_has_mermaid(self):
        data = client.post("/analyze/graph", json={"files": FILES_SIMPLE, "graph_type": "import"}).json()
        assert "mermaid" in data
        assert "flowchart" in data["mermaid"]
        assert len(data["mermaid"]) > 20

    def test_import_summary(self):
        data = client.post("/analyze/graph", json={"files": FILES_SIMPLE, "graph_type": "import"}).json()
        s = data["summary"]
        assert s["total_files"] == 5
        assert s["total_imports"] >= 1

    def test_import_entry_points(self):
        data = client.post("/analyze/graph", json={"files": FILES_SIMPLE, "graph_type": "import"}).json()
        assert "entry_points" in data

    def test_import_empty_files(self):
        data = client.post("/analyze/graph", json={"files": FILES_EMPTY, "graph_type": "import"}).json()
        assert data["nodes"] == []
        assert "mermaid" in data

    def test_import_single_file(self):
        r = client.post("/analyze/graph", json={"files": [FILES_SIMPLE[0]], "graph_type": "import"})
        assert r.status_code == 200


# ─── /analyze/graph — Call Graph ─────────────────────────────────────────────


class TestCallGraph:
    def test_call_ok(self):
        r = client.post("/analyze/graph", json={"files": FILES_SIMPLE, "graph_type": "call"})
        assert r.status_code == 200

    def test_call_graph_type(self):
        data = client.post("/analyze/graph", json={"files": FILES_SIMPLE, "graph_type": "call"}).json()
        assert data["graph_type"] == "call"

    def test_call_has_nodes(self):
        data = client.post("/analyze/graph", json={"files": FILES_SIMPLE, "graph_type": "call"}).json()
        assert len(data["nodes"]) >= 1

    def test_call_node_has_big_o(self):
        data = client.post("/analyze/graph", json={"files": FILES_HEATMAP, "graph_type": "call"}).json()
        for n in data["nodes"]:
            assert "big_o" in n
            assert "cc" in n
            assert "color" in n
            assert "level" in n

    def test_call_has_mermaid(self):
        data = client.post("/analyze/graph", json={"files": FILES_SIMPLE, "graph_type": "call"}).json()
        assert "flowchart" in data["mermaid"]

    def test_call_summary(self):
        data = client.post("/analyze/graph", json={"files": FILES_HEATMAP, "graph_type": "call"}).json()
        s = data["summary"]
        assert "total_functions" in s
        assert "hot_paths" in s

    def test_call_hot_paths_expensive(self):
        data = client.post("/analyze/graph", json={"files": FILES_HEATMAP, "graph_type": "call"}).json()
        hot = data["summary"]["hot_paths"]
        if hot:
            for h in hot:
                assert h["level"] == "expensive"

    def test_call_empty(self):
        data = client.post("/analyze/graph", json={"files": FILES_EMPTY, "graph_type": "call"}).json()
        assert data["nodes"] == []


# ─── /analyze/graph — Circular Dependencies ──────────────────────────────────


class TestCircularGraph:
    def test_circular_ok(self):
        r = client.post("/analyze/graph", json={"files": FILES_CIRCULAR, "graph_type": "circular"})
        assert r.status_code == 200

    def test_circular_detects_cycle(self):
        data = client.post("/analyze/graph", json={"files": FILES_CIRCULAR, "graph_type": "circular"}).json()
        assert data["has_cycles"] is True
        assert len(data["cycles"]) >= 1

    def test_circular_no_cycle(self):
        data = client.post("/analyze/graph", json={"files": FILES_SIMPLE, "graph_type": "circular"}).json()
        # FILES_SIMPLE no tiene ciclos
        assert data["has_cycles"] is False

    def test_circular_cycle_contains_files(self):
        data = client.post("/analyze/graph", json={"files": FILES_CIRCULAR, "graph_type": "circular"}).json()
        all_cycle_files = set()
        for c in data["cycles"]:
            all_cycle_files.update(c)
        assert "a.py" in all_cycle_files or "b.py" in all_cycle_files or "c.py" in all_cycle_files

    def test_circular_nodes_marked(self):
        data = client.post("/analyze/graph", json={"files": FILES_CIRCULAR, "graph_type": "circular"}).json()
        in_cycle = [n for n in data["nodes"] if n["in_cycle"]]
        assert len(in_cycle) >= 2

    def test_circular_edges_marked(self):
        data = client.post("/analyze/graph", json={"files": FILES_CIRCULAR, "graph_type": "circular"}).json()
        cycle_edges = [e for e in data["edges"] if e.get("is_cycle")]
        assert len(cycle_edges) >= 1

    def test_circular_mermaid_has_cycle_style(self):
        data = client.post("/analyze/graph", json={"files": FILES_CIRCULAR, "graph_type": "circular"}).json()
        # Nodos en ciclo deben tener estilo rojo
        assert "ff3366" in data["mermaid"]

    def test_circular_summary(self):
        data = client.post("/analyze/graph", json={"files": FILES_CIRCULAR, "graph_type": "circular"}).json()
        s = data["summary"]
        assert s["total_cycles"] >= 1
        assert s["affected_files"] >= 2
        assert len(s["cycle_descriptions"]) >= 1

    def test_circular_no_cycle_mermaid_ok(self):
        data = client.post("/analyze/graph", json={"files": FILES_SIMPLE, "graph_type": "circular"}).json()
        assert "Sin dependencias circulares" in data["mermaid"]

    def test_circular_empty(self):
        data = client.post("/analyze/graph", json={"files": FILES_EMPTY, "graph_type": "circular"}).json()
        assert data["has_cycles"] is False


# ─── /analyze/graph — Complexity Heatmap ─────────────────────────────────────


class TestHeatmap:
    def test_heatmap_ok(self):
        r = client.post("/analyze/graph", json={"files": FILES_HEATMAP, "graph_type": "heatmap"})
        assert r.status_code == 200

    def test_heatmap_graph_type(self):
        data = client.post("/analyze/graph", json={"files": FILES_HEATMAP, "graph_type": "heatmap"}).json()
        assert data["graph_type"] == "heatmap"

    def test_heatmap_has_functions(self):
        data = client.post("/analyze/graph", json={"files": FILES_HEATMAP, "graph_type": "heatmap"}).json()
        assert len(data["functions"]) >= 2

    def test_heatmap_function_structure(self):
        data = client.post("/analyze/graph", json={"files": FILES_HEATMAP, "graph_type": "heatmap"}).json()
        for fn in data["functions"]:
            assert "name" in fn
            assert "file" in fn
            assert "cc" in fn
            assert "cc_color" in fn
            assert "cc_level" in fn
            assert "big_o" in fn
            assert "bigo_color" in fn
            assert "bigo_level" in fn

    def test_heatmap_cc_levels_valid(self):
        data = client.post("/analyze/graph", json={"files": FILES_HEATMAP, "graph_type": "heatmap"}).json()
        valid = {"low", "medium", "high", "critical"}
        for fn in data["functions"]:
            assert fn["cc_level"] in valid

    def test_heatmap_bigo_levels_valid(self):
        data = client.post("/analyze/graph", json={"files": FILES_HEATMAP, "graph_type": "heatmap"}).json()
        valid = {"efficient", "moderate", "expensive"}
        for fn in data["functions"]:
            assert fn["bigo_level"] in valid

    def test_heatmap_hot_function_detected(self):
        data = client.post("/analyze/graph", json={"files": FILES_HEATMAP, "graph_type": "heatmap"}).json()
        # critical() tiene 3 loops → O(n³) → expensive
        hot = [f for f in data["functions"] if f["bigo_level"] == "expensive"]
        assert len(hot) >= 1

    def test_heatmap_sorted_worst_first(self):
        data = client.post("/analyze/graph", json={"files": FILES_HEATMAP, "graph_type": "heatmap"}).json()
        fns = data["functions"]
        # El primer elemento debe ser peor o igual que el último
        if len(fns) >= 2:
            first_level = {"low": 0, "medium": 1, "high": 2, "critical": 3}.get(fns[0]["cc_level"], 0)
            last_level = {"low": 0, "medium": 1, "high": 2, "critical": 3}.get(fns[-1]["cc_level"], 0)
            assert first_level >= last_level

    def test_heatmap_mermaid_subgraph(self):
        data = client.post("/analyze/graph", json={"files": FILES_HEATMAP, "graph_type": "heatmap"}).json()
        assert "subgraph" in data["mermaid"]

    def test_heatmap_summary(self):
        data = client.post("/analyze/graph", json={"files": FILES_HEATMAP, "graph_type": "heatmap"}).json()
        s = data["summary"]
        assert "total_functions" in s
        assert "avg_cc" in s
        assert "by_level" in s
        assert "hot_paths" in s

    def test_heatmap_by_level_sums(self):
        data = client.post("/analyze/graph", json={"files": FILES_HEATMAP, "graph_type": "heatmap"}).json()
        s = data["summary"]
        total = sum(s["by_level"].values())
        assert total == s["total_functions"]

    def test_heatmap_colors_are_hex(self):
        data = client.post("/analyze/graph", json={"files": FILES_HEATMAP, "graph_type": "heatmap"}).json()
        for fn in data["functions"]:
            assert fn["cc_color"].startswith("#")
            assert fn["bigo_color"].startswith("#")

    def test_heatmap_empty(self):
        data = client.post("/analyze/graph", json={"files": FILES_EMPTY, "graph_type": "heatmap"}).json()
        assert data["functions"] == []


# ─── /analyze/graph — Centrality / Hub Detection (Fase 14) ───────────────────


class TestCentralityGraph:
    def test_centrality_ok(self):
        r = client.post("/analyze/graph", json={"files": FILES_HUB, "graph_type": "centrality"})
        assert r.status_code == 200

    def test_centrality_detects_hub(self):
        data = client.post("/analyze/graph", json={"files": FILES_HUB, "graph_type": "centrality"}).json()
        assert "utils.py" in data["summary"]["hubs"]

    def test_centrality_hub_has_highest_in_degree(self):
        data = client.post("/analyze/graph", json={"files": FILES_HUB, "graph_type": "centrality"}).json()
        utils_node = next(n for n in data["nodes"] if n["id"] == "utils.py")
        assert utils_node["in_degree"] == 3
        assert utils_node["is_hub"] is True

    def test_centrality_leaf_not_hub(self):
        data = client.post("/analyze/graph", json={"files": FILES_HUB, "graph_type": "centrality"}).json()
        b_node = next(n for n in data["nodes"] if n["id"] == "b.py")
        assert b_node["is_hub"] is False

    def test_centrality_no_edges_no_hubs(self):
        files = [{"filename": "a.py", "content": "x = 1\n"}, {"filename": "b.py", "content": "y = 2\n"}]
        data = client.post("/analyze/graph", json={"files": files, "graph_type": "centrality"}).json()
        assert data["summary"]["hubs"] == []

    def test_centrality_mermaid_marks_hub(self):
        data = client.post("/analyze/graph", json={"files": FILES_HUB, "graph_type": "centrality"}).json()
        assert "[HUB]" in data["mermaid"]

    def test_centrality_empty(self):
        data = client.post("/analyze/graph", json={"files": FILES_EMPTY, "graph_type": "centrality"}).json()
        assert data["summary"]["hubs"] == []
        assert data["nodes"] == []

    def test_centrality_appears_in_graph_types(self):
        data = client.get("/analyze/graph/types").json()
        ids = [t["id"] for t in data["types"]]
        assert "centrality" in ids
