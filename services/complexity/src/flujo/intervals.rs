//! Fase 32 (Sythrall Mathematical Analysis) — interpretación abstracta,
//! primera porción: dominio de intervalos enteros.
//!
//! **Qué cambia respecto de todo lo demás del motor.** El resto de Sythrall
//! reconoce FORMAS: un `malloc` sin `free`, un `printf` con formato no
//! literal, un bucle anidado con el índice en la posición equivocada. Esto
//! razona sobre VALORES: mantiene una aproximación de qué puede valer cada
//! variable en cada punto del programa, y de ahí deduce cosas que ninguna
//! forma revela.
//!
//! El caso que lo resume:
//!
//! ```python
//! def f(x):
//!     if x > 100:
//!         if x < 50:      # ← imposible, y no hay ninguna "forma" que lo diga
//!             ...
//! ```
//!
//! Ningún patrón sintáctico ve eso: las dos condiciones son perfectamente
//! normales por separado. Lo que lo delata es que, dentro de la primera rama,
//! `x ∈ [101, +∞)`, y ese intervalo no se cruza con `x < 50`. Eso es
//! interpretación abstracta: no se ejecuta el programa, se ejecuta sobre una
//! ABSTRACCIÓN de sus valores.
//!
//! **Complementa al CFG, no lo repite.** `cfg.rs` encuentra código
//! inalcanzable por la ESTRUCTURA del grafo (después de un `return`, después
//! de un `while True` sin `break`). Esto lo encuentra por los VALORES: la
//! rama existe en el grafo, pero su guarda nunca puede ser verdadera.
//!
//! **Alcance deliberado, y por qué.**
//!   - **Enteros y largos de secuencia.** Los flotantes tienen redondeo, NaN
//!     e infinitos: un dominio de intervalos sobre floats necesita reglas
//!     propias, y aplicar las de enteros daría afirmaciones falsas.
//!   - **Bucles: punto fijo con *widening*** (segunda porción). Sin él,
//!     iterar no converge; con él, `n = 0` con `n = n + 1` adentro da
//!     `[0, +∞)` en vez de ⊤ — se pierde el techo a propósito y sobrevive el
//!     piso, que es de donde sale el valor.
//!   - **Estados de salida en vez de un CFG explícito** (tercera porción).
//!     Cada bloque devuelve tres cosas —con qué entorno se cae al final, y
//!     con cuáles se sale por `break` y por `continue`—, exactamente la forma
//!     de `cfg.rs::Salidas` pero llevando entornos en vez de nodos. Eso es lo
//!     que da la propagación por el grafo sin construir uno segundo: un
//!     `return` mata su camino (y el análisis sigue con lo que queda, que es
//!     más preciso que unir con una rama muerta), y los `break` de un bucle
//!     anidado los consume ESE bucle sin necesidad de salir a detectarlos.
//!   - **Solo variables determinadas localmente.** Un parámetro, un retorno
//!     de función o una lectura de atributo son ⊤. Si el valor no se puede
//!     acotar leyendo esta función, no se afirma nada.
//!
//! Por todo eso, lo que se reporta es **decidible**: cuando dice que una
//! condición es siempre falsa, lo probó con los intervalos que dedujo, no lo
//! estimó. Cuando no puede probarlo, se calla.
//!
//! **Lo que NO va a estar acá, y no es un olvido.** El *overflow* de enteros
//! no existe en Python: `int` es de precisión arbitraria, así que no hay nada
//! que probar — ese hallazgo pertenece a un analizador de C/C++, sobre otro
//! árbol y con los anchos de tipo a mano.

use std::collections::HashMap;

use rustpython_parser::ast::{BoolOp, CmpOp, Constant, Expr, Operator, Ranged, Stmt, UnaryOp};

use crate::ast::parser::LineIndex;
use crate::hallazgos::smells::StructuralSmell;

/// Intervalo cerrado `[lo, hi]`. `None` es infinito de ese lado, así que
/// `Interval { lo: None, hi: None }` es ⊤ — "puede valer cualquier cosa".
#[derive(Clone, Copy, PartialEq, Debug)]
struct Interval {
    lo: Option<i64>,
    hi: Option<i64>,
}

impl Interval {
    const TOP: Interval = Interval { lo: None, hi: None };

    fn exacto(v: i64) -> Self {
        Interval { lo: Some(v), hi: Some(v) }
    }

    fn es_top(&self) -> bool {
        self.lo.is_none() && self.hi.is_none()
    }

    /// `true` si el intervalo está vacío — o sea, si el camino que lo produjo
    /// es imposible.
    fn vacio(&self) -> bool {
        matches!((self.lo, self.hi), (Some(a), Some(b)) if a > b)
    }

    /// Unión: el valor puede venir de cualquiera de los dos caminos. Se toma
    /// el intervalo más chico que contiene a los dos (no la unión exacta, que
    /// podría tener agujeros y no es representable acá).
    fn unir(self, otro: Interval) -> Interval {
        if self.vacio() {
            return otro;
        }
        if otro.vacio() {
            return self;
        }
        Interval {
            lo: match (self.lo, otro.lo) {
                (Some(a), Some(b)) => Some(a.min(b)),
                _ => None,
            },
            hi: match (self.hi, otro.hi) {
                (Some(a), Some(b)) => Some(a.max(b)),
                _ => None,
            },
        }
    }

    /// Intersección: se cumplen las dos restricciones a la vez. Es lo que
    /// hace el estrechamiento al entrar en una rama.
    fn intersecar(self, otro: Interval) -> Interval {
        Interval {
            lo: match (self.lo, otro.lo) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (Some(a), None) | (None, Some(a)) => Some(a),
                (None, None) => None,
            },
            hi: match (self.hi, otro.hi) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (Some(a), None) | (None, Some(a)) => Some(a),
                (None, None) => None,
            },
        }
    }

    /// `true` si el 0 está dentro del intervalo. Con ⊤ da `true` también,
    /// pero eso NO alcanza para reportar nada: no saber nada de un valor no
    /// es evidencia de que sea cero. El caller filtra ⊤ antes de preguntar.
    fn contiene_cero(&self) -> bool {
        self.lo.is_none_or(|l| l <= 0) && self.hi.is_none_or(|h| h >= 0)
    }

    fn describir(&self) -> String {
        let l = self.lo.map(|v| v.to_string()).unwrap_or_else(|| "-∞".into());
        let h = self.hi.map(|v| v.to_string()).unwrap_or_else(|| "+∞".into());
        format!("[{l}, {h}]")
    }
}

type Entorno = HashMap<String, Interval>;

/// Clave con la que el LARGO de una secuencia vive en el mismo entorno que
/// los enteros. El espacio hace que no pueda chocar con ningún identificador
/// de Python, y guardarlo acá —en vez de en un segundo mapa— hace que el
/// widening, la unión de ramas y el estrechamiento lo traten igual que a
/// cualquier otro valor, sin una línea de código extra para sostenerlo.
fn clave_largo(nombre: &str) -> String {
    format!("len {nombre}")
}

/// **Nulidad: un retículo propio, encodado sobre el dominio que ya existe.**
///
/// El retículo de nulidad tiene tres elementos útiles —"es `None`", "no es
/// `None`" y "puede ser cualquiera de las dos"— y ese orden es EXACTAMENTE el
/// de los intervalos sobre `{0, 1}`: `[0,0] ⊑ [0,1] ⊒ [1,1]`. No es un truco:
/// es un isomorfismo, y por eso la unión de ramas, el widening y el
/// estrechamiento salen correctos por construcción en vez de necesitar una
/// segunda implementación de cada uno.
///
/// **La cuarta posibilidad es la ausencia de la clave, y es la importante.**
/// "No sé nada de esta variable" (un parámetro cualquiera) NO es lo mismo que
/// "puede ser `None`" (un `d.get(k)`): lo primero tiene que quedarse callado
/// y lo segundo tiene que reportar. Por eso se leen con `es_none` y
/// `puede_ser_none`, que exigen el intervalo EXACTO; cualquier otra cosa
/// —incluido el ⊤ que deja un widening— significa "no sé" y no reporta.
const NULO: Interval = Interval { lo: Some(0), hi: Some(0) };
const NO_NULO: Interval = Interval { lo: Some(1), hi: Some(1) };
const PUEDE_SER_NULO: Interval = Interval { lo: Some(0), hi: Some(1) };

fn clave_nulidad(nombre: &str) -> String {
    format!("None? {nombre}")
}

fn es_none(i: Interval) -> bool {
    i == NULO
}

fn puede_ser_none(i: Interval) -> bool {
    i == PUEDE_SER_NULO
}

fn es_literal_none(e: &Expr) -> bool {
    matches!(e, Expr::Constant(c) if matches!(c.value, Constant::None))
}

/// Funciones de ESTE archivo que pueden devolver `None`, y si son `async`.
/// Es la única pieza interprocedural del análisis, y a propósito: el caso que
/// importa —la función que devuelve un valor en una rama y `None` en otra, y
/// el llamador que no lo chequea— no se ve mirando una sola función.
pub type FuncionesNullables = std::collections::HashMap<String, bool>;

/// Decoradores que no cambian lo que devuelve la función. Con cualquier otro
/// —un `@app.route`, un wrapper propio— no se puede saber qué se llama
/// realmente, así que la función no se registra.
const DECORADORES_TRANSPARENTES: &[&str] =
    &["staticmethod", "classmethod", "property", "lru_cache", "functools.lru_cache", "cache", "functools.cache"];

fn nombre_decorador(e: &Expr) -> String {
    match e {
        Expr::Call(c) => crate::ast::structure::node_name(&c.func),
        otro => crate::ast::structure::node_name(otro),
    }
}

/// Recorre el archivo y anota qué funciones pueden devolver `None`.
///
/// Dos formas cuentan, y la segunda es la que más aparece en código real:
///   - un `return` pelado o un `return None` explícito;
///   - **caerse por abajo** — el `for` que busca algo, no lo encuentra y
///     termina. Ahí no hay ningún `return` que leer, y por eso la pregunta se
///     la hace `cfg.rs::cae_por_abajo` en vez de un escaneo sintáctico.
///
/// En los dos casos se exige ADEMÁS que la función devuelva algún valor real:
/// un procedimiento que nunca devuelve nada no es el caso que esto busca, y
/// registrarlo llenaría de ruido cada `resultado = hacer_algo()`.
///
/// Los generadores quedan afuera: llamar a uno devuelve un generador, nunca
/// `None`, por más `return` pelados que tenga adentro.
pub fn funciones_nullables(body: &[Stmt], line_index: &LineIndex) -> FuncionesNullables {
    let mut out = FuncionesNullables::new();
    recolectar_nullables(body, line_index, &mut out);
    out
}

fn recolectar_nullables(body: &[Stmt], line_index: &LineIndex, out: &mut FuncionesNullables) {
    for stmt in body {
        let datos = match stmt {
            Stmt::FunctionDef(f) => Some((f.name.as_str(), &f.body, &f.decorator_list, false)),
            Stmt::AsyncFunctionDef(f) => Some((f.name.as_str(), &f.body, &f.decorator_list, true)),
            _ => None,
        };
        if let Some((nombre, cuerpo, decoradores, es_async)) = datos {
            let transparente = decoradores
                .iter()
                .all(|d| DECORADORES_TRANSPARENTES.contains(&nombre_decorador(d).as_str()));
            if transparente && puede_devolver_none(cuerpo, line_index) {
                out.insert(nombre.to_string(), es_async);
            }
            recolectar_nullables(cuerpo, line_index, out);
        }
        if let Stmt::ClassDef(c) = stmt {
            recolectar_nullables(&c.body, line_index, out);
        }
    }
}

