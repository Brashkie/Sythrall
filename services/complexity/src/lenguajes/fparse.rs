//! Parser Fortran — Fase 20 (Scientific Intelligence). A diferencia de
//! `cparse.rs` (que porta un parser Python preexistente), este nace directo
//! en Rust: Sythrall no tenía ninguna infraestructura Fortran previa. Usa
//! `tree-sitter-fortran` sobre el mismo patrón de `cparse.rs` (walk
//! recursivo + `text_of` + heurística de texto para Big-O), y agrega 3
//! señales nuevas — DO-loops/candidatos a vectorización, reconocimiento de
//! algoritmos numéricos, uso de BLAS/LAPACK — con el mismo criterio de
//! `purity.rs`/Type-1 (Fase 16): un patrón de texto concreto y falseable, no
//! un análisis real de dependencias entre iteraciones. Probar que un
//! `A(I) = ...` dentro de un `DO I = ...` es realmente vectorizable
//! requeriría análisis de dependencias de datos entre iteraciones, que este
//! motor no hace para ningún lenguaje — se documenta como "candidato", no
//! como hecho probado.

use serde::Serialize;

use crate::flujo::graph::{self, CallEdge};
use crate::hallazgos::smells::{self, StructuralSmell};
use tree_sitter::{Node, Parser};

use crate::ast::tsnode::{self, text_of};

#[derive(Serialize, Clone)]
pub struct FortranFunction {
    pub name: String,
    pub kind: &'static str,
    pub line: usize,
    pub end_line: usize,
    pub loc: usize,
    pub do_loop_depth: u32,
    /// Complejidad ciclomatica. **Fortran era el unico de los seis
    /// lenguajes que no la emitia**, y el panel la lee sin chequear
    /// (`${f.complexity}`): la tabla de Big-O mostraba `undefined` en
    /// esa columna. Verificado en el navegador antes de arreglarlo.
    pub complexity: u32,
    pub big_o: String,
    pub big_o_reason: String,
    pub calls: Vec<String>,
    pub vectorization_note: Option<String>,
    pub numerical_algorithm_note: Option<String>,
    pub blas_lapack_calls: Vec<String>,
}

#[derive(Serialize, Clone)]
pub struct FortranUse {
    pub module: String,
    pub line: usize,
}

#[derive(Serialize)]
pub struct FortranParseResult {
    pub functions: Vec<FortranFunction>,
    pub imports: Vec<FortranUse>,
    pub call_graph: Vec<CallEdge>,
    /// Fase 22, 2026-09-05 — ver `structural_smells_fortran`.
    pub structural_smells: Vec<StructuralSmell>,
    /// Fase 32, 2026-09-25 — segunda porción de localidad espacial. Column
    /// major, o sea la regla OPUESTA a la de C; ver
    /// `locality::locality_findings_fortran`.
    pub locality: Vec<crate::hallazgos::locality::LocalityFinding>,
}

fn container_name(node: Node, source: &str) -> String {
    let stmt_kind = if node.kind() == "function" { "function_statement" } else { "subroutine_statement" };
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == stmt_kind {
            if let Some(name_node) = child.child_by_field_name("name") {
                return text_of(name_node, source).to_string();
            }
        }
    }
    "<anonymous>".to_string()
}

fn extract_calls(node: Node, source: &str) -> Vec<String> {
    let mut calls = std::collections::HashSet::new();
    tsnode::walk_flat(node, &mut |n| match n.kind() {
        "call_expression" => {
            if let Some(f) = n.child_by_field_name("function") {
                calls.insert(text_of(f, source).to_string());
            }
        }
        "subroutine_call" => {
            if let Some(s) = n.child_by_field_name("subroutine") {
                calls.insert(text_of(s, source).to_string());
            }
        }
        _ => {}
    });
    calls.into_iter().collect()
}

