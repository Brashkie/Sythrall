"""
CodeWatch PRO — Backend Flask v3.0
flake8 + pylint + radon + ML/DL inspector (numpy, pandas, sklearn, pytorch, tensorflow)
"""
import os, ast, sys, json, time, re, tempfile, subprocess, traceback
from datetime import datetime
from pathlib import Path

import requests as req_lib
from flask import Flask, request, jsonify
from flask_cors import CORS

# ── Librerías de análisis de código
try:
    import flake8; HAS_FLAKE8 = True
except ImportError: HAS_FLAKE8 = False

try:
    import pylint; HAS_PYLINT = True
except ImportError: HAS_PYLINT = False

try:
    from radon.complexity import cc_visit, cc_rank
    from radon.metrics import mi_visit
    from radon.raw import analyze as radon_raw
    HAS_RADON = True
except ImportError: HAS_RADON = False

# ── ML/DL libs
try:
    import numpy as np; HAS_NUMPY = True
except ImportError: HAS_NUMPY = False

try:
    import pandas as pd; HAS_PANDAS = True
except ImportError: HAS_PANDAS = False

try:
    import sklearn; HAS_SKLEARN = True
except ImportError: HAS_SKLEARN = False

try:
    import torch; HAS_TORCH = True
except ImportError: HAS_TORCH = False

try:
    import tensorflow as tf; HAS_TF = True
except ImportError: HAS_TF = False

try:
    import scipy; HAS_SCIPY = True
except ImportError: HAS_SCIPY = False

try:
    import cv2; HAS_CV2 = True
except ImportError: HAS_CV2 = False

try:
    import plotly; HAS_PLOTLY = True
except ImportError: HAS_PLOTLY = False

try:
    import polars as pl; HAS_POLARS = True
except ImportError: HAS_POLARS = False

try:
    import lightgbm as lgb; HAS_LGB = True
except ImportError: HAS_LGB = False

try:
    import spacy; HAS_SPACY = True
except ImportError: HAS_SPACY = False

try:
    import icecream; HAS_ICECREAM = True
except ImportError: HAS_ICECREAM = False

try:
    import Cython; HAS_CYTHON = True
except ImportError: HAS_CYTHON = False

# ════════════════════════════════════════════════
app = Flask(__name__)
CORS(app, origins="*")

LOG_HISTORY = []
API_HISTORY = {}

# ════════════════════════════════════════════════
#  HELPERS
# ════════════════════════════════════════════════

def now():
    return datetime.now().strftime("%Y-%m-%d %H:%M:%S")

def add_log(level, msg):
    entry = {"ts": now(), "level": level, "msg": msg}
    LOG_HISTORY.append(entry)
    if len(LOG_HISTORY) > 200: LOG_HISTORY.pop(0)
    print(f"[{level.upper()}] {msg}")

def save_temp(content, suffix):
    tmp = tempfile.NamedTemporaryFile(mode="w", suffix=suffix, delete=False, encoding="utf-8")
    tmp.write(content); tmp.close(); return tmp.name

def safe_remove(path):
    try: os.unlink(path)
    except: pass

def _get_lib_version(lib_name):
    try:
        if lib_name == "numpy"      and HAS_NUMPY:   return np.__version__
        if lib_name == "pandas"     and HAS_PANDAS:  return pd.__version__
        if lib_name == "torch"      and HAS_TORCH:   return torch.__version__
        if lib_name == "tensorflow" and HAS_TF:      return tf.__version__
        if lib_name == "scipy"      and HAS_SCIPY:    return scipy.__version__
        if lib_name == "cv2"        and HAS_CV2:      return cv2.__version__
        if lib_name == "plotly"     and HAS_PLOTLY:   return plotly.__version__
        if lib_name == "icecream"   and HAS_ICECREAM: return icecream.__version__
        if lib_name == "polars"     and HAS_POLARS:   return pl.__version__
        if lib_name == "lightgbm"   and HAS_LGB:      return lgb.__version__
        if lib_name == "spacy"      and HAS_SPACY:    return spacy.__version__
        if lib_name == "cython"     and HAS_CYTHON:
            import Cython; return Cython.__version__
        if lib_name == "sklearn"    and HAS_SKLEARN:
            import sklearn; return sklearn.__version__
    except: pass
    return None

# ════════════════════════════════════════════════
#  HEALTH / CAPABILITIES
# ════════════════════════════════════════════════

@app.route("/health", methods=["GET"])
def health():
    return jsonify({
        "status": "ok", "ts": now(),
        "capabilities": {
            "flake8": HAS_FLAKE8, "pylint": HAS_PYLINT, "radon": HAS_RADON,
            "numpy": HAS_NUMPY, "pandas": HAS_PANDAS,
            "sklearn": HAS_SKLEARN, "torch": HAS_TORCH, "tensorflow": HAS_TF,
            "cython": HAS_CYTHON,
        }
    })

