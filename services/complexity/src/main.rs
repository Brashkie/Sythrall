use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct ComplexityRequest {
    #[allow(dead_code)]
    filename: String,
    content: String,
}

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
        .route("/graph/circular", post(graph_circular));

    let addr: std::net::SocketAddr = format!("{host}:{port}").parse().expect("host/puerto inválido");
    println!("🦀 Complexity engine escuchando en http://{addr}");

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
    Json(serde_json::to_value(result).unwrap_or_else(|_| json!({"error": "serialización falló"})))
}

/// Fase 1 de la migración de `static_parser.py` a Rust — functions/classes/
/// imports/call_graph/summary para un archivo Python, mismo shape que
/// `_parse_python()`. Deliberadamente no incluye wasm_hints/dead_code — el
/// backend Python sigue calculando esas piezas (`circular_deps` existía acá
/// también, pero se eliminó del todo: código muerto, sin consumidores).
async fn parse_python(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    let result = complexity_core::analyze_rich(&req.content);
    Json(serde_json::to_value(result).unwrap_or_else(|_| json!({"error": "serialización falló"})))
}

/// Fase 18 — primer slice del Graph Engine: Import Graph. Recibe el resumen
/// ya parseado de cada archivo del proyecto (no archivos crudos — el parsing
/// multi-lenguaje sigue en Python) y devuelve nodes/edges/mermaid/entry_points
/// en el mismo shape que `_build_import_graph` en `graph.py`.
async fn graph_import(Json(req): Json<Vec<complexity_core::graph::FileSummary>>) -> Json<Value> {
    let result = complexity_core::build_import_graph(req);
    Json(serde_json::to_value(result).unwrap_or_else(|_| json!({"error": "serialización falló"})))
}

/// Fase 18 — segunda porción del Graph Engine: Centrality. Mismo contrato
/// que `graph_import` (resumen ya parseado por archivo, no archivos crudos).
async fn graph_centrality(Json(req): Json<Vec<complexity_core::graph::FileSummary>>) -> Json<Value> {
    let result = complexity_core::build_centrality_graph(req);
    Json(serde_json::to_value(result).unwrap_or_else(|_| json!({"error": "serialización falló"})))
}

/// Fase 18 — tercera porción del Graph Engine: Call Graph. A diferencia de
/// `graph_import`/`graph_centrality`, el payload no es `FileSummary` — Call
/// Graph necesita detalle por función y el `call_graph` ya calculado por
/// archivo, no imports/dead_code.
async fn graph_call(Json(req): Json<Vec<complexity_core::graph::CallGraphFileInput>>) -> Json<Value> {
    let result = complexity_core::build_call_graph(req);
    Json(serde_json::to_value(result).unwrap_or_else(|_| json!({"error": "serialización falló"})))
}

/// Fase 18 — cuarta y última porción del Graph Engine: Circular Deps.
async fn graph_circular(Json(req): Json<Vec<complexity_core::graph::FileSummary>>) -> Json<Value> {
    let result = complexity_core::build_circular_graph(req);
    Json(serde_json::to_value(result).unwrap_or_else(|_| json!({"error": "serialización falló"})))
}
