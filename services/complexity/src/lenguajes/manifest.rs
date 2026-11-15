//! Dependency Intelligence — primera **Format Extension** de la plataforma.
//!
//! La Fase 24 nombró 5 categorías de plugin y hasta acá solo una tenía
//! contenido: los 9 Language plugins. "Format Extensions — entender un
//! formato de archivo que los parsers propios no cubren" estaba definida y
//! vacía. Este módulo la estrena, y de paso ataca algo que el usuario
//! planteó como principio: *no alcanza con evitar dependencias de terceros,
//! hay que analizarlas.*
//!
//! **Despacho por NOMBRE de archivo, no por extensión.** Es la diferencia
//! estructural con los Language plugins: `package.json` es un `.json` como
//! cualquier otro, y `requirements.txt` un `.txt` — despachar por extensión
//! mandaría cualquier JSON del proyecto a este analizador. `plugin.rs` gana
//! por eso un campo `filenames`, y el despacho chequea nombre exacto antes
//! que extensión.
//!
//! **Todo lo que reporta se prueba leyendo el manifest, igual que
//! `cssparse.rs`.** Un rango sin cota superior admite cualquier versión
//! futura — eso es lo que dice el archivo, no una estimación. Una
//! dependencia declarada dos veces con specs distintos es una contradicción
//! visible. Deliberadamente NO hay consulta de CVEs ni de la última versión
//! publicada: eso exige red, y la red es un `Permission` que la Fase 24
//! declaró y que ningún plugin ejerce todavía. Cuando exista ese camino,
//! este módulo es su primer consumidor natural — pero mientras tanto no se
//! finge tener datos que no se tienen.
//!
//! Alcance de esta primera porción: `package.json` (JSON, vía `serde_json`
//! que ya era dependencia) y `requirements.txt` (texto plano). `Cargo.toml`
//! y `pyproject.toml` quedan afuera **por una razón concreta**: necesitarían
//! un parser TOML, y agregar un crate nuevo sin un benchmark que lo
//! justifique contradice el criterio del propio proyecto — igual que
//! escribir un parser TOML a mano para leer 3 secciones. Se decide cuando
//! haya un motivo, no antes.

use std::collections::HashMap;

use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct Dependency {
    pub name: String,
    /// El spec tal cual está escrito (`^4.17.21`, `>=2.0`, `*`, `git+https://…`).
    pub spec: String,
    /// `"dependencies"` / `"devDependencies"` / `"requirements"` — de dónde
    /// salió, para poder distinguir producción de desarrollo.
    pub section: String,
    pub line: usize,
    /// Clasificación del spec: qué tan acotada está la versión.
    pub pinning: &'static str,
}

#[derive(Serialize, Clone)]
pub struct ManifestFinding {
    pub kind: &'static str,
    pub dependency: String,
    pub line: usize,
    pub severity: &'static str,
    pub confidence: &'static str,
    pub message: String,
}

#[derive(Serialize)]
pub struct ManifestSummary {
    pub ecosystem: &'static str,
    pub total: usize,
    pub production: usize,
    pub development: usize,
    /// Cuántas están fijadas a una versión exacta.
    pub exact: usize,
    /// Cuántas admiten un rango acotado (`^`, `~`, `>=x,<y`).
    pub ranged: usize,
    /// Cuántas admiten CUALQUIER versión futura.
    pub unbounded: usize,
}

#[derive(Serialize)]
pub struct ManifestAnalysisResult {
    pub dependencies: Vec<Dependency>,
    pub findings: Vec<ManifestFinding>,
    pub summary: ManifestSummary,
    pub note: String,
    pub error: Option<String>,
}

const NOTE: &str = "Análisis del manifest solamente: todo lo que se reporta se puede probar leyendo este archivo. NO incluye consulta de CVEs ni de la última versión publicada — eso exige acceso a red, que ningún plugin ejerce todavía. Tampoco afirma que una dependencia no se use: eso necesita cruzar con los imports del proyecto.";

/// Clasifica un spec de versión por qué tan acotado está. Es la señal de la
/// que cuelgan los hallazgos, así que conviene que sea explícita.
fn pinning_of(spec: &str) -> &'static str {
    let s = spec.trim();
    if s.is_empty() {
        return "unbounded"; // `requests` sin nada = cualquier versión
    }
    if s.starts_with("git+") || s.starts_with("http://") || s.starts_with("https://") || s.contains(".git") || s.starts_with("file:") {
        return "external_source";
    }
    if s == "*" || s == "x" || s == "latest" {
        return "unbounded";
    }
    // Un `>=`/`>` sin ninguna cota superior en el mismo spec admite cualquier
    // versión futura, incluida una major con cambios incompatibles.
    let has_lower_only = (s.starts_with(">=") || s.starts_with('>')) && !s.contains('<');
    if has_lower_only {
        return "unbounded";
    }
    if s.starts_with('^') || s.starts_with('~') || s.contains('<') || s.contains(" - ") || s.contains("||") {
        return "ranged";
    }
    // `1.2.3`, `==1.2.3` — una sola versión.
    "exact"
}

