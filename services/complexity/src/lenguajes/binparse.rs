//! Fase 19 (Machine Intelligence) — segundo bullet, cierra la fase: un
//! analizador de ejecutables reales (PE/ELF/Mach-O/COFF/wasm) — secciones,
//! símbolos, imports/exports. A diferencia de `asmparse.rs` (pattern-matching
//! sobre texto `.s`/`.asm` que el usuario ya escribió o pegó), esto parsea la
//! estructura BINARIA de un archivo ya compilado — un `.exe`/`.dll`/`.so`/
//! `.dylib`/`.o` — algo que un análisis de texto no puede tocar.
//!
//! Mismo principio de "integrar herramientas maduras, no reinventarlas" que
//! el resto de esta fase, solo que la herramienta madura elegida no es
//! Capstone/LIEF/objdump por subprocess (dependencias C/C++ externas, FFI o
//! parsing de SU output de texto) sino el crate `object` — la misma librería
//! que usan `rustc`/`lld`/`wasmtime` para exactamente esto. Es la versión
//! "ya integrada al binario, pura Rust, sin proceso externo" de la misma
//! idea de "no hand-write it": ni este módulo ni `object` reimplementan la
//! especificación de ningún formato, `object` ya lo hizo y está probado en
//! producción por herramientas mucho más críticas que esta.
//!
//! Deliberadamente NO hace disassembly de instrucciones — eso seguiría
//! siendo trabajo de un disassembler real (Capstone u otro), explícitamente
//! fuera de este bullet tal como quedó escrito en el ROADMAP. Esto es forma
//! binaria (dónde está cada sección, qué símbolos declara, qué importa/
//! exporta), no semántica de código.

use object::{Object, ObjectSection, ObjectSymbol};
use serde::Serialize;

#[derive(Serialize)]
pub struct SectionInfo {
    pub name: String,
    pub address: u64,
    pub size: u64,
    pub kind: String,
}

#[derive(Serialize)]
pub struct SymbolInfo {
    pub name: String,
    pub address: u64,
    pub kind: String,
    pub is_undefined: bool,
    pub is_global: bool,
}