/// Rutinas BLAS/LAPACK conocidas, sin el prefijo de precisión (S/D/C/Z) —
/// lista representativa (Level 1-3 BLAS + factorizaciones/eigen-solvers de
/// LAPACK más comunes), no exhaustiva: el roadmap pide "detección de uso",
/// no un catálogo completo de las ~1500 rutinas de LAPACK.
const BLAS_LAPACK_STEMS: &[&str] = &[
    "AXPY", "COPY", "SCAL", "DOT", "NRM2", "ASUM", "SWAP", "ROT", "GEMV", "GER", "TRSV", "TRMV", "SYMV", "GBMV", "GEMM", "TRSM", "TRMM",
    "SYMM", "SYRK", "SYR2K", "GETRF", "GETRS", "GETRI", "POTRF", "POTRS", "POTRI", "GEQRF", "ORGQR", "SYEV", "SYEVD", "GEEV", "GESVD",
    "GESDD", "GELS", "SYSV", "GBSV", "PBSV",
];
const BLAS_LAPACK_PRECISION_PREFIXES: &[char] = &['S', 'D', 'C', 'Z'];

fn is_blas_lapack_name(name: &str) -> bool {
    let upper = name.to_uppercase();
    if BLAS_LAPACK_STEMS.contains(&upper.as_str()) {
        return true;
    }
    if let Some(first) = upper.chars().next() {
        if BLAS_LAPACK_PRECISION_PREFIXES.contains(&first) && BLAS_LAPACK_STEMS.contains(&&upper[1..]) {
            return true;
        }
    }
    false
}

/// Profundidad máxima de anidamiento de `do_loop` en cualquier punto del
/// subárbol — no solo loops directos del cuerpo, cualquier `do_loop`
/// anidado dentro de un `if`/`select` cuenta igual.
/// Smells estructurales de Fortran (Fase 22, 2026-09-05).
///
/// **Corrige una afirmación previa de este repo.** Cuando se auditaron las
/// ramas del scanner se dijo que el `[]` de Fortran era "honesto, no un
/// olvido, porque su parser de verdad no computa nada de esto". Eso era
/// circular: no computaba smells porque nadie los había escrito, exactamente
/// la misma excusa que valía para C/C++ una pasada antes. `loc` y
/// `do_loop_depth` ya se calculan en esta misma llamada, así que los dos
/// smells de acá son pura reinterpretación — cero AST nuevo.
///
/// `excessive_parameters` no está porque `FortranFunction` no guarda los
/// parámetros; eso sí haría falta parsear algo nuevo, y se deja dicho en vez
/// de omitirlo en silencio.
fn structural_smells_fortran(functions: &[FortranFunction]) -> Vec<StructuralSmell> {
    let mut out = Vec::new();
    for f in functions {
        out.extend(smells::check_long_function(&f.name, f.line, f.loc));
        // `do_loop_depth` cuenta SOLO bucles `DO`, no bloques `IF`/`SELECT`,
        // así que es una cota INFERIOR del anidamiento real. Usarlo con el
        // mismo umbral es conservador a propósito: cuando dispara hay 5+
        // bucles anidados de verdad, sin ambigüedad — y el mensaje dice qué
        // se contó, para que la evidencia coincida con la afirmación.
        if f.do_loop_depth > smells::DEEP_NESTING_DEPTH {
            out.push(StructuralSmell {
                kind: "deep_nesting",
                name: f.name.clone(),
                line: f.line,
                message: format!(
                    "{} nested DO loops (> {}) — counted over DO loops only, not IF/SELECT blocks, so the real nesting is at least this deep",
                    f.do_loop_depth,
                    smells::DEEP_NESTING_DEPTH
                ),
            });
        }
    }
    out.sort_by_key(|s| s.line);
    out
}

fn max_do_depth(node: Node) -> u32 {
    let mut max_child = 0;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let d = max_do_depth(child);
        if d > max_child {
            max_child = d;
        }
    }
    if node.kind() == "do_loop" {
        max_child + 1
    } else {
        max_child
    }
}

