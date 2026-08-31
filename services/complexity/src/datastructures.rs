//! Fase 14 — Data Structure Intelligence. Mismo estilo heurístico que los 3
//! clasificadores del CS Engine (`classifiers.rs`): honesto sobre ser
//! heurística de nombre/forma, no un análisis semántico real. Primera vez
//! que un clasificador vive a nivel de *clase* (`data_structure_info`) y no
//! solo de función (`heap_info`) — `RichClass` no tenía ningún campo de
//! clasificador antes de esto.
//!
//! Las 10 estructuras que pide Fase 14, completas — elegidas y portadas en 3
//! porciones por señal de detección (confianza alta, bajo riesgo de falso
//! positivo), no por orden de la lista original:
//! - **Primera porción**: Heap (uso de `heapq`, function-level), Trie y
//!   Fenwick Tree (class-level, shape solo ya alcanza — insert+search+dict
//!   para Trie, el idiom de bit `i & (-i)` para Fenwick).
//! - **Segunda porción**: AVL Tree y Red-Black Tree (mismo shape "BST con
//!   rotación", distinguidas por el atributo extra que cada una mantiene —
//!   height vs. color — cuando el nombre no lo dice) y Bloom Filter (acá el
//!   nombre ya pasa a ser obligatorio — add+contains es un shape demasiado
//!   genérico para confiar solo en la forma).
//! - **Tercera porción**: Segment Tree, B-Tree, Skip List, HashMap — las 4
//!   últimas, todas con nombre obligatorio (ninguna tiene una señal de
//!   shape lo bastante distintiva por sí sola: build+update+query también
//!   describe un Fenwick sin ese nombre). HashMap además exige un método
//!   con "hash" en el nombre — el nombre solo no alcanza, porque cualquier
//!   wrapper de `dict` podría llamarse "HashMap" sin implementar hashing
//!   propio, y detectar el uso del `dict` nativo de Python sería ruido puro.

use rustpython_parser::ast::{Expr, Operator, Stmt, UnaryOp};

use crate::structure::RichMethod;
use crate::walk::{walk_stmts, walk_stmts_own_scope};

// ─── Heap (function-level) — uso de heapq ───────────────────────────────────

const HEAPQ_METHODS: &[&str] =
    &["heappush", "heappop", "heapify", "heappushpop", "heapreplace", "nlargest", "nsmallest", "merge"];

pub struct HeapInfo {
    pub uses_heap: bool,
}

/// Mismo patrón que `classifiers::regex_info` — detecta `heapq.XXX(...)`
/// directo, no rastrea un alias guardado en variable (`from heapq import
/// heappush as hp` no se detecta, mismo límite honesto que regex_info tiene
/// con `re`).
pub fn heap_info(body: &[Stmt]) -> HeapInfo {
    let mut uses_heap = false;
    let mut on_expr = |expr: &Expr| {
        if uses_heap {
            return;
        }
        if let Expr::Call(c) = expr {
            if let Expr::Attribute(a) = &*c.func {
                if HEAPQ_METHODS.contains(&a.attr.as_str()) {
                    if let Expr::Name(n) = &*a.value {
                        if n.id.as_str() == "heapq" {
                            uses_heap = true;
                        }
                    }
                }
            }
        }
    };
    let mut on_stmt = |_: &Stmt| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    HeapInfo { uses_heap }
}

pub fn heap_note(info: &HeapInfo) -> Option<String> {
    if !info.uses_heap {
        return None;
    }
    Some(
        "Heap (via `heapq`) detected — push/pop in O(log n), peek-min in O(1), \
         build-heap (`heapify`) in O(n). Typical uses: priority queues, k-largest/ \
         k-smallest selection, Dijkstra/Prim. Trades the O(n) pop-min of a plain \
         sorted-on-demand list for logarithmic push/pop, at the cost of no O(1) \
         arbitrary index access."
            .to_string(),
    )
}

// ─── Trie / Fenwick Tree (class-level) ──────────────────────────────────────

const TRIE_NAME_KEYWORDS: &[&str] = &["trie", "prefixtree"];
const TRIE_INSERT_METHODS: &[&str] = &["insert", "add", "add_word"];
const TRIE_SEARCH_METHODS: &[&str] = &["search", "find", "contains", "starts_with", "startswith"];

