//! Control-Flow Graph (Fase 15, Mathematical Intelligence) — la estructura
//! que a este motor le faltaba.
//!
//! **Por qué éste antes que cualquier otro análisis nuevo.** Tres módulos del
//! crate dicen textualmente que se detienen porque no hay un CFG:
//! `asmparse.rs` (dos veces) y `callingconv.rs`. Y hay hallazgos ya
//! shippeados que reportan confianza `medium` por el mismo motivo:
//! `use_after_free` y `unchecked_malloc_dereference` de `modernization.rs`
//! son heurísticos de ORDEN DE LÍNEAS — un `free` en una rama seguido de un
//! uso en la otra los engaña, y eso está documentado como límite aceptado.
//! Un CFG es lo que convierte esos `medium` en demostrables.
//!
//! **Grafo a nivel de sentencia, no de bloque básico.** Un CFG "de verdad"
//! agrupa sentencias consecutivas sin saltos en bloques básicos. Acá cada
//! sentencia es un nodo. La diferencia es de compresión, no de corrección:
//! la alcanzabilidad, los caminos y los ciclos son idénticos en los dos, y
//! la versión por sentencia hace que cada hallazgo apunte a una línea real
//! sin tener que mapear de vuelta desde un rango. Agrupar en bloques básicos
//! es una optimización posterior, no un requisito, y se dice acá en vez de
//! dejar creer que esto es menos de lo que es.
//!
//! **Qué modela y qué no.** Modela el flujo estructurado de Python:
//! secuencia, `if`/`else`, `while`/`for` (con su arista de retroceso y su
//! `else`), `try`/`except`/`finally`, `with`, `match`, y los cuatro
//! terminadores (`return`, `raise`, `break`, `continue`). NO modela: saltos
//! entre funciones (es intra-procedural, una función a la vez), ni la
//! condición de cada rama (no evalúa si `x > 5` puede ser verdadero — eso es
//! interpretación abstracta, otro análisis). Una excepción puede ocurrir en
//! cualquier sentencia, no solo dentro de un `try`; acá se modela solo el
//! flujo explícito, que es el que se puede afirmar leyendo.

use std::collections::{HashSet, VecDeque};

use rustpython_parser::ast::{Constant, Expr, ExprContext, Ranged, Stmt};
use serde::Serialize;

use crate::ast::parser::LineIndex;
use crate::hallazgos::smells::StructuralSmell;

/// Un nodo del grafo: una sentencia, con su línea real.
#[derive(Serialize, Clone)]
pub struct CfgNode {
    pub id: usize,
    pub line: usize,
    /// Tipo de sentencia (`if`, `while`, `return`, …) — sirve para leer el
    /// grafo sin volver al fuente.
    pub kind: &'static str,
}

/// `label` explica POR QUÉ existe la arista: sin eso, un grafo de `if` con
/// dos salidas es indistinguible de una secuencia con dos sucesores.
#[derive(Serialize, Clone)]
pub struct CfgEdge {
    pub from: usize,
    pub to: usize,
    pub label: &'static str,
}

/// Variables que una sentencia define y lee **por sí misma**, sin contar los
/// cuerpos anidados — cada sentencia de adentro es su propio nodo.
///
/// Se calcula en `nodo()`, que es el único lugar donde nace un nodo, para no
/// tener un segundo recorrido que haya que mantener en sincronía con el
/// primero. Es lo que `ssa.rs` necesita y el grafo no guardaba.
#[derive(Clone, Default)]
pub struct VarsNodo {
    pub defs: Vec<String>,
    pub usos: Vec<String>,
    /// Subconjunto de `defs` cuyo destino es un nombre pelado (`x = ...`), no
    /// una tupla, un atributo ni un subíndice. Es el mismo criterio que
    /// `muertas.rs::nombre_asignado` — un desempaquetado define igual, pero
    /// no se puede pedir que se evite recibir la mitad que no se usa.
    pub defs_simples: Vec<String>,
}

#[derive(Serialize, Clone)]
pub struct FunctionCfg {
    pub function: String,
    pub nodes: Vec<CfgNode>,
    pub edges: Vec<CfgEdge>,
    /// Paralelo a `nodes` por índice. `serde(skip)`: es información interna
    /// para el análisis, no parte del contrato JSON que ya consume el panel.
    #[serde(skip)]
    pub vars: Vec<VarsNodo>,
    /// Por dónde entra el flujo. `None` si el cuerpo está vacío.
    #[serde(skip)]
    pub entry: Option<usize>,
    /// Nodos a los que NO llega ningún camino desde la entrada. Es el primer
    /// resultado que solo un CFG puede dar: ninguna heurística de texto
    /// distingue "código después de un `return`" de "código después de un
    /// `while True` sin `break`", y los dos son inalcanzables.
    pub unreachable: Vec<usize>,
}

