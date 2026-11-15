//! **Asignaciones muertas: variables que se escriben y no se leen nunca.**
//!
//! Primer corte del análisis de *def-use* — lo que una forma SSA daría
//! gratis con sus φ-nodos, hecho acá sobre el AST directo. `dead_code` ya
//! existía pero cubre **solo imports**; una variable local asignada y jamás
//! leída no la veía nadie.
//!
//! **La aproximación es deliberadamente conservadora, y eso es el diseño.**
//! No se pregunta "¿esta escritura en particular se lee después?" —eso exige
//! orden y flujo, y con bucles y ramas se vuelve sutil rápido— sino
//! "**¿este nombre se lee EN ALGÚN LADO de la función?**". Si la respuesta es
//! no, la escritura es muerta con certeza, sin importar el flujo. La
//! contrapartida es que se pierde el caso de una variable que se pisa antes
//! de leerse (`x = caro(); x = 1; usar(x)`): ahí la primera escritura también
//! es muerta y esto no la ve. **Se prefiere perder hallazgos antes que
//! inventarlos**, que es el mismo criterio del resto del motor.
//!
//! La distinción lectura/escritura no es heurística: el AST de Python marca
//! cada `Name` con su contexto (`Load` / `Store` / `Del`). Es el mismo dato
//! que usa el intérprete para decidir qué hace con ese nombre.
//!
//! **Cinco exclusiones, cada una por un motivo, no por prudencia genérica:**
//!
//! - `_` y los nombres que empiezan con `_`: la convención dice justamente
//!   "esto se descarta a propósito". Marcarlos sería pelear con el lenguaje.
//! - Nombres declarados `global`/`nonlocal`: la lectura puede estar en otra
//!   función, y desde acá no se ve.
//! - Desempaquetado de tuplas (`a, b = f()`): no se puede evitar recibir la
//!   mitad que no se usa sin renombrarla a `_`, así que marcarlo sería pedir
//!   un cambio que no mejora nada.
//! - Atributos y subíndices (`self.x = 1`, `d[k] = v`): no son variables
//!   locales; su lectura puede estar en cualquier parte del programa.
//! - **Funciones que llaman a `eval`, `exec` o `locals()`**: ahí cualquier
//!   nombre puede leerse dinámicamente, así que **no se reporta nada de esa
//!   función**. Sin esta guarda el análisis afirmaría algo que no puede saber.

use std::collections::{HashMap, HashSet};

use rustpython_parser::ast::{Expr, ExprContext, Ranged, Stmt};

use crate::ast::parser::LineIndex;
use crate::hallazgos::smells::StructuralSmell;
use crate::ast::walk::walk_stmts;

/// Llamadas que pueden leer cualquier nombre local sin nombrarlo.
const LECTORES_DINAMICOS: &[&str] = &["eval", "exec", "locals", "vars", "globals"];

/// `true` si la función usa algo que puede leer nombres dinámicamente.
fn tiene_lectura_dinamica(body: &[Stmt]) -> bool {
    let mut hay = false;
    let mut on_expr = |e: &Expr| {
        if hay {
            return;
        }
        if let Expr::Call(c) = e {
            if let Expr::Name(n) = &*c.func {
                if LECTORES_DINAMICOS.contains(&n.id.as_str()) {
                    hay = true;
                }
            }
        }
    };
    let mut on_stmt = |_: &Stmt| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    hay
}

/// Nombres declarados `global` o `nonlocal` en cualquier parte del cuerpo.
fn nombres_compartidos(body: &[Stmt]) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut on_stmt = |s: &Stmt| match s {
        Stmt::Global(g) => out.extend(g.names.iter().map(|n| n.to_string())),
        Stmt::Nonlocal(g) => out.extend(g.names.iter().map(|n| n.to_string())),
        _ => {}
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    out
}

/// `true` si el destino de la asignación es un nombre pelado — no una tupla,
/// un atributo ni un subíndice. Ver el doc del módulo para el porqué.
fn nombre_asignado(target: &Expr) -> Option<&str> {
    match target {
        Expr::Name(n) => Some(n.id.as_str()),
        _ => None,
    }
}

