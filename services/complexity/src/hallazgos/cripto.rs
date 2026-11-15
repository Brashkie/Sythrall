//! Fase 32 (Sythrall Mathematical Analysis) — teoría de números aplicada:
//! aleatoriedad, comparación de secretos y tamaño de clave.
//!
//! **No repite a `security.rs`, lo completa.** Aquel ya cubre CWE-327: qué
//! primitiva se eligió (`md5`, `sha1`, `DES`). Éste cubre las tres cosas que
//! están bien elegidas y mal usadas: de dónde salieron los bits, cómo se
//! comparan, y cuántos son.
//!
//! **El problema de fondo, y por qué el análisis se ve distinto en cada
//! caso.** Un PRNG como el Mersenne Twister de `random` no es débil por ser
//! poco uniforme —lo es muchísimo— sino porque es **predecible**: con 624
//! salidas consecutivas se reconstruye el estado interno y se calcula todo lo
//! que viene después. Para una simulación eso es irrelevante; para un token
//! de sesión es la vulnerabilidad entera. La diferencia no está en el código,
//! **está en el propósito**, y el propósito no vive en el AST.
//!
//! Por eso los tres hallazgos tienen confianzas distintas, y no por
//! prudencia genérica:
//!   - **El tamaño de clave es `High`**: `key_size=1024` es un número
//!     literal contra un mínimo publicado. Se lee, no se estima.
//!   - **La comparación de un digest es `High` cuando la forma la prueba**
//!     (`hmac.new(...).hexdigest() == firma`): ahí no hace falta adivinar
//!     para qué sirve, la llamada lo dice.
//!   - **Todo lo que depende del NOMBRE de una variable es `Medium`**, y el
//!     mensaje lo dice: `token = random.choice(...)` es casi seguro un bug y
//!     `x = random.choice(...)` no se puede juzgar. El analizador va por el
//!     nombre porque es la única evidencia que hay, no porque lo pruebe.
//!
//! **Dos exclusiones que un detector ingenuo se comería.**
//!   - **`random.SystemRandom` NO es inseguro**: usa `os.urandom` por
//!     debajo. Meterlo en la bolsa de `random.*` sería exactamente el error
//!     que este módulo existe para no cometer.
//!   - **La palabra `key` sola no entra en la lista de nombres sensibles.**
//!     `cache_key`, `sort_key` y `dict_key` son omnipresentes y no tienen
//!     nada que ver con criptografía. Entran `api_key`, `secret_key` y
//!     `private_key`, que sí son inequívocas.

use rustpython_parser::ast::{CmpOp, Constant, Expr, Ranged, Stmt};

use crate::ast::parser::LineIndex;
use crate::hallazgos::security::SecurityFinding;
use crate::ast::structure::node_name;
use crate::ast::walk::{walk_expr_tree, walk_stmts_own_scope};

/// Funciones de `random` que producen bits. `SystemRandom` queda afuera a
/// propósito: es la envoltura segura sobre `os.urandom`.
const FUNCIONES_RANDOM: &[&str] = &[
    "random", "randint", "randrange", "choice", "choices", "sample", "uniform", "getrandbits",
    "shuffle", "randbytes",
];

/// Nombres que hacen inequívoco el propósito criptográfico. `key` pelada
/// queda afuera: ver el doc del módulo.
const NOMBRES_DE_SECRETO: &[&str] = &[
    "token",
    "password",
    "passwd",
    "secret",
    "salt",
    "nonce",
    "apikey",
    "api_key",
    "sessionid",
    "session_id",
    "csrf",
    "otp",
    "credential",
    "privatekey",
    "private_key",
    "secretkey",
    "secret_key",
];

/// Para la comparación, además de los secretos entran los nombres de lo que
/// se compara CONTRA un secreto — ahí `digest`/`hash`/`mac` sí son señal.
const NOMBRES_COMPARABLES: &[&str] =
    &["signature", "hmac", "digest", "checksum", "hash", "mac", "tag"];

/// Mínimos vigentes (NIST SP 800-57 / BSI): 2048 bits para RSA y DSA.
const MINIMO_RSA_DSA: i64 = 2048;
/// 16 bytes = 128 bits, el piso habitual para un token o una sal.
const MINIMO_BYTES_DE_TOKEN: i64 = 16;

fn constante_entera(e: &Expr) -> Option<i64> {
    match e {
        Expr::Constant(c) => match &c.value {
            Constant::Int(i) => i.to_string().parse().ok(),
            _ => None,
        },
        _ => None,
    }
}

