//! **El panel de ML — Fase 35, portado desde `routers/ml.py`.**
//!
//! Orquesta la detección que `servicio::ml` ya hacía (librerías, etapas del
//! pipeline, modelos, métricas, problemas) y le agrega las dos piezas que
//! vivían sólo en Python: el **diagrama Mermaid** del pipeline y el **score
//! con sugerencias**.
//!
//! **De las 742 líneas del router, 380 eran los cinco detectores
//! `_fallback`** —versiones en `ast`/regex de lo que `ml.rs` ya hacía de
//! verdad— que existían para cuando el sidecar no respondiera. Adentro del
//! motor esa rama es imposible, igual que en `diagrama.rs`: se borraron.
//!
//! **Y se borró un campo que informaba mal.** El router hacía:
//!
//! ```python
//! lib["version"] = _get_lib_version(lib["module"])
//! ```
//!
//! con el comentario *"`version` nunca puede calcularse en Rust — necesita
//! introspeccionar el propio entorno Python del backend"*. Ahí está el
//! problema: **ésa es la versión instalada en el backend, no la que usa el
//! proyecto analizado.** Alguien subiendo un proyecto que pinea `numpy==1.19`
//! veía la versión que tuviera el servidor, presentada como si fuera la suya.
//! No es una capacidad que se pierda al migrar: es una que estaba dando un
//! dato equivocado. La versión real del proyecto está en su manifest
//! (`requirements.txt`, `pyproject.toml`), que `lenguajes::manifest` ya sabe
//! leer — cuando se quiera mostrar de verdad, el dato sale de ahí.

use serde::Deserialize;
use serde_json::{json, Map, Value};

/// Cuántas etapas del pipeline entran en el diagrama.
const MAX_ETAPAS: usize = 10;
/// Cuántos modelos se dibujan.
const MAX_MODELOS: usize = 4;
/// Cuántas sugerencias se devuelven.
const MAX_SUGERENCIAS: usize = 8;

#[derive(Deserialize)]
pub struct MlRequest {
    #[serde(default = "archivo_por_defecto")]
    pub filename: String,
    #[serde(default)]
    pub content: String,
}

fn archivo_por_defecto() -> String {
    "script.py".to_string()
}

/// Escapa lo que rompe una etiqueta de Mermaid. Mismo criterio que
/// `servicio::diagrama`: la etiqueta va entre comillas dobles.
fn etiqueta(s: &str) -> String {
    s.replace('"', "'").replace(['\n', '\r'], " ")
}

// Los ids de nodo de este diagrama son `S0..Sn`/`M0..Mn`, generados acá: no
// salen de ningún nombre del código del usuario, así que no hace falta
// sanearlos como en `servicio::diagrama`.

/// Los colores de cada etapa, por familia.
///
/// Es la misma tabla del router, agrupada: entrenamiento en verde, evaluación
/// en violeta, preprocesamiento en azul, carga de datos en ámbar, el resto en
/// gris.
fn estilo_de_etapa(id: &str) -> (&'static str, &'static str) {
    match id {
        "entrenamiento" | "backprop" | "compilacion" => ("#0f3020", "#00f5a0"),
        "evaluacion" | "prediccion" => ("#1a1040", "#b87dff"),
        "limpieza" | "escalado" | "encoding" | "features" => ("#1a2040", "#3d9eff"),
        "carga_datos" | "dataloader" | "data_pipeline" => ("#0a1a40", "#ffb627"),
        _ => ("#1a1820", "#4a5880"),
    }
}

