//! Fase 23 (Execution Intelligence) — "Memory visualizer (stack/heap/data/
//! bss)". El ROADMAP describe este bullet como inspección de un proceso
//! CORRIENDO, no de texto — este módulo NO hace eso: sigue el mismo modelo
//! de seguridad que el resto del motor (lee texto, nunca ejecuta nada del
//! usuario). Reinterpretación de alcance confirmada explícitamente con el
//! usuario, no asumida: clasifica cada variable de C/C++ en stack/heap/data/
//! bss por su DECLARACIÓN en el AST — correcto por construcción dado el
//! storage class (nunca es ambiguo en C/C++), no una heurística adivinada.
//!
//! `"heap"` no es una región de variable — es un allocation site separado.
//! Un puntero (`int *p`) vive en el stack como cualquier otra variable
//! local; lo que vive en el heap es la memoria a la que apunta. Por eso
//! `malloc`/`calloc`/`realloc`/`free` (C) y `new`/`delete` (C++) se reportan
//! en `allocations`, con el nombre de variable resuelto solo en los 2
//! patrones simples y comunes (`T *x = malloc(...)`, `x = malloc(...)`) —
//! sin data-flow real, `variable: None` cuando no calza ninguno.
//!
//! Recibe un `Node` ya parseado (comparte el árbol que `cparse.rs` ya
//! construyó para `/parse/c`/`/parse/cpp` — evita parsear el mismo texto
//! dos veces).

use serde::Serialize;
use tree_sitter::Node;

#[derive(Serialize, Clone)]
pub struct MemoryVariable {
    pub name: String,
    pub region: &'static str, // "stack" | "data" | "bss"
    pub scope: String,        // nombre de función, o "global"
    pub line: usize,
    pub type_hint: String,
}

#[derive(Serialize, Clone)]
pub struct AllocationSite {
    pub kind: &'static str, // "malloc" | "calloc" | "realloc" | "free" | "new" | "delete"
    pub line: usize,
    pub variable: Option<String>,
    /// Solo tiene sentido para `"realloc"` — `true` cuando el patrón es
    /// `p = realloc(p, size)`: si `realloc` falla devuelve `NULL`, y
    /// reasignarlo directo al mismo puntero pisa la única referencia que
    /// quedaba a la memoria original — un leak real y un bug de C
    /// clásico (Fase 25, `modernization.rs` lo usa para el patrón
    /// `unsafe_realloc_reassignment`). `false` para cualquier otro `kind`.
    pub reassigns_same_pointer: bool,
}

#[derive(Serialize)]
pub struct MemoryLayoutSummary {
    pub stack: usize,
    pub heap_allocations: usize,
    pub data: usize,
    pub bss: usize,
}

#[derive(Serialize)]
pub struct MemoryLayoutResult {
    pub variables: Vec<MemoryVariable>,
    pub allocations: Vec<AllocationSite>,
    pub summary: MemoryLayoutSummary,
    pub note: String,
}

const NOTE: &str = "Clasificación por declaración estática (AST) — refleja dónde vive cada variable según el lenguaje C/C++, no una medición de un proceso corriendo. Un puntero clasificado como stack puede apuntar a memoria heap; ver `allocations` para esos sitios.";

const HEAP_ALLOC_FNS: &[&str] = &["malloc", "calloc", "realloc", "free"];

pub(crate) fn text_of<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

pub(crate) fn walk_flat<'a>(node: Node<'a>, f: &mut impl FnMut(Node<'a>)) {
    f(node);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_flat(child, f);
    }
}

/// Encuentra el identificador "de fondo" de un declarator, atravesando
/// `pointer_declarator`/`array_declarator`/`init_declarator`/
/// `parenthesized_declarator`/`reference_declarator` (C++). `None` si el
/// declarator es en realidad un `function_declarator` — un prototipo, no
/// una variable.
fn declarator_name(node: Node, source: &str) -> Option<String> {
    match node.kind() {
        "identifier" | "field_identifier" => Some(text_of(node, source).to_string()),
        "function_declarator" => None,
        "pointer_declarator" | "array_declarator" | "parenthesized_declarator" | "reference_declarator" | "init_declarator" => {
            declarator_name(node.child_by_field_name("declarator")?, source)
        }
        _ => None,
    }
}

fn function_definition_name(node: Node, source: &str) -> String {
    node.child_by_field_name("declarator")
        .and_then(|d| declarator_function_name(d, source))
        .unwrap_or_else(|| "<anonymous>".to_string())
}

