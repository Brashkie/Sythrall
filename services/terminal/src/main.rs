mod auth;
mod pty_session;

use std::io::{Read, Write};
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
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
    /// Secreto compartido con `apps/api` (único emisor de tokens hoy) para
    /// verificar la firma HS256 de los JWT de sesión — ver `auth.rs`.
    secret: Arc<Vec<u8>>,
}

#[derive(Deserialize)]
struct WsAuthQuery {
    token: Option<String>,
    shell: Option<String>,
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

    // El secreto lo genera/persiste `scripts/lib/authSecret.mjs` (llamado
    // por `dev-banner.mjs`, antes de que `npm run dev` levante nada) y lo
    // inyecta `run-terminal.mjs` en el entorno de este proceso — acá no se
    // lee `.env` ni se genera nada: si falta, se falla rápido con un
    // mensaje claro en vez de arrancar sin poder verificar nada.
    let secret = std::env::var("SYTHRALL_AUTH_SECRET").unwrap_or_else(|_| {
        panic!(
            "SYTHRALL_AUTH_SECRET no está seteada. Corré `npm run dev` (scripts/dev-banner.mjs \
             la genera sola) o exportala a mano antes de levantar este binario."
        )
    });
    println!("Terminal lista — esperando conexiones autenticadas por token de sesión (JWT).");

    let host = std::env::var("TERMINAL_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = std::env::var("TERMINAL_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(7681);

    let state = AppState {
        secret: Arc::new(secret.into_bytes()),
    };

    // Registrado con el mismo prefijo "/terminal" que usa el proxy de Vite en
    // dev (server.proxy en vite.config.ts no reescribe el path, igual que el
    // resto de los proxies hacia el backend Python).
    let app = Router::new()
        .route("/terminal/ws", get(ws_handler))
        .route("/terminal/shells", get(shells_handler))
        .with_state(state);

    let addr: SocketAddr = format!("{host}:{port}").parse().expect("host/puerto inválido");
    tracing::info!("Terminal server escuchando en {addr}");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("no se pudo bindear {addr}: {e}"));
    axum::serve(listener, app.into_make_service())
        .await
        .expect("servidor caído");
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsAuthQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let provided = query.token.unwrap_or_default();
    let Some(claims) = auth::verify_terminal_token(&state.secret, &provided) else {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    };
    tracing::info!(sub = %claims.sub, "sesión de terminal autenticada");
    ws.on_upgrade(move |socket| handle_socket(socket, query.shell))
}

/// Sin auth a propósito — no expone nada sensible, solo qué binarios de
/// shell existen para el SO donde corre el sidecar (mismo criterio que
/// `available_shells()` documenta en `pty_session.rs`). El frontend puebla
/// el selector con esto en vez de una lista hardcodeada que podría no
/// calzar con la plataforma real del sidecar.
async fn shells_handler() -> impl IntoResponse {
    let shells: Vec<_> = pty_session::available_shells()
        .into_iter()
        .map(|(id, label)| json!({"id": id, "label": label}))
        .collect();
    axum::Json(json!({ "shells": shells }))
}

async fn handle_socket(socket: WebSocket, shell: Option<String>) {
    let (mut session, mut reader, mut writer) = match PtySession::spawn(shell.as_deref()) {
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
