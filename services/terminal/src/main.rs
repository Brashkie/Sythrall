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

use complexity_core::logstore::{self, LogEntry};
use pty_session::PtySession;

fn log_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("SYTHRALL_LOG_DIR").unwrap_or_else(|_| "logs".to_string()))
}

fn log_terminal(level: &str, msg: String) {
    if let Err(e) =
        logstore::append(&log_dir().join("terminal.cbor"), &LogEntry { ts: logstore::now_string(), level: level.into(), msg, source: "terminal".into() })
    {
        // Best-effort — nunca debe tumbar una sesión de terminal, pero
        // silenciarlo del todo socavaría la observabilidad que este log
        // existe para dar en primer lugar (mismo criterio que el startup
        // log de complexity-engine, ver main.rs de ese binario).
        tracing::warn!("no se pudo escribir al log de terminal: {e}");
    }
}

/// Decodifica lo más posible de `buf` como UTF-8 válido, dejando en `buf`
/// cualquier cola de bytes que sea el PRINCIPIO de un carácter multi-byte
/// todavía incompleto, para completarlo con el próximo chunk. Sin esto: el
/// reader del PTY lee en chunks fijos de 4096 bytes (ver `reader_task` más
/// abajo), y si un carácter multi-byte (glifos de `htop`/`vim`/`tmux`, un
/// nombre de archivo no-ASCII que devuelve `ls`, etc.) cae justo en ese
/// límite, cada mitad es UTF-8 inválido POR SU CUENTA — decodificar cada
/// chunk de forma aislada con `from_utf8_lossy` (como se hacía antes)
/// reemplazaba ambas mitades por U+FFFD, corrompiendo la salida en
/// silencio. Bytes genuinamente inválidos (no solo incompletos) sí se
/// reemplazan con el criterio usual de "lossy" — esto no relaja esa parte,
/// solo evita tratar como inválido lo que en realidad estaba cortado.
fn drain_valid_utf8(buf: &mut Vec<u8>) -> String {
    let mut out = String::new();
    loop {
        match std::str::from_utf8(buf) {
            Ok(s) => {
                out.push_str(s);
                buf.clear();
                return out;
            }
            Err(e) => {
                let valid_up_to = e.valid_up_to();
                // Bytes 0..valid_up_to son válidos por contrato de from_utf8.
                out.push_str(std::str::from_utf8(&buf[..valid_up_to]).expect("valid_up_to garantiza UTF-8 válido"));
                match e.error_len() {
                    // Secuencia incompleta al final del buffer — todavía
                    // podría completarse con el próximo chunk del PTY, se
                    // deja en `buf` en vez de reemplazarla ya mismo.
                    None => {
                        buf.drain(..valid_up_to);
                        return out;
                    }
                    // Bytes genuinamente inválidos (ningún dato adicional
                    // los va a arreglar) — U+FFFD y seguir decodificando lo
                    // que venga después, en el mismo buffer.
                    Some(bad_len) => {
                        out.push('\u{FFFD}');
                        buf.drain(..valid_up_to + bad_len);
                    }
                }
            }
        }
    }
}

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
    log_terminal("info", format!("Terminal server escuchando en {addr}"));

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
    log_terminal("info", format!("sesión de terminal autenticada: {}", claims.sub));
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
        let mut leftover: Vec<u8> = Vec::new();
        while let Some(chunk) = out_rx.recv().await {
            leftover.extend_from_slice(&chunk);
            let text = drain_valid_utf8(&mut leftover);
            if text.is_empty() {
                continue;
            }
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
                // Si la shell ya murió (ej. el usuario tipeó `exit`), el PTY
                // deja de aceptar escrituras — antes esto se descartaba en
                // silencio y la sesión quedaba colgada hasta que el reader
                // notara el EOF por su cuenta. Cortar acá mismo dispara la
                // misma limpieza (`session.kill()`/abort de las tareas) sin
                // esperar a esa segunda señal.
                if writer.write_all(data.as_bytes()).is_err() || writer.flush().is_err() {
                    tracing::warn!("escritura al PTY falló — la shell probablemente ya terminó, cerrando sesión");
                    break;
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texto_ascii_simple_pasa_entero() {
        let mut buf = b"hola mundo".to_vec();
        assert_eq!(drain_valid_utf8(&mut buf), "hola mundo");
        assert!(buf.is_empty());
    }

    #[test]
    fn caracter_multibyte_partido_justo_en_el_limite_del_chunk() {
        // Regresión del bug real: '→' (U+2192) es 3 bytes en UTF-8
        // (0xE2 0x86 0x92) — simula que el "chunk" del PTY corta justo
        // después del primer byte de la flecha.
        let prefix = b"antes"; // 5 bytes ASCII
        let arrow = "→".as_bytes(); // [0xE2, 0x86, 0x92]
        let suffix = b"despues"; // 7 bytes ASCII

        let mut leftover = Vec::new();

        // Primer chunk: "antes" + el primer byte de la flecha (incompleto).
        leftover.extend_from_slice(prefix);
        leftover.extend_from_slice(&arrow[..1]);
        let out1 = drain_valid_utf8(&mut leftover);
        assert_eq!(out1, "antes", "el prefijo completo sale ya; el byte incompleto se queda en el buffer");
        assert_eq!(leftover, vec![arrow[0]], "el primer byte de la flecha debe quedar guardado, no descartado");

        // Segundo chunk: completa la flecha + el sufijo.
        leftover.extend_from_slice(&arrow[1..]);
        leftover.extend_from_slice(suffix);
        let out2 = drain_valid_utf8(&mut leftover);
        assert_eq!(out2, "→despues");
        assert!(leftover.is_empty());

        assert_eq!(out1 + &out2, "antes→despues");
    }

    #[test]
    fn bytes_genuinamente_invalidos_se_reemplazan_y_se_sigue_decodificando() {
        let mut buf = vec![b'a', 0xFF, b'b'];
        let out = drain_valid_utf8(&mut buf);
        assert_eq!(out, "a\u{FFFD}b");
        assert!(buf.is_empty());
    }

    #[test]
    fn buffer_vacio_da_string_vacio() {
        let mut buf: Vec<u8> = Vec::new();
        assert_eq!(drain_valid_utf8(&mut buf), "");
    }

    #[test]
    fn secuencia_incompleta_al_final_no_se_pierde_nunca() {
        // '日' (U+65E5) es 3 bytes (0xE6 0x97 0xA5) — probar los 3 puntos de
        // corte posibles (después de 1 o 2 bytes) uno por uno.
        let ch = "日".as_bytes();
        for cut in 1..ch.len() {
            let mut leftover = Vec::new();
            leftover.extend_from_slice(&ch[..cut]);
            let out1 = drain_valid_utf8(&mut leftover);
            assert_eq!(out1, "", "corte en byte {cut}: no debería emitir nada todavía");
            leftover.extend_from_slice(&ch[cut..]);
            let out2 = drain_valid_utf8(&mut leftover);
            assert_eq!(out2, "日", "corte en byte {cut}: el carácter completo debe recuperarse");
        }
    }
}
