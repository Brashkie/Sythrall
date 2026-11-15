//! Fase 19 (Machine Intelligence) — primer bullet: soporte de Assembly
//! x86-64 como target de análisis. Pattern-matching sobre texto, NO un
//! disassembler real (eso es el 2do bullet de la fase, wrapping Capstone/
//! LIEF sobre ejecutables PE/ELF/Mach-O — deliberadamente no atacado acá).
//! Genuinamente nuevo en Rust, como Fortran en la Fase 20 — Sythrall nunca
//! tuvo soporte de Assembly antes de esto.
//!
//! Sintaxis auto-detectada, confirmado con el usuario: si el texto tiene
//! registros `%reg` o inmediatos `$N` es AT&T (la que usa `gcc -S` en
//! Linux); si no, se asume Intel (NASM/MASM/Godbolt). Los mnemónicos
//! (`mov`/`jmp`/`call`/...) son idénticos en ambas sintaxis — una sola tabla
//! de clasificación sirve para las dos, solo cambia cómo se extraen
//! registros de los operandos (AT&T los prefija con `%`, sin ambigüedad;
//! Intel no tiene sigilo, así que hace falta comparar contra una lista fija
//! de nombres de registro conocidos).
//!
//! Big-O deliberadamente acotado: un salto hacia atrás (a un label ya visto
//! antes en el mismo procedimiento) es evidencia de un loop → O(n). No se
//! intenta distinguir anidamiento real (O(n²) etc.) — eso necesitaría un CFG
//! de verdad, un salto de complejidad que esta primera porción no da, mismo
//! criterio de "no reclamar más de lo que el heurístico puede probar" que
//! `numerical_algorithm_note`/Type-1 ya establecen en otras partes del motor.

use std::collections::{BTreeSet, HashSet};

use serde::Serialize;

use crate::flujo::graph::CallEdge;
use crate::hallazgos::smells::{self, StructuralSmell};

use crate::costo::callingconv::{self, ContextSwitchBlock, StackFrameInfo};

#[derive(Serialize, Clone)]
pub struct AsmInstruction {
    pub line: usize,
    pub mnemonic: String,
    pub operands: Vec<String>,
    pub category: &'static str,
    pub explanation: String,
}

/// Smells estructurales de Assembly (Fase 22, 2026-09-05).
///
/// **Solo uno, y el motivo importa más que el smell.** `long_function` sale
/// de `loc`, que ya se calcula acá. Los otros dos que emiten los demás
/// lenguajes no aplican: un procedimiento en ensamblador no tiene parámetros
/// declarados (van por registro o por stack, según la convención de llamada
/// — eso lo explica `callingconv.rs`, no un conteo), y no tiene bloques
/// anidados que contar: el control de flujo son saltos, y reconstruir
/// anidamiento a partir de saltos es armar un CFG, que es un salto de
/// complejidad, no una reinterpretación.
fn structural_smells_asm(procedures: &[AsmProcedure]) -> Vec<StructuralSmell> {
    let mut out: Vec<StructuralSmell> =
        procedures.iter().filter_map(|p| smells::check_long_function(&p.name, p.line, p.loc)).collect();
    out.sort_by_key(|s| s.line);
    out
}

/// **Uso de SIMD: qué tan ancho opera el código, y si está vectorizado de
/// verdad.**
///
/// Nace de una pregunta concreta que el motor no podía responder: *"¿el
/// compilador vectorizó este bucle?"*. El módulo ya reconocía `xmm` y nada
/// más — ni `ymm`, ni `zmm`, ni los registros de máscara de AVX-512—, así que
/// el 100% del código AVX pasaba como registros desconocidos.
///
/// **La distinción que hace que esto no mienta: usar un registro vectorial no
/// es estar vectorizado.** El sufijo del mnemónico lo dice sin ambigüedad:
///
/// - `addps` / `mulpd` — **packed**: una instrucción, varios elementos. Eso
///   es vectorización.
/// - `addss` / `mulsd` — **scalar**: la misma instrucción sobre UN elemento,
///   usando el registro ancho como si fuera angosto. Es lo que emite cualquier
///   compilador para aritmética de punto flotante común desde SSE2, **sin
///   vectorizar nada**.
///
/// Contar `xmm` y decir "vectorizado" daría un sí para casi todo programa con
/// un `float` adentro. Por eso `vectorized` mira solo las packed.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SimdInfo {
    /// Extensión más alta de la que hay evidencia: `"sse"`, `"avx"`, `"avx2"`
    /// o `"avx512"`.
    pub isa: &'static str,
    /// Ancho del registro vectorial más grande tocado, en bits.
    pub max_width_bits: u32,
    /// Instrucciones que operan sobre varios elementos a la vez.
    pub packed_instructions: usize,
    /// Instrucciones que usan un registro vectorial para UN solo elemento.
    pub scalar_simd_instructions: usize,
    /// Registros vectoriales distintos, ordenados.
    pub vector_registers: Vec<String>,
    /// `true` **solo** si hay instrucciones packed.
    pub vectorized: bool,
}

/// Ancho en bits de un registro vectorial, por su prefijo.
fn ancho_de_registro(reg: &str) -> Option<u32> {
    let r = reg.trim_start_matches('%').to_ascii_lowercase();
    let (prefijo, resto) = ["zmm", "ymm", "xmm"]
        .iter()
        .find_map(|p| r.strip_prefix(p).map(|resto| (*p, resto)))?;
    // `xmm31` existe (AVX-512), `xmmabc` no.
    if resto.is_empty() || !resto.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(match prefijo {
        "zmm" => 512,
        "ymm" => 256,
        _ => 128,
    })
}

/// `k0`–`k7`: los registros de máscara, que **solo existen en AVX-512**.
/// Encontrar uno es evidencia dura de esa extensión aunque no haya un `zmm`
/// a la vista, porque AVX-512 permite enmascarar también sobre `xmm`/`ymm`.
fn es_registro_de_mascara(reg: &str) -> bool {
    let r = reg.trim_start_matches('%').to_ascii_lowercase();
    let r = r.split(['{', '}']).next().unwrap_or(&r);
    matches!(r, "k0" | "k1" | "k2" | "k3" | "k4" | "k5" | "k6" | "k7")
}

/// Cómo opera una instrucción sobre su registro vectorial.
#[derive(PartialEq, Clone, Copy)]
enum FormaSimd {
    /// Varios elementos a la vez.
    Packed,
    /// Un elemento, en un registro ancho.
    Escalar,
    /// Movimiento/conversión que no dice por sí mismo cuántos elementos toca.
    Otra,
}

/// **El sufijo del mnemónico es la fuente de verdad, no el registro.**
///
/// - `…ps` / `…pd` → packed float (single / double)
/// - `…ss` / `…sd` → scalar float
/// - `p…` (tras quitar la `v` de VEX) → packed integer (`paddd`, `vpmullw`)
fn forma_de(mnemonico: &str) -> FormaSimd {
    let m = mnemonico.to_ascii_lowercase();
    // La `v` inicial es el prefijo VEX/EVEX de AVX, no parte de la operación.
    let base = m.strip_prefix('v').unwrap_or(&m);
    if base.ends_with("ss") || base.ends_with("sd") {
        return FormaSimd::Escalar;
    }
    if base.ends_with("ps") || base.ends_with("pd") {
        return FormaSimd::Packed;
    }
    // `movdqa`/`movdqu` mueven el registro entero; `punpck*`, `padd*`, etc.
    // son enteros empaquetados.
    if base.starts_with('p') || base.starts_with("movdq") {
        return FormaSimd::Packed;
    }
    FormaSimd::Otra
}