/// `true` si el nombre delata un propósito criptográfico. Se normaliza a
/// minúsculas y se busca por subcadena, así que `nuevo_token_de_sesion` y
/// `TOKEN` entran igual.
fn nombre_sensible(nombre: &str, lista: &[&str]) -> bool {
    let n = nombre.to_lowercase();
    lista.iter().any(|s| n.contains(s))
}

fn argumento<'a>(c: &'a rustpython_parser::ast::ExprCall, pos: usize, clave: &str) -> Option<&'a Expr> {
    c.args.get(pos).or_else(|| {
        c.keywords
            .iter()
            .find(|k| k.arg.as_ref().is_some_and(|a| a.as_str() == clave))
            .map(|k| &k.value)
    })
}

/// `true` si la llamada saca bits del PRNG predecible de `random`.
fn es_random_predecible(nombre_llamada: &str) -> bool {
    // `random.SystemRandom().choice(...)` lleva `SystemRandom` en el medio:
    // es seguro y no debe entrar por el prefijo `random.`.
    if nombre_llamada.contains("SystemRandom") {
        return false;
    }
    if let Some(resto) = nombre_llamada.strip_prefix("random.") {
        return FUNCIONES_RANDOM.contains(&resto);
    }
    // `from random import choice` deja el nombre pelado.
    FUNCIONES_RANDOM.contains(&nombre_llamada)
}

/// Nombres a los que se les asigna algo en esta sentencia, más el nombre de
/// la función que la contiene: las dos formas en que el propósito se declara.
fn nombres_del_contexto(stmt: &Stmt, scope: &str) -> Vec<String> {
    let mut v = vec![scope.to_string()];
    let mut destinos = |e: &Expr| {
        if let Expr::Name(n) = e {
            v.push(n.id.to_string());
        }
        if let Expr::Attribute(a) = e {
            v.push(a.attr.to_string());
        }
    };
    match stmt {
        Stmt::Assign(a) => a.targets.iter().for_each(&mut destinos),
        Stmt::AnnAssign(a) => destinos(&a.target),
        Stmt::AugAssign(a) => destinos(&a.target),
        _ => {}
    }
    v
}

/// CWE-338: bits predecibles usados para algo que necesita ser impredecible.
fn revisar_aleatoriedad(
    scope: &str,
    body: &[Stmt],
    line_index: &LineIndex,
    out: &mut Vec<SecurityFinding>,
) {
    let mut on_stmt = |stmt: &Stmt| {
        let contexto = nombres_del_contexto(stmt, scope);
        if !contexto.iter().any(|n| nombre_sensible(n, NOMBRES_DE_SECRETO)) {
            return;
        }
        let mut encontrados: Vec<(usize, String)> = Vec::new();
        let mut mirar = |e: &Expr| {
            let Expr::Call(c) = e else { return };
            let nombre = node_name(&c.func);
            if es_random_predecible(&nombre) {
                encontrados.push((line_index.line_of(e.range().start().to_usize()), nombre));
            }
        };
        // Solo las expresiones propias de ESTA sentencia: el nombre del
        // destino es lo que le da sentido al hallazgo.
        for e in expresiones_de(stmt) {
            walk_expr_tree(e, &mut mirar);
        }
        for (line, nombre) in encontrados {
            out.push(SecurityFinding {
                cwe: "CWE-338",
                category: "Insecure Randomness",
                severity: "High",
                confidence: "Medium",
                source: format!("{nombre}(...)"),
                sink: None,
                line,
                function: Some(scope.to_string()),
                recommendation:
                    "Use `secrets` (`secrets.token_urlsafe`, `secrets.token_bytes`, `secrets.choice`) or `random.SystemRandom`. The Mersenne Twister behind `random` is not weak because it is non-uniform — it is weak because it is PREDICTABLE: 624 consecutive outputs reveal the internal state and every value that follows. For a simulation that does not matter; for anything an attacker should not be able to guess, it is the whole vulnerability. Confidence is Medium because the analysis reads the PURPOSE from the variable/function name, which is evidence, not proof.",
            });
        }
    };
    let mut sin_expr = |_: &Expr| {};
    walk_stmts_own_scope(body, &mut on_stmt, &mut sin_expr);
}

fn expresiones_de(stmt: &Stmt) -> Vec<&Expr> {
    match stmt {
        Stmt::Assign(a) => vec![&a.value],
        Stmt::AugAssign(a) => vec![&a.value],
        Stmt::AnnAssign(a) => a.value.iter().map(|v| &**v).collect(),
        Stmt::Return(r) => r.value.iter().map(|v| &**v).collect(),
        Stmt::Expr(e) => vec![&e.value],
        _ => vec![],
    }
}

