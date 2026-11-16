//! **Análisis de código por archivo y por proyecto — Fase 35, portado desde
//! `routers/analysis.py`.**
//!
//! Los dos endpoints que alimentan el panel de problemas: `/analyze/code`
//! para un archivo y `/analyze/project` para un proyecto entero.
//!
//! **Y acá se borraron `flake8` y `pylint`, por decisión explícita del
//! usuario (2026-11-15).** El router los corría por subprocess, y el motor
//! podía seguir haciéndolo con el mismo `toolchain` que usa para
//! `gcc`/`gnatprove`/`dmd`. Se descartó porque dejaba el mandato a medias:
//! *"Sythrall sin Python"* seguiría necesitando un Python instalado en la
//! máquina para que este panel funcione igual que hoy. Se pierden las reglas
//! de esos dos linters; queda el análisis propio del motor
//! (`security`/`smells`/`naming`/`modernization`/`control`) más las **siete
//! reglas que `_run_ast` tenía**, que se verificó que **ninguna existía** en
//! el motor y se portan acá enteras.
//!
//! **Lo que esto NO reemplaza**: `/static/parse` sigue siendo el análisis
//! profundo (seguridad, smells, Big-O, modernización). Esto es el tier de
//! issues con línea y severidad que el panel de problemas lista.

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::ast::walk::walk_stmts;

/// El largo de línea que dispara `E501`. Es el del router viejo, que a su vez
/// es el default de `flake8` con `max-line-length` subido de 79 a 120.
const MAX_LARGO_DE_LINEA: usize = 120;

/// Cuántos caracteres del mensaje entran en la clave de deduplicación.
const PREFIJO_DE_DEDUP: usize = 40;

#[derive(Deserialize)]
pub struct ArchivoDeCodigo {
    #[serde(default = "desconocido")]
    pub filename: String,
    #[serde(default)]
    pub content: String,
}

fn desconocido() -> String {
    "unknown".to_string()
}

#[derive(Deserialize)]
pub struct CodigoRequest {
    #[serde(default = "archivo_por_defecto")]
    pub filename: String,
    #[serde(default)]
    pub content: String,
    #[serde(default = "herramientas_por_defecto")]
    pub tools: Vec<String>,
}

fn archivo_por_defecto() -> String {
    "script.py".to_string()
}

/// **Sin `flake8` ni `pylint`.** El cliente puede seguir mandándolos en
/// `tools` —el frontend viejo lo hace— y simplemente se ignoran: un nombre de
/// herramienta que no existe no es un error del pedido. Lo que el cliente
/// sabe es qué corrió de verdad, por `tools_used`.
fn herramientas_por_defecto() -> Vec<String> {
    vec!["ast".to_string(), "complexity".to_string()]
}

#[derive(Deserialize)]
pub struct ProyectoRequest {
    #[serde(default)]
    pub files: Vec<ArchivoDeCodigo>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default = "herramientas_por_defecto")]
    pub tools: Vec<String>,
}

// ── Las siete reglas que `_run_ast` tenía ─────────────────────────────────

fn issue(linea: usize, col: usize, severidad: &str, codigo: &str, mensaje: String) -> Value {
    json!({
        "tool": "ast",
        "line": linea,
        "col": col,
        "severity": severidad,
        "code": codigo,
        "message": mensaje,
    })
}

