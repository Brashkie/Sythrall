//! Reducción de Python (mandato "Rust es lo principal") — primera mitad de
//! `apps/api/routers/ml.py`: detección estructurada (qué librerías/etapas de
//! pipeline/modelos/métricas aparecen en un archivo), portada literal desde
//! las 4 tablas de patrones que `ml.py` ya tenía (`ML_LIB_MAP`/
//! `PIPELINE_PATTERNS`/`MODEL_PATTERNS`/`METRIC_PATTERNS`). Genuinamente
//! nuevo en Rust, no un cutover de algo ya duplicado — a diferencia de
//! Heatmap/`diagram.py`, esto nunca tuvo un consumidor Rust que reemplazar.
//! `detect_ml_issues` (las 21 reglas heurísticas de `ml.py`, portadas acá
//! también) corre sobre el mismo `Vec<MlLibrary>` que `detect_ml_libraries`
//! ya calculó en la misma llamada — nunca lee `version`, así que es seguro
//! correrla antes de que Python enriquezca esa lista.
//!
//! Una pieza que NUNCA puede vivir acá: la versión instalada de cada
//! librería (`_get_lib_version` en `shared.py`) necesita introspeccionar el
//! propio entorno Python del backend (`importlib.import_module`), algo que
//! Rust no tiene cómo hacer. Este módulo expone `module` (el nombre base)
//! por librería para que Python haga ese lookup como paso de
//! post-procesamiento — no vive acá.

use std::collections::{HashMap, HashSet};

use regex::Regex;

/// Regex compilada UNA vez por sitio de uso, no una vez por llamada.
///
/// `detect_ml_issues` tenía 35 `Regex::new(...)` sueltas y las tres tablas de
/// patrones compilaban las suyas dentro del bucle: 62+ compilaciones de regex
/// por archivo analizado, y este módulo corre por archivo. Compilar una regex
/// no es barato (parsea el patrón y construye el autómata), así que era el
/// costo dominante de `detect`, muy por encima del matcheo en sí.
///
/// Se resuelve con un `OnceLock` por sitio en vez de una tabla global de
/// patrones: así el patrón sigue escrito donde se usa. Una tabla paralela
/// "nombre → patrón" es justo la clase de estructura que se desincroniza en
/// silencio, y este repo ya arregló ese modo de falla varias veces.
macro_rules! re {
    ($pattern:expr) => {{
        static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        RE.get_or_init(|| Regex::new($pattern).expect("patrón inválido en ml.rs"))
    }};
}

use rustpython_parser::ast::{Expr, Stmt};
use serde::Serialize;

use crate::ast::parser::parse_module;
use crate::ast::walk::walk_stmts;

#[derive(Serialize, Clone)]
pub struct MlLibrary {
    pub name: String,
    pub category: &'static str,
    pub color: &'static str,
    pub import: String,
    pub alias: Option<String>,
    pub module: String,
}

#[derive(Serialize, Clone)]
pub struct PipelineStage {
    pub id: &'static str,
    pub description: &'static str,
    pub line: usize,
    pub count: usize,
}

#[derive(Serialize, Clone)]
pub struct DetectedModel {
    pub name: &'static str,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub family: &'static str,
    pub framework: &'static str,
    pub line: usize,
}

#[derive(Serialize, Clone)]
pub struct MetricInfo {
    pub found: bool,
    pub count: usize,
    pub value: Option<f64>,
}

#[derive(Serialize, Clone, Debug)]
pub struct MlIssue {
    pub severity: &'static str,
    pub category: &'static str,
    pub message: String,
    pub suggestion: &'static str,
}

#[derive(Serialize)]
pub struct MlDetectionResult {
    pub libraries: Vec<MlLibrary>,
    pub pipeline: Vec<PipelineStage>,
    pub models: Vec<DetectedModel>,
    pub metrics: HashMap<String, MetricInfo>,
    pub issues: Vec<MlIssue>,
}

struct LibInfo {
    name: &'static str,
    category: &'static str,
    color: &'static str,
}

/// Puerto literal de `ML_LIB_MAP` (`ml.py`) — mismas 23 entradas, mismo
/// orden. Nota deliberada: "numpy" y "np" (mismo patrón para pandas/pd,
/// tensorflow/tf) apuntan al mismo `name` canónico a propósito — ver
/// `detect_ml_libraries` para el dedup que evita contarlas dos veces.
const ML_LIB_MAP: &[(&str, LibInfo)] = &[
    ("numpy", LibInfo { name: "NumPy", category: "datos", color: "#4d9fff" }),
    ("np", LibInfo { name: "NumPy", category: "datos", color: "#4d9fff" }),
    ("pandas", LibInfo { name: "Pandas", category: "datos", color: "#4d9fff" }),
    ("pd", LibInfo { name: "Pandas", category: "datos", color: "#4d9fff" }),
    ("sklearn", LibInfo { name: "Scikit-learn", category: "ml", color: "#f5a623" }),
    ("torch", LibInfo { name: "PyTorch", category: "dl", color: "#ee4c2c" }),
    ("torchvision", LibInfo { name: "TorchVision", category: "dl", color: "#ee4c2c" }),
    ("tensorflow", LibInfo { name: "TensorFlow", category: "dl", color: "#ff6b35" }),
    ("tf", LibInfo { name: "TensorFlow", category: "dl", color: "#ff6b35" }),
    ("keras", LibInfo { name: "Keras", category: "dl", color: "#d00000" }),
    ("matplotlib", LibInfo { name: "Matplotlib", category: "viz", color: "#00c07a" }),
    ("plt", LibInfo { name: "Matplotlib", category: "viz", color: "#00c07a" }),
    ("seaborn", LibInfo { name: "Seaborn", category: "viz", color: "#00c07a" }),
    ("sns", LibInfo { name: "Seaborn", category: "viz", color: "#00c07a" }),
    ("xgboost", LibInfo { name: "XGBoost", category: "ml", color: "#f5a623" }),
    ("lightgbm", LibInfo { name: "LightGBM", category: "ml", color: "#f5a623" }),
    ("cv2", LibInfo { name: "OpenCV", category: "vision", color: "#b87dff" }),
    ("PIL", LibInfo { name: "Pillow", category: "vision", color: "#b87dff" }),
    ("transformers", LibInfo { name: "HuggingFace", category: "nlp", color: "#ffdd57" }),
    ("nltk", LibInfo { name: "NLTK", category: "nlp", color: "#ffdd57" }),
    ("spacy", LibInfo { name: "spaCy", category: "nlp", color: "#ffdd57" }),
    ("scipy", LibInfo { name: "SciPy", category: "ciencia", color: "#4d9fff" }),
    ("cython", LibInfo { name: "Cython", category: "rendimiento", color: "#ffd43b" }),
    ("pyximport", LibInfo { name: "Cython", category: "rendimiento", color: "#ffd43b" }),
    ("cython.parallel", LibInfo { name: "Cython Parallel", category: "rendimiento", color: "#ffd43b" }),
    // ── Fase 32 (2026-09-24): SDKs cuánticos ────────────────────────────
    // Categoría propia y NO `dl`: un circuito cuántico no es una red
    // neuronal, y meterlo bajo "deep learning" porque el panel se llama
    // ML/DL sería etiquetar mal para no agregar una categoría.
    ("qiskit", LibInfo { name: "Qiskit", category: "cuantico", color: "#6929c4" }),
    ("cirq", LibInfo { name: "Cirq", category: "cuantico", color: "#6929c4" }),
    ("pennylane", LibInfo { name: "PennyLane", category: "cuantico", color: "#6929c4" }),
    ("braket", LibInfo { name: "Amazon Braket", category: "cuantico", color: "#6929c4" }),
    ("pyquil", LibInfo { name: "pyQuil (Rigetti)", category: "cuantico", color: "#6929c4" }),
    ("pytket", LibInfo { name: "pytket (Quantinuum)", category: "cuantico", color: "#6929c4" }),
    ("qsharp", LibInfo { name: "Q#", category: "cuantico", color: "#6929c4" }),
    ("qutip", LibInfo { name: "QuTiP", category: "cuantico", color: "#6929c4" }),
    ("projectq", LibInfo { name: "ProjectQ", category: "cuantico", color: "#6929c4" }),
    ("qulacs", LibInfo { name: "Qulacs", category: "cuantico", color: "#6929c4" }),
    ("strawberryfields", LibInfo { name: "Strawberry Fields", category: "cuantico", color: "#6929c4" }),

    // ── Fase 32: criptografía post-cuántica ─────────────────────────────
    ("pqcrypto", LibInfo { name: "pqcrypto", category: "pqc", color: "#00b8a9" }),
    ("pqclean", LibInfo { name: "PQClean", category: "pqc", color: "#00b8a9" }),
    ("oqs", LibInfo { name: "liboqs (Open Quantum Safe)", category: "pqc", color: "#00b8a9" }),
    ("liboqs", LibInfo { name: "liboqs (Open Quantum Safe)", category: "pqc", color: "#00b8a9" }),
    ("kyber", LibInfo { name: "Kyber / ML-KEM", category: "pqc", color: "#00b8a9" }),
    ("dilithium", LibInfo { name: "Dilithium / ML-DSA", category: "pqc", color: "#00b8a9" }),
    ("sphincs", LibInfo { name: "SPHINCS+ / SLH-DSA", category: "pqc", color: "#00b8a9" }),
    // `falcon` NO está en esta tabla, y es una omisión deliberada: colisiona
    // de frente con Falcon, el framework web de Python, y desde un `import
    // falcon` no hay forma de distinguirlos. Etiquetar una API REST como
    // criptografía post-cuántica es peor que no detectar el esquema.
    ("sike", LibInfo { name: "SIKE", category: "pqc", color: "#00b8a9" }),

    // ── Fase 32: renderizado neuronal ───────────────────────────────────
    ("nerfstudio", LibInfo { name: "Nerfstudio", category: "render", color: "#ff8c42" }),
    ("gsplat", LibInfo { name: "gsplat (3D Gaussian Splatting)", category: "render", color: "#ff8c42" }),
    ("tinycudann", LibInfo { name: "tiny-cuda-nn", category: "render", color: "#ff8c42" }),
    ("nvdiffrast", LibInfo { name: "nvdiffrast", category: "render", color: "#ff8c42" }),
    ("pytorch3d", LibInfo { name: "PyTorch3D", category: "render", color: "#ff8c42" }),
    ("kaolin", LibInfo { name: "NVIDIA Kaolin", category: "render", color: "#ff8c42" }),
    ("mitsuba", LibInfo { name: "Mitsuba 3", category: "render", color: "#ff8c42" }),
    ("threestudio", LibInfo { name: "threestudio", category: "render", color: "#ff8c42" }),
];