@app.route("/capabilities", methods=["GET"])
def capabilities():
    caps = {
        "python": sys.version, "server": "CodeWatch PRO Backend v3.0", "ts": now(),
        "flake8": HAS_FLAKE8, "pylint": HAS_PYLINT, "radon": HAS_RADON,
        "numpy": HAS_NUMPY, "pandas": HAS_PANDAS,
        "sklearn": HAS_SKLEARN, "torch": HAS_TORCH, "tensorflow": HAS_TF,
        "scipy": HAS_SCIPY, "opencv": HAS_CV2, "plotly": HAS_PLOTLY, "icecream": HAS_ICECREAM,
        "cython": HAS_CYTHON,
    }
    for tool, flag, cmd in [
        ("flake8_version",  HAS_FLAKE8, [sys.executable,"-m","flake8","--version"]),
        ("pylint_version",  HAS_PYLINT, [sys.executable,"-m","pylint","--version"]),
    ]:
        if flag:
            try:
                r = subprocess.run(cmd, capture_output=True, text=True)
                caps[tool] = r.stdout.strip().splitlines()[0]
            except: pass
    if HAS_NUMPY:  caps["numpy_version"]   = np.__version__
    if HAS_PANDAS: caps["pandas_version"]  = pd.__version__
    if HAS_SKLEARN:
        import sklearn; caps["sklearn_version"] = sklearn.__version__
    if HAS_TORCH:  caps["torch_version"]   = torch.__version__
    if HAS_TF:     caps["tf_version"]      = tf.__version__
    if HAS_SCIPY:    caps["scipy_version"]    = scipy.__version__ if HAS_SCIPY else None
    if HAS_CV2:      caps["opencv_version"]   = cv2.__version__ if HAS_CV2 else None
    if HAS_PLOTLY:   caps["plotly_version"]   = plotly.__version__ if HAS_PLOTLY else None
    if HAS_ICECREAM: caps["icecream_version"] = icecream.__version__ if HAS_ICECREAM else None
    if HAS_POLARS:   caps["polars_version"]   = pl.__version__ if HAS_POLARS else None
    if HAS_LGB:      caps["lightgbm_version"] = lgb.__version__ if HAS_LGB else None
    if HAS_SPACY:    caps["spacy_version"]    = spacy.__version__ if HAS_SPACY else None
    if HAS_CYTHON:
        import Cython; caps["cython_version"] = Cython.__version__
    return jsonify(caps)

# ════════════════════════════════════════════════
#  ANALIZAR CÓDIGO
# ════════════════════════════════════════════════

@app.route("/analyze/code", methods=["POST"])
def analyze_code():
    data     = request.get_json(force=True)
    filename = data.get("filename", "script.py")
    content  = data.get("content", "")
    tools    = data.get("tools", ["ast","flake8","pylint","radon"])
    ext      = Path(filename).suffix.lower()
    result   = {"filename": filename, "ts": now(), "issues": [], "metrics": {},
                "complexity": [], "maintainability": None, "raw_stats": {}, "tools_used": []}
    tmp_path = None
    try:
        if ext == ".py" and "ast" in tools:
            result["issues"].extend(_run_ast(content, filename))
            result["tools_used"].append("ast")
        if ext == ".py":
            tmp_path = save_temp(content, ".py")
        if ext == ".py" and "flake8" in tools:
            result["issues"].extend(_run_flake8(tmp_path))
            result["tools_used"].append("flake8")
        if ext == ".py" and "pylint" in tools:
            pl, score = _run_pylint(tmp_path)
            result["issues"].extend(pl)
            result["metrics"]["pylint_score"] = score
            result["tools_used"].append("pylint")
        if ext == ".py" and "radon" in tools and HAS_RADON:
            cx, mi, raw = _run_radon(content, filename)
            result["complexity"] = cx
            result["maintainability"] = mi
            result["raw_stats"] = raw
            result["tools_used"].append("radon")
        if ext == ".json":
            try: json.loads(content)
            except json.JSONDecodeError as e:
                result["issues"].append({"tool":"json","line":e.lineno,"col":e.colno,
                    "severity":"error","code":"E999","message":f"JSON inválido: {e.msg}"})
        seen, unique = set(), []
        for iss in result["issues"]:
            k = (iss.get("line"), iss.get("message","")[:40])
            if k not in seen: seen.add(k); unique.append(iss)
        result["issues"] = sorted(unique, key=lambda x: x.get("line") or 0)
    except Exception as e:
        add_log("error", f"Error analizando {filename}: {e}")
        result["error"] = str(e)
    finally:
        if tmp_path: safe_remove(tmp_path)
    return jsonify(result)


def _run_ast(content, filename):
    issues = []
    lines  = content.splitlines()
    try:
        tree = ast.parse(content, filename=filename)
    except SyntaxError as e:
        return [{"tool":"ast","line":e.lineno,"col":e.offset,"severity":"error",
                 "code":"E001","message":f"SyntaxError: {e.msg}"}]
    for node in ast.walk(tree):
        ln = getattr(node, "lineno", None)
        if isinstance(node, ast.Assert):
            issues.append({"tool":"ast","line":ln,"col":0,"severity":"warning","code":"W001","message":"assert puede desactivarse con -O"})
        if isinstance(node, ast.ExceptHandler) and node.type is None:
            issues.append({"tool":"ast","line":ln,"col":0,"severity":"warning","code":"W002","message":"except genérico captura todo"})
        if isinstance(node, ast.Global):
            issues.append({"tool":"ast","line":ln,"col":0,"severity":"warning","code":"W003","message":f"Variable global: {', '.join(node.names)}"})
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            if not (node.body and isinstance(node.body[0], ast.Expr) and isinstance(node.body[0].value, ast.Constant)):
                issues.append({"tool":"ast","line":ln,"col":0,"severity":"info","code":"C001","message":f"'{node.name}' sin docstring"})
        if isinstance(node, ast.Call):
            func = node.func
            if isinstance(func, ast.Name) and func.id == "print":
                issues.append({"tool":"ast","line":ln,"col":0,"severity":"info","code":"C002","message":"print() — usa logging"})
    for i, line in enumerate(lines, 1):
        if len(line) > 120:
            issues.append({"tool":"ast","line":i,"col":121,"severity":"warning","code":"E501","message":f"Línea de {len(line)} chars"})
    return issues


def _run_flake8(filepath):
    issues = []
    try:
        result = subprocess.run(
            [sys.executable,"-m","flake8","--max-line-length=120",
             "--format=%(row)d|%(col)d|%(code)s|%(text)s", filepath],
            capture_output=True, text=True, timeout=30)
        for line in result.stdout.splitlines():
            parts = line.strip().split("|")
            if len(parts) >= 4:
                try:
                    issues.append({"tool":"flake8","line":int(parts[0]),"col":int(parts[1]),
                        "severity":"error" if parts[2].startswith("E") else "warning",
                        "code":parts[2],"message":"|".join(parts[3:])})
                except: pass
    except Exception as e:
        issues.append({"tool":"flake8","line":0,"col":0,"severity":"info","code":"ERR","message":str(e)})
    return issues


