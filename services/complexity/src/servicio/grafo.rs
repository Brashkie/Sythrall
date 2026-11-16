//! **Los grafos del proyecto — Fase 35, portado desde `routers/graph.py`.**
//!
//! Ese router no construía ningún grafo: parseaba los archivos, armaba uno de
//! tres payloads según el tipo pedido y se lo mandaba al motor por HTTP. Los
//! seis constructores (import, call, circular, centrality, heatmap,
//! architecture) ya vivían acá desde la Fase 18.
//!
//! **Lo único que había que portar eran los tres payloads**, y son tres
//! porque cada grafo necesita cosas distintas del mismo archivo parseado:
//!   - *import / circular / centrality* → `FileSummary`: cuántas funciones y
//!     qué importa, sin detalle por función.
//!   - *call* → `CallGraphFileInput`: detalle por función **y** el grafo de
//!     llamadas que cada parser ya calculó.
//!   - *heatmap* → `HeatmapFileInput`: igual que el anterior **más** `loc`,
//!     porque el área de cada celda sale de ahí.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::flujo::graph::{
    self, CallEdgeInput, CallGraphFileInput, CallGraphFunctionInput, HeatmapFileInput,
    HeatmapFunctionInput,
};
use crate::servicio::proyecto::{self, ArchivoCrudo};

#[derive(Deserialize)]
pub struct GrafoRequest {
    #[serde(default)]
    pub files: Vec<ArchivoCrudo>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default = "tipo_por_defecto")]
    pub graph_type: String,
}

fn tipo_por_defecto() -> String {
    "import".to_string()
}

/// Los tipos que el selector del panel ofrece. Es una lista estática y así
/// estaba también en Python: cada entrada describe un grafo que existe.
pub fn tipos() -> Value {
    json!({ "types": [
        { "id": "import", "label": "Import Graph",
          "description": "Dependencias entre archivos vía imports/requires" },
        { "id": "call", "label": "Call Graph",
          "description": "Qué función llama a cuál dentro de cada archivo" },
        { "id": "circular", "label": "Circular Dependencies",
          "description": "Ciclos en el grafo de dependencias entre módulos" },
        { "id": "heatmap", "label": "Complexity Heatmap",
          "description": "Mapa de calor por complejidad ciclomática y Big-O" },
        { "id": "centrality", "label": "Centrality / Hubs",
          "description": "Archivos más conectados del proyecto — mismo concepto que centralidad en redes sociales, aplicado a dependencias" },
    ]})
}

fn funciones_de(archivo: &Value) -> &[Value] {
    archivo.get("functions").and_then(Value::as_array).map_or(&[][..], Vec::as_slice)
}

fn texto(v: &Value, campo: &str, por_defecto: &str) -> String {
    v.get(campo).and_then(Value::as_str).unwrap_or(por_defecto).to_string()
}

fn numero(v: &Value, campo: &str, por_defecto: u64) -> u64 {
    v.get(campo).and_then(Value::as_u64).unwrap_or(por_defecto)
}

