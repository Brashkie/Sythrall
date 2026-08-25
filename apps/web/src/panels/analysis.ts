// ══════════════════════════════════════════
//  Sythrall — Analysis Panels
// ══════════════════════════════════════════
// panels/analysis.ts
import { currentIssueSource } from '../panels/apis'
import { state } from '../store/state'
import type { CodeFile, HalsteadMetrics } from '../types'
import { languageBadge } from '../utils/icons'
import { renderProjectContextBanner, wireProjectContextBanner } from '../utils/projectHeader'

// ── Helper
const mr = (k: string, v: unknown, color?: string) =>
  `<div class="metric-row"><span class="mr-k">${k}</span><span class="mr-v"${color ? ` style="color:${color}"` : ''}>${v ?? '—'}</span></div>`

// Fase 22: desglose de las métricas de Halstead que ya alimentan el MI de
// arriba — antes solo se usaba el Volumen, cocinado dentro de la fórmula de
// Coleman-Oman; esto muestra los 5 componentes por separado.
function _renderHalstead(h: HalsteadMetrics): string {
  return `<details style="margin-top:8px">
    <summary style="font-size:.6rem;color:var(--muted);cursor:pointer;user-select:none">Halstead — desglose</summary>
    <div style="margin-top:6px;display:grid;grid-template-columns:1fr 1fr;gap:4px 10px;font-family:var(--mono);font-size:.65rem">
      <span style="color:var(--muted)">Vocabulario (η)</span><span>${h.vocabulary}</span>
      <span style="color:var(--muted)">Longitud (N)</span><span>${h.length}</span>
      <span style="color:var(--muted)">Volumen (V)</span><span>${h.volume.toFixed(1)}</span>
      <span style="color:var(--muted)">Dificultad (D)</span><span>${h.difficulty.toFixed(1)}</span>
      <span style="color:var(--muted)">Esfuerzo (E)</span><span>${h.effort.toFixed(0)}</span>
    </div>
  </details>`
}

// ══ File Analysis (Right panel)
export function renderFileAnalysis(f: CodeFile): void {
  const el = document.getElementById('analysis-content')
  if (!el) return
  const m = f.metrics
  const cx = m.complexity ?? []
  const mi = m.mi
  const raw = m.raw
  const miColor = mi == null ? 'var(--muted)' : mi > 70 ? 'var(--ok)' : mi > 40 ? 'var(--warn)' : 'var(--err)'

  el.innerHTML = `
    <div style="font-size:.58rem;font-weight:700;letter-spacing:1.4px;text-transform:uppercase;color:var(--muted);margin-bottom:10px">ANÁLISIS: ${f.name}</div>
    ${
      m.pylint_score != null
        ? `<div style="background:var(--s2);border-radius:8px;padding:10px;margin-bottom:10px;text-align:center">
      <div style="font-size:.58rem;color:var(--muted);margin-bottom:3px">PYLINT SCORE</div>
      <div style="font-family:var(--mono);font-size:1.8rem;color:${m.pylint_score >= 8 ? 'var(--ok)' : m.pylint_score >= 5 ? 'var(--warn)' : 'var(--err)'}">${m.pylint_score.toFixed(1)}<span style="font-size:.85rem;color:var(--muted)">/10</span></div>
    </div>`
        : ''
    }
    ${
      mi != null
        ? `<div style="background:var(--s2);border-radius:8px;padding:10px;margin-bottom:10px">
      <div style="font-size:.58rem;color:var(--muted);margin-bottom:2px">MAINTAINABILITY INDEX</div>
      <div style="font-family:var(--mono);font-size:1.4rem;color:${miColor}">${mi}<span style="font-size:.7rem;color:var(--muted)">/100</span></div>
      <div class="complexity-bar" style="margin-top:5px"><div class="cb-fill" style="width:${mi}%;background:${miColor}"></div></div>
      ${m.halstead ? _renderHalstead(m.halstead) : ''}
    </div>`
        : ''
    }
    ${
      raw
        ? `<div class="metric-section">
      <div class="ms-title">Estadísticas</div>
      ${mr('Líneas totales', raw.loc)} ${mr('Líneas código', raw.sloc)} ${mr('Comentarios', raw.comments)} ${mr('Blancos', raw.blank)}
    </div>`
        : ''
    }
    <div class="metric-section">
      <div class="ms-title">Herramientas</div>
      <div style="display:flex;gap:5px;flex-wrap:wrap">${(m.tools_used ?? ['cliente']).map((t) => `<span class="ii-tool t-${t}">${t}</span>`).join('')}</div>
    </div>
    ${
      cx.length
        ? `<div class="metric-section">
      <div class="ms-title">Complejidad por función</div>
      ${cx
        .map(
          (
            fn,
          ) => `<div style="display:flex;align-items:center;justify-content:space-between;padding:5px 8px;border-radius:5px;margin-bottom:4px;background:var(--s2)">
        <span style="font-family:var(--mono);font-size:.68rem">${fn.name}<span style="color:var(--muted)">:${fn.line}</span></span>
        <span class="rank-${fn.rank}" style="font-family:var(--mono);font-size:.72rem;font-weight:700">CC=${fn.complexity} (${fn.rank})</span>
      </div>`,
        )
        .join('')}
    </div>`
        : ''
    }
    <div class="metric-section">
      <div class="ms-title">Problemas (${f.issues.length})</div>
      ${
        f.issues.length === 0
          ? '<div class="empty" style="padding:8px;color:var(--ok)">Sin problemas</div>'
          : f.issues
              .slice(0, 15)
              .map(
                (i) => `<div class="issue-item ii-${i.severity}" style="margin-bottom:5px">
          <div class="ii-head"><span class="ii-sev sev-${i.severity}">${i.severity.toUpperCase()}</span><span class="ii-tool t-${i.tool}">${i.tool}</span></div>
          <div class="ii-msg">${i.message ?? ''}</div>
          ${i.line ? `<div style="font-family:var(--mono);font-size:.6rem;color:var(--muted)">Línea ${i.line}</div>` : ''}
        </div>`,
              )
              .join('')
      }
    </div>
    <div style="margin-top:10px;display:flex;gap:6px;flex-wrap:wrap">
      <button class="btn btn-ghost btn-sm" style="flex:1;justify-content:center" data-action="diagram" data-fid="${f.id}">Diagrama</button>
      ${f.ext === '.py' ? `<button class="btn btn-ghost btn-sm" style="flex:1;justify-content:center" data-action="ml" data-fid="${f.id}">ML/DL</button>` : ''}
    </div>`
}