/// El diagrama Mermaid del pipeline.
pub fn diagrama(pipeline: &[Value], modelos: &[Value], filename: &str) -> String {
    if pipeline.is_empty() {
        return format!(
            "flowchart TD\n    A[{}]\n    B[Sin pipeline ML detectado]\n    A --> B",
            etiqueta(filename)
        );
    }
    let visibles = pipeline.len().min(MAX_ETAPAS);
    let mut l = vec![
        "flowchart TD".to_string(),
        format!("    START([Pipeline: {}])", etiqueta(filename)),
    ];

    let mut previo = "START".to_string();
    for (i, etapa) in pipeline.iter().take(visibles).enumerate() {
        let sid = format!("S{i}");
        let mut texto =
            etiqueta(etapa.get("description").and_then(Value::as_str).unwrap_or("etapa"));
        let veces = etapa.get("count").and_then(Value::as_u64).unwrap_or(1);
        if veces > 1 {
            texto.push_str(&format!("\\n({veces}x)"));
        }
        l.push(format!("    {sid}[\"{texto}\"]"));
        l.push(format!("    {previo} --> {sid}"));
        previo = sid;
    }

    // Los modelos cuelgan de la etapa de entrenamiento si existe, y si no del
    // último nodo — mismo criterio que antes.
    let nodo_de_entrenamiento = pipeline
        .iter()
        .take(visibles)
        .position(|e| e.get("id").and_then(Value::as_str) == Some("entrenamiento"))
        .map_or_else(|| previo.clone(), |i| format!("S{i}"));

    for (j, modelo) in modelos.iter().take(MAX_MODELOS).enumerate() {
        let mid = format!("M{j}");
        let nombre = etiqueta(modelo.get("name").and_then(Value::as_str).unwrap_or("modelo"));
        let tipo = etiqueta(modelo.get("type").and_then(Value::as_str).unwrap_or(""));
        l.push(format!("    {mid}([{nombre}\\n{tipo}])"));
        l.push(format!("    {nodo_de_entrenamiento} --> {mid}"));
    }

    l.push("    END([Fin])".to_string());
    l.push(format!("    {previo} --> END"));

    for (i, etapa) in pipeline.iter().take(visibles).enumerate() {
        let id = etapa.get("id").and_then(Value::as_str).unwrap_or("");
        let (fondo, borde) = estilo_de_etapa(id);
        l.push(format!("    style S{i} fill:{fondo},stroke:{borde},color:#c8d4f0"));
    }
    for j in 0..modelos.len().min(MAX_MODELOS) {
        l.push(format!("    style M{j} fill:#300a1a,stroke:#ff3366,color:#c8d4f0"));
    }
    l.push("    style START fill:#0a2040,stroke:#3d9eff,color:#c8d4f0".to_string());
    l.push("    style END fill:#0a2040,stroke:#00f5a0,color:#c8d4f0".to_string());
    l.join("\n")
}

fn tiene_etapa(pipeline: &[Value], id: &str) -> bool {
    pipeline.iter().any(|e| e.get("id").and_then(Value::as_str) == Some(id))
}

/// El score de 0 a 100 y las sugerencias.
///
/// **Es una heurística de buenas prácticas, no una medición**, y conviene
/// decirlo: resta por problemas encontrados y suma por señales de rigor
/// (hay evaluación, hay split, hay validación cruzada, hay semilla fija).
/// Los pesos son los del router, sin tocar: cambiarlos movería el número que
/// el panel viene mostrando sin que nadie lo pidiera.
pub fn score(contenido: &str, problemas: &[Value], pipeline: &[Value]) -> (i64, Vec<String>) {
    let mut puntos: i64 = 100;
    let severidad = |s: &str| {
        problemas
            .iter()
            .filter(|i| i.get("severity").and_then(Value::as_str) == Some(s))
            .count() as i64
    };
    puntos -= severidad("error") * 15;
    puntos -= severidad("warning") * 7;

    if tiene_etapa(pipeline, "evaluacion") {
        puntos += 5;
    }
    if tiene_etapa(pipeline, "split") {
        puntos += 5;
    }
    // Validación cruzada: la señal más fuerte de rigor del lote.
    if contenido.contains("cross_val_score") || contenido.contains("KFold") {
        puntos += 10;
    }
    // Una semilla fija es lo que hace reproducible un experimento.
    if contenido.contains("random_state") {
        puntos += 5;
    }
    if pipeline.len() >= 4 {
        puntos += 5;
    }

    let mut sugerencias: Vec<String> = Vec::new();
    if !tiene_etapa(pipeline, "evaluacion") {
        sugerencias.push("Agrega métricas de evaluación (accuracy_score, f1_score)".to_string());
    }
    if !tiene_etapa(pipeline, "guardado") {
        sugerencias.push("Guarda el modelo con joblib.dump() o model.save()".to_string());
    }
    if !tiene_etapa(pipeline, "limpieza") && !tiene_etapa(pipeline, "escalado") {
        sugerencias.push("Agrega preprocesamiento (StandardScaler, fillna)".to_string());
    }
    for p in problemas {
        if let Some(s) = p.get("suggestion").and_then(Value::as_str) {
            if !s.is_empty() {
                sugerencias.push(s.to_string());
            }
        }
    }
    // Sin repetidos y conservando el orden, igual que el
    // `dict.fromkeys` de Python.
    let mut vistas = std::collections::BTreeSet::new();
    sugerencias.retain(|s| vistas.insert(s.clone()));
    sugerencias.truncate(MAX_SUGERENCIAS);

    (puntos.clamp(0, 100), sugerencias)
}

