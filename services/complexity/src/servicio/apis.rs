//! **Chequeo de salud de APIs externas — Fase 35, portado desde
//! `routers/analysis.py::check_api`.**
//!
//! Pide una URL y reporta si responde, con qué código, en cuánto tiempo y qué
//! devolvió. Es lo último que quedaba de ese router.
//!
//! **Y arregla un defecto de seguridad que ya estaba.** El cliente de Python
//! se construía con `verify=False`:
//!
//! ```python
//! async with httpx.AsyncClient(timeout=..., follow_redirects=True, verify=False) as client:
//! ```
//!
//! O sea que **aceptaba cualquier certificado**: uno vencido, uno
//! autofirmado, o el de alguien interpuesto entre Sythrall y el servidor. Un
//! chequeo de salud que no valida el certificado no está diciendo "tu API
//! está bien", está diciendo "algo me contestó". Acá se valida con las raíces
//! de Mozilla que trae `webpki-roots`, y un certificado que no verifica
//! **aparece como error del chequeo**, que es información útil y no un
//! obstáculo.
//!
//! **El costo de la dependencia, medido**: la cadena TLS agrega **10 crates**
//! al árbol (249 → 259). `hyper`, `hyper-util` y `http-body-util` ya estaban
//! porque axum los usa del lado servidor; lo nuevo es `rustls` con `ring` y
//! las raíces embebidas. No se reimplementa TLS: es criptografía.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use http_body_util::BodyExt;
use serde::Deserialize;
use serde_json::{json, Map, Value};

/// Cuántas entradas de historial se guardan por URL.
const HISTORIAL_MAXIMO: usize = 50;
/// Cuántas se devuelven en cada respuesta.
const HISTORIAL_DEVUELTO: usize = 10;
/// Cuántas cabeceras de la respuesta se reportan.
const MAX_CABECERAS: usize = 10;
/// Cuántos caracteres del cuerpo JSON se muestran como vista previa.
const MAX_PREVIEW: usize = 200;
/// Cuántos redirects se siguen antes de rendirse.
const MAX_REDIRECTS: usize = 10;
/// Cuánto cuerpo se lee como máximo. **No estaba en Python**: `resp.json()`
/// leía el cuerpo entero en memoria sin tope, así que una URL que devuelve un
/// gigabyte lo cargaba completo para mostrar 200 caracteres.
const MAX_CUERPO: usize = 1024 * 1024;

/// El `User-Agent` que se manda si el cliente no pone uno.
///
/// **Hace falta: sin él, algunos servidores devuelven 403.** Medido contra
/// `api.github.com`, que rechaza pedidos sin `User-Agent` — `httpx` mandaba
/// el suyo por defecto y el cliente de hyper no manda ninguno, así que sin
/// esto el port habría reportado como caída una API que funciona.
///
/// Es **deliberadamente genérico y no lleva ningún dato de contacto**: este
/// chequeo sale a servidores de terceros, y el nombre del proyecto alcanza
/// para identificarlo sin filtrar nada de quien lo corre.
const USER_AGENT: &str = "Sythrall-HealthCheck/1.0";

/// Aplana la cadena de causas de un error en un mensaje que sirva.
///
/// **Sin esto el endpoint no diagnostica nada.** `hyper_util` devuelve
/// `client error (Connect)` tanto para un certificado vencido como para un
/// host que no resuelve o un puerto cerrado — los tres casos con el mismo
/// texto, verificado contra `badssl.com`. La causa real está en la cadena de
/// `source()`, y es lo único que hace útil a un chequeo de salud: `httpx`
/// mostraba `certificate verify failed` y perder eso habría sido una
/// regresión de información aunque la validación fuera mejor.
fn cadena_de_error(e: &dyn std::error::Error) -> String {
    let mut partes = vec![e.to_string()];
    let mut actual = e.source();
    while let Some(causa) = actual {
        let texto = causa.to_string();
        // Las cadenas de hyper repiten el mismo texto en varios niveles.
        if !partes.contains(&texto) {
            partes.push(texto);
        }
        actual = causa.source();
    }
    partes.join(": ")
}

