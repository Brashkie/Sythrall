//! Fase 25 (Modernization Intelligence) — APIs deprecadas y removidas de
//! Python. Cierra la mitad del bullet "deprecated library/API usage → its
//! current replacement" que sí se puede hacer hoy: la que mira CÓDIGO. La
//! otra mitad (dialectos de SQL viejos, formatos de datos legacy) sigue
//! esperando a las Connectors/Format Extensions de la Fase 24 para poder
//! leer esos artefactos en primer lugar.
//!
//! Hasta acá, `modernization.rs` era el único motor de la fase y solo
//! entendía C/C++ (tree-sitter). El efecto colateral no era "falta una
//! feature": el rollup de proyecto (`modernization_by_file`) agrega sobre
//! TODOS los archivos, así que **un proyecto de puro Python mostraba el
//! informe de modernización vacío**, indistinguible de un proyecto que no
//! tiene nada que modernizar. La misma forma del "score perfecto engañoso"
//! que ya apareció tres veces en este repo.
//!
//! Emite el MISMO shape de candidato que `modernization.rs` (`variable`/
//! `pattern`/`line`/`current`/`suggested_target`/`reasoning`/`confidence`),
//! así que la agregación de `static_analysis.py` y el renderer del panel
//! funcionan sin tocarse — el único cambio de frontend son las etiquetas de
//! los `suggested_target` nuevos.
//!
//! **El límite central, y está en el `note` que ve el usuario: este módulo
//! no sabe a qué versión de Python apunta el proyecto.** Un proyecto fijado
//! a 3.8 tiene todo el derecho de usar `typing.List`. Por eso cada hallazgo
//! nombra la versión en la que el API se fue o se deprecó, en vez de
//! afirmar que el código está roto — el usuario sabe a qué versión apunta,
//! este análisis no.
//!
//! Confianza, con un criterio que se sostiene solo:
//!   - `high` = el API **ya no existe** en alguna versión publicada de
//!     Python (importarlo es `ModuleNotFoundError`, llamarlo es
//!     `AttributeError`). No es una opinión de estilo.
//!   - `medium` = sigue existiendo pero está deprecado. Funciona hoy;
//!     el costo es futuro o de legibilidad.
//!
//! Lo que NO hace, dicho antes de que alguien lo asuma:
//!   - **Deprecaciones de librerías de terceros** (`numpy.float`,
//!     `DataFrame.append`, etc.) — eso necesitaría saber qué versión de cada
//!     paquete usa el proyecto. La única excepción es `pkg_resources`, que
//!     el propio setuptools deprecó para todas sus versiones.
//!   - **Distinguir un módulo local de uno de la stdlib**: si el proyecto
//!     tiene su propio `pipes.py`, `import pipes` se va a reportar igual.
//!     Se emparejan nombres de módulo, no rutas resueltas.
//!   - **Usos vía atributo de un `import typing`** (`typing.List[int]`) —
//!     los genéricos legacy se reportan una vez en el `from typing import`,
//!     que es donde está el arreglo, no una vez por anotación.

use std::collections::HashSet;

use rustpython_parser::ast::{Expr, Ranged, Stmt};
use serde::Serialize;

use crate::ast::parser::LineIndex;
use crate::ast::structure::{last_segment, node_name};
use crate::ast::walk::{walk_expr_tree, walk_stmts};

#[derive(Serialize, Clone)]
pub struct PyModernizationCandidate {
    /// El símbolo deprecado. Se llama `variable` y no `symbol` a propósito:
    /// es el mismo nombre de campo que usa `modernization.rs`, y el renderer
    /// del panel lo lee por ese nombre.
    pub variable: String,
    pub pattern: &'static str,
    pub line: usize,
    pub current: String,
    pub suggested_target: &'static str,
    pub reasoning: String,
    pub confidence: &'static str,
}

#[derive(Serialize)]
pub struct PyModernizationSummary {
    pub total: usize,
    pub removed_stdlib_module: usize,
    pub deprecated_library_module: usize,
    pub removed_stdlib_api: usize,
    pub deprecated_stdlib_api: usize,
    pub moved_to_collections_abc: usize,
    pub legacy_typing_generic: usize,
}

