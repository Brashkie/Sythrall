//! **SPARK: la categoría de evidencia que el motor no tenía.**
//!
//! Fase 34, Línea E, paso 3 — y es la razón por la que Ada entró a la fase.
//!
//! `decidibilidad.rs` clasificaba los hallazgos en dos lados y decía, textual,
//! *"no hay un tercero: o se prueba leyendo, o no se puede"*. GNATprove
//! produce algo que no es ninguno de los dos: **una obligación que un
//! probador externo descargó**. No la probó Sythrall leyendo el código —la
//! probó Alt-Ergo o Z3 resolviendo una fórmula—, y meterla en *decidible*
//! sería adjudicarse una demostración ajena.
//!
//! **Y midiendo apareció que no es una categoría nueva sino dos.** La sonda
//! con la que se verificó la instalación devolvió las dos respuestas:
//!
//! ```text
//! Doble             → postcondition proved · overflow check proved
//! Doble_Sin_Guarda  → overflow check might fail, cannot prove lower bound
//! ```
//!
//! *No pude probarlo* tampoco es *aproximado*. Aproximado significa que el
//! análisis **renuncia** por un límite teórico —el problema de la parada, la
//! falta de información de runtime— y lo dice de antemano. Acá el problema sí
//! es demostrable en principio, y una herramienta de demostración **se quedó
//! sin camino**: con otro prover, más tiempo o una precondición más, podría
//! cerrarse. Son estados distintos y mezclarlos borraría justo la
//! información útil.
//!
//! **No se parsea prosa.** GNATprove escribe un `gnatprove.sarif` —formato
//! estándar de resultados de análisis— donde cada obligación trae su regla
//! (`VC_OVERFLOW_CHECK`, `VC_POSTCONDITION`, …) y un `kind` que vale `pass` o
//! `open`. Ese par es el contrato; el texto en inglés se usa solo para
//! mostrarlo, nunca para decidir.
//!
//! **Y el diseño de `decidibilidad.rs` no se rompe.** Ese módulo clasifica
//! por *tipo* de hallazgo, no por instancia, y eso se mantiene: el resultado
//! de cada obligación viaja en el **kind** (`spark_proved` / `spark_unproved`),
//! así que la tabla sigue siendo una función del tipo. Era la alternativa a
//! meterle una excepción por instancia a la pieza más simple del motor.

use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::costo::toolchain;

/// Lo que GNATprove llama una obligación de prueba (*verification
/// condition*), ya traducida al vocabulario del motor.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Obligacion {
    /// `spark_proved` o `spark_unproved`. Es el `kind` que `decidibilidad.rs`
    /// clasifica, y por eso el resultado va acá y no en un campo aparte.
    pub kind: &'static str,
    /// La regla de GNATprove: `VC_OVERFLOW_CHECK`, `VC_POSTCONDITION`,
    /// `SUBPROGRAM_TERMINATION`, …
    pub rule: String,
    pub file: String,
    pub line: usize,
    /// El texto de GNATprove, tal cual. Se muestra; no se interpreta.
    pub message: String,
}

#[derive(Serialize, Clone, Default)]
pub struct ResultadoSpark {
    pub available: bool,
    pub prover: Option<String>,
    pub proved: usize,
    /// **Las que el probador no pudo cerrar.** No son errores ni sospechas:
    /// son obligaciones abiertas.
    pub unproved: usize,
    pub obligations: Vec<Obligacion>,
    pub note: String,
}

pub const PROBADA: &str = "spark_proved";
pub const NO_PROBADA: &str = "spark_unproved";

const NOTA: &str = "GNATprove corre sobre este archivo y pide a un probador externo (Alt-Ergo, Z3) que descargue cada obligación. Lo que dice `probada` NO lo probó Sythrall leyendo el código: lo probó el prover resolviendo una fórmula, y por eso se reporta como una categoría propia. Lo que queda `sin probar` tampoco es un hallazgo: es una obligación que el prover no pudo cerrar con el tiempo y los axiomas que tenía — con una precondición más, otro prover o más tiempo podría cerrarse.";

