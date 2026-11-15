//! **False sharing: dos campos atómicos pegados en el mismo struct.**
//!
//! Cuando dos hilos escriben campos distintos que caen en la **misma línea de
//! caché**, cada escritura de un núcleo invalida la copia del otro. El
//! programa es correcto —no hay data race, son atómicos— pero el paralelismo
//! se convierte en tráfico de coherencia, y puede terminar **más lento que
//! la versión de un solo hilo**. Es el defecto de rendimiento que mejor
//! encaja con la forma de este motor: se ve en el layout, no en el flujo.
//!
//! **Lo que se afirma es `potential`, y el límite lo puso el usuario cuando
//! propuso el análisis.** Demostrar false sharing exige el layout real del
//! compilador, runtime y profiling; lo que se puede leer del AST es que
//! **hay dos campos atómicos adyacentes sin nada que los separe**. Eso es una
//! condición necesaria, no suficiente: si los escribe un solo hilo, no pasa
//! nada. El hallazgo lo dice con todas las letras en vez de dejarlo implícito.
//!
//! **Las tres mitigaciones que se reconocen** —y que hacen que NO se
//! reporte— son las tres formas reales de arreglarlo:
//!
//! - `alignas(64)` (o `alignas(std::hardware_destructive_interference_size)`,
//!   que es la forma portable) sobre el campo: lo empuja a su propia línea.
//! - Un campo de **relleno** entre los dos (`char pad[64];`), que es la forma
//!   clásica de antes de C++17.
//! - Que no sean adyacentes: con un campo no atómico en el medio, el
//!   análisis ya no puede afirmar que compartan línea.
//!
//! Reconocerlas importa tanto como detectar el problema: un analizador que
//! marcara igual el struct ya arreglado estaría pidiendo un cambio que ya
//! está hecho, y se lo dejaría de leer.

use tree_sitter::Node;

use crate::hallazgos::smells::StructuralSmell;
use crate::ast::tsnode::{self, text_of};

/// Tipos que indican que el campo se escribe desde varios hilos.
///
/// `volatile` entra aunque en C++ **no** sea una herramienta de concurrencia
/// (no ordena ni hace atómicas las operaciones): en código real se usa con
/// esa intención equivocada, y dos `volatile` adyacentes escritos por hilos
/// distintos tienen el mismo problema de línea de caché — más otro peor.
fn es_compartido(campo: Node, source: &str) -> bool {
    // `atomic` vive en el TIPO (`std::atomic<int>`, `atomic_int`, `_Atomic`).
    if let Some(tipo) = campo.child_by_field_name("type") {
        let t = text_of(tipo, source).replace(char::is_whitespace, "");
        if t.contains("atomic") || t.contains("_Atomic") {
            return true;
        }
    }
    // `volatile` NO: es un `type_qualifier` hermano del tipo, así que
    // mirarlo dentro del campo `type` no lo encuentra nunca. Lo descubrió
    // un test que esperaba marcarlo y no marcaba nada.
    let mut c = campo.walk();
    let hay_volatile = campo
        .children(&mut c)
        .any(|h| h.kind() == "type_qualifier" && text_of(h, source).trim() == "volatile");
    hay_volatile
}

/// El nombre declarado, para poder citarlo en el mensaje.
fn nombre_del_campo(campo: Node, source: &str) -> Option<String> {
    let d = campo.child_by_field_name("declarator")?;
    fn identificador(n: Node, source: &str) -> Option<String> {
        if n.kind() == "field_identifier" || n.kind() == "identifier" {
            return Some(text_of(n, source).to_string());
        }
        // Se liga a un `let` en vez de devolverlo directo: como expresión
        // final, el iterador temporal sobrevive al cursor local y el préstamo
        // no cierra. Cuarta vez que aparece en este crate.
        let mut c = n.walk();
        let encontrado = n.children(&mut c).find_map(|h| identificador(h, source));
        encontrado
    }
    identificador(d, source)
}

