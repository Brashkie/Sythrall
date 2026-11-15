//! **El puente que faltaba: del código fuente al código máquina.**
//!
//! Sythrall tenía las dos puntas y ningún camino entre ellas. Por un lado
//! analiza fuente (Big-O, complejidad, smells); por el otro, `asmparse.rs`
//! son 852 líneas que leen ensamblador AT&T/Intel/ARM —instrucciones por
//! categoría, registros, saltos hacia atrás—, pero **solo si alguien le pega
//! ensamblador a mano**. Nadie lo hace. El paso que une las dos mitades es
//! el que cualquier compilador ya sabe hacer: `gcc -S`, `rustc --emit asm`,
//! `zig build-obj -femit-asm`.
//!
//! Con eso, una pregunta que antes no se podía hacer pasa a tener respuesta
//! medida: *"¿qué cambia realmente entre `-O0` y `-O2` en esta función?"*.
//! Sobre una suma de un arreglo en C, medido acá: 19 instrucciones contra 13,
//! y **9 movimientos de datos contra 3** — el optimizador deja de volcar a la
//! pila y mantiene los valores en registros. Eso es un hecho del binario, no
//! una opinión sobre el fuente.
//!
//! ## Lo que este módulo NO hace, dicho de entrada
//!
//! - **No ejecuta el código.** Todos los compiladores se invocan en modo
//!   "emitir ensamblador" (`-S`), que se detiene antes de ensamblar y antes
//!   de linkear. Es estrictamente menos que lo que ya hacen `zig_bench.rs` y
//!   `fortran_bench.rs`, que compilan Y corren para medir tiempos reales.
//! - **No es un disassembler.** Lee lo que el compilador emite, no lo que hay
//!   adentro de un binario ya armado — eso sigue siendo trabajo de Capstone y
//!   sigue explícitamente afuera, igual que en `binparse.rs`.
//! - **No inventa toolchains.** Si `gcc` no está, lo dice (`available: false`)
//!   y no falla: misma degradación con gracia que el resto del motor.
//!
//! ## Sobre los argumentos
//!
//! **Nada que venga del usuario se interpola en la línea de comandos.** El
//! lenguaje y el nivel de optimización se validan contra una lista fija y se
//! traducen a flags escritos acá; el código del usuario viaja por un archivo,
//! nunca por un argumento. Tampoco hay shell de por medio: `Command` ejecuta
//! el binario directo.

use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::lenguajes::asmparse::{self, AsmParseResult};
use crate::bench::benchsupport::TempWorkDir;

/// Un lenguaje que se puede compilar a ensamblador, con su toolchain.
struct Lenguaje {
    /// Como lo nombra el pedido.
    id: &'static str,
    /// Binario a invocar.
    programa: &'static str,
    /// Extensión del archivo fuente temporal.
    extension: &'static str,
}

const LENGUAJES: &[Lenguaje] = &[
    Lenguaje { id: "c", programa: "gcc", extension: "c" },
    Lenguaje { id: "cpp", programa: "g++", extension: "cpp" },
    Lenguaje { id: "rust", programa: "rustc", extension: "rs" },
    Lenguaje { id: "zig", programa: "zig", extension: "zig" },
    Lenguaje { id: "fortran", programa: "gfortran", extension: "f90" },
    // **`gnat-gcc`, no `gcc`, y la diferencia no es cosmetica.** En esta
    // maquina hay un `gcc` en el PATH que anuncia Ada en su `languages=`
    // pero no trae el frontend `gnat1`: compilar Ada con el falla con un
    // `CreateProcess: No such file or directory`. `toolchain::resolver`
    // busca el de GNAT por su nombre propio para no agarrar el equivocado.
    Lenguaje { id: "ada", programa: "gnat-gcc", extension: "adb" },
];

/// Niveles aceptados. **Lista fija**: lo que llega del pedido se busca acá y
/// lo que se pasa al compilador es la cadena de esta tabla, nunca la del
/// usuario.
const NIVELES: &[&str] = &["0", "1", "2", "3", "s"];

/// Tope de líneas que se devuelven en el campo `assembly`.
const MAX_LINEAS_DEVUELTAS: usize = 4000;

/// **Extensiones de CPU que se pueden pedir, y su flag por toolchain.**
///
/// Sin esto el puente no sirve para la pregunta que más interesa —*"¿el
/// compilador vectorizó esto?"*—: `gcc -O3` a secas no emite una sola
/// instrucción SIMD para un bucle que con `-mavx2` da AVX2 de 256 bits
/// (medido acá). Pero un campo de texto libre que termina en la línea de
/// comandos del compilador es exactamente lo que no se hace, así que es una
/// tabla cerrada: lo que llega se busca acá y lo que se pasa es la cadena de
/// esta tabla.
///
/// Zig queda afuera a propósito: no usa flags tipo `-m<feature>` sino
/// `-mcpu=<modelo>`, que es otro vocabulario y merece su propia tabla, no un
/// mapeo a medias.
struct Feature {
    id: &'static str,
    gcc: &'static str,
    rustc: &'static str,
}

