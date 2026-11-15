//! Fase 27 (AI Intelligence), Jupyter Notebook analysis — el único de los 3
//! bullets de esta fase que es implementable hoy sin necesitar el
//! `AIProvider` ni ninguna infraestructura de modelo (esos 2 siguen
//! deliberadamente sin empezar, ver el ROADMAP): un `.ipynb` es JSON con
//! celdas de código/markdown (formato `nbformat`), y las celdas de código
//! Python ya pueden pasar tal cual por el motor de análisis Python
//! existente (`rich::analyze_rich`) sin ningún cambio — extraer el texto de
//! esas celdas y concatenarlo es toda la novedad de este módulo.
//!
//! **Reproducibilidad (2026-09-02)** — la única señal de este módulo que NO
//! viene de correr el motor de Python sobre el código, sino del formato del
//! notebook en sí: `execution_count` (el `[N]` que Jupyter escribe al
//! ejecutar cada celda) estaba siendo ignorado por completo, y es
//! exactamente el dato que responde la pregunta más común sobre un notebook
//! ajeno: *¿esto se reproduce corriéndolo de arriba hacia abajo?* Ver
//! `reproducibility()` abajo.
//!
//! Deliberadamente NO intentado acá: mapear cada hallazgo de vuelta a su
//! celda de origen dentro de `analyze_rich` mismo (los números de línea que
//! devuelve son relativos al pseudo-archivo CONCATENADO, no a la celda
//! individual) — en cambio, `cell_ranges` viaja aparte en la respuesta, así
//! que quien consuma esto puede hacer ese mapeo (línea → celda) sin que
//! `analyze_rich` tenga que saber nada de notebooks. Celdas markdown se
//! ignoran del todo — no hay nada que analizar ahí.

use serde::{Deserialize, Serialize};

use crate::servicio::rich::{analyze_rich, RichAnalysisResult, RichSummary};

#[derive(Deserialize)]
#[serde(untagged)]
enum CellSource {
    /// La mayoría de los notebooks reales guardan `source` como un array de
    /// líneas (cada una YA incluye su propio `\n` final, salvo la última).
    Lines(Vec<String>),
    /// El formato también permite un único string — algunos exportadores
    /// (y notebooks armados a mano) lo usan así.
    Single(String),
}

impl CellSource {
    fn into_text(self) -> String {
        match self {
            CellSource::Lines(lines) => lines.concat(),
            CellSource::Single(s) => s,
        }
    }
}

#[derive(Deserialize)]
struct NotebookCell {
    cell_type: String,
    #[serde(default = "empty_source")]
    source: CellSource,
    /// El contador que Jupyter escribe al ejecutar una celda (el `[N]` a su
    /// izquierda). `null` en una celda que nunca se ejecutó, o cuya salida
    /// se limpió antes de guardar. Ausente del todo en celdas markdown y en
    /// notebooks armados a mano — de ahí el `Option` con `default`.
    #[serde(default)]
    execution_count: Option<u32>,
}

fn empty_source() -> CellSource {
    CellSource::Single(String::new())
}

#[derive(Deserialize)]
struct NotebookDocument {
    #[serde(default)]
    cells: Vec<NotebookCell>,
}

pub struct NotebookExtraction {
    pub source: String,
    /// `(línea_inicio, línea_fin)`, ambas 1-based e inclusive, una entrada
    /// por celda de CÓDIGO en el orden en que aparecen en el notebook —
    /// las celdas markdown no generan entrada, ya que no aportan texto al
    /// pseudo-archivo concatenado.
    pub cell_ranges: Vec<(usize, usize)>,
    /// `execution_count` de cada celda de código, mismo orden e índice que
    /// `cell_ranges`.
    pub execution_counts: Vec<Option<u32>>,
}

