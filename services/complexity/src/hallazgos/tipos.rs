//! **Teoría de tipos en lenguajes de bajo nivel: las conversiones que el
//! compilador hace en silencio.**
//!
//! El encuadre lo fijó el ROADMAP y es el correcto: `tsc`, `mypy` y
//! `pyright` cubren TypeScript y Python, y competir ahí no tiene sentido.
//! **Nadie cubre C/C++/Fortran en este encuadre**, y lo que se busca es
//! mecánico: conversiones implícitas con pérdida de precisión, mezcla de
//! signo, y enteros que se vuelven punteros.
//!
//! **Lo que hace falta antes que nada es rastrear declaraciones**, porque
//! ninguno de estos hallazgos se puede ver mirando una expresión suelta: que
//! `i < n` mezcle signo depende de cómo se declararon `i` y `n`, no de la
//! comparación. Este módulo construye ese mapa por función y lo consulta.
//! Es la misma pieza que el ROADMAP declara faltante para el análisis de
//! localidad espacial en Fortran (*"en tree-sitter-fortran `A(I,J)` es
//! indistinguible de una llamada a función"*): acá queda construida para
//! Fortran también.
//!
//! **El alcance está acotado a propósito y es intraprocedural.** No se sigue
//! el tipo a través de llamadas ni se resuelven `typedef`s: sin tabla de
//! tipos del proyecto, afirmar el tipo de retorno de una función ajena sería
//! adivinar. Lo que se ve es lo declarado en la misma función más los
//! literales, y con eso alcanza para los cuatro hallazgos de C y los tres de
//! Fortran — mismo criterio de "no afirmar más de lo que se puede probar"
//! que `rsparse.rs` ya declara sobre los alias de tipo.

use std::collections::HashMap;

use tree_sitter::Node;

use crate::hallazgos::smells::StructuralSmell;
use crate::ast::tsnode::{self, text_of};

/// La clase de tipo que alcanza para decidir estos hallazgos.
///
/// **No es un sistema de tipos, y el nombre lo dice.** No distingue `int` de
/// `long` ni `float` de `double` por sí sola — para eso está `ancho`. Lo que
/// separa son las categorías entre las que una conversión implícita
/// sorprende.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Clase {
    EnteroConSigno,
    EnteroSinSigno,
    /// `char` pelado. **No es ninguna de las dos anteriores**: el estándar
    /// deja su signo definido por la implementación, y en la práctica es con
    /// signo en x86 y sin signo en ARM. Tiene categoría propia porque esa
    /// ambigüedad ES el hallazgo.
    CharPelado,
    Flotante,
    Puntero,
    Otro,
}

#[derive(Clone, Copy, Debug)]
pub struct TipoDeclarado {
    pub clase: Clase,
    /// Bits, cuando el tipo los fija. `None` para los que dependen de la
    /// plataforma (`int`, `long`, `size_t`) — y no se inventa un valor:
    /// asumir 32 o 64 sería codificar un modelo de datos que el archivo no
    /// declara.
    pub ancho: Option<u16>,
}

