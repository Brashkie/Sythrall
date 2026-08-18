//! Maintainability Index — reemplaza a `radon.metrics.mi_visit(content, multi=True)`.
//! Fórmula de Coleman-Oman (la misma que usa radon):
//!
//!   MI = 171 - 5.2·ln(V) - 0.23·CC_promedio - 16.2·ln(SLOC) [+ 50·sin(√(2.4·CM))]
//!   MI = max(0, min(100, MI * 100 / 171))
//!
//! donde V es el volumen de Halstead, CC_promedio el promedio de complejidad
//! ciclomática de las funciones del archivo, SLOC líneas de código fuente, y
//! CM el % de líneas que son comentarios (el término +50·sin(...) solo se
//! suma en modo `multi`, igual que radon).
//!
//! El volumen de Halstead (V = (N1+N2)·log2(η1+η2)) se calcula acá contando
//! operadores/operandos directo del AST en vez de re-tokenizar el texto — es
//! al menos tan preciso (no hay ambigüedad de qué es un operador dentro de un
//! string) y evita sumar una segunda pasada sobre el código fuente.

use std::collections::HashMap;

use rustpython_parser::ast::{self as ast, Expr, Stmt};

#[derive(Default)]
struct Halstead {
    operators: HashMap<&'static str, u32>,
    operands: HashMap<String, u32>,
}

impl Halstead {
    fn op(&mut self, key: &'static str) {
        *self.operators.entry(key).or_insert(0) += 1;
    }
    fn operand(&mut self, key: String) {
        *self.operands.entry(key).or_insert(0) += 1;
    }

    fn volume(&self) -> f64 {
        let eta1 = self.operators.len() as f64;
        let eta2 = self.operands.len() as f64;
        let n1: u32 = self.operators.values().sum();
        let n2: u32 = self.operands.values().sum();
        let vocabulary = eta1 + eta2;
        if vocabulary == 0.0 {
            return 0.0;
        }
        (n1 + n2) as f64 * vocabulary.log2()
    }
}

pub fn compute(suite: &[Stmt], avg_complexity: f64, sloc: usize, comment_ratio: f64, multi: bool) -> Option<f64> {
    if sloc == 0 {
        return None;
    }
    let mut h = Halstead::default();
    for stmt in suite {
        walk_stmt(stmt, &mut h);
    }
    let volume = h.volume();
    if volume <= 0.0 {
        return None;
    }

    let mut mi = 171.0 - 5.2 * volume.ln() - 0.23 * avg_complexity - 16.2 * (sloc as f64).ln();
    if multi {
        mi += 50.0 * (2.4 * comment_ratio).sqrt().sin();
    }
    let normalized = (mi * 100.0 / 171.0).clamp(0.0, 100.0);
    Some((normalized * 100.0).round() / 100.0)
}

fn walk_body(body: &[Stmt], h: &mut Halstead) {
    for s in body {
        walk_stmt(s, h);
    }
}

