//! Structural smells (Fase 22, segundo ítem) — heurísticas de forma de AST,
//! mismo estilo que el resto del CS Engine: cada smell trae su umbral y su
//! razonamiento en el mensaje, nunca una etiqueta sola. Umbrales
//! convencionales de la literatura de code smells (Fowler/Martin: long
//! method ~50 LOC, large class ~15 métodos, long parameter list ~5,
//! god object combina tamaño con cantidad de estado) — no configurables
//! todavía, mismo honestidad-sobre-límites del resto del engine.
//!
//! Deliberadamente NO incluye "duplicated logic" (comparación de forma de
//! AST entre funciones) — necesita un esquema de normalización/hashing que
//! todavía no existe acá; queda para una porción siguiente, no silenciado.
//!
//! **Bug real de cobertura arreglado (2026-09-02, mismo día y mismo gap que
//! `security.rs` — atacado apenas se cerró ese)**: `check_deep_nesting`/
//! `check_quadratic_list_membership`/`check_de_morgan_simplifiable` siempre
//! fueron genéricas sobre cualquier `&[Stmt]` (nunca asumieron que `body`
//! tenía que ser el cuerpo de una función), pero `rich.rs::collect_functions`
//! solo las llamaba una vez POR FUNCIÓN — código a nivel de MÓDULO quedaba
//! sin analizar para estas 3. `rich.rs::analyze_rich` ahora también las
//! llama una vez sobre el `suite` completo del módulo (`"<module>"` como
//! nombre de scope). `check_long_function`/`check_excessive_parameters`
//! quedan deliberadamente afuera de este arreglo — ninguna tiene un
//! equivalente conceptual a nivel de módulo.

use rustpython_parser::ast::{BoolOp, CmpOp, Expr, Stmt, UnaryOp};
use serde::Serialize;

use crate::ast::parser::LineIndex;
use crate::ast::walk::walk_stmts_own_scope;

pub const LONG_FUNCTION_LOC: usize = 50;
pub const EXCESSIVE_PARAMS: usize = 5;
pub const DEEP_NESTING_DEPTH: u32 = 4;
pub const LARGE_CLASS_METHODS: usize = 15;
pub const LARGE_CLASS_LOC: usize = 300;
pub const GOD_OBJECT_METHODS: usize = 20;
pub const GOD_OBJECT_ATTRS: usize = 10;

#[derive(Clone, Debug)]
pub struct StructuralSmell {
    pub kind: &'static str,
    pub name: String,
    pub line: usize,
    pub message: String,
}

/// **La decidibilidad se agrega al SERIALIZAR, no al construir.**
///
/// Es propiedad del TIPO de hallazgo, no de la instancia: `division_by_zero`
/// es decidible siempre, `potential_false_sharing` es aproximado siempre. Así
/// que se deriva de `kind` acá, y **los ~20 módulos que emiten smells no se
/// tocaron** — el primer intento fue agregar dos campos al struct, y rompió
/// 41 sitios de construcción para repetir en cada uno un dato que ya estaba
/// determinado por `kind`.
///
/// Ver `decidibilidad.rs` para qué significa cada lado y por qué importa.
impl Serialize for StructuralSmell {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let motivo = crate::teoria::decidibilidad::motivo_de(self.kind);
        let mut st = s.serialize_struct("StructuralSmell", if motivo.is_some() { 6 } else { 5 })?;
        st.serialize_field("kind", self.kind)?;
        st.serialize_field("name", &self.name)?;
        st.serialize_field("line", &self.line)?;
        st.serialize_field("message", &self.message)?;
        st.serialize_field("decidability", crate::teoria::decidibilidad::etiqueta_de(self.kind))?;
        if let Some(m) = motivo {
            st.serialize_field("decidability_reason", m)?;
        }
        st.end()
    }
}

pub fn check_long_function(name: &str, line: usize, loc: usize) -> Option<StructuralSmell> {
    if loc <= LONG_FUNCTION_LOC {
        return None;
    }
    Some(StructuralSmell {
        kind: "long_function",
        name: name.to_string(),
        line,
        message: format!(
            "{loc} lines (> {LONG_FUNCTION_LOC}) — consider splitting into smaller, focused functions"
        ),
    })
}

pub fn check_excessive_parameters(name: &str, line: usize, arg_count: usize) -> Option<StructuralSmell> {
    if arg_count <= EXCESSIVE_PARAMS {
        return None;
    }
    Some(StructuralSmell {
        kind: "excessive_parameters",
        name: name.to_string(),
        line,
        message: format!(
            "{arg_count} parameters (> {EXCESSIVE_PARAMS}) — consider grouping related parameters into an object/dataclass"
        ),
    })
}

