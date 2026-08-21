//! Naming smells (Fase 22, tercer ítem) — heurísticas de nombres, mismo
//! espíritu conservador que Structural Smells (smells.rs): solo casos
//! mecánicamente verificables (nombre de una letra fuera de un loop o
//! comprehension, mezcla de snake_case/camelCase en el mismo archivo, un
//! nombre interno que tapa uno de un scope que lo contiene) — nunca un
//! juicio de "nombre poco claro", que necesitaría un LLM para arbitrar.

use std::collections::HashSet;

use rustpython_parser::ast::{Comprehension, Expr, Stmt};
use serde::Serialize;

use crate::parser::line_of_offset;
use crate::structure;
use crate::walk::{walk_stmts, walk_stmts_own_scope};

const CASING_IGNORE: [&str; 2] = ["self", "cls"];

#[derive(Serialize, Clone)]
pub struct NamingSmell {
    pub kind: &'static str,
    pub name: String,
    pub line: usize,
    pub message: String,
}

/// Recorre un target de asignación (Name / Tuple / List / Starred, para
/// cubrir unpacking) y llama `on_name` por cada `Name` que liga.
fn walk_target_names(expr: &Expr, source: &str, on_name: &mut dyn FnMut(&str, usize)) {
    match expr {
        Expr::Name(n) => on_name(n.id.as_str(), line_of_offset(source, n.range.start().to_usize())),
        Expr::Tuple(t) => {
            for el in &t.elts {
                walk_target_names(el, source, on_name);
            }
        }
        Expr::List(l) => {
            for el in &l.elts {
                walk_target_names(el, source, on_name);
            }
        }
        Expr::Starred(s) => walk_target_names(&s.value, source, on_name),
        _ => {}
    }
}

/// (nombre, línea) de todo target de `for`/comprehension — quedan exentos
/// del chequeo de una letra porque su alcance es chico y el significado
/// suele ser obvio por contexto (`for i in range(n)`).
fn collect_loop_target_names(source: &str, suite: &[Stmt]) -> HashSet<(String, usize)> {
    let mut exempt = HashSet::new();
    {
        let mut on_stmt = |stmt: &Stmt| {
            let target: Option<&Expr> = match stmt {
                Stmt::For(s) => Some(&s.target),
                Stmt::AsyncFor(s) => Some(&s.target),
                _ => None,
            };
            if let Some(t) = target {
                walk_target_names(t, source, &mut |name, line| {
                    exempt.insert((name.to_string(), line));
                });
            }
        };
        let mut noop_expr = |_: &Expr| {};
        walk_stmts(suite, &mut on_stmt, &mut noop_expr);
    }
    {
        let mut noop_stmt = |_: &Stmt| {};
        let mut on_expr = |expr: &Expr| {
            let gens: Option<&[Comprehension]> = match expr {
                Expr::ListComp(e) => Some(&e.generators),
                Expr::SetComp(e) => Some(&e.generators),
                Expr::GeneratorExp(e) => Some(&e.generators),
                Expr::DictComp(e) => Some(&e.generators),
                _ => None,
            };
            if let Some(gens) = gens {
                for gen in gens {
                    walk_target_names(&gen.target, source, &mut |name, line| {
                        exempt.insert((name.to_string(), line));
                    });
                }
            }
        };
        walk_stmts(suite, &mut noop_stmt, &mut on_expr);
    }
    exempt
}

fn check_single_letter_name(name: &str, line: usize) -> Option<NamingSmell> {
    let mut chars = name.chars();
    let first = chars.next()?;
    if chars.next().is_some() || !first.is_alphabetic() {
        return None;
    }
    Some(NamingSmell {
        kind: "single_letter_name",
        name: name.to_string(),
        line,
        message: format!(
            "single-letter variable ('{name}') outside a loop/comprehension — consider a name that describes what it represents"
        ),
    })
}

