// ══════════════════════════════════════════
//  Explorador de código máquina (Fase 19)
// ══════════════════════════════════════════
//
// **Lo que el motor ya sabía hacer y nadie podía ver.** `compilar.rs` compila
// el fuente a ensamblador y `asmparse.rs` lo analiza desde hace fases, pero
// hasta la Fase 19 solo se llegaba por API. Un análisis que no se ve no
// existe, igual que un atajo de teclado que nadie anuncia.
//
// La vista es deliberadamente una COMPARACIÓN y no un volcado: un número de
// instrucciones suelto no dice nada, y "19 contra 13, con 9 movimientos de
// datos contra 3" dice exactamente qué hizo el optimizador.
//
// **Por qué es un módulo y no una sección de `panels/static.ts`.** Vivía ahí,
// bajo un banner que decía "Analizador de ejecutables" y que no era: medido,
// ese banner cubría 1.221 líneas con 63 nombres cruzando su borde, o sea que
// no era una unidad de nada. Este bloque sí lo es — 5 variables de estado
// propias y nada más que un tipo compartido — y sacarlo también se llevó sus
// 6 ramas de dispatch, que estaban 2.000 líneas más arriba, y su función de
// compilación, que estaba 200 más abajo dentro de OTRA sección.
//
// El módulo no toca el DOM: devuelve HTML y pide que lo repinten. Quién
// repinta y de dónde sale el fuente los decide el panel, porque son suyos.

import { api, type CompilacionAsm, type CompileAsmResponse, type SimdInfo } from '../api/client'
import { esc, toast } from '../utils/helpers'

/** Lo único que esta vista necesita de un resultado de análisis.
 *
 * **No importa `StaticResult` a propósito**: ese tipo no se exporta de
 * `panels/static.ts`, así que traerlo obligaría a exportarlo y crearía una
 * dependencia circular entre el panel y su propio módulo. De sus ~40
 * propiedades acá se usan dos, y declararlas deja escrito cuál es la
 * dependencia real en vez de esconderla detrás de un tipo gigante. */
export interface ResultadoAnalizado {
  filename: string
  language: string
}

/** Lo que el módulo necesita del panel para hacer su trabajo.
 *
 * Son funciones y no valores porque el panel las resuelve en el momento del
 * click, no cuando se registra el handler: el resultado que se está viendo
 * cambia con cada análisis. */
export interface ContextoDeMaquina {
  /** El resultado que se está viendo, o `null` si todavía no hay ninguno. */
  resultado: () => ResultadoAnalizado | null
  /** El contenido que produjo ese resultado. */
  fuente: () => string
  /** Repinta el resultado sin rehacer el panel entero. */
  repintar: () => void
}

/** Lenguajes que el puente sabe compilar, con la extensión que los delata. */
const LENGUAJES_COMPILABLES: Record<string, string> = {
  c: 'C',
  cpp: 'C++',
  rust: 'Rust',
  fortran: 'Fortran',
  zig: 'Zig',
  ada: 'Ada',
}

/** De la extensión del archivo al id de lenguaje del motor. */
const EXT_A_LENGUAJE: Record<string, string> = {
  '.c': 'c',
  '.h': 'c',
  '.cpp': 'cpp',
  '.cc': 'cpp',
  '.cxx': 'cpp',
  '.hpp': 'cpp',
  '.rs': 'rust',
  '.f90': 'fortran',
  '.f95': 'fortran',
  '.f': 'fortran',
  '.zig': 'zig',
  // Solo `.adb`, no `.ads`: una especificacion no tiene cuerpo, asi que
  // compilarla emite poco y nada que mirar. El cuerpo es lo que tiene
  // instrucciones.
  '.adb': 'ada',
}

/** Extensiones de CPU que el motor acepta. Es la misma tabla cerrada de
 * `compilar.rs`: lo que no está acá no llega al compilador. */
const FEATURES_CPU = ['sse2', 'sse4.2', 'avx', 'avx2', 'avx512f', 'fma'] as const

const NIVELES_OPT = ['0', '1', '2', '3', 's'] as const

// El estado de la vista. Son cinco variables y las cinco son de acá: los
// chips marcados, el resultado de la última compilación, si hay una en vuelo,
// y qué nivel tiene el texto del ensamblador abierto.
let _asmCompilando = false
let _asmResultado: CompileAsmResponse | null = null
const _asmNiveles = new Set<string>(['0', '2'])
const _asmFeatures = new Set<string>()
let _asmVerTexto: string | null = null

/** `null` si el archivo no es de un lenguaje que se pueda compilar. */
export function lenguajeCompilable(r: ResultadoAnalizado): string | null {
  const ext = (r.filename.match(/\.[^.]+$/)?.[0] ?? '').toLowerCase()
  const porExt = EXT_A_LENGUAJE[ext]
  if (porExt) return porExt
  // Algunos resultados traen el lenguaje y no una extensión reconocible.
  const lang = (r.language ?? '').toLowerCase()
  return LENGUAJES_COMPILABLES[lang] ? lang : null
}

