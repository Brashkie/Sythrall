//! **Project Health: los cuatro scores, en Rust.**
//!
//! Eran las últimas fórmulas de autoridad que vivían en Python. El resto del
//! motor ya decidía en Rust y Python relevaba, pero los **cuatro números que
//! el producto muestra como su resultado principal** —seguridad, calidad,
//! complejidad, arquitectura— se calculaban en
//! `routers/static_analysis.py::compute_project_analysis`: 245 líneas con 42
//! operaciones aritméticas. Medido así antes de mover nada.
//!
//! No es una migración por prolijidad. La regla del proyecto es que **Rust
//! tiene la autoridad y Python releva**, y un score es exactamente una
//! afirmación de autoridad: es el número contra el que alguien compara dos
//! versiones de su código. Tenerlo en la capa de relevo significaba que la
//! única parte del análisis que el usuario mira primero era la única que el
//! motor no decidía.
//!
//! ## Lo que este módulo NO cambia
//!
//! **Las fórmulas son las mismas, byte por byte.** Portar y recalibrar a la
//! vez haría imposible saber cuál de las dos cosas movió un número —y peor,
//! los snapshots guardados compararían contra una fórmula distinta sin que
//! nadie lo note. Por eso `scoring_version` sigue en 2 y hay un test que
//! exige igualdad exacta contra los casos medidos del lado Python.
//!
//! ## Las cuatro fórmulas, y por qué cada una es como es
//!
//! - **Seguridad**: lineal y sin normalizar. Una vulnerabilidad alta es una
//!   vulnerabilidad alta, tenga el proyecto 10 archivos o 10.000 — no se
//!   diluye con el tamaño como sí tiene sentido para los smells.
//! - **Calidad**: normalizada por unidad (función o clase) y con caída
//!   exponencial. La densidad es lo que importa, y el exponencial evita que
//!   el score muera dentro del rango del código real (medido: la forma
//!   lineal anterior saturaba en 0 para el 13% de los paquetes reales).
//! - **Complejidad**: 5 es el umbral "bueno" de complejidad ciclomática, las
//!   mismas bandas que usaba radon antes de la Fase 11. Por debajo no
//!   penaliza nada.
//! - **Arquitectura**: los ciclos NO se normalizan y el acoplamiento SÍ. Un
//!   ciclo de imports es un problema en sí mismo; un proyecto de 200
//!   archivos con una docena de hubs legítimos no debe castigarse como uno
//!   de 10 con la misma cuenta absoluta.

use serde::{Deserialize, Serialize};

/// Versión de las fórmulas. Sube **solo** cuando cambia la forma de alguno de
/// los cuatro scores, para que `history.compare` pueda avisar que dos
/// snapshots no son comparables en vez de mostrar un delta inventado.
///
/// v1 = calidad lineal con recorte (hasta 2026-10-02).
/// v2 = calidad con caída exponencial.
pub const SCORING_VERSION: u32 = 2;

/// Lo que hace falta para calcular los cuatro scores. Son CONTEOS, no
/// hallazgos: quien los junta ya los tiene, y mandar los hallazgos enteros
/// solo para contarlos de nuevo sería mover kilobytes para nada.
#[derive(Deserialize, Default)]
pub struct EntradaSalud {
    #[serde(default)]
    pub security_high: usize,
    #[serde(default)]
    pub security_medium: usize,
    #[serde(default)]
    pub security_low: usize,
    #[serde(default)]
    pub structural_smells: usize,
    #[serde(default)]
    pub naming_smells: usize,
    #[serde(default)]
    pub total_functions: usize,
    #[serde(default)]
    pub total_classes: usize,
    #[serde(default)]
    pub avg_complexity: f64,
    #[serde(default)]
    pub total_cycles: usize,
    /// Smells de arquitectura SIN contar las entradas de ciclo: ésas ya
    /// pesan en `total_cycles`, y contarlas dos veces sería castigar el
    /// mismo hallazgo dos veces.
    #[serde(default)]
    pub coupling_smells: usize,
    #[serde(default)]
    pub total_files: usize,
}

