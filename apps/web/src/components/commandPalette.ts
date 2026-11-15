// ══════════════════════════════════════════
//  Paleta de comandos — Ctrl/⌘+K
//  ══════════════════════════════════════════
//
//  **Lo que separa una herramienta enterprise de una de click-click no es el
//  aspecto: es que se pueda operar sin el mouse.** Sythrall tiene 11 paneles,
//  N proyectos y N archivos cargados; llegar a cualquiera de esos era
//  siempre "mirá la barra lateral, encontrá el item, hacé click". Eso escala
//  mal con el uso diario: el que abre la app ocho horas por día quiere
//  escribir tres letras y apretar Enter.
//
//  La paleta indexa TRES cosas, que es lo que la hace útil de verdad y no un
//  menú con otra forma:
//
//    - **navegación** — los 11 paneles
//    - **acciones**   — analizar, tema, terminal, ZIP
//    - **entidades**  — los archivos cargados y los proyectos guardados
//
//  Sin la tercera sería un lanzador de comandos; con ella se puede saltar a
//  `intervals.rs` escribiendo `inte`. Ese es el salto.
//
//  **Coincidencia por subsecuencia, no por substring.** `dsh` encuentra
//  "Dashboard" y `cmplx` encuentra "Complexity": es lo que hace que escribir
//  a medias funcione, y lo que uno espera después de usar cualquier paleta
//  moderna. Un `includes()` obligaría a escribir el prefijo exacto.

import { api } from '../api/client'
import { loadProject } from '../panels/upload'
import { state } from '../store/state'
import type { TabId } from '../types'
import { esc } from '../utils/helpers'
import { toggleTheme } from '../utils/theme'
import { selectFile, switchTab } from './app'

interface Comando {
  id: string
  grupo: 'Ir a' | 'Acciones' | 'Archivos' | 'Proyectos'
  etiqueta: string
  pista?: string
  ejecutar: () => void
}

/** Los 11 paneles, con el nombre que muestra la barra lateral. */
const PANELES: Array<[TabId, string, string]> = [
  ['dashboard', 'Dashboard', 'Resumen del proyecto'],
  ['upload', 'Proyectos', 'Abrir o crear un proyecto'],
  ['diff', 'Diff', 'Comparar versiones'],
  ['static', 'Static', 'Big-O, smells y seguridad'],
  ['metrics', 'Métricas', 'Métricas en vivo'],
  ['diagram', 'Arquitectura', 'Diagrama de dependencias'],
  ['issues', 'Hallazgos', 'Todos los hallazgos'],
  ['apis', 'APIs', 'Chequeo de endpoints'],
  ['logs', 'Logs', 'Log unificado'],
  ['ml', 'ML/DL', 'Librerías y modelos detectados'],
  ['editor', 'Editor', 'Ver el código'],
]

/**
 * `true` si cada caracter de `consulta` aparece en `texto` EN ORDEN, aunque
 * no sea consecutivo. Devuelve además una puntuación: cuanto más juntos los
 * caracteres, mejor, así que `dash` prefiere "Dashboard" sobre "Diagrama de
 * ArquitecturaS".
 */
function coincide(texto: string, consulta: string): number | null {
  if (!consulta) return 0
  const t = texto.toLowerCase()
  const q = consulta.toLowerCase()
  let i = 0
  let puntaje = 0
  let anterior = -1
  for (const c of q) {
    const j = t.indexOf(c, i)
    if (j === -1) return null
    // Caracteres contiguos valen más que caracteres dispersos.
    puntaje += anterior === j - 1 ? 3 : 1
    // Y empezar al principio de una palabra vale más que en el medio.
    if (j === 0 || t[j - 1] === ' ' || t[j - 1] === '.' || t[j - 1] === '_') puntaje += 2
    anterior = j
    i = j + 1
  }
  return puntaje
}

let _abierta = false
let _activo = 0
let _filtrados: Comando[] = []
let _proyectos: Array<{ id: string; name: string }> = []