/// Analiza el uso de SIMD de un procedimiento. `None` si no toca ni un
/// registro vectorial — **ausente no es lo mismo que cero**, igual que el
/// resto del motor: un procedimiento sin SIMD no tiene que mostrar una
/// sección vacía.
pub fn simd_info(instructions: &[AsmInstruction]) -> Option<SimdInfo> {
    let mut registros: BTreeSet<String> = BTreeSet::new();
    let mut ancho_max = 0u32;
    let mut packed = 0usize;
    let mut escalares = 0usize;
    let mut hay_mascara = false;
    let mut hay_vex = false;

    for ins in instructions {
        let mut toca_vector = false;
        for op in &ins.operands {
            // Un operando puede traer varios tokens (`(%rax,%rbx,4)`,
            // `%zmm1{%k2}`), así que se parten por lo que no es alfanumérico.
            for token in op.split(|c: char| !c.is_ascii_alphanumeric()) {
                if token.is_empty() {
                    continue;
                }
                if let Some(bits) = ancho_de_registro(token) {
                    ancho_max = ancho_max.max(bits);
                    registros.insert(token.to_ascii_lowercase());
                    toca_vector = true;
                } else if es_registro_de_mascara(token) {
                    hay_mascara = true;
                    toca_vector = true;
                }
            }
        }
        if !toca_vector {
            continue;
        }
        if ins.mnemonic.to_ascii_lowercase().starts_with('v') {
            hay_vex = true;
        }
        match forma_de(&ins.mnemonic) {
            FormaSimd::Packed => packed += 1,
            FormaSimd::Escalar => escalares += 1,
            FormaSimd::Otra => {}
        }
    }

    if registros.is_empty() && !hay_mascara {
        return None;
    }

    // **La extensión se deduce de la evidencia más fuerte disponible**, y cada
    // escalón necesita algo que el anterior no puede producir: un `zmm` o un
    // registro de máscara no existen fuera de AVX-512; un `ymm` no existe
    // fuera de AVX; el prefijo VEX (`v…`) distingue AVX-128 de SSE sobre los
    // mismos `xmm`. Enteros empaquetados de 256 bits son AVX2, no AVX1.
    let isa = if ancho_max >= 512 || hay_mascara {
        "avx512"
    } else if ancho_max >= 256 {
        if packed > 0 { "avx2" } else { "avx" }
    } else if hay_vex {
        "avx"
    } else {
        "sse"
    };

    Some(SimdInfo {
        isa,
        max_width_bits: ancho_max,
        packed_instructions: packed,
        scalar_simd_instructions: escalares,
        vector_registers: registros.into_iter().collect(),
        vectorized: packed > 0,
    })
}

#[derive(Serialize, Clone)]
pub struct AsmProcedure {
    pub name: String,
    pub line: usize,
    pub end_line: usize,
    pub loc: usize,
    pub complexity: u32,
    pub big_o: String,
    pub big_o_reason: String,
    pub instructions: Vec<AsmInstruction>,
    pub registers_used: Vec<String>,
    pub calls: Vec<String>,
    /// Fase 19, 3er bullet — explicador de calling-convention/stack-frame
    /// (`callingconv.rs`), calculado sobre las `instructions` de este mismo
    /// procedimiento. Reinterpretación, no un campo que este módulo calcule
    /// dos veces.
    pub stack_frame: StackFrameInfo,
    /// Fase 32 — bloques de guardado/restauración de contexto (`PUSHA`/`POPA`
    /// en x86, `STM`/`LDM` en ARM), calculados sobre estas mismas
    /// `instructions`. Vacío para la mayoría de los procedimientos: son
    /// instrucciones de código escrito a mano y de manejadores de
    /// interrupción, no de salida de compilador.
    pub context_switch: Vec<ContextSwitchBlock>,
    /// Uso de SIMD. `None` cuando el procedimiento no toca un solo registro
    /// vectorial — ausente no es lo mismo que cero.
    pub simd: Option<SimdInfo>,
}

#[derive(Serialize)]
pub struct AsmParseResult {
    /// `"att"`, `"intel"` o —desde la Fase 32— `"arm"`.
    ///
    /// Que ARM aparezca acá y no en un campo `architecture` aparte es
    /// deliberado: el campo responde *"¿cómo se leen estos operandos?"*, y
    /// ARM es una tercera respuesta a esa misma pregunta, no un eje nuevo.
    /// Dos campos que siempre se mueven juntos serían dos formas de decir lo
    /// mismo, y una de ellas terminaría desactualizada.
    pub syntax: &'static str,
    pub procedures: Vec<AsmProcedure>,
    pub call_graph: Vec<CallEdge>,
    /// Fase 22, 2026-09-05 — ver `structural_smells_asm`.
    pub structural_smells: Vec<StructuralSmell>,
}

fn is_att_syntax(content: &str) -> bool {
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_alphabetic() {
            return true;
        }
        if bytes[i] == b'$' && i + 1 < bytes.len() && (bytes[i + 1].is_ascii_digit() || bytes[i + 1] == b'-') {
            return true;
        }
        i += 1;
    }
    false
}

/// **Arquitectura, que hasta la Fase 32 se asumía x86 en silencio.**
///
/// El módulo detectaba sintaxis (AT&T contra Intel) y reportaba `intel` para
/// un archivo de ARM, que no es ninguna de las dos — una afirmación falsa,
/// no un hueco, y arrastraba a `callingconv.rs` a explicar un procedimiento
/// de ARM con un flag de GCC para x86-64.
///
/// La evidencia que se acepta es deliberadamente estrecha y toda de ARM: una
/// transferencia múltiple (`stm`/`ldm` con modo conocido), una lista de
/// registros entre llaves, o un `bx`/`blx`. **Un archivo sin ninguna de las
/// tres se asume x86**, que es exactamente lo que ya pasaba; la diferencia
/// es que ahora está escrito en vez de ser una suposición tácita.
fn es_arm(content: &str) -> bool {
    for linea in content.lines() {
        let l = strip_comment(linea).trim();
        if l.is_empty() {
            continue;
        }
        if l.contains('{') && l.contains('r') {
            // Una lista entre llaves solo aparece en ARM; en x86 las llaves
            // no son sintaxis de operando.
            return true;
        }
        // El mnemónico es el primer token después de un label opcional.
        let sin_label = l.split_once(':').map(|(_, r)| r).unwrap_or(l).trim();
        let Some(mnemonico) = sin_label.split_whitespace().next() else { continue };
        let m = mnemonico.to_ascii_lowercase();
        if m == "bx" || m == "blx" || callingconv::arm_bulk_transfer(&m).is_some() {
            return true;
        }
    }
    false
}

/// Registros de ARM. Hacen falta por lo mismo que `INTEL_REGISTERS`: sin
/// sigilo no hay forma de distinguir un registro de una etiqueta. `r0`-`r3`
/// no están en la lista de x86 —ahí los numerados empiezan en `r8`—, así que
/// antes de la Fase 32 los argumentos de una función de ARM eran invisibles.
const ARM_REGISTERS: &[&str] = &[
    "r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15", "sp", "lr", "pc", "ip", "fp", "sl", "sb",
];

fn strip_comment(line: &str) -> &str {
    let hash = line.find('#');
    let semi = line.find(';');
    let cut = match (hash, semi) {
        (Some(h), Some(s)) => Some(h.min(s)),
        (Some(h), None) => Some(h),
        (None, Some(s)) => Some(s),
        (None, None) => None,
    };
    match cut {
        Some(idx) => &line[..idx],
        None => line,
    }
}

/// Candidato válido como nombre de label: un identificador normal, o una
/// secuencia puramente numérica (labels locales estilo GAS, ej. `1:`).
fn is_label_candidate(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    if s.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    let mut chars = s.chars();
    let first_ok = chars.next().map(|c| c.is_ascii_alphabetic() || c == '_' || c == '.' || c == '$').unwrap_or(false);
    first_ok && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '$')
}

