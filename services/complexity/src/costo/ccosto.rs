//! **Big-O de C/C++ sobre el ÁRBOL, y el costo real de cada operación de
//! contenedor.**
//!
//! Lo que había antes era coincidencia de texto sobre el cuerpo de la
//! función (`body.matches(" for ").count()`, `body.contains("mid")`). Medido
//! contra el sidecar vivo antes de escribir este módulo, **5 de 8 casos
//! daban mal**:
//!
//! ```text
//! dos bucles SECUENCIALES              real O(n)        decía O(n²)      FALSO
//! variable llamada `midpoint`          real O(n)        decía O(log n)   FALSO
//! una llamada a `humidity(i)`          real O(n)        decía O(log n)   FALSO
//! vector::insert(begin) en un bucle    real O(n²)       decía O(n)       FALSO
//! map::insert en un bucle              real O(n log n)  decía O(n)       FALSO
//! ```
//!
//! Los tres defectos son independientes y cada uno tiene su causa:
//!
//! 1. **Contar `" for "` cuenta OCURRENCIAS, no ANIDAMIENTO.** Dos bucles
//!    uno detrás del otro recorren n cada uno: O(n), no O(n²). Es el más
//!    común de los tres porque el patrón es cotidiano.
//! 2. **`contains("mid")` matchea adentro de cualquier identificador** —
//!    `midpoint`, `middle`, y el ejemplo que lo deja más claro, `humidity`.
//! 3. **El costo de la operación de adentro no se miraba**, igual que pasaba
//!    del lado Python antes de `amortizado.rs`.
//!
//! **Y C++ necesita algo que Python no**: `x.insert(…)` no dice nada por sí
//! solo. En un `std::vector` es O(n); en un `std::map`, O(log n); en un
//! `std::unordered_map`, O(1) amortizado. El mismo texto, tres costos
//! distintos. Por eso acá hay rastreo de declaraciones, y por eso **un
//! receptor de tipo desconocido no dispara nada**: sin el tipo no hay nada
//! que afirmar, y callarse es la respuesta correcta.

use std::collections::HashMap;
use tree_sitter::Node;

use crate::ast::tsnode::{self, text_of};

/// Cuánto cuesta UNA llamada, en potencias de `n`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Costo {
    /// O(1), amortizado o no: no multiplica nada.
    Constante,
    /// O(log n) — `std::map`/`std::set`.
    Logaritmico,
    /// O(n) — recorre o corre el contenedor entero.
    Lineal,
}

/// Familias de contenedor que cambian el costo de la MISMA operación.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Familia {
    /// `vector`, `string`, `deque`: acceso contiguo, insertar en el medio corre la cola.
    Contiguo,
    /// `map`, `set`, `multimap`, `multiset`: árbol balanceado.
    Ordenado,
    /// `unordered_*`: tabla hash.
    Hash,
    /// `list`, `forward_list`: insertar/borrar con un iterador es O(1).
    Enlazado,
}

/// De qué familia es un tipo declarado. Se mira el texto del tipo porque es
/// lo único que hay sin resolución de nombres — `std::vector<int>`,
/// `vector<int>`, `std::pmr::vector<int>` caen todos en `Contiguo`.
fn familia_de(tipo: &str) -> Option<Familia> {
    // El orden importa: `unordered_map` contiene `map` como substring, así
    // que lo no-ordenado se pregunta PRIMERO. Es el mismo tipo de trampa que
    // el `contains("mid")` que este módulo existe para arreglar.
    if tipo.contains("unordered_") {
        return Some(Familia::Hash);
    }
    if tipo.contains("forward_list") || tipo.contains("list") {
        return Some(Familia::Enlazado);
    }
    if tipo.contains("map") || tipo.contains("set") {
        return Some(Familia::Ordenado);
    }
    if tipo.contains("vector") || tipo.contains("string") || tipo.contains("deque") {
        return Some(Familia::Contiguo);
    }
    None
}

