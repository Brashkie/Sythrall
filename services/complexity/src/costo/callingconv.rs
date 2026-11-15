//! Fase 19 (Machine Intelligence) — 3er bullet: explicadores de
//! calling-convention y stack-frame, atados a la vista de Assembly que el
//! 1er bullet ya shippeó (`asmparse.rs`). Pura reinterpretación de las
//! `AsmInstruction` que `asmparse.rs` ya extrajo para cada `AsmProcedure` —
//! cero parsing nuevo, mismo criterio que `modernization.rs` reinterpretando
//! `memlayout.rs::allocations`.
//!
//! Deliberadamente heurístico y posicional, no un desensamblador ni un CFG:
//! reconoce la FORMA textual del prólogo/epílogo estándar x86 —
//! `push <bp>` seguido de `mov <sp>, <bp>` (o `<bp>, <sp>` en Intel, orden de
//! operandos invertido respecto de AT&T) al empezar el procedimiento, y
//! `leave` o `pop <bp>` justo antes del último `ret` al terminar — mirando
//! solo posiciones fijas (primeras 2-3 instrucciones, últimas 2), no
//! reconstruye el flujo de control real. Un procedimiento con una forma
//! atípica (múltiples `ret`, saltos computados hacia el epílogo) puede
//! clasificarse mal — límite aceptado, no oculto.
//!
//! **Frame-pointer omission es un resultado válido y esperado, no un
//! error de este detector**: GCC/Clang con optimización moderna
//! (`-fomit-frame-pointer`, default en x86-64 desde hace años) rutinariamente
//! no arma un frame explícito, sobre todo en funciones leaf (que no llaman a
//! nada, así que no necesitan preservar nada del caller en la pila). Esto se
//! explica como tal, nunca se reporta como "prólogo roto".
//!
//! Deliberadamente NO intentado acá: el 2do bullet de esta fase (wrapping
//! Capstone/LIEF sobre ejecutables reales PE/ELF/Mach-O) — este módulo solo
//! ve las instrucciones que `asmparse.rs` ya extrajo de texto `.s`/`.asm`
//! pegado/subido por el usuario, nunca un binario compilado ni un proceso
//! corriendo.

use serde::Serialize;

use crate::lenguajes::asmparse::AsmInstruction;

#[derive(Serialize, Clone)]
pub struct StackFrameInfo {
    pub has_standard_prologue: bool,
    pub has_standard_epilogue: bool,
    pub is_leaf_function: bool,
    pub local_stack_bytes: Option<i64>,
    pub explanation: String,
}

/// Mismo criterio que `asmparse::strip_att_size_suffix`, duplicado acá
/// (no expuesto como `pub(crate)` desde `asmparse.rs`) porque el mnemónico
/// guardado en `AsmInstruction` es la forma cruda (`pushl`, `movl`), nunca
/// la base ya pelada — mismo estilo de "cada módulo se basta a sí mismo con
/// sus propios helpers chicos" que `zig_bench.rs`/`asm_bench.rs` ya siguen.
fn base_mnemonic(m: &str) -> &str {
    if m.len() > 1 {
        let (base, suffix) = m.split_at(m.len() - 1);
        if matches!(suffix, "b" | "w" | "l" | "q") && matches!(base, "push" | "pop" | "mov" | "sub" | "add") {
            return base;
        }
    }
    m
}

fn strip_sigil(operand: &str) -> &str {
    operand.trim_start_matches('%')
}

fn is_bp(operand: &str) -> bool {
    matches!(strip_sigil(operand), "rbp" | "ebp" | "bp")
}

fn is_sp(operand: &str) -> bool {
    matches!(strip_sigil(operand), "rsp" | "esp" | "sp")
}

/// Los tres sigilos de inmediato que aparecen aca: `$N` (AT&T), `#N` (ARM)
/// y sin nada (Intel).
fn parse_immediate(operand: &str) -> Option<i64> {
    strip_sigil(operand.trim_start_matches('$').trim_start_matches('#')).parse::<i64>().ok()
}

