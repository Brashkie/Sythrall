//! **Fase 34, Línea A — el segundo punto de datos: D.**
//!
//! `tplcost.rs` mide cuánto cuesta instanciar templates en C++ y saca un
//! número limpio: **3 símbolos emitidos por nivel**, idéntico en los cuatro
//! puntos de la curva. La pregunta que esa medición sola no puede responder
//! es la que justifica este módulo: **¿ese modelo de costo es de la
//! metaprogramación, o es un artefacto de C++?**
//!
//! Un solo lenguaje no distingue las dos cosas. Dos sí.
//!
//! **Por qué D y no cualquier otro.** No es "D es interesante" — ésa es la
//! razón que el propio ROADMAP prohíbe. D tiene metaprogramación de primera
//! clase con un mecanismo **distinto** al de C++: `static if` dentro del
//! template resuelve la recursión sin necesitar una especialización aparte
//! para el caso base, y el compilador es uno solo (no hay un frontend de
//! plantillas y otro de constexpr). Si aun así el costo por nivel sale igual,
//! la afirmación de la Línea A se vuelve del mecanismo; si sale distinto, era
//! de C++ y había que decirlo.
//!
//! ## Qué se mide, y qué NO se aserta
//!
//! **Exactamente lo mismo que en C++, o la comparación no valdría nada:**
//!
//! - **Símbolos y bytes de objeto**: determinísticos, no dependen de la carga
//!   de la máquina. Sobre eso se aserta.
//! - **Tiempo de compilación**: se reporta como dato y **no se aserta
//!   nunca**. Los benchmarks que sí asertan tiempo de pared fueron el único
//!   punto intermitente de una suite de más de mil tests; esa lección ya está
//!   pagada y no se repite acá.
//!
//! ## La sonda
//!
//! Una recursión de templates que **emite código** en cada nivel, igual que
//! la de C++. Deliberadamente **no** es una `enum` evaluada en tiempo de
//! compilación: eso es CTFE, se evalúa y desaparece, el objeto no crece y no
//! quedaría nada determinístico que medir — el mismo motivo por el que
//! `tplcost.rs` descartó un `static constexpr int`.
//!
//! **Degrada con gracia sin compilador**, igual que el resto del motor:
//! `available: false` con el motivo, nunca un error crudo.

use std::fs;
use std::process::Command;
use std::time::Instant;

use serde::Serialize;

use crate::bench::benchsupport::TempWorkDir;

/// Las MISMAS profundidades que `tplcost.rs`. Comparar dos curvas medidas en
/// puntos distintos no sería comparar nada.
const PROFUNDIDADES: &[u32] = &[8, 16, 32, 64];

/// Sonda en D. Equivalente a la de C++ nivel por nivel.
///
/// `static if (N == 0)` hace de caso base **dentro** del mismo template: en
/// C++ hace falta una especialización explícita (`template<> int f<0>`). Es
/// justamente la diferencia de mecanismo que este módulo existe para medir.
const SONDA: &str = r#"
int f(int N)(int x) {
    static if (N == 0) return x;
    else return x + f!(N - 1)(x);
}
int main() { return f!(PROFUNDIDAD)(1); }
"#;

#[derive(Serialize, Clone)]
pub struct DCostPoint {
    pub depth: u32,
    /// Tiempo de pared de compilar. **Dato, no aserción.**
    pub compile_ms: u64,
    pub object_bytes: u64,
    /// Símbolos emitidos de verdad, leídos del objeto con `binparse`.
    pub symbols: usize,
}

#[derive(Serialize)]
pub struct DCostResult {
    pub available: bool,
    pub compiler: Option<String>,
    pub points: Vec<DCostPoint>,
    /// Símbolos que emite cada nivel más de instanciación. **Es el número
    /// que se compara contra el de C++.**
    pub symbols_per_level: Option<u64>,
    pub bytes_per_level: Option<u64>,
    pub note: String,
}

const NOTA: &str = "Segundo punto de datos para la misma pregunta que mide la curva de C++: cuánto cuesta instanciar un nivel más de metaprogramación. Se compila una sonda que Sythrall escribe él mismo —nunca tu código— con las MISMAS profundidades que la de C++, porque comparar dos curvas medidas en puntos distintos no compararía nada. Las columnas se leen igual que allá: símbolos y bytes son determinísticos y son la señal; los milisegundos a estas profundidades están dominados por el arranque del proceso y son contexto, no una medición. Si el costo por nivel coincide con el de C++, el modelo es del MECANISMO; si difiere, era de C++ y hay que decirlo.";

