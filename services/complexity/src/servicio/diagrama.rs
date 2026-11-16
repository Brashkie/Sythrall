//! **Diagramas Mermaid — Fase 35, portado desde `routers/diagram.py`.**
//!
//! Cuatro vistas de un archivo Python (flowchart, call graph, clases,
//! secuencia) más un flowchart genérico para el resto de los lenguajes. Lo
//! único que hace es **armar un string Mermaid** a partir de un análisis que
//! ya existe: no parsea nada por su cuenta.
//!
//! **Y se borraron 190 líneas en vez de portarlas.** El router traía dos
//! implementaciones de cada builder: una sobre el análisis del motor
//! (`_py_*_from_rich`) y otra con `ast.walk` propio en Python
//! (`_py_*_fallback`), para el caso de que el sidecar no respondiera. Su
//! propio docstring justificaba conservarlas: *"antes de esta migración estos
//! builders eran 100% independientes del sidecar, así que borrarlos sería una
//! regresión real, no una limpieza"*.
//!
//! Ese argumento era correcto entonces y **dejó de serlo al mover el código
//! acá**: el análisis corre en el mismo proceso que arma el diagrama, así que
//! la rama "el sidecar no responde" no es un camino degradado, es un estado
//! imposible. No se puede caer la conexión con uno mismo. Mantener una
//! segunda heurística para un caso que no puede ocurrir no es robustez: es
//! una implementación paralela que nadie ejercita y que se desincroniza en
//! silencio de la que sí corre.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::ast::structure::{RichClass, RichMethod};
use crate::flujo::graph::CallEdge;
use crate::servicio::rich::{self, RichFunction};

/// Cuántos elementos entran en cada diagrama antes de cortar.
///
/// Los topes vienen del router viejo y son del **render**, no del análisis:
/// un Mermaid con 200 nodos es ilegible y además tarda en dibujarse en el
/// navegador. El análisis completo está en `/parse/python`; esto es una vista.
const MAX_FUNCIONES_FLOWCHART: usize = 12;
const MAX_FUNCIONES_CALLGRAPH: usize = 12;
const MAX_CLASES: usize = 8;
const MAX_ATRIBUTOS: usize = 6;
const MAX_METODOS: usize = 8;
const MAX_PARTICIPANTES: usize = 6;
const MAX_PASOS_SECUENCIA: usize = 5;
const MAX_FUNCIONES_GENERICO: usize = 10;

#[derive(Deserialize)]
pub struct DiagramaRequest {
    #[serde(default = "archivo_por_defecto")]
    pub filename: String,
    #[serde(default)]
    pub content: String,
    #[serde(default = "tipo_por_defecto")]
    pub diagram_type: String,
}

fn archivo_por_defecto() -> String {
    "script.py".to_string()
}

fn tipo_por_defecto() -> String {
    "flowchart".to_string()
}

/// **El Mermaid que se muestra cuando el archivo no compila.**
///
/// Cada tipo de diagrama trae su propio texto porque cada uno es un dialecto
/// distinto de Mermaid: un `flowchart TD` dentro de un panel que espera un
/// `classDiagram` no se dibuja, se rompe. Por eso el error no puede ser un
/// string único.
fn mermaid_de_error(tipo: &str, linea: usize) -> String {
    match tipo {
        "callgraph" => "graph LR\n    ERR[Error de sintaxis]".to_string(),
        "classes" => "classDiagram\n    class Error".to_string(),
        "sequence" => "sequenceDiagram\n    participant Error".to_string(),
        _ => format!("flowchart TD\n    ERR[SyntaxError línea {linea}]"),
    }
}

