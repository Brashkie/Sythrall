// ══════════════════════════════════════════
//  CodeWatch PRO — ML/DL Panel
// ══════════════════════════════════════════
// panels/ml.ts
import type { MLAnalysisResult, CodeFile, NotebookEnv, NotebookEnvId } from '../types'

const CAT_COLORS: Record<string, string> = {
  datos:        'var(--info)',
  ml:           'var(--warn)',
  dl:           'var(--err)',
  viz:          '#636efa',
  nlp:          '#09a3d5',
  vision:       '#b87dff',
  ciencia:      'var(--info)',
  debug:        '#ff69b4',
  rendimiento:  '#ffd43b',
}

// ── Definición de entornos de notebook instalables
const NOTEBOOK_ENVS: NotebookEnv[] = [
  {
    id:          'thebe',
    name:        'Thebe',
    description: 'Convierte HTML estático en celdas ejecutables conectadas a un kernel Jupyter (Binder/JupyterHub)',
    npmPackage:  'thebe',
    version:     '0.9.3',
    color:       '#e86e4d',
    icon:        '🔥',
    useCases:    ['Documentación interactiva', 'Notebooks en sitios web', 'Tutoriales ejecutables'],
  },
  {
    id:          'pyodide',
    name:        'Pyodide',
    description: 'Python completo en el navegador vía WebAssembly — sin servidor, sin kernel externo',
    npmPackage:  'pyodide',
    version:     '0.29.3',
    color:       '#3572a5',
    icon:        '🐍',
    useCases:    ['Python offline en browser', 'Ejecución de numpy/pandas en cliente', 'Apps web Python-first'],
  },
  {
    id:          'starboard',
    name:        'Starboard Notebook',
    description: 'Framework para notebooks interactivos en el navegador con soporte nativo de celdas de código y carga de librerías',
    npmPackage:  'starboard-notebook',
    version:     '0.15.7',
    color:       '#f5a623',
    icon:        '⭐',
    useCases:    ['Notebooks embebidos', 'Celdas JS/Python/SQL', 'Dashboards reactivos'],
  },
]

// ── Detecta cuáles entornos usa el archivo (cliente)
function detectNotebookUsage(content: string): NotebookEnvId[] {
  const found: NotebookEnvId[] = []
  if (/thebe|ThebeManager|ThebeSever/i.test(content))             found.push('thebe')
  if (/pyodide|loadPyodide|micropip/i.test(content))              found.push('pyodide')
  if (/starboard|StarboardNotebook|import.*starboard/i.test(content)) found.push('starboard')
  return found
}

