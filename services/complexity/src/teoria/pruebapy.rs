//! **Fase 34, Línea D — la superficie de prueba de Python, que completa el
//! trío con Rust (`rsparse.rs`) y TypeScript (`jscosto.rs`).**
//!
//! El mismo concepto en los tres: un tipo es una proposición y un valor suyo
//! es su demostración, así que hay lugares donde el programa **tenía una
//! prueba y la tiró**. Lo notable es que **cada lenguaje trae su propio par
//! justificado/sin justificar**, y los tres son la misma idea:
//!
//! ```text
//! Rust         .unwrap()              .expect("por qué no puede fallar")
//! TypeScript   @ts-ignore             @ts-expect-error
//! Python       # type: ignore         # type: ignore[assignment]
//! ```
//!
//! **El de Python es el más específico de los tres.** Un `# type: ignore`
//! pelado calla **cualquier** error de esa línea, para siempre — incluido
//! uno distinto que aparezca años después. `# type: ignore[assignment]`
//! nombra cuál se está callando, así que si mañana surge un error de otra
//! clase en la misma línea, mypy lo reporta igual. No es que uno esté mal y
//! el otro bien: es que uno **sigue avisando** y el otro no.
//!
//! **Los comentarios no están en el AST de rustpython, y eso obliga a un
//! cuidado.** Buscarlos con coincidencia de texto sobre el archivo entero
//! marcaría un `"# type: ignore"` escrito adentro de una cadena. Acá se
//! resuelve exacto: se recolectan del AST los rangos de **todos los literales
//! de cadena** y se buscan los `#` que caen **fuera** de ellos. No es una
//! heurística — es la misma información que tiene el intérprete.

use rustpython_parser::ast::{Constant, Expr, Ranged, Stmt};

use crate::ast::parser::LineIndex;
use crate::hallazgos::smells::StructuralSmell;
use crate::ast::walk::walk_stmts;

/// Dónde un programa Python descarta una prueba que el sistema de tipos
/// tenía. Ver el doc del módulo para la simetría con Rust y TypeScript.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ProofSurfacePy {
    /// `# type: ignore` pelado — calla cualquier error de esa línea, para
    /// siempre, incluido uno distinto que aparezca después.
    pub bare_type_ignores: usize,
    /// `# type: ignore[code]` — nombra cuál se calla, así que sigue avisando
    /// si mañana surge otro en la misma línea. La forma justificada.
    pub coded_type_ignores: usize,
    /// `cast(T, x)` — le dice al verificador "confiá en mí" sin verificar
    /// nada en runtime. Es el `as` de TypeScript.
    pub casts: usize,
    /// Anotaciones `Any`: descartan toda la información de tipo de una vez.
    pub any_annotations: usize,
}

/// Los rangos de bytes de todos los literales de cadena del módulo.
///
/// Es lo que permite encontrar los comentarios sin caer en coincidencia de
/// texto: un `#` adentro de uno de estos rangos es parte de una cadena, no
/// el comienzo de un comentario.
fn rangos_de_cadenas(suite: &[Stmt]) -> Vec<(usize, usize)> {
    let mut rangos = Vec::new();
    let mut on_expr = |e: &Expr| {
        if let Expr::Constant(c) = e {
            if matches!(c.value, Constant::Str(_) | Constant::Bytes(_)) {
                rangos.push((c.range().start().to_usize(), c.range().end().to_usize()));
            }
        }
    };
    let mut on_stmt = |_: &Stmt| {};
    walk_stmts(suite, &mut on_stmt, &mut on_expr);
    rangos
}

/// Los comentarios del archivo, como `(offset, texto)`.
fn comentarios(source: &str, cadenas: &[(usize, usize)]) -> Vec<(usize, String)> {
    let dentro_de_cadena = |off: usize| cadenas.iter().any(|(a, b)| off >= *a && off < *b);
    let mut out = Vec::new();
    let mut offset = 0usize;
    for linea in source.split_inclusive('\n') {
        if let Some(pos) = linea.find('#') {
            let abs = offset + pos;
            if !dentro_de_cadena(abs) {
                out.push((abs, linea[pos..].trim_end().to_string()));
            }
        }
        offset += linea.len();
    }
    out
}

/// `# type: ignore[assignment]` contra `# type: ignore` pelado.
///
/// Lo que separa uno de otro es un `[` inmediatamente después, sin espacio —
/// así lo define mypy, y `# type: ignore [x]` con espacio **no** es un
/// ignore con código para mypy, es uno pelado seguido de texto. Se respeta
/// esa regla en vez de ser permisivo: ser más laxo que la herramienta que
/// interpreta el comentario daría un conteo que no coincide con la realidad.
fn clasificar_ignore(texto: &str) -> Option<bool> {
    let pos = texto.find("type: ignore").or_else(|| texto.find("type:ignore"))?;
    let resto = &texto[pos..];
    let tras = resto.trim_start_matches("type:").trim_start_matches(' ').trim_start_matches("ignore");
    Some(tras.starts_with('['))
}