const FENWICK_NAME_KEYWORDS: &[&str] = &["fenwick", "bit", "binaryindexed", "binary_indexed"];
const FENWICK_UPDATE_METHODS: &[&str] = &["update", "add"];
const FENWICK_QUERY_METHODS: &[&str] = &["query", "sum", "prefix_sum"];

// ─── AVL / Red-Black Tree (class-level) — self-balancing BST, distinguished
// by which extra bookkeeping attribute each keeps to decide when/how to
// rebalance: height (AVL) vs. color (Red-Black). Ambos comparten el shape
// "nodo con left/right + método de rotación" — el atributo extra es lo que
// separa una de otra cuando el nombre de la clase no lo dice.

const AVL_NAME_KEYWORDS: &[&str] = &["avl"];
const REDBLACK_NAME_KEYWORDS: &[&str] = &["redblack", "red_black", "rbtree", "rb_tree"];
const HEIGHT_ATTR_NAMES: &[&str] = &["height", "bf", "balance", "balance_factor"];
const COLOR_ATTR_NAMES: &[&str] = &["color", "red", "is_red", "black"];

// ─── Bloom Filter (class-level) — a diferencia de Fenwick/Trie, acá el
// nombre es obligatorio (no hay un fallback de shape solo): add+contains es
// un shape genérico que cualquier wrapper de colección comparte, así que sin
// el nombre sería demasiado laxo — mismo criterio que `grammar_info` (nombre
// Y shape, no cualquiera de los dos solo).

const BLOOM_NAME_KEYWORDS: &[&str] = &["bloom"];
const BLOOM_ADD_METHODS: &[&str] = &["add", "insert"];
const BLOOM_QUERY_METHODS: &[&str] = &["contains", "might_contain", "possibly_contains", "check", "query"];

// ─── Segment Tree / B-Tree / Skip List / HashMap (class-level) — las 4
// restantes de las 10 que pide Fase 14. A diferencia de Trie/Fenwick, ninguna
// de estas 4 tiene una señal de *shape* lo bastante distintiva por sí sola
// (build+update+query también describe un Fenwick sin ese nombre; insert+
// search también describe un Trie o un BST cualquiera) — el nombre es
// obligatorio para las 4, mismo criterio que ya usa Bloom Filter.

const SEGMENT_TREE_NAME_KEYWORDS: &[&str] = &["segmenttree", "segment_tree", "segtree"];
const SEGMENT_TREE_METHODS: &[&str] = &["build", "update", "query"];

const BTREE_NAME_KEYWORDS: &[&str] = &["btree", "b_tree"];

const SKIPLIST_NAME_KEYWORDS: &[&str] = &["skiplist", "skip_list"];

/// A diferencia de las otras 3, HashMap pide shape además del nombre — el
/// nombre solo no alcanza porque cualquier wrapper de `dict` podría llamarse
/// "HashMap" sin implementar el hashing/bucketing real que hace a esto una
/// estructura de datos propia (Python ya tiene un hash map nativo, `dict` —
/// detectar su uso sería ruido puro, no una señal). Un método cuyo nombre
/// contenga "hash" es la señal de que la clase construye su propio
/// mecanismo de hashing, no que envuelve el de Python.
const HASHMAP_NAME_KEYWORDS: &[&str] = &["hashmap", "hash_map", "hashtable", "hash_table"];

pub struct DataStructureInfo {
    pub kind: Option<&'static str>,
}

