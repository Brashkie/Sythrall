//! Fase 34, Línea A — costo de compilación de la metaprogramación de C++,
//! primera porción.
//!
//! **Un código puede ser perfecto y rapidísimo en runtime y aun así hacer que
//! el compilador instancie cantidades enormes de tipos.** Casi ninguna
//! herramienta mide eso, y es un costo real: minutos de build, gigabytes de
//! RAM del compilador, y a veces un error duro cuando se pasa el límite de
//! profundidad de instanciación.
//!
//! **La separación que ordena todo el módulo: lo estático da la FORMA, medir
//! da el COSTO.** Eso no es prudencia, es lo que cada lado puede saber:
//!
//!   - **Exacto desde el fuente**: qué templates se instancian a sí mismos, si
//!     tienen caso base a la vista, con qué profundidad literal se los invoca,
//!     cuántos sitios de instanciación hay, cuántas ramas de `if constexpr`.
//!   - **Requiere compilar**: cuántas instanciaciones se crean de verdad,
//!     cuántos símbolos se generan, cuánta memoria y cuánto tiempo usa el
//!     compilador.
//!
//! Este módulo hace **solo la primera columna**, y nunca afirma nada de la
//! segunda. Un número como "12.430 símbolos generados" no se puede deducir
//! leyendo: para saberlo hay que instanciar, o sea compilar. Misma línea que
//! ya trazan `colas.rs` con `maxsize=cfg.tam` y `cripto.rs` con
//! `RSA.generate(cfg.bits)`: un valor que no es literal no se juzga.
//!
//! **La segunda columna tiene su propio camino, y ya existe la maquinaria**:
//! la Fase 23 compila y cronometra código real (`fortran_bench` compila un
//! kernel con gfortran y ajusta el exponente medido). Medir el costo de
//! compilación es esa misma máquina apuntada a otro lado — porción siguiente,
//! no ésta.
//!
//! **Alcance: solo C++.** C no tiene templates, así que para C esto devuelve
//! vacío sin recorrer nada.
//!
//! **Una nota sobre la gramática, porque casi me cuesta un análisis mudo:**
//! en tree-sitter-cpp el `constexpr` de un `if constexpr` es un token
//! **anónimo** dentro del `if_statement`. Un recorrido que mire solo nodos
//! nombrados —lo natural— no lo ve NUNCA, y el detector encontraría cero sin
//! fallar. Por eso acá se miran los hijos anónimos explícitamente.

use serde::Serialize;
use tree_sitter::Node;

use crate::hallazgos::smells::StructuralSmell;
use crate::ast::tsnode;

/// A partir de acá la recursión de templates deja de ser un detalle: cada
/// nivel es un tipo distinto que el compilador tiene que crear, con su propio
/// nombre mangled y su propia entrada en las tablas internas.
///
/// No es el límite duro: gcc corta por defecto en ~900 niveles
/// (`-ftemplate-depth`) y clang en ~1024. Este umbral está mucho más abajo
/// porque el costo ya se nota bastante antes de que el compilador se plante.
const PROFUNDIDAD_NOTABLE: i64 = 64;

/// El nombre de lo que declara un `template_declaration`, y si lo que declara
/// es una ESPECIALIZACIÓN (`template<> struct X<0>`) en vez de la plantilla
/// primaria. Las dos cosas salen del mismo lugar, así que se leen juntas.
fn declarado_por(td: Node, source: &str) -> Option<(String, bool)> {
    let mut cursor = td.walk();
    for hijo in td.named_children(&mut cursor) {
        match hijo.kind() {
            "struct_specifier" | "class_specifier" | "union_specifier" => {
                let mut c2 = hijo.walk();
                for n in hijo.named_children(&mut c2) {
                    // `struct X` primario contra `struct X<0>` especializado.
                    if n.kind() == "type_identifier" {
                        return Some((tsnode::text_of(n, source).to_string(), false));
                    }
                    if n.kind() == "template_type" {
                        let nombre = n.child_by_field_name("name")?;
                        return Some((tsnode::text_of(nombre, source).to_string(), true));
                    }
                }
            }
            "function_definition" | "declaration" => {
                let nombre = tsnode::function_definition_name(hijo, source);
                if nombre != "<anonymous>" {
                    return Some((nombre, false));
                }
            }
            _ => {}
        }
    }
    None
}

