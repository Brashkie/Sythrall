//! Fase 32 (Sythrall Mathematical Analysis) — autómatas / máquinas de estados.
//!
//! **La identificación es mecánica, no adivinanza, y eso es la mitad del
//! trabajo.** "Esto parece una máquina de estados" no es analizable; lo que sí
//! lo es son dos formas concretas que el código escribe de verdad:
//!
//!   1. **Una tabla de transiciones**: un diccionario literal de
//!      diccionarios donde **al menos un valor interno es también una clave
//!      externa**. Esa superposición —estados que apuntan a estados— es lo que
//!      distingue una tabla de transiciones de cualquier otro dict anidado, y
//!      es la condición que se exige antes de afirmar nada.
//!   2. **Una variable de estado**: un `self.state` / `estado` / `status` /
//!      `phase` / `mode` al que se le asignan **al menos dos literales
//!      distintos**. Menos de dos no es una máquina, es una bandera.
//!
//! **Con la tabla a la vista, dos cosas son demostrables** — y no con la misma
//! fuerza, que es lo que hay que decir:
//!
//!   - **Una transición a un estado que no tiene entrada propia no depende de
//!     ninguna suposición.** Todas las claves de la tabla están ahí mismo: si
//!     el destino no es una de ellas, un `TABLA[estado][evento]` revienta con
//!     `KeyError` apenas la máquina llegue. Esto es un hecho.
//!   - **Un estado sin transiciones entrantes sí depende de una**: que el
//!     primero declarado sea el inicial. Se exime al primero y se dice en el
//!     mensaje, en vez de esconderlo.
//!
//! **Con la variable de estado, la comparación es entre lo escrito y lo
//! leído**, y cada dirección significa algo distinto: un valor que se asigna y
//! nadie consulta es un estado al que la máquina entra y nadie atiende; un
//! valor que se consulta y nadie asigna es una rama muerta. Un `case _:` o un
//! `else` final callan la primera —ahí sí se está atendiendo genéricamente— y
//! por eso se detectan explícitamente.
//!
//! **El alcance de una clase es la clase entera, no el método.** `__init__`
//! escribe el estado inicial y `step()` lo lee: analizarlos por separado no
//! vería nada. Por eso este módulo lleva su propio recorrido en vez de
//! colgarse del "una vez por función" del resto.

use std::collections::{HashMap, HashSet};

use rustpython_parser::ast::{CmpOp, Constant, Expr, Pattern, Ranged, Stmt};

use crate::ast::parser::LineIndex;
use crate::hallazgos::smells::StructuralSmell;
use crate::ast::structure::node_name;
use crate::ast::walk::{walk_stmts, walk_stmts_own_scope};

/// Nombres que identifican la variable de estado. Se compara EXACTO (tras
/// sacar los `_` de adelante y bajar a minúsculas), no por subcadena:
/// `state_history` o `estados` no son la variable de estado.
const NOMBRES_DE_ESTADO: &[&str] = &["state", "estado", "status", "phase", "fase", "mode", "modo"];

fn es_nombre_de_estado(bruto: &str) -> bool {
    let n = bruto.trim_start_matches('_').to_lowercase();
    NOMBRES_DE_ESTADO.contains(&n.as_str())
}

/// La variable de estado que esta expresión nombra, si lo es.
fn variable_de_estado(e: &Expr) -> Option<String> {
    match e {
        Expr::Name(n) if es_nombre_de_estado(&n.id) => Some(n.id.to_string()),
        Expr::Attribute(a) if es_nombre_de_estado(&a.attr) => Some(node_name(e)),
        _ => None,
    }
}

/// Un valor de estado escrito como literal. Cubre las tres formas que se usan:
/// una cadena, un entero, y un miembro de enum (`Estado.IDLE`), que se
/// reconoce por la convención de mayúsculas — sin ella no habría forma de
/// distinguir `Estado.IDLE` de `self.contador`.
fn literal_de_estado(e: &Expr) -> Option<String> {
    match e {
        Expr::Constant(c) => match &c.value {
            Constant::Str(s) => Some(format!("{s:?}")),
            Constant::Int(i) => Some(i.to_string()),
            _ => None,
        },
        Expr::Attribute(a) => {
            let attr = a.attr.as_str();
            let en_mayusculas = attr.chars().any(|c| c.is_alphabetic())
                && attr.chars().all(|c| c.is_uppercase() || c.is_numeric() || c == '_');
            en_mayusculas.then(|| node_name(e))
        }
        _ => None,
    }
}

