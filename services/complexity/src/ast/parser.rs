use rustpython_parser::ast::Suite;
use rustpython_parser::{Parse, ParseError};

/// Parsea un módulo Python completo. Envuelve el error de rustpython-parser en
/// un String legible en vez de propagar su tipo (el caller solo necesita
/// mostrarlo, no inspeccionarlo).
pub fn parse_module(content: &str) -> Result<Suite, String> {
    Suite::parse(content, "<sythrall>").map_err(|e: ParseError| e.to_string())
}

/// Índice de saltos de línea, construido UNA VEZ por archivo — resuelve
/// byte-offset → línea/columna en O(log n) por lookup (búsqueda binaria) en
/// vez de re-escanear desde el principio del archivo en cada llamada.
///
/// `jsts.rs` ya tenía este mismo fix para JS/TS (mismo razonamiento, mismo
/// comentario sobre el benchmark de Criterion que lo hizo evidente: 331ms en
/// 1000 funciones antes del fix, con un salto de 10x en n dando ~80x en
/// tiempo — muy lejos de lineal). Este es ESE mismo patrón generalizado al
/// resto del motor de Python (`symbols.rs`/`structure.rs`/`security.rs`/
/// `naming.rs`/`smells.rs`/`rich.rs`/`complexity.rs`), que hasta esta pasada
/// llamaban a dos funciones sueltas (`line_of_offset`/`column_of_offset`,
/// cada una re-escaneando desde el offset 0) una vez por cada nodo del AST —
/// O(n) por llamada, O(n²) total en un archivo con N funciones/clases/
/// imports proporcional a su propio tamaño.
pub struct LineIndex {
    newline_offsets: Vec<usize>,
    content_len: usize,
}

impl LineIndex {
    pub fn build(content: &str) -> Self {
        let newline_offsets = content.bytes().enumerate().filter(|&(_, b)| b == b'\n').map(|(i, _)| i).collect();
        LineIndex { newline_offsets, content_len: content.len() }
    }

    /// Línea 1-based de un offset de bytes (lo que devuelve rustpython-ast
    /// en `range()`) — mismo comportamiento que la vieja `line_of_offset`
    /// suelta, verificado término a término contra ella antes de borrarla:
    /// cuenta los saltos de línea con offset ESTRICTAMENTE MENOR al dado.
    /// El `.min(content_len)` preserva el clamp defensivo que la función
    /// vieja ya hacía para un offset fuera de rango (no debería pasar en la
    /// práctica — todo offset viene de un nodo del AST del propio
    /// `content` — pero mantiene el mismo piso de seguridad, no lo afloja).
    pub fn line_of(&self, byte_offset: usize) -> usize {
        self.newline_offsets.partition_point(|&nl| nl < byte_offset.min(self.content_len)) + 1
    }

    /// Columna 1-based (offset de bytes desde el último `\n`, o desde el
    /// principio del archivo) — para el Symbol Engine (`symbols.rs`), que
    /// necesita columna además de línea. Mismo criterio que `col_offset` de
    /// Python's `ast` (offset de bytes dentro de la línea, no de
    /// caracteres). Mismo comportamiento que la vieja `column_of_offset`
    /// suelta, incluido el mismo clamp defensivo.
    pub fn column_of(&self, byte_offset: usize) -> usize {
        let byte_offset = byte_offset.min(self.content_len);
        let count_before = self.newline_offsets.partition_point(|&nl| nl < byte_offset);
        let line_start = if count_before == 0 { 0 } else { self.newline_offsets[count_before - 1] + 1 };
        byte_offset - line_start + 1
    }
}
