//! Fase 18 — Symbol Engine: go-to-definition/find-references, portado 1:1
//! desde Python (`routers/intelligence.py::_find_definitions_python`/
//! `_find_references_python`/`_find_references_js_ts`/`_find_references_regex`)
//! a Rust. Mismo alcance que antes — por archivo, no a nivel de proyecto
//! entero (eso queda como un ítem futuro separado, no parte de este) — lo
//! único que cambia es dónde vive la lógica. JS/TS reusa `jsts::parse_js_ts`
//! para las definiciones (ya calculadas ahí desde la porción anterior de
//! esta fase, no una segunda pasada); Python reusa el mismo parser
//! (`parser::parse_module`) y el mismo walker exhaustivo (`walk::walk_stmts`,
//! equivalente a `ast.walk`) que el resto del motor ya usa, más los helpers
//! `structure::arg_names`/`docstring_of`/`node_name` en vez de reimplementar
//! esa extracción una tercera vez.
//!
//! Diferencia deliberada, no un error: los `signature` de acá se arman con
//! un slice literal del código fuente (`slice_of`) en vez de reconstruir con
//! algo equivalente a `ast.unparse()` — más simple, más fiel al código real
//! (sin normalizar comillas/espacios), y ningún test existente depende del
//! string exacto (solo de que contenga el nombre del símbolo).

use std::cell::RefCell;

use regex::Regex;
use rustpython_parser::ast::{Expr, Ranged, Stmt};
use rustpython_parser::text_size::TextRange;
use serde::Serialize;

use crate::jsts;
use crate::parser::{column_of_offset, line_of_offset, parse_module};
use crate::structure;

#[derive(Serialize, Clone)]
pub struct SymbolDef {
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub kind: &'static str,
    pub signature: String,
    pub docstring: String,
}

#[derive(Serialize, Clone)]
pub struct SymbolRef {
    pub line: usize,
    pub column: usize,
    pub kind: &'static str,
    pub preview: String,
}

fn slice_of(content: &str, range: TextRange) -> &str {
    &content[range.start().to_usize()..range.end().to_usize()]
}

fn preview(lines: &[&str], lineno: usize, max_len: usize) -> String {
    if lineno < 1 || lineno > lines.len() {
        return String::new();
    }
    let s = lines[lineno - 1].trim();
    if s.chars().count() > max_len {
        let truncated: String = s.chars().take(max_len).collect();
        format!("{truncated}…")
    } else {
        s.to_string()
    }
}

fn word_boundary_regex(symbol: &str) -> Regex {
    Regex::new(&format!(r"\b{}\b", regex::escape(symbol))).expect("símbolo escapado siempre produce un regex válido")
}

// ══════════════════════════════════════════════════════════════════════════════
//  PYTHON — go to definition
// ══════════════════════════════════════════════════════════════════════════════

pub fn find_definitions_python(content: &str, symbol: &str) -> Vec<SymbolDef> {
    let suite = match parse_module(content) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut defs: Vec<SymbolDef> = Vec::new();
    let mut on_stmt = |stmt: &Stmt| match stmt {
        Stmt::FunctionDef(f) if f.name.as_str() == symbol => {
            defs.push(function_def(content, symbol, f.range, &f.args, f.returns.as_deref(), &f.body));
        }
        Stmt::AsyncFunctionDef(f) if f.name.as_str() == symbol => {
            defs.push(function_def(content, symbol, f.range, &f.args, f.returns.as_deref(), &f.body));
        }
        Stmt::ClassDef(c) if c.name.as_str() == symbol => {
            let line = line_of_offset(content, c.range.start().to_usize());
            let bases: Vec<String> = c.bases.iter().map(structure::node_name).collect();
            let signature =
                if bases.is_empty() { format!("class {symbol}") } else { format!("class {symbol}({})", bases.join(", ")) };
            defs.push(SymbolDef {
                line,
                column: column_of_offset(content, c.range.start().to_usize()),
                end_line: line_of_offset(content, c.range.end().to_usize()),
                kind: "class",
                signature,
                docstring: structure::docstring_of(&c.body).unwrap_or_default(),
            });
        }
        Stmt::Assign(a) => {
            for target in &a.targets {
                if let Expr::Name(n) = target {
                    if n.id.as_str() == symbol {
                        let line = line_of_offset(content, a.range.start().to_usize());
                        let value_src = slice_of(content, a.value.range());
                        let truncated: String = value_src.chars().take(60).collect();
                        defs.push(SymbolDef {
                            line,
                            column: column_of_offset(content, n.range.start().to_usize()),
                            end_line: line,
                            kind: "variable",
                            signature: format!("{symbol} = {truncated}"),
                            docstring: String::new(),
                        });
                    }
                }
            }
        }
        Stmt::AnnAssign(a) => {
            if let Expr::Name(n) = a.target.as_ref() {
                if n.id.as_str() == symbol {
                    let line = line_of_offset(content, a.range.start().to_usize());
                    let ann_src = slice_of(content, a.annotation.range());
                    defs.push(SymbolDef {
                        line,
                        column: column_of_offset(content, n.range.start().to_usize()),
                        end_line: line,
                        kind: "variable",
                        signature: format!("{symbol}: {ann_src}"),
                        docstring: String::new(),
                    });
                }
            }
        }
        _ => {}
    };
    let mut on_expr = |_: &Expr| {};
    crate::walk::walk_stmts(&suite, &mut on_stmt, &mut on_expr);

    let kind_priority = |k: &str| -> u8 {
        match k {
            "function" | "method" => 0,
            "class" => 1,
            _ => 2,
        }
    };
    defs.sort_by_key(|d| (kind_priority(d.kind), d.line));
    defs
}