/// **El error del motor habla en bytes y el panel en líneas.**
///
/// `analyze_rich` devuelve mensajes como `invalid syntax. Got unexpected
/// token ':' at byte offset 6` — medido, no supuesto. El panel venía
/// mostrando `SyntaxError línea N` porque el `SyntaxError` de Python trae
/// `lineno`. Para no cambiarle el texto al usuario se convierte el offset
/// acá: contar saltos de línea hasta esa posición es exacto y no necesita
/// que el parser cambie su contrato.
///
/// Si el mensaje no trae offset —otro error de parseo, otra versión del
/// parser— devuelve 1 en vez de inventar una línea: un número equivocado es
/// peor que un número vago cuando alguien lo usa para ir a mirar el archivo.
fn linea_del_error(mensaje: &str, contenido: &str) -> usize {
    let Some(resto) = mensaje.split("byte offset ").nth(1) else {
        return 1;
    };
    let digitos: String = resto.chars().take_while(char::is_ascii_digit).collect();
    let Ok(offset) = digitos.parse::<usize>() else {
        return 1;
    };
    let hasta = offset.min(contenido.len());
    contenido[..hasta].matches('\n').count() + 1
}

/// Escapa lo que rompe una etiqueta de Mermaid entre comillas dobles.
///
/// Mermaid delimita la etiqueta de un nodo con `"`, así que una comilla doble
/// dentro la corta. El router viejo hacía `.replace('"', "'")` sólo sobre el
/// docstring; acá se aplica a **todo** lo que viene del código del usuario
/// —nombres de función, de clase, de argumento— porque cualquiera de esos
/// puede traer una comilla y romper el diagrama entero.
fn etiqueta(s: &str) -> String {
    s.replace('"', "'").replace(['\n', '\r'], " ")
}

// ── Builders de Python ────────────────────────────────────────────────────

fn flowchart_de_python(funciones: &[RichFunction], filename: &str) -> String {
    let mut fns: Vec<&RichFunction> = funciones.iter().collect();
    fns.sort_by_key(|f| f.line);

    if fns.is_empty() {
        return format!(
            "flowchart TD\n    A[{}]\n    B[Sin funciones]\n    A --> B",
            etiqueta(filename)
        );
    }

    let visibles = fns.len().min(MAX_FUNCIONES_FLOWCHART);
    let mut l = vec!["flowchart TD".to_string(), format!("    START([{}])", etiqueta(filename))];

    for (i, f) in fns.iter().take(visibles).enumerate() {
        // `self` no se muestra: es ruido en cada método y no informa nada.
        let args: Vec<String> =
            f.args.iter().filter(|a| *a != "self").take(2).map(|a| etiqueta(a)).collect();
        let prefijo = if f.is_async { "async " } else { "" };
        let mut texto = format!("{prefijo}{}({})", etiqueta(&f.name), args.join(", "));
        let doc = f.docstring.as_deref().unwrap_or("");
        if !doc.is_empty() {
            // 40 caracteres, no 40 bytes: cortar por byte parte un acento al
            // medio y el Mermaid resultante no se dibuja.
            let corto: String = doc.chars().take(40).collect();
            texto.push_str(&format!("\\n{}", etiqueta(&corto)));
        }
        l.push(format!("    F{i}[\"{texto}\"]"));
    }

    l.push("    END([Fin])".to_string());
    l.push("    START --> F0".to_string());
    for i in 0..visibles.saturating_sub(1) {
        l.push(format!("    F{i} --> F{}", i + 1));
    }
    l.push(format!("    F{} --> END", visibles - 1));

    for (i, f) in fns.iter().take(visibles).enumerate() {
        let (fondo, borde) = if f.is_async {
            ("#300a3a", "#b87dff")
        } else if f.returns_value {
            ("#0f3020", "#00f5a0")
        } else {
            ("#1a2040", "#3d9eff")
        };
        l.push(format!("    style F{i} fill:{fondo},stroke:{borde},color:#c8d4f0"));
    }
    l.push("    style START fill:#0a2040,stroke:#3d9eff,color:#c8d4f0".to_string());
    l.push("    style END fill:#0a2040,stroke:#3d9eff,color:#c8d4f0".to_string());
    l.join("\n")
}