fn finding_for(dep: &Dependency) -> Option<ManifestFinding> {
    match dep.pinning {
        "unbounded" => Some(ManifestFinding {
            kind: "unbounded_dependency",
            dependency: dep.name.clone(),
            line: dep.line,
            severity: "Medium",
            confidence: "High",
            message: format!(
                "`{}` acepta cualquier versión futura (spec: `{}`). Dos instalaciones del mismo commit pueden traer código distinto, y una versión comprometida publicada mañana entra sin que nada cambie en este repositorio. Acotá con una cota superior (`^1.2.3`, `>=1.2,<2.0`).",
                dep.name,
                if dep.spec.trim().is_empty() { "sin versión" } else { dep.spec.trim() }
            ),
        }),
        "external_source" => Some(ManifestFinding {
            kind: "external_source_dependency",
            dependency: dep.name.clone(),
            line: dep.line,
            severity: "Medium",
            confidence: "High",
            message: format!(
                "`{}` no viene del registro sino de una fuente externa (`{}`). No hay resolución de versiones ni historial de publicación: lo que traiga esa URL el día del build es lo que se instala.",
                dep.name, dep.spec
            ),
        }),
        _ => None,
    }
}

/// Una dependencia declarada en más de una sección. Cuál gana depende del
/// instalador, así que el archivo no dice qué versión se usa — que es
/// exactamente el problema.
fn duplicate_findings(deps: &[Dependency]) -> Vec<ManifestFinding> {
    let mut seen: HashMap<&str, &Dependency> = HashMap::new();
    let mut out = Vec::new();
    for d in deps {
        if let Some(first) = seen.get(d.name.as_str()) {
            out.push(ManifestFinding {
                kind: "duplicate_dependency",
                dependency: d.name.clone(),
                line: d.line,
                severity: if first.spec == d.spec { "Low" } else { "Medium" },
                confidence: "High",
                message: if first.spec == d.spec {
                    format!(
                        "`{}` está declarada en `{}` y en `{}` con el mismo spec (`{}`) — la duplicación no cambia qué se instala, pero sí qué hay que actualizar en 2 lugares.",
                        d.name, first.section, d.section, d.spec
                    )
                } else {
                    format!(
                        "`{}` está declarada en `{}` como `{}` y en `{}` como `{}`. Cuál gana depende del instalador: el archivo no dice qué versión se termina usando.",
                        d.name, first.section, first.spec, d.section, d.spec
                    )
                },
            });
        } else {
            seen.insert(d.name.as_str(), d);
        }
    }
    out
}

/// Línea (1-based) donde aparece `"nombre"` como clave dentro del texto
/// crudo. `serde_json` no conserva posiciones, y un hallazgo sin línea es
/// mucho menos útil en el editor — buscar la clave en el texto es barato y
/// da la ubicación real.
fn line_of_key(source: &str, key: &str) -> usize {
    let needle = format!("\"{key}\"");
    source.lines().position(|l| l.contains(&needle)).map_or(1, |i| i + 1)
}

fn parse_package_json(source: &str) -> ManifestAnalysisResult {
    let value: serde_json::Value = match serde_json::from_str(source) {
        Ok(v) => v,
        Err(e) => return empty_result("npm", Some(format!("package.json inválido: {e}"))),
    };
    let mut deps = Vec::new();
    for section in ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"] {
        let Some(obj) = value.get(section).and_then(|v| v.as_object()) else { continue };
        for (name, spec) in obj {
            let spec = spec.as_str().unwrap_or("").to_string();
            deps.push(Dependency {
                pinning: pinning_of(&spec),
                line: line_of_key(source, name),
                name: name.clone(),
                spec,
                section: section.to_string(),
            });
        }
    }
    build_result("npm", deps, None)
}

/// Separa `nombre` de `spec` en un requirement de Python.
///
/// **Lo usan DOS formatos, no uno**: cada línea de `requirements.txt` y cada
/// elemento de `[project] dependencies` de un `pyproject.toml` (PEP 621), que
/// son la misma gramática escrita en archivos distintos. Estaba inline en el
/// primero; se extrajo al aparecer el segundo, para no tener dos separadores
/// que puedan divergir.
///
/// Los extras (`requests[security]`) son parte del NOMBRE del paquete, no del
/// spec: se sacan para que el nombre sea comparable entre manifests.
fn separar_requirement(linea: &str) -> Option<(String, String)> {
    let linea = linea.trim();
    let split_at = linea.find(|c| "=<>!~".contains(c));
    let (name, spec) = match split_at {
        Some(idx) => (linea[..idx].trim(), linea[idx..].trim()),
        None => (linea, ""),
    };
    // Un marcador de entorno (`; python_version < "3.11"`) no es parte del
    // nombre ni del spec de versión.
    let name = name.split(';').next().unwrap_or(name).trim();
    let name = name.split('[').next().unwrap_or(name).trim();
    if name.is_empty() {
        return None;
    }
    Some((name.to_string(), spec.split(';').next().unwrap_or(spec).trim().to_string()))
}

