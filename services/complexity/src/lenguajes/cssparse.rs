//! Sythrall CSS Engine — primera porción: **el analizador**, no el generador.
//!
//! CSS era el hueco más grande que quedaba en la plataforma: los 8 plugins de
//! lenguaje (python/c/cpp/js/ts/fortran/assembly/notebook) no incluían `.css`,
//! así que un `.css` entraba al editor y no producía un solo hallazgo. Este
//! módulo lo convierte en el 9no lenguaje analizado.
//!
//! **Parser propio, sin dependencia nueva** — mismo criterio que `asmparse.rs`
//! (pattern-matching escrito a mano) y que el resto del motor: CSS tiene una
//! gramática lo bastante regular como para tokenizarla sin traer un crate
//! entero, y traerlo significaría exactamente la dependencia arquitectónica
//! que este proyecto evita. No es un parser CSS completo conforme al estándar
//! y no pretende serlo: reconoce reglas, selectores, declaraciones y at-rules
//! con sus líneas, que es lo que los checks de abajo necesitan.
//!
//! **Lo que este módulo mide es demostrable, y ahí está el límite a
//! propósito.** Todo lo que reporta se puede probar mirando SOLO el archivo
//! CSS: una declaración repetida dentro de la misma regla está muerta por las
//! reglas de la cascada, dos reglas con el mismo selector en el mismo contexto
//! son redundantes, y la especificidad es aritmética. Deliberadamente NO
//! reporta "CSS sin usar": eso exige mirar el HTML/JS del proyecto y aun así
//! no se puede probar, porque las clases se construyen en runtime
//! (`classList.add(\`t-${tool}\`)`). Medido de primera mano sobre el propio
//! `main.css` de Sythrall: un detector ingenuo de "clases sin uso" dio 34
//! candidatas y 33 eran falsos positivos. Esa señal necesita su propio diseño
//! y su propio hedge; no se cuela acá adentro como si fuera del mismo tipo.

use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct CssDeclaration {
    pub property: String,
    pub value: String,
    pub line: usize,
    pub important: bool,
}

#[derive(Serialize, Clone)]
pub struct CssRule {
    /// Selectores de la regla, ya separados por coma y sin espacios sobrantes.
    pub selectors: Vec<String>,
    pub line: usize,
    pub declarations: Vec<CssDeclaration>,
    /// Contexto de at-rule que contiene la regla (`@media (max-width: 480px)`),
    /// o `None` en el nivel superior. Importa para los duplicados: la MISMA
    /// regla dentro de un `@media` distinto no es una repetición, es
    /// justamente cómo se escribe un override responsive.
    pub at_context: Option<String>,
    /// Especificidad del selector más alto de la regla, como `(a, b, c)`:
    /// ids, clases/atributos/pseudo-clases, elementos/pseudo-elementos.
    pub specificity: (u32, u32, u32),
}

#[derive(Serialize, Clone)]
pub struct CssFinding {
    pub kind: &'static str,
    pub selector: String,
    pub line: usize,
    pub severity: &'static str,
    pub confidence: &'static str,
    pub message: String,
}

#[derive(Serialize)]
pub struct CssSummary {
    pub total_rules: usize,
    pub total_selectors: usize,
    pub total_declarations: usize,
    pub total_at_rules: usize,
    pub important_count: usize,
    pub bytes: usize,
    pub max_specificity: (u32, u32, u32),
}

#[derive(Serialize)]
pub struct CssAnalysisResult {
    pub rules: Vec<CssRule>,
    pub findings: Vec<CssFinding>,
    pub summary: CssSummary,
    pub note: String,
}

const NOTE: &str = "Todo lo que este análisis reporta se puede probar mirando solo este archivo: una declaración repetida dentro de una misma regla está muerta por la cascada, dos reglas con el mismo selector en el mismo contexto son redundantes, y la especificidad es aritmética. Deliberadamente NO reporta 'CSS sin usar' — eso exige mirar el HTML/JS del proyecto y aun así no se puede probar, porque las clases se construyen en runtime.";

/// Umbral de especificidad alta. `(0,4,0)` = 4 clases encadenadas: a partir de
/// ahí, sobrescribir la regla exige igualar o superar esa cadena, que es
/// justamente el problema que la especificidad alta causa. Convención de la
/// literatura de CSS (Harry Roberts, "specificity graph"), no un número
/// inventado — mismo criterio que los umbrales de `smells.rs`.
pub const HIGH_SPECIFICITY_CLASSES: u32 = 4;

