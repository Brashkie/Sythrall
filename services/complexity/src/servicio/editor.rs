//! **La inteligencia del editor — Fase 35, portado desde
//! `routers/intelligence.py`.**
//!
//! Los siete endpoints que alimentan el editor Monaco: lint rápido mientras
//! se escribe, análisis completo en reposo, hover, ir-a-definición,
//! referencias, autocompletado y renombrar.
//!
//! **De las 1.445 líneas del router, buena parte eran fallbacks.** Los de
//! `definition`/`references` existían porque Python nunca había dependido del
//! sidecar para eso y su propio docstring decía que *"perderlo sería una
//! regresión real"*. Adentro del motor esa rama es imposible —el mismo
//! argumento que en `diagrama.rs`— así que se borraron: `ast::symbols` ya
//! tiene las cuatro funciones (`find_definitions_python`,
//! `find_references_python`, y sus pares de JS/TS).
//!
//! **Y `flake8`/`pylint` tampoco están**, por la misma decisión que en
//! `servicio::analisis`: invocarlos por subprocess dejaba al producto
//! necesitando un Python instalado. `/intel/analyze` queda con la
//! complejidad, el Big-O y los hallazgos del motor, que es lo que ya daba el
//! 80 % de sus markers.
//!
//! **Las severidades son las de Monaco** (8=Error, 4=Warning, 2=Info,
//! 1=Hint), no las del resto del motor: este módulo habla con un editor
//! concreto y traduce en el borde.

use serde::Deserialize;
use serde_json::{json, Map, Value};

/// Severidades de Monaco. No son las de `analisis.rs` (`error`/`warning`/
/// `info` como texto) porque Monaco las quiere numéricas.
const ERROR: u8 = 8;
const AVISO: u8 = 4;
const INFO: u8 = 2;
const PISTA: u8 = 1;

/// El largo de línea que dispara `E501`, igual que en `analisis.rs`.
const MAX_LARGO_DE_LINEA: usize = 120;
/// Cuántos caracteres del docstring entran en el hover.
const MAX_DOCSTRING: usize = 200;
/// Cuántos caracteres de la línea se muestran como vista previa.
const MAX_PREVIEW: usize = 70;

#[derive(Deserialize)]
pub struct ArchivoRequest {
    #[serde(default = "archivo_por_defecto")]
    pub filename: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub tools: Vec<String>,
}

fn archivo_por_defecto() -> String {
    "script.py".to_string()
}

#[derive(Deserialize)]
pub struct PosicionRequest {
    #[serde(default = "archivo_por_defecto")]
    pub filename: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub line: usize,
    #[serde(default)]
    pub column: usize,
    #[serde(default)]
    pub symbol_name: String,
}

#[derive(Deserialize)]
pub struct SimboloRequest {
    #[serde(default = "archivo_por_defecto")]
    pub filename: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub symbol_name: String,
}

#[derive(Deserialize)]
pub struct CompletadoRequest {
    #[serde(default = "archivo_por_defecto")]
    pub filename: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub prefix: String,
}

#[derive(Deserialize)]
pub struct RenombrarRequest {
    #[serde(default = "archivo_por_defecto")]
    pub filename: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub symbol_name: String,
    #[serde(default)]
    pub new_name: String,
}

fn extension(nombre: &str) -> String {
    match nombre.rfind('.') {
        Some(0) | None => String::new(),
        Some(i) => nombre[i..].to_ascii_lowercase(),
    }
}

fn es_python(ext: &str) -> bool {
    ext == ".py"
}

fn es_jsts(ext: &str) -> Option<bool> {
    match ext {
        ".ts" | ".tsx" => Some(true),
        ".js" | ".jsx" => Some(false),
        _ => None,
    }
}

/// Un marker de Monaco.
fn marker(
    linea: usize,
    col_ini: usize,
    col_fin: usize,
    mensaje: &str,
    severidad: u8,
    codigo: &str,
    fuente: &str,
) -> Value {
    json!({
        "startLineNumber": linea,
        "startColumn": col_ini.max(1),
        "endLineNumber": linea,
        "endColumn": col_fin.max(col_ini + 1),
        "message": mensaje,
        "severity": severidad,
        "source": fuente,
        "code": codigo,
    })
}

// ── Lint rápido ───────────────────────────────────────────────────────────

/// **El camino rápido: se dispara en cada debounce de 300 ms.**
///
/// Comparte las reglas con `servicio::analisis::reglas_de_ast` en lo
/// conceptual, pero **no el código**, y es a propósito: allá el shape es
/// `{tool, line, col, severity, code, message}` para el panel de problemas y
/// acá es el de un marker de Monaco, con rangos de columna que el editor usa
/// para subrayar. Unificarlos obligaría a una traducción en cada llamada del
/// endpoint más frecuente del editor.
///
/// Y acá hay una regla que el panel no tiene: `TODO`/`FIXME`/`HACK` como
/// pista, que sólo tiene sentido subrayado en el editor.
pub fn lint_rapido(filename: &str, contenido: &str) -> Value {
    let inicio = std::time::Instant::now();
    let ext = extension(filename);
    let markers = if es_python(&ext) {
        lint_python(contenido)
    } else if es_jsts(&ext).is_some() {
        lint_jsts(contenido)
    } else {
        // Otros lenguajes no tienen lint rápido. El motor los parsea de
        // verdad en `/static/parse`, pero eso no entra en un presupuesto de
        // 300 ms por pulsación.
        Vec::new()
    };
    let ms = (inicio.elapsed().as_secs_f64() * 1000.0 * 10.0).round() / 10.0;
    json!({ "markers": markers, "ms": ms, "source": "fast" })
}

