//! Fase 26 (Algorithm Validation Engine) — generaliza `fortran_bench.rs`
//! (Fase 23) más allá de Fortran/matmul, esta vez en Assembly x86 real:
//! compila y corre un kernel de suma de cuadrados (`sum_squares`, escrito a
//! mano en GNU as, sintaxis AT&T — la misma que `asmparse.rs` ya sabe leer)
//! para validar empíricamente que escala como O(n), no solo por su forma
//! estática (un solo loop).
//!
//! Necesita un driver C mínimo (`validate_sum_squares_driver.c`) porque un
//! objeto `.s` puro no trae su propio entry point — el driver solo parsea
//! `argv[1]`, mide el tiempo de la llamada real a la función en Assembly, e
//! imprime segundos en stdout, mismo formato que el resto de los kernels de
//! validación. Ni el `.s` ni el `.c` son código de usuario — ambos, escritos
//! por Sythrall, son la superficie de ejecución fija de este módulo, mismo
//! límite de seguridad que `fortran_bench.rs`/`zig_bench.rs` ya documentan.
//!
//! Convención cdecl de 32 bits en el kernel de Assembly — el toolchain
//! MinGW de esta máquina de desarrollo (`gcc -dumpmachine` → `mingw32`) es
//! de 32 bits; el binario resultante corre igual como proceso separado,
//! sin ningún link directo con este sidecar (compilado en 64 bits) — mismo
//! desacople que ya existe con el binario de Fortran/Zig, ningún problema
//! de ABI porque nunca se linkea, solo se ejecuta y se lee su stdout.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;

/// Tamaños de `n` calibrados a mano en esta máquina de desarrollo (`gcc -O2`,
/// `clock()` de resolución gruesa en Windows) — confirmado en vivo que hacen
/// falta cientos de millones de iteraciones para que el loop más chico
/// supere la resolución del reloj, a diferencia de Zig (`std.time.Timer` de
/// alta resolución) — no es descuido, es la realidad de `clock()`.
const KERNEL_SIZES: &[u32] = &[200_000_000, 400_000_000, 800_000_000, 1_600_000_000];

/// El kernel de Assembly y su driver C viven como archivos reales
/// (`kernels/validate_sum_squares.s` / `_driver.c`), traídos con
/// `include_str!` — mismo criterio que `fortran_bench.rs`/`zig_bench.rs`.
const ASM_KERNEL_SRC: &str = include_str!("kernels/validate_sum_squares.s");
const ASM_DRIVER_SRC: &str = include_str!("kernels/validate_sum_squares_driver.c");

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
/// que `fortran_bench.rs`/`zig_bench.rs::TempWorkDir`, duplicado a
/// propósito (cada validador de Fase 26 es autocontenido).
struct TempWorkDir(PathBuf);

impl TempWorkDir {
    fn new() -> std::io::Result<Self> {
        let suffix: u32 = rand::random();
        let dir = std::env::temp_dir().join(format!("sythrall_abench_{suffix:08x}"));
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
/// `fortran_bench.rs::fit_exponent`, duplicada a propósito.
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
    const PREDICTED: &str = "O(n)";
    const SHAPE_NOTE: &str = "Confirms that a single-loop sum-of-squares written directly in Assembly really does scale as O(n) in general — this runs a reference kernel Sythrall wrote itself, not the user's specific function, so it validates the SHAPE, not a particular implementation's performance.";

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
    // que `fortran_bench.rs`/`zig_bench.rs` ya usan.
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

pub fn validate_sum_squares_linear() -> EmpiricalValidationResult {
    if !gcc_available() {
        return EmpiricalValidationResult {
            available: false,
            predicted_big_o: "O(n)".to_string(),
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
    fn fit_exponent_detecta_lineal_exacto() {
        let points: Vec<(f64, f64)> = [10.0, 20.0, 40.0, 80.0].iter().map(|&n| (n, n * 0.001)).collect();
        let exp = fit_exponent(&points).unwrap();
        assert!((exp - 1.0).abs() < 0.01, "exponente esperado ~1.0, dio {exp}");
    }

    #[test]
    fn gcc_available_no_panickea() {
        let _ = gcc_available();
    }

    #[test]
    fn validate_sum_squares_end_to_end_si_gcc_esta_disponible() {
        if !gcc_available() {
            eprintln!("gcc no disponible en este entorno — test salteado");
            return;
        }
        let result = validate_sum_squares_linear();
        assert!(result.available);
        assert!(result.measurements.len() >= 2, "esperaba al menos 2 mediciones válidas");
        let exp = result.estimated_exponent.expect("con >=2 mediciones debería poder ajustar un exponente");
        // Rango generoso a propósito, mismo criterio que fortran_bench.rs:
        // atrapar una implementación rota (exponente ~0 o ~2), no exigir
        // precisión de laboratorio con clock() de resolución gruesa.
        assert!((0.5..1.7).contains(&exp), "exponente medido fuera de rango razonable: {exp}");
    }

    #[test]
    fn validate_sum_squares_degrada_con_gracia_sin_gcc() {
        if gcc_available() {
            eprintln!("gcc SÍ está disponible en este entorno — el camino de degradación se prueba indirectamente arriba");
            return;
        }
        let result = validate_sum_squares_linear();
        assert!(!result.available);
        assert!(result.measurements.is_empty());
        assert!(result.estimated_exponent.is_none());
    }
}