/// Deliberadamente NO incluye parámetros de función: `def add(a, b)` es
/// idiomático en helpers matemáticos/genéricos y el nombre está a la vista
/// al lado de la firma — marcarlo sería ruido, no señal. El chequeo se
/// enfoca en variables asignadas dentro del cuerpo.
fn check_single_letter_smells(source: &str, suite: &[Stmt], exempt: &HashSet<(String, usize)>) -> Vec<NamingSmell> {
    let mut smells = Vec::new();
    let mut flagged: HashSet<(String, usize)> = HashSet::new();
    {
        let mut on_stmt = |stmt: &Stmt| {
            let targets: Vec<&Expr> = match stmt {
                Stmt::Assign(s) => s.targets.iter().collect(),
                Stmt::AnnAssign(s) => vec![s.target.as_ref()],
                _ => Vec::new(),
            };
            for t in targets {
                walk_target_names(t, source, &mut |name, line| {
                    let key = (name.to_string(), line);
                    if exempt.contains(&key) || flagged.contains(&key) {
                        return;
                    }
                    if let Some(smell) = check_single_letter_name(name, line) {
                        flagged.insert(key);
                        smells.push(smell);
                    }
                });
            }
        };
        let mut noop_expr = |_: &Expr| {};
        walk_stmts(suite, &mut on_stmt, &mut noop_expr);
    }
    smells
}

/// Clasifica un identificador como snake_case/camelCase, o None si es
/// ambiguo (todo minúscula sin guión, dunder, etc.) — en ese caso no aporta
/// señal para el chequeo de mezcla de convenciones.
fn classify_casing(name: &str) -> Option<&'static str> {
    if name.starts_with("__") || name.chars().count() <= 1 {
        return None;
    }
    let core = name.trim_matches('_');
    if core.is_empty() {
        return None;
    }
    if core.contains('_') {
        return if core == core.to_lowercase() { Some("snake_case") } else { None };
    }
    let first = name.chars().next().unwrap();
    if first.is_lowercase() && name != name.to_lowercase() {
        return Some("camelCase");
    }
    None
}

fn check_inconsistent_casing(suite: &[Stmt]) -> Vec<NamingSmell> {
    let mut snake_examples: Vec<String> = Vec::new();
    let mut camel_examples: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    {
        let mut consider = |name: &str| {
            if seen.contains(name) || CASING_IGNORE.contains(&name) {
                return;
            }
            seen.insert(name.to_string());
            match classify_casing(name) {
                Some("snake_case") if snake_examples.len() < 3 => snake_examples.push(name.to_string()),
                Some("camelCase") if camel_examples.len() < 3 => camel_examples.push(name.to_string()),
                _ => {}
            }
        };
        let mut on_stmt = |stmt: &Stmt| match stmt {
            Stmt::FunctionDef(f) => {
                consider(&f.name);
                for name in structure::arg_names(&f.args) {
                    consider(&name);
                }
            }
            Stmt::AsyncFunctionDef(f) => {
                consider(&f.name);
                for name in structure::arg_names(&f.args) {
                    consider(&name);
                }
            }
            Stmt::Assign(s) => {
                for t in &s.targets {
                    if let Expr::Name(n) = t {
                        consider(n.id.as_str());
                    }
                }
            }
            _ => {}
        };
        let mut noop_expr = |_: &Expr| {};
        walk_stmts(suite, &mut on_stmt, &mut noop_expr);
    }

    if snake_examples.is_empty() || camel_examples.is_empty() {
        return Vec::new();
    }
    vec![NamingSmell {
        kind: "inconsistent_casing",
        name: "file".to_string(),
        line: 1,
        message: format!(
            "mixes snake_case ({}) and camelCase ({}) in the same file — pick a single convention",
            snake_examples.join(", "),
            camel_examples.join(", ")
        ),
    }]
}

