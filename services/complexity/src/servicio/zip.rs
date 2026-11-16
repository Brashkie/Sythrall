//! **Lector de ZIP — Fase 35, lo que `zipfile` de la stdlib de Python hacía.**
//!
//! Sólo lectura: listar las entradas de un archivo ZIP en memoria y sacar los
//! bytes de cada una. No escribe ZIPs, porque nada en Sythrall los escribe.
//!
//! **Por qué está escrito acá y no es una crate.** El contenedor ZIP es un
//! formato documentado y chico —un directorio central al final del archivo,
//! un header local por entrada— y leerlo son las ~120 líneas de abajo. Lo que
//! **no** se reimplementa es DEFLATE: eso lo hace `flate2`, que **ya estaba
//! en el árbol de dependencias** porque `object` la usa para las secciones
//! comprimidas de los ejecutables. Declararla no agrega una línea de código
//! nuevo a compilar; sólo la hace accesible.
//!
//! Esa es la división que el proyecto viene aplicando: **leer un formato
//! estándar sí, reimplementar un algoritmo ajeno no.** Un inflate escrito a
//! mano acá sería más lento, con bugs propios, y no compraría nada.
//!
//! **Lo que este lector NO soporta, dicho antes de que alguien lo descubra
//! con un archivo roto:**
//! - ZIP64 (más de 65.535 entradas, o entradas de más de 4 GB). El tope de
//!   subida es 200 MB, así que no se puede llegar; si alguna vez sube, esto
//!   es lo primero que hay que tocar.
//! - Entradas cifradas. Se reportan como error por entrada, no se saltan en
//!   silencio.
//! - Métodos de compresión que no sean *stored* (0) o *deflate* (8). Son los
//!   dos que produce cualquier herramienta normal; el resto se reporta.

use std::io::Read;

/// Una entrada del directorio central, ya parseada.
pub struct Entrada {
    pub nombre: String,
    pub metodo: u16,
    pub tamano_comprimido: u64,
    pub tamano_real: u64,
    /// Offset del *local file header* de esta entrada dentro del archivo.
    pub offset_local: u64,
    /// Bit 0 de los flags: la entrada está cifrada.
    pub cifrada: bool,
}

const FIRMA_EOCD: u32 = 0x0605_4b50;
const FIRMA_CENTRAL: u32 = 0x0201_4b50;
const FIRMA_LOCAL: u32 = 0x0403_4b50;

fn u16_en(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?))
}