const FEATURES: &[Feature] = &[
    Feature { id: "sse2", gcc: "-msse2", rustc: "-Ctarget-feature=+sse2" },
    Feature { id: "sse4.2", gcc: "-msse4.2", rustc: "-Ctarget-feature=+sse4.2" },
    Feature { id: "avx", gcc: "-mavx", rustc: "-Ctarget-feature=+avx" },
    Feature { id: "avx2", gcc: "-mavx2", rustc: "-Ctarget-feature=+avx2" },
    Feature { id: "avx512f", gcc: "-mavx512f", rustc: "-Ctarget-feature=+avx512f" },
    Feature { id: "fma", gcc: "-mfma", rustc: "-Ctarget-feature=+fma" },
];

#[derive(Serialize)]
pub struct CompilacionResult {
    /// `false` si el toolchain no está instalado — no es un error, es una
    /// capacidad ausente.
    pub available: bool,
    pub language: String,
    pub toolchain: String,
    /// Versión reportada por el compilador, primera línea de `--version`.
    pub toolchain_version: String,
    pub opt_level: String,
    /// Extensiones de CPU efectivamente pedidas (las que estaban en la tabla).
    pub features: Vec<String>,
    /// El ensamblador generado. **Puede venir recortado** — ver
    /// `assembly_truncated`; `assembly_lines` siempre trae el total real.
    pub assembly: String,
    /// Cuántas líneas tiene en total, recortado o no. Es el número con el que
    /// se comparan dos niveles sin leer el texto.
    pub assembly_lines: usize,
    /// `true` si `assembly` se recortó para la respuesta.
    pub assembly_truncated: bool,
    /// `asmparse` corriendo sobre lo generado: el resto del módulo ya existía.
    pub analysis: Option<AsmParseResult>,
    /// `stderr` del compilador si falló. Un error de compilación del usuario
    /// NO es un error de Sythrall: se devuelve para que lo lea.
    pub error: Option<String>,
}

fn vacio(language: &str, toolchain: &str, opt: &str, error: Option<String>, available: bool) -> CompilacionResult {
    CompilacionResult {
        available,
        language: language.to_string(),
        toolchain: toolchain.to_string(),
        toolchain_version: String::new(),
        opt_level: opt.to_string(),
        features: Vec::new(),
        assembly: String::new(),
        assembly_lines: 0,
        assembly_truncated: false,
        analysis: None,
        error,
    }
}

/// Versión del toolchain, o `None` si no está instalado.
///
/// **Se prueban las dos formas y gana la primera que sirva.** `zig --version`
/// no existe: imprime el uso general y sale con código de error, así que un
/// `return None` ante el primer fallo daba "zig no está instalado" con zig
/// instalado. La forma canónica es `zig version`. Y como la salida de uso sí
/// trae texto, tampoco alcanza con mirar si stdout está vacío: una línea que
/// dice "Usage" no es un número de versión.
/// Acepta un `OsStr` y no un `&str` porque desde que el programa se resuelve
/// por `toolchain::resolver` puede ser una ruta absoluta, no un nombre.
fn version_de(programa: &std::ffi::OsStr) -> Option<String> {
    for arg in ["--version", "version"] {
        let Ok(out) = Command::new(programa).arg(arg).output() else {
            // El binario no existe: probar el otro argumento no va a ayudar.
            return None;
        };
        let texto = String::from_utf8_lossy(&out.stdout);
        let primera = texto.lines().next().unwrap_or("").trim().to_string();
        if primera.is_empty() || primera.to_ascii_lowercase().contains("usage") {
            continue;
        }
        return Some(primera);
    }
    None
}

/// Compila `content` a ensamblador y lo analiza con `asmparse`.
/// El nombre de la primera unidad de un fuente Ada, en minusculas, que es
/// como GNAT espera que se llame el archivo.
///
/// Si no se puede determinar se usa `fuente`: GNAT igual compila y solo avisa,
/// asi que el peor caso es un warning, no un fallo.
fn unidad_de_ada(content: &str) -> String {
    let nombre = crate::lenguajes::adaparse::parse_ada(content).and_then(|r| {
        r.units
            .first()
            .map(|u| u.name.clone())
            .or_else(|| r.functions.first().map(|f| f.name.clone()))
    });
    match nombre {
        // Una unidad hija se escribe `Padre.Hijo` y su archivo es
        // `padre-hijo.adb`: el punto se vuelve guion.
        Some(n) if !n.is_empty() && n != "<anonymous>" => n.to_lowercase().replace('.', "-"),
        _ => "fuente".to_string(),
    }
}

pub fn compilar_a_asm(language: &str, content: &str, opt_level: &str) -> CompilacionResult {
    compilar_con_features(language, content, opt_level, &[])
}