fn find_lib_info(key: &str) -> Option<&'static LibInfo> {
    ML_LIB_MAP.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
}

/// Puerto 1:1 de `_detect_ml_libraries` (`ml.py`) — el dedup es por NOMBRE
/// CANÓNICO (`info.name`, ej. "NumPy"), no por el alias que matcheó: sin
/// esto, `import numpy as np` pasaría dos veces (una vez con "numpy", otra
/// con "np") y se contaría dos veces — bug real ya documentado y arreglado
/// del lado Python, replicado acá a propósito, no una versión "más limpia".
pub fn detect_ml_libraries(suite: &[Stmt]) -> Vec<MlLibrary> {
    let mut found: Vec<MlLibrary> = Vec::new();
    let mut seen_canonical: HashSet<&'static str> = HashSet::new();

    let mut on_stmt = |stmt: &Stmt| match stmt {
        Stmt::Import(s) => {
            for alias in &s.names {
                let full = alias.name.to_string();
                let base = full.split('.').next().unwrap_or(&full).to_string();
                let key = alias.asname.as_ref().map(|a| a.to_string()).unwrap_or_else(|| base.clone());
                for candidate in [full.as_str(), base.as_str(), key.as_str()] {
                    if let Some(info) = find_lib_info(candidate) {
                        if seen_canonical.insert(info.name) {
                            found.push(MlLibrary {
                                name: info.name.to_string(),
                                category: info.category,
                                color: info.color,
                                import: full.clone(),
                                alias: alias.asname.as_ref().map(|a| a.to_string()),
                                module: base.clone(),
                            });
                        }
                        break;
                    }
                }
            }
        }
        Stmt::ImportFrom(s) => {
            let module = s.module.as_ref().map(|m| m.to_string()).unwrap_or_default();
            let base = module.split('.').next().unwrap_or(&module).to_string();
            for candidate in [module.as_str(), base.as_str()] {
                if let Some(info) = find_lib_info(candidate) {
                    if seen_canonical.insert(info.name) {
                        found.push(MlLibrary {
                            name: info.name.to_string(),
                            category: info.category,
                            color: info.color,
                            import: format!("from {module} import ..."),
                            alias: None,
                            module: base.clone(),
                        });
                    }
                    break;
                }
            }
        }
        _ => {}
    };
    let mut on_expr = |_: &Expr| {};
    walk_stmts(suite, &mut on_stmt, &mut on_expr);
    found
}

struct PipelinePattern {
    pattern: &'static str,
    id: &'static str,
    description: &'static str,
}

/// Puerto literal de `PIPELINE_PATTERNS` (`ml.py`) — mismas 23 entradas,
/// mismo orden, mismos regex.
/// Compiladas una vez, en paralelo a la tabla de arriba — ver el macro `re!`.
static PIPELINE_RES: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
    PIPELINE_PATTERNS.iter().map(|p| Regex::new(p.pattern).expect("patrón de pipeline inválido")).collect()
});

const PIPELINE_PATTERNS: &[PipelinePattern] = &[
    PipelinePattern {
        pattern: r"\bpd\.read_csv\b|\bpd\.read_excel\b|\bpd\.read_json\b|\bpd\.read_parquet\b|\bpd\.read_sql\b",
        id: "carga_datos",
        description: "Carga de datos",
    },
    PipelinePattern { pattern: r"\bdropna\b|\bfillna\b|\bdrop_duplicates\b|SimpleImputer", id: "limpieza", description: "Limpieza de datos" },
    PipelinePattern {
        pattern: r"\bLabelEncoder\b|\bOneHotEncoder\b|\bget_dummies\b|\bOrdinalEncoder\b",
        id: "encoding",
        description: "Encoding categórico",
    },
    PipelinePattern {
        pattern: r"\bStandardScaler\b|\bMinMaxScaler\b|\bRobustScaler\b|\bnormalize\b",
        id: "escalado",
        description: "Escalado/Normalización",
    },
    PipelinePattern {
        pattern: r"\btrain_test_split\b|\bKFold\b|\bStratifiedKFold\b|\bcross_val_score\b",
        id: "split",
        description: "División train/test",
    },
    PipelinePattern { pattern: r"\bPCA\b|\bTSNE\b|\bSelectKBest\b|feature_selection", id: "features", description: "Selección de features" },
    PipelinePattern {
        pattern: r"\bnn\.Module\b|\bnn\.Sequential\b|\bnn\.Linear\b|\bnn\.Conv2d\b",
        id: "arquitectura",
        description: "Definición de arquitectura",
    },
    PipelinePattern {
        pattern: r"\bmodel\.compile\b|\boptim\.\w+\(|\bAdam\b|\bSGD\b|\bAdamW\b",
        id: "optimizador",
        description: "Configuración del optimizador",
    },
    PipelinePattern {
        pattern: r"\bDataLoader\b|\bDataset\b|\bImageDataGenerator\b|\btf\.data\.Dataset\b",
        id: "dataloader",
        description: "Pipeline de datos",
    },
    PipelinePattern {
        pattern: r"\bfit\s*\(|\bfit_transform\s*\(|\bmodel\.fit\b|\bloss\.backward\(\)",
        id: "entrenamiento",
        description: "Entrenamiento del modelo",
    },
    PipelinePattern {
        pattern: r"\bEarlyStopping\b|\bModelCheckpoint\b|\bCallbacks\b|\bTensorBoard\b",
        id: "callbacks",
        description: "Callbacks de entrenamiento",
    },
    PipelinePattern { pattern: r"\bpredict\s*\(|\bpredict_proba\s*\(|\bmodel\.predict\b", id: "prediccion", description: "Predicción/Inferencia" },
    PipelinePattern {
        pattern: r"\baccuracy_score\b|\bconfusion_matrix\b|\bclassification_report\b|\br2_score\b|\bmean_squared_error\b|\bf1_score\b|\broc_auc_score\b",
        id: "evaluacion",
        description: "Evaluación del modelo",
    },
    PipelinePattern {
        pattern: r"\bjoblib\.dump\b|\bpickle\.dump\b|\bmodel\.save\b|\btorch\.save\b",
        id: "guardado",
        description: "Guardado del modelo",
    },
    PipelinePattern {
        pattern: r"\bplt\.plot\b|\bplt\.show\b|\bsns\.heatmap\b|\bplt\.figure\b",
        id: "visualizacion",
        description: "Visualización Matplotlib",
    },
    PipelinePattern {
        pattern: r"\bpx\.scatter\b|\bpx\.line\b|\bpx\.bar\b|\bgo\.Figure\b|\bplotly\.",
        id: "viz_interactiva",
        description: "Visualización interactiva Plotly",
    },
    PipelinePattern {
        pattern: r"\bcv2\.imread\b|\bcv2\.resize\b|\bcv2\.cvtColor\b|\bcv2\.VideoCapture\b",
        id: "vision",
        description: "Procesamiento de imágenes OpenCV",
    },
    PipelinePattern {
        pattern: r"\bscipy\.stats\b|\bscipy\.optimize\b|\bscipy\.signal\b|\bscipy\.linalg\b",
        id: "scipy_calc",
        description: "Cálculo científico SciPy",
    },
    PipelinePattern { pattern: r"\bic\s*\(|from icecream import", id: "debugging", description: "Debug con IceCream" },
    PipelinePattern { pattern: r"\bpl\.read_csv\b|\bpl\.read_parquet\b|\bpl\.DataFrame\b", id: "carga_polars", description: "Carga datos Polars" },
    PipelinePattern { pattern: r"\blgb\.train\b|\bLGBMClassifier\b|\bLGBMRegressor\b", id: "lightgbm_train", description: "Modelo LightGBM" },
    PipelinePattern { pattern: r"\bspacy\.load\b|nlp\s*=\s*spacy", id: "nlp_spacy", description: "Procesamiento NLP spaCy" },
    PipelinePattern {
        pattern: r"\bcdef\s|\bcpdef\s|\bctypedef\s|\bpyximport\b|cimport\b",
        id: "cython_compile",
        description: "Compilación Cython",
    },
];

