//! Fase 25 (Modernization Intelligence) — primer motor real, no más visión.
//! Entiende código C/C++ legacy y propone candidatos de modernización con
//! evidencia, nunca una conversión ciega — el mandato explícito de esta
//! fase: *"estudiar un sistema legacy y determinar qué partes son
//! candidatas a modernización"*, no un transpilador.
//!
//! Este primer corte trabaja 100% sobre datos que `memlayout.rs` (Fase 23)
//! ya calculó — cero AST nuevo, pura reinterpretación de `allocations` ya
//! detectadas: empareja cada sitio de adquisición (`malloc`/`calloc`/
//! `realloc`/`new`) con una liberación (`free`/`delete`) de la MISMA
//! variable en el mismo archivo. Deliberadamente sin scope real (misma
//! limitación que `memlayout.rs::resolve_target_variable` ya acepta): dos
//! funciones distintas con una variable local llamada igual podrían
//! empatarse por error — un heurístico honesto, no una garantía.
//!
//! Ocho patrones a esta altura (el 3ro al 8vo se sumaron en pasadas
//! posteriores a la primera porción — ver el ROADMAP, Fase 25):
//! - **`manual_memory_raii`**: adquisición Y liberación encontradas — el
//!   par existe y está bien administrado a mano, pero un smart pointer
//!   (C++ moderno) o el ownership de Rust lo harían imposible de romper.
//! - **`unmatched_allocation`**: adquisición sin liberación emparejada en
//!   el mismo archivo — señal de alta confianza (leak real, u ownership
//!   transferido a algo que este heurístico no seguiría).
//! - **`double_release`**: la MISMA variable liberada más de una vez en el
//!   archivo — un double-free real (comportamiento indefinido en C/C++),
//!   no solo un candidato de modernización sino un bug de correctitud ya
//!   presente. Máxima confianza posible: contar liberaciones repetidas no
//!   tiene el mismo margen de error que emparejar adquisición↔liberación
//!   por nombre (esa parte sí puede confundir variables de scopes
//!   distintos con el mismo nombre — ver el límite documentado arriba).
//! - **`unsafe_realloc_reassignment`**: el patrón `p = realloc(p, size)` —
//!   `memlayout.rs` ya detecta esto (`AllocationSite::reassigns_same_pointer`)
//!   comparando el primer argumento de `realloc` contra la variable a la
//!   que se reasigna. Si `realloc` falla devuelve `NULL`, y esa
//!   reasignación pisa la única referencia que quedaba a la memoria
//!   original — un leak real y uno de los bugs de C más clásicos y
//!   documentados que existen. Máxima confianza, igual que `double_release`.
//! - **`use_after_free`**: la MISMA variable usada (leída, desreferenciada,
//!   pasada como argumento) después de liberarse, sin una reasignación a
//!   memoria nueva detectada entre medio. A diferencia de los otros
//!   patrones, este SÍ recorre el AST directamente (no solo reinterpreta
//!   `allocations`) para buscar el primer uso posterior dentro de la
//!   ventana [línea de liberación, próxima adquisición de la misma
//!   variable). Heurístico textual/de orden de líneas, no un análisis de
//!   flujo de control real — un `free` dentro de un `if` seguido de un uso
//!   incondicional puede ser un falso positivo si las ramas son mutuamente
//!   excluyentes; por eso confianza `medium`, no `high` como los patrones
//!   anteriores.
//! - **`dangling_local_reference`** (2026-08-31, sexto patrón): `return
//!   &x;` donde `x` es una variable local (stack) — incluyendo parámetros —
//!   DE LA MISMA función que contiene el `return`. A diferencia de los 5
//!   patrones de arriba (que emparejan por nombre en TODO el archivo, la
//!   limitación ya documentada), este usa el `scope` por función que
//!   `memlayout.rs` ya calcula, así que no hereda esa ambigüedad — cuando
//!   dispara, es un bug real sin excepción posible en C/C++: el storage
//!   automático de `x` termina apenas la función retorna, así que el
//!   puntero devuelto queda colgando de inmediato, no "a veces". Por eso
//!   confianza `high`, igual que `double_release`/`unsafe_realloc_reassignment`
//!   (no `medium` como `use_after_free`, que sí tiene ambigüedad real de
//!   control de flujo). Deliberadamente acotado a `return &identificador`
//!   textual — `return &arr[i]`/`return &s.campo` (dirección de un elemento
//!   o de un miembro) no se detectan, mismo criterio de "no reclamar más de
//!   lo que el patrón puede probar" que el resto del motor.
//! - **`sizeof_pointer_allocation`** (2026-08-31, séptimo patrón; extendido a
//!   `calloc`/`realloc` en una pasada posterior, mismo día): `malloc(sizeof(x))`
//!   (o `calloc(n, sizeof(x))`/`realloc(x, sizeof(x))`) donde `x` está
//!   declarado como puntero (`T *x`), no la estructura a la que apunta — el
//!   clásico error de tamaño de asignación en C: `sizeof(x)` mide el tamaño
//!   del PUNTERO (4 u 8 bytes), no de lo que `x` va a apuntar, así que el
//!   buffer resultante queda demasiado chico y escribir más allá de esos
//!   pocos bytes corrompe memoria adyacente sin ningún error visible en el
//!   momento. Puramente mecánico, sin forward-scanning ni ambigüedad de
//!   scope: el bug está completo en la propia llamada de asignación, no
//!   depende de nada que pase antes o después — por eso confianza `high`.
//!   Determina "es un puntero" recorriendo declaraciones/parámetros de la
//!   función directamente (una distinción que
//!   `memlayout.rs::MemoryVariable::type_hint` no guarda — ese campo
//!   conserva solo el tipo base, `int` para `int *p`, sin la estrella), sin
//!   tocar el schema público de `memlayout.rs`. Las 3 funciones comparten el
//!   mismo idiom en su argumento de tamaño (`calloc`/`realloc` simplemente
//!   tienen argumentos adicionales antes del `sizeof` — el buscador ya mira
//!   TODOS los argumentos de la llamada, no solo el primero, así que
//!   extender de `malloc` a las 3 fue un cambio de una sola condición, no un
//!   patrón nuevo). Deliberadamente acotado a `sizeof(identificador)`
//!   textual — `sizeof(*x)`/`sizeof(x[0])`/`sizeof(TipoStruct)` son
//!   exactamente las formas CORRECTAS que este patrón no debe tocar, así que
//!   solo mira el caso `sizeof(identificador)` puro.
//! - **`unchecked_malloc_dereference`** (2026-09-01, octavo patrón):
//!   `p = malloc(...)` (o `calloc`/`realloc`) desreferenciado (`*p`),
//!   indexado (`p[i]`) o con un campo accedido (`p->campo`) SIN que un
//!   chequeo de null/truthiness (`if (p)`/`if (!p)`/`if (p == NULL)`/
//!   `if (p != NULL)`, o las mismas formas contra `0`) haya aparecido antes
//!   en la función. `malloc`/`calloc`/`realloc` solamente — `new` en C++
//!   lanza `std::bad_alloc` por default en vez de devolver null, así que no
//!   comparte este bug (salvo `nothrow`, fuera de alcance). A diferencia de
//!   `sizeof_pointer_allocation`, este SÍ necesita forward-scanning (como
//!   `use_after_free`, reusa directamente `use_after_free_window_end` para
//!   la ventana [línea de adquisición, próxima adquisición)): confianza
//!   `medium`, no `high`, mismo motivo que `use_after_free` — es un
//!   heurístico de orden de líneas, no de flujo de control real, así que un
//!   chequeo que existe en el texto pero no protege estructuralmente ese
//!   uso específico (ej. sin `return` en su cuerpo) se cuenta igual como
//!   "chequeado". Igual que `use_after_free` (y a diferencia de
//!   `dangling_local_reference`/`sizeof_pointer_allocation`), recorre el
//!   AST completo del ARCHIVO, no de una función aislada — hereda la misma
//!   limitación de scope que el resto del catálogo "original" (ver la nota
//!   de más abajo).
//! - **`c_style_cast`** (2026-09-01, noveno patrón; **solo C++**, primer
//!   patrón de este archivo con esa restricción de lenguaje): cualquier
//!   `(Tipo)expr` — tree-sitter ya lo distingue como su propio nodo
//!   `cast_expression` (a diferencia de una llamada tipo-función
//!   `Tipo(expr)`, que es un nodo distinto), así que no hace falta ninguna
//!   heurística propia para separar un cast real de una expresión entre
//!   paréntesis. Deliberadamente ausente en C puro: `static_cast`/
//!   `reinterpret_cast`/`const_cast` son sintaxis de C++ que no existe en C,
//!   así que marcar cada cast de un `.c` sería puro ruido sin ninguna
//!   sugerencia real detrás — `analyze_c_cpp` recibe un nuevo parámetro
//!   `is_cpp` (que `cparse.rs` ya tenía a mano en `parse_with`, nunca hizo
//!   falta calcularlo de nuevo) para este único propósito. No es un bug
//!   como `double_release`/`dangling_local_reference` — un C-style cast no
//!   es incorrecto por sí solo, es una sugerencia de estilo/intención más
//!   explícita — así que confianza `medium`, igual que `manual_memory_raii`.
//! - **`format_string_vulnerability`** (2026-09-02, décimo patrón; C y C++
//!   por igual, a diferencia de `c_style_cast`): una llamada a
//!   `printf`/`fprintf`/`sprintf`/`snprintf`/`syslog` cuyo argumento de
//!   formato (la posición varía por función — `printf` es el 1ro,
//!   `fprintf`/`sprintf`/`syslog` el 2do, `snprintf` el 3ro) NO es un
//!   literal (`string_literal`/`concatenated_string`) — el mismo criterio
//!   que `-Wformat-nonliteral` de gcc/clang ya usa. Si ese argumento puede
//!   contener datos externos, un atacante controla directivas de formato
//!   (`%x` filtra memoria del stack, `%n` la escribe) — CWE-134, uno de los
//!   bugs de C más clásicos y documentados que existen. Puramente mecánico,
//!   sin forward-scanning ni ambigüedad de scope, igual que
//!   `sizeof_pointer_allocation`: el patrón está completo en la propia
//!   llamada. Confianza `medium`, no `high` — a diferencia de
//!   `sizeof_pointer_allocation` (donde la forma en sí SIEMPRE es un bug,
//!   sin importar el contexto), acá el argumento podría legítimamente no
//!   contener nunca datos externos (ej. una cadena traducida/localizada
//!   fija en tiempo de ejecución) — el patrón señala el riesgo, no prueba
//!   que el dato sea realmente controlable por un atacante, mismo criterio
//!   de honestidad que `use_after_free`/`unchecked_malloc_dereference` ya
//!   usan para heurísticos sin datos de flujo reales. Sugiere
//!   `rust_format_macro` — las macros `format!`/`println!` de Rust exigen
//!   un string literal en tiempo de COMPILACIÓN, así que esta clase de bug
//!   no se puede escribir en absoluto, ni por accidente.
//! - **`unsafe_string_function`** (2026-09-02, undécimo patrón; C y C++ por
//!   igual): una llamada a `gets`/`strcpy`/`strcat` — no depende de qué
//!   argumentos reciba, es la FUNCIÓN en sí la que es inherentemente
//!   riesgosa, así que ni siquiera hace falta mirar los argumentos (a
//!   diferencia de todos los demás patrones de este archivo). `gets` no
//!   tiene ningún parámetro de tamaño — literalmente no hay forma de
//!   llamarla de manera segura, por eso fue eliminada del estándar C11 —
//!   confianza `high`. `strcpy`/`strcat` sí pueden usarse sin desbordar si
//!   el llamador ya garantizó los tamaños por otro lado (algo que este
//!   heurístico no puede ver sin tracking de tamaños real) — confianza
//!   `medium`, mismo criterio que `format_string_vulnerability`. Sugiere
//!   `rust_ownership` — `String`/`&str` de Rust siempre validan límites y
//!   crecen solos, esta clase de desbordamiento de buffer no existe.
//!
//! - **`unbounded_scanf`** (12º patrón, 2026-09-02) — `scanf`/`fscanf`/
//!   `sscanf` con una directiva `%s`/`%[...]` SIN ancho en su formato
//!   literal. Es el mismo desbordamiento que `gets` representa, pero acá el
//!   problema no es la función sino la forma de llamarla: `scanf("%19s",
//!   buf)` es perfectamente seguro, `scanf("%s", buf)` no puede serlo nunca.
//!   Confianza `high` (a diferencia de `strcpy`/`strcat`, que son `medium`):
//!   la evidencia está entera en la llamada — el formato es un literal ahí
//!   mismo y el ancho está ausente a la vista, sin nada fuera del archivo
//!   que pudiera desmentirlo. `%*s` (supresión de asignación, no escribe en
//!   ningún buffer) y `%%` no disparan. Sugiere `rust_ownership` — leer una
//!   línea a un `String` de Rust crece el buffer solo, no hay un largo
//!   máximo que adivinar.
//!
//! Deliberadamente NO intentado acá: análisis de ownership a través de
//! fronteras de función, tracking de scope real (misma limitación de
//! `memlayout.rs::resolve_target_variable`), o generar el código migrado en
//! sí — esta fase entiende y propone, no convierte.

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use tree_sitter::Node;

