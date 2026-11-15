//! **Big-O y complejidad ciclomática de JS/TS sobre el ÁRBOL.**
//!
//! `jsts.rs` extrae funciones con expresiones regulares y después mide sobre
//! el TEXTO del cuerpo (`LOOP_KW_RE.find_iter(body).count()`,
//! `body.contains("Math.floor")`). Medido contra el sidecar vivo antes de
//! escribir este módulo, **3 de 5 casos de Big-O y 3 de 8 de complejidad
//! daban mal**:
//!
//! ```text
//! dos bucles SECUENCIALES                real O(n)   decía O(n²)      FALSO
//! un bucle con `Math.floor` adentro       real O(n)   decía O(log n)   FALSO
//! un comentario que nombra las keywords   real O(1)   decía O(n²)      FALSO
//!
//! un comentario que nombra las keywords   real CC 1   decía CC 4       de MÁS
//! una cadena que nombra las keywords      real CC 1   decía CC 4       de MÁS
//! `if(x)` sin espacio                     real CC 2   decía CC 1       de MENOS
//! ```
//!
//! El tercero es el más ilustrativo: **una función sin un solo bucle**,
//! apenas con un comentario que dice "loop for each item while…", se
//! reportaba `O(n²)`.
//!
//! **`Math.floor` como señal de división binaria merece mención aparte.** En
//! C el falso positivo era `contains("mid")`; acá es peor, porque
//! `Math.floor` es la forma normal de truncar cualquier cosa en JavaScript —
//! precios, índices, porcentajes. La reducción a la mitad ahora se lee del
//! árbol (`/ 2`, `>> 1`, `>>> 1`, y sus formas compuestas) y **solo cuenta
//! adentro de un bucle**.
//!
//! **Y después también la EXTRACCIÓN**, que fallaba de cinco formas más,
//! todas medidas contra el sidecar antes de portarla:
//!
//! - **Los métodos de clase no aparecían.** Una `class` entera devolvía cero
//!   funciones. Es el peor de los cinco: JS/TS moderno está lleno de clases,
//!   así que no era un caso de borde sino la mitad del lenguaje.
//! - **Los métodos de un objeto literal, tampoco.**
//! - **Una función escrita adentro de un COMENTARIO se extraía**, porque el
//!   texto la contiene.
//! - **Se deduplicaba por NOMBRE**, así que dos `handler` en scopes distintos
//!   perdían uno.
//! - **`estimate_func_loc` contaba llaves sobre el texto**, de modo que una
//!   `"}"` adentro de una cadena cortaba la función: una de 5 líneas
//!   reportaba `loc` 2.
//!
//! El árbol resuelve los cinco por construcción, no con parches. El
//! extractor por regex sigue en `jsts.rs` como reserva para el caso en que
//! la gramática no pueda parsear un archivo: degradar a algo imperfecto es
//! mejor que devolver una lista vacía, y está dicho que es eso.
//!
//! **Y por último el COSTO DE LAS OPERACIONES DE CONTENEDOR**, que completa
//! el trío con `amortizado.rs` (Python) y `ccosto.rs` (C++). Medido antes:
//!
//! ```text
//! `unshift` en un bucle          real O(n²)  decía O(n)   FALSO
//! `splice` en un bucle           real O(n²)  decía O(n)   FALSO
//! `includes` en un bucle         real O(n²)  decía O(n)   FALSO
//! `out = [...out, x]` en bucle   real O(n²)  decía O(n)   FALSO
//! `push` en un bucle             real O(n)   decía O(n)   ok
//! `Set.has` en un bucle          real O(n)   decía O(n)   ok
//! ```
//!
//! **JS no tiene declaraciones de tipo donde apoyarse como C++, pero sí
//! tiene el valor que se le asigna a la variable.** `const out = []` dice
//! que `out` es un arreglo; `new Set()` dice que no lo es. Eso alcanza, y
//! donde no alcanza —un receptor que no se sabe qué es— **no se afirma
//! nada**. El `Set` se rastrea justamente para NO marcarlo, y porque "usá un
//! Set" solo es una recomendación honesta si el motor sabe distinguirlos.
//!
//! **La concatenación de cadenas queda AFUERA, y ahí está lo interesante.**
//! En Python `s = s + t` dentro de un bucle es O(n²) y `amortizado.rs` lo
//! marca. En JavaScript **no**: V8, SpiderMonkey y JSC usan *cons-strings*,
//! así que concatenar arma un nodo de árbol en O(1) y el aplanado es
//! perezoso. **La misma forma de código tiene otra complejidad porque cambia
//! el runtime, no la sintaxis.** Copiar la regla de Python acá habría sido
//! inventar un hallazgo, y por eso hay un test que exige que NO salga.

use std::collections::HashMap;

use tree_sitter::{Node, Parser};

use crate::hallazgos::smells::StructuralSmell;

/// Lo que este módulo le devuelve a `jsts.rs` por función.
#[derive(Debug, Clone)]
pub struct MetricasJs {
    pub complexity: u32,
    pub big_o: String,
    pub big_o_reason: String,
    pub name: String,
    /// 1-based, como todo lo que sale del motor.
    pub line: usize,
    pub end_line: usize,
    pub loc: usize,
    pub is_async: bool,
    pub params: Vec<String>,
    pub calls: Vec<String>,
    /// Operaciones de costo O(n) por llamada que caen adentro de un
    /// bucle. Vacio en la mayoria de las funciones.
    pub costos: Vec<CostoJs>,
}

/// El nombre de una función, que en JS puede venir de cuatro lugares.
///
/// `function f(){}` y `metodo(){}` lo llevan en su campo `name`. Una arrow o
/// una `function` anónima no tienen nombre propio: lo toman de **quién las
/// recibe** — `const suma = (a,b) => …` del `variable_declarator`, y
/// `{ flecha: (w) => w }` del `pair`. Si no la recibe nadie con nombre, es
/// anónima de verdad y se dice así en vez de inventar uno.
fn nombre_de(n: Node, source: &str) -> String {
    if let Some(nombre) = n.child_by_field_name("name") {
        return texto(nombre, source).to_string();
    }
    match n.parent().map(|p| (p.kind(), p)) {
        Some(("variable_declarator", p)) => p
            .child_by_field_name("name")
            .map(|x| texto(x, source).to_string())
            .unwrap_or_else(|| "<anonymous>".into()),
        Some(("pair", p)) => p
            .child_by_field_name("key")
            .map(|x| texto(x, source).to_string())
            .unwrap_or_else(|| "<anonymous>".into()),
        _ => "<anonymous>".into(),
    }
}