fn callgraph_de_python(funciones: &[RichFunction], aristas: &[CallEdge]) -> String {
    if funciones.is_empty() {
        return "graph LR\n    A[Sin funciones]".to_string();
    }
    let nombres: std::collections::BTreeSet<&str> =
        funciones.iter().map(|f| f.name.as_str()).collect();
    let visibles = nombres_unicos_por_linea(funciones, MAX_FUNCIONES_CALLGRAPH);

    let mut l = vec!["graph LR".to_string()];
    for n in &visibles {
        l.push(format!("    {}[\"{}\"]", identificador(n), etiqueta(n)));
    }

    let mut vistas: std::collections::BTreeSet<(String, String)> = std::collections::BTreeSet::new();
    for a in aristas {
        // Sólo llamadas entre funciones del mismo archivo: una arista a algo
        // que no está declarado acá apuntaría a un nodo que no existe, y
        // Mermaid lo inventaría como un nodo suelto sin etiqueta.
        if !nombres.contains(a.from.as_str()) || !nombres.contains(a.to.as_str()) {
            continue;
        }
        let clave = (a.from.clone(), a.to.clone());
        if vistas.insert(clave) {
            l.push(format!("    {} -->|llama| {}", identificador(&a.from), identificador(&a.to)));
        }
    }
    l.join("\n")
}

/// Nombres de función únicos, en orden de aparición en el archivo.
///
/// **Resuelve dos defectos a la vez, y uno lo introduje yo.** En Python esto
/// era `list({fn["name"] for fn in functions})[:12]`: el set deduplicaba
/// —bien— pero el orden de un set **no está definido**, así que el mismo
/// archivo podía dar nodos distintos entre dos llamadas. Al cambiarlo por una
/// lista ordenada por línea arreglé el orden y **perdí la deduplicación**, y
/// se vio en la verificación real: un `hablar` sobreescrito en una subclase
/// salía dos veces, declarando el mismo id de nodo dos veces en el callgraph
/// y dos `participant` iguales en el de secuencia, que Mermaid colapsa en uno
/// y convierte en una autollamada que el código no tiene.
///
/// Un método con el mismo nombre en dos clases es lo más normal que hay —un
/// override—, así que el caso no es raro.
///
/// Acá el nombre es la **identidad del nodo**, así que repetirlo no aporta un
/// nodo nuevo. En el flowchart es distinto: cada función es `F0..Fn` por
/// índice, y ahí dos funciones homónimas sí son dos cajas legítimas.
fn nombres_unicos_por_linea(funciones: &[RichFunction], tope: usize) -> Vec<&str> {
    let mut ordenadas: Vec<&RichFunction> = funciones.iter().collect();
    ordenadas.sort_by_key(|f| f.line);
    let mut vistos: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    ordenadas
        .iter()
        .map(|f| f.name.as_str())
        .filter(|n| vistos.insert(n))
        .take(tope)
        .collect()
}

/// Un identificador de nodo seguro para Mermaid.
///
/// El nombre va **dos veces** en cada línea: como id del nodo y como etiqueta.
/// La etiqueta va entre comillas y tolera casi todo; el id no — un carácter
/// raro ahí rompe el parser de Mermaid. En Python el nombre crudo se usaba
/// como id directamente, lo que funcionaba sólo porque los nombres de Python
/// son `[A-Za-z0-9_]`; cualquier otro lenguaje que entre acá después lo
/// rompería.
fn identificador(nombre: &str) -> String {
    let limpio: String =
        nombre.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect();
    // Mermaid no acepta un id que arranque con dígito.
    if limpio.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("n{limpio}")
    } else {
        limpio
    }
}

fn clases_de_python(clases: &[RichClass]) -> String {
    if clases.is_empty() {
        return "classDiagram\n    class SinClases".to_string();
    }
    let nombres: std::collections::BTreeSet<&str> = clases.iter().map(|c| c.name.as_str()).collect();
    let mut l = vec!["classDiagram".to_string()];
    for c in clases.iter().take(MAX_CLASES) {
        l.push(format!("    class {} {{", identificador(&c.name)));
        for a in c.attributes.iter().take(MAX_ATRIBUTOS) {
            l.push(format!("        +{}", etiqueta(a)));
        }
        for m in c.methods.iter().take(MAX_METODOS) {
            l.push(format!("        +{}", firma_de_metodo(m)));
        }
        l.push("    }".to_string());
        for base in &c.bases {
            // Sólo se dibuja la herencia cuando la base está en el MISMO
            // archivo: una base importada no tiene nodo, y Mermaid la
            // inventaría como una clase vacía que no existe en el código.
            if nombres.contains(base.as_str()) {
                l.push(format!(
                    "    {} <|-- {} : hereda",
                    identificador(base),
                    identificador(&c.name)
                ));
            }
        }
    }
    l.join("\n")
}