/// Una fila de la tabla: el estado, la línea donde se declara, y sus destinos
/// con la línea de cada uno.
type Fila = (String, usize, Vec<(String, usize)>);

/// `true` si el dict es una tabla de transiciones, con sus filas.
///
/// La condición que la separa de cualquier otro dict anidado —un diccionario
/// de traducciones, por ejemplo— es que **al menos un destino sea también una
/// clave**: estados que apuntan a estados. Sin esa superposición no se afirma
/// nada.
fn tabla_de_transiciones(e: &Expr, line_index: &LineIndex) -> Option<Vec<Fila>> {
    let Expr::Dict(d) = e else { return None };
    if d.keys.len() < 2 {
        return None;
    }
    let mut filas: Vec<Fila> = Vec::new();
    for (clave, valor) in d.keys.iter().zip(d.values.iter()) {
        let clave = clave.as_ref()?;
        let estado = literal_de_estado(clave)?;
        let Expr::Dict(interno) = valor else { return None };
        let mut destinos = Vec::new();
        for v in &interno.values {
            let destino = literal_de_estado(v)?;
            destinos.push((destino, line_index.line_of(v.range().start().to_usize())));
        }
        filas.push((estado, line_index.line_of(clave.range().start().to_usize()), destinos));
    }
    let claves: HashSet<&str> = filas.iter().map(|(k, _, _)| k.as_str()).collect();
    let hay_superposicion =
        filas.iter().flat_map(|(_, _, d)| d.iter()).any(|(d, _)| claves.contains(d.as_str()));
    hay_superposicion.then_some(filas)
}

fn revisar_tablas(scope: &str, body: &[Stmt], line_index: &LineIndex, out: &mut Vec<StructuralSmell>) {
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |e: &Expr| {
        let Some(filas) = tabla_de_transiciones(e, line_index) else { return };
        let claves: HashSet<&str> = filas.iter().map(|(k, _, _)| k.as_str()).collect();
        let destinos: HashSet<&str> =
            filas.iter().flat_map(|(_, _, d)| d.iter()).map(|(d, _)| d.as_str()).collect();

        // Destinos sin entrada propia. No descansa en ninguna suposición.
        let mut vistos: HashSet<&str> = HashSet::new();
        for (destino, linea) in filas.iter().flat_map(|(_, _, d)| d.iter()) {
            if claves.contains(destino.as_str()) || !vistos.insert(destino.as_str()) {
                continue;
            }
            out.push(StructuralSmell {
                kind: "undefined_state_transition",
                name: scope.to_string(),
                line: *linea,
                message: format!(
                    "A transition leads to {destino}, which has no entry of its own in this table — there is nothing to read for the next step, so a lookup like `TABLE[state][event]` raises KeyError as soon as the machine lands there. Unlike the unreachable-state check, this rests on no assumption about which state is initial: every key of the table is right here, and {destino} is not one of them."
                ),
            });
        }

        // Estados a los que no llega ninguna transición. El primero declarado
        // se exime: es el inicial, y decirlo es parte del hallazgo.
        let Some((primero, _, _)) = filas.first() else { return };
        for (estado, linea, _) in filas.iter().skip(1) {
            if destinos.contains(estado.as_str()) {
                continue;
            }
            out.push(StructuralSmell {
                kind: "unreachable_state",
                name: scope.to_string(),
                line: *linea,
                message: format!(
                    "State {estado} is declared in this transition table but is not the destination of any transition, so nothing in the table can put the machine into it. The first key ({primero}) is taken as the initial state and exempted — that is the one assumption this check makes, and it is stated rather than hidden. If {estado} is meant to be entered some other way (set directly, or restored from storage) then the table simply is not the whole story."
                ),
            });
        }
    };
    walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
}

