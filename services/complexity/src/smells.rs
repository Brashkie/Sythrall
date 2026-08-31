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

use rustpython_parser::ast::{BoolOp, CmpOp, Expr, Stmt, UnaryOp};
use serde::Serialize;

use crate::parser::line_of_offset;
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

/// Nombres únicos de atributos asignados vía `self.X = ...` en cualquier
/// método de la clase — proxy heurístico de cuánto estado mantiene la clase.
/// No distingue atributos "reales" de temporales reasignados en cada
/// llamada, ni sigue atributos heredados de una clase base. Devuelve un
/// `Vec` en vez de un `HashSet` a propósito: preserva el orden de primera
/// aparición (determinístico, no depende del seed aleatorio de hashing de
/// cada corrida) — importa desde que `apps/api/routers/diagram.py` empezó a
/// mostrar estos nombres directamente en un diagrama (antes solo se usaba
/// `.len()` para el check de god object, donde el orden nunca importó).
pub fn self_attribute_names(class_body: &[Stmt]) -> Vec<String> {
    let mut attrs: Vec<String> = Vec::new();
    for item in class_body {
        if let Stmt::FunctionDef(f) = item {
            let mut on_stmt = |stmt: &Stmt| {
                if let Stmt::Assign(s) = stmt {
                    for t in &s.targets {
                        if let Expr::Attribute(a) = t {
                            if matches!(&*a.value, Expr::Name(n) if n.id.as_str() == "self") {
                                let name = a.attr.to_string();
                                if !attrs.contains(&name) {
                                    attrs.push(name);
                                }
                            }
                        }
                    }
                }
            };
            let mut on_expr = |_: &Expr| {};
            walk_stmts_own_scope(&f.body, &mut on_stmt, &mut on_expr);
        }
    }
    attrs
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

/// Variables asignadas como lista vacía literal (`x = []`) en cualquier
/// punto del cuerpo — candidatas a acumulador tipo "seen list". No sigue
/// reasignaciones posteriores (si `x` se reasigna como dict más adelante,
/// igual queda como candidata) — mismo nivel de heurística honesta que el
/// resto del engine, no un análisis de tipos real.
fn empty_list_vars(body: &[Stmt]) -> std::collections::HashSet<String> {
    let mut vars = std::collections::HashSet::new();
    let mut on_stmt = |stmt: &Stmt| {
        if let Stmt::Assign(s) = stmt {
            if let Expr::List(l) = &*s.value {
                if l.elts.is_empty() {
                    for t in &s.targets {
                        if let Expr::Name(n) = t {
                            vars.insert(n.id.to_string());
                        }
                    }
                }
            }
        }
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
    vars
}

/// Recolecta, en UN SOLO recorrido del cuerpo del loop, los nombres que
/// aparecen del lado derecho de un `in` (chequeo de membresía) y los que
/// reciben un `.append(...)` — antes esto era una función que solo
/// contestaba "¿`var` hace las dos cosas?" y `check_quadratic_list_membership`
/// la llamaba una vez POR CANDIDATO (`list_vars`), recorriendo el mismo
/// `loop_body` V veces para V candidatos en vez de una sola; acá se
/// recolectan ambos conjuntos de una pasada y el caller solo intersecta.
fn membership_and_append_vars(loop_body: &[Stmt]) -> (std::collections::HashSet<String>, std::collections::HashSet<String>) {
    let mut has_membership = std::collections::HashSet::new();
    let mut has_append = std::collections::HashSet::new();
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |e: &Expr| {
        if let Expr::Compare(c) = e {
            if c.ops.iter().any(|op| matches!(op, CmpOp::In)) {
                for comp in &c.comparators {
                    if let Expr::Name(n) = comp {
                        has_membership.insert(n.id.to_string());
                    }
                }
            }
        }
        if let Expr::Call(call) = e {
            if let Expr::Attribute(a) = &*call.func {
                if a.attr.as_str() == "append" {
                    if let Expr::Name(n) = &*a.value {
                        has_append.insert(n.id.to_string());
                    }
                }
            }
        }
    };
    walk_stmts_own_scope(loop_body, &mut on_stmt, &mut on_expr);
    (has_membership, has_append)
}

/// Recolecta todos los `for`/`async for` del cuerpo, a cualquier profundidad
/// (descendiendo por if/while/with/try, igual que `max_nesting_depth` de
/// arriba), junto con su línea y su propio cuerpo.
fn collect_for_loops<'a>(stmts: &'a [Stmt], source: &str, out: &mut Vec<(usize, &'a [Stmt])>) {
    for stmt in stmts {
        match stmt {
            Stmt::For(s) => {
                out.push((line_of_offset(source, s.range.start().to_usize()), &s.body));
                collect_for_loops(&s.body, source, out);
            }
            Stmt::AsyncFor(s) => {
                out.push((line_of_offset(source, s.range.start().to_usize()), &s.body));
                collect_for_loops(&s.body, source, out);
            }
            Stmt::If(s) => {
                collect_for_loops(&s.body, source, out);
                collect_for_loops(&s.orelse, source, out);
            }
            Stmt::While(s) => {
                collect_for_loops(&s.body, source, out);
                collect_for_loops(&s.orelse, source, out);
            }
            Stmt::With(s) => collect_for_loops(&s.body, source, out),
            Stmt::AsyncWith(s) => collect_for_loops(&s.body, source, out),
            Stmt::Try(s) => {
                collect_for_loops(&s.body, source, out);
                for h in &s.handlers {
                    let rustpython_parser::ast::ExceptHandler::ExceptHandler(h) = h;
                    collect_for_loops(&h.body, source, out);
                }
                collect_for_loops(&s.orelse, source, out);
                collect_for_loops(&s.finalbody, source, out);
            }
            _ => {}
        }
    }
}

/// Fase 14, último bullet ("nested-structure interaction detection"),
/// acotado al caso concreto de mayor valor y menor riesgo de falso
/// positivo: un acumulador inicializado como lista vacía (`seen = []`),
/// chequeado por membresía (`in`) Y ampliado (`.append(...)`) dentro del
/// mismo loop — el smell clásico que debería resolverse con un `set()`.
/// Cada chequeo de membresía en una lista es O(n); hacerlo una vez por
/// iteración de un loop que además la hace crecer es un O(n²) oculto —
/// exactamente el tipo de interacción *entre* estructuras que este ítem del
/// roadmap pide, no complejidad de una función aislada. No necesita
/// inferencia de tipos: alcanza con que la variable se haya inicializado
/// como lista vacía literal, así no hay falsos positivos sobre un dict/set
/// (donde `in` ya es O(1) — no hay nada que arreglar ahí).
pub fn check_quadratic_list_membership(name: &str, source: &str, body: &[Stmt]) -> Vec<StructuralSmell> {
    let list_vars = empty_list_vars(body);
    if list_vars.is_empty() {
        return Vec::new();
    }
    let mut for_loops = Vec::new();
    collect_for_loops(body, source, &mut for_loops);

    let mut out = Vec::new();
    for (line, loop_body) in for_loops {
        let (membership_vars, append_vars) = membership_and_append_vars(loop_body);
        for var in &list_vars {
            if membership_vars.contains(var) && append_vars.contains(var) {
                out.push(StructuralSmell {
                    kind: "quadratic_list_membership",
                    name: name.to_string(),
                    line,
                    message: format!(
                        "'{var}' is checked with 'in' and grown with .append() in the same loop — each membership check on a list is O(n), making this loop a hidden O(n\u{b2}) traversal; use a set() instead for O(1) membership checks"
                    ),
                });
            }
        }
    }
    out
}

/// Fase 15 (Mathematical Intelligence), primer ítem: Boolean algebra — De
/// Morgan. `not (a and b)` es lógicamente equivalente a `(not a) or (not b)`
/// (y viceversa para `or`) — la fase es explícita sobre el alcance: una nota
/// de legibilidad, no una reescritura automática, así que acá solo se
/// detecta y explica, nunca se transforma el código. Dispara solo con
/// exactamente 2 operandos (`a and b`, no `a and b and c`) — con 3+ la frase
/// "simplificado" dejaría de ser una única aplicación limpia de la ley y
/// pasaría a describir algo más confuso que el original.
pub fn check_de_morgan_simplifiable(name: &str, source: &str, body: &[Stmt]) -> Vec<StructuralSmell> {
    let mut out = Vec::new();
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |expr: &Expr| {
        let Expr::UnaryOp(u) = expr else { return };
        if !matches!(u.op, UnaryOp::Not) {
            return;
        }
        let Expr::BoolOp(b) = u.operand.as_ref() else { return };
        if b.values.len() != 2 {
            return;
        }
        let (conn, other) = match b.op {
            BoolOp::And => ("and", "or"),
            BoolOp::Or => ("or", "and"),
        };
        out.push(StructuralSmell {
            kind: "de_morgan_simplifiable",
            name: name.to_string(),
            line: line_of_offset(source, u.range.start().to_usize()),
            message: format!(
                "'not (a {conn} b)' is logically equivalent to '(not a) {other} (not b)' (De Morgan's law) — may read clearer depending on context"
            ),
        });
    };
    walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
    out
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
        assert_eq!(self_attribute_names(&body), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn atributos_self_preservan_orden_de_primera_aparicion() {
        let src = "class C:\n    def __init__(self):\n        self.z = 1\n        self.a = 2\n    def other(self):\n        self.m = 3\n";
        let body = first_class_body(src);
        assert_eq!(self_attribute_names(&body), vec!["z".to_string(), "a".to_string(), "m".to_string()]);
    }

    #[test]
    fn seen_list_con_membresia_y_append_dispara_quadratic() {
        let src = "def f(items):\n    seen = []\n    for item in items:\n        if item in seen:\n            continue\n        seen.append(item)\n";
        let body = first_function_body(src);
        let smells = check_quadratic_list_membership("f", src, &body);
        assert_eq!(smells.len(), 1);
        assert_eq!(smells[0].kind, "quadratic_list_membership");
    }

    #[test]
    fn membresia_sin_append_no_dispara() {
        let src = "def f(items):\n    allowed = []\n    for item in items:\n        if item in allowed:\n            continue\n    return items\n";
        let body = first_function_body(src);
        assert!(check_quadratic_list_membership("f", src, &body).is_empty());
    }

    #[test]
    fn append_sin_chequeo_de_membresia_no_dispara() {
        let src = "def f(items):\n    seen = []\n    for item in items:\n        seen.append(item)\n    return seen\n";
        let body = first_function_body(src);
        assert!(check_quadratic_list_membership("f", src, &body).is_empty());
    }

    #[test]
    fn set_en_vez_de_lista_no_dispara() {
        let src = "def f(items):\n    seen = set()\n    for item in items:\n        if item in seen:\n            continue\n        seen.add(item)\n";
        let body = first_function_body(src);
        assert!(check_quadratic_list_membership("f", src, &body).is_empty());
    }

    #[test]
    fn sin_ninguna_lista_vacia_no_dispara() {
        let src = "def f(items):\n    for item in items:\n        print(item)\n";
        let body = first_function_body(src);
        assert!(check_quadratic_list_membership("f", src, &body).is_empty());
    }

    #[test]
    fn dos_candidatas_solo_una_dispara_sin_contaminacion_cruzada() {
        // Regresión de la optimización de un solo walk por loop en vez de
        // uno por candidato: `seen` hace el idiom completo (in + append),
        // `other` solo hace append — el fix no debe hacer que `other`
        // "contagie" un falso positivo, ni que se pierda el de `seen`.
        let src = "def f(items):\n    seen = []\n    other = []\n    for item in items:\n        if item in seen:\n            continue\n        seen.append(item)\n        other.append(item)\n";
        let body = first_function_body(src);
        let smells = check_quadratic_list_membership("f", src, &body);
        assert_eq!(smells.len(), 1);
        assert!(smells[0].message.contains("'seen'"));
    }

    #[test]
    fn not_and_de_2_operandos_dispara_de_morgan() {
        let src = "def f(a, b):\n    return not (a and b)\n";
        let body = first_function_body(src);
        let smells = check_de_morgan_simplifiable("f", src, &body);
        assert_eq!(smells.len(), 1);
        assert_eq!(smells[0].kind, "de_morgan_simplifiable");
        assert!(smells[0].message.contains("and"));
        assert!(smells[0].message.contains("or"));
    }

    #[test]
    fn not_or_de_2_operandos_dispara_de_morgan() {
        let src = "def f(a, b):\n    return not (a or b)\n";
        let body = first_function_body(src);
        let smells = check_de_morgan_simplifiable("f", src, &body);
        assert_eq!(smells.len(), 1);
        assert!(smells[0].message.contains("(not a) and (not b)"));
    }

    #[test]
    fn not_and_de_3_operandos_no_dispara() {
        let src = "def f(a, b, c):\n    return not (a and b and c)\n";
        let body = first_function_body(src);
        assert!(check_de_morgan_simplifiable("f", src, &body).is_empty());
    }

    #[test]
    fn and_sin_not_no_dispara() {
        let src = "def f(a, b):\n    return a and b\n";
        let body = first_function_body(src);
        assert!(check_de_morgan_simplifiable("f", src, &body).is_empty());
    }

    #[test]
    fn not_sobre_algo_que_no_es_boolop_no_dispara() {
        let src = "def f(a):\n    return not a\n";
        let body = first_function_body(src);
        assert!(check_de_morgan_simplifiable("f", src, &body).is_empty());
    }

    #[test]
    fn de_morgan_anidado_dentro_de_condicional_tambien_se_detecta() {
        let src = "def f(a, b):\n    if not (a and b):\n        return 1\n    return 0\n";
        let body = first_function_body(src);
        assert_eq!(check_de_morgan_simplifiable("f", src, &body).len(), 1);
    }
}