function _chip(texto: string, activo: boolean, attr: string): string {
  return `<button class="mc-chip${activo ? ' active' : ''}" ${attr}>${esc(texto)}</button>`
}

/** Badge de SIMD. **Separa packed de escalar a propósito**: son la misma
 * familia de instrucciones y significan cosas opuestas sobre si el código
 * está vectorizado. */
function _badgeSimd(s: SimdInfo | null): string {
  if (!s)
    return `<span class="mc-badge mc-badge-off" title="El procedimiento no toca un solo registro vectorial">sin SIMD</span>`
  const estado = s.vectorized
    ? `<span class="mc-badge mc-badge-ok" title="Hay instrucciones packed: una instrucción opera sobre varios elementos">vectorizado</span>`
    : `<span class="mc-badge mc-badge-off" title="Usa registros vectoriales pero solo con instrucciones escalares (addss/mulsd): un elemento por instrucción, sin vectorizar">escalar</span>`
  return `${estado}
    <span class="mc-badge" title="Extensión de CPU más alta de la que hay evidencia">${esc(s.isa.toUpperCase())}</span>
    <span class="mc-badge" title="Ancho del registro vectorial más grande que se toca">${s.max_width_bits} bits</span>
    <span class="mc-badge" title="Instrucciones que operan sobre varios elementos a la vez">${s.packed_instructions} packed</span>
    <span class="mc-badge" title="Instrucciones que usan un registro vectorial para un solo elemento">${s.scalar_simd_instructions} escalar</span>`
}

const CATEGORIAS: Array<[string, string]> = [
  ['data_movement', 'mov. datos'],
  ['arithmetic', 'aritmética'],
  ['logic', 'lógica'],
  ['comparison', 'comparación'],
  ['control_flow', 'control'],
  ['stack', 'pila'],
  ['other', 'otras'],
]

function _filaNivel(c: CompilacionAsm): string {
  if (!c.available) {
    return `<tr><td class="mono">-O${esc(c.opt_level)}</td><td colspan="9" class="text-muted">${esc(c.error ?? 'toolchain no disponible')}</td></tr>`
  }
  if (c.error) {
    return `<tr><td class="mono">-O${esc(c.opt_level)}</td><td colspan="9" class="mc-err">${esc(c.error)}</td></tr>`
  }
  const procs = c.analysis?.procedures ?? []
  const instrucciones = procs.flatMap((p) => p.instructions)
  const porCat: Record<string, number> = {}
  for (const i of instrucciones) porCat[i.category] = (porCat[i.category] ?? 0) + 1
  const simd = procs.map((p) => p.simd).find((x) => x) ?? null
  const celdas = CATEGORIAS.map(([k]) => `<td class="num">${porCat[k] ?? 0}</td>`).join('')
  return `<tr>
    <td class="mono">-O${esc(c.opt_level)}</td>
    <td class="num">${c.assembly_lines}</td>
    <td class="num"><strong>${instrucciones.length}</strong></td>
    ${celdas}
    <td>${_badgeSimd(simd)}</td>
    <td><button class="btn btn-ghost btn-sm" data-mc-ver="${esc(c.opt_level)}">ver asm</button></td>
  </tr>`
}

function _renderAsmTexto(rs: CompilacionAsm[]): string {
  if (!_asmVerTexto) return ''
  const c = rs.find((x) => x.opt_level === _asmVerTexto)
  if (!c?.assembly) return ''
  return `
    <div class="mc-asm">
      <div class="mc-asm-head">
        <span class="mono">ensamblador -O${esc(c.opt_level)}${c.features.length ? ` · ${esc(c.features.join(' '))}` : ''}</span>
        ${c.assembly_truncated ? `<span class="mc-badge mc-badge-warn" title="El análisis de arriba cubre el archivo entero; solo el texto se recorta">recortado de ${c.assembly_lines} líneas</span>` : ''}
        <button class="btn btn-ghost btn-sm ml-auto" data-mc-cerrar>cerrar</button>
      </div>
      <pre class="mc-asm-body">${esc(c.assembly)}</pre>
    </div>`
}