/// El costo de `familia::metodo`, y por qué, para el mensaje.
fn costo_de(familia: Familia, metodo: &str) -> Option<(Costo, &'static str)> {
    // Amortizadas en todas las familias donde existen: el realloc duplica
    // capacidad, así que su costo se reparte entre las inserciones previas.
    // Misma decisión que `amortizado.rs` toma del lado Python, y por el
    // mismo motivo: marcarlas convertiría el acumulador CORRECTO en un
    // falso positivo.
    if matches!(metodo, "push_back" | "emplace_back" | "push_front" | "emplace_front" | "append") {
        return Some((Costo::Constante, "it is amortized O(1)"));
    }

    match (familia, metodo) {
        (Familia::Contiguo, "insert" | "emplace" | "erase") => {
            Some((Costo::Lineal, "in a contiguous container it shifts every element after that position"))
        }
        (Familia::Contiguo, "find") => Some((Costo::Lineal, "a contiguous container has no index to search by")),
        (Familia::Ordenado, "insert" | "emplace" | "erase" | "find" | "count" | "at") => {
            Some((Costo::Logaritmico, "an ordered container is a balanced tree"))
        }
        (Familia::Hash, "insert" | "emplace" | "erase" | "find" | "count" | "at") => {
            Some((Costo::Constante, "a hash container is O(1) on average"))
        }
        (Familia::Enlazado, "insert" | "emplace" | "erase") => {
            Some((Costo::Constante, "splicing a linked node is O(1)"))
        }
        (Familia::Enlazado, "find") => Some((Costo::Lineal, "a linked container has to walk to find")),
        _ => None,
    }
}

/// Lo que se encontró: qué operación, de qué tipo, y a qué nivel de `n` llega.
#[derive(Debug, Clone)]
pub struct CostoCpp {
    pub operacion: String,
    pub costo: Costo,
    pub profundidad_bucle: u32,
    pub motivo: &'static str,
}

/// Mapa `nombre de variable -> familia`, armado de las declaraciones que se
/// ven en el archivo. Incluye parámetros, que es de donde vienen la mayoría
/// de los contenedores que una función muta.
fn declaraciones(raiz: Node, source: &str) -> HashMap<String, Familia> {
    let mut mapa = HashMap::new();
    tsnode::walk_flat(raiz, &mut |n| {
        if !matches!(n.kind(), "declaration" | "parameter_declaration" | "field_declaration") {
            return;
        }
        let Some(tipo) = n.child_by_field_name("type") else {
            return;
        };
        let Some(familia) = familia_de(text_of(tipo, source)) else {
            return;
        };
        // El declarador puede ser `x`, `x = {}`, `&x`, `*x`, `x[10]`: en
        // todos, el identificador que interesa es el primero que aparece.
        let mut cursor = n.walk();
        for hijo in n.children(&mut cursor) {
            if hijo.id() == tipo.id() {
                continue;
            }
            if let Some(nombre) = primer_identificador(hijo, source) {
                mapa.insert(nombre, familia);
            }
        }
    });
    mapa
}

fn primer_identificador(n: Node, source: &str) -> Option<String> {
    if n.kind() == "identifier" {
        return Some(text_of(n, source).to_string());
    }
    let mut cursor = n.walk();
    for hijo in n.children(&mut cursor) {
        if let Some(x) = primer_identificador(hijo, source) {
            return Some(x);
        }
    }
    None
}

const BUCLES: &[&str] = &["for_statement", "while_statement", "do_statement", "for_range_loop"];

