//! Fase 26 (Algorithm Validation Engine) — noveno kernel, cuarto uso de Zig
//! en este motor (`zig_bench.rs`: bubble sort O(n²); `bfs_bench.rs`: BFS
//! O(V+E); `agg_bench.rs`: agregación O(n)) — y el primero en demostrar
//! recursión en Zig dentro de esta fase. Quicksort recursivo (partición de
//! Lomuto) valida la misma forma O(n log n) que `mergesort_bench.rs`
//! (Assembly, bottom-up iterativo) ya valida, pero por una técnica
//! genuinamente distinta — divide y vencerás recursivo, no fusión
//! iterativa. Esto es exactamente lo que el ROADMAP nombraba como
//! pendiente: "a recursive sorting shape (quicksort)".
//!
//! Mismo límite de seguridad textual que el resto de este módulo: la
//! superficie de ejecución es el kernel fijo en
//! `kernels/validate_quicksort.zig`, escrito por Sythrall, NUNCA código de
//! usuario.

use crate::bench::benchsupport::{TempWorkDir, fit_exponent};
use std::fs;
use std::process::Command;

use serde::Serialize;

/// Calibrado en vivo en esta máquina de desarrollo (`zig build-exe -O
/// ReleaseFast`): n=250k/500k/1M/2M midieron (mínimo de 3)
/// 0.0140/0.0290/0.0618/0.1305s — exponente log-log ≈1.08, consistente con
/// el rango ≈1.07-1.11 que `mergesort_bench.rs` ya midió para la misma
/// forma O(n log n) por una técnica distinta.
const KERNEL_SIZES: &[u32] = &[250_000, 500_000, 1_000_000, 2_000_000];

const ZIG_KERNEL_SRC: &str = include_str!("kernels/validate_quicksort.zig");

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
    const PREDICTED: &str = "O(n log n)";
    const SHAPE_NOTE: &str = "Confirms that a recursive quicksort (Lomuto partition, divide-and-conquer) really does scale as O(n log n) on average-case (random, non-adversarial) input — this runs a reference kernel Sythrall wrote itself in Zig, not the user's specific data, so it validates the SHAPE, not a particular dataset's performance. Unlike a clean power law, O(n log n) doesn't have one fixed log-log exponent — it drifts slightly with the chosen size range, same honesty already applied to mergesort_bench.rs's identical shape.";

    let workdir = match TempWorkDir::new("qsbench") {
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

    // 3 corridas por tamaño, mínimo — mismo criterio de microbenchmarking
    // que el resto de esta fase. `seconds <= 0.0` también descarta el
    // sentinel `-1` que el kernel imprime si su propio chequeo de cordura
    // detecta que el array NO quedó ordenado — un kernel roto no debe
    // alimentar en silencio un número plausible al ajuste de exponente.
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
        Some(exp) => format!("Predicted {PREDICTED} (exponent ≈1.0, not exact — see below) vs. measured exponent ≈ {exp:.2}. {SHAPE_NOTE}"),
        None => "No se pudieron obtener suficientes mediciones válidas para ajustar un exponente (menos de 2 puntos utilizables).".to_string(),
    };

    EmpiricalValidationResult { available: true, predicted_big_o: PREDICTED.to_string(), measurements, estimated_exponent, note }
}

pub fn validate_quicksort_nlogn() -> EmpiricalValidationResult {
    if !zig_available() {
        return EmpiricalValidationResult {
            available: false,
            predicted_big_o: "O(n log n)".to_string(),
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
    fn fit_exponent_detecta_n_log_n_aproximado() {
        // n·log(n) no es una ley de potencia exacta, pero el exponente
        // log-log debería quedar cerca de 1.0-1.15 en un rango razonable —
        // mismo comportamiento que mergesort_bench.rs ya documenta.
        let sizes: [f64; 4] = [1000.0, 2000.0, 4000.0, 8000.0];
        let points: Vec<(f64, f64)> = sizes.iter().map(|&n| (n, n * n.log2() * 1e-7)).collect();
        let exp = fit_exponent(&points).unwrap();
        assert!((1.0..1.2).contains(&exp), "exponente esperado ~1.0-1.15 para n·log(n), dio {exp}");
    }

    #[test]
    fn zig_available_no_panickea() {
        let _ = zig_available();
    }

    #[test]
    fn validate_quicksort_end_to_end_si_zig_esta_disponible() {
        if !zig_available() {
            eprintln!("zig no disponible en este entorno — test salteado");
            return;
        }
        let result = validate_quicksort_nlogn();
        assert!(result.available);
        assert!(result.measurements.len() >= 2, "esperaba al menos 2 mediciones válidas");
        let exp = result.estimated_exponent.expect("con >=2 mediciones debería poder ajustar un exponente");
        // Rango generoso a propósito, mismo criterio que mergesort_bench.rs:
        // atrapar una implementación rota (p.ej. degradada a O(n²) por el
        // problema de la bandera holandesa con datos duplicados), no exigir
        // precisión de laboratorio.
        assert!((0.8..2.0).contains(&exp), "exponente medido fuera de rango razonable: {exp}");
    }

    #[test]
    fn validate_quicksort_degrada_con_gracia_sin_zig() {
        if zig_available() {
            eprintln!("zig SÍ está disponible en este entorno — el camino de degradación se prueba indirectamente arriba");
            return;
        }
        let result = validate_quicksort_nlogn();
        assert!(!result.available);
        assert!(result.measurements.is_empty());
        assert!(result.estimated_exponent.is_none());
    }
}