/// Igual, pidiendo además extensiones de CPU. Las que no estén en `FEATURES`
/// se ignoran en silencio — no se pasa nada que no venga de la tabla.
pub fn compilar_con_features(
    language: &str,
    content: &str,
    opt_level: &str,
    features: &[String],
) -> CompilacionResult {
    let Some(lang) = LENGUAJES.iter().find(|l| l.id == language) else {
        return vacio(language, "", opt_level, Some(format!("lenguaje no soportado: `{language}`")), false);
    };
    // El nivel que se usa es el de la tabla, no el que llegó.
    let Some(nivel) = NIVELES.iter().find(|n| **n == opt_level) else {
        return vacio(language, lang.programa, opt_level, Some(format!("nivel de optimización no soportado: `{opt_level}`")), false);
    };

    // **Se resuelve, no se asume que esta en el PATH.** `dmd` y el `gcc` de
    // GNAT estan instalados fuera de el a proposito; sin esto, los dos
    // lenguajes que dependen de ellos degradan diciendo "no esta instalado"
    // cuando si lo esta.
    let Some(ejecutable) = crate::costo::toolchain::resolver(lang.programa) else {
        return vacio(
            language,
            lang.programa,
            nivel,
            Some(format!(
                "`{}` no está instalado. Si lo tenés en otra ruta, apuntá `{}` a su ejecutable.",
                lang.programa,
                crate::costo::toolchain::nombre_de_variable(lang.programa)
            )),
            false,
        );
    };
    let Some(version) = version_de(&ejecutable) else {
        return vacio(
            language,
            lang.programa,
            nivel,
            Some(format!("`{}` no respondió al pedirle la versión", lang.programa)),
            false,
        );
    };

    let Ok(dir) = TempWorkDir::new("cmpasm") else {
        return vacio(language, lang.programa, nivel, Some("no se pudo crear el directorio temporal".into()), true);
    };
    // **Ada quiere que el archivo se llame como la unidad.** Con otro
    // nombre GNAT compila igual pero avisa (`file name does not match unit
    // name`), y ese warning saldria en la salida de todos los analisis de Ada
    // como si fuera un hallazgo sobre el codigo del usuario. El nombre se
    // saca del parser que la Linea E ya tiene, asi que el paso 1 le sirve al
    // paso 2 en vez de ser dos trabajos sueltos.
    let base = if lang.id == "ada" { unidad_de_ada(content) } else { "fuente".to_string() };
    let src = dir.0.join(format!("{base}.{}", lang.extension));
    let out = dir.0.join("salida.s");
    if std::fs::write(&src, content).is_err() {
        return vacio(language, lang.programa, nivel, Some("no se pudo escribir el fuente".into()), true);
    }

    let pedidas: Vec<&Feature> =
        features.iter().filter_map(|f| FEATURES.iter().find(|x| x.id == f.as_str())).collect();

    let mut cmd = Command::new(&ejecutable);
    match lang.id {
        "rust" => {
            // `--crate-type=lib` para que no exija un `main`; el objetivo es
            // mirar UNA función, no armar un programa.
            cmd.arg("--emit=asm").arg("--crate-type=lib").arg(format!("-Copt-level={nivel}"));
            for f in &pedidas {
                cmd.arg(f.rustc);
            }
            cmd.arg("-o").arg(&out).arg(&src);
        }
        "zig" => {
            // Zig no tiene `-O0..-O3`: tiene modos con nombre.
            let modo = if *nivel == "0" { "Debug" } else { "ReleaseFast" };
            cmd.arg("build-obj")
                .arg("-O")
                .arg(modo)
                .arg(format!("-femit-asm={}", out.display()))
                .arg("-fno-emit-bin")
                .arg(&src);
        }
        _ => {
            // gcc / g++ / gfortran comparten la forma.
            cmd.arg("-S").arg(format!("-O{nivel}"));
            for f in &pedidas {
                cmd.arg(f.gcc);
            }
            cmd.arg("-o").arg(&out).arg(&src);
        }
    }
    cmd.current_dir(&dir.0);

    let salida = match cmd.output() {
        Ok(s) => s,
        Err(e) => return vacio(language, lang.programa, nivel, Some(format!("no se pudo ejecutar: {e}")), true),
    };

    if !salida.status.success() {
        let err = String::from_utf8_lossy(&salida.stderr).trim().to_string();
        let mut r = vacio(language, lang.programa, nivel, Some(err), true);
        r.toolchain_version = version;
        r.features = pedidas.iter().map(|f| f.id.to_string()).collect();
        return r;
    }

    let asm = std::fs::read_to_string(&out).unwrap_or_default();
    let lineas = asm.lines().count();
    // **El análisis corre sobre el texto COMPLETO; lo que se recorta es solo
    // lo que viaja de vuelta.** Zig en modo `Debug` emite 312.286 líneas para
    // una suma de diez — se lleva adentro la biblioteca estándar y los
    // chequeos de seguridad—, y mandar eso por JSON ahoga a cualquier UI. Los
    // conteos siguen siendo los de verdad.
    let analisis = if asm.trim().is_empty() { None } else { Some(asmparse::parse(&asm)) };

    // **Compilar bien y no emitir nada son cosas distintas, y callarse la
    // segunda sería lo peor que puede hacer este módulo**: el usuario vería
    // un panel vacío sin saber por qué.
    //
    // Pasa de verdad, y con Rust en particular: un `pub fn` común de un
    // crate de biblioteca se emite a `-O0` (169 líneas, medido) y a `-O2`
    // **no se emite** (7 líneas, ninguna suya). No es un error — nada dentro
    // del crate lo llama, y el cuerpo de una función pública no genérica
    // viaja en los metadatos para que quien la use la inline, así que el
    // objeto queda sin ella. `#[inline(never)]` o `#[no_mangle]` la fuerzan:
    // las dos dan 62 líneas con la función adentro.
    let instrucciones: usize = analisis
        .as_ref()
        .map(|a| a.procedures.iter().map(|p| p.instructions.len()).sum())
        .unwrap_or(0);
    let error = if instrucciones == 0 {
        Some(if lang.id == "rust" {
            "compiló sin errores pero no emitió ninguna instrucción. En un crate de biblioteca,              rustc descarta una `pub fn` no genérica que nadie llama en cuanto se optimiza: su              cuerpo viaja en los metadatos para que quien la use la pueda inline, y el objeto              queda sin ella. Anotarla con `#[inline(never)]` o `#[no_mangle]` la fuerza a salir."
                .to_string()
        } else {
            "compiló sin errores pero no emitió ninguna instrucción — revisá que el fuente              contenga una función y no solo declaraciones."
                .to_string()
        })
    } else {
        None
    };

    let truncado = lineas > MAX_LINEAS_DEVUELTAS;
    let texto = if truncado {
        let mut t: String = asm.lines().take(MAX_LINEAS_DEVUELTAS).collect::<Vec<_>>().join("
");
        t.push_str(&format!(
            "

; … recortado: {} de {lineas} líneas. El análisis de arriba cubre el archivo entero.",
            lineas - MAX_LINEAS_DEVUELTAS
        ));
        t
    } else {
        asm
    };

    CompilacionResult {
        available: true,
        language: language.to_string(),
        toolchain: lang.programa.to_string(),
        toolchain_version: version,
        opt_level: nivel.to_string(),
        features: pedidas.iter().map(|f| f.id.to_string()).collect(),
        assembly: texto,
        assembly_lines: lineas,
        assembly_truncated: truncado,
        analysis: analisis,
        error,
    }
}

/// Compila el MISMO fuente a varios niveles. Es la forma en que esto se usa
/// de verdad: un número suelto no dice nada, la comparación sí.
pub fn comparar_niveles(
    language: &str,
    content: &str,
    niveles: &[String],
    features: &[String],
) -> Vec<CompilacionResult> {
    niveles.iter().map(|n| compilar_con_features(language, content, n, features)).collect()
}


// ─── Búsqueda multi-objetivo sobre la configuración de compilación ───────────
//
// **El último de los cinco temas reinstaurados, entrado por la puerta que sí
// se sostiene.** El bullet de programación genética proponía *evolucionar
// ASTs contra el validador de la Fase 26*, y decía que lo difícil —validar—
// ya estaba resuelto. No lo está: la Fase 26 valida los kernels que **Sythrall
// mismo escribe**, donde la equivalencia se conoce porque la escribió él.
// Para una función cualquiera del usuario, probar que una variante mutada
// sigue siendo equivalente es indecidible, y "lo corrí con diez entradas y dio
// igual" no es una prueba.
//
// **Así que acá no se toca el fuente.** Lo que se busca es la CONFIGURACIÓN
// del compilador: mismo código, distintas opciones. La equivalencia deja de
// ser un problema —no hay nada que probar, el programa es el mismo— y la
// búsqueda sigue siendo multi-objetivo de verdad, que es lo que el bullet
// nombraba con NSGA-II / frente de Pareto.
//
// **Y hay un trade-off real, medido antes de escribir esto.** Sobre un
// producto escalar-vector en C, con nueve configuraciones:
//
//     -Os          14 instrucciones,  0 packed   ← el código más chico
//     -O1/-O2/-O3  19 instrucciones,  0 packed
//     -O0          23 instrucciones,  0 packed
//     -O3 +avx2   195 instrucciones,  2 packed   ← el único que vectoriza
//     -O3 +avx512 313 instrucciones,  2 packed
//
// El frente son **dos** puntos: `-Os` y `-O3 +avx2`. Las otras siete están
// estrictamente dominadas — hay otra configuración que no es peor en ningún
// objetivo y es mejor en alguno. Eso es información que ningún número suelto
// da.
//
// ## Lo que esto NO dice
//
// **Menos instrucciones no es más rápido.** El conteo es un proxy de tamaño,
// no una medición de tiempo: el bucle vectorizado de 195 instrucciones puede
// correr mucho más rápido que el escalar de 14, justamente porque desenrolla.
// El frente **presenta el compromiso, no lo resuelve** — ordenarlo exigiría
// un peso entre tamaño y velocidad que depende del proyecto, no del análisis.
// Medir el tiempo de verdad es trabajo de `*_bench.rs`, que ya existe y
// compila Y ejecuta.
//
// **Y pedir una extensión no la pone en el CPU donde el binario va a correr.**
// Compilar con `-mavx512f` produce código que revienta con instrucción ilegal
// en una máquina sin AVX-512. El frente dice qué emite el compilador, no qué
// soporta el destino.

/// Una configuración de compilación: nivel más extensiones.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConfigCompilacion {
    pub opt_level: String,
    #[serde(default)]
    pub features: Vec<String>,
}

