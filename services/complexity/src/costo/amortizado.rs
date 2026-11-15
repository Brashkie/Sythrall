//! **El costo real de una operación de contenedor, y la distinción
//! amortizado/peor-caso que la hace no trivial.**
//!
//! El motor de Big-O (`bigo.rs`) cuenta PROFUNDIDAD DE BUCLES y no mira qué
//! cuesta la operación de adentro. Medido contra el backend vivo antes de
//! escribir este módulo, con los tres casos dando el mismo veredicto:
//!
//! ```text
//! for x in items: out.append(x)        real O(n)    decía O(n)   correcto
//! for x in items: out = out + [x]      real O(n²)   decía O(n)   FALSO
//! for x in items: out.insert(0, x)     real O(n²)   decía O(n)   FALSO
//! ```
//!
//! **Y no se arregla diciendo "las operaciones de contenedor cuestan O(n)".**
//! `append` también es O(n) en su peor caso —cuando toca realocar y copiar—
//! pero es **O(1) amortizado**: el realloc duplica la capacidad, así que el
//! costo de copiar se reparte entre las n inserciones que vinieron antes.
//! Contarlo como O(n) convertiría el caso CORRECTO en un falso positivo. La
//! distinción amortizado/peor-caso no es un adorno teórico acá: es lo único
//! que separa los dos grupos.
//!
//! Lo que este módulo cuenta como costo oculto es **O(n) por llamada, de
//! verdad, no amortizable**:
//!
//! - `.insert(i, x)` — corre todos los elementos desde `i` una posición.
//! - `.pop(i)` con índice explícito — lo mismo, en la otra dirección. Sin
//!   argumento (`.pop()`) es O(1) y no cuenta.
//! - `.remove(x)` — busca y después corre.
//! - `x = x + …` — **construye una lista/cadena NUEVA y copia todo**.
//!
//! Y la exclusión que más importa, porque es la que casi nadie ve:
//! **`x += […]` NO es lo mismo que `x = x + […]`**. El primero es
//! `list.extend` en el lugar, amortizado O(k); el segundo copia entero, O(n).
//! Se leen casi igual y difieren asintóticamente. Hay un test para cada uno.

use rustpython_parser::ast::{Constant, Expr, Operator, Ranged, Stmt, UnaryOp};

use crate::ast::parser::LineIndex;
use crate::hallazgos::smells::StructuralSmell;

/// Un punto del cuerpo donde una operación cuesta O(n) por llamada.
#[derive(Debug, Clone)]
pub struct CostoOculto {
    /// Cómo se escribe, para que el mensaje cite lo que el autor tipeó.
    pub operacion: String,
    /// Offset de bytes, no línea: `bigo::full` necesita el NIVEL y no
    /// recibe un `LineIndex`. La conversión a línea la hace el único
    /// consumidor que la necesita, el que arma los hallazgos.
    pub offset: usize,
    /// Cuántos bucles lo envuelven.
    pub profundidad_bucle: u32,
    /// `profundidad_bucle + 1`: el nivel de `n` al que llega esta operación.
    /// Un `insert` suelto es O(n); dentro de un bucle es O(n²).
    pub nivel_efectivo: u32,
    /// Por qué cuesta O(n), en una línea, para el mensaje del hallazgo.
    pub motivo: &'static str,
}

/// Métodos cuyo costo es O(n) por llamada y **no** se amortiza.
const METODOS_LINEALES: &[(&str, &str)] = &[
    ("insert", "it shifts every element from that index onward"),
    ("remove", "it scans for the value and then shifts the rest"),
];

/// Lo que este módulo NO marca, escrito para que quede claro que es una
/// decisión y no un olvido. Todas son O(1) amortizado: el costo del realloc
/// se reparte entre las inserciones anteriores, así que una secuencia de n
/// de éstas cuesta O(n) en total, no O(n²).
const AMORTIZADAS: &[&str] = &["append", "add", "extend", "push", "update"];

/// ¿La expresión es la constante entera `valor`?
fn es_entero(e: &Expr, valor: i64) -> bool {
    matches!(e, Expr::Constant(c) if matches!(&c.value, Constant::Int(i) if i == &valor.into()))
}

