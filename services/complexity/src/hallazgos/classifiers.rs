//! Los clasificadores del CS Engine — los primeros 3 son puerto 1:1 de
//! `_regex_info_python`, `_grammar_info_python`, `_graph_traversal_info_python`
//! (y sus `_note`) en `static_parser.py`. Mismas heurísticas, mismos
//! umbrales — ver ese archivo para el razonamiento completo de cada una;
//! acá solo la traducción. El cuarto (`semantic_analysis_info`, Fase 16)
//! nació directamente en Rust — no tiene equivalente Python previo.

use std::collections::HashSet;

use rustpython_parser::ast::{CmpOp, Constant, Expr, Operator, Stmt};

use crate::ast::walk::walk_stmts;

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

// ─── Sorting shape → bubble sort ─────────────────────────────────────────────
//
// Fase 26 (Algorithm Validation Engine), 2026-09-01 — primer detector de
// FORMA de algoritmo para el auto-match kernel↔función (más allá de
// comparar el Big-O ya calculado, que solo confirma el EXPONENTE, no la
// forma real): reconoce específicamente bubble sort — 2+ loops anidados
// cuyo loop más interno compara dos elementos ADYACENTES de la misma
// secuencia (`seq[j] > seq[j+1]`) y, de cumplirse, los intercambia. Señal
// de FORMA, no verifica que la implementación esté libre de bugs (ej. no
// confirma que el loop externo realmente reduzca el rango del interno en
// cada pasada, ni que el resultado final quede ordenado) — mismo criterio
// que `numerical_algorithm_note` (Fase 20, Fortran) ya establece para
// matrix multiply: "esto tiene la FORMA de X", no "esto ES X, verificado".
//
// Deliberadamente acotado a esta primera porción: solo reconoce el idiom
// pythónico de intercambio en una línea (`seq[j], seq[j+1] = seq[j+1], seq[j]`)
// — el idiom de 3 líneas con variable temporal (`tmp = seq[j]; seq[j] =
// seq[j+1]; seq[j+1] = tmp`) queda fuera de alcance acá, mismo criterio de
// "primera porción angosta" que otros patrones de este motor ya aceptaron
// (ej. `sizeof_pointer_allocation` con solo `malloc` en su primera versión).

pub struct SortingInfo {
    pub algorithm: Option<&'static str>,
}

fn loop_body(stmt: &Stmt) -> Option<&[Stmt]> {
    match stmt {
        Stmt::For(s) => Some(&s.body),
        Stmt::AsyncFor(s) => Some(&s.body),
        Stmt::While(s) => Some(&s.body),
        _ => None,
    }
}

fn index_is_name(e: &Expr, want: &str) -> bool {
    matches!(e, Expr::Name(n) if n.id.as_str() == want)
}

/// `true` si `e` es exactamente `want + 1` (`j + 1`, no `1 + j` ni
/// `j + 2` — deliberadamente literal, mismo criterio angosto del resto de
/// este detector).
fn index_is_name_plus_one(e: &Expr, want: &str) -> bool {
    let Expr::BinOp(b) = e else { return false };
    matches!(b.op, Operator::Add)
        && index_is_name(&b.left, want)
        && matches!(&*b.right, Expr::Constant(c) if matches!(&c.value, Constant::Int(i) if i == &1i64.into()))
}

/// `(nombre_secuencia, nombre_índice)` si `e` es `seq[idx]` con `idx` un
/// identificador simple — `arr[i+1]` como IZQUIERDA de una comparación no
/// es el caso que este detector busca (eso sería la resta, no la suma).
fn subscript_by_name(e: &Expr) -> Option<(&str, &str)> {
    let Expr::Subscript(s) = e else { return None };
    let Expr::Name(seq) = &*s.value else { return None };
    let Expr::Name(idx) = &*s.slice else { return None };
    Some((seq.id.as_str(), idx.id.as_str()))
}

/// `seq[idx]` donde `idx` es exactamente `base + 1` (`arr[j+1]`).
fn subscript_by_name_plus_one<'a>(e: &'a Expr, base: &str) -> Option<&'a str> {
    let Expr::Subscript(s) = e else { return None };
    let Expr::Name(seq) = &*s.value else { return None };
    index_is_name_plus_one(&s.slice, base).then_some(seq.id.as_str())
}

/// `if seq[j] > seq[j+1]:` — devuelve `(seq, j)` si el test matchea esa
/// forma exacta (un solo operador `>`, ambos lados subíndices de la misma
/// secuencia, el de la derecha exactamente uno más que el de la izquierda).
fn adjacent_gt_compare(test: &Expr) -> Option<(&str, &str)> {
    let Expr::Compare(c) = test else { return None };
    if c.ops.len() != 1 || !matches!(c.ops[0], CmpOp::Gt) {
        return None;
    }
    let (seq, idx) = subscript_by_name(&c.left)?;
    let seq2 = subscript_by_name_plus_one(c.comparators.first()?, idx)?;
    (seq == seq2).then_some((seq, idx))
}

/// `seq[j], seq[j+1] = seq[j+1], seq[j]` — el idiom pythónico de intercambio
/// en una sola línea, la única forma que esta primera porción reconoce.
fn is_tuple_swap(stmt: &Stmt, seq: &str, idx: &str) -> bool {
    let Stmt::Assign(a) = stmt else { return false };
    let Some(Expr::Tuple(target)) = a.targets.first() else { return false };
    let Expr::Tuple(value) = &*a.value else { return false };
    if target.elts.len() != 2 || value.elts.len() != 2 {
        return false;
    }
    let is_plain = |e: &Expr| subscript_by_name(e) == Some((seq, idx));
    let is_plus_one = |e: &Expr| subscript_by_name_plus_one(e, idx) == Some(seq);
    is_plain(&target.elts[0]) && is_plus_one(&target.elts[1]) && is_plus_one(&value.elts[0]) && is_plain(&value.elts[1])
}

/// Busca, DENTRO del cuerpo del loop más interno (no en cualquier parte de
/// la función — evitar falsos positivos de un swap sin relación en otro
/// lado), un `if` con la forma `adjacent_gt_compare` cuyo cuerpo contenga el
/// swap correspondiente.
fn innermost_has_bubble_swap(inner_body: &[Stmt]) -> bool {
    inner_body.iter().any(|stmt| {
        let Stmt::If(if_stmt) = stmt else { return false };
        let Some((seq, idx)) = adjacent_gt_compare(&if_stmt.test) else { return false };
        if_stmt.body.iter().any(|s| is_tuple_swap(s, seq, idx))
    })
}

pub fn sorting_info(body: &[Stmt]) -> SortingInfo {
    fn walk(stmts: &[Stmt]) -> bool {
        for stmt in stmts {
            let Some(outer_body) = loop_body(stmt) else { continue };
            for inner in outer_body {
                if let Some(inner_body) = loop_body(inner) {
                    if innermost_has_bubble_swap(inner_body) {
                        return true;
                    }
                }
            }
            if walk(outer_body) {
                return true;
            }
        }
        false
    }
    SortingInfo { algorithm: walk(body).then_some("bubble_sort") }
}

pub fn sorting_note(info: &SortingInfo) -> Option<String> {
    match info.algorithm? {
        "bubble_sort" => Some(
            "Nested loops whose innermost body compares two ADJACENT elements of the same sequence \
             (seq[j] > seq[j+1]) and swaps them when out of order — shape matches classic bubble \
             sort, O(n\u{b2}). This is a shape signal, not a correctness check: it does not confirm \
             the outer loop actually shrinks the inner range each pass, or that the comparison \
             direction produces ascending (vs. descending) order."
                .to_string(),
        ),
        _ => None,
    }
}

// ─── Search shape → binary search ────────────────────────────────────────────
//
// Fase 26 (Algorithm Validation Engine), 2026-09-01 — segundo detector de
// FORMA de algoritmo, mismo criterio y mismo hedge que `sorting_info` de
// arriba: reconoce específicamente binary search — un `while lo <= hi:`
// que calcula un punto medio (`mid = (lo + hi) // 2`) y, dentro del mismo
// loop, achica el rango hacia un lado (`lo = mid + 1`) o el otro
// (`hi = mid - 1`) según una comparación que usa ese `mid` como índice.
// Señal de FORMA, no verifica que la secuencia buscada esté realmente
// ordenada, ni que ambas ramas sean alcanzables.
//
// Deliberadamente acotado a esta primera porción: solo reconoce la forma
// exacta `(lo + hi) // 2` (o `(hi + lo) // 2`) — la variante que evita
// overflow en otros lenguajes (`lo + (hi - lo) // 2`) no tiene ese riesgo
// en Python (enteros de precisión arbitraria) y por eso casi no aparece en
// código Python real, pero queda fuera de alcance de todos modos, mismo
// criterio de "primera porción angosta" que `sorting_info` ya aceptó.

