//! Space complexity (Fase 13, segundo ítem) — heurística AST paralela al
//! motor de Big-O de *tiempo* en `bigo.rs`: mismo criterio general (forma del
//! loop, forma de la recursión), pero mirando qué estructuras auxiliares se
//! *crean* en vez de cuántas veces se itera. Un loop que solo acumula en un
//! escalar (`total += x`) es O(1) de espacio aunque sea O(n) de tiempo — la
//! señal que importa acá es si algo con tamaño creciente (`list`/`set`/
//! `dict`) se construye, no si hay iteración.
//!
//! Heurístico y honesto sobre sus límites, mismo estilo que el resto del CS
//! Engine: no ejecuta código, no mide allocaciones reales, no distingue una
//! comprehension con múltiples generators (`[x for row in m for x in row]`)
//! de una comprehension simple — solo detecta anidamiento real (una
//! comprehension dentro del `elt` de otra).

use rustpython_parser::ast::{Expr, Stmt};

use crate::bigo::has_binary_split;
use crate::walk::walk_stmts;

pub struct SpaceInfo {
    pub space: String,
    pub reason: String,
}

const GROWABLE_METHODS: &[&str] = &["append", "add", "update", "insert", "extend"];

/// ¿Esta llamada hace crecer una colección (`.append(...)`, `.add(...)`, …)?
fn is_growing_call(expr: &Expr) -> bool {
    matches!(expr, Expr::Call(c) if matches!(&*c.func, Expr::Attribute(a) if GROWABLE_METHODS.contains(&a.attr.as_str())))
}

/// ¿Este statement crea/hace crecer una estructura auxiliar? Cubre
/// `xs.append(...)` (llamada) y `d[key] = value` (asignación por subscript —
/// el patrón típico para construir un dict incrementalmente).
fn grows_structure(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Expr(e) => is_growing_call(&e.value),
        Stmt::Assign(s) => s.targets.iter().any(|t| matches!(t, Expr::Subscript(_))),
        Stmt::AugAssign(s) => matches!(&*s.target, Expr::Subscript(_)),
        _ => false,
    }
}

/// Profundidad máxima de anidamiento de loop en la que ocurre una operación
/// que hace crecer una estructura — separado de la profundidad de loop "para
/// tiempo" de `bigo.rs::loop_analysis`, que cuenta iteración sin importar si
/// algo se construye.
fn aux_structure_depth(body: &[Stmt]) -> u32 {
    fn walk(stmts: &[Stmt], loop_depth: u32, max_depth: &mut u32) {
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
            let d = if is_loop { loop_depth + 1 } else { loop_depth };
            if d > 0 && grows_structure(stmt) {
                *max_depth = (*max_depth).max(d);
            }
            walk(children, d, max_depth);
            walk(orelse, d, max_depth);
            if let Stmt::Try(s) = stmt {
                for h in &s.handlers {
                    let rustpython_parser::ast::ExceptHandler::ExceptHandler(h) = h;
                    walk(&h.body, d, max_depth);
                }
                walk(&s.finalbody, d, max_depth);
            }
        }
    }
    let mut max_depth = 0u32;
    walk(body, 0, &mut max_depth);
    max_depth
}

/// Profundidad de anidamiento de comprehensions (`[x for x in y]` cuenta 1;
/// `[[x for x in row] for row in m]` cuenta 2 porque la interna vive en el
/// `elt` de la externa). No distingue múltiples generators en una sola
/// comprehension — heurística deliberadamente conservadora.
fn comprehension_depth(expr: &Expr) -> u32 {
    match expr {
        Expr::ListComp(c) => 1 + comprehension_depth(&c.elt),
        Expr::SetComp(c) => 1 + comprehension_depth(&c.elt),
        Expr::GeneratorExp(c) => 1 + comprehension_depth(&c.elt),
        Expr::DictComp(c) => 1 + comprehension_depth(&c.value).max(comprehension_depth(&c.key)),
        _ => 0,
    }
}

fn max_comprehension_depth(body: &[Stmt]) -> u32 {
    let mut max_depth = 0u32;
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |expr: &Expr| {
        max_depth = max_depth.max(comprehension_depth(expr));
    };
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    max_depth
}