/// Análisis de reproducibilidad — la única señal de este módulo que NO sale
/// de correr el motor de Python sobre el código, sino del propio formato del
/// notebook. Un `.ipynb` guarda en qué ORDEN se ejecutaron sus celdas
/// (`execution_count`), y cuando ese orden no coincide con el orden del
/// archivo, el estado guardado no se puede reproducir con "Restart & Run
/// All": alguna celda leyó una variable que, leyendo el archivo de arriba
/// hacia abajo, todavía no existía.
#[derive(Serialize)]
pub struct NotebookReproducibility {
    pub executed_cells: usize,
    /// Celdas de código con `execution_count: null` — nunca ejecutadas en
    /// esta sesión guardada, o con la salida limpiada antes de guardar.
    pub unexecuted_cells: usize,
    /// Índices (1-based, mismos que `cell_ranges`) de las celdas cuyo
    /// `execution_count` es MENOR que el de una celda anterior del archivo:
    /// se ejecutaron antes que algo que está más arriba.
    pub out_of_order_cells: Vec<usize>,
    pub note: String,
}

/// Nota honesta sobre qué prueba y qué no prueba esta señal — misma
/// disciplina que `numerical_algorithm_note`/Type-1 ya establecieron: nombra
/// el patrón, no emite un veredicto sobre la corrección de los resultados.
fn reproducibility_note(out_of_order: usize, unexecuted: usize) -> String {
    if out_of_order == 0 && unexecuted == 0 {
        return "Los contadores de ejecución van en orden creciente y no hay celdas sin ejecutar: el estado guardado es consistente con correr el notebook de arriba hacia abajo. No prueba que los resultados sean correctos, solo que el orden guardado coincide con el orden del archivo.".to_string();
    }
    let mut parts = Vec::new();
    if out_of_order > 0 {
        parts.push(format!(
            "{out_of_order} celda(s) se ejecutaron ANTES que alguna celda que está más arriba en el archivo — 'Restart & Run All' puede no reproducir el estado guardado"
        ));
    }
    if unexecuted > 0 {
        parts.push(format!("{unexecuted} celda(s) de código sin ejecutar (`execution_count: null`)"));
    }
    format!(
        "{}. Esto describe el estado GUARDADO, no el código: un notebook desarrollado desordenado pero re-ejecutado entero antes de guardar no aparece acá, y al revés, esto no prueba que ningún resultado sea incorrecto.",
        parts.join("; ")
    )
}

/// Recorre las celdas de código en orden de ARCHIVO y marca las que rompen
/// el orden de ejecución. Las celdas sin ejecutar no se comparan (no tienen
/// contador que ordenar) pero sí se cuentan aparte — son un problema
/// distinto, no el mismo.
pub fn reproducibility(execution_counts: &[Option<u32>]) -> NotebookReproducibility {
    let mut out_of_order_cells = Vec::new();
    let mut unexecuted_cells = 0;
    let mut previous: Option<u32> = None;
    for (i, count) in execution_counts.iter().enumerate() {
        match count {
            None => unexecuted_cells += 1,
            Some(c) => {
                if previous.is_some_and(|p| *c < p) {
                    out_of_order_cells.push(i + 1);
                }
                // El máximo, no el último: después de una celda desordenada,
                // seguir comparando contra ELLA marcaría como desordenadas a
                // las que siguen aunque estén bien entre sí.
                previous = Some(previous.map_or(*c, |p| p.max(*c)));
            }
        }
    }
    let executed_cells = execution_counts.len() - unexecuted_cells;
    let note = reproducibility_note(out_of_order_cells.len(), unexecuted_cells);
    NotebookReproducibility { executed_cells, unexecuted_cells, out_of_order_cells, note }
}

/// Parsea el JSON de un `.ipynb` y concatena el texto de sus celdas `code`,
/// separadas por una línea en blanco (así un error de sintaxis cerca del
/// final de una celda no se confunde con el comienzo de la siguiente).
/// `Err` cuando el contenido no es JSON válido o no tiene la forma mínima de
/// un notebook (`cells`) — nunca panickea con un archivo random.
pub fn extract_python_source(content: &str) -> Result<NotebookExtraction, String> {
    let doc: NotebookDocument =
        serde_json::from_str(content).map_err(|e| format!("No es un .ipynb válido (JSON malformado o no tiene forma de notebook): {e}"))?;

    let mut source = String::new();
    let mut cell_ranges = Vec::new();
    let mut execution_counts = Vec::new();
    for cell in doc.cells {
        if cell.cell_type != "code" {
            continue;
        }
        execution_counts.push(cell.execution_count);
        let text = cell.source.into_text();
        let start_line = source.lines().count() + 1;
        source.push_str(&text);
        if !text.ends_with('\n') {
            source.push('\n');
        }
        let end_line = source.lines().count();
        source.push('\n'); // separador en blanco entre celdas
        cell_ranges.push((start_line, end_line.max(start_line)));
    }
    Ok(NotebookExtraction { source, cell_ranges, execution_counts })
}