/// Extrae las obligaciones de un informe SARIF de GNATprove.
///
/// Separado de la invocación **a propósito**: así los tests prueban la
/// traducción sin necesitar el toolchain instalado, que es lo que permite que
/// esta parte esté cubierta en cualquier máquina.
pub fn obligaciones_de_sarif(sarif: &str) -> Option<(Option<String>, Vec<Obligacion>)> {
    let v: serde_json::Value = serde_json::from_str(sarif).ok()?;
    let run = v.get("runs")?.as_array()?.first()?;
    let prover = run
        .get("tool")
        .and_then(|t| t.get("driver"))
        .and_then(|d| {
            let n = d.get("name").and_then(|x| x.as_str())?;
            let ver = d.get("version").and_then(|x| x.as_str()).unwrap_or("");
            Some(if ver.is_empty() { n.to_string() } else { format!("{n} {ver}") })
        });

    let mut out = Vec::new();
    for r in run.get("results").and_then(|x| x.as_array()).unwrap_or(&Vec::new()) {
        // **El par `kind`/`ruleId` es el contrato.** Un `kind` que no sea
        // `pass` ni `open` se ignora en vez de adivinarle un lado: GNATprove
        // puede agregar estados, y asumir que todo lo desconocido está
        // probado sería la peor forma posible de equivocarse.
        let kind = match r.get("kind").and_then(|x| x.as_str()) {
            Some("pass") => PROBADA,
            Some("open") => NO_PROBADA,
            _ => continue,
        };
        let rule = r.get("ruleId").and_then(|x| x.as_str()).unwrap_or("");
        // **No todo lo que GNATprove escribe es una obligacion de prueba.**
        // Medido contra el motor real: un aviso de `cannot unroll loop (too
        // many loop iterations)` llega con `kind: "pass"` y `ruleId: "error"`,
        // y contarlo como demostrada inflaba la superficie probada con un
        // mensaje que no prueba nada. Las obligaciones de verdad se nombran
        // en MAYUSCULAS (`VC_OVERFLOW_CHECK`, `SUBPROGRAM_TERMINATION`); los
        // diagnosticos genericos usan la palabra de severidad en minuscula.
        if !es_obligacion(rule) {
            continue;
        }
        let loc = r
            .get("locations")
            .and_then(|x| x.as_array())
            .and_then(|a| a.first())
            .and_then(|l| l.get("physicalLocation"));
        let file = loc
            .and_then(|l| l.get("artifactLocation"))
            .and_then(|a| a.get("uri"))
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string();
        let line = loc
            .and_then(|l| l.get("region"))
            .and_then(|g| g.get("startLine"))
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as usize;
        out.push(Obligacion {
            kind,
            rule: rule.to_string(),
            file,
            line,
            message: r
                .get("message")
                .and_then(|m| m.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string(),
        });
    }
    out.sort_by_key(|a| (a.file.clone(), a.line));
    Some((prover, out))
}

/// `true` si el `ruleId` nombra una obligacion de prueba y no un diagnostico
/// generico.
///
/// GNATprove nombra sus chequeos en MAYUSCULAS_CON_GUIONES y sus mensajes
/// sueltos con la palabra de severidad en minuscula (`error`, `warning`,
/// `info`). La regla es esa diferencia, no una lista de chequeos conocidos:
/// enumerarlos dejaria afuera cualquier chequeo que GNATprove agregue.
fn es_obligacion(rule: &str) -> bool {
    !rule.is_empty() && rule == rule.to_ascii_uppercase()
}

fn degradado(motivo: &str) -> ResultadoSpark {
    ResultadoSpark { available: false, note: motivo.to_string(), ..Default::default() }
}

/// El nombre de archivo que GNAT espera para una unidad, igual que en
/// `compilar.rs`: el del cuerpo o la especificación, en minúsculas, con los
/// puntos de una unidad hija vueltos guiones.
fn nombre_de_unidad(source: &str) -> String {
    crate::lenguajes::adaparse::parse_ada(source)
        .and_then(|r| {
            r.units
                .first()
                .map(|u| u.name.clone())
                .or_else(|| r.specs.first().map(|s| s.name.clone()))
                .or_else(|| r.functions.first().map(|f| f.name.clone()))
        })
        .filter(|n| !n.is_empty() && n != "<anonymous>")
        .map_or_else(|| "fuente".to_string(), |n| n.to_lowercase().replace('.', "-"))
}

/// La extensión que le corresponde al fuente: `.ads` si es sólo una
/// especificación (cero cuerpos), `.adb` si tiene alguno.
///
/// Darle `.adb` a una especificación hace que GNAT se queje de que falta el
/// cuerpo, y ese error se leería como un problema del código del usuario.
fn extension_de(source: &str) -> &'static str {
    let tiene_cuerpo = crate::lenguajes::adaparse::parse_ada(source)
        .is_some_and(|r| !r.functions.is_empty() || r.units.iter().any(|u| u.kind == "package_body"));
    if tiene_cuerpo { "adb" } else { "ads" }
}

