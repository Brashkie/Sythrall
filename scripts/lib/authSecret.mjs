// Secreto compartido para firmar/verificar los tokens de sesión (JWT) entre
// apps/api (emisor) y services/terminal (verificador) — ninguno de los dos
// lo genera por su cuenta ni lee `.env` directamente (siguen el patrón que
// ya tienen, `std::env::var`/`os.environ.get` puro); es responsabilidad de
// esta capa de orquestación garantizar que llegue a su entorno.
//
// `ensureAuthSecret()` la llama `dev-banner.mjs`, que corre secuencialmente
// ANTES de que `concurrently` arranque `run-backend.mjs`/`run-terminal.mjs`
// en paralelo — evita cualquier carrera entre esos dos tratando de generarla
// a la vez. `readAuthSecret()` (usada por esos dos) solo necesita leerla, y
// cae a `ensureAuthSecret()` como red de seguridad si alguien corre uno de
// esos scripts suelto, sin haber pasado nunca por `npm run dev`.

import { existsSync, readFileSync, appendFileSync } from 'node:fs'
import { randomBytes } from 'node:crypto'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const ENV_PATH = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '.env')
const KEY = 'SYTHRALL_AUTH_SECRET'

function readEnvValue(content, key) {
  const line = content.split('\n').find((l) => l.trim().startsWith(`${key}=`))
  return line ? line.slice(line.indexOf('=') + 1).trim() : null
}

export function ensureAuthSecret() {
  const content = existsSync(ENV_PATH) ? readFileSync(ENV_PATH, 'utf8') : ''
  const existing = readEnvValue(content, KEY)
  if (existing) return existing

  const secret = randomBytes(32).toString('hex')
  const prefix = content && !content.endsWith('\n') ? '\n' : ''
  appendFileSync(
    ENV_PATH,
    `${prefix}\n# Secreto compartido para firmar/verificar tokens de sesión (JWT) entre\n` +
      `# apps/api (emisor) y services/terminal (verificador) — generado acá\n` +
      `# porque scripts/dev-banner.mjs corre antes que todo lo demás en "npm run dev".\n` +
      `${KEY}=${secret}\n`,
  )
  return secret
}

export function readAuthSecret() {
  if (!existsSync(ENV_PATH)) return ensureAuthSecret()
  const value = readEnvValue(readFileSync(ENV_PATH, 'utf8'), KEY)
  return value ?? ensureAuthSecret()
}
