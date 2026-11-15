"""
Router: Binary/Executable Analysis
Fase 19 (Machine Intelligence) — segundo bullet, cierra la fase.

Analiza la estructura de un ejecutable/librería YA COMPILADO (.exe/.dll/.so/
.dylib/.o, sin filtro de extensión — el formato se detecta por firma binaria,
no por nombre): secciones, símbolos, imports/exports, vía el sidecar Rust
(`services/complexity/src/binparse.rs`, crate `object` — la misma librería
que usan rustc/lld/wasmtime para esto, no un disassembler hand-rolled).

Merece su propio router en vez de vivir en `static_analysis.py`: ese router
despacha por EXTENSIÓN hacia un parser de TEXTO fuente
(`_LANGUAGE_HANDLERS` lee el contenido como UTF-8) — un binario compilado no
es texto fuente y no encaja en ese modelo de "plugin de lenguaje" en
absoluto, aunque ambos terminen mostrándose en el mismo panel Static del
frontend.

Puro relay Rust: cero lógica Python real más allá de leer el upload y
reenviar los bytes — por eso NO tiene un archivo de tests propio, mismo
criterio que el resto de los endpoints Rust-only puros de la Fase 26
(`cargo test` en `binparse.rs` es la única verificación de correctitud)."""

from fastapi import APIRouter, File, UploadFile

from services.complexity_client import parse_executable_rust

router = APIRouter()

# Mismo límite que el `DefaultBodyLimit` de la ruta `/parse/executable` del
# sidecar (`main.rs`) — rechazar acá, antes de gastar la ida y vuelta HTTP al
# sidecar, un archivo que de todos modos va a ser rechazado del otro lado.
_MAX_UPLOAD_BYTES = 64 * 1024 * 1024


def _degraded(note: str) -> dict:
    return {
        "available": False,
        "format": "unknown",
        "architecture": "unknown",
        "is_64_bit": False,
        "entry_point": 0,
        "sections": [],
        "symbols": [],
        "imports": [],
        "exports": [],
        "truncated": False,
        "note": note,
    }


@router.post("/parse")
async def parse_executable(file: UploadFile = File(...)):
    """Sube un ejecutable/librería compilado y devuelve su estructura real:
    formato (PE/ELF/Mach-O/COFF/wasm), arquitectura, punto de entrada,
    secciones, símbolos, imports/exports. Degrada con gracia (mismo shape
    de respuesta, `available: false`) si el archivo excede el límite de
    tamaño o si el sidecar Rust no está disponible — nunca un 500 crudo."""
    content = await file.read()
    if len(content) > _MAX_UPLOAD_BYTES:
        mb = _MAX_UPLOAD_BYTES // (1024 * 1024)
        return _degraded(f"el archivo pesa más de {mb}MB — no se analiza.")

    result = await parse_executable_rust(content)
    return result if result is not None else _degraded("sidecar Rust no disponible")
