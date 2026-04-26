"""
Router: Analysis
Migración exacta de /analyze/code, /check/api, /analyze/logs del app.py Flask v3.0.
Preserva toda la lógica: _run_ast, _run_flake8, _run_pylint, _run_radon.
"""

import ast
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

from fastapi import APIRouter
from pydantic import BaseModel

from shared import add_log, now, save_temp, safe_remove, LIB_FLAGS, API_HISTORY

# radon imports (opcionales)
try:
    from radon.complexity import cc_visit, cc_rank
    from radon.metrics    import mi_visit
    from radon.raw        import analyze as radon_raw
except ImportError:
    pass

router = APIRouter()


# ── Helpers (migrados de Flask) ───────────────────────────────────────────────

def safe_remove(path: str) -> None:
    try:
        os.unlink(path)
    except Exception:
        pass


def save_temp(content: str, suffix: str) -> str:
    tmp = tempfile.NamedTemporaryFile(
        mode="w", suffix=suffix, delete=False, encoding="utf-8"
    )
    tmp.write(content)
    tmp.close()
    return tmp.name


# ── Schemas ───────────────────────────────────────────────────────────────────

class AnalyzeCodeRequest(BaseModel):
    filename: str = "script.py"
    content:  str = ""
    tools:    list[str] = ["ast", "flake8", "pylint", "radon"]


class CheckApiRequest(BaseModel):
    urls:    list[str]
    timeout: int = 10
    headers: dict[str, str] = {}


class AnalyzeLogsRequest(BaseModel):
    files: list[dict]


# ── /analyze/code ─────────────────────────────────────────────────────────────

@router.post("/code")
async def analyze_code(req: AnalyzeCodeRequest):
    filename = req.filename
    content  = req.content
    tools    = req.tools
    ext      = Path(filename).suffix.lower()

    result: dict[str, Any] = {
        "filename": filename, "ts": now(),
        "issues": [], "metrics": {},
        "complexity": [], "maintainability": None,
        "raw_stats": {}, "tools_used": [],
    }
    tmp_path = None
    try:
        if ext == ".py" and "ast" in tools:
            result["issues"].extend(_run_ast(content, filename))
            result["tools_used"].append("ast")

        if ext == ".py":
            tmp_path = save_temp(content, ".py")

        if ext == ".py" and "flake8" in tools:
            result["issues"].extend(_run_flake8(tmp_path))
            result["tools_used"].append("flake8")

        if ext == ".py" and "pylint" in tools:
            pl_issues, score = _run_pylint(tmp_path)
            result["issues"].extend(pl_issues)
            result["metrics"]["pylint_score"] = score
            result["tools_used"].append("pylint")

        if ext == ".py" and "radon" in tools and LIB_FLAGS["HAS_RADON"]:
            cx, mi, raw = _run_radon(content, filename)
            result["complexity"]      = cx
            result["maintainability"] = mi
            result["raw_stats"]       = raw
            result["tools_used"].append("radon")

        if ext == ".json":
            try:
                json.loads(content)
            except json.JSONDecodeError as e:
                result["issues"].append({
                    "tool": "json", "line": e.lineno, "col": e.colno,
                    "severity": "error", "code": "E999",
                    "message": f"JSON inválido: {e.msg}",
                })

        # Deduplicar
        seen, unique = set(), []
        for iss in result["issues"]:
            k = (iss.get("line"), (iss.get("message") or "")[:40])
            if k not in seen:
                seen.add(k)
                unique.append(iss)
        result["issues"] = sorted(unique, key=lambda x: x.get("line") or 0)

    except Exception as e:
        add_log("error", f"Error analizando {filename}: {e}")
        result["error"] = str(e)
    finally:
        if tmp_path:
            safe_remove(tmp_path)

    return result


# ── Funciones internas de análisis (idénticas al Flask original) ──────────────

