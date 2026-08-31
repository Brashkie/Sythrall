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
use rustpython_parser::ast::{Expr, Stmt};
use serde::Serialize;

use crate::parser::parse_module;
use crate::walk::walk_stmts;

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

#[derive(Serialize, Clone)]
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
    for p in PIPELINE_PATTERNS {
        let re = Regex::new(p.pattern).expect("patrón de pipeline inválido");
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
    for m in MODEL_PATTERNS {
        let pattern = format!(r"\b{}\b", regex::escape(m.name));
        let re = Regex::new(&pattern).expect("patrón de modelo inválido");
        if let Some(mat) = re.find(content) {
            let line = content[..mat.start()].matches('\n').count() + 1;
            found.push(DetectedModel { name: m.name, kind: m.kind, family: m.family, framework: m.framework, line });
        }
    }
    found
}

/// Puerto literal de `METRIC_PATTERNS` (`ml.py`) — mismas 14 entradas.
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
    for (name, pattern) in METRIC_PATTERNS {
        let re = Regex::new(&format!("(?i){pattern}")).expect("patrón de métrica inválido");
        let count = re.find_iter(content).count();
        if count == 0 {
            continue;
        }
        let mut value: Option<f64> = None;
        for alt in pattern.split('|') {
            let alt = alt.trim();
            let value_re = match Regex::new(&format!(r"(?i){alt}\s*[=:]\s*([\d]+\.?[\d]*)")) {
                Ok(r) => r,
                Err(_) => continue,
            };
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
    let scaler_pos = Regex::new(r"\bfit_transform\b")
        .expect("patrón inválido")
        .find_iter(&code_only)
        .map(|m| m.start())
        .min();
    let split_pos = Regex::new(r"\btrain_test_split\b")
        .expect("patrón inválido")
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

    let has_random_state = Regex::new(r"random_state\s*=").expect("patrón inválido").is_match(content);

    // Random state
    if Regex::new(r"train_test_split\s*\(").expect("patrón inválido").is_match(content) && !has_random_state {
        issues.push(MlIssue {
            severity: "warning",
            category: "reproducibilidad",
            message: "train_test_split() sin random_state".to_string(),
            suggestion: "Agrega random_state=42 para resultados reproducibles",
        });
    }

    if Regex::new(r"KFold\s*\(|StratifiedKFold\s*\(").expect("patrón inválido").is_match(content) && !has_random_state {
        issues.push(MlIssue {
            severity: "warning",
            category: "reproducibilidad",
            message: "KFold sin random_state".to_string(),
            suggestion: "Fija random_state en KFold",
        });
    }

    // Semillas
    if has_torch && !Regex::new(r"torch\.manual_seed\b").expect("patrón inválido").is_match(content) {
        issues.push(MlIssue {
            severity: "warning",
            category: "reproducibilidad",
            message: "PyTorch sin torch.manual_seed()".to_string(),
            suggestion: "Agrega torch.manual_seed(42)",
        });
    }

    if has_tf
        && !Regex::new(r"tf\.random\.set_seed\b|keras\.utils\.set_random_seed\b")
            .expect("patrón inválido")
            .is_match(content)
    {
        issues.push(MlIssue {
            severity: "warning",
            category: "reproducibilidad",
            message: "TensorFlow sin tf.random.set_seed()".to_string(),
            suggestion: "Agrega tf.random.set_seed(42)",
        });
    }

    if has_np && !Regex::new(r"np\.random\.seed\b").expect("patrón inválido").is_match(content) {
        issues.push(MlIssue {
            severity: "info",
            category: "reproducibilidad",
            message: "NumPy sin np.random.seed()".to_string(),
            suggestion: "Agrega np.random.seed(42)",
        });
    }

    // Validación
    if Regex::new(r"\bmodel\.fit\b|\b\.fit\s*\(").expect("patrón inválido").is_match(content)
        && !Regex::new(r"validation_split|validation_data|cross_val_score|KFold").expect("patrón inválido").is_match(content)
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
        && !Regex::new(r"Normalize|normalize|StandardScaler|MinMaxScaler").expect("patrón inválido").is_match(content)
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
        && Regex::new(r"loss\.backward\(\)").expect("patrón inválido").is_match(content)
        && !Regex::new(r"optimizer\.zero_grad\(\)").expect("patrón inválido").is_match(content)
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
        && Regex::new(r"nn\.Module|nn\.Sequential").expect("patrón inválido").is_match(content)
        && !Regex::new(r"\.to\s*\(\s*device\s*\)|\.cuda\(\)|\.cpu\(\)").expect("patrón inválido").is_match(content)
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
        && Regex::new(r"model\.fit\b").expect("patrón inválido").is_match(content)
        && !Regex::new(r"EarlyStopping\b").expect("patrón inválido").is_match(content)
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
        && Regex::new(r"DataLoader|model\.fit").expect("patrón inválido").is_match(content)
        && !Regex::new(r"batch_size\s*=").expect("patrón inválido").is_match(content)
    {
        issues.push(MlIssue {
            severity: "info",
            category: "hiperparametros",
            message: "batch_size no definido explícitamente".to_string(),
            suggestion: "Define batch_size=32 según tu memoria disponible",
        });
    }

    // Polars mezclado con pandas
    if Regex::new(r"polars|\bpl\.").expect("patrón inválido").is_match(content)
        && Regex::new(r"\bpd\.DataFrame|\bpd\.read_csv").expect("patrón inválido").is_match(content)
    {
        issues.push(MlIssue {
            severity: "info",
            category: "polars",
            message: "Mezcla de Polars y Pandas detectada".to_string(),
            suggestion: "Usa solo Polars o convierte con pl.from_pandas() puntualmente",
        });
    }

    let to_pandas_n = Regex::new(r"\.to_pandas\(\)").expect("patrón inválido").find_iter(content).count();
    if to_pandas_n > 3 {
        issues.push(MlIssue {
            severity: "warning",
            category: "polars",
            message: format!("to_pandas() llamado {to_pandas_n} veces"),
            suggestion: "Minimiza conversiones Polars<->Pandas, son costosas",
        });
    }

    // LightGBM
    if Regex::new(r"lgb\.train|LGBMClassifier|LGBMRegressor").expect("patrón inválido").is_match(content) {
        if !content.contains("early_stopping") {
            issues.push(MlIssue {
                severity: "warning",
                category: "lightgbm",
                message: "LightGBM sin early_stopping".to_string(),
                suggestion: "Agrega callbacks=[lgb.early_stopping(50)] para evitar overfitting",
            });
        }
        if !Regex::new(r"num_leaves|max_depth").expect("patrón inválido").is_match(content) {
            issues.push(MlIssue {
                severity: "info",
                category: "lightgbm",
                message: "Hiperparámetros clave no definidos en LightGBM".to_string(),
                suggestion: "Define num_leaves y max_depth para controlar el modelo",
            });
        }
    }

    // spaCy
    if Regex::new(r"spacy\.load|nlp\s*=\s*spacy").expect("patrón inválido").is_match(content) {
        if !Regex::new(r"try.*spacy\.load|except.*OSError").expect("patrón inválido").is_match(content) {
            issues.push(MlIssue {
                severity: "warning",
                category: "spacy",
                message: "spacy.load() sin manejo de error".to_string(),
                suggestion: "Envuelve en try/except: el modelo puede no estar descargado",
            });
        }
        if Regex::new(r"for\s+\w+.*:\s*\n\s+.*nlp\s*\(").expect("patrón inválido").is_match(content) {
            issues.push(MlIssue {
                severity: "warning",
                category: "spacy",
                message: "nlp() llamado dentro de un bucle".to_string(),
                suggestion: "Usa nlp.pipe(textos, batch_size=256) para procesar en lote",
            });
        }
    }

    // IceCream en producción
    let ic_n = Regex::new(r"\bic\s*\(").expect("patrón inválido").find_iter(content).count();
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
    if Regex::new(r"cv2\.imread\b").expect("patrón inválido").is_match(content)
        && !Regex::new(r"is None|== None|if.*img|if.*image").expect("patrón inválido").is_match(content)
    {
        issues.push(MlIssue {
            severity: "warning",
            category: "opencv",
            message: "cv2.imread() sin verificación de None".to_string(),
            suggestion: "Agrega: if img is None: raise FileNotFoundError(...)",
        });
    }

    if Regex::new(r"cv2\.VideoCapture\b").expect("patrón inválido").is_match(content) && !content.contains(".release()") {
        issues.push(MlIssue {
            severity: "warning",
            category: "opencv",
            message: "VideoCapture sin .release()".to_string(),
            suggestion: "Llama cap.release() al terminar para liberar recursos",
        });
    }

    // Plotly
    if Regex::new(r"px\.|go\.Figure|plotly\.express").expect("patrón inválido").is_match(content)
        && !Regex::new(r"\.show\(\)|\.write_html\(|\.write_image\(").expect("patrón inválido").is_match(content)
    {
        issues.push(MlIssue {
            severity: "info",
            category: "plotly",
            message: "Figura Plotly sin .show() ni .write_html()".to_string(),
            suggestion: "Usa fig.show() o fig.write_html('salida.html')",
        });
    }

    // SciPy
    if Regex::new(r"ttest_|chi2_contingency|mannwhitneyu|anova").expect("patrón inválido").is_match(content)
        && !Regex::new(r"p_value|pvalue|\.pvalue").expect("patrón inválido").is_match(content)
    {
        issues.push(MlIssue {
            severity: "info",
            category: "scipy",
            message: "Test estadístico sin verificar p-value".to_string(),
            suggestion: "Verifica: if result.pvalue < 0.05: ...",
        });
    }

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
}
