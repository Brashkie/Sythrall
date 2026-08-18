#!/usr/bin/env node
// Lanza el sidecar de terminal (Rust, services/terminal) con cwd=raíz del repo
// (ahí vive Cargo.toml). `cargo run` compila si hace falta y cachea
// incrementalmente — no requiere un paso de build aparte para desarrollo.

import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { ascii, color } from 'ansimax'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')

const child = spawn('cargo', ['run', '--bin', 'terminal-server'], {
  cwd: root,
  stdio: 'inherit',
  shell: process.platform === 'win32',
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
