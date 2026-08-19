//! Extracción de clases e imports — puerto 1:1 de las secciones "Imports" y
//! "Funciones y clases" de `_parse_python` en `static_parser.py` (líneas
//! 88-180), más los helpers genéricos de AST (`_decorator_name`, `_node_name`,
//! `_extract_calls_python`) que necesita `rich.rs` para armar el shape
//! completo por función.

use rustpython_parser::ast::{Constant, Expr, Stmt};
use serde::Serialize;

use crate::parser::line_of_offset;
use crate::smells;
use crate::walk::walk_stmts;

#[derive(Serialize, Clone)]
pub struct RichImport {
    pub module: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub line: usize,
}

#[derive(Serialize, Clone)]
pub struct RichMethod {
    pub name: String,
    pub line: usize,
    pub args: Vec<String>,
    pub is_async: bool,
}

#[derive(Serialize, Clone)]
pub struct RichClass {
    pub name: String,
    pub line: usize,
    pub end_line: usize,
    pub loc: usize,
    pub bases: Vec<String>,
    pub methods: Vec<RichMethod>,
    pub decorators: Vec<String>,
    pub docstring: Option<String>,
    /// Atributos únicos asignados vía `self.X = ...` en cualquier método —
    /// proxy de cuánto estado mantiene la clase, usado por el check de
    /// god object (Fase 22, `smells.rs`).
    pub attribute_count: usize,
}

pub fn extract_imports(source: &str, suite: &[Stmt]) -> Vec<RichImport> {
    let mut out = Vec::new();
    let mut on_stmt = |stmt: &Stmt| match stmt {
        Stmt::Import(s) => {
            for alias in &s.names {
                out.push(RichImport {
                    module: alias.name.to_string(),
                    name: None,
                    alias: alias.asname.as_ref().map(|a| a.to_string()),
                    kind: "import",
                    line: line_of_offset(source, s.range.start().to_usize()),
                });
            }
        }
        Stmt::ImportFrom(s) => {
            let module = s.module.as_ref().map(|m| m.to_string()).unwrap_or_default();
            for alias in &s.names {
                out.push(RichImport {
                    module: module.clone(),
                    name: Some(alias.name.to_string()),
                    alias: alias.asname.as_ref().map(|a| a.to_string()),
                    kind: "from_import",
                    line: line_of_offset(source, s.range.start().to_usize()),
                });
            }
        }
        _ => {}
    };
    // Imports viven al nivel que sea (top-level la gran mayoría, pero
    // `ast.walk` de Python los encuentra en cualquier profundidad) — se
    // camina todo el árbol, no solo el nivel superior.
    let mut on_expr = |_: &Expr| {};
    walk_stmts(suite, &mut on_stmt, &mut on_expr);
    out
}

pub fn extract_classes(source: &str, suite: &[Stmt]) -> Vec<RichClass> {
    let mut out = Vec::new();
    for stmt in suite {
        collect_classes(source, stmt, &mut out);
    }
    out
}

fn collect_classes(source: &str, stmt: &Stmt, out: &mut Vec<RichClass>) {
    if let Stmt::ClassDef(c) = stmt {
        let methods = c
            .body
            .iter()
            .filter_map(|item| match item {
                Stmt::FunctionDef(f) => Some(RichMethod {
                    name: f.name.to_string(),
                    line: line_of_offset(source, f.range.start().to_usize()),
                    args: arg_names(&f.args),
                    is_async: false,
                }),
                Stmt::AsyncFunctionDef(f) => Some(RichMethod {
                    name: f.name.to_string(),
                    line: line_of_offset(source, f.range.start().to_usize()),
                    args: arg_names(&f.args),
                    is_async: true,
                }),
                _ => None,
            })
            .collect();

        let line = line_of_offset(source, c.range.start().to_usize());
        let end_line = line_of_offset(source, c.range.end().to_usize());
        out.push(RichClass {
            name: c.name.to_string(),
            line,
            end_line,
            loc: end_line.saturating_sub(line) + 1,
            bases: c.bases.iter().map(node_name).collect(),
            methods,
            decorators: c.decorator_list.iter().map(decorator_name).collect(),
            docstring: docstring_of(&c.body),
            attribute_count: smells::count_self_attributes(&c.body),
        });
    }
    // Clases anidadas adentro de funciones/otras clases también cuentan,
    // igual que `ast.walk` las encontraría todas.
    for child in nested_bodies(stmt) {
        for s in child {
            collect_classes(source, s, out);
        }
    }
}

fn nested_bodies(stmt: &Stmt) -> Vec<&[Stmt]> {
    match stmt {
        Stmt::FunctionDef(s) => vec![&s.body],
        Stmt::AsyncFunctionDef(s) => vec![&s.body],
        Stmt::If(s) => vec![&s.body, &s.orelse],
        Stmt::For(s) => vec![&s.body, &s.orelse],
        Stmt::AsyncFor(s) => vec![&s.body, &s.orelse],
        Stmt::While(s) => vec![&s.body, &s.orelse],
        Stmt::With(s) => vec![&s.body],
        Stmt::AsyncWith(s) => vec![&s.body],
        Stmt::Try(s) => vec![&s.body, &s.orelse, &s.finalbody],
        _ => vec![],
    }
}

pub fn arg_names(args: &rustpython_parser::ast::Arguments) -> Vec<String> {
    args.args.iter().map(|a| a.def.arg.to_string()).collect()
}

pub fn docstring_of(body: &[Stmt]) -> Option<String> {
    let first = body.first()?;
    if let Stmt::Expr(e) = first {
        if let Expr::Constant(c) = &*e.value {
            if let Constant::Str(s) = &c.value {
                return Some(s.to_string());
            }
        }
    }
    None
}

pub fn decorator_name(expr: &Expr) -> String {
    match expr {
        Expr::Name(n) => n.id.to_string(),
        Expr::Attribute(a) => format!("{}.{}", node_name(&a.value), a.attr),
        Expr::Call(c) => decorator_name(&c.func),
        _ => "?".to_string(),
    }
}

pub fn node_name(expr: &Expr) -> String {
    match expr {
        Expr::Name(n) => n.id.to_string(),
        Expr::Attribute(a) => format!("{}.{}", node_name(&a.value), a.attr),
        _ => "?".to_string(),
    }
}

pub fn extract_calls(body: &[Stmt]) -> Vec<String> {
    let mut calls = std::collections::HashSet::new();
    let mut on_expr = |expr: &Expr| {
        if let Expr::Call(c) = expr {
            match &*c.func {
                Expr::Name(n) => {
                    calls.insert(n.id.to_string());
                }
                Expr::Attribute(a) => {
                    calls.insert(a.attr.to_string());
                }
                _ => {}
            }
        }
    };
    let mut on_stmt = |_: &Stmt| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    calls.into_iter().collect()
}
