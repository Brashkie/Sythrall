//! **Los 81 módulos del motor, agrupados por lo que hacen.**
//!
//! Estuvieron planos hasta el 2026-10-16: 83 archivos en un solo directorio,
//! 48.694 líneas, sin ninguna señal de que `ccosto.rs` y `jscosto.rs` son el
//! mismo trabajo para dos lenguajes, ni de que `walk.rs` es infraestructura
//! compartida y `naming.rs` es una regla que afirma algo sobre el código del
//! usuario. El agrupamiento sale de la descripción que cada módulo ya tenía
//! escrita arriba de todo, no de su nombre.
//!
//! **El orden de abajo es el del pipeline**: el texto entra por `lenguajes`,
//! se vuelve árbol en `ast`, se mide en `costo` y `flujo`, produce findings
//! en `hallazgos` con el respaldo formal de `teoria`, y sale por `servicio`.
//! `bench` es el único que no analiza código del usuario: valida al motor
//! mismo contra la realidad, compilando y corriendo kernels de verdad.

/// El front-end: un módulo por lenguaje.
pub mod lenguajes;

/// Infraestructura compartida de árbol, sin saber de ningún lenguaje.
pub mod ast;

/// Cuánto cuesta: Big-O, ciclomática, espacio, costo de compilar.
pub mod costo;
/// Flujo de control y de datos: CFG, SSA, grafo de llamadas.
pub mod flujo;

/// Las reglas que afirman algo sobre el código del usuario.
pub mod hallazgos;
/// Ciencias de la computación formal aplicada al código real.
pub mod teoria;

/// La capa de servicio del sidecar.
pub mod servicio;

/// Validación empírica del motor contra la realidad.
pub mod bench;

pub use costo::maintainability::HalsteadMetrics;
pub use flujo::graph::{
    build_architecture_smells, build_call_graph, build_centrality_graph, build_circular_graph, build_heatmap, build_import_graph,
};
pub use lenguajes::notebook::analyze_notebook;
pub use servicio::rich::analyze_rich;

use serde::Serialize;

#[derive(Serialize)]
pub struct AnalysisResult {
    pub functions: Vec<costo::complexity::FunctionComplexity>,
    pub mi: Option<f64>,
    pub halstead: Option<HalsteadMetrics>,
    pub raw: ast::raw::RawStats,
    pub error: Option<String>,
}

/// Punto de entrada único — lo usan tanto el server HTTP (`main.rs`) como los
/// benchmarks de Criterion, para no duplicar el orden parse → complexity →
/// raw → mi en dos lugares.
pub fn analyze(content: &str) -> AnalysisResult {
    let suite = match ast::parser::parse_module(content) {
        Ok(s) => s,
        Err(e) => {
            return AnalysisResult {
                functions: Vec::new(),
                mi: None,
                halstead: None,
                raw: ast::raw::RawStats::default(),
                error: Some(e),
            };
        }
    };

    let functions = costo::complexity::analyze(content, &suite);
    let raw_stats = ast::raw::analyze(content, &suite);

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
    let halstead = costo::maintainability::halstead_metrics(&suite);
    let mi = costo::maintainability::compute(halstead.as_ref(), avg_complexity, raw_stats.sloc, comment_ratio, true);

    AnalysisResult {
        functions,
        mi,
        halstead,
        raw: raw_stats,
        error: None,
    }
}