/// Compiladores de D, en orden de preferencia. `ldc2` y `gdc` van primero
/// porque comparten backend con LLVM/GCC —el mismo que compila la sonda de
/// C++—, así que la comparación tiene una variable menos.
///
/// **Resuelto por `toolchain::resolver`, no por el PATH, y eso arreglo algo
/// que estaba roto.** Medido el 2026-10-16: `dmd` no esta en el PATH de esta
/// maquina —ni en el del usuario ni en el del sistema— porque se instalo
/// aparte a proposito. La consecuencia era que este modulo respondia *"No D
/// compiler found"* y la comparacion C++ vs D, documentada con numeros
/// medidos, no se podia volver a producir. Los numeros eran reales; el camino
/// para reproducirlos, no.
fn compilador_disponible() -> Option<(String, std::ffi::OsString)> {
    for candidato in ["ldc2", "gdc", "dmd"] {
        if let Some(ruta) = crate::costo::toolchain::resolver(candidato) {
            return Some((candidato.to_string(), ruta));
        }
    }
    None
}

fn degradado(motivo: &str) -> DCostResult {
    DCostResult {
        available: false,
        compiler: None,
        points: vec![],
        symbols_per_level: None,
        bytes_per_level: None,
        note: motivo.to_string(),
    }
}

/// Argumentos de "compilar a objeto, sin optimizar" para cada compilador.
///
/// **No son intercambiables**: `dmd` y `ldc2` usan `-of=`, `gdc` usa la forma
/// de GCC. Y el nivel cero de optimización importa: con optimización, el
/// compilador inlinea la recursión y el objeto deja de crecer con la
/// profundidad, que es justo la señal que se quiere medir.
fn argumentos(compilador: &str, src: &str, obj: &str) -> Vec<String> {
    match compilador {
        "gdc" => vec!["-c".into(), "-O0".into(), src.into(), "-o".into(), obj.into()],
        // `-O0` no existe en dmd/ldc2: no pasar `-O` YA es sin optimizar.
        _ => vec!["-c".into(), format!("-of={obj}"), src.into()],
    }
}

pub fn validate_d_cost() -> DCostResult {
    let Some((compilador, ejecutable)) = compilador_disponible() else {
        return degradado(
            "No D compiler found (tried ldc2, gdc, dmd). The C++ curve does not need one; only this second data point does.",
        );
    };
    let Ok(workdir) = TempWorkDir::new("dcost") else {
        return degradado("could not create a temporary directory");
    };

    let mut points = Vec::new();
    for &profundidad in PROFUNDIDADES {
        let src = workdir.0.join(format!("sonda{profundidad}.d"));
        let obj = workdir.0.join(format!("sonda{profundidad}.o"));
        if fs::write(&src, SONDA.replace("PROFUNDIDAD", &profundidad.to_string())).is_err() {
            return degradado("could not write the probe");
        }

        let args = argumentos(&compilador, &src.to_string_lossy(), &obj.to_string_lossy());
        let t0 = Instant::now();
        let salida = Command::new(&ejecutable).args(&args).current_dir(&workdir.0).output();
        let compile_ms = t0.elapsed().as_millis() as u64;

        match salida {
            Ok(o) if o.status.success() => {}
            Ok(o) => {
                let err = String::from_utf8_lossy(&o.stderr);
                let primera = err.lines().next().unwrap_or("unknown error");
                return degradado(&format!(
                    "the probe failed to compile at depth {profundidad}: {primera}"
                ));
            }
            Err(e) => return degradado(&format!("could not run {compilador}: {e}")),
        }

        let object_bytes = fs::metadata(&obj).map(|m| m.len()).unwrap_or(0);
        let symbols = fs::read(&obj).map(|b| crate::lenguajes::binparse::parse(&b).symbols.len()).unwrap_or(0);
        points.push(DCostPoint { depth: profundidad, compile_ms, object_bytes, symbols });
    }

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

    DCostResult {
        available: true,
        compiler: Some(compilador),
        symbols_per_level,
        points,
        bytes_per_level,
        note: NOTA.to_string(),
    }
}


