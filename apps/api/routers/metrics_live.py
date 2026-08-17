"""
Router: Live Metrics + Safe Mode + Session Restore
Endpoints para v4.3:

POST /metrics/live      → Métricas instantáneas de un archivo (LOC, funciones, imports, CC, Big-O, parse time)
POST /metrics/validate  → Validar si un archivo es parseble, detectar corrupción
GET  /metrics/health    → Estado del parser engine
"""

from __future__ import annotations

import ast
import re
import time
from pathlib import Path
from typing import Any

from fastapi import APIRouter
from pydantic import BaseModel

from services.static_parser import _cyclomatic_python
from services.complexity_client import check_complexity_engine

router = APIRouter()


# ── Schemas ───────────────────────────────────────────────────────────────────


class LiveMetricsRequest(BaseModel):
    filename: str = "script.py"
    content: str = ""


class ValidateRequest(BaseModel):
    filename: str
    content: str


# ══════════════════════════════════════════════════════════════════════════════
#  LIVE METRICS
#  Análisis ultrarrápido para la barra de métricas del editor.
#  Target: < 5ms. Sin subprocess.
# ══════════════════════════════════════════════════════════════════════════════


@router.post("/live")
async def live_metrics(req: LiveMetricsRequest) -> dict[str, Any]:
    """
    Métricas instantáneas para mostrar en la barra del editor mientras se escribe.
    Incluye: LOC, funciones, clases, imports, CC promedio, Big-O peor caso, parse time.
    """
    t0 = time.perf_counter()
    ext = Path(req.filename).suffix.lower()

    result: dict[str, Any] = {
        "filename": req.filename,
        "language": _detect_language(ext),
        "loc": 0,
        "sloc": 0,
        "functions": 0,
        "classes": 0,
        "imports": 0,
        "avg_cc": 0.0,
        "max_cc": 0,
        "big_o_worst": "?",
        "big_o_dist": {},
        "parse_ok": True,
        "parse_error": None,
        "safe_mode": False,
        "ms": 0,
    }

    if not req.content.strip():
        result["ms"] = 0
        return result

    lines = req.content.splitlines()
    result["loc"] = len(lines)
    result["sloc"] = sum(1 for ln in lines if ln.strip() and not ln.strip().startswith(("#", "//")))

    try:
        if ext == ".py":
            _metrics_python(req.content, result)
        elif ext in (".ts", ".tsx", ".js", ".jsx"):
            _metrics_js_ts(req.content, result)
        elif ext in (".c", ".cpp", ".h", ".hpp"):
            _metrics_c(req.content, result)
        else:
            _metrics_generic(req.content, result)
    except SyntaxError as e:
        result["parse_ok"] = False
        result["parse_error"] = f"SyntaxError: {e.msg} (línea {e.lineno})"
        result["safe_mode"] = True
        # Fallback: métricas básicas por regex aunque haya error de sintaxis
        _metrics_fallback_regex(req.content, ext, result)
    except Exception as e:
        result["parse_ok"] = False
        result["parse_error"] = str(e)
        result["safe_mode"] = True
        _metrics_fallback_regex(req.content, ext, result)

    result["ms"] = round((time.perf_counter() - t0) * 1000, 2)
    return result


def _metrics_python(content: str, result: dict) -> None:
    """Métricas Python via AST."""
    tree = ast.parse(content)
    funcs: list[ast.FunctionDef | ast.AsyncFunctionDef] = []
    classes: list[ast.ClassDef] = []
    imports = 0

    for node in ast.walk(tree):
        if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef):
            funcs.append(node)
        elif isinstance(node, ast.ClassDef):
            classes.append(node)
        elif isinstance(node, ast.Import | ast.ImportFrom):
            imports += 1

    # Complejidad ciclomática simple
    cc_values: list[int] = []
    big_o_list: list[str] = []
    BIG_O_ORDER = ["O(2^n)", "O(n³)", "O(n²)", "O(n log n)", "O(n)", "O(log n)", "O(1)"]

    for fn in funcs:
        cc = _cyclomatic_python(fn)
        cc_values.append(cc)
        bigo = _big_o_python(fn)
        big_o_list.append(bigo)

    result["functions"] = len(funcs)
    result["classes"] = len(classes)
    result["imports"] = imports
    result["avg_cc"] = round(sum(cc_values) / len(cc_values), 1) if cc_values else 0.0
    result["max_cc"] = max(cc_values) if cc_values else 0
    result["big_o_worst"] = _worst_big_o(big_o_list, BIG_O_ORDER)
    result["big_o_dist"] = _big_o_distribution(big_o_list)


