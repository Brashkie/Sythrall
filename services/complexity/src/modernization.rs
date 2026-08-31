//! Fase 25 (Modernization Intelligence) — primer motor real, no más visión.
//! Entiende código C/C++ legacy y propone candidatos de modernización con
//! evidencia, nunca una conversión ciega — el mandato explícito de esta
//! fase: *"estudiar un sistema legacy y determinar qué partes son
//! candidatas a modernización"*, no un transpilador.
//!
//! Este primer corte trabaja 100% sobre datos que `memlayout.rs` (Fase 23)
//! ya calculó — cero AST nuevo, pura reinterpretación de `allocations` ya
//! detectadas: empareja cada sitio de adquisición (`malloc`/`calloc`/
//! `realloc`/`new`) con una liberación (`free`/`delete`) de la MISMA
//! variable en el mismo archivo. Deliberadamente sin scope real (misma
//! limitación que `memlayout.rs::resolve_target_variable` ya acepta): dos
//! funciones distintas con una variable local llamada igual podrían
//! empatarse por error — un heurístico honesto, no una garantía.
//!
//! Cinco patrones para esta porción (el 3ro, 4to y 5to se sumaron en pasadas
//! posteriores — ver el ROADMAP, Fase 25):
//! - **`manual_memory_raii`**: adquisición Y liberación encontradas — el
//!   par existe y está bien administrado a mano, pero un smart pointer
//!   (C++ moderno) o el ownership de Rust lo harían imposible de romper.
//! - **`unmatched_allocation`**: adquisición sin liberación emparejada en
//!   el mismo archivo — señal de alta confianza (leak real, u ownership
//!   transferido a algo que este heurístico no seguiría).
//! - **`double_release`**: la MISMA variable liberada más de una vez en el
//!   archivo — un double-free real (comportamiento indefinido en C/C++),
//!   no solo un candidato de modernización sino un bug de correctitud ya
//!   presente. Máxima confianza posible: contar liberaciones repetidas no
//!   tiene el mismo margen de error que emparejar adquisición↔liberación
//!   por nombre (esa parte sí puede confundir variables de scopes
//!   distintos con el mismo nombre — ver el límite documentado arriba).
//! - **`unsafe_realloc_reassignment`**: el patrón `p = realloc(p, size)` —
//!   `memlayout.rs` ya detecta esto (`AllocationSite::reassigns_same_pointer`)
//!   comparando el primer argumento de `realloc` contra la variable a la
//!   que se reasigna. Si `realloc` falla devuelve `NULL`, y esa
//!   reasignación pisa la única referencia que quedaba a la memoria
//!   original — un leak real y uno de los bugs de C más clásicos y
//!   documentados que existen. Máxima confianza, igual que `double_release`.
//! - **`use_after_free`**: la MISMA variable usada (leída, desreferenciada,
//!   pasada como argumento) después de liberarse, sin una reasignación a
//!   memoria nueva detectada entre medio. A diferencia de los otros
//!   patrones, este SÍ recorre el AST directamente (no solo reinterpreta
//!   `allocations`) para buscar el primer uso posterior dentro de la
//!   ventana [línea de liberación, próxima adquisición de la misma
//!   variable). Heurístico textual/de orden de líneas, no un análisis de
//!   flujo de control real — un `free` dentro de un `if` seguido de un uso
//!   incondicional puede ser un falso positivo si las ramas son mutuamente
//!   excluyentes; por eso confianza `medium`, no `high` como los patrones
//!   anteriores.
//!
//! Deliberadamente NO intentado acá: análisis de ownership a través de
//! fronteras de función, tracking de scope real (misma limitación de
//! `memlayout.rs::resolve_target_variable`), o generar el código migrado en
//! sí — esta fase entiende y propone, no convierte.

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use tree_sitter::Node;

use crate::memlayout::{self, MemoryLayoutResult};

#[derive(Serialize, Clone)]
pub struct ModernizationCandidate {
    pub variable: String,
    pub pattern: &'static str,
    pub line: usize,
    pub current: String,
    pub suggested_target: &'static str,
    pub reasoning: String,
    pub confidence: &'static str,
}

