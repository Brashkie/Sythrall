use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Query};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use complexity_core::servicio::logstore::{self, LogEntry};
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

/// **El shape que manda el frontend para SPARK: solo `content`.**
///
/// El router de Python que esto reemplaza (Fase 35) le agregaba un
/// `filename` fijo antes de reenviar, asi que `ComplexityRequest` nunca vio
/// la peticion cruda. Al sacar esa capa aparecio que el cliente no manda
/// `filename` — y para verificar un fuente Ada no hace falta: el nombre del
/// archivo lo deriva `spark.rs` de la unidad, que es lo que GNAT exige.
#[derive(Deserialize)]
struct VerificacionRequest {
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
        // El `/health` del MOTOR (un `{"ok": true}` para los scripts de
        // arranque y el conftest) se mueve a `/engine/health`, y `/health`
        // pasa a ser el del producto, que es el que servia `main.py` y el que
        // el frontend pide.
        .route("/engine/health", get(health))
        .route("/health", get(sistema_salud))
        .route("/capabilities", get(sistema_capacidades))
        .route("/auth/terminal-token", get(sistema_token))
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
        // **Fase 35** — el cliente pide `/logs?limit=N`, que es como lo
        // exponia el router de Python mientras reenviaba aca. Mismo
        // handler: el shape ya era identico.
        .route("/logs", get(log_read))
        .route("/parse/c", post(parse_c))
        .route("/parse/cpp", post(parse_cpp))
        .route("/parse/js", post(parse_js))
        .route("/parse/ts", post(parse_ts))
        .route("/parse/fortran", post(parse_fortran))
        .route("/parse/ada", post(parse_ada))
        .route("/parse/d", post(parse_d))
        .route("/verify/spark", post(verify_spark))
        // El frontend lo pedia por este camino cuando el router de
        // Python lo reenviaba; se conserva para no cambiar el cliente
        // al mismo tiempo que se borra la capa de abajo.
        .route("/execution/spark-verify", post(verify_spark))
        .route("/parse/rust", post(parse_rust))
        .route("/parse/asm", post(parse_asm))
        // ── Fase 35: el prefijo `/static`, que vivia en FastAPI ──
        // ── Fase 35: los grafos, que vivian en `routers/graph.py` ──
        .route("/analyze/graph/types", get(grafo_tipos))
        .route("/analyze/graph", post(grafo_generar))
        .route("/analyze/graph/project", post(grafo_generar))
        // ── Fase 35: los diagramas, que vivian en `routers/diagram.py` ──
        .route("/analyze/diagram", post(diagrama_generar))
        // ── Fase 35: lo que queda de `routers/analysis.py` ──
        .route("/analyze/logs-analyze", post(logs_analizar))
        .route("/analyze/code", post(analisis_de_codigo))
        .route("/analyze/project", post(analisis_de_proyecto))
        .route("/analyze/api", post(apis_chequear))
        .route("/analyze/ml", post(ml_panel))
        // ── Fase 35: `/intel`, de `routers/intelligence.py` ──
        .route("/intel/lint", post(intel_lint))
        .route("/intel/analyze", post(intel_analizar))
        .route("/intel/hover", post(intel_hover))
        .route("/intel/definition", post(intel_definicion))
        .route("/intel/references", post(intel_referencias))
        .route("/intel/completions", post(intel_completado))
        .route("/intel/rename", post(intel_renombrar))
        // ── Fase 35: las metricas en vivo, de `routers/metrics_live.py` ──
        .route("/metrics/live", post(metricas_en_vivo))
        .route("/metrics/validate", post(metricas_validar))
        .route("/metrics/health", get(metricas_estado))
        // ── Fase 35: el historial, que vivia en `routers/history.py` ──
        //
        // **El orden ya no decide, pero el conflicto sigue existiendo.**
        // FastAPI resolvia por orden de declaracion, asi que `/overview`
        // declarado despues de `/:project_id` devolvia "los snapshots del
        // proyecto llamado 'overview'" — una lista vacia. El router de axum
        // prefiere el segmento literal sobre el parametro sin importar el
        // orden, asi que aca el bug no se puede cometer; se deja igual el
        // orden de Python para que la comparacion entre ambos sea directa.
        .route("/history/overview", get(historial_panorama))
        .route("/history/compare/:desde/:hasta", get(historial_comparar))
        .route("/history/snapshot/:snapshot_id", delete(historial_borrar))
        .route("/history/:project_id/snapshot", post(historial_crear_snapshot))
        .route("/history/:project_id", get(historial_de_proyecto))
        // ── Fase 35: las subidas, que vivian en `routers/upload.py` ──
        //
        // El tope de cuerpo es por ruta y no global: `/files` y `/folder`
        // pueden traer una carpeta entera, `/zip` tiene su propio tope de
        // 200 MB declarado en el handler, y los endpoints de lectura no
        // necesitan ninguno. Un tope global tendria que ser el mayor de
        // todos, o sea ninguno.
        .route(
            "/api/upload/files",
            post(upload_files).layer(DefaultBodyLimit::max(512 * 1024 * 1024)),
        )
        .route(
            "/api/upload/folder",
            post(upload_folder).layer(DefaultBodyLimit::max(512 * 1024 * 1024)),
        )
        .route(
            "/api/upload/zip",
            // Un poco arriba del tope real (200 MB) para que un ZIP pasado
            // llegue al handler y salga con un 413 que dice el limite, en vez
            // de un rechazo crudo del layer sin cuerpo.
            post(upload_zip).layer(DefaultBodyLimit::max(subidas::MAX_ZIP as usize + 1024 * 1024)),
        )
        .route("/api/upload/empty", post(upload_empty))
        .route("/api/upload/projects", get(upload_projects))
        .route("/api/upload/projects/:project_id/tree", get(upload_tree))
        .route(
            "/api/upload/projects/:project_id/file",
            get(upload_file_content).post(upload_create_file),
        )
        .route("/api/upload/projects/:project_id", delete(upload_delete_project))
        .route("/static/languages", get(static_languages))
        .route("/static/parse", post(static_parse))
        .route("/static/parse-project", post(static_parse_project))
        .route("/static/bigO", post(static_big_o))
        .route("/static/wasm", post(static_wasm))
        .route("/static/compile-asm", post(static_compile_asm))
        .route("/static/compile-pareto", post(static_compile_pareto))
        .route("/static/css-atomic", post(static_css_atomic))
        .route("/static/css-usage", post(static_css_usage))
        .route("/compile/asm", post(compilar_asm))
        .route("/compile/pareto", post(pareto_compilacion))
        .route("/parse/notebook", post(parse_notebook))
        .route("/parse/css", post(parse_css))
        .route("/analyze/css-atomic", post(css_atomico))
        .route("/health/scores", post(salud_proyecto))
        .route("/execution/metaprogramming-cost", post(comparar_metaprogramacion))
        .route("/parse/html", post(parse_html))
        .route("/parse/manifest", post(parse_manifest))
        .route("/analyze/css-usage", post(css_usage))
        .route(
            "/parse/executable",
            post(parse_executable).layer(DefaultBodyLimit::max(64 * 1024 * 1024)),
        )
        .route(
            "/binary/parse",
            // El tope del layer va un poco ARRIBA del tope real a proposito:
            // asi un archivo pasado de tamanio llega al handler y sale con la
            // forma degradada de siempre, en vez de un 413 crudo que el panel
            // no sabe mostrar.
            post(binary_parse).layer(DefaultBodyLimit::max(MAX_UPLOAD_BINARIO + 1024 * 1024)),
        )
        .route("/execution/validate-matmul", post(validate_matmul))
        .route("/execution/validate-bubble-sort", post(validate_bubble_sort))
        .route("/execution/validate-sum-squares", post(validate_sum_squares))
        .route("/execution/validate-graph-bfs", post(validate_graph_bfs))
        .route("/execution/validate-fibonacci", post(validate_fibonacci))
        .route("/execution/validate-mergesort", post(validate_mergesort))
        .route("/execution/validate-aggregation", post(validate_aggregation))
        .route("/execution/validate-linreg", post(validate_linreg))
        .route("/execution/validate-quicksort", post(validate_quicksort))
        .route("/execution/validate-binary-search", post(validate_binary_search))
        .route("/execution/validate-template-cost", post(validate_template_cost))
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
        &logstore::log_dir().join("complexity.cbor"),
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
    // `into_make_service_with_connect_info` y no `app` directo: el handler
    // del token necesita la IP del peer para el chequeo de loopback, y sin
    // esto `ConnectInfo` no está disponible.
    axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .await
        .expect("servidor caído");
}

/// Mismo tope que tenia el router de Python que esto reemplaza.
const MAX_UPLOAD_BINARIO: usize = 64 * 1024 * 1024;

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

/// Fase 27 (AI Intelligence) — Jupyter Notebook analysis: `req.content` es
/// el JSON crudo de un `.ipynb`, no código Python — `analyze_notebook`
/// extrae las celdas de código y corre el mismo `analyze_rich` de arriba
/// sobre el texto concatenado (ver `notebook.rs`).
async fn parse_notebook(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    let result = complexity_core::analyze_notebook(&req.content);
    to_json(result)
}

