//! Structural smells (Fase 22, segundo ítem) — heurísticas de forma de AST,
//! mismo estilo que el resto del CS Engine: cada smell trae su umbral y su
//! razonamiento en el mensaje, nunca una etiqueta sola. Umbrales
//! convencionales de la literatura de code smells (Fowler/Martin: long
//! method ~50 LOC, large class ~15 métodos, long parameter list ~5,
//! god object combina tamaño con cantidad de estado) — no configurables
//! todavía, mismo honestidad-sobre-límites del resto del engine.
//!
//! Deliberadamente NO incluye "duplicated logic" (comparación de forma de
//! AST entre funciones) — necesita un esquema de normalización/hashing que
//! todavía no existe acá; queda para una porción siguiente, no silenciado.

use rustpython_parser::ast::{Expr, Stmt};
use serde::Serialize;

use crate::walk::walk_stmts_own_scope;

pub const LONG_FUNCTION_LOC: usize = 50;
pub const EXCESSIVE_PARAMS: usize = 5;
pub const DEEP_NESTING_DEPTH: u32 = 4;
pub const LARGE_CLASS_METHODS: usize = 15;
pub const LARGE_CLASS_LOC: usize = 300;
pub const GOD_OBJECT_METHODS: usize = 20;
pub const GOD_OBJECT_ATTRS: usize = 10;

#[derive(Serialize, Clone)]
pub struct StructuralSmell {
    pub kind: &'static str,
    pub name: String,
    pub line: usize,
    pub message: String,
}

pub fn check_long_function(name: &str, line: usize, loc: usize) -> Option<StructuralSmell> {
    if loc <= LONG_FUNCTION_LOC {
        return None;
    }
    Some(StructuralSmell {
        kind: "long_function",
        name: name.to_string(),
        line,
        message: format!(
            "{loc} lines (> {LONG_FUNCTION_LOC}) — consider splitting into smaller, focused functions"
        ),
    })
}

pub fn check_excessive_parameters(name: &str, line: usize, arg_count: usize) -> Option<StructuralSmell> {
    if arg_count <= EXCESSIVE_PARAMS {
        return None;
    }
    Some(StructuralSmell {
        kind: "excessive_parameters",
        name: name.to_string(),
        line,
        message: format!(
            "{arg_count} parameters (> {EXCESSIVE_PARAMS}) — consider grouping related parameters into an object/dataclass"
        ),
    })
}