/// Lo que se hace con una variable de estado dentro de un alcance.
#[derive(Default)]
struct Uso {
    escritos: Vec<(String, usize)>,
    leidos: HashSet<String>,
    /// Un `case _:` o un `else` final: los estados no contemplados SÍ se
    /// están atendiendo, genéricamente.
    comodin: bool,
}

/// Valores literales que un patrón de `match` compara. `case _` y
/// `case otro` (captura) devuelven `None`: son comodines.
fn valores_del_patron(p: &Pattern, out: &mut Vec<String>) -> bool {
    match p {
        Pattern::MatchValue(v) => {
            if let Some(l) = literal_de_estado(&v.value) {
                out.push(l);
            }
            false
        }
        Pattern::MatchOr(o) => o.patterns.iter().any(|x| valores_del_patron(x, out)),
        // `case _` y `case nombre` capturan todo.
        Pattern::MatchAs(a) => a.pattern.is_none(),
        _ => false,
    }
}

/// `true` si la cadena `if/elif` termina en un `else` pelado.
fn termina_en_else(orelse: &[Stmt]) -> bool {
    match orelse.first() {
        None => false,
        Some(Stmt::If(i)) => termina_en_else(&i.orelse),
        Some(_) => true,
    }
}

fn recolectar_usos(body: &[Stmt], profundo: bool) -> HashMap<String, Uso> {
    let mut usos: HashMap<String, Uso> = HashMap::new();

    // Pase 1 — lecturas por comparación. Va aparte del pase 2 porque las dos
    // mitades de `walk_stmts` no pueden capturar el mismo mapa a la vez.
    {
        let mut on_stmt = |_: &Stmt| {};
        let mut on_expr = |e: &Expr| {
            let Expr::Compare(c) = e else { return };
            if c.ops.len() != 1 || !matches!(c.ops[0], CmpOp::Eq | CmpOp::NotEq | CmpOp::In | CmpOp::NotIn) {
                return;
            }
            let derecha = &c.comparators[0];
            let (variable, valores) = match variable_de_estado(&c.left) {
                Some(v) => (v, derecha),
                None => match variable_de_estado(derecha) {
                    Some(v) => (v, &*c.left),
                    None => return,
                },
            };
            let entrada = usos.entry(variable).or_default();
            match valores {
                // `estado in ("a", "b")` lee los dos.
                Expr::Tuple(t) => {
                    for x in &t.elts {
                        if let Some(l) = literal_de_estado(x) {
                            entrada.leidos.insert(l);
                        }
                    }
                }
                Expr::List(t) => {
                    for x in &t.elts {
                        if let Some(l) = literal_de_estado(x) {
                            entrada.leidos.insert(l);
                        }
                    }
                }
                Expr::Set(t) => {
                    for x in &t.elts {
                        if let Some(l) = literal_de_estado(x) {
                            entrada.leidos.insert(l);
                        }
                    }
                }
                otro => {
                    if let Some(l) = literal_de_estado(otro) {
                        entrada.leidos.insert(l);
                    }
                }
            }
        };
        if profundo {
            walk_stmts(body, &mut on_stmt, &mut on_expr);
        } else {
            walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
        }
    }

    // Pase 2 — escrituras, `match` y comodines.
    {
        let mut on_stmt = |s: &Stmt| match s {
            Stmt::Assign(a) => {
                let Some(valor) = literal_de_estado(&a.value) else { return };
                let linea = a.range.start().to_usize();
                for t in &a.targets {
                    if let Some(v) = variable_de_estado(t) {
                        usos.entry(v).or_default().escritos.push((valor.clone(), linea));
                    }
                }
            }
            Stmt::AnnAssign(a) => {
                let (Some(valor), Some(v)) =
                    (a.value.as_deref().and_then(literal_de_estado), variable_de_estado(&a.target))
                else {
                    return;
                };
                usos.entry(v).or_default().escritos.push((valor, a.range.start().to_usize()));
            }
            Stmt::Match(m) => {
                let Some(v) = variable_de_estado(&m.subject) else { return };
                let entrada = usos.entry(v).or_default();
                for caso in &m.cases {
                    let mut valores = Vec::new();
                    if valores_del_patron(&caso.pattern, &mut valores) {
                        entrada.comodin = true;
                    }
                    for l in valores {
                        entrada.leidos.insert(l);
                    }
                }
            }
            Stmt::If(i) => {
                if !termina_en_else(&i.orelse) {
                    return;
                }
                // Un `else` final atiende a lo que las ramas no cubren, pero
                // solo para la variable que la cadena consulta.
                let mut vistas: Vec<String> = Vec::new();
                let mut mirar = |e: &Expr| {
                    if let Expr::Compare(c) = e {
                        for lado in [&*c.left, &c.comparators[0]] {
                            if let Some(v) = variable_de_estado(lado) {
                                vistas.push(v);
                            }
                        }
                    }
                };
                crate::ast::walk::walk_expr_tree(&i.test, &mut mirar);
                for v in vistas {
                    usos.entry(v).or_default().comodin = true;
                }
            }
            _ => {}
        };
        let mut sin_expr = |_: &Expr| {};
        if profundo {
            walk_stmts(body, &mut on_stmt, &mut sin_expr);
        } else {
            walk_stmts_own_scope(body, &mut on_stmt, &mut sin_expr);
        }
    }

    usos
}

