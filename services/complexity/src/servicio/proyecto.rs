//! **El análisis de proyecto entero — Fase 35, portado desde Python.**
//!
//! Esto vivía en `apps/api/routers/static_analysis.py::compute_project_analysis`,
//! y no era análisis: era **orquestación**. Encadena cuatro cosas que el motor
//! ya hacía —escanear el proyecto, armar el grafo circular, sacar los smells
//! de arquitectura y calcular los cuatro scores de salud— y agrega ~30
//! agregaciones sobre los resultados por archivo.
//!
//! **Por qué sobre `Value` y no sobre tipos propios.** Cada parser devuelve un
//! shape distinto (un `.ads` trae `specs`, un `.rs` trae `trust_surface`, un
//! `.css` trae `css_findings`), y el agregador sólo los aplana con su archivo
//! de origen. Tipar eso obligaría a un enum de 13 variantes que no aporta
//! nada: lo que importa es que **el JSON que sale sea byte a byte el que el
//! panel ya consume**, y traducir sobre `Value` es lo que hace imposible que
//! se desincronice al portarlo.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::flujo::graph::{self, FileSummary, ImportRef};
use crate::servicio::salud::{self, EntradaSalud};

#[derive(Deserialize, Default)]
pub struct ProyectoRequest {
    #[serde(default)]
    pub project_id: Option<String>,
    /// Archivos en crudo, para el camino sin `project_id`.
    #[serde(default)]
    pub files: Vec<ArchivoCrudo>,
}

#[derive(Deserialize)]
pub struct ArchivoCrudo {
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub content: String,
}

#[derive(Serialize)]
pub struct ProyectoResult(pub Value);

/// Lista de un campo de cada archivo, aplanada y con su `file` de origen.
///
/// Es el patrón que el agregador repite diez veces (security, structural,
/// naming, css, html, manifest, control, locality, modernization…): cada
/// hallazgo sale del archivo que lo produjo y necesita saber de cuál vino.
fn aplanar_con_archivo(resultados: &[Value], campo: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for r in resultados {
        let archivo = r.get("filename").and_then(Value::as_str).unwrap_or("");
        let Some(items) = r.get(campo).and_then(Value::as_array) else {
            continue;
        };
        for it in items {
            let mut m = it.as_object().cloned().unwrap_or_default();
            m.insert("file".to_string(), json!(archivo));
            out.push(Value::Object(m));
        }
    }
    out
}

fn cuantos(resultados: &[Value], campo: &str) -> usize {
    resultados
        .iter()
        .filter_map(|r| r.get(campo).and_then(Value::as_array))
        .map(Vec::len)
        .sum()
}