pub struct SearchInfo {
    pub algorithm: Option<&'static str>,
}

/// `true` si `e` es exactamente `want - 1` (`mid - 1`, no `1 - mid` ni
/// `mid - 2`) — misma disciplina literal que `index_is_name_plus_one`.
fn is_name_minus_one(e: &Expr, want: &str) -> bool {
    let Expr::BinOp(b) = e else { return false };
    matches!(b.op, Operator::Sub)
        && index_is_name(&b.left, want)
        && matches!(&*b.right, Expr::Constant(c) if matches!(&c.value, Constant::Int(i) if i == &1i64.into()))
}

/// `(nombre_lo, nombre_hi)` si `test` es `lo <= hi` con ambos lados
/// identificadores simples.
fn while_loop_bounds(test: &Expr) -> Option<(&str, &str)> {
    let Expr::Compare(c) = test else { return None };
    if c.ops.len() != 1 || !matches!(c.ops[0], CmpOp::LtE) {
        return None;
    }
    let Expr::Name(lo) = &*c.left else { return None };
    let Expr::Name(hi) = c.comparators.first()? else { return None };
    Some((lo.id.as_str(), hi.id.as_str()))
}

/// Nombre del target si `stmt` es `mid = (lo + hi) // 2` (en cualquier
/// orden de suma).
fn find_midpoint_assign<'a>(body: &'a [Stmt], lo: &str, hi: &str) -> Option<&'a str> {
    body.iter().find_map(|stmt| {
        let Stmt::Assign(a) = stmt else { return None };
        let Expr::Name(mid) = a.targets.first()? else { return None };
        let Expr::BinOp(div) = &*a.value else { return None };
        if !matches!(div.op, Operator::FloorDiv) {
            return None;
        }
        if !matches!(&*div.right, Expr::Constant(c) if matches!(&c.value, Constant::Int(i) if i == &2i64.into()))
        {
            return None;
        }
        let Expr::BinOp(add) = &*div.left else { return None };
        if !matches!(add.op, Operator::Add) {
            return None;
        }
        let is_name = |e: &Expr, want: &str| matches!(e, Expr::Name(n) if n.id.as_str() == want);
        let ok = (is_name(&add.left, lo) && is_name(&add.right, hi)) || (is_name(&add.left, hi) && is_name(&add.right, lo));
        ok.then_some(mid.id.as_str())
    })
}

/// `true` si `test` es una comparación donde alguno de los dos lados es
/// `algo[mid]` — el guard que ata una reasignación de `lo`/`hi` a una
/// decisión basada en el punto medio, no en cualquier otra cosa.
fn compare_uses_index(test: &Expr, idx: &str) -> bool {
    let Expr::Compare(c) = test else { return false };
    let mentions = |e: &Expr| subscript_by_name(e).map(|(_, i)| i == idx).unwrap_or(false);
    mentions(&c.left) || c.comparators.iter().any(mentions)
}

/// `true` si `stmt` es exactamente `target = <valor que matchea>`.
fn reassigns_to(stmt: &Stmt, target: &str, matches_value: impl Fn(&Expr) -> bool) -> bool {
    let Stmt::Assign(a) = stmt else { return false };
    let Some(Expr::Name(t)) = a.targets.first() else { return false };
    t.id.as_str() == target && matches_value(&a.value)
}

/// Busca, dentro de `stmts`, narrowing de `lo`/`hi` guardado por un `if`
/// cuyo test usa `mid` como índice — recorre la cadena `elif` (representada
/// como un `If` anidado en `orelse`) para juntar señal de las 2 o 3 ramas.
fn find_bound_narrowing(stmts: &[Stmt], lo: &str, hi: &str, mid: &str) -> (bool, bool) {
    let check_branch = |branch: &[Stmt]| -> (bool, bool) {
        let mut l = false;
        let mut h = false;
        for s in branch {
            if reassigns_to(s, lo, |e| index_is_name_plus_one(e, mid)) {
                l = true;
            }
            if reassigns_to(s, hi, |e| is_name_minus_one(e, mid)) {
                h = true;
            }
        }
        (l, h)
    };
    let mut narrows_lo = false;
    let mut narrows_hi = false;
    for stmt in stmts {
        if let Stmt::If(if_stmt) = stmt {
            if compare_uses_index(&if_stmt.test, mid) {
                let (l, h) = check_branch(&if_stmt.body);
                narrows_lo |= l;
                narrows_hi |= h;
                let (l2, h2) = check_branch(&if_stmt.orelse);
                narrows_lo |= l2;
                narrows_hi |= h2;
            }
            let (l3, h3) = find_bound_narrowing(&if_stmt.orelse, lo, hi, mid);
            narrows_lo |= l3;
            narrows_hi |= h3;
        }
    }
    (narrows_lo, narrows_hi)
}

pub fn search_info(body: &[Stmt]) -> SearchInfo {
    fn walk(stmts: &[Stmt]) -> bool {
        for stmt in stmts {
            if let Stmt::While(w) = stmt {
                if let Some((lo, hi)) = while_loop_bounds(&w.test) {
                    if let Some(mid) = find_midpoint_assign(&w.body, lo, hi) {
                        let (narrows_lo, narrows_hi) = find_bound_narrowing(&w.body, lo, hi, mid);
                        if narrows_lo && narrows_hi {
                            return true;
                        }
                    }
                }
            }
            if let Some(inner) = loop_body(stmt) {
                if walk(inner) {
                    return true;
                }
            }
        }
        false
    }
    SearchInfo { algorithm: walk(body).then_some("binary_search") }
}

pub fn search_note(info: &SearchInfo) -> Option<String> {
    match info.algorithm? {
        "binary_search" => Some(
            "A while loop bounded by lo <= hi computes a midpoint ((lo+hi)//2) and narrows the \
             range on both sides based on comparisons that use the midpoint as an index — shape \
             matches classic binary search, O(log n). This is a shape signal, not a correctness \
             check: it does not confirm the sequence searched is actually sorted, or that both \
             narrowing branches are reachable."
                .to_string(),
        ),
        _ => None,
    }
}

// ─── Recursion shape → naive Fibonacci ───────────────────────────────────────
//
// Fase 26 (Algorithm Validation Engine), 2026-09-01 — tercer detector de
// FORMA de algoritmo, mismo criterio que `sorting_info`/`search_info` de
// arriba: reconoce la recursión doble clásica de Fibonacci ingenuo (sin
// memoización) — una auto-llamada con `n - 1` sumada a otra auto-llamada
// con `n - 2`, ambas usando el MISMO parámetro, en cualquier parte del
// cuerpo (no solo en la posición de retorno directo, así que cubre tanto
// `return fib(n-1) + fib(n-2)` como `result = fib(n-1) + fib(n-2)`). Señal
// de FORMA, no de complejidad — el Big-O exponencial de esta forma ya lo
// calcula `bigo.rs` de manera independiente vía profundidad de recursión
// de la rama; este detector solo nombra el patrón clásico específico, no
// reemplaza ni depende de ese cálculo.

pub struct RecursionShapeInfo {
    pub algorithm: Option<&'static str>,
}

// ─── Memoización ─────────────────────────────────────────────────────────────
//
// Fase 26 (Algorithm Validation Engine), 2026-09-02 — cierra la salvedad
// que el detector de Fibonacci de abajo venía documentando explícitamente
// desde que se shippeó: "no chequea si hay un lookup de cache/memoización
// que gatee esta expresión, lo que haría que la complejidad real sea lineal
// en vez de exponencial". Eso era un hueco real, no cosmético: un
// `@lru_cache`-eado y uno ingenuo tienen exactamente la misma forma de AST
// en el paso recursivo, y hasta ahora el motor los llamaba igual.
//
// Dos formas, ambas honestas por evidencia distinta:
//   1. **Decorador** — `@lru_cache`/`@cache` (o su forma con punto,
//      `@functools.lru_cache`), incluyendo la variante llamada
//      `@lru_cache(maxsize=None)`: `structure::decorator_name` ya normaliza
//      las 3 sintaxis al mismo string, así que acá alcanza con comparar
//      nombres. Es la señal más fuerte que existe — el propio decorador es
//      la memoización.
//   2. **Cache manual** — un dict que es a la vez consultado por
//      pertenencia (`if k in cache`) Y escrito por subíndice
//      (`cache[k] = ...`) dentro de la misma función.
//
// Dos exclusiones deliberadas, cada una con un test propio:
//   - Un dict CREADO VACÍO dentro del cuerpo (`cache = {}` como primera
//      línea) NO es memoización: se reinicia en cada llamada, así que no
//      cachea nada entre invocaciones. Es justamente el error que un lector
//      apurado cometería, y el motor no lo comete.
//   - Un dict cuyo chequeo de pertenencia LEVANTA (`raise`) es el patrón de
//      tabla de símbolos que `semantic_analysis_info` ya reconoce (Type-1
//      informal), no un cache — la distinción que ese módulo de arriba ya
//      documentaba al revés ("memoización nunca levanta"), reusada acá
//      tal cual vía `has_membership_gated_raise`.