def _run_pylint(filepath):
    issues, score = [], None
    try:
        result = subprocess.run(
            [sys.executable,"-m","pylint","--output-format=json","--disable=C0114,C0115", filepath],
            capture_output=True, text=True, timeout=45)
        raw = result.stdout.strip()
        if raw:
            try:
                for item in json.loads(raw):
                    sev = {"error":"error","warning":"warning","convention":"info",
                           "refactor":"info","fatal":"error"}.get(item.get("type","warning"),"warning")
                    issues.append({"tool":"pylint","line":item.get("line",0),"col":item.get("column",0),
                        "severity":sev,"code":item.get("message-id",""),"message":item.get("message","")})
            except: pass
        for line in (result.stderr+result.stdout).splitlines():
            if "Your code has been rated at" in line:
                try: score = float(line.split("at")[1].split("/")[0].strip())
                except: pass
    except Exception as e:
        issues.append({"tool":"pylint","line":0,"col":0,"severity":"info","code":"ERR","message":str(e)})
    return issues, score


def _run_radon(content, filename):
    cx_list, mi_score, raw_stats = [], None, {}
    try:
        for block in cc_visit(content):
            cx_list.append({"name":block.name,"type":getattr(block,"type","function"),
                "line":block.lineno,"complexity":block.complexity,"rank":cc_rank(block.complexity)})
        mi_raw = mi_visit(content, multi=True)
        mi_score = round(mi_raw, 2) if isinstance(mi_raw, float) else None
        raw = radon_raw(content)
        raw_stats = {"loc":raw.loc,"lloc":raw.lloc,"sloc":raw.sloc,
                     "comments":raw.comments,"blank":raw.blank,"multi":raw.multi}
    except Exception as e:
        raw_stats["error"] = str(e)
    return cx_list, mi_score, raw_stats

# ════════════════════════════════════════════════
#  ANALIZAR ML/DL
# ════════════════════════════════════════════════

ML_LIB_MAP = {
    "numpy":{"name":"NumPy","category":"datos","color":"#4d9fff"},
    "np":{"name":"NumPy","category":"datos","color":"#4d9fff"},
    "pandas":{"name":"Pandas","category":"datos","color":"#4d9fff"},
    "pd":{"name":"Pandas","category":"datos","color":"#4d9fff"},
    "sklearn":{"name":"Scikit-learn","category":"ml","color":"#f5a623"},
    "torch":{"name":"PyTorch","category":"dl","color":"#ee4c2c"},
    "torchvision":{"name":"TorchVision","category":"dl","color":"#ee4c2c"},
    "tensorflow":{"name":"TensorFlow","category":"dl","color":"#ff6b35"},
    "tf":{"name":"TensorFlow","category":"dl","color":"#ff6b35"},
    "keras":{"name":"Keras","category":"dl","color":"#d00000"},
    "matplotlib":{"name":"Matplotlib","category":"viz","color":"#00c07a"},
    "plt":{"name":"Matplotlib","category":"viz","color":"#00c07a"},
    "seaborn":{"name":"Seaborn","category":"viz","color":"#00c07a"},
    "sns":{"name":"Seaborn","category":"viz","color":"#00c07a"},
    "xgboost":{"name":"XGBoost","category":"ml","color":"#f5a623"},
    "lightgbm":{"name":"LightGBM","category":"ml","color":"#f5a623"},
    "cv2":{"name":"OpenCV","category":"vision","color":"#b87dff"},
    "PIL":{"name":"Pillow","category":"vision","color":"#b87dff"},
    "transformers":{"name":"HuggingFace","category":"nlp","color":"#ffdd57"},
    "nltk":{"name":"NLTK","category":"nlp","color":"#ffdd57"},
    "spacy":{"name":"spaCy","category":"nlp","color":"#ffdd57"},
    "scipy":{"name":"SciPy","category":"ciencia","color":"#4d9fff"},
    "cython":{"name":"Cython","category":"rendimiento","color":"#ffd43b"},
    "pyximport":{"name":"Cython","category":"rendimiento","color":"#ffd43b"},
    "cython.parallel":{"name":"Cython Parallel","category":"rendimiento","color":"#ffd43b"},
}