#[derive(Serialize)]
pub struct PyModernizationReport {
    pub candidates: Vec<PyModernizationCandidate>,
    pub summary: PyModernizationSummary,
    pub note: &'static str,
}

const NOTE: &str = "Este análisis no sabe a qué versión de Python apunta el proyecto: cada hallazgo nombra la versión en la que el API se fue o se deprecó, no afirma que el código esté roto. Confianza `high` = el API ya no existe en alguna versión publicada (importarlo o llamarlo falla); `medium` = sigue existiendo pero está deprecado. Empareja nombres de módulo, no rutas resueltas — un módulo propio del proyecto llamado igual que uno de la stdlib se reporta igual. No cubre deprecaciones de librerías de terceros (haría falta saber qué versión de cada paquete se usa), con la única excepción de pkg_resources, deprecado por el propio setuptools. Propone, nunca convierte código.";

/// Módulos que ya NO existen en alguna versión publicada: `(módulo, versión
/// que los removió, reemplazo)`. La mayoría vienen de PEP 594 ("dead
/// batteries", 3.13) y de las remociones de 3.12.
const REMOVED_MODULES: &[(&str, &str, &str)] = &[
    ("imp", "3.12", "`importlib` (`importlib.util.find_spec` / `module_from_spec`)"),
    ("distutils", "3.12", "`setuptools` para el build y `packaging` para versiones (PEP 632)"),
    ("asyncore", "3.12", "`asyncio`"),
    ("asynchat", "3.12", "`asyncio`"),
    ("smtpd", "3.12", "`aiosmtpd`"),
    ("telnetlib", "3.13", "`telnetlib3`, o `paramiko` si lo que hacía falta era SSH"),
    ("cgi", "3.13", "el framework web que ya use el proyecto; `email` para parsear multipart a mano"),
    ("cgitb", "3.13", "`traceback`"),
    ("imghdr", "3.13", "`filetype` o `puremagic`"),
    ("sndhdr", "3.13", "`filetype`"),
    ("nntplib", "3.13", "`pynntp`"),
    ("pipes", "3.13", "`subprocess`"),
    ("crypt", "3.13", "`passlib`, o `hashlib.scrypt` para derivar claves"),
    ("spwd", "3.13", "`python-pam`"),
    ("uu", "3.13", "`base64`"),
    ("xdrlib", "3.13", "un formato binario explícito con `struct`"),
    ("aifc", "3.13", "una librería de audio de terceros"),
    ("sunau", "3.13", "una librería de audio de terceros"),
    ("audioop", "3.13", "`audioop-lts`, o una librería de audio de terceros"),
    ("chunk", "3.13", "leer el contenedor a mano con `struct`"),
    ("mailcap", "3.13", "`mimetypes`"),
    ("msilib", "3.13", "una herramienta de empaquetado externa"),
    ("ossaudiodev", "3.13", "`sounddevice` o `pyaudio`"),
    ("lib2to3", "3.13", "`ast` para leer y `libcst` para reescribir código"),
];

/// Módulos que existen pero están deprecados por sus propios mantenedores.
const DEPRECATED_MODULES: &[(&str, &str, &str)] = &[(
    "pkg_resources",
    "setuptools 67 (2023)",
    "`importlib.metadata` para metadatos de paquetes y `importlib.resources` para archivos de datos",
)];

/// APIs removidas, emparejadas por nombre punteado completo o por su sufijo:
/// `(nombre, versión que lo removió, reemplazo)`. El emparejamiento por
/// sufijo importa — `ssl.wrap_socket` se fue, pero `contexto.wrap_socket()`
/// (sobre un `SSLContext`) es justamente la forma correcta, así que este
/// nombre NO se puede emparejar por último segmento.
const REMOVED_APIS: &[(&str, &str, &str)] = &[
    ("time.clock", "3.8", "`time.perf_counter()`, o `time.process_time()` si lo que se medía era tiempo de CPU"),
    ("inspect.getargspec", "3.11", "`inspect.signature()`"),
    ("base64.encodestring", "3.9", "`base64.encodebytes()`"),
    ("base64.decodestring", "3.9", "`base64.decodebytes()`"),
    (
        "ssl.wrap_socket",
        "3.12",
        "`ssl.create_default_context()` y después `contexto.wrap_socket()` — el reemplazo verifica el certificado y el hostname por default, que es exactamente lo que la función vieja no hacía",
    ),
    ("asyncio.coroutine", "3.11", "`async def`"),
];

