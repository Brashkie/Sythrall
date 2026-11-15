//! Fase 34, Línea A — tercera y última porción: **medir** el costo de
//! compilación que `plantillas.rs` solo puede describir.
//!
//! **La promesa que cierra.** `plantillas.rs` cuenta lo ESCRITO y dice
//! explícitamente que el costo real —símbolos, memoria, tiempo de build—
//! exige compilar. Esto es ese "exige compilar", hecho.
//!
//! **Compila una sonda que Sythrall escribe, nunca el código del usuario.**
//! Mismo criterio que `fortran_bench.rs` de la Fase 23: el archivo de alguien
//! necesita sus includes, sus flags y su sistema de build, así que intentar
//! compilarlo fallaría casi siempre y la falla no diría nada. Lo que sí se
//! puede medir es **cómo escala el costo de instanciar templates en ESTA
//! máquina**, y con eso ubicar la profundidad que el análisis estático
//! encontró sobre una curva real.
//!
//! **Lo que se mide y lo que NO se afirma en los tests, que es la decisión
//! importante del módulo.** Los benchmarks que ya existen (`fortran_bench`,
//! `zig_bench`, `agg_bench`, `bfs_bench`) **asertan sobre tiempo de pared**:
//! ajustan un exponente y exigen que caiga en un rango. Eso los convirtió en
//! el único punto intermitente de una suite de más de mil tests — falla con
//! la máquina cargada, y falló de verdad varias veces durante el desarrollo.
//!
//! Acá eso no se repite, porque hay una magnitud **determinística** a mano:
//! el **tamaño del objeto** que sale de compilar. Cada instanciación de un
//! template de función a `-O0` es una función emitida de verdad, con su
//! símbolo; el `.o` crece con la profundidad, y ese número no depende de
//! cuánta CPU haya libre. Entonces:
//!
//!   - **El test aserta sobre bytes de objeto** (determinístico): más
//!     profundidad, más objeto.
//!   - **El tiempo se reporta como dato, y no se aserta nunca.** Es la
//!     información que el usuario quiere ver; no es una condición de
//!     corrección de este análisis.
//!
//! **Degrada con gracia sin compilador**, igual que el resto: `available:
//! false` con el motivo, nunca un error crudo.

use std::fs;
use std::process::Command;
use std::time::Instant;

use serde::Serialize;

use crate::bench::benchsupport::TempWorkDir;

/// Profundidades de la sonda. Cortas a propósito: la curva se ve igual y
/// compilar cuatro veces tiene que seguir siendo cosa de segundos.
const PROFUNDIDADES: &[u32] = &[8, 16, 32, 64];

/// Las mismas profundidades, visibles para . **Comparar dos curvas
/// medidas en puntos distintos no compararia nada**, asi que el modulo de D
/// tiene un test que exige que coincidan y necesita poder leerlas.
pub const PROFUNDIDADES_PUBLICAS: &[u32] = PROFUNDIDADES;

/// Sonda: una recursión de templates que **emite código** en cada nivel.
///
/// Es deliberadamente una función y no un `static constexpr int` como el
/// `Factorial` de los ejemplos: una constante de compilación se evalúa y
/// desaparece, así que el objeto no crecería y no habría nada determinístico
/// que medir. A `-O0` nada se inlinea, cada `f<k>` es una función real, y el
/// `.o` crece con la profundidad.
const SONDA: &str = r#"
template<int N> int f(int x) { return x + f<N - 1>(x); }
template<> int f<0>(int x) { return x; }
int main() { return f<PROFUNDIDAD>(1); }
"#;

#[derive(Serialize, Clone)]
pub struct TemplateCostPoint {
    pub depth: u32,
    /// Tiempo de pared de compilar. **Dato, no aserción** — ver el doc del
    /// módulo.
    pub compile_ms: u64,
    /// Bytes del objeto. Determinístico: no depende de la carga de la máquina.
    pub object_bytes: u64,
    /// **Símbolos que el compilador emitió de verdad**, leídos de la tabla
    /// del objeto con `binparse`.
    ///
    /// Este número es la razón de ser del módulo. El bullet de la Línea A
    /// dejó escrita la trampa exacta: una maqueta del hallazgo mostraba
    /// *"Generated symbols: 12.430"* junto a *"Depth: 7"*, y **el primero
    /// no es estáticamente computable** — saberlo exige instanciar, o sea
    /// compilar. Acá se compila y se cuenta, en vez de estimarlo.
    pub symbols: usize,
}

