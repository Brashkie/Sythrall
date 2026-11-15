//! Fase 32 (Sythrall Mathematical Analysis) — cálculo numérico: convergencia,
//! cancelación catastrófica y comparación de flotantes.
//!
//! **El hilo común es que la aritmética de punto flotante no es la de los
//! reales**, y los tres hallazgos son lugares donde código escrito pensando en
//! los reales se rompe contra IEEE-754.
//!
//! **Convergencia.** Un bucle `while abs(err) > tol:` sin tope puede no
//! terminar por dos motivos independientes, y solo uno es de matemática: que
//! la iteración no sea una contracción y el error no baje, **o que la
//! tolerancia esté por debajo de lo que el formato puede representar a esa
//! magnitud**, en cuyo caso la condición es insatisfacible por más correcto
//! que sea el método. La distancia entre dos `double` vecinos cerca de 1.0 es
//! ~2.2e-16; cerca de 1e6 es ~1.2e-10. Pedir `tol=1e-12` sobre valores del
//! orden del millón es pedir algo que no existe.
//!
//! **Cancelación catastrófica.** Acá está la intuición que casi todo el mundo
//! tiene al revés: en `(f(x+h) - f(x)) / h`, **más chico NO es más preciso**.
//! El error de truncamiento baja como `h`, pero el de redondeo sube como
//! `ε/h`; la suma se minimiza cerca de `√ε ≈ 1.5e-8` para float64. Por debajo
//! de eso la respuesta **empeora**, y cuando `h` es tan chico que `x + h == x`
//! la resta da exactamente 0 y la derivada sale 0.
//!
//! **Igualdad de flotantes.** `0.1 + 0.2 == 0.3` es falso en todos los
//! lenguajes con IEEE-754. **`0.0` queda deliberadamente afuera del chequeo**:
//! sí es exactamente representable, y compararlo suele ser un test de divisor
//! o de centinela hecho a propósito, no un accidente.
//!
//! **Lo que se prueba y lo que no.** Que el bucle no tenga tope se lee de la
//! forma; que no converja depende del mapa iterado y de los datos, y eso el
//! código no lo dice. Por eso el hallazgo es "no hay tope", y lo que el
//! mensaje ofrece es convertir un cuelgue en un retorno sin convergencia, que
//! es una falla visible y manejable.

use std::collections::HashSet;

use rustpython_parser::ast::{CmpOp, Constant, Expr, Operator, Ranged, Stmt, UnaryOp};

use crate::ast::parser::LineIndex;
use crate::hallazgos::smells::StructuralSmell;
use crate::ast::structure::node_name;
use crate::ast::walk::{walk_expr_tree, walk_stmts_own_scope};

/// Paso óptimo de una diferencia hacia adelante en float64: `√ε`, con
/// `ε ≈ 2.22e-16`. Es donde el error total se minimiza.
const PASO_OPTIMO: f64 = 1.49e-8;

/// **A partir de dónde se reporta, que NO es el óptimo.** Cerca del mínimo la
/// curva de error total es plana: `1e-8` está apenas por debajo de `√ε` y en
/// la práctica es la elección correcta, la que todo el mundo escribe.
/// Reportarla sería pedantería. El umbral se pone una década más abajo, donde
/// el redondeo ya domina de verdad y bajar el paso empeora el resultado de
/// forma medible.
const UMBRAL_DE_PASO: f64 = 1e-9;

/// Un literal por debajo de esto en el lado derecho de una comparación se lee
/// como tolerancia, no como una constante cualquiera.
const UMBRAL_DE_TOLERANCIA: f64 = 1e-3;

/// Nombres que delatan una iteración de convergencia.
const NOMBRES_DE_ERROR: &[&str] =
    &["err", "error", "delta", "residual", "resid", "tol", "epsilon", "eps", "diff", "gap", "converg"];

/// Nombres que delatan un tope de iteraciones ya presente.
const NOMBRES_DE_TOPE: &[&str] =
    &["max_iter", "maxiter", "max_steps", "maxsteps", "n_iter", "niter", "iterations", "limit", "max_it"];

