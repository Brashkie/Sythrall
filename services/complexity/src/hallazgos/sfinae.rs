//! **SFINAE contra *concepts*: la misma restricción, veinte años de
//! distancia.**
//!
//! Las dos formas responden a la misma pregunta —*"¿para qué tipos vale esta
//! plantilla?"*— y la diferencia no es de estilo:
//!
//! ```cpp
//! // Pre-C++20: la condición viaja escondida en un parámetro que no se usa.
//! template<typename T, typename = std::enable_if_t<std::is_integral_v<T>>>
//! void f(T x);
//!
//! // C++20: la condición ES la firma.
//! template<std::integral T>
//! void f(T x);
//! ```
//!
//! **SFINAE no expresa la restricción: la esconde.** Funciona por un efecto
//! lateral del lenguaje — *Substitution Failure Is Not An Error* — así que
//! un tipo que no cumple hace que la sobrecarga **desaparezca en silencio**,
//! y el error que ve quien la usa es *"no matching function"*, sin decir qué
//! faltaba. Un `requires` o un concept dicen la condición y el compilador la
//! cita cuando falla.
//!
//! **Y por eso el hallazgo NO dice que SFINAE esté mal.** Si el proyecto
//! apunta a C++17 o anterior, es la única forma de escribirlo — y **el motor
//! no puede saber a qué estándar apunta el proyecto**: eso vive en el sistema
//! de build, no en el archivo. Es el mismo límite que `pymodern.rs` declara
//! sobre la versión de Python objetivo. El hallazgo señala la alternativa y
//! dice cuándo no aplica, en vez de pedir un cambio a ciegas.

use serde::Serialize;
use tree_sitter::Node;

use crate::hallazgos::smells::StructuralSmell;
use crate::ast::tsnode::{self, text_of};

/// Cuentas de cómo este archivo restringe sus plantillas.
#[derive(Serialize, Clone, Copy, Default, Debug)]
pub struct ConstraintMetrics {
    /// `concept X = ...;` — definiciones propias (C++20).
    pub concepts_defined: usize,
    /// Cláusulas `requires` (C++20).
    pub requires_clauses: usize,
    /// Sitios donde aparece `enable_if`/`enable_if_t` — la forma legacy.
    pub enable_if_sites: usize,
    /// `decltype(...)` en un tipo de retorno trailing. No es SFINAE por sí
    /// solo: es deducción de tipo, y se cuenta aparte justamente para no
    /// mezclarlo con la restricción.
    pub decltype_returns: usize,
}

fn menciona_enable_if(n: Node, source: &str) -> bool {
    text_of(n, source).contains("enable_if")
}

/// Los lugares donde SFINAE se escribe de verdad, en orden de frecuencia:
/// como parámetro de plantilla por defecto, y como tipo de retorno trailing.
const SITIOS_SFINAE: &[&str] = &["optional_type_parameter_declaration", "trailing_return_type"];

pub fn analyze(root: Node, source: &str, is_cpp: bool) -> (ConstraintMetrics, Vec<StructuralSmell>) {
    let mut m = ConstraintMetrics::default();
    let mut smells = Vec::new();
    if !is_cpp {
        return (m, smells);
    }

    tsnode::walk_flat(root, &mut |n| {
        match n.kind() {
            "concept_definition" => m.concepts_defined += 1,
            "requires_clause" => m.requires_clauses += 1,
            // `-> decltype(a + b)` es deducción, no restricción. Se cuenta
            // aparte para no inflar el número de SFINAE con algo que no lo es.
            "trailing_return_type"
                if text_of(n, source).contains("decltype") && !menciona_enable_if(n, source) =>
            {
                m.decltype_returns += 1;
            }
            _ => {}
        }
        if SITIOS_SFINAE.contains(&n.kind()) && menciona_enable_if(n, source) {
            m.enable_if_sites += 1;
            smells.push(StructuralSmell {
                kind: "sfinae_instead_of_concepts",
                name: nombre_de_la_plantilla(n, source),
                line: n.start_position().row + 1,
                message:
                    "This constraint is written with `enable_if` (SFINAE), the pre-C++20 form. \
                     SFINAE does not express the condition — it HIDES it: it works through a \
                     side effect of the language (Substitution Failure Is Not An Error), so a \
                     type that does not satisfy it makes the overload vanish silently, and the \
                     caller's error reads `no matching function`, never saying what was missing. \
                     A `requires` clause or a named concept states the condition, and the \
                     compiler quotes it back when it fails. **This does NOT mean the code is \
                     wrong**: targeting C++17 or earlier, `enable_if` is the only way to write \
                     it — and this engine cannot know which standard the project targets, \
                     because that lives in the build system, not in the file."
                        .to_string(),
            });
        }
    });
    (m, smells)
}

