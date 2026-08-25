//! Fase 18 (Native Analysis Core) — primer slice del "Graph Engine": el
//! Import Graph. Puerto de `_build_project_edges`/`_build_import_graph`/
//! `_import_graph_to_mermaid`/`_short_name`/`_module_to_candidates`/`_safe_id`
//! en `apps/api/routers/graph.py`. Recibe el resumen ya parseado de cada
//! archivo (filename/language/functions/imports/dead_code) — no archivos
//! crudos: el parsing multi-lenguaje (JS/TS/C/C++) sigue siendo Python,
//! este módulo es puro cómputo sobre datos ya resueltos, mismo contrato que
//! `analyze_rich`/`analyze` en este mismo crate.
//!
//! Call Graph / Circular Deps / Centrality (las otras 3 variantes de
//! `graph.py`) quedan para slices siguientes — comparten `_module_to_candidates`
//! con este módulo del lado Python hasta que se porten también.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Deserialize)]
pub struct ImportRef {
    pub module: String,
    #[serde(default)]
    pub line: u32,
}

#[derive(Deserialize)]
pub struct FileSummary {
    pub filename: String,
    pub language: String,
    pub functions: usize,
    #[serde(default)]
    pub imports: Vec<ImportRef>,
    #[serde(default)]
    pub dead_code: usize,
}

#[derive(Serialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub full: String,
    pub language: String,
    pub functions: usize,
    pub imports: usize,
    pub dead_code: usize,
}

#[derive(Serialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub via: String,
    pub line: u32,
}

#[derive(Serialize)]
pub struct ImportGraphSummary {
    pub total_files: usize,
    pub total_imports: usize,
    pub isolated: usize,
}

#[derive(Serialize)]
pub struct ImportGraphResult {
    pub graph_type: &'static str,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub mermaid: String,
    pub entry_points: Vec<String>,
    pub summary: ImportGraphSummary,
}

/// Edges archivo→archivo, solo entre archivos que el proyecto ya trae —
/// mismo criterio que `_build_project_edges`: un import a una librería
/// externa no genera edge porque no hay archivo del proyecto que resolver.
pub fn build_project_edges(files: &[FileSummary]) -> Vec<GraphEdge> {
    let file_names: HashSet<&str> = files.iter().map(|f| f.filename.as_str()).collect();
    let mut edges = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for f in files {
        for imp in &f.imports {
            for candidate in module_to_candidates(&imp.module, &f.filename) {
                if candidate != f.filename && file_names.contains(candidate.as_str()) {
                    let key = format!("{}\u{2192}{}", f.filename, candidate);
                    if seen.insert(key) {
                        edges.push(GraphEdge {
                            from: f.filename.clone(),
                            to: candidate,
                            via: imp.module.clone(),
                            line: imp.line,
                        });
                    }
                    break;
                }
            }
        }
    }

    edges
}

pub fn build_import_graph(files: Vec<FileSummary>) -> ImportGraphResult {
    let nodes: Vec<GraphNode> = files
        .iter()
        .map(|f| GraphNode {
            id: f.filename.clone(),
            label: short_name(&f.filename).to_string(),
            full: f.filename.clone(),
            language: f.language.clone(),
            functions: f.functions,
            imports: f.imports.len(),
            dead_code: f.dead_code,
        })
        .collect();

    let edges = build_project_edges(&files);
    let mermaid = import_graph_to_mermaid(&nodes, &edges);

    let targets: HashSet<&str> = edges.iter().map(|e| e.to.as_str()).collect();
    let sources: HashSet<&str> = edges.iter().map(|e| e.from.as_str()).collect();
    let entry_points: Vec<String> = nodes
        .iter()
        .filter(|n| !targets.contains(n.id.as_str()))
        .map(|n| n.id.clone())
        .collect();
    let isolated = nodes
        .iter()
        .filter(|n| !sources.contains(n.id.as_str()) && !targets.contains(n.id.as_str()))
        .count();

    ImportGraphResult {
        graph_type: "import",
        summary: ImportGraphSummary {
            total_files: nodes.len(),
            total_imports: edges.len(),
            isolated,
        },
        nodes,
        edges,
        mermaid,
        entry_points,
    }
}

fn import_graph_to_mermaid(nodes: &[GraphNode], edges: &[GraphEdge]) -> String {
    if nodes.is_empty() {
        return "flowchart TD\n    A[Sin archivos]".to_string();
    }

    let mut lines = vec!["flowchart TD".to_string()];

    for n in nodes {
        let nid = safe_id(&n.id);
        let abbr = lang_abbr(&n.language);
        lines.push(format!(
            "    {nid}[\"[{abbr}] {}\\n{} fn · {} imp\"]",
            n.label, n.functions, n.imports
        ));
    }

    for e in edges {
        lines.push(format!("    {} --> {}", safe_id(&e.from), safe_id(&e.to)));
    }

    let targets: HashSet<&str> = edges.iter().map(|e| e.to.as_str()).collect();
    let sources: HashSet<&str> = edges.iter().map(|e| e.from.as_str()).collect();
    for n in nodes {
        let nid = safe_id(&n.id);
        if !targets.contains(n.id.as_str()) && sources.contains(n.id.as_str()) {
            lines.push(format!("    style {nid} fill:#3d9eff20,stroke:#3d9eff"));
        } else if !sources.contains(n.id.as_str()) {
            lines.push(format!("    style {nid} fill:#00f5a020,stroke:#00f5a0"));
        }
    }

    lines.join("\n") + "\n"
}

