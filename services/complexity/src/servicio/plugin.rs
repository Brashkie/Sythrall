//! Fase 24 (Extensibility Platform) — manifest + interfaz de capacidades.
//! Puramente declarativo a propósito: un manifest describe QUÉ hace un
//! plugin y QUÉ necesita, no vive dentro de su implementación — por eso este
//! módulo no depende de `cparse.rs`/`jsts.rs`/`fparse.rs`/`rich.rs`, solo
//! describe sus 11 built-in — 10 de lenguaje (python/c/cpp/javascript/
//! typescript/fortran/assembly/notebook/css/html) y 1 de formato
//! (`manifest`) — con la misma info que antes vivía hardcodeada en un dict de
//! Python (`routers/static_analysis.py::/languages`). Nada de sandboxing,
//! trust model, ni registro de terceros acá — eso es deliberadamente lo
//! próximo, no esto (ver el bullet "explicitly deferred" de la Fase 24).
//!
//! **Plugin vs. Extension** (Fase 24, tercer bullet — nombrando una costura
//! que ya existía de manera informal, sin arquitectura nueva):
//! - Un **plugin** es lo que este módulo describe: agrega capacidad de
//!   análisis (un lenguaje, una regla, un tipo de finding) e implementa la
//!   interfaz de arriba (`PluginManifest`/`Capability`). Los 8 de acá son
//!   los únicos que existen hoy — no hay todavía ningún camino para que un
//!   tercero registre uno propio (eso es lo "explicitly deferred").
//! - Una **extension** agrega UI, no capacidad de análisis: consume la
//!   salida de uno o más plugins sobre el mismo shape JSON que cualquier
//!   panel ya lee, sin necesitar esta interfaz. `apps/web/src/panels/
//!   static.ts` (consume los 8 manifests de arriba) y `problems.ts` (la
//!   Fase 12 ya la nombraba como el ejemplo — ver su propio comentario de
//!   cabecera) son extensions en este sentido. No hay ningún tipo nuevo en
//!   TypeScript para esto — es un nombre para algo que ya era así.
//!
//! **Modelo de permisos, SOLO schema** (2026-09-01) — `Permission`/
//! `GrantLevel`/`PermissionGrant` de abajo, y el nuevo campo
//! `PluginManifest::permissions`. Deliberadamente sin UI de revisión y sin
//! ningún enforcement real todavía: ningún plugin existente (los 7
//! built-in de acá) pide nada más allá de `read_project`/`analyze_files` —
//! el modelo sandboxless de hoy ya alcanza para eso. Esto es pura
//! infraestructura declarativa, escrita ahora porque un futuro Connector
//! (SQL Server/GitHub/S3, Fase 24) va a ser el primer plugin en la
//! historia de Sythrall que necesite pedir `network`/`filesystem`, y ese
//! es el momento real de construir la UI de revisión/enforcement — no
//! antes. Mismo criterio que `AIProvider` (Fase 27) ya documenta: construir
//! la interfaz completa antes de que exista un consumidor real sería
//! infraestructura especulativa.
//! - `Permission`: distinto de `Capability` de arriba — `Capability`
//!   describe qué necesita un plugin DEL PROPIO pipeline de análisis (¿el
//!   AST? ¿el call graph?), `Permission` describe qué necesita del
//!   SISTEMA/mundo exterior (la pregunta de seguridad real).
//! - `GrantLevel::Sandbox` es su propio nivel, no una variante de
//!   `Allowed` — ejecutar código de un plugin bajo sandbox es una decisión
//!   de infraestructura completamente distinta (todavía sin diseñar,
//!   Fase 30) a simplemente permitirlo sin restricciones.
//! - Un manifest solo enumera los permisos que SÍ pide (`Allowed`/
//!   `Limited`/`Sandbox`) — la ausencia de un permiso significa "no
//!   solicitado", no una entrada `Denied` explícita por cada uno de los 6
//!   posibles. Mismo criterio ya establecido por `needs: &[Capability]`
//!   (solo lista lo que hace falta, no un enum completo con `false` para
//!   el resto).