/// Tipos de C/C++ que importan acá. Se consulta sobre el texto del tipo ya
/// normalizado (sin `const`, `volatile`, `static` ni espacios de más).
fn clasificar_c(texto: &str) -> TipoDeclarado {
    let t = texto
        .replace("const", " ")
        .replace("volatile", " ")
        .replace("static", " ")
        .replace("register", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let t = t.trim();

    if t.contains('*') {
        return TipoDeclarado { clase: Clase::Puntero, ancho: None };
    }
    // `char` pelado primero: `signed char` y `unsigned char` sí están
    // definidos y no son este caso.
    if t == "char" {
        return TipoDeclarado { clase: Clase::CharPelado, ancho: Some(8) };
    }
    if t.starts_with("unsigned") || matches!(t, "size_t" | "uintptr_t" | "uint8_t" | "uint16_t" | "uint32_t" | "uint64_t") {
        let ancho = match t {
            "uint8_t" => Some(8),
            "uint16_t" => Some(16),
            "uint32_t" => Some(32),
            "uint64_t" => Some(64),
            "unsigned char" => Some(8),
            "unsigned short" => Some(16),
            _ => None,
        };
        return TipoDeclarado { clase: Clase::EnteroSinSigno, ancho };
    }
    match t {
        "float" => TipoDeclarado { clase: Clase::Flotante, ancho: Some(32) },
        "double" | "long double" => TipoDeclarado { clase: Clase::Flotante, ancho: Some(64) },
        "int" | "long" | "short" | "long long" | "signed" | "signed char" | "ptrdiff_t" | "ssize_t" | "intptr_t" => {
            let ancho = match t {
                "signed char" => Some(8),
                "short" => Some(16),
                _ => None,
            };
            TipoDeclarado { clase: Clase::EnteroConSigno, ancho }
        }
        "int8_t" => TipoDeclarado { clase: Clase::EnteroConSigno, ancho: Some(8) },
        "int16_t" => TipoDeclarado { clase: Clase::EnteroConSigno, ancho: Some(16) },
        "int32_t" => TipoDeclarado { clase: Clase::EnteroConSigno, ancho: Some(32) },
        "int64_t" => TipoDeclarado { clase: Clase::EnteroConSigno, ancho: Some(64) },
        _ => TipoDeclarado { clase: Clase::Otro, ancho: None },
    }
}

/// El nombre declarado dentro de un declarador, bajando por punteros y
/// arreglos hasta el identificador.
fn identificador_de(n: Node, source: &str) -> Option<String> {
    if matches!(n.kind(), "identifier" | "field_identifier" | "type_identifier") {
        return Some(text_of(n, source).to_string());
    }
    let mut c = n.walk();
    let hallado = n.children(&mut c).find_map(|h| identificador_de(h, source));
    hallado
}

/// `true` si el declarador envuelve el nombre en un puntero (`int *p`), en
/// cuyo caso el tipo declarado del NOMBRE es puntero aunque el campo `type`
/// diga `int`.
fn declarador_es_puntero(n: Node) -> bool {
    if n.kind() == "pointer_declarator" {
        return true;
    }
    let mut c = n.walk();
    let hay = n.children(&mut c).any(|h| h.kind() == "pointer_declarator");
    hay
}

/// El mapa nombre → tipo de una función de C/C++: parámetros más variables
/// locales.
///
/// **Es intraprocedural y no resuelve `typedef`s.** Un nombre que no esté
/// acá no se juzga: el silencio es la respuesta correcta cuando no se sabe.
pub fn tipos_de_la_funcion_c(funcion: Node, source: &str) -> HashMap<String, TipoDeclarado> {
    let mut mapa = HashMap::new();
    tsnode::walk_flat(funcion, &mut |n| {
        if !matches!(n.kind(), "declaration" | "parameter_declaration") {
            return;
        }
        let Some(tipo) = n.child_by_field_name("type") else { return };
        let texto_tipo = text_of(tipo, source);
        // **El declarador de un parámetro pelado es un `identifier`, no un
        // `*_declarator`.** Filtrar solo por el sufijo dejaba afuera todos
        // los parámetros simples: en `int f(unsigned n)`, `n` nunca entraba
        // al mapa, así que `i < n` no tenía con qué comparar y el hallazgo
        // de mezcla de signo no salía nunca. Es la misma forma de defecto
        // que este crate ya encontró cuatro veces: el mismo concepto con
        // dos representaciones distintas en la gramática.
        let mut c = n.walk();
        let declaradores: Vec<Node> =
            n.children(&mut c).filter(|h| h.kind().ends_with("declarator") || h.kind() == "identifier").collect();
        for d in declaradores {
            let Some(nombre) = identificador_de(d, source) else { continue };
            let mut t = clasificar_c(texto_tipo);
            if declarador_es_puntero(d) {
                t = TipoDeclarado { clase: Clase::Puntero, ancho: None };
            }
            mapa.insert(nombre, t);
        }
    });
    mapa
}

/// El tipo de una expresión, **solo cuando se puede saber sin inferencia**:
/// un identificador declarado en esta función, o un literal.
fn tipo_de_expresion(n: Node, source: &str, tipos: &HashMap<String, TipoDeclarado>) -> Option<TipoDeclarado> {
    match n.kind() {
        "identifier" => tipos.get(text_of(n, source)).copied(),
        // **Un literal decimal sin sufijo es `double` en C, no `float`.** Es
        // la causa exacta de que `float f = 0.1;` pierda precisión, y el
        // motivo por el que este caso vale la pena detectar.
        "number_literal" => {
            let t = text_of(n, source).to_lowercase();
            if t.contains('.') || (t.contains('e') && !t.starts_with("0x")) {
                if t.ends_with('f') {
                    Some(TipoDeclarado { clase: Clase::Flotante, ancho: Some(32) })
                } else {
                    Some(TipoDeclarado { clase: Clase::Flotante, ancho: Some(64) })
                }
            } else if t.ends_with('u') || t.contains("ul") || t.contains("u") {
                Some(TipoDeclarado { clase: Clase::EnteroSinSigno, ancho: None })
            } else {
                Some(TipoDeclarado { clase: Clase::EnteroConSigno, ancho: None })
            }
        }
        "parenthesized_expression" => n.named_child(0).and_then(|h| tipo_de_expresion(h, source, tipos)),
        _ => None,
    }
}

const COMPARADORES: &[&str] = &["<", ">", "<=", ">=", "==", "!="];

/// `true` si la expresión es un literal negativo o la macro `EOF`.
///
/// `EOF` entra por nombre y no por valor: es una macro del preprocesador que
/// el árbol ve como un identificador, y su valor (`-1` en toda
/// implementación real) no está en el archivo que se analiza.
fn es_negativo_o_eof(n: Node, source: &str) -> bool {
    if n.kind() == "identifier" {
        return text_of(n, source) == "EOF";
    }
    // **En tree-sitter-c el signo de `-1` va DENTRO del literal**, no en una
    // expresión unaria: `c == -1` da `right="-1"[number_literal]`. Lo
    // encontró un test que fallaba mientras el caso de `EOF`, escrito con la
    // misma lógica, ya pasaba. Se aceptan las dos formas porque en otros
    // contextos (`-x`, `- (a + b)`) sí aparece la unaria.
    if n.kind() == "number_literal" {
        return text_of(n, source).starts_with('-');
    }
    if n.kind() != "unary_expression" {
        return false;
    }
    let Some(op) = n.child_by_field_name("operator") else { return false };
    let Some(arg) = n.child_by_field_name("argument") else { return false };
    text_of(op, source) == "-" && arg.kind() == "number_literal"
}

/// Los cuatro hallazgos de tipos de C/C++.
pub fn tipos_c(root: Node, source: &str) -> Vec<StructuralSmell> {
    let mut out = Vec::new();

    tsnode::walk_flat(root, &mut |f| {
        if f.kind() != "function_definition" {
            return;
        }
        let nombre_fn = tsnode::function_definition_name(f, source);
        let tipos = tipos_de_la_funcion_c(f, source);

        tsnode::walk_flat(f, &mut |n| {
            match n.kind() {
                // ── 1) Comparación que mezcla signo ──────────────────────
                "binary_expression" => {
                    let Some(op) = n.child_by_field_name("operator") else { return };
                    if !COMPARADORES.contains(&text_of(op, source)) {
                        return;
                    }
                    let (Some(izq), Some(der)) = (n.child_by_field_name("left"), n.child_by_field_name("right")) else { return };
                    // Los dos tipos se piden como `Option`: el chequeo de
                    // `char` solo necesita UNO de los dos lados, y exigir los
                    // dos lo mataba — en `c == -1` el lado derecho es un
                    // `unary_expression` sin tipo conocido, y en `c != EOF`
                    // un identificador de macro que no está en el mapa.
                    let (ti_opt, td_opt) = (tipo_de_expresion(izq, source, &tipos), tipo_de_expresion(der, source, &tipos));
                    // **`char` pelado comparado contra algo negativo.**
                    // El estándar deja el signo de `char` definido por la
                    // implementación: con signo en x86, SIN signo en ARM. En
                    // una plataforma donde es sin signo, `c == -1` es
                    // SIEMPRE falso, así que `while ((c = getchar()) != EOF)`
                    // nunca termina. El compilador no avisa porque no hay
                    // nada ilegal.
                    let char_contra_negativo = ti_opt.is_some_and(|t| t.clase == Clase::CharPelado) && es_negativo_o_eof(der, source)
                        || td_opt.is_some_and(|t| t.clase == Clase::CharPelado) && es_negativo_o_eof(izq, source);
                    if char_contra_negativo {
                        out.push(StructuralSmell {
                            kind: "char_signedness_assumption",
                            name: nombre_fn.clone(),
                            line: n.start_position().row + 1,
                            message: format!(
                                "`{}` compara un `char` pelado contra un valor negativo. El estándar NO dice si `char` tiene signo: es `signed` en x86 y `unsigned` en ARM, y son tres tipos distintos (`char`, `signed char`, `unsigned char`), no dos. Donde `char` es sin signo esta comparación es SIEMPRE falsa, así que el clásico `while ((c = getchar()) != EOF)` no termina nunca — y el compilador no avisa, porque no hay nada ilegal. Por eso `getchar()` devuelve `int` y no `char`: para que EOF quepa fuera del rango de los caracteres. La variable tiene que ser `int`.",
                                text_of(n, source).replace(char::is_whitespace, " ")
                            ),
                        });
                        return;
                    }
                    let (Some(ti), Some(td)) = (ti_opt, td_opt) else { return };
                    let mezcla = (ti.clase == Clase::EnteroConSigno && td.clase == Clase::EnteroSinSigno)
                        || (ti.clase == Clase::EnteroSinSigno && td.clase == Clase::EnteroConSigno);
                    if mezcla {
                        out.push(StructuralSmell {
                            kind: "signed_unsigned_comparison",
                            name: nombre_fn.clone(),
                            line: n.start_position().row + 1,
                            message: format!(
                                "`{}` compara un entero con signo contra uno sin signo. Las conversiones aritméticas usuales convierten el CON SIGNO a SIN SIGNO, no al revés, así que un valor negativo no se vuelve 'menor': se vuelve enorme. Es la forma clásica de un bucle infinito o de un acceso fuera de rango, y el compilador la hace en silencio salvo que se le pida el aviso (`-Wsign-compare`, que `-Wall` incluye). La solución no es un cast que tape el aviso, sino declarar el índice del mismo tipo que el límite.",
                                text_of(n, source).replace('\n', " ")
                            ),
                        });
                    }
                }
                // ── 2) Pérdida de precisión en una inicialización ────────
                "init_declarator" => {
                    let (Some(d), Some(v)) = (n.child_by_field_name("declarator"), n.child_by_field_name("value")) else { return };
                    let Some(nombre) = identificador_de(d, source) else { return };
                    let Some(destino) = tipos.get(&nombre).copied() else { return };
                    let Some(origen) = tipo_de_expresion(v, source, &tipos) else { return };
                    let Some(mensaje) = mensaje_de_estrechamiento(destino, origen, &nombre, text_of(v, source)) else { return };
                    out.push(StructuralSmell { kind: "implicit_precision_loss", name: nombre_fn.clone(), line: n.start_position().row + 1, message: mensaje });
                }
                // ── 3) Entero <-> puntero ────────────────────────────────
                "cast_expression" => {
                    let (Some(tipo), Some(valor)) = (n.child_by_field_name("type"), n.child_by_field_name("value")) else { return };
                    let destino = clasificar_c(text_of(tipo, source));
                    let Some(origen) = tipo_de_expresion(valor, source, &tipos) else { return };
                    let texto_destino = text_of(tipo, source);
                    // `intptr_t`/`uintptr_t` existen exactamente para esto y
                    // no se marcan: son la forma correcta.
                    if texto_destino.contains("intptr_t") {
                        return;
                    }
                    let cruza = (destino.clase == Clase::Puntero && matches!(origen.clase, Clase::EnteroConSigno | Clase::EnteroSinSigno))
                        || (matches!(destino.clase, Clase::EnteroConSigno | Clase::EnteroSinSigno) && origen.clase == Clase::Puntero);
                    if cruza {
                        out.push(StructuralSmell {
                            kind: "int_pointer_cast",
                            name: nombre_fn.clone(),
                            line: n.start_position().row + 1,
                            message:
                                "Conversión entre un entero y un puntero. El ancho de las dos cosas no tiene por qué coincidir: en el modelo LP64 que usan Linux y macOS en 64 bits, `int` mide 32 bits y un puntero 64, así que el viaje de ida y vuelta pierde la mitad de la dirección sin ningún aviso. Los tipos que existen para esto son `intptr_t` y `uintptr_t`, que el estándar define como lo bastante anchos para guardar un puntero; usarlos convierte un error silencioso en uno imposible."
                                    .to_string(),
                        });
                    }
                }
                _ => {}
            }
        });
    });

    out.sort_by_key(|s| s.line);
    out
}

/// El mensaje de un estrechamiento, o `None` si la conversión no pierde nada.
///
/// **Los dos casos se explican distinto porque fallan distinto**: pasar de
/// flotante a entero trunca hacia cero —el error es de valor—, y pasar de
/// `double` a `float` conserva la magnitud pero baja los dígitos
/// significativos —el error es de precisión—.
fn mensaje_de_estrechamiento(destino: TipoDeclarado, origen: TipoDeclarado, nombre: &str, texto_origen: &str) -> Option<String> {
    let origen_corto = texto_origen.replace('\n', " ");
    if origen.clase == Clase::Flotante && matches!(destino.clase, Clase::EnteroConSigno | Clase::EnteroSinSigno) {
        return Some(format!(
            "`{nombre}` es entero y se inicializa con `{origen_corto}`, que es de punto flotante. La conversión TRUNCA hacia cero, no redondea: 2.9 da 2 y -2.9 da -2. Y si el valor no entra en el rango del entero el comportamiento es indefinido, no un valor cualquiera."
        ));
    }
    if origen.clase == Clase::Flotante && destino.clase == Clase::Flotante {
        let (Some(ad), Some(ao)) = (destino.ancho, origen.ancho) else { return None };
        if ad < ao {
            return Some(format!(
                "`{nombre}` es `float` (24 bits de mantisa, ~7 dígitos decimales) y se inicializa con `{origen_corto}`, que es `double` (~16 dígitos). Un literal decimal sin sufijo es `double` en C, así que `float f = 0.1;` no guarda 0.1: guarda el `double` más cercano a 0.1, redondeado otra vez a `float`. Escribirlo `0.1f` evita la doble conversión."
            ));
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────
// Fortran
// ─────────────────────────────────────────────────────────────────────────

/// Los contenedores que en Fortran abren un ámbito con sus propias
/// declaraciones.
const AMBITOS_FORTRAN: &[&str] = &["program", "module", "subroutine", "function"];

/// `true` si el nodo es el CONTENEDOR y no la palabra clave homónima.
///
/// **Cuarta vez que los tokens anónimos muerden en este crate** (después de
/// `unsafe` en Rust, `constexpr` en C++ y `.and.`/`.or.` en Fortran): la
/// palabra `program` de `program p` es un token cuyo `kind()` también es
/// `"program"`, y lo mismo `subroutine` en la cabecera y en el `end
/// subroutine`. Sin este filtro, un programa sin `implicit none` producía
/// TRES hallazgos idénticos en vez de uno. Los tokens son anónimos, así que
/// `is_named()` los separa exactamente.
fn es_contenedor_fortran(n: Node) -> bool {
    n.is_named() && AMBITOS_FORTRAN.contains(&n.kind())
}

/// `true` si el texto de un tipo de Fortran denota doble precisión **con
/// certeza**.
///
/// `real(dp)` NO entra acá: `dp` es un nombre, y qué precisión sea depende de
/// cómo se lo haya definido en el archivo. Eso lo resuelve
/// `kinds_de_doble_precision`, y si no se puede resolver no se afirma.
fn es_doble_precision_literal(texto: &str) -> bool {
    let t = texto.to_lowercase().replace(char::is_whitespace, "");
    t.starts_with("doubleprecision") || t == "real(8)" || t == "real(kind=8)" || t == "real(16)" || t == "real(kind=16)"
}

/// Los nombres de *kind* que el archivo define como doble precisión.
///
/// El idioma real de Fortran moderno no es `real(kind=8)` —que no es
/// portable— sino declarar un parámetro y usarlo: `integer, parameter :: dp
/// = kind(1.0d0)`. Sin resolver eso, el hallazgo se perdería justamente en
/// el código bien escrito.
fn kinds_de_doble_precision(root: Node, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    tsnode::walk_flat(root, &mut |n| {
        if n.kind() != "init_declarator" {
            return;
        }
        let (Some(izq), Some(der)) = (n.child_by_field_name("left"), n.child_by_field_name("right")) else { return };
        let valor = text_of(der, source).to_lowercase().replace(char::is_whitespace, "");
        // `kind(1.0d0)` y `selected_real_kind(15, 307)` son las dos formas
        // con que se pide doble precisión de manera portable.
        let es_doble = (valor.starts_with("kind(") && valor.contains('d')) || valor.starts_with("selected_real_kind(");
        if es_doble {
            out.push(text_of(izq, source).to_lowercase());
        }
    });
    out
}

/// Clase de un tipo de Fortran, resolviendo los *kinds* simbólicos que el
/// archivo haya definido.
fn clase_fortran(texto: &str, kinds_dobles: &[String]) -> (Clase, bool) {
    let t = texto.to_lowercase().replace(char::is_whitespace, "");
    if t.starts_with("integer") {
        return (Clase::EnteroConSigno, false);
    }
    if t.starts_with("real") || t.starts_with("doubleprecision") {
        let doble = es_doble_precision_literal(&t) || kinds_dobles.iter().any(|k| t == format!("real({k})") || t == format!("real(kind={k})"));
        return (Clase::Flotante, doble);
    }
    (Clase::Otro, false)
}

/// Todos los nombres declarados por una `variable_declaration`.
///
/// El campo `declarator` trae **solo el primero**: en `real(kind=8) :: b, c`
/// devuelve `b` y `c` se pierde. Se recorren los hijos posteriores al `::`.
fn nombres_declarados_fortran(n: Node, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut c = n.walk();
    let mut despues_del_separador = false;
    for hijo in n.children(&mut c) {
        if hijo.kind() == "::" {
            despues_del_separador = true;
            continue;
        }
        if !despues_del_separador {
            continue;
        }
        match hijo.kind() {
            "identifier" => out.push(text_of(hijo, source).to_lowercase()),
            // `x = valor` dentro de una declaración.
            "init_declarator" => {
                if let Some(izq) = hijo.child_by_field_name("left") {
                    out.push(text_of(izq, source).to_lowercase());
                }
            }
            // `a(10)` — arreglo declarado con su forma.
            "call_expression" | "sized_declarator" => {
                if let Some(nombre) = identificador_de(hijo, source) {
                    out.push(nombre.to_lowercase());
                }
            }
            _ => {}
        }
    }
    out
}

/// `true` si el literal fija su precisión: exponente `d`/`D`, o sufijo
/// `_kind`.
fn literal_fija_su_precision(texto: &str) -> bool {
    let t = texto.to_lowercase();
    t.contains('_') || (t.contains('d') && !t.starts_with("0x"))
}

/// `true` si el literal es de punto flotante escrito con punto o exponente.
fn es_literal_real(texto: &str) -> bool {
    let t = texto.to_lowercase();
    t.contains('.') || t.contains('e') || t.contains('d')
}

/// Los tres hallazgos de tipos de Fortran.
///
/// **Y el mapa de declaraciones que hace falta para los dos primeros es la
/// pieza que el ROADMAP declara faltante** para el análisis de localidad
/// espacial en Fortran: ahí el problema es que `A(I,J)` es indistinguible de
/// una llamada a función sin saber si `A` fue declarado como arreglo, que es
/// exactamente lo que este mapa responde.
pub fn tipos_fortran(root: Node, source: &str) -> Vec<StructuralSmell> {
    let mut out = Vec::new();
    let kinds_dobles = kinds_de_doble_precision(root, source);

    // ── 1) `implicit none` ──────────────────────────────────────────────
    tsnode::walk_flat(root, &mut |n| {
        if !es_contenedor_fortran(n) {
            return;
        }
        // Un procedimiento dentro de un módulo/programa que ya declaró
        // `implicit none` está cubierto: la declaración es heredada, y
        // repetir el hallazgo en cada subrutina sería ruido.
        let mut ancestro = n.parent();
        while let Some(a) = ancestro {
            if es_contenedor_fortran(a) && tiene_implicit_none(a, source) {
                return;
            }
            ancestro = a.parent();
        }
        if tiene_implicit_none(n, source) {
            return;
        }
        out.push(StructuralSmell {
            kind: "missing_implicit_none",
            name: nombre_de_ambito(n, source),
            line: n.start_position().row + 1,
            message:
                "Este ámbito no declara `implicit none`, así que rige la regla implícita de Fortran: toda variable no declarada que empiece con I, J, K, L, M o N es INTEGER, y cualquier otra es REAL. No es una cuestión de estilo — significa que un nombre mal escrito no es un error de compilación sino una VARIABLE NUEVA, inicializada con basura, y el compilador no dice nada. `implicit none` convierte esa clase entera de errores silenciosos en errores de compilación."
                    .to_string(),
        });
    });

    // ── 2) y 3) necesitan el mapa de tipos por ámbito ───────────────────
    tsnode::walk_flat(root, &mut |amb| {
        if !es_contenedor_fortran(amb) {
            return;
        }
        let mut tipos: HashMap<String, (Clase, bool)> = HashMap::new();
        tsnode::walk_flat(amb, &mut |d| {
            if d.kind() != "variable_declaration" {
                return;
            }
            let Some(tipo) = d.child_by_field_name("type") else { return };
            let clase = clase_fortran(text_of(tipo, source), &kinds_dobles);
            for nombre in nombres_declarados_fortran(d, source) {
                tipos.insert(nombre, clase);
            }
        });
        let nombre_amb = nombre_de_ambito(amb, source);

        tsnode::walk_flat(amb, &mut |a| {
            if a.kind() != "assignment_statement" {
                return;
            }
            let (Some(izq), Some(der)) = (a.child_by_field_name("left"), a.child_by_field_name("right")) else { return };
            let Some(&(clase, es_doble)) = tipos.get(&text_of(izq, source).to_lowercase()) else { return };

            // 2) Literal de precisión simple en una variable de doble.
            if clase == Clase::Flotante && es_doble && der.kind() == "number_literal" {
                let texto = text_of(der, source);
                if es_literal_real(texto) && !literal_fija_su_precision(texto) {
                    out.push(StructuralSmell {
                        kind: "single_precision_literal_in_double",
                        name: nombre_amb.clone(),
                        line: a.start_position().row + 1,
                        message: format!(
                            "`{}` es de doble precisión, pero `{texto}` es un literal REAL POR DEFECTO, o sea de precisión simple. La constante se redondea a ~7 dígitos ANTES de asignarse, y lo que queda guardado no es el valor escrito sino su versión de simple precisión ensanchada — se pierden ~8 dígitos sin ningún aviso, y el compilador no puede advertirlo porque la conversión es legal. La forma correcta es fijar la precisión en el literal: `{texto}d0`, o `{texto}_dp` si el archivo define ese kind.",
                            text_of(izq, source)
                        ),
                    });
                }
            }

            // 3) División entera asignada a un real.
            if clase == Clase::Flotante && der.kind() == "math_expression" {
                let Some(op) = der.child_by_field_name("operator") else { return };
                if text_of(op, source) != "/" {
                    return;
                }
                let (Some(a1), Some(a2)) = (der.child_by_field_name("left"), der.child_by_field_name("right")) else { return };
                if es_entero_fortran(a1, source, &tipos) && es_entero_fortran(a2, source, &tipos) {
                    out.push(StructuralSmell {
                        kind: "integer_division_to_real",
                        name: nombre_amb.clone(),
                        line: a.start_position().row + 1,
                        message: format!(
                            "`{}` es real, pero `{}` es una división entre ENTEROS: se evalúa con aritmética entera y se trunca ANTES de convertirse a real. `1 / 3` no da 0.333…, da 0, y recién después se convierte a 0.0. La conversión a real ocurre al asignar, no durante el cálculo. Se arregla haciendo real a alguno de los dos operandos: `1.0 / 3.0`, o `real(a) / b`.",
                            text_of(izq, source),
                            text_of(der, source).replace(char::is_whitespace, " ")
                        ),
                    });
                }
            }
        });
    });

    out.sort_by_key(|s| s.line);
    out
}

fn es_entero_fortran(n: Node, source: &str, tipos: &HashMap<String, (Clase, bool)>) -> bool {
    match n.kind() {
        "number_literal" => !es_literal_real(text_of(n, source)),
        "identifier" => tipos.get(&text_of(n, source).to_lowercase()).map(|(c, _)| *c == Clase::EnteroConSigno).unwrap_or(false),
        _ => false,
    }
}

fn tiene_implicit_none(ambito: Node, source: &str) -> bool {
    let mut hay = false;
    tsnode::walk_flat(ambito, &mut |n| {
        if n.kind() == "implicit_statement" && text_of(n, source).to_lowercase().contains("none") {
            hay = true;
        }
    });
    hay
}

/// El nombre de la unidad de programa.
///
/// **Hay que mirar las dos formas.** `subroutine_statement` expone el nombre
/// como CAMPO (`name="s"`), pero `program_statement` y `module_statement` lo
/// tienen como hijo de kind `name` sin campo asociado. Mirando solo el campo,
/// todo hallazgo de un `program` salía como `<program>` — cosmético, pero
/// inútil en un archivo con varias unidades.
fn nombre_de_ambito(n: Node, source: &str) -> String {
    let mut c = n.walk();
    let hijos: Vec<Node> = n.children(&mut c).collect();
    for h in hijos {
        if !h.kind().ends_with("_statement") {
            continue;
        }
        if let Some(nombre) = h.child_by_field_name("name") {
            return text_of(nombre, source).to_string();
        }
        let mut c2 = h.walk();
        let por_kind = h.children(&mut c2).find(|x| x.kind() == "name");
        if let Some(nombre) = por_kind {
            return text_of(nombre, source).to_string();
        }
    }
    format!("<{}>", n.kind())
}

/// Un arreglo declarado en Fortran.
///
/// **Esto es exactamente el bloqueo que el ROADMAP nombraba** para el
/// análisis de localidad espacial en Fortran: *"en tree-sitter-fortran
/// `A(I,J)` es indistinguible de una llamada a función"*. Y es cierto — los
/// dos parsean como `call_expression`. Lo único que los separa es si `A` fue
/// declarado con forma, que es lo que responde este mapa.
#[derive(Clone, Copy, Debug)]
pub struct ArregloFortran {
    /// Cantidad de dimensiones.
    pub rango: usize,
    /// `(:,:)` — la forma se difiere. Un arreglo así puede ser un argumento
    /// mudo de forma asumida, y entonces **puede recibir una rebanada NO
    /// contigua**, que es la única forma en que un arreglo de Fortran deja
    /// de ser contiguo.
    pub forma_diferida: bool,
    /// `true` si el nombre es un argumento mudo del procedimiento.
    pub es_argumento: bool,
}

/// El rango que declara un texto de forma: `(10,10)` son 2, `(:,:,:)` son 3.
fn rango_de_forma(texto: &str) -> usize {
    texto.matches(',').count() + 1
}

/// `true` si la forma se difiere: hay `:` y ningún dígito que la fije.
fn forma_es_diferida(texto: &str) -> bool {
    texto.contains(':') && !texto.chars().any(|c| c.is_ascii_digit())
}

/// Los nombres de los argumentos mudos del procedimiento.
fn argumentos_mudos(ambito: Node, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut c = ambito.walk();
    let hijos: Vec<Node> = ambito.children(&mut c).collect();
    for h in hijos {
        if !h.kind().ends_with("_statement") {
            continue;
        }
        tsnode::walk_flat(h, &mut |n| {
            if n.kind() == "parameters" {
                tsnode::walk_flat(n, &mut |x| {
                    if x.kind() == "identifier" {
                        out.push(text_of(x, source).to_lowercase());
                    }
                });
            }
        });
    }
    out
}

/// El mapa nombre → arreglo de un ámbito de Fortran.
///
/// Cubre las dos formas de declarar la dimensión, que la gramática
/// representa distinto: `real :: a(10,10)` la pone en un `sized_declarator`
/// junto al nombre, y `real, dimension(10,10) :: a` la pone en un
/// `type_qualifier` que vale para TODOS los nombres de esa declaración.
pub fn arreglos_declarados_fortran(ambito: Node, source: &str) -> HashMap<String, ArregloFortran> {
    let mut mapa = HashMap::new();
    let mudos = argumentos_mudos(ambito, source);

    tsnode::walk_flat(ambito, &mut |d| {
        if d.kind() != "variable_declaration" {
            return;
        }
        // `dimension(...)` como atributo: vale para toda la declaración.
        let mut forma_comun: Option<String> = None;
        let mut c = d.walk();
        let hijos: Vec<Node> = d.children(&mut c).collect();
        for h in &hijos {
            if h.kind() == "type_qualifier" && text_of(*h, source).to_lowercase().starts_with("dimension") {
                let mut c2 = h.walk();
                let lista = h.children(&mut c2).find(|x| x.kind() == "argument_list");
                if let Some(l) = lista {
                    forma_comun = Some(text_of(l, source).to_string());
                }
            }
        }

        for h in &hijos {
            let (nombre, forma) = match h.kind() {
                // `a(10,10)` — la forma va pegada al nombre.
                "sized_declarator" => {
                    let mut c2 = h.walk();
                    let hs: Vec<Node> = h.children(&mut c2).collect();
                    let n = hs.iter().find(|x| x.kind() == "identifier").map(|x| text_of(*x, source).to_lowercase());
                    let f = hs.iter().find(|x| x.kind() == "size").map(|x| text_of(*x, source).to_string());
                    match (n, f.or_else(|| forma_comun.clone())) {
                        (Some(n), Some(f)) => (n, f),
                        _ => continue,
                    }
                }
                // Nombre pelado: solo es arreglo si hay `dimension(...)`.
                "identifier" => {
                    let Some(f) = forma_comun.clone() else { continue };
                    (text_of(*h, source).to_lowercase(), f)
                }
                _ => continue,
            };
            let es_argumento = mudos.contains(&nombre);
            mapa.insert(nombre, ArregloFortran { rango: rango_de_forma(&forma), forma_diferida: forma_es_diferida(&forma), es_argumento });
        }
    });
    mapa
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn analizar_c(src: &str) -> Vec<StructuralSmell> {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_c::LANGUAGE.into()).unwrap();
        let t = p.parse(src, None).unwrap();
        tipos_c(t.root_node(), src)
    }

    fn kinds(src: &str) -> Vec<&'static str> {
        analizar_c(src).into_iter().map(|s| s.kind).collect()
    }

    /// **El caso canónico**: un índice con signo contra un límite sin signo.
    #[test]
    fn el_indice_con_signo_contra_un_limite_sin_signo_se_marca() {
        let src = "int f(unsigned n) {\n    for (int i = 0; i < n; i++) { }\n    return 0;\n}\n";
        assert_eq!(kinds(src), vec!["signed_unsigned_comparison"]);
    }

    /// **Y lo que hace que no sea ruido**: con los dos del mismo signo no se
    /// dice nada.
    #[test]
    fn dos_del_mismo_signo_no_se_marcan() {
        let src = "int f(unsigned n) {\n    for (unsigned i = 0; i < n; i++) { }\n    return 0;\n}\n";
        assert!(kinds(src).is_empty());
    }

    /// Un nombre que no está declarado en esta función no se juzga: el
    /// silencio es la respuesta correcta cuando no se sabe el tipo.
    #[test]
    fn un_nombre_de_tipo_desconocido_no_produce_hallazgo() {
        let src = "int f(unsigned n) {\n    if (global_desconocido < n) { }\n    return 0;\n}\n";
        assert!(kinds(src).is_empty());
    }

    /// El mensaje tiene que decir en qué DIRECCIÓN convierte el lenguaje,
    /// que es lo que hace peligroso el caso.
    #[test]
    fn el_mensaje_dice_que_el_con_signo_va_hacia_sin_signo() {
        let src = "int f(unsigned n) {\n    int i = 0;\n    if (i < n) { }\n    return 0;\n}\n";
        let h = analizar_c(src);
        let m = &h.iter().find(|s| s.kind == "signed_unsigned_comparison").unwrap().message;
        assert!(m.contains("CON SIGNO a SIN SIGNO"), "{m}");
        assert!(m.contains("no se vuelve 'menor'"), "{m}");
        assert!(m.contains("-Wsign-compare"), "da la bandera concreta: {m}");
    }

    /// **`0.1` es `double`, no `float`** — la causa exacta de la pérdida.
    #[test]
    fn un_literal_decimal_en_un_float_pierde_precision() {
        let src = "void f(void) {\n    float x = 0.1;\n}\n";
        let h = analizar_c(src);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].kind, "implicit_precision_loss");
        assert!(h[0].message.contains("es `double` en C"), "{}", h[0].message);
        assert!(h[0].message.contains("0.1f"), "da la solución: {}", h[0].message);
    }

    /// Con el sufijo `f` no hay conversión y no se dice nada.
    #[test]
    fn el_sufijo_f_evita_el_hallazgo() {
        assert!(kinds("void f(void) {\n    float x = 0.1f;\n}\n").is_empty());
    }

    /// Flotante a entero TRUNCA, y el mensaje lo dice distinto porque falla
    /// distinto.
    #[test]
    fn de_flotante_a_entero_el_mensaje_habla_de_truncar() {
        let src = "void f(void) {\n    int n = 2.9;\n}\n";
        let h = analizar_c(src);
        assert_eq!(h[0].kind, "implicit_precision_loss");
        assert!(h[0].message.contains("TRUNCA hacia cero"), "{}", h[0].message);
        assert!(h[0].message.contains("indefinido"), "y el caso fuera de rango: {}", h[0].message);
    }

    /// Ensanchar no pierde nada y no se marca.
    #[test]
    fn ensanchar_no_es_un_hallazgo() {
        assert!(kinds("void f(void) {\n    double d = 1.5f;\n}\n").is_empty());
    }

    #[test]
    fn el_cast_de_entero_a_puntero_se_marca() {
        let src = "void f(void) {\n    int direccion = 4096;\n    char *p = (char *)direccion;\n}\n";
        assert!(kinds(src).contains(&"int_pointer_cast"), "{:?}", kinds(src));
    }

    /// **`intptr_t` existe exactamente para esto y no se marca**: marcar la
    /// forma correcta sería pedir un cambio ya hecho.
    #[test]
    fn intptr_t_es_la_forma_correcta_y_no_se_marca() {
        let src = "void f(char *p) {\n    intptr_t v = (intptr_t)p;\n}\n";
        assert!(!kinds(src).contains(&"int_pointer_cast"), "{:?}", kinds(src));
    }

    #[test]
    fn el_mensaje_del_puntero_explica_lp64() {
        let src = "void f(void) {\n    int d = 1;\n    char *p = (char *)d;\n}\n";
        let h = analizar_c(src);
        let m = &h.iter().find(|s| s.kind == "int_pointer_cast").unwrap().message;
        assert!(m.contains("LP64"), "{m}");
        assert!(m.contains("intptr_t"), "{m}");
    }

    #[test]
    fn un_archivo_sin_nada_de_esto_no_produce_hallazgos() {
        assert!(kinds("int main(void) {\n    return 0;\n}\n").is_empty());
    }


    /// **El bug de libro de texto de C**: `char` pelado contra EOF.
    #[test]
    fn un_char_pelado_contra_un_negativo_se_marca() {
        let src = "void f(void) {\n    char c = 'a';\n    if (c == -1) { }\n}\n";
        assert!(kinds(src).contains(&"char_signedness_assumption"), "{:?}", kinds(src));
    }

    /// `EOF` entra por NOMBRE: es una macro, y su valor no está en el
    /// archivo que se analiza.
    #[test]
    fn eof_se_reconoce_por_nombre_porque_es_una_macro() {
        let src = "void f(void) {\n    char c = 'a';\n    while (c != EOF) { }\n}\n";
        assert!(kinds(src).contains(&"char_signedness_assumption"), "{:?}", kinds(src));
    }

    /// **`signed char` SÍ está definido** y no es este caso: son tres tipos
    /// distintos, no dos.
    #[test]
    fn signed_char_esta_definido_y_no_se_marca() {
        let src = "void f(void) {\n    signed char c = 'a';\n    if (c == -1) { }\n}\n";
        assert!(!kinds(src).contains(&"char_signedness_assumption"), "{:?}", kinds(src));
    }

    /// Y un `char` contra un valor no negativo es normal.
    #[test]
    fn un_char_contra_un_positivo_no_se_marca() {
        let src = "void f(void) {\n    char c = 'a';\n    if (c == 65) { }\n}\n";
        assert!(kinds(src).is_empty(), "{:?}", kinds(src));
    }

    #[test]
    fn el_mensaje_del_char_explica_por_que_getchar_devuelve_int() {
        let src = "void f(void) {\n    char c = 'a';\n    if (c == -1) { }\n}\n";
        let h = analizar_c(src);
        let m = &h.iter().find(|s| s.kind == "char_signedness_assumption").unwrap().message;
        assert!(m.contains("`signed` en x86 y `unsigned` en ARM"), "{m}");
        assert!(m.contains("getchar()` devuelve `int`"), "{m}");
    }

    // ── Fortran ─────────────────────────────────────────────────────────

    fn analizar_f(src: &str) -> Vec<StructuralSmell> {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_fortran::LANGUAGE.into()).unwrap();
        let t = p.parse(src, None).unwrap();
        tipos_fortran(t.root_node(), src)
    }

    fn kinds_f(src: &str) -> Vec<&'static str> {
        analizar_f(src).into_iter().map(|s| s.kind).collect()
    }

    /// **El error más famoso de Fortran numérico**: el literal es de
    /// precisión simple aunque la variable sea doble.
    #[test]
    fn un_literal_por_defecto_en_una_variable_doble_se_marca() {
        let src = "program p\n    implicit none\n    double precision :: x\n    x = 0.1\nend program p\n";
        assert_eq!(kinds_f(src), vec!["single_precision_literal_in_double"]);
    }

    /// Y con el exponente `d0` no hay pérdida y no se dice nada.
    #[test]
    fn el_exponente_d0_fija_la_precision_y_no_se_marca() {
        let src = "program p\n    implicit none\n    double precision :: x\n    x = 0.1d0\nend program p\n";
        assert!(kinds_f(src).is_empty());
    }

    /// El sufijo `_kind` también.
    #[test]
    fn el_sufijo_de_kind_tambien_fija_la_precision() {
        let src = "program p\n    implicit none\n    integer, parameter :: dp = kind(1.0d0)\n    real(dp) :: x\n    x = 0.1_dp\nend program p\n";
        assert!(kinds_f(src).is_empty());
    }

    /// **El idioma real de Fortran moderno no es `real(kind=8)`**, que no es
    /// portable, sino un parámetro. Sin resolverlo, el hallazgo se perdería
    /// justamente en el código bien escrito.
    #[test]
    fn un_kind_simbolico_definido_en_el_archivo_se_resuelve() {
        let src = "program p\n    implicit none\n    integer, parameter :: dp = kind(1.0d0)\n    real(dp) :: x\n    x = 0.1\nend program p\n";
        assert_eq!(kinds_f(src), vec!["single_precision_literal_in_double"]);
    }

    /// Y un kind simbólico que el archivo NO define no se adivina: `real(sp)`
    /// sin definición podría ser cualquier cosa.
    #[test]
    fn un_kind_simbolico_sin_definir_no_se_afirma() {
        let src = "program p\n    implicit none\n    real(sp) :: x\n    x = 0.1\nend program p\n";
        assert!(kinds_f(src).is_empty());
    }

    /// Una variable `real` a secas es de precisión simple: el literal por
    /// defecto es exactamente lo correcto ahí.
    #[test]
    fn un_real_simple_con_literal_por_defecto_no_es_un_hallazgo() {
        let src = "program p\n    implicit none\n    real :: y\n    y = 0.1\nend program p\n";
        assert!(kinds_f(src).is_empty());
    }

    /// **La división se trunca ANTES de convertirse**: `1/3` da 0, no 0.333.
    #[test]
    fn la_division_entera_asignada_a_un_real_se_marca() {
        let src = "program p\n    implicit none\n    real :: y\n    y = 1 / 3\nend program p\n";
        let h = analizar_f(src);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].kind, "integer_division_to_real");
        assert!(h[0].message.contains("ANTES de convertirse"), "{}", h[0].message);
        assert!(h[0].message.contains("no da 0.333") || h[0].message.contains("da 0"), "{}", h[0].message);
    }

    /// Con un operando real no hay truncamiento y no se dice nada.
    #[test]
    fn con_un_operando_real_no_hay_division_entera() {
        let src = "program p\n    implicit none\n    real :: y\n    y = 1.0 / 3\nend program p\n";
        assert!(kinds_f(src).is_empty());
    }

    /// Y una división entera asignada a un ENTERO es lo que se quiso
    /// escribir: no es un hallazgo.
    #[test]
    fn la_division_entera_en_una_variable_entera_es_correcta() {
        let src = "program p\n    implicit none\n    integer :: n\n    n = 7 / 2\nend program p\n";
        assert!(kinds_f(src).is_empty());
    }

    /// `implicit none` falta: un nombre mal escrito se vuelve una variable
    /// nueva en vez de un error.
    #[test]
    fn la_falta_de_implicit_none_se_marca_con_su_motivo() {
        let src = "program p\n    real :: y\n    y = 1.0\nend program p\n";
        let h = analizar_f(src);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].kind, "missing_implicit_none");
        assert!(h[0].message.contains("I, J, K, L, M o N"), "{}", h[0].message);
        assert!(h[0].message.contains("VARIABLE NUEVA"), "y la consecuencia real: {}", h[0].message);
    }

    /// **Y no se repite en cada procedimiento de un módulo que ya lo
    /// declaró**: la declaración se hereda, y repetirla sería ruido.
    #[test]
    fn implicit_none_del_modulo_cubre_a_sus_procedimientos() {
        let src = "module m\n    implicit none\ncontains\n    subroutine s()\n        real :: y\n        y = 1.0\n    end subroutine s\nend module m\n";
        assert!(kinds_f(src).is_empty(), "{:?}", kinds_f(src));
    }

    /// Y una subrutina suelta sin `implicit none` sí se marca.
    #[test]
    fn una_subrutina_suelta_sin_implicit_none_se_marca() {
        let src = "subroutine s(a)\n    real :: a\n    a = 1.0\nend subroutine s\n";
        assert_eq!(kinds_f(src), vec!["missing_implicit_none"]);
    }

    /// **El campo `declarator` trae solo el PRIMER nombre**: en
    /// `real(kind=8) :: b, c` el segundo se perdía.
    #[test]
    fn una_declaracion_con_varios_nombres_los_registra_a_todos() {
        let src = "program p\n    implicit none\n    double precision :: b, c\n    b = 0.1\n    c = 0.2\nend program p\n";
        assert_eq!(kinds_f(src), vec!["single_precision_literal_in_double"; 2]);
    }


    /// **Las dos formas del nombre en la gramática**: `subroutine_statement`
    /// lo expone como campo, `program_statement` como hijo de kind `name`.
    /// Mirando solo el campo, todo `program` salía como `<program>`.
    #[test]
    fn el_nombre_de_la_unidad_sale_en_las_dos_formas() {
        let h = analizar_f("program calculo
    real :: y
    y = 1.0
end program calculo
");
        assert_eq!(h[0].name, "calculo", "el nombre viene como hijo `name`, no como campo");
        let h2 = analizar_f("subroutine sumar(a)
    real :: a
    a = 1.0
end subroutine sumar
");
        assert_eq!(h2[0].name, "sumar", "acá sí viene como campo");
    }

    #[test]
    fn un_programa_limpio_no_produce_hallazgos() {
        let src = "program p\n    implicit none\n    integer, parameter :: dp = kind(1.0d0)\n    real(dp) :: x\n    x = 0.1_dp\nend program p\n";
        assert!(kinds_f(src).is_empty());
    }

    // ── Rastreo de arreglos (desbloquea la localidad espacial) ──────────

    fn arreglos_de(src: &str) -> HashMap<String, ArregloFortran> {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_fortran::LANGUAGE.into()).unwrap();
        let t = p.parse(src, None).unwrap();
        let mut amb = None;
        tsnode::walk_flat(t.root_node(), &mut |n| {
            if es_contenedor_fortran(n) && amb.is_none() {
                amb = Some(n);
            }
        });
        arreglos_declarados_fortran(amb.unwrap(), src)
    }

    /// **La forma pegada al nombre.**
    #[test]
    fn un_arreglo_con_forma_en_el_declarador_se_registra() {
        let m = arreglos_de("subroutine s()\n  real :: a(10,10)\nend subroutine s\n");
        assert_eq!(m["a"].rango, 2);
        assert!(!m["a"].forma_diferida);
    }

    /// **Y la otra forma, que la gramática representa distinto**: el atributo
    /// `dimension(...)` vale para TODOS los nombres de la declaración.
    #[test]
    fn el_atributo_dimension_vale_para_todos_los_nombres() {
        let m = arreglos_de("subroutine s()\n  real, dimension(20,30) :: b, d\nend subroutine s\n");
        assert_eq!(m["b"].rango, 2);
        assert_eq!(m["d"].rango, 2, "el segundo nombre también");
    }

    /// Un escalar NO entra al mapa: ese es todo el punto — `f(i,j)` sin
    /// declaración de arreglo es una llamada a función.
    #[test]
    fn un_escalar_no_entra_al_mapa_de_arreglos() {
        let m = arreglos_de("subroutine s()\n  real :: x\n  integer :: n\nend subroutine s\n");
        assert!(m.is_empty(), "{m:?}");
    }

    /// **La forma diferida importa**: un argumento mudo de forma asumida
    /// puede recibir una rebanada no contigua.
    #[test]
    fn la_forma_diferida_y_el_argumento_mudo_se_distinguen() {
        let m = arreglos_de("subroutine s(a)\n  real :: a(:,:)\n  real :: local(5,5)\nend subroutine s\n");
        assert!(m["a"].forma_diferida);
        assert!(m["a"].es_argumento);
        assert!(!m["local"].forma_diferida);
        assert!(!m["local"].es_argumento);
    }

    #[test]
    fn el_rango_se_cuenta_por_dimensiones() {
        let m = arreglos_de("subroutine s()\n  real :: a(2,3,4)\n  real :: v(10)\nend subroutine s\n");
        assert_eq!(m["a"].rango, 3);
        assert_eq!(m["v"].rango, 1);
    }
}
