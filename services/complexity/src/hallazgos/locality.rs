//! Fase 32 (Sythrall Mathematical Analysis) — localidad espacial de acceso.
//!
//! Nace del aporte del usuario sobre curvas de llenado de espacio
//! (Hilbert/Morton) y la maldición de la dimensionalidad. **Pero lo que se
//! implementa acá es deliberadamente la parte chica y demostrable de esa
//! idea: el stride.** El ROADMAP dejó escrita la trampa antes de construir
//! nada, y este módulo la respeta:
//!
//! > Para un recorrido COMPLETO, row-major ya es óptimo. Morton/Hilbert solo
//! > pagan en consultas de rango y acceso por tiles. Un analizador ingenuo
//! > recomendaría Morton para código que no lo necesita.
//!
//! Así que acá **nunca se sugiere Morton ni Hilbert**. Lo que se sugiere es
//! intercambiar los bucles, que es el arreglo real de un recorrido completo
//! con stride grande, y cuesta una línea.
//!
//! **Por qué esto es demostrable y no una opinión.** C y C++ almacenan
//! `int a[N][M]` en orden **row-major**: `a[i][j]` y `a[i][j+1]` son
//! contiguos en memoria; `a[i][j]` y `a[i+1][j]` están separados por `M`
//! elementos. Entonces, en un nido `for i { for j { ... } }`, la variable
//! que varía más rápido es `j`, y el acceso tiene stride 1 solo si `j` está
//! en la ÚLTIMA posición del subíndice. Si `j` está en la primera, cada
//! iteración salta una fila entera. Eso se lee del árbol, no se estima.
//!
//! **El lenguaje importa, y ahí está lo interesante.** Fortran es
//! column-major: el mismo nido con el mismo subíndice es óptimo en Fortran y
//! pésimo en C. Sythrall ya sabe en qué lenguaje está. Fortran queda para la
//! próxima porción por un motivo concreto, no por olvido: en la gramática de
//! tree-sitter-fortran, `A(I,J)` es indistinguible de una llamada a función
//! `A(I,J)` sin rastrear declaraciones, así que hace falta esa pieza antes.
//!
//! **Lo que NO se afirma.** Que el acceso sea lento de verdad depende de que
//! el arreglo sea contiguo. `int a[N][M]` lo es; `int **a` no —sus filas
//! pueden estar dispersas en el heap—, y ahí intercambiar los bucles ayuda
//! menos. Esa diferencia se lee de la declaración (`array_declarator`
//! anidado contra `pointer_declarator`) y decide la confianza, en vez de
//! reportar las dos con la misma seguridad.

use tree_sitter::Node;

use serde::Serialize;

use crate::ast::tsnode;

#[derive(Serialize, Clone)]
pub struct LocalityFinding {
    pub kind: &'static str,
    /// Función que contiene el nido de bucles.
    pub function: String,
    /// Arreglo accedido con stride grande.
    pub array: String,
    pub line: usize,
    /// Variable del bucle MÁS INTERNO — la que varía más rápido.
    pub inner_loop_variable: String,
    pub severity: &'static str,
    pub confidence: &'static str,
    pub message: String,
}

/// Nombre de la variable de un `for` de C. Cubre las 2 formas:
/// `for (int i = 0; …)` (un `declaration` con `init_declarator`) y
/// `for (i = 0; …)` (un `assignment_expression`).
fn loop_variable<'a>(for_node: Node, source: &'a str) -> Option<&'a str> {
    let init = for_node.child_by_field_name("initializer")?;
    match init.kind() {
        "declaration" => {
            let mut cursor = init.walk();
            let decl = init.children(&mut cursor).find(|c| c.kind() == "init_declarator")?;
            let id = decl.child_by_field_name("declarator")?;
            (id.kind() == "identifier").then(|| tsnode::text_of(id, source))
        }
        "assignment_expression" => {
            let left = init.child_by_field_name("left")?;
            (left.kind() == "identifier").then(|| tsnode::text_of(left, source))
        }
        _ => None,
    }
}

