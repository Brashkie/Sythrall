//! Punto de entrada de la Fase 1 de migración: arma el mismo shape que
//! `_parse_python()` en `static_parser.py` (functions/classes/imports/
//! call_graph/wasm_hints/summary) para un archivo Python, combinando
//! complexity.rs + bigo.rs + recursion.rs + classifiers.rs + structure.rs +
//! wasm.rs. `call_graph`/`wasm_hints` se agregaron después (agregación pura
//! sobre datos ya calculados acá — ver `build_call_graph`/`wasm::wasm_hints`
//! más abajo). `dead_code` (imports no usados) se sumó después
//! (`structure::unused_imports`) — Fase 18, Project Scanner: necesitaba que
//! Rust devolviera el shape *completo* por archivo en una sola llamada, sin
//! que Python tuviera que recibir el contenido de vuelta solo para recalcular
//! esta pieza. `static_parser.py::_parse_python` la sigue calculando en el
//! camino sin sidecar (`rich is None`) — ver su docstring. (`circular_deps`,
//! que vivía acá en versiones anteriores de este comentario, se eliminó del
//! todo — resultó ser código muerto, cero consumidores en el frontend.)

use rustpython_parser::ast::{Expr, Stmt};
use serde::Serialize;

use crate::costo::amortizado;
use crate::flujo::muertas;
use crate::flujo::ssa;
use crate::teoria::pruebapy;
use crate::teoria::automata;
use crate::costo::bigo;
use crate::hallazgos::classifiers;
use crate::costo::complexity::cyclomatic;
use crate::flujo::cfg;
use crate::teoria::colas;
use crate::hallazgos::cripto;
use crate::flujo::intervals;
use crate::flujo::graph::{self, CallEdge};
use crate::flujo::control::{self, ControlFinding};
use crate::teoria::proposicional;
use crate::hallazgos::pymodern::{self, PyModernizationReport};
use crate::hallazgos::datastructures;
use crate::hallazgos::naming::{self, NamingSmell};
use crate::teoria::numerico;
use crate::ast::parser::{parse_module, LineIndex};
use crate::flujo::recursion;
use crate::hallazgos::security::{self, SecurityFinding};
use crate::hallazgos::smells::{self, StructuralSmell};
use crate::costo::space;
use crate::ast::structure::{self, RichClass, RichImport};
use crate::ast::walk::walk_stmts;
use crate::hallazgos::wasm::{self, WasmHint};

#[derive(Serialize, Clone)]
pub struct RichFunction {
    pub name: String,
    pub line: usize,
    pub end_line: usize,
    pub loc: usize,
    pub args: Vec<String>,
    pub is_async: bool,
    pub decorators: Vec<String>,
    pub docstring: Option<String>,
    pub complexity: u32,
    pub big_o: String,
    pub big_o_reason: String,
    pub big_o_theta: String,
    pub big_o_omega: String,
    /// Fase 15 (Mathematical Intelligence): Combinatoria — ver `bigo::combinatorics_note`.
    pub combinatorics_note: Option<String>,
    pub space_complexity: String,
    pub space_reason: String,
    pub is_recursive: bool,
    pub is_tail_recursive: bool,
    pub recursion_note: Option<String>,
    /// Fase 15 (Mathematical Intelligence): Proof-by-induction framing —
    /// ver `recursion::induction_note`.
    pub induction_note: Option<String>,
    pub recurrence: Option<String>,
    /// Señal de recursión divide-and-conquer (a, c_own) — interna, se
    /// resuelve en el segundo pase de `analyze_rich` (`apply_master_theorem`)
    /// una vez que el Big-O de las funciones que esta llama ya se conoce, y
    /// no viaja al frontend.
    #[serde(skip)]
    dc_signal: Option<(u32, u32)>,
    pub regex_class: Option<String>,
    pub regex_note: Option<String>,
    pub grammar_class: Option<String>,
    pub grammar_note: Option<String>,
    pub graph_traversal: Option<String>,
    pub graph_traversal_note: Option<String>,
    /// Fase 16 (Formal Language Intelligence): patrón de análisis semántico
    /// — informalmente Type-1 (Context-Sensitive), ver
    /// `classifiers::semantic_analysis_note` para el hedge completo.
    pub semantic_analysis_class: Option<String>,
    pub semantic_analysis_note: Option<String>,
    pub data_structure: Option<String>,
    pub data_structure_note: Option<String>,
    /// Fase 26 (Algorithm Validation Engine), 2026-09-01 — primer detector
    /// de FORMA de algoritmo (no de complejidad): ver
    /// `classifiers::sorting_note` para el hedge completo. Deliberadamente
    /// NO conectado al auto-match kernel↔función del Static panel — esa
    /// sección solo renderiza para c/cpp/fortran/assembly, este campo solo
    /// existe para Python, así que las dos señales nunca coexisten en el
    /// mismo archivo (mismo callejón sin salida que `graph_traversal`).
    pub sorting_algorithm: Option<String>,
    pub sorting_algorithm_note: Option<String>,
    /// Segundo detector de FORMA (2026-09-01): ver `classifiers::search_note`.
    /// Mismo hedge y misma no-conexión al auto-match que `sorting_algorithm`.
    pub search_algorithm: Option<String>,
    pub search_algorithm_note: Option<String>,
    /// Tercer detector de FORMA (2026-09-01): ver
    /// `classifiers::recursion_shape_note`. Mismo hedge y misma no-conexión
    /// al auto-match que `sorting_algorithm`/`search_algorithm`. Desde
    /// 2026-09-02 distingue `naive_fibonacci` de `memoized_fibonacci` —
    /// ver `memoization` abajo, que es de dónde sale esa distinción.
    pub recursion_shape_algorithm: Option<String>,
    pub recursion_shape_algorithm_note: Option<String>,
    /// Cuarto detector de FORMA (Fase 32, 2026-09-24) — Shor y Grover. Campo
    /// propio y no `search_algorithm`: Grover sí es búsqueda, pero Shor
    /// factoriza, y meter los dos en un campo llamado "search" habría
    /// obligado a mentir en la mitad de los casos. Mismo hedge que los otros
    /// tres: reconoce la forma declarada, no verifica el circuito.
    pub quantum_algorithm: Option<String>,
    pub quantum_algorithm_note: Option<String>,
    /// `"decorator"` (`@lru_cache`/`@cache`) o `"manual_cache"` (dict
    /// consultado por pertenencia y escrito por subíndice) — 2026-09-02,
    /// nacido para cerrar la salvedad que el detector de Fibonacci
    /// documentaba, pero útil por sí solo en cualquier función cacheada.
    /// Ver `classifiers::memoization_note` para el hedge completo.
    pub memoization: Option<String>,
    pub memoization_note: Option<String>,
    pub calls: Vec<String>,
    pub returns_annotated: bool,
    /// `true` si el cuerpo contiene al menos un `return <valor>` en cualquier
    /// punto (incluyendo funciones anidadas — mismo alcance que `ast.walk`,
    /// no el scope-aware que usa taint tracking). Distinto de
    /// `returns_annotated` (que solo mira la anotación `-> T`, no si el
    /// cuerpo realmente retorna algo) — usado por `apps/api/routers/
    /// diagram.py` para colorear el nodo del flowchart.
    pub returns_value: bool,
    /// Fase 15 (Mathematical Intelligence): pure vs. side-effecting — ver
    /// `purity.rs` para el alcance exacto (intra-procedural, conservador).
    pub is_pure: bool,
    pub purity_note: String,
}

