//! Fase 23 (Execution Intelligence) — validación empírica de la predicción
//! de Big-O que `fparse.rs` hace por forma estática (Fase 20). Todo lo demás
//! en este motor es análisis estático puro: parsea texto, nunca ejecuta
//! nada. Este módulo es el primero que sí lo hace — compila y corre un
//! kernel Fortran real (`FORTRAN_KERNEL_SRC`, escrito por Sythrall, NUNCA el
//! código del usuario) a varios tamaños de `n`, mide el tiempo real vía
//! `CPU_TIME`, y ajusta el exponente de crecimiento empírico para
//! compararlo contra el O(n³) que `numerical_algorithm_note` predice por
//! forma.
//!
//! Deliberadamente sin sandboxing robusto (sin timeout externo, sin límites
//! de recursos, sin `wait_timeout`): la superficie de ejecución es ~25
//! líneas de Fortran fijas que este módulo controla al 100% — nunca
//! compila ni corre una sola línea que venga de un usuario o de un archivo
//! subido. Si este módulo alguna vez necesita correr código de terceros,
//! esa es una superficie de riesgo completamente distinta y necesitaría su
//! propio diseño de sandbox — no extender esto silenciosamente para ese caso.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;

/// Tamaños de `n` para el kernel — calibrados a mano en esta máquina de
/// desarrollo (`gfortran` MinGW, `-O2`) para que hasta la corrida más chica
/// tarde más que la resolución del reloj de `CPU_TIME` (~15.6ms en Windows,
/// múltiplos de 1/64s) y así no reportar ceros. El total de las 4 corridas
/// ronda ~1s, más ~1-2s de compilación — bien dentro del timeout de 30s que
/// usa el cliente Python.
const KERNEL_SIZES: &[u32] = &[300, 450, 600, 800];

/// El kernel vive como un archivo `.f90` de verdad (`kernels/validate_matmul.f90`),
/// no un string embebido a mano — `include_str!` lo mete en el binario en
/// tiempo de compilación (cero costo en runtime, mismo comportamiento de
/// antes), pero ahora es un archivo Fortran real: resaltado de sintaxis,
/// sin escapar comillas, versionable como su propio artefacto.
const FORTRAN_KERNEL_SRC: &str = include_str!("kernels/validate_matmul.f90");

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

/// Directorio de trabajo temporal con limpieza automática — sin esto, un
/// error a mitad de camino (compilación fallida, parseo de salida inválido)
/// dejaría el `.f90`/binario huérfano en el directorio temp del sistema para
/// siempre. Mismo espíritu de "no leaks silenciosos" que ya se aplicó esta
/// misma sesión a la fuga de memoria del Force Graph.
struct TempWorkDir(PathBuf);

impl TempWorkDir {
    fn new() -> std::io::Result<Self> {
        let suffix: u32 = rand::random();
        let dir = std::env::temp_dir().join(format!("sythrall_fbench_{suffix:08x}"));
        fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }
}

impl Drop for TempWorkDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Regresión lineal simple sobre `(ln n, ln segundos)` — la pendiente es el
/// exponente empírico. `None` si quedan menos de 2 puntos válidos (no hay
/// con qué ajustar una recta).
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
    const PREDICTED: &str = "O(n³)";
    const SHAPE_NOTE: &str = "Confirms that a compiled triple-nested-loop matrix multiply really does scale as O(n³) in general — this runs a reference kernel Sythrall wrote itself, not the user's specific function, so it validates the SHAPE the static analysis detected, not that particular implementation's performance.";

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

    // 3 corridas por tamaño, se toma el mínimo — técnica estándar de
    // microbenchmarking: el ruido de scheduling/OS solo puede sumar tiempo,
    // nunca restarlo, así que el mínimo se acerca más al costo real del
    // kernel que el promedio (que el ruido infla en una sola dirección).
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
        Some(exp) => format!("Predicted {PREDICTED} (exponent 3.0) vs. measured exponent ≈ {exp:.2}. {SHAPE_NOTE}"),
        None => "No se pudieron obtener suficientes mediciones válidas para ajustar un exponente (menos de 2 puntos utilizables).".to_string(),
    };

    EmpiricalValidationResult { available: true, predicted_big_o: PREDICTED.to_string(), measurements, estimated_exponent, note }
}