/// Salidas de un fragmento de código: por dónde puede seguir el flujo.
/// `normal` son las que continúan a la sentencia siguiente. Las otras tres
/// se propagan hacia arriba hasta encontrar quién las recibe: un `break` sale
/// del loop más cercano, un `continue` vuelve a su cabecera, y un `return`/
/// `raise` sale de la función entera.
#[derive(Default)]
struct Salidas {
    normal: Vec<usize>,
    breaks: Vec<usize>,
    continues: Vec<usize>,
    returns: Vec<usize>,
}

impl Salidas {
    fn absorber_no_locales(&mut self, otra: &mut Salidas) {
        self.breaks.append(&mut otra.breaks);
        self.continues.append(&mut otra.continues);
        self.returns.append(&mut otra.returns);
    }
}

struct Constructor<'a> {
    nodes: Vec<CfgNode>,
    edges: Vec<CfgEdge>,
    vars: Vec<VarsNodo>,
    line_index: &'a LineIndex,
}

/// Lo que una sentencia define y lee **de su parte propia**. Los cuerpos
/// anidados no se tocan: cada sentencia de adentro es su propio nodo del
/// grafo y aporta lo suyo ahí.
///
/// **Dos casos se salen del patrón y por eso están escritos aparte:**
///
/// - `x += 1` **lee y escribe** `x`. El AST marca el target `Store`, así que
///   tomarlo solo como escritura perdería la lectura y haría creer que el
///   valor anterior se descarta.
/// - Un `def`/`class` anidado lee lo que quiera de su clausura, y su cuerpo
///   **no** genera nodos. Se lo trata como lector de todos los nombres que
///   aparecen adentro: pasarse de conservador acá evita afirmar que una
///   variable capturada por una clausura no se usa.
fn vars_de(stmt: &Stmt) -> VarsNodo {
    let mut v = VarsNodo::default();
    match stmt {
        Stmt::Assign(s) => {
            for t in &s.targets {
                if let Expr::Name(n) = t {
                    v.defs_simples.push(n.id.to_string());
                }
                nombres(t, &mut v.defs, &mut v.usos);
            }
            nombres(&s.value, &mut v.defs, &mut v.usos);
        }
        Stmt::AugAssign(s) => {
            nombres(&s.target, &mut v.defs, &mut v.usos);
            // El target de un `+=` también se LEE, y el AST no lo dice.
            v.usos.extend(v.defs.iter().cloned());
            nombres(&s.value, &mut v.defs, &mut v.usos);
        }
        Stmt::AnnAssign(s) => {
            if s.value.is_some() {
                if let Expr::Name(n) = &*s.target {
                    v.defs_simples.push(n.id.to_string());
                }
                nombres(&s.target, &mut v.defs, &mut v.usos);
            }
            if let Some(val) = &s.value {
                nombres(val, &mut v.defs, &mut v.usos);
            }
        }
        Stmt::Return(s) => {
            if let Some(e) = &s.value {
                nombres(e, &mut v.defs, &mut v.usos);
            }
        }
        Stmt::Expr(s) => nombres(&s.value, &mut v.defs, &mut v.usos),
        Stmt::If(s) => nombres(&s.test, &mut v.defs, &mut v.usos),
        Stmt::While(s) => nombres(&s.test, &mut v.defs, &mut v.usos),
        Stmt::For(s) => target_de_for(&s.target, &s.iter, &mut v),
        Stmt::AsyncFor(s) => target_de_for(&s.target, &s.iter, &mut v),
        Stmt::With(s) => items_with(&s.items, &mut v),
        Stmt::AsyncWith(s) => items_with(&s.items, &mut v),
        // **`try` y `match` sobre-aproximan sus LECTURAS a propósito.** Sus
        // partes propias no se enumeran bien desde afuera: el tipo de un
        // `except exc_type as e` se evalúa al saltar la excepción, y un
        // `case` puede leer nombres desde el patrón (`case Color.RED:`), que
        // no es una expresión normal. En vez de enumerarlas a medias, se
        // cuentan como lectura todos los nombres leídos en cualquier parte
        // de la sentencia. Sobrar en lecturas solo hace PERDER hallazgos;
        // faltar los inventa — `_pytest/outcomes.py` reportaba mal el
        // `exc_type = ModuleNotFoundError` que un `except exc_type` lee.
        Stmt::Try(_) | Stmt::Match(_) => {
            let propio = std::slice::from_ref(stmt);
            let mut on_stmt = |_: &Stmt| {};
            let mut on_expr = |e: &Expr| {
                if let Expr::Name(n) = e {
                    if matches!(n.ctx, ExprContext::Load) {
                        v.usos.push(n.id.to_string());
                    }
                }
            };
            crate::ast::walk::walk_stmts(propio, &mut on_stmt, &mut on_expr);
        }
        Stmt::Assert(s) => {
            nombres(&s.test, &mut v.defs, &mut v.usos);
            if let Some(m) = &s.msg {
                nombres(m, &mut v.defs, &mut v.usos);
            }
        }
        Stmt::Raise(s) => {
            for e in s.exc.iter().chain(s.cause.iter()) {
                nombres(e, &mut v.defs, &mut v.usos);
            }
        }
        Stmt::Delete(s) => {
            for t in &s.targets {
                nombres(t, &mut v.defs, &mut v.usos);
            }
        }
        Stmt::FunctionDef(_) | Stmt::AsyncFunctionDef(_) | Stmt::ClassDef(_) => {
            let nombre = match stmt {
                Stmt::FunctionDef(f) => f.name.to_string(),
                Stmt::AsyncFunctionDef(f) => f.name.to_string(),
                Stmt::ClassDef(c) => c.name.to_string(),
                _ => unreachable!(),
            };
            v.defs.push(nombre);
            // Todo lo que se lea adentro cuenta como lectura acá.
            let cuerpo = std::slice::from_ref(stmt);
            let mut on_stmt = |_: &Stmt| {};
            let mut on_expr = |e: &Expr| {
                if let Expr::Name(n) = e {
                    if matches!(n.ctx, ExprContext::Load) {
                        v.usos.push(n.id.to_string());
                    }
                }
            };
            crate::ast::walk::walk_stmts(cuerpo, &mut on_stmt, &mut on_expr);
        }
        _ => {}
    }
    v
}