/// **Las reglas, una por una, con su motivo.**
///
/// Ninguna existía en el motor —se verificó buscando cada una en
/// `hallazgos/`— así que son el aporte real de este port, no una
/// reimplementación.
///
/// - **`E001`** error de sintaxis. Es la única que corta: sin AST no hay nada
///   más que decir del archivo.
/// - **`W001`** `assert` en código de producción: `python -O` lo borra, así
///   que una validación escrita con `assert` **desaparece** en una corrida
///   optimizada. Es la regla más valiosa de las siete.
/// - **`W002`** `except:` sin tipo: se come `KeyboardInterrupt` y
///   `SystemExit`, así que un Ctrl-C deja de funcionar.
/// - **`W003`** `global`: estado mutable compartido, difícil de testear.
/// - **`C001`** falta de docstring en función o clase.
/// - **`C002`** `print()` en vez de logging.
/// - **`E501`** línea de más de 120 caracteres.
pub fn reglas_de_ast(contenido: &str) -> Vec<Value> {
    let mut issues = Vec::new();
    let suite = match crate::ast::parser::parse_module(contenido) {
        Ok(s) => s,
        Err(e) => {
            // La línea sale del byte offset del mensaje, igual que en
            // `servicio::diagrama`: el parser habla en offsets y el panel
            // ubica por línea.
            let linea = linea_del_error(&e, contenido);
            return vec![issue(linea, 0, "error", "E001", format!("SyntaxError: {e}"))];
        }
    };

    let indice = crate::ast::parser::LineIndex::build(contenido);
    // Dos acumuladores y no uno: `walk_stmts` toma los dos closures a la vez,
    // asi que no pueden capturar el mismo `Vec` mutable. Se unen abajo.
    let mut de_stmts: Vec<Value> = Vec::new();
    let mut de_exprs: Vec<Value> = Vec::new();
    let mut en_stmt = |stmt: &rustpython_parser::ast::Stmt| {
        use rustpython_parser::ast::Stmt;
        let linea = |r: &rustpython_parser::text_size::TextRange| {
            indice.line_of(r.start().to_usize())
        };
        match stmt {
            Stmt::Assert(s) => {
                de_stmts.push(issue(
                    linea(&s.range),
                    0,
                    "warning",
                    "W001",
                    "assert puede desactivarse con -O".to_string(),
                ));
            }
            Stmt::Global(s) => {
                de_stmts.push(issue(
                    linea(&s.range),
                    0,
                    "warning",
                    "W003",
                    format!(
                        "Variable global: {}",
                        s.names.iter().map(|n| n.as_str()).collect::<Vec<_>>().join(", ")
                    ),
                ));
            }
            Stmt::FunctionDef(f) => {
                if !tiene_docstring(&f.body) {
                    de_stmts.push(issue(
                        linea(&f.range),
                        0,
                        "info",
                        "C001",
                        format!("'{}' sin docstring", f.name),
                    ));
                }
            }
            Stmt::AsyncFunctionDef(f) => {
                if !tiene_docstring(&f.body) {
                    de_stmts.push(issue(
                        linea(&f.range),
                        0,
                        "info",
                        "C001",
                        format!("'{}' sin docstring", f.name),
                    ));
                }
            }
            Stmt::ClassDef(c) => {
                if !tiene_docstring(&c.body) {
                    de_stmts.push(issue(
                        linea(&c.range),
                        0,
                        "info",
                        "C001",
                        format!("'{}' sin docstring", c.name),
                    ));
                }
            }
            Stmt::Try(s) => {
                for h in &s.handlers {
                    let rustpython_parser::ast::ExceptHandler::ExceptHandler(h) = h;
                    // `except:` pelado. `except Exception:` NO entra: es
                    // amplio pero deja pasar KeyboardInterrupt, así que no es
                    // el mismo problema. El router miraba `node.type is None`,
                    // que es exactamente esto.
                    if h.type_.is_none() {
                        de_stmts.push(issue(
                            linea(&h.range),
                            0,
                            "warning",
                            "W002",
                            "except genérico captura todo".to_string(),
                        ));
                    }
                }
            }
            _ => {}
        }
    };
    let mut en_expr = |expr: &rustpython_parser::ast::Expr| {
        use rustpython_parser::ast::Expr;
        if let Expr::Call(c) = expr {
            if let Expr::Name(n) = &*c.func {
                if n.id.as_str() == "print" {
                    de_exprs.push(issue(
                        indice.line_of(c.range.start().to_usize()),
                        0,
                        "info",
                        "C002",
                        "print() — usa logging".to_string(),
                    ));
                }
            }
        }
    };
    walk_stmts(&suite, &mut en_stmt, &mut en_expr);
    issues.extend(de_stmts);
    issues.extend(de_exprs);

    // E501 va por líneas crudas, sin AST: una línea larga lo es sin importar
    // qué contenga. Se cuenta en **caracteres**, no en bytes, que es lo que
    // hace el `len(line)` de Python — una línea de 100 caracteres acentuados
    // no mide 200.
    for (i, linea) in contenido.lines().enumerate() {
        let largo = linea.chars().count();
        if largo > MAX_LARGO_DE_LINEA {
            issues.push(issue(
                i + 1,
                MAX_LARGO_DE_LINEA + 1,
                "warning",
                "E501",
                format!("Línea de {largo} chars"),
            ));
        }
    }

    issues
}

