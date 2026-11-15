//! Parser de Ada — Fase 34, Línea E, **paso 1 de tres**: Ada como lenguaje
//! *analizado*.
//!
//! Los otros dos pasos de esa línea (código máquina vía GNAT, y SPARK como
//! tercera categoría de evidencia en `decidibilidad.rs`) están bloqueados
//! por el toolchain ausente en esta máquina. **Este paso no lo está**, y por
//! eso se hace primero: la gramática `tree-sitter-ada` parsea desde el texto,
//! igual que los otros seis lenguajes del motor.
//!
//! **Y eso es una decisión, no una limitación.** El plan original decía
//! "aprovechar GNAT para obtener la información semántica". Eso convertiría a
//! Ada en el único lenguaje cuyo ANÁLISIS exige un compilador instalado. Se
//! paga un precio concreto por no hacerlo, y está documentado donde se paga:
//! ver `llamadas` — sin semántica, `A (I)` es indistinguible de una llamada a
//! `A`, porque en Ada el indexado de arrays y la llamada a función se
//! escriben igual. La gramática lo refleja: las dos formas son un
//! `function_call`.
//!
//! Lo que sí aporta Ada y ningún otro lenguaje del motor da gratis: el fuente
//! **distingue** el `and`/`or` que evalúa los dos lados del `and then`/`or
//! else` que corta. En C y en Fortran el motor cuenta `&&`/`.AND.` como punto
//! de decisión y acierta por convención; acá la cuenta es exacta, y
//! `solo_los_cortocircuitos_cuentan` lo fija como test.

use serde::Serialize;
use tree_sitter::{Node, Parser};

use crate::flujo::graph::{self, CallEdge};
use crate::hallazgos::smells::{self, StructuralSmell};
use crate::ast::tsnode::{self, text_of};

/// Un subprograma **con cuerpo**. Las declaraciones sin cuerpo (lo que vive
/// en un `.ads`) van en `AdaSpec`, no acá: inventarles `loc`, `complexity` y
/// Big-O a partir de su firma sería reportar métricas de un cuerpo que no se
/// leyó.
#[derive(Serialize, Clone)]
pub struct AdaSubprogram {
    pub name: String,
    /// `function`, `procedure` o `expression_function` (la forma
    /// `function F (X : T) return T is (X + 1);` de Ada 2012, que es un
    /// cuerpo de verdad aunque no tenga `begin`).
    pub kind: &'static str,
    pub line: usize,
    pub end_line: usize,
    /// Incluye los subprogramas anidados, porque es el span del nodo. Las
    /// métricas que SÍ se desanidan (`complexity`, `loop_depth`, `calls`) lo
    /// dicen en su propia documentación.
    pub loc: usize,
    pub loop_depth: u32,
    pub complexity: u32,
    pub big_o: String,
    pub big_o_reason: String,
    /// Parámetros, contados por NOMBRE y no por declaración: `(A, B : Integer)`
    /// es una `parameter_specification` con dos parámetros.
    pub params: usize,
    pub calls: Vec<String>,
}

/// Una declaración sin cuerpo: `function Vacia (C : Contenedor) return
/// Boolean;` dentro de un `.ads`. Es la interfaz del paquete, y un `.ads`
/// entero produce cero `functions` y N `specs` — que es la verdad sobre ese
/// archivo.
#[derive(Serialize, Clone)]
pub struct AdaSpec {
    pub name: String,
    pub kind: &'static str,
    pub line: usize,
    pub params: usize,
}

/// Unidad de compilación o unidad de programa anidada: paquetes (declaración
/// y cuerpo), tipos tarea y tipos protegidos.
#[derive(Serialize, Clone)]
pub struct AdaUnit {
    pub name: String,
    pub kind: &'static str,
    pub line: usize,
    pub end_line: usize,
    /// `true` si el paquete está envuelto en un `generic_package_declaration`.
    /// Importa para el paso 3 (SPARK): un genérico se prueba por instancia.
    pub is_generic: bool,
}

/// Una cláusula `with`. `is_private` distingue `private with X;` de `with
/// X;`: la dependencia privada no se propaga a los clientes del paquete, así
/// que es una arista de otro tipo en el grafo de dependencias.
///
/// **El campo se llama `module` y no `unit` a propósito.** El panel renderiza
/// cada import leyendo `imp.module` y `imp.type`, igual para los siete
/// lenguajes; con `unit` el chip habría salido en blanco en el navegador sin
/// que ningún test de Rust ni de Python lo notara. En Ada la unidad importada
/// se llama unidad, no módulo — pero el nombre del campo es parte del
/// contrato del shape, no del vocabulario del lenguaje.
#[derive(Serialize, Clone)]
pub struct AdaWith {
    pub module: String,
    pub line: usize,
    pub is_private: bool,
    /// `with` o `private_with`, para la etiqueta del chip.
    #[serde(rename = "type")]
    pub tipo: &'static str,
}

#[derive(Serialize)]
pub struct AdaParseResult {
    pub functions: Vec<AdaSubprogram>,
    pub specs: Vec<AdaSpec>,
    pub units: Vec<AdaUnit>,
    pub imports: Vec<AdaWith>,
    pub call_graph: Vec<CallEdge>,
    pub structural_smells: Vec<StructuralSmell>,
}

