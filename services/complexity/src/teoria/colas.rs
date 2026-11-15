//! Fase 32 (Sythrall Mathematical Analysis) — teoría de colas: la ausencia de
//! backpressure.
//!
//! **Lo que se prueba y lo que no, que acá es toda la diferencia.** Que una
//! cola no tenga cota se LEE de la llamada: `Queue()` sin `maxsize` es un
//! hecho, no una estimación. Que esa cola vaya a crecer es otra cosa
//! completamente — es la ley de Little (`L = λW`): si la tasa de llegada λ
//! supera a la de servicio μ, la longitud crece sin techo; si λ < μ se
//! estabiliza en una media finita. **Nada en el código fuente dice cuánto
//! valen λ y μ**: dependen de la carga, no del texto. Por eso lo que se
//! reporta es la cota que falta, nunca un leak.
//!
//! **Y la cota no compra corrección, compra DÓNDE falla.** Con `maxsize`, un
//! productor que le gana a su consumidor se bloquea (o es rechazado) en la
//! cola misma, al lado de la causa. Sin ella, la misma sobrecarga aparece
//! mucho después como un OOM en un lugar que no tiene nada que ver. Eso es
//! backpressure: convertir una sobrecarga invisible en una falla local y
//! visible.
//!
//! **Dos exclusiones deliberadas, porque incluirlas sería afirmar mal.**
//!   - **`ThreadPoolExecutor()` sin `max_workers` NO es un pool sin cota.**
//!     El default es `min(32, cpu_count + 4)`, o sea acotado. Un analizador
//!     ingenuo lo reportaría junto con las colas —"le falta el parámetro"— y
//!     estaría diciendo algo falso.
//!   - **`collections.deque()` sin `maxlen` tampoco entra.** Usar un deque
//!     como lista es el caso normal y no tiene nada de malo; solo es un leak
//!     cuando se lo usa de buffer, y **nada en la forma distingue los dos
//!     usos**. Reportarlo sería convertir en hallazgo a casi todo `deque()`
//!     del mundo.
//!
//! La cota de una caché entra por el mismo razonamiento: `lru_cache` sin
//! tope es una cola de resultados que nunca desaloja.

use rustpython_parser::ast::{Constant, Expr, Ranged, Stmt, UnaryOp};

use crate::ast::parser::LineIndex;
use crate::hallazgos::smells::StructuralSmell;
use crate::ast::structure::node_name;
use crate::ast::walk::walk_stmts_own_scope;

/// Colas cuyo primer parámetro es `maxsize` y cuyo default (0) significa
/// "sin tope". Se aceptan también los nombres pelados porque `from queue
/// import Queue` es la forma habitual; si el proyecto tiene una clase propia
/// con ese nombre, el mensaje deja ver enseguida que no aplica.
const COLAS_CON_MAXSIZE: &[&str] = &[
    "Queue",
    "LifoQueue",
    "PriorityQueue",
    "JoinableQueue",
    "queue.Queue",
    "queue.LifoQueue",
    "queue.PriorityQueue",
    "asyncio.Queue",
    "asyncio.LifoQueue",
    "asyncio.PriorityQueue",
    "multiprocessing.Queue",
    "multiprocessing.JoinableQueue",
    "mp.Queue",
];

/// `SimpleQueue` no tiene `maxsize` NI SIQUIERA como opción: es no acotada por
/// diseño. Merece su propio mensaje, porque el arreglo no es agregar un
/// argumento sino cambiar de clase.
const COLAS_SIN_TOPE_POSIBLE: &[&str] =
    &["SimpleQueue", "queue.SimpleQueue", "multiprocessing.SimpleQueue", "asyncio.SimpleQueue"];

const LANZADORES: &[&str] = &["Thread", "threading.Thread"];

fn constante_entera(e: &Expr) -> Option<i64> {
    match e {
        Expr::Constant(c) => match &c.value {
            Constant::Int(i) => i.to_string().parse().ok(),
            _ => None,
        },
        // `maxsize=-1` no es una constante en el árbol: es un `-` unario
        // sobre el 1. Sin este caso, el negativo pasaba por "no literal" y se
        // trataba como una cota — que es justo lo contrario de lo que es.
        Expr::UnaryOp(u) if matches!(u.op, UnaryOp::USub) => constante_entera(&u.operand).map(|v| -v),
        _ => None,
    }
}

fn es_none(e: &Expr) -> bool {
    matches!(e, Expr::Constant(c) if matches!(c.value, Constant::None))
}

/// El argumento `maxsize`, sea posicional (es el primero) o por nombre.
fn argumento_maxsize(c: &rustpython_parser::ast::ExprCall) -> Option<&Expr> {
    if let Some(primero) = c.args.first() {
        return Some(primero);
    }
    c.keywords
        .iter()
        .find(|k| k.arg.as_ref().is_some_and(|a| a.as_str() == "maxsize"))
        .map(|k| &k.value)
}

