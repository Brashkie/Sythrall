//! Fase 26 (Algorithm Validation Engine) — octavo kernel, segunda vez en
//! Fortran (`fortran_bench.rs`: matmul O(n³); `fib_bench.rs`: Fibonacci
//! exponencial). Esta es específicamente la pieza 3 (y última) de "migrar de
//! numpy/pandas/scikit-learn" que el usuario pidió (2026-08-31): antes de
//! proponer un reemplazo nativo para `sklearn.LinearRegression`, medir un
//! kernel equivalente escrito por Sythrall (nunca código de usuario) y
//! comparar contra scikit-learn real — ver
//! `routers/execution.py::validate_linreg_vs_sklearn`.
//!
//! Regresión lineal univariada por mínimos cuadrados en forma cerrada
//! (ecuación normal de 2 variables) — O(n), un solo recorrido de los datos.
//! Deliberadamente NO multivariada: eso necesita álgebra lineal real
//! (inversión de matriz/eliminación gaussiana), fuera de alcance acá.
//!
//! Mismo límite de seguridad textual que `fortran_bench.rs`: la superficie
//! de ejecución es el kernel fijo en `kernels/validate_linreg.f90`, escrito
//! por Sythrall, NUNCA código de usuario.

use crate::bench::benchsupport::{TempWorkDir, fit_exponent};
use std::fs;
use std::process::Command;

use serde::Serialize;

/// Calibrado en vivo en esta máquina de desarrollo (`gfortran -O2`, MinGW
/// 32 bits): un rango más ancho (probado hasta 240M) falló directamente por
/// límite de memoria del proceso (2 arreglos REAL de 240M elementos ≈1.9GB).
/// n=40M/80M/120M/160M midieron (mínimo de 3) 0.031/0.094/0.125/0.188s —
/// ruidoso por la resolución gruesa de `CPU_TIME` en Windows (~15.6ms,
/// múltiplos de 1/64s), pero suficiente para distinguir O(n) de una
/// implementación rota.
const KERNEL_SIZES: &[u32] = &[40_000_000, 80_000_000, 120_000_000, 160_000_000];

const FORTRAN_KERNEL_SRC: &str = include_str!("kernels/validate_linreg.f90");

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

fn gfortran_available() -> bool {
    Command::new("gfortran").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn compile_and_run(sizes: &[u32]) -> EmpiricalValidationResult {
    const PREDICTED: &str = "O(n)";
    const SHAPE_NOTE: &str = "Confirms that a closed-form least-squares linear regression (a single pass accumulating 4 running sums) really does scale as O(n) — this runs a reference kernel Sythrall wrote itself in Fortran, not the user's specific data, so it validates the SHAPE, not a particular dataset's performance.";

    let workdir = match TempWorkDir::new("lrbench") {
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

    let src_path = workdir.0.join("kernel.f90");
    let bin_path = workdir.0.join(if cfg!(windows) { "kernel.exe" } else { "kernel" });

    if fs::write(&src_path, FORTRAN_KERNEL_SRC).is_err() {
        return EmpiricalValidationResult {
            available: false,
            predicted_big_o: PREDICTED.to_string(),
            measurements: vec![],
            estimated_exponent: None,
            note: "no se pudo escribir el archivo fuente temporal".to_string(),
        };
    }

    let compile = Command::new("gfortran").arg("-O2").arg("-o").arg(&bin_path).arg(&src_path).output();
    match compile {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            return EmpiricalValidationResult {
                available: false,
                predicted_big_o: PREDICTED.to_string(),
                measurements: vec![],
                estimated_exponent: None,
                note: format!("gfortran falló al compilar el kernel: {}", String::from_utf8_lossy(&out.stderr)),
            };
        }
        Err(e) => {
            return EmpiricalValidationResult {
                available: false,
                predicted_big_o: PREDICTED.to_string(),
                measurements: vec![],
                estimated_exponent: None,
                note: format!("no se pudo invocar gfortran: {e}"),
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
            // El kernel imprime 2 líneas (ver el comentario en
            // validate_linreg.f90 sobre por qué la 2da existe — evitar que
            // gfortran elimine el cómputo entero por dead-code) — solo la
            // primera es el tiempo transcurrido.
            let Some(first_line) = stdout.lines().next() else { continue };
            let Some(seconds) = first_line.trim().parse::<f64>().ok() else { continue };
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

pub fn validate_linreg_linear() -> EmpiricalValidationResult {
    if !gfortran_available() {
        return EmpiricalValidationResult {
            available: false,
            predicted_big_o: "O(n)".to_string(),
            measurements: vec![],
            estimated_exponent: None,
            note: "gfortran no está instalado en este entorno — no se puede compilar/correr Fortran real para validar empíricamente.".to_string(),
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
    fn gfortran_available_no_panickea() {
        let _ = gfortran_available();
    }

    #[test]
    fn validate_linreg_end_to_end_si_gfortran_esta_disponible() {
        if !gfortran_available() {
            eprintln!("gfortran no disponible en este entorno — test salteado");
            return;
        }
        let result = validate_linreg_linear();
        assert!(result.available);
        assert!(result.measurements.len() >= 2, "esperaba al menos 2 mediciones válidas");
        let exp = result.estimated_exponent.expect("con >=2 mediciones debería poder ajustar un exponente");
        // Rango generoso a propósito, mismo criterio que el resto de la Fase
        // 26: atrapar una implementación rota, no exigir precisión de
        // laboratorio en una máquina compartida con CPU_TIME de resolución
        // gruesa.
        assert!((0.5..2.0).contains(&exp), "exponente medido fuera de rango razonable: {exp}");
    }

    #[test]
    fn validate_linreg_degrada_con_gracia_sin_gfortran() {
        if gfortran_available() {
            eprintln!("gfortran SÍ está disponible en este entorno — el camino de degradación se prueba indirectamente arriba");
            return;
        }
        let result = validate_linreg_linear();
        assert!(!result.available);
        assert!(result.measurements.is_empty());
        assert!(result.estimated_exponent.is_none());
    }
}