/// Los tres nodos que llevan cuerpo. Se usan tanto para recolectar como para
/// SALTEAR: un subprograma anidado tiene su propia complejidad, su propia
/// profundidad de bucles y sus propias llamadas, igual que en
/// `ccosto::ciclomatica` una función anidada en C no suma a la de afuera.
///
/// **El tercero lo encontró un test que probaba una afirmación mía y la
/// desmintió.** `procedure P is null;` (Ada 2005) es un cuerpo: la gramática
/// le da nodo propio, `null_procedure_declaration`, y el parser lo tiraba en
/// silencio — ni cuerpo, ni firma, ni nada. Su cuerpo no hace nada, pero el
/// subprograma existe, se puede llamar y tiene parámetros.
fn es_cuerpo_de_subprograma(n: Node) -> bool {
    matches!(n.kind(), "subprogram_body" | "expression_function_declaration" | "null_procedure_declaration")
}

/// El nombre de una `function_specification`/`procedure_specification`.
///
/// Las tres formas que aparecen, y la tercera **la encontró la medición, no la
/// gramática**: de los 9.970 cuerpos de un corpus de 2.718 archivos reales,
/// 111 salían como `<anonymous>` — todos sobrecargas de operador.
///   - `identifier`: el caso normal, `function Suma (...)`;
///   - `selected_component`: una unidad hija, `procedure Padre.Hijo (...)`,
///     donde el nombre es el texto entero y no sólo la última componente;
///   - `string_literal`: una sobrecarga de operador, `function "=" (L, R : T)
///     return Boolean`. Ada escribe el operador entre comillas, así que la
///     gramática lo entrega como literal de cadena y no como identificador.
///     Es idiomático, no un caso raro: 111 de 9.970.
fn nombre_de_especificacion(spec: Node, source: &str) -> String {
    let mut cursor = spec.walk();
    for hijo in spec.named_children(&mut cursor) {
        if matches!(hijo.kind(), "identifier" | "selected_component" | "string_literal") {
            return text_of(hijo, source).to_string();
        }
    }
    "<anonymous>".to_string()
}

/// Parámetros de una especificación, contados por nombre.
///
/// `(A, B : Integer; C : Float)` son TRES parámetros en dos
/// `parameter_specification`. Los nombres son los `identifier` que están
/// antes del `:`; los que están después son el tipo, y `in out` aparece como
/// `non_empty_mode` en el medio.
fn cuenta_parametros(spec: Node) -> usize {
    let mut cursor = spec.walk();
    let formal = spec.named_children(&mut cursor).find(|c| c.kind() == "formal_part");
    let Some(formal) = formal else {
        return 0;
    };
    let mut total = 0;
    let mut c2 = formal.walk();
    for ps in formal.named_children(&mut c2) {
        if ps.kind() != "parameter_specification" {
            continue;
        }
        let mut c3 = ps.walk();
        for hijo in ps.children(&mut c3) {
            if hijo.kind() == ":" {
                break;
            }
            if hijo.kind() == "identifier" {
                total += 1;
            }
        }
    }
    total
}

/// **Un bucle con cuenta de vueltas literal no agrega profundidad
/// asintótica.**
///
/// No es una regla nueva: es la misma que `bigo::vueltas_constantes` aplica
/// en Python y `space.rs` en el consumo de memoria. `for I in 1 .. 10 loop`
/// corre diez veces para cualquier entrada, así que un `for J in 1 .. N`
/// adentro de ese bucle es O(n), no O(n²).
///
/// Un `while` y un `loop` pelado NO entran acá: su cantidad de vueltas
/// depende del estado, y afirmar que es constante requeriría probar la
/// terminación — que es justo lo que `decidibilidad.rs` clasifica como no
/// decidible.
fn vueltas_constantes(bucle: Node) -> bool {
    let mut cursor = bucle.walk();
    let Some(esquema) = bucle.named_children(&mut cursor).find(|c| c.kind() == "iteration_scheme") else {
        return false;
    };
    let mut c2 = esquema.walk();
    let Some(lps) = esquema.named_children(&mut c2).find(|c| c.kind() == "loop_parameter_specification") else {
        return false;
    };
    let mut c3 = lps.walk();
    let Some(rango) = lps.named_children(&mut c3).find(|c| c.kind() == "range_g") else {
        return false;
    };
    // Los dos extremos tienen que ser literales. Un `term` envuelve al
    // literal, así que se pregunta por el subárbol del extremo y no por su
    // hijo directo.
    let mut c4 = rango.walk();
    let extremos: Vec<Node> = rango.named_children(&mut c4).collect();
    if extremos.len() != 2 {
        return false;
    }
    extremos.iter().all(|e| {
        let mut solo_literal = true;
        let mut visto = false;
        tsnode::walk_flat(*e, &mut |n| match n.kind() {
            "numeric_literal" => visto = true,
            "identifier" | "function_call" | "selected_component" => solo_literal = false,
            _ => {}
        });
        visto && solo_literal
    })
}

