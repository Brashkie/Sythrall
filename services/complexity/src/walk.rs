//! Recorrido genérico y exhaustivo del AST — equivalente a `ast.walk()` de
//! Python (visita cada statement y cada expresión, sin importar cuán anidado
//! esté: comprehensions, lambdas, dict literals, todo). `complexity.rs`
//! resuelve su propio recorrido inline porque solo necesita sumar un
//! contador; los módulos que necesitan buscar patrones distintos en el mismo
//! árbol (recursion.rs, classifiers.rs, bigo.rs) comparten este walker en vez
//! de repetir la misma cascada de matches cuatro veces con el riesgo de que
//! alguna se quede corta en algún tipo de nodo.

use rustpython_parser::ast::{self as ast, Expr, Stmt};

/// Tope de profundidad de anidamiento — sin esto, un archivo con expresiones
/// anidadas artificialmente (ej. `"(" * 200_000 + "1" + ")" * 200_000`, muy
/// por debajo del límite de tamaño de body que acepta axum) hace que este
/// walker recurra tanto que desborda el stack nativo del proceso. A
/// diferencia de un panic normal, un stack overflow NO es catcheable
/// (`catch_unwind` no ayuda) — tumba el proceso `complexity-engine`
/// entero, no solo el pedido que lo disparó. Pasado este tope, se deja de
/// bajar (ni se visita ni se recorre más profundo esa rama) — código real
/// jamás se acerca a 400 niveles de anidamiento, así que esto no afecta
/// ningún análisis legítimo.
const MAX_WALK_DEPTH: u32 = 400;

pub fn walk_stmts<F, G>(body: &[Stmt], on_stmt: &mut F, on_expr: &mut G)
where
    F: FnMut(&Stmt),
    G: FnMut(&Expr),
{
    walk_stmts_impl(body, on_stmt, on_expr, true, 0)
}

/// Como `walk_stmts`, pero NO desciende a los cuerpos de `FunctionDef`/
/// `AsyncFunctionDef` anidados — esas son su propio scope, con su propia
/// línea de tiempo de ejecución (pueden llamarse después, varias veces, o
/// nunca). Mezclar sus statements con el análisis lineal del scope padre
/// produce falsos positivos reales (ej. taint tracking: una función anidada
/// que reusa un nombre de variable puede filtrar taint al scope externo —
/// bug encontrado y corregido primero en `static_parser.py`, ver
/// `security.rs`). Los otros clasificadores (`classifiers.rs`, `recursion.rs`)
/// siguen usando `walk_stmts` a propósito: ahí el scope-bleed solo afecta un
/// booleano/label agregado, no un finding puntual con línea y severidad.
pub fn walk_stmts_own_scope<F, G>(body: &[Stmt], on_stmt: &mut F, on_expr: &mut G)
where
    F: FnMut(&Stmt),
    G: FnMut(&Expr),
{
    walk_stmts_impl(body, on_stmt, on_expr, false, 0)
}

fn walk_stmts_impl<F, G>(body: &[Stmt], on_stmt: &mut F, on_expr: &mut G, descend_into_funcs: bool, depth: u32)
where
    F: FnMut(&Stmt),
    G: FnMut(&Expr),
{
    if depth > MAX_WALK_DEPTH {
        return;
    }
    for stmt in body {
        walk_stmt(stmt, on_stmt, on_expr, descend_into_funcs, depth);
    }
}

