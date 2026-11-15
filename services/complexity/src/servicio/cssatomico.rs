//! **Atomic CSS: generar, y sobre todo MEDIR qué se gana y qué se paga.**
//!
//! El bullet de la Fase 31 lo dejó escrito como condición, no como detalle:
//! *"generar recién vale la pena cuando el medir puede probar qué se ganó"*.
//! El analizador ya existe (`cssparse.rs` da reglas, declaraciones, contexto
//! de `@media` y bytes), así que la condición está cumplida y esto es la otra
//! mitad.
//!
//! ## Qué es atomizar
//!
//! Dos reglas que comparten una declaración la repiten:
//!
//! ```css
//! .boton   { color: #fff; padding: 8px }
//! .enlace  { color: #fff; text-decoration: none }
//! ```
//!
//! Atomizado, cada par `(propiedad, valor)` existe **una sola vez** y los
//! elementos acumulan clases. `color: #fff` pasa de dos declaraciones a una.
//! La compresión es exactamente *declaraciones ÷ átomos*.
//!
//! ## Lo que este módulo afirma, y lo que NO
//!
//! **El CSS se achica; el HTML crece, y eso no se mide acá.** Atomizar no
//! borra información: la mueve del archivo de estilos a los atributos `class`
//! del marcado. Un elemento que tenía `class="boton"` pasa a tener
//! `class="c-fff p-8px"`. Decir "ahorrás N bytes" mirando solo el CSS sería
//! contar un lado del canje — por eso el resumen trae los bytes del CSS antes
//! y después **y dice explícitamente que el otro lado no está medido**.
//! `cssparse.rs` ya se niega a reportar "CSS sin usar" por la misma razón:
//! exige mirar el HTML/JS y aun así las clases se arman en runtime.
//!
//! **Y los átomos de un solo uso no comprimen nada.** Un par
//! `(propiedad, valor)` que aparece una vez sola se convierte en una clase
//! más para escribir en el marcado sin ahorrar una línea de CSS. El resumen
//! los cuenta aparte: sin ese número, una hoja con mucha variedad y poca
//! repetición parece atomizable y no lo es.
//!
//! **No reescribe nada.** Devuelve la hoja atómica y el mapa
//! `selector original → clases`, para que se pueda leer y comparar. Migrar el
//! marcado no se puede hacer con seguridad desde acá, y hacerlo a medias es
//! peor que no hacerlo — mismo criterio de *"detectar la brecha, proponer,
//! nunca convertir en silencio"* que la Fase 25 ya aplica a C/C++.
//!
//! ## Dos cosas que cambian el significado y por eso se separan
//!
//! - **El contexto de at-rule es parte de la identidad del átomo.**
//!   `color: red` dentro de `@media (max-width: 480px)` no es la misma
//!   declaración que fuera: colapsarlas rompería el override responsive, que
//!   es justo para lo que se escribió. La clave es `(contexto, propiedad,
//!   valor)`, no `(propiedad, valor)`.
//! - **`!important` tampoco se mezcla.** Gana contra reglas que el átomo sin
//!   `!important` perdería, así que son átomos distintos.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::lenguajes::cssparse::{CssAnalysisResult, CssRule};

#[derive(Serialize, Clone)]
pub struct AtomoCss {
    /// Nombre generado, determinista a partir de propiedad y valor.
    pub class: String,
    pub property: String,
    pub value: String,
    pub important: bool,
    /// `@media (...)` que lo contiene, o `None` en el nivel superior.
    pub at_context: Option<String>,
    /// Cuántas declaraciones del original colapsa este átomo.
    pub uses: usize,
    /// Selectores que lo usarían. Es lo que haría falta tocar en el marcado.
    pub selectors: Vec<String>,
}

#[derive(Serialize)]
pub struct AtomizacionSummary {
    pub declarations_before: usize,
    pub atoms_after: usize,
    pub bytes_before: usize,
    /// Bytes de la hoja atómica generada. **Solo del CSS**: lo que crece en
    /// el marcado no está contado acá, ver la nota.
    pub bytes_after: usize,
    /// `declaraciones / átomos`. 1.0 = no se repite nada y atomizar no
    /// comprime.
    pub reuse_ratio: f64,
    /// Átomos usados UNA sola vez: no ahorran una línea y agregan una clase
    /// al marcado. Es el costo que el ratio promedio esconde.
    pub single_use_atoms: usize,
    /// El átomo más reusado, para ver de dónde sale la compresión.
    pub top_atom: Option<String>,
    pub top_atom_uses: usize,
}