fn tiene_docstring(cuerpo: &[rustpython_parser::ast::Stmt]) -> bool {
    use rustpython_parser::ast::{Expr, Stmt};
    // El criterio del router era "el primer statement es una expresión
    // constante". Deliberadamente laxo: `42` como primera línea cuenta como
    // docstring. Se conserva porque es la regla que el panel viene mostrando,
    // y endurecerla acá cambiaría los conteos sin que nadie lo pidiera.
    matches!(cuerpo.first(), Some(Stmt::Expr(e)) if matches!(&*e.value, Expr::Constant(_)))
}

fn linea_del_error(mensaje: &str, contenido: &str) -> usize {
    let Some(resto) = mensaje.split("byte offset ").nth(1) else { return 1 };
    let digitos: String = resto.chars().take_while(char::is_ascii_digit).collect();
    let Ok(offset) = digitos.parse::<usize>() else { return 1 };
    contenido[..offset.min(contenido.len())].matches('\n').count() + 1
}

// ── Deduplicación ─────────────────────────────────────────────────────────

/// Quita issues repetidos y ordena por línea.
///
/// **La clave es `(línea, primeros 40 caracteres del mensaje)`**, igual que
/// el `dedup_by_key` de Python. Ese prefijo existía para que dos herramientas
/// distintas reportando el mismo problema en la misma línea con mensajes
/// parecidos colapsaran en uno. **Con `flake8` y `pylint` afuera ya no hay
/// dos herramientas**, así que hoy sólo colapsa duplicados internos — se
/// conserva porque el orden y la unicidad son parte del shape que el panel
/// consume, no porque siga cumpliendo su propósito original.
///
/// El corte del prefijo es por **caracteres**: cortar a 40 bytes partiría un
/// acento y dos mensajes distintos podrían colapsar por casualidad.
pub fn deduplicar(mut issues: Vec<Value>) -> Vec<Value> {
    let mut vistos: std::collections::BTreeSet<(i64, String)> = std::collections::BTreeSet::new();
    issues.retain(|i| {
        let linea = i.get("line").and_then(Value::as_i64).unwrap_or(0);
        let mensaje: String = i
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("")
            .chars()
            .take(PREFIJO_DE_DEDUP)
            .collect();
        vistos.insert((linea, mensaje))
    });
    // Orden estable por línea: dos issues de la misma línea conservan el
    // orden en que se encontraron.
    issues.sort_by_key(|i| i.get("line").and_then(Value::as_i64).unwrap_or(0));
    issues
}

// ── Un archivo ────────────────────────────────────────────────────────────

fn extension(nombre: &str) -> String {
    match nombre.rfind('.') {
        Some(0) | None => String::new(),
        Some(i) => nombre[i..].to_ascii_lowercase(),
    }
}

fn pidio(tools: &[String], cual: &str) -> bool {
    tools.iter().any(|t| t == cual)
}

/// El análisis de un archivo, con el mismo shape que el router devolvía.
pub fn un_archivo(filename: &str, contenido: &str, tools: &[String]) -> Value {
    let ext = extension(filename);
    let mut issues: Vec<Value> = Vec::new();
    let mut usadas: Vec<&str> = Vec::new();
    // **`metrics` queda vacío, y es una consecuencia visible de borrar
    // pylint.** Su único campo era `pylint_score`. Se conserva el objeto
    // porque el shape de la respuesta lo trae y el cliente lo lee con
    // `?.`, pero ya no hay nada que poner adentro.
    let metricas = Map::new();
    let mut complejidad = json!([]);
    let mut mantenibilidad = Value::Null;
    let mut halstead = Value::Null;
    let mut raw = json!({});

    if ext == ".py" && pidio(tools, "ast") {
        issues.extend(reglas_de_ast(contenido));
        usadas.push("ast");
    }

    if ext == ".py" && pidio(tools, "complexity") {
        let r = crate::analyze(contenido);
        complejidad = serde_json::to_value(&r.functions).unwrap_or_else(|_| json!([]));
        mantenibilidad = r.mi.map_or(Value::Null, |v| json!(v));
        halstead = r.halstead.as_ref().map_or(Value::Null, |h| {
            serde_json::to_value(h).unwrap_or(Value::Null)
        });
        raw = serde_json::to_value(&r.raw).unwrap_or_else(|_| json!({}));
        // El error del parseo viaja dentro de `raw_stats`, donde el router lo
        // ponía: el panel ya lo lee de ahí.
        if let (Some(e), Some(o)) = (r.error.as_ref(), raw.as_object_mut()) {
            o.insert("error".to_string(), json!(e));
        }
        usadas.push("complexity");
    }

    // **JSON inválido también es un issue**, con el mismo código que usa el
    // error de sintaxis de Python. `serde_json` reporta línea y columna
    // directamente, así que no hace falta convertir offsets.
    if ext == ".json" {
        if let Err(e) = serde_json::from_str::<Value>(contenido) {
            issues.push(json!({
                "tool": "json",
                "line": e.line(),
                "col": e.column(),
                "severity": "error",
                "code": "E999",
                "message": format!("JSON inválido: {e}"),
            }));
        }
    }

    json!({
        "filename": filename,
        "ts": crate::servicio::logstore::now_string(),
        "issues": deduplicar(issues),
        "metrics": metricas,
        "complexity": complejidad,
        "maintainability": mantenibilidad,
        "halstead": halstead,
        "raw_stats": raw,
        "tools_used": usadas,
    })
}