/// Funciones que miden cuán lejos se está de la solución.
const FUNCIONES_DE_NORMA: &[&str] = &["abs", "fabs", "math.fabs", "np.abs", "numpy.abs", "norm", "np.linalg.norm", "numpy.linalg.norm"];

fn constante_flotante(e: &Expr) -> Option<f64> {
    match e {
        Expr::Constant(c) => match &c.value {
            Constant::Float(f) => Some(*f),
            _ => None,
        },
        Expr::UnaryOp(u) if matches!(u.op, UnaryOp::USub) => constante_flotante(&u.operand).map(|v| -v),
        _ => None,
    }
}

fn contiene(nombre: &str, lista: &[&str]) -> bool {
    let n = nombre.to_lowercase();
    lista.iter().any(|s| n.contains(s))
}

/// Todos los identificadores que aparecen en una expresión, incluyendo el
/// atributo de un `self.err`.
fn nombres_en(e: &Expr) -> HashSet<String> {
    let mut out = HashSet::new();
    walk_expr_tree(e, &mut |x: &Expr| match x {
        Expr::Name(n) => {
            out.insert(n.id.to_string());
        }
        Expr::Attribute(a) => {
            out.insert(a.attr.to_string());
        }
        _ => {}
    });
    out
}

/// `true` si la condición mide un error contra una tolerancia.
fn es_condicion_de_convergencia(test: &Expr) -> bool {
    let mut hay_norma = false;
    let mut hay_tolerancia = false;
    walk_expr_tree(test, &mut |x: &Expr| {
        if let Expr::Call(c) = x {
            if FUNCIONES_DE_NORMA.contains(&node_name(&c.func).as_str()) {
                hay_norma = true;
            }
        }
        if let Some(v) = constante_flotante(x) {
            if v.abs() > 0.0 && v.abs() < UMBRAL_DE_TOLERANCIA {
                hay_tolerancia = true;
            }
        }
    });
    hay_norma
        || hay_tolerancia
        || nombres_en(test).iter().any(|n| contiene(n, NOMBRES_DE_ERROR))
}

/// Nombres que el cuerpo incrementa con una constante: los candidatos a
/// contador. Cubre `i += 1` y `i = i + 1`.
fn nombres_incrementados(body: &[Stmt]) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut on_stmt = |s: &Stmt| match s {
        Stmt::AugAssign(a) if matches!(a.op, Operator::Add) => {
            if let Expr::Name(n) = &*a.target {
                out.insert(n.id.to_string());
            }
        }
        Stmt::Assign(a) => {
            let Expr::BinOp(b) = &*a.value else { return };
            if !matches!(b.op, Operator::Add) {
                return;
            }
            for t in &a.targets {
                let Expr::Name(destino) = t else { continue };
                if nombres_en(&b.left).contains(destino.id.as_str())
                    || nombres_en(&b.right).contains(destino.id.as_str())
                {
                    out.insert(destino.id.to_string());
                }
            }
        }
        _ => {}
    };
    let mut sin_expr = |_: &Expr| {};
    walk_stmts_own_scope(body, &mut on_stmt, &mut sin_expr);
    out
}

