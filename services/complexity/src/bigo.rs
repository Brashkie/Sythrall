//! Heurística de Big-O + Θ/Ω — puerto 1:1 de `_infer_big_o_python`/
//! `_theta_omega_python`/`_has_binary_split_python` en `static_parser.py`.
//! Reusa `is_recursive` y `depth`/`has_early_exit` ya calculados por
//! `recursion.rs`/esta misma pasada, en vez de volver a recorrer el AST.

use rustpython_parser::ast::{Expr, Operator, Stmt};
use serde::Serialize;

use crate::walk::walk_stmts;

#[derive(Serialize, Clone)]
pub struct BigO {
    pub big_o: String,
    pub reason: String,
    pub theta: String,
    pub omega: String,
    pub recurrence: Option<String>,
    /// Fase 15 (Mathematical Intelligence): Combinatoria — ver
    /// `combinatorics_note` más abajo.
    pub combinatorics_note: Option<String>,
}

/// Fase 15 (Mathematical Intelligence), tercer ítem: la cardinalidad de
/// loops anidados ya se calcula para Big-O (`depth`, de `loop_analysis` acá
/// mismo) — esto reencuadra ese mismo número explícitamente como lo que en
/// realidad es: la regla del producto (multiplication principle) de
/// combinatoria, no solo una "complejidad" abstracta. `depth` loops
/// anidados, cada uno recorriendo n elementos, consideran exactamente n^depth
/// combinaciones — el mismo conteo que se usa para contar tuplas ordenadas
/// de longitud `depth` con repetición a partir de n elementos. Deliberadamente
/// acotado a loops anidados puros (`!is_recursive && !binary`): O(log n)
/// (búsqueda binaria), O(n log n) (loop + recursión) y O(2^n) (recursión sin
/// reducir el problema) no tienen esta misma lectura de "regla del
/// producto sobre loops", así que no se les agrega la nota.
fn combinatorics_note(depth: u32, is_recursive: bool, binary: bool) -> Option<String> {
    if is_recursive || binary || depth == 0 {
        return None;
    }
    if depth == 1 {
        return Some(
            "Single pass over n elements — one independent choice, no combinatorial interaction with anything else in the loop".to_string(),
        );
    }
    Some(format!(
        "{depth} nested loops, each ranging independently over n — by the rule of product (multiplication principle), that's n^{depth} total combinations considered, the same count as ordered {depth}-tuples with repetition drawn from n elements"
    ))
}

/// Señal de recursión divide-and-conquer: `a` subproblemas (de
/// `divide_conquer_factor`, NO de `recursion::analyze`'s `call_count` — ver
/// ese fn para el porqué) sobre `n/2` (el único divisor que
/// `has_binary_split` reconoce hoy), más `c_own` — el grado de trabajo de
/// "combine" visible dentro de la propia función (su propia profundidad de
/// loop). El segundo pase en `rich.rs` puede subir ese grado si la función
/// llama a un helper (ej. `merge()`) ya analizado.
#[derive(Clone, Copy)]
pub struct DivideConquerSignal {
    pub a: u32,
    pub c_own: u32,
}

/// Profundidad máxima de loops anidados y si hay un break/return dentro de
/// algún loop (early exit) — un solo recorrido para ambas señales.
pub fn loop_analysis(body: &[Stmt]) -> (u32, bool) {
    fn walk(stmts: &[Stmt], depth: u32, inside_loop: bool, max_depth: &mut u32, has_early_exit: &mut bool) {
        for stmt in stmts {
            let (is_loop, children, orelse): (bool, &[Stmt], &[Stmt]) = match stmt {
                Stmt::For(s) => (true, &s.body, &s.orelse),
                Stmt::AsyncFor(s) => (true, &s.body, &s.orelse),
                Stmt::While(s) => (true, &s.body, &s.orelse),
                Stmt::If(s) => (false, &s.body, &s.orelse),
                Stmt::With(s) => (false, &s.body, &[]),
                Stmt::AsyncWith(s) => (false, &s.body, &[]),
                Stmt::Try(s) => (false, &s.body, &s.orelse),
                _ => (false, &[], &[]),
            };
            let d = if is_loop { depth + 1 } else { depth };
            *max_depth = (*max_depth).max(d);
            let child_inside_loop = inside_loop || is_loop;
            if child_inside_loop && matches!(stmt, Stmt::Break(_) | Stmt::Return(_)) {
                *has_early_exit = true;
            }
            walk(children, d, child_inside_loop, max_depth, has_early_exit);
            walk(orelse, d, child_inside_loop, max_depth, has_early_exit);
            if let Stmt::Try(s) = stmt {
                for h in &s.handlers {
                    let rustpython_parser::ast::ExceptHandler::ExceptHandler(h) = h;
                    walk(&h.body, d, child_inside_loop, max_depth, has_early_exit);
                }
                walk(&s.finalbody, d, child_inside_loop, max_depth, has_early_exit);
            }
        }
    }
    let mut max_depth = 0u32;
    let mut has_early_exit = false;
    walk(body, 0, false, &mut max_depth, &mut has_early_exit);
    (max_depth, has_early_exit)
}