/// `.pop()` sin argumento saca del final: O(1). `.pop(-1)` es lo mismo
/// escrito distinto. Con cualquier otro índice hay que correr la cola.
fn pop_es_lineal(c: &rustpython_parser::ast::ExprCall) -> bool {
    let Some(arg) = c.args.first() else {
        return false;
    };
    if es_entero(arg, -1) {
        return false;
    }
    // `-1` llega como `UnaryOp(USub, Constant(1))`, no como una constante
    // negativa — el mismo detalle que ya había mordido en `colas.rs`.
    if let Expr::UnaryOp(u) = arg {
        if matches!(u.op, UnaryOp::USub) && es_entero(&u.operand, 1) {
            return false;
        }
    }
    true
}

/// El nombre de la variable de una expresión, si es un nombre pelado.
fn nombre_pelado(e: &Expr) -> Option<&str> {
    match e {
        Expr::Name(n) => Some(n.id.as_str()),
        _ => None,
    }
}

/// `x = x + …` — reconstruye el contenedor entero en cada vuelta.
///
/// Exige que el nombre asignado sea **el mismo** que aparece a la izquierda
/// de la suma: `total = a + b` no acumula nada y no es esto. Cubre listas y
/// cadenas por igual, que es correcto — las dos copian.
fn reasignacion_acumuladora(stmt: &Stmt) -> Option<String> {
    let Stmt::Assign(a) = stmt else {
        return None;
    };
    let destino = nombre_pelado(a.targets.first()?)?;
    let Expr::BinOp(b) = &*a.value else {
        return None;
    };
    if !matches!(b.op, Operator::Add) {
        return None;
    }
    (nombre_pelado(&b.left)? == destino).then(|| format!("{destino} = {destino} + …"))
}

/// Recorre el cuerpo llevando la profundidad de bucles, y junta cada
/// operación de costo lineal con el nivel de `n` al que llega.
fn recolectar(body: &[Stmt]) -> Vec<CostoOculto> {
    fn walk(stmts: &[Stmt], depth: u32, out: &mut Vec<CostoOculto>) {
        for stmt in stmts {
            if let Some(operacion) = reasignacion_acumuladora(stmt) {
                out.push(CostoOculto {
                    operacion,
                    offset: stmt.range().start().to_usize(),
                    profundidad_bucle: depth,
                    nivel_efectivo: depth + 1,
                    motivo: "it builds a whole new object and copies every element into it",
                });
            }
            revisar_llamadas(stmt, depth, out);

            let (es_bucle, hijos, orelse): (bool, &[Stmt], &[Stmt]) = match stmt {
                Stmt::For(s) => (true, &s.body, &s.orelse),
                Stmt::AsyncFor(s) => (true, &s.body, &s.orelse),
                Stmt::While(s) => (true, &s.body, &s.orelse),
                Stmt::If(s) => (false, &s.body, &s.orelse),
                Stmt::With(s) => (false, &s.body, &[]),
                Stmt::AsyncWith(s) => (false, &s.body, &[]),
                Stmt::Try(s) => (false, &s.body, &s.orelse),
                // Una función anidada tiene su propio `n`: lo que pasa
                // adentro no multiplica el costo del bucle de afuera acá.
                Stmt::FunctionDef(_) | Stmt::AsyncFunctionDef(_) | Stmt::ClassDef(_) => (false, &[], &[]),
                _ => (false, &[], &[]),
            };
            let d = if es_bucle { depth + 1 } else { depth };
            walk(hijos, d, out);
            walk(orelse, d, out);
            if let Stmt::Try(s) = stmt {
                for h in &s.handlers {
                    let rustpython_parser::ast::ExceptHandler::ExceptHandler(h) = h;
                    walk(&h.body, d, out);
                }
                walk(&s.finalbody, d, out);
            }
        }
    }

    let mut out = Vec::new();
    walk(body, 0, &mut out);
    out
}

