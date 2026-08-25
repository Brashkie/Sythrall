// Banner de bienvenida de la terminal integrada — mismo patrón/paleta que
// scripts/dev-banner.mjs (consola de "npm run dev"), para que la identidad
// visual de Sythrall sea la misma en ambos lugares. Usa solo las funciones
// puras de generación de strings ANSI de `ansimax` (`ascii.banner`/`box`) —
// nunca `write`/`writeln`/`getTerminalWidth`, que asumen un TTY real de
// Node y no tienen sentido en el browser. El string resultante se manda
// directo a `term.write()` de xterm.js, que interpreta ANSI como cualquier
// terminal real.
import './processShim'
import { ascii, gradient } from 'ansimax'

const GRADIENT = ['#3d9eff', '#b87dff', '#00f5a0']

export function terminalWelcomeBanner(shellLabel: string, cols: number): string {
  const logo = ascii.banner('Sythrall', {
    font: 'small',
    colorFn: (t) => gradient(t, GRADIENT),
  })
  const info = ascii.box(`shell: ${shellLabel}`, {
    padding: 1,
    borderStyle: 'rounded',
    width: Math.max(20, Math.min(cols - 2, 60)),
  })
  // xterm necesita \r\n explícito — a diferencia de una consola Node real,
  // no traduce \n solo a nueva línea con retorno de carro.
  return `${logo}\r\n${info}`.split('\n').join('\r\n') + '\r\n'
}