#[derive(Serialize)]
pub struct NotebookAnalysisResult {
    #[serde(flatten)]
    pub analysis: RichAnalysisResult,
    pub code_cells: usize,
    pub cell_ranges: Vec<(usize, usize)>,
    /// `execution_count` por celda de código, mismo orden que `cell_ranges`
    /// — viaja crudo además del resumen de `reproducibility` para que la UI
    /// pueda mostrar el `[N]` real de cada celda, no solo el veredicto.
    pub execution_counts: Vec<Option<u32>>,
    pub reproducibility: NotebookReproducibility,
}

fn empty_analysis(error: String) -> RichAnalysisResult {
    RichAnalysisResult {
        // Un notebook no tiene supresiones de tipo propias: las de sus
        // celdas ya las ve el analisis del pseudo-archivo concatenado.
        proof_surface: Default::default(),
        functions: Vec::new(),
        classes: Vec::new(),
        imports: Vec::new(),
        dead_code: Vec::new(),
        call_graph: Vec::new(),
        wasm_hints: Vec::new(),
        security_findings: Vec::new(),
        structural_smells: Vec::new(),
        naming_smells: Vec::new(),
        control_findings: Vec::new(),
        modernization: crate::hallazgos::pymodern::empty_report(),
        summary: RichSummary { total_functions: 0, total_classes: 0, total_imports: 0, avg_complexity: 0.0, max_loc_function: 0 },
        error: Some(error),
    }
}