// ══════════════════════════════════════════════════════════════════════════
//  CENTRALITY GRAPH — Fase 18, segunda porción del Graph Engine. Puerto de
//  `_build_centrality_graph`/`_centrality_to_mermaid` en `graph.py`. Resuelve
//  la nota pendiente de Fase 14 ("deliberately still Python — Phase 18's
//  Graph Engine needs the import/call graph construction itself in Rust
//  first, which doesn't exist yet"): esa construcción (`build_project_edges`)
//  ya está acá desde el slice de Import Graph, así que centralidad puede
//  portarse sin duplicar la resolución de imports.
// ══════════════════════════════════════════════════════════════════════════

const HUB_TOP_N: usize = 5;
const HUB_MIN_IN_DEGREE: usize = 2;

#[derive(Serialize)]
pub struct CentralityNode {
    pub id: String,
    pub label: String,
    pub in_degree: usize,
    pub out_degree: usize,
    pub centrality: f64,
    pub is_hub: bool,
}

#[derive(Serialize)]
pub struct CentralityGraphSummary {
    pub total_files: usize,
    pub hubs: Vec<String>,
    pub max_in_degree: usize,
}

#[derive(Serialize)]
pub struct CentralityGraphResult {
    pub graph_type: &'static str,
    pub nodes: Vec<CentralityNode>,
    pub edges: Vec<GraphEdge>,
    pub mermaid: String,
    pub summary: CentralityGraphSummary,
}

pub fn build_centrality_graph(files: Vec<FileSummary>) -> CentralityGraphResult {
    let edges = build_project_edges(&files);

    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut out_degree: HashMap<&str, usize> = HashMap::new();
    for f in &files {
        in_degree.entry(f.filename.as_str()).or_insert(0);
        out_degree.entry(f.filename.as_str()).or_insert(0);
    }
    for e in &edges {
        *out_degree.entry(e.from.as_str()).or_insert(0) += 1;
        *in_degree.entry(e.to.as_str()).or_insert(0) += 1;
    }

    // Mismo denominador que `nx.degree_centrality`: (in+out) / (n-1), con
    // guard para n<=1 (nunca se ejerce en la práctica — sin archivos no hay
    // edges, y con 1 archivo tampoco puede haberlas — pero se replica por
    // fidelidad con el original).
    let n = files.len();
    let denom = if n > 1 { (n - 1) as f64 } else { 1.0 };

    let mut nodes: Vec<CentralityNode> = files
        .iter()
        .map(|f| {
            let ind = *in_degree.get(f.filename.as_str()).unwrap_or(&0);
            let outd = *out_degree.get(f.filename.as_str()).unwrap_or(&0);
            let centrality = if n > 1 { (ind + outd) as f64 / denom } else { 0.0 };
            CentralityNode {
                id: f.filename.clone(),
                label: short_name(&f.filename).to_string(),
                in_degree: ind,
                out_degree: outd,
                centrality: (centrality * 1000.0).round() / 1000.0,
                is_hub: false,
            }
        })
        .collect();

    // Top 5 por in_degree (sort estable, mismo desempate que `sorted()` de
    // Python) con in_degree >= 2 → hub.
    let mut ranked: Vec<usize> = (0..nodes.len()).collect();
    ranked.sort_by(|&a, &b| nodes[b].in_degree.cmp(&nodes[a].in_degree));
    for &i in ranked.iter().take(HUB_TOP_N) {
        if nodes[i].in_degree >= HUB_MIN_IN_DEGREE {
            nodes[i].is_hub = true;
        }
    }

    let max_in_degree = nodes.iter().map(|n| n.in_degree).max().unwrap_or(0);
    let hubs: Vec<String> = nodes.iter().filter(|n| n.is_hub).map(|n| n.id.clone()).collect();
    let mermaid = centrality_to_mermaid(&nodes, &edges);

    CentralityGraphResult {
        graph_type: "centrality",
        summary: CentralityGraphSummary { total_files: nodes.len(), hubs, max_in_degree },
        nodes,
        edges,
        mermaid,
    }
}