use serde::Serialize;

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum PluginCategory {
    Language,
    /// Entiende un FORMATO de archivo que los parsers de lenguaje no cubren
    /// (manifests, datos, configuracion). La Fase 24 nombro esta categoria y
    /// quedo vacia hasta que `manifest.rs` la estreno.
    Format,
}

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Source,
    Ast,
    Metrics,
    CallGraph,
}

/// Qué necesita un plugin del SISTEMA/mundo exterior — no del pipeline de
/// análisis (eso es `Capability` de arriba). Ver el doc del módulo para el
/// porqué de agregar esto ahora sin UI/enforcement todavía.
#[derive(Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    ReadProject,
    AnalyzeFiles,
    Network,
    Filesystem,
    ExecuteCode,
    Secrets,
}

/// Mismo shape que la pantalla de permisos de una app de celular — la
/// analogía que el ROADMAP usa explícitamente para este modelo.
#[derive(Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GrantLevel {
    Allowed,
    Denied,
    Limited,
    Sandbox,
}

#[derive(Serialize, Clone, Copy)]
pub struct PermissionGrant {
    pub permission: Permission,
    pub level: GrantLevel,
}

#[derive(Serialize, Clone)]
pub struct PluginManifest {
    pub id: &'static str,
    pub category: PluginCategory,
    pub needs: &'static [Capability],
    pub extensions: &'static [&'static str],
    /// Nombres de archivo EXACTOS que este plugin reclama, cuando la
    /// extension no alcanza para decidir. `package.json` es un `.json` como
    /// cualquier otro: despachar por extension mandaria todo JSON del
    /// proyecto al analizador de manifests. El despacho chequea este campo
    /// antes que `extensions`. Vacio para los 9 plugins de lenguaje.
    pub filenames: &'static [&'static str],
    pub parser: &'static str,
    pub features: &'static [&'static str],
    /// `true` para los 8 plugins de Sythrall — el campo que distinguiría un
    /// plugin de tercero más adelante (Fase 24, bullets deferred). Hoy
    /// siempre `true`: no existe todavía ningún camino para registrar otra cosa.
    pub builtin: bool,
    /// Solo los permisos que este plugin SÍ pide — ver el doc del módulo
    /// arriba para por qué la ausencia de un permiso no lleva una entrada
    /// `Denied` explícita.
    pub permissions: &'static [PermissionGrant],
}

// ── Features de HALLAZGO ──
//
// Las features que nombran un TIPO DE HALLAZGO (no una capacidad de parseo
// como `functions`/`big_o`) son las que deciden si dos análisis del mismo
// proyecto son comparables entre sí: si entre dos corridas un lenguaje
// empieza a emitir `security_findings` donde antes emitía `[]`, el score
// baja sin que el código haya cambiado. Por eso `history.py` guarda la lista
// de features de cada snapshot y avisa cuando difieren.
//
// **Estaban desactualizadas, exactamente igual que el scanner**: `python` no
// declaraba `security_findings` pese a tener taint tracking desde la Fase 21,
// `c`/`cpp` no declaraban `modernization` con 12 patrones ya shippeados, y
// `notebook` sí declaraba `security_findings` — o sea que los manifests se
// contradecían entre ellos. `manifests_declaran_los_hallazgos_que_el_motor_
// produce` fija esto contra un escaneo real, en las dos direcciones.
const FINDING_FEATURES: &[&str] = &[
    "security_findings",
    "structural_smells",
    "naming_smells",
    "control_findings",
    "modernization",
    "css_findings",
    "html_findings",
    "manifest_findings",
];

/// `true` si esta feature nombra un tipo de hallazgo y no una capacidad de
/// parseo — ver el comentario de `FINDING_FEATURES`.
pub fn is_finding_feature(feature: &str) -> bool {
    FINDING_FEATURES.contains(&feature)
}