/// Punto de entrada — recibe las instrucciones de UN procedimiento que
/// `asmparse.rs` ya extrajo, más si ese procedimiento es leaf (`calls`
/// vacío, ya calculado ahí también). Infalible: una lista vacía de
/// instrucciones da `StackFrameInfo` todo en `false`/`None`, no un error.
pub fn analyze(instructions: &[AsmInstruction], is_leaf_function: bool, arquitectura: &str) -> StackFrameInfo {
    // Fase 32 — antes de esto, un procedimiento de ARM salía como "sin
    // prólogo estándar" y se explicaba con `-fomit-frame-pointer`, un flag
    // de GCC para x86-64. No era un hueco: era una explicación falsa sobre
    // código que SÍ tiene prólogo, solo que el de su arquitectura.
    if arquitectura == "arm" {
        return analizar_arm(instructions, is_leaf_function);
    }
    let has_push_bp = instructions.first().is_some_and(|i| base_mnemonic(&i.mnemonic) == "push" && i.operands.first().is_some_and(|o| is_bp(o)));

    let has_standard_prologue = has_push_bp
        && instructions.get(1).is_some_and(|i| {
            base_mnemonic(&i.mnemonic) == "mov"
                && i.operands.len() == 2
                && ((is_sp(&i.operands[0]) && is_bp(&i.operands[1])) || (is_bp(&i.operands[0]) && is_sp(&i.operands[1])))
        });

    // El `sub` que reserva espacio para variables locales viene justo
    // después del prólogo (`push bp; mov sp, bp; sub $N, sp`) — si no está
    // ahí, no se busca más lejos: no hay garantía de que un `sub` posterior
    // sobre `sp` sea parte del prólogo y no, por ejemplo, un `alloca`.
    let local_stack_bytes = has_standard_prologue
        .then(|| instructions.get(2))
        .flatten()
        .filter(|i| base_mnemonic(&i.mnemonic) == "sub" && i.operands.iter().any(|o| is_sp(o)))
        .and_then(|i| i.operands.iter().find_map(|o| parse_immediate(o)));

    let has_standard_epilogue = instructions
        .iter()
        .rposition(|i| i.mnemonic == "ret")
        .filter(|&idx| idx > 0)
        .and_then(|idx| instructions.get(idx - 1))
        .is_some_and(|prev| {
            let base = base_mnemonic(&prev.mnemonic);
            base == "leave" || (base == "pop" && prev.operands.first().is_some_and(|o| is_bp(o)))
        });

    // Si el procedimiento usa `PUSHA`/`POPA`, el archivo NO es x86-64 — esas
    // instrucciones se eliminaron del modo de 64 bits. Explicar entonces la
    // ausencia de frame pointer con "el default de GCC/Clang para x86-64"
    // sería citar un default que no rige acá. Lo encontró volver a medir
    // después de agregar los bloques de contexto, no un test.
    let es_16_o_32_bits = context_blocks(instructions).iter().any(|b| b.invalid_in_x86_64);
    let explanation = explain(has_standard_prologue, has_standard_epilogue, is_leaf_function, local_stack_bytes, es_16_o_32_bits);

    StackFrameInfo { has_standard_prologue, has_standard_epilogue, is_leaf_function, local_stack_bytes, explanation }
}

/// El prólogo/epílogo de ARM, que no se parece en nada al de x86.
///
/// En x86 el frame se arma con dos instrucciones (`push bp; mov sp, bp`) y
/// el retorno es un `ret` aparte. En ARM **una sola instrucción** guarda los
/// registros preservados más `lr`, y **otra sola** los restaura metiendo la
/// dirección de vuelta directamente en `pc`, con lo cual restaurar y
/// retornar son el mismo acto.
fn analizar_arm(instructions: &[AsmInstruction], is_leaf_function: bool) -> StackFrameInfo {
    let bloques = context_blocks(instructions);
    let has_standard_prologue = context_blocks(&instructions[..instructions.len().min(1)]).first().is_some_and(|b| b.direction == "save");
    let restauracion = bloques.iter().rev().find(|b| b.direction == "restore");
    let vuelve_por_pc = restauracion.is_some_and(|b| b.is_return);
    let has_standard_epilogue = vuelve_por_pc || instructions.last().is_some_and(|i| i.mnemonic.eq_ignore_ascii_case("bx") && i.operands.first().is_some_and(|o| o.eq_ignore_ascii_case("lr")));

    // `sub sp, sp, #N` — la reserva de locales de ARM, con tres operandos en
    // vez de los dos de x86.
    let local_stack_bytes = instructions
        .iter()
        .take(3)
        .find(|i| i.mnemonic.eq_ignore_ascii_case("sub") && i.operands.first().is_some_and(|o| is_sp(o)))
        .and_then(|i| i.operands.iter().find_map(|o| parse_immediate(o)));

    let explanation = if has_standard_prologue && has_standard_epilogue {
        let nota_pc = if vuelve_por_pc {
            " El epílogo restaura dentro de `pc`, así que devolver el contexto y volver al llamador son la misma instrucción — por eso el prólogo apila `lr` y no `pc`."
        } else {
            " El retorno es un `bx lr` aparte, la forma que hace falta cuando el epílogo no puede escribir `pc` directamente (código Thumb interoperando, por ejemplo)."
        };
        let nota_bytes = match local_stack_bytes {
            Some(n) if n > 0 => format!(" Reserva {n} bytes de locales con un `sub` sobre el stack pointer."),
            _ => String::new(),
        };
        format!("Prólogo/epílogo estándar de ARM: una transferencia múltiple guarda los registros preservados más el link register de una sola vez, y otra los restaura.{nota_pc}{nota_bytes} No hay `push`/`mov` de base pointer como en x86 — no es que falte, es que ARM no arma el frame así.")
    } else if is_leaf_function {
        "No se detectó el prólogo/epílogo de transferencia múltiple — consistente con una función leaf de ARM: si no llama a nada, `lr` no se pisa y no hay nada que preservar, así que guardar contexto sería trabajo puro perdido.".to_string()
    } else {
        "No se detectó el prólogo/epílogo de transferencia múltiple de ARM (`stm` al entrar, `ldm` o `bx lr` al salir) en una función que sí llama a otras — puede ser una forma que este heurístico posicional no reconoce, un `stm` con código de condición (que deliberadamente no se reconoce), o un ABI distinto.".to_string()
    };

    StackFrameInfo { has_standard_prologue, has_standard_epilogue, is_leaf_function, local_stack_bytes, explanation }
}