/// `requirements.txt`: una dependencia por línea. Se saltean comentarios,
/// líneas vacías y directivas del propio pip (`-r otro.txt`, `--index-url`),
/// que no son dependencias.
fn parse_requirements(source: &str) -> ManifestAnalysisResult {
    let mut deps = Vec::new();
    for (i, raw) in source.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with('-') {
            continue;
        }
        let Some((name, spec)) = separar_requirement(line) else { continue };
        deps.push(Dependency {
            pinning: pinning_of(&spec),
            name,
            spec,
            section: "requirements".to_string(),
            line: i + 1,
        });
    }
    build_result("pypi", deps, None)
}

fn build_result(ecosystem: &'static str, deps: Vec<Dependency>, error: Option<String>) -> ManifestAnalysisResult {
    let mut findings: Vec<ManifestFinding> = deps.iter().filter_map(finding_for).collect();
    findings.extend(duplicate_findings(&deps));
    findings.sort_by_key(|f| f.line);

    let summary = ManifestSummary {
        ecosystem,
        total: deps.len(),
        production: deps.iter().filter(|d| d.section != "devDependencies").count(),
        development: deps.iter().filter(|d| d.section == "devDependencies").count(),
        exact: deps.iter().filter(|d| d.pinning == "exact").count(),
        ranged: deps.iter().filter(|d| d.pinning == "ranged").count(),
        unbounded: deps.iter().filter(|d| d.pinning == "unbounded" || d.pinning == "external_source").count(),
    };
    ManifestAnalysisResult { dependencies: deps, findings, summary, note: NOTE.to_string(), error }
}

fn empty_result(ecosystem: &'static str, error: Option<String>) -> ManifestAnalysisResult {
    build_result(ecosystem, Vec::new(), error)
}

/// Punto de entrada — elige el parser por nombre de archivo. Infalible: un
/// nombre desconocido devuelve un reporte vacío con `error`, nunca un panic.
/// **El spec de una dependencia de Cargo puede ser una cadena o una tabla.**
///
/// `serde = "1"` y `serde = { version = "1", features = ["derive"] }` son la
/// misma dependencia escrita de dos formas, y la segunda es la que usa
/// cualquier `Cargo.toml` real en cuanto pide una feature. Leer solo la
/// primera dejaría afuera justo las dependencias más configuradas.
///
/// Y una tabla **sin `version`** no está sin acotar: está apuntando a otro
/// lado. `{ path = "../otra" }` es una ruta local, `{ git = "…" }` es un
/// repositorio y `{ workspace = true }` hereda del workspace. Las tres se
/// marcan como tales en vez de inventarles una versión vacía, que es lo que
/// las haría aparecer como "cualquier versión futura" sin serlo.
/// **Cargo invierte el default de npm y pip, así que no puede compartir
/// clasificador.**
///
/// En `package.json` y en `requirements.txt`, `"1.2.3"` y `==1.2.3` son una
/// versión y nada más. En `Cargo.toml`, `serde = "1.2.3"` significa
/// **`^1.2.3`** — Cargo le pone el caret solo— y aceptar cualquier `1.x`
/// posterior. Fijar de verdad exige escribir `=1.2.3`.
///
/// Reusar `pinning_of` acá daba un resultado falso y medible: sobre el
/// `Cargo.toml` de este mismo repo reportaba **26 de 26 dependencias
/// "exactas"** cuando ninguna lo está. Un número así, en el panel que existe
/// para decir qué tan acotado está el proyecto, es peor que no tenerlo.
fn pinning_cargo(spec: &str) -> &'static str {
    let s = spec.trim();
    if s.is_empty() {
        return "unbounded";
    }
    // Formas que no son una versión: otro origen.
    if s.starts_with("path:") || s.starts_with("git+") || s == "workspace" {
        return "external_source";
    }
    if s == "*" {
        return "unbounded";
    }
    // Lo único que fija de verdad en Cargo.
    if s.starts_with('=') {
        return "exact";
    }
    // `>=1` sin cota superior admite cualquier major futura, igual que en el
    // resto de los ecosistemas.
    if (s.starts_with(">=") || s.starts_with('>')) && !s.contains('<') {
        return "unbounded";
    }
    // Todo lo demás —`1`, `1.2`, `^1.2`, `~1.2`, `>=1, <2`— es un rango.
    "ranged"
}