fn declarator_function_name(node: Node, source: &str) -> Option<String> {
    match node.kind() {
        "identifier" | "field_identifier" | "qualified_identifier" | "destructor_name" | "operator_name" => Some(text_of(node, source).to_string()),
        "function_declarator" | "pointer_declarator" | "reference_declarator" | "parenthesized_declarator" => {
            declarator_function_name(node.child_by_field_name("declarator")?, source)
        }
        _ => None,
    }
}

fn storage_class(node: Node, source: &str) -> (bool, bool) {
    // (is_static, is_extern)
    let mut is_static = false;
    let mut is_extern = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "storage_class_specifier" {
            match text_of(child, source) {
                "static" => is_static = true,
                "extern" => is_extern = true,
                _ => {}
            }
        }
    }
    (is_static, is_extern)
}

fn classify_declaration(node: Node, source: &str, scope: Option<&str>, out: &mut Vec<MemoryVariable>) {
    let (is_static, is_extern) = storage_class(node, source);
    if is_extern {
        return; // no reserva storage en esta unidad de traducción
    }
    let type_hint = node.child_by_field_name("type").map(|t| text_of(t, source).to_string()).unwrap_or_else(|| "?".to_string());
    let line = node.start_position().row + 1;
    let mut cursor = node.walk();
    for declarator in node.children_by_field_name("declarator", &mut cursor) {
        let Some(name) = declarator_name(declarator, source) else { continue };
        let has_initializer = declarator.kind() == "init_declarator";
        let region: &'static str = if scope.is_none() || is_static {
            if has_initializer {
                "data"
            } else {
                "bss"
            }
        } else {
            "stack"
        };
        out.push(MemoryVariable {
            name,
            region,
            scope: scope.map(|s| s.to_string()).unwrap_or_else(|| "global".to_string()),
            line,
            type_hint: type_hint.clone(),
        });
    }
}

fn collect_parameters(func_declarator: Node, source: &str, scope: &str, out: &mut Vec<MemoryVariable>) {
    walk_flat(func_declarator, &mut |n| {
        if n.kind() != "parameter_declaration" {
            return;
        }
        let type_hint = n.child_by_field_name("type").map(|t| text_of(t, source).to_string()).unwrap_or_else(|| "?".to_string());
        if let Some(d) = n.child_by_field_name("declarator") {
            if let Some(name) = declarator_name(d, source) {
                out.push(MemoryVariable { name, region: "stack", scope: scope.to_string(), line: n.start_position().row + 1, type_hint });
            }
        }
    });
}