fn walk_stmt<F, G>(stmt: &Stmt, on_stmt: &mut F, on_expr: &mut G, descend_into_funcs: bool, depth: u32)
where
    F: FnMut(&Stmt),
    G: FnMut(&Expr),
{
    if depth > MAX_WALK_DEPTH {
        return;
    }
    let d = depth + 1;
    on_stmt(stmt);
    match stmt {
        Stmt::If(s) => {
            walk_expr(&s.test, on_expr, d);
            walk_stmts_impl(&s.body, on_stmt, on_expr, descend_into_funcs, d);
            walk_stmts_impl(&s.orelse, on_stmt, on_expr, descend_into_funcs, d);
        }
        Stmt::For(s) => {
            walk_expr(&s.target, on_expr, d);
            walk_expr(&s.iter, on_expr, d);
            walk_stmts_impl(&s.body, on_stmt, on_expr, descend_into_funcs, d);
            walk_stmts_impl(&s.orelse, on_stmt, on_expr, descend_into_funcs, d);
        }
        Stmt::AsyncFor(s) => {
            walk_expr(&s.target, on_expr, d);
            walk_expr(&s.iter, on_expr, d);
            walk_stmts_impl(&s.body, on_stmt, on_expr, descend_into_funcs, d);
            walk_stmts_impl(&s.orelse, on_stmt, on_expr, descend_into_funcs, d);
        }
        Stmt::While(s) => {
            walk_expr(&s.test, on_expr, d);
            walk_stmts_impl(&s.body, on_stmt, on_expr, descend_into_funcs, d);
            walk_stmts_impl(&s.orelse, on_stmt, on_expr, descend_into_funcs, d);
        }
        Stmt::With(s) => {
            for item in &s.items {
                walk_expr(&item.context_expr, on_expr, d);
            }
            walk_stmts_impl(&s.body, on_stmt, on_expr, descend_into_funcs, d);
        }
        Stmt::AsyncWith(s) => {
            for item in &s.items {
                walk_expr(&item.context_expr, on_expr, d);
            }
            walk_stmts_impl(&s.body, on_stmt, on_expr, descend_into_funcs, d);
        }
        Stmt::Try(s) => {
            walk_stmts_impl(&s.body, on_stmt, on_expr, descend_into_funcs, d);
            for h in &s.handlers {
                let ast::ExceptHandler::ExceptHandler(h) = h;
                walk_stmts_impl(&h.body, on_stmt, on_expr, descend_into_funcs, d);
            }
            walk_stmts_impl(&s.orelse, on_stmt, on_expr, descend_into_funcs, d);
            walk_stmts_impl(&s.finalbody, on_stmt, on_expr, descend_into_funcs, d);
        }
        Stmt::TryStar(s) => {
            walk_stmts_impl(&s.body, on_stmt, on_expr, descend_into_funcs, d);
            for h in &s.handlers {
                let ast::ExceptHandler::ExceptHandler(h) = h;
                walk_stmts_impl(&h.body, on_stmt, on_expr, descend_into_funcs, d);
            }
            walk_stmts_impl(&s.orelse, on_stmt, on_expr, descend_into_funcs, d);
            walk_stmts_impl(&s.finalbody, on_stmt, on_expr, descend_into_funcs, d);
        }
        Stmt::Match(s) => {
            walk_expr(&s.subject, on_expr, d);
            for case in &s.cases {
                walk_stmts_impl(&case.body, on_stmt, on_expr, descend_into_funcs, d);
            }
        }
        Stmt::FunctionDef(s) => {
            if descend_into_funcs {
                walk_stmts_impl(&s.body, on_stmt, on_expr, descend_into_funcs, d);
            }
        }
        Stmt::AsyncFunctionDef(s) => {
            if descend_into_funcs {
                walk_stmts_impl(&s.body, on_stmt, on_expr, descend_into_funcs, d);
            }
        }
        Stmt::ClassDef(s) => walk_stmts_impl(&s.body, on_stmt, on_expr, descend_into_funcs, d),
        Stmt::Assign(s) => {
            for t in &s.targets {
                walk_expr(t, on_expr, d);
            }
            walk_expr(&s.value, on_expr, d);
        }
        Stmt::AugAssign(s) => {
            walk_expr(&s.target, on_expr, d);
            walk_expr(&s.value, on_expr, d);
        }
        Stmt::AnnAssign(s) => {
            walk_expr(&s.target, on_expr, d);
            if let Some(v) = &s.value {
                walk_expr(v, on_expr, d);
            }
        }
        Stmt::Return(s) => {
            if let Some(v) = &s.value {
                walk_expr(v, on_expr, d);
            }
        }
        Stmt::Expr(s) => walk_expr(&s.value, on_expr, d),
        Stmt::Assert(s) => {
            walk_expr(&s.test, on_expr, d);
            if let Some(m) = &s.msg {
                walk_expr(m, on_expr, d);
            }
        }
        Stmt::Delete(s) => {
            for t in &s.targets {
                walk_expr(t, on_expr, d);
            }
        }
        Stmt::Raise(s) => {
            if let Some(e) = &s.exc {
                walk_expr(e, on_expr, d);
            }
            if let Some(c) = &s.cause {
                walk_expr(c, on_expr, d);
            }
        }
        _ => {}
    }
}