const MEMOIZE_DECORATORS: &[&str] =
    &["lru_cache", "cache", "functools.lru_cache", "functools.cache"];

pub struct MemoizationInfo {
    /// `"decorator"` | `"manual_cache"` — o `None` si no hay memoización.
    pub kind: Option<&'static str>,
    /// El decorador o el nombre del dict que produjo la señal, para que la
    /// nota pueda ser concreta en vez de decir "algo cachea acá".
    pub detail: String,
}

/// Nombres que reciben una asignación por subíndice (`d[k] = v`) — el lado
/// "escribe en el cache" del patrón. Análogo a `grows_via_subscript_assign`
/// de arriba, pero devolviendo el conjunto en vez de preguntar por uno solo
/// (acá los candidatos no se conocen de antemano).
fn subscript_assigned_names(body: &[Stmt]) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut on_stmt = |stmt: &Stmt| {
        if let Stmt::Assign(s) = stmt {
            for t in &s.targets {
                if let Expr::Subscript(sub) = t {
                    if let Expr::Name(n) = &*sub.value {
                        names.insert(n.id.to_string());
                    }
                }
            }
        }
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    names
}

/// Nombres contra los que se testea pertenencia en la condición de un `if`
/// (`if k in d:` / `if k not in d:`) — el lado "consulta el cache".
fn membership_tested_names(body: &[Stmt]) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut on_stmt = |stmt: &Stmt| {
        if let Stmt::If(s) = stmt {
            if let Expr::Compare(c) = &*s.test {
                if c.ops.iter().any(|op| matches!(op, CmpOp::In | CmpOp::NotIn)) {
                    for comp in &c.comparators {
                        if let Expr::Name(n) = comp {
                            names.insert(n.id.to_string());
                        }
                    }
                }
            }
        }
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    names
}

pub fn memoization_info(body: &[Stmt], decorators: &[String]) -> MemoizationInfo {
    if let Some(dec) = decorators.iter().find(|d| MEMOIZE_DECORATORS.contains(&d.as_str())) {
        return MemoizationInfo { kind: Some("decorator"), detail: format!("@{dec}") };
    }
    let written = subscript_assigned_names(body);
    let fresh_each_call = empty_dict_vars(body);
    let mut candidates: Vec<String> = membership_tested_names(body)
        .into_iter()
        .filter(|n| written.contains(n))
        .filter(|n| !fresh_each_call.contains(n))
        .filter(|n| !has_membership_gated_raise(body, n))
        .collect();
    candidates.sort(); // determinismo: el conjunto no tiene orden estable
    match candidates.first() {
        Some(name) => MemoizationInfo { kind: Some("manual_cache"), detail: name.clone() },
        None => MemoizationInfo { kind: None, detail: String::new() },
    }
}

pub fn memoization_note(info: &MemoizationInfo) -> Option<String> {
    let detail = &info.detail;
    match info.kind? {
        "decorator" => Some(format!(
            "Decorated with {detail} — results are cached per argument tuple, so repeated calls \
             with the same arguments return without re-executing the body. Note this engine's \
             Big-O analysis measures the body's own structure and does not subtract the cache: \
             for a function whose cost comes from repeated self-calls, the reported complexity is \
             the uncached one."
        )),
        "manual_cache" => Some(format!(
            "`{detail}` is both membership-tested (`in {detail}`) and written by subscript \
             (`{detail}[...] = ...`) inside this function — the shape of a hand-rolled memoization \
             cache. This is a shape signal, not a proof: it does not verify the cache key is the \
             full argument set, nor that every path consults it before recomputing. A dict created \
             empty inside the body would be reset on every call and is deliberately not counted."
        )),
        _ => None,
    }
}

/// Nombre del parámetro si `e` es exactamente `fn_name(param - amount)` —
/// una auto-llamada de un solo argumento con esa resta exacta.
fn self_call_minus<'a>(e: &'a Expr, fn_name: &str, amount: i64) -> Option<&'a str> {
    let Expr::Call(c) = e else { return None };
    let Expr::Name(f) = &*c.func else { return None };
    if f.id.as_str() != fn_name || c.args.len() != 1 {
        return None;
    }
    let Expr::BinOp(sub) = &c.args[0] else { return None };
    if !matches!(sub.op, Operator::Sub) {
        return None;
    }
    let Expr::Name(param) = &*sub.left else { return None };
    let matches_amount =
        matches!(&*sub.right, Expr::Constant(cc) if matches!(&cc.value, Constant::Int(i) if i == &amount.into()));
    matches_amount.then_some(param.id.as_str())
}

/// `true` si `e` es `fib(x - 1) + fib(x - 2)` (en cualquier orden), con el
/// mismo `x` en ambas llamadas.
fn is_fibonacci_add(e: &Expr, fn_name: &str) -> bool {
    let Expr::BinOp(b) = e else { return false };
    if !matches!(b.op, Operator::Add) {
        return false;
    }
    let direct = self_call_minus(&b.left, fn_name, 1).zip(self_call_minus(&b.right, fn_name, 2));
    let swapped = self_call_minus(&b.left, fn_name, 2).zip(self_call_minus(&b.right, fn_name, 1));
    direct.or(swapped).is_some_and(|(a, b2)| a == b2)
}

/// `memoized` viene de `memoization_info` (arriba) — la misma forma de AST
/// recursiva con y sin cache es indistinguible mirando solo el paso
/// recursivo, así que esa señal se calcula aparte y se pasa acá en vez de
/// re-derivarla. Con cache, el algoritmo deja de llamarse "naive".
pub fn recursion_shape_info(name: &str, body: &[Stmt], memoized: bool) -> RecursionShapeInfo {
    let mut found = false;
    let mut on_expr = |e: &Expr| {
        if !found && is_fibonacci_add(e, name) {
            found = true;
        }
    };
    let mut on_stmt = |_: &Stmt| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    let algorithm = match (found, memoized) {
        (true, false) => Some("naive_fibonacci"),
        (true, true) => Some("memoized_fibonacci"),
        (false, _) => None,
    };
    RecursionShapeInfo { algorithm }
}