/// **Símbolos que el propio ensamblador declara como función.**
///
/// La heurística de nombres (`is_procedure_label`) alcanza para ensamblador
/// escrito a mano, que es contra lo que este módulo se construyó. Contra
/// salida de compilador real se rompe: `gcc -S -O2` sobre una función sola
/// emite `_suma_escalar:` y además `LFB0:`, `L3:`, `L4:`, `LFE0:` —todas
/// etiquetas locales SIN punto inicial, porque el formato COFF/Mach-O no usa
/// el `.L` de ELF—. El resultado medido era una función partida en cinco
/// procedimientos falsos, cuatro de ellos con el cuerpo repartido y el real
/// con cero instrucciones.
///
/// **Las directivas son autoritativas y las heurísticas no**, así que cuando
/// el archivo declara algo, manda lo declarado:
///
/// - `.globl` / `.global NAME` — símbolo exportado
/// - `.type NAME, @function` / `%function` — ELF, dice explícitamente que es función
/// - `.def NAME;` — COFF/MinGW
/// - `NAME PROC` — MASM (sintaxis Intel)
///
/// Si el archivo **no declara nada** —ensamblador a mano, que es el caso de
/// todos los tests de este módulo— se vuelve a la heurística de antes. No se
/// reemplaza una regla por otra: se usa la buena cuando hay con qué.
fn simbolos_declarados(content: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for raw in content.lines() {
        let l = strip_comment(raw).trim();
        if l.is_empty() {
            continue;
        }
        // **Se parte por espacios en blanco, no por un espacio literal.** El
        // ensamblador que emite gcc separa la directiva de su argumento con
        // un TAB (`.globl	_suma_escalar`), así que un `strip_prefix(".globl ")`
        // no matchea nada y el archivo entero parece no declarar símbolos.
        // Lo agarró medir contra salida real, no un test.
        let mut it = l.split_whitespace();
        let Some(primero) = it.next() else { continue };
        let resto: Vec<&str> = it.collect();

        let dir = primero.to_ascii_lowercase();
        match dir.as_str() {
            ".globl" | ".global" | ".def" | ".ent" => {
                if let Some(n) = resto.first().and_then(|t| primer_simbolo(t)) {
                    out.insert(n);
                }
            }
            ".type" => {
                // `.type nombre, @function` — solo si dice que es función.
                if resto.iter().any(|t| t.contains("function")) {
                    if let Some(n) = resto.first().and_then(|t| primer_simbolo(t)) {
                        out.insert(n);
                    }
                }
            }
            _ => {
                // MASM: `nombre PROC`.
                if resto.first().is_some_and(|k| k.eq_ignore_ascii_case("proc"))
                    && is_label_candidate(primero)
                {
                    out.insert(primero.to_string());
                }
            }
        }
    }
    out
}

/// Primer símbolo de una lista de argumentos de directiva — corta en coma,
/// punto y coma o espacio (`.type suma, @function`, `.def _f;`).
fn primer_simbolo(s: &str) -> Option<String> {
    let t: String = s
        .trim()
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != ',' && *c != ';')
        .collect();
    if t.is_empty() || !is_label_candidate(&t) {
        None
    } else {
        Some(t)
    }
}

/// Un label "de procedimiento" — no empieza con `.` (convención GCC para
/// labels locales, ej. `.L2`) y no es puramente numérico (labels locales
/// tipo `1:` usados como blanco de salto dentro de la misma función, no
/// como entrada de un procedimiento nuevo).
fn is_procedure_label(label: &str) -> bool {
    !label.starts_with('.') && !label.chars().all(|c| c.is_ascii_digit())
}

/// `label:` al principio de la línea, con o sin instrucción a continuación
/// en la misma línea (ej. `.L2: addl $1, %eax`).
fn split_label(line: &str) -> Option<(&str, Option<&str>)> {
    let colon = line.find(':')?;
    let candidate = &line[..colon];
    if !is_label_candidate(candidate) {
        return None;
    }
    let rest = line[colon + 1..].trim();
    Some((candidate, if rest.is_empty() { None } else { Some(rest) }))
}

