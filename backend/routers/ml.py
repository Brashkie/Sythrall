"""
Router: ML
Migración exacta de /analyze/ml del app.py Flask v3.0.
Preserva: ML_LIB_MAP, PIPELINE_PATTERNS, MODEL_PATTERNS, METRIC_PATTERNS,
_detect_ml_libraries, _detect_pipeline, _detect_models, _detect_metrics,
_detect_ml_issues (con todas las 20+ reglas), _ml_diagram, _ml_score.
"""

import ast
import re
from typing import Any

from fastapi import APIRouter
from pydantic import BaseModel

from shared import add_log, now, _get_lib_version

router = APIRouter()


# ── Schemas ───────────────────────────────────────────────────────────────────

class AnalyzeMLRequest(BaseModel):
    filename: str = "script.py"
    content:  str = ""


# ── Mapas (idénticos al Flask original) ──────────────────────────────────────

ML_LIB_MAP: dict[str, dict] = {
    "numpy":           {"name":"NumPy",         "category":"datos",       "color":"#4d9fff"},
    "np":              {"name":"NumPy",          "category":"datos",       "color":"#4d9fff"},
    "pandas":          {"name":"Pandas",         "category":"datos",       "color":"#4d9fff"},
    "pd":              {"name":"Pandas",         "category":"datos",       "color":"#4d9fff"},
    "sklearn":         {"name":"Scikit-learn",   "category":"ml",          "color":"#f5a623"},
    "torch":           {"name":"PyTorch",        "category":"dl",          "color":"#ee4c2c"},
    "torchvision":     {"name":"TorchVision",    "category":"dl",          "color":"#ee4c2c"},
    "tensorflow":      {"name":"TensorFlow",     "category":"dl",          "color":"#ff6b35"},
    "tf":              {"name":"TensorFlow",     "category":"dl",          "color":"#ff6b35"},
    "keras":           {"name":"Keras",          "category":"dl",          "color":"#d00000"},
    "matplotlib":      {"name":"Matplotlib",     "category":"viz",         "color":"#00c07a"},
    "plt":             {"name":"Matplotlib",     "category":"viz",         "color":"#00c07a"},
    "seaborn":         {"name":"Seaborn",        "category":"viz",         "color":"#00c07a"},
    "sns":             {"name":"Seaborn",        "category":"viz",         "color":"#00c07a"},
    "xgboost":         {"name":"XGBoost",        "category":"ml",          "color":"#f5a623"},
    "lightgbm":        {"name":"LightGBM",       "category":"ml",          "color":"#f5a623"},
    "cv2":             {"name":"OpenCV",         "category":"vision",      "color":"#b87dff"},
    "PIL":             {"name":"Pillow",         "category":"vision",      "color":"#b87dff"},
    "transformers":    {"name":"HuggingFace",    "category":"nlp",         "color":"#ffdd57"},
    "nltk":            {"name":"NLTK",           "category":"nlp",         "color":"#ffdd57"},
    "spacy":           {"name":"spaCy",          "category":"nlp",         "color":"#ffdd57"},
    "scipy":           {"name":"SciPy",          "category":"ciencia",     "color":"#4d9fff"},
    "cython":          {"name":"Cython",         "category":"rendimiento", "color":"#ffd43b"},
    "pyximport":       {"name":"Cython",         "category":"rendimiento", "color":"#ffd43b"},
    "cython.parallel": {"name":"Cython Parallel","category":"rendimiento", "color":"#ffd43b"},
}