pub fn recursion_shape_note(info: &RecursionShapeInfo) -> Option<String> {
    match info.algorithm? {
        "naive_fibonacci" => Some(
            "A self-call with (n-1) added to another self-call with (n-2), same parameter in both \
             — matches the recursive-step shape of classic Fibonacci doubling, with no memoization \
             detected in this function (neither an lru_cache/cache decorator nor a hand-rolled \
             dict cache), so it blows up to O(2\u{207f}); this engine's own recursion-depth Big-O \
             analysis already confirms that independently. Still a shape signal, not a complexity \
             proof: a cache reached through some other mechanism this detector doesn't recognize \
             would change the real cost."
                .to_string(),
        ),
        "memoized_fibonacci" => Some(
            "Same Fibonacci doubling shape as the naive case — a self-call with (n-1) added to one \
             with (n-2) — but this function DOES memoize (see the memoization signal on this same \
             row for which form), so each argument is computed once and the real cost is linear, \
             not O(2\u{207f}). Read the Big-O column with that in mind: it measures this body's own \
             recursive structure and does not subtract the cache, so it still reports the uncached \
             exponential figure."
                .to_string(),
        ),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Grafos de de Bruijn (Fase 32, 2026-09-24)
// ─────────────────────────────────────────────────────────────────────────

/// Nombres que delatan la construcción, en el nombre de la función o de una
/// variable asignada.
const DEBRUIJN_NAME_KEYWORDS: &[&str] = &["debruijn", "de_bruijn", "kmer", "k_mer", "kmers", "k_mers"];

/// Nombres de camino/ciclo euleriano, que es la MITAD que resuelve el
/// ensamblado una vez construido el grafo.
const EULER_NAME_KEYWORDS: &[&str] = &["euler", "eulerian", "euleriano", "hierholzer"];

/// Nombres del encuadre de ensamblado.
///
/// **Esta lista NO participa de la detección, solo del encuadre de la nota**,
/// y por eso puede ser amplia sin riesgo: si nada acá matchea, la nota se
/// queda en el grafo y dice explícitamente que no afirma el dominio. Es la
/// diferencia entre equivocar un énfasis y equivocar un hallazgo.
///
/// Se incluyen los singulares porque los plurales solos se perdían casos
/// reales: en `for lectura in lecturas`, ni el parámetro ni el blanco del
/// `for` entran en `assigned_names`, así que lo único visible era `lectura`.
const ENSAMBLADO_NAME_KEYWORDS: &[&str] =
    &["contig", "genome", "genoma", "assembl", "ensambl", "read", "lectura", "scaffold", "adn", "dna", "fasta", "fastq", "nucleot", "sequenc", "secuenc"];

#[derive(Default, Clone)]
pub struct DeBruijnInfo {
    /// La construcción del grafo: k-mers partidos en prefijo y sufijo.
    pub builds_graph: bool,
    /// El recorrido que resuelve el ensamblado.
    pub walks_eulerian: bool,
    /// **Solo para decidir el encuadre de la nota, nunca la detección.** Un
    /// grafo de de Bruijn no es de ADN: es una construcción general sobre
    /// k-mers (de Bruijn, 1946, para secuencias cíclicas), y aparece igual
    /// en registros de desplazamiento. Afirmar "esto es bioinformática"
    /// porque se vio un grafo de de Bruijn sería el mismo error que el
    /// ROADMAP corrigió en el otro sentido con SCS.
    pub menciona_ensamblado: bool,
}

impl DeBruijnInfo {
    fn detectado(&self) -> bool {
        self.builds_graph || self.walks_eulerian
    }
}

/// Los nombres de las variables de bucle (`for lectura in lecturas`).
///
/// **Hacían falta y no estaban**: `assigned_names` mira `Stmt::Assign`, y el
/// blanco de un `for` es `Stmt::For`, así que en `for lectura in lecturas` no
/// se veía ninguno de los dos nombres. Lo encontró un test que esperaba el
/// encuadre de ensamblado y no lo conseguía — ampliar la lista de palabras
/// no habría servido, porque el nombre no llegaba a compararse.
fn nombres_de_bucle(body: &[Stmt]) -> Vec<String> {
    let mut out = Vec::new();
    let mut on_stmt = |stmt: &Stmt| {
        let Stmt::For(f) = stmt else { return };
        match &*f.target {
            Expr::Name(n) => out.push(n.id.to_string()),
            Expr::Tuple(t) => {
                for e in &t.elts {
                    if let Expr::Name(n) = e {
                        out.push(n.id.to_string());
                    }
                }
            }
            _ => {}
        }
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    out
}

fn contiene_alguna(texto: &str, claves: &[&str]) -> bool {
    let t = texto.to_lowercase().replace('-', "_");
    claves.iter().any(|k| t.contains(k))
}

/// `true` si la expresión es un slice que pela un extremo: `kmer[:-1]` o
/// `kmer[1:]`.
///
/// **Ese par es la jugada que DEFINE un grafo de de Bruijn**: los nodos son
/// los (k-1)-mers y las aristas los k-mers, así que cada k-mer aporta una
/// arista de su prefijo a su sufijo. Sin ese par, lo que hay es un conteo
/// de k-mers, que es otra cosa.
fn es_slice_que_pela_un_extremo(e: &Expr) -> Option<bool> {
    let Expr::Subscript(sub) = e else { return None };
    let Expr::Slice(sl) = &*sub.slice else { return None };
    // `x[:-1]` — sin inicio, fin negativo.
    let quita_el_ultimo = sl.lower.is_none()
        && sl.upper.as_deref().is_some_and(|u| match u {
            Expr::UnaryOp(op) => matches!(&*op.operand, Expr::Constant(c) if matches!(&c.value, Constant::Int(_))),
            _ => false,
        });
    // `x[1:]` — inicio literal, sin fin.
    let quita_el_primero = sl.upper.is_none() && matches!(sl.lower.as_deref(), Some(Expr::Constant(c)) if matches!(&c.value, Constant::Int(_)));
    if quita_el_ultimo {
        return Some(true);
    }
    if quita_el_primero {
        return Some(false);
    }
    None
}

/// Reconoce la construcción de un grafo de de Bruijn y/o su recorrido
/// euleriano.
///
/// **La detección NO se apoya en los nombres solos.** Un `kmer` en el
/// nombre de una variable puede ser cualquier cosa; lo que se exige para
/// `builds_graph` es la estructura: que en el mismo cuerpo aparezcan los dos
/// slices que pelan extremos opuestos. Los nombres suben la confianza, no la
/// crean.
pub fn debruijn_info(name: &str, body: &[Stmt]) -> DeBruijnInfo {
    let asignados = assigned_names(body);
    let texto_de_nombres: String =
        std::iter::once(name.to_string()).chain(asignados.iter().cloned()).chain(nombres_de_bucle(body)).collect::<Vec<_>>().join(" ");

    let (mut pela_final, mut pela_inicio) = (false, false);
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |e: &Expr| match es_slice_que_pela_un_extremo(e) {
        Some(true) => pela_final = true,
        Some(false) => pela_inicio = true,
        None => {}
    };
    walk_stmts(body, &mut on_stmt, &mut on_expr);

    let nombres_de_kmer = contiene_alguna(&texto_de_nombres, DEBRUIJN_NAME_KEYWORDS);
    // El par prefijo/sufijo es la evidencia estructural. Con los dos slices
    // alcanza si además algo se llama k-mer o de Bruijn; sin los nombres se
    // exige que la función misma lo diga, para no marcar cualquier código
    // que parta una cadena por los dos lados.
    let builds_graph = (pela_final && pela_inicio) && (nombres_de_kmer || contiene_alguna(name, DEBRUIJN_NAME_KEYWORDS));

    let walks_eulerian = contiene_alguna(&texto_de_nombres, EULER_NAME_KEYWORDS);

    DeBruijnInfo {
        builds_graph,
        walks_eulerian,
        menciona_ensamblado: contiene_alguna(&texto_de_nombres, ENSAMBLADO_NAME_KEYWORDS),
    }
}

/// El hecho que hace que esto valga la pena contar, y que el propio ROADMAP
/// dejó anotado: **cambiar la formulación del grafo convierte un problema
/// NP-difícil en uno lineal.**
pub fn debruijn_note(info: &DeBruijnInfo) -> Option<String> {
    if !info.detectado() {
        return None;
    }

    let encuadre = if info.menciona_ensamblado {
        "Reconstruir una secuencia a partir de fragmentos es, planteado de frente, el problema del Shortest Common Superstring, que es NP-difícil — y por eso los ensambladores reales NO lo resuelven. "
    } else {
        "Un grafo de de Bruijn NO es una estructura de bioinformática: es una construcción general sobre k-mers (de Bruijn, 1946, para secuencias cíclicas), y aparece igual en registros de desplazamiento. Acá se reconoce la forma, no se afirma el dominio. "
    };

    let cuerpo = if info.builds_graph && info.walks_eulerian {
        "Se ven las dos mitades: la construcción (nodos = (k-1)-mers, aristas = k-mers, cada k-mer aporta una arista de su prefijo a su sufijo) y el recorrido euleriano que la resuelve. \
         Ese cambio de formulación es todo el truco: buscar un camino que use cada arista exactamente una vez es un camino euleriano, y eso se resuelve en O(V+E), lineal — mientras que buscar un camino que visite cada NODO una vez sería hamiltoniano, que es NP-completo. El mismo problema, dos grafos distintos, y la diferencia entre lineal e intratable."
    } else if info.builds_graph {
        "Se ve la construcción: los dos slices que pelan extremos opuestos de cada k-mer son la jugada que define el grafo — nodos = (k-1)-mers, aristas = k-mers. Lo que falta para ensamblar es el recorrido euleriano, que es donde la formulación paga: usar cada arista una vez se resuelve en O(V+E), mientras que visitar cada nodo una vez (hamiltoniano) es NP-completo."
    } else {
        "Se ve el recorrido euleriano (usar cada arista exactamente una vez), que se resuelve en O(V+E) — lineal. Es el contraste exacto con el camino hamiltoniano (visitar cada nodo una vez), que es NP-completo: la diferencia no está en el problema sino en cómo se lo plantea sobre el grafo."
    };

    let limite = if info.builds_graph {
        " Y no es gratis: el camino euleriano devuelve *una* secuencia consistente con los k-mers, no necesariamente la original. Toda repetición más larga que `k` colapsa en un solo nodo y se pierde, que es exactamente donde los ensambladores reales fallan — de ahí que elegir `k` sea un compromiso: más grande resuelve más repeticiones, pero exige más cobertura y sufre más los errores de lectura."
    } else {
        ""
    };

    let nota_de_costo = if info.builds_graph {
        " Sobre el Big-O de arriba: los dos bucles anidados de la construcción no recorren ambos la misma n — el de afuera va por lecturas y el de adentro por posiciones dentro de UNA lectura, así que el trabajo total es lineal en la cantidad de bases, no cuadrático."
    } else {
        ""
    };

    Some(format!("{encuadre}{cuerpo}{limite}{nota_de_costo}"))
}

// ─────────────────────────────────────────────────────────────────────────
// Algoritmos cuánticos (Fase 32, 2026-09-24)
// ─────────────────────────────────────────────────────────────────────────

/// Reconocimiento de FORMA de Shor y Grover.
///
/// **Lo que esto NO hace, y hay que decirlo antes que nada: verificar que el
/// circuito sea correcto.** Desde el AST no se puede comprobar que una
/// secuencia de compuertas implemente Shor — eso exigiría simular el
/// circuito o razonar sobre su semántica. Lo que se reconoce es la forma
/// declarada: el nombre más las piezas que ese algoritmo necesariamente usa.
/// Es el mismo alcance que `sorting_algorithm`/`search_algorithm` ya
/// declaran para bubble sort y búsqueda binaria.
#[derive(Default, Clone)]
pub struct QuantumAlgorithmInfo {
    pub kind: Option<&'static str>,
}

/// Piezas que Grover necesita sí o sí: un oráculo que marca la solución y un
/// operador de difusión que amplifica su amplitud. Sin los dos, lo que hay
/// es un circuito con Hadamards.
const GROVER_PIEZAS_ORACULO: &[&str] = &["oracle", "oraculo", "marcador", "marker"];
const GROVER_PIEZAS_DIFUSION: &[&str] = &["diffus", "difus", "amplific", "amplif"];

/// Shor necesita una transformada de Fourier cuántica (para encontrar el
/// período) y una exponenciación modular (para construir la función cuyo
/// período factoriza).
const SHOR_PIEZAS_QFT: &[&str] = &["qft", "fourier"];
const SHOR_PIEZAS_MODULAR: &[&str] = &["modexp", "mod_exp", "modular", "power_mod", "modpow"];

pub fn quantum_algorithm_info(name: &str, body: &[Stmt]) -> QuantumAlgorithmInfo {
    let asignados = assigned_names(body);
    let llamados = nombres_llamados(body);
    let texto: String = std::iter::once(name.to_lowercase())
        .chain(asignados.iter().map(|s| s.to_lowercase()))
        .chain(llamados.iter().map(|s| s.to_lowercase()))
        .chain(nombres_de_bucle(body).iter().map(|s| s.to_lowercase()))
        .collect::<Vec<_>>()
        .join(" ");

    let dice_grover = contiene_alguna(&texto, &["grover"]);
    let dice_shor = contiene_alguna(&texto, &["shor"]);
    let tiene_oraculo = contiene_alguna(&texto, GROVER_PIEZAS_ORACULO);
    let tiene_difusion = contiene_alguna(&texto, GROVER_PIEZAS_DIFUSION);
    let tiene_qft = contiene_alguna(&texto, SHOR_PIEZAS_QFT);
    let tiene_modular = contiene_alguna(&texto, SHOR_PIEZAS_MODULAR) || usa_pow_de_tres_argumentos(body);

    // El nombre solo alcanza —una función llamada `grover_search` está
    // declarando qué es— pero las piezas sin el nombre también, y ahí la
    // señal es más fuerte que el nombre.
    if dice_grover || (tiene_oraculo && tiene_difusion) {
        return QuantumAlgorithmInfo { kind: Some("grover_search") };
    }
    if dice_shor || (tiene_qft && tiene_modular) {
        return QuantumAlgorithmInfo { kind: Some("shor_factoring") };
    }
    QuantumAlgorithmInfo { kind: None }
}

/// `pow(a, x, n)` — la exponenciación modular de Python, que es la pieza
/// clásica que Shor necesita construir en su versión cuántica.
fn usa_pow_de_tres_argumentos(body: &[Stmt]) -> bool {
    let mut hay = false;
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |e: &Expr| {
        let Expr::Call(c) = e else { return };
        if matches!(&*c.func, Expr::Name(n) if n.id.as_str() == "pow") && c.args.len() == 3 {
            hay = true;
        }
    };
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    hay
}

/// Nombres de todo lo que se llama en el cuerpo, incluidos métodos.
fn nombres_llamados(body: &[Stmt]) -> Vec<String> {
    let mut out = Vec::new();
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |e: &Expr| {
        let Expr::Call(c) = e else { return };
        match &*c.func {
            Expr::Name(n) => out.push(n.id.to_string()),
            Expr::Attribute(a) => out.push(a.attr.to_string()),
            _ => {}
        }
    };
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    out
}

/// **El contraste que vale la pena contar: los dos aceleran, pero no en la
/// misma liga, y uno de los dos tiene techo demostrado.**
pub fn quantum_algorithm_note(info: &QuantumAlgorithmInfo) -> Option<String> {
    let comun = " Esto es reconocimiento de FORMA, no verificación: desde el código no se puede comprobar que el circuito sea correcto, solo que están el nombre y las piezas que el algoritmo necesita.";
    match info.kind? {
        "grover_search" => Some(format!(
            "Forma de algoritmo de Grover: búsqueda no estructurada en O(√N) — un oráculo que marca la solución y un operador de difusión que amplifica su amplitud, repetidos unas √N veces. \
             La aceleración es CUADRÁTICA, no exponencial, y eso no es un detalle: Bennett, Bernstein, Brassard y Vazirani demostraron que √N es ÓPTIMO para búsqueda no estructurada, así que no hay margen para mejorarlo. \
             La consecuencia práctica es la que casi siempre se cuenta mal: Grover no rompe la criptografía simétrica, solo le baja la seguridad efectiva a la mitad de los bits — AES-256 queda en ~128 bits, que sigue siendo holgado. \
             Y hay un detalle que se pasa por alto: iterar de más PASA DE LARGO la solución, porque la amplitud vuelve a bajar; el número de iteraciones no es \"cuantas más, mejor\".{comun}"
        )),
        "shor_factoring" => Some(format!(
            "Forma del algoritmo de Shor: factorización en tiempo polinómico, apoyada en encontrar el PERÍODO de la exponenciación modular con una transformada de Fourier cuántica. \
             Acá sí la aceleración es EXPONENCIAL frente a lo mejor que se conoce clásicamente, y por eso es el que de verdad rompe cosas: RSA, Diffie-Hellman y curvas elípticas, todo lo que se apoya en factorizar o en logaritmo discreto. \
             El contraste con Grover es el punto: Grover acelera de forma cuadrática y con techo demostrado, Shor cambia la clase de complejidad del problema. \
             Lo que NO rompe: hashes ni cifrado simétrico, que no se apoyan en esa estructura.{comun}"
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::parse_module;

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

    #[test]
    fn bubble_sort_clasico_se_detecta() {
        let src = "def bubble_sort(arr):\n    n = len(arr)\n    for i in range(n):\n        for j in range(n - i - 1):\n            if arr[j] > arr[j+1]:\n                arr[j], arr[j+1] = arr[j+1], arr[j]\n";
        let body = first_function_body(src);
        assert_eq!(sorting_info(&body).algorithm, Some("bubble_sort"));
    }

    #[test]
    fn bubble_sort_con_while_tambien_se_detecta() {
        // El detector no exige `for` específicamente -- `while` anidado con
        // la misma forma interna también cuenta como loop.
        let src = "def bubble_sort(seq):\n    i = 0\n    while i < len(seq):\n        j = 0\n        while j < len(seq) - 1:\n            if seq[j] > seq[j+1]:\n                seq[j], seq[j+1] = seq[j+1], seq[j]\n            j += 1\n        i += 1\n";
        let body = first_function_body(src);
        assert_eq!(sorting_info(&body).algorithm, Some("bubble_sort"));
    }

    #[test]
    fn un_solo_loop_no_dispara() {
        // Bubble sort necesita 2+ loops anidados -- un solo loop, aunque
        // compare elementos adyacentes, no tiene la forma completa.
        let src = "def f(arr):\n    for j in range(len(arr) - 1):\n        if arr[j] > arr[j+1]:\n            arr[j], arr[j+1] = arr[j+1], arr[j]\n";
        let body = first_function_body(src);
        assert_eq!(sorting_info(&body).algorithm, None);
    }

    #[test]
    fn comparacion_no_adyacente_no_dispara() {
        // `arr[j] > arr[k]` (dos índices independientes, no `j` y `j+1`) no
        // es la forma de bubble sort -- podría ser cualquier otra cosa
        // (selection sort, una búsqueda de máximo, etc.).
        let src = "def f(arr):\n    for i in range(len(arr)):\n        for j in range(len(arr)):\n            if arr[i] > arr[j]:\n                arr[i], arr[j] = arr[j], arr[i]\n";
        let body = first_function_body(src);
        assert_eq!(sorting_info(&body).algorithm, None);
    }

    #[test]
    fn comparacion_sin_swap_no_dispara() {
        // Comparar elementos adyacentes sin intercambiarlos (ej. solo
        // contar inversiones) no es bubble sort -- el swap es parte de la
        // forma, no un detalle incidental.
        let src = "def f(arr):\n    count = 0\n    for i in range(len(arr)):\n        for j in range(len(arr) - 1):\n            if arr[j] > arr[j+1]:\n                count += 1\n    return count\n";
        let body = first_function_body(src);
        assert_eq!(sorting_info(&body).algorithm, None);
    }

    #[test]
    fn swap_de_variables_no_adyacentes_no_dispara() {
        // El swap existe, pero de índices que NO son los mismos que la
        // comparación (`k`/`k+1` en vez de `j`/`j+1`) -- no debe confundirse
        // con la forma real.
        let src = "def f(arr, k):\n    for i in range(len(arr)):\n        for j in range(len(arr) - 1):\n            if arr[j] > arr[j+1]:\n                arr[k], arr[k+1] = arr[k+1], arr[k]\n";
        let body = first_function_body(src);
        assert_eq!(sorting_info(&body).algorithm, None);
    }

    #[test]
    fn sorting_note_es_none_sin_algoritmo() {
        assert!(sorting_note(&SortingInfo { algorithm: None }).is_none());
    }

    #[test]
    fn sorting_note_bubble_sort_menciona_la_forma_no_una_certeza() {
        let note = sorting_note(&SortingInfo { algorithm: Some("bubble_sort") }).unwrap();
        assert!(note.contains("bubble"));
        assert!(note.contains("not a correctness check"));
    }

    #[test]
    fn binary_search_clasico_de_tres_ramas_se_detecta() {
        let src = "def binary_search(arr, target):\n    lo = 0\n    hi = len(arr) - 1\n    while lo <= hi:\n        mid = (lo + hi) // 2\n        if arr[mid] < target:\n            lo = mid + 1\n        elif arr[mid] > target:\n            hi = mid - 1\n        else:\n            return mid\n    return -1\n";
        let body = first_function_body(src);
        assert_eq!(search_info(&body).algorithm, Some("binary_search"));
    }

    #[test]
    fn binary_search_de_dos_ramas_if_else_tambien_se_detecta() {
        let src = "def f(arr, target):\n    lo, hi = 0, len(arr) - 1\n    while lo <= hi:\n        mid = (lo + hi) // 2\n        if arr[mid] < target:\n            lo = mid + 1\n        else:\n            hi = mid - 1\n    return -1\n";
        let body = first_function_body(src);
        assert_eq!(search_info(&body).algorithm, Some("binary_search"));
    }

    #[test]
    fn narrowing_de_un_solo_lado_no_dispara() {
        // Solo `lo` se achica -- `hi` nunca se toca (la rama `else` corta
        // con un `return`) -- no es la forma completa de binary search.
        let src = "def f(arr, target):\n    lo, hi = 0, len(arr) - 1\n    while lo <= hi:\n        mid = (lo + hi) // 2\n        if arr[mid] < target:\n            lo = mid + 1\n        else:\n            return mid\n    return -1\n";
        let body = first_function_body(src);
        assert_eq!(search_info(&body).algorithm, None);
    }

    #[test]
    fn condicion_de_loop_distinta_de_lo_lte_hi_no_dispara() {
        // `lo < hi` (estricto) en vez de `lo <= hi` no es la forma exacta
        // que este detector reconoce.
        let src = "def f(arr, target):\n    lo, hi = 0, len(arr) - 1\n    while lo < hi:\n        mid = (lo + hi) // 2\n        if arr[mid] < target:\n            lo = mid + 1\n        else:\n            hi = mid - 1\n    return -1\n";
        let body = first_function_body(src);
        assert_eq!(search_info(&body).algorithm, None);
    }

    #[test]
    fn punto_medio_no_estandar_no_dispara() {
        // `mid = lo` no es la forma `(lo + hi) // 2` -- sin punto medio
        // reconocible, no hay binary search que detectar.
        let src = "def f(arr, target):\n    lo, hi = 0, len(arr) - 1\n    while lo <= hi:\n        mid = lo\n        if arr[mid] < target:\n            lo = mid + 1\n        else:\n            hi = mid - 1\n    return -1\n";
        let body = first_function_body(src);
        assert_eq!(search_info(&body).algorithm, None);
    }

    #[test]
    fn narrowing_sin_guard_sobre_mid_no_dispara() {
        // `lo`/`hi` se achican, pero el `if` que los guarda compara `flag`,
        // no `arr[mid]` -- no hay evidencia de que el achicamiento dependa
        // del punto medio en absoluto.
        let src = "def f(arr, target, flag):\n    lo, hi = 0, len(arr) - 1\n    while lo <= hi:\n        mid = (lo + hi) // 2\n        if flag:\n            lo = mid + 1\n        else:\n            hi = mid - 1\n    return -1\n";
        let body = first_function_body(src);
        assert_eq!(search_info(&body).algorithm, None);
    }

    #[test]
    fn search_note_es_none_sin_algoritmo() {
        assert!(search_note(&SearchInfo { algorithm: None }).is_none());
    }

    #[test]
    fn search_note_binary_search_menciona_la_forma_no_una_certeza() {
        let note = search_note(&SearchInfo { algorithm: Some("binary_search") }).unwrap();
        assert!(note.contains("binary search"));
        assert!(note.contains("not a correctness check"));
    }

    #[test]
    fn fibonacci_ingenuo_clasico_se_detecta() {
        let src = "def fib(n):\n    if n < 2:\n        return n\n    return fib(n - 1) + fib(n - 2)\n";
        let body = first_function_body(src);
        assert_eq!(recursion_shape_info("fib", &body, false).algorithm, Some("naive_fibonacci"));
    }

    #[test]
    fn fibonacci_con_orden_de_suma_invertido_tambien_se_detecta() {
        let src = "def fib(n):\n    if n < 2:\n        return n\n    return fib(n - 2) + fib(n - 1)\n";
        let body = first_function_body(src);
        assert_eq!(recursion_shape_info("fib", &body, false).algorithm, Some("naive_fibonacci"));
    }

    #[test]
    fn fibonacci_asignado_a_variable_antes_de_retornar_tambien_se_detecta() {
        // No exige que la suma esté directamente en un `return` -- cualquier
        // parte del cuerpo con la forma exacta cuenta.
        let src = "def fib(n):\n    if n < 2:\n        return n\n    result = fib(n - 1) + fib(n - 2)\n    return result\n";
        let body = first_function_body(src);
        assert_eq!(recursion_shape_info("fib", &body, false).algorithm, Some("naive_fibonacci"));
    }

    #[test]
    fn una_sola_auto_llamada_no_dispara() {
        // Recursión simple (factorial) -- una sola auto-llamada, no la
        // forma de suma doble de Fibonacci.
        let src = "def factorial(n):\n    if n <= 1:\n        return 1\n    return n * factorial(n - 1)\n";
        let body = first_function_body(src);
        assert_eq!(recursion_shape_info("factorial", &body, false).algorithm, None);
    }

    #[test]
    fn offsets_distintos_de_1_y_2_no_disparan() {
        // Dos auto-llamadas sumadas, pero con offsets `n-1`/`n-3` -- no es
        // la forma exacta de Fibonacci.
        let src = "def f(n):\n    if n < 2:\n        return n\n    return f(n - 1) + f(n - 3)\n";
        let body = first_function_body(src);
        assert_eq!(recursion_shape_info("f", &body, false).algorithm, None);
    }

    #[test]
    fn parametros_distintos_en_cada_llamada_no_disparan() {
        // `f(n-1)` y `f(m-2)` -- distintos nombres, no la misma variable en
        // ambas restas.
        let src = "def f(n, m):\n    if n < 2:\n        return n\n    return f(n - 1) + f(m - 2)\n";
        let body = first_function_body(src);
        assert_eq!(recursion_shape_info("f", &body, false).algorithm, None);
    }

    #[test]
    fn recursion_shape_note_es_none_sin_algoritmo() {
        assert!(recursion_shape_note(&RecursionShapeInfo { algorithm: None }).is_none());
    }

    #[test]
    fn recursion_shape_note_fibonacci_menciona_la_forma_no_una_certeza() {
        let note = recursion_shape_note(&RecursionShapeInfo { algorithm: Some("naive_fibonacci") }).unwrap();
        assert!(note.contains("Fibonacci"));
        assert!(note.contains("shape signal, not a complexity proof"));
    }

    // ── Memoización (2026-09-02) ─────────────────────────────────────────

    /// Los decoradores llegan a `memoization_info` ya normalizados por
    /// `structure::decorator_name` — replicado acá para que el test ejercite
    /// exactamente el mismo input que produce `rich.rs`.
    fn first_function_decorators(src: &str) -> Vec<String> {
        let suite = parse_module(src).unwrap();
        for stmt in &suite {
            if let Stmt::FunctionDef(f) = stmt {
                return f.decorator_list.iter().map(crate::ast::structure::decorator_name).collect();
            }
        }
        panic!("no se encontró ninguna función en el src de prueba");
    }

    fn memo_of(src: &str) -> MemoizationInfo {
        memoization_info(&first_function_body(src), &first_function_decorators(src))
    }

    #[test]
    fn lru_cache_pelado_se_detecta_como_decorador() {
        let src = "from functools import lru_cache\n@lru_cache\ndef fib(n):\n    return n\n";
        let info = memo_of(src);
        assert_eq!(info.kind, Some("decorator"));
        assert_eq!(info.detail, "@lru_cache");
    }

    #[test]
    fn lru_cache_llamado_con_maxsize_se_detecta_igual() {
        // `@lru_cache(maxsize=None)` es un Expr::Call -- decorator_name ya
        // lo normaliza al mismo nombre que la forma sin llamar.
        let src = "from functools import lru_cache\n@lru_cache(maxsize=None)\ndef fib(n):\n    return n\n";
        assert_eq!(memo_of(src).kind, Some("decorator"));
    }

    #[test]
    fn functools_cache_con_punto_se_detecta() {
        let src = "import functools\n@functools.cache\ndef fib(n):\n    return n\n";
        let info = memo_of(src);
        assert_eq!(info.kind, Some("decorator"));
        assert_eq!(info.detail, "@functools.cache");
    }

    #[test]
    fn decorador_cualquiera_no_es_memoizacion() {
        let src = "@app.route(\"/x\")\ndef view():\n    return 1\n";
        assert_eq!(memo_of(src).kind, None);
    }

    #[test]
    fn cache_manual_por_parametro_se_detecta() {
        let src = "def fib(n, memo):\n    if n in memo:\n        return memo[n]\n    memo[n] = fib(n - 1, memo) + fib(n - 2, memo)\n    return memo[n]\n";
        let info = memo_of(src);
        assert_eq!(info.kind, Some("manual_cache"));
        assert_eq!(info.detail, "memo");
    }

    #[test]
    fn cache_manual_con_not_in_tambien_se_detecta() {
        let src = "def fib(n, memo):\n    if n not in memo:\n        memo[n] = fib(n - 1, memo) + fib(n - 2, memo)\n    return memo[n]\n";
        assert_eq!(memo_of(src).kind, Some("manual_cache"));
    }

    #[test]
    fn dict_creado_vacio_en_el_cuerpo_no_es_memoizacion() {
        // Se reinicia en CADA llamada -- no cachea nada entre invocaciones.
        // Es la confusión que este detector tiene que evitar, no cometer.
        let src = "def f(items):\n    cache = {}\n    for k in items:\n        if k in cache:\n            continue\n        cache[k] = compute(k)\n    return cache\n";
        assert_eq!(memo_of(src).kind, None);
    }

    #[test]
    fn tabla_de_simbolos_que_levanta_no_es_memoizacion() {
        // Mismo chequeo de dict, pero RECHAZA -- es el patrón Type-1
        // (semantic_analysis_info), no un cache.
        let src = "def check(names, table):\n    for n in names:\n        if n in table:\n            raise ValueError(n)\n        table[n] = True\n";
        assert_eq!(memo_of(src).kind, None);
    }

    #[test]
    fn dict_solo_leido_sin_escritura_no_es_memoizacion() {
        let src = "def f(k, table):\n    if k in table:\n        return table[k]\n    return None\n";
        assert_eq!(memo_of(src).kind, None);
    }

    #[test]
    fn dict_solo_escrito_sin_consulta_no_es_memoizacion() {
        let src = "def f(items, out):\n    for k in items:\n        out[k] = compute(k)\n    return out\n";
        assert_eq!(memo_of(src).kind, None);
    }

    #[test]
    fn fibonacci_memoizado_por_decorador_cambia_de_algoritmo() {
        let src = "from functools import lru_cache\n@lru_cache\ndef fib(n):\n    if n < 2:\n        return n\n    return fib(n - 1) + fib(n - 2)\n";
        let body = first_function_body(src);
        let memo = memo_of(src);
        assert_eq!(recursion_shape_info("fib", &body, memo.kind.is_some()).algorithm, Some("memoized_fibonacci"));
    }

    #[test]
    fn fibonacci_sin_cache_sigue_siendo_naive() {
        let src = "def fib(n):\n    if n < 2:\n        return n\n    return fib(n - 1) + fib(n - 2)\n";
        let body = first_function_body(src);
        let memo = memo_of(src);
        assert_eq!(memo.kind, None);
        assert_eq!(recursion_shape_info("fib", &body, memo.kind.is_some()).algorithm, Some("naive_fibonacci"));
    }

    #[test]
    fn memoizacion_sin_forma_de_fibonacci_no_inventa_un_algoritmo() {
        // La señal de memoización es independiente: existe sola, sin
        // necesitar que la función sea recursiva ni con forma de Fibonacci.
        let src = "from functools import lru_cache\n@lru_cache\ndef costoso(x):\n    return x * 2\n";
        let body = first_function_body(src);
        assert_eq!(memo_of(src).kind, Some("decorator"));
        assert_eq!(recursion_shape_info("costoso", &body, true).algorithm, None);
    }

    #[test]
    fn nota_de_fibonacci_memoizado_advierte_que_el_bigo_no_resta_el_cache() {
        let note = recursion_shape_note(&RecursionShapeInfo { algorithm: Some("memoized_fibonacci") }).unwrap();
        assert!(note.contains("linear"));
        assert!(note.contains("does not subtract the cache"));
    }

    #[test]
    fn nota_de_memoizacion_nombra_la_evidencia_concreta() {
        let dec = memoization_note(&MemoizationInfo { kind: Some("decorator"), detail: "@lru_cache".into() }).unwrap();
        assert!(dec.contains("@lru_cache"));
        let manual = memoization_note(&MemoizationInfo { kind: Some("manual_cache"), detail: "memo".into() }).unwrap();
        assert!(manual.contains("`memo`"));
        assert!(manual.contains("shape signal, not a proof"));
        assert!(memoization_note(&MemoizationInfo { kind: None, detail: String::new() }).is_none());
    }

    // ── Fase 32: grafos de de Bruijn ────────────────────────────────────

    fn debruijn_de(src: &str) -> DeBruijnInfo {
        let suite = parse_module(src).unwrap();
        for stmt in &suite {
            if let Stmt::FunctionDef(f) = stmt {
                return debruijn_info(f.name.as_str(), &f.body);
            }
        }
        panic!("sin función en el src de prueba");
    }

    /// El par prefijo/sufijo es la jugada que DEFINE el grafo: nodos =
    /// (k-1)-mers, aristas = k-mers.
    #[test]
    fn la_construccion_con_prefijo_y_sufijo_se_detecta() {
        let src = "def construir_de_bruijn(lecturas, k):\n    grafo = {}\n    for lectura in lecturas:\n        for i in range(len(lectura) - k + 1):\n            kmer = lectura[i:i + k]\n            prefijo = kmer[:-1]\n            sufijo = kmer[1:]\n            grafo.setdefault(prefijo, []).append(sufijo)\n    return grafo\n";
        let info = debruijn_de(src);
        assert!(info.builds_graph, "el par de slices es la evidencia estructural");
    }

    /// **Un solo slice NO alcanza.** Pelar un extremo solo es cualquier cosa;
    /// lo que define el grafo es pelar los DOS extremos opuestos del mismo
    /// k-mer, porque eso es lo que arma la arista prefijo→sufijo.
    #[test]
    fn un_solo_slice_no_alcanza_para_afirmar_de_bruijn() {
        let src = "def kmer_prefijos(lecturas, k):\n    out = []\n    for lectura in lecturas:\n        for i in range(len(lectura) - k + 1):\n            kmer = lectura[i:i + k]\n            out.append(kmer[:-1])\n    return out\n";
        assert!(!debruijn_de(src).builds_graph);
    }

    /// Y los nombres solos tampoco: sin la estructura, no se afirma.
    #[test]
    fn el_nombre_solo_no_crea_la_deteccion() {
        let src = "def de_bruijn(x):\n    return x + 1\n";
        assert!(!debruijn_de(src).builds_graph);
    }

    #[test]
    fn el_camino_euleriano_se_reconoce_por_su_nombre() {
        let src = "def camino_euleriano(grafo, inicio):\n    pila = [inicio]\n    camino = []\n    while pila:\n        v = pila[-1]\n        if grafo.get(v):\n            pila.append(grafo[v].pop())\n        else:\n            camino.append(pila.pop())\n    return camino[::-1]\n";
        let info = debruijn_de(src);
        assert!(info.walks_eulerian);
        assert!(!info.builds_graph, "reconocer el recorrido no es afirmar que construyó el grafo");
    }

    /// **El hecho que hace que esto valga la pena contar**: el mismo grafo,
    /// dos formulaciones, y la diferencia entre lineal e intratable.
    #[test]
    fn la_nota_contrasta_euleriano_con_hamiltoniano() {
        let src = "def ensamblar_euleriano(lecturas, k):\n    grafo = {}\n    for lectura in lecturas:\n        for i in range(len(lectura) - k + 1):\n            kmer = lectura[i:i + k]\n            grafo.setdefault(kmer[:-1], []).append(kmer[1:])\n    return grafo\n";
        let nota = debruijn_note(&debruijn_de(src)).unwrap();
        assert!(nota.contains("O(V+E)"), "{nota}");
        assert!(nota.contains("hamiltoniano"), "{nota}");
        assert!(nota.contains("NP-completo"), "{nota}");
    }

    /// **Y no es gratis**: repeticiones más largas que `k` colapsan, que es
    /// exactamente donde los ensambladores reales fallan.
    #[test]
    fn la_nota_dice_donde_se_rompe_de_verdad() {
        let src = "def ensamblar_contigs(lecturas, k):\n    grafo = {}\n    for lectura in lecturas:\n        for i in range(len(lectura) - k + 1):\n            kmer = lectura[i:i + k]\n            grafo.setdefault(kmer[:-1], []).append(kmer[1:])\n    return grafo\n";
        let nota = debruijn_note(&debruijn_de(src)).unwrap();
        assert!(nota.contains("más larga que `k`"), "{nota}");
        assert!(nota.contains("NP-difícil"), "con nombres de ensamblado sí se encuadra con SCS: {nota}");
    }

    /// **Un grafo de de Bruijn NO es una estructura de bioinformática.** Sin
    /// nombres de ensamblado, la nota se queda en el grafo y lo dice, en vez
    /// de afirmar el dominio.
    #[test]
    fn sin_nombres_de_ensamblado_no_se_afirma_que_sea_adn() {
        let src = "def de_bruijn_kmers(cadena, k):\n    g = {}\n    for i in range(len(cadena) - k + 1):\n        kmer = cadena[i:i + k]\n        g.setdefault(kmer[:-1], []).append(kmer[1:])\n    return g\n";
        let nota = debruijn_note(&debruijn_de(src)).unwrap();
        assert!(nota.contains("NO es una estructura de bioinformática"), "{nota}");
        assert!(!nota.contains("Shortest Common Superstring"), "no se encuadra con SCS sin evidencia: {nota}");
    }

    /// La nota corrige el O(n²) que el heurístico general reporta: los dos
    /// bucles no recorren ambos la misma n.
    #[test]
    fn la_nota_explica_por_que_no_es_cuadratico() {
        let src = "def construir_de_bruijn(lecturas, k):\n    g = {}\n    for lectura in lecturas:\n        for i in range(len(lectura) - k + 1):\n            kmer = lectura[i:i + k]\n            g.setdefault(kmer[:-1], []).append(kmer[1:])\n    return g\n";
        let nota = debruijn_note(&debruijn_de(src)).unwrap();
        assert!(nota.contains("no recorren ambos la misma n"), "{nota}");
    }

    #[test]
    fn una_funcion_cualquiera_no_produce_nota_de_de_bruijn() {
        let src = "def sumar(a, b):\n    return a + b\n";
        assert!(debruijn_note(&debruijn_de(src)).is_none());
    }

    /// La lista de ensamblado **no participa de la detección, solo del
    /// encuadre**: equivocar un énfasis es distinto de equivocar un
    /// hallazgo, y por eso puede ser amplia sin riesgo.
    #[test]
    fn los_nombres_de_ensamblado_cambian_el_encuadre_pero_no_detectan() {
        // Nombres de ensamblado SIN la estructura: no se detecta nada.
        let sin_forma = "def ensamblar_genoma(contigs):\n    return contigs\n";
        assert!(debruijn_note(&debruijn_de(sin_forma)).is_none());

        // La misma estructura, con y sin nombres de dominio: detectada en
        // los dos casos, y solo cambia cómo se encuadra.
        let con = "def construir(lecturas, k):\n    g = {}\n    for lectura in lecturas:\n        kmer = lectura[0:k]\n        g.setdefault(kmer[:-1], []).append(kmer[1:])\n    return g\n";
        let sin = "def construir(cadenas, k):\n    g = {}\n    for c in cadenas:\n        kmer = c[0:k]\n        g.setdefault(kmer[:-1], []).append(kmer[1:])\n    return g\n";
        let (ic, is_) = (debruijn_de(con), debruijn_de(sin));
        assert!(ic.builds_graph && is_.builds_graph, "la estructura es la misma");
        assert!(ic.menciona_ensamblado);
        assert!(!is_.menciona_ensamblado);
        assert!(debruijn_note(&ic).unwrap().contains("NP-difícil"));
        assert!(debruijn_note(&is_).unwrap().contains("NO es una estructura de bioinformática"));
    }

    // ── Fase 32: Shor y Grover ──────────────────────────────────────────

    fn cuantico_de(src: &str) -> QuantumAlgorithmInfo {
        let suite = parse_module(src).unwrap();
        for stmt in &suite {
            if let Stmt::FunctionDef(f) = stmt {
                return quantum_algorithm_info(f.name.as_str(), &f.body);
            }
        }
        panic!("sin función");
    }

    #[test]
    fn el_nombre_grover_declara_la_forma() {
        assert_eq!(cuantico_de("def grover_search(n):\n    return n\n").kind, Some("grover_search"));
    }

    /// **Y las piezas sin el nombre también**, que es la señal más fuerte:
    /// un oráculo más un operador de difusión son lo que Grover ES.
    #[test]
    fn el_oraculo_mas_la_difusion_alcanzan_sin_el_nombre() {
        let src = "def buscar(n):\n    oraculo = construir()\n    difusion = armar()\n    return oraculo, difusion\n";
        assert_eq!(cuantico_de(src).kind, Some("grover_search"));
    }

    /// Una sola pieza no alcanza: un `oracle` suelto es cualquier cosa.
    #[test]
    fn una_sola_pieza_de_grover_no_alcanza() {
        let src = "def consultar(n):\n    oracle = conectar()\n    return oracle\n";
        assert_eq!(cuantico_de(src).kind, None);
    }

    #[test]
    fn qft_mas_exponenciacion_modular_dan_shor() {
        let src = "def periodo(a, n):\n    qft = aplicar()\n    r = pow(a, 2, n)\n    return qft, r\n";
        assert_eq!(cuantico_de(src).kind, Some("shor_factoring"));
    }

    /// `pow(a, b)` de dos argumentos NO es exponenciación modular.
    #[test]
    fn pow_de_dos_argumentos_no_es_exponenciacion_modular() {
        let src = "def calcular(a):\n    qft = aplicar()\n    return pow(a, 2)\n";
        assert_eq!(cuantico_de(src).kind, None);
    }

    /// **El contraste que vale contar**: Grover acelera de forma cuadrática
    /// y con techo DEMOSTRADO; Shor cambia la clase del problema.
    #[test]
    fn la_nota_contrasta_la_liga_de_cada_uno() {
        let g = quantum_algorithm_note(&cuantico_de("def grover(n):\n    return n\n")).unwrap();
        assert!(g.contains("CUADRÁTICA"), "{g}");
        assert!(g.contains("ÓPTIMO"), "el techo está demostrado: {g}");
        assert!(g.contains("AES-256"), "{g}");
        assert!(g.contains("PASA DE LARGO"), "iterar de más empeora: {g}");

        let sh = quantum_algorithm_note(&cuantico_de("def shor(n):\n    return n\n")).unwrap();
        assert!(sh.contains("EXPONENCIAL"), "{sh}");
        assert!(sh.contains("RSA"), "{sh}");
        assert!(sh.contains("NO rompe"), "y dice qué no rompe: {sh}");
    }

    /// **Esto NO verifica el circuito**, y las dos notas lo dicen.
    #[test]
    fn las_notas_declaran_que_no_verifican_el_circuito() {
        for src in ["def grover(n):\n    return n\n", "def shor(n):\n    return n\n"] {
            let nota = quantum_algorithm_note(&cuantico_de(src)).unwrap();
            assert!(nota.contains("no verificación"), "{nota}");
        }
    }

    #[test]
    fn una_funcion_cualquiera_no_es_un_algoritmo_cuantico() {
        assert_eq!(cuantico_de("def sumar(a, b):\n    return a + b\n").kind, None);
        assert!(quantum_algorithm_note(&QuantumAlgorithmInfo::default()).is_none());
    }
}