/// **Una cadena `elif` NO es anidamiento**, y en el AST parece que sí.
///
/// Python no tiene un nodo `elif`: `if / elif / elif / else` se codifica como
/// un `If` cuyo `orelse` contiene otro `If`, cuyo `orelse` contiene otro. Un
/// recorrido ingenuo suma un nivel por cada `elif`, así que una cadena de 5
/// —que en pantalla es **código completamente plano**— se reportaba como
/// "5 levels of nested blocks". Medido contra el sidecar antes de arreglarlo.
///
/// **La discriminación es la COLUMNA, y es exacta, no heurística.** Un `elif`
/// empieza en la misma columna que el `if` del que cuelga; un `else:` con un
/// `if` adentro empieza más indentado. Las dos formas producen el mismo AST
/// y difieren solo en eso, así que sin la columna no se pueden separar — por
/// eso esta función necesita el `LineIndex` que antes no pedía.
///
/// El lado C/C++ ya no tenía el problema (`una_cadena_else_if_no_es_anidamiento`
/// lo fija desde que `cparse.rs` lee el árbol); esta era la mitad Python.
fn es_elif(orelse: &[Stmt], columna_del_if: usize, line_index: &LineIndex) -> bool {
    matches!(orelse, [Stmt::If(interno)]
        if line_index.column_of(interno.range.start().to_usize()) == columna_del_if)
}

/// Profundidad máxima de anidamiento de CUALQUIER bloque (if/for/while/with/
/// try) — a diferencia de `bigo.rs::loop_analysis`, que solo cuenta loops
/// para el Big-O de tiempo, acá un `if` anidado en otro `if` anidado en un
/// `try` también cuenta: no afecta el tiempo de ejecución, pero sí qué tan
/// difícil es leer la función. Lo que NO cuenta es una cadena `elif`: ver
/// `es_elif` arriba.
fn max_nesting_depth(body: &[Stmt], line_index: &LineIndex) -> u32 {
    fn walk(stmts: &[Stmt], depth: u32, line_index: &LineIndex, max_depth: &mut u32) {
        for stmt in stmts {
            let (is_block, children, orelse): (bool, &[Stmt], &[Stmt]) = match stmt {
                Stmt::If(s) => (true, &s.body, &s.orelse),
                Stmt::For(s) => (true, &s.body, &s.orelse),
                Stmt::AsyncFor(s) => (true, &s.body, &s.orelse),
                Stmt::While(s) => (true, &s.body, &s.orelse),
                Stmt::With(s) => (true, &s.body, &[]),
                Stmt::AsyncWith(s) => (true, &s.body, &[]),
                Stmt::Try(s) => (true, &s.body, &s.orelse),
                _ => (false, &[], &[]),
            };
            let d = if is_block { depth + 1 } else { depth };
            *max_depth = (*max_depth).max(d);
            walk(children, d, line_index, max_depth);
            // Un `elif` es HERMANO del `if`, no hijo: se recorre al nivel de
            // afuera para que la cadena no acumule. Cualquier otro `orelse`
            // —un `else:` de verdad, el `else` de un `for`/`while`/`try`— sí
            // es un bloque adentro y va a `d`.
            let nivel_orelse = match stmt {
                Stmt::If(s) if es_elif(orelse, line_index.column_of(s.range.start().to_usize()), line_index) => depth,
                _ => d,
            };
            walk(orelse, nivel_orelse, line_index, max_depth);
            if let Stmt::Try(s) = stmt {
                for h in &s.handlers {
                    let rustpython_parser::ast::ExceptHandler::ExceptHandler(h) = h;
                    walk(&h.body, d, line_index, max_depth);
                }
                walk(&s.finalbody, d, line_index, max_depth);
            }
        }
    }
    let mut max_depth = 0u32;
    walk(body, 0, line_index, &mut max_depth);
    max_depth
}

/// Constructores de la stdlib que devuelven un contenedor MUTABLE — mismo
/// riesgo que un literal `[]`/`{}` como default: el objeto se construye una
/// sola vez, al definir la función, no en cada llamada.
const MUTABLE_CONSTRUCTORS: &[&str] =
    &["list", "dict", "set", "bytearray", "defaultdict", "OrderedDict", "Counter", "deque"];

/// `true` si `e` construye un contenedor mutable — un literal de lista/dict/
/// set (vacío o no: lo que importa es la mutabilidad, no el contenido) o una
/// llamada a uno de los constructores de arriba, con o sin punto
/// (`collections.deque()` y `deque()` cuentan igual, mismo criterio de
/// "nombre escrito tal cual" que usa el resto del motor).
fn is_mutable_default(e: &Expr) -> bool {
    match e {
        Expr::List(_) | Expr::Dict(_) | Expr::Set(_) => true,
        Expr::Call(c) => {
            let last = match &*c.func {
                Expr::Name(n) => n.id.as_str(),
                Expr::Attribute(a) => a.attr.as_str(),
                _ => return false,
            };
            MUTABLE_CONSTRUCTORS.contains(&last)
        }
        _ => false,
    }
}

