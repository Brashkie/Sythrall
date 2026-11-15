//! Fase 32 (Sythrall Mathematical Analysis) — lógica proposicional más allá
//! de De Morgan: contradicciones, tautologías, duplicados y absorción.
//!
//! **La mitad ESTRUCTURAL del asunto, y por eso vive aparte de
//! `intervals.rs`.** Aquel prueba que `x > 10 and x < 5` es imposible mirando
//! los VALORES: necesita que `x` sea un entero que el análisis pueda acotar.
//! Éste prueba que `a and not a` es imposible mirando la FORMA, sin saber
//! absolutamente nada de `a` — puede ser un objeto, una lista, lo que sea.
//! Los dos hacen falta y ninguno cubre al otro.
//!
//! **Por qué la pureza no es un detalle, es LA condición.** `f() and not f()`
//! NO es siempre falsa: las dos llamadas pueden devolver cosas distintas.
//! `x and not x` sí lo es, porque leer una variable dos veces dentro de la
//! misma expresión da lo mismo. Por eso solo se miran expresiones sin
//! llamadas, sin walrus, sin `await`/`yield` y sin comprensiones. Queda una
//! concesión, y va declarada acá en vez de escondida: **un acceso a atributo
//! cuenta como puro**, aunque un `@property` con efectos laterales podría
//! romperlo. Es un caso patológico y el costo de excluirlo sería perder
//! `self.activo and not self.activo`, que es justo la forma que aparece.
//!
//! **La igualdad entre operandos es TEXTUAL, a propósito.** Dos operandos se
//! consideran el mismo cuando su texto fuente coincide ignorando espacios. Es
//! deliberadamente conservador: se pierde `a == b` contra `b == a`, que son
//! equivalentes y no se detectan. Pero nunca afirma una igualdad falsa, que
//! es el error que importa — y al ser textual funciona igual sobre
//! `self.cfg.modo[0]` que sobre `x`, sin una tabla de casos por tipo de nodo.
//!
//! **NaN parte la simetría, y el módulo lo respeta.** `x < y` y `x >= y` son
//! complementarias… salvo con NaN, donde las DOS son falsas. Entonces:
//! `x < y and x >= y` sigue siendo una contradicción (falsa por los dos
//! lados), pero `x < y or x >= y` **no** es una tautología. Por eso las
//! comparaciones de orden solo cuentan para contradicción, y las totales
//! (`==`/`!=`, `is`/`is not`, `in`/`not in`, y `not A` contra `A`) para las
//! dos. Confundirlas sería reportar como "siempre verdadera" una condición
//! que con un NaN es falsa.

use rustpython_parser::ast::{BoolOp, CmpOp, Expr, Ranged, Stmt, UnaryOp};

use crate::ast::parser::LineIndex;
use crate::hallazgos::smells::StructuralSmell;
use crate::ast::walk::{walk_expr_tree, walk_stmts_own_scope};

/// Texto fuente tal cual, para los mensajes.
fn texto_crudo<'a>(e: &Expr, source: &'a str) -> &'a str {
    let (a, b) = (e.range().start().to_usize(), e.range().end().to_usize());
    source.get(a..b).unwrap_or("").trim()
}

/// Texto fuente sin espacios, que es la prueba de igualdad del módulo.
fn texto(e: &Expr, source: &str) -> String {
    texto_crudo(e, source).chars().filter(|c| !c.is_whitespace()).collect()
}

/// `true` si evaluar la expresión dos veces en la misma condición da lo mismo.
/// Ver el doc del módulo: es la condición que hace decidible a todo lo demás.
fn es_pura(e: &Expr) -> bool {
    let mut pura = true;
    walk_expr_tree(e, &mut |x: &Expr| {
        if matches!(
            x,
            Expr::Call(_)
                | Expr::NamedExpr(_)
                | Expr::Await(_)
                | Expr::Yield(_)
                | Expr::YieldFrom(_)
                | Expr::ListComp(_)
                | Expr::SetComp(_)
                | Expr::DictComp(_)
                | Expr::GeneratorExp(_)
        ) {
            pura = false;
        }
    });
    pura
}

fn equivalentes(a: &Expr, b: &Expr, source: &str) -> bool {
    let ta = texto(a, source);
    !ta.is_empty() && ta == texto(b, source) && es_pura(a) && es_pura(b)
}

/// Qué tan fuerte es la oposición entre dos condiciones.
#[derive(Clone, Copy, PartialEq)]
enum Oposicion {
    /// Exactamente una de las dos se cumple siempre. Sirve para contradicción
    /// Y para tautología.
    Total,
    /// Nunca las dos a la vez, pero pueden ser las dos falsas — el caso NaN.
    /// Solo sirve para contradicción.
    SoloIncompatible,
}