fn function_def(
    content: &str,
    symbol: &str,
    range: TextRange,
    args: &rustpython_parser::ast::Arguments,
    returns: Option<&Expr>,
    body: &[Stmt],
) -> SymbolDef {
    let line = line_of_offset(content, range.start().to_usize());
    let end_line = line_of_offset(content, range.end().to_usize());
    let column = column_of_offset(content, range.start().to_usize());
    // Indentado (columna > 1) == dentro de una clase == método, mismo
    // criterio que Python's `"method" if node.col_offset > 0 else "function"`.
    let kind = if column > 1 { "method" } else { "function" };
    let arg_list = structure::arg_names(args).join(", ");
    let ret = match returns {
        Some(r) => format!(" -> {}", slice_of(content, r.range())),
        None => String::new(),
    };
    SymbolDef {
        line,
        column,
        end_line,
        kind,
        signature: format!("def {symbol}({arg_list}){ret}"),
        docstring: structure::docstring_of(body).unwrap_or_default(),
    }
}

// ══════════════════════════════════════════════════════════════════════════════
//  PYTHON — find references
// ══════════════════════════════════════════════════════════════════════════════

pub fn find_references_python(content: &str, symbol: &str) -> (Vec<SymbolRef>, Option<usize>) {
    let suite = match parse_module(content) {
        Ok(s) => s,
        Err(_) => return (find_references_regex(content, symbol), None),
    };

    let lines: Vec<&str> = content.lines().collect();
    // Dos closures (`on_stmt`/`on_expr`) necesitan escribir en el mismo
    // acumulador — Rust no permite que dos closures capturen la misma
    // variable por referencia mutable a la vez, así que se comparte vía
    // `RefCell` (cada closure solo necesita una referencia inmutable al
    // `RefCell` en sí, eso sí está permitido dos veces).
    let refs: RefCell<Vec<SymbolRef>> = RefCell::new(Vec::new());
    let def_line: RefCell<Option<usize>> = RefCell::new(None);

    let mut on_stmt = |stmt: &Stmt| {
        let (name, range): (&str, TextRange) = match stmt {
            Stmt::FunctionDef(f) => (f.name.as_str(), f.range),
            Stmt::AsyncFunctionDef(f) => (f.name.as_str(), f.range),
            Stmt::ClassDef(c) => (c.name.as_str(), c.range),
            _ => return,
        };
        if name == symbol {
            let line = line_of_offset(content, range.start().to_usize());
            let column = column_of_offset(content, range.start().to_usize());
            *def_line.borrow_mut() = Some(line);
            refs.borrow_mut().push(SymbolRef { line, column, kind: "definition", preview: preview(&lines, line, 70) });
        }
    };
    let mut on_expr = |expr: &Expr| match expr {
        Expr::Name(n) if n.id.as_str() == symbol => {
            let line = line_of_offset(content, n.range.start().to_usize());
            let column = column_of_offset(content, n.range.start().to_usize());
            refs.borrow_mut().push(SymbolRef { line, column, kind: "read", preview: preview(&lines, line, 70) });
        }
        Expr::Attribute(a) if a.attr.as_str() == symbol => {
            // Mismo criterio (levemente inusual, pero fiel) que el Python
            // que reemplaza: columna del INICIO de la expresión completa
            // (`obj.attr`), línea del FINAL (relevante solo si la expresión
            // se parte en varias líneas).
            let line = line_of_offset(content, a.range.end().to_usize());
            let column = column_of_offset(content, a.range.start().to_usize());
            refs.borrow_mut().push(SymbolRef { line, column, kind: "call", preview: preview(&lines, line, 70) });
        }
        _ => {}
    };
    crate::walk::walk_stmts(&suite, &mut on_stmt, &mut on_expr);

    let mut seen: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    let mut unique: Vec<SymbolRef> = Vec::new();
    for r in refs.into_inner() {
        if seen.insert((r.line, r.column)) {
            unique.push(r);
        }
    }
    unique.sort_by_key(|r| r.line);
    (unique, def_line.into_inner())
}