fn spec_de_cargo(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => s.clone(),
        toml::Value::Table(t) => {
            if let Some(ver) = t.get("version").and_then(|x| x.as_str()) {
                ver.to_string()
            } else if let Some(p) = t.get("path").and_then(|x| x.as_str()) {
                format!("path:{p}")
            } else if let Some(g) = t.get("git").and_then(|x| x.as_str()) {
                format!("git+{g}")
            } else if t.get("workspace").and_then(|x| x.as_bool()) == Some(true) {
                "workspace".to_string()
            } else {
                String::new()
            }
        }
        _ => String::new(),
    }
}

/// `Cargo.toml`. Las cuatro secciones que Cargo reconoce, más las de
/// `[target.<cfg>.dependencies]`, que son dependencias igual — solo que
/// condicionadas a una plataforma.
fn parse_cargo_toml(source: &str) -> ManifestAnalysisResult {
    let value: toml::Value = match source.parse::<toml::Value>() {
        Ok(v) => v,
        Err(e) => return empty_result("cargo", Some(format!("Cargo.toml inválido: {e}"))),
    };
    let mut deps = Vec::new();

    let juntar = |tabla: Option<&toml::Value>, seccion: &str, deps: &mut Vec<Dependency>| {
        let Some(t) = tabla.and_then(|x| x.as_table()) else { return };
        for (name, v) in t {
            let spec = spec_de_cargo(v);
            deps.push(Dependency {
                pinning: pinning_cargo(&spec),
                line: line_of_key(source, name),
                name: name.clone(),
                spec,
                section: seccion.to_string(),
            });
        }
    };

    for seccion in ["dependencies", "dev-dependencies", "build-dependencies"] {
        juntar(value.get(seccion), seccion, &mut deps);
    }
    // `[target.'cfg(windows)'.dependencies]` — una tabla por plataforma.
    if let Some(targets) = value.get("target").and_then(|x| x.as_table()) {
        for (plataforma, t) in targets {
            for seccion in ["dependencies", "dev-dependencies", "build-dependencies"] {
                juntar(t.get(seccion), &format!("target.{plataforma}.{seccion}"), &mut deps);
            }
        }
    }
    // `[workspace.dependencies]` — el lugar donde un workspace fija versiones
    // una sola vez para todos sus miembros.
    if let Some(ws) = value.get("workspace") {
        juntar(ws.get("dependencies"), "workspace.dependencies", &mut deps);
    }

    build_result("cargo", deps, None)
}

/// `pyproject.toml`. **Hay dos formas distintas y no son intercambiables.**
///
/// - **PEP 621** (`[project]`): `dependencies` es una LISTA de specs de
///   requirement (`"requests>=2.0"`), igual que una línea de
///   `requirements.txt` — así que se reusa el mismo separador en vez de
///   escribir un segundo.
/// - **Poetry** (`[tool.poetry.dependencies]`): es una TABLA `nombre = spec`,
///   con el mismo caso de cadena-o-tabla que Cargo.
///
/// Un archivo puede traer las dos; se leen ambas y cada dependencia conserva
/// de qué sección salió.
fn parse_pyproject(source: &str) -> ManifestAnalysisResult {
    let value: toml::Value = match source.parse::<toml::Value>() {
        Ok(v) => v,
        Err(e) => return empty_result("python", Some(format!("pyproject.toml inválido: {e}"))),
    };
    let mut deps = Vec::new();

    // PEP 621: [project] dependencies = ["requests>=2.0", ...]
    if let Some(lista) = value.get("project").and_then(|p| p.get("dependencies")).and_then(|d| d.as_array()) {
        for item in lista.iter().filter_map(|x| x.as_str()) {
            if let Some((name, spec)) = separar_requirement(item) {
                deps.push(Dependency {
                    pinning: pinning_of(&spec),
                    line: line_of_key(source, &name),
                    name,
                    spec,
                    section: "project.dependencies".to_string(),
                });
            }
        }
    }
    // PEP 735 / PEP 621 extras: [project.optional-dependencies] grupo = [...]
    if let Some(grupos) =
        value.get("project").and_then(|p| p.get("optional-dependencies")).and_then(|d| d.as_table())
    {
        for (grupo, lista) in grupos {
            let Some(arr) = lista.as_array() else { continue };
            for item in arr.iter().filter_map(|x| x.as_str()) {
                if let Some((name, spec)) = separar_requirement(item) {
                    deps.push(Dependency {
                        pinning: pinning_of(&spec),
                        line: line_of_key(source, &name),
                        name,
                        spec,
                        section: format!("project.optional-dependencies.{grupo}"),
                    });
                }
            }
        }
    }
    // Poetry: [tool.poetry.dependencies] nombre = spec
    for (ruta, seccion) in [("dependencies", "tool.poetry.dependencies"), ("dev-dependencies", "tool.poetry.dev-dependencies")] {
        let Some(t) = value
            .get("tool")
            .and_then(|x| x.get("poetry"))
            .and_then(|x| x.get(ruta))
            .and_then(|x| x.as_table())
        else {
            continue;
        };
        for (name, v) in t {
            // `python = "^3.11"` no es una dependencia del proyecto: es el
            // intérprete requerido. Contarla inflaría el total con algo que
            // nadie puede actualizar como paquete.
            if name == "python" {
                continue;
            }
            let spec = spec_de_cargo(v);
            deps.push(Dependency {
                pinning: pinning_of(&spec),
                line: line_of_key(source, name),
                name: name.clone(),
                spec,
                section: seccion.to_string(),
            });
        }
    }

    build_result("python", deps, None)
}

