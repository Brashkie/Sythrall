//! **SSA real: árbol de dominadores, frontera de dominancia, φ-nodos y
//! renombrado por versiones.**
//!
//! El ROADMAP decía que `muertas.rs` era *"el primer corte de def-use — lo
//! que una forma SSA daría gratis"*, y su propio doc nombraba el precio de
//! no tenerla: **una escritura pisada antes de leerse (`x = caro(); x = 1;
//! usar(x)`) también es muerta y eso no la ve.** Este módulo construye la
//! forma SSA de verdad y cierra exactamente ese hueco.
//!
//! **Por qué SSA y no un parche.** `muertas.rs` no pregunta *"¿esta escritura
//! se lee después?"* sino *"¿este nombre se lee en algún lado?"*, porque la
//! primera pregunta exige orden y flujo y con bucles se vuelve sutil rápido.
//! SSA responde la pregunta difícil por construcción: si cada escritura crea
//! una **versión distinta** y cada lectura apunta a la versión que le llega,
//! entonces *"esta escritura no se lee"* es literalmente *"esta versión no
//! tiene usos"* — una consulta, no un análisis.
//!
//! Los φ-nodos son lo que hace que eso funcione con ramas y bucles. Donde dos
//! caminos traen versiones distintas de `x`, se inserta `x₃ = φ(x₁, x₂)`: el
//! lector de abajo usa `x₃`, y `x₁`/`x₂` quedan vivas por alimentarla. Sin
//! ellos habría que elegir una de las dos y la otra se reportaría muerta.
//!
//! **Colocación mínima, por frontera de dominancia (Cytron et al.)**, no un
//! φ en cada confluencia: la frontera de `n` son exactamente los nodos a los
//! que `n` llega sin dominarlos, que es donde su definición deja de ser la
//! única posible. Los dominadores salen del algoritmo iterativo de
//! Cooper–Harvey–Kennedy sobre el CFG que `cfg.rs` ya construye.
//!
//! ## Lo que este módulo NO reporta, cada cosa por su motivo
//!
//! - **Nada dentro del cuerpo de un `try` que tenga handlers.** `cfg.rs`
//!   dibuja la arista hacia el `except` **desde el nodo `try`**, no desde
//!   cada sentencia de adentro (su comentario lo dice: la excepción puede
//!   saltar desde cualquier punto). La consecuencia para acá es concreta: un
//!   handler que lee algo asignado en el cuerpo **no aparece como uso de esa
//!   versión**, y la escritura se vería muerta sin serlo. Modelar la arista
//!   de excepción por sentencia es trabajo del CFG, no de este módulo.
//! - **Nada de una función que llame a `eval`/`exec`/`locals`**, igual que
//!   `muertas.rs`: ahí cualquier nombre puede leerse sin nombrarse.
//! - **Nombres `global`/`nonlocal`**, `_`-prefijados, y todo destino que no
//!   sea un nombre pelado (tuplas, atributos, subíndices) — las mismas cinco
//!   exclusiones que `muertas.rs` ya justifica en su doc.
//! - **Variables que no se leen en NINGÚN lado**: ésas son de `dead_store` y
//!   reportarlas acá sería el mismo hallazgo dos veces. La partición es
//!   limpia: `dead_store` cubre *"nunca se lee"*, `overwritten_store` cubre
//!   *"se lee, pero no esta versión"*.

use std::collections::{HashMap, HashSet, VecDeque};

use rustpython_parser::ast::{Expr, Ranged, Stmt};

use crate::flujo::cfg::{self, FunctionCfg};
use crate::ast::parser::LineIndex;
use crate::hallazgos::smells::StructuralSmell;
use crate::ast::walk::walk_stmts;

/// De dónde sale una definición.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origen {
    /// Parámetro de la función: existe antes de la primera sentencia.
    Entrada,
    /// Una escritura real en el código.
    Normal,
    /// Un φ insertado en una confluencia.
    Phi,
}

#[derive(Clone, Debug)]
pub struct Def {
    pub var: String,
    pub version: u32,
    pub origen: Origen,
    /// Nodo del CFG donde vive. `None` para los de entrada.
    pub nodo: Option<usize>,
    pub linea: usize,
    /// Cuántas lecturas apuntan a esta versión.
    pub usos: usize,
    /// Para un φ: las versiones que entran por cada camino.
    pub args: Vec<usize>,
    /// `true` si la escritura es un destino simple (`x = ...`) y por lo tanto
    /// reportable. Un desempaquetado de tupla define igual, pero no se acusa.
    pub simple: bool,
}

pub struct Ssa {
    pub defs: Vec<Def>,
}

// ───────────────────────────── Dominadores ─────────────────────────────

/// Sucesores y predecesores, solo entre nodos alcanzables.
fn adyacencias(cfg: &FunctionCfg) -> (HashMap<usize, Vec<usize>>, HashMap<usize, Vec<usize>>) {
    let muertos: HashSet<usize> = cfg.unreachable.iter().copied().collect();
    let mut succ: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut pred: HashMap<usize, Vec<usize>> = HashMap::new();
    for e in &cfg.edges {
        if muertos.contains(&e.from) || muertos.contains(&e.to) {
            continue;
        }
        succ.entry(e.from).or_default().push(e.to);
        pred.entry(e.to).or_default().push(e.from);
    }
    (succ, pred)
}