#[derive(Serialize)]
pub struct ModernizationSummary {
    pub total: usize,
    pub manual_memory_raii: usize,
    pub unmatched_allocation: usize,
    pub double_release: usize,
    pub unsafe_realloc_reassignment: usize,
    pub use_after_free: usize,
}

#[derive(Serialize)]
pub struct ModernizationReport {
    pub candidates: Vec<ModernizationCandidate>,
    pub summary: ModernizationSummary,
    pub note: String,
}

const NOTE: &str = "Candidatos de modernización derivados de los allocation sites que memlayout.rs ya detectó — empareja por nombre de variable dentro del mismo archivo, sin rastrear scope real (misma limitación que memlayout.rs ya documenta). Esto propone, nunca convierte código automáticamente.";

fn is_acquire(kind: &str) -> bool {
    matches!(kind, "malloc" | "calloc" | "realloc" | "new")
}

fn is_release(kind: &str) -> bool {
    matches!(kind, "free" | "delete")
}

fn candidate_for(variable: &str, acquire_kind: &str, line: usize, has_release: bool) -> ModernizationCandidate {
    if has_release {
        ModernizationCandidate {
            variable: variable.to_string(),
            pattern: "manual_memory_raii",
            line,
            current: format!("`{acquire_kind}` con liberación manual emparejada para `{variable}`"),
            suggested_target: "raii_smart_pointer",
            reasoning: format!(
                "`{variable}` se adquiere y libera a mano — un smart pointer (`unique_ptr`/`shared_ptr` en C++ moderno) o el modelo de ownership de Rust eliminarían la posibilidad de un `free`/`delete` olvidado o duplicado."
            ),
            confidence: "medium",
        }
    } else {
        ModernizationCandidate {
            variable: variable.to_string(),
            pattern: "unmatched_allocation",
            line,
            current: format!("`{acquire_kind}` de `{variable}` sin liberación emparejada detectada en el mismo archivo"),
            suggested_target: "rust_ownership",
            reasoning: format!(
                "No se encontró un `free`/`delete` correspondiente para `{variable}` en este archivo — puede ser un leak real, o el ownership se transfiere a otra parte que este heurístico no rastrea. El modelo de ownership de Rust obligaría a hacer explícito qué pasa con esta memoria en tiempo de compilación."
            ),
            confidence: "high",
        }
    }
}

/// Cada liberación MÁS ALLÁ DE LA PRIMERA para una misma variable es un
/// double-free real — no un candidato "podría mejorarse", un bug de
/// correctitud ya presente. Apunta a la línea de la liberación repetida
/// (donde el double-free ocurre de verdad), no a la primera liberación ni a
/// la adquisición original.
fn double_release_candidates(memory: &MemoryLayoutResult) -> Vec<ModernizationCandidate> {
    let mut release_count: HashMap<&str, usize> = HashMap::new();
    let mut out = Vec::new();
    for a in &memory.allocations {
        if !is_release(a.kind) {
            continue;
        }
        let Some(variable) = a.variable.as_deref() else { continue };
        let count = release_count.entry(variable).or_insert(0);
        *count += 1;
        if *count > 1 {
            out.push(ModernizationCandidate {
                variable: variable.to_string(),
                pattern: "double_release",
                line: a.line,
                current: format!("`{}` de `{variable}` — ya se había liberado antes en este archivo", a.kind),
                suggested_target: "rust_ownership",
                reasoning: format!(
                    "`{variable}` aparece liberado más de una vez en este archivo — un double-free real (comportamiento indefinido en C/C++). El modelo de ownership de Rust lo vuelve imposible de escribir: el compilador rechaza mover o liberar un valor ya movido."
                ),
                confidence: "high",
            });
        }
    }
    out
}