use crate::hallazgos::memlayout::MemoryLayoutResult;
use crate::ast::tsnode;

#[derive(Serialize, Clone)]
pub struct ModernizationCandidate {
    pub variable: String,
    pub pattern: &'static str,
    pub line: usize,
    pub current: String,
    pub suggested_target: &'static str,
    pub reasoning: String,
    pub confidence: &'static str,
}

#[derive(Serialize)]
pub struct ModernizationSummary {
    pub total: usize,
    pub manual_memory_raii: usize,
    pub unmatched_allocation: usize,
    pub double_release: usize,
    pub unsafe_realloc_reassignment: usize,
    pub use_after_free: usize,
    pub dangling_local_reference: usize,
    pub sizeof_pointer_allocation: usize,
    pub unchecked_malloc_dereference: usize,
    pub c_style_cast: usize,
    pub format_string_vulnerability: usize,
    pub unsafe_string_function: usize,
    pub unbounded_scanf: usize,
}

#[derive(Serialize)]
pub struct ModernizationReport {
    pub candidates: Vec<ModernizationCandidate>,
    pub summary: ModernizationSummary,
    pub note: String,
}

const NOTE: &str = "6 de los 12 candidatos (manual_memory_raii/unmatched_allocation/double_release/unsafe_realloc_reassignment/use_after_free/unchecked_malloc_dereference) emparejan por nombre de variable dentro del mismo archivo, sin rastrear scope real (misma limitación que memlayout.rs ya documenta) — dangling_local_reference y sizeof_pointer_allocation determinan la función que contiene cada hallazgo directamente sobre el AST, c_style_cast es un chequeo puramente sintáctico sin ningún concepto de variable/scope involucrado y solo aplica a archivos C++ (no a C, que no tiene static_cast/reinterpret_cast como alternativa), y format_string_vulnerability/unsafe_string_function/unbounded_scanf son igual de sintácticos pero aplican a C y C++ por igual. Esto propone, nunca convierte código automáticamente.";

fn is_acquire(kind: &str) -> bool {
    matches!(kind, "malloc" | "calloc" | "realloc" | "new")
}

fn is_release(kind: &str) -> bool {
    matches!(kind, "free" | "delete")
}

fn candidate_for(variable: &str, acquire_kind: &str, line: usize, has_release: bool) -> ModernizationCandidate {
    if has_release {
        ModernizationCandidate {
            variable: variable.to_string(),
            pattern: "manual_memory_raii",
            line,
            current: format!("`{acquire_kind}` con liberación manual emparejada para `{variable}`"),
            suggested_target: "raii_smart_pointer",
            reasoning: format!(
                "`{variable}` se adquiere y libera a mano — un smart pointer (`unique_ptr`/`shared_ptr` en C++ moderno) o el modelo de ownership de Rust eliminarían la posibilidad de un `free`/`delete` olvidado o duplicado."
            ),
            confidence: "medium",
        }
    } else {
        ModernizationCandidate {
            variable: variable.to_string(),
            pattern: "unmatched_allocation",
            line,
            current: format!("`{acquire_kind}` de `{variable}` sin liberación emparejada detectada en el mismo archivo"),
            suggested_target: "rust_ownership",
            reasoning: format!(
                "No se encontró un `free`/`delete` correspondiente para `{variable}` en este archivo — puede ser un leak real, o el ownership se transfiere a otra parte que este heurístico no rastrea. El modelo de ownership de Rust obligaría a hacer explícito qué pasa con esta memoria en tiempo de compilación."
            ),
            confidence: "high",
        }
    }
}

/// Cada liberación MÁS ALLÁ DE LA PRIMERA para una misma variable es un
/// double-free real — no un candidato "podría mejorarse", un bug de
/// correctitud ya presente. Apunta a la línea de la liberación repetida
/// (donde el double-free ocurre de verdad), no a la primera liberación ni a
/// la adquisición original.
fn double_release_candidates(memory: &MemoryLayoutResult) -> Vec<ModernizationCandidate> {
    let mut release_count: HashMap<&str, usize> = HashMap::new();
    let mut out = Vec::new();
    for a in &memory.allocations {
        if !is_release(a.kind) {
            continue;
        }
        let Some(variable) = a.variable.as_deref() else { continue };
        let count = release_count.entry(variable).or_insert(0);
        *count += 1;
        if *count > 1 {
            out.push(ModernizationCandidate {
                variable: variable.to_string(),
                pattern: "double_release",
                line: a.line,
                current: format!("`{}` de `{variable}` — ya se había liberado antes en este archivo", a.kind),
                suggested_target: "rust_ownership",
                reasoning: format!(
                    "`{variable}` aparece liberado más de una vez en este archivo — un double-free real (comportamiento indefinido en C/C++). El modelo de ownership de Rust lo vuelve imposible de escribir: el compilador rechaza mover o liberar un valor ya movido."
                ),
                confidence: "high",
            });
        }
    }
    out
}

/// `p = realloc(p, size)` — `memlayout.rs` ya marcó cada sitio así con
/// `reassigns_same_pointer`, esta función solo lo convierte al shape de
/// `ModernizationCandidate` con su propia evidencia/razonamiento.
fn unsafe_realloc_candidates(memory: &MemoryLayoutResult) -> Vec<ModernizationCandidate> {
    memory
        .allocations
        .iter()
        .filter(|a| a.reassigns_same_pointer)
        .filter_map(|a| {
            let variable = a.variable.as_deref()?;
            Some(ModernizationCandidate {
                variable: variable.to_string(),
                pattern: "unsafe_realloc_reassignment",
                line: a.line,
                current: format!("`{variable} = realloc({variable}, ...)` — reasignación directa al mismo puntero"),
                suggested_target: "rust_ownership",
                reasoning: format!(
                    "si `realloc` falla devuelve `NULL`, y asignarlo directo a `{variable}` pisa la única referencia que quedaba a la memoria original — un leak real, uno de los bugs de C más clásicos. La forma segura en C es reasignar a una variable temporal y chequear antes de pisar `{variable}`; el `Vec`/ownership de Rust maneja el realloc internamente, esta clase de bug no puede escribirse."
                ),
                confidence: "high",
            })
        })
        .collect()
}

/// La próxima adquisición (`malloc`/`calloc`/`realloc`/`new`) de la MISMA
/// variable después de `after_line`, si existe — reasignar a memoria nueva
/// vuelve el puntero válido de nuevo, así que la ventana de "uso
/// sospechoso" termina ahí. Sin una adquisición posterior, la ventana llega
/// hasta el final del archivo (`usize::MAX`).
/// Índice de una sola pasada sobre el árbol, para los 2 patrones que
/// necesitan mirar "qué pasa después de esta línea".
///
/// **Optimización medida (2026-09-05), no una suposición.** `use_after_free`
/// y `unchecked_malloc_dereference` hacían un `walk_flat(root)` COMPLETO por
/// cada allocation site. En un archivo con 80 `malloc` y 80 `free` eso son
/// 160 recorridos del árbol entero — O(allocations × nodos). Perfilando
/// `parse_c` sobre 481 líneas de C, `modernization` se llevaba **261 de los
/// 276 ms** del análisis completo; todas las demás etapas estaban en
/// milisegundos de un dígito.
///
/// La inversión es simple: en vez de preguntar "¿este nodo usa la variable
/// X?" una vez por variable, se pregunta "¿qué variable usa este nodo?" una
/// sola vez y se indexa por ella. Los predicados pasaron de
/// `(nodo, nombre) -> bool` a `(nodo) -> Option<nombre>`.
#[derive(Default)]
struct TreeIndex<'a> {
    /// Los nodos que cada patrón necesita, recolectados en la MISMA pasada.
    ///
    /// **Segunda medición (2026-09-05).** Después de arreglar el walk por
    /// allocation, `modernization` seguía siendo la mitad del costo de
    /// `parse_c`: hacía **8 recorridos completos** del mismo árbol, cuatro de
    /// ellos filtrando por el mismo `call_expression`. Escanear 10 archivos
    /// `.c` tardaba 75 ms contra 11 ms de la misma cantidad de Python o JS.
    /// Recolectar una vez y que cada patrón itere su lista deja UNA pasada.
    call_expressions: Vec<Node<'a>>,
    cast_expressions: Vec<Node<'a>>,
    function_definitions: Vec<Node<'a>>,
    /// Por identificador: `(línea, es el lado izquierdo de una asignación)`.
    /// El booleano importa: `p = NULL` es justamente el idiom SEGURO, no un
    /// uso, y confundirlos convertiría el arreglo en el hallazgo.
    identifier_uses: HashMap<&'a str, Vec<(usize, bool)>>,
    /// Por variable: líneas de un `if` que la chequea contra null.
    null_checks: HashMap<&'a str, Vec<usize>>,
    /// Por variable: líneas de un uso peligroso (deref, índice, campo).
    dangerous_uses: HashMap<&'a str, Vec<usize>>,
}

impl<'a> TreeIndex<'a> {
    /// Primera línea de `lineas` dentro de la ventana `(desde, hasta)`,
    /// exclusiva en los dos extremos — misma semántica que tenían los cortes
    /// `line <= a.line || line >= window_end` de los bucles originales.
    fn primera_en_ventana(lineas: Option<&Vec<usize>>, desde: usize, hasta: usize) -> Option<usize> {
        lineas?.iter().copied().filter(|l| *l > desde && *l < hasta).min()
    }
}

/// Recorre solo los nodos que `build_tree_index` ya recolectó, en vez del
/// árbol entero. Firma idéntica a `tsnode::walk_flat` a propósito: cambiar un
/// patrón de "recorré todo y filtrá" a "iterá tu lista" es entonces una sola
/// línea, y el cuerpo —con sus `return` que significan "salteá este nodo"—
/// queda intacto. Convertirlo a un `for` habría cambiado esos `return` en
/// "salí de la función", que es un bug silencioso esperando a pasar.
fn visitar<'a>(nodos: &[Node<'a>], f: &mut impl FnMut(Node<'a>)) {
    for n in nodos {
        f(*n);
    }
}

fn build_tree_index<'a>(root: Node<'a>, source: &'a str) -> TreeIndex<'a> {
    let mut idx = TreeIndex::default();
    tsnode::walk_flat(root, &mut |n| {
        match n.kind() {
            "call_expression" => idx.call_expressions.push(n),
            "cast_expression" => idx.cast_expressions.push(n),
            "function_definition" => idx.function_definitions.push(n),
            _ => {}
        }
        if n.kind() == "identifier" {
            let nombre = tsnode::text_of(n, source);
            let es_asignacion = n.parent().is_some_and(|p| {
                p.kind() == "assignment_expression" && p.child_by_field_name("left").map(|l| l.id()) == Some(n.id())
            });
            idx.identifier_uses.entry(nombre).or_default().push((n.start_position().row + 1, es_asignacion));
        }
        if n.kind() == "if_statement" {
            if let Some(cond_wrap) = n.child_by_field_name("condition") {
                let cond = if cond_wrap.kind() == "parenthesized_expression" {
                    cond_wrap.named_child(0).unwrap_or(cond_wrap)
                } else {
                    cond_wrap
                };
                if let Some(v) = null_checked_variable(cond, source) {
                    idx.null_checks.entry(v).or_default().push(n.start_position().row + 1);
                }
            }
        }
        if let Some(v) = dangerously_used_variable(n, source) {
            idx.dangerous_uses.entry(v).or_default().push(n.start_position().row + 1);
        }
    });
    idx
}

