//! Parser JS/TS — puerto de `static_parser.py::_parse_js_ts` y sus
//! helpers. Mismo enfoque que el Python que reemplaza: regex + heurística
//! "AST-like", no un parser semántico real (JS/TS no tiene un equivalente
//! Rust-nativo tan directo como `rustpython_parser` para Python) — la
//! fidelidad del puerto es paridad de comportamiento con el heurístico
//! existente, no una mejora de precisión.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::flujo::graph::{self, CallEdge};
use crate::hallazgos::security::SecurityFinding;
use crate::hallazgos::smells::{self, StructuralSmell};
use crate::hallazgos::wasm::WasmHint;

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
    /// Nombres de los parámetros. `JS_FUNC_RE` ya los venía capturando en sus
    /// grupos 2/4/6 desde siempre, pero el resultado se descartaba — se
    /// exponen ahora porque `check_excessive_parameters` (`smells.rs`) los
    /// necesita, y porque son el mismo dato que las funciones de Python ya
    /// publican en su propio `args`.
    pub params: Vec<String>,
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
    /// Fase 21, 2026-09-02 — hasta acá este campo no existía y
    /// `static_parser.py` mandaba `[]` fijo para JS/TS, así que el Project
    /// Health de un proyecto JS/TS daba Security 100/100 por ausencia de
    /// análisis. Ver `jsts_security.rs` para el alcance exacto (presencia de
    /// sink, sin taint tracking — JS/TS no tiene AST real acá).
    pub security_findings: Vec<SecurityFinding>,
    /// Fase 22, 2026-09-02 — la otra mitad del mismo problema: `Quality`
    /// también daba 100/100 para cualquier proyecto JS/TS porque
    /// `structural_smells` era `[]` fijo. Ver `structural_smells_js` abajo.
    pub structural_smells: Vec<StructuralSmell>,
    /// Fase 34, Linea D — solo TypeScript: `as`, `!`, `any`,
    /// `@ts-ignore`. Ver `jscosto::ProofSurfaceTs`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_surface: Option<serde_json::Value>,
    /// Fase 32 — presente SOLO si el archivo es un `grammar.js` de
    /// tree-sitter. **Ausente es distinto de vacío**: un `.js` cualquiera no
    /// trae la clave, en vez de traerla con ceros.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grammar: Option<crate::teoria::gramatica::GrammarInfo>,
    pub avg_complexity: f64,
}

/// Smells estructurales para JS/TS. **No hay lógica de smell nueva acá**: usa
/// las mismas `smells::check_long_function`/`check_excessive_parameters` que
/// Python, porque las dos son funciones puras de valores planos (nombre,
/// línea, LOC / cantidad de parámetros) y nunca dependieron del AST de
/// `rustpython_parser`. Que el umbral sea el mismo importa: "función larga"
/// significa lo mismo en los 2 lenguajes, no dos criterios distintos con el
/// mismo nombre.
///
/// **Los otros 3 checks quedan deliberadamente afuera, y el motivo es el
/// mismo en los 3**: `check_deep_nesting`, `check_quadratic_list_membership`
/// y `check_de_morgan_simplifiable` necesitan un árbol, no texto. En JS ni
/// siquiera se puede contar profundidad de bloques por llaves: `{` es a la
/// vez bloque, literal de objeto y destructuring, así que un contador de
/// llaves mezclaría un `if` anidado 5 veces con un objeto de configuración
/// anidado 5 veces — dos cosas distintas bajo el mismo número. Preferimos no
/// reportar antes que reportar algo que no significa lo que dice.
fn structural_smells_js(functions: &[JsFunction]) -> Vec<StructuralSmell> {
    let mut out = Vec::new();
    for f in functions {
        out.extend(smells::check_long_function(&f.name, f.line, f.loc));
        out.extend(smells::check_excessive_parameters(&f.name, f.line, f.params.len()));
    }
    out.sort_by_key(|s| s.line);
    out
}