/// Índices de UN `subscript_expression`, cubriendo las dos gramáticas.
///
/// **C y C++ no coinciden acá, y confundirlos hace que el análisis no
/// encuentre nada en C++ sin fallar** (fue exactamente lo que pasó, y lo
/// detectó el test de C++): tree-sitter-c expone un campo `index` con un
/// único nodo, mientras que tree-sitter-cpp expone `indices`, una
/// `subscript_argument_list` — porque C++23 admite `a[i, j]` con varios
/// índices en un solo corchete. Se soportan las dos, así que la sintaxis
/// nueva de C++23 se analiza igual que la clásica.
///
/// Solo índices que son una variable pelada: `a[i+1][j]` tiene otro stride, y
/// afirmar algo sobre él sería inventar.
fn indices_de<'a>(node: Node, source: &'a str) -> Option<Vec<&'a str>> {
    if let Some(index) = node.child_by_field_name("index") {
        return (index.kind() == "identifier").then(|| vec![tsnode::text_of(index, source)]);
    }
    let lista = node.child_by_field_name("indices")?;
    let mut cursor = lista.walk();
    let hijos: Vec<Node> = lista.named_children(&mut cursor).collect();
    if hijos.is_empty() {
        return None;
    }
    hijos
        .iter()
        .map(|h| (h.kind() == "identifier").then(|| tsnode::text_of(*h, source)))
        .collect()
}

/// `a[i][j]` → `("a", ["i", "j"])`, en orden de fuente. `None` si la base no
/// es un identificador simple (`f(x)[i]`, `s.campo[i]`): sin un nombre de
/// arreglo no hay nada que reportar ni declaración que buscar.
fn subscript_chain<'a>(node: Node, source: &'a str) -> Option<(&'a str, Vec<&'a str>)> {
    // Cada nivel del encadenado aporta su propia lista; se recolectan de
    // afuera hacia adentro y se invierte el ORDEN DE LOS NIVELES, no el de
    // los índices dentro de cada uno (que ya vienen en orden de fuente).
    let mut niveles: Vec<Vec<&'a str>> = Vec::new();
    let mut actual = node;
    while actual.kind() == "subscript_expression" {
        niveles.push(indices_de(actual, source)?);
        actual = actual.child_by_field_name("argument")?;
    }
    if actual.kind() != "identifier" {
        return None;
    }
    niveles.reverse();
    let indices: Vec<&'a str> = niveles.into_iter().flatten().collect();
    if indices.len() < 2 {
        return None;
    }
    Some((tsnode::text_of(actual, source), indices))
}

/// `true` si `nombre` está declarado en esta función como un arreglo
/// multidimensional real (`int a[N][M]`), o sea contiguo en memoria.
fn es_arreglo_contiguo(func: Node, nombre: &str, source: &str) -> bool {
    let mut contiguo = false;
    tsnode::walk_flat(func, &mut |n| {
        if contiguo || n.kind() != "array_declarator" {
            return;
        }
        // `int a[N][M]` es un `array_declarator` cuyo declarator es OTRO
        // `array_declarator`. Un solo nivel es un arreglo 1D.
        let Some(interno) = n.child_by_field_name("declarator") else { return };
        if interno.kind() != "array_declarator" {
            return;
        }
        let Some(base) = interno.child_by_field_name("declarator") else { return };
        if base.kind() == "identifier" && tsnode::text_of(base, source) == nombre {
            contiguo = true;
        }
    });
    contiguo
}

/// `true` si este `for` no contiene otro `for` — o sea, es el más interno.
fn es_el_mas_interno(for_node: Node) -> bool {
    let Some(body) = for_node.child_by_field_name("body") else { return true };
    let mut hay_otro = false;
    tsnode::walk_flat(body, &mut |n| {
        if matches!(n.kind(), "for_statement" | "while_statement") {
            hay_otro = true;
        }
    });
    !hay_otro
}