/// **La variable de un `for` se define solo si el iterable da al menos un
/// elemento.** `i = 0` seguido de `for i in range(0):` deja `i` en 0 — el
/// bucle no corre y la asignación de arriba se lee después. Si el nodo del
/// `for` matara esa versión, se reportaría como pisada una escritura que el
/// autor puso justamente para ese caso (`_pytest/assertion/compare_text.py`
/// lo tiene con el comentario *"just in case left or right has zero length"*,
/// y era un falso positivo real de la primera versión de esto).
///
/// Se resuelve marcando el target también como **lectura**: el valor que
/// entraba sigue vivo. El precio es perder el caso en que la escritura previa
/// sí era inútil, y es el precio correcto — el motor prefiere perder
/// hallazgos antes que inventarlos.
fn target_de_for(target: &Expr, iter: &Expr, v: &mut VarsNodo) {
    let mut defs = Vec::new();
    nombres(target, &mut defs, &mut v.usos);
    v.usos.extend(defs.iter().cloned());
    v.defs.extend(defs);
    nombres(iter, &mut v.defs, &mut v.usos);
}

fn items_with(items: &[rustpython_parser::ast::WithItem], v: &mut VarsNodo) {
    for it in items {
        nombres(&it.context_expr, &mut v.defs, &mut v.usos);
        if let Some(o) = &it.optional_vars {
            nombres(o, &mut v.defs, &mut v.usos);
        }
    }
}

/// Junta nombres de un árbol de expresión separando lectura de escritura.
///
/// **El contexto lo marca el propio AST** (`Load`/`Store`/`Del`), así que la
/// distinción es exacta y no una heurística de forma.
///
/// **Y no se baja a buscar DEFINICIONES dentro de comprehensions ni lambdas**,
/// aunque sí lecturas. En Python 3 la variable de una comprehension vive en su
/// propio ámbito: en `xs = [i for i in range(3)]` ese `i` está marcado `Store`
/// pero **no** es el `i` de la función. Contarlo como una definición más
/// inventaría una escritura que nadie hizo, y sobre eso se construirían
/// hallazgos falsos.
fn nombres(expr: &Expr, defs: &mut Vec<String>, usos: &mut Vec<String>) {
    // Una sola pasada por el árbol: `walk_expr_tree` ya lo recorre entero.
    let mut ajenos: HashSet<String> = HashSet::new();
    crate::ast::walk::walk_expr_tree(expr, &mut |e: &Expr| match e {
        Expr::Name(n) => match n.ctx {
            ExprContext::Load => usos.push(n.id.to_string()),
            ExprContext::Store | ExprContext::Del => defs.push(n.id.to_string()),
        },
        // Los ligados por una comprehension o una lambda se anotan aparte y
        // después se descuentan de `defs`.
        Expr::ListComp(c) => ligados_de(&c.generators, &mut ajenos),
        Expr::SetComp(c) => ligados_de(&c.generators, &mut ajenos),
        Expr::DictComp(c) => ligados_de(&c.generators, &mut ajenos),
        Expr::GeneratorExp(c) => ligados_de(&c.generators, &mut ajenos),
        Expr::Lambda(l) => {
            for a in l.args.args.iter().chain(l.args.posonlyargs.iter()).chain(l.args.kwonlyargs.iter()) {
                ajenos.insert(a.def.arg.to_string());
            }
        }
        _ => {}
    });
    if !ajenos.is_empty() {
        defs.retain(|d| !ajenos.contains(d));
    }
}

