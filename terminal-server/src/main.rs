mod auth;
mod pty_session;

use std::io::{Read, Write};
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::connect_info::ConnectInfo;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;

use pty_session::PtySession;

#[derive(Clone)]
struct AppState {
    token: Arc<String>,
}

#[derive(Deserialize)]
struct WsAuthQuery {
    token: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ClientMsg {
    Input { data: String },
    Resize { cols: u16, rows: u16 },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let token = auth::generate_token();
    println!("🔑 Terminal token: {token}");
    println!("   Pegalo en el panel de Terminal del navegador para conectarte.");

    let host = std::env::var("TERMINAL_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = std::env::var("TERMINAL_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(7681);

    let state = AppState {
        token: Arc::new(token),
    };

    // Registrado con el mismo prefijo "/terminal" que usa el proxy de Vite en
    // dev (server.proxy en vite.config.ts no reescribe el path, igual que el
    // resto de los proxies hacia el backend Python).
    let app = Router::new()
        .route("/terminal/ws", get(ws_handler))
        .route("/terminal/token", get(token_handler))
        .with_state(state);

    let addr: SocketAddr = format!("{host}:{port}").parse().expect("host/puerto inválido");
    tracing::info!("Terminal server escuchando en {addr}");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("no se pudo bindear {addr}: {e}"));
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .expect("servidor caído");
}

/// True si el pedido viene de esta misma máquina. Si pasó por el proxy de
/// Vite (`xfwd: true` en vite.config.ts), Vite AGREGA su propia IP real al
/// final de X-Forwarded-For (ver node-http-proxy: `existing + ',' + real`) —
/// por eso hay que leer el ÚLTIMO valor, no el primero: el primero es lo que
/// el cliente mandó (falsificable — un atacante remoto podría poner
/// "X-Forwarded-For: 127.0.0.1" él mismo para intentar pasar por local). Sin
/// ese header (conexión directa, sin proxy), se usa la IP del peer.
fn is_loopback_request(headers: &HeaderMap, peer: SocketAddr) -> bool {
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        return xff
            .rsplit(',')
            .next()
            .and_then(|ip| ip.trim().parse::<std::net::IpAddr>().ok())
            .is_some_and(|ip| ip.is_loopback());
    }
    peer.ip().is_loopback()
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsAuthQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let provided = query.token.unwrap_or_default();
    if !auth::tokens_match(&state.token, &provided) {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    }
    ws.on_upgrade(handle_socket)
}

/// Conveniencia para uso 100% local: si el pedido viene de esta misma
/// máquina, sirve el token sin que el usuario tenga que copiarlo a mano. Si
/// alguna vez se expone TERMINAL_HOST más allá de localhost, esto se corta
/// solo para pedidos remotos — siguen necesitando el token a mano.
async fn token_handler(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if !is_loopback_request(&headers, peer) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    axum::Json(json!({ "token": *state.token })).into_response()
}

async fn handle_socket(socket: WebSocket) {
    let (mut session, mut reader, mut writer) = match PtySession::spawn() {
        Ok(v) => v,
        Err(e) => {
            let (mut sender, _) = socket.split();
            let msg = json!({"type": "output", "data": format!("No se pudo iniciar la shell: {e}\r\n")});
            let _ = sender.send(Message::Text(msg.to_string())).await;
            let _ = sender.close().await;
            return;
        }
    };

    let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(64);

    // portable-pty expone una API de lectura bloqueante — se corre en su propio
    // hilo (spawn_blocking) para no trabar el runtime async de tokio.
    let reader_task = tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if out_tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let (mut ws_sender, mut ws_receiver) = socket.split();

    let send_task = tokio::spawn(async move {
        while let Some(chunk) = out_rx.recv().await {
            let text = String::from_utf8_lossy(&chunk).into_owned();
            let msg = json!({"type": "output", "data": text}).to_string();
            if ws_sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(msg)) = ws_receiver.next().await {
        let Message::Text(text) = msg else { continue };
        match serde_json::from_str::<ClientMsg>(&text) {
            Ok(ClientMsg::Input { data }) => {
                let _ = writer.write_all(data.as_bytes());
                let _ = writer.flush();
            }
            Ok(ClientMsg::Resize { cols, rows }) => session.resize(cols, rows),
            Err(_) => {}
        }
    }

    // El cliente cerró (o la conexión se cayó): matar la shell y no dejar
    // procesos huérfanos, y frenar las tareas de lectura/escritura del PTY.
    session.kill();
    reader_task.abort();
    send_task.abort();
}