const PYTHON_FEATURES: &[&str] = &[
    "functions",
    "classes",
    "imports",
    "big_o",
    "cyclomatic_complexity",
    "dead_code",
    "call_graph",
    "wasm_hints",
    "security_findings",
    "structural_smells",
    "naming_smells",
    "control_findings",
    "modernization",
];
const C_FEATURES: &[&str] = &[
    "functions",
    "structs",
    "includes",
    "macros",
    "big_o",
    "call_graph",
    "wasm_hints",
    "security_findings",
    "structural_smells",
    "modernization",
];
const CPP_FEATURES: &[&str] = &[
    "functions",
    "classes",
    "includes",
    "macros",
    "big_o",
    "call_graph",
    "wasm_hints",
    "security_findings",
    "structural_smells",
    "modernization",
    // Solo C++: C no tiene templates, y por eso no esta en `C_FEATURES`.
    "template_complexity",
];
const JS_FEATURES: &[&str] = &[
    "functions",
    "classes",
    "imports",
    "exports",
    "big_o",
    "dead_code",
    "call_graph",
    "wasm_hints",
    "security_findings",
    "structural_smells",
];
const TS_FEATURES: &[&str] = &[
    "functions",
    "classes",
    "imports",
    "exports",
    "interfaces",
    "types",
    "big_o",
    "dead_code",
    "call_graph",
    "wasm_hints",
    "security_findings",
    "structural_smells",
];
/// Ada como target de analisis (Fase 34, Linea E, paso 1). `specs` esta en la
/// lista porque un `.ads` produce cero cuerpos y N firmas, y eso es una
/// capacidad propia: la interfaz del paquete sin inventarle metricas de un
/// cuerpo que no existe. Lo que NO esta: `machine_code` y `spark_proofs`, los
/// pasos 2 y 3, bloqueados por el toolchain ausente -- no se declaran como
/// capacidad hasta que se puedan verificar.
/// D como target de analisis (Fase 34, Linea C, paso 1). `trust_surface` es
/// la capacidad que D aporta y que ningun otro de los doce da: la frontera de
/// confianza graduada en TRES niveles, con el del medio (`@trusted`) nombrado
/// por el lenguaje en vez de por una convencion de comentario.
const D_FEATURES: &[&str] = &[
    "functions",
    "trust_surface",
    "review_surface",
    "attribute_scope_resolution",
    "unittests",
    "imports",
    "loop_depth",
    "cyclomatic_complexity",
    "big_o",
    "call_graph",
    "structural_smells",
];
const ADA_FEATURES: &[&str] = &[
    "functions",
    "procedures",
    "expression_functions",
    "specs",
    "packages",
    "generics",
    "tasks",
    "with_clauses",
    "loop_depth",
    "cyclomatic_complexity",
    "big_o",
    "call_graph",
    "structural_smells",
];
const FORTRAN_FEATURES: &[&str] = &[
    "functions",
    "subroutines",
    "do_loop_depth",
    "vectorization_candidates",
    "numerical_algorithm_shape",
    "blas_lapack_usage",
    "big_o",
    "call_graph",
    "structural_smells",
];
/// Rust como target de analisis (Fase 34, Linea B). Corto a proposito: esta
/// primera porcion trae funciones, `use`s y la superficie de confianza.
/// Declarar features que el motor no produce todavia seria prometer de mas.
const RUST_FEATURES: &[&str] =
    &["functions", "uses", "big_o", "trust_surface", "proof_surface", "structural_smells"];
const ASSEMBLY_FEATURES: &[&str] =
    &["procedures", "instructions", "registers_used", "big_o", "call_graph", "structural_smells"];
/// CSS y HTML son los 2 unicos plugins de lenguaje cuyas features no
/// incluyen `functions`/`big_o`: ninguno de los dos tiene funciones ni
/// complejidad algoritmica. Lo que si tienen —y lo que sus modulos miden—
/// son reglas/selectores/especificidad (CSS) y accesibilidad/estructura del
/// documento (HTML).
const CSS_FEATURES: &[&str] = &[
    "rules",
    "selectors",
    "specificity",
    "duplicate_declaration",
    "duplicate_selector",
    "high_specificity",
    "css_size",
    "css_findings",
];
const HTML_FEATURES: &[&str] = &[
    "accessibility",
    "document_structure",
    "duplicate_id",
    "heading_order",
    "form_labels",
    "html_findings",
];
/// Categoria distinta a la de los lenguajes (`Format`): no analiza codigo,
/// analiza un manifest de dependencias.
const MANIFEST_FEATURES: &[&str] =
    &["dependencies", "version_pinning", "duplicate_dependency", "external_source_dependency", "manifest_findings"];
