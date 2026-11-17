//! Soporte compartido de los validadores empíricos de la Fase 26.
//!
//! **Esto revierte una decisión anterior, y conviene decir por qué.** Cada
//! `*_bench.rs` traía su propia copia de estos helpers, con un doc-comment
//! que decía que la duplicación era deliberada: "cada validador de Fase 26 es
//! autocontenido, mismo patrón que `cparse.rs`/`jsts.rs` ya siguen con sus
//! propios helpers chicos en vez de una dependencia cruzada".
//!
//! Ese precedente ya no existe: los helpers chicos de `cparse.rs`/`jsts.rs`
//! eran las 4-5 copias de `text_of`/`walk` que se consolidaron en
//! `tsnode.rs`, justamente porque una de ellas se había arreglado y las otras
//! no. La justificación quedó apuntando a algo que se retiró.
//!
//! Pero el motivo de fondo es otro, y es una distinción que el comentario
//! original no hacía: **`fit_exponent` no es un helper de conveniencia, es la
//! medición.** Es el ajuste por mínimos cuadrados en escala log-log que
//! produce el exponente medido — el número con el que la Fase 26 dice si un
//! kernel escala como O(n) o como O(n²). Nueve copias de eso significan que
//! una corrección al ajuste (tratar un conjunto degenerado, pasar a mínimos
//! cuadrados ponderados, cambiar el criterio de descarte) se aplicaría a un
//! kernel y a los otros ocho no — y el sentido entero de la fase es que los
//! números sean comparables ENTRE kernels. `TempWorkDir` sí es un helper
//! chico; se comparte igual porque 10 copias cuya única diferencia era un
//! prefijo de string no son "autocontenido por diseño", son copiar y pegar.
//!
//! Lo que sí era una diferencia real —el prefijo del directorio, que evita
//! colisiones entre validaciones concurrentes y hace identificable qué dejó
//! cada una— se conserva como parámetro en vez de perderse.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Directorio de trabajo temporal con limpieza automática.
pub(crate) struct TempWorkDir(pub(crate) PathBuf);

impl TempWorkDir {
    /// `prefijo` identifica al validador que lo creó (`"aggbench"`,
    /// `"msbench"`, …): dos validaciones concurrentes no comparten
    /// directorio, y si algo queda sin limpiar se sabe de quién era.
    pub(crate) fn new(prefijo: &str) -> std::io::Result<Self> {
        let suffix: u32 = rand::random();
        let dir = std::env::temp_dir().join(format!("sythrall_{prefijo}_{suffix:08x}"));
        fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }
}

impl Drop for TempWorkDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// **Resolución del reloj más grueso observado**: `CPU_TIME` de gfortran en
/// Windows avanza de a ~15.625 ms (1/64 s). Los demás kernels usan relojes
/// más finos, así que este es el peor caso.
/// Sólo se usa como **respaldo** si la sonda no se puede compilar o correr:
/// ante la duda, el peor caso conocido. El valor que rige es el que
/// `fortran_bench::resolucion_de_cpu_time()` mide en la máquina.
pub(crate) const RESOLUCION_RELOJ: f64 = 0.015_625;

/// Cuántos ticks tiene que durar una medición para que valga.
///
/// Con 1 o 2 ticks, un error de ±1 tick es un error del 50–100%, y en escala
/// log-log eso mueve el punto casi `ln 2`. Con 8, el mismo ±1 tick es 12%.
/// No es un número mágico: es el umbral por debajo del cual la medición deja
/// de tener señal, medido, no supuesto — ver `descartar_bajo_resolucion`.
const TICKS_MINIMOS: f64 = 8.0;

/// **Descarta los puntos que están en el piso del reloj.**
///
/// El kernel de matmul de Fortran medía desde n=300, que en esta máquina da
/// **1 o 2 ticks**: la misma corrida devolvía 0.0156 s o 0.0312 s, un 100% de
/// diferencia. Ese punto, siendo el extremo izquierdo del ajuste log-log,
/// inclinaba la recta entera. Medido sobre 5 corridas de 3 repeticiones:
///
/// ```text
/// con n=300     exponentes 3.06 … 4.45   desvío 0.507   4/5 dentro del rango
/// sin n=300     exponentes 3.11 … 3.48   desvío 0.132   5/5 dentro del rango
/// ```
///
/// **Sacar un solo punto bajó el desvío 4×.** Y explica los fallos reales de
/// la suite, que daban 4.51 y 4.49 — por encima del tope de 4.0, no por
/// debajo: cuando n=300 caía en 1 tick en vez de 2, la recta se empinaba.
///
/// Devolver menos de 2 puntos hace que `fit_exponent` dé `None`, que es la
/// respuesta honesta —"no se pudo medir"— en vez de un exponente sacado de
/// ruido de cuantización.
/// **`resolucion` es el tick medido en ESTA máquina, no una constante.**
/// Hardcodear el peor caso (Windows, 1/64 s) no es ser conservador: en una
/// máquina con un reloj fino el piso de 0,125 s descarta mediciones
/// perfectamente resueltas y el validador contesta "no se pudo medir"
/// teniendo datos de sobra. Pasó de verdad: CI corre en `ubuntu-latest` y
/// ahí los tamaños 450 y 600 caían por debajo de ese piso, así que
/// sobrevivía un solo punto y `fit_exponent` daba `None`.
pub(crate) fn descartar_bajo_resolucion(points: &[(f64, f64)], resolucion: f64) -> Vec<(f64, f64)> {
    let piso = resolucion * TICKS_MINIMOS;
    points.iter().copied().filter(|(_, t)| *t >= piso).collect()
}

