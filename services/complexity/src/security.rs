//! Security & Taint Intelligence (Fase 21 v1) — puerto 1:1 de la sección
//! "SECURITY & TAINT" en `static_parser.py` (`_taint_source_kind`,
//! `_expr_taint_info`, `_taint_propagation_python`, `_check_sql_injection`,
//! `_check_command_injection`, `_security_findings_python`,
//! `_hardcoded_credentials_python`). Mismas heurísticas, mismos umbrales —
//! ver ese archivo para el razonamiento completo de cada regla; acá solo la
//! traducción, con dos diferencias deliberadas:
//!
//! 1. Taint tracking usa `walk_stmts_own_scope` (no `walk_stmts`): una función
//!    anidada que reusa el nombre de una variable del scope externo no debe
//!    contaminar su taint — bug real encontrado y corregido primero en la
//!    versión Python, ver el CHANGELOG.
//! 2. Sin agregar el crate `regex` como dependencia nueva: los dos patrones
//!    que Python resuelve con regex (nombre de variable tipo credencial,
//!    valor tipo placeholder) son en la práctica listas de substrings/formas
//!    fijas — `looks_like_credential_name`/`looks_like_placeholder` abajo
//!    hacen lo mismo con matching de strings plano, misma filosofía de
//!    dependencias mínimas que ya tiene este Cargo.toml.

use std::collections::HashMap;

use rustpython_parser::ast::{Constant, Expr, Stmt};
use serde::Serialize;

use crate::parser::line_of_offset;
use crate::structure::node_name;
use crate::walk::{walk_stmts, walk_stmts_own_scope};

#[derive(Serialize, Clone)]
pub struct SecurityFinding {
    pub cwe: &'static str,
    pub category: &'static str,
    pub severity: &'static str,
    pub confidence: &'static str,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sink: Option<String>,
    pub line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
    pub recommendation: &'static str,
}

const HTTP_ATTRS: &[&str] = &["args", "form", "values", "json", "GET", "POST", "COOKIES", "headers"];
const SQL_SINK_METHODS: &[&str] = &["execute", "executemany"];
const SHELL_CALL_DOTTED: &[&str] = &["os.system", "os.popen"];
const SUBPROCESS_SHELL_FUNCS: &[&str] = &["call", "run", "Popen", "check_call", "check_output"];
const CREDENTIAL_NAME_NEEDLES: &[&str] = &[
    "password", "passwd", "pwd", "secret", "api_key", "apikey", "access_key", "accesskey", "auth_token",
    "authtoken", "private_key", "privatekey",
];

// ─── Taint sources ───────────────────────────────────────────────────────────

/// Fuentes no confiables reconocidas: solicitud HTTP (`request.args/form/
/// values/json/GET/POST/COOKIES/headers`), stdin (`input()`), argv
/// (`sys.argv`), variables de entorno (`os.environ`/`os.getenv`).
fn taint_source_kind(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Call(c) => {
            if let Expr::Name(n) = &*c.func {
                if n.id.as_str() == "input" || n.id.as_str() == "raw_input" {
                    return Some("stdin (input())".to_string());
                }
            }
            match node_name(&c.func).as_str() {
                "os.getenv" | "os.environ.get" => return Some("environment variable".to_string()),
                "request.get_json" => return Some("HTTP request".to_string()),
                _ => {}
            }
            if let Expr::Attribute(a) = &*c.func {
                if a.attr.as_str() == "get" {
                    let base = node_name(&a.value);
                    if base == "os.environ" {
                        return Some("environment variable".to_string());
                    }
                    if base.rsplit('.').next().is_some_and(|last| HTTP_ATTRS.contains(&last)) {
                        return Some("HTTP request".to_string());
                    }
                }
            }
            None
        }
        Expr::Subscript(s) => {
            match node_name(&s.value).as_str() {
                "sys.argv" => return Some("CLI argument".to_string()),
                "os.environ" => return Some("environment variable".to_string()),
                _ => {}
            }
            if let Expr::Attribute(a) = &*s.value {
                if HTTP_ATTRS.contains(&a.attr.as_str()) {
                    return Some("HTTP request".to_string());
                }
            }
            None
        }
        Expr::Attribute(a) if HTTP_ATTRS.contains(&a.attr.as_str()) => Some("HTTP request".to_string()),
        _ => None,
    }
}