/// `true` si el campo lleva `alignas(...)` o `__declspec(align(...))`.
fn tiene_alineacion(campo: Node, source: &str) -> bool {
    let mut hay = false;
    tsnode::walk_flat(campo, &mut |n| {
        if n.kind() == "alignas_qualifier" {
            hay = true;
        }
    });
    hay || text_of(campo, source).contains("__declspec(align")
}

/// `true` si el campo parece relleno: un arreglo de bytes cuyo nombre lo dice.
///
/// Se exigen **las dos cosas** —forma de arreglo y nombre— a propósito. Solo
/// por el nombre, un `int padding_ms` de otra cosa callaría el hallazgo;
/// solo por la forma, cualquier `char buffer[N]` de datos reales también.
fn es_relleno(campo: Node, source: &str) -> bool {
    let texto = text_of(campo, source).to_lowercase();
    let con_forma_de_arreglo = texto.contains('[');
    let con_nombre_de_relleno =
        ["pad", "padding", "relleno", "filler", "_pad", "cacheline", "cache_line"]
            .iter()
            .any(|n| texto.contains(n));
    con_forma_de_arreglo && con_nombre_de_relleno
}

/// Emite `StructuralSmell`, el shape que ya viaja por todo el motor —
/// mismo criterio que `plantillas::template_smells`: sin clave nueva en el
/// resultado ni brazo nuevo en `scanner.rs`.
pub fn analyze(root: Node, source: &str) -> Vec<StructuralSmell> {
    let mut out = Vec::new();
    tsnode::walk_flat(root, &mut |n| {
        if n.kind() != "struct_specifier" && n.kind() != "class_specifier" {
            return;
        }
        let Some(lista) = n.child_by_field_name("body") else { return };
        let nombre_struct = n
            .child_by_field_name("name")
            .map(|x| text_of(x, source).to_string())
            .unwrap_or_else(|| "<anónimo>".into());

        // Los campos EN ORDEN: la adyacencia es el dato, así que el orden de
        // declaración es lo único que importa acá.
        let mut cursor = lista.walk();
        let campos: Vec<Node> =
            lista.children(&mut cursor).filter(|h| h.kind() == "field_declaration").collect();

        for par in campos.windows(2) {
            let (a, b) = (par[0], par[1]);
            if !es_compartido(a, source) || !es_compartido(b, source) {
                continue;
            }
            // Las tres mitigaciones. Con cualquiera de ellas, el análisis ya
            // no puede afirmar que compartan línea, así que se calla.
            if tiene_alineacion(a, source) || tiene_alineacion(b, source) {
                continue;
            }
            if es_relleno(a, source) || es_relleno(b, source) {
                continue;
            }

            let na = nombre_del_campo(a, source).unwrap_or_else(|| "?".into());
            let nb = nombre_del_campo(b, source).unwrap_or_else(|| "?".into());
            out.push(StructuralSmell {
                kind: "potential_false_sharing",
                name: nombre_struct.clone(),
                line: b.start_position().row + 1,
                message: format!(
                    "`{na}` and `{nb}` are adjacent atomic fields in `{nombre_struct}` with nothing \
                     between them, so they will almost certainly land on the same cache line. If \
                     two threads write them, every write from one core invalidates the other's \
                     copy — the program stays correct, but the parallelism turns into coherence \
                     traffic and can end up SLOWER than single-threaded. **This is POTENTIAL, not \
                     proven**: showing it really happens needs the compiler's actual layout, a \
                     running program and profiling. What can be read from the source is only that \
                     nothing separates them — which is necessary, not sufficient: if one thread \
                     writes both, nothing goes wrong. The fixes are `alignas(std::hardware_\
                     destructive_interference_size)` on each field (portable since C++17), a \
                     padding array between them, or simply not keeping them in the same struct."
                ),
            });
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn analizar(src: &str) -> Vec<StructuralSmell> {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_cpp::LANGUAGE.into()).unwrap();
        let t = p.parse(src, None).unwrap();
        analyze(t.root_node(), src)
    }

    #[test]
    fn dos_atomicos_pegados_se_marcan() {
        let src = "struct C {\n    std::atomic<int> a;\n    std::atomic<int> b;\n};\n";
        let f = analizar(src);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].kind, "potential_false_sharing");
        assert_eq!(f[0].name, "C");
        assert!(f[0].message.contains("POTENTIAL, not \\\n                     proven") || f[0].message.contains("POTENTIAL"));
    }

    /// **La mitigación portable de C++17.** Marcar un struct ya arreglado
    /// sería pedir un cambio que ya está hecho.
    #[test]
    fn alignas_lo_calla() {
        let src = "struct C {\n    alignas(64) std::atomic<int> a;\n    alignas(64) std::atomic<int> b;\n};\n";
        assert!(analizar(src).is_empty());
    }

    /// La forma clásica de antes de C++17: un arreglo de relleno en el medio.
    #[test]
    fn un_campo_de_relleno_en_el_medio_lo_calla() {
        let src = "struct C {\n    std::atomic<int> a;\n    char padding[64];\n    std::atomic<int> b;\n};\n";
        assert!(analizar(src).is_empty());
    }

    /// Y con cualquier campo no atómico en el medio tampoco se puede afirmar
    /// adyacencia — no hace falta que sea relleno explícito.
    #[test]
    fn un_campo_cualquiera_en_el_medio_rompe_la_adyacencia() {
        let src = "struct C {\n    std::atomic<int> a;\n    int contador;\n    std::atomic<int> b;\n};\n";
        assert!(analizar(src).is_empty());
    }

    #[test]
    fn dos_campos_normales_no_son_false_sharing() {
        let src = "struct C {\n    int a;\n    int b;\n};\n";
        assert!(analizar(src).is_empty());
    }

    /// Un solo atómico no tiene con quién compartir línea de forma visible.
    #[test]
    fn un_solo_atomico_no_se_marca() {
        let src = "struct C {\n    std::atomic<int> a;\n    int b;\n};\n";
        assert!(analizar(src).is_empty());
    }

    /// Tres seguidos dan dos pares: `a`-`b` y `b`-`c`.
    #[test]
    fn tres_atomicos_seguidos_dan_dos_pares() {
        let src = "struct C {\n    std::atomic<int> a;\n    std::atomic<int> b;\n    std::atomic<long> c;\n};\n";
        assert_eq!(analizar(src).len(), 2);
    }

    /// `volatile` entra aunque NO sea una herramienta de concurrencia en
    /// C++: en código real se usa con esa intención, y el problema de línea
    /// de caché es el mismo.
    #[test]
    fn volatile_tambien_cuenta() {
        let src = "struct C {\n    volatile int a;\n    volatile int b;\n};\n";
        assert_eq!(analizar(src).len(), 1);
    }

    /// El mensaje tiene que decir que es POTENCIAL y qué haría falta para
    /// probarlo — el límite que el propio bullet del ROADMAP fijó.
    #[test]
    fn el_mensaje_dice_que_es_potencial_y_que_exigiria_probarlo() {
        let src = "struct C {\n    std::atomic<int> a;\n    std::atomic<int> b;\n};\n";
        let m = &analizar(src)[0].message;
        assert!(m.contains("POTENTIAL"), "{m}");
        assert!(m.contains("profiling"), "dice qué haría falta para probarlo: {m}");
        assert!(m.contains("necessary, not sufficient"), "y que la condición no alcanza: {m}");
        assert!(m.contains("hardware_"), "y da la solución portable: {m}");
    }

    #[test]
    fn una_clase_tambien_se_analiza() {
        let src = "class C {\npublic:\n    std::atomic<int> a;\n    std::atomic<int> b;\n};\n";
        assert_eq!(analizar(src).len(), 1);
    }
}