const NOTEBOOK_FEATURES: &[&str] = &[
    "functions",
    "classes",
    "imports",
    "big_o",
    "cyclomatic_complexity",
    "code_cells",
    "cell_ranges",
    "security_findings",
    "structural_smells",
    "naming_smells",
    "control_findings",
    "modernization",
];

/// Los 8 built-in de hoy son todos Language/Analysis Extensions — ninguno
/// toca la red, el filesystem más allá del propio proyecto, ejecuta código,
/// ni maneja secretos. Compartida entre los 8 en vez de repetida, ya que
/// hoy es literalmente la misma para todos (a diferencia de `needs`, que sí
/// varía por plugin según si usa AST/call graph).
const LANGUAGE_PLUGIN_PERMISSIONS: &[PermissionGrant] = &[
    PermissionGrant { permission: Permission::ReadProject, level: GrantLevel::Allowed },
    PermissionGrant { permission: Permission::AnalyzeFiles, level: GrantLevel::Allowed },
];

/// Los 8 plugins built-in de Sythrall — mismo contenido que el dict
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
            filenames: &[],
            extensions: &[".py"],
            parser: "rustpython-parser (Rust sidecar) — Python ast (stdlib) como fallback sin sidecar",
            features: PYTHON_FEATURES,
            builtin: true,
            permissions: LANGUAGE_PLUGIN_PERMISSIONS,
        },
        PluginManifest {
            id: "c",
            category: PluginCategory::Language,
            needs: &[Capability::Source, Capability::Ast],
            filenames: &[],
            extensions: &[".c"],
            parser: "tree-sitter-c (Rust sidecar)",
            features: C_FEATURES,
            builtin: true,
            permissions: LANGUAGE_PLUGIN_PERMISSIONS,
        },
        PluginManifest {
            id: "cpp",
            category: PluginCategory::Language,
            needs: &[Capability::Source, Capability::Ast],
            filenames: &[],
            extensions: &[".cpp", ".cc", ".cxx", ".hpp", ".h"],
            parser: "tree-sitter-cpp (Rust sidecar)",
            features: CPP_FEATURES,
            builtin: true,
            permissions: LANGUAGE_PLUGIN_PERMISSIONS,
        },
        PluginManifest {
            id: "javascript",
            category: PluginCategory::Language,
            needs: &[Capability::Source],
            filenames: &[],
            extensions: &[".js", ".jsx"],
            parser: "regex + AST-like (Rust sidecar)",
            features: JS_FEATURES,
            builtin: true,
            permissions: LANGUAGE_PLUGIN_PERMISSIONS,
        },
        PluginManifest {
            id: "typescript",
            category: PluginCategory::Language,
            needs: &[Capability::Source],
            filenames: &[],
            extensions: &[".ts", ".tsx"],
            parser: "regex + AST-like (Rust sidecar)",
            features: TS_FEATURES,
            builtin: true,
            permissions: LANGUAGE_PLUGIN_PERMISSIONS,
        },
        PluginManifest {
            id: "fortran",
            category: PluginCategory::Language,
            needs: &[Capability::Source, Capability::Ast, Capability::CallGraph],
            filenames: &[],
            extensions: &[".f", ".f90", ".f95", ".f03", ".f08", ".for"],
            parser: "tree-sitter-fortran (Rust sidecar)",
            features: FORTRAN_FEATURES,
            builtin: true,
            permissions: LANGUAGE_PLUGIN_PERMISSIONS,
        },
        PluginManifest {
            id: "d",
            category: PluginCategory::Language,
            needs: &[Capability::Source, Capability::Ast, Capability::CallGraph],
            filenames: &[],
            extensions: &[".d"],
            parser: "tree-sitter-d (Rust sidecar)",
            features: D_FEATURES,
            builtin: true,
            permissions: LANGUAGE_PLUGIN_PERMISSIONS,
        },
        PluginManifest {
            id: "ada",
            category: PluginCategory::Language,
            needs: &[Capability::Source, Capability::Ast, Capability::CallGraph],
            filenames: &[],
            extensions: &[".adb", ".ads"],
            parser: "tree-sitter-ada (Rust sidecar)",
            features: ADA_FEATURES,
            builtin: true,
            permissions: LANGUAGE_PLUGIN_PERMISSIONS,
        },
        PluginManifest {
            id: "assembly",
            category: PluginCategory::Language,
            needs: &[Capability::Source],
            filenames: &[],
            extensions: &[".s", ".asm"],
            parser: "pattern-matching AT&T/Intel (Rust sidecar) — no es un disassembler real, ver Fase 19",
            features: ASSEMBLY_FEATURES,
            builtin: true,
            permissions: LANGUAGE_PLUGIN_PERMISSIONS,
        },
        PluginManifest {
            id: "rust",
            category: PluginCategory::Language,
            needs: &[Capability::Source, Capability::Ast],
            filenames: &[],
            extensions: &[".rs"],
            parser: "tree-sitter-rust (Rust sidecar)",
            features: RUST_FEATURES,
            builtin: true,
            permissions: LANGUAGE_PLUGIN_PERMISSIONS,
        },
        PluginManifest {
            id: "notebook",
            category: PluginCategory::Language,
            needs: &[Capability::Source, Capability::Ast],
            filenames: &[],
            extensions: &[".ipynb"],
            parser: "extracción de celdas de código (JSON, Rust sidecar) → delega en el mismo motor de análisis de Python (`rich::analyze_rich`) — ver Fase 27",
            features: NOTEBOOK_FEATURES,
            builtin: true,
            permissions: LANGUAGE_PLUGIN_PERMISSIONS,
        },
        PluginManifest {
            id: "css",
            category: PluginCategory::Language,
            // Solo `Source`: `cssparse.rs` trae su propio tokenizador, no
            // depende del AST de ningún otro motor (a diferencia de los 8
            // anteriores, que todos declaran `Ast`).
            needs: &[Capability::Source],
            filenames: &[],
            extensions: &[".css"],
            parser: "tokenizador propio escrito a mano (Rust sidecar, sin dependencia nueva) — Sythrall CSS Engine",
            features: CSS_FEATURES,
            builtin: true,
            permissions: LANGUAGE_PLUGIN_PERMISSIONS,
        },
        PluginManifest {
            id: "html",
            category: PluginCategory::Language,
            needs: &[Capability::Source],
            extensions: &[".html", ".htm"],
            filenames: &[],
            parser: "tokenizador propio escrito a mano (Rust sidecar, sin dependencia nueva) — accesibilidad y estructura, ver Fase 31",
            features: HTML_FEATURES,
            builtin: true,
            permissions: LANGUAGE_PLUGIN_PERMISSIONS,
        },
        PluginManifest {
            id: "manifest",
            category: PluginCategory::Format,
            needs: &[Capability::Source],
            // Sin extensiones: se reclama por NOMBRE, ver el campo de abajo.
            extensions: &[],
            // En minuscula: el despacho de Python busca con
            // `Path(filename).name.lower()`, asi que `Cargo.toml` entra
            // como `cargo.toml`. El parser tambien normaliza.
            filenames: &["package.json", "requirements.txt", "cargo.toml", "pyproject.toml"],
            parser: "parser propio de manifests de dependencias (Rust sidecar) — primera Format Extension, ver Fase 31",
            features: MANIFEST_FEATURES,
            builtin: true,
            // Deliberadamente SIN `network`: esta porcion no consulta CVEs ni
            // el registro. Cuando lo haga, este es el primer manifest que
            // tendria que pedir ese permiso — y sera un cambio visible aca.
            permissions: LANGUAGE_PLUGIN_PERMISSIONS,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_manifests_devuelve_los_13_lenguajes_mas_el_de_formato() {
        let manifests = builtin_manifests();
        assert_eq!(manifests.len(), 14);
        let ids: Vec<&str> = manifests.iter().map(|m| m.id).collect();
        assert_eq!(
            ids,
            vec![
                "python", "c", "cpp", "javascript", "typescript", "fortran", "d", "ada", "assembly",
                "rust", "notebook", "css", "html", "manifest"
            ]
        );
    }

    /// **Lo que Ada NO declara, y por que importa.** Los pasos 2 y 3 de la
    /// Linea E (codigo maquina via GNAT, pruebas de SPARK) estan bloqueados
    /// por el toolchain ausente. Declararlos como capacidad en el manifest
    /// seria afirmar una capacidad que no se puede verificar — el manifest es
    /// lo que el frontend y `/static/languages` muestran como cierto.
    #[test]
    fn ada_no_declara_las_capacidades_que_todavia_no_existen() {
        let ada = builtin_manifests().into_iter().find(|m| m.id == "ada").expect("sin manifest de Ada");
        assert!(!ada.features.contains(&"machine_code"), "{:?}", ada.features);
        assert!(!ada.features.contains(&"spark_proofs"), "{:?}", ada.features);
        assert!(ada.features.contains(&"specs"), "un .ads produce firmas, no cuerpos");
    }

    #[test]
    fn el_plugin_de_formato_reclama_por_nombre_y_los_de_lenguaje_por_extension() {
        for m in builtin_manifests() {
            match m.category {
                PluginCategory::Language => {
                    assert!(!m.extensions.is_empty(), "{} deberia reclamar extensiones", m.id);
                    assert!(m.filenames.is_empty(), "{} no deberia reclamar nombres exactos", m.id);
                }
                PluginCategory::Format => {
                    assert!(!m.filenames.is_empty(), "{} deberia reclamar nombres exactos", m.id);
                }
            }
        }
    }

    #[test]
    fn ningun_nombre_exacto_lo_reclaman_dos_plugins() {
        let mut vistos = std::collections::HashSet::new();
        for m in builtin_manifests() {
            for f in m.filenames {
                assert!(vistos.insert(*f), "`{f}` reclamado por mas de un plugin");
            }
        }
    }

    #[test]
    fn cada_manifest_reclama_algo_y_declara_features() {
        for m in builtin_manifests() {
            // Un plugin tiene que reclamar POR LO MENOS una de las 2 vías —
            // extensión o nombre exacto —, si no nunca se le despacharía
            // nada. Cuál de las 2 depende de la categoría (ver el test de
            // abajo); acá solo se exige que no sea ninguna.
            assert!(
                !m.extensions.is_empty() || !m.filenames.is_empty(),
                "{} no reclama ni extensiones ni nombres de archivo",
                m.id
            );
            assert!(!m.features.is_empty(), "{} sin features", m.id);
            assert!(m.builtin);
        }
    }

    #[test]
    fn cada_manifest_pide_read_project_y_analyze_files_permitidos() {
        for m in builtin_manifests() {
            let has = |perm: Permission| m.permissions.iter().any(|g| g.permission == perm && g.level == GrantLevel::Allowed);
            assert!(has(Permission::ReadProject), "{} no pide read_project", m.id);
            assert!(has(Permission::AnalyzeFiles), "{} no pide analyze_files", m.id);
        }
    }

    #[test]
    fn ningun_builtin_pide_permisos_riesgosos_todavia() {
        // Los 8 plugins de hoy son Language/Analysis Extensions puras — no
        // hay ningún Connector real (Fase 24) que necesite `network`/
        // `execute_code`/`secrets`/`filesystem` todavía.
        for m in builtin_manifests() {
            for risky in [Permission::Network, Permission::ExecuteCode, Permission::Secrets, Permission::Filesystem] {
                assert!(!m.permissions.iter().any(|g| g.permission == risky), "{} pide un permiso riesgoso sin necesitarlo", m.id);
            }
        }
    }

    #[test]
    fn permission_grant_serializa_snake_case() {
        let json = serde_json::to_value(PermissionGrant { permission: Permission::ReadProject, level: GrantLevel::Allowed }).unwrap();
        assert_eq!(json["permission"], "read_project");
        assert_eq!(json["level"], "allowed");
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
