//! Security para JS/TS (Fase 21, 2026-09-02) — cierra un hueco que era
//! **activamente engañoso**, no solo una falta de cobertura: hasta acá
//! `static_parser.py::_parse_js_ts_via_rust` devolvía `security_findings:
//! []` fijo, así que el Project Health de CUALQUIER proyecto JS/TS mostraba
//! Security = 100/100 sin importar qué tuviera adentro. Un score perfecto
//! por ausencia de análisis se lee igual que un score perfecto por código
//! limpio, y eso es peor que no mostrar nada.
//!
//! **Qué NO es esto, dicho al frente**: no hay taint tracking para JS/TS.
//! `security.rs` rastrea flujo de datos sobre el AST real de
//! `rustpython_parser`; acá el parser de JS/TS es regex sobre el texto
//! (`jsts.rs` lo documenta desde su primera línea — no existe un
//! equivalente Rust-nativo tan directo para JS/TS). Sin AST no hay
//! propagación de taint posible, así que estos checks reportan la PRESENCIA
//! de un sink peligroso, no que un dato externo llegue hasta él.
//!
//! Por eso 3 de los 4 checks son `confidence: "Medium"` — exactamente el
//! mismo nivel que `security.rs` ya usa para `check_insecure_deserialization`
//! cuando el argumento no traza a una fuente conocida ("usar esta función ya
//! es un riesgo real; que además llegue input externo es peor"). El cuarto
//! (TLS desactivado) es `High` por el mismo motivo que su gemelo de Python:
//! `rejectUnauthorized: false` no tiene lectura benigna, apagar la
//! validación es exacta y únicamente lo que hace.
//!
//! Límites conocidos, no escondidos: se saltean las líneas que empiezan con
//! `//`/`*`/`/*` (comentarios de una línea y cuerpos de bloque), pero un
//! sink escrito DENTRO de un string literal, o en la misma línea después de
//! código real seguido de `/*`, sí puede producir un falso positivo. Es la
//! misma clase de imprecisión que el parser de JS/TS de al lado ya acepta y
//! documenta.

use std::sync::LazyLock;

use regex::Regex;

use crate::lenguajes::jsts::JsFunction;
use crate::hallazgos::security::SecurityFinding;

/// `eval(...)` como llamada suelta (no `obj.eval(...)`, que casi siempre es
/// otra cosa — el `eval` de un motor de plantillas, de un ORM, etc.) y
/// `new Function(...)`, que compila un string a función igual que `eval`.
static JS_EVAL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(^|[^.\w])(eval\s*\(|new\s+Function\s*\()").unwrap());

/// Sinks de XSS en el DOM: los 4 que escriben HTML crudo. `textContent`
/// queda deliberadamente afuera — es justamente la alternativa segura.
static JS_XSS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\.(innerHTML|outerHTML)\s*=|document\s*\.\s*write(ln)?\s*\(|\.insertAdjacentHTML\s*\(|dangerouslySetInnerHTML")
        .unwrap()
});

/// `exec`/`execSync` de `child_process` — las 2 variantes que pasan el
/// comando por una shell (a diferencia de `execFile`/`spawn` con un array de
/// argumentos, la forma segura, que no se marca).
static JS_EXEC_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|[^.\w])exec(Sync)?\s*\(").unwrap());

/// Validación de certificado TLS apagada — la opción de Node (`https`/
/// `axios`/`request`) y la variable de entorno que la apaga globalmente.
/// La comilla y el corchete opcionales cubren las 2 formas de escribir la
/// variable de entorno (`process.env.NAME = '0'` y `process.env['NAME'] = 0`)
/// — la segunda se le escapaba a la primera versión de este regex, atrapado
/// por su propio test antes de shippear.
static JS_TLS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"rejectUnauthorized\s*:\s*false|NODE_TLS_REJECT_UNAUTHORIZED['"]?\s*\]?\s*=\s*['"]?0"#).unwrap()
});

/// `true` si la línea es (o arranca como) un comentario. Barato y suficiente
/// para el caso común de un sink citado en un comentario explicativo; no
/// intenta rastrear bloques `/* ... */` multilínea de verdad — ver el doc
/// del módulo.
fn is_comment_line(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("//") || t.starts_with('*') || t.starts_with("/*")
}

/// Nombre de la función que contiene `line`, si alguna la contiene — las
/// funciones ya vienen con su rango (`line`..`end_line`) calculado por
/// `jsts.rs`, así que esto es una búsqueda, no análisis nuevo. La más
/// ANIDADA gana (el rango más chico que contiene la línea), que es la
/// atribución útil cuando hay funciones dentro de funciones.
fn enclosing_function(functions: &[JsFunction], line: usize) -> Option<String> {
    functions
        .iter()
        .filter(|f| f.line <= line && line <= f.end_line)
        .min_by_key(|f| f.end_line.saturating_sub(f.line))
        .map(|f| f.name.clone())
}