/// Argumento con default mutable (`def f(items=[])`) — el default se evalúa
/// UNA sola vez, cuando se define la función, así que todas las llamadas que
/// no pasen ese argumento comparten el MISMO objeto: mutarlo en una llamada
/// lo deja mutado para la siguiente. Es de los pocos smells de este archivo
/// que no depende de ningún umbral: la forma sola ya determina el
/// comportamiento, no hay una versión "chica" que sea segura.
///
/// `memo_cache` es el nombre del dict que `classifiers::memoization_info`
/// reconoció como caché manual en esta misma función, si hubo alguno — se
/// pasa desde `rich.rs` en vez de calcularse acá para que este módulo siga
/// siendo función pura de sus argumentos, sin depender de `classifiers`.
/// Cuando el argumento marcado ES esa caché, el mensaje lo dice: la
/// memoización a mano es el caso real donde este comportamiento se usa a
/// PROPÓSITO, y llamarlo bug sin más sería un veredicto equivocado.
pub fn check_mutable_default_argument(
    name: &str,
    line: usize,
    args: &rustpython_parser::ast::Arguments,
    memo_cache: Option<&str>,
) -> Vec<StructuralSmell> {
    args.posonlyargs
        .iter()
        .chain(args.args.iter())
        .chain(args.kwonlyargs.iter())
        .filter(|a| a.default.as_deref().is_some_and(is_mutable_default))
        .map(|a| {
            let arg = a.def.arg.as_str();
            let deliberate = memo_cache == Some(arg);
            let message = if deliberate {
                format!(
                    "Parámetro `{arg}` con default mutable — evaluado una sola vez al definir la función, \
                     así que todas las llamadas que lo omitan comparten el mismo objeto. Acá eso parece \
                     DELIBERADO: `{arg}` también se detectó como caché de memoización en esta función, y \
                     ese estado compartido entre llamadas es justamente lo que la hace funcionar. Sigue \
                     siendo frágil ante un caller que sí pase su propio dict; la forma explícita es un \
                     `@functools.lru_cache` o un cache a nivel de módulo."
                )
            } else {
                format!(
                    "Parámetro `{arg}` con default mutable — el default se evalúa UNA sola vez, cuando se \
                     define la función, no en cada llamada: todas las invocaciones que omitan `{arg}` \
                     comparten el mismo objeto, así que mutarlo en una llamada lo deja mutado para la \
                     siguiente. La forma segura es `{arg}=None` y construirlo dentro del cuerpo."
                )
            };
            StructuralSmell { kind: "mutable_default_argument", name: name.to_string(), line, message }
        })
        .collect()
}

pub fn check_deep_nesting(
    name: &str,
    line: usize,
    body: &[Stmt],
    line_index: &LineIndex,
) -> Option<StructuralSmell> {
    let depth = max_nesting_depth(body, line_index);
    if depth <= DEEP_NESTING_DEPTH {
        return None;
    }
    Some(StructuralSmell {
        kind: "deep_nesting",
        name: name.to_string(),
        line,
        message: format!(
            "{depth} levels of nested blocks (> {DEEP_NESTING_DEPTH}) — consider extracting inner blocks into helper functions or using early returns"
        ),
    })
}

pub fn check_large_class(name: &str, line: usize, method_count: usize, loc: usize) -> Option<StructuralSmell> {
    if method_count <= LARGE_CLASS_METHODS && loc <= LARGE_CLASS_LOC {
        return None;
    }
    Some(StructuralSmell {
        kind: "large_class",
        name: name.to_string(),
        line,
        message: format!(
            "{method_count} methods, {loc} lines (thresholds: {LARGE_CLASS_METHODS} methods / {LARGE_CLASS_LOC} lines) — consider splitting responsibilities into smaller classes"
        ),
    })
}

/// Nombres únicos de atributos asignados vía `self.X = ...` en cualquier
/// método de la clase — proxy heurístico de cuánto estado mantiene la clase.
/// No distingue atributos "reales" de temporales reasignados en cada
/// llamada, ni sigue atributos heredados de una clase base. Devuelve un
/// `Vec` en vez de un `HashSet` a propósito: preserva el orden de primera
/// aparición (determinístico, no depende del seed aleatorio de hashing de
/// cada corrida) — importa desde que `apps/api/routers/diagram.py` empezó a
/// mostrar estos nombres directamente en un diagrama (antes solo se usaba
/// `.len()` para el check de god object, donde el orden nunca importó).
pub fn self_attribute_names(class_body: &[Stmt]) -> Vec<String> {
    let mut attrs: Vec<String> = Vec::new();
    for item in class_body {
        if let Stmt::FunctionDef(f) = item {
            let mut on_stmt = |stmt: &Stmt| {
                if let Stmt::Assign(s) = stmt {
                    for t in &s.targets {
                        if let Expr::Attribute(a) = t {
                            if matches!(&*a.value, Expr::Name(n) if n.id.as_str() == "self") {
                                let name = a.attr.to_string();
                                if !attrs.contains(&name) {
                                    attrs.push(name);
                                }
                            }
                        }
                    }
                }
            };
            let mut on_expr = |_: &Expr| {};
            walk_stmts_own_scope(&f.body, &mut on_stmt, &mut on_expr);
        }
    }
    attrs
}