fn explain(prologue: bool, epilogue: bool, is_leaf: bool, local_bytes: Option<i64>, es_16_o_32_bits: bool) -> String {
    if prologue && epilogue {
        let bytes_note = match local_bytes {
            Some(n) if n > 0 => format!(" Reserva {n} bytes de variables locales en la pila (`sub` sobre el stack pointer justo después del prólogo)."),
            _ => String::new(),
        };
        format!(
            "Prólogo/epílogo estándar detectado: `push` del base pointer seguido de `mov` del stack pointer al base pointer arma un stack frame explícito; se deshace simétricamente antes de `ret` (`leave` o `pop` del base pointer).{bytes_note} Esto es lo que permite reconstruir la pila de llamadas (backtrace) incluso sin símbolos de depuración — cada frame apunta al anterior."
        )
    } else if is_leaf && es_16_o_32_bits {
        "No se detectó el prólogo/epílogo estándar de base pointer, y este archivo usa `PUSHA`/`POPA`, así que es código de 16 o 32 bits — no se le puede atribuir el `-fomit-frame-pointer` que GCC/Clang activan por default en x86-64. En código escrito a mano (un manejador de interrupción, típicamente) armar un frame explícito simplemente no hace falta: el contexto se guarda entero de un golpe y se restaura igual.".to_string()
    } else if is_leaf {
        "No se detectó el prólogo/epílogo estándar de base pointer — consistente con una función leaf (no llama a nada) bajo frame-pointer omission (`-fomit-frame-pointer`, activado por default en optimización moderna de GCC/Clang para x86-64): sin llamadas propias que hagan falta desenrollar, un frame explícito no aporta nada y el registro de base pointer queda libre para uso general.".to_string()
    } else {
        "No se detectó el prólogo/epílogo estándar de base pointer (`push`+`mov` al empezar, `leave`/`pop` antes de `ret`) — puede ser frame-pointer omission del compilador en una función no-leaf (más difícil de depurar con un backtrace clásico, pero válido), otro ABI, código escrito a mano, o simplemente una forma que este heurístico posicional no reconoce.".to_string()
    }
}

// -------------------------------------------------------------------------
// Bloques de cambio de contexto (Fase 32, 2026-09-24)
// -------------------------------------------------------------------------

/// Un bloque de guardado o restauración de contexto: una sola instrucción
/// que toca **muchos** registros de golpe.
///
/// Existen porque el cambio de contexto es tiempo puro perdido para el
/// procesador —no calcula nada del programa, solo mueve estado— y una
/// instrucción que salva ocho registros tiene una codificación mucho más
/// corta que ocho instrucciones.
#[derive(Serialize, Clone)]
pub struct ContextSwitchBlock {
    pub line: usize,
    pub mnemonic: String,
    /// `"save"` o `"restore"`.
    pub direction: &'static str,
    /// Los registros que el bloque realmente toca, con los rangos de ARM ya
    /// expandidos (`{r4-r11, lr}` son nueve registros, no dos).
    pub registers: Vec<String>,
    pub register_count: usize,
    /// **ARM: restaurar dentro de `pc` ES retornar.** Escribir el program
    /// counter es un salto, así que `ldmfd sp!, {r4-r11, pc}` devuelve el
    /// contexto y vuelve al llamador en una sola instrucción — por eso el
    /// guardado simétrico apila `lr` y no `pc`.
    pub is_return: bool,
    /// **`PUSHA`/`POPA` no existen en x86-64**: se eliminaron del modo de
    /// 64 bits. Encontrar uno dice que el archivo es código de 16 o 32 bits,
    /// no que esté mal.
    pub invalid_in_x86_64: bool,
    pub explanation: String,
}

