//! Parser C/C++ — puerto de `static_parser.py::_parse_c`/`_parse_cpp` y sus
//! helpers `_ts_extract_*`/`_extract_calls_c`/`_infer_big_o_c`. Mismo motor
//! subyacente que el Python que reemplaza (tree-sitter, mismas gramáticas
//! C/C++), ahora vía los bindings nativos de Rust (`tree-sitter`/
//! `tree-sitter-c`/`tree-sitter-cpp`) en vez de los bindings Python — mismos
//! tipos de nodo (`function_definition`/`preproc_include`/
//! `struct_specifier`/`class_specifier`/`preproc_def`/`call_expression`),
//! mismo recorrido recursivo, misma heurística de Big-O/complejidad.

use serde::Serialize;
use tree_sitter::{Node, Parser};

use crate::flujo::graph::{self, CallEdge};
use crate::ast::tsnode::{self, text_of};

use crate::hallazgos::memlayout::MemoryLayoutResult;
use crate::hallazgos::modernization::ModernizationReport;
use crate::hallazgos::csecurity;
use crate::hallazgos::locality;
use crate::hallazgos::security::SecurityFinding;
use crate::hallazgos::smells::{self, StructuralSmell};
use crate::hallazgos::wasm::WasmHint;

#[derive(Serialize, Clone)]
pub struct CFunction {
    pub name: String,
    pub line: usize,
    pub end_line: usize,
    pub loc: usize,
    pub complexity: u32,
    pub big_o: String,
    pub big_o_reason: String,
    pub calls: Vec<String>,
    /// Fase 17 (Compiler Intelligence), segundo bullet, 2026-09-02: `true`
    /// si esta función tiene un `return f(...)` que ES exactamente su
    /// propia auto-llamada (nada pendiente después) — la misma forma que
    /// `recursion.rs::calls_self` ya detecta para Python, portada a
    /// tree-sitter. A diferencia de Python (que nunca hace esta
    /// optimización, ver Fase 8), un compilador C/C++ real con `-O1`+
    /// PUEDE reescribir esto como un loop sin costo de stack (sibling call
    /// optimization) — ver `recursion_note` para el hedge completo, y el
    /// botón de Compiler Explorer del frontend para confirmarlo en el
    /// ensamblador real en vez de asumirlo.
    pub is_tail_recursive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recursion_note: Option<String>,
}

/// Camina el subárbol de `node`, pero NO desciende a `lambda_expression`/
/// `function_definition`/`class_specifier`/`struct_specifier`/
/// `union_specifier` anidados — cada uno es su propio scope de nombres, así
/// que una auto-llamada encontrada ahí adentro no es evidencia de que la
/// función CONTENEDORA se llame a sí misma. Bug real encontrado en
/// auditoría (2026-09-02): `is_tail_recursive` usaba el `tsnode::walk_flat()` genérico
/// de arriba, sin ningún límite de scope — `auto helper = []() { return
/// total(5); };` dentro del cuerpo de `total` (una lambda LOCAL, nunca su
/// propia auto-llamada) hacía que `total` — que en los hechos nunca se
/// llama a sí misma, solo tiene `return 0;` — se marcara `is_tail_recursive:
/// true` por error. Mismo riesgo con un `struct`/`class` local cuyo método
/// comparta nombre con la función contenedora.
fn walk_own_scope<'a>(node: Node<'a>, f: &mut impl FnMut(Node<'a>)) {
    f(node);
    if matches!(node.kind(), "lambda_expression" | "function_definition" | "class_specifier" | "struct_specifier" | "union_specifier") {
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_own_scope(child, f);
    }
}

/// `true` si `n` es un `return_statement` cuyo único valor ES exactamente
/// una auto-llamada a `own_name` — no envuelta en nada (`return f(n-1) + 1`
/// no cuenta, mismo criterio que `calls_self` de Python). Camina TODO el
/// subárbol del cuerpo de la función (no solo el nivel superior), así que
/// una auto-llamada tail-call dentro de un `if`/`else` sigue contando —
/// pero no dentro de una lambda/función/clase local anidada, ver
/// `walk_own_scope`.
fn is_tail_recursive(body: Node, source: &str, own_name: &str) -> bool {
    let mut found = false;
    walk_own_scope(body, &mut |n| {
        if found || n.kind() != "return_statement" {
            return;
        }
        let Some(value) = n.named_child(0) else { return };
        if value.kind() != "call_expression" {
            return;
        }
        let Some(func) = value.child_by_field_name("function") else { return };
        if func.kind() == "identifier" && text_of(func, source) == own_name {
            found = true;
        }
    });
    found
}

fn recursion_note(is_tail_recursive: bool) -> Option<String> {
    if !is_tail_recursive {
        return None;
    }
    Some(
        "Tail-call — el return ES exactamente la auto-llamada, sin nada pendiente después. Con optimización \
         (-O1 o superior en gcc/clang) el compilador PUEDE reescribir esto como un loop sin costo de stack \
         (sibling call optimization) — a diferencia de Python, que nunca hace esta optimización. Esto es una \
         forma que lo permite, no una garantía de que el compilador lo haga: usá el visualizador de Compiler \
         Explorer (con -O2) para confirmarlo en el ensamblador real generado en vez de asumirlo."
            .to_string(),
    )
}

#[derive(Serialize, Clone)]
pub struct CInclude {
    pub module: String,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub line: usize,
}