struct Check {
    re: &'static LazyLock<Regex>,
    cwe: &'static str,
    category: &'static str,
    severity: &'static str,
    confidence: &'static str,
    source: &'static str,
    sink: &'static str,
    recommendation: &'static str,
    /// Substring que el ARCHIVO entero tiene que contener para que este
    /// check corra — evita el falso positivo más grande de `exec(`, que es
    /// un nombre de función perfectamente común fuera de `child_process`.
    requires_in_file: Option<&'static str>,
}

const CHECKS: &[Check] = &[
    Check {
        re: &JS_EVAL_RE,
        cwe: "CWE-95",
        category: "Dynamic Code Evaluation",
        severity: "High",
        confidence: "Medium",
        source: "dynamic code execution sink",
        sink: "eval(...) / new Function(...)",
        recommendation: "Don't compile strings into code — use JSON.parse for data, or a lookup table/dispatch object instead of evaluating source at runtime.",
        requires_in_file: None,
    },
    Check {
        re: &JS_XSS_RE,
        cwe: "CWE-79",
        category: "Cross-Site Scripting (DOM sink)",
        severity: "High",
        confidence: "Medium",
        source: "raw HTML write",
        sink: "innerHTML / outerHTML / document.write / insertAdjacentHTML",
        recommendation: "Assign through `textContent` (which never parses HTML), or sanitize with a maintained library before writing markup built from any value you don't fully control.",
        requires_in_file: None,
    },
    Check {
        re: &JS_EXEC_RE,
        cwe: "CWE-78",
        category: "OS Command Injection",
        severity: "High",
        confidence: "Medium",
        source: "shell command execution",
        sink: "child_process.exec / execSync",
        recommendation: "Use `execFile`/`spawn` with an argument array — those never hand the string to a shell, so quoting and metacharacters stop being your problem.",
        requires_in_file: Some("child_process"),
    },
    Check {
        re: &JS_TLS_RE,
        cwe: "CWE-295",
        category: "Improper Certificate Validation",
        severity: "High",
        confidence: "High",
        source: "TLS verification disabled",
        sink: "rejectUnauthorized: false / NODE_TLS_REJECT_UNAUTHORIZED=0",
        recommendation: "Point the client at the right CA bundle instead of accepting any certificate — with verification off the connection is still encrypted, but against anyone in the middle, which is exactly what the certificate exists to prevent.",
        requires_in_file: None,
    },
];