/// Las llamadas a método que aparecen en un statement (sin descender a los
/// cuerpos anidados — de eso se encarga `walk`).
fn revisar_llamadas(stmt: &Stmt, depth: u32, out: &mut Vec<CostoOculto>) {
    let mut visitar = |e: &Expr| {
        let Expr::Call(c) = e else {
            return;
        };
        let Expr::Attribute(attr) = &*c.func else {
            return;
        };
        let metodo = attr.attr.as_str();
        let receptor = nombre_pelado(&attr.value).unwrap_or("…");

        if AMORTIZADAS.contains(&metodo) {
            return;
        }
        let motivo = if metodo == "pop" {
            if !pop_es_lineal(c) {
                return;
            }
            "it shifts every element after that index"
        } else if let Some((_, m)) = METODOS_LINEALES.iter().find(|(n, _)| *n == metodo) {
            m
        } else {
            return;
        };

        out.push(CostoOculto {
            operacion: format!("{receptor}.{metodo}(…)"),
            offset: c.range().start().to_usize(),
            profundidad_bucle: depth,
            nivel_efectivo: depth + 1,
            motivo,
        });
    };
    for e in expresiones_del_statement(stmt) {
        recorrer_expr(e, &mut visitar);
    }
}

/// Las expresiones que cuelgan directamente de un statement.
fn expresiones_del_statement(stmt: &Stmt) -> Vec<&Expr> {
    match stmt {
        Stmt::Expr(s) => vec![&*s.value],
        Stmt::Assign(s) => vec![&*s.value],
        Stmt::AugAssign(s) => vec![&*s.value],
        Stmt::AnnAssign(s) => s.value.as_deref().into_iter().collect(),
        Stmt::Return(s) => s.value.as_deref().into_iter().collect(),
        Stmt::If(s) => vec![&*s.test],
        Stmt::While(s) => vec![&*s.test],
        Stmt::For(s) => vec![&*s.iter],
        _ => Vec::new(),
    }
}

fn recorrer_expr<'a>(e: &'a Expr, f: &mut impl FnMut(&'a Expr)) {
    f(e);
    match e {
        Expr::Call(c) => {
            recorrer_expr(&c.func, f);
            for a in &c.args {
                recorrer_expr(a, f);
            }
        }
        Expr::BinOp(b) => {
            recorrer_expr(&b.left, f);
            recorrer_expr(&b.right, f);
        }
        Expr::Attribute(a) => recorrer_expr(&a.value, f),
        Expr::Subscript(s) => {
            recorrer_expr(&s.value, f);
            recorrer_expr(&s.slice, f);
        }
        _ => {}
    }
}

/// El costo oculto que manda: el que llega al nivel de `n` más alto.
pub fn costo_dominante(body: &[Stmt]) -> Option<CostoOculto> {
    recolectar(body).into_iter().max_by_key(|c| c.nivel_efectivo)
}

/// Hallazgos para el panel. Solo los que están **dentro** de un bucle: un
/// `insert` suelto es O(n) y eso ya lo dice el Big-O de la función; lo que
/// merece un hallazgo aparte es el que multiplica.
pub fn check_linear_op_in_loop(scope: &str, line_index: &LineIndex, body: &[Stmt]) -> Vec<StructuralSmell> {
    recolectar(body)
        .into_iter()
        .filter(|c| c.profundidad_bucle > 0)
        .map(|c| StructuralSmell {
            kind: "linear_op_in_loop",
            name: scope.to_string(),
            line: line_index.line_of(c.offset),
            message: format!(
                "`{}` costs O(n) per call — {}. Inside {} loop{}, the real cost is O(n{}), not the O(n{}) that counting loops alone would give. This is NOT the same as `append`/`extend`/`add`, which are O(1) amortized: their reallocation cost is spread across the insertions that came before, so n of them cost O(n) in total. The distinction is what separates a correct accumulator from a quadratic one, and the two are often one character apart — `x += [y]` extends in place, `x = x + [y]` copies the whole thing.",
                c.operacion,
                c.motivo,
                c.profundidad_bucle,
                if c.profundidad_bucle == 1 { "" } else { "s" },
                superindice(c.nivel_efectivo),
                superindice(c.profundidad_bucle),
            ),
        })
        .collect()
}

