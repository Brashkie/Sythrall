//! Sythrall CSS Engine — referencias cruzadas CSS ↔ marcado/scripts.
//!
//! El bullet que `cssparse.rs` dejó explícitamente abierto: "detección de CSS
//! sin usar con un hedge honesto". Va en su propio módulo porque es una clase
//! de análisis distinta — `cssparse.rs` prueba cosas mirando UN archivo, esto
//! cruza archivos y **no puede probar nada**.
//!
//! **Por qué el hedge no es opcional acá.** Medido de primera mano sobre el
//! propio frontend de Sythrall: un detector ingenuo de clases sin uso sobre
//! `main.css` devolvió 34 candidatas y **33 eran falsos positivos** —
//! `.t-flake8`, `.sev-error`, `.rank-A`, `.src-be`, `.fstate-ok`, todas
//! construidas en runtime como `` `t-${tool}` ``. Una sola estaba realmente
//! muerta. Un análisis estático NO puede probar que una regla CSS no se usa;
//! como mucho puede decir que no encontró ninguna referencia escrita.
//!
//! De ahí las 2 decisiones de diseño que definen el módulo:
//!
//! 1. **El lado "referenciada" es deliberadamente permisivo.** Cualquier
//!    token del proyecto que coincida con el nombre de la clase cuenta como
//!    referencia, aunque sea una variable que se llama igual. Un falso
//!    "referenciada" no le hace daño a nadie; un falso "sin usar" invita a
//!    borrar CSS que sí se usa. El error se inclina siempre hacia el lado
//!    seguro, a propósito.
//! 2. **Los sitios de construcción dinámica se reportan aparte, como
//!    contexto de primera clase.** Si el proyecto arma nombres de clase en
//!    runtime, la lista de "sin referencia" deja de ser confiable EN SU
//!    TOTALIDAD, no solo para esas clases — porque no se sabe qué nombres
//!    genera. El reporte muestra los dos números juntos para que nadie lea el
//!    primero sin el segundo.
//!
//! Lo que este módulo NUNCA dice: "sin usar", "seguro de borrar", ni un
//! porcentaje de reducción alcanzable. Dice "sin referencia estática
//! encontrada", que es lo único que puede sostener.

use std::collections::{HashMap, HashSet};

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::lenguajes::cssparse::parse_css;

#[derive(Serialize, Clone)]
pub struct UnreferencedClass {
    pub name: String,
    pub file: String,
    pub line: usize,
}

#[derive(Serialize, Clone)]
pub struct DynamicClassSite {
    pub file: String,
    pub line: usize,
    /// El fragmento que construye el nombre, recortado — para que se pueda
    /// ver por qué el análisis no puede decidir sobre ese caso.
    pub snippet: String,
}

#[derive(Serialize)]
pub struct CssUsageReport {
    pub css_files: usize,
    pub scanned_files: usize,
    pub defined_classes: usize,
    pub referenced_classes: usize,
    pub unreferenced: Vec<UnreferencedClass>,
    pub dynamic_sites: Vec<DynamicClassSite>,
    pub note: String,
}

/// `class="..."` / `className="..."` cuyo VALOR contiene `${` — el nombre se
/// arma en runtime. Es el patrón real que produjo los 33 falsos positivos
/// medidos sobre este mismo repositorio (`class="ii-tool t-${i.tool}"`).
static DYNAMIC_ATTR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"class(?:Name)?\s*=\s*["'][^"']*\$\{"#).unwrap());

/// `classList.add(x)` con un argumento que NO empieza por comilla: una
/// variable, una concatenación o un template. Con una comilla el nombre es
/// literal y el lado permisivo de abajo ya lo cuenta como referencia.
static DYNAMIC_CLASSLIST_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"classList\s*\.\s*(?:add|remove|toggle|replace)\s*\(\s*[^"'\s)]"#).unwrap());

/// Cualquier token con forma de nombre de clase CSS. Se usa para el lado
/// permisivo: si el token aparece EN CUALQUIER PARTE del proyecto, la clase
/// cuenta como referenciada.
static TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z_][A-Za-z0-9_-]*").unwrap());

