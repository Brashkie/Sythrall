//! Punto de entrada de la Fase 1 de migración: arma el mismo shape que
//! `_parse_python()` en `static_parser.py` (functions/classes/imports/summary)
//! para un archivo Python, combinando complexity.rs + bigo.rs + recursion.rs +
//! classifiers.rs + structure.rs. Deliberadamente NO incluye call_graph,
//! circular_deps, wasm_hints ni dead_code — esos quedan para una fase
//! siguiente (necesitan razonamiento cross-archivo). `static_parser.py` sigue
//! siendo la fuente de verdad para esas piezas.

use rustpython_parser::ast::Stmt;
use serde::Serialize;

use crate::bigo;
use crate::classifiers;
use crate::complexity::cyclomatic;
use crate::parser::{line_of_offset, parse_module};
use crate::recursion;
use crate::security::{self, SecurityFinding};
use crate::smells::{self, StructuralSmell};
use crate::space;
use crate::structure::{self, RichClass, RichImport};

#[derive(Serialize, Clone)]
pub struct RichFunction {
    pub name: String,
    pub line: usize,
    pub end_line: usize,
    pub loc: usize,
    pub args: Vec<String>,
    pub is_async: bool,
    pub decorators: Vec<String>,
    pub docstring: Option<String>,
    pub complexity: u32,
    pub big_o: String,
    pub big_o_reason: String,
    pub big_o_theta: String,
    pub big_o_omega: String,
    pub space_complexity: String,
    pub space_reason: String,
    pub is_recursive: bool,
    pub is_tail_recursive: bool,
    pub recursion_note: Option<String>,
    pub regex_class: Option<String>,
    pub regex_note: Option<String>,
    pub grammar_class: Option<String>,
    pub grammar_note: Option<String>,
    pub graph_traversal: Option<String>,
    pub graph_traversal_note: Option<String>,
    pub calls: Vec<String>,
    pub returns_annotated: bool,
}

#[derive(Serialize, Clone)]
pub struct RichSummary {
    pub total_functions: usize,
    pub total_classes: usize,
    pub total_imports: usize,
    pub avg_complexity: f64,
    pub max_loc_function: usize,
}

#[derive(Serialize)]
pub struct RichAnalysisResult {
    pub functions: Vec<RichFunction>,
    pub classes: Vec<RichClass>,
    pub imports: Vec<RichImport>,
    pub security_findings: Vec<SecurityFinding>,
    pub structural_smells: Vec<StructuralSmell>,
    pub summary: RichSummary,
    pub error: Option<String>,
}

pub fn analyze_rich(content: &str) -> RichAnalysisResult {
    let suite = match parse_module(content) {
        Ok(s) => s,
        Err(e) => {
            return RichAnalysisResult {
                functions: Vec::new(),
                classes: Vec::new(),
                imports: Vec::new(),
                security_findings: Vec::new(),
                structural_smells: Vec::new(),
                summary: RichSummary {
                    total_functions: 0,
                    total_classes: 0,
                    total_imports: 0,
                    avg_complexity: 0.0,
                    max_loc_function: 0,
                },
                error: Some(e),
            };
        }
    };

    let mut functions = Vec::new();
    let mut security_findings = Vec::new();
    let mut structural_smells = Vec::new();
    collect_functions(content, &suite, &mut functions, &mut security_findings, &mut structural_smells);
    security_findings.extend(security::hardcoded_credentials(content, &suite));
    security_findings.sort_by_key(|f| f.line);

    let classes = structure::extract_classes(content, &suite);
    let imports = structure::extract_imports(content, &suite);

    for c in &classes {
        structural_smells.extend(smells::check_large_class(&c.name, c.line, c.methods.len(), c.loc));
        structural_smells.extend(smells::check_god_object(&c.name, c.line, c.methods.len(), c.attribute_count));
    }
    structural_smells.sort_by_key(|s| s.line);

    let total_functions = functions.len();
    let avg_complexity = if total_functions == 0 {
        0.0
    } else {
        functions.iter().map(|f| f.complexity as f64).sum::<f64>() / total_functions as f64
    };
    let max_loc_function = functions.iter().map(|f| f.loc).max().unwrap_or(0);

    RichAnalysisResult {
        summary: RichSummary {
            total_functions,
            total_classes: classes.len(),
            total_imports: imports.len(),
            avg_complexity: (avg_complexity * 100.0).round() / 100.0,
            max_loc_function,
        },
        functions,
        classes,
        imports,
        security_findings,
        structural_smells,
        error: None,
    }
}