/// Split de operandos consciente de delimitadores — `(%rbp,%rax,4)` tiene
/// una coma que NO separa operandos, es parte del modo de direccionamiento.
///
/// **Las llaves entraron en la Fase 32, y el bug lo encontró medir.** Este
/// comentario ya decía lo correcto y el código ya contaba profundidad; solo
/// no conocía `{}`, porque el módulo nació x86-only. La lista de registros
/// de ARM `{r4-r11, lr}` salía partida en `{r4-r11` y `lr}`: dos operandos
/// rotos, ninguno de los cuales es un registro.
fn split_operands(s: &str) -> Vec<String> {
    let mut operands = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for c in s.chars() {
        match c {
            '(' | '[' | '{' => {
                depth += 1;
                current.push(c);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    operands.push(trimmed.to_string());
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        operands.push(trimmed.to_string());
    }
    operands
}

struct MnemonicInfo {
    category: &'static str,
    explanation: &'static str,
}

/// Tabla de clasificación — mismo estilo que `ML_LIB_MAP`/`METRIC_PATTERNS`
/// en `ml.rs`. Cubre los mnemónicos x86-64 más comunes; uno no reconocido
/// cae a `"other"` con una explicación genérica, nunca inventada.
const MNEMONICS: &[(&str, MnemonicInfo)] = &[
    ("mov", MnemonicInfo { category: "data_movement", explanation: "copia datos del origen al destino" }),
    ("lea", MnemonicInfo { category: "data_movement", explanation: "carga la dirección efectiva calculada, no el valor en memoria" }),
    ("xchg", MnemonicInfo { category: "data_movement", explanation: "intercambia los valores de dos operandos" }),
    ("add", MnemonicInfo { category: "arithmetic", explanation: "suma el origen al destino" }),
    ("sub", MnemonicInfo { category: "arithmetic", explanation: "resta el origen del destino" }),
    ("mul", MnemonicInfo { category: "arithmetic", explanation: "multiplicación sin signo" }),
    ("imul", MnemonicInfo { category: "arithmetic", explanation: "multiplicación con signo" }),
    ("div", MnemonicInfo { category: "arithmetic", explanation: "división sin signo" }),
    ("idiv", MnemonicInfo { category: "arithmetic", explanation: "división con signo" }),
    ("inc", MnemonicInfo { category: "arithmetic", explanation: "incrementa el operando en 1" }),
    ("dec", MnemonicInfo { category: "arithmetic", explanation: "decrementa el operando en 1" }),
    ("neg", MnemonicInfo { category: "arithmetic", explanation: "niega aritméticamente el operando (complemento a 2)" }),
    ("adc", MnemonicInfo { category: "arithmetic", explanation: "suma con acarreo (carry flag)" }),
    ("sbb", MnemonicInfo { category: "arithmetic", explanation: "resta con préstamo (borrow/carry flag)" }),
    ("and", MnemonicInfo { category: "logic", explanation: "AND bit a bit" }),
    ("or", MnemonicInfo { category: "logic", explanation: "OR bit a bit" }),
    ("xor", MnemonicInfo { category: "logic", explanation: "XOR bit a bit — común para poner un registro en cero (`xor eax, eax`)" }),
    ("not", MnemonicInfo { category: "logic", explanation: "complemento bit a bit (NOT)" }),
    ("shl", MnemonicInfo { category: "logic", explanation: "desplazamiento lógico a la izquierda" }),
    ("sal", MnemonicInfo { category: "logic", explanation: "desplazamiento aritmético a la izquierda (igual a shl)" }),
    ("shr", MnemonicInfo { category: "logic", explanation: "desplazamiento lógico a la derecha" }),
    ("sar", MnemonicInfo { category: "logic", explanation: "desplazamiento aritmético a la derecha (preserva el signo)" }),
    ("rol", MnemonicInfo { category: "logic", explanation: "rotación de bits a la izquierda" }),
    ("ror", MnemonicInfo { category: "logic", explanation: "rotación de bits a la derecha" }),
    ("cmp", MnemonicInfo { category: "comparison", explanation: "compara dos operandos (resta sin guardar el resultado) y setea flags" }),
    ("test", MnemonicInfo { category: "comparison", explanation: "AND bit a bit sin guardar el resultado, solo setea flags" }),
    ("jmp", MnemonicInfo { category: "control_flow", explanation: "salto incondicional" }),
    ("je", MnemonicInfo { category: "control_flow", explanation: "salta si el zero flag está seteado (igual, de un cmp/test previo)" }),
    ("jz", MnemonicInfo { category: "control_flow", explanation: "salta si el zero flag está seteado (cero)" }),
    ("jne", MnemonicInfo { category: "control_flow", explanation: "salta si el zero flag NO está seteado (distinto)" }),
    ("jnz", MnemonicInfo { category: "control_flow", explanation: "salta si el zero flag NO está seteado (no-cero)" }),
    ("jg", MnemonicInfo { category: "control_flow", explanation: "salta si es mayor (con signo)" }),
    ("jge", MnemonicInfo { category: "control_flow", explanation: "salta si es mayor o igual (con signo)" }),
    ("jl", MnemonicInfo { category: "control_flow", explanation: "salta si es menor (con signo)" }),
    ("jle", MnemonicInfo { category: "control_flow", explanation: "salta si es menor o igual (con signo)" }),
    ("ja", MnemonicInfo { category: "control_flow", explanation: "salta si es mayor (sin signo)" }),
    ("jb", MnemonicInfo { category: "control_flow", explanation: "salta si es menor (sin signo)" }),
    ("call", MnemonicInfo { category: "control_flow", explanation: "llama a un procedimiento (empuja la dirección de retorno)" }),
    ("ret", MnemonicInfo { category: "control_flow", explanation: "retorna del procedimiento actual" }),
    ("loop", MnemonicInfo { category: "control_flow", explanation: "decrementa (r|e)cx y salta si no es cero — loop clásico" }),
    ("push", MnemonicInfo { category: "stack", explanation: "apila el operando (decrementa el stack pointer)" }),
    ("pop", MnemonicInfo { category: "stack", explanation: "desapila al operando (incrementa el stack pointer)" }),
    ("enter", MnemonicInfo { category: "stack", explanation: "arma un stack frame (prólogo de función)" }),
    ("leave", MnemonicInfo { category: "stack", explanation: "deshace el stack frame (epílogo de función)" }),
    // Fase 32 — cambio de contexto. Categoría propia y no `stack` porque lo
    // que las separa no es que toquen la pila (`push` también) sino que
    // mueven ESTADO COMPLETO de un golpe: son las instrucciones que existen
    // para abaratar el context switch, que es tiempo puro perdido.
    ("pusha", MnemonicInfo { category: "context_switch", explanation: "apila los 8 registros de propósito general de 16 bits de una sola vez (no existe en x86-64)" }),
    ("pushad", MnemonicInfo { category: "context_switch", explanation: "apila los 8 registros de propósito general de 32 bits de una sola vez (no existe en x86-64)" }),
    ("popa", MnemonicInfo { category: "context_switch", explanation: "desapila los 8 registros de propósito general de 16 bits (descarta el stack pointer guardado)" }),
    ("popad", MnemonicInfo { category: "context_switch", explanation: "desapila los 8 registros de propósito general de 32 bits (descarta el stack pointer guardado)" }),
    // El complemento exacto: `PUSHA` NO guarda las flags, y esto sí. Están
    // acá para que la ausencia de un `pushf` junto a un `pusha` se pueda ver.
    ("pushf", MnemonicInfo { category: "context_switch", explanation: "apila el registro de flags — lo que `PUSHA` deja afuera" }),
    ("pushfd", MnemonicInfo { category: "context_switch", explanation: "apila EFLAGS — lo que `PUSHAD` deja afuera" }),
    ("pushfq", MnemonicInfo { category: "context_switch", explanation: "apila RFLAGS (la forma de 64 bits, la única de esta familia que sobrevive en x86-64)" }),
    ("popf", MnemonicInfo { category: "context_switch", explanation: "restaura el registro de flags" }),
    ("popfd", MnemonicInfo { category: "context_switch", explanation: "restaura EFLAGS" }),
    ("popfq", MnemonicInfo { category: "context_switch", explanation: "restaura RFLAGS" }),
];

/// Mnemónicos que en sintaxis AT&T aparecen con un sufijo de tamaño de un
/// solo carácter (`movl`, `addl`, `cmpq`, `testb`...) — se pela el sufijo
/// antes de clasificar, pero solo si lo que queda es una base conocida
/// (evita el falso positivo de tratar `jl` como `j` + sufijo `l`).
const SIZABLE_BASES: &[&str] =
    &["mov", "add", "sub", "cmp", "test", "and", "or", "xor", "not", "inc", "dec", "neg", "push", "pop", "lea", "imul", "idiv", "mul", "div", "shl", "shr", "sar", "sal", "adc", "sbb", "rol", "ror"];

fn strip_att_size_suffix(mnemonic: &str) -> &str {
    if mnemonic.len() > 1 {
        let (base, suffix) = mnemonic.split_at(mnemonic.len() - 1);
        if matches!(suffix, "b" | "w" | "l" | "q") && SIZABLE_BASES.contains(&base) {
            return base;
        }
    }
    mnemonic
}

fn mnemonic_info(mnemonic: &str) -> (&'static str, String) {
    if let Some((_, info)) = MNEMONICS.iter().find(|(m, _)| *m == mnemonic) {
        return (info.category, info.explanation.to_string());
    }
    let stripped = strip_att_size_suffix(mnemonic);
    if stripped != mnemonic {
        if let Some((_, info)) = MNEMONICS.iter().find(|(m, _)| *m == stripped) {
            return (info.category, info.explanation.to_string());
        }
    }
    if mnemonic.starts_with("movz") {
        return ("data_movement", "copia extendiendo con ceros (de un tamaño menor a uno mayor)".to_string());
    }
    // Fase 32 — `STM`/`LDM` de ARM. El reconocimiento vive en
    // `callingconv.rs`, junto a la tabla de modos de direccionamiento, para
    // no tener dos listas de sufijos que se puedan desincronizar.
    if let Some((direccion, modo)) = callingconv::arm_bulk_transfer(mnemonic) {
        let verbo = if direccion == "save" { "guarda" } else { "restaura" };
        return ("context_switch", format!("{verbo} una lista entera de registros de una sola vez — {modo}"));
    }
    ("other", "instrucción no reconocida por la tabla de clasificación".to_string())
}

const INTEL_REGISTERS: &[&str] = &[
    "rax", "eax", "ax", "al", "ah", "rbx", "ebx", "bx", "bl", "bh", "rcx", "ecx", "cx", "cl", "ch", "rdx", "edx", "dx", "dl", "dh", "rsi", "esi", "si", "sil", "rdi", "edi", "di", "dil", "rbp", "ebp",
    "bp", "bpl", "rsp", "esp", "sp", "spl", "r8", "r8d", "r8w", "r8b", "r9", "r9d", "r9w", "r9b", "r10", "r10d", "r10w", "r10b", "r11", "r11d", "r11w", "r11b", "r12", "r12d", "r12w", "r12b", "r13",
    "r13d", "r13w", "r13b", "r14", "r14d", "r14w", "r14b", "r15", "r15d", "r15w", "r15b", "rip", "eip",
];

/// Extrae nombres de registro de un operando — en AT&T van prefijados con
/// `%` (inequívoco); en Intel no tienen sigilo, así que cada token se
/// compara contra `INTEL_REGISTERS` (sin esa lista no hay forma de
/// distinguir un registro de una etiqueta/símbolo en sintaxis Intel).
/// Fase 32 — extracción de registros consciente de la arquitectura.
fn extract_registers_arch(operand: &str, is_att: bool, es_arm: bool, out: &mut HashSet<String>) {
    if es_arm {
        // Una lista entre llaves se expande (`{r4-r11, lr}` son nueve
        // registros); cualquier otro operando se parte en tokens y se
        // compara contra la lista, igual que en Intel.
        let candidatos: Vec<String> = if operand.trim_start().starts_with('{') {
            callingconv::expandir_lista_de_registros(operand)
        } else {
            operand.split(|c: char| !c.is_ascii_alphanumeric()).map(|t| t.to_ascii_lowercase()).collect()
        };
        for c in candidatos {
            if ARM_REGISTERS.contains(&c.as_str()) {
                out.insert(c);
            }
        }
        return;
    }
    if is_att {
        let bytes = operand.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' {
                let start = i + 1;
                let mut end = start;
                while end < bytes.len() && bytes[end].is_ascii_alphanumeric() {
                    end += 1;
                }
                if end > start {
                    out.insert(operand[start..end].to_string());
                }
                i = end;
            } else {
                i += 1;
            }
        }
    } else {
        let mut token = String::new();
        let mut tokens = Vec::new();
        for c in operand.chars() {
            if c.is_ascii_alphanumeric() {
                token.push(c);
            } else if !token.is_empty() {
                tokens.push(std::mem::take(&mut token));
            }
        }
        if !token.is_empty() {
            tokens.push(token);
        }
        for t in tokens {
            let lower = t.to_lowercase();
            // **Los vectoriales se reconocen por prefijo, no por lista.**
            // Enumerar `xmm0..31` + `ymm0..31` + `zmm0..31` + `k0..7` son más
            // de cien entradas que solo dicen lo mismo que la regla.
            if INTEL_REGISTERS.contains(&lower.as_str())
                || ancho_de_registro(&lower).is_some()
                || es_registro_de_mascara(&lower)
            {
                out.insert(lower);
            }
        }
    }
}

