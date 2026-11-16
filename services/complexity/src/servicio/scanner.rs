//! Fase 18 — Project Scanner: primera vez que este sidecar lee del disco
//! directamente, en vez de recibir contenido de archivo por HTTP. Justificado
//! solo para este caso — analizar un proyecto entero — porque colapsa lo que
//! hoy son N llamadas HTTP (una por archivo, `read_project_files` en Python +
//! `asyncio.gather(parse_file(...) for f in files)`) en 1 sola por proyecto:
//! Python resuelve/valida el directorio (path traversal, etc. — sigue siendo
//! 100% su responsabilidad, sin duplicar esa lógica acá) y le pasa a Rust un
//! path absoluto YA validado; este módulo no hace ninguna validación de
//! seguridad nueva, solo camina y lee archivos de un directorio de confianza.
//!
//! `parse_one` reproduce, en Rust, el mismo dispatch por extensión que
//! `parse_file()` en `static_parser.py`, y el mismo re-shape final que sus
//! wrappers (`_parse_c_cpp_via_rust`/`_parse_js_ts_via_rust`/`_parse_python`)
//! ya hacen sobre la respuesta cruda del sidecar — para que el llamador
//! Python no tenga que tocar esa pieza, solo recibir el resultado ya armado
//! y agregarlo (los totales/distribuciones de `parse_project` en
//! `static_analysis.py` siguen siendo Python, pura agregación sobre esto).

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::lenguajes::{
    adaparse, asmparse, cparse, cssparse, dparse, fparse, htmlparse, jsts, manifest, notebook,
    rsparse,
};
use crate::servicio::rich;

#[derive(Deserialize)]
pub struct ScanRequest {
    pub project_dir: String,
    pub extensions: Vec<String>,
    pub ignored_dirs: Vec<String>,
    /// Nombres de archivo exactos a incluir aunque su extensión no esté en
    /// `extensions` — los manifests de dependencias (`package.json`,
    /// `requirements.txt`) se reclaman por nombre, no por extensión.
    /// Incluir `.json` entero en su lugar arrastraría todo archivo JSON del
    /// proyecto solo para llegar a uno. `#[serde(default)]` para que un
    /// caller viejo que no manda el campo siga funcionando igual que antes.
    #[serde(default)]
    pub filenames: Vec<String>,
}

/// Camina el árbol y parsea cada archivo encontrado — mismo criterio de
/// filtrado que `read_project_files` en `project_service.py`: extensión
/// permitida, directorio no ignorado (exacto o `*sufijo`, para casos como
/// `*.egg-info`), contenido no vacío. Orden determinístico (alfabético por
/// ruta relativa) para que la salida no dependa del orden de `read_dir`.
pub fn scan_and_parse_project(req: &ScanRequest) -> Vec<Value> {
    collect_project_files(req).into_iter().map(|(filename, content)| parse_one(filename, &content)).collect()
}

/// Los archivos del proyecto ya leídos, sin parsear — el paso previo que
/// `scan_and_parse_project` hacía en línea y que `cssusage.rs` necesita
/// crudo, porque cruza CSS contra el TEXTO del marcado/scripts, no contra su
/// AST.
pub fn collect_project_files(req: &ScanRequest) -> Vec<(String, String)> {
    let root = Path::new(&req.project_dir);
    let mut files = Vec::new();
    walk_dir(root, root, &req.extensions, &req.ignored_dirs, &req.filenames, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

/// `pub(crate)` desde 2026-09-02: `cssusage.rs` necesita EXACTAMENTE el mismo
/// criterio de recorrido y filtrado (extensiones permitidas, directorios
/// ignorados, contenido no vacío, orden determinístico). Duplicarlo sería
/// garantizar que los dos análisis miren conjuntos de archivos distintos con
/// el tiempo.
pub(crate) fn walk_dir(
    root: &Path,
    dir: &Path,
    extensions: &[String],
    ignored_dirs: &[String],
    filenames: &[String],
    out: &mut Vec<(String, String)>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else { continue };
        let path = entry.path();
        if file_type.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if is_ignored(name, ignored_dirs) {
                continue;
            }
            walk_dir(root, &path, extensions, ignored_dirs, filenames, out);
        } else if file_type.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let by_ext = extensions.iter().any(|e| e.eq_ignore_ascii_case(&ext));
            let by_name = filenames.iter().any(|f| f.eq_ignore_ascii_case(name));
            if !by_ext && !by_name {
                continue;
            }
            // `errors="replace"` de Python: bytes inválidos no descartan el
            // archivo, se reemplazan (U+FFFD) — `read_to_string` en cambio
            // fallaría entero ante el primer byte no-UTF8.
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let content = String::from_utf8_lossy(&bytes).into_owned();
            if content.trim().is_empty() {
                continue;
            }
            let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            out.push((rel, content));
        }
    }
}

fn is_ignored(name: &str, ignored_dirs: &[String]) -> bool {
    ignored_dirs.iter().any(|d| match d.strip_prefix('*') {
        Some(suffix) => name.ends_with(suffix),
        None => d == name,
    })
}

/// **Resultado de analizar UN archivo, tal como viaja al agregador.**
///
/// Nace de un bug que apareció **cinco veces**: cada brazo del despacho de
/// abajo armaba su JSON a mano con `json!({...})`, y la agregación de
/// `static_analysis.py` lee con `.get(clave, [])`. O sea que **olvidarse de
/// una clave no rompe nada visible**: el hallazgo simplemente nunca aparece y
/// el resultado se ve sano. `control_findings` anduvo archivo por archivo
/// desde el día uno y en un escaneo de proyecto nunca llegó, sin que nada lo
/// reportara.
///
/// Los tests de "CLAVES, no lenguajes" que hay más abajo son una red
/// **después** del hecho: atrapan el olvido si alguien se acuerda de
/// escribirlos. Esto lo atrapa **antes**, en el compilador.
///
/// **Por qué son dos structs y no uno.** `Nucleo` **no tiene `Default` a
/// propósito**: agregarle un campo rompe la compilación de los nueve brazos y
/// obliga a que cada lenguaje diga qué pone ahí. Si el tipo fuera uno solo con
/// `..Default::default()`, olvidarse de un campo del núcleo lo dejaría en
/// vacío **en silencio** — exactamente el bug que esto existe para impedir.
/// `Extras` sí tiene `Default`, porque ahí lo normal es no llenar nada.
///
/// **Alcance de lo que esto arregla, dicho para no venderlo de más**: unifica
/// la SALIDA, no la entrada. Cada lenguaje sigue teniendo su propio parser y
/// su propio árbol (rustpython para Python, tree-sitter para el resto, con
/// gramáticas que ni siquiera coinciden entre C y C++). Un IR de entrada —un
/// AST normalizado sobre el que corran todos los analizadores— es otro
/// trabajo, mucho más grande, y sigue abierto.
#[derive(Serialize)]
struct ArchivoAnalizado {
    #[serde(flatten)]
    nucleo: Nucleo,
    #[serde(flatten)]
    extras: Extras,
}