pub fn analyze_manifest(filename: &str, content: &str) -> ManifestAnalysisResult {
    let base = filename.rsplit(['/', '\\']).next().unwrap_or(filename).to_lowercase();
    if base == "package.json" {
        parse_package_json(content)
    } else if base.starts_with("requirements") && base.ends_with(".txt") {
        parse_requirements(content)
    } else if base == "cargo.toml" {
        parse_cargo_toml(content)
    } else if base == "pyproject.toml" {
        parse_pyproject(content)
    } else {
        empty_result("unknown", Some(format!("`{base}` no es un manifest reconocido por esta porción")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Clasificación de specs ───────────────────────────────────────────

    #[test]
    fn clasificacion_de_pinning() {
        assert_eq!(pinning_of("1.2.3"), "exact");
        assert_eq!(pinning_of("==1.2.3"), "exact");
        assert_eq!(pinning_of("^1.2.3"), "ranged");
        assert_eq!(pinning_of("~1.2"), "ranged");
        assert_eq!(pinning_of(">=1.2,<2.0"), "ranged");
        assert_eq!(pinning_of("*"), "unbounded");
        assert_eq!(pinning_of("latest"), "unbounded");
        assert_eq!(pinning_of(""), "unbounded");
        assert_eq!(pinning_of(">=2.0"), "unbounded");
        assert_eq!(pinning_of("git+https://github.com/x/y.git"), "external_source");
    }

    #[test]
    fn mayor_o_igual_con_cota_superior_no_es_ilimitado() {
        // La distincion que hace util al check: `>=1.2` admite la 9.0 de
        // mañana; `>=1.2,<2.0` no.
        assert_eq!(pinning_of(">=1.2"), "unbounded");
        assert_eq!(pinning_of(">=1.2,<2.0"), "ranged");
    }

    // ── package.json ─────────────────────────────────────────────────────

    const PKG: &str = r#"{
  "name": "demo",
  "dependencies": {
    "lodash": "*",
    "express": "^4.18.0",
    "left-pad": "1.3.0"
  },
  "devDependencies": {
    "vitest": ">=1.0"
  }
}"#;

    #[test]
    fn package_json_extrae_dependencias_de_todas_las_secciones() {
        let r = analyze_manifest("package.json", PKG);
        assert_eq!(r.summary.total, 4);
        assert_eq!(r.summary.production, 3);
        assert_eq!(r.summary.development, 1);
        assert_eq!(r.summary.exact, 1);
        assert_eq!(r.summary.ranged, 1);
        assert_eq!(r.summary.unbounded, 2);
    }

    #[test]
    fn asterisco_y_mayor_igual_abierto_se_marcan() {
        let r = analyze_manifest("package.json", PKG);
        let marcadas: Vec<&str> = r
            .findings
            .iter()
            .filter(|f| f.kind == "unbounded_dependency")
            .map(|f| f.dependency.as_str())
            .collect();
        assert!(marcadas.contains(&"lodash"));
        assert!(marcadas.contains(&"vitest"));
        assert_eq!(marcadas.len(), 2, "express (^) y left-pad (exacta) no deben marcarse");
    }

    #[test]
    fn los_hallazgos_traen_la_linea_real_de_la_clave() {
        let r = analyze_manifest("package.json", PKG);
        let lodash = r.findings.iter().find(|f| f.dependency == "lodash").unwrap();
        assert_eq!(lodash.line, 4, "`\"lodash\"` esta en la linea 4 del manifest");
    }

    #[test]
    fn dependencia_de_git_se_marca_como_fuente_externa() {
        let src = r#"{"dependencies": {"mylib": "git+https://github.com/a/b.git"}}"#;
        let r = analyze_manifest("package.json", src);
        assert!(r.findings.iter().any(|f| f.kind == "external_source_dependency"));
    }

    #[test]
    fn misma_dependencia_en_dos_secciones_con_specs_distintos_se_marca() {
        let src = r#"{"dependencies": {"typescript": "^5.0.0"}, "devDependencies": {"typescript": "^4.9.0"}}"#;
        let r = analyze_manifest("package.json", src);
        let dup = r.findings.iter().find(|f| f.kind == "duplicate_dependency").unwrap();
        assert_eq!(dup.severity, "Medium");
        assert!(dup.message.contains("Cuál gana depende del instalador"));
    }

    #[test]
    fn misma_dependencia_con_el_mismo_spec_es_severidad_baja() {
        let src = r#"{"dependencies": {"typescript": "^5.0.0"}, "devDependencies": {"typescript": "^5.0.0"}}"#;
        let r = analyze_manifest("package.json", src);
        let dup = r.findings.iter().find(|f| f.kind == "duplicate_dependency").unwrap();
        assert_eq!(dup.severity, "Low");
    }

    #[test]
    fn package_json_invalido_reporta_error_sin_panic() {
        let r = analyze_manifest("package.json", "{ no es json");
        assert!(r.error.is_some());
        assert_eq!(r.summary.total, 0);
    }

    #[test]
    fn package_json_sin_dependencias_no_rompe() {
        let r = analyze_manifest("package.json", r#"{"name": "x", "version": "1.0.0"}"#);
        assert_eq!(r.summary.total, 0);
        assert!(r.findings.is_empty());
        assert!(r.error.is_none());
    }

    // ── requirements.txt ─────────────────────────────────────────────────

    const REQ: &str = "# comentario\nrequests\nnumpy==1.26.0\nflask>=2.0\ndjango>=4.0,<5.0\n-r otro.txt\n--index-url https://x\n\nrequests[security]>=2.0,<3.0\n";

    #[test]
    fn requirements_saltea_comentarios_vacias_y_directivas() {
        let r = analyze_manifest("requirements.txt", REQ);
        let nombres: Vec<&str> = r.dependencies.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(nombres, vec!["requests", "numpy", "flask", "django", "requests"]);
    }

    #[test]
    fn requirements_sin_version_es_ilimitada() {
        let r = analyze_manifest("requirements.txt", REQ);
        let f = r.findings.iter().find(|f| f.dependency == "requests" && f.kind == "unbounded_dependency").unwrap();
        assert!(f.message.contains("sin versión"));
        assert_eq!(f.line, 2);
    }

    #[test]
    fn requirements_con_rango_acotado_no_dispara() {
        let r = analyze_manifest("requirements.txt", REQ);
        assert!(!r.findings.iter().any(|f| f.dependency == "django" && f.kind == "unbounded_dependency"));
        assert!(!r.findings.iter().any(|f| f.dependency == "numpy"));
    }

    #[test]
    fn los_extras_no_forman_parte_del_nombre() {
        // `requests[security]` es el paquete `requests` -- si el extra
        // quedara en el nombre, el duplicado con la linea 2 no se detectaria.
        let r = analyze_manifest("requirements.txt", REQ);
        assert!(r.findings.iter().any(|f| f.kind == "duplicate_dependency" && f.dependency == "requests"));
    }

    #[test]
    fn requirements_con_otro_nombre_tambien_se_reconoce() {
        let r = analyze_manifest("requirements-dev.txt", "pytest\n");
        assert_eq!(r.summary.total, 1);
        assert_eq!(r.summary.ecosystem, "pypi");
    }

    #[test]
    fn archivo_no_reconocido_devuelve_error_no_panic() {
        let r = analyze_manifest("tsconfig.json", "{}");
        assert!(r.error.is_some());
        assert_eq!(r.summary.total, 0);
    }

    #[test]
    fn la_nota_aclara_que_no_hay_consulta_de_cves_ni_de_uso() {
        let r = analyze_manifest("package.json", PKG);
        assert!(r.note.contains("CVE"));
        assert!(r.note.contains("no se use"));
    }

    #[test]
    fn hallazgos_ordenados_por_linea() {
        let r = analyze_manifest("package.json", PKG);
        let lines: Vec<usize> = r.findings.iter().map(|f| f.line).collect();
        assert!(lines.windows(2).all(|w| w[0] <= w[1]), "sin ordenar: {lines:?}");
    }
    // ── Cargo.toml ────────────────────────────────────────────────────────

    const CARGO: &str = r#"
[package]
name = "demo"
version = "0.1.0"

[dependencies]
serde = { version = "1", features = ["derive"] }
regex = "1.10"
anyhow = "*"
local = { path = "../otra" }
desde_git = { git = "https://github.com/x/y" }
heredada = { workspace = true }

[dev-dependencies]
criterion = "0.5"

[build-dependencies]
cc = "1.0"

[target.'cfg(windows)'.dependencies]
winapi = "0.3"
"#;

    /// **El caso que más fácil se hace mal.** `serde = { version = "1", … }`
    /// y `regex = "1.10"` son la misma dependencia escrita de dos formas, y
    /// la primera es la que usa cualquier `Cargo.toml` real en cuanto pide
    /// una feature. Leer solo la cadena dejaría afuera justo las más
    /// configuradas.
    #[test]
    fn un_spec_de_tabla_se_lee_igual_que_uno_de_cadena() {
        let r = analyze_manifest("Cargo.toml", CARGO);
        assert!(r.error.is_none(), "{:?}", r.error);
        let serde = r.dependencies.iter().find(|d| d.name == "serde").expect("falta serde");
        assert_eq!(serde.spec, "1");
        let regex = r.dependencies.iter().find(|d| d.name == "regex").expect("falta regex");
        assert_eq!(regex.spec, "1.10");
    }

    /// **Una tabla sin `version` no está sin acotar: apunta a otro lado.**
    /// Marcarla como "cualquier versión futura" sería falso.
    #[test]
    fn path_git_y_workspace_no_se_confunden_con_version_libre() {
        let r = analyze_manifest("Cargo.toml", CARGO);
        let por = |n: &str| r.dependencies.iter().find(|d| d.name == n).unwrap().spec.clone();
        assert!(por("local").starts_with("path:"), "{}", por("local"));
        assert!(por("desde_git").starts_with("git+"), "{}", por("desde_git"));
        assert_eq!(por("heredada"), "workspace");
    }

    #[test]
    fn se_leen_las_tres_secciones_y_las_de_target() {
        let r = analyze_manifest("Cargo.toml", CARGO);
        let secciones: Vec<&str> = r.dependencies.iter().map(|d| d.section.as_str()).collect();
        assert!(secciones.contains(&"dependencies"));
        assert!(secciones.contains(&"dev-dependencies"));
        assert!(secciones.contains(&"build-dependencies"));
        assert!(
            secciones.iter().any(|s| s.starts_with("target.") && s.ends_with(".dependencies")),
            "sin las de target: {secciones:?}"
        );
        assert_eq!(r.summary.ecosystem, "cargo");
    }

    #[test]
    fn un_cargo_toml_roto_devuelve_el_error_y_no_panickea() {
        let r = analyze_manifest("Cargo.toml", "[dependencies\nserde = ");
        assert!(r.error.is_some());
        assert!(r.dependencies.is_empty());
    }

    /// **Cargo invierte el default de npm y pip.** `serde = "1"` NO fija la
    /// versión: Cargo le pone el caret solo y acepta cualquier `1.x`. Fijar
    /// de verdad exige `=1.2.3`. Reusar el clasificador de npm reportaba
    /// sobre el `Cargo.toml` de este repo **26 de 26 "exactas"** cuando
    /// ninguna lo está.
    #[test]
    fn una_version_pelada_en_cargo_es_un_rango_no_una_fijacion() {
        assert_eq!(pinning_cargo("1"), "ranged");
        assert_eq!(pinning_cargo("1.10"), "ranged");
        assert_eq!(pinning_cargo("^1.2"), "ranged");
        assert_eq!(pinning_cargo("~1.2"), "ranged");
        // Lo unico que fija de verdad.
        assert_eq!(pinning_cargo("=1.2.3"), "exact");
    }

    /// Y el mismo texto clasifica distinto segun el ecosistema, que es
    /// justamente el punto.
    #[test]
    fn el_mismo_spec_da_distinto_en_npm_y_en_cargo() {
        assert_eq!(pinning_of("1.2.3"), "exact");
        assert_eq!(pinning_cargo("1.2.3"), "ranged");
    }

    #[test]
    fn un_origen_que_no_es_version_no_se_cuenta_como_rango() {
        assert_eq!(pinning_cargo("path:../otra"), "external_source");
        assert_eq!(pinning_cargo("git+https://github.com/x/y"), "external_source");
        assert_eq!(pinning_cargo("workspace"), "external_source");
    }

    /// El repo mismo: ninguna de sus dependencias esta fijada, y el resumen
    /// tiene que decirlo.
    #[test]
    fn el_resumen_de_un_cargo_toml_tipico_no_reporta_fijaciones_falsas() {
        let r = analyze_manifest("Cargo.toml", CARGO);
        assert_eq!(r.summary.exact, 0, "ninguna dependencia de CARGO usa `=`");
        assert!(r.summary.ranged >= 4, "las versiones peladas son rangos: {:?}", r.summary.ranged);
    }

    /// Un `*` sigue siendo "cualquier versión futura", acá también.
    #[test]
    fn el_asterisco_de_cargo_cuenta_como_sin_acotar() {
        let r = analyze_manifest("Cargo.toml", CARGO);
        let anyhow = r.dependencies.iter().find(|d| d.name == "anyhow").unwrap();
        assert_eq!(anyhow.pinning, "unbounded");
    }

    // ── pyproject.toml ────────────────────────────────────────────────────

    const PYPROJECT: &str = r#"
[project]
name = "demo"
dependencies = [
  "requests>=2.0",
  "pydantic==2.5.0",
  "rich",
  "httpx[http2]>=0.27",
  "tomli; python_version < '3.11'",
]

[project.optional-dependencies]
dev = ["pytest>=8", "ruff"]

[tool.poetry.dependencies]
python = "^3.11"
fastapi = "^0.110"
uvicorn = { version = "0.29", extras = ["standard"] }
"#;

    /// PEP 621: `dependencies` es una LISTA de requirements, la misma
    /// gramática de `requirements.txt` — por eso se reusa su separador.
    #[test]
    fn pep621_lee_la_lista_de_dependencias() {
        let r = analyze_manifest("pyproject.toml", PYPROJECT);
        assert!(r.error.is_none(), "{:?}", r.error);
        let por = |n: &str| r.dependencies.iter().find(|d| d.name == n).cloned();
        assert_eq!(por("requests").expect("requests").spec, ">=2.0");
        assert_eq!(por("pydantic").expect("pydantic").pinning, "exact");
        assert_eq!(por("rich").expect("rich").spec, "");
    }

    /// Los extras son parte del NOMBRE, no del spec — mismo criterio que
    /// `requirements.txt` ya aplicaba.
    #[test]
    fn los_extras_no_entran_en_el_nombre() {
        let r = analyze_manifest("pyproject.toml", PYPROJECT);
        assert!(r.dependencies.iter().any(|d| d.name == "httpx"), "{:?}", r.dependencies.iter().map(|d| &d.name).collect::<Vec<_>>());
        assert!(!r.dependencies.iter().any(|d| d.name.contains('[')));
    }

    /// **Un marcador de entorno no es parte del nombre ni del spec.**
    /// `"tomli; python_version < '3.11'"` es la dependencia `tomli`.
    #[test]
    fn un_marcador_de_entorno_no_ensucia_el_nombre() {
        let r = analyze_manifest("pyproject.toml", PYPROJECT);
        let t = r.dependencies.iter().find(|d| d.name == "tomli").expect("falta tomli");
        assert!(!t.spec.contains("python_version"), "spec sucio: {}", t.spec);
    }

    #[test]
    fn las_opcionales_conservan_su_grupo() {
        let r = analyze_manifest("pyproject.toml", PYPROJECT);
        let pytest = r.dependencies.iter().find(|d| d.name == "pytest").expect("falta pytest");
        assert!(pytest.section.contains("optional-dependencies"), "{}", pytest.section);
        assert!(pytest.section.ends_with("dev"), "{}", pytest.section);
    }

    /// Poetry usa una TABLA, no una lista: las dos formas conviven en el
    /// mismo archivo y se leen las dos.
    #[test]
    fn poetry_y_pep621_conviven_en_el_mismo_archivo() {
        let r = analyze_manifest("pyproject.toml", PYPROJECT);
        assert!(r.dependencies.iter().any(|d| d.name == "fastapi" && d.section.contains("poetry")));
        let uvicorn = r.dependencies.iter().find(|d| d.name == "uvicorn").expect("falta uvicorn");
        assert_eq!(uvicorn.spec, "0.29", "el spec de tabla de Poetry se lee igual");
    }

    /// **`python` no es una dependencia del proyecto, es el intérprete.**
    /// Contarla infla el total con algo que nadie actualiza como paquete.
    #[test]
    fn la_version_de_python_no_cuenta_como_dependencia() {
        let r = analyze_manifest("pyproject.toml", PYPROJECT);
        assert!(!r.dependencies.iter().any(|d| d.name == "python"), "{:?}", r.dependencies.iter().map(|d| &d.name).collect::<Vec<_>>());
    }

    #[test]
    fn un_pyproject_roto_devuelve_el_error() {
        let r = analyze_manifest("pyproject.toml", "[project\nname =");
        assert!(r.error.is_some());
    }

    /// El separador extraído tiene que seguir dando lo mismo para el formato
    /// del que salió.
    #[test]
    fn el_separador_extraido_no_cambio_requirements_txt() {
        let r = analyze_manifest("requirements.txt", "requests>=2.0\n# comentario\n-r otro.txt\npydantic==2.5.0\n");
        assert_eq!(r.dependencies.len(), 2);
        assert_eq!(r.dependencies[0].name, "requests");
        assert_eq!(r.dependencies[1].pinning, "exact");
    }

}