export function renderMLResults(data: MLAnalysisResult, container: HTMLElement): void {
  const { libraries, pipeline, models, metrics, issues, suggestions, score, diagram } = data
  const scoreColor = score >= 80 ? 'var(--ok)' : score >= 50 ? 'var(--warn)' : 'var(--err)'
  const scoreLabel = score >= 80 ? 'Excelente' : score >= 60 ? 'Bueno' : score >= 40 ? 'Regular' : 'Mejorar'

  // Separar libs normales de libs de rendimiento (Cython)
  const perfLibs = libraries.filter(l => l.category === 'rendimiento')
  const mlLibs   = libraries.filter(l => l.category !== 'rendimiento')

  container.innerHTML = `
  <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(110px,1fr));gap:10px;margin-bottom:16px">
    <div class="stat-card" style="text-align:center">
      <div class="sc-lbl">Score ML</div>
      <div class="sc-val" style="color:${scoreColor}">${score}</div>
      <div class="sc-sub">${scoreLabel}</div>
    </div>
    <div class="stat-card" style="text-align:center">
      <div class="sc-lbl">Librerías</div>
      <div class="sc-val" style="color:var(--info)">${libraries.length}</div>
      <div class="sc-sub">detectadas</div>
    </div>
    <div class="stat-card" style="text-align:center">
      <div class="sc-lbl">Etapas</div>
      <div class="sc-val" style="color:var(--purple)">${pipeline.length}</div>
      <div class="sc-sub">pipeline</div>
    </div>
    <div class="stat-card" style="text-align:center">
      <div class="sc-lbl">Issues ML</div>
      <div class="sc-val" style="color:${issues.length ? 'var(--err)' : 'var(--ok)'}">${issues.length}</div>
      <div class="sc-sub">encontrados</div>
    </div>
  </div>

  ${mlLibs.length ? `
  <div class="metric-section">
    <div class="ms-title">📦 Librerías ML/DL detectadas</div>
    <div style="display:flex;flex-wrap:wrap;gap:7px;padding:4px 0">
      ${mlLibs.map(lib => `
        <div style="background:var(--s2);border:1px solid var(--b1);border-radius:7px;padding:7px 11px;display:flex;align-items:center;gap:7px">
          <div style="width:9px;height:9px;border-radius:50%;background:${CAT_COLORS[lib.category] ?? 'var(--muted)'}"></div>
          <div>
            <div style="font-size:.78rem;font-weight:600">${lib.name}</div>
            <div style="font-size:.63rem;color:var(--muted);font-family:var(--mono)">${lib.version ? 'v' + lib.version : lib.category}</div>
          </div>
        </div>`).join('')}
    </div>
  </div>` : ''}

  ${perfLibs.length ? `
  <div class="metric-section">
    <div class="ms-title">⚡ Librerías de rendimiento</div>
    <div style="display:flex;flex-wrap:wrap;gap:7px;padding:4px 0">
      ${perfLibs.map(lib => `
        <div style="background:rgba(255,212,59,.08);border:1px solid rgba(255,212,59,.3);border-radius:7px;padding:7px 11px;display:flex;align-items:center;gap:7px">
          <div style="width:9px;height:9px;border-radius:50%;background:#ffd43b"></div>
          <div>
            <div style="font-size:.78rem;font-weight:600;color:#ffd43b">${lib.name}</div>
            <div style="font-size:.63rem;color:var(--muted);font-family:var(--mono)">${lib.version ? 'v' + lib.version : 'rendimiento'}</div>
          </div>
        </div>`).join('')}
    </div>
  </div>` : ''}

  ${pipeline.length ? `
  <div class="metric-section">
    <div class="ms-title">🔄 Pipeline ML (${pipeline.length} etapas)</div>
    ${pipeline.map((step, i) => `
      <div style="display:flex;align-items:center;gap:9px;padding:7px 0;${i < pipeline.length - 1 ? 'border-bottom:1px solid var(--b0)' : ''}">
        <div style="width:26px;height:26px;border-radius:50%;background:${step.id === 'cython_compile' ? 'rgba(255,212,59,.12)' : 'var(--s2)'};border:1px solid ${step.id === 'cython_compile' ? 'rgba(255,212,59,.4)' : 'var(--b1)'};display:grid;place-items:center;font-size:12px;flex-shrink:0">${step.icon}</div>
        <div style="flex:1">
          <div style="font-size:.78rem;font-weight:600;${step.id === 'cython_compile' ? 'color:#ffd43b' : ''}">${step.description}</div>
          <div style="font-size:.63rem;color:var(--muted);font-family:var(--mono)">línea ~${step.line}${step.count > 1 ? ' · ' + step.count + 'x' : ''}</div>
        </div>
        <span style="font-size:.6rem;padding:1px 6px;border-radius:3px;background:var(--s2);color:var(--muted);font-family:var(--mono)">${i + 1}/${pipeline.length}</span>
      </div>`).join('')}
    ${diagram ? `
    <div style="margin-top:12px">
      <button class="btn btn-ghost btn-sm" style="width:100%;justify-content:center" data-ml-diagram='${JSON.stringify(diagram).replace(/'/g, "\\'")}'>
        🔀 Ver diagrama del pipeline
      </button>
    </div>` : ''}
  </div>` : ''}

  ${models.length ? `
  <div class="metric-section">
    <div class="ms-title">🧠 Modelos detectados</div>
    ${models.map(m => `
      <div style="display:flex;align-items:center;gap:9px;padding:6px 0;border-bottom:1px solid var(--b0)">
        <span style="font-size:13px">🔷</span>
        <div style="flex:1">
          <div style="font-size:.78rem;font-weight:600">${m.name}</div>
          <div style="font-size:.63rem;color:var(--muted);font-family:var(--mono)">${m.type} · ${m.family} · ${m.framework}</div>
        </div>
      </div>`).join('')}
  </div>` : ''}

  ${Object.keys(metrics).length ? `
  <div class="metric-section">
    <div class="ms-title">📊 Métricas de entrenamiento</div>
    <div style="display:grid;grid-template-columns:repeat(auto-fill,minmax(100px,1fr));gap:7px">
      ${Object.entries(metrics).map(([k, v]) => `
        <div style="background:var(--s2);border-radius:6px;padding:7px;text-align:center">
          <div style="font-size:.58rem;color:var(--muted);font-family:var(--mono)">${k}</div>
          <div style="font-family:var(--mono);font-size:.8rem;color:var(--info)">${v.value != null ? v.value : '✓'}</div>
          <div style="font-size:.58rem;color:var(--muted)">${v.count}x</div>
        </div>`).join('')}
    </div>
  </div>` : ''}

  ${issues.length ? `
  <div class="metric-section">
    <div class="ms-title">⚠️ Issues ML/DL (${issues.length})</div>
    ${issues.map(iss => `
      <div class="issue-item ii-${iss.severity}" style="margin-bottom:6px">
        <div class="ii-head">
          <span class="ii-sev sev-${iss.severity}">${iss.severity.toUpperCase()}</span>
          ${iss.category ? `<span style="font-size:.6rem;font-family:var(--mono);color:${iss.category === 'cython' ? '#ffd43b' : 'var(--muted)'}">${iss.category}</span>` : ''}
        </div>
        <div class="ii-msg">${iss.message}</div>
        ${iss.suggestion ? `<div style="font-size:.65rem;color:var(--ok);margin-top:3px;font-family:var(--mono)">💡 ${iss.suggestion}</div>` : ''}
      </div>`).join('')}
  </div>` : `<div style="padding:8px 0;color:var(--ok);font-family:var(--mono);font-size:.7rem">✅ Sin issues ML</div>`}

  ${suggestions.length ? `
  <div class="metric-section">
    <div class="ms-title">💡 Sugerencias</div>
    ${suggestions.map((s, i) => `
      <div style="display:flex;gap:8px;align-items:flex-start;padding:6px 0;border-bottom:1px solid var(--b0)">
        <span style="color:var(--info);font-family:var(--mono);font-size:.65rem;flex-shrink:0">${i + 1}.</span>
        <span style="font-size:.72rem">${s}</span>
      </div>`).join('')}
  </div>` : ''}

  ${_renderNotebookSection()}
  `
}