/// Nombres que una comprehension liga en su propio ámbito.
fn ligados_de(generators: &[rustpython_parser::ast::Comprehension], out: &mut HashSet<String>) {
    for g in generators {
        crate::ast::walk::walk_expr_tree(&g.target, &mut |e: &Expr| {
            if let Expr::Name(n) = e {
                out.insert(n.id.to_string());
            }
        });
    }
}

impl<'a> Constructor<'a> {
    fn nodo(&mut self, stmt: &Stmt, kind: &'static str) -> usize {
        let id = self.nodes.len();
        let line = self.line_index.line_of(stmt.range().start().to_usize());
        self.nodes.push(CfgNode { id, line, kind });
        self.vars.push(vars_de(stmt));
        id
    }

    fn arista(&mut self, from: usize, to: usize, label: &'static str) {
        self.edges.push(CfgEdge { from, to, label });
    }

    fn conectar(&mut self, desde: &[usize], hasta: usize, label: &'static str) {
        for d in desde {
            self.arista(*d, hasta, label);
        }
    }

    /// Construye el subgrafo de una lista de sentencias. Devuelve por dónde
    /// entra (`None` si está vacía) y por dónde sale.
    fn suite(&mut self, body: &[Stmt]) -> (Option<usize>, Salidas) {
        let mut entrada: Option<usize> = None;
        let mut pendientes: Vec<usize> = Vec::new();
        let mut salidas = Salidas::default();
        // `true` cuando el flujo normal ya se cortó (hubo return/raise/
        // break/continue): todo lo que siga en esta suite queda sin arista
        // de entrada, que es exactamente lo que lo vuelve inalcanzable.
        let mut cortado = false;

        for stmt in body {
            let (ent, mut sal) = self.sentencia(stmt);
            let Some(ent) = ent else { continue };
            if entrada.is_none() {
                entrada = Some(ent);
            }
            if !cortado {
                self.conectar(&pendientes, ent, "seq");
            }
            salidas.absorber_no_locales(&mut sal);
            cortado = sal.normal.is_empty();
            pendientes = sal.normal;
        }
        salidas.normal = pendientes;
        (entrada, salidas)
    }

