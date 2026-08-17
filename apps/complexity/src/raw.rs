//! Métricas de líneas — reemplaza a `radon.raw.analyze`. loc/blank/comments/
//! multi se calculan escaneando el texto línea por línea (una tokenización
//! completa no aporta más precisión acá, solo más código); lloc (líneas
//! lógicas) se calcula aparte contando statements del AST, que es una medida
//! más fiel que contar líneas físicas de statement.

use rustpython_parser::ast::Stmt;
use serde::Serialize;

#[derive(Serialize, Clone, Default)]
pub struct RawStats {
    pub loc: usize,
    pub lloc: usize,
    pub sloc: usize,
    pub comments: usize,
    pub blank: usize,
    pub multi: usize,
}

pub fn analyze(content: &str, suite: &[Stmt]) -> RawStats {
    let mut stats = RawStats::default();
    let mut in_triple: Option<&str> = None;

    for raw_line in content.lines() {
        stats.loc += 1;
        let line = raw_line.trim();

        if let Some(delim) = in_triple {
            stats.multi += 1;
            // Una línea de cierre puede seguir teniendo el mismo delimitador
            // más de una vez (p.ej. `"""` sola) — alcanza con buscarlo.
            if line.contains(delim) {
                in_triple = None;
            }
            continue;
        }

        if line.is_empty() {
            stats.blank += 1;
            continue;
        }

        if line.starts_with('#') {
            stats.comments += 1;
            continue;
        }

        // Heurística simple para detectar el arranque de un string
        // triple-quoteado que no cierra en la misma línea (docstrings de
        // varias líneas, el caso que de verdad importa para `multi`).
        for delim in ["\"\"\"", "'''"] {
            if let Some(pos) = line.find(delim) {
                let rest = &line[pos + delim.len()..];
                if !rest.contains(delim) {
                    in_triple = Some(delim);
                    break;
                }
            }
        }

        stats.sloc += 1;
    }

    stats.lloc = count_logical_lines(suite);
    stats
}

fn count_logical_lines(body: &[Stmt]) -> usize {
    let mut n = 0;
    for stmt in body {
        n += 1;
        n += nested_body(stmt).map(count_logical_lines).unwrap_or(0);
    }
    n
}

/// Statements compuestos cuentan como 1 línea lógica (el `if`/`for`/`def`
/// mismo) más lo que haya adentro — igual que hace radon.
fn nested_body(stmt: &Stmt) -> Option<&[Stmt]> {
    match stmt {
        Stmt::If(s) => Some(&s.body),
        Stmt::For(s) => Some(&s.body),
        Stmt::AsyncFor(s) => Some(&s.body),
        Stmt::While(s) => Some(&s.body),
        Stmt::With(s) => Some(&s.body),
        Stmt::AsyncWith(s) => Some(&s.body),
        Stmt::FunctionDef(s) => Some(&s.body),
        Stmt::AsyncFunctionDef(s) => Some(&s.body),
        Stmt::ClassDef(s) => Some(&s.body),
        Stmt::Try(s) => Some(&s.body),
        Stmt::TryStar(s) => Some(&s.body),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_module;

    fn stats_of(src: &str) -> RawStats {
        let suite = parse_module(src).expect("parse ok");
        analyze(src, &suite)
    }

    #[test]
    fn cuenta_lineas_fisicas_y_blancos() {
        let src = "x = 1\n\ny = 2\n";
        let s = stats_of(src);
        assert_eq!(s.loc, 3);
        assert_eq!(s.blank, 1);
        assert_eq!(s.sloc, 2);
    }

    #[test]
    fn cuenta_comentarios() {
        let src = "# comentario\nx = 1\n";
        let s = stats_of(src);
        assert_eq!(s.comments, 1);
        assert_eq!(s.sloc, 1);
    }

    #[test]
    fn detecta_docstring_multilinea() {
        let src = "def f():\n    \"\"\"\n    docstring\n    \"\"\"\n    return 1\n";
        let s = stats_of(src);
        assert!(s.multi >= 2);
    }

    #[test]
    fn lloc_cuenta_statements_incluyendo_anidados() {
        // def (1) + if (1) + return (1) + return (1) = 4
        let src = "def f(x):\n    if x:\n        return 1\n    return 0\n";
        let s = stats_of(src);
        assert_eq!(s.lloc, 4);
    }
}
