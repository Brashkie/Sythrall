//! Los clasificadores del CS Engine — los primeros 3 son puerto 1:1 de
//! `_regex_info_python`, `_grammar_info_python`, `_graph_traversal_info_python`
//! (y sus `_note`) en `static_parser.py`. Mismas heurísticas, mismos
//! umbrales — ver ese archivo para el razonamiento completo de cada una;
//! acá solo la traducción. El cuarto (`semantic_analysis_info`, Fase 16)
//! nació directamente en Rust — no tiene equivalente Python previo.

use std::collections::HashSet;

use rustpython_parser::ast::{CmpOp, Expr, Stmt};

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
    /// Fase 15 (Mathematical Intelligence), primer ítem: Conjuntos y
    /// relaciones. `true` cuando el/los nombres que dispararon `has_visited`
    /// (`visited`/`seen`/`explored`) se pudieron confirmar como un `set()`
    /// real (constructor `set(...)` o un literal `{...}` no vacío) — no
    /// solo el nombre de la variable. La afirmación de O(V+E) de
    /// `graph_note` depende de que la membresía (`in`) sea O(1); eso solo
    /// es cierto si `visited` de verdad es un conjunto (hash lookup), no
    /// una lista (donde `in` es O(n) — el mismo caso que ya marca
    /// `smells::check_quadratic_list_membership` como anti-patrón). Cuando
    /// no se puede confirmar, la nota lo dice honestamente en vez de
    /// asumir que el nombre implica el tipo.
    pub visited_is_confirmed_set: bool,
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

