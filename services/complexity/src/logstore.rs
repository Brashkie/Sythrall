//! Log persistente unificado (Python backend + ambos sidecars Rust) — CBOR
//! en disco, decodificado a texto real solo al servir/leer, nunca al
//! escribir. Un archivo por origen (`logs/{api,complexity,terminal}.cbor`)
//! evita cualquier coordinación entre los 2 procesos Rust separados que
//! podrían escribir concurrentemente al mismo archivo — cada uno es dueño
//! del suyo, `GET /log` (`main.rs`) es quien los lee y mezcla los 3.
//!
//! Formato: una secuencia CBOR (RFC 8742) — valores CBOR independientes
//! concatenados uno tras otro, sin un array que los envuelva. Esto es lo que
//! hace el append barato (no hay que reescribir un header de longitud) y la
//! lectura simple (decodificar en loop hasta EOF).

use std::fs::OpenOptions;
use std::io::{self, BufReader, Seek, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Serializa cada entrada a un buffer en memoria antes de escribirla, y
/// serializa las escrituras entre sí con este lock — encontrado en vivo,
/// no en teoría: con axum corriendo en el runtime multi-hilo de tokio,
/// varios `/log` POST concurrentes pueden llamar a `append()` al mismo
/// tiempo desde threads distintos del mismo proceso. `ciborium::into_writer`
/// escribiendo directo a un `File` hace VARIOS `write()` chicos por entrada
/// (uno por campo, no uno solo) — sin este lock, dos entradas concurrentes
/// pueden intercalar sus bytes a mitad de escritura, corrompiendo la
/// secuencia CBOR para siempre a partir de ese punto (`read_all` para en el
/// primer error de decode, silenciosamente). Serializar en un `Vec<u8>`
/// primero y hacer UN solo `write_all` reduce la ventana de la carrera;
/// el `Mutex` la cierra del todo entre threads del mismo proceso (no
/// protege contra 2 procesos distintos escribiendo el mismo archivo, pero
/// eso ya no puede pasar: solo un proceso puede bindear el puerto HTTP a la
/// vez).
static APPEND_LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LogEntry {
    pub ts: String,
    pub level: String,
    pub msg: String,
    pub source: String,
}

/// Timestamp legible (`YYYY-MM-DD HH:MM:SS`, UTC) para entradas que se
/// originan directamente en Rust (arranque de los sidecars, sesiones de
/// terminal) — no depende de una crate de fecha/hora externa, alcanza con
/// aritmética de calendario civil sobre `SystemTime` (algoritmo estándar de
/// Howard Hinnant, `days_from_civil` invertido) ya que solo hace falta un
/// timestamp legible ocasional, no manejo de zonas horarias.
pub fn now_string() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (hour, minute, second) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    // civil_from_days (Hinnant) — días desde epoch (1970-01-01) a (año, mes, día).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };

    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
}

/// Agrega una entrada al final del archivo — crea el directorio/archivo si
/// no existen. Los errores de I/O se ignoran en los call sites (logging
/// best-effort, nunca debe tumbar al proceso que lo llama).
pub fn append(path: &Path, entry: &LogEntry) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut buf = Vec::new();
    ciborium::into_writer(entry, &mut buf).map_err(|e| io::Error::other(e.to_string()))?;

    let _guard = APPEND_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(&buf)
}