/// Camina un único árbol de expresión, sin ningún `Stmt` alrededor — para
/// módulos que necesitan revisar "¿aparece X en algún lado de esta
/// expresión?" (ej. `recursion.rs` buscando una auto-llamada anidada dentro
/// del valor de un `return`) sin fabricar un `Stmt::Expr` sintético solo
/// para poder reusar `walk_stmts`. Mismo tope de profundidad que el resto
/// del walker (`MAX_WALK_DEPTH`, ver arriba).
pub fn walk_expr_tree<G>(expr: &Expr, on_expr: &mut G)
where
    G: FnMut(&Expr),
{
    walk_expr(expr, on_expr, 0);
}

fn walk_expr<G>(expr: &Expr, on_expr: &mut G, depth: u32)
where
    G: FnMut(&Expr),
{
    if depth > MAX_WALK_DEPTH {
        return;
    }
    let d = depth + 1;
    on_expr(expr);
    match expr {
        Expr::BoolOp(e) => {
            for v in &e.values {
                walk_expr(v, on_expr, d);
            }
        }
        Expr::IfExp(e) => {
            walk_expr(&e.test, on_expr, d);
            walk_expr(&e.body, on_expr, d);
            walk_expr(&e.orelse, on_expr, d);
        }
        Expr::NamedExpr(e) => {
            walk_expr(&e.target, on_expr, d);
            walk_expr(&e.value, on_expr, d);
        }
        Expr::BinOp(e) => {
            walk_expr(&e.left, on_expr, d);
            walk_expr(&e.right, on_expr, d);
        }
        Expr::UnaryOp(e) => walk_expr(&e.operand, on_expr, d),
        Expr::Lambda(e) => walk_expr(&e.body, on_expr, d),
        Expr::Compare(e) => {
            walk_expr(&e.left, on_expr, d);
            for c in &e.comparators {
                walk_expr(c, on_expr, d);
            }
        }
        Expr::Call(e) => {
            walk_expr(&e.func, on_expr, d);
            for a in &e.args {
                walk_expr(a, on_expr, d);
            }
            for kw in &e.keywords {
                walk_expr(&kw.value, on_expr, d);
            }
        }
        Expr::Await(e) => walk_expr(&e.value, on_expr, d),
        Expr::Yield(e) => {
            if let Some(v) = &e.value {
                walk_expr(v, on_expr, d);
            }
        }
        Expr::YieldFrom(e) => walk_expr(&e.value, on_expr, d),
        Expr::Attribute(e) => walk_expr(&e.value, on_expr, d),
        Expr::Starred(e) => walk_expr(&e.value, on_expr, d),
        Expr::Subscript(e) => {
            walk_expr(&e.value, on_expr, d);
            walk_expr(&e.slice, on_expr, d);
        }
        Expr::Slice(e) => {
            if let Some(v) = &e.lower {
                walk_expr(v, on_expr, d);
            }
            if let Some(v) = &e.upper {
                walk_expr(v, on_expr, d);
            }
            if let Some(v) = &e.step {
                walk_expr(v, on_expr, d);
            }
        }
        Expr::List(e) => {
            for el in &e.elts {
                walk_expr(el, on_expr, d);
            }
        }
        Expr::Tuple(e) => {
            for el in &e.elts {
                walk_expr(el, on_expr, d);
            }
        }
        Expr::Set(e) => {
            for el in &e.elts {
                walk_expr(el, on_expr, d);
            }
        }
        Expr::Dict(e) => {
            for k in e.keys.iter().flatten() {
                walk_expr(k, on_expr, d);
            }
            for v in &e.values {
                walk_expr(v, on_expr, d);
            }
        }
        Expr::ListComp(e) => {
            walk_expr(&e.elt, on_expr, d);
            walk_comprehensions(&e.generators, on_expr, d);
        }
        Expr::SetComp(e) => {
            walk_expr(&e.elt, on_expr, d);
            walk_comprehensions(&e.generators, on_expr, d);
        }
        Expr::GeneratorExp(e) => {
            walk_expr(&e.elt, on_expr, d);
            walk_comprehensions(&e.generators, on_expr, d);
        }
        Expr::DictComp(e) => {
            walk_expr(&e.key, on_expr, d);
            walk_expr(&e.value, on_expr, d);
            walk_comprehensions(&e.generators, on_expr, d);
        }
        Expr::JoinedStr(e) => {
            for v in &e.values {
                walk_expr(v, on_expr, d);
            }
        }
        Expr::FormattedValue(e) => walk_expr(&e.value, on_expr, d),
        // Name/Constant son hojas — nada que recorrer.
        _ => {}
    }
}

