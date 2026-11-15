//! **Encontrar un compilador que no está en el PATH.**
//!
//! Medido el 2026-10-16: ni `dmd` ni el `gcc` de GNAT están en el PATH de esta
//! máquina — ni en el del usuario ni en el del sistema. Los dos se instalaron
//! a propósito fuera de él, y la consecuencia **no era teórica**:
//! `/execution/metaprogramming-cost` respondía *"No D compiler found (tried
//! ldc2, gdc, dmd)"*, así que la comparación C++ vs D —documentada en el
//! ROADMAP y el CHANGELOG con números medidos— estaba muerta en runtime. Los
//! números eran reales; el camino para volver a producirlos, no.
//!
//! **El orden de resolución, y por qué ése.**
//!   1. La variable de entorno `SYTHRALL_TOOLCHAIN_<PROGRAMA>`. Es la que
//!      manda siempre: una máquina distinta de ésta no tiene por qué parecerse
//!      a ésta, y sin una salida explícita este módulo sería una lista de
//!      rutas de una sola computadora.
//!   2. El PATH. Si el programa está donde el sistema lo busca, se usa ése y
//!      no se adivina nada.
//!   3. Las ubicaciones **por defecto de cada instalador**. No son rutas
//!      arbitrarias: `C:\D\dmd2` es donde el instalador de DMD pone las cosas
//!      sin preguntar, y `…\alire\cache\toolchains` es donde Alire guarda los
//!      toolchains que baja. Buscar ahí es asumir que el usuario aceptó el
//!      default, no inventar una convención.
//!
//! **Lo que NO hace: modificar el PATH del sistema.** El usuario ya dijo sobre
//! la instalación de DMD que estaba bien como quedó. Este módulo encuentra, no
//! instala ni configura.

use std::path::PathBuf;
use std::process::Command;

/// El nombre de la variable de entorno que fuerza la ruta de un programa.
///
/// `gnat-gcc` → `SYTHRALL_TOOLCHAIN_GNAT_GCC`. Todo lo que no es alfanumérico
/// pasa a `_` para que el nombre sea válido en cualquier shell.
pub fn nombre_de_variable(programa: &str) -> String {
    let limpio: String =
        programa.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    format!("SYTHRALL_TOOLCHAIN_{}", limpio.to_ascii_uppercase())
}

/// Las carpetas donde el instalador de cada herramienta deja sus binarios
/// cuando nadie le cambia el default.
///
/// **`gnat-gcc` es una clave propia y no `gcc` a propósito.** En esta máquina
/// hay un `gcc` en el PATH (MinGW) que anuncia Ada en su `languages=` pero no
/// trae el frontend `gnat1`, así que compilar Ada con él falla con un
/// `CreateProcess: No such file or directory`. Resolver "el gcc" por nombre
/// encontraría justo el equivocado; por eso el de GNAT se pide por su nombre.
fn candidatos(programa: &str) -> Vec<PathBuf> {
    let local = std::env::var("LOCALAPPDATA").ok().map(PathBuf::from);
    let mut out = Vec::new();

    match programa {
        "dmd" => out.push(PathBuf::from(r"C:\D\dmd2\windows\bin64\dmd.exe")),
        "gnatprove" => out.push(PathBuf::from(r"C:\Ada\gnatprove\bin\gnatprove.exe")),
        // Los que viven dentro de un toolchain de Alire: la carpeta lleva la
        // versión y un hash, así que hay que mirar qué hay en vez de
        // construir la ruta.
        "gnat-gcc" | "gnatmake" | "gnat" => {
            if let Some(l) = &local {
                out.extend(en_toolchain_de_alire(l, "gnat_native", binario_de(programa)));
            }
        }
        "gprbuild" => {
            if let Some(l) = &local {
                out.extend(en_toolchain_de_alire(l, "gprbuild", "gprbuild"));
            }
        }
        _ => {}
    }
    out
}

/// El ejecutable real detrás de una clave.
fn binario_de(programa: &str) -> &str {
    match programa {
        "gnat-gcc" => "gcc",
        otro => otro,
    }
}

/// Busca `<LOCALAPPDATA>\alire\cache\toolchains\<prefijo>_*\bin\<binario>.exe`.
fn en_toolchain_de_alire(local: &std::path::Path, prefijo: &str, binario: &str) -> Vec<PathBuf> {
    let raiz = local.join("alire").join("cache").join("toolchains");
    let Ok(entradas) = std::fs::read_dir(&raiz) else {
        return Vec::new();
    };
    let mut encontrados: Vec<PathBuf> = entradas
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(&format!("{prefijo}_")))
        })
        .map(|p| p.join("bin").join(format!("{binario}.exe")))
        .filter(|p| p.is_file())
        .collect();
    // Determinístico: si hay varias versiones instaladas, siempre la misma.
    encontrados.sort();
    encontrados
}

