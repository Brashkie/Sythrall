//! **Gramáticas atribuidas, primera porción: `grammar.js` de tree-sitter.**
//!
//! Sale del bullet que el usuario hizo reinstaurar, y que tenía razón en
//! los dos lados: *"nadie analiza `.g4`/`.y`/`grammar.js`"*, y *"Sythrall
//! está hecho de gramáticas tree-sitter, así que se come su propia comida"*.
//! Esta porción hace literal lo segundo: el analizador corre sobre la
//! **misma gramática que el motor usa para parsear Rust**.
//!
//! Medido antes de escribir nada, sobre `tree-sitter-rust/grammar.js` (1690
//! líneas): el motor reportaba **191 "funciones"** —las flechas `$ => …`— y
//! nada más. No sabía que era una gramática.
//!
//! **No hace falta un parser nuevo**: `grammar.js` es JavaScript, y el motor
//! ya lo parsea. Lo que faltaba era leer la estructura que ese JavaScript
//! describe.
//!
//! **El hecho que un analizador de gramáticas tiene que saber, y que hace
//! que este no sea el de ANTLR:** la recursión por izquierda **NO es un
//! error en tree-sitter**. Su parser es GLR, así que `expr: $ => seq($.expr,
//! '+', $.expr)` es legal y común. En un descenso recursivo es un bucle
//! infinito, y en ANTLR la indirecta es fatal. Reportarla acá como defecto
//! sería aplicar la regla de otro formalismo — así que **deliberadamente no
//! se reporta**, y queda dicho para que no se "arregle" más adelante.
//!
//! **La otra cosa que este módulo sabe y cuesta caro no saber**: una regla
//! que empieza con `_` es **oculta** — no produce un nodo en el árbol. Ese
//! es exactamente el tipo de sorpresa que este crate se comió varias veces
//! mirando `kind()` en el depurador.

use serde::Serialize;
use tree_sitter::Node;

use crate::hallazgos::smells::StructuralSmell;
use crate::ast::tsnode::{self, text_of};

/// Las cuentas de una gramática de tree-sitter.
#[derive(Serialize, Clone, Default, Debug)]
pub struct GrammarInfo {
    /// El `name:` de la gramática.
    pub name: String,
    pub total_rules: usize,
    /// Reglas que empiezan con `_`: **no producen nodo en el árbol**, se
    /// inlinean. Por eso un `kind()` nunca vale `_statement`.
    pub hidden_rules: usize,
    /// Entradas de `conflicts`. **Cada una es una ambigüedad DECLARADA**:
    /// el autor le dice a tree-sitter que use GLR en ese punto porque no
    /// se puede decidir con un solo token de lookahead.
    pub declared_conflicts: usize,
    pub supertypes: usize,
    /// La regla de arranque: en tree-sitter es **la primera** de `rules`,
    /// no una declarada aparte.
    pub start_rule: String,
    /// Tokens declarados en `externals`: los produce un escáner externo en
    /// C, **no una regla de la gramática**. Están definidos aunque no
    /// aparezcan en `rules`.
    pub external_tokens: usize,
    /// `true` si `rules` contiene un spread (`...helper(...)`), o sea que
    /// **parte de las reglas se generan con una función** y no se pueden
    /// leer del texto.
    pub has_generated_rules: bool,
}

/// `true` si la expresión es `$.algo`, y devuelve el nombre.
fn referencia_a_regla<'a>(n: Node, source: &'a str) -> Option<&'a str> {
    if n.kind() != "member_expression" {
        return None;
    }
    let obj = n.child_by_field_name("object")?;
    if text_of(obj, source) != "$" {
        return None;
    }
    let prop = n.child_by_field_name("property")?;
    Some(text_of(prop, source))
}