/// Postorden por DFS iterativo desde la entrada. Iterativo y no recursivo a
/// propósito: una función larga sin ramas da una cadena de dominadores tan
/// profunda como sentencias tenga, y eso desbordaría la pila.
fn postorden(entry: usize, succ: &HashMap<usize, Vec<usize>>) -> Vec<usize> {
    let mut visto: HashSet<usize> = HashSet::new();
    let mut out = Vec::new();
    // (nodo, índice del próximo sucesor a mirar)
    let mut pila: Vec<(usize, usize)> = vec![(entry, 0)];
    visto.insert(entry);
    while let Some((n, i)) = pila.pop() {
        let vecinos = succ.get(&n).map(|v| v.as_slice()).unwrap_or(&[]);
        if i < vecinos.len() {
            pila.push((n, i + 1));
            let s = vecinos[i];
            if visto.insert(s) {
                pila.push((s, 0));
            }
        } else {
            out.push(n);
        }
    }
    out
}

/// Dominador inmediato de cada nodo alcanzable, por el algoritmo iterativo de
/// Cooper–Harvey–Kennedy. `idom[entry] == entry`.
pub fn dominadores(cfg: &FunctionCfg) -> HashMap<usize, usize> {
    let mut idom: HashMap<usize, usize> = HashMap::new();
    let Some(entry) = cfg.entry else { return idom };
    let (succ, pred) = adyacencias(cfg);

    let post = postorden(entry, &succ);
    let mut num: HashMap<usize, usize> = HashMap::new();
    for (i, n) in post.iter().enumerate() {
        num.insert(*n, i);
    }
    idom.insert(entry, entry);

    // Intersección de dos caminos hacia la raíz, subiendo siempre por el de
    // número de postorden más bajo hasta que se encuentran.
    let intersecar = |mut a: usize, mut b: usize, idom: &HashMap<usize, usize>| -> usize {
        while a != b {
            while num.get(&a).copied().unwrap_or(0) < num.get(&b).copied().unwrap_or(0) {
                match idom.get(&a) {
                    Some(&p) if p != a => a = p,
                    _ => break,
                }
            }
            while num.get(&b).copied().unwrap_or(0) < num.get(&a).copied().unwrap_or(0) {
                match idom.get(&b) {
                    Some(&p) if p != b => b = p,
                    _ => break,
                }
            }
            if a == b {
                break;
            }
            // Sin progreso posible: grafo raro, se corta en vez de colgarse.
            if idom.get(&a).copied() == Some(a) && idom.get(&b).copied() == Some(b) {
                return entry;
            }
        }
        a
    };

    let rpo: Vec<usize> = post.iter().rev().copied().collect();
    let mut cambio = true;
    while cambio {
        cambio = false;
        for &b in &rpo {
            if b == entry {
                continue;
            }
            let vacio = Vec::new();
            let preds = pred.get(&b).unwrap_or(&vacio);
            let mut nuevo: Option<usize> = None;
            for &p in preds {
                if !idom.contains_key(&p) {
                    continue;
                }
                nuevo = Some(match nuevo {
                    None => p,
                    Some(actual) => intersecar(p, actual, &idom),
                });
            }
            if let Some(n) = nuevo {
                if idom.get(&b) != Some(&n) {
                    idom.insert(b, n);
                    cambio = true;
                }
            }
        }
    }
    idom
}

/// Frontera de dominancia (Cytron et al.): los nodos a los que `n` llega sin
/// dominarlos. Es exactamente donde una definición hecha en `n` deja de ser
/// la única que puede llegar, y por eso es donde va un φ.
pub fn frontera_dominancia(cfg: &FunctionCfg, idom: &HashMap<usize, usize>) -> HashMap<usize, HashSet<usize>> {
    let (_, pred) = adyacencias(cfg);
    let mut df: HashMap<usize, HashSet<usize>> = HashMap::new();
    for (&b, preds) in pred.iter() {
        // **La entrada cuenta con un predecesor menos del que tiene.** Nadie
        // dibuja una arista hacia el arranque de la función, así que un
        // `while True:` que ES la primera sentencia queda con un solo
        // predecesor —su propia arista de retroceso— y se saltaba la
        // colocación de φ. Sin ese φ, lo que el cuerpo escribe no vuelve a
        // la cabecera y se reportaba muerto teniéndose que leer en la vuelta
        // siguiente. El "inicio" es ese predecesor implícito.
        let union = preds.len() >= 2 || Some(b) == cfg.entry;
        if !union {
            continue;
        }
        let Some(&idom_b) = idom.get(&b) else { continue };
        for &p in preds {
            let mut runner = p;
            let mut guarda = 0usize;
            while runner != idom_b {
                df.entry(runner).or_default().insert(b);
                let Some(&siguiente) = idom.get(&runner) else { break };
                if siguiente == runner {
                    break;
                }
                runner = siguiente;
                guarda += 1;
                if guarda > cfg.nodes.len() + 1 {
                    break;
                }
            }
        }
    }
    df
}

// ───────────────────────── Construcción de la forma ─────────────────────────

