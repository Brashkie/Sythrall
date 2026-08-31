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

use serde::Deserialize;
use serde_json::{json, Value};

use crate::{cparse, jsts, rich};

#[derive(Deserialize)]
pub struct ScanRequest {
    pub project_dir: String,
    pub extensions: Vec<String>,
    pub ignored_dirs: Vec<String>,
}

/// Camina el árbol y parsea cada archivo encontrado — mismo criterio de
/// filtrado que `read_project_files` en `project_service.py`: extensión
/// permitida, directorio no ignorado (exacto o `*sufijo`, para casos como
/// `*.egg-info`), contenido no vacío. Orden determinístico (alfabético por
/// ruta relativa) para que la salida no dependa del orden de `read_dir`.
pub fn scan_and_parse_project(req: &ScanRequest) -> Vec<Value> {
    let root = Path::new(&req.project_dir);
    let mut files = Vec::new();
    walk_dir(root, root, &req.extensions, &req.ignored_dirs, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files.into_iter().map(|(filename, content)| parse_one(filename, &content)).collect()
}

fn walk_dir(root: &Path, dir: &Path, extensions: &[String], ignored_dirs: &[String], out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else { continue };
        let path = entry.path();
        if file_type.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if is_ignored(name, ignored_dirs) {
                continue;
            }
            walk_dir(root, &path, extensions, ignored_dirs, out);
        } else if file_type.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
            if !extensions.iter().any(|e| e.eq_ignore_ascii_case(&ext)) {
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

fn parse_one(filename: String, content: &str) -> Value {
    let ext = Path::new(&filename).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    // Conteo físico de líneas (mismo criterio que Python's `content.count("\n")`,
    // "wc -l") — se manda de vuelta en vez de `content` para que
    // `language_distribution` en `static_analysis.py::parse_project` no
    // necesite el archivo entero de nuevo solo para esta cuenta.
    let loc = content.matches('\n').count();
    let mut result = parse_one_inner(filename, ext.as_str(), content);
    result["loc"] = json!(loc);
    result
}

fn parse_one_inner(filename: String, ext: &str, content: &str) -> Value {
    match ext {
        "py" => {
            let r = rich::analyze_rich(content);
            json!({
                "filename": filename,
                "language": "python",
                "functions": r.functions,
                "classes": r.classes,
                "imports": r.imports,
                "exports": Vec::<Value>::new(),
                "dead_code": r.dead_code,
                "call_graph": r.call_graph,
                "wasm_hints": r.wasm_hints,
                "security_findings": r.security_findings,
                "structural_smells": r.structural_smells,
                "naming_smells": r.naming_smells,
                "summary": {
                    "total_functions": r.summary.total_functions,
                    "total_classes": r.summary.total_classes,
                    "total_imports": r.summary.total_imports,
                    "unused_imports": r.dead_code.len(),
                    "avg_complexity": r.summary.avg_complexity,
                    "max_loc_function": r.summary.max_loc_function,
                    "security_findings": r.security_findings.len(),
                    "structural_smells": r.structural_smells.len(),
                    "naming_smells": r.naming_smells.len(),
                },
            })
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
                    json!({
                        "filename": filename,
                        "language": lang,
                        "functions": result.functions,
                        "classes": result.classes,
                        "imports": result.imports,
                        "exports": Vec::<Value>::new(),
                        "dead_code": Vec::<Value>::new(),
                        "macros": result.macros,
                        "call_graph": result.call_graph,
                        "wasm_hints": result.wasm_hints,
                        "security_findings": Vec::<Value>::new(),
                        "structural_smells": Vec::<Value>::new(),
                        "naming_smells": Vec::<Value>::new(),
                        "summary": summary,
                    })
                }
                None => unsupported(&filename, &format!(".{lang}"), "parseo tree-sitter falló"),
            }
        }
        "js" | "jsx" | "ts" | "tsx" => {
            let is_ts = ext == "ts" || ext == "tsx";
            let lang = if is_ts { "typescript" } else { "javascript" };
            let result = jsts::parse_js_ts(content, is_ts);
            json!({
                "filename": filename,
                "language": lang,
                "functions": result.functions,
                "classes": result.classes,
                "imports": result.imports,
                "exports": result.exports,
                "interfaces": result.interfaces,
                "types": result.types,
                "dead_code": result.dead_code,
                "call_graph": result.call_graph,
                "wasm_hints": result.wasm_hints,
                "security_findings": Vec::<Value>::new(),
                "structural_smells": Vec::<Value>::new(),
                "naming_smells": Vec::<Value>::new(),
                "summary": {
                    "total_functions": result.functions.len(),
                    "total_classes": result.classes.len(),
                    "total_imports": result.imports.len(),
                    "total_exports": result.exports.len(),
                    "total_interfaces": result.interfaces.len(),
                    "unused_imports": result.dead_code.len(),
                    "avg_complexity": result.avg_complexity,
                },
            })
        }
        _ => unsupported(&filename, &format!(".{ext}"), "extensión no soportada"),
    }
}

fn unsupported(filename: &str, ext: &str, reason: &str) -> Value {
    json!({
        "filename": filename,
        "language": ext,
        "error": reason,
        "functions": [], "classes": [], "imports": [], "exports": [],
        "dead_code": [], "call_graph": [], "wasm_hints": [],
        "security_findings": [], "structural_smells": [], "naming_smells": [],
        "summary": {},
    })
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
        };
        assert!(scan_and_parse_project(&req).is_empty());
    }
}