#[derive(Serialize)]
pub struct AtomizacionResult {
    pub atoms: Vec<AtomoCss>,
    /// La hoja atómica generada, lista para leer y comparar.
    pub stylesheet: String,
    /// `selector original → clases atómicas que lo reemplazarían`.
    pub mapping: Vec<SelectorMapeado>,
    pub summary: AtomizacionSummary,
    pub note: String,
}

#[derive(Serialize, Clone)]
pub struct SelectorMapeado {
    pub selector: String,
    pub at_context: Option<String>,
    pub classes: Vec<String>,
}

const NOTE: &str = "El CSS se achica; el HTML crece, y ese lado NO está medido acá. Atomizar no borra información: la mueve del archivo de estilos a los atributos `class` del marcado, así que `bytes_after` es solo el costo del CSS y no el balance. Mirá también `single_use_atoms`: un par (propiedad, valor) que aparece una vez no ahorra una línea y suma una clase al marcado. Esto NO reescribe nada — devuelve la hoja atómica y el mapa para que se puedan leer y comparar; migrar el marcado no se puede hacer con seguridad desde acá, porque las clases se arman en runtime.";

/// Nombre de clase determinista a partir de propiedad y valor.
///
/// Legible a propósito —`c-fff`, no `a1b2c3`— porque el valor de este módulo
/// está en que la hoja generada se pueda **leer** y comparar contra la
/// original, no en producirla para shipear.
fn nombre_de(property: &str, value: &str, important: bool, indice: usize) -> String {
    let limpiar = |s: &str, tope: usize| -> String {
        let mut out = String::new();
        let mut ultimo_guion = false;
        for c in s.chars().take(64) {
            if c.is_ascii_alphanumeric() {
                out.push(c.to_ascii_lowercase());
                ultimo_guion = false;
            } else if !ultimo_guion && !out.is_empty() {
                out.push('-');
                ultimo_guion = true;
            }
        }
        while out.ends_with('-') {
            out.pop();
        }
        out.truncate(tope);
        while out.ends_with('-') {
            out.pop();
        }
        out
    };
    // Iniciales de la propiedad para que el nombre no sea larguísimo:
    // `background-color` -> `bc`.
    let prop: String = property
        .split('-')
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.chars().next())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let val = limpiar(value, 24);
    let base = if val.is_empty() { prop.clone() } else { format!("{prop}-{val}") };
    let base = if important { format!("{base}-i") } else { base };
    // El índice desempata colisiones de nombres distintos que limpian igual
    // (`#fff` y `# f f f`), sin esconderlas.
    if indice == 0 { format!("a-{base}") } else { format!("a-{base}-{indice}") }
}

/// Clave de identidad de un átomo. **El contexto entra**: la misma
/// declaración dentro de un `@media` distinto no es la misma.
type Clave = (Option<String>, String, String, bool);

