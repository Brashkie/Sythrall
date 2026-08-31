//! Parser JS/TS — puerto de `static_parser.py::_parse_js_ts` y sus
//! helpers. Mismo enfoque que el Python que reemplaza: regex + heurística
//! "AST-like", no un parser semántico real (JS/TS no tiene un equivalente
//! Rust-nativo tan directo como `rustpython_parser` para Python) — la
//! fidelidad del puerto es paridad de comportamiento con el heurístico
//! existente, no una mejora de precisión.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::wasm::WasmHint;

#[derive(Serialize, Clone)]
pub struct JsFunction {
    pub name: String,
    pub line: usize,
    pub end_line: usize,
    pub loc: usize,
    pub complexity: u32,
    pub big_o: String,
    pub big_o_reason: String,
    pub calls: Vec<String>,
    pub is_async: bool,
}

#[derive(Serialize, Clone)]
pub struct JsClass {
    pub name: String,
    pub extends: Option<String>,
    pub line: usize,
}

#[derive(Serialize, Clone)]
pub struct JsImport {
    pub module: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub line: usize,
}

#[derive(Serialize, Clone)]
pub struct JsExport {
    pub name: String,
    pub line: usize,
}

#[derive(Serialize, Clone)]
pub struct TsNamed {
    pub name: String,
    pub line: usize,
}

#[derive(Serialize, Clone)]
pub struct DeadImport {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub module: String,
    pub line: usize,
}

#[derive(Serialize)]
pub struct CallEdge {
    pub from: String,
    pub to: String,
}

#[derive(Serialize)]
pub struct JsTsResult {
    pub functions: Vec<JsFunction>,
    pub classes: Vec<JsClass>,
    pub imports: Vec<JsImport>,
    pub exports: Vec<JsExport>,
    pub interfaces: Vec<TsNamed>,
    pub types: Vec<TsNamed>,
    pub dead_code: Vec<DeadImport>,
    pub call_graph: Vec<CallEdge>,
    pub wasm_hints: Vec<WasmHint>,
    pub avg_complexity: f64,
}

const WASM_BIGO_HOT: &[&str] = &["O(n²)", "O(n³)", "O(2^n)"];

/// Puerto de `_wasm_hints_js` — más simple que el heurístico Python-only de
/// `wasm.rs::wasm_hints` (solo mira Big-O caliente, sin CC/LOC/nombre), a
/// propósito: es el mismo criterio más liviano que ya usaba el Python que
/// reemplaza para JS/TS.
fn wasm_hints_js(functions: &[JsFunction]) -> Vec<WasmHint> {
    functions
        .iter()
        .filter(|f| WASM_BIGO_HOT.contains(&f.big_o.as_str()))
        .map(|f| WasmHint {
            function: f.name.clone(),
            line: f.line,
            priority: 3,
            reasons: vec![format!("Hot path JS — {}", f.big_o)],
            recommendation: "Considera mover esta función a un módulo Rust/C++ compilado a WASM".to_string(),
            estimated_speedup: "3-20x para operaciones numéricas".to_string(),
        })
        .collect()
}