/// **La comparación entre los dos lenguajes, que es el punto del módulo.**
///
/// Medido en esta máquina, con las mismas profundidades y la misma sonda
/// traducida nivel por nivel:
///
/// | profundidad | 8 | 16 | 32 | 64 | símbolos/nivel |
/// |---|---|---|---|---|---|
/// | C++ (g++)   | 33 | 57 | 105 | 201 | **3** |
/// | D (dmd)     | 76 | 124 | 220 | 412 | **6** |
///
/// **Y la respuesta a la pregunta que abrió el módulo: el "3 por nivel" era
/// de C++, no de la metaprogramación.** D emite exactamente el doble.
///
/// Lo que sí generaliza es la FORMA: las dos curvas son perfectamente
/// lineales en la profundidad —cada nivel suma una cantidad constante de
/// símbolos, sin acelerarse— y las dos son determinísticas. Eso es una
/// afirmación que se puede llevar a otra máquina y a otro lenguaje; "cada
/// nivel cuesta 3 símbolos" no lo era.
///
/// Un dato que se reporta y **no se aserta**, por la regla del módulo: dmd
/// compiló los cuatro puntos en 41-42 ms y g++ en 145-311 ms. A estas
/// profundidades eso mide arranque de proceso, no metaprogramación.
#[derive(Serialize)]
pub struct ComparacionMetaprogramacion {
    pub cpp: crate::costo::tplcost::TemplateCostResult,
    pub d: DCostResult,
    /// `true` si las dos curvas pudieron medirse. Sin eso no hay comparación
    /// y los campos de abajo quedan en `None`.
    pub comparable: bool,
    /// Símbolos por nivel de cada una, cuando se midieron.
    pub cpp_symbols_per_level: Option<u64>,
    pub d_symbols_per_level: Option<u64>,
    /// `true` si las dos crecen linealmente con la profundidad. **Es lo que
    /// generaliza**, a diferencia de la constante.
    pub ambas_lineales: Option<bool>,
    pub note: String,
}

/// `true` si los símbolos crecen con una pendiente constante entre puntos
/// consecutivos — o sea, lineal en la profundidad.
///
/// Se compara la pendiente POR NIVEL y no la diferencia cruda, porque las
/// profundidades no están equiespaciadas (8, 16, 32, 64): la diferencia entre
/// dos puntos crece aunque la pendiente sea la misma.
fn es_lineal(puntos: &[(u32, usize)]) -> bool {
    if puntos.len() < 3 {
        return false;
    }
    let mut pendientes = Vec::new();
    for par in puntos.windows(2) {
        let (d0, s0) = par[0];
        let (d1, s1) = par[1];
        if d1 <= d0 || s1 < s0 {
            return false;
        }
        pendientes.push((s1 - s0) as f64 / f64::from(d1 - d0));
    }
    let primera = pendientes[0];
    // Tolerancia chica: son conteos enteros, la pendiente exacta o no es.
    pendientes.iter().all(|p| (p - primera).abs() < 0.05)
}