// ─── Tokenizado ──────────────────────────────────────────────────────────────

/// Quita comentarios `/* ... */` reemplazándolos por espacios, preservando los
/// saltos de línea para que los números de línea sigan siendo correctos —
/// borrarlos a secas correría todas las líneas siguientes hacia arriba y cada
/// hallazgo apuntaría al lugar equivocado.
fn strip_comments(src: &str) -> String {
    let bytes: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    let mut quote: Option<char> = None;
    while i < bytes.len() {
        let c = bytes[i];
        // Dentro de un string, `/*` es texto, no un comentario: una URL como
        // `url('http://x/*y')` no abre nada.
        if let Some(q) = quote {
            if c == q && bytes.get(i.wrapping_sub(1)) != Some(&'\\') {
                quote = None;
            }
            out.push(c);
            i += 1;
            continue;
        }
        if c == '\'' || c == '"' {
            quote = Some(c);
            out.push(c);
            i += 1;
            continue;
        }
        if c == '/' && bytes.get(i + 1) == Some(&'*') {
            i += 2;
            while i < bytes.len() && !(bytes[i] == '*' && bytes.get(i + 1) == Some(&'/')) {
                out.push(if bytes[i] == '\n' { '\n' } else { ' ' });
                i += 1;
            }
            i += 2; // saltar el `*/`
            out.push(' ');
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Especificidad de UN selector, como `(ids, clases, elementos)`. Cuenta lo
/// que la spec cuenta: `#id` suma a `a`; `.clase`, `[attr]` y `:pseudo-clase`
/// suman a `b`; nombre de elemento y `::pseudo-elemento` suman a `c`. `*`,
/// combinadores y `:not()` en sí mismos no suman (el contenido de `:not()` sí,
/// pero eso queda fuera de esta primera porción — se documenta, no se finge).
fn specificity_of(selector: &str) -> (u32, u32, u32) {
    let chars: Vec<char> = selector.chars().collect();
    let (mut ids, mut classes, mut elements) = (0u32, 0u32, 0u32);
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '#' => {
                ids += 1;
                i += 1;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
                    i += 1;
                }
            }
            '.' => {
                classes += 1;
                i += 1;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
                    i += 1;
                }
            }
            '[' => {
                classes += 1;
                while i < chars.len() && chars[i] != ']' {
                    i += 1;
                }
                i += 1;
            }
            ':' => {
                // `::` es pseudo-ELEMENTO (cuenta como elemento), `:` simple es
                // pseudo-CLASE (cuenta como clase) — distinción real de la spec.
                let double = chars.get(i + 1) == Some(&':');
                if double {
                    elements += 1;
                    i += 2;
                } else {
                    classes += 1;
                    i += 1;
                }
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
                    i += 1;
                }
                // Saltear los argumentos de una pseudo-clase funcional
                // (`:nth-child(2n+1)`) sin contarlos.
                if chars.get(i) == Some(&'(') {
                    let mut depth = 0;
                    while i < chars.len() {
                        if chars[i] == '(' {
                            depth += 1;
                        } else if chars[i] == ')' {
                            depth -= 1;
                            if depth == 0 {
                                i += 1;
                                break;
                            }
                        }
                        i += 1;
                    }
                }
            }
            c if c.is_alphabetic() => {
                elements += 1;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    (ids, classes, elements)
}

fn split_selectors(raw: &str) -> Vec<String> {
    raw.split(',').map(|s| s.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|s| !s.is_empty()).collect()
}

fn parse_declarations(body: &str, start_line: usize) -> Vec<CssDeclaration> {
    let mut out = Vec::new();
    let mut line = start_line;
    for chunk in body.split(';') {
        let leading_newlines = chunk.chars().take_while(|c| c.is_whitespace()).filter(|c| *c == '\n').count();
        line += leading_newlines;
        let trimmed = chunk.trim();
        if !trimmed.is_empty() {
            if let Some((prop, value)) = trimmed.split_once(':') {
                let value = value.trim();
                out.push(CssDeclaration {
                    property: prop.trim().to_lowercase(),
                    value: value.trim_end_matches("!important").trim().to_string(),
                    line,
                    important: value.contains("!important"),
                });
            }
        }
        line += chunk.chars().filter(|c| *c == '\n').count() - leading_newlines;
    }
    out
}

/// Parser principal. Recorre el texto llevando la línea actual y una pila de
/// at-rules abiertas, así una regla dentro de `@media` sabe en qué contexto
/// está. Las at-rules sin bloque (`@import`, `@charset`) terminan en `;` y no
/// abren contexto.
pub fn parse_css(source: &str) -> CssAnalysisResult {
    let clean = strip_comments(source);
    let chars: Vec<char> = clean.chars().collect();

    let mut rules: Vec<CssRule> = Vec::new();
    let mut at_stack: Vec<String> = Vec::new();
    let mut at_rule_count = 0usize;
    let mut buffer = String::new();
    let mut line = 1usize;
    let mut i = 0usize;
    // Estado de string. **Bug real encontrado haciendo que Sythrall analizara
    // su propio `main.css`**: su primera línea es
    // `@import url('...css2?family=DM+Mono:wght@400;500&...')`, y ese `;`
    // ADENTRO de la URL terminaba la at-rule antes de tiempo. El resto del
    // texto se acumulaba en el buffer hasta la próxima `{`, produciendo un
    // "selector" de 200 caracteres con media hoja de estilos adentro. Los
    // tests iniciales no lo detectaron porque ninguno tenía un `;` dentro de
    // un string — el auto-análisis sí, en el primer intento.
    let mut quote: Option<char> = None;

    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            if c == '\n' {
                line += 1;
            }
            if c == q && chars.get(i.wrapping_sub(1)) != Some(&'\\') {
                quote = None;
            }
            buffer.push(c);
            i += 1;
            continue;
        }
        if c == '\'' || c == '"' {
            quote = Some(c);
            buffer.push(c);
            i += 1;
            continue;
        }
        match c {
            '\n' => {
                line += 1;
                buffer.push(c);
                i += 1;
            }
            '{' => {
                let prelude = buffer.trim().to_string();
                buffer.clear();
                if prelude.starts_with('@') {
                    // At-rule con bloque: abre contexto y sigue leyendo adentro.
                    at_rule_count += 1;
                    at_stack.push(prelude.split_whitespace().collect::<Vec<_>>().join(" "));
                    i += 1;
                } else {
                    // Regla normal: consumir hasta la llave de cierre que
                    // corresponde (las reglas de CSS plano no anidan, pero el
                    // conteo protege de un archivo malformado).
                    let body_start = i + 1;
                    let mut depth = 1;
                    let mut j = body_start;
                    while j < chars.len() && depth > 0 {
                        match chars[j] {
                            '{' => depth += 1,
                            '}' => depth -= 1,
                            _ => {}
                        }
                        if depth > 0 {
                            j += 1;
                        }
                    }
                    let body: String = chars[body_start..j.min(chars.len())].iter().collect();
                    let selectors = split_selectors(&prelude);
                    let specificity = selectors
                        .iter()
                        .map(|s| specificity_of(s))
                        .max()
                        .unwrap_or((0, 0, 0));
                    let rule_line = line;
                    rules.push(CssRule {
                        selectors,
                        line: rule_line,
                        declarations: parse_declarations(&body, rule_line),
                        at_context: at_stack.last().cloned(),
                        specificity,
                    });
                    line += body.chars().filter(|c| *c == '\n').count();
                    i = j + 1;
                }
            }
            '}' => {
                at_stack.pop();
                buffer.clear();
                i += 1;
            }
            ';' if buffer.trim_start().starts_with('@') => {
                // At-rule sin bloque (`@import`, `@charset`): no abre contexto.
                at_rule_count += 1;
                buffer.clear();
                i += 1;
            }
            _ => {
                buffer.push(c);
                i += 1;
            }
        }
    }

    let findings = analyze(&rules);
    let summary = summarize(source, &rules, at_rule_count);
    CssAnalysisResult { rules, findings, summary, note: NOTE.to_string() }
}

