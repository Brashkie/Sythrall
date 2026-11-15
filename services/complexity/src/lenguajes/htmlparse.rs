//! HTML como lenguaje analizado — 10mo plugin, orientado a accesibilidad y
//! estructura del documento.
//!
//! `cssusage.rs` ya leía HTML, pero solo para buscar nombres de clase: el
//! archivo entraba como texto y salía como un conjunto de tokens. Este módulo
//! lo analiza de verdad.
//!
//! **Por qué accesibilidad y no "validación de HTML".** Validar HTML contra la
//! especificación completa es trabajo de un validador conforme, y hay uno
//! bueno y gratis (el del W3C). Lo que un analizador de código puede aportar
//! que ese validador no da es la clase de defecto que **rompe el uso real de
//! la página y no aparece mirándola**: una imagen sin texto alternativo, un
//! `id` repetido, un campo sin etiqueta. Nada de eso es un error de sintaxis
//! — el HTML es perfectamente válido y la página se ve bien.
//!
//! **Todo lo que reporta se prueba leyendo el archivo**, igual que
//! `cssparse.rs`: un `<img>` sin atributo `alt` no lo tiene, un `id` repetido
//! está repetido, un salto de `<h1>` a `<h3>` está en el orden del documento.
//! Confianza `High` en todos, sin excepción — y donde no se puede probar, no
//! hay check.
//!
//! **La distinción que evita el falso positivo más común de esta familia**:
//! `alt=""` (vacío) es correcto y deliberado — es como se marca una imagen
//! decorativa para que un lector de pantalla la SALTEE. Marcarlo, como hacen
//! varios linters, entrena a la gente a ignorar la herramienta. Solo se
//! reporta la ausencia total del atributo.
//!
//! Tokenizador propio escrito a mano, sin dependencia nueva — mismo criterio
//! que `cssparse.rs` y `asmparse.rs`.

use std::collections::HashMap;

use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct HtmlFinding {
    pub kind: &'static str,
    /// Elemento involucrado (`img`, `input`, `h3`…), para ubicar el hallazgo.
    pub element: String,
    pub line: usize,
    pub severity: &'static str,
    pub confidence: &'static str,
    pub message: String,
}

#[derive(Serialize)]
pub struct HtmlSummary {
    pub total_elements: usize,
    pub total_ids: usize,
    pub images: usize,
    pub links: usize,
    pub form_controls: usize,
    pub headings: usize,
    pub bytes: usize,
}

#[derive(Serialize)]
pub struct HtmlAnalysisResult {
    pub findings: Vec<HtmlFinding>,
    pub summary: HtmlSummary,
    pub note: String,
}

const NOTE: &str = "Analisis de accesibilidad y estructura sobre lo que el archivo dice: cada hallazgo se puede verificar leyendo la linea que indica. NO es un validador de HTML conforme al estandar (para eso esta el del W3C) ni una auditoria de accesibilidad completa — no evalua contraste, foco visible, orden de lectura ni nada que dependa del CSS o del comportamiento en runtime.";

/// Un tag abierto encontrado por el tokenizador.
struct Tag {
    name: String,
    attrs: HashMap<String, String>,
    line: usize,
    /// `true` para `</div>`.
    closing: bool,
}

