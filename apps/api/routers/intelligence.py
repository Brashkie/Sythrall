"""
Router: Editor Intelligence
Endpoints optimizados para feedback en tiempo real en Monaco Editor.

Fast path  (~15ms):  POST /intel/lint      → AST + syntax errors
Heavy path (~80ms):  POST /intel/analyze   → pylint + complexity engine + Big-O
Hover:               POST /intel/hover     → firma + Big-O + CC + docs para una función
"""

from __future__ import annotations

import ast
import re
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

from fastapi import APIRouter
from pydantic import BaseModel
from starlette.concurrency import run_in_threadpool

from shared import add_log, save_temp, safe_remove
from services.complexity_client import analyze_complexity, parse_python_rich
from services.static_parser import (
    _parse_ts,
    _parse_js,
    _cyclomatic_python,
)

router = APIRouter()


# ── Schemas ───────────────────────────────────────────────────────────────────


class LintRequest(BaseModel):
    filename: str = "script.py"
    content: str = ""


class AnalyzeRequest(BaseModel):
    filename: str = "script.py"
    content: str = ""
    tools: list[str] = ["ast", "flake8", "pylint", "complexity"]


class HoverRequest(BaseModel):
    filename: str
    content: str
    line: int  # 1-based (Monaco line)
    column: int  # 1-based (Monaco column)
    symbol_name: str = ""  # nombre del símbolo bajo el cursor (opcional, lo usa el frontend)


# ── Marker severity (compatible con Monaco) ───────────────────────────────────
# Monaco: 1=Hint, 2=Info, 4=Warning, 8=Error
SEVERITY_MAP = {
    "error": 8,
    "warning": 4,
    "info": 2,
    "hint": 1,
}


# ══════════════════════════════════════════════════════════════════════════════
#  FAST PATH — lint en tiempo real
#  Solo AST + syntax check. ~15ms. Se llama en cada debounce (300ms).
# ══════════════════════════════════════════════════════════════════════════════


@router.post("/lint")
async def fast_lint(req: LintRequest) -> dict[str, Any]:
    """
    Análisis rápido para feedback mientras se escribe.
    Solo AST (Python) o regex-based (TS/JS). Sin subprocess.
    Retorna markers en formato Monaco.
    """
    ext = Path(req.filename).suffix.lower()
    markers: list[dict] = []
    t0 = time.perf_counter()

    try:
        if ext == ".py":
            markers = _fast_lint_python(req.content)
        elif ext in (".ts", ".tsx"):
            markers = _fast_lint_ts(req.content)
        elif ext in (".js", ".jsx"):
            markers = _fast_lint_js(req.content)
        # otros lenguajes: sin lint fast (tree-sitter sync no es ideal aquí)
    except Exception as e:
        add_log("warn", f"fast_lint error {req.filename}: {e}")

    ms = round((time.perf_counter() - t0) * 1000, 1)
    return {"markers": markers, "ms": ms, "source": "fast"}


def _fast_lint_python(content: str) -> list[dict]:
    """AST Python: syntax + patrones comunes. Sin subprocess."""
    markers: list[dict] = []
    lines = content.splitlines()

    # Syntax check
    try:
        tree = ast.parse(content)
    except SyntaxError as e:
        return [
            {
                "startLineNumber": e.lineno or 1,
                "startColumn": e.offset or 1,
                "endLineNumber": e.lineno or 1,
                "endColumn": (e.offset or 1) + 10,
                "message": f"SyntaxError: {e.msg}",
                "severity": 8,
                "source": "ast",
                "code": "E001",
            }
        ]

    for node in ast.walk(tree):
        ln = getattr(node, "lineno", 0) or 0
        col = getattr(node, "col_offset", 0) or 0

        # print() sin logging
        if isinstance(node, ast.Call):
            if isinstance(node.func, ast.Name) and node.func.id == "print":
                markers.append(_mk(ln, col, col + 5, "print() detectado — usa logging", 2, "C002", "ast"))

        # Bare except
        if isinstance(node, ast.ExceptHandler) and node.type is None:
            markers.append(_mk(ln, col, col + 6, "except genérico captura todo", 4, "W002", "ast"))

        # Variables globales
        if isinstance(node, ast.Global):
            names = ", ".join(node.names)
            markers.append(_mk(ln, col, col + 6, f"Variable global: {names}", 4, "W003", "ast"))

        # assert desactivable
        if isinstance(node, ast.Assert):
            markers.append(_mk(ln, col, col + 6, "assert puede desactivarse con -O", 4, "W001", "ast"))

        # f-string debug leaks (ic() / print(f"..."))
        if isinstance(node, ast.Call):
            if isinstance(node.func, ast.Attribute) and node.func.attr == "debug":
                pass  # logger.debug OK

    # Líneas largas
    for i, line in enumerate(lines, 1):
        if len(line) > 120:
            markers.append(_mk(i, 121, len(line) + 1, f"Línea larga ({len(line)} chars, máx 120)", 4, "E501", "ast"))

    # TODO / FIXME / HACK
    for i, line in enumerate(lines, 1):
        m = re.search(r"\b(TODO|FIXME|HACK)\b", line, re.IGNORECASE)
        if m:
            markers.append(_mk(i, m.start() + 1, m.end() + 1, f"{m.group()}: pendiente", 1, "W100", "ast"))

    return markers