fn use_after_free_window_end(memory: &MemoryLayoutResult, variable: &str, after_line: usize) -> usize {
    memory
        .allocations
        .iter()
        .filter(|a| is_acquire(a.kind) && a.variable.as_deref() == Some(variable) && a.line > after_line)
        .map(|a| a.line)
        .min()
        .unwrap_or(usize::MAX)
}

/// Para cada liberación (`free`/`delete`) con variable resuelta, busca el
/// primer uso posterior de esa MISMA variable dentro de la ventana [línea
/// de liberación, próxima adquisición). Cuenta como "uso" cualquier
/// ocurrencia del identificador que NO sea el lado izquierdo de una
/// asignación (reasignar `p = NULL` es justo el idiom seguro que previene
/// esto, no un uso) y que no coincida con la línea de otro allocation site
/// ya conocido para la misma variable (evita duplicar evidencia con
/// `double_release`, que ya cubre una segunda liberación).
///
/// Único patrón de este módulo que recorre el AST directamente en vez de
/// solo reinterpretar `allocations` — necesario porque "uso" no es un
/// allocation site. Heurístico textual/de orden de líneas, no un análisis
/// de flujo de control real: un `free` dentro de un `if` seguido de un uso
/// incondicional puede ser un falso positivo si las ramas son mutuamente
/// excluyentes. Por eso reporta confianza `medium`, no `high`.
fn use_after_free_candidates(memory: &MemoryLayoutResult, index: &TreeIndex) -> Vec<ModernizationCandidate> {
    let mut out = Vec::new();
    for a in &memory.allocations {
        if !is_release(a.kind) {
            continue;
        }
        let Some(variable) = a.variable.as_deref() else { continue };
        let window_end = use_after_free_window_end(memory, variable, a.line);
        let known_allocation_lines: HashSet<usize> =
            memory.allocations.iter().filter(|o| o.variable.as_deref() == Some(variable)).map(|o| o.line).collect();

        // `min()` por línea en vez de "el primero que encuentre el recorrido":
        // el recorrido era pre-orden, o sea aproximadamente orden de fuente,
        // así que da el mismo resultado — y este es determinista por
        // construcción en vez de por casualidad.
        let first_use = index
            .identifier_uses
            .get(variable)
            .and_then(|usos| {
                usos.iter()
                    .filter(|(line, es_asignacion)| {
                        !es_asignacion
                            && *line > a.line
                            && *line < window_end
                            && !known_allocation_lines.contains(line)
                    })
                    .map(|(line, _)| *line)
                    .min()
            });

        if let Some(line) = first_use {
            out.push(ModernizationCandidate {
                variable: variable.to_string(),
                pattern: "use_after_free",
                line,
                current: format!("`{variable}` usado en la línea {line}, liberado antes en la línea {} (`{}`), sin reasignación detectada entre medio", a.line, a.kind),
                suggested_target: "rust_ownership",
                reasoning: format!(
                    "`{variable}` sigue en uso después de liberarse — comportamiento indefinido en C/C++ (la memoria puede haber sido reutilizada por otra asignación mientras tanto). El modelo de ownership de Rust lo vuelve un error de compilación: usar un valor después de moverlo o liberarlo no compila."
                ),
                confidence: "medium",
            });
        }
    }
    out
}

/// `return &x;` donde `x` es una variable de región `"stack"` con `scope`
/// IGUAL al nombre de la función que contiene ese `return` — a diferencia de
/// todo lo de arriba (que empareja por nombre en el archivo entero), acá el
/// `scope` por función ya elimina esa ambigüedad: si dispara, es un bug real
/// sin excepción en C/C++, nunca un falso positivo por colisión de nombres
/// entre dos funciones distintas. `x` local `static` no cuenta — `memlayout`
/// ya la clasifica como `"data"`/`"bss"` (storage estático, sigue viva
/// después del `return`), no `"stack"`.
///
/// Recorre el AST directamente (segundo patrón, después de
/// `use_after_free`, que necesita hacerlo: "la dirección de una variable
/// que se devuelve" no es un allocation site). `pointer_expression` es el
/// nodo real de tree-sitter-c/cpp para `&x`/`*x` (`unary_expression` en esta
/// gramática es solo `!`/`+`/`-`/`~`) — confirmado contra `node-types.json`
/// del crate antes de escribir esto, no adivinado.
fn dangling_local_reference_candidates(
    memory: &MemoryLayoutResult,
    index: &TreeIndex,
    source: &str,
) -> Vec<ModernizationCandidate> {
    let mut out = Vec::new();
    visitar(&index.function_definitions, &mut |func_node| {
        if func_node.kind() != "function_definition" {
            return;
        }
        let func_name = tsnode::function_definition_name(func_node, source);
        let locals: HashSet<&str> =
            memory.variables.iter().filter(|v| v.region == "stack" && v.scope == func_name).map(|v| v.name.as_str()).collect();
        if locals.is_empty() {
            return;
        }
        tsnode::walk_flat(func_node, &mut |ret_node| {
            if ret_node.kind() != "return_statement" {
                return;
            }
            let Some(expr) = ret_node.named_child(0) else { return };
            if expr.kind() != "pointer_expression" {
                return;
            }
            let Some(op) = expr.child_by_field_name("operator") else { return };
            if tsnode::text_of(op, source) != "&" {
                return; // `*x` (dereferencia) — no es esto
            }
            let Some(arg) = expr.child_by_field_name("argument") else { return };
            if arg.kind() != "identifier" {
                return; // `return &arr[i]`/`return &s.campo` — fuera de alcance, ver doc del módulo
            }
            let name = tsnode::text_of(arg, source);
            if !locals.contains(name) {
                return;
            }
            out.push(ModernizationCandidate {
                variable: name.to_string(),
                pattern: "dangling_local_reference",
                line: ret_node.start_position().row + 1,
                current: format!("`return &{name};` dentro de `{func_name}` — `{name}` es una variable local (stack)"),
                suggested_target: "rust_ownership",
                reasoning: format!(
                    "`{name}` vive en el stack de `{func_name}` — su memoria se libera apenas la función retorna, así que el puntero devuelto queda colgando (dangling) de inmediato; desreferenciarlo del lado del caller es comportamiento indefinido, no un riesgo ocasional. El borrow checker de Rust rechaza esto en tiempo de compilación: devolver una referencia a algo local sin una lifetime que la respalde no compila."
                ),
                confidence: "high",
            });
        });
    });
    out
}

/// Recorre un declarator (mismo shape que `memlayout::declarator_name`,
/// duplicado a propósito acá porque esa función es privada y no distingue
/// puntero de no-puntero) devolviendo el nombre final Y si en el camino
/// atravesó un `pointer_declarator` — la única forma de saber "esto es un
/// puntero" en esta gramática, ya que `type` (el campo que sí es público en
/// `MemoryVariable::type_hint`) conserva solo el tipo base (`int` para
/// `int *p`, sin la estrella).
fn declarator_name_and_is_pointer(node: Node, source: &str, seen_pointer: bool) -> Option<(String, bool)> {
    match node.kind() {
        "identifier" | "field_identifier" => Some((tsnode::text_of(node, source).to_string(), seen_pointer)),
        "function_declarator" => None,
        "pointer_declarator" => declarator_name_and_is_pointer(node.child_by_field_name("declarator")?, source, true),
        "array_declarator" | "parenthesized_declarator" | "reference_declarator" | "init_declarator" => {
            declarator_name_and_is_pointer(node.child_by_field_name("declarator")?, source, seen_pointer)
        }
        _ => None,
    }
}

/// Nombres de variables (locales o parámetros) declaradas como puntero
/// dentro de esta función — construido recorriendo declaraciones/parámetros
/// directamente, sin scope-dentro-de-scope real (misma simplificación que
/// el resto de este archivo acepta: un bloque anidado que re-declara el
/// mismo nombre no se distingue).
fn pointer_variable_names(func_node: Node, source: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    tsnode::walk_flat(func_node, &mut |n| match n.kind() {
        "declaration" => {
            let mut cursor = n.walk();
            for declarator in n.children_by_field_name("declarator", &mut cursor) {
                if let Some((name, true)) = declarator_name_and_is_pointer(declarator, source, false) {
                    out.insert(name);
                }
            }
        }
        "parameter_declaration" => {
            if let Some(declarator) = n.child_by_field_name("declarator") {
                if let Some((name, true)) = declarator_name_and_is_pointer(declarator, source, false) {
                    out.insert(name);
                }
            }
        }
        _ => {}
    });
    out
}

/// `malloc(sizeof(x))` / `calloc(n, sizeof(x))` / `realloc(x, sizeof(x))`
/// donde `x` es un puntero — el clásico error de tamaño de asignación en C.
/// Puramente mecánico: el bug está completo en la propia llamada, no
/// depende de nada que pase antes o después, así que no hace falta
/// forward-scanning como en `use_after_free`/`dangling_local_reference`. El
/// buscador de `sizeof_expression` ya mira TODOS los argumentos de la
/// llamada (no solo el primero), así que cubrir las 3 funciones en vez de
/// solo `malloc` es nada más relajar qué nombres de función se aceptan —
/// `calloc`/`realloc` traen argumentos adicionales antes del `sizeof`
/// (la cuenta, o el puntero a reasignar) que este buscador ya ignora sin
/// cambios. Ver el doc del módulo para por qué solo mira
/// `sizeof(identificador)` textual.
fn sizeof_pointer_allocation_candidates(index: &TreeIndex, source: &str) -> Vec<ModernizationCandidate> {
    let mut out = Vec::new();
    // Este patron itera FUNCIONES, no llamadas: primero necesita saber que
    // variables de la funcion son punteros, y recien despues busca los
    // `malloc(sizeof(...))` de adentro.
    visitar(&index.function_definitions, &mut |func_node| {
        if func_node.kind() != "function_definition" {
            return;
        }
        let pointer_names = pointer_variable_names(func_node, source);
        if pointer_names.is_empty() {
            return;
        }
        tsnode::walk_flat(func_node, &mut |call_node| {
            if call_node.kind() != "call_expression" {
                return;
            }
            let Some(func) = call_node.child_by_field_name("function") else { return };
            if func.kind() != "identifier" {
                return;
            }
            let alloc_fn = tsnode::text_of(func, source);
            if !matches!(alloc_fn, "malloc" | "calloc" | "realloc") {
                return;
            }
            let Some(args) = call_node.child_by_field_name("arguments") else { return };
            let mut cursor = args.walk();
            let Some(sizeof_node) = args.children(&mut cursor).find(|c| c.kind() == "sizeof_expression") else { return };
            let Some(mut value) = sizeof_node.child_by_field_name("value") else { return };
            // `sizeof(p)` — los paréntesis alrededor de `p` producen un
            // `parenthesized_expression` envolviendo el identificador, el
            // campo `value` apunta a ESE nodo, no al identificador directo.
            // Confirmado contra el árbol real antes de escribir esto (no
            // adivinado): `sizeof_expression value: (parenthesized_expression
            // (identifier))`.
            if value.kind() == "parenthesized_expression" {
                let Some(inner) = value.named_child(0) else { return };
                value = inner;
            }
            if value.kind() != "identifier" {
                return;
            }
            let name = tsnode::text_of(value, source);
            if !pointer_names.contains(name) {
                return;
            }
            out.push(ModernizationCandidate {
                variable: name.to_string(),
                pattern: "sizeof_pointer_allocation",
                line: call_node.start_position().row + 1,
                current: format!("`{alloc_fn}(...)` calcula el tamaño con `sizeof({name})` — `{name}` es un puntero, no la estructura a la que apunta"),
                suggested_target: "rust_ownership",
                reasoning: format!(
                    "`sizeof({name})` mide el tamaño del PUNTERO (4 u 8 bytes), no de lo que `{name}` va a apuntar — el buffer resultante queda demasiado chico, y escribir más allá de esos bytes corrompe memoria adyacente sin ningún error visible en el momento. La forma correcta en C es `sizeof(*{name})` (o `sizeof(TipoDeLaEstructura)`). En Rust, `Box::new`/`Vec::with_capacity` calculan el tamaño a partir del tipo real — esta clase de bug no puede escribirse."
                ),
                confidence: "high",
            });
        });
    });
    out
}