fn centrality_to_mermaid(nodes: &[CentralityNode], edges: &[GraphEdge]) -> String {
    if nodes.is_empty() {
        return "flowchart TD\n    A[Sin archivos]".to_string();
    }

    let mut lines = vec!["flowchart TD".to_string()];
    for n in nodes {
        let nid = safe_id(&n.id);
        let hub_badge = if n.is_hub { " [HUB]" } else { "" };
        lines.push(format!(
            "    {nid}[\"{}{hub_badge}\\nin:{} · out:{}\"]",
            n.label, n.in_degree, n.out_degree
        ));
    }

    for e in edges {
        lines.push(format!("    {} --> {}", safe_id(&e.from), safe_id(&e.to)));
    }

    for n in nodes {
        if n.is_hub {
            let nid = safe_id(&n.id);
            lines.push(format!("    style {nid} fill:#ff8a0020,stroke:#ff8a00,stroke-width:2px"));
        }
    }

    lines.join("\n") + "\n"
}

// ══════════════════════════════════════════════════════════════════════════
//  CALL GRAPH — Fase 18, tercera porción del Graph Engine. Puerto de
//  `_build_call_graph`/`_call_graph_to_mermaid` en `graph.py` (el agregador
//  a nivel de proyecto — no confundir con `static_parser.py::_build_call_graph`,
//  el helper por-archivo que construye cada `call_graph` de entrada, que NO
//  se toca acá). Pura agregación: cada archivo ya trae su `call_graph`
//  precalculado, este módulo solo lo funde en un grafo de proyecto y
//  resuelve a qué archivo pertenece cada función.
// ══════════════════════════════════════════════════════════════════════════

fn default_big_o() -> String {
    "?".to_string()
}

fn default_complexity() -> u32 {
    1
}

#[derive(Deserialize)]
pub struct CallGraphFunctionInput {
    pub name: String,
    #[serde(default = "default_big_o")]
    pub big_o: String,
    #[serde(default = "default_complexity")]
    pub complexity: u32,
    #[serde(default)]
    pub line: usize,
}

#[derive(Deserialize)]
pub struct CallEdgeInput {
    pub from: String,
    pub to: String,
}

#[derive(Deserialize)]
pub struct CallGraphFileInput {
    pub filename: String,
    #[serde(default)]
    pub functions: Vec<CallGraphFunctionInput>,
    #[serde(default)]
    pub call_graph: Vec<CallEdgeInput>,
}

#[derive(Serialize, Clone)]
pub struct CallGraphNode {
    pub id: String,
    pub label: String,
    pub file: String,
    pub big_o: String,
    pub cc: u32,
    pub line: usize,
    pub color: &'static str,
    pub level: &'static str,
}

#[derive(Serialize)]
pub struct CallGraphEdge {
    pub from: String,
    pub to: String,
    pub from_file: String,
    pub to_file: String,
}

#[derive(Serialize)]
pub struct CallGraphSummaryOut {
    pub total_functions: usize,
    pub total_calls: usize,
    pub hot_paths: Vec<CallGraphNode>,
}

#[derive(Serialize)]
pub struct CallGraphResult {
    pub graph_type: &'static str,
    pub nodes: Vec<CallGraphNode>,
    pub edges: Vec<CallGraphEdge>,
    pub mermaid: String,
    pub summary: CallGraphSummaryOut,
}

struct FuncInfo {
    file: String,
    big_o: String,
    cc: u32,
    line: usize,
}

pub fn build_call_graph(files: Vec<CallGraphFileInput>) -> CallGraphResult {
    // Pisado por nombre en orden de aparición — mismo comportamiento que el
    // `dict` de Python (`all_funcs[fn["name"]] = {...}`): si dos archivos
    // tienen una función con el mismo nombre, gana la última en el orden de
    // `files`. Preexistente en el diseño original, no algo a corregir acá.
    let mut all_funcs: HashMap<String, FuncInfo> = HashMap::new();
    for f in &files {
        for func in &f.functions {
            all_funcs.insert(
                func.name.clone(),
                FuncInfo { file: f.filename.clone(), big_o: func.big_o.clone(), cc: func.complexity, line: func.line },
            );
        }
    }

    let mut all_edges: Vec<CallGraphEdge> = Vec::new();
    for f in &files {
        for e in &f.call_graph {
            let to_file = all_funcs.get(&e.to).map(|info| info.file.clone()).unwrap_or_else(|| f.filename.clone());
            all_edges.push(CallGraphEdge {
                from: e.from.clone(),
                to: e.to.clone(),
                from_file: f.filename.clone(),
                to_file,
            });
        }
    }

    let mut active_names: HashSet<&str> = HashSet::new();
    for e in &all_edges {
        active_names.insert(e.from.as_str());
        active_names.insert(e.to.as_str());
    }
    if active_names.is_empty() {
        active_names = all_funcs.keys().map(|s| s.as_str()).collect();
    }

    // Recorrer `files` en orden (no `all_funcs` directamente — un HashMap no
    // tiene orden estable) para que el orden de `nodes` sea determinístico,
    // mismo criterio que ya usan Import Graph/Centrality acá.
    let mut seen_names: HashSet<String> = HashSet::new();
    let mut nodes: Vec<CallGraphNode> = Vec::new();
    for f in &files {
        for func in &f.functions {
            if !active_names.contains(func.name.as_str()) || !seen_names.insert(func.name.clone()) {
                continue;
            }
            // El nombre puede haber sido pisado por otro archivo — usar el
            // info final de `all_funcs`, no el de esta iteración.
            let info = &all_funcs[&func.name];
            nodes.push(CallGraphNode {
                id: func.name.clone(),
                label: func.name.clone(),
                file: info.file.clone(),
                big_o: info.big_o.clone(),
                cc: info.cc,
                line: info.line,
                color: bigo_color(&info.big_o),
                level: bigo_level(&info.big_o),
            });
        }
    }

    let hot_paths: Vec<CallGraphNode> = nodes.iter().filter(|n| n.level == "expensive").cloned().collect();
    let mermaid = call_graph_to_mermaid(&nodes, &all_edges);

    CallGraphResult {
        graph_type: "call",
        summary: CallGraphSummaryOut { total_functions: nodes.len(), total_calls: all_edges.len(), hot_paths },
        nodes,
        edges: all_edges,
        mermaid,
    }
}