#[derive(Serialize)]
pub struct TemplateCostResult {
    pub available: bool,
    /// Qué compilador se usó, para que el número sea interpretable.
    pub compiler: Option<String>,
    pub points: Vec<TemplateCostPoint>,
    /// **Símbolos que emite cada nivel más de instanciación.**
    ///
    /// Medido, es más limpio que la pendiente en bytes: `3` exacto en los
    /// cuatro puntos de la curva (33, 57, 105 y 201 símbolos para
    /// profundidades 8, 16, 32 y 64), mientras que los bytes por nivel
    /// dependen del ABI y de la máquina. "Cada nivel emite 3 símbolos" es
    /// una afirmación que se puede llevar a otra máquina; "cada nivel
    /// cuesta 376 bytes" no.
    pub symbols_per_level: Option<u64>,
    /// Pendiente medida: bytes de objeto que cuesta cada nivel más de
    /// instanciación en esta máquina. Es lo que vuelve útil a la curva —
    /// permite ubicar la profundidad que el análisis estático encontró.
    pub bytes_per_level: Option<u64>,
    pub note: String,
}

/// Nota del panel. **En español a propósito**, a diferencia de los mensajes
/// de hallazgo del motor: esto no es un hallazgo, es texto explicativo que
/// se renderiza dentro de un panel que ya está todo en español, y quedaba
/// como un bloque en inglés en el medio.
const NOTA: &str = "Medido EN ESTA MÁQUINA compilando una sonda que Sythrall escribe él mismo — nunca tu código, que necesitaría sus propios includes, flags y sistema de build. Lo que muestra es cómo escala acá el costo de instanciar templates según la profundidad, así que la profundidad que encontró el análisis estático se puede ubicar sobre una curva real. Las columnas se leen distinto: los símbolos y el tamaño del objeto son determinísticos y crecen linealmente con la profundidad (una función emitida por nivel), mientras que a estas profundidades chicas el TIEMPO de compilación está dominado por el arranque del proceso y casi no se mueve — así que los símbolos y los bytes son la señal, y los milisegundos son contexto, no una medición de los templates.";

fn compilador_disponible() -> Option<String> {
    for candidato in ["g++", "clang++", "c++"] {
        if Command::new(candidato).arg("--version").output().is_ok_and(|o| o.status.success()) {
            return Some(candidato.to_string());
        }
    }
    None
}

fn degradado(motivo: &str) -> TemplateCostResult {
    TemplateCostResult {
        available: false,
        compiler: None,
        points: vec![],
        symbols_per_level: None,
        bytes_per_level: None,
        note: motivo.to_string(),
    }
}