pub fn check_god_object(name: &str, line: usize, method_count: usize, attribute_count: usize) -> Option<StructuralSmell> {
    if method_count < GOD_OBJECT_METHODS || attribute_count < GOD_OBJECT_ATTRS {
        return None;
    }
    Some(StructuralSmell {
        kind: "god_object",
        name: name.to_string(),
        line,
        message: format!(
            "{method_count} methods and {attribute_count} attributes (thresholds: {GOD_OBJECT_METHODS}/{GOD_OBJECT_ATTRS}) — this class is likely doing too much; consider splitting by responsibility"
        ),
    })
}

/// Variables asignadas como lista vacía literal (`x = []`) en cualquier
/// punto del cuerpo — candidatas a acumulador tipo "seen list". No sigue
/// reasignaciones posteriores (si `x` se reasigna como dict más adelante,
/// igual queda como candidata) — mismo nivel de heurística honesta que el
/// resto del engine, no un análisis de tipos real.
fn empty_list_vars(body: &[Stmt]) -> std::collections::HashSet<String> {
    let mut vars = std::collections::HashSet::new();
    let mut on_stmt = |stmt: &Stmt| {
        if let Stmt::Assign(s) = stmt {
            if let Expr::List(l) = &*s.value {
                if l.elts.is_empty() {
                    for t in &s.targets {
                        if let Expr::Name(n) = t {
                            vars.insert(n.id.to_string());
                        }
                    }
                }
            }
        }
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
    vars
}

/// Recolecta, en UN SOLO recorrido del cuerpo del loop, los nombres que
/// aparecen del lado derecho de un `in` (chequeo de membresía) y los que
/// reciben un `.append(...)` — antes esto era una función que solo
/// contestaba "¿`var` hace las dos cosas?" y `check_quadratic_list_membership`
/// la llamaba una vez POR CANDIDATO (`list_vars`), recorriendo el mismo
/// `loop_body` V veces para V candidatos en vez de una sola; acá se
/// recolectan ambos conjuntos de una pasada y el caller solo intersecta.
fn membership_and_append_vars(loop_body: &[Stmt]) -> (std::collections::HashSet<String>, std::collections::HashSet<String>) {
    let mut has_membership = std::collections::HashSet::new();
    let mut has_append = std::collections::HashSet::new();
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |e: &Expr| {
        if let Expr::Compare(c) = e {
            if c.ops.iter().any(|op| matches!(op, CmpOp::In)) {
                for comp in &c.comparators {
                    if let Expr::Name(n) = comp {
                        has_membership.insert(n.id.to_string());
                    }
                }
            }
        }
        if let Expr::Call(call) = e {
            if let Expr::Attribute(a) = &*call.func {
                if a.attr.as_str() == "append" {
                    if let Expr::Name(n) = &*a.value {
                        has_append.insert(n.id.to_string());
                    }
                }
            }
        }
    };
    walk_stmts_own_scope(loop_body, &mut on_stmt, &mut on_expr);
    (has_membership, has_append)
}

/// Recolecta todos los `for`/`async for` del cuerpo, a cualquier profundidad
/// (descendiendo por if/while/with/try, igual que `max_nesting_depth` de
/// arriba), junto con su línea y su propio cuerpo.
fn collect_for_loops<'a>(stmts: &'a [Stmt], line_index: &LineIndex, out: &mut Vec<(usize, &'a [Stmt])>) {
    for stmt in stmts {
        match stmt {
            Stmt::For(s) => {
                out.push((line_index.line_of(s.range.start().to_usize()), &s.body));
                collect_for_loops(&s.body, line_index, out);
            }
            Stmt::AsyncFor(s) => {
                out.push((line_index.line_of(s.range.start().to_usize()), &s.body));
                collect_for_loops(&s.body, line_index, out);
            }
            Stmt::If(s) => {
                collect_for_loops(&s.body, line_index, out);
                collect_for_loops(&s.orelse, line_index, out);
            }
            Stmt::While(s) => {
                collect_for_loops(&s.body, line_index, out);
                collect_for_loops(&s.orelse, line_index, out);
            }
            Stmt::With(s) => collect_for_loops(&s.body, line_index, out),
            Stmt::AsyncWith(s) => collect_for_loops(&s.body, line_index, out),
            Stmt::Try(s) => {
                collect_for_loops(&s.body, line_index, out);
                for h in &s.handlers {
                    let rustpython_parser::ast::ExceptHandler::ExceptHandler(h) = h;
                    collect_for_loops(&h.body, line_index, out);
                }
                collect_for_loops(&s.orelse, line_index, out);
                collect_for_loops(&s.finalbody, line_index, out);
            }
            _ => {}
        }
    }
}

/// Fase 14, último bullet ("nested-structure interaction detection"),
/// acotado al caso concreto de mayor valor y menor riesgo de falso
/// positivo: un acumulador inicializado como lista vacía (`seen = []`),
/// chequeado por membresía (`in`) Y ampliado (`.append(...)`) dentro del
/// mismo loop — el smell clásico que debería resolverse con un `set()`.
/// Cada chequeo de membresía en una lista es O(n); hacerlo una vez por
/// iteración de un loop que además la hace crecer es un O(n²) oculto —
/// exactamente el tipo de interacción *entre* estructuras que este ítem del
/// roadmap pide, no complejidad de una función aislada. No necesita
/// inferencia de tipos: alcanza con que la variable se haya inicializado
/// como lista vacía literal, así no hay falsos positivos sobre un dict/set
/// (donde `in` ya es O(1) — no hay nada que arreglar ahí).
pub fn check_quadratic_list_membership(name: &str, line_index: &LineIndex, body: &[Stmt]) -> Vec<StructuralSmell> {
    let list_vars = empty_list_vars(body);
    if list_vars.is_empty() {
        return Vec::new();
    }
    let mut for_loops = Vec::new();
    collect_for_loops(body, line_index, &mut for_loops);

    let mut out = Vec::new();
    for (line, loop_body) in for_loops {
        let (membership_vars, append_vars) = membership_and_append_vars(loop_body);
        for var in &list_vars {
            if membership_vars.contains(var) && append_vars.contains(var) {
                out.push(StructuralSmell {
                    kind: "quadratic_list_membership",
                    name: name.to_string(),
                    line,
                    message: format!(
                        "'{var}' is checked with 'in' and grown with .append() in the same loop — each membership check on a list is O(n), making this loop a hidden O(n\u{b2}) traversal; use a set() instead for O(1) membership checks"
                    ),
                });
            }
        }
    }
    out
}

/// Fase 15 (Mathematical Intelligence), primer ítem: Boolean algebra — De
/// Morgan. `not (a and b)` es lógicamente equivalente a `(not a) or (not b)`
/// (y viceversa para `or`) — la fase es explícita sobre el alcance: una nota
/// de legibilidad, no una reescritura automática, así que acá solo se
/// detecta y explica, nunca se transforma el código. Dispara solo con
/// exactamente 2 operandos (`a and b`, no `a and b and c`) — con 3+ la frase
/// "simplificado" dejaría de ser una única aplicación limpia de la ley y
/// pasaría a describir algo más confuso que el original.
pub fn check_de_morgan_simplifiable(name: &str, line_index: &LineIndex, body: &[Stmt]) -> Vec<StructuralSmell> {
    let mut out = Vec::new();
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |expr: &Expr| {
        let Expr::UnaryOp(u) = expr else { return };
        if !matches!(u.op, UnaryOp::Not) {
            return;
        }
        let Expr::BoolOp(b) = u.operand.as_ref() else { return };
        if b.values.len() != 2 {
            return;
        }
        let (conn, other) = match b.op {
            BoolOp::And => ("and", "or"),
            BoolOp::Or => ("or", "and"),
        };
        out.push(StructuralSmell {
            kind: "de_morgan_simplifiable",
            name: name.to_string(),
            line: line_index.line_of(u.range.start().to_usize()),
            message: format!(
                "'not (a {conn} b)' is logically equivalent to '(not a) {other} (not b)' (De Morgan's law) — may read clearer depending on context"
            ),
        });
    };
    walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::parse_module;

    /// Cuerpo de la primera función top-level del archivo.
    fn first_function_body(src: &str) -> Vec<Stmt> {
        let suite = parse_module(src).unwrap();
        for stmt in &suite {
            if let Stmt::FunctionDef(f) = stmt {
                return f.body.clone();
            }
        }
        panic!("no se encontró ninguna función en el src de prueba");
    }

    /// Cuerpo de la primera clase top-level del archivo.
    fn first_class_body(src: &str) -> Vec<Stmt> {
        let suite = parse_module(src).unwrap();
        for stmt in &suite {
            if let Stmt::ClassDef(c) = stmt {
                return c.body.clone();
            }
        }
        panic!("no se encontró ninguna clase en el src de prueba");
    }

    #[test]
    fn funcion_corta_no_dispara_long_function() {
        assert!(check_long_function("f", 1, 10).is_none());
    }

    #[test]
    fn funcion_larga_dispara_long_function() {
        let smell = check_long_function("f", 1, 51).unwrap();
        assert_eq!(smell.kind, "long_function");
    }

    #[test]
    fn pocos_parametros_no_dispara() {
        assert!(check_excessive_parameters("f", 1, 5).is_none());
    }

    #[test]
    fn muchos_parametros_dispara() {
        let smell = check_excessive_parameters("f", 1, 6).unwrap();
        assert_eq!(smell.kind, "excessive_parameters");
    }

    #[test]
    fn anidamiento_normal_no_dispara() {
        let src = "def f(x):\n    if x:\n        if x > 1:\n            return 1\n    return 0\n";
        let body = first_function_body(src);
        assert!(check_deep_nesting("f", 1, &body, &LineIndex::build(src)).is_none());
    }

    #[test]
    fn anidamiento_profundo_dispara() {
        let src = "def f(a, b, c, d, e):\n    if a:\n        if b:\n            if c:\n                if d:\n                    if e:\n                        return 1\n    return 0\n";
        let body = first_function_body(src);
        let smell = check_deep_nesting("f", 1, &body, &LineIndex::build(src)).unwrap();
        assert_eq!(smell.kind, "deep_nesting");
    }

    /// **El falso positivo que este arreglo elimina.** Una cadena `elif` es
    /// código PLANO —cero indentación creciente— y se reportaba como
    /// "5 levels of nested blocks", porque en el AST de Python cada `elif`
    /// es un `If` adentro del `orelse` del anterior.
    #[test]
    fn una_cadena_de_elif_no_es_anidamiento() {
        let src = "def clasificar(x):\n    if x > 90:\n        return 90\n    elif x > 80:\n        return 80\n    elif x > 70:\n        return 70\n    elif x > 60:\n        return 60\n    elif x > 50:\n        return 50\n    else:\n        return 0\n";
        let body = first_function_body(src);
        assert_eq!(max_nesting_depth(&body, &LineIndex::build(src)), 1, "5 elif son UN nivel");
        assert!(check_deep_nesting("clasificar", 1, &body, &LineIndex::build(src)).is_none());
    }

    /// Y la contraparte, que es lo que hace que el arreglo no sea un
    /// silenciador: un `else:` con un `if` ADENTRO sí es anidamiento. Produce
    /// exactamente el mismo AST que un `elif` y difiere solo en la columna,
    /// que es justo lo que la discriminación mira.
    #[test]
    fn un_else_con_un_if_adentro_si_es_anidamiento() {
        let elif = "def f(x):\n    if x > 1:\n        return 1\n    elif x > 0:\n        return 0\n";
        let anidado = "def f(x):\n    if x > 1:\n        return 1\n    else:\n        if x > 0:\n            return 0\n";
        let idx_e = LineIndex::build(elif);
        let idx_a = LineIndex::build(anidado);
        assert_eq!(max_nesting_depth(&first_function_body(elif), &idx_e), 1);
        assert_eq!(max_nesting_depth(&first_function_body(anidado), &idx_a), 2);
    }

    /// El `else` de un `for` NO es un `elif` y tiene que seguir contando como
    /// bloque adentro — el arreglo mira solo los `If`, pero vale fijarlo.
    #[test]
    fn el_else_de_un_for_sigue_contando_como_bloque() {
        let src = "def f(xs):\n    for x in xs:\n        pass\n    else:\n        return 1\n";
        let body = first_function_body(src);
        assert_eq!(max_nesting_depth(&body, &LineIndex::build(src)), 1);
    }

    #[test]
    fn clase_chica_no_dispara_large_class() {
        assert!(check_large_class("C", 1, 3, 50).is_none());
    }

    #[test]
    fn clase_con_muchos_metodos_dispara_large_class() {
        let smell = check_large_class("C", 1, 16, 50).unwrap();
        assert_eq!(smell.kind, "large_class");
    }

    #[test]
    fn clase_con_pocos_atributos_no_es_god_object() {
        assert!(check_god_object("C", 1, 25, 2).is_none());
    }

    #[test]
    fn clase_con_muchos_metodos_y_atributos_es_god_object() {
        let smell = check_god_object("C", 1, 25, 12).unwrap();
        assert_eq!(smell.kind, "god_object");
    }

    #[test]
    fn cuenta_atributos_self_unicos() {
        let src = "class C:\n    def __init__(self):\n        self.a = 1\n        self.b = 2\n        self.a = 3\n";
        let body = first_class_body(src);
        assert_eq!(self_attribute_names(&body), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn atributos_self_preservan_orden_de_primera_aparicion() {
        let src = "class C:\n    def __init__(self):\n        self.z = 1\n        self.a = 2\n    def other(self):\n        self.m = 3\n";
        let body = first_class_body(src);
        assert_eq!(self_attribute_names(&body), vec!["z".to_string(), "a".to_string(), "m".to_string()]);
    }

    #[test]
    fn seen_list_con_membresia_y_append_dispara_quadratic() {
        let src = "def f(items):\n    seen = []\n    for item in items:\n        if item in seen:\n            continue\n        seen.append(item)\n";
        let body = first_function_body(src);
        let smells = check_quadratic_list_membership("f", &LineIndex::build(src), &body);
        assert_eq!(smells.len(), 1);
        assert_eq!(smells[0].kind, "quadratic_list_membership");
    }

    #[test]
    fn membresia_sin_append_no_dispara() {
        let src = "def f(items):\n    allowed = []\n    for item in items:\n        if item in allowed:\n            continue\n    return items\n";
        let body = first_function_body(src);
        assert!(check_quadratic_list_membership("f", &LineIndex::build(src), &body).is_empty());
    }

    #[test]
    fn append_sin_chequeo_de_membresia_no_dispara() {
        let src = "def f(items):\n    seen = []\n    for item in items:\n        seen.append(item)\n    return seen\n";
        let body = first_function_body(src);
        assert!(check_quadratic_list_membership("f", &LineIndex::build(src), &body).is_empty());
    }

    #[test]
    fn set_en_vez_de_lista_no_dispara() {
        let src = "def f(items):\n    seen = set()\n    for item in items:\n        if item in seen:\n            continue\n        seen.add(item)\n";
        let body = first_function_body(src);
        assert!(check_quadratic_list_membership("f", &LineIndex::build(src), &body).is_empty());
    }

    #[test]
    fn sin_ninguna_lista_vacia_no_dispara() {
        let src = "def f(items):\n    for item in items:\n        print(item)\n";
        let body = first_function_body(src);
        assert!(check_quadratic_list_membership("f", &LineIndex::build(src), &body).is_empty());
    }

    #[test]
    fn dos_candidatas_solo_una_dispara_sin_contaminacion_cruzada() {
        // Regresión de la optimización de un solo walk por loop en vez de
        // uno por candidato: `seen` hace el idiom completo (in + append),
        // `other` solo hace append — el fix no debe hacer que `other`
        // "contagie" un falso positivo, ni que se pierda el de `seen`.
        let src = "def f(items):\n    seen = []\n    other = []\n    for item in items:\n        if item in seen:\n            continue\n        seen.append(item)\n        other.append(item)\n";
        let body = first_function_body(src);
        let smells = check_quadratic_list_membership("f", &LineIndex::build(src), &body);
        assert_eq!(smells.len(), 1);
        assert!(smells[0].message.contains("'seen'"));
    }

    #[test]
    fn not_and_de_2_operandos_dispara_de_morgan() {
        let src = "def f(a, b):\n    return not (a and b)\n";
        let body = first_function_body(src);
        let smells = check_de_morgan_simplifiable("f", &LineIndex::build(src), &body);
        assert_eq!(smells.len(), 1);
        assert_eq!(smells[0].kind, "de_morgan_simplifiable");
        assert!(smells[0].message.contains("and"));
        assert!(smells[0].message.contains("or"));
    }

    #[test]
    fn not_or_de_2_operandos_dispara_de_morgan() {
        let src = "def f(a, b):\n    return not (a or b)\n";
        let body = first_function_body(src);
        let smells = check_de_morgan_simplifiable("f", &LineIndex::build(src), &body);
        assert_eq!(smells.len(), 1);
        assert!(smells[0].message.contains("(not a) and (not b)"));
    }

    #[test]
    fn not_and_de_3_operandos_no_dispara() {
        let src = "def f(a, b, c):\n    return not (a and b and c)\n";
        let body = first_function_body(src);
        assert!(check_de_morgan_simplifiable("f", &LineIndex::build(src), &body).is_empty());
    }

    #[test]
    fn and_sin_not_no_dispara() {
        let src = "def f(a, b):\n    return a and b\n";
        let body = first_function_body(src);
        assert!(check_de_morgan_simplifiable("f", &LineIndex::build(src), &body).is_empty());
    }

    #[test]
    fn not_sobre_algo_que_no_es_boolop_no_dispara() {
        let src = "def f(a):\n    return not a\n";
        let body = first_function_body(src);
        assert!(check_de_morgan_simplifiable("f", &LineIndex::build(src), &body).is_empty());
    }

    #[test]
    fn de_morgan_anidado_dentro_de_condicional_tambien_se_detecta() {
        let src = "def f(a, b):\n    if not (a and b):\n        return 1\n    return 0\n";
        let body = first_function_body(src);
        assert_eq!(check_de_morgan_simplifiable("f", &LineIndex::build(src), &body).len(), 1);
    }

    // ── mutable_default_argument (2026-09-02) ────────────────────────────

    /// Firma de la primera función top-level — este es el único smell del
    /// archivo que mira los argumentos y no el cuerpo.
    fn first_function_args(src: &str) -> rustpython_parser::ast::Arguments {
        let suite = parse_module(src).unwrap();
        for stmt in &suite {
            if let Stmt::FunctionDef(f) = stmt {
                return (*f.args).clone();
            }
        }
        panic!("no se encontró ninguna función en el src de prueba");
    }

    fn mutable_defaults(src: &str) -> Vec<StructuralSmell> {
        check_mutable_default_argument("f", 1, &first_function_args(src), None)
    }

    #[test]
    fn lista_vacia_como_default_se_detecta() {
        let smells = mutable_defaults("def f(items=[]):\n    return items\n");
        assert_eq!(smells.len(), 1);
        assert_eq!(smells[0].kind, "mutable_default_argument");
        assert!(smells[0].message.contains("`items`"));
        assert!(smells[0].message.contains("items=None"));
    }

    #[test]
    fn dict_y_set_como_default_tambien_se_detectan() {
        assert_eq!(mutable_defaults("def f(a={}):\n    return a\n").len(), 1);
        assert_eq!(mutable_defaults("def f(a={1, 2}):\n    return a\n").len(), 1);
    }

    #[test]
    fn lista_no_vacia_como_default_tambien_se_detecta() {
        // Lo que importa es la mutabilidad, no si arranca vacía.
        assert_eq!(mutable_defaults("def f(items=[1, 2]):\n    return items\n").len(), 1);
    }

    #[test]
    fn constructores_mutables_se_detectan_con_y_sin_punto() {
        assert_eq!(mutable_defaults("def f(a=list()):\n    return a\n").len(), 1);
        assert_eq!(mutable_defaults("def f(a=collections.deque()):\n    return a\n").len(), 1);
        assert_eq!(mutable_defaults("def f(a=defaultdict(int)):\n    return a\n").len(), 1);
    }

    #[test]
    fn defaults_inmutables_no_disparan() {
        assert!(mutable_defaults("def f(a=None):\n    return a\n").is_empty());
        assert!(mutable_defaults("def f(a=0, b=\"\", c=(), d=False):\n    return a\n").is_empty());
        // Tupla y frozenset son inmutables -- compartirlas no tiene riesgo.
        assert!(mutable_defaults("def f(a=(1, 2), b=frozenset()):\n    return a\n").is_empty());
    }

    #[test]
    fn parametro_sin_default_no_dispara() {
        assert!(mutable_defaults("def f(items):\n    return items\n").is_empty());
    }

    #[test]
    fn varios_defaults_mutables_se_reportan_por_separado() {
        let smells = mutable_defaults("def f(a=[], b={}, c=None):\n    return a\n");
        assert_eq!(smells.len(), 2);
    }

    #[test]
    fn kwonly_arg_con_default_mutable_tambien_se_detecta() {
        // Después de `*` -- otra lista de argumentos del AST, fácil de
        // olvidar si solo se recorriera `args.args`.
        let smells = mutable_defaults("def f(x, *, opts=[]):\n    return opts\n");
        assert_eq!(smells.len(), 1);
        assert!(smells[0].message.contains("`opts`"));
    }

    #[test]
    fn cuando_el_default_es_la_cache_de_memoizacion_el_mensaje_lo_reconoce() {
        let src = "def fib(n, memo={}):\n    return n\n";
        let smells = check_mutable_default_argument("fib", 1, &first_function_args(src), Some("memo"));
        assert_eq!(smells.len(), 1);
        assert!(smells[0].message.contains("DELIBERADO"));
        assert!(smells[0].message.contains("memoización"));
    }

    #[test]
    fn una_cache_con_otro_nombre_no_ablanda_el_mensaje() {
        // El hallazgo es sobre `items`, pero la caché detectada es `memo` --
        // el mensaje suave solo aplica al argumento que ES la caché.
        let src = "def f(items=[], memo={}):\n    return items\n";
        let smells = check_mutable_default_argument("f", 1, &first_function_args(src), Some("memo"));
        assert_eq!(smells.len(), 2);
        let items = smells.iter().find(|s| s.message.contains("`items`")).unwrap();
        assert!(!items.message.contains("DELIBERADO"));
        let memo = smells.iter().find(|s| s.message.contains("`memo`")).unwrap();
        assert!(memo.message.contains("DELIBERADO"));
    }
}