fn firma_de_metodo(m: &RichMethod) -> String {
    let args: Vec<String> =
        m.args.iter().filter(|a| *a != "self").take(3).map(|a| etiqueta(a)).collect();
    format!("{}({})", etiqueta(&m.name), args.join(", "))
}

fn secuencia_de_python(funciones: &[RichFunction]) -> String {
    // **Los participantes son únicos por nombre, y Python no lo hacía.** Dos
    // funciones homónimas daban dos `participant` iguales; Mermaid los
    // colapsa en uno solo y los pasos entre ellos se dibujan como una
    // autollamada que el código no tiene. Ver `nombres_unicos_por_linea`.
    let nombres = nombres_unicos_por_linea(funciones, MAX_PARTICIPANTES);

    // Con menos de dos participantes no hay ninguna interacción que mostrar,
    // y un diagrama de un solo participante llamándose a sí mismo no dice
    // nada: se muestra el placeholder explícito.
    if nombres.len() < 2 {
        return "sequenceDiagram\n    participant main\n    main->>main: ejecutar\n    main-->>main: fin"
            .to_string();
    }

    let mut l = vec!["sequenceDiagram".to_string(), "    participant Usuario".to_string()];
    for n in &nombres {
        l.push(format!("    participant {}", identificador(n)));
    }
    let pasos = nombres.len().min(MAX_PASOS_SECUENCIA);
    l.push(format!("    Usuario->>+{}: invocar", identificador(nombres[0])));
    for i in 0..pasos - 1 {
        l.push(format!(
            "    {}->>+{}: llamar",
            identificador(nombres[i]),
            identificador(nombres[i + 1])
        ));
    }
    for i in (1..pasos).rev() {
        l.push(format!(
            "    {}-->>-{}: retornar",
            identificador(nombres[i]),
            identificador(nombres[i - 1])
        ));
    }
    l.push(format!("    {}-->>-Usuario: resultado", identificador(nombres[0])));
    l.join("\n")
}

// ── Otros lenguajes ───────────────────────────────────────────────────────

/// Flowchart para lo que no es Python.
///
/// **Es deliberadamente pobre y eso está dicho**: detecta funciones de JS/TS
/// con una expresión regular y para cualquier otra extensión muestra sólo el
/// nombre del archivo y su cantidad de líneas. El motor ya parsea 13
/// lenguajes de verdad, así que esto es una brecha real y no una limitación
/// técnica — está anotada en el ROADMAP. Portarla tal cual es lo honesto para
/// este paso: ampliarla sería un cambio de producto escondido en una
/// migración.
fn flowchart_generico(contenido: &str, filename: &str, ext: &str) -> String {
    let lineas: Vec<&str> = contenido.lines().collect();
    let mut funciones: Vec<String> = Vec::new();

    if ext == ".js" || ext == ".ts" {
        for linea in &lineas {
            if let Some(n) = nombre_de_funcion_js(linea) {
                funciones.push(n);
            }
        }
    }

    if funciones.is_empty() {
        let etiqueta_ext = ext.trim_start_matches('.').to_uppercase();
        return format!(
            "flowchart TD\n    A[{}]\n    B[{} líneas · {}]\n    A --> B",
            etiqueta(filename),
            lineas.len(),
            etiqueta_ext
        );
    }

    let visibles = funciones.len().min(MAX_FUNCIONES_GENERICO);
    let mut s = format!("flowchart TD\n    START([{}])\n", etiqueta(filename));
    for (i, f) in funciones.iter().take(visibles).enumerate() {
        s.push_str(&format!("    F{i}[\"{}()\"]\n", etiqueta(f)));
    }
    s.push_str("    END([Fin])\n    START --> F0\n");
    for i in 0..visibles.saturating_sub(1) {
        s.push_str(&format!("    F{i} --> F{}\n", i + 1));
    }
    s.push_str(&format!("    F{} --> END\n", visibles - 1));
    s
}