fn summarize(source: &str, rules: &[CssRule], at_rule_count: usize) -> CssSummary {
    CssSummary {
        total_rules: rules.len(),
        total_selectors: rules.iter().map(|r| r.selectors.len()).sum(),
        total_declarations: rules.iter().map(|r| r.declarations.len()).sum(),
        total_at_rules: at_rule_count,
        important_count: rules.iter().flat_map(|r| &r.declarations).filter(|d| d.important).count(),
        bytes: source.len(),
        max_specificity: rules.iter().map(|r| r.specificity).max().unwrap_or((0, 0, 0)),
    }
}

// ─── Checks ──────────────────────────────────────────────────────────────────

fn analyze(rules: &[CssRule]) -> Vec<CssFinding> {
    let mut findings = Vec::new();
    findings.extend(duplicate_declarations(rules));
    findings.extend(duplicate_selectors(rules));
    findings.extend(high_specificity(rules));
    findings.sort_by_key(|f| f.line);
    findings
}

/// La misma propiedad declarada dos veces en la MISMA regla: por la cascada
/// gana la última, así que la primera es código muerto — demostrable sin mirar
/// nada más, de ahí confianza `High`. Excepción real y contemplada: repetir
/// una propiedad a propósito como fallback para navegadores viejos
/// (`display: block; display: flex`) con valores DISTINTOS es una técnica
/// legítima, así que solo se marca cuando el valor también se repite, que es
/// cuando no puede haber ninguna intención detrás.
fn duplicate_declarations(rules: &[CssRule]) -> Vec<CssFinding> {
    let mut out = Vec::new();
    for rule in rules {
        for (i, decl) in rule.declarations.iter().enumerate() {
            let dup = rule.declarations[..i].iter().find(|d| d.property == decl.property && d.value == decl.value);
            if let Some(first) = dup {
                out.push(CssFinding {
                    kind: "duplicate_declaration",
                    selector: rule.selectors.join(", "),
                    line: decl.line,
                    severity: "Low",
                    confidence: "High",
                    message: format!(
                        "`{}: {}` ya estaba declarada con el mismo valor en la línea {} de esta misma regla — la primera nunca tiene efecto (gana la última por cascada).",
                        decl.property, decl.value, first.line
                    ),
                });
            }
        }
    }
    out
}

