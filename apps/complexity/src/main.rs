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
        .route("/metrics/complexity", post(metrics_complexity));

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
