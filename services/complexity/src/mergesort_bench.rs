//! Fase 26 (Algorithm Validation Engine) — sexto kernel, y el primero que
//! demuestra la forma O(n log n): un mergesort bottom-up iterativo escrito
//! a mano en Assembly x86 (`kernels/validate_mergesort.s`), la misma
//! sintaxis AT&T/cdecl de 32 bits que `asm_bench.rs` ya usa para
//! `validate_sum_squares.s`. Iterativo, no recursivo — la ROADMAP ya nombra
//! "recursión" como su propia forma (`fib_bench.rs`), así que este kernel
//! deliberadamente demuestra O(n log n) por la vía de las FUSIONES por
//! pasadas (log2(n) pasadas, cada una O(n)), no por una llamada recursiva.
//!
//! **O(n log n) no es una ley de potencia limpia**, a diferencia de O(n),
//! O(n²), O(n³): el "exponente" que `fit_exponent` (ajuste log-log,
//! reusado tal cual de `asm_bench.rs`/`zig_bench.rs`) mide para datos
//! `n·log(n)` no es un valor fijo — se acerca a 1.0 pero típicamente queda
//! un poco por encima (medido ≈1.1 en esta máquina para el rango de
//! tamaños elegido), y ese exceso crece lentamente con el rango de N. Esto
//! se documenta explícitamente en el `note` de la respuesta en vez de
//! pretender una precisión que el modelo no tiene — la predicción ancla en
//! 1.0 (misma banda ±0.3 que el resto de kernels O(n)-ish) precisamente
//! porque sigue sirviendo para atrapar una implementación rota (ej. un
//! O(n²) accidental por un bug en la lógica de fusión), aunque no valide
//! un exponente exacto.
//!
//! **Bug real atrapado durante el desarrollo de este kernel, documentado
//! en detalle en el propio `.s`**: la primera versión usaba offsets de
//! locales que colisionaban con los registros callee-saved guardados
//! (`ebx`/`esi`/`edi`), corrompiéndolos silenciosamente a `-O0` (donde el
//! caller nunca confía en que sobrevivan a la llamada) pero causando un
//! crash real a `-O1` en adelante (donde GCC sí cachea valores en esos
//! registros a través de la llamada). Encontrado con guard pages
//! (`VirtualAlloc`+`VirtualProtect(PAGE_NOACCESS)`), no adivinado ni
//! atrapado por casualidad — un heap-padding tradicional no lo hubiera
//! detectado de forma confiable.
//!
//! Mismo límite de seguridad que el resto de Fase 26/23, textual: la
//! superficie de ejecución es el kernel fijo en
//! `kernels/validate_mergesort.s` + su driver C, escritos por Sythrall,
//! NUNCA código de usuario. Sin sandboxing robusto por la misma razón — no
//! hay input no confiable en el camino de ejecución.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;

/// Tamaños de `n` calibrados a mano en esta máquina de desarrollo (`gcc -O2`)
/// — confirmado en vivo antes de fijar estas constantes: por debajo de
/// ~200k el tiempo medido con `clock()` (resolución gruesa en Windows)
/// quedaba demasiado cerca del piso de resolución para un ajuste
/// confiable; el rango 200k–1.6M corre en bien menos de 1 segundo por
/// tamaño incluso con las 3 repeticiones de rigor.
const KERNEL_SIZES: &[u32] = &[200_000, 400_000, 800_000, 1_600_000];

/// El kernel de Assembly y su driver C viven como archivos reales
/// (`kernels/validate_mergesort.s` / `_driver.c`), traídos con
/// `include_str!` — mismo criterio que `asm_bench.rs`.
const ASM_KERNEL_SRC: &str = include_str!("kernels/validate_mergesort.s");
const ASM_DRIVER_SRC: &str = include_str!("kernels/validate_mergesort_driver.c");

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

