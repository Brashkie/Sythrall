//! **Big-O de Rust sobre el árbol — el cuarto hermano de `amortizado.rs`
//! (Python), `ccosto.rs` (C++) y `jscosto.rs` (JS/TS).**
//!
//! Medido antes de escribir nada: **Rust era el único de los 8 lenguajes
//! target sin Big-O**, y no porque fallara sino porque `rsparse.rs` devolvía
//! literalmente `"no estimado"` para toda función. La complejidad
//! ciclomática sí se calculaba bien, así que el recorrido del árbol ya
//! estaba: lo que faltaba era la inferencia.
//!
//! ```text
//! constante    real O(1)      decía "no estimado"
//! lineal       real O(n)      decía "no estimado"
//! cuadrática   real O(n²)     decía "no estimado"
//! binaria      real O(log n)  decía "no estimado"
//! fib          real O(2^n)    decía "no estimado"
//! ```
//!
//! **Es la sección principal del panel** (*"Algorithm Complexity — Big O"*),
//! así que un `.rs` analizado no aportaba nada ahí.
//!
//! **Los costos de contenedor de Rust traen una simetría que los otros
//! lenguajes no tienen tan nítida**: `contains` es el MISMO nombre de método
//! en `Vec` y en `HashSet`, y es O(n) en uno y O(1) en el otro. No es una
//! diferencia de estilo — es la misma llamada, escrita igual, con
//! asintótica distinta según el tipo del receptor. Por eso hace falta el
//! mapa de familias y no alcanza con una lista de nombres de método.

use std::collections::HashMap;

use tree_sitter::Node;

use crate::ast::tsnode::{self, text_of};

/// Bucles de Rust. `loop` incluido: no tiene condición, pero corre hasta un
/// `break` y su cuerpo puede depender de `n` igual que un `while`.
const BUCLES: &[&str] = &["for_expression", "while_expression", "loop_expression"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Costo {
    Constante,
    Logaritmico,
    Lineal,
    /// `sort`/`sort_by`: O(n log n) por llamada.
    Linearitmico,
}

/// La familia del contenedor, que es lo que decide el costo.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Familia {
    /// `Vec`, `String`, slices: contiguos. Insertar o borrar en el medio
    /// corre la cola; buscar no tiene índice.
    Contiguo,
    /// `HashMap`, `HashSet`: O(1) promedio.
    Hash,
    /// `BTreeMap`, `BTreeSet`: árbol balanceado, O(log n).
    Ordenado,
    /// `VecDeque`: contiguo circular. **La diferencia que importa con
    /// `Vec`**: insertar y quitar en CUALQUIERA de los dos extremos es O(1).
    Deque,
}

/// De qué familia es un tipo, mirando su texto. Es lo único disponible sin
/// resolución de nombres, igual que en `ccosto.rs`.
fn familia_de(texto: &str) -> Option<Familia> {
    let t = texto.replace(char::is_whitespace, "");
    if t.contains("HashMap") || t.contains("HashSet") {
        return Some(Familia::Hash);
    }
    if t.contains("BTreeMap") || t.contains("BTreeSet") {
        return Some(Familia::Ordenado);
    }
    if t.contains("VecDeque") {
        return Some(Familia::Deque);
    }
    if t.contains("Vec<") || t.contains("vec!") || t.starts_with("&[") || t.contains("String") {
        return Some(Familia::Contiguo);
    }
    None
}