/// Sythrall CSS Engine (2026-09-02) — 9no plugin de lenguaje. A diferencia
/// del resto, no comparte ningún motor previo: `cssparse.rs` trae su propio
/// tokenizador escrito a mano, sin dependencia nueva.
/// **Atomic CSS (Fase 31)**: genera la hoja atómica y, sobre todo, mide qué
/// se gana y qué se paga. No reescribe nada — ver el doc de `cssatomico.rs`.
/// **Project Health: los cuatro scores.** Eran las últimas fórmulas de
/// autoridad que vivían en Python — ver el doc de `salud.rs`.
/// **Fase 34, Línea A — el segundo punto de datos.** Mide la misma curva de
/// costo de metaprogramación en C++ y en D y las compara: si el costo por
/// nivel coincidiera, sería del mecanismo; medido, difiere, así que es del
/// lenguaje. Ver el doc de `dcost.rs`.
async fn comparar_metaprogramacion() -> Json<Value> {
    to_json(complexity_core::costo::dcost::comparar_metaprogramacion())
}

async fn salud_proyecto(
    Json(req): Json<complexity_core::servicio::salud::EntradaSalud>,
) -> Json<Value> {
    to_json(complexity_core::servicio::salud::calcular(&req))
}

// ───────────────────────── Fase 35 — el prefijo `/static` ─────────────────
//
// Nueve endpoints que servia `apps/api/routers/static_analysis.py`. El campo
// `available` del shape significaba "ipudo hablar con el sidecar?"; aca el
// sidecar **es** el proceso, asi que es siempre `true`. Se deja igual en vez
// de sacarlo: el cliente lo lee, y romperlo no era parte del trato.

/// Los cinco tipos de grafo que el selector ofrece.
async fn grafo_tipos() -> Json<Value> {
    Json(complexity_core::servicio::grafo::tipos())
}

// ── Fase 35: el historial, que vivia en `routers/history.py` ──────────────
//
// El router de Python guardaba en SQLite; aca el almacenamiento es el mismo
// CBOR append-only que `logstore`, para no meterle al motor una dependencia
// de SQLite por unas decenas de registros de cuatro numeros. El porque largo
// esta en el doc de `servicio::historial`.

/// Panorama entre todos los proyectos con snapshots guardados.
///
/// Lee el historial y el directorio de subidas, los dos de disco, asi que va
/// a `spawn_blocking`.
async fn historial_panorama() -> (StatusCode, Json<Value>) {
    match tokio::task::spawn_blocking(|| {
        let nombres = complexity_core::servicio::proyecto::nombres_de_proyectos();
        complexity_core::servicio::historial::panorama(&nombres)
    })
    .await
    {
        Ok(v) => (StatusCode::OK, Json(v)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "detail": format!("el panorama no se pudo armar: {e}") })),
        ),
    }
}

/// Corre el analisis del proyecto y guarda su `health`/`summary`.
///
/// **Usa `analizar_proyecto`, la misma funcion que `/static/parse-project`**
/// — una sola fuente de verdad para el scoring, no una segunda formula aca.
/// En Python esto era un salto HTTP del backend al motor; ahora las dos
/// mitades viven en el mismo proceso y la llamada es directa.
async fn historial_crear_snapshot(
    axum::extract::Path(project_id): axum::extract::Path<String>,
) -> (StatusCode, Json<Value>) {
    let resultado = tokio::task::spawn_blocking(move || {
        let dir = complexity_core::servicio::proyecto::directorio_de_subidas().join(&project_id);
        if !dir.is_dir() {
            return Err((
                StatusCode::NOT_FOUND,
                format!("Proyecto '{project_id}' no encontrado"),
            ));
        }
        let req = complexity_core::servicio::proyecto::ProyectoRequest {
            project_id: Some(project_id.clone()),
            ..Default::default()
        };
        let analisis = complexity_core::servicio::proyecto::analizar_proyecto(&req);
        let (Some(health), Some(summary)) = (analisis.get("health"), analisis.get("summary"))
        else {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "El analisis no devolvio scores: no se puede guardar un snapshot sin ellos."
                    .to_string(),
            ));
        };
        complexity_core::servicio::historial::guardar(
            &project_id,
            health.clone(),
            summary.clone(),
            Some(complexity_core::servicio::plugin::capacidades_de_analisis()),
        )
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("el snapshot no se pudo guardar: {e}"),
            )
        })
    })
    .await;
    match resultado {
        Ok(Ok(s)) => (
            StatusCode::OK,
            Json(serde_json::to_value(s).unwrap_or_else(|e| json!({ "detail": e.to_string() }))),
        ),
        Ok(Err((code, detail))) => (code, Json(json!({ "detail": detail }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "detail": format!("el analisis no termino: {e}") })),
        ),
    }
}

async fn historial_de_proyecto(
    axum::extract::Path(project_id): axum::extract::Path<String>,
) -> Json<Value> {
    let snaps =
        tokio::task::spawn_blocking(move || complexity_core::servicio::historial::de_proyecto(&project_id))
            .await
            .unwrap_or_default();
    Json(serde_json::to_value(snaps).unwrap_or_else(|_| json!([])))
}

async fn historial_comparar(
    axum::extract::Path((desde, hasta)): axum::extract::Path<(u64, u64)>,
) -> (StatusCode, Json<Value>) {
    match tokio::task::spawn_blocking(move || {
        complexity_core::servicio::historial::comparar(desde, hasta)
    })
    .await
    {
        Ok(Some(v)) => (StatusCode::OK, Json(v)),
        Ok(None) => {
            (StatusCode::NOT_FOUND, Json(json!({ "detail": "Snapshot no encontrado" })))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "detail": format!("la comparacion no termino: {e}") })),
        ),
    }
}

async fn historial_borrar(
    axum::extract::Path(snapshot_id): axum::extract::Path<u64>,
) -> (StatusCode, Json<Value>) {
    match tokio::task::spawn_blocking(move || complexity_core::servicio::historial::borrar(snapshot_id))
        .await
    {
        Ok(Ok(true)) => (StatusCode::OK, Json(json!({ "deleted": snapshot_id }))),
        Ok(Ok(false)) => {
            (StatusCode::NOT_FOUND, Json(json!({ "detail": "Snapshot no encontrado" })))
        }
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "detail": format!("el snapshot no se pudo borrar: {e}") })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "detail": format!("el borrado no termino: {e}") })),
        ),
    }
}

/// **Un solo handler para `/analyze/graph` y `/analyze/graph/project`.** En
/// Python eran dos endpoints porque uno recibia archivos y el otro un
/// `project_id`; aca el request trae los dos campos y `resultados_de` decide
/// de donde leer, asi que la duplicacion desaparece.
///
/// Va a `spawn_blocking`: con `project_id` esto lee y parsea el proyecto
/// entero del disco.
async fn grafo_generar(
    Json(req): Json<complexity_core::servicio::grafo::GrafoRequest>,
) -> Json<Value> {
    match tokio::task::spawn_blocking(move || complexity_core::servicio::grafo::generar(&req)).await {
        Ok(v) => Json(v),
        Err(e) => Json(json!({ "error": format!("el grafo no se pudo generar: {e}"), "mermaid": "", "nodes": [], "edges": [] })),
    }
}

// ── Fase 35, paso 6: sistema y auth, lo ultimo de Python ──────────────────

/// `/health` del producto. **Ojo con el nombre**: el motor ya tenia un
/// `/health` propio que devuelve `{"ok": true}` y lo usan los scripts de
/// arranque y el conftest. Este es el del PRODUCTO, con las capacidades, y
/// vive en la misma ruta que servia `main.py`. El del motor pasa a
/// `/engine/health` para que los dos convivan sin que uno tape al otro.
async fn sistema_salud() -> Json<Value> {
    Json(
        tokio::task::spawn_blocking(complexity_core::servicio::sistema::salud)
            .await
            .unwrap_or_else(|e| json!({ "status": "error", "detail": e.to_string() })),
    )
}

async fn sistema_capacidades() -> Json<Value> {
    Json(
        tokio::task::spawn_blocking(complexity_core::servicio::sistema::capacidades)
            .await
            .unwrap_or_else(|e| json!({ "server": "Sythrall", "detail": e.to_string() })),
    )
}

/// Firma un token de sesion para la terminal, **solo si el pedido viene de
/// esta maquina**. Si alguna vez esto se expone mas alla de localhost, se
/// corta hasta que exista login real.
async fn sistema_token(
    headers: axum::http::HeaderMap,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
) -> (StatusCode, Json<Value>) {
    let xff = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    if !complexity_core::servicio::sistema::es_loopback(xff, Some(&peer.ip().to_string())) {
        return (StatusCode::FORBIDDEN, Json(json!({ "detail": "forbidden" })));
    }
    match complexity_core::servicio::sistema::token_de_terminal() {
        Ok(v) => (StatusCode::OK, Json(v)),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "detail": e }))),
    }
}

// ── Fase 35: la inteligencia del editor, de `routers/intelligence.py` ─────
//
// Siete endpoints bajo `/intel`. `flake8`/`pylint` no estan (misma decision
// que en `servicio::analisis`) y los fallbacks de definition/references se
// borraron: `ast::symbols` ya tenia las cuatro funciones.

/// El camino rapido: se dispara en cada debounce de 300 ms del editor.
async fn intel_lint(
    Json(req): Json<complexity_core::servicio::editor::ArchivoRequest>,
) -> Json<Value> {
    match tokio::task::spawn_blocking(move || {
        complexity_core::servicio::editor::lint_rapido(&req.filename, &req.content)
    })
    .await
    {
        Ok(v) => Json(v),
        Err(e) => Json(json!({ "markers": [], "ms": 0, "source": "fast", "error": e.to_string() })),
    }
}

/// El camino lento: se dispara cuando el usuario deja de escribir (~2 s).
async fn intel_analizar(
    Json(req): Json<complexity_core::servicio::editor::ArchivoRequest>,
) -> Json<Value> {
    match tokio::task::spawn_blocking(move || {
        complexity_core::servicio::editor::analisis_completo(&req.filename, &req.content)
    })
    .await
    {
        Ok(v) => Json(v),
        Err(e) => Json(json!({ "markers": [], "source": "heavy", "error": e.to_string() })),
    }
}