pub fn data_structure_info(class_name: &str, methods: &[RichMethod], body: &[Stmt]) -> DataStructureInfo {
    // El idiom de bit `x & (-x)` es una señal de AST distintiva por sí
    // sola — nadie lo escribe por otra razón que aislar el low bit de un
    // Fenwick Tree/BIT. Se chequea primero porque no necesita ningún
    // apoyo de naming (a diferencia de Trie, que si no matchea nombre
    // necesita la combinación insert+search+dict para no ser demasiado
    // laxo).
    if has_low_bit_idiom(body) {
        return DataStructureInfo { kind: Some("Fenwick Tree (BIT)") };
    }

    let name_lower = class_name.to_lowercase();
    let method_names: Vec<String> = methods.iter().map(|m| m.name.to_lowercase()).collect();
    let has_any = |candidates: &[&str]| method_names.iter().any(|m| candidates.contains(&m.as_str()));

    if FENWICK_NAME_KEYWORDS.iter().any(|kw| name_lower.contains(kw))
        && has_any(FENWICK_UPDATE_METHODS)
        && has_any(FENWICK_QUERY_METHODS)
    {
        return DataStructureInfo { kind: Some("Fenwick Tree (BIT)") };
    }

    // AVL / Red-Black: nombre solo alcanza; sin nombre, hace falta el shape
    // completo de un BST auto-balanceado (left+right+rotate) MÁS el
    // atributo que distingue cuál de las dos es (height vs color) — un
    // left+right+rotate sin ninguno de los dos es un BST balanceado
    // genérico sin forma de saber cuál, así que no dispara ninguna.
    let name_says_avl = AVL_NAME_KEYWORDS.iter().any(|kw| name_lower.contains(kw));
    let name_says_redblack = REDBLACK_NAME_KEYWORDS.iter().any(|kw| name_lower.contains(kw));
    if name_says_avl || name_says_redblack {
        return DataStructureInfo { kind: Some(if name_says_avl { "AVL Tree" } else { "Red-Black Tree" }) };
    }
    let has_rotate = method_names.iter().any(|m| m.contains("rotate"));
    if has_rotate && has_self_attribute_named(body, &["left"]) && has_self_attribute_named(body, &["right"]) {
        if has_self_attribute_named(body, HEIGHT_ATTR_NAMES) {
            return DataStructureInfo { kind: Some("AVL Tree") };
        }
        if has_self_attribute_named(body, COLOR_ATTR_NAMES) {
            return DataStructureInfo { kind: Some("Red-Black Tree") };
        }
    }

    if BLOOM_NAME_KEYWORDS.iter().any(|kw| name_lower.contains(kw))
        && has_any(BLOOM_ADD_METHODS)
        && has_any(BLOOM_QUERY_METHODS)
    {
        return DataStructureInfo { kind: Some("Bloom Filter") };
    }

    // Segment Tree / B-Tree / Skip List / HashMap: el nombre es obligatorio
    // para las 4 — ninguna tiene una señal de shape lo bastante distintiva
    // por sí sola (build+update+query también describe un Fenwick sin ese
    // nombre; insert+search también describe un Trie o un BST cualquiera).
    if SEGMENT_TREE_NAME_KEYWORDS.iter().any(|kw| name_lower.contains(kw)) && has_any(SEGMENT_TREE_METHODS) {
        return DataStructureInfo { kind: Some("Segment Tree") };
    }
    if BTREE_NAME_KEYWORDS.iter().any(|kw| name_lower.contains(kw)) {
        return DataStructureInfo { kind: Some("B-Tree") };
    }
    if SKIPLIST_NAME_KEYWORDS.iter().any(|kw| name_lower.contains(kw)) {
        return DataStructureInfo { kind: Some("Skip List") };
    }
    if HASHMAP_NAME_KEYWORDS.iter().any(|kw| name_lower.contains(kw)) && method_names.iter().any(|m| m.contains("hash")) {
        return DataStructureInfo { kind: Some("HashMap") };
    }

    let name_says_trie = TRIE_NAME_KEYWORDS.iter().any(|kw| name_lower.contains(kw));
    let shape_says_trie = has_any(TRIE_INSERT_METHODS) && has_any(TRIE_SEARCH_METHODS) && has_dict_self_attribute(body);
    if name_says_trie || shape_says_trie {
        return DataStructureInfo { kind: Some("Trie") };
    }

    DataStructureInfo { kind: None }
}

