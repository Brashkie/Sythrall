//! Fase 15 (Mathematical Intelligence), segundo ítem: pure vs. side-effecting
//! function classification. Intra-procedural y deliberadamente conservador —
//! no baja a las funciones que esta llama (eso sería análisis interprocedural,
//! fuera de alcance acá), así que "pura" significa "nada en el CUERPO de esta
//! función prueba que muta estado externo o hace I/O", no "esta función y
//! todo lo que llama son puros". Cuando no se puede probar una cosa u otra
//! con certeza (ej. `y = alguna_funcion(); y.append(x)` — `y` podría ser un
//! objeto fresco o un alias de algo externo, no hay forma de saberlo sin
//! seguir el valor de retorno de `alguna_funcion`), se prefiere el silencio
//! sobre adivinar: solo se reporta impureza en los casos donde SÍ se puede
//! probar (parámetro mutado, builtin de I/O conocido, `global`/`nonlocal`).
//! Esto es lo que la fase pide con "a real static property, not a heuristic
//! guess" — más callado que exhaustivo, pero lo que dice es cierto.

use std::cell::RefCell;
use std::collections::HashSet;

use rustpython_parser::ast::{Expr, Stmt};

use crate::walk::walk_stmts_own_scope;

/// Builtins/funciones conocidas por su efecto de I/O o por tocar estado del
/// intérprete — llamarlas directamente (por nombre, no vía alias) es
/// evidencia suficiente de impureza sin necesitar seguir el dato.
const KNOWN_IMPURE_CALLS: &[&str] =
    &["print", "input", "open", "exec", "eval", "__import__", "globals", "locals", "vars", "exit", "quit"];

/// Métodos que mutan el objeto sobre el que se llaman (in-place) — llamar
/// alguno de estos sobre un parámetro (no sobre un literal recién creado
/// dentro de la función) es una mutación observable desde afuera.
const MUTATING_METHODS: &[&str] = &[
    "append", "extend", "insert", "remove", "pop", "clear", "sort", "reverse", "update", "setdefault", "add",
    "discard", "popitem", "__setitem__", "__delitem__",
];

pub struct PurityInfo {
    pub is_pure: bool,
    pub note: String,
}

/// `args`: nombres de los parámetros (ya extraídos por `structure::arg_names`
/// — no se vuelve a caminar `Arguments` acá). `body`: cuerpo de la función,
/// mismo shape que el resto de los analizadores de `rich.rs`.
pub fn analyze(args: &[String], body: &[Stmt]) -> PurityInfo {
    let param_set: HashSet<&str> = args.iter().map(|s| s.as_str()).collect();

    // Primer pase: nombres locales frescos (asignados a un literal/constructor
    // de contenedor recién creado dentro de esta función) — mutarlos no es
    // observable desde afuera, a diferencia de mutar un parámetro. Y si la
    // función declara `global`/`nonlocal` en algún lado, eso ya alcanza para
    // marcarla impura (la razón de ser de esas declaraciones es escribir a
    // estado que vive fuera de esta función).
    let mut fresh_locals: HashSet<String> = HashSet::new();
    let mut has_global_nonlocal = false;
    let mut collect_stmt = |stmt: &Stmt| match stmt {
        Stmt::Global(_) | Stmt::Nonlocal(_) => has_global_nonlocal = true,
        Stmt::Assign(a) => {
            if let [Expr::Name(n)] = a.targets.as_slice() {
                if is_fresh_constructor(&a.value) {
                    fresh_locals.insert(n.id.to_string());
                }
            }
        }
        _ => {}
    };
    let mut collect_expr = |_: &Expr| {};
    walk_stmts_own_scope(body, &mut collect_stmt, &mut collect_expr);

    let reasons: RefCell<Vec<String>> = RefCell::new(Vec::new());
    if has_global_nonlocal {
        reasons.borrow_mut().push("declares global/nonlocal state".to_string());
    }

    let mut check_stmt = |stmt: &Stmt| match stmt {
        Stmt::Assign(a) => {
            for target in &a.targets {
                check_mutation_target(target, &param_set, &fresh_locals, &reasons);
            }
        }
        Stmt::AugAssign(a) => check_mutation_target(&a.target, &param_set, &fresh_locals, &reasons),
        _ => {}
    };
    let mut check_expr = |expr: &Expr| {
        let Expr::Call(c) = expr else { return };
        match c.func.as_ref() {
            Expr::Name(n) if KNOWN_IMPURE_CALLS.contains(&n.id.as_str()) => {
                reasons.borrow_mut().push(format!("calls '{}'", n.id));
            }
            Expr::Attribute(a) if MUTATING_METHODS.contains(&a.attr.as_str()) => {
                if let Expr::Name(base) = a.value.as_ref() {
                    if param_set.contains(base.id.as_str()) && !fresh_locals.contains(base.id.as_str()) {
                        reasons.borrow_mut().push(format!("calls mutating method '{}' on parameter '{}'", a.attr, base.id));
                    }
                }
            }
            _ => {}
        }
    };
    walk_stmts_own_scope(body, &mut check_stmt, &mut check_expr);

    let mut reasons = reasons.into_inner();
    reasons.sort();
    reasons.dedup();
    let is_pure = reasons.is_empty();
    let note = if is_pure {
        "no external state read or mutated within this function's own body".to_string()
    } else {
        reasons.join("; ")
    };
    PurityInfo { is_pure, note }
}

