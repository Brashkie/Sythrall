//! **Análisis de archivos de log — Fase 35, portado desde
//! `routers/analysis.py::analyze_logs`.**
//!
//! Clasifica cada línea de un log por palabras clave y devuelve los errores,
//! los avisos y un resumen por archivo. No parsea ningún lenguaje: es
//! clasificación de texto, y por eso es el endpoint más simple de
//! `analysis.py` y el primero que se mueve.
//!
//! **El criterio es deliberadamente tonto y eso está bien.** Busca
//! subcadenas (`ERROR`, `Traceback`, `WARNING`) sin entender el formato del
//! log. No hay dos formatos de log iguales, así que una heurística por
//! palabras funciona en todos a medias en vez de en uno bien. Lo que **no**
//! hace es pretender precisión: una línea que menciona la palabra "error"
//! dentro de un mensaje normal se cuenta como error, y eso es un falso
//! positivo conocido, no un bug.

use serde::Deserialize;
use serde_json::{json, Map, Value};

/// Las palabras que marcan cada nivel, en orden de severidad.
///
/// **El orden importa y es parte del contrato**: la primera categoría que
/// matchea gana y la línea no se vuelve a clasificar. Una línea
/// `CRITICAL: ... WARNING ...` cuenta como crítica, no como las dos. En
/// Python esto salía de iterar un `dict` con un `break`, que desde Python 3.7
/// conserva el orden de inserción — acá el orden es explícito en vez de
/// depender de esa garantía del lenguaje.
pub const PALABRAS: &[(&str, &[&str])] = &[
    ("critical", &["CRITICAL", "FATAL"]),
    ("error", &["ERROR", "Exception", "Traceback", "Error:"]),
    ("warning", &["WARNING", "WARN", "DEPRECATED"]),
    ("info", &["INFO"]),
];

/// Cuántos elementos entran en la respuesta antes de cortar.
///
/// Son topes de **transporte**, no de análisis: el resumen por archivo cuenta
/// todas las líneas, lo que se recorta es la lista que viaja al panel. Un log
/// de 200 MB con un error por línea no puede mandarse entero por HTTP.
const MAX_ERRORES: usize = 100;
const MAX_AVISOS: usize = 100;
/// Las líneas de error que se listan por archivo en el resumen.
const MAX_LINEAS_POR_ARCHIVO: usize = 20;
/// Cada línea se recorta para el panel.
const MAX_LARGO_DE_LINEA: usize = 200;

#[derive(Deserialize)]
pub struct ArchivoDeLog {
    #[serde(default = "nombre_por_defecto")]
    pub name: String,
    #[serde(default)]
    pub content: String,
}

fn nombre_por_defecto() -> String {
    "log.txt".to_string()
}

#[derive(Deserialize)]
pub struct LogsRequest {
    #[serde(default)]
    pub files: Vec<ArchivoDeLog>,
}

/// El nivel de una línea, o `None` si ninguna palabra matchea.
fn nivel_de(linea: &str) -> Option<&'static str> {
    PALABRAS
        .iter()
        .find(|(_, claves)| claves.iter().any(|k| linea.contains(k)))
        .map(|(nivel, _)| *nivel)
}

/// Recorta a `MAX_LARGO_DE_LINEA` **caracteres**, no bytes.
///
/// El `[:200]` de Python corta por caracteres porque opera sobre `str`. En
/// Rust cortar por byte en un log con acentos o con salida de un programa en
/// UTF-8 partiría un carácter al medio, y el JSON resultante sería inválido.
fn recortar(linea: &str) -> String {
    linea.trim().chars().take(MAX_LARGO_DE_LINEA).collect()
}