def _fast_lint_ts(content: str) -> list[dict]:
    """Lint rápido TypeScript via regex — sin subprocess."""
    markers: list[dict] = []
    lines = content.splitlines()

    for i, line in enumerate(lines, 1):
        # console.log/debug/error
        m = re.search(r"\bconsole\.(log|debug|error|warn)\s*\(", line)
        if m:
            markers.append(
                _mk(i, m.start() + 1, m.end(), f"console.{m.group(1)}() en producción", 2, "TS001", "ts-lint")
            )

        # any explícito
        m = re.search(r":\s*any\b", line)
        if m:
            markers.append(
                _mk(i, m.start() + 1, m.end(), "Tipo 'any' reduce seguridad de tipos", 4, "TS002", "ts-lint")
            )

        # debugger
        if re.search(r"\bdebugger\b", line):
            markers.append(_mk(i, 1, len(line), "debugger encontrado", 8, "TS003", "ts-lint"))

        # var (usar let/const)
        m = re.search(r"^\s*var\s+", line)
        if m:
            markers.append(_mk(i, m.start() + 1, m.end(), "Usa 'const' o 'let' en vez de 'var'", 4, "TS004", "ts-lint"))

        # == en vez de ===
        m = re.search(r"[^=!<>]==[^=]", line)
        if m:
            markers.append(_mk(i, m.start() + 1, m.end(), "Usa === en vez de ==", 4, "TS005", "ts-lint"))

        # TODO / FIXME
        m = re.search(r"\b(TODO|FIXME|HACK)\b", line, re.IGNORECASE)
        if m:
            markers.append(_mk(i, m.start() + 1, m.end(), f"{m.group()}: pendiente", 1, "W100", "ts-lint"))

        # Líneas largas
        if len(line) > 120:
            markers.append(_mk(i, 121, len(line) + 1, f"Línea larga ({len(line)} chars)", 4, "E501", "ts-lint"))

    return markers


def _fast_lint_js(content: str) -> list[dict]:
    return _fast_lint_ts(content)  # mismas reglas base


def _mk(
    line: int,
    col_start: int,
    col_end: int,
    msg: str,
    severity: int,
    code: str,
    source: str,
) -> dict:
    return {
        "startLineNumber": line,
        "startColumn": max(1, col_start),
        "endLineNumber": line,
        "endColumn": max(col_start + 1, col_end),
        "message": msg,
        "severity": severity,  # 8=Error 4=Warning 2=Info 1=Hint
        "source": source,
        "code": code,
    }


# ══════════════════════════════════════════════════════════════════════════════
#  HEAVY PATH — análisis completo (idle 2s)
#  pylint + complexity engine + flake8. Se llama cuando el usuario para de escribir.
# ══════════════════════════════════════════════════════════════════════════════


@router.post("/analyze")
async def heavy_analyze(req: AnalyzeRequest) -> dict[str, Any]:
    """
    Análisis completo con subprocess (pylint, flake8) + sidecar Rust (complexity).
    Se llama en idle ~2s, no en cada keystroke.
    Retorna markers + métricas + Big-O.
    """
    ext = Path(req.filename).suffix.lower()
    markers: list[dict] = []
    metrics: dict = {}
    big_o: list[dict] = []
    # Findings del CS Engine (Fases 21/22) para el archivo abierto en el
    # Editor — Rust-only acá, sin fallback Python: mismo límite que Halstead/
    # MI arriba, correcto para una feature de live-typing (recalcular en
    # Python cada 2s sin el sidecar no vale la latencia). Vacíos si el
    # sidecar no está arriba o el archivo no es Python.
    security_findings: list[dict] = []
    structural_smells: list[dict] = []
    naming_smells: list[dict] = []
    t0 = time.perf_counter()
    tmp_path = None

    try:
        if ext == ".py":
            tmp_path = save_temp(req.content, ".py")

            # flake8
            if "flake8" in req.tools:
                markers.extend(await run_in_threadpool(_run_flake8_markers, tmp_path))

            # pylint (JSON)
            if "pylint" in req.tools:
                pylint_markers, score = await run_in_threadpool(_run_pylint_markers, tmp_path)
                markers.extend(pylint_markers)
                if score is not None:
                    metrics["pylint_score"] = score

            # complexity engine (Rust) — CC + MI + Halstead (Fase 22, Rust-only,
            # sin fallback Python — mismo límite que MI desde la Fase 11)
            if "complexity" in req.tools:
                cc_data, mi, halstead = await _run_complexity_metrics(req.content)
                metrics["complexity"] = cc_data
                metrics["maintainability"] = mi
                metrics["halstead"] = halstead

            # Big-O — Fase 18 (Native Intelligence): sidecar Rust primero
            # (parse_python_rich, benchmarkeado 5.6-20.6x más rápido que este
            # mismo recorrido en Python), degrada al AST-walk de siempre si
            # el sidecar no está disponible. Mismo shape en ambos casos.
            rich = await parse_python_rich(req.filename, req.content)
            if rich is not None:
                security_findings = rich.get("security_findings", [])
                structural_smells = rich.get("structural_smells", [])
                naming_smells = rich.get("naming_smells", [])
                big_o.extend(
                    {
                        "name": fn["name"],
                        "line": fn["line"],
                        "big_o": fn["big_o"],
                        "big_o_theta": fn["big_o_theta"],
                        "big_o_omega": fn["big_o_omega"],
                        "reason": fn["big_o_reason"],
                        "space_complexity": fn["space_complexity"],
                        "space_reason": fn["space_reason"],
                        "complexity": fn["complexity"],
                        "is_recursive": fn["is_recursive"],
                        "is_tail_recursive": fn["is_tail_recursive"],
                        "recursion_note": fn["recursion_note"],
                        "recurrence": fn.get("recurrence"),
                        "regex_class": fn["regex_class"],
                        "regex_note": fn["regex_note"],
                        "grammar_class": fn["grammar_class"],
                        "grammar_note": fn["grammar_note"],
                        "graph_traversal": fn["graph_traversal"],
                        "graph_traversal_note": fn["graph_traversal_note"],
                    }
                    for fn in rich["functions"]
                )
            else:
                # Sidecar caído — Big-O/space/recursión/regex/grammar/graph
                # classifiers/security son Rust-only ahora (ver
                # `static_parser.py::_parse_python`); sin el sidecar no se
                # recalculan, solo se reporta la estructura básica de la
                # función con valores neutros en vez de reimplementar el
                # motor en Python.
                try:
                    tree = ast.parse(req.content)
                    for node in ast.walk(tree):
                        if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef):
                            big_o.append(
                                {
                                    "name": node.name,
                                    "line": node.lineno,
                                    "big_o": "?",
                                    "big_o_theta": "?",
                                    "big_o_omega": "?",
                                    "reason": "",
                                    "space_complexity": "?",
                                    "space_reason": "",
                                    "complexity": _cyclomatic_python(node),
                                    "is_recursive": False,
                                    "is_tail_recursive": False,
                                    "recursion_note": None,
                                    "recurrence": None,
                                    "regex_class": None,
                                    "regex_note": None,
                                    "grammar_class": None,
                                    "grammar_note": None,
                                    "graph_traversal": None,
                                    "graph_traversal_note": None,
                                }
                            )
                except Exception:
                    pass

        elif ext in (".ts", ".tsx", ".js", ".jsx"):
            # Para TS/JS el heavy path usa el static parser
            parse_fn = _parse_ts if ext in (".ts", ".tsx") else _parse_js
            parsed = parse_fn(req.filename, req.content)
            for fn in parsed.get("functions", []):
                big_o.append(
                    {
                        "name": fn["name"],
                        "line": fn["line"],
                        "big_o": fn.get("big_o", "?"),
                        "reason": fn.get("big_o_reason", ""),
                        "complexity": fn.get("complexity", 1),
                    }
                )
            # Markers de dead imports
            for dead in parsed.get("dead_code", []):
                markers.append(
                    _mk(
                        dead["line"],
                        1,
                        1,
                        f"Import posiblemente no usado: {dead['module']}",
                        2,
                        "W200",
                        "static",
                    )
                )

    except Exception as e:
        add_log("warn", f"heavy_analyze error {req.filename}: {e}")
    finally:
        if tmp_path:
            safe_remove(tmp_path)

    # Deduplicar markers (flake8 + pylint pueden solapar)
    seen: set[str] = set()
    unique: list[dict] = []
    for m in markers:
        key = f"{m['startLineNumber']}:{m.get('code','')}"
        if key not in seen:
            seen.add(key)
            unique.append(m)

    ms = round((time.perf_counter() - t0) * 1000, 1)
    return {
        "markers": sorted(unique, key=lambda x: x["startLineNumber"]),
        "metrics": metrics,
        "big_o": big_o,
        "security_findings": security_findings,
        "structural_smells": structural_smells,
        "naming_smells": naming_smells,
        "ms": ms,
        "source": "heavy",
    }


