// ══════════════════════════════════════════
//  CodeWatch PRO — Mermaid Diagrams
// ══════════════════════════════════════════
import mermaid from 'mermaid'

let initialized = false

export function initMermaid(): void {
  mermaid.initialize({
    startOnLoad: false,
    theme: 'dark',
    darkMode: true,
    themeVariables: {
      primaryColor:       '#1a2040',
      primaryTextColor:   '#c8d4f0',
      primaryBorderColor: '#2d3768',
      lineColor:          '#4a5880',
      secondaryColor:     '#0e1225',
      background:         '#060810',
      mainBkg:            '#0e1225',
      fontSize:           '13px',
    },
    flowchart:  { curve: 'basis', padding: 18 },
    sequence:   { actorMargin: 70 },
  })
  initialized = true
}

export async function renderDiagram(code: string): Promise<string> {
  if (!initialized) initMermaid()
  const id     = 'mermaid-' + Date.now()
  const result = await mermaid.render(id, code)
  return result.svg
}