/// APIs deprecadas pero todavía presentes.
const DEPRECATED_APIS: &[(&str, &str, &str)] = &[
    (
        "locale.getdefaultlocale",
        "3.11",
        "`locale.setlocale()` seguido de `locale.getlocale()`",
    ),
    (
        "asyncio.get_event_loop",
        "3.12",
        "`asyncio.run()` desde código sincrónico, o `asyncio.get_running_loop()` dentro de una corrutina",
    ),
];

/// Métodos cuyo último segmento alcanza para identificarlos: no existen como
/// nombre de método en ninguna otra cosa de la stdlib, así que emparejar la
/// cola cubre cualquier alias del import (`from datetime import datetime as
/// dt` → `dt.utcnow()`) sin arriesgar confusión.
const DEPRECATED_METHOD_TAILS: &[(&str, &str, &str)] = &[
    (
        "utcnow",
        "3.12",
        "`datetime.now(datetime.UTC)` — el reemplazo devuelve un datetime CON tzinfo, en vez de uno naive que después se confunde con hora local",
    ),
    ("utcfromtimestamp", "3.12", "`datetime.fromtimestamp(ts, datetime.UTC)`"),
];

/// Alias de `unittest` removidos en 3.12: `(alias, reemplazo)`. Son un grupo
/// aparte porque el arreglo es puramente mecánico — renombrar la llamada.
const REMOVED_UNITTEST_ALIASES: &[(&str, &str)] = &[
    ("assertEquals", "assertEqual"),
    ("assertNotEquals", "assertNotEqual"),
    ("assertAlmostEquals", "assertAlmostEqual"),
    ("assertNotAlmostEquals", "assertNotAlmostEqual"),
    ("assert_", "assertTrue"),
    ("failUnless", "assertTrue"),
    ("failIf", "assertFalse"),
    ("failUnlessEqual", "assertEqual"),
    ("failIfEqual", "assertNotEqual"),
    ("failUnlessAlmostEqual", "assertAlmostEqual"),
    ("failIfAlmostEqual", "assertNotAlmostEqual"),
    ("failUnlessRaises", "assertRaises"),
    ("assertRegexpMatches", "assertRegex"),
    ("assertNotRegexpMatches", "assertNotRegex"),
    ("assertRaisesRegexp", "assertRaisesRegex"),
];

/// ABCs que se movieron de `collections` a `collections.abc` en 3.3 y cuyos
/// alias viejos se removieron en 3.10.
const COLLECTIONS_ABC: &[&str] = &[
    "Mapping",
    "MutableMapping",
    "Sequence",
    "MutableSequence",
    "Set",
    "MutableSet",
    "Iterable",
    "Iterator",
    "Generator",
    "Callable",
    "Container",
    "Hashable",
    "Sized",
    "Reversible",
    "Collection",
    "Coroutine",
    "Awaitable",
    "AsyncIterable",
    "AsyncIterator",
    "AsyncGenerator",
    "ByteString",
    "ItemsView",
    "KeysView",
    "ValuesView",
    "MappingView",
];

/// Genéricos de `typing` reemplazados por los tipos nativos (PEP 585, 3.9).
const TYPING_PEP585: &[&str] = &[
    "List",
    "Dict",
    "Set",
    "FrozenSet",
    "Tuple",
    "Type",
    "Deque",
    "DefaultDict",
    "OrderedDict",
    "ChainMap",
    "Counter",
];

/// `Optional`/`Union`, reemplazados por la sintaxis `X | Y` (PEP 604, 3.10).
const TYPING_PEP604: &[&str] = &["Optional", "Union"];

