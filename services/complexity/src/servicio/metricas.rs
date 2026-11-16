//! **Métricas en vivo del editor — Fase 35, portado desde
//! `routers/metrics_live.py`.**
//!
//! La barra que el editor muestra mientras alguien escribe: líneas,
//! funciones, clases, imports, complejidad ciclomática y el peor Big-O del
//! archivo. Es **el endpoint más llamado de toda la app** —se dispara en cada
//! pausa de tecleo— así que el presupuesto de tiempo es el requisito, no un
//! detalle.
//!
//! **Y no es un port: es un reemplazo.** El router traía su propio análisis
//! por lenguaje —`ast.walk` para Python y expresiones regulares para
//! JS/TS/C/Fortran— en paralelo a los parsers reales que el motor ya tenía.
//! No era equivalente: era **peor**, de forma medible.
//!
//! `_big_o_python` contaba los bucles **totales** de una función, no su
//! anidamiento:
//!
//! ```text
//! for a in xs: ...      # tres bucles SECUENCIALES
//! for b in ys: ...      # el archivo es O(n)
//! for c in zs: ...      # el router decía O(n³)
//! ```
//!
//! El Big-O del motor (`costo::bigo`) mide **profundidad de anidamiento
//! real**, así que ese caso ahora da `O(n)`. Además cubre lo que el router no
//! tenía: `O(n^k)` para k ≥ 4 (Python topaba en `O(n³)`), `O(log n)` con
//! detección real de división binaria —el router devolvía `O(log n)` ante
//! cualquier `//=` en una función con un solo bucle— y `O(n log n)` para
//! bucle combinado con recursión, que no existía. Los números que la barra
//! muestra **cambian**, y está dicho en el CHANGELOG porque es visible.
//!
//! **Lo que NO mejora, y lo verifiqué midiendo después de afirmar lo
//! contrario:** una recursión que no divide el problema sigue dando `O(2^n)`
//! en los dos. `factorial(n-1)` es O(n) y el motor dice `O(2^n)` igual que
//! Python, porque `bigo::infer` sólo reconoce la reducción cuando hay un
//! *binary split*. Es una sobreestimación real y compartida, no un punto a
//! favor del reemplazo; queda con un test que la fija para que el día que se
//! arregle se note.
//!
//! **Lo que sí se portó, y por qué es distinto del fallback de
//! `diagrama.rs`.** Si el archivo no parsea, se cuenta por líneas y se marca
//! `safe_mode`. En `diagram.py` el fallback cubría *"el sidecar no
//! responde"*, que adentro del motor es imposible — y se borró. Acá cubre
//! *"el código del usuario todavía no compila"*, que mientras alguien
//! escribe pasa la mitad del tiempo: sin esta rama la barra se vaciaría justo
//! cuando más se la mira.

use std::time::Instant;

use serde::Deserialize;
use serde_json::{json, Map, Value};

/// Qué tan grave es una clase de complejidad. Más alto, peor.
///
/// **Es una escala y no una lista literal porque la lista no alcanzaba.** La
/// primera versión de este módulo tenía una tabla de siete entradas copiada
/// del router viejo —hasta `O(n³)`— y un test con cuatro bucles anidados la
/// desmintió al toque: `bigo::infer` emite `O(n^4)`, `O(n^5)` y lo que haga
/// falta, así que cualquier archivo con grado 4 o más caía en "desconocido".
/// El router no tenía el problema sólo porque su heurística tampoco sabía
/// pasar de 3.
///
/// `None` para lo que no se reconoce, incluido el `"?"` que el motor emite
/// cuando no pudo determinar la complejidad.
pub fn severidad_de_big_o(valor: &str) -> Option<u32> {
    match valor {
        "O(1)" => Some(0),
        "O(log n)" => Some(10),
        "O(n)" => Some(20),
        "O(n log n)" => Some(25),
        // Los superíndices son los que emite `bigo::infer` para grado 2 y 3.
        "O(n²)" => Some(32),
        "O(n³)" => Some(33),
        "O(2^n)" => Some(1_000),
        "O(n!)" => Some(1_100),
        // `O(n^k)` genérico, que es la forma que toma todo grado ≥ 4.
        otro => otro
            .strip_prefix("O(n^")
            .and_then(|r| r.strip_suffix(')'))
            .and_then(|k| k.parse::<u32>().ok())
            .map(|k| 30 + k.min(900)),
    }
}

/// El peor Big-O de una lista.
///
/// **Un valor que la escala no reconoce no gana.** El motor emite `"?"`
/// cuando no pudo determinar la complejidad, y el desempate de Python era
/// *"devolver el primero de la lista"* — así que un solo `?` entre diez
/// funciones podía terminar anunciado como el peor caso del archivo. Decir
/// "no sé" es correcto; decir que "no sé" es lo peor que hay, no.
///
/// Si **todos** son desconocidos sí se devuelve `"?"`: ahí es la respuesta
/// honesta.
pub fn peor_big_o(valores: &[String]) -> String {
    if valores.is_empty() {
        return "O(1)".to_string();
    }
    valores
        .iter()
        .filter_map(|v| severidad_de_big_o(v).map(|s| (s, v)))
        .max_by_key(|(s, _)| *s)
        .map_or_else(|| "?".to_string(), |(_, v)| v.clone())
}