PIPELINE_PATTERNS: list[tuple] = [
    (r'\bpd\.read_csv\b|\bpd\.read_excel\b|\bpd\.read_json\b|\bpd\.read_parquet\b|\bpd\.read_sql\b',
     "carga_datos",     "Carga de datos",                  "📥"),
    (r'\bdropna\b|\bfillna\b|\bdrop_duplicates\b|SimpleImputer',
     "limpieza",        "Limpieza de datos",               "🧹"),
    (r'\bLabelEncoder\b|\bOneHotEncoder\b|\bget_dummies\b|\bOrdinalEncoder\b',
     "encoding",        "Encoding categórico",             "🔢"),
    (r'\bStandardScaler\b|\bMinMaxScaler\b|\bRobustScaler\b|\bnormalize\b',
     "escalado",        "Escalado/Normalización",          "⚖️"),
    (r'\btrain_test_split\b|\bKFold\b|\bStratifiedKFold\b|\bcross_val_score\b',
     "split",           "División train/test",             "✂️"),
    (r'\bPCA\b|\bTSNE\b|\bSelectKBest\b|feature_selection',
     "features",        "Selección de features",           "🔍"),
    (r'\bnn\.Module\b|\bnn\.Sequential\b|\bnn\.Linear\b|\bnn\.Conv2d\b',
     "arquitectura",    "Definición de arquitectura",      "🧠"),
    (r'\bmodel\.compile\b|\boptim\.\w+\(|\bAdam\b|\bSGD\b|\bAdamW\b',
     "optimizador",     "Configuración del optimizador",   "⚙️"),
    (r'\bDataLoader\b|\bDataset\b|\bImageDataGenerator\b|\btf\.data\.Dataset\b',
     "dataloader",      "Pipeline de datos",               "🔄"),
    (r'\bfit\s*\(|\bfit_transform\s*\(|\bmodel\.fit\b|\bloss\.backward\(\)',
     "entrenamiento",   "Entrenamiento del modelo",        "🏋️"),
    (r'\bEarlyStopping\b|\bModelCheckpoint\b|\bCallbacks\b|\bTensorBoard\b',
     "callbacks",       "Callbacks de entrenamiento",      "🔔"),
    (r'\bpredict\s*\(|\bpredict_proba\s*\(|\bmodel\.predict\b',
     "prediccion",      "Predicción/Inferencia",           "🎯"),
    (r'\baccuracy_score\b|\bconfusion_matrix\b|\bclassification_report\b'
     r'|\br2_score\b|\bmean_squared_error\b|\bf1_score\b|\broc_auc_score\b',
     "evaluacion",      "Evaluación del modelo",           "📊"),
    (r'\bjoblib\.dump\b|\bpickle\.dump\b|\bmodel\.save\b|\btorch\.save\b',
     "guardado",        "Guardado del modelo",             "💾"),
    (r'\bplt\.plot\b|\bplt\.show\b|\bsns\.heatmap\b|\bplt\.figure\b',
     "visualizacion",   "Visualización Matplotlib",        "📈"),
    (r'\bpx\.scatter\b|\bpx\.line\b|\bpx\.bar\b|\bgo\.Figure\b|\bplotly\.',
     "viz_interactiva", "Visualización interactiva Plotly","📊"),
    (r'\bcv2\.imread\b|\bcv2\.resize\b|\bcv2\.cvtColor\b|\bcv2\.VideoCapture\b',
     "vision",          "Procesamiento de imágenes OpenCV","📷"),
    (r'\bscipy\.stats\b|\bscipy\.optimize\b|\bscipy\.signal\b|\bscipy\.linalg\b',
     "scipy_calc",      "Cálculo científico SciPy",        "🔬"),
    (r'\bic\s*\(|from icecream import',
     "debugging",       "Debug con IceCream",              "🍦"),
    (r'\bpl\.read_csv\b|\bpl\.read_parquet\b|\bpl\.DataFrame\b',
     "carga_polars",    "Carga datos Polars",              "🐻"),
    (r'\blgb\.train\b|\bLGBMClassifier\b|\bLGBMRegressor\b',
     "lightgbm_train",  "Modelo LightGBM",                 "🌿"),
    (r'\bspacy\.load\b|nlp\s*=\s*spacy',
     "nlp_spacy",       "Procesamiento NLP spaCy",         "🔤"),
    (r'\bcdef\s|\bcpdef\s|\bctypedef\s|\bpyximport\b|cimport\b',
     "cython_compile",  "Compilación Cython",              "⚡"),
]