fn texto<'a>(n: Node, source: &'a str) -> &'a str {
    &source[n.start_byte()..n.end_byte()]
}

/// `async` es un token hijo de la función, en las cuatro formas.
fn es_async(n: Node) -> bool {
    let mut c = n.walk();
    let hay = n.children(&mut c).any(|h| h.kind() == "async");
    hay
}

/// Nombres de los parámetros. Una arrow puede tener un parámetro pelado sin
/// paréntesis (`x => x + 1`), que no produce un `formal_parameters`.
fn parametros(n: Node, source: &str) -> Vec<String> {
    let Some(lista) = n.child_by_field_name("parameters") else {
        return match n.child_by_field_name("parameter") {
            Some(p) => vec![texto(p, source).to_string()],
            None => Vec::new(),
        };
    };
    let mut cursor = lista.walk();
    lista
        .children(&mut cursor)
        .filter(|h| h.is_named() && h.kind() != "comment")
        .map(|h| texto(h, source).to_string())
        .collect()
}

/// A quién llama esta función, sin contar lo que llaman las de adentro.
fn llamadas(func: Node, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(n: Node, source: &str, out: &mut Vec<String>) {
        if n.kind() == "call_expression" {
            if let Some(f) = n.child_by_field_name("function") {
                let nombre = match f.kind() {
                    "identifier" => Some(texto(f, source).to_string()),
                    // `a.b.c()` se reporta como `c`, igual que hacía el
                    // extractor por regex: lo que interesa es qué se invoca.
                    "member_expression" => {
                        f.child_by_field_name("property").map(|p| texto(p, source).to_string())
                    }
                    _ => None,
                };
                if let Some(x) = nombre {
                    if !out.contains(&x) {
                        out.push(x);
                    }
                }
            }
        }
        let mut cursor = n.walk();
        for hijo in n.children(&mut cursor) {
            if FUNCIONES.contains(&hijo.kind()) {
                continue;
            }
            walk(hijo, source, out);
        }
    }
    let mut cursor = func.walk();
    for hijo in func.children(&mut cursor) {
        if FUNCIONES.contains(&hijo.kind()) {
            continue;
        }
        walk(hijo, source, &mut out);
    }
    out
}

/// Qué es una variable, hasta donde se puede saber sin tipos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TipoJs {
    /// Un arreglo: acceso por índice, y mutar el principio corre la cola.
    Arreglo,
    /// `Set`/`Map`: búsqueda y alta en O(1) promedio. **Se rastrea para NO
    /// marcarlo**, y porque es la recomendación concreta del hallazgo de
    /// `includes` — "usá un Set" solo vale si el motor sabe distinguirlos.
    Conjunto,
}

/// Métodos de arreglo que cuestan O(n) por llamada, con el motivo.
const ARREGLO_LINEAL: &[(&str, &str)] = &[
    ("shift", "removing from the front reindexes every remaining element"),
    ("unshift", "inserting at the front reindexes every existing element"),
    ("splice", "it reindexes every element after that position"),
    ("indexOf", "an array has no index to search by, so it scans"),
    ("lastIndexOf", "an array has no index to search by, so it scans"),
    ("includes", "an array has no index to search by, so it scans"),
];

/// Lo que NO se marca, escrito para que se lea como decisión y no como
/// olvido. `push`/`pop` son O(1) amortizado — el mismo razonamiento que en
/// `amortizado.rs` del lado Python.
const ARREGLO_CONSTANTE: &[&str] = &["push", "pop"];

/// `Set`/`Map`: todo lo que importa acá es O(1) promedio.
const CONJUNTO_CONSTANTE: &[&str] = &["has", "get", "add", "set", "delete"];