fn es_lanzador_de_tarea(nombre: &str) -> bool {
    LANZADORES.contains(&nombre) || nombre.ends_with("create_task") || nombre.ends_with("ensure_future")
}

/// `true` si en la función hay algún semáforo a la vista. No se intenta
/// probar que efectivamente rodea al lanzamiento: alcanza con que exista para
/// CALLARSE, que es el lado seguro del error.
fn hay_semaforo(body: &[Stmt]) -> bool {
    let mut encontrado = false;
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |e: &Expr| {
        if matches!(e, Expr::Name(_) | Expr::Attribute(_)) && node_name(e).to_lowercase().contains("semaphore") {
            encontrado = true;
        }
    };
    walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
    encontrado
}

/// Colas sin cota construidas en este cuerpo.
fn revisar_colas(scope: &str, body: &[Stmt], line_index: &LineIndex, out: &mut Vec<StructuralSmell>) {
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |e: &Expr| {
        let Expr::Call(c) = e else { return };
        let nombre = node_name(&c.func);
        let linea = line_index.line_of(e.range().start().to_usize());

        if COLAS_SIN_TOPE_POSIBLE.contains(&nombre.as_str()) {
            out.push(StructuralSmell {
                kind: "unbounded_queue",
                name: scope.to_string(),
                line: linea,
                message: format!(
                    "`{nombre}` has no `maxsize` at all — it is unbounded by design, so the fix is not an extra argument but a different class (`Queue(maxsize=…)`). What a bound buys is not correctness but WHERE the failure lands: with one, a producer that outruns its consumer blocks at the queue, next to the cause; without one, the same overload surfaces later as an OOM somewhere unrelated."
                ),
            });
            return;
        }

        if !COLAS_CON_MAXSIZE.contains(&nombre.as_str()) {
            return;
        }
        let (explicito, detalle) = match argumento_maxsize(c) {
            None => (false, "no `maxsize`".to_string()),
            Some(a) => match constante_entera(a) {
                // 0 y los negativos significan "sin tope" en las tres
                // familias de cola: el autor lo eligió, pero el efecto es el
                // mismo y conviene decirlo distinto.
                Some(n) if n <= 0 => (true, format!("`maxsize={n}`, which means unbounded")),
                // Un tope no literal (`maxsize=cfg.tam`) es un tope: no se
                // afirma nada sobre su valor.
                _ => return,
            },
        };
        let matiz = if explicito {
            "The bound was chosen explicitly, so this may be deliberate — but the consequence is the same."
        } else {
            "The default is 0, which means infinite; leaving it out is not the same as picking a large number."
        };
        out.push(StructuralSmell {
            kind: "unbounded_queue",
            name: scope.to_string(),
            line: linea,
            message: format!(
                "`{nombre}` is created with {detalle}, so it grows until memory runs out. {matiz} Whether it actually grows is Little's law — it depends on arrival rate against service rate, and nothing in this file says what those are, which is why this reports the missing bound and not a leak. What the bound buys is where the failure lands: at the queue, next to the cause, instead of as an OOM somewhere unrelated."
            ),
        });
    };
    walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
}

/// Lanzamientos de tarea/hilo dentro de un bucle, sin nada que los acote.
fn revisar_lanzamientos(scope: &str, body: &[Stmt], line_index: &LineIndex, out: &mut Vec<StructuralSmell>) {
    if hay_semaforo(body) {
        return;
    }
    let mut vistos: Vec<usize> = Vec::new();
    let mut on_stmt = |s: &Stmt| {
        let cuerpo = match s {
            Stmt::For(f) => &f.body,
            Stmt::While(w) => &w.body,
            Stmt::AsyncFor(f) => &f.body,
            _ => return,
        };
        // Un `.join()` adentro del mismo bucle secuencializa los
        // lanzamientos: nunca hay más de uno vivo, y reportarlo sería falso.
        let mut espera = false;
        let mut lanzamientos: Vec<(usize, String)> = Vec::new();
        let mut nada = |_: &Stmt| {};
        let mut mirar = |e: &Expr| {
            let Expr::Call(c) = e else { return };
            let nombre = node_name(&c.func);
            if nombre.ends_with(".join") {
                espera = true;
                return;
            }
            if es_lanzador_de_tarea(&nombre) {
                lanzamientos.push((line_index.line_of(e.range().start().to_usize()), nombre));
            }
        };
        walk_stmts_own_scope(cuerpo, &mut nada, &mut mirar);
        if espera {
            return;
        }
        for (linea, nombre) in lanzamientos {
            if vistos.contains(&linea) {
                continue; // bucles anidados: el mismo lanzamiento una sola vez
            }
            vistos.push(linea);
            out.push(StructuralSmell {
                kind: "unbounded_task_spawn",
                name: scope.to_string(),
                line: linea,
                message: format!(
                    "`{nombre}` runs inside a loop with nothing limiting how many are live at once, so the concurrency is whatever the iterated collection happens to hold. A `Semaphore`/`BoundedSemaphore` around the body, or a fixed-size pool, is what caps it. If that collection is small and fixed this is fine — the analysis cannot tell how big it is, only that nothing bounds it."
                ),
            });
        }
    };
    let mut sin_expr = |_: &Expr| {};
    walk_stmts_own_scope(body, &mut on_stmt, &mut sin_expr);
}