MODEL_PATTERNS: dict[str, dict] = {
    "RandomForestClassifier":   {"type":"clasificación", "family":"ensemble",    "framework":"sklearn"},
    "RandomForestRegressor":    {"type":"regresión",     "family":"ensemble",    "framework":"sklearn"},
    "GradientBoostingClassifier":{"type":"clasificación","family":"boosting",    "framework":"sklearn"},
    "LogisticRegression":       {"type":"clasificación", "family":"lineal",      "framework":"sklearn"},
    "LinearRegression":         {"type":"regresión",     "family":"lineal",      "framework":"sklearn"},
    "SVC":                      {"type":"clasificación", "family":"svm",         "framework":"sklearn"},
    "KMeans":                   {"type":"clustering",    "family":"clustering",  "framework":"sklearn"},
    "DecisionTreeClassifier":   {"type":"clasificación", "family":"árbol",       "framework":"sklearn"},
    "XGBClassifier":            {"type":"clasificación", "family":"boosting",    "framework":"xgboost"},
    "LGBMClassifier":           {"type":"clasificación", "family":"boosting",    "framework":"lightgbm"},
    "LGBMRegressor":            {"type":"regresión",     "family":"boosting",    "framework":"lightgbm"},
    "Linear":                   {"type":"capa densa",    "family":"linear",      "framework":"pytorch"},
    "Conv2d":                   {"type":"capa conv",     "family":"cnn",         "framework":"pytorch"},
    "LSTM":                     {"type":"recurrente",    "family":"rnn",         "framework":"pytorch/keras"},
    "GRU":                      {"type":"recurrente",    "family":"rnn",         "framework":"pytorch/keras"},
    "Transformer":              {"type":"transformer",   "family":"attention",   "framework":"pytorch"},
    "Dense":                    {"type":"capa densa",    "family":"linear",      "framework":"keras"},
    "Conv2D":                   {"type":"capa conv",     "family":"cnn",         "framework":"keras"},
    "Dropout":                  {"type":"regularización","family":"dropout",     "framework":"keras"},
    "BatchNormalization":       {"type":"normalización", "family":"batchnorm",   "framework":"keras"},
    "Embedding":                {"type":"embedding",     "family":"nlp",         "framework":"keras"},
    "MultiHeadAttention":       {"type":"atención",      "family":"transformer", "framework":"keras"},
    "BertModel":                {"type":"BERT",          "family":"transformer", "framework":"transformers"},
    "GPT2Model":                {"type":"GPT-2",         "family":"transformer", "framework":"transformers"},
    "AutoModel":                {"type":"transformer",   "family":"pretrained",  "framework":"transformers"},
}

METRIC_PATTERNS: dict[str, str] = {
    "accuracy":      r'\baccuracy_score\b|\baccuracy\b|\bacc\b',
    "loss":          r'\bloss\b',
    "val_loss":      r'\bval_loss\b',
    "val_accuracy":  r'\bval_accuracy\b|\bval_acc\b',
    "mae":           r'\bmae\b|\bmean_absolute_error\b',
    "mse":           r'\bmse\b|\bmean_squared_error\b',
    "r2":            r'\br2_score\b|\br2\b',
    "f1":            r'\bf1_score\b|\bf1\b',
    "auc":           r'\broc_auc_score\b|\bauc\b',
    "precision":     r'\bprecision_score\b|\bprecision\b',
    "recall":        r'\brecall_score\b|\brecall\b',
    "epochs":        r'\bepochs\b',
    "batch_size":    r'\bbatch_size\b',
    "learning_rate": r'\blearning_rate\b|\blr\b',
}


# ── Endpoint ──────────────────────────────────────────────────────────────────