    fn sentencia(&mut self, stmt: &Stmt) -> (Option<usize>, Salidas) {
        let mut sal = Salidas::default();
        match stmt {
            Stmt::Return(_) => {
                let n = self.nodo(stmt, "return");
                sal.returns.push(n);
                (Some(n), sal)
            }
            Stmt::Raise(_) => {
                let n = self.nodo(stmt, "raise");
                sal.returns.push(n);
                (Some(n), sal)
            }
            Stmt::Break(_) => {
                let n = self.nodo(stmt, "break");
                sal.breaks.push(n);
                (Some(n), sal)
            }
            Stmt::Continue(_) => {
                let n = self.nodo(stmt, "continue");
                sal.continues.push(n);
                (Some(n), sal)
            }
            Stmt::If(s) => {
                let n = self.nodo(stmt, "if");
                let (ent_si, mut sal_si) = self.suite(&s.body);
                if let Some(e) = ent_si {
                    self.arista(n, e, "true");
                }
                let (ent_no, mut sal_no) = self.suite(&s.orelse);
                if let Some(e) = ent_no {
                    self.arista(n, e, "false");
                }
                sal.absorber_no_locales(&mut sal_si);
                sal.absorber_no_locales(&mut sal_no);
                sal.normal.extend(sal_si.normal);
                sal.normal.extend(sal_no.normal);
                // Sin `else`, la rama falsa cae directo en lo que sigue.
                if ent_no.is_none() {
                    sal.normal.push(n);
                }
                // Un `if` cuyo cuerpo termina en `return` y no tiene `else`
                // igual deja pasar el flujo por la rama falsa: eso ya está
                // cubierto arriba y es la diferencia con un heurístico de
                // "hay un return, cortá".
                if ent_si.is_none() {
                    sal.normal.push(n);
                }
                (Some(n), sal)
            }
            Stmt::While(s) => {
                let n = self.nodo(stmt, "while");
                let (ent, mut sal_cuerpo) = self.suite(&s.body);
                if let Some(e) = ent {
                    self.arista(n, e, "true");
                }
                // Arista de retroceso: fin del cuerpo y cada `continue`
                // vuelven a la cabecera. Es lo que hace del grafo un ciclo.
                self.conectar(&sal_cuerpo.normal, n, "loop");
                self.conectar(&sal_cuerpo.continues, n, "continue");
                sal_cuerpo.continues.clear();

                let (ent_else, mut sal_else) = self.suite(&s.orelse);
                if let Some(e) = ent_else {
                    self.arista(n, e, "false");
                }
                sal.absorber_no_locales(&mut sal_cuerpo);
                sal.absorber_no_locales(&mut sal_else);
                // `while True:` sin `break` NO tiene salida normal — lo que
                // venga después es inalcanzable. Ninguna heurística de texto
                // ve esto; el grafo sí.
                if !es_condicion_siempre_verdadera(&s.test) {
                    if ent_else.is_none() {
                        sal.normal.push(n);
                    }
                    sal.normal.extend(sal_else.normal);
                }
                sal.normal.extend(std::mem::take(&mut sal.breaks));
                (Some(n), sal)
            }
            Stmt::For(s) => self.bucle_for(stmt, &s.body, &s.orelse),
            Stmt::AsyncFor(s) => self.bucle_for(stmt, &s.body, &s.orelse),
            Stmt::With(s) => self.bloque_simple(stmt, "with", &s.body),
            Stmt::AsyncWith(s) => self.bloque_simple(stmt, "with", &s.body),
            Stmt::Try(s) => {
                let n = self.nodo(stmt, "try");
                let (ent, mut sal_body) = self.suite(&s.body);
                if let Some(e) = ent {
                    self.arista(n, e, "seq");
                }
                // **Las salidas del cuerpo y las de los handlers se llevan
                // SEPARADAS, y el motivo es el `else`.** En Python el `else`
                // de un `try` corre si y solo si NO hubo excepción: nunca
                // después de un handler. Mezclarlas dibujaba una arista
                // handler → else, y con ella el motor creía que lo que el
                // handler asigna lo pisa el `else` enseguida. Encontrado
                // midiendo `setuptools/_distutils/command/build_ext.py`,
                // donde el `suffix` del `except` salía reportado como
                // escritura pisada teniéndose que leer.
                let normales_body = sal_body.normal.clone();
                let mut normales_handlers: Vec<usize> = Vec::new();
                sal.absorber_no_locales(&mut sal_body);

                for h in &s.handlers {
                    let rustpython_parser::ast::ExceptHandler::ExceptHandler(h) = h;
                    let (ent_h, mut sal_h) = self.suite(&h.body);
                    if let Some(e) = ent_h {
                        // La arista sale del `try`, no del final del cuerpo:
                        // la excepción puede saltar desde cualquier punto de
                        // adentro, no solo desde la última sentencia.
                        self.arista(n, e, "except");
                    }
                    sal.absorber_no_locales(&mut sal_h);
                    normales_handlers.extend(sal_h.normal);
                }
                let (ent_else, mut sal_else) = self.suite(&s.orelse);
                let mut normales: Vec<usize>;
                if let Some(e) = ent_else {
                    // Solo el cuerpo entra al `else`; los handlers lo saltan.
                    self.conectar(&normales_body, e, "seq");
                    normales = normales_handlers;
                } else {
                    normales = normales_body;
                    normales.extend(normales_handlers);
                }
                sal.absorber_no_locales(&mut sal_else);
                normales.extend(sal_else.normal);

                let (ent_fin, mut sal_fin) = self.suite(&s.finalbody);
                if let Some(e) = ent_fin {
                    // `finally` corre por TODOS los caminos, incluidos los que
                    // salen por `return`/`raise`.
                    self.conectar(&normales.clone(), e, "finally");
                    self.conectar(&sal.returns.clone(), e, "finally");
                    normales.clear();
                }
                sal.absorber_no_locales(&mut sal_fin);
                normales.extend(sal_fin.normal);
                sal.normal = normales;
                (Some(n), sal)
            }
            Stmt::Match(s) => {
                let n = self.nodo(stmt, "match");
                let mut hubo_comodin = false;
                for caso in &s.cases {
                    let (ent, mut sal_c) = self.suite(&caso.body);
                    if let Some(e) = ent {
                        self.arista(n, e, "case");
                    }
                    sal.absorber_no_locales(&mut sal_c);
                    sal.normal.extend(sal_c.normal);
                    if caso.guard.is_none() && es_patron_comodin(&caso.pattern) {
                        hubo_comodin = true;
                    }
                }
                // Sin un `case _:` sin guarda, el `match` puede no coincidir
                // con nada y el flujo sigue de largo.
                if !hubo_comodin {
                    sal.normal.push(n);
                }
                (Some(n), sal)
            }
            // Las definiciones anidadas son su propio scope: se registran
            // como una sentencia del flujo (definir la función SÍ ocurre acá)
            // pero su cuerpo no es flujo de esta función.
            Stmt::FunctionDef(_) | Stmt::AsyncFunctionDef(_) => self.simple(stmt, "def"),
            Stmt::ClassDef(_) => self.simple(stmt, "class"),
            Stmt::Assign(_) | Stmt::AugAssign(_) | Stmt::AnnAssign(_) => self.simple(stmt, "assign"),
            Stmt::Expr(_) => self.simple(stmt, "expr"),
            Stmt::Assert(_) => self.simple(stmt, "assert"),
            Stmt::Delete(_) => self.simple(stmt, "delete"),
            _ => self.simple(stmt, "stmt"),
        }
    }