fn puede_devolver_none(body: &[Stmt], line_index: &LineIndex) -> bool {
    use std::cell::Cell;
    let none_explicito = Cell::new(false);
    let valor_real = Cell::new(false);
    let generador = Cell::new(false);
    let mut on_stmt = |s: &Stmt| {
        if let Stmt::Return(r) = s {
            match &r.value {
                None => none_explicito.set(true),
                Some(v) if es_literal_none(v) => none_explicito.set(true),
                Some(_) => valor_real.set(true),
            }
        }
    };
    let mut on_expr = |e: &Expr| {
        if matches!(e, Expr::Yield(_) | Expr::YieldFrom(_)) {
            generador.set(true);
        }
    };
    crate::ast::walk::walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
    if generador.get() || !valor_real.get() {
        return false;
    }
    none_explicito.get() || crate::flujo::cfg::cae_por_abajo(body, line_index)
}

/// Llamadas que pueden devolver `None`.
///
/// La lista es corta a propósito. `d.get(k)` sin default es el caso más común
/// de Python entero; `re.match`/`search`/`fullmatch` el segundo. **Un
/// `patron.match(s)` sobre un patrón ya compilado NO está**: `.match` sobre un
/// objeto cualquiera no es probadamente el de `re`, y acertar por nombre sería
/// justo el tipo de afirmación que este módulo no hace.
fn nulidad_de_llamada(e: &Expr, nullables: &FuncionesNullables, bajo_await: bool) -> Option<Interval> {
    let Expr::Call(c) = e else { return None };
    let nombre = crate::ast::structure::node_name(&c.func);
    // Con default (`d.get(k, 0)`) ya no devuelve None.
    if nombre.ends_with(".get") && c.args.len() == 1 {
        return Some(PUEDE_SER_NULO);
    }
    if matches!(nombre.as_str(), "re.match" | "re.search" | "re.fullmatch" | "shutil.which") {
        return Some(PUEDE_SER_NULO);
    }
    if nombre == "next" && c.args.len() == 2 && es_literal_none(&c.args[1]) {
        return Some(PUEDE_SER_NULO);
    }
    // Funciones del propio archivo, llamadas por su nombre pelado. Un
    // `obj.helper()` no cuenta: no hay forma de saber que ese `helper` es el
    // de este archivo. Y una `async def` solo devuelve su valor bajo `await`
    // — sin él lo que se obtiene es una corrutina, que nunca es `None`.
    if let Expr::Name(n) = &*c.func {
        if nullables.get(n.id.as_str()) == Some(&bajo_await) {
            return Some(PUEDE_SER_NULO);
        }
    }
    None
}

/// Nulidad abstracta de una expresión. `None` cuando no se sabe nada, que es
/// distinto de saber que puede ser `None`.
fn nulidad_de(e: &Expr, env: &Entorno, nullables: &FuncionesNullables) -> Option<Interval> {
    match e {
        _ if es_literal_none(e) => Some(NULO),
        // Formas que no pueden evaluar a `None` por construcción. Un `BinOp`
        // no está en la lista a propósito: `a + b` sobre objetos propios puede
        // devolver cualquier cosa.
        Expr::Constant(_)
        | Expr::List(_)
        | Expr::Tuple(_)
        | Expr::Dict(_)
        | Expr::Set(_)
        | Expr::ListComp(_)
        | Expr::DictComp(_)
        | Expr::SetComp(_)
        | Expr::JoinedStr(_)
        | Expr::Lambda(_)
        | Expr::Compare(_) => Some(NO_NULO),
        Expr::Name(n) => env.get(&clave_nulidad(&n.id)).copied(),
        Expr::Await(a) => nulidad_de_llamada(&a.value, nullables, true),
        Expr::Call(_) => nulidad_de_llamada(e, nullables, false),
        _ => None,
    }
}

/// `true` si la anotación admite `None`: `Optional[T]`, `Union[T, None]` o
/// `T | None`. Es una afirmación del propio autor del código, no una
/// inferencia — por eso alcanza para marcar el parámetro.
fn anotacion_admite_none(e: &Expr) -> bool {
    match e {
        Expr::Subscript(s) => {
            let base = crate::ast::structure::node_name(&s.value);
            let base = base.rsplit('.').next().unwrap_or(&base);
            if base == "Optional" {
                return true;
            }
            if base != "Union" {
                return false;
            }
            match &*s.slice {
                Expr::Tuple(t) => t.elts.iter().any(es_literal_none),
                otro => es_literal_none(otro),
            }
        }
        Expr::BinOp(b) if matches!(b.op, Operator::BitOr) => {
            es_literal_none(&b.left) || es_literal_none(&b.right) || anotacion_admite_none(&b.left) || anotacion_admite_none(&b.right)
        }
        _ => false,
    }
}

/// Largo abstracto de una expresión de secuencia. Solo lo que se lee de
/// frente: un literal de lista/tupla/cadena, o un nombre cuyo largo ya se
/// dedujo. Un desempaquetado (`[*a, 1]`) no tiene largo conocido.
fn largo_de(e: &Expr, env: &Entorno) -> Interval {
    let literal = |elts: &[Expr]| {
        if elts.iter().any(|x| matches!(x, Expr::Starred(_))) {
            Interval::TOP
        } else {
            Interval::exacto(elts.len() as i64)
        }
    };
    match e {
        Expr::List(l) => literal(&l.elts),
        Expr::Tuple(t) => literal(&t.elts),
        Expr::Constant(c) => match &c.value {
            Constant::Str(t) => Interval::exacto(t.chars().count() as i64),
            _ => Interval::TOP,
        },
        Expr::Name(n) => env.get(&clave_largo(&n.id)).copied().unwrap_or(Interval::TOP),
        _ => Interval::TOP,
    }
}

/// **Widening (∇), el operador que hace converger el análisis de un bucle.**
///
/// El problema que resuelve: si en cada vuelta `n` crece de `[0,0]` a `[0,1]`
/// a `[0,2]`…, iterar hasta el punto fijo no termina nunca. El widening
/// corta eso de raíz: **cuando un extremo se mueve hacia afuera, se lo manda
/// directamente al infinito** en vez de seguirlo paso a paso. Se pierde
/// precisión a propósito —`[0, +∞)` en vez del `[0, 99]` exacto— a cambio de
/// que el análisis termine, que es el compromiso central de la
/// interpretación abstracta.
///
/// El extremo que NO se mueve se conserva: por eso `n = 0` seguido de
/// `n = n + 1` dentro de un bucle da `[0, +∞)` y no ⊤. Ese `0` de abajo es
/// justamente lo que después permite afirmar algo — con ⊤ no quedaría nada.
fn ensanchar_intervalo(viejo: Interval, nuevo: Interval) -> Interval {
    Interval {
        lo: match (viejo.lo, nuevo.lo) {
            (Some(v), Some(n)) if n < v => None,
            (Some(v), Some(_)) => Some(v),
            _ => None,
        },
        hi: match (viejo.hi, nuevo.hi) {
            (Some(v), Some(n)) if n > v => None,
            (Some(v), Some(_)) => Some(v),
            _ => None,
        },
    }
}

fn ensanchar(viejo: &Entorno, nuevo: &Entorno) -> Entorno {
    let mut out = Entorno::new();
    for k in viejo.keys().chain(nuevo.keys()) {
        let v = viejo.get(k).copied().unwrap_or(Interval::TOP);
        let n = nuevo.get(k).copied().unwrap_or(Interval::TOP);
        out.insert(k.clone(), ensanchar_intervalo(v, n));
    }
    out
}

fn unir_entornos(a: &Entorno, b: &Entorno) -> Entorno {
    let mut out = Entorno::new();
    for k in a.keys().chain(b.keys()) {
        let x = a.get(k).copied().unwrap_or(Interval::TOP);
        let y = b.get(k).copied().unwrap_or(Interval::TOP);
        out.insert(k.clone(), x.unir(y));
    }
    out
}

/// Intervalo que recorre la variable de un `for … in range(…)`.
///
/// Es el caso más común de bucle acotado en Python y es completamente
/// demostrable: `range(10)` recorre `[0, 9]`, ni uno más. Con el argumento
/// desconocido (`range(n)`) queda `[0, +∞)`, que **sigue siendo sólido**: si
/// `n <= 0` el cuerpo no corre, y si corre, el índice nunca es negativo. Ese
/// piso en 0 es información real, no una suposición.
fn rango_de(e: &Expr, env: &Entorno) -> Option<Interval> {
    let Expr::Call(c) = e else { return None };
    if crate::ast::structure::node_name(&c.func) != "range" {
        return None;
    }
    let arg = |i: usize| c.args.get(i).map(|a| evaluar(a, env));
    match c.args.len() {
        1 => {
            let fin = arg(0)?;
            Some(Interval { lo: Some(0), hi: fin.hi.map(|h| h - 1) })
        }
        2 | 3 => {
            // Con paso distinto de 1 positivo, los extremos dejan de ser
            // obvios: se prefiere no afirmar nada antes que afirmar mal.
            if c.args.len() == 3 {
                let paso = arg(2)?;
                if paso.lo != paso.hi || paso.lo.is_none_or(|p| p <= 0) {
                    return None;
                }
            }
            let (inicio, fin) = (arg(0)?, arg(1)?);
            Some(Interval { lo: inicio.lo, hi: fin.hi.map(|h| h - 1) })
        }
        _ => None,
    }
}

/// **Estados de salida de un bloque — la misma forma que `cfg.rs::Salidas`,
/// pero llevando ENTORNOS en vez de nodos.**
///
/// Es lo que reemplaza a "recorrer el AST con un entorno". Un bloque no tiene
/// una sola salida: se puede caer por abajo, se puede salir por `break`, se
/// puede volver al encabezado por `continue`, y se puede no salir nunca
/// (`return`/`raise`). Llevar las tres por separado es exactamente lo que da
/// la propagación sobre el grafo de flujo, sin construir un segundo grafo:
///
///   - `normal: None` significa que el camino murió. Un `if` cuya rama
///     verdadera hace `return` sigue con el entorno ESTRECHADO de la otra
///     rama, en vez de unir con una rama que no llega — más preciso, y de ahí
///     salen hallazgos que antes se perdían en la unión.
///   - Los `break`/`continue` los consume el bucle que los contiene, así que
///     los de un bucle anidado nunca llegan al de afuera. Antes eso era un
///     escaneo aparte que tenía que saltearse los bucles internos a mano;
///     ahora sale solo de la estructura.
#[derive(Clone, Default)]
struct Flujo {
    normal: Option<Entorno>,
    breaks: Vec<Entorno>,
    continues: Vec<Entorno>,
}

impl Flujo {
    fn sigue(env: Entorno) -> Flujo {
        Flujo { normal: Some(env), ..Default::default() }
    }
}

/// Confluencia de dos caminos. Si uno de los dos murió, el otro pasa TAL
/// CUAL: unirlo con nada sería tirar el estrechamiento que lo hace útil.
fn unir_flujos(a: Flujo, b: Flujo) -> Flujo {
    let normal = match (a.normal, b.normal) {
        (Some(x), Some(y)) => Some(unir_entornos(&x, &y)),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    };
    let mut breaks = a.breaks;
    breaks.extend(b.breaks);
    let mut continues = a.continues;
    continues.extend(b.continues);
    Flujo { normal, breaks, continues }
}