@router.post("/ml")
async def analyze_ml(req: AnalyzeMLRequest):
    """Equivalente a POST /analyze/ml del Flask original."""
    filename = req.filename
    content  = req.content

    result: dict[str, Any] = {
        "filename":    filename, "ts": now(),
        "libraries":   [], "pipeline":    [],
        "issues":      [], "metrics":     {},
        "models":      [], "diagram":     "",
        "score":       0,  "suggestions": [],
    }
    try:
        tree = ast.parse(content)
        result["libraries"]   = _detect_ml_libraries(tree)
        result["pipeline"]    = _detect_pipeline(content)
        result["models"]      = _detect_models(content)
        result["metrics"]     = _detect_metrics(content)
        result["issues"]      = _detect_ml_issues(content, result["libraries"])
        result["diagram"]     = _ml_diagram(result["pipeline"], result["models"], filename)
        result["score"], result["suggestions"] = _ml_score(content, result)
        add_log("info",
            f"ML analizado: {filename} — "
            f"{len(result['libraries'])} libs, {len(result['pipeline'])} etapas")
    except SyntaxError as e:
        result["issues"].append(
            {"severity":"error","message":f"SyntaxError línea {e.lineno}: {e.msg}"}
        )
    except Exception as e:
        add_log("error", f"Error ML {filename}: {e}")
        result["error"] = str(e)
    return result


# ── Funciones internas (idénticas al Flask original) ─────────────────────────

def _detect_ml_libraries(tree: ast.AST) -> list[dict]:
    found: dict[str, dict] = {}
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                base = alias.name.split(".")[0]
                key  = alias.asname or base
                for k in [alias.name, base, key]:
                    if k in ML_LIB_MAP and k not in found:
                        info = ML_LIB_MAP[k].copy()
                        info["import"]  = alias.name
                        info["alias"]   = alias.asname
                        info["version"] = _get_lib_version(base)
                        found[k] = info
        if isinstance(node, ast.ImportFrom):
            mod  = node.module or ""
            base = mod.split(".")[0]
            for k in [mod, base]:
                if k in ML_LIB_MAP and k not in found:
                    info = ML_LIB_MAP[k].copy()
                    info["import"]  = f"from {mod} import ..."
                    info["alias"]   = None
                    info["version"] = _get_lib_version(base)
                    found[k] = info
    return list(found.values())


def _detect_pipeline(content: str) -> list[dict]:
    steps, seen = [], set()
    for pattern, stage_id, description, icon in PIPELINE_PATTERNS:
        matches = list(re.finditer(pattern, content))
        if matches and stage_id not in seen:
            seen.add(stage_id)
            line_no = content[:matches[0].start()].count("\n") + 1
            steps.append({
                "id":          stage_id,
                "description": description,
                "icon":        icon,
                "line":        line_no,
                "count":       len(matches),
            })
    return sorted(steps, key=lambda x: x["line"])


def _detect_models(content: str) -> list[dict]:
    found, seen = [], set()
    for name, info in MODEL_PATTERNS.items():
        if re.search(r'\b' + re.escape(name) + r'\b', content) and name not in seen:
            seen.add(name)
            m = re.search(r'\b' + re.escape(name) + r'\b', content)
            found.append({
                **info, "name": name,
                "line": content[:m.start()].count("\n") + 1 if m else 0,
            })
    return found


def _detect_metrics(content: str) -> dict:
    metrics: dict = {}
    for metric, pattern in METRIC_PATTERNS.items():
        matches = re.findall(pattern, content, re.IGNORECASE)
        if matches:
            val = None
            # Buscar asignaciones numéricas: metric_name = 0.95 o metric_name: 0.95
            # Usar grupo separado para no mezclar con las alternativas del pattern
            # Buscar cada alternativa del pattern por separado
            for alt in pattern.split('|'):
                alt = alt.strip()
                m = re.search(alt + r'\s*[=:]\s*([\d]+\.?[\d]*)', content, re.IGNORECASE)
                if m and m.group(1) is not None:
                    try:
                        val = float(m.group(1))
                        break
                    except (ValueError, TypeError):
                        pass
            metrics[metric] = {"found": True, "count": len(matches), "value": val}
    return metrics