def _metrics_js_ts(content: str, result: dict) -> None:
    """Métricas JS/TS via regex."""
    fn_count = len(re.findall(r"\bfunction\s+\w+\s*\(|=>\s*\{|=>\s*[^{]", content))
    cls_count = len(re.findall(r"\bclass\s+\w+", content))
    imp_count = len(re.findall(r"^import\s+", content, re.MULTILINE))

    # CC simple: contar keywords de control
    cc_tokens = len(re.findall(r"\b(if|else|for|while|switch|catch|&&|\|\||\?)\b", content))
    avg_cc = round(cc_tokens / max(fn_count, 1), 1)

    # Big-O heurístico: contar loops totales y detectar si alguno está anidado
    # dentro de otro. Ojo: "nested" es un booleano (¿hay anidamiento?), no un
    # contador — encontrar el patrón "for...{...for" UNA vez ya implica 2
    # loops anidados, así que no se puede usar como conteo de profundidad.
    loop_count = len(re.findall(r"\b(?:for|while)\s*\(", content))
    has_nested = bool(re.search(r"(?:for|while)\s*\([^)]*\)\s*\{[^{}]*(?:for|while)\s*\(", content, re.DOTALL))
    if loop_count == 0:
        bigo = "O(1)"
    elif has_nested:
        bigo = "O(n²)"
    else:
        bigo = "O(n)"

    result["functions"] = fn_count
    result["classes"] = cls_count
    result["imports"] = imp_count
    result["avg_cc"] = avg_cc
    result["max_cc"] = cc_tokens
    result["big_o_worst"] = bigo
    result["big_o_dist"] = {bigo: fn_count} if fn_count else {}


def _metrics_c(content: str, result: dict) -> None:
    """Métricas C/C++ via regex."""
    # Funciones: tipo nombre(params) {
    fn_count = len(re.findall(r"\w+\s+\w+\s*\([^)]*\)\s*\{", content))
    cls_count = len(re.findall(r"\b(class|struct)\s+\w+", content))
    imp_count = len(re.findall(r'#include\s*[<"]', content))
    cc_tokens = len(re.findall(r"\b(if|else|for|while|switch|case|&&|\|\|)\b", content))
    nested = len(re.findall(r"for\s*\(.*\)\s*\{[^}]*for", content, re.DOTALL))
    bigo = "O(n²)" if nested >= 2 else ("O(n)" if nested == 1 else "O(1)")

    result["functions"] = fn_count
    result["classes"] = cls_count
    result["imports"] = imp_count
    result["avg_cc"] = round(cc_tokens / max(fn_count, 1), 1)
    result["max_cc"] = cc_tokens
    result["big_o_worst"] = bigo
    result["big_o_dist"] = {bigo: fn_count} if fn_count else {}


def _metrics_generic(content: str, result: dict) -> None:
    """Métricas genéricas por regex para lenguajes no soportados."""
    _metrics_fallback_regex(content, "", result)


def _metrics_fallback_regex(content: str, ext: str, result: dict) -> None:
    """Safe mode: regex puro cuando el AST falla. Siempre funciona."""
    lines = content.splitlines()
    result["sloc"] = sum(1 for ln in lines if ln.strip() and not ln.strip().startswith(("#", "//", "/*", "*")))

    if ext == ".py":
        result["functions"] = len(re.findall(r"^\s*(?:async\s+)?def\s+\w+\s*\(", content, re.MULTILINE))
        result["classes"] = len(re.findall(r"^\s*class\s+\w+", content, re.MULTILINE))
        result["imports"] = len(re.findall(r"^(?:import|from)\s+", content, re.MULTILINE))
    else:
        result["functions"] = len(re.findall(r"\bfunction\s+\w+|\w+\s*=\s*(?:async\s*)?\(", content))
        result["classes"] = len(re.findall(r"\bclass\s+\w+", content))
        result["imports"] = len(re.findall(r"^(?:import|#include|require)\s+", content, re.MULTILINE))

    result["big_o_worst"] = "?"
    result["avg_cc"] = 0.0


# ── Helpers Big-O ─────────────────────────────────────────────────────────────


def _big_o_python(fn: ast.FunctionDef | ast.AsyncFunctionDef) -> str:
    loops = 0
    has_recursion = False
    name = fn.name

    for node in ast.walk(fn):
        if isinstance(node, ast.For | ast.While):
            loops += 1
        if isinstance(node, ast.Call):
            if isinstance(node.func, ast.Name) and node.func.id == name:
                has_recursion = True

    if has_recursion:
        return "O(2^n)"
    if loops >= 3:
        return "O(n³)"
    if loops == 2:
        return "O(n²)"
    if loops == 1:
        # Detectar división por 2 → log n
        for node in ast.walk(fn):
            if isinstance(node, ast.AugAssign):
                if isinstance(node.op, ast.FloorDiv):
                    return "O(log n)"
        return "O(n)"
    return "O(1)"


def _worst_big_o(bigo_list: list[str], order: list[str]) -> str:
    if not bigo_list:
        return "O(1)"
    for o in order:
        if o in bigo_list:
            return o
    return bigo_list[0]