def _run_ast(content: str, filename: str) -> list[dict]:
    issues: list[dict] = []
    lines = content.splitlines()
    try:
        tree = ast.parse(content, filename=filename)
    except SyntaxError as e:
        return [{"tool": "ast", "line": e.lineno, "col": e.offset,
                 "severity": "error", "code": "E001",
                 "message": f"SyntaxError: {e.msg}"}]

    for node in ast.walk(tree):
        ln = getattr(node, "lineno", None)
        if isinstance(node, ast.Assert):
            issues.append({"tool":"ast","line":ln,"col":0,"severity":"warning",
                           "code":"W001","message":"assert puede desactivarse con -O"})
        if isinstance(node, ast.ExceptHandler) and node.type is None:
            issues.append({"tool":"ast","line":ln,"col":0,"severity":"warning",
                           "code":"W002","message":"except genérico captura todo"})
        if isinstance(node, ast.Global):
            issues.append({"tool":"ast","line":ln,"col":0,"severity":"warning",
                           "code":"W003",
                           "message":f"Variable global: {', '.join(node.names)}"})
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            no_doc = not (
                node.body
                and isinstance(node.body[0], ast.Expr)
                and isinstance(node.body[0].value, ast.Constant)
            )
            if no_doc:
                issues.append({"tool":"ast","line":ln,"col":0,"severity":"info",
                               "code":"C001",
                               "message":f"'{node.name}' sin docstring"})
        if isinstance(node, ast.Call):
            func = node.func
            if isinstance(func, ast.Name) and func.id == "print":
                issues.append({"tool":"ast","line":ln,"col":0,"severity":"info",
                               "code":"C002","message":"print() — usa logging"})

    for i, line in enumerate(lines, 1):
        if len(line) > 120:
            issues.append({"tool":"ast","line":i,"col":121,"severity":"warning",
                           "code":"E501",
                           "message":f"Línea de {len(line)} chars"})
    return issues


def _run_flake8(filepath: str) -> list[dict]:
    issues: list[dict] = []
    try:
        result = subprocess.run(
            [sys.executable, "-m", "flake8", "--max-line-length=120",
             "--format=%(row)d|%(col)d|%(code)s|%(text)s", filepath],
            capture_output=True, text=True, timeout=30,
        )
        for line in result.stdout.splitlines():
            parts = line.strip().split("|")
            if len(parts) >= 4:
                try:
                    issues.append({
                        "tool":     "flake8",
                        "line":     int(parts[0]),
                        "col":      int(parts[1]),
                        "severity": "error" if parts[2].startswith("E") else "warning",
                        "code":     parts[2],
                        "message":  "|".join(parts[3:]),
                    })
                except Exception:
                    pass
    except Exception as e:
        issues.append({"tool":"flake8","line":0,"col":0,"severity":"info",
                       "code":"ERR","message":str(e)})
    return issues


def _run_pylint(filepath: str) -> tuple[list[dict], float | None]:
    issues: list[dict] = []
    score: float | None = None
    try:
        result = subprocess.run(
            [sys.executable, "-m", "pylint",
             "--output-format=json", "--disable=C0114,C0115", filepath],
            capture_output=True, text=True, timeout=45,
        )
        raw = result.stdout.strip()
        if raw:
            try:
                sev_map = {
                    "error":"error","warning":"warning","convention":"info",
                    "refactor":"info","fatal":"error",
                }
                for item in json.loads(raw):
                    sev = sev_map.get(item.get("type","warning"), "warning")
                    issues.append({
                        "tool":     "pylint",
                        "line":     item.get("line", 0),
                        "col":      item.get("column", 0),
                        "severity": sev,
                        "code":     item.get("message-id", ""),
                        "message":  item.get("message", ""),
                    })
            except Exception:
                pass
        for line in (result.stderr + result.stdout).splitlines():
            if "Your code has been rated at" in line:
                try:
                    score = float(line.split("at")[1].split("/")[0].strip())
                except Exception:
                    pass
    except Exception as e:
        issues.append({"tool":"pylint","line":0,"col":0,"severity":"info",
                       "code":"ERR","message":str(e)})
    return issues, score


