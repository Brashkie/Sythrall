//! Complejidad ciclomática de McCabe, calculada sobre el AST — reemplaza a
//! `radon.complexity.cc_visit`/`cc_rank`. Reglas (mismo criterio que radon):
//! cada función/método parte de 1, +1 por cada `if`/`elif`, `for`, `while`,
//! `except`, comparación booleana adicional (`and`/`or`: n operandos = n-1
//! puntos de decisión), filtro `if` de comprehension, y ternario (`x if c
//! else y`).

use rustpython_parser::ast::{self as ast, Expr, Stmt};
use serde::Serialize;

use crate::parser::line_of_offset;

#[derive(Serialize, Clone)]
pub struct FunctionComplexity {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub line: usize,
    pub complexity: u32,
    pub rank: char,
}

/// Letra A-F según el score — mismos cortes que `radon.complexity.cc_rank`.
pub fn cc_rank(complexity: u32) -> char {
    match complexity {
        1..=5 => 'A',
        6..=10 => 'B',
        11..=20 => 'C',
        21..=30 => 'D',
        31..=40 => 'E',
        _ => 'F',
    }
}

pub fn analyze(source: &str, suite: &[Stmt]) -> Vec<FunctionComplexity> {
    let mut out = Vec::new();
    collect_functions(source, suite, false, &mut out);
    out
}

fn collect_functions(source: &str, body: &[Stmt], in_class: bool, out: &mut Vec<FunctionComplexity>) {
    for stmt in body {
        match stmt {
            Stmt::FunctionDef(f) => {
                out.push(function_entry(source, &f.name, f.range.start().to_usize(), &f.body, in_class));
            }
            Stmt::AsyncFunctionDef(f) => {
                out.push(function_entry(source, &f.name, f.range.start().to_usize(), &f.body, in_class));
            }
            Stmt::ClassDef(c) => {
                collect_functions(source, &c.body, true, out);
            }
            _ => {}
        }
    }
}

fn function_entry(
    source: &str,
    name: &str,
    start_offset: usize,
    body: &[Stmt],
    in_class: bool,
) -> FunctionComplexity {
    let mut complexity = 1u32;
    walk_body(body, &mut complexity);
    FunctionComplexity {
        name: name.to_string(),
        kind: if in_class { "method" } else { "function" },
        line: line_of_offset(source, start_offset),
        complexity,
        rank: cc_rank(complexity),
    }
}

fn walk_body(body: &[Stmt], cx: &mut u32) {
    for stmt in body {
        walk_stmt(stmt, cx);
    }
}