pub fn locality_findings(root: Node, source: &str) -> Vec<LocalityFinding> {
    let mut out: Vec<LocalityFinding> = Vec::new();

    // Se recorren funciones para poder mirar sus declaraciones (la
    // contigüidad del arreglo) sin volver a subir por el árbol después.
    let mut funciones = Vec::new();
    tsnode::walk_flat(root, &mut |n| {
        if n.kind() == "function_definition" {
            funciones.push(n);
        }
    });

    for func in funciones {
        let nombre_func = tsnode::function_definition_name(func, source);
        let mut internos = Vec::new();
        tsnode::walk_flat(func, &mut |n| {
            if n.kind() == "for_statement" && es_el_mas_interno(n) {
                internos.push(n);
            }
        });

        for interno in internos {
            // Solo interesa un bucle que esté DENTRO de otro: un `for` suelto
            // recorriendo un arreglo 1D no tiene un orden alternativo.
            let anidado = {
                let mut p = interno.parent();
                let mut dentro = false;
                while let Some(n) = p {
                    if n.kind() == "function_definition" {
                        break;
                    }
                    if n.kind() == "for_statement" {
                        dentro = true;
                        break;
                    }
                    p = n.parent();
                }
                dentro
            };
            if !anidado {
                continue;
            }
            let Some(var_interna) = loop_variable(interno, source) else { continue };
            let Some(body) = interno.child_by_field_name("body") else { continue };

            let mut accesos = Vec::new();
            tsnode::walk_flat(body, &mut |n| {
                if n.kind() != "subscript_expression" {
                    return;
                }
                // Solo la raíz de la cadena: `a[j][i]` visita también `a[j]`,
                // y contarlo sería reportar el mismo acceso dos veces.
                if n.parent().is_some_and(|p| p.kind() == "subscript_expression") {
                    return;
                }
                if let Some(par) = subscript_chain(n, source) {
                    accesos.push((par, n.start_position().row + 1));
                }
            });

            for ((arreglo, indices), line) in accesos {
                // C/C++ es row-major: la variable que varía más rápido debe
                // estar en la ÚLTIMA posición. Si está en la primera, cada
                // iteración salta una fila entera.
                let en_la_ultima = indices.last() == Some(&var_interna);
                let en_la_primera = indices.first() == Some(&var_interna);
                if en_la_ultima || !en_la_primera {
                    continue;
                }
                if out.iter().any(|f| f.line == line && f.array == arreglo) {
                    continue;
                }
                let contiguo = es_arreglo_contiguo(func, arreglo, source);
                let (confidence, nota_layout) = if contiguo {
                    (
                        "High",
                        "`{arreglo}` está declarado como arreglo multidimensional real, así que sus filas SON contiguas y el salto es el que dice esto.",
                    )
                } else {
                    (
                        "Medium",
                        "No se encontró en esta función una declaración de `{arreglo}` como arreglo multidimensional real. Si es un `T **` (filas asignadas por separado), las filas ya están dispersas y reordenar los bucles ayuda menos; el patrón de acceso igual es el descripto.",
                    )
                };
                out.push(LocalityFinding {
                    kind: "column_major_traversal",
                    function: nombre_func.clone(),
                    array: arreglo.to_string(),
                    line,
                    inner_loop_variable: var_interna.to_string(),
                    severity: "Medium",
                    confidence,
                    message: format!(
                        "`{arreglo}[{}]` con `{var_interna}` en la PRIMERA posición del subíndice, dentro del bucle más interno. C/C++ guarda las matrices en orden row-major, así que el índice que varía más rápido tiene que ser el ÚLTIMO para leer memoria contigua: acá cada iteración avanza una fila entera en vez de un elemento, y cada línea de caché traída se usa una sola vez. {} El arreglo es intercambiar los dos bucles (una línea), NO reordenar los datos: para un recorrido completo, row-major ya es el orden óptimo — Morton/Hilbert solo pagan en consultas de rango y acceso por tiles.",
                        indices.join("]["),
                        nota_layout.replace("{arreglo}", arreglo)
                    ),
                });
            }
        }
    }
    out.sort_by_key(|f| f.line);
    out
}

// ─────────────────────────────────────────────────────────────────────────
// Fortran (Fase 32, 2026-09-25) — segunda porción
// ─────────────────────────────────────────────────────────────────────────

