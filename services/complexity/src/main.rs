use axum::extract::Query;
use axum::routing::{get, post};
use axum::{Json, Router};
use complexity_core::logstore::{self, LogEntry};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

/// Serializa `result` a JSON — comparte la lógica de "convertir a `Value` y
/// devolver un shape de error si falla" entre los ~14 endpoints de este
/// archivo, y loguea el error real antes de degradar. Antes cada endpoint
/// tenía su propio `unwrap_or_else(|_| json!({"error": "..."}))` que se
/// tragaba el error en silencio — un bug real de serialización (ej. un
/// `NaN`/`Infinity` colándose en algún campo `f64`) se veía indistinguible
/// de cualquier otra respuesta 200 exitosa, sin ninguna pista de qué pasó.
fn to_json<T: Serialize>(result: T) -> Json<Value> {
    match serde_json::to_value(result) {
        Ok(v) => Json(v),
        Err(e) => {
            tracing::error!("serialización de respuesta falló: {e}");
            Json(json!({"error": "serialización falló"}))
        }
    }
}

#[derive(Deserialize)]
struct ComplexityRequest {
    #[allow(dead_code)]
    filename: String,
    content: String,
}

#[derive(Deserialize)]
struct SymbolRequest {
    content: String,
    symbol: String,
}

#[derive(Deserialize)]
struct LogPost {
    level: String,
    msg: String,
    #[serde(default = "default_log_source")]
    source: String,
}

fn default_log_source() -> String {
    "api".to_string()
}

#[derive(Deserialize)]
struct LogQuery {
    #[serde(default = "default_log_limit")]
    limit: usize,
}

fn default_log_limit() -> usize {
    100
}

fn log_dir() -> PathBuf {
    PathBuf::from(std::env::var("SYTHRALL_LOG_DIR").unwrap_or_else(|_| "logs".to_string()))
}

/// Fuentes conocidas — un archivo por origen, cada uno escrito por un solo
/// proceso (este sidecar para "api"/"complexity", `terminal-server` para
/// "terminal"), así que no hay que coordinar escrituras concurrentes al
/// mismo archivo entre los 2 binarios Rust separados.
const LOG_SOURCES: &[&str] = &["api", "complexity", "terminal"];

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let host = std::env::var("COMPLEXITY_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = std::env::var("COMPLEXITY_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(7682);

    // Sin token: es una función pura de cómputo (parseo + conteo), sin acceso
    // al sistema de archivos ni ejecución de comandos — el riesgo que
    // justifica el token en terminal-server (una shell real) no aplica acá.
    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics/complexity", post(metrics_complexity))
        .route("/parse/python", post(parse_python))
        .route("/graph/import", post(graph_import))
        .route("/graph/centrality", post(graph_centrality))
        .route("/graph/call", post(graph_call))
        .route("/graph/circular", post(graph_circular))
        .route("/graph/architecture", post(graph_architecture))
        .route("/graph/heatmap", post(graph_heatmap))
        .route("/ml/detect", post(ml_detect))
        .route("/log", post(log_append).get(log_read))
        .route("/parse/c", post(parse_c))
        .route("/parse/cpp", post(parse_cpp))
        .route("/parse/js", post(parse_js))
        .route("/parse/ts", post(parse_ts))
        .route("/parse/fortran", post(parse_fortran))
        .route("/parse/asm", post(parse_asm))
        .route("/execution/validate-matmul", post(validate_matmul))
        .route("/execution/validate-bubble-sort", post(validate_bubble_sort))
        .route("/execution/validate-sum-squares", post(validate_sum_squares))
        .route("/execution/validate-graph-bfs", post(validate_graph_bfs))
        .route("/execution/validate-fibonacci", post(validate_fibonacci))
        .route("/execution/validate-mergesort", post(validate_mergesort))
        .route("/plugins/manifests", get(plugin_manifests))
        .route("/scan/project", post(scan_project))
        .route("/symbols/definition/python", post(symbols_definition_python))
        .route("/symbols/definition/js", post(symbols_definition_js))
        .route("/symbols/definition/ts", post(symbols_definition_ts))
        .route("/symbols/references/python", post(symbols_references_python))
        .route("/symbols/references/js", post(symbols_references_js))
        .route("/symbols/references/ts", post(symbols_references_ts));

    let addr: std::net::SocketAddr = format!("{host}:{port}").parse().expect("host/puerto inválido");
    println!("🦀 Complexity engine escuchando en http://{addr}");
    if let Err(e) = logstore::append(
        &log_dir().join("complexity.cbor"),
        &LogEntry { ts: logstore::now_string(), level: "info".into(), msg: format!("Complexity engine escuchando en http://{addr}"), source: "complexity".into() },
    ) {
        // Best-effort — no debe tumbar el arranque si SYTHRALL_LOG_DIR no es
        // escribible, pero silenciarlo del todo socavaría la observabilidad
        // que este log existe para dar en primer lugar.
        tracing::warn!("no se pudo escribir el log de arranque: {e}");
    }

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("no se pudo bindear {addr}: {e}"));
    axum::serve(listener, app).await.expect("servidor caído");
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true }))
}