/// Fallback regex — usado cuando el archivo Python no parsea (error de
/// sintaxis) y como el paso de "usos" de JS/TS (`find_references_jsts`),
/// que nunca tuvo un parser real para esta parte, solo regex con límite de
/// palabra, igual que el Python que reemplaza.
pub fn find_references_regex(content: &str, symbol: &str) -> Vec<SymbolRef> {
    let pattern = word_boundary_regex(symbol);
    let lines: Vec<&str> = content.lines().collect();
    let mut refs = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        for m in pattern.find_iter(line) {
            refs.push(SymbolRef { line: i + 1, column: m.start() + 1, kind: "use", preview: preview(&lines, i + 1, 70) });
        }
    }
    refs
}

// ══════════════════════════════════════════════════════════════════════════════
//  JS/TS — go to definition / find references
// ══════════════════════════════════════════════════════════════════════════════

pub fn find_definitions_jsts(content: &str, is_typescript: bool, symbol: &str) -> Vec<SymbolDef> {
    let parsed = jsts::parse_js_ts(content, is_typescript);
    let mut defs: Vec<SymbolDef> = Vec::new();

    for f in &parsed.functions {
        if f.name == symbol {
            defs.push(SymbolDef {
                line: f.line,
                column: 1,
                end_line: f.end_line,
                kind: "function",
                // `JsFunction` no trae nombres de argumento (ver docstring
                // de `jsts::JsFunction`) — mismo límite que ya tenía el
                // Python que reemplaza desde que ese parser se portó.
                signature: format!("function {symbol}()"),
                docstring: String::new(),
            });
        }
    }
    for c in &parsed.classes {
        if c.name == symbol {
            let extends = c.extends.as_deref().filter(|e| !e.is_empty()).map(|e| format!(" extends {e}")).unwrap_or_default();
            defs.push(SymbolDef {
                line: c.line,
                column: 1,
                end_line: c.line,
                kind: "class",
                signature: format!("class {symbol}{extends}"),
                docstring: String::new(),
            });
        }
    }
    for iface in &parsed.interfaces {
        if iface.name == symbol {
            defs.push(SymbolDef {
                line: iface.line,
                column: 1,
                end_line: iface.line,
                kind: "interface",
                signature: format!("interface {symbol}"),
                docstring: String::new(),
            });
        }
    }
    for t in &parsed.types {
        if t.name == symbol {
            defs.push(SymbolDef {
                line: t.line,
                column: 1,
                end_line: t.line,
                kind: "type",
                signature: format!("type {symbol}"),
                docstring: String::new(),
            });
        }
    }

    defs.sort_by_key(|d| d.line);
    defs
}