def _big_o_distribution(bigo_list: list[str]) -> dict[str, int]:
    dist: dict[str, int] = {}
    for b in bigo_list:
        dist[b] = dist.get(b, 0) + 1
    return dist


def _detect_language(ext: str) -> str:
    table = {
        ".py": "python",
        ".ts": "typescript",
        ".tsx": "typescript",
        ".js": "javascript",
        ".jsx": "javascript",
        ".c": "c",
        ".cpp": "cpp",
        ".h": "c",
        ".hpp": "cpp",
        ".java": "java",
        ".go": "go",
        ".rs": "rust",
        ".php": "php",
        ".rb": "ruby",
        ".sql": "sql",
        ".md": "markdown",
        ".json": "json",
        ".yaml": "yaml",
    }
    return table.get(ext, "plaintext")


# ══════════════════════════════════════════════════════════════════════════════
#  VALIDATE — Detección de archivos corruptos / no parseables
# ══════════════════════════════════════════════════════════════════════════════


@router.post("/validate")
async def validate_file(req: ValidateRequest) -> dict[str, Any]:
    """
    Valida si un archivo es legible y parseble.
    Detecta: encoding issues, archivos binarios, archivos truncados, sintaxis rota.
    """
    ext = Path(req.filename).suffix.lower()
    content = req.content

    result: dict[str, Any] = {
        "filename": req.filename,
        "valid": True,
        "warnings": [],
        "errors": [],
        "safe_mode": False,
        "is_binary": False,
        "is_truncated": False,
        "encoding_ok": True,
        "size_bytes": len(content.encode("utf-8", errors="replace")),
    }

    # 1. Detectar contenido binario
    null_ratio = content.count("\x00") / max(len(content), 1)
    if null_ratio > 0.01:
        result["is_binary"] = True
        result["valid"] = False
        result["errors"].append("Archivo binario detectado — no es texto")
        return result

    # 2. Detectar archivo truncado (termina abruptamente)
    stripped = content.rstrip()
    if ext == ".py" and stripped and stripped[-1] in (":", "\\", ",", "(", "[", "{"):
        result["is_truncated"] = True
        result["warnings"].append("El archivo parece truncado (termina en símbolo abierto)")

    if ext in (".ts", ".js") and stripped and stripped.count("{") != stripped.count("}"):
        diff = stripped.count("{") - stripped.count("}")
        result["warnings"].append(f"Llaves desbalanceadas ({diff:+d}) — archivo posiblemente incompleto")

    # 3. Validar sintaxis
    if ext == ".py":
        try:
            ast.parse(content)
        except SyntaxError as e:
            result["safe_mode"] = True
            result["warnings"].append(f"SyntaxError en línea {e.lineno}: {e.msg} — modo seguro activado")
        except Exception as e:
            result["safe_mode"] = True
            result["warnings"].append(f"Error de parse: {e} — modo seguro activado")

    # 4. Detectar encoding problemático
    try:
        content.encode("utf-8")
    except UnicodeEncodeError:
        result["encoding_ok"] = False
        result["warnings"].append("Caracteres no-UTF-8 detectados")

    # 5. Archivo muy largo (puede degradar rendimiento)
    lines = content.count("\n")
    if lines > 5000:
        result["warnings"].append(f"Archivo muy largo ({lines} líneas) — análisis puede ser lento")
    elif lines > 10000:
        result["errors"].append(f"Archivo extremadamente largo ({lines} líneas) — considera dividirlo")

    # 6. Archivo vacío
    if not content.strip():
        result["warnings"].append("Archivo vacío")

    result["valid"] = len(result["errors"]) == 0
    return result


# ══════════════════════════════════════════════════════════════════════════════
#  HEALTH — Estado del motor de análisis
# ══════════════════════════════════════════════════════════════════════════════


@router.get("/health")
async def metrics_health() -> dict[str, Any]:
    """Estado del motor de análisis — usado por el frontend para mostrar capacidades."""
    caps: dict[str, bool] = {}

    try:
        import ast as _ast

        _ast.parse("x=1")
        caps["python_ast"] = True
    except Exception:
        caps["python_ast"] = False

    try:
        import tree_sitter  # noqa: F401

        caps["tree_sitter"] = True
    except ImportError:
        caps["tree_sitter"] = False

    caps["complexity"] = await check_complexity_engine()

    try:
        import pylint  # noqa: F401

        caps["pylint"] = True
    except ImportError:
        caps["pylint"] = False

    try:
        import networkx  # noqa: F401

        caps["networkx"] = True
    except ImportError:
        caps["networkx"] = False

    return {
        "status": "ok",
        "capabilities": caps,
        "safe_mode_available": True,
        "fallback_parser": "regex",
        "supported_languages": [
            "python",
            "typescript",
            "javascript",
            "c",
            "cpp",
            "java",
            "go",
            "rust",
            "php",
            "sql",
        ],
    }
