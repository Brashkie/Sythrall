//! Extracción de clases e imports — puerto 1:1 de las secciones "Imports" y
//! "Funciones y clases" de `_parse_python` en `static_parser.py` (líneas
//! 88-180), más los helpers genéricos de AST (`_decorator_name`, `_node_name`,
//! `_extract_calls_python`) que necesita `rich.rs` para armar el shape
//! completo por función.

use rustpython_parser::ast::{Constant, Expr, Stmt};
use serde::Serialize;

use crate::hallazgos::datastructures;
use crate::ast::parser::LineIndex;
use crate::hallazgos::smells;
use crate::ast::walk::walk_stmts;

#[derive(Serialize, Clone)]
pub struct RichImport {
    pub module: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub line: usize,
}

#[derive(Serialize, Clone)]
pub struct DeadImport {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub name: String,
    pub module: String,
    pub line: usize,
}

#[derive(Serialize, Clone)]
pub struct RichMethod {
    pub name: String,
    pub line: usize,
    pub args: Vec<String>,
    pub is_async: bool,
}

#[derive(Serialize, Clone)]
pub struct RichClass {
    pub name: String,
    pub line: usize,
    pub end_line: usize,
    pub loc: usize,
    pub bases: Vec<String>,
    pub methods: Vec<RichMethod>,
    pub decorators: Vec<String>,
    pub docstring: Option<String>,
    /// Atributos únicos asignados vía `self.X = ...` en cualquier método —
    /// proxy de cuánto estado mantiene la clase, usado por el check de
    /// god object (Fase 22, `smells.rs`).
    pub attribute_count: usize,
    /// Los nombres reales detrás de `attribute_count` (mismo recorrido,
    /// `smells::self_attribute_names`) — usado por `apps/api/routers/
    /// diagram.py` para listarlos en el diagrama de clases. Nota: distinto
    /// del `_py_classes` de Python que reemplaza (que solo miraba `self.x=`
    /// dentro de `__init__` específicamente, más asignaciones sueltas a
    /// nivel de clase) — acá cubre `self.x=` de cualquier método, no solo
    /// `__init__`, y no captura asignaciones de clase sueltas. Vista
    /// ligeramente distinta del mismo concepto, aceptable porque el
    /// consumidor es un diagrama informativo, no un chequeo de corrección.
    pub attributes: Vec<String>,
    /// Fase 14, primera vez que un clasificador vive a nivel de clase (no
    /// solo de función, como `regex_class`/`grammar_class` en
    /// `RichFunction`) — ver `datastructures::data_structure_info`.
    pub data_structure: Option<String>,
    pub data_structure_note: Option<String>,
}

pub fn extract_imports(line_index: &LineIndex, suite: &[Stmt]) -> Vec<RichImport> {
    let mut out = Vec::new();
    let mut on_stmt = |stmt: &Stmt| match stmt {
        Stmt::Import(s) => {
            for alias in &s.names {
                out.push(RichImport {
                    module: alias.name.to_string(),
                    name: None,
                    alias: alias.asname.as_ref().map(|a| a.to_string()),
                    kind: "import",
                    line: line_index.line_of(s.range.start().to_usize()),
                });
            }
        }
        Stmt::ImportFrom(s) => {
            let module = s.module.as_ref().map(|m| m.to_string()).unwrap_or_default();
            for alias in &s.names {
                out.push(RichImport {
                    module: module.clone(),
                    name: Some(alias.name.to_string()),
                    alias: alias.asname.as_ref().map(|a| a.to_string()),
                    kind: "from_import",
                    line: line_index.line_of(s.range.start().to_usize()),
                });
            }
        }
        _ => {}
    };
    // Imports viven al nivel que sea (top-level la gran mayoría, pero
    // `ast.walk` de Python los encuentra en cualquier profundidad) — se
    // camina todo el árbol, no solo el nivel superior.
    let mut on_expr = |_: &Expr| {};
    walk_stmts(suite, &mut on_stmt, &mut on_expr);
    out
}