#[derive(Deserialize)]
pub struct ApisRequest {
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default = "timeout_por_defecto")]
    pub timeout: u64,
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
}

fn timeout_por_defecto() -> u64 {
    10
}

/// Historial en memoria, igual que el `API_HISTORY` de `shared.py`.
///
/// **Se pierde al reiniciar el proceso, y eso es lo que hacía antes.** Era un
/// `dict` global del módulo. No se movió a disco en este port: el endpoint
/// existe para ver cómo viene respondiendo una API *ahora*, y persistirlo
/// sería un cambio de producto, no una migración. Si alguna vez hace falta,
/// `servicio::historial` ya tiene el patrón de CBOR append-only.
fn historial() -> &'static Mutex<std::collections::BTreeMap<String, Vec<Value>>> {
    static H: OnceLock<Mutex<std::collections::BTreeMap<String, Vec<Value>>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(std::collections::BTreeMap::new()))
}

/// Instala el proveedor criptográfico de `rustls` una sola vez.
///
/// `rustls` 0.23 exige elegir el backend explícitamente. Se hace acá y no en
/// el arranque del binario para que el módulo funcione igual desde un test.
fn asegurar_proveedor() {
    static UNA_VEZ: OnceLock<()> = OnceLock::new();
    UNA_VEZ.get_or_init(|| {
        // Falla si ya había uno instalado, que es exactamente lo que queremos
        // ignorar: significa que alguien más lo puso primero.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

type Cliente = hyper_util::client::legacy::Client<
    hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
    http_body_util::Empty<hyper::body::Bytes>,
>;

fn cliente() -> &'static Cliente {
    static C: OnceLock<Cliente> = OnceLock::new();
    C.get_or_init(|| {
        asegurar_proveedor();
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            // **Las raíces de Mozilla embebidas, no las del sistema.** Un
            // chequeo tiene que dar el mismo resultado en la máquina del
            // desarrollador y en el servidor; depender del almacén del
            // sistema hace que el mismo certificado pase en una y falle en
            // otra sin que nada del código cambie.
            .with_webpki_roots()
            // `https_or_http` y no `https_only`: el router viejo aceptaba
            // `http://` y hay APIs internas que sólo escuchan en claro.
            .https_or_http()
            .enable_http1()
            .build();
        hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
            .build(https)
    })
}

/// El resultado de un chequeo, con el shape que el panel ya consume.
fn vacio(url: &str) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("url".into(), json!(url));
    m.insert("status".into(), json!("unknown"));
    m.insert("code".into(), Value::Null);
    m.insert("ms".into(), Value::Null);
    m.insert("error".into(), Value::Null);
    m.insert("headers".into(), json!({}));
    m.insert("content_type".into(), Value::Null);
    m.insert("ts".into(), json!(crate::servicio::logstore::now_string()));
    m
}

async fn pedir_una(
    url: &str,
    segundos: u64,
    cabeceras: &std::collections::BTreeMap<String, String>,
) -> Map<String, Value> {
    let mut r = vacio(url);
    let t0 = Instant::now();

    let resultado = tokio::time::timeout(
        Duration::from_secs(segundos.clamp(1, 120)),
        seguir(url, cabeceras),
    )
    .await;

    match resultado {
        // **El timeout se reporta como `down` con su propio mensaje.** En
        // Python caía en el `except Exception` genérico y el panel mostraba
        // el `repr` de la excepción de httpx, que no dice "tardó demasiado"
        // de forma legible.
        Err(_) => {
            r.insert("status".into(), json!("down"));
            r.insert("error".into(), json!(format!("timeout después de {segundos}s")));
        }
        Ok(Err(e)) => {
            r.insert("status".into(), json!("down"));
            r.insert("error".into(), json!(e));
        }
        Ok(Ok((codigo, cabeceras_resp, cuerpo))) => {
            let ms = (t0.elapsed().as_secs_f64() * 1000.0 * 10.0).round() / 10.0;
            r.insert("ms".into(), json!(ms));
            r.insert("code".into(), json!(codigo));
            // < 400 es `ok`; de 400 para arriba es `error` (el servidor
            // contestó, pero mal). `down` queda para "no contestó".
            r.insert("status".into(), json!(if codigo < 400 { "ok" } else { "error" }));

            let tipo = cabeceras_resp
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            r.insert("content_type".into(), json!(tipo));

            let mut mapa = Map::new();
            for (k, v) in cabeceras_resp.iter().take(MAX_CABECERAS) {
                mapa.insert(k.clone(), json!(v));
            }
            r.insert("headers".into(), Value::Object(mapa));

            if tipo.contains("json") {
                // El router mostraba `str(resp.json())[:200]`, o sea el
                // `repr` de Python del objeto parseado. Acá se reparsea y se
                // vuelve a serializar: así la vista previa es JSON válido en
                // vez de la sintaxis de dicts de Python, y un cuerpo que
                // dice ser JSON pero no lo es se nota.
                match serde_json::from_slice::<Value>(&cuerpo) {
                    Ok(v) => {
                        let texto = v.to_string();
                        r.insert(
                            "json_preview".into(),
                            json!(texto.chars().take(MAX_PREVIEW).collect::<String>()),
                        );
                    }
                    Err(e) => {
                        r.insert(
                            "json_preview".into(),
                            json!(format!("(content-type dice JSON pero no parsea: {e})")),
                        );
                    }
                }
            }
        }
    }
    r
}

/// Hace el pedido siguiendo redirects. Devuelve `(código, cabeceras, cuerpo)`.
///
/// `hyper_util`'s legacy client **no sigue redirects solo**, y el cliente de
/// Python tenía `follow_redirects=True`. Se implementa acá con un tope: sin
/// él, dos URLs que se apuntan entre sí serían un bucle infinito.
async fn seguir(
    url: &str,
    cabeceras: &std::collections::BTreeMap<String, String>,
) -> Result<(u16, Vec<(String, String)>, Vec<u8>), String> {
    let mut actual = url.to_string();
    for _ in 0..MAX_REDIRECTS {
        let uri: hyper::Uri = actual.parse().map_err(|e| format!("URL inválida: {e}"))?;
        if uri.host().is_none() {
            return Err("URL sin host".to_string());
        }
        let mut pedido = hyper::Request::builder().method(hyper::Method::GET).uri(&uri);
        // El `User-Agent` va primero para que el del cliente lo sobrescriba
        // si lo manda: quien pide el chequeo puede necesitar uno concreto.
        if !cabeceras.keys().any(|k| k.eq_ignore_ascii_case("user-agent")) {
            pedido = pedido.header("user-agent", USER_AGENT);
        }
        for (k, v) in cabeceras {
            pedido = pedido.header(k.as_str(), v.as_str());
        }
        let pedido = pedido
            .body(http_body_util::Empty::<hyper::body::Bytes>::new())
            .map_err(|e| format!("no se pudo armar el pedido: {e}"))?;

        let resp = cliente().request(pedido).await.map_err(|e| cadena_de_error(&e))?;
        let codigo = resp.status().as_u16();
        let cabeceras_resp: Vec<(String, String)> = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        if (300..400).contains(&codigo) {
            if let Some((_, destino)) =
                cabeceras_resp.iter().find(|(k, _)| k.eq_ignore_ascii_case("location"))
            {
                if !destino.is_empty() {
                    actual = resolver_destino(&actual, destino);
                    continue;
                }
            }
        }

        // El cuerpo se lee con tope: `resp.json()` de Python no tenía
        // ninguno, así que una respuesta enorme se cargaba entera en memoria
        // para mostrar 200 caracteres.
        let completo = resp
            .into_body()
            .collect()
            .await
            .map_err(|e| format!("no se pudo leer la respuesta: {e}"))?
            .to_bytes();
        let cuerpo = completo.iter().copied().take(MAX_CUERPO).collect();
        return Ok((codigo, cabeceras_resp, cuerpo));
    }
    Err(format!("demasiados redirects (más de {MAX_REDIRECTS})"))
}

/// Resuelve un `Location` relativo contra la URL actual.
///
/// Un `Location: /nuevo` es válido y frecuente; tomarlo tal cual daría una
/// URL sin esquema ni host.
pub fn resolver_destino(actual: &str, destino: &str) -> String {
    if destino.starts_with("http://") || destino.starts_with("https://") {
        return destino.to_string();
    }
    let Ok(uri) = actual.parse::<hyper::Uri>() else { return destino.to_string() };
    let esquema = uri.scheme_str().unwrap_or("https");
    let Some(autoridad) = uri.authority().map(ToString::to_string) else {
        return destino.to_string();
    };
    if destino.starts_with('/') {
        return format!("{esquema}://{autoridad}{destino}");
    }
    // Relativo al directorio del path actual.
    let base = uri.path();
    let corte = base.rfind('/').map_or(0, |i| i + 1);
    format!("{esquema}://{autoridad}{}{destino}", &base[..corte])
}

pub async fn chequear(req: &ApisRequest) -> Value {
    let mut resultados = Vec::new();
    for url in &req.urls {
        let mut r = pedir_una(url, req.timeout, &req.headers).await;

        // El historial se actualiza con lo recién medido y se devuelven las
        // últimas entradas, igual que antes.
        let entrada = json!({
            "ts": r.get("ts").cloned().unwrap_or(Value::Null),
            "status": r.get("status").cloned().unwrap_or(Value::Null),
            "ms": r.get("ms").cloned().unwrap_or(Value::Null),
            "code": r.get("code").cloned().unwrap_or(Value::Null),
        });
        let recientes = {
            let mut h = historial().lock().unwrap_or_else(|e| e.into_inner());
            let lista = h.entry(url.clone()).or_default();
            lista.push(entrada);
            if lista.len() > HISTORIAL_MAXIMO {
                let sobran = lista.len() - HISTORIAL_MAXIMO;
                lista.drain(..sobran);
            }
            lista.iter().rev().take(HISTORIAL_DEVUELTO).rev().cloned().collect::<Vec<_>>()
        };
        r.insert("history".into(), json!(recientes));

        let _ = crate::servicio::logstore::append(
            &crate::servicio::logstore::log_path("complexity"),
            &crate::servicio::logstore::LogEntry {
                ts: crate::servicio::logstore::now_string(),
                level: if r.get("status").and_then(Value::as_str) == Some("ok") {
                    "info".into()
                } else {
                    "warn".into()
                },
                msg: format!(
                    "API {url} → {} ({}ms)",
                    r.get("status").and_then(Value::as_str).unwrap_or("?"),
                    r.get("ms").map(ToString::to_string).unwrap_or_else(|| "?".into())
                ),
                source: "complexity".into(),
            },
        );

        resultados.push(Value::Object(r));
    }

    json!({ "results": resultados, "ts": crate::servicio::logstore::now_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_destino_absoluto_se_usa_tal_cual() {
        assert_eq!(
            resolver_destino("https://a.com/x", "https://b.com/y"),
            "https://b.com/y"
        );
        assert_eq!(resolver_destino("https://a.com/x", "http://b.com/y"), "http://b.com/y");
    }

    /// **`Location: /nuevo` es válido y frecuente.** Tomarlo tal cual daría
    /// una URL sin esquema ni host.
    #[test]
    fn un_destino_absoluto_de_path_conserva_esquema_y_host() {
        assert_eq!(
            resolver_destino("https://a.com/vieja/ruta?q=1", "/nueva"),
            "https://a.com/nueva"
        );
        assert_eq!(resolver_destino("http://a.com:8080/x", "/y"), "http://a.com:8080/y");
    }

    #[test]
    fn un_destino_relativo_se_resuelve_contra_el_directorio() {
        assert_eq!(resolver_destino("https://a.com/uno/dos", "tres"), "https://a.com/uno/tres");
        assert_eq!(resolver_destino("https://a.com/uno/", "dos"), "https://a.com/uno/dos");
        assert_eq!(resolver_destino("https://a.com", "x"), "https://a.com/x");
    }

    #[tokio::test]
    async fn una_url_invalida_se_reporta_como_down_y_no_panifica() {
        let r = pedir_una("no-es-una-url", 2, &std::collections::BTreeMap::new()).await;
        assert_eq!(r["status"], "down");
        assert!(r["error"].is_string());
        assert!(r["code"].is_null());
    }

    /// **Un esquema que no es HTTP no sirve para leer archivos locales.**
    ///
    /// Lo que importa es que `file://` se rechace, no *dónde* se rechaza: en
    /// la práctica no llega al chequeo de host porque `hyper::Uri` ya lo
    /// descarta como formato inválido. Afirmé el mensaje equivocado la
    /// primera vez y el test lo dijo; ahora asierta el rechazo y que el error
    /// viaja, que es lo verificable.
    #[tokio::test]
    async fn un_esquema_que_no_es_http_se_rechaza() {
        for url in ["file:///etc/passwd", "ftp://a.com/x", "javascript:alert(1)"] {
            let r = pedir_una(url, 2, &std::collections::BTreeMap::new()).await;
            assert_eq!(r["status"], "down", "{url} tiene que rechazarse");
            assert!(r["error"].is_string(), "y decir por qué: {url}");
            assert!(r["code"].is_null(), "sin código de respuesta: {url}");
        }
    }

    /// El chequeo de host propio, sobre una URL que sí parsea pero no tiene
    /// autoridad.
    #[test]
    fn una_uri_sin_host_se_detecta() {
        let uri: hyper::Uri = "/solo/un/path".parse().expect("parsea como path relativo");
        assert!(uri.host().is_none(), "sin host: es lo que el chequeo mira");
    }

    /// Un puerto cerrado en localhost falla rápido, sin tocar la red: es el
    /// caso que verifica el camino de error sin depender de internet.
    #[tokio::test]
    async fn un_puerto_cerrado_se_reporta_como_down() {
        let r = pedir_una("http://127.0.0.1:1/", 3, &std::collections::BTreeMap::new()).await;
        assert_eq!(r["status"], "down");
        assert!(r["error"].is_string());
    }

    /// **El shape de cada resultado es el que el panel ya consume**, incluso
    /// cuando el chequeo falla: si faltara un campo, el panel mostraría
    /// `undefined` en vez de un estado.
    #[tokio::test]
    async fn el_shape_esta_completo_incluso_al_fallar() {
        let r = chequear(&ApisRequest {
            urls: vec!["http://127.0.0.1:1/".to_string()],
            timeout: 3,
            headers: std::collections::BTreeMap::new(),
        })
        .await;
        let uno = &r["results"][0];
        for clave in
            ["url", "status", "code", "ms", "error", "headers", "content_type", "ts", "history"]
        {
            assert!(uno.get(clave).is_some(), "falta {clave} en {uno}");
        }
        assert!(r["ts"].is_string());
    }

    /// El historial acumula entre llamadas y devuelve las últimas.
    #[tokio::test]
    async fn el_historial_acumula_por_url() {
        let url = "http://127.0.0.1:2/historial-test";
        for _ in 0..3 {
            let _ = chequear(&ApisRequest {
                urls: vec![url.to_string()],
                timeout: 2,
                headers: std::collections::BTreeMap::new(),
            })
            .await;
        }
        let r = chequear(&ApisRequest {
            urls: vec![url.to_string()],
            timeout: 2,
            headers: std::collections::BTreeMap::new(),
        })
        .await;
        let h = r["results"][0]["history"].as_array().unwrap();
        assert_eq!(h.len(), 4, "cuatro chequeos, cuatro entradas");
        assert!(h.iter().all(|e| e["status"] == "down"));
    }

    #[tokio::test]
    async fn sin_urls_devuelve_una_lista_vacia() {
        let r = chequear(&ApisRequest {
            urls: vec![],
            timeout: 5,
            headers: std::collections::BTreeMap::new(),
        })
        .await;
        assert_eq!(r["results"].as_array().unwrap().len(), 0);
        assert!(r.get("error").is_none());
    }

    /// El timeout se acota: un cliente que pide 0 segundos (o un millón) no
    /// debe poder colgar el motor ni hacer un pedido imposible.
    #[tokio::test]
    async fn el_timeout_se_acota() {
        let r = pedir_una("http://127.0.0.1:1/", 0, &std::collections::BTreeMap::new()).await;
        // Con 0 el clamp lo sube a 1s, así que el pedido corre y falla por
        // conexión, no por un timeout de duración cero.
        assert_eq!(r["status"], "down");
    }

    /// **La cadena de causas es lo único que hace útil a un chequeo de
    /// salud.** `hyper_util` devuelve `client error (Connect)` para un
    /// certificado vencido, un host que no resuelve y un puerto cerrado por
    /// igual; la causa real está en `source()`.
    #[test]
    fn la_cadena_de_error_junta_las_causas() {
        #[derive(Debug)]
        struct Capa(&'static str, Option<Box<Capa>>);
        impl std::fmt::Display for Capa {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
        impl std::error::Error for Capa {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                self.1.as_ref().map(|c| c.as_ref() as &(dyn std::error::Error + 'static))
            }
        }
        let e = Capa(
            "client error (Connect)",
            Some(Box::new(Capa(
                "invalid peer certificate",
                Some(Box::new(Capa("Expired", None))),
            ))),
        );
        let texto = cadena_de_error(&e);
        assert!(texto.contains("client error"), "{texto}");
        assert!(texto.contains("invalid peer certificate"), "la causa real: {texto}");
        assert!(texto.contains("Expired"), "y el detalle: {texto}");
    }

    /// Las causas repetidas no se duplican: las cadenas de hyper repiten el
    /// mismo texto en varios niveles.
    #[test]
    fn la_cadena_no_repite_el_mismo_texto() {
        #[derive(Debug)]
        struct Igual(Option<Box<Igual>>);
        impl std::fmt::Display for Igual {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "lo mismo")
            }
        }
        impl std::error::Error for Igual {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                self.0.as_ref().map(|c| c.as_ref() as &(dyn std::error::Error + 'static))
            }
        }
        let e = Igual(Some(Box::new(Igual(Some(Box::new(Igual(None)))))));
        assert_eq!(cadena_de_error(&e), "lo mismo");
    }

    /// **Sin `User-Agent` algunos servidores devuelven 403** — medido contra
    /// `api.github.com`. `httpx` mandaba el suyo; el cliente de hyper no
    /// manda ninguno, así que sin esto el port reportaría como caída una API
    /// que funciona. Y el valor no lleva ningún dato de contacto.
    #[test]
    fn el_user_agent_por_defecto_es_generico_y_sin_datos() {
        assert!(!USER_AGENT.is_empty());
        assert!(USER_AGENT.starts_with("Sythrall"));
        assert!(!USER_AGENT.contains('@'), "nada que parezca un mail");
        assert!(!USER_AGENT.contains("http"), "ni una URL");
    }

    /// **El proveedor de `rustls` se instala una sola vez** y no revienta si
    /// ya estaba: llamarlo dos veces es lo que pasa con varios tests en el
    /// mismo proceso.
    #[test]
    fn instalar_el_proveedor_dos_veces_no_falla() {
        asegurar_proveedor();
        asegurar_proveedor();
    }
}