/// El nombre de la plantilla a la que pertenece esta restricción, subiendo
/// hasta la declaración que la contiene. Sin él, el hallazgo diría solo el
/// número de línea, que en un header lleno de sobrecargas no alcanza.
fn nombre_de_la_plantilla(n: Node, source: &str) -> String {
    let mut actual = n.parent();
    while let Some(nodo) = actual {
        if matches!(nodo.kind(), "function_definition" | "declaration") {
            if let Some(nombre) = primer_declarador(nodo, source) {
                return nombre;
            }
        }
        // Un `template_declaration` NO tiene declarador propio: la función
        // que declara es su HIJA. Subir hasta él y pedirle el declarador
        // devuelve nada — la restricción vive en su lista de parámetros, que
        // es hermana de la función, no su ancestro.
        if nodo.kind() == "template_declaration" {
            let mut c = nodo.walk();
            let hijos: Vec<Node> = nodo.children(&mut c).collect();
            for hijo in hijos {
                if matches!(hijo.kind(), "function_definition" | "declaration") {
                    if let Some(nombre) = primer_declarador(hijo, source) {
                        return nombre;
                    }
                }
            }
        }
        actual = nodo.parent();
    }
    "<plantilla>".to_string()
}

fn primer_declarador(n: Node, source: &str) -> Option<String> {
    let d = n.child_by_field_name("declarator")?;
    fn identificador(n: Node, source: &str) -> Option<String> {
        if n.kind() == "identifier" || n.kind() == "field_identifier" {
            return Some(text_of(n, source).to_string());
        }
        let mut c = n.walk();
        let hallado = n.children(&mut c).find_map(|h| identificador(h, source));
        hallado
    }
    identificador(d, source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn analizar(src: &str) -> (ConstraintMetrics, Vec<StructuralSmell>) {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_cpp::LANGUAGE.into()).unwrap();
        let t = p.parse(src, None).unwrap();
        analyze(t.root_node(), src, true)
    }

    /// La forma legacy más común: la condición escondida en un parámetro de
    /// plantilla por defecto que nadie usa.
    #[test]
    fn enable_if_en_un_parametro_por_defecto_se_marca() {
        let src = "template<typename T, typename = std::enable_if_t<std::is_integral_v<T>>>\nvoid f(T x) {}\n";
        let (m, s) = analizar(src);
        assert_eq!(m.enable_if_sites, 1);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].kind, "sfinae_instead_of_concepts");
        assert_eq!(s[0].name, "f", "el hallazgo cita la plantilla, no solo la línea");
    }

    /// La otra forma clásica: en el tipo de retorno trailing.
    #[test]
    fn enable_if_en_el_retorno_trailing_tambien() {
        let src = "template<typename T>\nauto g(T x) -> typename std::enable_if<std::is_integral<T>::value, void>::type {}\n";
        let (m, s) = analizar(src);
        assert_eq!(m.enable_if_sites, 1);
        assert_eq!(s.len(), 1);
    }

    /// **La contraparte que evita que esto sea ruido**: un `requires` o un
    /// concept ya expresan la condición, así que no se marcan.
    #[test]
    fn un_requires_no_se_marca() {
        let src = "template<typename T>\nvoid f(T x) requires std::is_integral_v<T> {}\n";
        let (m, s) = analizar(src);
        assert_eq!(m.requires_clauses, 1);
        assert!(s.is_empty(), "la forma moderna no se pide cambiar");
    }

    #[test]
    fn un_concept_propio_se_cuenta_y_no_se_marca() {
        let src = "template<typename T>\nconcept Entero = std::is_integral_v<T>;\n";
        let (m, s) = analizar(src);
        assert_eq!(m.concepts_defined, 1);
        assert!(s.is_empty());
    }

    /// **`decltype` en el retorno NO es SFINAE**: es deducción de tipo. Se
    /// cuenta aparte justamente para no inflar el conteo de restricciones
    /// con algo que no restringe nada.
    #[test]
    fn decltype_en_el_retorno_es_deduccion_no_restriccion() {
        let src = "template<typename T>\nauto suma(T a, T b) -> decltype(a + b) { return a + b; }\n";
        let (m, s) = analizar(src);
        assert_eq!(m.decltype_returns, 1);
        assert_eq!(m.enable_if_sites, 0);
        assert!(s.is_empty());
    }

    /// El mensaje tiene que decir las DOS cosas: que hay una alternativa, y
    /// que puede no aplicar porque el motor no sabe a qué estándar se apunta.
    #[test]
    fn el_mensaje_ofrece_la_alternativa_y_dice_cuando_no_aplica() {
        let src = "template<typename T, typename = std::enable_if_t<std::is_integral_v<T>>>\nvoid f(T x) {}\n";
        let m = &analizar(src).1[0].message;
        assert!(m.contains("requires"), "da la alternativa: {m}");
        assert!(m.contains("does NOT mean the code is wrong"), "y no acusa: {m}");
        assert!(m.contains("C++17 or earlier"), "dice cuándo no aplica: {m}");
        assert!(m.contains("build system"), "y por qué el motor no puede saberlo: {m}");
    }

    /// C no tiene nada de esto: se devuelve vacío sin recorrer el árbol.
    #[test]
    fn en_c_no_se_analiza_nada() {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_cpp::LANGUAGE.into()).unwrap();
        let src = "template<typename T, typename = std::enable_if_t<int>>\nvoid f(T x) {}\n";
        let t = p.parse(src, None).unwrap();
        let (m, s) = analyze(t.root_node(), src, false);
        assert_eq!(m.enable_if_sites, 0);
        assert!(s.is_empty());
    }

    #[test]
    fn un_archivo_sin_plantillas_no_produce_nada() {
        let (m, s) = analizar("int main() { return 0; }\n");
        assert_eq!(m.concepts_defined, 0);
        assert_eq!(m.enable_if_sites, 0);
        assert!(s.is_empty());
    }
}