fn gcc_available() -> bool {
    Command::new("gcc").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// Directorio de trabajo temporal con limpieza automática — mismo criterio
/// que `asm_bench.rs::TempWorkDir` (duplicado deliberadamente, no
/// compartido entre módulos: cada validador de Fase 26 es autocontenido).
struct TempWorkDir(PathBuf);

impl TempWorkDir {
    fn new() -> std::io::Result<Self> {
        let suffix: u32 = rand::random();
        let dir = std::env::temp_dir().join(format!("sythrall_msbench_{suffix:08x}"));
        fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }
}

impl Drop for TempWorkDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Regresión lineal simple sobre `(ln n, ln segundos)` — idéntica a
/// `asm_bench.rs::fit_exponent`, duplicada a propósito. Ver el doc de
/// módulo de arriba para por qué esto no da un exponente "limpio" para
/// datos O(n log n) como sí lo da para O(n)/O(n²)/O(n³).
fn fit_exponent(points: &[(f64, f64)]) -> Option<f64> {
    let n = points.len() as f64;
    if points.len() < 2 {
        return None;
    }
    let xs: Vec<f64> = points.iter().map(|(x, _)| x.ln()).collect();
    let ys: Vec<f64> = points.iter().map(|(_, y)| y.ln()).collect();
    let x_mean = xs.iter().sum::<f64>() / n;
    let y_mean = ys.iter().sum::<f64>() / n;
    let numerator: f64 = xs.iter().zip(&ys).map(|(x, y)| (x - x_mean) * (y - y_mean)).sum();
    let denominator: f64 = xs.iter().map(|x| (x - x_mean).powi(2)).sum();
    if denominator == 0.0 {
        return None;
    }
    Some(numerator / denominator)
}

fn compile_and_run(sizes: &[u32]) -> EmpiricalValidationResult {
    const PREDICTED: &str = "O(n log n)";
    const SHAPE_NOTE: &str = "Confirms that an iterative bottom-up mergesort (log2(n) merge passes, each O(n)) written directly in Assembly really does scale close to O(n log n) in general — this runs a reference kernel Sythrall wrote itself, not the user's specific function, so it validates the SHAPE, not a particular implementation's performance. Note: O(n log n) is not a clean power law, so the fitted exponent here is expected to land a little above 1.0 (not exactly 1.0 like a true O(n) kernel) and drift slightly with the size range chosen — this anchors against 1.0 anyway because it still catches a badly broken implementation (e.g. an accidental O(n²) merge), just without claiming lab-grade precision on the exponent itself.";

    let workdir = match TempWorkDir::new() {
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

    let asm_path = workdir.0.join("kernel.s");
    let driver_path = workdir.0.join("driver.c");
    let bin_path = workdir.0.join(if cfg!(windows) { "kernel.exe" } else { "kernel" });

    if fs::write(&asm_path, ASM_KERNEL_SRC).is_err() || fs::write(&driver_path, ASM_DRIVER_SRC).is_err() {
        return EmpiricalValidationResult {
            available: false,
            predicted_big_o: PREDICTED.to_string(),
            measurements: vec![],
            estimated_exponent: None,
            note: "no se pudieron escribir los archivos fuente temporales".to_string(),
        };
    }

    let compile = Command::new("gcc").arg(&asm_path).arg(&driver_path).arg("-O2").arg("-o").arg(&bin_path).output();
    match compile {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            return EmpiricalValidationResult {
                available: false,
                predicted_big_o: PREDICTED.to_string(),
                measurements: vec![],
                estimated_exponent: None,
                note: format!("gcc falló al compilar el kernel de Assembly: {}", String::from_utf8_lossy(&out.stderr)),
            };
        }
        Err(e) => {
            return EmpiricalValidationResult {
                available: false,
                predicted_big_o: PREDICTED.to_string(),
                measurements: vec![],
                estimated_exponent: None,
                note: format!("no se pudo invocar gcc: {e}"),
            };
        }
    }

    // 3 corridas por tamaño, mínimo — mismo criterio de microbenchmarking
    // que el resto de Fase 26/23 ya usa.
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
            // El driver imprime "-1" cuando su propio chequeo de cordura
            // detecta que el array NO quedó ordenado — un kernel roto no
            // debe alimentar el ajuste de exponente con un tiempo bogus.
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
        Some(exp) => format!("Predicted {PREDICTED} (anchored at exponent ≈1.0) vs. measured exponent ≈ {exp:.2}. {SHAPE_NOTE}"),
        None => "No se pudieron obtener suficientes mediciones válidas para ajustar un exponente (menos de 2 puntos utilizables).".to_string(),
    };

    EmpiricalValidationResult { available: true, predicted_big_o: PREDICTED.to_string(), measurements, estimated_exponent, note }
}

pub fn validate_mergesort_nlogn() -> EmpiricalValidationResult {
    if !gcc_available() {
        return EmpiricalValidationResult {
            available: false,
            predicted_big_o: "O(n log n)".to_string(),
            measurements: vec![],
            estimated_exponent: None,
            note: "gcc no está instalado en este entorno — no se puede ensamblar/correr Assembly real para validar empíricamente.".to_string(),
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
    fn fit_exponent_detecta_nlogn_cerca_de_uno() {
        // n*ln(n) construido a mano -- el exponente ajustado debe quedar
        // un poco por encima de 1.0 (no exactamente 1.0), consistente con
        // la propia advertencia del módulo sobre O(n log n) no siendo una
        // ley de potencia limpia.
        let points: Vec<(f64, f64)> = [100_000.0f64, 200_000.0, 400_000.0, 800_000.0].iter().map(|&n| (n, n * n.ln() * 1e-9)).collect();
        let exp = fit_exponent(&points).unwrap();
        assert!((1.0..1.2).contains(&exp), "exponente esperado un poco sobre 1.0, dio {exp}");
    }

    #[test]
    fn gcc_available_no_panickea() {
        let _ = gcc_available();
    }

    #[test]
    fn validate_mergesort_end_to_end_si_gcc_esta_disponible() {
        if !gcc_available() {
            eprintln!("gcc no disponible en este entorno — test salteado");
            return;
        }
        let result = validate_mergesort_nlogn();
        assert!(result.available);
        assert!(result.measurements.len() >= 2, "esperaba al menos 2 mediciones válidas");
        let exp = result.estimated_exponent.expect("con >=2 mediciones debería poder ajustar un exponente");
        // Rango generoso a propósito, mismo criterio que el resto de
        // kernels: atrapar una implementación rota (ej. ~2.0 por un merge
        // que degeneró en O(n²)), no exigir precisión de laboratorio.
        assert!((0.7..1.6).contains(&exp), "exponente medido fuera de rango razonable: {exp}");
    }

    #[test]
    fn validate_mergesort_degrada_con_gracia_sin_gcc() {
        if gcc_available() {
            eprintln!("gcc SÍ está disponible en este entorno — el camino de degradación se prueba indirectamente arriba");
            return;
        }
        let result = validate_mergesort_nlogn();
        assert!(!result.available);
        assert!(result.measurements.is_empty());
        assert!(result.estimated_exponent.is_none());
    }
}