#[derive(Serialize, Clone)]
pub struct CStruct {
    pub name: String,
    pub line: usize,
    pub kind: String,
}

#[derive(Serialize, Clone)]
pub struct CMacro {
    pub name: String,
    pub line: usize,
}

#[derive(Serialize)]
pub struct CParseResult {
    pub functions: Vec<CFunction>,
    /// Structs/unions para C, clases para C++ — mismo campo `classes` que
    /// ya usa el shape de `_parse_c`/`_parse_cpp` en Python.
    pub classes: Vec<CStruct>,
    pub imports: Vec<CInclude>,
    pub macros: Vec<CMacro>,
    pub call_graph: Vec<CallEdge>,
    pub wasm_hints: Vec<WasmHint>,
    /// Fase 23 — clasificación estática stack/heap/data/bss, ver
    /// `memlayout.rs`. Comparte el mismo árbol ya parseado acá, no vuelve a
    /// parsear el texto.
    pub memory: MemoryLayoutResult,
    /// Fase 25 (Modernization Intelligence) — candidatos de modernización
    /// derivados de `memory.allocations`, ver `modernization.rs`. Cero
    /// cómputo nuevo de AST, pura reinterpretación de lo que `memlayout.rs`
    /// ya calculó en esta misma llamada.
    pub modernization: ModernizationReport,
    /// Fase 22 (Quality Intelligence), 2026-09-05 — hasta acá este campo no
    /// existía y tanto `static_parser.py` como `scanner.rs` mandaban `[]`
    /// fijo para C/C++, así que un proyecto de puro C sacaba **Quality
    /// 100/100 por ausencia de análisis**. Exactamente el mismo bug que ya
    /// se había arreglado para JS/TS. Ver `structural_smells_c`.
    pub structural_smells: Vec<StructuralSmell>,
    /// Fase 21 (Security & Taint Intelligence), 2026-09-05 — hasta acá esto
    /// era `[]` fijo y un proyecto de puro C sacaba **Security 100/100 por
    /// ausencia de análisis**. Ver `csecurity.rs`, incluyendo por qué
    /// deliberadamente NO repite los CWEs que `modernization.rs` ya reporta.
    pub security_findings: Vec<SecurityFinding>,
    /// Fase 32 (2026-09-05) — localidad espacial de acceso. Ver `locality.rs`.
    pub locality: Vec<locality::LocalityFinding>,
    /// Fase 34, Línea A — métricas de FORMA de la metaprogramación de
    /// C++ (`plantillas.rs`). Default vacío para C, que no tiene
    /// templates. Ver el doc de `TemplateComplexity`: son cuentas de lo
    /// ESCRITO, nunca una predicción del trabajo del compilador.
    pub templates: crate::costo::plantillas::TemplateComplexity,
}

/// El nombre de una funcion, bajando por el campo `declarator`.
///
/// **El recorrido por hijos en orden se equivocaba con C++ moderno.** En
/// `std::string f(int n)` el PRIMER hijo de `function_definition` es el tipo
/// de retorno, y su kind es `qualified_identifier` — el mismo que el de un
/// nombre calificado. La version anterior aceptaba ese kind y devolvia
/// `std::string` como nombre de la funcion, asi que toda funcion de C++ que
/// devuelve un tipo de `std::` aparecia en el panel con el nombre de su tipo
/// de retorno. Con dos de ellas en un archivo, las dos se llamaban igual.
///
/// La gramatica ya distingue las dos cosas: el tipo de retorno esta en el
/// campo `type` y el nombre baja por `declarator`. Seguir el campo, y no el
/// orden, es lo que separa uno de otro.
fn func_name(node: Node, source: &str) -> String {
    fn nombre_en(n: Node, source: &str) -> Option<String> {
        // El nombre final: `identifier` (libre), `field_identifier` (metodo
        // definido dentro de la clase) o `qualified_identifier`
        // (`Clase::metodo` definido afuera) — este ultimo SI es un nombre.
        if matches!(n.kind(), "identifier" | "field_identifier" | "qualified_identifier" | "destructor_name" | "operator_name") {
            return Some(text_of(n, source).to_string());
        }
        // `function_declarator`, `pointer_declarator`, `reference_declarator`,
        // `parenthesized_declarator`: todos envuelven al de adentro.
        if let Some(d) = n.child_by_field_name("declarator") {
            return nombre_en(d, source);
        }
        // **`reference_declarator` no expone su interior como CAMPO**, a
        // diferencia de `pointer_declarator`: en `int &ref(int)` seguir solo
        // el campo devolvia `<anonymous>`. Lo encontro un test que esperaba
        // que pasara. Se baja por los hijos nombrados como respaldo.
        let mut c = n.walk();
        let hijos: Vec<Node> = n.named_children(&mut c).collect();
        hijos.iter().find_map(|h| nombre_en(*h, source))
    }
    node.child_by_field_name("declarator").and_then(|d| nombre_en(d, source)).unwrap_or_else(|| "<anonymous>".to_string())
}

fn extract_calls(func_node: Node, source: &str) -> Vec<String> {
    let mut calls = std::collections::HashSet::new();
    tsnode::walk_flat(func_node, &mut |n| {
        if n.kind() == "call_expression" {
            let mut cursor = n.walk();
            for child in n.children(&mut cursor) {
                if child.kind() == "identifier" {
                    calls.insert(text_of(child, source).to_string());
                    break;
                }
            }
        }
    });
    calls.into_iter().collect()
}