fn lint_python(contenido: &str) -> Vec<Value> {
    let suite = match crate::ast::parser::parse_module(contenido) {
        Ok(s) => s,
        Err(e) => {
            let linea = linea_de_offset(&e, contenido);
            return vec![marker(
                linea,
                1,
                11,
                &format!("SyntaxError: {e}"),
                ERROR,
                "E001",
                "ast",
            )];
        }
    };
    let indice = crate::ast::parser::LineIndex::build(contenido);
    let mut de_stmts: Vec<Value> = Vec::new();
    let mut de_exprs: Vec<Value> = Vec::new();

    let mut en_stmt = |stmt: &rustpython_parser::ast::Stmt| {
        use rustpython_parser::ast::Stmt;
        let ubicar = |r: &rustpython_parser::text_size::TextRange| {
            (indice.line_of(r.start().to_usize()), indice.column_of(r.start().to_usize()))
        };
        match stmt {
            Stmt::Assert(s) => {
                let (l, c) = ubicar(&s.range);
                de_stmts.push(marker(
                    l,
                    c,
                    c + 6,
                    "assert puede desactivarse con -O",
                    AVISO,
                    "W001",
                    "ast",
                ));
            }
            Stmt::Global(s) => {
                let (l, c) = ubicar(&s.range);
                let nombres: Vec<&str> = s.names.iter().map(|n| n.as_str()).collect();
                de_stmts.push(marker(
                    l,
                    c,
                    c + 6,
                    &format!("Variable global: {}", nombres.join(", ")),
                    AVISO,
                    "W003",
                    "ast",
                ));
            }
            Stmt::Try(s) => {
                for h in &s.handlers {
                    let rustpython_parser::ast::ExceptHandler::ExceptHandler(h) = h;
                    if h.type_.is_none() {
                        let (l, c) = ubicar(&h.range);
                        de_stmts.push(marker(
                            l,
                            c,
                            c + 6,
                            "except genérico captura todo",
                            AVISO,
                            "W002",
                            "ast",
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
                    let inicio = c.range.start().to_usize();
                    let (l, col) = (indice.line_of(inicio), indice.column_of(inicio));
                    de_exprs.push(marker(
                        l,
                        col,
                        col + 5,
                        "print() detectado — usa logging",
                        INFO,
                        "C002",
                        "ast",
                    ));
                }
            }
        }
    };
    crate::ast::walk::walk_stmts(&suite, &mut en_stmt, &mut en_expr);

    let mut markers = de_stmts;
    markers.extend(de_exprs);
    markers.extend(reglas_de_linea(contenido, "ast"));
    markers
}

/// Las reglas que van por línea cruda, sin AST: largo y marcas de pendiente.
///
/// Se cuentan **caracteres y no bytes**, igual que en `analisis.rs`: una línea
/// de 100 caracteres acentuados no mide 200.
fn reglas_de_linea(contenido: &str, fuente: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for (i, linea) in contenido.lines().enumerate() {
        let n = i + 1;
        let largo = linea.chars().count();
        if largo > MAX_LARGO_DE_LINEA {
            out.push(marker(
                n,
                MAX_LARGO_DE_LINEA + 1,
                largo + 1,
                &format!("Línea larga ({largo} chars, máx {MAX_LARGO_DE_LINEA})"),
                AVISO,
                "E501",
                fuente,
            ));
        }
        if let Some((col, palabra)) = marca_de_pendiente(linea) {
            out.push(marker(
                n,
                col + 1,
                col + 1 + palabra.len(),
                &format!("{palabra}: pendiente"),
                PISTA,
                "W100",
                fuente,
            ));
        }
    }
    out
}

/// `TODO`, `FIXME` o `HACK`, sin distinguir mayúsculas. Devuelve la columna
/// (0-based) y la palabra como está escrita.
fn marca_de_pendiente(linea: &str) -> Option<(usize, String)> {
    let mayus = linea.to_uppercase();
    ["TODO", "FIXME", "HACK"]
        .iter()
        .filter_map(|p| mayus.find(p).map(|i| (i, *p)))
        .min_by_key(|(i, _)| *i)
        .map(|(i, p)| {
            // Se devuelve el texto original, no el de la búsqueda: un
            // `todo:` en minúscula se reporta como lo escribió el autor.
            (i, linea.chars().skip(i).take(p.len()).collect())
        })
}

/// Lint de JS/TS por línea. **Son las mismas reglas para los dos**, igual que
/// en el router: `_fast_lint_js` llamaba a `_fast_lint_ts`.
fn lint_jsts(contenido: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for (i, linea) in contenido.lines().enumerate() {
        let n = i + 1;
        if let Some(col) = buscar_console(linea) {
            out.push(marker(n, col + 1, col + 12, "console en producción", INFO, "TS001", "ts-lint"));
        }
        if let Some(col) = buscar_any(linea) {
            out.push(marker(
                n,
                col + 1,
                col + 5,
                "Tipo 'any' reduce seguridad de tipos",
                AVISO,
                "TS002",
                "ts-lint",
            ));
        }
        if palabra_suelta(linea, "debugger") {
            out.push(marker(n, 1, linea.chars().count().max(2), "debugger encontrado", ERROR, "TS003", "ts-lint"));
        }
        if linea.trim_start().starts_with("var ") {
            let col = linea.len() - linea.trim_start().len();
            out.push(marker(
                n,
                col + 1,
                col + 4,
                "Usa 'const' o 'let' en vez de 'var'",
                AVISO,
                "TS004",
                "ts-lint",
            ));
        }
        if let Some(col) = igualdad_laxa(linea) {
            out.push(marker(n, col + 1, col + 3, "Usa === en vez de ==", AVISO, "TS005", "ts-lint"));
        }
    }
    out.extend(reglas_de_linea(contenido, "ts-lint"));
    out
}

fn buscar_console(linea: &str) -> Option<usize> {
    let i = linea.find("console.")?;
    let resto = &linea[i + 8..];
    ["log", "debug", "error", "warn"].iter().find(|m| resto.starts_with(**m)).map(|_| i)
}

/// `: any` como anotación de tipo. **No matchea `anything` ni `company`**: el
/// patrón del router era `:\s*any\b`, con el límite de palabra.
fn buscar_any(linea: &str) -> Option<usize> {
    let bytes = linea.as_bytes();
    let mut i = 0;
    while let Some(rel) = linea[i..].find(':') {
        let pos = i + rel;
        let resto = linea[pos + 1..].trim_start();
        if resto.starts_with("any") {
            let despues = resto.as_bytes().get(3);
            if despues.is_none_or(|c| !c.is_ascii_alphanumeric() && *c != b'_') {
                return Some(pos);
            }
        }
        i = pos + 1;
        if i >= bytes.len() {
            break;
        }
    }
    None
}

fn palabra_suelta(linea: &str, palabra: &str) -> bool {
    let Some(i) = linea.find(palabra) else { return false };
    let antes = linea[..i].chars().next_back();
    let despues = linea[i + palabra.len()..].chars().next();
    let limite = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric() && c != '_');
    limite(antes) && limite(despues)
}

/// `==` que no es `===`, `!==`, `<=` ni `>=`.
fn igualdad_laxa(linea: &str) -> Option<usize> {
    let b = linea.as_bytes();
    for i in 0..b.len().saturating_sub(1) {
        if b[i] == b'=' && b[i + 1] == b'=' {
            let antes = if i == 0 { None } else { Some(b[i - 1]) };
            let despues = b.get(i + 2);
            if antes.is_none_or(|c| !matches!(c, b'=' | b'!' | b'<' | b'>'))
                && despues.is_none_or(|c| *c != b'=')
            {
                return Some(i);
            }
        }
    }
    None
}

fn linea_de_offset(mensaje: &str, contenido: &str) -> usize {
    let Some(resto) = mensaje.split("byte offset ").nth(1) else { return 1 };
    let digitos: String = resto.chars().take_while(char::is_ascii_digit).collect();
    let Ok(offset) = digitos.parse::<usize>() else { return 1 };
    contenido[..offset.min(contenido.len())].matches('\n').count() + 1
}

// ── Análisis completo ─────────────────────────────────────────────────────

/// **El camino lento: se dispara cuando se deja de escribir (~2 s).**
///
/// Acá sí vale `analyze_rich`, al contrario que en `servicio::metricas`: el
/// presupuesto es de segundos y lo que se muestra —Big-O con su razón,
/// hallazgos de seguridad, smells, naming— es exactamente lo que esa función
/// calcula. Los 16 ms que la descartaban para la barra del editor acá están
/// bien gastados.
pub fn analisis_completo(filename: &str, contenido: &str) -> Value {
    let inicio = std::time::Instant::now();
    let ext = extension(filename);
    let mut markers: Vec<Value> = Vec::new();
    let mut metricas = Map::new();
    let mut big_o: Vec<Value> = Vec::new();
    let mut seguridad: Vec<Value> = Vec::new();
    let mut estructurales: Vec<Value> = Vec::new();
    let mut naming: Vec<Value> = Vec::new();

    if es_python(&ext) {
        let liviano = crate::analyze(contenido);
        metricas.insert(
            "complexity".into(),
            serde_json::to_value(&liviano.functions).unwrap_or_else(|_| json!([])),
        );
        metricas.insert(
            "maintainability".into(),
            liviano.mi.map_or(Value::Null, |v| json!(v)),
        );
        metricas.insert(
            "halstead".into(),
            liviano
                .halstead
                .as_ref()
                .map_or(Value::Null, |h| serde_json::to_value(h).unwrap_or(Value::Null)),
        );

        let rico = crate::servicio::rich::analyze_rich(contenido);
        for f in &rico.functions {
            big_o.push(json!({
                "name": f.name,
                "line": f.line,
                "big_o": f.big_o,
                "reason": f.big_o_reason,
                "complexity": f.complexity,
            }));
        }
        seguridad = serde_json::to_value(&rico.security_findings)
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        estructurales = serde_json::to_value(&rico.structural_smells)
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        naming = serde_json::to_value(&rico.naming_smells)
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();

        // Los imports sin usar van como markers, no como findings: el editor
        // los subraya en su línea.
        for muerto in &rico.dead_code {
            markers.push(marker(
                muerto.line,
                1,
                1,
                &format!("Import posiblemente no usado: {}", muerto.module),
                INFO,
                "W200",
                "static",
            ));
        }
    }

    // **La deduplicación se conserva aunque ya no haga falta.** Existía
    // porque flake8 y pylint reportaban el mismo problema en la misma línea;
    // sin ellos no hay dos fuentes que solapen. Se deja porque el orden por
    // línea es parte del shape que Monaco consume.
    let mut vistos: std::collections::BTreeSet<(i64, String)> = std::collections::BTreeSet::new();
    markers.retain(|m| {
        let l = m.get("startLineNumber").and_then(Value::as_i64).unwrap_or(0);
        let c = m.get("code").and_then(Value::as_str).unwrap_or("").to_string();
        vistos.insert((l, c))
    });
    markers.sort_by_key(|m| m.get("startLineNumber").and_then(Value::as_i64).unwrap_or(0));

    let ms = (inicio.elapsed().as_secs_f64() * 1000.0 * 10.0).round() / 10.0;
    json!({
        "markers": markers,
        "metrics": metricas,
        "big_o": big_o,
        "security_findings": seguridad,
        "structural_smells": estructurales,
        "naming_smells": naming,
        "ms": ms,
        "source": "heavy",
    })
}

// ── Hover ─────────────────────────────────────────────────────────────────

fn etiqueta_de_cc(cc: u32) -> &'static str {
    match cc {
        1..=5 => "simple",
        6..=10 => "moderada",
        11..=20 => "compleja",
        21..=30 => "muy compleja",
        _ => "refactorizar",
    }
}

/// El hover de Monaco para la función bajo el cursor.
pub fn hover(req: &PosicionRequest) -> Value {
    let ext = extension(&req.filename);
    if !es_python(&ext) {
        // JS/TS tenía su propio hover más pobre en el router; se deja sin
        // hover en vez de portarlo a medias, y queda anotado en el ROADMAP.
        return json!({ "markdown": "", "range": Value::Null });
    }
    let rico = crate::servicio::rich::analyze_rich(&req.content);
    if rico.error.is_some() {
        return json!({ "markdown": "", "range": Value::Null });
    }

    // La función que contiene el cursor; si hay varias, la que empieza más
    // abajo.
    //
    // **Ojo con las anidadas**: `rich::collect_functions` no entra al cuerpo
    // de un `def`, así que un `def` dentro de otro no está en esta lista y el
    // hover muestra la externa. Es una regresión acotada contra el router,
    // que usaba su propio `ast.walk`; está fijada con un test y explicada ahí.
    let elegida = rico
        .functions
        .iter()
        .filter(|f| f.line <= req.line && req.line <= f.end_line)
        .max_by_key(|f| f.line)
        // Si el cursor no está sobre ninguna, se busca por nombre: el
        // frontend manda `symbol_name` cuando lo sabe.
        .or_else(|| {
            if req.symbol_name.is_empty() {
                None
            } else {
                rico.functions.iter().find(|f| f.name == req.symbol_name)
            }
        });
    let Some(f) = elegida else {
        return json!({ "markdown": "", "range": Value::Null });
    };

    let args: Vec<String> = f.args.clone();
    let prefijo = if f.is_async { "async def " } else { "def " };
    let firma = format!("{prefijo}{}({})", f.name, args.join(", "));

    let mut md = format!("```python\n{firma}\n```");
    if let Some(doc) = f.docstring.as_ref().filter(|d| !d.is_empty()) {
        let corto: String = doc.chars().take(MAX_DOCSTRING).collect();
        let puntos = if doc.chars().count() > MAX_DOCSTRING { "…" } else { "" };
        md.push_str(&format!("\n\n{corto}{puntos}"));
    }

    md.push_str(&format!(
        "\n\n---\n| Métrica | Valor |\n|---------|-------|\n\
         | **Time Complexity (O)** | `{}` |\n\
         | **Cota ajustada (Θ)** | `{}` |\n\
         | **Mejor caso (Ω)** | `{}` |\n\
         | **Razón** | {} |\n\
         | **Space Complexity** | `{}` |\n\
         | **Razón (espacio)** | {} |\n\
         | **Cyclomatic CC** | `{}` — {} |\n\
         | **LOC** | `{}` líneas |\n\
         | **Línea** | `{}` |",
        f.big_o,
        f.big_o_theta,
        f.big_o_omega,
        f.big_o_reason,
        f.space_complexity,
        f.space_reason,
        f.complexity,
        etiqueta_de_cc(f.complexity),
        f.loc,
        f.line,
    ));

    if let Some(nota) = f.recursion_note.as_ref().filter(|n| !n.is_empty()) {
        let insignia = if f.is_tail_recursive { "tail-call" } else { "no tail-call" };
        md.push_str(&format!("\n\n**Recursión** — {insignia}\n\n{nota}"));
    }
    if f.is_async {
        md.push_str("\n\nFunción **async**");
    }
    // El aviso de camino caliente usa el mismo criterio que el heatmap.
    if matches!(f.big_o.as_str(), "O(n²)" | "O(n³)" | "O(2^n)") {
        md.push_str("\n\n**Hot Path** — candidato a Cython / WASM");
    }
    md.push_str(
        "\n\n<sub>O = cota superior (peor caso) · Θ = cota ajustada · Ω = cota inferior \
         (mejor caso) — convención específica de Sythrall. Definiciones generales, más \
         o (cota superior estricta) y ω (cota inferior estricta), en el panel Static.</sub>",
    );

    json!({
        "markdown": md,
        "range": {
            "startLineNumber": f.line,
            "startColumn": 1,
            "endLineNumber": f.end_line,
            "endColumn": 999,
        },
    })
}

// ── Definición y referencias ──────────────────────────────────────────────

/// Relay directo a `ast::symbols`, que ya tenía las cuatro funciones.
pub fn definicion(req: &SimboloRequest) -> Value {
    let simbolo = req.symbol_name.trim();
    if simbolo.is_empty() {
        return json!({ "found": false, "symbol": "", "definitions": [] });
    }
    let ext = extension(&req.filename);
    let defs = if es_python(&ext) {
        crate::ast::symbols::find_definitions_python(&req.content, simbolo)
    } else if let Some(ts) = es_jsts(&ext) {
        crate::ast::symbols::find_definitions_jsts(&req.content, ts, simbolo)
    } else {
        Vec::new()
    };
    json!({
        "found": !defs.is_empty(),
        "symbol": simbolo,
        "filename": req.filename,
        "definitions": serde_json::to_value(&defs).unwrap_or_else(|_| json!([])),
    })
}

pub fn referencias(req: &SimboloRequest) -> Value {
    let simbolo = req.symbol_name.trim();
    if simbolo.is_empty() {
        return json!({ "symbol": "", "references": [], "definition_line": Value::Null, "total": 0 });
    }
    let ext = extension(&req.filename);
    let (refs, linea) = buscar_referencias(&ext, &req.content, simbolo);
    json!({
        "symbol": simbolo,
        "filename": req.filename,
        "references": serde_json::to_value(&refs).unwrap_or_else(|_| json!([])),
        "definition_line": linea.map_or(Value::Null, |l| json!(l)),
        "total": refs.len(),
    })
}

fn buscar_referencias(
    ext: &str,
    contenido: &str,
    simbolo: &str,
) -> (Vec<crate::ast::symbols::SymbolRef>, Option<usize>) {
    if es_python(ext) {
        crate::ast::symbols::find_references_python(contenido, simbolo)
    } else if let Some(ts) = es_jsts(ext) {
        crate::ast::symbols::find_references_jsts(contenido, ts, simbolo)
    } else {
        // Para el resto, la búsqueda por texto: es lo que hacía el router
        // (`_find_references_regex`) y es honesta — no pretende entender el
        // lenguaje, encuentra el identificador.
        (crate::ast::symbols::find_references_regex(contenido, simbolo), None)
    }
}

// ── Autocompletado ────────────────────────────────────────────────────────

/// Los símbolos del archivo, para el autocomplete de Monaco.
///
/// **El orden es parte del contrato**: primero funciones y métodos, después
/// clases, variables, imports e interfaces/tipos, y dentro de cada grupo
/// alfabético. Monaco respeta el orden en que llegan.
pub fn completado(req: &CompletadoRequest) -> Value {
    let ext = extension(&req.filename);
    let mut simbolos: Vec<Value> = if es_python(&ext) {
        completado_python(&req.content)
    } else if let Some(ts) = es_jsts(&ext) {
        completado_jsts(&req.content, ts)
    } else {
        Vec::new()
    };

    let prefijo = req.prefix.to_lowercase();
    if !prefijo.is_empty() {
        simbolos.retain(|s| {
            s.get("label")
                .and_then(Value::as_str)
                .is_some_and(|l| l.to_lowercase().starts_with(&prefijo))
        });
    }

    let peso = |k: &str| match k {
        "function" | "method" => 0,
        "class" => 1,
        "variable" => 2,
        "import" => 3,
        "interface" | "type" => 4,
        _ => 5,
    };
    simbolos.sort_by_key(|s| {
        (
            peso(s.get("kind").and_then(Value::as_str).unwrap_or("")),
            s.get("label").and_then(Value::as_str).unwrap_or("").to_string(),
        )
    });

    json!({
        "symbols": simbolos,
        "total": simbolos.len(),
        "filename": req.filename,
        "prefix": req.prefix,
    })
}

fn simbolo(label: &str, kind: &str, detail: String) -> Value {
    json!({ "label": label, "kind": kind, "detail": detail })
}

fn completado_python(contenido: &str) -> Vec<Value> {
    let rico = crate::servicio::rich::analyze_rich(contenido);
    if rico.error.is_some() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for f in &rico.functions {
        let args: Vec<&str> = f.args.iter().map(String::as_str).filter(|a| *a != "self").collect();
        out.push(simbolo(
            &f.name,
            "function",
            format!("({}) — {} · CC {}", args.join(", "), f.big_o, f.complexity),
        ));
    }
    for c in &rico.classes {
        out.push(simbolo(
            &c.name,
            "class",
            format!("{} métodos, {} atributos", c.methods.len(), c.attributes.len()),
        ));
        for m in &c.methods {
            let args: Vec<&str> =
                m.args.iter().map(String::as_str).filter(|a| *a != "self").collect();
            out.push(simbolo(&m.name, "method", format!("{}({})", c.name, args.join(", "))));
        }
    }
    for i in &rico.imports {
        out.push(simbolo(&i.module, "import", "import".to_string()));
    }
    out
}

fn completado_jsts(contenido: &str, typescript: bool) -> Vec<Value> {
    let r = crate::lenguajes::jsts::parse_js_ts(contenido, typescript);
    let mut out = Vec::new();
    for f in &r.functions {
        out.push(simbolo(&f.name, "function", format!("{} · CC {}", f.big_o, f.complexity)));
    }
    for c in &r.classes {
        // `JsClass` no lleva la lista de métodos (el parser de JS/TS los
        // devuelve en `functions`, no anidados en la clase), así que se
        // reporta lo que sí tiene: de quién hereda.
        let detalle = c
            .extends
            .as_deref()
            .map_or_else(|| "class".to_string(), |b| format!("extends {b}"));
        out.push(simbolo(&c.name, "class", detalle));
    }
    for i in &r.imports {
        out.push(simbolo(&i.module, "import", "import".to_string()));
    }
    for i in &r.interfaces {
        out.push(simbolo(&i.name, "interface", "interface".to_string()));
    }
    for t in &r.types {
        out.push(simbolo(&t.name, "type", "type".to_string()));
    }
    out
}

// ── Renombrar ─────────────────────────────────────────────────────────────

/// `true` si el nombre es un identificador válido: letra o `_` y después
/// alfanuméricos o `_`. Mismo criterio que el `^[a-zA-Z_][a-zA-Z0-9_]*$` del
/// router — **deliberadamente ASCII**, aunque Python acepte identificadores
/// Unicode: ofrecer un renombrado a `café` y que funcione en Python pero no
/// en JS sería peor que no ofrecerlo.
pub fn es_identificador(nombre: &str) -> bool {
    let mut cs = nombre.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Los edits que Monaco aplica para renombrar un símbolo.
pub fn renombrar(req: &RenombrarRequest) -> Value {
    if req.symbol_name.is_empty() || req.new_name.is_empty() {
        return json!({ "valid": false, "edits": [], "error": "Nombres vacíos" });
    }
    if !es_identificador(&req.new_name) {
        return json!({
            "valid": false, "edits": [],
            "error": format!("'{}' no es un identificador válido", req.new_name),
        });
    }
    if req.symbol_name == req.new_name {
        return json!({ "valid": false, "edits": [], "error": "El nombre es igual" });
    }

    let ext = extension(&req.filename);
    let (refs, _) = buscar_referencias(&ext, &req.content, &req.symbol_name);
    if refs.is_empty() {
        return json!({
            "valid": false, "edits": [],
            "error": format!("Símbolo '{}' no encontrado", req.symbol_name),
        });
    }

    let lineas: Vec<&str> = req.content.lines().collect();
    let mut edits: Vec<Value> = Vec::new();
    let mut vistos: std::collections::BTreeSet<(usize, usize)> = std::collections::BTreeSet::new();

    for r in &refs {
        if r.line == 0 || r.line > lineas.len() {
            continue;
        }
        let linea = lineas[r.line - 1];
        // **Se busca el símbolo COMO PALABRA en la línea referenciada**, no
        // se usa la columna de la referencia: una línea puede tener el
        // símbolo varias veces (`x = x + 1`) y las tres ocurrencias hay que
        // renombrarlas. El router hacía lo mismo con `finditer`.
        for (col, _) in ocurrencias_de_palabra(linea, &req.symbol_name) {
            if !vistos.insert((r.line, col)) {
                continue;
            }
            edits.push(json!({
                "range": {
                    "startLineNumber": r.line,
                    "startColumn": col + 1,
                    "endLineNumber": r.line,
                    "endColumn": col + 1 + req.symbol_name.chars().count(),
                },
                "newText": req.new_name,
                "kind": r.kind,
                "preview": linea.trim().chars().take(MAX_PREVIEW).collect::<String>(),
            }));
        }
    }
    edits.sort_by_key(|e| {
        (
            e["range"]["startLineNumber"].as_u64().unwrap_or(0),
            e["range"]["startColumn"].as_u64().unwrap_or(0),
        )
    });

    json!({
        "valid": true,
        "edits": edits,
        "total": edits.len(),
        "old_name": req.symbol_name,
        "new_name": req.new_name,
        "filename": req.filename,
    })
}

/// Las posiciones (en caracteres) donde `palabra` aparece como identificador
/// completo. Equivale al `\b{simbolo}\b` del router.
fn ocurrencias_de_palabra(linea: &str, palabra: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if palabra.is_empty() {
        return out;
    }
    let caracteres: Vec<char> = linea.chars().collect();
    let objetivo: Vec<char> = palabra.chars().collect();
    let limite = |c: Option<&char>| c.is_none_or(|c| !c.is_alphanumeric() && *c != '_');
    let mut i = 0;
    while i + objetivo.len() <= caracteres.len() {
        if caracteres[i..i + objetivo.len()] == objetivo[..]
            && limite(if i == 0 { None } else { caracteres.get(i - 1) })
            && limite(caracteres.get(i + objetivo.len()))
        {
            out.push((i, i + objetivo.len()));
            i += objetivo.len();
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codigos(markers: &[Value]) -> Vec<&str> {
        markers.iter().map(|m| m["code"].as_str().unwrap()).collect()
    }

    fn lint(nombre: &str, contenido: &str) -> Vec<Value> {
        lint_rapido(nombre, contenido)["markers"].as_array().unwrap().clone()
    }

    // ── lint rápido ───────────────────────────────────────────────────

    #[test]
    fn el_lint_de_python_marca_las_cinco_reglas() {
        let src = "def f(x):\n    global g\n    assert x\n    try:\n        print(x)\n    except:\n        pass\n    # TODO: arreglar\n";
        let markers = lint("a.py", src);
        let c = codigos(&markers);
        for esperado in ["W003", "W001", "C002", "W002", "W100"] {
            assert!(c.contains(&esperado), "falta {esperado} en {c:?}");
        }
    }

    #[test]
    fn un_error_de_sintaxis_es_el_unico_marker() {
        let m = lint("a.py", "x = 1\nassert 1\ndef roto(:\n    pass\n");
        assert_eq!(codigos(&m), vec!["E001"]);
        assert_eq!(m[0]["severity"], ERROR);
        assert_eq!(m[0]["startLineNumber"], 3);
    }

    /// Las severidades son las de Monaco, numéricas.
    #[test]
    fn las_severidades_son_las_de_monaco() {
        let m = lint("a.py", "def f(x):\n    assert x\n    print(x)\n    # TODO: y\n");
        let por_codigo: std::collections::BTreeMap<&str, u64> =
            m.iter().map(|x| (x["code"].as_str().unwrap(), x["severity"].as_u64().unwrap())).collect();
        assert_eq!(por_codigo["W001"], u64::from(AVISO));
        assert_eq!(por_codigo["C002"], u64::from(INFO));
        assert_eq!(por_codigo["W100"], u64::from(PISTA));
    }

    #[test]
    fn el_rango_del_marker_tiene_columnas_validas() {
        for m in lint("a.py", "def f(x):\n    assert x\n") {
            let ini = m["startColumn"].as_u64().unwrap();
            let fin = m["endColumn"].as_u64().unwrap();
            assert!(ini >= 1, "las columnas de Monaco arrancan en 1: {m}");
            assert!(fin > ini, "el rango tiene que avanzar: {m}");
        }
    }

    /// **`TODO` se detecta sin distinguir mayúsculas**, pero se reporta como
    /// lo escribió el autor.
    #[test]
    fn la_marca_de_pendiente_conserva_como_se_escribio() {
        assert_eq!(marca_de_pendiente("# todo: algo"), Some((2, "todo".to_string())));
        assert_eq!(marca_de_pendiente("# FixMe ya"), Some((2, "FixMe".to_string())));
        assert_eq!(marca_de_pendiente("# nada que ver"), None);
    }

    /// La primera marca gana si hay varias en la línea.
    #[test]
    fn la_primera_marca_de_pendiente_gana() {
        assert_eq!(marca_de_pendiente("# HACK y TODO"), Some((2, "HACK".to_string())));
    }

    #[test]
    fn el_largo_de_linea_cuenta_caracteres() {
        let acentos = format!("x = '{}'\n", "á".repeat(100));
        assert!(!codigos(&lint("a.py", &acentos)).contains(&"E501"), "100 acentos son 100 chars");
        let largo = format!("x = '{}'\n", "a".repeat(130));
        assert!(codigos(&lint("a.py", &largo)).contains(&"E501"));
    }

    // ── lint de JS/TS ─────────────────────────────────────────────────

    #[test]
    fn el_lint_de_ts_marca_sus_reglas() {
        let src = "console.log('x')\nlet a: any = 1\ndebugger\nvar b = 2\nif (a == b) {}\n";
        let markers = lint("a.ts", src);
        let c = codigos(&markers);
        for esperado in ["TS001", "TS002", "TS003", "TS004", "TS005"] {
            assert!(c.contains(&esperado), "falta {esperado} en {c:?}");
        }
    }

    /// **`: any` no matchea `anything`.** El patrón del router tenía el
    /// límite de palabra y se conserva.
    #[test]
    fn any_no_matchea_una_palabra_mas_larga() {
        assert!(buscar_any("let x: any = 1").is_some());
        assert!(buscar_any("let x: anything = 1").is_none(), "anything no es any");
        assert!(buscar_any("let x: any_t = 1").is_none());
        assert!(buscar_any("let x: number = 1").is_none());
    }

    /// `===` y `!==` no son `==`.
    #[test]
    fn la_igualdad_estricta_no_se_marca() {
        assert!(igualdad_laxa("a == b").is_some());
        assert!(igualdad_laxa("a === b").is_none());
        assert!(igualdad_laxa("a !== b").is_none());
        assert!(igualdad_laxa("a <= b").is_none());
        assert!(igualdad_laxa("a >= b").is_none());
        assert!(igualdad_laxa("a != b").is_none());
    }

    #[test]
    fn debugger_como_palabra_suelta() {
        assert!(palabra_suelta("  debugger;", "debugger"));
        assert!(!palabra_suelta("mydebuggerthing", "debugger"));
        assert!(!palabra_suelta("debuggerX", "debugger"));
    }

    #[test]
    fn js_y_ts_comparten_las_reglas() {
        let src = "console.log('x')\nvar a = 1\n";
        assert_eq!(codigos(&lint("a.js", src)), codigos(&lint("a.ts", src)));
    }

    #[test]
    fn una_extension_sin_lint_no_devuelve_markers() {
        let r = lint_rapido("a.rb", "puts 1\n");
        assert_eq!(r["markers"].as_array().unwrap().len(), 0);
        assert_eq!(r["source"], "fast");
    }

    // ── análisis completo ─────────────────────────────────────────────

    #[test]
    fn el_analisis_completo_trae_metricas_y_big_o() {
        let src = "import os\n\ndef f(xs):\n    for x in xs:\n        for y in x:\n            print(y)\n";
        let r = analisis_completo("a.py", src);
        assert_eq!(r["source"], "heavy");
        assert!(r["metrics"]["maintainability"].is_number());
        let bo = r["big_o"].as_array().unwrap();
        assert_eq!(bo.len(), 1);
        assert_eq!(bo[0]["name"], "f");
        assert_eq!(bo[0]["big_o"], "O(n²)");
        assert!(!bo[0]["reason"].as_str().unwrap().is_empty(), "la razon tiene que venir");
    }

    /// Los imports sin usar llegan como markers, en su línea.
    #[test]
    fn un_import_sin_usar_es_un_marker() {
        let r = analisis_completo("a.py", "import os\nimport sys\n\nprint(sys.path)\n");
        let m = r["markers"].as_array().unwrap();
        assert!(!m.is_empty(), "os no se usa: {m:?}");
        assert_eq!(m[0]["code"], "W200");
        assert_eq!(m[0]["startLineNumber"], 1);
    }

    #[test]
    fn el_analisis_completo_de_otro_lenguaje_devuelve_el_shape_vacio() {
        let r = analisis_completo("a.rb", "puts 1\n");
        assert_eq!(r["markers"].as_array().unwrap().len(), 0);
        assert_eq!(r["big_o"].as_array().unwrap().len(), 0);
    }

    // ── hover ─────────────────────────────────────────────────────────

    fn hover_en(contenido: &str, linea: usize) -> Value {
        hover(&PosicionRequest {
            filename: "a.py".into(),
            content: contenido.into(),
            line: linea,
            column: 1,
            symbol_name: String::new(),
        })
    }

    #[test]
    fn el_hover_trae_la_firma_y_la_tabla() {
        let src = "def calcular(a, b):\n    \"\"\"Suma dos cosas.\"\"\"\n    return a + b\n";
        let md = hover_en(src, 2)["markdown"].as_str().unwrap().to_string();
        assert!(md.contains("```python\ndef calcular(a, b)\n```"), "{md}");
        assert!(md.contains("Suma dos cosas."));
        assert!(md.contains("Time Complexity"));
        assert!(md.contains("Cyclomatic CC"));
        assert!(md.contains("| **LOC** |"));
    }

    /// **Una función anidada muestra la externa, y es una limitación
    /// heredada del motor, no de este módulo.**
    ///
    /// `rich::collect_functions` desciende a las clases y a los cuerpos
    /// anidados que no son funciones (`if`/`for`/`try`), pero **no entra al
    /// cuerpo de un `def`**: un `def` dentro de otro no aparece en
    /// `rich.functions`. El hover del router usaba su propio `ast.walk`, que
    /// sí las veía, así que acá hay una regresión real, acotada a closures y
    /// funciones escritas adentro de otra.
    ///
    /// No se arregla desde acá: cambiar ese recorrido afecta los conteos, los
    /// smells y los hallazgos de seguridad de **todo** el motor, y eso merece
    /// su propia pasada medida. El test fija el comportamiento **actual**
    /// para que el día que se arregle, falle y alguien lea esto.
    #[test]
    fn una_funcion_anidada_todavia_muestra_la_externa() {
        let src = "def afuera():\n    \"\"\"d\"\"\"\n    def adentro():\n        \"\"\"d\"\"\"\n        return 1\n    return adentro\n";
        let md = hover_en(src, 4)["markdown"].as_str().unwrap().to_string();
        assert!(md.contains("def afuera()"), "limitacion conocida de `rich`: {md}");
        assert!(
            !md.contains("def adentro()"),
            "si esto cambia, `collect_functions` aprendio a bajar a los cuerpos"
        );
    }

    /// Si el cursor no está sobre ninguna función, se busca por nombre.
    #[test]
    fn el_hover_cae_al_nombre_del_simbolo() {
        let src = "def objetivo():\n    \"\"\"d\"\"\"\n    return 1\n\nx = 1\n";
        let r = hover(&PosicionRequest {
            filename: "a.py".into(),
            content: src.into(),
            line: 5,
            column: 1,
            symbol_name: "objetivo".into(),
        });
        assert!(r["markdown"].as_str().unwrap().contains("def objetivo()"));
    }

    #[test]
    fn el_hover_marca_async_y_el_camino_caliente() {
        let src = "async def pesada(m):\n    \"\"\"d\"\"\"\n    for a in m:\n        for b in a:\n            print(b)\n";
        let md = hover_en(src, 3)["markdown"].as_str().unwrap().to_string();
        assert!(md.contains("async def pesada"), "{md}");
        assert!(md.contains("Función **async**"));
        assert!(md.contains("Hot Path"), "O(n²) es camino caliente: {md}");
    }

    #[test]
    fn sin_funcion_el_hover_es_vacio() {
        let r = hover_en("x = 1\ny = 2\n", 1);
        assert_eq!(r["markdown"], "");
        assert!(r["range"].is_null());
    }

    #[test]
    fn un_archivo_roto_no_da_hover() {
        assert_eq!(hover_en("def roto(:\n    pass\n", 1)["markdown"], "");
    }

    #[test]
    fn el_rango_del_hover_cubre_la_funcion() {
        let src = "def f():\n    \"\"\"d\"\"\"\n    return 1\n";
        let r = hover_en(src, 2);
        assert_eq!(r["range"]["startLineNumber"], 1);
        assert_eq!(r["range"]["endLineNumber"], 3);
    }

    // ── definición y referencias ──────────────────────────────────────

    fn simbolo_req(nombre: &str, contenido: &str, s: &str) -> SimboloRequest {
        SimboloRequest {
            filename: nombre.into(),
            content: contenido.into(),
            symbol_name: s.into(),
        }
    }

    #[test]
    fn encuentra_la_definicion_en_python() {
        let src = "def objetivo():\n    return 1\n\nobjetivo()\n";
        let r = definicion(&simbolo_req("a.py", src, "objetivo"));
        assert_eq!(r["found"], true);
        assert_eq!(r["definitions"][0]["line"], 1);
    }

    #[test]
    fn un_simbolo_vacio_no_busca_nada() {
        let r = definicion(&simbolo_req("a.py", "def f(): pass", "   "));
        assert_eq!(r["found"], false);
        assert_eq!(r["symbol"], "");
    }

    #[test]
    fn encuentra_las_referencias_con_su_linea_de_definicion() {
        let src = "def objetivo():\n    return 1\n\nobjetivo()\nobjetivo()\n";
        let r = referencias(&simbolo_req("a.py", src, "objetivo"));
        assert!(r["total"].as_u64().unwrap() >= 2, "{r}");
        assert_eq!(r["definition_line"], 1);
    }

    /// Para un lenguaje sin parser de símbolos, la búsqueda por texto: no
    /// pretende entender el lenguaje, encuentra el identificador.
    #[test]
    fn en_otro_lenguaje_busca_por_texto() {
        let r = referencias(&simbolo_req("a.rb", "def foo\n  foo\nend\n", "foo"));
        assert!(r["total"].as_u64().unwrap() >= 1, "{r}");
        assert!(r["definition_line"].is_null());
    }

    // ── autocompletado ────────────────────────────────────────────────

    fn completar(nombre: &str, contenido: &str, prefijo: &str) -> Value {
        completado(&CompletadoRequest {
            filename: nombre.into(),
            content: contenido.into(),
            prefix: prefijo.into(),
        })
    }

    #[test]
    fn el_completado_trae_funciones_clases_metodos_e_imports() {
        let src = "import os\n\nclass Perro:\n    def ladrar(self, veces):\n        return veces\n\ndef libre():\n    return 1\n";
        let r = completar("a.py", src, "");
        let etiquetas: Vec<&str> =
            r["symbols"].as_array().unwrap().iter().map(|s| s["label"].as_str().unwrap()).collect();
        for esperado in ["libre", "ladrar", "Perro", "os"] {
            assert!(etiquetas.contains(&esperado), "falta {esperado} en {etiquetas:?}");
        }
    }

    /// **El orden es parte del contrato**: funciones y métodos primero,
    /// después clases, variables, imports.
    #[test]
    fn el_completado_ordena_por_tipo_y_despues_alfabetico() {
        let src = "import zlib\n\nclass Alfa:\n    def zeta(self):\n        return 1\n\ndef beta():\n    return 2\n";
        let r = completar("a.py", src, "");
        let tipos: Vec<&str> =
            r["symbols"].as_array().unwrap().iter().map(|s| s["kind"].as_str().unwrap()).collect();
        let peso_de = |k: &str| match k {
            "function" | "method" => 0,
            "class" => 1,
            "variable" => 2,
            "import" => 3,
            _ => 4,
        };
        let pesos: Vec<i32> = tipos.iter().map(|t| peso_de(t)).collect();
        let mut ordenados = pesos.clone();
        ordenados.sort_unstable();
        assert_eq!(pesos, ordenados, "{tipos:?}");
    }

    #[test]
    fn el_prefijo_filtra_sin_distinguir_mayusculas() {
        let src = "def calcular():\n    return 1\n\ndef Cargar():\n    return 2\n\ndef otra():\n    return 3\n";
        let r = completar("a.py", src, "ca");
        let e: Vec<&str> =
            r["symbols"].as_array().unwrap().iter().map(|s| s["label"].as_str().unwrap()).collect();
        assert_eq!(e.len(), 2, "calcular y Cargar: {e:?}");
        assert!(!e.contains(&"otra"));
    }

    #[test]
    fn en_ts_trae_interfaces_y_tipos() {
        let src = "import { x } from './x'\ninterface Datos { a: number }\ntype Alias = string\nexport function f() { return 1 }\n";
        let r = completar("a.ts", src, "");
        let tipos: Vec<&str> =
            r["symbols"].as_array().unwrap().iter().map(|s| s["kind"].as_str().unwrap()).collect();
        assert!(tipos.contains(&"interface"), "{tipos:?}");
        assert!(tipos.contains(&"type"), "{tipos:?}");
        assert!(tipos.contains(&"function"), "{tipos:?}");
    }

    #[test]
    fn un_archivo_roto_no_completa() {
        assert_eq!(completar("a.py", "def roto(:\n", "")["total"], 0);
    }

    // ── renombrar ─────────────────────────────────────────────────────

    fn renombrar_en(contenido: &str, viejo: &str, nuevo: &str) -> Value {
        renombrar(&RenombrarRequest {
            filename: "a.py".into(),
            content: contenido.into(),
            symbol_name: viejo.into(),
            new_name: nuevo.into(),
        })
    }

    #[test]
    fn renombrar_produce_un_edit_por_ocurrencia() {
        let src = "def viejo():\n    return 1\n\nviejo()\nx = viejo\n";
        let r = renombrar_en(src, "viejo", "nuevo");
        assert_eq!(r["valid"], true);
        let edits = r["edits"].as_array().unwrap();
        assert!(edits.len() >= 3, "definicion y dos usos: {}", edits.len());
        assert!(edits.iter().all(|e| e["newText"] == "nuevo"));
    }

    /// **Una línea con el símbolo varias veces produce varios edits.** Por
    /// eso se busca en la línea en vez de usar la columna de la referencia.
    #[test]
    fn varias_ocurrencias_en_la_misma_linea_dan_varios_edits() {
        let src = "def f(contador):\n    contador = contador + contador\n    return contador\n";
        let r = renombrar_en(src, "contador", "total");
        let en_la_dos = r["edits"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["range"]["startLineNumber"] == 2)
            .count();
        assert_eq!(en_la_dos, 3, "tres ocurrencias en la linea 2");
    }

    /// Y una subcadena NO se renombra: `contador` no toca `contadores`.
    #[test]
    fn una_subcadena_no_se_renombra() {
        assert_eq!(ocurrencias_de_palabra("contadores = contador", "contador"), vec![(13, 21)]);
        assert_eq!(ocurrencias_de_palabra("x = x_y + x", "x"), vec![(0, 1), (10, 11)]);
    }

    #[test]
    fn un_nombre_nuevo_invalido_se_rechaza() {
        for malo in ["123abc", "con-guion", "con espacio", "", "class-x"] {
            let r = renombrar_en("def viejo(): pass", "viejo", malo);
            assert_eq!(r["valid"], false, "{malo} tendria que rechazarse");
        }
        assert!(es_identificador("valido_1"));
        assert!(es_identificador("_privado"));
        assert!(!es_identificador("1malo"));
    }

    #[test]
    fn renombrar_al_mismo_nombre_se_rechaza() {
        let r = renombrar_en("def f(): pass", "f", "f");
        assert_eq!(r["valid"], false);
        assert_eq!(r["error"], "El nombre es igual");
    }

    #[test]
    fn un_simbolo_que_no_existe_se_reporta() {
        let r = renombrar_en("def f():\n    return 1\n", "no_existe", "otro");
        assert_eq!(r["valid"], false);
        assert!(r["error"].as_str().unwrap().contains("no encontrado"));
    }

    #[test]
    fn los_edits_vienen_ordenados_por_linea_y_columna() {
        let src = "def x():\n    return 1\n\ny = x\nz = x + x\n";
        let r = renombrar_en(src, "x", "w");
        let pares: Vec<(u64, u64)> = r["edits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["range"]["startLineNumber"].as_u64().unwrap(),
                    e["range"]["startColumn"].as_u64().unwrap(),
                )
            })
            .collect();
        let mut ordenados = pares.clone();
        ordenados.sort_unstable();
        assert_eq!(pares, ordenados, "{pares:?}");
    }

    #[test]
    fn el_rango_del_edit_cubre_el_largo_del_nombre_viejo() {
        let r = renombrar_en("def largo():\n    return largo\n", "largo", "x");
        for e in r["edits"].as_array().unwrap() {
            let ini = e["range"]["startColumn"].as_u64().unwrap();
            let fin = e["range"]["endColumn"].as_u64().unwrap();
            assert_eq!(fin - ini, 5, "`largo` son 5 caracteres: {e}");
        }
    }
}