/// `FileSummary` es lo que el Graph Engine necesita: nombre, lenguaje,
/// cantidad de funciones y los imports con su línea.
pub fn resumen_para_el_grafo(resultados: &[Value]) -> Vec<FileSummary> {
    resultados
        .iter()
        .map(|r| FileSummary {
            filename: r.get("filename").and_then(Value::as_str).unwrap_or("").to_string(),
            language: r.get("language").and_then(Value::as_str).unwrap_or("?").to_string(),
            functions: r.get("functions").and_then(Value::as_array).map_or(0, Vec::len),
            imports: r
                .get("imports")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|i| ImportRef {
                            module: i.get("module").and_then(Value::as_str).unwrap_or("").to_string(),
                            line: i.get("line").and_then(Value::as_u64).unwrap_or(0) as u32,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            dead_code: r.get("dead_code").and_then(Value::as_array).map_or(0, Vec::len),
        })
        .collect()
}

/// **El mismo redondeo que hacía Python.** `round(x, 2)` de Python usa
/// banker's rounding; acá el valor sale de una división y se usa sólo para
/// mostrar y para alimentar `salud::calcular`, que ya replica el redondeo de
/// Python donde importa. Se deja explícito para que no parezca un descuido.
fn redondear_2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

pub fn analizar(resultados: Vec<Value>) -> Value {
    let total_funcs: usize = cuantos(&resultados, "functions");
    let total_classes: usize = cuantos(&resultados, "classes");
    let total_imports: usize = cuantos(&resultados, "imports");
    let total_dead: usize = cuantos(&resultados, "dead_code");

    // Distribución de Big-O sobre todas las funciones del proyecto.
    let mut big_o: Map<String, Value> = Map::new();
    for r in &resultados {
        for f in r.get("functions").and_then(Value::as_array).unwrap_or(&vec![]) {
            let k = f.get("big_o").and_then(Value::as_str).unwrap_or("?").to_string();
            let n = big_o.get(&k).and_then(Value::as_u64).unwrap_or(0) + 1;
            big_o.insert(k, json!(n));
        }
    }

    let wasm_candidates: Vec<Value> = resultados
        .iter()
        .filter(|r| r.get("wasm_hints").and_then(Value::as_array).is_some_and(|a| !a.is_empty()))
        .map(|r| json!({ "file": r.get("filename"), "hints": r.get("wasm_hints") }))
        .collect();

    // Top 10 por complejidad ciclomática, aplanado entre archivos.
    let mut todas_las_funciones: Vec<Value> = Vec::new();
    for r in &resultados {
        let archivo = r.get("filename").cloned().unwrap_or(json!(""));
        for f in r.get("functions").and_then(Value::as_array).unwrap_or(&vec![]) {
            todas_las_funciones.push(json!({
                "file": archivo,
                "name": f.get("name"),
                "line": f.get("line"),
                "complexity": f.get("complexity").and_then(Value::as_u64).unwrap_or(1),
                "big_o": f.get("big_o").and_then(Value::as_str).unwrap_or("?"),
            }));
        }
    }
    todas_las_funciones.sort_by_key(|f| {
        std::cmp::Reverse(f.get("complexity").and_then(Value::as_u64).unwrap_or(1))
    });
    let top_complex: Vec<Value> = todas_las_funciones.iter().take(10).cloned().collect();

    // Distribución real por lenguaje: archivos, líneas y funciones.
    let mut distribucion: Map<String, Value> = Map::new();
    for r in &resultados {
        let lang = r.get("language").and_then(Value::as_str).unwrap_or("?").to_string();
        let loc = r.get("loc").and_then(Value::as_u64).unwrap_or(0);
        let fns = r.get("functions").and_then(Value::as_array).map_or(0, Vec::len) as u64;
        let e = distribucion
            .entry(lang)
            .or_insert_with(|| json!({ "files": 0, "loc": 0, "functions": 0 }));
        let o = e.as_object_mut().expect("entrada de distribución");
        for (k, v) in [("files", 1), ("loc", loc), ("functions", fns)] {
            let actual = o.get(k).and_then(Value::as_u64).unwrap_or(0);
            o.insert(k.to_string(), json!(actual + v));
        }
    }
    let total_loc: u64 = distribucion
        .values()
        .map(|v| v.get("loc").and_then(Value::as_u64).unwrap_or(0))
        .sum();

    let security = aplanar_con_archivo(&resultados, "security_findings");
    let structural = aplanar_con_archivo(&resultados, "structural_smells");
    let naming = aplanar_con_archivo(&resultados, "naming_smells");
    let css = aplanar_con_archivo(&resultados, "css_findings");
    let html = aplanar_con_archivo(&resultados, "html_findings");
    let manifest = aplanar_con_archivo(&resultados, "manifest_findings");
    let control = aplanar_con_archivo(&resultados, "control_findings");
    let locality = aplanar_con_archivo(&resultados, "locality_findings");

    // Modernización: los candidatos viven un nivel más adentro.
    let mut modernization: Vec<Value> = Vec::new();
    for r in &resultados {
        let archivo = r.get("filename").and_then(Value::as_str).unwrap_or("");
        let Some(cands) = r.get("modernization").and_then(|m| m.get("candidates")).and_then(Value::as_array)
        else {
            continue;
        };
        for c in cands {
            let mut m = c.as_object().cloned().unwrap_or_default();
            m.insert("file".to_string(), json!(archivo));
            modernization.push(Value::Object(m));
        }
    }
    // Rollup por archivo: total, por confianza y por patrón.
    let mut por_archivo: Map<String, Value> = Map::new();
    for c in &modernization {
        let archivo = c.get("file").and_then(Value::as_str).unwrap_or("").to_string();
        let e = por_archivo.entry(archivo.clone()).or_insert_with(|| {
            json!({ "file": archivo, "total": 0, "high": 0, "medium": 0, "patterns": {} })
        });
        let o = e.as_object_mut().expect("entrada de rollup");
        let total = o.get("total").and_then(Value::as_u64).unwrap_or(0);
        o.insert("total".to_string(), json!(total + 1));
        if let Some(conf) = c.get("confidence").and_then(Value::as_str) {
            let n = o.get(conf).and_then(Value::as_u64).unwrap_or(0);
            o.insert(conf.to_string(), json!(n + 1));
        }
        if let Some(pat) = c.get("pattern").and_then(Value::as_str) {
            let pats = o.get_mut("patterns").and_then(Value::as_object_mut).expect("patterns");
            let n = pats.get(pat).and_then(Value::as_u64).unwrap_or(0);
            pats.insert(pat.to_string(), json!(n + 1));
        }
    }
    let mut modernization_by_file: Vec<Value> = por_archivo.into_values().collect();
    modernization_by_file.sort_by_key(|e| {
        std::cmp::Reverse(e.get("total").and_then(Value::as_u64).unwrap_or(0))
    });

    let complejidades: Vec<u64> = resultados
        .iter()
        .flat_map(|r| r.get("functions").and_then(Value::as_array).cloned().unwrap_or_default())
        .map(|f| f.get("complexity").and_then(Value::as_u64).unwrap_or(1))
        .collect();
    let avg_complexity = if complejidades.is_empty() {
        0.0
    } else {
        redondear_2(complejidades.iter().sum::<u64>() as f64 / complejidades.len() as f64)
    };

    // ── Las tres piezas que el motor ya tenía ──────────────────────────
    // `FileSummary` no es `Clone` y las dos funciones lo consumen, asi que el
    // resumen se arma dos veces. Es barato —son cuatro campos por archivo— y
    // es preferible a hacer `Clone` un tipo publico solo por esto.
    let circular = graph::build_circular_graph(resumen_para_el_grafo(&resultados));
    let total_cycles = circular.summary.total_cycles;
    let arquitectura = graph::build_architecture_smells(resumen_para_el_grafo(&resultados));
    // Las entradas de ciclo se excluyen del score: ya pesan en
    // `total_cycles`, y contarlas dos veces castigaría el mismo hallazgo dos
    // veces. **El conteo del `summary` SÍ las incluye**, porque ahí la
    // pregunta es "cuántos hallazgos hay para mostrar", no un input de score.
    let coupling = arquitectura.iter().filter(|s| s.kind != "circular_dependency").count();

    let sev = |nivel: &str| {
        security.iter().filter(|f| f.get("severity").and_then(Value::as_str) == Some(nivel)).count()
    };
    let (alto, medio, bajo) = (sev("High"), sev("Medium"), sev("Low"));

    let health = salud::calcular(&EntradaSalud {
        security_high: alto,
        security_medium: medio,
        security_low: bajo,
        structural_smells: structural.len(),
        naming_smells: naming.len(),
        total_functions: total_funcs,
        total_classes,
        avg_complexity,
        total_cycles,
        coupling_smells: coupling,
        total_files: resultados.len(),
    });

    json!({
        "files": resultados,
        "summary": {
            "total_files": resultados.len(),
            "total_functions": total_funcs,
            "total_classes": total_classes,
            "total_imports": total_imports,
            "unused_imports": total_dead,
            "big_o_distribution": big_o,
            "wasm_candidates": wasm_candidates.len(),
            "security_findings": security.len(),
            "structural_smells": structural.len(),
            "naming_smells": naming.len(),
            "modernization_candidates": modernization.len(),
            "architecture_smells": arquitectura.len(),
            "css_findings": css.len(),
            "html_findings": html.len(),
            "manifest_findings": manifest.len(),
            "control_findings": control.len(),
            "locality_findings": locality.len(),
            "total_loc": total_loc,
        },
        "wasm_candidates": wasm_candidates,
        "security_findings": security,
        "structural_smells": structural,
        "naming_smells": naming,
        "modernization_candidates": modernization,
        "modernization_by_file": modernization_by_file,
        "architecture_smells": arquitectura,
        "css_findings": css,
        "html_findings": html,
        "manifest_findings": manifest,
        "control_findings": control,
        "locality_findings": locality,
        "top_complex_functions": top_complex,
        "language_distribution": distribucion,
        "health": health,
    })
}

/// Directorios que no se escanean nunca.
///
/// Venia de `project_service.py`, que lo mandaba por HTTP en cada escaneo.
/// Aca es una constante: el motor es quien escanea, asi que la lista es suya.
pub const DIRECTORIOS_IGNORADOS: &[&str] = &[
    "__pycache__", ".git", ".svn", "node_modules", ".venv", "venv", ".idea", ".vscode", "dist",
    "build", ".next", ".nuxt", "target", ".pytest_cache", ".mypy_cache", "coverage", ".cache",
];

/// Donde viven los proyectos subidos. Mismo patron que `log_dir()`: variable
/// de entorno con un default relativo.
pub fn directorio_de_subidas() -> std::path::PathBuf {
    std::path::PathBuf::from(
        std::env::var("SYTHRALL_UPLOADS_DIR").unwrap_or_else(|_| "uploads/projects".to_string()),
    )
}

/// El nombre del archivo donde cada proyecto subido cachea su metadata.
pub const ARCHIVO_DE_METADATA: &str = ".sythrall-meta.json";

/// **Qué proyectos hay en disco y cómo se llama cada uno.**
///
/// El `Option<String>` es deliberado: un proyecto puede existir **sin**
/// nombre guardado — la metadata de los subidos antes de que existiera ese
/// campo no lo trae. Eso es distinto de que el proyecto no exista, y el
/// panorama del historial depende de la diferencia (`exists` sale de la
/// clave, `project_name` del valor).
///
/// **No recorre el árbol de ningún proyecto.** `list_projects` en Python
/// llamaba a `get_project_info_cached`, que ante una metadata ausente
/// recorría el disco entero para recalcularla — y con muchos proyectos
/// acumulados eso bloqueaba el servidor en cada llamada. Para resolver
/// nombres no hace falta ningún dato de ese recálculo: si el archivo no
/// está, el nombre es `None` y listo.
pub fn nombres_de_proyectos() -> std::collections::BTreeMap<String, Option<String>> {
    let mut out = std::collections::BTreeMap::new();
    let Ok(entradas) = std::fs::read_dir(directorio_de_subidas()) else {
        return out;
    };
    for entrada in entradas.flatten() {
        if !entrada.path().is_dir() {
            continue;
        }
        let id = entrada.file_name().to_string_lossy().to_string();
        let nombre = std::fs::read_to_string(entrada.path().join(ARCHIVO_DE_METADATA))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.get("project_name").and_then(Value::as_str).map(str::to_string));
        out.insert(id, nombre);
    }
    out
}