/// Puerto literal de `_detect_pipeline` (`ml.py`).
pub fn detect_pipeline(content: &str) -> Vec<PipelineStage> {
    let mut steps: Vec<PipelineStage> = Vec::new();
    let mut seen: HashSet<&'static str> = HashSet::new();
    for (p, re) in PIPELINE_PATTERNS.iter().zip(PIPELINE_RES.iter()) {
        let matches: Vec<_> = re.find_iter(content).collect();
        if !matches.is_empty() && seen.insert(p.id) {
            let line = content[..matches[0].start()].matches('\n').count() + 1;
            steps.push(PipelineStage { id: p.id, description: p.description, line, count: matches.len() });
        }
    }
    steps.sort_by_key(|s| s.line);
    steps
}

struct ModelPattern {
    name: &'static str,
    kind: &'static str,
    family: &'static str,
    framework: &'static str,
}

/// Puerto literal de `MODEL_PATTERNS` (`ml.py`) — mismas 25 entradas, mismo
/// orden.
/// Compiladas una vez — el patrón se arma escapando el nombre, así que no puede ser un literal y no sirve el macro `re!`.
static MODEL_RES: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
    MODEL_PATTERNS.iter().map(|m| Regex::new(&format!(r"\b{}\b", regex::escape(m.name))).expect("patrón de modelo inválido")).collect()
});

const MODEL_PATTERNS: &[ModelPattern] = &[
    ModelPattern { name: "RandomForestClassifier", kind: "clasificación", family: "ensemble", framework: "sklearn" },
    ModelPattern { name: "RandomForestRegressor", kind: "regresión", family: "ensemble", framework: "sklearn" },
    ModelPattern { name: "GradientBoostingClassifier", kind: "clasificación", family: "boosting", framework: "sklearn" },
    ModelPattern { name: "LogisticRegression", kind: "clasificación", family: "lineal", framework: "sklearn" },
    ModelPattern { name: "LinearRegression", kind: "regresión", family: "lineal", framework: "sklearn" },
    ModelPattern { name: "SVC", kind: "clasificación", family: "svm", framework: "sklearn" },
    ModelPattern { name: "KMeans", kind: "clustering", family: "clustering", framework: "sklearn" },
    ModelPattern { name: "DecisionTreeClassifier", kind: "clasificación", family: "árbol", framework: "sklearn" },
    ModelPattern { name: "XGBClassifier", kind: "clasificación", family: "boosting", framework: "xgboost" },
    ModelPattern { name: "LGBMClassifier", kind: "clasificación", family: "boosting", framework: "lightgbm" },
    ModelPattern { name: "LGBMRegressor", kind: "regresión", family: "boosting", framework: "lightgbm" },
    ModelPattern { name: "Linear", kind: "capa densa", family: "linear", framework: "pytorch" },
    ModelPattern { name: "Conv2d", kind: "capa conv", family: "cnn", framework: "pytorch" },
    ModelPattern { name: "LSTM", kind: "recurrente", family: "rnn", framework: "pytorch/keras" },
    ModelPattern { name: "GRU", kind: "recurrente", family: "rnn", framework: "pytorch/keras" },
    ModelPattern { name: "Transformer", kind: "transformer", family: "attention", framework: "pytorch" },
    ModelPattern { name: "Dense", kind: "capa densa", family: "linear", framework: "keras" },
    ModelPattern { name: "Conv2D", kind: "capa conv", family: "cnn", framework: "keras" },
    ModelPattern { name: "Dropout", kind: "regularización", family: "dropout", framework: "keras" },
    ModelPattern { name: "BatchNormalization", kind: "normalización", family: "batchnorm", framework: "keras" },
    ModelPattern { name: "Embedding", kind: "embedding", family: "nlp", framework: "keras" },
    ModelPattern { name: "MultiHeadAttention", kind: "atención", family: "transformer", framework: "keras" },
    ModelPattern { name: "BertModel", kind: "BERT", family: "transformer", framework: "transformers" },
    ModelPattern { name: "GPT2Model", kind: "GPT-2", family: "transformer", framework: "transformers" },
    ModelPattern { name: "AutoModel", kind: "transformer", family: "pretrained", framework: "transformers" },
];

/// Puerto de `_detect_models` (`ml.py`) — el `seen` de Python era redundante
/// (`MODEL_PATTERNS.items()` ya itera cada nombre una sola vez, un dict no
/// tiene claves repetidas), así que no hace falta replicarlo acá.
pub fn detect_models(content: &str) -> Vec<DetectedModel> {
    let mut found = Vec::new();
    for (m, re) in MODEL_PATTERNS.iter().zip(MODEL_RES.iter()) {
        if let Some(mat) = re.find(content) {
            let line = content[..mat.start()].matches('\n').count() + 1;
            found.push(DetectedModel { name: m.name, kind: m.kind, family: m.family, framework: m.framework, line });
        }
    }
    found
}

/// Puerto literal de `METRIC_PATTERNS` (`ml.py`) — mismas 14 entradas.
/// Compiladas una vez, dos por métrica: la de PRESENCIA (`(?i)<patrón>`) y una
/// de VALOR por cada alternativa del patrón. El bucle compilaba las dos en
/// cada llamada, y las de valor una por alternativa — el peor de los tres
/// casos. Una alternativa que no compila se descarta acá, igual que antes la
/// descartaba el `Err(_) => continue` del bucle.
type MetricRegexes = (Regex, Vec<Regex>);

static METRIC_RES: std::sync::LazyLock<Vec<MetricRegexes>> = std::sync::LazyLock::new(|| {
    METRIC_PATTERNS
        .iter()
        .map(|(_, pattern)| {
            let presencia = Regex::new(&format!("(?i){pattern}")).expect("patrón de métrica inválido");
            let valores = pattern
                .split('|')
                .filter_map(|alt| Regex::new(&format!(r"(?i){}\s*[=:]\s*([\d]+\.?[\d]*)", alt.trim())).ok())
                .collect();
            (presencia, valores)
        })
        .collect()
});

const METRIC_PATTERNS: &[(&str, &str)] = &[
    ("accuracy", r"\baccuracy_score\b|\baccuracy\b|\bacc\b"),
    ("loss", r"\bloss\b"),
    ("val_loss", r"\bval_loss\b"),
    ("val_accuracy", r"\bval_accuracy\b|\bval_acc\b"),
    ("mae", r"\bmae\b|\bmean_absolute_error\b"),
    ("mse", r"\bmse\b|\bmean_squared_error\b"),
    ("r2", r"\br2_score\b|\br2\b"),
    ("f1", r"\bf1_score\b|\bf1\b"),
    ("auc", r"\broc_auc_score\b|\bauc\b"),
    ("precision", r"\bprecision_score\b|\bprecision\b"),
    ("recall", r"\brecall_score\b|\brecall\b"),
    ("epochs", r"\bepochs\b"),
    ("batch_size", r"\bbatch_size\b"),
    ("learning_rate", r"\blearning_rate\b|\blr\b"),
];

