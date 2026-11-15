//! Fase 26 (Algorithm Validation Engine) — séptimo kernel, tercer uso de
//! Zig en este motor (`zig_bench.rs`: bubble sort O(n²); `bfs_bench.rs`:
//! BFS O(V+E)). A diferencia de esos dos, este kernel no nace de "generalizar
//! el patrón a otra forma" — nace de la pieza 2 de "migrar de
//! numpy/pandas/scikit-learn" que el usuario pidió (2026-08-31): antes de
//! proponer un reemplazo nativo para un `groupby().agg()` de pandas, medir
//! HONESTAMENTE contra pandas real (ver
//! `routers/execution.py::validate_aggregation_vs_pandas`), mismo criterio
//! que la pieza 1 (matmul vs. numpy) ya estableció con `fortran_bench.rs`.
//!
//! Mismo límite de seguridad textual que el resto de este módulo: la
//! superficie de ejecución es el kernel fijo en
//! `kernels/validate_aggregation.zig`, escrito por Sythrall, NUNCA código de
//! usuario.

use crate::bench::benchsupport::{TempWorkDir, fit_exponent};
use std::fs;
use std::process::Command;

use serde::Serialize;

/// Calibrado en vivo en esta máquina de desarrollo (`zig build-exe -O
/// ReleaseFast`), no adivinado: n=2M/4M/8M/16M midieron 2.12/4.19/8.57/17.24
/// ms — un doblado limpio en cada paso. Un rango más ancho (probado hasta
/// 40M) empieza a mostrar exponente >1 por efectos de caché (dos arreglos de
/// ~9 bytes/elemento superan la caché L2/L3 de esta máquina) — deliberadamente
/// evitado acá porque el objetivo es validar la FORMA del algoritmo (un
/// recorrido lineal), no medir el comportamiento de la jerarquía de memoria.
const KERNEL_SIZES: &[u32] = &[2_000_000, 4_000_000, 8_000_000, 16_000_000];

const ZIG_KERNEL_SRC: &str = include_str!("kernels/validate_aggregation.zig");

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
    const PREDICTED: &str = "O(n)";
    const SHAPE_NOTE: &str = "Confirms that a category-sum/mean aggregation (a single linear pass over the data, the shape behind a groupby().agg()) really does scale as O(n) — this runs a reference kernel Sythrall wrote itself in Zig, not the user's specific data, so it validates the SHAPE, not a particular dataset's performance.";

    let workdir = match TempWorkDir::new("aggbench") {
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
        Some(exp) => format!("Predicted {PREDICTED} (exponent 1.0) vs. measured exponent ≈ {exp:.2}. {SHAPE_NOTE}"),
        None => "No se pudieron obtener suficientes mediciones válidas para ajustar un exponente (menos de 2 puntos utilizables).".to_string(),
    };

    EmpiricalValidationResult { available: true, predicted_big_o: PREDICTED.to_string(), measurements, estimated_exponent, note }
}

pub fn validate_aggregation_linear() -> EmpiricalValidationResult {
    if !zig_available() {
        return EmpiricalValidationResult {
            available: false,
            predicted_big_o: "O(n)".to_string(),
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
    fn fit_exponent_detecta_lineal_exacto() {
        let sizes: [f64; 4] = [100.0, 200.0, 400.0, 800.0];
        let points: Vec<(f64, f64)> = sizes.iter().map(|&n| (n, n * 0.0001)).collect();
        let exp = fit_exponent(&points).unwrap();
        assert!((exp - 1.0).abs() < 0.01, "exponente esperado ~1.0, dio {exp}");
    }

    #[test]
    fn zig_available_no_panickea() {
        let _ = zig_available();
    }

    #[test]
    fn validate_aggregation_end_to_end_si_zig_esta_disponible() {
        if !zig_available() {
            eprintln!("zig no disponible en este entorno — test salteado");
            return;
        }
        let result = validate_aggregation_linear();
        assert!(result.available);
        assert!(result.measurements.len() >= 2, "esperaba al menos 2 mediciones válidas");
        let exp = result.estimated_exponent.expect("con >=2 mediciones debería poder ajustar un exponente");
        // Rango generoso a propósito, mismo criterio que zig_bench.rs: atrapar
        // una implementación rota (p.ej. O(n²) por un bug), no exigir
        // precisión de laboratorio.
        assert!((0.7..1.5).contains(&exp), "exponente medido fuera de rango razonable: {exp}");
    }

    #[test]
    fn validate_aggregation_degrada_con_gracia_sin_zig() {
        if zig_available() {
            eprintln!("zig SÍ está disponible en este entorno — el camino de degradación se prueba indirectamente arriba");
            return;
        }
        let result = validate_aggregation_linear();
        assert!(!result.available);
        assert!(result.measurements.is_empty());
        assert!(result.estimated_exponent.is_none());
    }
}