async fn intel_hover(
    Json(req): Json<complexity_core::servicio::editor::PosicionRequest>,
) -> Json<Value> {
    match tokio::task::spawn_blocking(move || complexity_core::servicio::editor::hover(&req)).await {
        Ok(v) => Json(v),
        Err(_) => Json(json!({ "markdown": "", "range": Value::Null })),
    }
}

async fn intel_definicion(
    Json(req): Json<complexity_core::servicio::editor::SimboloRequest>,
) -> Json<Value> {
    match tokio::task::spawn_blocking(move || complexity_core::servicio::editor::definicion(&req))
        .await
    {
        Ok(v) => Json(v),
        Err(_) => Json(json!({ "found": false, "symbol": "", "definitions": [] })),
    }
}

async fn intel_referencias(
    Json(req): Json<complexity_core::servicio::editor::SimboloRequest>,
) -> Json<Value> {
    match tokio::task::spawn_blocking(move || complexity_core::servicio::editor::referencias(&req))
        .await
    {
        Ok(v) => Json(v),
        Err(_) => Json(json!({ "symbol": "", "references": [], "definition_line": Value::Null, "total": 0 })),
    }
}

async fn intel_completado(
    Json(req): Json<complexity_core::servicio::editor::CompletadoRequest>,
) -> Json<Value> {
    match tokio::task::spawn_blocking(move || complexity_core::servicio::editor::completado(&req))
        .await
    {
        Ok(v) => Json(v),
        Err(_) => Json(json!({ "symbols": [], "total": 0 })),
    }
}

async fn intel_renombrar(
    Json(req): Json<complexity_core::servicio::editor::RenombrarRequest>,
) -> Json<Value> {
    match tokio::task::spawn_blocking(move || complexity_core::servicio::editor::renombrar(&req))
        .await
    {
        Ok(v) => Json(v),
        Err(e) => Json(json!({ "valid": false, "edits": [], "error": e.to_string() })),
    }
}

/// **El panel de ML, que vivia en `routers/ml.py`** (Fase 35). La deteccion
/// ya estaba en `servicio::ml`; lo que se porto es el diagrama Mermaid del
/// pipeline y el score con sugerencias.
async fn ml_panel(
    Json(req): Json<complexity_core::servicio::mlpanel::MlRequest>,
) -> Json<Value> {
    match tokio::task::spawn_blocking(move || complexity_core::servicio::mlpanel::analizar(&req))
        .await
    {
        Ok(v) => Json(v),
        Err(e) => Json(json!({ "issues": [], "error": format!("el analisis de ML no termino: {e}") })),
    }
}

/// **Chequeo de APIs externas, lo ultimo de `routers/analysis.py`** (Fase 35).
///
/// **NO va a `spawn_blocking`**: es I/O de red asincronico, no computo. Meter
/// esto en el threadpool ocuparia un hilo entero esperando un socket, que es
/// exactamente lo contrario de lo que hay que hacer — y el motivo por el que
/// el resto de los handlers si lo usan (ellos queman CPU).
///
/// El port valida certificados; el cliente de Python usaba `verify=False`.
async fn apis_chequear(
    Json(req): Json<complexity_core::servicio::apis::ApisRequest>,
) -> Json<Value> {
    Json(complexity_core::servicio::apis::chequear(&req).await)
}

// ── Fase 35: el panel de problemas, de `routers/analysis.py` ──────────────
//
// `flake8` y `pylint` NO estan: se borraron por decision del usuario, porque
// invocarlos por subprocess dejaba a "Sythrall sin Python" necesitando un
// Python instalado. Quedan las 7 reglas que `_run_ast` tenia, portadas
// enteras, mas la complejidad del motor.

async fn analisis_de_codigo(
    Json(req): Json<complexity_core::servicio::analisis::CodigoRequest>,
) -> Json<Value> {
    match tokio::task::spawn_blocking(move || {
        complexity_core::servicio::analisis::analizar_codigo(&req)
    })
    .await
    {
        Ok(v) => Json(v),
        Err(e) => Json(json!({ "issues": [], "error": format!("el analisis no termino: {e}") })),
    }
}

/// Con `project_id` lee el proyecto entero del disco, asi que es el pedido
/// mas caro de los dos: `spawn_blocking` no es opcional.
async fn analisis_de_proyecto(
    Json(req): Json<complexity_core::servicio::analisis::ProyectoRequest>,
) -> Json<Value> {
    match tokio::task::spawn_blocking(move || {
        complexity_core::servicio::analisis::analizar_proyecto(&req)
    })
    .await
    {
        Ok(v) => Json(v),
        Err(e) => Json(json!({ "files": {}, "error": format!("el analisis no termino: {e}") })),
    }
}

/// **Analisis de logs, que vivia en `routers/analysis.py::analyze_logs`**
/// (Fase 35). Clasificacion de texto por palabras clave: no parsea ningun
/// lenguaje, pero recorre archivos enteros, asi que va a `spawn_blocking`.
async fn logs_analizar(
    Json(req): Json<complexity_core::servicio::logs::LogsRequest>,
) -> Json<Value> {
    match tokio::task::spawn_blocking(move || complexity_core::servicio::logs::analizar(&req)).await
    {
        Ok(v) => Json(v),
        Err(e) => Json(json!({
            "errors": [], "warnings": [], "summary": {},
            "error": format!("el analisis de logs no termino: {e}"),
        })),
    }
}

// ── Fase 35: las metricas en vivo, que vivian en `routers/metrics_live.py` ──
//
// `/metrics/live` es el endpoint mas llamado de la app: lo dispara el editor
// en cada pausa de tecleo. El presupuesto declarado es 5 ms y el camino
// liviano de `servicio::metricas` mide 1,74 ms en release — ver el doc de
// `de_python` para por que NO usa `analyze_rich`.

async fn metricas_en_vivo(
    Json(req): Json<complexity_core::servicio::metricas::MetricasRequest>,
) -> Json<Value> {
    match tokio::task::spawn_blocking(move || complexity_core::servicio::metricas::en_vivo(&req))
        .await
    {
        Ok(v) => Json(v),
        Err(e) => Json(json!({ "parse_ok": false, "safe_mode": true, "parse_error": e.to_string() })),
    }
}

async fn metricas_validar(
    Json(req): Json<complexity_core::servicio::metricas::MetricasRequest>,
) -> Json<Value> {
    match tokio::task::spawn_blocking(move || complexity_core::servicio::metricas::validar(&req))
        .await
    {
        Ok(v) => Json(v),
        Err(e) => Json(json!({ "valid": false, "errors": [e.to_string()] })),
    }
}

async fn metricas_estado() -> Json<Value> {
    // Consulta el toolchain por `pylint`, que toca disco: va al threadpool.
    Json(
        tokio::task::spawn_blocking(complexity_core::servicio::metricas::estado)
            .await
            .unwrap_or_else(|e| json!({ "status": "error", "detail": e.to_string() })),
    )
}

/// **Los diagramas Mermaid, que vivian en `routers/diagram.py`** (Fase 35).
///
/// `analyze_rich` sobre un archivo grande no es gratis, asi que va a
/// `spawn_blocking` igual que el resto del parseo.
async fn diagrama_generar(
    Json(req): Json<complexity_core::servicio::diagrama::DiagramaRequest>,
) -> Json<Value> {
    match tokio::task::spawn_blocking(move || complexity_core::servicio::diagrama::generar(&req))
        .await
    {
        Ok(v) => Json(v),
        // El panel espera SIEMPRE un `mermaid`: si no llega uno, no dibuja
        // nada y no dice por que. Asi que el error viaja como un diagrama.
        Err(e) => Json(json!({
            "filename": "",
            "diagram_type": "",
            "mermaid": format!("flowchart TD\n    ERR[Error: {}]", e.to_string().chars().take(60).collect::<String>()),
            "ts": logstore::now_string(),
        })),
    }
}

// ── Fase 35: las subidas, que vivian en `routers/upload.py` ───────────────
//
// Nueve endpoints bajo `/api/upload`. Lo que cambio de forma, y a proposito:
// cada archivo se escribe a disco **a medida que llega** en un directorio de
// staging, en vez de tener el cuerpo entero del multipart en memoria. Ver el
// doc de `subidas::directorio_temporal_de_subida` para el porque (axum
// streamea el multipart y FastAPI no lo hacia).

use complexity_core::servicio::subidas;

/// De donde sale la ruta de destino de cada archivo de la subida — es la
/// unica diferencia real entre `/files` y `/folder`, que en Python eran dos
/// handlers de 50 lineas casi identicas.
#[derive(Clone, Copy, PartialEq)]
enum ModoDeSubida {
    /// Archivos sueltos: se queda solo con el nombre, sin ninguna ruta.
    Sueltos,
    /// Una carpeta: se conserva la ruta relativa que manda el browser.
    Carpeta,
}

struct SubidaRecibida {
    nombre_pedido: String,
    project_id_pedido: Option<String>,
    guardados: Vec<Value>,
    errores: Vec<Value>,
    ignorados: u64,
    /// El primer segmento del primer archivo, que el picker de carpeta del
    /// browser garantiza que es el nombre de la carpeta elegida.
    primera_carpeta: Option<String>,
    temporal: PathBuf,
}