/// Un import cubre el módulo exacto y sus submódulos: `import distutils.core`
/// es tan removido como `import distutils`.
fn module_matches(imported: &str, rule: &str) -> bool {
    imported == rule || imported.strip_prefix(rule).is_some_and(|rest| rest.starts_with('.'))
}

/// `ssl.wrap_socket` empareja `ssl.wrap_socket` y `a.b.ssl.wrap_socket`, pero
/// NUNCA `contexto.wrap_socket` — el punto de anclaje es el nombre completo
/// de la regla, no su último segmento.
fn api_matches(dotted: &str, rule: &str) -> bool {
    dotted == rule || dotted.ends_with(&format!(".{rule}"))
}

/// Shape vacío para el camino de error (archivo que no parsea): mismo
/// contrato que un análisis sin hallazgos, para que ningún consumidor tenga
/// que distinguir "no hay nada" de "no se pudo analizar".
pub fn empty_report() -> PyModernizationReport {
    PyModernizationReport {
        candidates: Vec::new(),
        summary: PyModernizationSummary {
            total: 0,
            removed_stdlib_module: 0,
            deprecated_library_module: 0,
            removed_stdlib_api: 0,
            deprecated_stdlib_api: 0,
            moved_to_collections_abc: 0,
            legacy_typing_generic: 0,
        },
        note: NOTE,
    }
}

pub fn python_modernization(suite: &[Stmt], line_index: &LineIndex) -> PyModernizationReport {
    // Dos pasadas sobre el mismo árbol, no una: `on_stmt` y `on_expr` del
    // mismo walker no pueden compartir estado mutable (dos préstamos mutables
    // vivos a la vez). Se juntan después, que además deja la deduplicación en
    // un solo lugar en vez de repartida por cada sitio que agrega.
    let mut candidates: Vec<PyModernizationCandidate> = Vec::new();
    {
        let mut on_stmt = |stmt: &Stmt| {
            let line = line_index.line_of(stmt.range().start().to_usize());
            match stmt {
                Stmt::Import(imp) => {
                    for alias in &imp.names {
                        check_module(&alias.name, line, &mut candidates);
                    }
                }
                Stmt::ImportFrom(imp) => {
                    let Some(module) = imp.module.as_ref().map(|m| m.to_string()) else { return };
                    check_module(&module, line, &mut candidates);
                    let imported: Vec<&str> = imp.names.iter().map(|a| a.name.as_str()).collect();
                    check_from_import(&module, &imported, line, &mut candidates);
                }
                // El walker compartido de `walk.rs` NO baja a `decorator_list`
                // (por eso `classifiers::memoization_info` recibe los
                // decoradores como parametro aparte en vez de encontrarlos
                // solo). Se recorren aca, contenido en este modulo, en vez de
                // cambiar un walker del que dependen otros ocho.
                Stmt::FunctionDef(f) => decorators(&f.decorator_list, line_index, &mut candidates),
                Stmt::AsyncFunctionDef(f) => decorators(&f.decorator_list, line_index, &mut candidates),
                Stmt::ClassDef(c) => decorators(&c.decorator_list, line_index, &mut candidates),
                _ => {}
            }
        };
        let mut on_expr = |_: &Expr| {};
        walk_stmts(suite, &mut on_stmt, &mut on_expr);
    }
    {
        // Se emparejan ATRIBUTOS, no llamadas: `ssl.wrap_socket(...)` contiene
        // el atributo `ssl.wrap_socket`, y `@asyncio.coroutine` es un atributo
        // sin llamada ninguna. Mirar el atributo cubre las dos formas con un
        // solo caso, y no duplica el hallazgo como sí pasaría mirando ambas.
        let mut on_stmt = |_: &Stmt| {};
        let mut on_expr = |e: &Expr| {
            let Expr::Attribute(_) = e else { return };
            let dotted = node_name(e);
            if dotted.contains('?') {
                return; // nombre no resoluble (`f().attr`): no hay nada que afirmar
            }
            let line = line_index.line_of(e.range().start().to_usize());
            check_attribute(&dotted, line, &mut candidates);
        };
        walk_stmts(suite, &mut on_stmt, &mut on_expr);
    }

    // El mismo símbolo puede aparecer varias veces en una línea
    // (`x.utcnow() - y.utcnow()`); un hallazgo por línea y símbolo alcanza.
    let mut seen: HashSet<(String, usize)> = HashSet::new();
    candidates.retain(|c| seen.insert((c.variable.clone(), c.line)));
    candidates.sort_by_key(|c| c.line);

    let count = |p: &str| candidates.iter().filter(|c| c.pattern == p).count();
    let summary = PyModernizationSummary {
        total: candidates.len(),
        removed_stdlib_module: count("removed_stdlib_module"),
        deprecated_library_module: count("deprecated_library_module"),
        removed_stdlib_api: count("removed_stdlib_api"),
        deprecated_stdlib_api: count("deprecated_stdlib_api"),
        moved_to_collections_abc: count("moved_to_collections_abc"),
        legacy_typing_generic: count("legacy_typing_generic"),
    };
    PyModernizationReport { candidates, summary, note: NOTE }
}

