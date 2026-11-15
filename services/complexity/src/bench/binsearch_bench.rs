//! Fase 26 (Algorithm Validation Engine) — décimo kernel, y el primero en
//! validar una forma SUB-LINEAL: búsqueda binaria iterativa escrita a mano
//! en Assembly x86 (`kernels/validate_binary_search.s`), tercer kernel en
//! Assembly de esta fase junto a `asm_bench.rs` (O(n)) y
//! `mergesort_bench.rs` (O(n log n)). El "searching...shapes beyond BFS"
//! que el ROADMAP nombraba como pendiente.
//!
//! **El eje `n` de este kernel para el ajuste de exponente NO es el tamaño
//! del arreglo — es `log2(tamaño del arreglo)`.** Para O(log n) real, el
//! tiempo de una búsqueda escala linealmente con `log2(n)`, no con `n`
//! directo — así que graficar `(log2(n), tiempo)` y ajustar la MISMA
//! regresión log-log que el resto de kernels ya usa (`fit_exponent`,
//! reusada tal cual) recupera un exponente ≈1.0 para O(log n) real, exacto
//! mismo criterio de interpretación que cualquier kernel O(n) — solo que el
//! eje horizontal es `log2(tamaño)`, no el tamaño en sí. Esto se documenta
//! explícitamente en el `note` de la respuesta, misma disciplina de "no
//! sobrecargar un campo en silencio" que `fib_bench.rs` ya estableció al
//! reusar `estimated_exponent` como base medida en vez de exponente n^k.
//!
//! **Por qué `searches` fijo, no una sola búsqueda por tamaño**: una
//! búsqueda binaria individual hace ~log2(n) comparaciones — para el rango
//! de tamaños de este kernel (256 a 134M), eso son apenas 8 a 27
//! comparaciones, muy por debajo de la resolución de `clock()` en Windows
//! (~15.6ms). Mismo problema ya resuelto por `asm_bench.rs` (cientos de
//! millones de iteraciones) — acá la solución es repetir la búsqueda un
//! número FIJO de veces (`SEARCHES`, igual para todos los tamaños), así el
//! tiempo total escala con `SEARCHES * log2(n)`, medible de sobra.
//!
//! **Targets pseudo-aleatorios (LCG con AND-mask), no secuenciales —
//! decidido en vivo, no adivinado**: la primera versión ciclaba el target
//! secuencialmente (0,1,2,...,n-1,0,...), midiendo un exponente ≈0.8 en vez
//! de cerca de 1.0. Un patrón de accesos tan regular deja que el predictor
//! de saltos de la CPU aprenda la secuencia de izquierda/derecha de cada
//! búsqueda, subestimando el costo real de comparar contra targets
//! arbitrarios. Reemplazado por un LCG (`state = state*1103515245+12345`,
//! mismo generador que `validate_mergesort_driver.c` ya usa para datos) con
//! `target = (state >> 16) & (n-1)` — el AND-mask en vez de `i mod n`
//! evita un `idiv` por búsqueda, que sería un costo FIJO no relacionado con
//! `log2(n)` y diluiría exactamente la señal que este kernel mide (`n`
//! elegido siempre como potencia de 2 por esto). Con el cambio, el
//! exponente medido subió a ≈0.95-0.97, mucho más cerca de 1.0.
//!
//! **Rango de tamaños deliberadamente MUY ancho** (256 a 134,217,728,
//! log2 = 8/16/24/27) — más ancho que cualquier otro kernel de esta fase,
//! necesario porque el ajuste log-log es sensible al ruido cuando el rango
//! es angosto: un rango más angosto (probado en vivo, ej. log2 =
//! 14/18/22/26) midió un exponente notablemente peor (≈0.60) por la misma
//! cantidad de ruido absoluto pesando más sobre una escala más comprimida.
//! 134,217,728 enteros son 512MB — dentro de lo que este proceso de 32
//! bits soporta cómodamente, confirmado en vivo antes de fijarlo (no se
//! probó más allá para no arriesgar fallos de `malloc` en otros entornos).
//!
//! Mismo límite de seguridad que el resto de Fase 26/23, textual: la
//! superficie de ejecución es el kernel fijo en
//! `kernels/validate_binary_search.s` + su driver C, escritos por Sythrall,
//! NUNCA código de usuario. Sin sandboxing robusto por la misma razón — no
//! hay input no confiable en el camino de ejecución.

use crate::bench::benchsupport::{TempWorkDir, fit_exponent, supports_dash_m32};
use std::fs;
use std::process::Command;

use serde::Serialize;

/// Tamaños de `n` (siempre potencia de 2, ver el doc del módulo para el
/// porqué del AND-mask) calibrados en vivo en esta máquina — rango
/// deliberadamente muy ancho (log2 = 8/16/24/27) para que el ajuste log-log
/// no sea tan sensible al ruido como con un rango más angosto.
const KERNEL_SIZES: &[u32] = &[256, 65_536, 16_777_216, 134_217_728];