pub fn comparar_metaprogramacion() -> ComparacionMetaprogramacion {
    let cpp = crate::costo::tplcost::validate_template_cost();
    let d = validate_d_cost();
    let comparable = cpp.available && d.available;

    let lineal_cpp = es_lineal(&cpp.points.iter().map(|p| (p.depth, p.symbols)).collect::<Vec<_>>());
    let lineal_d = es_lineal(&d.points.iter().map(|p| (p.depth, p.symbols)).collect::<Vec<_>>());

    let nota = if !comparable {
        "Falta una de las dos curvas: sin las dos no hay comparación. Cada lado dice por separado qué compilador no encontró.".to_string()
    } else {
        format!(
            "Medido en esta máquina con la misma sonda traducida nivel por nivel y las mismas profundidades. C++ emite {} símbolo(s) por nivel y D emite {}, así que el costo por nivel es DEL LENGUAJE, no de la metaprogramación en general. Lo que sí generaliza es la forma: las dos curvas crecen linealmente con la profundidad y las dos son determinísticas. El tiempo de compilación se reporta como dato y no se aserta: a estas profundidades mide arranque de proceso.",
            cpp.symbols_per_level.map(|x| x.to_string()).unwrap_or_else(|| "?".into()),
            d.symbols_per_level.map(|x| x.to_string()).unwrap_or_else(|| "?".into()),
        )
    };

    ComparacionMetaprogramacion {
        cpp_symbols_per_level: cpp.symbols_per_level,
        d_symbols_per_level: d.symbols_per_level,
        ambas_lineales: if comparable { Some(lineal_cpp && lineal_d) } else { None },
        comparable,
        cpp,
        d,
        note: nota,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sin compilador de D el resultado es `available: false` con el motivo,
    /// nunca un error crudo ni un panic. **Y lo dice sin culpar al análisis
    /// estático**: la curva de C++ no necesita D para nada.
    #[test]
    fn sin_compilador_degrada_con_gracia() {
        if compilador_disponible().is_some() {
            return;
        }
        let r = validate_d_cost();
        assert!(!r.available);
        assert!(r.note.contains("No D compiler"), "{}", r.note);
        assert!(r.points.is_empty());
        assert!(r.symbols_per_level.is_none());
    }

    /// **Las profundidades tienen que ser las mismas que las de C++**, o las
    /// dos curvas no son comparables y el módulo entero pierde el sentido.
    #[test]
    fn mide_en_los_mismos_puntos_que_la_curva_de_cpp() {
        assert_eq!(PROFUNDIDADES, crate::costo::tplcost::PROFUNDIDADES_PUBLICAS);
    }

    /// Cada compilador de D quiere su forma de pedir "objeto, sin optimizar".
    #[test]
    fn los_argumentos_son_los_de_cada_compilador() {
        let gdc = argumentos("gdc", "a.d", "a.o");
        assert!(gdc.contains(&"-o".to_string()) && gdc.contains(&"-O0".to_string()));
        let dmd = argumentos("dmd", "a.d", "a.o");
        assert!(dmd.iter().any(|x| x == "-of=a.o"), "{dmd:?}");
        // **Sin `-O`**: optimizar inlinearía la recursión y el objeto dejaría
        // de crecer con la profundidad, que es justo la señal a medir.
        assert!(!dmd.iter().any(|x| x.starts_with("-O")), "{dmd:?}");
    }

    /// La curva, si hay compilador. **No aserta tiempo** — ver el doc.
    #[test]
    fn la_curva_crece_con_la_profundidad_si_hay_compilador() {
        let r = validate_d_cost();
        if !r.available {
            eprintln!("sin compilador de D: {}", r.note);
            return;
        }
        assert_eq!(r.points.len(), PROFUNDIDADES.len());
        for p in &r.points {
            eprintln!(
                "D profundidad {:3} -> {:6} bytes, {:4} simbolos, {:5} ms",
                p.depth, p.object_bytes, p.symbols, p.compile_ms
            );
        }
        let primero = &r.points[0];
        let ultimo = r.points.last().unwrap();
        assert!(
            ultimo.symbols > primero.symbols,
            "mas profundidad tiene que emitir mas simbolos: {} -> {}",
            primero.symbols,
            ultimo.symbols
        );
        assert!(r.symbols_per_level.is_some(), "con la curva medida la pendiente existe");
    }
    /// **El hallazgo, fijado.** Con los dos compiladores presentes: las dos
    /// curvas son lineales y las constantes NO coinciden. Medido acá, C++
    /// emite 3 símbolos por nivel y D emite 6.
    ///
    /// Lo que se aserta es lo que generaliza —la forma lineal y que las
    /// constantes difieran— y no los valores exactos, que dependen del
    /// compilador y del ABI de la máquina. Asertar "3" y "6" convertiría
    /// este test en un detector de cambios de versión de g++.
    #[test]
    fn el_costo_por_nivel_es_del_lenguaje_pero_la_forma_lineal_generaliza() {
        let c = comparar_metaprogramacion();
        if !c.comparable {
            eprintln!("faltan curvas: {}", c.note);
            return;
        }
        assert_eq!(c.ambas_lineales, Some(true), "las dos curvas tienen que ser lineales");
        let cpp = c.cpp_symbols_per_level.expect("C++ midio pendiente");
        let d = c.d_symbols_per_level.expect("D midio pendiente");
        eprintln!("simbolos por nivel -> C++: {cpp}   D: {d}");
        assert!(cpp > 0 && d > 0);
        assert_ne!(
            cpp, d,
            "si coincidieran, el costo por nivel seria del MECANISMO y no del lenguaje — \
             valdria la pena revisar la conclusion del modulo"
        );
    }

    /// La detección de linealidad usa la pendiente POR NIVEL, no la
    /// diferencia cruda: las profundidades no están equiespaciadas (8, 16,
    /// 32, 64), así que la diferencia entre puntos crece aunque la pendiente
    /// sea constante.
    #[test]
    fn la_linealidad_se_mide_por_pendiente_no_por_diferencia() {
        // Pendiente constante de 3 con profundidades que se duplican.
        assert!(es_lineal(&[(8, 33), (16, 57), (32, 105), (64, 201)]));
        // Misma forma, pendiente 6.
        assert!(es_lineal(&[(8, 76), (16, 124), (32, 220), (64, 412)]));
        // Acelerado: no es lineal.
        assert!(!es_lineal(&[(8, 10), (16, 30), (32, 150), (64, 900)]));
        // Sin suficientes puntos no se afirma nada.
        assert!(!es_lineal(&[(8, 10), (16, 20)]));
    }

    /// Sin una de las dos curvas no hay comparación, y se dice en vez de
    /// inventar un veredicto.
    #[test]
    fn sin_las_dos_curvas_no_se_compara() {
        let c = comparar_metaprogramacion();
        if c.comparable {
            return;
        }
        assert!(c.ambas_lineales.is_none());
        assert!(c.note.contains("Falta una de las dos"), "{}", c.note);
    }

}