/// Lee todas las entradas de un archivo — tolerante a que el archivo no
/// exista todavía (sidecar recién arrancado, nada logueado aún): devuelve
/// una lista vacía en vez de un error.
pub fn read_all(path: &Path) -> Vec<LogEntry> {
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let total_len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut reader = BufReader::new(file);
    let mut out = Vec::new();
    // Termina en EOF limpio o en la primera entrada corrupta/truncada — se
    // distinguen comparando la posición final contra el tamaño del archivo:
    // si terminó exactamente en el final, fue EOF limpio; si quedaron bytes
    // sin leer, algo a mitad de una entrada no decodificó, y eso se loguea
    // en vez de perderse en silencio (encontrado en vivo: antes esto
    // devolvía menos entradas de las esperadas sin ninguna señal de por qué).
    while let Ok(entry) = ciborium::from_reader::<LogEntry, _>(&mut reader) {
        out.push(entry);
    }
    if let Ok(pos) = reader.stream_position() {
        if pos < total_len {
            tracing::warn!(
                "logstore::read_all({}): entrada corrupta o truncada en el byte {pos} de {total_len} — se devuelven solo las {} entradas leídas antes de ese punto",
                path.display(),
                out.len(),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_y_read_all_hacen_roundtrip_de_multiples_entradas() {
        let dir = std::env::temp_dir().join(format!("sythrall_logstore_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.cbor");
        let _ = std::fs::remove_file(&path);

        let e1 = LogEntry { ts: "2026-01-01 00:00:00".into(), level: "info".into(), msg: "primero".into(), source: "test".into() };
        let e2 = LogEntry { ts: "2026-01-01 00:00:01".into(), level: "warn".into(), msg: "segundo".into(), source: "test".into() };

        append(&path, &e1).unwrap();
        append(&path, &e2).unwrap();

        let entries = read_all(&path);
        assert_eq!(entries, vec![e1, e2]);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn read_all_de_archivo_inexistente_da_vacio() {
        let path = std::env::temp_dir().join("sythrall_logstore_nunca_existio.cbor");
        let _ = std::fs::remove_file(&path);
        assert!(read_all(&path).is_empty());
    }

    #[test]
    fn read_all_con_bytes_truncados_al_final_devuelve_las_entradas_completas_sin_error() {
        // Regresión: antes, una entrada corrupta a mitad de escritura (ej.
        // el proceso murió mientras escribía, o el disco se corrompió)
        // hacía que `read_all` simplemente dejara de leer, sin ninguna señal
        // de que había pasado algo raro — ahora se compara la posición
        // final contra el tamaño del archivo (`tracing::warn!` si difieren),
        // pero el comportamiento observable (devolver lo leído hasta ese
        // punto, sin panic) tiene que seguir siendo el mismo.
        let dir = std::env::temp_dir().join(format!("sythrall_logstore_truncated_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("truncated.cbor");
        let _ = std::fs::remove_file(&path);

        let e1 = LogEntry { ts: "2026-01-01 00:00:00".into(), level: "info".into(), msg: "completa".into(), source: "test".into() };
        append(&path, &e1).unwrap();

        // Simula una segunda entrada cortada a mitad de escritura: se le
        // pega al archivo un puñado de bytes que arrancan una entrada CBOR
        // válida pero nunca la completan.
        let mut partial = Vec::new();
        ciborium::into_writer(
            &LogEntry { ts: "2026-01-01 00:00:01".into(), level: "warn".into(), msg: "esta nunca se completa".into(), source: "test".into() },
            &mut partial,
        )
        .unwrap();
        let cut = partial.len() / 2;
        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(&partial[..cut]).unwrap();
        drop(file);

        let entries = read_all(&path);
        assert_eq!(entries, vec![e1]);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn append_concurrente_desde_varios_threads_no_corrompe_el_archivo() {
        // Regresión del bug real: sin el lock/buffer, escrituras concurrentes
        // desde threads distintos (el caso real: varios `/log` POST al mismo
        // tiempo bajo el runtime multi-hilo de tokio) intercalaban bytes a
        // mitad de una entrada CBOR, corrompiendo la secuencia — `read_all`
        // paraba de leer en ese punto y perdía en silencio todo lo escrito
        // después. N threads × M entradas cada uno debe dar exactamente
        // N*M entradas leídas de vuelta, ninguna menos.
        let dir = std::env::temp_dir().join(format!("sythrall_logstore_concurrent_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("concurrent.cbor");
        let _ = std::fs::remove_file(&path);

        const THREADS: usize = 8;
        const PER_THREAD: usize = 50;

        std::thread::scope(|scope| {
            for t in 0..THREADS {
                let path = &path;
                scope.spawn(move || {
                    for i in 0..PER_THREAD {
                        let entry = LogEntry {
                            ts: now_string(),
                            level: "info".into(),
                            msg: format!("thread{t}-entry{i}"),
                            source: "test".into(),
                        };
                        append(path, &entry).unwrap();
                    }
                });
            }
        });

        let entries = read_all(&path);
        assert_eq!(entries.len(), THREADS * PER_THREAD);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn now_string_tiene_el_shape_esperado() {
        let s = now_string();
        assert_eq!(s.len(), 19);
        assert_eq!(s.as_bytes()[4], b'-');
        assert_eq!(s.as_bytes()[7], b'-');
        assert_eq!(s.as_bytes()[10], b' ');
        assert_eq!(s.as_bytes()[13], b':');
        assert_eq!(s.as_bytes()[16], b':');
    }
}