fn distribucion(valores: &[String]) -> Map<String, Value> {
    let mut conteo: std::collections::BTreeMap<&str, u64> = std::collections::BTreeMap::new();
    for v in valores {
        *conteo.entry(v.as_str()).or_insert(0) += 1;
    }
    conteo.into_iter().map(|(k, v)| (k.to_string(), json!(v))).collect()
}

#[derive(Deserialize)]
pub struct MetricasRequest {
    #[serde(default = "archivo_por_defecto")]
    pub filename: String,
    #[serde(default)]
    pub content: String,
}

fn archivo_por_defecto() -> String {
    "script.py".to_string()
}

fn extension(nombre: &str) -> String {
    match nombre.rfind('.') {
        Some(0) | None => String::new(),
        Some(i) => nombre[i..].to_ascii_lowercase(),
    }
}

/// El nombre de lenguaje que el editor muestra en la barra.
///
/// Es el mismo mapa del router viejo, incluidas las entradas que el motor
/// todavía no analiza (`java`, `go`, `php`, `sql`): son una etiqueta para el
/// usuario, no una declaración de capacidad. Lo que el motor sabe analizar
/// sale del manifest de plugins, y `lenguaje_soportado` lo consulta ahí.
pub fn lenguaje_de(ext: &str) -> &'static str {
    match ext {
        ".py" => "python",
        ".ts" | ".tsx" => "typescript",
        ".js" | ".jsx" => "javascript",
        ".c" | ".h" => "c",
        ".cpp" | ".hpp" | ".cc" | ".hh" => "cpp",
        ".f" | ".f90" | ".f95" | ".f03" | ".f08" | ".for" => "fortran",
        ".rs" => "rust",
        ".java" => "java",
        ".go" => "go",
        ".php" => "php",
        ".sql" => "sql",
        ".adb" | ".ads" => "ada",
        ".d" => "d",
        ".s" | ".asm" => "assembly",
        ".css" => "css",
        ".html" => "html",
        _ => "plaintext",
    }
}

/// Métricas ya resueltas, antes de volverse JSON.
#[derive(Default)]
struct Cuenta {
    funciones: usize,
    clases: usize,
    imports: usize,
    complejidades: Vec<u32>,
    big_o: Vec<String>,
    /// `None` si el archivo parseó bien.
    error: Option<String>,
}

fn promedio(v: &[u32]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let suma: u32 = v.iter().sum();
    // Un decimal, igual que el `round(x, 1)` del router: la barra del editor
    // no tiene lugar para más y `CC 3.4285714` no informa nada extra.
    ((suma as f64 / v.len() as f64) * 10.0).round() / 10.0
}

/// **Python por el camino corto, y la razón es un número.**
///
/// La primera versión llamaba a `analyze_rich`, que es la fuente de verdad
/// del análisis de Python — una sola implementación, cero código nuevo. Medir
/// lo descartó: sobre un archivo de 443 líneas con 40 funciones, en build de
/// release, `analyze_rich` cuesta **16,3 ms** contra **1,7 ms** del parseo más
/// la complejidad. El presupuesto declarado de este endpoint es **5 ms** y se
/// dispara en cada pausa de tecleo, así que 16 ms no entraban.
///
/// La diferencia no es el parseo (1,68 ms, inevitable): es que `analyze_rich`
/// calcula por cada función los hallazgos de seguridad, los smells
/// estructurales, naming, modernización, nullabilidad y los hints de WASM.
/// **Nada de eso aparece en la barra del editor.**
///
/// Así que acá se recorre el AST una vez pidiendo sólo lo que la barra
/// muestra, con las mismas funciones públicas que usa `analyze_rich`
/// (`complexity::cyclomatic`, `bigo::loop_analysis`, `bigo::infer`,
/// `recursion::analyze`): **no es una heurística paralela**, es el mismo
/// cálculo sin las etapas que no se muestran. El panel de análisis completo
/// sigue llamando a `analyze_rich` y ahí los 16 ms están bien gastados.
fn de_python(contenido: &str) -> Cuenta {
    let suite = match crate::ast::parser::parse_module(contenido) {
        Ok(s) => s,
        Err(e) => return Cuenta { error: Some(e), ..Default::default() },
    };
    let mut c = Cuenta::default();
    recorrer_python(&suite, &mut c);
    c
}

fn recorrer_python(cuerpo: &[rustpython_parser::ast::Stmt], c: &mut Cuenta) {
    use rustpython_parser::ast::Stmt;
    for stmt in cuerpo {
        match stmt {
            Stmt::FunctionDef(f) => {
                medir_funcion(&f.name, &f.body, c);
                // Las funciones anidadas cuentan, igual que en
                // `analyze_rich`: un `def` adentro de otro es una función.
                recorrer_python(&f.body, c);
            }
            Stmt::AsyncFunctionDef(f) => {
                medir_funcion(&f.name, &f.body, c);
                recorrer_python(&f.body, c);
            }
            Stmt::ClassDef(k) => {
                c.clases += 1;
                recorrer_python(&k.body, c);
            }
            Stmt::Import(_) | Stmt::ImportFrom(_) => c.imports += 1,
            // Un `import` dentro de un `if`/`try` sigue siendo un import, y
            // en Python real pasa seguido (imports condicionales,
            // `try: import ujson`).
            Stmt::If(s) => {
                recorrer_python(&s.body, c);
                recorrer_python(&s.orelse, c);
            }
            Stmt::Try(s) => {
                recorrer_python(&s.body, c);
                recorrer_python(&s.orelse, c);
                recorrer_python(&s.finalbody, c);
                for h in &s.handlers {
                    let rustpython_parser::ast::ExceptHandler::ExceptHandler(h) = h;
                    recorrer_python(&h.body, c);
                }
            }
            Stmt::With(s) => recorrer_python(&s.body, c),
            Stmt::For(s) => {
                recorrer_python(&s.body, c);
                recorrer_python(&s.orelse, c);
            }
            Stmt::While(s) => {
                recorrer_python(&s.body, c);
                recorrer_python(&s.orelse, c);
            }
            _ => {}
        }
    }
}