/// `p = realloc(p, size)` — `memlayout.rs` ya marcó cada sitio así con
/// `reassigns_same_pointer`, esta función solo lo convierte al shape de
/// `ModernizationCandidate` con su propia evidencia/razonamiento.
fn unsafe_realloc_candidates(memory: &MemoryLayoutResult) -> Vec<ModernizationCandidate> {
    memory
        .allocations
        .iter()
        .filter(|a| a.reassigns_same_pointer)
        .filter_map(|a| {
            let variable = a.variable.as_deref()?;
            Some(ModernizationCandidate {
                variable: variable.to_string(),
                pattern: "unsafe_realloc_reassignment",
                line: a.line,
                current: format!("`{variable} = realloc({variable}, ...)` — reasignación directa al mismo puntero"),
                suggested_target: "rust_ownership",
                reasoning: format!(
                    "si `realloc` falla devuelve `NULL`, y asignarlo directo a `{variable}` pisa la única referencia que quedaba a la memoria original — un leak real, uno de los bugs de C más clásicos. La forma segura en C es reasignar a una variable temporal y chequear antes de pisar `{variable}`; el `Vec`/ownership de Rust maneja el realloc internamente, esta clase de bug no puede escribirse."
                ),
                confidence: "high",
            })
        })
        .collect()
}

/// La próxima adquisición (`malloc`/`calloc`/`realloc`/`new`) de la MISMA
/// variable después de `after_line`, si existe — reasignar a memoria nueva
/// vuelve el puntero válido de nuevo, así que la ventana de "uso
/// sospechoso" termina ahí. Sin una adquisición posterior, la ventana llega
/// hasta el final del archivo (`usize::MAX`).
fn use_after_free_window_end(memory: &MemoryLayoutResult, variable: &str, after_line: usize) -> usize {
    memory
        .allocations
        .iter()
        .filter(|a| is_acquire(a.kind) && a.variable.as_deref() == Some(variable) && a.line > after_line)
        .map(|a| a.line)
        .min()
        .unwrap_or(usize::MAX)
}

/// Para cada liberación (`free`/`delete`) con variable resuelta, busca el
/// primer uso posterior de esa MISMA variable dentro de la ventana [línea
/// de liberación, próxima adquisición). Cuenta como "uso" cualquier
/// ocurrencia del identificador que NO sea el lado izquierdo de una
/// asignación (reasignar `p = NULL` es justo el idiom seguro que previene
/// esto, no un uso) y que no coincida con la línea de otro allocation site
/// ya conocido para la misma variable (evita duplicar evidencia con
/// `double_release`, que ya cubre una segunda liberación).
///
/// Único patrón de este módulo que recorre el AST directamente en vez de
/// solo reinterpretar `allocations` — necesario porque "uso" no es un
/// allocation site. Heurístico textual/de orden de líneas, no un análisis
/// de flujo de control real: un `free` dentro de un `if` seguido de un uso
/// incondicional puede ser un falso positivo si las ramas son mutuamente
/// excluyentes. Por eso reporta confianza `medium`, no `high`.
fn use_after_free_candidates(memory: &MemoryLayoutResult, root: Node, source: &str) -> Vec<ModernizationCandidate> {
    let mut out = Vec::new();
    for a in &memory.allocations {
        if !is_release(a.kind) {
            continue;
        }
        let Some(variable) = a.variable.as_deref() else { continue };
        let window_end = use_after_free_window_end(memory, variable, a.line);
        let known_allocation_lines: HashSet<usize> =
            memory.allocations.iter().filter(|o| o.variable.as_deref() == Some(variable)).map(|o| o.line).collect();

        let mut first_use: Option<usize> = None;
        memlayout::walk_flat(root, &mut |n| {
            if first_use.is_some() || n.kind() != "identifier" || memlayout::text_of(n, source) != variable {
                return;
            }
            let line = n.start_position().row + 1;
            if line <= a.line || line >= window_end || known_allocation_lines.contains(&line) {
                return;
            }
            if let Some(parent) = n.parent() {
                let is_assignment_target = parent.kind() == "assignment_expression" && parent.child_by_field_name("left").map(|l| l.id()) == Some(n.id());
                if is_assignment_target {
                    return; // reasignación, no un uso
                }
            }
            first_use = Some(line);
        });

        if let Some(line) = first_use {
            out.push(ModernizationCandidate {
                variable: variable.to_string(),
                pattern: "use_after_free",
                line,
                current: format!("`{variable}` usado en la línea {line}, liberado antes en la línea {} (`{}`), sin reasignación detectada entre medio", a.line, a.kind),
                suggested_target: "rust_ownership",
                reasoning: format!(
                    "`{variable}` sigue en uso después de liberarse — comportamiento indefinido en C/C++ (la memoria puede haber sido reutilizada por otra asignación mientras tanto). El modelo de ownership de Rust lo vuelve un error de compilación: usar un valor después de moverlo o liberarlo no compila."
                ),
                confidence: "medium",
            });
        }
    }
    out
}