/// Nombres CREADOS por un `alias(regla, nombre)`.
///
/// **El segundo argumento de `alias` no referencia nada: inventa un nombre
/// de nodo.** `alias($.identifier, $.property_identifier)` hace que ese
/// `identifier` aparezca en el árbol como `property_identifier`, que no es
/// —ni tiene por qué ser— una regla. Sin esta distinción, las gramáticas de
/// Rust y JavaScript daban 9 y 5 referencias rotas, todas falsas.
fn alias_definidos(n: Node, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    tsnode::walk_flat(n, &mut |x| {
        if x.kind() != "call_expression" {
            return;
        }
        let Some(f) = x.child_by_field_name("function") else { return };
        if text_of(f, source) != "alias" {
            return;
        }
        let Some(args) = x.child_by_field_name("arguments") else { return };
        let mut c = args.walk();
        let a: Vec<Node> = args.named_children(&mut c).collect();
        // El SEGUNDO argumento es el nombre nuevo.
        if let Some(segundo) = a.get(1) {
            if let Some(nombre) = referencia_a_regla(*segundo, source) {
                out.push(nombre.to_string());
            }
        }
    });
    out
}

/// Todas las reglas referenciadas en un subárbol.
fn referencias_en(n: Node, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    tsnode::walk_flat(n, &mut |x| {
        if let Some(r) = referencia_a_regla(x, source) {
            out.push(r.to_string());
        }
    });
    out
}

/// El texto de una clave de par: soporta `foo:`, `'foo':` y `"foo":`.
fn clave_de_par<'a>(par: Node, source: &'a str) -> Option<&'a str> {
    let k = par.child_by_field_name("key")?;
    let t = text_of(k, source);
    Some(t.trim_matches(|c| c == '\'' || c == '"'))
}

/// El objeto que se le pasa a `grammar({...})`, si el archivo es una
/// gramática de tree-sitter.
fn objeto_de_gramatica<'a>(raiz: Node<'a>, source: &str) -> Option<Node<'a>> {
    let mut hallado = None;
    tsnode::walk_flat(raiz, &mut |n| {
        if hallado.is_some() || n.kind() != "call_expression" {
            return;
        }
        let Some(f) = n.child_by_field_name("function") else { return };
        if text_of(f, source) != "grammar" {
            return;
        }
        let Some(args) = n.child_by_field_name("arguments") else { return };
        let mut c = args.walk();
        // `grammar(base, {...})` también existe (herencia de gramática): se
        // toma el ÚLTIMO objeto, que es el de las reglas propias.
        let objetos: Vec<Node> = args.named_children(&mut c).filter(|x| x.kind() == "object").collect();
        let obj = objetos.last().copied();
        if let Some(o) = obj {
            hallado = Some(o);
        }
    });
    hallado
}

/// Los pares directos de un objeto, sin descender.
fn pares_de<'a>(obj: Node<'a>) -> Vec<Node<'a>> {
    let mut c = obj.walk();
    obj.named_children(&mut c).filter(|x| x.kind() == "pair").collect()
}

/// Entrada desde el texto: parsea con la gramática de JavaScript y analiza.
///
/// **Filtra por texto ANTES de parsear**, y el motivo es de costo: esto
/// corre para todo `.js` del proyecto, y parsear dos veces cada archivo
/// para que el 99,9% no sea una gramática no se paga. Las dos subcadenas
/// que se exigen están en toda gramática de tree-sitter por construcción.
pub fn analyze_source(content: &str) -> Option<(GrammarInfo, Vec<StructuralSmell>)> {
    if !(content.contains("grammar(") && content.contains("rules:")) {
        return None;
    }
    let mut p = tree_sitter::Parser::new();
    p.set_language(&tree_sitter_javascript::LANGUAGE.into()).ok()?;
    let arbol = p.parse(content, None)?;
    analyze(arbol.root_node(), content)
}