/// Sitios de instanciación de `nombre`: cada `template_type` que lo usa, con
/// su argumento literal si lo tiene.
fn instanciaciones<'a>(raiz: Node<'a>, nombre: &str, source: &str) -> Vec<(Node<'a>, Option<i64>)> {
    let mut out = Vec::new();
    tsnode::walk_flat(raiz, &mut |n| {
        if n.kind() != "template_type" {
            return;
        }
        let Some(id) = n.child_by_field_name("name") else { return };
        if tsnode::text_of(id, source) != nombre {
            return;
        }
        // `named_child(0)` en vez de un iterador con cursor: el iterador
        // toma prestado el cursor y vive hasta el final del bloque, así que
        // en un `if let` el préstamo sobrevive al cursor local.
        let literal = n
            .child_by_field_name("arguments")
            .and_then(|args| args.named_child(0))
            .filter(|a| a.kind() == "number_literal")
            .and_then(|a| tsnode::text_of(a, source).parse::<i64>().ok());
        out.push((n, literal));
    });
    out
}

/// `true` si el cuerpo del template tiene alguna forma de cortar la recursión
/// que NO sea una especialización aparte.
///
/// `if constexpr` es la manera moderna (C++17) de escribir el caso base sin
/// una especialización, y `enable_if`/`requires`/`concept` también cortan por
/// SFINAE o por restricciones. Si aparece cualquiera de ellas, este módulo se
/// calla: afirmar "no tiene caso base" con una de esas a la vista sería falso.
fn corta_por_dentro(td: Node, source: &str) -> bool {
    let mut corta = false;
    tsnode::walk_flat(td, &mut |n| {
        if corta {
            return;
        }
        // Ver el doc del módulo: el `constexpr` de un `if constexpr` es un
        // token ANÓNIMO, así que hay que mirar todos los hijos, no los
        // nombrados.
        if n.kind() == "if_statement" {
            let mut c = n.walk();
            if n.children(&mut c).any(|h| h.kind() == "constexpr") {
                corta = true;
                return;
            }
        }
        if matches!(n.kind(), "identifier" | "type_identifier" | "qualified_identifier") {
            let t = tsnode::text_of(n, source);
            if t.contains("enable_if") || t == "requires" || t.contains("void_t") {
                corta = true;
            }
        }
        if n.kind() == "requires_clause" || n.kind() == "concept_definition" {
            corta = true;
        }
    });
    corta
}


/// **Métricas de FORMA de la metaprogramación de un archivo.** Cuánto hay
/// escrito, no cuánto trabajo le da al compilador — la distinción que ordena
/// el módulo entero.
///
/// **Por qué estos números NO predicen el costo de compilar, dicho acá para
/// que nadie los lea como si lo hicieran:**
///   - `instantiation_sites` cuenta **usos escritos**. `vector<int>` usado 50
///     veces es 50 sitios y **una sola** instanciación para el compilador, que
///     deduplica. Sobrecuenta.
///   - `distinct_instantiations` deduplica por texto, así que se acerca — pero
///     `Factorial<N - 1>` es **un** texto y **N** instanciaciones reales.
///     Subcuenta, y justo en el caso que más cuesta.
///
/// O sea: los dos se equivocan, en direcciones opuestas, y por eso están los
/// dos. Saber el número real exige instanciar, o sea compilar. Los templates
/// recursivos van aparte en `recursive_templates` precisamente porque son
/// donde "lo escrito" y "lo generado" más se separan.
#[derive(Serialize, Clone, Default)]
pub struct TemplateComplexity {
    /// Plantillas primarias declaradas.
    pub templates: usize,
    /// Especializaciones, totales o parciales (`template<> struct X<0>`).
    pub specializations: usize,
    /// Cada `X<...>` escrito en el archivo.
    pub instantiation_sites: usize,
    /// Los anteriores deduplicados por texto.
    pub distinct_instantiations: usize,
    /// Ramas `if constexpr` — bifurcación resuelta en compilación.
    pub constexpr_branches: usize,
    /// `static_assert` — evaluación obligatoria en compilación.
    pub static_asserts: usize,
    /// Plantillas que se instancian a sí mismas.
    pub recursive_templates: Vec<String>,
    /// **Cómo se restringen las plantillas de este archivo.** Va acá y no
    /// en una clave nueva porque es la misma pregunta que el resto del
    /// struct responde —qué hace este archivo en tiempo de compilación—
    /// y comparte el panel. Ver `sfinae.rs`.
    #[serde(flatten)]
    pub constraints: crate::hallazgos::sfinae::ConstraintMetrics,
}

