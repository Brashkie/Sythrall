//! Fase 15 (Mathematical Intelligence) — precondiciones de control óptimo.
//!
//! Nace de una conversación sobre un paper de Moving Target Defense en
//! sistemas de vuelo (LQR + Riccati + selección softmax por entropía). La
//! conclusión de esa charla fue que Sythrall **analiza código fuente, no
//! modelos de planta**: un motor de teoría de control que recibe matrices A/B
//! sería otro producto. Lo que sí es de Sythrall es lo que este módulo hace —
//! mirar el CÓDIGO que llama a un solver de control y chequear las
//! precondiciones matemáticas que ese código asume en silencio.
//!
//! `scipy.linalg.solve_continuous_are(A, B, Q, R)` y `control.lqr(A, B, Q, R)`
//! exigen, para que el resultado signifique algo:
//!   - dimensiones compatibles: `A` es n×n, `B` es n×m, `Q` es n×n, `R` es m×m;
//!   - `Q` y `R` simétricas;
//!   - `R` definida positiva (aparece invertida en `R⁻¹BᵀP`);
//!   - `Q` semidefinida positiva.
//!
//! Ninguna de las dos librerías valida todo esto: con una `R` singular o mal
//! dimensionada devuelven basura o una excepción críptica, y el bug se
//! descubre volando, no compilando.
//!
//! **Qué se puede probar y qué no, que es la parte importante.** Cuando las
//! matrices son literales en el mismo scope, la incompatibilidad de
//! dimensiones y la falta de simetría son **demostrables**: se ven, no se
//! estiman. Confianza `High`. La definición positiva NO se decide con
//! literales sin calcular autovalores — pero su condición NECESARIA sí (una
//! matriz definida positiva no puede tener un elemento ≤ 0 en la diagonal, ni
//! semidefinida positiva uno < 0). Violar una condición necesaria **prueba**
//! que la matriz no es PD/PSD, así que ese hallazgo también es `High`;
//! cumplirla no prueba nada, y por eso no existe un hallazgo "R está bien".
//! Ese es exactamente el límite: este módulo reporta violaciones probadas,
//! nunca certifica correctitud.
//!
//! **La otra mitad del paper: la selección de Gibbs.** ADMIRE no solo
//! resuelve un LQR — después elige la configuración siguiente con
//! `p_i ∝ exp(-V_i/ε)`, y sube ε para ganar imprevisibilidad. Ahí también hay
//! precondiciones que el código asume sin decirlo: una ε que llega en 0 o
//! negativa, un softmax escrito a mano que desborda, un vector de
//! probabilidades que NumPy va a rechazar. Los checks están más abajo, en su
//! propia sección.
//!
//! Lo que este módulo **no** hace, y conviene decirlo porque es la parte que
//! más fácil se malinterpreta del paper: no opina sobre si subir ε mejora la
//! resiliencia. Subir ε aumenta la entropía de la distribución — eso es
//! aritmética, no un resultado. Que más entropía implique más resiliencia
//! frente a un atacante real es una hipótesis sobre el atacante, no algo que
//! se lea en el código ni que se siga de la fórmula. Sythrall reporta
//! condiciones violadas, no calidad de una política.
//!
//! Alcance deliberado: solo matrices escritas como literal en el mismo scope
//! (lista anidada, con o sin `np.array(...)`, más `np.eye`/`np.identity`/
//! `np.diag`/`np.zeros`). Si `A` viene de un archivo, de otra función o de un
//! parámetro, no hay nada que chequear y no se inventa nada.

use std::collections::{HashMap, HashSet};

use rustpython_parser::ast::{Constant, Expr, Operator, Ranged, Stmt, UnaryOp};
use serde::Serialize;

use crate::ast::parser::LineIndex;
use crate::ast::structure::{last_segment, node_name};
use crate::ast::walk::{walk_expr_tree, walk_stmts_own_scope};

#[derive(Serialize, Clone)]
pub struct ControlFinding {
    pub kind: &'static str,
    /// Función que contiene la llamada, o `<module>`.
    pub function: String,
    pub line: usize,
    pub severity: &'static str,
    pub confidence: &'static str,
    pub message: String,
}

/// Solvers que reciben `(A, B, Q, R)` en ese orden posicional. `lqr` de la
/// librería `control` y las 2 ecuaciones algebraicas de Riccati de scipy
/// comparten firma en sus 4 primeros argumentos, que es lo único que este
/// módulo mira.
const ARE_SOLVERS: &[&str] = &[
    "solve_continuous_are",
    "solve_discrete_are",
    "scipy.linalg.solve_continuous_are",
    "scipy.linalg.solve_discrete_are",
    "linalg.solve_continuous_are",
    "linalg.solve_discrete_are",
    "lqr",
    "control.lqr",
    "ct.lqr",
    "dlqr",
    "control.dlqr",
];

/// Una matriz cuyo contenido completo se pudo determinar leyendo el código.
#[derive(Clone)]
struct MatrixLiteral {
    rows: usize,
    cols: usize,
    values: Vec<Vec<f64>>,
}

impl MatrixLiteral {
    fn is_symmetric(&self) -> bool {
        if self.rows != self.cols {
            return false;
        }
        (0..self.rows).all(|i| (0..i).all(|j| (self.values[i][j] - self.values[j][i]).abs() < 1e-12))
    }

    /// Índice del primer elemento de la diagonal que viola `> 0` (para PD) o
    /// `>= 0` (para PSD). Es la condición NECESARIA — ver el doc del módulo.
    fn bad_diagonal(&self, strict: bool) -> Option<(usize, f64)> {
        (0..self.rows.min(self.cols))
            .map(|i| (i, self.values[i][i]))
            .find(|(_, v)| if strict { *v <= 0.0 } else { *v < 0.0 })
    }
}