def _run_flake8_markers(filepath: str) -> list[dict]:
    markers = []
    try:
        r = subprocess.run(
            [
                sys.executable,
                "-m",
                "flake8",
                "--max-line-length=120",
                "--format=%(row)d|%(col)d|%(code)s|%(text)s",
                filepath,
            ],
            capture_output=True,
            text=True,
            timeout=20,
        )
        for line in r.stdout.splitlines():
            parts = line.strip().split("|")
            if len(parts) >= 4:
                try:
                    ln = int(parts[0])
                    col = int(parts[1])
                    code = parts[2]
                    msg = "|".join(parts[3:])
                    sev = 8 if code.startswith("E") else 4
                    markers.append(_mk(ln, col, col + len(code) + 4, msg, sev, code, "flake8"))
                except (ValueError, IndexError):
                    pass
    except Exception as e:
        add_log("warn", f"flake8 error: {e}")
    return markers


def _run_pylint_markers(filepath: str) -> tuple[list[dict], float | None]:
    import json as _json

    markers: list[dict] = []
    score: float | None = None
    try:
        r = subprocess.run(
            [sys.executable, "-m", "pylint", "--output-format=json", "--disable=C0114,C0115,C0116", filepath],
            capture_output=True,
            text=True,
            timeout=30,
        )
        raw = r.stdout.strip()
        if raw:
            sev_map = {"error": 8, "fatal": 8, "warning": 4, "convention": 2, "refactor": 1}
            for item in _json.loads(raw):
                ln = item.get("line", 1)
                col = item.get("column", 0) + 1
                sev = sev_map.get(item.get("type", "warning"), 4)
                code = item.get("message-id", "")
                msg = item.get("message", "")
                markers.append(_mk(ln, col, col + 4, f"[pylint] {msg}", sev, code, "pylint"))
        for line in r.stdout + r.stderr:
            if "Your code has been rated at" in line:
                try:
                    score = float(line.split("at")[1].split("/")[0].strip())
                except Exception:
                    pass
    except Exception as e:
        add_log("warn", f"pylint error: {e}")
    return markers, score


async def _run_complexity_metrics(content: str) -> tuple[list[dict], float | None, dict | None]:
    data = await analyze_complexity("hover.py", content)
    return data.get("functions") or [], data.get("mi"), data.get("halstead")


# ══════════════════════════════════════════════════════════════════════════════
#  HOVER — información de una función bajo el cursor
#  Retorna markdown para Monaco HoverProvider
# ══════════════════════════════════════════════════════════════════════════════


@router.post("/hover")
async def hover_info(req: HoverRequest) -> dict[str, Any]:
    """
    Dado un filename + content + línea/columna, retorna información
    para el HoverProvider de Monaco:
    - Firma de la función
    - Big-O con color
    - Complejidad ciclomática
    - LOC
    - Docstring
    - WASM hint si aplica
    """
    ext = Path(req.filename).suffix.lower()

    if ext == ".py":
        return await _hover_python(req)
    elif ext in (".ts", ".tsx", ".js", ".jsx"):
        return _hover_js_ts(req)
    else:
        return {"markdown": "", "range": None}


async def _hover_python(req: HoverRequest) -> dict:
    """Hover para Python usando AST."""
    try:
        tree = ast.parse(req.content)
        target_line = req.line  # 1-based

        # Encontrar la función que contiene la línea del cursor
        best: ast.FunctionDef | ast.AsyncFunctionDef | None = None
        for node in ast.walk(tree):
            if not isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef):
                continue
            end = getattr(node, "end_lineno", node.lineno)
            if node.lineno <= target_line <= end:
                # Preferir el nodo más cercano (más anidado)
                if best is None or node.lineno >= best.lineno:
                    best = node

        # Si no hay función, intentar buscar por nombre del símbolo
        if best is None and req.symbol_name:
            for node in ast.walk(tree):
                if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef):
                    if node.name == req.symbol_name:
                        best = node
                        break

        if best is None:
            return {"markdown": "", "range": None}

        return await _build_hover_python(best, req)

    except (SyntaxError, Exception):
        return {"markdown": "", "range": None}