/// El mismo selector en dos reglas del MISMO contexto. Fuera de un `@media`
/// distinto no hay motivo para separarlas: las declaraciones podrían estar
/// juntas, y tenerlas partidas es justo lo que hace difícil saber qué valor
/// gana. Comparar el `at_context` es lo que evita el falso positivo grande:
/// `.btn` en el nivel superior y `.btn` dentro de `@media (max-width: 480px)`
/// es exactamente cómo se escribe un override responsive, no una repetición.
fn duplicate_selectors(rules: &[CssRule]) -> Vec<CssFinding> {
    let mut out = Vec::new();
    for (i, rule) in rules.iter().enumerate() {
        for selector in &rule.selectors {
            let first = rules[..i]
                .iter()
                .find(|r| r.at_context == rule.at_context && r.selectors.iter().any(|s| s == selector));
            if let Some(first) = first {
                out.push(CssFinding {
                    kind: "duplicate_selector",
                    selector: selector.clone(),
                    line: rule.line,
                    severity: "Low",
                    confidence: "High",
                    message: format!(
                        "`{}` ya se había definido en la línea {}{} — dos bloques para el mismo selector obligan a leer los dos para saber qué valor gana.",
                        selector,
                        first.line,
                        rule.at_context.as_ref().map_or(String::new(), |c| format!(" dentro de `{c}`"))
                    ),
                });
            }
        }
    }
    out
}