fn collect_functions(
    source: &str,
    body: &[Stmt],
    out: &mut Vec<RichFunction>,
    security_out: &mut Vec<SecurityFinding>,
    smells_out: &mut Vec<StructuralSmell>,
) {
    for stmt in body {
        match stmt {
            Stmt::FunctionDef(f) => {
                security_out.extend(security::security_findings(&f.name, &f.body, source));
                let rf = build_function(source, &f.name, f.range, &f.body, &f.args, &f.decorator_list, f.returns.is_some(), false);
                smells_out.extend(function_smells(&rf, &f.body));
                out.push(rf);
            }
            Stmt::AsyncFunctionDef(f) => {
                security_out.extend(security::security_findings(&f.name, &f.body, source));
                let rf = build_function(source, &f.name, f.range, &f.body, &f.args, &f.decorator_list, f.returns.is_some(), true);
                smells_out.extend(function_smells(&rf, &f.body));
                out.push(rf);
            }
            Stmt::ClassDef(c) => collect_functions(source, &c.body, out, security_out, smells_out),
            _ => {}
        }
        // Funciones anidadas dentro de otros statements (if/for/with/try a
        // nivel de módulo, poco común pero `ast.walk` las encontraría) — se
        // cubre recorriendo también los cuerpos anidados no-función.
        for child in nested(stmt) {
            collect_functions(source, child, out, security_out, smells_out);
        }
    }
}

/// Smells a nivel de función (Fase 22): función larga, exceso de parámetros,
/// anidamiento profundo — los otros dos (large_class/god_object) se calculan
/// aparte, a nivel de clase, en `analyze_rich`.
fn function_smells(rf: &RichFunction, body: &[Stmt]) -> Vec<StructuralSmell> {
    let mut out = Vec::new();
    out.extend(smells::check_long_function(&rf.name, rf.line, rf.loc));
    out.extend(smells::check_excessive_parameters(&rf.name, rf.line, rf.args.len()));
    out.extend(smells::check_deep_nesting(&rf.name, rf.line, body));
    out
}

fn nested(stmt: &Stmt) -> Vec<&[Stmt]> {
    match stmt {
        Stmt::If(s) => vec![&s.body, &s.orelse],
        Stmt::For(s) => vec![&s.body, &s.orelse],
        Stmt::AsyncFor(s) => vec![&s.body, &s.orelse],
        Stmt::While(s) => vec![&s.body, &s.orelse],
        Stmt::With(s) => vec![&s.body],
        Stmt::AsyncWith(s) => vec![&s.body],
        Stmt::Try(s) => vec![&s.body, &s.orelse, &s.finalbody],
        _ => vec![],
    }
}