/// **Complejidad ciclomatica de Fortran, leida del arbol.**
///
/// Los puntos de decision son los mismos que cuenta el resto del motor,
/// traducidos a este lenguaje: `if_statement`, cada `elseif_clause` (que aca
/// SI es un nodo propio, a diferencia de Python donde un `elif` es un `If`
/// anidado), los `do_loop` —que cubren tanto `DO I = 1, N` como `DO WHILE`—,
/// cada `case_statement` con valor, y cada `.AND.`/`.OR.`.
///
/// **`.AND.` y `.OR.` son tokens ANONIMOS** de la gramatica, igual que el
/// `unsafe` de un `impl` en Rust: invisibles a un recorrido de nodos
/// nombrados. Hay que preguntarles el kind a los hijos directos del
/// `logical_expression`.
///
/// `CASE DEFAULT` no cuenta: es la rama que queda cuando ninguna se tomo. Se
/// distingue porque tiene un hijo `default`, no por el kind.
fn ciclomatica(func: Node) -> u32 {
    fn walk(n: Node, cx: &mut u32) {
        match n.kind() {
            "if_statement" | "elseif_clause" | "do_loop" => *cx += 1,
            "case_statement" => {
                let mut c = n.walk();
                let es_default = n.children(&mut c).any(|h| h.kind() == "default");
                if !es_default {
                    *cx += 1;
                }
            }
            "logical_expression" => {
                let mut c = n.walk();
                let ops = n.children(&mut c).filter(|h| matches!(h.kind(), ".and." | ".or.")).count();
                *cx += ops as u32;
            }
            _ => {}
        }
        let mut cursor = n.walk();
        for hijo in n.children(&mut cursor) {
            walk(hijo, cx);
        }
    }
    let mut cx = 1;
    let mut cursor = func.walk();
    for hijo in func.children(&mut cursor) {
        walk(hijo, &mut cx);
    }
    cx
}

/// Igual que `infer_big_o_c` (Fase 18) pero contando nodos `do_loop` en vez
/// de la palabra clave `for`/`while` en texto — acá sí tenemos el AST real
/// de tree-sitter-fortran a mano, no hace falta el heurístico de texto.
fn infer_big_o_fortran(depth: u32) -> (String, String) {
    match depth {
        0 => ("O(1)".to_string(), "sin loops DO".to_string()),
        1 => ("O(n)".to_string(), "un loop DO".to_string()),
        2 => ("O(n²)".to_string(), "loops DO anidados dobles".to_string()),
        _ => ("O(n³)".to_string(), "loops DO anidados triples o más".to_string()),
    }
}

/// Variables de control de cada `DO var = ...` dentro del subárbol, vía
/// texto (no navegación de campos de `loop_control_expression`, cuya forma
/// interna exacta no está documentada de forma estable en el node-types.json
/// de esta gramática) — mismo nivel de heurística de texto que
/// `infer_big_o_c`, aplicado acá a extraer un identificador en vez de contar
/// keywords.
/// Compiladas una vez, no en cada nodo del recorrido.
///
/// `do_loop_vars` corre por cada `do_loop` del archivo y `numerical_algorithm_note`
/// por cada función, y las tres regexes de acá tienen patrón CONSTANTE — no
/// había motivo para reconstruir el autómata cada vez. La cuarta (la que
/// arma `var_alt` a partir de los nombres de variable encontrados) sí depende
/// del archivo, así que se queda donde está: hoistearla necesitaría una caché
/// con clave, y no es el costo dominante.
static DO_LOOP_VAR_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"(?i)\bdo\s+(?:\d+\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=").expect("regex de DO-loop inválida")
});

static ASSIGN_2D_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"(?im)^\s*[A-Za-z_]\w*\s*\(\s*\w+\s*,\s*\w+\s*\)\s*=\s*(.+)$").expect("regex de asignación 2D inválida")
});

static REF_2D_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"[A-Za-z_]\w*\s*\(\s*\w+\s*,\s*\w+\s*\)").expect("regex de referencia 2D inválida")
});

fn do_loop_vars(node: Node, source: &str) -> Vec<String> {
    let text = text_of(node, source);
    DO_LOOP_VAR_RE.captures_iter(text).map(|c| c[1].to_string()).collect()
}

/// Fase 20, bullet 1 — candidato a vectorización/SIMD: un loop DO simple o
/// doblemente anidado donde el índice del loop aparece como subíndice del
/// lado izquierdo de una asignación a un array (`A(I) = ...`). No verifica
/// independencia real entre iteraciones (ej. `A(I) = A(I-1) + 1` matchea
/// igual, aunque tiene una dependencia secuencial obvia) — el texto de la
/// nota es explícito sobre esa limitación, no la esconde.
fn vectorization_note(node: Node, source: &str, depth: u32) -> Option<String> {
    if !(1..=2).contains(&depth) {
        return None;
    }
    let vars = do_loop_vars(node, source);
    if vars.is_empty() {
        return None;
    }
    let var_alt = vars.iter().map(|v| regex::escape(v)).collect::<Vec<_>>().join("|");
    let pat = format!(r"(?im)^\s*[A-Za-z_]\w*\s*\(\s*[^()\n]*\b({var_alt})\b[^()\n]*\)\s*=[^=]");
    let re = regex::Regex::new(&pat).ok()?;
    let text = text_of(node, source);
    if re.is_match(text) {
        let plural = if depth == 1 { "a DO loop" } else { "nested DO loops" };
        Some(format!(
            "{plural} with an array element assigned using the loop index as a subscript — shape resembles an elementwise array operation. \
             Vectorization/SIMD candidate IF the iterations are actually independent (no cross-iteration dependency, e.g. no A(i) reading \
             A(i-1) or A(i+1)) — that is not verified here, it would need real data-dependency analysis across iterations, which this engine \
             does not do for any language yet."
        ))
    } else {
        None
    }
}