/// Profundidad de bucles anidados, **sin contar los de subprogramas
/// anidados** ni los de cuenta literal.
fn profundidad_bucles(node: Node) -> u32 {
    fn rec(n: Node, es_raiz: bool) -> u32 {
        if !es_raiz && es_cuerpo_de_subprograma(n) {
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
        if n.kind() == "loop_statement" && !vueltas_constantes(n) {
            max_hijo + 1
        } else {
            max_hijo
        }
    }
    rec(node, true)
}

/// **Complejidad ciclomática de Ada, leída del árbol.**
///
/// Los puntos de decisión, con el motivo de cada uno:
///   - `if_statement` y cada `elsif_statement_item` (en Ada un `elsif` es un
///     nodo propio, como el `elseif_clause` de Fortran y a diferencia del
///     `elif` de Python, que es un `If` anidado);
///   - cada `loop_statement`, incluido el pelado — un `loop ... exit when
///     ...` es una decisión aunque la condición esté adentro;
///   - cada `case_statement_alternative` **salvo `when others`**, que es la
///     rama que queda cuando ninguna se tomó (mismo criterio que el
///     `default:` de C y el `CASE DEFAULT` de Fortran);
///   - cada `exception_handler`, igual que el `catch_clause` de C;
///   - cada `and then` y cada `or else`.
///
/// **Y acá Ada se separa del resto del motor.** `and`/`or` evalúan los dos
/// lados: no hay rama, no hay punto de decisión. Sólo las formas de
/// cortocircuito la crean, y el fuente las distingue — en C `&&` siempre
/// corta y en Fortran `.AND.` siempre cuenta, así que esas cuentas son
/// convención; ésta es exacta.
fn ciclomatica(func: Node) -> u32 {
    fn walk(n: Node, cx: &mut u32) {
        match n.kind() {
            "if_statement" | "elsif_statement_item" | "loop_statement" | "exception_handler" => *cx += 1,
            "case_statement_alternative" if !es_others(n) => *cx += 1,
            "expression" => *cx += cortocircuitos(n),
            _ => {}
        }
        let mut cursor = n.walk();
        for hijo in n.children(&mut cursor) {
            if es_cuerpo_de_subprograma(hijo) {
                continue;
            }
            walk(hijo, cx);
        }
    }
    let mut cx = 1;
    let mut cursor = func.walk();
    for hijo in func.children(&mut cursor) {
        if es_cuerpo_de_subprograma(hijo) {
            continue;
        }
        walk(hijo, &mut cx);
    }
    cx
}

/// `when others =>` en un `case`. El `others` es un token ANÓNIMO dentro del
/// `discrete_choice`, así que un recorrido de nodos nombrados no lo ve — el
/// mismo detalle que el `.AND.` de Fortran.
fn es_others(alt: Node) -> bool {
    let mut encontrado = false;
    tsnode::walk_flat(alt, &mut |n| {
        if n.kind() == "others" {
            encontrado = true;
        }
    });
    encontrado
}

/// Cuenta `and then` y `or else` entre los hijos DIRECTOS de una
/// `expression`.
///
/// Los cuatro operadores son tokens anónimos y la gramática no los agrupa en
/// un nodo propio, así que la forma de cortocircuito se reconoce por
/// adyacencia: `and` seguido de `then`, `or` seguido de `else`. Mirar sólo
/// los hijos directos es lo que evita confundir el `else` de un
/// `if_statement`, que no es hijo de la expresión.
fn cortocircuitos(expr: Node) -> u32 {
    let mut cursor = expr.walk();
    let kinds: Vec<&str> = expr.children(&mut cursor).map(|c| c.kind()).collect();
    kinds
        .windows(2)
        .filter(|w| matches!((w[0], w[1]), ("and", "then") | ("or", "else")))
        .count() as u32
}

/// Llamadas de un subprograma, **sin las de sus subprogramas anidados**.
///
/// **Acá se paga el precio de no usar GNAT.** En Ada `A (I)` es a la vez el
/// indexado de un array y la llamada a una función de un argumento, y la
/// gramática no puede desempatar: las dos son `function_call`. Sin
/// información semántica —que exigiría el compilador instalado— no hay forma
/// de distinguirlas leyendo el texto.
///
/// El daño queda acotado por `graph::call_edges`, que sólo emite una arista
/// si el destino es un subprograma **de este mismo archivo**: un `A (I)`
/// sobre un array produce una arista sólo si en el archivo hay además un
/// subprograma llamado `A`. `un_array_no_inventa_una_arista` lo fija.
fn llamadas(node: Node, source: &str) -> Vec<String> {
    let mut vistas = std::collections::HashSet::new();
    fn rec(n: Node, source: &str, vistas: &mut std::collections::HashSet<String>, es_raiz: bool) {
        if !es_raiz && es_cuerpo_de_subprograma(n) {
            return;
        }
        if matches!(n.kind(), "function_call" | "procedure_call_statement") {
            let mut cursor = n.walk();
            let nombre = n
                .named_children(&mut cursor)
                .find(|c| matches!(c.kind(), "identifier" | "selected_component"))
                .map(|c| text_of(c, source).to_string());
            if let Some(nombre) = nombre {
                vistas.insert(nombre);
            }
        }
        let mut cursor = n.walk();
        for hijo in n.children(&mut cursor) {
            rec(hijo, source, vistas, false);
        }
    }
    rec(node, source, &mut vistas, true);
    let mut out: Vec<String> = vistas.into_iter().collect();
    out.sort();
    out
}

fn infer_big_o(depth: u32) -> (String, String) {
    let razon = match depth {
        0 => "sin bucles de cuenta variable".to_string(),
        1 => "un bucle".to_string(),
        n => format!("{n} bucles anidados"),
    };
    (crate::costo::ccosto::etiqueta(depth), razon)
}

/// Smells estructurales de Ada.
///
/// Los tres que el motor sabe emitir desde una firma y un span, y Ada da los
/// tres: a diferencia de Fortran —donde `excessive_parameters` no está porque
/// `FortranFunction` no guarda los parámetros— acá la `formal_part` está en
/// el árbol y se cuenta por nombre.
///
/// `deep_nesting` se mide sobre `loop_depth`, que cuenta sólo bucles y no
/// bloques `if`/`case`: igual que en Fortran es una cota INFERIOR del
/// anidamiento real, y el mensaje lo dice para que la evidencia no afirme más
/// que lo medido.
fn smells_ada(subs: &[AdaSubprogram]) -> Vec<StructuralSmell> {
    let mut out = Vec::new();
    for f in subs {
        out.extend(smells::check_long_function(&f.name, f.line, f.loc));
        out.extend(smells::check_excessive_parameters(&f.name, f.line, f.params));
        if f.loop_depth > smells::DEEP_NESTING_DEPTH {
            out.push(StructuralSmell {
                kind: "deep_nesting",
                name: f.name.clone(),
                line: f.line,
                message: format!(
                    "{} nested loops (> {}) — counted over loop statements only, not if/case blocks, so the real nesting is at least this deep",
                    f.loop_depth,
                    smells::DEEP_NESTING_DEPTH
                ),
            });
        }
    }
    out.sort_by_key(|s| s.line);
    out
}

fn extraer_subprogramas(root: Node, source: &str) -> Vec<AdaSubprogram> {
    let mut out = Vec::new();
    tsnode::walk_flat(root, &mut |node| {
        if !es_cuerpo_de_subprograma(node) {
            return;
        }
        let mut cursor = node.walk();
        let spec = node
            .named_children(&mut cursor)
            .find(|c| matches!(c.kind(), "function_specification" | "procedure_specification"));
        let Some(spec) = spec else {
            return;
        };
        let kind: &'static str = match node.kind() {
            "expression_function_declaration" => "expression_function",
            "null_procedure_declaration" => "null_procedure",
            _ if spec.kind() == "function_specification" => "function",
            _ => "procedure",
        };
        let start = node.start_position().row + 1;
        let end = node.end_position().row + 1;
        let depth = profundidad_bucles(node);
        let (big_o, big_o_reason) = infer_big_o(depth);
        out.push(AdaSubprogram {
            name: nombre_de_especificacion(spec, source),
            kind,
            line: start,
            end_line: end,
            loc: end - start + 1,
            loop_depth: depth,
            complexity: ciclomatica(node),
            big_o,
            big_o_reason,
            params: cuenta_parametros(spec),
            calls: llamadas(node, source),
        });
    });
    out
}