fn extract_functions(root: Node, source: &str) -> Vec<CFunction> {
    let mut functions = Vec::new();
    tsnode::walk_flat(root, &mut |node| {
        if node.kind() == "function_definition" {
            let name = func_name(node, source);
            let start = node.start_position().row + 1;
            let end = node.end_position().row + 1;
            let loc = end - start + 1;
            // Leida del ARBOL, no del texto: contar `" if "` sobre el
            // cuerpo contaba las keywords que aparecian en comentarios y en
            // cadenas, y se perdia `if(x)` sin espacio, `a&&b`, el ternario
            // y los `catch`. Ver `ccosto::ciclomatica`.
            let cc = crate::costo::ccosto::ciclomatica(node);

            let (big_o, big_o_reason) = crate::costo::ccosto::inferir(node, root, source);
            let tail_recursive = node.child_by_field_name("body").is_some_and(|b| is_tail_recursive(b, source, &name));

            functions.push(CFunction {
                name,
                line: start,
                end_line: end,
                loc,
                complexity: cc,
                big_o,
                big_o_reason,
                calls: extract_calls(node, source),
                is_tail_recursive: tail_recursive,
                recursion_note: recursion_note(tail_recursive),
            });
        }
    });
    functions
}

// ─────────────────── Smells estructurales de C/C++ (Fase 22) ──────────────
//
// Hasta acá `structural_smells` era `[]` fijo para C/C++, así que un proyecto
// de puro C sacaba **Quality 100/100 por ausencia de análisis** — exactamente
// el mismo bug que las Fases 21 y 22 arreglaron para JS/TS y que la auditoría
// del scanner encontró todavía vivo en la ruta de proyecto.
//
// Reusa los mismos `kind` que ya emiten Python y JS/TS (`long_function`,
// `excessive_parameters`, `deep_nesting`), así que el frontend no necesita
// ninguna etiqueta nueva. `large_class`/`god_object` quedan afuera a
// propósito: `CStruct` guarda solo nombre/línea/tipo, sin conteo de miembros
// ni de métodos, y estimarlos sería inventar un número.

/// Cantidad de parámetros declarados. Busca el `function_declarator` en el
/// subárbol (no solo entre los hijos directos, porque `int *f(int a)` lo
/// envuelve en un `pointer_declarator`) y cuenta sus `parameter_declaration`.
/// `void f(void)` da 0, no 1: `void` como única declaración es la forma de C
/// de decir "sin parámetros", y contarla dispararía el smell un parámetro
/// antes de tiempo en todo archivo C viejo. `...` es un `variadic_parameter`,
/// que tampoco es un parámetro declarado.
fn param_count(node: Node, source: &str) -> usize {
    fn find_declarator<'a>(n: Node<'a>) -> Option<Node<'a>> {
        if n.kind() == "function_declarator" {
            return Some(n);
        }
        let mut cursor = n.walk();
        let children: Vec<Node<'a>> = n.children(&mut cursor).collect();
        children.into_iter().find_map(find_declarator)
    }
    let Some(decl) = node.child_by_field_name("declarator").and_then(find_declarator) else {
        return 0;
    };
    let Some(params) = decl.child_by_field_name("parameters") else { return 0 };
    let mut cursor = params.walk();
    params
        .children(&mut cursor)
        .filter(|c| c.kind() == "parameter_declaration")
        .filter(|c| text_of(*c, source).trim() != "void")
        .count()
}

/// Profundidad máxima de bloques anidados dentro del cuerpo de una función.
///
/// **Una cadena `else if` NO cuenta como anidamiento**, y esa es una
/// diferencia deliberada con el motor de Python: sintácticamente el segundo
/// `if` cuelga del primero (acá, `else_clause > if_statement` — confirmado
/// contra el árbol real de tree-sitter antes de escribir esto, no asumido),
/// pero semánticamente es un despacho plano. Contarlo como anidamiento haría
/// que un `switch` escrito con 4 `else if` — una forma completamente normal
/// en C — se reportara como "5 niveles de bloques anidados", que es una
/// afirmación falsa, no una opinión de estilo.
fn max_nesting_depth_c(body: Node) -> u32 {
    fn nesting_kind(kind: &str) -> bool {
        matches!(kind, "if_statement" | "for_statement" | "while_statement" | "do_statement" | "switch_statement")
    }
    fn walk_depth(n: Node, depth: u32, max_depth: &mut u32) {
        // Un cuerpo anidado es su propio scope: su anidamiento interno no es
        // anidamiento de la función que lo contiene.
        if matches!(n.kind(), "function_definition" | "lambda_expression") {
            return;
        }
        if n.kind() == "if_statement" {
            let d = depth + 1;
            *max_depth = (*max_depth).max(d);
            if let Some(cons) = n.child_by_field_name("consequence") {
                walk_depth(cons, d, max_depth);
            }
            if let Some(alt) = n.child_by_field_name("alternative") {
                let mut cursor = alt.walk();
                let inner: Vec<Node> = alt.children(&mut cursor).filter(|c| c.is_named()).collect();
                // `else if`: se sigue la cadena a la MISMA profundidad que
                // este `if`, no una más adentro.
                if inner.len() == 1 && inner[0].kind() == "if_statement" {
                    walk_depth(inner[0], depth, max_depth);
                } else {
                    for c in inner {
                        walk_depth(c, d, max_depth);
                    }
                }
            }
            return;
        }
        let d = if nesting_kind(n.kind()) { depth + 1 } else { depth };
        *max_depth = (*max_depth).max(d);
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            if child.is_named() {
                walk_depth(child, d, max_depth);
            }
        }
    }
    let mut max_depth = 0u32;
    walk_depth(body, 0, &mut max_depth);
    max_depth
}

