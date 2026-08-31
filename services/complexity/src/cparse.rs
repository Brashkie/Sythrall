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

use crate::memlayout::MemoryLayoutResult;
use crate::modernization::ModernizationReport;
use crate::wasm::WasmHint;

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
pub struct CallEdge {
    pub from: String,
    pub to: String,
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
}

fn text_of<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

fn walk<'a>(node: Node<'a>, f: &mut impl FnMut(Node<'a>)) {
    f(node);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, f);
    }
}

fn func_name(node: Node, source: &str) -> String {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "function_declarator" {
            let mut sub_cursor = child.walk();
            for sub in child.children(&mut sub_cursor) {
                if sub.kind() == "identifier" {
                    return text_of(sub, source).to_string();
                }
            }
        }
        if child.kind() == "identifier" || child.kind() == "qualified_identifier" {
            return text_of(child, source).to_string();
        }
    }
    "<anonymous>".to_string()
}

/// Puerto de `_infer_big_o_c` — mismo heurístico por keywords.
fn infer_big_o_c(body: &str) -> (String, String) {
    let loops = body.matches(" for ").count() + body.matches("\tfor ").count() + body.matches(" while ").count() + body.matches("\twhile ").count();
    let has_binary = body.contains("/ 2") || body.contains(">> 1") || body.contains("mid");

    if loops == 0 {
        ("O(1)".to_string(), "sin loops".to_string())
    } else if loops == 1 && has_binary {
        ("O(log n)".to_string(), "loop con división binaria".to_string())
    } else if loops == 1 {
        ("O(n)".to_string(), "un loop".to_string())
    } else if loops == 2 {
        ("O(n²)".to_string(), "loops anidados dobles".to_string())
    } else {
        ("O(n³)".to_string(), "loops anidados triples o más".to_string())
    }
}

fn extract_calls(func_node: Node, source: &str) -> Vec<String> {
    let mut calls = std::collections::HashSet::new();
    walk(func_node, &mut |n| {
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
    let lines: Vec<&str> = source.lines().collect();
    let mut functions = Vec::new();
    walk(root, &mut |node| {
        if node.kind() == "function_definition" {
            let name = func_name(node, source);
            let start = node.start_position().row + 1;
            let end = node.end_position().row + 1;
            let loc = end - start + 1;
            let body_src = lines[(start - 1).min(lines.len())..end.min(lines.len())].join("\n");

            // Igual que el Python: " if " sola cubre `if` y `else if` (que
            // contiene " if " como substring), no hay que contarlo aparte.
            let cc = 1
                + body_src.matches(" if ").count() as u32
                + body_src.matches(" for ").count() as u32
                + body_src.matches(" while ").count() as u32
                + body_src.matches(" case ").count() as u32
                + body_src.matches(" && ").count() as u32
                + body_src.matches(" || ").count() as u32;

            let (big_o, big_o_reason) = infer_big_o_c(&body_src);

            functions.push(CFunction {
                name,
                line: start,
                end_line: end,
                loc,
                complexity: cc,
                big_o,
                big_o_reason,
                calls: extract_calls(node, source),
            });
        }
    });
    functions
}

fn extract_includes(root: Node, source: &str) -> Vec<CInclude> {
    let mut includes = Vec::new();
    walk(root, &mut |node| {
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
    walk(root, &mut |node| {
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
    walk(root, &mut |node| {
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
    walk(root, &mut |node| {
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
    let names: std::collections::HashSet<&str> = functions.iter().map(|f| f.name.as_str()).collect();
    let mut edges = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for f in functions {
        for callee in &f.calls {
            if names.contains(callee.as_str()) && callee != &f.name {
                let key = format!("{}\u{2192}{}", f.name, callee);
                if seen.insert(key) {
                    edges.push(CallEdge { from: f.name.clone(), to: callee.clone() });
                }
            }
        }
    }
    edges
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
    let memory = crate::memlayout::build(root, source);
    let modernization = crate::modernization::analyze_c_cpp(&memory, root, source);

    Some(CParseResult { functions, classes, imports, macros, call_graph, wasm_hints, memory, modernization })
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
    }

    #[test]
    fn loops_anidados_dan_on2_c() {
        let src = "void f(int n) {\n    for (int i = 0; i < n; i++) {\n        for (int j = 0; j < n; j++) {\n            do_something(i, j);\n        }\n    }\n}\n";
        let r = parse_c(src).unwrap();
        assert_eq!(r.functions[0].big_o, "O(n²)");
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

    #[test]
    fn wasm_hint_dispara_en_hot_path() {
        let src = "void f(int n) {\n    for (int i = 0; i < n; i++) {\n        for (int j = 0; j < n; j++) {\n            do_something(i, j);\n        }\n    }\n}\n";
        let r = parse_c(src).unwrap();
        assert!(!r.wasm_hints.is_empty());
    }
}