/// `function nombre` o `const nombre = (` / `const nombre = async (`.
///
/// Se escribe a mano en vez de con `regex` para no pagar la compilación de un
/// patrón por línea; el equivalente del router era
/// `(?:function\s+(\w+)|const\s+(\w+)\s*=\s*(?:async\s*)?\()`.
fn identificador_desde(s: &str) -> Option<(String, &str)> {
    let n: String = s.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    if n.is_empty() {
        None
    } else {
        let resto = &s[n.len()..];
        Some((n, resto))
    }
}

fn nombre_de_funcion_js(linea: &str) -> Option<String> {
    let t = linea.trim();

    if let Some(resto) = t.find("function ").map(|i| &t[i + 9..]) {
        let (n, _) = identificador_desde(resto.trim_start())?;
        return descartar_palabra_clave(n);
    }
    for clave in ["const ", "let ", "var "] {
        if let Some(resto) = t.strip_prefix(clave) {
            let (n, luego) = identificador_desde(resto.trim_start())?;
            let luego = luego.trim_start();
            let Some(luego) = luego.strip_prefix('=') else { continue };
            let luego = luego.trim_start();
            let luego = luego.strip_prefix("async").map_or(luego, str::trim_start);
            // Sólo cuenta si lo asignado abre paréntesis: `const x = 1` no es
            // una función.
            if luego.starts_with('(') {
                return descartar_palabra_clave(n);
            }
        }
    }
    None
}

fn descartar_palabra_clave(n: String) -> Option<String> {
    if matches!(n.as_str(), "if" | "for" | "while" | "switch" | "catch") {
        None
    } else {
        Some(n)
    }
}

// ── Punto de entrada ──────────────────────────────────────────────────────

pub fn generar(req: &DiagramaRequest) -> Value {
    let ext = extension(&req.filename);
    let mermaid = if ext == ".py" {
        let analisis = rich::analyze_rich(&req.content);
        match &analisis.error {
            Some(e) => mermaid_de_error(&req.diagram_type, linea_del_error(e, &req.content)),
            None => match req.diagram_type.as_str() {
                "callgraph" => callgraph_de_python(&analisis.functions, &analisis.call_graph),
                "classes" => clases_de_python(&analisis.classes),
                "sequence" => secuencia_de_python(&analisis.functions),
                _ => flowchart_de_python(&analisis.functions, &req.filename),
            },
        }
    } else {
        flowchart_generico(&req.content, &req.filename, &ext)
    };

    json!({
        "filename": req.filename,
        "diagram_type": req.diagram_type,
        "mermaid": mermaid,
        "ts": crate::servicio::logstore::now_string(),
    })
}