/// Analiza un `grammar.js` de tree-sitter. `None` si el archivo no lo es.
pub fn analyze(raiz: Node, source: &str) -> Option<(GrammarInfo, Vec<StructuralSmell>)> {
    let obj = objeto_de_gramatica(raiz, source)?;
    let mut info = GrammarInfo::default();
    let mut reglas: Vec<(String, Node)> = Vec::new();
    let mut referencias: Vec<String> = Vec::new();
    let mut externos: Vec<String> = Vec::new();
    let alias: Vec<String> = alias_definidos(obj, source);

    for par in pares_de(obj) {
        let Some(clave) = clave_de_par(par, source) else { continue };
        let Some(valor) = par.child_by_field_name("value") else { continue };
        match clave {
            "name" => info.name = text_of(valor, source).trim_matches(|c| c == '\'' || c == '"').to_string(),
            "rules" => {
                for regla in pares_de(valor) {
                    let Some(nombre) = clave_de_par(regla, source) else { continue };
                    reglas.push((nombre.to_string(), regla));
                }
                // **Un spread significa que hay reglas que no están en el
                // texto**: `...preprocIf('', $ => …)` de la gramática de C
                // fabrica `preproc_if`, `preproc_ifdef` y varias más.
                let mut c = valor.walk();
                info.has_generated_rules = valor.named_children(&mut c).any(|x| x.kind() == "spread_element");
            }
            "conflicts" => info.declared_conflicts = contar_arreglos_internos(valor),
            // **Los `externals` SÍ están definidos**, aunque no estén en
            // `rules`: los produce un escáner externo escrito en C. Sin
            // leerlos, `$.string_content` de la gramática de Rust salía
            // como referencia rota.
            "externals" => {
                let nombres = referencias_en(valor, source);
                info.external_tokens = nombres.len();
                externos.extend(nombres);
            }
            "supertypes" => info.supertypes = referencias_en(valor, source).len(),
            _ => {}
        }
        // `extras`, `inline`, `word`, `precedences` y `conflicts` TAMBIÉN
        // referencian reglas, y una regla usada solo ahí no está muerta.
        if clave != "rules" {
            referencias.extend(referencias_en(valor, source));
        }
    }

    if reglas.is_empty() {
        return None;
    }

    for (_, nodo) in &reglas {
        if let Some(cuerpo) = nodo.child_by_field_name("value") {
            referencias.extend(referencias_en(cuerpo, source));
        }
    }

    info.total_rules = reglas.len();
    info.hidden_rules = reglas.iter().filter(|(n, _)| n.starts_with('_')).count();
    info.start_rule = reglas.first().map(|(n, _)| n.clone()).unwrap_or_default();

    let mut definidas: std::collections::HashSet<&str> = reglas.iter().map(|(n, _)| n.as_str()).collect();
    definidas.extend(externos.iter().map(|s| s.as_str()));
    definidas.extend(alias.iter().map(|s| s.as_str()));
    let mut smells = Vec::new();

    // **Con reglas generadas, el conjunto de definidas es incompleto y los
    // DOS hallazgos dejan de ser afirmables.** Medido contra las cuatro
    // gramáticas que este motor usa de verdad, la primera versión de este
    // módulo daba 19/19, 13/13, 59/59 y 10/10 hallazgos, y **todos eran
    // falsos**: unos eran `externals` y otros reglas fabricadas por una
    // función auxiliar. No hay forma de leer del texto lo que produce una
    // función, así que acá se calla en vez de acusar.
    if info.has_generated_rules {
        return Some((info, smells));
    }

    // ── Referencia a una regla que no existe ────────────────────────────
    //
    // **Esto no es un estilo discutible: `tree-sitter generate` falla.** Es
    // decidible desde el texto y no depende del formalismo.
    let mut vistas = std::collections::HashSet::new();
    for r in &referencias {
        if definidas.contains(r.as_str()) || !vistas.insert(r.clone()) {
            continue;
        }
        smells.push(StructuralSmell {
            kind: "undefined_grammar_rule",
            name: r.clone(),
            line: 1,
            message: format!(
                "La gramática referencia `$.{r}` pero no define una regla con ese nombre. `tree-sitter generate` falla con esto — no es una cuestión de estilo. Suele ser un renombre a medias o una regla que se movió a otra gramática."
            ),
        });
    }

    // ── Regla definida y nunca referenciada ─────────────────────────────
    //
    // **La regla de arranque se excluye a mano, y el motivo importa**: en
    // tree-sitter el punto de entrada es LA PRIMERA de `rules`, no una
    // declarada aparte, así que nadie la referencia por diseño. Tratarla
    // como muerta sería marcar el corazón de toda gramática.
    let usadas: std::collections::HashSet<&str> = referencias.iter().map(|s| s.as_str()).collect();
    for (nombre, nodo) in &reglas {
        if *nombre == info.start_rule || usadas.contains(nombre.as_str()) {
            continue;
        }
        smells.push(StructuralSmell {
            kind: "unreferenced_grammar_rule",
            name: nombre.clone(),
            line: nodo.start_position().row + 1,
            message: format!(
                "La regla `{nombre}` está definida y ninguna otra la referencia, y tampoco es la regla de arranque (en tree-sitter esa es la PRIMERA de `rules`, no una declarada aparte). tree-sitter la genera igual, así que no rompe nada: engorda la tabla de parseo y confunde a quien lea la gramática. Suele quedar de un refactor donde la regla que la usaba cambió de forma."
            ),
        });
    }

    smells.sort_by_key(|s| s.line);
    Some((info, smells))
}