pub fn analizar_codigo(req: &CodigoRequest) -> Value {
    un_archivo(&req.filename, &req.content, &req.tools)
}

// ── Un proyecto ───────────────────────────────────────────────────────────

/// El análisis de un proyecto entero.
///
/// **Con `project_id` lee del disco; sin él usa los archivos del pedido**, y
/// si vienen los dos gana `project_id` — mismo criterio que
/// `/analyze/graph/project`, y el que los tests de Python ya fijaban.
///
/// **El motivo original del endpoint se evaporó, y conviene decirlo.** Existía
/// porque `/code` arrancaba un intérprete de `flake8` y otro de `pylint`
/// **por archivo** (~0,9 s cada uno, ~15 min proyectados a 1.000 archivos), y
/// este endpoint los invocaba una sola vez con todos los paths juntos: 200
/// archivos pasaron de ~180 s a ~5,6 s. Sin esos dos subprocess no queda
/// ningún costo de arranque que amortizar: analizar N archivos es N veces el
/// análisis de uno. El endpoint se conserva porque **el cliente lo usa** y
/// porque leer un proyecto del disco por `project_id` sigue siendo útil, no
/// por el batch.
pub fn analizar_proyecto(req: &ProyectoRequest) -> Value {
    let archivos: Vec<(String, String)> = match req.project_id.as_deref() {
        Some(id) => leer_del_disco(id),
        None => req.files.iter().map(|f| (f.filename.clone(), f.content.clone())).collect(),
    };

    let mut salida = Map::new();
    for (nombre, contenido) in &archivos {
        salida.insert(nombre.clone(), un_archivo(nombre, contenido, &req.tools));
    }
    json!({ "files": salida })
}