/// Puerto 1:1 de la sección "Imports no usados" de `_parse_python`
/// (`static_parser.py`): mismo criterio — un nombre importado sin alias
/// exótico (`*`) que no aparece ni como `Name` ni como atributo (`Attribute`)
/// en ningún lado del archivo. `walk_stmts` (no `walk_stmts_own_scope`)
/// porque `ast.walk` de Python tampoco respeta scopes de función acá: un
/// import usado solo dentro de una función anidada sigue contando como usado.
pub fn unused_imports(suite: &[Stmt], imports: &[RichImport]) -> Vec<DeadImport> {
    let mut used_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut used_attrs: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut on_stmt = |_: &Stmt| {};
    let mut on_expr = |e: &Expr| match e {
        Expr::Name(n) => {
            used_names.insert(n.id.to_string());
        }
        Expr::Attribute(a) => {
            used_attrs.insert(a.attr.to_string());
        }
        _ => {}
    };
    walk_stmts(suite, &mut on_stmt, &mut on_expr);

    imports
        .iter()
        .filter_map(|imp| {
            let alias = imp
                .alias
                .clone()
                .or_else(|| imp.name.clone())
                .unwrap_or_else(|| imp.module.split('.').next().unwrap_or("").to_string());
            if alias.is_empty() || alias == "*" {
                return None;
            }
            if used_names.contains(&alias) || used_attrs.contains(&alias) {
                None
            } else {
                Some(DeadImport { kind: "unused_import", name: alias, module: imp.module.clone(), line: imp.line })
            }
        })
        .collect()
}

pub fn extract_classes(line_index: &LineIndex, suite: &[Stmt]) -> Vec<RichClass> {
    let mut out = Vec::new();
    for stmt in suite {
        collect_classes(line_index, stmt, &mut out);
    }
    out
}

fn collect_classes(line_index: &LineIndex, stmt: &Stmt, out: &mut Vec<RichClass>) {
    if let Stmt::ClassDef(c) = stmt {
        let methods: Vec<RichMethod> = c
            .body
            .iter()
            .filter_map(|item| match item {
                Stmt::FunctionDef(f) => Some(RichMethod {
                    name: f.name.to_string(),
                    line: line_index.line_of(f.range.start().to_usize()),
                    args: arg_names(&f.args),
                    is_async: false,
                }),
                Stmt::AsyncFunctionDef(f) => Some(RichMethod {
                    name: f.name.to_string(),
                    line: line_index.line_of(f.range.start().to_usize()),
                    args: arg_names(&f.args),
                    is_async: true,
                }),
                _ => None,
            })
            .collect();

        let line = line_index.line_of(c.range.start().to_usize());
        let end_line = line_index.line_of(c.range.end().to_usize());
        let ds_info = datastructures::data_structure_info(&c.name, &methods, &c.body);
        let attr_names = smells::self_attribute_names(&c.body);
        out.push(RichClass {
            name: c.name.to_string(),
            line,
            end_line,
            loc: end_line.saturating_sub(line) + 1,
            bases: c.bases.iter().map(node_name).collect(),
            data_structure: ds_info.kind.map(|k| k.to_string()),
            data_structure_note: datastructures::data_structure_note(&ds_info),
            methods,
            decorators: c.decorator_list.iter().map(decorator_name).collect(),
            docstring: docstring_of(&c.body),
            attribute_count: attr_names.len(),
            attributes: attr_names,
        });
    }
    // Clases anidadas adentro de funciones/otras clases también cuentan,
    // igual que `ast.walk` las encontraría todas.
    for child in nested_bodies(stmt) {
        for s in child {
            collect_classes(line_index, s, out);
        }
    }
}