async fn metrics_complexity(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    let result = complexity_core::analyze(&req.content);
    to_json(result)
}

/// Fase 1 de la migración de `static_parser.py` a Rust — functions/classes/
/// imports/dead_code/call_graph/wasm_hints/summary para un archivo Python,
/// mismo shape que `_parse_python()` (`circular_deps` existía acá también,
/// pero se eliminó del todo: código muerto, sin consumidores).
async fn parse_python(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    let result = complexity_core::analyze_rich(&req.content);
    to_json(result)
}

/// Fase 18 — primer slice del Graph Engine: Import Graph. Recibe el resumen
/// ya parseado de cada archivo del proyecto (no archivos crudos — el parsing
/// multi-lenguaje sigue en Python) y devuelve nodes/edges/mermaid/entry_points
/// en el mismo shape que `_build_import_graph` en `graph.py`.
async fn graph_import(Json(req): Json<Vec<complexity_core::graph::FileSummary>>) -> Json<Value> {
    let result = complexity_core::build_import_graph(req);
    to_json(result)
}

/// Fase 18 — segunda porción del Graph Engine: Centrality. Mismo contrato
/// que `graph_import` (resumen ya parseado por archivo, no archivos crudos).
async fn graph_centrality(Json(req): Json<Vec<complexity_core::graph::FileSummary>>) -> Json<Value> {
    let result = complexity_core::build_centrality_graph(req);
    to_json(result)
}

/// Fase 18 — tercera porción del Graph Engine: Call Graph. A diferencia de
/// `graph_import`/`graph_centrality`, el payload no es `FileSummary` — Call
/// Graph necesita detalle por función y el `call_graph` ya calculado por
/// archivo, no imports/dead_code.
async fn graph_call(Json(req): Json<Vec<complexity_core::graph::CallGraphFileInput>>) -> Json<Value> {
    let result = complexity_core::build_call_graph(req);
    to_json(result)
}

/// Fase 18 — cuarta y última porción del Graph Engine: Circular Deps.
async fn graph_circular(Json(req): Json<Vec<complexity_core::graph::FileSummary>>) -> Json<Value> {
    let result = complexity_core::build_circular_graph(req);
    to_json(result)
}

/// Fase 18 — "Dependency Engine": Architecture Smells, el último ítem del
/// Graph Engine que todavía era Python-only (`_build_architecture_smells`
/// en `graph.py`). Mismo payload `FileSummary` que Import/Centrality/
/// Circular — pura orquestación sobre lo que esas 3 porciones ya
/// construyeron, ver el comentario de sección de `graph.rs`.
async fn graph_architecture(Json(req): Json<Vec<complexity_core::graph::FileSummary>>) -> Json<Value> {
    let result = complexity_core::build_architecture_smells(req);
    to_json(result)
}

/// Complexity Heatmap — la última pieza del Graph Engine que quedaba
/// Python-only (todos los comentarios de Import/Call/Circular/Architecture
/// Smells la nombraban explícitamente como "todavía Python, fuera de este
/// slice"). Payload propio (`HeatmapFileInput`, no `FileSummary`): necesita
/// detalle por función (complexity/big_o/line/loc), no solo el resumen
/// agregado que el resto de los tipos de grafo usa.
async fn graph_heatmap(Json(req): Json<Vec<complexity_core::graph::HeatmapFileInput>>) -> Json<Value> {
    let result = complexity_core::build_heatmap(req);
    to_json(result)
}

#[derive(Deserialize)]
struct MlDetectRequest {
    content: String,
}

/// Reducción de Python — primera mitad de `apps/api/routers/ml.py`
/// (`ml.rs`, ver su doc de módulo para qué queda deliberadamente afuera).
/// Sin `filename`: nada en la detección lo necesita.
async fn ml_detect(Json(req): Json<MlDetectRequest>) -> Json<Value> {
    match complexity_core::ml::detect(&req.content) {
        Some(result) => to_json(result),
        None => Json(json!({"error": "parseo falló"})),
    }
}

