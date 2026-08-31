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

/// Columna 1-based (offset de bytes desde el último `\n`, o desde el
/// principio del archivo) — hermano de `line_of_offset`, para el Symbol
/// Engine (`symbols.rs`), que necesita columna además de línea. Mismo
/// criterio que `col_offset` de Python's `ast` (offset de bytes dentro de
/// la línea, no de caracteres) — no hace falta más precisión que esa para
/// go-to-definition/find-references.
pub fn column_of_offset(content: &str, offset: usize) -> usize {
    let offset = offset.min(content.len());
    let line_start = content.as_bytes()[..offset].iter().rposition(|&b| b == b'\n').map(|i| i + 1).unwrap_or(0);
    offset - line_start + 1
}
