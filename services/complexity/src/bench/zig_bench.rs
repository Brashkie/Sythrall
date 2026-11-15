//! Fase 26 (Algorithm Validation Engine) — generaliza `fortran_bench.rs`
//! (Fase 23) más allá de Fortran/matmul: mismo patrón (compilar y correr un
//! kernel fijo, medir tiempo real, ajustar el exponente de crecimiento
//! empírico), esta vez en Zig, validando la forma O(n²) de un bubble sort
//! en vez de la O(n³) de una multiplicación de matrices.
//!
//! Por qué Zig específicamente y no "porque sí": es la primera vez que
//! Sythrall ejecuta código propio en Zig — la actualización de filosofía de
//! lenguajes del 2026-08-30 lo abrió como candidato real de implementación,
//! y este módulo es la prueba de que compila y corre de verdad, mismo
//! criterio que `fortran_bench.rs` ya estableció para Fortran.
//!
//! Mismo límite de seguridad que `fortran_bench.rs`, textual: la superficie
//! de ejecución es el kernel fijo en `kernels/validate_bubble_sort.zig`,
//! escrito por Sythrall, NUNCA código de usuario. Sin sandboxing robusto
//! por la misma razón — no hay input no confiable en el camino de ejecución.

use crate::bench::benchsupport::{TempWorkDir, fit_exponent};
use std::fs;
use std::process::Command;

use serde::Serialize;

/// Tamaños de `n` calibrados a mano en esta máquina de desarrollo (`zig
/// build-exe -O ReleaseFast`) — a diferencia de `CPU_TIME` en Fortran,
/// `std.time.Timer` de Zig es de alta resolución (QueryPerformanceCounter
/// en Windows), así que no hace falta un `n` tan grande para superar la
/// resolución del reloj — confirmado en vivo antes de fijar estas
/// constantes, no adivinado.
const KERNEL_SIZES: &[u32] = &[1000, 2000, 4000, 8000];

/// El kernel vive como un archivo `.zig` de verdad
/// (`kernels/validate_bubble_sort.zig`), traído con `include_str!` — mismo
/// criterio que `fortran_bench.rs` usa para su propio kernel.
const ZIG_KERNEL_SRC: &str = include_str!("kernels/validate_bubble_sort.zig");

#[derive(Serialize, Clone)]
pub struct EmpiricalMeasurement {
    pub n: u32,
    pub seconds: f64,
}

#[derive(Serialize)]
pub struct EmpiricalValidationResult {
    pub available: bool,
    pub predicted_big_o: String,
    pub measurements: Vec<EmpiricalMeasurement>,
    pub estimated_exponent: Option<f64>,
    pub note: String,
}

fn zig_available() -> bool {
    Command::new("zig").arg("version").output().is_ok_and(|o| o.status.success())
}