// ── Sección de entornos de notebook (Thebe / Pyodide / Starboard)
function _renderNotebookSection(): string {
  return `
  <div class="metric-section">
    <div class="ms-title">🌐 Entornos de notebook disponibles</div>
    <div style="font-size:.65rem;color:var(--muted);margin-bottom:10px;font-family:var(--mono)">
      Instalados vía npm — listos para integrar en el frontend
    </div>
    ${NOTEBOOK_ENVS.map(env => `
    <div style="background:var(--s2);border:1px solid var(--b1);border-radius:8px;padding:10px 12px;margin-bottom:8px">
      <div style="display:flex;align-items:center;gap:8px;margin-bottom:6px">
        <span style="font-size:15px">${env.icon}</span>
        <span style="font-size:.82rem;font-weight:600;color:${env.color}">${env.name}</span>
        <span style="font-family:var(--mono);font-size:.6rem;color:var(--muted);margin-left:auto">v${env.version}</span>
        <code style="font-family:var(--mono);font-size:.6rem;background:var(--bg);padding:1px 6px;border-radius:4px;color:var(--muted)">${env.npmPackage}</code>
      </div>
      <div style="font-size:.68rem;color:var(--t2);margin-bottom:7px;line-height:1.5">${env.description}</div>
      <div style="display:flex;flex-wrap:wrap;gap:5px">
        ${env.useCases.map(u => `<span style="font-size:.6rem;padding:2px 7px;border-radius:3px;background:rgba(${env.color === '#ffd43b' ? '255,212,59' : env.color === '#e86e4d' ? '232,110,77' : env.color === '#3572a5' ? '53,114,165' : '245,166,35'},.1);color:${env.color};border:1px solid rgba(${env.color === '#ffd43b' ? '255,212,59' : env.color === '#e86e4d' ? '232,110,77' : env.color === '#3572a5' ? '53,114,165' : '245,166,35'},.25)">${u}</span>`).join('')}
      </div>
    </div>`).join('')}
  </div>`
}

// ── Fallback client-side ML analysis (cuando backend no disponible)
export function clientMLAnalysis(f: CodeFile): MLAnalysisResult {
  const content  = f.content
  const libNames: Record<string, string> = {
    numpy:'NumPy', pandas:'Pandas', polars:'Polars', sklearn:'Scikit-learn',
    torch:'PyTorch', tensorflow:'TensorFlow', keras:'Keras', lightgbm:'LightGBM',
    spacy:'spaCy', matplotlib:'Matplotlib', seaborn:'Seaborn', plotly:'Plotly',
    cv2:'OpenCV', scipy:'SciPy', icecream:'IceCream', xgboost:'XGBoost',
    cython:'Cython', pyximport:'Cython',
  }
  const catMap: Record<string, string> = {
    numpy:'datos', pandas:'datos', polars:'dados', sklearn:'ml',
    lightgbm:'ml', xgboost:'ml', torch:'dl', tensorflow:'dl',
    keras:'dl', spacy:'nlp', matplotlib:'viz', seaborn:'viz',
    plotly:'viz', cv2:'vision', scipy:'ciencia', icecream:'debug',
    cython:'rendimiento', pyximport:'rendimiento',
  }
  const libraries = Object.entries(libNames)
    .filter(([k]) => new RegExp(`\\b${k}\\b`).test(content))
    .map(([k, name]) => ({
      name, category: catMap[k] ?? 'otros', color: catMap[k] === 'rendimiento' ? '#ffd43b' : '#4a5880',
      import: k, alias: null, version: null,
    }))

  // Detectar uso de entornos de notebook en el archivo
  const notebookUsed = detectNotebookUsage(content)
  const nbSuggestions = notebookUsed.length
    ? notebookUsed.map(id => {
        const env = NOTEBOOK_ENVS.find(e => e.id === id)!
        return `${env.icon} ${env.name} detectado — úsalo con: npm install ${env.npmPackage}`
      })
    : []

  return {
    filename: f.name, ts: new Date().toLocaleTimeString(),
    libraries, pipeline: [], models: [], metrics: {}, issues: [],
    diagram: '', score: 50,
    suggestions: ['Ejecuta con backend para análisis completo', ...nbSuggestions],
  }
}