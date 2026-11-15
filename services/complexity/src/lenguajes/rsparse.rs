//! Fase 34, Línea B — **Trust Surface**, primera porción: Rust como target.
//!
//! **Qué es Trust Surface y por qué no es un "security score".** Es dónde se
//! concentran las partes de un programa que necesitan garantías adicionales, y
//! **qué garantía deja de aplicar exactamente ahí**. Cada lenguaje aporta su
//! propio modelo: Rust marca la frontera con `unsafe`, D la gradúa en
//! `@safe`/`@trusted`/`@system`, C/C++ no la marca en absoluto. Comparar
//! conteos entre lenguajes sería incorrecto —no miden la misma población— así
//! que esto no produce ningún número comparable entre lenguajes.
//!
//! **Por qué Rust primero y no D**: es el mismo concepto con el corpus
//! incomparablemente más grande, y `tree-sitter-rust` es una gramática madura.
//! Hoy Sythrall solo podía dar la mitad *implícita* (C/C++: punteros crudos,
//! allocación manual), y esa mitad sola dice "todo es superficie de
//! confianza": cierto y poco accionable. El concepto ilumina recién cuando hay
//! al lado un lenguaje que **sí** marca la frontera.
//!
//! **Los dos hallazgos, y por qué son los correctos.** En Rust `unsafe` no
//! significa "inseguro": significa "acá el compilador deja de verificar y
//! empieza una promesa humana". Lo que importa entonces no es cuántos hay,
//! sino **si esa promesa está escrita**:
//!
//!   - **Un bloque `unsafe` sin comentario `// SAFETY:`** es una promesa que
//!     nadie escribió. Quien lo lea después no tiene forma de saber qué
//!     invariante hay que preservar para no romperlo.
//!   - **Una `unsafe fn` sin sección `# Safety` en su doc** traslada la
//!     responsabilidad al llamador **sin decirle qué tiene que cumplir**. Es
//!     peor que el caso anterior: el que tiene que honrar el contrato ni
//!     siquiera está mirando este archivo.
//!
//! **Esto no es redundante con clippy, y conviene decirlo.** Clippy tiene
//! `undocumented_unsafe_blocks` y `missing_safety_doc`, pero la primera está
//! en el grupo `restriction` (apagada por defecto) y casi ningún proyecto la
//! enciende. Que la herramienta exista no es lo mismo que que esté corriendo.
//!
//! **Dos trampas de gramática, las dos del mismo tipo que ya costó una vez.**
//! En `impl_item` el `unsafe` de un `unsafe impl` es un token **anónimo**: un
//! recorrido sobre nodos nombrados no lo ve nunca. En `function_item`, en
//! cambio, vive dentro de un `function_modifiers` que **sí** es nombrado. Los
//! dos casos se volcaron de la gramática real antes de escribir una línea de
//! detector.

use serde::Serialize;
use tree_sitter::{Node, Parser};

use crate::flujo::graph::CallEdge;
use crate::hallazgos::smells::StructuralSmell;
use crate::ast::tsnode::{self, text_of};

#[derive(Serialize, Clone)]
pub struct RustFunction {
    pub name: String,
    pub line: usize,
    pub end_line: usize,
    pub loc: usize,
    pub complexity: u32,
    pub big_o: String,
    pub big_o_reason: String,
    /// `true` si la firma lleva `unsafe`.
    pub is_unsafe: bool,
    /// Fase 34 (2026-09-26) — nombres llamados desde el cuerpo. Medido antes
    /// de agregarlo: **Rust era el único de los 8 targets sin grafo de
    /// llamadas**, ni siquiera con la clave presente. El mismo programa en
    /// Python daba tres aristas y en Rust no daba ninguna.
    pub calls: Vec<String>,
}

#[derive(Serialize, Clone)]
pub struct RustUse {
    pub module: String,
    /// El mismo campo `type` que emiten Python/JS/TS. El panel lo lee sin
    /// chequear —`imp.type.replace(...)`— asi que omitirlo no daba un hueco
    /// silencioso: rompia el render entero del archivo. Es la version
    /// visible del bug de la clave caida, y la encontro el navegador, no
    /// un test: el contrato de `Nucleo` cubre las claves del NUCLEO, no la
    /// forma de los elementos de adentro de cada lista.
    #[serde(rename = "type")]
    pub clase: &'static str,
    pub line: usize,
}

/// **El mapa de la superficie de confianza de un archivo.** No es un score:
/// es dónde deja de aplicar la garantía del compilador, y cuánto de eso está
/// justificado por escrito.
#[derive(Serialize, Clone, Default)]
pub struct TrustSurface {
    /// Bloques `unsafe { ... }`.
    pub unsafe_blocks: usize,
    /// De esos, cuántos llevan un `// SAFETY:` delante.
    pub documented_blocks: usize,
    /// Funciones con `unsafe` en la firma — el contrato se traslada al
    /// llamador.
    pub unsafe_fns: usize,
    /// De esas, cuántas tienen una sección `# Safety` en su doc.
    pub documented_fns: usize,
    /// `unsafe impl` — afirma a mano una propiedad que el compilador no puede
    /// verificar (típicamente `Send`/`Sync`).
    pub unsafe_impls: usize,
}

/// **La superficie de PRUEBA: dónde el programa tenía una demostración y la
/// tiró.** Es el dual exacto de `TrustSurface`, y las dos juntas cubren los
/// dos lados de la misma frontera.
///
/// La superficie de confianza mide dónde el compilador **deja de chequear**
/// (`unsafe`). Ésta mide lo contrario: lugares donde el compilador **sí
/// sabía algo** —que esta operación puede fallar, que este número puede no
/// entrar— y el programa descartó esa información a propósito.
///
/// Viene de Curry-Howard: un tipo es una proposición y un valor de ese tipo
/// es su demostración. `Result<T, E>` es la proposición "esto puede fallar";
/// `.unwrap()` la descarta sin dar motivo. **Y tiene su forma justificada,
/// igual que `// SAFETY:` justifica un `unsafe`: `.expect("por qué no puede
/// fallar")` descarta la misma prueba, pero deja escrito el razonamiento.**
/// Esa simetría no es una analogía forzada: es el mismo eje de *revisado*
/// contra *sin revisar*.
#[derive(Serialize, Clone, Default, Debug)]
pub struct ProofSurface {
    /// `.unwrap()` — prueba descartada sin razón escrita.
    pub unwrap_calls: usize,
    /// `.expect("…")` — la misma prueba descartada, con el motivo escrito.
    pub expect_calls: usize,
    /// `x as T`. En Rust `as` es la conversión **sin chequear**: trunca o
    /// envuelve en silencio. La forma que conserva la prueba es `try_into()`,
    /// que devuelve un `Result`. No todo `as` está mal —convertir a un tipo
    /// más ancho no pierde nada— pero todos son la variante sin verificar.
    pub unchecked_casts: usize,
    /// Funciones que devuelven `Result`: las que tienen `?` disponible.
    pub fallible_fns: usize,
    /// `.unwrap()` adentro de una de ésas — ver `unwrap_in_fallible_fn`.
    pub unwraps_in_fallible_fns: usize,
}