/// Profundidad máxima de anidamiento de CUALQUIER bloque (if/for/while/with/
/// try) — a diferencia de `bigo.rs::loop_analysis`, que solo cuenta loops
/// para el Big-O de tiempo, acá un `if` anidado en otro `if` anidado en un
/// `try` también cuenta: no afecta el tiempo de ejecución, pero sí qué tan
/// difícil es leer la función.
fn max_nesting_depth(body: &[Stmt]) -> u32 {
    fn walk(stmts: &[Stmt], depth: u32, max_depth: &mut u32) {
        for stmt in stmts {
            let (is_block, children, orelse): (bool, &[Stmt], &[Stmt]) = match stmt {
                Stmt::If(s) => (true, &s.body, &s.orelse),
                Stmt::For(s) => (true, &s.body, &s.orelse),
                Stmt::AsyncFor(s) => (true, &s.body, &s.orelse),
                Stmt::While(s) => (true, &s.body, &s.orelse),
                Stmt::With(s) => (true, &s.body, &[]),
                Stmt::AsyncWith(s) => (true, &s.body, &[]),
                Stmt::Try(s) => (true, &s.body, &s.orelse),
                _ => (false, &[], &[]),
            };
            let d = if is_block { depth + 1 } else { depth };
            *max_depth = (*max_depth).max(d);
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

pub fn check_deep_nesting(name: &str, line: usize, body: &[Stmt]) -> Option<StructuralSmell> {
    let depth = max_nesting_depth(body);
    if depth <= DEEP_NESTING_DEPTH {
        return None;
    }
    Some(StructuralSmell {
        kind: "deep_nesting",
        name: name.to_string(),
        line,
        message: format!(
            "{depth} levels of nested blocks (> {DEEP_NESTING_DEPTH}) — consider extracting inner blocks into helper functions or using early returns"
        ),
    })
}

pub fn check_large_class(name: &str, line: usize, method_count: usize, loc: usize) -> Option<StructuralSmell> {
    if method_count <= LARGE_CLASS_METHODS && loc <= LARGE_CLASS_LOC {
        return None;
    }
    Some(StructuralSmell {
        kind: "large_class",
        name: name.to_string(),
        line,
        message: format!(
            "{method_count} methods, {loc} lines (thresholds: {LARGE_CLASS_METHODS} methods / {LARGE_CLASS_LOC} lines) — consider splitting responsibilities into smaller classes"
        ),
    })
}

/// Cuenta atributos únicos asignados vía `self.X = ...` en cualquier método
/// de la clase — proxy heurístico de cuánto estado mantiene la clase. No
/// distingue atributos "reales" de temporales reasignados en cada llamada,
/// ni seguir atributos heredados de una clase base.
pub fn count_self_attributes(class_body: &[Stmt]) -> usize {
    let mut attrs = std::collections::HashSet::new();
    for item in class_body {
        if let Stmt::FunctionDef(f) = item {
            let mut on_stmt = |stmt: &Stmt| {
                if let Stmt::Assign(s) = stmt {
                    for t in &s.targets {
                        if let Expr::Attribute(a) = t {
                            if matches!(&*a.value, Expr::Name(n) if n.id.as_str() == "self") {
                                attrs.insert(a.attr.to_string());
                            }
                        }
                    }
                }
            };
            let mut on_expr = |_: &Expr| {};
            walk_stmts_own_scope(&f.body, &mut on_stmt, &mut on_expr);
        }
    }
    attrs.len()
}

pub fn check_god_object(name: &str, line: usize, method_count: usize, attribute_count: usize) -> Option<StructuralSmell> {
    if method_count < GOD_OBJECT_METHODS || attribute_count < GOD_OBJECT_ATTRS {
        return None;
    }
    Some(StructuralSmell {
        kind: "god_object",
        name: name.to_string(),
        line,
        message: format!(
            "{method_count} methods and {attribute_count} attributes (thresholds: {GOD_OBJECT_METHODS}/{GOD_OBJECT_ATTRS}) — this class is likely doing too much; consider splitting by responsibility"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_module;

    /// Cuerpo de la primera función top-level del archivo.
    fn first_function_body(src: &str) -> Vec<Stmt> {
        let suite = parse_module(src).unwrap();
        for stmt in &suite {
            if let Stmt::FunctionDef(f) = stmt {
                return f.body.clone();
            }
        }
        panic!("no se encontró ninguna función en el src de prueba");
    }

    /// Cuerpo de la primera clase top-level del archivo.
    fn first_class_body(src: &str) -> Vec<Stmt> {
        let suite = parse_module(src).unwrap();
        for stmt in &suite {
            if let Stmt::ClassDef(c) = stmt {
                return c.body.clone();
            }
        }
        panic!("no se encontró ninguna clase en el src de prueba");
    }

    #[test]
    fn funcion_corta_no_dispara_long_function() {
        assert!(check_long_function("f", 1, 10).is_none());
    }

    #[test]
    fn funcion_larga_dispara_long_function() {
        let smell = check_long_function("f", 1, 51).unwrap();
        assert_eq!(smell.kind, "long_function");
    }

    #[test]
    fn pocos_parametros_no_dispara() {
        assert!(check_excessive_parameters("f", 1, 5).is_none());
    }

    #[test]
    fn muchos_parametros_dispara() {
        let smell = check_excessive_parameters("f", 1, 6).unwrap();
        assert_eq!(smell.kind, "excessive_parameters");
    }

    #[test]
    fn anidamiento_normal_no_dispara() {
        let src = "def f(x):\n    if x:\n        if x > 1:\n            return 1\n    return 0\n";
        let body = first_function_body(src);
        assert!(check_deep_nesting("f", 1, &body).is_none());
    }

    #[test]
    fn anidamiento_profundo_dispara() {
        let src = "def f(a, b, c, d, e):\n    if a:\n        if b:\n            if c:\n                if d:\n                    if e:\n                        return 1\n    return 0\n";
        let body = first_function_body(src);
        let smell = check_deep_nesting("f", 1, &body).unwrap();
        assert_eq!(smell.kind, "deep_nesting");
    }

    #[test]
    fn clase_chica_no_dispara_large_class() {
        assert!(check_large_class("C", 1, 3, 50).is_none());
    }

    #[test]
    fn clase_con_muchos_metodos_dispara_large_class() {
        let smell = check_large_class("C", 1, 16, 50).unwrap();
        assert_eq!(smell.kind, "large_class");
    }

    #[test]
    fn clase_con_pocos_atributos_no_es_god_object() {
        assert!(check_god_object("C", 1, 25, 2).is_none());
    }

    #[test]
    fn clase_con_muchos_metodos_y_atributos_es_god_object() {
        let smell = check_god_object("C", 1, 25, 12).unwrap();
        assert_eq!(smell.kind, "god_object");
    }

    #[test]
    fn cuenta_atributos_self_unicos() {
        let src = "class C:\n    def __init__(self):\n        self.a = 1\n        self.b = 2\n        self.a = 3\n";
        let body = first_class_body(src);
        assert_eq!(count_self_attributes(&body), 2);
    }
}
