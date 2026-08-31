//! Fase 26 (Algorithm Validation Engine) — segundo kernel Zig, generalizando
//! `zig_bench.rs` (bubble sort, O(n²)) a una forma algorítmica genuinamente
//! distinta: recorrido de grafos (BFS), no ordenamiento ni aritmética. Mismo
//! patrón exacto que `zig_bench.rs`/`fortran_bench.rs`/`asm_bench.rs` ya
//! establecieron: compilar y correr un kernel fijo, medir tiempo real,
//! ajustar el exponente de crecimiento empírico vía `fit_exponent`.
//!
//! El grafo es disperso y determinista: cada vértice tiene un grado de
//! salida fijo (4), la primera arista de cada vértice arma un anillo
//! (`i -> (i+1) mod n`) que garantiza que el BFS desde el vértice 0 visita
//! los `n` vértices siempre — sin esa garantía, una corrida podría terminar
//! antes de tocar todo el grafo si algún vértice quedara desconectado, lo
//! que ensuciaría la medición. Con grado fijo, `E = O(V)`, así que
//! `O(V + E) = O(V)` — la predicción es O(n), no un exponente distinto de
//! los otros 3 kernels; lo nuevo acá es la FORMA algorítmica (recorrido de
//! grafos con una cola, no un loop aritmético o de comparación), tal como
//! el ROADMAP pide explícitamente ("recorrido de grafos" como forma todavía
//! no intentada), no un exponente nuevo por sí solo.
//!
//! Mismo límite de seguridad que el resto de Fase 26/23, textual: la
//! superficie de ejecución es el kernel fijo en
//! `kernels/validate_graph_bfs.zig`, escrito por Sythrall, NUNCA código de
//! usuario. Sin sandboxing robusto por la misma razón — no hay input no
//! confiable en el camino de ejecución.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;

/// Tamaños de `n` (vértices) calibrados a mano en esta máquina de
/// desarrollo (`zig build-exe -O ReleaseFast`) — confirmado en vivo antes
/// de fijar estas constantes, no adivinado: a estos tamaños el allocator
/// general-purpose de Zig deja de dominar el tiempo medido (a n=100k el
/// overhead de allocar era visible y ensuciaba el ajuste), y el rango de
/// 500k a 4M todavía corre en bien menos de 1 segundo por corrida.
const KERNEL_SIZES: &[u32] = &[500_000, 1_000_000, 2_000_000, 4_000_000];

/// El kernel vive como un archivo `.zig` de verdad
/// (`kernels/validate_graph_bfs.zig`), traído con `include_str!` — mismo
/// criterio que `zig_bench.rs`/`fortran_bench.rs` usan para los suyos.
const ZIG_KERNEL_SRC: &str = include_str!("kernels/validate_graph_bfs.zig");

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

/// Directorio de trabajo temporal con limpieza automática — mismo criterio
/// que `zig_bench.rs::TempWorkDir` (duplicado deliberadamente, no
/// compartido entre módulos: cada validador de Fase 26 es autocontenido).
struct TempWorkDir(PathBuf);

impl TempWorkDir {
    fn new() -> std::io::Result<Self> {
        let suffix: u32 = rand::random();
        let dir = std::env::temp_dir().join(format!("sythrall_gbench_{suffix:08x}"));
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
/// `zig_bench.rs::fit_exponent`, duplicada a propósito (ver el comentario
/// de `TempWorkDir` arriba).
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
    const PREDICTED: &str = "O(V+E)";
    const SHAPE_NOTE: &str = "Confirms that BFS over a sparse graph (fixed out-degree per vertex, so E = O(V)) really does scale as O(V) in general — this runs a reference kernel Sythrall wrote itself in Zig, not the user's specific function, so it validates the SHAPE (graph traversal with a queue), not a particular implementation's performance.";

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
            // El kernel imprime "segundos visited_count" — solo el primer
            // token importa acá; `visited_count` existe en el stdout del
            // kernel como chequeo manual de cordura (confirmar que el BFS
            // realmente visitó los `n` vértices), no se parsea del lado Rust.
            let Some(seconds) = stdout.split_whitespace().next().and_then(|s| s.parse::<f64>().ok()) else { continue };
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
        Some(exp) => format!("Predicted {PREDICTED} (≈ exponent 1.0 given fixed out-degree) vs. measured exponent ≈ {exp:.2}. {SHAPE_NOTE}"),
        None => "No se pudieron obtener suficientes mediciones válidas para ajustar un exponente (menos de 2 puntos utilizables).".to_string(),
    };

    EmpiricalValidationResult { available: true, predicted_big_o: PREDICTED.to_string(), measurements, estimated_exponent, note }
}

pub fn validate_graph_bfs_linear() -> EmpiricalValidationResult {
    if !zig_available() {
        return EmpiricalValidationResult {
            available: false,
            predicted_big_o: "O(V+E)".to_string(),
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
        let sizes: [f64; 4] = [100.0, 200.0, 300.0, 400.0];
        let points: Vec<(f64, f64)> = sizes.iter().map(|&n| (n, n * 0.0001)).collect();
        let exp = fit_exponent(&points).unwrap();
        assert!((exp - 1.0).abs() < 0.01, "exponente esperado ~1.0, dio {exp}");
    }

    #[test]
    fn zig_available_no_panickea() {
        let _ = zig_available();
    }

    #[test]
    fn validate_graph_bfs_end_to_end_si_zig_esta_disponible() {
        if !zig_available() {
            eprintln!("zig no disponible en este entorno — test salteado");
            return;
        }
        let result = validate_graph_bfs_linear();
        assert!(result.available);
        assert!(result.measurements.len() >= 2, "esperaba al menos 2 mediciones válidas");
        let exp = result.estimated_exponent.expect("con >=2 mediciones debería poder ajustar un exponente");
        // Rango generoso a propósito, mismo criterio que los otros 3
        // kernels: atrapar una implementación rota (ej. O(V²) por accidente
        // recorriendo mal la cola), no exigir precisión de laboratorio en
        // una máquina de desarrollo compartida y con ruido de allocator.
        assert!((0.6..1.8).contains(&exp), "exponente medido fuera de rango razonable: {exp}");
    }

    #[test]
    fn validate_graph_bfs_degrada_con_gracia_sin_zig() {
        if zig_available() {
            eprintln!("zig SÍ está disponible en este entorno — el camino de degradación se prueba indirectamente arriba");
            return;
        }
        let result = validate_graph_bfs_linear();
        assert!(!result.available);
        assert!(result.measurements.is_empty());
        assert!(result.estimated_exponent.is_none());
    }
}
