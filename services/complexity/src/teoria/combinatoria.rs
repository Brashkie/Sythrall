//! **Explosión combinatoria: cuando el bucle no recorre `n` elementos sino
//! `n!`.**
//!
//! Medido antes de escribir nada, sobre dos funciones que iteran
//! `itertools.permutations`:
//!
//! ```text
//! asignación por fuerza bruta    real O(n!·n)   decía O(n²)   FALSO
//! viajante por fuerza bruta      real O(n!·n)   decía O(n²)   FALSO
//! ```
//!
//! No es un hueco: es una afirmación falsa, y de las peores que puede hacer
//! un analizador de complejidad. Para `n = 13`, `n!` ya son 6.200 millones
//! de tuplas; el motor le decía al usuario "cuadrático".
//!
//! **La causa es que el conteo de anidamiento mide la FORMA del bucle y no
//! cuántas veces corre.** `for perm in permutations(xs)` es un solo `for`,
//! así que suma profundidad 1 igual que `for x in xs`. Lo que cambia es el
//! cardinal del iterable, y eso está escrito en la llamada.
//!
//! **Y el hallazgo que sale de acá tiene que decir dos cosas opuestas según
//! el problema, que es lo que lo hace honesto.** El mismo código —recorrer
//! todas las permutaciones y quedarse con la mejor— es:
//!
//! - un **error arreglable** si lo que se minimiza es una suma de costos por
//!   asignación: el algoritmo húngaro (Kuhn–Munkres, 1955) da **el mismo
//!   óptimo** en O(n³), y ya viene en `scipy.optimize.linear_sum_assignment`.
//!   No es una aproximación más rápida: es la misma respuesta.
//! - un **costo inherente** si es el problema del viajante: es NP-difícil y
//!   no se conoce ningún algoritmo exacto polinómico. Held–Karp baja a
//!   O(n²·2ⁿ), que es mejor que `n!` pero sigue siendo exponencial.
//!
//! Distinguir los dos casos desde el AST no es decidible en general, así que
//! **no se adivina**: se reconoce la forma del problema de asignación
//! —indexar una matriz con `[i][perm[i]]`— y solo ahí se nombra el húngaro.
//! Sin esa forma, el hallazgo dice el costo y presenta las dos salidas.

use rustpython_parser::ast::{Expr, Stmt};

use crate::hallazgos::smells::StructuralSmell;
use crate::ast::walk::walk_stmts;

/// Lo que se encontró: qué generador, de qué clase, y por qué.
#[derive(Debug, Clone)]
pub struct Explosion {
    pub generador: String,
    /// La clase que le corresponde, ya formateada.
    pub clase: &'static str,
    pub motivo: String,
    /// `true` si además se ve la forma del problema de asignación.
    pub forma_de_asignacion: bool,
    pub linea_aprox: usize,
}

/// El nombre de la función llamada, sea `permutations(...)` o
/// `itertools.permutations(...)`.
fn nombre_llamado(e: &Expr) -> Option<&str> {
    let Expr::Call(c) = e else { return None };
    match &*c.func {
        Expr::Name(n) => Some(n.id.as_str()),
        Expr::Attribute(a) => Some(a.attr.as_str()),
        _ => None,
    }
}

/// `true` si la expresión es un literal entero.
fn es_entero_literal(e: &Expr) -> bool {
    matches!(e, Expr::Constant(c) if matches!(&c.value, rustpython_parser::ast::Constant::Int(_)))
}

/// La clase de un generador combinatorio de `itertools`, o `None` si no se
/// puede afirmar.
///
/// **Lo que deliberadamente NO se clasifica**: `product(a, b)` sin `repeat`
/// —es `|a|·|b|`, que no es explosivo por sí solo—, y cualquier
/// `combinations(x, k)` con `k` no literal: `C(n, k)` es polinómica para `k`
/// fijo y exponencial para `k = n/2`, así que sin el valor no hay nada que
/// afirmar.
fn clase_de(e: &Expr) -> Option<(&'static str, String)> {
    let Expr::Call(c) = e else { return None };
    let nombre = nombre_llamado(e)?;
    match nombre {
        // `permutations(xs)` son n!; con longitud literal, n!/(n-r)! ≈ n^r.
        "permutations" => {
            if c.args.len() >= 2 && es_entero_literal(&c.args[1]) {
                Some((
                    "O(n^k)",
                    "`permutations(xs, k)` with a literal length yields n!/(n-k)! tuples, which grows like n^k".to_string(),
                ))
            } else {
                Some((
                    "O(n!)",
                    "`permutations(xs)` yields n! tuples — for n = 13 that is already 6.2 billion, and for n = 20 it is 2.4 quintillion".to_string(),
                ))
            }
        }
        "combinations" | "combinations_with_replacement" if c.args.len() >= 2 && es_entero_literal(&c.args[1]) => {
            Some(("O(n^k)", "`combinations(xs, k)` with a literal k yields C(n, k) tuples, which grows like n^k".to_string()))
        }
        "product" => {
            let con_repeat = c.keywords.iter().any(|k| k.arg.as_ref().is_some_and(|a| a.as_str() == "repeat") && es_entero_literal(&k.value));
            con_repeat.then(|| ("O(n^k)", "`product(xs, repeat=k)` yields n^k tuples".to_string()))
        }
        _ => None,
    }
}