fn structural_smells_c(root: Node, source: &str) -> Vec<StructuralSmell> {
    let mut out = Vec::new();
    tsnode::walk_flat(root, &mut |node| {
        if node.kind() != "function_definition" {
            return;
        }
        let name = func_name(node, source);
        let line = node.start_position().row + 1;
        let loc = node.end_position().row + 1 - line + 1;
        out.extend(smells::check_long_function(&name, line, loc));
        out.extend(smells::check_excessive_parameters(&name, line, param_count(node, source)));
        if let Some(body) = node.child_by_field_name("body") {
            let depth = max_nesting_depth_c(body);
            if depth > smells::DEEP_NESTING_DEPTH {
                out.push(StructuralSmell {
                    kind: "deep_nesting",
                    name: name.clone(),
                    line,
                    message: format!(
                        "{depth} levels of nested blocks (> {}) — consider extracting inner blocks into helper functions or using early returns",
                        smells::DEEP_NESTING_DEPTH
                    ),
                });
            }
        }
    });
    out.sort_by_key(|s| s.line);
    out
}

fn extract_includes(root: Node, source: &str) -> Vec<CInclude> {
    let mut includes = Vec::new();
    tsnode::walk_flat(root, &mut |node| {
        if node.kind() == "preproc_include" {
            let mut cursor = node.walk();
            let path_node = node.children(&mut cursor).find(|c| c.kind() == "string_literal" || c.kind() == "system_lib_string");
            if let Some(p) = path_node {
                let raw = text_of(p, source).trim_matches(|c| c == '"' || c == '<' || c == '>').to_string();
                includes.push(CInclude { module: raw, kind: "include", line: node.start_position().row + 1 });
            }
        }
    });
    includes
}

fn extract_structs(root: Node, source: &str) -> Vec<CStruct> {
    let mut structs = Vec::new();
    tsnode::walk_flat(root, &mut |node| {
        if node.kind() == "struct_specifier" || node.kind() == "union_specifier" {
            let mut cursor = node.walk();
            let name_node = node.children(&mut cursor).find(|c| c.kind() == "type_identifier");
            if let Some(n) = name_node {
                structs.push(CStruct {
                    name: text_of(n, source).to_string(),
                    line: node.start_position().row + 1,
                    kind: node.kind().replace("_specifier", ""),
                });
            }
        }
    });
    structs
}

fn extract_classes_cpp(root: Node, source: &str) -> Vec<CStruct> {
    let mut classes = Vec::new();
    tsnode::walk_flat(root, &mut |node| {
        if node.kind() == "class_specifier" {
            let mut cursor = node.walk();
            let name_node = node.children(&mut cursor).find(|c| c.kind() == "type_identifier");
            if let Some(n) = name_node {
                classes.push(CStruct { name: text_of(n, source).to_string(), line: node.start_position().row + 1, kind: "class".to_string() });
            }
        }
    });
    classes
}

fn extract_macros(root: Node, source: &str) -> Vec<CMacro> {
    let mut macros = Vec::new();
    tsnode::walk_flat(root, &mut |node| {
        if node.kind() == "preproc_def" {
            let mut cursor = node.walk();
            let name_node = node.children(&mut cursor).find(|c| c.kind() == "identifier");
            if let Some(n) = name_node {
                macros.push(CMacro { name: text_of(n, source).to_string(), line: node.start_position().row + 1 });
            }
        }
    });
    macros
}

fn build_call_graph(functions: &[CFunction]) -> Vec<CallEdge> {
    let pares: Vec<(&str, &[String])> =
        functions.iter().map(|f| (f.name.as_str(), f.calls.as_slice())).collect();
    graph::call_edges(&pares)
}

const WASM_BIGO_HOT: &[&str] = &["O(n²)", "O(n³)", "O(2^n)"];
const WASM_CC_THRESHOLD: u32 = 5;

/// Puerto de `_wasm_hints_c`.
fn wasm_hints_c(functions: &[CFunction]) -> Vec<WasmHint> {
    functions
        .iter()
        .filter(|f| WASM_BIGO_HOT.contains(&f.big_o.as_str()) || f.complexity >= WASM_CC_THRESHOLD)
        .map(|f| WasmHint {
            function: f.name.clone(),
            line: f.line,
            priority: 3,
            reasons: vec![format!("Hot path C — {}, CC={}", f.big_o, f.complexity)],
            recommendation: "Compilar con Emscripten: emcc -O3 -s WASM=1".to_string(),
            estimated_speedup: "2-10x vs JavaScript".to_string(),
        })
        .collect()
}