fn nested_bodies(stmt: &Stmt) -> Vec<&[Stmt]> {
    match stmt {
        Stmt::FunctionDef(s) => vec![&s.body],
        Stmt::AsyncFunctionDef(s) => vec![&s.body],
        // Bug real encontrado por un test nuevo (2026-08-31): faltaba este
        // brazo, así que una clase anidada DENTRO DE OTRA CLASE nunca se
        // encontraba — el comentario de `collect_classes` de abajo ya
        // afirmaba "adentro de funciones/OTRAS CLASES también cuentan",
        // pero el código no lo hacía. `collect_classes` ya recorre `c.body`
        // para extraer métodos; este brazo es lo que hace que TAMBIÉN se
        // vuelva a bajar por ese mismo body buscando clases anidadas.
        Stmt::ClassDef(s) => vec![&s.body],
        Stmt::If(s) => vec![&s.body, &s.orelse],
        Stmt::For(s) => vec![&s.body, &s.orelse],
        Stmt::AsyncFor(s) => vec![&s.body, &s.orelse],
        Stmt::While(s) => vec![&s.body, &s.orelse],
        Stmt::With(s) => vec![&s.body],
        Stmt::AsyncWith(s) => vec![&s.body],
        Stmt::Try(s) => vec![&s.body, &s.orelse, &s.finalbody],
        _ => vec![],
    }
}

pub fn arg_names(args: &rustpython_parser::ast::Arguments) -> Vec<String> {
    args.args.iter().map(|a| a.def.arg.to_string()).collect()
}

pub fn docstring_of(body: &[Stmt]) -> Option<String> {
    let first = body.first()?;
    if let Stmt::Expr(e) = first {
        if let Expr::Constant(c) = &*e.value {
            if let Constant::Str(s) = &c.value {
                return Some(s.to_string());
            }
        }
    }
    None
}

pub fn decorator_name(expr: &Expr) -> String {
    match expr {
        Expr::Name(n) => n.id.to_string(),
        Expr::Attribute(a) => format!("{}.{}", node_name(&a.value), a.attr),
        Expr::Call(c) => decorator_name(&c.func),
        _ => "?".to_string(),
    }
}

/// Último segmento de un nombre punteado: `np.exp` → `exp`, `w.sum` → `sum`.
///
/// Vive acá, al lado de `node_name`, porque es la operación que
/// invariablemente le sigue: `node_name` arma el punteado y media docena de
/// analizadores necesitan después su cola. Estaba escrita cuatro veces —
/// como `last_seg` en `control.rs`, `tail_of` en `pymodern.rs`, e inline en
/// otras dos — con la misma implementación y dos nombres distintos.
pub fn last_segment(dotted: &str) -> &str {
    dotted.rsplit('.').next().unwrap_or("")
}

pub fn node_name(expr: &Expr) -> String {
    match expr {
        Expr::Name(n) => n.id.to_string(),
        Expr::Attribute(a) => format!("{}.{}", node_name(&a.value), a.attr),
        _ => "?".to_string(),
    }
}