/// Los decoradores viven fuera del recorrido del walker compartido, pero un
/// `@asyncio.coroutine` es exactamente el mismo hallazgo que su forma
/// llamada — se recorren igual que cualquier otra expresion.
fn decorators(list: &[Expr], line_index: &LineIndex, out: &mut Vec<PyModernizationCandidate>) {
    for d in list {
        walk_expr_tree(d, &mut |e: &Expr| {
            let Expr::Attribute(_) = e else { return };
            let dotted = node_name(e);
            if dotted.contains('?') {
                return;
            }
            check_attribute(&dotted, line_index.line_of(e.range().start().to_usize()), out);
        });
    }
}

fn check_module(module: &str, line: usize, out: &mut Vec<PyModernizationCandidate>) {
    for (name, version, replacement) in REMOVED_MODULES {
        if module_matches(module, name) {
            out.push(PyModernizationCandidate {
                variable: (*name).to_string(),
                pattern: "removed_stdlib_module",
                line,
                current: format!("`import {module}` — removido de la stdlib en Python {version}"),
                suggested_target: "python_stdlib_replacement",
                reasoning: format!(
                    "`{name}` ya no viene con Python desde {version}: en esa versión el import falla con `ModuleNotFoundError`, no con un warning. Reemplazo: {replacement}."
                ),
                confidence: "high",
            });
            return;
        }
    }
    for (name, version, replacement) in DEPRECATED_MODULES {
        if module_matches(module, name) {
            out.push(PyModernizationCandidate {
                variable: (*name).to_string(),
                pattern: "deprecated_library_module",
                line,
                current: format!("`import {module}` — deprecado desde {version}"),
                suggested_target: "python_stdlib_replacement",
                reasoning: format!(
                    "`{name}` está deprecado desde {version} por sus propios mantenedores y su import es lento (escanea todos los paquetes instalados al arrancar). Reemplazo: {replacement}."
                ),
                confidence: "medium",
            });
            return;
        }
    }
}