#[derive(Serialize)]
pub struct ScoreSeguridad {
    pub score: i64,
    pub high: usize,
    pub medium: usize,
    pub low: usize,
}

#[derive(Serialize)]
pub struct ScoreCalidad {
    pub score: i64,
    pub smells: usize,
    pub naming: usize,
}

#[derive(Serialize)]
pub struct ScoreComplejidad {
    pub score: i64,
    pub avg_complexity: f64,
}

#[derive(Serialize)]
pub struct ScoreArquitectura {
    pub score: i64,
    pub cycles: usize,
    pub smells: usize,
}

#[derive(Serialize)]
pub struct Salud {
    pub security: ScoreSeguridad,
    pub quality: ScoreCalidad,
    pub complexity: ScoreComplejidad,
    pub architecture: ScoreArquitectura,
    pub scoring_version: u32,
}

/// Redondeo medio-arriba, el de Rust.
///
/// **Python redondea medio-a-par** (`round(0.5)` es `0`, `round(2.5)` es
/// `2`), Rust redondea medio-arriba. Sobre estas fórmulas la diferencia solo
/// aparece en un empate exacto en `.5`, que con una exponencial y una
/// división por conteos es rarísimo — pero "rarísimo" no es "nunca", y un
/// score que cambia de 74 a 75 al portarlo rompería la comparación contra un
/// snapshot guardado. Se replica el comportamiento de Python a propósito.
fn redondear_como_python(x: f64) -> f64 {
    let piso = x.floor();
    let resto = x - piso;
    if (resto - 0.5).abs() < f64::EPSILON {
        // Empate exacto: al par.
        if (piso as i64) % 2 == 0 { piso } else { piso + 1.0 }
    } else {
        x.round()
    }
}