async def _build_hover_python(fn: ast.FunctionDef | ast.AsyncFunctionDef, req: HoverRequest) -> dict:
    """Construye el markdown de hover para una función Python."""
    # Firma — anotaciones de tipos
    arg_strs: list[str] = []
    for arg in fn.args.args:
        ann = ""
        if arg.annotation:
            try:
                ann = ": " + ast.unparse(arg.annotation)
            except Exception:
                pass
        arg_strs.append(arg.arg + ann)

    ret_ann = ""
    if fn.returns:
        try:
            ret_ann = " -> " + ast.unparse(fn.returns)
        except Exception:
            pass

    is_async = isinstance(fn, ast.AsyncFunctionDef)
    prefix = "async def " if is_async else "def "
    signature = f"{prefix}{fn.name}({', '.join(arg_strs)}){ret_ann}"

    # Big-O — Fase 18: sidecar Rust primero (parse_python_rich), degrada al
    # cálculo en Python si el sidecar no está disponible o no encuentra esta
    # función puntual en su respuesta (mismo shape en ambos casos).
    rich = await parse_python_rich(req.filename, req.content)
    rich_fn = None
    if rich is not None:
        rich_fn = next((f for f in rich["functions"] if f["name"] == fn.name and f["line"] == fn.lineno), None)

    if rich_fn is not None:
        bigo, reason = rich_fn["big_o"], rich_fn["big_o_reason"]
        theta, omega = rich_fn["big_o_theta"], rich_fn["big_o_omega"]
        recurrence = rich_fn.get("recurrence")
        space, space_reason = rich_fn["space_complexity"], rich_fn["space_reason"]
        rec_note = rich_fn["recursion_note"]
        is_tail_recursive = rich_fn["is_tail_recursive"]
        cc = rich_fn["complexity"]
        regex_note = rich_fn["regex_note"]
        grammar_note = rich_fn["grammar_note"]
        graph_note = rich_fn["graph_traversal_note"]
    else:
        # Sidecar caído (o no encontró esta función puntual) — Big-O/space/
        # recursión/regex/grammar/graph classifiers son Rust-only ahora (ver
        # `static_parser.py::_parse_python`); sin el sidecar no se
        # recalculan, quedan en valores neutros en vez de reimplementar el
        # motor acá.
        bigo, reason, theta, omega = "?", "", "?", "?"
        recurrence = None
        space, space_reason = "?", ""
        rec_note = None
        is_tail_recursive = False
        cc = _cyclomatic_python(fn)
        regex_note = None
        grammar_note = None
        graph_note = None
    end_line = getattr(fn, "end_lineno", fn.lineno)
    loc = end_line - fn.lineno + 1
    docstring = ast.get_docstring(fn) or ""

    # Security & Taint (Fase 21) — Rust-only ahora, mismo rich ya resuelto
    # arriba para Big-O; sin sidecar (o si no encontró esta función
    # puntual), queda vacío en vez de recalcularse en Python.
    if rich_fn is not None:
        security_findings = [f for f in rich["security_findings"] if f.get("function") == fn.name]
    else:
        security_findings = []

    # WASM hint
    wasm_hint = ""
    if bigo in ("O(n²)", "O(n³)", "O(2^n)"):
        wasm_hint = "\n\n**Hot Path** — candidato a Cython / WASM"

    # Construir markdown
    md = f"```python\n{signature}\n```"

    if docstring:
        md += f"\n\n{docstring[:200]}{'…' if len(docstring) > 200 else ''}"

    recurrence_row = f"| **Recurrencia** | `{recurrence}` |\n" if recurrence else ""
    md += f"""

---
| Métrica | Valor |
|---------|-------|
| **Time Complexity (O)** | `{bigo}` |
| **Cota ajustada (Θ)** | `{theta}` |
| **Mejor caso (Ω)** | `{omega}` |
| **Razón** | {reason} |
{recurrence_row}| **Space Complexity** | `{space}` |
| **Razón (espacio)** | {space_reason} |
| **Cyclomatic CC** | `{cc}` — {_cc_label(cc)} |
| **LOC** | `{loc}` líneas |
| **Línea** | `{fn.lineno}` |"""

    if rec_note:
        tail_badge = "tail-call" if is_tail_recursive else "no tail-call"
        md += f"\n\n**Recursión** — {tail_badge}\n\n{rec_note}"

    if regex_note:
        md += f"\n\n**Regex** — {regex_note}"

    if grammar_note:
        md += f"\n\n**Grammar/Parser** — {grammar_note}"

    if graph_note:
        md += f"\n\n**Graph Traversal** — {graph_note}"

    for finding in security_findings:
        sink_txt = f" → `{finding['sink']}`" if finding.get("sink") else ""
        confidence_es = {"High": "alta", "Medium": "media", "Low": "baja"}.get(
            finding["confidence"], finding["confidence"]
        )
        md += (
            f"\n\n**{finding['category']} ({finding['cwe']})** — confianza {confidence_es}\n\n"
            f"`{finding['source']}`{sink_txt} — línea {finding['line']}\n\n"
            f"{finding['recommendation']}"
        )

    if is_async:
        md += "\n\nFunción **async**"

    if wasm_hint:
        md += wasm_hint

    md += (
        "\n\n<sub>O = cota superior (peor caso) · Θ = cota ajustada · Ω = cota inferior "
        "(mejor caso) — convención específica de Sythrall. Definiciones generales, más "
        "o (cota superior estricta) y ω (cota inferior estricta), en el panel Static.</sub>"
    )

    return {
        "markdown": md,
        "range": {
            "startLineNumber": fn.lineno,
            "startColumn": 1,
            "endLineNumber": end_line,
            "endColumn": 999,
        },
    }