/// Las expresiones PROPIAS de una sentencia — no las de los cuerpos anidados,
/// que se recorren aparte con su entorno estrechado. Incluye los destinos de
/// asignación a propósito: `xs[a / b] = 1` tiene una división que revisar, y
/// `xs.append(...)` cambia un largo que hay que olvidar.
fn expresiones_propias(stmt: &Stmt) -> Vec<&Expr> {
    let mut v: Vec<&Expr> = Vec::new();
    match stmt {
        Stmt::Assign(a) => {
            v.extend(a.targets.iter());
            v.push(&a.value);
        }
        Stmt::AugAssign(a) => {
            v.push(&a.target);
            v.push(&a.value);
        }
        Stmt::AnnAssign(a) => {
            v.push(&a.target);
            v.extend(a.value.iter().map(|x| &**x));
        }
        Stmt::Return(r) => v.extend(r.value.iter().map(|x| &**x)),
        Stmt::Expr(e) => v.push(&e.value),
        Stmt::If(i) => v.push(&i.test),
        Stmt::While(w) => v.push(&w.test),
        Stmt::For(f) => {
            v.push(&f.target);
            v.push(&f.iter);
        }
        Stmt::Delete(d) => v.extend(d.targets.iter()),
        Stmt::Assert(a) => v.push(&a.test),
        _ => {}
    }
    v
}

/// Todo largo cuyo nombre se usa de una forma que podría cambiarlo deja de
/// ser conocido.
///
/// La regla es de CONTEO, no de forma: se cuentan las apariciones del nombre
/// y las apariciones en las dos únicas posiciones que probadamente no mutan
/// —base de un subíndice (`xs[i]`, leer o escribir un elemento no cambia el
/// largo) y argumento de `len(...)`—; si sobra alguna, el largo se descarta.
/// `xs.append(1)`, `f(xs)`, `xs = otra_cosa` y `del xs[0]` caen todos ahí. Es
/// deliberadamente conservador: perder un largo cuesta un hallazgo, inventarlo
/// cuesta una afirmación falsa.
fn invalidar_largos_usados(stmt: &Stmt, env: &mut Entorno) {
    let mut total: HashMap<String, usize> = HashMap::new();
    let mut seguros: HashMap<String, usize> = HashMap::new();
    // `del xs[0]` SÍ cambia el largo, y el subíndice lo haría pasar por seguro.
    let borrado = matches!(stmt, Stmt::Delete(_));
    for e in expresiones_propias(stmt) {
        crate::ast::walk::walk_expr_tree(e, &mut |x: &Expr| match x {
            Expr::Name(n) => *total.entry(n.id.to_string()).or_default() += 1,
            Expr::Subscript(sub) if !borrado => {
                if let Expr::Name(n) = &*sub.value {
                    *seguros.entry(n.id.to_string()).or_default() += 1;
                }
            }
            Expr::Call(c)
                if crate::ast::structure::node_name(&c.func) == "len" && c.args.len() == 1 =>
            {
                if let Expr::Name(n) = &c.args[0] {
                    *seguros.entry(n.id.to_string()).or_default() += 1;
                }
            }
            _ => {}
        });
    }
    for (nombre, veces) in total {
        if veces > seguros.get(&nombre).copied().unwrap_or(0) {
            env.insert(clave_largo(&nombre), Interval::TOP);
        }
    }
}

/// Dónde está una condición, que es lo que decide qué significa que esté
/// determinada. Un conjunto de un `and` que siempre es FALSO mata la guarda
/// entera; el mismo conjunto siempre VERDADERO no mata nada — simplemente no
/// aporta, y eso es un hallazgo distinto.
#[derive(Clone, Copy, PartialEq)]
enum Rol {
    Guarda,
    /// Operando de un `and`.
    Conjunto,
    /// Operando de un `or`.
    Alternativa,
}

/// Resultado de evaluar una condición contra el entorno actual.
#[derive(PartialEq, Clone, Copy)]
enum Veredicto {
    SiempreVerdadera,
    SiempreFalsa,
    Desconocida,
}

fn suma(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    a?.checked_add(b?)
}

fn resta(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    a?.checked_sub(b?)
}

/// Valor abstracto de una expresión. ⊤ para todo lo que no se pueda acotar
/// leyendo — que es la mayoría, y está bien: sin certeza no hay hallazgo.
fn evaluar(e: &Expr, env: &Entorno) -> Interval {
    match e {
        Expr::Constant(c) => match &c.value {
            Constant::Int(i) => i.to_string().parse::<i64>().map(Interval::exacto).unwrap_or(Interval::TOP),
            Constant::Bool(b) => Interval::exacto(i64::from(*b)),
            _ => Interval::TOP,
        },
        Expr::Name(n) => env.get(n.id.as_str()).copied().unwrap_or(Interval::TOP),
        // `len(...)` nunca es negativo. Incluso con la secuencia desconocida
        // eso deja `[0, +∞)` en vez de ⊤, que ya es información real: hace
        // sólido a `range(len(xs))` sin saber nada de `xs`.
        Expr::Call(c)
            if crate::ast::structure::node_name(&c.func) == "len" && c.args.len() == 1 =>
        {
            let l = largo_de(&c.args[0], env);
            Interval { lo: l.lo.or(Some(0)), hi: l.hi }
        }
        Expr::UnaryOp(u) if matches!(u.op, UnaryOp::USub) => {
            let v = evaluar(&u.operand, env);
            Interval { lo: v.hi.map(|h| -h), hi: v.lo.map(|l| -l) }
        }
        Expr::BinOp(b) => {
            let (x, y) = (evaluar(&b.left, env), evaluar(&b.right, env));
            match b.op {
                Operator::Add => Interval { lo: suma(x.lo, y.lo), hi: suma(x.hi, y.hi) },
                Operator::Sub => Interval { lo: resta(x.lo, y.hi), hi: resta(x.hi, y.lo) },
                // La multiplicación cambia de signo los extremos según los
                // signos de los operandos: se calculan las 4 esquinas en vez
                // de asumir que `lo*lo` es el mínimo.
                Operator::Mult => match (x.lo, x.hi, y.lo, y.hi) {
                    (Some(a), Some(b2), Some(c), Some(d)) => {
                        let esquinas = [a.checked_mul(c), a.checked_mul(d), b2.checked_mul(c), b2.checked_mul(d)];
                        if esquinas.iter().any(Option::is_none) {
                            Interval::TOP
                        } else {
                            let v: Vec<i64> = esquinas.into_iter().flatten().collect();
                            Interval { lo: v.iter().copied().min(), hi: v.iter().copied().max() }
                        }
                    }
                    _ => Interval::TOP,
                },
                _ => Interval::TOP,
            }
        }
        _ => Interval::TOP,
    }
}

/// El intervalo que satisface `variable <op> constante`. Es la pieza que
/// convierte una guarda en conocimiento: dentro de `if x > 100:`, `x` ya no
/// puede ser cualquier cosa.
fn restriccion(op: &CmpOp, k: i64, negada: bool) -> Option<Interval> {
    let op = if !negada {
        *op
    } else {
        match op {
            CmpOp::Lt => CmpOp::GtE,
            CmpOp::LtE => CmpOp::Gt,
            CmpOp::Gt => CmpOp::LtE,
            CmpOp::GtE => CmpOp::Lt,
            CmpOp::Eq => CmpOp::NotEq,
            CmpOp::NotEq => CmpOp::Eq,
            _ => return None,
        }
    };
    Some(match op {
        CmpOp::Lt => Interval { lo: None, hi: Some(k - 1) },
        CmpOp::LtE => Interval { lo: None, hi: Some(k) },
        CmpOp::Gt => Interval { lo: Some(k + 1), hi: None },
        CmpOp::GtE => Interval { lo: Some(k), hi: None },
        CmpOp::Eq => Interval::exacto(k),
        // `x != k` no es un intervalo (tiene un agujero): no se estrecha, que
        // es la respuesta conservadora correcta.
        CmpOp::NotEq => Interval::TOP,
        _ => return None,
    })
}

/// `(nombre, op, constante)` de una comparación simple `x < 10` o `10 > x`.
///
/// **No exige que la variable ya se conozca**, y esa distinción importa:
/// ESTRECHAR y JUZGAR son dos cosas distintas. Para juzgar si una condición
/// está determinada hace falta conocer el valor de antes (`juzgar` filtra ⊤).
/// Para estrechar no: `if x >= 0:` acota `x` a `[0, +∞)` dentro de la rama
/// aunque `x` fuera un parámetro sin acotar — y ese es justamente el caso
/// útil, el que después revela una división por cero que el guard no cubrió.
/// La primera versión exigía las dos cosas y perdía ese hallazgo entero.
fn comparacion_simple(e: &Expr) -> Option<(&str, CmpOp, i64)> {
    let Expr::Compare(c) = e else { return None };
    if c.ops.len() != 1 || c.comparators.len() != 1 {
        return None; // `a < b < c`: fuera de alcance de esta porción
    }
    let derecha = &c.comparators[0];
    if let (Expr::Name(n), Some(k)) = (&*c.left, constante(derecha)) {
        return Some((n.id.as_str(), c.ops[0], k));
    }
    if let (Some(k), Expr::Name(n)) = (constante(&c.left), derecha) {
        // `10 > x` equivale a `x < 10`: se da vuelta el operador.
        let invertido = match c.ops[0] {
            CmpOp::Lt => CmpOp::Gt,
            CmpOp::Gt => CmpOp::Lt,
            CmpOp::LtE => CmpOp::GtE,
            CmpOp::GtE => CmpOp::LtE,
            otro => otro,
        };
        return Some((n.id.as_str(), invertido, k));
    }
    None
}

fn constante(e: &Expr) -> Option<i64> {
    match e {
        Expr::Constant(c) => match &c.value {
            Constant::Int(i) => i.to_string().parse::<i64>().ok(),
            _ => None,
        },
        Expr::UnaryOp(u) if matches!(u.op, UnaryOp::USub) => constante(&u.operand).map(|v| -v),
        _ => None,
    }
}

fn juzgar(test: &Expr, env: &Entorno) -> Veredicto {
    let Some((nombre, op, k)) = comparacion_simple(test) else { return Veredicto::Desconocida };
    let actual = env.get(nombre).copied().unwrap_or(Interval::TOP);
    if actual.es_top() {
        return Veredicto::Desconocida;
    }
    let (Some(si), Some(no)) = (restriccion(&op, k, false), restriccion(&op, k, true)) else {
        return Veredicto::Desconocida;
    };
    let puede_ser_verdadera = !actual.intersecar(si).vacio();
    let puede_ser_falsa = !actual.intersecar(no).vacio();
    match (puede_ser_verdadera, puede_ser_falsa) {
        (true, false) => Veredicto::SiempreVerdadera,
        (false, true) => Veredicto::SiempreFalsa,
        _ => Veredicto::Desconocida,
    }
}