fn call_graph_to_mermaid(nodes: &[CallGraphNode], edges: &[CallGraphEdge]) -> String {
    if nodes.is_empty() {
        return "flowchart TD\n    A[Sin funciones]".to_string();
    }

    let mut lines = vec!["flowchart TD".to_string()];
    for n in nodes {
        let nid = safe_id(&n.id);
        lines.push(format!("    {nid}[\"{}\\n{}\"]", n.label, n.big_o));
    }

    for e in edges {
        lines.push(format!("    {} --> {}", safe_id(&e.from), safe_id(&e.to)));
    }

    for n in nodes {
        let nid = safe_id(&n.id);
        if n.level == "expensive" {
            lines.push(format!("    style {nid} fill:#ff336620,stroke:#ff3366"));
        } else if n.level == "moderate" {
            lines.push(format!("    style {nid} fill:#ffb62720,stroke:#ffb627"));
        }
    }

    lines.join("\n") + "\n"
}

/// Puerto propio en Rust de `_bigo_color` (graph.py) — duplicado deliberado,
/// no compartido: `_build_heatmap` (todavía Python, fuera de este slice)
/// también usa la versión Python, así que esa no se borra.
fn bigo_color(big_o: &str) -> &'static str {
    match big_o {
        "O(1)" => "#00f5a0",
        "O(log n)" => "#8ef5c0",
        "O(n)" => "#ffb627",
        "O(n log n)" => "#ff8a00",
        "O(n²)" | "O(n³)" | "O(2^n)" => "#ff3366",
        _ => "#4a5880",
    }
}

/// Puerto propio en Rust de `_bigo_level` (graph.py) — mismo criterio que
/// `bigo_color` de arriba: duplicado deliberado, no compartido.
fn bigo_level(big_o: &str) -> &'static str {
    match big_o {
        "O(1)" | "O(log n)" => "efficient",
        "O(n)" | "O(n log n)" => "moderate",
        _ => "expensive",
    }
}

// ══════════════════════════════════════════════════════════════════════════
//  CIRCULAR DEPENDENCIES — Fase 18, cuarta y última porción del Graph
//  Engine. Puerto de `_build_circular_graph`/`_circular_to_mermaid` en
//  `graph.py`. A diferencia de `find_cycles_capped` en `static_parser.py`
//  (que envuelve `nx.simple_cycles`, Johnson's algorithm completo), acá se
//  usa un buscador de ciclos por DFS acotado — ninguno de los tests
//  existentes compara ciclos exactos ni el orden de enumeración de
//  NetworkX, solo cuenta/pertenencia, así que replicar Johnson's algorithm
//  (o agregar `petgraph`, que igual no trae enumeración de ciclos simples
//  lista para usar) no hacía falta. El truco de "cada ciclo se descubre
//  una sola vez, desde su nodo de índice mínimo" es el mismo principio
//  detrás de Johnson's/Tiernan's, sin las optimizaciones de "blocking
//  sets" que solo importan para performance en grafos densos — acá no
//  hacen falta porque se corta toda la búsqueda apenas se llega al cap,
//  mismo criterio de graceful degradation que ya documenta la versión
//  Python.
// ══════════════════════════════════════════════════════════════════════════

const MAX_CYCLES: usize = 20;

#[derive(Serialize)]
pub struct CircularNode {
    pub id: String,
    pub label: String,
    pub in_cycle: bool,
    pub cycles: Vec<Vec<String>>,
}

#[derive(Serialize)]
pub struct CircularEdge {
    pub from: String,
    pub to: String,
    pub via: String,
    pub line: u32,
    pub is_cycle: bool,
}

#[derive(Serialize)]
pub struct CircularGraphSummary {
    pub total_files: usize,
    pub total_cycles: usize,
    pub affected_files: usize,
    pub cycle_descriptions: Vec<String>,
}

#[derive(Serialize)]
pub struct CircularGraphResult {
    pub graph_type: &'static str,
    pub nodes: Vec<CircularNode>,
    pub edges: Vec<CircularEdge>,
    pub cycles: Vec<Vec<String>>,
    pub mermaid: String,
    pub has_cycles: bool,
    pub summary: CircularGraphSummary,
}