/// Lo que TODO lenguaje tiene que llenar, aunque sea vacío. Sin `Default`:
/// ver el doc de arriba.
#[derive(Serialize)]
struct Nucleo {
    filename: String,
    language: String,
    functions: Value,
    classes: Value,
    imports: Value,
    exports: Value,
    dead_code: Value,
    call_graph: Value,
    wasm_hints: Value,
    security_findings: Value,
    structural_smells: Value,
    naming_smells: Value,
    control_findings: Value,
    summary: Value,
}

/// Lo específico de cada lenguaje: ausente del JSON cuando no aplica.
#[derive(Serialize, Default)]
struct Extras {
    /// Líneas físicas. Lo llena `parse_one`, no los brazos.
    loc: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    macros: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    interfaces: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    types: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    modernization: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    locality_findings: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    template_complexity: Option<Value>,
    /// Fase 34, Linea B — mapa de la superficie de confianza (Rust).
    #[serde(skip_serializing_if = "Option::is_none")]
    trust_surface: Option<Value>,
    /// Fase 34, Linea D — su dual: donde el programa tiro una prueba.
    #[serde(skip_serializing_if = "Option::is_none")]
    proof_surface: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    asm_syntax: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code_cells: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cell_ranges: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    execution_counts: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reproducibility: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    css_rules: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    css_findings: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    css_note: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    html_findings: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    html_note: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest_dependencies: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest_findings: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest_note: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Atajo para las listas vacías del núcleo, que es la mayor parte de lo que
/// escribe un lenguaje que no produce cierto análisis. Decirlo así es
/// distinto de olvidarse: queda escrito que ese lenguaje no lo produce.
fn vacio() -> Value {
    json!([])
}

/// **`pub` desde la Fase 35.** Este despachador por extension ya existia
/// para escanear proyectos; `static_parser.py` tenia su propia copia del
/// mismo mapa extension -> parser en 1.122 lineas. Ahora `/static/parse` usa
/// este, que es el mismo que usa el escaneo: una sola tabla de despacho.
pub fn parse_one(filename: String, content: &str) -> Value {
    let ext = Path::new(&filename).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    // Conteo físico de líneas (mismo criterio que Python's `content.count("\n")`,
    // "wc -l") — se manda de vuelta en vez de `content` para que
    // `language_distribution` en `static_analysis.py::parse_project` no
    // necesite el archivo entero de nuevo solo para esta cuenta.
    let loc = content.matches('\n').count();
    let mut analizado =
        manifest_result(&filename, content).unwrap_or_else(|| parse_one_inner(filename, ext.as_str(), content));
    analizado.extras.loc = loc;
    // `to_value` de un `Serialize` no puede fallar salvo por un tipo no
    // representable en JSON, y acá todos los campos ya son `Value`.
    serde_json::to_value(analizado).unwrap_or_else(|_| json!({}))
}

fn parse_one_inner(filename: String, ext: &str, content: &str) -> ArchivoAnalizado {
    match ext {
        "py" => {
            let r = rich::analyze_rich(content);
            ArchivoAnalizado {
                nucleo: Nucleo {
                    filename,
                    language: "python".into(),
                    functions: json!(r.functions),
                    classes: json!(r.classes),
                    imports: json!(r.imports),
                    exports: vacio(),
                    dead_code: json!(r.dead_code),
                    call_graph: json!(r.call_graph),
                    wasm_hints: json!(r.wasm_hints),
                    security_findings: json!(r.security_findings),
                    structural_smells: json!(r.structural_smells),
                    naming_smells: json!(r.naming_smells),
                    control_findings: json!(r.control_findings),
                    summary: json!({
                        "total_functions": r.summary.total_functions,
                        "total_classes": r.summary.total_classes,
                        "total_imports": r.summary.total_imports,
                        "unused_imports": r.dead_code.len(),
                        "avg_complexity": r.summary.avg_complexity,
                        "max_loc_function": r.summary.max_loc_function,
                        "security_findings": r.security_findings.len(),
                        "structural_smells": r.structural_smells.len(),
                        "naming_smells": r.naming_smells.len(),
                    }),
                },
                extras: Extras {
                    modernization: Some(json!(r.modernization)),
                    proof_surface: Some(json!(r.proof_surface)),
                    ..Default::default()
                },
            }
        }
        "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" => {
            let lang = if ext == "c" { "c" } else { "cpp" };
            let parsed = if lang == "c" { cparse::parse_c(content) } else { cparse::parse_cpp(content) };
            match parsed {
                Some(result) => {
                    let mut summary = json!({
                        "total_functions": result.functions.len(),
                        "total_includes": result.imports.len(),
                    });
                    if lang == "c" {
                        summary["total_structs"] = json!(result.classes.len());
                        summary["total_macros"] = json!(result.macros.len());
                    } else {
                        summary["total_classes"] = json!(result.classes.len());
                    }
                    ArchivoAnalizado {
                        nucleo: Nucleo {
                            filename,
                            language: lang.into(),
                            functions: json!(result.functions),
                            classes: json!(result.classes),
                            imports: json!(result.imports),
                            exports: vacio(),
                            dead_code: vacio(),
                            call_graph: json!(result.call_graph),
                            wasm_hints: json!(result.wasm_hints),
                            security_findings: json!(result.security_findings),
                            structural_smells: json!(result.structural_smells),
                            // C/C++ no tiene analisis de nombres propio: este
                            // `[]` SI es honesto, no un olvido.
                            naming_smells: vacio(),
                            // Las precondiciones de control optimo son de
                            // Python (scipy/control): vacio honesto.
                            control_findings: vacio(),
                            summary,
                        },
                        extras: Extras {
                            macros: Some(json!(result.macros)),
                            // `cparse` ya calculo esto en esta misma llamada
                            // (`modernization.rs`, 12 patrones).
                            modernization: Some(json!(result.modernization)),
                            // Fase 32 — localidad espacial de acceso.
                            locality_findings: Some(json!(result.locality)),
                            // Fase 34, Linea A — metricas de forma de la
                            // metaprogramacion. Vacias para C: no tiene templates.
                            template_complexity: Some(json!(result.templates)),
                            ..Default::default()
                        },
                    }
                }
                None => unsupported(&filename, &format!(".{lang}"), "parseo tree-sitter falló"),
            }
        }
        "js" | "jsx" | "ts" | "tsx" => {
            let is_ts = ext == "ts" || ext == "tsx";
            let lang = if is_ts { "typescript" } else { "javascript" };
            let result = jsts::parse_js_ts(content, is_ts);
            ArchivoAnalizado {
                nucleo: Nucleo {
                    filename,
                    language: lang.into(),
                    functions: json!(result.functions),
                    classes: json!(result.classes),
                    imports: json!(result.imports),
                    exports: json!(result.exports),
                    dead_code: json!(result.dead_code),
                    call_graph: json!(result.call_graph),
                    wasm_hints: json!(result.wasm_hints),
                    // Estos dos campos de `JsTsResult` existen EXACTAMENTE para
                    // arreglar el "100/100 por ausencia de analisis" de las Fases
                    // 21 y 22. Mandar `[]` fijo aca lo dejaba sin arreglar en la
                    // ruta de proyecto — la unica donde ese score se calcula.
                    security_findings: json!(result.security_findings),
                    structural_smells: json!(result.structural_smells),
                    // JS/TS no tiene analisis de nombres: este `[]` si es honesto.
                    naming_smells: vacio(),
                    control_findings: vacio(),
                    summary: json!({
                        "total_functions": result.functions.len(),
                        "total_classes": result.classes.len(),
                        "total_imports": result.imports.len(),
                        "total_exports": result.exports.len(),
                        "total_interfaces": result.interfaces.len(),
                        "unused_imports": result.dead_code.len(),
                        "avg_complexity": result.avg_complexity,
                        "security_findings": result.security_findings.len(),
                        "structural_smells": result.structural_smells.len(),
                    }),
                },
                extras: Extras {
                    interfaces: Some(json!(result.interfaces)),
                    types: Some(json!(result.types)),
                    ..Default::default()
                },
            }
        }
        "rs" => {
            // Fase 34, Linea B — Rust como target. `parse_rust` devuelve
            // `Option` por el mismo motivo que Fortran: sin arbol no hay
            // resultado, y el archivo degrada en vez de romper el escaneo.
            let Some(r) = rsparse::parse_rust(content) else {
                return unsupported(&filename, ".rs", "parser Rust no disponible");
            };
            let resumen = json!({
                "total_functions": r.functions.len(),
                "total_uses": r.imports.len(),
                "unsafe_blocks": r.trust_surface.unsafe_blocks,
                "unsafe_fns": r.trust_surface.unsafe_fns,
                "unsafe_impls": r.trust_surface.unsafe_impls,
                "unwrap_calls": r.proof_surface.unwrap_calls,
                "expect_calls": r.proof_surface.expect_calls,
                "structural_smells": r.structural_smells.len(),
            });
            ArchivoAnalizado {
                nucleo: Nucleo {
                    filename,
                    language: "rust".into(),
                    functions: json!(r.functions),
                    // Fase 34 (2026-09-26) — struct/enum/trait/union con los
                    // metodos que sus `impl` les agregan. La clave se llama
                    // `classes` por el nombre que el panel ya usa; Rust no
                    // tiene clases, tiene tipos.
                    classes: json!(r.types),
                    imports: json!(r.imports),
                    exports: vacio(),
                    dead_code: vacio(),
                    // Fase 34 (2026-09-26) — Rust era el unico de los 8
                    // targets sin grafo de llamadas.
                    call_graph: json!(r.call_graph),
                    wasm_hints: vacio(),
                    // Sin taint tracking ni analisis de nombres todavia:
                    // vacios declarados, no olvidos.
                    security_findings: vacio(),
                    structural_smells: json!(r.structural_smells),
                    naming_smells: vacio(),
                    control_findings: vacio(),
                    summary: resumen,
                },
                extras: Extras {
                    trust_surface: Some(json!(r.trust_surface)),
                    proof_surface: Some(json!(r.proof_surface)),
                    ..Default::default()
                },
            }
        }
        "f" | "f90" | "f95" | "f03" | "f08" | "for" => {
            // `parse_fortran` devuelve `Option`: sin tree-sitter no hay
            // resultado, y ahi el archivo degrada igual que cualquier otro
            // no soportado en vez de romper el escaneo entero.
            let Some(r) = fparse::parse_fortran(content) else {
                return unsupported(&filename, ".f90", "parser Fortran no disponible");
            };
            ArchivoAnalizado {
                nucleo: Nucleo {
                    filename,
                    language: "fortran".into(),
                    functions: json!(r.functions),
                    classes: vacio(),
                    imports: json!(r.imports),
                    exports: vacio(),
                    dead_code: vacio(),
                    call_graph: json!(r.call_graph),
                    wasm_hints: vacio(),
                    structural_smells: json!(r.structural_smells),
                    // Fortran no tiene taint tracking ni analisis de nombres:
                    // estos dos SI son vacios honestos.
                    security_findings: vacio(),
                    naming_smells: vacio(),
                    control_findings: vacio(),
                    summary: json!({ "total_functions": r.functions.len(), "total_uses": r.imports.len() }),
                },
                extras: Extras {
                    macros: Some(vacio()),
                    // Fase 32 — localidad espacial de Fortran. Column-major,
                    // o sea la regla OPUESTA a la de C.
                    locality_findings: Some(json!(r.locality)),
                    ..Default::default()
                },
            }
        }
        "d" => {
            // Fase 34, Linea C, paso 1. `.d` tambien es la extension de los
            // archivos de dependencias que genera `gcc -MD`; esos no parsean
            // como D y degradan solos, sin resultados inventados.
            let Some(r) = dparse::parse_d(content) else {
                return unsupported(&filename, ".d", "parser D no disponible");
            };
            ArchivoAnalizado {
                nucleo: Nucleo {
                    filename,
                    language: "d".into(),
                    functions: json!(r.functions),
                    classes: vacio(),
                    imports: json!(r.imports),
                    exports: vacio(),
                    dead_code: vacio(),
                    call_graph: json!(r.call_graph),
                    wasm_hints: vacio(),
                    structural_smells: json!(r.structural_smells),
                    security_findings: vacio(),
                    naming_smells: vacio(),
                    control_findings: vacio(),
                    summary: json!({
                        "total_functions": r.functions.len(),
                        "total_unittests": r.unittests,
                        "total_imports": r.imports.len(),
                        "trusted_fns": r.trust_surface.trusted_fns
                    }),
                },
                extras: Extras {
                    macros: Some(vacio()),
                    trust_surface: Some(json!(r.trust_surface)),
                    ..Default::default()
                },
            }
        }
        "adb" | "ads" => {
            // Fase 34, Linea E, paso 1. Un `.ads` (especificacion) da cero
            // `functions` y N `specs`: es la verdad sobre ese archivo, no una
            // degradacion.
            let Some(r) = adaparse::parse_ada(content) else {
                return unsupported(&filename, ".adb", "parser Ada no disponible");
            };
            ArchivoAnalizado {
                nucleo: Nucleo {
                    filename,
                    language: "ada".into(),
                    functions: json!(r.functions),
                    // Un paquete Ada no es una clase, pero es la unidad que
                    // agrupa estado y operaciones, que es lo que el panel
                    // muestra en esa columna.
                    classes: json!(r.units),
                    imports: json!(r.imports),
                    exports: json!(r.specs),
                    dead_code: vacio(),
                    call_graph: json!(r.call_graph),
                    wasm_hints: vacio(),
                    structural_smells: json!(r.structural_smells),
                    // Sin taint tracking ni analisis de nombres para Ada:
                    // estos dos SI son vacios honestos.
                    security_findings: vacio(),
                    naming_smells: vacio(),
                    control_findings: vacio(),
                    summary: json!({
                        "total_functions": r.functions.len(),
                        "total_specs": r.specs.len(),
                        "total_units": r.units.len(),
                        "total_uses": r.imports.len()
                    }),
                },
                extras: Extras { macros: Some(vacio()), ..Default::default() },
            }
        }
        "s" | "asm" => {
            let r = asmparse::parse(content);
            ArchivoAnalizado {
                nucleo: Nucleo {
                    filename,
                    language: "assembly".into(),
                    functions: json!(r.procedures),
                    classes: vacio(),
                    imports: vacio(),
                    exports: vacio(),
                    dead_code: vacio(),
                    call_graph: json!(r.call_graph),
                    wasm_hints: vacio(),
                    structural_smells: json!(r.structural_smells),
                    // Assembly no tiene taint tracking ni analisis de nombres.
                    security_findings: vacio(),
                    naming_smells: vacio(),
                    control_findings: vacio(),
                    summary: json!({ "total_procedures": r.procedures.len(), "syntax": r.syntax }),
                },
                extras: Extras {
                    asm_syntax: Some(json!(r.syntax)),
                    macros: Some(vacio()),
                    ..Default::default()
                },
            }
        }
        "ipynb" => {
            let r = notebook::analyze_notebook(content);
            ArchivoAnalizado {
                nucleo: Nucleo {
                    filename,
                    language: "notebook".into(),
                    functions: json!(r.analysis.functions),
                    classes: json!(r.analysis.classes),
                    imports: json!(r.analysis.imports),
                    exports: vacio(),
                    dead_code: json!(r.analysis.dead_code),
                    call_graph: json!(r.analysis.call_graph),
                    wasm_hints: json!(r.analysis.wasm_hints),
                    security_findings: json!(r.analysis.security_findings),
                    structural_smells: json!(r.analysis.structural_smells),
                    naming_smells: json!(r.analysis.naming_smells),
                    control_findings: json!(r.analysis.control_findings),
                    summary: json!({
                        "total_functions": r.analysis.summary.total_functions,
                        "total_classes": r.analysis.summary.total_classes,
                        "total_imports": r.analysis.summary.total_imports,
                        "avg_complexity": r.analysis.summary.avg_complexity,
                    }),
                },
                extras: Extras {
                    modernization: Some(json!(r.analysis.modernization)),
                    code_cells: Some(json!(r.code_cells)),
                    cell_ranges: Some(json!(r.cell_ranges)),
                    execution_counts: Some(json!(r.execution_counts)),
                    reproducibility: Some(json!(r.reproducibility)),
                    ..Default::default()
                },
            }
        }
        "css" => {
            let r = cssparse::parse_css(content);
            ArchivoAnalizado {
                nucleo: Nucleo {
                    filename,
                    language: "css".into(),
                    functions: vacio(),
                    classes: vacio(),
                    imports: vacio(),
                    exports: vacio(),
                    dead_code: vacio(),
                    call_graph: vacio(),
                    wasm_hints: vacio(),
                    security_findings: vacio(),
                    structural_smells: vacio(),
                    naming_smells: vacio(),
                    control_findings: vacio(),
                    summary: json!(r.summary),
                },
                extras: Extras {
                    css_rules: Some(json!(r.rules)),
                    css_findings: Some(json!(r.findings)),
                    css_note: Some(json!(r.note)),
                    ..Default::default()
                },
            }
        }
        "html" | "htm" => {
            let r = htmlparse::analyze_html(content);
            ArchivoAnalizado {
                nucleo: Nucleo {
                    filename,
                    language: "html".into(),
                    functions: vacio(),
                    classes: vacio(),
                    imports: vacio(),
                    exports: vacio(),
                    dead_code: vacio(),
                    call_graph: vacio(),
                    wasm_hints: vacio(),
                    security_findings: vacio(),
                    structural_smells: vacio(),
                    naming_smells: vacio(),
                    control_findings: vacio(),
                    summary: json!(r.summary),
                },
                extras: Extras {
                    html_findings: Some(json!(r.findings)),
                    html_note: Some(json!(r.note)),
                    ..Default::default()
                },
            }
        }
        _ => unsupported(&filename, &format!(".{ext}"), "extensión no soportada"),
    }
}

/// Manifests de dependencias: se reclaman por NOMBRE, no por extensión
/// (`package.json` es un `.json` como cualquier otro) — misma regla que el
/// despacho de `static_parser.py`, que chequea nombre antes que extensión.
/// Por eso este chequeo va ANTES del `match ext` de arriba.
fn manifest_result(filename: &str, content: &str) -> Option<ArchivoAnalizado> {
    let base = filename.rsplit(['/', '\\']).next().unwrap_or(filename).to_lowercase();
    if base != "package.json" && !(base.starts_with("requirements") && base.ends_with(".txt")) {
        return None;
    }
    let r = manifest::analyze_manifest(filename, content);
    Some(ArchivoAnalizado {
        nucleo: Nucleo {
            filename: filename.to_string(),
            language: "manifest".into(),
            functions: vacio(),
            classes: vacio(),
            imports: vacio(),
            exports: vacio(),
            dead_code: vacio(),
            call_graph: vacio(),
            wasm_hints: vacio(),
            security_findings: vacio(),
            structural_smells: vacio(),
            naming_smells: vacio(),
            control_findings: vacio(),
            summary: json!(r.summary),
        },
        extras: Extras {
            manifest_dependencies: Some(json!(r.dependencies)),
            manifest_findings: Some(json!(r.findings)),
            manifest_note: Some(json!(r.note)),
            ..Default::default()
        },
    })
}

fn unsupported(filename: &str, ext: &str, reason: &str) -> ArchivoAnalizado {
    ArchivoAnalizado {
        nucleo: Nucleo {
            filename: filename.to_string(),
            language: ext.to_string(),
            functions: vacio(),
            classes: vacio(),
            imports: vacio(),
            exports: vacio(),
            dead_code: vacio(),
            call_graph: vacio(),
            wasm_hints: vacio(),
            security_findings: vacio(),
            structural_smells: vacio(),
            naming_smells: vacio(),
            control_findings: vacio(),
            summary: json!({}),
        },
        extras: Extras { error: Some(reason.to_string()), ..Default::default() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sythrall_scanner_test_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn escanea_python_js_y_c_en_un_solo_pase() {
        let dir = tmp_dir("mixto");
        write(&dir, "a.py", "import os\n\ndef f():\n    return os.getcwd()\n");
        write(&dir, "src/b.js", "function g() { return 1; }\n");
        write(&dir, "c.c", "int main() { return 0; }\n");

        let req = ScanRequest {
            project_dir: dir.to_string_lossy().into_owned(),
            extensions: vec![".py".into(), ".js".into(), ".c".into()],
            ignored_dirs: vec!["node_modules".into()],
            filenames: vec![],
        };
        let files = scan_and_parse_project(&req);
        let names: Vec<&str> = files.iter().map(|f| f["filename"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["a.py", "c.c", "src/b.js"]);
        assert_eq!(files[0]["language"], "python");
        assert_eq!(files[2]["language"], "javascript");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ignora_directorios_ignorados_y_extensiones_no_pedidas() {
        let dir = tmp_dir("ignorados");
        write(&dir, "keep.py", "x = 1\n");
        write(&dir, "node_modules/dep.js", "module.exports = 1;\n");
        write(&dir, "skip.txt", "no es código\n");

        let req = ScanRequest {
            project_dir: dir.to_string_lossy().into_owned(),
            extensions: vec![".py".into(), ".js".into()],
            ignored_dirs: vec!["node_modules".into()],
            filenames: vec![],
        };
        let files = scan_and_parse_project(&req);
        let names: Vec<&str> = files.iter().map(|f| f["filename"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["keep.py"]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn respeta_patron_glob_de_sufijo_en_ignored_dirs() {
        let dir = tmp_dir("egginfo");
        write(&dir, "keep.py", "x = 1\n");
        write(&dir, "mypkg.egg-info/PKG-INFO", "no debería importar, no es .py\n");
        write(&dir, "mypkg.egg-info/ignored.py", "y = 2\n");

        let req = ScanRequest {
            project_dir: dir.to_string_lossy().into_owned(),
            extensions: vec![".py".into()],
            ignored_dirs: vec!["*.egg-info".into()],
            filenames: vec![],
        };
        let files = scan_and_parse_project(&req);
        let names: Vec<&str> = files.iter().map(|f| f["filename"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["keep.py"]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn python_incluye_dead_code_calculado_en_rust() {
        let dir = tmp_dir("deadcode");
        write(&dir, "unused.py", "import os\nimport sys\n\ndef f():\n    return sys.argv\n");

        let req = ScanRequest {
            project_dir: dir.to_string_lossy().into_owned(),
            extensions: vec![".py".into()],
            ignored_dirs: vec![],
            filenames: vec![],
        };
        let files = scan_and_parse_project(&req);
        let dead = files[0]["dead_code"].as_array().unwrap();
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0]["name"], "os");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn loc_es_conteo_fisico_de_lineas_no_el_contenido_completo() {
        // `static_analysis.py::parse_project` necesita un conteo de líneas por
        // archivo (`language_distribution`) sin que Rust tenga que mandar de
        // vuelta el contenido completo (eso anularía el ahorro de un solo
        // round-trip) — mismo criterio que Python's `content.count("\n")`.
        let dir = tmp_dir("loc");
        write(&dir, "tres_lineas.py", "a = 1\nb = 2\nc = 3\n");

        let req = ScanRequest {
            project_dir: dir.to_string_lossy().into_owned(),
            extensions: vec![".py".into()],
            ignored_dirs: vec![],
            filenames: vec![],
        };
        let files = scan_and_parse_project(&req);
        assert_eq!(files[0]["loc"], 3);
        assert!(files[0].get("content").is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn directorio_inexistente_da_lista_vacia() {
        let req = ScanRequest {
            project_dir: "F:/esto/no/existe/sythrall_test".into(),
            extensions: vec![".py".into()],
            ignored_dirs: vec![],
            filenames: vec![],
        };
        assert!(scan_and_parse_project(&req).is_empty());
    }


    /// **El tercer chequeo de la familia: los manifests contra la realidad.**
    /// Los dos de arriba miran que el scanner cubra los lenguajes y que no
    /// tire claves. Este mira que `plugin.rs` DECLARE lo que el motor produce
    /// de verdad, en las dos direcciones — y estaban desactualizados igual
    /// que el scanner: `python` no declaraba `security_findings` con taint
    /// tracking desde la Fase 21, `c` no declaraba `modernization` con 12
    /// patrones, y `notebook` sí declaraba `security_findings`, o sea que los
    /// manifests se contradecían entre ellos.
    ///
    /// Importa porque `history.py` guarda estas features con cada snapshot
    /// para avisar cuándo dos análisis dejaron de ser comparables: un
    /// manifest que miente hace que el aviso no salga justo cuando hace falta.
    #[test]
    fn los_manifests_declaran_los_hallazgos_que_el_motor_produce() {
        let dir = tmp_dir("manifiesto_real");
        write(&dir, "a.py", "import imp\nimport os\nimport numpy as np\nfrom flask import request\nfrom scipy.linalg import solve_continuous_are\n\ndef handler(a, b, c, d, e, g):\n    cmd = request.args.get('c')\n    os.system(cmd)\n    A = [[0, 1], [0, 0]]\n    B = [[0], [1]]\n    Q = np.eye(2)\n    R = [[0]]\n    solve_continuous_are(A, B, Q, R)\n    x = 1\n    return x\n");
        write(&dir, "b.c", "#include <stdlib.h>\nint run(char **argv, int a, int b, int c, int d, int e) {\n    int *p = malloc(100);\n    system(argv[1]);\n    return a + b + c + d + e + (p == 0);\n}\n");
        // Generados en vez de escritos a mano: 60 lineas literales en un
        // test no aportan nada y esconden la intencion.
        let fortran_largo =
            format!("SUBROUTINE LARGA(X)\n  REAL X\n{}END SUBROUTINE LARGA\n", "  X = X + 1\n".repeat(60));
        let asm_largo = format!(".text\nlarga:\n{}  ret\n", "  mov %eax, %ebx\n".repeat(60));
        write(&dir, "d.f90", &fortran_largo);
        write(&dir, "e.s", &asm_largo);
        write(&dir, "c.js", "function render(req, res, a, b, c, d, e, g) {\n  document.body.innerHTML = req.query.name\n  eval(req.query.code)\n}\n");

        let req = ScanRequest {
            project_dir: dir.to_string_lossy().into_owned(),
            extensions: vec![".py".into(), ".c".into(), ".js".into(), ".f90".into(), ".s".into()],
            ignored_dirs: vec![],
            filenames: vec![],
        };
        let files = scan_and_parse_project(&req);
        let manifests = crate::servicio::plugin::builtin_manifests();

        for f in &files {
            let lang = f["language"].as_str().unwrap();
            // `javascript` es el `id` del plugin; el resto coincide.
            let manifest = manifests.iter().find(|m| m.id == lang).unwrap_or_else(|| panic!("sin manifest para {lang}"));
            let declaradas: Vec<&str> =
                manifest.features.iter().copied().filter(|x| crate::servicio::plugin::is_finding_feature(x)).collect();

            // Direccion 1 — nada que el motor produzca queda sin declarar.
            // Es la que atrapa el drift real: se agrega un analisis nuevo y
            // el manifest se queda viejo, en silencio.
            for key in ["security_findings", "structural_smells", "naming_smells", "control_findings"] {
                let produce = f[key].as_array().map(|a| !a.is_empty()).unwrap_or(false);
                if produce {
                    assert!(declaradas.contains(&key), "{lang} produce `{key}` y su manifest no lo declara");
                }
            }
            let produce_mod =
                f["modernization"]["candidates"].as_array().map(|a| !a.is_empty()).unwrap_or(false);
            if produce_mod {
                assert!(
                    declaradas.contains(&"modernization"),
                    "{lang} produce candidatos de modernizacion y su manifest no lo declara"
                );
            }

            // Direccion 2 — nada declarado es imposible de producir. El
            // fixture de cada lenguaje esta escrito para disparar todo lo que
            // ese lenguaje declara, asi que una feature declarada que no sale
            // es una promesa que el motor no cumple.
            for d in &declaradas {
                let sale = if *d == "modernization" {
                    produce_mod
                } else {
                    f[*d].as_array().map(|a| !a.is_empty()).unwrap_or(false)
                };
                assert!(sale, "{lang} declara `{d}` en su manifest pero el motor no lo produjo");
            }
        }
    }

    /// Hermano del de Python: mismo modo de falla, otra rama. `jsts.rs`
    /// calcula `security_findings` y `structural_smells` desde las Fases 21 y
    /// 22 — sus doc-comments dicen literalmente que existen para arreglar el
    /// "Security/Quality 100/100 por ausencia de analisis" de un proyecto
    /// JS/TS. El arreglo habia entrado en `static_parser.py` (la ruta de
    /// archivos sueltos) pero NO en el scanner, que es la ruta por la que un
    /// analisis de PROYECTO pasa — o sea, la unica donde ese score se
    /// calcula. El bug seguia entero justo donde importaba.
    #[test]
    fn el_escaneo_de_un_js_trae_los_hallazgos_de_seguridad_y_los_smells() {
        let dir = tmp_dir("claves_js");
        write(
            &dir,
            "app.js",
            "const url = req.query.next\ndocument.body.innerHTML = req.query.name\neval(req.query.code)\n",
        );
        let req = ScanRequest {
            project_dir: dir.to_string_lossy().into_owned(),
            extensions: vec![".js".into()],
            ignored_dirs: vec![],
            filenames: vec![],
        };
        let files = scan_and_parse_project(&req);
        let f = &files[0];
        assert_eq!(f["language"], "javascript");
        let sec = f["security_findings"].as_array().expect("falta `security_findings`");
        assert!(!sec.is_empty(), "JS/TS volvio con 0 hallazgos de seguridad: {f:?}");
        assert!(f["structural_smells"].as_array().is_some(), "falta `structural_smells`");
        assert_eq!(f["summary"]["security_findings"], sec.len());
    }

    /// Y el tercero: `cparse` calcula el informe de modernizacion en la misma
    /// llamada (`modernization.rs`, 12 patrones), pero el scanner lo tiraba.
    /// `modernization_by_file` se escribio EXACTAMENTE para C/C++ a nivel de
    /// proyecto, y nunca habia recibido un solo candidato por esta ruta.
    #[test]
    fn el_escaneo_de_un_c_trae_modernizacion_seguridad_y_smells() {
        let dir = tmp_dir("claves_c");
        write(
            &dir,
            "leak.c",
            "#include <stdlib.h>\nint main(int argc, char **argv) {\n    int *p = malloc(100);\n    system(argv[1]);\n    return 0;\n}\n",
        );
        let req = ScanRequest {
            project_dir: dir.to_string_lossy().into_owned(),
            extensions: vec![".c".into()],
            ignored_dirs: vec![],
            filenames: vec![],
        };
        let files = scan_and_parse_project(&req);
        let f = &files[0];
        assert_eq!(f["language"], "c");
        let cand = f["modernization"]["candidates"].as_array().expect("falta `modernization.candidates`");
        // `malloc` sin `free` en el archivo: `unmatched_allocation`.
        assert!(
            cand.iter().any(|c| c["pattern"] == "unmatched_allocation"),
            "C volvio sin candidatos de modernizacion: {cand:?}"
        );
        // Fase 21 — `system(argv[1])` es CWE-78, y `security_findings` era un
        // `[]` fijo para C/C++ hasta 2026-09-05.
        let sec = f["security_findings"].as_array().expect("falta `security_findings`");
        assert!(sec.iter().any(|x| x["cwe"] == "CWE-78"), "C volvio sin hallazgos de seguridad: {sec:?}");
        // Fase 22 — smells estructurales de C/C++.
        assert!(f["structural_smells"].as_array().is_some(), "falta `structural_smells`");
    }

    /// Mismo criterio de "CLAVES, no lenguajes" que los de abajo, para la
    /// clave que agrega la Fase 34. `template_complexity` es un OBJETO, no un
    /// array, así que el modo de fallar es todavía más silencioso: un
    /// `.get(...)` con default devolvería un dict vacío y el panel mostraría
    /// ceros, indistinguible de "este archivo no usa templates".
    #[test]
    fn el_escaneo_de_un_cpp_trae_las_metricas_de_templates() {
        let dir = tmp_dir("claves_cpp");
        write(
            &dir,
            "meta.cpp",
            "template<int N> struct Fact { static constexpr int v = N * Fact<N - 1>::v; };
template<> struct Fact<0> { static constexpr int v = 1; };
int main() { return Fact<3>::v; }
",
        );
        let req = ScanRequest {
            project_dir: dir.to_string_lossy().into_owned(),
            extensions: vec![".cpp".into()],
            ignored_dirs: vec![],
            filenames: vec![],
        };
        let files = scan_and_parse_project(&req);
        let f = &files[0];
        assert_eq!(f["language"], "cpp");
        let tc = &f["template_complexity"];
        assert!(!tc.is_null(), "falta `template_complexity` en el escaneo de proyecto");
        assert_eq!(tc["templates"], 1, "{tc:?}");
        assert_eq!(tc["specializations"], 1, "{tc:?}");
        assert_eq!(
            tc["recursive_templates"].as_array().map(|a| a.len()),
            Some(1),
            "la recursion de Fact tiene que llegar al proyecto: {tc:?}"
        );
    }

    /// El test de cobertura de arriba mira los LENGUAJES; este mira las
    /// CLAVES. La rama de Python del scanner arma su propio JSON a mano, y
    /// omitir una clave no rompe nada visible: la agregación de
    /// `static_analysis.py` usa `.get(...)` con default, así que el hallazgo
    /// simplemente nunca aparece y el resultado se ve sano. Fue exactamente
    /// lo que pasó con `control_findings` (Fase 15): andaba archivo por
    /// archivo desde el día uno y en un escaneo de proyecto nunca llegó, sin
    /// que nada lo reportara. Este test falla si alguien agrega un análisis
    /// nuevo a `rich.rs` y se olvida de enseñárselo al scanner.
    #[test]
    fn el_escaneo_de_un_python_trae_todas_las_claves_que_el_proyecto_agrega() {
        let dir = tmp_dir("claves_py");
        write(
            &dir,
            "legacy.py",
            "import imp\nimport numpy as np\nfrom scipy.linalg import solve_continuous_are\nA = [[0, 1], [0, 0]]\nB = [[0], [1]]\nQ = np.eye(2)\nR = [[0]]\nsolve_continuous_are(A, B, Q, R)\n",
        );
        let req = ScanRequest {
            project_dir: dir.to_string_lossy().into_owned(),
            extensions: vec![".py".into()],
            ignored_dirs: vec![],
            filenames: vec![],
        };
        let files = scan_and_parse_project(&req);
        let f = &files[0];
        assert_eq!(f["language"], "python");

        // Fase 15 — precondiciones de control optimo. `R = [[0]]` no puede
        // ser definida positiva, asi que tiene que haber al menos un hallazgo.
        let control = f["control_findings"].as_array().expect("falta `control_findings`");
        assert!(!control.is_empty(), "control_findings vacio: {f:?}");

        // Fase 25 — APIs deprecadas de Python. `import imp` se fue en 3.12.
        let moderniz = f["modernization"]["candidates"].as_array().expect("falta `modernization.candidates`");
        assert!(
            moderniz.iter().any(|c| c["variable"] == "imp"),
            "modernization no trajo el `import imp`: {moderniz:?}"
        );
    }

    /// **El test que el tipo `Nucleo` hace posible afirmar con fuerza.**
    ///
    /// Antes, cada brazo del despacho armaba su JSON a mano y las claves del
    /// nucleo aparecian en unos si y en otros no: `control_findings` estaba
    /// en 3 de 9 brazos, y ese hueco fue un bug real que anduvo archivo por
    /// archivo y nunca llego al escaneo de proyecto. Los tests de "CLAVES, no
    /// lenguajes" atrapaban el olvido de a una clave y de a un lenguaje;
    /// este lo afirma para TODOS contra TODOS, y solo se puede escribir asi
    /// porque el compilador ya garantiza que ningun brazo se saltee un campo.
    #[test]
    fn todo_lenguaje_devuelve_el_nucleo_completo() {
        let dir = tmp_dir("nucleo");
        write(&dir, "a.py", "def f():\n    return 1\n");
        write(&dir, "b.c", "int main() { return 0; }\n");
        write(&dir, "b2.cpp", "template<typename T> struct Caja { T v; };\n");
        write(&dir, "c.ts", "export function g(): number { return 1 }\n");
        write(&dir, "c2.js", "function h() { return 1 }\n");
        write(&dir, "d.f90", "SUBROUTINE S(X)\n  REAL X\nEND SUBROUTINE S\n");
        write(&dir, "e.s", ".text\nmain:\n  ret\n");
        write(&dir, "e2.rs", "pub fn f(x: i32) -> i32 { x + 1 }\n");
        write(&dir, "f.ipynb", "{\"cells\": [{\"cell_type\": \"code\", \"source\": [\"x = 1\\n\"]}]}");
        write(&dir, "g.css", ".btn { color: red; }\n");
        write(&dir, "h.html", "<html><img src=x></html>\n");
        write(&dir, "package.json", "{\"dependencies\": {\"lodash\": \"*\"}}");
        let req = ScanRequest {
            project_dir: dir.to_string_lossy().into_owned(),
            extensions: vec![
                ".py".into(), ".c".into(), ".cpp".into(), ".ts".into(), ".js".into(),
                ".f90".into(), ".s".into(), ".rs".into(), ".ipynb".into(), ".css".into(),
                ".html".into(),
            ],
            ignored_dirs: vec![],
            filenames: vec!["package.json".into()],
        };
        let files = scan_and_parse_project(&req);
        assert!(files.len() >= 12, "esperaba un archivo por lenguaje: {}", files.len());

        const NUCLEO: &[&str] = &[
            "filename",
            "language",
            "loc",
            "functions",
            "classes",
            "imports",
            "exports",
            "dead_code",
            "call_graph",
            "wasm_hints",
            "security_findings",
            "structural_smells",
            "naming_smells",
            "control_findings",
            "summary",
        ];
        for f in &files {
            let lang = f["language"].as_str().unwrap_or("?");
            for clave in NUCLEO {
                assert!(
                    f.get(*clave).is_some(),
                    "`{lang}` volvio sin la clave de nucleo `{clave}`: {f:?}"
                );
            }
            // Y las 10 listas del nucleo son listas de verdad, no `null`: un
            // `null` se agrega distinto que un `[]` del otro lado.
            for clave in [
                "functions",
                "classes",
                "imports",
                "exports",
                "dead_code",
                "call_graph",
                "wasm_hints",
                "security_findings",
                "structural_smells",
                "naming_smells",
                "control_findings",
            ] {
                assert!(
                    f[clave].is_array(),
                    "`{lang}`.`{clave}` tendria que ser una lista y es {:?}",
                    f[clave]
                );
            }
        }
    }

    /// **El test que faltaba, y que explica por qué existe este bullet.** El
    /// escaneo de proyecto conocía solo Python, C/C++ y JS/TS: Fortran,
    /// Assembly, Notebook, CSS, HTML y los manifests estaban implementados y
    /// andaban archivo por archivo, pero en un escaneo COMPLETO caían todos
    /// en `unsupported` — o sea que sus hallazgos nunca llegaban al análisis
    /// de proyecto. Este test falla si alguien agrega un plugin nuevo y se
    /// olvida de enseñárselo al scanner, que es exactamente lo que había
    /// pasado 6 veces seguidas.
    #[test]
    fn el_escaneo_de_proyecto_cubre_los_11_lenguajes_y_los_manifests() {
        let dir = tmp_dir("cobertura");
        write(&dir, "a.py", "def f():\n    return 1\n");
        write(&dir, "b.c", "int main() { return 0; }\n");
        write(&dir, "c.ts", "export function g(): number { return 1 }\n");
        write(&dir, "d.f90", "SUBROUTINE S(X)\n  REAL X\nEND SUBROUTINE S\n");
        write(&dir, "e.s", ".text\nmain:\n  ret\n");
        write(&dir, "e2.rs", "use std::fmt;\npub fn f(x: i32) -> i32 { x + 1 }\n");
        write(&dir, "f.ipynb", "{\"cells\": [{\"cell_type\": \"code\", \"source\": [\"x = 1\\n\"]}]}");
        write(&dir, "g.css", ".btn { color: red; }\n");
        write(&dir, "h.html", "<html><img src=x></html>\n");
        write(&dir, "package.json", "{\"dependencies\": {\"lodash\": \"*\"}}");

        let req = ScanRequest {
            project_dir: dir.to_string_lossy().into_owned(),
            extensions: vec![
                ".py".into(), ".c".into(), ".ts".into(), ".f90".into(), ".s".into(),
                ".rs".into(), ".ipynb".into(), ".css".into(), ".html".into(),
            ],
            ignored_dirs: vec![],
            // `package.json` entra por NOMBRE: sin esto habria que incluir
            // `.json` entero y arrastrar todo JSON del proyecto.
            filenames: vec!["package.json".into()],
        };
        let files = scan_and_parse_project(&req);
        let langs: Vec<&str> = files.iter().map(|f| f["language"].as_str().unwrap_or("?")).collect();

        for esperado in [
            "python", "c", "typescript", "fortran", "assembly", "rust", "notebook", "css", "html",
            "manifest",
        ] {
            assert!(langs.contains(&esperado), "falta `{esperado}` en el escaneo: {langs:?}");
        }
        // Y ninguno degradó a `unsupported`, que es como se veía antes.
        assert!(files.iter().all(|f| f.get("error").is_none()), "algún archivo degradó: {files:?}");

        // Los hallazgos propios de cada uno llegan de verdad, no solo el
        // nombre del lenguaje.
        let css = files.iter().find(|f| f["language"] == "css").unwrap();
        assert!(css["css_rules"].as_array().is_some_and(|r| !r.is_empty()));
        let html = files.iter().find(|f| f["language"] == "html").unwrap();
        assert!(html["html_findings"].as_array().is_some_and(|f| !f.is_empty()), "el <img> sin alt");
        let man = files.iter().find(|f| f["language"] == "manifest").unwrap();
        assert!(man["manifest_findings"].as_array().is_some_and(|f| !f.is_empty()), "lodash con `*`");
        // Rust: la superficie de confianza viaja aunque sea toda cero. Un
        // archivo sin `unsafe` tiene superficie VACIA, que es un dato, y no
        // es lo mismo que no haberla medido.
        let rs = files.iter().find(|f| f["language"] == "rust").unwrap();
        assert_eq!(rs["trust_surface"]["unsafe_blocks"], 0);
        assert_eq!(rs["imports"].as_array().map(|i| i.len()), Some(1), "el `use std::fmt`");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn un_json_cualquiera_no_se_toma_por_un_manifest() {
        // El despacho por nombre tiene prioridad sobre la extensión, pero
        // solo para los nombres que el plugin reclama: `tsconfig.json` no es
        // un manifest de dependencias.
        let dir = tmp_dir("json_ajeno");
        write(&dir, "tsconfig.json", "{\"compilerOptions\": {}}");
        let req = ScanRequest {
            project_dir: dir.to_string_lossy().into_owned(),
            extensions: vec![".json".into()],
            ignored_dirs: vec![],
            filenames: vec!["package.json".into()],
        };
        let files = scan_and_parse_project(&req);
        assert_eq!(files.len(), 1);
        assert_ne!(files[0]["language"], "manifest");

        std::fs::remove_dir_all(&dir).ok();
    }
}
