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
//!
//! **Catálogo v2** (2026-09-01, 3 CWEs nuevos, ninguno existió nunca en
//! Python — la implementación Python de taint tracking ya estaba borrada
//! para cuando se agregaron, así que nacen 100% Rust-nativos): SSRF
//! (CWE-918, `check_ssrf`), Code Injection vía `eval`/`exec` (CWE-94,
//! `check_code_injection`), y XML External Entity vía `lxml.etree`
//! (CWE-611, `check_xxe`). Los 3 reusan el mismo taint tracker/catálogo de
//! sinks de v1, sin infraestructura nueva — 8 CWEs en total.
//!
//! **Catálogo v3** (2026-09-01, mismo día, 1 CWE más): Open Redirect
//! (CWE-601, `check_open_redirect`) — `redirect(url)`/`flask.redirect(url)`
//! con taint alcanzando el destino, mismo criterio "sin forma segura por
//! concatenación" que SSRF/Code Injection ya usan (dispara con cualquier
//! taint alcanzando el sink, construido o no). Primera combinación
//! severidad `Medium` + confianza `High` de este catálogo — es un
//! habilitador de phishing, no compromiso directo del sistema como
//! SQLi/RCE/SSRF/XXE, pero cuando el patrón taint+sink dispara la certeza
//! es la misma que cualquier otro finding `High`. 9 CWEs en total.
//!
//! **Catálogo v4** (2026-09-01, mismo día, 1 CWE más): Server-Side Template
//! Injection (CWE-1336, `check_ssti`) — `render_template_string(s)`/
//! `flask.render_template_string(s)` con `s` tainted. Misma familia "sin
//! forma segura, dispara con cualquier taint" que SSRF/Code
//! Injection/Open Redirect: a diferencia de `render_template` (que carga
//! una plantilla desde un ARCHIVO fijo y solo recibe datos de usuario como
//! variables de contexto, la forma segura), `render_template_string`
//! COMPILA el string recibido como plantilla Jinja2 — si ese string
//! contiene datos de usuario, el atacante puede inyectar sintaxis Jinja2
//! propia (`{{ config.__class__.__init__.__globals__... }}`) y llegar a
//! ejecución remota de código completa, no solo XSS. Severidad `High`,
//! misma categoría de impacto que Code Injection. 10 CWEs en total.
//!
//! **Catálogo v5** (2026-09-02, 2 CWEs más — y la primera familia de este
//! catálogo que NO depende del taint tracker): Broken Cryptographic
//! Algorithm (CWE-327, `check_weak_crypto`) e Improper Certificate
//! Validation (CWE-295, `check_tls_verification_disabled`). Los 10 CWEs
//! anteriores comparten la misma pregunta — "¿llega dato externo a este
//! sink?" — y por eso todos toman el mapa de taint. Estos 2 preguntan otra
//! cosa: qué primitiva se eligió. `hashlib.md5(...)` es igual de roto con
//! datos internos que con datos del usuario, y `requests.get(url,
//! verify=False)` acepta el certificado de cualquiera sin importar de dónde
//! salga la URL. Por eso sus funciones no reciben `tainted` — no es una
//! omisión, es la diferencia de fondo entre las 2 familias. 12 CWEs en
//! total.
//!
//! **Bug real de cobertura arreglado (2026-09-02, encontrado durante la
//! Fase 27, Jupyter Notebook analysis)**: `security_findings()` de abajo
//! siempre fue genérica sobre cualquier `&[Stmt]` — nunca asumió que `body`
//! tenía que ser literalmente el cuerpo de una función — pero
//! `rich.rs::collect_functions` solo la llamaba una vez POR FUNCIÓN del
//! archivo, así que cualquier sentencia a nivel de MÓDULO (fuera de
//! cualquier `def`) nunca pasaba por ningún camino de taint tracking, sin
//! importar qué tan obviamente tainted estuviera. `rich.rs::analyze_rich`
//! ahora también llama a `security_findings("<module>", &suite, ...)` una
//! vez sobre el `suite` completo — como esta función ya usa
//! `walk_stmts_own_scope` internamente, esa sola llamada extra no
//! desciende a los cuerpos de las funciones ya cubiertas (sin duplicar
//! hallazgos) y sí atrapa taint que fluye directo a nivel de módulo.
//! `"<module>"` como nombre de scope — misma convención que Python usa en
//! sus propios tracebacks.

use std::collections::HashMap;

use rustpython_parser::ast::{Constant, Expr, Stmt};
use serde::Serialize;

