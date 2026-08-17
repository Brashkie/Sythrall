use rustpython_parser::ast::Suite;
use rustpython_parser::{Parse, ParseError};

/// Parsea un módulo Python completo. Envuelve el error de rustpython-parser en
/// un String legible en vez de propagar su tipo (el caller solo necesita
/// mostrarlo, no inspeccionarlo).
pub fn parse_module(content: &str) -> Result<Suite, String> {
    Suite::parse(content, "<sythrall>").map_err(|e: ParseError| e.to_string())
}

/// Traduce un offset de bytes (lo que devuelve rustpython-ast en `range()`) a
/// un número de línea 1-based. rustpython-ast guarda posiciones como offsets,
/// no como línea/columna — se resuelve acá mismo en vez de sumar una
/// dependencia extra solo para esto.
pub fn line_of_offset(content: &str, offset: usize) -> usize {
    content
        .as_bytes()
        .iter()
        .take(offset.min(content.len()))
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}