pub fn infer(body: &[Stmt], is_recursive: bool) -> SpaceInfo {
    let structure_depth = aux_structure_depth(body).max(max_comprehension_depth(body));
    let has_binary = is_recursive && has_binary_split(body);

    if structure_depth >= 2 {
        return SpaceInfo {
            space: "O(n\u{b2})".to_string(),
            reason: "A 2D auxiliary structure grows with n\u{b2} — a matrix built inside nested loops, or a nested comprehension".to_string(),
        };
    }
    if structure_depth == 1 {
        return SpaceInfo {
            space: "O(n)".to_string(),
            reason: "One auxiliary list/set/dict grows with the input — roughly one stored value per element processed".to_string(),
        };
    }
    if is_recursive && has_binary {
        return SpaceInfo {
            space: "O(log n)".to_string(),
            reason: "Recursion halves the problem each call — the call stack never grows past log n frames".to_string(),
        };
    }
    if is_recursive {
        return SpaceInfo {
            space: "O(n)".to_string(),
            reason: "Recursion without splitting the input — the call stack grows one frame per call, up to n deep (Python doesn't optimize tail calls)".to_string(),
        };
    }
    SpaceInfo {
        space: "O(1)".to_string(),
        reason: "No auxiliary structure grows with n, and no recursion — constant extra space beyond the input itself".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_module;
    use crate::recursion;
    use rustpython_parser::ast::Stmt;

    fn space_of(src: &str, name: &str) -> SpaceInfo {
        let suite = parse_module(src).expect("parse error");
        for stmt in &suite {
            if let Stmt::FunctionDef(f) = stmt {
                if f.name.as_str() == name {
                    let r = recursion::analyze(name, &f.body);
                    return infer(&f.body, r.is_recursive);
                }
            }
        }
        panic!("no se encontró la función {name}");
    }

    #[test]
    fn acumulador_escalar_es_o1() {
        let src = "def total_de(arr):\n    total = 0\n    for x in arr:\n        total += x\n    return total\n";
        assert_eq!(space_of(src, "total_de").space, "O(1)");
    }

    #[test]
    fn copiar_a_lista_es_on() {
        let src = "def copiar(arr):\n    out = []\n    for x in arr:\n        out.append(x)\n    return out\n";
        assert_eq!(space_of(src, "copiar").space, "O(n)");
    }

    #[test]
    fn list_comprehension_simple_es_on() {
        let src = "def duplicar(arr):\n    return [x * 2 for x in arr]\n";
        assert_eq!(space_of(src, "duplicar").space, "O(n)");
    }

    #[test]
    fn matriz_con_loops_anidados_es_on2() {
        let src = "def matriz(n):\n    m = []\n    for i in range(n):\n        row = []\n        for j in range(n):\n            row.append(0)\n        m.append(row)\n    return m\n";
        assert_eq!(space_of(src, "matriz").space, "O(n\u{b2})");
    }

    #[test]
    fn comprehension_anidada_es_on2() {
        let src = "def matriz_comp(n):\n    return [[0 for _ in range(n)] for _ in range(n)]\n";
        assert_eq!(space_of(src, "matriz_comp").space, "O(n\u{b2})");
    }

    #[test]
    fn recursion_binaria_es_ologn() {
        let src = "def binary_search(arr, lo, hi):\n    if lo >= hi:\n        return -1\n    mid = (lo + hi) // 2\n    return binary_search(arr, lo, mid - 1)\n";
        assert_eq!(space_of(src, "binary_search").space, "O(log n)");
    }

    #[test]
    fn recursion_lineal_es_on() {
        let src = "def factorial(n):\n    if n <= 1:\n        return 1\n    return n * factorial(n - 1)\n";
        assert_eq!(space_of(src, "factorial").space, "O(n)");
    }

    #[test]
    fn dict_por_subscript_es_on() {
        let src = "def contar(items):\n    counts = {}\n    for x in items:\n        counts[x] = counts.get(x, 0) + 1\n    return counts\n";
        assert_eq!(space_of(src, "contar").space, "O(n)");
    }
}