fn parse_with(source: &str, language: tree_sitter::Language, is_cpp: bool) -> Option<CParseResult> {
    let mut parser = Parser::new();
    parser.set_language(&language).ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();

    let functions = extract_functions(root, source);
    let imports = extract_includes(root, source);
    let classes = if is_cpp { extract_classes_cpp(root, source) } else { extract_structs(root, source) };
    let macros = extract_macros(root, source);
    let call_graph = build_call_graph(&functions);
    let wasm_hints = wasm_hints_c(&functions);
    let memory = crate::hallazgos::memlayout::build(root, source);
    let modernization = crate::hallazgos::modernization::analyze_c_cpp(&memory, root, source, is_cpp);

    let mut structural_smells = structural_smells_c(root, source);
    // Fase 34, Linea A — costo de compilacion de la metaprogramacion. Emite
    // `StructuralSmell`, el shape que ya viaja, asi que no hace falta una
    // clave nueva en el resultado ni un brazo nuevo en `scanner.rs`.
    structural_smells.extend(crate::costo::plantillas::template_smells(root, source, is_cpp));
    // Fase 32 — conversiones implicitas, mezcla de signo y entero<->puntero
    // (`tipos.rs`). Mismo criterio: emite `StructuralSmell`, sin clave nueva.
    structural_smells.extend(crate::hallazgos::tipos::tipos_c(root, source));
    // Fase 32 — false sharing. Mismo canal: dos campos atomicos pegados
    // en un struct comparten linea de cache casi seguro.
    structural_smells.extend(crate::hallazgos::falsesharing::analyze(root, source));
    // Fase 32 — SFINAE contra concepts. Las metricas van adentro de
    // `TemplateComplexity` (misma pregunta, mismo panel) y los hallazgos
    // por el canal de siempre.
    let (constraints, sfinae_smells) = crate::hallazgos::sfinae::analyze(root, source, is_cpp);
    structural_smells.extend(sfinae_smells);
    let mut templates = crate::costo::plantillas::template_complexity(root, source, is_cpp);
    templates.constraints = constraints;
    let security_findings = csecurity::security_findings_c(root, source);
    let locality = locality::locality_findings(root, source);
    Some(CParseResult {
        functions,
        classes,
        imports,
        macros,
        call_graph,
        wasm_hints,
        memory,
        modernization,
        structural_smells,
        security_findings,
        locality,
        templates,
    })
}

pub fn parse_c(source: &str) -> Option<CParseResult> {
    parse_with(source, tree_sitter_c::LANGUAGE.into(), false)
}

pub fn parse_cpp(source: &str) -> Option<CParseResult> {
    parse_with(source, tree_sitter_cpp::LANGUAGE.into(), true)
}

#[cfg(test)]
mod tests {
    use super::*;









    #[test]
    fn detecta_funcion_c_simple() {
        let src = "int add(int a, int b) {\n    return a + b;\n}\n";
        let r = parse_c(src).unwrap();
        assert_eq!(r.functions.len(), 1);
        assert_eq!(r.functions[0].name, "add");
        assert_eq!(r.functions[0].big_o, "O(1)");
        assert!(!r.functions[0].is_tail_recursive);
        assert!(r.functions[0].recursion_note.is_none());
    }

    #[test]
    fn factorial_con_acumulador_es_tail_recursive() {
        let src = "int fact(int n, int acc) {\n    if (n <= 1) return acc;\n    return fact(n - 1, n * acc);\n}\n";
        let r = parse_c(src).unwrap();
        assert!(r.functions[0].is_tail_recursive);
        assert!(r.functions[0].recursion_note.as_ref().unwrap().contains("Tail-call"));
    }

    #[test]
    fn factorial_clasico_no_es_tail_recursive() {
        // `return n * fact(n - 1);` -- la auto-llamada está envuelta en un
        // `binary_expression`, no ES el valor completo del return -- misma
        // distinción que `calls_self` de Python ya hace.
        let src = "int fact(int n) {\n    if (n <= 1) return 1;\n    return n * fact(n - 1);\n}\n";
        let r = parse_c(src).unwrap();
        assert!(!r.functions[0].is_tail_recursive);
    }

    #[test]
    fn tail_call_dentro_de_un_if_tambien_se_detecta() {
        let src = "int f(int n, int acc) {\n    if (n <= 0) {\n        return acc;\n    }\n    if (n % 2 == 0) {\n        return f(n - 1, acc + 1);\n    }\n    return f(n - 1, acc);\n}\n";
        let r = parse_c(src).unwrap();
        assert!(r.functions[0].is_tail_recursive);
    }

    #[test]
    fn tail_recursion_tambien_se_detecta_en_cpp() {
        let src = "int fact(int n, int acc) {\n    if (n <= 1) return acc;\n    return fact(n - 1, n * acc);\n}\n";
        let r = parse_cpp(src).unwrap();
        assert!(r.functions[0].is_tail_recursive);
    }

    #[test]
    fn auto_llamada_dentro_de_una_lambda_local_no_cuenta() {
        // Bug real de auditoría (2026-09-02): `total` nunca se llama a sí
        // misma (su único return es `return 0;`) -- la auto-llamada está
        // DENTRO de una lambda local, su propio scope de nombres. Antes del
        // fix, el walk sin límite de scope de `is_tail_recursive` la
        // encontraba igual y marcaba `total` como tail-recursive por error.
        let src = "int total(int n) {\n    auto helper = []() { return total(5); };\n    return 0;\n}\n";
        let r = parse_cpp(src).unwrap();
        assert!(!r.functions[0].is_tail_recursive);
    }

    #[test]
    fn loops_anidados_dan_on2_c() {
        let src = "void f(int n) {\n    for (int i = 0; i < n; i++) {\n        for (int j = 0; j < n; j++) {\n            do_something(i, j);\n        }\n    }\n}\n";
        let r = parse_c(src).unwrap();
        assert_eq!(r.functions[0].big_o, "O(n²)");
    }