/// Los ocho registros que `PUSHAD`/`POPAD` tocan, **en el orden en que se
/// apilan**. No son "todos los registros": faltan EFLAGS (eso es `PUSHF`),
/// los de segmento, y todo el estado x87/SSE/AVX.
const GPR_32: &[&str] = &["eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi"];
const GPR_16: &[&str] = &["ax", "cx", "dx", "bx", "sp", "bp", "si", "di"];

/// Modos de direccionamiento de `STM`/`LDM`. Los cuatro primeros son los
/// nombres de pila (`fd` es el estándar de ARM) y los otros los aritméticos;
/// cada par es sinónimo del otro según sea store o load, y por eso el modo
/// se reporta pero no se usa para decidir nada.
const MODOS_ARM: &[(&str, &str)] = &[
    ("fd", "full descending, el modo de pila estándar de ARM"),
    ("fa", "full ascending"),
    ("ed", "empty descending"),
    ("ea", "empty ascending"),
    ("ia", "increment after"),
    ("ib", "increment before"),
    ("da", "decrement after"),
    ("db", "decrement before"),
];

/// Si el mnemónico es una transferencia múltiple de ARM, devuelve su
/// dirección y el modo de direccionamiento.
///
/// **Los códigos de condición (`stmfdeq`) no se reconocen a propósito**:
/// aceptarlos exigiría una segunda tabla de 15 sufijos combinables con los 8
/// modos, y un `stm` condicional en un prólogo es raro. Un mnemónico así cae
/// a `other`, que es lo que ya pasaba — no se afirma de menos, se deja de
/// reconocer algo que no se midió.
pub fn arm_bulk_transfer(mnemonic: &str) -> Option<(&'static str, &'static str)> {
    let m = mnemonic.to_ascii_lowercase();
    // Thumb-2 escribe `stmdb.w`; el sufijo dice el ancho de la codificación,
    // no cambia la semántica.
    let m = m.strip_suffix(".w").unwrap_or(&m);
    let (resto, direccion) = match m.strip_prefix("stm") {
        Some(r) => (r, "save"),
        None => (m.strip_prefix("ldm")?, "restore"),
    };
    if resto.is_empty() {
        return Some((direccion, "increment after, el modo por defecto cuando no se escribe ninguno"));
    }
    MODOS_ARM.iter().find(|(s, _)| *s == resto).map(|(_, d)| (direccion, *d))
}

fn indice_de_registro_arm(s: &str) -> Option<u8> {
    s.trim().strip_prefix('r')?.parse::<u8>().ok().filter(|n| *n <= 15)
}

/// Expande una lista de registros de ARM: `{r4-r11, lr}` son **nueve**
/// registros, no dos.
///
/// Un rango que no se entiende se deja como texto crudo en vez de
/// descartarlo o de inventar su contenido — mismo criterio por el que un
/// mnemónico desconocido cae a `other` con explicación genérica.
pub fn expandir_lista_de_registros(operando: &str) -> Vec<String> {
    let interior = operando.trim().trim_start_matches('{').trim_end_matches('}');
    let mut out = Vec::new();
    for parte in interior.split(',') {
        let p = parte.trim().to_ascii_lowercase();
        if p.is_empty() {
            continue;
        }
        if let Some((a, b)) = p.split_once('-') {
            if let (Some(ia), Some(ib)) = (indice_de_registro_arm(a), indice_de_registro_arm(b)) {
                if ia <= ib {
                    out.extend((ia..=ib).map(|n| format!("r{n}")));
                    continue;
                }
            }
        }
        out.push(p);
    }
    out
}

fn es_lista_de_registros(operando: &str) -> bool {
    operando.trim_start().starts_with('{')
}