/// Lee el multipart entero, escribiendo cada archivo al staging.
async fn recibir_subida(
    mut multipart: axum::extract::Multipart,
    modo: ModoDeSubida,
) -> Result<SubidaRecibida, String> {
    let temporal = subidas::directorio_temporal_de_subida();
    std::fs::create_dir_all(&temporal).map_err(|e| format!("no se pudo crear el staging: {e}"))?;

    let mut r = SubidaRecibida {
        nombre_pedido: String::new(),
        project_id_pedido: None,
        guardados: Vec::new(),
        errores: Vec::new(),
        ignorados: 0,
        primera_carpeta: None,
        temporal: temporal.clone(),
    };

    while let Some(campo) = multipart.next_field().await.map_err(|e| e.to_string())? {
        let nombre_campo = campo.name().unwrap_or("").to_string();
        let nombre_archivo = campo.file_name().map(str::to_string);

        // Un campo sin `filename` es un campo de texto del formulario.
        let Some(nombre_archivo) = nombre_archivo else {
            let valor = campo.text().await.unwrap_or_default();
            match nombre_campo.as_str() {
                "project_name" => r.nombre_pedido = valor,
                "project_id" if !valor.is_empty() => r.project_id_pedido = Some(valor),
                _ => {}
            }
            continue;
        };

        if r.primera_carpeta.is_none() {
            r.primera_carpeta =
                subidas::segmentos_seguros(&nombre_archivo).first().cloned();
        }

        let ext = subidas::extension_de(&nombre_archivo);
        if subidas::esta_bloqueada(&ext) {
            r.errores.push(json!({
                "file": nombre_archivo,
                "reason": format!("Extensión no permitida: {ext}"),
            }));
            continue;
        }

        // La ruta de destino, segun el modo.
        let partes: Vec<String> = match modo {
            ModoDeSubida::Sueltos => subidas::segmentos_seguros(&nombre_archivo)
                .last()
                .cloned()
                .map(|n| vec![n])
                .unwrap_or_default(),
            ModoDeSubida::Carpeta => {
                let mut p = subidas::segmentos_seguros(&nombre_archivo);
                // El picker de carpeta del browser antepone SIEMPRE el nombre
                // de la carpeta elegida a cada archivo. Ese segmento ya se usa
                // aparte como sugerencia de nombre del proyecto; dejarlo aca
                // anidaria todo un nivel de mas dentro de una carpeta con el
                // mismo nombre. El `> 1` es para no vaciar la lista cuando hay
                // un solo archivo sin ruta relativa.
                if p.len() > 1 {
                    p.remove(0);
                }
                p
            }
        };
        if partes.is_empty() {
            continue;
        }
        // Carpetas del sistema: mismo criterio que la extraccion de ZIP. Sin
        // esto, subir una carpeta de un proyecto JS normal escribia decenas de
        // miles de archivos de `node_modules` a disco.
        if partes[..partes.len() - 1].iter().any(|p| subidas::es_ignorado(p)) {
            r.ignorados += 1;
            continue;
        }

        let datos = match campo.bytes().await {
            Ok(b) => b,
            Err(e) => {
                r.errores.push(json!({ "file": nombre_archivo, "reason": e.to_string() }));
                continue;
            }
        };
        if datos.len() as u64 > subidas::MAX_ARCHIVO {
            r.errores
                .push(json!({ "file": nombre_archivo, "reason": "Archivo demasiado grande (máx 50 MB)" }));
            continue;
        }

        let destino = partes.iter().fold(temporal.clone(), |a, p| a.join(p));
        if let Some(padre) = destino.parent() {
            if let Err(e) = std::fs::create_dir_all(padre) {
                r.errores.push(json!({ "file": nombre_archivo, "reason": e.to_string() }));
                continue;
            }
        }
        match std::fs::write(&destino, &datos) {
            Ok(()) => r.guardados.push(json!({
                "name": nombre_archivo,
                "size": datos.len(),
                "path": partes.join("/"),
            })),
            Err(e) => r.errores.push(json!({ "file": nombre_archivo, "reason": e.to_string() })),
        }
    }

    Ok(r)
}

/// La cola comun de los cuatro endpoints que crean o modifican un proyecto:
/// arbol → info → nombre resuelto → guardar metadata → podar los viejos.
fn finalizar(dir: &std::path::Path, nombre_pedido: &str, fallback: &str) -> (Value, String, Value) {
    let arbol = subidas::arbol(dir);
    let mut info = subidas::info_de(dir);
    let nombre = subidas::resolver_nombre(dir, &mut info, nombre_pedido, fallback);
    subidas::guardar_metadata(dir, &info);
    subidas::podar(subidas::MAX_PROYECTOS);
    (arbol, nombre, info)
}

/// Resuelve el directorio destino: reusa el proyecto pedido (404 si no esta)
/// o crea uno nuevo.
fn resolver_o_crear(
    project_id: Option<String>,
) -> Result<(String, PathBuf), (StatusCode, String)> {
    match project_id {
        Some(id) => {
            let dir = subidas::ruta_de_proyecto(&id)
                .ok_or((StatusCode::BAD_REQUEST, "Path inválido.".to_string()))?;
            if !dir.is_dir() {
                return Err((StatusCode::NOT_FOUND, format!("Proyecto {id} no encontrado.")));
            }
            Ok((id, dir))
        }
        None => {
            let id = subidas::uuid_v4();
            let dir = subidas::directorio_de_subidas().join(&id);
            std::fs::create_dir_all(&dir)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Ok((id, dir))
        }
    }
}

async fn upload_files(multipart: axum::extract::Multipart) -> (StatusCode, Json<Value>) {
    subida_comun(multipart, ModoDeSubida::Sueltos).await
}

async fn upload_folder(multipart: axum::extract::Multipart) -> (StatusCode, Json<Value>) {
    subida_comun(multipart, ModoDeSubida::Carpeta).await
}

async fn subida_comun(
    multipart: axum::extract::Multipart,
    modo: ModoDeSubida,
) -> (StatusCode, Json<Value>) {
    let recibida = match recibir_subida(multipart, modo).await {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "detail": e })));
        }
    };
    let temporal = recibida.temporal.clone();
    // El staging se borra pase lo que pase: un `?` que salga antes del final
    // no tiene que dejar basura en disco.
    let limpiar = || {
        let _ = std::fs::remove_dir_all(&temporal);
    };

    if recibida.guardados.is_empty() && recibida.errores.is_empty() {
        limpiar();
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "detail": "No se enviaron archivos." })),
        );
    }

    let (project_id, dir) = match resolver_o_crear(recibida.project_id_pedido.clone()) {
        Ok(v) => v,
        Err((code, detail)) => {
            limpiar();
            return (code, Json(json!({ "detail": detail })));
        }
    };

    if let Err(e) = subidas::mover_contenido(&recibida.temporal, &dir) {
        limpiar();
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "detail": format!("no se pudieron guardar los archivos: {e}") })),
        );
    }
    limpiar();

    let fallback = match modo {
        ModoDeSubida::Sueltos => format!("project-{}", &project_id[..8.min(project_id.len())]),
        ModoDeSubida::Carpeta => {
            recibida.primera_carpeta.clone().unwrap_or_else(|| "folder".to_string())
        }
    };
    let (arbol, nombre, _info) = finalizar(&dir, &recibida.nombre_pedido, &fallback);

    (
        StatusCode::OK,
        Json(json!({
            "project_id": project_id,
            "project_name": nombre,
            "type": if modo == ModoDeSubida::Sueltos { "files" } else { "folder" },
            "saved": recibida.guardados,
            "errors": recibida.errores,
            "tree": arbol,
            "total_files": recibida.guardados.len(),
        })),
    )
}

async fn upload_zip(mut multipart: axum::extract::Multipart) -> (StatusCode, Json<Value>) {
    let mut nombre_pedido = String::new();
    let mut nombre_archivo: Option<String> = None;
    let mut contenido: Option<axum::body::Bytes> = None;

    while let Ok(Some(campo)) = multipart.next_field().await {
        let campo_nombre = campo.name().unwrap_or("").to_string();
        match campo.file_name().map(str::to_string) {
            Some(f) => {
                nombre_archivo = Some(f);
                contenido = campo.bytes().await.ok();
            }
            None if campo_nombre == "project_name" => {
                nombre_pedido = campo.text().await.unwrap_or_default();
            }
            None => {}
        }
    }

    let (Some(nombre_archivo), Some(bytes)) = (nombre_archivo, contenido) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "detail": "No se envió ningún archivo." })),
        );
    };
    if !nombre_archivo.to_ascii_lowercase().ends_with(".zip") {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "detail": "Solo se aceptan archivos .zip" })),
        );
    }
    if bytes.len() as u64 > subidas::MAX_ZIP {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({ "detail": "ZIP demasiado grande (máx 200 MB)" })),
        );
    }
    if !complexity_core::servicio::zip::es_zip(&bytes) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "detail": "El archivo no es un ZIP válido." })),
        );
    }

    let resultado = tokio::task::spawn_blocking(move || {
        let (project_id, dir) = resolver_o_crear(None)?;
        let extraccion = subidas::extraer_zip(&bytes, &dir);
        // El nombre por defecto es el del ZIP sin la extension.
        let tallo = nombre_archivo.rsplit_once('.').map(|(a, _)| a).unwrap_or(&nombre_archivo).to_string();
        let (arbol, nombre, info) = finalizar(&dir, &nombre_pedido, &tallo);
        Ok::<Value, (StatusCode, String)>(json!({
            "project_id": project_id,
            "project_name": nombre,
            "type": "zip",
            "original_zip": nombre_archivo,
            "extracted": extraccion["extracted"],
            "skipped": extraccion["skipped"],
            "errors": extraccion["errors"],
            "tree": arbol,
            "info": info,
            // A diferencia de /files y /folder, esta respuesta no traia
            // `total_files` en el nivel superior (solo dentro de `info`), y el
            // frontend lo lee de arriba para el header del explorador: todo
            // proyecto creado por ZIP mostraba "undefined archivos".
            "total_files": info["total_files"],
        }))
    })
    .await;

    match resultado {
        Ok(Ok(v)) => (StatusCode::OK, Json(v)),
        Ok(Err((code, detail))) => (code, Json(json!({ "detail": detail }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "detail": format!("la extracción no terminó: {e}") })),
        ),
    }
}