pub fn find_references_jsts(content: &str, is_typescript: bool, symbol: &str) -> (Vec<SymbolRef>, Option<usize>) {
    let parsed = jsts::parse_js_ts(content, is_typescript);
    let lines: Vec<&str> = content.lines().collect();
    let mut def_line: Option<usize> = None;
    let mut refs: Vec<SymbolRef> = Vec::new();

    for f in &parsed.functions {
        if f.name == symbol {
            def_line = Some(f.line);
            refs.push(SymbolRef { line: f.line, column: 1, kind: "definition", preview: preview(&lines, f.line, 70) });
        }
    }
    for c in &parsed.classes {
        if c.name == symbol {
            def_line = Some(c.line);
            refs.push(SymbolRef { line: c.line, column: 1, kind: "definition", preview: preview(&lines, c.line, 70) });
        }
    }

    let pattern = word_boundary_regex(symbol);
    for (i, line) in lines.iter().enumerate() {
        let lineno = i + 1;
        for m in pattern.find_iter(line) {
            let col = m.start() + 1;
            if refs.iter().any(|r| r.line == lineno && r.column == col) {
                continue;
            }
            refs.push(SymbolRef { line: lineno, column: col, kind: "use", preview: preview(&lines, lineno, 70) });
        }
    }

    refs.sort_by_key(|r| r.line);
    (refs, def_line)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PY_SRC: &str = r#"class DataProcessor:
    """Procesa datos."""
    def process(self, x):
        return x

def bubble_sort(arr):
    """Ordena."""
    return sorted(arr)

dp = DataProcessor()
bubble_sort([1, 2])
"#;

    #[test]
    fn definicion_de_funcion_top_level_es_kind_function() {
        let defs = find_definitions_python(PY_SRC, "bubble_sort");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].kind, "function");
        assert_eq!(defs[0].line, 6);
        assert!(defs[0].signature.contains("bubble_sort"));
        assert!(defs[0].docstring.contains("Ordena"));
    }

    #[test]
    fn definicion_de_metodo_dentro_de_clase_es_kind_method() {
        let defs = find_definitions_python(PY_SRC, "process");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].kind, "method");
    }

    #[test]
    fn definicion_de_clase_incluye_docstring() {
        let defs = find_definitions_python(PY_SRC, "DataProcessor");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].kind, "class");
        assert!(defs[0].docstring.contains("Procesa datos"));
    }

    #[test]
    fn simbolo_inexistente_da_lista_vacia() {
        assert!(find_definitions_python(PY_SRC, "nunca_existio_xyz").is_empty());
    }

    #[test]
    fn error_de_sintaxis_no_rompe_devuelve_vacio() {
        assert!(find_definitions_python("def broken(\n  pass", "broken").is_empty());
    }

    #[test]
    fn variable_de_modulo_detectada_como_variable() {
        let src = "x = 42\n";
        let defs = find_definitions_python(src, "x");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].kind, "variable");
        assert!(defs[0].signature.contains('4'));
    }

    #[test]
    fn referencias_encuentra_definicion_y_uso() {
        let (refs, def_line) = find_references_python(PY_SRC, "bubble_sort");
        assert_eq!(def_line, Some(6));
        let kinds: Vec<&str> = refs.iter().map(|r| r.kind).collect();
        assert!(kinds.contains(&"definition"));
        assert!(refs.len() >= 2);
    }

    #[test]
    fn referencias_metodo_incluye_kind_call() {
        let (refs, _) = find_references_python(PY_SRC, "process");
        let kinds: Vec<&str> = refs.iter().map(|r| r.kind).collect();
        assert!(kinds.contains(&"definition"));
    }

    #[test]
    fn referencias_ordenadas_por_linea() {
        let (refs, _) = find_references_python(PY_SRC, "bubble_sort");
        let lines: Vec<usize> = refs.iter().map(|r| r.line).collect();
        let mut sorted = lines.clone();
        sorted.sort();
        assert_eq!(lines, sorted);
    }

    #[test]
    fn referencias_con_error_de_sintaxis_usa_fallback_regex() {
        let (refs, def_line) = find_references_python("def broken(\n  pass\nbroken()\n", "broken");
        assert_eq!(def_line, None);
        assert!(!refs.is_empty());
        assert!(refs.iter().all(|r| r.kind == "use"));
    }

    const TS_SRC: &str = r#"export interface User {
    id: number;
}

export class UserService {
    load() {}
}

function processUsers(users: User[]) {
    return users;
}

const svc = new UserService();
processUsers([]);
"#;

    #[test]
    fn jsts_definicion_de_funcion() {
        let defs = find_definitions_jsts(TS_SRC, true, "processUsers");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].kind, "function");
    }

    #[test]
    fn jsts_definicion_de_clase() {
        let defs = find_definitions_jsts(TS_SRC, true, "UserService");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].kind, "class");
    }

    #[test]
    fn jsts_definicion_de_interface() {
        let defs = find_definitions_jsts(TS_SRC, true, "User");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].kind, "interface");
    }

    #[test]
    fn jsts_referencias_encuentra_uso_y_definicion() {
        let (refs, def_line) = find_references_jsts(TS_SRC, true, "processUsers");
        assert!(def_line.is_some());
        assert!(refs.len() >= 2);
    }

    #[test]
    fn jsts_simbolo_inexistente_definicion_vacia() {
        assert!(find_definitions_jsts(TS_SRC, true, "NoExisteXYZ").is_empty());
    }
}