/// Los nombres llamados desde el cuerpo de una función de Rust.
///
/// **Rust tiene más formas de llamar que C, y cada una es un nodo distinto
/// en la gramática:**
///
/// - `f(x)` — `call_expression` con `identifier`.
/// - `modulo::f(x)` — `call_expression` con `scoped_identifier`; interesa la
///   última parte, que es el nombre de la función.
/// - `x.metodo()` — `call_expression` con `field_expression`. Se recoge el
///   nombre del método aunque casi nunca produzca arista: resolverlo a una
///   definición exige tipos, y sin ellos no se afirma a qué apunta. Que
///   aparezca en `calls` sirve igual, y es lo mismo que hacen los otros
///   lenguajes con sus llamadas a método.
///
/// **Las macros quedan afuera a propósito.** `println!(…)` es un
/// `macro_invocation`, no un `call_expression`, y tratarla como llamada
/// metería en el grafo nodos que no son funciones del archivo. Es la misma
/// decisión que toma `cparse.rs` al no contar los `#define` como llamadas.
fn extract_calls_rust(func: Node, source: &str) -> Vec<String> {
    let mut calls = std::collections::HashSet::new();
    tsnode::walk_flat(func, &mut |n| {
        if n.kind() != "call_expression" {
            return;
        }
        let Some(f) = n.child_by_field_name("function") else { return };
        let nombre = match f.kind() {
            "identifier" => Some(text_of(f, source).to_string()),
            // `a::b::f` — la última parte es la función.
            "scoped_identifier" => f.child_by_field_name("name").map(|x| text_of(x, source).to_string()),
            // `x.metodo()` — el campo es el nombre del método.
            "field_expression" => f.child_by_field_name("field").map(|x| text_of(x, source).to_string()),
            _ => None,
        };
        if let Some(n) = nombre {
            calls.insert(n);
        }
    });
    let mut out: Vec<String> = calls.into_iter().collect();
    out.sort();
    out
}

fn build_call_graph_rust(functions: &[RustFunction]) -> Vec<CallEdge> {
    let pares: Vec<(&str, &[String])> = functions.iter().map(|f| (f.name.as_str(), f.calls.as_slice())).collect();
    crate::flujo::graph::call_edges(&pares)
}


/// Un tipo de Rust: `struct`, `enum`, `trait` o `union`.
///
/// **Los nombres de los campos son los que ya usa el panel** (`name`,
/// `kind`, `line`, `bases`, `methods`), así que esto se renderiza sin tocar
/// el frontend. `bases` lleva los traits IMPLEMENTADOS para el tipo (o los
/// supertraits, si es un trait), que es lo más cercano a un "extends" que
/// Rust tiene.
#[derive(Serialize, Clone)]
pub struct RustType {
    pub name: String,
    /// `"struct"`, `"enum"`, `"trait"` o `"union"`.
    pub kind: &'static str,
    pub line: usize,
    pub end_line: usize,
    pub loc: usize,
    /// Campos de un struct, variantes de un enum, o métodos requeridos de un
    /// trait.
    pub fields: Vec<String>,
    /// **Los métodos NO viven en el tipo: viven en bloques `impl`** que
    /// pueden estar en cualquier parte del archivo. Un `struct` de Rust no
    /// "tiene" métodos como una clase — los `impl` se los agregan, y puede
    /// haber varios. Por eso hay que juntar las dos cosas en vez de leer el
    /// cuerpo del tipo.
    pub methods: Vec<RustMethodRef>,
    /// Traits implementados para este tipo (o supertraits, en un trait).
    pub bases: Vec<String>,
    pub is_public: bool,
}

#[derive(Serialize, Clone)]
pub struct RustMethodRef {
    pub name: String,
    pub line: usize,
    /// El trait que lo aporta, si viene de un `impl Trait for Tipo`.
    ///
    /// **`impl Trait for Tipo` no es lo mismo que `impl Tipo`**: el primero
    /// cumple un contrato ajeno, el segundo agrega métodos propios. Contar
    /// los dos juntos perdería qué traits implementa el tipo, que es
    /// justamente lo que se quiere ver.
    pub from_trait: Option<String>,
}

fn es_publico(n: Node, source: &str) -> bool {
    let mut c = n.walk();
    let hay = n.children(&mut c).any(|h| h.kind() == "visibility_modifier" && text_of(h, source).starts_with("pub"));
    hay
}