/// Separa una lista de parámetros de JS/TS en nombres. Corta por comas de
/// nivel superior: un default con objeto/array (`{a, b} = {}`, `[x, y] = []`)
/// o un tipo genérico de TS (`Map<string, number>`) traen comas ADENTRO que
/// no separan parámetros — partir por `,` a secas contaría de más y
/// dispararía `excessive_parameters` sobre funciones de 2 argumentos.
fn split_params(raw: &str) -> Vec<String> {
    let mut params = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for ch in raw.chars() {
        match ch {
            '(' | '[' | '{' | '<' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' | '>' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth <= 0 => {
                params.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    params.push(current);
    params.into_iter().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
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
    let pares: Vec<(&str, &[String])> =
        functions.iter().map(|f| (f.name.as_str(), f.calls.as_slice())).collect();
    graph::call_edges(&pares)
}


/// **Extractor de reserva, por regex.** Es lo que corria antes de que la
/// gramatica de JS/TS entrara al proyecto, y queda solo para el caso en que
/// tree-sitter no pueda parsear el archivo. Tiene los cinco defectos que el
/// comentario de `parse_js_ts` enumera: se conserva porque devolver algo
/// imperfecto es mejor que devolver nada, no porque sea correcto.
fn extraer_funciones_por_regex(
    content: &str,
    idx: &LineIndex,
    lines: &[&str],
    n_lines: usize,
) -> Vec<JsFunction> {
    let mut functions = Vec::new();
    let mut seen_funcs = std::collections::HashSet::new();
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
        let (big_o, big_o_reason) = infer_big_o_js(&body);
        let window_start = full.start().saturating_sub(10);
        let is_async =
            content.get(window_start..(full.start() + 5).min(content.len())).unwrap_or("").contains("async");

        functions.push(JsFunction {
            name,
            line: line_no,
            end_line: (line_no + loc).min(n_lines),
            loc,
            complexity: cyclomatic_js(&body),
            big_o,
            big_o_reason,
            calls: extract_calls_js(&body),
            is_async,
            params: split_params(m.get(2).or_else(|| m.get(4)).or_else(|| m.get(6)).map_or("", |g| g.as_str())),
        });
    }
    functions
}

pub fn parse_js_ts(content: &str, is_typescript: bool) -> JsTsResult {
    // Calculado una sola vez por archivo, no por match — con N funciones en
    // un archivo de tamano proporcional a N, recomputarlo adentro de un loop
    // seria O(n2) total, no O(n).
    let idx = LineIndex::build(content);
    let lines: Vec<&str> = content.lines().collect();
    let n_lines = lines.len();

    // **Fase 32 — las funciones salen del ARBOL, no de una regex.** Lo que
    // habia (`JS_FUNC_RE` + `estimate_func_loc`) fallaba de cinco formas
    // medidas contra el sidecar vivo: no encontraba METODOS DE CLASE (una
    // `class` entera daba cero funciones), ni metodos de objeto literal;
    // extraia funciones escritas adentro de COMENTARIOS; deduplicaba por
    // NOMBRE, asi que dos `handler` en scopes distintos perdian uno; y
    // `estimate_func_loc` contaba llaves sobre el texto, de modo que una
    // `"}"` adentro de una cadena cortaba la funcion (una de 5 lineas daba
    // `loc` 2). El arbol resuelve los cinco por construccion.
    //
    // Si la gramatica no puede con el archivo se cae al extractor viejo, que
    // es peor pero existe — degradar es mejor que devolver una lista vacia.
    let mapa_del_arbol = crate::costo::jscosto::analizar(content, is_typescript);
    let functions: Vec<JsFunction> = match &mapa_del_arbol {
        Some(mapa) => mapa
            .funciones()
            .map(|m| JsFunction {
                name: m.name.clone(),
                line: m.line,
                end_line: m.end_line,
                loc: m.loc,
                complexity: m.complexity,
                big_o: m.big_o.clone(),
                big_o_reason: m.big_o_reason.clone(),
                calls: m.calls.clone(),
                is_async: m.is_async,
                params: m.params.clone(),
            })
            .collect(),
        None => extraer_funciones_por_regex(content, &idx, &lines, n_lines),
    };

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

    // Después de `functions`, porque atribuye cada hallazgo a la función que
    // lo contiene usando los rangos que ese loop ya calculó.
    let security_findings = crate::hallazgos::jsts_security::js_security_findings(content, &functions);
    let mut structural_smells = structural_smells_js(&functions);
    // Fase 32 — operaciones de costo O(n) por llamada adentro de un bucle.
    // Salen del arbol (hace falta saber que la variable es un arreglo), asi
    // que no pueden calcularse desde `structural_smells_js`, que solo ve la
    // lista de funciones ya armada.
    if let Some(mapa) = &mapa_del_arbol {
        structural_smells.extend(crate::costo::jscosto::linear_op_smells(mapa));
    }
    structural_smells.extend(crate::costo::jscosto::proof_smells(content, is_typescript));
    // Fase 32 — gramáticas atribuidas, primera porción. Un `grammar.js` de
    // tree-sitter es JavaScript, así que no hace falta un parser nuevo: lo
    // que faltaba era leer la estructura que ese JavaScript describe.
    let mut grammar = None;
    if let Some((info, smells)) = crate::teoria::gramatica::analyze_source(content) {
        structural_smells.extend(smells);
        grammar = Some(info);
    }
    let ps = crate::costo::jscosto::proof_surface(content, is_typescript);
    let proof_surface = is_typescript.then(|| {
        serde_json::json!({
            "non_null_assertions": ps.non_null_assertions,
            "type_assertions": ps.type_assertions,
            "double_assertions": ps.double_assertions,
            "any_annotations": ps.any_annotations,
            "ts_ignore": ps.ts_ignore,
            "ts_expect_error": ps.ts_expect_error,
        })
    });

    JsTsResult {
        grammar,
        functions,
        classes,
        imports,
        exports,
        interfaces,
        types: types_list,
        dead_code,
        call_graph,
        wasm_hints,
        security_findings,
        structural_smells,
        proof_surface,
        avg_complexity,
    }
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

    // ── Parámetros + smells estructurales (Fase 22, 2026-09-02) ──────────

    #[test]
    fn parametros_de_funcion_declarada_se_extraen() {
        let r = parse_js_ts("function f(a, b, c) {\n  return a;\n}\n", false);
        assert_eq!(r.functions[0].params, vec!["a", "b", "c"]);
    }

    #[test]
    fn parametros_de_arrow_function_se_extraen() {
        let r = parse_js_ts("const f = (x, y) => x + y\n", false);
        assert_eq!(r.functions[0].params, vec!["x", "y"]);
    }

    #[test]
    fn funcion_sin_parametros_da_lista_vacia() {
        let r = parse_js_ts("function f() {\n  return 1;\n}\n", false);
        assert!(r.functions[0].params.is_empty());
    }

    #[test]
    fn coma_dentro_de_un_default_o_de_un_generico_no_cuenta_como_separador() {
        // 2 parámetros, no 4: las comas de `{a, b}` y de `Map<string, number>`
        // están adentro y no separan argumentos.
        assert_eq!(split_params("{ a, b } = {}, m: Map<string, number>").len(), 2);
        assert_eq!(split_params("[x, y] = [], z").len(), 2);
    }

    #[test]
    fn exceso_de_parametros_dispara_el_mismo_smell_que_python() {
        // 6 > EXCESSIVE_PARAMS (5), el mismo umbral que usan los .py.
        let src = "function f(a, b, c, d, e, g) {\n  return a;\n}\n";
        let r = parse_js_ts(src, false);
        let smells: Vec<_> = r.structural_smells.iter().filter(|s| s.kind == "excessive_parameters").collect();
        assert_eq!(smells.len(), 1);
        assert_eq!(smells[0].name, "f");
    }

    #[test]
    fn cinco_parametros_no_disparan() {
        let r = parse_js_ts("function f(a, b, c, d, e) {\n  return a;\n}\n", false);
        assert!(!r.structural_smells.iter().any(|s| s.kind == "excessive_parameters"));
    }

    #[test]
    fn funcion_larga_dispara_el_smell() {
        // 60 líneas de cuerpo > LONG_FUNCTION_LOC (50).
        let body = "  const x = 1;\n".repeat(60);
        let src = format!("function larga() {{\n{body}}}\n");
        let r = parse_js_ts(&src, false);
        assert!(r.structural_smells.iter().any(|s| s.kind == "long_function" && s.name == "larga"));
    }

    #[test]
    fn funcion_corta_no_dispara_ningun_smell() {
        let r = parse_js_ts("function f(a) {\n  return a;\n}\n", false);
        assert!(r.structural_smells.is_empty());
    }

    #[test]
    fn los_smells_vienen_ordenados_por_linea() {
        let body = "  const x = 1;\n".repeat(60);
        let src = format!("function larga() {{\n{body}}}\nfunction otra(a, b, c, d, e, g) {{\n  return a;\n}}\n");
        let r = parse_js_ts(&src, false);
        let lines: Vec<usize> = r.structural_smells.iter().map(|s| s.line).collect();
        assert!(lines.windows(2).all(|w| w[0] <= w[1]), "sin ordenar: {lines:?}");
    }

    #[test]
    fn typescript_usa_el_mismo_camino_que_javascript() {
        let src = "export function f(a: string, b: number, c: A, d: B, e: C, g: D): void {\n  return;\n}\n";
        let r = parse_js_ts(src, true);
        assert!(r.structural_smells.iter().any(|s| s.kind == "excessive_parameters"));
    }
}