fn walk_stmt(stmt: &Stmt, cx: &mut u32) {
    match stmt {
        Stmt::If(s) => {
            *cx += 1;
            walk_expr(&s.test, cx);
            walk_body(&s.body, cx);
            walk_body(&s.orelse, cx);
        }
        Stmt::For(s) => {
            *cx += 1;
            walk_expr(&s.iter, cx);
            walk_body(&s.body, cx);
            walk_body(&s.orelse, cx);
        }
        Stmt::AsyncFor(s) => {
            *cx += 1;
            walk_expr(&s.iter, cx);
            walk_body(&s.body, cx);
            walk_body(&s.orelse, cx);
        }
        Stmt::While(s) => {
            *cx += 1;
            walk_expr(&s.test, cx);
            walk_body(&s.body, cx);
            walk_body(&s.orelse, cx);
        }
        Stmt::Try(s) => {
            *cx += s.handlers.len() as u32;
            walk_body(&s.body, cx);
            for h in &s.handlers {
                let ast::ExceptHandler::ExceptHandler(h) = h;
                walk_body(&h.body, cx);
            }
            walk_body(&s.orelse, cx);
            walk_body(&s.finalbody, cx);
        }
        Stmt::TryStar(s) => {
            *cx += s.handlers.len() as u32;
            walk_body(&s.body, cx);
            for h in &s.handlers {
                let ast::ExceptHandler::ExceptHandler(h) = h;
                walk_body(&h.body, cx);
            }
            walk_body(&s.orelse, cx);
            walk_body(&s.finalbody, cx);
        }
        Stmt::With(s) => {
            for item in &s.items {
                walk_expr(&item.context_expr, cx);
            }
            walk_body(&s.body, cx);
        }
        Stmt::AsyncWith(s) => {
            for item in &s.items {
                walk_expr(&item.context_expr, cx);
            }
            walk_body(&s.body, cx);
        }
        Stmt::Match(s) => {
            // Cada `case` extra es otra ruta posible, igual que un `elif`.
            *cx += s.cases.len().saturating_sub(1) as u32;
            walk_expr(&s.subject, cx);
            for case in &s.cases {
                walk_body(&case.body, cx);
            }
        }
        Stmt::Assign(s) => walk_expr(&s.value, cx),
        Stmt::AugAssign(s) => walk_expr(&s.value, cx),
        Stmt::AnnAssign(s) => {
            if let Some(v) = &s.value {
                walk_expr(v, cx);
            }
        }
        Stmt::Return(s) => {
            if let Some(v) = &s.value {
                walk_expr(v, cx);
            }
        }
        Stmt::Expr(s) => walk_expr(&s.value, cx),
        Stmt::Assert(s) => walk_expr(&s.test, cx),
        Stmt::Delete(s) => {
            for t in &s.targets {
                walk_expr(t, cx);
            }
        }
        // FunctionDef/ClassDef anidados adentro de otra función son funciones
        // propias — no suman a la complejidad del padre, se cuentan aparte en
        // collect_functions cuando se camine ese cuerpo directamente.
        _ => {}
    }
}

fn walk_expr(expr: &Expr, cx: &mut u32) {
    match expr {
        Expr::BoolOp(e) => {
            *cx += e.values.len().saturating_sub(1) as u32;
            for v in &e.values {
                walk_expr(v, cx);
            }
        }
        Expr::IfExp(e) => {
            *cx += 1;
            walk_expr(&e.test, cx);
            walk_expr(&e.body, cx);
            walk_expr(&e.orelse, cx);
        }
        Expr::NamedExpr(e) => walk_expr(&e.value, cx),
        Expr::BinOp(e) => {
            walk_expr(&e.left, cx);
            walk_expr(&e.right, cx);
        }
        Expr::UnaryOp(e) => walk_expr(&e.operand, cx),
        Expr::Lambda(e) => walk_expr(&e.body, cx),
        Expr::Compare(e) => {
            walk_expr(&e.left, cx);
            for c in &e.comparators {
                walk_expr(c, cx);
            }
        }
        Expr::Call(e) => {
            walk_expr(&e.func, cx);
            for a in &e.args {
                walk_expr(a, cx);
            }
            for kw in &e.keywords {
                walk_expr(&kw.value, cx);
            }
        }
        Expr::Await(e) => walk_expr(&e.value, cx),
        Expr::Yield(e) => {
            if let Some(v) = &e.value {
                walk_expr(v, cx);
            }
        }
        Expr::YieldFrom(e) => walk_expr(&e.value, cx),
        Expr::Attribute(e) => walk_expr(&e.value, cx),
        Expr::Starred(e) => walk_expr(&e.value, cx),
        Expr::Subscript(e) => {
            walk_expr(&e.value, cx);
            walk_expr(&e.slice, cx);
        }
        Expr::Slice(e) => {
            if let Some(v) = &e.lower {
                walk_expr(v, cx);
            }
            if let Some(v) = &e.upper {
                walk_expr(v, cx);
            }
            if let Some(v) = &e.step {
                walk_expr(v, cx);
            }
        }
        Expr::List(e) => {
            for el in &e.elts {
                walk_expr(el, cx);
            }
        }
        Expr::Tuple(e) => {
            for el in &e.elts {
                walk_expr(el, cx);
            }
        }
        Expr::Set(e) => {
            for el in &e.elts {
                walk_expr(el, cx);
            }
        }
        Expr::Dict(e) => {
            for k in e.keys.iter().flatten() {
                walk_expr(k, cx);
            }
            for v in &e.values {
                walk_expr(v, cx);
            }
        }
        Expr::ListComp(e) => {
            walk_expr(&e.elt, cx);
            walk_comprehensions(&e.generators, cx);
        }
        Expr::SetComp(e) => {
            walk_expr(&e.elt, cx);
            walk_comprehensions(&e.generators, cx);
        }
        Expr::GeneratorExp(e) => {
            walk_expr(&e.elt, cx);
            walk_comprehensions(&e.generators, cx);
        }
        Expr::DictComp(e) => {
            walk_expr(&e.key, cx);
            walk_expr(&e.value, cx);
            walk_comprehensions(&e.generators, cx);
        }
        Expr::JoinedStr(e) => {
            for v in &e.values {
                walk_expr(v, cx);
            }
        }
        Expr::FormattedValue(e) => walk_expr(&e.value, cx),
        // Name/Constant y el resto son hojas — nada que recorrer.
        _ => {}
    }
}