    fn simple(&mut self, stmt: &Stmt, kind: &'static str) -> (Option<usize>, Salidas) {
        let n = self.nodo(stmt, kind);
        let mut sal = Salidas::default();
        sal.normal.push(n);
        (Some(n), sal)
    }

    fn bloque_simple(&mut self, stmt: &Stmt, kind: &'static str, body: &[Stmt]) -> (Option<usize>, Salidas) {
        let n = self.nodo(stmt, kind);
        let (ent, mut sal_body) = self.suite(body);
        if let Some(e) = ent {
            self.arista(n, e, "seq");
        }
        let mut sal = Salidas::default();
        sal.absorber_no_locales(&mut sal_body);
        sal.normal = if ent.is_none() { vec![n] } else { sal_body.normal };
        (Some(n), sal)
    }

    fn bucle_for(&mut self, stmt: &Stmt, body: &[Stmt], orelse: &[Stmt]) -> (Option<usize>, Salidas) {
        let n = self.nodo(stmt, "for");
        let (ent, mut sal_cuerpo) = self.suite(body);
        if let Some(e) = ent {
            self.arista(n, e, "iter");
        }
        self.conectar(&sal_cuerpo.normal, n, "loop");
        self.conectar(&sal_cuerpo.continues, n, "continue");
        sal_cuerpo.continues.clear();

        let (ent_else, mut sal_else) = self.suite(orelse);
        if let Some(e) = ent_else {
            self.arista(n, e, "exhausted");
        }
        let mut sal = Salidas::default();
        sal.absorber_no_locales(&mut sal_cuerpo);
        sal.absorber_no_locales(&mut sal_else);
        // Un `for` sobre un iterable puede no iterar nunca: siempre hay
        // salida normal, a diferencia de `while True`.
        if ent_else.is_none() {
            sal.normal.push(n);
        }
        sal.normal.extend(sal_else.normal);
        sal.normal.extend(std::mem::take(&mut sal.breaks));
        (Some(n), sal)
    }
}

/// `while True:` / `while 1:` — las dos formas que un lector reconoce como
/// bucle infinito deliberado. NO evalúa expresiones (`while x or True`): eso
/// es interpretación abstracta, y afirmarlo desde acá sería inventar.
fn es_condicion_siempre_verdadera(test: &Expr) -> bool {
    match test {
        Expr::Constant(c) => matches!(&c.value, Constant::Bool(true) | Constant::Int(_)),
        _ => false,
    }
}

fn es_patron_comodin(patron: &rustpython_parser::ast::Pattern) -> bool {
    matches!(patron, rustpython_parser::ast::Pattern::MatchAs(p) if p.pattern.is_none())
}

pub fn build(function: &str, body: &[Stmt], line_index: &LineIndex) -> FunctionCfg {
    let mut c = Constructor { nodes: Vec::new(), edges: Vec::new(), vars: Vec::new(), line_index };
    let (entrada, _) = c.suite(body);

    // Alcanzabilidad: BFS desde la entrada. Lo que no se toca, no se ejecuta.
    let mut alcanzados: HashSet<usize> = HashSet::new();
    if let Some(e) = entrada {
        let mut cola = VecDeque::from([e]);
        alcanzados.insert(e);
        while let Some(actual) = cola.pop_front() {
            for edge in c.edges.iter().filter(|x| x.from == actual) {
                if alcanzados.insert(edge.to) {
                    cola.push_back(edge.to);
                }
            }
        }
    }
    let unreachable: Vec<usize> = c.nodes.iter().map(|n| n.id).filter(|id| !alcanzados.contains(id)).collect();

    FunctionCfg {
        function: function.to_string(),
        nodes: c.nodes,
        edges: c.edges,
        vars: c.vars,
        entry: entrada,
        unreachable,
    }
}