pub(crate) fn has_binary_split(body: &[Stmt]) -> bool {
    let mut found = false;
    let mut on_expr = |expr: &Expr| {
        if found {
            return;
        }
        if let Expr::BinOp(b) = expr {
            let const_eq = |e: &Expr, want: i64| matches!(e, Expr::Constant(c) if matches!(&c.value, rustpython_parser::ast::Constant::Int(i) if i == &want.into()));
            match b.op {
                Operator::FloorDiv if const_eq(&b.right, 2) => found = true,
                Operator::RShift if const_eq(&b.right, 1) => found = true,
                _ => {}
            }
        }
    };
    let mut on_stmt = |_: &Stmt| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    found
}

pub fn infer(is_recursive: bool, depth: u32, binary: bool) -> (String, String) {
    if depth == 0 && !is_recursive {
        return ("O(1)".into(), "No loops or recursion — time doesn't depend on n".into());
    }
    if depth == 1 && binary {
        return (
            "O(log n)".into(),
            "Loop with binary split — the range halves every iteration".into(),
        );
    }
    if depth == 1 && is_recursive {
        return (
            "O(n log n)".into(),
            "Loop combined with recursion — mixes a linear pass with logarithmic reduction".into(),
        );
    }
    if depth == 1 {
        return ("O(n)".into(), "One loop — walks the n elements once".into());
    }
    if depth == 2 {
        return (
            "O(n\u{b2})".into(),
            "2 nested loops — the inner loop runs n times per outer iteration".into(),
        );
    }
    if depth == 3 {
        return (
            "O(n\u{b3})".into(),
            "3 nested loops — each extra level multiplies the work by n".into(),
        );
    }
    if depth >= 4 {
        return (
            format!("O(n^{depth})"),
            format!("{depth} nested loops — polynomial growth of degree {depth}"),
        );
    }
    if is_recursive && !binary {
        return (
            "O(2^n)".into(),
            "Recursion without reducing the problem — each call spawns new calls without splitting the input"
                .into(),
        );
    }
    ("O(n)".into(), "Base case — linear behavior by default".into())
}

pub fn theta_omega(has_early_exit: bool, worst: &str) -> (String, String) {
    let suffix = &worst[1..]; // "O(n²)" -> "(n²)"
    if has_early_exit {
        return (format!("varies between \u{3a9}(1) and O{suffix}"), "\u{3a9}(1)".to_string());
    }
    (format!("\u{398}{suffix}"), format!("\u{3a9}{suffix}"))
}

pub fn full(body: &[Stmt], name: &str, is_recursive: bool) -> (BigO, Option<DivideConquerSignal>) {
    let (depth, has_early_exit) = loop_analysis(body);
    let binary = has_binary_split(body);
    let (big_o, reason) = infer(is_recursive, depth, binary);
    let (theta, omega) = theta_omega(has_early_exit, &big_o);
    let combinatorics_note = combinatorics_note(depth, is_recursive, binary);
    let signal = if is_recursive && binary {
        let a = divide_conquer_factor(body, name);
        (a > 0).then_some(DivideConquerSignal { a, c_own: depth })
    } else {
        None
    };
    (BigO { big_o, reason, theta, omega, recurrence: None, combinatorics_note }, signal)
}

fn calls_self(expr: &Expr, name: &str) -> bool {
    matches!(expr, Expr::Call(c) if matches!(&*c.func, Expr::Name(n) if n.id.as_str() == name))
}

