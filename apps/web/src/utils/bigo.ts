// ─────────────────────────────────────────────────────────────────────────────
//  Big-O en pantalla: el color de cada clase, su orden de peor a mejor, y la
//  distribucion del proyecto. `panels/dashboard.ts` ya consumia el render
//  desde `panels/static.ts`; el color y el orden lo acompanan porque son la
//  misma decision visual, no tres.
// ─────────────────────────────────────────────────────────────────────────────

import { esc } from './helpers'

export const BIG_O_COLOR: Record<string, string> = {
  'O(1)': 'var(--ok)',
  'O(log n)': '#8ef5c0',
  'O(n)': 'var(--info)',
  'O(n log n)': 'var(--warn)',
  'O(n²)': 'var(--orange)',
  'O(n³)': 'var(--err)',
  'O(2^n)': 'var(--err)',
}

const BIG_O_ORDER = ['O(1)', 'O(log n)', 'O(n)', 'O(n log n)', 'O(n²)', 'O(n³)', 'O(2^n)']

export function renderBigODistribution(
  dist: Record<string, number>,
  title = 'Distribución Big O del proyecto',
): string {
  const entries = Object.entries(dist)
  if (!entries.length) return ''
  const max = Math.max(...Object.values(dist))
  const rows = entries
    .sort((a, b) => BIG_O_ORDER.indexOf(a[0]) - BIG_O_ORDER.indexOf(b[0]))
    .map(([bigo, count]) => {
      const color = BIG_O_COLOR[bigo] ?? 'var(--muted)'
      const pct = Math.round((count / max) * 100)
      return `<div class="bigo-dist-row">
        <span class="bigo-badge" style="color:${color};border-color:${color};min-width:80px">${esc(bigo)}</span>
        <div class="bigo-dist-bar-wrap">
          <div class="bigo-dist-bar" style="width:${pct}%;background:${color}"></div>
        </div>
        <span style="font-family:var(--mono);font-size:.65rem;color:var(--muted)">${count} fn${count > 1 ? 's' : ''}</span>
      </div>`
    })
    .join('')
  return `
  <div class="metric-section">
    <div class="ms-title">${esc(title)}</div>
    <div style="padding:4px 0">${rows}</div>
  </div>`
}