/// Los nombres declarados en el cuerpo de un tipo.
///
/// **Un `enum` de Rust no es un `enum` de C**: sus variantes pueden llevar
/// datos (`Activo(u32)`, `Error { codigo, msg }`), o sea que es un tipo
/// SUMA, no una lista de constantes. Se registra la variante con su forma
/// para que eso se vea.
fn campos_de(cuerpo: Node, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    // **Solo los hijos DIRECTOS del cuerpo, no un recorrido en profundidad.**
    // Con `walk_flat` una variante con forma de struct (`Error { codigo: i32 }`)
    // se contaba dos veces: una como variante y otra como campo suelto,
    // porque el recorrido bajaba adentro de ella. Lo vio la medición contra
    // el motor vivo, no un test.
    let mut c = cuerpo.walk();
    let hijos: Vec<Node> = cuerpo.named_children(&mut c).collect();
    for n in hijos {
        match n.kind() {
            "field_declaration" => {
                if let Some(nombre) = n.child_by_field_name("name") {
                    let tipo = n.child_by_field_name("type").map(|t| text_of(t, source)).unwrap_or("");
                    out.push(format!("{}: {tipo}", text_of(nombre, source)));
                }
            }
            "enum_variant" => {
                if let Some(nombre) = n.child_by_field_name("name") {
                    let forma = n.child_by_field_name("body").map(|b| text_of(b, source).replace(char::is_whitespace, "")).unwrap_or_default();
                    out.push(format!("{}{forma}", text_of(nombre, source)));
                }
            }
            // **Método REQUERIDO de un trait**: firma sin cuerpo. No es una
            // función definida —por eso no aparece en `functions`, y está
            // bien— pero sí es parte del contrato que el trait declara.
            "function_signature_item" => {
                if let Some(nombre) = n.child_by_field_name("name") {
                    out.push(format!("fn {}(…)", text_of(nombre, source)));
                }
            }
            // **Método PROVISTO**: una firma con cuerpo por defecto dentro
            // de un trait. También es parte del trait, y la diferencia con
            // el anterior importa: el requerido obliga a quien implemente,
            // el provisto no. Se distinguen en el texto en vez de mezclarse.
            "function_item" => {
                if let Some(nombre) = n.child_by_field_name("name") {
                    out.push(format!("fn {}(…) [con cuerpo]", text_of(nombre, source)));
                }
            }
            _ => {}
        }
    }
    out
}

/// El nombre base de un tipo en un `impl`: de `Punto<T>` interesa `Punto`.
fn nombre_base_de_tipo(n: Node, source: &str) -> String {
    let t = text_of(n, source);
    // Dos recortes, en orden: `Punto<T>` -> `Punto`, y `modulo::Punto` ->
    // `Punto`. El `::` no cabe en un `char`, asi que va aparte.
    let sin_generico = t.split('<').next().unwrap_or(t);
    sin_generico.rsplit("::").next().unwrap_or(sin_generico).trim().to_string()
}

/// Los tipos de un archivo de Rust, con los métodos que sus `impl` les
/// agregan.
pub fn extract_types(raiz: Node, source: &str) -> Vec<RustType> {
    let mut tipos: Vec<RustType> = Vec::new();

    tsnode::walk_flat(raiz, &mut |n| {
        let kind = match n.kind() {
            "struct_item" => "struct",
            "enum_item" => "enum",
            "trait_item" => "trait",
            "union_item" => "union",
            _ => return,
        };
        let Some(nombre) = n.child_by_field_name("name") else { return };
        let line = n.start_position().row + 1;
        let end_line = n.end_position().row + 1;
        let fields = n.child_by_field_name("body").map(|b| campos_de(b, source)).unwrap_or_default();
        // Los supertraits de un trait viven en `bounds` (`trait T: Clone`).
        let bases = n
            .child_by_field_name("bounds")
            .map(|b| text_of(b, source).trim_start_matches(':').split('+').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
            .unwrap_or_default();
        tipos.push(RustType {
            name: text_of(nombre, source).to_string(),
            kind,
            line,
            end_line,
            loc: end_line.saturating_sub(line) + 1,
            fields,
            methods: Vec::new(),
            bases,
            is_public: es_publico(n, source),
        });
    });

    // Segunda pasada: los `impl` reparten sus métodos y sus traits.
    tsnode::walk_flat(raiz, &mut |n| {
        if n.kind() != "impl_item" {
            return;
        }
        let Some(tipo_nodo) = n.child_by_field_name("type") else { return };
        let nombre_tipo = nombre_base_de_tipo(tipo_nodo, source);
        let trait_impl = n.child_by_field_name("trait").map(|t| nombre_base_de_tipo(t, source));

        let mut metodos = Vec::new();
        if let Some(cuerpo) = n.child_by_field_name("body") {
            tsnode::walk_flat(cuerpo, &mut |m| {
                if m.kind() != "function_item" {
                    return;
                }
                if let Some(nm) = m.child_by_field_name("name") {
                    metodos.push(RustMethodRef {
                        name: text_of(nm, source).to_string(),
                        line: m.start_position().row + 1,
                        from_trait: trait_impl.clone(),
                    });
                }
            });
        }

        // **Un `impl` para un tipo que no está en este archivo no inventa un
        // tipo**: `impl MiTrait for String` no agrega `String` a la lista.
        // Solo se reparten métodos a tipos declarados acá.
        if let Some(t) = tipos.iter_mut().find(|t| t.name == nombre_tipo) {
            t.methods.extend(metodos);
            if let Some(tr) = &trait_impl {
                if !t.bases.contains(tr) {
                    t.bases.push(tr.clone());
                }
            }
        }
    });

    tipos.sort_by_key(|t| t.line);
    tipos
}

#[derive(Serialize)]
pub struct RustParseResult {
    pub functions: Vec<RustFunction>,
    /// Fase 34 (2026-09-26) — struct/enum/trait/union con los métodos que
    /// sus `impl` les agregan. Se llama `classes` en el JSON por el nombre
    /// que el panel ya usa, pero Rust no tiene clases: son tipos.
    #[serde(rename = "classes")]
    pub types: Vec<RustType>,
    pub imports: Vec<RustUse>,
    /// Aristas entre funciones **definidas en este archivo**, mismo criterio
    /// que los otros seis lenguajes: una llamada a algo de otro crate no
    /// tiene nodo que apuntar.
    pub call_graph: Vec<CallEdge>,
    pub structural_smells: Vec<StructuralSmell>,
    pub trust_surface: TrustSurface,
    pub proof_surface: ProofSurface,
}

/// Nodos que suman una rama al cálculo de complejidad ciclomática. Mismo
/// criterio que el resto del motor: cada punto de decisión, uno.
const RAMAS: &[&str] = &[
    "if_expression",
    "match_arm",
    "while_expression",
    "loop_expression",
    "for_expression",
    "try_expression",
];

/// `true` si algún hijo —**incluidos los anónimos**— es el token `unsafe`.
/// Ver el doc del módulo: en `impl_item` esa es la única forma de verlo.
fn tiene_token_unsafe(n: Node) -> bool {
    let mut c = n.walk();
    // Se liga a un `let` en vez de devolverlo directo: como expresión final,
    // el iterador temporal sobrevive al cursor local y el préstamo no cierra.
    let hay = n.children(&mut c).any(|h| h.kind() == "unsafe");
    hay
}

/// `true` si la firma de la función lleva `unsafe`, que en `function_item`
/// vive dentro de un `function_modifiers` nombrado.
fn firma_unsafe(f: Node, source: &str) -> bool {
    let mut c = f.walk();
    let hay = f
        .children(&mut c)
        .any(|h| h.kind() == "function_modifiers" && tsnode::text_of(h, source).contains("unsafe"));
    hay
}

/// El comentario que precede a un nodo, saltando hacia arriba hasta el nivel
/// de sentencia.
///
/// Un `unsafe_block` casi nunca es hijo directo de su bloque: suele estar
/// envuelto en un `expression_statement` o en un `let`. Sin subir, el
/// `prev_sibling` es `None` y todo bloque parecería indocumentado.
fn comentarios_previos<'a>(nodo: Node<'a>, raiz: Node<'a>) -> Vec<Node<'a>> {
    let mut actual = nodo;
    while actual.prev_sibling().is_none() {
        match actual.parent() {
            Some(p) if p.id() != raiz.id() => actual = p,
            _ => return Vec::new(),
        }
    }
    let mut out = Vec::new();
    let mut previo = actual.prev_sibling();
    // Se juntan TODOS los comentarios consecutivos: una justificación de
    // varias líneas es lo normal, y mirar solo el de más abajo perdería la
    // que empieza con `// SAFETY:` arriba.
    while let Some(p) = previo {
        if !matches!(p.kind(), "line_comment" | "block_comment") {
            break;
        }
        out.push(p);
        previo = p.prev_sibling();
    }
    out
}

fn hay_safety(comentarios: &[Node], source: &str) -> bool {
    comentarios.iter().any(|c| {
        let t = tsnode::text_of(*c, source).to_uppercase();
        t.contains("SAFETY")
    })
}

fn nombre_de_funcion<'a>(f: Node, source: &'a str) -> &'a str {
    f.child_by_field_name("name").map(|n| tsnode::text_of(n, source)).unwrap_or("<anonymous>")
}

