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

fn has_binary_split(body: &[Stmt]) -> bool {
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

pub fn infer(body: &[Stmt], is_recursive: bool, depth: u32) -> (String, String) {
    let binary = has_binary_split(body);

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

pub fn full(body: &[Stmt], is_recursive: bool) -> BigO {
    let (depth, has_early_exit) = loop_analysis(body);
    let (big_o, reason) = infer(body, is_recursive, depth);
    let (theta, omega) = theta_omega(has_early_exit, &big_o);
    BigO { big_o, reason, theta, omega }
}