def _hover_js_ts(req: HoverRequest) -> dict:
    """Hover para JS/TS usando el static parser."""
    ext = Path(req.filename).suffix.lower()
    parse_fn = _parse_ts if ext in (".ts", ".tsx") else _parse_js
    parsed = parse_fn(req.filename, req.content)
    target = req.line

    # Buscar función que contiene la línea
    best = None
    for fn in parsed.get("functions", []):
        fn_start = fn["line"]
        fn_end = fn.get("end_line", fn_start + fn.get("loc", 1))
        if fn_start <= target <= fn_end:
            if best is None or fn_start >= best["line"]:
                best = fn

    if best is None and req.symbol_name:
        best = next((f for f in parsed["functions"] if f["name"] == req.symbol_name), None)

    if best is None:
        return {"markdown": "", "range": None}

    bigo = best.get("big_o", "?")
    reason = best.get("big_o_reason", "")
    cc = best.get("complexity", 1)
    loc = best.get("loc", 1)
    is_async = best.get("is_async", False)
    args = best.get("args", [])
    calls = best.get("calls", [])

    lang = "typescript" if ext in (".ts", ".tsx") else "javascript"
    prefix = "async " if is_async else ""
    sig = f"{prefix}function {best['name']}({', '.join(args[:6])}{'…' if len(args)>6 else ''})"

    md = f"```{lang}\n{sig}\n```"
    md += f"""

---
| Métrica | Valor |
|---------|-------|
| **Time Complexity** | `{bigo}` |
| **Razón** | {reason} |
| **Cyclomatic CC** | `{cc}` — {_cc_label(cc)} |
| **LOC** | `{loc}` líneas |
| **Línea** | `{best['line']}` |"""

    if calls:
        md += f"\n\n**Llama a:** `{'`, `'.join(calls[:5])}{'…' if len(calls)>5 else ''}`"

    if bigo in ("O(n²)", "O(n³)", "O(2^n)"):
        md += "\n\n**Hot Path** — considera mover a WASM / Worker"

    return {
        "markdown": md,
        "range": {
            "startLineNumber": best["line"],
            "startColumn": 1,
            "endLineNumber": best.get("end_line", best["line"]),
            "endColumn": 999,
        },
    }


# ── Helpers ───────────────────────────────────────────────────────────────────


def _cc_label(cc: int) -> str:
    if cc <= 4:
        return "Simple"
    if cc <= 7:
        return "Moderado"
    if cc <= 10:
        return "Complejo"
    return "Muy complejo"


# ══════════════════════════════════════════════════════════════════════════════
#  GO TO DEFINITION
#  Dado un símbolo en una posición, retorna la línea/col donde está definido
# ══════════════════════════════════════════════════════════════════════════════


class DefinitionRequest(BaseModel):
    filename: str
    content: str
    line: int
    column: int
    symbol_name: str = ""


class ReferenceRequest(BaseModel):
    filename: str
    content: str
    symbol_name: str


@router.post("/definition")
async def go_to_definition(req: DefinitionRequest) -> dict[str, Any]:
    """
    Retorna la ubicación de la definición del símbolo bajo el cursor.
    Soporta: funciones, clases, métodos, variables de nivel módulo.
    """
    ext = Path(req.filename).suffix.lower()
    symbol = req.symbol_name.strip()

    if not symbol:
        return {"found": False, "symbol": "", "definitions": []}

    try:
        if ext == ".py":
            defs = _find_definitions_python(req.content, symbol)
        elif ext in (".ts", ".tsx", ".js", ".jsx"):
            defs = _find_definitions_js_ts(req.content, symbol, ext)
        else:
            defs = []

        return {
            "found": len(defs) > 0,
            "symbol": symbol,
            "filename": req.filename,
            "definitions": defs,
        }
    except Exception as e:
        add_log("warn", f"definition error: {e}")
        return {"found": False, "symbol": symbol, "definitions": []}


def _find_definitions_python(content: str, symbol: str) -> list[dict]:
    """Busca la definición de un símbolo en el AST Python."""
    try:
        tree = ast.parse(content)
    except SyntaxError:
        return []

    defs: list[dict] = []

    for node in ast.walk(tree):
        # Funciones y métodos
        if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef):
            if node.name == symbol:
                end = getattr(node, "end_lineno", node.lineno)
                args = [a.arg for a in node.args.args]
                ret = ""
                if node.returns:
                    try:
                        ret = " -> " + ast.unparse(node.returns)
                    except Exception:
                        pass
                defs.append(
                    {
                        "line": node.lineno,
                        "column": node.col_offset + 1,
                        "end_line": end,
                        "kind": "method" if node.col_offset > 0 else "function",
                        "signature": f"def {symbol}({', '.join(args)}){ret}",
                        "docstring": ast.get_docstring(node) or "",
                    }
                )

        # Clases
        elif isinstance(node, ast.ClassDef) and node.name == symbol:
            end = getattr(node, "end_lineno", node.lineno)
            bases = []
            for b in node.bases:
                try:
                    bases.append(ast.unparse(b))
                except Exception:
                    pass
            defs.append(
                {
                    "line": node.lineno,
                    "column": node.col_offset + 1,
                    "end_line": end,
                    "kind": "class",
                    "signature": f"class {symbol}({', '.join(bases)})" if bases else f"class {symbol}",
                    "docstring": ast.get_docstring(node) or "",
                }
            )

        # Variables de nivel módulo (Assign simple: x = ...)
        elif isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name) and target.id == symbol:
                    val_repr = ""
                    try:
                        val_repr = " = " + ast.unparse(node.value)[:60]
                    except Exception:
                        pass
                    defs.append(
                        {
                            "line": node.lineno,
                            "column": target.col_offset + 1,
                            "end_line": node.lineno,
                            "kind": "variable",
                            "signature": f"{symbol}{val_repr}",
                            "docstring": "",
                        }
                    )

        # AnnAssign: x: int = ...
        elif isinstance(node, ast.AnnAssign):
            if isinstance(node.target, ast.Name) and node.target.id == symbol:
                ann = ""
                try:
                    ann = ": " + ast.unparse(node.annotation)
                except Exception:
                    pass
                defs.append(
                    {
                        "line": node.lineno,
                        "column": node.target.col_offset + 1,
                        "end_line": node.lineno,
                        "kind": "variable",
                        "signature": f"{symbol}{ann}",
                        "docstring": "",
                    }
                )

    # Ordenar por línea, priorizar funciones/clases sobre variables
    kind_priority = {"function": 0, "method": 0, "class": 1, "variable": 2}
    defs.sort(key=lambda d: (kind_priority.get(d["kind"], 3), d["line"]))
    return defs