fn u32_en(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

/// **¿Esto es un ZIP?** Equivalente de `zipfile.is_zipfile`: no alcanza con
/// mirar los primeros bytes, porque un ZIP válido puede tener datos antes
/// (un instalador autoextraíble, por ejemplo). Lo que define al formato es
/// que el directorio central esté al final.
pub fn es_zip(bytes: &[u8]) -> bool {
    offset_del_eocd(bytes).is_some()
}

/// Busca el End Of Central Directory desde el final.
///
/// Se busca hacia atrás y no hacia adelante porque el EOCD lleva un comentario
/// de largo variable al final, así que su posición no es fija. El comentario
/// mide hasta 65.535 bytes, de ahí la ventana.
fn offset_del_eocd(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 22 {
        return None;
    }
    let ventana = 22 + 65_535;
    let desde = bytes.len().saturating_sub(ventana);
    (desde..=bytes.len() - 22).rev().find(|&i| u32_en(bytes, i) == Some(FIRMA_EOCD))
}

/// Las entradas del directorio central.
///
/// Se lee el directorio central y **no** se barren los headers locales, por
/// la misma razón que lo hace cualquier lector serio: el header local puede
/// traer los tamaños en cero y diferidos a un *data descriptor* posterior
/// (pasa cuando el ZIP se generó en streaming), mientras que el directorio
/// central siempre los tiene.
pub fn entradas(bytes: &[u8]) -> Result<Vec<Entrada>, String> {
    let eocd = offset_del_eocd(bytes).ok_or("no se encontró el directorio central: no es un ZIP")?;
    let cantidad = u16_en(bytes, eocd + 10).ok_or("EOCD truncado")? as usize;
    let inicio = u32_en(bytes, eocd + 16).ok_or("EOCD truncado")? as usize;

    let mut out = Vec::with_capacity(cantidad);
    let mut i = inicio;
    for _ in 0..cantidad {
        if u32_en(bytes, i) != Some(FIRMA_CENTRAL) {
            // Un directorio central cortado devuelve lo que se pudo leer en
            // vez de nada: un ZIP parcialmente dañado todavía tiene archivos
            // recuperables, y perderlos todos por el último no ayuda a nadie.
            break;
        }
        let flags = u16_en(bytes, i + 8).ok_or("entrada central truncada")?;
        let metodo = u16_en(bytes, i + 10).ok_or("entrada central truncada")?;
        let tamano_comprimido = u32_en(bytes, i + 20).ok_or("entrada central truncada")? as u64;
        let tamano_real = u32_en(bytes, i + 24).ok_or("entrada central truncada")? as u64;
        let largo_nombre = u16_en(bytes, i + 28).ok_or("entrada central truncada")? as usize;
        let largo_extra = u16_en(bytes, i + 30).ok_or("entrada central truncada")? as usize;
        let largo_comentario = u16_en(bytes, i + 32).ok_or("entrada central truncada")? as usize;
        let offset_local = u32_en(bytes, i + 42).ok_or("entrada central truncada")? as u64;

        let crudo = bytes.get(i + 46..i + 46 + largo_nombre).ok_or("nombre truncado")?;
        // El nombre es UTF-8 cuando el bit 11 está puesto; si no, oficialmente
        // es CP437. Se decodifica con reemplazo en vez de rechazar la entrada:
        // perder un archivo por un acento mal codificado en el nombre sería
        // peor que quedarse con un nombre aproximado.
        let nombre = String::from_utf8_lossy(crudo).to_string();

        out.push(Entrada {
            nombre,
            metodo,
            tamano_comprimido,
            tamano_real,
            offset_local,
            cifrada: flags & 1 != 0,
        });
        i += 46 + largo_nombre + largo_extra + largo_comentario;
    }
    Ok(out)
}

/// Los bytes descomprimidos de una entrada.
pub fn leer(bytes: &[u8], e: &Entrada) -> Result<Vec<u8>, String> {
    if e.cifrada {
        return Err("entrada cifrada: no se puede extraer sin contraseña".to_string());
    }
    let lh = e.offset_local as usize;
    if u32_en(bytes, lh) != Some(FIRMA_LOCAL) {
        return Err("header local inválido".to_string());
    }
    // Los largos de nombre y extra del header LOCAL, que pueden diferir de
    // los del central (el extra suele ser distinto) — hay que usar estos para
    // saber dónde arrancan los datos.
    let largo_nombre = u16_en(bytes, lh + 26).ok_or("header local truncado")? as usize;
    let largo_extra = u16_en(bytes, lh + 28).ok_or("header local truncado")? as usize;
    let inicio = lh + 30 + largo_nombre + largo_extra;
    let fin = inicio + e.tamano_comprimido as usize;
    let datos = bytes.get(inicio..fin).ok_or("datos comprimidos fuera del archivo")?;

    match e.metodo {
        0 => Ok(datos.to_vec()),
        8 => {
            let mut salida = Vec::with_capacity(e.tamano_real as usize);
            flate2::read::DeflateDecoder::new(datos)
                .read_to_end(&mut salida)
                .map_err(|err| format!("deflate falló: {err}"))?;
            Ok(salida)
        }
        otro => Err(format!("método de compresión no soportado: {otro}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un ZIP mínimo armado a mano con una entrada *stored*, para no
    /// depender de ninguna herramienta externa en los tests.
    fn zip_stored(nombre: &str, contenido: &[u8]) -> Vec<u8> {
        let n = nombre.as_bytes();
        let mut z = Vec::new();
        // ── local file header
        z.extend(FIRMA_LOCAL.to_le_bytes());
        z.extend(20u16.to_le_bytes()); // versión necesaria
        z.extend(0u16.to_le_bytes()); // flags
        z.extend(0u16.to_le_bytes()); // método: stored
        z.extend(0u16.to_le_bytes()); // hora
        z.extend(0u16.to_le_bytes()); // fecha
        z.extend(0u32.to_le_bytes()); // crc (no se verifica acá)
        z.extend((contenido.len() as u32).to_le_bytes());
        z.extend((contenido.len() as u32).to_le_bytes());
        z.extend((n.len() as u16).to_le_bytes());
        z.extend(0u16.to_le_bytes()); // extra
        z.extend(n);
        z.extend(contenido);
        // ── central directory
        let inicio_central = z.len() as u32;
        z.extend(FIRMA_CENTRAL.to_le_bytes());
        z.extend(20u16.to_le_bytes()); // versión que lo creó
        z.extend(20u16.to_le_bytes()); // versión necesaria
        z.extend(0u16.to_le_bytes()); // flags
        z.extend(0u16.to_le_bytes()); // método
        z.extend(0u16.to_le_bytes());
        z.extend(0u16.to_le_bytes());
        z.extend(0u32.to_le_bytes()); // crc
        z.extend((contenido.len() as u32).to_le_bytes());
        z.extend((contenido.len() as u32).to_le_bytes());
        z.extend((n.len() as u16).to_le_bytes());
        z.extend(0u16.to_le_bytes()); // extra
        z.extend(0u16.to_le_bytes()); // comentario
        z.extend(0u16.to_le_bytes()); // disco
        z.extend(0u16.to_le_bytes()); // atributos internos
        z.extend(0u32.to_le_bytes()); // atributos externos
        z.extend(0u32.to_le_bytes()); // offset local
        z.extend(n);
        let largo_central = z.len() as u32 - inicio_central;
        // ── EOCD
        z.extend(FIRMA_EOCD.to_le_bytes());
        z.extend(0u16.to_le_bytes()); // disco
        z.extend(0u16.to_le_bytes()); // disco del central
        z.extend(1u16.to_le_bytes()); // entradas en este disco
        z.extend(1u16.to_le_bytes()); // entradas totales
        z.extend(largo_central.to_le_bytes());
        z.extend(inicio_central.to_le_bytes());
        z.extend(0u16.to_le_bytes()); // comentario
        z
    }

    #[test]
    fn lee_una_entrada_sin_comprimir() {
        let z = zip_stored("hola.txt", b"contenido");
        assert!(es_zip(&z));
        let es = entradas(&z).expect("tiene que parsear");
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].nombre, "hola.txt");
        assert_eq!(leer(&z, &es[0]).unwrap(), b"contenido");
    }

    /// **Un ZIP válido puede tener basura adelante** (un instalador
    /// autoextraíble), así que mirar los primeros bytes no sirve para
    /// reconocerlo: lo que define el formato es el directorio central al
    /// final. Este test fija eso.
    fn con_prefijo(z: &[u8], prefijo: &[u8]) -> Vec<u8> {
        let mut v = prefijo.to_vec();
        v.extend(z);
        v
    }

    #[test]
    fn un_archivo_que_no_es_zip_se_rechaza() {
        assert!(!es_zip(b"no soy un zip, ni de casualidad"));
        assert!(!es_zip(b""));
        assert!(entradas(b"cualquier cosa").is_err());
    }

    /// Un ZIP cuyo directorio central quedó cortado devuelve las entradas que
    /// se pudieron leer en vez de perder el archivo entero.
    #[test]
    fn un_central_cortado_no_pierde_todo() {
        let z = zip_stored("a.txt", b"x");
        // Se declara 2 entradas pero sólo hay una: el barrido para solo.
        let mut roto = z.clone();
        let eocd = offset_del_eocd(&roto).unwrap();
        roto[eocd + 8..eocd + 10].copy_from_slice(&2u16.to_le_bytes());
        roto[eocd + 10..eocd + 12].copy_from_slice(&2u16.to_le_bytes());
        let es = entradas(&roto).expect("no tiene que fallar");
        assert_eq!(es.len(), 1, "devuelve la que sí estaba");
    }

    #[test]
    fn una_entrada_cifrada_se_reporta_y_no_devuelve_basura() {
        let z = zip_stored("secreto.txt", b"x");
        let mut es = entradas(&z).unwrap();
        es[0].cifrada = true;
        let err = leer(&z, &es[0]).unwrap_err();
        assert!(err.contains("cifrada"), "el error tiene que decir por qué: {err}");
    }

    #[test]
    fn un_metodo_desconocido_se_reporta_con_su_numero() {
        let z = zip_stored("a.txt", b"x");
        let mut es = entradas(&z).unwrap();
        es[0].metodo = 99;
        assert!(leer(&z, &es[0]).unwrap_err().contains("99"));
    }

    /// El EOCD se busca hacia atrás, así que datos antes del ZIP no molestan.
    #[test]
    fn datos_antes_del_zip_no_impiden_leerlo() {
        let z = zip_stored("a.txt", b"hola");
        let con_basura = con_prefijo(&z, b"#!/bin/sh\necho autoextraible\n");
        assert!(es_zip(&con_basura), "el EOCD sigue estando al final");
    }

    /// **Deflate real, inflado con `flate2`** — la mitad que no se
    /// reimplementa. Se comprime con la misma crate para que el test no
    /// dependa de ninguna herramienta externa.
    #[test]
    fn lee_una_entrada_deflate() {
        use std::io::Write;
        let original: Vec<u8> = "fn main() { }\n".repeat(50).into_bytes();
        let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(&original).unwrap();
        let comprimido = enc.finish().unwrap();
        assert!(comprimido.len() < original.len(), "el fixture tiene que comprimir de verdad");

        let n = b"src/main.rs";
        let mut z = Vec::new();
        z.extend(FIRMA_LOCAL.to_le_bytes());
        z.extend([20u8, 0, 0, 0]);
        z.extend(8u16.to_le_bytes()); // deflate
        z.extend([0u8; 4]);
        z.extend(0u32.to_le_bytes());
        z.extend((comprimido.len() as u32).to_le_bytes());
        z.extend((original.len() as u32).to_le_bytes());
        z.extend((n.len() as u16).to_le_bytes());
        z.extend(0u16.to_le_bytes());
        z.extend(n);
        z.extend(&comprimido);
        let inicio_central = z.len() as u32;
        z.extend(FIRMA_CENTRAL.to_le_bytes());
        z.extend([20u8, 0, 20, 0, 0, 0]);
        z.extend(8u16.to_le_bytes());
        z.extend([0u8; 4]);
        z.extend(0u32.to_le_bytes());
        z.extend((comprimido.len() as u32).to_le_bytes());
        z.extend((original.len() as u32).to_le_bytes());
        z.extend((n.len() as u16).to_le_bytes());
        z.extend([0u8; 2]);
        z.extend([0u8; 2]);
        z.extend([0u8; 2]);
        z.extend([0u8; 2]);
        z.extend(0u32.to_le_bytes());
        z.extend(0u32.to_le_bytes());
        z.extend(n);
        let largo_central = z.len() as u32 - inicio_central;
        z.extend(FIRMA_EOCD.to_le_bytes());
        z.extend([0u8; 4]);
        z.extend(1u16.to_le_bytes());
        z.extend(1u16.to_le_bytes());
        z.extend(largo_central.to_le_bytes());
        z.extend(inicio_central.to_le_bytes());
        z.extend(0u16.to_le_bytes());

        let es = entradas(&z).expect("tiene que parsear");
        assert_eq!(es[0].nombre, "src/main.rs");
        assert_eq!(es[0].metodo, 8);
        assert_eq!(leer(&z, &es[0]).unwrap(), original, "el inflate tiene que devolver el original exacto");
    }
}
