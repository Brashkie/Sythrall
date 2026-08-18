pub mod bigo;
pub mod classifiers;
pub mod complexity;
pub mod maintainability;
pub mod parser;
pub mod raw;
pub mod recursion;
pub mod rich;
pub mod structure;
pub mod walk;

pub use rich::analyze_rich;

use serde::Serialize;

#[derive(Serialize)]
pub struct AnalysisResult {
    pub functions: Vec<complexity::FunctionComplexity>,
    pub mi: Option<f64>,
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
    let mi = maintainability::compute(&suite, avg_complexity, raw_stats.sloc, comment_ratio, true);

    AnalysisResult {
        functions,
        mi,
        raw: raw_stats,
        error: None,
    }
}