/// `true` si el nodo abre con la palabra clave `package`.
///
/// `generic_instantiation` es el MISMO nodo para `package X is new Y (...)` y
/// para `function F is new G (...)`, y la diferencia importa: una
/// instanciación de paquete es una unidad, una de subprograma es una firma. La
/// palabra clave que las separa es un token anónimo.
fn abre_con_package(node: Node) -> bool {
    node.child(0).is_some_and(|c| c.kind() == "package")
}

fn extraer_specs(root: Node, source: &str) -> Vec<AdaSpec> {
    let mut out = Vec::new();
    tsnode::walk_flat(root, &mut |node| {
        let kind: &'static str = match node.kind() {
            "subprogram_declaration" => "declaration",
            "entry_declaration" => "entry",
            // `generic ... procedure P (...)` guarda la especificación DIRECTO,
            // sin nodo `subprogram_declaration` en el medio — a diferencia de
            // `generic package`, que sí envuelve un `package_declaration`.
            "generic_subprogram_declaration" => "generic_declaration",
            // Una instanciación de subprograma declara un subprograma sin
            // cuerpo propio: el cuerpo es el del genérico.
            "generic_instantiation" if !abre_con_package(node) => "instantiation",
            _ => return,
        };
        let mut cursor = node.walk();
        let spec = node
            .named_children(&mut cursor)
            .find(|c| matches!(c.kind(), "function_specification" | "procedure_specification"));
        let (nombre, params) = match spec {
            Some(s) => (nombre_de_especificacion(s, source), cuenta_parametros(s)),
            // Un `entry` declara su nombre directo, sin nodo de
            // especificación en el medio.
            None => {
                let mut c2 = node.walk();
                let id = node.named_children(&mut c2).find(|c| c.kind() == "identifier");
                (id.map_or_else(|| "<anonymous>".to_string(), |n| text_of(n, source).to_string()), cuenta_parametros(node))
            }
        };
        out.push(AdaSpec { name: nombre, kind, line: node.start_position().row + 1, params });
    });
    out
}