pub fn data_structure_note(info: &DataStructureInfo) -> Option<String> {
    match info.kind? {
        "Trie" => Some(
            "Trie (prefix tree) detected — insert/search/prefix-check in O(k), where k is \
             the key length, independent of how many keys are stored. Typical uses: \
             autocomplete, spell-check, IP routing tables. Trades a dict-per-node memory \
             overhead for lookup cost that doesn't grow with the number of stored keys, \
             unlike a hash set's O(k) average but O(n·k) worst case on hash collisions."
                .to_string(),
        ),
        "Fenwick Tree (BIT)" => Some(
            "Fenwick Tree (Binary Indexed Tree) detected — prefix-sum queries and point \
             updates in O(log n), trading the O(1) update / O(n) query of a plain \
             running-sum array for balanced logarithmic cost on both operations. Detected \
             via the `i & (-i)` low-bit isolation idiom, the structural signature of this \
             data structure — not just naming."
                .to_string(),
        ),
        "AVL Tree" => Some(
            "AVL Tree detected — a self-balancing BST guaranteeing O(log n) worst-case \
             insert/delete/search by keeping subtree heights within 1 of each other, \
             rebalancing via rotations on every insert/delete that breaks that invariant. \
             Typical uses: lookup-heavy workloads where the tighter balance (vs. Red-Black) \
             pays off in faster reads. Trades an unbalanced BST's simpler, rotation-free \
             insert for a worst case that can't degrade to O(n) on adversarial insertion order."
                .to_string(),
        ),
        "Red-Black Tree" => Some(
            "Red-Black Tree detected — a self-balancing BST guaranteeing O(log n) \
             worst-case insert/delete/search via a looser color-based balance invariant \
             than AVL (height up to ~2x the theoretical minimum, vs. AVL's ~1.44x), which \
             means fewer rotations per insert/delete. Typical uses: insert-heavy ordered \
             maps/sets (the data structure behind C++ `std::map`, Java `TreeMap`) — where \
             cheaper rebalancing matters more than AVL's tighter lookup performance."
                .to_string(),
        ),
        "Bloom Filter" => Some(
            "Bloom Filter detected — probabilistic set-membership testing in O(k) (k = \
             number of hash functions), using a fixed-size bit array instead of storing \
             elements. Trades perfect accuracy for space: guarantees no false negatives, \
             but allows a tunable false-positive rate. Typical uses: skipping a disk/network \
             lookup for a definitely-absent key (databases, web caches), duplicate-packet \
             detection in routers — unlike a hash set's exact O(1) average membership check \
             at the cost of storing every element."
                .to_string(),
        ),
        "Segment Tree" => Some(
            "Segment Tree detected — range queries (sum/min/max over an interval) and point \
             updates in O(log n), after an O(n) build. Typical uses: range-sum/range-min \
             queries over a mutable array (competitive programming, interval scheduling). \
             Trades a plain array's O(1) point update / O(n) range query for balanced \
             logarithmic cost on both — the same tradeoff a Fenwick Tree makes, but a \
             segment tree also supports range *updates* and non-sum aggregates (min/max/gcd), \
             at the cost of more memory per node."
                .to_string(),
        ),
        "B-Tree" => Some(
            "B-Tree detected — search/insert/delete in O(log n) with a much higher branching \
             factor than a binary tree, keeping the tree shallow. Typical uses: database and \
             filesystem indexes, where each node maps to one disk page — the high branching \
             factor means fewer disk reads per lookup than a binary-tree-shaped index would \
             need. Trades a binary tree's simpler node structure for far fewer levels between \
             root and leaf on large datasets."
                .to_string(),
        ),
        "Skip List" => Some(
            "Skip List detected — search/insert/delete in expected O(log n) via randomized \
             multi-level linked lists (each level skips over more elements than the one \
             below). Typical uses: an alternative to a balanced BST when simpler, lock-free \
             concurrent implementations matter more than worst-case guarantees (Redis' sorted \
             sets, some concurrent map implementations) — trades a balanced tree's worst-case \
             O(log n) for a simpler structure with only expected (not guaranteed) logarithmic \
             performance."
                .to_string(),
        ),
        "HashMap" => Some(
            "Custom hash map detected — average O(1) insert/lookup/delete via its own hashing \
             and bucketing, instead of Python's built-in `dict`. Typical uses: this is usually \
             an educational/interview implementation, or one needing a hash/collision policy \
             Python's `dict` doesn't expose. Trades `dict`'s highly-optimized C implementation \
             for control over the hash function and collision strategy (chaining vs. open \
             addressing) — worth it only when that control is actually needed."
                .to_string(),
        ),
        _ => None,
    }
}