#[allow(clippy::too_many_arguments)]
fn build_function(
    source: &str,
    name: &str,
    range: rustpython_parser::text_size::TextRange,
    body: &[Stmt],
    args: &rustpython_parser::ast::Arguments,
    decorator_list: &[rustpython_parser::ast::Expr],
    returns_annotated: bool,
    is_async: bool,
) -> RichFunction {
    let line = line_of_offset(source, range.start().into());
    let end_line = line_of_offset(source, range.end().into());
    let loc = end_line.saturating_sub(line) + 1;

    let recursion = recursion::analyze(name, body);
    let bigo = bigo::full(body, recursion.is_recursive);
    let space_info = space::infer(body, recursion.is_recursive);
    let regex = classifiers::regex_info(body);
    let grammar = classifiers::grammar_info(name, body, recursion.is_recursive);
    let graph = classifiers::graph_info(body, recursion.is_recursive);

    RichFunction {
        name: name.to_string(),
        line,
        end_line,
        loc,
        args: structure::arg_names(args),
        is_async,
        decorators: decorator_list.iter().map(structure::decorator_name).collect(),
        docstring: structure::docstring_of(body),
        complexity: cyclomatic(body),
        big_o: bigo.big_o,
        big_o_reason: bigo.reason,
        big_o_theta: bigo.theta,
        big_o_omega: bigo.omega,
        space_complexity: space_info.space,
        space_reason: space_info.reason,
        is_recursive: recursion.is_recursive,
        is_tail_recursive: recursion.is_tail_recursive,
        recursion_note: recursion::note(&recursion),
        regex_class: regex.uses_regex.then(|| "Type-3 (Regular)".to_string()),
        regex_note: classifiers::regex_note(&regex),
        grammar_class: grammar.is_grammar_shaped.then(|| "Type-2 (Context-Free)".to_string()),
        grammar_note: classifiers::grammar_note(&grammar),
        graph_traversal: graph.traversal_kind.clone(),
        graph_traversal_note: classifiers::graph_note(&graph),
        calls: structure::extract_calls(body),
        returns_annotated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn func<'a>(result: &'a RichAnalysisResult, name: &str) -> &'a RichFunction {
        result
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("no se encontró la función {name}"))
    }

    #[test]
    fn bubble_sort_es_on2() {
        let src = "def bubble_sort(arr):\n    n = len(arr)\n    for i in range(n):\n        for j in range(n - i - 1):\n            if arr[j] > arr[j+1]:\n                arr[j], arr[j+1] = arr[j+1], arr[j]\n";
        let r = analyze_rich(src);
        assert_eq!(func(&r, "bubble_sort").big_o, "O(n\u{b2})");
    }

    #[test]
    fn binary_search_es_ologn() {
        let src = "def binary_search(arr, target):\n    lo, hi = 0, len(arr)\n    while lo < hi:\n        mid = (lo + hi) // 2\n        if arr[mid] == target:\n            return mid\n        elif arr[mid] < target:\n            lo = mid + 1\n        else:\n            hi = mid\n    return -1\n";
        let r = analyze_rich(src);
        assert_eq!(func(&r, "binary_search").big_o, "O(log n)");
    }

    #[test]
    fn constant_es_o1() {
        let src = "def constant(x):\n    return x * 2\n";
        let r = analyze_rich(src);
        assert_eq!(func(&r, "constant").big_o, "O(1)");
    }

    #[test]
    fn factorial_es_recursivo_no_tail() {
        let src = "def factorial(n):\n    if n <= 1:\n        return 1\n    return n * factorial(n - 1)\n";
        let r = analyze_rich(src);
        let f = func(&r, "factorial");
        assert!(f.is_recursive);
        assert!(!f.is_tail_recursive);
    }

    #[test]
    fn tail_call_detectado() {
        let src = "def countdown(n):\n    if n <= 0:\n        return 0\n    return countdown(n - 1)\n";
        let r = analyze_rich(src);
        let f = func(&r, "countdown");
        assert!(f.is_recursive);
        assert!(f.is_tail_recursive);
    }

    #[test]
    fn regex_detectado() {
        let src = "import re\ndef has_email(s):\n    return re.search(r\"x\", s) is not None\n";
        let r = analyze_rich(src);
        assert_eq!(func(&r, "has_email").regex_class.as_deref(), Some("Type-3 (Regular)"));
    }

    #[test]
    fn plain_function_sin_clasificadores() {
        let src = "def plain(x):\n    return x + 1\n";
        let r = analyze_rich(src);
        let f = func(&r, "plain");
        assert!(f.regex_class.is_none());
        assert!(f.grammar_class.is_none());
        assert!(f.graph_traversal.is_none());
    }

    #[test]
    fn grammar_shaped_detectado() {
        let src = "def parse_expr(tokens, pos):\n    stack = []\n    stack.append(tokens[pos])\n    if pos < len(tokens):\n        return parse_expr(tokens, pos + 1)\n    return stack.pop()\n";
        let r = analyze_rich(src);
        assert_eq!(func(&r, "parse_expr").grammar_class.as_deref(), Some("Type-2 (Context-Free)"));
    }

    #[test]
    fn grammar_requiere_nombre_no_solo_recursion() {
        let src = "def factorial(n):\n    if n <= 1:\n        return 1\n    return n * factorial(n - 1)\n";
        let r = analyze_rich(src);
        assert!(func(&r, "factorial").grammar_class.is_none());
    }

    #[test]
    fn bfs_detectado() {
        let src = "from collections import deque\ndef bfs(graph, start):\n    visited = set([start])\n    queue = deque([start])\n    while queue:\n        node = queue.popleft()\n        for n in graph[node]:\n            if n not in visited:\n                visited.add(n)\n                queue.append(n)\n    return visited\n";
        let r = analyze_rich(src);
        assert_eq!(func(&r, "bfs").graph_traversal.as_deref(), Some("BFS"));
    }

    #[test]
    fn topological_sort_detectado() {
        let src = "def topo(graph):\n    in_degree = {}\n    for node in graph:\n        in_degree[node] = 0\n    return in_degree\n";
        let r = analyze_rich(src);
        assert_eq!(func(&r, "topo").graph_traversal.as_deref(), Some("Topological Sort (Kahn's algorithm)"));
    }

    #[test]
    fn clase_con_metodo_async() {
        let src = "class Animal:\n    \"\"\"doc\"\"\"\n    def __init__(self, name):\n        self.name = name\n\n    async def speak(self):\n        return self.name\n";
        let r = analyze_rich(src);
        assert_eq!(r.classes.len(), 1);
        let c = &r.classes[0];
        assert_eq!(c.name, "Animal");
        assert_eq!(c.docstring.as_deref(), Some("doc"));
        assert_eq!(c.methods.len(), 2);
        assert!(c.methods.iter().any(|m| m.name == "speak" && m.is_async));
        // Los métodos también aparecen en la lista plana de funciones,
        // igual que `ast.walk` los encontraría en static_parser.py.
        assert!(func(&r, "speak").is_async);
    }

    #[test]
    fn imports_extraidos() {
        let src = "import re\nfrom collections import deque\n";
        let r = analyze_rich(src);
        assert_eq!(r.imports.len(), 2);
        assert!(r.imports.iter().any(|i| i.module == "re" && i.kind == "import"));
        assert!(r.imports.iter().any(|i| i.module == "collections" && i.kind == "from_import"));
    }

    #[test]
    fn summary_cuenta_todo() {
        let src = "def a():\n    pass\ndef b():\n    pass\nclass C:\n    pass\nimport os\n";
        let r = analyze_rich(src);
        assert_eq!(r.summary.total_functions, 2);
        assert_eq!(r.summary.total_classes, 1);
        assert_eq!(r.summary.total_imports, 1);
    }
}