/// Profundidad de bucles ANIDADOS (no cuántos hay), y las operaciones de
/// contenedor con el nivel al que llega cada una. Un solo recorrido.
pub fn analizar(func: Node, raiz: Node, source: &str) -> (u32, Vec<CostoCpp>, bool) {
    // **El mapa de tipos se arma GLOBAL y despues se pisa con lo LOCAL.**
    // Armarlo solo global era un defecto real: dos funciones del mismo
    // archivo con un parametro del mismo nombre y distinto tipo se pisaban
    // entre si, y ganaba la ultima declaracion del archivo. Lo encontro la
    // verificacion en navegador — el mismo archivo daba O(n^2) midiendo una
    // funcion sola y O(n) con las dos juntas. Lo global sigue haciendo
    // falta para ver campos y variables de fuera de la funcion.
    let mut tipos = declaraciones(raiz, source);
    tipos.extend(declaraciones(func, source));
    let mut cadenas = nombres_de_cadena(raiz, source);
    cadenas.extend(nombres_de_cadena(func, source));
    let mut max_depth = 0u32;
    let mut costos = Vec::new();
    let mut binario = false;

    #[allow(clippy::too_many_arguments)]
    fn walk(
        n: Node,
        depth: u32,
        source: &str,
        tipos: &HashMap<String, Familia>,
        cadenas: &std::collections::HashSet<String>,
        max_depth: &mut u32,
        costos: &mut Vec<CostoCpp>,
        binario: &mut bool,
    ) {
        if let Some(c) = costo_de_concatenacion(n, source, cadenas, depth) {
            costos.push(c);
        }
        let es_bucle = BUCLES.contains(&n.kind());
        let d = if es_bucle { depth + 1 } else { depth };
        if es_bucle {
            *max_depth = (*max_depth).max(d);
        }
        // La división binaria solo significa algo ADENTRO de un bucle: un
        // `/ 2` suelto no reduce ningún rango.
        if d > 0 && divide_a_la_mitad(n, source) {
            *binario = true;
        }
        if n.kind() == "call_expression" {
            if let Some(c) = costo_de_llamada(n, source, tipos, d) {
                costos.push(c);
            }
        }
        let mut cursor = n.walk();
        for hijo in n.children(&mut cursor) {
            walk(hijo, d, source, tipos, cadenas, max_depth, costos, binario);
        }
    }

    walk(func, 0, source, &tipos, &cadenas, &mut max_depth, &mut costos, &mut binario);
    (max_depth, costos, binario)
}

/// `x / 2`, `x >> 1`, `x /= 2`, `x >>= 1` — reducción a la mitad de verdad,
/// leída del árbol. **No** se mira ningún nombre de variable: ese era el
/// `contains("mid")` que hacía que `humidity` diera `O(log n)`.
fn divide_a_la_mitad(n: Node, source: &str) -> bool {
    let (op, derecha) = match n.kind() {
        "binary_expression" | "assignment_expression" => {
            let Some(op) = n.child_by_field_name("operator") else {
                return false;
            };
            let Some(der) = n.child_by_field_name("right") else {
                return false;
            };
            (text_of(op, source), der)
        }
        _ => return false,
    };
    let valor = text_of(derecha, source).trim();
    matches!((op, valor), ("/" | "/=", "2") | (">>" | ">>=", "1"))
}


/// Funciones LIBRES cuyo costo es lineal en el rango o la cadena que
/// reciben.
///
/// **Esto es lo que faltaba para que la tabla por familia sirviera de algo
/// en un vector.** `costo_de` tiene una fila que dice que `find` sobre un
/// contenedor contiguo es O(n), pero `std::vector` **no tiene** un miembro
/// `.find()`: la forma idiomática de buscar en un vector es
/// `std::find(v.begin(), v.end(), x)`, que es una función libre. Esa fila
/// solo podía dispararse para `std::string::find`, que sí existe. Medido:
/// un `std::find` dentro de un bucle daba O(n) en vez de O(n²).
const LIBRES_LINEALES: &[(&str, &str)] = &[
    ("find", "a linear scan over the whole range"),
    ("find_if", "a linear scan over the whole range"),
    ("count", "a linear scan over the whole range"),
    ("count_if", "a linear scan over the whole range"),
    ("search", "a linear scan over the whole range"),
    ("remove", "a linear scan over the whole range"),
    ("remove_if", "a linear scan over the whole range"),
    ("max_element", "a linear scan over the whole range"),
    ("min_element", "a linear scan over the whole range"),
    ("accumulate", "a linear scan over the whole range"),
    ("all_of", "a linear scan over the whole range"),
    ("any_of", "a linear scan over the whole range"),
    ("none_of", "a linear scan over the whole range"),
    // Las de C. `strlen` es la más valiosa de la lista por dónde aparece:
    // en la GUARDA de un `for`, donde se reevalúa en cada iteración.
    ("strlen", "it walks the string to its NUL terminator, every time it is called"),
    ("strcat", "it walks the destination to its end before copying"),
    ("strcpy", "it copies the whole string"),
    ("strcmp", "it compares up to the whole string"),
    ("memset", "it writes the whole block"),
    ("memcpy", "it copies the whole block"),
];