/// El costo de un método según la familia del receptor.
///
/// **`push`/`pop` de `Vec` son amortizados y NO se marcan**, por el mismo
/// motivo que `push_back` en C++ y `append` en Python: el realloc duplica
/// capacidad y su costo se reparte. Marcarlos convertiría el acumulador
/// correcto en un falso positivo.
fn costo_de(familia: Familia, metodo: &str) -> Option<(Costo, &'static str)> {
    if matches!(metodo, "push" | "pop" | "push_back" | "push_front" | "pop_back" | "pop_front") {
        // En `Vec`, `push`/`pop` son por el final y amortizados. En
        // `VecDeque`, los cuatro son O(1) de verdad.
        return Some((Costo::Constante, "it is amortized O(1)"));
    }
    if matches!(metodo, "sort" | "sort_by" | "sort_by_key" | "sort_unstable" | "sort_unstable_by") {
        return Some((Costo::Linearitmico, "a comparison sort is O(n log n)"));
    }
    if matches!(metodo, "binary_search" | "binary_search_by") {
        return Some((Costo::Logaritmico, "a binary search halves the range"));
    }
    match (familia, metodo) {
        (Familia::Contiguo, "insert" | "remove" | "drain" | "retain") => {
            Some((Costo::Lineal, "in a contiguous container it shifts every element after that position"))
        }
        (Familia::Contiguo, "contains" | "position" | "iter" | "find" | "clone" | "to_vec" | "concat" | "join") => {
            Some((Costo::Lineal, "a contiguous container has no index to search by, so it scans"))
        }
        (Familia::Hash, "insert" | "remove" | "get" | "contains" | "contains_key" | "entry") => {
            Some((Costo::Constante, "a hash container is O(1) on average"))
        }
        (Familia::Ordenado, "insert" | "remove" | "get" | "contains" | "contains_key" | "range") => {
            Some((Costo::Logaritmico, "an ordered container is a balanced tree"))
        }
        (Familia::Deque, "insert" | "remove") => {
            Some((Costo::Lineal, "inserting in the MIDDLE of a deque still shifts, even though both ends are O(1)"))
        }
        _ => None,
    }
}

/// Una operación con costo, y a qué nivel de `n` llega.
#[derive(Debug, Clone)]
pub struct CostoRust {
    pub operacion: String,
    pub costo: Costo,
    pub profundidad_bucle: u32,
    pub motivo: &'static str,
}

/// El mapa nombre → familia del archivo.
///
/// Tres formas, porque Rust declara de tres maneras y las tres son comunes:
/// `let v: Vec<i32>` (anotación), `let v = Vec::new()` (inferencia desde el
/// constructor) y `fn f(v: &[i32])` (parámetro).
fn declaraciones(raiz: Node, source: &str) -> HashMap<String, Familia> {
    let mut mapa = HashMap::new();
    tsnode::walk_flat(raiz, &mut |n| {
        match n.kind() {
            "let_declaration" => {
                let Some(patron) = n.child_by_field_name("pattern") else { return };
                let Some(nombre) = identificador_de(patron, source) else { return };
                // La anotación manda; si no hay, se mira el inicializador.
                let familia = n
                    .child_by_field_name("type")
                    .and_then(|t| familia_de(text_of(t, source)))
                    .or_else(|| n.child_by_field_name("value").and_then(|v| familia_de(text_of(v, source))));
                if let Some(f) = familia {
                    mapa.insert(nombre, f);
                }
            }
            "parameter" => {
                let (Some(patron), Some(tipo)) = (n.child_by_field_name("pattern"), n.child_by_field_name("type")) else {
                    return;
                };
                let Some(nombre) = identificador_de(patron, source) else { return };
                if let Some(f) = familia_de(text_of(tipo, source)) {
                    mapa.insert(nombre, f);
                }
            }
            _ => {}
        }
    });
    mapa
}

fn identificador_de(n: Node, source: &str) -> Option<String> {
    if n.kind() == "identifier" {
        return Some(text_of(n, source).to_string());
    }
    // `mut v` es un `mut_pattern` que envuelve al identificador.
    let mut c = n.walk();
    let hallado = n.named_children(&mut c).find_map(|h| identificador_de(h, source));
    hallado
}