/// `self.X = ...` para algún `X` en `names`, en cualquier método —
/// independiente del valor asignado (a diferencia de
/// `has_dict_self_attribute`, que además exige que el valor sea un dict).
/// Mismo estilo de recorrido que `smells::self_attribute_names`.
fn has_self_attribute_named(class_body: &[Stmt], names: &[&str]) -> bool {
    let mut found = false;
    for item in class_body {
        if found {
            return true;
        }
        if let Stmt::FunctionDef(f) = item {
            let mut on_stmt = |stmt: &Stmt| {
                if found {
                    return;
                }
                if let Stmt::Assign(s) = stmt {
                    let matches_self_attr = s.targets.iter().any(|t| {
                        matches!(t, Expr::Attribute(a) if matches!(&*a.value, Expr::Name(n) if n.id.as_str() == "self") && names.contains(&a.attr.as_str()))
                    });
                    if matches_self_attr {
                        found = true;
                    }
                }
            };
            let mut on_expr = |_: &Expr| {};
            walk_stmts_own_scope(&f.body, &mut on_stmt, &mut on_expr);
        }
    }
    found
}

/// `self.X = {}` (o `dict()`) en cualquier método — proxy de un mapa
/// hijo-por-carácter (`children`), mismo estilo de recorrido que
/// `smells::self_attribute_names`.
fn has_dict_self_attribute(class_body: &[Stmt]) -> bool {
    let mut found = false;
    for item in class_body {
        if found {
            return true;
        }
        if let Stmt::FunctionDef(f) = item {
            let mut on_stmt = |stmt: &Stmt| {
                if found {
                    return;
                }
                if let Stmt::Assign(s) = stmt {
                    let is_self_attr = s.targets.iter().any(|t| matches!(t, Expr::Attribute(_)));
                    let is_dict_literal = matches!(&*s.value, Expr::Dict(_))
                        || matches!(&*s.value, Expr::Call(c) if matches!(&*c.func, Expr::Name(n) if n.id.as_str() == "dict"));
                    if is_self_attr && is_dict_literal {
                        found = true;
                    }
                }
            };
            let mut on_expr = |_: &Expr| {};
            walk_stmts_own_scope(&f.body, &mut on_stmt, &mut on_expr);
        }
    }
    found
}