struct Proc {
    name: String,
    start_line: usize,
    end_line: usize,
    instructions: Vec<AsmInstruction>,
    labels_seen: HashSet<String>,
    backward_jumps: u32,
    registers: HashSet<String>,
    calls: Vec<String>,
    /// Fase 32 — `"x86"` o `"arm"`. Se guarda por procedimiento porque es lo
    /// que `finish` necesita para no explicar un prólogo de ARM con un flag
    /// de x86-64.
    arquitectura: &'static str,
}

impl Proc {
    fn new(name: &str, line: usize, arquitectura: &'static str) -> Self {
        let mut labels_seen = HashSet::new();
        labels_seen.insert(name.to_string());
        Proc {
            name: name.to_string(),
            start_line: line,
            end_line: line,
            instructions: Vec::new(),
            labels_seen,
            backward_jumps: 0,
            registers: HashSet::new(),
            calls: Vec::new(),
            arquitectura,
        }
    }

    fn finish(self) -> AsmProcedure {
        let loc = self.end_line - self.start_line + 1;
        let (big_o, big_o_reason) = if self.backward_jumps > 0 {
            ("O(n)".to_string(), format!("{} salto(s) hacia atrás detectado(s) — forma de loop; no se distingue anidamiento real", self.backward_jumps))
        } else {
            ("O(1)".to_string(), "sin saltos hacia atrás detectados".to_string())
        };
        let mut registers_used: Vec<String> = self.registers.into_iter().collect();
        registers_used.sort();
        let context_switch = callingconv::context_blocks(&self.instructions);
        let stack_frame = callingconv::analyze(&self.instructions, self.calls.is_empty(), self.arquitectura);
        let simd = simd_info(&self.instructions);
        AsmProcedure {
            name: self.name,
            line: self.start_line,
            end_line: self.end_line,
            loc,
            complexity: 1 + self.backward_jumps,
            big_o,
            big_o_reason,
            context_switch,
            instructions: self.instructions,
            registers_used,
            simd,
            calls: self.calls,
            stack_frame,
        }
    }
}

fn process_instruction(line: &str, line_no: usize, is_att: bool, proc: &mut Proc) {
    let es_arm_aqui = proc.arquitectura == "arm";
    let mut parts = line.splitn(2, char::is_whitespace);
    let mnemonic_raw = parts.next().unwrap_or("").trim();
    if mnemonic_raw.is_empty() {
        return;
    }
    let mnemonic = mnemonic_raw.to_lowercase();
    let rest = parts.next().unwrap_or("").trim();
    let operands = split_operands(rest);

    for op in &operands {
        extract_registers_arch(op, is_att, es_arm_aqui, &mut proc.registers);
    }

    let (category, explanation) = mnemonic_info(&mnemonic);

    if category == "control_flow" && mnemonic != "ret" {
        if let Some(target) = operands.first() {
            let target = target.trim_start_matches('*');
            if mnemonic == "call" {
                proc.calls.push(target.to_string());
            } else if proc.labels_seen.contains(target) {
                proc.backward_jumps += 1;
            }
        }
    }

    proc.end_line = line_no;
    proc.instructions.push(AsmInstruction { line: line_no, mnemonic, operands, category, explanation });
}

/// Punto de entrada — nunca falla (a diferencia de los parsers con
/// tree-sitter, esto es pattern-matching sobre texto plano, así que
/// cualquier input produce algún resultado, aunque sea vacío).
pub fn parse(content: &str) -> AsmParseResult {
    let arm = es_arm(content);
    let is_att = !arm && is_att_syntax(content);
    let syntax: &'static str = if arm {
        "arm"
    } else if is_att {
        "att"
    } else {
        "intel"
    };
    let arquitectura: &'static str = if arm { "arm" } else { "x86" };

    // Si el archivo declara símbolos, mandan ellos; si no, la heurística.
    let declarados = simbolos_declarados(content);

    let mut procedures: Vec<AsmProcedure> = Vec::new();
    let mut current: Option<Proc> = None;

    for (idx, raw_line) in content.lines().enumerate() {
        let line_no = idx + 1;
        let stripped = strip_comment(raw_line).trim();
        if stripped.is_empty() {
            continue;
        }

        // **MASM abre y cierra procedimientos sin dos puntos**: `nombre PROC`
        // … `nombre ENDP`. Va ANTES del chequeo de label porque esas líneas no
        // tienen `:` y `split_label` no las vería nunca. Sin esto, un archivo
        // MASM declaraba sus símbolos (y por lo tanto apagaba la heurística)
        // pero no abría un solo procedimiento: todo caía en `<entry>`.
        {
            let mut it = stripped.split_whitespace();
            if let (Some(nombre), Some(kw)) = (it.next(), it.next()) {
                if kw.eq_ignore_ascii_case("proc") && is_label_candidate(nombre) {
                    if let Some(prev) = current.take() {
                        procedures.push(prev.finish());
                    }
                    current = Some(Proc::new(nombre, line_no, arquitectura));
                    continue;
                }
                if kw.eq_ignore_ascii_case("endp") {
                    if let Some(prev) = current.take() {
                        procedures.push(prev.finish());
                    }
                    continue;
                }
            }
        }

        // Ojo con el orden: un label local estilo GCC (`.L1:`) también
        // empieza con `.`, igual que una directiva real (`.text`) — hay que
        // chequear "¿es un label?" ANTES de descartarlo como directiva, o
        // todo local label se pierde silenciosamente (bug real, atrapado
        // por el test `salto_hacia_atras_a_label_local_es_on`).
        if let Some((label, rest)) = split_label(stripped) {
            let abre_procedimiento = if declarados.is_empty() {
                is_procedure_label(label)
            } else {
                declarados.contains(label)
            };
            if abre_procedimiento {
                if let Some(p) = current.take() {
                    procedures.push(p.finish());
                }
                current = Some(Proc::new(label, line_no, arquitectura));
            } else if let Some(p) = current.as_mut() {
                p.labels_seen.insert(label.to_string());
                p.end_line = line_no;
            }
            if let Some(rest) = rest {
                let p = current.get_or_insert_with(|| Proc::new("<entry>", line_no, arquitectura));
                process_instruction(rest, line_no, is_att, p);
            }
            continue;
        }

        if stripped.starts_with('.') {
            continue; // directiva real (.text/.globl/.cfi_*/etc.), no un label
        }

        // **`simbolo = valor` es una asignación del ensamblador, no una
        // instrucción.** `rustc --emit=asm` emite `@feat.00 = 0` en archivos
        // que por lo demás están vacíos, y contarla hacía que un archivo sin
        // una sola instrucción real pareciera tener una. Se exige que el `=`
        // esté suelto (no `addl $1, %eax` ni un `>=`) y que lo de la
        // izquierda sea un único token.
        if let Some((izq, der)) = stripped.split_once('=') {
            let izq_t = izq.trim();
            let es_comparacion = izq_t.ends_with('<') || izq_t.ends_with('>') || izq_t.ends_with('!') || der.starts_with('=');
            if !es_comparacion && !izq_t.is_empty() && !izq_t.contains(char::is_whitespace) {
                continue;
            }
        }

        let p = current.get_or_insert_with(|| Proc::new("<entry>", line_no, arquitectura));
        process_instruction(stripped, line_no, is_att, p);
    }
    if let Some(p) = current.take() {
        procedures.push(p.finish());
    }

    let call_graph = build_call_graph(&procedures);
    let structural_smells = structural_smells_asm(&procedures);
    AsmParseResult { syntax, procedures, call_graph, structural_smells }
}