/// `@cache` / `@lru_cache(maxsize=None)`: una cola de resultados que nunca
/// desaloja. Se mira el decorador, así que se recorre el árbol de
/// definiciones aparte del cuerpo.
fn revisar_cache_del_decorador(
    nombre_funcion: &str,
    decoradores: &[Expr],
    line_index: &LineIndex,
    out: &mut Vec<StructuralSmell>,
) {
    for d in decoradores {
        let (texto, sin_tope) = match d {
            Expr::Call(c) => {
                let n = node_name(&c.func);
                if !matches!(n.as_str(), "lru_cache" | "functools.lru_cache") {
                    continue;
                }
                let arg = c
                    .args
                    .first()
                    .or_else(|| {
                        c.keywords
                            .iter()
                            .find(|k| k.arg.as_ref().is_some_and(|a| a.as_str() == "maxsize"))
                            .map(|k| &k.value)
                    })
                    .filter(|a| es_none(a));
                (format!("@{n}(maxsize=None)"), arg.is_some())
            }
            // `@cache` y `@functools.cache` SON `lru_cache(maxsize=None)`.
            // Un `@lru_cache` pelado, en cambio, es maxsize=128: acotado, y
            // reportarlo sería falso.
            otro => {
                let n = node_name(otro);
                (format!("@{n}"), matches!(n.as_str(), "cache" | "functools.cache"))
            }
        };
        if !sin_tope {
            continue;
        }
        out.push(StructuralSmell {
            kind: "unbounded_cache",
            name: nombre_funcion.to_string(),
            line: line_index.line_of(d.range().start().to_usize()),
            message: format!(
                "`{texto}` keeps every distinct argument tuple forever and never evicts. That is correct while the key space is small and closed (an enum, a config name); it becomes a memory leak the moment a key can come from input the caller does not control. Any `maxsize` turns unbounded growth into eviction — `@lru_cache` with no arguments is already bounded at 128."
            ),
        });
    }
}

/// Punto de entrada, sobre el MÓDULO entero.
///
/// A diferencia del resto de los productores de smells, éste no se llama una
/// vez por función: necesita ver los decoradores, que no están en el cuerpo de
/// la función sino en su definición. Recorre él mismo el árbol llevando el
/// nombre del scope, que es lo que después muestra el panel.
pub fn resource_smells(suite: &[Stmt], line_index: &LineIndex) -> Vec<StructuralSmell> {
    let mut out = Vec::new();
    recorrer(suite, "<module>", line_index, &mut out);
    out.sort_by_key(|s| s.line);
    out
}