/// **Fortran es column-major, y por eso es el caso interesante: el MISMO
/// nido que en C es pésimo, acá es óptimo.**
///
/// C guarda `a[i][j]` con la ÚLTIMA dimensión contigua, así que el índice
/// que varía más rápido tiene que ser el último. Fortran guarda `a(i,j)` con
/// la PRIMERA contigua: `a(i,j)` y `a(i+1,j)` son vecinos, y `a(i,j)` y
/// `a(i,j+1)` están separados por una columna entera. La regla se da vuelta
/// exactamente.
///
/// Esa simetría es lo que hace que portar un nido de bucles entre los dos
/// lenguajes sin darlo vuelta sea un error de rendimiento clásico — y la
/// razón por la que este módulo no podía reusar el análisis de C con otro
/// nombre: la condición es la opuesta, no una variante.
///
/// **El bloqueo que el ROADMAP nombraba está resuelto en `tipos.rs`**: en
/// tree-sitter-fortran `A(I,J)` parsea igual que una llamada a función, y lo
/// único que los separa es si `A` fue declarado con forma.
pub fn locality_findings_fortran(root: Node, source: &str) -> Vec<LocalityFinding> {
    let mut out: Vec<LocalityFinding> = Vec::new();

    let mut ambitos = Vec::new();
    tsnode::walk_flat(root, &mut |n| {
        if n.is_named() && matches!(n.kind(), "program" | "module" | "subroutine" | "function") {
            ambitos.push(n);
        }
    });

    for amb in ambitos {
        let arreglos = crate::hallazgos::tipos::arreglos_declarados_fortran(amb, source);
        if arreglos.is_empty() {
            continue;
        }
        let nombre_amb = nombre_de_unidad(amb, source);

        let mut internos = Vec::new();
        tsnode::walk_flat(amb, &mut |n| {
            if n.kind() == "do_loop" && es_el_do_mas_interno(n) {
                internos.push(n);
            }
        });

        for interno in internos {
            // Sin anidamiento no hay orden alternativo que proponer — mismo
            // criterio que la porción de C.
            if !esta_dentro_de_otro_do(interno) {
                continue;
            }
            let Some(var_interna) = variable_del_do(interno, source) else { continue };

            tsnode::walk_flat(interno, &mut |acceso| {
                if acceso.kind() != "call_expression" {
                    return;
                }
                let mut c = acceso.walk();
                let hijos: Vec<Node> = acceso.children(&mut c).collect();
                let Some(nombre_nodo) = hijos.iter().find(|h| h.kind() == "identifier") else { return };
                let nombre = tsnode::text_of(*nombre_nodo, source).to_lowercase();
                // **Acá es donde el rastreo de declaraciones paga**: sin él,
                // esto podría ser una llamada a función.
                let Some(arr) = arreglos.get(&nombre) else { return };
                if arr.rango < 2 {
                    return;
                }
                let Some(lista) = hijos.iter().find(|h| h.kind() == "argument_list") else { return };

                // Solo índices que sean una variable pelada: `a(i+1,j)` tiene
                // otro stride y afirmar algo sería inventar.
                let mut c2 = lista.walk();
                let indices: Vec<Node> = lista.named_children(&mut c2).collect();
                if indices.len() < 2 || !indices.iter().all(|i| i.kind() == "identifier") {
                    return;
                }
                let textos: Vec<String> = indices.iter().map(|i| tsnode::text_of(*i, source).to_string()).collect();

                // Column-major: stride 1 exige que la PRIMERA posición sea la
                // variable del bucle más interno. El hallazgo es lo contrario:
                // la variable interna aparece, pero NO en la primera.
                let primera_es_la_interna = textos[0].eq_ignore_ascii_case(&var_interna);
                let aparece_despues = textos[1..].iter().any(|t| t.eq_ignore_ascii_case(&var_interna));
                if primera_es_la_interna || !aparece_despues {
                    return;
                }

                let (confidence, nota) = if arr.forma_diferida && arr.es_argumento {
                    (
                        "Medium",
                        "`{a}` es un argumento mudo de forma asumida, así que puede recibir una REBANADA no contigua (`x(1:n:2, :)`) — el único caso en que un arreglo de Fortran no es contiguo. Con el atributo `contiguous` o con una forma explícita, la contigüidad queda garantizada y el salto es exactamente el descripto.",
                    )
                } else {
                    (
                        "High",
                        "`{a}` está declarado con forma en este ámbito, así que es contiguo de verdad: Fortran no tiene el equivalente del `T **` de C, donde las filas pueden estar dispersas por el heap.",
                    )
                };

                out.push(LocalityFinding {
                    kind: "row_major_traversal",
                    function: nombre_amb.clone(),
                    array: nombre.clone(),
                    line: acceso.start_position().row + 1,
                    inner_loop_variable: var_interna.clone(),
                    severity: "Medium",
                    confidence,
                    message: format!(
                        "`{}({})` con `{var_interna}` fuera de la PRIMERA posición del subíndice, dentro del bucle más interno. Fortran guarda las matrices en orden column-major —al revés que C—, así que el índice que varía más rápido tiene que ser el PRIMERO para leer memoria contigua: acá cada iteración avanza una columna entera en vez de un elemento, y cada línea de caché traída se usa una sola vez. {} El arreglo es intercambiar los dos bucles (una línea). Y conviene tenerlo presente al portar: este MISMO nido es el óptimo en C, donde la regla es la opuesta — traducir un nido entre los dos lenguajes sin darlo vuelta es un error de rendimiento clásico.",
                        nombre,
                        textos.join(","),
                        nota.replace("{a}", &nombre)
                    ),
                });
            });
        }
    }

    out.sort_by_key(|f| f.line);
    out
}