/// La forma del problema de ASIGNACIÓN: una matriz indexada con el contador
/// del bucle interno en la primera posición y la permutación en la segunda,
/// o sea `costos[i][perm[i]]`.
///
/// **Es lo único que autoriza a nombrar el húngaro**, porque es lo único que
/// distingue "sumar un costo por asignación" de "sumar distancias de un
/// recorrido". Sin esta forma no se propone un reemplazo que podría no
/// aplicar.
fn tiene_forma_de_asignacion(body: &[Stmt], nombre_perm: &str) -> bool {
    let mut hay = false;
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |e: &Expr| {
        if hay {
            return;
        }
        // `costos[i][perm[i]]` es un Subscript cuyo VALOR es otro Subscript,
        // y cuyo índice menciona la permutación.
        let Expr::Subscript(externo) = e else { return };
        if !matches!(&*externo.value, Expr::Subscript(_)) {
            return;
        }
        let mut menciona = false;
        crate::ast::walk::walk_expr_tree(&externo.slice, &mut |x| {
            if matches!(x, Expr::Name(n) if n.id.as_str() == nombre_perm) {
                menciona = true;
            }
        });
        if menciona {
            hay = true;
        }
    };
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    hay
}

/// El nombre del blanco de un `for`, si es un nombre pelado.
fn blanco_simple(target: &Expr) -> Option<&str> {
    match target {
        Expr::Name(n) => Some(n.id.as_str()),
        _ => None,
    }
}