/// ¿Responde este ejecutable? Se le pide la versión, que es lo más barato que
/// todos entienden.
fn responde(cmd: &std::ffi::OsStr) -> bool {
    for flag in ["--version", "-version", "/?"] {
        if Command::new(cmd).arg(flag).output().is_ok_and(|o| o.status.success()) {
            return true;
        }
    }
    false
}

/// La ruta con la que invocar `programa`, o `None` si no está en ningún lado.
///
/// Devolver el nombre pelado cuando está en el PATH no es un detalle: deja que
/// el sistema resuelva, que es lo que el usuario espera si lo configuró.
pub fn resolver(programa: &str) -> Option<std::ffi::OsString> {
    // 1. La variable de entorno manda.
    if let Ok(ruta) = std::env::var(nombre_de_variable(programa)) {
        let p = PathBuf::from(&ruta);
        if p.is_file() {
            return Some(p.into_os_string());
        }
    }

    // 2. El PATH, salvo para las claves que existen justamente para NO
    //    resolverse por PATH (ver `candidatos`).
    if programa != "gnat-gcc" {
        let bare = std::ffi::OsString::from(programa);
        if responde(&bare) {
            return Some(bare);
        }
    }

    // 3. Donde cada instalador deja las cosas por defecto.
    candidatos(programa).into_iter().find(|p| p.is_file()).map(PathBuf::into_os_string)
}

/// `true` si el programa se puede usar. Azúcar para los sitios que sólo
/// quieren saber si existe.
pub fn disponible(programa: &str) -> bool {
    resolver(programa).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_nombre_de_variable_sanea_los_guiones() {
        assert_eq!(nombre_de_variable("gnat-gcc"), "SYTHRALL_TOOLCHAIN_GNAT_GCC");
        assert_eq!(nombre_de_variable("dmd"), "SYTHRALL_TOOLCHAIN_DMD");
        assert_eq!(nombre_de_variable("g++"), "SYTHRALL_TOOLCHAIN_G__");
    }

    /// **La variable de entorno gana sobre todo lo demás.** Sin esta salida el
    /// módulo sería una lista de rutas de una sola computadora.
    #[test]
    fn la_variable_de_entorno_tiene_prioridad() {
        // Un archivo que seguro existe en cualquier máquina donde corra esto.
        let propio = std::env::current_exe().expect("sin ejecutable propio");
        let var = nombre_de_variable("compilador-inventado");
        // SAFETY-equivalente: el test es de un solo hilo por el
        // `--test-threads=1` que usa la suite, y la variable se limpia al
        // final.
        unsafe { std::env::set_var(&var, &propio) };
        let r = resolver("compilador-inventado");
        unsafe { std::env::remove_var(&var) };
        assert_eq!(r, Some(propio.into_os_string()));
    }

    /// Una variable que apunta a un archivo inexistente **no** se usa: se
    /// sigue buscando. Apuntar mal no debería romper lo que sí funciona.
    #[test]
    fn una_variable_que_apunta_a_la_nada_se_ignora() {
        let var = nombre_de_variable("otro-inventado");
        unsafe { std::env::set_var(&var, r"Z:\no\existe\nada.exe") };
        let r = resolver("otro-inventado");
        unsafe { std::env::remove_var(&var) };
        assert_eq!(r, None);
    }

    #[test]
    fn un_programa_que_no_existe_no_se_resuelve() {
        assert!(resolver("compilador-que-no-existe-xyz").is_none());
    }

    /// `gnat-gcc` es una clave propia justamente para NO caer en el `gcc` del
    /// PATH, que en esta máquina anuncia Ada sin tener el frontend.
    #[test]
    fn gnat_gcc_no_se_resuelve_por_el_path() {
        assert_eq!(binario_de("gnat-gcc"), "gcc");
        assert_ne!(resolver("gnat-gcc"), Some(std::ffi::OsString::from("gnat-gcc")));
    }

    /// Un programa que sí está en el PATH se devuelve pelado, para que lo
    /// resuelva el sistema.
    #[test]
    fn lo_que_esta_en_el_path_se_devuelve_pelado() {
        // `cmd` existe en cualquier Windows; en otros sistemas el test no
        // afirma nada en vez de inventar una equivalencia.
        if cfg!(windows) {
            assert_eq!(resolver("cmd"), Some(std::ffi::OsString::from("cmd")));
        }
    }
}