/// `true` si este `do_loop` no contiene otro.
fn es_el_do_mas_interno(n: Node) -> bool {
    let mut hay_otro = false;
    let mut c = n.walk();
    let hijos: Vec<Node> = n.children(&mut c).collect();
    for h in hijos {
        tsnode::walk_flat(h, &mut |x| {
            if x.kind() == "do_loop" {
                hay_otro = true;
            }
        });
    }
    !hay_otro
}

fn esta_dentro_de_otro_do(n: Node) -> bool {
    let mut p = n.parent();
    while let Some(x) = p {
        if x.is_named() && matches!(x.kind(), "subroutine" | "function" | "program" | "module") {
            return false;
        }
        if x.kind() == "do_loop" {
            return true;
        }
        p = x.parent();
    }
    false
}

/// La variable de control de un `do`: el primer `identifier` de su
/// `loop_control_expression`.
fn variable_del_do(n: Node, source: &str) -> Option<String> {
    let mut hallada = None;
    tsnode::walk_flat(n, &mut |x| {
        if hallada.is_some() || x.kind() != "loop_control_expression" {
            return;
        }
        let mut c = x.walk();
        let primera = x.children(&mut c).find(|h| h.kind() == "identifier");
        if let Some(i) = primera {
            hallada = Some(tsnode::text_of(i, source).to_string());
        }
    });
    hallada
}

