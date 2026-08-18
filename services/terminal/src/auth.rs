use rand::rngs::OsRng;
use rand::RngCore;
use subtle::ConstantTimeEq;

/// Token aleatorio criptográfico (32 bytes de un CSPRNG del SO, hex-encoded).
/// Se genera una vez por arranque del proceso y vive solo en memoria.
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Comparación en tiempo constante — evita timing attacks al validar el token
/// que llega por query param en cada intento de conexión WebSocket.
pub fn tokens_match(expected: &str, provided: &str) -> bool {
    let expected = expected.as_bytes();
    let provided = provided.as_bytes();
    if expected.len() != provided.len() {
        return false;
    }
    expected.ct_eq(provided).into()
}
