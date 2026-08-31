//! Detección de recursión directa + tail-call — puerto 1:1 de
//! `_recursion_info_python`/`_recursion_note` en `static_parser.py`. Mismo
//! criterio: `return f(n-1)` es tail (nada pendiente después de la llamada),
//! `return n + f(n-1)` no lo es. Python no optimiza tail calls — sigue
//! consumiendo stack — pero es información útil: indica que se podría
//! reescribir como loop ("Cálculo Lambda": reducible a iteración).

use rustpython_parser::ast::{Expr, Stmt};
use serde::Serialize;

use crate::walk::{walk_expr_tree, walk_stmts};

#[derive(Serialize, Clone, Default)]
pub struct RecursionInfo {
    pub is_recursive: bool,
    pub is_tail_recursive: bool,
    pub call_count: u32,
    /// Fase 15 (Mathematical Intelligence), último ítem: Proof-by-induction
    /// framing — ver `has_base_case`/`induction_note` más abajo.
    pub has_base_case: bool,
}

fn calls_self(expr: &Expr, name: &str) -> bool {
    matches!(expr, Expr::Call(c) if matches!(&*c.func, Expr::Name(n) if n.id.as_str() == name))
}

/// `true` si el valor de `expr` contiene una auto-llamada EN CUALQUIER
/// PARTE de su árbol, no solo en la posición de tail-call — a diferencia de
/// `calls_self` (que exige que la llamada SEA la expresión completa, la
/// semántica correcta para detectar tail-call), acá `return n * f(n-1)`
/// también cuenta como "este return recursa", aunque la llamada esté
/// envuelta en un `BinOp`.
fn contains_self_call(expr: &Expr, name: &str) -> bool {
    let mut found = false;
    let mut on_expr = |e: &Expr| {
        if calls_self(e, name) {
            found = true;
        }
    };
    walk_expr_tree(expr, &mut on_expr);
    found
}

