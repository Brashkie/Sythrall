//! **Sistema y autenticación — Fase 35, paso 6: lo último de Python.**
//!
//! `/health`, `/capabilities` y el emisor de tokens de la terminal, que
//! vivían en `main.py` y `routers/auth.py`.
//!
//! **Y `/capabilities` cambió de significado, porque el anterior dejó de
//! existir.** Reportaba qué librerías de Python estaban instaladas en el
//! backend: `HAS_FLAKE8`, `HAS_PYLINT`, `HAS_TORCH`, `HAS_TENSORFLOW`,
//! `HAS_POLARS`, con sus versiones. Eso era información **del intérprete que
//! servía la API**, no del producto — y ese intérprete ya no está. Mantener
//! los campos devolviendo `false` sería peor que sacarlos: diría que el
//! producto perdió capacidades, cuando lo que pasó es que la pregunta dejó de
//! tener sentido.
//!
//! Ahora reporta **lo que el motor sabe hacer**: los lenguajes que analiza
//! (del manifest de plugins, la fuente de verdad desde la Fase 24) y los
//! toolchains externos que encuentra (del `toolchain`, el mismo que resuelve
//! `gcc`/`gnatprove`/`dmd`). Eso sí es una capacidad del producto, y se mide
//! en vez de declararse.

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

/// La versión que el cliente muestra. Sale del `Cargo.toml` en compilación,
/// así que no puede quedar desincronizada a mano.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Programas externos que el motor usa si están, y de los que informa.
///
/// **Ninguno es obligatorio**: cada uno habilita una función concreta y su
/// ausencia la saltea en vez de romper nada. Es la misma lista que
/// `costo::toolchain` ya resuelve, expuesta para que el cliente pueda decir
/// qué está disponible.
pub const PROGRAMAS: &[(&str, &str)] = &[
    ("gcc", "compilar C/C++ a ensamblador y medir el frente de Pareto"),
    ("gnat-gcc", "compilar Ada a código máquina"),
    ("gnatprove", "demostrar contratos SPARK"),
    ("dmd", "medir el costo de metaprogramación de D"),
    ("zig", "compilar los kernels de validación empírica"),
    ("gfortran", "compilar los kernels de Fortran"),
];

pub fn salud() -> Value {
    json!({
        "status": "ok",
        "ts": crate::servicio::logstore::now_string(),
        "capabilities": capacidades_del_motor(),
    })
}

/// Las capacidades **medidas**, no declaradas.
fn capacidades_del_motor() -> Value {
    let mut m = Map::new();
    // El análisis vive en este proceso: preguntarse si "está disponible" no
    // tiene sentido. Se deja el campo porque el cliente lo lee.
    m.insert("complexity".into(), json!(true));
    for (programa, _) in PROGRAMAS {
        m.insert((*programa).to_string(), json!(crate::costo::toolchain::disponible(programa)));
    }
    Value::Object(m)
}

pub fn capacidades() -> Value {
    let mut lenguajes: Vec<Value> = crate::servicio::plugin::builtin_manifests()
        .iter()
        .filter(|p| matches!(p.category, crate::servicio::plugin::PluginCategory::Language))
        .map(|p| json!({ "id": p.id, "parser": p.parser, "features": p.features.len() }))
        .collect();
    lenguajes.sort_by_key(|l| l["id"].as_str().unwrap_or("").to_string());

    let mut toolchains = Map::new();
    for (programa, para_que) in PROGRAMAS {
        let ruta = crate::costo::toolchain::resolver(programa);
        toolchains.insert(
            (*programa).to_string(),
            json!({
                "available": ruta.is_some(),
                "purpose": para_que,
                // La variable de entorno con la que se puede apuntar a mano,
                // para que el cliente sepa qué setear si falta.
                "env": crate::costo::toolchain::nombre_de_variable(programa),
            }),
        );
    }

    json!({
        "server": format!("Sythrall v{VERSION}"),
        "engine": "complexity-engine (Rust)",
        "ts": crate::servicio::logstore::now_string(),
        "languages": lenguajes,
        "toolchains": toolchains,
        // **Los flags `HAS_*` de librerías de Python ya no están.** Ver el
        // doc del módulo: describían el intérprete que servía la API, que ya
        // no existe. Este campo lo dice en vez de dejar al cliente adivinando
        // por qué desaparecieron.
        "note": "Las capacidades son del motor. Los flags HAS_* de librerías de Python se retiraron en la Fase 35: describían el intérprete del backend, que ya no existe.",
    })
}