/// `true` si la expresión ES un digest por su forma, sin depender del nombre.
fn es_digest_por_forma(e: &Expr) -> bool {
    let Expr::Call(c) = e else { return false };
    let nombre = node_name(&c.func);
    nombre.ends_with(".hexdigest") || nombre.ends_with(".digest") || nombre.starts_with("hmac.")
}

/// Cómo se escribe el operando en el hallazgo.
///
/// No se usa `structure::node_name` porque devuelve `"?"` para una llamada, y
/// justo el caso interesante —`hmac.new(k, d).hexdigest() == firma`— es una
/// llamada sobre una llamada: el mensaje quedaba como `? == firma`.
fn nombre_legible(e: &Expr) -> String {
    match e {
        Expr::Name(n) => n.id.to_string(),
        Expr::Attribute(a) => format!("{}.{}", nombre_legible(&a.value), a.attr),
        Expr::Call(c) => format!("{}(...)", nombre_legible(&c.func)),
        Expr::Constant(_) => "a literal".to_string(),
        _ => "the value".to_string(),
    }
}

/// El porqué del hallazgo es el mismo en los dos casos; lo que cambia es de
/// dónde salió la certeza. Van escritos enteros porque `recommendation` es
/// `&'static str` y no vale agregar una dependencia solo para concatenar.
const RECOMENDACION_TIMING_PROBADO: &str = "Use `hmac.compare_digest(a, b)`. Python's `==` on bytes/str short-circuits at the first differing byte, so how long the comparison takes leaks how many leading bytes were right — an attacker who can time the endpoint recovers the secret one byte at a time instead of brute-forcing it whole. `compare_digest` always reads both operands fully. The call itself says what this is — a digest — so nothing about the purpose had to be guessed, which is why this one is High confidence.";

const RECOMENDACION_TIMING_POR_NOMBRE: &str = "Use `hmac.compare_digest(a, b)`. Python's `==` on bytes/str short-circuits at the first differing byte, so how long the comparison takes leaks how many leading bytes were right — an attacker who can time the endpoint recovers the secret one byte at a time instead of brute-forcing it whole. `compare_digest` always reads both operands fully. Confidence is Medium because the purpose is read from the operand name, which is evidence, not proof.";

/// CWE-208: comparar un secreto con `==` filtra su contenido por el tiempo.
fn revisar_comparaciones(
    scope: &str,
    body: &[Stmt],
    line_index: &LineIndex,
    out: &mut Vec<SecurityFinding>,
) {
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |e: &Expr| {
        let Expr::Compare(c) = e else { return };
        if c.ops.len() != 1 || !matches!(c.ops[0], CmpOp::Eq | CmpOp::NotEq) {
            return;
        }
        let lados = [&*c.left, &c.comparators[0]];
        // La forma prueba el propósito; el nombre solo lo sugiere.
        let por_forma = lados.iter().any(|l| es_digest_por_forma(l));
        let por_nombre = lados.iter().any(|l| {
            matches!(l, Expr::Name(_) | Expr::Attribute(_))
                && nombre_sensible(&nombre_legible(l), NOMBRES_DE_SECRETO)
        }) || lados.iter().any(|l| {
            matches!(l, Expr::Name(_) | Expr::Attribute(_))
                && nombre_sensible(&nombre_legible(l), NOMBRES_COMPARABLES)
        });
        if !por_forma && !por_nombre {
            return;
        }
        // Comparar contra un literal no filtra un secreto del servidor: el
        // valor ya está en el código.
        if lados.iter().any(|l| matches!(l, Expr::Constant(_))) {
            return;
        }
        // `recommendation` es `&'static str`, así que el matiz sobre la
        // confianza se elige entre dos textos en vez de interpolarse.
        let (confidence, recommendation) = if por_forma {
            ("High", RECOMENDACION_TIMING_PROBADO)
        } else {
            ("Medium", RECOMENDACION_TIMING_POR_NOMBRE)
        };
        out.push(SecurityFinding {
            cwe: "CWE-208",
            category: "Observable Timing Discrepancy",
            severity: "Medium",
            confidence,
            source: format!("{} == {}", nombre_legible(lados[0]), nombre_legible(lados[1])),
            sink: None,
            line: line_index.line_of(e.range().start().to_usize()),
            function: Some(scope.to_string()),
            recommendation,
        });
    };
    walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
}