/// Fase 20, bullet 2 — reconocimiento de algoritmos numéricos: forma de
/// multiplicación de matrices (triple loop DO + asignación a un array 2D
/// cuyo lado derecho multiplica otras dos referencias a arrays 2D). Señal de
/// forma, no verifica que los índices realmente formen `C(i,j) += A(i,k)*B(k,j)`.
fn numerical_algorithm_note(node: Node, source: &str, depth: u32) -> Option<String> {
    if depth < 3 {
        return None;
    }
    let text = text_of(node, source);
    let re_assign = &*ASSIGN_2D_RE;
    let re_2d_ref = &*REF_2D_RE;
    for caps in re_assign.captures_iter(text) {
        let rhs = &caps[1];
        if rhs.contains('*') && re_2d_ref.find_iter(rhs).count() >= 2 {
            return Some(
                "Triple-nested DO loop with an assignment to a doubly-subscripted array whose right-hand side multiplies two other \
                 doubly-subscripted array references — shape matches classic matrix multiplication, O(n³). Candidates: SIMD, loop \
                 blocking/tiling, parallelization (OpenMP/coarrays). Domain: HPC/Numerical Computing. This is a shape signal, not verified \
                 index-correctness — it does not confirm the indices actually form C(i,j) += A(i,k)*B(k,j)."
                    .to_string(),
            );
        }
    }
    None
}

fn extract_functions(root: Node, source: &str) -> Vec<FortranFunction> {
    let mut functions = Vec::new();
    tsnode::walk_flat(root, &mut |node| {
        // `is_named()` importa acá: la gramática de tree-sitter-fortran usa
        // el mismo string "function"/"subroutine" tanto para el nodo
        // contenedor (con sus `_statements` hijos) como para el token
        // anónimo de la palabra clave (`SUBROUTINE`/`END SUBROUTINE`) — sin
        // este chequeo, cada procedimiento se contaba 3 veces (el
        // contenedor real + 2 tokens de palabra clave sueltos).
        if node.is_named() && (node.kind() == "function" || node.kind() == "subroutine") {
            let kind: &'static str = if node.kind() == "function" { "function" } else { "subroutine" };
            let name = container_name(node, source);
            let start = node.start_position().row + 1;
            let end = node.end_position().row + 1;
            let loc = end - start + 1;
            let depth = max_do_depth(node);
            let (big_o, big_o_reason) = infer_big_o_fortran(depth);
            let calls = extract_calls(node, source);
            let blas_lapack_calls: Vec<String> = calls.iter().filter(|c| is_blas_lapack_name(c)).cloned().collect();

            functions.push(FortranFunction {
                name,
                kind,
                line: start,
                end_line: end,
                loc,
                do_loop_depth: depth,
                complexity: ciclomatica(node),
                big_o,
                big_o_reason,
                vectorization_note: vectorization_note(node, source, depth),
                numerical_algorithm_note: numerical_algorithm_note(node, source, depth),
                blas_lapack_calls,
                calls,
            });
        }
    });
    functions
}

fn extract_uses(root: Node, source: &str) -> Vec<FortranUse> {
    let mut uses = Vec::new();
    tsnode::walk_flat(root, &mut |node| {
        if node.kind() == "use_statement" {
            let mut cursor = node.walk();
            let module_node = node.children(&mut cursor).find(|c| c.kind() == "module_name");
            if let Some(m) = module_node {
                uses.push(FortranUse { module: text_of(m, source).to_string(), line: node.start_position().row + 1 });
            }
        }
    });
    uses
}