fn extension(nombre: &str) -> String {
    match nombre.rfind('.') {
        Some(0) | None => String::new(),
        Some(i) => nombre[i..].to_ascii_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pedir(contenido: &str, tipo: &str) -> String {
        let r = generar(&DiagramaRequest {
            filename: "script.py".to_string(),
            content: contenido.to_string(),
            diagram_type: tipo.to_string(),
        });
        r["mermaid"].as_str().unwrap().to_string()
    }

    const DOS_FUNCIONES: &str = "def primera(a, b):\n    \"\"\"Hace algo.\"\"\"\n    return a + b\n\ndef segunda():\n    return primera(1, 2)\n";

    #[test]
    fn el_flowchart_arranca_en_start_y_termina_en_end() {
        let m = pedir(DOS_FUNCIONES, "flowchart");
        assert!(m.starts_with("flowchart TD"));
        assert!(m.contains("START([script.py])"));
        assert!(m.contains("START --> F0"));
        assert!(m.contains("--> END"));
        assert!(m.contains("primera(a, b)"), "la firma va en la etiqueta: {m}");
        assert!(m.contains("Hace algo."), "el docstring tambien");
    }

    /// `self` no se muestra: es ruido en cada método y no informa nada.
    #[test]
    fn el_flowchart_no_muestra_self() {
        let m = pedir("class C:\n    def metodo(self, x):\n        return x\n", "flowchart");
        assert!(m.contains("metodo(x)"), "esperaba metodo(x), no metodo(self, x): {m}");
    }

    #[test]
    fn un_archivo_sin_funciones_lo_dice() {
        let m = pedir("x = 1\ny = 2\n", "flowchart");
        assert!(m.contains("Sin funciones"));
    }

    /// **Cada tipo de diagrama necesita SU propio Mermaid de error**: un
    /// `flowchart TD` dentro de un panel que espera `classDiagram` no se
    /// dibuja mal, no se dibuja.
    #[test]
    fn cada_tipo_tiene_su_propio_mermaid_de_error() {
        let roto = "def f(:\n    pass\n";
        assert!(pedir(roto, "flowchart").starts_with("flowchart TD"));
        assert!(pedir(roto, "callgraph").starts_with("graph LR"));
        assert!(pedir(roto, "classes").starts_with("classDiagram"));
        assert!(pedir(roto, "sequence").starts_with("sequenceDiagram"));
    }

    /// **El error del motor viene en bytes y el mensaje sale en líneas.**
    #[test]
    fn el_byte_offset_del_error_se_convierte_a_numero_de_linea() {
        // El error está en la cuarta línea.
        let roto = "x = 1\ny = 2\nz = 3\ndef f(:\n    pass\n";
        let m = pedir(roto, "flowchart");
        assert!(m.contains("línea 4"), "esperaba la linea 4: {m}");

        // Y la conversión directa, sin pasar por el parser.
        assert_eq!(linea_del_error("... at byte offset 0", "abc\ndef"), 1);
        assert_eq!(linea_del_error("... at byte offset 4", "abc\ndef"), 2);
        // Un mensaje sin offset no inventa una línea.
        assert_eq!(linea_del_error("otro error cualquiera", "abc\ndef"), 1);
        // Un offset más allá del final no entra en pánico.
        assert_eq!(linea_del_error("... at byte offset 99999", "abc"), 1);
    }

    #[test]
    fn el_callgraph_dibuja_la_arista_entre_dos_funciones_del_archivo() {
        let m = pedir(DOS_FUNCIONES, "callgraph");
        assert!(m.starts_with("graph LR"));
        assert!(m.contains("segunda -->|llama| primera"), "falta la arista: {m}");
    }

    /// **Una llamada a algo que no está declarado en el archivo no se
    /// dibuja**: Mermaid inventaría un nodo suelto sin etiqueta.
    #[test]
    fn una_llamada_a_algo_de_afuera_no_crea_un_nodo_fantasma() {
        let src = "import os\n\ndef f():\n    return os.getcwd()\n";
        let m = pedir(src, "callgraph");
        assert!(!m.contains("getcwd"), "no tiene que aparecer: {m}");
        assert!(!m.contains("-->"), "y no deberia haber ninguna arista: {m}");
    }

    #[test]
    fn el_callgraph_sin_funciones_lo_dice() {
        assert!(pedir("x = 1\n", "callgraph").contains("Sin funciones"));
    }

    #[test]
    fn el_diagrama_de_clases_lista_atributos_y_metodos() {
        let src = "class Base:\n    pass\n\nclass Hija(Base):\n    def __init__(self):\n        self.valor = 1\n    def usar(self, x):\n        return self.valor + x\n";
        let m = pedir(src, "classes");
        assert!(m.starts_with("classDiagram"));
        assert!(m.contains("class Hija {"));
        assert!(m.contains("+valor"), "el atributo: {m}");
        assert!(m.contains("+usar(x)"), "el metodo sin self: {m}");
        assert!(m.contains("Base <|-- Hija : hereda"), "la herencia: {m}");
    }

    /// **Una base importada no se dibuja**: no tiene nodo en este archivo, y
    /// Mermaid la inventaría como una clase vacía que no existe en el código.
    #[test]
    fn una_base_de_afuera_del_archivo_no_se_dibuja() {
        let src = "from django.db import models\n\nclass Modelo(models.Model):\n    pass\n";
        let m = pedir(src, "classes");
        assert!(m.contains("class Modelo"));
        assert!(!m.contains("hereda"), "la base no esta en el archivo: {m}");
    }

    #[test]
    fn sin_clases_lo_dice() {
        assert!(pedir("def f():\n    pass\n", "classes").contains("SinClases"));
    }

    #[test]
    fn la_secuencia_va_y_vuelve() {
        let m = pedir(DOS_FUNCIONES, "sequence");
        assert!(m.starts_with("sequenceDiagram"));
        assert!(m.contains("participant Usuario"));
        assert!(m.contains("Usuario->>+primera: invocar"));
        assert!(m.contains("primera->>+segunda: llamar"));
        assert!(m.contains("segunda-->>-primera: retornar"));
        assert!(m.contains("primera-->>-Usuario: resultado"));
    }

    /// Con una sola función no hay interacción que mostrar.
    #[test]
    fn una_sola_funcion_no_arma_una_secuencia_falsa() {
        let m = pedir("def sola():\n    pass\n", "sequence");
        assert!(m.contains("participant main"), "placeholder explicito: {m}");
    }

    /// **Un override no declara el mismo nodo dos veces.**
    ///
    /// `Animal.hablar` y `Perro.hablar` son dos funciones con el mismo
    /// nombre, y en el callgraph el nombre ES el id del nodo. Se vio en la
    /// verificación real: dos `hablar["hablar"]` seguidos.
    #[test]
    fn dos_metodos_homonimos_dan_un_solo_nodo_en_el_callgraph() {
        let src = "class A:\n    def hablar(self):\n        return 1\n\nclass B(A):\n    def hablar(self):\n        return 2\n";
        let m = pedir(src, "callgraph");
        assert_eq!(m.matches("hablar[\"hablar\"]").count(), 1, "un solo nodo: {m}");
    }

    /// **Y tampoco dos participantes iguales.** Mermaid los colapsa en uno y
    /// los pasos entre ellos se vuelven una autollamada que no existe.
    #[test]
    fn dos_metodos_homonimos_no_dan_dos_participantes_iguales() {
        let src = "class A:\n    def hablar(self):\n        return 1\n\nclass B(A):\n    def hablar(self):\n        return 2\n\ndef otra():\n    return 3\n";
        let m = pedir(src, "sequence");
        assert_eq!(m.matches("participant hablar").count(), 1, "un solo participante: {m}");
        // Y ninguna autollamada.
        assert!(!m.contains("hablar->>+hablar"), "autollamada inventada: {m}");
    }

    /// **En el flowchart sí son dos cajas**: ahí el id es `F0..Fn` por
    /// índice, no el nombre, así que dos funciones homónimas son dos
    /// funciones distintas y corresponde mostrarlas.
    #[test]
    fn en_el_flowchart_dos_homonimas_siguen_siendo_dos_cajas() {
        let src = "class A:\n    def hablar(self):\n        return 1\n\nclass B(A):\n    def hablar(self):\n        return 2\n";
        let m = pedir(src, "flowchart");
        assert_eq!(m.matches("[\"hablar()\"]").count(), 2, "dos cajas: {m}");
    }

    /// El orden de los nodos no puede depender del recorrido de un set: en
    /// Python era `list({...})`, y dos llamadas podían dar órdenes distintos.
    #[test]
    fn el_orden_de_los_nodos_es_estable_y_sigue_las_lineas() {
        let src = "def tercera():\n    pass\n\ndef primera():\n    pass\n\ndef segunda():\n    pass\n";
        let primero = pedir(src, "callgraph");
        assert_eq!(primero, pedir(src, "callgraph"), "dos llamadas, mismo resultado");
        let pos = |n: &str| primero.find(n).unwrap();
        assert!(pos("tercera") < pos("primera"), "orden por linea, no alfabetico: {primero}");
    }

    /// **Un nombre con una comilla doble rompería el Mermaid entero**, porque
    /// la etiqueta de un nodo va entre comillas dobles.
    #[test]
    fn un_docstring_con_comillas_no_rompe_el_diagrama() {
        let src = "def f():\n    \"\"\"dice \\\"hola\\\" y chau\"\"\"\n    return 1\n";
        let m = pedir(src, "flowchart");
        // Cada etiqueta tiene que tener un número PAR de comillas dobles:
        // las que la abren y la cierran, ninguna adentro.
        for linea in m.lines().filter(|l| l.contains('[')) {
            assert_eq!(
                linea.matches('"').count() % 2,
                0,
                "comillas desbalanceadas en: {linea}"
            );
        }
    }

    /// **Un id de nodo no tolera lo que una etiqueta sí.** En Python el
    /// nombre crudo se usaba como id, y funcionaba sólo porque los nombres de
    /// Python son `[A-Za-z0-9_]`.
    #[test]
    fn el_identificador_de_nodo_se_sanea() {
        assert_eq!(identificador("mi_funcion"), "mi_funcion");
        assert_eq!(identificador("con-guion"), "con_guion");
        assert_eq!(identificador("con.punto"), "con_punto");
        assert_eq!(identificador("9empieza"), "n9empieza", "Mermaid no acepta id con digito inicial");
    }

    #[test]
    fn un_docstring_con_acentos_no_se_corta_al_medio_de_un_caracter() {
        // 45 'á' — el corte a 40 caracteres cae donde un corte por byte
        // partiría el último en dos y dejaría UTF-8 inválido.
        let doc = "á".repeat(45);
        let src = format!("def f():\n    \"\"\"{doc}\"\"\"\n    return 1\n");
        let m = pedir(&src, "flowchart");
        assert!(m.contains(&"á".repeat(40)));
        assert!(!m.contains(&"á".repeat(41)), "tiene que cortar en 40 caracteres");
    }

    // ── Otros lenguajes ───────────────────────────────────────────────

    fn pedir_ext(nombre: &str, contenido: &str) -> String {
        let r = generar(&DiagramaRequest {
            filename: nombre.to_string(),
            content: contenido.to_string(),
            diagram_type: "flowchart".to_string(),
        });
        r["mermaid"].as_str().unwrap().to_string()
    }

    #[test]
    fn en_js_detecta_funciones_declaradas_y_asignadas() {
        let src = "function uno() {}\nconst dos = () => {}\nconst tres = async (x) => x\nconst cuatro = 42\n";
        let m = pedir_ext("app.js", src);
        for n in ["uno", "dos", "tres"] {
            assert!(m.contains(&format!("{n}()")), "falta {n}: {m}");
        }
        assert!(!m.contains("cuatro"), "`const x = 42` no es una funcion: {m}");
    }

    /// `if (...)` no es una declaración de función aunque matchee el patrón.
    #[test]
    fn en_js_no_confunde_palabras_clave_con_funciones() {
        let m = pedir_ext("app.js", "if (x) { }\nfor (;;) { }\nwhile (y) { }\n");
        assert!(m.contains("líneas · JS"), "sin funciones detectadas: {m}");
    }

    #[test]
    fn una_extension_sin_soporte_muestra_el_conteo_de_lineas() {
        let m = pedir_ext("datos.rb", "puts 1\nputs 2\nputs 3\n");
        assert!(m.contains("3 líneas · RB"), "{m}");
    }

    #[test]
    fn el_shape_de_la_respuesta_es_el_que_el_panel_espera() {
        let r = generar(&DiagramaRequest {
            filename: "a.py".to_string(),
            content: "def f():\n    pass\n".to_string(),
            diagram_type: "flowchart".to_string(),
        });
        assert_eq!(r["filename"], "a.py");
        assert_eq!(r["diagram_type"], "flowchart");
        assert!(r["mermaid"].as_str().unwrap().starts_with("flowchart TD"));
        assert!(r["ts"].as_str().unwrap().len() >= 19, "timestamp legible");
    }
}