/// Log unificado (Python backend + los 2 sidecars Rust), persistido en CBOR
/// — ver `logstore.rs`. `POST /log` es el único punto de entrada externo (lo
/// llama `apps/api/services/log_client.py`); Python nunca toca bytes CBOR
/// directamente, solo manda/recibe JSON plano por este mismo canal HTTP que
/// ya existe para todo lo demás.
async fn log_append(Json(req): Json<LogPost>) -> Json<Value> {
    let entry = LogEntry { ts: logstore::now_string(), level: req.level, msg: req.msg, source: req.source.clone() };
    let path = log_dir().join(format!("{}.cbor", req.source));
    match logstore::append(&path, &entry) {
        Ok(()) => Json(json!({ "ok": true })),
        Err(e) => Json(json!({ "ok": false, "error": e.to_string() })),
    }
}

/// Lee y mezcla los 3 archivos de log conocidos (mismo disco, aunque cada
/// uno lo escriba un proceso distinto), ordena por timestamp y devuelve los
/// últimos `limit` — la decodificación CBOR→texto real pasa acá, nunca
/// antes. Mismo shape de respuesta que ya devuelve `GET /logs` en el
/// backend Python hoy (`{"logs": [...], "total": N}`), para que ese router
/// pueda usar esto como fuente primaria sin que el frontend note nada.
/// Fase 18 — parsers C/C++/JS/TS portados a Rust (tree-sitter para C/C++,
/// mismo regex+heurística que el Python que reemplazan para JS/TS). Cada
/// uno recibe error interno (`error: "..."`, ver `_unsupported()` en
/// `static_parser.py`) — `Option::None` del parser Rust (grammar/parse
/// failure) se traduce a ese shape en vez de un 500, mismo criterio que el
/// resto de los endpoints de este archivo.
async fn parse_c(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    match complexity_core::cparse::parse_c(&req.content) {
        Some(result) => to_json(result),
        None => Json(json!({"error": "parseo tree-sitter falló"})),
    }
}

async fn parse_cpp(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    match complexity_core::cparse::parse_cpp(&req.content) {
        Some(result) => to_json(result),
        None => Json(json!({"error": "parseo tree-sitter falló"})),
    }
}

async fn parse_js(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    let result = complexity_core::jsts::parse_js_ts(&req.content, false);
    to_json(result)
}

async fn parse_ts(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    let result = complexity_core::jsts::parse_js_ts(&req.content, true);
    to_json(result)
}

/// Fase 20 (Scientific Intelligence) — primer lenguaje que nace directo en
/// Rust sin parser Python previo que reemplazar (a diferencia de C/C++/JS/
/// TS, todos puertos de `static_parser.py`). Mismo criterio de error que
/// `parse_c`/`parse_cpp`: `None` (falla de gramática tree-sitter) se traduce
/// a un shape de error, no a un 500.
async fn parse_fortran(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    match complexity_core::fparse::parse_fortran(&req.content) {
        Some(result) => to_json(result),
        None => Json(json!({"error": "parseo tree-sitter falló"})),
    }
}

/// Fase 19 (Machine Intelligence) — pattern-matching sobre texto plano, no
/// tree-sitter: `asmparse::parse` nunca devuelve `None` (a diferencia de los
/// demás `/parse/*`), así que no hay branch de error acá.
async fn parse_asm(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    to_json(complexity_core::asmparse::parse(&req.content))
}

/// Fase 23 (Execution Intelligence) — primer endpoint de este binario que
/// ejecuta código (todo lo demás es análisis estático puro). Compila y corre
/// un kernel Fortran escrito por Sythrall mismo (`fortran_bench.rs`, nunca
/// código del usuario) para validar empíricamente el O(n³) que
/// `numerical_algorithm_note` predice por forma en Fase 20. Sin body — el
/// kernel y los tamaños de `n` son fijos en esta primera versión.
async fn validate_matmul() -> Json<Value> {
    to_json(complexity_core::fortran_bench::validate_matmul_cubic())
}

/// Fase 26 (Algorithm Validation Engine) — generaliza `validate_matmul`
/// arriba más allá de Fortran/matmul: compila y corre un bubble sort escrito
/// a mano en Zig (`zig_bench.rs`) para validar empíricamente O(n²). Sin
/// body — mismo criterio que `validate_matmul`.
async fn validate_bubble_sort() -> Json<Value> {
    to_json(complexity_core::zig_bench::validate_bubble_sort_quadratic())
}

/// Fase 26 (Algorithm Validation Engine) — tercer kernel de validación,
/// esta vez en Assembly x86 real (`asm_bench.rs`) para validar
/// empíricamente O(n) de una suma de cuadrados. Sin body.
async fn validate_sum_squares() -> Json<Value> {
    to_json(complexity_core::asm_bench::validate_sum_squares_linear())
}