fn medir_funcion(nombre: &str, cuerpo: &[rustpython_parser::ast::Stmt], c: &mut Cuenta) {
    c.funciones += 1;
    c.complejidades.push(crate::costo::complexity::cyclomatic(cuerpo));
    let recursion = crate::flujo::recursion::analyze(nombre, cuerpo);
    let (profundidad, binaria) = crate::costo::bigo::loop_analysis(cuerpo);
    let (big_o, _razon) = crate::costo::bigo::infer(recursion.is_recursive, profundidad, binaria);
    c.big_o.push(big_o);
}

fn de_js_ts(contenido: &str, typescript: bool) -> Cuenta {
    let r = crate::lenguajes::jsts::parse_js_ts(contenido, typescript);
    Cuenta {
        funciones: r.functions.len(),
        clases: r.classes.len(),
        imports: r.imports.len(),
        complejidades: r.functions.iter().map(|f| f.complexity).collect(),
        big_o: r.functions.iter().map(|f| f.big_o.clone()).collect(),
        error: None,
    }
}

fn de_c(contenido: &str, es_cpp: bool) -> Cuenta {
    let parseado =
        if es_cpp { crate::lenguajes::cparse::parse_cpp(contenido) } else { crate::lenguajes::cparse::parse_c(contenido) };
    let Some(r) = parseado else {
        return Cuenta {
            error: Some("el parser de C/C++ no pudo leer el archivo".to_string()),
            ..Default::default()
        };
    };
    Cuenta {
        funciones: r.functions.len(),
        clases: r.classes.len(),
        imports: r.imports.len(),
        complejidades: r.functions.iter().map(|f| f.complexity).collect(),
        big_o: r.functions.iter().map(|f| f.big_o.clone()).collect(),
        error: None,
    }
}

fn de_fortran(contenido: &str) -> Cuenta {
    let Some(r) = crate::lenguajes::fparse::parse_fortran(contenido) else {
        return Cuenta {
            error: Some("el parser de Fortran no pudo leer el archivo".to_string()),
            ..Default::default()
        };
    };
    Cuenta {
        funciones: r.functions.len(),
        // Fortran no tiene clases en el sentido que la barra muestra; el
        // router viejo también ponía 0 acá, y es correcto.
        clases: 0,
        imports: r.imports.len(),
        complejidades: r.functions.iter().map(|f| f.complexity).collect(),
        big_o: r.functions.iter().map(|f| f.big_o.clone()).collect(),
        error: None,
    }
}

/// **El modo seguro: contar por líneas, sin parsear.**
///
/// Se usa cuando el archivo no compila, que mientras alguien escribe es la
/// mitad del tiempo. No pretende ser análisis: no calcula complejidad ni
/// Big-O, y lo dice devolviendo `"?"` en vez de un número inventado.
fn por_lineas(contenido: &str, ext: &str) -> Cuenta {
    let mut c = Cuenta::default();
    for linea in contenido.lines() {
        let t = linea.trim_start();
        if ext == ".py" {
            let sin_async = t.strip_prefix("async ").unwrap_or(t);
            if sin_async.starts_with("def ") {
                c.funciones += 1;
            } else if t.starts_with("class ") {
                c.clases += 1;
            } else if t.starts_with("import ") || t.starts_with("from ") {
                c.imports += 1;
            }
        } else {
            if t.starts_with("function ") || t.contains("=> ") || t.contains("=>{") {
                c.funciones += 1;
            }
            if t.starts_with("class ") || t.starts_with("struct ") {
                c.clases += 1;
            }
            if t.starts_with("import ") || t.starts_with("#include") || t.starts_with("use ") {
                c.imports += 1;
            }
        }
    }
    c
}

/// Líneas totales y líneas de código.
///
/// `sloc` descarta vacías y comentarios de una línea. **Es una aproximación y
/// no la de `ast::raw`**, que para Python hace el conteo exacto con el AST
/// (cadenas de tres comillas incluidas). Acá se usa el conteo rápido para
/// todos los lenguajes por igual, porque la barra necesita un número para
/// cualquier archivo que el editor tenga abierto, incluidos los que ningún
/// parser conoce.
fn conteo_de_lineas(contenido: &str) -> (usize, usize) {
    let lineas: Vec<&str> = contenido.lines().collect();
    let sloc = lineas
        .iter()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty()
                && !t.starts_with('#')
                && !t.starts_with("//")
                && !t.starts_with("/*")
                && !t.starts_with('*')
                && !t.starts_with('!')
        })
        .count();
    (lineas.len(), sloc)
}