/// Búsquedas libres que NO son lineales, y por eso no entran arriba:
/// `std::binary_search`/`lower_bound`/`upper_bound` son O(log n) — marcarlas
/// convertiría el algoritmo CORRECTO en un falso positivo, que es el mismo
/// criterio por el que `push_back` no se marca.
const LIBRES_LOGARITMICAS: &[&str] = &["binary_search", "lower_bound", "upper_bound", "equal_range"];

/// El nombre de una llamada libre, con o sin calificar: tanto `std::find`
/// como `find` (tras un `using namespace std`) tienen que reconocerse.
fn nombre_de_funcion_libre<'a>(funcion: Node, source: &'a str) -> Option<&'a str> {
    match funcion.kind() {
        "identifier" => Some(text_of(funcion, source)),
        // `std::find` es un `qualified_identifier`; interesa la última parte.
        "qualified_identifier" => {
            let n = funcion.child_by_field_name("name")?;
            (n.kind() == "identifier").then(|| text_of(n, source))
        }
        _ => None,
    }
}

fn costo_de_llamada_libre(n: Node, source: &str, depth: u32) -> Option<CostoCpp> {
    let funcion = n.child_by_field_name("function")?;
    let nombre = nombre_de_funcion_libre(funcion, source)?;
    if LIBRES_LOGARITMICAS.contains(&nombre) {
        return None;
    }
    let (_, motivo) = LIBRES_LINEALES.iter().find(|(m, _)| *m == nombre)?;
    Some(CostoCpp { operacion: format!("{nombre}(…)"), costo: Costo::Lineal, profundidad_bucle: depth, motivo })
}


/// Nombres declarados con un tipo de cadena.
///
/// Hace falta aparte de `declaraciones` porque `Familia::Contiguo` junta
/// `vector`, `string` y `deque`, y el acumulador de concatenación solo
/// aplica a las cadenas: `v = v + w` no compila para un `std::vector`.
fn nombres_de_cadena(raiz: Node, source: &str) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    tsnode::walk_flat(raiz, &mut |n| {
        if !matches!(n.kind(), "declaration" | "parameter_declaration" | "field_declaration") {
            return;
        }
        let Some(tipo) = n.child_by_field_name("type") else { return };
        if !text_of(tipo, source).contains("string") {
            return;
        }
        let mut cursor = n.walk();
        let hijos: Vec<Node> = n.children(&mut cursor).collect();
        for hijo in hijos {
            if hijo.id() == tipo.id() {
                continue;
            }
            if let Some(nombre) = primer_identificador(hijo, source) {
                out.insert(nombre);
            }
        }
    });
    out
}

/// `true` si el subárbol contiene el identificador `nombre`.
fn menciona_identificador(n: Node, source: &str, nombre: &str) -> bool {
    let mut hay = false;
    tsnode::walk_flat(n, &mut |x| {
        if x.kind() == "identifier" && text_of(x, source) == nombre {
            hay = true;
        }
    });
    hay
}

/// El acumulador de concatenación: `s = s + x`.
///
/// **Es el par que se lee a un carácter de distancia y difiere
/// asintóticamente**, el mismo que `amortizado.rs` distingue del lado
/// Python entre `x = x + [y]` y `x += [y]`. `s += x` escribe al final del
/// buffer que la cadena ya tiene y es amortizado O(k); `s = s + x`
/// construye una cadena NUEVA con todo el contenido y la copia, o sea O(n)
/// por vuelta. En un bucle, eso es O(n²) — y las dos líneas se ven casi
/// iguales.
///
/// Se exige que la cadena se mencione a sí misma a la derecha: `s = a + b`
/// dentro de un bucle no crece, así que no es el defecto.
fn costo_de_concatenacion(n: Node, source: &str, cadenas: &std::collections::HashSet<String>, depth: u32) -> Option<CostoCpp> {
    if n.kind() != "assignment_expression" {
        return None;
    }
    let izq = n.child_by_field_name("left")?;
    if izq.kind() != "identifier" {
        return None;
    }
    // `+=` tiene su propio operador y NO es este caso: ahí está el punto.
    let op = n.child_by_field_name("operator")?;
    if text_of(op, source) != "=" {
        return None;
    }
    let nombre = text_of(izq, source);
    if !cadenas.contains(nombre) {
        return None;
    }
    let der = n.child_by_field_name("right")?;
    if der.kind() != "binary_expression" {
        return None;
    }
    let sub_op = der.child_by_field_name("operator")?;
    if text_of(sub_op, source) != "+" {
        return None;
    }
    if !menciona_identificador(der, source, nombre) {
        return None;
    }
    Some(CostoCpp {
        operacion: format!("{nombre} = {nombre} + …"),
        costo: Costo::Lineal,
        profundidad_bucle: depth,
        motivo: "`s = s + x` builds a NEW string with the whole content and copies it, while `s += x` appends into the buffer the string already has — the two read one character apart and differ asymptotically",
    })
}