export function renderCodigoMaquina(r: ResultadoAnalizado): string {
  const lang = lenguajeCompilable(r)
  if (!lang) return ''

  const niveles = NIVELES_OPT.map((n) => _chip(`-O${n}`, _asmNiveles.has(n), `data-mc-nivel="${n}"`)).join('')
  const feats = FEATURES_CPU.map((f) => _chip(f, _asmFeatures.has(f), `data-mc-feature="${f}"`)).join('')

  let cuerpo: string
  if (_asmCompilando) {
    cuerpo = `<div class="mc-wait"><span class="up-spinner"></span> Compilando con el toolchain real…</div>`
  } else if (_asmResultado && !_asmResultado.available) {
    cuerpo = `<div class="empty">${esc(_asmResultado.error ?? 'El motor no está disponible.')}</div>`
  } else if (_asmResultado) {
    const rs = _asmResultado.results
    const tool = rs.find((x) => x.toolchain_version)?.toolchain_version ?? ''
    const cabecera = CATEGORIAS.map(([, etiqueta]) => `<th>${esc(etiqueta)}</th>`).join('')
    cuerpo = `
      ${tool ? `<div class="meta mb-2">toolchain: ${esc(tool)}</div>` : ''}
      <div class="mc-table-wrap">
        <table class="mc-table">
          <thead><tr><th>nivel</th><th>líneas asm</th><th>instr.</th>${cabecera}<th>SIMD</th><th></th></tr></thead>
          <tbody>${rs.map(_filaNivel).join('')}</tbody>
        </table>
      </div>
      ${_renderAsmTexto(rs)}`
  } else {
    cuerpo = `<div class="empty">Elegí los niveles y apretá Compilar.</div>`
  }

  return `
  <div class="metric-section">
    <div class="ms-title">
      CÓDIGO MÁQUINA
      <span class="meta">${esc(LENGUAJES_COMPILABLES[lang] ?? lang)}</span>
    </div>
    <div class="mc-explica">
      Compila este archivo con su compilador real y analiza el ensamblador que sale.
      <strong>No ejecuta nada</strong>: se detiene antes de ensamblar y de linkear.
      Comparar dos niveles es el punto — un conteo suelto no dice nada.
    </div>
    <div class="mc-controles">
      <div class="mc-grupo"><span class="mc-label">optimización</span>${niveles}</div>
      <div class="mc-grupo"><span class="mc-label" title="Sin pedir una extensión, el compilador no vectoriza y la pregunta '¿se vectorizó?' no se puede responder">CPU</span>${feats}</div>
      <button class="btn btn-run btn-sm" data-mc-compilar ${_asmCompilando ? 'disabled' : ''}>Compilar</button>
    </div>
    ${cuerpo}
  </div>`
}

async function compilar(r: ResultadoAnalizado, ctx: ContextoDeMaquina): Promise<void> {
  const lang = lenguajeCompilable(r)
  if (!lang) return
  const contenido = ctx.fuente()
  if (!contenido.trim()) {
    toast('No hay contenido que compilar', 'warn')
    return
  }
  if (!_asmNiveles.size) {
    toast('Elegí al menos un nivel de optimización', 'warn')
    return
  }
  _asmCompilando = true
  _asmVerTexto = null
  ctx.repintar()
  try {
    // El orden importa para leer la tabla: -O0 primero.
    const niveles = NIVELES_OPT.filter((n) => _asmNiveles.has(n))
    _asmResultado = await api.compileAsm(lang, contenido, [...niveles], [..._asmFeatures])
  } catch (e) {
    _asmResultado = {
      available: false,
      results: [],
      error: e instanceof Error ? e.message : 'Error al compilar',
    }
  } finally {
    _asmCompilando = false
    ctx.repintar()
  }
}

/** Resuelve el click de los 6 controles de esta vista. `true` si era uno.
 *
 * **Era un bloque de 25 líneas dentro del dispatch del panel**, mezclado
 * entre las ramas de Pareto y de Atomic CSS — la rama de `mc-cerrar` había
 * terminado DESPUÉS de las dos, separada de las otras cinco que le
 * corresponden. Acá están juntas y el panel llama una sola función.
 *
 * **Devuelve en cuanto acierta, y eso no cambia el comportamiento**: los seis
 * atributos `data-mc-*` son mutuamente excluyentes sobre un mismo elemento,
 * así que el `if` encadenado de antes nunca podía acertar dos veces.
 *
 * **Los chips alternan y NO recompilan solos**: una compilación de verdad
 * tarda, así que elegir filtros tiene que ser gratis y el gasto tiene que
 * pedirlo el usuario. */
export function manejarClickDeMaquina(t: HTMLElement, ctx: ContextoDeMaquina): boolean {
  const nivel = t.dataset['mcNivel']
  if (nivel !== undefined) {
    if (_asmNiveles.has(nivel)) _asmNiveles.delete(nivel)
    else _asmNiveles.add(nivel)
    ctx.repintar()
    return true
  }

  const feat = t.dataset['mcFeature']
  if (feat !== undefined) {
    if (_asmFeatures.has(feat)) _asmFeatures.delete(feat)
    else _asmFeatures.add(feat)
    ctx.repintar()
    return true
  }

  if (t.dataset['mcCompilar'] !== undefined) {
    const r = ctx.resultado()
    if (r) void compilar(r, ctx)
    return true
  }

  const ver = t.dataset['mcVer']
  if (ver !== undefined) {
    _asmVerTexto = _asmVerTexto === ver ? null : ver
    ctx.repintar()
    return true
  }

  if (t.dataset['mcCerrar'] !== undefined) {
    _asmVerTexto = null
    ctx.repintar()
    return true
  }

  return false
}