pub fn check_dead_stores(scope: &str, line_index: &LineIndex, body: &[Stmt]) -> Vec<StructuralSmell> {
    if tiene_lectura_dinamica(body) {
        return Vec::new();
    }
    let compartidos = nombres_compartidos(body);

    // Todo nombre que se LEE en algún lado del cuerpo. El contexto lo marca
    // el propio AST, así que esto es exacto y no una heurística de forma.
    let mut leidos: HashSet<String> = HashSet::new();
    let mut on_expr = |e: &Expr| {
        if let Expr::Name(n) = e {
            if matches!(n.ctx, ExprContext::Load) {
                leidos.insert(n.id.to_string());
            }
        }
    };
    let mut on_stmt = |_: &Stmt| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);

    // Las escrituras, con la PRIMERA línea de cada nombre: si una variable se
    // asigna tres veces y no se lee nunca, el hallazgo es uno, en la primera.
    let mut escrituras: HashMap<String, usize> = HashMap::new();
    let mut on_stmt = |s: &Stmt| {
        let (targets, offset): (Vec<&Expr>, usize) = match s {
            Stmt::Assign(a) => (a.targets.iter().collect(), a.range().start().to_usize()),
            // `x: int = 1` cuenta; `x: int` (sin valor) es una declaración de
            // tipo, no una escritura, y no se marca.
            Stmt::AnnAssign(a) if a.value.is_some() => {
                (vec![&a.target], a.range().start().to_usize())
            }
            _ => return,
        };
        for t in targets {
            let Some(nombre) = nombre_asignado(t) else { continue };
            if nombre.starts_with('_') || compartidos.contains(nombre) || leidos.contains(nombre) {
                continue;
            }
            let linea = line_index.line_of(offset);
            escrituras.entry(nombre.to_string()).and_modify(|l| *l = (*l).min(linea)).or_insert(linea);
        }
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);

    let mut out: Vec<StructuralSmell> = escrituras
        .into_iter()
        .map(|(nombre, linea)| StructuralSmell {
            kind: "dead_store",
            name: scope.to_string(),
            line: linea,
            message: format!(
                "`{nombre}` is assigned here and never read anywhere in this scope. Whatever was \
                 computed for it is thrown away — and if that computation has side effects, the \
                 assignment is hiding that the VALUE is unused while the effect still happens, \
                 which reads as an accident. If the discard is deliberate, naming it `_{nombre}` \
                 says so; the convention exists for exactly this. Note this check is \
                 conservative: it only reports names never read at all, so a store that is \
                 overwritten before being read is NOT reported."
            ),
        })
        .collect();
    out.sort_by_key(|s| s.line);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::parse_module;

    fn cuerpo(src: &str) -> (Vec<Stmt>, LineIndex) {
        (parse_module(src).unwrap(), LineIndex::build(src))
    }

    fn marcados(src: &str) -> Vec<String> {
        let (suite, idx) = cuerpo(src);
        check_dead_stores("f", &idx, &suite)
            .into_iter()
            .map(|s| s.message.split('`').nth(1).unwrap_or("?").to_string())
            .collect()
    }

    #[test]
    fn una_variable_asignada_y_nunca_leida_se_marca() {
        assert_eq!(marcados("total = caro()\nreturn 1\n"), vec!["total"]);
    }

    #[test]
    fn una_variable_que_se_lee_no_se_marca() {
        assert!(marcados("total = caro()\nreturn total\n").is_empty());
    }

    /// **La exclusión más importante**: `_` dice explícitamente "esto se
    /// descarta". Marcarlo sería pelear con la convención del lenguaje.
    #[test]
    fn el_guion_bajo_dice_que_el_descarte_es_a_proposito() {
        assert!(marcados("_ = caro()\n_temporal = otro()\nreturn 1\n").is_empty());
    }

    /// **La guarda que evita afirmar lo que no se puede saber**: con `eval`,
    /// `exec` o `locals()` cualquier nombre puede leerse dinámicamente.
    #[test]
    fn con_lectura_dinamica_no_se_reporta_nada() {
        assert!(marcados("total = caro()\nprint(locals())\n").is_empty());
        assert!(marcados("total = caro()\neval(codigo)\n").is_empty());
    }

    #[test]
    fn un_nombre_global_puede_leerse_en_otra_funcion() {
        assert!(marcados("global cache\ncache = caro()\n").is_empty());
    }

    /// El desempaquetado no se marca: no se puede evitar recibir la mitad que
    /// no se usa sin renombrarla, así que pedirlo no mejora nada.
    #[test]
    fn el_desempaquetado_de_tuplas_no_se_marca() {
        assert!(marcados("a, b = f()\nreturn a\n").is_empty());
    }

    #[test]
    fn un_atributo_o_un_subindice_no_son_variables_locales() {
        assert!(marcados("self.x = 1\nd[k] = 2\nreturn 0\n").is_empty());
    }

    /// Una anotación sin valor declara un tipo, no escribe nada.
    #[test]
    fn una_anotacion_sin_valor_no_es_una_escritura() {
        assert!(marcados("total: int\nreturn 1\n").is_empty());
        assert_eq!(marcados("total: int = caro()\nreturn 1\n"), vec!["total"]);
    }

    /// Si se asigna varias veces y nunca se lee, el hallazgo es UNO, en la
    /// primera línea — no uno por escritura.
    #[test]
    fn varias_escrituras_del_mismo_nombre_dan_un_solo_hallazgo() {
        let (suite, idx) = cuerpo("x = 1\nx = 2\nx = 3\nreturn 0\n");
        let s = check_dead_stores("f", &idx, &suite);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].line, 1);
    }

    /// Una lectura adentro de una f-string cuenta como lectura.
    #[test]
    fn una_lectura_en_una_f_string_cuenta() {
        assert!(marcados("nombre = calcular()\nreturn f\"hola {nombre}\"\n").is_empty());
    }

    /// Y el límite que el mensaje declara: una escritura pisada antes de
    /// leerse NO se reporta. Es la contrapartida de ser conservador.
    #[test]
    fn una_escritura_pisada_antes_de_leerse_no_se_reporta_y_eso_esta_dicho() {
        assert!(marcados("x = caro()\nx = 1\nreturn x\n").is_empty());
    }
    /// **Bug preexistente, encontrado midiendo `ssa.rs`.** El walker no
    /// visitaba el TIPO de un `except`, asi que un nombre leido unicamente
    /// ahi no aparecia en `leidos` y esto lo acusaba de nunca leerse.
    #[test]
    fn un_nombre_leido_solo_como_tipo_de_un_except_si_esta_leido() {
        let src = "elegido = ValueError
try:
    importar(m)
except elegido:
    pass
";
        assert!(marcados(src).is_empty(), "{:?}", marcados(src));
    }

    /// Mismo walker, misma clase: la guarda de un `case`.
    #[test]
    fn un_nombre_leido_solo_en_la_guarda_de_un_case_si_esta_leido() {
        let src = "limite = 10
match v:
    case x if x > limite:
        pass
";
        assert!(marcados(src).is_empty(), "{:?}", marcados(src));
    }

}
