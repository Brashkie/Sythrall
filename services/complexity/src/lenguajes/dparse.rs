//! Parser de D — Fase 34, Línea C, **paso 1 del orden interno que fija el
//! ROADMAP**: `@trusted`/`@system`.
//!
//! **Por qué D entró a esta fase, y es lo que este módulo mide.** La Línea B
//! midió la superficie de confianza de Rust: dos niveles, `safe` y `unsafe`.
//! D gradúa la misma frontera en **tres**, y el del medio es el que importa:
//!
//!   - `@safe` — el compilador chequea.
//!   - `@trusted` — el compilador **deja de chequear porque el autor lo
//!     afirma**. Es código `@system` que se declara seguro de llamar desde
//!     `@safe`.
//!   - `@system` — sin chequeo y sin afirmación.
//!
//! **Y la diferencia con Rust no es de cantidad, es de dónde vive la
//! afirmación.** En Rust la superficie de revisión es una *convención de
//! comentario*: `// SAFETY:` no existe para el compilador, y enumerarla
//! significa leer comentarios. En D es una *palabra del lenguaje*: las
//! funciones a revisar se pueden enumerar exactamente, preguntándole al
//! árbol. Eso es lo que el ROADMAP llama "nombrar la superficie de revisión",
//! y es la dimensión que ni C++ ni Rust cubren.
//!
//! **Lo que hace falta para medirlo bien, y por qué un grep no alcanza.** El
//! nivel de confianza de una función puede venir de tres lugares, y la
//! gramática no los anida:
//!
//! ```d
//! @safe:                        // etiqueta: aplica al resto del alcance
//! int a() { }                   // @safe, sin decirlo
//! @trusted int b() { }          // atributo propio: gana sobre la etiqueta
//! @system { int c() { } }       // bloque: aplica adentro y nada más
//! int d() { }                   // vuelve a @safe
//! ```
//!
//! Medido contra la gramática: ese `@system { ... }` **no produce ningún nodo
//! contenedor**. El atributo, las funciones de adentro y las de después son
//! todos hermanos planos, y las llaves son tokens anónimos. Un recorrido de
//! nodos nombrados no puede distinguir `c` de `d`. Por eso
//! `resolver_alcance` recorre los hijos **crudos** en orden con una pila de
//! llaves, en vez de buscar atributos sueltos por el árbol.

use serde::Serialize;
use tree_sitter::{Node, Parser};

use crate::ast::tsnode::{self, text_of};
use crate::flujo::graph::{self, CallEdge};
use crate::hallazgos::smells::{self, StructuralSmell};

/// Los tres niveles, más el cuarto que no es un nivel sino su ausencia.
///
/// `sin_marcar` no es lo mismo que `@system`: en D clásico ese es el default
/// efectivo, pero el autor no afirmó nada, y la distinción es justamente lo
/// que este módulo existe para no borrar. Decir que una función sin atributo
/// "es `@system`" sería ponerle al autor una afirmación que no hizo.
pub const SIN_MARCAR: &str = "sin_marcar";

#[derive(Serialize, Clone)]
pub struct DFunction {
    pub name: String,
    pub line: usize,
    pub end_line: usize,
    pub loc: usize,
    pub loop_depth: u32,
    pub complexity: u32,
    pub big_o: String,
    pub big_o_reason: String,
    pub params: usize,
    pub calls: Vec<String>,
    /// `safe`, `trusted`, `system` o `sin_marcar`.
    pub confianza: String,
    /// `true` cuando el nivel **no está escrito en la función** sino heredado
    /// de una etiqueta `@safe:` o de un bloque `@attr { ... }`. Importa para
    /// revisar: una `@trusted` heredada de una etiqueta de arriba es mucho
    /// más fácil de pasar por alto leyendo que una que lo dice en su firma.
    pub heredada: bool,
    /// Sólo mirado en las `@trusted`: si hay un comentario pegado arriba.
    ///
    /// **No es el equivalente de `// SAFETY:`, y decir que lo es sería
    /// inventar una convención que D no tiene.** Rust tiene un marcador
    /// convencional que se puede buscar; D no estandarizó ninguno. Lo único
    /// comprobable es si el autor escribió *algo* al lado, así que eso es lo
    /// que se reporta y así se llama.
    pub comentario_adjunto: bool,
}