fn check_from_import(module: &str, imported: &[&str], line: usize, out: &mut Vec<PyModernizationCandidate>) {
    // `from collections import Mapping` — ImportError en 3.10+.
    if module == "collections" {
        let moved: Vec<&str> = imported.iter().copied().filter(|n| COLLECTIONS_ABC.contains(n)).collect();
        if !moved.is_empty() {
            out.push(PyModernizationCandidate {
                variable: moved.join(", "),
                pattern: "moved_to_collections_abc",
                line,
                current: format!("`from collections import {}`", moved.join(", ")),
                suggested_target: "collections_abc",
                reasoning: format!(
                    "Estos ABCs se movieron a `collections.abc` en Python 3.3 y sus alias en `collections` se removieron en 3.10 — en esa versión el import falla con `ImportError`. El arreglo es cambiar el módulo: `from collections.abc import {}`.",
                    moved.join(", ")
                ),
                confidence: "high",
            });
        }
    }

    // `from typing import List, Optional` — un hallazgo por import, no uno
    // por anotación: el arreglo está acá, no en cada uso.
    if module == "typing" {
        let p585: Vec<&str> = imported.iter().copied().filter(|n| TYPING_PEP585.contains(n)).collect();
        let p604: Vec<&str> = imported.iter().copied().filter(|n| TYPING_PEP604.contains(n)).collect();
        if p585.is_empty() && p604.is_empty() {
            return;
        }
        let mut partes = Vec::new();
        if !p585.is_empty() {
            partes.push(format!(
                "{} {} equivalente nativo desde Python 3.9 (PEP 585): `list[int]` en vez de `List[int]`, `dict[str, int]` en vez de `Dict[str, int]`",
                p585.join(", "),
                if p585.len() == 1 { "tiene" } else { "tienen" }
            ));
        }
        if !p604.is_empty() {
            partes.push(format!(
                "{} {} con `|` desde 3.10 (PEP 604): `int | None` en vez de `Optional[int]`, `int | str` en vez de `Union[int, str]`",
                p604.join(", "),
                if p604.len() == 1 { "se escribe" } else { "se escriben" }
            ));
        }
        let todos: Vec<&str> = p585.iter().chain(p604.iter()).copied().collect();
        out.push(PyModernizationCandidate {
            variable: todos.join(", "),
            pattern: "legacy_typing_generic",
            line,
            current: format!("`from typing import {}`", todos.join(", ")),
            suggested_target: "pep585_builtin_generics",
            reasoning: format!(
                "{}. Deprecados, pero sin fecha de remoción anunciada: esto es una modernización de legibilidad, no una rotura futura — y solo aplica si el proyecto ya no soporta versiones anteriores a la que introdujo cada sintaxis.",
                partes.join("; ")
            ),
            confidence: "medium",
        });
    }
}

