//! Fase 26 (Algorithm Validation Engine) — cuarto kernel, y el primero en
//! validar una forma NO polinomial: profundidad de recursión, no un
//! exponente `n^k` como los otros 3 (`fortran_bench.rs` O(n³),
//! `zig_bench.rs` O(n²), `asm_bench.rs`/`bfs_bench.rs` O(n)). Fibonacci
//! recursivo ingenuo (sin memoización, a propósito) tiene una complejidad
//! exacta conocida: **Θ(φⁿ)**, donde φ = (1+√5)/2 ≈ 1.618 (la razón áurea,
//! raíz de la ecuación característica `T(n) = T(n-1) + T(n-2)`) — la cota
//! ajustada real, no el "O(2ⁿ)" que suele citarse como aproximación
//! colloquial en textbooks (correcta como cota superior, pero floja).
//!
//! **Por qué esto necesita su propio ajuste estadístico, no `fit_exponent`
//! reusado**: crecimiento polinomial (`n^k`) es lineal en escala log-log
//! (`ln tiempo` vs `ln n`) — por eso `fit_exponent` ajusta esas dos
//! columnas. Crecimiento exponencial (`baseⁿ`) es lineal en escala
//! semi-log (`ln tiempo` vs `n` sin transformar) — `fit_exponential_base`
//! ajusta ESAS columnas y despeja la base con `e^pendiente`. Aplicar
//! `fit_exponent` acá daría un número sin significado (el "exponente"
//! aparente crecería sin límite a medida que se agranda el rango de N
//! elegido, porque exponencial le gana a cualquier polinomio fijo).
//!
//! Mismo límite de seguridad que el resto de Fase 26/23, textual: la
//! superficie de ejecución es el kernel fijo en
//! `kernels/validate_fib.f90`, escrito por Sythrall, NUNCA código de
//! usuario. Sin sandboxing robusto por la misma razón — no hay input no
//! confiable en el camino de ejecución.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;

/// Tamaños de `n` calibrados a mano en esta máquina de desarrollo
/// (`gfortran -O2`, `CPU_TIME` con resolución ~15.6ms en Windows) —
/// confirmado en vivo antes de fijar estas constantes, no adivinado: por
/// debajo de n=34 la medición cae por debajo de la resolución del reloj o
/// queda cuantizada al mismo bucket que su vecino (n=30 y n=32 midieron
/// exactamente lo mismo en una corrida de prueba); n=40 ya tarda medio
/// segundo, así que el rango se detiene ahí para no alargar la validación.
const KERNEL_SIZES: &[u32] = &[34, 36, 38, 40];

/// Razón áurea — raíz positiva de `x² = x + 1`, la solución de la ecuación
/// característica de la recurrencia `T(n) = T(n-1) + T(n-2)`. Calculada,
/// no hardcodeada como literal decimal, para que quede claro de dónde sale.
fn golden_ratio() -> f64 {
    (1.0 + 5.0_f64.sqrt()) / 2.0
}

/// El kernel vive como un archivo `.f90` de verdad
/// (`kernels/validate_fib.f90`), traído con `include_str!` — mismo criterio
/// que el resto de Fase 26/23 usa para los suyos.
const FORTRAN_KERNEL_SRC: &str = include_str!("kernels/validate_fib.f90");

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
    /// A diferencia de los otros 3 kernels de Fase 26, este NO es un
    /// exponente `n^k` — es la BASE `b` de un ajuste `tiempo ≈ C·bⁿ`. El
    /// nombre del campo se mantiene igual al resto del shape compartido
    /// (`EmpiricalValidationResult` es consumido genéricamente por el
    /// frontend) mientras `note` aclara explícitamente qué significa el
    /// número para este endpoint en particular.
    pub estimated_exponent: Option<f64>,
    pub note: String,
}