/// La superficie de confianza de D, el dual exacto de `rsparse::TrustSurface`.
#[derive(Serialize, Clone, Default)]
pub struct SuperficieConfianza {
    pub safe_fns: usize,
    /// **La superficie de revisión, y acá es enumerable.** Cada una es un
    /// lugar donde el compilador dejó de chequear porque alguien lo afirmó.
    pub trusted_fns: usize,
    pub system_fns: usize,
    /// Sin atributo propio ni heredado: el autor no afirmó nada.
    pub unmarked_fns: usize,
    /// De todas, cuántas tienen el nivel heredado en vez de escrito.
    pub inherited: usize,
    /// De las `@trusted`, cuántas tienen un comentario pegado arriba.
    pub trusted_with_comment: usize,
    /// Las `@trusted` con nombre y línea. Es el listado concreto para
    /// revisar, no un número: el punto entero de que el lenguaje la nombre.
    pub review_surface: Vec<PuntoDeRevision>,
}

#[derive(Serialize, Clone)]
pub struct PuntoDeRevision {
    pub name: String,
    pub line: usize,
    pub inherited: bool,
    pub has_comment: bool,
}

#[derive(Serialize, Clone)]
pub struct DImport {
    pub module: String,
    pub line: usize,
    #[serde(rename = "type")]
    pub tipo: &'static str,
}

#[derive(Serialize)]
pub struct DParseResult {
    pub functions: Vec<DFunction>,
    pub imports: Vec<DImport>,
    pub call_graph: Vec<CallEdge>,
    pub structural_smells: Vec<StructuralSmell>,
    pub trust_surface: SuperficieConfianza,
    /// Los bloques `unittest`. D los tiene en el lenguaje y son comunísimos;
    /// no son funciones con nombre, así que se cuentan aparte en vez de
    /// ensuciar las métricas de las que sí lo son.
    pub unittests: usize,
}

/// Primer hijo nombrado que cumple el predicado.
///
/// Por indice y no con un cursor: `named_children(&mut cursor)` ata el `Node`
/// devuelto a la vida del cursor local, asi que no se puede retornar.
fn hijo_nombrado<'a>(n: Node<'a>, pred: impl Fn(Node<'a>) -> bool) -> Option<Node<'a>> {
    (0..n.named_child_count()).filter_map(|i| n.named_child(i)).find(|c| pred(*c))
}

/// El nivel que nombra un `at_attribute`, si es uno de los tres.
fn nivel_de_atributo(at: Node, source: &str) -> Option<&'static str> {
    let mut cursor = at.walk();
    for hijo in at.named_children(&mut cursor) {
        if hijo.kind() == "identifier" {
            return match text_of(hijo, source) {
                "safe" => Some("safe"),
                "trusted" => Some("trusted"),
                "system" => Some("system"),
                _ => None,
            };
        }
    }
    None
}

/// El nivel que una `attribute_declaration` (`@safe:`) fija para el resto del
/// alcance.
fn nivel_de_etiqueta(decl: Node, source: &str) -> Option<&'static str> {
    hijo_nombrado(decl, |c| c.kind() == "at_attribute").and_then(|at| nivel_de_atributo(at, source))
}

/// El nivel escrito **en la función misma**, adelante o atrás de la firma.
///
/// D admite las dos: `@trusted int f()` y `int f() @trusted`. La segunda
/// llega envuelta en un `member_function_attribute`.
fn nivel_propio(func: Node, source: &str) -> Option<&'static str> {
    let mut cursor = func.walk();
    for hijo in func.named_children(&mut cursor) {
        let at = match hijo.kind() {
            "at_attribute" => Some(hijo),
            "member_function_attribute" => hijo_nombrado(hijo, |c| c.kind() == "at_attribute"),
            _ => None,
        };
        if let Some(n) = at.and_then(|a| nivel_de_atributo(a, source)) {
            return Some(n);
        }
    }
    None
}