/// Tokenizador de HTML mínimo pero honesto con los casos que importan:
/// comentarios, regiones de texto crudo (`<script>`/`<style>`, donde un `<`
/// no abre un tag), atributos con comillas simples/dobles/sin comillas, y
/// tags de cierre. No construye un árbol — los checks de abajo trabajan sobre
/// la secuencia de tags, que alcanza para todos ellos.
fn tokenize(source: &str) -> Vec<Tag> {
    let chars: Vec<char> = source.chars().collect();
    let mut tags = Vec::new();
    let mut i = 0usize;
    let mut line = 1usize;

    while i < chars.len() {
        if chars[i] == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if chars[i] != '<' {
            i += 1;
            continue;
        }
        // Comentario: saltar hasta `-->` contando líneas.
        if chars[i..].starts_with(&['<', '!', '-', '-']) {
            i += 4;
            while i < chars.len() && !chars[i..].starts_with(&['-', '-', '>']) {
                if chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            i += 3;
            continue;
        }
        // `<!DOCTYPE ...>` y similares: no son elementos.
        if chars.get(i + 1) == Some(&'!') {
            while i < chars.len() && chars[i] != '>' {
                if chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            i += 1;
            continue;
        }

        let tag_line = line;
        i += 1;
        let closing = chars.get(i) == Some(&'/');
        if closing {
            i += 1;
        }
        let name_start = i;
        while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '-') {
            i += 1;
        }
        if i == name_start {
            continue; // un `<` suelto que no abre nada
        }
        let name: String = chars[name_start..i].iter().collect::<String>().to_lowercase();

        // Atributos hasta el `>` que cierra el tag.
        let mut attrs = HashMap::new();
        while i < chars.len() && chars[i] != '>' {
            if chars[i] == '\n' {
                line += 1;
            }
            if chars[i].is_whitespace() || chars[i] == '/' {
                i += 1;
                continue;
            }
            let a_start = i;
            while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' && chars[i] != '>' {
                i += 1;
            }
            let attr: String = chars[a_start..i].iter().collect::<String>().to_lowercase();
            let mut value = String::new();
            // Saltear espacios antes de un posible `=`.
            while i < chars.len() && chars[i].is_whitespace() {
                if chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            if chars.get(i) == Some(&'=') {
                i += 1;
                while i < chars.len() && chars[i].is_whitespace() {
                    i += 1;
                }
                match chars.get(i) {
                    Some(&q @ ('"' | '\'')) => {
                        i += 1;
                        let v_start = i;
                        while i < chars.len() && chars[i] != q {
                            if chars[i] == '\n' {
                                line += 1;
                            }
                            i += 1;
                        }
                        value = chars[v_start..i].iter().collect();
                        i += 1;
                    }
                    Some(_) => {
                        let v_start = i;
                        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '>' {
                            i += 1;
                        }
                        value = chars[v_start..i].iter().collect();
                    }
                    None => {}
                }
            }
            if !attr.is_empty() {
                attrs.insert(attr, value);
            }
        }
        i += 1; // consumir el `>`

        // `<script>`/`<style>`: su contenido es texto crudo, un `<` adentro
        // no abre un tag. Sin esto, `if (a < b)` dentro de un script se
        // interpretaría como el comienzo de un elemento.
        if !closing && (name == "script" || name == "style") {
            let close = format!("</{name}");
            let rest: String = chars[i..].iter().collect();
            if let Some(pos) = rest.to_lowercase().find(&close) {
                line += rest[..pos].matches('\n').count();
                i += rest[..pos].chars().count();
            }
        }
        tags.push(Tag { name, attrs, line: tag_line, closing });
    }
    tags
}

fn heading_level(name: &str) -> Option<u8> {
    match name {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
}

/// Tipos de `<input>` que no necesitan etiqueta: no reciben entrada del
/// usuario (`hidden`) o llevan su propio texto accesible (`submit`, `button`,
/// `reset`, `image`).
fn input_needs_label(attrs: &HashMap<String, String>) -> bool {
    let t = attrs.get("type").map(|s| s.to_lowercase()).unwrap_or_else(|| "text".to_string());
    !matches!(t.as_str(), "hidden" | "submit" | "button" | "reset" | "image")
}

fn has_accessible_name(attrs: &HashMap<String, String>) -> bool {
    ["aria-label", "aria-labelledby", "title"].iter().any(|k| attrs.get(*k).is_some_and(|v| !v.trim().is_empty()))
}

pub fn analyze_html(source: &str) -> HtmlAnalysisResult {
    let tags = tokenize(source);
    let mut findings = Vec::new();

    // ── `id` duplicados ──
    let mut seen_ids: HashMap<&str, usize> = HashMap::new();
    // ── `for` de las etiquetas, para saber qué inputs están etiquetados ──
    let labeled: Vec<&str> =
        tags.iter().filter(|t| t.name == "label" && !t.closing).filter_map(|t| t.attrs.get("for").map(|s| s.as_str())).collect();

    let (mut images, mut links, mut form_controls, mut headings) = (0usize, 0usize, 0usize, 0usize);
    let mut last_heading: Option<u8> = None;
    // Profundidad de `<label>`: un input envuelto en una etiqueta ya está
    // etiquetado aunque no tenga `id`. Sin esto, la forma `<label>Texto
    // <input></label>` — perfectamente correcta — daría falso positivo.
    let mut label_depth = 0i32;

    for tag in &tags {
        if tag.name == "label" {
            label_depth += if tag.closing { -1 } else { 1 };
            label_depth = label_depth.max(0);
        }
        if tag.closing {
            continue;
        }

        if let Some(id) = tag.attrs.get("id").filter(|v| !v.trim().is_empty()) {
            match seen_ids.get(id.as_str()) {
                Some(first) => findings.push(HtmlFinding {
                    kind: "duplicate_id",
                    element: tag.name.clone(),
                    line: tag.line,
                    severity: "Medium",
                    confidence: "High",
                    message: format!(
                        "`id=\"{id}\"` ya estaba usado en la linea {first}. Un id tiene que ser unico en el documento: `getElementById` y `querySelector` devuelven solo el primero, y un `<label for=\"{id}\">` apunta solo a ese."
                    ),
                }),
                None => {
                    seen_ids.insert(id.as_str(), tag.line);
                }
            }
        }

        match tag.name.as_str() {
            "html" => {
                if !tag.attrs.contains_key("lang") {
                    findings.push(HtmlFinding {
                        kind: "missing_lang",
                        element: "html".to_string(),
                        line: tag.line,
                        severity: "Medium",
                        confidence: "High",
                        message: "`<html>` sin atributo `lang`. Un lector de pantalla elige la pronunciacion por ese atributo: sin el, lee el contenido con la fonetica del idioma configurado en el sistema.".to_string(),
                    });
                }
            }
            "img" => {
                images += 1;
                // `alt=""` es CORRECTO y deliberado (imagen decorativa, el
                // lector la saltea) — solo la ausencia total del atributo es
                // un hallazgo.
                if !tag.attrs.contains_key("alt") {
                    findings.push(HtmlFinding {
                        kind: "img_without_alt",
                        element: "img".to_string(),
                        line: tag.line,
                        severity: "Medium",
                        confidence: "High",
                        message: "`<img>` sin atributo `alt`. Un lector de pantalla anuncia el nombre del archivo en su lugar. Si la imagen es decorativa, `alt=\"\"` es la forma correcta de decir \"salteala\" — y no se reporta.".to_string(),
                    });
                }
            }
            "a" => {
                links += 1;
                if !tag.attrs.contains_key("href") {
                    findings.push(HtmlFinding {
                        kind: "link_without_href",
                        element: "a".to_string(),
                        line: tag.line,
                        severity: "Low",
                        confidence: "High",
                        message: "`<a>` sin `href` no es alcanzable con el teclado ni se anuncia como enlace. Si es un control, `<button>` ya trae ese comportamiento; si es un ancla de destino, no necesita ser un `<a>`.".to_string(),
                    });
                }
            }
            "input" | "select" | "textarea" => {
                form_controls += 1;
                let needs = tag.name != "input" || input_needs_label(&tag.attrs);
                let has_for = tag.attrs.get("id").is_some_and(|id| labeled.contains(&id.as_str()));
                if needs && label_depth == 0 && !has_for && !has_accessible_name(&tag.attrs) {
                    findings.push(HtmlFinding {
                        kind: "control_without_label",
                        element: tag.name.clone(),
                        line: tag.line,
                        severity: "Medium",
                        confidence: "High",
                        message: format!(
                            "`<{}>` sin etiqueta asociada: no esta dentro de un `<label>`, ningun `<label for=...>` apunta a su id, y no tiene `aria-label`/`aria-labelledby`/`title`. Un lector de pantalla lo anuncia sin decir para que sirve.",
                            tag.name
                        ),
                    });
                }
            }
            name => {
                if let Some(level) = heading_level(name) {
                    headings += 1;
                    if let Some(prev) = last_heading {
                        if level > prev + 1 {
                            findings.push(HtmlFinding {
                                kind: "heading_level_skip",
                                element: name.to_string(),
                                line: tag.line,
                                severity: "Low",
                                confidence: "High",
                                message: format!(
                                    "`<{name}>` viene despues de un `<h{prev}>`: se saltea{}. La navegacion por encabezados de un lector de pantalla usa esos niveles como indice del documento, y un salto rompe la jerarquia.",
                                    (prev + 1..level).map(|l| format!(" `<h{l}>`")).collect::<String>()
                                ),
                            });
                        }
                    }
                    last_heading = Some(level);
                }
            }
        }

        // `tabindex` positivo: reordena la navegación por teclado de toda la
        // página, no solo de este elemento.
        if let Some(t) = tag.attrs.get("tabindex").and_then(|v| v.trim().parse::<i32>().ok()) {
            if t > 0 {
                findings.push(HtmlFinding {
                    kind: "positive_tabindex",
                    element: tag.name.clone(),
                    line: tag.line,
                    severity: "Low",
                    confidence: "High",
                    message: format!(
                        "`tabindex=\"{t}\"` saca a este elemento del orden natural y lo pone ANTES que todo lo que tenga tabindex 0 o ninguno — en toda la pagina, no solo aca. `tabindex=\"0\"` lo hace enfocable respetando el orden del documento."
                    ),
                });
            }
        }
    }

    findings.sort_by_key(|f| f.line);
    let summary = HtmlSummary {
        total_elements: tags.iter().filter(|t| !t.closing).count(),
        total_ids: seen_ids.len(),
        images,
        links,
        form_controls,
        headings,
        bytes: source.len(),
    };
    HtmlAnalysisResult { findings, summary, note: NOTE.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<&'static str> {
        analyze_html(src).findings.iter().map(|f| f.kind).collect()
    }

    // ── Tokenizador ──────────────────────────────────────────────────────

    #[test]
    fn cuenta_elementos_y_no_los_cierres() {
        let r = analyze_html("<div><span></span></div>");
        assert_eq!(r.summary.total_elements, 2);
    }

    #[test]
    fn un_menor_dentro_de_un_script_no_abre_un_tag() {
        // Sin tratar `<script>` como texto crudo, `a < b` se leeria como el
        // comienzo de un elemento llamado `b`.
        let src = "<html lang=\"es\"><script>if (a < b) { c() }</script></html>";
        let r = analyze_html(src);
        assert_eq!(r.summary.total_elements, 2, "html y script, nada mas");
        assert!(r.findings.is_empty());
    }

    #[test]
    fn un_tag_dentro_de_un_comentario_se_ignora() {
        let r = analyze_html("<!-- <img src=x> -->\n<html lang=\"es\"></html>");
        assert!(r.findings.is_empty());
        assert_eq!(r.summary.images, 0);
    }

    #[test]
    fn doctype_no_cuenta_como_elemento() {
        let r = analyze_html("<!DOCTYPE html>\n<html lang=\"es\"></html>");
        assert_eq!(r.summary.total_elements, 1);
    }

    #[test]
    fn atributos_con_y_sin_comillas_se_leen_igual() {
        assert!(kinds("<img alt=hola src=x>").is_empty());
        assert!(kinds("<img alt='hola' src=x>").is_empty());
        assert!(kinds("<img alt=\"hola\" src=x>").is_empty());
    }

    #[test]
    fn la_linea_reportada_es_la_del_tag() {
        let r = analyze_html("<div>\n<p>\n<img src=x>\n</div>");
        assert_eq!(r.findings[0].line, 3);
    }

    // ── alt ──────────────────────────────────────────────────────────────

    #[test]
    fn img_sin_alt_se_reporta() {
        assert_eq!(kinds("<img src=\"x.png\">"), vec!["img_without_alt"]);
    }

    #[test]
    fn alt_vacio_es_correcto_y_no_se_reporta() {
        // La distincion que evita el falso positivo mas comun de esta
        // familia: `alt=""` marca una imagen decorativa a proposito.
        assert!(kinds("<img src=\"x.png\" alt=\"\">").is_empty());
    }

    // ── id duplicados ────────────────────────────────────────────────────

    #[test]
    fn id_repetido_se_reporta_con_la_linea_del_primero() {
        let r = analyze_html("<div id=\"x\"></div>\n<span id=\"x\"></span>");
        let f = r.findings.iter().find(|f| f.kind == "duplicate_id").unwrap();
        assert_eq!(f.line, 2);
        assert!(f.message.contains("linea 1"));
    }

    #[test]
    fn ids_distintos_no_se_reportan() {
        assert!(kinds("<div id=\"a\"></div><div id=\"b\"></div>").is_empty());
    }

    // ── lang ─────────────────────────────────────────────────────────────

    #[test]
    fn html_sin_lang_se_reporta_y_con_lang_no() {
        assert_eq!(kinds("<html></html>"), vec!["missing_lang"]);
        assert!(kinds("<html lang=\"es\"></html>").is_empty());
    }

    // ── encabezados ──────────────────────────────────────────────────────

    #[test]
    fn salto_de_nivel_de_encabezado_se_reporta() {
        let r = analyze_html("<h1>a</h1><h3>b</h3>");
        let f = r.findings.iter().find(|f| f.kind == "heading_level_skip").unwrap();
        assert!(f.message.contains("`<h2>`"));
    }

    #[test]
    fn secuencia_correcta_y_bajar_de_nivel_no_se_reportan() {
        assert!(kinds("<h1>a</h1><h2>b</h2><h3>c</h3>").is_empty());
        // Volver de h3 a h2 es normal: se cierra una subseccion.
        assert!(kinds("<h1>a</h1><h2>b</h2><h3>c</h3><h2>d</h2>").is_empty());
    }

    // ── controles de formulario ──────────────────────────────────────────

    #[test]
    fn input_sin_etiqueta_se_reporta() {
        assert_eq!(kinds("<input type=\"text\" id=\"n\">"), vec!["control_without_label"]);
    }

    #[test]
    fn input_con_label_for_no_se_reporta() {
        assert!(kinds("<label for=\"n\">Nombre</label><input type=\"text\" id=\"n\">").is_empty());
    }

    #[test]
    fn input_envuelto_en_label_no_se_reporta() {
        // Forma perfectamente correcta que un check ingenuo marcaria: no hay
        // `for` ni `id`, la asociacion viene de estar adentro.
        assert!(kinds("<label>Nombre <input type=\"text\"></label>").is_empty());
    }

    #[test]
    fn input_con_aria_label_no_se_reporta() {
        assert!(kinds("<input type=\"text\" aria-label=\"Buscar\">").is_empty());
    }

    #[test]
    fn input_hidden_y_submit_no_necesitan_etiqueta() {
        assert!(kinds("<input type=\"hidden\" name=\"t\">").is_empty());
        assert!(kinds("<input type=\"submit\" value=\"Enviar\">").is_empty());
    }

    #[test]
    fn select_y_textarea_tambien_se_chequean() {
        assert_eq!(kinds("<select></select>"), vec!["control_without_label"]);
        assert_eq!(kinds("<textarea></textarea>"), vec!["control_without_label"]);
    }

    #[test]
    fn el_cierre_de_label_baja_la_profundidad() {
        // El input de afuera SI debe reportarse: el `</label>` cerro el
        // contexto. Sin llevar la profundidad, quedaria "etiquetado" para
        // siempre despues del primer `<label>`.
        let r = analyze_html("<label>A <input type=\"text\"></label>\n<input type=\"text\">");
        let f: Vec<_> = r.findings.iter().filter(|f| f.kind == "control_without_label").collect();
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].line, 2);
    }

    // ── enlaces y tabindex ───────────────────────────────────────────────

    #[test]
    fn enlace_sin_href_se_reporta() {
        assert_eq!(kinds("<a>click</a>"), vec!["link_without_href"]);
        assert!(kinds("<a href=\"/x\">click</a>").is_empty());
    }

    #[test]
    fn tabindex_positivo_se_reporta_y_cero_o_menos_uno_no() {
        assert_eq!(kinds("<div tabindex=\"3\"></div>"), vec!["positive_tabindex"]);
        assert!(kinds("<div tabindex=\"0\"></div>").is_empty());
        assert!(kinds("<div tabindex=\"-1\"></div>").is_empty());
    }

    // ── general ──────────────────────────────────────────────────────────

    #[test]
    fn documento_accesible_no_produce_hallazgos() {
        let src = "<!DOCTYPE html>\n<html lang=\"es\">\n<body>\n<h1>T</h1>\n<h2>S</h2>\n<img src=\"a.png\" alt=\"Un gato\">\n<label for=\"q\">Buscar</label><input type=\"text\" id=\"q\">\n<a href=\"/x\">ir</a>\n</body>\n</html>";
        assert!(analyze_html(src).findings.is_empty());
    }

    #[test]
    fn el_resumen_cuenta_cada_categoria() {
        let src = "<html lang=\"es\"><img src=a alt=\"\"><a href=\"/\">x</a><input type=\"hidden\"><h1>t</h1></html>";
        let s = analyze_html(src).summary;
        assert_eq!(s.images, 1);
        assert_eq!(s.links, 1);
        assert_eq!(s.form_controls, 1);
        assert_eq!(s.headings, 1);
        assert!(s.bytes > 0);
    }

    #[test]
    fn archivo_vacio_no_rompe() {
        let r = analyze_html("");
        assert_eq!(r.summary.total_elements, 0);
        assert!(r.findings.is_empty());
    }

    #[test]
    fn la_nota_aclara_que_no_es_un_validador_ni_una_auditoria_completa() {
        let n = analyze_html("<html lang=\"es\"></html>").note;
        assert!(n.contains("NO es un validador"));
        assert!(n.contains("contraste"));
    }

    #[test]
    fn hallazgos_ordenados_por_linea() {
        let src = "<html>\n<img src=a>\n<a>x</a>";
        let lines: Vec<usize> = analyze_html(src).findings.iter().map(|f| f.line).collect();
        assert!(lines.windows(2).all(|w| w[0] <= w[1]), "sin ordenar: {lines:?}");
    }
}