fn extraer_unidades(root: Node, source: &str) -> Vec<AdaUnit> {
    let mut out = Vec::new();
    tsnode::walk_flat(root, &mut |node| {
        let kind: &'static str = match node.kind() {
            "package_declaration" => "package_declaration",
            "package_body" => "package_body",
            "task_type_declaration" => "task_type",
            "protected_type_declaration" => "protected_type",
            // `package X is new Y (...)`. Sin esto, un archivo que no hace más
            // que instanciar un genérico —y en el corpus medido hay varios—
            // no producía NADA: ni cuerpos, ni firmas, ni unidades.
            "generic_instantiation" if abre_con_package(node) => "package_instantiation",
            // `package Corto renames Muy.Largo.Nombre;`
            "package_renaming_declaration" => "package_renaming",
            _ => return,
        };
        let mut cursor = node.walk();
        let nombre = node
            .named_children(&mut cursor)
            .find(|c| matches!(c.kind(), "identifier" | "selected_component"))
            .map_or_else(|| "<anonymous>".to_string(), |n| text_of(n, source).to_string());
        // `generic package` se parsea como un `generic_package_declaration`
        // que ENVUELVE al `package_declaration`, así que el genérico se
        // detecta mirando al padre.
        let is_generic = node.parent().is_some_and(|p| p.kind() == "generic_package_declaration");
        out.push(AdaUnit {
            name: nombre,
            kind,
            line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            is_generic,
        });
    });
    out
}

fn extraer_withs(root: Node, source: &str) -> Vec<AdaWith> {
    let mut out = Vec::new();
    tsnode::walk_flat(root, &mut |node| {
        if node.kind() != "with_clause" {
            return;
        }
        let mut cursor = node.walk();
        let is_private = node.children(&mut cursor).any(|c| c.kind() == "private");
        let mut c2 = node.walk();
        for hijo in node.named_children(&mut c2) {
            if matches!(hijo.kind(), "identifier" | "selected_component") {
                out.push(AdaWith {
                    module: text_of(hijo, source).to_string(),
                    line: node.start_position().row + 1,
                    is_private,
                    tipo: if is_private { "private_with" } else { "with" },
                });
            }
        }
    });
    out
}

pub fn parse_ada(source: &str) -> Option<AdaParseResult> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_ada::LANGUAGE.into()).ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();

    let functions = extraer_subprogramas(root, source);
    let specs = extraer_specs(root, source);
    let units = extraer_unidades(root, source);
    let imports = extraer_withs(root, source);
    let pares: Vec<(&str, &[String])> =
        functions.iter().map(|f| (f.name.as_str(), f.calls.as_slice())).collect();
    let call_graph = graph::call_edges(&pares);
    let structural_smells = smells_ada(&functions);

    Some(AdaParseResult { functions, specs, units, imports, call_graph, structural_smells })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUERPO: &str = r#"
with Ada.Text_IO;
private with Interno.Detalle;

package body Ejemplo is
   function Suma (A : Arreglo; N : Integer) return Integer is
      T : Integer := 0;
   begin
      for I in 1 .. N loop
         for J in 1 .. N loop
            T := T + 1;
         end loop;
      end loop;
      return T;
   end Suma;

   procedure Vacio (X : in out Integer) is
   begin
      Otro_Proc (X);
      X := Suma (Nada, 1);
   end Vacio;
