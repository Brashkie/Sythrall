//! Fase 21 (Security & Taint Intelligence) — CWEs de C/C++.
//!
//! Hasta acá `security_findings` era un `[]` fijo para C/C++, así que un
//! proyecto de puro C sacaba **Security 100/100 por ausencia de análisis**.
//! Es la cuarta vez que aparece esta forma en el repo (JS/TS en Security,
//! JS/TS en Quality, C/C++ en Quality, y ahora C/C++ en Security) y la
//! última que quedaba: con esto, las 4 familias de lenguajes aportan a las 2
//! dimensiones que dependen de análisis por archivo.
//!
//! **Deliberadamente NO repite lo que `modernization.rs` ya reporta.** Ese
//! motor cubre `gets`/`strcpy`/`strcat` (CWE-120), el formato no literal
//! (CWE-134) y el `scanf` sin ancho (CWE-120) desde la Fase 25. Emitirlos
//! también acá pondría la misma línea bajo dos etiquetas distintas, que se
//! lee como dos problemas cuando es uno. Este módulo cubre 3 CWEs que ese
//! motor no toca.
//!
//! Sin taint tracking, y eso es una diferencia real con `security.rs`
//! (Python): allá se puede trazar un valor desde una fuente HTTP conocida
//! hasta el sink. Acá no hay AST con scope ni resolución de nombres, así que
//! lo que se reporta es la **forma** — el mismo criterio que
//! `jsts_security.rs` ya estableció para JS/TS, y el mismo que usa
//! `-Wformat-nonliteral` de gcc. Por eso ninguna confianza es `High` salvo
//! donde el hallazgo es literalmente visible en la llamada.

use tree_sitter::Node;

use crate::ast::tsnode;
use crate::hallazgos::security::SecurityFinding;

/// Funciones que ejecutan un comando del sistema. El índice es el argumento
/// que lleva el comando: `system(cmd)` y `popen(cmd, mode)` lo tienen
/// primero; la familia `exec*` lleva el path del programa primero, que es lo
/// que importa mirar.
const COMMAND_FUNCS: &[&str] =
    &["system", "popen", "execl", "execlp", "execle", "execv", "execvp", "execvpe", "_popen", "_wsystem"];

/// Funciones que reciben un modo de permisos en octal, con el índice del
/// argumento que lo lleva. `umask` queda afuera a propósito: su semántica
/// está INVERTIDA (los bits que pasa son los que quita), así que aplicarle
/// el mismo chequeo reportaría lo contrario de lo que pasa.
const PERMISSION_FUNCS: &[(&str, usize)] = &[
    ("chmod", 1),
    ("fchmod", 1),
    ("mkdir", 1),
    ("creat", 1),
    ("open", 2),
    ("openat", 3),
];

/// Valor de un literal octal de C (`0777`), o `None` si el nodo no lo es.
/// `0` solo no cuenta: es el cero decimal, no un modo de permisos.
fn octal_value(node: Node, source: &str) -> Option<u32> {
    if node.kind() != "number_literal" {
        return None;
    }
    let text = tsnode::text_of(node, source).trim();
    let digits = text.strip_prefix('0')?;
    if digits.is_empty() || !digits.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
        return None;
    }
    u32::from_str_radix(digits, 8).ok()
}

/// Nombre de la función que contiene este nodo, subiendo por el árbol.
/// `None` para código a nivel de archivo (un inicializador global).
fn enclosing_function(node: Node, source: &str) -> Option<String> {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "function_definition" {
            return Some(tsnode::function_definition_name(n, source));
        }
        current = n.parent();
    }
    None
}

fn nth_arg<'a>(call: Node<'a>, index: usize) -> Option<Node<'a>> {
    call.child_by_field_name("arguments")?.named_child(index)
}

