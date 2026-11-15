use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Query};
use axum::routing::{get, post};
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
        .route("/parse/ada", post(parse_ada))
        .route("/parse/d", post(parse_d))
        .route("/verify/spark", post(verify_spark))
        .route("/parse/rust", post(parse_rust))
        .route("/parse/asm", post(parse_asm))
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
async fn verify_spark(Json(req): Json<ComplexityRequest>) -> Json<Value> {
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
    let dir = log_dir();
    let mut entries: Vec<LogEntry> = LOG_SOURCES.iter().flat_map(|src| logstore::read_all(&dir.join(format!("{src}.cbor")))).collect();
    entries.sort_by(|a, b| a.ts.cmp(&b.ts));
    let total = entries.len();
    let tail: Vec<LogEntry> = entries.split_off(total.saturating_sub(q.limit));
    Json(json!({ "logs": tail, "total": total }))
}