/// Las condiciones de los `if` que contienen un `break` — o sea, las que
/// pueden cortar el bucle.
fn guardas_de_salida(body: &[Stmt]) -> Vec<&Expr> {
    fn buscar<'a>(stmts: &'a [Stmt], out: &mut Vec<&'a Expr>) {
        for s in stmts {
            match s {
                Stmt::If(i) => {
                    if i.body.iter().any(|x| matches!(x, Stmt::Break(_)))
                        || i.orelse.iter().any(|x| matches!(x, Stmt::Break(_)))
                    {
                        out.push(&i.test);
                    }
                    buscar(&i.body, out);
                    buscar(&i.orelse, out);
                }
                Stmt::With(w) => buscar(&w.body, out),
                Stmt::Try(t) => {
                    buscar(&t.body, out);
                    buscar(&t.orelse, out);
                    buscar(&t.finalbody, out);
                }
                // No se entra a bucles anidados: sus `break` salen de ELLOS.
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    buscar(body, &mut out);
    out
}

fn es_literal_verdadero(e: &Expr) -> bool {
    matches!(e, Expr::Constant(c) if matches!(c.value, Constant::Bool(true)))
}

/// `true` si algo acota la cantidad de vueltas: un contador que el cuerpo
/// incrementa y que aparece en una condición de corte, o un nombre que dice
/// explícitamente ser un tope.
fn tiene_tope(test: &Expr, body: &[Stmt]) -> bool {
    let contadores = nombres_incrementados(body);
    let mut condiciones: Vec<&Expr> = vec![test];
    let salidas = guardas_de_salida(body);
    condiciones.extend(salidas.iter().copied());
    condiciones.iter().any(|c| {
        let nombres = nombres_en(c);
        nombres.iter().any(|n| contadores.contains(n)) || nombres.iter().any(|n| contiene(n, NOMBRES_DE_TOPE))
    })
}

fn revisar_convergencia(
    scope: &str,
    body: &[Stmt],
    line_index: &LineIndex,
    out: &mut Vec<StructuralSmell>,
) {
    let mut on_stmt = |s: &Stmt| {
        let Stmt::While(w) = s else { return };
        // `while True:` con un `if abs(err) < tol: break` adentro es la forma
        // más común de escribir esto, y la condición del bucle no dice nada.
        let de_convergencia = if es_literal_verdadero(&w.test) {
            guardas_de_salida(&w.body).iter().any(|g| es_condicion_de_convergencia(g))
        } else {
            es_condicion_de_convergencia(&w.test)
        };
        if !de_convergencia || tiene_tope(&w.test, &w.body) {
            return;
        }
        out.push(StructuralSmell {
            kind: "unbounded_convergence_loop",
            name: scope.to_string(),
            line: line_index.line_of(w.range.start().to_usize()),
            message:
                "This loop iterates until a numerical tolerance is met, with nothing capping how many times it may run. Two independent things can keep it going forever and only one is about the mathematics: the iteration may not be a contraction, so the error never shrinks — or the tolerance may be below what the floating-point format can represent at this magnitude, in which case the condition is unsatisfiable no matter how correct the method is (two neighbouring doubles are ~2.2e-16 apart near 1.0, but ~1.2e-10 apart near 1e6, so `tol=1e-12` on values around a million is asking for something that does not exist). A `max_iter` counter turns 'hangs forever' into 'returned without converging', which is a failure you can see and handle."
                    .to_string(),
        });
    };
    let mut sin_expr = |_: &Expr| {};
    walk_stmts_own_scope(body, &mut on_stmt, &mut sin_expr);
}

/// Nombres a los que se les asignó un flotante muy chico en este cuerpo.
fn pasos_chicos(body: &[Stmt]) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut on_stmt = |s: &Stmt| {
        let Stmt::Assign(a) = s else { return };
        let Some(v) = constante_flotante(&a.value) else { return };
        if v.abs() >= UMBRAL_DE_PASO || v == 0.0 {
            return;
        }
        for t in &a.targets {
            if let Expr::Name(n) = t {
                out.insert(n.id.to_string());
            }
        }
    };
    let mut sin_expr = |_: &Expr| {};
    walk_stmts_own_scope(body, &mut on_stmt, &mut sin_expr);
    out
}

fn revisar_cancelacion(
    scope: &str,
    body: &[Stmt],
    line_index: &LineIndex,
    out: &mut Vec<StructuralSmell>,
) {
    let chicos = pasos_chicos(body);
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |e: &Expr| {
        let Expr::BinOp(div) = e else { return };
        if !matches!(div.op, Operator::Div) {
            return;
        }
        // Una diferencia dividida: `(f(x+h) - f(x)) / h`.
        let Expr::BinOp(resta) = &*div.left else { return };
        if !matches!(resta.op, Operator::Sub) {
            return;
        }
        let paso = match &*div.right {
            Expr::Name(n) if chicos.contains(n.id.as_str()) => Some(n.id.to_string()),
            otro => constante_flotante(otro)
                .filter(|v| v.abs() > 0.0 && v.abs() < UMBRAL_DE_PASO)
                .map(|v| format!("{v:e}")),
        };
        let Some(paso) = paso else { return };
        out.push(StructuralSmell {
            kind: "catastrophic_cancellation_step",
            name: scope.to_string(),
            line: line_index.line_of(e.range().start().to_usize()),
            message: format!(
                "A divided difference with a step of `{paso}`. Smaller is NOT more accurate here, which is the intuition almost everyone has backwards: truncation error falls like `h`, but rounding error grows like `ε/h`, so the total is minimised around `√ε ≈ {PASO_OPTIMO:e}` for float64. Below that the answer gets worse, and once `h` is small enough that `x + h == x` the subtraction returns exactly 0 and the derivative comes out as 0. For more accuracy the ways up are a central difference (`(f(x+h) - f(x-h)) / (2h)`, optimal near `ε^(1/3) ≈ 6e-6`) or complex-step differentiation (`imag(f(x + ih)) / h`), which has no subtraction and therefore no cancellation at all."
            ),
        });
    };
    walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
}

fn revisar_igualdad_flotante(
    scope: &str,
    body: &[Stmt],
    line_index: &LineIndex,
    out: &mut Vec<StructuralSmell>,
) {
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |e: &Expr| {
        let Expr::Compare(c) = e else { return };
        if c.ops.len() != 1 || !matches!(c.ops[0], CmpOp::Eq | CmpOp::NotEq) {
            return;
        }
        let literal = [&*c.left, &c.comparators[0]]
            .into_iter()
            .filter_map(constante_flotante)
            // `0.0` SÍ es exactamente representable, y compararlo suele ser
            // un test de divisor o centinela deliberado.
            .find(|v| *v != 0.0);
        let Some(v) = literal else { return };
        out.push(StructuralSmell {
            kind: "float_equality_comparison",
            name: scope.to_string(),
            line: line_index.line_of(e.range().start().to_usize()),
            message: format!(
                "Exact `==`/`!=` against the float literal `{v}`. Most decimal fractions have no finite binary representation, so a value that is mathematically equal can still compare false — `0.1 + 0.2 == 0.3` is False in every IEEE-754 language, not just Python. Use `math.isclose(a, b)`, or an explicit tolerance when the scale is known. Comparing against `0.0` is deliberately NOT reported: zero is exactly representable, and testing for it is usually an intentional divisor or sentinel check rather than an accident."
            ),
        });
    };
    walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
}

/// Punto de entrada por función, misma convención que el resto de los
/// productores de `StructuralSmell`.
pub fn numeric_smells(scope: &str, body: &[Stmt], line_index: &LineIndex) -> Vec<StructuralSmell> {
    let mut out = Vec::new();
    revisar_convergencia(scope, body, line_index, &mut out);
    revisar_cancelacion(scope, body, line_index, &mut out);
    revisar_igualdad_flotante(scope, body, line_index, &mut out);
    out.sort_by_key(|s| s.line);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::parse_module;

    fn hallazgos(src: &str) -> Vec<StructuralSmell> {
        let suite = parse_module(src).expect("parse error");
        let idx = LineIndex::build(src);
        let (scope, body) = match &suite[0] {
            Stmt::FunctionDef(f) => (f.name.to_string(), f.body.clone()),
            _ => ("<module>".to_string(), suite.clone()),
        };
        numeric_smells(&scope, &body, &idx)
    }

    fn tiene(src: &str, kind: &str) -> bool {
        hallazgos(src).iter().any(|s| s.kind == kind)
    }

    // ── Convergencia ──

    #[test]
    fn un_bucle_de_convergencia_sin_tope_se_marca() {
        let src = "def f(x):\n    err = 1.0\n    while abs(err) > 1e-9:\n        err = paso(x)\n";
        assert!(tiene(src, "unbounded_convergence_loop"), "{:?}", hallazgos(src));
    }

    #[test]
    fn un_contador_en_la_condicion_es_un_tope() {
        let src = "def f(x):\n    err = 1.0\n    i = 0\n    while abs(err) > 1e-9 and i < 100:\n        err = paso(x)\n        i += 1\n";
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src));
    }

    #[test]
    fn un_break_por_contador_tambien_es_un_tope() {
        let src = "def f(x):\n    err = 1.0\n    i = 0\n    while abs(err) > 1e-9:\n        err = paso(x)\n        i = i + 1\n        if i > 100:\n            break\n";
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src));
    }

    #[test]
    fn un_nombre_de_tope_explicito_alcanza() {
        // Aunque no se vea el incremento, `max_iter` dice lo que dice.
        let src = "def f(x, max_iter):\n    err = 1.0\n    while abs(err) > 1e-9 and pasos < max_iter:\n        err = paso(x)\n";
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src));
    }

    #[test]
    fn while_true_con_break_de_convergencia_tambien_cuenta() {
        // Es la forma mas comun de escribirlo, y la condicion del `while` no
        // dice nada por si sola.
        let src = "def f(x):\n    while True:\n        err = paso(x)\n        if abs(err) < 1e-9:\n            break\n";
        assert!(tiene(src, "unbounded_convergence_loop"), "{:?}", hallazgos(src));
    }

    #[test]
    fn un_while_que_no_mide_error_no_se_toca() {
        assert!(hallazgos("def f(items):\n    while items:\n        items.pop()\n").is_empty());
        assert!(hallazgos("def f(n):\n    while n > 0:\n        n = n - 1\n").is_empty());
    }

    #[test]
    fn el_nombre_de_la_variable_alcanza_como_señal() {
        let src = "def f(x):\n    residual = 1.0\n    while residual > umbral:\n        residual = paso(x)\n";
        assert!(tiene(src, "unbounded_convergence_loop"));
    }

    // ── Cancelación catastrófica ──

    #[test]
    fn un_paso_literal_demasiado_chico_se_marca() {
        let src = "def d(f, x):\n    return (f(x + 1e-12) - f(x)) / 1e-12\n";
        assert!(tiene(src, "catastrophic_cancellation_step"), "{:?}", hallazgos(src));
    }

    #[test]
    fn un_paso_por_variable_tambien() {
        let src = "def d(f, x):\n    h = 1e-15\n    return (f(x + h) - f(x)) / h\n";
        assert!(tiene(src, "catastrophic_cancellation_step"), "{:?}", hallazgos(src));
    }

    #[test]
    fn el_paso_optimo_no_se_reporta() {
        // ~sqrt(epsilon): es el minimo del error total, no un problema.
        let src = "def d(f, x):\n    h = 1e-8\n    return (f(x + h) - f(x)) / h\n";
        assert!(!tiene(src, "catastrophic_cancellation_step"), "{:?}", hallazgos(src));
    }

    #[test]
    fn una_division_que_no_es_una_diferencia_no_cuenta() {
        let src = "def f(a):\n    h = 1e-15\n    return a / h\n";
        assert!(!tiene(src, "catastrophic_cancellation_step"), "{:?}", hallazgos(src));
    }

    // ── Igualdad de flotantes ──

    #[test]
    fn comparar_con_un_literal_flotante_se_marca() {
        assert!(tiene("def f(x):\n    return x == 0.1\n", "float_equality_comparison"));
        assert!(tiene("def f(x):\n    return x != 3.14\n", "float_equality_comparison"));
    }

    #[test]
    fn comparar_contra_cero_no_se_reporta() {
        // 0.0 SI es exactamente representable, y compararlo suele ser un test
        // de divisor deliberado.
        assert!(hallazgos("def f(x):\n    return x == 0.0\n").is_empty());
    }

    #[test]
    fn comparar_enteros_no_se_toca() {
        assert!(hallazgos("def f(x):\n    return x == 3\n").is_empty());
    }

    #[test]
    fn el_mensaje_explica_por_que_mas_chico_es_peor() {
        let h = hallazgos("def d(f, x):\n    return (f(x + 1e-12) - f(x)) / 1e-12\n");
        assert!(h[0].message.contains("Smaller is NOT more accurate"), "{}", h[0].message);
    }
}
