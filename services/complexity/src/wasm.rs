//! WASM/hot-path hints — puerto de `static_parser.py::_wasm_hints_python`
//! (Fase 18): la última pieza que el docstring de `rich.rs` marcaba como
//! "genuinamente Python, Rust todavía no tiene ese heurístico". Pura
//! post-computación sobre `RichFunction` (big_o/complexity/loc/name/line ya
//! calculados) — mismo tipo de "agregación, no un análisis nuevo" que
//! `build_call_graph`. El fallback sin sidecar (`_skeleton_functions_python`)
//! no reimplementa esto: como el resto de los heurísticos que no son la
//! excepción deliberada de call_graph, `wasm_hints` degrada a `[]` cuando no
//! hay sidecar, documentado, no silenciado.

use serde::Serialize;

use crate::rich::RichFunction;

const CC_THRESHOLD: u32 = 5;
const LOC_THRESHOLD: usize = 30;
const BIGO_HOT: &[&str] = &["O(n²)", "O(n³)", "O(2^n)"];
const NUMERIC_KEYWORDS: &[&str] = &[
    "sort", "search", "compute", "calc", "matrix", "multiply", "fft", "transform", "encode", "decode", "hash", "compress", "convolve",
];

#[derive(Serialize, Clone)]
pub struct WasmHint {
    pub function: String,
    pub line: usize,
    pub priority: i32,
    pub reasons: Vec<String>,
    pub recommendation: String,
    pub estimated_speedup: String,
}

/// Equivalente a los `\b...\b` de la regex de Python (`cimport`/`cdef`/
/// `cpdef`) — una coincidencia cuenta solo si los caracteres a los lados (si
/// existen) no son alfanuméricos ni `_`, para no confundir un substring
/// dentro de otro identificador con la palabra real.
fn contains_word(haystack: &str, word: &str) -> bool {
    let bytes = haystack.as_bytes();
    let wlen = word.len();
    let is_word_char = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    haystack.match_indices(word).any(|(i, _)| {
        let before_ok = i == 0 || !is_word_char(bytes[i - 1]);
        let after_ok = i + wlen >= bytes.len() || !is_word_char(bytes[i + wlen]);
        before_ok && after_ok
    })
}

fn recommendation(name: &str, big_o: &str, has_cython: bool) -> String {
    if has_cython {
        return format!("Ya usas Cython — añade 'cdef' a '{name}' con tipos C para compilar a .so");
    }
    match big_o {
        "O(n²)" | "O(n³)" => format!(
            "'{name}' es un hot path crítico. Opciones:\n  1. Cython: cdef double {name}(int n) — compila a .so nativo\n  2. NumPy vectorización — elimina loops Python\n  3. Emscripten → .wasm si necesitas correrlo en browser"
        ),
        "O(2^n)" => format!(
            "'{name}' tiene complejidad exponencial. Antes de WASM, considera:\n  1. Memoización con @functools.lru_cache\n  2. Programación dinámica\n  3. Si aún necesitas velocidad: Cython + tipos estáticos"
        ),
        _ => format!("'{name}' puede beneficiarse de Cython:\n  Agrega 'cdef' al archivo .pyx y compila con: python setup.py build_ext --inplace"),
    }
}

fn estimated_speedup(big_o: &str, complexity: u32) -> &'static str {
    match big_o {
        "O(n³)" | "O(2^n)" => "10-100x (crítico)",
        "O(n²)" => "5-30x",
        _ if complexity >= 10 => "2-10x",
        _ => "1.5-3x",
    }
}