/// De qué tipo es el valor que se le asigna a una variable.
///
/// Es lo más cerca de una declaración que hay en JS. No pretende ser
/// inferencia de tipos: si el valor no es una de estas formas, no se sabe, y
/// no saber significa **no afirmar nada**.
fn tipo_del_valor(valor: Node, source: &str) -> Option<TipoJs> {
    match valor.kind() {
        "array" => Some(TipoJs::Arreglo),
        "new_expression" => {
            let c = valor.child_by_field_name("constructor")?;
            match texto(c, source) {
                "Array" => Some(TipoJs::Arreglo),
                "Set" | "Map" | "WeakSet" | "WeakMap" => Some(TipoJs::Conjunto),
                _ => None,
            }
        }
        "call_expression" => {
            let f = valor.child_by_field_name("function")?;
            if f.kind() != "member_expression" {
                return None;
            }
            let prop = texto(f.child_by_field_name("property")?, source);
            let receptor = f.child_by_field_name("object").map(|o| texto(o, source));
            match (receptor, prop) {
                // `Array.from(...)` / `Array.of(...)` — se exige el receptor
                // exacto porque un `from` cualquiera no dice nada.
                (Some("Array"), "from" | "of") => Some(TipoJs::Arreglo),
                // `Object.keys` devuelve un arreglo; `map.keys()` devuelve un
                // iterador. Sin el receptor no se pueden separar.
                (Some("Object"), "keys" | "values" | "entries") => Some(TipoJs::Arreglo),
                (_, "split" | "map" | "filter" | "slice" | "flat") => Some(TipoJs::Arreglo),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Mapa `variable -> tipo`, armado de las asignaciones visibles del archivo.
fn tipos_de_variables(raiz: Node, source: &str) -> HashMap<String, TipoJs> {
    let mut mapa = HashMap::new();
    fn walk(n: Node, source: &str, mapa: &mut HashMap<String, TipoJs>) {
        if matches!(n.kind(), "variable_declarator" | "assignment_expression") {
            if let (Some(nombre), Some(valor)) =
                (n.child_by_field_name("name").or_else(|| n.child_by_field_name("left")), n.child_by_field_name("value").or_else(|| n.child_by_field_name("right")))
            {
                if nombre.kind() == "identifier" {
                    if let Some(t) = tipo_del_valor(valor, source) {
                        mapa.insert(texto(nombre, source).to_string(), t);
                    }
                }
            }
        }
        let mut cursor = n.walk();
        for hijo in n.children(&mut cursor) {
            walk(hijo, source, mapa);
        }
    }
    walk(raiz, source, &mut mapa);
    mapa
}

/// Una operación de costo O(n) por llamada, y a qué nivel de `n` llega.
#[derive(Debug, Clone)]
pub struct CostoJs {
    pub operacion: String,
    pub motivo: &'static str,
    pub profundidad_bucle: u32,
    pub nivel_efectivo: u32,
    pub linea: usize,
}

/// `x = [...x, y]` y `x = x.concat(y)` — **reconstruyen el arreglo entero**.
///
/// No hace falta rastrear el tipo acá: que la variable aparezca a los dos
/// lados ES la prueba. Es la misma forma que `x = x + [y]` en Python.
///
/// **Y la cadena NO entra, que es la diferencia con Python.** `s += t` en un
/// bucle es O(n²) en CPython, pero V8/SpiderMonkey/JSC usan *cons-strings*:
/// concatenar arma un nodo de árbol en O(1) y el aplanado es perezoso. La
/// misma forma de código tiene otra complejidad porque cambia el runtime, no
/// la sintaxis — copiar la regla de Python acá sería inventar un hallazgo.
fn autocopia(n: Node, source: &str) -> Option<String> {
    if n.kind() != "assignment_expression" {
        return None;
    }
    let izq = n.child_by_field_name("left")?;
    if izq.kind() != "identifier" {
        return None;
    }
    let nombre = texto(izq, source);
    let der = n.child_by_field_name("right")?;

    match der.kind() {
        "array" => {
            let mut cursor = der.walk();
            let hay = der.children(&mut cursor).any(|h| {
                h.kind() == "spread_element"
                    && h.named_child(0).is_some_and(|a| a.kind() == "identifier" && texto(a, source) == nombre)
            });
            hay.then(|| format!("{nombre} = [...{nombre}, …]"))
        }
        "call_expression" => {
            let f = der.child_by_field_name("function")?;
            if f.kind() != "member_expression" {
                return None;
            }
            let recibe = texto(f.child_by_field_name("object")?, source) == nombre;
            let concat = texto(f.child_by_field_name("property")?, source) == "concat";
            (recibe && concat).then(|| format!("{nombre} = {nombre}.concat(…)"))
        }
        _ => None,
    }
}

/// El costo de una llamada a método, si el receptor es una variable de tipo
/// conocido. Receptor desconocido → `None`, que es callarse.
fn costo_de_llamada_js(
    n: Node,
    source: &str,
    tipos: &HashMap<String, TipoJs>,
) -> Option<(String, &'static str)> {
    let f = n.child_by_field_name("function")?;
    if f.kind() != "member_expression" {
        return None;
    }
    let receptor = f.child_by_field_name("object")?;
    if receptor.kind() != "identifier" {
        return None;
    }
    let nombre = texto(receptor, source);
    let metodo = texto(f.child_by_field_name("property")?, source);

    match tipos.get(nombre)? {
        TipoJs::Conjunto => {
            let _ = CONJUNTO_CONSTANTE;
            None
        }
        TipoJs::Arreglo => {
            if ARREGLO_CONSTANTE.contains(&metodo) {
                return None;
            }
            ARREGLO_LINEAL
                .iter()
                .find(|(m, _)| *m == metodo)
                .map(|(_, motivo)| (format!("{nombre}.{metodo}(…)"), *motivo))
        }
    }
}

/// **La superficie de prueba de TypeScript — la misma idea que en Rust, con
/// las herramientas que da este lenguaje.**
///
/// `rsparse.rs::ProofSurface` mide dónde un programa Rust tenía una
/// demostración y la tiró. TypeScript tiene exactamente los mismos lugares,
/// con otra sintaxis: un tipo es una proposición, y cada una de estas formas
/// le dice al compilador *"confiá en mí"* en vez de demostrárselo.
///
/// **Y la simetría justificado/sin justificar también existe acá, y es más
/// nítida que en Rust.** `@ts-expect-error` **falla si el error desaparece**:
/// obliga a sacarlo cuando deja de hacer falta. `@ts-ignore` calla para
/// siempre y no avisa nunca que ya sobra. Es la misma relación que hay entre
/// `.expect("por qué")` y `.unwrap()`, o entre un `unsafe` con `// SAFETY:` y
/// uno sin él — la forma que deja escrito el razonamiento contra la que no.
#[derive(Debug, Clone, Default)]
pub struct ProofSurfaceTs {
    /// `x!` — el gemelo exacto de `.unwrap()`: descarta la prueba de que
    /// podía ser `null`.
    pub non_null_assertions: usize,
    /// `x as T`. Cuenta cada `as` escrito, así que `x as unknown as T`
    /// suma 2: son dos aserciones, no una.
    pub type_assertions: usize,
    /// `x as unknown as T` — la escotilla doble. Pasar por `unknown`
    /// **desactiva el chequeo de compatibilidad que TypeScript le hace a un
    /// `as` normal**, que es lo único que impide asertar cualquier cosa a
    /// cualquier cosa. Por eso se cuenta aparte.
    pub double_assertions: usize,
    /// `: any` — descarta toda la información de tipo de una vez.
    pub any_annotations: usize,
    /// `@ts-ignore` — calla el error para siempre y nunca avisa que sobra.
    pub ts_ignore: usize,
    /// `@ts-expect-error` — la forma justificada: falla si el error
    /// desaparece, así que no puede quedar podrido.
    pub ts_expect_error: usize,
}

/// `true` si este `as_expression` asierta a `unknown` — el primer paso de la
/// escotilla doble.
fn asierta_a_unknown(n: Node, source: &str) -> bool {
    n.kind() == "as_expression" && texto(n, source).trim_end().ends_with("unknown")
}

fn superficie_de_prueba(raiz: Node, source: &str) -> ProofSurfaceTs {
    let mut p = ProofSurfaceTs::default();
    fn walk(n: Node, source: &str, p: &mut ProofSurfaceTs) {
        match n.kind() {
            "non_null_expression" => p.non_null_assertions += 1,
            "as_expression" => {
                p.type_assertions += 1;
                // `x as unknown as T`: el hijo izquierdo es a su vez un
                // `as_expression` que termina en `unknown`.
                if n.named_child(0).is_some_and(|h| asierta_a_unknown(h, source)) {
                    p.double_assertions += 1;
                }
            }
            "predefined_type" if texto(n, source).trim() == "any" => p.any_annotations += 1,
            "comment" => {
                let t = texto(n, source);
                // El orden importa: `@ts-expect-error` NO contiene
                // `@ts-ignore`, pero preguntar primero por el que justifica
                // deja la intención clara y evita un futuro solapamiento.
                if t.contains("@ts-expect-error") {
                    p.ts_expect_error += 1;
                } else if t.contains("@ts-ignore") {
                    p.ts_ignore += 1;
                }
            }
            _ => {}
        }
        let mut c = n.walk();
        for h in n.children(&mut c) {
            walk(h, source, p);
        }
    }
    walk(raiz, source, &mut p);
    p
}

/// Hallazgos de la superficie de prueba. Dos, los dos con alternativa
/// concreta — no se marca todo lo que se cuenta.
pub fn proof_smells(source: &str, es_typescript: bool) -> Vec<StructuralSmell> {
    if !es_typescript {
        return Vec::new();
    }
    let Some(arbol) = parsear(source, true) else {
        return Vec::new();
    };
    let raiz = arbol.root_node();
    let mut out = Vec::new();
    fn walk(n: Node, source: &str, out: &mut Vec<StructuralSmell>) {
        if n.kind() == "comment" {
            let t = texto(n, source);
            if !t.contains("@ts-expect-error") && t.contains("@ts-ignore") {
                out.push(StructuralSmell {
                    kind: "ts_ignore_instead_of_expect_error",
                    name: "<file>".to_string(),
                    line: n.start_position().row + 1,
                    message: "`@ts-ignore` silences the error forever and never tells you when it is no longer needed. `@ts-expect-error` discards the same proof but **fails if the error goes away**, so it cannot rot: the day the underlying type is fixed, the compiler asks you to remove the suppression. Same relationship as `.expect(\"why\")` to `.unwrap()` — the form that writes down the reasoning against the one that does not.".to_string(),
                });
            }
        }
        if n.kind() == "as_expression" && n.named_child(0).is_some_and(|h| asierta_a_unknown(h, source)) {
            out.push(StructuralSmell {
                kind: "double_type_assertion",
                name: "<file>".to_string(),
                line: n.start_position().row + 1,
                message: "`x as unknown as T` is not a stronger assertion than `x as T`: it is the escape hatch that **turns off the one check TypeScript still runs on an assertion**. A plain `as` requires the two types to overlap; routing through `unknown` removes that requirement, so anything can be asserted to anything. It is the TypeScript equivalent of a `transmute`.".to_string(),
            });
        }
        let mut c = n.walk();
        for h in n.children(&mut c) {
            walk(h, source, out);
        }
    }
    walk(raiz, source, &mut out);
    out
}

/// Las funciones del archivo con su rango de líneas (0-based, inclusivo).
pub struct MapaDeFunciones {
    entradas: Vec<(usize, usize, MetricasJs)>,
}

impl MapaDeFunciones {
    /// Todas las funciones del archivo, en orden de aparición — incluidas
    /// las anidadas, que son funciones propias con su propio `n`.
    pub fn funciones(&self) -> impl Iterator<Item = &MetricasJs> {
        self.entradas.iter().map(|(_, _, m)| m)
    }
}

const FUNCIONES: &[&str] = &[
    "function_declaration",
    "function_expression",
    "generator_function_declaration",
    "generator_function",
    "arrow_function",
    "method_definition",
];

const BUCLES: &[&str] =
    &["for_statement", "for_in_statement", "while_statement", "do_statement"];

/// `x / 2`, `x >> 1`, `x >>> 1` y sus formas compuestas. **No** se mira
/// ningún nombre: `Math.floor` dejó de ser una señal, que es el punto.
fn divide_a_la_mitad(n: Node, source: &str) -> bool {
    if !matches!(n.kind(), "binary_expression" | "augmented_assignment_expression") {
        return false;
    }
    let (Some(op), Some(der)) = (n.child_by_field_name("operator"), n.child_by_field_name("right"))
    else {
        return false;
    };
    let valor = source[der.start_byte()..der.end_byte()].trim();
    matches!(
        (op.kind(), valor),
        ("/" | "/=", "2") | (">>" | ">>=", "1") | (">>>" | ">>>=", "1")
    )
}

/// Recorre el cuerpo de UNA función sin entrar a las funciones de adentro:
/// una función anidada tiene su propio `n` y su propia complejidad.
fn medir(func: Node, source: &str, tipos: &HashMap<String, TipoJs>) -> MetricasJs {
    let mut cc = 1u32;
    let mut max_depth = 0u32;
    let mut binario = false;
    let mut costos: Vec<CostoJs> = Vec::new();

    #[allow(clippy::too_many_arguments)]
    fn walk(
        n: Node,
        depth: u32,
        source: &str,
        tipos: &HashMap<String, TipoJs>,
        cc: &mut u32,
        max_depth: &mut u32,
        binario: &mut bool,
        costos: &mut Vec<CostoJs>,
    ) {
        let es_bucle = BUCLES.contains(&n.kind());
        let d = if es_bucle { depth + 1 } else { depth };
        if es_bucle {
            *max_depth = (*max_depth).max(d);
        }

        match n.kind() {
            k if BUCLES.contains(&k) => *cc += 1,
            "if_statement" | "switch_case" | "catch_clause" | "ternary_expression" => *cc += 1,
            // `switch_default` es un kind propio en esta gramática (a
            // diferencia de C, donde `default:` es un `case_statement` sin
            // valor) y no cuenta: es la rama que queda cuando ninguna se tomó.
            "binary_expression" => {
                if let Some(op) = n.child_by_field_name("operator") {
                    // `?.` es `optional_chain`, no un operador binario, así
                    // que no llega acá — que es lo correcto: acceder a algo
                    // que puede faltar no es una decisión del programa.
                    if matches!(op.kind(), "&&" | "||" | "??") {
                        *cc += 1;
                    }
                }
            }
            _ => {}
        }

        // La división binaria solo significa algo ADENTRO de un bucle: un
        // `/ 2` suelto no reduce ningún rango.
        if d > 0 && divide_a_la_mitad(n, source) {
            *binario = true;
        }

        let hallado = autocopia(n, source)
            .map(|op| (op, "it rebuilds the whole array and copies every element into it"))
            .or_else(|| {
                (n.kind() == "call_expression")
                    .then(|| costo_de_llamada_js(n, source, tipos))
                    .flatten()
            });
        if let Some((operacion, motivo)) = hallado {
            costos.push(CostoJs {
                operacion,
                motivo,
                profundidad_bucle: d,
                nivel_efectivo: d + 1,
                linea: n.start_position().row + 1,
            });
        }

        let mut cursor = n.walk();
        for hijo in n.children(&mut cursor) {
            if FUNCIONES.contains(&hijo.kind()) {
                continue;
            }
            walk(hijo, d, source, tipos, cc, max_depth, binario, costos);
        }
    }

    let mut cursor = func.walk();
    for hijo in func.children(&mut cursor) {
        if FUNCIONES.contains(&hijo.kind()) {
            continue;
        }
        walk(hijo, 0, source, tipos, &mut cc, &mut max_depth, &mut binario, &mut costos);
    }

    // El costo que manda: el que llega al nivel de `n` mas alto, y solo
    // si esta ADENTRO de un bucle. Mismo criterio que `amortizado.rs`:
    // con `binario` no se ajusta, porque log n x O(n) es O(n log n) y no
    // O(n^2) — subir la profundidad exageraria.
    let dominante = costos.iter().filter(|c| c.profundidad_bucle > 0).max_by_key(|c| c.nivel_efectivo);
    let profundidad = match (&dominante, binario) {
        (Some(c), false) => max_depth.max(c.nivel_efectivo),
        _ => max_depth,
    };
    let (big_o, big_o_reason) = match (profundidad, binario) {
        (0, _) => ("O(1)".to_string(), "sin loops".to_string()),
        (1, true) => ("O(log n)".to_string(), "loop que reduce el rango a la mitad".to_string()),
        (1, _) => ("O(n)".to_string(), "un loop".to_string()),
        (2, _) => ("O(n²)".to_string(), "2 loops anidados".to_string()),
        (3, _) => ("O(n³)".to_string(), "3 loops anidados".to_string()),
        (d, _) => (format!("O(n^{d})"), format!("{d} loops anidados")),
    };
    let big_o_reason = match &dominante {
        Some(c) if !binario && c.nivel_efectivo > max_depth => format!(
            "`{}` costs O(n) per call — {}. With {} loop{} around it, that multiplies out to {}",
            c.operacion,
            c.motivo,
            c.profundidad_bucle,
            if c.profundidad_bucle == 1 { "" } else { "s" },
            big_o,
        ),
        _ => big_o_reason,
    };

    // Rango REAL del nodo, no un conteo de llaves sobre el texto. El
    // `estimate_func_loc` que esto reemplaza contaba llaves, así que una
    // `"}"` adentro de una cadena le cortaba la función a la mitad: medido,
    // una función de 5 líneas con `const s = "}";` reportaba `loc` 2.
    let line = func.start_position().row + 1;
    let end_line = func.end_position().row + 1;

    MetricasJs {
        complexity: cc,
        big_o,
        big_o_reason,
        name: nombre_de(func, source),
        line,
        end_line,
        loc: end_line - line + 1,
        is_async: es_async(func),
        params: parametros(func, source),
        calls: llamadas(func, source),
        costos,
    }
}

/// Hallazgos para el panel — solo lo que esta ADENTRO de un bucle: una
/// operacion lineal suelta ya la refleja el Big-O de la funcion; lo que
/// merece un hallazgo aparte es la que multiplica.
pub fn linear_op_smells(mapa: &MapaDeFunciones) -> Vec<StructuralSmell> {
    let mut out = Vec::new();
    for f in mapa.funciones() {
        for c in f.costos.iter().filter(|c| c.profundidad_bucle > 0) {
            out.push(StructuralSmell {
                kind: "linear_op_in_loop",
                name: f.name.clone(),
                line: c.linea,
                message: format!(
                    "`{}` costs O(n) per call — {}. Inside {} loop{}, the real cost is O(n{}), not the O(n{}) that counting loops alone would give. `push`/`pop` are NOT the same: they are O(1) amortized, so n of them cost O(n) in total. For membership, a `Set` answers in O(1) where an array has to scan.",
                    c.operacion,
                    c.motivo,
                    c.profundidad_bucle,
                    if c.profundidad_bucle == 1 { "" } else { "s" },
                    superindice(c.nivel_efectivo),
                    superindice(c.profundidad_bucle),
                ),
            });
        }
    }
    out
}

fn superindice(n: u32) -> String {
    match n {
        1 => String::new(),
        2 => "²".into(),
        3 => "³".into(),
        otro => format!("^{otro}"),
    }
}

/// Parsea el archivo y mide todas sus funciones. `None` si la gramática no
/// puede con el archivo — el caller degrada, no rompe.
fn parsear(source: &str, es_typescript: bool) -> Option<tree_sitter::Tree> {
    let mut parser = Parser::new();
    let lenguaje = if es_typescript {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    } else {
        tree_sitter_javascript::LANGUAGE.into()
    };
    parser.set_language(&lenguaje).ok()?;
    parser.parse(source, None)
}

/// La superficie de prueba del archivo. Vacia para JS: `as`, `!` y `any` son
/// sintaxis de TypeScript.
pub fn proof_surface(source: &str, es_typescript: bool) -> ProofSurfaceTs {
    if !es_typescript {
        return ProofSurfaceTs::default();
    }
    match parsear(source, true) {
        Some(t) => superficie_de_prueba(t.root_node(), source),
        None => ProofSurfaceTs::default(),
    }
}

pub fn analizar(source: &str, es_typescript: bool) -> Option<MapaDeFunciones> {
    let arbol = parsear(source, es_typescript)?;

    let tipos = tipos_de_variables(arbol.root_node(), source);
    let mut entradas = Vec::new();
    fn recorrer(
        n: Node,
        source: &str,
        tipos: &HashMap<String, TipoJs>,
        out: &mut Vec<(usize, usize, MetricasJs)>,
    ) {
        if FUNCIONES.contains(&n.kind()) {
            out.push((n.start_position().row, n.end_position().row, medir(n, source, tipos)));
        }
        let mut cursor = n.walk();
        for hijo in n.children(&mut cursor) {
            recorrer(hijo, source, tipos, out);
        }
    }
    recorrer(arbol.root_node(), source, &tipos, &mut entradas);
    Some(MapaDeFunciones { entradas })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn una(src: &str) -> MetricasJs {
        let mapa = analizar(src, false).expect("parsea");
        mapa.entradas.first().expect("hay una función").2.clone()
    }

    /// `x!` es el gemelo exacto de `.unwrap()` en Rust: descarta la prueba
    /// de que el valor podia ser `null`.
    #[test]
    fn la_asercion_de_no_nulo_es_el_gemelo_de_unwrap() {
        let p = proof_surface("function f(o: A | null) {
  return o!;
}
", true);
        assert_eq!(p.non_null_assertions, 1);
    }

    /// **`as unknown as T` no es un `as` mas fuerte: es el que APAGA el
    /// unico chequeo que TypeScript todavia hace sobre una asercion.** Un
    /// `as` normal exige que los dos tipos se solapen; pasar por `unknown`
    /// saca ese requisito. Por eso se cuenta aparte.
    #[test]
    fn la_escotilla_doble_se_cuenta_aparte() {
        let p = proof_surface("function f(n: unknown) {
  return n as unknown as Date;
}
", true);
        assert_eq!(p.double_assertions, 1);
        assert_eq!(p.type_assertions, 2, "son dos `as` escritos, no uno");
    }

    #[test]
    fn un_as_normal_no_es_una_escotilla_doble() {
        let p = proof_surface("function f(n: unknown) {
  return n as string;
}
", true);
        assert_eq!(p.type_assertions, 1);
        assert_eq!(p.double_assertions, 0);
    }

    /// **La simetria justificado/sin justificar, mas nitida que en Rust.**
    /// `@ts-expect-error` falla si el error desaparece, asi que obliga a
    /// sacarlo cuando deja de hacer falta. `@ts-ignore` calla para siempre.
    #[test]
    fn ts_ignore_y_ts_expect_error_se_cuentan_por_separado() {
        let src = "// @ts-ignore
const a = mal();
// @ts-expect-error el tipo del vendor esta mal
const b = otro();
";
        let p = proof_surface(src, true);
        assert_eq!(p.ts_ignore, 1);
        assert_eq!(p.ts_expect_error, 1);
    }

    #[test]
    fn el_any_se_cuenta() {
        let p = proof_surface("function f() {
  let d: any = 1;
  return d;
}
", true);
        assert_eq!(p.any_annotations, 1);
    }

    /// Un archivo JS no tiene ninguna de estas formas: son sintaxis de
    /// TypeScript. Se devuelve vacio en vez de intentar parsearlo como TS.
    #[test]
    fn un_archivo_js_no_tiene_superficie_de_prueba() {
        let p = proof_surface("function f(o) {
  return o;
}
", false);
        assert_eq!(p.type_assertions, 0);
        assert_eq!(p.non_null_assertions, 0);
    }

    #[test]
    fn los_dos_hallazgos_salen_con_su_alternativa() {
        let src = "// @ts-ignore
const a = mal();
const b = n as unknown as Date;
";
        let smells = proof_smells(src, true);
        let kinds: Vec<&str> = smells.iter().map(|s| s.kind).collect();
        assert!(kinds.contains(&"ts_ignore_instead_of_expect_error"));
        assert!(kinds.contains(&"double_type_assertion"));
        assert!(smells[0].message.contains("@ts-expect-error"), "el mensaje da la alternativa");
    }

    /// Y el que NO se marca: `@ts-expect-error` ya es la forma justificada.
    #[test]
    fn ts_expect_error_no_se_marca() {
        let smells = proof_smells("// @ts-expect-error el vendor miente
const a = mal();
", true);
        assert!(smells.is_empty());
    }

    #[test]
    fn dos_bucles_secuenciales_son_lineales_no_cuadraticos() {
        let m = una("function f(n) {\n  for (let i = 0; i < n; i++) { a(i); }\n  for (let j = 0; j < n; j++) { b(j); }\n}\n");
        assert_eq!(m.big_o, "O(n)", "dos pasadas seguidas siguen siendo O(n)");
    }

    #[test]
    fn dos_bucles_anidados_si_son_cuadraticos() {
        let m = una("function f(n) {\n  for (let i = 0; i < n; i++) {\n    for (let j = 0; j < n; j++) { a(i, j); }\n  }\n}\n");
        assert_eq!(m.big_o, "O(n²)");
    }

    /// `Math.floor` es la forma normal de truncar cualquier cosa en JS: no
    /// dice nada sobre reducir un rango a la mitad.
    #[test]
    fn math_floor_no_convierte_un_bucle_en_logaritmico() {
        let m = una("function f(xs) {\n  for (let i = 0; i < xs.length; i++) { out.push(Math.floor(xs[i] / 3)); }\n}\n");
        assert_eq!(m.big_o, "O(n)");
    }

    /// Y la contraparte: una búsqueda binaria de verdad sigue siendo
    /// logarítmica. Sin esto, matar el falso positivo podría matar el caso
    /// que la heurística existía para reconocer.
    #[test]
    fn una_busqueda_binaria_de_verdad_sigue_siendo_logaritmica() {
        let m = una("function f(a, k) {\n  let lo = 0, hi = a.length;\n  while (lo < hi) {\n    const m = (lo + hi) >> 1;\n    if (a[m] < k) lo = m + 1; else hi = m;\n  }\n  return lo;\n}\n");
        assert_eq!(m.big_o, "O(log n)");
    }

    #[test]
    fn un_comentario_que_nombra_keywords_no_inventa_bucles() {
        let m = una("function f() {\n  // if we need to loop for each item while draining\n  hacer();\n}\n");
        assert_eq!(m.big_o, "O(1)");
        assert_eq!(m.complexity, 1);
    }

    #[test]
    fn una_cadena_que_nombra_keywords_tampoco() {
        let m = una("function f() {\n  console.log(\"if for while case\");\n}\n");
        assert_eq!(m.complexity, 1);
    }

    #[test]
    fn un_if_sin_espacio_igual_cuenta() {
        let m = una("function f(x) {\n  if(x) { a(); }\n}\n");
        assert_eq!(m.complexity, 2);
    }

    #[test]
    fn el_encadenamiento_opcional_no_es_una_decision_pero_el_nullish_si() {
        // `?.` accede a algo que puede faltar; `??` elige entre dos valores.
        assert_eq!(una("function f(o) {\n  return o?.a;\n}\n").complexity, 1);
        assert_eq!(una("function f(o) {\n  return o ?? 1;\n}\n").complexity, 2);
    }

    #[test]
    fn el_default_de_un_switch_no_cuenta_como_case() {
        let m = una("function f(x) {\n  switch (x) {\n    case 1: return 1;\n    case 2: return 2;\n    default: return 0;\n  }\n}\n");
        assert_eq!(m.complexity, 3, "2 case + default = CC 3");
    }

    /// Una función anidada tiene su propio `n` y su propia complejidad: lo
    /// que pasa adentro no se le suma a la de afuera.
    #[test]
    fn una_funcion_anidada_no_le_suma_a_la_de_afuera() {
        let src = "function f(xs) {\n  xs.forEach(function inner(x) {\n    for (const y of x) { if (y) { a(y); } }\n  });\n}\n";
        let mapa = analizar(src, false).unwrap();
        let externa = mapa.entradas.iter().min_by_key(|(i, fin, _)| fin - i).unwrap();
        assert_eq!(externa.2.big_o, "O(n)", "el bucle vive en `inner`, no en `f`");
        let f = mapa.entradas.iter().max_by_key(|(i, fin, _)| fin - i).unwrap();
        assert_eq!(f.2.big_o, "O(1)", "`f` no tiene ningún bucle propio");
        assert_eq!(f.2.complexity, 1);
    }

    #[test]
    fn los_metodos_de_clase_aparecen() {
        let mapa = analizar("class Repo {\n  cargar(id) {\n    return id;\n  }\n  guardar(x) {\n    return x;\n  }\n}\n", false).unwrap();
        let nombres: Vec<&str> = mapa.funciones().map(|f| f.name.as_str()).collect();
        assert_eq!(nombres, vec!["cargar", "guardar"]);
    }

    #[test]
    fn los_metodos_de_un_objeto_literal_tambien() {
        let mapa = analizar("const api = {\n  accion(x) {\n    return x;\n  },\n  otra: function (y) {\n    return y;\n  },\n  flecha: (w) => w,\n};\n", false).unwrap();
        let nombres: Vec<&str> = mapa.funciones().map(|f| f.name.as_str()).collect();
        assert_eq!(nombres, vec!["accion", "otra", "flecha"], "las tres formas toman el nombre de quien las recibe");
    }

    /// Una función escrita adentro de un comentario no existe. La regex la
    /// extraía porque el texto la contiene.
    #[test]
    fn una_funcion_en_un_comentario_no_existe() {
        let mapa = analizar("// function fantasma() {}\nfunction real() {\n  return 1;\n}\n", false).unwrap();
        let nombres: Vec<&str> = mapa.funciones().map(|f| f.name.as_str()).collect();
        assert_eq!(nombres, vec!["real"]);
    }

    /// El extractor viejo deduplicaba por NOMBRE, así que de dos `handler`
    /// en scopes distintos se perdía uno — y encima se quedaba con el de
    /// afuera, que ni siquiera se llamaba así.
    #[test]
    fn dos_funciones_con_el_mismo_nombre_en_scopes_distintos_son_dos() {
        let src = "function a() {\n  function handler() { return 1; }\n  return handler;\n}\nfunction b() {\n  function handler() { return 2; }\n  return handler;\n}\n";
        let mapa = analizar(src, false).unwrap();
        let handlers = mapa.funciones().filter(|f| f.name == "handler").count();
        assert_eq!(handlers, 2);
        assert_eq!(mapa.funciones().count(), 4, "`a`, `b` y los dos `handler`");
    }

    /// `estimate_func_loc` contaba llaves sobre el texto: una `"}"` adentro
    /// de una cadena le cortaba la función. Medido, una de 5 líneas daba 2.
    #[test]
    fn una_llave_adentro_de_una_cadena_no_corta_la_funcion() {
        let src = "function f() {\n  const s = \"}\";\n  const t = \"{\";\n  return s + t;\n}\n";
        let f = una(src);
        assert_eq!(f.loc, 5);
        assert_eq!((f.line, f.end_line), (1, 5));
    }

    #[test]
    fn async_y_parametros_salen_del_arbol() {
        let f = una("async function traer(u, opts) {\n  return await fetch(u, opts);\n}\n");
        assert!(f.is_async);
        assert_eq!(f.params, vec!["u", "opts"]);
        assert_eq!(f.calls, vec!["fetch"]);
    }

    #[test]
    fn una_arrow_con_un_parametro_pelado_igual_lo_reporta() {
        // `x => x + 1` no produce un `formal_parameters`: el parámetro
        // cuelga directo del nodo.
        let f = una("const doble = x => x * 2;\n");
        assert_eq!(f.name, "doble");
        assert_eq!(f.params, vec!["x"]);
    }

    /// Las llamadas de una función anidada son de ella, no de la de afuera.
    #[test]
    fn las_llamadas_no_se_heredan_de_las_funciones_de_adentro() {
        let src = "function outer() {\n  const inner = () => { peligroso(); };\n  seguro();\n  return inner;\n}\n";
        let mapa = analizar(src, false).unwrap();
        let outer = mapa.funciones().find(|f| f.name == "outer").unwrap();
        assert_eq!(outer.calls, vec!["seguro"]);
        let inner = mapa.funciones().find(|f| f.name == "inner").unwrap();
        assert_eq!(inner.calls, vec!["peligroso"]);
    }

    #[test]
    fn push_en_un_bucle_sigue_siendo_lineal() {
        // O(1) amortizado: marcarlo convertiria el acumulador CORRECTO en un
        // falso positivo, igual que del lado Python.
        let f = una("function f(xs) {\n  const out = [];\n  for (const x of xs) { out.push(x); }\n  return out;\n}\n");
        assert_eq!(f.big_o, "O(n)");
        assert!(f.costos.is_empty());
    }

    #[test]
    fn unshift_en_un_bucle_es_cuadratico() {
        let f = una("function f(xs) {\n  const out = [];\n  for (const x of xs) { out.unshift(x); }\n  return out;\n}\n");
        assert_eq!(f.big_o, "O(n\u{b2})");
        assert!(f.big_o_reason.contains("out.unshift"), "la razon cita la operacion: {}", f.big_o_reason);
    }

    #[test]
    fn splice_tambien() {
        let f = una("function f(xs) {\n  const out = [];\n  for (const x of xs) { out.splice(0, 0, x); }\n}\n");
        assert_eq!(f.big_o, "O(n\u{b2})");
    }

    /// El gemelo en JS de `quadratic_list_membership`: buscar linealmente
    /// adentro de un bucle.
    #[test]
    fn includes_sobre_un_arreglo_adentro_de_un_bucle_es_cuadratico() {
        let f = una("function f(xs) {\n  const vistos = [];\n  for (const x of xs) { if (!vistos.includes(x)) { vistos.push(x); } }\n  return vistos;\n}\n");
        assert_eq!(f.big_o, "O(n\u{b2})");
    }

    /// Y el mismo codigo con un `Set` NO se marca: `has` es O(1) promedio.
    /// Es la recomendacion concreta del hallazgo de arriba, asi que el motor
    /// tiene que saber distinguirlos o la recomendacion no vale nada.
    #[test]
    fn el_mismo_patron_con_un_set_no_se_marca() {
        let f = una("function f(xs) {\n  const vistos = new Set();\n  for (const x of xs) { if (!vistos.has(x)) { vistos.add(x); } }\n  return vistos;\n}\n");
        assert_eq!(f.big_o, "O(n)");
        assert!(f.costos.is_empty());
    }

    #[test]
    fn el_spread_que_se_copia_a_si_mismo_es_cuadratico() {
        let f = una("function f(xs) {\n  let out = [];\n  for (const x of xs) { out = [...out, x]; }\n  return out;\n}\n");
        assert_eq!(f.big_o, "O(n\u{b2})");
    }

    #[test]
    fn concat_sobre_si_mismo_tambien() {
        let f = una("function f(xs) {\n  let out = [];\n  for (const x of xs) { out = out.concat(x); }\n  return out;\n}\n");
        assert_eq!(f.big_o, "O(n\u{b2})");
    }

    /// **La diferencia con Python, y es real.** `s = s + t` en un bucle es
    /// O(n^2) en CPython, pero V8/SpiderMonkey/JSC usan cons-strings: armar
    /// la concatenacion es O(1) y el aplanado es perezoso. La misma forma de
    /// codigo tiene otra complejidad porque cambia el RUNTIME, no la
    /// sintaxis. Copiar la regla de Python seria inventar un hallazgo.
    #[test]
    fn concatenar_cadenas_no_se_marca_en_js_aunque_si_en_python() {
        for src in [
            "function f(partes) {\n  let s = '';\n  for (const t of partes) { s += t; }\n  return s;\n}\n",
            "function f(partes) {\n  let s = '';\n  for (const t of partes) { s = s + t; }\n  return s;\n}\n",
        ] {
            let f = una(src);
            assert_eq!(f.big_o, "O(n)", "cons-strings: no es cuadratico en JS");
            assert!(f.costos.is_empty());
        }
    }

    /// Un receptor de tipo desconocido no dispara nada: sin saber que es,
    /// no hay nada que afirmar.
    #[test]
    fn un_receptor_de_tipo_desconocido_no_afirma_nada() {
        let f = una("function f(xs, cosa) {\n  for (const x of xs) { cosa.splice(0, 0, x); }\n}\n");
        assert_eq!(f.big_o, "O(n)");
        assert!(f.costos.is_empty());
    }

    /// Una operacion lineal SUELTA no genera hallazgo: el Big-O de la
    /// funcion ya la refleja. Lo que merece hallazgo es la que multiplica.
    #[test]
    fn el_hallazgo_solo_sale_para_lo_que_esta_dentro_de_un_bucle() {
        let mapa = analizar("function f(xs) {\n  const out = [];\n  out.unshift(1);\n}\n", false).unwrap();
        assert!(linear_op_smells(&mapa).is_empty());

        let mapa = analizar("function f(xs) {\n  const out = [];\n  for (const x of xs) { out.unshift(x); }\n}\n", false).unwrap();
        let s = linear_op_smells(&mapa);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].kind, "linear_op_in_loop");
        assert_eq!(s[0].name, "f");
        assert!(s[0].message.contains("O(n\u{b2})"));
        assert!(s[0].message.contains("amortized"), "y por que `push` no cuenta");
    }

    #[test]
    fn typescript_tambien_parsea() {
        let src = "function f(x: number): number {\n  if (x > 0) { return 1; }\n  return 0;\n}\n";
        let mapa = analizar(src, true).expect("la gramática de TS");
        assert_eq!(mapa.funciones().next().unwrap().complexity, 2);
    }

    #[test]
    fn un_metodo_de_clase_tambien_se_mide() {
        let src = "class K {\n  m(x) {\n    if (x) { return 1; }\n    return 0;\n  }\n}\n";
        let mapa = analizar(src, false).unwrap();
        let m = mapa.funciones().next().unwrap();
        assert_eq!(m.name, "m");
        assert_eq!(m.complexity, 2);
    }
}