end Ejemplo;
"#;

    #[test]
    fn la_gramatica_parsea_ada_real_sin_errores() {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_ada::LANGUAGE.into()).expect("ABI incompatible");
        let t = p.parse(CUERPO, None).unwrap();
        assert!(!t.root_node().has_error());
    }

    #[test]
    fn encuentra_funcion_y_procedimiento_con_su_tipo() {
        let r = parse_ada(CUERPO).unwrap();
        let nombres: Vec<(&str, &str)> =
            r.functions.iter().map(|f| (f.name.as_str(), f.kind)).collect();
        assert_eq!(nombres, vec![("Suma", "function"), ("Vacio", "procedure")]);
    }

    #[test]
    fn dos_bucles_anidados_dan_o_n_cuadrado() {
        let r = parse_ada(CUERPO).unwrap();
        let suma = r.functions.iter().find(|f| f.name == "Suma").unwrap();
        assert_eq!(suma.loop_depth, 2);
        assert_eq!(suma.big_o, "O(n²)");
    }

    #[test]
    fn las_clausulas_with_se_separan_de_las_privadas() {
        let r = parse_ada(CUERPO).unwrap();
        let publicas: Vec<&str> =
            r.imports.iter().filter(|w| !w.is_private).map(|w| w.module.as_str()).collect();
        let privadas: Vec<&str> =
            r.imports.iter().filter(|w| w.is_private).map(|w| w.module.as_str()).collect();
        assert_eq!(r.imports.iter().find(|w| w.is_private).map(|w| w.tipo), Some("private_with"));
        assert_eq!(publicas, vec!["Ada.Text_IO"]);
        assert_eq!(privadas, vec!["Interno.Detalle"], "`private with` no se propaga a los clientes");
    }

    #[test]
    fn el_grafo_de_llamadas_une_subprogramas_del_mismo_archivo() {
        let r = parse_ada(CUERPO).unwrap();
        assert!(r.call_graph.iter().any(|e| e.from == "Vacio" && e.to == "Suma"));
        // `Otro_Proc` no está en el archivo: no es una arista del grafo local.
        assert!(!r.call_graph.iter().any(|e| e.to == "Otro_Proc"));
    }

    /// Un `.ads` no tiene cuerpos. Emitir `functions` vacío y las firmas en
    /// `specs` es la verdad sobre ese archivo; darles `loc` y Big-O a partir
    /// de la firma sería reportar métricas de un cuerpo que no se leyó.
    #[test]
    fn una_especificacion_da_specs_y_cero_funciones() {
        let src = "package Pila is\n   function Vacia (C : Contenedor) return Boolean;\n   procedure Empujar (C : in out Contenedor; E : Elemento);\nend Pila;\n";
        let r = parse_ada(src).unwrap();
        assert!(r.functions.is_empty(), "{:?}", r.functions.len());
        let nombres: Vec<&str> = r.specs.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(nombres, vec!["Vacia", "Empujar"]);
        assert_eq!(r.units.len(), 1);
        assert_eq!(r.units[0].kind, "package_declaration");
    }

    /// La forma `is (expresión)` de Ada 2012 es un cuerpo de verdad aunque no
    /// tenga `begin`.
    #[test]
    fn una_funcion_expresion_cuenta_como_cuerpo() {
        let src = "package body P is\n   function Doble (X : Integer) return Integer is (X * 2);\nend P;\n";
        let r = parse_ada(src).unwrap();
        assert_eq!(r.functions.len(), 1);
        assert_eq!(r.functions[0].kind, "expression_function");
        assert_eq!(r.functions[0].complexity, 1);
    }

    /// **La diferencia que aporta Ada.** `and`/`or` evalúan los dos lados: no
    /// hay rama. Sólo `and then`/`or else` cortan, y el fuente los distingue
    /// — en C `&&` siempre corta y en Fortran `.AND.` siempre cuenta, así que
    /// esas cuentas son convención; ésta es exacta.
    #[test]
    fn solo_los_cortocircuitos_cuentan() {
        let corto = "package body P is\n   procedure Q (A, B : Boolean) is\n   begin\n      if A and then B then\n         null;\n      end if;\n   end Q;\nend P;\n";
        let largo = "package body P is\n   procedure Q (A, B : Boolean) is\n   begin\n      if A and B then\n         null;\n      end if;\n   end Q;\nend P;\n";
        let c = parse_ada(corto).unwrap();
        let l = parse_ada(largo).unwrap();
        assert_eq!(c.functions[0].complexity, 3, "1 + el if + el `and then`");
        assert_eq!(l.functions[0].complexity, 2, "1 + el if; el `and` pelado no es una rama");
    }

    /// `elsif` es un nodo propio en Ada (como el `elseif_clause` de Fortran y
    /// a diferencia del `elif` de Python, que es un `If` anidado). El `else`
    /// pelado no es una decisión.
    #[test]
    fn cada_elsif_cuenta_y_el_else_pelado_no() {
        let src = "package body P is\n   procedure Q (X : Integer) is\n   begin\n      if X > 9 then\n         null;\n      elsif X > 5 then\n         null;\n      else\n         null;\n      end if;\n   end Q;\nend P;\n";
        let r = parse_ada(src).unwrap();
        assert_eq!(r.functions[0].complexity, 3, "1 + if + elsif; el else no suma");
    }

    /// `when others` es la rama que queda cuando ninguna se tomó, y su
    /// `others` es un token ANÓNIMO — mismo criterio que el `default:` de C y
    /// el `CASE DEFAULT` de Fortran.
    #[test]
    fn el_when_others_no_cuenta_como_alternativa() {
        let src = "package body P is\n   procedure Q (X : Integer) is\n   begin\n      case X is\n         when 0 => null;\n         when 1 => null;\n         when others => null;\n      end case;\n   end Q;\nend P;\n";
        let r = parse_ada(src).unwrap();
        assert_eq!(r.functions[0].complexity, 3, "1 + 2 alternativas con valor; others no suma");
    }

    /// Un manejador de excepción es una rama, igual que el `catch_clause` de
    /// C.
    #[test]
    fn cada_manejador_de_excepcion_es_una_rama() {
        let src = "package body P is\n   procedure Q is\n   begin\n      null;\n   exception\n      when Constraint_Error => null;\n      when others => null;\n   end Q;\nend P;\n";
        let r = parse_ada(src).unwrap();
        assert_eq!(r.functions[0].complexity, 3, "1 + los 2 manejadores");
    }

    /// **La misma regla que `bigo::vueltas_constantes` aplica en Python.** Un
    /// bucle con cuenta de vueltas literal corre lo mismo para cualquier
    /// entrada: no agrega profundidad asintótica.
    #[test]
    fn un_bucle_de_cuenta_literal_no_agrega_profundidad() {
        let src = "package body P is\n   procedure Q (N : Integer) is\n   begin\n      for I in 1 .. 10 loop\n         for J in 1 .. N loop\n            null;\n         end loop;\n      end loop;\n   end Q;\nend P;\n";
        let r = parse_ada(src).unwrap();
        assert_eq!(r.functions[0].loop_depth, 1, "el bucle de 1..10 no cuenta");
        assert_eq!(r.functions[0].big_o, "O(n)");
        // Pero sí es un punto de decisión: se ejecuta o no se ejecuta.
        assert_eq!(r.functions[0].complexity, 3);
    }

    /// Un `while` NO entra en esa regla: su cantidad de vueltas depende del
    /// estado, y afirmar que es constante requeriría probar la terminación —
    /// lo que `decidibilidad.rs` clasifica como no decidible.
    #[test]
    fn un_while_siempre_agrega_profundidad() {
        let src = "package body P is\n   procedure Q (N : in out Integer) is\n   begin\n      while N > 0 loop\n         N := N - 1;\n      end loop;\n   end Q;\nend P;\n";
        let r = parse_ada(src).unwrap();
        assert_eq!(r.functions[0].loop_depth, 1);
        assert_eq!(r.functions[0].big_o, "O(n)");
    }

    /// Un subprograma anidado tiene su propia complejidad, su propia
    /// profundidad y sus propias llamadas — igual que una función anidada en
    /// C en `ccosto::ciclomatica`.
    #[test]
    fn un_subprograma_anidado_no_suma_al_de_afuera() {
        let src = "package body P is\n   procedure Afuera (N : Integer) is\n      procedure Adentro is\n      begin\n         for I in 1 .. N loop\n            null;\n         end loop;\n      end Adentro;\n   begin\n      Adentro;\n   end Afuera;\nend P;\n";
        let r = parse_ada(src).unwrap();
        let afuera = r.functions.iter().find(|f| f.name == "Afuera").unwrap();
        let adentro = r.functions.iter().find(|f| f.name == "Adentro").unwrap();
        assert_eq!(afuera.loop_depth, 0, "el bucle es de Adentro");
        assert_eq!(afuera.complexity, 1);
        assert_eq!(adentro.loop_depth, 1);
        assert!(afuera.calls.contains(&"Adentro".to_string()));
    }

    /// `(A, B : Integer)` es UNA `parameter_specification` con DOS
    /// parámetros. Contar declaraciones en vez de nombres daría 1.
    #[test]
    fn los_parametros_se_cuentan_por_nombre_no_por_declaracion() {
        let src = "package body P is\n   procedure Q (A, B : Integer; C : Float) is\n   begin\n      null;\n   end Q;\nend P;\n";
        let r = parse_ada(src).unwrap();
        assert_eq!(r.functions[0].params, 3);
    }

    /// A diferencia de Fortran, Ada sí da los parámetros en el árbol, así que
    /// el tercer smell estructural del motor también aplica.
    #[test]
    fn demasiados_parametros_se_marca() {
        let src = "package body P is\n   procedure Q (A, B, C, D, E, F : Integer) is\n   begin\n      null;\n   end Q;\nend P;\n";
        let r = parse_ada(src).unwrap();
        assert!(r.structural_smells.iter().any(|s| s.kind == "excessive_parameters"), "{:?}", r.structural_smells.len());
    }

    #[test]
    fn un_subprograma_largo_se_marca() {
        let cuerpo = "      null;\n".repeat(60);
        let src = format!("package body P is\n   procedure Larga is\n   begin\n{cuerpo}   end Larga;\nend P;\n");
        let r = parse_ada(&src).unwrap();
        assert!(r.structural_smells.iter().any(|s| s.kind == "long_function"));
    }

    #[test]
    fn cinco_bucles_anidados_se_marcan_y_el_mensaje_dice_que_se_conto() {
        let src = "package body P is\n   procedure Q (N : Integer) is\n   begin\n      for A in 1 .. N loop\n       for B in 1 .. N loop\n        for C in 1 .. N loop\n         for D in 1 .. N loop\n          for E in 1 .. N loop\n           null;\n          end loop;\n         end loop;\n        end loop;\n       end loop;\n      end loop;\n   end Q;\nend P;\n";
        let r = parse_ada(src).unwrap();
        let hit = r.structural_smells.iter().find(|s| s.kind == "deep_nesting").expect("sin hallazgo");
        assert!(hit.message.contains("loop statements only"), "{}", hit.message);
    }

    /// **El precio documentado de no exigir GNAT.** En Ada `A (I)` es a la
    /// vez indexado de array y llamada de un argumento, y la gramática no
    /// desempata. El daño queda acotado por `graph::call_edges`, que sólo
    /// emite aristas hacia subprogramas de este archivo.
    #[test]
    fn un_array_no_inventa_una_arista() {
        let src = "package body P is\n   procedure Q (A : Arreglo; N : Integer) is\n      T : Integer := 0;\n   begin\n      T := A (N);\n   end Q;\nend P;\n";
        let r = parse_ada(src).unwrap();
        // El indexado SÍ aparece como llamada: se dice, no se esconde.
        assert!(r.functions[0].calls.contains(&"A".to_string()));
        // Pero no hay ningún subprograma `A` en el archivo, así que no hay
        // arista.
        assert!(r.call_graph.is_empty());
    }

    #[test]
    fn un_paquete_generico_se_marca_como_generico() {
        let src = "generic\n   type Elemento is private;\npackage Pila is\n   procedure Nada;\nend Pila;\n";
        let r = parse_ada(src).unwrap();
        let u = r.units.iter().find(|u| u.kind == "package_declaration").expect("sin paquete");
        assert!(u.is_generic, "un generico se prueba por instancia; importa para SPARK");
    }

    #[test]
    fn las_tareas_y_los_tipos_protegidos_son_unidades() {
        let src = "package P is\n   task type Trabajador is\n      entry Arrancar;\n   end Trabajador;\n   protected type Candado is\n      procedure Tomar;\n   end Candado;\nend P;\n";
        let r = parse_ada(src).unwrap();
        let kinds: Vec<&str> = r.units.iter().map(|u| u.kind).collect();
        assert!(kinds.contains(&"task_type"), "{kinds:?}");
        assert!(kinds.contains(&"protected_type"), "{kinds:?}");
        assert!(r.specs.iter().any(|s| s.kind == "entry" && s.name == "Arrancar"), "{:?}", r.specs.len());
    }

    // ── Lo que encontró la medición contra 2.718 archivos Ada reales ──

    /// **111 de 9.970 cuerpos salían como `<anonymous>`, y todos eran esto.**
    /// Ada escribe el operador entre comillas, así que la gramática lo
    /// entrega como `string_literal` y no como identificador. No es un caso
    /// raro: es idiomático.
    #[test]
    fn una_sobrecarga_de_operador_tiene_nombre() {
        let src = "package body P is\n   function \"=\" (L, R : T) return Boolean is\n   begin\n      return True;\n   end \"=\";\nend P;\n";
        let r = parse_ada(src).unwrap();
        assert_eq!(r.functions.len(), 1);
        assert_eq!(r.functions[0].name, "\"=\"");
        assert_eq!(r.functions[0].params, 2, "`(L, R : T)` son dos parámetros");
    }

    /// **Un archivo que no hacía más que instanciar un genérico producía
    /// NADA**: ni cuerpos, ni firmas, ni unidades. En el corpus medido hay
    /// varios así, y un `.ads` de una línea que instancia un contenedor es
    /// una unidad de compilación como cualquier otra.
    #[test]
    fn una_instanciacion_de_paquete_es_una_unidad() {
        let src = "with Ada.Containers.Hashed_Sets;\npackage Conjunto is new Ada.Containers.Hashed_Sets (Integer, Hash);\n";
        let r = parse_ada(src).unwrap();
        assert_eq!(r.units.len(), 1, "{:?}", r.units.iter().map(|u| u.kind).collect::<Vec<_>>());
        assert_eq!(r.units[0].kind, "package_instantiation");
        assert_eq!(r.units[0].name, "Conjunto");
    }

    /// `generic_instantiation` es el MISMO nodo para una instanciación de
    /// paquete y una de subprograma; la palabra clave que las separa es un
    /// token anónimo. Una de subprograma no es una unidad: es una firma, y su
    /// cuerpo es el del genérico.
    #[test]
    fn una_instanciacion_de_subprograma_es_una_firma_no_una_unidad() {
        let src = "function Ordenar_Enteros is new Ordenar (Integer);\n";
        let r = parse_ada(src).unwrap();
        assert!(r.units.is_empty(), "{:?}", r.units.iter().map(|u| u.kind).collect::<Vec<_>>());
        assert_eq!(r.specs.len(), 1);
        assert_eq!(r.specs[0].kind, "instantiation");
        assert_eq!(r.specs[0].name, "Ordenar_Enteros");
    }

    #[test]
    fn un_renombre_de_paquete_es_una_unidad() {
        let r = parse_ada("package Corto renames Muy.Largo.Nombre;\n").unwrap();
        assert_eq!(r.units.len(), 1);
        assert_eq!(r.units[0].kind, "package_renaming");
        assert_eq!(r.units[0].name, "Corto");
    }

    /// `generic ... procedure P (...)` guarda la especificación DIRECTO, sin
    /// nodo `subprogram_declaration` en el medio — a diferencia de `generic
    /// package`, que sí envuelve un `package_declaration`. La asimetría de la
    /// gramática hacía que estas declaraciones se perdieran enteras.
    #[test]
    fn un_generico_de_subprograma_da_una_firma() {
        let src = "generic\n   type T is private;\nprocedure Ordenar (X : T);\n";
        let r = parse_ada(src).unwrap();
        assert_eq!(r.specs.len(), 1, "{:?}", r.specs.iter().map(|x| x.kind).collect::<Vec<_>>());
        assert_eq!(r.specs[0].kind, "generic_declaration");
        assert_eq!(r.specs[0].name, "Ordenar");
    }

    /// **Este test nació afirmando lo contrario y se dio vuelta.** Decía que
    /// `procedure P is null;` le producía un `ERROR` a la gramática — el
    /// fixture del corpus donde lo vi tenía un `ERROR`, sí, pero por otra
    /// cosa (`type Enum (A, B);`, que es Ada inválido). La gramática lo
    /// parsea perfecto, con nodo propio, y el parser lo estaba tirando en
    /// silencio: ni cuerpo, ni firma, ni nada.
    #[test]
    fn un_procedimiento_nulo_es_un_cuerpo() {
        let src = "package P is\n   procedure Nada (X : Integer) is null;\nend P;\n";
        let mut p = Parser::new();
        p.set_language(&tree_sitter_ada::LANGUAGE.into()).unwrap();
        assert!(!p.parse(src, None).unwrap().root_node().has_error(), "la gramática SÍ lo soporta");
        let r = parse_ada(src).unwrap();
        assert_eq!(r.functions.len(), 1, "un cuerpo que no hace nada sigue siendo un cuerpo");
        assert_eq!(r.functions[0].kind, "null_procedure");
        assert_eq!(r.functions[0].name, "Nada");
        assert_eq!(r.functions[0].params, 1);
        assert_eq!(r.functions[0].complexity, 1);
    }

    /// Un fuente que no es Ada no debe reventar ni inventar resultados.
    #[test]
    fn un_fuente_que_no_es_ada_no_revienta() {
        let r = parse_ada("def f(x):\n    return x + 1\n").unwrap();
        assert!(r.functions.is_empty());
        assert!(r.units.is_empty());
    }






}