/// El nombre del metodo de una llamada `x.metodo(...)`, si lo es.
fn metodo_llamado<'a>(n: Node, source: &'a str) -> Option<&'a str> {
    if n.kind() != "call_expression" {
        return None;
    }
    let f = n.child_by_field_name("function")?;
    if f.kind() != "field_expression" {
        return None;
    }
    Some(tsnode::text_of(f.child_by_field_name("field")?, source))
}

/// `true` si la funcion devuelve un `Result` — o sea, si tiene `?`
/// disponible para propagar el error en vez de descartarlo.
///
/// Se mira el TEXTO del tipo de retorno y no se resuelven alias: un
/// `-> MiResultado<T>` que sea `type MiResultado<T> = Result<T, E>` no se
/// detecta. Es una subcuenta conocida, no un olvido — resolver alias
/// necesita tabla de tipos, y sin ella la respuesta honesta es no afirmar.
fn devuelve_result(f: Node, source: &str) -> bool {
    f.child_by_field_name("return_type")
        .is_some_and(|t| tsnode::text_of(t, source).trim_start().starts_with("Result"))
}

/// Cuenta `.unwrap()` en el cuerpo de una funcion **sin entrar a los
/// closures ni a las funciones anidadas**.
///
/// El motivo es exacto, no de prolijidad: `?` propaga al `Result` de la
/// funcion que lo contiene. Adentro de un closure, `?` propagaria al closure
/// —no a la funcion de afuera—, asi que ahi `.unwrap()` no tiene la
/// alternativa que este hallazgo recomienda, y marcarlo seria falso.
fn unwraps_propios(f: Node, source: &str) -> Vec<usize> {
    fn walk(n: Node, source: &str, out: &mut Vec<usize>) {
        if matches!(n.kind(), "closure_expression" | "function_item") {
            return;
        }
        if metodo_llamado(n, source) == Some("unwrap") {
            out.push(n.start_position().row + 1);
        }
        let mut c = n.walk();
        for h in n.children(&mut c) {
            walk(h, source, out);
        }
    }
    let mut out = Vec::new();
    let mut c = f.walk();
    for h in f.children(&mut c) {
        if matches!(h.kind(), "closure_expression" | "function_item") {
            continue;
        }
        walk(h, source, &mut out);
    }
    out
}