def _detect_ml_issues(content: str, libraries: list[dict]) -> list[dict]:
    issues: list[dict] = []
    lib_names   = {lib.get("name","").lower() for lib in libraries}
    lib_imports = " ".join(lib.get("import","") for lib in libraries).lower()

    has_torch = "pytorch"     in lib_names or "torch"      in lib_imports
    has_tf    = "tensorflow"  in lib_names or "keras"      in lib_names or "tensorflow" in lib_imports
    has_np    = "numpy"       in lib_names or "np"         in lib_imports
    has_sk    = "scikit-learn" in lib_names or "sklearn"   in lib_imports

    # Data leakage — fit antes de split
    # Filtrar imports para no confundir 'from sklearn import train_test_split'
    _code_lines = [l for l in content.splitlines() if not l.strip().startswith(("import ", "from "))]
    _code_only  = "\n".join(_code_lines)
    scaler_pos = [m.start() for m in re.finditer(r'\bfit_transform\b', _code_only)]
    split_pos  = [m.start() for m in re.finditer(r'\btrain_test_split\b', _code_only)]
    if scaler_pos and split_pos and min(scaler_pos) < min(split_pos):
        issues.append({"severity":"error","category":"data_leakage",
            "message":"fit_transform() aparece ANTES de train_test_split()",
            "suggestion":"Escala los datos DESPUÉS de dividirlos para evitar data leakage"})

    # Random state
    if re.search(r'train_test_split\s*\(', content) and not re.search(r'random_state\s*=', content):
        issues.append({"severity":"warning","category":"reproducibilidad",
            "message":"train_test_split() sin random_state",
            "suggestion":"Agrega random_state=42 para resultados reproducibles"})

    if re.search(r'KFold\s*\(|StratifiedKFold\s*\(', content) and not re.search(r'random_state\s*=', content):
        issues.append({"severity":"warning","category":"reproducibilidad",
            "message":"KFold sin random_state","suggestion":"Fija random_state en KFold"})

    # Semillas
    if has_torch and not re.search(r'torch\.manual_seed\b', content):
        issues.append({"severity":"warning","category":"reproducibilidad",
            "message":"PyTorch sin torch.manual_seed()",
            "suggestion":"Agrega torch.manual_seed(42)"})

    if has_tf and not re.search(r'tf\.random\.set_seed\b|keras\.utils\.set_random_seed\b', content):
        issues.append({"severity":"warning","category":"reproducibilidad",
            "message":"TensorFlow sin tf.random.set_seed()",
            "suggestion":"Agrega tf.random.set_seed(42)"})

    if has_np and not re.search(r'np\.random\.seed\b', content):
        issues.append({"severity":"info","category":"reproducibilidad",
            "message":"NumPy sin np.random.seed()",
            "suggestion":"Agrega np.random.seed(42)"})

    # Validación
    if re.search(r'\bmodel\.fit\b|\b\.fit\s*\(', content):
        if not re.search(r'validation_split|validation_data|cross_val_score|KFold', content):
            issues.append({"severity":"warning","category":"evaluacion",
                "message":"Entrenamiento sin validación cruzada ni validation_split",
                "suggestion":"Usa validation_split=0.2 o KFold para detectar overfitting"})

    # Normalización para redes
    if (has_torch or has_tf) and not re.search(r'Normalize|normalize|StandardScaler|MinMaxScaler', content):
        issues.append({"severity":"warning","category":"preprocesamiento",
            "message":"Red neuronal sin normalización detectada",
            "suggestion":"Normaliza los datos de entrada para acelerar el entrenamiento"})

    # PyTorch zero_grad
    if has_torch and re.search(r'loss\.backward\(\)', content):
        if not re.search(r'optimizer\.zero_grad\(\)', content):
            issues.append({"severity":"error","category":"pytorch",
                "message":"loss.backward() sin optimizer.zero_grad()",
                "suggestion":"Llama optimizer.zero_grad() antes de cada backward()"})

    # .to(device)
    if has_torch and re.search(r'nn\.Module|nn\.Sequential', content):
        if not re.search(r'\.to\s*\(\s*device\s*\)|\.cuda\(\)|\.cpu\(\)', content):
            issues.append({"severity":"info","category":"pytorch",
                "message":"Modelo PyTorch sin .to(device)",
                "suggestion":"Agrega model.to(device) para compatibilidad GPU/CPU"})

    # EarlyStopping Keras
    if has_tf and re.search(r'model\.fit\b', content):
        if not re.search(r'EarlyStopping\b', content):
            issues.append({"severity":"info","category":"keras",
                "message":"Entrenamiento Keras sin EarlyStopping",
                "suggestion":"Usa EarlyStopping(patience=5) para evitar overfitting"})

    # batch_size
    if (has_torch or has_tf) and re.search(r'DataLoader|model\.fit', content):
        if not re.search(r'batch_size\s*=', content):
            issues.append({"severity":"info","category":"hiperparametros",
                "message":"batch_size no definido explícitamente",
                "suggestion":"Define batch_size=32 según tu memoria disponible"})

    # Polars mezclado con pandas
    if re.search(r"polars|\bpl\.", content) and re.search(r"\bpd\.DataFrame|\bpd\.read_csv", content):
        issues.append({"severity":"info","category":"polars",
            "message":"Mezcla de Polars y Pandas detectada",
            "suggestion":"Usa solo Polars o convierte con pl.from_pandas() puntualmente"})

    if re.search(r"\.to_pandas\(\)", content):
        n = len(re.findall(r"\.to_pandas\(\)", content))
        if n > 3:
            issues.append({"severity":"warning","category":"polars",
                "message":f"to_pandas() llamado {n} veces",
                "suggestion":"Minimiza conversiones Polars<->Pandas, son costosas"})

    # LightGBM
    if re.search(r"lgb\.train|LGBMClassifier|LGBMRegressor", content):
        if not re.search(r"early_stopping", content):
            issues.append({"severity":"warning","category":"lightgbm",
                "message":"LightGBM sin early_stopping",
                "suggestion":"Agrega callbacks=[lgb.early_stopping(50)] para evitar overfitting"})
        if not re.search(r"num_leaves|max_depth", content):
            issues.append({"severity":"info","category":"lightgbm",
                "message":"Hiperparámetros clave no definidos en LightGBM",
                "suggestion":"Define num_leaves y max_depth para controlar el modelo"})

    # spaCy
    if re.search(r"spacy\.load|nlp\s*=\s*spacy", content):
        if not re.search(r"try.*spacy\.load|except.*OSError", content):
            issues.append({"severity":"warning","category":"spacy",
                "message":"spacy.load() sin manejo de error",
                "suggestion":"Envuelve en try/except: el modelo puede no estar descargado"})
        if re.findall(r"for\s+\w+.*:\s*\n\s+.*nlp\s*\(", content):
            issues.append({"severity":"warning","category":"spacy",
                "message":"nlp() llamado dentro de un bucle",
                "suggestion":"Usa nlp.pipe(textos, batch_size=256) para procesar en lote"})

    # IceCream en producción
    ic_matches = re.findall(r'\bic\s*\(', content)
    if ic_matches:
        n = len(ic_matches)
        if n > 5:
            issues.append({"severity":"warning","category":"icecream",
                "message":f"Se encontraron {n} llamadas ic() — eliminar en producción",
                "suggestion":"Usa ic.disable() o remueve los ic() antes de producción"})
        else:
            issues.append({"severity":"info","category":"icecream",
                "message":f"IceCream activo ({n} llamadas ic())",
                "suggestion":"Recuerda desactivarlo en producción con ic.disable()"})

    # OpenCV
    if re.search(r'cv2\.imread\b', content):
        if not re.search(r'is None|== None|if.*img|if.*image', content):
            issues.append({"severity":"warning","category":"opencv",
                "message":"cv2.imread() sin verificación de None",
                "suggestion":"Agrega: if img is None: raise FileNotFoundError(...)"})

    if re.search(r'cv2\.VideoCapture\b', content):
        if not re.search(r'\.release\(\)', content):
            issues.append({"severity":"warning","category":"opencv",
                "message":"VideoCapture sin .release()",
                "suggestion":"Llama cap.release() al terminar para liberar recursos"})

    # Plotly
    if re.search(r'px\.|go\.Figure|plotly\.express', content):
        if not re.search(r'\.show\(\)|\.write_html\(|\.write_image\(', content):
            issues.append({"severity":"info","category":"plotly",
                "message":"Figura Plotly sin .show() ni .write_html()",
                "suggestion":"Usa fig.show() o fig.write_html('salida.html')"})

    # SciPy
    if re.search(r'ttest_|chi2_contingency|mannwhitneyu|anova', content):
        if not re.search(r'p_value|pvalue|\.pvalue', content):
            issues.append({"severity":"info","category":"scipy",
                "message":"Test estadístico sin verificar p-value",
                "suggestion":"Verifica: if result.pvalue < 0.05: ..."})

    return issues