/// Ver `TemplateComplexity`. Devuelve el default para C, que no tiene
/// templates: recorrer sería trabajo puro sin resultado.
pub fn template_complexity(root: Node, source: &str, is_cpp: bool) -> TemplateComplexity {
    let mut m = TemplateComplexity::default();
    if !is_cpp {
        return m;
    }

    let mut primarias: Vec<(Node, String)> = Vec::new();
    let mut textos: std::collections::HashSet<String> = std::collections::HashSet::new();
    tsnode::walk_flat(root, &mut |n| {
        match n.kind() {
            "template_declaration" => match declarado_por(n, source) {
                Some((nombre, true)) => {
                    m.specializations += 1;
                    let _ = nombre;
                }
                Some((nombre, false)) => {
                    m.templates += 1;
                    primarias.push((n, nombre));
                }
                None => {}
            },
            "template_type" => {
                m.instantiation_sites += 1;
                textos.insert(tsnode::text_of(n, source).split_whitespace().collect());
            }
            "static_assert_declaration" => m.static_asserts += 1,
            "if_statement" => {
                // Ver el doc del módulo: token ANÓNIMO.
                let mut c = n.walk();
                if n.children(&mut c).any(|h| h.kind() == "constexpr") {
                    m.constexpr_branches += 1;
                }
            }
            _ => {}
        }
    });
    m.distinct_instantiations = textos.len();
    m.recursive_templates = primarias
        .iter()
        .filter(|(td, nombre)| !instanciaciones(*td, nombre, source).is_empty())
        .map(|(_, nombre)| nombre.clone())
        .collect();
    m
}