PIPELINE_PATTERNS = [
    (r'\bpd\.read_csv\b|\bpd\.read_excel\b|\bpd\.read_json\b|\bpd\.read_parquet\b|\bpd\.read_sql\b',
        "carga_datos","Carga de datos","📥"),
    (r'\bdropna\b|\bfillna\b|\bdrop_duplicates\b|SimpleImputer',
        "limpieza","Limpieza de datos","🧹"),
    (r'\bLabelEncoder\b|\bOneHotEncoder\b|\bget_dummies\b|\bOrdinalEncoder\b',
        "encoding","Encoding categórico","🔢"),
    (r'\bStandardScaler\b|\bMinMaxScaler\b|\bRobustScaler\b|\bnormalize\b',
        "escalado","Escalado/Normalización","⚖️"),
    (r'\btrain_test_split\b|\bKFold\b|\bStratifiedKFold\b|\bcross_val_score\b',
        "split","División train/test","✂️"),
    (r'\bPCA\b|\bTSNE\b|\bSelectKBest\b|feature_selection',
        "features","Selección de features","🔍"),
    (r'\bnn\.Module\b|\bnn\.Sequential\b|\bnn\.Linear\b|\bnn\.Conv2d\b',
        "arquitectura","Definición de arquitectura","🧠"),
    (r'\bmodel\.compile\b|\boptim\.\w+\(|\bAdam\b|\bSGD\b|\bAdamW\b',
        "optimizador","Configuración del optimizador","⚙️"),
    (r'\bDataLoader\b|\bDataset\b|\bImageDataGenerator\b|\btf\.data\.Dataset\b',
        "dataloader","Pipeline de datos","🔄"),
    (r'\bfit\s*\(|\bfit_transform\s*\(|\bmodel\.fit\b|\bloss\.backward\(\)',
        "entrenamiento","Entrenamiento del modelo","🏋️"),
    (r'\bEarlyStopping\b|\bModelCheckpoint\b|\bCallbacks\b|\bTensorBoard\b',
        "callbacks","Callbacks de entrenamiento","🔔"),
    (r'\bpredict\s*\(|\bpredict_proba\s*\(|\bmodel\.predict\b',
        "prediccion","Predicción/Inferencia","🎯"),
    (r'\baccuracy_score\b|\bconfusion_matrix\b|\bclassification_report\b'
     r'|\br2_score\b|\bmean_squared_error\b|\bf1_score\b|\broc_auc_score\b',
        "evaluacion","Evaluación del modelo","📊"),
    (r'\bjoblib\.dump\b|\bpickle\.dump\b|\bmodel\.save\b|\btorch\.save\b',
        "guardado","Guardado del modelo","💾"),
    (r'\bplt\.plot\b|\bplt\.show\b|\bsns\.heatmap\b|\bplt\.figure\b',
        "visualizacion","Visualizacion Matplotlib","📈"),
    (r'\bpx\.scatter\b|\bpx\.line\b|\bpx\.bar\b|\bgo\.Figure\b|\bplotly\.',
        "viz_interactiva","Visualizacion interactiva Plotly","📊"),
    (r'\bcv2\.imread\b|\bcv2\.resize\b|\bcv2\.cvtColor\b|\bcv2\.VideoCapture\b',
        "vision","Procesamiento de imagenes OpenCV","📷"),
    (r'\bscipy\.stats\b|\bscipy\.optimize\b|\bscipy\.signal\b|\bscipy\.linalg\b',
        "scipy_calc","Calculo cientifico SciPy","🔬"),
    (r'\bic\s*\(|from icecream import',
        "debugging","Debug con IceCream","🍦"),
    (r'\bpl\.read_csv\b|\bpl\.read_parquet\b|\bpl\.DataFrame\b',
        "carga_polars","Carga datos Polars","🐻"),
    (r'\blgb\.train\b|\bLGBMClassifier\b|\bLGBMRegressor\b',
        "lightgbm_train","Modelo LightGBM","🌿"),
    (r'\bspacy\.load\b|nlp\s*=\s*spacy',
        "nlp_spacy","Procesamiento NLP spaCy","🔤"),
    (r'\bcdef\s|\bcpdef\s|\bctypedef\s|\bpyximport\b|cimport\b',
        "cython_compile","Compilación Cython","⚡"),
]

MODEL_PATTERNS = {
    "RandomForestClassifier":{"type":"clasificación","family":"ensemble","framework":"sklearn"},
    "RandomForestRegressor":{"type":"regresión","family":"ensemble","framework":"sklearn"},
    "GradientBoostingClassifier":{"type":"clasificación","family":"boosting","framework":"sklearn"},
    "LogisticRegression":{"type":"clasificación","family":"lineal","framework":"sklearn"},
    "LinearRegression":{"type":"regresión","family":"lineal","framework":"sklearn"},
    "SVC":{"type":"clasificación","family":"svm","framework":"sklearn"},
    "KMeans":{"type":"clustering","family":"clustering","framework":"sklearn"},
    "DecisionTreeClassifier":{"type":"clasificación","family":"árbol","framework":"sklearn"},
    "XGBClassifier":{"type":"clasificación","family":"boosting","framework":"xgboost"},
    "LGBMClassifier":{"type":"clasificacion","family":"boosting","framework":"lightgbm"},
    "LGBMRegressor":{"type":"regresion","family":"boosting","framework":"lightgbm"},
    "Linear":{"type":"capa densa","family":"linear","framework":"pytorch"},
    "Conv2d":{"type":"capa conv","family":"cnn","framework":"pytorch"},
    "LSTM":{"type":"recurrente","family":"rnn","framework":"pytorch/keras"},
    "GRU":{"type":"recurrente","family":"rnn","framework":"pytorch/keras"},
    "Transformer":{"type":"transformer","family":"attention","framework":"pytorch"},
    "Dense":{"type":"capa densa","family":"linear","framework":"keras"},
    "Conv2D":{"type":"capa conv","family":"cnn","framework":"keras"},
    "Dropout":{"type":"regularización","family":"dropout","framework":"keras"},
    "BatchNormalization":{"type":"normalización","family":"batchnorm","framework":"keras"},
    "Embedding":{"type":"embedding","family":"nlp","framework":"keras"},
    "MultiHeadAttention":{"type":"atención","family":"transformer","framework":"keras"},
    "BertModel":{"type":"BERT","family":"transformer","framework":"transformers"},
    "GPT2Model":{"type":"GPT-2","family":"transformer","framework":"transformers"},
    "AutoModel":{"type":"transformer","family":"pretrained","framework":"transformers"},
}

METRIC_PATTERNS = {
    "accuracy":r'\baccuracy\b|\bacc\b',
    "loss":r'\bloss\b',
    "val_loss":r'\bval_loss\b',
    "val_accuracy":r'\bval_accuracy\b|\bval_acc\b',
    "mae":r'\bmae\b|\bmean_absolute_error\b',
    "mse":r'\bmse\b|\bmean_squared_error\b',
    "r2":r'\br2_score\b|\br2\b',
    "f1":r'\bf1_score\b|\bf1\b',
    "auc":r'\broc_auc_score\b|\bauc\b',
    "precision":r'\bprecision_score\b|\bprecision\b',
    "recall":r'\brecall_score\b|\brecall\b',
    "epochs":r'\bepochs\b',
    "batch_size":r'\bbatch_size\b',
    "learning_rate":r'\blearning_rate\b|\blr\b',
}