/// El kernel de Assembly y su driver C viven como archivos reales
/// (`kernels/validate_binary_search.s` / `_driver.c`), traídos con
/// `include_str!` — mismo criterio que `asm_bench.rs`/`mergesort_bench.rs`.
const ASM_KERNEL_SRC: &str = include_str!("kernels/validate_binary_search.s");
const ASM_DRIVER_SRC: &str = include_str!("kernels/validate_binary_search_driver.c");

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
    Command::new("gcc").arg("--version").output().is_ok_and(|o| o.status.success()) && supports_dash_m32()
}

fn compile_and_run(sizes: &[u32]) -> EmpiricalValidationResult {
    const PREDICTED: &str = "O(log n)";
    const SHAPE_NOTE: &str = "Confirms that an iterative binary search written directly in Assembly really does scale close to O(log n) in general — this runs a reference kernel Sythrall wrote itself, not the user's specific function, so it validates the SHAPE, not a particular implementation's performance. IMPORTANT: unlike every other kernel in this engine, the exponent fit here is NOT against the array size directly — it's against log2(array size), since that's the axis O(log n) growth is linear in. An exponent near 1.0 here means time scales linearly with log2(n), which is exactly what O(log n) means; it does NOT mean the same thing as an exponent near 1.0 for an O(n) kernel elsewhere in this engine.";

    let workdir = match TempWorkDir::new("bsbench") {
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

    let compile = Command::new("gcc").arg(&asm_path).arg(&driver_path).arg("-m32").arg("-O2").arg("-o").arg(&bin_path).output();
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
            // detecta que no TODAS las búsquedas encontraron su índice
            // correcto — un kernel roto no debe alimentar el ajuste de
            // exponente con un tiempo bogus.
            if seconds <= 0.0 {
                continue;
            }
            best = Some(best.map_or(seconds, |b: f64| b.min(seconds)));
        }
        if let Some(seconds) = best {
            measurements.push(EmpiricalMeasurement { n, seconds });
            // El eje del ajuste es log2(n), no n -- ver el doc del módulo.
            points.push(((n as f64).log2(), seconds));
        }
    }

    let estimated_exponent = fit_exponent(&points);
    let note = match estimated_exponent {
        Some(exp) => format!("Predicted {PREDICTED} (anchored at exponent ≈1.0 against log2(array size), not array size directly) vs. measured exponent ≈ {exp:.2}. {SHAPE_NOTE}"),
        None => "No se pudieron obtener suficientes mediciones válidas para ajustar un exponente (menos de 2 puntos utilizables).".to_string(),
    };

    EmpiricalValidationResult { available: true, predicted_big_o: PREDICTED.to_string(), measurements, estimated_exponent, note }
}

pub fn validate_binary_search_logn() -> EmpiricalValidationResult {
    if !gcc_available() {
        return EmpiricalValidationResult {
            available: false,
            predicted_big_o: "O(log n)".to_string(),
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
        assert!(fit_exponent(&[(8.0, 0.01)]).is_none());
    }

    #[test]
    fn fit_exponent_detecta_lineal_contra_log2n_exacto() {
        // Datos sintéticos donde el tiempo escala EXACTAMENTE lineal con
        // log2(n) (no con n) -- confirma que pasar log2(n) como `x` a
        // `fit_exponent` recupera un exponente ≈1.0 para O(log n) real,
        // mismo criterio de interpretación que cualquier kernel O(n).
        let points: Vec<(f64, f64)> = [8.0f64, 16.0, 24.0, 27.0].iter().map(|&log2n| (log2n, log2n * 0.01)).collect();
        let exp = fit_exponent(&points).unwrap();
        assert!((0.95..1.05).contains(&exp), "esperaba exponente ≈1.0, dio {exp}");
    }

    #[test]
    fn gcc_available_no_panickea() {
        let _ = gcc_available();
    }

    #[test]
    fn validate_binary_search_end_to_end_si_gcc_esta_disponible() {
        if !gcc_available() {
            eprintln!("gcc no disponible en este entorno — test salteado");
            return;
        }
        let result = validate_binary_search_logn();
        assert!(result.available);
        assert!(result.measurements.len() >= 2, "esperaba al menos 2 mediciones válidas");
        let exp = result.estimated_exponent.expect("con >=2 mediciones debería poder ajustar un exponente");
        // Rango generoso a propósito, mismo criterio que el resto de
        // kernels: atrapar una implementación rota (ej. una búsqueda lineal
        // por accidente, que mediría un exponente MUCHO más alto contra
        // este mismo eje log2(n)), no exigir precisión de laboratorio.
        assert!((0.6..1.4).contains(&exp), "exponente medido fuera de rango razonable: {exp}");
    }

    #[test]
    fn validate_binary_search_degrada_con_gracia_sin_gcc() {
        if gcc_available() {
            eprintln!("gcc SÍ está disponible en este entorno — el camino de degradación se prueba indirectamente arriba");
            return;
        }
        let result = validate_binary_search_logn();
        assert!(!result.available);
        assert!(result.measurements.is_empty());
        assert!(result.estimated_exponent.is_none());
    }
}
