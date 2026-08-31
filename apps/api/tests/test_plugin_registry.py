"""
Tests — services/plugin_registry.py (Fase 24, Extensibility Platform)

`conftest.py` ya deja el sidecar real corriendo para toda la sesión, así que
`main.py` (importado indirectamente vía `test_complexity_client.py`'s
`TestClient(app)` en otros archivos) ya pobló `_MANIFESTS` de verdad al
arrancar. Estos tests fuerzan explícitamente el estado vacío (monkeypatch)
para ejercitar el camino de respaldo sin depender de si el sidecar respondió
a tiempo en ESTE proceso de pytest en particular.
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

import services.plugin_registry as plugin_registry


class TestExtensionMapWithManifests:
    def test_derives_extension_map_from_loaded_manifests(self, monkeypatch):
        monkeypatch.setattr(
            plugin_registry,
            "_MANIFESTS",
            [
                {
                    "id": "python",
                    "extensions": [".py"],
                    "category": "language",
                    "parser": "x",
                    "features": [],
                    "needs": [],
                    "builtin": True,
                }
            ],
        )
        assert plugin_registry.extension_map() == {".py": "python"}

    def test_manifest_by_id_finds_loaded_manifest(self, monkeypatch):
        manifest = {
            "id": "fortran",
            "extensions": [".f90"],
            "category": "language",
            "parser": "x",
            "features": [],
            "needs": [],
            "builtin": True,
        }
        monkeypatch.setattr(plugin_registry, "_MANIFESTS", [manifest])
        assert plugin_registry.manifest_by_id("fortran") == manifest
        assert plugin_registry.manifest_by_id("nope") is None


class TestExtensionMapFallback:
    """Con `_MANIFESTS` vacío (sidecar no respondió al arranque), todo debe
    seguir funcionando vía `_FALLBACK_MANIFESTS` — mismas 6 extensiones que
    el sidecar reportaría, congeladas acá como piso mínimo."""

    def test_falls_back_when_manifests_empty(self, monkeypatch):
        monkeypatch.setattr(plugin_registry, "_MANIFESTS", [])
        ext_map = plugin_registry.extension_map()
        assert ext_map[".py"] == "python"
        assert ext_map[".f90"] == "fortran"
        assert ext_map[".cpp"] == "cpp"

    def test_fallback_covers_all_seven_languages(self, monkeypatch):
        monkeypatch.setattr(plugin_registry, "_MANIFESTS", [])
        ids = {m["id"] for m in plugin_registry.manifests()}
        assert ids == {"python", "c", "cpp", "javascript", "typescript", "fortran", "assembly"}

    def test_fallback_manifest_by_id(self, monkeypatch):
        monkeypatch.setattr(plugin_registry, "_MANIFESTS", [])
        fortran = plugin_registry.manifest_by_id("fortran")
        assert fortran is not None
        assert ".f90" in fortran["extensions"]