@app.route("/analyze/ml", methods=["POST"])
def analyze_ml():
    data     = request.get_json(force=True)
    filename = data.get("filename","script.py")
    content  = data.get("content","")
    result   = {
        "filename":filename,"ts":now(),
        "libraries":[],"pipeline":[],"issues":[],
        "metrics":{},"models":[],"diagram":"",
        "score":0,"suggestions":[],
    }
    try:
        tree  = ast.parse(content)
        lines = content.splitlines()
        result["libraries"] = _detect_ml_libraries(tree)
        result["pipeline"]  = _detect_pipeline(content)
        result["models"]    = _detect_models(content)
        result["metrics"]   = _detect_metrics(content)
        result["issues"]    = _detect_ml_issues(content, result["libraries"])
        result["diagram"]   = _ml_diagram(result["pipeline"], result["models"], filename)
        result["score"], result["suggestions"] = _ml_score(content, result)
        add_log("info", f"ML analizado: {filename} — {len(result['libraries'])} libs, {len(result['pipeline'])} etapas")
    except SyntaxError as e:
        result["issues"].append({"severity":"error","message":f"SyntaxError línea {e.lineno}: {e.msg}"})
    except Exception as e:
        add_log("error", f"Error ML {filename}: {e}")
        result["error"] = str(e)
    return jsonify(result)


def _detect_ml_libraries(tree):
    found = {}
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


def _detect_pipeline(content):
    steps, seen = [], set()
    for pattern, stage_id, description, icon in PIPELINE_PATTERNS:
        matches = list(re.finditer(pattern, content))
        if matches and stage_id not in seen:
            seen.add(stage_id)
            line_no = content[:matches[0].start()].count("\n") + 1
            steps.append({"id":stage_id,"description":description,"icon":icon,
                          "line":line_no,"count":len(matches)})
    return sorted(steps, key=lambda x: x["line"])


def _detect_models(content):
    found, seen = [], set()
    for name, info in MODEL_PATTERNS.items():
        if re.search(r'\b' + re.escape(name) + r'\b', content) and name not in seen:
            seen.add(name)
            m = re.search(r'\b' + re.escape(name) + r'\b', content)
            found.append({**info, "name":name, "line":content[:m.start()].count("\n")+1 if m else 0})
    return found


def _detect_metrics(content):
    metrics = {}
    for metric, pattern in METRIC_PATTERNS.items():
        matches = re.findall(pattern, content, re.IGNORECASE)
        if matches:
            val = None
            m = re.search(pattern + r'\s*[=:]\s*([\d.]+)', content, re.IGNORECASE)
            if m:
                try: val = float(m.group(1))
                except: pass
            metrics[metric] = {"found":True,"count":len(matches),"value":val}
    return metrics


def _detect_ml_issues(content, libraries):
    issues = []
    lib_names = {lib.get("name","").lower() for lib in libraries}
    lib_imports = " ".join(lib.get("import","") for lib in libraries).lower()

    has_torch = "pytorch" in lib_names or "torch" in lib_imports
    has_tf    = "tensorflow" in lib_names or "keras" in lib_names or "tensorflow" in lib_imports
    has_np    = "numpy" in lib_names or "np" in lib_imports
    has_sk    = "scikit-learn" in lib_names or "sklearn" in lib_imports

    # Data leakage — fit antes de split
    scaler_pos = [m.start() for m in re.finditer(r'\bfit_transform\b|\bfit\b', content)]
    split_pos  = [m.start() for m in re.finditer(r'\btrain_test_split\b', content)]
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
            "message":"PyTorch sin torch.manual_seed()","suggestion":"Agrega torch.manual_seed(42)"})

    if has_tf and not re.search(r'tf\.random\.set_seed\b|keras\.utils\.set_random_seed\b', content):
        issues.append({"severity":"warning","category":"reproducibilidad",
            "message":"TensorFlow sin tf.random.set_seed()","suggestion":"Agrega tf.random.set_seed(42)"})

    if has_np and not re.search(r'np\.random\.seed\b', content):
        issues.append({"severity":"info","category":"reproducibilidad",
            "message":"NumPy sin np.random.seed()","suggestion":"Agrega np.random.seed(42)"})

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

    # LightGBM sin early stopping
    if re.search(r"lgb\.train|LGBMClassifier|LGBMRegressor", content):
        if not re.search(r"early_stopping", content):
            issues.append({"severity":"warning","category":"lightgbm",
                "message":"LightGBM sin early_stopping",
                "suggestion":"Agrega callbacks=[lgb.early_stopping(50)] para evitar overfitting"})
        if not re.search(r"num_leaves|max_depth", content):
            issues.append({"severity":"info","category":"lightgbm",
                "message":"Hiperparametros clave no definidos en LightGBM",
                "suggestion":"Define num_leaves y max_depth para controlar el modelo"})

    # spaCy nlp() en loop vs nlp.pipe()
    if re.search(r"spacy\.load|nlp\s*=\s*spacy", content):
        if not re.search(r"try.*spacy\.load|except.*OSError", content):
            issues.append({"severity":"warning","category":"spacy",
                "message":"spacy.load() sin manejo de error",
                "suggestion":"Envuelve en try/except: el modelo puede no estar descargado"})
        loop_nlp = re.findall(r"for\s+\w+.*:\s*\n\s+.*nlp\s*\(", content)
        if loop_nlp:
            issues.append({"severity":"warning","category":"spacy",
                "message":"nlp() llamado dentro de un bucle",
                "suggestion":"Usa nlp.pipe(textos, batch_size=256) para procesar en lote"})

    # IceCream ic() en produccion
    ic_matches = re.findall(r'\bic\s*\(', content)
    if ic_matches:
        n = len(ic_matches)
        if n > 5:
            issues.append({"severity":"warning","category":"icecream",
                "message":f"Se encontraron {n} llamadas ic() — eliminar en produccion",
                "suggestion":"Usa ic.disable() o remueve los ic() antes de produccion"})
        else:
            issues.append({"severity":"info","category":"icecream",
                "message":f"IceCream activo ({n} llamadas ic())",
                "suggestion":"Recuerda desactivarlo en produccion con ic.disable()"})

    # OpenCV: imread sin verificar None
    if re.search(r'cv2\.imread\b', content):
        if not re.search(r'is None|== None|if.*img|if.*image', content):
            issues.append({"severity":"warning","category":"opencv",
                "message":"cv2.imread() sin verificacion de None",
                "suggestion":"Agrega: if img is None: raise FileNotFoundError(...)"})

    # OpenCV: VideoCapture sin release
    if re.search(r'cv2\.VideoCapture\b', content):
        if not re.search(r'\.release\(\)', content):
            issues.append({"severity":"warning","category":"opencv",
                "message":"VideoCapture sin .release()",
                "suggestion":"Llama cap.release() al terminar para liberar recursos"})

    # Plotly: figura sin show() ni write_html()
    if re.search(r'px\.|go\.Figure|plotly\.express', content):
        if not re.search(r'\.show\(\)|\.write_html\(|\.write_image\(', content):
            issues.append({"severity":"info","category":"plotly",
                "message":"Figura Plotly sin .show() ni .write_html()",
                "suggestion":"Usa fig.show() o fig.write_html('salida.html')"})

    # SciPy: test estadistico sin p-value
    if re.search(r'ttest_|chi2_contingency|mannwhitneyu|anova', content):
        if not re.search(r'p_value|pvalue|\.pvalue', content):
            issues.append({"severity":"info","category":"scipy",
                "message":"Test estadistico sin verificar p-value",
                "suggestion":"Verifica: if result.pvalue < 0.05: ..."})

    return issues