use crate::ast::parser::LineIndex;
use crate::ast::structure::node_name;
use crate::ast::walk::{walk_stmts, walk_stmts_own_scope};

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
const UNSAFE_DESERIALIZE_DOTTED: &[&str] = &["pickle.loads", "pickle.load", "marshal.loads"];
const SSRF_SINK_DOTTED: &[&str] = &[
    "requests.get", "requests.post", "requests.put", "requests.delete", "requests.head", "requests.patch",
    "requests.request", "urllib.request.urlopen",
];
const CODE_EXEC_NAMES: &[&str] = &["eval", "exec"];
// Cubre tanto `import lxml.etree` (`lxml.etree.parse(...)`) como
// `from lxml import etree` (`etree.parse(...)`) — mismo criterio de nombre
// escrito tal cual, sin resolver alias de import, que ya usa el resto de
// este archivo (`UNSAFE_DESERIALIZE_DOTTED`/`yaml.load`). Límite aceptado:
// alguien que hace `import xml.etree.ElementTree as etree` (inusual, pero
// posible) también matchearía `etree.parse` acá aunque sea el stdlib, no
// lxml — mismo tipo de simplificación por nombre que el resto del motor.
const XXE_UNSAFE_DOTTED: &[&str] = &["lxml.etree.parse", "lxml.etree.fromstring", "lxml.etree.XML", "etree.parse", "etree.fromstring", "etree.XML"];
// `redirect` (Flask, `from flask import redirect`) y `flask.redirect` (`import
// flask`) — mismo criterio de nombre escrito tal cual que el resto del
// archivo. Django's `django.shortcuts.redirect` queda fuera de esta primera
// porción a propósito: acepta tanto una URL como un nombre de vista/modelo,
// y distinguir esos 2 casos sin tipado real agregaría ambigüedad que Flask's
// `redirect` (siempre recibe una URL/path) no tiene.
const OPEN_REDIRECT_NAMES: &[&str] = &["redirect"];
const OPEN_REDIRECT_DOTTED: &[&str] = &["flask.redirect"];
// `render_template_string` (`from flask import render_template_string`) y
// `flask.render_template_string` (`import flask`) — mismo criterio de
// nombre escrito tal cual que el resto del archivo. Deliberadamente NO
// `render_template` (carga una plantilla desde archivo, la forma segura —
// nunca comparte este sink) ni `jinja2.Template(...)` directo (constructor
// de la librería subyacente sin pasar por Flask, fuera de alcance de esta
// primera porción).
const SSTI_SINK_NAMES: &[&str] = &["render_template_string"];
const SSTI_SINK_DOTTED: &[&str] = &["flask.render_template_string"];
// CWE-327 (catálogo v5) — hashes rotos para uso criptográfico. MD5 y SHA-1
// tienen colisiones prácticas demostradas (SHAttered, 2017); ninguno sirve
// para firmas, integridad ante un atacante, ni derivación de contraseñas.
// `hashlib.md5` (import del módulo) y `md5` pelado (`from hashlib import
// md5`) — mismo criterio de nombre escrito tal cual que el resto del
// archivo. Deliberadamente NO se incluyen SHA-256 y superiores (siguen
// siendo recomendados) ni `hashlib.new("md5")` con el algoritmo como string
// dinámico — ver `check_weak_crypto` para el caso del literal.
const WEAK_HASH_NAMES: &[&str] = &["md5", "sha1"];
// Cifradores simétricos rotos de pycryptodome/PyCrypto: DES (clave de 56
// bits, rompible por fuerza bruta hace décadas) y ARC4/RC4 (sesgos
// estadísticos explotables, prohibido en TLS desde RFC 7465). Se matchea el
// `.new(...)` que ambos usan como constructor.
const WEAK_CIPHER_DOTTED: &[&str] = &[
    "DES.new", "ARC4.new", "Crypto.Cipher.DES.new", "Crypto.Cipher.ARC4.new",
    "Cryptodome.Cipher.DES.new", "Cryptodome.Cipher.ARC4.new",
];
// CWE-295 (catálogo v5) — clientes HTTP cuyo `verify=False` desactiva la
// validación del certificado TLS. Acotado a estos 2 prefijos a propósito:
// `verify` es un nombre de keyword genérico que cualquier función podría
// usar para otra cosa, así que sin anclar el callee a una librería HTTP
// conocida el patrón dispararía sobre código no relacionado.
const TLS_CLIENT_PREFIXES: &[&str] = &["requests.", "httpx."];

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
fn check_sql_injection(call: &rustpython_parser::ast::ExprCall, tainted: &HashMap<String, (String, bool)>, line_index: &LineIndex) -> Option<SecurityFinding> {
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
        line: line_index.line_of(call.range.start().to_usize()),
        function: None,
        recommendation: "Use a parameterized query (`?`/`%s` placeholders + a params tuple) instead of building the SQL string via concatenation/f-string.",
    })
}

/// CWE-78. `os.system`/`os.popen` siempre corren en shell, así que cualquier
/// taint alcanzando el comando alcanza — construido o no. `subprocess.*` solo
/// dispara con `shell=True` explícito.
fn check_command_injection(call: &rustpython_parser::ast::ExprCall, tainted: &HashMap<String, (String, bool)>, line_index: &LineIndex) -> Option<SecurityFinding> {
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
        line: line_index.line_of(call.range.start().to_usize()),
        function: None,
        recommendation: "Pass the command as a list of arguments (`subprocess.run([...])`, without `shell=True`) instead of a string built from external data.",
    })
}

/// CWE-22. `open(path)` only fires when the path is *built* via concatenation/
/// f-string/`.format()` with a tainted value — same "built" signal as SQL
/// injection, so `open(f"uploads/{name}")` fires but `open(config_path)`
/// (a plain variable, not concatenated) doesn't. `os.path.join(base, seg)`
/// fires on any tainted non-first segment, built or not — joining a raw
/// tainted `"../etc/passwd"` is already the traversal, no concatenation
/// needed to make it dangerous.
fn check_path_traversal(call: &rustpython_parser::ast::ExprCall, tainted: &HashMap<String, (String, bool)>, line_index: &LineIndex) -> Option<SecurityFinding> {
    if let Expr::Name(n) = &*call.func {
        if n.id.as_str() == "open" {
            let path_expr = call.args.first()?;
            let (found_source, built) = expr_taint_info(path_expr, tainted)?;
            if !built {
                return None;
            }
            return Some(SecurityFinding {
                cwe: "CWE-22",
                category: "Path Traversal",
                severity: "High",
                confidence: "High",
                source: found_source,
                sink: Some("open(...)".to_string()),
                line: line_index.line_of(call.range.start().to_usize()),
                function: None,
                recommendation: "Validate/normalize the path against an allow-listed base directory (e.g. `Path(base).joinpath(name).resolve()` and check it's still under `base`) before opening it.",
            });
        }
    }
    if node_name(&call.func) == "os.path.join" && call.args.len() > 1 {
        for arg in &call.args[1..] {
            if let Some((found_source, _built)) = expr_taint_info(arg, tainted) {
                return Some(SecurityFinding {
                    cwe: "CWE-22",
                    category: "Path Traversal",
                    severity: "High",
                    confidence: "High",
                    source: found_source,
                    sink: Some("os.path.join(...)".to_string()),
                    line: line_index.line_of(call.range.start().to_usize()),
                    function: None,
                    recommendation: "Validate/normalize the joined path against an allow-listed base directory before using it — `os.path.join` doesn't strip `..` segments.",
                });
            }
        }
    }
    None
}