pub fn parse_rust(source: &str) -> Option<RustParseResult> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_rust::LANGUAGE.into()).ok()?;
    let arbol = parser.parse(source, None)?;
    let raiz = arbol.root_node();

    let mut functions = Vec::new();
    let mut imports = Vec::new();
    let mut smells = Vec::new();
    let mut trust = TrustSurface::default();
    let mut proof = ProofSurface::default();

    // Recorrido único: cada nodo se mira una vez y se reparte a quien lo
    // necesite, en vez de recorrer el árbol cuatro veces.
    tsnode::walk_flat(raiz, &mut |n| {
        // La superficie de prueba se cuenta en TODO nodo, no solo en los que
        // abren un brazo del `match` de abajo.
        match metodo_llamado(n, source) {
            Some("unwrap") => proof.unwrap_calls += 1,
            Some("expect") => proof.expect_calls += 1,
            _ => {}
        }
        if n.kind() == "type_cast_expression" {
            proof.unchecked_casts += 1;
        }
        match n.kind() {
        "use_declaration" => {
            let texto = tsnode::text_of(n, source);
            let modulo = texto
                .trim_start_matches("use")
                .trim()
                .trim_end_matches(';')
                .trim()
                .to_string();
            imports.push(RustUse {
                module: modulo,
                clase: "use",
                line: n.start_position().row + 1,
            });
        }
        "function_item" => {
            if devuelve_result(n, source) {
                proof.fallible_fns += 1;
                let lineas = unwraps_propios(n, source);
                proof.unwraps_in_fallible_fns += lineas.len();
                let nombre_fn = nombre_de_funcion(n, source).to_string();
                for linea in lineas {
                    smells.push(StructuralSmell {
                        kind: "unwrap_in_fallible_fn",
                        name: nombre_fn.clone(),
                        line: linea,
                        message: format!(
                            "`{nombre_fn}` returns a `Result`, and this `.unwrap()` throws away the proof it was carrying. The type said the operation can fail; `.unwrap()` answers by aborting the process instead of telling the caller — who is already prepared to hear it, because the signature promises a `Result`. `?` propagates it in one character. This is NOT the same as `.unwrap()` in a function that cannot return an error: there the panic may be the only option. Here the option existed and was not taken."
                        ),
                    });
                }
            }
            let es_unsafe = firma_unsafe(n, source);
            let nombre = nombre_de_funcion(n, source).to_string();
            let line = n.start_position().row + 1;
            let end_line = n.end_position().row + 1;
            let mut complexity = 1u32;
            tsnode::walk_flat(n, &mut |x| {
                if RAMAS.contains(&x.kind()) {
                    complexity += 1;
                }
            });
            if es_unsafe {
                trust.unsafe_fns += 1;
                let documentada = hay_safety(&comentarios_previos(n, raiz), source);
                if documentada {
                    trust.documented_fns += 1;
                } else {
                    smells.push(StructuralSmell {
                        kind: "unsafe_fn_without_safety_doc",
                        name: nombre.clone(),
                        line,
                        message: format!(
                            "`unsafe fn {nombre}` moves responsibility to the CALLER — calling it is only sound if the caller upholds some invariant — but no `# Safety` section in the docs says which one. That is worse than an undocumented `unsafe` block: the person who has to honour the contract is not even looking at this file. Read from the shape of the declaration and its doc comments, not from what the body does."
                        ),
                    });
                }
            }
            let (big_o, big_o_reason) = crate::costo::rscosto::inferir(n, raiz, source, &nombre);
            functions.push(RustFunction {
                name: nombre,
                line,
                end_line,
                loc: end_line.saturating_sub(line) + 1,
                complexity,
                // Fase 34 (2026-09-26) — `rscosto.rs`, el cuarto hermano de
                // `amortizado.rs`/`ccosto.rs`/`jscosto.rs`. Antes decía "no
                // estimado" para TODA función, que era honesto pero dejaba a
                // Rust como el único de los 8 targets sin la sección
                // principal del panel.
                big_o: big_o.clone(),
                big_o_reason: big_o_reason.clone(),
                is_unsafe: es_unsafe,
                calls: extract_calls_rust(n, source),
            });
        }
        "unsafe_block" => {
            trust.unsafe_blocks += 1;
            let comentarios = comentarios_previos(n, raiz);
            if hay_safety(&comentarios, source) {
                trust.documented_blocks += 1;
            } else {
                smells.push(StructuralSmell {
                    kind: "unsafe_block_without_safety_comment",
                    name: "<block>".to_string(),
                    line: n.start_position().row + 1,
                    message:
                        "An `unsafe` block with no `// SAFETY:` comment above it. In Rust `unsafe` does not mean \"insecure\": it means the compiler stops checking here and a human promise begins — and this promise was never written down, so whoever reads it next has no way to know which invariant must hold for it to stay sound. Clippy has `undocumented_unsafe_blocks` for this, but it lives in the `restriction` group and is off by default, so most projects never see it."
                            .to_string(),
                });
            }
        }
        "impl_item" if tiene_token_unsafe(n) => {
            trust.unsafe_impls += 1;
        }
        _ => {}
        }
    });

    let call_graph = build_call_graph_rust(&functions);
    let types = extract_types(raiz, source);
    Some(RustParseResult {
        functions,
        types,
        imports,
        call_graph,
        structural_smells: smells,
        trust_surface: trust,
        proof_surface: proof,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analizar(src: &str) -> RustParseResult {
        parse_rust(src).expect("el parser de Rust tiene que estar disponible")
    }

    /// **El hallazgo central de la superficie de prueba.** La firma dice que
    /// la función puede fallar, así que el llamador ya está preparado para
    /// recibir un error — y `.unwrap()` elige abortar el proceso en vez de
    /// decírselo. `?` lo propaga en un carácter.
    #[test]
    fn un_unwrap_en_una_funcion_que_devuelve_result_se_marca() {
        let src = "fn leer() -> Result<i32, E> {\n    let x = abrir().unwrap();\n    Ok(x)\n}\n";
        let r = parse_rust(src).unwrap();
        assert_eq!(r.proof_surface.fallible_fns, 1);
        assert_eq!(r.proof_surface.unwraps_in_fallible_fns, 1);
        assert!(r.structural_smells.iter().any(|s| s.kind == "unwrap_in_fallible_fn"));
    }

    /// Y la contraparte, que es la que evita que esto sea ruido: en una
    /// función que NO puede devolver error, el pánico puede ser la única
    /// opción. Ahí `.unwrap()` no tiene alternativa que recomendar.
    #[test]
    fn el_mismo_unwrap_en_una_funcion_infalible_no_se_marca() {
        let src = "fn leer() -> i32 {\n    abrir().unwrap()\n}\n";
        let r = parse_rust(src).unwrap();
        assert_eq!(r.proof_surface.fallible_fns, 0);
        assert_eq!(r.proof_surface.unwraps_in_fallible_fns, 0);
        assert!(r.structural_smells.iter().all(|s| s.kind != "unwrap_in_fallible_fn"));
        // Pero el conteo global SÍ lo ve: sigue siendo prueba descartada.
        assert_eq!(r.proof_surface.unwrap_calls, 1);
    }

    /// **`?` dentro de un closure propaga al CLOSURE, no a la función de
    /// afuera.** Así que un `.unwrap()` ahí adentro no tiene la alternativa
    /// que el hallazgo recomienda, y marcarlo sería falso. El recorrido no
    /// entra a los closures por eso, no por prolijidad.
    #[test]
    fn un_unwrap_adentro_de_un_closure_no_se_marca() {
        let src = "fn leer() -> Result<i32, E> {\n    let f = || abrir().unwrap();\n    Ok(f())\n}\n";
        let r = parse_rust(src).unwrap();
        assert_eq!(r.proof_surface.unwraps_in_fallible_fns, 0);
        assert!(r.structural_smells.iter().all(|s| s.kind != "unwrap_in_fallible_fn"));
        assert_eq!(r.proof_surface.unwrap_calls, 1, "el conteo global igual lo cuenta");
    }

    /// `.expect("por qué")` descarta la MISMA prueba que `.unwrap()`, pero
    /// deja escrito el razonamiento — exactamente la relación que hay entre
    /// un `unsafe` con `// SAFETY:` y uno sin él. Se cuenta aparte, no se
    /// mezcla, porque la diferencia es el punto entero de la métrica.
    #[test]
    fn expect_y_unwrap_se_cuentan_por_separado() {
        let src = "fn f() {\n    a().unwrap();\n    b().expect(\"ya se valido arriba\");\n}\n";
        let r = parse_rust(src).unwrap();
        assert_eq!(r.proof_surface.unwrap_calls, 1);
        assert_eq!(r.proof_surface.expect_calls, 1);
    }

    /// En Rust `as` es la conversión SIN chequear: trunca o envuelve en
    /// silencio. `try_into()` es la que conserva la prueba, devolviendo un
    /// `Result`. Se cuentan todos los `as` porque todos son la variante sin
    /// verificar, no porque todos estén mal.
    #[test]
    fn los_casts_con_as_se_cuentan_como_conversiones_sin_chequear() {
        let src = "fn f(x: i64) -> u8 {\n    let y = x as u32;\n    y as u8\n}\n";
        let r = parse_rust(src).unwrap();
        assert_eq!(r.proof_surface.unchecked_casts, 2);
    }

    #[test]
    fn el_codigo_sin_prueba_descartada_tiene_superficie_vacia() {
        let src = "fn f(x: i32) -> Result<i32, E> {\n    let y = g(x)?;\n    Ok(y)\n}\n";
        let r = parse_rust(src).unwrap();
        let p = &r.proof_surface;
        assert_eq!((p.unwrap_calls, p.expect_calls, p.unchecked_casts), (0, 0, 0));
        assert_eq!(p.fallible_fns, 1, "la funcion es falible, pero usa `?`");
        assert_eq!(p.unwraps_in_fallible_fns, 0);
    }

    /// Un alias de tipo no se resuelve: `-> MiResultado<T>` no se reconoce
    /// como `Result`. Es una subcuenta CONOCIDA, y el test la fija para que
    /// quede como decisión documentada y no como sorpresa.
    #[test]
    fn un_alias_de_result_no_se_reconoce_y_eso_esta_dicho() {
        let src = "fn f() -> MiResultado<i32> {\n    a().unwrap()\n}\n";
        let r = parse_rust(src).unwrap();
        assert_eq!(r.proof_surface.fallible_fns, 0, "sin tabla de tipos no se puede afirmar");
    }

    #[test]
    fn un_bloque_unsafe_sin_safety_se_marca() {
        let src = "fn f(v: &[u8]) -> u8 {\n    unsafe { *v.get_unchecked(0) }\n}\n";
        let r = analizar(src);
        assert_eq!(r.trust_surface.unsafe_blocks, 1);
        assert_eq!(r.trust_surface.documented_blocks, 0);
        assert!(r.structural_smells.iter().any(|s| s.kind == "unsafe_block_without_safety_comment"));
    }

    #[test]
    fn un_comentario_safety_lo_calla() {
        // El `unsafe_block` esta envuelto en un `expression_statement`, asi
        // que sin subir al nivel de sentencia el `prev_sibling` seria `None`
        // y TODO bloque pareceria indocumentado.
        let src = concat!(
            "fn f(v: &[u8]) -> u8 {\n",
            "    // SAFETY: v no esta vacio, chequeado por el llamador.\n",
            "    unsafe { *v.get_unchecked(0) }\n",
            "}\n",
        );
        let r = analizar(src);
        assert_eq!(r.trust_surface.unsafe_blocks, 1);
        assert_eq!(r.trust_surface.documented_blocks, 1, "{:?}", r.structural_smells.len());
        assert!(r.structural_smells.is_empty());
    }

    #[test]
    fn una_justificacion_de_varias_lineas_tambien_cuenta() {
        // Mirar solo el comentario de mas abajo perderia el que empieza con
        // SAFETY arriba.
        let src = concat!(
            "fn f(v: &[u8]) -> u8 {\n",
            "    // SAFETY: el invariante lo garantiza el constructor,\n",
            "    // que rechaza vectores vacios.\n",
            "    unsafe { *v.get_unchecked(0) }\n",
            "}\n",
        );
        assert!(analizar(src).structural_smells.is_empty());
    }

    #[test]
    fn una_unsafe_fn_sin_seccion_safety_se_marca() {
        let src = "unsafe fn leer(p: *const u8) -> u8 { *p }\n";
        let r = analizar(src);
        assert_eq!(r.trust_surface.unsafe_fns, 1);
        assert_eq!(r.trust_surface.documented_fns, 0);
        let s = r
            .structural_smells
            .iter()
            .find(|s| s.kind == "unsafe_fn_without_safety_doc")
            .expect("esperaba el hallazgo");
        assert!(s.message.contains("CALLER"), "{}", s.message);
    }

    #[test]
    fn una_unsafe_fn_documentada_no_se_marca() {
        let src = concat!(
            "/// # Safety\n",
            "/// El puntero tiene que ser valido y estar alineado.\n",
            "pub unsafe fn leer(p: *const u8) -> u8 { *p }\n",
        );
        let r = analizar(src);
        assert_eq!(r.trust_surface.unsafe_fns, 1);
        assert_eq!(r.trust_surface.documented_fns, 1);
        assert!(r.structural_smells.is_empty());
    }

    #[test]
    fn un_unsafe_impl_se_cuenta_aunque_el_token_sea_anonimo() {
        // En `impl_item` el `unsafe` es un token ANONIMO: un recorrido sobre
        // nodos nombrados no lo ve nunca, y el conteo daria cero sin fallar.
        let src = "struct Cosa;\nunsafe impl Send for Cosa {}\n";
        let r = analizar(src);
        assert_eq!(r.trust_surface.unsafe_impls, 1);
    }

    #[test]
    fn un_impl_normal_no_cuenta_como_superficie() {
        let src = "struct Cosa;\nimpl Cosa { fn nueva() -> Self { Cosa } }\n";
        let r = analizar(src);
        assert_eq!(r.trust_surface.unsafe_impls, 0);
    }

    #[test]
    fn el_codigo_seguro_no_produce_superficie_ninguna() {
        let src = "use std::fmt;\n\nfn suma(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        let r = analizar(src);
        assert!(r.structural_smells.is_empty());
        assert_eq!(r.trust_surface.unsafe_blocks, 0);
        assert_eq!(r.trust_surface.unsafe_fns, 0);
        assert_eq!(r.functions.len(), 1);
        assert_eq!(r.functions[0].name, "suma");
        assert_eq!(r.imports.len(), 1);
        assert_eq!(r.imports[0].module, "std::fmt");
        // El panel lee `imp.type` sin chequear: si el JSON no lo trae, el
        // render del archivo entero explota. Se afirma sobre el JSON
        // serializado, no sobre el struct, porque `type` es un nombre
        // renombrado por serde y el campo de Rust se llama distinto.
        let j = serde_json::to_value(&r.imports[0]).unwrap();
        assert_eq!(j["type"], "use", "el panel necesita `type` en cada import");
    }

    #[test]
    fn la_complejidad_cuenta_los_puntos_de_decision() {
        let src = concat!(
            "fn clasificar(n: i32) -> i32 {\n",
            "    if n < 0 { return -1; }\n",
            "    match n {\n",
            "        0 => 0,\n",
            "        _ => 1,\n",
            "    }\n",
            "}\n",
        );
        let r = analizar(src);
        // 1 base + 1 `if` + 2 brazos de `match`.
        assert_eq!(r.functions[0].complexity, 4, "{:?}", r.functions[0].complexity);
    }

    /// **Este test decia lo contrario hasta el 2026-09-26**, y decia bien:
    /// mientras el Big-O no cubriera Rust, devolver `"no estimado"` era mas
    /// honesto que inventar una cota. Ahora `rscosto.rs` lo cubre, asi que
    /// el contrato cambio de "ausencia honesta" a "estimacion real" y el
    /// test afirma lo nuevo — no se relajo, se reemplazo.
    #[test]
    fn el_big_o_de_rust_se_estima_de_verdad() {
        let r = analizar("fn f() {}\nfn g(xs: &[i32]) {\n    for _x in xs {}\n}\n");
        assert_eq!(r.functions[0].big_o, "O(1)");
        assert_eq!(r.functions[1].big_o, "O(n)");
        assert!(!r.functions.iter().any(|f| f.big_o == "no estimado"));
    }

    // ── Grafo de llamadas (Fase 34, 2026-09-26) ─────────────────────────

    /// **El hueco que encontró la matriz de capacidades**: Rust era el único
    /// de los 8 targets sin grafo de llamadas — ni siquiera con la clave
    /// presente. El mismo programa en Python daba tres aristas.
    #[test]
    fn el_grafo_de_llamadas_sale_como_en_los_otros_lenguajes() {
        let r = analizar("pub fn validar(xs: &[i32]) -> i32 {\n    let s = sumar(xs);\n    let p = promediar(s, xs.len());\n    p\n}\nfn sumar(xs: &[i32]) -> i32 { xs.iter().sum() }\nfn promediar(s: i32, n: usize) -> i32 { s / n as i32 }\n");
        let mut aristas: Vec<String> = r.call_graph.iter().map(|e| format!("{}->{}", e.from, e.to)).collect();
        aristas.sort();
        assert_eq!(aristas, vec!["validar->promediar", "validar->sumar"], "{aristas:?}");
    }

    /// **Solo aristas a funciones DEFINIDAS en este archivo**, mismo criterio
    /// que los otros seis: una llamada a otro crate no tiene nodo al que
    /// apuntar.
    #[test]
    fn una_llamada_externa_no_produce_arista() {
        let r = analizar("fn f() {\n    otra_crate::hacer();\n}\n");
        assert!(r.call_graph.is_empty(), "{:?}", r.call_graph.len());
        // Pero el nombre SÍ queda en `calls`, que es lo que hacen los demás.
        assert!(r.functions[0].calls.contains(&"hacer".to_string()), "{:?}", r.functions[0].calls);
    }

    /// `modulo::f(x)` — interesa la última parte, que es el nombre.
    #[test]
    fn una_llamada_calificada_se_resuelve_por_su_ultima_parte() {
        let r = analizar("fn f() {\n    self::ayuda();\n}\nfn ayuda() {}\n");
        assert_eq!(r.call_graph.len(), 1, "{:?}", r.call_graph.iter().map(|e| (&e.from, &e.to)).collect::<Vec<_>>());
    }

    /// Los métodos entran en `calls` aunque casi nunca produzcan arista:
    /// resolverlos a una definición exige tipos, y sin ellos no se afirma.
    #[test]
    fn los_metodos_entran_en_calls_aunque_no_se_resuelvan() {
        let r = analizar("fn f(v: Vec<i32>) -> usize {\n    v.len()\n}\n");
        assert!(r.functions[0].calls.contains(&"len".to_string()), "{:?}", r.functions[0].calls);
        assert!(r.call_graph.is_empty(), "sin tipos no se afirma a qué apunta");
    }

    /// **Las macros quedan afuera a propósito**: `println!` es un
    /// `macro_invocation`, no una llamada, y meterla pondría en el grafo
    /// nodos que no son funciones del archivo.
    #[test]
    fn las_macros_no_cuentan_como_llamadas() {
        let r = analizar("fn f() {\n    println!(\"hola\");\n}\n");
        assert!(r.functions[0].calls.is_empty(), "{:?}", r.functions[0].calls);
    }

    #[test]
    fn un_archivo_sin_llamadas_da_un_grafo_vacio_no_ausente() {
        let r = analizar("fn f() -> i32 { 1 }\n");
        assert!(r.call_graph.is_empty());
    }

    // ── Tipos de Rust (Fase 34, 2026-09-26) ─────────────────────────────

    /// **El hueco**: struct, enum y trait eran completamente invisibles — la
    /// clave `classes` ni existía.
    #[test]
    fn los_cuatro_tipos_se_extraen() {
        let r = analizar("pub struct S { x: i32 }\npub enum E { A }\npub trait T { }\npub union U { a: i32 }\n");
        let k: Vec<&str> = r.types.iter().map(|t| t.kind).collect();
        assert_eq!(k, vec!["struct", "enum", "trait", "union"], "{k:?}");
        assert!(r.types.iter().all(|t| t.is_public));
    }

    /// **Los métodos no viven en el tipo: viven en bloques `impl`** que
    /// pueden estar en cualquier parte. Hay que juntar las dos cosas.
    #[test]
    fn los_impl_reparten_sus_metodos_al_tipo() {
        let r = analizar("struct P { x: f64 }\nimpl P {\n    fn nuevo() -> Self { todo!() }\n    fn area(&self) -> f64 { 0.0 }\n}\n");
        let p = &r.types[0];
        let m: Vec<&str> = p.methods.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(m, vec!["nuevo", "area"], "{m:?}");
        assert!(p.methods.iter().all(|x| x.from_trait.is_none()), "un `impl Tipo` no viene de un trait");
    }

    /// **Y varios `impl` para el mismo tipo se suman**, que es justamente lo
    /// que un lector de una clase no esperaría.
    #[test]
    fn varios_impl_del_mismo_tipo_se_suman() {
        let r = analizar("struct P;\nimpl P { fn a() {} }\nimpl P { fn b() {} }\n");
        assert_eq!(r.types[0].methods.len(), 2);
    }

    /// **`impl Trait for Tipo` no es lo mismo que `impl Tipo`**: el primero
    /// cumple un contrato ajeno, y eso queda registrado.
    #[test]
    fn un_impl_de_trait_registra_de_donde_viene_el_metodo() {
        let r = analizar("struct P;\ntrait D { fn dibujar(&self); }\nimpl D for P { fn dibujar(&self) {} }\n");
        let p = r.types.iter().find(|t| t.name == "P").unwrap();
        assert_eq!(p.methods[0].from_trait.as_deref(), Some("D"));
        assert!(p.bases.contains(&"D".to_string()), "el trait implementado sale como base: {:?}", p.bases);
    }

    /// **Un `enum` de Rust no es un `enum` de C**: las variantes llevan
    /// datos, así que es un tipo SUMA y la forma se registra.
    #[test]
    fn las_variantes_de_un_enum_conservan_su_forma() {
        let r = analizar("enum E { Nada, Uno(u32), Campos { a: i32 } }\n");
        let f = &r.types[0].fields;
        assert!(f.contains(&"Nada".to_string()), "{f:?}");
        assert!(f.iter().any(|x| x.starts_with("Uno(")), "una variante con datos conserva su forma: {f:?}");
        assert!(f.iter().any(|x| x.starts_with("Campos{")), "{f:?}");
    }

    #[test]
    fn los_campos_de_un_struct_traen_su_tipo() {
        let r = analizar("struct P { x: f64, nombre: String }\n");
        assert_eq!(r.types[0].fields, vec!["x: f64", "nombre: String"]);
    }

    /// Los supertraits van a `bases`, que es lo más cercano a un "extends"
    /// que Rust tiene.
    #[test]
    fn los_supertraits_van_a_bases() {
        let r = analizar("trait D: Clone + Send { }\n");
        assert_eq!(r.types[0].bases, vec!["Clone", "Send"]);
    }

    /// **Un `impl` para un tipo de OTRO archivo no inventa un tipo**:
    /// `impl MiTrait for String` no agrega `String` a la lista.
    #[test]
    fn un_impl_sobre_un_tipo_ajeno_no_inventa_un_tipo() {
        let r = analizar("trait T { fn f(&self); }\nimpl T for String { fn f(&self) {} }\n");
        let nombres: Vec<&str> = r.types.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(nombres, vec!["T"], "solo el trait declarado acá: {nombres:?}");
    }

    /// Un genérico o un camino calificado no rompen el emparejamiento.
    #[test]
    fn un_impl_generico_o_calificado_encuentra_su_tipo() {
        let r = analizar("struct P<T> { x: T }\nimpl<T> P<T> { fn f(&self) {} }\n");
        assert_eq!(r.types[0].methods.len(), 1, "{:?}", r.types[0].methods.len());
    }

    /// **El método REQUERIDO de un trait no es una función definida** —no
    /// tiene cuerpo—, así que no aparece en `functions`; pero sí es parte
    /// del contrato, y por eso se registra como campo del trait.
    #[test]
    fn el_metodo_requerido_de_un_trait_no_es_una_funcion_pero_si_un_campo() {
        let r = analizar("trait D { fn dibujar(&self) -> String; }\n");
        assert!(r.functions.is_empty(), "una firma sin cuerpo no es una función definida");
        assert_eq!(r.types[0].fields, vec!["fn dibujar(…)"]);
    }

    #[test]
    fn un_archivo_sin_tipos_da_una_lista_vacia_no_ausente() {
        let r = analizar("fn f() {}\n");
        assert!(r.types.is_empty());
    }

    /// **Defecto que vio la medición contra el motor vivo, no un test**: una
    /// variante con forma de struct se contaba DOS veces —como variante y
    /// como campo suelto— porque el recorrido bajaba adentro de ella.
    #[test]
    fn una_variante_con_forma_de_struct_no_se_cuenta_dos_veces() {
        let r = analizar("enum E { Error { codigo: i32 } }\n");
        assert_eq!(r.types[0].fields, vec!["Error{codigo:i32}"], "{:?}", r.types[0].fields);
    }

    /// **Requerido contra provisto**: el primero obliga a quien implemente,
    /// el segundo no. Los dos son parte del trait y se distinguen.
    #[test]
    fn el_trait_distingue_metodo_requerido_de_provisto() {
        let r = analizar("trait D {\n    fn dibujar(&self) -> String;\n    fn area(&self) -> f64 { 0.0 }\n}\n");
        let f = &r.types[0].fields;
        assert!(f.contains(&"fn dibujar(…)".to_string()), "{f:?}");
        assert!(f.contains(&"fn area(…) [con cuerpo]".to_string()), "{f:?}");
    }
}
