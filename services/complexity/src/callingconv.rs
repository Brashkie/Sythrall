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

use crate::asmparse::AsmInstruction;

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

fn parse_immediate(operand: &str) -> Option<i64> {
    strip_sigil(operand.trim_start_matches('$')).parse::<i64>().ok()
}

/// Punto de entrada — recibe las instrucciones de UN procedimiento que
/// `asmparse.rs` ya extrajo, más si ese procedimiento es leaf (`calls`
/// vacío, ya calculado ahí también). Infalible: una lista vacía de
/// instrucciones da `StackFrameInfo` todo en `false`/`None`, no un error.
pub fn analyze(instructions: &[AsmInstruction], is_leaf_function: bool) -> StackFrameInfo {
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

    let explanation = explain(has_standard_prologue, has_standard_epilogue, is_leaf_function, local_stack_bytes);

    StackFrameInfo { has_standard_prologue, has_standard_epilogue, is_leaf_function, local_stack_bytes, explanation }
}

fn explain(prologue: bool, epilogue: bool, is_leaf: bool, local_bytes: Option<i64>) -> String {
    if prologue && epilogue {
        let bytes_note = match local_bytes {
            Some(n) if n > 0 => format!(" Reserva {n} bytes de variables locales en la pila (`sub` sobre el stack pointer justo después del prólogo)."),
            _ => String::new(),
        };
        format!(
            "Prólogo/epílogo estándar detectado: `push` del base pointer seguido de `mov` del stack pointer al base pointer arma un stack frame explícito; se deshace simétricamente antes de `ret` (`leave` o `pop` del base pointer).{bytes_note} Esto es lo que permite reconstruir la pila de llamadas (backtrace) incluso sin símbolos de depuración — cada frame apunta al anterior."
        )
    } else if is_leaf {
        "No se detectó el prólogo/epílogo estándar de base pointer — consistente con una función leaf (no llama a nada) bajo frame-pointer omission (`-fomit-frame-pointer`, activado por default en optimización moderna de GCC/Clang para x86-64): sin llamadas propias que hagan falta desenrollar, un frame explícito no aporta nada y el registro de base pointer queda libre para uso general.".to_string()
    } else {
        "No se detectó el prólogo/epílogo estándar de base pointer (`push`+`mov` al empezar, `leave`/`pop` antes de `ret`) — puede ser frame-pointer omission del compilador en una función no-leaf (más difícil de depurar con un backtrace clásico, pero válido), otro ABI, código escrito a mano, o simplemente una forma que este heurístico posicional no reconoce.".to_string()
    }
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
        let info = analyze(&instrs, false);
        assert!(info.has_standard_prologue);
        assert!(info.has_standard_epilogue);
    }

    #[test]
    fn epilogo_con_leave_tambien_se_detecta() {
        let instrs = vec![ins("push", &["%rbp"]), ins("mov", &["%rsp", "%rbp"]), ins("leave", &[]), ins("ret", &[])];
        let info = analyze(&instrs, false);
        assert!(info.has_standard_epilogue);
    }

    #[test]
    fn orden_de_operandos_intel_tambien_se_reconoce() {
        // Intel es dst, src (al revés de AT&T) — `mov ebp, esp` arma el
        // frame igual que `mov %rsp, %rbp` en AT&T, solo cambia el orden.
        let instrs = vec![ins("push", &["ebp"]), ins("mov", &["ebp", "esp"]), ins("pop", &["ebp"]), ins("ret", &[])];
        let info = analyze(&instrs, false);
        assert!(info.has_standard_prologue);
    }

    #[test]
    fn sub_inmediato_despues_del_prologo_da_bytes_locales() {
        let instrs = vec![ins("push", &["%rbp"]), ins("mov", &["%rsp", "%rbp"]), ins("sub", &["$32", "%rsp"]), ins("pop", &["%rbp"]), ins("ret", &[])];
        let info = analyze(&instrs, false);
        assert_eq!(info.local_stack_bytes, Some(32));
    }

    #[test]
    fn sin_sub_despues_del_prologo_no_hay_bytes_locales() {
        let instrs = vec![ins("push", &["%rbp"]), ins("mov", &["%rsp", "%rbp"]), ins("pop", &["%rbp"]), ins("ret", &[])];
        let info = analyze(&instrs, false);
        assert_eq!(info.local_stack_bytes, None);
    }

    #[test]
    fn funcion_leaf_sin_prologo_se_explica_como_frame_pointer_omission() {
        let instrs = vec![ins("mov", &["%edi", "%eax"]), ins("ret", &[])];
        let info = analyze(&instrs, true);
        assert!(!info.has_standard_prologue);
        assert!(info.is_leaf_function);
        assert!(info.explanation.contains("frame-pointer omission"));
    }

    #[test]
    fn funcion_no_leaf_sin_prologo_no_se_confunde_con_leaf() {
        let instrs = vec![ins("call", &["helper"]), ins("ret", &[])];
        let info = analyze(&instrs, false);
        assert!(!info.has_standard_prologue);
        assert!(!info.is_leaf_function);
    }

    #[test]
    fn sufijo_de_tamano_att_no_rompe_la_deteccion() {
        // `pushl`/`movl`/`popl` — el sufijo de tamaño de 32-bit AT&T no debe
        // impedir el match contra "push"/"mov"/"pop".
        let instrs = vec![ins("pushl", &["%ebp"]), ins("movl", &["%esp", "%ebp"]), ins("popl", &["%ebp"]), ins("ret", &[])];
        let info = analyze(&instrs, false);
        assert!(info.has_standard_prologue);
        assert!(info.has_standard_epilogue);
    }

    #[test]
    fn instrucciones_vacias_no_rompe_nada() {
        let info = analyze(&[], true);
        assert!(!info.has_standard_prologue);
        assert!(!info.has_standard_epilogue);
    }
}