/// Nombres de clase de un selector: cada `.` seguido de un identificador.
/// `.a.b` da 2, `.a .b` da 2, `a.b` da 1 (`b`).
fn class_names_of(selector: &str) -> Vec<String> {
    let chars: Vec<char> = selector.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '.' {
            i += 1;
            continue;
        }
        i += 1;
        let start = i;
        while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
            i += 1;
        }
        if i > start {
            out.push(chars[start..i].iter().collect());
        }
    }
    out
}

fn is_css(filename: &str) -> bool {
    filename.to_lowercase().ends_with(".css")
}

fn note_for(unreferenced: usize, dynamic: usize, scanned: usize, css_files: usize) -> String {
    let base = format!(
        "{unreferenced} clase(s) sin ninguna referencia escrita en los {scanned} archivo(s) analizados ({css_files} de ellos CSS). Esto NO significa \"sin usar\" ni \"seguro de borrar\": un analisis estatico no puede probar que una regla CSS no se aplica."
    );
    // El conteo de archivos va en la nota a proposito. Encontrado en vivo
    // corriendo esto sobre el propio frontend de Sythrall: apuntandolo a
    // `apps/web/src` (sin `index.html`, que esta un nivel arriba) el reporte
    // dio 105 clases "sin referencia"; con la raiz correcta dio 31. El
    // resultado vale exactamente lo que valga el conjunto de archivos
    // mirado, y esa es la primera pregunta que hay que poder responder al
    // leerlo.
    let alcance = " Si ese conteo no cubre todo el marcado del proyecto (plantillas del servidor, HTML fuera del directorio analizado), sobran entradas en la lista por esa sola razon.";
    if dynamic == 0 {
        format!(
            "{base} No se encontraron sitios que construyan nombres de clase en runtime, asi que la lista de arriba es lo mas confiable que este analisis puede ofrecer — pero sigue sin ser una prueba (un nombre puede venir de datos, de un template del servidor o de una libreria).{alcance}"
        )
    } else {
        format!(
            "{base} Ademas hay {dynamic} sitio(s) que ARMAN nombres de clase en runtime (ver la lista): mientras existan, la lista entera pierde confiabilidad, no solo algunas entradas — porque no se sabe que nombres generan.{alcance}"
        )
    }
}