/// CWE-502. `pickle.loads`/`pickle.load`/`marshal.loads` are unsafe by
/// construction — they can execute arbitrary code when deserializing,
/// regardless of whether the input is provably tainted (unlike SQL/command
/// injection, no "safe form" exists for these calls). `yaml.load` is only
/// unsafe without an explicit `Loader=yaml.SafeLoader`/`CSafeLoader`.
/// Confidence is High when the argument traces to a known taint source,
/// Medium otherwise — using these functions at all is a real risk, tainted
/// input flowing into them is worse.
fn check_insecure_deserialization(call: &rustpython_parser::ast::ExprCall, tainted: &HashMap<String, (String, bool)>, line_index: &LineIndex) -> Option<SecurityFinding> {
    let dotted = node_name(&call.func);
    let is_pickle_marshal = UNSAFE_DESERIALIZE_DOTTED.contains(&dotted.as_str());
    let is_unsafe_yaml = dotted == "yaml.load"
        && !call.keywords.iter().any(|kw| {
            kw.arg.as_ref().is_some_and(|a| a.as_str() == "Loader")
                && matches!(&kw.value, Expr::Attribute(a) if a.attr.as_str().contains("SafeLoader"))
        });
    if !(is_pickle_marshal || is_unsafe_yaml) {
        return None;
    }
    let arg_taint = call.args.first().and_then(|a| expr_taint_info(a, tainted));
    let (confidence, found_source) = match arg_taint {
        Some((src, _)) => ("High", src),
        None => ("Medium", "unvalidated deserialization call".to_string()),
    };
    let recommendation = if is_unsafe_yaml {
        "Use `yaml.safe_load()` (or `Loader=yaml.SafeLoader`) instead of the default `yaml.load`, which can construct arbitrary Python objects."
    } else {
        "Avoid deserializing untrusted data with `pickle`/`marshal` — use a safe format (JSON) or a schema-validated serializer instead."
    };
    Some(SecurityFinding {
        cwe: "CWE-502",
        category: "Insecure Deserialization",
        severity: "High",
        confidence,
        source: found_source,
        sink: Some(format!("{dotted}(...)")),
        line: line_index.line_of(call.range.start().to_usize()),
        function: None,
        recommendation,
    })
}

/// CWE-918. `requests.get/post/put/delete/head/patch/request` y
/// `urllib.request.urlopen` — a diferencia de SQL Injection/Path Traversal,
/// no hay una "forma parametrizada" segura para la URL de un request HTTP:
/// cualquier taint que llegue a la URL (concatenada o no) ya es el riesgo,
/// mismo criterio que `os.path.join` ya usa para Path Traversal (dispara
/// aunque no haya concatenación de por medio). Límite aceptado, no oculto:
/// no distingue taint en el host (el riesgo real de SSRF) de taint en un
/// segmento de path de una URL por lo demás fija — mismo tipo de
/// simplificación que el resto de este motor ya acepta.
fn check_ssrf(call: &rustpython_parser::ast::ExprCall, tainted: &HashMap<String, (String, bool)>, line_index: &LineIndex) -> Option<SecurityFinding> {
    let dotted = node_name(&call.func);
    if !SSRF_SINK_DOTTED.contains(&dotted.as_str()) {
        return None;
    }
    let url_expr = call.args.first()?;
    let (found_source, _built) = expr_taint_info(url_expr, tainted)?;
    Some(SecurityFinding {
        cwe: "CWE-918",
        category: "Server-Side Request Forgery",
        severity: "High",
        confidence: "High",
        source: found_source,
        sink: Some(format!("{dotted}(...)")),
        line: line_index.line_of(call.range.start().to_usize()),
        function: None,
        recommendation: "Validate the target host against an allow-list of known destinations before making the request — don't let external input choose which host/URL this process connects to.",
    })
}

/// CWE-94. `eval`/`exec` sobre un valor tainted: sin forma segura posible
/// (a diferencia de `os.system`, no existe un equivalente a "lista de
/// argumentos" que evite ejecutar código arbitrario) — dispara con
/// cualquier taint alcanzando el argumento, construido o no, mismo criterio
/// que `os.system`/`os.popen` ya usan para Command Injection.
fn check_code_injection(call: &rustpython_parser::ast::ExprCall, tainted: &HashMap<String, (String, bool)>, line_index: &LineIndex) -> Option<SecurityFinding> {
    let Expr::Name(n) = &*call.func else { return None };
    if !CODE_EXEC_NAMES.contains(&n.id.as_str()) {
        return None;
    }
    let code_expr = call.args.first()?;
    let (found_source, _built) = expr_taint_info(code_expr, tainted)?;
    Some(SecurityFinding {
        cwe: "CWE-94",
        category: "Code Injection",
        severity: "High",
        confidence: "High",
        source: found_source,
        sink: Some(format!("{}(...)", n.id)),
        line: line_index.line_of(call.range.start().to_usize()),
        function: None,
        recommendation: "Never pass externally-influenced data to `eval`/`exec` — use `ast.literal_eval` for data, or a proper parser/dispatch table instead of executing a string as code.",
    })
}