pub fn security_findings_c(root: Node, source: &str) -> Vec<SecurityFinding> {
    let mut out = Vec::new();
    tsnode::walk_flat(root, &mut |node| {
        if node.kind() != "call_expression" {
            return;
        }
        let Some(func) = node.child_by_field_name("function") else { return };
        if func.kind() != "identifier" {
            return;
        }
        let fn_name = tsnode::text_of(func, source);
        let line = node.start_position().row + 1;
        let function = enclosing_function(node, source);

        // ── CWE-78: comando del sistema construido, no fijo ──
        if COMMAND_FUNCS.contains(&fn_name) {
            if let Some(arg) = nth_arg(node, 0) {
                if !tsnode::is_string_literal(arg) {
                    let arg_text = tsnode::text_of(arg, source);
                    out.push(SecurityFinding {
                        cwe: "CWE-78",
                        category: "OS Command Injection",
                        severity: "High",
                        confidence: "Medium",
                        source: format!("`{arg_text}` (no es un literal)"),
                        sink: Some(format!("{fn_name}(...)")),
                        line,
                        function: function.clone(),
                        recommendation: "Si el comando puede incorporar datos externos, quien controle ese valor controla lo que se ejecuta: `; rm -rf /` no necesita nada más. Evitá el shell — `execve` con un arreglo de argumentos separado nunca interpreta metacaracteres. Confianza Media: un argumento no literal puede ser una cadena interna fija; lo que está probado es la forma, no que haya entrada externa.",
                    });
                }
            }
        }

        // ── CWE-337: semilla predecible ──
        if fn_name == "srand" || fn_name == "srandom" {
            if let Some(arg) = nth_arg(node, 0) {
                let mut usa_time = false;
                tsnode::walk_flat(arg, &mut |n| {
                    if n.kind() == "call_expression" {
                        if let Some(f) = n.child_by_field_name("function") {
                            if matches!(tsnode::text_of(f, source), "time" | "clock" | "getpid") {
                                usa_time = true;
                            }
                        }
                    }
                });
                if usa_time {
                    out.push(SecurityFinding {
                        cwe: "CWE-337",
                        category: "Predictable Seed in PRNG",
                        severity: "Medium",
                        confidence: "High",
                        source: tsnode::text_of(arg, source).to_string(),
                        sink: Some(format!("{fn_name}(...)")),
                        line,
                        function: function.clone(),
                        recommendation: "La semilla es la hora (o el PID), así que la secuencia completa se reproduce sabiendo aproximadamente cuándo arrancó el proceso — `time(NULL)` tiene resolución de un segundo. Confianza Alta porque esto se ve en la llamada, no se estima. Para simulaciones o juegos está bien; para un token, una clave o un nonce hace falta una fuente del sistema (`getrandom`, `/dev/urandom`, `BCryptGenRandom`).",
                    });
                }
            }
        }

        // ── CWE-732: permisos abiertos al resto del mundo ──
        if let Some(&(_, mode_index)) = PERMISSION_FUNCS.iter().find(|(name, _)| *name == fn_name) {
            if let Some(arg) = nth_arg(node, mode_index) {
                if let Some(mode) = octal_value(arg, source) {
                    // 0o002 = escritura para "otros". Es el bit que convierte
                    // un archivo en modificable por cualquier usuario local.
                    if mode & 0o002 != 0 {
                        out.push(SecurityFinding {
                            cwe: "CWE-732",
                            category: "Incorrect Permission Assignment",
                            severity: "Medium",
                            confidence: "High",
                            source: format!("modo 0{mode:o}"),
                            sink: Some(format!("{fn_name}(...)")),
                            line,
                            function: function.clone(),
                            recommendation: "El modo tiene el bit de escritura para `otros`, así que cualquier usuario local puede modificar el archivo — y si el proceso después confía en su contenido, eso es escalación. Confianza Alta: el modo es un literal, se lee directo. `0644` para datos y `0600` para lo sensible cubren casi todos los casos.",
                        });
                    }
                }
            }
        }
    });
    out.sort_by_key(|f| f.line);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lenguajes::cparse;

    fn findings(src: &str) -> Vec<SecurityFinding> {
        cparse::parse_c(src).expect("parse error").security_findings
    }

    #[test]
    fn un_archivo_c_limpio_no_produce_nada() {
        let src = "#include <stdlib.h>\nint main() {\n    system(\"ls -la\");\n    srand(42);\n    chmod(\"/tmp/f\", 0644);\n    return 0;\n}\n";
        assert!(findings(src).is_empty(), "{:?}", findings(src).len());
    }

    #[test]
    fn system_con_argumento_no_literal_es_command_injection() {
        let src = "#include <stdlib.h>\nint main(int argc, char **argv) {\n    system(argv[1]);\n    return 0;\n}\n";
        let f = findings(src);
        let hit = f.iter().find(|x| x.cwe == "CWE-78").expect("sin hallazgo");
        assert_eq!(hit.severity, "High");
        assert_eq!(hit.confidence, "Medium");
        assert_eq!(hit.function.as_deref(), Some("main"));
    }

    #[test]
    fn system_con_literal_no_dispara() {
        // Un comando fijo escrito en el codigo no lo controla nadie de afuera.
        let src = "#include <stdlib.h>\nvoid f() {\n    system(\"date\");\n}\n";
        assert!(findings(src).is_empty());
    }

    #[test]
    fn popen_y_execlp_tambien_se_reconocen() {
        let src = "void f(char *cmd) {\n    popen(cmd, \"r\");\n    execlp(cmd, cmd, 0);\n}\n";
        assert_eq!(findings(src).iter().filter(|x| x.cwe == "CWE-78").count(), 2);
    }

    #[test]
    fn srand_con_time_es_semilla_predecible() {
        let src = "#include <stdlib.h>\n#include <time.h>\nint main() {\n    srand(time(NULL));\n    return rand();\n}\n";
        let f = findings(src);
        let hit = f.iter().find(|x| x.cwe == "CWE-337").expect("sin hallazgo");
        assert_eq!(hit.confidence, "High");
        assert!(hit.recommendation.contains("getrandom"));
    }

    #[test]
    fn srand_con_una_constante_no_dispara() {
        // Semilla fija: reproducible a proposito, no un descuido de entropia.
        let src = "#include <stdlib.h>\nint main() {\n    srand(12345);\n    return 0;\n}\n";
        assert!(findings(src).is_empty());
    }

    #[test]
    fn chmod_world_writable_se_detecta() {
        let src = "#include <sys/stat.h>\nvoid f() {\n    chmod(\"/tmp/data\", 0666);\n}\n";
        let f = findings(src);
        let hit = f.iter().find(|x| x.cwe == "CWE-732").expect("sin hallazgo");
        assert!(hit.source.contains("0666"), "{}", hit.source);
    }

    #[test]
    fn un_modo_sin_el_bit_de_otros_no_dispara() {
        // 0644 y 0660 no dan escritura al resto del mundo.
        let src = "#include <sys/stat.h>\nvoid f() {\n    chmod(\"/a\", 0644);\n    chmod(\"/b\", 0660);\n    mkdir(\"/c\", 0755);\n}\n";
        assert!(findings(src).is_empty());
    }

    #[test]
    fn open_lleva_el_modo_en_el_tercer_argumento() {
        // El indice del modo cambia por funcion: `open` lo tiene 3ro, no 2do.
        // Mirar siempre el mismo indice haria que ninguna de las dos ande.
        let src = "#include <fcntl.h>\nvoid f() {\n    open(\"/tmp/x\", O_CREAT, 0666);\n}\n";
        assert!(findings(src).iter().any(|x| x.cwe == "CWE-732"));
    }

    #[test]
    fn un_cero_decimal_no_se_toma_por_un_modo_octal() {
        let src = "#include <sys/stat.h>\nvoid f() {\n    chmod(\"/a\", 0);\n}\n";
        assert!(findings(src).is_empty());
    }

    #[test]
    fn no_repite_lo_que_modernization_ya_reporta() {
        // `strcpy`/`gets`/`printf(fmt)` son de `modernization.rs` desde la
        // Fase 25. Emitirlos tambien aca pondria la misma linea bajo dos
        // etiquetas, que se lee como dos problemas cuando es uno.
        let src = "#include <string.h>\n#include <stdio.h>\nvoid f(char *s, char *fmt) {\n    char buf[10];\n    strcpy(buf, s);\n    gets(buf);\n    printf(fmt);\n}\n";
        let r = cparse::parse_c(src).unwrap();
        assert!(r.security_findings.is_empty(), "{:?}", r.security_findings.len());
        // ...pero el otro motor SI los ve, o sea que no se perdieron.
        assert!(r.modernization.candidates.len() >= 3, "{:?}", r.modernization.candidates.len());
    }

    #[test]
    fn los_hallazgos_salen_ordenados_por_linea() {
        let src = "#include <stdlib.h>\nvoid f(char *c) {\n    chmod(\"/a\", 0666);\n    system(c);\n}\n";
        let f = findings(src);
        assert_eq!(f.len(), 2);
        assert!(f[0].line <= f[1].line);
    }

    #[test]
    fn tambien_anda_en_cpp() {
        let src = "#include <cstdlib>\nint main(int argc, char **argv) {\n    std::string cmd = argv[1];\n    system(cmd.c_str());\n    return 0;\n}\n";
        let r = cparse::parse_cpp(src).unwrap();
        assert!(r.security_findings.iter().any(|x| x.cwe == "CWE-78"));
    }
}