/// Fase 26 (Algorithm Validation Engine) — cuarto kernel de validación,
/// segunda vez en Zig pero una forma algorítmica distinta a
/// `validate_bubble_sort`: recorrido de grafos (BFS, `bfs_bench.rs`) para
/// validar empíricamente O(V+E) sobre un grafo disperso de grado fijo. Sin
/// body.
async fn validate_graph_bfs() -> Json<Value> {
    to_json(complexity_core::bfs_bench::validate_graph_bfs_linear())
}

/// Fase 26 (Algorithm Validation Engine) — quinto kernel, y el primero en
/// validar una forma NO polinomial: profundidad de recursión (Fibonacci
/// recursivo ingenuo, `fib_bench.rs`) para confirmar empíricamente que
/// crece exponencialmente (Θ(φⁿ)), con la base medida comparada contra la
/// razón áurea teórica. Sin body.
async fn validate_fibonacci() -> Json<Value> {
    to_json(complexity_core::fib_bench::validate_naive_fibonacci_exponential())
}

/// Fase 26 (Algorithm Validation Engine) — sexto kernel, y el primero en
/// validar O(n log n): mergesort bottom-up iterativo en Assembly x86
/// (`mergesort_bench.rs`) para confirmar empíricamente esa forma. Sin
/// body.
async fn validate_mergesort() -> Json<Value> {
    to_json(complexity_core::mergesort_bench::validate_mergesort_nlogn())
}

/// Fase 24 (Extensibility Platform) — el manifest de los 7 plugins built-in
/// de Sythrall (`plugin.rs`), fuente de verdad que reemplaza el dict
/// hardcodeado que antes vivía en `routers/static_analysis.py::/languages`.
async fn plugin_manifests() -> Json<Value> {
    to_json(complexity_core::plugin::builtin_manifests())
}

/// Fase 18 — Symbol Engine: go-to-definition/find-references, portado 1:1
/// desde Python (`routers/intelligence.py`, ver `symbols.rs` para el
/// detalle completo). Por archivo, mismo alcance que antes — no a nivel de
/// proyecto entero, eso queda para un ítem futuro separado.
async fn symbols_definition_python(Json(req): Json<SymbolRequest>) -> Json<Value> {
    let defs = complexity_core::symbols::find_definitions_python(&req.content, &req.symbol);
    to_json(defs)
}

async fn symbols_definition_js(Json(req): Json<SymbolRequest>) -> Json<Value> {
    let defs = complexity_core::symbols::find_definitions_jsts(&req.content, false, &req.symbol);
    to_json(defs)
}

async fn symbols_definition_ts(Json(req): Json<SymbolRequest>) -> Json<Value> {
    let defs = complexity_core::symbols::find_definitions_jsts(&req.content, true, &req.symbol);
    to_json(defs)
}

async fn symbols_references_python(Json(req): Json<SymbolRequest>) -> Json<Value> {
    let (refs, definition_line) = complexity_core::symbols::find_references_python(&req.content, &req.symbol);
    Json(json!({ "references": refs, "definition_line": definition_line }))
}

async fn symbols_references_js(Json(req): Json<SymbolRequest>) -> Json<Value> {
    let (refs, definition_line) = complexity_core::symbols::find_references_jsts(&req.content, false, &req.symbol);
    Json(json!({ "references": refs, "definition_line": definition_line }))
}

async fn symbols_references_ts(Json(req): Json<SymbolRequest>) -> Json<Value> {
    let (refs, definition_line) = complexity_core::symbols::find_references_jsts(&req.content, true, &req.symbol);
    Json(json!({ "references": refs, "definition_line": definition_line }))
}

/// Fase 18 — Project Scanner: única vez que este sidecar lee del disco
/// directamente (ver `scanner.rs` para la justificación completa). Recibe
/// un directorio ya validado/resuelto por Python (nunca uno crudo del
/// cliente) más las mismas listas de extensiones/directorios ignorados que
/// `project_service.py` ya usa — Python sigue siendo la única fuente de
/// verdad de esas listas, acá solo se reciben como parámetros para no
/// duplicarlas ni arriesgar que diverjan entre las dos implementaciones.
async fn scan_project(Json(req): Json<complexity_core::scanner::ScanRequest>) -> Json<Value> {
    let files = complexity_core::scanner::scan_and_parse_project(&req);
    Json(json!({ "files": files }))
}

async fn log_read(Query(q): Query<LogQuery>) -> Json<Value> {
    let dir = log_dir();
    let mut entries: Vec<LogEntry> = LOG_SOURCES.iter().flat_map(|src| logstore::read_all(&dir.join(format!("{src}.cbor")))).collect();
    entries.sort_by(|a, b| a.ts.cmp(&b.ts));
    let total = entries.len();
    let tail: Vec<LogEntry> = entries.split_off(total.saturating_sub(q.limit));
    Json(json!({ "logs": tail, "total": total }))
}