/// Cuántas auto-llamadas puede disparar UNA sola ejecución de la función —
/// a diferencia de `recursion::analyze`'s `call_count`, que cuenta TODOS los
/// sitios de auto-llamada del cuerpo sin importar si son alcanzables en el
/// mismo camino de ejecución. Trata las ramas de un mismo `if/elif/else`
/// como alternativas mutuamente excluyentes (toma el máximo entre ramas) y
/// las statements secuenciales como aditivas (se suman). Así `merge_sort`
/// (`left = merge_sort(...)` seguido de `right = merge_sort(...)`, ambas
/// ejecutan siempre) da `a=2`, mientras una búsqueda binaria recursiva (una
/// auto-llamada en la rama `elif` y otra en la rama `else` del mismo if,
/// mutuamente excluyentes) da `a=1` — sin esta distinción, el Master
/// Theorem resolvería la búsqueda binaria como si fuera `T(n)=2T(n/2)+O(1)`
/// (\u{398}(n)) en vez de la real `T(n)=T(n/2)+O(1)` (\u{398}(log n)).
fn divide_conquer_factor(body: &[Stmt], name: &str) -> u32 {
    body.iter().map(|s| stmt_factor(s, name)).sum()
}

fn stmt_factor(stmt: &Stmt, name: &str) -> u32 {
    match stmt {
        Stmt::If(s) => divide_conquer_factor(&s.body, name).max(divide_conquer_factor(&s.orelse, name)),
        Stmt::For(s) => divide_conquer_factor(&s.body, name) + divide_conquer_factor(&s.orelse, name),
        Stmt::AsyncFor(s) => divide_conquer_factor(&s.body, name) + divide_conquer_factor(&s.orelse, name),
        Stmt::While(s) => divide_conquer_factor(&s.body, name) + divide_conquer_factor(&s.orelse, name),
        Stmt::With(s) => divide_conquer_factor(&s.body, name),
        Stmt::AsyncWith(s) => divide_conquer_factor(&s.body, name),
        Stmt::Try(s) => {
            let handlers_max = s
                .handlers
                .iter()
                .map(|h| {
                    let rustpython_parser::ast::ExceptHandler::ExceptHandler(h) = h;
                    divide_conquer_factor(&h.body, name)
                })
                .max()
                .unwrap_or(0);
            divide_conquer_factor(&s.body, name).max(handlers_max)
                + divide_conquer_factor(&s.orelse, name)
                + divide_conquer_factor(&s.finalbody, name)
        }
        _ => leaf_self_call_count(stmt, name),
    }
}

fn leaf_self_call_count(stmt: &Stmt, name: &str) -> u32 {
    let mut count = 0u32;
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |e: &Expr| {
        if calls_self(e, name) {
            count += 1;
        }
    };
    walk_stmts(std::slice::from_ref(stmt), &mut on_stmt, &mut on_expr);
    count
}

/// Parsea el grado polinomial de una string de Big-O ya generada por
/// `infer()` — usado por el segundo pase de `rich.rs` para inferir el grado
/// del "combine step" de una función divide-and-conquer a partir de los
/// Big-O ya resueltos de las funciones que llama (ej. `merge()` → O(n) → 1).
/// `O(log n)`/`O(2^n)`/formas desconocidas no aportan un grado polinomial
/// útil y devuelven `None` — el llamador simplemente no las cuenta.
pub fn degree_of(big_o: &str) -> Option<u32> {
    match big_o {
        "O(1)" => Some(0),
        "O(n)" | "O(n log n)" => Some(1),
        "O(n\u{b2})" => Some(2),
        "O(n\u{b3})" => Some(3),
        s if s.starts_with("O(n^") => s.trim_start_matches("O(n^").trim_end_matches(')').parse().ok(),
        _ => None,
    }
}

/// `n^e` formateado como lo escribiría un libro de texto: exponentes enteros
/// (o muy cercanos a un entero, ej. `log2(8)` con error de punto flotante)
/// se muestran sin decimales (`n`, `n^2`); el resto (Karatsuba-style,
/// `log2(3)\u{2248}1.585`) con 3 decimales.
fn n_pow_label(e: f64) -> String {
    if (e - e.round()).abs() < 1e-6 {
        match e.round() as i64 {
            0 => "1".to_string(),
            1 => "n".to_string(),
            k => format!("n^{k}"),
        }
    } else {
        format!("n^{e:.3}")
    }
}