def _find_definitions_js_ts(content: str, symbol: str, ext: str) -> list[dict]:
    """Busca la definición de un símbolo en JS/TS usando el static parser."""
    from services.static_parser import _parse_ts, _parse_js

    parse_fn = _parse_ts if ext in (".ts", ".tsx") else _parse_js
    parsed = parse_fn(f"file{ext}", content)
    defs: list[dict] = []

    # Funciones
    for fn in parsed.get("functions", []):
        if fn["name"] == symbol:
            args = fn.get("args", [])
            defs.append(
                {
                    "line": fn["line"],
                    "column": 1,
                    "end_line": fn.get("end_line", fn["line"]),
                    "kind": "function",
                    "signature": f"function {symbol}({', '.join(args[:6])})",
                    "docstring": "",
                }
            )

    # Clases
    for cls in parsed.get("classes", []):
        if cls["name"] == symbol:
            extends = f" extends {cls['extends']}" if cls.get("extends") else ""
            defs.append(
                {
                    "line": cls["line"],
                    "column": 1,
                    "end_line": cls["line"],
                    "kind": "class",
                    "signature": f"class {symbol}{extends}",
                    "docstring": "",
                }
            )

    # Interfaces (TS)
    for iface in parsed.get("interfaces", []):
        if iface["name"] == symbol:
            defs.append(
                {
                    "line": iface["line"],
                    "column": 1,
                    "end_line": iface["line"],
                    "kind": "interface",
                    "signature": f"interface {symbol}",
                    "docstring": "",
                }
            )

    # Types (TS)
    for t in parsed.get("types", []):
        if t["name"] == symbol:
            defs.append(
                {
                    "line": t["line"],
                    "column": 1,
                    "end_line": t["line"],
                    "kind": "type",
                    "signature": f"type {symbol}",
                    "docstring": "",
                }
            )

    defs.sort(key=lambda d: d["line"])
    return defs


# ══════════════════════════════════════════════════════════════════════════════
#  FIND REFERENCES
#  Retorna todas las ubicaciones donde se usa un símbolo en el archivo
# ══════════════════════════════════════════════════════════════════════════════


@router.post("/references")
async def find_references(req: ReferenceRequest) -> dict[str, Any]:
    """
    Busca todas las referencias (usos) de un símbolo en el archivo.
    Incluye la línea de definición marcada separadamente.
    """
    ext = Path(req.filename).suffix.lower()
    symbol = req.symbol_name.strip()

    if not symbol:
        return {"symbol": "", "references": [], "definition_line": None, "total": 0}

    try:
        if ext == ".py":
            refs, def_line = _find_references_python(req.content, symbol)
        elif ext in (".ts", ".tsx", ".js", ".jsx"):
            refs, def_line = _find_references_js_ts(req.content, symbol, ext)
        else:
            refs, def_line = [], None

        return {
            "symbol": symbol,
            "filename": req.filename,
            "references": refs,
            "definition_line": def_line,
            "total": len(refs),
        }
    except Exception as e:
        add_log("warn", f"references error: {e}")
        return {"symbol": symbol, "references": [], "definition_line": None, "total": 0}


def _find_references_python(content: str, symbol: str) -> tuple[list[dict], int | None]:
    """Encuentra todas las referencias a un símbolo en Python vía AST."""
    try:
        tree = ast.parse(content)
    except SyntaxError:
        # Fallback regex si hay error de sintaxis
        return _find_references_regex(content, symbol), None

    refs: list[dict] = []
    def_line: int | None = None
    lines = content.splitlines()

    for node in ast.walk(tree):
        ln = getattr(node, "lineno", None)
        if ln is None:
            continue

        # Definiciones (marcar como tal)
        if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef | ast.ClassDef):
            if node.name == symbol:
                def_line = node.lineno
                refs.append(
                    {
                        "line": node.lineno,
                        "column": node.col_offset + 1,
                        "kind": "definition",
                        "preview": _preview(lines, node.lineno),
                    }
                )

        # Usos: Name (variables, llamadas directas)
        elif isinstance(node, ast.Name) and node.id == symbol:
            kind = "read"
            # Detectar si es el target de una asignación
            refs.append(
                {
                    "line": node.lineno,
                    "column": node.col_offset + 1,
                    "kind": kind,
                    "preview": _preview(lines, node.lineno),
                }
            )

        # Llamadas a métodos/atributos: obj.symbol(...)
        elif isinstance(node, ast.Attribute) and node.attr == symbol:
            refs.append(
                {
                    "line": getattr(node, "end_lineno", ln),
                    "column": node.col_offset + 1,
                    "kind": "call",
                    "preview": _preview(lines, ln),
                }
            )

    # Deduplicar por línea+col
    seen: set[str] = set()
    unique: list[dict] = []
    for r in refs:
        key = f"{r['line']}:{r['column']}"
        if key not in seen:
            seen.add(key)
            unique.append(r)

    unique.sort(key=lambda r: r["line"])
    return unique, def_line


def _find_references_js_ts(content: str, symbol: str, ext: str) -> tuple[list[dict], int | None]:
    """Encuentra referencias en JS/TS via regex + posiciones del parser."""
    from services.static_parser import _parse_ts, _parse_js

    parse_fn = _parse_ts if ext in (".ts", ".tsx") else _parse_js
    parsed = parse_fn(f"file{ext}", content)

    def_line: int | None = None
    refs: list[dict] = []
    lines = content.splitlines()

    # Línea de definición desde el parser
    for fn in parsed.get("functions", []):
        if fn["name"] == symbol:
            def_line = fn["line"]
            refs.append(
                {
                    "line": fn["line"],
                    "column": 1,
                    "kind": "definition",
                    "preview": _preview(lines, fn["line"]),
                }
            )
    for cls in parsed.get("classes", []):
        if cls["name"] == symbol:
            def_line = cls["line"]
            refs.append(
                {
                    "line": cls["line"],
                    "column": 1,
                    "kind": "definition",
                    "preview": _preview(lines, cls["line"]),
                }
            )

    # Resto de usos via regex word-boundary
    pattern = re.compile(rf"\b{re.escape(symbol)}\b")
    for i, line in enumerate(lines, 1):
        for m in pattern.finditer(line):
            col = m.start() + 1
            # Evitar duplicar la línea de definición
            if any(r["line"] == i and r["column"] == col for r in refs):
                continue
            refs.append(
                {
                    "line": i,
                    "column": col,
                    "kind": "use",
                    "preview": _preview(lines, i),
                }
            )

    refs.sort(key=lambda r: r["line"])
    return refs, def_line