/// CWE-326: una clave o un token por debajo del mínimo vigente.
fn revisar_tamanos(scope: &str, body: &[Stmt], line_index: &LineIndex, out: &mut Vec<SecurityFinding>) {
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |e: &Expr| {
        let Expr::Call(c) = e else { return };
        let nombre = node_name(&c.func);
        let linea = line_index.line_of(e.range().start().to_usize());

        // `cryptography`: rsa/dsa.generate_private_key(key_size=...).
        let clave_asimetrica = nombre.ends_with("generate_private_key")
            || matches!(nombre.as_str(), "RSA.generate" | "DSA.generate" | "Crypto.PublicKey.RSA.generate");
        if clave_asimetrica {
            // `key_size` es keyword en `cryptography` y el 1er posicional en
            // pycryptodome; `generate_private_key` de RSA lo lleva 2do.
            let bits = argumento(c, 0, "key_size")
                .and_then(constante_entera)
                .or_else(|| argumento(c, 1, "key_size").and_then(constante_entera));
            if let Some(n) = bits {
                if n < MINIMO_RSA_DSA {
                    out.push(SecurityFinding {
                        cwe: "CWE-326",
                        category: "Inadequate Key Size",
                        severity: "High",
                        confidence: "High",
                        source: format!("{nombre}(... {n} bits ...)"),
                        sink: None,
                        line: linea,
                        function: Some(scope.to_string()),
                        recommendation:
                            "Use at least 2048 bits for RSA/DSA (NIST SP 800-57 withdrew 1024 in 2013), or move to an elliptic curve — P-256 gives comparable strength with far smaller keys. This one is High confidence in both dimensions: the size is a literal in the call, checked against a published minimum, so it is read rather than estimated.",
                    });
                    return;
                }
            }
        }

        // Un token o una sal más corta que 128 bits.
        let generador_de_bytes = matches!(
            nombre.as_str(),
            "os.urandom" | "urandom" | "secrets.token_bytes" | "token_bytes" | "secrets.token_hex" | "token_hex" | "secrets.token_urlsafe" | "token_urlsafe"
        );
        if generador_de_bytes {
            if let Some(n) = c.args.first().and_then(constante_entera) {
                if n < MINIMO_BYTES_DE_TOKEN {
                    out.push(SecurityFinding {
                        cwe: "CWE-326",
                        category: "Inadequate Key Size",
                        severity: "Medium",
                        confidence: "High",
                        source: format!("{nombre}({n})"),
                        sink: None,
                        line: linea,
                        function: Some(scope.to_string()),
                        recommendation:
                            "Ask for at least 16 bytes (128 bits). The source of entropy here is right — this is only about how much of it is taken. Below 128 bits a token stops being infeasible to guess and becomes a matter of how many attempts the attacker is allowed.",
                    });
                }
            }
        }
    };
    walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
}