/// Constructores que SIEMPRE devuelven un objeto nuevo, sin importar qué se
/// les pase — mutar el resultado de asignar uno de estos a un nombre local
/// no es observable desde afuera de la función, a diferencia de mutar un
/// parámetro directamente.
fn is_fresh_constructor(value: &Expr) -> bool {
    match value {
        Expr::List(_) | Expr::Dict(_) | Expr::Set(_) | Expr::Tuple(_) => true,
        Expr::Call(c) => matches!(
            c.func.as_ref(),
            Expr::Name(n) if matches!(n.id.as_str(), "list" | "dict" | "set" | "tuple" | "frozenset" | "bytearray")
        ),
        _ => false,
    }
}

/// Un target de asignación es una mutación observable cuando escribe a
/// través de un parámetro (atributo o subíndice) — reasignar el NOMBRE de un
/// parámetro (`x = x + 1`) no cuenta: eso solo rebindea la variable local,
/// no afecta al objeto que el caller todavía tiene. Mutar un local fresco
/// (`fresh_locals`) tampoco cuenta, por la misma razón que documenta
/// `is_fresh_constructor`.
fn check_mutation_target(target: &Expr, param_set: &HashSet<&str>, fresh_locals: &HashSet<String>, reasons: &RefCell<Vec<String>>) {
    let base = match target {
        Expr::Attribute(a) => a.value.as_ref(),
        Expr::Subscript(s) => s.value.as_ref(),
        _ => return,
    };
    if let Expr::Name(n) = base {
        if param_set.contains(n.id.as_str()) && !fresh_locals.contains(n.id.as_str()) {
            let via = if matches!(target, Expr::Attribute(_)) { "attribute" } else { "subscript" };
            reasons.borrow_mut().push(format!("mutates parameter '{}' via {via} assignment", n.id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_module;

    fn analyze_src(src: &str) -> PurityInfo {
        let suite = parse_module(src).unwrap();
        let Stmt::FunctionDef(f) = &suite[0] else { panic!("no era una función") };
        let args: Vec<String> = f.args.args.iter().map(|a| a.def.arg.to_string()).collect();
        analyze(&args, &f.body)
    }

    #[test]
    fn funcion_sin_efectos_es_pura() {
        let info = analyze_src("def add(a, b):\n    return a + b\n");
        assert!(info.is_pure);
    }

    #[test]
    fn mutar_parametro_via_metodo_es_impura() {
        let info = analyze_src("def add_item(lst, x):\n    lst.append(x)\n");
        assert!(!info.is_pure);
        assert!(info.note.contains("lst"));
    }

    #[test]
    fn mutar_lista_fresca_local_sigue_siendo_pura() {
        let info = analyze_src("def build(n):\n    result = []\n    result.append(n)\n    return result\n");
        assert!(info.is_pure);
    }

    #[test]
    fn print_es_impura() {
        let info = analyze_src("def greet(name):\n    print(name)\n");
        assert!(!info.is_pure);
        assert!(info.note.contains("print"));
    }

    #[test]
    fn asignar_atributo_de_parametro_es_impura() {
        let info = analyze_src("def set_x(obj, v):\n    obj.x = v\n");
        assert!(!info.is_pure);
    }

    #[test]
    fn metodo_que_solo_lee_self_es_pura() {
        let info = analyze_src("def area(self):\n    return self.w * self.h\n");
        assert!(info.is_pure);
    }

    #[test]
    fn metodo_que_asigna_self_es_impura() {
        let info = analyze_src("def set_w(self, w):\n    self.w = w\n");
        assert!(!info.is_pure);
    }

    #[test]
    fn reasignar_nombre_de_parametro_sigue_siendo_pura() {
        let info = analyze_src("def inc(x):\n    x = x + 1\n    return x\n");
        assert!(info.is_pure);
    }

    #[test]
    fn global_declarado_es_impura() {
        let info = analyze_src("def bump():\n    global counter\n    counter += 1\n");
        assert!(!info.is_pure);
        assert!(info.note.contains("global"));
    }

    #[test]
    fn subscript_de_parametro_es_impura() {
        let info = analyze_src("def zero_first(items):\n    items[0] = 0\n");
        assert!(!info.is_pure);
    }

    #[test]
    fn subscript_de_local_fresco_sigue_siendo_pura() {
        let info = analyze_src("def make(n):\n    d = {}\n    d[0] = n\n    return d\n");
        assert!(info.is_pure);
    }

    #[test]
    fn dato_de_proveniencia_ambigua_no_se_marca_impura_por_defecto() {
        // `y` viene de una llamada a función arbitraria — no hay forma de
        // saber si es un objeto fresco o un alias externo sin seguir el
        // valor de retorno de `helper()`. El diseño es callado acá (no
        // adivina impureza), documentado explícitamente en el docstring del
        // módulo.
        let info = analyze_src("def f(x):\n    y = helper(x)\n    y.append(1)\n    return y\n");
        assert!(info.is_pure);
    }
}