fn oposicion_de_operadores(a: CmpOp, b: CmpOp) -> Option<Oposicion> {
    let total = matches!(
        (a, b),
        (CmpOp::Eq, CmpOp::NotEq)
            | (CmpOp::NotEq, CmpOp::Eq)
            | (CmpOp::Is, CmpOp::IsNot)
            | (CmpOp::IsNot, CmpOp::Is)
            | (CmpOp::In, CmpOp::NotIn)
            | (CmpOp::NotIn, CmpOp::In)
    );
    if total {
        return Some(Oposicion::Total);
    }
    let orden = matches!(
        (a, b),
        (CmpOp::Lt, CmpOp::GtE)
            | (CmpOp::GtE, CmpOp::Lt)
            | (CmpOp::Gt, CmpOp::LtE)
            | (CmpOp::LtE, CmpOp::Gt)
    );
    orden.then_some(Oposicion::SoloIncompatible)
}

fn oposicion(a: &Expr, b: &Expr, source: &str) -> Option<Oposicion> {
    // `not A` contra `A`. El `not` de Python siempre devuelve el complemento
    // del valor de verdad, así que esto es total incluso con NaN.
    for (x, y) in [(a, b), (b, a)] {
        if let Expr::UnaryOp(u) = y {
            if matches!(u.op, UnaryOp::Not) && equivalentes(x, &u.operand, source) {
                return Some(Oposicion::Total);
            }
        }
    }
    let (Expr::Compare(p), Expr::Compare(q)) = (a, b) else { return None };
    if p.ops.len() != 1 || q.ops.len() != 1 {
        return None; // `a < b < c`: fuera de alcance
    }
    let fuerza = oposicion_de_operadores(p.ops[0], q.ops[0])?;
    (equivalentes(&p.left, &q.left, source)
        && equivalentes(&p.comparators[0], &q.comparators[0], source))
    .then_some(fuerza)
}

fn mismo_operador(a: BoolOp, b: BoolOp) -> bool {
    matches!(a, BoolOp::And) == matches!(b, BoolOp::And)
}

fn revisar_boolop(
    b: &rustpython_parser::ast::ExprBoolOp,
    scope: &str,
    line_index: &LineIndex,
    source: &str,
    out: &mut Vec<StructuralSmell>,
) {
    let es_and = matches!(b.op, BoolOp::And);
    let conector = if es_and { "and" } else { "or" };
    let linea = line_index.line_of(b.range().start().to_usize());
    let mut agregar = |kind: &'static str, message: String| {
        out.push(StructuralSmell { kind, name: scope.to_string(), line: linea, message });
    };

    for i in 0..b.values.len() {
        for j in (i + 1)..b.values.len() {
            let (x, y) = (&b.values[i], &b.values[j]);
            if equivalentes(x, y, source) {
                let t = texto_crudo(x, source);
                agregar(
                    "duplicated_condition",
                    format!(
                        "`{t}` appears twice in the same `{conector}` — the repeat cannot change the result. The usual cause is a copy-paste where the second one was meant to test something else, so the bug is not the redundancy but the check that is missing."
                    ),
                );
                continue;
            }
            let Some(fuerza) = oposicion(x, y, source) else { continue };
            let (tx, ty) = (texto_crudo(x, source), texto_crudo(y, source));
            if es_and {
                agregar(
                    "contradictory_condition",
                    format!(
                        "`{tx}` and `{ty}` cannot both hold, so this `and` is always false and the guarded body never runs. Proven from the shape alone — no value makes both true — and it holds for any type, which is what separates it from the interval analysis: that one needs a bounded integer, this one needs nothing."
                    ),
                );
            } else if fuerza == Oposicion::Total {
                agregar(
                    "tautological_condition",
                    format!(
                        "`{tx}` and `{ty}` cover every case between them, so this `or` is always true and the `else` branch is dead. Only exact opposites count here: `<` and `>=` are NOT treated as covering everything, because with a NaN both are false."
                    ),
                );
            }
        }
    }

    // Absorción: `A or (A and B)` es `A`, y `A and (A or B)` también.
    for (i, externo) in b.values.iter().enumerate() {
        for (j, anidado) in b.values.iter().enumerate() {
            if i == j {
                continue;
            }
            let Expr::BoolOp(interno) = anidado else { continue };
            if mismo_operador(interno.op, b.op) {
                continue;
            }
            if !interno.values.iter().any(|w| equivalentes(externo, w, source)) {
                continue;
            }
            let t = texto_crudo(externo, source);
            let interior = if es_and { "or" } else { "and" };
            agregar(
                "absorbable_condition",
                format!(
                    "`{t} {conector} ({t} {interior} …)` reduces to just `{t}` (absorption law) — whatever the rest of the inner group says, it cannot change the result. Either the outer `{t}` is redundant, or the inner one was meant to be a different check."
                ),
            );
        }
    }
}