fn costo_de_llamada(
    n: Node,
    source: &str,
    tipos: &HashMap<String, Familia>,
    depth: u32,
) -> Option<CostoCpp> {
    let funcion = n.child_by_field_name("function")?;
    if funcion.kind() != "field_expression" {
        // No es `obj.metodo(...)`: puede ser una función libre lineal.
        return costo_de_llamada_libre(n, source, depth);
    }
    let receptor = funcion.child_by_field_name("argument")?;
    let campo = funcion.child_by_field_name("field")?;
    let metodo = text_of(campo, source);
    // El receptor tiene que ser un nombre pelado para poder buscarlo: con
    // `a.b.c.insert(...)` no se sabe de qué tipo es `c` y no se afirma nada.
    if receptor.kind() != "identifier" {
        return None;
    }
    let nombre = text_of(receptor, source);
    let familia = *tipos.get(nombre)?;
    let (costo, motivo) = costo_de(familia, metodo)?;
    Some(CostoCpp {
        operacion: format!("{nombre}.{metodo}(…)"),
        costo,
        profundidad_bucle: depth,
        motivo,
    })
}

/// **Complejidad ciclomática de C/C++, leída del árbol.**
///
/// Lo que había era la misma técnica que el Big-O de arriba —contar `" if "`,
/// `" for "`, `" && "` sobre el texto— y fallaba en los dos sentidos. Medido
/// contra el sidecar vivo antes de reemplazarla, **6 de 9 casos daban mal**:
///
/// ```text
/// un comentario que nombra las keywords   real 1   decía 4   de MÁS
/// una cadena que nombra las keywords      real 1   decía 3   de MÁS
/// `if(x)` sin espacio                     real 2   decía 1   de MENOS
/// `a&&b` sin espacios                     real 2   decía 1   de MENOS
/// un operador ternario                    real 2   decía 1   de MENOS
/// un `catch` de C++                       real 2   decía 1   de MENOS
/// ```
///
/// Contar de MÁS y contar de MENOS son dos problemas distintos: lo primero
/// marca código simple como complejo (ruido), lo segundo deja pasar código
/// complejo (silencio). Leer el árbol arregla los dos de una, y los
/// comentarios y las cadenas dejan de contar **sin que haya que excluirlos**:
/// son nodos de otra clase y nunca se preguntan.
///
/// Los puntos de decisión son los mismos que `complexity.rs::cyclomatic`
/// cuenta del lado Python, traducidos a C/C++: `if` (y cada `else if`, que en
/// el árbol es otro `if_statement` anidado), los tres bucles, cada `case` con
/// valor, el ternario, cada `&&`/`||`, y cada `catch`.
pub fn ciclomatica(func: Node) -> u32 {
    fn walk(n: Node, cx: &mut u32) {
        match n.kind() {
            // Una función anidada tiene su propia complejidad.
            "function_definition" if n.parent().is_some() => {}
            "if_statement" | "for_statement" | "for_range_loop" | "while_statement" | "do_statement"
            | "conditional_expression" | "catch_clause" => *cx += 1,
            // `default:` también es un `case_statement`, pero no es una
            // decisión: es la rama que queda cuando ninguna se tomó. Se
            // distingue porque no tiene campo `value`.
            "case_statement" if n.child_by_field_name("value").is_some() => *cx += 1,
            "binary_expression" => {
                if let Some(op) = n.child_by_field_name("operator") {
                    // `text_of` necesita el fuente; acá alcanza con el kind
                    // del operador, que la gramática expone como token.
                    if matches!(op.kind(), "&&" | "||") {
                        *cx += 1;
                    }
                }
            }
            _ => {}
        }
        let mut cursor = n.walk();
        for hijo in n.children(&mut cursor) {
            if hijo.kind() == "function_definition" {
                continue;
            }
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

/// La etiqueta de Big-O para un exponente entero de `n`.
///
/// `pub(crate)` desde la Fase 34 (Linea E): `adaparse.rs` necesitaba la
/// misma tabla y copiarla habria sido la sexta duplicacion de este repo
/// -- las cinco copias de `walk` salieron caras de encontrar.
pub(crate) fn etiqueta(exponente: u32) -> String {
    match exponente {
        0 => "O(1)".into(),
        1 => "O(n)".into(),
        2 => "O(n²)".into(),
        3 => "O(n³)".into(),
        e => format!("O(n^{e})"),
    }
}

/// El reemplazo de `infer_big_o_c`: mismo contrato (devuelve etiqueta y
/// razón), pero leyendo el árbol en vez del texto.
pub fn inferir(func: Node, raiz: Node, source: &str) -> (String, String) {
    let (depth, costos, binario) = analizar(func, raiz, source);

    // El costo que manda: el que llega al nivel de `n` más alto.
    let dominante = costos
        .iter()
        .filter(|c| c.costo != Costo::Constante && c.profundidad_bucle > 0)
        .max_by_key(|c| (c.profundidad_bucle, c.costo == Costo::Lineal));

    if depth == 0 {
        return ("O(1)".into(), "sin loops".into());
    }

    match dominante {
        // Una operación logarítmica adentro de un bucle: n × log n.
        Some(c) if c.costo == Costo::Logaritmico && depth == 1 && !binario => (
            "O(n log n)".into(),
            format!("`{}` costs O(log n) per call — {}, and the loop runs n times", c.operacion, c.motivo),
        ),
        // Una operación lineal adentro de un bucle: sube un nivel.
        Some(c) if c.costo == Costo::Lineal => {
            let e = c.profundidad_bucle + 1;
            (
                etiqueta(e),
                format!(
                    "`{}` costs O(n) per call — {}. With {} loop{} around it, that multiplies out to {}",
                    c.operacion,
                    c.motivo,
                    c.profundidad_bucle,
                    if c.profundidad_bucle == 1 { "" } else { "s" },
                    etiqueta(e),
                ),
            )
        }
        _ if depth == 1 && binario => ("O(log n)".into(), "loop que reduce el rango a la mitad".into()),
        _ => (etiqueta(depth), razon_por_anidamiento(depth)),
    }
}

fn razon_por_anidamiento(depth: u32) -> String {
    match depth {
        1 => "un loop".into(),
        2 => "2 loops anidados".into(),
        d => format!("{d} loops anidados"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unordered_se_pregunta_antes_que_map() {
        // `unordered_map` contiene `map`: si se preguntara al revés, un hash
        // se clasificaría como árbol y `find` pasaría de O(1) a O(log n).
        assert_eq!(familia_de("std::unordered_map<int,int>"), Some(Familia::Hash));
        assert_eq!(familia_de("std::map<int,int>"), Some(Familia::Ordenado));
    }

    #[test]
    fn forward_list_no_se_confunde_con_otra_cosa() {
        assert_eq!(familia_de("std::forward_list<int>"), Some(Familia::Enlazado));
        assert_eq!(familia_de("std::list<int>"), Some(Familia::Enlazado));
    }

    #[test]
    fn un_tipo_que_no_es_contenedor_no_tiene_familia() {
        assert_eq!(familia_de("int"), None);
        assert_eq!(familia_de("MiClase"), None);
    }

    #[test]
    fn el_mismo_metodo_cuesta_distinto_segun_la_familia() {
        // El punto entero del rastreo de declaraciones, en un test.
        assert_eq!(costo_de(Familia::Contiguo, "insert").unwrap().0, Costo::Lineal);
        assert_eq!(costo_de(Familia::Ordenado, "insert").unwrap().0, Costo::Logaritmico);
        assert_eq!(costo_de(Familia::Hash, "insert").unwrap().0, Costo::Constante);
        assert_eq!(costo_de(Familia::Enlazado, "insert").unwrap().0, Costo::Constante);
    }

    #[test]
    fn push_back_es_constante_en_toda_familia() {
        for f in [Familia::Contiguo, Familia::Ordenado, Familia::Hash, Familia::Enlazado] {
            assert_eq!(costo_de(f, "push_back").unwrap().0, Costo::Constante);
        }
    }

    #[test]
    fn la_etiqueta_usa_superindices_hasta_el_cubo() {
        assert_eq!(etiqueta(1), "O(n)");
        assert_eq!(etiqueta(2), "O(n²)");
        assert_eq!(etiqueta(3), "O(n³)");
        assert_eq!(etiqueta(4), "O(n^4)");
    }

    // ── Fase 32: funciones LIBRES (2026-09-25) ──────────────────────────

    fn bigo_cpp(src: &str) -> (String, String) {
        let mut p = tree_sitter::Parser::new();
        p.set_language(&tree_sitter_cpp::LANGUAGE.into()).unwrap();
        let t = p.parse(src, None).unwrap();
        let raiz = t.root_node();
        let mut func = None;
        crate::ast::tsnode::walk_flat(raiz, &mut |n| {
            if n.kind() == "function_definition" && func.is_none() {
                func = Some(n);
            }
        });
        inferir(func.expect("sin función"), raiz, src)
    }

    /// **El caso que hacía inútil la fila de la tabla**: `std::vector` no
    /// tiene un miembro `.find()`, así que buscar en un vector se escribe
    /// `std::find(v.begin(), v.end(), x)` — una función LIBRE. Dentro de un
    /// bucle eso es cuadrático, y se reportaba O(n).
    #[test]
    fn std_find_dentro_de_un_bucle_es_cuadratico() {
        let src = "#include <algorithm>\n#include <vector>\nvoid f(const std::vector<int> &v, const std::vector<int> &k) {\n    for (size_t i = 0; i < k.size(); i++) {\n        std::find(v.begin(), v.end(), k[i]);\n    }\n}\n";
        let (bigo, motivo) = bigo_cpp(src);
        assert_eq!(bigo, "O(n²)", "{motivo}");
        assert!(motivo.contains("linear scan"), "{motivo}");
    }

    /// **`strlen` en la GUARDA de un `for` es el O(n²) clásico de C**: se
    /// reevalúa en cada iteración, y ahí no lo ve nadie.
    #[test]
    fn strlen_en_la_guarda_de_un_for_es_cuadratico() {
        let src = "#include <cstring>\nvoid f(const char *s) {\n    for (size_t i = 0; i < strlen(s); i++) { }\n}\n";
        let (bigo, motivo) = bigo_cpp(src);
        assert_eq!(bigo, "O(n²)", "{motivo}");
        assert!(motivo.contains("every time it is called"), "{motivo}");
    }

    /// **Y la contraparte que evita el falso positivo**: `std::binary_search`
    /// y compañía son O(log n). Marcarlas convertiría el algoritmo CORRECTO
    /// en un hallazgo, que es el mismo criterio por el que `push_back` no se
    /// marca.
    #[test]
    fn las_busquedas_binarias_libres_no_se_marcan() {
        let src = "#include <algorithm>\n#include <vector>\nvoid f(const std::vector<int> &v, int n) {\n    for (int i = 0; i < n; i++) {\n        std::binary_search(v.begin(), v.end(), i);\n    }\n}\n";
        assert_eq!(bigo_cpp(src).0, "O(n)");
    }

    /// Tanto `std::find` como `find` tras un `using namespace std`.
    #[test]
    fn el_nombre_se_reconoce_calificado_y_sin_calificar() {
        let calificado = "#include <algorithm>\nvoid f(int n) {\n    for (int i = 0; i < n; i++) { std::find(a, b, i); }\n}\n";
        let pelado = "#include <algorithm>\nusing namespace std;\nvoid f(int n) {\n    for (int i = 0; i < n; i++) { find(a, b, i); }\n}\n";
        assert_eq!(bigo_cpp(calificado).0, "O(n²)");
        assert_eq!(bigo_cpp(pelado).0, "O(n²)");
    }

    /// Una función libre que no está en la tabla no cambia nada: no se
    /// inventa un costo para lo que no se midió.
    #[test]
    fn una_funcion_libre_desconocida_no_cambia_el_bigo() {
        let src = "void f(int n) {\n    for (int i = 0; i < n; i++) { mi_funcion(i); }\n}\n";
        assert_eq!(bigo_cpp(src).0, "O(n)");
    }

    /// **El par que se lee a un caracter de distancia y difiere
    /// asintoticamente** — el mismo que `amortizado.rs` distingue del lado
    /// Python entre `x = x + [y]` y `x += [y]`.
    #[test]
    fn el_acumulador_de_concatenacion_es_cuadratico_y_el_mas_igual_no() {
        let malo = "#include <string>\n#include <vector>\nstd::string f(const std::vector<std::string> &p) {\n    std::string s;\n    for (size_t i = 0; i < p.size(); i++) { s = s + p[i]; }\n    return s;\n}\n";
        let bueno = "#include <string>\n#include <vector>\nstd::string f(const std::vector<std::string> &p) {\n    std::string s;\n    for (size_t i = 0; i < p.size(); i++) { s += p[i]; }\n    return s;\n}\n";
        let (bm, motivo) = bigo_cpp(malo);
        assert_eq!(bm, "O(n²)", "{motivo}");
        assert!(motivo.contains("one character apart"), "{motivo}");
        assert_eq!(bigo_cpp(bueno).0, "O(n)", "`+=` escribe en el buffer que la cadena ya tiene");
    }

    /// Y se exige que la cadena se mencione a si misma: `s = a + b` dentro
    /// de un bucle no crece, asi que no es el defecto.
    #[test]
    fn una_concatenacion_que_no_se_acumula_no_se_marca() {
        let src = "#include <string>\nstd::string f(const std::string &a, const std::string &b, int n) {\n    std::string s;\n    for (int i = 0; i < n; i++) { s = a + b; }\n    return s;\n}\n";
        assert_eq!(bigo_cpp(src).0, "O(n)");
    }

    /// Un `vector` no entra: `v = v + w` no compila, y por eso el mapa de
    /// cadenas es aparte del de familias.
    #[test]
    fn el_acumulador_solo_aplica_a_cadenas() {
        let src = "#include <vector>\nvoid f(std::vector<int> v, int n) {\n    for (int i = 0; i < n; i++) { v = v + v; }\n}\n";
        assert_eq!(bigo_cpp(src).0, "O(n)");
    }

    /// **Defecto que encontro la verificacion en navegador**: el mapa de
    /// tipos era global al archivo, asi que dos funciones con un parametro
    /// del mismo nombre y distinto tipo se pisaban, y ganaba la ultima del
    /// archivo. Medido: `busca_en_vector` daba O(n) en vez de O(n^2).
    #[test]
    fn dos_funciones_con_un_parametro_homonimo_no_se_pisan() {
        let src = "#include <vector>\n#include <unordered_set>\nint a(std::vector<int> v, int n) {\n    for (int i = 0; i < n; i++) { v.insert(v.begin(), 1); }\n    return 0;\n}\nint b(std::unordered_set<int> v, int n) {\n    for (int i = 0; i < n; i++) { v.insert(1); }\n    return 0;\n}\n";
        let mut p = tree_sitter::Parser::new();
        p.set_language(&tree_sitter_cpp::LANGUAGE.into()).unwrap();
        let t = p.parse(src, None).unwrap();
        let raiz = t.root_node();
        let mut funcs = Vec::new();
        crate::ast::tsnode::walk_flat(raiz, &mut |n| {
            if n.kind() == "function_definition" {
                funcs.push(n);
            }
        });
        assert_eq!(inferir(funcs[0], raiz, src).0, "O(n²)", "el vector corre la cola");
        assert_eq!(inferir(funcs[1], raiz, src).0, "O(n)", "la tabla hash no");
    }
}
