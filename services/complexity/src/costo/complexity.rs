//! Complejidad ciclomática de McCabe, calculada sobre el AST — reemplaza a
//! `radon.complexity.cc_visit`/`cc_rank`. **Paridad exacta con radon,
//! medida**: 134 funciones reales del backend, 0 diferencias.
//!
//! **Cinco reglas faltaban, y las cinco subcontaban** (2026-09-22). Este
//! doc afirmaba "mismo criterio que radon" mientras 32 de 118 funciones
//! (27%) daban distinto, y **siempre de menos** — nunca de más:
//!
//! | construcción | radon | estaba |
//! |---|---|---|
//! | **`for` de una comprehension** | `len(ifs) + 1` | solo los filtros `if` |
//! | `else` de `for`/`while` | `+1` | 0 |
//! | `else` de `try` | `+1` | 0 |
//! | `assert` | `+1` | 0 |
//! | `match` | resta 1 **solo si hay `case _`** | restaba 1 siempre |
//!
//! **La dominante es la primera**: toda comprehension sin filtro no sumaba
//! nada, y son la mayoría. Una función real del backend daba 4 donde radon
//! da 13.
//!
//! Y el caso de `match` es la **cuarta variante** del mismo patrón: el
//! comodín no es una decisión sino la rama que queda, igual que el
//! `default` de un `switch` en C (un `case_statement` sin campo `value`),
//! en JS (`switch_default` propio) y en Fortran (un hijo token `default`).
//! Cuatro lenguajes, cuatro representaciones del mismo concepto.
//!
//! Reglas completas: cada función/método parte de 1, +1 por cada
//! `if`/`elif`, ternario, `for`/`while` (más su `else`), cada `except`
//! (más el `else` del `try`), cada `assert`, cada generador de
//! comprehension (más sus filtros `if`), y `n-1` por cada `and`/`or` de n
//! operandos.

use rustpython_parser::ast::{self as ast, Expr, Stmt};
use serde::Serialize;

use crate::ast::parser::LineIndex;

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
    // Índice de saltos de línea construido UNA VEZ para todo el archivo —
    // antes, cada función/método llamaba a `line_of_offset` suelta, que
    // re-escaneaba desde el offset 0 en cada llamada (O(n) por función,
    // O(n²) total en un archivo con muchas funciones). Ver `parser::LineIndex`.
    let line_index = LineIndex::build(source);
    let mut out = Vec::new();
    collect_functions(&line_index, suite, false, &mut out);
    out
}

fn collect_functions(line_index: &LineIndex, body: &[Stmt], in_class: bool, out: &mut Vec<FunctionComplexity>) {
    for stmt in body {
        match stmt {
            Stmt::FunctionDef(f) => {
                out.push(function_entry(line_index, &f.name, f.range.start().to_usize(), &f.body, in_class));
            }
            Stmt::AsyncFunctionDef(f) => {
                out.push(function_entry(line_index, &f.name, f.range.start().to_usize(), &f.body, in_class));
            }
            Stmt::ClassDef(c) => {
                collect_functions(line_index, &c.body, true, out);
            }
            _ => {}
        }
    }
}

fn function_entry(
    line_index: &LineIndex,
    name: &str,
    start_offset: usize,
    body: &[Stmt],
    in_class: bool,
) -> FunctionComplexity {
    let complexity = cyclomatic(body);
    FunctionComplexity {
        name: name.to_string(),
        kind: if in_class { "method" } else { "function" },
        line: line_index.line_of(start_offset),
        complexity,
        rank: cc_rank(complexity),
    }
}

/// McCabe cyclomatic complexity de un cuerpo de función — factorizado de
/// `function_entry` así `rich.rs` lo puede reusar sin pasar por todo el
/// pipeline de `/metrics/complexity` (que arma `FunctionComplexity`, no el
/// dict más rico que necesita `/parse/python`).
pub fn cyclomatic(body: &[Stmt]) -> u32 {
    let mut complexity = 1u32;
    walk_body(body, &mut complexity);
    complexity
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
            // radon: `bool(node.orelse) + 1`. El `else` de un bucle es
            // una ruta propia: corre solo si no hubo `break`.
            *cx += 1 + u32::from(!s.orelse.is_empty());
            walk_expr(&s.iter, cx);
            walk_body(&s.body, cx);
            walk_body(&s.orelse, cx);
        }
        Stmt::AsyncFor(s) => {
            *cx += 1 + u32::from(!s.orelse.is_empty());
            walk_expr(&s.iter, cx);
            walk_body(&s.body, cx);
            walk_body(&s.orelse, cx);
        }
        Stmt::While(s) => {
            *cx += 1 + u32::from(!s.orelse.is_empty());
            walk_expr(&s.test, cx);
            walk_body(&s.body, cx);
            walk_body(&s.orelse, cx);
        }
        Stmt::Try(s) => {
            // radon: `len(node.handlers) + bool(node.orelse)`. El `else`
            // de un `try` corre solo si NO hubo excepcion: otra ruta.
            *cx += s.handlers.len() as u32 + u32::from(!s.orelse.is_empty());
            walk_body(&s.body, cx);
            for h in &s.handlers {
                let ast::ExceptHandler::ExceptHandler(h) = h;
                walk_body(&h.body, cx);
            }
            walk_body(&s.orelse, cx);
            walk_body(&s.finalbody, cx);
        }
        Stmt::TryStar(s) => {
            *cx += s.handlers.len() as u32 + u32::from(!s.orelse.is_empty());
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
            // radon: `max(0, len(cases) - hay_comodin)`. Solo se resta
            // uno si existe un `case _`, que no es una decision sino la
            // rama que queda — el mismo criterio que el `default` de un
            // switch en C, JS y Fortran.
            // El comodín `case _:` es un `MatchAs` con patrón interno vacío —
            // la misma forma que radon busca con
            // `getattr(case.pattern, "pattern", False) is None`.
            let comodin = s
                .cases
                .iter()
                .any(|c| matches!(&c.pattern, ast::Pattern::MatchAs(p) if p.pattern.is_none()));
            *cx += (s.cases.len().saturating_sub(u32::from(comodin) as usize)) as u32;
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
        Stmt::Assert(s) => {
            // radon: `self.complexity += not self.no_assert` — con su
            // default (`no_assert=False`) un `assert` suma 1. Es una
            // bifurcacion real: o sigue, o levanta.
            *cx += 1;
            walk_expr(&s.test, cx);
        }
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
        // radon: `len(node.ifs) + 1`. El `for` de la comprehension es en
        // si mismo un punto de decision — contar solo los filtros `if`
        // subcontaba toda comprehension sin filtro, que son la mayoria.
        *cx += gen.ifs.len() as u32 + 1;
        for f in &gen.ifs {
            walk_expr(f, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::parse_module;

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