/// Un proyecto sin ningun archivo, para escribir codigo desde cero en vez de
/// partir de algo ya existente.
async fn upload_empty(mut multipart: axum::extract::Multipart) -> (StatusCode, Json<Value>) {
    let mut nombre_pedido = String::new();
    while let Ok(Some(campo)) = multipart.next_field().await {
        if campo.name() == Some("project_name") {
            nombre_pedido = campo.text().await.unwrap_or_default();
        }
    }
    match tokio::task::spawn_blocking(move || {
        let (project_id, dir) = resolver_o_crear(None)?;
        let (arbol, nombre, _info) = finalizar(&dir, &nombre_pedido, "proyecto-vacío");
        Ok::<Value, (StatusCode, String)>(json!({
            "project_id": project_id,
            "project_name": nombre,
            "type": "empty",
            "saved": [],
            "errors": [],
            "tree": arbol,
            "total_files": 0,
        }))
    })
    .await
    {
        Ok(Ok(v)) => (StatusCode::OK, Json(v)),
        Ok(Err((code, detail))) => (code, Json(json!({ "detail": detail }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "detail": e.to_string() })),
        ),
    }
}

async fn upload_projects() -> Json<Value> {
    let proyectos = tokio::task::spawn_blocking(subidas::listar).await.unwrap_or_default();
    let total = proyectos.len();
    Json(json!({ "projects": proyectos, "total": total }))
}

async fn upload_tree(
    axum::extract::Path(project_id): axum::extract::Path<String>,
) -> (StatusCode, Json<Value>) {
    match tokio::task::spawn_blocking(move || {
        let dir = subidas::ruta_de_proyecto(&project_id)
            .ok_or((StatusCode::BAD_REQUEST, "Path inválido.".to_string()))?;
        if !dir.is_dir() {
            return Err((StatusCode::NOT_FOUND, "Proyecto no encontrado.".to_string()));
        }
        Ok::<Value, (StatusCode, String)>(json!({
            "project_id": project_id,
            "tree": subidas::arbol(&dir),
            "info": subidas::info_cacheada(&dir),
        }))
    })
    .await
    {
        Ok(Ok(v)) => (StatusCode::OK, Json(v)),
        Ok(Err((code, detail))) => (code, Json(json!({ "detail": detail }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "detail": e.to_string() })),
        ),
    }
}

#[derive(Deserialize)]
struct RutaDeArchivo {
    path: String,
}

async fn upload_file_content(
    axum::extract::Path(project_id): axum::extract::Path<String>,
    Query(q): Query<RutaDeArchivo>,
) -> (StatusCode, Json<Value>) {
    match tokio::task::spawn_blocking(move || {
        let dir = subidas::ruta_de_proyecto(&project_id)
            .ok_or((StatusCode::BAD_REQUEST, "Path inválido.".to_string()))?;
        // El path del archivo se valida igual que el del proyecto: por
        // componentes normalizados, no por prefijo de texto.
        let candidato = subidas::normalizar(&dir.join(&q.path));
        if !candidato.starts_with(&dir) {
            return Err((StatusCode::BAD_REQUEST, "Path inválido.".to_string()));
        }
        if !candidato.is_file() {
            return Err((StatusCode::NOT_FOUND, "Archivo no encontrado.".to_string()));
        }
        let tamano = candidato.metadata().map(|m| m.len()).unwrap_or(0);
        if tamano > subidas::MAX_LECTURA {
            return Err((
                StatusCode::PAYLOAD_TOO_LARGE,
                "Archivo muy grande para leer (máx 5 MB).".to_string(),
            ));
        }
        let crudo = std::fs::read(&candidato).map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al leer el archivo: {e}"))
        })?;
        // `from_utf8_lossy` es el equivalente de `errors="replace"`: un
        // archivo con bytes invalidos se muestra con el caracter de reemplazo
        // en vez de fallar — abrir un binario por accidente no tiene que dar
        // un 500.
        let nombre = candidato.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        Ok::<Value, (StatusCode, String)>(json!({
            "path": q.path,
            "content": String::from_utf8_lossy(&crudo),
            "size": tamano,
            "extension": subidas::extension_de(&nombre),
        }))
    })
    .await
    {
        Ok(Ok(v)) => (StatusCode::OK, Json(v)),
        Ok(Err((code, detail))) => (code, Json(json!({ "detail": detail }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "detail": e.to_string() })),
        ),
    }
}

async fn upload_create_file(
    axum::extract::Path(project_id): axum::extract::Path<String>,
    mut multipart: axum::extract::Multipart,
) -> (StatusCode, Json<Value>) {
    let mut ruta = String::new();
    let mut contenido = String::new();
    while let Ok(Some(campo)) = multipart.next_field().await {
        match campo.name().unwrap_or("") {
            "path" => ruta = campo.text().await.unwrap_or_default(),
            "content" => contenido = campo.text().await.unwrap_or_default(),
            _ => {}
        }
    }

    match tokio::task::spawn_blocking(move || {
        let dir = subidas::ruta_de_proyecto(&project_id)
            .ok_or((StatusCode::BAD_REQUEST, "Path inválido.".to_string()))?;
        if !dir.is_dir() {
            return Err((StatusCode::NOT_FOUND, "Proyecto no encontrado.".to_string()));
        }
        // **Aca el path lo escribe una persona a mano**, a diferencia de
        // `/folder`, donde la lista de archivos la genero el browser. Un `..`
        // se RECHAZA en vez de sanearse en silencio hacia otro lugar dentro
        // del proyecto: reinterpretar lo que alguien tipeo es peor que
        // decirle que no.
        let crudas: Vec<&str> = ruta.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
        if crudas.is_empty() || crudas.iter().any(|p| *p == "." || *p == "..") {
            return Err((StatusCode::BAD_REQUEST, "Path inválido.".to_string()));
        }
        let destino = crudas.iter().fold(dir.clone(), |a, p| a.join(p));
        if !subidas::normalizar(&destino).starts_with(&dir) {
            return Err((StatusCode::BAD_REQUEST, "Path inválido.".to_string()));
        }
        if destino.exists() {
            return Err((StatusCode::CONFLICT, "Ya existe un archivo con ese nombre.".to_string()));
        }
        if let Some(padre) = destino.parent() {
            std::fs::create_dir_all(padre)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }
        std::fs::write(&destino, contenido.as_bytes())
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        // `actualizar_metadata` y no `info_de` + `guardar_metadata`: eso
        // ultimo le borraba el nombre al proyecto en cada archivo nuevo. Ver
        // el doc de esa funcion.
        subidas::actualizar_metadata(&dir);
        Ok::<Value, (StatusCode, String)>(json!({
            "path": crudas.join("/"),
            "size": contenido.len(),
        }))
    })
    .await
    {
        Ok(Ok(v)) => (StatusCode::OK, Json(v)),
        Ok(Err((code, detail))) => (code, Json(json!({ "detail": detail }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "detail": e.to_string() })),
        ),
    }
}

async fn upload_delete_project(
    axum::extract::Path(project_id): axum::extract::Path<String>,
) -> (StatusCode, Json<Value>) {
    match tokio::task::spawn_blocking(move || {
        let dir = subidas::ruta_de_proyecto(&project_id)
            .ok_or((StatusCode::BAD_REQUEST, "Path inválido.".to_string()))?;
        if !dir.is_dir() {
            return Err((StatusCode::NOT_FOUND, "Proyecto no encontrado.".to_string()));
        }
        subidas::borrar_proyecto(&dir)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok::<Value, (StatusCode, String)>(
            json!({ "message": format!("Proyecto {project_id} eliminado correctamente.") }),
        )
    })
    .await
    {
        Ok(Ok(v)) => (StatusCode::OK, Json(v)),
        Ok(Err((code, detail))) => (code, Json(json!({ "detail": detail }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "detail": e.to_string() })),
        ),
    }
}

/// Lo que cada plugin de lenguaje reclama y con que parser. Sale del manifest,
/// que es la unica fuente de verdad desde la Fase 24.
async fn static_languages() -> Json<Value> {
    let mut lenguajes = serde_json::Map::new();
    for m in complexity_core::servicio::plugin::builtin_manifests() {
        lenguajes.insert(
            m.id.to_string(),
            json!({
                "extensions": m.extensions,
                "parser": m.parser,
                "features": m.features,
                // Antes esto preguntaba si el sidecar respondia. Ahora el
                // motor es quien sirve, asi que si esta ruta contesta, esta.
                "available": true,
            }),
        );
    }
    Json(json!({
        "languages": lenguajes,
        "capabilities": { "complexity_engine": true },
    }))
}

async fn static_parse(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    Json(complexity_core::servicio::scanner::parse_one(req.filename, &req.content))
}

/// **Puede tardar**: escanea y parsea un proyecto entero del disco, asi que
/// va a `spawn_blocking` por el mismo motivo que SPARK.
async fn static_parse_project(
    Json(req): Json<complexity_core::servicio::proyecto::ProyectoRequest>,
) -> Json<Value> {
    match tokio::task::spawn_blocking(move || {
        complexity_core::servicio::proyecto::analizar_proyecto(&req)
    })
    .await
    {
        Ok(v) => Json(v),
        Err(e) => Json(json!({ "files": [], "error": format!("el análisis no terminó: {e}") })),
    }
}