    /// **Bucles SECUENCIALES no son bucles anidados.** `infer_big_o_c`
    /// contaba ocurrencias del texto `" for "` en el cuerpo, así que dos
    /// bucles uno detrás del otro —que recorren n cada uno, O(n) en total—
    /// se reportaban como `O(n²)`. Es el error más común de los tres que
    /// tenía esta función, porque el patrón es cotidiano.
    #[test]
    fn dos_bucles_secuenciales_son_lineales_no_cuadraticos() {
        let src = "void f(int n) {\n    for (int i = 0; i < n; i++) { a(i); }\n    for (int j = 0; j < n; j++) { b(j); }\n}\n";
        let r = parse_c(src).unwrap();
        assert_eq!(r.functions[0].big_o, "O(n)", "dos pasadas seguidas siguen siendo O(n)");
    }

    /// **`has_binary` miraba si el cuerpo CONTENÍA la cadena `"mid"`.**
    /// Cualquier identificador que la tuviera adentro —`midpoint`,
    /// `humidity`, `middle`— convertía un bucle lineal en `O(log n)`.
    #[test]
    fn una_variable_llamada_midpoint_no_convierte_el_bucle_en_logaritmico() {
        let src = "void f(int n) {\n    int midpoint = 0;\n    for (int i = 0; i < n; i++) { midpoint += i; }\n}\n";
        let r = parse_c(src).unwrap();
        assert_eq!(r.functions[0].big_o, "O(n)");
    }

    #[test]
    fn la_palabra_humidity_tampoco() {
        let src = "void f(int n) {\n    for (int i = 0; i < n; i++) { humidity(i); }\n}\n";
        let r = parse_c(src).unwrap();
        assert_eq!(r.functions[0].big_o, "O(n)");
    }

    /// Y la contraparte: una búsqueda binaria DE VERDAD sigue dando
    /// `O(log n)`. Sin este test, "arreglar" el falso positivo de `mid`
    /// podría romper el caso que la heurística existía para reconocer.
    #[test]
    fn una_busqueda_binaria_de_verdad_sigue_siendo_logaritmica() {
        let src = "int f(int* a, int n, int k) {\n    int lo = 0, hi = n;\n    while (lo < hi) {\n        int m = lo + (hi - lo) / 2;\n        if (a[m] < k) lo = m + 1; else hi = m;\n    }\n    return lo;\n}\n";
        let r = parse_c(src).unwrap();
        assert_eq!(r.functions[0].big_o, "O(log n)");
    }

    /// **El costo de la operación de adentro** — el mismo defecto que
    /// `amortizado.rs` arregló del lado Python, medido también acá:
    /// `push_back` es O(1) amortizado y el bucle es O(n)…
    #[test]
    fn push_back_en_un_bucle_sigue_siendo_lineal() {
        let src = "#include <vector>\nvoid f(std::vector<int>& in, std::vector<int>& out) {\n    for (int x : in) { out.push_back(x); }\n}\n";
        let r = parse_cpp(src).unwrap();
        assert_eq!(r.functions[0].big_o, "O(n)");
    }

    /// …pero `vector::insert` corre la cola entera, O(n) por llamada.
    #[test]
    fn vector_insert_en_un_bucle_es_cuadratico() {
        let src = "#include <vector>\nvoid f(std::vector<int>& in, std::vector<int>& out) {\n    for (int x : in) { out.insert(out.begin(), x); }\n}\n";
        let r = parse_cpp(src).unwrap();
        assert_eq!(r.functions[0].big_o, "O(n\u{b2})");
    }

    /// **La distinción que Python no necesitaba y C++ sí.** `x.insert(...)`
    /// no dice nada por sí solo: en un `std::vector` es O(n), en un
    /// `std::map` es O(log n), en un `std::unordered_map` es O(1)
    /// amortizado. Sin saber de qué tipo se declaró `x` no se puede afirmar
    /// nada — por eso hace falta rastrear declaraciones, y por eso un
    /// receptor de tipo desconocido no dispara nada.
    #[test]
    fn map_insert_es_logaritmico_no_lineal() {
        let src = "#include <map>\n#include <vector>\nvoid f(std::vector<int>& in) {\n    std::map<int,int> m;\n    for (int x : in) { m.insert({x, x}); }\n}\n";
        let r = parse_cpp(src).unwrap();
        assert_eq!(r.functions[0].big_o, "O(n log n)");
    }

    #[test]
    fn un_receptor_de_tipo_desconocido_no_afirma_nada() {
        // `cosa` no se declara en ningún lado visible: sin tipo, no hay
        // costo que reclamar, así que el Big-O queda en el del bucle.
        let src = "void f(int n) {\n    for (int i = 0; i < n; i++) { cosa.insert(i); }\n}\n";
        let r = parse_cpp(src).unwrap();
        assert_eq!(r.functions[0].big_o, "O(n)");
    }

    /// **Contar texto contaba las keywords que aparecían en COMENTARIOS.**
    /// Leyendo el árbol el problema desaparece sin que haya que excluirlos:
    /// un `comment` es un nodo de otra clase y nunca se pregunta.
    #[test]
    fn un_comentario_que_nombra_keywords_no_suma_complejidad() {
        let src = "void f() {\n    // if we need to loop for each item while the queue drains\n    hacer();\n}\n";
        let r = parse_c(src).unwrap();
        assert_eq!(r.functions[0].complexity, 1, "un comentario no es una decisión");
    }