fn estrechar(test: &Expr, env: &Entorno, rama_verdadera: bool) -> Entorno {
    // Una conjunción verdadera dice algo de CADA operando; una disyunción
    // falsa también, por De Morgan (`¬(A ∨ B) ≡ ¬A ∧ ¬B`). Los otros dos
    // casos —una conjunción falsa, una disyunción verdadera— no dicen nada de
    // ningún operando en particular, solo que alguno cumple, y ahí no se
    // estrecha nada.
    if let Expr::BoolOp(b) = test {
        let separable = matches!(b.op, BoolOp::And) == rama_verdadera;
        if !separable {
            return env.clone();
        }
        let mut acumulado = env.clone();
        for v in &b.values {
            acumulado = estrechar(v, &acumulado, rama_verdadera);
        }
        return acumulado;
    }
    let mut nuevo = env.clone();
    if let Some((nombre, op, k)) = comparacion_simple(test) {
        if let Some(r) = restriccion(&op, k, !rama_verdadera) {
            let actual = nuevo.get(nombre).copied().unwrap_or(Interval::TOP);
            nuevo.insert(nombre.to_string(), actual.intersecar(r));
        }
    }
    estrechar_nulidad(test, &mut nuevo, rama_verdadera);
    nuevo
}

/// Lo que una guarda dice sobre la nulidad. Son las tres formas que el código
/// real usa, y cubrirlas es lo que separa un análisis útil de uno que reporta
/// cada acceso a atributo del archivo.
fn estrechar_nulidad(test: &Expr, env: &mut Entorno, rama_verdadera: bool) {
    // `x is None`, `x is not None`, `x == None`, `x != None`.
    if let Expr::Compare(c) = test {
        if c.ops.len() == 1 && c.comparators.len() == 1 {
            if let Expr::Name(n) = &*c.left {
                if es_literal_none(&c.comparators[0]) {
                    let afirma = matches!(c.ops[0], CmpOp::Is | CmpOp::Eq);
                    let niega = matches!(c.ops[0], CmpOp::IsNot | CmpOp::NotEq);
                    if afirma || niega {
                        let resulta_none = afirma == rama_verdadera;
                        env.insert(
                            clave_nulidad(&n.id),
                            if resulta_none { NULO } else { NO_NULO },
                        );
                    }
                }
            }
        }
        return;
    }
    // `if x:` — que sea verdadero implica que no es `None`. Que sea falso NO
    // implica lo contrario: `0`, `""` y `[]` también son falsos.
    if let Expr::Name(n) = test {
        if rama_verdadera {
            env.insert(clave_nulidad(&n.id), NO_NULO);
        }
        return;
    }
    if let Expr::UnaryOp(u) = test {
        if matches!(u.op, UnaryOp::Not) {
            estrechar_nulidad(&u.operand, env, !rama_verdadera);
        }
    }
}

/// Guardas que protegen dentro de la MISMA expresión, por cortocircuito:
/// `v is not None and v.x`, `v and v.x`, `v is None or v.x`, `v.x if v else 0`.
///
/// Sin esto el análisis reportaría justamente el idiom que la gente usa para
/// evitar el error, que es la peor clase de falso positivo posible. La guarda
/// se aplica a la expresión ENTERA y no solo a lo que viene después del
/// operador: eso puede callar un `v.x and v is not None` que sí es un bug,
/// pero un falso negativo cuesta un hallazgo y un falso positivo cuesta la
/// confianza en todos los demás.
fn guardas_locales(e: &Expr, env: &Entorno) -> Entorno {
    match e {
        // `A and B` protege a B cuando A es verdadera; `A or B` protege a B
        // cuando A es FALSA (es la única forma de llegar a B). Las dos son
        // exactamente lo que `estrechar` ya sabe hacer con un `BoolOp`.
        Expr::BoolOp(b) => estrechar(e, env, matches!(b.op, BoolOp::And)),
        Expr::IfExp(i) => estrechar(&i.test, env, true),
        _ => env.clone(),
    }
}

/// Todo nombre asignado en este bloque pasa a ⊤ — su valor Y su largo. Es lo
/// que se aplica al cuerpo de un `try`: una excepción puede cortarlo en
/// cualquier punto, así que después no se sabe cuáles de sus asignaciones
/// llegaron a correr.
fn invalidar_asignados(body: &[Stmt], env: &mut Entorno) {
    let mut on_stmt = |s: &Stmt| {
        let mut olvidar = |n: &str| {
            env.insert(n.to_string(), Interval::TOP);
            env.insert(clave_largo(n), Interval::TOP);
            env.insert(clave_nulidad(n), Interval::TOP);
        };
        if let Stmt::Assign(a) = s {
            for t in &a.targets {
                if let Expr::Name(n) = t {
                    olvidar(&n.id);
                }
            }
        }
        if let Stmt::AugAssign(a) = s {
            if let Expr::Name(n) = &*a.target {
                olvidar(&n.id);
            }
        }
        if let Stmt::For(f) = s {
            if let Expr::Name(n) = &*f.target {
                olvidar(&n.id);
            }
        }
    };
    let mut on_expr = |_: &Expr| {};
    crate::ast::walk::walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
}

/// Divisiones cuyo divisor el análisis puede acotar.
///
/// **Es el ejemplo canónico de por qué un dominio de valores sirve**, y tiene
/// dos casos que conviene no mezclar:
///
///   - El divisor es exactamente `[0, 0]`: la división **siempre** falla.
///     Demostrado, no estimado.
///   - El divisor es un intervalo acotado que **contiene** el 0 (típicamente
///     `[0, +∞)` después de un `if x >= 0:`): la división falla para *algún*
///     valor posible. Es el hallazgo más útil de los dos, porque nombra un
///     error de razonamiento real — se chequeó el signo pero no el cero — y
///     ninguna forma sintáctica lo ve.
///
/// Si el divisor es ⊤ **no se reporta nada**. Que un intervalo desconocido
/// "contenga" el cero es una propiedad de no saber, no evidencia de un bug:
/// reportarlo convertiría toda división del archivo en un hallazgo.
fn revisar_divisiones(e: &Expr, env: &Entorno, out: &mut Vec<(usize, bool, String, String)>, li: &LineIndex) {
    crate::ast::walk::walk_expr_tree(e, &mut |x: &Expr| {
        let Expr::BinOp(b) = x else { return };
        if !matches!(b.op, Operator::Div | Operator::FloorDiv | Operator::Mod) {
            return;
        }
        let divisor = evaluar(&b.right, env);
        if divisor.es_top() || !divisor.contiene_cero() {
            return;
        }
        let seguro = divisor == Interval::exacto(0);
        let texto = match &*b.right {
            Expr::Name(n) => n.id.to_string(),
            _ => "el divisor".to_string(),
        };
        out.push((li.line_of(x.range().start().to_usize()), seguro, texto, divisor.describir()));
    });
}

/// Subíndices cuyo largo Y cuyo índice el análisis puede acotar.
///
/// Es el segundo hallazgo que el dominio habilita, y el que se apoya en las
/// dos mitades a la vez: hace falta saber el largo de la secuencia (que sale
/// del literal que la creó) y el rango del índice (que sale de `range(...)`,
/// de una constante o de una guarda).
///
/// **Se exige largo EXACTO y índice acotado por los dos lados.** Un largo
/// aproximado haría el mensaje ambiguo, y un índice sin techo (`[0, +∞)`)
/// convertiría en hallazgo a cualquier `xs[i]` dentro de un bucle abierto:
/// eso no es evidencia de un bug, es la forma de no saber. Los negativos
/// cuentan como Python los cuenta, desde el final: el rango válido es
/// `[-n, n-1]`, no `[0, n-1]`.
fn revisar_indices(
    e: &Expr,
    env: &Entorno,
    out: &mut Vec<(usize, bool, String, i64, String)>,
    li: &LineIndex,
) {
    crate::ast::walk::walk_expr_tree(e, &mut |x: &Expr| {
        let Expr::Subscript(sub) = x else { return };
        let Expr::Name(base) = &*sub.value else { return };
        let largo = env.get(&clave_largo(&base.id)).copied().unwrap_or(Interval::TOP);
        let (Some(lo), Some(hi)) = (largo.lo, largo.hi) else { return };
        if lo != hi || lo <= 0 {
            return;
        }
        let indice = evaluar(&sub.slice, env);
        if indice.lo.is_none() || indice.hi.is_none() {
            return;
        }
        let valido = Interval { lo: Some(-lo), hi: Some(lo - 1) };
        let dentro = indice.intersecar(valido);
        if dentro == indice {
            return; // enteramente adentro: nada que decir
        }
        out.push((
            li.line_of(x.range().start().to_usize()),
            dentro.vacio(),
            base.id.to_string(),
            lo,
            indice.describir(),
        ));
    });
}

struct Analisis<'a> {
    line_index: &'a LineIndex,
    funcion: String,
    hallazgos: Vec<StructuralSmell>,
    nullables: &'a FuncionesNullables,
    /// Un `None` que se desreferencia corta la ejecución ahí: los accesos
    /// siguientes a la MISMA variable son consecuencia del primero, no
    /// problemas distintos. Se reporta el primero y nada más, igual que
    /// `cfg.rs` reporta la primera sentencia de cada tramo muerto.
    ya_reportados: std::collections::HashSet<String>,
    /// Durante la iteración hacia el punto fijo NO se reporta nada: los
    /// estados intermedios son aproximaciones a medio camino, y un hallazgo
    /// emitido ahí podría ser falso Y estar repetido una vez por vuelta. Se
    /// reporta una sola vez, en la pasada final sobre el estado estabilizado.
    reportar: bool,
}