pub fn template_smells(root: Node, source: &str, is_cpp: bool) -> Vec<StructuralSmell> {
    if !is_cpp {
        return Vec::new();
    }
    let mut out = Vec::new();

    // Primero, todas las declaraciones de template del archivo, separando
    // plantillas primarias de especializaciones.
    let mut primarias: Vec<(Node, String)> = Vec::new();
    let mut especializados: Vec<String> = Vec::new();
    tsnode::walk_flat(root, &mut |n| {
        if n.kind() != "template_declaration" {
            return;
        }
        if let Some((nombre, es_especializacion)) = declarado_por(n, source) {
            if es_especializacion {
                especializados.push(nombre);
            } else {
                primarias.push((n, nombre));
            }
        }
    });

    for (td, nombre) in &primarias {
        // ¿Se instancia a sí mismo? Ése es el patrón de recursión.
        let propias = instanciaciones(*td, nombre, source);
        if propias.is_empty() {
            continue;
        }
        let linea_decl = td.start_position().row + 1;
        let tiene_base = especializados.iter().any(|e| e == nombre) || corta_por_dentro(*td, source);

        if !tiene_base {
            out.push(StructuralSmell {
                kind: "recursive_template_no_base_case",
                name: nombre.clone(),
                line: linea_decl,
                message: format!(
                    "`{nombre}` instantiates itself but this file shows no way for the recursion to stop: no specialization of `{nombre}`, no `if constexpr`, no `enable_if`/`requires`. Template recursion without a base case is not a slow program — it is a build that fails, when the compiler hits its instantiation-depth limit (~900 in gcc, ~1024 in clang). Read from the shape of the declarations; if the base case lives in another translation unit, this analysis cannot see it."
                ),
            });
            continue;
        }

        // Con caso base, lo que importa es CUÁN hondo se lo invoca. Solo
        // cuentan los sitios con argumento literal: `Factorial<N - 1>` dentro
        // de la propia plantilla no dice nada, y `Factorial<n>` con `n`
        // variable tampoco — no se juzga lo que no es literal.
        let mut mas_hondo: Option<(i64, usize)> = None;
        for (nodo, literal) in instanciaciones(root, nombre, source) {
            let Some(v) = literal else { continue };
            let linea = nodo.start_position().row + 1;
            if mas_hondo.is_none_or(|(prev, _)| v > prev) {
                mas_hondo = Some((v, linea));
            }
        }
        let Some((profundidad, linea)) = mas_hondo else { continue };
        if profundidad < PROFUNDIDAD_NOTABLE {
            continue;
        }
        out.push(StructuralSmell {
            kind: "deep_template_recursion",
            name: nombre.clone(),
            line: linea,
            message: format!(
                "`{nombre}<{profundidad}>` makes the compiler create about {} distinct types, one per recursion level, each with its own mangled name and its own entries in the compiler's tables. That is compile-time cost, not runtime cost: the program may well run instantly. The depth is exact because the argument is a literal — with a non-literal argument nothing would be claimed. What this CANNOT tell you is how many symbols end up generated or how long the build takes: knowing that requires instantiating, which means compiling. gcc stops at ~900 levels by default and clang at ~1024.",
                profundidad + 1
            ),
        });
    }

    out.sort_by_key(|s| s.line);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn hallazgos(src: &str) -> Vec<StructuralSmell> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_cpp::LANGUAGE.into()).unwrap();
        let arbol = parser.parse(src, None).unwrap();
        template_smells(arbol.root_node(), src, true)
    }

    fn tiene(src: &str, kind: &str) -> bool {
        hallazgos(src).iter().any(|s| s.kind == kind)
    }

    const CON_BASE: &str = concat!(
        "template<int N>\n",
        "struct Factorial { static constexpr int value = N * Factorial<N - 1>::value; };\n",
        "template<>\n",
        "struct Factorial<0> { static constexpr int value = 1; };\n",
    );