fn walk_stmt(stmt: &Stmt, h: &mut Halstead) {
    match stmt {
        Stmt::FunctionDef(s) => {
            h.op("def");
            h.operand(s.name.to_string());
            walk_body(&s.body, h);
        }
        Stmt::AsyncFunctionDef(s) => {
            h.op("async def");
            h.operand(s.name.to_string());
            walk_body(&s.body, h);
        }
        Stmt::ClassDef(s) => {
            h.op("class");
            h.operand(s.name.to_string());
            walk_body(&s.body, h);
        }
        Stmt::Return(s) => {
            h.op("return");
            if let Some(v) = &s.value {
                walk_expr(v, h);
            }
        }
        Stmt::Delete(s) => {
            h.op("del");
            for t in &s.targets {
                walk_expr(t, h);
            }
        }
        Stmt::Assign(s) => {
            h.op("=");
            for t in &s.targets {
                walk_expr(t, h);
            }
            walk_expr(&s.value, h);
        }
        Stmt::AugAssign(s) => {
            h.op("aug=");
            walk_expr(&s.target, h);
            walk_expr(&s.value, h);
        }
        Stmt::AnnAssign(s) => {
            h.op("=");
            walk_expr(&s.target, h);
            if let Some(v) = &s.value {
                walk_expr(v, h);
            }
        }
        Stmt::For(s) => {
            h.op("for");
            walk_expr(&s.target, h);
            walk_expr(&s.iter, h);
            walk_body(&s.body, h);
            walk_body(&s.orelse, h);
        }
        Stmt::AsyncFor(s) => {
            h.op("for");
            walk_expr(&s.target, h);
            walk_expr(&s.iter, h);
            walk_body(&s.body, h);
            walk_body(&s.orelse, h);
        }
        Stmt::While(s) => {
            h.op("while");
            walk_expr(&s.test, h);
            walk_body(&s.body, h);
            walk_body(&s.orelse, h);
        }
        Stmt::If(s) => {
            h.op("if");
            walk_expr(&s.test, h);
            walk_body(&s.body, h);
            walk_body(&s.orelse, h);
        }
        Stmt::With(s) => {
            h.op("with");
            for item in &s.items {
                walk_expr(&item.context_expr, h);
            }
            walk_body(&s.body, h);
        }
        Stmt::AsyncWith(s) => {
            h.op("with");
            for item in &s.items {
                walk_expr(&item.context_expr, h);
            }
            walk_body(&s.body, h);
        }
        Stmt::Try(s) => {
            h.op("try");
            walk_body(&s.body, h);
            for handler in &s.handlers {
                let ast::ExceptHandler::ExceptHandler(handler) = handler;
                h.op("except");
                walk_body(&handler.body, h);
            }
            walk_body(&s.orelse, h);
            walk_body(&s.finalbody, h);
        }
        Stmt::TryStar(s) => {
            h.op("try*");
            walk_body(&s.body, h);
            for handler in &s.handlers {
                let ast::ExceptHandler::ExceptHandler(handler) = handler;
                h.op("except*");
                walk_body(&handler.body, h);
            }
            walk_body(&s.orelse, h);
            walk_body(&s.finalbody, h);
        }
        Stmt::Assert(s) => {
            h.op("assert");
            walk_expr(&s.test, h);
        }
        Stmt::Import(_) | Stmt::ImportFrom(_) => h.op("import"),
        Stmt::Global(_) => h.op("global"),
        Stmt::Nonlocal(_) => h.op("nonlocal"),
        Stmt::Expr(s) => walk_expr(&s.value, h),
        Stmt::Pass(_) => h.op("pass"),
        Stmt::Break(_) => h.op("break"),
        Stmt::Continue(_) => h.op("continue"),
        Stmt::Match(s) => {
            h.op("match");
            walk_expr(&s.subject, h);
            for case in &s.cases {
                h.op("case");
                walk_body(&case.body, h);
            }
        }
        _ => {}
    }
}