/// El costo de una llamada a método `receptor.metodo(…)`.
fn costo_de_llamada(n: Node, source: &str, tipos: &HashMap<String, Familia>, depth: u32) -> Option<CostoRust> {
    let funcion = n.child_by_field_name("function")?;
    if funcion.kind() != "field_expression" {
        return None;
    }
    let receptor = funcion.child_by_field_name("value")?;
    let campo = funcion.child_by_field_name("field")?;
    let metodo = text_of(campo, source);
    // El receptor tiene que ser un nombre pelado: con `a.b.c.remove(…)` no
    // se sabe de qué tipo es `c` y no se afirma nada.
    if receptor.kind() != "identifier" {
        return None;
    }
    let nombre = text_of(receptor, source);
    let familia = *tipos.get(nombre)?;
    let (costo, motivo) = costo_de(familia, metodo)?;
    Some(CostoRust { operacion: format!("{nombre}.{metodo}(…)"), costo, profundidad_bucle: depth, motivo })
}

/// `true` si el cuerpo divide un rango a la mitad — la firma de una búsqueda
/// binaria escrita a mano.
fn hay_division_binaria(func: Node, source: &str) -> bool {
    let mut hay = false;
    tsnode::walk_flat(func, &mut |n| {
        if !matches!(n.kind(), "binary_expression" | "assignment_expression" | "compound_assignment_expr") {
            return;
        }
        let t = text_of(n, source).replace(char::is_whitespace, "");
        if t.contains("/2") || t.contains(">>1") {
            hay = true;
        }
    });
    hay
}

/// Cuántas veces se llama a sí misma la función, en el mismo camino.
fn autollamadas(func: Node, source: &str, nombre: &str) -> u32 {
    let mut n_llamadas = 0u32;
    tsnode::walk_flat(func, &mut |n| {
        if n.kind() != "call_expression" {
            return;
        }
        let Some(f) = n.child_by_field_name("function") else { return };
        if f.kind() == "identifier" && text_of(f, source) == nombre {
            n_llamadas += 1;
        }
    });
    n_llamadas
}

/// Profundidad de bucles anidados y las operaciones con costo.
fn analizar(func: Node, raiz: Node, source: &str) -> (u32, Vec<CostoRust>) {
    // **El mapa de tipos se arma GLOBAL y despues se pisa con lo LOCAL.**
    // Armarlo solo global era un defecto real: dos funciones del mismo
    // archivo con un parametro del mismo nombre y distinto tipo se pisaban
    // entre si, y ganaba la ultima declaracion del archivo. Lo encontro la
    // verificacion en navegador — el mismo archivo daba O(n^2) midiendo una
    // funcion sola y O(n) con las dos juntas. Lo global sigue haciendo
    // falta para ver campos y variables de fuera de la funcion.
    let mut tipos = declaraciones(raiz, source);
    tipos.extend(declaraciones(func, source));
    let mut max_depth = 0u32;
    let mut costos = Vec::new();

    fn walk(n: Node, depth: u32, source: &str, tipos: &HashMap<String, Familia>, max_depth: &mut u32, costos: &mut Vec<CostoRust>) {
        let es_bucle = BUCLES.contains(&n.kind());
        let d = if es_bucle { depth + 1 } else { depth };
        if es_bucle {
            *max_depth = (*max_depth).max(d);
        }
        if n.kind() == "call_expression" {
            if let Some(c) = costo_de_llamada(n, source, tipos, d) {
                costos.push(c);
            }
        }
        let mut cursor = n.walk();
        let hijos: Vec<Node> = n.children(&mut cursor).collect();
        for hijo in hijos {
            walk(hijo, d, source, tipos, max_depth, costos);
        }
    }
    walk(func, 0, source, &tipos, &mut max_depth, &mut costos);
    (max_depth, costos)
}

fn etiqueta(profundidad: u32) -> String {
    match profundidad {
        0 => "O(1)".into(),
        1 => "O(n)".into(),
        2 => "O(n²)".into(),
        3 => "O(n³)".into(),
        k => format!("O(n^{k})"),
    }
}