def _find_references_regex(content: str, symbol: str) -> list[dict]:
    """Fallback: busca referencias por regex cuando el AST falla."""
    lines = content.splitlines()
    pattern = re.compile(rf"\b{re.escape(symbol)}\b")
    refs: list[dict] = []
    for i, line in enumerate(lines, 1):
        for m in pattern.finditer(line):
            refs.append(
                {
                    "line": i,
                    "column": m.start() + 1,
                    "kind": "use",
                    "preview": _preview(lines, i),
                }
            )
    return refs


def _preview(lines: list[str], lineno: int, max_len: int = 70) -> str:
    """Retorna la línea de código como preview, truncada."""
    if 1 <= lineno <= len(lines):
        s = lines[lineno - 1].strip()
        return s[:max_len] + ("…" if len(s) > max_len else "")
    return ""


# ══════════════════════════════════════════════════════════════════════════════
#  AUTOCOMPLETE — símbolos del archivo para CompletionItemProvider
# ══════════════════════════════════════════════════════════════════════════════


class CompletionRequest(BaseModel):
    filename: str
    content: str
    prefix: str = ""  # texto que el usuario ya escribió (para filtrar)


@router.post("/completions")
async def get_completions(req: CompletionRequest) -> dict[str, Any]:
    """
    Retorna símbolos del archivo para el autocomplete de Monaco.
    Incluye: funciones, clases, métodos, variables, imports.
    Filtra por prefix si se proporciona.
    """
    ext = Path(req.filename).suffix.lower()
    prefix = req.prefix.lower()

    try:
        if ext == ".py":
            symbols = _completions_python(req.content)
        elif ext in (".ts", ".tsx", ".js", ".jsx"):
            symbols = _completions_js_ts(req.content, ext)
        else:
            symbols = []

        # Filtrar por prefix
        if prefix:
            symbols = [s for s in symbols if s["label"].lower().startswith(prefix)]

        # Ordenar: primero funciones/clases, luego variables/imports
        kind_order = {"function": 0, "method": 0, "class": 1, "variable": 2, "import": 3, "interface": 4, "type": 4}
        symbols.sort(key=lambda s: (kind_order.get(s["kind"], 5), s["label"]))

        return {
            "symbols": symbols,
            "total": len(symbols),
            "filename": req.filename,
            "prefix": req.prefix,
        }

    except Exception as e:
        add_log("warn", f"completions error: {e}")
        return {"symbols": [], "total": 0, "filename": req.filename, "prefix": req.prefix}


def _completions_python(content: str) -> list[dict]:
    """Extrae símbolos de Python para autocomplete."""
    try:
        tree = ast.parse(content)
    except SyntaxError:
        return []

    symbols: list[dict] = []

    for node in ast.walk(tree):
        # Funciones top-level
        if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef):
            args = [a.arg for a in node.args.args]
            ret = ""
            if node.returns:
                try:
                    ret = " -> " + ast.unparse(node.returns)
                except Exception:
                    pass

            is_method = node.col_offset > 0
            kind = "method" if is_method else "function"
            doc = ast.get_docstring(node) or ""

            symbols.append(
                {
                    "label": node.name,
                    "kind": kind,
                    "detail": f"{'async ' if isinstance(node, ast.AsyncFunctionDef) else ''}def {node.name}({', '.join(args)}){ret}",
                    "documentation": doc[:200] if doc else None,
                    "insert_text": f"{node.name}(${{1}})",  # snippet con cursor
                    "line": node.lineno,
                    "sort_text": f"0{node.name}",  # para ordenar en Monaco
                }
            )

        # Clases
        elif isinstance(node, ast.ClassDef):
            bases = []
            for b in node.bases:
                try:
                    bases.append(ast.unparse(b))
                except Exception:
                    pass
            doc = ast.get_docstring(node) or ""
            symbols.append(
                {
                    "label": node.name,
                    "kind": "class",
                    "detail": f"class {node.name}({', '.join(bases)})" if bases else f"class {node.name}",
                    "documentation": doc[:200] if doc else None,
                    "insert_text": node.name,
                    "line": node.lineno,
                    "sort_text": f"1{node.name}",
                }
            )

            # Métodos de la clase
            for item in node.body:
                if isinstance(item, ast.FunctionDef | ast.AsyncFunctionDef):
                    args = [a.arg for a in item.args.args if a.arg != "self"]
                    symbols.append(
                        {
                            "label": item.name,
                            "kind": "method",
                            "detail": f"{node.name}.{item.name}({', '.join(args)})",
                            "documentation": ast.get_docstring(item) or None,
                            "insert_text": f"{item.name}(${{1}})",
                            "line": item.lineno,
                            "sort_text": f"0{item.name}",
                        }
                    )

        # Variables de módulo
        elif isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name) and target.id.isupper():
                    # Solo constantes (UPPER_CASE)
                    val = ""
                    try:
                        val = " = " + ast.unparse(node.value)[:40]
                    except Exception:
                        pass
                    symbols.append(
                        {
                            "label": target.id,
                            "kind": "variable",
                            "detail": f"{target.id}{val}",
                            "documentation": None,
                            "insert_text": target.id,
                            "line": node.lineno,
                            "sort_text": f"2{target.id}",
                        }
                    )

        # Imports
        elif isinstance(node, ast.Import):
            for alias in node.names:
                name = alias.asname or alias.name.split(".")[0]
                symbols.append(
                    {
                        "label": name,
                        "kind": "import",
                        "detail": f"import {alias.name}",
                        "documentation": None,
                        "insert_text": name,
                        "line": node.lineno,
                        "sort_text": f"3{name}",
                    }
                )

        elif isinstance(node, ast.ImportFrom):
            mod = node.module or ""
            for alias in node.names:
                if alias.name == "*":
                    continue
                name = alias.asname or alias.name
                symbols.append(
                    {
                        "label": name,
                        "kind": "import",
                        "detail": f"from {mod} import {alias.name}",
                        "documentation": None,
                        "insert_text": name,
                        "line": node.lineno,
                        "sort_text": f"3{name}",
                    }
                )

    # Deduplicar por label+kind
    seen: set[str] = set()
    unique: list[dict] = []
    for s in symbols:
        key = f"{s['label']}:{s['kind']}"
        if key not in seen:
            seen.add(key)
            unique.append(s)

    return unique