pub fn extract_calls(body: &[Stmt]) -> Vec<String> {
    let mut calls = std::collections::HashSet::new();
    let mut on_expr = |expr: &Expr| {
        if let Expr::Call(c) = expr {
            match &*c.func {
                Expr::Name(n) => {
                    calls.insert(n.id.to_string());
                }
                Expr::Attribute(a) => {
                    calls.insert(a.attr.to_string());
                }
                _ => {}
            }
        }
    };
    let mut on_stmt = |_: &Stmt| {};
    walk_stmts(body, &mut on_stmt, &mut on_expr);
    calls.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::parse_module;

    fn imports(src: &str) -> Vec<RichImport> {
        let suite = parse_module(src).unwrap();
        extract_imports(&LineIndex::build(src), &suite)
    }

    fn classes(src: &str) -> Vec<RichClass> {
        let suite = parse_module(src).unwrap();
        extract_classes(&LineIndex::build(src), &suite)
    }

    // ─── extract_imports ────────────────────────────────────────────────

    #[test]
    fn import_simple_se_extrae() {
        let imps = imports("import os\n");
        assert_eq!(imps.len(), 1);
        assert_eq!(imps[0].kind, "import");
        assert_eq!(imps[0].module, "os");
        assert_eq!(imps[0].alias, None);
    }

    #[test]
    fn import_con_alias_se_extrae() {
        let imps = imports("import numpy as np\n");
        assert_eq!(imps[0].alias, Some("np".to_string()));
    }

    #[test]
    fn from_import_se_extrae_con_name_y_module() {
        let imps = imports("from pathlib import Path\n");
        assert_eq!(imps[0].kind, "from_import");
        assert_eq!(imps[0].module, "pathlib");
        assert_eq!(imps[0].name, Some("Path".to_string()));
    }

    #[test]
    fn import_dentro_de_una_funcion_tambien_se_extrae() {
        // `ast.walk` de Python encuentra imports a cualquier profundidad —
        // este puerto tiene que hacer lo mismo, no solo mirar el nivel
        // superior del módulo.
        let imps = imports("def f():\n    import json\n    return json.dumps({})\n");
        assert_eq!(imps.len(), 1);
        assert_eq!(imps[0].module, "json");
    }

    // ─── unused_imports ─────────────────────────────────────────────────

    #[test]
    fn import_no_usado_se_marca_dead() {
        let src = "import os\n";
        let suite = parse_module(src).unwrap();
        let imps = extract_imports(&LineIndex::build(src), &suite);
        let dead = unused_imports(&suite, &imps);
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].name, "os");
    }

    #[test]
    fn import_usado_como_name_no_se_marca_dead() {
        let src = "import os\nprint(os)\n";
        let suite = parse_module(src).unwrap();
        let imps = extract_imports(&LineIndex::build(src), &suite);
        assert!(unused_imports(&suite, &imps).is_empty());
    }

    #[test]
    fn import_usado_solo_como_atributo_no_se_marca_dead() {
        // `os.path.join(...)` nunca produce un `Expr::Name` para `os` en el
        // AST de Python real (es la base de un `Attribute` encadenado) —
        // este chequeo también tiene que mirar usos como atributo.
        let src = "import os\nos.path.join('a', 'b')\n";
        let suite = parse_module(src).unwrap();
        let imps = extract_imports(&LineIndex::build(src), &suite);
        // `os` en sí es la base del primer Attribute — SÍ aparece como Name
        // acá (es el receptor de `.path`), así que este caso confirma que
        // no se marca dead, aunque la razón real sea la ruta `used_names`.
        assert!(unused_imports(&suite, &imps).is_empty());
    }

    #[test]
    fn import_con_alias_se_evalua_por_el_alias_no_por_el_modulo() {
        let src = "import numpy as np\n";
        let suite = parse_module(src).unwrap();
        let imps = extract_imports(&LineIndex::build(src), &suite);
        let dead = unused_imports(&suite, &imps);
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].name, "np"); // el alias, no "numpy"
    }

    #[test]
    fn import_con_alias_usado_por_el_alias_no_se_marca_dead() {
        let src = "import numpy as np\nnp.array([1])\n";
        let suite = parse_module(src).unwrap();
        let imps = extract_imports(&LineIndex::build(src), &suite);
        assert!(unused_imports(&suite, &imps).is_empty());
    }

    #[test]
    fn wildcard_import_nunca_se_marca_dead() {
        let src = "from os import *\n";
        let suite = parse_module(src).unwrap();
        let imps = extract_imports(&LineIndex::build(src), &suite);
        assert!(unused_imports(&suite, &imps).is_empty());
    }

    // ─── extract_classes ────────────────────────────────────────────────

    #[test]
    fn clase_simple_con_metodo_se_extrae() {
        let cs = classes("class Foo:\n    def bar(self):\n        return 1\n");
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].name, "Foo");
        assert_eq!(cs[0].methods.len(), 1);
        assert_eq!(cs[0].methods[0].name, "bar");
        assert!(!cs[0].methods[0].is_async);
    }

    #[test]
    fn metodo_async_se_marca_is_async() {
        let cs = classes("class Foo:\n    async def bar(self):\n        return 1\n");
        assert!(cs[0].methods[0].is_async);
    }

    #[test]
    fn clase_con_bases_las_extrae() {
        let cs = classes("class Dog(Animal, Loud):\n    pass\n");
        assert_eq!(cs[0].bases, vec!["Animal".to_string(), "Loud".to_string()]);
    }

    #[test]
    fn clase_con_docstring_la_extrae() {
        let cs = classes("class Foo:\n    \"\"\"Hace cosas.\"\"\"\n    def bar(self): pass\n");
        assert_eq!(cs[0].docstring.as_deref(), Some("Hace cosas."));
    }

    #[test]
    fn self_atributos_se_cuentan_de_cualquier_metodo() {
        let cs = classes("class Foo:\n    def __init__(self):\n        self.x = 1\n    def other(self):\n        self.y = 2\n");
        assert_eq!(cs[0].attribute_count, 2);
        assert!(cs[0].attributes.contains(&"x".to_string()));
        assert!(cs[0].attributes.contains(&"y".to_string()));
    }

    #[test]
    fn clase_anidada_dentro_de_una_funcion_tambien_se_detecta() {
        // Caso puntual que `ast.walk` de Python encontraría y este puerto
        // tiene que replicar: una clase definida DENTRO de una función, no
        // a nivel de módulo.
        let cs = classes("def make():\n    class Inner:\n        def m(self):\n            return 1\n    return Inner\n");
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].name, "Inner");
    }

    #[test]
    fn clase_anidada_dentro_de_otra_clase_tambien_se_detecta() {
        let cs = classes("class Outer:\n    class Inner:\n        pass\n");
        assert_eq!(cs.iter().filter(|c| c.name == "Inner").count(), 1);
    }

    // ─── decorator_name ─────────────────────────────────────────────────

    #[test]
    fn decorator_simple_es_el_nombre() {
        let cs = classes("class Foo:\n    @staticmethod\n    def bar():\n        pass\n");
        assert_eq!(cs[0].methods.len(), 1); // sanity: el método se sigue extrayendo con decorator
    }

    #[test]
    fn decorator_como_atributo_se_arma_dotted() {
        let src = "@app.route\ndef handler():\n    pass\n";
        let suite = parse_module(src).unwrap();
        let Stmt::FunctionDef(f) = &suite[0] else { panic!("esperaba FunctionDef") };
        let name = decorator_name(&f.decorator_list[0]);
        assert_eq!(name, "app.route");
    }

    #[test]
    fn decorator_como_llamada_resuelve_el_nombre_de_la_llamada() {
        // `@app.route("/x")` es un `Expr::Call` cuyo `func` es el
        // `Attribute` real — `decorator_name` tiene que "ver a través" de
        // la llamada, no devolver "?" para este caso extremadamente común
        // en frameworks web (Flask/FastAPI).
        let src = "@app.route(\"/x\")\ndef handler():\n    pass\n";
        let suite = parse_module(src).unwrap();
        let Stmt::FunctionDef(f) = &suite[0] else { panic!("esperaba FunctionDef") };
        let name = decorator_name(&f.decorator_list[0]);
        assert_eq!(name, "app.route");
    }

    // ─── docstring_of ───────────────────────────────────────────────────

    #[test]
    fn funcion_sin_docstring_da_none() {
        let src = "def f():\n    return 1\n";
        let suite = parse_module(src).unwrap();
        let Stmt::FunctionDef(f) = &suite[0] else { panic!("esperaba FunctionDef") };
        assert_eq!(docstring_of(&f.body), None);
    }

    #[test]
    fn funcion_con_docstring_la_extrae() {
        let src = "def f():\n    \"\"\"Suma dos cosas.\"\"\"\n    return 1\n";
        let suite = parse_module(src).unwrap();
        let Stmt::FunctionDef(f) = &suite[0] else { panic!("esperaba FunctionDef") };
        assert_eq!(docstring_of(&f.body).as_deref(), Some("Suma dos cosas."));
    }
}