// ── Token de la terminal ──────────────────────────────────────────────────

/// Cuánto vive un token, en segundos.
const TTL: u64 = 3600;
const SCOPE: &str = "terminal";

/// El secreto con el que se firman los tokens.
///
/// **Falla si no está, a propósito y como antes.** `routers/auth.py` hacía lo
/// mismo al importarse: un backend que arranca sin secreto emitiría tokens
/// que el sidecar de terminal no puede verificar, y el error aparecería
/// recién al abrir una terminal, lejos de la causa.
pub fn secreto() -> Result<String, String> {
    std::env::var("SYTHRALL_AUTH_SECRET").map_err(|_| {
        "SYTHRALL_AUTH_SECRET no está seteada. Corré `npm run dev` \
         (scripts/dev-banner.mjs la genera sola) o exportala a mano."
            .to_string()
    })
}

/// `true` si la dirección es de loopback.
///
/// **Mismo criterio que `is_loopback_request` de `services/terminal`**: el
/// **último** valor de `X-Forwarded-For` si existe —el que estampa Vite con
/// la IP real, que el cliente no puede falsificar— y si no, la IP del peer
/// directo. Tomar el primero sería el bug clásico: ese valor lo pone el
/// cliente y puede decir lo que quiera.
pub fn es_loopback(forwarded_for: Option<&str>, peer: Option<&str>) -> bool {
    if let Some(xff) = forwarded_for.filter(|s| !s.trim().is_empty()) {
        let ultimo = xff.rsplit(',').next().unwrap_or("").trim();
        return ip_es_loopback(ultimo);
    }
    peer.is_some_and(ip_es_loopback)
}