/// Punto de entrada — extrae las celdas de código y corre el mismo
/// `analyze_rich` que ya analiza archivos `.py` sueltos, sin ningún cambio
/// a ese motor. Infalible: un `.ipynb` inválido devuelve un reporte vacío
/// con `error` seteado, mismo shape que `analyze_rich` ya usa para un `.py`
/// con error de sintaxis — nunca un 500.
pub fn analyze_notebook(content: &str) -> NotebookAnalysisResult {
    match extract_python_source(content) {
        Ok(extraction) => {
            let code_cells = extraction.cell_ranges.len();
            NotebookAnalysisResult {
                analysis: analyze_rich(&extraction.source),
                code_cells,
                cell_ranges: extraction.cell_ranges,
                reproducibility: reproducibility(&extraction.execution_counts),
                execution_counts: extraction.execution_counts,
            }
        }
        Err(e) => NotebookAnalysisResult {
            analysis: empty_analysis(e),
            code_cells: 0,
            cell_ranges: Vec::new(),
            execution_counts: Vec::new(),
            reproducibility: reproducibility(&[]),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notebook_json(cells: &[(&str, &str)]) -> String {
        let cell_objs: Vec<String> = cells
            .iter()
            .map(|(cell_type, src)| {
                let lines: Vec<String> = src.lines().map(|l| format!("\"{l}\\n\"")).collect();
                format!("{{\"cell_type\": \"{cell_type}\", \"source\": [{}]}}", lines.join(", "))
            })
            .collect();
        format!("{{\"cells\": [{}], \"nbformat\": 4}}", cell_objs.join(", "))
    }

    #[test]
    fn extrae_una_sola_celda_de_codigo() {
        let json = notebook_json(&[("code", "def add(a, b):\n    return a + b")]);
        let extraction = extract_python_source(&json).unwrap();
        assert!(extraction.source.contains("def add(a, b):"));
        assert_eq!(extraction.cell_ranges.len(), 1);
        assert_eq!(extraction.cell_ranges[0], (1, 2));
    }

    #[test]
    fn celdas_markdown_se_ignoran() {
        let json = notebook_json(&[("markdown", "# Título\nExplicación"), ("code", "x = 1")]);
        let extraction = extract_python_source(&json).unwrap();
        assert!(!extraction.source.contains("Título"));
        assert_eq!(extraction.cell_ranges.len(), 1);
    }

    #[test]
    fn multiples_celdas_de_codigo_dan_rangos_no_solapados() {
        let json = notebook_json(&[("code", "a = 1"), ("code", "b = 2\nc = 3")]);
        let extraction = extract_python_source(&json).unwrap();
        assert_eq!(extraction.cell_ranges.len(), 2);
        let (_s1, e1) = extraction.cell_ranges[0];
        let (s2, _e2) = extraction.cell_ranges[1];
        assert!(s2 > e1, "la segunda celda debe empezar después de que termine la primera");
    }

    #[test]
    fn json_invalido_da_error_en_vez_de_panic() {
        let result = extract_python_source("esto no es json");
        assert!(result.is_err());
    }

    #[test]
    fn json_valido_pero_sin_forma_de_notebook_da_error() {
        let result = extract_python_source("{\"foo\": \"bar\"}");
        // `cells` tiene `#[serde(default)]` -- un objeto sin esa clave es un
        // notebook vacío válido, no un error. Confirma ese comportamiento
        // explícitamente en vez de asumirlo.
        assert!(result.is_ok());
        assert_eq!(result.unwrap().cell_ranges.len(), 0);
    }

    #[test]
    fn analyze_notebook_corre_el_motor_python_real_sobre_las_celdas() {
        let json = notebook_json(&[("code", "def f(n):\n    for i in range(n):\n        for j in range(n):\n            pass")]);
        let result = analyze_notebook(&json);
        assert_eq!(result.code_cells, 1);
        assert_eq!(result.analysis.functions.len(), 1);
        assert_eq!(result.analysis.functions[0].big_o, "O(n\u{b2})");
    }

    #[test]
    fn notebook_invalido_no_panickea_y_reporta_error() {
        let result = analyze_notebook("no es json");
        assert_eq!(result.code_cells, 0);
        assert!(result.analysis.error.is_some());
        assert!(result.analysis.functions.is_empty());
    }

    #[test]
    fn source_como_string_unico_tambien_se_soporta() {
        // Forma alternativa del formato nbformat -- `source` como un solo
        // string en vez de un array de líneas.
        let json = "{\"cells\": [{\"cell_type\": \"code\", \"source\": \"x = 1\\ny = 2\"}]}";
        let extraction = extract_python_source(json).unwrap();
        assert!(extraction.source.contains("x = 1"));
        assert!(extraction.source.contains("y = 2"));
    }

    // ── Reproducibilidad (2026-09-02) ────────────────────────────────────

    /// Notebook con `execution_count` explícito por celda de código —
    /// `None` se serializa como `null`, igual que Jupyter escribe una celda
    /// sin ejecutar.
    fn notebook_with_counts(counts: &[Option<u32>]) -> String {
        let cells: Vec<String> = counts
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let count = c.map_or("null".to_string(), |n| n.to_string());
                format!("{{\"cell_type\": \"code\", \"execution_count\": {count}, \"source\": [\"x{i} = {i}\\n\"]}}")
            })
            .collect();
        format!("{{\"cells\": [{}], \"nbformat\": 4}}", cells.join(", "))
    }

    #[test]
    fn orden_creciente_no_reporta_nada() {
        let r = reproducibility(&[Some(1), Some(2), Some(3)]);
        assert_eq!(r.executed_cells, 3);
        assert_eq!(r.unexecuted_cells, 0);
        assert!(r.out_of_order_cells.is_empty());
        assert!(r.note.contains("orden creciente"));
    }

    #[test]
    fn celda_ejecutada_antes_que_una_anterior_se_marca() {
        // La celda 3 del archivo corrió con [2], después de la celda 2 que
        // corrió con [5] -- se ejecutó ANTES que algo que está más arriba.
        let r = reproducibility(&[Some(1), Some(5), Some(2)]);
        assert_eq!(r.out_of_order_cells, vec![3]);
        assert!(r.note.contains("Restart & Run All"));
    }

    #[test]
    fn saltos_hacia_adelante_no_son_desorden() {
        // [1, 7, 9]: hay huecos (celdas borradas, o ejecuciones repetidas),
        // pero el ORDEN relativo es correcto -- no es el problema que esta
        // señal reporta.
        assert!(reproducibility(&[Some(1), Some(7), Some(9)]).out_of_order_cells.is_empty());
    }

    #[test]
    fn celdas_sin_ejecutar_se_cuentan_pero_no_rompen_el_orden() {
        let r = reproducibility(&[Some(1), None, Some(2)]);
        assert_eq!(r.executed_cells, 2);
        assert_eq!(r.unexecuted_cells, 1);
        assert!(r.out_of_order_cells.is_empty(), "una celda sin ejecutar no tiene contador que ordenar");
        assert!(r.note.contains("sin ejecutar"));
    }

    #[test]
    fn tras_una_celda_desordenada_las_siguientes_se_comparan_contra_el_maximo() {
        // [1, 9, 2, 3]: la 3ra rompe el orden. Las que siguen (2 -> 3) están
        // bien entre sí, pero siguen por debajo del 9 ya visto, así que
        // TAMBIÉN se ejecutaron antes que la celda 2 -- ambas se marcan.
        // Comparar contra el ÚLTIMO valor en vez del máximo daría solo [3].
        let r = reproducibility(&[Some(1), Some(9), Some(2), Some(3)]);
        assert_eq!(r.out_of_order_cells, vec![3, 4]);
    }

    #[test]
    fn notebook_sin_ninguna_celda_de_codigo_no_rompe() {
        let r = reproducibility(&[]);
        assert_eq!(r.executed_cells, 0);
        assert_eq!(r.unexecuted_cells, 0);
        assert!(r.out_of_order_cells.is_empty());
    }

    #[test]
    fn execution_count_ausente_se_trata_como_sin_ejecutar() {
        // Un notebook armado a mano puede no traer la clave -- no es un
        // error de parseo, es una celda sin ejecutar.
        let json = "{\"cells\": [{\"cell_type\": \"code\", \"source\": [\"x = 1\\n\"]}]}";
        let extraction = extract_python_source(json).unwrap();
        assert_eq!(extraction.execution_counts, vec![None]);
    }

    #[test]
    fn los_contadores_siguen_el_orden_de_las_celdas_de_codigo_salteando_markdown() {
        let json = "{\"cells\": [\
            {\"cell_type\": \"code\", \"execution_count\": 3, \"source\": [\"a = 1\\n\"]}, \
            {\"cell_type\": \"markdown\", \"source\": [\"# titulo\\n\"]}, \
            {\"cell_type\": \"code\", \"execution_count\": 1, \"source\": [\"b = 2\\n\"]}]}";
        let extraction = extract_python_source(json).unwrap();
        // La celda markdown del medio NO ocupa un lugar -- los índices de
        // `execution_counts` calzan con los de `cell_ranges`.
        assert_eq!(extraction.execution_counts, vec![Some(3), Some(1)]);
        assert_eq!(extraction.cell_ranges.len(), 2);
        assert_eq!(reproducibility(&extraction.execution_counts).out_of_order_cells, vec![2]);
    }

    #[test]
    fn analyze_notebook_expone_la_reproducibilidad_end_to_end() {
        let json = notebook_with_counts(&[Some(1), Some(4), Some(2), None]);
        let result = analyze_notebook(&json);
        assert_eq!(result.code_cells, 4);
        assert_eq!(result.execution_counts, vec![Some(1), Some(4), Some(2), None]);
        assert_eq!(result.reproducibility.out_of_order_cells, vec![3]);
        assert_eq!(result.reproducibility.unexecuted_cells, 1);
    }

    #[test]
    fn notebook_invalido_devuelve_reproducibilidad_vacia_no_panic() {
        let result = analyze_notebook("no es json");
        assert_eq!(result.execution_counts.len(), 0);
        assert_eq!(result.reproducibility.executed_cells, 0);
    }
}