/// Cruza las clases definidas en los `.css` contra todo lo escrito en el
/// resto de los archivos. `files` son pares `(ruta_relativa, contenido)`.
pub fn analyze_css_usage(files: &[(String, String)]) -> CssUsageReport {
    // ── Lado CSS: qué clases se definen y dónde ──
    let mut defined: HashMap<String, (String, usize)> = HashMap::new();
    let mut css_files = 0usize;
    for (name, content) in files.iter().filter(|(n, _)| is_css(n)) {
        css_files += 1;
        for rule in parse_css(content).rules {
            for selector in &rule.selectors {
                for class in class_names_of(selector) {
                    // La primera definición gana: es donde el lector va a
                    // buscar la regla.
                    defined.entry(class).or_insert_with(|| (name.clone(), rule.line));
                }
            }
        }
    }

    // ── Lado marcado/scripts: qué se escribe, y qué se arma en runtime ──
    let mut referenced: HashSet<String> = HashSet::new();
    let mut dynamic_sites = Vec::new();
    for (name, content) in files.iter().filter(|(n, _)| !is_css(n)) {
        for token in TOKEN_RE.find_iter(content) {
            referenced.insert(token.as_str().to_string());
        }
        for (i, line) in content.lines().enumerate() {
            let hit = DYNAMIC_ATTR_RE.find(line).or_else(|| DYNAMIC_CLASSLIST_RE.find(line));
            if let Some(m) = hit {
                let from = m.start().saturating_sub(10);
                let to = (m.end() + 30).min(line.len());
                dynamic_sites.push(DynamicClassSite {
                    file: name.clone(),
                    line: i + 1,
                    snippet: line[from..to].trim().to_string(),
                });
            }
        }
    }

    let mut unreferenced: Vec<UnreferencedClass> = defined
        .iter()
        .filter(|(class, _)| !referenced.contains(class.as_str()))
        .map(|(class, (file, line))| UnreferencedClass { name: class.clone(), file: file.clone(), line: *line })
        .collect();
    // Orden estable: sin esto la salida cambia entre corridas (HashMap).
    unreferenced.sort_by(|a, b| (&a.file, a.line, &a.name).cmp(&(&b.file, b.line, &b.name)));
    dynamic_sites.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));

    let note = note_for(unreferenced.len(), dynamic_sites.len(), files.len(), css_files);
    CssUsageReport {
        css_files,
        scanned_files: files.len(),
        defined_classes: defined.len(),
        referenced_classes: defined.len() - unreferenced.len(),
        unreferenced,
        dynamic_sites,
        note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn clase_usada_en_html_no_se_reporta() {
        let f = files(&[(".css", ""), ("a.css", ".btn { color: red; }"), ("i.html", "<div class=\"btn\"></div>")]);
        let r = analyze_css_usage(&f);
        assert_eq!(r.defined_classes, 1);
        assert!(r.unreferenced.is_empty());
    }

    #[test]
    fn clase_sin_ninguna_referencia_se_reporta_con_archivo_y_linea() {
        let f = files(&[("a.css", ".btn { color: red; }\n.huerfana { color: blue; }"), ("i.html", "<div class=\"btn\"></div>")]);
        let r = analyze_css_usage(&f);
        assert_eq!(r.unreferenced.len(), 1);
        assert_eq!(r.unreferenced[0].name, "huerfana");
        assert_eq!(r.unreferenced[0].file, "a.css");
        assert_eq!(r.unreferenced[0].line, 2);
    }

    #[test]
    fn el_lado_referenciado_es_permisivo_a_proposito() {
        // `activo` aparece como nombre de variable, no como clase. Cuenta
        // igual: un falso "referenciada" es inofensivo, un falso "sin usar"
        // invita a borrar CSS que si se usa.
        let f = files(&[("a.css", ".activo { color: red; }"), ("s.ts", "const activo = 1")]);
        assert!(analyze_css_usage(&f).unreferenced.is_empty());
    }

    #[test]
    fn clase_construida_en_un_atributo_se_marca_como_sitio_dinamico() {
        // El patron real que produjo 33 falsos positivos en este repositorio.
        let f = files(&[
            ("a.css", ".t-flake8 { color: red; }"),
            ("s.ts", "const h = `<span class=\"ii-tool t-${tool}\">${tool}</span>`"),
        ]);
        let r = analyze_css_usage(&f);
        assert_eq!(r.dynamic_sites.len(), 1);
        assert_eq!(r.dynamic_sites[0].file, "s.ts");
        assert!(r.note.contains("ARMAN nombres de clase en runtime"));
    }

    #[test]
    fn class_list_add_con_variable_es_sitio_dinamico() {
        let f = files(&[("a.css", ".x { color: red; }"), ("s.ts", "el.classList.add(nombre)")]);
        assert_eq!(analyze_css_usage(&f).dynamic_sites.len(), 1);
    }

    #[test]
    fn class_list_add_con_literal_no_es_dinamico() {
        // Con comillas el nombre es literal, y el lado permisivo ya lo ve.
        let f = files(&[("a.css", ".activa { color: red; }"), ("s.ts", "el.classList.add('activa')")]);
        let r = analyze_css_usage(&f);
        assert!(r.dynamic_sites.is_empty());
        assert!(r.unreferenced.is_empty());
    }

    #[test]
    fn class_estatico_no_es_sitio_dinamico() {
        let f = files(&[("a.css", ".btn { color: red; }"), ("i.html", "<div class=\"btn activo\"></div>")]);
        assert!(analyze_css_usage(&f).dynamic_sites.is_empty());
    }

    #[test]
    fn la_nota_cambia_segun_haya_o_no_construccion_dinamica() {
        let limpio = files(&[("a.css", ".x { color: red; }"), ("i.html", "<div class=\"y\"></div>")]);
        let n1 = analyze_css_usage(&limpio).note;
        assert!(n1.contains("No se encontraron sitios"));
        assert!(n1.contains("sigue sin ser una prueba"));

        let dinamico = files(&[("a.css", ".x { color: red; }"), ("s.ts", "h = `<i class=\"a-${b}\">`")]);
        let n2 = analyze_css_usage(&dinamico).note;
        assert!(n2.contains("pierde confiabilidad"));
    }

    #[test]
    fn la_nota_nunca_dice_sin_usar_ni_seguro_de_borrar() {
        let f = files(&[("a.css", ".x { color: red; }"), ("i.html", "<div></div>")]);
        let note = analyze_css_usage(&f).note;
        assert!(note.contains("NO significa"));
        assert!(note.contains("referencia escrita"));
        assert!(note.contains("no puede probar"));
    }

    #[test]
    fn la_nota_dice_cuantos_archivos_miro() {
        // Encontrado en vivo: apuntando el analisis a `apps/web/src` (sin
        // `index.html`) el reporte daba 105 clases "sin referencia"; con la
        // raiz correcta, 31. El conteo de archivos es la primera pregunta
        // que hay que poder responder al leer el resultado.
        let f = files(&[("a.css", ".x { color: red; }"), ("i.html", "<div></div>")]);
        let note = analyze_css_usage(&f).note;
        assert!(note.contains("2 archivo(s) analizados"));
        assert!(note.contains("1 de ellos CSS"));
        assert!(note.contains("sobran entradas"));
    }

    #[test]
    fn nombres_de_clase_de_un_selector_compuesto() {
        assert_eq!(class_names_of(".a.b"), vec!["a", "b"]);
        assert_eq!(class_names_of(".a .b"), vec!["a", "b"]);
        assert_eq!(class_names_of("div.card"), vec!["card"]);
        assert_eq!(class_names_of("#id"), Vec::<String>::new());
        assert_eq!(class_names_of(".con-guion_y_bajo"), vec!["con-guion_y_bajo"]);
    }

    #[test]
    fn la_primera_definicion_es_la_que_se_reporta() {
        let f = files(&[("a.css", ".x { color: red; }\n.otra { color: blue; }\n.x { padding: 0; }")]);
        let r = analyze_css_usage(&f);
        let x = r.unreferenced.iter().find(|u| u.name == "x").unwrap();
        assert_eq!(x.line, 1, "el lector va a buscar la regla donde se define primero");
    }

    #[test]
    fn proyecto_sin_css_no_rompe() {
        let r = analyze_css_usage(&files(&[("s.ts", "const x = 1")]));
        assert_eq!(r.css_files, 0);
        assert_eq!(r.defined_classes, 0);
        assert!(r.unreferenced.is_empty());
    }

    #[test]
    fn la_salida_es_estable_entre_corridas() {
        // `defined` es un HashMap: sin el sort, el orden cambiaria y el
        // reporte seria distinto cada vez sobre el mismo proyecto.
        let f = files(&[("a.css", ".z { color: red; }\n.y { color: red; }\n.x { color: red; }")]);
        let uno: Vec<String> = analyze_css_usage(&f).unreferenced.iter().map(|u| u.name.clone()).collect();
        let dos: Vec<String> = analyze_css_usage(&f).unreferenced.iter().map(|u| u.name.clone()).collect();
        assert_eq!(uno, dos);
        assert_eq!(uno.len(), 3);
    }

    #[test]
    fn conteos_del_resumen_son_coherentes() {
        let f = files(&[("a.css", ".usada { color: red; }\n.sola { color: blue; }"), ("i.html", "<b class=\"usada\"></b>")]);
        let r = analyze_css_usage(&f);
        assert_eq!(r.defined_classes, 2);
        assert_eq!(r.referenced_classes, 1);
        assert_eq!(r.unreferenced.len(), 1);
        assert_eq!(r.css_files, 1);
        assert_eq!(r.scanned_files, 2);
    }
}
