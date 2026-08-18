//! Recorrido genérico y exhaustivo del AST — equivalente a `ast.walk()` de
//! Python (visita cada statement y cada expresión, sin importar cuán anidado
//! esté: comprehensions, lambdas, dict literals, todo). `complexity.rs`
//! resuelve su propio recorrido inline porque solo necesita sumar un
//! contador; los módulos que necesitan buscar patrones distintos en el mismo
//! árbol (recursion.rs, classifiers.rs, bigo.rs) comparten este walker en vez
//! de repetir la misma cascada de matches cuatro veces con el riesgo de que
//! alguna se quede corta en algún tipo de nodo.

use rustpython_parser::ast::{self as ast, Expr, Stmt};

pub fn walk_stmts<F, G>(body: &[Stmt], on_stmt: &mut F, on_expr: &mut G)
where
    F: FnMut(&Stmt),
    G: FnMut(&Expr),
{
    for stmt in body {
        walk_stmt(stmt, on_stmt, on_expr);
    }
}

fn walk_stmt<F, G>(stmt: &Stmt, on_stmt: &mut F, on_expr: &mut G)
where
    F: FnMut(&Stmt),
    G: FnMut(&Expr),
{
    on_stmt(stmt);
    match stmt {
        Stmt::If(s) => {
            walk_expr(&s.test, on_expr);
            walk_stmts(&s.body, on_stmt, on_expr);
            walk_stmts(&s.orelse, on_stmt, on_expr);
        }
        Stmt::For(s) => {
            walk_expr(&s.target, on_expr);
            walk_expr(&s.iter, on_expr);
            walk_stmts(&s.body, on_stmt, on_expr);
            walk_stmts(&s.orelse, on_stmt, on_expr);
        }
        Stmt::AsyncFor(s) => {
            walk_expr(&s.target, on_expr);
            walk_expr(&s.iter, on_expr);
            walk_stmts(&s.body, on_stmt, on_expr);
            walk_stmts(&s.orelse, on_stmt, on_expr);
        }
        Stmt::While(s) => {
            walk_expr(&s.test, on_expr);
            walk_stmts(&s.body, on_stmt, on_expr);
            walk_stmts(&s.orelse, on_stmt, on_expr);
        }
        Stmt::With(s) => {
            for item in &s.items {
                walk_expr(&item.context_expr, on_expr);
            }
            walk_stmts(&s.body, on_stmt, on_expr);
        }
        Stmt::AsyncWith(s) => {
            for item in &s.items {
                walk_expr(&item.context_expr, on_expr);
            }
            walk_stmts(&s.body, on_stmt, on_expr);
        }
        Stmt::Try(s) => {
            walk_stmts(&s.body, on_stmt, on_expr);
            for h in &s.handlers {
                let ast::ExceptHandler::ExceptHandler(h) = h;
                walk_stmts(&h.body, on_stmt, on_expr);
            }
            walk_stmts(&s.orelse, on_stmt, on_expr);
            walk_stmts(&s.finalbody, on_stmt, on_expr);
        }
        Stmt::TryStar(s) => {
            walk_stmts(&s.body, on_stmt, on_expr);
            for h in &s.handlers {
                let ast::ExceptHandler::ExceptHandler(h) = h;
                walk_stmts(&h.body, on_stmt, on_expr);
            }
            walk_stmts(&s.orelse, on_stmt, on_expr);
            walk_stmts(&s.finalbody, on_stmt, on_expr);
        }
        Stmt::Match(s) => {
            walk_expr(&s.subject, on_expr);
            for case in &s.cases {
                walk_stmts(&case.body, on_stmt, on_expr);
            }
        }
        Stmt::FunctionDef(s) => walk_stmts(&s.body, on_stmt, on_expr),
        Stmt::AsyncFunctionDef(s) => walk_stmts(&s.body, on_stmt, on_expr),
        Stmt::ClassDef(s) => walk_stmts(&s.body, on_stmt, on_expr),
        Stmt::Assign(s) => {
            for t in &s.targets {
                walk_expr(t, on_expr);
            }
            walk_expr(&s.value, on_expr);
        }
        Stmt::AugAssign(s) => {
            walk_expr(&s.target, on_expr);
            walk_expr(&s.value, on_expr);
        }
        Stmt::AnnAssign(s) => {
            walk_expr(&s.target, on_expr);
            if let Some(v) = &s.value {
                walk_expr(v, on_expr);
            }
        }
        Stmt::Return(s) => {
            if let Some(v) = &s.value {
                walk_expr(v, on_expr);
            }
        }
        Stmt::Expr(s) => walk_expr(&s.value, on_expr),
        Stmt::Assert(s) => {
            walk_expr(&s.test, on_expr);
            if let Some(m) = &s.msg {
                walk_expr(m, on_expr);
            }
        }
        Stmt::Delete(s) => {
            for t in &s.targets {
                walk_expr(t, on_expr);
            }
        }
        Stmt::Raise(s) => {
            if let Some(e) = &s.exc {
                walk_expr(e, on_expr);
            }
            if let Some(c) = &s.cause {
                walk_expr(c, on_expr);
            }
        }
        _ => {}
    }
}