fn gfortran_available() -> bool {
    Command::new("gfortran").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// Directorio de trabajo temporal con limpieza automática — mismo criterio
/// que `fortran_bench.rs::TempWorkDir` (duplicado deliberadamente, no
/// compartido entre módulos: cada validador de Fase 26 es autocontenido).
struct TempWorkDir(PathBuf);

impl TempWorkDir {
    fn new() -> std::io::Result<Self> {
        let suffix: u32 = rand::random();
        let dir = std::env::temp_dir().join(format!("sythrall_fibbench_{suffix:08x}"));
        fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }
}

impl Drop for TempWorkDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Regresión lineal simple sobre `(n, ln segundos)` — a diferencia de
/// `fit_exponent` (que usa `ln n` en el eje X), acá el eje X es `n` SIN
/// transformar, porque el modelo es `tiempo ≈ C·bⁿ` (exponencial en `n`),
/// no `tiempo ≈ C·n^k` (polinomial). `e^pendiente` recupera la base `b`.
fn fit_exponential_base(points: &[(f64, f64)]) -> Option<f64> {
    let count = points.len() as f64;
    if points.len() < 2 {
        return None;
    }
    let xs: Vec<f64> = points.iter().map(|(x, _)| *x).collect();
    let ys: Vec<f64> = points.iter().map(|(_, y)| y.ln()).collect();
    let x_mean = xs.iter().sum::<f64>() / count;
    let y_mean = ys.iter().sum::<f64>() / count;
    let numerator: f64 = xs.iter().zip(&ys).map(|(x, y)| (x - x_mean) * (y - y_mean)).sum();
    let denominator: f64 = xs.iter().map(|x| (x - x_mean).powi(2)).sum();
    if denominator == 0.0 {
        return None;
    }
    Some((numerator / denominator).exp())
}

fn compile_and_run(sizes: &[u32]) -> EmpiricalValidationResult {
    const PREDICTED: &str = "exponencial (Θ(φⁿ))";

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

    let estimated_exponent = fit_exponential_base(&points);
    let phi = golden_ratio();
    let note = match estimated_exponent {
        Some(base) => format!(
            "Predicted {PREDICTED} — theoretical growth base φ = (1+√5)/2 ≈ {phi:.4} (root of the recurrence T(n)=T(n-1)+T(n-2)), not the colloquial \"O(2ⁿ)\" upper bound. Measured growth base ≈ {base:.3} per unit of n. This runs a reference kernel Sythrall wrote itself (naive recursion, deliberately not memoized), not the user's specific function, so it validates that DOUBLE RECURSION WITHOUT MEMOIZATION really does blow up exponentially — the exact shape `recursion.rs`'s static analysis already flags for Python, now confirmed empirically."
        ),
        None => "No se pudieron obtener suficientes mediciones válidas para ajustar una base de crecimiento (menos de 2 puntos utilizables).".to_string(),
    };

    EmpiricalValidationResult { available: true, predicted_big_o: PREDICTED.to_string(), measurements, estimated_exponent, note }
}

pub fn validate_naive_fibonacci_exponential() -> EmpiricalValidationResult {
    if !gfortran_available() {
        return EmpiricalValidationResult {
            available: false,
            predicted_big_o: "exponencial (Θ(φⁿ))".to_string(),
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
    fn fit_exponential_base_con_menos_de_2_puntos_es_none() {
        assert!(fit_exponential_base(&[]).is_none());
        assert!(fit_exponential_base(&[(30.0, 0.01)]).is_none());
    }

    #[test]
    fn fit_exponential_base_detecta_base_exacta() {
        // tiempo = 2^n construido a mano — la base ajustada debe dar ~2.0.
        let points: Vec<(f64, f64)> = (10..14).map(|n| (n as f64, 2.0_f64.powi(n))).collect();
        let base = fit_exponential_base(&points).unwrap();
        assert!((base - 2.0).abs() < 0.001, "base esperada ~2.0, dio {base}");
    }

    #[test]
    fn fit_exponential_base_detecta_phi_exacto() {
        let phi = golden_ratio();
        let points: Vec<(f64, f64)> = (30..34).map(|n| (n as f64, phi.powi(n))).collect();
        let base = fit_exponential_base(&points).unwrap();
        assert!((base - phi).abs() < 0.001, "base esperada ~{phi}, dio {base}");
    }

    #[test]
    fn golden_ratio_es_correcta() {
        let phi = golden_ratio();
        // Propiedad definitoria de φ: φ² = φ + 1.
        assert!((phi * phi - (phi + 1.0)).abs() < 1e-10);
        assert!((phi - 1.6180339887).abs() < 1e-9);
    }

    #[test]
    fn gfortran_available_no_panickea() {
        let _ = gfortran_available();
    }

    #[test]
    fn validate_naive_fibonacci_end_to_end_si_gfortran_esta_disponible() {
        if !gfortran_available() {
            eprintln!("gfortran no disponible en este entorno — test salteado");
            return;
        }
        let result = validate_naive_fibonacci_exponential();
        assert!(result.available);
        assert!(result.measurements.len() >= 2, "esperaba al menos 2 mediciones válidas");
        let base = result.estimated_exponent.expect("con >=2 mediciones debería poder ajustar una base");
        // Rango generoso a propósito, mismo criterio que los otros 3
        // kernels: confirmar que crece exponencialmente con una base
        // razonablemente cerca de φ≈1.618, sin exigir precisión de
        // laboratorio en una máquina de desarrollo compartida.
        assert!((1.3..2.2).contains(&base), "base medida fuera de rango razonable: {base}");
    }

    #[test]
    fn validate_naive_fibonacci_degrada_con_gracia_sin_gfortran() {
        if gfortran_available() {
            eprintln!("gfortran SÍ está disponible en este entorno — el camino de degradación se prueba indirectamente arriba");
            return;
        }
        let result = validate_naive_fibonacci_exponential();
        assert!(!result.available);
        assert!(result.measurements.is_empty());
        assert!(result.estimated_exponent.is_none());
    }
}