def _ml_diagram(pipeline, models, filename):
    if not pipeline:
        return f"flowchart TD\n    A[📄 {filename}]\n    B[Sin pipeline ML detectado]\n    A --> B"
    lines = ["flowchart TD", f'    START([🤖 Pipeline: {filename}])']
    prev  = "START"
    for i, step in enumerate(pipeline[:10]):
        sid   = f"S{i}"
        label = f'{step["icon"]} {step["description"]}'
        if step.get("count",1) > 1: label += f'\\n({step["count"]}x)'
        lines.append(f'    {sid}["{label}"]')
        lines.append(f'    {prev} --> {sid}')
        prev = sid
    for j, model in enumerate(models[:4]):
        mid = f"M{j}"
        lines.append(f'    {mid}([⚡ {model["name"]}\\n{model["type"]}])')
        train_node = next((f"S{i}" for i,s in enumerate(pipeline[:10]) if s["id"]=="entrenamiento"), prev)
        lines.append(f'    {train_node} --> {mid}')
    lines.append(f'    END([✅ Fin])')
    lines.append(f'    {prev} --> END')
    for i, step in enumerate(pipeline[:10]):
        sid = f"S{i}"
        if step["id"] in ("entrenamiento","backprop","compilacion"):
            lines.append(f'    style {sid} fill:#0f3020,stroke:#00f5a0,color:#c8d4f0')
        elif step["id"] in ("evaluacion","prediccion"):
            lines.append(f'    style {sid} fill:#1a1040,stroke:#b87dff,color:#c8d4f0')
        elif step["id"] in ("limpieza","escalado","encoding","features"):
            lines.append(f'    style {sid} fill:#1a2040,stroke:#3d9eff,color:#c8d4f0')
        elif step["id"] in ("carga_datos","dataloader","data_pipeline"):
            lines.append(f'    style {sid} fill:#0a1a40,stroke:#ffb627,color:#c8d4f0')
        else:
            lines.append(f'    style {sid} fill:#1a1820,stroke:#4a5880,color:#c8d4f0')
    for j in range(len(models[:4])):
        lines.append(f'    style M{j} fill:#300a1a,stroke:#ff3366,color:#c8d4f0')
    lines.append('    style START fill:#0a2040,stroke:#3d9eff,color:#c8d4f0')
    lines.append('    style END fill:#0a2040,stroke:#00f5a0,color:#c8d4f0')
    return "\n".join(lines)


def _ml_score(content, result):
    score = 100
    suggestions = []
    issues   = result["issues"]
    pipeline = result["pipeline"]
    score -= len([i for i in issues if i["severity"]=="error"]) * 15
    score -= len([i for i in issues if i["severity"]=="warning"]) * 7
    if any(s["id"]=="evaluacion" for s in pipeline):  score += 5
    if any(s["id"]=="split"      for s in pipeline):  score += 5
    if re.search(r'cross_val_score|KFold', content):   score += 10
    if re.search(r'random_state\s*=', content):        score += 5
    if len(pipeline) >= 4:                             score += 5
    if not any(s["id"]=="evaluacion" for s in pipeline):
        suggestions.append("Agrega métricas de evaluación (accuracy_score, f1_score)")
    if not any(s["id"]=="guardado" for s in pipeline):
        suggestions.append("Guarda el modelo con joblib.dump() o model.save()")
    if not any(s["id"] in ("limpieza","escalado") for s in pipeline):
        suggestions.append("Agrega preprocesamiento (StandardScaler, fillna)")
    for iss in issues:
        if iss.get("suggestion"): suggestions.append(iss["suggestion"])
    return max(0, min(100, score)), list(dict.fromkeys(suggestions))[:8]

# ════════════════════════════════════════════════
#  VERIFICAR APIs
# ════════════════════════════════════════════════

@app.route("/check/api", methods=["POST"])
def check_api():
    data    = request.get_json(force=True)
    urls    = data.get("urls",[])
    timeout = data.get("timeout",10)
    headers = data.get("headers",{})
    results = []
    for url in urls:
        r = _check_single_url(url, timeout, headers)
        if url not in API_HISTORY: API_HISTORY[url] = []
        API_HISTORY[url].append({"ts":now(),"status":r["status"],"ms":r["ms"],"code":r["code"]})
        API_HISTORY[url] = API_HISTORY[url][-50:]
        r["history"] = API_HISTORY[url][-10:]
        results.append(r)
        add_log("info" if r["status"]=="ok" else "warn", f"API {url} → {r['status']} ({r['ms']}ms)")
    return jsonify({"results":results,"ts":now()})