fn nombre_de_unidad(n: Node, source: &str) -> String {
    let mut c = n.walk();
    let hijos: Vec<Node> = n.children(&mut c).collect();
    for h in hijos {
        if !h.kind().ends_with("_statement") {
            continue;
        }
        if let Some(nombre) = h.child_by_field_name("name") {
            return tsnode::text_of(nombre, source).to_string();
        }
        let mut c2 = h.walk();
        let por_kind = h.children(&mut c2).find(|x| x.kind() == "name");
        if let Some(nombre) = por_kind {
            return tsnode::text_of(nombre, source).to_string();
        }
    }
    format!("<{}>", n.kind())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lenguajes::cparse;

    fn hallazgos(src: &str) -> Vec<LocalityFinding> {
        cparse::parse_c(src).expect("parse error").locality
    }

    #[test]
    fn el_recorrido_row_major_correcto_no_produce_nada() {
        // `j` (bucle interno) en la ULTIMA posicion: stride 1, lo optimo.
        let src = "void f(int n) {\n  int a[10][10];\n  for (int i = 0; i < n; i++) {\n    for (int j = 0; j < n; j++) {\n      a[i][j] = 0;\n    }\n  }\n}\n";
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src).len());
    }

    #[test]
    fn el_recorrido_column_major_se_detecta() {
        let src = "void f(int n) {\n  int a[10][10];\n  for (int i = 0; i < n; i++) {\n    for (int j = 0; j < n; j++) {\n      a[j][i] = 0;\n    }\n  }\n}\n";
        let f = hallazgos(src);
        assert_eq!(f.len(), 1, "{f:?}", f = f.len());
        assert_eq!(f[0].kind, "column_major_traversal");
        assert_eq!(f[0].array, "a");
        assert_eq!(f[0].inner_loop_variable, "j");
        assert_eq!(f[0].function, "f");
    }

    #[test]
    fn un_arreglo_declarado_de_verdad_da_confianza_alta() {
        // `int a[10][10]` es contiguo: el salto es exactamente el que se dice.
        let src = "void f(int n) {\n  int a[10][10];\n  for (int i = 0; i < n; i++) {\n    for (int j = 0; j < n; j++) {\n      a[j][i] = 0;\n    }\n  }\n}\n";
        let f = hallazgos(src);
        assert_eq!(f[0].confidence, "High");
        assert!(f[0].message.contains("SON contiguas"), "{}", f[0].message);
    }

    #[test]
    fn un_puntero_a_puntero_baja_la_confianza_en_vez_de_afirmar_lo_mismo() {
        // `int **b`: las filas pueden estar dispersas en el heap, asi que
        // intercambiar los bucles ayuda menos. El patron es el mismo; la
        // certeza sobre su impacto, no.
        let src = "void f(int n, int **b) {\n  for (int i = 0; i < n; i++) {\n    for (int j = 0; j < n; j++) {\n      b[j][i] = 0;\n    }\n  }\n}\n";
        let f = hallazgos(src);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].confidence, "Medium");
        assert!(f[0].message.contains("dispersas"), "{}", f[0].message);
    }

    #[test]
    fn nunca_sugiere_morton_ni_hilbert() {
        // La trampa que el ROADMAP dejo escrita ANTES de construir esto: para
        // un recorrido completo row-major ya es optimo, y recomendar una
        // curva de llenado de espacio seria un veredicto sin evidencia.
        let src = "void f(int n) {\n  int a[10][10];\n  for (int i = 0; i < n; i++) {\n    for (int j = 0; j < n; j++) {\n      a[j][i] = 0;\n    }\n  }\n}\n";
        let m = &hallazgos(src)[0].message;
        assert!(m.contains("intercambiar los dos bucles"), "{m}");
        assert!(!m.to_lowercase().contains("usar morton"), "{m}");
        assert!(m.contains("solo pagan en consultas de rango"), "{m}");
    }

    #[test]
    fn un_bucle_suelto_no_se_reporta() {
        // Sin anidamiento no hay orden alternativo que proponer.
        let src = "void f(int n) {\n  int a[10][10];\n  for (int j = 0; j < n; j++) {\n    a[j][0] = 0;\n  }\n}\n";
        assert!(hallazgos(src).is_empty());
    }

    #[test]
    fn un_indice_que_no_es_una_variable_pelada_no_se_reporta() {
        // `a[j+1][i]` tiene otro stride; afirmar algo seria inventar.
        let src = "void f(int n) {\n  int a[10][10];\n  for (int i = 0; i < n; i++) {\n    for (int j = 0; j < n; j++) {\n      a[j + 1][i] = 0;\n    }\n  }\n}\n";
        assert!(hallazgos(src).is_empty());
    }

    #[test]
    fn un_acceso_1d_no_se_reporta() {
        let src = "void f(int n) {\n  int a[10];\n  for (int i = 0; i < n; i++) {\n    for (int j = 0; j < n; j++) {\n      a[j] = i;\n    }\n  }\n}\n";
        assert!(hallazgos(src).is_empty());
    }

    #[test]
    fn el_mismo_acceso_no_se_cuenta_dos_veces() {
        // `a[j][i]` visita tambien el subarbol `a[j]`: sin el filtro de raiz,
        // el mismo acceso saldria dos veces.
        let src = "void f(int n) {\n  int a[10][10];\n  for (int i = 0; i < n; i++) {\n    for (int j = 0; j < n; j++) {\n      a[j][i] = a[j][i] + 1;\n    }\n  }\n}\n";
        assert_eq!(hallazgos(src).len(), 1);
    }




    #[test]
    fn la_sintaxis_multiindice_de_cpp23_tambien_se_analiza() {
        // `a[j, i]` en un solo corchete: tree-sitter-cpp lo expone como una
        // `subscript_argument_list` con 2 hijos, no como 2 subscripts
        // encadenados. Es la misma pregunta y tiene que dar el mismo
        // resultado.
        let src = "void f(int n) {\n  int a[10][10];\n  for (int i = 0; i < n; i++) {\n    for (int j = 0; j < n; j++) {\n      g(a[j, i]);\n    }\n  }\n}\n";
        let r = cparse::parse_cpp(src).unwrap();
        assert_eq!(r.locality.len(), 1, "{:?}", r.locality.len());
        assert_eq!(r.locality[0].inner_loop_variable, "j");
    }

    #[test]
    fn tambien_anda_en_cpp() {
        let src = "void f(int n) {\n  int a[10][10];\n  for (int i = 0; i < n; i++) {\n    for (int j = 0; j < n; j++) {\n      a[j][i] = 0;\n    }\n  }\n}\n";
        assert_eq!(cparse::parse_cpp(src).unwrap().locality.len(), 1);
    }

    #[test]
    fn la_forma_con_variable_ya_declarada_tambien_se_reconoce() {
        // `for (i = 0; ...)` en vez de `for (int i = 0; ...)` — comun en C89.
        let src = "void f(int n) {\n  int a[10][10];\n  int i, j;\n  for (i = 0; i < n; i++) {\n    for (j = 0; j < n; j++) {\n      a[j][i] = 0;\n    }\n  }\n}\n";
        assert_eq!(hallazgos(src).len(), 1);
    }

    // ── Fortran: la regla es la OPUESTA a la de C ───────────────────────

    fn fortran(src: &str) -> Vec<LocalityFinding> {
        let mut p = tree_sitter::Parser::new();
        p.set_language(&tree_sitter_fortran::LANGUAGE.into()).unwrap();
        let t = p.parse(src, None).unwrap();
        locality_findings_fortran(t.root_node(), src)
    }

    const CABECERA: &str = "subroutine s(n)\n  implicit none\n  integer :: n, i, j\n  real :: a(100,100)\n";

    /// **El caso que el bullet llamaba el más interesante**: este nido
    /// —`i` afuera, `j` adentro, acceso `a(i,j)`— es PÉSIMO en Fortran,
    /// y es exactamente el ÓPTIMO en C.
    #[test]
    fn el_nido_que_es_optimo_en_c_es_pesimo_en_fortran() {
        let src = format!("{CABECERA}  do i = 1, n\n    do j = 1, n\n      a(i,j) = 0.0\n    end do\n  end do\nend subroutine s\n");
        let f = fortran(&src);
        assert_eq!(f.len(), 1, "{:?}", f.iter().map(|x| &x.array).collect::<Vec<_>>());
        assert_eq!(f[0].kind, "row_major_traversal");
        assert_eq!(f[0].array, "a");
        assert_eq!(f[0].inner_loop_variable, "j");
    }

    /// **Y el simétrico**: `j` afuera, `i` adentro, con `i` en la PRIMERA
    /// posición es el orden correcto en Fortran — y no se dice nada.
    #[test]
    fn el_nido_correcto_de_fortran_no_produce_hallazgo() {
        let src = format!("{CABECERA}  do j = 1, n\n    do i = 1, n\n      a(i,j) = 0.0\n    end do\n  end do\nend subroutine s\n");
        assert!(fortran(&src).is_empty(), "{:?}", fortran(&src).len());
    }

    /// **El mensaje tiene que decir la simetría con C**, porque portar un
    /// nido entre los dos lenguajes sin darlo vuelta es el error clásico.
    #[test]
    fn el_mensaje_advierte_sobre_portar_desde_c() {
        let src = format!("{CABECERA}  do i = 1, n\n    do j = 1, n\n      a(i,j) = 0.0\n    end do\n  end do\nend subroutine s\n");
        let f = fortran(&src);
        let m = &f[0].message;
        assert!(m.contains("column-major"), "{m}");
        assert!(m.contains("al revés que C"), "{m}");
        assert!(m.contains("el óptimo en C"), "y que portar sin dar vuelta es un error: {m}");
    }

    /// **El bloqueo que el ROADMAP nombraba**: sin saber que `a` es un
    /// arreglo declarado, `a(i,j)` es indistinguible de una llamada. Un
    /// nombre NO declarado con forma no produce hallazgo.
    #[test]
    fn una_llamada_a_funcion_no_se_confunde_con_un_acceso() {
        let src = "subroutine s(n)\n  implicit none\n  integer :: n, i, j\n  real :: x\n  do i = 1, n\n    do j = 1, n\n      x = f(i,j)\n    end do\n  end do\nend subroutine s\n";
        assert!(fortran(src).is_empty(), "sin declaración de arreglo no se afirma nada");
    }

    /// Un arreglo de rango 1 no tiene orden alternativo.
    #[test]
    fn un_arreglo_unidimensional_no_produce_hallazgo() {
        let src = "subroutine s(n)\n  implicit none\n  integer :: n, i, j\n  real :: v(100)\n  do i = 1, n\n    do j = 1, n\n      v(i) = 0.0\n    end do\n  end do\nend subroutine s\n";
        assert!(fortran(src).is_empty());
    }

    /// Un `do` suelto, sin anidamiento, no tiene orden alternativo que
    /// proponer — mismo criterio que la porción de C.
    #[test]
    fn un_do_suelto_no_produce_hallazgo() {
        let src = format!("{CABECERA}  do i = 1, n\n    a(i,1) = 0.0\n  end do\nend subroutine s\n");
        assert!(fortran(&src).is_empty());
    }

    /// Índices que no son una variable pelada tienen otro stride: afirmar
    /// algo sería inventar.
    #[test]
    fn un_indice_con_aritmetica_no_se_juzga() {
        let src = format!("{CABECERA}  do i = 1, n\n    do j = 1, n\n      a(i+1,j) = 0.0\n    end do\n  end do\nend subroutine s\n");
        assert!(fortran(&src).is_empty());
    }

    /// **La única forma en que un arreglo de Fortran no es contiguo**: un
    /// argumento mudo de forma asumida, que puede recibir una rebanada.
    #[test]
    fn un_argumento_de_forma_asumida_baja_la_confianza_y_lo_explica() {
        let src = "subroutine s(a, n)\n  implicit none\n  integer :: n, i, j\n  real :: a(:,:)\n  do i = 1, n\n    do j = 1, n\n      a(i,j) = 0.0\n    end do\n  end do\nend subroutine s\n";
        let f = fortran(src);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].confidence, "Medium");
        assert!(f[0].message.contains("REBANADA"), "{}", f[0].message);
        assert!(f[0].message.contains("contiguous"), "da el atributo que lo arregla: {}", f[0].message);
    }

    /// Un arreglo local con forma explícita es contiguo de verdad.
    #[test]
    fn un_arreglo_con_forma_explicita_tiene_confianza_alta() {
        let src = format!("{CABECERA}  do i = 1, n\n    do j = 1, n\n      a(i,j) = 0.0\n    end do\n  end do\nend subroutine s\n");
        let f = fortran(&src);
        assert_eq!(f[0].confidence, "High");
        assert!(f[0].message.contains("no tiene el equivalente del `T **`"), "{}", f[0].message);
    }

    /// El atributo `dimension(...)` es la otra forma de declarar, y también
    /// habilita la detección.
    #[test]
    fn el_atributo_dimension_tambien_habilita_la_deteccion() {
        let src = "subroutine s(n)\n  implicit none\n  integer :: n, i, j\n  real, dimension(50,50) :: b\n  do i = 1, n\n    do j = 1, n\n      b(i,j) = 0.0\n    end do\n  end do\nend subroutine s\n";
        assert_eq!(fortran(src).len(), 1);
    }
}