fn walk_comprehensions<G>(generators: &[ast::Comprehension], on_expr: &mut G, depth: u32)
where
    G: FnMut(&Expr),
{
    for gen in generators {
        walk_expr(&gen.target, on_expr, depth);
        walk_expr(&gen.iter, on_expr, depth);
        for f in &gen.ifs {
            walk_expr(f, on_expr, depth);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_module;

    fn walk_and_count(src: &str) -> (usize, usize) {
        let suite = parse_module(src).unwrap();
        let mut stmt_count = 0;
        let mut expr_count = 0;
        walk_stmts(&suite, &mut |_| stmt_count += 1, &mut |_| expr_count += 1);
        (stmt_count, expr_count)
    }

    #[test]
    fn deeply_nested_unaryop_no_desborda_el_stack() {
        // Regresión del hallazgo real: sin el tope de profundidad, un
        // archivo con expresiones anidadas artificialmente (acá, `not`
        // repetido) hacía que `walk_expr` recurriera sin límite y
        // desbordara el stack nativo del proceso — un stack overflow no es
        // catcheable, tumba el proceso entero, no solo este análisis. 2000
        // niveles supera ampliamente `MAX_WALK_DEPTH` (400) sin acercarse al
        // límite de recursión del parser en sí (una preocupación aparte, en
        // la crate externa `rustpython_parser`, no en este walker).
        let src = format!("x = {}True\n", "not ".repeat(2000));
        let (_, expr_count) = walk_and_count(&src);
        // No importa el número exacto (se trunca en MAX_WALK_DEPTH), solo
        // que termine sin crashear y haya visitado ALGO.
        assert!(expr_count > 0);
    }

    #[test]
    fn deeply_nested_boolop_no_desborda_el_stack() {
        let mut src = String::from("x = True");
        for _ in 0..2000 {
            src.push_str(" and True");
        }
        src.push('\n');
        let (_, expr_count) = walk_and_count(&src);
        assert!(expr_count > 0);
    }

    #[test]
    fn recorrido_normal_no_trunca_nada() {
        let src = "def f(a, b):\n    if a:\n        for x in b:\n            print(x)\n    return a + b\n";
        let (stmt_count, expr_count) = walk_and_count(src);
        assert!(stmt_count >= 4);
        assert!(expr_count >= 4);
    }

    #[test]
    fn match_stmt_se_recorre() {
        let src = "def f(x):\n    match x:\n        case 1:\n            return 1\n        case _:\n            return 0\n";
        let (stmt_count, _) = walk_and_count(src);
        assert!(stmt_count >= 3);
    }

    #[test]
    fn comprehension_se_recorre() {
        let src = "def f(items):\n    return [x for x in items if x > 0]\n";
        let (_, expr_count) = walk_and_count(src);
        assert!(expr_count >= 3);
    }

    #[test]
    fn walk_stmts_own_scope_no_desciende_a_funcion_anidada() {
        let src = "def outer():\n    x = 1\n    def inner():\n        y = 2\n    return x\n";
        let suite = parse_module(src).unwrap();
        let mut names = Vec::new();
        walk_stmts_own_scope(&suite, &mut |_| {}, &mut |e| {
            if let Expr::Name(n) = e {
                names.push(n.id.to_string());
            }
        });
        assert!(!names.contains(&"y".to_string()));
    }
}
