//! Fase 24 (Extensibility Platform) — manifest + interfaz de capacidades.
//! Puramente declarativo a propósito: un manifest describe QUÉ hace un
//! plugin y QUÉ necesita, no vive dentro de su implementación — por eso este
//! módulo no depende de `cparse.rs`/`jsts.rs`/`fparse.rs`/`rich.rs`, solo
//! describe sus 7 "plugins" built-in (python/c/cpp/javascript/typescript/
//! fortran/assembly) con la misma info que antes vivía hardcodeada en un dict de
//! Python (`routers/static_analysis.py::/languages`). Nada de sandboxing,
//! trust model, ni registro de terceros acá — eso es deliberadamente lo
//! próximo, no esto (ver el bullet "explicitly deferred" de la Fase 24).
//!
//! **Plugin vs. Extension** (Fase 24, tercer bullet — nombrando una costura
//! que ya existía de manera informal, sin arquitectura nueva):
//! - Un **plugin** es lo que este módulo describe: agrega capacidad de
//!   análisis (un lenguaje, una regla, un tipo de finding) e implementa la
//!   interfaz de arriba (`PluginManifest`/`Capability`). Los 7 de acá son
//!   los únicos que existen hoy — no hay todavía ningún camino para que un
//!   tercero registre uno propio (eso es lo "explicitly deferred").
//! - Una **extension** agrega UI, no capacidad de análisis: consume la
//!   salida de uno o más plugins sobre el mismo shape JSON que cualquier
//!   panel ya lee, sin necesitar esta interfaz. `apps/web/src/panels/
//!   static.ts` (consume los 7 manifests de arriba) y `problems.ts` (la
//!   Fase 12 ya la nombraba como el ejemplo — ver su propio comentario de
//!   cabecera) son extensions en este sentido. No hay ningún tipo nuevo en
//!   TypeScript para esto — es un nombre para algo que ya era así.

use serde::Serialize;

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum PluginCategory {
    Language,
}

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Source,
    Ast,
    Metrics,
    CallGraph,
}

#[derive(Serialize, Clone)]
pub struct PluginManifest {
    pub id: &'static str,
    pub category: PluginCategory,
    pub needs: &'static [Capability],
    pub extensions: &'static [&'static str],
    pub parser: &'static str,
    pub features: &'static [&'static str],
    /// `true` para los 7 plugins de Sythrall — el campo que distinguiría un
    /// plugin de tercero más adelante (Fase 24, bullets deferred). Hoy
    /// siempre `true`: no existe todavía ningún camino para registrar otra cosa.
    pub builtin: bool,
}

const PYTHON_FEATURES: &[&str] =
    &["functions", "classes", "imports", "big_o", "cyclomatic_complexity", "dead_code", "call_graph", "wasm_hints"];
const C_FEATURES: &[&str] = &["functions", "structs", "includes", "macros", "big_o", "call_graph", "wasm_hints"];
const CPP_FEATURES: &[&str] = &["functions", "classes", "includes", "macros", "big_o", "call_graph", "wasm_hints"];
const JS_FEATURES: &[&str] = &["functions", "classes", "imports", "exports", "big_o", "dead_code", "call_graph", "wasm_hints"];
const TS_FEATURES: &[&str] =
    &["functions", "classes", "imports", "exports", "interfaces", "types", "big_o", "dead_code", "call_graph", "wasm_hints"];
const FORTRAN_FEATURES: &[&str] = &[
    "functions",
    "subroutines",
    "do_loop_depth",
    "vectorization_candidates",
    "numerical_algorithm_shape",
    "blas_lapack_usage",
    "big_o",
    "call_graph",
];
const ASSEMBLY_FEATURES: &[&str] = &["procedures", "instructions", "registers_used", "big_o", "call_graph"];