/// Resuelve el Master Theorem para `T(n) = a\u{b7}T(n/2) + f(n)`, con `f(n) =
/// \u{398}(n^c)` donde `c` es el máximo entre el trabajo de combine visible
/// dentro de la propia función (`sig.c_own`) y el grado de las funciones que
/// llama y que ya fueron analizadas (`callee_degrees`, típicamente un
/// helper como `merge()`). Devuelve `(recurrence, big_o, theta, omega, reason)` —
/// los casos 1 y 2 dan una cota ajustada real (`omega == theta`); el caso 3
/// hereda la cota del combine step, que domina a las llamadas recursivas.
pub fn resolve_master_theorem(sig: &DivideConquerSignal, callee_degrees: &[u32]) -> (String, String, String, String, String) {
    let a = sig.a;
    let c = callee_degrees.iter().copied().chain(std::iter::once(sig.c_own)).max().unwrap_or(0);

    let f_n = match c {
        0 => "\u{398}(1)".to_string(),
        1 => "\u{398}(n)".to_string(),
        c => format!("\u{398}(n^{c})"),
    };
    let recurrence = format!("T(n) = {a}T(n/2) + {f_n}");

    let (log_b_a, log_is_exact) = if a.is_power_of_two() {
        (a.trailing_zeros() as f64, true)
    } else {
        ((a as f64).log2(), false)
    };

    let c_f = c as f64;
    let below = if log_is_exact { c < log_b_a as u32 } else { c_f + 1e-9 < log_b_a };
    let equal = if log_is_exact { c == log_b_a as u32 } else { (c_f - log_b_a).abs() < 1e-9 };

    let (theta_inner, reason) = if below {
        (
            n_pow_label(log_b_a),
            format!(
                "Master Theorem, case 1 — {a} recursive subcalls on n/2 each, and the combine step ({f_n}) grows slower than n^log2({a})\u{2248}n^{log_b_a:.3}, so the recursive leaves dominate."
            ),
        )
    } else if equal {
        let inner = if c == 0 { "log n".to_string() } else { format!("{} log n", n_pow_label(c_f)) };
        (
            inner,
            format!(
                "Master Theorem, case 2 — {a} recursive subcalls on n/2 each, and the combine step ({f_n}) matches n^log2({a}), so every recursion level contributes equally."
            ),
        )
    } else {
        (
            n_pow_label(c_f),
            format!(
                "Master Theorem, case 3 — the combine step ({f_n}) outgrows n^log2({a})\u{2248}n^{log_b_a:.3}, so it dominates the {a} recursive subcalls."
            ),
        )
    };

    let theta = format!("\u{398}({theta_inner})");
    let big_o = format!("O({theta_inner})");
    (recurrence, big_o, theta.clone(), theta, reason)
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
    fn sin_loops_ni_recursion_no_tiene_nota_combinatoria() {
        assert!(combinatorics_note(0, false, false).is_none());
    }

    #[test]
    fn un_loop_da_nota_de_paso_unico() {
        let note = combinatorics_note(1, false, false).unwrap();
        assert!(note.contains("Single pass"));
    }

    #[test]
    fn dos_loops_anidados_da_regla_del_producto_n_cuadrado() {
        let note = combinatorics_note(2, false, false).unwrap();
        assert!(note.contains("n^2"));
        assert!(note.contains("rule of product"));
    }

    #[test]
    fn tres_loops_anidados_da_regla_del_producto_n_cubo() {
        let note = combinatorics_note(3, false, false).unwrap();
        assert!(note.contains("n^3"));
    }

    #[test]
    fn recursion_no_tiene_nota_combinatoria_aunque_haya_loops() {
        // O(n log n) (loop + recursión) no es una lectura de "regla del
        // producto sobre loops anidados" — se deja sin nota a propósito.
        assert!(combinatorics_note(1, true, false).is_none());
    }

    #[test]
    fn busqueda_binaria_no_tiene_nota_combinatoria() {
        assert!(combinatorics_note(1, false, true).is_none());
    }

    #[test]
    fn funcion_o_n2_end_to_end_tiene_nota_combinatoria() {
        let src = "def f(items):\n    for a in items:\n        for b in items:\n            print(a, b)\n";
        let body = first_function_body(src);
        let (bigo, _) = full(&body, "f", false);
        assert_eq!(bigo.big_o, "O(n\u{b2})");
        assert!(bigo.combinatorics_note.unwrap().contains("n^2"));
    }

    #[test]
    fn funcion_o_log_n_end_to_end_no_tiene_nota_combinatoria() {
        let src = "def f(n):\n    while n > 1:\n        n = n // 2\n";
        let body = first_function_body(src);
        let (bigo, _) = full(&body, "f", false);
        assert_eq!(bigo.big_o, "O(log n)");
        assert!(bigo.combinatorics_note.is_none());
    }
}