fn nombre_de_funcion(func: Node, source: &str) -> String {
    hijo_nombrado(func, |c| c.kind() == "identifier")
        .map_or_else(|| "<anonymous>".to_string(), |n| text_of(n, source).to_string())
}

fn cuenta_parametros(func: Node) -> usize {
    hijo_nombrado(func, |c| c.kind() == "parameters").map_or(0, |p| {
            let mut c2 = p.walk();
            p.named_children(&mut c2).filter(|c| c.kind() == "parameter").count()
        })
}

/// Un comentario pegado arriba de la declaración, sin línea en blanco en el
/// medio. Mismo criterio que usa `rsparse` para el `// SAFETY:` de Rust.
fn tiene_comentario_arriba(func: Node, source: &str) -> bool {
    let Some(previo) = func.prev_sibling() else {
        return false;
    };
    if !previo.kind().contains("comment") {
        return false;
    }
    // Sin línea en blanco entre el comentario y la declaración.
    let entre = &source[previo.end_byte()..func.start_byte()];
    entre.matches('\n').count() <= 1
}

fn es_cuerpo_de_funcion(n: Node) -> bool {
    n.kind() == "function_declaration"
}

/// Profundidad de bucles anidados, sin contar los de funciones anidadas.
fn profundidad_bucles(node: Node) -> u32 {
    fn rec(n: Node, es_raiz: bool) -> u32 {
        if !es_raiz && es_cuerpo_de_funcion(n) {
            return 0;
        }
        let mut max_hijo = 0;
        let mut cursor = n.walk();
        for hijo in n.children(&mut cursor) {
            let d = rec(hijo, false);
            if d > max_hijo {
                max_hijo = d;
            }
        }
        if matches!(n.kind(), "for_statement" | "foreach_statement" | "while_statement" | "do_statement") {
            max_hijo + 1
        } else {
            max_hijo
        }
    }
    rec(node, true)
}

/// Complejidad ciclomática de D.
///
/// Los puntos de decisión son los mismos que cuenta el resto del motor:
/// `if`, cada bucle, cada `case` **con valor** (el `default` no es una
/// decisión, se distingue por tener un hijo `default`), el ternario, y los
/// operadores de cortocircuito — que acá son **nodos nombrados**
/// (`logical_and_expression`/`logical_or_expression`), a diferencia de Ada,
/// donde son tokens anónimos, y de Fortran, donde hay que preguntarle el kind
/// a los hijos.
fn ciclomatica(func: Node) -> u32 {
    fn walk(n: Node, cx: &mut u32) {
        match n.kind() {
            "if_statement"
            | "for_statement"
            | "foreach_statement"
            | "while_statement"
            | "do_statement"
            | "ternary_expression"
            | "logical_and_expression"
            | "logical_or_expression"
            | "catch_parameter"
            | "catch" => *cx += 1,
            "case_statement" => {
                let mut c = n.walk();
                if !n.children(&mut c).any(|h| h.kind() == "default") {
                    *cx += 1;
                }
            }
            _ => {}
        }
        let mut cursor = n.walk();
        for hijo in n.children(&mut cursor) {
            if es_cuerpo_de_funcion(hijo) {
                continue;
            }
            walk(hijo, cx);
        }
    }
    let mut cx = 1;
    let mut cursor = func.walk();
    for hijo in func.children(&mut cursor) {
        if es_cuerpo_de_funcion(hijo) {
            continue;
        }
        walk(hijo, &mut cx);
    }
    cx
}

fn llamadas(func: Node, source: &str) -> Vec<String> {
    let mut vistas = std::collections::HashSet::new();
    fn rec(n: Node, source: &str, vistas: &mut std::collections::HashSet<String>, es_raiz: bool) {
        if !es_raiz && es_cuerpo_de_funcion(n) {
            return;
        }
        if n.kind() == "call_expression" {
            if let Some(primero) = n.named_child(0) {
                if matches!(primero.kind(), "identifier" | "qualified_identifier") {
                    vistas.insert(text_of(primero, source).to_string());
                }
            }
        }
        let mut cursor = n.walk();
        for hijo in n.children(&mut cursor) {
            rec(hijo, source, vistas, false);
        }
    }
    rec(func, source, &mut vistas, true);
    let mut out: Vec<String> = vistas.into_iter().collect();
    out.sort();
    out
}

