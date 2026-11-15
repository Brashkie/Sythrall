//! Helpers compartidos sobre nodos de tree-sitter.
//!
//! Nace de una limpieza (2026-09-05), no de una feature: estos helpers
//! estaban repetidos por el crate y una de las copias era un riesgo real de
//! correctitud.
//!
//! - `text_of` existía **tres veces**, idéntica, en `cparse.rs`, `fparse.rs`
//!   y `memlayout.rs`.
//! - `is_string_literal` existía **dos veces**: como tal en `csecurity.rs` y
//!   como `is_literal_format_arg` en `modernization.rs`. Esa es la copia
//!   peligrosa: la de `modernization.rs` lleva un doc-comment sobre un falso
//!   positivo real que costó encontrar (le faltaba `raw_string_literal`, así
//!   que `printf(R"(...)", user)` se marcaba como formato no literal). La
//!   segunda copia nació ya corregida por casualidad, porque se escribió
//!   copiando la primera — pero la próxima corrección a una no habría
//!   llegado a la otra. Un predicado, un lugar.
//! - `walk_flat` y `function_definition_name` vivían en `memlayout.rs` como
//!   `pub(crate)`. Funcionaba, pero un módulo sobre disposición de memoria
//!   no es el lugar de las utilidades de recorrido que usan otros cuatro.
//!
//! `text_of` y `walk_flat` son **agnósticas del lenguaje** (cualquier árbol
//! de tree-sitter). `is_string_literal` y `function_definition_name` nombran
//! nodos de la gramática **C/C++** concretamente, y está dicho en cada una.

use tree_sitter::Node;

/// Texto fuente de un nodo. `""` cuando el rango no es UTF-8 válido, en vez
/// de propagar un `Result` que ningún caller de este crate podría hacer otra
/// cosa que ignorar.
pub(crate) fn text_of<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

/// Recorre TODO el subárbol en preorden, sin cortar por scope. El nombre lo
/// dice: es plano a propósito — los recorridos con límite de scope
/// (`cparse::walk_own_scope`, `walk::walk_stmts_own_scope`) son otra cosa y
/// existen aparte.
///
/// **Un solo cursor para todo el árbol, no uno por nodo.** La versión
/// anterior era recursiva y llamaba a `node.walk()` en CADA nodo, o sea que
/// asignaba un `TreeCursor` por nodo visitado. Medido (2026-09-05): el
/// pipeline de C/C++ hace ~15 recorridos completos del mismo árbol entre
/// `cparse`, `memlayout`, `modernization` y `csecurity`, y escanear 10
/// archivos `.c` tardaba **93 ms contra 11 ms de la misma cantidad de código
/// Python o JS** — 8× más lento sin ninguna razón estructural. La causa era
/// esta función, no los análisis. `TreeCursor` sabe navegar el árbol entero
/// (`goto_first_child`/`goto_next_sibling`/`goto_parent`), así que uno solo
/// alcanza. Mismo orden de visita, misma cobertura.
pub(crate) fn walk_flat<'a>(node: Node<'a>, f: &mut impl FnMut(Node<'a>)) {
    let mut cursor = node.walk();
    // Profundidad relativa al nodo de arranque: sin esto, `goto_parent`
    // treparía POR ENCIMA de `node` y recorrería el resto del archivo.
    let mut profundidad = 0usize;
    loop {
        f(cursor.node());
        if cursor.goto_first_child() {
            profundidad += 1;
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if profundidad == 0 || !cursor.goto_parent() {
                return;
            }
            profundidad -= 1;
        }
    }
}

/// **C/C++.** `true` si el nodo es un literal de string en cualquiera de sus
/// tres formas gramaticales.
///
/// Las tres son nodos HERMANOS en la gramática, no subtipos de
/// `string_literal`, y olvidar una es un falso positivo silencioso: cuando
/// faltaba `raw_string_literal`, `printf(R"(Hello %s)", user)` —un literal
/// perfectamente fijo, justo lo que `-Wformat-nonliteral` de gcc/clang NO
/// marca— se reportaba como formato no literal. `raw_string_literal` es
/// sintaxis de C++11 y ese nodo nunca aparece en un árbol de tree-sitter-c,
/// así que incluirlo es inofensivo para los callers de C.
pub(crate) fn is_string_literal(node: Node) -> bool {
    matches!(node.kind(), "string_literal" | "concatenated_string" | "raw_string_literal")
}

/// **C/C++.** Nombre de la función que define este `function_definition`,
/// atravesando los declarators que la envuelven (`int *f()` mete un
/// `pointer_declarator` en el medio).
pub(crate) fn function_definition_name(node: Node, source: &str) -> String {
    node.child_by_field_name("declarator")
        .and_then(|d| declarator_function_name(d, source))
        .unwrap_or_else(|| "<anonymous>".to_string())
}

fn declarator_function_name(node: Node, source: &str) -> Option<String> {
    match node.kind() {
        "identifier" | "field_identifier" | "qualified_identifier" | "destructor_name" | "operator_name" => {
            Some(text_of(node, source).to_string())
        }
        "function_declarator" | "pointer_declarator" | "reference_declarator" | "parenthesized_declarator" => {
            declarator_function_name(node.child_by_field_name("declarator")?, source)
        }
        _ => None,
    }
}