/// Corre GNATprove sobre un fuente Ada y devuelve sus obligaciones.
pub fn verificar(source: &str) -> ResultadoSpark {
    let Some(gnatprove) = toolchain::resolver("gnatprove") else {
        return degradado(&format!(
            "GNATprove no está instalado. Si lo tenés en otra ruta, apuntá `{}` a su ejecutable.",
            toolchain::nombre_de_variable("gnatprove")
        ));
    };
    // **GNATprove necesita al compilador y a gprbuild en el PATH del hijo.**
    // No alcanza con resolver su propio ejecutable: por dentro invoca `gcc` y
    // `gprbuild` por nombre. Como los dos están instalados fuera del PATH, se
    // los agrega al entorno DEL PROCESO HIJO — nunca al del sistema.
    let (Some(gcc), Some(gprbuild)) = (toolchain::resolver("gnat-gcc"), toolchain::resolver("gprbuild"))
    else {
        return degradado("GNATprove está, pero falta el compilador de Ada o gprbuild que necesita por dentro.");
    };

    let Ok(dir) = crate::bench::benchsupport::TempWorkDir::new("spark") else {
        return degradado("no se pudo crear el directorio temporal");
    };
    let src = dir.0.join("src");
    if std::fs::create_dir_all(&src).is_err() {
        return degradado("no se pudo preparar el directorio de fuentes");
    }
    let archivo = src.join(format!("{}.{}", nombre_de_unidad(source), extension_de(source)));
    if std::fs::write(&archivo, source).is_err() {
        return degradado("no se pudo escribir el fuente");
    }
    let gpr = dir.0.join("sonda.gpr");
    if std::fs::write(
        &gpr,
        "project Sonda is\n   for Source_Dirs use (\"src\");\n   for Object_Dir use \"obj\";\nend Sonda;\n",
    )
    .is_err()
    {
        return degradado("no se pudo escribir el proyecto");
    }

    let mut cmd = Command::new(&gnatprove);
    cmd.arg("-P")
        .arg(&gpr)
        .arg("--level=1")
        .arg("--report=all")
        // Sin paralelismo: la salida es la misma y el costo de arrancar
        // procesos a esta escala domina el tiempo.
        .arg("-j1")
        .current_dir(&dir.0);
    if let Some(ruta) = path_con(&[&gcc, &gprbuild]) {
        cmd.env("PATH", ruta);
    }

    let salida = match cmd.output() {
        Ok(s) => s,
        Err(e) => return degradado(&format!("no se pudo ejecutar GNATprove: {e}")),
    };

    let sarif = dir.0.join("obj").join("gnatprove").join("gnatprove.sarif");
    let Ok(texto) = std::fs::read_to_string(&sarif) else {
        // **Que GNATprove falle y que no deje informe son cosas distintas, y
        // callarse la segunda dejaría un panel vacío sin motivo.**
        let err = String::from_utf8_lossy(&salida.stderr);
        let primera = err.lines().find(|l| !l.trim().is_empty()).unwrap_or("sin informe SARIF");
        return degradado(&format!("GNATprove no dejó informe: {primera}"));
    };

    let Some((prover, obligations)) = obligaciones_de_sarif(&texto) else {
        return degradado("el informe de GNATprove no se pudo leer");
    };

    let proved = obligations.iter().filter(|o| o.kind == PROBADA).count();
    let unproved = obligations.iter().filter(|o| o.kind == NO_PROBADA).count();
    ResultadoSpark { available: true, prover, proved, unproved, obligations, note: NOTA.to_string() }
}