/// Puerto de `any(isinstance(x, ast.Return) and x.value for x in ast.walk(n))`
/// (`diagram.py::_py_flowchart`) — `walk_stmts` (no `walk_stmts_own_scope`)
/// porque `ast.walk` original desciende a funciones anidadas también.
fn has_return_with_value(body: &[Stmt]) -> bool {
    let mut found = false;
    let mut on_stmt = |stmt: &Stmt| {
        if let Stmt::Return(r) = stmt {
            if r.value.is_some() {
                found = true;
            }
        }
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    found
}

#[derive(Serialize, Clone)]
pub struct RichSummary {
    pub total_functions: usize,
    pub total_classes: usize,
    pub total_imports: usize,
    pub avg_complexity: f64,
    pub max_loc_function: usize,
}

#[derive(Serialize)]
pub struct RichAnalysisResult {
    pub functions: Vec<RichFunction>,
    pub classes: Vec<RichClass>,
    pub imports: Vec<RichImport>,
    pub dead_code: Vec<structure::DeadImport>,
    pub call_graph: Vec<CallEdge>,
    pub wasm_hints: Vec<WasmHint>,
    pub security_findings: Vec<SecurityFinding>,
    pub structural_smells: Vec<StructuralSmell>,
    pub naming_smells: Vec<NamingSmell>,
    /// Fase 15 (Mathematical Intelligence), 2026-09-02 — precondiciones de
    /// control optimo violadas de forma demostrable (dimensiones, simetria,
    /// diagonal no positiva). Ver `control.rs` para el limite exacto: reporta
    /// violaciones probadas, nunca certifica que las matrices esten bien.
    pub control_findings: Vec<ControlFinding>,
    /// Fase 25 (Modernization Intelligence), 2026-09-05 — APIs de Python
    /// deprecadas o removidas. Mismo shape de candidato que el motor de
    /// C/C++ de `modernization.rs`, a proposito: la agregacion de proyecto y
    /// el renderer del panel ya lo consumen por ese shape, asi que este
    /// motor no necesito plomeria propia. Ver `pymodern.rs` para el limite
    /// central (no conoce la version de Python objetivo del proyecto).
    pub modernization: PyModernizationReport,
    /// Fase 34, Linea D — donde el programa descarta una prueba que el
    /// sistema de tipos tenia. Ver `pruebapy.rs`.
    pub proof_surface: pruebapy::ProofSurfacePy,
    pub summary: RichSummary,
    pub error: Option<String>,
}

/// Agrega edges `{from, to}` entre funciones conocidas del mismo archivo, a
/// partir de `RichFunction::calls` (ya calculado por `structure::extract_calls`)
/// — puerto literal de `_build_call_graph` en `static_parser.py`. Pura
/// agregación, no un algoritmo nuevo: el dato de entrada (`calls`) ya existe.
fn build_call_graph(functions: &[RichFunction]) -> Vec<CallEdge> {
    let pares: Vec<(&str, &[String])> =
        functions.iter().map(|f| (f.name.as_str(), f.calls.as_slice())).collect();
    graph::call_edges(&pares)
}

pub fn analyze_rich(content: &str) -> RichAnalysisResult {
    let suite = match parse_module(content) {
        Ok(s) => s,
        Err(e) => {
            return RichAnalysisResult {
                proof_surface: pruebapy::ProofSurfacePy::default(),
                functions: Vec::new(),
                classes: Vec::new(),
                imports: Vec::new(),
                dead_code: Vec::new(),
                call_graph: Vec::new(),
                wasm_hints: Vec::new(),
                security_findings: Vec::new(),
                structural_smells: Vec::new(),
                naming_smells: Vec::new(),
                control_findings: Vec::new(),
                modernization: pymodern::empty_report(),
                summary: RichSummary {
                    total_functions: 0,
                    total_classes: 0,
                    total_imports: 0,
                    avg_complexity: 0.0,
                    max_loc_function: 0,
                },
                error: Some(e),
            };
        }
    };

    // Índice de saltos de línea construido UNA VEZ para todo el archivo y
    // compartido por TODO el resto del pipeline de abajo (naming/security/
    // structure/smells, más `build_function` unas líneas más abajo) — antes,
    // cada uno de esos módulos llamaba a su cuenta a `line_of_offset` suelta
    // (re-escaneo O(n) desde el offset 0 en cada llamada), y `security::
    // security_findings` en particular se llama una vez POR FUNCIÓN del
    // archivo — sin este índice compartido, el costo total era O(n²) en un
    // archivo con muchas funciones. `jsts.rs` ya tenía este mismo fix para
    // JS/TS desde antes; esto lo generaliza al pipeline de Python.
    let line_index = LineIndex::build(content);

    let mut functions = Vec::new();
    let mut security_findings = Vec::new();
    let mut structural_smells = Vec::new();
    // Fase 32 — nullabilidad: que funciones del archivo pueden devolver
    // `None`. Se calcula UNA vez sobre el modulo entero porque el caso que
    // importa cruza funciones, y baja hasta `intervals.rs` por la misma
    // ruta que ya lleva el `line_index`.
    let nullables = intervals::funciones_nullables(&suite, &line_index);
    collect_functions(&line_index, content, &suite, &nullables, &mut functions, &mut security_findings, &mut structural_smells);
    // Bug real preexistente encontrado durante Fase 27 (Jupyter Notebook
    // analysis, 2026-09-02): `collect_functions` de arriba SOLO llama a
    // `security::security_findings` una vez POR FUNCIÓN — cualquier
    // sentencia a nivel de MÓDULO (fuera de cualquier `def`) era invisible
    // para todo el catálogo de taint tracking (SQLi/command injection/SSRF/
    // etc.), sin importar qué tan obviamente tainted estuviera. Afecta a
    // cualquier `.py`, no solo notebooks — pero pega mucho más fuerte ahí,
    // donde código a nivel de tope es la norma, no la excepción. Arreglado
    // tratando el propio `suite` (la lista de statements de tope del
    // archivo) como un scope más: `security_findings` ya es genérica sobre
    // cualquier `&[Stmt]` y ya usa `walk_stmts_own_scope` internamente, así
    // que esta sola llamada extra no desciende a los cuerpos de las
    // funciones ya cubiertas arriba (sin duplicar hallazgos) y sí atrapa
    // taint que fluye directo en el cuerpo del módulo. `"<module>"` como
    // nombre de scope — misma convención que Python usa en sus propios
    // tracebacks (`File "x.py", line N, in <module>`), no un nombre
    // inventado.
    security_findings.extend(security::security_findings("<module>", &suite, &line_index));
    security_findings.extend(security::hardcoded_credentials(&line_index, &suite));
    // Fase 32 — teoria de numeros aplicada. Completa a `security.rs`, que
    // cubre QUE primitiva se eligio (CWE-327); esto cubre las que estan
    // bien elegidas y mal usadas: de donde salieron los bits, como se
    // comparan, y cuantos son.
    security_findings.extend(cripto::crypto_findings("<module>", &suite, &line_index));
    security_findings.sort_by_key(|f| f.line);
    apply_master_theorem(&mut functions);

    let classes = structure::extract_classes(&line_index, &suite);
    let imports = structure::extract_imports(&line_index, &suite);
    let dead_code = structure::unused_imports(&suite, &imports);

    for c in &classes {
        structural_smells.extend(smells::check_large_class(&c.name, c.line, c.methods.len(), c.loc));
        structural_smells.extend(smells::check_god_object(&c.name, c.line, c.methods.len(), c.attribute_count));
    }
    // Mismo bug de cobertura que `security_findings` de arriba, mismo
    // arreglo (2026-09-02, a pedido explícito del usuario de atacar este gap
    // análogo apenas se cerró el de seguridad): `check_deep_nesting`/
    // `check_quadratic_list_membership`/`check_de_morgan_simplifiable` ya
    // eran genéricas sobre cualquier `&[Stmt]` y ya usaban
    // `walk_stmts_own_scope` (o un descenso manual equivalente que tampoco
    // entra a `FunctionDef`/`ClassDef` — `check_deep_nesting`'s propio
    // `max_nesting_depth`) — nunca hacía falta una función real para
    // aplicarlas, solo nadie las había llamado sobre el `suite` completo
    // todavía. `check_long_function`/`check_excessive_parameters` quedan
    // deliberadamente afuera: ninguna de las 2 tiene un equivalente
    // conceptual a nivel de módulo (no hay un LOC de "la función módulo" ni
    // una lista de parámetros que medir).
    structural_smells.extend(smells::check_deep_nesting("<module>", 1, &suite, &line_index));
    structural_smells.extend(smells::check_quadratic_list_membership("<module>", &line_index, &suite));
    // Fase 32 — costo amortizado. Misma familia que
    // `quadratic_list_membership`, la otra mitad: aquella cubre la
    // BUSQUEDA lineal dentro de un bucle (`x in lista`), esta cubre la
    // MUTACION lineal (`lista.insert(0, x)`, `x = x + [...]`).
    structural_smells.extend(amortizado::check_linear_op_in_loop("<module>", &line_index, &suite));
    // Fase 34, Linea D — supresiones de tipo. Van a nivel de MODULO
    // porque un `# type: ignore` puede estar en cualquier linea, dentro
    // o fuera de una funcion.
    structural_smells.extend(pruebapy::proof_smells_py(content, &suite, &line_index));
    structural_smells.extend(smells::check_de_morgan_simplifiable("<module>", &line_index, &suite));
    // Fase 32 — logica proposicional. Va tambien a nivel de modulo por el
    // mismo motivo que de_morgan: una condicion contradictoria fuera de
    // toda funcion es igual de defectuosa.
    structural_smells.extend(proposicional::propositional_smells("<module>", &suite, &line_index, content));
    // Fase 32 — teoria de colas. A diferencia del resto, se llama UNA vez
    // sobre el modulo entero: necesita ver los decoradores, que no estan
    // en el cuerpo de la funcion sino en su definicion, asi que lleva su
    // propio recorrido con el nombre del scope.
    structural_smells.extend(colas::resource_smells(&suite, &line_index));
    // Fase 32 — calculo numerico. A nivel de modulo igual que de_morgan:
    // una comparacion de flotantes fuera de toda funcion es igual de fragil.
    structural_smells.extend(numerico::numeric_smells("<module>", &suite, &line_index));
    // Fase 32 — automatas. Lleva su propio recorrido: el alcance de una
    // clase es la clase entera (`__init__` escribe el estado, otro metodo
    // lo lee), asi que no encaja en el "una vez por funcion" del resto.
    structural_smells.extend(automata::state_machine_smells(&suite, &line_index));
    structural_smells.sort_by_key(|s| s.line);

    let mut naming_smells = naming::check_naming_smells(&line_index, &suite);
    naming_smells.sort_by_key(|s| s.line);

    // Mismo patron de doble alcance que security/smells: una pasada por cada
    // funcion y una sobre el modulo. En codigo de control el caso normal es
    // justamente el nivel de modulo (un script que define A/B/Q/R y llama al
    // solver), asi que sin la segunda llamada este analisis no veria nada en
    // el archivo tipico. `control_findings` usa `walk_stmts_own_scope`, asi
    // que la pasada de modulo no re-recorre los cuerpos ya cubiertos.
    let mut control_findings = Vec::new();
    for f in &suite {
        match f {
            Stmt::FunctionDef(func) => {
                control_findings.extend(control::control_findings(&func.name, &func.body, &line_index))
            }
            Stmt::AsyncFunctionDef(func) => {
                control_findings.extend(control::control_findings(&func.name, &func.body, &line_index))
            }
            _ => {}
        }
    }
    control_findings.extend(control::control_findings("<module>", &suite, &line_index));
    control_findings.sort_by_key(|f| f.line);

    let total_functions = functions.len();
    let avg_complexity = if total_functions == 0 {
        0.0
    } else {
        functions.iter().map(|f| f.complexity as f64).sum::<f64>() / total_functions as f64
    };
    let max_loc_function = functions.iter().map(|f| f.loc).max().unwrap_or(0);

    RichAnalysisResult {
        proof_surface: pruebapy::proof_surface_py(content, &suite),
        summary: RichSummary {
            total_functions,
            total_classes: classes.len(),
            total_imports: imports.len(),
            avg_complexity: (avg_complexity * 100.0).round() / 100.0,
            max_loc_function,
        },
        control_findings,
        modernization: pymodern::python_modernization(&suite, &line_index),
        call_graph: build_call_graph(&functions),
        wasm_hints: wasm::wasm_hints(&functions, content),
        functions,
        classes,
        imports,
        dead_code,
        security_findings,
        structural_smells,
        naming_smells,
        error: None,
    }
}

fn collect_functions(
    line_index: &LineIndex,
    source: &str,
    body: &[Stmt],
    nullables: &intervals::FuncionesNullables,
    out: &mut Vec<RichFunction>,
    security_out: &mut Vec<SecurityFinding>,
    smells_out: &mut Vec<StructuralSmell>,
) {
    for stmt in body {
        match stmt {
            Stmt::FunctionDef(f) => {
                security_out.extend(security::security_findings(&f.name, &f.body, line_index));
                security_out.extend(cripto::crypto_findings(&f.name, &f.body, line_index));
                let rf = build_function(line_index, &f.name, f.range, &f.body, &f.args, &f.decorator_list, f.returns.is_some(), false);
                smells_out.extend(function_smells(&rf, line_index, source, &f.body, &f.args, nullables));
                out.push(rf);
            }
            Stmt::AsyncFunctionDef(f) => {
                security_out.extend(security::security_findings(&f.name, &f.body, line_index));
                security_out.extend(cripto::crypto_findings(&f.name, &f.body, line_index));
                let rf = build_function(line_index, &f.name, f.range, &f.body, &f.args, &f.decorator_list, f.returns.is_some(), true);
                smells_out.extend(function_smells(&rf, line_index, source, &f.body, &f.args, nullables));
                out.push(rf);
            }
            Stmt::ClassDef(c) => collect_functions(line_index, source, &c.body, nullables, out, security_out, smells_out),
            _ => {}
        }
        // Funciones anidadas dentro de otros statements (if/for/with/try a
        // nivel de módulo, poco común pero `ast.walk` las encontraría) — se
        // cubre recorriendo también los cuerpos anidados no-función.
        for child in nested(stmt) {
            collect_functions(line_index, source, child, nullables, out, security_out, smells_out);
        }
    }
}

/// Smells a nivel de función (Fase 22): función larga, exceso de parámetros,
/// anidamiento profundo — los otros dos (large_class/god_object) se calculan
/// aparte, a nivel de clase, en `analyze_rich`. `quadratic_list_membership`
/// (Fase 14, "nested-structure interaction detection") se agregó después,
/// mismo criterio: heurística de forma de AST sobre el cuerpo de la función.
/// `de_morgan_simplifiable` (Fase 15, Mathematical Intelligence) reusa el
/// mismo mecanismo (`StructuralSmell`) para una nota de legibilidad en vez
/// de inventar una categoría/endpoint nuevos — cero superficie de UI nueva,
/// solo un `kind` más que el frontend ya sabe renderizar genéricamente.
fn function_smells(
    rf: &RichFunction,
    line_index: &LineIndex,
    source: &str,
    body: &[Stmt],
    args: &rustpython_parser::ast::Arguments,
    nullables: &intervals::FuncionesNullables,
) -> Vec<StructuralSmell> {
    let mut out = Vec::new();
    // `mutable_default_argument` (2026-09-02) es el único smell que mira la
    // FIRMA en vez del cuerpo, de ahí el parámetro `args` nuevo. Se le pasa
    // el nombre de la caché de memoización (si hay) para que pueda
    // distinguir el `def fib(n, memo={})` deliberado del accidente —
    // recalcular `memoization_info` acá cuesta un recorrido de AST más,
    // aceptado a cambio de que `smells.rs` no dependa de `classifiers.rs`
    // (queda función pura de sus argumentos, como el resto del archivo).
    let memo = classifiers::memoization_info(body, &rf.decorators);
    let memo_cache = (memo.kind == Some("manual_cache")).then_some(memo.detail.as_str());
    out.extend(smells::check_mutable_default_argument(&rf.name, rf.line, args, memo_cache));
    out.extend(smells::check_long_function(&rf.name, rf.line, rf.loc));
    out.extend(smells::check_excessive_parameters(&rf.name, rf.line, rf.args.len()));
    out.extend(smells::check_deep_nesting(&rf.name, rf.line, body, line_index));
    out.extend(smells::check_quadratic_list_membership(&rf.name, line_index, body));
    out.extend(amortizado::check_linear_op_in_loop(&rf.name, line_index, body));
    // Fase 32 — explosion combinatoria. Va por funcion, con la linea de la
    // funcion: `explosion_dominante` reporta la peor del cuerpo, no todas.
    if let Some(e) = crate::teoria::combinatoria::explosion_dominante(body) {
        out.push(crate::teoria::combinatoria::smell_de(&e, &rf.name, rf.line));
    }
    // Fase 32 — asignaciones muertas. Va POR FUNCION y no a nivel de
    // modulo: a nivel de modulo, una variable asignada y no leida puede
    // estar exportada y leerse desde otro archivo.
    out.extend(muertas::check_dead_stores(&rf.name, line_index, body));
    // Fase 32 - SSA. La particion con el de arriba es limpia: `dead_store`
    // cubre "el nombre no se lee nunca", `overwritten_store` cubre "se lee,
    // pero esta version no". Ver el doc de `ssa.rs`.
    out.extend(ssa::check_overwritten_stores(&rf.name, line_index, body, &rf.args));
    out.extend(smells::check_de_morgan_simplifiable(&rf.name, line_index, body));
    out.extend(proposicional::propositional_smells(&rf.name, body, line_index, source));
    out.extend(numerico::numeric_smells(&rf.name, body, line_index));
    // Fase 32 (2026-09-05) — el unico smell de esta lista que NO sale de un
    // heuristico sobre el texto ni sobre la forma del AST, sino de la
    // alcanzabilidad en el grafo de flujo. Ver `cfg.rs`.
    out.extend(cfg::unreachable_code_smells(&cfg::build(&rf.name, body, line_index)));
    // Fase 32 — interpretacion abstracta. Complementa al CFG: aquel encuentra
    // codigo inalcanzable por la ESTRUCTURA del grafo, este por los VALORES
    // (la rama existe, pero su guarda nunca puede ser verdadera).
    out.extend(intervals::interval_findings(&rf.name, body, line_index, Some(args), nullables));
    out
}

/// Segundo pase — Fase 13 (recurrence relations): resuelve el Master
/// Theorem para las funciones que `build_function` marcó con `dc_signal`
/// (recursión divide-and-conquer sobre n/2). Corre DESPUÉS de que todas las
/// funciones del archivo ya tienen su `big_o` resuelto, para poder mirar el
/// de las funciones que cada una llama (ej. `merge_sort` llamando a
/// `merge()`, ya analizada por su cuenta como O(n)) sin volver a recorrer
/// AST. El snapshot de grados se toma antes de mutar nada, así ninguna
/// función divide-and-conquer usa el resultado ya reescrito de otra.
fn apply_master_theorem(functions: &mut [RichFunction]) {
    let degrees: std::collections::HashMap<String, u32> = functions
        .iter()
        .filter_map(|f| bigo::degree_of(&f.big_o).map(|d| (f.name.clone(), d)))
        .collect();

    for f in functions.iter_mut() {
        let Some((a, c_own)) = f.dc_signal else { continue };
        let name = f.name.as_str();
        let callee_degrees: Vec<u32> = f.calls.iter().filter(|c| c.as_str() != name).filter_map(|c| degrees.get(c.as_str()).copied()).collect();
        let sig = bigo::DivideConquerSignal { a, c_own };
        let (recurrence, big_o, theta, omega, reason) = bigo::resolve_master_theorem(&sig, &callee_degrees);
        f.big_o = big_o;
        f.big_o_theta = theta;
        f.big_o_omega = omega;
        f.big_o_reason = reason;
        f.recurrence = Some(recurrence);
    }
}

fn nested(stmt: &Stmt) -> Vec<&[Stmt]> {
    match stmt {
        Stmt::If(s) => vec![&s.body, &s.orelse],
        Stmt::For(s) => vec![&s.body, &s.orelse],
        Stmt::AsyncFor(s) => vec![&s.body, &s.orelse],
        Stmt::While(s) => vec![&s.body, &s.orelse],
        Stmt::With(s) => vec![&s.body],
        Stmt::AsyncWith(s) => vec![&s.body],
        Stmt::Try(s) => vec![&s.body, &s.orelse, &s.finalbody],
        _ => vec![],
    }
}

#[allow(clippy::too_many_arguments)]
fn build_function(
    line_index: &LineIndex,
    name: &str,
    range: rustpython_parser::text_size::TextRange,
    body: &[Stmt],
    args: &rustpython_parser::ast::Arguments,
    decorator_list: &[rustpython_parser::ast::Expr],
    returns_annotated: bool,
    is_async: bool,
) -> RichFunction {
    let line = line_index.line_of(range.start().into());
    let end_line = line_index.line_of(range.end().into());
    let loc = end_line.saturating_sub(line) + 1;

    let recursion = recursion::analyze(name, body);
    let (bigo, dc_signal) = bigo::full(body, name, recursion.is_recursive);
    let space_info = space::infer(body, recursion.is_recursive);
    let regex = classifiers::regex_info(body);
    let grammar = classifiers::grammar_info(name, body, recursion.is_recursive);
    let graph = classifiers::graph_info(body, recursion.is_recursive);
    let cuantico = classifiers::quantum_algorithm_info(name, body);
    // Fase 32 — de Bruijn. Se parte en los DOS campos que ya se renderizan,
    // cada uno con el nombre que de verdad le corresponde: el grafo es una
    // estructura, el camino euleriano es un recorrido. Meter los dos en uno
    // solo habría obligado a llamar "recorrido" a una construcción.
    let debruijn = classifiers::debruijn_info(name, body);
    let debruijn_nota = classifiers::debruijn_note(&debruijn);
    let semantic = classifiers::semantic_analysis_info(body);
    let sorting = classifiers::sorting_info(body);
    let search = classifiers::search_info(body);
    let decorators: Vec<String> = decorator_list.iter().map(structure::decorator_name).collect();
    let memoization = classifiers::memoization_info(body, &decorators);
    let recursion_shape = classifiers::recursion_shape_info(name, body, memoization.kind.is_some());
    let heap = datastructures::heap_info(body);
    let arg_names = structure::arg_names(args);
    let purity = crate::flujo::purity::analyze(&arg_names, body);

    RichFunction {
        name: name.to_string(),
        line,
        end_line,
        loc,
        args: arg_names,
        is_async,
        decorators,
        docstring: structure::docstring_of(body),
        complexity: cyclomatic(body),
        big_o: bigo.big_o,
        big_o_reason: bigo.reason,
        big_o_theta: bigo.theta,
        big_o_omega: bigo.omega,
        combinatorics_note: bigo.combinatorics_note,
        space_complexity: space_info.space,
        space_reason: space_info.reason,
        is_recursive: recursion.is_recursive,
        is_tail_recursive: recursion.is_tail_recursive,
        recursion_note: recursion::note(&recursion),
        induction_note: recursion::induction_note(&recursion),
        recurrence: None,
        dc_signal: dc_signal.map(|s| (s.a, s.c_own)),
        regex_class: regex.uses_regex.then(|| "Type-3 (Regular)".to_string()),
        regex_note: classifiers::regex_note(&regex),
        grammar_class: grammar.is_grammar_shaped.then(|| "Type-2 (Context-Free)".to_string()),
        grammar_note: classifiers::grammar_note(&grammar),
        // El recorrido euleriano solo se pone si `graph_info` no detectó
        // nada: BFS/DFS ya detectados son más específicos sobre lo que esa
        // función hace, y pisarlos sería perder información.
        quantum_algorithm: cuantico.kind.map(|k| k.to_string()),
        quantum_algorithm_note: classifiers::quantum_algorithm_note(&cuantico),
        graph_traversal: graph.traversal_kind.clone().or_else(|| debruijn.walks_eulerian.then(|| "Eulerian Path".to_string())),
        graph_traversal_note: classifiers::graph_note(&graph).or_else(|| debruijn.walks_eulerian.then(|| debruijn_nota.clone()).flatten()),
        semantic_analysis_class: semantic.is_semantic_analysis_shaped.then(|| "Type-1 (informal)".to_string()),
        semantic_analysis_note: classifiers::semantic_analysis_note(&semantic),
        data_structure: heap.uses_heap.then(|| "Heap".to_string()).or_else(|| debruijn.builds_graph.then(|| "de Bruijn Graph".to_string())),
        data_structure_note: datastructures::heap_note(&heap).or_else(|| debruijn.builds_graph.then(|| debruijn_nota.clone()).flatten()),
        sorting_algorithm: sorting.algorithm.map(|s| s.to_string()),
        sorting_algorithm_note: classifiers::sorting_note(&sorting),
        search_algorithm: search.algorithm.map(|s| s.to_string()),
        search_algorithm_note: classifiers::search_note(&search),
        recursion_shape_algorithm: recursion_shape.algorithm.map(|s| s.to_string()),
        recursion_shape_algorithm_note: classifiers::recursion_shape_note(&recursion_shape),
        memoization: memoization.kind.map(|s| s.to_string()),
        memoization_note: classifiers::memoization_note(&memoization),
        calls: structure::extract_calls(body),
        returns_annotated,
        returns_value: has_return_with_value(body),
        is_pure: purity.is_pure,
        purity_note: purity.note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn func<'a>(result: &'a RichAnalysisResult, name: &str) -> &'a RichFunction {
        result
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("no se encontró la función {name}"))
    }

    #[test]
    fn bubble_sort_es_on2() {
        let src = "def bubble_sort(arr):\n    n = len(arr)\n    for i in range(n):\n        for j in range(n - i - 1):\n            if arr[j] > arr[j+1]:\n                arr[j], arr[j+1] = arr[j+1], arr[j]\n";
        let r = analyze_rich(src);
        assert_eq!(func(&r, "bubble_sort").big_o, "O(n\u{b2})");
    }

    #[test]
    fn binary_search_es_ologn() {
        let src = "def binary_search(arr, target):\n    lo, hi = 0, len(arr)\n    while lo < hi:\n        mid = (lo + hi) // 2\n        if arr[mid] == target:\n            return mid\n        elif arr[mid] < target:\n            lo = mid + 1\n        else:\n            hi = mid\n    return -1\n";
        let r = analyze_rich(src);
        assert_eq!(func(&r, "binary_search").big_o, "O(log n)");
    }

    #[test]
    fn returns_value_true_con_return_con_valor() {
        let src = "def f(x):\n    return x * 2\n";
        let r = analyze_rich(src);
        assert!(func(&r, "f").returns_value);
    }

    #[test]
    fn returns_value_false_sin_ningun_return() {
        let src = "def f(x):\n    print(x)\n";
        let r = analyze_rich(src);
        assert!(!func(&r, "f").returns_value);
    }

    #[test]
    fn returns_value_false_con_return_vacio() {
        let src = "def f(x):\n    if x:\n        return\n    print(x)\n";
        let r = analyze_rich(src);
        assert!(!func(&r, "f").returns_value);
    }

    #[test]
    fn returns_value_true_en_funcion_anidada_tambien() {
        // ast.walk (el que reemplaza) desciende a funciones anidadas —
        // mismo alcance acá, no el scope-aware de taint tracking.
        let src = "def outer(x):\n    def inner():\n        return 1\n    inner()\n";
        let r = analyze_rich(src);
        assert!(func(&r, "outer").returns_value);
    }

    #[test]
    fn merge_sort_reconoce_recurrencia_onlogn() {
        // merge() no es recursiva y tiene un loop propio (O(n)) — merge_sort
        // llama a merge() ya analizada, así que el segundo pase de
        // apply_master_theorem debe leer su grado (1) como c, no solo su
        // propia profundidad de loop (0, no tiene loop propio).
        let src = "def merge(left, right):\n    result = []\n    i = 0\n    for x in left:\n        result.append(x)\n    return result\n\ndef merge_sort(arr):\n    if len(arr) <= 1:\n        return arr\n    mid = len(arr) // 2\n    left = merge_sort(arr[:mid])\n    right = merge_sort(arr[mid:])\n    return merge(left, right)\n";
        let r = analyze_rich(src);
        let f = func(&r, "merge_sort");
        assert_eq!(f.big_o, "O(n log n)");
        assert_eq!(f.big_o_theta, "\u{398}(n log n)");
        assert_eq!(f.big_o_omega, "\u{398}(n log n)");
        assert_eq!(f.recurrence.as_deref(), Some("T(n) = 2T(n/2) + \u{398}(n)"));
    }

    #[test]
    fn binary_search_recursivo_es_ologn_no_on() {
        // Las dos auto-llamadas están en ramas elif/else mutuamente
        // excluyentes de un mismo if — el factor divide-and-conquer real es
        // a=1 (T(n) = T(n/2) + O(1)), no a=2 como daría contar sitios de
        // auto-llamada sin distinguir ramas alternativas.
        let src = "def binary_search_rec(arr, target, lo, hi):\n    if lo >= hi:\n        return -1\n    mid = (lo + hi) // 2\n    if arr[mid] == target:\n        return mid\n    elif arr[mid] < target:\n        return binary_search_rec(arr, target, mid + 1, hi)\n    else:\n        return binary_search_rec(arr, target, lo, mid)\n";
        let r = analyze_rich(src);
        let f = func(&r, "binary_search_rec");
        assert_eq!(f.big_o, "O(log n)");
        assert_eq!(f.recurrence.as_deref(), Some("T(n) = 1T(n/2) + \u{398}(1)"));
    }

    #[test]
    fn recursion_sin_binary_split_no_dispara_master_theorem() {
        // factorial es recursiva pero no divide el problema (n-1, no n//2)
        // — no debe activar el path de Master Theorem, `recurrence` queda
        // en None y el comportamiento heurístico existente no cambia.
        let src = "def factorial(n):\n    if n <= 1:\n        return 1\n    return n * factorial(n - 1)\n";
        let r = analyze_rich(src);
        assert!(func(&r, "factorial").recurrence.is_none());
    }

    #[test]
    fn constant_es_o1() {
        let src = "def constant(x):\n    return x * 2\n";
        let r = analyze_rich(src);
        assert_eq!(func(&r, "constant").big_o, "O(1)");
    }

    #[test]
    fn factorial_es_recursivo_no_tail() {
        let src = "def factorial(n):\n    if n <= 1:\n        return 1\n    return n * factorial(n - 1)\n";
        let r = analyze_rich(src);
        let f = func(&r, "factorial");
        assert!(f.is_recursive);
        assert!(!f.is_tail_recursive);
    }

    #[test]
    fn tail_call_detectado() {
        let src = "def countdown(n):\n    if n <= 0:\n        return 0\n    return countdown(n - 1)\n";
        let r = analyze_rich(src);
        let f = func(&r, "countdown");
        assert!(f.is_recursive);
        assert!(f.is_tail_recursive);
    }

    #[test]
    fn regex_detectado() {
        let src = "import re\ndef has_email(s):\n    return re.search(r\"x\", s) is not None\n";
        let r = analyze_rich(src);
        assert_eq!(func(&r, "has_email").regex_class.as_deref(), Some("Type-3 (Regular)"));
    }

    #[test]
    fn plain_function_sin_clasificadores() {
        let src = "def plain(x):\n    return x + 1\n";
        let r = analyze_rich(src);
        let f = func(&r, "plain");
        assert!(f.regex_class.is_none());
        assert!(f.grammar_class.is_none());
        assert!(f.graph_traversal.is_none());
    }

    #[test]
    fn funcion_pura_end_to_end() {
        let src = "def add(a, b):\n    return a + b\n";
        let r = analyze_rich(src);
        assert!(func(&r, "add").is_pure);
    }

    #[test]
    fn funcion_que_muta_parametro_end_to_end_no_es_pura() {
        let src = "def add_item(lst, x):\n    lst.append(x)\n";
        let r = analyze_rich(src);
        let f = func(&r, "add_item");
        assert!(!f.is_pure);
        assert!(f.purity_note.contains("lst"));
    }

    #[test]
    fn grammar_shaped_detectado() {
        let src = "def parse_expr(tokens, pos):\n    stack = []\n    stack.append(tokens[pos])\n    if pos < len(tokens):\n        return parse_expr(tokens, pos + 1)\n    return stack.pop()\n";
        let r = analyze_rich(src);
        assert_eq!(func(&r, "parse_expr").grammar_class.as_deref(), Some("Type-2 (Context-Free)"));
    }

    #[test]
    fn semantic_analysis_shaped_detectado_end_to_end() {
        let src = "def declare(names):\n    table = {}\n    for name in names:\n        if name in table:\n            raise ValueError('ya declarado')\n        table[name] = True\n";
        let r = analyze_rich(src);
        let f = func(&r, "declare");
        assert_eq!(f.semantic_analysis_class.as_deref(), Some("Type-1 (informal)"));
        assert!(f.semantic_analysis_note.as_ref().unwrap().contains("Resembles"));
    }

    #[test]
    fn memoizacion_end_to_end_no_dispara_semantic_analysis() {
        let src = "def fib(n, cache={}):\n    if n in cache:\n        return cache[n]\n    result = n if n < 2 else fib(n - 1) + fib(n - 2)\n    cache[n] = result\n    return result\n";
        let r = analyze_rich(src);
        assert!(func(&r, "fib").semantic_analysis_class.is_none());
    }

    #[test]
    fn grammar_requiere_nombre_no_solo_recursion() {
        let src = "def factorial(n):\n    if n <= 1:\n        return 1\n    return n * factorial(n - 1)\n";
        let r = analyze_rich(src);
        assert!(func(&r, "factorial").grammar_class.is_none());
    }

    #[test]
    fn bfs_detectado() {
        let src = "from collections import deque\ndef bfs(graph, start):\n    visited = set([start])\n    queue = deque([start])\n    while queue:\n        node = queue.popleft()\n        for n in graph[node]:\n            if n not in visited:\n                visited.add(n)\n                queue.append(n)\n    return visited\n";
        let r = analyze_rich(src);
        assert_eq!(func(&r, "bfs").graph_traversal.as_deref(), Some("BFS"));
    }

    #[test]
    fn topological_sort_detectado() {
        let src = "def topo(graph):\n    in_degree = {}\n    for node in graph:\n        in_degree[node] = 0\n    return in_degree\n";
        let r = analyze_rich(src);
        assert_eq!(func(&r, "topo").graph_traversal.as_deref(), Some("Topological Sort (Kahn's algorithm)"));
    }

    #[test]
    fn clase_con_metodo_async() {
        let src = "class Animal:\n    \"\"\"doc\"\"\"\n    def __init__(self, name):\n        self.name = name\n\n    async def speak(self):\n        return self.name\n";
        let r = analyze_rich(src);
        assert_eq!(r.classes.len(), 1);
        let c = &r.classes[0];
        assert_eq!(c.name, "Animal");
        assert_eq!(c.docstring.as_deref(), Some("doc"));
        assert_eq!(c.methods.len(), 2);
        assert!(c.methods.iter().any(|m| m.name == "speak" && m.is_async));
        // Los métodos también aparecen en la lista plana de funciones,
        // igual que `ast.walk` los encontraría en static_parser.py.
        assert!(func(&r, "speak").is_async);
    }

    #[test]
    fn clase_expone_nombres_de_atributos_no_solo_el_conteo() {
        let src = "class Animal:\n    def __init__(self, name):\n        self.name = name\n        self.age = 0\n    def birthday(self):\n        self.age += 1\n";
        let r = analyze_rich(src);
        let c = &r.classes[0];
        assert_eq!(c.attribute_count, 2);
        assert_eq!(c.attributes, vec!["name".to_string(), "age".to_string()]);
    }

    #[test]
    fn imports_extraidos() {
        let src = "import re\nfrom collections import deque\n";
        let r = analyze_rich(src);
        assert_eq!(r.imports.len(), 2);
        assert!(r.imports.iter().any(|i| i.module == "re" && i.kind == "import"));
        assert!(r.imports.iter().any(|i| i.module == "collections" && i.kind == "from_import"));
    }

    #[test]
    fn summary_cuenta_todo() {
        let src = "def a():\n    pass\ndef b():\n    pass\nclass C:\n    pass\nimport os\n";
        let r = analyze_rich(src);
        assert_eq!(r.summary.total_functions, 2);
        assert_eq!(r.summary.total_classes, 1);
        assert_eq!(r.summary.total_imports, 1);
    }

    // ── Call graph (agregación, Fase 18) ────────────────────────────────────

    #[test]
    fn call_graph_detecta_llamada_entre_funciones_conocidas() {
        let src = "def a():\n    return b()\ndef b():\n    return 1\n";
        let r = analyze_rich(src);
        assert_eq!(r.call_graph.len(), 1);
        assert_eq!(r.call_graph[0].from, "a");
        assert_eq!(r.call_graph[0].to, "b");
    }

    #[test]
    fn call_graph_ignora_llamada_a_funcion_no_definida_en_el_archivo() {
        let src = "def a():\n    return len([1, 2, 3])\n";
        let r = analyze_rich(src);
        assert!(r.call_graph.is_empty());
    }

    #[test]
    fn call_graph_ignora_auto_llamada_recursiva() {
        let src = "def factorial(n):\n    if n <= 1:\n        return 1\n    return n * factorial(n - 1)\n";
        let r = analyze_rich(src);
        assert!(r.call_graph.iter().all(|e| !(e.from == "factorial" && e.to == "factorial")));
    }

    #[test]
    fn call_graph_vacio_sin_funciones() {
        let r = analyze_rich("x = 1\n");
        assert!(r.call_graph.is_empty());
    }

    // ── Data Structure Intelligence (Fase 14) — end-to-end vía analyze_rich ──

    fn class_named<'a>(result: &'a RichAnalysisResult, name: &str) -> &'a RichClass {
        result
            .classes
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("no se encontró la clase {name}"))
    }

    #[test]
    fn heap_detectado_end_to_end() {
        let src = "def f(nums):\n    h = []\n    heapq.heappush(h, nums[0])\n    return heapq.heappop(h)\n";
        let r = analyze_rich(src);
        assert_eq!(func(&r, "f").data_structure.as_deref(), Some("Heap"));
    }

    #[test]
    fn trie_detectada_end_to_end_por_nombre() {
        let src = "class Trie:\n    def __init__(self):\n        self.children = {}\n";
        let r = analyze_rich(src);
        assert_eq!(class_named(&r, "Trie").data_structure.as_deref(), Some("Trie"));
    }

    #[test]
    fn fenwick_detectado_end_to_end_por_idiom() {
        let src = "class Tree:\n    def update(self, i, delta):\n        while i <= self.n:\n            self.arr[i] += delta\n            i += i & (-i)\n";
        let r = analyze_rich(src);
        assert_eq!(class_named(&r, "Tree").data_structure.as_deref(), Some("Fenwick Tree (BIT)"));
    }

    #[test]
    fn clase_sin_ninguna_senal_no_tiene_data_structure() {
        let src = "class Config:\n    def __init__(self):\n        self.debug = True\n";
        let r = analyze_rich(src);
        assert!(class_named(&r, "Config").data_structure.is_none());
    }

    #[test]
    fn avl_detectado_end_to_end_por_nombre() {
        let r = analyze_rich("class AVLTree:\n    def __init__(self):\n        pass\n");
        assert_eq!(class_named(&r, "AVLTree").data_structure.as_deref(), Some("AVL Tree"));
    }

    #[test]
    fn redblack_detectado_end_to_end_por_shape() {
        let src = "class Node:\n    def __init__(self):\n        self.left = None\n        self.right = None\n        self.color = 'red'\n    def rotate_right(self):\n        pass\n";
        let r = analyze_rich(src);
        assert_eq!(class_named(&r, "Node").data_structure.as_deref(), Some("Red-Black Tree"));
    }

    #[test]
    fn bloom_filter_detectado_end_to_end() {
        let src = "class BloomFilter:\n    def add(self, item):\n        pass\n    def contains(self, item):\n        pass\n";
        let r = analyze_rich(src);
        assert_eq!(class_named(&r, "BloomFilter").data_structure.as_deref(), Some("Bloom Filter"));
    }

    #[test]
    fn segment_tree_detectado_end_to_end() {
        let src = "class SegmentTree:\n    def build(self, arr):\n        pass\n    def update(self, i, val):\n        pass\n    def query(self, l, r):\n        pass\n";
        let r = analyze_rich(src);
        assert_eq!(class_named(&r, "SegmentTree").data_structure.as_deref(), Some("Segment Tree"));
    }

    #[test]
    fn btree_detectado_end_to_end() {
        let r = analyze_rich("class BTree:\n    def __init__(self):\n        pass\n");
        assert_eq!(class_named(&r, "BTree").data_structure.as_deref(), Some("B-Tree"));
    }

    #[test]
    fn skiplist_detectado_end_to_end() {
        let r = analyze_rich("class SkipList:\n    def __init__(self):\n        pass\n");
        assert_eq!(class_named(&r, "SkipList").data_structure.as_deref(), Some("Skip List"));
    }

    #[test]
    fn hashmap_detectado_end_to_end() {
        let src = "class HashMap:\n    def _hash(self, key):\n        pass\n    def put(self, key, value):\n        pass\n";
        let r = analyze_rich(src);
        assert_eq!(class_named(&r, "HashMap").data_structure.as_deref(), Some("HashMap"));
    }

    // ── Taint tracking a nivel de módulo (2026-09-02, encontrado durante la
    // Fase 27, Jupyter Notebook analysis) ───────────────────────────────────

    #[test]
    fn sqli_a_nivel_de_modulo_se_detecta() {
        // Antes de este fix, `collect_functions` solo llamaba a
        // `security::security_findings` por función -- código a nivel de
        // módulo (fuera de cualquier `def`, el estilo por defecto de un
        // notebook) era invisible para todo el catálogo de taint tracking.
        let src = "query = \"SELECT * FROM users WHERE name = '\" + input() + \"'\"\ncursor.execute(query)\n";
        let r = analyze_rich(src);
        assert!(r.security_findings.iter().any(|f| f.cwe == "CWE-89"));
    }

    #[test]
    fn hallazgo_de_modulo_usa_la_convencion_de_traceback_de_python() {
        let src = "query = \"SELECT * FROM users WHERE name = '\" + input() + \"'\"\ncursor.execute(query)\n";
        let r = analyze_rich(src);
        let finding = r.security_findings.iter().find(|f| f.cwe == "CWE-89").unwrap();
        assert_eq!(finding.function.as_deref(), Some("<module>"));
    }

    #[test]
    fn hallazgo_dentro_de_funcion_no_se_duplica_al_agregar_scope_de_modulo() {
        // El nuevo `security_findings("<module>", &suite, ...)` no debe
        // descender al cuerpo de `run_query` (ya cubierto por su propia
        // llamada en `collect_functions`) -- si lo hiciera, este finding
        // aparecería 2 veces.
        let src = "def run_query(request):\n    username = request.args.get('username')\n    query = \"SELECT * FROM users WHERE name = '\" + username + \"'\"\n    cursor.execute(query)\n";
        let r = analyze_rich(src);
        let matches: Vec<_> = r.security_findings.iter().filter(|f| f.cwe == "CWE-89").collect();
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn taint_de_modulo_no_se_filtra_a_una_funcion_ni_viceversa() {
        // Mismo criterio de aislamiento de scope que `scope_anidado_no_filtra_taint`
        // (security.rs) ya prueba entre función padre/anidada -- acá entre
        // módulo y función: una variable `query` tainted a nivel de módulo
        // no debe ensuciar una `query` DISTINTA (parametrizada, segura)
        // dentro de una función, y viceversa.
        let src = "query = \"SELECT * FROM t WHERE x = '\" + input() + \"'\"\ncursor.execute(query)\n\ndef safe():\n    query = \"SELECT * FROM t WHERE id = %s\"\n    cursor.execute(query, (1,))\n";
        let r = analyze_rich(src);
        let matches: Vec<_> = r.security_findings.iter().filter(|f| f.cwe == "CWE-89").collect();
        assert_eq!(matches.len(), 1, "solo el query de módulo debe disparar, el de safe() está parametrizado");
        assert_eq!(matches[0].function.as_deref(), Some("<module>"));
    }

    #[test]
    fn codigo_de_modulo_limpio_no_agrega_findings_falsos() {
        let src = "import os\nx = 1\nprint(x)\n";
        let r = analyze_rich(src);
        assert!(r.security_findings.is_empty());
        assert!(r.structural_smells.is_empty());
    }

    // ── Structural smells a nivel de módulo (mismo día, mismo gap que el
    // taint tracking de arriba — atacado apenas se cerró ese) ────────────────

    #[test]
    fn deep_nesting_a_nivel_de_modulo_se_detecta() {
        // DEEP_NESTING_DEPTH es 4 -- 5 niveles de `if` anidados sin estar
        // adentro de ninguna función debe disparar, algo que antes de este
        // fix era invisible (solo `function_smells` llamaba a
        // `check_deep_nesting`, una vez por función).
        let src = "if 1:\n    if 1:\n        if 1:\n            if 1:\n                if 1:\n                    pass\n";
        let r = analyze_rich(src);
        assert!(r.structural_smells.iter().any(|s| s.kind == "deep_nesting" && s.name == "<module>"));
    }

    #[test]
    fn quadratic_list_membership_a_nivel_de_modulo_se_detecta() {
        let src = "seen = []\nfor x in range(10):\n    if x in seen:\n        continue\n    seen.append(x)\n";
        let r = analyze_rich(src);
        assert!(r.structural_smells.iter().any(|s| s.kind == "quadratic_list_membership" && s.name == "<module>"));
    }

    #[test]
    fn de_morgan_a_nivel_de_modulo_se_detecta() {
        let src = "a = True\nb = False\nif not (a and b):\n    pass\n";
        let r = analyze_rich(src);
        assert!(r.structural_smells.iter().any(|s| s.kind == "de_morgan_simplifiable" && s.name == "<module>"));
    }

    #[test]
    fn smell_dentro_de_funcion_no_se_duplica_al_agregar_scope_de_modulo() {
        // El mismo patrón de nesting profundo, pero DENTRO de una función --
        // ya cubierto por `function_smells` en `collect_functions`. La nueva
        // llamada a nivel de módulo no debe descender al cuerpo de `f` y
        // volver a reportarlo.
        let src = "def f():\n    if 1:\n        if 1:\n            if 1:\n                if 1:\n                    if 1:\n                        pass\n";
        let r = analyze_rich(src);
        let matches: Vec<_> = r.structural_smells.iter().filter(|s| s.kind == "deep_nesting").collect();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "f");
    }
}