impl Analisis<'_> {
    /// Recorre un bloque hasta que un camino muera. Lo que viene después de un
    /// `return`/`break`/`continue` no se analiza: es inalcanzable, y quien lo
    /// reporta como tal es `cfg.rs`, no esto.
    fn bloque(&mut self, body: &[Stmt], entrada: Entorno) -> Flujo {
        let mut flujo = Flujo::sigue(entrada);
        for stmt in body {
            let Some(env) = flujo.normal.take() else { break };
            let paso = self.sentencia(stmt, env);
            flujo.normal = paso.normal;
            flujo.breaks.extend(paso.breaks);
            flujo.continues.extend(paso.continues);
        }
        flujo
    }

    fn revisar_expresiones_propias(&mut self, stmt: &Stmt, env: &Entorno) {
        if !self.reportar {
            return;
        }
        let propias = expresiones_propias(stmt);
        let mut encontrados = Vec::new();
        let mut fuera_de_rango = Vec::new();
        for e in &propias {
            revisar_divisiones(e, env, &mut encontrados, self.line_index);
            revisar_indices(e, env, &mut fuera_de_rango, self.line_index);
        }
        for e in &propias {
            self.revisar_derefs(e, env);
        }
        for (line, seguro, nombre, largo, intervalo) in fuera_de_rango {
            let (kind, mensaje) = if seguro {
                (
                    "index_out_of_range",
                    format!(
                        "`{nombre}` has exactly {largo} element(s) here and the index is {intervalo}, so this subscript always raises IndexError. Valid indices are [-{largo}, {}] — Python counts negatives from the end — and no reachable value lands inside. Proven from the literal that built the sequence and the range of the index, not guessed.",
                        largo - 1
                    ),
                )
            } else {
                (
                    "possible_index_out_of_range",
                    format!(
                        "`{nombre}` has exactly {largo} element(s) here and the index is {intervalo}, which reaches outside the valid range [-{largo}, {}] for at least one reachable value. The expression itself looks ordinary; only the two ranges together give it away.",
                        largo - 1
                    ),
                )
            };
            self.hallazgos.push(StructuralSmell { kind, name: self.funcion.clone(), line, message: mensaje });
        }
        for (line, seguro, nombre, intervalo) in encontrados {
            let (kind, mensaje) = if seguro {
                (
                    "division_by_zero",
                    format!(
                        "`{nombre}` is {intervalo} at this point, so this division always fails with ZeroDivisionError. Proven by interval analysis over the values assigned in this function, not estimated."
                    ),
                )
            } else {
                (
                    "possible_division_by_zero",
                    format!(
                        "`{nombre}` is {intervalo} at this point, which includes 0 — this division fails for at least one reachable value. The usual cause is a guard that checks the sign but not the zero (`if {nombre} >= 0:` allows `{nombre} == 0`). Nothing in the shape of this expression looks wrong; it is the range that gives it away."
                    ),
                )
            };
            self.hallazgos.push(StructuralSmell { kind, name: self.funcion.clone(), line, message: mensaje });
        }
    }

    /// Accesos a atributo (`v.x`) y subíndices (`v[0]`) sobre algo que el
    /// análisis probó `None`, o que puede serlo.
    ///
    /// **El caso que justifica la porción entera** es el que cruza funciones:
    /// una que devuelve un valor en una rama y `None` en otra —muchas veces
    /// cayéndose por abajo, sin ningún `return` que leer— y un llamador que
    /// usa el resultado sin chequearlo. Ninguna forma sintáctica lo delata:
    /// las dos funciones, por separado, son perfectamente normales.
    fn revisar_derefs(&mut self, e: &Expr, env: &Entorno) {
        let env = guardas_locales(e, env);
        let li = self.line_index;
        let mut encontrados: Vec<(usize, bool, String, &'static str, &'static str)> = Vec::new();
        crate::ast::walk::walk_expr_tree(e, &mut |x: &Expr| {
            let (base, operacion, excepcion) = match x {
                Expr::Attribute(a) => (&*a.value, "attribute access", "AttributeError"),
                Expr::Subscript(sub) => (&*sub.value, "subscript", "TypeError"),
                _ => return,
            };
            let Expr::Name(n) = base else { return };
            let Some(nulo) = env.get(&clave_nulidad(&n.id)).copied() else { return };
            let seguro = es_none(nulo);
            if !seguro && !puede_ser_none(nulo) {
                return;
            }
            encontrados.push((
                li.line_of(x.range().start().to_usize()),
                seguro,
                n.id.to_string(),
                operacion,
                excepcion,
            ));
        });
        encontrados.sort_by_key(|h| h.0);
        for (line, seguro, nombre, operacion, excepcion) in encontrados {
            if !self.ya_reportados.insert(nombre.clone()) {
                continue;
            }
            let (kind, mensaje) = if seguro {
                (
                    "none_dereference",
                    format!(
                        "`{nombre}` is None on every path that reaches this line, so this {operacion} raises {excepcion}. The value was tracked through the assignments and guards of this function; nothing in the shape of the expression looks wrong on its own."
                    ),
                )
            } else {
                (
                    "possible_none_dereference",
                    format!(
                        "`{nombre}` can be None here — it is None on at least one path that reaches this line — so this {operacion} raises {excepcion} on that path. What the analysis looks for and did not find is a guard: `if {nombre} is None: return`, `if {nombre} is not None:`, or `assert {nombre} is not None` all silence this, because each one proves the value at this point."
                    ),
                )
            };
            self.hallazgos.push(StructuralSmell { kind, name: self.funcion.clone(), line, message: mensaje });
        }
    }

    fn sentencia(&mut self, stmt: &Stmt, mut env: Entorno) -> Flujo {
        self.revisar_expresiones_propias(stmt, &env);
        invalidar_largos_usados(stmt, &mut env);
        match stmt {
            Stmt::Assign(a) => {
                let v = evaluar(&a.value, &env);
                let largo = largo_de(&a.value, &env);
                let nulo = nulidad_de(&a.value, &env, self.nullables).unwrap_or(Interval::TOP);
                for t in &a.targets {
                    if let Expr::Name(n) = t {
                        env.insert(n.id.to_string(), v);
                        env.insert(clave_largo(&n.id), largo);
                        env.insert(clave_nulidad(&n.id), nulo);
                    }
                }
                Flujo::sigue(env)
            }
            // `x: Optional[int] = None` — una asignación anotada asigna igual
            // que una pelada; hasta ahora caía en el `_` y no se miraba.
            Stmt::AnnAssign(a) => {
                if let (Expr::Name(n), Some(valor)) = (&*a.target, &a.value) {
                    env.insert(n.id.to_string(), evaluar(valor, &env));
                    env.insert(clave_largo(&n.id), largo_de(valor, &env));
                    let nulo = nulidad_de(valor, &env, self.nullables).unwrap_or(Interval::TOP);
                    env.insert(clave_nulidad(&n.id), nulo);
                }
                Flujo::sigue(env)
            }
            // Un `assert` que se pasa deja su condición como conocimiento: el
            // `assert x is not None` idiomático protege lo que viene después.
            Stmt::Assert(a) => Flujo::sigue(estrechar(&a.test, &env, true)),
            Stmt::AugAssign(a) => {
                if let Expr::Name(n) = &*a.target {
                    env.insert(n.id.to_string(), Interval::TOP);
                    env.insert(clave_nulidad(&n.id), Interval::TOP);
                }
                Flujo::sigue(env)
            }
            // Estos tres cortan el camino. `return`/`raise` no dejan nada
            // atrás; `break`/`continue` se llevan el entorno vigente para que
            // el bucle que los contiene lo una donde corresponde.
            Stmt::Return(_) | Stmt::Raise(_) => Flujo::default(),
            Stmt::Break(_) => Flujo { normal: None, breaks: vec![env], continues: Vec::new() },
            Stmt::Continue(_) => Flujo { normal: None, breaks: Vec::new(), continues: vec![env] },
            Stmt::If(s) => {
                self.revisar_condicion(&s.test, &env);
                let si = self.bloque(&s.body, estrechar(&s.test, &env, true));
                let no = self.bloque(&s.orelse, estrechar(&s.test, &env, false));
                unir_flujos(si, no)
            }
            Stmt::While(s) => {
                let f = self.bucle(Some(&s.test), None, &s.body, env);
                self.despues_del_bucle(f, &s.orelse)
            }
            Stmt::For(s) => {
                if let Expr::Name(n) = &*s.target {
                    env.insert(clave_nulidad(&n.id), Interval::TOP);
                }
                let ligadura = match (&*s.target, rango_de(&s.iter, &env)) {
                    (Expr::Name(n), Some(r)) => Some((n.id.to_string(), r)),
                    (Expr::Name(n), None) => Some((n.id.to_string(), Interval::TOP)),
                    _ => None,
                };
                let f = self.bucle(None, ligadura, &s.body, env);
                self.despues_del_bucle(f, &s.orelse)
            }
            Stmt::With(s) => self.bloque(&s.body, env),
            Stmt::Try(s) => {
                // Una excepción puede cortar el cuerpo en cualquier punto, así
                // que lo que se asigne ahí no es confiable después.
                let cuerpo = self.bloque(&s.body, env.clone());
                invalidar_asignados(&s.body, &mut env);
                let mut breaks = cuerpo.breaks;
                let mut continues = cuerpo.continues;
                for h in &s.handlers {
                    let rustpython_parser::ast::ExceptHandler::ExceptHandler(h) = h;
                    let f = self.bloque(&h.body, env.clone());
                    breaks.extend(f.breaks);
                    continues.extend(f.continues);
                }
                let f1 = self.bloque(&s.orelse, env.clone());
                breaks.extend(f1.breaks);
                continues.extend(f1.continues);
                let vivo = f1.normal.is_some();
                let f2 = self.bloque(&s.finalbody, f1.normal.unwrap_or(env));
                breaks.extend(f2.breaks);
                continues.extend(f2.continues);
                Flujo { normal: if vivo { f2.normal } else { None }, breaks, continues }
            }
            _ => Flujo::sigue(env),
        }
    }

    /// El `else` de un bucle corre con lo que quedó al salir. Un `break` que
    /// aparezca ADENTRO del `else` no es de este bucle —el `else` está fuera—
    /// así que su flujo se propaga hacia arriba tal cual.
    fn despues_del_bucle(&mut self, f: Flujo, orelse: &[Stmt]) -> Flujo {
        if orelse.is_empty() {
            return f;
        }
        match f.normal {
            Some(env) => self.bloque(orelse, env),
            None => f,
        }
    }

    /// Punto fijo con widening sobre el cuerpo de un bucle.
    ///
    /// Reemplaza al "todo lo que el bucle toca pasa a ⊤" de la porción
    /// anterior. La diferencia se ve en el caso típico: `n = 0` con
    /// `n = n + 1` adentro daba ⊤ (nada que decir) y ahora da `[0, +∞)` — el
    /// piso sobrevive, y eso alcanza para probar cosas.
    fn bucle(
        &mut self,
        test: Option<&Expr>,
        ligadura: Option<(String, Interval)>,
        body: &[Stmt],
        env: Entorno,
    ) -> Flujo {
        // 6 vueltas es holgado: con widening el punto fijo se alcanza en 2 o
        // 3 para cualquier bucle real. El tope existe para que un caso
        // patológico no cuelgue el análisis, no porque se espere llegar ahí.
        const MAX_VUELTAS: usize = 6;

        let preparar = |estado: &Entorno, this_test: Option<&Expr>| {
            let mut e = match this_test {
                Some(t) => estrechar(t, estado, true),
                None => estado.clone(),
            };
            if let Some((nombre, intervalo)) = &ligadura {
                e.insert(nombre.clone(), *intervalo);
            }
            e
        };

        // Con qué entorno arranca la próxima vuelta: la caída normal del
        // cuerpo más cada `continue`, que también vuelve al encabezado.
        let reentrada = |f: &Flujo| -> Option<Entorno> {
            f.continues.iter().fold(f.normal.clone(), |acc, c| {
                Some(match acc {
                    Some(x) => unir_entornos(&x, c),
                    None => c.clone(),
                })
            })
        };

        let anterior = self.reportar;
        self.reportar = false;
        let mut estado = env;
        for _ in 0..MAX_VUELTAS {
            let f = self.bloque(body, preparar(&estado, test));
            let candidato = match reentrada(&f) {
                Some(x) => unir_entornos(&estado, &x),
                None => estado.clone(),
            };
            let siguiente = ensanchar(&estado, &candidato);
            if siguiente == estado {
                break;
            }
            estado = siguiente;
        }
        self.reportar = anterior;

        // Una sola pasada con reporte, sobre el estado ya estabilizado.
        let f = self.bloque(body, preparar(&estado, test));

        // Salida por agotar la condición — incluye el caso de cero vueltas,
        // porque `estado` arrancó siendo el entorno de antes de entrar.
        let mut salida = match test {
            Some(t) => estrechar(t, &estado, false),
            None => estado,
        };
        // Y salida por `break`, cada uno con SU entorno. Antes esto era un
        // escaneo del cuerpo que, al encontrar un `break`, apagaba el
        // estrechamiento entero; ahora el `break` aporta lo que realmente
        // sabía en ese punto, que es más preciso y no necesita el escaneo.
        for b in &f.breaks {
            salida = unir_entornos(&salida, b);
        }
        // Los `continue` ya se consumieron arriba; ni ellos ni los `break`
        // salen de este bucle.
        Flujo::sigue(salida)
    }

    fn revisar_condicion(&mut self, test: &Expr, env: &Entorno) {
        self.juzgar_condicion(test, env, Rol::Guarda);
    }

    /// **Se baja a los operandos de un `and`/`or` en vez de juzgar el conjunto
    /// entero, y ahí está lo que gana.**
    ///
    /// En `if x > 10 and x < 5:` ninguna de las dos comparaciones está
    /// determinada por separado — hace falta la primera para probar la
    /// segunda. Juzgando operando por operando, con el entorno estrechado por
    /// los ANTERIORES (que es lo que el cortocircuito garantiza), el hallazgo
    /// sale solo, apunta a la línea exacta de la comparación imposible, y el
    /// mensaje que ya existía sirve tal cual.
    ///
    /// De la misma máquina sale la redundancia: en `x > 10 and x > 5`, la
    /// segunda es siempre VERDADERA una vez que la primera se cumplió. No mata
    /// la guarda —no aporta nada—, y por eso es otro hallazgo y no el mismo.
    fn juzgar_condicion(&mut self, test: &Expr, env: &Entorno, rol: Rol) {
        if !self.reportar {
            return;
        }
        if let Expr::BoolOp(b) = test {
            let (rol_hijo, rama) = match b.op {
                BoolOp::And => (Rol::Conjunto, true),
                BoolOp::Or => (Rol::Alternativa, false),
            };
            let mut acumulado = env.clone();
            for v in &b.values {
                self.juzgar_condicion(v, &acumulado, rol_hijo);
                acumulado = estrechar(v, &acumulado, rama);
            }
            return;
        }
        let veredicto = juzgar(test, env);
        if veredicto == Veredicto::Desconocida {
            return;
        }
        let Some((nombre, _, _)) = comparacion_simple(test) else { return };
        let intervalo = env.get(nombre).copied().unwrap_or(Interval::TOP);
        let verdadera = veredicto == Veredicto::SiempreVerdadera;
        let (kind, adjetivo, consecuencia) = match (rol, verdadera) {
            (Rol::Guarda, true) => (
                "condition_always_true",
                "always true",
                "the `else` branch (or the code that assumes it can be false) is dead",
            ),
            (Rol::Guarda, false) => {
                ("condition_always_false", "always false", "the guarded body never runs")
            }
            (Rol::Conjunto, false) => (
                "condition_always_false",
                "always false",
                "it makes the whole `and` false, so the guarded body never runs",
            ),
            (Rol::Alternativa, true) => (
                "condition_always_true",
                "always true",
                "it makes the whole `or` true, so the `else` branch is dead",
            ),
            (Rol::Conjunto, true) => (
                "redundant_condition",
                "always true",
                "it adds nothing to the `and` — the operands before it already guarantee it, and removing it does not change what the guard accepts",
            ),
            (Rol::Alternativa, false) => (
                "redundant_condition",
                "always false",
                "it adds nothing to the `or` — reaching it means the operands before it were false, and that already rules this one out too",
            ),
        };
        self.hallazgos.push(StructuralSmell {
            kind,
            name: self.funcion.clone(),
            line: self.line_index.line_of(test.range().start().to_usize()),
            message: format!(
                "`{nombre}` is {} at this point, so this condition is {adjetivo} — {consecuencia}. Derived by interval analysis over the values that reach this line, not by pattern matching: nothing in the shape of the conditions looks wrong on its own.",
                intervalo.describir()
            ),
        });
    }
}