def _ml_diagram(pipeline: list, models: list, filename: str) -> str:
    if not pipeline:
        return (
            f"flowchart TD\n"
            f'    A[📄 {filename}]\n'
            f"    B[Sin pipeline ML detectado]\n"
            f"    A --> B"
        )
    lines = ["flowchart TD", f'    START([🤖 Pipeline: {filename}])']
    prev  = "START"
    for i, step in enumerate(pipeline[:10]):
        sid   = f"S{i}"
        label = f'{step["icon"]} {step["description"]}'
        if step.get("count", 1) > 1:
            label += f'\\n({step["count"]}x)'
        lines.append(f'    {sid}["{label}"]')
        lines.append(f'    {prev} --> {sid}')
        prev = sid

    for j, model in enumerate(models[:4]):
        mid = f"M{j}"
        lines.append(f'    {mid}([⚡ {model["name"]}\\n{model["type"]}])')
        train_node = next(
            (f"S{i}" for i, s in enumerate(pipeline[:10]) if s["id"] == "entrenamiento"),
            prev,
        )
        lines.append(f'    {train_node} --> {mid}')

    lines.append('    END([✅ Fin])')
    lines.append(f'    {prev} --> END')

    for i, step in enumerate(pipeline[:10]):
        sid = f"S{i}"
        if step["id"] in ("entrenamiento", "backprop", "compilacion"):
            lines.append(f'    style {sid} fill:#0f3020,stroke:#00f5a0,color:#c8d4f0')
        elif step["id"] in ("evaluacion", "prediccion"):
            lines.append(f'    style {sid} fill:#1a1040,stroke:#b87dff,color:#c8d4f0')
        elif step["id"] in ("limpieza", "escalado", "encoding", "features"):
            lines.append(f'    style {sid} fill:#1a2040,stroke:#3d9eff,color:#c8d4f0')
        elif step["id"] in ("carga_datos", "dataloader", "data_pipeline"):
            lines.append(f'    style {sid} fill:#0a1a40,stroke:#ffb627,color:#c8d4f0')
        else:
            lines.append(f'    style {sid} fill:#1a1820,stroke:#4a5880,color:#c8d4f0')

    for j in range(len(models[:4])):
        lines.append(f'    style M{j} fill:#300a1a,stroke:#ff3366,color:#c8d4f0')

    lines.append('    style START fill:#0a2040,stroke:#3d9eff,color:#c8d4f0')
    lines.append('    style END fill:#0a2040,stroke:#00f5a0,color:#c8d4f0')
    return "\n".join(lines)