pub fn validate_template_cost() -> TemplateCostResult {
    let Some(compilador) = compilador_disponible() else {
        return degradado(
            "No C++ compiler found (tried g++, clang++, c++). The static template metrics do not need one; only this measured half does.",
        );
    };
    let Ok(workdir) = TempWorkDir::new("tplcost") else {
        return degradado("could not create a temporary directory");
    };

    let mut points = Vec::new();
    for &profundidad in PROFUNDIDADES {
        let src = workdir.0.join(format!("sonda{profundidad}.cpp"));
        let obj = workdir.0.join(format!("sonda{profundidad}.o"));
        if fs::write(&src, SONDA.replace("PROFUNDIDAD", &profundidad.to_string())).is_err() {
            return degradado("could not write the probe");
        }

        let t0 = Instant::now();
        let salida = Command::new(&compilador)
            .arg("-c")
            .arg("-O0")
            .arg(&src)
            .arg("-o")
            .arg(&obj)
            .output();
        let compile_ms = t0.elapsed().as_millis() as u64;

        match salida {
            Ok(o) if o.status.success() => {}
            Ok(o) => {
                return degradado(&format!(
                    "the probe failed to compile at depth {profundidad}: {}",
                    String::from_utf8_lossy(&o.stderr).lines().next().unwrap_or("unknown error")
                ));
            }
            Err(e) => return degradado(&format!("could not run {compilador}: {e}")),
        }

        let object_bytes = fs::metadata(&obj).map(|m| m.len()).unwrap_or(0);
        // La tabla de símbolos del objeto recién compilado. `binparse` ya
        // sabía leerla desde la Fase 19 (analizador de ejecutables): acá
        // solo se la apunta a un objeto que este mismo módulo generó.
        let symbols = fs::read(&obj).map(|b| crate::lenguajes::binparse::parse(&b).symbols.len()).unwrap_or(0);
        points.push(TemplateCostPoint { depth: profundidad, compile_ms, object_bytes, symbols });
    }

    // Pendiente entre los extremos. Con la relación lineal que sale de
    // medir (un nivel = una función emitida), dos puntos alcanzan.
    let bytes_per_level = match (points.first(), points.last()) {
        (Some(a), Some(b)) if b.depth > a.depth && b.object_bytes > a.object_bytes => {
            Some((b.object_bytes - a.object_bytes) / u64::from(b.depth - a.depth))
        }
        _ => None,
    };
    let symbols_per_level = match (points.first(), points.last()) {
        (Some(a), Some(b)) if b.depth > a.depth && b.symbols > a.symbols => {
            Some((b.symbols - a.symbols) as u64 / u64::from(b.depth - a.depth))
        }
        _ => None,
    };
    TemplateCostResult {
        available: true,
        compiler: Some(compilador),
        symbols_per_level,
        points,
        bytes_per_level,
        note: NOTA.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_compilador_degrada_con_gracia() {
        // Cuando no hay compilador el resultado es `available: false` con el
        // motivo, nunca un error crudo ni un panic.
        if compilador_disponible().is_some() {
            return;
        }
        let r = validate_template_cost();
        assert!(!r.available);
        assert!(r.note.contains("No C++ compiler"), "{}", r.note);
        assert!(r.points.is_empty());
    }

    #[test]
    fn volcar_curva() {
        let r = validate_template_cost();
        if !r.available {
            eprintln!("sin compilador: {}", r.note);
            return;
        }
        for p in &r.points {
            eprintln!("profundidad {:3} -> {:6} bytes, {:4} simbolos, {:5} ms",
                      p.depth, p.object_bytes, p.symbols, p.compile_ms);
        }
    }

    /// **El numero que el bullet de la Linea A dijo que no se puede calcular
    /// estaticamente.** Una maqueta del hallazgo mostraba "Generated symbols:
    /// 12.430" junto a "Depth: 7"; el primero exige instanciar, o sea
    /// compilar. Aca se compila y se cuenta.
    ///
    /// Se afirma la FORMA (crece con la profundidad, de manera lineal), no un
    /// numero exacto: el conteo depende del compilador y de su version. Sobre
    /// el gcc de esta maquina la curva dio 33, 57, 105 y 201 simbolos para
    /// profundidades 8, 16, 32 y 64 — exactamente 3 por nivel.
    #[test]
    fn los_simbolos_generados_crecen_con_la_profundidad() {
        let r = validate_template_cost();
        if !r.available {
            eprintln!("sin compilador — test salteado: {}", r.note);
            return;
        }
        let puntos = &r.points;
        assert!(puntos.len() >= 2, "hacen falta dos puntos para una pendiente");
        let primero = puntos.first().unwrap();
        let ultimo = puntos.last().unwrap();
        assert!(
            ultimo.symbols > primero.symbols,
            "mas instanciaciones tienen que emitir mas simbolos: {} -> {}",
            primero.symbols,
            ultimo.symbols
        );
        assert!(primero.symbols > 0, "un objeto compilado siempre tiene simbolos");
        let por_nivel = r.symbols_per_level.expect("con pendiente positiva hay simbolos por nivel");
        assert!(por_nivel >= 1, "al menos un simbolo por nivel de instanciacion");
    }

    /// **Los simbolos aportan senal PROPIA, no repiten la de los bytes.** Si
    /// fueran redundantes no valdria la pena leerlos: lo que los hace utiles
    /// es que su pendiente es un entero chico y estable (3 por nivel en esta
    /// maquina) mientras la de bytes son cientos, dependientes del ABI.
    /// "Cada nivel emite 3 simbolos" se puede llevar a otra maquina; "cada
    /// nivel cuesta 376 bytes" no.
    #[test]
    fn la_pendiente_en_simbolos_es_mas_interpretable_que_la_de_bytes() {
        let r = validate_template_cost();
        if !r.available {
            return;
        }
        let (Some(simbolos), Some(bytes)) = (r.symbols_per_level, r.bytes_per_level) else {
            return;
        };
        assert!(
            simbolos < bytes,
            "la pendiente en simbolos tiene que ser mucho menor que la de bytes: {simbolos} vs {bytes}"
        );
        assert!(simbolos <= 20, "un entero chico e interpretable, no un numero de maquina: {simbolos}");
    }

    #[test]
    fn mas_profundidad_es_mas_objeto_si_hay_compilador() {
        // **La aserción es sobre BYTES, no sobre tiempo**, y ése es el punto
        // del módulo: el tamaño del objeto es determinístico, así que este
        // test no puede fallar por tener la máquina cargada — que es
        // exactamente lo que vuelve intermitentes a los otros benchmarks.
        if compilador_disponible().is_none() {
            return;
        }
        let r = validate_template_cost();
        assert!(r.available, "{}", r.note);
        assert_eq!(r.points.len(), PROFUNDIDADES.len());
        let primero = &r.points[0];
        let ultimo = r.points.last().unwrap();
        assert!(
            ultimo.object_bytes > primero.object_bytes,
            "profundidad {} dio {} bytes y profundidad {} dio {}: el objeto tiene que crecer con la profundidad",
            primero.depth,
            primero.object_bytes,
            ultimo.depth,
            ultimo.object_bytes
        );
        // La pendiente es lo que vuelve útil a la curva: sin ella hay cuatro
        // números sueltos y ninguna forma de ubicar la profundidad que el
        // análisis estático encontró.
        let pendiente = r.bytes_per_level.expect("con el objeto creciendo, la pendiente existe");
        assert!(pendiente > 0, "cada nivel tiene que costar algo de objeto");
    }

    #[test]
    fn la_nota_dice_que_la_sonda_no_es_el_codigo_del_usuario() {
        // Misma promesa que `fortran_bench`: se compila lo que Sythrall
        // escribe, y eso tiene que estar dicho donde el usuario lo lea.
        let r = validate_template_cost();
        if r.available {
            assert!(r.note.contains("nunca tu código"), "{}", r.note);
        }
    }
}