function comandos(): Comando[] {
  const cerrar = (fn: () => void) => () => {
    cerrarPaleta()
    fn()
  }
  const out: Comando[] = PANELES.map(([id, etiqueta, pista]) => ({
    id: `nav-${id}`,
    grupo: 'Ir a' as const,
    etiqueta,
    pista,
    ejecutar: cerrar(() => switchTab(id)),
  }))

  out.push(
    {
      id: 'a-analizar',
      grupo: 'Acciones',
      etiqueta: 'Analizar',
      pista: 'Corre el análisis del proyecto activo',
      ejecutar: cerrar(() => document.getElementById('run-btn')?.click()),
    },
    {
      id: 'a-tema',
      grupo: 'Acciones',
      etiqueta: 'Cambiar tema (claro / oscuro)',
      ejecutar: cerrar(() => toggleTheme()),
    },
    {
      id: 'a-terminal',
      grupo: 'Acciones',
      etiqueta: 'Abrir terminal',
      ejecutar: cerrar(() => document.getElementById('terminal-toggle-btn')?.click()),
    },
    {
      id: 'a-zip',
      grupo: 'Acciones',
      etiqueta: 'Exportar ZIP',
      ejecutar: cerrar(() => document.getElementById('btn-export')?.click()),
    },
  )

  // **Las entidades son lo que hace útil a la paleta.** Un archivo cargado se
  // abre escribiendo parte de su nombre, sin buscarlo en el árbol.
  for (const f of state.files) {
    out.push({
      id: `f-${f.id}`,
      grupo: 'Archivos',
      etiqueta: f.name,
      pista: f.analyzed ? `${f.issues.length} hallazgo(s)` : 'sin analizar',
      ejecutar: cerrar(() => {
        selectFile(f.id)
        switchTab('editor')
      }),
    })
  }

  for (const p of _proyectos) {
    out.push({
      id: `p-${p.id}`,
      grupo: 'Proyectos',
      etiqueta: p.name,
      pista: p.id === state.activeProjectId ? 'activo' : 'abrir',
      // **Abre el proyecto de verdad**, no solo cambia de panel: `loadProject`
      // es exactamente lo que corre al hacer click en su tarjeta.
      ejecutar: cerrar(() => {
        switchTab('upload')
        void loadProject(p.id)
      }),
    })
  }

  return out
}

function filtrar(q: string): Comando[] {
  const todos = comandos()
  if (!q.trim()) return todos.slice(0, 40)
  const conPuntaje: Array<[Comando, number]> = []
  for (const c of todos) {
    const p = coincide(`${c.etiqueta} ${c.pista ?? ''}`, q.trim())
    if (p !== null) conPuntaje.push([c, p])
  }
  conPuntaje.sort((a, b) => b[1] - a[1])
  return conPuntaje.slice(0, 40).map(([c]) => c)
}

function pintar(): void {
  const lista = document.getElementById('cp-list')
  if (!lista) return
  if (!_filtrados.length) {
    lista.innerHTML = `<div class="cp-empty">Sin resultados</div>`
    return
  }
  let grupoActual = ''
  const filas: string[] = []
  _filtrados.forEach((c, i) => {
    if (c.grupo !== grupoActual) {
      grupoActual = c.grupo
      filas.push(`<div class="cp-group">${esc(c.grupo)}</div>`)
    }
    filas.push(
      `<div class="cp-item${i === _activo ? ' active' : ''}" data-i="${i}" role="option" aria-selected="${i === _activo}">
        <span class="cp-label">${esc(c.etiqueta)}</span>
        ${c.pista ? `<span class="cp-hint">${esc(c.pista)}</span>` : ''}
      </div>`,
    )
  })
  lista.innerHTML = filas.join('')
  lista.querySelector('.cp-item.active')?.scrollIntoView({ block: 'nearest' })
}