/// Punto de entrada por función, misma convención que
/// `security::security_findings`.
pub fn crypto_findings(scope: &str, body: &[Stmt], line_index: &LineIndex) -> Vec<SecurityFinding> {
    let mut out = Vec::new();
    revisar_aleatoriedad(scope, body, line_index, &mut out);
    revisar_comparaciones(scope, body, line_index, &mut out);
    revisar_tamanos(scope, body, line_index, &mut out);
    out.sort_by_key(|f| f.line);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::parse_module;

    fn hallazgos(src: &str) -> Vec<SecurityFinding> {
        let suite = parse_module(src).expect("parse error");
        let idx = LineIndex::build(src);
        let (scope, body) = match &suite[0] {
            Stmt::FunctionDef(f) => (f.name.to_string(), f.body.clone()),
            _ => ("<module>".to_string(), suite.clone()),
        };
        crypto_findings(&scope, &body, &idx)
    }

    fn tiene(src: &str, cwe: &str) -> bool {
        hallazgos(src).iter().any(|f| f.cwe == cwe)
    }

    // ── CWE-338 ──

    #[test]
    fn un_token_generado_con_random_se_marca() {
        assert!(tiene("def f():\n    token = random.choice(letras)\n", "CWE-338"));
        assert!(tiene("def f():\n    password = ''.join(random.choices(abc, k=12))\n", "CWE-338"));
    }

    #[test]
    fn el_nombre_de_la_funcion_tambien_declara_el_proposito() {
        assert!(tiene("def generar_token():\n    return random.getrandbits(64)\n", "CWE-338"));
    }

    #[test]
    fn un_random_sin_proposito_criptografico_no_se_toca() {
        // Sin nombre que declare el proposito no hay evidencia de nada.
        assert!(hallazgos("def f():\n    x = random.randint(1, 6)\n").is_empty());
        assert!(hallazgos("def tirar_dado():\n    return random.randint(1, 6)\n").is_empty());
    }

    #[test]
    fn system_random_no_es_inseguro() {
        // Usa os.urandom por debajo: meterlo en la bolsa de `random.*` seria
        // justo el error que este modulo existe para no cometer.
        assert!(hallazgos("def f():\n    token = random.SystemRandom().choice(abc)\n").is_empty());
    }

    #[test]
    fn secrets_es_la_forma_correcta_y_no_se_reporta() {
        assert!(hallazgos("def f():\n    token = secrets.token_urlsafe(32)\n").is_empty());
    }

    #[test]
    fn la_palabra_key_sola_no_alcanza() {
        // `cache_key`, `sort_key` y `dict_key` son omnipresentes.
        assert!(hallazgos("def f():\n    cache_key = random.random()\n").is_empty());
        // Pero `api_key` si es inequivoca.
        assert!(tiene("def f():\n    api_key = random.random()\n", "CWE-338"));
    }

    // ── CWE-208 ──

    #[test]
    fn comparar_un_digest_con_igual_se_prueba_por_la_forma() {
        let h = hallazgos("def f(esperado, data):\n    return hmac.new(k, data).hexdigest() == esperado\n");
        let f = h.iter().find(|f| f.cwe == "CWE-208").expect("esperaba CWE-208");
        assert_eq!(f.confidence, "High", "la forma lo prueba, no hace falta el nombre");
        // El operando se escribe entero: `node_name` daba "?" para una
        // llamada, asi que el hallazgo salia como `? == esperado`.
        assert_eq!(f.source, "hmac.new(...).hexdigest(...) == esperado", "{}", f.source);
    }

    #[test]
    fn comparar_por_nombre_baja_la_confianza() {
        let h = hallazgos("def f(a, b):\n    return a.signature == b\n");
        let f = h.iter().find(|f| f.cwe == "CWE-208").expect("esperaba CWE-208");
        assert_eq!(f.confidence, "Medium");
    }

    #[test]
    fn comparar_contra_un_literal_no_filtra_un_secreto() {
        // El valor ya esta en el codigo: el timing no revela nada nuevo.
        assert!(hallazgos("def f(token):\n    return token == \"\"\n").is_empty());
    }

    #[test]
    fn una_comparacion_normal_no_se_toca() {
        assert!(hallazgos("def f(a, b):\n    return a == b\n").is_empty());
        assert!(hallazgos("def f(nombre):\n    return nombre == otro\n").is_empty());
    }

    // ── CWE-326 ──

    #[test]
    fn una_clave_rsa_corta_se_prueba() {
        assert!(tiene("def f():\n    k = rsa.generate_private_key(public_exponent=65537, key_size=1024)\n", "CWE-326"));
        assert!(tiene("def f():\n    k = RSA.generate(1024)\n", "CWE-326"));
    }

    #[test]
    fn una_clave_rsa_suficiente_no_se_reporta() {
        assert!(hallazgos("def f():\n    k = RSA.generate(4096)\n").is_empty());
        assert!(hallazgos("def f():\n    k = rsa.generate_private_key(public_exponent=65537, key_size=2048)\n").is_empty());
    }

    #[test]
    fn un_token_demasiado_corto_se_marca() {
        assert!(tiene("def f():\n    return secrets.token_bytes(8)\n", "CWE-326"));
        assert!(tiene("def f():\n    return os.urandom(4)\n", "CWE-326"));
    }

    #[test]
    fn un_token_de_128_bits_alcanza() {
        assert!(hallazgos("def f():\n    return secrets.token_bytes(16)\n").is_empty());
        assert!(hallazgos("def f():\n    return os.urandom(32)\n").is_empty());
    }

    #[test]
    fn un_tamano_no_literal_no_se_juzga() {
        assert!(hallazgos("def f(cfg):\n    k = RSA.generate(cfg.bits)\n").is_empty());
    }

    #[test]
    fn el_tamano_de_clave_es_high_en_las_dos_dimensiones() {
        let h = hallazgos("def f():\n    k = RSA.generate(1024)\n");
        assert_eq!(h[0].confidence, "High");
        assert_eq!(h[0].severity, "High");
    }
}