pub fn analizar(req: &MlRequest) -> Value {
    let mut salida = Map::new();
    salida.insert("filename".into(), json!(req.filename));
    salida.insert("ts".into(), json!(crate::servicio::logstore::now_string()));

    let deteccion = crate::servicio::ml::detect(&req.content);
    let Some(d) = deteccion else {
        // `detect` devuelve `None` cuando el archivo no parsea. El router
        // reportaba el `SyntaxError` como un problema más de la lista, con
        // el resto del shape vacío; se conserva.
        let error = crate::servicio::rich::analyze_rich(&req.content)
            .error
            .unwrap_or_else(|| "el archivo no se pudo analizar".to_string());
        salida.insert("libraries".into(), json!([]));
        salida.insert("pipeline".into(), json!([]));
        salida.insert(
            "issues".into(),
            json!([{ "severity": "error", "message": format!("SyntaxError: {error}") }]),
        );
        salida.insert("metrics".into(), json!({}));
        salida.insert("models".into(), json!([]));
        salida.insert("diagram".into(), json!(diagrama(&[], &[], &req.filename)));
        salida.insert("score".into(), json!(0));
        salida.insert("suggestions".into(), json!([]));
        return Value::Object(salida);
    };

    let como_valor = serde_json::to_value(&d).unwrap_or_else(|_| json!({}));
    let lista = |k: &str| {
        como_valor.get(k).and_then(Value::as_array).cloned().unwrap_or_default()
    };
    let pipeline = lista("pipeline");
    let modelos = lista("models");
    let problemas = lista("issues");

    let (puntos, sugerencias) = score(&req.content, &problemas, &pipeline);

    salida.insert("libraries".into(), json!(lista("libraries")));
    salida.insert("pipeline".into(), json!(pipeline));
    salida.insert("models".into(), json!(modelos));
    salida.insert(
        "metrics".into(),
        como_valor.get("metrics").cloned().unwrap_or_else(|| json!({})),
    );
    salida.insert("issues".into(), json!(problemas));
    salida.insert(
        "diagram".into(),
        json!(diagrama(
            salida["pipeline"].as_array().unwrap_or(&Vec::new()),
            salida["models"].as_array().unwrap_or(&Vec::new()),
            &req.filename
        )),
    );
    salida.insert("score".into(), json!(puntos));
    salida.insert("suggestions".into(), json!(sugerencias));
    Value::Object(salida)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn etapa(id: &str, desc: &str) -> Value {
        json!({ "id": id, "description": desc, "count": 1 })
    }

    fn pedir(contenido: &str) -> Value {
        analizar(&MlRequest { filename: "modelo.py".into(), content: contenido.into() })
    }

    const PIPELINE_SKLEARN: &str = r#"
import pandas as pd
from sklearn.model_selection import train_test_split, cross_val_score
from sklearn.preprocessing import StandardScaler
from sklearn.ensemble import RandomForestClassifier
from sklearn.metrics import accuracy_score
import joblib

df = pd.read_csv("datos.csv")
df = df.fillna(0)
X_train, X_test, y_train, y_test = train_test_split(df, df.y, random_state=42)
escalador = StandardScaler()
X_train = escalador.fit_transform(X_train)
modelo = RandomForestClassifier(random_state=42)
modelo.fit(X_train, y_train)
pred = modelo.predict(X_test)
print(accuracy_score(y_test, pred))
joblib.dump(modelo, "modelo.pkl")
"#;

    #[test]
    fn detecta_el_pipeline_y_arma_el_diagrama() {
        let r = pedir(PIPELINE_SKLEARN);
        assert!(!r["libraries"].as_array().unwrap().is_empty(), "tiene que ver sklearn/pandas");
        assert!(!r["pipeline"].as_array().unwrap().is_empty(), "y las etapas");
        let m = r["diagram"].as_str().unwrap();
        assert!(m.starts_with("flowchart TD"));
        assert!(m.contains("START([Pipeline: modelo.py])"));
        assert!(m.contains("--> END"));
    }

    #[test]
    fn el_shape_es_el_que_el_panel_espera() {
        let r = pedir(PIPELINE_SKLEARN);
        for clave in [
            "filename", "ts", "libraries", "pipeline", "issues", "metrics", "models", "diagram",
            "score", "suggestions",
        ] {
            assert!(r.get(clave).is_some(), "falta {clave}");
        }
        assert_eq!(r["filename"], "modelo.py");
    }

    /// **El campo `version` ya no está**, y no es una pérdida: reportaba la
    /// versión instalada en el backend, no la que usa el proyecto analizado.
    #[test]
    fn las_librerias_no_traen_una_version_del_backend() {
        let r = pedir(PIPELINE_SKLEARN);
        for lib in r["libraries"].as_array().unwrap() {
            assert!(
                lib.get("version").is_none(),
                "la version del backend no es la del proyecto analizado: {lib}"
            );
        }
    }

    #[test]
    fn un_archivo_sin_ml_no_arma_pipeline() {
        let r = pedir("def sumar(a, b):\n    return a + b\n");
        assert!(r["pipeline"].as_array().unwrap().is_empty());
        assert!(r["diagram"].as_str().unwrap().contains("Sin pipeline ML detectado"));
    }

    #[test]
    fn un_archivo_roto_reporta_el_error_como_problema() {
        let r = pedir("import sklearn\ndef roto(:\n    pass\n");
        let issues = r["issues"].as_array().unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0]["severity"], "error");
        assert!(issues[0]["message"].as_str().unwrap().contains("SyntaxError"));
        assert_eq!(r["score"], 0);
        // Y el shape sigue completo: el panel no se queda sin campos.
        assert!(r["diagram"].as_str().unwrap().starts_with("flowchart TD"));
    }

    // ── el score ──────────────────────────────────────────────────────

    #[test]
    fn un_pipeline_completo_puntua_alto() {
        let pipeline = vec![
            etapa("carga_datos", "carga"),
            etapa("limpieza", "limpieza"),
            etapa("split", "split"),
            etapa("entrenamiento", "fit"),
            etapa("evaluacion", "accuracy"),
            etapa("guardado", "dump"),
        ];
        let (p, s) = score("cross_val_score(m, X, y)\nrandom_state=42", &[], &pipeline);
        assert_eq!(p, 100, "100 es el techo");
        assert!(s.is_empty(), "sin sugerencias: {s:?}");
    }

    #[test]
    fn los_errores_restan_mas_que_los_avisos() {
        let con_error = score("", &[json!({"severity": "error"})], &[]).0;
        let con_aviso = score("", &[json!({"severity": "warning"})], &[]).0;
        assert!(con_error < con_aviso, "error -15, warning -7: {con_error} vs {con_aviso}");
    }

    /// El score se acota a `[0, 100]`: ni un archivo pésimo da negativo ni
    /// uno excelente pasa de 100.
    #[test]
    fn el_score_se_acota() {
        let muchos: Vec<Value> = (0..20).map(|_| json!({"severity": "error"})).collect();
        assert_eq!(score("", &muchos, &[]).0, 0, "no puede ser negativo");
        let completo = vec![
            etapa("evaluacion", "e"),
            etapa("split", "s"),
            etapa("limpieza", "l"),
            etapa("guardado", "g"),
        ];
        assert_eq!(score("cross_val_score KFold random_state", &[], &completo).0, 100);
    }

    #[test]
    fn sugiere_lo_que_falta() {
        let (_, s) = score("", &[], &[etapa("entrenamiento", "fit")]);
        assert!(s.iter().any(|x| x.contains("evaluación")), "{s:?}");
        assert!(s.iter().any(|x| x.contains("joblib")), "{s:?}");
        assert!(s.iter().any(|x| x.contains("preprocesamiento")), "{s:?}");
    }

    /// Las sugerencias de los problemas se suman, **sin repetir**: dos
    /// problemas con la misma sugerencia dan una sola línea.
    #[test]
    fn las_sugerencias_no_se_repiten() {
        let problemas = vec![
            json!({"severity": "warning", "suggestion": "usa random_state"}),
            json!({"severity": "warning", "suggestion": "usa random_state"}),
        ];
        let completo = vec![etapa("evaluacion", "e"), etapa("guardado", "g"), etapa("limpieza", "l")];
        let (_, s) = score("", &problemas, &completo);
        assert_eq!(s, vec!["usa random_state"]);
    }

    #[test]
    fn las_sugerencias_se_topean() {
        let problemas: Vec<Value> = (0..20)
            .map(|i| json!({"severity": "warning", "suggestion": format!("arregla {i}")}))
            .collect();
        let (_, s) = score("", &problemas, &[]);
        assert_eq!(s.len(), MAX_SUGERENCIAS);
    }

    // ── el diagrama ───────────────────────────────────────────────────

    #[test]
    fn el_diagrama_encadena_las_etapas_en_orden() {
        let p = vec![etapa("carga_datos", "leer csv"), etapa("entrenamiento", "fit")];
        let m = diagrama(&p, &[], "x.py");
        assert!(m.contains("START --> S0"));
        assert!(m.contains("S0 --> S1"));
        assert!(m.contains("S1 --> END"));
    }

    /// **Los modelos cuelgan de la etapa de entrenamiento** si existe.
    #[test]
    fn los_modelos_cuelgan_del_entrenamiento() {
        let p = vec![etapa("carga_datos", "carga"), etapa("entrenamiento", "fit")];
        let modelos = vec![json!({"name": "RandomForest", "type": "ensemble"})];
        let m = diagrama(&p, &modelos, "x.py");
        assert!(m.contains("S1 --> M0"), "del nodo de entrenamiento: {m}");
        assert!(m.contains("RandomForest"));
    }

    /// Y si no hay etapa de entrenamiento, del último nodo — no se pierden.
    #[test]
    fn sin_entrenamiento_los_modelos_cuelgan_del_ultimo() {
        let p = vec![etapa("carga_datos", "carga")];
        let modelos = vec![json!({"name": "M", "type": "t"})];
        let m = diagrama(&p, &modelos, "x.py");
        assert!(m.contains("S0 --> M0"), "{m}");
    }

    #[test]
    fn el_contador_de_repeticiones_se_muestra() {
        let p = vec![json!({"id": "limpieza", "description": "fillna", "count": 3})];
        assert!(diagrama(&p, &[], "x.py").contains("(3x)"));
    }

    /// Una comilla doble en una descripción rompería el Mermaid entero.
    #[test]
    fn una_comilla_en_la_descripcion_no_rompe_el_diagrama() {
        let p = vec![etapa("limpieza", "llama a df[\"col\"]")];
        let m = diagrama(&p, &[], "x.py");
        for linea in m.lines().filter(|l| l.contains('[')) {
            assert_eq!(linea.matches('"').count() % 2, 0, "desbalanceadas: {linea}");
        }
    }

    #[test]
    fn el_diagrama_topea_las_etapas_y_los_modelos() {
        let p: Vec<Value> = (0..30).map(|i| etapa("otra", &format!("etapa {i}"))).collect();
        let modelos: Vec<Value> = (0..10).map(|i| json!({"name": format!("M{i}"), "type": "t"})).collect();
        let m = diagrama(&p, &modelos, "x.py");
        assert!(m.contains(&format!("S{}", MAX_ETAPAS - 1)));
        assert!(!m.contains(&format!("S{MAX_ETAPAS}")), "no pasa del tope");
        assert!(!m.contains(&format!("M{MAX_MODELOS}")));
    }

    #[test]
    fn cada_familia_de_etapa_tiene_su_color() {
        let p = vec![
            etapa("entrenamiento", "fit"),
            etapa("evaluacion", "score"),
            etapa("limpieza", "fillna"),
            etapa("carga_datos", "read_csv"),
            etapa("cualquiera", "otra"),
        ];
        let m = diagrama(&p, &[], "x.py");
        for color in ["#00f5a0", "#b87dff", "#3d9eff", "#ffb627", "#4a5880"] {
            assert!(m.contains(color), "falta el color {color}: {m}");
        }
    }
}