static JS_FUNC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:export\s+)?(?:async\s+)?function\s+(\w+)\s*\(([^)]*)\)|const\s+(\w+)\s*=\s*(?:async\s*)?\(([^)]*)\)\s*=>|(?:export\s+)?(?:async\s+)?function\s*\(([^)]*)\)",
    )
    .unwrap()
});
static JS_CLASS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:export\s+)?class\s+(\w+)(?:\s+extends\s+(\w+))?").unwrap());
static JS_IMPORT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"import\s+(?:\{[^}]+\}|[\w*]+)?\s*(?:,\s*\{[^}]+\})?\s*from\s+['"]([^'"]+)['"]"#).unwrap());
static JS_EXPORT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"export\s+(?:default\s+)?(?:function|class|const|let|var)\s+(\w+)").unwrap());
static TS_IFACE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:export\s+)?interface\s+(\w+)").unwrap());
static TS_TYPE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:export\s+)?type\s+(\w+)\s*=").unwrap());
static CALL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\w+)\s*\(").unwrap());
static LOOP_KW_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(for|while|forEach|map|filter|reduce)\b").unwrap());
static NESTED_LOOP_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)(for|while|forEach|map)[^{]*\{[^}]*(for|while|forEach|map)").unwrap());

/// Índice de saltos de línea, construido una sola vez por archivo — resuelve
/// byte-offset → número de línea en O(log n) (búsqueda binaria) en vez de
/// re-escanear desde el principio del archivo en cada llamada. Con N
/// funciones/clases/imports/exports en un archivo de tamaño proporcional a
/// N, escanear desde cero en cada una de las ~N llamadas a `line_of` es
/// O(n²) total — el mismo tipo de bug que un benchmark de Criterion con
/// tamaños crecientes (10/100/1000) deja en evidencia enseguida (331ms en
/// 1000 funciones antes de este fix, con un salto de 10x en n dando ~80x en
/// tiempo — muy lejos de lineal).
struct LineIndex {
    newline_offsets: Vec<usize>,
}

impl LineIndex {
    fn build(content: &str) -> Self {
        let newline_offsets = content.bytes().enumerate().filter(|&(_, b)| b == b'\n').map(|(i, _)| i).collect();
        LineIndex { newline_offsets }
    }

    fn line_of(&self, byte_offset: usize) -> usize {
        self.newline_offsets.partition_point(|&nl| nl < byte_offset) + 1
    }
}

/// Puerto de `_estimate_js_func_loc` — ver su docstring en Python para el
/// razonamiento del corte en `;`/llave balanceada.
fn estimate_func_loc(content: &str, start: usize) -> usize {
    let bytes = content.as_bytes();
    let (mut depth, mut paren_depth, mut max_loc) = (0i32, 0i32, 0usize);
    let mut in_func = false;
    let mut i = start;
    while i < bytes.len() && max_loc < 500 {
        match bytes[i] {
            b'(' => paren_depth += 1,
            b')' => paren_depth -= 1,
            b'{' => {
                depth += 1;
                in_func = true;
            }
            b'}' if in_func => {
                depth -= 1;
                if depth == 0 {
                    return content[start..=i].matches('\n').count() + 1;
                }
            }
            b';' if !in_func && paren_depth <= 0 => {
                return content[start..i].matches('\n').count() + 1;
            }
            b'\n' => max_loc += 1,
            _ => {}
        }
        i += 1;
    }
    max_loc.max(1)
}

fn cyclomatic_js(body: &str) -> u32 {
    let keywords = ["if ", "for ", "while ", "case ", "catch ", "&&", "||", "? "];
    let mut cc: u32 = 1;
    for kw in keywords {
        cc += body.matches(kw).count() as u32;
    }
    cc
}

fn infer_big_o_js(body: &str) -> (String, String) {
    let loops = LOOP_KW_RE.find_iter(body).count();
    let has_binary = body.contains("/ 2") || body.contains(">> 1") || body.contains("Math.floor");
    let nested = NESTED_LOOP_RE.is_match(body);

    if loops == 0 {
        ("O(1)".to_string(), "sin loops".to_string())
    } else if loops == 1 && has_binary {
        ("O(log n)".to_string(), "loop con división binaria".to_string())
    } else if loops == 1 {
        ("O(n)".to_string(), "un loop".to_string())
    } else if nested || loops >= 2 {
        ("O(n²)".to_string(), "loops anidados".to_string())
    } else {
        ("O(n)".to_string(), "caso base".to_string())
    }
}

fn extract_calls_js(body: &str) -> Vec<String> {
    let mut set = std::collections::HashSet::new();
    for cap in CALL_RE.captures_iter(body) {
        set.insert(cap[1].to_string());
    }
    set.into_iter().collect()
}

fn dead_js_imports(imports: &[JsImport], content: &str) -> Vec<DeadImport> {
    let mut dead = Vec::new();
    for imp in imports {
        let module_last = imp.module.rsplit('/').next().unwrap_or(&imp.module);
        let mod_normalized = module_last.replace(['-', '.'], "_");
        let base: String = mod_normalized.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if base.len() > 1 && content.matches(&base).count() <= 1 {
            dead.push(DeadImport { kind: "possibly_unused_import", module: imp.module.clone(), line: imp.line });
        }
    }
    dead
}

fn build_call_graph(functions: &[JsFunction]) -> Vec<CallEdge> {
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

pub fn parse_js_ts(content: &str, is_typescript: bool) -> JsTsResult {
    let mut functions = Vec::new();
    let mut seen_funcs = std::collections::HashSet::new();
    // Calculados una sola vez por archivo, no por match — con N funciones en
    // un archivo de tamaño proporcional a N, recomputar cualquiera de los
    // dos adentro del loop de abajo es O(n²) total, no O(n).
    let idx = LineIndex::build(content);
    let lines: Vec<&str> = content.lines().collect();
    let n_lines = lines.len();

    for m in JS_FUNC_RE.captures_iter(content) {
        let full = m.get(0).unwrap();
        let name = m
            .get(1)
            .or_else(|| m.get(3))
            .map(|g| g.as_str().to_string())
            .unwrap_or_else(|| "<anonymous>".to_string());
        if !seen_funcs.insert(name.clone()) {
            continue;
        }
        let line_no = idx.line_of(full.start());
        let loc = estimate_func_loc(content, full.start());
        let end_idx = (line_no - 1 + loc).min(lines.len());
        let body = lines[(line_no - 1).min(lines.len())..end_idx].join("\n");
        let cc = cyclomatic_js(&body);
        let (big_o, big_o_reason) = infer_big_o_js(&body);
        let window_start = full.start().saturating_sub(10);
        let is_async = content.get(window_start..(full.start() + 5).min(content.len())).unwrap_or("").contains("async");

        functions.push(JsFunction {
            name,
            line: line_no,
            end_line: (line_no + loc).min(n_lines),
            loc,
            complexity: cc,
            big_o,
            big_o_reason,
            calls: extract_calls_js(&body),
            is_async,
        });
    }

    let mut classes = Vec::new();
    for m in JS_CLASS_RE.captures_iter(content) {
        let full = m.get(0).unwrap();
        classes.push(JsClass {
            name: m[1].to_string(),
            extends: m.get(2).map(|g| g.as_str().to_string()),
            line: idx.line_of(full.start()),
        });
    }

    let mut imports = Vec::new();
    for m in JS_IMPORT_RE.captures_iter(content) {
        let full = m.get(0).unwrap();
        imports.push(JsImport { module: m[1].to_string(), kind: "esm_import".to_string(), line: idx.line_of(full.start()) });
    }

    let mut exports = Vec::new();
    for m in JS_EXPORT_RE.captures_iter(content) {
        let full = m.get(0).unwrap();
        exports.push(JsExport { name: m[1].to_string(), line: idx.line_of(full.start()) });
    }

    let mut interfaces = Vec::new();
    let mut types_list = Vec::new();
    if is_typescript {
        for m in TS_IFACE_RE.captures_iter(content) {
            let full = m.get(0).unwrap();
            interfaces.push(TsNamed { name: m[1].to_string(), line: idx.line_of(full.start()) });
        }
        for m in TS_TYPE_RE.captures_iter(content) {
            let full = m.get(0).unwrap();
            types_list.push(TsNamed { name: m[1].to_string(), line: idx.line_of(full.start()) });
        }
    }

    let dead_code = dead_js_imports(&imports, content);
    let call_graph = build_call_graph(&functions);
    let wasm_hints = wasm_hints_js(&functions);
    let avg_complexity = if functions.is_empty() {
        0.0
    } else {
        let sum: u32 = functions.iter().map(|f| f.complexity).sum();
        (sum as f64 / functions.len() as f64 * 100.0).round() / 100.0
    };

    JsTsResult { functions, classes, imports, exports, interfaces, types: types_list, dead_code, call_graph, wasm_hints, avg_complexity }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_index_coincide_con_conteo_manual_de_saltos_de_linea() {
        let src = "a\nb\nc\nd\n";
        let idx = LineIndex::build(src);
        assert_eq!(idx.line_of(0), 1); // 'a', antes de cualquier '\n'
        assert_eq!(idx.line_of(2), 2); // 'b', después del primer '\n'
        assert_eq!(idx.line_of(4), 3); // 'c'
        assert_eq!(idx.line_of(6), 4); // 'd'
    }

    #[test]
    fn detecta_function_declaration_simple() {
        let src = "function add(a, b) {\n  return a + b;\n}\n";
        let r = parse_js_ts(src, false);
        assert_eq!(r.functions.len(), 1);
        assert_eq!(r.functions[0].name, "add");
        assert_eq!(r.functions[0].big_o, "O(1)");
    }

    #[test]
    fn detecta_arrow_function_const() {
        let src = "const isEven = (n) => n % 2 === 0;\n";
        let r = parse_js_ts(src, false);
        assert_eq!(r.functions.len(), 1);
        assert_eq!(r.functions[0].name, "isEven");
    }

    #[test]
    fn arrow_sin_llaves_no_absorbe_la_siguiente_funcion() {
        let src = "const isEven = (n) => n % 2 === 0;\nfunction other() {\n  return 1;\n}\n";
        let r = parse_js_ts(src, false);
        let is_even = r.functions.iter().find(|f| f.name == "isEven").unwrap();
        assert_eq!(is_even.loc, 1);
    }

    #[test]
    fn firma_multilinea_se_mide_correctamente() {
        // Regresión portada desde Python (test_static_analysis.py) — una
        // firma de función repartida en varias líneas no debe descontarse
        // ni sumarse de más al LOC total.
        let src = "function longSignature(\n  a, b, c\n) {\n  return a + b + c;\n}\n";
        let r = parse_js_ts(src, false);
        let f = r.functions.iter().find(|f| f.name == "longSignature").unwrap();
        assert_eq!(f.loc, 5);
    }

    #[test]
    fn else_if_no_se_cuenta_dos_veces() {
        // Regresión portada desde Python — "else if" contiene " if " como
        // substring; contarlo aparte además de "if " duplicaría el CC. 1
        // if + 2 else-if debe dar CC=4 (1 base + 3 apariciones de "if "),
        // no CC=6.
        let src = "function classify(x) {\n  if (x > 10) { return 1; }\n  else if (x > 5) { return 2; }\n  else if (x > 0) { return 3; }\n  return 0;\n}\n";
        let r = parse_js_ts(src, false);
        let f = r.functions.iter().find(|f| f.name == "classify").unwrap();
        assert_eq!(f.complexity, 4);
    }

    #[test]
    fn loops_anidados_dan_on2() {
        let src = "function f(items) {\n  for (const a of items) {\n    for (const b of items) {\n      console.log(a, b);\n    }\n  }\n}\n";
        let r = parse_js_ts(src, false);
        assert_eq!(r.functions[0].big_o, "O(n²)");
    }

    #[test]
    fn detecta_clase_con_extends() {
        let src = "class Dog extends Animal {}\n";
        let r = parse_js_ts(src, false);
        assert_eq!(r.classes[0].name, "Dog");
        assert_eq!(r.classes[0].extends.as_deref(), Some("Animal"));
    }

    #[test]
    fn detecta_import_esm() {
        let src = "import { useState } from 'react';\n";
        let r = parse_js_ts(src, false);
        assert_eq!(r.imports[0].module, "react");
        assert_eq!(r.imports[0].kind, "esm_import");
    }

    #[test]
    fn interfaces_y_types_solo_en_typescript() {
        let src = "interface Foo {}\ntype Bar = string;\n";
        let js = parse_js_ts(src, false);
        assert!(js.interfaces.is_empty() && js.types.is_empty());
        let ts = parse_js_ts(src, true);
        assert_eq!(ts.interfaces[0].name, "Foo");
        assert_eq!(ts.types[0].name, "Bar");
    }

    #[test]
    fn call_graph_detecta_llamada_entre_funciones_conocidas() {
        let src = "function a() {\n  return b();\n}\nfunction b() {\n  return 1;\n}\n";
        let r = parse_js_ts(src, false);
        assert!(r.call_graph.iter().any(|e| e.from == "a" && e.to == "b"));
    }

    #[test]
    fn is_async_detectado() {
        let src = "async function fetchData() {\n  return 1;\n}\n";
        let r = parse_js_ts(src, false);
        assert!(r.functions[0].is_async);
    }
}