impl ConfigCompilacion {
    /// Etiqueta estable, la misma que se usa para nombrar quién domina a quién.
    fn etiqueta(&self) -> String {
        if self.features.is_empty() {
            format!("-O{}", self.opt_level)
        } else {
            format!("-O{} +{}", self.opt_level, self.features.join(" +"))
        }
    }
}

#[derive(Serialize)]
pub struct PuntoPareto {
    pub label: String,
    pub config: ConfigCompilacion,
    /// Objetivo 1: a minimizar.
    pub instructions: usize,
    /// Objetivo 2: a maximizar. Son las instrucciones que operan sobre varios
    /// elementos a la vez — ver `asmparse::SimdInfo` para por qué packed y no
    /// "usa registros vectoriales".
    pub packed: usize,
    pub assembly_lines: usize,
    pub vectorized: bool,
    /// Etiquetas de las configuraciones que lo dominan. Vacío = está en el
    /// frente.
    pub dominated_by: Vec<String>,
    /// `Some` si esta configuración no se pudo compilar — no saca a las demás.
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct BusquedaPareto {
    pub language: String,
    pub toolchain: String,
    pub toolchain_version: String,
    pub points: Vec<PuntoPareto>,
    /// Etiquetas de los puntos no dominados.
    pub front: Vec<String>,
    pub note: String,
    pub error: Option<String>,
}

const NOTA_PARETO: &str = "El frente presenta el compromiso, no lo resuelve: MENOS INSTRUCCIONES NO ES MÁS RÁPIDO. El conteo mide tamaño del código emitido, no tiempo — un bucle vectorizado con más instrucciones suele correr más rápido justamente porque desenrolla. Ordenar el frente exigiría un peso entre tamaño y velocidad que depende del proyecto. Medir tiempo real es trabajo de los kernels de benchmark, que compilan Y ejecutan. Además, pedir una extensión de CPU no la pone en la máquina destino: `-mavx512f` emite código que falla con instrucción ilegal en un CPU sin AVX-512.";

/// La grilla por defecto. Chica a propósito: cada punto es una invocación
/// real del compilador, así que el costo es lineal en configuraciones y el
/// usuario paga ese tiempo.
pub fn configs_por_defecto() -> Vec<ConfigCompilacion> {
    let simples = ["0", "2", "3", "s"];
    let mut out: Vec<ConfigCompilacion> = simples
        .iter()
        .map(|n| ConfigCompilacion { opt_level: n.to_string(), features: Vec::new() })
        .collect();
    // Solo los niveles que pueden vectorizar valen la pena con AVX: pedirlo a
    // `-O0` agrega una compilación que no puede cambiar nada.
    for n in ["2", "3"] {
        out.push(ConfigCompilacion {
            opt_level: n.to_string(),
            features: vec!["avx2".to_string()],
        });
    }
    out
}

/// `a` domina a `b` si no es peor en ningún objetivo y es mejor en alguno.
///
/// Objetivos: **minimizar** instrucciones, **maximizar** packed. Una
/// configuración que falló no domina ni es dominada — no tiene valores que
/// comparar.
fn domina(a: &PuntoPareto, b: &PuntoPareto) -> bool {
    if a.error.is_some() || b.error.is_some() {
        return false;
    }
    let no_peor = a.instructions <= b.instructions && a.packed >= b.packed;
    let mejor_en_algo = a.instructions < b.instructions || a.packed > b.packed;
    no_peor && mejor_en_algo
}

/// Compila el mismo fuente con varias configuraciones y devuelve el frente de
/// Pareto sobre (instrucciones ↓, packed ↑).
pub fn buscar_pareto(
    language: &str,
    content: &str,
    configs: &[ConfigCompilacion],
) -> BusquedaPareto {
    let mut points: Vec<PuntoPareto> = Vec::new();
    let mut toolchain = String::new();
    let mut version = String::new();
    let mut error_global: Option<String> = None;

    for cfg in configs {
        let r = compilar_con_features(language, content, &cfg.opt_level, &cfg.features);
        if toolchain.is_empty() {
            toolchain = r.toolchain.clone();
        }
        if version.is_empty() && !r.toolchain_version.is_empty() {
            version = r.toolchain_version.clone();
        }
        if !r.available && error_global.is_none() {
            error_global = r.error.clone();
        }
        let (instr, packed, vect) = match &r.analysis {
            Some(a) => {
                let i: usize = a.procedures.iter().map(|p| p.instructions.len()).sum();
                let pk: usize = a
                    .procedures
                    .iter()
                    .filter_map(|p| p.simd.as_ref())
                    .map(|s| s.packed_instructions)
                    .sum();
                (i, pk, pk > 0)
            }
            None => (0, 0, false),
        };
        points.push(PuntoPareto {
            label: cfg.etiqueta(),
            config: cfg.clone(),
            instructions: instr,
            packed,
            assembly_lines: r.assembly_lines,
            vectorized: vect,
            dominated_by: Vec::new(),
            error: r.error.clone(),
        });
    }

    // Comparación todos contra todos. Con una grilla de media docena de
    // configuraciones el costo es irrelevante al lado de invocar el
    // compilador una vez por punto.
    for i in 0..points.len() {
        let mut dominadores = Vec::new();
        for j in 0..points.len() {
            if i != j && domina(&points[j], &points[i]) {
                dominadores.push(points[j].label.clone());
            }
        }
        points[i].dominated_by = dominadores;
    }

    let front: Vec<String> = points
        .iter()
        .filter(|p| p.error.is_none() && p.dominated_by.is_empty())
        .map(|p| p.label.clone())
        .collect();

    BusquedaPareto {
        language: language.to_string(),
        toolchain,
        toolchain_version: version,
        points,
        front,
        note: NOTA_PARETO.to_string(),
        error: error_global,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUMA_C: &str = "int suma(const int *a, int n) {\n    int t = 0;\n    for (int i = 0; i < n; i++) t += a[i];\n    return t;\n}\n";

    fn hay(programa: &str) -> bool {
        version_de(std::ffi::OsStr::new(programa)).is_some()
    }

    #[test]
    fn un_lenguaje_desconocido_no_panickea() {
        let r = compilar_a_asm("cobol", "x", "2");
        assert!(!r.available);
        assert!(r.error.unwrap().contains("cobol"));
    }

    /// **El nivel que llega NO se pasa tal cual.** Sin la lista fija, un
    /// `opt_level` arbitrario terminaría concatenado en un flag.
    #[test]
    fn un_nivel_fuera_de_la_lista_se_rechaza() {
        let r = compilar_a_asm("c", SUMA_C, "2; rm -rf /");
        assert!(!r.available);
        assert!(r.error.unwrap().contains("nivel de optimización"));
    }

    #[test]
    fn sin_toolchain_degrada_con_gracia_en_vez_de_fallar() {
        // No se puede desinstalar gcc en un test, así que se ejercita el
        // camino con un lenguaje cuyo compilador casi seguro no está.
        let r = compilar_a_asm("zig", "x", "2");
        // Con zig instalado será un error de compilación (available=true);
        // sin zig, available=false. Las dos son respuestas, ninguna panickea.
        assert!(r.error.is_some() || r.analysis.is_some());
    }

    #[test]
    fn c_a_ensamblador_end_to_end_si_gcc_esta() {
        if !hay("gcc") {
            return;
        }
        let r = compilar_a_asm("c", SUMA_C, "2");
        assert!(r.available, "error: {:?}", r.error);
        assert!(r.error.is_none(), "{:?}", r.error);
        assert!(r.assembly_lines > 5, "ensamblador vacío");
        let a = r.analysis.expect("sin análisis");
        // **La función tiene que salir ENTERA, no partida en etiquetas
        // locales** — es el bug que motivó todo esto.
        let suma = a.procedures.iter().find(|p| p.name.contains("suma")).expect("sin la función");
        assert!(suma.instructions.len() >= 8, "solo {} instrucciones", suma.instructions.len());
        assert_eq!(suma.big_o, "O(n)", "el bucle tiene que verse");
    }

    /// El resultado que justifica el módulo entero: dos niveles del MISMO
    /// fuente dan binarios distintos, y la diferencia es medible.
    #[test]
    fn o0_y_o2_dan_codigo_distinto_y_la_diferencia_se_mide() {
        if !hay("gcc") {
            return;
        }
        let rs = comparar_niveles("c", SUMA_C, &["0".to_string(), "2".to_string()], &[]);
        assert_eq!(rs.len(), 2);
        let (o0, o2) = (&rs[0], &rs[1]);
        if o0.error.is_some() || o2.error.is_some() {
            return;
        }
        let cuenta = |r: &CompilacionResult| {
            r.analysis
                .as_ref()
                .map(|a| a.procedures.iter().map(|p| p.instructions.len()).sum::<usize>())
                .unwrap_or(0)
        };
        let (n0, n2) = (cuenta(o0), cuenta(o2));
        assert!(n0 > 0 && n2 > 0, "O0={n0} O2={n2}");
        assert!(n2 < n0, "-O2 debería emitir menos instrucciones que -O0 (O0={n0}, O2={n2})");
    }

    #[test]
    fn un_fuente_que_no_compila_devuelve_el_error_del_compilador() {
        if !hay("gcc") {
            return;
        }
        let r = compilar_a_asm("c", "int f( { esto no es C\n", "2");
        assert!(r.available, "el toolchain SÍ está");
        assert!(r.error.is_some(), "un fuente roto tiene que devolver el error");
        assert!(r.analysis.is_none());
    }
    /// **`zig --version` no existe**: imprime el uso general y sale con
    /// código de error. La forma canónica es `zig version`. Probar solo la
    /// primera daba "zig no está instalado" con zig instalado.
    #[test]
    fn la_version_se_detecta_aunque_el_toolchain_no_acepte_doble_guion_version() {
        // `rustc --version` sí existe: sirve de control de que la función
        // devuelve algo razonable y no la cadena de uso.
        if let Some(v) = version_de(std::ffi::OsStr::new("rustc")) {
            assert!(!v.to_ascii_lowercase().contains("usage"), "devolvió el uso: {v}");
            assert!(v.to_ascii_lowercase().contains("rustc"), "{v}");
        }
    }

    /// Un binario que no existe no cuelga ni panickea.
    #[test]
    fn un_toolchain_inexistente_devuelve_none() {
        assert!(version_de(std::ffi::OsStr::new("no_existe_este_compilador_xyz")).is_none());
    }

    /// **Compilar bien y emitir algo son cosas distintas.** Un `pub fn` común
    /// de un crate de biblioteca desaparece al optimizar; el módulo tiene que
    /// decirlo, no devolver un panel vacío en silencio.
    #[test]
    fn rust_sin_anotacion_y_optimizado_explica_por_que_no_emitio() {
        if version_de(std::ffi::OsStr::new("rustc")).is_none() {
            return;
        }
        let src = "pub fn suma(a: &[i32]) -> i32 { let mut t = 0; for x in a { t += *x; } t }\n";
        let r = compilar_a_asm("rust", src, "2");
        assert!(r.available);
        let err = r.error.expect("tenía que explicar por qué no emitió");
        assert!(err.contains("inline(never)"), "el mensaje tiene que decir cómo resolverlo: {err}");
    }

    /// Y con la anotación sí sale, que es lo que vuelve accionable al mensaje
    /// de arriba.
    #[test]
    fn rust_con_inline_never_si_emite_la_funcion() {
        if version_de(std::ffi::OsStr::new("rustc")).is_none() {
            return;
        }
        let src = "#[inline(never)]\npub fn suma(a: &[i32]) -> i32 { let mut t = 0; for x in a { t += *x; } t }\n";
        let r = compilar_a_asm("rust", src, "2");
        assert!(r.error.is_none(), "{:?}", r.error);
        let a = r.analysis.expect("sin análisis");
        let total: usize = a.procedures.iter().map(|p| p.instructions.len()).sum();
        assert!(total > 0, "no emitió instrucciones");
    }

    /// Fortran entra por el mismo camino que C — comparten la forma `-S -O`.
    #[test]
    fn fortran_a_ensamblador_si_gfortran_esta() {
        if !hay("gfortran") {
            return;
        }
        let src = "integer function suma(a, n)\n  integer :: a(n), n, i, t\n  t = 0\n  do i = 1, n\n    t = t + a(i)\n  end do\n  suma = t\nend function\n";
        let r = compilar_a_asm("fortran", src, "2");
        assert!(r.available, "{:?}", r.error);
        assert!(r.error.is_none(), "{:?}", r.error);
        let a = r.analysis.expect("sin análisis");
        assert!(a.procedures.iter().any(|p| p.name.contains("suma")), "{:?}", a.procedures.iter().map(|p| &p.name).collect::<Vec<_>>());
    }

    /// **Zig en `Debug` emite 312.286 líneas para una suma de diez**: se
    /// lleva adentro la biblioteca estándar y los chequeos de seguridad. El
    /// texto se recorta para la respuesta, pero el conteo y el análisis son
    /// los del archivo entero.
    #[test]
    fn un_ensamblador_enorme_se_recorta_pero_el_conteo_es_el_real() {
        if !hay("zig") {
            return;
        }
        let src = "export fn suma(a: [*]const i32, n: usize) i32 {
    var t: i32 = 0;
    var i: usize = 0;
    while (i < n) : (i += 1) { t += a[i]; }
    return t;
}
";
        let r = compilar_a_asm("zig", src, "0");
        if !r.available || r.error.is_some() {
            return;
        }
        if r.assembly_lines > MAX_LINEAS_DEVUELTAS {
            assert!(r.assembly_truncated, "tenía que marcarse como recortado");
            assert!(r.assembly.lines().count() <= MAX_LINEAS_DEVUELTAS + 4, "el texto no se recortó");
            assert!(r.assembly.contains("recortado"), "el texto tiene que decir que se recortó");
        }
    }

    const VECTORIZABLE_C: &str =
        "void escalar_por_vector(float *a, const float *b, float k, int n) {\n    for (int i = 0; i < n; i++) a[i] = b[i] * k;\n}\n";

    /// **El círculo completo: fuente → compilador → ensamblador → SIMD.**
    /// Es la pregunta que el motor no podía responder y ahora sí: *"¿esto se
    /// vectorizó?"*. Y la respuesta depende de lo que se le pida al
    /// compilador, que es justo el punto.
    #[test]
    fn pedir_avx2_vectoriza_y_sin_pedirlo_no() {
        if !hay("gcc") {
            return;
        }
        let sin = compilar_con_features("c", VECTORIZABLE_C, "3", &[]);
        let con = compilar_con_features("c", VECTORIZABLE_C, "3", &["avx2".to_string()]);
        if sin.error.is_some() || con.error.is_some() {
            return;
        }
        assert_eq!(con.features, vec!["avx2".to_string()], "la feature tiene que quedar registrada");

        let simd_de = |r: &CompilacionResult| {
            r.analysis.as_ref().and_then(|a| a.procedures.iter().find_map(|p| p.simd.clone()))
        };
        // Con AVX2 el bucle sale vectorizado y de 256 bits.
        let s = simd_de(&con).expect("con -mavx2 tenía que haber SIMD");
        assert!(s.vectorized, "no vectorizó: {s:?}");
        assert_eq!(s.max_width_bits, 256);
        assert_eq!(s.isa, "avx2");
        // Sin pedir nada, este gcc no emite SIMD para el mismo bucle.
        assert!(
            simd_de(&sin).is_none_or(|x| !x.vectorized),
            "sin -mavx2 no debería reportar vectorización"
        );
    }

    /// **Una feature que no está en la tabla se ignora, no se pasa.** Es lo
    /// que impide que este campo sea un agujero hacia la línea de comandos
    /// del compilador.
    #[test]
    fn una_feature_fuera_de_la_tabla_no_llega_al_compilador() {
        if !hay("gcc") {
            return;
        }
        let r = compilar_con_features(
            "c",
            VECTORIZABLE_C,
            "2",
            &["-o /tmp/pwned".to_string(), "avx2".to_string()],
        );
        // Solo sobrevive la conocida.
        assert_eq!(r.features, vec!["avx2".to_string()]);
        assert!(r.error.is_none(), "{:?}", r.error);
    }

    // ── Frente de Pareto sobre la configuración de compilación ─────────────

    const VECTORIZABLE: &str =
        "void escalar_por_vector(float *a, const float *b, float k, int n) {\n    for (int i = 0; i < n; i++) a[i] = b[i] * k;\n}\n";

    fn punto(label: &str, instr: usize, packed: usize) -> PuntoPareto {
        PuntoPareto {
            label: label.to_string(),
            config: ConfigCompilacion { opt_level: "2".into(), features: vec![] },
            instructions: instr,
            packed,
            assembly_lines: instr,
            vectorized: packed > 0,
            dominated_by: Vec::new(),
            error: None,
        }
    }

    /// **La relación de dominancia, sin compilar nada.** Minimizar
    /// instrucciones, maximizar packed: `a` domina a `b` solo si no es peor en
    /// ninguno de los dos y es mejor en alguno.
    #[test]
    fn la_dominancia_exige_no_ser_peor_en_ningun_objetivo() {
        let chico_escalar = punto("A", 14, 0);
        let mediano_escalar = punto("B", 19, 0);
        let grande_vectorizado = punto("C", 195, 2);

        // Mismo packed, menos instrucciones: domina.
        assert!(domina(&chico_escalar, &mediano_escalar));
        assert!(!domina(&mediano_escalar, &chico_escalar));

        // **Ninguno domina al otro**: uno gana en tamaño, el otro en
        // vectorización. Eso es exactamente un trade-off, y los dos van al
        // frente.
        assert!(!domina(&chico_escalar, &grande_vectorizado));
        assert!(!domina(&grande_vectorizado, &chico_escalar));
    }

    /// Dos configuraciones idénticas en todo no se dominan: hace falta ser
    /// mejor en ALGO.
    #[test]
    fn un_empate_exacto_no_es_dominancia() {
        assert!(!domina(&punto("A", 19, 0), &punto("B", 19, 0)));
    }

    /// Una configuración que no compiló no domina ni es dominada — no tiene
    /// valores que comparar, y tratarla como 0 instrucciones la volvería la
    /// mejor de todas.
    #[test]
    fn una_configuracion_fallida_no_entra_en_la_comparacion() {
        let mut roto = punto("ROTO", 0, 0);
        roto.error = Some("no compiló".into());
        let ok = punto("OK", 19, 0);
        assert!(!domina(&roto, &ok), "un fallo con 0 instrucciones no puede ganar");
        assert!(!domina(&ok, &roto));
    }

    /// **El resultado medido que motivó el módulo.** De nueve
    /// configuraciones, el frente son dos: el código más chico y el único que
    /// vectoriza. Las otras siete están estrictamente dominadas.
    #[test]
    fn sobre_un_bucle_vectorizable_el_frente_tiene_el_mas_chico_y_el_vectorizado() {
        if !hay("gcc") {
            return;
        }
        let r = buscar_pareto("c", VECTORIZABLE, &configs_por_defecto());
        if r.points.iter().all(|p| p.error.is_some()) {
            return;
        }
        assert!(!r.front.is_empty(), "el frente no puede estar vacío: {:?}", r.points.iter().map(|p| (&p.label, p.instructions, p.packed, &p.error)).collect::<Vec<_>>());

        let en_frente: Vec<&PuntoPareto> =
            r.points.iter().filter(|p| r.front.contains(&p.label)).collect();

        // El de menos instrucciones de todos los que compilaron tiene que
        // estar en el frente: nadie puede dominarlo.
        let min_instr = r.points.iter().filter(|p| p.error.is_none()).map(|p| p.instructions).min().unwrap();
        assert!(en_frente.iter().any(|p| p.instructions == min_instr), "el más chico tiene que estar en el frente");

        // Y si alguna vectorizó, el frente tiene que incluir una vectorizada.
        if r.points.iter().any(|p| p.vectorized) {
            assert!(en_frente.iter().any(|p| p.vectorized), "una vectorizada no puede estar dominada por una escalar");
        }
    }

    /// Nadie en el frente domina a nadie más del frente — es la definición.
    #[test]
    fn los_puntos_del_frente_no_se_dominan_entre_si() {
        if !hay("gcc") {
            return;
        }
        let r = buscar_pareto("c", VECTORIZABLE, &configs_por_defecto());
        let frente: Vec<&PuntoPareto> = r.points.iter().filter(|p| r.front.contains(&p.label)).collect();
        for a in &frente {
            for b in &frente {
                if a.label != b.label {
                    assert!(!domina(a, b), "{} domina a {}, no pueden estar los dos", a.label, b.label);
                }
            }
        }
    }

    /// **La nota tiene que decir que el frente no ordena por velocidad.**
    /// Contar instrucciones es un proxy de tamaño; un bucle vectorizado con
    /// más instrucciones suele ser más rápido. Sin ese aviso, el número se
    /// lee al revés.
    #[test]
    fn la_nota_aclara_que_menos_instrucciones_no_es_mas_rapido() {
        let r = buscar_pareto("c", VECTORIZABLE, &[]);
        assert!(r.note.to_uppercase().contains("NO ES MÁS RÁPIDO"), "{}", r.note);
        assert!(r.note.contains("avx512"), "tiene que avisar lo del CPU destino");
    }

    /// Un toolchain ausente no rompe la búsqueda: cada punto trae su error.
    #[test]
    fn sin_toolchain_los_puntos_traen_su_error_y_el_frente_queda_vacio() {
        let r = buscar_pareto("cobol", "x", &configs_por_defecto());
        assert!(r.points.iter().all(|p| p.error.is_some()));
        assert!(r.front.is_empty());
    }

}