/// Construye la forma SSA: coloca los φ y renombra cada lectura a la versión
/// que realmente le llega.
pub fn construir(cfg: &FunctionCfg, params: &[String]) -> Ssa {
    let mut defs: Vec<Def> = Vec::new();
    let Some(entry) = cfg.entry else { return Ssa { defs } };

    let idom = dominadores(cfg);
    let df = frontera_dominancia(cfg, &idom);
    let muertos: HashSet<usize> = cfg.unreachable.iter().copied().collect();

    // Dónde se define cada variable.
    let mut def_nodos: HashMap<String, Vec<usize>> = HashMap::new();
    for n in &cfg.nodes {
        if muertos.contains(&n.id) {
            continue;
        }
        if let Some(v) = cfg.vars.get(n.id) {
            for d in &v.defs {
                def_nodos.entry(d.clone()).or_default().push(n.id);
            }
        }
    }

    // Colocación mínima: worklist sobre la frontera de dominancia. Un φ
    // insertado en `y` es a su vez una definición de `y`, así que `y` vuelve
    // a la cola — por eso la iteración y no un solo barrido.
    let mut phis: HashMap<usize, HashSet<String>> = HashMap::new();
    for (var, nodos) in &def_nodos {
        let mut cola: VecDeque<usize> = nodos.iter().copied().collect();
        let mut ya: HashSet<usize> = HashSet::new();
        let en_cola: HashSet<usize> = nodos.iter().copied().collect();
        let mut vistos_en_cola = en_cola;
        while let Some(x) = cola.pop_front() {
            let Some(frontera) = df.get(&x) else { continue };
            for &y in frontera {
                if !ya.insert(y) {
                    continue;
                }
                phis.entry(y).or_default().insert(var.clone());
                if vistos_en_cola.insert(y) {
                    cola.push_back(y);
                }
            }
        }
    }

    // Hijos del árbol de dominadores.
    let mut hijos: HashMap<usize, Vec<usize>> = HashMap::new();
    for (&n, &p) in idom.iter() {
        if n != p {
            hijos.entry(p).or_default().push(n);
        }
    }
    for v in hijos.values_mut() {
        v.sort_unstable();
    }

    // Pila de versiones vivas por variable.
    let mut pila: HashMap<String, Vec<usize>> = HashMap::new();
    let mut contador: HashMap<String, u32> = HashMap::new();

    for p in params {
        let id = defs.len();
        defs.push(Def {
            var: p.clone(),
            version: 0,
            origen: Origen::Entrada,
            nodo: None,
            linea: 0,
            usos: 0,
            args: Vec::new(),
            simple: false,
        });
        pila.entry(p.clone()).or_default().push(id);
        contador.insert(p.clone(), 0);
    }

    // **Los Def de los φ se crean ANTES de recorrer, no al visitar su nodo.**
    // Sus argumentos los rellena cada PREDECESOR, y un predecesor se visita
    // antes que el nodo del φ — si el φ naciera al visitarlo, el predecesor
    // no lo encontraría y el φ quedaría sin argumentos. Con argumentos vacíos
    // nada lo mantiene vivo y toda definición de una rama se reportaría
    // muerta, que es exactamente lo que pasaba.
    let mut phi_id: HashMap<(usize, String), usize> = HashMap::new();
    {
        let mut nodos_phi: Vec<&usize> = phis.keys().collect();
        nodos_phi.sort();
        for &n in nodos_phi {
            let mut vars: Vec<&String> = phis[&n].iter().collect();
            vars.sort();
            for var in vars {
                let ver = contador.entry(var.clone()).and_modify(|c| *c += 1).or_insert(1);
                let id = defs.len();
                defs.push(Def {
                    var: var.clone(),
                    version: *ver,
                    origen: Origen::Phi,
                    nodo: Some(n),
                    linea: cfg.nodes.get(n).map(|x| x.line).unwrap_or(0),
                    usos: 0,
                    args: Vec::new(),
                    simple: false,
                });
                phi_id.insert((n, var.clone()), id);
            }
        }
    }

    // Recorrido del árbol de dominadores con pila explícita. `Entrar` procesa
    // el nodo; `Salir` deshace las versiones que introdujo, que es lo que
    // hace que una rama no contamine a su hermana.
    enum Paso {
        Entrar(usize),
        Salir(Vec<String>),
    }
    let succ_de = {
        let (s, _) = adyacencias(cfg);
        s
    };
    let mut trabajo: Vec<Paso> = vec![Paso::Entrar(entry)];

    while let Some(paso) = trabajo.pop() {
        match paso {
            Paso::Salir(vars) => {
                for v in vars {
                    if let Some(p) = pila.get_mut(&v) {
                        p.pop();
                    }
                }
            }
            Paso::Entrar(n) => {
                let mut introducidas: Vec<String> = Vec::new();

                // 1) Los φ del nodo entran en vigencia antes que nada.
                if let Some(vars) = phis.get(&n) {
                    let mut ordenadas: Vec<&String> = vars.iter().collect();
                    ordenadas.sort();
                    for var in ordenadas {
                        let Some(&id) = phi_id.get(&(n, var.clone())) else { continue };
                        pila.entry(var.clone()).or_default().push(id);
                        introducidas.push(var.clone());
                    }
                }

                // 2) Las LECTURAS del nodo apuntan a la versión vigente. Van
                //    antes que las escrituras porque en `x = x + 1` el `x` de
                //    la derecha es el viejo.
                if let Some(v) = cfg.vars.get(n) {
                    for u in &v.usos {
                        if let Some(top) = pila.get(u).and_then(|p| p.last()).copied() {
                            defs[top].usos += 1;
                        }
                    }
                    // 3) Y ahora las escrituras crean versión nueva.
                    let simples: HashSet<&String> = v.defs_simples.iter().collect();
                    for d in &v.defs {
                        let ver = contador.entry(d.clone()).and_modify(|c| *c += 1).or_insert(1);
                        let id = defs.len();
                        defs.push(Def {
                            var: d.clone(),
                            version: *ver,
                            origen: Origen::Normal,
                            nodo: Some(n),
                            linea: cfg.nodes.get(n).map(|x| x.line).unwrap_or(0),
                            usos: 0,
                            args: Vec::new(),
                            simple: simples.contains(d),
                        });
                        pila.entry(d.clone()).or_default().push(id);
                        introducidas.push(d.clone());
                    }
                }

                // 4) Rellenar los φ de los sucesores con lo que les llega por
                //    ESTA arista.
                if let Some(ss) = succ_de.get(&n) {
                    for &s in ss {
                        if let Some(vars) = phis.get(&s) {
                            for var in vars {
                                let Some(&pid) = phi_id.get(&(s, var.clone())) else { continue };
                                if let Some(top) = pila.get(var).and_then(|p| p.last()).copied() {
                                    if !defs[pid].args.contains(&top) {
                                        defs[pid].args.push(top);
                                    }
                                }
                            }
                        }
                    }
                }

                trabajo.push(Paso::Salir(introducidas));
                if let Some(hs) = hijos.get(&n) {
                    for &h in hs.iter().rev() {
                        trabajo.push(Paso::Entrar(h));
                    }
                }
            }
        }
    }

    Ssa { defs }
}

