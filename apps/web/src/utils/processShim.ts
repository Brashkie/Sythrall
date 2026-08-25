// `ansimax` está pensada para Node (CLI rendering) — algunas de sus rutas
// internas (detección de soporte de color, principalmente) leen `process`
// incondicionalmente, que no existe como global en el browser y tira
// "process is not defined" apenas se llama a cualquier función que pase por
// ahí. Shim mínimo, solo lo que esas rutas necesitan.
//
// `FORCE_COLOR: '3'` fuerza truecolor: sin esto, `ansimax` detecta soporte
// de color vía `process.stdout.isTTY`, que naturalmente es `undefined` acá
// (no hay un TTY real) aunque xterm.js sí renderiza ANSI/truecolor
// perfectamente — sin el force, el texto saldría plano, sin color.
//
// Debe importarse ANTES que cualquier módulo de `ansimax` (el orden de los
// imports en el mismo archivo se preserva, aunque ESM los "hoistea") —
// ver `terminalBanner.ts`.
if (typeof (globalThis as Record<string, unknown>).process === 'undefined') {
  ;(globalThis as Record<string, unknown>).process = {
    env: { FORCE_COLOR: '3' },
    platform: 'browser',
    argv: [],
  }
}

export {}
