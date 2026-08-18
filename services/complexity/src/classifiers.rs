//! Los 3 clasificadores del CS Engine — puerto 1:1 de `_regex_info_python`,
//! `_grammar_info_python`, `_graph_traversal_info_python` (y sus `_note`) en
//! `static_parser.py`. Mismas heurísticas, mismos umbrales — ver ese archivo
//! para el razonamiento completo de cada una; acá solo la traducción.

use std::collections::HashSet;

use rustpython_parser::ast::{Expr, Stmt};

use crate::walk::walk_stmts;

// ─── Regex → Chomsky Type-3 ─────────────────────────────────────────────────

const REGEX_METHODS: &[&str] = &["compile", "match", "fullmatch", "search", "findall", "finditer", "sub", "subn", "split"];

pub struct RegexInfo {
    pub uses_regex: bool,
}

/// Solo detecta llamadas directas `re.XXX(...)` — no rastrea un `re.Pattern`
/// guardado en variable, esa parte necesitaría rastrear asignaciones.
pub fn regex_info(body: &[Stmt]) -> RegexInfo {
    let mut count = 0u32;
    let mut on_expr = |expr: &Expr| {
        if let Expr::Call(c) = expr {
            if let Expr::Attribute(a) = &*c.func {
                if REGEX_METHODS.contains(&a.attr.as_str()) {
                    if let Expr::Name(n) = &*a.value {
                        if n.id.as_str() == "re" {
                            count += 1;
                        }
                    }
                }
            }
        }
    };
    let mut on_stmt = |_: &Stmt| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    RegexInfo { uses_regex: count > 0 }
}

pub fn regex_note(info: &RegexInfo) -> Option<String> {
    if !info.uses_regex {
        return None;
    }
    Some(
        "Regex detected — Chomsky Type-3 (regular language), recognized by a finite \
         automaton. Python's `re` engine is backtracking, not a pure DFA: most patterns \
         are linear, but ambiguous/nested patterns can degrade to O(2^n) worst case \
         (catastrophic backtracking)."
            .to_string(),
    )
}

// ─── Grammar/parser-shaped → Chomsky Type-2 ─────────────────────────────────

const GRAMMAR_NAME_KEYWORDS: &[&str] = &["parse", "parser", "grammar", "tokenize", "lexer", "lex_", "ast_"];

pub struct GrammarInfo {
    pub is_grammar_shaped: bool,
}

/// Nombres de variable local sobre los que se llamó alguno de `method_names`
/// (ej. {"append","pop"} para pila explícita, {"popleft"} para cola).
fn names_with_calls(body: &[Stmt], method_names: &[&str]) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut on_expr = |expr: &Expr| {
        if let Expr::Call(c) = expr {
            if let Expr::Attribute(a) = &*c.func {
                if method_names.contains(&a.attr.as_str()) {
                    if let Expr::Name(n) = &*a.value {
                        names.insert(n.id.to_string());
                    }
                }
            }
        }
    };
    let mut on_stmt = |_: &Stmt| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    names
}

pub fn grammar_info(name: &str, body: &[Stmt], is_recursive: bool) -> GrammarInfo {
    let name_match = GRAMMAR_NAME_KEYWORDS.iter().any(|kw| name.to_lowercase().contains(kw));
    if !name_match {
        return GrammarInfo { is_grammar_shaped: false };
    }
    let has_stack = !names_with_calls(body, &["append", "pop"]).is_empty();
    GrammarInfo {
        is_grammar_shaped: is_recursive || has_stack,
    }
}

pub fn grammar_note(info: &GrammarInfo) -> Option<String> {
    if !info.is_grammar_shaped {
        return None;
    }
    Some(
        "Name + recursion/stack pattern suggest parsing code — Chomsky Type-2 \
         (context-free grammar), recognized by a pushdown automaton. Heuristic based \
         on naming and code shape, not semantic analysis — may have false positives/negatives."
            .to_string(),
    )
}

// ─── Graph traversal → BFS/DFS/Topological Sort ─────────────────────────────

pub struct GraphInfo {
    pub traversal_kind: Option<String>,
}

fn assigned_names(body: &[Stmt]) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut on_stmt = |stmt: &Stmt| match stmt {
        Stmt::Assign(s) => {
            for t in &s.targets {
                if let Expr::Name(n) = t {
                    names.insert(n.id.to_lowercase());
                }
            }
        }
        Stmt::AnnAssign(s) => {
            if let Expr::Name(n) = &*s.target {
                names.insert(n.id.to_lowercase());
            }
        }
        _ => {}
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    names
}

pub fn graph_info(body: &[Stmt], is_recursive: bool) -> GraphInfo {
    let assigned = assigned_names(body);
    let has_indegree = ["in_degree", "indegree", "in_deg"].iter().any(|k| assigned.contains(*k));
    let has_visited = ["visited", "seen", "explored"].iter().any(|k| assigned.contains(*k));

    if has_indegree {
        return GraphInfo {
            traversal_kind: Some("Topological Sort (Kahn's algorithm)".to_string()),
        };
    }
    if has_visited {
        if !names_with_calls(body, &["popleft"]).is_empty() {
            return GraphInfo {
                traversal_kind: Some("BFS".to_string()),
            };
        }
        let has_stack = !names_with_calls(body, &["append", "pop"]).is_empty();
        if is_recursive || has_stack {
            return GraphInfo {
                traversal_kind: Some("DFS".to_string()),
            };
        }
    }
    GraphInfo { traversal_kind: None }
}

pub fn graph_note(info: &GraphInfo) -> Option<String> {
    let kind = info.traversal_kind.as_ref()?;
    Some(format!(
        "{kind} detected (variable-name heuristic) — O(V+E): every node and edge is visited a constant number of times."
    ))
}