/// Puerto literal de `_detect_metrics` (`ml.py`), incluyendo el sub-paso de
/// extracción de valor: cada alternativa del patrón (separadas por `|`) se
/// prueba por separado con un sufijo `\s*[=:]\s*([\d]+\.?[\d]*)` — la
/// primera que matchea un valor numérico gana. Puerto literal a propósito,
/// no una reescritura "más limpia": el comportamiento exacto (qué valor
/// gana si hay varias alternativas) tiene que calzar con el Python que
/// reemplaza.
pub fn detect_metrics(content: &str) -> HashMap<String, MetricInfo> {
    let mut metrics = HashMap::new();
    for ((name, _), (re, value_res)) in METRIC_PATTERNS.iter().zip(METRIC_RES.iter()) {
        let count = re.find_iter(content).count();
        if count == 0 {
            continue;
        }
        let mut value: Option<f64> = None;
        for value_re in value_res {
            if let Some(caps) = value_re.captures(content) {
                if let Some(g) = caps.get(1) {
                    if let Ok(v) = g.as_str().parse::<f64>() {
                        value = Some(v);
                        break;
                    }
                }
            }
        }
        metrics.insert((*name).to_string(), MetricInfo { found: true, count, value });
    }
    metrics
}

// ─────────────────────────────────────────────────────────────────────────
// Criptografía post-cuántica y algoritmos cuánticos (Fase 32, 2026-09-24)
// ─────────────────────────────────────────────────────────────────────────

/// `true` si un esquema se menciona en una línea de **import**.
///
/// **Las dos reglas salieron de tests que fallaron, y cada una evita un
/// error distinto.**
///
/// 1. *Solo líneas de import.* Buscar el nombre en todo el archivo hacía que
///    un `rainbow` de un colormap de matplotlib disparara un hallazgo de
///    severidad alta sobre un esquema criptográfico roto. Un esquema se trae
///    con un import; un color se usa en cualquier lado.
/// 2. *Coincidencia por prefijo, no por palabra exacta.* Un límite de
///    palabra estricto no reconocía `kyber768` ni `dilithium3`, que son
///    justamente cómo se escriben los nombres reales — el sufijo es el
///    conjunto de parámetros, no otra palabra. Se exige que lo que sigue al
///    prefijo sea un dígito o un separador, nunca otra letra, para que
///    `sikeria` no pase por `sike`.
fn menciona_esquema(content: &str, esquema: &str) -> bool {
    for linea in content.lines() {
        let l = linea.trim_start();
        if !(l.starts_with("import ") || l.starts_with("from ")) {
            continue;
        }
        for token in l.to_lowercase().split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            let Some(resto) = token.strip_prefix(esquema) else { continue };
            let sigue_siendo_el_mismo_nombre = resto.is_empty() || resto.starts_with(|c: char| c.is_ascii_digit() || c == '_');
            if sigue_siendo_el_mismo_nombre {
                return true;
            }
        }
    }
    false
}

/// **Los renombres de NIST de agosto de 2024.** Los tres esquemas
/// seleccionados dejaron de llamarse por su nombre de competencia y pasaron
/// a tener nombre de estándar. El nombre viejo sigue siendo el que aparece
/// en el código y en casi toda la documentación, así que el dato útil no es
/// "esto está mal" sino "esto ahora se llama así, y este es el FIPS".
const RENOMBRES_NIST: &[(&str, &str, &str)] = &[
    ("kyber", "ML-KEM", "FIPS 203"),
    ("crystals-kyber", "ML-KEM", "FIPS 203"),
    ("dilithium", "ML-DSA", "FIPS 204"),
    ("crystals-dilithium", "ML-DSA", "FIPS 204"),
    ("sphincs", "SLH-DSA", "FIPS 205"),
];

/// **Esquemas rotos, y las dos roturas son CLÁSICAS.** Ni SIKE ni Rainbow
/// cayeron ante una computadora cuántica: los rompieron ataques corriendo en
/// hardware común, en 2022, siendo los dos candidatos vivos de NIST. Es el
/// recordatorio más concreto de que "resistente a cuántica" no significa
/// "seguro" — solo significa que Shor no aplica.
const ESQUEMAS_ROTOS: &[(&str, &str)] = &[
    (
        "sike",
        "SIKE/SIDH está ROTO desde 2022, y no por una computadora cuántica: el ataque de Castryck y Decru es CLÁSICO y recupera la clave en aproximadamente una hora en un solo núcleo. Era candidato de la ronda 4 de NIST cuando cayó. No se debe usar para nada.",
    ),
    (
        "sidh",
        "SIKE/SIDH está ROTO desde 2022, y no por una computadora cuántica: el ataque de Castryck y Decru es CLÁSICO y recupera la clave en aproximadamente una hora en un solo núcleo. No se debe usar para nada.",
    ),
    (
        "rainbow",
        "Rainbow está ROTO desde 2022 por el ataque de Beullens, también CLÁSICO: un fin de semana en una laptop para el nivel de seguridad más alto. Era finalista de NIST cuando cayó.",
    ),
];

/// Hallazgos sobre criptografía post-cuántica.
///
/// **Lo que NO se hace acá**: afirmar que el código es inseguro por usar RSA
/// o ECC. Migrar tiene un costo y un calendario, y el motor no sabe qué
/// amenaza enfrenta este proyecto. Lo que sí se dice es lo verificable: qué
/// esquema está roto hoy, y cómo se llama ahora cada uno.
fn issues_post_cuanticos(content: &str, libraries: &[MlLibrary]) -> Vec<MlIssue> {
    let mut out = Vec::new();

    for (nombre, motivo) in ESQUEMAS_ROTOS {
        if menciona_esquema(content, nombre) {
            out.push(MlIssue {
                // `error`, no un valor nuevo: el vocabulario del motor es
                // error/warning/info, y `_ml_score` resta puntos por los dos
                // primeros. Con una severidad inventada el hallazgo salía
                // junto a un score de 100/100 "Excelente" — lo vi en el
                // navegador, no lo dijo ningún test.
                severity: "error",
                category: "pqc",
                message: (*motivo).to_string(),
                suggestion: "Reemplazar por ML-KEM (FIPS 203) para intercambio de claves o ML-DSA (FIPS 204) para firmas — los esquemas que NIST sí estandarizó.",
            });
        }
    }

    for (viejo, nuevo, fips) in RENOMBRES_NIST {
        if menciona_esquema(content, viejo) {
            out.push(MlIssue {
                severity: "info",
                category: "pqc",
                message: format!(
                    "`{viejo}` es el nombre de competencia; desde agosto de 2024 el esquema estandarizado se llama {nuevo} ({fips}). No es lo mismo que un renombre cosmético: la versión estandarizada difiere en detalles de la propuesta original, así que una implementación vieja puede NO ser interoperable con una que diga {nuevo}."
                ),
                suggestion: "Verificar contra qué versión implementa la librería (la de la ronda de NIST o la del FIPS final) antes de asumir interoperabilidad.",
            });
        }
    }

    // La aclaración que más falta hace, y solo cuando el archivo ya habla de
    // esto: "cuántico rompe toda la criptografía" es falso.
    let hay_cuantico = libraries.iter().any(|l| l.category == "cuantico");
    let hay_pqc = libraries.iter().any(|l| l.category == "pqc");
    if hay_cuantico || hay_pqc {
        out.push(MlIssue {
            severity: "info",
            category: "pqc",
            message:
                "Qué rompe y qué no: Shor rompe lo que se apoya en factorizar o en logaritmo discreto — RSA, Diffie-Hellman y curvas elípticas. Grover NO rompe la criptografía simétrica: da una aceleración solo CUADRÁTICA sobre búsqueda no estructurada, así que AES-256 baja a ~128 bits de seguridad efectiva, que sigue siendo holgado. Por eso la migración urgente es la asimétrica, no la simétrica, y por eso \"lo cuántico rompe toda la criptografía\" es falso."
                    .to_string(),
            suggestion: "El motivo para migrar ya, sin que exista todavía una máquina capaz: el tráfico cifrado que se captura hoy se puede descifrar después (harvest now, decrypt later).",
        });
    }

    out
}


