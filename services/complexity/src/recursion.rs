//! Detección de recursión directa + tail-call — puerto 1:1 de
//! `_recursion_info_python`/`_recursion_note` en `static_parser.py`. Mismo
//! criterio: `return f(n-1)` es tail (nada pendiente después de la llamada),
//! `return n + f(n-1)` no lo es. Python no optimiza tail calls — sigue
//! consumiendo stack — pero es información útil: indica que se podría
//! reescribir como loop ("Cálculo Lambda": reducible a iteración).

use rustpython_parser::ast::{Expr, Stmt};
use serde::Serialize;

use crate::walk::walk_stmts;

#[derive(Serialize, Clone, Default)]
pub struct RecursionInfo {
    pub is_recursive: bool,
    pub is_tail_recursive: bool,
    pub call_count: u32,
}

fn calls_self(expr: &Expr, name: &str) -> bool {
    matches!(expr, Expr::Call(c) if matches!(&*c.func, Expr::Name(n) if n.id.as_str() == name))
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