/// Nombres ligados en el scope PROPIO de una función: parámetros + targets
/// de asignación directos, sin bajar a funciones anidadas (mismo límite que
/// `walk_stmts_own_scope`).
fn own_bound_names(args: &rustpython_parser::ast::Arguments, body: &[Stmt]) -> HashSet<String> {
    let mut names: HashSet<String> = structure::arg_names(args).into_iter().collect();
    for a in &args.posonlyargs {
        names.insert(a.def.arg.to_string());
    }
    for a in &args.kwonlyargs {
        names.insert(a.def.arg.to_string());
    }
    if let Some(v) = &args.vararg {
        names.insert(v.arg.to_string());
    }
    if let Some(k) = &args.kwarg {
        names.insert(k.arg.to_string());
    }
    {
        let mut on_stmt = |stmt: &Stmt| match stmt {
            Stmt::Assign(s) => {
                for t in &s.targets {
                    if let Expr::Name(n) = t {
                        names.insert(n.id.to_string());
                    }
                }
            }
            Stmt::AnnAssign(s) => {
                if let Expr::Name(n) = s.target.as_ref() {
                    names.insert(n.id.to_string());
                }
            }
            Stmt::For(s) => {
                if let Expr::Name(n) = s.target.as_ref() {
                    names.insert(n.id.to_string());
                }
            }
            Stmt::AsyncFor(s) => {
                if let Expr::Name(n) = s.target.as_ref() {
                    names.insert(n.id.to_string());
                }
            }
            _ => {}
        };
        let mut noop_expr = |_: &Expr| {};
        walk_stmts_own_scope(body, &mut on_stmt, &mut noop_expr);
    }
    names
}

fn module_level_names(suite: &[Stmt]) -> HashSet<String> {
    let mut names = HashSet::new();
    for stmt in suite {
        match stmt {
            Stmt::Assign(s) => {
                for t in &s.targets {
                    if let Expr::Name(n) = t {
                        names.insert(n.id.to_string());
                    }
                }
            }
            Stmt::AnnAssign(s) => {
                if let Expr::Name(n) = s.target.as_ref() {
                    names.insert(n.id.to_string());
                }
            }
            _ => {}
        }
    }
    names
}