/// `true` si al menos un `return` del cuerpo (propio scope de la función,
/// mismo criterio de `walk_stmts` que ya usa `analyze` acá abajo) devuelve
/// algo que NO contiene ninguna auto-llamada — evidencia de que existe al
/// menos un camino de ejecución que termina sin recursar, la mitad "caso
/// base" de una prueba por inducción. `return` sin valor (`return` pelado)
/// también cuenta: nunca recursa. No es una prueba de terminación real (eso
/// es indecidible en general) — es la misma heurística de forma de AST que
/// el resto del CS Engine usa: evidencia razonable, no una garantía.
fn has_base_case(name: &str, body: &[Stmt]) -> bool {
    let mut found = false;
    let mut on_stmt = |stmt: &Stmt| {
        if found {
            return;
        }
        if let Stmt::Return(r) = stmt {
            let recurses = r.value.as_ref().is_some_and(|v| contains_self_call(v, name));
            if !recurses {
                found = true;
            }
        }
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    found
}

pub fn analyze(name: &str, body: &[Stmt]) -> RecursionInfo {
    let mut call_count = 0u32;
    let mut tail_count = 0u32;

    let mut on_stmt = |stmt: &Stmt| {
        if let Stmt::Return(r) = stmt {
            if let Some(v) = &r.value {
                if calls_self(v, name) {
                    tail_count += 1;
                }
            }
        }
    };
    let mut on_expr = |expr: &Expr| {
        if calls_self(expr, name) {
            call_count += 1;
        }
    };
    walk_stmts(body, &mut on_stmt, &mut on_expr);

    if call_count == 0 {
        return RecursionInfo::default();
    }
    RecursionInfo {
        is_recursive: true,
        is_tail_recursive: tail_count == call_count,
        call_count,
        has_base_case: has_base_case(name, body),
    }
}

pub fn note(info: &RecursionInfo) -> Option<String> {
    if !info.is_recursive {
        return None;
    }
    Some(if info.is_tail_recursive {
        "Tail-call — every recursive call is the last thing the function does; \
         equivalent to a loop (Lambda Calculus: reducible to iteration). \
         Python doesn't optimize tail calls, so it still consumes stack."
            .to_string()
    } else {
        "Not tail-call — there's pending work after the recursive call \
         (e.g. adding its result); each recursion level consumes a stack frame."
            .to_string()
    })
}

/// Fase 15 (Mathematical Intelligence), último ítem: Proof-by-induction
/// framing. Solo se emite cuando SE PUDO CONFIRMAR una evidencia razonable
/// de las dos mitades — un `has_base_case` detectado (`recursion.rs`, más
/// arriba) además de `is_recursive` (la auto-llamada, la mitad "paso
/// inductivo") — nunca cuando falta una de las dos: sin caso base
/// detectado, no hay nada honesto que decir sobre inducción todavía (podría
/// terminar por otra vía que esta heurística no reconoce, o ser un bug de
/// recursión infinita), así que se queda en silencio en vez de adivinar,
/// mismo criterio que el resto de esta fase. Explícitamente una nota
/// explicativa, no una demostración generada: no verifica que el caso base
/// sea alcanzable de verdad, ni que el paso inductivo reduzca el problema
/// (eso ya lo cubre `is_recursive`/Big-O por separado) — solo señala que el
/// CUERPO de la función tiene la forma de una prueba por inducción.
pub fn induction_note(info: &RecursionInfo) -> Option<String> {
    if !info.is_recursive || !info.has_base_case {
        return None;
    }
    Some(
        "Shaped like a proof by induction — a base case (at least one return path that \
         doesn't call itself, the P(k) you'd verify directly) and an inductive step (the \
         recursive call, assuming correctness for a smaller input to establish it for this \
         one). This is a shape observation, not a generated proof: it doesn't check that the \
         base case is actually reachable or that the recursion is well-founded."
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_module;

    fn first_function_body(src: &str) -> Vec<Stmt> {
        let suite = parse_module(src).unwrap();
        for stmt in &suite {
            if let Stmt::FunctionDef(f) = stmt {
                return f.body.clone();
            }
        }
        panic!("no se encontró ninguna función en el src de prueba");
    }

    #[test]
    fn factorial_tiene_caso_base_y_paso_inductivo() {
        let src = "def factorial(n):\n    if n <= 1:\n        return 1\n    return n * factorial(n - 1)\n";
        let body = first_function_body(src);
        let info = analyze("factorial", &body);
        assert!(info.is_recursive);
        assert!(info.has_base_case);
    }

    #[test]
    fn recursion_sin_ningun_return_que_no_recurse_no_tiene_caso_base() {
        // Recursión infinita real (bug) — nunca hay un return que no se
        // llame a sí misma. `has_base_case` debe ser honesto: false.
        let src = "def loop_forever(n):\n    return loop_forever(n)\n";
        let body = first_function_body(src);
        let info = analyze("loop_forever", &body);
        assert!(info.is_recursive);
        assert!(!info.has_base_case);
    }

    #[test]
    fn return_desnudo_cuenta_como_caso_base() {
        let src = "def f(n):\n    if n <= 0:\n        return\n    f(n - 1)\n";
        let body = first_function_body(src);
        let info = analyze("f", &body);
        assert!(info.is_recursive);
        assert!(info.has_base_case);
    }

    #[test]
    fn llamada_recursiva_envuelta_en_expresion_sigue_contando_como_recursion_no_como_base() {
        // "return n * factorial(n-1)" — la auto-llamada NO es la expresión
        // completa (está envuelta en un BinOp), pero `contains_self_call`
        // tiene que verla igual: este return SÍ recursa, no es un caso base.
        let src = "def f(n):\n    return n * f(n - 1)\n";
        let body = first_function_body(src);
        assert!(!has_base_case("f", &body));
    }

    #[test]
    fn funcion_no_recursiva_no_tiene_induction_note() {
        let info = RecursionInfo { is_recursive: false, is_tail_recursive: false, call_count: 0, has_base_case: true };
        assert!(induction_note(&info).is_none());
    }

    #[test]
    fn recursiva_sin_caso_base_no_tiene_induction_note() {
        let info = RecursionInfo { is_recursive: true, is_tail_recursive: false, call_count: 1, has_base_case: false };
        assert!(induction_note(&info).is_none());
    }

    #[test]
    fn recursiva_con_caso_base_tiene_induction_note() {
        let info = RecursionInfo { is_recursive: true, is_tail_recursive: false, call_count: 1, has_base_case: true };
        let note = induction_note(&info).unwrap();
        assert!(note.contains("base case"));
        assert!(note.contains("inductive step"));
    }

    #[test]
    fn factorial_end_to_end_tiene_induction_note() {
        let src = "def factorial(n):\n    if n <= 1:\n        return 1\n    return n * factorial(n - 1)\n";
        let body = first_function_body(src);
        let info = analyze("factorial", &body);
        assert!(induction_note(&info).is_some());
    }

    #[test]
    fn loop_forever_end_to_end_no_tiene_induction_note() {
        let src = "def loop_forever(n):\n    return loop_forever(n)\n";
        let body = first_function_body(src);
        let info = analyze("loop_forever", &body);
        assert!(induction_note(&info).is_none());
    }
}