/// Las extensiones y los nombres exactos que algun plugin reclama.
///
/// **Sale del manifest, no de una lista aparte.** `project_service.py` tenia
/// su propio `PARSEABLE_EXTENSIONS` que habia que acordarse de actualizar
/// cada vez que entraba un lenguaje — y ya se habia desincronizado antes.
pub fn reclamos_de_los_plugins() -> (Vec<String>, Vec<String>) {
    let manifests = crate::servicio::plugin::builtin_manifests();
    let mut exts: Vec<String> = manifests.iter().flat_map(|m| m.extensions.iter().map(|e| (*e).to_string())).collect();
    let mut nombres: Vec<String> = manifests.iter().flat_map(|m| m.filenames.iter().map(|f| (*f).to_string())).collect();
    exts.sort();
    exts.dedup();
    nombres.sort();
    nombres.dedup();
    (exts, nombres)
}

/// **De donde salen los resultados por archivo**, que es lo unico que cambia
/// entre analizar un proyecto subido y analizar un punado de archivos
/// sueltos. Compartido con `servicio::grafo`, que necesita exactamente lo
/// mismo antes de construir cualquier grafo.
pub fn resultados_de(project_id: Option<&str>, files: &[ArchivoCrudo]) -> Vec<Value> {
    match project_id.filter(|s| !s.is_empty()) {
        Some(id) => {
            // **El id no se interpola en una ruta sin mirarlo.** Un `..` aca
            // leeria fuera del directorio de subidas.
            if id.contains("..") || id.contains('/') || id.contains('\\') {
                return Vec::new();
            }
            let dir = directorio_de_subidas().join(id);
            if !dir.is_dir() {
                Vec::new()
            } else {
                let (extensions, filenames) = reclamos_de_los_plugins();
                crate::servicio::scanner::scan_and_parse_project(&crate::servicio::scanner::ScanRequest {
                    project_dir: dir.to_string_lossy().to_string(),
                    extensions,
                    ignored_dirs: DIRECTORIOS_IGNORADOS.iter().map(|d| (*d).to_string()).collect(),
                    filenames,
                })
            }
        }
        None => files
            .iter()
            .map(|f| {
                let mut v = crate::servicio::scanner::parse_one(f.filename.clone(), &f.content);
                // Mismo criterio que `wc -l`, igual que hacia Python para este
                // camino: el escaneo por disco ya trae su propio `loc`.
                if let Some(o) = v.as_object_mut() {
                    o.insert("loc".to_string(), json!(f.content.matches('\n').count()));
                }
                v
            })
            .collect(),
    }
}