/// Solo el Big-O de un archivo, con su distribucion y los caminos calientes.
async fn static_big_o(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    let parsed = complexity_core::servicio::scanner::parse_one(req.filename.clone(), &req.content);
    let vacio = vec![];
    let funciones = parsed.get("functions").and_then(Value::as_array).unwrap_or(&vacio);
    let tabla: Vec<Value> = funciones
        .iter()
        .map(|f| {
            json!({
                "function": f.get("name"),
                "line": f.get("line"),
                "big_o": f.get("big_o").and_then(Value::as_str).unwrap_or("?"),
                "reason": f.get("big_o_reason").and_then(Value::as_str).unwrap_or(""),
                "complexity": f.get("complexity").and_then(Value::as_u64).unwrap_or(1),
                "loc": f.get("loc").and_then(Value::as_u64).unwrap_or(0),
            })
        })
        .collect();
    let mut distribucion = serde_json::Map::new();
    for e in &tabla {
        let k = e.get("big_o").and_then(Value::as_str).unwrap_or("?").to_string();
        let n = distribucion.get(&k).and_then(Value::as_u64).unwrap_or(0) + 1;
        distribucion.insert(k, json!(n));
    }
    // "Caliente" es O(n²) o peor — la misma lista que usaba Python.
    const CALIENTES: &[&str] = &["O(n²)", "O(n³)", "O(2^n)", "O(n^4)"];
    let hot: Vec<Value> = tabla
        .iter()
        .filter(|e| {
            e.get("big_o").and_then(Value::as_str).is_some_and(|b| CALIENTES.contains(&b))
        })
        .cloned()
        .collect();
    Json(json!({
        "filename": req.filename,
        "language": parsed.get("language").and_then(Value::as_str).unwrap_or("?"),
        "functions": tabla,
        "distribution": distribucion,
        "hot_paths": hot,
        "total": tabla.len(),
    }))
}

async fn static_wasm(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    let parsed = complexity_core::servicio::scanner::parse_one(req.filename.clone(), &req.content);
    let vacio = vec![];
    let hints = parsed.get("wasm_hints").and_then(Value::as_array).unwrap_or(&vacio);
    let criticos: Vec<Value> = hints
        .iter()
        .filter(|h| h.get("priority").and_then(Value::as_u64).unwrap_or(0) >= 4)
        .cloned()
        .collect();
    let resumen = if hints.is_empty() {
        format!("{} función(es) candidata(s) a WASM/Cython. Sin candidatos detectados.", hints.len())
    } else {
        format!(
            "{} función(es) candidata(s) a WASM/Cython. {} crítica(s).",
            hints.len(),
            criticos.len()
        )
    };
    Json(json!({
        "filename": req.filename,
        "language": parsed.get("language").and_then(Value::as_str).unwrap_or("?"),
        "hints": hints,
        "total": hints.len(),
        "critical": criticos,
        "summary": resumen,
    }))
}

async fn static_compile_asm(Json(req): Json<CompilacionRequest>) -> Json<Value> {
    let niveles = if req.opt_levels.is_empty() { niveles_por_defecto() } else { req.opt_levels };
    let r = complexity_core::costo::compilar::comparar_niveles(
        &req.language,
        &req.content,
        &niveles,
        &req.features,
    );
    Json(json!({ "available": true, "results": r, "error": Value::Null }))
}

async fn static_compile_pareto(Json(req): Json<ParetoRequest>) -> Json<Value> {
    let configs = if req.configs.is_empty() {
        complexity_core::costo::compilar::configs_por_defecto()
    } else {
        req.configs
    };
    let r = complexity_core::costo::compilar::buscar_pareto(&req.language, &req.content, &configs);
    Json(json!({ "available": true, "result": r, "error": Value::Null }))
}

async fn static_css_atomic(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    let analisis = complexity_core::lenguajes::cssparse::parse_css(&req.content);
    let r = complexity_core::servicio::cssatomico::atomizar(&analisis);
    Json(json!({ "available": true, "result": r, "error": Value::Null }))
}

/// El cliente manda `project_id`; el directorio y las listas las resuelve el
/// motor, que es quien lee el disco.
async fn static_css_usage(
    Json(req): Json<complexity_core::servicio::proyecto::ProyectoRequest>,
) -> Json<Value> {
    let Some(id) = req.project_id.as_deref().filter(|s| !s.is_empty()) else {
        return Json(json!({ "available": false, "reason": "falta el project_id" }));
    };
    if id.contains("..") || id.contains('/') || id.contains('\\') {
        return Json(json!({ "available": false, "reason": "identificador de proyecto inválido" }));
    }
    let dir = complexity_core::servicio::proyecto::directorio_de_subidas().join(id);
    if !dir.is_dir() {
        return Json(json!({ "available": false, "reason": "el proyecto no existe" }));
    }
    let (extensions, filenames) = complexity_core::servicio::proyecto::reclamos_de_los_plugins();
    let req = complexity_core::servicio::scanner::ScanRequest {
        project_dir: dir.to_string_lossy().to_string(),
        extensions,
        ignored_dirs: complexity_core::servicio::proyecto::DIRECTORIOS_IGNORADOS
            .iter()
            .map(|d| (*d).to_string())
            .collect(),
        filenames,
    };
    let files = complexity_core::servicio::scanner::collect_project_files(&req);
    let r = complexity_core::servicio::cssusage::analyze_css_usage(&files);
    let mut salida = serde_json::to_value(r).unwrap_or_else(|_| json!({}));
    if let Some(o) = salida.as_object_mut() {
        o.insert("available".to_string(), json!(true));
    }
    Json(salida)
}

async fn css_atomico(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    let analisis = complexity_core::lenguajes::cssparse::parse_css(&req.content);
    to_json(complexity_core::servicio::cssatomico::atomizar(&analisis))
}

async fn parse_css(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    let result = complexity_core::lenguajes::cssparse::parse_css(&req.content);
    to_json(result)
}

/// HTML (2026-09-02) — 10mo plugin de lenguaje. Accesibilidad y estructura
/// del documento, no metricas de codigo: un HTML no tiene funciones ni Big-O.
async fn parse_html(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    let result = complexity_core::lenguajes::htmlparse::analyze_html(&req.content);
    to_json(result)
}

/// Dependency Intelligence (2026-09-02) — primera Format Extension. A
/// diferencia de los 9 plugins de lenguaje, despacha por NOMBRE de archivo,
/// asi que necesita el `filename` del request (no solo el contenido).
async fn parse_manifest(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    let result = complexity_core::lenguajes::manifest::analyze_manifest(&req.filename, &req.content);
    to_json(result)
}

/// Sythrall CSS Engine — referencias cruzadas CSS ↔ marcado/scripts. Recibe
/// el mismo shape que `/scan/project` (directorio + extensiones + ignorados)
/// porque necesita leer el proyecto ENTERO: cruzar archivos es justamente lo
/// que este análisis hace, a diferencia de `/parse/css`, que mira uno solo.
async fn css_usage(Json(req): Json<complexity_core::servicio::scanner::ScanRequest>) -> Json<Value> {
    let files = complexity_core::servicio::scanner::collect_project_files(&req);
    to_json(complexity_core::servicio::cssusage::analyze_css_usage(&files))
}

/// Fase 18 — primer slice del Graph Engine: Import Graph. Recibe el resumen
/// ya parseado de cada archivo del proyecto (no archivos crudos — el parsing
/// multi-lenguaje sigue en Python) y devuelve nodes/edges/mermaid/entry_points
/// en el mismo shape que `_build_import_graph` en `graph.py`.
async fn graph_import(Json(req): Json<Vec<complexity_core::flujo::graph::FileSummary>>) -> Json<Value> {
    let result = complexity_core::build_import_graph(req);
    to_json(result)
}

/// Fase 18 — segunda porción del Graph Engine: Centrality. Mismo contrato
/// que `graph_import` (resumen ya parseado por archivo, no archivos crudos).
async fn graph_centrality(Json(req): Json<Vec<complexity_core::flujo::graph::FileSummary>>) -> Json<Value> {
    let result = complexity_core::build_centrality_graph(req);
    to_json(result)
}

/// Fase 18 — tercera porción del Graph Engine: Call Graph. A diferencia de
/// `graph_import`/`graph_centrality`, el payload no es `FileSummary` — Call
/// Graph necesita detalle por función y el `call_graph` ya calculado por
/// archivo, no imports/dead_code.
async fn graph_call(Json(req): Json<Vec<complexity_core::flujo::graph::CallGraphFileInput>>) -> Json<Value> {
    let result = complexity_core::build_call_graph(req);
    to_json(result)
}

/// Fase 18 — cuarta y última porción del Graph Engine: Circular Deps.
async fn graph_circular(Json(req): Json<Vec<complexity_core::flujo::graph::FileSummary>>) -> Json<Value> {
    let result = complexity_core::build_circular_graph(req);
    to_json(result)
}

/// Fase 18 — "Dependency Engine": Architecture Smells, el último ítem del
/// Graph Engine que todavía era Python-only (`_build_architecture_smells`
/// en `graph.py`). Mismo payload `FileSummary` que Import/Centrality/
/// Circular — pura orquestación sobre lo que esas 3 porciones ya
/// construyeron, ver el comentario de sección de `graph.rs`.
async fn graph_architecture(Json(req): Json<Vec<complexity_core::flujo::graph::FileSummary>>) -> Json<Value> {
    let result = complexity_core::build_architecture_smells(req);
    to_json(result)
}