fn razon_por_anidamiento(depth: u32) -> String {
    match depth {
        1 => "un loop".into(),
        2 => "2 loops anidados".into(),
        3 => "3 loops anidados".into(),
        k => format!("{k} loops anidados"),
    }
}

/// El Big-O de una función de Rust.
///
/// **Los límites, dichos porque son los mismos que los otros hermanos
/// declaran**: dos bucles SECUENCIALES son O(n), no O(n²) —se mide
/// anidamiento, no cantidad—; una recursión que no se puede resolver con una
/// regla clara se reporta por su forma y no se inventa una recurrencia; y
/// una llamada a otra función no se sigue (sin eso, todo archivo exigiría un
/// análisis interprocedural).
pub fn inferir(func: Node, raiz: Node, source: &str, nombre: &str) -> (String, String) {
    let (depth, costos) = analizar(func, raiz, source);
    let binario = hay_division_binaria(func, source);
    let auto = autollamadas(func, source, nombre);

    // ── Recursión ───────────────────────────────────────────────────────
    if auto >= 2 && !binario {
        return (
            "O(2^n)".into(),
            format!("`{nombre}` se llama a sí misma {auto} veces por nivel sin dividir el problema — el árbol de llamadas se duplica en cada paso"),
        );
    }
    if auto >= 2 && binario {
        return ("O(n log n)".into(), format!("`{nombre}` se llama a sí misma {auto} veces sobre mitades del problema — divide y vencerás"));
    }
    if auto == 1 && binario {
        return ("O(log n)".into(), format!("`{nombre}` se llama a sí misma una vez sobre la mitad del problema"));
    }
    if auto == 1 {
        return ("O(n)".into(), format!("`{nombre}` se llama a sí misma una vez por nivel"));
    }

    // ── Operación con costo dentro de un bucle ──────────────────────────
    let dominante = costos
        .iter()
        .filter(|c| c.costo != Costo::Constante && c.profundidad_bucle > 0)
        .max_by_key(|c| (c.profundidad_bucle, c.costo == Costo::Lineal));

    if depth == 0 {
        // Una operación cara FUERA de todo bucle igual domina a O(1).
        let suelta = costos.iter().find(|c| matches!(c.costo, Costo::Lineal | Costo::Linearitmico));
        return match suelta {
            Some(c) if c.costo == Costo::Linearitmico => {
                ("O(n log n)".into(), format!("`{}` cuesta O(n log n) — {}", c.operacion, c.motivo))
            }
            Some(c) => ("O(n)".into(), format!("`{}` cuesta O(n) — {}", c.operacion, c.motivo)),
            None => ("O(1)".into(), "sin loops".into()),
        };
    }

    match dominante {
        Some(c) if c.costo == Costo::Linearitmico => (
            etiqueta(c.profundidad_bucle + 1),
            format!("`{}` cuesta O(n log n) — {}, dentro de {} loop(s)", c.operacion, c.motivo, c.profundidad_bucle),
        ),
        Some(c) if c.costo == Costo::Lineal => {
            let e = c.profundidad_bucle + 1;
            (
                etiqueta(e),
                format!(
                    "`{}` cuesta O(n) por llamada — {}. Con {} loop{} alrededor, eso multiplica a {}",
                    c.operacion,
                    c.motivo,
                    c.profundidad_bucle,
                    if c.profundidad_bucle == 1 { "" } else { "s" },
                    etiqueta(e)
                ),
            )
        }
        Some(c) if c.costo == Costo::Logaritmico && depth == 1 => (
            "O(n log n)".into(),
            format!("`{}` cuesta O(log n) por llamada — {}, y el loop corre n veces", c.operacion, c.motivo),
        ),
        _ if depth == 1 && binario => ("O(log n)".into(), "loop que reduce el rango a la mitad".into()),
        _ => (etiqueta(depth), razon_por_anidamiento(depth)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn bigo(src: &str) -> (String, String) {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let t = p.parse(src, None).unwrap();
        let raiz = t.root_node();
        let mut func = None;
        tsnode::walk_flat(raiz, &mut |n| {
            if n.kind() == "function_item" && func.is_none() {
                func = Some(n);
            }
        });
        let f = func.expect("sin función");
        let nombre = f.child_by_field_name("name").map(|x| text_of(x, src).to_string()).unwrap_or_default();
        inferir(f, raiz, src, &nombre)
    }

    #[test]
    fn sin_bucles_es_constante() {
        assert_eq!(bigo("fn f(x: i32) -> i32 { x + 1 }\n").0, "O(1)");
    }

    #[test]
    fn un_bucle_es_lineal() {
        assert_eq!(bigo("fn f(xs: &[i32]) -> i32 {\n    let mut t = 0;\n    for x in xs { t += x; }\n    t\n}\n").0, "O(n)");
    }

    #[test]
    fn dos_bucles_anidados_son_cuadraticos() {
        assert_eq!(bigo("fn f(xs: &[i32]) -> i32 {\n    let mut t = 0;\n    for x in xs { for y in xs { t += x * y; } }\n    t\n}\n").0, "O(n²)");
    }

    /// **Dos bucles SECUENCIALES no son cuadráticos** — se mide anidamiento,
    /// no cantidad. Es el mismo error que `jscosto.rs` documenta haber
    /// encontrado en el motor viejo de JS.
    #[test]
    fn dos_bucles_secuenciales_siguen_siendo_lineales() {
        let src = "fn f(xs: &[i32]) -> i32 {\n    let mut t = 0;\n    for x in xs { t += x; }\n    for y in xs { t += y; }\n    t\n}\n";
        assert_eq!(bigo(src).0, "O(n)");
    }

    #[test]
    fn un_while_que_divide_a_la_mitad_es_logaritmico() {
        let src = "fn f(xs: &[i32], o: i32) -> bool {\n    let (mut lo, mut hi) = (0usize, xs.len());\n    while lo < hi {\n        let mid = (lo + hi) / 2;\n        if xs[mid] == o { return true; }\n        lo = mid + 1;\n    }\n    false\n}\n";
        assert_eq!(bigo(src).0, "O(log n)");
    }

    #[test]
    fn dos_autollamadas_sin_dividir_son_exponenciales() {
        let src = "fn fib(n: u64) -> u64 {\n    if n < 2 { return n; }\n    fib(n - 1) + fib(n - 2)\n}\n";
        let (b, r) = bigo(src);
        assert_eq!(b, "O(2^n)");
        assert!(r.contains("se duplica"), "{r}");
    }

    /// **La simetría que Rust muestra más nítida que ningún otro lenguaje**:
    /// `contains` es el MISMO método en `Vec` y en `HashSet`, y es O(n) en
    /// uno y O(1) en el otro.
    #[test]
    fn contains_es_lineal_en_vec_y_constante_en_hashset() {
        let en_vec = "fn f(v: Vec<i32>, xs: &[i32]) -> i32 {\n    let mut t = 0;\n    for x in xs { if v.contains(x) { t += 1; } }\n    t\n}\n";
        let en_set = "use std::collections::HashSet;\nfn f(v: HashSet<i32>, xs: &[i32]) -> i32 {\n    let mut t = 0;\n    for x in xs { if v.contains(x) { t += 1; } }\n    t\n}\n";
        let (bv, rv) = bigo(en_vec);
        assert_eq!(bv, "O(n²)", "{rv}");
        assert!(rv.contains("no index to search by"), "{rv}");
        assert_eq!(bigo(en_set).0, "O(n)", "una tabla hash no sube el nivel");
    }

    /// `Vec::remove` corre la cola; dentro de un bucle es cuadrático.
    #[test]
    fn vec_remove_en_un_bucle_es_cuadratico() {
        let src = "fn f(mut v: Vec<i32>, n: usize) {\n    for _ in 0..n { v.remove(0); }\n}\n";
        let (b, r) = bigo(src);
        assert_eq!(b, "O(n²)", "{r}");
        assert!(r.contains("shifts every element"), "{r}");
    }

    /// **Y la contraparte que hace que no sea ruido**: `push` es amortizado
    /// y no sube nada, igual que `push_back` en C++ y `append` en Python.
    #[test]
    fn push_es_amortizado_y_no_sube_el_nivel() {
        let src = "fn f(mut v: Vec<i32>, n: usize) {\n    for i in 0..n { v.push(i as i32); }\n}\n";
        assert_eq!(bigo(src).0, "O(n)");
    }

    /// `VecDeque` es la diferencia que importa: quitar por el frente es O(1)
    /// ahí y O(n) en un `Vec`.
    #[test]
    fn vecdeque_pop_front_no_sube_el_nivel() {
        let src = "use std::collections::VecDeque;\nfn f(mut d: VecDeque<i32>, n: usize) {\n    for _ in 0..n { d.pop_front(); }\n}\n";
        assert_eq!(bigo(src).0, "O(n)");
    }

    #[test]
    fn sort_dentro_de_un_bucle_sube_un_nivel() {
        let src = "fn f(mut v: Vec<i32>, n: usize) {\n    for _ in 0..n { v.sort(); }\n}\n";
        let (b, r) = bigo(src);
        assert_eq!(b, "O(n²)", "{r}");
        assert!(r.contains("O(n log n)"), "{r}");
    }

    /// Un `sort` suelto, fuera de todo bucle, es O(n log n) y no O(1).
    #[test]
    fn un_sort_suelto_es_linearitmico_no_constante() {
        assert_eq!(bigo("fn f(mut v: Vec<i32>) {\n    v.sort();\n}\n").0, "O(n log n)");
    }

    /// Un receptor de tipo desconocido no se juzga: el silencio es la
    /// respuesta correcta cuando no se sabe de qué familia es.
    #[test]
    fn un_receptor_de_tipo_desconocido_no_cambia_el_bigo() {
        let src = "fn f(algo: Desconocido, n: usize) {\n    for _ in 0..n { algo.remove(0); }\n}\n";
        assert_eq!(bigo(src).0, "O(n)");
    }

    #[test]
    fn tres_bucles_anidados_son_cubicos() {
        let src = "fn f(xs: &[i32]) {\n    for _a in xs { for _b in xs { for _c in xs { } } }\n}\n";
        assert_eq!(bigo(src).0, "O(n³)");
    }

    /// **El mismo defecto que en `ccosto.rs`, heredado al espejarlo**, y
    /// encontrado por la verificacion en navegador: con las dos funciones en
    /// el archivo, el `HashSet` de una pisaba el `Vec` de la otra y
    /// `busca_en_vec` bajaba de O(n^2) a O(n).
    #[test]
    fn dos_funciones_con_un_parametro_homonimo_no_se_pisan() {
        let src = "use std::collections::HashSet;\nfn a(v: Vec<i32>, xs: &[i32]) -> i32 {\n    let mut t = 0;\n    for x in xs { if v.contains(x) { t += 1; } }\n    t\n}\nfn b(v: HashSet<i32>, xs: &[i32]) -> i32 {\n    let mut t = 0;\n    for x in xs { if v.contains(x) { t += 1; } }\n    t\n}\n";
        let mut p = Parser::new();
        p.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let t = p.parse(src, None).unwrap();
        let raiz = t.root_node();
        let mut funcs = Vec::new();
        tsnode::walk_flat(raiz, &mut |n| {
            if n.kind() == "function_item" {
                funcs.push(n);
            }
        });
        assert_eq!(inferir(funcs[0], raiz, src, "a").0, "O(n²)", "buscar en un Vec escanea");
        assert_eq!(inferir(funcs[1], raiz, src, "b").0, "O(n)", "buscar en un HashSet no");
    }
}