fn check_attribute(dotted: &str, line: usize, out: &mut Vec<PyModernizationCandidate>) {
    for (name, version, replacement) in REMOVED_APIS {
        if api_matches(dotted, name) {
            out.push(PyModernizationCandidate {
                variable: (*name).to_string(),
                pattern: "removed_stdlib_api",
                line,
                current: format!("`{dotted}` — removido de la stdlib en Python {version}"),
                suggested_target: "python_modern_api",
                reasoning: format!(
                    "`{name}` ya no existe desde Python {version}: en esa versión esto es un `AttributeError` en runtime, no un warning. Reemplazo: {replacement}."
                ),
                confidence: "high",
            });
        }
    }
    for (name, version, replacement) in DEPRECATED_APIS {
        if api_matches(dotted, name) {
            out.push(PyModernizationCandidate {
                variable: (*name).to_string(),
                pattern: "deprecated_stdlib_api",
                line,
                current: format!("`{dotted}` — deprecado desde Python {version}"),
                suggested_target: "python_modern_api",
                reasoning: format!(
                    "`{name}` sigue funcionando pero está deprecado desde Python {version}. Reemplazo: {replacement}."
                ),
                confidence: "medium",
            });
        }
    }
    let tail = last_segment(dotted);
    for (name, version, replacement) in DEPRECATED_METHOD_TAILS {
        if tail == *name {
            out.push(PyModernizationCandidate {
                variable: (*name).to_string(),
                pattern: "deprecated_stdlib_api",
                line,
                current: format!("`{dotted}` — deprecado desde Python {version}"),
                suggested_target: "python_modern_api",
                reasoning: format!("`{name}` está deprecado desde Python {version}. Reemplazo: {replacement}."),
                confidence: "medium",
            });
        }
    }
    for (alias, replacement) in REMOVED_UNITTEST_ALIASES {
        if tail == *alias {
            out.push(PyModernizationCandidate {
                variable: (*alias).to_string(),
                pattern: "removed_stdlib_api",
                line,
                current: format!("`{dotted}` — alias de unittest removido en Python 3.12"),
                suggested_target: "python_modern_api",
                reasoning: format!(
                    "`{alias}` era un alias deprecado de `{replacement}` y se removió en Python 3.12: ahí el test falla con `AttributeError` antes de probar nada. El arreglo es mecánico, renombrar la llamada a `{replacement}`."
                ),
                confidence: "high",
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::parse_module;

    fn report(src: &str) -> PyModernizationReport {
        let suite = parse_module(src).expect("parse error");
        let idx = LineIndex::build(src);
        python_modernization(&suite, &idx)
    }

    fn patterns(src: &str) -> Vec<&'static str> {
        report(src).candidates.into_iter().map(|c| c.pattern).collect()
    }

    #[test]
    fn un_archivo_moderno_no_produce_nada() {
        let src = "import importlib\nfrom collections.abc import Mapping\nfrom datetime import datetime, UTC\n\ndef ahora() -> list[int]:\n    return [datetime.now(UTC).year]\n";
        assert!(report(src).candidates.is_empty());
    }

    #[test]
    fn un_modulo_removido_se_detecta_con_confianza_alta() {
        let src = "import imp\n";
        let c = &report(src).candidates[0];
        assert_eq!(c.pattern, "removed_stdlib_module");
        assert_eq!(c.confidence, "high");
        assert!(c.reasoning.contains("3.12"));
    }

    #[test]
    fn un_submodulo_cuenta_igual_que_el_modulo() {
        // `import distutils.core` es tan removido como `import distutils`.
        assert_eq!(patterns("import distutils.core\n"), vec!["removed_stdlib_module"]);
    }

    #[test]
    fn un_modulo_que_solo_empieza_igual_no_dispara() {
        // `importlib` empieza con `imp` — sin el chequeo del punto, todo
        // `import importlib` seria un falso positivo, justo sobre el modulo
        // que este analisis RECOMIENDA usar.
        assert!(report("import importlib\nimport implementacion\n").candidates.is_empty());
    }

    #[test]
    fn from_import_de_un_modulo_removido_tambien_dispara() {
        assert_eq!(patterns("from imp import reload\n"), vec!["removed_stdlib_module"]);
    }

    #[test]
    fn pkg_resources_es_confianza_media_no_alta() {
        // Sigue existiendo: el costo es futuro, no una rotura de hoy.
        let c = &report("import pkg_resources\n").candidates[0];
        assert_eq!(c.pattern, "deprecated_library_module");
        assert_eq!(c.confidence, "medium");
    }

    #[test]
    fn un_api_removido_se_detecta() {
        let c = &report("import time\nt = time.clock()\n").candidates[0];
        assert_eq!(c.pattern, "removed_stdlib_api");
        assert_eq!(c.confidence, "high");
        assert!(c.reasoning.contains("perf_counter"));
    }

    #[test]
    fn wrap_socket_sobre_un_contexto_no_se_confunde_con_ssl_wrap_socket() {
        // `ssl.wrap_socket` se fue; `contexto.wrap_socket()` es EL reemplazo.
        // Emparejar por ultimo segmento reportaria el arreglo como el bug.
        let src = "import ssl\nctx = ssl.create_default_context()\ns = ctx.wrap_socket(sock)\n";
        assert!(report(src).candidates.is_empty());
    }

    #[test]
    fn ssl_wrap_socket_de_verdad_si_dispara() {
        let src = "import ssl\ns = ssl.wrap_socket(sock)\n";
        assert_eq!(patterns(src), vec!["removed_stdlib_api"]);
    }

    #[test]
    fn un_decorador_sin_llamada_tambien_se_detecta() {
        // `@asyncio.coroutine` es un Attribute suelto, no un Call: mirar solo
        // llamadas lo dejaria pasar entero.
        let src = "import asyncio\n\n@asyncio.coroutine\ndef vieja():\n    yield from asyncio.sleep(1)\n";
        assert!(report(src).candidates.iter().any(|c| c.variable == "asyncio.coroutine"));
    }

    #[test]
    fn utcnow_se_detecta_por_la_cola_para_cubrir_cualquier_alias() {
        let src = "from datetime import datetime as dt\nahora = dt.utcnow()\n";
        let c = &report(src).candidates[0];
        assert_eq!(c.pattern, "deprecated_stdlib_api");
        assert_eq!(c.confidence, "medium");
        assert!(c.reasoning.contains("naive"));
    }

    #[test]
    fn los_alias_viejos_de_unittest_se_detectan() {
        let src = "import unittest\n\nclass T(unittest.TestCase):\n    def test_x(self):\n        self.assertEquals(1, 1)\n        self.failUnless(True)\n";
        let r = report(src);
        assert_eq!(r.summary.removed_stdlib_api, 2);
        assert!(r.candidates.iter().any(|c| c.reasoning.contains("assertEqual")));
    }

    #[test]
    fn el_nombre_moderno_de_unittest_no_dispara() {
        let src = "import unittest\n\nclass T(unittest.TestCase):\n    def test_x(self):\n        self.assertEqual(1, 1)\n";
        assert!(report(src).candidates.is_empty());
    }

    #[test]
    fn los_abcs_movidos_se_reportan_juntos_en_un_hallazgo() {
        let src = "from collections import Mapping, Sequence, OrderedDict\n";
        let r = report(src);
        assert_eq!(r.candidates.len(), 1);
        let c = &r.candidates[0];
        assert_eq!(c.pattern, "moved_to_collections_abc");
        assert_eq!(c.confidence, "high");
        // OrderedDict NO se movio: sigue viviendo en `collections`.
        assert!(!c.variable.contains("OrderedDict"), "{}", c.variable);
        assert!(c.variable.contains("Mapping") && c.variable.contains("Sequence"));
    }

    #[test]
    fn collections_abc_correcto_no_dispara() {
        assert!(report("from collections.abc import Mapping\n").candidates.is_empty());
    }

    #[test]
    fn los_genericos_de_typing_son_un_hallazgo_por_import_no_uno_por_anotacion() {
        let src = "from typing import List, Optional\n\ndef f(a: List[int], b: Optional[str], c: List[str]) -> Optional[List[int]]:\n    return None\n";
        let r = report(src);
        assert_eq!(r.candidates.len(), 1, "un hallazgo, no uno por anotacion");
        assert_eq!(r.candidates[0].confidence, "medium");
        assert!(r.candidates[0].reasoning.contains("PEP 585"));
        assert!(r.candidates[0].reasoning.contains("PEP 604"));
    }

    #[test]
    fn typing_no_generico_no_dispara() {
        // `Any`/`TypeVar`/`Protocol` no tienen equivalente nativo.
        assert!(report("from typing import Any, TypeVar, Protocol\n").candidates.is_empty());
    }

    #[test]
    fn el_mismo_simbolo_dos_veces_en_una_linea_es_un_solo_hallazgo() {
        let src = "from datetime import datetime as dt\nd = dt.utcnow() - dt.utcnow()\n";
        assert_eq!(report(src).candidates.len(), 1);
    }

    #[test]
    fn los_hallazgos_salen_ordenados_por_linea() {
        let src = "import time\nimport imp\nfrom typing import List\nt = time.clock()\n";
        let lines: Vec<usize> = report(src).candidates.iter().map(|c| c.line).collect();
        let mut ordenadas = lines.clone();
        ordenadas.sort_unstable();
        assert_eq!(lines, ordenadas);
    }

    #[test]
    fn el_resumen_cuenta_por_patron() {
        let src = "import imp\nimport pkg_resources\nfrom typing import Dict\nfrom collections import Mapping\n";
        let s = report(src).summary;
        assert_eq!(s.total, 4);
        assert_eq!(s.removed_stdlib_module, 1);
        assert_eq!(s.deprecated_library_module, 1);
        assert_eq!(s.legacy_typing_generic, 1);
        assert_eq!(s.moved_to_collections_abc, 1);
    }

    #[test]
    fn la_nota_dice_que_no_conoce_la_version_objetivo() {
        // Es EL limite del modulo: sin esto, cada hallazgo se leeria como
        // "tu codigo esta roto", que es justo lo que no se puede afirmar.
        assert!(report("import imp\n").note.contains("no sabe a qué versión"));
    }

    #[test]
    fn un_import_dentro_de_una_funcion_tambien_cuenta() {
        let src = "def cargar():\n    import imp\n    return imp\n";
        assert_eq!(patterns(src), vec!["removed_stdlib_module"]);
    }
}