def _check_single_url(url, timeout, headers):
    r = {"url":url,"status":"unknown","code":None,"ms":None,"error":None,
         "headers":{},"content_type":None,"ts":now()}
    try:
        t0   = time.perf_counter()
        resp = req_lib.get(url, timeout=timeout, headers=headers, allow_redirects=True, verify=False)
        r["ms"]           = round((time.perf_counter()-t0)*1000,1)
        r["code"]         = resp.status_code
        r["status"]       = "ok" if resp.status_code < 400 else "error"
        r["content_type"] = resp.headers.get("Content-Type","")
        r["headers"]      = dict(list(resp.headers.items())[:10])
        if "json" in r["content_type"]:
            try: r["json_preview"] = str(resp.json())[:200]
            except: pass
    except req_lib.exceptions.ConnectionError:
        r["status"]="down"; r["error"]="Conexión rechazada"
    except req_lib.exceptions.Timeout:
        r["status"]="down"; r["error"]=f"Timeout >{timeout}s"
    except Exception as e:
        r["status"]="down"; r["error"]=str(e)
    return r

# ════════════════════════════════════════════════
#  ANALIZAR LOGS
# ════════════════════════════════════════════════

@app.route("/analyze/logs", methods=["POST"])
def analyze_logs():
    data  = request.get_json(force=True)
    files = data.get("files",[])
    all_errors, all_warnings, summary = [], [], {}
    KEYWORDS = {"critical":["CRITICAL","FATAL"],"error":["ERROR","Exception","Traceback","Error:"],
                "warning":["WARNING","WARN","DEPRECATED"],"info":["INFO"]}
    for f in files:
        name, content = f.get("name","log.txt"), f.get("content","")
        lines, counts, file_errors = content.splitlines(), {k:0 for k in KEYWORDS}, []
        for i, line in enumerate(lines, 1):
            for level, kws in KEYWORDS.items():
                if any(kw in line for kw in kws):
                    counts[level] += 1
                    if level in ("critical","error"):
                        all_errors.append({"file":name,"lineNo":i,"level":level,"line":line.strip()[:200]})
                        file_errors.append(i)
                    elif level=="warning":
                        all_warnings.append({"file":name,"lineNo":i,"line":line.strip()[:200]})
                    break
        summary[name] = {"total_lines":len(lines),"counts":counts,"error_lines":file_errors[:20]}
        add_log("info", f"Log {name}: {counts['error']} errores")
    return jsonify({"errors":all_errors[:100],"warnings":all_warnings[:100],"summary":summary,"ts":now()})

# ════════════════════════════════════════════════
#  DIAGRAMA MERMAID
# ════════════════════════════════════════════════

@app.route("/analyze/diagram", methods=["POST"])
def analyze_diagram():
    data      = request.get_json(force=True)
    filename  = data.get("filename","script.py")
    content   = data.get("content","")
    diag_type = data.get("diagram_type","flowchart")
    ext       = Path(filename).suffix.lower()
    result    = {"filename":filename,"diagram_type":diag_type,"mermaid":"","ts":now()}
    try:
        if ext == ".py":
            fns = {"flowchart":_py_flowchart,"callgraph":_py_callgraph,"classes":_py_classes,"sequence":_py_sequence}
            result["mermaid"] = fns.get(diag_type, _py_flowchart)(content, filename) if diag_type=="flowchart" else fns.get(diag_type, _py_flowchart)(content)
        else:
            result["mermaid"] = _generic_flowchart(content, filename, ext)
        add_log("info", f"Diagrama '{diag_type}' para {filename}")
    except Exception as e:
        result["mermaid"] = f"flowchart TD\n    ERR[Error: {str(e)[:60]}]"
    return jsonify(result)


def _py_flowchart(content, filename="script.py"):
    try: tree = ast.parse(content)
    except SyntaxError as e: return f"flowchart TD\n    ERR[SyntaxError línea {e.lineno}]"
    funcs = sorted([{"name":n.name,"line":n.lineno,
        "args":[a.arg for a in n.args.args if a.arg!='self'],
        "returns":any(isinstance(x,ast.Return) and x.value for x in ast.walk(n)),
        "is_async":isinstance(n,ast.AsyncFunctionDef),
        "docstring":(ast.get_docstring(n) or "")[:40].replace('"',"'")}
        for n in ast.walk(tree) if isinstance(n,(ast.FunctionDef,ast.AsyncFunctionDef))],
        key=lambda x:x["line"])
    if not funcs: return f"flowchart TD\n    A[📄 {filename}]\n    B[Sin funciones]\n    A --> B"
    lines = ["flowchart TD", f'    START([🚀 {filename}])']
    for i, fn in enumerate(funcs[:12]):
        icon  = "⚡" if fn["is_async"] else "⚙️"
        label = f'{icon} {fn["name"]}({", ".join(fn["args"][:2])})'
        if fn["docstring"]: label += f'\\n📝 {fn["docstring"]}'
        lines.append(f'    F{i}["{label}"]')
    lines += ['    END([🏁 Fin])','    START --> F0']
    for i in range(min(len(funcs),12)-1): lines.append(f'    F{i} --> F{i+1}')
    lines.append(f'    F{min(len(funcs)-1,11)} --> END')
    for i, fn in enumerate(funcs[:12]):
        c = '#300a3a' if fn["is_async"] else ('#0f3020' if fn["returns"] else '#1a2040')
        s = '#b87dff' if fn["is_async"] else ('#00f5a0' if fn["returns"] else '#3d9eff')
        lines.append(f'    style F{i} fill:{c},stroke:{s},color:#c8d4f0')
    lines += ['    style START fill:#0a2040,stroke:#3d9eff,color:#c8d4f0','    style END fill:#0a2040,stroke:#3d9eff,color:#c8d4f0']
    return "\n".join(lines)