/// (fuente, construido_por_concatenación) si `expr` es o contiene un valor
/// tainted. El flag "construido" es la señal real de riesgo para SQL/command
/// injection: True si el taint llegó a través de concatenación (BinOp,
/// incluye `%`), f-string, o `.format()`.
fn expr_taint_info(expr: &Expr, tainted: &HashMap<String, (String, bool)>) -> Option<(String, bool)> {
    if let Some(direct) = taint_source_kind(expr) {
        return Some((direct, false));
    }
    match expr {
        Expr::Name(n) => tainted.get(n.id.as_str()).cloned(),
        Expr::BinOp(b) => {
            let found = expr_taint_info(&b.left, tainted).or_else(|| expr_taint_info(&b.right, tainted));
            found.map(|(src, _)| (src, true))
        }
        Expr::JoinedStr(j) => j.values.iter().find_map(|v| match v {
            Expr::FormattedValue(fv) => expr_taint_info(&fv.value, tainted).map(|(src, _)| (src, true)),
            _ => None,
        }),
        Expr::Call(c) => {
            let is_format = matches!(&*c.func, Expr::Attribute(a) if a.attr.as_str() == "format");
            if !is_format {
                return None;
            }
            c.args
                .iter()
                .chain(c.keywords.iter().map(|kw| &kw.value))
                .find_map(|arg| expr_taint_info(arg, tainted).map(|(src, _)| (src, true)))
        }
        _ => None,
    }
}