/// Punto de entrada por función. `scope` es su nombre, misma convención que
/// `security.rs`/`control.rs`.
///
/// `args` sirve para sembrar la nulidad de los parámetros (un default `None`
/// o una anotación `Optional[...]`), y `nullables` es lo que se sabe del
/// resto del archivo. Los dos pueden faltar —un `<module>` no tiene firma— y
/// el análisis simplemente afirma menos.
pub fn interval_findings(
    scope: &str,
    body: &[Stmt],
    line_index: &LineIndex,
    args: Option<&rustpython_parser::ast::Arguments>,
    nullables: &FuncionesNullables,
) -> Vec<StructuralSmell> {
    let mut env = Entorno::new();
    if let Some(args) = args {
        for a in args.posonlyargs.iter().chain(args.args.iter()).chain(args.kwonlyargs.iter()) {
            let por_default = a.default.as_deref().is_some_and(es_literal_none);
            let por_anotacion = a.def.annotation.as_deref().is_some_and(anotacion_admite_none);
            if por_default || por_anotacion {
                env.insert(clave_nulidad(&a.def.arg), PUEDE_SER_NULO);
            }
        }
    }
    let mut a = Analisis {
        line_index,
        funcion: scope.to_string(),
        hallazgos: Vec::new(),
        nullables,
        ya_reportados: std::collections::HashSet::new(),
        reportar: true,
    };
    a.bloque(body, env);
    a.hallazgos.sort_by_key(|h| h.line);
    a.hallazgos
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::parse_module;

    fn hallazgos(src: &str) -> Vec<StructuralSmell> {
        let suite = parse_module(src).expect("parse error");
        let idx = LineIndex::build(src);
        let nullables = funciones_nullables(&suite, &idx);
        let (body, args) = match &suite[0] {
            Stmt::FunctionDef(f) => (f.body.clone(), Some((*f.args).clone())),
            _ => (suite.clone(), None),
        };
        interval_findings("f", &body, &idx, args.as_ref(), &nullables)
    }

    /// Igual que `hallazgos`, pero analizando la ÚLTIMA función del archivo.
    /// Es lo que hace falta para el caso interprocedural: la función que
    /// devuelve `None` se define antes que la que la llama.
    fn hallazgos_de_la_ultima(src: &str) -> Vec<StructuralSmell> {
        let suite = parse_module(src).expect("parse error");
        let idx = LineIndex::build(src);
        let nullables = funciones_nullables(&suite, &idx);
        match suite.last() {
            Some(Stmt::FunctionDef(f)) => interval_findings(&f.name, &f.body, &idx, Some(&f.args), &nullables),
            Some(Stmt::AsyncFunctionDef(f)) => interval_findings(&f.name, &f.body, &idx, Some(&f.args), &nullables),
            _ => panic!("esperaba una función al final"),
        }
    }

    #[test]
    fn el_caso_que_ninguna_forma_revela() {
        // Las 2 condiciones son normales por separado. Lo que las delata es
        // que dentro de la primera rama `x` ya no puede ser menor que 50.
        let src = "def f():\n    x = 200\n    if x > 100:\n        if x < 50:\n            pass\n";
        let h = hallazgos(src);
        assert!(h.iter().any(|s| s.kind == "condition_always_false"), "{:?}", h.len());
    }

    #[test]
    fn el_estrechamiento_por_rama_funciona_sin_valor_inicial_conocido() {
        // `x` empieza en top; la guarda externa lo acota a [101, +inf), y eso
        // alcanza para probar que la interna es imposible.
        let src = "def f(x):\n    x = 0\n    if x > 100:\n        if x < 50:\n            pass\n";
        assert!(hallazgos(src).iter().any(|s| s.kind == "condition_always_false"));
    }

    #[test]
    fn una_condicion_siempre_verdadera_se_detecta() {
        let src = "def f():\n    n = 5\n    if n < 10:\n        pass\n";
        let h = hallazgos(src);
        assert!(h.iter().any(|s| s.kind == "condition_always_true"), "{h:?}", h = h.len());
    }

    #[test]
    fn el_mensaje_incluye_el_intervalo_que_lo_prueba() {
        let src = "def f():\n    n = 5\n    if n > 10:\n        pass\n";
        let h = hallazgos(src);
        assert!(h[0].message.contains("[5, 5]"), "{}", h[0].message);
    }

    #[test]
    fn una_variable_desconocida_no_produce_nada() {
        // Un parametro no acotado es top: no hay nada que probar.
        let src = "def f(x):\n    if x > 100:\n        pass\n";
        assert!(hallazgos(src).is_empty());
    }

    #[test]
    fn un_contador_de_bucle_queda_indeterminado_hacia_arriba() {
        // Con widening `n` queda en [0, +inf): el piso sobrevive pero el
        // techo no, asi que `n > 100` PUEDE ser verdadera o falsa y no se
        // afirma nada. Antes del widening era ⊤ y tampoco se afirmaba; la
        // diferencia se ve en el test de abajo, donde el piso SI alcanza.
        let src = "def f(xs):\n    n = 0\n    for x in xs:\n        n = n + 1\n    if n > 100:\n        pass\n";
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src).len());
    }

    #[test]
    fn la_union_al_salir_de_un_if_no_afirma_de_mas() {
        // `x` vale 1 o 100 segun la rama: [1, 100]. Ninguna de las 2
        // condiciones de abajo esta determinada.
        let src = "def f(c):\n    if c:\n        x = 1\n    else:\n        x = 100\n    if x > 50:\n        pass\n";
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src).len());
    }

    #[test]
    fn la_union_si_permite_probar_lo_que_ambas_ramas_comparten() {
        // Las 2 ramas dejan `x` en [1, 100], asi que `x > 1000` es imposible
        // venga por donde venga.
        let src = "def f(c):\n    if c:\n        x = 1\n    else:\n        x = 100\n    if x > 1000:\n        pass\n";
        assert!(hallazgos(src).iter().any(|s| s.kind == "condition_always_false"));
    }

    #[test]
    fn la_aritmetica_se_propaga() {
        let src = "def f():\n    a = 10\n    b = a + 5\n    if b < 3:\n        pass\n";
        assert!(hallazgos(src).iter().any(|s| s.kind == "condition_always_false"));
    }

    #[test]
    fn la_multiplicacion_por_un_negativo_no_confunde_los_extremos() {
        // `a * -1` con a = 10 es -10, no 10: si se asume que lo*lo es el
        // minimo, el intervalo sale al reves y la afirmacion es falsa.
        let src = "def f():\n    a = 10\n    b = a * -1\n    if b > 0:\n        pass\n";
        assert!(hallazgos(src).iter().any(|s| s.kind == "condition_always_false"));
    }

    #[test]
    fn la_comparacion_dada_vuelta_tambien_se_entiende() {
        // `100 < n` es `n > 100`.
        let src = "def f():\n    n = 5\n    if 100 < n:\n        pass\n";
        assert!(hallazgos(src).iter().any(|s| s.kind == "condition_always_false"));
    }

    #[test]
    fn if_true_no_se_reporta() {
        // Idiom deliberado, no un bug: no hay variable involucrada.
        let src = "def f():\n    if True:\n        pass\n";
        assert!(hallazgos(src).is_empty());
    }

    #[test]
    fn distinto_de_no_estrecha_porque_no_es_un_intervalo() {
        // `x != 5` deja un agujero, que este dominio no representa. La
        // respuesta conservadora es no afirmar nada.
        let src = "def f():\n    x = 0\n    if x != 5:\n        if x > 100:\n            pass\n";
        let h = hallazgos(src);
        // La externa SI es demostrable (x = 0 nunca es 5); la interna tambien
        // porque x sigue valiendo 0. Lo que no debe pasar es un panic ni una
        // afirmacion sobre un intervalo con agujero.
        assert!(h.iter().all(|s| s.kind.starts_with("condition_always")));
    }