fn compile_and_run(sizes: &[u32]) -> EmpiricalValidationResult {
    const PREDICTED: &str = "O(n²)";
    const SHAPE_NOTE: &str = "Confirms that a bubble sort (two nested loops) really does scale as O(n²) in general — this runs a reference kernel Sythrall wrote itself in Zig, not the user's specific function, so it validates the SHAPE, not a particular implementation's performance.";

    let workdir = match TempWorkDir::new("zbench") {
        Ok(w) => w,
        Err(e) => {
            return EmpiricalValidationResult {
                available: false,
                predicted_big_o: PREDICTED.to_string(),
                measurements: vec![],
                estimated_exponent: None,
                note: format!("no se pudo crear el directorio temporal: {e}"),
            };
        }
    };

    let src_path = workdir.0.join("kernel.zig");
    let bin_path = workdir.0.join(if cfg!(windows) { "kernel.exe" } else { "kernel" });

    if fs::write(&src_path, ZIG_KERNEL_SRC).is_err() {
        return EmpiricalValidationResult {
            available: false,
            predicted_big_o: PREDICTED.to_string(),
            measurements: vec![],
            estimated_exponent: None,
            note: "no se pudo escribir el archivo fuente temporal".to_string(),
        };
    }

    // `-femit-bin` necesita el path pegado con `=` en un solo argumento —
    // separado en dos (`.arg("-femit-bin").arg(path)`) zig lo interpreta
    // como flag booleano + un archivo fuente de más, y falla con
    // "unrecognized file extension" (bug real atrapado por el primer test
    // end-to-end antes de shippear, no adivinado).
    let emit_bin_arg = format!("-femit-bin={}", bin_path.display());
    let compile = Command::new("zig").arg("build-exe").arg(&src_path).arg("-O").arg("ReleaseFast").arg(emit_bin_arg).current_dir(&workdir.0).output();
    match compile {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            return EmpiricalValidationResult {
                available: false,
                predicted_big_o: PREDICTED.to_string(),
                measurements: vec![],
                estimated_exponent: None,
                note: format!("zig build-exe falló al compilar el kernel: {}", String::from_utf8_lossy(&out.stderr)),
            };
        }
        Err(e) => {
            return EmpiricalValidationResult {
                available: false,
                predicted_big_o: PREDICTED.to_string(),
                measurements: vec![],
                estimated_exponent: None,
                note: format!("no se pudo invocar zig: {e}"),
            };
        }
    }

    // 3 corridas por tamaño, mínimo — mismo criterio de microbenchmarking
    // que `fortran_bench.rs` ya usa.
    const REPEATS: u32 = 3;
    let mut measurements = Vec::new();
    let mut points = Vec::new();
    for &n in sizes {
        let mut best: Option<f64> = None;
        for _ in 0..REPEATS {
            let Ok(out) = Command::new(&bin_path).arg(n.to_string()).output() else { continue };
            if !out.status.success() {
                continue;
            }
            let stdout = String::from_utf8_lossy(&out.stdout);
            let Some(seconds) = stdout.trim().parse::<f64>().ok() else { continue };
            if seconds <= 0.0 {
                continue;
            }
            best = Some(best.map_or(seconds, |b: f64| b.min(seconds)));
        }
        if let Some(seconds) = best {
            measurements.push(EmpiricalMeasurement { n, seconds });
            points.push((n as f64, seconds));
        }
    }

    let estimated_exponent = fit_exponent(&points);
    let note = match estimated_exponent {
        Some(exp) => format!("Predicted {PREDICTED} (exponent 2.0) vs. measured exponent ≈ {exp:.2}. {SHAPE_NOTE}"),
        None => "No se pudieron obtener suficientes mediciones válidas para ajustar un exponente (menos de 2 puntos utilizables).".to_string(),
    };

    EmpiricalValidationResult { available: true, predicted_big_o: PREDICTED.to_string(), measurements, estimated_exponent, note }
}

pub fn validate_bubble_sort_quadratic() -> EmpiricalValidationResult {
    if !zig_available() {
        return EmpiricalValidationResult {
            available: false,
            predicted_big_o: "O(n²)".to_string(),
            measurements: vec![],
            estimated_exponent: None,
            note: "zig no está instalado en este entorno — no se puede compilar/correr Zig real para validar empíricamente.".to_string(),
        };
    }
    compile_and_run(KERNEL_SIZES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_exponent_con_menos_de_2_puntos_es_none() {
        assert!(fit_exponent(&[]).is_none());
        assert!(fit_exponent(&[(100.0, 0.01)]).is_none());
    }

    #[test]
    fn fit_exponent_detecta_cuadratico_exacto() {
        let sizes: [f64; 4] = [100.0, 200.0, 300.0, 400.0];
        let points: Vec<(f64, f64)> = sizes.iter().map(|&n| (n, (n / 100.0).powi(2) * 0.01)).collect();
        let exp = fit_exponent(&points).unwrap();
        assert!((exp - 2.0).abs() < 0.01, "exponente esperado ~2.0, dio {exp}");
    }

    #[test]
    fn zig_available_no_panickea() {
        let _ = zig_available();
    }

    #[test]
    fn validate_bubble_sort_end_to_end_si_zig_esta_disponible() {
        if !zig_available() {
            eprintln!("zig no disponible en este entorno — test salteado");
            return;
        }
        let result = validate_bubble_sort_quadratic();
        assert!(result.available);
        assert!(result.measurements.len() >= 2, "esperaba al menos 2 mediciones válidas");
        let exp = result.estimated_exponent.expect("con >=2 mediciones debería poder ajustar un exponente");
        // Rango generoso a propósito, mismo criterio que fortran_bench.rs:
        // atrapar una implementación rota, no exigir precisión de laboratorio.
        assert!((1.3..3.0).contains(&exp), "exponente medido fuera de rango razonable: {exp}");
    }

    #[test]
    fn validate_bubble_sort_degrada_con_gracia_sin_zig() {
        if zig_available() {
            eprintln!("zig SÍ está disponible en este entorno — el camino de degradación se prueba indirectamente arriba");
            return;
        }
        let result = validate_bubble_sort_quadratic();
        assert!(!result.available);
        assert!(result.measurements.is_empty());
        assert!(result.estimated_exponent.is_none());
    }
}
