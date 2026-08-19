// ══════════════════════════════════════════
//  Sythrall — Icon system
// ══════════════════════════════════════════
// Set curado de iconos SVG inline (stroke="currentColor", hereda color del
// tema activo automáticamente — algo que un emoji no puede hacer) y badges
// de lenguaje por extensión. Reemplaza los mapas de emoji que estaban
// duplicados en upload.ts / explorer.ts / analysis.ts / problems.ts.

export type IconName =
  | 'dashboard'
  | 'editor'
  | 'apis'
  | 'issues'
  | 'diagram'
  | 'ml'
  | 'metrics'
  | 'diff'
  | 'logs'
  | 'projects'
  | 'static'
  | 'terminal'
  | 'sun'
  | 'moon'
  | 'plus'
  | 'download'
  | 'play'
  | 'trash'
  | 'close'
  | 'menu'
  | 'globe'
  | 'clock'
  | 'more'
  | 'search'
  | 'folder'
  | 'warning'
  | 'check'
  | 'shield'

const PATHS: Record<IconName, string> = {
  dashboard:
    '<rect x="3" y="3" width="7" height="9" rx="1.5"/><rect x="14" y="3" width="7" height="5" rx="1.5"/><rect x="14" y="12" width="7" height="9" rx="1.5"/><rect x="3" y="16" width="7" height="5" rx="1.5"/>',
  editor:
    '<path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/><path d="M14 3v6h6"/><path d="M9 13h6M9 17h4"/>',
  apis: '<path d="M12 20h.01M8.5 16.5a5 5 0 0 1 7 0M5 13a10 10 0 0 1 14 0M2 9.5a15 15 0 0 1 20 0"/>',
  issues: '<circle cx="12" cy="12" r="8.5"/><path d="M12 8v5M12 16h.01"/>',
  diagram:
    '<circle cx="6" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="12" r="3"/><path d="M8.6 7.6 15.4 10.4M8.6 16.4 15.4 13.6"/>',
  ml: '<rect x="7" y="7" width="10" height="10" rx="2"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2 2M17.1 17.1l2 2M4.9 19.1l2-2M17.1 6.9l2-2"/>',
  metrics: '<path d="M3 3v18h18"/><path d="M7 15l4-5 3 3 5-7"/>',
  diff: '<circle cx="9" cy="12" r="6"/><circle cx="15" cy="12" r="6"/>',
  logs: '<path d="M4 6h16M4 12h16M4 18h10"/>',
  projects: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
  static: '<circle cx="10" cy="10" r="6"/><path d="M14.5 14.5 20 20"/>',
  terminal: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M7 9l3 3-3 3M13 15h4"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.2 4.2l1.4 1.4M18.4 18.4l1.4 1.4M2 12h2M20 12h2M4.2 19.8l1.4-1.4M18.4 5.6l1.4-1.4"/>',
  moon: '<path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  download: '<path d="M12 3v12M7 10l5 5 5-5M4 19h16"/>',
  play: '<path d="M7 4l13 8-13 8z"/>',
  trash: '<path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13"/>',
  close: '<path d="M6 6l12 12M18 6 6 18"/>',
  menu: '<path d="M4 6h16M4 12h16M4 18h16"/>',
  globe: '<circle cx="12" cy="12" r="9"/><path d="M3 9h18M3 15h18M12 3c-3 3-3 15 0 18M12 3c3 3 3 15 0 18"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 3"/>',
  more: '<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="M21 21l-4.3-4.3"/>',
  folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
  warning:
    '<path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0Z"/><path d="M12 9v4"/><path d="M12 17h.01"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
  shield: '<path d="M12 2 4 5v6c0 5 3.4 8.4 8 11 4.6-2.6 8-6 8-11V5z"/>',
}

/** Devuelve un `<svg>` inline. `stroke="currentColor"` — hereda color de donde se use. */
export function icon(name: IconName, size = 16): string {
  return `<svg class="icon" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${PATHS[name]}</svg>`
}

// ─── Badge de lenguaje por extensión ──────────────────────────────────────────
// Reemplaza los emoji por extensión (🐍 Python, 🟦 TS, 🦀 Rust...) por un
// badge de color + abreviatura — evita reproducir logos de marca y da el
// mismo valor de "reconocible al toque".

interface LangDef {
  label: string
  cls: string
}

const LANG: Record<string, LangDef> = {
  '.py': { label: 'PY', cls: 'lang-py' },
  '.ts': { label: 'TS', cls: 'lang-ts' },
  '.tsx': { label: 'TSX', cls: 'lang-ts' },
  '.js': { label: 'JS', cls: 'lang-js' },
  '.jsx': { label: 'JSX', cls: 'lang-js' },
  '.vue': { label: 'VUE', cls: 'lang-js' },
  '.svelte': { label: 'SV', cls: 'lang-js' },
  '.html': { label: 'HTML', cls: 'lang-html' },
  '.css': { label: 'CSS', cls: 'lang-css' },
  '.scss': { label: 'SCSS', cls: 'lang-css' },
  '.json': { label: 'JSON', cls: 'lang-data' },
  '.yaml': { label: 'YML', cls: 'lang-data' },
  '.yml': { label: 'YML', cls: 'lang-data' },
  '.toml': { label: 'TOML', cls: 'lang-data' },
  '.env': { label: 'ENV', cls: 'lang-data' },
  '.sql': { label: 'SQL', cls: 'lang-data' },
  '.md': { label: 'MD', cls: 'lang-doc' },
  '.txt': { label: 'TXT', cls: 'lang-doc' },
  '.log': { label: 'LOG', cls: 'lang-doc' },
  '.png': { label: 'IMG', cls: 'lang-doc' },
  '.jpg': { label: 'IMG', cls: 'lang-doc' },
  '.svg': { label: 'IMG', cls: 'lang-doc' },
  '.zip': { label: 'ZIP', cls: 'lang-doc' },
  '.pdf': { label: 'PDF', cls: 'lang-doc' },
  '.go': { label: 'GO', cls: 'lang-go' },
  '.rs': { label: 'RS', cls: 'lang-rust' },
  '.java': { label: 'JAVA', cls: 'lang-java' },
  '.cpp': { label: 'C++', cls: 'lang-cpp' },
  '.c': { label: 'C', cls: 'lang-cpp' },
  '.sh': { label: 'SH', cls: 'lang-shell' },
  '.rb': { label: 'RB', cls: 'lang-ruby' },
  '.php': { label: 'PHP', cls: 'lang-php' },
}

export function languageBadge(ext: string): string {
  const def = LANG[ext] ?? { label: '•', cls: 'lang-generic' }
  return `<span class="lang-badge ${def.cls}">${def.label}</span>`
}

/** Igual que languageBadge() pero por nombre de lenguaje (`r.language` del
 * parser: 'python'/'c'/'cpp'/'javascript'/'typescript') en vez de extensión —
 * usado por static.ts/graph.ts, que reciben el nombre, no el archivo. */
const LANG_NAME_TO_EXT: Record<string, string> = {
  python: '.py',
  c: '.c',
  cpp: '.cpp',
  javascript: '.js',
  typescript: '.ts',
}

export function languageBadgeByName(lang: string): string {
  return languageBadge(LANG_NAME_TO_EXT[lang] ?? '')
}