def _completions_js_ts(content: str, ext: str) -> list[dict]:
    """Extrae símbolos de JS/TS para autocomplete."""
    from services.static_parser import _parse_ts, _parse_js

    parse_fn = _parse_ts if ext in (".ts", ".tsx") else _parse_js
    parsed = parse_fn(f"file{ext}", content)
    symbols: list[dict] = []

    for fn in parsed.get("functions", []):
        args = fn.get("args", [])
        symbols.append(
            {
                "label": fn["name"],
                "kind": "function",
                "detail": f"{'async ' if fn.get('is_async') else ''}function {fn['name']}({', '.join(args[:6])})",
                "documentation": None,
                "insert_text": f"{fn['name']}(${{1}})",
                "line": fn["line"],
                "sort_text": f"0{fn['name']}",
            }
        )

    for cls in parsed.get("classes", []):
        extends = f" extends {cls['extends']}" if cls.get("extends") else ""
        symbols.append(
            {
                "label": cls["name"],
                "kind": "class",
                "detail": f"class {cls['name']}{extends}",
                "documentation": None,
                "insert_text": cls["name"],
                "line": cls["line"],
                "sort_text": f"1{cls['name']}",
            }
        )

    for iface in parsed.get("interfaces", []):
        symbols.append(
            {
                "label": iface["name"],
                "kind": "interface",
                "detail": f"interface {iface['name']}",
                "documentation": None,
                "insert_text": iface["name"],
                "line": iface["line"],
                "sort_text": f"1{iface['name']}",
            }
        )

    for t in parsed.get("types", []):
        symbols.append(
            {
                "label": t["name"],
                "kind": "type",
                "detail": f"type {t['name']}",
                "documentation": None,
                "insert_text": t["name"],
                "line": t["line"],
                "sort_text": f"1{t['name']}",
            }
        )

    for imp in parsed.get("imports", []):
        mod = imp["module"].split("/")[-1]
        name = re.sub(r"[^a-zA-Z0-9_]", "_", mod)
        if name:
            symbols.append(
                {
                    "label": name,
                    "kind": "import",
                    "detail": f"from '{imp['module']}'",
                    "documentation": None,
                    "insert_text": name,
                    "line": imp["line"],
                    "sort_text": f"3{name}",
                }
            )

    seen: set[str] = set()
    unique: list[dict] = []
    for s in symbols:
        key = f"{s['label']}:{s['kind']}"
        if key not in seen:
            seen.add(key)
            unique.append(s)
    return unique


# ══════════════════════════════════════════════════════════════════════════════
#  RENAME SYMBOL
#  Retorna todos los edits necesarios para renombrar un símbolo en el archivo
# ══════════════════════════════════════════════════════════════════════════════


class RenameRequest(BaseModel):
    filename: str
    content: str
    symbol_name: str
    new_name: str


@router.post("/rename")
async def rename_symbol(req: RenameRequest) -> dict[str, Any]:
    """
    Calcula los WorkspaceEdits necesarios para renombrar un símbolo.
    Monaco aplica los edits automáticamente.

    Retorna edits en formato Monaco:
    { range: { startLine, startCol, endLine, endCol }, newText: str }
    """
    if not req.symbol_name or not req.new_name:
        return {"valid": False, "edits": [], "error": "Nombres vacíos"}

    # Validar que new_name es un identificador válido
    if not re.match(r"^[a-zA-Z_][a-zA-Z0-9_]*$", req.new_name):
        return {"valid": False, "edits": [], "error": f"'{req.new_name}' no es un identificador válido"}

    if req.symbol_name == req.new_name:
        return {"valid": False, "edits": [], "error": "El nombre es igual"}

    ext = Path(req.filename).suffix.lower()

    try:
        if ext == ".py":
            refs, _def_line = _find_references_python(req.content, req.symbol_name)
        elif ext in (".ts", ".tsx", ".js", ".jsx"):
            refs, _def_line = _find_references_js_ts(req.content, req.symbol_name, ext)
        else:
            refs, _def_line = _find_references_regex(req.content, req.symbol_name), None

        if not refs:
            return {
                "valid": False,
                "edits": [],
                "error": f"Símbolo '{req.symbol_name}' no encontrado",
            }

        # Construir edits — buscar el símbolo exacto en cada línea referenciada
        lines = req.content.splitlines()
        pattern = re.compile(rf"\b{re.escape(req.symbol_name)}\b")
        edits: list[dict] = []
        seen_positions: set[str] = set()

        for ref in refs:
            ln = ref["line"]
            if ln < 1 or ln > len(lines):
                continue
            line_str = lines[ln - 1]

            for m in pattern.finditer(line_str):
                key = f"{ln}:{m.start()}"
                if key in seen_positions:
                    continue
                seen_positions.add(key)

                edits.append(
                    {
                        "range": {
                            "startLineNumber": ln,
                            "startColumn": m.start() + 1,
                            "endLineNumber": ln,
                            "endColumn": m.end() + 1,
                        },
                        "newText": req.new_name,
                        "kind": ref["kind"],
                        "preview": line_str.strip()[:70],
                    }
                )

        edits.sort(key=lambda e: (e["range"]["startLineNumber"], e["range"]["startColumn"]))

        return {
            "valid": True,
            "edits": edits,
            "total": len(edits),
            "old_name": req.symbol_name,
            "new_name": req.new_name,
            "filename": req.filename,
        }

    except Exception as e:
        add_log("warn", f"rename error: {e}")
        return {"valid": False, "edits": [], "error": str(e)}