/// El `PATH` del proceso hijo con las carpetas de estos ejecutables adelante.
fn path_con(ejecutables: &[&std::ffi::OsString]) -> Option<std::ffi::OsString> {
    let mut dirs: Vec<std::path::PathBuf> = ejecutables
        .iter()
        .filter_map(|e| Path::new(e.as_os_str()).parent().map(Path::to_path_buf))
        .collect();
    if dirs.is_empty() {
        return None;
    }
    dirs.dedup();
    let actual = std::env::var_os("PATH").unwrap_or_default();
    let mut partes: Vec<std::ffi::OsString> =
        dirs.into_iter().map(std::path::PathBuf::into_os_string).collect();
    partes.push(actual);
    std::env::join_paths(partes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un SARIF de GNATprove recortado a lo que importa: una obligación
    /// cerrada y una abierta.
    const SARIF: &str = r#"{
      "runs": [{
        "tool": { "driver": { "name": "GNATProve", "version": "FSF 16.1.0" } },
        "results": [
          { "kind": "pass", "level": "none", "ruleId": "VC_POSTCONDITION",
            "message": { "text": "postcondition proved" },
            "locations": [{ "physicalLocation": {
              "artifactLocation": { "uri": "caja.ads" }, "region": { "startLine": 7 } } }] },
          { "kind": "open", "level": "none", "ruleId": "VC_OVERFLOW_CHECK",
            "message": { "text": "overflow check might fail, cannot prove lower bound for 2 * X" },
            "locations": [{ "physicalLocation": {
              "artifactLocation": { "uri": "caja.adb" }, "region": { "startLine": 5 } } }] }
        ]
      }]
    }"#;

    #[test]
    fn el_par_pass_open_decide_el_kind_y_no_el_texto() {
        let (prover, obs) = obligaciones_de_sarif(SARIF).expect("SARIF ilegible");
        assert_eq!(prover.as_deref(), Some("GNATProve FSF 16.1.0"));
        assert_eq!(obs.len(), 2);
        let probada = obs.iter().find(|o| o.kind == PROBADA).expect("sin probada");
        assert_eq!(probada.rule, "VC_POSTCONDITION");
        assert_eq!((probada.file.as_str(), probada.line), ("caja.ads", 7));
        let abierta = obs.iter().find(|o| o.kind == NO_PROBADA).expect("sin abierta");
        assert_eq!(abierta.rule, "VC_OVERFLOW_CHECK");
        assert_eq!(abierta.line, 5);
    }

    /// **Un estado que GNATprove agregue mañana no se adivina.** Asumir que
    /// lo desconocido está probado sería la peor forma de equivocarse.
    #[test]
    fn un_kind_desconocido_se_ignora_en_vez_de_darse_por_probado() {
        let raro = SARIF.replace("\"kind\": \"pass\"", "\"kind\": \"algo_nuevo\"");
        let (_, obs) = obligaciones_de_sarif(&raro).unwrap();
        assert_eq!(obs.len(), 1, "solo deberia quedar la abierta");
        assert_eq!(obs[0].kind, NO_PROBADA);
    }

    /// **El caso real que engaño al modulo antes de corregirlo.** GNATprove
    /// emitio `kind: "pass"` con `ruleId: "error"` para un aviso de que no
    /// pudo desenrollar un bucle. Contarlo como demostrada inflaba la
    /// superficie probada con un mensaje que no prueba nada.
    #[test]
    fn un_aviso_generico_no_cuenta_como_obligacion_demostrada() {
        let con_aviso = r#"{
          "runs": [{
            "tool": { "driver": { "name": "GNATProve" } },
            "results": [
              { "kind": "pass", "level": "none", "ruleId": "error",
                "message": { "text": "cannot unroll loop (too many loop iterations)" },
                "locations": [{ "physicalLocation": {
                  "artifactLocation": { "uri": "a.adb" }, "region": { "startLine": 4 } } }] },
              { "kind": "open", "level": "none", "ruleId": "VC_OVERFLOW_CHECK",
                "message": { "text": "overflow check might fail" },
                "locations": [{ "physicalLocation": {
                  "artifactLocation": { "uri": "a.adb" }, "region": { "startLine": 5 } } }] }
            ]
          }]
        }"#;
        let (_, obs) = obligaciones_de_sarif(con_aviso).unwrap();
        assert_eq!(obs.len(), 1, "el aviso no es una obligacion: {obs:?}");
        assert_eq!(obs[0].kind, NO_PROBADA);
        assert_eq!(obs[0].rule, "VC_OVERFLOW_CHECK");
    }

    #[test]
    fn la_regla_distingue_chequeos_de_diagnosticos() {
        assert!(es_obligacion("VC_OVERFLOW_CHECK"));
        assert!(es_obligacion("SUBPROGRAM_TERMINATION"));
        assert!(!es_obligacion("error"));
        assert!(!es_obligacion("warning"));
        assert!(!es_obligacion(""));
    }

    #[test]
    fn un_sarif_que_no_es_json_no_revienta() {
        assert!(obligaciones_de_sarif("no soy json").is_none());
        assert!(obligaciones_de_sarif("{}").is_none());
    }

    /// El nombre del archivo sale de la unidad, igual que en `compilar.rs`.
    #[test]
    fn el_nombre_del_archivo_sale_de_la_unidad() {
        assert_eq!(nombre_de_unidad("package Caja is\nend Caja;\n"), "caja");
        assert_eq!(nombre_de_unidad("package Padre.Hijo is\nend Padre.Hijo;\n"), "padre-hijo");
        assert_eq!(nombre_de_unidad("-- nada\n"), "fuente");
    }

    /// **Una especificación va a `.ads` y un cuerpo a `.adb`.** Con la
    /// extensión equivocada GNAT se queja de que falta el cuerpo, y ese error
    /// se leería como un problema del código del usuario.
    #[test]
    fn la_extension_distingue_especificacion_de_cuerpo() {
        assert_eq!(extension_de("package Caja is\n   procedure P;\nend Caja;\n"), "ads");
        assert_eq!(
            extension_de("package body Caja is\n   procedure P is\n   begin\n      null;\n   end P;\nend Caja;\n"),
            "adb"
        );
    }

    /// Sin toolchain el módulo **dice qué falta y cómo apuntarlo**, en vez de
    /// devolver cero obligaciones como si el código estuviera probado.
    #[test]
    fn sin_toolchain_degrada_diciendo_que_falta() {
        let r = degradado("GNATprove no está instalado");
        assert!(!r.available);
        assert_eq!(r.proved, 0);
        assert_eq!(r.unproved, 0);
        assert!(r.obligations.is_empty());
        assert!(!r.note.is_empty());
    }
}