// ── Métricas de forma ──

    fn metricas(src: &str) -> TemplateComplexity {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_cpp::LANGUAGE.into()).unwrap();
        let arbol = parser.parse(src, None).unwrap();
        template_complexity(arbol.root_node(), src, true)
    }

    #[test]
    fn las_metricas_cuentan_lo_escrito() {
        let src = concat!(
            "template<int N> struct Factorial { static constexpr int v = N * Factorial<N - 1>::v; };\n",
            "template<> struct Factorial<0> { static constexpr int v = 1; };\n",
            "template<typename T> struct Caja { T x; };\n",
            "static_assert(Factorial<3>::v == 6, \"mal\");\n",
            "template<typename T> void f(T t) { if constexpr (sizeof(T) > 4) { g(t); } }\n",
        );
        let m = metricas(src);
        assert_eq!(m.templates, 3, "Factorial, Caja y f");
        assert_eq!(m.specializations, 1, "Factorial<0>");
        assert_eq!(m.static_asserts, 1);
        assert_eq!(m.constexpr_branches, 1);
        assert_eq!(m.recursive_templates, vec!["Factorial".to_string()]);
    }

    #[test]
    fn los_sitios_sobrecuentan_y_los_distintos_subcuentan() {
        // La razon por la que estan los DOS numeros, y el limite honesto de
        // cada uno: `Caja<int>` escrito 3 veces son 3 sitios pero UNA sola
        // instanciacion para el compilador (sobrecuenta), y ese mismo conteo
        // deduplicado no ve que `Factorial<N - 1>` es un texto y N
        // instanciaciones (subcuenta).
        let src = concat!(
            "template<typename T> struct Caja { T x; };\n",
            "Caja<int> a; Caja<int> b; Caja<int> c;\n",
        );
        let m = metricas(src);
        assert_eq!(m.instantiation_sites, 3, "tres usos escritos");
        assert_eq!(m.distinct_instantiations, 1, "una sola forma distinta");
    }

    #[test]
    fn en_c_las_metricas_quedan_vacias() {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_cpp::LANGUAGE.into()).unwrap();
        let src = "template<typename T> struct Caja { T x; };\n";
        let arbol = parser.parse(src, None).unwrap();
        let m = template_complexity(arbol.root_node(), src, false);
        assert_eq!(m.templates, 0);
        assert_eq!(m.instantiation_sites, 0);
    }

    #[test]
    fn una_recursion_sin_caso_base_se_marca() {
        let src = "template<int N>\nstruct Sin { static constexpr int v = Sin<N - 1>::v; };\n";
        assert!(tiene(src, "recursive_template_no_base_case"), "{:?}", hallazgos(src));
    }

    #[test]
    fn una_especializacion_es_el_caso_base() {
        assert!(!tiene(CON_BASE, "recursive_template_no_base_case"), "{:?}", hallazgos(CON_BASE));
    }

    #[test]
    fn un_if_constexpr_tambien_corta() {
        // La forma moderna del caso base (C++17), sin especializacion aparte.
        // El `constexpr` del `if` es un token ANONIMO en la gramatica: si el
        // recorrido mirara solo nodos nombrados, esto se reportaria mal.
        let src = concat!(
            "template<int N>\n",
            "constexpr int suma() {\n",
            "  if constexpr (N == 0) { return 0; }\n",
            "  else { return N + suma<N - 1>(); }\n",
            "}\n",
        );
        assert!(!tiene(src, "recursive_template_no_base_case"), "{:?}", hallazgos(src));
    }

    #[test]
    fn enable_if_tambien_corta() {
        let src = concat!(
            "template<int N>\n",
            "struct Con { typename enable_if<(N > 0)>::type t; static constexpr int v = Con<N - 1>::v; };\n",
        );
        assert!(!tiene(src, "recursive_template_no_base_case"), "{:?}", hallazgos(src));
    }

    #[test]
    fn una_profundidad_literal_grande_se_reporta() {
        let src = format!("{CON_BASE}int main() {{ return Factorial<500>::value; }}\n");
        let h = hallazgos(&src);
        let f = h.iter().find(|s| s.kind == "deep_template_recursion").expect("esperaba el hallazgo");
        // 500 niveles + el caso base.
        assert!(f.message.contains("501"), "{}", f.message);
    }

    #[test]
    fn una_profundidad_chica_no_se_reporta() {
        let src = format!("{CON_BASE}int main() {{ return Factorial<10>::value; }}\n");
        assert!(!tiene(&src, "deep_template_recursion"), "{:?}", hallazgos(&src));
    }

    #[test]
    fn un_argumento_no_literal_no_se_juzga() {
        // `Factorial<n>` con `n` variable: la profundidad no se puede leer, y
        // afirmar una seria inventarla.
        let src = format!("{CON_BASE}template<int n> int usar() {{ return Factorial<n>::value; }}\n");
        assert!(!tiene(&src, "deep_template_recursion"), "{:?}", hallazgos(&src));
    }

    #[test]
    fn el_mensaje_declara_lo_que_no_puede_saber() {
        // La promesa del modulo: nunca afirmar nada del lado del compilador.
        let src = format!("{CON_BASE}int main() {{ return Factorial<500>::value; }}\n");
        let h = hallazgos(&src);
        let f = h.iter().find(|s| s.kind == "deep_template_recursion").unwrap();
        assert!(f.message.contains("requires instantiating"), "{}", f.message);
    }

    #[test]
    fn un_template_que_no_se_instancia_a_si_mismo_no_es_recursivo() {
        let src = "template<typename T>\nstruct Caja { T valor; };\n";
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src));
    }

    #[test]
    fn usar_otro_template_no_es_recursion() {
        let src = concat!(
            "template<typename T> struct Caja { T v; };\n",
            "template<typename T> struct Envoltorio { Caja<T> c; };\n",
        );
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src));
    }

    #[test]
    fn en_c_no_se_analiza_nada() {
        // C no tiene templates: recorrer seria trabajo puro sin resultado.
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_cpp::LANGUAGE.into()).unwrap();
        let src = "template<int N>\nstruct Sin { static constexpr int v = Sin<N - 1>::v; };\n";
        let arbol = parser.parse(src, None).unwrap();
        assert!(template_smells(arbol.root_node(), src, false).is_empty());
    }
}