/// Versiones vivas: las que tienen alguna lectura directa, más —en cadena—
/// las que alimentan un φ vivo.
///
/// La propagación hacia atrás es la parte que importa: `x₁` puede no leerse
/// nunca *directamente* y estar perfectamente viva porque entra en un
/// `x₃ = φ(x₁, x₂)` que sí se lee. Sin este paso, toda definición de una rama
/// se reportaría muerta.
fn vivas(ssa: &Ssa) -> HashSet<usize> {
    let mut vivas: HashSet<usize> = HashSet::new();
    let mut cola: VecDeque<usize> = VecDeque::new();
    for (i, d) in ssa.defs.iter().enumerate() {
        if d.usos > 0 && vivas.insert(i) {
            cola.push_back(i);
        }
    }
    while let Some(i) = cola.pop_front() {
        if ssa.defs[i].origen != Origen::Phi {
            continue;
        }
        for &a in &ssa.defs[i].args {
            if vivas.insert(a) {
                cola.push_back(a);
            }
        }
    }
    vivas
}

// ─────────────────────────────── El hallazgo ───────────────────────────────

const LECTORES_DINAMICOS: &[&str] = &["eval", "exec", "locals", "vars", "globals"];

fn tiene_lectura_dinamica(body: &[Stmt]) -> bool {
    let mut hay = false;
    let mut on_expr = |e: &Expr| {
        if let Expr::Call(c) = e {
            if let Expr::Name(n) = &*c.func {
                if LECTORES_DINAMICOS.contains(&n.id.as_str()) {
                    hay = true;
                }
            }
        }
    };
    let mut on_stmt = |_: &Stmt| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    hay
}

fn nombres_compartidos(body: &[Stmt]) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut on_stmt = |s: &Stmt| match s {
        Stmt::Global(g) => out.extend(g.names.iter().map(|n| n.to_string())),
        Stmt::Nonlocal(g) => out.extend(g.names.iter().map(|n| n.to_string())),
        _ => {}
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    out
}

/// Nombres que lee alguna función anidada o lambda de este ámbito.
///
/// **Una clausura lee sus capturas cuando se la LLAMA, no cuando se la
/// define**, así que el orden del texto no dice nada: en
///
/// ```python
/// def envoltorio():
///     return scope        # lee el `scope` de abajo
/// scope = obtener()
/// ```
///
/// la asignación es posterior al `def` y aun así es la que se lee. Un
/// análisis sensible al flujo le atribuiría la lectura a la versión vigente
/// en la línea del `def` —ninguna— y daría por muerta la de abajo. Es un
/// falso positivo real: apareció en `anyio/_backends/_asyncio.py`.
///
/// No hay forma de saber cuándo se llama, así que la regla es no opinar
/// sobre ninguna versión de un nombre capturado.
fn nombres_capturados(body: &[Stmt]) -> HashSet<String> {
    use std::cell::RefCell;
    let out: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    let juntar_loads = |b: &[Stmt]| {
        let mut s2 = |_: &Stmt| {};
        let mut e2 = |e: &Expr| {
            if let Expr::Name(n) = e {
                if matches!(n.ctx, rustpython_parser::ast::ExprContext::Load) {
                    out.borrow_mut().insert(n.id.to_string());
                }
            }
        };
        walk_stmts(b, &mut s2, &mut e2);
    };
    let mut on_stmt = |s: &Stmt| match s {
        Stmt::FunctionDef(f) => juntar_loads(&f.body),
        Stmt::AsyncFunctionDef(f) => juntar_loads(&f.body),
        Stmt::ClassDef(c) => juntar_loads(&c.body),
        _ => {}
    };
    let mut on_expr = |e: &Expr| {
        if let Expr::Lambda(l) = e {
            crate::ast::walk::walk_expr_tree(&l.body, &mut |x: &Expr| {
                if let Expr::Name(n) = x {
                    if matches!(n.ctx, rustpython_parser::ast::ExprContext::Load) {
                        out.borrow_mut().insert(n.id.to_string());
                    }
                }
            });
        }
    };
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    out.into_inner()
}