fn build_call_graph(functions: &[FortranFunction]) -> Vec<CallEdge> {
    let pares: Vec<(&str, &[String])> =
        functions.iter().map(|f| (f.name.as_str(), f.calls.as_slice())).collect();
    graph::call_edges(&pares)
}

pub fn parse_fortran(source: &str) -> Option<FortranParseResult> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_fortran::LANGUAGE.into()).ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();

    let functions = extract_functions(root, source);
    let imports = extract_uses(root, source);
    let call_graph = build_call_graph(&functions);

    let mut structural_smells = structural_smells_fortran(&functions);
    // Fase 32 — `implicit none`, literal de precision simple en variable
    // doble, y division entera asignada a un real (`tipos.rs`).
    structural_smells.extend(crate::hallazgos::tipos::tipos_fortran(root, source));
    structural_smells.sort_by_key(|s| s.line);
    let locality = crate::hallazgos::locality::locality_findings_fortran(root, source);
    Some(FortranParseResult { functions, imports, call_graph, structural_smells, locality })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Smells estructurales (Fase 22) ──

    /// **Fortran era el único de los seis lenguajes que no emitía
    /// `complexity`**, y el panel la lee sin chequear — la tabla de Big-O
    /// mostraba `undefined` en esa columna. Verificado en el navegador.
    #[test]
    fn una_subrutina_sin_decisiones_tiene_complejidad_1() {
        let src = "SUBROUTINE S(A, B)\n  REAL A, B\n  A = A + B\nEND SUBROUTINE S\n";
        let r = parse_fortran(src).unwrap();
        assert_eq!(r.functions[0].complexity, 1);
    }

    /// `.AND.` y `.OR.` son tokens ANÓNIMOS de la gramática: invisibles a un
    /// recorrido de nodos nombrados, igual que el `unsafe` de un `impl` en
    /// Rust. Sin preguntarles el kind a los hijos directos, no se cuentan.
    #[test]
    fn los_operadores_logicos_cuentan_aunque_el_token_sea_anonimo() {
        let src = "SUBROUTINE S(X)\n  REAL X\n  IF (X > 1.0 .AND. X < 9.0) THEN\n    X = 0.0\n  END IF\nEND SUBROUTINE S\n";
        let r = parse_fortran(src).unwrap();
        assert_eq!(r.functions[0].complexity, 3, "1 + el IF + el .AND.");
    }

    /// Un `ELSE IF` en Fortran SÍ es un nodo propio (`elseif_clause`), a
    /// diferencia de Python donde un `elif` es un `If` anidado. Y el `ELSE`
    /// pelado no es una decisión.
    #[test]
    fn cada_else_if_cuenta_y_el_else_pelado_no() {
        let src = "SUBROUTINE S(X)\n  REAL X\n  IF (X > 9.0) THEN\n    X = 1.0\n  ELSE IF (X > 5.0) THEN\n    X = 2.0\n  ELSE\n    X = 3.0\n  END IF\nEND SUBROUTINE S\n";
        let r = parse_fortran(src).unwrap();
        assert_eq!(r.functions[0].complexity, 3, "1 + IF + ELSE IF; el ELSE no suma");
    }

    /// `DO WHILE` es un `do_loop` como cualquier otro en esta gramática.
    #[test]
    fn el_do_while_cuenta_como_bucle() {
        let src = "SUBROUTINE S(N)\n  INTEGER N, K\n  K = N\n  DO WHILE (K > 0)\n    K = K - 1\n  END DO\nEND SUBROUTINE S\n";
        let r = parse_fortran(src).unwrap();
        assert_eq!(r.functions[0].complexity, 2);
        assert_eq!(r.functions[0].big_o, "O(n)");
    }

    /// `CASE DEFAULT` es un `case_statement` en la gramática pero NO es una
    /// decisión: es la rama que queda cuando ninguna se tomó. Se distingue
    /// por tener un hijo `default`, no por el kind — mismo tipo de detalle
    /// que el `switch_default` de JS y el `case` sin valor de C.
    #[test]
    fn el_case_default_no_cuenta_como_case() {
        let src = "SUBROUTINE S(K)\n  INTEGER K\n  SELECT CASE (K)\n    CASE (1)\n      K = 10\n    CASE (2)\n      K = 20\n    CASE DEFAULT\n      K = 0\n  END SELECT\nEND SUBROUTINE S\n";
        let r = parse_fortran(src).unwrap();
        assert_eq!(r.functions[0].complexity, 3, "1 + 2 CASE con valor; DEFAULT no suma");
    }

    #[test]
    fn una_subrutina_larga_se_marca() {
        let cuerpo = "  X = X + 1\n".repeat(60);
        let src = format!("SUBROUTINE LARGA(X)\n  REAL X\n{cuerpo}END SUBROUTINE LARGA\n");
        let r = parse_fortran(&src).expect("tree-sitter-fortran no disponible");
        assert!(r.structural_smells.iter().any(|s| s.kind == "long_function"), "{:?}", r.structural_smells.len());
    }

    #[test]
    fn cinco_do_anidados_se_marcan_como_anidamiento_profundo() {
        let src = "SUBROUTINE P(A)\n  REAL A\n  DO I = 1, 10\n    DO J = 1, 10\n      DO K = 1, 10\n        DO L = 1, 10\n          DO M = 1, 10\n            A = A + 1\n          END DO\n        END DO\n      END DO\n    END DO\n  END DO\nEND SUBROUTINE P\n";
        let r = parse_fortran(src).expect("tree-sitter-fortran no disponible");
        let hit = r.structural_smells.iter().find(|s| s.kind == "deep_nesting").expect("sin hallazgo");
        // El mensaje tiene que decir QUE se conto: `do_loop_depth` ignora los
        // bloques IF/SELECT, asi que es una cota inferior, no la profundidad
        // real. Afirmar mas de lo que se midio seria mentir.
        assert!(hit.message.contains("DO loops only"), "{}", hit.message);
    }

    #[test]
    fn dos_do_anidados_no_alcanzan_el_umbral() {
        let src = "SUBROUTINE P(A)\n  REAL A\n  DO I = 1, 10\n    DO J = 1, 10\n      A = A + 1\n    END DO\n  END DO\nEND SUBROUTINE P\n";
        let r = parse_fortran(src).expect("tree-sitter-fortran no disponible");
        // Se afirma lo que este test es: que dos `DO` anidados NO alcanzan el
        // umbral de anidamiento. Antes decia `is_empty()`, que afirmaba de
        // mas — el fixture tampoco declara `implicit none`, y desde la Fase
        // 32 eso es un hallazgo propio y correcto sobre este mismo codigo.
        assert!(!r.structural_smells.iter().any(|s| s.kind == "deep_nesting"), "{:?}", r.structural_smells.iter().map(|s| s.kind).collect::<Vec<_>>());
    }


    #[test]
    fn detecta_subrutina_simple_sin_loops() {
        let src = "SUBROUTINE ADD(A, B, C)\n  REAL :: A, B, C\n  C = A + B\nEND SUBROUTINE ADD\n";
        let r = parse_fortran(src).unwrap();
        assert_eq!(r.functions.len(), 1);
        assert_eq!(r.functions[0].name, "ADD");
        assert_eq!(r.functions[0].kind, "subroutine");
        assert_eq!(r.functions[0].big_o, "O(1)");
        assert_eq!(r.functions[0].do_loop_depth, 0);
    }

    #[test]
    fn detecta_funcion_con_un_loop_do() {
        let src = "FUNCTION SUMA(A, N) RESULT(S)\n  REAL :: A(N), S\n  INTEGER :: N, I\n  S = 0\n  DO I = 1, N\n    S = S + A(I)\n  END DO\nEND FUNCTION SUMA\n";
        let r = parse_fortran(src).unwrap();
        assert_eq!(r.functions[0].name, "SUMA");
        assert_eq!(r.functions[0].kind, "function");
        assert_eq!(r.functions[0].big_o, "O(n)");
        assert_eq!(r.functions[0].do_loop_depth, 1);
    }

    #[test]
    fn loops_do_anidados_dobles_dan_on2() {
        let src = "SUBROUTINE F(A, N)\n  REAL :: A(N,N)\n  INTEGER :: N, I, J\n  DO I = 1, N\n    DO J = 1, N\n      A(I,J) = 0\n    END DO\n  END DO\nEND SUBROUTINE F\n";
        let r = parse_fortran(src).unwrap();
        assert_eq!(r.functions[0].big_o, "O(n²)");
        assert_eq!(r.functions[0].do_loop_depth, 2);
    }

    #[test]
    fn vectorization_note_dispara_en_asignacion_elementwise() {
        let src = "SUBROUTINE SCALE(A, N)\n  REAL :: A(N)\n  INTEGER :: N, I\n  DO I = 1, N\n    A(I) = A(I) * 2.0\n  END DO\nEND SUBROUTINE SCALE\n";
        let r = parse_fortran(src).unwrap();
        assert!(r.functions[0].vectorization_note.is_some());
        assert!(r.functions[0].vectorization_note.as_ref().unwrap().contains("Vectorization"));
    }

    #[test]
    fn vectorization_note_ausente_sin_do_loop() {
        let src = "SUBROUTINE ADD(A, B, C)\n  REAL :: A, B, C\n  C = A + B\nEND SUBROUTINE ADD\n";
        let r = parse_fortran(src).unwrap();
        assert!(r.functions[0].vectorization_note.is_none());
    }

    #[test]
    fn numerical_algorithm_note_dispara_en_forma_de_multiplicacion_de_matrices() {
        let src = "SUBROUTINE MATMULT(A, B, C, N)\n  REAL :: A(N,N), B(N,N), C(N,N)\n  INTEGER :: N, I, J, K\n  DO I = 1, N\n    DO J = 1, N\n      DO K = 1, N\n        C(I,J) = C(I,J) + A(I,K) * B(K,J)\n      END DO\n    END DO\n  END DO\nEND SUBROUTINE MATMULT\n";
        let r = parse_fortran(src).unwrap();
        assert_eq!(r.functions[0].do_loop_depth, 3);
        assert_eq!(r.functions[0].big_o, "O(n³)");
        assert!(r.functions[0].numerical_algorithm_note.is_some());
        assert!(r.functions[0].numerical_algorithm_note.as_ref().unwrap().contains("matrix multiplication"));
    }

    #[test]
    fn numerical_algorithm_note_ausente_con_menos_de_3_loops() {
        let src = "SUBROUTINE F(A, N)\n  REAL :: A(N,N)\n  INTEGER :: N, I, J\n  DO I = 1, N\n    DO J = 1, N\n      A(I,J) = 0\n    END DO\n  END DO\nEND SUBROUTINE F\n";
        let r = parse_fortran(src).unwrap();
        assert!(r.functions[0].numerical_algorithm_note.is_none());
    }

    #[test]
    fn detecta_llamada_a_rutina_blas() {
        let src = "SUBROUTINE WRAP(A, B, C, N)\n  REAL :: A(N,N), B(N,N), C(N,N)\n  INTEGER :: N\n  CALL DGEMM('N', 'N', N, N, N, 1.0, A, N, B, N, 0.0, C, N)\nEND SUBROUTINE WRAP\n";
        let r = parse_fortran(src).unwrap();
        assert!(r.functions[0].blas_lapack_calls.iter().any(|c| c.eq_ignore_ascii_case("DGEMM")));
    }

    #[test]
    fn no_blas_lapack_para_llamada_generica() {
        let src = "SUBROUTINE WRAP(A)\n  REAL :: A\n  CALL MY_HELPER(A)\nEND SUBROUTINE WRAP\n";
        let r = parse_fortran(src).unwrap();
        assert!(r.functions[0].blas_lapack_calls.is_empty());
    }

    #[test]
    fn detecta_use_statement() {
        let src = "SUBROUTINE F()\n  USE MY_MODULE\nEND SUBROUTINE F\n";
        let r = parse_fortran(src).unwrap();
        assert!(r.imports.iter().any(|u| u.module.eq_ignore_ascii_case("MY_MODULE")));
    }

    #[test]
    fn call_graph_detecta_llamada_entre_subrutinas_conocidas() {
        let src = "SUBROUTINE B()\n  CONTINUE\nEND SUBROUTINE B\n\nSUBROUTINE A()\n  CALL B()\nEND SUBROUTINE A\n";
        let r = parse_fortran(src).unwrap();
        assert!(r.call_graph.iter().any(|e| e.from == "A" && e.to == "B"));
    }

    #[test]
    fn is_blas_lapack_name_reconoce_prefijos_de_precision() {
        assert!(is_blas_lapack_name("DGEMM"));
        assert!(is_blas_lapack_name("sgemm"));
        assert!(is_blas_lapack_name("ZPOTRF"));
        assert!(is_blas_lapack_name("GEMM"));
        assert!(!is_blas_lapack_name("PRINT_RESULT"));
    }
}