fn build_call_graph(procedures: &[AsmProcedure]) -> Vec<CallEdge> {
    let names: HashSet<&str> = procedures.iter().map(|p| p.name.as_str()).collect();
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for p in procedures {
        for callee in &p.calls {
            if names.contains(callee.as_str()) && callee != &p.name {
                let key = format!("{}\u{2192}{}", p.name, callee);
                if seen.insert(key) {
                    edges.push(CallEdge { from: p.name.clone(), to: callee.clone() });
                }
            }
        }
    }
    edges
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Smells estructurales (Fase 22) ──

    #[test]
    fn un_procedimiento_largo_se_marca() {
        let cuerpo = "  mov %eax, %ebx\n".repeat(60);
        let src = format!(".text\nlarga:\n{cuerpo}  ret\n");
        let r = parse(&src);
        assert!(r.structural_smells.iter().any(|s| s.kind == "long_function"), "{:?}", r.structural_smells.len());
    }

    #[test]
    fn un_procedimiento_corto_no_produce_nada() {
        let r = parse(".text\nmain:\n  mov %eax, %ebx\n  ret\n");
        assert!(r.structural_smells.is_empty(), "{:?}", r.structural_smells.len());
    }


    #[test]
    fn detecta_att_por_registro_con_signo_porcentaje() {
        assert!(is_att_syntax("movl %eax, %ebx\n"));
    }

    #[test]
    fn detecta_att_por_inmediato_con_signo_dolar() {
        assert!(is_att_syntax("movl $5, %eax\n"));
    }

    #[test]
    fn detecta_intel_por_ausencia_de_sigilos_att() {
        assert!(!is_att_syntax("mov eax, 5\n"));
    }

    #[test]
    fn procedimiento_simple_sin_saltos_hacia_atras_es_o1() {
        let src = "my_func:\n    mov %eax, %ebx\n    ret\n";
        let result = parse(src);
        assert_eq!(result.procedures.len(), 1);
        assert_eq!(result.procedures[0].name, "my_func");
        assert_eq!(result.procedures[0].big_o, "O(1)");
    }

    #[test]
    fn salto_hacia_atras_a_label_local_es_on() {
        let src = "my_func:\n.L1:\n    dec %ecx\n    jnz .L1\n    ret\n";
        let result = parse(src);
        assert_eq!(result.procedures[0].big_o, "O(n)");
        assert!(result.procedures[0].complexity >= 2);
    }

    #[test]
    fn salto_hacia_adelante_no_cuenta_como_backward() {
        let src = "my_func:\n    cmp $0, %eax\n    je .L_end\n    nop\n.L_end:\n    ret\n";
        let result = parse(src);
        assert_eq!(result.procedures[0].big_o, "O(1)");
    }

    #[test]
    fn split_operandos_respeta_parentesis_con_coma_interna() {
        let ops = split_operands("(%rbp,%rax,4), %eax");
        assert_eq!(ops, vec!["(%rbp,%rax,4)".to_string(), "%eax".to_string()]);
    }

    #[test]
    fn extrae_registros_att() {
        let mut regs = HashSet::new();
        extract_registers_arch("%eax", true, false, &mut regs);
        extract_registers_arch("(%rbp,%rax,4)", true, false, &mut regs);
        assert!(regs.contains("eax"));
        assert!(regs.contains("rbp"));
        assert!(regs.contains("rax"));
    }

    #[test]
    fn extrae_registros_intel() {
        let mut regs = HashSet::new();
        extract_registers_arch("eax", false, false, &mut regs);
        extract_registers_arch("[rbp+8]", false, false, &mut regs);
        assert!(regs.contains("eax"));
        assert!(regs.contains("rbp"));
    }

    #[test]
    fn clasifica_mnemonico_conocido_por_categoria() {
        assert_eq!(mnemonic_info("mov").0, "data_movement");
        assert_eq!(mnemonic_info("add").0, "arithmetic");
        assert_eq!(mnemonic_info("jne").0, "control_flow");
        assert_eq!(mnemonic_info("push").0, "stack");
        assert_eq!(mnemonic_info("cmp").0, "comparison");
    }

    #[test]
    fn sufijo_de_tamano_att_se_reconoce_igual() {
        assert_eq!(mnemonic_info("movl").0, "data_movement");
        assert_eq!(mnemonic_info("addl").0, "arithmetic");
        assert_eq!(mnemonic_info("cmpq").0, "comparison");
    }

    #[test]
    fn jl_no_se_confunde_con_sufijo_de_tamano() {
        // "jl" (jump if less) NO es "j" + sufijo "l" — "j" no está en
        // SIZABLE_BASES, así que debe matchear directo la entrada "jl".
        assert_eq!(mnemonic_info("jl").0, "control_flow");
    }

    #[test]
    fn mnemonico_desconocido_es_other_sin_romper() {
        let (cat, expl) = mnemonic_info("vfmadd231ps");
        assert_eq!(cat, "other");
        assert!(!expl.is_empty());
    }

    #[test]
    fn call_graph_detecta_llamada_entre_procedimientos_conocidos() {
        let src = "helper:\n    ret\nmain:\n    call helper\n    ret\n";
        let result = parse(src);
        assert!(result.call_graph.iter().any(|e| e.from == "main" && e.to == "helper"));
    }

    #[test]
    fn label_puramente_numerico_no_abre_procedimiento_nuevo() {
        let src = "my_func:\n1:\n    dec %ecx\n    jnz 1b\n    ret\n";
        let result = parse(src);
        assert_eq!(result.procedures.len(), 1);
    }

    #[test]
    fn sintaxis_intel_end_to_end() {
        let src = "my_func:\n    mov eax, 5\n    add eax, 1\n    ret\n";
        let result = parse(src);
        assert_eq!(result.syntax, "intel");
        assert_eq!(result.procedures.len(), 1);
        assert!(result.procedures[0].registers_used.contains(&"eax".to_string()));
    }

    #[test]
    fn stack_frame_viaja_end_to_end_desde_parse() {
        // Fase 19, 3er bullet — `callingconv::analyze` corre por procedimiento
        // dentro de `Proc::finish()`; este test confirma que el resultado
        // realmente llega al `AsmProcedure` que `parse()` devuelve, no solo
        // que `callingconv.rs` funciona aislado (ya cubierto en sus propios
        // tests).
        let src = "my_func:\n    push %rbp\n    mov %rsp, %rbp\n    pop %rbp\n    ret\n";
        let result = parse(src);
        assert!(result.procedures[0].stack_frame.has_standard_prologue);
        assert!(result.procedures[0].stack_frame.has_standard_epilogue);
    }

    #[test]
    fn instrucciones_antes_de_cualquier_label_forman_procedimiento_entry() {
        let src = "    mov %eax, %ebx\n    ret\n";
        let result = parse(src);
        assert_eq!(result.procedures.len(), 1);
        assert_eq!(result.procedures[0].name, "<entry>");
    }

    #[test]
    fn directivas_y_comentarios_no_cuentan_como_instrucciones() {
        let src = "my_func:\n    .cfi_startproc\n    mov %eax, %ebx # comentario\n    ret\n";
        let result = parse(src);
        assert_eq!(result.procedures[0].instructions.len(), 2);
    }

    // ── Fase 32: ARM y cambio de contexto ───────────────────────────────

    /// **El bug que encontró medir.** El splitter ya contaba profundidad y
    /// su comentario ya decía lo correcto; solo no conocía `{}`, porque el
    /// módulo nació x86-only. `{r4-r11, lr}` salía partido en `{r4-r11` y
    /// `lr}`: dos operandos rotos, ninguno de los cuales es un registro.
    #[test]
    fn la_lista_de_registros_entre_llaves_es_un_solo_operando() {
        let ops = split_operands("sp!, {r4-r11, lr}");
        assert_eq!(ops.len(), 2, "{ops:?}");
        assert_eq!(ops[0], "sp!");
        assert_eq!(ops[1], "{r4-r11, lr}");
    }

    /// Y el caso de x86 que el splitter ya resolvía no se rompe.
    #[test]
    fn el_modo_de_direccionamiento_de_x86_sigue_entero() {
        let ops = split_operands("(%rbp,%rax,4), %eax");
        assert_eq!(ops.len(), 2, "{ops:?}");
    }

    #[test]
    fn un_archivo_de_arm_no_se_reporta_como_intel() {
        let src = "suma:\n    stmfd sp!, {r4-r11, lr}\n    mov r4, r0\n    ldmfd sp!, {r4-r11, pc}\n";
        assert_eq!(parse(src).syntax, "arm");
    }

    /// La evidencia es estrecha a propósito: un archivo sin `stm`/`ldm`, sin
    /// lista entre llaves y sin `bx`/`blx` se asume x86 — que es lo que ya
    /// pasaba en silencio.
    #[test]
    fn un_archivo_sin_evidencia_de_arm_sigue_siendo_x86() {
        assert_eq!(parse("f:\n    mov eax, 5\n    ret\n").syntax, "intel");
        assert_eq!(parse("f:\n    movl $5, %eax\n    ret\n").syntax, "att");
    }

    /// **Los registros de ARM eran invisibles**: `r0`-`r7` no están en la
    /// lista de x86 (ahí los numerados empiezan en `r8`), así que los
    /// argumentos de una función de ARM no se veían, y `r11` aparecía solo
    /// por coincidir con un registro de x86-64.
    #[test]
    fn los_registros_de_arm_se_extraen_incluido_el_rango_de_la_lista() {
        let src = "suma:\n    stmfd sp!, {r4-r11, lr}\n    add r4, r0, r1\n    ldmfd sp!, {r4-r11, pc}\n";
        let r = parse(src);
        let regs = &r.procedures[0].registers_used;
        for esperado in ["r0", "r1", "r4", "r7", "lr", "pc", "sp"] {
            assert!(regs.contains(&esperado.to_string()), "falta {esperado} en {regs:?}");
        }
    }

    #[test]
    fn pusha_y_stmfd_caen_en_la_categoria_de_cambio_de_contexto() {
        assert_eq!(mnemonic_info("pusha").0, "context_switch");
        assert_eq!(mnemonic_info("popad").0, "context_switch");
        assert_eq!(mnemonic_info("pushfq").0, "context_switch");
        assert_eq!(mnemonic_info("stmfd").0, "context_switch");
        assert_eq!(mnemonic_info("ldmia").0, "context_switch");
        // Y lo que no se reconoce sigue cayendo a `other`, sin inventar.
        assert_eq!(mnemonic_info("stmfdeq").0, "other");
    }

    /// De punta a punta: los bloques llegan al `AsmProcedure`.
    #[test]
    fn los_bloques_de_contexto_llegan_al_procedimiento() {
        let src = "suma:\n    stmfd sp!, {r4-r11, lr}\n    add r4, r0, r1\n    ldmfd sp!, {r4-r11, pc}\n";
        let p = &parse(src).procedures[0];
        assert_eq!(p.context_switch.len(), 2);
        assert_eq!(p.context_switch[0].direction, "save");
        assert!(p.context_switch[1].is_return);
        assert!(p.stack_frame.has_standard_prologue, "{}", p.stack_frame.explanation);
    }

    #[test]
    fn un_manejador_de_interrupcion_de_x86_muestra_su_par_pusha_popa() {
        let src = "manejador:\n    pusha\n    mov eax, 1\n    popa\n    ret\n";
        let p = &parse(src).procedures[0];
        assert_eq!(p.context_switch.len(), 2);
        assert_eq!(p.context_switch[0].direction, "save");
        assert_eq!(p.context_switch[1].direction, "restore");
        assert!(p.context_switch.iter().all(|b| b.invalid_in_x86_64));
    }

    /// Un archivo de x86 común no tiene ningún bloque de contexto — estas
    /// instrucciones son de código escrito a mano, no de salida de compilador.
    #[test]
    fn un_procedimiento_normal_de_x86_no_tiene_bloques_de_contexto() {
        let src = "f:\n    push %rbp\n    mov %rsp, %rbp\n    pop %rbp\n    ret\n";
        assert!(parse(src).procedures[0].context_switch.is_empty());
    }
    /// **Salida real de `gcc -S -O2` en Windows/MinGW.** Es el caso que
    /// rompía el módulo: COFF y Mach-O no usan el `.L` de ELF para sus
    /// etiquetas locales, así que `LFB0`, `L3` y `L4` pasaban la heurística
    /// de nombres y abrían cada una un procedimiento falso. La función real
    /// quedaba con CERO instrucciones y el cuerpo repartido en cuatro.
    ///
    /// La directiva `.globl` dice cuál es la función de verdad, y manda ella.
    #[test]
    fn salida_real_de_gcc_es_un_solo_procedimiento() {
        let src = "\t.file\t\"suma.c\"\n\
                   \t.text\n\
                   \t.p2align 4,,15\n\
                   \t.globl\t_suma_escalar\n\
                   \t.def\t_suma_escalar;\t.scl\t2;\t.type\t32;\t.endef\n\
                   _suma_escalar:\n\
                   LFB0:\n\
                   \t.cfi_startproc\n\
                   \tmovl\t8(%esp), %eax\n\
                   \ttestl\t%eax, %eax\n\
                   \tjle\tL4\n\
                   \tmovl\t4(%esp), %edx\n\
                   \txorl\t%ecx, %ecx\n\
                   L3:\n\
                   \taddl\t(%edx), %ecx\n\
                   \taddl\t$4, %edx\n\
                   \tcmpl\t%eax, %ecx\n\
                   \tjne\tL3\n\
                   L4:\n\
                   \txorl\t%eax, %eax\n\
                   \tret\n\
                   \t.cfi_endproc\n\
                   LFE0:\n";
        let r = parse(src);
        assert_eq!(r.procedures.len(), 1, "procedimientos: {:?}", r.procedures.iter().map(|p| &p.name).collect::<Vec<_>>());
        let p = &r.procedures[0];
        assert_eq!(p.name, "_suma_escalar");
        // Todas las instrucciones del cuerpo cuentan para la función, no se
        // reparten entre etiquetas locales.
        assert!(p.instructions.len() >= 10, "solo {} instrucciones", p.instructions.len());
        // `jne L3` salta hacia atrás: sigue siendo evidencia de bucle.
        assert_eq!(p.big_o, "O(n)");
    }

    /// **El separador es un TAB, no un espacio.** gcc emite `.globl\t_f`, y
    /// una comparación contra `".globl "` no matchea nada — el archivo
    /// entero parecería no declarar ningún símbolo y se volvería a la
    /// heurística, que es justo la que falla acá. Lo agarró medir contra
    /// salida real, no un test.
    #[test]
    fn la_directiva_se_reconoce_con_tabulacion() {
        let src = "\t.globl\tmi_func\nmi_func:\nLBB0_1:\n\tmovl\t%eax, %ebx\n\tret\n";
        let r = parse(src);
        assert_eq!(r.procedures.len(), 1);
        assert_eq!(r.procedures[0].name, "mi_func");
    }

    /// Forma ELF: `.type nombre, @function` es la que dice explícitamente
    /// que el símbolo es una función.
    #[test]
    fn el_tipo_function_de_elf_tambien_declara() {
        let src = "\t.type\tsumar, @function\nsumar:\n.L2:\n\taddl\t%eax, %ebx\n\tret\n";
        let r = parse(src);
        assert_eq!(r.procedures.len(), 1);
        assert_eq!(r.procedures[0].name, "sumar");
    }

    /// MASM, sintaxis Intel: `nombre PROC`.
    #[test]
    fn masm_declara_con_proc() {
        let src = "mi_proc PROC\n  mov eax, 1\n  ret\nmi_proc ENDP\n";
        let r = parse(src);
        assert!(r.procedures.iter().any(|p| p.name == "mi_proc"), "{:?}", r.procedures.iter().map(|p| &p.name).collect::<Vec<_>>());
    }

    /// **Y lo que NO cambia: ensamblador escrito a mano, sin una sola
    /// directiva, sigue usando la heurística de nombres de siempre.** No se
    /// reemplazó una regla por otra — se usa la buena cuando hay con qué.
    #[test]
    fn sin_directivas_sigue_valiendo_la_heuristica_de_nombres() {
        let src = "primera:\n  mov eax, 1\n  ret\nsegunda:\n  mov ebx, 2\n  ret\n";
        let r = parse(src);
        assert_eq!(r.procedures.len(), 2);
        assert_eq!(r.procedures[0].name, "primera");
        assert_eq!(r.procedures[1].name, "segunda");
    }

    // ── SIMD ──────────────────────────────────────────────────────────────

    /// **El test que impide que esto mienta.** `addss` usa `xmm` y opera
    /// sobre UN elemento: es lo que emite cualquier compilador para
    /// aritmética de punto flotante común desde SSE2. Contar el registro y
    /// decir "vectorizado" daría un sí para casi todo programa con un `float`.
    #[test]
    fn usar_xmm_no_alcanza_para_decir_vectorizado() {
        let src = "f:\n  movss (%rdi), %xmm0\n  addss (%rsi), %xmm0\n  movss %xmm0, (%rdi)\n  ret\n";
        let s = parse(src).procedures[0].simd.clone().expect("tenía que detectar SIMD");
        assert!(!s.vectorized, "scalar no es vectorizado");
        assert_eq!(s.packed_instructions, 0);
        assert_eq!(s.scalar_simd_instructions, 3);
        assert_eq!(s.isa, "sse");
        assert_eq!(s.max_width_bits, 128);
    }

    /// Y con packed sí: una instrucción, varios elementos.
    #[test]
    fn packed_si_es_vectorizado() {
        let src = "f:\n  movaps (%rdi), %xmm0\n  mulps (%rsi), %xmm0\n  movaps %xmm0, (%rdi)\n  ret\n";
        let s = parse(src).procedures[0].simd.clone().unwrap();
        assert!(s.vectorized);
        assert_eq!(s.packed_instructions, 3);
        assert_eq!(s.isa, "sse");
    }

    /// `ymm` no existe fuera de AVX, y con packed de por medio es AVX2.
    #[test]
    fn ymm_con_packed_es_avx2_de_256_bits() {
        let src = "f:\n  vmovaps (%rdi), %ymm0\n  vmulps (%rsi), %ymm0, %ymm1\n  vmovaps %ymm1, (%rdi)\n  ret\n";
        let s = parse(src).procedures[0].simd.clone().unwrap();
        assert_eq!(s.isa, "avx2");
        assert_eq!(s.max_width_bits, 256);
        assert!(s.vectorized);
    }

    /// `zmm` solo existe en AVX-512.
    #[test]
    fn zmm_es_avx512_de_512_bits() {
        let src = "f:\n  vmovaps (%rdi), %zmm0\n  vaddps (%rsi), %zmm0, %zmm1\n  ret\n";
        let s = parse(src).procedures[0].simd.clone().unwrap();
        assert_eq!(s.isa, "avx512");
        assert_eq!(s.max_width_bits, 512);
    }

    /// **Un registro de máscara es evidencia de AVX-512 aunque no haya un
    /// `zmm`**: AVX-512 permite enmascarar también sobre `xmm`/`ymm`.
    #[test]
    fn un_registro_de_mascara_delata_avx512_sin_zmm() {
        let src = "f:\n  vaddps %ymm1, %ymm2, %ymm3{%k1}\n  ret\n";
        let s = parse(src).procedures[0].simd.clone().unwrap();
        assert_eq!(s.isa, "avx512", "la máscara manda sobre el ancho");
    }

    /// El prefijo VEX (`v…`) distingue AVX-128 de SSE sobre los mismos `xmm`.
    #[test]
    fn el_prefijo_vex_sobre_xmm_es_avx_no_sse() {
        let sse = parse("f:\n  addps %xmm1, %xmm0\n  ret\n").procedures[0].simd.clone().unwrap();
        let avx = parse("f:\n  vaddps %xmm1, %xmm2, %xmm0\n  ret\n").procedures[0].simd.clone().unwrap();
        assert_eq!(sse.isa, "sse");
        assert_eq!(avx.isa, "avx");
        assert_eq!(sse.max_width_bits, avx.max_width_bits, "el ancho es el mismo; cambia la codificación");
    }

    /// Enteros empaquetados: `paddd` no termina en `ps`/`pd` y es packed igual.
    #[test]
    fn los_enteros_empaquetados_cuentan_como_packed() {
        let s = parse("f:\n  paddd %xmm1, %xmm0\n  ret\n").procedures[0].simd.clone().unwrap();
        assert!(s.vectorized);
        assert_eq!(s.packed_instructions, 1);
    }

    /// **Ausente no es lo mismo que cero**: un procedimiento sin un solo
    /// registro vectorial no tiene que mostrar una sección de SIMD vacía.
    #[test]
    fn sin_registros_vectoriales_no_hay_seccion_de_simd() {
        assert!(parse("f:\n  mov %eax, %ebx\n  ret\n").procedures[0].simd.is_none());
    }

    /// Los registros vectoriales ahora también salen en `registers_used`:
    /// antes `ymm`/`zmm` no estaban en ninguna lista y se perdían.
    #[test]
    fn los_registros_vectoriales_entran_en_registers_used() {
        let p = &parse("f:\n  vaddps %ymm1, %ymm2, %ymm3\n  ret\n").procedures[0];
        assert!(p.registers_used.iter().any(|r| r == "ymm1"), "{:?}", p.registers_used);
        assert!(p.registers_used.iter().any(|r| r == "ymm3"), "{:?}", p.registers_used);
    }

}