fn payload_llamadas(resultados: &[Value]) -> Vec<CallGraphFileInput> {
    resultados
        .iter()
        .map(|p| CallGraphFileInput {
            filename: texto(p, "filename", ""),
            functions: funciones_de(p)
                .iter()
                .map(|f| CallGraphFunctionInput {
                    name: texto(f, "name", ""),
                    big_o: texto(f, "big_o", "?"),
                    complexity: numero(f, "complexity", 1) as u32,
                    line: numero(f, "line", 0) as usize,
                })
                .collect(),
            call_graph: p
                .get("call_graph")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|e| CallEdgeInput { from: texto(e, "from", ""), to: texto(e, "to", "") })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect()
}

fn payload_heatmap(resultados: &[Value]) -> Vec<HeatmapFileInput> {
    resultados
        .iter()
        .map(|p| HeatmapFileInput {
            filename: texto(p, "filename", ""),
            functions: funciones_de(p)
                .iter()
                .map(|f| HeatmapFunctionInput {
                    name: texto(f, "name", ""),
                    big_o: texto(f, "big_o", "?"),
                    complexity: numero(f, "complexity", 1) as u32,
                    line: numero(f, "line", 0) as usize,
                    loc: numero(f, "loc", 0) as usize,
                })
                .collect(),
        })
        .collect()
}

/// Construye el grafo pedido sobre resultados ya parseados.
pub fn construir(tipo: &str, resultados: &[Value]) -> Value {
    match tipo {
        "import" => serde_json::to_value(graph::build_import_graph(
            proyecto::resumen_para_el_grafo(resultados),
        )),
        "circular" => serde_json::to_value(graph::build_circular_graph(
            proyecto::resumen_para_el_grafo(resultados),
        )),
        "centrality" => serde_json::to_value(graph::build_centrality_graph(
            proyecto::resumen_para_el_grafo(resultados),
        )),
        "call" => serde_json::to_value(graph::build_call_graph(payload_llamadas(resultados))),
        "heatmap" => serde_json::to_value(graph::build_heatmap(payload_heatmap(resultados))),
        otro => {
            return json!({ "error": format!("Tipo de grafo desconocido: {otro}") });
        }
    }
    .unwrap_or_else(|e| json!({ "error": e.to_string(), "mermaid": "", "nodes": [], "edges": [] }))
}

pub fn generar(req: &GrafoRequest) -> Value {
    let resultados = proyecto::resultados_de(req.project_id.as_deref(), &req.files);
    if resultados.is_empty() {
        return json!({
            "graph_type": req.graph_type,
            "nodes": [], "edges": [], "mermaid": "",
            "summary": { "total_files": 0 },
        });
    }
    construir(&req.graph_type, &resultados)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archivo(nombre: &str, imports: Value, funciones: Value) -> Value {
        json!({
            "filename": nombre, "language": "python", "loc": 10,
            "functions": funciones, "imports": imports, "classes": [], "dead_code": [],
            "call_graph": [],
        })
    }

    #[test]
    fn los_cinco_tipos_estan_en_la_lista() {
        let t = tipos();
        let ids: Vec<&str> =
            t["types"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["import", "call", "circular", "heatmap", "centrality"]);
    }

    #[test]
    fn un_tipo_desconocido_lo_dice_en_vez_de_devolver_un_grafo_vacio() {
        let r = construir("inventado", &[archivo("a.py", json!([]), json!([]))]);
        assert!(r["error"].as_str().unwrap().contains("inventado"));
    }

    /// **Los tres payloads no son intercambiables**, y confundirlos daría un
    /// grafo que parece bien pero está vacío de lo que importa.
    #[test]
    fn el_grafo_de_imports_ve_las_dependencias() {
        // El modulo es `b`, no `b.py`: asi lo escribe un `import b` real, y
        // es el grafo el que lo resuelve al archivo.
        let a = archivo("a.py", json!([{"module": "b", "line": 1}]), json!([]));
        let b = archivo("b.py", json!([]), json!([]));
        let r = construir("import", &[a, b]);
        assert_eq!(r["nodes"].as_array().unwrap().len(), 2);
        assert!(!r["edges"].as_array().unwrap().is_empty(), "la arista a->b tiene que estar");
    }

    #[test]
    fn el_heatmap_usa_el_loc_de_cada_funcion() {
        let fns = json!([{"name": "f", "line": 1, "complexity": 7, "big_o": "O(n²)", "loc": 40}]);
        let r = construir("heatmap", &[archivo("a.py", json!([]), fns)]);
        // El heatmap no devuelve nodos: devuelve una fila por funcion, con su
        // nivel y su color ya resueltos.
        let celdas = r["functions"].as_array().expect("sin funciones");
        assert_eq!(celdas.len(), 1);
        assert_eq!(celdas[0]["loc"], 40, "el loc por funcion es lo que da el area");
        assert_eq!(celdas[0]["cc"], 7);
        assert_eq!(r["summary"]["hot_paths"], 1, "O(n²) es camino caliente");
    }

    #[test]
    fn el_grafo_de_llamadas_usa_el_detalle_por_funcion() {
        let mut a = archivo(
            "a.py",
            json!([]),
            json!([{"name": "f", "line": 1, "complexity": 2, "big_o": "O(n)"},
                   {"name": "g", "line": 5, "complexity": 1, "big_o": "O(1)"}]),
        );
        a["call_graph"] = json!([{"from": "f", "to": "g"}]);
        let r = construir("call", &[a]);
        assert_eq!(r["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(r["edges"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn sin_archivos_devuelve_un_grafo_vacio_y_no_un_error() {
        let r = generar(&GrafoRequest {
            files: vec![],
            project_id: None,
            graph_type: "import".to_string(),
        });
        assert_eq!(r["summary"]["total_files"], 0);
        assert!(r.get("error").is_none());
    }
}