/// CWE-611. `lxml.etree.parse`/`fromstring`/`XML` son inseguros por
/// default (resuelven entidades externas, el vector clásico de XXE) A
/// MENOS que se pase un `parser=etree.XMLParser(resolve_entities=False)`
/// explícito — mismo criterio "inseguro por default salvo flag explícita"
/// que `yaml.load`/`Loader=yaml.SafeLoader` ya usa. Confianza `High` cuando
/// el argumento de entrada rastrea a un taint conocido, `Medium` si no (la
/// llamada insegura sola ya es un riesgo real, igual que pickle/marshal).
/// Límite aceptado: solo reconoce un `XMLParser(...)` construido INLINE en
/// el propio keyword `parser=` — un parser seguro guardado en una variable
/// (`parser=my_safe_parser`) no se reconoce como seguro y produciría un
/// falso positivo, documentado acá en vez de escondido.
fn check_xxe(call: &rustpython_parser::ast::ExprCall, tainted: &HashMap<String, (String, bool)>, line_index: &LineIndex) -> Option<SecurityFinding> {
    let dotted = node_name(&call.func);
    if !XXE_UNSAFE_DOTTED.contains(&dotted.as_str()) {
        return None;
    }
    let has_safe_parser = call.keywords.iter().any(|kw| {
        kw.arg.as_ref().is_some_and(|a| a.as_str() == "parser")
            && matches!(&kw.value, Expr::Call(pc) if node_name(&pc.func).ends_with("XMLParser")
                && pc.keywords.iter().any(|k| {
                    k.arg.as_ref().is_some_and(|a| a.as_str() == "resolve_entities")
                        && matches!(&k.value, Expr::Constant(c) if matches!(c.value, Constant::Bool(false)))
                }))
    });
    if has_safe_parser {
        return None;
    }
    let arg_taint = call.args.first().and_then(|a| expr_taint_info(a, tainted));
    let (confidence, found_source) = match arg_taint {
        Some((src, _)) => ("High", src),
        None => ("Medium", "unvalidated XML parse call".to_string()),
    };
    Some(SecurityFinding {
        cwe: "CWE-611",
        category: "XML External Entity",
        severity: "High",
        confidence,
        source: found_source,
        sink: Some(format!("{dotted}(...)")),
        line: line_index.line_of(call.range.start().to_usize()),
        function: None,
        recommendation: "Use `etree.XMLParser(resolve_entities=False)` (or the `defusedxml` package) instead of parsing XML with entity resolution enabled by default.",
    })
}

/// CWE-601. `redirect(url)`/`flask.redirect(url)` con un valor tainted —
/// mismo criterio "sin forma segura por concatenación" que SSRF/Code
/// Injection: la vulnerabilidad es dejar que el destino completo del
/// redirect lo elija el atacante, dispara con cualquier taint alcanzando el
/// argumento, construido o no. Deliberadamente NO cubre
/// `django.shortcuts.redirect` (ver la constante de arriba) ni intenta
/// distinguir un path relativo seguro (`redirect("/dashboard")`, un literal,
/// nunca dispara igual) de una URL absoluta controlada por el atacante —
/// misma granularidad que el resto de este motor.
fn check_open_redirect(call: &rustpython_parser::ast::ExprCall, tainted: &HashMap<String, (String, bool)>, line_index: &LineIndex) -> Option<SecurityFinding> {
    let dotted = node_name(&call.func);
    let is_bare_name = matches!(&*call.func, Expr::Name(n) if OPEN_REDIRECT_NAMES.contains(&n.id.as_str()));
    let is_dotted = OPEN_REDIRECT_DOTTED.contains(&dotted.as_str());
    if !(is_bare_name || is_dotted) {
        return None;
    }
    let url_expr = call.args.first()?;
    let (found_source, _built) = expr_taint_info(url_expr, tainted)?;
    Some(SecurityFinding {
        cwe: "CWE-601",
        category: "Open Redirect",
        severity: "Medium",
        confidence: "High",
        source: found_source,
        // `dotted` ya trae el nombre real matcheado (`redirect` o
        // `flask.redirect`) -- bug real encontrado en auditoría: antes esto
        // hardcodeaba el literal "redirect(...)" sin importar cuál de las 2
        // formas disparó, perdiendo la información de `flask.redirect` que
        // el resto del catálogo (`check_ssrf`/`check_code_injection`/
        // `check_xxe`) sí reporta con `format!("{dotted}(...)")`.
        sink: Some(format!("{dotted}(...)")),
        line: line_index.line_of(call.range.start().to_usize()),
        function: None,
        recommendation: "Validate the redirect target against an allow-list of known-safe paths/hosts, or only allow relative paths, before redirecting there.",
    })
}

/// CWE-1336. `render_template_string(s)`/`flask.render_template_string(s)`
/// con `s` tainted — mismo criterio "sin forma segura, dispara con
/// cualquier taint" que SSRF/Code Injection/Open Redirect ya usan.
/// `render_template_string` COMPILA el string recibido como plantilla
/// Jinja2 antes de renderizarla — a diferencia de `render_template` (que
/// carga una plantilla desde un archivo fijo y solo recibe datos de usuario
/// como variables de CONTEXTO, nunca como el texto de la plantilla en sí,
/// la forma segura que nunca comparte este sink), acá el string ENTERO se
/// interpreta como sintaxis de plantilla — si contiene datos de usuario, el
/// atacante puede inyectar su propia sintaxis Jinja2 y llegar a ejecución
/// remota de código, no solo XSS.
fn check_ssti(call: &rustpython_parser::ast::ExprCall, tainted: &HashMap<String, (String, bool)>, line_index: &LineIndex) -> Option<SecurityFinding> {
    let dotted = node_name(&call.func);
    let is_bare_name = matches!(&*call.func, Expr::Name(n) if SSTI_SINK_NAMES.contains(&n.id.as_str()));
    let is_dotted = SSTI_SINK_DOTTED.contains(&dotted.as_str());
    if !(is_bare_name || is_dotted) {
        return None;
    }
    let template_expr = call.args.first()?;
    let (found_source, _built) = expr_taint_info(template_expr, tainted)?;
    Some(SecurityFinding {
        cwe: "CWE-1336",
        category: "Server-Side Template Injection",
        severity: "High",
        confidence: "High",
        source: found_source,
        sink: Some(format!("{dotted}(...)")),
        line: line_index.line_of(call.range.start().to_usize()),
        function: None,
        recommendation: "Never build the template STRING from user input — use `render_template` with a fixed template file, passing user data only as a context variable, never as part of the template source itself.",
    })
}