/// Pasada única sobre las asignaciones de la función (scope propio, sin bajar
/// a funciones anidadas), en orden de aparición (el walker es DFS pre-order,
/// ya visita en orden de línea — a diferencia del `ast.walk` BFS de Python,
/// que necesitaba un sort explícito): si el lado derecho es o contiene un
/// valor tainted, el nombre del lado izquierdo queda tainted; si NO lo es, se
/// limpia cualquier taint previo de ese nombre.
fn taint_propagation(body: &[Stmt]) -> HashMap<String, (String, bool)> {
    let mut tainted: HashMap<String, (String, bool)> = HashMap::new();
    let mut on_stmt = |stmt: &Stmt| {
        if let Stmt::Assign(s) = stmt {
            let info = expr_taint_info(&s.value, &tainted);
            for target in &s.targets {
                if let Expr::Name(n) = target {
                    match &info {
                        Some(i) => {
                            tainted.insert(n.id.to_string(), i.clone());
                        }
                        None => {
                            tainted.remove(n.id.as_str());
                        }
                    }
                }
            }
        }
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
    tainted
}

// ─── Sinks ───────────────────────────────────────────────────────────────────

/// CWE-89. Solo dispara si el query se arma por concatenación/f-string/
/// `.format()` con un valor tainted — un query parametrizado real
/// (`cursor.execute("...%s...", (val,))`) no lo dispara, porque `args[0]` ahí
/// es un literal, no una expresión construida.
fn check_sql_injection(call: &rustpython_parser::ast::ExprCall, tainted: &HashMap<String, (String, bool)>, source: &str) -> Option<SecurityFinding> {
    let attr = match &*call.func {
        Expr::Attribute(a) => a,
        _ => return None,
    };
    if !SQL_SINK_METHODS.contains(&attr.attr.as_str()) {
        return None;
    }
    let query_expr = call.args.first()?;
    let (found_source, built) = expr_taint_info(query_expr, tainted)?;
    if !built {
        return None;
    }
    let base = node_name(&attr.value);
    let sink = if base != "?" { format!("{base}.{}(...)", attr.attr) } else { format!("{}(...)", attr.attr) };
    Some(SecurityFinding {
        cwe: "CWE-89",
        category: "SQL Injection",
        severity: "High",
        confidence: "High",
        source: found_source,
        sink: Some(sink),
        line: line_of_offset(source, call.range.start().to_usize()),
        function: None,
        recommendation: "Use a parameterized query (`?`/`%s` placeholders + a params tuple) instead of building the SQL string via concatenation/f-string.",
    })
}

/// CWE-78. `os.system`/`os.popen` siempre corren en shell, así que cualquier
/// taint alcanzando el comando alcanza — construido o no. `subprocess.*` solo
/// dispara con `shell=True` explícito.
fn check_command_injection(call: &rustpython_parser::ast::ExprCall, tainted: &HashMap<String, (String, bool)>, source: &str) -> Option<SecurityFinding> {
    let dotted = node_name(&call.func);
    let is_os_shell = SHELL_CALL_DOTTED.contains(&dotted.as_str());
    let is_subprocess_shell = matches!(&*call.func, Expr::Attribute(a) if SUBPROCESS_SHELL_FUNCS.contains(&a.attr.as_str()) && node_name(&a.value) == "subprocess")
        && call.keywords.iter().any(|kw| {
            kw.arg.as_ref().is_some_and(|a| a.as_str() == "shell")
                && matches!(&kw.value, Expr::Constant(c) if matches!(c.value, Constant::Bool(true)))
        });
    if !(is_os_shell || is_subprocess_shell) {
        return None;
    }
    let cmd_expr = call.args.first()?;
    let (found_source, _built) = expr_taint_info(cmd_expr, tainted)?;
    let sink_name = if dotted != "?" {
        dotted
    } else if let Expr::Attribute(a) = &*call.func {
        a.attr.to_string()
    } else {
        "?".to_string()
    };
    Some(SecurityFinding {
        cwe: "CWE-78",
        category: "Command Injection",
        severity: "High",
        confidence: "High",
        source: found_source,
        sink: Some(format!("{sink_name}(...)")),
        line: line_of_offset(source, call.range.start().to_usize()),
        function: None,
        recommendation: "Pass the command as a list of arguments (`subprocess.run([...])`, without `shell=True`) instead of a string built from external data.",
    })
}

/// Taint tracking + catálogo de sinks conocidos, dentro de una sola función.
pub fn security_findings(name: &str, body: &[Stmt], source: &str) -> Vec<SecurityFinding> {
    let tainted = taint_propagation(body);
    if tainted.is_empty() {
        return Vec::new();
    }
    let mut findings = Vec::new();
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |expr: &Expr| {
        if let Expr::Call(call) = expr {
            let finding = check_sql_injection(call, &tainted, source).or_else(|| check_command_injection(call, &tainted, source));
            if let Some(mut f) = finding {
                f.function = Some(name.to_string());
                findings.push(f);
            }
        }
    };
    walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
    findings.sort_by_key(|f| f.line);
    findings
}

// ─── Hardcoded credentials (CWE-798, a nivel de archivo) ─────────────────────

fn looks_like_credential_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    CREDENTIAL_NAME_NEEDLES.iter().any(|n| lower.contains(n))
}

fn looks_like_placeholder(value: &str) -> bool {
    let v = value.trim();
    if v.is_empty() {
        return true;
    }
    let lower = v.to_lowercase();
    if lower == "changeme" || lower == "todo" {
        return true;
    }
    if lower.len() >= 3 && lower.chars().all(|c| c == 'x') {
        return true;
    }
    if lower.starts_with("your") {
        return true;
    }
    if v.starts_with('<') && v.ends_with('>') {
        return true;
    }
    if v.starts_with("{{") && v.ends_with("}}") {
        return true;
    }
    if v.starts_with("${") && v.ends_with('}') {
        return true;
    }
    false
}

fn string_literal(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Constant(c) => match &c.value {
            Constant::Str(s) => Some(s.as_str()),
            _ => None,
        },
        _ => None,
    }
}