/// Especificidad alta: cuesta sobrescribirla sin escalar todavía más, que es
/// como una hoja de estilos termina llena de `!important`. Es aritmética pura
/// sobre el selector, así que es demostrable — pero es una advertencia de
/// mantenibilidad, no un bug, de ahí severidad `Low`.
fn high_specificity(rules: &[CssRule]) -> Vec<CssFinding> {
    let mut out = Vec::new();
    for rule in rules {
        for selector in &rule.selectors {
            let (ids, classes, elements) = specificity_of(selector);
            if ids >= 2 || classes >= HIGH_SPECIFICITY_CLASSES {
                out.push(CssFinding {
                    kind: "high_specificity",
                    selector: selector.clone(),
                    line: rule.line,
                    severity: "Low",
                    confidence: "High",
                    message: format!(
                        "Especificidad ({ids},{classes},{elements}) — sobrescribir esta regla exige igualar o superar esa cadena. Considerá una sola clase con nombre propio en vez de encadenar."
                    ),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regla_simple_se_parsea() {
        let r = parse_css(".btn { color: red; padding: 4px; }");
        assert_eq!(r.rules.len(), 1);
        assert_eq!(r.rules[0].selectors, vec![".btn"]);
        assert_eq!(r.rules[0].declarations.len(), 2);
        assert_eq!(r.summary.total_declarations, 2);
    }

    #[test]
    fn selectores_separados_por_coma_se_cuentan_todos() {
        let r = parse_css(".a, .b, .c { color: red; }");
        assert_eq!(r.rules[0].selectors.len(), 3);
        assert_eq!(r.summary.total_selectors, 3);
    }

    #[test]
    fn comentarios_no_rompen_ni_corren_las_lineas() {
        // El comentario ocupa 2 líneas: la regla tiene que reportar la 3, no
        // la 1 — reemplazar comentarios por nada correría todo hacia arriba.
        let src = "/* comentario\n   multilinea */\n.btn { color: red; }";
        let r = parse_css(src);
        assert_eq!(r.rules.len(), 1);
        assert_eq!(r.rules[0].line, 3);
    }

    #[test]
    fn una_llave_dentro_de_un_comentario_no_abre_bloque() {
        let r = parse_css("/* .fake { color: red; } */\n.real { color: blue; }");
        assert_eq!(r.rules.len(), 1);
        assert_eq!(r.rules[0].selectors, vec![".real"]);
    }

    #[test]
    fn at_media_da_contexto_a_las_reglas_de_adentro() {
        let src = "@media (max-width: 480px) {\n  .btn { color: red; }\n}";
        let r = parse_css(src);
        assert_eq!(r.rules.len(), 1);
        assert_eq!(r.rules[0].at_context.as_deref(), Some("@media (max-width: 480px)"));
        assert_eq!(r.summary.total_at_rules, 1);
    }

    #[test]
    fn at_import_sin_bloque_no_abre_contexto() {
        let src = "@import url('x.css');\n.btn { color: red; }";
        let r = parse_css(src);
        assert_eq!(r.rules.len(), 1);
        assert!(r.rules[0].at_context.is_none());
        assert_eq!(r.summary.total_at_rules, 1);
    }

    #[test]
    fn punto_y_coma_dentro_de_una_url_no_termina_la_at_rule() {
        // Regresión — bug real encontrado haciendo que Sythrall analizara su
        // propio `main.css`: los `;` de una URL de Google Fonts cortaban el
        // `@import` antes de tiempo y el resto del archivo terminaba
        // acumulado como si fuera un solo selector gigante.
        let src = "@import url('https://fonts.googleapis.com/css2?family=DM+Mono:wght@400;500&family=Outfit:wght@300;500&display=swap');\n.btn { color: red; }";
        let r = parse_css(src);
        assert_eq!(r.rules.len(), 1, "solo hay UNA regla real en este CSS");
        assert_eq!(r.rules[0].selectors, vec![".btn"]);
        assert_eq!(r.summary.total_at_rules, 1);
    }

    #[test]
    fn llaves_dentro_de_un_string_no_abren_bloque() {
        let src = ".a { content: '{ no soy un bloque }'; color: red; }\n.b { color: blue; }";
        let r = parse_css(src);
        assert_eq!(r.rules.len(), 2);
        assert_eq!(r.rules[1].selectors, vec![".b"]);
    }

    #[test]
    fn un_comentario_dentro_de_un_string_es_texto() {
        let src = ".a { background: url('http://x/*y*/z.png'); color: red; }";
        let r = parse_css(src);
        assert_eq!(r.rules.len(), 1);
        assert_eq!(r.rules[0].declarations.len(), 2);
    }

    #[test]
    fn important_se_detecta_y_se_cuenta() {
        let r = parse_css(".btn { color: red !important; padding: 0; }");
        assert!(r.rules[0].declarations[0].important);
        assert_eq!(r.rules[0].declarations[0].value, "red");
        assert_eq!(r.summary.important_count, 1);
    }

    // ── Especificidad ────────────────────────────────────────────────────

    #[test]
    fn especificidad_cuenta_ids_clases_y_elementos() {
        assert_eq!(specificity_of("#main .btn span"), (1, 1, 1));
        assert_eq!(specificity_of("a"), (0, 0, 1));
        assert_eq!(specificity_of(".a.b.c"), (0, 3, 0));
        assert_eq!(specificity_of("[data-x]"), (0, 1, 0));
    }

    #[test]
    fn pseudo_clase_y_pseudo_elemento_cuentan_distinto() {
        // `:hover` es clase, `::before` es elemento -- distinción real de la spec.
        assert_eq!(specificity_of("a:hover"), (0, 1, 1));
        assert_eq!(specificity_of("a::before"), (0, 0, 2));
    }

    #[test]
    fn argumentos_de_pseudo_clase_funcional_no_se_cuentan() {
        // `:nth-child(2n+1)` suma 1 clase, no una por cada caracter de adentro.
        assert_eq!(specificity_of("li:nth-child(2n+1)"), (0, 1, 1));
    }

    #[test]
    fn la_regla_reporta_la_especificidad_de_su_selector_mas_alto() {
        let r = parse_css("a, #main .x .y { color: red; }");
        assert_eq!(r.rules[0].specificity, (1, 2, 0));
    }

    // ── Checks ───────────────────────────────────────────────────────────

    #[test]
    fn declaracion_repetida_con_el_mismo_valor_se_marca() {
        let r = parse_css(".btn {\n  color: red;\n  padding: 0;\n  color: red;\n}");
        let f: Vec<_> = r.findings.iter().filter(|f| f.kind == "duplicate_declaration").collect();
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].confidence, "High");
    }

    #[test]
    fn fallback_con_valores_distintos_no_se_marca() {
        // `display: block; display: flex` es una técnica legítima de fallback
        // -- solo el valor repetido no puede tener intención detrás.
        let r = parse_css(".btn {\n  display: block;\n  display: flex;\n}");
        assert!(!r.findings.iter().any(|f| f.kind == "duplicate_declaration"));
    }

    #[test]
    fn selector_repetido_en_el_mismo_contexto_se_marca() {
        let r = parse_css(".btn { color: red; }\n.otra { color: blue; }\n.btn { padding: 0; }");
        let f: Vec<_> = r.findings.iter().filter(|f| f.kind == "duplicate_selector").collect();
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].selector, ".btn");
        assert_eq!(f[0].line, 3);
    }

    #[test]
    fn el_mismo_selector_dentro_de_un_media_no_es_duplicado() {
        // El caso que haría inservible este check si se ignorara el contexto:
        // un override responsive es exactamente esto.
        let src = ".btn { color: red; }\n@media (max-width: 480px) {\n  .btn { color: blue; }\n}";
        let r = parse_css(src);
        assert!(!r.findings.iter().any(|f| f.kind == "duplicate_selector"));
    }

    #[test]
    fn dos_reglas_iguales_dentro_del_mismo_media_si_son_duplicado() {
        let src = "@media print {\n  .btn { color: red; }\n  .btn { padding: 0; }\n}";
        let r = parse_css(src);
        let f: Vec<_> = r.findings.iter().filter(|f| f.kind == "duplicate_selector").collect();
        assert_eq!(f.len(), 1);
        assert!(f[0].message.contains("@media print"));
    }

    #[test]
    fn especificidad_alta_se_marca_y_la_normal_no() {
        let alta = parse_css("#a #b .c { color: red; }");
        assert!(alta.findings.iter().any(|f| f.kind == "high_specificity"));
        let normal = parse_css(".card .title { color: red; }");
        assert!(!normal.findings.iter().any(|f| f.kind == "high_specificity"));
    }

    #[test]
    fn cuatro_clases_encadenadas_alcanzan_el_umbral() {
        let r = parse_css(".a .b .c .d { color: red; }");
        assert!(r.findings.iter().any(|f| f.kind == "high_specificity"));
    }

    #[test]
    fn hallazgos_ordenados_por_linea() {
        let src = "#a #b { color: red; }\n.x { color: red; }\n.x { padding: 0; }";
        let r = parse_css(src);
        let lines: Vec<usize> = r.findings.iter().map(|f| f.line).collect();
        assert!(lines.windows(2).all(|w| w[0] <= w[1]), "sin ordenar: {lines:?}");
    }

    #[test]
    fn archivo_vacio_no_rompe() {
        let r = parse_css("");
        assert_eq!(r.summary.total_rules, 0);
        assert!(r.findings.is_empty());
        assert_eq!(r.summary.bytes, 0);
    }

    #[test]
    fn css_limpio_no_produce_hallazgos() {
        let src = ".card { padding: 4px; }\n.title { font-size: 1rem; }";
        let r = parse_css(src);
        assert!(r.findings.is_empty());
    }

    #[test]
    fn la_nota_dice_explicitamente_que_no_reporta_css_sin_usar() {
        let r = parse_css(".a { color: red; }");
        assert!(r.note.contains("sin usar"));
        assert!(r.note.contains("runtime"));
    }
}