/// Calcula los cuatro scores. **Función pura**: conteos adentro, números
/// afuera, sin red ni disco.
pub fn calcular(e: &EntradaSalud) -> Salud {
    // ── Seguridad: lineal, sin normalizar por tamaño ─────────────────────
    let penalizacion_seg = e.security_high * 15 + e.security_medium * 5 + e.security_low;
    let security = (100i64 - penalizacion_seg as i64).max(0);

    // ── Calidad: densidad por unidad, caída exponencial ──────────────────
    let unidades = (e.total_functions + e.total_classes).max(1) as f64;
    let penalizacion_cal =
        e.structural_smells as f64 / unidades * 300.0 + e.naming_smells as f64 / unidades * 100.0;
    let quality = redondear_como_python(100.0 * (-penalizacion_cal / 100.0).exp()) as i64;

    // ── Complejidad: 5 es el umbral "bueno" ──────────────────────────────
    let exceso = (e.avg_complexity - 5.0).max(0.0);
    let complexity = redondear_como_python(100.0 - exceso * 8.0).clamp(0.0, 100.0) as i64;

    // ── Arquitectura: ciclos sin normalizar, acoplamiento normalizado ────
    let archivos = e.total_files.max(1) as f64;
    let acoplamiento = redondear_como_python(e.coupling_smells as f64 / archivos * 100.0) as i64;
    let penalizacion_arq = (e.total_cycles as i64 * 15 + acoplamiento).min(100);
    let architecture = (100 - penalizacion_arq).max(0);

    Salud {
        security: ScoreSeguridad {
            score: security,
            high: e.security_high,
            medium: e.security_medium,
            low: e.security_low,
        },
        quality: ScoreCalidad {
            score: quality,
            smells: e.structural_smells,
            naming: e.naming_smells,
        },
        complexity: ScoreComplejidad { score: complexity, avg_complexity: e.avg_complexity },
        architecture: ScoreArquitectura {
            score: architecture,
            cycles: e.total_cycles,
            smells: e.coupling_smells,
        },
        scoring_version: SCORING_VERSION,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entrada() -> EntradaSalud {
        EntradaSalud::default()
    }

    /// **Un proyecto sin hallazgos da 100 en las cuatro.** Es el caso que
    /// más se mira y el que más fácil se rompe con un off-by-one.
    #[test]
    fn un_proyecto_limpio_da_cien_en_las_cuatro() {
        let s = calcular(&entrada());
        assert_eq!(s.security.score, 100);
        assert_eq!(s.quality.score, 100);
        assert_eq!(s.complexity.score, 100);
        assert_eq!(s.architecture.score, 100);
        assert_eq!(s.scoring_version, 2);
    }

    /// Seguridad: lineal y **sin normalizar por tamaño**. Una vulnerabilidad
    /// alta pesa igual en un proyecto de 10 archivos que en uno de 10.000.
    #[test]
    fn la_seguridad_no_se_diluye_con_el_tamano() {
        let mut e = entrada();
        e.security_high = 2;
        e.security_medium = 1;
        e.security_low = 5;
        let chico = calcular(&e).security.score;
        e.total_files = 10_000;
        e.total_functions = 50_000;
        assert_eq!(calcular(&e).security.score, chico, "el tamaño no puede cambiar el score de seguridad");
        assert_eq!(chico, 100 - (2 * 15 + 5 + 5));
    }

    #[test]
    fn la_seguridad_no_baja_de_cero() {
        let mut e = entrada();
        e.security_high = 50;
        assert_eq!(calcular(&e).security.score, 0);
    }

    /// Calidad: lo que importa es la DENSIDAD, no la cuenta absoluta.
    #[test]
    fn la_calidad_mide_densidad_no_cantidad() {
        let mut chico = entrada();
        chico.structural_smells = 1;
        chico.total_functions = 10;

        let mut grande = entrada();
        grande.structural_smells = 10;
        grande.total_functions = 100;

        assert_eq!(
            calcular(&chico).quality.score,
            calcular(&grande).quality.score,
            "misma densidad, mismo score"
        );
    }

    /// **El defecto que la v2 arregló, fijado con un test.** La forma lineal
    /// anterior daba 0 con densidad 1/3 y de ahí no informaba nada.
    #[test]
    fn la_calidad_conserva_resolucion_donde_la_forma_vieja_saturaba() {
        let mut e = entrada();
        e.total_functions = 30;
        e.structural_smells = 10; // densidad 0.33 -> penalizacion 100
        let a = calcular(&e).quality.score;
        e.structural_smells = 20; // densidad 0.66 -> penalizacion 200
        let b = calcular(&e).quality.score;
        assert!(a > 0 && b > 0, "ninguna de las dos satura: {a} y {b}");
        assert!(b < a, "mas smells tiene que puntuar menos: {a} -> {b}");
        assert_eq!(a, 37, "penalizacion 100 da 37, no 0");
    }

    /// El naming pesa un tercio que un smell estructural: es más ruidoso por
    /// ocurrencia.
    #[test]
    fn el_naming_pesa_un_tercio_que_un_smell_estructural() {
        let mut con_smell = entrada();
        con_smell.total_functions = 10;
        con_smell.structural_smells = 1;

        let mut con_naming = entrada();
        con_naming.total_functions = 10;
        con_naming.naming_smells = 3;

        assert_eq!(calcular(&con_smell).quality.score, calcular(&con_naming).quality.score);
    }

    /// Complejidad: por debajo del umbral no penaliza nada.
    #[test]
    fn la_complejidad_no_penaliza_debajo_del_umbral() {
        let mut e = entrada();
        for cc in [0.0, 1.0, 3.0, 5.0] {
            e.avg_complexity = cc;
            assert_eq!(calcular(&e).complexity.score, 100, "CC {cc} no debería penalizar");
        }
        e.avg_complexity = 6.0;
        assert_eq!(calcular(&e).complexity.score, 92);
    }

    #[test]
    fn la_complejidad_queda_acotada_entre_cero_y_cien() {
        let mut e = entrada();
        e.avg_complexity = 1000.0;
        assert_eq!(calcular(&e).complexity.score, 0);
    }

    /// **Arquitectura: los ciclos NO se normalizan, el acoplamiento SÍ.** Un
    /// ciclo de imports es un problema por sí mismo; una docena de hubs en
    /// 200 archivos no es lo mismo que en 10.
    #[test]
    fn los_ciclos_no_se_normalizan_y_el_acoplamiento_si() {
        let mut e = entrada();
        e.total_cycles = 2;
        e.total_files = 10;
        let pocos = calcular(&e).architecture.score;
        e.total_files = 1000;
        assert_eq!(calcular(&e).architecture.score, pocos, "los ciclos no se diluyen");
        assert_eq!(pocos, 100 - 2 * 15);

        let mut c = entrada();
        c.coupling_smells = 10;
        c.total_files = 10;
        let denso = calcular(&c).architecture.score;
        c.total_files = 1000;
        let diluido = calcular(&c).architecture.score;
        assert!(diluido > denso, "el acoplamiento sí se normaliza: {denso} -> {diluido}");
    }

    /// **Paridad exacta con la implementación de Python que reemplaza.**
    /// Los valores salen de correr la función Python sobre estas mismas
    /// entradas: portar y recalibrar a la vez haría imposible saber cuál de
    /// las dos cosas movió un número.
    #[test]
    fn paridad_exacta_con_los_valores_medidos_del_lado_python() {
        /// (smells, naming, funcs, clases, cc, ciclos, acopl, archivos)
        type Conteos = (usize, usize, usize, usize, f64, usize, usize, usize);
        /// (seguridad, calidad, complejidad, arquitectura)
        type Scores = (i64, i64, i64, i64);
        let casos: &[(Conteos, Scores)] = &[
            ((0, 0, 10, 0, 1.0, 0, 0, 5), (100, 100, 100, 100)),
            ((1, 0, 10, 0, 2.0, 0, 0, 5), (100, 74, 100, 100)),
            ((3, 0, 11, 0, 2.0, 0, 0, 1), (100, 44, 100, 100)),
            ((10, 0, 30, 0, 5.0, 0, 0, 10), (100, 37, 100, 100)),
            ((0, 0, 0, 0, 13.0, 0, 0, 1), (100, 100, 36, 100)),
            ((0, 0, 1, 0, 1.0, 3, 0, 10), (100, 100, 100, 55)),
            ((0, 0, 1, 0, 1.0, 0, 5, 10), (100, 100, 100, 50)),
        ];
        for ((sm, nm, fu, cl, cc, cy, co, fi), (sec, cal, cx, arq)) in casos {
            let e = EntradaSalud {
                structural_smells: *sm,
                naming_smells: *nm,
                total_functions: *fu,
                total_classes: *cl,
                avg_complexity: *cc,
                total_cycles: *cy,
                coupling_smells: *co,
                total_files: *fi,
                ..Default::default()
            };
            let s = calcular(&e);
            assert_eq!(s.security.score, *sec, "seguridad con {e:?}", e = (sm, nm, fu, cl, cc));
            assert_eq!(s.quality.score, *cal, "calidad con smells={sm} naming={nm} unidades={}", fu + cl);
            assert_eq!(s.complexity.score, *cx, "complejidad con cc={cc}");
            assert_eq!(s.architecture.score, *arq, "arquitectura con ciclos={cy} acopl={co} archivos={fi}");
        }
    }

    /// **Python redondea medio-a-par y Rust medio-arriba.** En un empate
    /// exacto, portar sin replicarlo movería un score y rompería la
    /// comparación contra un snapshot guardado.
    #[test]
    fn el_redondeo_replica_el_de_python() {
        assert_eq!(redondear_como_python(0.5), 0.0);
        assert_eq!(redondear_como_python(1.5), 2.0);
        assert_eq!(redondear_como_python(2.5), 2.0);
        assert_eq!(redondear_como_python(3.5), 4.0);
        // Y fuera del empate se comporta normal.
        assert_eq!(redondear_como_python(2.4), 2.0);
        assert_eq!(redondear_como_python(2.6), 3.0);
    }

    /// Cero unidades no divide por cero: un archivo sin funciones ni clases
    /// con un smell encima es un caso real (un módulo de solo constantes).
    #[test]
    fn cero_unidades_no_divide_por_cero() {
        let mut e = entrada();
        e.structural_smells = 1;
        let s = calcular(&e);
        assert!(s.quality.score >= 0 && s.quality.score <= 100, "{}", s.quality.score);
    }
}