/// CWE-327 (catálogo v5). Primer check de este archivo que NO mira taint en
/// absoluto: el defecto no es de dónde vienen los datos, es el algoritmo
/// elegido. `hashlib.md5(...)`/`hashlib.sha1(...)` (o `md5`/`sha1` pelados,
/// `from hashlib import md5`), y `DES.new`/`ARC4.new` de pycryptodome.
///
/// **Escape hatch real, no inventado**: Python 3.9+ acepta
/// `hashlib.md5(data, usedforsecurity=False)` justamente para declarar
/// "esto es un checksum, no seguridad" — cuando está presente, no hay
/// hallazgo. Mismo criterio "inseguro por default salvo flag explícita" que
/// `yaml.load`/`Loader=SafeLoader` (CWE-502) y `XMLParser(resolve_entities=
/// False)` (CWE-611) ya usan en este mismo archivo.
///
/// Confianza `Medium`, no `High`: MD5 sigue siendo una elección legítima
/// para cosas que no son seguridad (deduplicación de archivos, claves de
/// caché, hashing de particiones) y no todo ese código pasa el
/// `usedforsecurity=False`. El hallazgo señala el algoritmo roto, no afirma
/// haber probado que se use para seguridad — misma disciplina de
/// "evidencia, nunca un veredicto" del resto del motor.
fn check_weak_crypto(call: &rustpython_parser::ast::ExprCall, line_index: &LineIndex) -> Option<SecurityFinding> {
    let dotted = node_name(&call.func);
    let is_weak_hash = match &*call.func {
        Expr::Name(n) => WEAK_HASH_NAMES.contains(&n.id.as_str()),
        Expr::Attribute(a) => dotted.starts_with("hashlib.") && WEAK_HASH_NAMES.contains(&a.attr.as_str()),
        _ => false,
    };
    let is_weak_cipher = WEAK_CIPHER_DOTTED.contains(&dotted.as_str());
    if !(is_weak_hash || is_weak_cipher) {
        return None;
    }
    // `usedforsecurity=False` solo aplica a los hashes de `hashlib` — los
    // cifradores no tienen (ni tendrían sentido con) esa flag.
    let declared_non_security = call.keywords.iter().any(|kw| {
        kw.arg.as_ref().is_some_and(|a| a.as_str() == "usedforsecurity")
            && matches!(&kw.value, Expr::Constant(c) if matches!(c.value, Constant::Bool(false)))
    });
    if is_weak_hash && declared_non_security {
        return None;
    }
    Some(SecurityFinding {
        cwe: "CWE-327",
        category: "Broken Cryptographic Algorithm",
        severity: "Medium",
        confidence: "Medium",
        source: if is_weak_cipher {
            "broken symmetric cipher".to_string()
        } else {
            "broken hash algorithm".to_string()
        },
        sink: Some(format!("{dotted}(...)")),
        line: line_index.line_of(call.range.start().to_usize()),
        function: None,
        recommendation: if is_weak_cipher {
            "DES and RC4 are broken — use AES-GCM (or ChaCha20-Poly1305) from a maintained library instead."
        } else {
            "MD5/SHA-1 have practical collisions — use SHA-256+ for integrity, or a password KDF (`hashlib.scrypt`, `argon2`, `bcrypt`) for passwords. If this hash is genuinely not security-related, pass `usedforsecurity=False` to say so explicitly."
        },
    })
}

/// CWE-295 (catálogo v5). Segundo check sin taint, y el más mecánico de
/// todo el catálogo: `requests.get(url, verify=False)` (o cualquier otra
/// función de `requests.`/`httpx.`) desactiva la validación del certificado
/// TLS por completo — la conexión sigue cifrada, pero contra cualquiera que
/// se ponga en el medio, que es exactamente lo que el certificado existe
/// para impedir. También `ssl._create_unverified_context()`, cuyo propio
/// guión bajo inicial ya declara que no es API pública.
///
/// Confianza `High`: a diferencia de MD5 (que tiene usos legítimos fuera de
/// seguridad), `verify=False` no tiene ninguna lectura benigna en producción
/// — apagar la validación es exactamente lo que hace, siempre.
fn check_tls_verification_disabled(call: &rustpython_parser::ast::ExprCall, line_index: &LineIndex) -> Option<SecurityFinding> {
    let dotted = node_name(&call.func);
    if dotted == "ssl._create_unverified_context" || dotted == "_create_unverified_context" {
        return Some(SecurityFinding {
            cwe: "CWE-295",
            category: "Improper Certificate Validation",
            severity: "High",
            confidence: "High",
            source: "unverified SSL context".to_string(),
            sink: Some(format!("{dotted}()")),
            line: line_index.line_of(call.range.start().to_usize()),
            function: None,
            recommendation: "Use `ssl.create_default_context()` — an unverified context accepts any certificate, including an attacker's.",
        });
    }
    if !TLS_CLIENT_PREFIXES.iter().any(|p| dotted.starts_with(p)) {
        return None;
    }
    let disabled = call.keywords.iter().any(|kw| {
        kw.arg.as_ref().is_some_and(|a| a.as_str() == "verify")
            && matches!(&kw.value, Expr::Constant(c) if matches!(c.value, Constant::Bool(false)))
    });
    if !disabled {
        return None;
    }
    Some(SecurityFinding {
        cwe: "CWE-295",
        category: "Improper Certificate Validation",
        severity: "High",
        confidence: "High",
        source: "verify=False".to_string(),
        sink: Some(format!("{dotted}(...)")),
        line: line_index.line_of(call.range.start().to_usize()),
        function: None,
        recommendation: "Don't disable certificate verification — point the client at the right CA bundle (`verify=\"/path/to/ca.pem\"`) instead of accepting any certificate.",
    })
}