pub fn en_vivo(req: &MetricasRequest) -> Value {
    let t0 = Instant::now();
    let ext = extension(&req.filename);
    let (loc, sloc) = conteo_de_lineas(&req.content);

    let base = |cuenta: Cuenta, loc: usize, sloc: usize, ms: f64| -> Value {
        let safe = cuenta.error.is_some();
        json!({
            "filename": req.filename,
            "language": lenguaje_de(&ext),
            "loc": loc,
            "sloc": sloc,
            "functions": cuenta.funciones,
            "classes": cuenta.clases,
            "imports": cuenta.imports,
            "avg_cc": promedio(&cuenta.complejidades),
            "max_cc": cuenta.complejidades.iter().copied().max().unwrap_or(0),
            "big_o_worst": if cuenta.big_o.is_empty() { "?".to_string() } else { peor_big_o(&cuenta.big_o) },
            "big_o_dist": distribucion(&cuenta.big_o),
            "parse_ok": !safe,
            "parse_error": cuenta.error,
            "safe_mode": safe,
            "ms": ms,
        })
    };

    if req.content.trim().is_empty() {
        // Un archivo vacío no es un error de parseo: es un archivo vacío.
        // `ms: 0` sin medir, igual que el router viejo.
        return base(Cuenta::default(), loc, sloc, 0.0);
    }

    let mut cuenta = match ext.as_str() {
        ".py" => de_python(&req.content),
        ".ts" | ".tsx" => de_js_ts(&req.content, true),
        ".js" | ".jsx" => de_js_ts(&req.content, false),
        ".c" | ".h" => de_c(&req.content, false),
        ".cpp" | ".hpp" | ".cc" | ".hh" => de_c(&req.content, true),
        ".f" | ".f90" | ".f95" | ".f03" | ".f08" | ".for" => de_fortran(&req.content),
        // Sin parser para esta extensión: se cuenta por líneas, pero esto NO
        // es modo seguro — no falló nada, simplemente no hay analizador. Por
        // eso no se marca `error`.
        _ => por_lineas(&req.content, &ext),
    };

    // Si el parser falló, la barra igual muestra algo: se cuenta por líneas
    // conservando el error, que es lo que prende `safe_mode`.
    if cuenta.error.is_some() {
        let error = cuenta.error.take();
        cuenta = por_lineas(&req.content, &ext);
        cuenta.error = error;
    }

    let ms = (t0.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0;
    base(cuenta, loc, sloc, ms)
}

// ── Validación de archivo ─────────────────────────────────────────────────

/// Chequeos de salud de un archivo: binario, truncado, encoding, tamaño.
///
/// **No tiene ningún consumidor en el frontend**, y eso ya se había auditado
/// antes de este port: la conclusión de entonces fue conservarlo por tener
/// tests propios y parecer superficie pensada para algo que no aterrizó. Se
/// porta respetando esa decisión, que sigue siendo razonable y cuesta poco
/// —son chequeos de archivo, no análisis—, con el mismo contrato de salida.
pub fn validar(req: &MetricasRequest) -> Value {
    let ext = extension(&req.filename);
    let contenido = &req.content;
    let mut avisos: Vec<String> = Vec::new();
    let mut errores: Vec<String> = Vec::new();

    // 1. Binario. El umbral de 1 % de bytes nulos es el del router viejo.
    let nulos = contenido.chars().filter(|c| *c == '\0').count();
    let proporcion = nulos as f64 / contenido.chars().count().max(1) as f64;
    if proporcion > 0.01 {
        return json!({
            "filename": req.filename,
            "valid": false,
            "warnings": [],
            "errors": ["Archivo binario detectado — no es texto"],
            "safe_mode": false,
            "is_binary": true,
            "is_truncated": false,
            "encoding_ok": true,
            "size_bytes": contenido.len(),
        });
    }

    // 2. Truncado: termina en un símbolo que abre algo.
    let recortado = contenido.trim_end();
    let mut truncado = false;
    if ext == ".py" {
        if let Some(ultimo) = recortado.chars().last() {
            if matches!(ultimo, ':' | '\\' | ',' | '(' | '[' | '{') {
                truncado = true;
                avisos.push("El archivo parece truncado (termina en símbolo abierto)".to_string());
            }
        }
    }
    if ext == ".ts" || ext == ".js" {
        let abre = recortado.matches('{').count() as i64;
        let cierra = recortado.matches('}').count() as i64;
        if abre != cierra {
            avisos.push(format!(
                "Llaves desbalanceadas ({:+}) — archivo posiblemente incompleto",
                abre - cierra
            ));
        }
    }

    // 3. Sintaxis (sólo Python, igual que antes).
    let mut safe = false;
    if ext == ".py" && !contenido.trim().is_empty() {
        if let Some(e) = crate::servicio::rich::analyze_rich(contenido).error {
            safe = true;
            avisos.push(format!("Error de parse: {e} — modo seguro activado"));
        }
    }

    // 4. Encoding. **Acá el chequeo de Python no podía fallar nunca**: una
    // `str` de Python siempre se puede codificar a UTF-8, así que ese
    // `except UnicodeEncodeError` era inalcanzable. En Rust pasa lo mismo con
    // `String`. Se reporta `true` y se dice por qué en vez de simular una
    // verificación: lo que de verdad decidiría esto es el decodificado al
    // leer el archivo, que ocurre en `/api/upload/.../file`, no acá.
    let encoding_ok = true;

    // 5. Tamaño.
    let lineas = contenido.matches('\n').count();
    if lineas > 10_000 {
        errores.push(format!("Archivo extremadamente largo ({lineas} líneas) — considera dividirlo"));
    } else if lineas > 5_000 {
        avisos.push(format!("Archivo muy largo ({lineas} líneas) — análisis puede ser lento"));
    }

    // 6. Vacío.
    if contenido.trim().is_empty() {
        avisos.push("Archivo vacío".to_string());
    }

    json!({
        "filename": req.filename,
        "valid": errores.is_empty(),
        "warnings": avisos,
        "errors": errores,
        "safe_mode": safe,
        "is_binary": false,
        "is_truncated": truncado,
        "encoding_ok": encoding_ok,
        "size_bytes": contenido.len(),
    })
}

// ── Estado del motor ──────────────────────────────────────────────────────

/// Capacidades del motor de análisis.
///
/// **Dos cosas cambiaron respecto del router, y las dos eran defectos.**
///
/// `complexity` preguntaba *"¿pudo hablar con el sidecar?"*. Acá el sidecar
/// **es** el proceso, así que es siempre `true` — mismo criterio que ya se
/// aplicó en `/static/languages`. Se deja el campo porque el cliente lo lee.
///
/// `supported_languages` era una **lista literal de 11 lenguajes** escrita a
/// mano, y ya estaba desincronizada: el motor analiza 13 y la fuente de
/// verdad es el manifest de plugins desde la Fase 24. Ahora sale de ahí, así
/// que agregar un lenguaje no deja esta lista atrás.
///
/// **Y se filtra por categoría.** La primera versión listaba todos los
/// plugins, lo que metía `manifest` —el lector de `package.json`/`Cargo.toml`,
/// categoría `Format`— en un campo que se llama *languages*. Lo vi en la
/// verificación real: 14 entradas donde tenían que haber 13. Un plugin de
/// formato es algo que el motor sabe leer, no un lenguaje.
pub fn estado() -> Value {
    let mut lenguajes: Vec<String> = crate::servicio::plugin::builtin_manifests()
        .iter()
        .filter(|m| matches!(m.category, crate::servicio::plugin::PluginCategory::Language))
        .map(|m| m.id.to_string())
        .collect();
    lenguajes.sort();
    lenguajes.dedup();

    json!({
        "status": "ok",
        "capabilities": {
            // El parser de Python es `rustpython-parser`, en proceso: no hay
            // un módulo externo que pueda faltar.
            "python_ast": true,
            "complexity": true,
            // `pylint` es una herramienta EXTERNA de Python. Se resuelve con
            // el mismo `toolchain` que el resto de los programas de afuera,
            // en vez de con un `import` que acá no significaría nada.
            "pylint": crate::costo::toolchain::disponible("pylint"),
        },
        "safe_mode_available": true,
        "fallback_parser": "conteo por líneas",
        "supported_languages": lenguajes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vivo(nombre: &str, contenido: &str) -> Value {
        en_vivo(&MetricasRequest { filename: nombre.to_string(), content: contenido.to_string() })
    }

    /// **El defecto que este reemplazo arregla, con el caso exacto.** Tres
    /// bucles secuenciales son O(n); el router decía O(n³) porque contaba
    /// bucles totales en vez de anidamiento.
    #[test]
    fn tres_bucles_secuenciales_no_son_cubicos() {
        let src = "def f(xs, ys, zs):\n    for a in xs:\n        print(a)\n    for b in ys:\n        print(b)\n    for c in zs:\n        print(c)\n";
        let m = vivo("a.py", src);
        assert_eq!(m["functions"], 1);
        assert_eq!(
            m["big_o_worst"], "O(n)",
            "tres bucles SECUENCIALES son lineales, no cubicos: {}",
            m["big_o_worst"]
        );
    }

    /// Y el anidamiento real sí se detecta.
    #[test]
    fn dos_bucles_anidados_si_son_cuadraticos() {
        let src = "def f(m):\n    for fila in m:\n        for celda in fila:\n            print(celda)\n";
        assert_eq!(vivo("a.py", src)["big_o_worst"], "O(n²)");
    }

    /// **Una limitación que el reemplazo NO arregla, fijada a propósito.**
    ///
    /// `factorial(n-1)` es O(n) y acá sale `O(2^n)`, igual que en Python:
    /// `bigo::infer` sólo reconoce que el problema se reduce cuando detecta
    /// un *binary split*, y restar uno no lo es. Yo había afirmado que el
    /// motor distinguía la forma de la recursión; lo medí y era falso.
    ///
    /// El test asierta el comportamiento **real**, no el deseable, para que
    /// el día que `bigo.rs` aprenda a reconocer la reducción lineal este
    /// test falle y alguien venga a leer esto en vez de descubrirlo de nuevo.
    #[test]
    fn una_recursion_lineal_todavia_se_sobreestima_como_exponencial() {
        let src = "def factorial(n):\n    if n <= 1:\n        return 1\n    return n * factorial(n - 1)\n";
        assert_eq!(
            vivo("a.py", src)["big_o_worst"], "O(2^n)",
            "sobreestimacion conocida y compartida con el router viejo"
        );
    }

    /// **Lo que el router NO tenía: grado polinomial arriba de 3.** Topaba en
    /// `O(n³)` porque su tabla terminaba ahí.
    #[test]
    fn cuatro_bucles_anidados_dan_grado_cuatro() {
        let src = "def f(m):\n    for a in m:\n        for b in a:\n            for c in b:\n                for d in c:\n                    print(d)\n";
        assert_eq!(vivo("a.py", src)["big_o_worst"], "O(n^4)");
    }

    #[test]
    fn cuenta_funciones_clases_e_imports_de_python() {
        let src = "import os\nfrom sys import path\n\nclass A:\n    def m(self):\n        return 1\n\ndef libre():\n    return 2\n";
        let m = vivo("a.py", src);
        assert_eq!(m["functions"], 2, "el metodo y la funcion libre");
        assert_eq!(m["classes"], 1);
        assert_eq!(m["imports"], 2);
        assert_eq!(m["parse_ok"], true);
        assert_eq!(m["safe_mode"], false);
    }

    /// **Un archivo roto igual muestra números**: mientras alguien escribe,
    /// el archivo está roto la mitad del tiempo, y vaciar la barra justo ahí
    /// sería lo peor.
    #[test]
    fn un_archivo_roto_marca_modo_seguro_y_sigue_contando() {
        let src = "import os\n\ndef buena():\n    return 1\n\ndef rota(:\n    pass\n";
        let m = vivo("a.py", src);
        assert_eq!(m["parse_ok"], false);
        assert_eq!(m["safe_mode"], true);
        assert!(m["parse_error"].is_string(), "el error tiene que viajar");
        assert_eq!(m["functions"], 2, "las cuenta por lineas: {m}");
        assert_eq!(m["imports"], 1);
        // Pero NO inventa una complejidad ni un Big-O.
        assert_eq!(m["avg_cc"], 0.0);
        assert_eq!(m["big_o_worst"], "?");
    }

    #[test]
    fn un_archivo_vacio_no_es_un_error_de_parseo() {
        let m = vivo("a.py", "   \n\n  ");
        assert_eq!(m["parse_ok"], true);
        assert_eq!(m["safe_mode"], false);
        assert_eq!(m["functions"], 0);
        assert_eq!(m["ms"], 0.0);
    }

    #[test]
    fn el_sloc_descarta_vacias_y_comentarios() {
        let m = vivo("a.py", "# comentario\n\nx = 1\n\n# otro\ny = 2\n");
        assert_eq!(m["loc"], 6);
        assert_eq!(m["sloc"], 2);
    }

    /// **Un `?` no gana el peor caso.** El motor lo emite cuando no pudo
    /// determinar la complejidad, y el desempate de Python devolvía el primer
    /// elemento de la lista, así que un solo `?` podía anunciarse como el
    /// peor Big-O del archivo.
    #[test]
    fn un_big_o_desconocido_no_se_reporta_como_el_peor() {
        let valores: Vec<String> =
            vec!["?".into(), "O(n)".into(), "O(1)".into()];
        assert_eq!(peor_big_o(&valores), "O(n)");
        // Si TODOS son desconocidos, ahí sí es la respuesta honesta.
        assert_eq!(peor_big_o(&["?".to_string(), "?".to_string()]), "?");
        // Y sin funciones, O(1).
        assert_eq!(peor_big_o(&[]), "O(1)");
    }

    #[test]
    fn el_peor_big_o_respeta_el_orden() {
        let v: Vec<String> = vec!["O(1)".into(), "O(n²)".into(), "O(log n)".into(), "O(n)".into()];
        assert_eq!(peor_big_o(&v), "O(n²)");
        let v2: Vec<String> = vec!["O(n log n)".into(), "O(2^n)".into()];
        assert_eq!(peor_big_o(&v2), "O(2^n)");
    }

    /// **El grado polinomial se compara como número, no se busca en una
    /// lista.** La primera versión tenía una tabla hasta `O(n³)` y cualquier
    /// grado mayor caía en "desconocido" — lo destapó el test de los cuatro
    /// bucles anidados.
    #[test]
    fn el_grado_polinomial_alto_se_ordena_bien() {
        assert!(severidad_de_big_o("O(n^4)") > severidad_de_big_o("O(n³)"));
        assert!(severidad_de_big_o("O(n^7)") > severidad_de_big_o("O(n^4)"));
        assert!(severidad_de_big_o("O(2^n)") > severidad_de_big_o("O(n^9)"));
        let v: Vec<String> = vec!["O(n³)".into(), "O(n^5)".into(), "O(n)".into()];
        assert_eq!(peor_big_o(&v), "O(n^5)");
    }

    /// La escala completa, de menos a más grave, en un solo lugar.
    #[test]
    fn la_escala_de_severidad_es_monotona() {
        let orden = ["O(1)", "O(log n)", "O(n)", "O(n log n)", "O(n²)", "O(n³)", "O(n^4)", "O(2^n)", "O(n!)"];
        let severidades: Vec<u32> = orden.iter().map(|b| severidad_de_big_o(b).unwrap()).collect();
        let mut ordenadas = severidades.clone();
        ordenadas.sort_unstable();
        assert_eq!(severidades, ordenadas, "la escala tiene que crecer: {severidades:?}");
        assert!(severidad_de_big_o("?").is_none());
        assert!(severidad_de_big_o("O(algo raro)").is_none());
    }

    #[test]
    fn la_distribucion_cuenta_cada_clase() {
        let src = "def a(xs):\n    for x in xs:\n        print(x)\n\ndef b():\n    return 1\n\ndef c():\n    return 2\n";
        let m = vivo("a.py", src);
        let d = m["big_o_dist"].as_object().unwrap();
        assert_eq!(d["O(1)"], 2);
        assert_eq!(d["O(n)"], 1);
    }

    // ── Otros lenguajes, con los parsers REALES del motor ─────────────

    #[test]
    fn en_js_usa_el_parser_real_y_no_una_expresion_regular() {
        let src = "import { x } from './x'\n\nclass C {}\n\nfunction uno(a) {\n  if (a) { return 1 }\n  return 2\n}\n\nconst dos = () => 3\n";
        let m = vivo("app.js", src);
        assert_eq!(m["language"], "javascript");
        assert_eq!(m["classes"], 1);
        assert_eq!(m["imports"], 1);
        assert!(m["functions"].as_u64().unwrap() >= 2, "uno y dos: {m}");
        // El Big-O sale del parser, no de contar `for` con una regex.
        assert!(m["big_o_worst"].as_str().unwrap().starts_with("O("), "{m}");
    }

    #[test]
    fn en_ts_detecta_typescript_por_la_extension() {
        let m = vivo("app.ts", "export function f(x: number): number { return x }\n");
        assert_eq!(m["language"], "typescript");
        assert_eq!(m["functions"], 1);
    }

    #[test]
    fn en_c_usa_el_parser_real() {
        let src = "#include <stdio.h>\n\nstruct P { int x; };\n\nint sumar(int n) {\n    int t = 0;\n    for (int i = 0; i < n; i++) { t += i; }\n    return t;\n}\n";
        let m = vivo("a.c", src);
        assert_eq!(m["language"], "c");
        assert_eq!(m["imports"], 1, "el include");
        assert_eq!(m["functions"], 1);
        assert_eq!(m["big_o_worst"], "O(n)", "un bucle: {m}");
    }

    #[test]
    fn en_fortran_usa_el_parser_real_y_no_tiene_clases() {
        let src = "subroutine suma(n, t)\n  integer :: i, n, t\n  t = 0\n  do i = 1, n\n    t = t + i\n  end do\nend subroutine suma\n";
        let m = vivo("a.f90", src);
        assert_eq!(m["language"], "fortran");
        assert_eq!(m["classes"], 0);
        assert_eq!(m["functions"], 1);
    }

    /// Una extensión sin parser **no es modo seguro**: no falló nada,
    /// simplemente no hay analizador. La distinción importa porque
    /// `safe_mode` prende un aviso en la UI.
    #[test]
    fn una_extension_sin_parser_no_marca_modo_seguro() {
        let m = vivo("notas.rb", "def hola\n  puts 1\nend\n");
        assert_eq!(m["language"], "plaintext");
        assert_eq!(m["safe_mode"], false, "no fallo nada: no hay parser");
        assert_eq!(m["parse_ok"], true);
        assert!(m["parse_error"].is_null());
        assert_eq!(m["loc"], 3);
    }

    // ── validar ───────────────────────────────────────────────────────

    fn validacion(nombre: &str, contenido: &str) -> Value {
        validar(&MetricasRequest { filename: nombre.to_string(), content: contenido.to_string() })
    }

    #[test]
    fn un_archivo_sano_es_valido() {
        let v = validacion("a.py", "x = 1\n");
        assert_eq!(v["valid"], true);
        assert_eq!(v["is_binary"], false);
        assert_eq!(v["errors"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn detecta_contenido_binario_y_corta_ahi() {
        let v = validacion("a.py", "\0\0\0\0\0datos");
        assert_eq!(v["is_binary"], true);
        assert_eq!(v["valid"], false);
        assert!(!v["errors"].as_array().unwrap().is_empty());
    }

    #[test]
    fn detecta_un_python_truncado() {
        let v = validacion("a.py", "def f():\n    if x:\n");
        assert_eq!(v["is_truncated"], true);
        assert!(v["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("truncado")));
    }

    #[test]
    fn detecta_llaves_desbalanceadas_en_ts() {
        let v = validacion("a.ts", "function f() {\n  if (x) {\n");
        let avisos = v["warnings"].as_array().unwrap();
        assert!(
            avisos.iter().any(|w| w.as_str().unwrap().contains("desbalanceadas")),
            "{avisos:?}"
        );
    }

    #[test]
    fn un_error_de_sintaxis_prende_el_modo_seguro() {
        let v = validacion("a.py", "def f(:\n    pass\n");
        assert_eq!(v["safe_mode"], true);
        assert!(v["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("modo seguro")));
    }

    #[test]
    fn un_archivo_enorme_es_un_error_y_uno_grande_solo_un_aviso() {
        let grande = "x = 1\n".repeat(6_000);
        let v = validacion("a.py", &grande);
        assert_eq!(v["valid"], true, "6.000 lineas avisa, no invalida");
        assert!(v["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("muy largo")));

        let enorme = "x = 1\n".repeat(11_000);
        let v2 = validacion("a.py", &enorme);
        assert_eq!(v2["valid"], false, "11.000 lineas si invalida");
    }

    #[test]
    fn un_archivo_vacio_avisa() {
        let v = validacion("a.py", "");
        assert!(v["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("vacío")));
    }

    /// **El presupuesto de este endpoint, medido.**
    ///
    /// Se dispara en cada pausa de tecleo, así que el costo importa más acá
    /// que en cualquier otro endpoint del motor. `analyze_rich` calcula
    /// bastante más de lo que la barra muestra (seguridad, smells, naming,
    /// modernización, hints de WASM), así que había que ver el número en vez
    /// de suponerlo.
    ///
    /// Va con `#[ignore]` porque es wall-clock: mide el entorno tanto como el
    /// código, y en una máquina cargada o en CI daría falsos rojos. Misma
    /// política que los otros diez benchmarks del motor. Se corre a mano con
    /// `cargo test -- --ignored presupuesto`.
    #[test]
    #[ignore = "wall-clock: se corre a mano, no en CI"]
    fn presupuesto_de_tiempo_de_la_barra() {
        // Un archivo del tamaño que alguien edita de verdad: ~400 líneas con
        // 40 funciones, bucles anidados y clases.
        let mut src = String::from("import os\nimport sys\n\n");
        for i in 0..40 {
            src.push_str(&format!(
                "class C{i}:\n    def metodo_{i}(self, xs, ys):\n        total = 0\n        for x in xs:\n            for y in ys:\n                if x > y:\n                    total += x\n                else:\n                    total -= y\n        return total\n\n"
            ));
        }
        let lineas = src.lines().count();

        // Una pasada de calentamiento: la primera incluye costos que no se
        // repiten y mediría otra cosa.
        let _ = vivo("grande.py", &src);

        let vueltas = 20;
        let t0 = std::time::Instant::now();
        for _ in 0..vueltas {
            let _ = vivo("grande.py", &src);
        }
        let por_vuelta = t0.elapsed().as_secs_f64() * 1000.0 / f64::from(vueltas);

        println!(
            "\n  presupuesto de /metrics/live: {por_vuelta:.2} ms por llamada \
             ({lineas} líneas, 40 funciones, {vueltas} vueltas)"
        );

        // Desglose: dónde se va el tiempo.
        let medir = |etiqueta: &str, f: &dyn Fn()| {
            f();
            let t = std::time::Instant::now();
            for _ in 0..vueltas {
                f();
            }
            println!("    {etiqueta:28} {:7.2} ms", t.elapsed().as_secs_f64() * 1000.0 / f64::from(vueltas));
        };
        medir("parse_module solo", &|| {
            let _ = crate::ast::parser::parse_module(&src);
        });
        medir("analyze (el liviano)", &|| {
            let _ = crate::analyze(&src);
        });
        medir("analyze_rich (el completo)", &|| {
            let _ = crate::servicio::rich::analyze_rich(&src);
        });
        // **El tope está puesto para atrapar una regresión concreta.**
        // Medido en release en esta máquina: **1,74 ms** por el camino
        // liviano contra **15,51 ms** cuando esto llamaba a `analyze_rich`.
        // 10 ms deja 5x de margen sobre lo medido —el wall-clock varía con la
        // máquina— y queda bien debajo del costo de `analyze_rich`, así que
        // si alguien vuelve a enchufar el análisis completo acá, este test lo
        // dice en vez de dejar la barra del editor 9x más lenta en silencio.
        assert!(
            por_vuelta < 10.0,
            "la barra del editor tardó {por_vuelta:.2} ms por pulsación — \
             ¿volvió a usar `analyze_rich` en vez del camino liviano?"
        );
    }

    // ── estado ────────────────────────────────────────────────────────

    /// **La lista de lenguajes sale del manifest, no de una tabla a mano.**
    /// La del router tenía 11 escritos literalmente y el motor ya analizaba
    /// más.
    #[test]
    fn los_lenguajes_soportados_salen_del_manifest() {
        let e = estado();
        let ls: Vec<&str> =
            e["supported_languages"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert!(ls.len() > 11, "el router listaba 11 a mano; el manifest tiene mas: {}", ls.len());
        for esperado in ["python", "c", "cpp", "fortran", "ada", "d"] {
            assert!(ls.contains(&esperado), "falta {esperado} en {ls:?}");
        }
        // **Un plugin de FORMATO no es un lenguaje.** `manifest` lee
        // `package.json`/`Cargo.toml`; la primera versión de esto lo listaba
        // acá porque no filtraba por categoría.
        assert!(!ls.contains(&"manifest"), "`manifest` es categoria Format, no un lenguaje: {ls:?}");
        // Y viene ordenada y sin repetidos, para que dos llamadas se comparen.
        let mut ordenada = ls.clone();
        ordenada.sort_unstable();
        assert_eq!(ls, ordenada);
    }

    #[test]
    fn el_estado_declara_el_motor_en_proceso() {
        let e = estado();
        assert_eq!(e["status"], "ok");
        assert_eq!(e["capabilities"]["complexity"], true, "el sidecar ES el proceso");
        assert_eq!(e["capabilities"]["python_ast"], true);
        // `pylint` es externo: puede estar o no, pero tiene que ser booleano.
        assert!(e["capabilities"]["pylint"].is_boolean());
    }
}