/// Detecta el idiom `x & (-x)` (en cualquier orden) en el cuerpo de
/// cualquier método — aislamiento del low bit, la operación central de un
/// Fenwick Tree/BIT (`i & (-i)` para subir/bajar el índice).
fn has_low_bit_idiom(class_body: &[Stmt]) -> bool {
    fn same_name(a: &Expr, b: &Expr) -> bool {
        matches!((a, b), (Expr::Name(x), Expr::Name(y)) if x.id == y.id)
    }
    fn is_negation_of(candidate: &Expr, other: &Expr) -> bool {
        matches!(candidate, Expr::UnaryOp(u) if matches!(u.op, UnaryOp::USub) && same_name(&u.operand, other))
    }

    let mut found = false;
    for item in class_body {
        if found {
            break;
        }
        if let Stmt::FunctionDef(f) = item {
            let mut on_expr = |expr: &Expr| {
                if found {
                    return;
                }
                if let Expr::BinOp(b) = expr {
                    if matches!(b.op, Operator::BitAnd) && (is_negation_of(&b.left, &b.right) || is_negation_of(&b.right, &b.left)) {
                        found = true;
                    }
                }
            };
            let mut on_stmt = |_: &Stmt| {};
            walk_stmts(&f.body, &mut on_stmt, &mut on_expr);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_module;

    fn functions_body(src: &str) -> Vec<Stmt> {
        let suite = parse_module(src).unwrap();
        if let Stmt::FunctionDef(f) = &suite[0] {
            f.body.clone()
        } else {
            panic!("primer statement no es una función")
        }
    }

    fn class_stmt(src: &str) -> Stmt {
        let suite = parse_module(src).unwrap();
        suite.into_iter().next().unwrap()
    }

    fn class_body(stmt: &Stmt) -> &[Stmt] {
        if let Stmt::ClassDef(c) = stmt {
            &c.body
        } else {
            panic!("statement no es una clase")
        }
    }

    fn class_methods(body: &[Stmt]) -> Vec<RichMethod> {
        body.iter()
            .filter_map(|item| match item {
                Stmt::FunctionDef(f) => Some(RichMethod { name: f.name.to_string(), line: 0, args: vec![], is_async: false }),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn heap_detectado_por_heappush() {
        let body = functions_body("def f():\n    heapq.heappush(h, 1)\n");
        assert!(heap_info(&body).uses_heap);
    }

    #[test]
    fn heap_no_detectado_sin_heapq() {
        let body = functions_body("def f():\n    lst.append(1)\n");
        assert!(!heap_info(&body).uses_heap);
    }

    #[test]
    fn trie_detectado_por_nombre() {
        let stmt = class_stmt("class Trie:\n    def __init__(self):\n        pass\n");
        let body = class_body(&stmt);
        let methods = class_methods(body);
        let info = data_structure_info("Trie", &methods, body);
        assert_eq!(info.kind, Some("Trie"));
    }

    #[test]
    fn trie_detectado_por_shape_sin_nombre() {
        let src = "class Node:\n    def __init__(self):\n        self.children = {}\n    def insert(self, word):\n        pass\n    def search(self, word):\n        pass\n";
        let stmt = class_stmt(src);
        let body = class_body(&stmt);
        let methods = class_methods(body);
        let info = data_structure_info("Node", &methods, body);
        assert_eq!(info.kind, Some("Trie"));
    }

    #[test]
    fn clase_generica_no_dispara_trie() {
        let src = "class Config:\n    def __init__(self):\n        self.data = {}\n    def get(self, key):\n        pass\n";
        let stmt = class_stmt(src);
        let body = class_body(&stmt);
        let methods = class_methods(body);
        let info = data_structure_info("Config", &methods, body);
        assert_eq!(info.kind, None);
    }

    #[test]
    fn fenwick_detectado_por_idiom_bit_sin_nombre_especifico() {
        let src = "class Tree:\n    def update(self, i, delta):\n        while i <= self.n:\n            self.arr[i] += delta\n            i += i & (-i)\n";
        let stmt = class_stmt(src);
        let body = class_body(&stmt);
        let methods = class_methods(body);
        let info = data_structure_info("Tree", &methods, body);
        assert_eq!(info.kind, Some("Fenwick Tree (BIT)"));
    }

    #[test]
    fn fenwick_detectado_por_nombre_y_metodos() {
        let src = "class FenwickTree:\n    def update(self, i, delta):\n        pass\n    def query(self, i):\n        pass\n";
        let stmt = class_stmt(src);
        let body = class_body(&stmt);
        let methods = class_methods(body);
        let info = data_structure_info("FenwickTree", &methods, body);
        assert_eq!(info.kind, Some("Fenwick Tree (BIT)"));
    }

    #[test]
    fn expresion_con_y_bitand_no_relacionada_no_dispara_fenwick() {
        let src = "class Mask:\n    def apply(self, x, y):\n        return x & y\n";
        let stmt = class_stmt(src);
        let body = class_body(&stmt);
        let methods = class_methods(body);
        let info = data_structure_info("Mask", &methods, body);
        assert_eq!(info.kind, None);
    }

    #[test]
    fn avl_detectado_por_nombre_sin_shape() {
        let stmt = class_stmt("class AVLTree:\n    def __init__(self):\n        pass\n");
        let body = class_body(&stmt);
        let methods = class_methods(body);
        let info = data_structure_info("AVLTree", &methods, body);
        assert_eq!(info.kind, Some("AVL Tree"));
    }

    #[test]
    fn avl_detectado_por_shape_sin_nombre() {
        let src = "class Node:\n    def __init__(self):\n        self.left = None\n        self.right = None\n        self.height = 1\n    def rotate_left(self):\n        pass\n";
        let stmt = class_stmt(src);
        let body = class_body(&stmt);
        let methods = class_methods(body);
        let info = data_structure_info("Node", &methods, body);
        assert_eq!(info.kind, Some("AVL Tree"));
    }

    #[test]
    fn redblack_detectado_por_nombre_sin_shape() {
        let stmt = class_stmt("class RedBlackTree:\n    def __init__(self):\n        pass\n");
        let body = class_body(&stmt);
        let methods = class_methods(body);
        let info = data_structure_info("RedBlackTree", &methods, body);
        assert_eq!(info.kind, Some("Red-Black Tree"));
    }

    #[test]
    fn redblack_detectado_por_shape_sin_nombre() {
        let src = "class Node:\n    def __init__(self):\n        self.left = None\n        self.right = None\n        self.color = 'red'\n    def rotate_right(self):\n        pass\n";
        let stmt = class_stmt(src);
        let body = class_body(&stmt);
        let methods = class_methods(body);
        let info = data_structure_info("Node", &methods, body);
        assert_eq!(info.kind, Some("Red-Black Tree"));
    }

    #[test]
    fn bst_balanceado_generico_sin_height_ni_color_no_dispara_nada() {
        // left+right+rotate sin height/color: no hay forma de saber si es
        // AVL o Red-Black, así que no debe disparar ninguna de las dos.
        let src = "class Node:\n    def __init__(self):\n        self.left = None\n        self.right = None\n    def rotate_left(self):\n        pass\n";
        let stmt = class_stmt(src);
        let body = class_body(&stmt);
        let methods = class_methods(body);
        let info = data_structure_info("Node", &methods, body);
        assert_eq!(info.kind, None);
    }

    #[test]
    fn bloom_filter_detectado_por_nombre_y_shape() {
        let src = "class BloomFilter:\n    def add(self, item):\n        pass\n    def contains(self, item):\n        pass\n";
        let stmt = class_stmt(src);
        let body = class_body(&stmt);
        let methods = class_methods(body);
        let info = data_structure_info("BloomFilter", &methods, body);
        assert_eq!(info.kind, Some("Bloom Filter"));
    }

    #[test]
    fn add_contains_sin_nombre_bloom_no_dispara() {
        // add+contains es un shape demasiado genérico (cualquier wrapper de
        // colección lo tiene) — sin el nombre, Bloom Filter no debe disparar.
        let src = "class MySet:\n    def add(self, item):\n        pass\n    def contains(self, item):\n        pass\n";
        let stmt = class_stmt(src);
        let body = class_body(&stmt);
        let methods = class_methods(body);
        let info = data_structure_info("MySet", &methods, body);
        assert_eq!(info.kind, None);
    }

    #[test]
    fn segment_tree_detectado_por_nombre_y_metodos() {
        let src = "class SegmentTree:\n    def build(self, arr):\n        pass\n    def update(self, i, val):\n        pass\n    def query(self, l, r):\n        pass\n";
        let stmt = class_stmt(src);
        let body = class_body(&stmt);
        let methods = class_methods(body);
        let info = data_structure_info("SegmentTree", &methods, body);
        assert_eq!(info.kind, Some("Segment Tree"));
    }

    #[test]
    fn segment_tree_nombre_sin_metodos_no_dispara() {
        let stmt = class_stmt("class SegmentTree:\n    def __init__(self):\n        pass\n");
        let body = class_body(&stmt);
        let methods = class_methods(body);
        let info = data_structure_info("SegmentTree", &methods, body);
        assert_eq!(info.kind, None);
    }

    #[test]
    fn btree_detectado_por_nombre() {
        let stmt = class_stmt("class BTree:\n    def __init__(self):\n        pass\n");
        let body = class_body(&stmt);
        let methods = class_methods(body);
        let info = data_structure_info("BTree", &methods, body);
        assert_eq!(info.kind, Some("B-Tree"));
    }

    #[test]
    fn skiplist_detectado_por_nombre() {
        let stmt = class_stmt("class SkipList:\n    def __init__(self):\n        pass\n");
        let body = class_body(&stmt);
        let methods = class_methods(body);
        let info = data_structure_info("SkipList", &methods, body);
        assert_eq!(info.kind, Some("Skip List"));
    }

    #[test]
    fn hashmap_detectado_por_nombre_y_metodo_hash() {
        let src = "class HashMap:\n    def _hash(self, key):\n        pass\n    def put(self, key, value):\n        pass\n";
        let stmt = class_stmt(src);
        let body = class_body(&stmt);
        let methods = class_methods(body);
        let info = data_structure_info("HashMap", &methods, body);
        assert_eq!(info.kind, Some("HashMap"));
    }

    #[test]
    fn hashmap_nombre_sin_metodo_hash_no_dispara() {
        // Un wrapper de dict llamado "HashMap" sin hashing propio no debería
        // dispararse — sería lo mismo que marcar cualquier uso de `dict`.
        let src = "class HashMap:\n    def __init__(self):\n        self.data = {}\n    def get(self, key):\n        pass\n";
        let stmt = class_stmt(src);
        let body = class_body(&stmt);
        let methods = class_methods(body);
        let info = data_structure_info("HashMap", &methods, body);
        assert_eq!(info.kind, None);
    }
}
