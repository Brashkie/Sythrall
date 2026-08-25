use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::Deserialize;

/// Claims de un token de sesión de terminal — emitido por `apps/api`
/// (`routers/auth.py`), verificado acá. `scope` es defensa en profundidad:
/// si este mismo secreto compartido se reusa para otro tipo de token más
/// adelante (ej. una sesión de usuario "de verdad"), ninguno de los dos
/// puede hacerse pasar por el otro — `jsonwebtoken` no valida claims custom
/// por sí solo, así que `scope` se chequea a mano después de decodificar.
#[derive(Deserialize)]
pub struct Claims {
    pub sub: String,
    pub scope: String,
    #[allow(dead_code)] // se valida automático por `jsonwebtoken` (expiración), no se lee a mano
    pub exp: usize,
    #[allow(dead_code)] // no se usa todavía — queda para auditoría/logging futuro
    pub iat: usize,
}

const REQUIRED_SCOPE: &str = "terminal";

/// Verifica firma (HS256) + expiración + `scope` de un token de sesión.
/// `apps/api` es el único emisor hoy (un usuario implícito `"local"`, sin
/// login real todavía) — acá no importa quién lo emitió, solo que la firma
/// con el secreto compartido y la expiración sean válidas. El día que haya
/// cuentas reales, `apps/api` emite tokens tras un login de verdad y esta
/// función no cambia una sola línea.
pub fn verify_terminal_token(secret: &[u8], token: &str) -> Option<Claims> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_required_spec_claims(&["exp", "sub"]);
    let data = decode::<Claims>(token, &DecodingKey::from_secret(secret), &validation).ok()?;
    if data.claims.scope != REQUIRED_SCOPE {
        return None;
    }
    Some(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use std::time::{SystemTime, UNIX_EPOCH};

    const SECRET: &[u8] = b"test-secret-not-for-real-use";

    fn now() -> usize {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as usize
    }

    fn sign(sub: &str, scope: &str, iat: usize, exp: usize) -> String {
        #[derive(serde::Serialize)]
        struct RawClaims<'a> {
            sub: &'a str,
            scope: &'a str,
            iat: usize,
            exp: usize,
        }
        encode(&Header::new(jsonwebtoken::Algorithm::HS256), &RawClaims { sub, scope, iat, exp }, &EncodingKey::from_secret(SECRET))
            .unwrap()
    }

    #[test]
    fn token_valido_es_aceptado() {
        let token = sign("local", "terminal", now(), now() + 3600);
        let claims = verify_terminal_token(SECRET, &token);
        assert!(claims.is_some());
        assert_eq!(claims.unwrap().sub, "local");
    }

    #[test]
    fn token_expirado_es_rechazado() {
        let token = sign("local", "terminal", now() - 7200, now() - 3600);
        assert!(verify_terminal_token(SECRET, &token).is_none());
    }

    #[test]
    fn firma_alterada_es_rechazada() {
        let token = sign("local", "terminal", now(), now() + 3600);
        let wrong_secret = b"otro-secreto-completamente-distinto";
        assert!(verify_terminal_token(wrong_secret, &token).is_none());
    }

    #[test]
    fn scope_incorrecto_es_rechazado() {
        let token = sign("local", "otra-cosa", now(), now() + 3600);
        assert!(verify_terminal_token(SECRET, &token).is_none());
    }

    #[test]
    fn token_con_texto_basura_es_rechazado() {
        assert!(verify_terminal_token(SECRET, "no-soy-un-jwt").is_none());
    }
}