/// La aclaración sobre renderizado neuronal.
///
/// **"Renderizado neuronal" es un paraguas flojo**, y la diferencia entre
/// las dos familias que junta no es de grado: es si hay o no una red en el
/// camino de render. Decirlo importa porque explica la brecha de velocidad
/// que todo el mundo nota y casi nadie atribuye bien.
fn issues_de_renderizado(libraries: &[MlLibrary]) -> Vec<MlIssue> {
    if !libraries.iter().any(|l| l.category == "render") {
        return Vec::new();
    }
    vec![MlIssue {
        severity: "info",
        category: "render",
        message:
            "NeRF (2020) representa la escena como una FUNCIÓN CONTINUA: un MLP que se consulta una vez por muestra a lo largo de cada rayo, así que pintar un píxel cuesta muchas evaluaciones de red — de ahí la lentitud. 3D Gaussian Splatting (2023) reemplaza el MLP por un conjunto explícito de gaussianas que se rasterizan directo, y en su forma original NO tiene ninguna red en el camino de render (usa coeficientes de armónicos esféricos, no una red): por eso llega a tiempo real. Los dos se agrupan como \"renderizado neuronal\", pero solo uno tiene red donde se renderiza."
                .to_string(),
        suggestion: "El atajo intermedio es instant-ngp/tiny-cuda-nn: la red sigue estando, pero una codificación hash multirresolución la deja lo bastante chica como para entrenar en segundos.",
    }]
}

/// Puerto 1:1 de `_detect_ml_issues` (`ml.py`) — 21 chequeos heurísticos
/// puro texto/regex (nunca necesita AST), mismo orden y mismos umbrales que
/// el Python que reemplaza; no es una reescritura "más limpia" ni una
/// reordenación por categoría. `libraries` es el mismo `Vec<MlLibrary>` que
/// `detect_ml_libraries` ya calculó en esta llamada — nunca lee `version`
/// (ese campo ni siquiera existe acá, lo agrega Python después), así que
/// corre seguro sobre el resultado crudo del sidecar.
pub fn detect_ml_issues(content: &str, libraries: &[MlLibrary]) -> Vec<MlIssue> {
    let mut issues = Vec::new();
    let lib_names: HashSet<String> = libraries.iter().map(|l| l.name.to_lowercase()).collect();
    let lib_imports: String = libraries.iter().map(|l| l.import.to_lowercase()).collect::<Vec<_>>().join(" ");

    let has_torch = lib_names.contains("pytorch") || lib_imports.contains("torch");
    let has_tf = lib_names.contains("tensorflow") || lib_names.contains("keras") || lib_imports.contains("tensorflow");
    let has_np = lib_names.contains("numpy") || lib_imports.contains("np");

    // Data leakage — fit antes de split. Filtra líneas de import para no
    // confundir `from sklearn import train_test_split`.
    let code_only: String = content
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("import ") && !t.starts_with("from ")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let scaler_pos = re!(r"\bfit_transform\b")
        .find_iter(&code_only)
        .map(|m| m.start())
        .min();
    let split_pos = re!(r"\btrain_test_split\b")
        .find_iter(&code_only)
        .map(|m| m.start())
        .min();
    if let (Some(sp), Some(tp)) = (scaler_pos, split_pos) {
        if sp < tp {
            issues.push(MlIssue {
                severity: "error",
                category: "data_leakage",
                message: "fit_transform() aparece ANTES de train_test_split()".to_string(),
                suggestion: "Escala los datos DESPUÉS de dividirlos para evitar data leakage",
            });
        }
    }

    let has_random_state = re!(r"random_state\s*=").is_match(content);

    // Random state
    if re!(r"train_test_split\s*\(").is_match(content) && !has_random_state {
        issues.push(MlIssue {
            severity: "warning",
            category: "reproducibilidad",
            message: "train_test_split() sin random_state".to_string(),
            suggestion: "Agrega random_state=42 para resultados reproducibles",
        });
    }

    if re!(r"KFold\s*\(|StratifiedKFold\s*\(").is_match(content) && !has_random_state {
        issues.push(MlIssue {
            severity: "warning",
            category: "reproducibilidad",
            message: "KFold sin random_state".to_string(),
            suggestion: "Fija random_state en KFold",
        });
    }

    // Semillas
    if has_torch && !re!(r"torch\.manual_seed\b").is_match(content) {
        issues.push(MlIssue {
            severity: "warning",
            category: "reproducibilidad",
            message: "PyTorch sin torch.manual_seed()".to_string(),
            suggestion: "Agrega torch.manual_seed(42)",
        });
    }

    if has_tf
        && !re!(r"tf\.random\.set_seed\b|keras\.utils\.set_random_seed\b")
            .is_match(content)
    {
        issues.push(MlIssue {
            severity: "warning",
            category: "reproducibilidad",
            message: "TensorFlow sin tf.random.set_seed()".to_string(),
            suggestion: "Agrega tf.random.set_seed(42)",
        });
    }

    if has_np && !re!(r"np\.random\.seed\b").is_match(content) {
        issues.push(MlIssue {
            severity: "info",
            category: "reproducibilidad",
            message: "NumPy sin np.random.seed()".to_string(),
            suggestion: "Agrega np.random.seed(42)",
        });
    }

    // Validación
    if re!(r"\bmodel\.fit\b|\b\.fit\s*\(").is_match(content)
        && !re!(r"validation_split|validation_data|cross_val_score|KFold").is_match(content)
    {
        issues.push(MlIssue {
            severity: "warning",
            category: "evaluacion",
            message: "Entrenamiento sin validación cruzada ni validation_split".to_string(),
            suggestion: "Usa validation_split=0.2 o KFold para detectar overfitting",
        });
    }

    // Normalización para redes
    if (has_torch || has_tf)
        && !re!(r"Normalize|normalize|StandardScaler|MinMaxScaler").is_match(content)
    {
        issues.push(MlIssue {
            severity: "warning",
            category: "preprocesamiento",
            message: "Red neuronal sin normalización detectada".to_string(),
            suggestion: "Normaliza los datos de entrada para acelerar el entrenamiento",
        });
    }

    // PyTorch zero_grad
    if has_torch
        && re!(r"loss\.backward\(\)").is_match(content)
        && !re!(r"optimizer\.zero_grad\(\)").is_match(content)
    {
        issues.push(MlIssue {
            severity: "error",
            category: "pytorch",
            message: "loss.backward() sin optimizer.zero_grad()".to_string(),
            suggestion: "Llama optimizer.zero_grad() antes de cada backward()",
        });
    }

    // .to(device)
    if has_torch
        && re!(r"nn\.Module|nn\.Sequential").is_match(content)
        && !re!(r"\.to\s*\(\s*device\s*\)|\.cuda\(\)|\.cpu\(\)").is_match(content)
    {
        issues.push(MlIssue {
            severity: "info",
            category: "pytorch",
            message: "Modelo PyTorch sin .to(device)".to_string(),
            suggestion: "Agrega model.to(device) para compatibilidad GPU/CPU",
        });
    }

    // EarlyStopping Keras
    if has_tf
        && re!(r"model\.fit\b").is_match(content)
        && !re!(r"EarlyStopping\b").is_match(content)
    {
        issues.push(MlIssue {
            severity: "info",
            category: "keras",
            message: "Entrenamiento Keras sin EarlyStopping".to_string(),
            suggestion: "Usa EarlyStopping(patience=5) para evitar overfitting",
        });
    }

    // batch_size
    if (has_torch || has_tf)
        && re!(r"DataLoader|model\.fit").is_match(content)
        && !re!(r"batch_size\s*=").is_match(content)
    {
        issues.push(MlIssue {
            severity: "info",
            category: "hiperparametros",
            message: "batch_size no definido explícitamente".to_string(),
            suggestion: "Define batch_size=32 según tu memoria disponible",
        });
    }

    // Polars mezclado con pandas
    if re!(r"polars|\bpl\.").is_match(content)
        && re!(r"\bpd\.DataFrame|\bpd\.read_csv").is_match(content)
    {
        issues.push(MlIssue {
            severity: "info",
            category: "polars",
            message: "Mezcla de Polars y Pandas detectada".to_string(),
            suggestion: "Usa solo Polars o convierte con pl.from_pandas() puntualmente",
        });
    }

    let to_pandas_n = re!(r"\.to_pandas\(\)").find_iter(content).count();
    if to_pandas_n > 3 {
        issues.push(MlIssue {
            severity: "warning",
            category: "polars",
            message: format!("to_pandas() llamado {to_pandas_n} veces"),
            suggestion: "Minimiza conversiones Polars<->Pandas, son costosas",
        });
    }

    // LightGBM
    if re!(r"lgb\.train|LGBMClassifier|LGBMRegressor").is_match(content) {
        if !content.contains("early_stopping") {
            issues.push(MlIssue {
                severity: "warning",
                category: "lightgbm",
                message: "LightGBM sin early_stopping".to_string(),
                suggestion: "Agrega callbacks=[lgb.early_stopping(50)] para evitar overfitting",
            });
        }
        if !re!(r"num_leaves|max_depth").is_match(content) {
            issues.push(MlIssue {
                severity: "info",
                category: "lightgbm",
                message: "Hiperparámetros clave no definidos en LightGBM".to_string(),
                suggestion: "Define num_leaves y max_depth para controlar el modelo",
            });
        }
    }

    // spaCy
    if re!(r"spacy\.load|nlp\s*=\s*spacy").is_match(content) {
        if !re!(r"try.*spacy\.load|except.*OSError").is_match(content) {
            issues.push(MlIssue {
                severity: "warning",
                category: "spacy",
                message: "spacy.load() sin manejo de error".to_string(),
                suggestion: "Envuelve en try/except: el modelo puede no estar descargado",
            });
        }
        if re!(r"for\s+\w+.*:\s*\n\s+.*nlp\s*\(").is_match(content) {
            issues.push(MlIssue {
                severity: "warning",
                category: "spacy",
                message: "nlp() llamado dentro de un bucle".to_string(),
                suggestion: "Usa nlp.pipe(textos, batch_size=256) para procesar en lote",
            });
        }
    }

    // IceCream en producción
    let ic_n = re!(r"\bic\s*\(").find_iter(content).count();
    if ic_n > 5 {
        issues.push(MlIssue {
            severity: "warning",
            category: "icecream",
            message: format!("Se encontraron {ic_n} llamadas ic() — eliminar en producción"),
            suggestion: "Usa ic.disable() o remueve los ic() antes de producción",
        });
    } else if ic_n > 0 {
        issues.push(MlIssue {
            severity: "info",
            category: "icecream",
            message: format!("IceCream activo ({ic_n} llamadas ic())"),
            suggestion: "Recuerda desactivarlo en producción con ic.disable()",
        });
    }

    // OpenCV
    if re!(r"cv2\.imread\b").is_match(content)
        && !re!(r"is None|== None|if.*img|if.*image").is_match(content)
    {
        issues.push(MlIssue {
            severity: "warning",
            category: "opencv",
            message: "cv2.imread() sin verificación de None".to_string(),
            suggestion: "Agrega: if img is None: raise FileNotFoundError(...)",
        });
    }

    if re!(r"cv2\.VideoCapture\b").is_match(content) && !content.contains(".release()") {
        issues.push(MlIssue {
            severity: "warning",
            category: "opencv",
            message: "VideoCapture sin .release()".to_string(),
            suggestion: "Llama cap.release() al terminar para liberar recursos",
        });
    }

    // Plotly
    if re!(r"px\.|go\.Figure|plotly\.express").is_match(content)
        && !re!(r"\.show\(\)|\.write_html\(|\.write_image\(").is_match(content)
    {
        issues.push(MlIssue {
            severity: "info",
            category: "plotly",
            message: "Figura Plotly sin .show() ni .write_html()".to_string(),
            suggestion: "Usa fig.show() o fig.write_html('salida.html')",
        });
    }

    // SciPy
    if re!(r"ttest_|chi2_contingency|mannwhitneyu|anova").is_match(content)
        && !re!(r"p_value|pvalue|\.pvalue").is_match(content)
    {
        issues.push(MlIssue {
            severity: "info",
            category: "scipy",
            message: "Test estadístico sin verificar p-value".to_string(),
            suggestion: "Verifica: if result.pvalue < 0.05: ...",
        });
    }

    // Fase 32 — criptografía post-cuántica. Va al final para que los
    // hallazgos de ML sigan apareciendo primero: son los que el panel
    // existía para mostrar.
    issues.extend(issues_post_cuanticos(content, libraries));
    issues.extend(issues_de_renderizado(libraries));

    issues
}