/// Complexity Heatmap — la última pieza del Graph Engine que quedaba
/// Python-only (todos los comentarios de Import/Call/Circular/Architecture
/// Smells la nombraban explícitamente como "todavía Python, fuera de este
/// slice"). Payload propio (`HeatmapFileInput`, no `FileSummary`): necesita
/// detalle por función (complexity/big_o/line/loc), no solo el resumen
/// agregado que el resto de los tipos de grafo usa.
async fn graph_heatmap(Json(req): Json<Vec<complexity_core::flujo::graph::HeatmapFileInput>>) -> Json<Value> {
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
    match complexity_core::servicio::ml::detect(&req.content) {
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
    let path = logstore::log_dir().join(format!("{}.cbor", req.source));
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
    match complexity_core::lenguajes::cparse::parse_c(&req.content) {
        Some(result) => to_json(result),
        None => Json(json!({"error": "parseo tree-sitter falló"})),
    }
}

async fn parse_cpp(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    match complexity_core::lenguajes::cparse::parse_cpp(&req.content) {
        Some(result) => to_json(result),
        None => Json(json!({"error": "parseo tree-sitter falló"})),
    }
}

async fn parse_js(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    let result = complexity_core::lenguajes::jsts::parse_js_ts(&req.content, false);
    to_json(result)
}

async fn parse_ts(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    let result = complexity_core::lenguajes::jsts::parse_js_ts(&req.content, true);
    to_json(result)
}

/// Fase 20 (Scientific Intelligence) — primer lenguaje que nace directo en
/// Rust sin parser Python previo que reemplazar (a diferencia de C/C++/JS/
/// TS, todos puertos de `static_parser.py`). Mismo criterio de error que
/// `parse_c`/`parse_cpp`: `None` (falla de gramática tree-sitter) se traduce
/// a un shape de error, no a un 500.
async fn parse_fortran(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    match complexity_core::lenguajes::fparse::parse_fortran(&req.content) {
        Some(result) => to_json(result),
        None => Json(json!({"error": "parseo tree-sitter falló"})),
    }
}

/// Fase 34, Línea E, paso 3 — SPARK: la categoría de evidencia que el motor
/// no tenía. Corre GNATprove sobre el fuente y devuelve sus obligaciones, cada
/// una con el resultado en el `kind` (`spark_proved`/`spark_unproved`) para
/// que `decidibilidad.rs` la clasifique sin excepciones por instancia.
///
/// **Va a `spawn_blocking`, y eso se midió antes de escribirlo.** Un probador
/// resolviendo fórmulas no es un parser: puede tardar minutos. Llamándolo
/// directo desde el handler `async` ocupa un worker del runtime — medido, un
/// `/health` durante una corrida de SPARK pasó de ~60 ms a **1.533 ms**, y
/// esa corrida era de las rápidas. Es el mismo error que la regla de
/// `run_in_threadpool` evita del lado de Python, en la otra punta.
async fn verify_spark(Json(req): Json<VerificacionRequest>) -> Json<Value> {
    match tokio::task::spawn_blocking(move || complexity_core::teoria::spark::verificar(&req.content))
        .await
    {
        Ok(r) => to_json(r),
        Err(e) => Json(json!({"error": format!("la verificación no terminó: {e}")})),
    }
}

/// Fase 34, Línea C, paso 1 del orden interno — D como lenguaje ANALIZADO,
/// y con él la superficie de confianza de TRES niveles que es la razón por la
/// que D entró a esta fase. Mismo criterio de error que los demás `/parse/*`.
async fn parse_d(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    match complexity_core::lenguajes::dparse::parse_d(&req.content) {
        Some(result) => to_json(result),
        None => Json(json!({"error": "parseo tree-sitter falló"})),
    }
}

/// Fase 34, Línea E, paso 1 — Ada como lenguaje ANALIZADO.
///
/// Los otros dos pasos de esa línea (código máquina vía GNAT, SPARK como
/// tercera categoría de evidencia) están bloqueados por el toolchain ausente;
/// éste no, porque `tree-sitter-ada` parsea desde el texto como los otros
/// seis. Mismo criterio de error que `parse_c`/`parse_fortran`.
async fn parse_ada(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    match complexity_core::lenguajes::adaparse::parse_ada(&req.content) {
        Some(result) => to_json(result),
        None => Json(json!({"error": "parseo tree-sitter falló"})),
    }
}

/// Fase 34, Línea B (Trust Surface) — Rust como TARGET de análisis, no solo
/// como lenguaje de implementación. Mismo criterio de error que
/// `parse_c`/`parse_fortran`: un `None` de la gramática se traduce a un shape
/// de error, no a un 500.
async fn parse_rust(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    match complexity_core::lenguajes::rsparse::parse_rust(&req.content) {
        Some(result) => to_json(result),
        None => Json(json!({"error": "parseo tree-sitter falló"})),
    }
}

/// Fase 19 (Machine Intelligence) — pattern-matching sobre texto plano, no
/// tree-sitter: `asmparse::parse` nunca devuelve `None` (a diferencia de los
/// demás `/parse/*`), así que no hay branch de error acá.
async fn parse_asm(Json(req): Json<ComplexityRequest>) -> Json<Value> {
    to_json(complexity_core::lenguajes::asmparse::parse(&req.content))
}

/// Pedido de compilación a ensamblador. `opt_levels` es una lista porque la
/// forma útil de esto es la COMPARACIÓN —`-O0` contra `-O2`—, no un número
/// suelto; con uno solo se manda una lista de uno.
#[derive(Deserialize)]
struct CompilacionRequest {
    language: String,
    content: String,
    #[serde(default = "niveles_por_defecto")]
    opt_levels: Vec<String>,
    /// Extensiones de CPU a pedirle al compilador (`avx2`, `avx512f`, …).
    /// Lo que no esté en la tabla de `compilar.rs` se ignora.
    #[serde(default)]
    features: Vec<String>,
}

fn niveles_por_defecto() -> Vec<String> {
    vec!["0".to_string(), "2".to_string()]
}

/// **El puente del fuente al código máquina** (`compilar.rs`). Invoca el
/// compilador del lenguaje en modo "emitir ensamblador" —se detiene antes de
/// ensamblar y de linkear, no ejecuta nada— y le pasa el resultado a
/// `asmparse.rs`, que ya existía y hasta ahora solo servía si alguien pegaba
/// ensamblador a mano.
#[derive(Deserialize)]
struct ParetoRequest {
    language: String,
    content: String,
    /// Grilla de configuraciones. Vacía usa la de `compilar.rs`.
    #[serde(default)]
    configs: Vec<complexity_core::costo::compilar::ConfigCompilacion>,
}

/// **Búsqueda multi-objetivo sobre la configuración de compilación.** Mismo
/// fuente, distintas opciones: el frente de Pareto sobre (instrucciones ↓,
/// instrucciones packed ↑). No toca el código del usuario, así que no hay
/// nada que probar sobre equivalencia — ver el doc de `compilar.rs`.
async fn pareto_compilacion(Json(req): Json<ParetoRequest>) -> Json<Value> {
    let configs = if req.configs.is_empty() {
        complexity_core::costo::compilar::configs_por_defecto()
    } else {
        req.configs
    };
    to_json(complexity_core::costo::compilar::buscar_pareto(&req.language, &req.content, &configs))
}

async fn compilar_asm(Json(req): Json<CompilacionRequest>) -> Json<Value> {
    let niveles = if req.opt_levels.is_empty() { niveles_por_defecto() } else { req.opt_levels };
    to_json(complexity_core::costo::compilar::comparar_niveles(
        &req.language,
        &req.content,
        &niveles,
        &req.features,
    ))
}

/// Fase 19 (Machine Intelligence) — segundo bullet, cierra la fase:
/// analizador de ejecutables reales (PE/ELF/Mach-O/COFF/wasm) vía el crate
/// `object` (`binparse.rs`). A diferencia de todos los demás `/parse/*`, el
/// body NO es JSON — es el binario crudo (`Content-Type:
/// application/octet-stream`), con un límite de tamaño elevado a 64MB
/// específicamente en esta ruta (el default de axum, 2MB, alcanza para
/// texto fuente pero no para un `.exe`/`.dll` real cualquiera).
/// **Fase 35 — el primer prefijo que Python deja de sostener.**
///
/// `/binary/parse` vivia en `apps/api/routers/binary_analysis.py`, que leia
/// el upload y reenviaba los bytes por HTTP a `/parse/executable` de este
/// mismo binario. **Aca `binparse` esta en proceso**, asi que ese salto HTTP
/// no se porto: desaparecio. Es la primera prueba de lo que la Fase 35 dice
/// sobre las ~2.300 lineas de relay — no se traducen, se borran.
///
/// El contrato con el frontend no cambia: mismo `multipart/form-data` con el
/// campo `file`, mismo shape de respuesta, misma degradacion con gracia.
async fn binary_parse(mut multipart: axum::extract::Multipart) -> Json<Value> {
    while let Ok(Some(campo)) = multipart.next_field().await {
        let Ok(bytes) = campo.bytes().await else {
            continue;
        };
        if bytes.len() > MAX_UPLOAD_BINARIO {
            let mb = MAX_UPLOAD_BINARIO / (1024 * 1024);
            return to_json(complexity_core::lenguajes::binparse::degraded(format!(
                "el archivo pesa más de {mb}MB — no se analiza."
            )));
        }
        return to_json(complexity_core::lenguajes::binparse::parse(&bytes));
    }
    to_json(complexity_core::lenguajes::binparse::degraded(
        "no llegó ningún archivo en el formulario.".to_string(),
    ))
}

async fn parse_executable(bytes: Bytes) -> Json<Value> {
    to_json(complexity_core::lenguajes::binparse::parse(&bytes))
}

/// Fase 23 (Execution Intelligence) — primer endpoint de este binario que
/// ejecuta código (todo lo demás es análisis estático puro). Compila y corre
/// un kernel Fortran escrito por Sythrall mismo (`fortran_bench.rs`, nunca
/// código del usuario) para validar empíricamente el O(n³) que
/// `numerical_algorithm_note` predice por forma en Fase 20. Sin body — el
/// kernel y los tamaños de `n` son fijos en esta primera versión.
async fn validate_matmul() -> Json<Value> {
    to_json(complexity_core::bench::fortran_bench::validate_matmul_cubic())
}

/// Fase 34, Linea A — cierra la promesa de `plantillas.rs`: ese modulo cuenta
/// lo ESCRITO y dice que el costo real exige compilar; esto es ese "exige
/// compilar". Compila una sonda que Sythrall escribe (nunca codigo del
/// usuario, mismo criterio que `validate_matmul`) a varias profundidades de
/// instanciacion y mide como escala el costo EN ESTA MAQUINA. Sin body.
async fn validate_template_cost() -> Json<Value> {
    to_json(complexity_core::costo::tplcost::validate_template_cost())
}

/// Fase 26 (Algorithm Validation Engine) — generaliza `validate_matmul`
/// arriba más allá de Fortran/matmul: compila y corre un bubble sort escrito
/// a mano en Zig (`zig_bench.rs`) para validar empíricamente O(n²). Sin
/// body — mismo criterio que `validate_matmul`.
async fn validate_bubble_sort() -> Json<Value> {
    to_json(complexity_core::bench::zig_bench::validate_bubble_sort_quadratic())
}

/// Fase 26 (Algorithm Validation Engine) — tercer kernel de validación,
/// esta vez en Assembly x86 real (`asm_bench.rs`) para validar
/// empíricamente O(n) de una suma de cuadrados. Sin body.
async fn validate_sum_squares() -> Json<Value> {
    to_json(complexity_core::bench::asm_bench::validate_sum_squares_linear())
}

/// Fase 26 (Algorithm Validation Engine) — cuarto kernel de validación,
/// segunda vez en Zig pero una forma algorítmica distinta a
/// `validate_bubble_sort`: recorrido de grafos (BFS, `bfs_bench.rs`) para
/// validar empíricamente O(V+E) sobre un grafo disperso de grado fijo. Sin
/// body.
async fn validate_graph_bfs() -> Json<Value> {
    to_json(complexity_core::bench::bfs_bench::validate_graph_bfs_linear())
}

/// Fase 26 (Algorithm Validation Engine) — quinto kernel, y el primero en
/// validar una forma NO polinomial: profundidad de recursión (Fibonacci
/// recursivo ingenuo, `fib_bench.rs`) para confirmar empíricamente que
/// crece exponencialmente (Θ(φⁿ)), con la base medida comparada contra la
/// razón áurea teórica. Sin body.
async fn validate_fibonacci() -> Json<Value> {
    to_json(complexity_core::bench::fib_bench::validate_naive_fibonacci_exponential())
}

/// Fase 26 (Algorithm Validation Engine) — sexto kernel, y el primero en
/// validar O(n log n): mergesort bottom-up iterativo en Assembly x86
/// (`mergesort_bench.rs`) para confirmar empíricamente esa forma. Sin
/// body.
async fn validate_mergesort() -> Json<Value> {
    to_json(complexity_core::bench::mergesort_bench::validate_mergesort_nlogn())
}

/// Fase 26 (Algorithm Validation Engine) — séptimo kernel, tercera vez en
/// Zig (`agg_bench.rs`), esta vez para la pieza 2 de "migrar de
/// numpy/pandas/scikit-learn": una agregación por categoría (suma/promedio,
/// la forma detrás de un `groupby().agg()`) para validar empíricamente
/// O(n), comparada luego contra pandas real en
/// `routers/execution.py::validate_aggregation_vs_pandas`. Sin body.
async fn validate_aggregation() -> Json<Value> {
    to_json(complexity_core::bench::agg_bench::validate_aggregation_linear())
}

/// Fase 26 (Algorithm Validation Engine) — octavo kernel, segunda vez en
/// Fortran (`linreg_bench.rs`), esta vez para la pieza 3 (y última) de
/// "migrar de numpy/pandas/scikit-learn": una regresión lineal univariada
/// por mínimos cuadrados (forma cerrada) para validar empíricamente O(n),
/// comparada luego contra scikit-learn real en
/// `routers/execution.py::validate_linreg_vs_sklearn`. Sin body.
async fn validate_linreg() -> Json<Value> {
    to_json(complexity_core::bench::linreg_bench::validate_linreg_linear())
}

/// Fase 26 (Algorithm Validation Engine) — noveno kernel, cuarto uso de Zig
/// (`quicksort_bench.rs`) y el primero en demostrar recursión en Zig: un
/// quicksort recursivo (partición de Lomuto) para validar empíricamente la
/// misma forma O(n log n) que `validate_mergesort` ya valida, por una
/// técnica genuinamente distinta (divide y vencerás recursivo, no fusión
/// iterativa) — el "recursive sorting shape" que el ROADMAP nombraba como
/// pendiente. Sin body.
async fn validate_quicksort() -> Json<Value> {
    to_json(complexity_core::bench::quicksort_bench::validate_quicksort_nlogn())
}

/// Fase 26 (Algorithm Validation Engine) — décimo kernel, tercera vez en
/// Assembly (`binsearch_bench.rs`) y la primera en validar una forma
/// SUB-LINEAL: una búsqueda binaria iterativa, el "searching...shapes
/// beyond BFS" que el ROADMAP nombraba como pendiente. A diferencia de
/// todo lo anterior, el eje del ajuste de exponente es `log2(tamaño del
/// arreglo)`, no el tamaño en sí — ver el doc del módulo. Sin body.
async fn validate_binary_search() -> Json<Value> {
    to_json(complexity_core::bench::binsearch_bench::validate_binary_search_logn())
}

/// Fase 24 (Extensibility Platform) — el manifest de los 7 plugins built-in
/// de Sythrall (`plugin.rs`), fuente de verdad que reemplaza el dict
/// hardcodeado que antes vivía en `routers/static_analysis.py::/languages`.
async fn plugin_manifests() -> Json<Value> {
    to_json(complexity_core::servicio::plugin::builtin_manifests())
}

/// Fase 18 — Symbol Engine: go-to-definition/find-references, portado 1:1
/// desde Python (`routers/intelligence.py`, ver `symbols.rs` para el
/// detalle completo). Por archivo, mismo alcance que antes — no a nivel de
/// proyecto entero, eso queda para un ítem futuro separado.
async fn symbols_definition_python(Json(req): Json<SymbolRequest>) -> Json<Value> {
    let defs = complexity_core::ast::symbols::find_definitions_python(&req.content, &req.symbol);
    to_json(defs)
}

async fn symbols_definition_js(Json(req): Json<SymbolRequest>) -> Json<Value> {
    let defs = complexity_core::ast::symbols::find_definitions_jsts(&req.content, false, &req.symbol);
    to_json(defs)
}

async fn symbols_definition_ts(Json(req): Json<SymbolRequest>) -> Json<Value> {
    let defs = complexity_core::ast::symbols::find_definitions_jsts(&req.content, true, &req.symbol);
    to_json(defs)
}

async fn symbols_references_python(Json(req): Json<SymbolRequest>) -> Json<Value> {
    let (refs, definition_line) = complexity_core::ast::symbols::find_references_python(&req.content, &req.symbol);
    Json(json!({ "references": refs, "definition_line": definition_line }))
}

async fn symbols_references_js(Json(req): Json<SymbolRequest>) -> Json<Value> {
    let (refs, definition_line) = complexity_core::ast::symbols::find_references_jsts(&req.content, false, &req.symbol);
    Json(json!({ "references": refs, "definition_line": definition_line }))
}

async fn symbols_references_ts(Json(req): Json<SymbolRequest>) -> Json<Value> {
    let (refs, definition_line) = complexity_core::ast::symbols::find_references_jsts(&req.content, true, &req.symbol);
    Json(json!({ "references": refs, "definition_line": definition_line }))
}

/// Fase 18 — Project Scanner: única vez que este sidecar lee del disco
/// directamente (ver `scanner.rs` para la justificación completa). Recibe
/// un directorio ya validado/resuelto por Python (nunca uno crudo del
/// cliente) más las mismas listas de extensiones/directorios ignorados que
/// `project_service.py` ya usa — Python sigue siendo la única fuente de
/// verdad de esas listas, acá solo se reciben como parámetros para no
/// duplicarlas ni arriesgar que diverjan entre las dos implementaciones.
async fn scan_project(Json(req): Json<complexity_core::servicio::scanner::ScanRequest>) -> Json<Value> {
    let files = complexity_core::servicio::scanner::scan_and_parse_project(&req);
    Json(json!({ "files": files }))
}

async fn log_read(Query(q): Query<LogQuery>) -> Json<Value> {
    let dir = logstore::log_dir();
    let mut entries: Vec<LogEntry> = LOG_SOURCES.iter().flat_map(|src| logstore::read_all(&dir.join(format!("{src}.cbor")))).collect();
    entries.sort_by(|a, b| a.ts.cmp(&b.ts));
    let total = entries.len();
    let tail: Vec<LogEntry> = entries.split_off(total.saturating_sub(q.limit));
    Json(json!({ "logs": tail, "total": total }))
}