/// `true` si `cond` (la expresión adentro de los paréntesis de un `if`) es
/// un chequeo de null/truthiness sobre la variable `name`: `p`, `!p`,
/// `p == NULL`, `p != NULL`, `NULL == p`, `NULL != p`, y las mismas 4 formas
/// contra el literal `0` (algunas bases de código chequean contra `0` en
/// vez de `NULL`). Confirmado contra el árbol real de tree-sitter-c antes
/// de escribir esto (no adivinado): `p == NULL` es
/// `binary_expression left: (identifier) right: (null)` — `NULL` tiene su
/// propio nodo de gramática (`null`), no es un `identifier` genérico; `!p`
/// es `unary_expression operator: "!" argument: (identifier)`.
fn null_checked_variable<'a>(cond: Node, source: &'a str) -> Option<&'a str> {
    let identificador = |n: Node| (n.kind() == "identifier").then(|| tsnode::text_of(n, source));
    let es_null_o_cero =
        |n: Node| n.kind() == "null" || (n.kind() == "number_literal" && tsnode::text_of(n, source) == "0");
    match cond.kind() {
        "identifier" => identificador(cond),
        "unary_expression" => {
            let op = cond.child_by_field_name("operator")?;
            if tsnode::text_of(op, source) != "!" {
                return None;
            }
            identificador(cond.child_by_field_name("argument")?)
        }
        "binary_expression" => {
            let op = cond.child_by_field_name("operator")?;
            let op_text = tsnode::text_of(op, source);
            if op_text != "==" && op_text != "!=" {
                return None;
            }
            let (left, right) = (cond.child_by_field_name("left")?, cond.child_by_field_name("right")?);
            if es_null_o_cero(right) {
                identificador(left)
            } else if es_null_o_cero(left) {
                identificador(right)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// `true` si `node` es un uso "peligroso" de la variable `name` — una
/// desreferencia directa (`*p`), un acceso a campo (`p->campo`), o un
/// índice (`p[i]`) — las 3 formas que sí crashean si `p` es `NULL`. NO
/// cuenta pasar `p` como argumento a una función (`free(p)` es válido y
/// seguro con `p == NULL`; una función que recibe `p` puede chequearlo por
/// su cuenta) — deliberadamente más angosto que "cualquier uso", para no
/// convertir el paso más común y genuinamente seguro (pasar el puntero a
/// otra función) en un falso positivo.
fn dangerously_used_variable<'a>(node: Node, source: &'a str) -> Option<&'a str> {
    let base = |n: Node| {
        let a = n.child_by_field_name("argument")?;
        (a.kind() == "identifier").then(|| tsnode::text_of(a, source))
    };
    match node.kind() {
        "pointer_expression" => {
            let op = node.child_by_field_name("operator")?;
            if tsnode::text_of(op, source) != "*" {
                return None;
            }
            base(node)
        }
        "field_expression" | "subscript_expression" => base(node),
        _ => None,
    }
}

/// `p = malloc(...)` (o `calloc`/`realloc`) desreferenciado/indexado/con un
/// campo accedido SIN que un chequeo de null haya aparecido antes en la
/// función, dentro de la ventana [línea de adquisición, próxima adquisición
/// de la misma variable) que `use_after_free_window_end` ya calcula (una
/// reasignación a memoria nueva es lo que cierra la ventana de riesgo de
/// ESTE `malloc` en particular). Deliberadamente `malloc`/`calloc`/`realloc`
/// solamente, no `new` — `new` en C++ lanza `std::bad_alloc` por default en
/// vez de devolver null (una excepción, no un puntero inválido silencioso),
/// así que no comparte este bug a menos que se use `nothrow`, fuera de
/// alcance acá.
///
/// Heurístico textual/de orden de líneas, no un análisis de flujo de
/// control real — mismo límite que `use_after_free` ya acepta, en la
/// dirección opuesta: un chequeo que EXISTE antes del uso se cuenta como
/// protección aunque su propio cuerpo no sea lo que en verdad protege ese
/// uso (ej. `if (p == NULL) { log(...); }` sin `return`, seguido de un uso
/// incondicional, se trataría como "chequeado" acá aunque en los hechos
/// siga desprotegido) — por eso confianza `medium`, no `high`, igual que
/// `use_after_free`.
fn unchecked_malloc_dereference_candidates(
    memory: &MemoryLayoutResult,
    index: &TreeIndex,
) -> Vec<ModernizationCandidate> {
    let mut out = Vec::new();
    for a in &memory.allocations {
        if !matches!(a.kind, "malloc" | "calloc" | "realloc") {
            continue;
        }
        let Some(variable) = a.variable.as_deref() else { continue };
        let window_end = use_after_free_window_end(memory, variable, a.line);

        let checked_line = TreeIndex::primera_en_ventana(index.null_checks.get(variable), a.line, window_end);
        let first_use_line = TreeIndex::primera_en_ventana(index.dangerous_uses.get(variable), a.line, window_end);

        let Some(use_line) = first_use_line else { continue };
        let is_checked = checked_line.is_some_and(|c| c <= use_line);
        if is_checked {
            continue;
        }
        out.push(ModernizationCandidate {
            variable: variable.to_string(),
            pattern: "unchecked_malloc_dereference",
            line: use_line,
            current: format!("`{variable}` usado en la línea {use_line} sin chequeo de null detectado desde la línea {} (`{}`)", a.line, a.kind),
            suggested_target: "rust_ownership",
            reasoning: format!(
                "`{}` puede devolver `NULL` si falla la asignación de memoria — usar `{variable}` (desreferencia/índice/acceso a campo) sin chequearlo primero es un null pointer dereference real si esa asignación falla alguna vez. `Box::new`/`Vec::with_capacity` de Rust abortan en vez de devolver un puntero inválido silencioso, o el fallo se puede manejar explícitamente con `try_reserve`.",
                a.kind
            ),
            confidence: "medium",
        });
    }
    out
}

/// `(Tipo)expr` en código C++ — tree-sitter ya lo distingue como su propio
/// nodo `cast_expression` (confirmado contra el árbol real antes de escribir
/// esto: `cast_expression type: (type_descriptor ...) value: (expression)`,
/// a diferencia de una llamada tipo-función `Tipo(expr)`, que es un
/// `call_expression` distinto), así que no hace falta ninguna heurística
/// propia para separar un cast real de una expresión entre paréntesis: si el
/// parser ya lo clasificó como cast, es un cast. Deliberadamente solo se
/// llama cuando `is_cpp` — en C puro no existe una alternativa más segura a
/// la que migrar (`static_cast`/`reinterpret_cast`/`const_cast` son sintaxis
/// de C++), así que marcar cada cast de un archivo `.c` sería puro ruido sin
/// ninguna sugerencia real detrás.
fn c_style_cast_candidates(index: &TreeIndex, source: &str) -> Vec<ModernizationCandidate> {
    let mut out = Vec::new();
    visitar(&index.cast_expressions, &mut |node| {
        if node.kind() != "cast_expression" {
            return;
        }
        let Some(type_node) = node.child_by_field_name("type") else { return };
        let type_text = tsnode::text_of(type_node, source);
        out.push(ModernizationCandidate {
            variable: type_text.to_string(),
            pattern: "c_style_cast",
            line: node.start_position().row + 1,
            current: format!("`({type_text}){}` — C-style cast", tsnode::text_of(node.child_by_field_name("value").unwrap_or(node), source)),
            suggested_target: "cpp_named_cast",
            reasoning: "Un C-style cast hace silenciosamente lo que haga falta de static_cast/const_cast/reinterpret_cast (o una combinación) sin que el código diga cuál — un cast con nombre (static_cast para conversiones bien definidas, reinterpret_cast para reinterpretar punteros, const_cast para quitar constness) deja la intención explícita, es buscable con grep en todo el proyecto, y el compilador no puede llegar por accidente a una conversión más peligrosa que la que realmente se buscaba.".to_string(),
            confidence: "medium",
        });
    });
    out
}

/// Nombre de función → índice (0-based) de su argumento de formato — la
/// posición varía según cuántos parámetros vienen ANTES del formato en cada
/// función de la familia `printf`.
const FORMAT_STRING_FUNCS: &[(&str, usize)] = &[("printf", 0), ("fprintf", 1), ("sprintf", 1), ("snprintf", 2), ("syslog", 1)];


/// `printf`/`fprintf`/`sprintf`/`snprintf`/`syslog` cuyo argumento de
/// formato NO es un literal — mismo criterio que `-Wformat-nonliteral` de
/// gcc/clang. Puramente mecánico: el patrón está completo en la propia
/// llamada, sin forward-scanning ni ambigüedad de scope (igual que
/// `sizeof_pointer_allocation`).
fn format_string_vulnerability_candidates(index: &TreeIndex, source: &str) -> Vec<ModernizationCandidate> {
    let mut out = Vec::new();
    visitar(&index.call_expressions, &mut |node| {
        if node.kind() != "call_expression" {
            return;
        }
        let Some(func) = node.child_by_field_name("function") else { return };
        if func.kind() != "identifier" {
            return;
        }
        let fn_name = tsnode::text_of(func, source);
        let Some(&(_, fmt_index)) = FORMAT_STRING_FUNCS.iter().find(|(name, _)| *name == fn_name) else { return };
        let Some(args) = node.child_by_field_name("arguments") else { return };
        let mut cursor = args.walk();
        let Some(fmt_arg) = args.named_children(&mut cursor).nth(fmt_index) else { return };
        if tsnode::is_string_literal(fmt_arg) {
            return;
        }
        let arg_text = tsnode::text_of(fmt_arg, source);
        out.push(ModernizationCandidate {
            variable: arg_text.to_string(),
            pattern: "format_string_vulnerability",
            line: node.start_position().row + 1,
            current: format!("`{fn_name}(...)` usa `{arg_text}` como string de formato — no es un literal"),
            suggested_target: "rust_format_macro",
            reasoning: format!(
                "El argumento de formato de `{fn_name}` no es un literal — si `{arg_text}` puede contener datos externos, quien controle ese valor controla directivas de formato (`%x` filtra memoria del stack, `%n` la escribe) sin pasar por ningún otro argumento (CWE-134). La forma segura es `{fn_name}(\"%s\", {arg_text})`. Las macros `format!`/`println!` de Rust exigen un string literal en tiempo de COMPILACIÓN — esta clase de bug no se puede escribir, ni por accidente."
            ),
            confidence: "medium",
        });
    });
    out
}

/// Nombre de función insegura → `(confianza, alternativa sugerida)`. A
/// diferencia de todos los demás patrones de este archivo, acá no hace
/// falta mirar los argumentos de la llamada en absoluto — es el NOMBRE de
/// la función el que es inherentemente riesgoso, no una forma particular de
/// usarla.
const UNSAFE_STRING_FUNCS: &[(&str, &str, &str)] = &[
    ("gets", "high", "fgets(buf, sizeof(buf), stdin)"),
    ("strcpy", "medium", "strncpy(dst, src, sizeof(dst))"),
    ("strcat", "medium", "strncat(dst, src, sizeof(dst) - strlen(dst) - 1)"),
];

/// `gets`/`strcpy`/`strcat` — funciones de la libc estándar sin ningún
/// parámetro de tamaño del buffer destino, así que no hay forma de que la
/// función misma evite escribir más allá de sus límites. `gets` en
/// particular no tiene NINGUNA forma seguro de invocarse — por eso fue
/// eliminada del propio estándar C11, no solo desaconsejada — confianza
/// `high`. `strcpy`/`strcat` sí pueden usarse sin desbordar si el llamador
/// ya garantizó los tamaños por otra vía que este heurístico textual no
/// puede ver — confianza `medium`, mismo criterio que
/// `format_string_vulnerability`.
fn unsafe_string_function_candidates(index: &TreeIndex, source: &str) -> Vec<ModernizationCandidate> {
    let mut out = Vec::new();
    visitar(&index.call_expressions, &mut |node| {
        if node.kind() != "call_expression" {
            return;
        }
        let Some(func) = node.child_by_field_name("function") else { return };
        if func.kind() != "identifier" {
            return;
        }
        let fn_name = tsnode::text_of(func, source);
        let Some(&(_, confidence, alternative)) = UNSAFE_STRING_FUNCS.iter().find(|(name, _, _)| *name == fn_name) else { return };
        out.push(ModernizationCandidate {
            variable: fn_name.to_string(),
            pattern: "unsafe_string_function",
            line: node.start_position().row + 1,
            current: format!("`{fn_name}(...)` — sin ningún parámetro de tamaño del buffer destino"),
            suggested_target: "rust_ownership",
            reasoning: format!(
                "`{fn_name}` no puede saber cuánto espacio tiene el buffer destino, así que no puede evitar escribir más allá de sus límites si la fuente es más larga — un desbordamiento de buffer clásico (CWE-120/CWE-676). Alternativa con límite explícito: `{alternative}`. `String`/`&str` de Rust siempre validan sus propios límites y crecen solos — esta clase de desbordamiento no existe."
            ),
            confidence,
        });
    });
    out
}

/// Nombre de función de la familia `scanf` → índice (0-based) de su
/// argumento de formato, misma forma que `FORMAT_STRING_FUNCS`.
/// `fscanf(FILE*, fmt, ...)` y `sscanf(const char*, fmt, ...)` reciben su
/// primer parámetro antes del formato; `scanf(fmt, ...)` no.
const SCANF_FUNCS: &[(&str, usize)] = &[("scanf", 0), ("fscanf", 1), ("sscanf", 1)];

/// Devuelve la primera directiva de conversión de string SIN límite de
/// ancho del formato (`%s`, `%ls`, `%[a-z]`), o `None` si todas están
/// acotadas. Un `%s` sin ancho le dice a la libc "escribí en este buffer
/// hasta que se acabe la entrada" — la función no conoce el tamaño del
/// destino, así que la longitud de la entrada decide cuánto se escribe
/// (CWE-120). `%20s` sí lo acota; `%*s` lee y descarta sin escribir en
/// ningún buffer, así que tampoco es un riesgo.
///
/// Parser propio de directivas en vez de un regex — mismo criterio de
/// dependencias mínimas que `security.rs` ya aplica para lo mismo. Maneja
/// los 4 casos que importan: `%%` (porcentaje literal, no es directiva),
/// `*` (supresión de asignación), dígitos de ancho, y modificadores de
/// longitud (`h`/`l`/`ll`/`L`/`z`/`j`/`t`) entre el ancho y la conversión.
/// Para los scansets (`%[...]`) salta hasta el `]` de cierre, contemplando
/// que `%[]abc]` y `%[^]abc]` incluyen un `]` LITERAL justo después del
/// `[`/`^` que no cierra el conjunto (regla real de C, no un caso inventado).
fn first_unbounded_scan_conversion(fmt: &str) -> Option<String> {
    let chars: Vec<char> = fmt.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '%' {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        if i >= chars.len() {
            break;
        }
        if chars[i] == '%' {
            i += 1;
            continue;
        }
        let suppressed = chars[i] == '*';
        if suppressed {
            i += 1;
        }
        let width_start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        let has_width = i > width_start;
        while i < chars.len() && matches!(chars[i], 'h' | 'l' | 'L' | 'z' | 'j' | 't') {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let is_string_conv = chars[i] == 's' || chars[i] == '[';
        if chars[i] == '[' {
            i += 1;
            if i < chars.len() && chars[i] == '^' {
                i += 1;
            }
            if i < chars.len() && chars[i] == ']' {
                i += 1;
            }
            while i < chars.len() && chars[i] != ']' {
                i += 1;
            }
        }
        if is_string_conv && !has_width && !suppressed {
            let end = i.min(chars.len() - 1);
            return Some(chars[start..=end].iter().collect());
        }
        i += 1;
    }
    None
}

/// `scanf`/`fscanf`/`sscanf` con un `%s`/`%[...]` sin ancho en su formato —
/// el desbordamiento de buffer que `gets` ya representa
/// (`unsafe_string_function`), pero acá el problema NO es la función: es la
/// forma de llamarla, y la evidencia está completa en la propia llamada
/// (el formato es un literal, ahí mismo, con el ancho ausente a la vista).
/// Por eso confianza `high` aunque `strcpy`/`strcat` sean `medium`: para
/// aquellas el llamador podría haber garantizado los tamaños por fuera y
/// este heurístico no lo vería, mientras que un `%s` sin ancho es
/// literalmente "escribí hasta que se acabe la entrada" sin importar nada
/// más del programa.
///
/// Solo mira formatos LITERALES: si el formato es una variable, este patrón
/// no puede leer sus directivas y no inventa un hallazgo. Ese caso ya lo
/// cubre `format_string_vulnerability` para la familia `printf`; extenderlo
/// a `scanf` es trabajo aparte, deliberadamente no mezclado acá.
fn unbounded_scanf_candidates(index: &TreeIndex, source: &str) -> Vec<ModernizationCandidate> {
    let mut out = Vec::new();
    visitar(&index.call_expressions, &mut |node| {
        if node.kind() != "call_expression" {
            return;
        }
        let Some(func) = node.child_by_field_name("function") else { return };
        if func.kind() != "identifier" {
            return;
        }
        let fn_name = tsnode::text_of(func, source);
        let Some(&(_, fmt_index)) = SCANF_FUNCS.iter().find(|(name, _)| *name == fn_name) else { return };
        let Some(args) = node.child_by_field_name("arguments") else { return };
        let mut cursor = args.walk();
        let Some(fmt_arg) = args.named_children(&mut cursor).nth(fmt_index) else { return };
        if !tsnode::is_string_literal(fmt_arg) {
            return;
        }
        let fmt_text = tsnode::text_of(fmt_arg, source);
        let Some(directive) = first_unbounded_scan_conversion(fmt_text) else { return };
        let bounded = directive.replacen('%', "%<N>", 1);
        out.push(ModernizationCandidate {
            variable: fn_name.to_string(),
            pattern: "unbounded_scanf",
            line: node.start_position().row + 1,
            current: format!("`{fn_name}({fmt_text}, ...)` — la directiva `{directive}` no tiene límite de ancho"),
            suggested_target: "rust_ownership",
            reasoning: format!(
                "`{directive}` le pide a `{fn_name}` que escriba en el buffer destino hasta que se acabe la entrada: la longitud de lo que llegue decide cuánto se escribe, no el tamaño del buffer, así que una entrada más larga desborda (CWE-120, la misma clase de bug que `gets`). La forma acotada es dar el ancho explícito — `{bounded}`, uno menos que el tamaño del buffer para dejar lugar al `\\0`. En Rust, leer una línea a un `String` crece el buffer solo: no hay un largo máximo que adivinar ni un desbordamiento posible."
            ),
            confidence: "high",
        });
    });
    out
}

/// Punto de entrada — recibe el `MemoryLayoutResult` que `memlayout.rs` ya
/// calculó para el mismo archivo, más el `root`/`source` que `cparse.rs` ya
/// tenía a mano (nunca vuelve a parsear el texto), necesarios solo para
/// `use_after_free_candidates`. `is_cpp` (que `cparse.rs::parse_with` ya
/// tenía a mano) solo se usa para `c_style_cast_candidates`, el único patrón
/// de este archivo restringido a un lenguaje. Infalible: sin `allocations`,
/// devuelve un reporte vacío, no `None`.
pub fn analyze_c_cpp(memory: &MemoryLayoutResult, root: Node, source: &str, is_cpp: bool) -> ModernizationReport {
    let released: HashSet<&str> = memory.allocations.iter().filter(|a| is_release(a.kind)).filter_map(|a| a.variable.as_deref()).collect();

    let mut candidates: Vec<ModernizationCandidate> = memory
        .allocations
        .iter()
        .filter(|a| is_acquire(a.kind))
        .filter_map(|a| a.variable.as_deref().map(|v| (a, v)))
        .map(|(a, var)| candidate_for(var, a.kind, a.line, released.contains(var)))
        .collect();
    candidates.extend(double_release_candidates(memory));
    candidates.extend(unsafe_realloc_candidates(memory));
    // Un solo recorrido del árbol para los 2 patrones que miran "qué pasa
    // después de esta línea", en vez de uno por allocation site.
    let index = build_tree_index(root, source);
    candidates.extend(use_after_free_candidates(memory, &index));
    candidates.extend(dangling_local_reference_candidates(memory, &index, source));
    candidates.extend(sizeof_pointer_allocation_candidates(&index, source));
    candidates.extend(unchecked_malloc_dereference_candidates(memory, &index));
    if is_cpp {
        candidates.extend(c_style_cast_candidates(&index, source));
    }
    candidates.extend(format_string_vulnerability_candidates(&index, source));
    candidates.extend(unsafe_string_function_candidates(&index, source));
    candidates.extend(unbounded_scanf_candidates(&index, source));
    candidates.sort_by_key(|c| c.line);

    let manual_memory_raii = candidates.iter().filter(|c| c.pattern == "manual_memory_raii").count();
    let unmatched_allocation = candidates.iter().filter(|c| c.pattern == "unmatched_allocation").count();
    let double_release = candidates.iter().filter(|c| c.pattern == "double_release").count();
    let unsafe_realloc_reassignment = candidates.iter().filter(|c| c.pattern == "unsafe_realloc_reassignment").count();
    let use_after_free = candidates.iter().filter(|c| c.pattern == "use_after_free").count();
    let dangling_local_reference = candidates.iter().filter(|c| c.pattern == "dangling_local_reference").count();
    let sizeof_pointer_allocation = candidates.iter().filter(|c| c.pattern == "sizeof_pointer_allocation").count();
    let unchecked_malloc_dereference = candidates.iter().filter(|c| c.pattern == "unchecked_malloc_dereference").count();
    let c_style_cast = candidates.iter().filter(|c| c.pattern == "c_style_cast").count();
    let format_string_vulnerability = candidates.iter().filter(|c| c.pattern == "format_string_vulnerability").count();
    let unsafe_string_function = candidates.iter().filter(|c| c.pattern == "unsafe_string_function").count();
    let unbounded_scanf = candidates.iter().filter(|c| c.pattern == "unbounded_scanf").count();

    ModernizationReport {
        summary: ModernizationSummary {
            total: candidates.len(),
            manual_memory_raii,
            unmatched_allocation,
            double_release,
            unsafe_realloc_reassignment,
            use_after_free,
            dangling_local_reference,
            sizeof_pointer_allocation,
            unchecked_malloc_dereference,
            c_style_cast,
            format_string_vulnerability,
            unsafe_string_function,
            unbounded_scanf,
        },
        candidates,
        note: NOTE.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hallazgos::memlayout::{self, AllocationSite, MemoryLayoutSummary};
    use tree_sitter::{Parser, Tree};

    fn fake_memory(allocations: Vec<AllocationSite>) -> MemoryLayoutResult {
        MemoryLayoutResult {
            variables: vec![],
            summary: MemoryLayoutSummary { stack: 0, heap_allocations: allocations.len(), data: 0, bss: 0 },
            allocations,
            note: String::new(),
        }
    }

    fn parse_c(src: &str) -> Tree {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_c::LANGUAGE.into()).unwrap();
        parser.parse(src, None).unwrap()
    }


    /// Wrapper para los tests que no ejercitan `use_after_free` — les da un
    /// AST vacío, así que esa parte del reporte siempre queda en 0 sin que
    /// cada test tenga que lidiar con parsear una fuente real.
    fn analyze(memory: &MemoryLayoutResult) -> ModernizationReport {
        let tree = parse_c("");
        analyze_c_cpp(memory, tree.root_node(), "", false)
    }

    fn parse_cpp(src: &str) -> Tree {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_cpp::LANGUAGE.into()).unwrap();
        parser.parse(src, None).unwrap()
    }

    /// Wrapper análogo a `analyze()`, pero parseando como C++ con `is_cpp:
    /// true` — el único camino de este archivo que necesita distinguir el
    /// lenguaje (`c_style_cast_candidates`, ver el doc del módulo).
    fn analyze_cpp(src: &str) -> ModernizationReport {
        let tree = parse_cpp(src);
        let memory = fake_memory(vec![]);
        analyze_c_cpp(&memory, tree.root_node(), src, true)
    }

    #[test]
    fn malloc_con_free_emparejado_es_manual_memory_raii() {
        let memory = fake_memory(vec![
            AllocationSite { kind: "malloc", line: 2, variable: Some("p".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "free", line: 5, variable: Some("p".to_string()), reassigns_same_pointer: false },
        ]);
        let report = analyze(&memory);
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.candidates[0].pattern, "manual_memory_raii");
        assert_eq!(report.candidates[0].suggested_target, "raii_smart_pointer");
        assert_eq!(report.candidates[0].confidence, "medium");
    }

    #[test]
    fn malloc_sin_free_es_unmatched_allocation_alta_confianza() {
        let memory = fake_memory(vec![AllocationSite { kind: "malloc", line: 2, variable: Some("p".to_string()), reassigns_same_pointer: false }]);
        let report = analyze(&memory);
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.candidates[0].pattern, "unmatched_allocation");
        assert_eq!(report.candidates[0].suggested_target, "rust_ownership");
        assert_eq!(report.candidates[0].confidence, "high");
    }

    #[test]
    fn new_con_delete_es_manual_memory_raii_en_cpp() {
        let memory = fake_memory(vec![
            AllocationSite { kind: "new", line: 3, variable: Some("obj".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "delete", line: 9, variable: Some("obj".to_string()), reassigns_same_pointer: false },
        ]);
        let report = analyze(&memory);
        assert_eq!(report.candidates[0].pattern, "manual_memory_raii");
    }

    #[test]
    fn free_sin_malloc_correspondiente_no_genera_candidato() {
        // Un `free` suelto (variable llegó de otro lado, ej. un parámetro)
        // no es una adquisición — no hay nada que "modernizar" del lado de
        // la adquisición porque este heurístico nunca la vio.
        let memory = fake_memory(vec![AllocationSite { kind: "free", line: 4, variable: Some("p".to_string()), reassigns_same_pointer: false }]);
        let report = analyze(&memory);
        assert!(report.candidates.is_empty());
    }

    #[test]
    fn allocation_sin_variable_resuelta_se_ignora() {
        let memory = fake_memory(vec![AllocationSite { kind: "malloc", line: 2, variable: None, reassigns_same_pointer: false }]);
        let report = analyze(&memory);
        assert!(report.candidates.is_empty());
    }

    #[test]
    fn multiples_variables_producen_multiples_candidatos_ordenados_por_linea() {
        let memory = fake_memory(vec![
            AllocationSite { kind: "malloc", line: 10, variable: Some("b".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "malloc", line: 2, variable: Some("a".to_string()), reassigns_same_pointer: false },
        ]);
        let report = analyze(&memory);
        assert_eq!(report.candidates.len(), 2);
        assert_eq!(report.candidates[0].line, 2);
        assert_eq!(report.candidates[1].line, 10);
    }

    #[test]
    fn summary_cuenta_cada_patron_por_separado() {
        let memory = fake_memory(vec![
            AllocationSite { kind: "malloc", line: 2, variable: Some("a".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "free", line: 3, variable: Some("a".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "malloc", line: 5, variable: Some("b".to_string()), reassigns_same_pointer: false },
        ]);
        let report = analyze(&memory);
        assert_eq!(report.summary.total, 2);
        assert_eq!(report.summary.manual_memory_raii, 1);
        assert_eq!(report.summary.unmatched_allocation, 1);
    }

    #[test]
    fn sin_allocations_da_reporte_vacio() {
        let memory = fake_memory(vec![]);
        let report = analyze(&memory);
        assert!(report.candidates.is_empty());
        assert_eq!(report.summary.total, 0);
    }

    #[test]
    fn segunda_liberacion_de_la_misma_variable_es_double_release() {
        let memory = fake_memory(vec![
            AllocationSite { kind: "malloc", line: 2, variable: Some("p".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "free", line: 5, variable: Some("p".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "free", line: 8, variable: Some("p".to_string()), reassigns_same_pointer: false },
        ]);
        let report = analyze(&memory);
        let double_free = report.candidates.iter().find(|c| c.pattern == "double_release").unwrap();
        assert_eq!(double_free.line, 8); // apunta a la liberación repetida, no a la primera
        assert_eq!(double_free.confidence, "high");
        assert_eq!(double_free.suggested_target, "rust_ownership");
        assert_eq!(report.summary.double_release, 1);
    }

    #[test]
    fn una_sola_liberacion_no_es_double_release() {
        let memory = fake_memory(vec![
            AllocationSite { kind: "malloc", line: 2, variable: Some("p".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "free", line: 5, variable: Some("p".to_string()), reassigns_same_pointer: false },
        ]);
        let report = analyze(&memory);
        assert!(!report.candidates.iter().any(|c| c.pattern == "double_release"));
        assert_eq!(report.summary.double_release, 0);
    }

    #[test]
    fn tercera_liberacion_tambien_se_reporta_por_separado() {
        let memory = fake_memory(vec![
            AllocationSite { kind: "new", line: 1, variable: Some("obj".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "delete", line: 3, variable: Some("obj".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "delete", line: 6, variable: Some("obj".to_string()), reassigns_same_pointer: false },
            AllocationSite { kind: "delete", line: 9, variable: Some("obj".to_string()), reassigns_same_pointer: false },
        ]);
        let report = analyze(&memory);
        assert_eq!(report.summary.double_release, 2); // la 2da y la 3ra liberación, cada una su propio candidato
    }

    #[test]
    fn realloc_reasignado_al_mismo_puntero_es_candidato_de_alta_confianza() {
        let memory = fake_memory(vec![AllocationSite { kind: "realloc", line: 4, variable: Some("p".to_string()), reassigns_same_pointer: true }]);
        let report = analyze(&memory);
        let unsafe_realloc = report.candidates.iter().find(|c| c.pattern == "unsafe_realloc_reassignment").unwrap();
        assert_eq!(unsafe_realloc.variable, "p");
        assert_eq!(unsafe_realloc.confidence, "high");
        assert_eq!(unsafe_realloc.suggested_target, "rust_ownership");
        assert_eq!(report.summary.unsafe_realloc_reassignment, 1);
    }

    #[test]
    fn realloc_a_variable_temporal_no_dispara_el_patron_unsafe() {
        let memory = fake_memory(vec![AllocationSite { kind: "realloc", line: 4, variable: Some("tmp".to_string()), reassigns_same_pointer: false }]);
        let report = analyze(&memory);
        assert!(!report.candidates.iter().any(|c| c.pattern == "unsafe_realloc_reassignment"));
        assert_eq!(report.summary.unsafe_realloc_reassignment, 0);
    }

    // `use_after_free` es el único patrón que recorre el AST directamente
    // (no solo reinterpreta `allocations`), así que sus tests parsean
    // fuente real en vez de armar un `AllocationSite` a mano.

    #[test]
    fn uso_despues_de_liberar_se_detecta() {
        let src = "void f() {\n    int *p = malloc(sizeof(int));\n    free(p);\n    use(p);\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        let uaf = report.candidates.iter().find(|c| c.pattern == "use_after_free").unwrap();
        assert_eq!(uaf.variable, "p");
        assert_eq!(uaf.line, 4);
        assert_eq!(uaf.confidence, "medium");
        assert_eq!(uaf.suggested_target, "rust_ownership");
        assert_eq!(report.summary.use_after_free, 1);
    }

    #[test]
    fn reasignar_a_null_despues_de_liberar_no_dispara_use_after_free() {
        // `p = NULL;` es justo el idiom seguro que previene esto — no es un
        // uso, es una reasignación, así que no debe reportarse como bug.
        let src = "void f() {\n    int *p = malloc(sizeof(int));\n    free(p);\n    p = NULL;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(!report.candidates.iter().any(|c| c.pattern == "use_after_free"));
        assert_eq!(report.summary.use_after_free, 0);
    }

    #[test]
    fn reasignar_a_memoria_nueva_cierra_la_ventana_de_use_after_free() {
        // Un `malloc` posterior sobre la misma variable vuelve el puntero
        // válido de nuevo — el uso después de ESE punto no es sospechoso.
        let src = "void f() {\n    int *p = malloc(sizeof(int));\n    free(p);\n    p = malloc(sizeof(int));\n    use(p);\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(!report.candidates.iter().any(|c| c.pattern == "use_after_free"));
    }

    #[test]
    fn segunda_liberacion_no_se_duplica_como_use_after_free() {
        // La 2da liberación de `p` ya la cubre `double_release` — no debe
        // aparecer TAMBIÉN como `use_after_free` para la misma línea.
        let src = "void f() {\n    int *p = malloc(sizeof(int));\n    free(p);\n    free(p);\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(!report.candidates.iter().any(|c| c.pattern == "use_after_free"));
        assert_eq!(report.summary.double_release, 1);
    }

    // `dangling_local_reference` también recorre el AST directamente (igual
    // que `use_after_free`), así que sus tests parsean fuente real.

    #[test]
    fn devolver_direccion_de_variable_local_se_detecta() {
        let src = "int* f() {\n    int x = 5;\n    return &x;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        let d = report.candidates.iter().find(|c| c.pattern == "dangling_local_reference").unwrap();
        assert_eq!(d.variable, "x");
        assert_eq!(d.line, 3);
        assert_eq!(d.confidence, "high");
        assert_eq!(d.suggested_target, "rust_ownership");
        assert_eq!(report.summary.dangling_local_reference, 1);
    }

    #[test]
    fn devolver_direccion_de_un_parametro_tambien_se_detecta() {
        // Un parámetro es una copia local en el stack de la función — su
        // dirección queda colgando exactamente igual que una variable local.
        let src = "int* f(int x) {\n    return &x;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(report.candidates.iter().any(|c| c.pattern == "dangling_local_reference" && c.variable == "x"));
    }

    #[test]
    fn devolver_direccion_de_variable_static_no_dispara_el_patron() {
        // `static` tiene storage duration de programa completo — sigue viva
        // después del `return`, no es dangling. `memlayout` ya la clasifica
        // como "data", no "stack", así que el patrón no debería ni verla.
        let src = "int* f() {\n    static int x = 5;\n    return &x;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(!report.candidates.iter().any(|c| c.pattern == "dangling_local_reference"));
    }

    #[test]
    fn devolver_un_puntero_heap_directamente_no_dispara_el_patron() {
        // `return p;` (sin `&`) es devolver el VALOR de un puntero heap —
        // el patrón exacto acá es "return &identificador", no cualquier return.
        let src = "int* f() {\n    int *p = malloc(sizeof(int));\n    return p;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(!report.candidates.iter().any(|c| c.pattern == "dangling_local_reference"));
    }

    #[test]
    fn devolver_direccion_de_una_global_no_dispara_el_patron() {
        let src = "int g;\nint* f() {\n    return &g;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(!report.candidates.iter().any(|c| c.pattern == "dangling_local_reference"));
    }

    // `sizeof_pointer_allocation` es puramente mecánico (no necesita
    // forward-scanning), pero sí necesita AST real para distinguir un
    // puntero de un no-puntero — mismo criterio que los 2 patrones de arriba.

    #[test]
    fn malloc_sizeof_puntero_declarado_sin_inicializador_se_detecta() {
        let src = "int* f() {\n    int *p;\n    p = malloc(sizeof(p));\n    return p;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        let c = report.candidates.iter().find(|c| c.pattern == "sizeof_pointer_allocation").unwrap();
        assert_eq!(c.variable, "p");
        assert_eq!(c.confidence, "high");
        assert_eq!(c.suggested_target, "rust_ownership");
        assert_eq!(report.summary.sizeof_pointer_allocation, 1);
    }

    #[test]
    fn malloc_sizeof_puntero_con_inicializador_inline_tambien_se_detecta() {
        let src = "int* f() {\n    int *p = malloc(sizeof(p));\n    return p;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(report.candidates.iter().any(|c| c.pattern == "sizeof_pointer_allocation" && c.variable == "p"));
    }

    #[test]
    fn malloc_sizeof_de_una_variable_no_puntero_no_dispara() {
        let src = "int* f() {\n    int n = 5;\n    int *p = malloc(sizeof(n));\n    return p;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(!report.candidates.iter().any(|c| c.pattern == "sizeof_pointer_allocation"));
    }

    #[test]
    fn malloc_sizeof_dereferencia_es_la_forma_correcta_y_no_dispara() {
        // `sizeof(*p)` es EXACTAMENTE la forma correcta que este patrón no
        // debe castigar — mide el tamaño de lo apuntado, no del puntero.
        let src = "int* f() {\n    int *p;\n    p = malloc(sizeof(*p));\n    return p;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(!report.candidates.iter().any(|c| c.pattern == "sizeof_pointer_allocation"));
    }

    #[test]
    fn malloc_sizeof_de_un_tipo_no_dispara() {
        // `sizeof(int)` es un tipo, no un identificador — el campo `value`
        // del `sizeof_expression` ni siquiera existe en este caso.
        let src = "int* f() {\n    int *p = malloc(sizeof(int));\n    return p;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(!report.candidates.iter().any(|c| c.pattern == "sizeof_pointer_allocation"));
    }

    #[test]
    fn parametro_puntero_tambien_se_detecta() {
        let src = "void f(int *p) {\n    p = malloc(sizeof(p));\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(report.candidates.iter().any(|c| c.pattern == "sizeof_pointer_allocation" && c.variable == "p"));
    }

    #[test]
    fn puntero_a_puntero_tambien_se_detecta() {
        let src = "int** f() {\n    int **pp;\n    pp = malloc(sizeof(pp));\n    return pp;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(report.candidates.iter().any(|c| c.pattern == "sizeof_pointer_allocation" && c.variable == "pp"));
    }

    #[test]
    fn calloc_con_el_mismo_idiom_tambien_se_detecta() {
        // Extendido más allá de `malloc` en una pasada posterior a la
        // primera porción — ver el doc del módulo. `calloc` trae un
        // argumento extra (la cuenta) ANTES del `sizeof`, a diferencia de
        // `malloc` — confirma que el buscador no asume que el `sizeof` es
        // el único/primer argumento.
        let src = "int* f() {\n    int *p;\n    p = calloc(1, sizeof(p));\n    return p;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        let c = report.candidates.iter().find(|c| c.pattern == "sizeof_pointer_allocation").unwrap();
        assert_eq!(c.variable, "p");
        assert_eq!(c.confidence, "high");
    }

    #[test]
    fn realloc_con_el_mismo_idiom_tambien_se_detecta() {
        // `realloc(p, sizeof(p))` — el primer argumento es el propio
        // puntero `p` (un identificador suelto, no envuelto en `sizeof`);
        // el buscador debe encontrar el `sizeof_expression` del SEGUNDO
        // argumento sin confundirse con el primero.
        let src = "int* f() {\n    int *p;\n    p = realloc(p, sizeof(p));\n    return p;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        let c = report.candidates.iter().find(|c| c.pattern == "sizeof_pointer_allocation").unwrap();
        assert_eq!(c.variable, "p");
        assert_eq!(c.confidence, "high");
    }

    #[test]
    fn calloc_sizeof_dereferencia_es_la_forma_correcta_y_no_dispara() {
        // Misma forma correcta que ya se prueba para `malloc` arriba —
        // confirma que la extensión a `calloc` no perdió esa distinción.
        let src = "int* f() {\n    int *p;\n    p = calloc(1, sizeof(*p));\n    return p;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(!report.candidates.iter().any(|c| c.pattern == "sizeof_pointer_allocation"));
    }

    #[test]
    fn otras_funciones_de_asignacion_no_reconocidas_no_disparan() {
        // Solo malloc/calloc/realloc son parte de este patrón — una función
        // con el mismo idiom pero un nombre distinto (ej. un wrapper propio)
        // no debe dispararlo por casualidad de forma.
        let src = "int* f() {\n    int *p;\n    p = my_alloc(sizeof(p));\n    return p;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(!report.candidates.iter().any(|c| c.pattern == "sizeof_pointer_allocation"));
    }

    // `unchecked_malloc_dereference` necesita AST real (igual que
    // `use_after_free`/`dangling_local_reference`), así que sus tests
    // parsean fuente real en vez de armar un `AllocationSite` a mano.

    #[test]
    fn malloc_desreferenciado_sin_chequeo_se_detecta() {
        let src = "int f() {\n    int *p = malloc(sizeof(int));\n    *p = 1;\n    return 0;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        let c = report.candidates.iter().find(|c| c.pattern == "unchecked_malloc_dereference").unwrap();
        assert_eq!(c.variable, "p");
        assert_eq!(c.confidence, "medium");
        assert_eq!(c.suggested_target, "rust_ownership");
    }

    #[test]
    fn malloc_indexado_sin_chequeo_se_detecta() {
        let src = "int f() {\n    int *p = malloc(sizeof(int) * 4);\n    p[0] = 1;\n    return 0;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(report.candidates.iter().any(|c| c.pattern == "unchecked_malloc_dereference"));
    }

    #[test]
    fn malloc_campo_accedido_sin_chequeo_se_detecta() {
        let src = "int f() {\n    struct S *p = malloc(sizeof(struct S));\n    p->x = 1;\n    return 0;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(report.candidates.iter().any(|c| c.pattern == "unchecked_malloc_dereference"));
    }

    #[test]
    fn malloc_con_chequeo_if_p_no_dispara() {
        let src = "int f() {\n    int *p = malloc(sizeof(int));\n    if (p) {\n        *p = 1;\n    }\n    return 0;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(!report.candidates.iter().any(|c| c.pattern == "unchecked_malloc_dereference"));
    }

    #[test]
    fn malloc_con_chequeo_p_igual_null_no_dispara() {
        let src = "int f() {\n    int *p = malloc(sizeof(int));\n    if (p == NULL) return -1;\n    *p = 1;\n    return 0;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(!report.candidates.iter().any(|c| c.pattern == "unchecked_malloc_dereference"));
    }

    #[test]
    fn malloc_con_chequeo_negacion_no_dispara() {
        let src = "int f() {\n    int *p = malloc(sizeof(int));\n    if (!p) return -1;\n    *p = 1;\n    return 0;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(!report.candidates.iter().any(|c| c.pattern == "unchecked_malloc_dereference"));
    }

    #[test]
    fn malloc_con_chequeo_contra_cero_no_dispara() {
        let src = "int f() {\n    int *p = malloc(sizeof(int));\n    if (p != 0) {\n        *p = 1;\n    }\n    return 0;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(!report.candidates.iter().any(|c| c.pattern == "unchecked_malloc_dereference"));
    }

    #[test]
    fn malloc_pasado_como_argumento_no_es_un_uso_peligroso() {
        // Pasar `p` a otra función (`free`, o cualquier función que reciba
        // el puntero) no es una desreferencia — la función invocada puede
        // manejar `NULL` por su cuenta (`free(NULL)` es válido en C).
        let src = "int f() {\n    int *p = malloc(sizeof(int));\n    free(p);\n    return 0;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(!report.candidates.iter().any(|c| c.pattern == "unchecked_malloc_dereference"));
    }

    #[test]
    fn calloc_desreferenciado_sin_chequeo_tambien_se_detecta() {
        let src = "int f() {\n    int *p = calloc(1, sizeof(int));\n    *p = 1;\n    return 0;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(report.candidates.iter().any(|c| c.pattern == "unchecked_malloc_dereference"));
    }

    #[test]
    fn new_en_cpp_no_dispara_este_patron() {
        // `new` lanza `std::bad_alloc` por default en vez de devolver null
        // — no comparte este bug, así que el patrón nunca debe mirar `new`.
        // `fake_memory`/`analyze()` alcanzan acá: la función descarta
        // `a.kind == "new"` antes de tocar el AST en absoluto, así que ni
        // hace falta parsear C++ real para probarlo.
        let memory = fake_memory(vec![AllocationSite { kind: "new", line: 2, variable: Some("p".to_string()), reassigns_same_pointer: false }]);
        let report = analyze(&memory);
        assert!(!report.candidates.iter().any(|c| c.pattern == "unchecked_malloc_dereference"));
    }

    #[test]
    fn el_scope_por_funcion_evita_falsos_positivos_entre_funciones_distintas() {
        // `g` no tiene ninguna variable local llamada `x` — que `f` sí la
        // tenga no debe hacer que el `return &x` (inválido, no compilaría,
        // pero es lo que este test ejercita) de `g` dispare nada. Prueba
        // que el `scope` por función realmente aísla, a diferencia de los
        // demás patrones de este archivo (que emparejan por nombre en TODO
        // el archivo).
        let src = "int* f() {\n    int x = 1;\n    return &x;\n}\nint* g() {\n    int y = 2;\n    return &y;\n}\n";
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        let matches: Vec<_> = report.candidates.iter().filter(|c| c.pattern == "dangling_local_reference").collect();
        assert_eq!(matches.len(), 2);
        assert!(matches.iter().any(|c| c.variable == "x"));
        assert!(matches.iter().any(|c| c.variable == "y"));
    }

    #[test]
    fn c_style_cast_primitivo_se_detecta_en_cpp() {
        let src = "void f() {\n    double y = 3.5;\n    int x = (int)y;\n}\n";
        let report = analyze_cpp(src);
        let matches: Vec<_> = report.candidates.iter().filter(|c| c.pattern == "c_style_cast").collect();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].suggested_target, "cpp_named_cast");
        assert_eq!(matches[0].confidence, "medium");
    }

    #[test]
    fn c_style_cast_de_puntero_se_detecta_en_cpp() {
        let src = "void f(Base *b) {\n    Derived *d = (Derived*)b;\n}\n";
        let report = analyze_cpp(src);
        let matches: Vec<_> = report.candidates.iter().filter(|c| c.pattern == "c_style_cast").collect();
        assert_eq!(matches.len(), 1);
        assert!(matches[0].variable.contains("Derived"));
    }

    #[test]
    fn c_style_cast_no_dispara_en_archivos_c() {
        // Mismo texto que el test anterior, pero analizado con `is_cpp:
        // false` -- en C puro no hay static_cast/reinterpret_cast a los que
        // migrar, así que este patrón nunca debe aparecer ahí.
        let src = "void f() {\n    double y = 3.5;\n    int x = (int)y;\n}\n";
        let tree = parse_c(src);
        let memory = fake_memory(vec![]);
        let report = analyze_c_cpp(&memory, tree.root_node(), src, false);
        assert!(!report.candidates.iter().any(|c| c.pattern == "c_style_cast"));
    }

    #[test]
    fn parentesis_de_agrupacion_sin_tipo_no_es_un_cast() {
        // `(a + b)` es una expresión entre paréntesis, no un cast -- la
        // gramática de tree-sitter-cpp ya los distingue como nodos
        // distintos (`parenthesized_expression` vs. `cast_expression`), así
        // que este detector no necesita heurística propia para separarlos.
        let src = "void f(int a, int b) {\n    int x = (a + b);\n}\n";
        let report = analyze_cpp(src);
        assert!(!report.candidates.iter().any(|c| c.pattern == "c_style_cast"));
    }

    #[test]
    fn c_style_cast_note_menciona_intencion_explicita() {
        let src = "void f() {\n    double y = 3.5;\n    int x = (int)y;\n}\n";
        let report = analyze_cpp(src);
        let candidate = report.candidates.iter().find(|c| c.pattern == "c_style_cast").unwrap();
        assert!(candidate.reasoning.contains("static_cast"));
    }

    fn analyze_c_src(src: &str) -> ModernizationReport {
        let tree = parse_c(src);
        let memory = memlayout::build(tree.root_node(), src);
        analyze_c_cpp(&memory, tree.root_node(), src, false)
    }

    #[test]
    fn printf_con_variable_como_formato_se_detecta() {
        let src = "void f(char *user) {\n    printf(user);\n}\n";
        let report = analyze_c_src(src);
        let c = report.candidates.iter().find(|c| c.pattern == "format_string_vulnerability").unwrap();
        assert_eq!(c.variable, "user");
        assert_eq!(c.confidence, "medium");
        assert_eq!(c.suggested_target, "rust_format_macro");
    }

    #[test]
    fn printf_con_literal_no_dispara() {
        let src = "void f(char *user) {\n    printf(\"hello %s\\n\", user);\n}\n";
        let report = analyze_c_src(src);
        assert!(!report.candidates.iter().any(|c| c.pattern == "format_string_vulnerability"));
    }

    #[test]
    fn printf_con_literales_concatenados_no_dispara() {
        let src = "void f() {\n    printf(\"a\" \"b\");\n}\n";
        let report = analyze_c_src(src);
        assert!(!report.candidates.iter().any(|c| c.pattern == "format_string_vulnerability"));
    }

    #[test]
    fn printf_con_raw_string_literal_de_cpp_no_dispara() {
        // Bug real de auditoría (2026-09-02): `raw_string_literal`
        // (`R"(...)"`, C++11) es un nodo de gramática HERMANO de
        // `string_literal`, no reconocido antes por `is_literal_format_arg`
        // -- un literal perfectamente fijo en tiempo de compilación
        // disparaba `format_string_vulnerability` como falso positivo.
        let src = "void f(char *user) {\n    printf(R\"(Hello %s)\", user);\n}\n";
        let report = analyze_cpp(src);
        assert!(!report.candidates.iter().any(|c| c.pattern == "format_string_vulnerability"));
    }

    #[test]
    fn sprintf_toma_el_segundo_argumento_como_formato() {
        let src = "void f(char *buf, char *user) {\n    sprintf(buf, user);\n}\n";
        let report = analyze_c_src(src);
        assert!(report.candidates.iter().any(|c| c.pattern == "format_string_vulnerability" && c.variable == "user"));
    }

    #[test]
    fn sprintf_con_literal_no_dispara() {
        let src = "void f(char *buf, int x) {\n    sprintf(buf, \"%d\", x);\n}\n";
        let report = analyze_c_src(src);
        assert!(!report.candidates.iter().any(|c| c.pattern == "format_string_vulnerability"));
    }

    #[test]
    fn snprintf_toma_el_tercer_argumento_como_formato() {
        let src = "void f(char *buf, char *user) {\n    snprintf(buf, 10, user);\n}\n";
        let report = analyze_c_src(src);
        assert!(report.candidates.iter().any(|c| c.pattern == "format_string_vulnerability" && c.variable == "user"));
    }

    #[test]
    fn funcion_no_relacionada_no_dispara() {
        let src = "void f(char *user) {\n    log_message(user);\n}\n";
        let report = analyze_c_src(src);
        assert!(!report.candidates.iter().any(|c| c.pattern == "format_string_vulnerability"));
    }

    #[test]
    fn format_string_note_menciona_cwe_y_rust() {
        let src = "void f(char *user) {\n    printf(user);\n}\n";
        let report = analyze_c_src(src);
        let c = report.candidates.iter().find(|c| c.pattern == "format_string_vulnerability").unwrap();
        assert!(c.reasoning.contains("CWE-134"));
        assert!(c.reasoning.contains("format!"));
    }

    #[test]
    fn gets_se_detecta_con_confianza_alta() {
        let src = "void f(char *buf) {\n    gets(buf);\n}\n";
        let report = analyze_c_src(src);
        let c = report.candidates.iter().find(|c| c.pattern == "unsafe_string_function").unwrap();
        assert_eq!(c.variable, "gets");
        assert_eq!(c.confidence, "high");
        assert_eq!(c.suggested_target, "rust_ownership");
    }

    #[test]
    fn strcpy_se_detecta_con_confianza_media() {
        let src = "void f(char *dst, char *src) {\n    strcpy(dst, src);\n}\n";
        let report = analyze_c_src(src);
        let c = report.candidates.iter().find(|c| c.pattern == "unsafe_string_function").unwrap();
        assert_eq!(c.variable, "strcpy");
        assert_eq!(c.confidence, "medium");
    }

    #[test]
    fn strcat_tambien_se_detecta() {
        let src = "void f(char *dst, char *src) {\n    strcat(dst, src);\n}\n";
        let report = analyze_c_src(src);
        assert!(report.candidates.iter().any(|c| c.pattern == "unsafe_string_function" && c.variable == "strcat"));
    }

    #[test]
    fn strncpy_no_dispara() {
        // La variante con límite explícito es justo la alternativa segura
        // que este patrón sugiere -- no debe marcarse a sí misma.
        let src = "void f(char *dst, char *src) {\n    strncpy(dst, src, 10);\n}\n";
        let report = analyze_c_src(src);
        assert!(!report.candidates.iter().any(|c| c.pattern == "unsafe_string_function"));
    }

    #[test]
    fn multiples_llamadas_inseguras_se_reportan_por_separado() {
        let src = "void f(char *buf, char *dst, char *src) {\n    gets(buf);\n    strcpy(dst, src);\n}\n";
        let report = analyze_c_src(src);
        let matches: Vec<_> = report.candidates.iter().filter(|c| c.pattern == "unsafe_string_function").collect();
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn unsafe_string_function_note_menciona_cwe_y_alternativa() {
        let src = "void f(char *buf) {\n    gets(buf);\n}\n";
        let report = analyze_c_src(src);
        let c = report.candidates.iter().find(|c| c.pattern == "unsafe_string_function").unwrap();
        assert!(c.reasoning.contains("CWE-120"));
        assert!(c.reasoning.contains("fgets"));
    }

    // ── 12º patrón: unbounded_scanf ──────────────────────────────────────

    #[test]
    fn scanf_con_s_sin_ancho_es_candidato() {
        let src = "void f(char *buf) {\n    scanf(\"%s\", buf);\n}\n";
        let report = analyze_c_src(src);
        let c = report.candidates.iter().find(|c| c.pattern == "unbounded_scanf").unwrap();
        assert_eq!(c.confidence, "high");
        assert_eq!(c.line, 2);
        assert_eq!(c.suggested_target, "rust_ownership");
        assert!(c.reasoning.contains("CWE-120"));
        assert_eq!(report.summary.unbounded_scanf, 1);
    }

    #[test]
    fn scanf_con_ancho_explicito_no_dispara() {
        let src = "void f(char *buf) {\n    scanf(\"%19s\", buf);\n}\n";
        let report = analyze_c_src(src);
        assert!(!report.candidates.iter().any(|c| c.pattern == "unbounded_scanf"));
    }

    #[test]
    fn scanf_con_supresion_de_asignacion_no_dispara() {
        // `%*s` lee y descarta -- no escribe en ningún buffer.
        let src = "void f(void) {\n    scanf(\"%*s\");\n}\n";
        let report = analyze_c_src(src);
        assert!(!report.candidates.iter().any(|c| c.pattern == "unbounded_scanf"));
    }

    #[test]
    fn scanf_solo_con_conversiones_numericas_no_dispara() {
        let src = "void f(int *n, double *d) {\n    scanf(\"%d %lf\", n, d);\n}\n";
        let report = analyze_c_src(src);
        assert!(!report.candidates.iter().any(|c| c.pattern == "unbounded_scanf"));
    }

    #[test]
    fn porcentaje_literal_no_se_confunde_con_una_directiva() {
        // `%%` es un '%' literal -- la 's' que sigue es texto, no una
        // conversión de string.
        let src = "void f(int *n) {\n    scanf(\"100%%s %d\", n);\n}\n";
        let report = analyze_c_src(src);
        assert!(!report.candidates.iter().any(|c| c.pattern == "unbounded_scanf"));
    }

    #[test]
    fn fscanf_y_sscanf_usan_el_indice_de_formato_correcto() {
        // El formato es el 2do argumento en ambas -- si el índice estuviera
        // mal, se inspeccionaría el FILE*/buffer de entrada y no dispararía.
        let src = "void f(FILE *fp, const char *in, char *buf) {\n    fscanf(fp, \"%s\", buf);\n    sscanf(in, \"%s\", buf);\n}\n";
        let report = analyze_c_src(src);
        let matches: Vec<_> = report.candidates.iter().filter(|c| c.pattern == "unbounded_scanf").collect();
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn scanset_sin_ancho_es_candidato() {
        let src = "void f(char *buf) {\n    scanf(\"%[^\\n]\", buf);\n}\n";
        let report = analyze_c_src(src);
        assert!(report.candidates.iter().any(|c| c.pattern == "unbounded_scanf"));
    }

    #[test]
    fn scanset_con_ancho_no_dispara() {
        let src = "void f(char *buf) {\n    scanf(\"%31[^\\n]\", buf);\n}\n";
        let report = analyze_c_src(src);
        assert!(!report.candidates.iter().any(|c| c.pattern == "unbounded_scanf"));
    }

    #[test]
    fn scanset_que_empieza_con_corchete_literal_no_rompe_el_parseo() {
        // `%[]abc]` incluye un ']' LITERAL justo después del '[' -- ese no
        // cierra el conjunto. Si el parser se cortara ahí, la directiva
        // quedaría mal delimitada; igual es un scanset sin ancho, así que
        // el hallazgo sigue siendo correcto, y el `%5s` posterior (acotado)
        // no debe ser el que se reporta.
        let src = "void f(char *buf, char *b2) {\n    scanf(\"%[]abc] %5s\", buf, b2);\n}\n";
        let report = analyze_c_src(src);
        let c = report.candidates.iter().find(|c| c.pattern == "unbounded_scanf").unwrap();
        assert!(c.current.contains("%[]abc]"));
    }

    #[test]
    fn modificador_de_longitud_antes_de_s_se_reconoce() {
        // `%ls` (string ancho) sin ancho es igual de ilimitado.
        let src = "void f(wchar_t *buf) {\n    scanf(\"%ls\", buf);\n}\n";
        let report = analyze_c_src(src);
        assert!(report.candidates.iter().any(|c| c.pattern == "unbounded_scanf"));
    }

    #[test]
    fn formato_no_literal_no_inventa_un_hallazgo() {
        // Sin el texto del formato no hay directivas que leer -- este
        // patrón no adivina (ese caso es de format_string_vulnerability,
        // que hoy solo cubre la familia printf).
        let src = "void f(const char *fmt, char *buf) {\n    scanf(fmt, buf);\n}\n";
        let report = analyze_c_src(src);
        assert!(!report.candidates.iter().any(|c| c.pattern == "unbounded_scanf"));
    }

    #[test]
    fn scanf_tambien_se_detecta_en_cpp() {
        // Los 3 patrones sintácticos de este archivo aplican a C y C++ por
        // igual -- solo c_style_cast está restringido a C++.
        let src = "void f(char *buf) {\n    scanf(\"%s\", buf);\n}\n";
        let report = analyze_cpp(src);
        assert!(report.candidates.iter().any(|c| c.pattern == "unbounded_scanf"));
    }

    #[test]
    fn scanf_y_gets_conviven_como_hallazgos_independientes() {
        let src = "void f(char *buf) {\n    gets(buf);\n    scanf(\"%s\", buf);\n}\n";
        let report = analyze_c_src(src);
        assert_eq!(report.summary.unsafe_string_function, 1);
        assert_eq!(report.summary.unbounded_scanf, 1);
    }

    #[test]
    fn primera_directiva_ilimitada_es_la_que_se_reporta() {
        // Un formato con una acotada primero y una ilimitada después: el
        // hallazgo tiene que señalar la ilimitada, no la primera que vea.
        let src = "void f(char *a, char *b) {\n    scanf(\"%9s %s\", a, b);\n}\n";
        let report = analyze_c_src(src);
        let c = report.candidates.iter().find(|c| c.pattern == "unbounded_scanf").unwrap();
        assert!(c.current.contains("`%s`"));
        assert!(c.reasoning.contains("%<N>s"));
    }
}