/// Exponente `b` de `t ≈ a·nᵇ`, por mínimos cuadrados sobre `(ln n, ln t)`.
///
/// Es la medición central de la Fase 26: un exponente cercano a 1 confirma
/// una forma lineal, cercano a 2 una cuadrática. `None` con menos de 2 puntos
/// (no hay recta que ajustar) o con varianza nula en x (todos los tamaños
/// iguales: la pendiente sería una división por cero, no un exponente).
///
/// **No confundir con `fib_bench::fit_exponential_base`**, que es otra cosa:
/// ajusta la BASE de un crecimiento exponencial (`t ≈ a·bⁿ`) usando `n` sin
/// logaritmo contra `ln t`. Una forma exponencial no tiene exponente de
/// potencia que medir, así que ese validador necesita su propio ajuste.
pub(crate) fn fit_exponent(points: &[(f64, f64)]) -> Option<f64> {
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

/// `true` si el compilador de C de esta máquina acepta `-m32`.
///
/// Se prueba compilando de verdad, no consultando una lista de plataformas:
/// tener gcc no implica tener las librerías de 32 bits instaladas, y esa
/// diferencia solo aparece al enlazar. Los kernels en Assembly x86 de 32 bits
/// la necesitan; si devuelve `false`, el validador se declara no disponible
/// en vez de reportar un fallo de compilación como si fuera un resultado.
pub(crate) fn supports_dash_m32() -> bool {
    let Ok(dir) = TempWorkDir::new("m32probe") else { return false };
    let src = dir.0.join("probe.c");
    let bin = dir.0.join(if cfg!(windows) { "probe.exe" } else { "probe" });
    if fs::write(&src, "int main(void){return 0;}\n").is_err() {
        return false;
    }
    Command::new("gcc")
        .arg("-m32")
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .output()
        .is_ok_and(|o| o.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_exponente_de_una_recta_perfecta_es_uno() {
        // t = 2n exactamente: exponente 1, sin ruido.
        let puntos: Vec<(f64, f64)> = (1..=10).map(|n| (n as f64, 2.0 * n as f64)).collect();
        let b = fit_exponent(&puntos).unwrap();
        assert!((b - 1.0).abs() < 1e-9, "{b}");
    }

    #[test]
    fn el_exponente_de_una_cuadratica_perfecta_es_dos() {
        let puntos: Vec<(f64, f64)> = (1..=10).map(|n| (n as f64, (n * n) as f64)).collect();
        let b = fit_exponent(&puntos).unwrap();
        assert!((b - 2.0).abs() < 1e-9, "{b}");
    }

    #[test]
    fn con_menos_de_dos_puntos_no_hay_recta_que_ajustar() {
        assert!(fit_exponent(&[]).is_none());
        assert!(fit_exponent(&[(1.0, 1.0)]).is_none());
    }

    #[test]
    fn con_todos_los_tamanos_iguales_devuelve_none_en_vez_de_dividir_por_cero() {
        // Varianza nula en x: la pendiente no está definida. Devolver `Some`
        // acá seria reportar un exponente inventado.
        let puntos = [(4.0, 1.0), (4.0, 2.0), (4.0, 3.0)];
        assert!(fit_exponent(&puntos).is_none());
    }

    #[test]
    fn un_directorio_temporal_se_borra_al_salir_de_scope() {
        let ruta = {
            let d = TempWorkDir::new("probe_test").expect("no se pudo crear");
            let ruta = d.0.clone();
            assert!(ruta.exists());
            ruta
        };
        assert!(!ruta.exists(), "el Drop no limpió {ruta:?}");
    }

    #[test]
    fn el_prefijo_llega_al_nombre_del_directorio() {
        // Es lo unico que diferenciaba a las 10 copias: si se pierde, dos
        // validaciones concurrentes podrian pisarse.
        let d = TempWorkDir::new("xyzzy").expect("no se pudo crear");
        assert!(d.0.to_string_lossy().contains("sythrall_xyzzy_"), "{:?}", d.0);
    }
}