fn superindice(n: u32) -> String {
    match n {
        1 => String::new(),
        2 => "²".into(),
        3 => "³".into(),
        otro => format!("^{otro}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::LineIndex;

    fn cuerpo(src: &str) -> (Vec<Stmt>, LineIndex) {
        let suite = crate::ast::parser::parse_module(src).expect("parsea");
        (suite, LineIndex::build(src))
    }

    fn niveles(src: &str) -> Vec<(String, u32)> {
        let (suite, _) = cuerpo(src);
        recolectar(&suite).into_iter().map(|c| (c.operacion, c.nivel_efectivo)).collect()
    }

    #[test]
    fn append_en_un_bucle_no_es_costo_oculto() {
        // El caso que hay que NO marcar: O(1) amortizado. Marcarlo
        // convertiría el acumulador correcto en un falso positivo.
        assert!(niveles("for x in items:\n    out.append(x)\n").is_empty());
    }

    #[test]
    fn extend_y_add_tampoco() {
        assert!(niveles("for x in items:\n    out.extend(x)\n    s.add(x)\n").is_empty());
    }

    #[test]
    fn insert_en_un_bucle_llega_a_n_cuadrado() {
        let n = niveles("for x in items:\n    out.insert(0, x)\n");
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].1, 2, "un bucle + una operación O(n) = O(n²)");
    }

    #[test]
    fn insert_suelto_es_lineal_no_cuadratico() {
        let n = niveles("out.insert(0, x)\n");
        assert_eq!(n[0].1, 1, "sin bucle alrededor, un insert es O(n)");
    }

    #[test]
    fn la_reasignacion_que_copia_se_detecta() {
        let n = niveles("for x in items:\n    out = out + [x]\n");
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].0, "out = out + …");
        assert_eq!(n[0].1, 2);
    }

    #[test]
    fn pero_el_mas_igual_no_copia_y_no_se_marca() {
        // `x += [y]` es `list.extend` en el lugar: amortizado O(k). Se lee
        // casi igual que el test de arriba y difiere asintóticamente — es la
        // razón entera por la que este módulo distingue en vez de generalizar.
        assert!(niveles("for x in items:\n    out += [x]\n").is_empty());
    }

    #[test]
    fn una_suma_que_no_acumula_no_cuenta() {
        // `total = a + b` no reconstruye nada sobre sí mismo.
        assert!(niveles("for x in items:\n    total = a + b\n").is_empty());
    }

    #[test]
    fn pop_del_final_es_barato_y_pop_por_indice_no() {
        assert!(niveles("for x in items:\n    out.pop()\n").is_empty());
        assert!(niveles("for x in items:\n    out.pop(-1)\n").is_empty());
        assert_eq!(niveles("for x in items:\n    out.pop(0)\n").len(), 1);
    }

    #[test]
    fn dos_bucles_anidados_con_un_insert_llegan_a_n_cubo() {
        let n = niveles("for a in xs:\n    for b in ys:\n        out.insert(0, b)\n");
        assert_eq!(n[0].1, 3);
    }

    #[test]
    fn una_funcion_anidada_no_multiplica_el_bucle_de_afuera() {
        // El `n` de `g` no es el `n` del bucle de `f`: un `insert` adentro de
        // `g` no corre una vez por vuelta del bucle, corre cuando alguien
        // llame a `g`. Y no hace falta reportarlo desde acá, porque
        // `rich.rs::collect_functions` corre los chequeos de smells UNA VEZ
        // POR FUNCIÓN — `g` recibe su propia pasada. Descender acá lo
        // contaría dos veces, y con el nivel equivocado en una de las dos.
        assert!(niveles("for x in items:\n    def g():\n        out.insert(0, x)\n").is_empty());
    }

    #[test]
    fn el_dominante_es_el_de_nivel_mas_alto() {
        let (suite, _) = cuerpo("out.insert(0, x)\nfor x in items:\n    out.remove(x)\n");
        let c = costo_dominante(&suite).expect("hay uno");
        assert_eq!(c.nivel_efectivo, 2);
    }

    #[test]
    fn el_hallazgo_solo_sale_para_lo_que_esta_dentro_de_un_bucle() {
        let (suite, idx) = cuerpo("out.insert(0, x)\n");
        assert!(check_linear_op_in_loop("f", &idx, &suite).is_empty());
        let (suite, idx) = cuerpo("for x in items:\n    out.insert(0, x)\n");
        let s = check_linear_op_in_loop("f", &idx, &suite);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].kind, "linear_op_in_loop");
        assert!(s[0].message.contains("O(n²)"), "el mensaje dice el costo real: {}", s[0].message);
        assert!(s[0].message.contains("amortized"), "y por qué `append` no cuenta");
    }
}