fn walk_variables(node: Node, source: &str, scope: Option<&str>, out: &mut Vec<MemoryVariable>) {
    match node.kind() {
        "function_definition" => {
            let name = function_definition_name(node, source);
            if let Some(declarator) = node.child_by_field_name("declarator") {
                collect_parameters(declarator, source, &name, out);
            }
            if let Some(body) = node.child_by_field_name("body") {
                walk_variables(body, source, Some(&name), out);
            }
            return;
        }
        "declaration" => {
            classify_declaration(node, source, scope, out);
            return;
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_variables(child, source, scope, out);
    }
}

/// Best-effort: resuelve el nombre de variable de un sitio de allocation
/// solo cuando el patrón es uno de los 2 más comunes — `T *x = malloc(...)`
/// (el nodo es el `value` de un `init_declarator`) o `x = malloc(...)` (el
/// nodo es el `right` de una `assignment_expression`). Sin estos dos casos,
/// `None` — no se inventa un valor vía data-flow que este módulo no rastrea.
fn resolve_target_variable(node: Node, source: &str) -> Option<String> {
    let parent = node.parent()?;
    match parent.kind() {
        "init_declarator" => declarator_name(parent.child_by_field_name("declarator")?, source),
        "assignment_expression" => {
            let left = parent.child_by_field_name("left")?;
            (left.kind() == "identifier").then(|| text_of(left, source).to_string())
        }
        _ => None,
    }
}

/// Para `free(p)`: el primer identificador dentro de la `argument_list` del
/// `call_expression` — a diferencia de `resolve_target_variable`, acá la
/// variable de interés es el argumento, no algo que el call asigne.
fn first_argument_identifier(call_node: Node, source: &str) -> Option<String> {
    let args = call_node.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let found = args.children(&mut cursor).find(|c| c.kind() == "identifier").map(|c| text_of(c, source).to_string());
    found
}

fn extract_allocations(root: Node, source: &str) -> Vec<AllocationSite> {
    let mut out = Vec::new();
    walk_flat(root, &mut |n| match n.kind() {
        "call_expression" => {
            let Some(func) = n.child_by_field_name("function") else { return };
            if func.kind() != "identifier" {
                return;
            }
            let name = text_of(func, source);
            let Some(kind) = HEAP_ALLOC_FNS.iter().find(|k| **k == name) else { return };
            // `free(p)` no asigna nada — la variable liberada es el propio
            // argumento, no algo resuelto vía el padre del call_expression
            // (eso es lo que hace `resolve_target_variable`, para malloc/
            // calloc/realloc, donde SÍ hay una asignación de por medio).
            let variable = if *kind == "free" { first_argument_identifier(n, source) } else { resolve_target_variable(n, source) };
            // `p = realloc(p, size)` — el primer argumento de `realloc` es
            // el MISMO nombre que la variable a la que se reasigna el
            // resultado. Si `realloc` falla y devuelve `NULL`, esa
            // reasignación pisa la única referencia que quedaba a la
            // memoria original — leak real, no solo un candidato de
            // modernización (ver `modernization.rs`).
            let reassigns_same_pointer = *kind == "realloc" && variable.is_some() && first_argument_identifier(n, source) == variable;
            out.push(AllocationSite { kind, line: n.start_position().row + 1, variable, reassigns_same_pointer });
        }
        "new_expression" => {
            out.push(AllocationSite { kind: "new", line: n.start_position().row + 1, variable: resolve_target_variable(n, source), reassigns_same_pointer: false });
        }
        "delete_expression" => {
            let mut cursor = n.walk();
            let variable = n.children(&mut cursor).find(|c| c.kind() == "identifier").map(|c| text_of(c, source).to_string());
            out.push(AllocationSite { kind: "delete", line: n.start_position().row + 1, variable, reassigns_same_pointer: false });
        }
        _ => {}
    });
    out
}

/// Punto de entrada — recibe el `Node` raíz que `cparse.rs` ya parseó (nunca
/// vuelve a parsear el mismo texto). Sirve tanto para C como para C++: los
/// node kinds que mira son comunes a ambas gramáticas, más `new_expression`/
/// `delete_expression` (propios de C++, simplemente nunca aparecen al
/// parsear con la gramática C).
pub fn build(root: Node, source: &str) -> MemoryLayoutResult {
    let mut variables = Vec::new();
    walk_variables(root, source, None, &mut variables);
    let allocations = extract_allocations(root, source);

    let mut summary = MemoryLayoutSummary { stack: 0, heap_allocations: allocations.len(), data: 0, bss: 0 };
    for v in &variables {
        match v.region {
            "stack" => summary.stack += 1,
            "data" => summary.data += 1,
            "bss" => summary.bss += 1,
            _ => {}
        }
    }

    MemoryLayoutResult { variables, allocations, summary, note: NOTE.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::{Parser, Tree};

    fn parse_c(src: &str) -> Tree {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_c::LANGUAGE.into()).unwrap();
        parser.parse(src, None).unwrap()
    }

    fn parse_cpp(src: &str) -> Tree {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_cpp::LANGUAGE.into()).unwrap();
        parser.parse(src, None).unwrap()
    }

    #[test]
    fn global_con_inicializador_es_data() {
        let src = "int x = 5;\n";
        let tree = parse_c(src);
        let r = build(tree.root_node(), src);
        assert_eq!(r.variables.len(), 1);
        assert_eq!(r.variables[0].name, "x");
        assert_eq!(r.variables[0].region, "data");
        assert_eq!(r.variables[0].scope, "global");
    }

    #[test]
    fn global_sin_inicializador_es_bss() {
        let src = "int y;\n";
        let tree = parse_c(src);
        let r = build(tree.root_node(), src);
        assert_eq!(r.variables[0].region, "bss");
    }

    #[test]
    fn local_sin_static_es_stack() {
        let src = "void f() {\n    int a = 1;\n}\n";
        let tree = parse_c(src);
        let r = build(tree.root_node(), src);
        let a = r.variables.iter().find(|v| v.name == "a").unwrap();
        assert_eq!(a.region, "stack");
        assert_eq!(a.scope, "f");
    }

    #[test]
    fn local_static_con_inicializador_es_data() {
        let src = "void f() {\n    static int counter = 0;\n}\n";
        let tree = parse_c(src);
        let r = build(tree.root_node(), src);
        let c = r.variables.iter().find(|v| v.name == "counter").unwrap();
        assert_eq!(c.region, "data");
    }

    #[test]
    fn local_static_sin_inicializador_es_bss() {
        let src = "void f() {\n    static int counter;\n}\n";
        let tree = parse_c(src);
        let r = build(tree.root_node(), src);
        let c = r.variables.iter().find(|v| v.name == "counter").unwrap();
        assert_eq!(c.region, "bss");
    }

    #[test]
    fn parametro_es_stack() {
        let src = "int add(int a, int b) {\n    return a + b;\n}\n";
        let tree = parse_c(src);
        let r = build(tree.root_node(), src);
        let a = r.variables.iter().find(|v| v.name == "a").unwrap();
        assert_eq!(a.region, "stack");
        assert_eq!(a.scope, "add");
        assert!(r.variables.iter().any(|v| v.name == "b"));
    }

    #[test]
    fn extern_se_omite() {
        let src = "extern int shared_counter;\n";
        let tree = parse_c(src);
        let r = build(tree.root_node(), src);
        assert!(r.variables.is_empty());
    }

    #[test]
    fn prototipo_de_funcion_no_es_variable() {
        let src = "int add(int, int);\n";
        let tree = parse_c(src);
        let r = build(tree.root_node(), src);
        assert!(r.variables.is_empty());
    }

    #[test]
    fn declaracion_multiple_produce_dos_variables() {
        let src = "int a, b;\n";
        let tree = parse_c(src);
        let r = build(tree.root_node(), src);
        assert_eq!(r.variables.len(), 2);
        assert!(r.variables.iter().any(|v| v.name == "a"));
        assert!(r.variables.iter().any(|v| v.name == "b"));
    }

    #[test]
    fn malloc_con_declarator_resuelve_variable() {
        let src = "void f() {\n    int *p = malloc(sizeof(int));\n}\n";
        let tree = parse_c(src);
        let r = build(tree.root_node(), src);
        assert_eq!(r.allocations.len(), 1);
        assert_eq!(r.allocations[0].kind, "malloc");
        assert_eq!(r.allocations[0].variable.as_deref(), Some("p"));
        // `p` en sí es una variable STACK — el puntero, no la memoria apuntada.
        let p = r.variables.iter().find(|v| v.name == "p").unwrap();
        assert_eq!(p.region, "stack");
    }

    #[test]
    fn malloc_con_asignacion_suelta_resuelve_variable() {
        let src = "void f() {\n    int *p;\n    p = malloc(sizeof(int));\n}\n";
        let tree = parse_c(src);
        let r = build(tree.root_node(), src);
        let m = r.allocations.iter().find(|a| a.kind == "malloc").unwrap();
        assert_eq!(m.variable.as_deref(), Some("p"));
    }

    #[test]
    fn realloc_reasignado_al_mismo_puntero_se_marca() {
        let src = "void f() {\n    p = realloc(p, newsize);\n}\n";
        let tree = parse_c(src);
        let r = build(tree.root_node(), src);
        let re = r.allocations.iter().find(|a| a.kind == "realloc").unwrap();
        assert_eq!(re.variable.as_deref(), Some("p"));
        assert!(re.reassigns_same_pointer);
    }

    #[test]
    fn realloc_a_variable_temporal_no_se_marca() {
        let src = "void f() {\n    tmp = realloc(p, newsize);\n}\n";
        let tree = parse_c(src);
        let r = build(tree.root_node(), src);
        let re = r.allocations.iter().find(|a| a.kind == "realloc").unwrap();
        assert_eq!(re.variable.as_deref(), Some("tmp"));
        assert!(!re.reassigns_same_pointer);
    }

    #[test]
    fn malloc_sin_patron_resoluble_da_none() {
        let src = "void f() {\n    do_something(malloc(10));\n}\n";
        let tree = parse_c(src);
        let r = build(tree.root_node(), src);
        assert_eq!(r.allocations[0].variable, None);
    }

    #[test]
    fn free_resuelve_variable() {
        let src = "void f() {\n    free(p);\n}\n";
        let tree = parse_c(src);
        let r = build(tree.root_node(), src);
        assert_eq!(r.allocations.len(), 1);
        assert_eq!(r.allocations[0].kind, "free");
        assert_eq!(r.allocations[0].variable.as_deref(), Some("p"));
    }

    #[test]
    fn new_y_delete_en_cpp() {
        let src = "void f() {\n    int *p = new int(5);\n    delete p;\n}\n";
        let tree = parse_cpp(src);
        let r = build(tree.root_node(), src);
        assert!(r.allocations.iter().any(|a| a.kind == "new" && a.variable.as_deref() == Some("p")));
        assert!(r.allocations.iter().any(|a| a.kind == "delete" && a.variable.as_deref() == Some("p")));
    }

    #[test]
    fn end_to_end_resumen_cuenta_bien_las_regiones() {
        let src = "int global_init = 1;\nint global_uninit;\nvoid f(int param) {\n    int local = 2;\n    static int local_static;\n    int *heap_ptr = malloc(4);\n}\n";
        let tree = parse_c(src);
        let r = build(tree.root_node(), src);
        assert_eq!(r.summary.data, 1); // global_init
        assert_eq!(r.summary.bss, 2); // global_uninit + local_static
        assert_eq!(r.summary.stack, 3); // param, local, heap_ptr
        assert_eq!(r.summary.heap_allocations, 1);
    }
}