fn infer_big_o(depth: u32) -> (String, String) {
    let razon = match depth {
        0 => "sin bucles".to_string(),
        1 => "un bucle".to_string(),
        n => format!("{n} bucles anidados"),
    };
    (crate::costo::ccosto::etiqueta(depth), razon)
}

/// **El recorrido que las tres formas de atributo obligan a hacer.**
///
/// Va por los hijos **crudos** (nombrados y anónimos) en orden, porque las
/// llaves de un bloque `@attr { ... }` son tokens anónimos y la gramática no
/// anida nada adentro: el atributo, las funciones del bloque y las de después
/// son todos hermanos. Sin mirar las llaves no hay forma de saber dónde
/// termina el bloque.
fn resolver_alcance(
    nodo: Node,
    source: &str,
    ambiente: Option<&'static str>,
    heredado: bool,
    out: &mut Vec<DFunction>,
    unittests: &mut usize,
) {
    let mut actual = ambiente;
    let mut actual_heredado = heredado;
    // Cada llave abierta guarda el ambiente al que hay que volver.
    let mut pila: Vec<(Option<&'static str>, bool)> = Vec::new();
    // Un `at_attribute` suelto espera a su `{` (forma de bloque) o a la
    // declaración siguiente (forma de prefijo).
    let mut pendiente: Option<&'static str> = None;

    let mut cursor = nodo.walk();
    for hijo in nodo.children(&mut cursor) {
        match hijo.kind() {
            "attribute_declaration" => {
                if let Some(n) = nivel_de_etiqueta(hijo, source) {
                    actual = Some(n);
                    actual_heredado = true;
                }
            }
            "at_attribute" => pendiente = nivel_de_atributo(hijo, source),
            "{" => {
                pila.push((actual, actual_heredado));
                if let Some(p) = pendiente.take() {
                    actual = Some(p);
                    actual_heredado = true;
                }
            }
            "}" => {
                if let Some((prev, prev_h)) = pila.pop() {
                    actual = prev;
                    actual_heredado = prev_h;
                }
            }
            "function_declaration" => {
                let propio = nivel_propio(hijo, source);
                let (nivel, es_heredado) = match (propio, pendiente.take()) {
                    (Some(n), _) => (Some(n), false),
                    (None, Some(p)) => (Some(p), false),
                    (None, None) => (actual, actual_heredado),
                };
                registrar(hijo, source, nivel, es_heredado, out);
                // Una función anidada vive en el alcance de la de afuera.
                if let Some(cuerpo) = cuerpo_de(hijo) {
                    resolver_alcance(cuerpo, source, nivel, true, out, unittests);
                }
            }
            "unittest_declaration" => {
                *unittests += 1;
                pendiente = None;
            }
            "struct_declaration" | "class_declaration" | "union_declaration" | "interface_declaration"
            | "template_declaration" => {
                pendiente = None;
                let mut c2 = hijo.walk();
                for nieto in hijo.children(&mut c2) {
                    if matches!(nieto.kind(), "aggregate_body" | "declaration_block") {
                        resolver_alcance(nieto, source, actual, actual_heredado, out, unittests);
                    }
                }
            }
            _ => {
                if hijo.is_named() {
                    pendiente = None;
                }
            }
        }
    }
}

fn cuerpo_de(func: Node) -> Option<Node> {
    hijo_nombrado(func, |c| c.kind() == "function_body")
}

fn registrar(
    func: Node,
    source: &str,
    nivel: Option<&'static str>,
    heredada: bool,
    out: &mut Vec<DFunction>,
) {
    let confianza = nivel.unwrap_or(SIN_MARCAR);
    let start = func.start_position().row + 1;
    let end = func.end_position().row + 1;
    let depth = profundidad_bucles(func);
    let (big_o, big_o_reason) = infer_big_o(depth);
    out.push(DFunction {
        name: nombre_de_funcion(func, source),
        line: start,
        end_line: end,
        loc: end - start + 1,
        loop_depth: depth,
        complexity: ciclomatica(func),
        big_o,
        big_o_reason,
        params: cuenta_parametros(func),
        calls: llamadas(func, source),
        confianza: confianza.to_string(),
        heredada: nivel.is_some() && heredada,
        comentario_adjunto: confianza == "trusted" && tiene_comentario_arriba(func, source),
    });
}

fn superficie(funcs: &[DFunction]) -> SuperficieConfianza {
    let mut s = SuperficieConfianza::default();
    for f in funcs {
        match f.confianza.as_str() {
            "safe" => s.safe_fns += 1,
            "trusted" => {
                s.trusted_fns += 1;
                if f.comentario_adjunto {
                    s.trusted_with_comment += 1;
                }
                s.review_surface.push(PuntoDeRevision {
                    name: f.name.clone(),
                    line: f.line,
                    inherited: f.heredada,
                    has_comment: f.comentario_adjunto,
                });
            }
            "system" => s.system_fns += 1,
            _ => s.unmarked_fns += 1,
        }
        if f.heredada {
            s.inherited += 1;
        }
    }
    s.review_surface.sort_by_key(|p| p.line);
    s
}

fn extraer_imports(root: Node, source: &str) -> Vec<DImport> {
    let mut out = Vec::new();
    tsnode::walk_flat(root, &mut |n| {
        if n.kind() != "import_declaration" {
            return;
        }
        let linea = n.start_position().row + 1;
        let texto = text_of(n, source);
        let tipo = if texto.trim_start().starts_with("public") { "public_import" } else { "import" };
        tsnode::walk_flat(n, &mut |m| {
            if m.kind() == "module_fqn" {
                out.push(DImport { module: text_of(m, source).to_string(), line: linea, tipo });
            }
        });
    });
    out
}

fn smells_d(funcs: &[DFunction]) -> Vec<StructuralSmell> {
    let mut out = Vec::new();
    for f in funcs {
        out.extend(smells::check_long_function(&f.name, f.line, f.loc));
        out.extend(smells::check_excessive_parameters(&f.name, f.line, f.params));
        if f.loop_depth > smells::DEEP_NESTING_DEPTH {
            out.push(StructuralSmell {
                kind: "deep_nesting",
                name: f.name.clone(),
                line: f.line,
                message: format!(
                    "{} nested loops (> {}) — counted over loop statements only, not if/switch blocks, so the real nesting is at least this deep",
                    f.loop_depth,
                    smells::DEEP_NESTING_DEPTH
                ),
            });
        }
    }
    out.sort_by_key(|s| s.line);
    out
}

pub fn parse_d(source: &str) -> Option<DParseResult> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_d::LANGUAGE.into()).ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();

    let mut functions = Vec::new();
    let mut unittests = 0;
    // `module_def` envuelve todo cuando el archivo declara `module x;`; sin
    // esa declaración las declaraciones cuelgan directo de la raíz.
    let envoltorio = hijo_nombrado(root, |c| c.kind() == "module_def");
    let alcance = envoltorio.unwrap_or(root);
    resolver_alcance(alcance, source, None, false, &mut functions, &mut unittests);

    functions.sort_by_key(|f| f.line);
    let imports = extraer_imports(root, source);
    let pares: Vec<(&str, &[String])> =
        functions.iter().map(|f| (f.name.as_str(), f.calls.as_slice())).collect();
    let call_graph = graph::call_edges(&pares);
    let structural_smells = smells_d(&functions);
    let trust_surface = superficie(&functions);

    Some(DParseResult { functions, imports, call_graph, structural_smells, trust_surface, unittests })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_gramatica_parsea_d_real_sin_errores() {
        let src = "module m;\nimport std.stdio;\n@safe int f(int x) { return x; }\n";
        let mut p = Parser::new();
        p.set_language(&tree_sitter_d::LANGUAGE.into()).expect("ABI incompatible");
        assert!(!p.parse(src, None).unwrap().root_node().has_error());
    }

    #[test]
    fn los_tres_niveles_se_leen_del_atributo_propio() {
        let src = "@safe int a() { return 0; }\n@trusted int b() { return 0; }\n@system int c() { return 0; }\nint d() { return 0; }\n";
        let r = parse_d(src).unwrap();
        let niveles: Vec<(&str, &str)> =
            r.functions.iter().map(|f| (f.name.as_str(), f.confianza.as_str())).collect();
        assert_eq!(
            niveles,
            vec![("a", "safe"), ("b", "trusted"), ("c", "system"), ("d", SIN_MARCAR)]
        );
    }

    /// **`sin_marcar` no es `@system`, y confundirlos sería ponerle al autor
    /// una afirmación que no hizo.** En D clásico el default efectivo es
    /// `@system`, pero eso es una regla del compilador, no algo que alguien
    /// escribió.
    #[test]
    fn una_funcion_sin_atributo_no_se_reporta_como_system() {
        let r = parse_d("int f() { return 0; }\n").unwrap();
        assert_eq!(r.functions[0].confianza, SIN_MARCAR);
        assert_eq!(r.trust_surface.system_fns, 0);
        assert_eq!(r.trust_surface.unmarked_fns, 1);
    }

    /// D admite el atributo adelante y atrás de la firma.
    #[test]
    fn el_atributo_al_final_de_la_firma_tambien_cuenta() {
        let r = parse_d("int f(int x) pure nothrow @trusted { return x; }\n").unwrap();
        assert_eq!(r.functions[0].confianza, "trusted");
    }

    /// **La etiqueta `@safe:` aplica al resto del alcance**, y la gramática no
    /// anida nada: sin resolver el alcance, estas funciones saldrían todas
    /// como `sin_marcar`.
    #[test]
    fn la_etiqueta_se_hereda_y_el_atributo_propio_la_pisa() {
        let src = "@safe:\nint a() { return 0; }\n@trusted int b() { return 0; }\nint c() { return 0; }\n";
        let r = parse_d(src).unwrap();
        let v: Vec<(&str, &str, bool)> =
            r.functions.iter().map(|f| (f.name.as_str(), f.confianza.as_str(), f.heredada)).collect();
        assert_eq!(v, vec![("a", "safe", true), ("b", "trusted", false), ("c", "safe", true)]);
        assert_eq!(r.trust_surface.inherited, 2);
    }

    /// **El caso que obliga a mirar las llaves anónimas.** Un `@system { ... }`
    /// no produce ningún nodo contenedor: el atributo, lo de adentro y lo de
    /// después son hermanos planos. Si el bloque no se cerrara bien, `c`
    /// saldría `@system` siendo `@safe`.
    #[test]
    fn un_bloque_de_atributo_termina_donde_cierra_la_llave() {
        let src = "@safe:\nint a() { return 0; }\n@system\n{\n    int b() { return 0; }\n}\nint c() { return 0; }\n";
        let r = parse_d(src).unwrap();
        let v: Vec<(&str, &str)> =
            r.functions.iter().map(|f| (f.name.as_str(), f.confianza.as_str())).collect();
        assert_eq!(v, vec![("a", "safe"), ("b", "system"), ("c", "safe")], "el bloque no se cerro bien");
    }

    #[test]
    fn los_metodos_de_un_struct_heredan_el_alcance_del_archivo() {
        let src = "@safe:\nstruct S\n{\n    void a() { }\n    @trusted void b() { }\n}\n";
        let r = parse_d(src).unwrap();
        let v: Vec<(&str, &str)> =
            r.functions.iter().map(|f| (f.name.as_str(), f.confianza.as_str())).collect();
        assert_eq!(v, vec![("a", "safe"), ("b", "trusted")]);
    }

    /// **La superficie de revisión es una LISTA, no un número** — ése es el
    /// punto de que el lenguaje la nombre. En Rust habría que leer
    /// comentarios para armarla.
    #[test]
    fn la_superficie_de_revision_trae_nombre_y_linea() {
        let src = "@trusted int a() { return 0; }\n@safe int b() { return 0; }\n@trusted int c() { return 0; }\n";
        let r = parse_d(src).unwrap();
        assert_eq!(r.trust_surface.trusted_fns, 2);
        let nombres: Vec<&str> = r.trust_surface.review_surface.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(nombres, vec!["a", "c"]);
        assert_eq!(r.trust_surface.review_surface[0].line, 1);
    }

    /// **No es el `// SAFETY:` de Rust, y por eso no se llama así.** Rust
    /// tiene un marcador convencional buscable; D no estandarizó ninguno. Lo
    /// único comprobable es si hay un comentario pegado arriba.
    #[test]
    fn un_comentario_pegado_arriba_de_una_trusted_se_reporta_como_tal() {
        let con = "// el puntero viene del allocator de arriba, nunca es nulo\n@trusted int a(int* p) { return *p; }\n";
        let sin = "@trusted int a(int* p) { return *p; }\n";
        assert_eq!(parse_d(con).unwrap().trust_surface.trusted_with_comment, 1);
        assert_eq!(parse_d(sin).unwrap().trust_surface.trusted_with_comment, 0);
    }

    /// Una línea en blanco entre el comentario y la función rompe la
    /// adyacencia: ese comentario habla de otra cosa.
    #[test]
    fn un_comentario_despegado_no_cuenta() {
        let src = "// nota suelta sobre el modulo\n\n@trusted int a(int* p) { return *p; }\n";
        assert_eq!(parse_d(src).unwrap().trust_surface.trusted_with_comment, 0);
    }

    #[test]
    fn los_cortocircuitos_y_el_ternario_cuentan_como_decision() {
        let src = "int f(int a, int b) { return (a > 0 && b > 0) ? a : b; }\n";
        let r = parse_d(src).unwrap();
        assert_eq!(r.functions[0].complexity, 3, "1 + el && + el ternario");
    }

    #[test]
    fn el_case_default_no_cuenta_como_case() {
        let src = "int f(int n) { switch (n) { case 1: break; case 2: break; default: break; } return 0; }\n";
        let r = parse_d(src).unwrap();
        assert_eq!(r.functions[0].complexity, 3, "1 + 2 case con valor; default no suma");
    }

    #[test]
    fn dos_bucles_anidados_dan_o_n_cuadrado() {
        let src = "int f(int n) { int t = 0; for (int i = 0; i < n; i++) for (int j = 0; j < n; j++) t++; return t; }\n";
        let r = parse_d(src).unwrap();
        assert_eq!(r.functions[0].loop_depth, 2);
        assert_eq!(r.functions[0].big_o, "O(n²)");
    }

    #[test]
    fn el_foreach_es_un_bucle() {
        let src = "int f(int[] xs) { int t = 0; foreach (x; xs) t += x; return t; }\n";
        let r = parse_d(src).unwrap();
        assert_eq!(r.functions[0].loop_depth, 1);
        assert_eq!(r.functions[0].big_o, "O(n)");
    }

    #[test]
    fn los_imports_salen_con_su_modulo() {
        let src = "import std.stdio;\nimport std.algorithm.sorting;\n";
        let r = parse_d(src).unwrap();
        let m: Vec<&str> = r.imports.iter().map(|i| i.module.as_str()).collect();
        assert_eq!(m, vec!["std.stdio", "std.algorithm.sorting"]);
    }

    /// Los `unittest` de D no son funciones con nombre. Contarlos como tales
    /// ensuciaría las métricas de las que sí lo son.
    #[test]
    fn los_unittest_se_cuentan_aparte() {
        let src = "int f() { return 1; }\nunittest\n{\n    assert(f() == 1);\n}\n";
        let r = parse_d(src).unwrap();
        assert_eq!(r.functions.len(), 1);
        assert_eq!(r.unittests, 1);
    }

    #[test]
    fn el_grafo_de_llamadas_une_funciones_del_mismo_archivo() {
        let src = "int a() { return 1; }\nint b() { return a() + a(); }\nint c() { return externa(); }\n";
        let r = parse_d(src).unwrap();
        assert!(r.call_graph.iter().any(|e| e.from == "b" && e.to == "a"));
        assert!(!r.call_graph.iter().any(|e| e.to == "externa"), "externa no esta en el archivo");
    }

    #[test]
    fn un_fuente_que_no_es_d_no_revienta() {
        let r = parse_d("def f(x):\n    return x + 1\n").unwrap();
        assert!(r.functions.is_empty() || r.functions.iter().all(|f| f.name != "<anonymous>"));
    }


}