/// Punto de entrada — recibe el `MemoryLayoutResult` que `memlayout.rs` ya
/// calculó para el mismo archivo, más el `root`/`source` que `cparse.rs` ya
/// tenía a mano (nunca vuelve a parsear el texto), necesarios solo para
/// `use_after_free_candidates`. Infalible: sin `allocations`, devuelve un
/// reporte vacío, no `None`.
pub fn analyze_c_cpp(memory: &MemoryLayoutResult, root: Node, source: &str) -> ModernizationReport {
    let released: HashSet<&str> = memory.allocations.iter().filter(|a| is_release(a.kind)).filter_map(|a| a.variable.as_deref()).collect();

    let mut candidates: Vec<ModernizationCandidate> = memory
        .allocations
        .iter()
        .filter(|a| is_acquire(a.kind))
        .filter_map(|a| a.variable.as_deref().map(|v| (a, v)))
        .map(|(a, var)| candidate_for(var, a.kind, a.line, released.contains(var)))
        .collect();
    candidates.extend(double_release_candidates(memory));
    candidates.extend(unsafe_realloc_candidates(memory));
    candidates.extend(use_after_free_candidates(memory, root, source));
    candidates.sort_by_key(|c| c.line);

    let manual_memory_raii = candidates.iter().filter(|c| c.pattern == "manual_memory_raii").count();
    let unmatched_allocation = candidates.iter().filter(|c| c.pattern == "unmatched_allocation").count();
    let double_release = candidates.iter().filter(|c| c.pattern == "double_release").count();
    let unsafe_realloc_reassignment = candidates.iter().filter(|c| c.pattern == "unsafe_realloc_reassignment").count();
    let use_after_free = candidates.iter().filter(|c| c.pattern == "use_after_free").count();

    ModernizationReport {
        summary: ModernizationSummary {
            total: candidates.len(),
            manual_memory_raii,
            unmatched_allocation,
            double_release,
            unsafe_realloc_reassignment,
            use_after_free,
        },
        candidates,
        note: NOTE.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memlayout::{self, AllocationSite, MemoryLayoutSummary};
    use tree_sitter::{Parser, Tree};

    fn fake_memory(allocations: Vec<AllocationSite>) -> MemoryLayoutResult {
        MemoryLayoutResult {
            variables: vec![],
            summary: MemoryLayoutSummary { stack: 0, heap_allocations: allocations.len(), data: 0, bss: 0 },
            allocations,
            note: String::new(),
        }
    }

    fn parse_c(src: &str) -> Tree {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_c::LANGUAGE.into()).unwrap();
        parser.parse(src, None).unwrap()
    }

    /// Wrapper para los tests que no ejercitan `use_after_free` — les da un
    /// AST vacío, así que esa parte del reporte siempre queda en 0 sin que
    /// cada test tenga que lidiar con parsear una fuente real.
    fn analyze(memory: &MemoryLayoutResult) -> ModernizationReport {
        let tree = parse_c("");
        analyze_c_cpp(memory, tree.root_node(), "")
    }

    #[test]
    fn malloc_con_free_emparejado_es_manual_memory_raii() {
        let memory = fake_memory(vec![
            AllocationSite { kind: "malloc", line: 2, variable: Some("p".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "free", line: 5, variable: Some("p".to_string()), reassigns_same_pointer: false },
        ]);
        let report = analyze(&memory);
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.candidates[0].pattern, "manual_memory_raii");
        assert_eq!(report.candidates[0].suggested_target, "raii_smart_pointer");
        assert_eq!(report.candidates[0].confidence, "medium");
    }

    #[test]
    fn malloc_sin_free_es_unmatched_allocation_alta_confianza() {
        let memory = fake_memory(vec![AllocationSite { kind: "malloc", line: 2, variable: Some("p".to_string()), reassigns_same_pointer: false }]);
        let report = analyze(&memory);
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.candidates[0].pattern, "unmatched_allocation");
        assert_eq!(report.candidates[0].suggested_target, "rust_ownership");
        assert_eq!(report.candidates[0].confidence, "high");
    }

    #[test]
    fn new_con_delete_es_manual_memory_raii_en_cpp() {
        let memory = fake_memory(vec![
            AllocationSite { kind: "new", line: 3, variable: Some("obj".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "delete", line: 9, variable: Some("obj".to_string()), reassigns_same_pointer: false },
        ]);
        let report = analyze(&memory);
        assert_eq!(report.candidates[0].pattern, "manual_memory_raii");
    }

    #[test]
    fn free_sin_malloc_correspondiente_no_genera_candidato() {
        // Un `free` suelto (variable llegó de otro lado, ej. un parámetro)
        // no es una adquisición — no hay nada que "modernizar" del lado de
        // la adquisición porque este heurístico nunca la vio.
        let memory = fake_memory(vec![AllocationSite { kind: "free", line: 4, variable: Some("p".to_string()), reassigns_same_pointer: false }]);
        let report = analyze(&memory);
        assert!(report.candidates.is_empty());
    }

    #[test]
    fn allocation_sin_variable_resuelta_se_ignora() {
        let memory = fake_memory(vec![AllocationSite { kind: "malloc", line: 2, variable: None, reassigns_same_pointer: false }]);
        let report = analyze(&memory);
        assert!(report.candidates.is_empty());
    }

    #[test]
    fn multiples_variables_producen_multiples_candidatos_ordenados_por_linea() {
        let memory = fake_memory(vec![
            AllocationSite { kind: "malloc", line: 10, variable: Some("b".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "malloc", line: 2, variable: Some("a".to_string()), reassigns_same_pointer: false },
        ]);
        let report = analyze(&memory);
        assert_eq!(report.candidates.len(), 2);
        assert_eq!(report.candidates[0].line, 2);
        assert_eq!(report.candidates[1].line, 10);
    }

    #[test]
    fn summary_cuenta_cada_patron_por_separado() {
        let memory = fake_memory(vec![
            AllocationSite { kind: "malloc", line: 2, variable: Some("a".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "free", line: 3, variable: Some("a".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "malloc", line: 5, variable: Some("b".to_string()), reassigns_same_pointer: false },
        ]);
        let report = analyze(&memory);
        assert_eq!(report.summary.total, 2);
        assert_eq!(report.summary.manual_memory_raii, 1);
        assert_eq!(report.summary.unmatched_allocation, 1);
    }

    #[test]
    fn sin_allocations_da_reporte_vacio() {
        let memory = fake_memory(vec![]);
        let report = analyze(&memory);
        assert!(report.candidates.is_empty());
        assert_eq!(report.summary.total, 0);
    }

    #[test]
    fn segunda_liberacion_de_la_misma_variable_es_double_release() {
        let memory = fake_memory(vec![
            AllocationSite { kind: "malloc", line: 2, variable: Some("p".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "free", line: 5, variable: Some("p".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "free", line: 8, variable: Some("p".to_string()), reassigns_same_pointer: false },
        ]);
        let report = analyze(&memory);
        let double_free = report.candidates.iter().find(|c| c.pattern == "double_release").unwrap();
        assert_eq!(double_free.line, 8); // apunta a la liberación repetida, no a la primera
        assert_eq!(double_free.confidence, "high");
        assert_eq!(double_free.suggested_target, "rust_ownership");
        assert_eq!(report.summary.double_release, 1);
    }

    #[test]
    fn una_sola_liberacion_no_es_double_release() {
        let memory = fake_memory(vec![
            AllocationSite { kind: "malloc", line: 2, variable: Some("p".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "free", line: 5, variable: Some("p".to_string()), reassigns_same_pointer: false },
        ]);
        let report = analyze(&memory);
        assert!(!report.candidates.iter().any(|c| c.pattern == "double_release"));
        assert_eq!(report.summary.double_release, 0);
    }

    #[test]
    fn tercera_liberacion_tambien_se_reporta_por_separado() {
        let memory = fake_memory(vec![
            AllocationSite { kind: "new", line: 1, variable: Some("obj".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "delete", line: 3, variable: Some("obj".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "delete", line: 6, variable: Some("obj".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "delete", line: 9, variable: Some("obj".to_string()), reassigns_same_pointer: false },
        ]);
        let report = analyze(&memory);
        assert_eq!(report.summary.double_release, 2); // la 2da y la 3ra liberación, cada una su propio candidato
    }

    #[test]
    fn realloc_reasignado_al_mismo_puntero_es_candidato_de_alta_confianza() {
        let memory = fake_memory(vec![AllocationSite { kind: "realloc", line: 4, variable: Some("p".to_string()), reassigns_same_pointer: true }]);
        let report = analyze(&memory);
        let unsafe_realloc = report.candidates.iter().find(|c| c.pattern == "unsafe_realloc_reassignment").unwrap();
        assert_eq!(unsafe_realloc.variable, "p");
        assert_eq!(unsafe_realloc.confidence, "high");
        assert_eq!(unsafe_realloc.suggested_target, "rust_ownership");
        assert_eq!(report.summary.unsafe_realloc_reassignment, 1);
    }

    #[test]
    fn realloc_a_variable_temporal_no_dispara_el_patron_unsafe() {
        let memory = fake_memory(vec![AllocationSite { kind: "realloc", line: 4, variable: Some("tmp".to_string()), reassigns_same_pointer: false }]);
        let report = analyze(&memory);
        assert!(!report.candidates.iter().any(|c| c.pattern == "unsafe_realloc_reassignment"));
        assert_eq!(report.summary.unsafe_realloc_reassignment, 0);
    }

    // `use_after_free` es el único patrón que recorre el AST directamente
    // (no solo reinterpreta `allocations`), así que sus tests parsean
    // fuente real en vez de armar un `AllocationSite` a mano.

    #[test]
    fn uso_despues_de_liberar_se_detecta() {
        let src = "void f() {\n    int *p = malloc(sizeof(int));\n    free(p);\n    use(p);\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src);
        let uaf = report.candidates.iter().find(|c| c.pattern == "use_after_free").unwrap();
        assert_eq!(uaf.variable, "p");
        assert_eq!(uaf.line, 4);
        assert_eq!(uaf.confidence, "medium");
        assert_eq!(uaf.suggested_target, "rust_ownership");
        assert_eq!(report.summary.use_after_free, 1);
    }

    #[test]
    fn reasignar_a_null_despues_de_liberar_no_dispara_use_after_free() {
        // `p = NULL;` es justo el idiom seguro que previene esto — no es un
        // uso, es una reasignación, así que no debe reportarse como bug.
        let src = "void f() {\n    int *p = malloc(sizeof(int));\n    free(p);\n    p = NULL;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src);
        assert!(!report.candidates.iter().any(|c| c.pattern == "use_after_free"));
        assert_eq!(report.summary.use_after_free, 0);
    }

    #[test]
    fn reasignar_a_memoria_nueva_cierra_la_ventana_de_use_after_free() {
        // Un `malloc` posterior sobre la misma variable vuelve el puntero
        // válido de nuevo — el uso después de ESE punto no es sospechoso.
        let src = "void f() {\n    int *p = malloc(sizeof(int));\n    free(p);\n    p = malloc(sizeof(int));\n    use(p);\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src);
        assert!(!report.candidates.iter().any(|c| c.pattern == "use_after_free"));
    }

    #[test]
    fn segunda_liberacion_no_se_duplica_como_use_after_free() {
        // La 2da liberación de `p` ya la cubre `double_release` — no debe
        // aparecer TAMBIÉN como `use_after_free` para la misma línea.
        let src = "void f() {\n    int *p = malloc(sizeof(int));\n    free(p);\n    free(p);\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src);
        assert!(!report.candidates.iter().any(|c| c.pattern == "use_after_free"));
        assert_eq!(report.summary.double_release, 1);
    }
}