def _run_radon(content: str, filename: str) -> tuple[list, float | None, dict]:
    cx_list:   list    = []
    mi_score:  float | None = None
    raw_stats: dict    = {}
    try:
        for block in cc_visit(content):
            cx_list.append({
                "name":       block.name,
                "type":       getattr(block, "type", "function"),
                "line":       block.lineno,
                "complexity": block.complexity,
                "rank":       cc_rank(block.complexity),
            })
        mi_raw   = mi_visit(content, multi=True)
        mi_score = round(mi_raw, 2) if isinstance(mi_raw, float) else None
        raw      = radon_raw(content)
        raw_stats = {
            "loc": raw.loc, "lloc": raw.lloc, "sloc": raw.sloc,
            "comments": raw.comments, "blank": raw.blank, "multi": raw.multi,
        }
    except Exception as e:
        raw_stats["error"] = str(e)
    return cx_list, mi_score, raw_stats


# ── /check/api ────────────────────────────────────────────────────────────────

@router.post("/api")
async def check_api(req: CheckApiRequest):
    """Equivalente a POST /check/api del Flask original."""
    import httpx

    results = []
    async with httpx.AsyncClient(
        timeout=req.timeout,
        follow_redirects=True,
        verify=False,
    ) as client:
        for url in req.urls:
            r = await _check_single_url(client, url, req.timeout, req.headers)

            # Actualizar API_HISTORY (importado desde main)
            if url not in API_HISTORY:
                API_HISTORY[url] = []
            API_HISTORY[url].append({
                "ts": now(), "status": r["status"],
                "ms": r["ms"], "code": r["code"],
            })
            API_HISTORY[url] = API_HISTORY[url][-50:]
            r["history"] = API_HISTORY[url][-10:]
            results.append(r)
            add_log(
                "info" if r["status"] == "ok" else "warn",
                f"API {url} → {r['status']} ({r['ms']}ms)",
            )

    return {"results": results, "ts": now()}


async def _check_single_url(
    client,
    url: str,
    timeout: int,
    headers: dict,
) -> dict:
    import time as _time

    r: dict[str, Any] = {
        "url": url, "status": "unknown", "code": None,
        "ms": None, "error": None, "headers": {},
        "content_type": None, "ts": now(),
    }
    try:
        t0   = _time.perf_counter()
        resp = await client.get(url, headers=headers)
        r["ms"]           = round((_time.perf_counter() - t0) * 1000, 1)
        r["code"]         = resp.status_code
        r["status"]       = "ok" if resp.status_code < 400 else "error"
        r["content_type"] = resp.headers.get("content-type", "")
        r["headers"]      = dict(list(resp.headers.items())[:10])
        if "json" in (r["content_type"] or ""):
            try:
                r["json_preview"] = str(resp.json())[:200]
            except Exception:
                pass
    except Exception as e:
        r["status"] = "down"
        r["error"]  = str(e)
    return r


# ── /analyze/logs ─────────────────────────────────────────────────────────────

@router.post("/logs-analyze")
async def analyze_logs(req: AnalyzeLogsRequest):
    """Equivalente a POST /analyze/logs del Flask original."""
    KEYWORDS = {
        "critical": ["CRITICAL", "FATAL"],
        "error":    ["ERROR", "Exception", "Traceback", "Error:"],
        "warning":  ["WARNING", "WARN", "DEPRECATED"],
        "info":     ["INFO"],
    }
    all_errors:   list[dict] = []
    all_warnings: list[dict] = []
    summary:      dict       = {}

    for f in req.files:
        name    = f.get("name", "log.txt")
        content = f.get("content", "")
        lines   = content.splitlines()
        counts  = {k: 0 for k in KEYWORDS}
        file_errors: list[int] = []

        for i, line in enumerate(lines, 1):
            for level, kws in KEYWORDS.items():
                if any(kw in line for kw in kws):
                    counts[level] += 1
                    if level in ("critical", "error"):
                        all_errors.append({
                            "file": name, "lineNo": i,
                            "level": level, "line": line.strip()[:200],
                        })
                        file_errors.append(i)
                    elif level == "warning":
                        all_warnings.append({
                            "file": name, "lineNo": i, "line": line.strip()[:200],
                        })
                    break

        summary[name] = {
            "total_lines": len(lines),
            "counts":      counts,
            "error_lines": file_errors[:20],
        }
        add_log("info", f"Log {name}: {counts['error']} errores")

    return {
        "errors":   all_errors[:100],
        "warnings": all_warnings[:100],
        "summary":  summary,
        "ts":       now(),
    }