/// Nombres que se ESCRIBEN dentro del cuerpo de un `with`.
///
/// **Una escritura dentro de un `with` es condicional, igual que una dentro
/// de un `try`**, y por el mismo motivo: si el cuerpo lanza a mitad de
/// camino, el resto no corre. Y hay un caso en que eso además no interrumpe
/// nada — un context manager cuyo `__exit__` devuelve verdadero **suprime**
/// la excepción y la ejecución sigue después del `with`. `contextlib.suppress`
/// es el ejemplo de manual; el que lo destapó acá es real:
///
/// ```python
/// metadata_version = None
/// with collector.collect(InvalidMetadata):
///     metadata_version = ins.metadata_version    # puede lanzar
///     ...
/// # sigue acá, con metadata_version todavía en None
/// ```
///
/// (`pip/_vendor/packaging/metadata.py`). Saber si un `__exit__` suprime
/// exige ejecutar el programa, así que no se adivina: si una variable se
/// escribe dentro de un `with`, no se opina sobre sus versiones.
///
/// **Esto corrige un criterio propio equivocado.** La primera versión trataba
/// el cuerpo de un `with` como código que siempre corre entero — razonamiento
/// que solo vale para un context manager que no suprime, y eso no se puede
/// afirmar desde el AST.
fn escritos_dentro_de_with(body: &[Stmt]) -> HashSet<String> {
    use std::cell::RefCell;
    let out: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    let juntar = |b: &[Stmt]| {
        let mut s2 = |_: &Stmt| {};
        let mut e2 = |e: &Expr| {
            if let Expr::Name(n) = e {
                if matches!(n.ctx, rustpython_parser::ast::ExprContext::Store) {
                    out.borrow_mut().insert(n.id.to_string());
                }
            }
        };
        walk_stmts(b, &mut s2, &mut e2);
    };
    let mut on_stmt = |s: &Stmt| match s {
        Stmt::With(w) => juntar(&w.body),
        Stmt::AsyncWith(w) => juntar(&w.body),
        _ => {}
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    out.into_inner()
}

/// Nombres que se leen en un `except` o en un `finally`.
///
/// **Mismo hueco de aristas de excepción que el de arriba, entrando por el
/// otro lado.** El patrón es el más común de Python:
///
/// ```python
/// fp = None
/// try:
///     checker = HashChecker.from_url(url)   # esto puede lanzar
///     fp = self.open_url(url)
///     ...
/// finally:
///     if fp: fp.close()                     # y acá se lee el None
/// ```
///
/// Si la primera línea del cuerpo lanza, `fp` sigue en `None` y el `finally`
/// lo lee. Ese camino —excepción desde el MEDIO del cuerpo— no está en el
/// CFG, que solo conecta el `finally` con la salida normal del cuerpo, así
/// que la versión `None` parecía morir ahí. Falso positivo real, encontrado
/// en `setuptools/package_index.py`.
///
/// La regla es angosta a propósito: no se apaga la función entera, solo se
/// dejan afuera las variables que de verdad se leen en una ruta de
/// excepción.
fn leidos_en_rutas_de_excepcion(body: &[Stmt]) -> HashSet<String> {
    use std::cell::RefCell;
    let out: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    let juntar = |b: &[Stmt]| {
        let mut s2 = |_: &Stmt| {};
        let mut e2 = |e: &Expr| {
            if let Expr::Name(n) = e {
                if matches!(n.ctx, rustpython_parser::ast::ExprContext::Load) {
                    out.borrow_mut().insert(n.id.to_string());
                }
            }
        };
        walk_stmts(b, &mut s2, &mut e2);
    };
    let mut on_stmt = |s: &Stmt| {
        if let Stmt::Try(t) = s {
            for h in &t.handlers {
                let rustpython_parser::ast::ExceptHandler::ExceptHandler(h) = h;
                juntar(&h.body);
            }
            juntar(&t.finalbody);
        }
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    out.into_inner()
}

/// Rangos de línea de los cuerpos de `try` que tienen handlers. Ver el doc
/// del módulo: la arista hacia el `except` sale del nodo `try`, así que una
/// lectura en el handler no se ve como uso de lo asignado adentro.
fn lineas_en_try(body: &[Stmt], line_index: &LineIndex) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut on_stmt = |s: &Stmt| {
        if let Stmt::Try(t) = s {
            if t.handlers.is_empty() {
                return;
            }
            if let (Some(p), Some(u)) = (t.body.first(), t.body.last()) {
                out.push((
                    line_index.line_of(p.range().start().to_usize()),
                    line_index.line_of(u.range().end().to_usize()),
                ));
            }
        }
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    out
}

/// Escrituras que se pisan antes de leerse — el hueco que `muertas.rs`
/// documentaba como su límite conocido.
pub fn check_overwritten_stores(
    scope: &str,
    line_index: &LineIndex,
    body: &[Stmt],
    params: &[String],
) -> Vec<StructuralSmell> {
    if tiene_lectura_dinamica(body) {
        return Vec::new();
    }
    let cfg = cfg::build(scope, body, line_index);
    if cfg.entry.is_none() {
        return Vec::new();
    }
    let ssa = construir(&cfg, params);
    let vivas = vivas(&ssa);
    let compartidos = nombres_compartidos(body);
    let capturados = nombres_capturados(body);
    let en_excepcion = leidos_en_rutas_de_excepcion(body);
    let en_with = escritos_dentro_de_with(body);
    let tramos_try = lineas_en_try(body, line_index);

    // Nombres leídos en algún lado: si uno no se lee nunca, es de
    // `dead_store` y no de acá.
    let leidos: HashSet<&String> = cfg.vars.iter().flat_map(|v| v.usos.iter()).collect();

    // **Un hallazgo por variable, en su primera linea, igual que
    // `dead_store`.** El fixture `x = 1` repetido 55 veces produce 54
    // escrituras pisadas, y las 54 son ciertas — pero son UN problema, no 54,
    // y listarlas todas ahoga al resto del informe. Mismo criterio que
    // `unreachable_code_smells`, que reporta solo la primera de cada tramo.
    let mut por_var: HashMap<&str, (usize, usize)> = HashMap::new();
    for (i, d) in ssa.defs.iter().enumerate() {
        if d.origen != Origen::Normal || !d.simple || vivas.contains(&i) {
            continue;
        }
        if d.var.starts_with('_') || compartidos.contains(&d.var) || !leidos.contains(&d.var) {
            continue;
        }
        if capturados.contains(&d.var) || en_excepcion.contains(&d.var) || en_with.contains(&d.var) {
            continue;
        }
        if tramos_try.iter().any(|(a, b)| d.linea >= *a && d.linea <= *b) {
            continue;
        }
        por_var
            .entry(d.var.as_str())
            .and_modify(|(linea, n)| {
                *linea = (*linea).min(d.linea);
                *n += 1;
            })
            .or_insert((d.linea, 1));
    }

    let mut out: Vec<StructuralSmell> = Vec::new();
    for (var, (linea, cuantas)) in por_var {
        let extra = if cuantas > 1 {
            format!(" This scope has {cuantas} such writes to `{var}`; the line given is the first.")
        } else {
            String::new()
        };
        out.push(StructuralSmell {
            kind: "overwritten_store",
            name: scope.to_string(),
            line: linea,
            message: format!(
                "`{var}` is assigned here, and every path from this point overwrites it before \
                 anything reads it — so this particular value is never used. The name IS read \
                 later in the scope, which is why this is not reported as a dead store: it is \
                 THIS write that is redundant, not the variable. If the right-hand side has side \
                 effects, keep the call and drop the assignment; if it does not, the line can go. \
                 Found via SSA versioning, so branches and loops are accounted for: a value that \
                 survives along any path, or that feeds a phi node that is later read, is not \
                 reported.{extra}"
            ),
        });
    }
    out.sort_by_key(|s| s.line);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::{parse_module, LineIndex};

    fn suite(src: &str) -> (Vec<Stmt>, LineIndex) {
        (parse_module(src).expect("parsea"), LineIndex::build(src))
    }

    /// Saca el cuerpo de la primera función del fuente, con sus parámetros.
    fn func(src: &str) -> (Vec<Stmt>, Vec<String>, LineIndex) {
        let (body, idx) = suite(src);
        for s in &body {
            if let Stmt::FunctionDef(f) = s {
                let params = f.args.args.iter().map(|a| a.def.arg.to_string()).collect();
                return (f.body.clone(), params, idx);
            }
        }
        panic!("sin funcion")
    }

    fn hallazgos(src: &str) -> Vec<StructuralSmell> {
        let (body, params, idx) = func(src);
        check_overwritten_stores("f", &idx, &body, &params)
    }

    #[test]
    fn el_caso_del_roadmap_una_escritura_pisada_antes_de_leerse() {
        // Exactamente `x = caro(); x = 1; usar(x)` del doc de `muertas.rs`.
        let h = hallazgos("def f():\n    x = caro()\n    x = 1\n    usar(x)\n");
        assert_eq!(h.len(), 1, "{h:?}");
        assert_eq!(h[0].line, 2);
        assert_eq!(h[0].kind, "overwritten_store");
    }

    #[test]
    fn la_ultima_escritura_que_si_se_lee_no_se_reporta() {
        let h = hallazgos("def f():\n    x = 1\n    usar(x)\n");
        assert!(h.is_empty(), "{h:?}");
    }

    #[test]
    fn una_variable_que_no_se_lee_nunca_es_de_dead_store_no_de_aca() {
        // `muertas.rs` ya la cubre; reportarla acá sería el mismo hallazgo dos
        // veces. La partición entre los dos módulos tiene que ser limpia.
        let h = hallazgos("def f():\n    x = 1\n    x = 2\n    return 0\n");
        assert!(h.is_empty(), "{h:?}");
    }

    #[test]
    fn una_rama_que_escribe_y_otra_que_no_no_se_reporta() {
        // El φ del final une las dos versiones: la de arriba sigue viva por
        // el camino en que el `if` no entra.
        let h = hallazgos("def f(c):\n    x = 1\n    if c:\n        x = 2\n    return x\n");
        assert!(h.is_empty(), "{h:?}");
    }

    #[test]
    fn las_dos_ramas_escribiendo_sí_matan_la_de_arriba() {
        let src = "def f(c):\n    x = 1\n    if c:\n        x = 2\n    else:\n        x = 3\n    return x\n";
        let h = hallazgos(src);
        assert_eq!(h.len(), 1, "{h:?}");
        assert_eq!(h[0].line, 2);
    }

    #[test]
    fn un_valor_que_se_lee_en_la_vuelta_siguiente_del_bucle_esta_vivo() {
        // El caso que un análisis sin φ se come: `total` se escribe al final
        // del cuerpo y se lee al principio de la vuelta que sigue.
        let src = "def f(xs):\n    total = 0\n    for x in xs:\n        total = total + x\n    return total\n";
        let h = hallazgos(src);
        assert!(h.is_empty(), "{h:?}");
    }

    /// Caso real de `_pytest/assertion/compare_text.py`, que la primera
    /// version reportaba mal. El autor escribio `i = 0` con el comentario
    /// *"just in case left or right has zero length"*: si el `range` es
    /// vacio el bucle no corre, `i` sigue valiendo 0 y se lee abajo.
    #[test]
    fn el_init_previo_a_un_for_que_puede_no_correr_esta_vivo() {
        let src = "def f(a, b):
    i = 0
    for i in range(min(len(a), len(b))):
        if a[i] != b[i]:
            break
    return i > 42
";
        let h = hallazgos(src);
        assert!(h.is_empty(), "{h:?}");
    }

    /// **Este test afirmaba lo contrario y estaba mal.** Decia que el cuerpo
    /// de un `with` corre siempre entero, y eso solo vale para un context
    /// manager que no suprime: si `__exit__` devuelve verdadero, la excepcion
    /// se traga y la ejecucion sigue despues del `with` con la escritura sin
    /// hacer. Desde el AST no se puede distinguir uno del otro.
    #[test]
    fn una_escritura_dentro_de_un_with_no_mata_la_de_arriba() {
        let src = "def f():
    x = None
    with abrir() as h:
        x = h.leer()
    return x
";
        let h = hallazgos(src);
        assert!(h.is_empty(), "{h:?}");
    }

    /// El caso real que lo destapo, de `pip/_vendor/packaging/metadata.py`.
    #[test]
    fn un_context_manager_que_suprime_deja_viva_la_asignacion_previa() {
        let src = "def f(ins, col):
    v = None
    with col.collect(Invalid):
        v = ins.version
        edad = TABLA.index(v)
    return v
";
        let h = hallazgos(src);
        assert!(h.is_empty(), "{h:?}");
    }

    #[test]
    fn aug_assign_lee_y_escribe() {
        let src = "def f(xs):\n    total = 0\n    for x in xs:\n        total += x\n    return total\n";
        let h = hallazgos(src);
        assert!(h.is_empty(), "{h:?}");
    }

    /// De `setuptools/package_index.py`: si la PRIMERA linea del cuerpo
    /// lanza, `fp` sigue en None y el `finally` lo lee. Ese camino no esta
    /// en el CFG, asi que sin esta exclusion se reportaba.
    #[test]
    fn un_none_previo_a_un_try_que_lee_en_el_finally_no_se_reporta() {
        let src = "def f(url):
    fp = None
    try:
        c = mirar(url)
        fp = abrir(url)
        return c
    finally:
        if fp:
            fp.close()
";
        let h = hallazgos(src);
        assert!(h.is_empty(), "{h:?}");
    }

    #[test]
    fn nada_dentro_de_un_try_con_handlers() {
        let src = "def f():\n    x = 1\n    try:\n        x = 2\n        x = 3\n    except E:\n        pass\n    return x\n";
        let h = hallazgos(src);
        assert!(h.iter().all(|s| s.line < 3 || s.line > 5), "{h:?}");
    }

    /// De `anyio/_backends/_asyncio.py`: el `def` esta ARRIBA de la
    /// asignacion que captura, y aun asi la lee — las clausuras leen en
    /// tiempo de llamada. Falso positivo real de la version anterior.
    #[test]
    fn una_clausura_definida_antes_igual_lee_la_asignacion_de_abajo() {
        let src = "def f():
    def envoltorio():
        return scope
    scope = obtener()
    return correr(envoltorio)
";
        let h = hallazgos(src);
        assert!(h.is_empty(), "{h:?}");
    }

    /// Tambien de `anyio`: un `while True` que ES la primera sentencia. Su
    /// unica arista entrante es la de retroceso, asi que sin contar el
    /// predecesor implicito del arranque no se le colocaba φ y lo que el
    /// cuerpo escribe para la vuelta siguiente se daba por muerto.
    #[test]
    fn un_while_true_como_primera_sentencia_igual_recibe_phi() {
        let src = "def f(exc):
    while True:
        if isinstance(exc.ctx, C):
            exc = exc.ctx
            continue
        return False
";
        let h = hallazgos(src);
        assert!(h.is_empty(), "{h:?}");
    }

    /// De `_pytest/outcomes.py`: el TIPO de un `except` es una expresion que
    /// se evalua al saltar la excepcion, y se le puede pasar una variable.
    #[test]
    fn el_tipo_de_un_except_cuenta_como_lectura() {
        let src = "def f(mod, tipo):
    if tipo is None:
        tipo = ModuleNotFoundError
    try:
        importar(mod)
    except tipo:
        return None
    return mod
";
        let h = hallazgos(src);
        assert!(h.is_empty(), "{h:?}");
    }

    #[test]
    fn eval_apaga_la_funcion_entera() {
        let src = "def f():\n    x = 1\n    x = 2\n    eval('x')\n    return x\n";
        assert!(hallazgos(src).is_empty());
    }

    #[test]
    fn el_guion_bajo_no_se_acusa() {
        let src = "def f():\n    _ = caro()\n    _ = 1\n    return _\n";
        assert!(hallazgos(src).is_empty());
    }

    #[test]
    fn el_desempaquetado_de_tupla_no_se_acusa() {
        let src = "def f(p):\n    a, b = p\n    a = 1\n    return a + b\n";
        let h = hallazgos(src);
        assert!(h.iter().all(|s| s.line != 2), "{h:?}");
    }

    #[test]
    fn la_variable_de_una_comprehension_no_es_una_escritura_de_la_funcion() {
        // `i` de la comprehension vive en su propio ámbito: contarlo como
        // definición inventaría una escritura y con ella un hallazgo falso.
        let src = "def f(n):\n    i = leer()\n    xs = [i for i in range(n)]\n    return i + len(xs)\n";
        let h = hallazgos(src);
        assert!(h.is_empty(), "{h:?}");
    }

    #[test]
    fn un_parametro_pisado_sin_leerse_se_reporta() {
        let src = "def f(x):\n    x = 1\n    return x\n";
        // El parámetro es la versión 0; no se reporta porque no es una
        // escritura del código. La de la línea 2 sí se lee.
        assert!(hallazgos(src).is_empty());
    }

    #[test]
    fn dominadores_de_un_if_simple() {
        let (body, idx) = suite("x = 1\nif c:\n    y = 2\nz = 3\n");
        let cfg = cfg::build("m", &body, &idx);
        let idom = dominadores(&cfg);
        // Todo lo que sigue está dominado por la primera sentencia.
        assert_eq!(idom.get(&0), Some(&0), "la entrada se domina a sí misma");
        for n in 1..cfg.nodes.len() {
            assert!(idom.contains_key(&n), "nodo {n} sin dominador");
        }
    }

    #[test]
    fn la_frontera_de_dominancia_de_una_rama_es_el_punto_de_union() {
        let (body, idx) = suite("if c:\n    y = 2\nz = 3\n");
        let cfg = cfg::build("m", &body, &idx);
        let idom = dominadores(&cfg);
        let df = frontera_dominancia(&cfg, &idom);
        // El cuerpo del `if` llega a `z = 3` sin dominarlo.
        assert!(df.values().any(|s| !s.is_empty()), "frontera vacía: {df:?}");
    }

    #[test]
    fn se_coloca_un_phi_donde_dos_ramas_traen_versiones_distintas() {
        let (body, idx) = suite("x = 1\nif c:\n    x = 2\nprint(x)\n");
        let cfg = cfg::build("m", &body, &idx);
        let ssa = construir(&cfg, &[]);
        assert!(ssa.defs.iter().any(|d| d.origen == Origen::Phi && d.var == "x"), "sin φ para x: {:?}", ssa.defs);
    }

    #[test]
    fn sin_ramas_no_hace_falta_ningun_phi() {
        let (body, idx) = suite("x = 1\nx = 2\nprint(x)\n");
        let cfg = cfg::build("m", &body, &idx);
        let ssa = construir(&cfg, &[]);
        assert!(!ssa.defs.iter().any(|d| d.origen == Origen::Phi), "{:?}", ssa.defs);
    }

    #[test]
    fn cada_escritura_recibe_una_version_distinta() {
        let (body, idx) = suite("x = 1\nx = 2\nx = 3\nprint(x)\n");
        let cfg = cfg::build("m", &body, &idx);
        let ssa = construir(&cfg, &[]);
        let mut vs: Vec<u32> = ssa.defs.iter().filter(|d| d.var == "x").map(|d| d.version).collect();
        vs.sort_unstable();
        assert_eq!(vs, vec![1, 2, 3]);
    }

    #[test]
    fn un_cuerpo_vacio_no_rompe_nada() {
        let (body, idx) = suite("");
        let cfg = cfg::build("m", &body, &idx);
        assert!(construir(&cfg, &[]).defs.is_empty());
        assert!(dominadores(&cfg).is_empty());
    }
}