fn ip_es_loopback(texto: &str) -> bool {
    // Un peer puede venir como `127.0.0.1:54321`: se saca el puerto. Para
    // IPv6 el formato es `[::1]:54321`.
    let limpio = texto.trim();
    let sin_puerto = if let Some(resto) = limpio.strip_prefix('[') {
        resto.split(']').next().unwrap_or("")
    } else if limpio.matches(':').count() == 1 {
        limpio.split(':').next().unwrap_or("")
    } else {
        limpio
    };
    sin_puerto
        .parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

/// Firma un token de sesión para la terminal.
pub fn token_de_terminal() -> Result<Value, String> {
    let clave = secreto()?;
    let ahora =
        SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();

    #[derive(serde::Serialize)]
    struct Claims {
        sub: String,
        scope: String,
        iat: u64,
        exp: u64,
    }
    // `sub` es "local" porque todavía no hay login real. La cañería ya tiene
    // la forma que va a tener el día que lo haya: ese día cambia quién se
    // firma acá, no cómo lo verifica el sidecar.
    let claims =
        Claims { sub: "local".into(), scope: SCOPE.into(), iat: ahora, exp: ahora + TTL };

    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(clave.as_bytes()),
    )
    .map_err(|e| format!("no se pudo firmar el token: {e}"))?;

    Ok(json!({ "token": token, "expires_in": TTL }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_salud_declara_el_motor_en_proceso() {
        let s = salud();
        assert_eq!(s["status"], "ok");
        assert_eq!(s["capabilities"]["complexity"], true);
        assert!(s["ts"].as_str().unwrap().len() >= 19);
    }

    /// **Los flags de los toolchains se MIDEN**, no se declaran: cada uno
    /// dice si el programa está, y puede no estar.
    #[test]
    fn los_toolchains_se_reportan_como_booleanos() {
        let s = salud();
        for (programa, _) in PROGRAMAS {
            assert!(
                s["capabilities"][*programa].is_boolean(),
                "{programa} tiene que ser booleano"
            );
        }
    }

    #[test]
    fn las_capacidades_listan_los_lenguajes_del_manifest() {
        let c = capacidades();
        let ls = c["languages"].as_array().unwrap();
        assert!(ls.len() >= 13, "el motor analiza 13 lenguajes, hay {}", ls.len());
        let ids: Vec<&str> = ls.iter().map(|l| l["id"].as_str().unwrap()).collect();
        for esperado in ["python", "c", "cpp", "fortran", "ada", "d", "rust", "assembly"] {
            assert!(ids.contains(&esperado), "falta {esperado}");
        }
        // Ordenados, para que dos llamadas se comparen sin ruido.
        let mut ordenados = ids.clone();
        ordenados.sort_unstable();
        assert_eq!(ids, ordenados);
    }

    /// **Cada toolchain dice para qué sirve y qué variable lo apunta.** Un
    /// booleano suelto no le dice a nadie qué hacer si falta.
    #[test]
    fn cada_toolchain_explica_su_proposito_y_su_variable() {
        let c = capacidades();
        for (programa, _) in PROGRAMAS {
            let t = &c["toolchains"][*programa];
            assert!(t["available"].is_boolean());
            assert!(!t["purpose"].as_str().unwrap().is_empty(), "{programa} sin proposito");
            assert!(
                t["env"].as_str().unwrap().starts_with("SYTHRALL_TOOLCHAIN_"),
                "{programa}: {}",
                t["env"]
            );
        }
    }

    /// **Los flags `HAS_*` de Python no vuelven**, y la respuesta explica por
    /// qué en vez de dejar al cliente adivinando.
    #[test]
    fn no_hay_flags_de_librerias_de_python() {
        let c = capacidades();
        let texto = c.to_string();
        for viejo in ["HAS_FLAKE8", "HAS_PYLINT", "HAS_TORCH", "HAS_NUMPY", "python_version"] {
            assert!(!texto.contains(viejo), "{viejo} no deberia estar");
        }
        assert!(c["note"].as_str().unwrap().contains("Fase 35"));
    }

    #[test]
    fn la_version_sale_del_cargo_toml() {
        let c = capacidades();
        let s = c["server"].as_str().unwrap();
        assert!(s.starts_with("Sythrall v"), "{s}");
        assert!(s.len() > 10, "con una version de verdad: {s}");
    }

    // ── loopback ──────────────────────────────────────────────────────

    #[test]
    fn reconoce_las_direcciones_de_loopback() {
        assert!(es_loopback(None, Some("127.0.0.1")));
        assert!(es_loopback(None, Some("127.0.0.1:54321")));
        assert!(es_loopback(None, Some("::1")));
        assert!(es_loopback(None, Some("[::1]:54321")));
        assert!(es_loopback(None, Some("127.5.5.5")), "todo 127/8 es loopback");
    }

    #[test]
    fn rechaza_las_que_no_lo_son() {
        assert!(!es_loopback(None, Some("192.168.1.10")));
        assert!(!es_loopback(None, Some("8.8.8.8")));
        assert!(!es_loopback(None, Some("no-es-una-ip")));
        assert!(!es_loopback(None, None));
    }

    /// **El ÚLTIMO valor de `X-Forwarded-For`, no el primero.** El primero lo
    /// pone el cliente y puede decir lo que quiera; el último lo estampa el
    /// proxy con la IP real. Tomar el primero sería el bug clásico de este
    /// chequeo.
    #[test]
    fn usa_el_ultimo_valor_de_forwarded_for() {
        // Un atacante dice venir de loopback, pero el proxy estampa su IP
        // real al final: se rechaza.
        assert!(
            !es_loopback(Some("127.0.0.1, 203.0.113.9"), Some("127.0.0.1")),
            "el ultimo valor manda, y es publico"
        );
        // Y al revés: el cliente dice una IP pública pero el proxy local
        // estampa loopback — se acepta.
        assert!(es_loopback(Some("203.0.113.9, 127.0.0.1"), None));
    }

    /// Con `X-Forwarded-For` presente, el peer **no** se consulta: si se
    /// consultara, un proxy mal configurado dejaría pasar cualquier cosa.
    #[test]
    fn con_forwarded_for_el_peer_no_decide() {
        assert!(!es_loopback(Some("203.0.113.9"), Some("127.0.0.1")));
    }

    /// Un `X-Forwarded-For` vacío se ignora y se usa el peer.
    #[test]
    fn un_forwarded_for_vacio_cae_al_peer() {
        assert!(es_loopback(Some("   "), Some("127.0.0.1")));
        assert!(es_loopback(Some(""), Some("127.0.0.1")));
    }

    // ── token ─────────────────────────────────────────────────────────

    /// **Sin secreto no se firma nada**, y el error lo dice. Igual que antes:
    /// un token que el sidecar no puede verificar daría un error lejos de la
    /// causa.
    #[test]
    fn sin_secreto_el_token_falla_con_un_mensaje_util() {
        // No se toca la variable de entorno (los tests corren en paralelo):
        // se verifica el mensaje de `secreto()` cuando no está.
        let mensaje = "SYTHRALL_AUTH_SECRET no está seteada";
        if std::env::var("SYTHRALL_AUTH_SECRET").is_err() {
            let e = secreto().unwrap_err();
            assert!(e.contains(mensaje), "{e}");
            assert!(e.contains("npm run dev"), "y dice como arreglarlo: {e}");
        } else {
            // Si el entorno la tiene, se verifica el camino bueno.
            assert!(secreto().is_ok());
        }
    }

    #[test]
    fn el_token_tiene_tres_partes_y_el_ttl_declarado() {
        if secreto().is_err() {
            return; // sin secreto en el entorno, no hay nada que firmar
        }
        let t = token_de_terminal().expect("con secreto tiene que firmar");
        assert_eq!(t["expires_in"], TTL);
        let token = t["token"].as_str().unwrap();
        assert_eq!(token.split('.').count(), 3, "un JWT son tres partes: {token}");
    }

    /// El token que se firma lo tiene que poder verificar el sidecar de
    /// terminal, con el mismo secreto y el mismo algoritmo.
    #[test]
    fn el_token_se_verifica_con_el_mismo_secreto() {
        let Ok(clave) = secreto() else { return };
        let t = token_de_terminal().expect("firma");
        let token = t["token"].as_str().unwrap();

        #[derive(serde::Deserialize)]
        struct Claims {
            sub: String,
            scope: String,
        }
        let mut val = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
        val.set_required_spec_claims(&["exp"]);
        let datos = jsonwebtoken::decode::<Claims>(
            token,
            &jsonwebtoken::DecodingKey::from_secret(clave.as_bytes()),
            &val,
        )
        .expect("el token propio tiene que verificar");
        assert_eq!(datos.claims.sub, "local");
        assert_eq!(datos.claims.scope, SCOPE);
    }

    /// Y **no** se verifica con otro secreto: es lo que hace que el token
    /// signifique algo.
    #[test]
    fn el_token_no_se_verifica_con_otro_secreto() {
        if secreto().is_err() {
            return;
        }
        let t = token_de_terminal().expect("firma");
        let token = t["token"].as_str().unwrap();
        let r = jsonwebtoken::decode::<serde_json::Value>(
            token,
            &jsonwebtoken::DecodingKey::from_secret(b"otro-secreto-cualquiera"),
            &jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256),
        );
        assert!(r.is_err(), "con otra clave no puede verificar");
    }
}
