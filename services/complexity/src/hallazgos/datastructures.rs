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

use crate::ast::structure::RichMethod;
use crate::ast::walk::{walk_stmts, walk_stmts_own_scope};

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
    /// Cuántas funciones hash usa un Bloom Filter, si está escrito como
    /// literal. Ver `techo_de_falsos_positivos`.
    pub bloom_hashes: Option<u32>,
    /// Bits del arreglo, si está como literal.
    pub bloom_bits: Option<u64>,
}

/// Nombres de atributo que suelen llevar el conteo de funciones hash.
const NOMBRES_HASHES: &[&str] = &["num_hashes", "n_hashes", "hash_count", "num_hash", "hashes", "k"];
/// Nombres que suelen llevar el tamaño del arreglo de bits.
const NOMBRES_BITS: &[&str] = &["size", "num_bits", "n_bits", "bits", "capacity", "m"];

/// Busca `self.X = <entero>` y los defaults de `__init__` — las dos formas
/// en que estos parámetros aparecen escritos en la práctica.
fn entero_de_atributo(body: &[Stmt], nombres: &[&str]) -> Option<u64> {
    fn literal(e: &Expr) -> Option<u64> {
        match e {
            Expr::Constant(c) => match &c.value {
                rustpython_parser::ast::Constant::Int(i) => i.to_string().parse().ok(),
                _ => None,
            },
            _ => None,
        }
    }
    fn coincide(nombre: &str, nombres: &[&str]) -> bool {
        let n = nombre.trim_start_matches('_').to_lowercase();
        nombres.contains(&n.as_str())
    }

    for item in body {
        let Stmt::FunctionDef(f) = item else { continue };
        // Defaults de `__init__`: `def __init__(self, size=1000, k=3)`.
        let args = &f.args;
        let base = args.args.len().saturating_sub(args.defaults().count());
        for (i, a) in args.args.iter().enumerate().skip(base) {
            if coincide(a.def.arg.as_str(), nombres) {
                if let Some(v) = args.defaults().nth(i - base).and_then(literal) {
                    return Some(v);
                }
            }
        }
        // `self.X = 1000` en cualquier método.
        for stmt in &f.body {
            let Stmt::Assign(a) = stmt else { continue };
            let Some(Expr::Attribute(attr)) = a.targets.first() else { continue };
            if coincide(attr.attr.as_str(), nombres) {
                if let Some(v) = literal(&a.value) {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// **El hecho exacto que se puede derivar de un solo parámetro.**
///
/// A carga óptima, la tasa de falsos positivos de un Bloom Filter es
/// exactamente `2^-k`, donde `k` es la cantidad de funciones hash — y **no
/// depende del tamaño del arreglo de bits**. Agrandar el arreglo mueve
/// *cuántos elementos* entran antes de llegar a ese óptimo; no mejora el
/// piso. Con k=3 la tasa no puede bajar del 12.5% por más bits que se le den.
///
/// Es la cota de concentración escrita como aritmética: el mismo tipo de
/// cuenta que `w = ⌈e/ε⌉` en un Count-Min Sketch. La fórmula sale de que en
/// el óptimo la mitad de los bits está en 1, así que k bits consultados dan
/// falso positivo con probabilidad `(1/2)^k`.
fn techo_de_falsos_positivos(k: u32) -> f64 {
    2f64.powi(-(k as i32))
}

/// Cuántos elementos entran antes de pasar ese óptimo: `n* = m·ln2/k`.
fn capacidad_optima(bits: u64, k: u32) -> u64 {
    ((bits as f64) * std::f64::consts::LN_2 / (k as f64)).round() as u64
}

pub fn data_structure_info(class_name: &str, methods: &[RichMethod], body: &[Stmt]) -> DataStructureInfo {
    // El idiom de bit `x & (-x)` es una señal de AST distintiva por sí
    // sola — nadie lo escribe por otra razón que aislar el low bit de un
    // Fenwick Tree/BIT. Se chequea primero porque no necesita ningún
    // apoyo de naming (a diferencia de Trie, que si no matchea nombre
    // necesita la combinación insert+search+dict para no ser demasiado
    // laxo).
    if has_low_bit_idiom(body) {
        return DataStructureInfo { kind: Some("Fenwick Tree (BIT)"), bloom_hashes: None, bloom_bits: None };
    }

    let name_lower = class_name.to_lowercase();
    let method_names: Vec<String> = methods.iter().map(|m| m.name.to_lowercase()).collect();
    let has_any = |candidates: &[&str]| method_names.iter().any(|m| candidates.contains(&m.as_str()));

    if FENWICK_NAME_KEYWORDS.iter().any(|kw| name_lower.contains(kw))
        && has_any(FENWICK_UPDATE_METHODS)
        && has_any(FENWICK_QUERY_METHODS)
    {
        return DataStructureInfo { kind: Some("Fenwick Tree (BIT)"), bloom_hashes: None, bloom_bits: None };
    }

    // AVL / Red-Black: nombre solo alcanza; sin nombre, hace falta el shape
    // completo de un BST auto-balanceado (left+right+rotate) MÁS el
    // atributo que distingue cuál de las dos es (height vs color) — un
    // left+right+rotate sin ninguno de los dos es un BST balanceado
    // genérico sin forma de saber cuál, así que no dispara ninguna.
    let name_says_avl = AVL_NAME_KEYWORDS.iter().any(|kw| name_lower.contains(kw));
    let name_says_redblack = REDBLACK_NAME_KEYWORDS.iter().any(|kw| name_lower.contains(kw));
    if name_says_avl || name_says_redblack {
        return DataStructureInfo { kind: Some(if name_says_avl { "AVL Tree" } else { "Red-Black Tree" }), bloom_hashes: None, bloom_bits: None };
    }
    let has_rotate = method_names.iter().any(|m| m.contains("rotate"));
    if has_rotate && has_self_attribute_named(body, &["left"]) && has_self_attribute_named(body, &["right"]) {
        if has_self_attribute_named(body, HEIGHT_ATTR_NAMES) {
            return DataStructureInfo { kind: Some("AVL Tree"), bloom_hashes: None, bloom_bits: None };
        }
        if has_self_attribute_named(body, COLOR_ATTR_NAMES) {
            return DataStructureInfo { kind: Some("Red-Black Tree"), bloom_hashes: None, bloom_bits: None };
        }
    }

    if BLOOM_NAME_KEYWORDS.iter().any(|kw| name_lower.contains(kw))
        && has_any(BLOOM_ADD_METHODS)
        && has_any(BLOOM_QUERY_METHODS)
    {
        return DataStructureInfo {
            kind: Some("Bloom Filter"),
            bloom_hashes: entero_de_atributo(body, NOMBRES_HASHES).and_then(|v| u32::try_from(v).ok()),
            bloom_bits: entero_de_atributo(body, NOMBRES_BITS),
        };
    }

    // Segment Tree / B-Tree / Skip List / HashMap: el nombre es obligatorio
    // para las 4 — ninguna tiene una señal de shape lo bastante distintiva
    // por sí sola (build+update+query también describe un Fenwick sin ese
    // nombre; insert+search también describe un Trie o un BST cualquiera).
    if SEGMENT_TREE_NAME_KEYWORDS.iter().any(|kw| name_lower.contains(kw)) && has_any(SEGMENT_TREE_METHODS) {
        return DataStructureInfo { kind: Some("Segment Tree"), bloom_hashes: None, bloom_bits: None };
    }
    if BTREE_NAME_KEYWORDS.iter().any(|kw| name_lower.contains(kw)) {
        return DataStructureInfo { kind: Some("B-Tree"), bloom_hashes: None, bloom_bits: None };
    }
    if SKIPLIST_NAME_KEYWORDS.iter().any(|kw| name_lower.contains(kw)) {
        return DataStructureInfo { kind: Some("Skip List"), bloom_hashes: None, bloom_bits: None };
    }
    if HASHMAP_NAME_KEYWORDS.iter().any(|kw| name_lower.contains(kw)) && method_names.iter().any(|m| m.contains("hash")) {
        return DataStructureInfo { kind: Some("HashMap"), bloom_hashes: None, bloom_bits: None };
    }

    let name_says_trie = TRIE_NAME_KEYWORDS.iter().any(|kw| name_lower.contains(kw));
    let shape_says_trie = has_any(TRIE_INSERT_METHODS) && has_any(TRIE_SEARCH_METHODS) && has_dict_self_attribute(body);
    if name_says_trie || shape_says_trie {
        return DataStructureInfo { kind: Some("Trie"), bloom_hashes: None, bloom_bits: None };
    }

    // Fase 32 — persistentes/retroactivas. Va al final a propósito: un
    // Trie o un Segment Tree escritos de forma inmutable tienen que seguir
    // reportándose por lo que SON; la persistencia es cómo se los actualiza,
    // no qué estructura son.
    if let Some(k) = nivel_de_persistencia(class_name, methods, body) {
        return DataStructureInfo { kind: Some(k), bloom_hashes: None, bloom_bits: None };
    }

    DataStructureInfo { kind: None, bloom_hashes: None, bloom_bits: None }
}

pub fn data_structure_note(info: &DataStructureInfo) -> Option<String> {
    // Fase 32 — las tres variantes de persistencia comparten la mayor parte
    // del texto, así que se arman en su propia función en vez de repetir el
    // párrafo de desambiguación tres veces en este `match`.
    let kind = info.kind?;
    if kind.starts_with("Persistent Structure") || kind == "Retroactive Structure" {
        return nota_de_persistencia(kind);
    }
    match kind {
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
        "Bloom Filter" => {
            let base = "Bloom Filter detected — probabilistic set-membership testing in O(k) (k = \
             number of hash functions), using a fixed-size bit array instead of storing \
             elements. Trades perfect accuracy for space: guarantees no false negatives, \
             but allows a tunable false-positive rate. Typical uses: skipping a disk/network \
             lookup for a definitely-absent key (databases, web caches), duplicate-packet \
             detection in routers — unlike a hash set's exact O(1) average membership check \
             at the cost of storing every element."
                .to_string();
            // Si los parámetros están escritos como literales se puede decir
            // algo EXACTO en vez de genérico. Ver `techo_de_falsos_positivos`.
            let Some(k) = info.bloom_hashes.filter(|k| *k > 0) else {
                return Some(base);
            };
            let techo = techo_de_falsos_positivos(k) * 100.0;
            let mut extra = format!(
                " **This one uses k={k} hash functions, and that alone caps its accuracy: at its \
                 optimal load the false-positive rate is exactly 2^-{k} = {techo:.2}%, and NO \
                 amount of extra bits improves that floor.** More bits buy capacity — how many \
                 elements fit before you pass that optimum — not a better rate. To go below \
                 {techo:.2}% you need more hash functions, not a bigger array."
            );
            if let Some(m) = info.bloom_bits.filter(|m| *m > 0) {
                let n = capacidad_optima(m, k);
                extra.push_str(&format!(
                    " With m={m} bits that optimum sits at about n={n} elements (m·ln2/k); past \
                     it the rate climbs above the {techo:.2}% floor."
                ));
            }
            Some(base + &extra)
        }
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

// ─────────────────────────────────────────────────────────────────────────
// Estructuras persistentes y retroactivas (Fase 32, 2026-09-24)
// ─────────────────────────────────────────────────────────────────────────

/// Nombres de atributo donde vive la lista de versiones.
const NOMBRES_VERSIONES: &[&str] = &["versions", "versiones", "history", "historia", "historial", "snapshots", "revisions", "roots", "raices"];

/// Nombres que dicen el nivel sin que haya que deducirlo.
const NOMBRES_PERSISTENTE: &[&str] = &["persistent", "persistente", "immutable", "inmutable", "versioned", "versionado", "undoable"];
const NOMBRES_RETROACTIVO: &[&str] = &["retroactive", "retroactivo", "retroactiva"];

/// Nombres de argumento que identifican una versión o un instante.
const ARGS_DE_VERSION: &[&str] = &["version", "version_id", "versión", "rev", "revision", "snapshot", "time", "tiempo", "t", "timestamp"];

/// Métodos que MODIFICAN. Que uno de estos acepte una versión es lo que
/// separa la persistencia total de la parcial.
const METODOS_DE_ESCRITURA: &[&str] = &["insert", "insertar", "update", "actualizar", "set", "add", "agregar", "delete", "borrar", "remove", "quitar", "push", "pop", "put", "assoc", "dissoc"];

fn nombre_dice(texto: &str, claves: &[&str]) -> bool {
    let t = texto.to_lowercase().replace('-', "_");
    claves.iter().any(|k| t.contains(k))
}

/// `true` si algún método devuelve una instancia NUEVA de la propia clase.
///
/// **Es la jugada que define una estructura persistente**: en vez de mutar
/// y devolver `None`, se construye una versión nueva y se devuelve. Sin
/// esto, lo que hay es una estructura mutable con un historial al lado, que
/// es otra cosa.
fn devuelve_una_instancia_de_si_misma(body: &[Stmt], class_name: &str) -> bool {
    let mut encontrado = false;
    let mut on_stmt = |stmt: &Stmt| {
        if encontrado {
            return;
        }
        let Stmt::Return(r) = stmt else { return };
        let Some(valor) = r.value.as_deref() else { return };
        let Expr::Call(c) = valor else { return };
        if matches!(&*c.func, Expr::Name(n) if n.id.as_str() == class_name) {
            encontrado = true;
        }
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    encontrado
}

/// Nivel de persistencia, con los tres que el ROADMAP pide distinguir.
///
/// La diferencia parcial/total se decide por una señal mecánica y no por el
/// nombre: **si un método de ESCRITURA acepta una versión como argumento**,
/// se puede modificar desde el pasado, y eso es persistencia total. Si las
/// escrituras no toman versión, solo extienden la última: parcial.
fn nivel_de_persistencia(class_name: &str, methods: &[RichMethod], body: &[Stmt]) -> Option<&'static str> {
    let nombres_de_metodos: String = methods.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(" ");
    let todo_el_texto = format!("{class_name} {nombres_de_metodos}");

    // La retroactividad se reconoce por el nombre y NO se deduce. Cambiar el
    // pasado y recalcular el presente no tiene una firma distinguible de
    // escribir en una versión vieja; afirmarlo sin que el código lo diga
    // sería adivinar.
    if nombre_dice(&todo_el_texto, NOMBRES_RETROACTIVO) {
        return Some("Retroactive Structure");
    }

    let tiene_lista_de_versiones = {
        let mut hay = false;
        let mut on_expr = |e: &Expr| {
            if let Expr::Attribute(a) = e {
                if nombre_dice(a.attr.as_str(), NOMBRES_VERSIONES) {
                    hay = true;
                }
            }
        };
        let mut on_stmt = |_: &Stmt| {};
        walk_stmts(body, &mut on_stmt, &mut on_expr);
        hay
    };

    let devuelve_nueva = devuelve_una_instancia_de_si_misma(body, class_name);
    let nombre_lo_dice = nombre_dice(&todo_el_texto, NOMBRES_PERSISTENTE);

    // Hace falta evidencia ESTRUCTURAL, no solo un nombre: una clase que se
    // llame `ImmutableConfig` sin ninguna de las dos formas no es una
    // estructura persistente, es una convención de nombre.
    if !devuelve_nueva && !tiene_lista_de_versiones {
        return None;
    }
    // Y al revés: devolver una instancia nueva tampoco alcanza solo — un
    // builder encadenado hace eso mismo. Se exige que algo lo confirme.
    if !nombre_lo_dice && !tiene_lista_de_versiones {
        return None;
    }

    let escritura_acepta_version = methods.iter().any(|m| {
        nombre_dice(&m.name, METODOS_DE_ESCRITURA) && m.args.iter().any(|a| ARGS_DE_VERSION.contains(&a.to_lowercase().as_str()))
    });

    Some(if escritura_acepta_version { "Persistent Structure (full)" } else { "Persistent Structure (partial)" })
}

/// La nota de las tres variantes.
///
/// **Lo primero que dice es la desambiguación, porque es lo más valioso**:
/// acá "persistente" no tiene nada que ver con guardar en disco. La palabra
/// choca de frente con la persistencia de bases de datos, y quien lee
/// "estructura persistente" en un repo adivina mal la mitad de las veces.
pub fn nota_de_persistencia(kind: &str) -> Option<String> {
    let base = "\"Persistente\" acá NO significa \"guardado en disco\" — nada que ver con la persistencia de una base de datos. Significa que las versiones viejas siguen siendo legibles después de una modificación, en vez de que el cambio pise lo anterior (Driscoll, Sarnak, Sleator y Tarjan, 1986). \
                Y no cuesta copiar todo: la técnica es copiar solo el camino de la raíz a la hoja que cambia y compartir el resto, así que un árbol persistente cuesta O(log n) de espacio extra por actualización, no O(n). Por eso la persistencia sale barata en árboles y cara en arreglos.";

    let detalle = match kind {
        "Persistent Structure (partial)" => {
            " Persistencia parcial: se puede LEER cualquier versión pasada, pero solo se modifica la última — las escrituras no aceptan una versión, así que la historia es una línea, no un árbol."
        }
        "Persistent Structure (full)" => {
            " Persistencia total: un método de escritura acepta una versión como argumento, así que se puede modificar DESDE una versión pasada y la historia se bifurca en un árbol en vez de una línea."
        }
        "Retroactive Structure" => {
            " Retroactividad, que NO es persistencia total y conviene no confundirlas. Con persistencia total se puede leer el pasado y bifurcar desde ahí, pero cada rama es una línea temporal aparte y el presente no se entera. Retroactivo significa insertar una operación *en el pasado* y que todas las versiones posteriores cambien: el presente se recalcula. Es estrictamente más fuerte y estrictamente más caro — Demaine, Iacono y Langerman (2007) mostraron que no hay una transformación general que lo dé gratis, y que para algunas estructuras cuesta un factor polinómico más."
        }
        _ => return None,
    };
    Some(format!("{base}{detalle}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::parse_module;

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

    /// **El hecho exacto: a carga optima la tasa de falsos positivos es
    /// `2^-k`, y solo depende de k.** Con 3 hashes no se puede bajar del
    /// 12.5% por mas bits que se le den al arreglo.
    #[test]
    fn el_techo_de_falsos_positivos_depende_solo_de_k() {
        assert!((techo_de_falsos_positivos(3) - 0.125).abs() < 1e-12);
        assert!((techo_de_falsos_positivos(7) - 0.0078125).abs() < 1e-12);
        assert!((techo_de_falsos_positivos(1) - 0.5).abs() < 1e-12);
    }

    /// `n* = m·ln2/k` — cuantos elementos entran antes de pasar ese optimo.
    #[test]
    fn la_capacidad_optima_sale_de_los_bits_y_las_hashes() {
        // 1000 bits, 3 hashes -> 1000 * 0.6931 / 3 = 231
        assert_eq!(capacidad_optima(1000, 3), 231);
    }

    /// Los parametros escritos como defaults de `__init__`, que es la forma
    /// mas comun de escribirlos.
    #[test]
    fn los_parametros_salen_de_los_defaults_del_init() {
        let src = "class BloomFilter:\n    def __init__(self, size=2048, num_hashes=4):\n        self.bits = [0] * size\n    def add(self, x):\n        pass\n    def contains(self, x):\n        return True\n";
        let st = class_stmt(src);
        let cuerpo = class_body(&st);
        let info = data_structure_info("BloomFilter", &class_methods(cuerpo), cuerpo);
        assert_eq!(info.kind, Some("Bloom Filter"));
        assert_eq!(info.bloom_hashes, Some(4));
        assert_eq!(info.bloom_bits, Some(2048));
    }

    /// Y la otra forma: asignados como atributos.
    #[test]
    fn los_parametros_tambien_salen_de_self_x() {
        let src = "class BloomFilter:\n    def __init__(self):\n        self.num_hashes = 3\n        self.size = 1000\n    def add(self, x):\n        pass\n    def contains(self, x):\n        return True\n";
        let st = class_stmt(src);
        let cuerpo = class_body(&st);
        let info = data_structure_info("BloomFilter", &class_methods(cuerpo), cuerpo);
        assert_eq!(info.bloom_hashes, Some(3));
        assert_eq!(info.bloom_bits, Some(1000));
    }

    /// **La nota dice el numero exacto, no una generalidad.** Es la
    /// diferencia entre "tiene una tasa configurable" y "esta tasa no puede
    /// bajar del 12.5%".
    #[test]
    fn la_nota_dice_el_piso_exacto_y_que_mas_bits_no_lo_mejoran() {
        let src = "class BloomFilter:\n    def __init__(self, size=1000, num_hashes=3):\n        self.bits = [0] * size\n    def add(self, x):\n        pass\n    def contains(self, x):\n        return True\n";
        let st = class_stmt(src);
        let cuerpo = class_body(&st);
        let nota = data_structure_note(&data_structure_info("BloomFilter", &class_methods(cuerpo), cuerpo)).unwrap();
        assert!(nota.contains("k=3"), "{nota}");
        assert!(nota.contains("12.50%"), "el piso exacto: {nota}");
        assert!(nota.contains("NO amount of extra bits"), "y que mas bits no ayudan: {nota}");
        assert!(nota.contains("n=231"), "la capacidad en el optimo: {nota}");
    }

    /// Sin parametros literales no se inventa un numero: la nota queda en la
    /// descripcion general, que es la respuesta honesta.
    #[test]
    fn sin_parametros_literales_no_se_afirma_ningun_piso() {
        let src = "class BloomFilter:\n    def __init__(self, size, num_hashes):\n        self.bits = [0] * size\n    def add(self, x):\n        pass\n    def contains(self, x):\n        return True\n";
        let st = class_stmt(src);
        let cuerpo = class_body(&st);
        let info = data_structure_info("BloomFilter", &class_methods(cuerpo), cuerpo);
        assert_eq!(info.bloom_hashes, None);
        let nota = data_structure_note(&info).unwrap();
        assert!(!nota.contains("2^-"), "no se afirma un piso que no se midio: {nota}");
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

    // ── Fase 32: estructuras persistentes y retroactivas ────────────────

    /// `class_methods` no captura argumentos, y la distinción
    /// parcial/total depende justamente de ellos.
    fn metodos_con_args(body: &[Stmt]) -> Vec<RichMethod> {
        body.iter()
            .filter_map(|item| match item {
                Stmt::FunctionDef(f) => Some(RichMethod {
                    name: f.name.to_string(),
                    line: 0,
                    args: f.args.args.iter().map(|a| a.def.arg.to_string()).collect(),
                    is_async: false,
                }),
                _ => None,
            })
            .collect()
    }

    fn persistencia_de(src: &str, nombre: &str) -> Option<&'static str> {
        let stmt = class_stmt(src);
        let body = class_body(&stmt);
        data_structure_info(nombre, &metodos_con_args(body), body).kind
    }

    /// Devolver una instancia nueva de la propia clase es la jugada que
    /// define una estructura persistente.
    #[test]
    fn devolver_una_instancia_nueva_mas_el_nombre_da_persistencia_parcial() {
        let src = "class ListaPersistente:\n    def __init__(self, cabeza=None, cola=None):\n        self.cabeza = cabeza\n        self.cola = cola\n    def push(self, valor):\n        return ListaPersistente(valor, self)\n";
        assert_eq!(persistencia_de(src, "ListaPersistente"), Some("Persistent Structure (partial)"));
    }

    /// Una lista de versiones más una lectura por versión también alcanza,
    /// sin necesidad de que el nombre lo diga.
    #[test]
    fn una_lista_de_versiones_alcanza_sin_que_el_nombre_lo_diga() {
        let src = "class Arbol:\n    def __init__(self):\n        self.versiones = []\n    def insertar(self, clave, valor):\n        self.versiones.append((clave, valor))\n        return len(self.versiones) - 1\n    def leer_version(self, version, clave):\n        return self.versiones[version]\n";
        assert_eq!(persistencia_de(src, "Arbol"), Some("Persistent Structure (partial)"));
    }

    /// **La señal mecánica que separa total de parcial**: que un método de
    /// ESCRITURA acepte una versión. Si se puede escribir desde el pasado,
    /// la historia se bifurca en un árbol en vez de una línea.
    #[test]
    fn una_escritura_que_acepta_version_es_persistencia_total() {
        let src = "class Arbol:\n    def __init__(self):\n        self.versiones = []\n    def insertar(self, version, clave, valor):\n        return len(self.versiones)\n    def leer(self, version, clave):\n        return self.versiones[version]\n";
        assert_eq!(persistencia_de(src, "Arbol"), Some("Persistent Structure (full)"));
    }

    #[test]
    fn el_nombre_retroactivo_se_respeta_y_no_se_deduce() {
        let src = "class ColaRetroactiva:\n    def __init__(self):\n        self.versiones = []\n    def insertar_retroactivo(self, tiempo, x):\n        return x\n";
        assert_eq!(persistencia_de(src, "ColaRetroactiva"), Some("Retroactive Structure"));
    }

    /// **Un nombre sin estructura no es una estructura persistente**, es una
    /// convención de nombre.
    #[test]
    fn un_nombre_inmutable_sin_ninguna_forma_no_se_marca() {
        let src = "class ConfiguracionInmutable:\n    def __init__(self, a):\n        self.a = a\n    def get(self):\n        return self.a\n";
        assert_eq!(persistencia_de(src, "ConfiguracionInmutable"), None);
    }

    /// Y al revés: devolver una instancia nueva tampoco alcanza solo — un
    /// builder encadenado hace exactamente eso.
    #[test]
    fn un_builder_encadenado_no_es_una_estructura_persistente() {
        let src = "class Consulta:\n    def __init__(self, partes=None):\n        self.partes = partes or []\n    def donde(self, c):\n        return Consulta(self.partes + [c])\n";
        assert_eq!(persistencia_de(src, "Consulta"), None);
    }

    /// **La detección de persistencia va ÚLTIMA**: un Trie escrito de forma
    /// inmutable sigue siendo un Trie. La persistencia es cómo se actualiza,
    /// no qué estructura es.
    #[test]
    fn un_trie_inmutable_sigue_reportandose_como_trie() {
        let src = "class TriePersistente:\n    def __init__(self):\n        self.versiones = []\n    def insert(self, palabra):\n        return TriePersistente()\n    def search(self, palabra):\n        return False\n";
        assert_eq!(persistencia_de(src, "TriePersistente"), Some("Trie"));
    }

    /// **La desambiguación es lo más valioso de la nota**: acá
    /// "persistente" no tiene nada que ver con guardar en disco.
    #[test]
    fn la_nota_desambigua_persistente_de_guardado_en_disco() {
        let nota = nota_de_persistencia("Persistent Structure (partial)").unwrap();
        assert!(nota.contains("NO significa"), "{nota}");
        assert!(nota.contains("disco"), "{nota}");
        assert!(nota.contains("O(log n)"), "y dice por qué no cuesta copiar todo: {nota}");
    }

    /// Y la otra: retroactivo **no** es persistencia total.
    #[test]
    fn la_nota_separa_retroactivo_de_persistencia_total() {
        let nota = nota_de_persistencia("Retroactive Structure").unwrap();
        assert!(nota.contains("NO es persistencia total"), "{nota}");
        assert!(nota.contains("todas las versiones posteriores cambien"), "{nota}");
        assert!(nota.contains("Demaine"), "{nota}");
    }

    #[test]
    fn un_kind_que_no_es_de_persistencia_no_produce_esta_nota() {
        assert!(nota_de_persistencia("Trie").is_none());
    }
}