/// Los bloques de cambio de contexto de un procedimiento.
///
/// **Sin hallazgo asociado, y el motivo importa.** Lo tentador sería marcar
/// un `pusha` como "no existe en x86-64", pero eso afirmaría que el archivo
/// ES x86-64, y no hay nada en el texto que lo pruebe: un `.s` de 32 bits es
/// perfectamente válido, y este repo mismo compila kernels con `-m32`. El
/// dato se explica; la acusación no se hace.
pub fn context_blocks(instructions: &[AsmInstruction]) -> Vec<ContextSwitchBlock> {
    let mut out = Vec::new();
    for i in instructions {
        let m = i.mnemonic.to_ascii_lowercase();

        // -- x86: los ocho registros de propósito general de un saque --
        if let Some((direccion, anchos)) = match m.as_str() {
            "pusha" => Some(("save", GPR_16)),
            "pushad" => Some(("save", GPR_32)),
            "popa" => Some(("restore", GPR_16)),
            "popad" => Some(("restore", GPR_32)),
            _ => None,
        } {
            let verbo = if direccion == "save" { "guarda" } else { "restaura" };
            let nota_popa = if direccion == "restore" {
                " Detalle que sorprende: descarta el stack pointer guardado en vez de cargarlo — lo incrementa como corresponde, porque restaurarlo de verdad rompería la propia secuencia de desapilado."
            } else {
                ""
            };
            out.push(ContextSwitchBlock {
                line: i.line,
                mnemonic: i.mnemonic.clone(),
                direction: direccion,
                registers: anchos.iter().map(|r| r.to_string()).collect(),
                register_count: anchos.len(),
                is_return: false,
                invalid_in_x86_64: true,
                explanation: format!(
                    "`{}` {verbo} los ocho registros de propósito general de una sola vez ({}). Existe para abaratar el cambio de contexto, que es tiempo puro perdido para el procesador: no calcula nada del programa, solo mueve estado. No guarda TODOS los registros: quedan afuera EFLAGS (eso es `PUSHF`/`POPF`), los registros de segmento y todo el estado x87/SSE/AVX — creer lo contrario es un bug real en un manejador de interrupciones.{nota_popa} Y no existe en x86-64: se eliminó del modo de 64 bits, así que este archivo es código de 16 o 32 bits. En procesadores modernos es además microcodificada y suele salir más cara que apilar solo los registros que hacen falta, por lo que ningún compilador la emite.",
                    i.mnemonic,
                    anchos.join(", ")
                ),
            });
            continue;
        }

        // -- ARM: transferencia múltiple con lista de registros --
        let Some(lista) = i.operands.iter().find(|o| es_lista_de_registros(o)) else { continue };
        let registros = expandir_lista_de_registros(lista);
        let escribe_pc = registros.iter().any(|r| r == "pc");

        let (direccion, modo) = match arm_bulk_transfer(&m) {
            Some(x) => x,
            // `push {r4, lr}` / `pop {r4, pc}` son los alias de Thumb para
            // `stmdb sp!` / `ldmia sp!`. Se reconocen por la lista entre
            // llaves, que es lo que los separa del `push` de un solo
            // operando de x86.
            None if m == "push" => ("save", "alias de `stmdb sp!`"),
            None if m == "pop" => ("restore", "alias de `ldmia sp!`"),
            None => continue,
        };

        let escritura_diferida = i.operands.iter().any(|o| o.trim_end().ends_with('!'));
        let nota_wb = if escritura_diferida {
            " El `!` es write-back: el registro base queda actualizado, que es lo que lo convierte en una operación de pila de verdad."
        } else {
            ""
        };
        let nota_pc = if escribe_pc {
            " Y restaura dentro de `pc`, así que ESTO ES EL RETORNO: escribir el program counter es un salto, de modo que una sola instrucción devuelve el contexto y vuelve al llamador. Por eso el guardado simétrico apila `lr` y no `pc` — lo que se guarda es la dirección de retorno, lo que se restaura es el salto a esa dirección."
        } else {
            ""
        };
        let verbo = if direccion == "save" { "guarda" } else { "restaura" };
        out.push(ContextSwitchBlock {
            line: i.line,
            mnemonic: i.mnemonic.clone(),
            direction: direccion,
            register_count: registros.len(),
            is_return: direccion == "restore" && escribe_pc,
            invalid_in_x86_64: false,
            explanation: format!(
                "`{}` {verbo} {} registros de una sola vez ({}) — {modo}. Es la forma en que ARM abarata el cambio de contexto: una instrucción con una lista, en vez de una por registro.{nota_wb}{nota_pc}",
                i.mnemonic,
                registros.len(),
                registros.join(", ")
            ),
            registers: registros,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ins(mnemonic: &str, operands: &[&str]) -> AsmInstruction {
        AsmInstruction {
            line: 1,
            mnemonic: mnemonic.to_string(),
            operands: operands.iter().map(|s| s.to_string()).collect(),
            category: "other",
            explanation: String::new(),
        }
    }

    #[test]
    fn prologo_y_epilogo_estandar_att_se_detectan() {
        let instrs = vec![ins("push", &["%rbp"]), ins("mov", &["%rsp", "%rbp"]), ins("mov", &["$0", "%eax"]), ins("pop", &["%rbp"]), ins("ret", &[])];
        let info = analyze(&instrs, false, "x86");
        assert!(info.has_standard_prologue);
        assert!(info.has_standard_epilogue);
    }

    #[test]
    fn epilogo_con_leave_tambien_se_detecta() {
        let instrs = vec![ins("push", &["%rbp"]), ins("mov", &["%rsp", "%rbp"]), ins("leave", &[]), ins("ret", &[])];
        let info = analyze(&instrs, false, "x86");
        assert!(info.has_standard_epilogue);
    }

    #[test]
    fn orden_de_operandos_intel_tambien_se_reconoce() {
        // Intel es dst, src (al revés de AT&T) — `mov ebp, esp` arma el
        // frame igual que `mov %rsp, %rbp` en AT&T, solo cambia el orden.
        let instrs = vec![ins("push", &["ebp"]), ins("mov", &["ebp", "esp"]), ins("pop", &["ebp"]), ins("ret", &[])];
        let info = analyze(&instrs, false, "x86");
        assert!(info.has_standard_prologue);
    }

    #[test]
    fn sub_inmediato_despues_del_prologo_da_bytes_locales() {
        let instrs = vec![ins("push", &["%rbp"]), ins("mov", &["%rsp", "%rbp"]), ins("sub", &["$32", "%rsp"]), ins("pop", &["%rbp"]), ins("ret", &[])];
        let info = analyze(&instrs, false, "x86");
        assert_eq!(info.local_stack_bytes, Some(32));
    }

    #[test]
    fn sin_sub_despues_del_prologo_no_hay_bytes_locales() {
        let instrs = vec![ins("push", &["%rbp"]), ins("mov", &["%rsp", "%rbp"]), ins("pop", &["%rbp"]), ins("ret", &[])];
        let info = analyze(&instrs, false, "x86");
        assert_eq!(info.local_stack_bytes, None);
    }

    #[test]
    fn funcion_leaf_sin_prologo_se_explica_como_frame_pointer_omission() {
        let instrs = vec![ins("mov", &["%edi", "%eax"]), ins("ret", &[])];
        let info = analyze(&instrs, true, "x86");
        assert!(!info.has_standard_prologue);
        assert!(info.is_leaf_function);
        assert!(info.explanation.contains("frame-pointer omission"));
    }

    #[test]
    fn funcion_no_leaf_sin_prologo_no_se_confunde_con_leaf() {
        let instrs = vec![ins("call", &["helper"]), ins("ret", &[])];
        let info = analyze(&instrs, false, "x86");
        assert!(!info.has_standard_prologue);
        assert!(!info.is_leaf_function);
    }

    #[test]
    fn sufijo_de_tamano_att_no_rompe_la_deteccion() {
        // `pushl`/`movl`/`popl` — el sufijo de tamaño de 32-bit AT&T no debe
        // impedir el match contra "push"/"mov"/"pop".
        let instrs = vec![ins("pushl", &["%ebp"]), ins("movl", &["%esp", "%ebp"]), ins("popl", &["%ebp"]), ins("ret", &[])];
        let info = analyze(&instrs, false, "x86");
        assert!(info.has_standard_prologue);
        assert!(info.has_standard_epilogue);
    }

    #[test]
    fn instrucciones_vacias_no_rompe_nada() {
        let info = analyze(&[], true, "x86");
        assert!(!info.has_standard_prologue);
        assert!(!info.has_standard_epilogue);
    }

    // ── Fase 32: bloques de cambio de contexto ──────────────────────────

    #[test]
    fn pusha_es_un_bloque_de_guardado_de_los_ocho_registros() {
        let b = context_blocks(&[ins("pusha", &[])]);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].direction, "save");
        assert_eq!(b[0].register_count, 8);
        assert!(!b[0].is_return);
    }

    /// **El dato más filoso de los cuatro, y el que corrige una creencia**:
    /// `PUSHA` no guarda "todos" los registros — las flags quedan afuera, y
    /// creer lo contrario es un bug real en un manejador de interrupciones.
    #[test]
    fn la_explicacion_dice_que_pusha_no_guarda_las_flags() {
        let b = context_blocks(&[ins("pushad", &[])]);
        assert!(b[0].explanation.contains("PUSHF"), "{}", b[0].explanation);
        assert!(b[0].explanation.contains("SSE"), "y tampoco el estado vectorial: {}", b[0].explanation);
    }

    /// Y que no existe en x86-64 — pero como DATO, no como acusación: el
    /// archivo puede ser legítimamente de 32 bits.
    #[test]
    fn pusha_se_marca_invalido_en_x86_64_sin_emitir_hallazgo() {
        let b = context_blocks(&[ins("pusha", &[])]);
        assert!(b[0].invalid_in_x86_64);
        assert!(b[0].explanation.contains("32 bits"), "{}", b[0].explanation);
    }

    #[test]
    fn popa_descarta_el_stack_pointer_guardado_y_lo_dice() {
        let b = context_blocks(&[ins("popad", &[])]);
        assert_eq!(b[0].direction, "restore");
        assert!(b[0].explanation.contains("descarta el stack pointer"), "{}", b[0].explanation);
    }

    /// `{r4-r11, lr}` son **nueve** registros, no dos: el rango se expande.
    #[test]
    fn el_rango_de_registros_de_arm_se_expande() {
        let b = context_blocks(&[ins("stmfd", &["sp!", "{r4-r11, lr}"])]);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].direction, "save");
        assert_eq!(b[0].register_count, 9, "{:?}", b[0].registers);
        assert!(b[0].registers.contains(&"r7".to_string()));
        assert!(b[0].registers.contains(&"lr".to_string()));
    }

    /// **En ARM, restaurar dentro de `pc` ES retornar.** Una sola
    /// instrucción devuelve el contexto y salta de vuelta, y por eso el
    /// guardado simétrico apila `lr` y no `pc`.
    #[test]
    fn restaurar_dentro_de_pc_es_el_retorno() {
        let b = context_blocks(&[ins("ldmfd", &["sp!", "{r4-r11, pc}"])]);
        assert_eq!(b[0].direction, "restore");
        assert!(b[0].is_return);
        assert!(b[0].explanation.contains("ESTO ES EL RETORNO"), "{}", b[0].explanation);

        // El guardado simétrico apila `lr`, y ESE no es un retorno.
        let g = context_blocks(&[ins("stmfd", &["sp!", "{r4-r11, lr}"])]);
        assert!(!g[0].is_return);
    }

    /// `push {r4, lr}` de Thumb es un bloque de contexto; `push %rbp` de x86
    /// no. Lo que los separa es la lista entre llaves, no el mnemónico.
    #[test]
    fn el_push_de_thumb_es_contexto_y_el_de_x86_no() {
        let arm = context_blocks(&[ins("push", &["{r4, r5, lr}"])]);
        assert_eq!(arm.len(), 1);
        assert_eq!(arm[0].register_count, 3);

        let x86 = context_blocks(&[ins("push", &["%rbp"])]);
        assert!(x86.is_empty(), "un push de un solo registro no es cambio de contexto");
    }

    #[test]
    fn el_write_back_se_explica_cuando_esta() {
        let con = context_blocks(&[ins("stmfd", &["sp!", "{r4, lr}"])]);
        assert!(con[0].explanation.contains("write-back"), "{}", con[0].explanation);
        let sin = context_blocks(&[ins("stmia", &["r0", "{r4, r5}"])]);
        assert!(!sin[0].explanation.contains("write-back"));
    }

    #[test]
    fn arm_bulk_transfer_reconoce_modos_y_rechaza_lo_que_no_midio() {
        assert_eq!(arm_bulk_transfer("stmfd").unwrap().0, "save");
        assert_eq!(arm_bulk_transfer("ldmia").unwrap().0, "restore");
        assert!(arm_bulk_transfer("stm").is_some(), "sin sufijo es el modo por defecto");
        assert!(arm_bulk_transfer("stmdb.w").is_some(), "el sufijo de ancho de Thumb-2 no cambia la semántica");
        // Con código de condición NO se reconoce, y eso es deliberado.
        assert!(arm_bulk_transfer("stmfdeq").is_none());
        assert!(arm_bulk_transfer("mov").is_none());
    }

    /// Un rango que no se entiende se deja crudo en vez de descartarlo o de
    /// inventar qué contiene.
    #[test]
    fn un_rango_ilegible_se_deja_crudo() {
        assert_eq!(expandir_lista_de_registros("{r4-r6}"), vec!["r4", "r5", "r6"]);
        assert_eq!(expandir_lista_de_registros("{r9-r4}"), vec!["r9-r4"], "un rango al revés no se adivina");
        assert_eq!(expandir_lista_de_registros("{d0-d3}"), vec!["d0-d3"], "los de punto flotante no se expanden, no se descartan");
    }

    /// **El defecto que esto arregla no era un hueco: era una explicación
    /// falsa.** Un procedimiento de ARM salía como "sin prólogo estándar" y
    /// se justificaba con `-fomit-frame-pointer`, un flag de GCC para x86-64.
    #[test]
    fn un_procedimiento_de_arm_ya_no_se_explica_con_un_flag_de_x86_64() {
        let instrs = vec![ins("stmfd", &["sp!", "{r4-r11, lr}"]), ins("mov", &["r4", "r0"]), ins("ldmfd", &["sp!", "{r4-r11, pc}"])];
        let info = analyze(&instrs, true, "arm");
        assert!(info.has_standard_prologue, "ARM SÍ tiene prólogo: {}", info.explanation);
        assert!(info.has_standard_epilogue);
        assert!(!info.explanation.contains("fomit-frame-pointer"), "{}", info.explanation);
        assert!(info.explanation.contains("ARM"), "{}", info.explanation);
    }

    /// Y `bx lr` es el otro epílogo válido de ARM, cuando el `ldm` no puede
    /// escribir `pc` directamente.
    #[test]
    fn bx_lr_tambien_cierra_un_procedimiento_de_arm() {
        let instrs = vec![ins("stmfd", &["sp!", "{r4, lr}"]), ins("ldmfd", &["sp!", "{r4, lr}"]), ins("bx", &["lr"])];
        let info = analyze(&instrs, false, "arm");
        assert!(info.has_standard_epilogue);
    }

    /// El `#N` de ARM es un inmediato igual que el `$N` de AT&T.
    #[test]
    fn el_inmediato_con_almohadilla_de_arm_se_lee() {
        let instrs = vec![ins("stmfd", &["sp!", "{r4, lr}"]), ins("sub", &["sp", "sp", "#16"]), ins("bx", &["lr"])];
        let info = analyze(&instrs, false, "arm");
        assert_eq!(info.local_stack_bytes, Some(16));
    }

    /// Una función leaf de ARM sin `stm` es correcta, y se explica con la
    /// razón de ARM (`lr` no se pisa), no con la de x86.
    #[test]
    fn una_funcion_leaf_de_arm_se_explica_con_la_razon_de_arm() {
        let instrs = vec![ins("mov", &["r0", "#1"]), ins("bx", &["lr"])];
        let info = analyze(&instrs, true, "arm");
        assert!(!info.has_standard_prologue);
        assert!(info.explanation.contains("lr"), "{}", info.explanation);
        assert!(!info.explanation.contains("fomit-frame-pointer"));
    }

    #[test]
    fn sin_instrucciones_no_hay_bloques_ni_panico() {
        assert!(context_blocks(&[]).is_empty());
        let info = analyze(&[], true, "arm");
        assert!(!info.has_standard_prologue);
    }

    /// **Residuo que encontró volver a medir, no un test.** Un archivo con
    /// `PUSHA` no puede ser x86-64 —la instrucción no existe en modo de 64
    /// bits—, así que explicar su falta de frame pointer citando el default
    /// de GCC/Clang *para x86-64* sería invocar una regla que no rige ahí.
    #[test]
    fn un_archivo_de_32_bits_no_se_explica_con_el_default_de_x86_64() {
        let instrs = vec![ins("pusha", &[]), ins("mov", &["eax", "1"]), ins("popa", &[]), ins("ret", &[])];
        let info = analyze(&instrs, true, "x86");
        // El mensaje SÍ nombra la bandera — para decir que no se le puede
        // atribuir. Nombrarla y descartarla explica más que callarla.
        assert!(info.explanation.contains("no se le puede atribuir"), "{}", info.explanation);
        assert!(info.explanation.contains("16 o 32 bits"), "{}", info.explanation);
    }

    /// Y una función leaf normal de x86-64 sigue explicándose como siempre.
    #[test]
    fn una_funcion_leaf_de_x86_64_sigue_citando_fomit_frame_pointer() {
        let instrs = vec![ins("mov", &["%eax", "$1"]), ins("ret", &[])];
        let info = analyze(&instrs, true, "x86");
        assert!(info.explanation.contains("fomit-frame-pointer"), "{}", info.explanation);
    }
}