def _py_callgraph(content):
    try: tree = ast.parse(content)
    except: return "graph LR\n    ERR[Error de sintaxis]"
    func_names = {n.name for n in ast.walk(tree) if isinstance(n,(ast.FunctionDef,ast.AsyncFunctionDef))}
    call_map, current_fn = {}, None
    for node in ast.walk(tree):
        if isinstance(node,(ast.FunctionDef,ast.AsyncFunctionDef)):
            current_fn = node.name; call_map.setdefault(current_fn,[])
        if current_fn and isinstance(node,ast.Call):
            callee = node.func.id if isinstance(node.func,ast.Name) else (node.func.attr if isinstance(node.func,ast.Attribute) else None)
            if callee and callee in func_names and callee!=current_fn: call_map[current_fn].append(callee)
    if not func_names: return "graph LR\n    A[Sin funciones]"
    lines = ["graph LR"] + [f'    {fn}["⚙️ {fn}"]' for fn in list(func_names)[:12]]
    seen = set()
    for caller, callees in call_map.items():
        for callee in callees:
            k=f"{caller}_{callee}"
            if k not in seen and callee in func_names: seen.add(k); lines.append(f'    {caller} -->|llama| {callee}')
    return "\n".join(lines)


def _py_classes(content):
    try: tree = ast.parse(content)
    except: return "classDiagram\n    class Error"
    classes = []
    for node in ast.walk(tree):
        if isinstance(node,ast.ClassDef):
            methods,attrs=[],[]
            for item in node.body:
                if isinstance(item,(ast.FunctionDef,ast.AsyncFunctionDef)):
                    methods.append({"name":item.name,"args":[a.arg for a in item.args.args if a.arg!='self']})
                if isinstance(item,ast.Assign):
                    for t in item.targets:
                        if isinstance(t,ast.Name): attrs.append(t.id)
            for item in node.body:
                if isinstance(item,ast.FunctionDef) and item.name=='__init__':
                    for n in ast.walk(item):
                        if isinstance(n,ast.Assign):
                            for t in n.targets:
                                if isinstance(t,ast.Attribute) and isinstance(t.value,ast.Name) and t.value.id=='self':
                                    if t.attr not in attrs: attrs.append(t.attr)
            classes.append({"name":node.name,"methods":methods,"attrs":attrs,"bases":[b.id for b in node.bases if isinstance(b,ast.Name)]})
    if not classes: return "classDiagram\n    class SinClases"
    lines = ["classDiagram"]
    for cls in classes[:8]:
        lines.append(f'    class {cls["name"]} {{')
        for a in cls["attrs"][:6]: lines.append(f'        +{a}')
        for m in cls["methods"][:8]: lines.append(f'        +{m["name"]}({", ".join(m["args"][:3])})')
        lines.append('    }')
        for base in cls["bases"]:
            if any(c["name"]==base for c in classes): lines.append(f'    {base} <|-- {cls["name"]} : hereda')
    return "\n".join(lines)


def _py_sequence(content):
    try: tree = ast.parse(content)
    except: return "sequenceDiagram\n    participant Error"
    funcs = sorted([{"name":n.name,"line":n.lineno} for n in ast.walk(tree)
                    if isinstance(n,(ast.FunctionDef,ast.AsyncFunctionDef))], key=lambda x:x["line"])
    if len(funcs)<2: return "sequenceDiagram\n    participant main\n    main->>main: ejecutar\n    main-->>main: fin"
    lines = ["sequenceDiagram","    participant Usuario"]+[f'    participant {fn["name"]}' for fn in funcs[:6]]
    lines.append(f'    Usuario->>+{funcs[0]["name"]}: invocar')
    for i in range(min(len(funcs),5)-1): lines.append(f'    {funcs[i]["name"]}->>+{funcs[i+1]["name"]}: llamar')
    for i in range(min(len(funcs),5)-1,0,-1): lines.append(f'    {funcs[i]["name"]}-->>-{funcs[i-1]["name"]}: retornar')
    lines.append(f'    {funcs[0]["name"]}-->>-Usuario: resultado')
    return "\n".join(lines)


def _generic_flowchart(content, filename, ext):
    lines_list, funcs = content.splitlines(), []
    if ext in [".js",".ts"]:
        for i,line in enumerate(lines_list):
            m=re.search(r'(?:function\s+(\w+)|const\s+(\w+)\s*=\s*(?:async\s*)?\()',line)
            if m:
                name=m.group(1) or m.group(2)
                if name and name not in ['if','for','while']: funcs.append({"name":name,"line":i+1})
    if not funcs:
        return f"flowchart TD\n    A[📄 {filename}]\n    B[{len(lines_list)} líneas · {ext.replace('.','').upper()}]\n    A --> B"
    code = f"flowchart TD\n    START([📄 {filename}])\n"
    for i,fn in enumerate(funcs[:10]): code += f'    F{i}["⚙️ {fn["name"]}()"]\n'
    code += "    END([🏁])\n    START --> F0\n"
    for i in range(min(len(funcs),10)-1): code += f"    F{i} --> F{i+1}\n"
    code += f"    F{min(len(funcs)-1,9)} --> END\n"
    return code

# ════════════════════════════════════════════════
#  LOGS / HISTORIAL
# ════════════════════════════════════════════════

@app.route("/logs", methods=["GET"])
def get_logs():
    limit = int(request.args.get("limit",100))
    return jsonify({"logs":LOG_HISTORY[-limit:],"total":len(LOG_HISTORY)})

@app.route("/api/history", methods=["GET"])
def get_api_history():
    return jsonify({"history":API_HISTORY})

# ════════════════════════════════════════════════
#  MAIN
# ════════════════════════════════════════════════

if __name__ == "__main__":
    port  = int(os.environ.get("PORT",5000))
    debug = os.environ.get("FLASK_DEBUG","0") == "1"
    add_log("info", f"🚀 CodeWatch PRO v3.0 — puerto {port}")
    add_log("info", f"   flake8={'✓' if HAS_FLAKE8 else '✗'} pylint={'✓' if HAS_PYLINT else '✗'} radon={'✓' if HAS_RADON else '✗'}")
    add_log("info", f"   numpy={'✓' if HAS_NUMPY else '✗'} pandas={'✓' if HAS_PANDAS else '✗'} sklearn={'✓' if HAS_SKLEARN else '✗'}")
    add_log("info", f"   pytorch={'✓' if HAS_TORCH else '✗'} tensorflow={'✓' if HAS_TF else '✗'}")
    add_log("info", f"   cython={'✓' if HAS_CYTHON else '✗'}")
    app.run(host="0.0.0.0", port=port, debug=debug)