/// Los 7 plugins built-in de Sythrall — mismo contenido que el dict
/// hardcodeado que reemplazan en `routers/static_analysis.py::/languages`,
/// portado literal (no se inventan features nuevas acá). La entrada
/// `"python"` corrige una inexactitud real que tenía ese dict: dice
/// "Python ast (stdlib)" desde antes de la Fase 18, pero el parseo real hoy
/// pasa por `rustpython-parser` en este mismo sidecar — el `ast` de stdlib
/// solo corre en el esqueleto de fallback sin sidecar
/// (`_skeleton_functions_python` en `static_parser.py`).
pub fn builtin_manifests() -> Vec<PluginManifest> {
    vec![
        PluginManifest {
            id: "python",
            category: PluginCategory::Language,
            needs: &[Capability::Source, Capability::Ast],
            extensions: &[".py"],
            parser: "rustpython-parser (Rust sidecar) — Python ast (stdlib) como fallback sin sidecar",
            features: PYTHON_FEATURES,
            builtin: true,
        },
        PluginManifest {
            id: "c",
            category: PluginCategory::Language,
            needs: &[Capability::Source, Capability::Ast],
            extensions: &[".c"],
            parser: "tree-sitter-c (Rust sidecar)",
            features: C_FEATURES,
            builtin: true,
        },
        PluginManifest {
            id: "cpp",
            category: PluginCategory::Language,
            needs: &[Capability::Source, Capability::Ast],
            extensions: &[".cpp", ".cc", ".cxx", ".hpp", ".h"],
            parser: "tree-sitter-cpp (Rust sidecar)",
            features: CPP_FEATURES,
            builtin: true,
        },
        PluginManifest {
            id: "javascript",
            category: PluginCategory::Language,
            needs: &[Capability::Source],
            extensions: &[".js", ".jsx"],
            parser: "regex + AST-like (Rust sidecar)",
            features: JS_FEATURES,
            builtin: true,
        },
        PluginManifest {
            id: "typescript",
            category: PluginCategory::Language,
            needs: &[Capability::Source],
            extensions: &[".ts", ".tsx"],
            parser: "regex + AST-like (Rust sidecar)",
            features: TS_FEATURES,
            builtin: true,
        },
        PluginManifest {
            id: "fortran",
            category: PluginCategory::Language,
            needs: &[Capability::Source, Capability::Ast, Capability::CallGraph],
            extensions: &[".f", ".f90", ".f95", ".f03", ".f08", ".for"],
            parser: "tree-sitter-fortran (Rust sidecar)",
            features: FORTRAN_FEATURES,
            builtin: true,
        },
        PluginManifest {
            id: "assembly",
            category: PluginCategory::Language,
            needs: &[Capability::Source],
            extensions: &[".s", ".asm"],
            parser: "pattern-matching AT&T/Intel (Rust sidecar) — no es un disassembler real, ver Fase 19",
            features: ASSEMBLY_FEATURES,
            builtin: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_manifests_devuelve_los_7_lenguajes() {
        let manifests = builtin_manifests();
        assert_eq!(manifests.len(), 7);
        let ids: Vec<&str> = manifests.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec!["python", "c", "cpp", "javascript", "typescript", "fortran", "assembly"]);
    }

    #[test]
    fn cada_manifest_tiene_extensiones_y_features_no_vacias() {
        for m in builtin_manifests() {
            assert!(!m.extensions.is_empty(), "{} sin extensiones", m.id);
            assert!(!m.features.is_empty(), "{} sin features", m.id);
            assert!(m.builtin);
        }
    }

    #[test]
    fn fortran_incluye_todas_las_extensiones_reconocidas() {
        let manifests = builtin_manifests();
        let fortran = manifests.iter().find(|m| m.id == "fortran").unwrap();
        for ext in [".f", ".f90", ".f95", ".f03", ".f08", ".for"] {
            assert!(fortran.extensions.contains(&ext), "falta {ext}");
        }
    }

    #[test]
    fn ninguna_extension_se_repite_entre_lenguajes() {
        let manifests = builtin_manifests();
        let mut seen = std::collections::HashSet::new();
        for m in &manifests {
            for ext in m.extensions {
                assert!(seen.insert(*ext), "extensión {ext} declarada por más de un plugin");
            }
        }
    }

    #[test]
    fn serializa_a_json_con_snake_case() {
        let manifests = builtin_manifests();
        let json = serde_json::to_value(&manifests[0]).unwrap();
        assert_eq!(json["category"], "language");
        assert_eq!(json["id"], "python");
    }
}