fn revisar_variable(
    scope: &str,
    body: &[Stmt],
    profundo: bool,
    line_index: &LineIndex,
    out: &mut Vec<StructuralSmell>,
) {
    for (variable, uso) in recolectar_usos(body, profundo) {
        let escritos: HashSet<&str> = uso.escritos.iter().map(|(v, _)| v.as_str()).collect();
        // Menos de dos estados distintos no es una máquina, es una bandera.
        if escritos.len() < 2 {
            continue;
        }

        if !uso.comodin {
            let mut ya: HashSet<&str> = HashSet::new();
            for (valor, offset) in &uso.escritos {
                if uso.leidos.contains(valor) || !ya.insert(valor.as_str()) {
                    continue;
                }
                out.push(StructuralSmell {
                    kind: "unhandled_state",
                    name: scope.to_string(),
                    line: line_index.line_of(*offset),
                    message: format!(
                        "`{variable}` is set to {valor} here, but nothing in `{scope}` ever tests for it — no `case {valor}:`, no `== {valor}`. The machine can enter this state and then no branch reacts to it. There is no `case _:` or trailing `else` either, so it is not being handled generically."
                    ),
                });
            }
        }

        let mut sin_asignar: Vec<&String> = uso.leidos.iter().filter(|v| !escritos.contains(v.as_str())).collect();
        sin_asignar.sort();
        for valor in sin_asignar {
            out.push(StructuralSmell {
                kind: "unreachable_state",
                name: scope.to_string(),
                line: line_index.line_of(uso.escritos[0].1),
                message: format!(
                    "`{scope}` branches on `{variable}` being {valor}, but {valor} is never assigned to `{variable}` anywhere in this scope — the branch cannot be taken. That is only a bug if the value is supposed to come from here: a constructor argument, a database row or another class could still set it, and this analysis sees only this scope."
                ),
            });
        }
    }
}

/// Punto de entrada, sobre el módulo entero. Ver el doc del módulo: el alcance
/// de una clase es la clase completa, no cada método.
pub fn state_machine_smells(suite: &[Stmt], line_index: &LineIndex) -> Vec<StructuralSmell> {
    let mut out = Vec::new();
    recorrer(suite, "<module>", false, line_index, &mut out);
    out.sort_by_key(|s| s.line);
    out
}