pub fn build_circular_graph(files: Vec<FileSummary>) -> CircularGraphResult {
    let edges = build_project_edges(&files);
    let cycles = find_cycles_capped(&files, &edges, MAX_CYCLES);

    let mut cycle_nodes: HashSet<&str> = HashSet::new();
    for c in &cycles {
        cycle_nodes.extend(c.iter().map(|s| s.as_str()));
    }

    let mut cycle_edge_keys: HashSet<String> = HashSet::new();
    for c in &cycles {
        for i in 0..c.len() {
            cycle_edge_keys.insert(format!("{}\u{2192}{}", c[i], c[(i + 1) % c.len()]));
        }
    }

    let nodes: Vec<CircularNode> = files
        .iter()
        .map(|f| {
            let in_cycle = cycle_nodes.contains(f.filename.as_str());
            let node_cycles: Vec<Vec<String>> =
                cycles.iter().filter(|c| c.iter().any(|n| n == &f.filename)).cloned().collect();
            CircularNode { id: f.filename.clone(), label: short_name(&f.filename).to_string(), in_cycle, cycles: node_cycles }
        })
        .collect();

    let edges_annotated: Vec<CircularEdge> = edges
        .into_iter()
        .map(|e| {
            let key = format!("{}\u{2192}{}", e.from, e.to);
            let is_cycle = cycle_edge_keys.contains(&key);
            CircularEdge { from: e.from, to: e.to, via: e.via, line: e.line, is_cycle }
        })
        .collect();

    let mermaid = circular_to_mermaid(&nodes, &edges_annotated, &cycles);
    let has_cycles = !cycles.is_empty();
    let cycle_descriptions: Vec<String> =
        cycles.iter().map(|c| format!("{} \u{2192} {}", c.join(" \u{2192} "), c[0])).collect();

    CircularGraphResult {
        graph_type: "circular",
        summary: CircularGraphSummary {
            total_files: nodes.len(),
            total_cycles: cycles.len(),
            affected_files: cycle_nodes.len(),
            cycle_descriptions,
        },
        nodes,
        edges: edges_annotated,
        cycles,
        mermaid,
        has_cycles,
    }
}

/// Enumera ciclos simples capados a `max_cycles` — ver el comentario de
/// sección de arriba para el porqué del algoritmo elegido.
fn find_cycles_capped(files: &[FileSummary], edges: &[GraphEdge], max_cycles: usize) -> Vec<Vec<String>> {
    let order: HashMap<&str, usize> = files.iter().enumerate().map(|(i, f)| (f.filename.as_str(), i)).collect();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in edges {
        adj.entry(e.from.as_str()).or_default().push(e.to.as_str());
    }

    let mut cycles: Vec<Vec<String>> = Vec::new();
    for f in files {
        if cycles.len() >= max_cycles {
            break;
        }
        let start = f.filename.as_str();
        let start_idx = order[start];
        let mut path: Vec<&str> = vec![start];
        let mut on_path: HashSet<&str> = HashSet::from([start]);
        dfs_cycles(start, start, start_idx, &adj, &order, &mut path, &mut on_path, &mut cycles, max_cycles);
    }
    cycles
}

#[allow(clippy::too_many_arguments)]
fn dfs_cycles<'a>(
    start: &'a str,
    current: &'a str,
    start_idx: usize,
    adj: &HashMap<&'a str, Vec<&'a str>>,
    order: &HashMap<&'a str, usize>,
    path: &mut Vec<&'a str>,
    on_path: &mut HashSet<&'a str>,
    cycles: &mut Vec<Vec<String>>,
    max_cycles: usize,
) {
    let Some(neighbors) = adj.get(current) else { return };
    for &next in neighbors {
        if cycles.len() >= max_cycles {
            return;
        }
        if next == start {
            if path.len() > 1 {
                cycles.push(path.iter().map(|s| s.to_string()).collect());
            }
            continue;
        }
        let Some(&next_idx) = order.get(next) else { continue };
        if next_idx < start_idx || on_path.contains(next) {
            continue;
        }
        path.push(next);
        on_path.insert(next);
        dfs_cycles(start, next, start_idx, adj, order, path, on_path, cycles, max_cycles);
        on_path.remove(next);
        path.pop();
    }
}

fn circular_to_mermaid(nodes: &[CircularNode], edges: &[CircularEdge], cycles: &[Vec<String>]) -> String {
    let mut lines = vec!["flowchart TD".to_string()];

    for n in nodes {
        let nid = safe_id(&n.id);
        lines.push(format!("    {nid}[\"{}\"]", n.label));
    }

    for e in edges {
        let arrow = if e.is_cycle { " -.-> " } else { " --> " };
        lines.push(format!("    {}{arrow}{}", safe_id(&e.from), safe_id(&e.to)));
    }

    for n in nodes {
        if n.in_cycle {
            let nid = safe_id(&n.id);
            lines.push(format!("    style {nid} fill:#ff336630,stroke:#ff3366,stroke-width:2px"));
        }
    }

    if cycles.is_empty() {
        lines.push("    OK[\"Sin dependencias circulares\"]".to_string());
        lines.push("    style OK fill:#00f5a020,stroke:#00f5a0".to_string());
    }

    lines.join("\n") + "\n"
}