/// La explosión combinatoria dominante del cuerpo, si la hay.
pub fn explosion_dominante(body: &[Stmt]) -> Option<Explosion> {
    let mut hallada: Option<Explosion> = None;
    let mut on_stmt = |stmt: &Stmt| {
        let Stmt::For(f) = stmt else { return };
        let Some((clase, motivo)) = clase_de(&f.iter) else { return };
        // `O(n!)` domina a `O(n^k)`: si hay las dos, se reporta la peor.
        if hallada.as_ref().is_some_and(|h| h.clase == "O(n!)") {
            return;
        }
        let generador = nombre_llamado(&f.iter).unwrap_or("?").to_string();
        let forma = blanco_simple(&f.target).is_some_and(|n| tiene_forma_de_asignacion(&f.body, n));
        hallada = Some(Explosion {
            generador,
            clase,
            motivo,
            forma_de_asignacion: forma,
            linea_aprox: 0,
        });
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    hallada
}

/// El hallazgo estructural que acompaña a la explosión.
pub fn smell_de(explosion: &Explosion, nombre_funcion: &str, linea: usize) -> StructuralSmell {
    let salida = if explosion.forma_de_asignacion {
        "Y acá hay una salida concreta, porque se ve la forma del problema de ASIGNACIÓN: la matriz se indexa con `[i][perm[i]]`, o sea que lo que se minimiza es una suma de costos por asignación. El algoritmo húngaro (Kuhn–Munkres, 1955) resuelve exactamente ese problema en O(n³), y ya viene implementado en `scipy.optimize.linear_sum_assignment`. NO es una aproximación más rápida: da EL MISMO óptimo. La fuerza bruta no es más confiable acá, es la misma respuesta exponencialmente más cara."
    } else {
        "Qué hacer con esto depende del problema, y desde el código no se puede decidir cuál es. Si lo que se minimiza es una suma de costos por asignación, el algoritmo húngaro da el MISMO óptimo en O(n³) (`scipy.optimize.linear_sum_assignment`). Si es el problema del viajante, no hay salida polinómica conocida —es NP-difícil— y lo mejor exacto es Held–Karp, O(n²·2ⁿ): sigue siendo exponencial, pero para n = 20 son mil millones de operaciones en vez de 2,4 trillones."
    };
    StructuralSmell {
        kind: "factorial_brute_force",
        name: nombre_funcion.to_string(),
        line: linea,
        message: format!(
            "Este bucle no recorre n elementos: recorre lo que genera `{}`. {} El conteo de bucles anidados mide la FORMA del bucle, no cuántas veces corre, así que sin esto la función se reportaría como polinómica. {salida}",
            explosion.generador, explosion.motivo
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::parse_module;

    fn cuerpo(src: &str) -> Vec<Stmt> {
        let suite = parse_module(src).unwrap();
        for stmt in &suite {
            if let Stmt::FunctionDef(f) = stmt {
                return f.body.clone();
            }
        }
        panic!("sin función");
    }

    /// **El caso central**: un `for` sobre `permutations` es un solo bucle
    /// sintáctico y n! iteraciones reales.
    #[test]
    fn un_for_sobre_permutations_es_factorial() {
        let src = "from itertools import permutations\ndef f(xs):\n    for p in permutations(xs):\n        pass\n";
        let e = explosion_dominante(&cuerpo(src)).unwrap();
        assert_eq!(e.clase, "O(n!)");
        assert!(e.motivo.contains("6.2 billion"), "{}", e.motivo);
    }

    /// Calificado con el módulo también.
    #[test]
    fn itertools_permutations_calificado_tambien() {
        let src = "import itertools\ndef f(xs):\n    for p in itertools.permutations(xs):\n        pass\n";
        assert_eq!(explosion_dominante(&cuerpo(src)).unwrap().clase, "O(n!)");
    }

    /// Con longitud literal ya no es factorial: `n!/(n-k)!` crece como n^k.
    #[test]
    fn permutations_con_longitud_literal_no_es_factorial() {
        let src = "from itertools import permutations\ndef f(xs):\n    for p in permutations(xs, 2):\n        pass\n";
        assert_eq!(explosion_dominante(&cuerpo(src)).unwrap().clase, "O(n^k)");
    }

    /// **Lo que deliberadamente NO se clasifica**: `product` sin `repeat` es
    /// `|a|·|b|`, que no explota por sí solo.
    #[test]
    fn product_sin_repeat_no_se_clasifica() {
        let src = "from itertools import product\ndef f(a, b):\n    for p in product(a, b):\n        pass\n";
        assert!(explosion_dominante(&cuerpo(src)).is_none());
    }

    #[test]
    fn product_con_repeat_literal_si() {
        let src = "from itertools import product\ndef f(a):\n    for p in product(a, repeat=3):\n        pass\n";
        assert_eq!(explosion_dominante(&cuerpo(src)).unwrap().clase, "O(n^k)");
    }

    /// Y un `for` normal no es una explosión.
    #[test]
    fn un_for_normal_no_es_una_explosion() {
        let src = "def f(xs):\n    for x in xs:\n        pass\n";
        assert!(explosion_dominante(&cuerpo(src)).is_none());
    }

    /// **La forma del problema de asignación**: `costos[i][perm[i]]`.
    #[test]
    fn la_forma_de_asignacion_se_reconoce() {
        let src = "from itertools import permutations\ndef f(costos):\n    n = len(costos)\n    for perm in permutations(range(n)):\n        total = 0\n        for i in range(n):\n            total += costos[i][perm[i]]\n";
        let e = explosion_dominante(&cuerpo(src)).unwrap();
        assert!(e.forma_de_asignacion);
        let m = smell_de(&e, "f", 1).message;
        assert!(m.contains("húngaro"), "{m}");
        assert!(m.contains("EL MISMO óptimo"), "no es una aproximación: {m}");
        assert!(m.contains("linear_sum_assignment"), "{m}");
    }

    /// **Y sin esa forma NO se nombra el húngaro**, porque proponerlo para
    /// un viajante sería proponer algo que no aplica.
    #[test]
    fn sin_la_forma_de_asignacion_no_se_propone_el_hungaro_como_solucion() {
        let src = "from itertools import permutations\ndef f(d):\n    for ruta in permutations(range(5)):\n        total = 0\n        anterior = 0\n        for c in ruta:\n            total += d[anterior][c]\n            anterior = c\n";
        let e = explosion_dominante(&cuerpo(src)).unwrap();
        assert!(!e.forma_de_asignacion, "un recorrido no es una asignación");
        let m = smell_de(&e, "f", 1).message;
        assert!(m.contains("Held–Karp"), "presenta la salida del viajante: {m}");
        assert!(m.contains("NP-difícil"), "{m}");
        assert!(m.contains("depende del problema"), "y no decide por el usuario: {m}");
    }

    /// El mensaje explica por qué el conteo de bucles se equivocaba.
    #[test]
    fn el_mensaje_explica_por_que_el_conteo_de_bucles_fallaba() {
        let src = "from itertools import permutations\ndef f(xs):\n    for p in permutations(xs):\n        pass\n";
        let m = smell_de(&explosion_dominante(&cuerpo(src)).unwrap(), "f", 1).message;
        assert!(m.contains("la FORMA del bucle, no cuántas veces corre"), "{m}");
    }
}