/// Cuenta los arreglos internos de `conflicts: $ => [[...], [...]]`.
fn contar_arreglos_internos(n: Node) -> usize {
    let mut externo = None;
    tsnode::walk_flat(n, &mut |x| {
        if externo.is_none() && x.kind() == "array" {
            externo = Some(x);
        }
    });
    let Some(a) = externo else { return 0 };
    let mut c = a.walk();
    a.named_children(&mut c).filter(|x| x.kind() == "array").count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn analizar(src: &str) -> Option<(GrammarInfo, Vec<StructuralSmell>)> {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_javascript::LANGUAGE.into()).unwrap();
        let t = p.parse(src, None).unwrap();
        analyze(t.root_node(), src)
    }

    const BASE: &str = r#"module.exports = grammar({
  name: 'mini',
  extras: $ => [/\s/, $.comment],
  conflicts: $ => [[$.a, $.b], [$.b, $.c]],
  rules: {
    source_file: $ => repeat($._item),
    _item: $ => choice($.a, $.b, $.c),
    a: $ => 'a',
    b: $ => 'b',
    c: $ => 'c',
    comment: $ => token(seq('//', /.*/)),
  }
});
"#;

    #[test]
    fn se_leen_las_cuentas_de_la_gramatica() {
        let (info, _) = analizar(BASE).unwrap();
        assert_eq!(info.name, "mini");
        assert_eq!(info.total_rules, 6);
        assert_eq!(info.hidden_rules, 1, "solo `_item` empieza con guion bajo");
        assert_eq!(info.declared_conflicts, 2);
        assert_eq!(info.start_rule, "source_file");
    }

    /// **La regla de arranque no está muerta**: en tree-sitter es la
    /// PRIMERA de `rules` y nadie la referencia por diseño.
    #[test]
    fn la_regla_de_arranque_no_se_marca_como_muerta() {
        let (_, smells) = analizar(BASE).unwrap();
        assert!(!smells.iter().any(|s| s.name == "source_file"), "{:?}", smells.iter().map(|s| &s.name).collect::<Vec<_>>());
    }

    /// **Una regla usada solo en `extras` no está muerta.** `comment` no
    /// aparece en ninguna otra regla, pero `extras` la referencia.
    #[test]
    fn una_regla_usada_solo_en_extras_no_esta_muerta() {
        let (_, smells) = analizar(BASE).unwrap();
        assert!(!smells.iter().any(|s| s.name == "comment"), "{:?}", smells.iter().map(|s| &s.name).collect::<Vec<_>>());
    }

    #[test]
    fn una_regla_que_nadie_referencia_se_marca() {
        let src = BASE.replace("c: $ => 'c',", "c: $ => 'c',\n    huerfana: $ => 'x',");
        let (_, smells) = analizar(&src).unwrap();
        let h = smells.iter().find(|s| s.name == "huerfana").expect("sin hallazgo");
        assert_eq!(h.kind, "unreferenced_grammar_rule");
        assert!(h.message.contains("no rompe nada"), "{}", h.message);
    }

    /// Referenciar una regla inexistente **hace fallar `tree-sitter
    /// generate`** — no es estilo.
    #[test]
    fn una_referencia_a_una_regla_inexistente_se_marca() {
        let src = BASE.replace("c: $ => 'c',", "c: $ => $.no_existe,");
        let (_, smells) = analizar(&src).unwrap();
        let h = smells.iter().find(|s| s.name == "no_existe").expect("sin hallazgo");
        assert_eq!(h.kind, "undefined_grammar_rule");
        assert!(h.message.contains("generate` falla"), "{}", h.message);
    }

    /// **La recursión por izquierda NO se reporta**, y es deliberado: el
    /// parser de tree-sitter es GLR y la acepta. Marcarla sería aplicar la
    /// regla de otro formalismo.
    #[test]
    fn la_recursion_por_izquierda_no_se_reporta() {
        let src = r#"module.exports = grammar({
  name: 'r',
  rules: {
    source_file: $ => $.expr,
    expr: $ => choice(seq($.expr, '+', $.expr), 'x'),
  }
});
"#;
        let (_, smells) = analizar(src).unwrap();
        assert!(smells.is_empty(), "{:?}", smells.iter().map(|s| s.kind).collect::<Vec<_>>());
    }

    /// Un `.js` que no es una gramática no produce nada — ni un resultado
    /// vacío, sino `None`.
    #[test]
    fn un_js_cualquiera_no_es_una_gramatica() {
        assert!(analizar("function f() { return 1 }\n").is_none());
        // Una llamada a algo que se llame `grammar` pero sin `rules` tampoco.
        assert!(analizar("const x = grammar({ name: 'x' });\n").is_none());
    }

    /// `grammar(base, {...})` es herencia de gramática: interesa el último
    /// objeto, que trae las reglas propias.
    #[test]
    fn una_gramatica_que_hereda_toma_el_objeto_de_sus_reglas() {
        let src = "module.exports = grammar(require('otra'), {\n  name: 'hija',\n  rules: { source_file: $ => 'x' }\n});\n";
        let (info, _) = analizar(src).unwrap();
        assert_eq!(info.name, "hija");
        assert_eq!(info.total_rules, 1);
    }

    /// Las claves con comillas también son nombres de regla válidos.
    #[test]
    fn una_clave_entre_comillas_se_lee_igual() {
        let src = "module.exports = grammar({\n  name: 'q',\n  rules: { 'source_file': $ => 'x' }\n});\n";
        assert_eq!(analizar(src).unwrap().0.start_rule, "source_file");
    }


    /// **El falso positivo que encontró correr sobre gramáticas reales**:
    /// los `externals` están definidos aunque no estén en `rules`. Los
    /// produce un escáner externo en C.
    #[test]
    fn los_tokens_externos_no_son_referencias_rotas() {
        let src = "module.exports = grammar({\n  name: 'x',\n  externals: $ => [$.string_content],\n  rules: {\n    source_file: $ => $.string_content,\n  }\n});\n";
        let (info, smells) = analizar(src).unwrap();
        assert_eq!(info.external_tokens, 1);
        assert!(smells.is_empty(), "{:?}", smells.iter().map(|s| (&s.kind, &s.name)).collect::<Vec<_>>());
    }

    /// **Y el segundo falso positivo**: con un spread, parte de las reglas
    /// las fabrica una función y no se pueden leer del texto — así que los
    /// dos hallazgos dejan de ser afirmables y se calla.
    #[test]
    fn con_reglas_generadas_no_se_afirma_nada() {
        let src = "module.exports = grammar({\n  name: 'x',\n  rules: {\n    source_file: $ => $.generada,\n    ...preprocIf('', $ => $.source_file),\n  }\n});\n";
        let (info, smells) = analizar(src).unwrap();
        assert!(info.has_generated_rules);
        assert!(smells.is_empty(), "no se acusa lo que no se puede leer: {:?}", smells.len());
    }

    /// Y sin spread, la referencia rota SÍ se sigue marcando: el silencio
    /// es condicional, no general.
    #[test]
    fn sin_reglas_generadas_la_referencia_rota_se_sigue_marcando() {
        let src = "module.exports = grammar({\n  name: 'x',\n  rules: {\n    source_file: $ => $.no_existe,\n  }\n});\n";
        let (_, smells) = analizar(src).unwrap();
        assert_eq!(smells.len(), 1);
        assert_eq!(smells[0].kind, "undefined_grammar_rule");
    }


    /// **La tercera fuente de falsos positivos**: el segundo argumento de
    /// `alias` INVENTA un nombre de nodo, no referencia una regla.
    #[test]
    fn el_nombre_inventado_por_un_alias_no_es_una_referencia_rota() {
        let src = "module.exports = grammar({\n  name: 'x',\n  rules: {\n    source_file: $ => alias($.identificador, $.nombre_de_campo),\n    identificador: $ => /[a-z]+/,\n  }\n});\n";
        let (_, smells) = analizar(src).unwrap();
        assert!(smells.is_empty(), "{:?}", smells.iter().map(|s| (&s.kind, &s.name)).collect::<Vec<_>>());
    }


    /// **El filtro de texto corre antes que el parser**, para no parsear
    /// dos veces cada `.js` del proyecto.
    #[test]
    fn el_filtro_de_texto_descarta_sin_parsear() {
        assert!(analyze_source("function f() { return 1 }\n").is_none());
        assert!(analyze_source("const rules: 1\n").is_none(), "hace falta tambien `grammar(`");
        assert!(analyze_source(BASE).is_some());
    }

    /// **"Se come su propia comida"**, literal: corre sobre las mismas
    /// gramáticas que el motor usa para parsear. No es un assert fijo — las
    /// versiones del registro cambian —, es una medición que imprime.
    #[test]
    fn se_come_su_propia_comida() {
        let base = "C:/Users/PC/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/";
        for g in ["tree-sitter-rust-0.23.3", "tree-sitter-c-0.23.4", "tree-sitter-fortran-0.6.0", "tree-sitter-javascript-0.23.1"] {
            let ruta = format!("{base}{g}/grammar.js");
            let Ok(src) = std::fs::read_to_string(&ruta) else {
                eprintln!("G SALTEADA {g}");
                continue;
            };
            let mut p = Parser::new();
            p.set_language(&tree_sitter_javascript::LANGUAGE.into()).unwrap();
            let t = p.parse(&src, None).unwrap();
            match analyze(t.root_node(), &src) {
                Some((info, smells)) => {
                    eprintln!(
                        "G {:12} reglas={:4} ocultas={:4} conflicts={:3} supertypes={:3} arranque={:16} hallazgos={}",
                        info.name, info.total_rules, info.hidden_rules, info.declared_conflicts, info.supertypes, info.start_rule, smells.len()
                    );
                    for s in smells.iter().take(4) {
                        eprintln!("     {} -> {}", s.kind, s.name);
                    }
                }
                None => eprintln!("G {g}: NO reconocida como gramatica"),
            }
        }
    }
}