    #[test]
    fn una_cadena_que_nombra_keywords_tampoco() {
        let src = "void f() {\n    printf(\"if for while case\");\n}\n";
        let r = parse_c(src).unwrap();
        assert_eq!(r.functions[0].complexity, 1);
    }

    /// Y el error en el otro sentido, que es peor porque es SILENCIOSO:
    /// `" if "` exige espacios alrededor, así que `if(x)` no se contaba.
    #[test]
    fn un_if_sin_espacio_igual_cuenta() {
        let src = "void f(int x) {\n    if(x) { a(); }\n}\n";
        let r = parse_c(src).unwrap();
        assert_eq!(r.functions[0].complexity, 2);
    }

    #[test]
    fn un_and_sin_espacios_igual_cuenta() {
        let src = "int f(int a, int b) {\n    return a&&b;\n}\n";
        let r = parse_c(src).unwrap();
        assert_eq!(r.functions[0].complexity, 2);
    }

    /// El ternario es un punto de decisión y no se contaba de ninguna forma
    /// — `complexity.rs` sí cuenta su equivalente Python (`IfExp`), así que
    /// las dos mitades del motor medían cosas distintas con el mismo nombre.
    #[test]
    fn el_ternario_es_un_punto_de_decision() {
        let src = "int f(int x) {\n    return x > 0 ? 1 : -1;\n}\n";
        let r = parse_c(src).unwrap();
        assert_eq!(r.functions[0].complexity, 2);
    }

    #[test]
    fn un_catch_de_cpp_es_un_punto_de_decision() {
        let src = "void f() {\n    try { a(); } catch (...) { b(); }\n}\n";
        let r = parse_cpp(src).unwrap();
        assert_eq!(r.functions[0].complexity, 2);
    }

    /// `default:` es un `case_statement` en la gramática, pero NO es una
    /// decisión: es la rama que queda cuando ninguna se tomó. Se distingue
    /// porque no tiene campo `value`. 3 `case` + 1 `default` = CC 4, no 5.
    #[test]
    fn el_default_de_un_switch_no_cuenta_como_case() {
        let src = "int f(int x) {\n    switch (x) {\n        case 1: return 1;\n        case 2: return 2;\n        case 3: return 3;\n        default: return 0;\n    }\n}\n";
        let r = parse_c(src).unwrap();
        assert_eq!(r.functions[0].complexity, 4);
    }

    #[test]
    fn else_if_no_se_cuenta_dos_veces_c() {
        // Regresión portada desde Python (test_static_analysis.py) — " if "
        // sola cubre tanto `if` como `else if` (que la contiene como
        // substring); contarla aparte duplicaría el CC. 1 if + 2 else-if
        // debe dar CC=4, no CC=6 — ya documentado en el comentario de
        // `extract_functions` de arriba, ahora con un test que lo prueba.
        let src = "int classify(int x) {\n    if (x > 10) { return 1; }\n    else if (x > 5) { return 2; }\n    else if (x > 0) { return 3; }\n    return 0;\n}\n";
        let r = parse_c(src).unwrap();
        assert_eq!(r.functions[0].complexity, 4);
    }

    #[test]
    fn detecta_include() {
        let src = "#include <stdio.h>\nint main() { return 0; }\n";
        let r = parse_c(src).unwrap();
        assert!(r.imports.iter().any(|i| i.module == "stdio.h"));
    }

    #[test]
    fn detecta_struct() {
        let src = "struct Point {\n    int x;\n    int y;\n};\n";
        let r = parse_c(src).unwrap();
        assert!(r.classes.iter().any(|c| c.name == "Point" && c.kind == "struct"));
    }

    #[test]
    fn detecta_macro() {
        let src = "#define MAX_SIZE 100\n";
        let r = parse_c(src).unwrap();
        assert!(r.macros.iter().any(|m| m.name == "MAX_SIZE"));
    }

    #[test]
    fn detecta_clase_cpp() {
        let src = "class Animal {\npublic:\n    void speak();\n};\n";
        let r = parse_cpp(src).unwrap();
        assert!(r.classes.iter().any(|c| c.name == "Animal" && c.kind == "class"));
    }

    #[test]
    fn call_graph_c_detecta_llamada_entre_funciones_conocidas() {
        let src = "int b() {\n    return 1;\n}\nint a() {\n    return b();\n}\n";
        let r = parse_c(src).unwrap();
        assert!(r.call_graph.iter().any(|e| e.from == "a" && e.to == "b"));
    }

// ── Smells estructurales de C/C++ (Fase 22) ──

    #[test]
    fn una_funcion_larga_de_c_se_marca() {
        let cuerpo = "    int x = 0;\n".repeat(60);
        let src = format!("void larga() {{\n{cuerpo}}}\n");
        let r = parse_c(&src).unwrap();
        assert!(r.structural_smells.iter().any(|s| s.kind == "long_function" && s.name == "larga"));
    }

    #[test]
    fn exceso_de_parametros_se_marca() {
        let src = "int f(int a, int b, int c, int d, int e, int g) {\n    return a;\n}\n";
        let r = parse_c(src).unwrap();
        assert!(r.structural_smells.iter().any(|s| s.kind == "excessive_parameters"));
    }