def _ml_score(content: str, result: dict) -> tuple[int, list[str]]:
    score       = 100
    suggestions: list[str] = []
    issues      = result["issues"]
    pipeline    = result["pipeline"]

    score -= len([i for i in issues if i["severity"] == "error"])   * 15
    score -= len([i for i in issues if i["severity"] == "warning"]) * 7

    if any(s["id"] == "evaluacion" for s in pipeline): score += 5
    if any(s["id"] == "split"      for s in pipeline): score += 5
    if re.search(r'cross_val_score|KFold', content):   score += 10
    if re.search(r'random_state\s*=', content):        score += 5
    if len(pipeline) >= 4:                             score += 5

    if not any(s["id"] == "evaluacion" for s in pipeline):
        suggestions.append("Agrega métricas de evaluación (accuracy_score, f1_score)")
    if not any(s["id"] == "guardado" for s in pipeline):
        suggestions.append("Guarda el modelo con joblib.dump() o model.save()")
    if not any(s["id"] in ("limpieza", "escalado") for s in pipeline):
        suggestions.append("Agrega preprocesamiento (StandardScaler, fillna)")

    for iss in issues:
        if iss.get("suggestion"):
            suggestions.append(iss["suggestion"])

    return max(0, min(100, score)), list(dict.fromkeys(suggestions))[:8]