// ══ Metrics panel
export function renderMetrics(): void {
  const el = document.getElementById('metrics-content')
  if (!el) return
  if (!state.files.length) {
    // Sin archivos cargados a mano: si hay proyecto activo y ya se corrió un
    // análisis de proyecto (el mismo StaticProjectResult que Static/Dashboard
    // ya pidieron — no se vuelve a pedir acá), se muestra un resumen usando
    // los mismos hallazgos ricos que "Hallazgos" ya convierte (no la lista
    // de lint de state.results.issues, que solo se llena analizando archivo
    // por archivo y queda vacía aunque el proyecto ya esté analizado — bug
    // real encontrado al verificar este panel con un proyecto recién
    // analizado: mostraba "Corré Análisis completo" con datos ya listos).
    if (state.activeProjectId && state.results.projectDashboard) {
      const issues = currentIssueSource()
      const errors = issues.filter((i) => i.severity === 'error').length
      const warnings = issues.filter((i) => i.severity === 'warning').length
      el.innerHTML = `
        ${renderProjectContextBanner()}
        <div class="metric-section">
          <div class="ms-title">Proyecto activo</div>
          ${mr('Total hallazgos', issues.length, issues.length > 20 ? 'var(--err)' : issues.length > 5 ? 'var(--warn)' : 'var(--ok)')}
          ${mr('Errores', errors, 'var(--err)')}
          ${mr('Warnings', warnings, 'var(--warn)')}
        </div>`
      wireProjectContextBanner(el)
      return
    }
    el.innerHTML = `<div class="empty">${state.activeProjectId ? 'Corré "Análisis completo" para ver métricas' : 'Sube archivos, o elegí un proyecto activo en Proyectos'}</div>`
    return
  }
  const analyzed = state.files.filter((f) => f.analyzed)
  const scores = analyzed.filter((f) => f.metrics.pylint_score != null).map((f) => f.metrics.pylint_score!)
  const avg = scores.length ? scores.reduce((a, b) => a + b, 0) / scores.length : null

  el.innerHTML = `
    <div class="metric-section">
      <div class="ms-title">Proyecto</div>
      ${mr('Archivos', `${analyzed.length}/${state.files.length}`)}
      ${mr('Total issues', state.results.issues.length, state.results.issues.length > 20 ? 'var(--err)' : state.results.issues.length > 5 ? 'var(--warn)' : 'var(--ok)')}
      ${mr('Errores', state.results.issues.filter((i) => i.severity === 'error').length, 'var(--err)')}
      ${mr('Warnings', state.results.issues.filter((i) => i.severity === 'warning').length, 'var(--warn)')}
      ${avg != null ? mr('Pylint (prom.)', avg.toFixed(1) + '/10', avg >= 8 ? 'var(--ok)' : avg >= 5 ? 'var(--warn)' : 'var(--err)') : ''}
    </div>
    ${analyzed
      .map(
        (f) => `
    <div class="metric-section" style="cursor:pointer" data-select-file="${f.id}">
      <div class="ms-title" style="display:flex;align-items:center;gap:6px;text-transform:none">
        <span>${languageBadge(f.ext)}</span>${f.name}
        <span class="tag ${f.issues.length ? 'tg-err' : 'tg-ok'}" style="margin-left:auto">${f.issues.length || 'OK'}</span>
      </div>
      ${f.metrics.pylint_score != null ? mr('Pylint', f.metrics.pylint_score.toFixed(1) + '/10', f.metrics.pylint_score >= 8 ? 'var(--ok)' : f.metrics.pylint_score >= 5 ? 'var(--warn)' : 'var(--err)') : ''}
      ${f.metrics.mi != null ? mr('Maintainability', f.metrics.mi + '/100', f.metrics.mi > 70 ? 'var(--ok)' : f.metrics.mi > 40 ? 'var(--warn)' : 'var(--err)') : ''}
      ${f.metrics.raw?.sloc ? mr('SLOC', f.metrics.raw.sloc) : ''}
    </div>`,
      )
      .join('')}`
}