    #[test]
    fn void_como_unico_parametro_cuenta_como_cero() {
        // `void f(void)` es la forma de C de decir "sin parametros". Contarla
        // como 1 correria el umbral un parametro antes de tiempo en todo
        // archivo C viejo.
        let src = "int f(void) {\n    return 1;\n}\n";
        let r = parse_c(src).unwrap();
        assert!(r.structural_smells.is_empty(), "{:?}", r.structural_smells.len());
        let src6 = "int g(int a, int b, int c, int d, int e) {\n    return a;\n}\n";
        assert!(parse_c(src6).unwrap().structural_smells.is_empty(), "5 parametros no supera el umbral");
    }

    #[test]
    fn el_declarador_envuelto_en_puntero_igual_se_encuentra() {
        // `int *f(...)` envuelve el `function_declarator` en un
        // `pointer_declarator`: buscarlo solo entre los hijos directos lo
        // dejaria pasar y ninguna funcion que devuelve puntero se chequearia.
        let src = "int *f(int a, int b, int c, int d, int e, int g) {\n    return 0;\n}\n";
        let r = parse_c(src).unwrap();
        assert!(r.structural_smells.iter().any(|s| s.kind == "excessive_parameters"), "{:?}", r.structural_smells);
    }

    #[test]
    fn anidamiento_profundo_se_marca() {
        let src = "void f(int n) {\n  if (n) {\n    for (int i = 0; i < n; i++) {\n      while (n) {\n        switch (n) {\n          case 1: {\n            if (n) { return; }\n          }\n        }\n      }\n    }\n  }\n}\n";
        let r = parse_c(src).unwrap();
        assert!(r.structural_smells.iter().any(|s| s.kind == "deep_nesting"), "{:?}", r.structural_smells);
    }

    #[test]
    fn una_cadena_else_if_no_es_anidamiento() {
        // Un despacho de 6 ramas escrito con `else if` — completamente normal
        // en C. El arbol real lo anida (`else_clause > if_statement`), asi que
        // contar la profundidad sintactica reportaria "6 niveles de bloques
        // anidados", que es una afirmacion FALSA, no una opinion de estilo.
        let src = "int f(int a) {\n  if (a == 1) { return 1; }\n  else if (a == 2) { return 2; }\n  else if (a == 3) { return 3; }\n  else if (a == 4) { return 4; }\n  else if (a == 5) { return 5; }\n  else { return 0; }\n}\n";
        let r = parse_c(src).unwrap();
        assert!(
            !r.structural_smells.iter().any(|s| s.kind == "deep_nesting"),
            "cadena else-if reportada como anidamiento: {:?}",
            r.structural_smells
        );
    }

    #[test]
    fn una_funcion_c_normal_no_produce_ningun_smell() {
        let src = "int suma(int a, int b) {\n    return a + b;\n}\n";
        assert!(parse_c(src).unwrap().structural_smells.is_empty());
    }

    #[test]
    fn el_anidamiento_de_una_lambda_no_cuenta_como_de_la_funcion_que_la_contiene() {
        // Mismo criterio de scope que `is_tail_recursive` ya aplica.
        let src = "void f() {\n  auto g = []() {\n    if (1) { for (;;) { while (1) { if (1) { return; } } } }\n  };\n}\n";
        let r = parse_cpp(src).unwrap();
        assert!(
            !r.structural_smells.iter().any(|s| s.kind == "deep_nesting" && s.name == "f"),
            "{:?}",
            r.structural_smells
        );
    }

    #[test]
    fn wasm_hint_dispara_en_hot_path() {
        let src = "void f(int n) {\n    for (int i = 0; i < n; i++) {\n        for (int j = 0; j < n; j++) {\n            do_something(i, j);\n        }\n    }\n}\n";
        let r = parse_c(src).unwrap();
        assert!(!r.wasm_hints.is_empty());
    }

    /// **El defecto que 1323 tests no veían**: en `std::string f(int)` el
    /// PRIMER hijo de `function_definition` es el tipo de retorno, y su kind
    /// es `qualified_identifier` — el mismo que el de un nombre calificado.
    /// Recorriendo hijos en orden, la función se llamaba `std::string`.
    #[test]
    fn el_tipo_de_retorno_calificado_no_es_el_nombre_de_la_funcion() {
        let r = parse_cpp("#include <string>\nstd::string armar(int n) { return \"\"; }\nstd::vector<int> listar(int n) { return {}; }\n").unwrap();
        let nombres: Vec<&str> = r.functions.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(nombres, vec!["armar", "listar"], "{nombres:?}");
    }

    /// Y un nombre calificado DE VERDAD —un método definido fuera de su
    /// clase— sí tiene que conservarse entero.
    #[test]
    fn un_metodo_definido_fuera_de_su_clase_conserva_el_nombre_calificado() {
        let r = parse_cpp("struct S { void metodo(); };\nvoid S::metodo() { }\n").unwrap();
        assert!(r.functions.iter().any(|f| f.name == "S::metodo"), "{:?}", r.functions.iter().map(|f| &f.name).collect::<Vec<_>>());
    }

    /// Punteros y referencias en el retorno envuelven al declarador; el
    /// nombre sigue estando adentro.
    #[test]
    fn un_retorno_por_puntero_o_referencia_no_pierde_el_nombre() {
        let r = parse_cpp("int *dame(int n) { return nullptr; }\nint &ref(int n) { static int x; return x; }\n").unwrap();
        let nombres: Vec<&str> = r.functions.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(nombres, vec!["dame", "ref"], "{nombres:?}");
    }
}