fn walk_expr<G>(expr: &Expr, on_expr: &mut G)
where
    G: FnMut(&Expr),
{
    on_expr(expr);
    match expr {
        Expr::BoolOp(e) => {
            for v in &e.values {
                walk_expr(v, on_expr);
            }
        }
        Expr::IfExp(e) => {
            walk_expr(&e.test, on_expr);
            walk_expr(&e.body, on_expr);
            walk_expr(&e.orelse, on_expr);
        }
        Expr::NamedExpr(e) => {
            walk_expr(&e.target, on_expr);
            walk_expr(&e.value, on_expr);
        }
        Expr::BinOp(e) => {
            walk_expr(&e.left, on_expr);
            walk_expr(&e.right, on_expr);
        }
        Expr::UnaryOp(e) => walk_expr(&e.operand, on_expr),
        Expr::Lambda(e) => walk_expr(&e.body, on_expr),
        Expr::Compare(e) => {
            walk_expr(&e.left, on_expr);
            for c in &e.comparators {
                walk_expr(c, on_expr);
            }
        }
        Expr::Call(e) => {
            walk_expr(&e.func, on_expr);
            for a in &e.args {
                walk_expr(a, on_expr);
            }
            for kw in &e.keywords {
                walk_expr(&kw.value, on_expr);
            }
        }
        Expr::Await(e) => walk_expr(&e.value, on_expr),
        Expr::Yield(e) => {
            if let Some(v) = &e.value {
                walk_expr(v, on_expr);
            }
        }
        Expr::YieldFrom(e) => walk_expr(&e.value, on_expr),
        Expr::Attribute(e) => walk_expr(&e.value, on_expr),
        Expr::Starred(e) => walk_expr(&e.value, on_expr),
        Expr::Subscript(e) => {
            walk_expr(&e.value, on_expr);
            walk_expr(&e.slice, on_expr);
        }
        Expr::Slice(e) => {
            if let Some(v) = &e.lower {
                walk_expr(v, on_expr);
            }
            if let Some(v) = &e.upper {
                walk_expr(v, on_expr);
            }
            if let Some(v) = &e.step {
                walk_expr(v, on_expr);
            }
        }
        Expr::List(e) => {
            for el in &e.elts {
                walk_expr(el, on_expr);
            }
        }
        Expr::Tuple(e) => {
            for el in &e.elts {
                walk_expr(el, on_expr);
            }
        }
        Expr::Set(e) => {
            for el in &e.elts {
                walk_expr(el, on_expr);
            }
        }
        Expr::Dict(e) => {
            for k in e.keys.iter().flatten() {
                walk_expr(k, on_expr);
            }
            for v in &e.values {
                walk_expr(v, on_expr);
            }
        }
        Expr::ListComp(e) => {
            walk_expr(&e.elt, on_expr);
            walk_comprehensions(&e.generators, on_expr);
        }
        Expr::SetComp(e) => {
            walk_expr(&e.elt, on_expr);
            walk_comprehensions(&e.generators, on_expr);
        }
        Expr::GeneratorExp(e) => {
            walk_expr(&e.elt, on_expr);
            walk_comprehensions(&e.generators, on_expr);
        }
        Expr::DictComp(e) => {
            walk_expr(&e.key, on_expr);
            walk_expr(&e.value, on_expr);
            walk_comprehensions(&e.generators, on_expr);
        }
        Expr::JoinedStr(e) => {
            for v in &e.values {
                walk_expr(v, on_expr);
            }
        }
        Expr::FormattedValue(e) => walk_expr(&e.value, on_expr),
        // Name/Constant son hojas — nada que recorrer.
        _ => {}
    }
}

fn walk_comprehensions<G>(generators: &[ast::Comprehension], on_expr: &mut G)
where
    G: FnMut(&Expr),
{
    for gen in generators {
        walk_expr(&gen.target, on_expr);
        walk_expr(&gen.iter, on_expr);
        for f in &gen.ifs {
            walk_expr(f, on_expr);
        }
    }
}