/// Recorrida recursiva y consciente del anidamiento (a diferencia de
/// `walk_stmts`, que aplana todo en un solo callback) que arrastra el set de
/// nombres de scopes que la contienen — una función anidada que liga un
/// nombre ya usado en un scope externo (parámetro/local de la función que la
/// contiene, o global del módulo) queda marcada porque lo tapa, la misma
/// trampa clásica de Python de reusar un nombre sin darse cuenta.
fn visit_for_shadowing(source: &str, body: &[Stmt], enclosing: &HashSet<String>, out: &mut Vec<NamingSmell>) {
    for stmt in body {
        match stmt {
            Stmt::FunctionDef(f) => {
                shadow_check_and_recurse(source, &f.name, f.range, &f.args, &f.body, enclosing, out);
            }
            Stmt::AsyncFunctionDef(f) => {
                shadow_check_and_recurse(source, &f.name, f.range, &f.args, &f.body, enclosing, out);
            }
            Stmt::ClassDef(c) => visit_for_shadowing(source, &c.body, enclosing, out),
            Stmt::If(s) => {
                visit_for_shadowing(source, &s.body, enclosing, out);
                visit_for_shadowing(source, &s.orelse, enclosing, out);
            }
            Stmt::For(s) => {
                visit_for_shadowing(source, &s.body, enclosing, out);
                visit_for_shadowing(source, &s.orelse, enclosing, out);
            }
            Stmt::AsyncFor(s) => {
                visit_for_shadowing(source, &s.body, enclosing, out);
                visit_for_shadowing(source, &s.orelse, enclosing, out);
            }
            Stmt::While(s) => {
                visit_for_shadowing(source, &s.body, enclosing, out);
                visit_for_shadowing(source, &s.orelse, enclosing, out);
            }
            Stmt::With(s) => visit_for_shadowing(source, &s.body, enclosing, out),
            Stmt::AsyncWith(s) => visit_for_shadowing(source, &s.body, enclosing, out),
            Stmt::Try(s) => {
                visit_for_shadowing(source, &s.body, enclosing, out);
                for h in &s.handlers {
                    let rustpython_parser::ast::ExceptHandler::ExceptHandler(h) = h;
                    visit_for_shadowing(source, &h.body, enclosing, out);
                }
                visit_for_shadowing(source, &s.orelse, enclosing, out);
                visit_for_shadowing(source, &s.finalbody, enclosing, out);
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn shadow_check_and_recurse(
    source: &str,
    name: &str,
    range: rustpython_parser::text_size::TextRange,
    args: &rustpython_parser::ast::Arguments,
    body: &[Stmt],
    enclosing: &HashSet<String>,
    out: &mut Vec<NamingSmell>,
) {
    let own = own_bound_names(args, body);
    let mut shadowed: Vec<&String> = own.intersection(enclosing).collect();
    shadowed.sort();
    if let Some(first) = shadowed.first() {
        out.push(NamingSmell {
            kind: "shadowed_name",
            name: name.to_string(),
            line: line_of_offset(source, range.start().to_usize()),
            message: format!(
                "'{first}' shadows a name from an enclosing scope — consider renaming to avoid confusion"
            ),
        });
    }
    let merged: HashSet<String> = enclosing.union(&own).cloned().collect();
    visit_for_shadowing(source, body, &merged, out);
}

fn check_shadowed_names(source: &str, suite: &[Stmt]) -> Vec<NamingSmell> {
    let mut smells = Vec::new();
    let module_names = module_level_names(suite);
    visit_for_shadowing(source, suite, &module_names, &mut smells);
    smells
}

pub fn check_naming_smells(source: &str, suite: &[Stmt]) -> Vec<NamingSmell> {
    let exempt = collect_loop_target_names(source, suite);
    let mut smells = check_single_letter_smells(source, suite, &exempt);
    smells.extend(check_inconsistent_casing(suite));
    smells.extend(check_shadowed_names(source, suite));
    smells
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_module;

    fn smells(src: &str) -> Vec<NamingSmell> {
        let suite = parse_module(src).unwrap();
        check_naming_smells(src, &suite)
    }

    #[test]
    fn single_letter_variable_flagged() {
        let s = smells("def f():\n    x = compute()\n    return x\n");
        assert!(s.iter().any(|s| s.kind == "single_letter_name" && s.name == "x"));
    }

    #[test]
    fn for_loop_target_not_flagged() {
        let s = smells("def f(arr):\n    for i in range(len(arr)):\n        arr[i] += 1\n    return arr\n");
        assert!(!s.iter().any(|s| s.kind == "single_letter_name"));
    }

    #[test]
    fn comprehension_target_not_flagged() {
        let s = smells("def f(arr):\n    return [x * 2 for x in arr]\n");
        assert!(!s.iter().any(|s| s.kind == "single_letter_name"));
    }

    #[test]
    fn single_letter_parameter_not_flagged() {
        let s = smells("def add(a, b):\n    return a + b\n");
        assert!(!s.iter().any(|s| s.kind == "single_letter_name"));
    }

    #[test]
    fn mixed_casing_flagged() {
        let s = smells("def do_thing(value):\n    return value\n\ndef doOtherThing(value):\n    return value\n");
        assert!(s.iter().any(|s| s.kind == "inconsistent_casing"));
    }

    #[test]
    fn only_snake_case_not_flagged() {
        let s = smells("def do_thing(value):\n    return value\n\ndef do_other_thing(value):\n    return value\n");
        assert!(!s.iter().any(|s| s.kind == "inconsistent_casing"));
    }

    #[test]
    fn nested_function_shadows_outer_local() {
        let s = smells(
            "def outer():\n    total = 0\n    def inner():\n        total = 1\n        return total\n    return inner()\n",
        );
        assert!(s.iter().any(|s| s.kind == "shadowed_name" && s.name == "inner"));
    }

    #[test]
    fn nested_function_shadows_module_global() {
        let s = smells("config = {}\n\ndef load(config):\n    return config\n");
        assert!(s.iter().any(|s| s.kind == "shadowed_name" && s.name == "load"));
    }

    #[test]
    fn no_shadowing_not_flagged() {
        let s = smells(
            "def outer():\n    total = 0\n    def inner():\n        count = 1\n        return count\n    return inner()\n",
        );
        assert!(!s.iter().any(|s| s.kind == "shadowed_name"));
    }

    #[test]
    fn clean_code_has_no_smells() {
        let s = smells("def add(a, b):\n    return a + b\n");
        assert!(s.is_empty());
    }
}