pub fn wasm_hints(functions: &[RichFunction], content: &str) -> Vec<WasmHint> {
    let has_cython = contains_word(content, "cimport") || contains_word(content, "cdef") || contains_word(content, "cpdef");

    let mut hints: Vec<WasmHint> = Vec::new();
    for f in functions {
        let mut reasons = Vec::new();
        let mut priority = 0;

        if BIGO_HOT.contains(&f.big_o.as_str()) {
            reasons.push(format!("Complejidad {} — candidato a optimización WASM", f.big_o));
            priority += 3;
        }
        if f.complexity >= CC_THRESHOLD {
            reasons.push(format!("Complejidad ciclomática alta ({})", f.complexity));
            priority += 2;
        }
        if f.loc >= LOC_THRESHOLD {
            reasons.push(format!("Función grande ({} líneas)", f.loc));
            priority += 1;
        }
        let name_lower = f.name.to_lowercase();
        if NUMERIC_KEYWORDS.iter().any(|kw| name_lower.contains(kw)) {
            reasons.push("Nombre sugiere operación numérica intensiva".to_string());
            priority += 2;
        }

        if !reasons.is_empty() {
            hints.push(WasmHint {
                function: f.name.clone(),
                line: f.line,
                priority,
                reasons,
                recommendation: recommendation(&f.name, &f.big_o, has_cython),
                estimated_speedup: estimated_speedup(&f.big_o, f.complexity).to_string(),
            });
        }
    }

    if content.contains("import.wasm") || content.contains(".wasm") {
        hints.push(WasmHint {
            function: "<module>".to_string(),
            line: 1,
            priority: 5,
            reasons: vec!["Archivo usa módulos .wasm directamente".to_string()],
            recommendation: "Asegúrate de que los bindings WASM están tipados correctamente".to_string(),
            estimated_speedup: "N/A".to_string(),
        });
    }

    hints.sort_by_key(|h| std::cmp::Reverse(h.priority));
    hints
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rich::analyze_rich;

    fn functions_of(src: &str) -> Vec<RichFunction> {
        analyze_rich(src).functions
    }

    #[test]
    fn contains_word_respeta_limites_de_palabra() {
        assert!(contains_word("x = cdef", "cdef"));
        assert!(contains_word("cdef double f()", "cdef"));
        assert!(!contains_word("mycdefvar = 1", "cdef"));
        assert!(!contains_word("cdefine = 1", "cdef"));
    }

    #[test]
    fn funcion_o_n2_dispara_hint_con_prioridad() {
        let src = "def f(items):\n    for a in items:\n        for b in items:\n            pass\n";
        let hints = wasm_hints(&functions_of(src), src);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].function, "f");
        assert!(hints[0].priority >= 3);
    }

    #[test]
    fn funcion_o1_sin_nombre_numerico_no_dispara() {
        let src = "def add(a, b):\n    return a + b\n";
        let hints = wasm_hints(&functions_of(src), src);
        assert!(hints.is_empty());
    }

    #[test]
    fn nombre_numerico_dispara_aunque_sea_o1() {
        let src = "def compute_total(a, b):\n    return a + b\n";
        let hints = wasm_hints(&functions_of(src), src);
        assert_eq!(hints.len(), 1);
        assert!(hints[0].reasons.iter().any(|r| r.contains("numérica")));
    }

    #[test]
    fn hints_ordenados_por_prioridad_descendente() {
        let src = "def hash_value(a, b):\n    return a + b\n\ndef compute_matrix(items):\n    for a in items:\n        for b in items:\n            for c in items:\n                pass\n";
        let hints = wasm_hints(&functions_of(src), src);
        assert!(hints.len() >= 2);
        for w in hints.windows(2) {
            assert!(w[0].priority >= w[1].priority);
        }
    }

    #[test]
    fn deteccion_de_modulo_wasm_en_contenido() {
        let src = "import numpy\nx = load('mod.wasm')\n";
        let hints = wasm_hints(&functions_of(src), src);
        assert!(hints.iter().any(|h| h.function == "<module>"));
    }

    #[test]
    fn recomendacion_menciona_cython_si_ya_se_usa() {
        let src = "cdef int square(int n):\n    return n * n\n";
        // rustpython no parsea sintaxis Cython real — se usa contenido crudo
        // solo para el chequeo de `has_cython`, no para el AST de funciones.
        let py_src = "def compute(a, b):\n    for x in range(a):\n        for y in range(b):\n            pass\n";
        let content_with_cython = format!("{src}\n{py_src}");
        let hints = wasm_hints(&functions_of(py_src), &content_with_cython);
        assert!(hints.iter().any(|h| h.recommendation.contains("Cython")));
    }
}