/// Un hallazgo por LÍNEA y por check — dos sinks distintos en la misma línea
/// se reportan los dos, pero la misma línea no se reporta dos veces por el
/// mismo check aunque el patrón matchee más de una vez ahí.
pub fn js_security_findings(content: &str, functions: &[JsFunction]) -> Vec<SecurityFinding> {
    let mut findings = Vec::new();
    for check in CHECKS {
        if check.requires_in_file.is_some_and(|needle| !content.contains(needle)) {
            continue;
        }
        for (i, line) in content.lines().enumerate() {
            if is_comment_line(line) || !check.re.is_match(line) {
                continue;
            }
            let line_no = i + 1;
            findings.push(SecurityFinding {
                cwe: check.cwe,
                category: check.category,
                severity: check.severity,
                confidence: check.confidence,
                source: check.source.to_string(),
                sink: Some(check.sink.to_string()),
                line: line_no,
                function: enclosing_function(functions, line_no),
                recommendation: check.recommendation,
            });
        }
    }
    findings.sort_by_key(|f| f.line);
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn findings(src: &str) -> Vec<SecurityFinding> {
        js_security_findings(src, &[])
    }

    #[test]
    fn eval_suelto_se_detecta() {
        let f = findings("const r = eval(userInput);\n");
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].cwe, "CWE-95");
        assert_eq!(f[0].confidence, "Medium");
    }

    #[test]
    fn new_function_tambien_se_detecta() {
        assert!(findings("const f = new Function('a', body);\n").iter().any(|x| x.cwe == "CWE-95"));
    }

    #[test]
    fn eval_como_metodo_de_otro_objeto_no_dispara() {
        // `template.eval(...)` casi nunca es el eval global.
        assert!(findings("const r = template.eval(ctx);\n").is_empty());
    }

    #[test]
    fn inner_html_se_detecta_y_text_content_no() {
        assert!(findings("el.innerHTML = data;\n").iter().any(|x| x.cwe == "CWE-79"));
        assert!(findings("el.textContent = data;\n").is_empty());
    }

    #[test]
    fn document_write_e_insert_adjacent_html_se_detectan() {
        assert!(findings("document.write(x);\n").iter().any(|x| x.cwe == "CWE-79"));
        assert!(findings("el.insertAdjacentHTML('beforeend', x);\n").iter().any(|x| x.cwe == "CWE-79"));
    }

    #[test]
    fn exec_sin_child_process_en_el_archivo_no_dispara() {
        // `exec` es un nombre común (regex.exec, un helper propio) -- sin el
        // require/import de child_process en el archivo, no es este CWE.
        assert!(findings("const m = exec(pattern);\n").is_empty());
    }

    #[test]
    fn exec_con_child_process_en_el_archivo_si_dispara() {
        let src = "const { exec } = require('child_process');\nexec(cmd);\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.cwe == "CWE-78"));
    }

    #[test]
    fn spawn_y_exec_file_no_disparan() {
        let src = "const { spawn, execFile } = require('child_process');\nspawn('ls', args);\nexecFile('ls', args);\n";
        // `execFile(` no matchea `exec(` porque el regex exige el paréntesis
        // justo después de `exec`/`execSync`.
        assert!(!findings(src).iter().any(|x| x.cwe == "CWE-78"));
    }

    #[test]
    fn tls_desactivado_es_confianza_alta() {
        let f = findings("const agent = new https.Agent({ rejectUnauthorized: false });\n");
        let hit = f.iter().find(|x| x.cwe == "CWE-295").unwrap();
        assert_eq!(hit.confidence, "High");
    }

    #[test]
    fn variable_de_entorno_que_apaga_tls_tambien_se_detecta() {
        assert!(findings("process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';\n").iter().any(|x| x.cwe == "CWE-295"));
        assert!(findings("process.env['NODE_TLS_REJECT_UNAUTHORIZED'] = 0;\n").iter().any(|x| x.cwe == "CWE-295"));
    }

    #[test]
    fn tls_activado_no_dispara() {
        assert!(findings("const agent = new https.Agent({ rejectUnauthorized: true });\n").is_empty());
    }

    #[test]
    fn sink_en_un_comentario_no_dispara() {
        let src = "// nunca uses eval(x) acá\n * el.innerHTML = y\n/* document.write(z) */\n";
        assert!(findings(src).is_empty());
    }

    #[test]
    fn dos_sinks_distintos_en_la_misma_linea_se_reportan_los_dos() {
        let f = findings("el.innerHTML = eval(x);\n");
        assert_eq!(f.len(), 2);
        assert!(f.iter().any(|x| x.cwe == "CWE-79"));
        assert!(f.iter().any(|x| x.cwe == "CWE-95"));
    }

    #[test]
    fn hallazgos_ordenados_por_linea() {
        let src = "const a = 1;\nel.innerHTML = x;\nconst b = 2;\nconst r = eval(y);\n";
        let f = findings(src);
        assert_eq!(f.iter().map(|x| x.line).collect::<Vec<_>>(), vec![2, 4]);
    }

    #[test]
    fn se_atribuye_a_la_funcion_que_contiene_la_linea() {
        let funcs = vec![
            JsFunction {
                name: "outer".to_string(),
                line: 1,
                end_line: 10,
                loc: 10,
                complexity: 1,
                big_o: "O(1)".to_string(),
                big_o_reason: String::new(),
                calls: vec![],
                is_async: false,
                params: vec![],
            },
            JsFunction {
                name: "inner".to_string(),
                line: 3,
                end_line: 5,
                loc: 3,
                complexity: 1,
                big_o: "O(1)".to_string(),
                big_o_reason: String::new(),
                calls: vec![],
                is_async: false,
                params: vec![],
            },
        ];
        let src = "a\nb\nc\nconst r = eval(x);\ne\nf\ng\nh\ni\nj\n";
        let f = js_security_findings(src, &funcs);
        // La línea 4 cae dentro de las dos -- gana la más anidada.
        assert_eq!(f[0].function.as_deref(), Some("inner"));
    }

    #[test]
    fn linea_fuera_de_toda_funcion_no_tiene_atribucion() {
        let funcs = vec![JsFunction {
            name: "f".to_string(),
            line: 10,
            end_line: 20,
            loc: 11,
            complexity: 1,
            big_o: "O(1)".to_string(),
            big_o_reason: String::new(),
            calls: vec![],
            is_async: false,
            params: vec![],
        }];
        let f = js_security_findings("const r = eval(x);\n", &funcs);
        assert!(f[0].function.is_none());
    }

    #[test]
    fn archivo_limpio_no_produce_nada() {
        let src = "export function suma(a, b) {\n  return a + b;\n}\n";
        assert!(findings(src).is_empty());
    }
}