/// `true` si el flujo puede llegar al FINAL del cuerpo sin haber pasado por
/// un `return`/`raise` — o sea, si la función puede caerse por abajo y
/// devolver `None` implícitamente.
///
/// Existe para `intervals.rs`, que necesita saber qué funciones del archivo
/// pueden devolver `None` para poder juzgar a sus llamadores. **Es el mismo
/// recorrido que ya construye el grafo**, no un cuarto analizador de
/// terminación: la respuesta es exactamente "¿el `suite` tiene alguna salida
/// normal?", que el constructor ya calcula y `build` descarta.
///
/// Un cuerpo vacío cae por abajo por definición.
pub fn cae_por_abajo(body: &[Stmt], line_index: &LineIndex) -> bool {
    let mut c = Constructor { nodes: Vec::new(), edges: Vec::new(), vars: Vec::new(), line_index };
    let (entrada, salidas) = c.suite(body);
    entrada.is_none() || !salidas.normal.is_empty()
}

/// Código inalcanzable, como `StructuralSmell` para no inventar plomería
/// nueva: el shape ya lo agregan `static_analysis.py` y el panel.
///
/// Solo se reporta la PRIMERA sentencia inalcanzable de cada tramo contiguo.
/// Diez líneas muertas después de un `return` son un solo problema, no diez
/// hallazgos, y listarlas todas ahogaría al resto del informe.
pub fn unreachable_code_smells(cfg: &FunctionCfg) -> Vec<StructuralSmell> {
    let mut ids: Vec<usize> = cfg.unreachable.clone();
    ids.sort_unstable();
    let mut out = Vec::new();
    let mut anterior: Option<usize> = None;
    for id in ids {
        if anterior == Some(id.wrapping_sub(1)) {
            anterior = Some(id);
            continue;
        }
        anterior = Some(id);
        let Some(nodo) = cfg.nodes.get(id) else { continue };
        out.push(StructuralSmell {
            kind: "unreachable_code",
            name: cfg.function.clone(),
            line: nodo.line,
            message: format!(
                "no path reaches this `{}` from the function entry — the control-flow graph has no edge into it (dead after a return/raise/break/continue, or after a loop with no exit)",
                nodo.kind
            ),
        });
    }
    out.sort_by_key(|s| s.line);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::parse_module;

    #[test]
    fn cae_por_abajo_distingue_las_cuatro_formas_de_terminar() {
        let casos: &[(&str, bool, &str)] = &[
            ("def f():\n    return 1\n", false, "un return al final no cae"),
            ("def f():\n    x = 1\n", true, "sin return, cae"),
            (
                "def f(c):\n    if c:\n        return 1\n",
                true,
                "un if sin else cae por la rama que falta",
            ),
            (
                "def f(c):\n    if c:\n        return 1\n    else:\n        return 2\n",
                false,
                "las dos ramas vuelven: no cae",
            ),
            (
                "def f():\n    for x in [1]:\n        return x\n",
                true,
                "el for puede no correr nunca: cae",
            ),
            ("def f():\n    raise ValueError()\n", false, "un raise tampoco cae"),
            (
                "def f():\n    while True:\n        pass\n",
                false,
                "un while True sin break no llega nunca al final",
            ),
        ];
        for (src, esperado, por_que) in casos {
            let suite = parse_module(src).expect("parse error");
            let idx = LineIndex::build(src);
            let Stmt::FunctionDef(f) = &suite[0] else { panic!("esperaba una función") };
            assert_eq!(cae_por_abajo(&f.body, &idx), *esperado, "{por_que}");
        }
    }


    fn cfg_de(src: &str) -> FunctionCfg {
        let suite = parse_module(src).expect("parse error");
        let idx = LineIndex::build(src);
        // El cuerpo de la primera función del archivo.
        let body = match &suite[0] {
            Stmt::FunctionDef(f) => f.body.clone(),
            _ => suite.clone(),
        };
        build("f", &body, &idx)
    }

    fn lineas_muertas(src: &str) -> Vec<usize> {
        unreachable_code_smells(&cfg_de(src)).into_iter().map(|s| s.line).collect()
    }

    #[test]
    fn una_funcion_lineal_no_tiene_nada_inalcanzable() {
        let src = "def f():\n    a = 1\n    b = 2\n    return a + b\n";
        assert!(cfg_de(src).unreachable.is_empty());
    }

    #[test]
    fn codigo_despues_de_un_return_es_inalcanzable() {
        let src = "def f():\n    return 1\n    x = 2\n";
        assert_eq!(lineas_muertas(src), vec![3]);
    }

    #[test]
    fn un_return_dentro_de_un_if_no_mata_lo_que_sigue() {
        // El caso que separa un CFG de un heuristico de "hubo un return":
        // la rama falsa del `if` sigue de largo, asi que la linea 4 SI se
        // alcanza. Un chequeo textual la marcaria como muerta.
        let src = "def f(x):\n    if x:\n        return 1\n    y = 2\n    return y\n";
        assert!(lineas_muertas(src).is_empty(), "{:?}", lineas_muertas(src));
    }

    #[test]
    fn codigo_despues_de_un_while_true_sin_break_es_inalcanzable() {
        // Ninguna heuristica de texto ve esto: no hay `return` de por medio.
        let src = "def f():\n    while True:\n        trabajar()\n    limpiar()\n";
        assert_eq!(lineas_muertas(src), vec![4]);
    }

    #[test]
    fn un_while_true_con_break_si_deja_pasar_el_flujo() {
        let src = "def f():\n    while True:\n        if listo():\n            break\n    limpiar()\n";
        assert!(lineas_muertas(src).is_empty(), "{:?}", lineas_muertas(src));
    }

    #[test]
    fn codigo_despues_de_un_break_es_inalcanzable() {
        let src = "def f(xs):\n    for x in xs:\n        break\n        y = 1\n";
        assert_eq!(lineas_muertas(src), vec![4]);
    }

    #[test]
    fn un_tramo_contiguo_es_un_solo_hallazgo() {
        // 3 lineas muertas seguidas son UN problema, no 3.
        let src = "def f():\n    return 1\n    a = 1\n    b = 2\n    c = 3\n";
        assert_eq!(lineas_muertas(src), vec![3]);
    }

    #[test]
    fn el_grafo_tiene_una_arista_de_retroceso_en_un_while() {
        let src = "def f(n):\n    while n:\n        n = n - 1\n";
        let cfg = cfg_de(src);
        assert!(cfg.edges.iter().any(|e| e.label == "loop"), "sin arista de ciclo: {:?}", cfg.edges.len());
    }

    #[test]
    fn el_if_produce_las_dos_ramas_etiquetadas() {
        let src = "def f(x):\n    if x:\n        a = 1\n    else:\n        a = 2\n    return a\n";
        let cfg = cfg_de(src);
        assert!(cfg.edges.iter().any(|e| e.label == "true"));
        assert!(cfg.edges.iter().any(|e| e.label == "false"));
    }

    #[test]
    fn el_finally_se_alcanza_incluso_saliendo_por_return() {
        // `finally` corre por todos los caminos, tambien por el que retorna:
        // si no se modela esa arista, el cuerpo del finally parece muerto.
        let src = "def f():\n    try:\n        return 1\n    finally:\n        cerrar()\n";
        assert!(lineas_muertas(src).is_empty(), "{:?}", lineas_muertas(src));
    }

    #[test]
    fn el_cuerpo_de_un_except_se_alcanza() {
        let src = "def f():\n    try:\n        riesgo()\n    except ValueError:\n        manejar()\n    return 1\n";
        assert!(lineas_muertas(src).is_empty(), "{:?}", lineas_muertas(src));
    }

    #[test]
    fn una_funcion_anidada_no_aporta_flujo_a_la_de_afuera() {
        // Definirla es una sentencia del flujo; su CUERPO es otro scope.
        let src = "def f():\n    def g():\n        return 1\n    return 2\n";
        assert!(lineas_muertas(src).is_empty());
        let cfg = cfg_de(src);
        assert!(cfg.nodes.iter().any(|n| n.kind == "def"));
        // El `return 1` de adentro no es un nodo de este grafo.
        assert_eq!(cfg.nodes.iter().filter(|n| n.kind == "return").count(), 1);
    }

    #[test]
    fn un_match_sin_comodin_deja_pasar_el_flujo() {
        let src = "def f(x):\n    match x:\n        case 1:\n            a = 1\n    return 2\n";
        assert!(lineas_muertas(src).is_empty(), "{:?}", lineas_muertas(src));
    }
    /// **El `else` de un `try` corre solo si NO hubo excepcion**, asi que no
    /// puede haber una arista desde un handler hacia el. Mezclarlas hacia
    /// creer que lo que el handler asigna lo pisa el `else`.
    #[test]
    fn el_else_de_un_try_no_se_alcanza_desde_un_handler() {
        let cfg = cfg_de("try:
    a()
except E:
    b()
else:
    c()
");
        let handler = cfg.nodes.iter().find(|x| x.line == 4).expect("nodo del handler");
        let rama_else = cfg.nodes.iter().find(|x| x.line == 6).expect("nodo del else");
        assert!(
            !cfg.edges.iter().any(|e| e.from == handler.id && e.to == rama_else.id),
            "hay arista handler -> else: {:?}",
            cfg.edges.iter().map(|e| (e.from, e.to, e.label)).collect::<Vec<_>>()
        );
        // Y el `else` sigue siendo alcanzable desde el cuerpo.
        assert!(!cfg.unreachable.contains(&rama_else.id), "el else quedo inalcanzable");
    }

}
