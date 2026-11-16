#!/usr/bin/env node
// Lanza el sidecar de complejidad (Rust, services/complexity) con cwd=raíz del
// repo (ahí vive Cargo.toml). `cargo run` compila si hace falta y cachea
// incrementalmente — no requiere un paso de build aparte para desarrollo.

import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { ascii, color } from 'ansimax'
import { readAuthSecret } from './lib/authSecret.mjs'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')

// **Fase 35** — el secreto va acá porque el motor pasó a ser el EMISOR de los
// tokens de sesión de la terminal (`servicio/sistema.rs`). Antes lo recibía
// `run-backend.mjs`, que ya no existe; `run-terminal.mjs` sigue recibiéndolo
// porque es el verificador. Los dos tienen que ver el mismo valor o el token
// que uno firma el otro lo rechaza.
const child = spawn('cargo', ['run', '--bin', 'complexity-engine'], {
  cwd: root,
  stdio: 'inherit',
  shell: process.platform === 'win32',
  env: { ...process.env, SYTHRALL_AUTH_SECRET: readAuthSecret() },
})

child.on('error', (err) => {
  if (err.code === 'ENOENT') {
    console.error(
      ascii.box(color.red('No se encontró "cargo".\nInstalá el toolchain de Rust (https://rustup.rs) y volvé a intentar.'), {
        padding: 1,
        borderStyle: 'rounded',
      }),
    )
  } else {
    console.error(err)
  }
  process.exit(1)
})

child.on('exit', (code) => process.exit(code ?? 0))
for (const sig of ['SIGINT', 'SIGTERM']) {
  process.on(sig, () => child.kill(sig))
}