fn recorrer(body: &[Stmt], scope: &str, line_index: &LineIndex, out: &mut Vec<StructuralSmell>) {
    revisar_colas(scope, body, line_index, out);
    revisar_lanzamientos(scope, body, line_index, out);
    for stmt in body {
        let datos = match stmt {
            Stmt::FunctionDef(f) => Some((f.name.as_str(), &f.body, &f.decorator_list)),
            Stmt::AsyncFunctionDef(f) => Some((f.name.as_str(), &f.body, &f.decorator_list)),
            _ => None,
        };
        if let Some((nombre, cuerpo, decoradores)) = datos {
            revisar_cache_del_decorador(nombre, decoradores, line_index, out);
            recorrer(cuerpo, nombre, line_index, out);
        }
        if let Stmt::ClassDef(c) = stmt {
            recorrer(&c.body, scope, line_index, out);
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
        resource_smells(&suite, &idx)
    }

    fn tiene(src: &str, kind: &str) -> bool {
        hallazgos(src).iter().any(|s| s.kind == kind)
    }

    #[test]
    fn una_cola_sin_maxsize_se_marca() {
        assert!(tiene("import queue\n\n\ndef f():\n    q = queue.Queue()\n", "unbounded_queue"));
        assert!(tiene("def f():\n    q = asyncio.Queue()\n", "unbounded_queue"));
    }

    #[test]
    fn un_maxsize_cero_o_negativo_tambien_es_sin_tope() {
        assert!(tiene("def f():\n    q = queue.Queue(maxsize=0)\n", "unbounded_queue"));
        assert!(tiene("def f():\n    q = queue.Queue(0)\n", "unbounded_queue"));
        assert!(tiene("def f():\n    q = asyncio.Queue(maxsize=-1)\n", "unbounded_queue"));
    }

    #[test]
    fn el_mensaje_distingue_el_default_de_la_eleccion_explicita() {
        let implicito = hallazgos("def f():\n    q = queue.Queue()\n");
        assert!(implicito[0].message.contains("The default is 0"));
        let explicito = hallazgos("def f():\n    q = queue.Queue(maxsize=0)\n");
        assert!(explicito[0].message.contains("chosen explicitly"));
    }

    #[test]
    fn una_cola_acotada_no_produce_nada() {
        assert!(hallazgos("def f():\n    q = queue.Queue(maxsize=100)\n").is_empty());
        assert!(hallazgos("def f():\n    q = queue.Queue(1000)\n").is_empty());
    }

    #[test]
    fn un_tope_no_literal_es_un_tope() {
        // `maxsize=cfg.tam` es una cota: no se sabe cuanto vale y no se
        // afirma nada sobre ella.
        assert!(hallazgos("def f(cfg):\n    q = queue.Queue(maxsize=cfg.tam)\n").is_empty());
    }

    #[test]
    fn simple_queue_tiene_su_propio_mensaje() {
        let h = hallazgos("def f():\n    q = queue.SimpleQueue()\n");
        assert_eq!(h.len(), 1, "{h:?}");
        assert!(h[0].message.contains("a different class"), "{}", h[0].message);
    }

    #[test]
    fn un_threadpoolexecutor_sin_max_workers_no_es_un_pool_sin_cota() {
        // El default es min(32, cpu+4): acotado. Reportarlo seria falso, y es
        // justo lo que haria un detector ingenuo.
        assert!(hallazgos("def f():\n    p = ThreadPoolExecutor()\n").is_empty());
    }

    #[test]
    fn un_deque_sin_maxlen_no_se_reporta() {
        // Usar un deque como lista es lo normal, y nada en la forma lo
        // distingue de usarlo como buffer.
        assert!(hallazgos("def f():\n    d = collections.deque()\n").is_empty());
    }

    #[test]
    fn una_cache_sin_tope_se_marca() {
        assert!(tiene("@functools.cache\ndef f(x):\n    return x\n", "unbounded_cache"));
        assert!(tiene("@lru_cache(maxsize=None)\ndef f(x):\n    return x\n", "unbounded_cache"));
        assert!(tiene("@functools.lru_cache(None)\ndef f(x):\n    return x\n", "unbounded_cache"));
    }

    #[test]
    fn un_lru_cache_con_tope_no_se_reporta() {
        // Pelado ya es maxsize=128, o sea acotado.
        assert!(hallazgos("@lru_cache\ndef f(x):\n    return x\n").is_empty());
        assert!(hallazgos("@lru_cache(maxsize=256)\ndef f(x):\n    return x\n").is_empty());
    }

    #[test]
    fn lanzar_hilos_en_un_bucle_sin_cota_se_marca() {
        let src = "def f(items):\n    for it in items:\n        t = threading.Thread(target=g, args=(it,))\n        t.start()\n";
        assert!(tiene(src, "unbounded_task_spawn"));
    }

    #[test]
    fn un_join_adentro_del_bucle_lo_secuencializa() {
        // Nunca hay mas de un hilo vivo: reportarlo seria falso.
        let src = "def f(items):\n    for it in items:\n        t = threading.Thread(target=g)\n        t.start()\n        t.join()\n";
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src));
    }

    #[test]
    fn un_semaforo_a_la_vista_calla_el_analisis() {
        let src = "async def f(items, sem):\n    for it in items:\n        async with sem:\n            asyncio.create_task(g(it))\n    s = asyncio.Semaphore(5)\n";
        assert!(hallazgos(src).is_empty(), "{:?}", hallazgos(src));
    }

    #[test]
    fn un_lanzamiento_fuera_de_un_bucle_no_se_reporta() {
        assert!(hallazgos("def f():\n    t = threading.Thread(target=g)\n    t.start()\n").is_empty());
    }

    #[test]
    fn el_scope_es_la_funcion_que_contiene_la_cola() {
        let h = hallazgos("def arranque():\n    q = queue.Queue()\n");
        assert_eq!(h[0].name, "arranque");
    }

    #[test]
    fn una_cola_a_nivel_de_modulo_queda_en_module() {
        let h = hallazgos("q = queue.Queue()\n");
        assert_eq!(h[0].name, "<module>");
    }
}