pub fn proof_surface_py(source: &str, suite: &[Stmt]) -> ProofSurfacePy {
    let mut p = ProofSurfacePy::default();
    for (_, texto) in comentarios(source, &rangos_de_cadenas(suite)) {
        match clasificar_ignore(&texto) {
            Some(true) => p.coded_type_ignores += 1,
            Some(false) => p.bare_type_ignores += 1,
            None => {}
        }
    }

    let mut on_expr = |e: &Expr| {
        let Expr::Call(c) = e else { return };
        // `cast(T, x)` y `typing.cast(T, x)` cuentan igual: mismo criterio de
        // "nombre escrito tal cual" que usa el resto del motor.
        let nombre = match &*c.func {
            Expr::Name(n) => n.id.as_str(),
            Expr::Attribute(a) => a.attr.as_str(),
            _ => return,
        };
        if nombre == "cast" {
            p.casts += 1;
        }
    };
    // Las ANOTACIONES no cuelgan del recorrido de expresiones: viven en
    // `args[i].annotation`, en `returns` y en el `annotation` de un
    // `AnnAssign`. Hay que visitarlas aparte o `Any` no se ve nunca.
    let mut on_stmt = |s: &Stmt| {
        let mut contar = |e: Option<&Expr>| {
            if matches!(e, Some(Expr::Name(n)) if n.id.as_str() == "Any") {
                p.any_annotations += 1;
            }
        };
        match s {
            Stmt::FunctionDef(f) => {
                for a in f.args.args.iter().chain(f.args.posonlyargs.iter()).chain(f.args.kwonlyargs.iter()) {
                    contar(a.def.annotation.as_deref());
                }
                contar(f.returns.as_deref());
            }
            Stmt::AsyncFunctionDef(f) => {
                for a in f.args.args.iter().chain(f.args.posonlyargs.iter()).chain(f.args.kwonlyargs.iter()) {
                    contar(a.def.annotation.as_deref());
                }
                contar(f.returns.as_deref());
            }
            Stmt::AnnAssign(a) => contar(Some(&a.annotation)),
            _ => {}
        }
    };
    walk_stmts(suite, &mut on_stmt, &mut on_expr);
    p
}

/// El hallazgo: un `# type: ignore` pelado, con su alternativa concreta.
pub fn proof_smells_py(source: &str, suite: &[Stmt], line_index: &LineIndex) -> Vec<StructuralSmell> {
    comentarios(source, &rangos_de_cadenas(suite))
        .into_iter()
        .filter(|(_, t)| clasificar_ignore(t) == Some(false))
        .map(|(offset, _)| StructuralSmell {
            kind: "bare_type_ignore",
            name: "<module>".to_string(),
            line: line_index.line_of(offset),
            message:
                "A bare `# type: ignore` silences EVERY type error on this line, for good — \
                 including a different one that shows up years later, on the same line, for an \
                 unrelated reason. `# type: ignore[assignment]` names which error is being \
                 suppressed, so mypy keeps reporting anything else that appears there. Neither \
                 form is wrong; the difference is that one KEEPS TELLING YOU and the other does \
                 not. Same relationship as `.expect(\"why\")` to `.unwrap()` in Rust, and as \
                 `@ts-expect-error` to `@ts-ignore` in TypeScript."
                    .to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::parse_module;

    fn medir(src: &str) -> ProofSurfacePy {
        proof_surface_py(src, &parse_module(src).unwrap())
    }

    #[test]
    fn un_ignore_pelado_y_uno_con_codigo_se_cuentan_por_separado() {
        let src = "x = f()  # type: ignore\ny = g()  # type: ignore[assignment]\n";
        let p = medir(src);
        assert_eq!(p.bare_type_ignores, 1);
        assert_eq!(p.coded_type_ignores, 1);
    }

    /// **La razón entera por la que los comentarios NO se buscan con
    /// coincidencia de texto.** Un `# type: ignore` escrito adentro de una
    /// cadena no es una supresión: es texto. Los rangos de los literales
    /// salen del AST, así que esto es exacto, no una heurística.
    #[test]
    fn un_ignore_adentro_de_una_cadena_no_cuenta() {
        let src = "MENSAJE = \"usa # type: ignore para callarlo\"\n";
        let p = medir(src);
        assert_eq!(p.bare_type_ignores, 0);
        assert_eq!(p.coded_type_ignores, 0);
    }

    #[test]
    fn un_comentario_cualquiera_no_cuenta() {
        assert_eq!(medir("# esto es un comentario normal\nx = 1\n").bare_type_ignores, 0);
    }

    /// mypy exige el `[` pegado: `# type: ignore [x]` con espacio es un
    /// ignore PELADO seguido de texto. Se respeta esa regla en vez de ser
    /// más laxo que la herramienta que lo interpreta.
    #[test]
    fn el_corchete_separado_por_un_espacio_no_es_un_ignore_con_codigo() {
        let p = medir("x = f()  # type: ignore [assignment]\n");
        assert_eq!(p.bare_type_ignores, 1);
        assert_eq!(p.coded_type_ignores, 0);
    }

    #[test]
    fn cast_y_any_se_cuentan() {
        let src = "from typing import Any, cast\n\n\ndef f(x: Any) -> int:\n    return cast(int, x)\n";
        let p = medir(src);
        assert_eq!(p.casts, 1);
        assert!(p.any_annotations >= 1);
    }

    #[test]
    fn el_hallazgo_sale_solo_para_el_pelado_y_da_la_alternativa() {
        let src = "x = f()  # type: ignore\ny = g()  # type: ignore[arg-type]\n";
        let suite = parse_module(src).unwrap();
        let s = proof_smells_py(src, &suite, &LineIndex::build(src));
        assert_eq!(s.len(), 1, "solo el pelado");
        assert_eq!(s[0].kind, "bare_type_ignore");
        assert_eq!(s[0].line, 1);
        assert!(s[0].message.contains("type: ignore[assignment]"), "da la forma concreta");
    }

    #[test]
    fn un_archivo_sin_supresiones_tiene_superficie_vacia() {
        let p = medir("def f(x: int) -> int:\n    return x + 1\n");
        assert_eq!(p.bare_type_ignores, 0);
        assert_eq!(p.casts, 0);
        assert_eq!(p.any_annotations, 0);
    }
}