/// Los archivos de código de un proyecto subido.
///
/// Reusa el recorrido de `servicio::subidas`, que ya respeta la lista
/// unificada de directorios ignorados — en Python esto era
/// `read_project_files` con su propia copia de esa lista.
fn leer_del_disco(project_id: &str) -> Vec<(String, String)> {
    let Some(dir) = crate::servicio::subidas::ruta_de_proyecto(project_id) else {
        return Vec::new();
    };
    if !dir.is_dir() {
        return Vec::new();
    }
    let (extensiones, _nombres) = crate::servicio::proyecto::reclamos_de_los_plugins();
    crate::servicio::subidas::rutas_relativas(&dir)
        .into_iter()
        .filter(|rel| {
            let ext = extension(rel);
            !ext.is_empty() && extensiones.contains(&ext)
        })
        .filter_map(|rel| {
            let absoluta = rel.split('/').fold(dir.clone(), |a, p| a.join(p));
            let crudo = std::fs::read(&absoluta).ok()?;
            let texto = String::from_utf8_lossy(&crudo).to_string();
            // Un archivo que sólo tiene espacios no aporta nada al panel, y
            // el router también lo descartaba.
            if texto.trim().is_empty() {
                None
            } else {
                Some((rel, texto))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codigos(issues: &[Value]) -> Vec<&str> {
        issues.iter().map(|i| i["code"].as_str().unwrap()).collect()
    }

    fn analizar(contenido: &str) -> Vec<Value> {
        deduplicar(reglas_de_ast(contenido))
    }

    /// **La regla más valiosa de las siete**: `python -O` borra los `assert`,
    /// así que una validación escrita con `assert` desaparece en una corrida
    /// optimizada.
    #[test]
    fn detecta_assert_que_el_flag_o_borra() {
        let i = analizar("def f(x):\n    \"\"\"doc\"\"\"\n    assert x > 0\n    return x\n");
        assert!(codigos(&i).contains(&"W001"), "{:?}", codigos(&i));
        let w = i.iter().find(|x| x["code"] == "W001").unwrap();
        assert_eq!(w["line"], 3);
        assert_eq!(w["severity"], "warning");
    }

    /// **`except:` pelado sí; `except Exception:` no.** Son problemas
    /// distintos: el primero se come `KeyboardInterrupt` y rompe el Ctrl-C.
    #[test]
    fn el_except_pelado_se_reporta_y_el_tipado_no() {
        let pelado = analizar("try:\n    x = 1\nexcept:\n    pass\n");
        assert!(codigos(&pelado).contains(&"W002"), "{:?}", codigos(&pelado));

        let tipado = analizar("try:\n    x = 1\nexcept Exception:\n    pass\n");
        assert!(!codigos(&tipado).contains(&"W002"), "except Exception no es lo mismo");
    }

    #[test]
    fn detecta_global_con_los_nombres() {
        let i = analizar("def f():\n    \"\"\"d\"\"\"\n    global a, b\n    return 1\n");
        let w = i.iter().find(|x| x["code"] == "W003").unwrap();
        assert!(w["message"].as_str().unwrap().contains("a, b"), "{}", w["message"]);
    }

    #[test]
    fn detecta_falta_de_docstring_en_funcion_y_clase() {
        let i = analizar("def sin_doc():\n    return 1\n\nclass SinDoc:\n    pass\n");
        let c: Vec<&str> =
            i.iter().filter(|x| x["code"] == "C001").map(|x| x["message"].as_str().unwrap()).collect();
        assert_eq!(c.len(), 2, "{c:?}");
        assert!(c.iter().any(|m| m.contains("sin_doc")));
        assert!(c.iter().any(|m| m.contains("SinDoc")));
    }

    #[test]
    fn una_funcion_con_docstring_no_se_reporta() {
        let i = analizar("def con_doc():\n    \"\"\"Hace algo.\"\"\"\n    return 1\n");
        assert!(!codigos(&i).contains(&"C001"), "{:?}", codigos(&i));
    }

    /// El criterio de docstring es el del router: *el primer statement es una
    /// expresión constante*. Es laxo a propósito y se conserva para no
    /// cambiar los conteos que el panel viene mostrando.
    #[test]
    fn el_criterio_de_docstring_es_el_laxo_de_siempre() {
        let i = analizar("def f():\n    42\n    return 1\n");
        assert!(
            !codigos(&i).contains(&"C001"),
            "`42` como primer statement cuenta como docstring, igual que antes"
        );
    }

    #[test]
    fn detecta_print() {
        let i = analizar("def f():\n    \"\"\"d\"\"\"\n    print('hola')\n");
        let c = i.iter().find(|x| x["code"] == "C002").unwrap();
        assert_eq!(c["line"], 3);
        assert_eq!(c["severity"], "info");
    }

    /// `obj.print()` no es `print()`: sólo la llamada al builtin por nombre.
    #[test]
    fn un_metodo_llamado_print_no_cuenta() {
        let i = analizar("def f(log):\n    \"\"\"d\"\"\"\n    log.print('x')\n");
        assert!(!codigos(&i).contains(&"C002"), "{:?}", codigos(&i));
    }

    #[test]
    fn detecta_lineas_largas_con_su_largo() {
        let larga = "x = '".to_string() + &"a".repeat(130) + "'";
        let i = analizar(&format!("\"\"\"d\"\"\"\n{larga}\n"));
        let e = i.iter().find(|x| x["code"] == "E501").unwrap();
        assert_eq!(e["line"], 2);
        assert_eq!(e["col"], 121);
        assert!(e["message"].as_str().unwrap().contains("136"), "{}", e["message"]);
    }

    /// **E501 cuenta caracteres, no bytes.** 100 caracteres acentuados son
    /// 200 bytes; reportarlos como línea larga sería un falso positivo.
    #[test]
    fn el_largo_de_linea_se_mide_en_caracteres() {
        let i = analizar(&format!("\"\"\"d\"\"\"\nx = '{}'\n", "á".repeat(100)));
        assert!(
            !codigos(&i).contains(&"E501"),
            "100 acentos son 200 bytes pero 100 caracteres: {:?}",
            codigos(&i)
        );
    }

    /// **El error de sintaxis corta**: sin AST no hay nada más que decir.
    #[test]
    fn un_error_de_sintaxis_es_el_unico_issue() {
        let i = analizar("x = 1\ny = 2\nassert z\ndef roto(:\n    pass\n");
        assert_eq!(codigos(&i), vec!["E001"], "solo el error, no las otras reglas");
        assert_eq!(i[0]["line"], 4, "y con la linea convertida del byte offset");
        assert_eq!(i[0]["severity"], "error");
    }

    #[test]
    fn la_deduplicacion_colapsa_repetidos_y_ordena_por_linea() {
        let entrada = vec![
            issue(9, 0, "info", "C002", "print() — usa logging".to_string()),
            issue(2, 0, "warning", "W001", "assert puede desactivarse con -O".to_string()),
            issue(9, 0, "info", "C002", "print() — usa logging".to_string()),
            issue(5, 0, "warning", "W003", "Variable global: x".to_string()),
        ];
        let salida = deduplicar(entrada);
        assert_eq!(salida.len(), 3, "el repetido de la linea 9 colapsa");
        let lineas: Vec<i64> = salida.iter().map(|i| i["line"].as_i64().unwrap()).collect();
        assert_eq!(lineas, vec![2, 5, 9]);
    }

    /// Dos issues en la misma línea con mensajes distintos NO colapsan.
    #[test]
    fn mismos_renglones_con_mensajes_distintos_sobreviven() {
        let entrada = vec![
            issue(3, 0, "warning", "W001", "assert puede desactivarse con -O".to_string()),
            issue(3, 0, "info", "C002", "print() — usa logging".to_string()),
        ];
        assert_eq!(deduplicar(entrada).len(), 2);
    }

    // ── el endpoint de un archivo ─────────────────────────────────────

    fn pedir(filename: &str, contenido: &str) -> Value {
        un_archivo(filename, contenido, &herramientas_por_defecto())
    }

    #[test]
    fn el_shape_es_el_que_el_panel_espera() {
        let r = pedir("a.py", "def f():\n    return 1\n");
        for clave in [
            "filename",
            "ts",
            "issues",
            "metrics",
            "complexity",
            "maintainability",
            "halstead",
            "raw_stats",
            "tools_used",
        ] {
            assert!(r.get(clave).is_some(), "falta {clave}");
        }
        assert_eq!(r["filename"], "a.py");
        let usadas: Vec<&str> =
            r["tools_used"].as_array().unwrap().iter().map(|t| t.as_str().unwrap()).collect();
        assert_eq!(usadas, vec!["ast", "complexity"]);
    }

    /// **`flake8` y `pylint` se ignoran si el cliente los pide.** El frontend
    /// viejo los manda en `tools`; un nombre de herramienta que ya no existe
    /// no es un error del pedido, y `tools_used` dice la verdad.
    #[test]
    fn pedir_flake8_o_pylint_no_falla_ni_miente() {
        let tools: Vec<String> =
            ["ast", "flake8", "pylint", "complexity"].iter().map(|s| s.to_string()).collect();
        let r = un_archivo("a.py", "def f():\n    return 1\n", &tools);
        let usadas: Vec<&str> =
            r["tools_used"].as_array().unwrap().iter().map(|t| t.as_str().unwrap()).collect();
        assert_eq!(usadas, vec!["ast", "complexity"], "no declara lo que no corrio");
        assert!(r.get("error").is_none());
    }

    #[test]
    fn la_complejidad_sale_del_motor() {
        let r = pedir("a.py", "def f(xs):\n    \"\"\"d\"\"\"\n    for x in xs:\n        print(x)\n");
        assert_eq!(r["complexity"].as_array().unwrap().len(), 1);
        assert_eq!(r["complexity"][0]["name"], "f");
        assert!(r["raw_stats"]["loc"].as_u64().unwrap() >= 4);
        assert!(r["maintainability"].is_number(), "el MI tiene que venir");
    }

    #[test]
    fn un_json_invalido_se_reporta_con_linea_y_columna() {
        let r = pedir("datos.json", "{\n  \"a\": 1,\n  \"b\": \n}\n");
        let i = r["issues"].as_array().unwrap();
        assert_eq!(i.len(), 1);
        assert_eq!(i[0]["code"], "E999");
        assert_eq!(i[0]["tool"], "json");
        assert!(i[0]["line"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn un_json_valido_no_reporta_nada() {
        let r = pedir("datos.json", "{\"a\": [1, 2, 3]}\n");
        assert_eq!(r["issues"].as_array().unwrap().len(), 0);
    }

    /// Un archivo que no es `.py` ni `.json` devuelve el shape vacío sin
    /// inventar issues: no hay reglas para él en este tier.
    #[test]
    fn una_extension_sin_reglas_devuelve_el_shape_vacio() {
        let r = pedir("estilos.css", "body { color: red }\n");
        assert_eq!(r["issues"].as_array().unwrap().len(), 0);
        assert_eq!(r["tools_used"].as_array().unwrap().len(), 0);
        assert!(r["maintainability"].is_null());
    }

    // ── el endpoint de proyecto ───────────────────────────────────────

    #[test]
    fn el_proyecto_analiza_cada_archivo_que_le_pasan() {
        let r = analizar_proyecto(&ProyectoRequest {
            files: vec![
                ArchivoDeCodigo {
                    filename: "a.py".into(),
                    content: "def f():\n    assert 1\n".into(),
                },
                ArchivoDeCodigo { filename: "b.py".into(), content: "print('x')\n".into() },
            ],
            project_id: None,
            tools: herramientas_por_defecto(),
        });
        let archivos = r["files"].as_object().unwrap();
        assert_eq!(archivos.len(), 2);
        let de_a = codigos(archivos["a.py"]["issues"].as_array().unwrap());
        assert!(de_a.contains(&"W001"), "{de_a:?}");
        let de_b = codigos(archivos["b.py"]["issues"].as_array().unwrap());
        assert!(de_b.contains(&"C002"), "{de_b:?}");
    }

    #[test]
    fn un_proyecto_sin_archivos_devuelve_un_mapa_vacio() {
        let r = analizar_proyecto(&ProyectoRequest {
            files: vec![],
            project_id: None,
            tools: herramientas_por_defecto(),
        });
        assert_eq!(r["files"].as_object().unwrap().len(), 0);
    }

    /// Un `project_id` que no existe da un mapa vacío, no un error: es el
    /// mismo comportamiento que el router (leía `[]` si el directorio no
    /// estaba) y el que los tests de Python fijaban.
    #[test]
    fn un_project_id_inexistente_devuelve_vacio_sin_fallar() {
        let r = analizar_proyecto(&ProyectoRequest {
            files: vec![],
            project_id: Some("no-existe-de-ninguna-manera".into()),
            tools: herramientas_por_defecto(),
        });
        assert_eq!(r["files"].as_object().unwrap().len(), 0);
    }

    /// **Si vienen los dos, gana `project_id`** — mismo criterio que
    /// `/analyze/graph/project`. Se prueba con un id que no existe: si
    /// `files` se usara como respaldo, acá aparecería un archivo.
    #[test]
    fn si_vienen_los_dos_gana_el_project_id() {
        let r = analizar_proyecto(&ProyectoRequest {
            files: vec![ArchivoDeCodigo {
                filename: "ignorado.py".into(),
                content: "assert 1\n".into(),
            }],
            project_id: Some("no-existe-de-ninguna-manera".into()),
            tools: herramientas_por_defecto(),
        });
        assert_eq!(
            r["files"].as_object().unwrap().len(),
            0,
            "`files` no es un respaldo de un project_id que no resuelve"
        );
    }

    /// Y un `project_id` con traversal tampoco: `ruta_de_proyecto` lo
    /// rechaza antes de tocar el disco.
    #[test]
    fn un_project_id_con_traversal_no_lee_nada() {
        let r = analizar_proyecto(&ProyectoRequest {
            files: vec![],
            project_id: Some("../../etc".into()),
            tools: herramientas_por_defecto(),
        });
        assert_eq!(r["files"].as_object().unwrap().len(), 0);
    }
}