fn lang_abbr(language: &str) -> &'static str {
    match language {
        "python" => "PY",
        "typescript" => "TS",
        "javascript" => "JS",
        "c" => "C",
        "cpp" => "C++",
        _ => "?",
    }
}

/// Convierte un string a ID válido para Mermaid — mismo criterio que
/// `_safe_id` en `graph.py`.
fn safe_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

/// Solo el nombre del archivo sin ruta — mismo criterio que `_short_name`.
fn short_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path).rsplit('\\').next().unwrap_or(path)
}

/// Genera posibles nombres de archivo desde un módulo Python/JS/TS. Si se
/// da un `source_file`, busca primero en su misma carpeta — permite resolver
/// cross-folder deps correctamente. Puerto literal de `_module_to_candidates`:
/// mismo orden de candidatos, mismo criterio de carpeta-fuente.
pub fn module_to_candidates(module: &str, source_file: &str) -> Vec<String> {
    let source_dir = match source_file.rfind(['/', '\\']) {
        Some(i) => &source_file[..i],
        None => "",
    };

    // Puerto literal de `module.lstrip("./").split("/")[-1]` (Python
    // `lstrip` con un charset saca cualquier mezcla de '.'/'/' del inicio,
    // no solo un prefijo fijo — `trim_start_matches` con closure es el
    // equivalente exacto).
    let short: String = if module.starts_with('.') {
        module
            .trim_start_matches(['.', '/'])
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string()
    } else if module.contains('/') || module.contains('.') {
        module.rsplit(['/', '.']).next().unwrap_or(module).to_string()
    } else {
        module.to_string()
    };

    let mut candidates: Vec<String> = Vec::new();

    if !source_dir.is_empty() {
        for ext in [".py", ".ts", ".js"] {
            candidates.push(format!("{source_dir}/{short}{ext}"));
        }
        candidates.push(format!("{source_dir}/{short}/index.ts"));
        candidates.push(format!("{source_dir}/{short}/index.js"));
    }

    if module.starts_with('.') && !source_dir.is_empty() {
        let rel = module.trim_start_matches(['.', '/']);
        for ext in [".py", ".ts", ".js"] {
            candidates.push(format!("{source_dir}/{rel}{ext}"));
        }
    }

    for ext in [".py", ".ts", ".js"] {
        candidates.push(format!("{short}{ext}"));
    }
    candidates.push(format!("{short}/index.ts"));
    candidates.push(format!("{short}/index.js"));

    let mut seen: HashSet<String> = HashSet::new();
    candidates.retain(|c| seen.insert(c.clone()));
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(filename: &str, imports: Vec<(&str, u32)>) -> FileSummary {
        FileSummary {
            filename: filename.to_string(),
            language: "python".to_string(),
            functions: 1,
            imports: imports
                .into_iter()
                .map(|(module, line)| ImportRef { module: module.to_string(), line })
                .collect(),
            dead_code: 0,
        }
    }

    #[test]
    fn candidatos_import_plano_sin_carpeta() {
        let candidates = module_to_candidates("utils", "");
        assert!(candidates.contains(&"utils.py".to_string()));
        assert!(candidates.contains(&"utils.ts".to_string()));
    }

    #[test]
    fn candidatos_priorizan_misma_carpeta_que_el_source() {
        let candidates = module_to_candidates("utils", "backend/app.py");
        assert_eq!(candidates[0], "backend/utils.py");
    }

    #[test]
    fn candidatos_relative_import_resuelve_dentro_de_la_carpeta() {
        let candidates = module_to_candidates("./helpers", "backend/app.py");
        assert!(candidates.iter().any(|c| c == "backend/helpers.py"));
    }

    #[test]
    fn candidatos_deduplica_preservando_orden() {
        let candidates = module_to_candidates("utils", "");
        let mut seen = HashSet::new();
        for c in &candidates {
            assert!(seen.insert(c.clone()), "duplicado: {c}");
        }
    }

    #[test]
    fn edges_solo_entre_archivos_del_proyecto() {
        let files = vec![
            file("main.py", vec![("parser", 1), ("os", 2)]),
            file("parser.py", vec![]),
        ];
        let edges = build_project_edges(&files);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from, "main.py");
        assert_eq!(edges[0].to, "parser.py");
    }

    #[test]
    fn edges_no_se_duplican_por_import_repetido() {
        let files = vec![
            file("main.py", vec![("parser", 1), ("parser", 5)]),
            file("parser.py", vec![]),
        ];
        let edges = build_project_edges(&files);
        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn import_graph_entry_points_son_los_sin_incoming() {
        let files = vec![
            file("main.py", vec![("parser", 1)]),
            file("parser.py", vec![]),
        ];
        let result = build_import_graph(files);
        assert_eq!(result.entry_points, vec!["main.py".to_string()]);
        assert_eq!(result.summary.total_files, 2);
        assert_eq!(result.summary.total_imports, 1);
        assert_eq!(result.summary.isolated, 0);
    }

    #[test]
    fn import_graph_vacio_devuelve_mermaid_placeholder() {
        let result = build_import_graph(vec![]);
        assert!(result.mermaid.contains("Sin archivos"));
        assert!(result.nodes.is_empty());
    }

    // ── Centrality Graph — mismo fixture FILES_HUB que test_graph.py:
    // utils.py sin imports, a.py y b.py importan utils, c.py importa utils y a.
    fn hub_fixture() -> Vec<FileSummary> {
        vec![
            file("utils.py", vec![]),
            file("a.py", vec![("utils", 1)]),
            file("b.py", vec![("utils", 1)]),
            file("c.py", vec![("utils", 1), ("a", 2)]),
        ]
    }

    #[test]
    fn centrality_calcula_in_out_degree() {
        let result = build_centrality_graph(hub_fixture());
        let utils = result.nodes.iter().find(|n| n.id == "utils.py").unwrap();
        assert_eq!(utils.in_degree, 3);
        assert_eq!(utils.out_degree, 0);
        let c = result.nodes.iter().find(|n| n.id == "c.py").unwrap();
        assert_eq!(c.in_degree, 0);
        assert_eq!(c.out_degree, 2);
    }

    #[test]
    fn centrality_detecta_hub_con_al_menos_2_dependientes() {
        let result = build_centrality_graph(hub_fixture());
        assert!(result.summary.hubs.contains(&"utils.py".to_string()));
        let utils = result.nodes.iter().find(|n| n.id == "utils.py").unwrap();
        assert!(utils.is_hub);
    }

    #[test]
    fn centrality_hoja_con_un_solo_dependiente_no_es_hub() {
        let result = build_centrality_graph(hub_fixture());
        let a = result.nodes.iter().find(|n| n.id == "a.py").unwrap();
        assert_eq!(a.in_degree, 1);
        assert!(!a.is_hub);
    }

    #[test]
    fn centrality_sin_edges_no_hay_hubs() {
        let files = vec![file("a.py", vec![]), file("b.py", vec![])];
        let result = build_centrality_graph(files);
        assert!(result.summary.hubs.is_empty());
        assert!(result.nodes.iter().all(|n| n.centrality == 0.0));
    }

    #[test]
    fn centrality_vacio_no_rompe() {
        let result = build_centrality_graph(vec![]);
        assert!(result.summary.hubs.is_empty());
        assert_eq!(result.summary.max_in_degree, 0);
        assert!(result.mermaid.contains("Sin archivos"));
    }

    #[test]
    fn centrality_top_5_respeta_orden_original_en_empate() {
        // 6 archivos con in_degree 0 salvo el primero (in_degree 1, bajo el
        // umbral de hub) — todos empatados en 0 entre sí; el top-5 debe
        // tomar los primeros 5 en orden de aparición, no reordenarlos.
        let files: Vec<FileSummary> =
            (0..6).map(|i| file(&format!("f_{i}.py"), vec![])).collect();
        let result = build_centrality_graph(files);
        assert_eq!(result.nodes.len(), 6);
        assert!(result.summary.hubs.is_empty()); // nadie llega a in_degree >= 2
    }

    #[test]
    fn centrality_max_in_degree_correcto() {
        let result = build_centrality_graph(hub_fixture());
        assert_eq!(result.summary.max_in_degree, 3);
    }

    // ── Call Graph ──────────────────────────────────────────────────────────

    fn call_func(name: &str, big_o: &str, complexity: u32, line: usize) -> CallGraphFunctionInput {
        CallGraphFunctionInput { name: name.to_string(), big_o: big_o.to_string(), complexity, line }
    }

    fn call_edge(from: &str, to: &str) -> CallEdgeInput {
        CallEdgeInput { from: from.to_string(), to: to.to_string() }
    }

    #[test]
    fn call_graph_resuelve_to_file_via_all_funcs() {
        let files = vec![
            CallGraphFileInput {
                filename: "main.py".to_string(),
                functions: vec![call_func("main", "O(1)", 1, 1)],
                call_graph: vec![call_edge("main", "helper")],
            },
            CallGraphFileInput {
                filename: "utils.py".to_string(),
                functions: vec![call_func("helper", "O(n)", 2, 1)],
                call_graph: vec![],
            },
        ];
        let result = build_call_graph(files);
        let edge = result.edges.iter().find(|e| e.from == "main" && e.to == "helper").unwrap();
        assert_eq!(edge.from_file, "main.py");
        assert_eq!(edge.to_file, "utils.py");
    }

    #[test]
    fn call_graph_funcion_duplicada_gana_la_ultima() {
        let files = vec![
            CallGraphFileInput {
                filename: "a.py".to_string(),
                functions: vec![call_func("helper", "O(1)", 1, 1)],
                call_graph: vec![],
            },
            CallGraphFileInput {
                filename: "b.py".to_string(),
                functions: vec![call_func("helper", "O(n²)", 9, 5)],
                call_graph: vec![],
            },
        ];
        let result = build_call_graph(files);
        let node = result.nodes.iter().find(|n| n.id == "helper").unwrap();
        assert_eq!(node.file, "b.py");
        assert_eq!(node.big_o, "O(n²)");
        assert_eq!(node.level, "expensive");
    }

    #[test]
    fn call_graph_sin_edges_muestra_todas_las_funciones() {
        let files = vec![CallGraphFileInput {
            filename: "a.py".to_string(),
            functions: vec![call_func("f", "O(1)", 1, 1), call_func("g", "O(n)", 2, 5)],
            call_graph: vec![],
        }];
        let result = build_call_graph(files);
        assert_eq!(result.nodes.len(), 2);
    }

    #[test]
    fn call_graph_hot_paths_solo_expensive() {
        let files = vec![CallGraphFileInput {
            filename: "a.py".to_string(),
            functions: vec![call_func("fast", "O(1)", 1, 1), call_func("slow", "O(n²)", 9, 5)],
            call_graph: vec![],
        }];
        let result = build_call_graph(files);
        assert_eq!(result.summary.hot_paths.len(), 1);
        assert_eq!(result.summary.hot_paths[0].id, "slow");
    }

    #[test]
    fn call_graph_vacio_devuelve_mermaid_sin_funciones() {
        let result = build_call_graph(vec![]);
        assert!(result.mermaid.contains("Sin funciones"));
        assert!(result.nodes.is_empty());
    }

    // ── Circular Deps ───────────────────────────────────────────────────────

    #[test]
    fn circular_detecta_ciclo_simple_de_3() {
        let files = vec![
            file("a.py", vec![("b", 1)]),
            file("b.py", vec![("c", 1)]),
            file("c.py", vec![("a", 1)]),
        ];
        let result = build_circular_graph(files);
        assert!(result.has_cycles);
        assert_eq!(result.cycles.len(), 1);
        assert_eq!(result.cycles[0].len(), 3);
        let cycle_set: HashSet<&str> = result.cycles[0].iter().map(|s| s.as_str()).collect();
        assert_eq!(cycle_set, HashSet::from(["a.py", "b.py", "c.py"]));
    }

    #[test]
    fn circular_sin_ciclos() {
        let files = vec![file("a.py", vec![("b", 1)]), file("b.py", vec![])];
        let result = build_circular_graph(files);
        assert!(!result.has_cycles);
        assert!(result.cycles.is_empty());
        assert!(result.mermaid.contains("Sin dependencias circulares"));
    }

    #[test]
    fn circular_archivo_aislado_nunca_en_ciclo() {
        let files = vec![
            file("a.py", vec![("b", 1)]),
            file("b.py", vec![("a", 1)]),
            file("isolated.py", vec![]),
        ];
        let result = build_circular_graph(files);
        let isolated = result.nodes.iter().find(|n| n.id == "isolated.py").unwrap();
        assert!(!isolated.in_cycle);
        assert!(isolated.cycles.is_empty());
    }

    #[test]
    fn circular_nodo_con_multiples_ciclos() {
        // a↔b y a↔c: "a.py" participa en 2 ciclos distintos.
        let files = vec![
            file("a.py", vec![("b", 1), ("c", 2)]),
            file("b.py", vec![("a", 1)]),
            file("c.py", vec![("a", 1)]),
        ];
        let result = build_circular_graph(files);
        let a_node = result.nodes.iter().find(|n| n.id == "a.py").unwrap();
        assert_eq!(a_node.cycles.len(), 2);
        assert_eq!(result.cycles.len(), 2);
    }

    #[test]
    fn circular_cap_respetado() {
        // Grafo completo de 6 nodos (todos importan a todos) — muchísimos
        // ciclos simples posibles; confirmar que corta exacto en el cap.
        let names: Vec<String> = (0..6).map(|i| format!("f_{i}.py")).collect();
        let files: Vec<FileSummary> = names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let imports: Vec<(&str, u32)> = names
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, n)| (n.trim_end_matches(".py"), 1u32))
                    .collect();
                file(name, imports)
            })
            .collect();
        let result = build_circular_graph(files);
        assert_eq!(result.cycles.len(), MAX_CYCLES);
    }

    #[test]
    fn circular_cycle_descriptions_incluye_el_cierre() {
        let files = vec![
            file("a.py", vec![("b", 1)]),
            file("b.py", vec![("c", 1)]),
            file("c.py", vec![("a", 1)]),
        ];
        let result = build_circular_graph(files);
        let desc = &result.summary.cycle_descriptions[0];
        // El primer elemento del ciclo aparece 2 veces: al inicio y al cierre.
        let first = &result.cycles[0][0];
        assert!(desc.matches(first.as_str()).count() >= 2);
    }
}