fn walk_expr(expr: &Expr, h: &mut Halstead) {
    match expr {
        Expr::BoolOp(e) => {
            h.op(if matches!(e.op, ast::BoolOp::And) { "and" } else { "or" });
            for v in &e.values {
                walk_expr(v, h);
            }
        }
        Expr::NamedExpr(e) => {
            h.op(":=");
            walk_expr(&e.target, h);
            walk_expr(&e.value, h);
        }
        Expr::BinOp(e) => {
            h.op("binop");
            walk_expr(&e.left, h);
            walk_expr(&e.right, h);
        }
        Expr::UnaryOp(e) => {
            h.op("unary");
            walk_expr(&e.operand, h);
        }
        Expr::Lambda(e) => {
            h.op("lambda");
            walk_expr(&e.body, h);
        }
        Expr::IfExp(e) => {
            h.op("ifexp");
            walk_expr(&e.test, h);
            walk_expr(&e.body, h);
            walk_expr(&e.orelse, h);
        }
        Expr::Dict(e) => {
            h.op("{}");
            for k in e.keys.iter().flatten() {
                walk_expr(k, h);
            }
            for v in &e.values {
                walk_expr(v, h);
            }
        }
        Expr::Set(e) => {
            h.op("{}");
            for el in &e.elts {
                walk_expr(el, h);
            }
        }
        Expr::ListComp(e) => {
            h.op("[for]");
            walk_expr(&e.elt, h);
            walk_comprehensions(&e.generators, h);
        }
        Expr::SetComp(e) => {
            h.op("{for}");
            walk_expr(&e.elt, h);
            walk_comprehensions(&e.generators, h);
        }
        Expr::DictComp(e) => {
            h.op("{k:v for}");
            walk_expr(&e.key, h);
            walk_expr(&e.value, h);
            walk_comprehensions(&e.generators, h);
        }
        Expr::GeneratorExp(e) => {
            h.op("(for)");
            walk_expr(&e.elt, h);
            walk_comprehensions(&e.generators, h);
        }
        Expr::Await(e) => {
            h.op("await");
            walk_expr(&e.value, h);
        }
        Expr::Yield(e) => {
            h.op("yield");
            if let Some(v) = &e.value {
                walk_expr(v, h);
            }
        }
        Expr::YieldFrom(e) => {
            h.op("yield from");
            walk_expr(&e.value, h);
        }
        Expr::Compare(e) => {
            h.op("compare");
            walk_expr(&e.left, h);
            for c in &e.comparators {
                walk_expr(c, h);
            }
        }
        Expr::Call(e) => {
            h.op("()");
            walk_expr(&e.func, h);
            for a in &e.args {
                walk_expr(a, h);
            }
            for kw in &e.keywords {
                walk_expr(&kw.value, h);
            }
        }
        Expr::Attribute(e) => {
            h.op(".");
            h.operand(e.attr.to_string());
            walk_expr(&e.value, h);
        }
        Expr::Subscript(e) => {
            h.op("[]");
            walk_expr(&e.value, h);
            walk_expr(&e.slice, h);
        }
        Expr::Starred(e) => {
            h.op("*");
            walk_expr(&e.value, h);
        }
        Expr::Name(e) => h.operand(e.id.to_string()),
        Expr::List(e) => {
            h.op("[]");
            for el in &e.elts {
                walk_expr(el, h);
            }
        }
        Expr::Tuple(e) => {
            h.op("()");
            for el in &e.elts {
                walk_expr(el, h);
            }
        }
        Expr::Slice(e) => {
            h.op(":");
            if let Some(v) = &e.lower {
                walk_expr(v, h);
            }
            if let Some(v) = &e.upper {
                walk_expr(v, h);
            }
            if let Some(v) = &e.step {
                walk_expr(v, h);
            }
        }
        Expr::JoinedStr(e) => {
            h.op("f-string");
            for v in &e.values {
                walk_expr(v, h);
            }
        }
        Expr::FormattedValue(e) => walk_expr(&e.value, h),
        Expr::Constant(e) => h.operand(format!("{:?}", e.value)),
    }
}

fn walk_comprehensions(generators: &[ast::Comprehension], h: &mut Halstead) {
    for gen in generators {
        h.op("for");
        walk_expr(&gen.target, h);
        walk_expr(&gen.iter, h);
        for f in &gen.ifs {
            h.op("if");
            walk_expr(f, h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_module;
    use crate::raw;

    #[test]
    fn archivo_vacio_no_tiene_mi() {
        let suite = parse_module("").unwrap();
        assert_eq!(compute(&suite, 1.0, 0, 0.0, true), None);
    }

    #[test]
    fn archivo_normal_da_mi_entre_0_y_100() {
        let src = "def f(x):\n    if x:\n        return 1\n    return 0\n";
        let suite = parse_module(src).unwrap();
        let raw_stats = raw::analyze(src, &suite);
        let mi = compute(&suite, 2.0, raw_stats.sloc, 0.0, true).expect("archivo no vacío da MI");
        assert!((0.0..=100.0).contains(&mi));
    }

    #[test]
    fn mas_complejidad_baja_el_mi() {
        let simple = "def f(x):\n    return x\n";
        let complejo = "def f(x):\n    if x == 1:\n        return 1\n    elif x == 2:\n        return 2\n    elif x == 3:\n        return 3\n    else:\n        return 0\n";
        let s1 = parse_module(simple).unwrap();
        let s2 = parse_module(complejo).unwrap();
        let r1 = raw::analyze(simple, &s1);
        let r2 = raw::analyze(complejo, &s2);
        let mi1 = compute(&s1, 1.0, r1.sloc, 0.0, true).unwrap();
        let mi2 = compute(&s2, 4.0, r2.sloc, 0.0, true).unwrap();
        assert!(mi2 < mi1, "MI de código más complejo ({mi2}) debería ser menor que el simple ({mi1})");
    }
}