/// `true` si alguna asignación a un nombre (case-insensitive) en `wanted`
/// tiene como valor un `set()`/`frozenset()` o un literal `{...}` no vacío
/// (`Expr::Set` — un `{}` vacío es un dict en Python, nunca matchea acá,
/// pero un `visited`/`seen` inicializado vacío casi siempre se escribe
/// `set()` de todas formas, así que no es una pérdida real de cobertura).
fn assigned_as_set_literal_or_constructor(body: &[Stmt], wanted: &[&str]) -> bool {
    let mut confirmed = false;
    let mut on_stmt = |stmt: &Stmt| {
        if confirmed {
            return;
        }
        let Stmt::Assign(s) = stmt else { return };
        let target_matches = s.targets.iter().any(|t| matches!(t, Expr::Name(n) if wanted.contains(&n.id.to_lowercase().as_str())));
        if !target_matches {
            return;
        }
        let is_set_value = match &*s.value {
            Expr::Set(_) => true,
            Expr::Call(c) => matches!(&*c.func, Expr::Name(n) if matches!(n.id.as_str(), "set" | "frozenset")),
            _ => false,
        };
        if is_set_value {
            confirmed = true;
        }
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    confirmed
}

pub fn graph_info(body: &[Stmt], is_recursive: bool) -> GraphInfo {
    let assigned = assigned_names(body);
    let has_indegree = ["in_degree", "indegree", "in_deg"].iter().any(|k| assigned.contains(*k));
    let has_visited = ["visited", "seen", "explored"].iter().any(|k| assigned.contains(*k));

    if has_indegree {
        return GraphInfo {
            traversal_kind: Some("Topological Sort (Kahn's algorithm)".to_string()),
            visited_is_confirmed_set: false,
        };
    }
    if has_visited {
        let visited_is_confirmed_set = assigned_as_set_literal_or_constructor(body, &["visited", "seen", "explored"]);
        if !names_with_calls(body, &["popleft"]).is_empty() {
            return GraphInfo {
                traversal_kind: Some("BFS".to_string()),
                visited_is_confirmed_set,
            };
        }
        let has_stack = !names_with_calls(body, &["append", "pop"]).is_empty();
        if is_recursive || has_stack {
            return GraphInfo {
                traversal_kind: Some("DFS".to_string()),
                visited_is_confirmed_set,
            };
        }
    }
    GraphInfo { traversal_kind: None, visited_is_confirmed_set: false }
}

pub fn graph_note(info: &GraphInfo) -> Option<String> {
    let kind = info.traversal_kind.as_ref()?;
    if info.visited_is_confirmed_set {
        return Some(format!(
            "{kind} detected — O(V+E): 'visited' is a real set() here, so membership testing ('in') is O(1) via hash lookup \
             (the set-theoretic \u{2208} relation, backed by a hash table instead of a scan) — that O(1) lookup is exactly what \
             keeps this traversal O(V+E) instead of O(V\u{b2}); the same reasoning that flags a list-backed 'visited' \
             (checked with 'in' and grown with .append()) as a hidden O(n\u{b2})."
        ));
    }
    Some(format!(
        "{kind} detected (variable-name heuristic: 'visited'/'seen'/'explored') — O(V+E) assumes membership testing is O(1), \
         which only holds if that tracker is really a set()/dict, not a list; couldn't confirm the type here, so take the \
         complexity claim as the common case, not a proven one."
    ))
}

// ─── Semantic analysis pattern → Chomsky Type-1 (informal) ──────────────────
//
// Fase 16 (Formal Language Intelligence): a diferencia de Type-3/Type-2
// (regex y grammar/parser-shaped code, arriba), NO existe una forma honesta
// de afirmar "esta función procesa un lenguaje context-sensitive" desde
// forma de AST sola — esa clasificación es una propiedad del LENGUAJE
// completo que algo reconoce, no de una función aislada. Lo que sí es
// honesto: reconocer el patrón clásico que los compiladores reales agregan
// ENCIMA de un parseo libre de contexto — una tabla de símbolos que CRECE
// (`table[name] = ...`) y funciona como gate de una validación de
// "declarado antes de usar"/"no puede declararse dos veces", RECHAZANDO
// (con un `raise`) ante la violación. Por eso el nombre del campo/badge dice
// "informal": es reconocer un patrón con teoría real detrás, no una
// clasificación formal probada.
//
// El punto crítico que evita confundir esto con memoización (que chequea
// un dict de la MISMA forma: `if key not in cache: cache[key] = compute()`)
// es el `raise`: memoización nunca levanta al no encontrar algo, siempre
// calcula un valor de reemplazo. Un chequeo de contexto real SÍ rechaza.

pub struct SemanticAnalysisInfo {
    pub is_semantic_analysis_shaped: bool,
}

/// Nombres asignados desde un dict vacío (`{}`) o `dict()` — candidatos a
/// "tabla de símbolos". Deliberadamente solo el caso vacío: un dict
/// pre-poblado con un literal es más una tabla de lookup estática
/// (Type-2/lookup-shaped), no algo que "crece" durante el análisis.
fn empty_dict_vars(body: &[Stmt]) -> HashSet<String> {
    let mut vars = HashSet::new();
    let mut on_stmt = |stmt: &Stmt| {
        if let Stmt::Assign(s) = stmt {
            let is_empty_dict = match &*s.value {
                Expr::Dict(d) => d.keys.is_empty(),
                Expr::Call(c) => matches!(&*c.func, Expr::Name(n) if n.id.as_str() == "dict"),
                _ => false,
            };
            if is_empty_dict {
                for t in &s.targets {
                    if let Expr::Name(n) = t {
                        vars.insert(n.id.to_string());
                    }
                }
            }
        }
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    vars
}

/// `true` si `var` recibe una asignación por subíndice (`var[k] = ...`) en
/// algún lado del cuerpo — la tabla "crece" en vez de ser solo consultada.
fn grows_via_subscript_assign(body: &[Stmt], var: &str) -> bool {
    let mut found = false;
    let mut on_stmt = |stmt: &Stmt| {
        if let Stmt::Assign(s) = stmt {
            for t in &s.targets {
                if let Expr::Subscript(sub) = t {
                    if matches!(&*sub.value, Expr::Name(n) if n.id.as_str() == var) {
                        found = true;
                    }
                }
            }
        }
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    found
}

fn tests_membership_of(test: &Expr, var: &str) -> bool {
    let Expr::Compare(c) = test else { return false };
    let targets_var = c.comparators.iter().any(|comp| matches!(comp, Expr::Name(n) if n.id.as_str() == var));
    let has_in_op = c.ops.iter().any(|op| matches!(op, CmpOp::In | CmpOp::NotIn));
    targets_var && has_in_op
}

fn contains_raise(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| match s {
        Stmt::Raise(_) => true,
        Stmt::If(s2) => contains_raise(&s2.body) || contains_raise(&s2.orelse),
        _ => false,
    })
}

/// `true` si hay un `if <var> in/not in <dict>:` cuya rama (el `body`, o el
/// `orelse` — cubre tanto `if not in: raise` como `if in: ... else: raise`)
/// levanta una excepción. Este es el gate de validación real, lo que
/// distingue el patrón de memoización (mismo chequeo de dict, nunca levanta).
fn has_membership_gated_raise(body: &[Stmt], var: &str) -> bool {
    let mut found = false;
    let mut on_stmt = |stmt: &Stmt| {
        if found {
            return;
        }
        if let Stmt::If(s) = stmt {
            if tests_membership_of(&s.test, var) && (contains_raise(&s.body) || contains_raise(&s.orelse)) {
                found = true;
            }
        }
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    found
}

pub fn semantic_analysis_info(body: &[Stmt]) -> SemanticAnalysisInfo {
    for var in empty_dict_vars(body) {
        if grows_via_subscript_assign(body, &var) && has_membership_gated_raise(body, &var) {
            return SemanticAnalysisInfo { is_semantic_analysis_shaped: true };
        }
    }
    SemanticAnalysisInfo { is_semantic_analysis_shaped: false }
}

pub fn semantic_analysis_note(info: &SemanticAnalysisInfo) -> Option<String> {
    if !info.is_semantic_analysis_shaped {
        return None;
    }
    Some(
        "Resembles the classic semantic-analysis pattern compilers add on top of a context-free \
         parse — a symbol table that grows (assignments into a dict) and gates a decision on \
         whether something is already/not-yet declared, rejecting (raising) on violation. \
         Informally 'Type-1 (Context-Sensitive)' territory: a constraint like declared-before-use \
         needs context a pure CFG/pushdown-automaton parse can't express on its own. Not a formal \
         classification — that's a property of the whole grammar a program recognizes, not one \
         function's shape — just naming a real pattern, not proving one."
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_module;

    fn first_function_body(src: &str) -> Vec<Stmt> {
        let suite = parse_module(src).unwrap();
        for stmt in &suite {
            if let Stmt::FunctionDef(f) = stmt {
                return f.body.clone();
            }
        }
        panic!("no se encontró ninguna función en el src de prueba");
    }

    #[test]
    fn visited_confirmado_como_set_constructor() {
        let src = "def bfs(graph, start):\n    visited = set()\n    visited.add(start)\n    while True:\n        if start in visited:\n            break\n";
        let body = first_function_body(src);
        assert!(assigned_as_set_literal_or_constructor(&body, &["visited", "seen", "explored"]));
    }

    #[test]
    fn visited_confirmado_como_set_literal() {
        let src = "def dfs(graph, start):\n    seen = {start}\n    return seen\n";
        let body = first_function_body(src);
        assert!(assigned_as_set_literal_or_constructor(&body, &["visited", "seen", "explored"]));
    }

    #[test]
    fn visited_como_lista_no_se_confirma_como_set() {
        let src = "def dfs(graph, start):\n    seen = []\n    seen.append(start)\n    return seen\n";
        let body = first_function_body(src);
        assert!(!assigned_as_set_literal_or_constructor(&body, &["visited", "seen", "explored"]));
    }

    #[test]
    fn visited_solo_referenciado_sin_asignacion_no_se_confirma() {
        let src = "def dfs(graph, seen):\n    return seen\n";
        let body = first_function_body(src);
        assert!(!assigned_as_set_literal_or_constructor(&body, &["visited", "seen", "explored"]));
    }

    #[test]
    fn graph_note_con_set_confirmado_menciona_hash_lookup() {
        let info = GraphInfo { traversal_kind: Some("BFS".to_string()), visited_is_confirmed_set: true };
        let note = graph_note(&info).unwrap();
        assert!(note.contains("hash lookup"));
        assert!(note.contains("O(1)"));
    }

    #[test]
    fn graph_note_sin_set_confirmado_es_honesto_sobre_la_incertidumbre() {
        let info = GraphInfo { traversal_kind: Some("DFS".to_string()), visited_is_confirmed_set: false };
        let note = graph_note(&info).unwrap();
        assert!(note.contains("couldn't confirm"));
    }

    #[test]
    fn graph_note_sin_traversal_kind_es_none() {
        let info = GraphInfo { traversal_kind: None, visited_is_confirmed_set: false };
        assert!(graph_note(&info).is_none());
    }

    #[test]
    fn bfs_end_to_end_con_set_real_confirma_membresia_o1() {
        let src = "from collections import deque\ndef bfs(graph, start):\n    visited = set([start])\n    queue = deque([start])\n    while queue:\n        node = queue.popleft()\n        for n in graph[node]:\n            if n not in visited:\n                visited.add(n)\n                queue.append(n)\n";
        let body = first_function_body(src);
        let info = graph_info(&body, false);
        assert_eq!(info.traversal_kind.as_deref(), Some("BFS"));
        assert!(info.visited_is_confirmed_set);
    }

    #[test]
    fn dfs_end_to_end_con_visited_como_lista_no_confirma_set() {
        let src = "def dfs(graph, start):\n    visited = []\n    stack = [start]\n    while stack:\n        node = stack.pop()\n        if node not in visited:\n            visited.append(node)\n            stack.append(node)\n";
        let body = first_function_body(src);
        let info = graph_info(&body, false);
        assert_eq!(info.traversal_kind.as_deref(), Some("DFS"));
        assert!(!info.visited_is_confirmed_set);
    }

    // ─── Fase 16: semantic_analysis (Type-1, informal) ─────────────────────

    #[test]
    fn tabla_de_simbolos_con_declarado_antes_de_usar_dispara() {
        let src = "def resolve(names):\n    table = {}\n    for name, is_decl in names:\n        if is_decl:\n            table[name] = True\n        elif name not in table:\n            raise NameError(name)\n";
        let body = first_function_body(src);
        assert!(semantic_analysis_info(&body).is_semantic_analysis_shaped);
    }

    #[test]
    fn redeclaracion_prohibida_tambien_dispara() {
        let src = "def declare(names):\n    table = {}\n    for name in names:\n        if name in table:\n            raise ValueError('ya declarado')\n        table[name] = True\n";
        let body = first_function_body(src);
        assert!(semantic_analysis_info(&body).is_semantic_analysis_shaped);
    }

    #[test]
    fn memoizacion_no_dispara_semantic_analysis() {
        // Misma forma (chequear un dict, y si falta, calcular y guardar) —
        // la diferencia crítica es que memoización NUNCA levanta.
        let src = "def fib(n, cache={}):\n    if n in cache:\n        return cache[n]\n    result = n if n < 2 else fib(n - 1) + fib(n - 2)\n    cache[n] = result\n    return result\n";
        let body = first_function_body(src);
        assert!(!semantic_analysis_info(&body).is_semantic_analysis_shaped);
    }

    #[test]
    fn dict_que_solo_se_lee_sin_crecer_no_dispara() {
        let src = "def check(names, table):\n    for name in names:\n        if name not in table:\n            raise NameError(name)\n";
        let body = first_function_body(src);
        // `table` viene como parámetro, no se asigna desde `{}`/`dict()` acá
        // dentro — no es candidato (mismo criterio conservador de siempre:
        // solo lo que se puede confirmar).
        assert!(!semantic_analysis_info(&body).is_semantic_analysis_shaped);
    }

    #[test]
    fn tabla_prepoblada_con_literal_no_es_candidata() {
        let src = "def f(name):\n    table = {'a': 1}\n    table[name] = 2\n    if name not in table:\n        raise KeyError(name)\n";
        let body = first_function_body(src);
        assert!(!semantic_analysis_info(&body).is_semantic_analysis_shaped);
    }

    #[test]
    fn semantic_analysis_note_es_none_sin_shape() {
        assert!(semantic_analysis_note(&SemanticAnalysisInfo { is_semantic_analysis_shaped: false }).is_none());
    }

    #[test]
    fn semantic_analysis_note_es_cauteloso_no_una_afirmacion_dura() {
        let note = semantic_analysis_note(&SemanticAnalysisInfo { is_semantic_analysis_shaped: true }).unwrap();
        assert!(note.contains("Resembles"));
        assert!(note.contains("Not a formal classification"));
    }
}