function mover(paso: number): void {
  if (!_filtrados.length) return
  // Envuelve en los dos extremos: llegar al final y seguir bajando vuelve al
  // principio, que es lo que hace toda paleta y lo que la mano espera.
  _activo = (_activo + paso + _filtrados.length) % _filtrados.length
  pintar()
}

function abrirPaleta(): void {
  const ov = document.getElementById('cp-overlay')
  const inp = document.getElementById('cp-input') as HTMLInputElement | null
  if (!ov || !inp) return
  _abierta = true
  ov.classList.add('open')
  inp.value = ''
  _activo = 0
  _filtrados = filtrar('')
  pintar()
  // El foco va en el próximo cuadro: si se pide en el mismo en que se
  // muestra el overlay, el navegador todavía lo tiene en `display:none`.
  requestAnimationFrame(() => inp.focus())

  // Los proyectos se piden al abrir y se cachean: no vale la pena una
  // petición por cada tecla, y la lista cambia poco.
  if (!_proyectos.length) {
    void api
      .listProjects()
      .then((r) => {
        // `project_name` es opcional: los proyectos subidos antes de que se
        // persistiera el nombre solo tienen id, y mostrar el id es mejor que
        // mostrar una fila en blanco.
        _proyectos = (r.projects ?? []).map((p) => ({
          id: p.project_id,
          name: p.project_name || p.project_id,
        }))
        if (_abierta) {
          _filtrados = filtrar(inp.value)
          pintar()
        }
      })
      .catch(() => {
        /* sin proyectos guardados, la paleta sigue sirviendo para el resto */
      })
  }
}

function cerrarPaleta(): void {
  _abierta = false
  document.getElementById('cp-overlay')?.classList.remove('open')
}

export function initCommandPalette(): void {
  const ov = document.createElement('div')
  ov.id = 'cp-overlay'
  ov.innerHTML = `
    <div class="cp-panel" role="dialog" aria-modal="true" aria-label="Paleta de comandos">
      <input id="cp-input" class="cp-input" type="text" role="combobox" aria-expanded="true"
             aria-controls="cp-list" autocomplete="off" spellcheck="false"
             placeholder="Ir a un panel, correr una acción, abrir un archivo…" />
      <div id="cp-list" class="cp-list" role="listbox"></div>
      <div class="cp-foot">
        <span><kbd>↑</kbd><kbd>↓</kbd> moverse</span>
        <span><kbd>Enter</kbd> abrir</span>
        <span><kbd>Esc</kbd> cerrar</span>
      </div>
    </div>`
  document.body.appendChild(ov)

  // Click afuera cierra; click adentro no.
  ov.addEventListener('mousedown', (e) => {
    if (e.target === ov) cerrarPaleta()
  })
  ov.addEventListener('click', (e) => {
    const fila = (e.target as HTMLElement).closest<HTMLElement>('.cp-item')
    if (fila) _filtrados[Number(fila.dataset['i'])]?.ejecutar()
  })

  const inp = document.getElementById('cp-input') as HTMLInputElement
  inp.addEventListener('input', () => {
    _activo = 0
    _filtrados = filtrar(inp.value)
    pintar()
  })
  inp.addEventListener('keydown', (e) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      mover(1)
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      mover(-1)
    } else if (e.key === 'Enter') {
      e.preventDefault()
      _filtrados[_activo]?.ejecutar()
    } else if (e.key === 'Escape') {
      e.preventDefault()
      cerrarPaleta()
    }
  })

  document.getElementById('cp-trigger')?.addEventListener('click', () => abrirPaleta())

  document.addEventListener('keydown', (e) => {
    const mod = e.ctrlKey || e.metaKey
    if (mod && e.key.toLowerCase() === 'k') {
      e.preventDefault()
      // **Alterna, no solo abre.** Apretar el mismo atajo dos veces tiene que
      // dejar la pantalla como estaba; si solo abriera, el segundo Ctrl+K no
      // haría nada y se sentiría trabado.
      if (_abierta) cerrarPaleta()
      else abrirPaleta()
    }
  })
}