fn recorrer(
    body: &[Stmt],
    scope: &str,
    dentro_de_clase: bool,
    line_index: &LineIndex,
    out: &mut Vec<StructuralSmell>,
) {
    revisar_tablas(scope, body, line_index, out);
    if !dentro_de_clase {
        revisar_variable(scope, body, false, line_index, out);
    }
    for stmt in body {
        match stmt {
            Stmt::ClassDef(c) => {
                // La clase entera como UN alcance: `__init__` escribe el
                // estado inicial y otro método lo lee.
                revisar_variable(&c.name, &c.body, true, line_index, out);
                recorrer(&c.body, &c.name, true, line_index, out);
            }
            Stmt::FunctionDef(f) => recorrer(&f.body, &f.name, dentro_de_clase, line_index, out),
            Stmt::AsyncFunctionDef(f) => recorrer(&f.body, &f.name, dentro_de_clase, line_index, out),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::parse_module;

    fn hallazgos(src: &str) -> Vec<StructuralSmell> {
        let suite = parse_module(src).expect("parse error");
        let idx = LineIndex::build(src);
        state_machine_smells(&suite, &idx)
    }

    fn tiene(src: &str, kind: &str) -> bool {
        hallazgos(src).iter().any(|s| s.kind == kind)
    }

    // ── Tabla de transiciones ──

    const TABLA_ROTA: &str = concat!(
        "TRANSICIONES = {\n",
        "    \"idle\": {\"start\": \"running\"},\n",
        "    \"running\": {\"stop\": \"idle\", \"fail\": \"broken\"},\n",
        "    \"paused\": {\"resume\": \"running\"},\n",
        "}\n",
    );

    #[test]
    fn un_destino_sin_entrada_propia_se_prueba() {
        // `broken` es destino pero no es clave: TABLA["broken"] revienta.
        assert!(tiene(TABLA_ROTA, "undefined_state_transition"), "{:?}", hallazgos(TABLA_ROTA));
    }

    #[test]
    fn un_estado_sin_transiciones_entrantes_se_marca() {
        // A `paused` no llega nadie, y no es el primero (que se exime como
        // inicial).
        let h = hallazgos(TABLA_ROTA);
        let f = h.iter().find(|s| s.kind == "unreachable_state").expect("esperaba unreachable_state");
        assert!(f.message.contains("paused"), "{}", f.message);
    }

    #[test]
    fn el_primer_estado_se_exime_por_ser_el_inicial() {
        // A `idle` tampoco llega nadie en esta tabla, pero es el primero.
        let h = hallazgos(TABLA_ROTA);
        assert!(
            !h.iter().any(|s| s.kind == "unreachable_state" && s.message.contains("State \"idle\"")),
            "{h:?}"
        );
    }

    #[test]
    fn una_tabla_consistente_no_produce_nada() {
        let src = concat!(
            "T = {\n",
            "    \"idle\": {\"start\": \"running\"},\n",
            "    \"running\": {\"stop\": \"idle\"},\n",
            "}\n",
        );
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src));
    }

    #[test]
    fn un_dict_anidado_que_no_es_una_tabla_no_se_toca() {
        // Sin superposicion entre claves y valores no hay estados apuntando a
        // estados: es un diccionario de traducciones, no una maquina.
        let src = concat!(
            "T = {\n",
            "    \"es\": {\"hello\": \"hola\"},\n",
            "    \"fr\": {\"hello\": \"bonjour\"},\n",
            "}\n",
        );
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src));
    }

    #[test]
    fn un_dict_plano_no_es_una_tabla() {
        assert!(hallazgos("T = {\"a\": \"b\", \"b\": \"a\"}\n").is_empty());
    }

    // ── Variable de estado ──

    #[test]
    fn un_estado_que_nadie_atiende_se_marca() {
        let src = concat!(
            "class M:\n",
            "    def __init__(self):\n",
            "        self.state = \"idle\"\n",
            "    def arrancar(self):\n",
            "        self.state = \"running\"\n",
            "    def fallar(self):\n",
            "        self.state = \"broken\"\n",
            "    def paso(self):\n",
            "        if self.state == \"idle\":\n",
            "            return 0\n",
            "        if self.state == \"running\":\n",
            "            return 1\n",
        );
        let h = hallazgos(src);
        let f = h.iter().find(|s| s.kind == "unhandled_state").expect("esperaba unhandled_state");
        assert!(f.message.contains("broken"), "{}", f.message);
    }

    #[test]
    fn el_alcance_de_una_clase_es_la_clase_entera() {
        // `__init__` escribe y `paso` lee: analizarlos por separado no veria
        // nada. El scope reportado es la clase.
        let src = concat!(
            "class M:\n",
            "    def __init__(self):\n",
            "        self.state = \"a\"\n",
            "    def otro(self):\n",
            "        self.state = \"b\"\n",
            "    def paso(self):\n",
            "        if self.state == \"z\":\n",
            "            return 1\n",
        );
        let h = hallazgos(src);
        assert!(h.iter().any(|s| s.name == "M"), "{h:?}");
    }

    #[test]
    fn un_case_comodin_calla_el_no_atendido() {
        let src = concat!(
            "class M:\n",
            "    def __init__(self):\n",
            "        self.state = \"idle\"\n",
            "    def fallar(self):\n",
            "        self.state = \"broken\"\n",
            "    def paso(self):\n",
            "        match self.state:\n",
            "            case \"idle\":\n",
            "                return 0\n",
            "            case _:\n",
            "                return -1\n",
        );
        assert!(!tiene(src, "unhandled_state"), "{:?}", hallazgos(src));
    }

    #[test]
    fn un_else_final_tambien_calla() {
        let src = concat!(
            "class M:\n",
            "    def __init__(self):\n",
            "        self.state = \"idle\"\n",
            "    def fallar(self):\n",
            "        self.state = \"broken\"\n",
            "    def paso(self):\n",
            "        if self.state == \"idle\":\n",
            "            return 0\n",
            "        else:\n",
            "            return -1\n",
        );
        assert!(!tiene(src, "unhandled_state"), "{:?}", hallazgos(src));
    }

    #[test]
    fn una_rama_sobre_un_estado_que_nadie_asigna_es_inalcanzable() {
        let src = concat!(
            "class M:\n",
            "    def __init__(self):\n",
            "        self.state = \"idle\"\n",
            "    def arrancar(self):\n",
            "        self.state = \"running\"\n",
            "    def paso(self):\n",
            "        if self.state == \"zombie\":\n",
            "            return 9\n",
            "        return 0\n",
        );
        let h = hallazgos(src);
        let f = h.iter().find(|s| s.kind == "unreachable_state").expect("esperaba unreachable_state");
        assert!(f.message.contains("zombie"), "{}", f.message);
    }

    #[test]
    fn una_sola_asignacion_no_es_una_maquina() {
        // Una bandera no es un automata: con un solo estado no se afirma nada.
        let src = concat!(
            "class M:\n",
            "    def __init__(self):\n",
            "        self.state = \"idle\"\n",
            "    def paso(self):\n",
            "        if self.state == \"otro\":\n",
            "            return 1\n",
        );
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src));
    }

    #[test]
    fn un_enum_cuenta_como_literal() {
        let src = concat!(
            "class M:\n",
            "    def __init__(self):\n",
            "        self.state = Estado.IDLE\n",
            "    def arrancar(self):\n",
            "        self.state = Estado.RUNNING\n",
            "    def fallar(self):\n",
            "        self.state = Estado.BROKEN\n",
            "    def paso(self):\n",
            "        if self.state == Estado.IDLE:\n",
            "            return 0\n",
            "        if self.state == Estado.RUNNING:\n",
            "            return 1\n",
        );
        let h = hallazgos(src);
        assert!(
            h.iter().any(|s| s.kind == "unhandled_state" && s.message.contains("Estado.BROKEN")),
            "{h:?}"
        );
    }

    #[test]
    fn un_in_sobre_una_tupla_cuenta_como_lectura() {
        let src = concat!(
            "class M:\n",
            "    def __init__(self):\n",
            "        self.state = \"a\"\n",
            "    def otro(self):\n",
            "        self.state = \"b\"\n",
            "    def paso(self):\n",
            "        if self.state in (\"a\", \"b\"):\n",
            "            return 1\n",
            "        return 0\n",
        );
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src));
    }

    #[test]
    fn una_variable_que_no_se_llama_estado_no_se_toca() {
        let src = concat!(
            "class M:\n",
            "    def __init__(self):\n",
            "        self.color = \"rojo\"\n",
            "    def otro(self):\n",
            "        self.color = \"azul\"\n",
        );
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src));
    }

    #[test]
    fn un_nombre_parecido_no_alcanza() {
        // `state_history` no es la variable de estado: la comparacion es
        // exacta, no por subcadena.
        let src = concat!(
            "class M:\n",
            "    def __init__(self):\n",
            "        self.state_history = \"a\"\n",
            "    def otro(self):\n",
            "        self.state_history = \"b\"\n",
        );
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src));
    }
}