/// Punto de entrada único — `None` si el contenido no parsea como Python
/// válido (no debería pasar en la práctica: `apps/api/routers/ml.py` ya hace
/// su propio `ast.parse()` local antes de llamar acá, así que esto es una
/// red de seguridad, no el camino esperado).
pub fn detect(content: &str) -> Option<MlDetectionResult> {
    let suite = parse_module(content).ok()?;
    let libraries = detect_ml_libraries(&suite);
    let issues = detect_ml_issues(content, &libraries);
    Some(MlDetectionResult {
        libraries,
        pipeline: detect_pipeline(content),
        models: detect_models(content),
        metrics: detect_metrics(content),
        issues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;



    fn parse(src: &str) -> Vec<Stmt> {
        parse_module(src).unwrap()
    }

    fn fake_lib(name: &str, import: &str) -> MlLibrary {
        MlLibrary {
            name: name.to_string(),
            category: "test",
            color: "#000000",
            import: import.to_string(),
            alias: None,
            module: String::new(),
        }
    }

    fn has_category(issues: &[MlIssue], category: &str) -> bool {
        issues.iter().any(|i| i.category == category)
    }

    #[test]
    fn detecta_numpy_simple() {
        let suite = parse("import numpy\n");
        let libs = detect_ml_libraries(&suite);
        assert_eq!(libs.len(), 1);
        assert_eq!(libs[0].name, "NumPy");
        assert_eq!(libs[0].module, "numpy");
    }

    #[test]
    fn alias_no_duplica_la_libreria() {
        // Bug real ya documentado en el Python que reemplaza: sin el dedup
        // por nombre canónico, esto contaba NumPy dos veces.
        let suite = parse("import numpy as np\n");
        let libs = detect_ml_libraries(&suite);
        assert_eq!(libs.len(), 1);
        assert_eq!(libs[0].name, "NumPy");
        assert_eq!(libs[0].alias.as_deref(), Some("np"));
    }

    #[test]
    fn import_from_detecta_sklearn() {
        let suite = parse("from sklearn.model_selection import train_test_split\n");
        let libs = detect_ml_libraries(&suite);
        assert!(libs.iter().any(|l| l.name == "Scikit-learn"));
    }

    #[test]
    fn libreria_no_reconocida_no_aparece() {
        let suite = parse("import os\nimport sys\n");
        assert!(detect_ml_libraries(&suite).is_empty());
    }

    #[test]
    fn detecta_etapa_de_carga_y_split() {
        let content = "import pandas as pd\ndf = pd.read_csv('x.csv')\nX_train, X_test = train_test_split(df)\n";
        let pipeline = detect_pipeline(content);
        let ids: Vec<&str> = pipeline.iter().map(|s| s.id).collect();
        assert!(ids.contains(&"carga_datos"));
        assert!(ids.contains(&"split"));
    }

    #[test]
    fn pipeline_ordenado_por_linea() {
        let content = "train_test_split(x)\npd.read_csv('a.csv')\n";
        let pipeline = detect_pipeline(content);
        assert!(pipeline[0].line <= pipeline[1].line);
    }

    #[test]
    fn pipeline_sin_matches_da_vacio() {
        assert!(detect_pipeline("x = 1\n").is_empty());
    }

    #[test]
    fn detecta_random_forest() {
        let content = "model = RandomForestClassifier(random_state=42)\n";
        let models = detect_models(content);
        assert!(models.iter().any(|m| m.name == "RandomForestClassifier" && m.framework == "sklearn"));
    }

    #[test]
    fn modelo_no_mencionado_no_aparece() {
        assert!(detect_models("x = 1\n").is_empty());
    }

    #[test]
    fn detecta_metrica_accuracy() {
        let content = "score = accuracy_score(y_test, preds)\n";
        let metrics = detect_metrics(content);
        assert!(metrics.contains_key("accuracy"));
        assert!(metrics["accuracy"].found);
    }

    #[test]
    fn detecta_valor_numerico_de_metrica() {
        let content = "epochs = 50\n";
        let metrics = detect_metrics(content);
        assert_eq!(metrics["epochs"].value, Some(50.0));
    }

    #[test]
    fn metrica_no_mencionada_no_aparece() {
        assert!(!detect_metrics("x = 1\n").contains_key("accuracy"));
    }

    #[test]
    fn detect_end_to_end_junta_todo() {
        let content = "import numpy as np\nimport pandas as pd\nfrom sklearn.model_selection import train_test_split\nfrom sklearn.ensemble import RandomForestClassifier\n\ndf = pd.read_csv('data.csv')\nX_train, X_test = train_test_split(df, random_state=42)\nmodel = RandomForestClassifier()\nmodel.fit(X_train)\n";
        let result = detect(content).unwrap();
        assert!(result.libraries.iter().any(|l| l.name == "NumPy"));
        assert!(result.pipeline.iter().any(|s| s.id == "carga_datos"));
        assert!(result.models.iter().any(|m| m.name == "RandomForestClassifier"));
        assert!(has_category(&result.issues, "reproducibilidad"));
    }

    // ── detect_ml_issues ─────────────────────────────────────────────────

    #[test]
    fn data_leakage_detecta_fit_antes_de_split() {
        let content = "scaler.fit_transform(X_raw)\ntrain_test_split(X)\n";
        let issues = detect_ml_issues(content, &[]);
        assert!(has_category(&issues, "data_leakage"));
    }

    #[test]
    fn data_leakage_no_dispara_si_split_va_primero() {
        let content = "train_test_split(X)\nscaler.fit_transform(X_train)\n";
        let issues = detect_ml_issues(content, &[]);
        assert!(!has_category(&issues, "data_leakage"));
    }

    #[test]
    fn data_leakage_ignora_apariciones_en_lineas_de_import() {
        // `_code_only` filtra líneas de import antes de este chequeo — un
        // import con ambos nombres en el texto no debería contar como orden.
        let content = "from sklearn.preprocessing import fit_transform\nfrom sklearn.model_selection import train_test_split\n";
        let issues = detect_ml_issues(content, &[]);
        assert!(!has_category(&issues, "data_leakage"));
    }

    #[test]
    fn train_test_split_sin_random_state_dispara_warning() {
        let issues = detect_ml_issues("train_test_split(X, y)\n", &[]);
        assert!(issues.iter().any(|i| i.message.contains("random_state")));
    }

    #[test]
    fn train_test_split_con_random_state_no_dispara() {
        let issues = detect_ml_issues("train_test_split(X, y, random_state=42)\n", &[]);
        assert!(!issues.iter().any(|i| i.message.contains("train_test_split() sin random_state")));
    }

    #[test]
    fn kfold_sin_random_state_dispara_warning() {
        let issues = detect_ml_issues("KFold(n_splits=5)\n", &[]);
        assert!(issues.iter().any(|i| i.message == "KFold sin random_state"));
    }

    #[test]
    fn torch_sin_manual_seed_dispara_warning() {
        let libs = [fake_lib("PyTorch", "torch")];
        let issues = detect_ml_issues("x = torch.randn(3)\n", &libs);
        assert!(issues.iter().any(|i| i.message.contains("manual_seed")));
    }

    #[test]
    fn torch_con_manual_seed_no_dispara() {
        let libs = [fake_lib("PyTorch", "torch")];
        let issues = detect_ml_issues("torch.manual_seed(42)\n", &libs);
        assert!(!issues.iter().any(|i| i.message.contains("manual_seed")));
    }

    #[test]
    fn tensorflow_sin_set_seed_dispara_warning() {
        let libs = [fake_lib("TensorFlow", "tensorflow")];
        let issues = detect_ml_issues("model = tf.keras.Sequential()\n", &libs);
        assert!(issues.iter().any(|i| i.message.contains("set_seed")));
    }

    #[test]
    fn numpy_sin_random_seed_dispara_info() {
        let libs = [fake_lib("NumPy", "numpy")];
        let issues = detect_ml_issues("x = np.array([1, 2, 3])\n", &libs);
        assert!(issues.iter().any(|i| i.severity == "info" && i.message.contains("np.random.seed")));
    }

    #[test]
    fn entrenamiento_sin_validacion_dispara_warning() {
        let issues = detect_ml_issues("model.fit(X_train, y_train)\n", &[]);
        assert!(has_category(&issues, "evaluacion"));
    }

    #[test]
    fn entrenamiento_con_validation_split_no_dispara() {
        let issues = detect_ml_issues("model.fit(X_train, y_train, validation_split=0.2)\n", &[]);
        assert!(!has_category(&issues, "evaluacion"));
    }

    #[test]
    fn red_sin_normalizacion_dispara_warning() {
        let libs = [fake_lib("PyTorch", "torch")];
        let issues = detect_ml_issues("class Net(nn.Module):\n    pass\n", &libs);
        assert!(has_category(&issues, "preprocesamiento"));
    }

    #[test]
    fn pytorch_backward_sin_zero_grad_dispara_error() {
        let libs = [fake_lib("PyTorch", "torch")];
        let issues = detect_ml_issues("loss.backward()\n", &libs);
        assert!(issues.iter().any(|i| i.severity == "error" && i.category == "pytorch"));
    }

    #[test]
    fn pytorch_backward_con_zero_grad_no_dispara() {
        let libs = [fake_lib("PyTorch", "torch")];
        let issues = detect_ml_issues("optimizer.zero_grad()\nloss.backward()\n", &libs);
        assert!(!issues.iter().any(|i| i.category == "pytorch" && i.severity == "error"));
    }

    #[test]
    fn pytorch_modulo_sin_to_device_dispara_info() {
        let libs = [fake_lib("PyTorch", "torch")];
        let issues = detect_ml_issues("class Net(nn.Module):\n    pass\n", &libs);
        assert!(issues.iter().any(|i| i.message.contains("to(device)")));
    }

    #[test]
    fn keras_fit_sin_early_stopping_dispara_info() {
        let libs = [fake_lib("Keras", "keras")];
        let issues = detect_ml_issues("model.fit(X, y)\n", &libs);
        assert!(has_category(&issues, "keras"));
    }

    #[test]
    fn sin_batch_size_explicito_dispara_info() {
        let libs = [fake_lib("PyTorch", "torch")];
        let issues = detect_ml_issues("loader = DataLoader(dataset)\n", &libs);
        assert!(has_category(&issues, "hiperparametros"));
    }

    #[test]
    fn polars_mezclado_con_pandas_dispara_info() {
        let content = "df1 = pl.DataFrame(x)\ndf2 = pd.read_csv('a.csv')\n";
        let issues = detect_ml_issues(content, &[]);
        assert!(has_category(&issues, "polars"));
    }

    #[test]
    fn to_pandas_mas_de_tres_veces_dispara_warning() {
        let content = "a.to_pandas()\nb.to_pandas()\nc.to_pandas()\nd.to_pandas()\n";
        let issues = detect_ml_issues(content, &[]);
        assert!(issues.iter().any(|i| i.severity == "warning" && i.message.contains("4 veces")));
    }

    #[test]
    fn to_pandas_tres_veces_no_dispara() {
        let content = "a.to_pandas()\nb.to_pandas()\nc.to_pandas()\n";
        let issues = detect_ml_issues(content, &[]);
        assert!(!issues.iter().any(|i| i.message.contains("to_pandas() llamado")));
    }

    #[test]
    fn lightgbm_sin_early_stopping_ni_hiperparametros_dispara_dos_issues() {
        let issues = detect_ml_issues("model = LGBMClassifier()\n", &[]);
        let lgbm: Vec<_> = issues.iter().filter(|i| i.category == "lightgbm").collect();
        assert_eq!(lgbm.len(), 2);
    }

    #[test]
    fn lightgbm_completo_no_dispara_nada() {
        let content = "model = LGBMClassifier(num_leaves=31, max_depth=5)\ncallbacks=[lgb.early_stopping(50)]\n";
        let issues = detect_ml_issues(content, &[]);
        assert!(!has_category(&issues, "lightgbm"));
    }

    #[test]
    fn spacy_load_sin_try_except_dispara_warning() {
        let issues = detect_ml_issues("nlp = spacy.load('en_core_web_sm')\n", &[]);
        assert!(issues.iter().any(|i| i.category == "spacy" && i.message.contains("manejo de error")));
    }

    #[test]
    fn spacy_nlp_en_loop_dispara_warning() {
        let content = "nlp = spacy.load('en')\nfor text in textos:\n    doc = nlp(text)\n";
        let issues = detect_ml_issues(content, &[]);
        assert!(issues.iter().any(|i| i.message.contains("dentro de un bucle")));
    }

    #[test]
    fn icecream_pocas_llamadas_dispara_info() {
        let issues = detect_ml_issues("ic(x)\nic(y)\n", &[]);
        assert!(issues.iter().any(|i| i.severity == "info" && i.category == "icecream"));
    }

    #[test]
    fn icecream_muchas_llamadas_dispara_warning() {
        let content = "ic(1)\nic(2)\nic(3)\nic(4)\nic(5)\nic(6)\n";
        let issues = detect_ml_issues(content, &[]);
        assert!(issues.iter().any(|i| i.severity == "warning" && i.category == "icecream"));
    }

    #[test]
    fn icecream_sin_llamadas_no_dispara_nada() {
        assert!(!has_category(&detect_ml_issues("x = 1\n", &[]), "icecream"));
    }

    #[test]
    fn opencv_imread_sin_none_check_dispara_warning() {
        let issues = detect_ml_issues("img = cv2.imread('a.png')\n", &[]);
        assert!(issues.iter().any(|i| i.message.contains("verificación de None")));
    }

    #[test]
    fn opencv_videocapture_sin_release_dispara_warning() {
        let issues = detect_ml_issues("cap = cv2.VideoCapture(0)\n", &[]);
        assert!(issues.iter().any(|i| i.message.contains(".release()")));
    }

    #[test]
    fn plotly_sin_show_dispara_info() {
        let issues = detect_ml_issues("fig = px.scatter(df, x='a', y='b')\n", &[]);
        assert!(has_category(&issues, "plotly"));
    }

    #[test]
    fn scipy_sin_pvalue_dispara_info() {
        let issues = detect_ml_issues("result = ttest_ind(a, b)\n", &[]);
        assert!(has_category(&issues, "scipy"));
    }

    #[test]
    fn scipy_con_pvalue_no_dispara() {
        let issues = detect_ml_issues("result = ttest_ind(a, b)\nprint(result.pvalue)\n", &[]);
        assert!(!has_category(&issues, "scipy"));
    }

    #[test]
    fn contenido_sin_ninguna_libreria_ni_patron_no_dispara_nada() {
        assert!(detect_ml_issues("x = 1 + 1\n", &[]).is_empty());
    }

    // ── Fase 32: cuántico, PQC y renderizado neuronal ───────────────────

    fn issues_de(src: &str) -> Vec<MlIssue> {
        let suite = parse_module(src).unwrap();
        let libs = detect_ml_libraries(&suite);
        detect_ml_issues(src, &libs)
    }

    fn libs_de(src: &str) -> Vec<MlLibrary> {
        detect_ml_libraries(&parse_module(src).unwrap())
    }

    #[test]
    fn los_sdks_cuanticos_se_detectan_con_categoria_propia() {
        let libs = libs_de("import qiskit\nimport cirq\nimport pennylane as qml\n");
        assert_eq!(libs.len(), 3, "{:?}", libs.iter().map(|l| &l.name).collect::<Vec<_>>());
        assert!(libs.iter().all(|l| l.category == "cuantico"), "no van bajo `dl`: un circuito no es una red");
    }

    #[test]
    fn las_librerias_de_renderizado_neuronal_se_detectan() {
        let libs = libs_de("import nerfstudio\nimport gsplat\nimport nvdiffrast\n");
        assert_eq!(libs.len(), 3);
        assert!(libs.iter().all(|l| l.category == "render"));
    }

    /// **La rotura de SIKE es CLÁSICA, no cuántica**, y eso es lo que la
    /// hace un hallazgo y no una curiosidad.
    #[test]
    fn sike_se_marca_como_roto_y_se_dice_que_el_ataque_es_clasico() {
        let iss = issues_de("import sike\nx = sike.keygen()\n");
        let roto = iss.iter().find(|i| i.message.contains("ROTO")).expect("sin hallazgo de esquema roto");
        assert_eq!(roto.severity, "error", "el vocabulario del motor es error/warning/info");
        assert!(roto.message.contains("CLÁSICO"), "{}", roto.message);
        assert!(roto.message.contains("Castryck"), "{}", roto.message);
        assert!(roto.suggestion.contains("ML-KEM"));
    }

    #[test]
    fn rainbow_tambien_esta_roto_por_un_ataque_clasico() {
        let iss = issues_de("import rainbow\n");
        assert!(iss.iter().any(|i| i.message.contains("Beullens")), "{iss:?}");
    }

    /// **El renombre de NIST de 2024**: el nombre viejo es el que aparece en
    /// el código, así que el dato útil es cómo se llama ahora y qué FIPS es.
    #[test]
    fn kyber_y_dilithium_traen_su_nombre_de_estandar() {
        let iss = issues_de("from pqcrypto.kem import kyber768\nfrom pqcrypto.sign import dilithium3\n");
        assert!(iss.iter().any(|i| i.message.contains("ML-KEM") && i.message.contains("FIPS 203")), "{iss:?}");
        assert!(iss.iter().any(|i| i.message.contains("ML-DSA") && i.message.contains("FIPS 204")), "{iss:?}");
    }

    /// Y el renombre NO se presenta como cosmético: la versión estandarizada
    /// difiere de la propuesta, así que la interoperabilidad no se asume.
    #[test]
    fn el_renombre_advierte_sobre_interoperabilidad() {
        let iss = issues_de("import kyber\n");
        let r = iss.iter().find(|i| i.message.contains("ML-KEM")).unwrap();
        assert!(r.message.contains("interoperable"), "{}", r.message);
        assert_eq!(r.severity, "info", "un renombre no es un defecto");
    }

    /// **La aclaración que más falta hace**: "lo cuántico rompe toda la
    /// criptografía" es falso, y Grover es la razón por la que es falso.
    #[test]
    fn se_aclara_que_grover_no_rompe_la_simetrica() {
        let iss = issues_de("import qiskit\n");
        let n = iss.iter().find(|i| i.message.contains("Grover")).expect("{iss:?}");
        assert!(n.message.contains("CUADRÁTICA"), "{}", n.message);
        assert!(n.message.contains("AES-256"), "{}", n.message);
        assert!(n.suggestion.contains("harvest now"), "{}", n.suggestion);
    }

    /// La nota de criptografía solo aparece si el archivo ya habla del tema:
    /// no se le cuelga a cualquier archivo de ML.
    #[test]
    fn un_archivo_de_ml_comun_no_recibe_la_nota_de_criptografia() {
        let iss = issues_de("import torch\nimport numpy as np\n");
        assert!(!iss.iter().any(|i| i.category == "pqc"), "{iss:?}");
        assert!(!iss.iter().any(|i| i.category == "render"), "{iss:?}");
    }

    /// **La diferencia real entre NeRF y Gaussian Splatting**: si hay o no
    /// una red en el camino de render.
    #[test]
    fn la_nota_de_render_separa_nerf_de_gaussian_splatting() {
        let iss = issues_de("import gsplat\n");
        let n = iss.iter().find(|i| i.category == "render").unwrap();
        assert!(n.message.contains("NO tiene ninguna red en el camino de render"), "{}", n.message);
        assert!(n.message.contains("armónicos esféricos"), "{}", n.message);
    }

    /// **Las dos reglas de `menciona_esquema`, cada una contra un error real
    /// que un test encontró.**
    #[test]
    fn menciona_esquema_solo_mira_imports_y_acepta_el_sufijo_de_parametros() {
        // Acepta el sufijo de conjunto de parámetros, que es cómo se
        // escriben los nombres de verdad.
        assert!(menciona_esquema("from pqcrypto.kem import kyber768", "kyber"));
        assert!(menciona_esquema("from pqcrypto.sign import dilithium3", "dilithium"));
        assert!(menciona_esquema("import sike", "sike"));
        // Pero no deja que otra palabra pase por el esquema.
        assert!(!menciona_esquema("import sikeria", "sike"));
        // Y fuera de un import no mira: un `rainbow` de colormap no es un
        // hallazgo de criptografía rota.
        assert!(!menciona_esquema("cmap = 'rainbow'", "rainbow"));
        assert!(!menciona_esquema("sike = 1", "sike"));
    }

    /// **`falcon` NO está en la tabla de librerías**, y es deliberado: desde
    /// un `import falcon` no se distingue el esquema post-cuántico del
    /// framework web homónimo, y etiquetar una API REST como criptografía
    /// sería peor que no detectar el esquema.
    #[test]
    fn falcon_no_se_detecta_porque_colisiona_con_el_framework_web() {
        let libs = libs_de("import falcon\n");
        assert!(libs.is_empty(), "{:?}", libs.iter().map(|l| &l.name).collect::<Vec<_>>());
    }

    /// Y el caso que motiva la regla de los imports, de punta a punta.
    #[test]
    fn un_colormap_llamado_rainbow_no_dispara_un_hallazgo_de_cripto_rota() {
        let iss = issues_de("import matplotlib.pyplot as plt\nplt.imshow(x, cmap='rainbow')\n");
        assert!(!iss.iter().any(|i| i.category == "pqc"), "{iss:?}");
    }
}