/// CWE-798. Dos señales requeridas: el nombre de variable sugiere una
/// credencial Y el valor es un string literal no vacío que no parece un
/// placeholder (`""`, `"changeme"`, `"<your-key>"`, `"${VAR}"`).
pub fn hardcoded_credentials(source: &str, suite: &[Stmt]) -> Vec<SecurityFinding> {
    let mut findings = Vec::new();
    let mut on_stmt = |stmt: &Stmt| {
        let (targets, value, offset): (&[Expr], &Expr, usize) = match stmt {
            Stmt::Assign(s) => (&s.targets, &s.value, s.range.start().to_usize()),
            Stmt::AnnAssign(s) => match &s.value {
                Some(v) => (std::slice::from_ref(&*s.target), v, s.range.start().to_usize()),
                None => return,
            },
            _ => return,
        };
        let Some(str_value) = string_literal(value) else { return };
        if looks_like_placeholder(str_value) {
            return;
        }
        for target in targets {
            if let Expr::Name(n) = target {
                if looks_like_credential_name(n.id.as_str()) {
                    findings.push(SecurityFinding {
                        cwe: "CWE-798",
                        category: "Hardcoded Credentials",
                        severity: "Medium",
                        confidence: "Medium",
                        source: format!("code literal (`{}`)", n.id),
                        sink: None,
                        line: line_of_offset(source, offset),
                        function: None,
                        recommendation: "Move the value to an environment variable or a secrets manager — don't leave it as a literal in source.",
                    });
                }
            }
        }
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(suite, &mut on_stmt, &mut on_expr);
    findings.sort_by_key(|f| f.line);
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_module;

    fn findings(src: &str) -> Vec<SecurityFinding> {
        let suite = parse_module(src).expect("parse error");
        let mut out = Vec::new();
        collect(&suite, src, &mut out);
        out.extend(hardcoded_credentials(src, &suite));
        out.sort_by_key(|f| f.line);
        out
    }

    fn collect(body: &[Stmt], source: &str, out: &mut Vec<SecurityFinding>) {
        for stmt in body {
            match stmt {
                Stmt::FunctionDef(f) => out.extend(security_findings(&f.name, &f.body, source)),
                Stmt::AsyncFunctionDef(f) => out.extend(security_findings(&f.name, &f.body, source)),
                Stmt::ClassDef(c) => collect(&c.body, source, out),
                Stmt::If(s) => {
                    collect(&s.body, source, out);
                    collect(&s.orelse, source, out);
                }
                _ => {}
            }
        }
    }

    #[test]
    fn sql_injection_concatenacion_detectada() {
        let src = "def get_user(request):\n    username = request.args[\"username\"]\n    db.execute(\"SELECT * FROM users WHERE name=\" + username)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-89" && x.confidence == "High"));
    }

    #[test]
    fn sql_injection_parametrizado_no_dispara() {
        let src = "def get_user(request):\n    username = request.args[\"username\"]\n    db.execute(\"SELECT * FROM users WHERE name=%s\", (username,))\n";
        let f = findings(src);
        assert!(!f.iter().any(|x| x.cwe == "CWE-89"));
    }

    #[test]
    fn command_injection_os_system_detectado() {
        let src = "import os\ndef run(request):\n    cmd = request.args[\"cmd\"]\n    os.system(cmd)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-78" && x.sink.as_deref() == Some("os.system(...)")));
    }

    #[test]
    fn command_injection_subprocess_sin_shell_no_dispara() {
        let src = "import subprocess\ndef run(user_input):\n    subprocess.run([\"ls\", user_input])\n";
        let f = findings(src);
        assert!(!f.iter().any(|x| x.cwe == "CWE-78"));
    }

    #[test]
    fn hardcoded_credential_detectado() {
        let src = "API_KEY = \"sk-live-abc123def456\"\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-798"));
    }

    #[test]
    fn placeholder_no_dispara() {
        let src = "DB_PASSWORD = \"\"\nSECRET_KEY = \"changeme\"\n";
        let f = findings(src);
        assert!(!f.iter().any(|x| x.cwe == "CWE-798"));
    }

    #[test]
    fn reasignacion_a_valor_seguro_limpia_taint() {
        let src = "import os\ndef run(request):\n    cmd = request.args.get(\"x\")\n    cmd = \"ls -la\"\n    os.system(cmd)\n";
        let f = findings(src);
        assert!(f.is_empty());
    }

    #[test]
    fn scope_anidado_no_filtra_taint() {
        let src = "import os\ndef outer(request):\n    cmd = \"ls -la\"\n    def inner():\n        cmd = request.args.get(\"x\")\n        return cmd\n    os.system(cmd)\n";
        let f = findings(src);
        assert!(!f.iter().any(|x| x.function.as_deref() == Some("outer")));
    }

    #[test]
    fn funcion_limpia_sin_findings() {
        let src = "def add(a, b):\n    return a + b\n";
        assert!(findings(src).is_empty());
    }
}