/// El punto de entrada que reemplaza a `compute_project_analysis`.
pub fn analizar_proyecto(req: &ProyectoRequest) -> Value {
    analizar(resultados_de(req.project_id.as_deref(), &req.files))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archivo(nombre: &str, lang: &str, loc: u64, fns: Value, extra: Value) -> Value {
        let mut m = json!({
            "filename": nombre, "language": lang, "loc": loc, "functions": fns,
            "classes": [], "imports": [], "dead_code": [],
        })
        .as_object()
        .unwrap()
        .clone();
        for (k, v) in extra.as_object().unwrap() {
            m.insert(k.clone(), v.clone());
        }
        Value::Object(m)
    }

    #[test]
    fn un_proyecto_vacio_no_revienta_y_da_ceros() {
        let r = analizar(vec![]);
        assert_eq!(r["summary"]["total_files"], 0);
        assert_eq!(r["summary"]["total_functions"], 0);
        assert_eq!(r["summary"]["total_loc"], 0);
        assert!(r["health"]["security"]["score"].is_number(), "los scores salen igual");
    }

    /// **Cada hallazgo tiene que saber de qué archivo vino.** Es lo que el
    /// panel usa para mandarte al archivo al hacer clic; sin el `file` la
    /// lista es inútil.
    #[test]
    fn los_hallazgos_se_aplanan_con_su_archivo_de_origen() {
        let a = archivo("a.py", "python", 10, json!([]), json!({
            "security_findings": [{"severity": "High", "kind": "x"}]
        }));
        let b = archivo("b.py", "python", 5, json!([]), json!({
            "security_findings": [{"severity": "Low", "kind": "y"}]
        }));
        let r = analizar(vec![a, b]);
        let f = r["security_findings"].as_array().unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(f[0]["file"], "a.py");
        assert_eq!(f[1]["file"], "b.py");
    }

    #[test]
    fn la_distribucion_por_lenguaje_suma_archivos_lineas_y_funciones() {
        let a = archivo("a.py", "python", 100, json!([{"name": "f", "line": 1, "complexity": 3}]), json!({}));
        let b = archivo("b.py", "python", 50, json!([{"name": "g", "line": 1, "complexity": 1}]), json!({}));
        let c = archivo("c.rs", "rust", 7, json!([]), json!({}));
        let r = analizar(vec![a, b, c]);
        assert_eq!(r["language_distribution"]["python"]["files"], 2);
        assert_eq!(r["language_distribution"]["python"]["loc"], 150);
        assert_eq!(r["language_distribution"]["python"]["functions"], 2);
        assert_eq!(r["language_distribution"]["rust"]["files"], 1);
        assert_eq!(r["summary"]["total_loc"], 157);
    }

    /// El widget del Dashboard muestra las diez peores; el orden es el punto.
    #[test]
    fn las_funciones_mas_complejas_salen_primero_y_son_diez() {
        let fns: Vec<Value> = (1..=15)
            .map(|i| json!({"name": format!("f{i}"), "line": i, "complexity": i}))
            .collect();
        let r = analizar(vec![archivo("a.py", "python", 1, json!(fns), json!({}))]);
        let top = r["top_complex_functions"].as_array().unwrap();
        assert_eq!(top.len(), 10);
        assert_eq!(top[0]["complexity"], 15);
        assert_eq!(top[9]["complexity"], 6);
    }

    #[test]
    fn la_distribucion_de_big_o_cuenta_por_clase() {
        let fns = json!([
            {"name": "a", "line": 1, "complexity": 1, "big_o": "O(n)"},
            {"name": "b", "line": 2, "complexity": 1, "big_o": "O(n)"},
            {"name": "c", "line": 3, "complexity": 1, "big_o": "O(n²)"},
        ]);
        let r = analizar(vec![archivo("a.py", "python", 1, fns, json!({}))]);
        assert_eq!(r["summary"]["big_o_distribution"]["O(n)"], 2);
        assert_eq!(r["summary"]["big_o_distribution"]["O(n²)"], 1);
    }

    /// **El rollup de modernización agrupa por archivo, confianza y patrón.**
    #[test]
    fn el_rollup_de_modernizacion_agrupa_por_archivo() {
        let a = archivo("a.c", "c", 1, json!([]), json!({
            "modernization": { "candidates": [
                {"pattern": "malloc", "confidence": "high"},
                {"pattern": "malloc", "confidence": "medium"},
            ]}
        }));
        let r = analizar(vec![a]);
        let roll = &r["modernization_by_file"].as_array().unwrap()[0];
        assert_eq!(roll["file"], "a.c");
        assert_eq!(roll["total"], 2);
        assert_eq!(roll["high"], 1);
        assert_eq!(roll["medium"], 1);
        assert_eq!(roll["patterns"]["malloc"], 2);
    }

    /// **El conteo del summary y el input del score NO son el mismo número**,
    /// y la diferencia es deliberada: los ciclos ya pesan en `total_cycles`,
    /// así que contarlos otra vez como acoplamiento castigaría el mismo
    /// hallazgo dos veces. El summary sí los incluye, porque ahí la pregunta
    /// es cuántos hallazgos hay para mostrar.
    #[test]
    fn los_ciclos_cuentan_en_el_summary_pero_no_dos_veces_en_el_score() {
        // Dos archivos que se importan mutuamente: un ciclo.
        let a = json!({
            "filename": "a.py", "language": "python", "loc": 10, "functions": [],
            "classes": [], "dead_code": [], "imports": [{"module": "b.py", "line": 1}],
        });
        let b = json!({
            "filename": "b.py", "language": "python", "loc": 10, "functions": [],
            "classes": [], "dead_code": [], "imports": [{"module": "a.py", "line": 1}],
        });
        let r = analizar(vec![a, b]);
        let total = r["summary"]["architecture_smells"].as_u64().unwrap();
        let del_score = r["health"]["architecture"]["smells"].as_u64().unwrap();
        assert!(total >= del_score, "el summary incluye los ciclos y el score no: {total} vs {del_score}");
    }
}