pub fn analizar(req: &LogsRequest) -> Value {
    let mut errores: Vec<Value> = Vec::new();
    let mut avisos: Vec<Value> = Vec::new();
    let mut resumen = Map::new();

    for archivo in &req.files {
        let mut conteo: std::collections::BTreeMap<&str, u64> =
            PALABRAS.iter().map(|(n, _)| (*n, 0)).collect();
        let mut lineas_con_error: Vec<usize> = Vec::new();
        let mut total = 0usize;

        for (indice, linea) in archivo.content.lines().enumerate() {
            total += 1;
            let numero = indice + 1;
            let Some(nivel) = nivel_de(linea) else { continue };
            *conteo.entry(nivel).or_insert(0) += 1;

            match nivel {
                "critical" | "error" => {
                    // El conteo del resumen es completo; lo que se recorta es
                    // la lista que viaja.
                    if errores.len() < MAX_ERRORES {
                        errores.push(json!({
                            "file": archivo.name,
                            "lineNo": numero,
                            "level": nivel,
                            "line": recortar(linea),
                        }));
                    }
                    lineas_con_error.push(numero);
                }
                "warning" if avisos.len() < MAX_AVISOS => {
                    avisos.push(json!({
                        "file": archivo.name,
                        "lineNo": numero,
                        "line": recortar(linea),
                    }));
                }
                _ => {}
            }
        }

        resumen.insert(
            archivo.name.clone(),
            json!({
                "total_lines": total,
                "counts": conteo,
                "error_lines": lineas_con_error.iter().take(MAX_LINEAS_POR_ARCHIVO).collect::<Vec<_>>(),
            }),
        );
    }

    json!({
        "errors": errores,
        "warnings": avisos,
        "summary": resumen,
        "ts": crate::servicio::logstore::now_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analizar_uno(nombre: &str, contenido: &str) -> Value {
        analizar(&LogsRequest {
            files: vec![ArchivoDeLog { name: nombre.to_string(), content: contenido.to_string() }],
        })
    }

    #[test]
    fn clasifica_cada_nivel() {
        let log = "2026-01-01 INFO arrancando\n2026-01-01 WARNING disco al 90%\n2026-01-01 ERROR no se pudo conectar\n2026-01-01 CRITICAL se cae todo\n";
        let r = analizar_uno("app.log", log);
        let c = &r["summary"]["app.log"]["counts"];
        assert_eq!(c["info"], 1);
        assert_eq!(c["warning"], 1);
        assert_eq!(c["error"], 1);
        assert_eq!(c["critical"], 1);
        assert_eq!(r["summary"]["app.log"]["total_lines"], 4);
        // Crítico y error van juntos a `errors`; warning va aparte.
        assert_eq!(r["errors"].as_array().unwrap().len(), 2);
        assert_eq!(r["warnings"].as_array().unwrap().len(), 1);
    }

    /// **La primera categoría que matchea gana.** Una línea con dos palabras
    /// de niveles distintos no se cuenta dos veces.
    #[test]
    fn una_linea_cuenta_en_un_solo_nivel() {
        let r = analizar_uno("a.log", "CRITICAL esto ademas tiene WARNING y ERROR adentro\n");
        let c = &r["summary"]["a.log"]["counts"];
        assert_eq!(c["critical"], 1);
        assert_eq!(c["error"], 0, "no se cuenta dos veces");
        assert_eq!(c["warning"], 0);
        assert_eq!(r["errors"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn reconoce_un_traceback_de_python() {
        let log = "Traceback (most recent call last):\n  File \"x.py\", line 1\nValueError: mal\n";
        let r = analizar_uno("err.log", log);
        // `Traceback` y `Error:` son dos líneas distintas que matchean.
        assert_eq!(r["summary"]["err.log"]["counts"]["error"], 2);
    }

    #[test]
    fn una_linea_sin_palabras_clave_no_se_clasifica() {
        let r = analizar_uno("a.log", "simplemente una linea normal\notra mas\n");
        let c = &r["summary"]["a.log"]["counts"];
        assert_eq!(c["info"], 0);
        assert_eq!(c["error"], 0);
        assert_eq!(r["summary"]["a.log"]["total_lines"], 2, "se cuentan igual en el total");
        assert_eq!(r["errors"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn el_numero_de_linea_arranca_en_uno() {
        let r = analizar_uno("a.log", "normal\nERROR aca\n");
        assert_eq!(r["errors"][0]["lineNo"], 2);
    }

    /// **Los topes son de transporte, no de análisis**: el conteo del resumen
    /// cuenta todo, la lista que viaja se recorta.
    #[test]
    fn los_topes_recortan_la_lista_pero_no_el_conteo() {
        let log = "ERROR falla\n".repeat(150);
        let r = analizar_uno("a.log", &log);
        assert_eq!(r["summary"]["a.log"]["counts"]["error"], 150, "el conteo es completo");
        assert_eq!(r["errors"].as_array().unwrap().len(), MAX_ERRORES, "la lista se recorta");
        assert_eq!(
            r["summary"]["a.log"]["error_lines"].as_array().unwrap().len(),
            MAX_LINEAS_POR_ARCHIVO
        );
    }

    /// **El recorte es por caracteres, no por bytes.** Cortar un log con
    /// acentos a 200 bytes partiría un carácter y dejaría JSON inválido.
    #[test]
    fn el_recorte_no_parte_un_caracter_al_medio() {
        let linea = format!("ERROR {}", "á".repeat(300));
        let r = analizar_uno("a.log", &format!("{linea}\n"));
        let recortada = r["errors"][0]["line"].as_str().unwrap();
        assert_eq!(recortada.chars().count(), MAX_LARGO_DE_LINEA);
        assert!(recortada.starts_with("ERROR "));
    }

    #[test]
    fn varios_archivos_se_resumen_por_separado_y_los_errores_se_juntan() {
        let r = analizar(&LogsRequest {
            files: vec![
                ArchivoDeLog { name: "uno.log".into(), content: "ERROR a\n".into() },
                ArchivoDeLog { name: "dos.log".into(), content: "ERROR b\nWARNING c\n".into() },
            ],
        });
        assert_eq!(r["summary"]["uno.log"]["counts"]["error"], 1);
        assert_eq!(r["summary"]["dos.log"]["counts"]["error"], 1);
        assert_eq!(r["errors"].as_array().unwrap().len(), 2, "juntos, con su `file`");
        let archivos: Vec<&str> =
            r["errors"].as_array().unwrap().iter().map(|e| e["file"].as_str().unwrap()).collect();
        assert_eq!(archivos, vec!["uno.log", "dos.log"]);
    }

    #[test]
    fn sin_archivos_devuelve_listas_vacias_y_no_un_error() {
        let r = analizar(&LogsRequest { files: vec![] });
        assert_eq!(r["errors"].as_array().unwrap().len(), 0);
        assert_eq!(r["summary"].as_object().unwrap().len(), 0);
        assert!(r.get("error").is_none());
    }

    #[test]
    fn un_archivo_vacio_se_resume_en_cero() {
        let r = analizar_uno("vacio.log", "");
        assert_eq!(r["summary"]["vacio.log"]["total_lines"], 0);
        assert_eq!(r["summary"]["vacio.log"]["counts"]["error"], 0);
    }

    /// La clasificación es sensible a mayúsculas, igual que en Python: busca
    /// `ERROR`, no `error`. Es un límite conocido del criterio por palabras.
    #[test]
    fn la_clasificacion_distingue_mayusculas() {
        assert_eq!(nivel_de("ERROR algo"), Some("error"));
        assert_eq!(nivel_de("error algo"), None, "minuscula no matchea, igual que antes");
        assert_eq!(nivel_de("Error: algo"), Some("error"), "pero `Error:` si esta en la lista");
    }
}