/// Taint tracking + catálogo de sinks conocidos, dentro de una sola función.
/// A diferencia de v1 (solo SQLi/command injection, ambos requieren taint),
/// ya no corta temprano si `tainted` está vacío — la deserialización insegura
/// (CWE-502) dispara aunque el argumento no sea provably tainted, y los 2
/// checks del catálogo v5 (CWE-327/CWE-295) no miran el taint map en
/// absoluto: su defecto es la primitiva elegida, no el origen de los datos.
pub fn security_findings(name: &str, body: &[Stmt], line_index: &LineIndex) -> Vec<SecurityFinding> {
    let tainted = taint_propagation(body);
    let mut findings = Vec::new();
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |expr: &Expr| {
        if let Expr::Call(call) = expr {
            let finding = check_sql_injection(call, &tainted, line_index)
                .or_else(|| check_command_injection(call, &tainted, line_index))
                .or_else(|| check_path_traversal(call, &tainted, line_index))
                .or_else(|| check_insecure_deserialization(call, &tainted, line_index))
                .or_else(|| check_ssrf(call, &tainted, line_index))
                .or_else(|| check_code_injection(call, &tainted, line_index))
                .or_else(|| check_xxe(call, &tainted, line_index))
                .or_else(|| check_open_redirect(call, &tainted, line_index))
                .or_else(|| check_ssti(call, &tainted, line_index))
                .or_else(|| check_weak_crypto(call, line_index))
                .or_else(|| check_tls_verification_disabled(call, line_index));
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
pub fn hardcoded_credentials(line_index: &LineIndex, suite: &[Stmt]) -> Vec<SecurityFinding> {
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
                        line: line_index.line_of(offset),
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
    use crate::ast::parser::parse_module;

    fn findings(src: &str) -> Vec<SecurityFinding> {
        let suite = parse_module(src).expect("parse error");
        let line_index = LineIndex::build(src);
        let mut out = Vec::new();
        collect(&suite, &line_index, &mut out);
        out.extend(hardcoded_credentials(&line_index, &suite));
        out.sort_by_key(|f| f.line);
        out
    }

    fn collect(body: &[Stmt], line_index: &LineIndex, out: &mut Vec<SecurityFinding>) {
        for stmt in body {
            match stmt {
                Stmt::FunctionDef(f) => out.extend(security_findings(&f.name, &f.body, line_index)),
                Stmt::AsyncFunctionDef(f) => out.extend(security_findings(&f.name, &f.body, line_index)),
                Stmt::ClassDef(c) => collect(&c.body, line_index, out),
                Stmt::If(s) => {
                    collect(&s.body, line_index, out);
                    collect(&s.orelse, line_index, out);
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

    #[test]
    fn path_traversal_open_concatenado_detectado() {
        let src = "def read_file(request):\n    name = request.args[\"name\"]\n    with open(\"uploads/\" + name) as f:\n        return f.read()\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-22" && x.sink.as_deref() == Some("open(...)")));
    }

    #[test]
    fn path_traversal_open_variable_simple_no_dispara() {
        let src = "def read_file(config_path):\n    with open(config_path) as f:\n        return f.read()\n";
        let f = findings(src);
        assert!(!f.iter().any(|x| x.cwe == "CWE-22"));
    }

    #[test]
    fn path_traversal_os_path_join_detectado() {
        let src = "import os\ndef read_file(request):\n    name = request.args[\"name\"]\n    path = os.path.join(\"uploads\", name)\n    open(path)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-22" && x.sink.as_deref() == Some("os.path.join(...)")));
    }

    #[test]
    fn insecure_deserialization_pickle_loads_tainted_high_confidence() {
        let src = "import pickle\ndef load(request):\n    data = request.args[\"data\"]\n    return pickle.loads(data)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-502" && x.confidence == "High"));
    }

    #[test]
    fn insecure_deserialization_pickle_loads_untainted_medium_confidence() {
        let src = "import pickle\ndef load(raw_bytes):\n    return pickle.loads(raw_bytes)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-502" && x.confidence == "Medium"));
    }

    #[test]
    fn insecure_deserialization_yaml_load_default_loader_detectado() {
        let src = "import yaml\ndef load(raw):\n    return yaml.load(raw)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-502"));
    }

    #[test]
    fn insecure_deserialization_yaml_safe_load_no_dispara() {
        let src = "import yaml\ndef load(raw):\n    return yaml.load(raw, Loader=yaml.SafeLoader)\n";
        let f = findings(src);
        assert!(!f.iter().any(|x| x.cwe == "CWE-502"));
    }

    #[test]
    fn ssrf_requests_get_tainted_detectado() {
        let src = "import requests\ndef fetch(request):\n    url = request.args[\"url\"]\n    return requests.get(url)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-918" && x.sink.as_deref() == Some("requests.get(...)")));
    }

    #[test]
    fn ssrf_urllib_urlopen_tainted_detectado() {
        let src = "import urllib.request\ndef fetch(request):\n    url = request.args[\"url\"]\n    return urllib.request.urlopen(url)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-918"));
    }

    #[test]
    fn ssrf_url_fija_no_dispara() {
        let src = "import requests\ndef fetch():\n    return requests.get(\"https://api.example.com/health\")\n";
        let f = findings(src);
        assert!(!f.iter().any(|x| x.cwe == "CWE-918"));
    }

    #[test]
    fn code_injection_eval_tainted_detectado() {
        let src = "def run(request):\n    expr = request.args[\"expr\"]\n    return eval(expr)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-94" && x.sink.as_deref() == Some("eval(...)")));
    }

    #[test]
    fn code_injection_exec_tainted_detectado() {
        let src = "def run(request):\n    code = request.args[\"code\"]\n    exec(code)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-94" && x.sink.as_deref() == Some("exec(...)")));
    }

    #[test]
    fn code_injection_eval_literal_no_dispara() {
        let src = "def run():\n    return eval(\"1 + 1\")\n";
        let f = findings(src);
        assert!(!f.iter().any(|x| x.cwe == "CWE-94"));
    }

    #[test]
    fn xxe_lxml_etree_parse_tainted_high_confidence() {
        let src = "from lxml import etree\ndef load(request):\n    data = request.args[\"xml\"]\n    return etree.fromstring(data)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-611" && x.confidence == "High"));
    }

    #[test]
    fn xxe_lxml_etree_parse_untainted_medium_confidence() {
        let src = "import lxml.etree\ndef load(raw_bytes):\n    return lxml.etree.fromstring(raw_bytes)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-611" && x.confidence == "Medium"));
    }

    #[test]
    fn xxe_lxml_con_parser_seguro_no_dispara() {
        let src = "from lxml import etree\ndef load(raw):\n    parser = etree.XMLParser(resolve_entities=False)\n    return etree.fromstring(raw, parser=etree.XMLParser(resolve_entities=False))\n";
        let f = findings(src);
        assert!(!f.iter().any(|x| x.cwe == "CWE-611"));
    }

    #[test]
    fn xxe_lxml_con_parser_sin_resolve_entities_false_dispara() {
        // `parser=etree.XMLParser()` sin `resolve_entities=False` explícito
        // sigue siendo inseguro (ese es el default de lxml) — el check no
        // debe confundir "pasa ALGÚN parser" con "pasa un parser seguro".
        let src = "from lxml import etree\ndef load(raw):\n    return etree.fromstring(raw, parser=etree.XMLParser())\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-611"));
    }

    #[test]
    fn open_redirect_flask_redirect_bare_name_tainted_detectado() {
        let src = "from flask import redirect\ndef go(request):\n    target = request.args[\"next\"]\n    return redirect(target)\n";
        let f = findings(src);
        let r = f.iter().find(|x| x.cwe == "CWE-601").unwrap();
        assert_eq!(r.severity, "Medium");
        assert_eq!(r.confidence, "High");
        assert_eq!(r.sink.as_deref(), Some("redirect(...)"));
    }

    #[test]
    fn open_redirect_flask_dotted_tainted_detectado() {
        // Bug real de auditoría: el `sink` hardcodeaba el literal
        // "redirect(...)" sin importar la forma que disparó — este test
        // confirma que la forma dotted (`flask.redirect`) se reporta con su
        // propio nombre, no con el de la forma bare-name.
        let src = "import flask\ndef go(request):\n    target = request.args[\"next\"]\n    return flask.redirect(target)\n";
        let f = findings(src);
        let r = f.iter().find(|x| x.cwe == "CWE-601").unwrap();
        assert_eq!(r.sink.as_deref(), Some("flask.redirect(...)"));
    }

    #[test]
    fn open_redirect_path_fijo_no_dispara() {
        let src = "from flask import redirect\ndef go():\n    return redirect(\"/dashboard\")\n";
        let f = findings(src);
        assert!(!f.iter().any(|x| x.cwe == "CWE-601"));
    }

    #[test]
    fn ssti_render_template_string_bare_name_tainted_detectado() {
        let src = "from flask import render_template_string\ndef greet(request):\n    name = request.args[\"name\"]\n    return render_template_string(\"Hello \" + name)\n";
        let f = findings(src);
        let r = f.iter().find(|x| x.cwe == "CWE-1336").unwrap();
        assert_eq!(r.severity, "High");
        assert_eq!(r.confidence, "High");
        assert_eq!(r.sink.as_deref(), Some("render_template_string(...)"));
    }

    #[test]
    fn ssti_flask_dotted_tainted_detectado() {
        let src = "import flask\ndef greet(request):\n    name = request.args[\"name\"]\n    return flask.render_template_string(name)\n";
        let f = findings(src);
        let r = f.iter().find(|x| x.cwe == "CWE-1336").unwrap();
        assert_eq!(r.sink.as_deref(), Some("flask.render_template_string(...)"));
    }

    #[test]
    fn ssti_template_fija_no_dispara() {
        let src = "from flask import render_template_string\ndef greet(name):\n    return render_template_string(\"Hello {{ name }}\", name=name)\n";
        let f = findings(src);
        assert!(!f.iter().any(|x| x.cwe == "CWE-1336"));
    }

    #[test]
    fn ssti_render_template_normal_no_comparte_el_sink() {
        // `render_template` (sin "_string") carga desde un archivo -- nunca
        // es este sink, sin importar qué tan tainted esté el 2do argumento
        // en adelante (esos son variables de contexto, la forma segura).
        let src = "from flask import render_template\ndef greet(request):\n    name = request.args[\"name\"]\n    return render_template(\"greet.html\", name=name)\n";
        let f = findings(src);
        assert!(!f.iter().any(|x| x.cwe == "CWE-1336"));
    }

    // ── Catálogo v5: CWE-327 (cripto rota) y CWE-295 (TLS sin verificar) ──
    // Ninguno de los 2 mira el taint map -- de ahí que todos estos tests
    // usen datos internos, sin ninguna fuente de taint a la vista.

    #[test]
    fn weak_crypto_hashlib_md5_detectado() {
        let src = "import hashlib\ndef sign(data):\n    return hashlib.md5(data).hexdigest()\n";
        let f = findings(src);
        let r = f.iter().find(|x| x.cwe == "CWE-327").unwrap();
        assert_eq!(r.sink.as_deref(), Some("hashlib.md5(...)"));
        assert_eq!(r.severity, "Medium");
        assert_eq!(r.confidence, "Medium");
    }

    #[test]
    fn weak_crypto_sha1_pelado_detectado() {
        // `from hashlib import sha1` -- el nombre llega pelado al call.
        let src = "from hashlib import sha1\ndef sign(data):\n    return sha1(data).hexdigest()\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-327"));
    }

    #[test]
    fn weak_crypto_usedforsecurity_false_no_dispara() {
        // El escape hatch documentado de Python 3.9+: declarar que el hash
        // no es para seguridad. Mismo criterio que Loader=SafeLoader.
        let src = "import hashlib\ndef cache_key(data):\n    return hashlib.md5(data, usedforsecurity=False).hexdigest()\n";
        let f = findings(src);
        assert!(!f.iter().any(|x| x.cwe == "CWE-327"));
    }

    #[test]
    fn weak_crypto_sha256_no_dispara() {
        let src = "import hashlib\ndef sign(data):\n    return hashlib.sha256(data).hexdigest()\n";
        let f = findings(src);
        assert!(!f.iter().any(|x| x.cwe == "CWE-327"));
    }

    #[test]
    fn weak_crypto_cipher_des_detectado() {
        let src = "from Crypto.Cipher import DES\ndef enc(key, data):\n    c = DES.new(key, DES.MODE_ECB)\n    return c.encrypt(data)\n";
        let f = findings(src);
        let r = f.iter().find(|x| x.cwe == "CWE-327").unwrap();
        assert_eq!(r.source, "broken symmetric cipher");
    }

    #[test]
    fn weak_crypto_cipher_ignora_usedforsecurity() {
        // La flag solo existe para los hashes de hashlib -- pasarla a un
        // cifrador no debe silenciar nada (ni es API real de pycryptodome).
        let src = "from Crypto.Cipher import ARC4\ndef enc(key, data):\n    return ARC4.new(key, usedforsecurity=False).encrypt(data)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-327"));
    }

    #[test]
    fn weak_crypto_metodo_ajeno_llamado_md5_no_dispara() {
        // `self.md5` / `obj.md5` no es hashlib -- solo el prefijo `hashlib.`
        // o el nombre pelado cuentan como el hash de la stdlib.
        let src = "def sign(hasher, data):\n    return hasher.md5(data)\n";
        let f = findings(src);
        assert!(!f.iter().any(|x| x.cwe == "CWE-327"));
    }

    #[test]
    fn tls_verify_false_detectado() {
        let src = "import requests\ndef fetch(url):\n    return requests.get(url, verify=False)\n";
        let f = findings(src);
        let r = f.iter().find(|x| x.cwe == "CWE-295").unwrap();
        assert_eq!(r.severity, "High");
        assert_eq!(r.confidence, "High");
        assert_eq!(r.sink.as_deref(), Some("requests.get(...)"));
    }

    #[test]
    fn tls_verify_true_o_ausente_no_dispara() {
        let src = "import requests\ndef fetch(url):\n    a = requests.get(url)\n    b = requests.get(url, verify=True)\n    return a, b\n";
        let f = findings(src);
        assert!(!f.iter().any(|x| x.cwe == "CWE-295"));
    }

    #[test]
    fn tls_verify_false_en_httpx_tambien_detectado() {
        let src = "import httpx\ndef fetch(url):\n    return httpx.post(url, verify=False)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-295"));
    }

    #[test]
    fn tls_verify_false_en_funcion_no_http_no_dispara() {
        // `verify` es un nombre de keyword genérico -- sin un callee de una
        // librería HTTP conocida, esto no es este CWE.
        let src = "def check(token):\n    return jwt_decode(token, verify=False)\n";
        let f = findings(src);
        assert!(!f.iter().any(|x| x.cwe == "CWE-295"));
    }

    #[test]
    fn tls_contexto_no_verificado_detectado() {
        let src = "import ssl\ndef ctx():\n    return ssl._create_unverified_context()\n";
        let f = findings(src);
        let r = f.iter().find(|x| x.cwe == "CWE-295").unwrap();
        assert_eq!(r.source, "unverified SSL context");
    }

    #[test]
    fn tls_contexto_default_no_dispara() {
        let src = "import ssl\ndef ctx():\n    return ssl.create_default_context()\n";
        let f = findings(src);
        assert!(!f.iter().any(|x| x.cwe == "CWE-295"));
    }

    #[test]
    fn cripto_rota_y_tls_conviven_como_hallazgos_independientes() {
        // Dos CWEs distintos en la misma función -- ninguno tapa al otro
        // (la cadena de `.or_else` corta por LLAMADA, no por función).
        let src = "import hashlib\nimport requests\ndef send(url, data):\n    h = hashlib.md5(data).hexdigest()\n    return requests.post(url, data=h, verify=False)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-327"));
        assert!(f.iter().any(|x| x.cwe == "CWE-295"));
    }

    #[test]
    fn cripto_rota_a_nivel_de_modulo_se_detecta() {
        // Los checks sin taint también tienen que funcionar fuera de una
        // función -- el mismo hueco que el fix de scope de módulo cerró
        // para el resto del catálogo (ver rich.rs).
        let src = "import hashlib\nDIGEST = hashlib.md5(b\"x\").hexdigest()\n";
        let suite = parse_module(src).expect("parse error");
        let line_index = LineIndex::build(src);
        let f = security_findings("<module>", &suite, &line_index);
        assert!(f.iter().any(|x| x.cwe == "CWE-327" && x.function.as_deref() == Some("<module>")));
    }
}