#[derive(Serialize)]
pub struct ImportInfo {
    pub library: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct ExportInfo {
    pub name: String,
    pub address: u64,
}

#[derive(Serialize)]
pub struct ExecutableParseResult {
    pub available: bool,
    pub format: String,
    pub architecture: String,
    pub is_64_bit: bool,
    pub entry_point: u64,
    pub sections: Vec<SectionInfo>,
    pub symbols: Vec<SymbolInfo>,
    pub imports: Vec<ImportInfo>,
    pub exports: Vec<ExportInfo>,
    pub truncated: bool,
    pub note: String,
}

/// Tope explícito y honesto — un binario real (una DLL de sistema con
/// símbolos de debug, por ejemplo) puede tener decenas de miles de símbolos;
/// devolverlos todos infla la respuesta sin agregar valor pedagógico.
/// `truncated` avisa cuándo se cortó una lista, nunca en silencio — mismo
/// criterio de honestidad que el resto del motor.
const MAX_ITEMS: usize = 500;

fn degraded(note: String) -> ExecutableParseResult {
    ExecutableParseResult {
        available: false,
        format: "unknown".to_string(),
        architecture: "unknown".to_string(),
        is_64_bit: false,
        entry_point: 0,
        sections: vec![],
        symbols: vec![],
        imports: vec![],
        exports: vec![],
        truncated: false,
        note,
    }
}

pub fn parse(bytes: &[u8]) -> ExecutableParseResult {
    if bytes.is_empty() {
        return degraded("el archivo está vacío".to_string());
    }

    let file = match object::File::parse(bytes) {
        Ok(f) => f,
        Err(e) => {
            return degraded(format!(
                "no se pudo reconocer como PE/ELF/Mach-O/COFF/wasm — ¿es realmente un ejecutable/librería compilada? ({e})"
            ));
        }
    };

    let mut truncated = false;

    let mut sections: Vec<SectionInfo> = Vec::new();
    for section in file.sections() {
        if sections.len() >= MAX_ITEMS {
            truncated = true;
            break;
        }
        let name = section.name().map(|s| s.to_string()).unwrap_or_else(|_| "<sin nombre>".to_string());
        sections.push(SectionInfo { name, address: section.address(), size: section.size(), kind: format!("{:?}", section.kind()) });
    }

    let mut symbols: Vec<SymbolInfo> = Vec::new();
    for symbol in file.symbols() {
        if symbols.len() >= MAX_ITEMS {
            truncated = true;
            break;
        }
        let name = symbol.name().map(|s| s.to_string()).unwrap_or_else(|_| "<sin nombre>".to_string());
        if name.is_empty() {
            continue;
        }
        symbols.push(SymbolInfo {
            name,
            address: symbol.address(),
            kind: format!("{:?}", symbol.kind()),
            is_undefined: symbol.is_undefined(),
            is_global: symbol.is_global(),
        });
    }

    // No todos los formatos soportan imports/exports (un `.o` relocatable no
    // tiene ninguno de los dos, por ejemplo) — `object` devuelve un error en
    // ese caso, no un pánico; se trata como "ninguno", no como una falla.
    let mut imports: Vec<ImportInfo> = file
        .imports()
        .unwrap_or_default()
        .into_iter()
        .map(|i| ImportInfo { library: String::from_utf8_lossy(i.library()).into_owned(), name: String::from_utf8_lossy(i.name()).into_owned() })
        .collect();
    if imports.len() > MAX_ITEMS {
        truncated = true;
        imports.truncate(MAX_ITEMS);
    }

    let mut exports: Vec<ExportInfo> = file
        .exports()
        .unwrap_or_default()
        .into_iter()
        .map(|e| ExportInfo { name: String::from_utf8_lossy(e.name()).into_owned(), address: e.address() })
        .collect();
    if exports.len() > MAX_ITEMS {
        truncated = true;
        exports.truncate(MAX_ITEMS);
    }

    let note = format!(
        "Estructura binaria real, parseada con el crate `object` (misma librería que usan rustc/lld/wasmtime) — NO es disassembly de instrucciones, solo forma: secciones, símbolos, imports/exports.{}",
        if truncated { format!(" Algunas listas se cortaron en {MAX_ITEMS} elementos.") } else { String::new() }
    );

    ExecutableParseResult {
        available: true,
        format: format!("{:?}", file.format()),
        architecture: format!("{:?}", file.architecture()),
        is_64_bit: file.is_64(),
        entry_point: file.entry(),
        sections,
        symbols,
        imports,
        exports,
        truncated,
        note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    #[test]
    fn bytes_vacios_degrada_con_gracia() {
        let result = parse(&[]);
        assert!(!result.available);
        assert!(result.sections.is_empty());
    }

    #[test]
    fn bytes_basura_degrada_con_gracia_no_panickea() {
        let garbage = vec![0u8, 1, 2, 3, 255, 254, 253, 42, 42, 42];
        let result = parse(&garbage);
        assert!(!result.available);
        assert!(!result.note.is_empty());
    }

    #[test]
    fn bytes_de_texto_plano_no_es_un_ejecutable_reconocido() {
        let text = b"no soy un ejecutable, soy texto plano de un archivo cualquiera";
        let result = parse(text);
        assert!(!result.available);
    }

    fn gcc_available() -> bool {
        Command::new("gcc").arg("--version").output().is_ok_and(|o| o.status.success())
    }

    /// Compila un `.c` trivial real con `gcc` (mismo patrón que
    /// `asm_bench.rs`/`mergesort_bench.rs`) y parsea el `.exe`/binario
    /// resultante de verdad — no un fixture binario hardcodeado a mano, que
    /// sería reinventar exactamente lo que este módulo existe para evitar.
    #[test]
    fn parsea_un_pe_real_compilado_en_esta_maquina_si_gcc_esta_disponible() {
        if !gcc_available() {
            eprintln!("gcc no disponible en este entorno — test salteado");
            return;
        }
        let dir = std::env::temp_dir().join(format!("sythrall_binparse_{:08x}", rand::random::<u32>()));
        fs::create_dir_all(&dir).expect("no se pudo crear el directorio temporal");
        let src = dir.join("probe.c");
        let bin = dir.join(if cfg!(windows) { "probe.exe" } else { "probe" });
        fs::write(&src, "int suma(int a, int b) { return a + b; }\nint main(void) { return suma(2, 3) - 5; }\n").expect("no se pudo escribir el .c de prueba");

        let compiled = Command::new("gcc").arg(&src).arg("-o").arg(&bin).output().expect("no se pudo invocar gcc");
        assert!(compiled.status.success(), "gcc falló: {}", String::from_utf8_lossy(&compiled.stderr));

        let bytes = fs::read(&bin).expect("no se pudo leer el binario compilado");
        let result = parse(&bytes);

        let _ = fs::remove_dir_all(&dir);

        assert!(result.available, "el parser debería reconocer un binario recién compilado por gcc: {}", result.note);
        assert!(!result.sections.is_empty(), "un ejecutable real siempre tiene al menos una sección (.text)");
        assert!(
            result.sections.iter().any(|s| s.kind == "Text"),
            "debería haber al menos una sección de código (.text), secciones encontradas: {:?}",
            result.sections.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        // El símbolo `suma` que el .c de arriba declara debería sobrevivir a
        // la compilación sin optimizar en la tabla de símbolos (gcc no
        // stripea por default).
        assert!(
            result.symbols.iter().any(|s| s.name.contains("suma")),
            "esperaba encontrar el símbolo 'suma' entre {} símbolos",
            result.symbols.len()
        );
    }
}