fn walk_comprehensions(generators: &[ast::Comprehension], cx: &mut u32) {
    for gen in generators {
        walk_expr(&gen.iter, cx);
        *cx += gen.ifs.len() as u32;
        for f in &gen.ifs {
            walk_expr(f, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_module;

    fn complexity_of(src: &str, name: &str) -> u32 {
        let suite = parse_module(src).expect("parse ok");
        analyze(src, &suite)
            .into_iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("no se encontró la función {name}"))
            .complexity
    }

    #[test]
    fn function_sin_ramas_es_1() {
        let src = "def f():\n    return 1\n";
        assert_eq!(complexity_of(src, "f"), 1);
    }

    #[test]
    fn un_if_es_2() {
        let src = "def f(x):\n    if x:\n        return 1\n    return 0\n";
        assert_eq!(complexity_of(src, "f"), 2);
    }

    #[test]
    fn if_elif_else_es_3() {
        // radon/McCabe: el `elif` es un StmtIf anidado en el `orelse` del
        // primero — cada uno suma 1, el `else` final no suma nada.
        let src = "def f(x):\n    if x == 1:\n        return 1\n    elif x == 2:\n        return 2\n    else:\n        return 0\n";
        assert_eq!(complexity_of(src, "f"), 3);
    }

    #[test]
    fn for_mas_if_es_3() {
        let src = "def f(items):\n    total = 0\n    for i in items:\n        if i > 0:\n            total += i\n    return total\n";
        assert_eq!(complexity_of(src, "f"), 3);
    }

    #[test]
    fn and_or_suman_por_operando_extra() {
        // `a and b or c` => 2 BoolOp con 2 operandos cada uno => +1 +1 = 2
        let src = "def f(a, b, c):\n    if a and b or c:\n        return 1\n    return 0\n";
        assert_eq!(complexity_of(src, "f"), 4);
    }

    #[test]
    fn ternario_suma_uno() {
        let src = "def f(x):\n    return 1 if x else 0\n";
        assert_eq!(complexity_of(src, "f"), 2);
    }

    #[test]
    fn metodo_de_clase_se_detecta_como_method() {
        let src = "class C:\n    def m(self):\n        return 1\n";
        let suite = parse_module(src).unwrap();
        let funcs = analyze(src, &suite);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].kind, "method");
    }

    #[test]
    fn rank_letters() {
        assert_eq!(cc_rank(1), 'A');
        assert_eq!(cc_rank(5), 'A');
        assert_eq!(cc_rank(6), 'B');
        assert_eq!(cc_rank(11), 'C');
        assert_eq!(cc_rank(21), 'D');
        assert_eq!(cc_rank(31), 'E');
        assert_eq!(cc_rank(41), 'F');
    }
}
