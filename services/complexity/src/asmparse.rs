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

use std::collections::HashSet;

use serde::Serialize;

use crate::callingconv::{self, StackFrameInfo};

#[derive(Serialize, Clone)]
pub struct AsmInstruction {
    pub line: usize,
    pub mnemonic: String,
    pub operands: Vec<String>,
    pub category: &'static str,
    pub explanation: String,
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
}

#[derive(Serialize)]
pub struct CallEdge {
    pub from: String,
    pub to: String,
}

#[derive(Serialize)]
pub struct AsmParseResult {
    pub syntax: &'static str,
    pub procedures: Vec<AsmProcedure>,
    pub call_graph: Vec<CallEdge>,
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

/// Split de operandos consciente de paréntesis — `(%rbp,%rax,4)` tiene una
/// coma que NO separa operandos, es parte del modo de direccionamiento.
fn split_operands(s: &str) -> Vec<String> {
    let mut operands = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for c in s.chars() {
        match c {
            '(' | '[' => {
                depth += 1;
                current.push(c);
            }
            ')' | ']' => {
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
    ("other", "instrucción no reconocida por la tabla de clasificación".to_string())
}

const INTEL_REGISTERS: &[&str] = &[
    "rax", "eax", "ax", "al", "ah", "rbx", "ebx", "bx", "bl", "bh", "rcx", "ecx", "cx", "cl", "ch", "rdx", "edx", "dx", "dl", "dh", "rsi", "esi", "si", "sil", "rdi", "edi", "di", "dil", "rbp", "ebp",
    "bp", "bpl", "rsp", "esp", "sp", "spl", "r8", "r8d", "r8w", "r8b", "r9", "r9d", "r9w", "r9b", "r10", "r10d", "r10w", "r10b", "r11", "r11d", "r11w", "r11b", "r12", "r12d", "r12w", "r12b", "r13",
    "r13d", "r13w", "r13b", "r14", "r14d", "r14w", "r14b", "r15", "r15d", "r15w", "r15b", "rip", "eip", "xmm0", "xmm1", "xmm2", "xmm3", "xmm4", "xmm5", "xmm6", "xmm7", "xmm8", "xmm9", "xmm10",
    "xmm11", "xmm12", "xmm13", "xmm14", "xmm15",
];

/// Extrae nombres de registro de un operando — en AT&T van prefijados con
/// `%` (inequívoco); en Intel no tienen sigilo, así que cada token se
/// compara contra `INTEL_REGISTERS` (sin esa lista no hay forma de
/// distinguir un registro de una etiqueta/símbolo en sintaxis Intel).
fn extract_registers(operand: &str, is_att: bool, out: &mut HashSet<String>) {
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
            if INTEL_REGISTERS.contains(&lower.as_str()) {
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
}

impl Proc {
    fn new(name: &str, line: usize) -> Self {
        let mut labels_seen = HashSet::new();
        labels_seen.insert(name.to_string());
        Proc { name: name.to_string(), start_line: line, end_line: line, instructions: Vec::new(), labels_seen, backward_jumps: 0, registers: HashSet::new(), calls: Vec::new() }
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
        let stack_frame = callingconv::analyze(&self.instructions, self.calls.is_empty());
        AsmProcedure {
            name: self.name,
            line: self.start_line,
            end_line: self.end_line,
            loc,
            complexity: 1 + self.backward_jumps,
            big_o,
            big_o_reason,
            instructions: self.instructions,
            registers_used,
            calls: self.calls,
            stack_frame,
        }
    }
}

fn process_instruction(line: &str, line_no: usize, is_att: bool, proc: &mut Proc) {
    let mut parts = line.splitn(2, char::is_whitespace);
    let mnemonic_raw = parts.next().unwrap_or("").trim();
    if mnemonic_raw.is_empty() {
        return;
    }
    let mnemonic = mnemonic_raw.to_lowercase();
    let rest = parts.next().unwrap_or("").trim();
    let operands = split_operands(rest);

    for op in &operands {
        extract_registers(op, is_att, &mut proc.registers);
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
    let is_att = is_att_syntax(content);
    let syntax: &'static str = if is_att { "att" } else { "intel" };

    let mut procedures: Vec<AsmProcedure> = Vec::new();
    let mut current: Option<Proc> = None;

    for (idx, raw_line) in content.lines().enumerate() {
        let line_no = idx + 1;
        let stripped = strip_comment(raw_line).trim();
        if stripped.is_empty() {
            continue;
        }

        // Ojo con el orden: un label local estilo GCC (`.L1:`) también
        // empieza con `.`, igual que una directiva real (`.text`) — hay que
        // chequear "¿es un label?" ANTES de descartarlo como directiva, o
        // todo local label se pierde silenciosamente (bug real, atrapado
        // por el test `salto_hacia_atras_a_label_local_es_on`).
        if let Some((label, rest)) = split_label(stripped) {
            if is_procedure_label(label) {
                if let Some(p) = current.take() {
                    procedures.push(p.finish());
                }
                current = Some(Proc::new(label, line_no));
            } else if let Some(p) = current.as_mut() {
                p.labels_seen.insert(label.to_string());
                p.end_line = line_no;
            }
            if let Some(rest) = rest {
                let p = current.get_or_insert_with(|| Proc::new("<entry>", line_no));
                process_instruction(rest, line_no, is_att, p);
            }
            continue;
        }

        if stripped.starts_with('.') {
            continue; // directiva real (.text/.globl/.cfi_*/etc.), no un label
        }

        let p = current.get_or_insert_with(|| Proc::new("<entry>", line_no));
        process_instruction(stripped, line_no, is_att, p);
    }
    if let Some(p) = current.take() {
        procedures.push(p.finish());
    }

    let call_graph = build_call_graph(&procedures);
    AsmParseResult { syntax, procedures, call_graph }
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
        extract_registers("%eax", true, &mut regs);
        extract_registers("(%rbp,%rax,4)", true, &mut regs);
        assert!(regs.contains("eax"));
        assert!(regs.contains("rbp"));
        assert!(regs.contains("rax"));
    }

    #[test]
    fn extrae_registros_intel() {
        let mut regs = HashSet::new();
        extract_registers("eax", false, &mut regs);
        extract_registers("[rbp+8]", false, &mut regs);
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
}