/// Atomiza el resultado de `cssparse::parse_css`.
pub fn atomizar(css: &CssAnalysisResult) -> AtomizacionResult {
    // `BTreeMap` y no `HashMap`: el orden de los átomos tiene que ser estable
    // entre corridas, o la hoja generada cambia sola y comparar dos salidas
    // deja de servir.
    let mut por_clave: BTreeMap<Clave, (Vec<String>, usize)> = BTreeMap::new();
    let mut declaraciones = 0usize;

    let selectores_de = |r: &CssRule| -> String { r.selectors.join(", ") };

    for regla in &css.rules {
        let sel = selectores_de(regla);
        for d in &regla.declarations {
            declaraciones += 1;
            let clave: Clave =
                (regla.at_context.clone(), d.property.clone(), d.value.clone(), d.important);
            let entrada = por_clave.entry(clave).or_insert_with(|| (Vec::new(), 0));
            entrada.1 += 1;
            if !entrada.0.contains(&sel) {
                entrada.0.push(sel.clone());
            }
        }
    }

    // Nombres: se asignan en el orden estable del BTreeMap y se desempatan
    // con un índice si dos claves distintas producen el mismo nombre limpio.
    let mut usados: BTreeMap<String, usize> = BTreeMap::new();
    let mut atoms: Vec<AtomoCss> = Vec::new();
    let mut nombre_por_clave: BTreeMap<Clave, String> = BTreeMap::new();

    for (clave, (selectores, usos)) in &por_clave {
        let (ctx, prop, val, important) = clave;
        let tentativo = nombre_de(prop, val, *important, 0);
        let n = usados.entry(tentativo.clone()).or_insert(0);
        let nombre = if *n == 0 { tentativo } else { nombre_de(prop, val, *important, *n) };
        *usados.get_mut(&nombre_de(prop, val, *important, 0)).unwrap() += 1;
        nombre_por_clave.insert(clave.clone(), nombre.clone());
        atoms.push(AtomoCss {
            class: nombre,
            property: prop.clone(),
            value: val.clone(),
            important: *important,
            at_context: ctx.clone(),
            uses: *usos,
            selectors: selectores.clone(),
        });
    }

    // La hoja generada, agrupando por contexto para que los `@media` queden
    // bien formados.
    let mut hoja = String::new();
    let mut contextos: Vec<Option<String>> =
        atoms.iter().map(|a| a.at_context.clone()).collect();
    contextos.dedup();
    contextos.sort();
    contextos.dedup();
    for ctx in &contextos {
        let del_ctx: Vec<&AtomoCss> = atoms.iter().filter(|a| &a.at_context == ctx).collect();
        if del_ctx.is_empty() {
            continue;
        }
        if let Some(c) = ctx {
            hoja.push_str(c);
            hoja.push_str(" {\n");
        }
        for a in del_ctx {
            let sangria = if ctx.is_some() { "  " } else { "" };
            hoja.push_str(&format!(
                "{sangria}.{} {{ {}: {}{} }}\n",
                a.class,
                a.property,
                a.value,
                if a.important { " !important" } else { "" }
            ));
        }
        if ctx.is_some() {
            hoja.push_str("}\n");
        }
    }

    // El mapa: qué clases reemplazarían a cada selector original.
    let mut mapping: Vec<SelectorMapeado> = Vec::new();
    for regla in &css.rules {
        let sel = selectores_de(regla);
        let mut clases: Vec<String> = Vec::new();
        for d in &regla.declarations {
            let clave: Clave =
                (regla.at_context.clone(), d.property.clone(), d.value.clone(), d.important);
            if let Some(n) = nombre_por_clave.get(&clave) {
                if !clases.contains(n) {
                    clases.push(n.clone());
                }
            }
        }
        if !clases.is_empty() {
            mapping.push(SelectorMapeado {
                selector: sel,
                at_context: regla.at_context.clone(),
                classes: clases,
            });
        }
    }

    let single = atoms.iter().filter(|a| a.uses == 1).count();
    let top = atoms.iter().max_by_key(|a| a.uses);
    let ratio = if atoms.is_empty() { 0.0 } else { declaraciones as f64 / atoms.len() as f64 };

    AtomizacionResult {
        summary: AtomizacionSummary {
            declarations_before: declaraciones,
            atoms_after: atoms.len(),
            bytes_before: css.summary.bytes,
            bytes_after: hoja.len(),
            reuse_ratio: (ratio * 100.0).round() / 100.0,
            single_use_atoms: single,
            top_atom: top.map(|a| a.class.clone()),
            top_atom_uses: top.map(|a| a.uses).unwrap_or(0),
        },
        atoms,
        stylesheet: hoja,
        mapping,
        note: NOTE.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lenguajes::cssparse::parse_css;

    fn at(src: &str) -> AtomizacionResult {
        atomizar(&parse_css(src))
    }

    /// El caso de manual: dos reglas que comparten una declaración la
    /// colapsan en un átomo usado dos veces.
    #[test]
    fn una_declaracion_compartida_colapsa_en_un_atomo() {
        let r = at(".boton { color: #fff; padding: 8px }\n.enlace { color: #fff; text-decoration: none }\n");
        assert_eq!(r.summary.declarations_before, 4);
        assert_eq!(r.summary.atoms_after, 3, "color:#fff se comparte");
        let compartido = r.atoms.iter().find(|a| a.property == "color").unwrap();
        assert_eq!(compartido.uses, 2);
        assert_eq!(compartido.selectors.len(), 2);
    }

    /// **El contexto de at-rule es parte de la identidad.** Colapsar un
    /// override responsive con la regla de afuera rompería justamente lo que
    /// el `@media` existe para hacer.
    #[test]
    fn la_misma_declaracion_en_un_media_distinto_no_es_el_mismo_atomo() {
        let r = at(".a { color: red }\n@media (max-width: 480px) { .a { color: red } }\n");
        assert_eq!(r.summary.atoms_after, 2, "no se pueden colapsar: {:?}", r.atoms.iter().map(|a| (&a.class, &a.at_context)).collect::<Vec<_>>());
        assert!(r.atoms.iter().any(|a| a.at_context.is_some()));
        assert!(r.atoms.iter().any(|a| a.at_context.is_none()));
    }

    /// `!important` gana contra reglas que el átomo sin él perdería: son
    /// átomos distintos.
    #[test]
    fn important_no_se_mezcla_con_la_misma_declaracion_sin_important() {
        let r = at(".a { color: red }\n.b { color: red !important }\n");
        assert_eq!(r.summary.atoms_after, 2);
        assert!(r.atoms.iter().any(|a| a.important));
    }

    /// **El número que impide vender humo.** Una hoja sin repeticiones no
    /// comprime nada: ratio 1.0 y todos los átomos de un solo uso.
    #[test]
    fn una_hoja_sin_repeticiones_no_comprime_y_lo_dice() {
        let r = at(".a { color: red }\n.b { margin: 0 }\n.c { padding: 1px }\n");
        assert_eq!(r.summary.reuse_ratio, 1.0);
        assert_eq!(r.summary.single_use_atoms, 3, "los tres son de un solo uso");
        assert_eq!(r.summary.atoms_after, r.summary.declarations_before);
    }

    /// La hoja generada tiene que ser CSS válido y re-parseable: si no, no se
    /// puede comparar contra la original, que es para lo que existe.
    #[test]
    fn la_hoja_generada_se_puede_volver_a_parsear() {
        let r = at(".boton { color: #fff; padding: 8px }\n@media (max-width: 480px) { .boton { padding: 4px } }\n");
        let re = parse_css(&r.stylesheet);
        assert_eq!(re.summary.total_rules, r.summary.atoms_after, "la hoja generada tiene una regla por átomo");
        assert!(r.stylesheet.contains("@media"), "el contexto se conserva: {}", r.stylesheet);
    }

    /// El mapa dice qué clases reemplazan a cada selector — es lo que haría
    /// falta tocar en el marcado, y lo que este módulo deliberadamente NO
    /// hace.
    #[test]
    fn el_mapa_lista_las_clases_de_cada_selector_original() {
        let r = at(".boton { color: #fff; padding: 8px }\n");
        let m = r.mapping.iter().find(|m| m.selector == ".boton").expect("falta .boton");
        assert_eq!(m.classes.len(), 2);
    }

    /// Los nombres son deterministas: dos corridas sobre el mismo fuente dan
    /// la misma hoja, o comparar dos salidas no sirve de nada.
    #[test]
    fn la_salida_es_determinista() {
        let src = ".a { color: red; margin: 0 }\n.b { color: blue; margin: 0 }\n";
        assert_eq!(at(src).stylesheet, at(src).stylesheet);
    }

    /// La nota tiene que decir que el otro lado del canje no está medido.
    #[test]
    fn la_nota_avisa_que_el_html_crece_y_no_se_mide() {
        let r = at(".a { color: red }");
        assert!(r.note.contains("HTML crece"), "{}", r.note);
        assert!(r.note.contains("single_use_atoms"), "{}", r.note);
    }

    #[test]
    fn una_hoja_vacia_no_rompe_nada() {
        let r = at("");
        assert_eq!(r.summary.atoms_after, 0);
        assert_eq!(r.summary.reuse_ratio, 0.0);
        assert!(r.stylesheet.is_empty());
    }
}