// ── División por cero (el dominio de intervalos aplicado) ──

    #[test]
    fn una_division_por_cero_segura_se_prueba() {
        let src = "def f(a):\n    d = 0\n    return a / d\n";
        let h = hallazgos(src);
        assert!(h.iter().any(|s| s.kind == "division_by_zero"), "{:?}", h.len());
        assert!(h[0].message.contains("[0, 0]"), "{}", h[0].message);
    }

    #[test]
    fn el_guard_que_chequea_el_signo_pero_no_el_cero_se_detecta() {
        // El hallazgo mas util de los dos: `x >= 0` permite `x == 0`, y
        // ninguna forma sintactica lo ve — es el RANGO lo que lo delata.
        let src = "def f(a, x):\n    if x >= 0:\n        return a / x\n    return 0\n";
        let h = hallazgos(src);
        assert!(h.iter().any(|s| s.kind == "possible_division_by_zero"), "{:?}", h.len());
        assert!(h[0].message.contains("[0, +∞]"), "{}", h[0].message);
    }

    #[test]
    fn un_guard_correcto_no_produce_nada() {
        // `x > 0` deja [1, +inf): el cero quedo afuera de verdad.
        let src = "def f(a, x):\n    if x > 0:\n        return a / x\n    return 0\n";
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src).len());
    }

    #[test]
    fn un_divisor_desconocido_no_se_reporta() {
        // Es LA regla que impide que toda division del archivo sea un
        // hallazgo: no saber nada de un valor no es evidencia de que sea 0.
        let src = "def f(a, x):\n    return a / x\n";
        assert!(hallazgos(src).is_empty());
    }

    #[test]
    fn el_modulo_cuenta_igual_que_la_division() {
        // `x % 0` tambien levanta ZeroDivisionError en Python.
        let src = "def f(a):\n    d = 0\n    return a % d\n";
        assert!(hallazgos(src).iter().any(|s| s.kind == "division_by_zero"));
    }

    #[test]
    fn la_division_entera_tambien_cuenta() {
        let src = "def f(a):\n    d = 0\n    return a // d\n";
        assert!(hallazgos(src).iter().any(|s| s.kind == "division_by_zero"));
    }

    #[test]
    fn un_divisor_tocado_por_un_bucle_no_se_reporta() {
        let src = "def f(a, xs):\n    d = 0\n    for x in xs:\n        d = x\n    return a / d\n";
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src).len());
    }

// ── Widening: lo que el punto fijo habilita ──

    #[test]
    fn el_piso_de_un_contador_sobrevive_al_widening() {
        // ESTE es el test que separa el widening del "todo a ⊤" anterior:
        // `n` arranca en 0 y solo crece, asi que `n < 0` es imposible. Con ⊤
        // no se podia afirmar nada; con [0, +inf) si.
        let src = "def f(xs):\n    n = 0\n    for x in xs:\n        n = n + 1\n    if n < 0:\n        pass\n";
        assert!(hallazgos(src).iter().any(|s| s.kind == "condition_always_false"), "{:?}", hallazgos(src).len());
    }

    #[test]
    fn range_acota_la_variable_del_for() {
        // `range(10)` recorre [0, 9], ni uno mas: es completamente
        // demostrable, y adentro del cuerpo eso alcanza para probar cosas.
        let src = "def f():\n    for i in range(10):\n        if i > 100:\n            pass\n";
        assert!(hallazgos(src).iter().any(|s| s.kind == "condition_always_false"));
    }

    #[test]
    fn range_con_argumento_desconocido_igual_da_el_piso() {
        // `range(n)` con `n` desconocida es [0, +inf): sigue siendo solido
        // (si n <= 0 el cuerpo no corre; si corre, el indice nunca es
        // negativo) y ese piso es informacion real.
        let src = "def f(n):\n    for i in range(n):\n        if i < 0:\n            pass\n";
        assert!(hallazgos(src).iter().any(|s| s.kind == "condition_always_false"));
    }

    #[test]
    fn range_con_inicio_y_fin_se_entiende() {
        let src = "def f():\n    for i in range(5, 10):\n        if i < 5:\n            pass\n";
        assert!(hallazgos(src).iter().any(|s| s.kind == "condition_always_false"));
    }

    #[test]
    fn un_paso_negativo_no_se_afirma() {
        // `range(10, 0, -1)` no tiene los extremos obvios: se prefiere no
        // decir nada antes que decirlo mal.
        let src = "def f():\n    for i in range(10, 0, -1):\n        if i > 100:\n            pass\n";
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src).len());
    }

    #[test]
    fn al_salir_de_un_while_la_condicion_es_falsa() {
        // Si el bucle termino por agotar la condicion, `i < 10` es falsa: o
        // sea `i >= 10`, y por lo tanto `i < 5` es imposible.
        let src = "def f():\n    i = 0\n    while i < 10:\n        i = i + 1\n    if i < 5:\n        pass\n";
        assert!(hallazgos(src).iter().any(|s| s.kind == "condition_always_false"));
    }

    #[test]
    fn un_break_impide_estrechar_al_salir() {
        // Con un `break` de por medio se pudo salir con la condicion
        // TODAVIA verdadera, asi que afirmar `i >= 10` seria falso.
        let src = "def f(c):\n    i = 0\n    while i < 10:\n        if c:\n            break\n        i = i + 1\n    if i < 5:\n        pass\n";
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src).len());
    }

    #[test]
    fn un_bucle_que_crece_sin_tope_termina_el_analisis() {
        // Sin widening esto iteraria para siempre. Que el test termine ES la
        // afirmacion.
        let src = "def f():\n    n = 0\n    while True:\n        n = n + 1\n";
        let _ = hallazgos(src);
    }

    #[test]
    fn no_se_reporta_una_vez_por_vuelta_del_punto_fijo() {
        // Los estados intermedios no reportan: si lo hicieran, este hallazgo
        // saldria 6 veces en vez de 1.
        let src = "def f(a):\n    d = 0\n    for i in range(3):\n        x = a / d\n";
        let h = hallazgos(src);
        assert_eq!(h.iter().filter(|s| s.kind == "division_by_zero").count(), 1, "{:?}", h.len());
    }

// ── Estados de salida: lo que la propagación por el flujo habilita ──

    #[test]
    fn un_return_temprano_refina_lo_que_sigue() {
        // ESTE es el test que separa esta porción de la anterior. La rama
        // verdadera no vuelve, asi que lo que sigue solo puede venir de la
        // FALSA — donde `x` ya se probo igual a 0. Uniendo las dos ramas
        // (que es lo que se hacia antes) el resultado era ⊤ y el hallazgo se
        // perdia entero.
        let src = "def f(x, a):\n    if x != 0:\n        return 0\n    return a / x\n";
        let h = hallazgos(src);
        assert!(h.iter().any(|s| s.kind == "division_by_zero"), "{:?}", h.len());
    }

    #[test]
    fn un_break_guardado_aporta_su_propio_entorno() {
        // El `break` sale con `i` en [8, 9] —lo dice su guarda— y agotar la
        // condicion sale con [10, +inf): la union es [8, +inf), que alcanza
        // para probar que `i < 5` es imposible. La version anterior, al ver
        // un `break`, apagaba el estrechamiento entero y quedaba en
        // [0, +inf), sin nada que afirmar.
        let src = "def f():\n    i = 0\n    while i < 10:\n        if i > 7:\n            break\n        i = i + 1\n    if i < 5:\n        pass\n";
        let h = hallazgos(src);
        assert!(h.iter().any(|s| s.kind == "condition_always_false"), "{:?}", h.len());
    }

    #[test]
    fn un_break_de_un_bucle_anidado_no_sale_del_de_afuera() {
        // El bucle interno consume su propio `break`; el externo no lo ve, y
        // por lo tanto sigue pudiendo estrechar al salir.
        let src = "def f(c):\n    i = 0\n    while i < 10:\n        while c:\n            break\n        i = i + 1\n    if i < 5:\n        pass\n";
        let h = hallazgos(src);
        assert!(h.iter().any(|s| s.kind == "condition_always_false"), "{:?}", h.len());
    }

    // ── Limites de arreglo ──

    #[test]
    fn un_indice_fuera_del_literal_se_prueba() {
        let src = "def f():\n    xs = [1, 2, 3]\n    return xs[5]\n";
        let h = hallazgos(src);
        assert!(h.iter().any(|s| s.kind == "index_out_of_range"), "{:?}", h);
    }

    #[test]
    fn los_indices_negativos_cuentan_desde_el_final() {
        // `xs[-1]` es el ultimo elemento, no un error; `xs[-4]` si lo es.
        let ok = "def f():\n    xs = [1, 2, 3]\n    return xs[-1]\n";
        assert!(hallazgos(ok).is_empty(), "{:?}", hallazgos(ok));
        let mal = "def f():\n    xs = [1, 2, 3]\n    return xs[-4]\n";
        assert!(hallazgos(mal).iter().any(|s| s.kind == "index_out_of_range"));
    }

    #[test]
    fn un_bucle_mas_largo_que_la_lista_es_un_posible_fuera_de_rango() {
        // El indice llega a [0, 4] y la lista tiene 3: falla para algun
        // valor alcanzable, no para todos.
        let src = "def f():\n    xs = [1, 2, 3]\n    for i in range(5):\n        print(xs[i])\n";
        let h = hallazgos(src);
        assert!(h.iter().any(|s| s.kind == "possible_index_out_of_range"), "{:?}", h);
    }

    #[test]
    fn el_idiom_correcto_no_produce_nada() {
        // `range(len(xs))` da exactamente [0, 2] para una lista de 3: el
        // analisis tiene que quedarse callado, o seria ruido en el 99% del
        // codigo real.
        let src = "def f():\n    xs = [1, 2, 3]\n    for i in range(len(xs)):\n        print(xs[i])\n";
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src));
    }

    #[test]
    fn una_cadena_tambien_tiene_largo() {
        let src = "def f():\n    s = \"abc\"\n    return s[7]\n";
        assert!(hallazgos(src).iter().any(|s| s.kind == "index_out_of_range"));
    }

    #[test]
    fn mutar_la_lista_borra_el_largo() {
        // `append` cambia el largo; despues de eso no se sabe cuanto mide y
        // no se afirma nada. Vale lo mismo para pasarla a una funcion, que
        // podria mutarla adentro.
        let crece = "def f():\n    xs = [1, 2, 3]\n    xs.append(4)\n    return xs[3]\n";
        assert!(hallazgos(crece).is_empty(), "{:?}", hallazgos(crece));
        let prestada = "def f(g):\n    xs = [1, 2, 3]\n    g(xs)\n    return xs[9]\n";
        assert!(hallazgos(prestada).is_empty(), "{:?}", hallazgos(prestada));
        let borrada = "def f():\n    xs = [1, 2, 3]\n    del xs[0]\n    return xs[2]\n";
        assert!(hallazgos(borrada).is_empty(), "{:?}", hallazgos(borrada));
    }

    #[test]
    fn escribir_un_elemento_no_cambia_el_largo() {
        // `xs[0] = 9` no la agranda: el largo sobrevive y el fuera de rango
        // que viene despues se sigue probando.
        let src = "def f():\n    xs = [1, 2, 3]\n    xs[0] = 9\n    return xs[5]\n";
        assert!(hallazgos(src).iter().any(|s| s.kind == "index_out_of_range"));
    }

    #[test]
    fn un_indice_sin_acotar_no_se_reporta() {
        // Sin techo conocido, `xs[i]` no es evidencia de nada: es la forma
        // de no saber, y reportarla convertiria en hallazgo a cada subindice
        // dentro de un bucle abierto.
        let src = "def f(n):\n    xs = [1, 2, 3]\n    for i in range(n):\n        print(xs[i])\n";
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src));
    }