/// Número literal, incluyendo el negativo (`-1` es `UnaryOp(USub, ...)` en el
/// AST, no una constante negativa — olvidarlo haría que ninguna matriz con un
/// valor negativo se pudiera leer, que es justo el caso que más importa para
/// detectar una `Q` no PSD).
fn number_of(e: &Expr) -> Option<f64> {
    match e {
        Expr::Constant(c) => match &c.value {
            Constant::Int(i) => i.to_string().parse::<f64>().ok(),
            Constant::Float(f) => Some(*f),
            Constant::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        },
        Expr::UnaryOp(u) if matches!(u.op, UnaryOp::USub) => number_of(&u.operand).map(|v| -v),
        Expr::UnaryOp(u) if matches!(u.op, UnaryOp::UAdd) => number_of(&u.operand),
        _ => None,
    }
}

fn number_list(e: &Expr) -> Option<Vec<f64>> {
    let items = match e {
        Expr::List(l) => &l.elts,
        Expr::Tuple(t) => &t.elts,
        _ => return None,
    };
    items.iter().map(number_of).collect()
}

/// Extrae una matriz de las formas que se escriben de verdad en código de
/// control. Devuelve `None` cuando el valor no se puede determinar leyendo —
/// que es la mayoría de los casos reales, y está bien: sin certeza no hay
/// hallazgo.
fn matrix_from(expr: &Expr) -> Option<MatrixLiteral> {
    match expr {
        // [[1, 2], [3, 4]] — filas como listas de números.
        Expr::List(l) => {
            let rows: Option<Vec<Vec<f64>>> = l.elts.iter().map(number_list).collect();
            let rows = rows?;
            if rows.is_empty() {
                return None;
            }
            let cols = rows[0].len();
            if cols == 0 || rows.iter().any(|r| r.len() != cols) {
                return None; // filas de largo distinto: no es una matriz
            }
            Some(MatrixLiteral { rows: rows.len(), cols, values: rows })
        }
        Expr::Call(c) => {
            let name = node_name(&c.func);
            let last = last_segment(&name);
            match last {
                // np.array([[...]]) / np.asarray(...) / np.matrix(...)
                "array" | "asarray" | "matrix" => c.args.first().and_then(matrix_from),
                // np.eye(n) / np.identity(n)
                "eye" | "identity" => {
                    let n = number_of(c.args.first()?)? as usize;
                    if n == 0 {
                        return None;
                    }
                    let values = (0..n).map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect()).collect();
                    Some(MatrixLiteral { rows: n, cols: n, values })
                }
                // np.diag([a, b, c])
                "diag" => {
                    let d = number_list(c.args.first()?)?;
                    let n = d.len();
                    if n == 0 {
                        return None;
                    }
                    let values = (0..n).map(|i| (0..n).map(|j| if i == j { d[i] } else { 0.0 }).collect()).collect();
                    Some(MatrixLiteral { rows: n, cols: n, values })
                }
                // np.zeros((n, m)) — una R de ceros nunca es definida positiva,
                // así que vale la pena poder leerla.
                "zeros" => {
                    let shape = number_list(c.args.first()?)?;
                    let (n, m) = match shape.len() {
                        1 => (shape[0] as usize, shape[0] as usize),
                        2 => (shape[0] as usize, shape[1] as usize),
                        _ => return None,
                    };
                    if n == 0 || m == 0 {
                        return None;
                    }
                    Some(MatrixLiteral { rows: n, cols: m, values: vec![vec![0.0; m]; n] })
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Matrices asignadas a un nombre en ESTE scope. `walk_stmts_own_scope` para
/// no arrastrar una `A` de una función anidada como si fuera la de acá —
/// mismo criterio de scope que el taint tracking de `security.rs`.
fn literal_matrices(body: &[Stmt]) -> HashMap<String, MatrixLiteral> {
    let mut out = HashMap::new();
    let mut on_stmt = |stmt: &Stmt| {
        if let Stmt::Assign(a) = stmt {
            if let Some(m) = matrix_from(&a.value) {
                for t in &a.targets {
                    if let Expr::Name(n) = t {
                        out.insert(n.id.to_string(), m.clone());
                    }
                }
            }
        }
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
    out
}

/// Resuelve un argumento a una matriz conocida: o es un literal escrito ahí
/// mismo, o el nombre de una variable asignada con un literal en este scope.
fn resolve(arg: &Expr, known: &HashMap<String, MatrixLiteral>) -> Option<(String, MatrixLiteral)> {
    if let Expr::Name(n) = arg {
        return known.get(n.id.as_str()).map(|m| (n.id.to_string(), m.clone()));
    }
    matrix_from(arg).map(|m| ("<literal>".to_string(), m))
}

fn dim_finding(scope: &str, line: usize, solver: &str, message: String) -> ControlFinding {
    ControlFinding {
        kind: "lqr_dimension_mismatch",
        function: scope.to_string(),
        line,
        severity: "High",
        confidence: "High",
        message: format!("`{solver}(A, B, Q, R)`: {message}"),
    }
}

/// Checks sobre una llamada concreta a un solver, con las matrices ya
/// resueltas. Devuelve todos los hallazgos que se puedan probar.
fn check_call(scope: &str, line: usize, solver: &str, args: &[Option<(String, MatrixLiteral)>]) -> Vec<ControlFinding> {
    let mut out = Vec::new();
    let (a, b, q, r) = (args.first(), args.get(1), args.get(2), args.get(3));

    // ── Dimensiones ──
    let n = a.and_then(|x| x.as_ref()).map(|(_, m)| {
        if m.rows != m.cols {
            out.push(dim_finding(scope, line, solver, format!("`A` es {}×{} — la matriz de estado tiene que ser cuadrada.", m.rows, m.cols)));
        }
        m.rows
    });
    let m_inputs = b.and_then(|x| x.as_ref()).map(|(_, mat)| {
        if let Some(n) = n {
            if mat.rows != n {
                out.push(dim_finding(
                    scope,
                    line,
                    solver,
                    format!("`B` tiene {} filas pero `A` es {n}×{n} — `B` tiene que ser {n}×m.", mat.rows),
                ));
            }
        }
        mat.cols
    });
    if let (Some(n), Some(Some((name, mat)))) = (n, q) {
        if mat.rows != n || mat.cols != n {
            out.push(dim_finding(
                scope,
                line,
                solver,
                format!("`{name}` (Q) es {}×{} pero tiene que ser {n}×{n}, igual que `A`.", mat.rows, mat.cols),
            ));
        }
    }
    if let (Some(m), Some(Some((name, mat)))) = (m_inputs, r) {
        if mat.rows != m || mat.cols != m {
            out.push(dim_finding(
                scope,
                line,
                solver,
                format!("`{name}` (R) es {}×{} pero tiene que ser {m}×{m}, una por cada columna de `B`.", mat.rows, mat.cols),
            ));
        }
    }

    // ── Simetría y definición positiva ──
    for (idx, label, strict) in [(2usize, "Q", false), (3usize, "R", true)] {
        let Some(Some((name, mat))) = args.get(idx) else { continue };
        if mat.rows != mat.cols {
            continue; // ya reportado como problema de dimensión
        }
        if !mat.is_symmetric() {
            out.push(ControlFinding {
                kind: "lqr_non_symmetric_weight",
                function: scope.to_string(),
                line,
                severity: "Medium",
                confidence: "High",
                message: format!(
                    "`{name}` ({label}) no es simétrica. El costo cuadrático `xᵀ{label}x` solo usa la parte simétrica de la matriz, así que la parte antisimétrica se descarta en silencio: el peso que se aplica no es el que está escrito."
                ),
            });
        }
        if let Some((i, v)) = mat.bad_diagonal(strict) {
            let (req, why) = if strict {
                ("definida positiva", "aparece invertida en `R⁻¹BᵀP`, así que con un elemento no positivo en la diagonal el solver devuelve basura o falla")
            } else {
                ("semidefinida positiva", "un peso negativo premia el error de estado en vez de penalizarlo, y el problema deja de tener una solución estabilizante")
            };
            out.push(ControlFinding {
                kind: "lqr_weight_not_positive_definite",
                function: scope.to_string(),
                line,
                severity: "High",
                confidence: "High",
                message: format!(
                    "`{name}` ({label}) tiene {v} en la posición ({i},{i}) de su diagonal, así que NO puede ser {req} — {why}. (La diagonal positiva es condición necesaria, no suficiente: que se cumpla no prueba que la matriz lo sea.)"
                ),
            });
        }
    }
    out
}

/// Punto de entrada. `scope` es el nombre de la función que contiene el
/// cuerpo, o `<module>` para el nivel superior — misma convención que
/// `security.rs` y `smells.rs`.
pub fn control_findings(scope: &str, body: &[Stmt], line_index: &LineIndex) -> Vec<ControlFinding> {
    let known = literal_matrices(body);
    let mut findings = Vec::new();
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |e: &Expr| {
        let Expr::Call(call) = e else { return };
        let dotted = node_name(&call.func);
        if !ARE_SOLVERS.contains(&dotted.as_str()) {
            return;
        }
        // Menos de 4 argumentos posicionales: puede ser una firma distinta o
        // uso de keywords, que este módulo no resuelve. No se inventa nada.
        if call.args.len() < 4 {
            return;
        }
        let solver = last_segment(&dotted).to_string();
        let resolved: Vec<Option<(String, MatrixLiteral)>> =
            call.args.iter().take(4).map(|a| resolve(a, &known)).collect();
        if resolved.iter().all(|r| r.is_none()) {
            return; // ninguna matriz legible: nada que chequear
        }
        let line = line_index.line_of(call.range.start().to_usize());
        findings.extend(check_call(scope, line, &solver, &resolved));
    };
    walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
    findings.extend(gibbs_findings(scope, body, line_index));
    findings.sort_by_key(|f| f.line);
    findings
}

// ─────────────── Selección de Gibbs / softmax (la otra mitad del paper) ───
//
// El modelo ADMIRE elige la próxima configuración con una distribución de
// Gibbs `p_i ∝ exp(-V_i/ε)` — el minimizador exacto de `Σp_iV_i - εH(p)`. En
// código eso es un softmax escrito a mano sobre un vector de costos, y ahí
// hay cosas que se pueden PROBAR leyendo, y una que no.
//
// Lo que NO se chequea, a propósito: el signo del exponente. `exp(+V/ε)`
// está mal si `V` es un costo y bien si es una recompensa, y el código no
// dice cuál de las dos cosas es. Adivinarlo sería inventar.


/// Argumento de una llamada a `exp` (`np.exp`, `math.exp`, `torch.exp`, …).
fn exp_arg(e: &Expr) -> Option<&Expr> {
    let Expr::Call(c) = e else { return None };
    if last_segment(&node_name(&c.func)) != "exp" {
        return None;
    }
    c.args.first()
}

fn is_call_named(e: &Expr, names: &[&str]) -> bool {
    let Expr::Call(c) = e else { return false };
    names.contains(&last_segment(&node_name(&c.func)))
}

fn contains<F: Fn(&Expr) -> bool>(e: &Expr, pred: F) -> bool {
    let mut hit = false;
    walk_expr_tree(e, &mut |x: &Expr| {
        if !hit && pred(x) {
            hit = true;
        }
    });
    hit
}

/// `exp(v - v.max())` y `exp(v - logsumexp(v))` son las dos formas estándar
/// de estabilizar un softmax. Se exige una RESTA cuyo lado derecho contenga
/// una de las dos, no la mera presencia de un `max` en cualquier parte del
/// argumento — eso también matchearía un `max` que no tiene nada que ver y
/// haría desaparecer el hallazgo justo en el código que sí está mal.
fn is_stabilized(arg: &Expr, stabilized_names: &HashSet<String>) -> bool {
    contains(arg, |e| {
        matches!(e, Expr::BinOp(b) if matches!(b.op, Operator::Sub)
            && contains(&b.right, |r| is_call_named(r, &["max", "amax", "maximum", "logsumexp"])))
            // `z = v - v.max()` y despues `exp(z / eps)`: el corrimiento está
            // hecho, solo que una línea más arriba. Sin esto, la forma en dos
            // pasos — la más legible, y por eso la más común — daría un falso
            // positivo sobre código que hizo justo lo que hay que hacer.
            || matches!(e, Expr::Name(n) if stabilized_names.contains(n.id.as_str()))
    })
}

/// Temperatura de una forma de Gibbs `exp(±V/ε)`: la expresión del
/// denominador. Solo la forma con división: `exp(-β·V)` usa la temperatura
/// INVERSA, y ahí `β = 0` es el límite uniforme — legítimo, no un error.
fn gibbs_temperature(arg: &Expr) -> Option<&Expr> {
    let inner = match arg {
        Expr::UnaryOp(u) if matches!(u.op, UnaryOp::USub | UnaryOp::UAdd) => &*u.operand,
        other => other,
    };
    match inner {
        Expr::BinOp(b) if matches!(b.op, Operator::Div) => Some(&b.right),
        _ => None,
    }
}

/// Vector literal — `[0.5, 0.5]`, con o sin `np.array(...)` encima.
fn vector_from(e: &Expr) -> Option<Vec<f64>> {
    match e {
        Expr::List(_) | Expr::Tuple(_) => number_list(e),
        Expr::Call(c) => match last_segment(&node_name(&c.func)) {
            "array" | "asarray" => c.args.first().and_then(vector_from),
            _ => None,
        },
        _ => None,
    }
}

/// Nombres asignados a un valor literal en este scope. Un nombre que en
/// ALGÚN punto recibe algo ilegible se borra del mapa: si `eps` es `0.1` en
/// una rama y `f(t)` en otra, no se sabe cuál llega a la llamada, y un
/// hallazgo sobre el valor viejo sería un falso positivo.
fn literal_bindings<T, F>(body: &[Stmt], read: F) -> HashMap<String, T>
where
    F: Fn(&Expr) -> Option<T>,
{
    let mut out: HashMap<String, T> = HashMap::new();
    let mut dead: Vec<String> = Vec::new();
    let mut on_stmt = |stmt: &Stmt| {
        let Stmt::Assign(a) = stmt else { return };
        for t in &a.targets {
            let Expr::Name(n) = t else { continue };
            match read(&a.value) {
                Some(v) => {
                    out.insert(n.id.to_string(), v);
                }
                None => dead.push(n.id.to_string()),
            }
        }
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
    for name in dead {
        out.remove(&name);
    }
    out
}

fn gibbs_finding(
    kind: &'static str,
    scope: &str,
    line: usize,
    severity: &'static str,
    message: String,
) -> ControlFinding {
    ControlFinding { kind, function: scope.to_string(), line, severity, confidence: "High", message }
}

/// Probabilidades pasadas a `np.random.choice` — el paso de conmutación
/// aleatoria del MTD. NumPy valida esto en runtime y levanta `ValueError`;
/// cuando el vector es literal, el error se ve sin correr nada.
fn choice_probabilities(
    call: &rustpython_parser::ast::ExprCall,
    known: &HashMap<String, Vec<f64>>,
) -> Option<(String, Vec<f64>)> {
    if last_segment(&node_name(&call.func)) != "choice" {
        return None;
    }
    let p = call
        .keywords
        .iter()
        .find(|kw| kw.arg.as_ref().map(|a| a.as_str()) == Some("p"))
        .map(|kw| &kw.value)
        // `choice(a, size, replace, p)` — la única firma con 4 posicionales es
        // la de NumPy; la de la stdlib toma un solo argumento.
        .or_else(|| call.args.get(3))?;
    if let Expr::Name(n) = p {
        return known.get(n.id.as_str()).map(|v| (n.id.to_string(), v.clone()));
    }
    vector_from(p).map(|v| ("<literal>".to_string(), v))
}

/// Checks sobre la selección aleatoria: el softmax escrito a mano y el vector
/// de probabilidades que termina en `np.random.choice`.
fn gibbs_findings(scope: &str, body: &[Stmt], line_index: &LineIndex) -> Vec<ControlFinding> {
    let scalars: HashMap<String, f64> = literal_bindings(body, number_of);
    let vectors: HashMap<String, Vec<f64>> = literal_bindings(body, vector_from);

    let mut out = Vec::new();
    // línea del `exp` → ¿está estabilizado?
    let mut exp_sites: HashMap<usize, bool> = HashMap::new();
    // nombre ← `w = np.exp(...)`, para reconocer el softmax escrito en 2 pasos
    let mut exp_by_name: HashMap<String, usize> = HashMap::new();
    // líneas de `exp` que además se dividen por una suma: eso es un softmax
    let mut normalized: Vec<usize> = Vec::new();

    let line_of = |e: &Expr| line_index.line_of(e.range().start().to_usize());

    // Dos pasadas, no una: `on_stmt` y `on_expr` del mismo walker no pueden
    // compartir estado mutable (dos préstamos mutables vivos a la vez). La
    // primera resuelve qué nombre guarda un `exp`, que es lo que la segunda
    // necesita para reconocer el softmax escrito en dos pasos.
    let mut normalized_in_place: Vec<usize> = Vec::new();
    let mut stabilized_names: HashSet<String> = HashSet::new();
    {
        let empty = HashSet::new();
        let mut on_stmt = |stmt: &Stmt| match stmt {
            Stmt::Assign(a) => {
                if is_stabilized(&a.value, &empty) {
                    for t in &a.targets {
                        if let Expr::Name(n) = t {
                            stabilized_names.insert(n.id.to_string());
                        }
                    }
                }
                if exp_arg(&a.value).is_some() {
                    let line = line_of(&a.value);
                    for t in &a.targets {
                        if let Expr::Name(n) = t {
                            exp_by_name.insert(n.id.to_string(), line);
                        }
                    }
                }
            }
            // `p /= p.sum()` normaliza in-place: sin esto, el idioma más corto
            // de softmax en dos pasos se escaparía entero.
            Stmt::AugAssign(a) if matches!(a.op, Operator::Div) => {
                if let Expr::Name(n) = &*a.target {
                    if let Some(&line) = exp_by_name.get(n.id.as_str()) {
                        if contains(&a.value, |e| is_call_named(e, &["sum", "nansum"])) {
                            normalized_in_place.push(line);
                        }
                    }
                }
            }
            _ => {}
        };
        let mut on_expr = |_: &Expr| {};
        walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
    }
    normalized.extend(normalized_in_place);

    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |e: &Expr| {
        if let Some(arg) = exp_arg(e) {
            let line = line_of(e);
            let stab = is_stabilized(arg, &stabilized_names);
            exp_sites.entry(line).and_modify(|s| *s = *s && stab).or_insert(stab);

            // ── Temperatura no positiva ──
            if let Some(temp) = gibbs_temperature(arg) {
                let value = number_of(temp).or_else(|| match temp {
                    Expr::Name(n) => scalars.get(n.id.as_str()).copied(),
                    _ => None,
                });
                match value {
                    Some(v) if v == 0.0 => out.push(gibbs_finding(
                        "gibbs_nonpositive_temperature",
                        scope,
                        line,
                        "High",
                        format!(
                            "`exp(… / ε)` con ε = {v}. Con floats de Python eso es `ZeroDivisionError`; con arrays de NumPy no hay excepción — da `inf`/`nan` con un `RuntimeWarning`, la distribución queda toda `nan` y la selección pasa a ser arbitraria en silencio."
                        ),
                    )),
                    Some(v) if v < 0.0 => out.push(gibbs_finding(
                        "gibbs_nonpositive_temperature",
                        scope,
                        line,
                        "High",
                        format!(
                            "`exp(… / ε)` con ε = {v}, negativa. Eso invierte el signo del exponente: la distribución de Gibbs deja de concentrarse en el costo mínimo y se concentra en el máximo. No falla — sigue devolviendo probabilidades válidas que suman 1, y elige sistemáticamente lo peor."
                        ),
                    )),
                    _ => {}
                }
            }
        }
        // ── Normalización: `<algo con exp> / <alguna suma>` ──
        if let Expr::BinOp(b) = e {
            if matches!(b.op, Operator::Div) && contains(&b.right, |x| is_call_named(x, &["sum", "nansum"])) {
                let mut hits: Vec<usize> = Vec::new();
                walk_expr_tree(&b.left, &mut |x: &Expr| {
                    if exp_arg(x).is_some() {
                        hits.push(line_of(x));
                    }
                });
                normalized.extend(hits);
                if let Expr::Name(n) = &*b.left {
                    if let Some(&line) = exp_by_name.get(n.id.as_str()) {
                        normalized.push(line);
                    }
                }
            }
        }
        // ── Probabilidades imposibles en la conmutación aleatoria ──
        if let Expr::Call(call) = e {
            if let Some((name, p)) = choice_probabilities(call, &vectors) {
                let line = line_of(e);
                if let Some((i, v)) = p.iter().enumerate().find(|(_, v)| **v < 0.0) {
                    out.push(gibbs_finding(
                        "random_choice_bad_probabilities",
                        scope,
                        line,
                        "High",
                        format!(
                            "`{name}` tiene {v} en la posición {i}: `numpy.random.choice` levanta `ValueError: probabilities are not non-negative`."
                        ),
                    ));
                } else {
                    let total: f64 = p.iter().sum();
                    if (total - 1.0).abs() > 1e-6 {
                        out.push(gibbs_finding(
                            "random_choice_bad_probabilities",
                            scope,
                            line,
                            "High",
                            format!(
                                "`{name}` suma {total:.6}, no 1: `numpy.random.choice` levanta `ValueError: probabilities do not sum to 1`. (Acá se tolera 1e-6, más holgado que el ~1.5e-8 real de NumPy, para no reportar una diferencia que NumPy sí acepta.)"
                            ),
                        ));
                    }
                }
            }
        }
    };

    walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);

    // ── Softmax escrito a mano sin estabilizar ──
    normalized.sort_unstable();
    normalized.dedup();
    for line in normalized {
        if exp_sites.get(&line) == Some(&false) {
            out.push(gibbs_finding("softmax_unstable", scope, line, "Medium", String::from(
                "Softmax escrito a mano sin restar el máximo. Lo que está probado acá es la AUSENCIA del corrimiento (`x - x.max()` o `x - logsumexp(x)`), no que vaya a desbordar: eso depende de las magnitudes en runtime — `np.exp` pasa a `inf` en float64 arriba de ~709.78, y después `inf/inf` es `nan`. Con costos grandes o una ε chica se llega ahí rápido, porque el exponente es `V/ε`. `scipy.special.softmax` ya hace el corrimiento.",
            )));
        }
    }
    out
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::parse_module;

    fn findings(src: &str) -> Vec<ControlFinding> {
        let suite = parse_module(src).expect("parse error");
        let idx = LineIndex::build(src);
        control_findings("<module>", &suite, &idx)
    }

    const OK: &str = "import numpy as np\nfrom scipy.linalg import solve_continuous_are\nA = [[0, 1], [0, 0]]\nB = [[0], [1]]\nQ = np.eye(2)\nR = [[1]]\nP = solve_continuous_are(A, B, Q, R)\n";

    #[test]
    fn caso_correcto_no_produce_hallazgos() {
        assert!(findings(OK).is_empty());
    }

    #[test]
    fn a_no_cuadrada_se_detecta() {
        let src = "from scipy.linalg import solve_continuous_are\nA = [[0, 1, 0], [0, 0, 1]]\nB = [[0], [1]]\nQ = [[1, 0], [0, 1]]\nR = [[1]]\nsolve_continuous_are(A, B, Q, R)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.kind == "lqr_dimension_mismatch" && x.message.contains("cuadrada")));
    }

    #[test]
    fn b_con_filas_incompatibles_con_a_se_detecta() {
        let src = "from scipy.linalg import solve_continuous_are\nA = [[0, 1], [0, 0]]\nB = [[0], [1], [2]]\nQ = [[1, 0], [0, 1]]\nR = [[1]]\nsolve_continuous_are(A, B, Q, R)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.kind == "lqr_dimension_mismatch" && x.message.contains("`B` tiene 3 filas")));
    }

    #[test]
    fn q_con_dimension_distinta_a_a_se_detecta() {
        let src = "from scipy.linalg import solve_continuous_are\nA = [[0, 1], [0, 0]]\nB = [[0], [1]]\nQ = [[1, 0, 0], [0, 1, 0], [0, 0, 1]]\nR = [[1]]\nsolve_continuous_are(A, B, Q, R)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.kind == "lqr_dimension_mismatch" && x.message.contains("(Q) es 3×3")));
    }

    #[test]
    fn r_con_dimension_distinta_a_las_columnas_de_b_se_detecta() {
        // B es 2×1 -> R tiene que ser 1×1, no 2×2. Es el error de dimension
        // mas comun en codigo de control real.
        let src = "from scipy.linalg import solve_continuous_are\nA = [[0, 1], [0, 0]]\nB = [[0], [1]]\nQ = [[1, 0], [0, 1]]\nR = [[1, 0], [0, 1]]\nsolve_continuous_are(A, B, Q, R)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.kind == "lqr_dimension_mismatch" && x.message.contains("(R) es 2×2")));
    }

    #[test]
    fn r_con_cero_en_la_diagonal_no_puede_ser_definida_positiva() {
        let src = "from scipy.linalg import solve_continuous_are\nA = [[0, 1], [0, 0]]\nB = [[0, 0], [1, 1]]\nQ = [[1, 0], [0, 1]]\nR = [[1, 0], [0, 0]]\nsolve_continuous_are(A, B, Q, R)\n";
        let f = findings(src);
        let hit = f.iter().find(|x| x.kind == "lqr_weight_not_positive_definite").unwrap();
        assert!(hit.message.contains("definida positiva"));
        assert_eq!(hit.confidence, "High");
    }

    #[test]
    fn q_con_negativo_en_la_diagonal_no_puede_ser_psd() {
        let src = "from scipy.linalg import solve_continuous_are\nA = [[0, 1], [0, 0]]\nB = [[0], [1]]\nQ = [[-1, 0], [0, 1]]\nR = [[1]]\nsolve_continuous_are(A, B, Q, R)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.kind == "lqr_weight_not_positive_definite" && x.message.contains("semidefinida positiva")));
    }

    #[test]
    fn q_negativa_se_lee_pese_al_menos_unario() {
        // `-1` es UnaryOp(USub, Constant(1)) en el AST -- sin manejarlo, la
        // matriz entera quedaria ilegible y este caso pasaria de largo.
        let src = "from scipy.linalg import solve_continuous_are\nQ = [[-2, 0], [0, -3]]\nA = [[0, 1], [0, 0]]\nB = [[0], [1]]\nR = [[1]]\nsolve_continuous_are(A, B, Q, R)\n";
        assert!(findings(src).iter().any(|x| x.kind == "lqr_weight_not_positive_definite"));
    }

    #[test]
    fn matriz_de_peso_no_simetrica_se_detecta() {
        let src = "from scipy.linalg import solve_continuous_are\nA = [[0, 1], [0, 0]]\nB = [[0], [1]]\nQ = [[1, 2], [0, 1]]\nR = [[1]]\nsolve_continuous_are(A, B, Q, R)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.kind == "lqr_non_symmetric_weight" && x.message.contains("(Q)")));
    }

    #[test]
    fn np_eye_y_np_diag_se_leen() {
        let src = "import numpy as np\nfrom scipy.linalg import solve_continuous_are\nA = [[0, 1], [0, 0]]\nB = [[0], [1]]\nQ = np.diag([1, -5])\nR = np.eye(1)\nsolve_continuous_are(A, B, Q, R)\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.kind == "lqr_weight_not_positive_definite"), "np.diag([1, -5]) no es PSD");
    }

    #[test]
    fn np_zeros_como_r_se_detecta() {
        let src = "import numpy as np\nfrom scipy.linalg import solve_continuous_are\nA = [[0, 1], [0, 0]]\nB = [[0], [1]]\nQ = np.eye(2)\nR = np.zeros((1, 1))\nsolve_continuous_are(A, B, Q, R)\n";
        assert!(findings(src).iter().any(|x| x.kind == "lqr_weight_not_positive_definite"));
    }

    #[test]
    fn np_array_envolviendo_el_literal_tambien_se_lee() {
        let src = "import numpy as np\nfrom scipy.linalg import solve_continuous_are\nA = np.array([[0, 1], [0, 0]])\nB = np.array([[0], [1]])\nQ = np.array([[1, 0], [0, 1]])\nR = np.array([[0]])\nsolve_continuous_are(A, B, Q, R)\n";
        assert!(findings(src).iter().any(|x| x.kind == "lqr_weight_not_positive_definite"));
    }

    #[test]
    fn lqr_de_la_libreria_control_tambien_se_reconoce() {
        let src = "import control\nA = [[0, 1], [0, 0]]\nB = [[0], [1]]\nQ = [[1, 0], [0, 1]]\nR = [[0]]\nK, S, E = control.lqr(A, B, Q, R)\n";
        assert!(findings(src).iter().any(|x| x.kind == "lqr_weight_not_positive_definite"));
    }

    #[test]
    fn matrices_no_legibles_no_producen_nada() {
        // Vienen de disco: no hay nada que chequear, y no se adivina.
        let src = "import numpy as np\nfrom scipy.linalg import solve_continuous_are\nA = np.load('a.npy')\nB = np.load('b.npy')\nQ = np.load('q.npy')\nR = np.load('r.npy')\nsolve_continuous_are(A, B, Q, R)\n";
        assert!(findings(src).is_empty());
    }

    #[test]
    fn una_funcion_ajena_llamada_lqr_con_menos_de_4_argumentos_no_dispara() {
        let src = "def lqr(x):\n    return x\nlqr(3)\n";
        assert!(findings(src).is_empty());
    }

    #[test]
    fn una_matriz_de_otro_scope_no_se_usa_como_si_fuera_de_este() {
        // La `R` mala esta dentro de una funcion; la llamada a nivel de modulo
        // no debe resolver contra ella (scope-aware, como el taint tracking).
        let src = "from scipy.linalg import solve_continuous_are\ndef otra():\n    R = [[0]]\n    return R\nA = [[0, 1], [0, 0]]\nB = [[0], [1]]\nQ = [[1, 0], [0, 1]]\nsolve_continuous_are(A, B, Q, R)\n";
        assert!(findings(src).is_empty(), "R no esta definida en este scope");
    }

    #[test]
    fn hallazgos_ordenados_por_linea() {
        let src = "from scipy.linalg import solve_continuous_are\nA = [[0, 1], [0, 0]]\nB = [[0], [1]]\nQ = [[1, 0], [0, 1]]\nRbad = [[0]]\nsolve_continuous_are(A, B, Q, Rbad)\nsolve_continuous_are(A, B, Q, Rbad)\n";
        let f = findings(src);
        assert_eq!(f.len(), 2);
        assert!(f[0].line <= f[1].line);
    }

    #[test]
    fn el_mensaje_aclara_que_la_diagonal_es_condicion_necesaria_no_suficiente() {
        let src = "from scipy.linalg import solve_continuous_are\nA = [[0, 1], [0, 0]]\nB = [[0], [1]]\nQ = [[1, 0], [0, 1]]\nR = [[0]]\nsolve_continuous_are(A, B, Q, R)\n";
        let hit = findings(src).into_iter().find(|x| x.kind == "lqr_weight_not_positive_definite").unwrap();
        assert!(hit.message.contains("necesaria, no suficiente"));
    }

    // ── Selección de Gibbs / softmax ──

    #[test]
    fn softmax_escrito_a_mano_sin_estabilizar_se_detecta() {
        let src = "import numpy as np\nv = [1.0, 2.0]\neps = 0.5\np = np.exp(-v / eps) / np.sum(np.exp(-v / eps))\n";
        let f = findings(src);
        assert!(f.iter().any(|x| x.kind == "softmax_unstable"), "{:?}", f.iter().map(|x| x.kind).collect::<Vec<_>>());
    }

    #[test]
    fn softmax_en_dos_pasos_tambien_se_detecta() {
        let src = "import numpy as np\nv = [1.0, 2.0]\nw = np.exp(-v / 0.5)\np = w / w.sum()\n";
        assert!(findings(src).iter().any(|x| x.kind == "softmax_unstable"));
    }

    #[test]
    fn softmax_con_division_in_place_se_detecta() {
        // `p /= p.sum()` es el idioma mas corto de los dos pasos; sin manejar
        // AugAssign se escapaba entero.
        let src = "import numpy as np\nv = [1.0, 2.0]\np = np.exp(-v / 0.5)\np /= p.sum()\n";
        assert!(findings(src).iter().any(|x| x.kind == "softmax_unstable"));
    }

    #[test]
    fn softmax_estabilizado_con_max_no_produce_hallazgo() {
        let src = "import numpy as np\nv = [1.0, 2.0]\nz = v - np.max(v)\np = np.exp(z / 0.5) / np.sum(np.exp(z / 0.5))\n";
        assert!(!findings(src).iter().any(|x| x.kind == "softmax_unstable"));
    }

    #[test]
    fn logsumexp_tambien_cuenta_como_estabilizacion() {
        // Es la otra forma correcta y estandar. Reportarla seria un falso
        // positivo sobre codigo que justamente hizo lo que hay que hacer.
        let src = "import numpy as np\nfrom scipy.special import logsumexp\nv = [1.0, 2.0]\np = np.exp(v - logsumexp(v)) / np.sum(np.exp(v - logsumexp(v)))\n";
        assert!(!findings(src).iter().any(|x| x.kind == "softmax_unstable"));
    }

    #[test]
    fn un_exp_sin_normalizar_no_es_un_softmax() {
        // Decaimiento exponencial: exp sin division por una suma. Sin este
        // limite, todo `np.exp` de cualquier codebase seria un hallazgo.
        let src = "import numpy as np\nt = [1.0, 2.0]\ntau = 3.0\ny = np.exp(-t / tau)\n";
        assert!(findings(src).is_empty());
    }

    #[test]
    fn scipy_special_softmax_no_dispara_nada() {
        // Ya esta estabilizado adentro: no hay nada que reportar.
        let src = "from scipy.special import softmax\nv = [1.0, 2.0]\np = softmax(v)\n";
        assert!(findings(src).is_empty());
    }

    #[test]
    fn temperatura_cero_se_detecta() {
        let src = "import numpy as np\nv = [1.0, 2.0]\np = np.exp(-v / 0.0)\n";
        let f = findings(src);
        let hit = f.iter().find(|x| x.kind == "gibbs_nonpositive_temperature").unwrap();
        assert_eq!(hit.confidence, "High");
        assert!(hit.message.contains("nan"));
    }

    #[test]
    fn temperatura_negativa_invierte_la_distribucion() {
        let src = "import numpy as np\nv = [1.0, 2.0]\np = np.exp(-v / -0.5)\n";
        let f = findings(src);
        let hit = f.iter().find(|x| x.kind == "gibbs_nonpositive_temperature").unwrap();
        assert!(hit.message.contains("lo peor"), "{}", hit.message);
    }

    #[test]
    fn temperatura_pasada_por_variable_se_resuelve() {
        let src = "import numpy as np\neps = 0.0\nv = [1.0, 2.0]\np = np.exp(-v / eps)\n";
        assert!(findings(src).iter().any(|x| x.kind == "gibbs_nonpositive_temperature"));
    }

    #[test]
    fn una_temperatura_reasignada_a_algo_ilegible_no_se_usa() {
        // Si `eps` termina viniendo de una funcion, el 0.0 de arriba ya no es
        // el valor que llega a la llamada: reportarlo seria un falso positivo.
        let src = "import numpy as np\neps = 0.0\neps = schedule(t)\nv = [1.0, 2.0]\np = np.exp(-v / eps)\n";
        assert!(!findings(src).iter().any(|x| x.kind == "gibbs_nonpositive_temperature"));
    }

    #[test]
    fn beta_cero_en_la_forma_multiplicativa_no_se_reporta() {
        // `exp(-beta*V)` usa la temperatura INVERSA: beta = 0 es el limite
        // uniforme, perfectamente legitimo. Limite deliberado del modulo.
        let src = "import numpy as np\nbeta = 0.0\nv = [1.0, 2.0]\np = np.exp(-beta * v)\n";
        assert!(findings(src).is_empty());
    }

    #[test]
    fn probabilidades_que_no_suman_uno_se_detectan() {
        let src = "import numpy as np\nprobs = [0.3, 0.3, 0.3]\nnp.random.choice(3, p=probs)\n";
        let f = findings(src);
        let hit = f.iter().find(|x| x.kind == "random_choice_bad_probabilities").unwrap();
        assert!(hit.message.contains("do not sum to 1"));
    }

    #[test]
    fn la_suma_se_muestra_sin_el_ruido_del_float() {
        // 0.3 + 0.3 + 0.3 = 0.8999999999999999 en binario. Mostrar eso crudo
        // desvia la atencion del problema real hacia un artefacto del formato.
        let src = "import numpy as np\nnp.random.choice(3, p=[0.3, 0.3, 0.3])\n";
        let hit = findings(src).into_iter().find(|x| x.kind == "random_choice_bad_probabilities").unwrap();
        assert!(hit.message.contains("suma 0.900000"), "{}", hit.message);
    }

    #[test]
    fn una_probabilidad_negativa_se_detecta() {
        let src = "import numpy as np\nnp.random.choice(3, p=[0.6, 0.5, -0.1])\n";
        assert!(findings(src)
            .iter()
            .any(|x| x.kind == "random_choice_bad_probabilities" && x.message.contains("non-negative")));
    }

    #[test]
    fn probabilidades_correctas_no_producen_nada() {
        let src = "import numpy as np\nnp.random.choice(3, p=[0.25, 0.25, 0.5])\n";
        assert!(findings(src).is_empty());
    }

    #[test]
    fn random_choice_de_la_stdlib_no_dispara() {
        // `random.choice(seq)` no tiene parametro de probabilidades: sin `p=`
        // ni un 4to posicional, no hay nada que chequear.
        let src = "import random\nrandom.choice([1, 2, 3])\n";
        assert!(findings(src).is_empty());
    }

    #[test]
    fn el_mensaje_del_softmax_dice_que_prueba_la_ausencia_no_el_desborde() {
        let src = "import numpy as np\nv = [1.0, 2.0]\np = np.exp(v) / np.sum(np.exp(v))\n";
        let hit = findings(src).into_iter().find(|x| x.kind == "softmax_unstable").unwrap();
        assert!(hit.message.contains("AUSENCIA"));
        assert_eq!(hit.severity, "Medium");
    }
}
