pub mod asm_bench;
pub mod asmparse;
pub mod bfs_bench;
pub mod bigo;
pub mod callingconv;
pub mod classifiers;
pub mod complexity;
pub mod cparse;
pub mod datastructures;
pub mod fib_bench;
pub mod fortran_bench;
pub mod fparse;
pub mod graph;
pub mod jsts;
pub mod logstore;
pub mod maintainability;
pub mod memlayout;
pub mod mergesort_bench;
pub mod ml;
pub mod modernization;
pub mod naming;
pub mod parser;
pub mod plugin;
pub mod purity;
pub mod raw;
pub mod recursion;
pub mod rich;
pub mod scanner;
pub mod security;
pub mod smells;
pub mod space;
pub mod structure;
pub mod symbols;
pub mod walk;
pub mod wasm;
pub mod zig_bench;

pub use graph::{
    build_architecture_smells, build_call_graph, build_centrality_graph, build_circular_graph, build_heatmap, build_import_graph,
};
pub use maintainability::HalsteadMetrics;
pub use rich::analyze_rich;

use serde::Serialize;

#[derive(Serialize)]
pub struct AnalysisResult {
    pub functions: Vec<complexity::FunctionComplexity>,
    pub mi: Option<f64>,
    pub halstead: Option<HalsteadMetrics>,
    pub raw: raw::RawStats,
    pub error: Option<String>,
}

/// Punto de entrada único — lo usan tanto el server HTTP (`main.rs`) como los
/// benchmarks de Criterion, para no duplicar el orden parse → complexity →
/// raw → mi en dos lugares.
pub fn analyze(content: &str) -> AnalysisResult {
    let suite = match parser::parse_module(content) {
        Ok(s) => s,
        Err(e) => {
            return AnalysisResult {
                functions: Vec::new(),
                mi: None,
                halstead: None,
                raw: raw::RawStats::default(),
                error: Some(e),
            };
        }
    };

    let functions = complexity::analyze(content, &suite);
    let raw_stats = raw::analyze(content, &suite);

    let avg_complexity = if functions.is_empty() {
        1.0
    } else {
        functions.iter().map(|f| f.complexity as f64).sum::<f64>() / functions.len() as f64
    };
    let comment_ratio = if raw_stats.loc == 0 {
        0.0
    } else {
        raw_stats.comments as f64 / raw_stats.loc as f64
    };
    let halstead = maintainability::halstead_metrics(&suite);
    let mi = maintainability::compute(halstead.as_ref(), avg_complexity, raw_stats.sloc, comment_ratio, true);

    AnalysisResult {
        functions,
        mi,
        halstead,
        raw: raw_stats,
        error: None,
    }
}