/// Punto de entrada. `scope` es el nombre de la función o `<module>`, misma
/// convención que el resto de los productores de `StructuralSmell`.
pub fn propositional_smells(
    scope: &str,
    body: &[Stmt],
    line_index: &LineIndex,
    source: &str,
) -> Vec<StructuralSmell> {
    let mut out = Vec::new();
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |e: &Expr| {
        if let Expr::BoolOp(b) = e {
            revisar_boolop(b, scope, line_index, source, &mut out);
        }
    };
    walk_stmts_own_scope(body, &mut on_stmt, &mut on_expr);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::parse_module;

    fn hallazgos(src: &str) -> Vec<StructuralSmell> {
        let suite = parse_module(src).expect("parse error");
        let idx = LineIndex::build(src);
        propositional_smells("f", &suite, &idx, src)
    }

    fn tiene(src: &str, kind: &str) -> bool {
        hallazgos(src).iter().any(|s| s.kind == kind)
    }

    #[test]
    fn una_contradiccion_no_necesita_saber_nada_del_valor() {
        // `a` puede ser un objeto, una lista, lo que sea: la conjuncion es
        // falsa igual. Eso es lo que `intervals.rs` NO puede hacer.
        assert!(tiene("if a and not a:\n    pass\n", "contradictory_condition"));
        assert!(tiene("if not a and a:\n    pass\n", "contradictory_condition"));
    }

    #[test]
    fn dos_comparaciones_opuestas_tambien_son_una_contradiccion() {
        assert!(tiene("if x == 1 and x != 1:\n    pass\n", "contradictory_condition"));
        assert!(tiene("if y is None and y is not None:\n    pass\n", "contradictory_condition"));
        assert!(tiene("if k in d and k not in d:\n    pass\n", "contradictory_condition"));
    }

    #[test]
    fn una_tautologia_se_detecta() {
        assert!(tiene("if a or not a:\n    pass\n", "tautological_condition"));
        assert!(tiene("if x == 1 or x != 1:\n    pass\n", "tautological_condition"));
    }

    #[test]
    fn nan_impide_llamar_tautologia_a_una_comparacion_de_orden() {
        // Con un NaN, `x < y` y `x >= y` son las DOS falsas: el `or` no cubre
        // todos los casos. Sigue siendo contradiccion del lado del `and`.
        assert!(!tiene("if x < y or x >= y:\n    pass\n", "tautological_condition"));
        assert!(tiene("if x < y and x >= y:\n    pass\n", "contradictory_condition"));
    }

    #[test]
    fn una_llamada_no_es_pura_y_no_se_afirma_nada() {
        // `f()` puede devolver cosas distintas en cada evaluacion.
        assert!(hallazgos("if f() and not f():\n    pass\n").is_empty());
        assert!(hallazgos("if f(1) and f(1):\n    pass\n").is_empty());
    }

    #[test]
    fn un_atributo_si_cuenta_como_puro() {
        // La unica concesion del modulo, declarada en el doc: es la forma que
        // realmente aparece en codigo.
        assert!(tiene("if self.activo and not self.activo:\n    pass\n", "contradictory_condition"));
    }

    #[test]
    fn un_operando_duplicado_se_marca() {
        assert!(tiene("if a and a:\n    pass\n", "duplicated_condition"));
        assert!(tiene("if x > 1 or x > 1:\n    pass\n", "duplicated_condition"));
    }

    #[test]
    fn el_espaciado_no_cambia_la_igualdad() {
        assert!(tiene("if x>1 and x > 1:\n    pass\n", "duplicated_condition"));
    }

    #[test]
    fn la_absorcion_se_detecta_en_las_dos_formas() {
        assert!(tiene("if a or (a and b):\n    pass\n", "absorbable_condition"));
        assert!(tiene("if a and (a or b):\n    pass\n", "absorbable_condition"));
    }

    #[test]
    fn un_grupo_anidado_del_mismo_operador_no_es_absorcion() {
        // `a and (a and b)` es solo un duplicado aplanable, no absorcion.
        assert!(!tiene("if a and (a and b):\n    pass\n", "absorbable_condition"));
    }

    #[test]
    fn una_condicion_normal_no_produce_nada() {
        for src in [
            "if a and b:\n    pass\n",
            "if x > 10 and y < 5:\n    pass\n",
            "if a or (b and c):\n    pass\n",
            "if x == 1 and y != 1:\n    pass\n",
        ] {
            assert!(hallazgos(src).is_empty(), "reporto de mas en: {src}");
        }
    }

    #[test]
    fn un_boolop_anidado_tambien_se_revisa() {
        // El hallazgo esta adentro del parentesis, no en el `and` de afuera.
        assert!(tiene("if c and (a or not a):\n    pass\n", "tautological_condition"));
    }
}