// ── Nulidad ──

    #[test]
    fn un_none_asignado_se_prueba() {
        let src = "def f():\n    x = None\n    return x.upper()\n";
        let h = hallazgos(src);
        assert!(h.iter().any(|s| s.kind == "none_dereference"), "{:?}", h);
    }

    #[test]
    fn el_subindice_sobre_none_es_un_typeerror_no_un_attributeerror() {
        let src = "def f():\n    x = None\n    return x[0]\n";
        let h = hallazgos(src);
        assert!(h.iter().any(|s| s.kind == "none_dereference" && s.message.contains("TypeError")), "{:?}", h);
    }

    #[test]
    fn solo_se_reporta_el_primer_deref_de_cada_variable() {
        // El primero corta la ejecucion; los siguientes son consecuencia, no
        // problemas distintos.
        let src = "def f():\n    x = None\n    print(x.a)\n    print(x.b)\n";
        let h = hallazgos(src);
        assert_eq!(h.iter().filter(|s| s.kind == "none_dereference").count(), 1, "{:?}", h);
    }

    #[test]
    fn un_default_none_sin_guarda_se_reporta() {
        let src = "def f(x=None):\n    return x.upper()\n";
        assert!(hallazgos(src).iter().any(|s| s.kind == "possible_none_dereference"));
    }

    #[test]
    fn las_tres_formas_de_anotar_opcional_se_entienden() {
        for src in [
            "def f(x: Optional[str]):\n    return x.upper()\n",
            "def f(x: str | None):\n    return x.upper()\n",
            "def f(x: Union[str, None]):\n    return x.upper()\n",
        ] {
            assert!(
                hallazgos(src).iter().any(|s| s.kind == "possible_none_dereference"),
                "no se detecto en: {src}"
            );
        }
    }

    #[test]
    fn una_guarda_calla_el_analisis() {
        // Las tres formas idiomaticas de protegerse. Reportar sobre codigo ya
        // protegido seria el peor falso positivo posible: justo el idiom que
        // la gente usa para evitar el error.
        for src in [
            "def f(x=None):\n    if x is None:\n        return 0\n    return x.upper()\n",
            "def f(x=None):\n    if x is not None:\n        return x.upper()\n    return 0\n",
            "def f(x=None):\n    assert x is not None\n    return x.upper()\n",
            "def f(x=None):\n    if not x:\n        return 0\n    return x.upper()\n",
        ] {
            assert!(hallazgos(src).is_empty(), "reporto de mas en: {src}");
        }
    }

    #[test]
    fn el_cortocircuito_protege_dentro_de_la_misma_expresion() {
        for src in [
            "def f(x=None):\n    return x and x.upper()\n",
            "def f(x=None):\n    return x is not None and x.upper()\n",
            "def f(x=None):\n    return x.upper() if x else \"\"\n",
        ] {
            assert!(hallazgos(src).is_empty(), "reporto de mas en: {src}");
        }
    }

    #[test]
    fn el_get_de_un_diccionario_puede_devolver_none() {
        let sin_default = "def f(d, k):\n    v = d.get(k)\n    return v.upper()\n";
        assert!(hallazgos(sin_default).iter().any(|s| s.kind == "possible_none_dereference"));
        // Con default ya no devuelve None: tiene que callarse.
        let con_default = "def f(d, k):\n    v = d.get(k, \"\")\n    return v.upper()\n";
        assert!(hallazgos(con_default).is_empty(), "{:?}", hallazgos(con_default));
    }

    #[test]
    fn re_match_puede_devolver_none() {
        let src = "def f(p, s):\n    m = re.match(p, s)\n    return m.group(0)\n";
        assert!(hallazgos(src).iter().any(|s| s.kind == "possible_none_dereference"));
    }

    #[test]
    fn una_union_de_ramas_da_puede_ser_none() {
        let src = "def f(c):\n    if c:\n        v = None\n    else:\n        v = \"a\"\n    return v.upper()\n";
        assert!(hallazgos(src).iter().any(|s| s.kind == "possible_none_dereference"));
    }

    #[test]
    fn el_widening_deja_desconocido_y_desconocido_no_reporta() {
        // Despues del bucle `v` queda en ⊤, que NO es "puede ser None": es
        // "no se". La diferencia entre las dos es la que decide si esto es un
        // analisis o un generador de ruido.
        let src = "def f(xs):\n    v = None\n    for x in xs:\n        v = x\n    return v.upper()\n";
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src));
    }

    // ── El caso interprocedural, que es el que justifica la porción ──

    #[test]
    fn una_funcion_que_se_cae_por_abajo_contagia_a_su_llamador() {
        // `buscar` no tiene NINGUN `return` que devuelva None: se cae por
        // abajo cuando el for no encuentra nada. Las dos funciones, por
        // separado, son perfectamente normales.
        let src = concat!(
            "def buscar(xs, k):\n",
            "    for x in xs:\n",
            "        if x == k:\n",
            "            return x\n",
            "\n",
            "\n",
            "def usar(xs):\n",
            "    encontrado = buscar(xs, 3)\n",
            "    return encontrado.nombre\n",
        );
        let h = hallazgos_de_la_ultima(src);
        assert!(h.iter().any(|s| s.kind == "possible_none_dereference"), "{:?}", h);
    }

    #[test]
    fn una_funcion_que_siempre_devuelve_valor_no_contagia() {
        let src = concat!(
            "def buscar(xs, k):\n",
            "    for x in xs:\n",
            "        if x == k:\n",
            "            return x\n",
            "    return xs[0]\n",
            "\n",
            "\n",
            "def usar(xs):\n",
            "    encontrado = buscar(xs, 3)\n",
            "    return encontrado.nombre\n",
        );
        assert!(hallazgos_de_la_ultima(src).is_empty(), "{:?}", hallazgos_de_la_ultima(src));
    }

    #[test]
    fn un_procedimiento_no_cuenta_como_nullable() {
        // Nunca devuelve un valor real: `r = log(...)` no es el caso que esto
        // busca, y registrarlo llenaria de ruido cada llamada del archivo.
        let src = concat!(
            "def log(m):\n",
            "    print(m)\n",
            "\n",
            "\n",
            "def usar():\n",
            "    r = log(\"x\")\n",
            "    return r.upper()\n",
        );
        assert!(hallazgos_de_la_ultima(src).is_empty(), "{:?}", hallazgos_de_la_ultima(src));
    }

    #[test]
    fn un_generador_devuelve_un_generador_no_none() {
        // Tiene un `return` pelado Y un `return 1`, asi que por la regla de
        // los returns seria nullable. Pero llamarlo devuelve un generador.
        let src = concat!(
            "def gen(xs):\n",
            "    if not xs:\n",
            "        return\n",
            "    yield xs[0]\n",
            "    return 1\n",
            "\n",
            "\n",
            "def usar(xs):\n",
            "    g = gen(xs)\n",
            "    return g.close()\n",
        );
        assert!(hallazgos_de_la_ultima(src).is_empty(), "{:?}", hallazgos_de_la_ultima(src));
    }

    #[test]
    fn una_corrutina_sin_await_es_una_corrutina() {
        // `cargar(k)` sin `await` devuelve un objeto corrutina, nunca None.
        let sin_await = concat!(
            "async def cargar(k):\n",
            "    if k:\n",
            "        return k\n",
            "    return None\n",
            "\n",
            "\n",
            "def usar(k):\n",
            "    v = cargar(k)\n",
            "    return v.upper()\n",
        );
        assert!(hallazgos_de_la_ultima(sin_await).is_empty(), "{:?}", hallazgos_de_la_ultima(sin_await));
        let con_await = concat!(
            "async def cargar(k):\n",
            "    if k:\n",
            "        return k\n",
            "    return None\n",
            "\n",
            "\n",
            "async def usar(k):\n",
            "    v = await cargar(k)\n",
            "    return v.upper()\n",
        );
        assert!(hallazgos_de_la_ultima(con_await).iter().any(|s| s.kind == "possible_none_dereference"));
    }

    #[test]
    fn un_metodo_del_mismo_nombre_no_se_confunde_con_la_funcion() {
        // `obj.buscar()` no es probadamente el `buscar` de este archivo, asi
        // que no se afirma nada sobre lo que devuelve.
        let src = concat!(
            "def buscar(xs, k):\n",
            "    for x in xs:\n",
            "        if x == k:\n",
            "            return x\n",
            "\n",
            "\n",
            "def usar(obj):\n",
            "    encontrado = obj.buscar(1, 2)\n",
            "    return encontrado.nombre\n",
        );
        assert!(hallazgos_de_la_ultima(src).is_empty(), "{:?}", hallazgos_de_la_ultima(src));
    }

// ── Lógica proposicional con valores: `and` / `or` ──

    #[test]
    fn dos_conjuntos_que_se_contradicen_se_prueban() {
        // Ninguna de las dos comparaciones esta determinada por separado:
        // hace falta la primera para probar la segunda. Ese es el caso que
        // solo aparece bajando a los operandos.
        let src = "def f(x):\n    if x > 10 and x < 5:\n        pass\n";
        let h = hallazgos(src);
        assert!(h.iter().any(|s| s.kind == "condition_always_false"), "{:?}", h);
    }

    #[test]
    fn un_conjunto_implicado_por_el_anterior_es_redundante() {
        // `x > 5` es siempre verdadera una vez que `x > 10` se cumplio: no
        // mata la guarda, no aporta nada. Es otro hallazgo, no el mismo.
        let src = "def f(x):\n    if x > 10 and x > 5:\n        pass\n";
        let h = hallazgos(src);
        assert!(h.iter().any(|s| s.kind == "redundant_condition"), "{:?}", h);
    }

    #[test]
    fn una_alternativa_ya_descartada_es_redundante() {
        // Llegar a `x > 20` significa que `x > 10` fue falsa, o sea x <= 10.
        let src = "def f(x):\n    if x > 10 or x > 20:\n        pass\n";
        let h = hallazgos(src);
        assert!(h.iter().any(|s| s.kind == "redundant_condition"), "{:?}", h);
    }

    #[test]
    fn una_disyuncion_que_cubre_todo_es_siempre_verdadera() {
        let src = "def f(x):\n    if x > 10 or x <= 10:\n        pass\n";
        let h = hallazgos(src);
        assert!(h.iter().any(|s| s.kind == "condition_always_true"), "{:?}", h);
    }

    #[test]
    fn una_conjuncion_verdadera_estrecha_por_las_dos_puntas() {
        // `x >= 0 and x <= 10` deja x en [0, 10], que contiene el 0: la
        // division de abajo falla para un valor alcanzable. Sin entender el
        // `and` esto no se veia.
        let src = "def f(x, a):\n    if x >= 0 and x <= 10:\n        return a / x\n    return 0\n";
        let h = hallazgos(src);
        assert!(h.iter().any(|s| s.kind == "possible_division_by_zero"), "{:?}", h);
    }

    #[test]
    fn una_disyuncion_verdadera_no_estrecha_nada() {
        // `x > 0 or c` verdadera NO prueba que x > 0: pudo ser `c`. Afirmar
        // lo contrario seria el error clasico.
        let src = "def f(x, c, a):\n    if x > 0 or c:\n        return a / x\n    return 0\n";
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src));
    }

    #[test]
    fn una_comparacion_encadenada_queda_fuera_de_alcance() {
        let src = "def f():\n    x = 5\n    if 1 < x < 3:\n        pass\n";
        assert!(hallazgos(src).is_empty());
    }
}