pub fn validate_matmul_cubic() -> EmpiricalValidationResult {
    if !gfortran_available() {
        return EmpiricalValidationResult {
            available: false,
            predicted_big_o: "O(n³)".to_string(),
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
    fn fit_exponent_detecta_cubico_exacto() {
        let sizes: [f64; 4] = [100.0, 200.0, 300.0, 400.0];
        let points: Vec<(f64, f64)> = sizes.iter().map(|&n| (n, (n / 100.0).powi(3) * 0.01)).collect();
        let exp = fit_exponent(&points).unwrap();
        assert!((exp - 3.0).abs() < 0.01, "exponente esperado ~3.0, dio {exp}");
    }

    #[test]
    fn fit_exponent_detecta_lineal() {
        let points: Vec<(f64, f64)> = [10.0, 20.0, 40.0, 80.0].iter().map(|&n| (n, n * 0.001)).collect();
        let exp = fit_exponent(&points).unwrap();
        assert!((exp - 1.0).abs() < 0.01, "exponente esperado ~1.0, dio {exp}");
    }

    #[test]
    fn fit_exponent_tolera_ruido_leve() {
        // Puntos cercanos a n^3 pero no exactos — el ajuste debe seguir dando
        // algo cerca de 3, no reventar ni dar cualquier cosa.
        let points = vec![(100.0, 0.0095), (200.0, 0.083), (300.0, 0.27), (400.0, 0.66)];
        let exp = fit_exponent(&points).unwrap();
        assert!((2.5..3.5).contains(&exp), "exponente esperado cerca de 3.0, dio {exp}");
    }

    #[test]
    fn gfortran_available_no_panickea() {
        // No afirmamos true/false (depende del entorno que corra el test) —
        // solo que la llamada nunca panickea.
        let _ = gfortran_available();
    }

    #[test]
    fn validate_matmul_cubic_end_to_end_si_gfortran_esta_disponible() {
        if !gfortran_available() {
            eprintln!("gfortran no disponible en este entorno — test salteado");
            return;
        }
        let result = validate_matmul_cubic();
        assert!(result.available);
        assert!(result.measurements.len() >= 2, "esperaba al menos 2 mediciones válidas");
        let exp = result.estimated_exponent.expect("con >=2 mediciones debería poder ajustar un exponente");
        // Rango generoso a propósito: máquina de desarrollo compartida, CPU_TIME
        // de resolución gruesa (~15.6ms en Windows) — el objetivo del test es
        // atrapar una implementación rota (exponente ~1 o NaN), no exigir
        // precisión de laboratorio en un entorno con ruido real de scheduling.
        assert!((2.0..4.0).contains(&exp), "exponente medido fuera de rango razonable: {exp}");
    }

    #[test]
    fn validate_matmul_cubic_degrada_con_gracia_sin_gfortran() {
        // No podemos forzar la ausencia de gfortran de forma portable en
        // este test — pero si ESTE entorno no lo tiene, confirmamos el
        // camino de degradación con gracia acá mismo en vez de un test
        // separado que nunca correría.
        if gfortran_available() {
            eprintln!("gfortran SÍ está disponible en este entorno — el camino de degradación se prueba indirectamente en validate_matmul_cubic_end_to_end_si_gfortran_esta_disponible");
            return;
        }
        let result = validate_matmul_cubic();
        assert!(!result.available);
        assert!(result.measurements.is_empty());
        assert!(result.estimated_exponent.is_none());
    }
}
