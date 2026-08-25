#!/usr/bin/env node
// Lanza uvicorn usando el Python del venv de la raíz (.venv), con cwd=apps/api/
// (rutas relativas como "uploads/projects" en main.py dependen de ese cwd).
// Resuelve la ruta del intérprete según el SO en vez de depender de un
// comando de shell distinto por plataforma — así `npm run dev` funciona
// igual en Windows/macOS/Linux.

import { existsSync } from 'node:fs'
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { ascii, color } from 'ansimax'
import { readAuthSecret } from './lib/authSecret.mjs'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const candidates = [join(root, '.venv', 'bin', 'python'), join(root, '.venv', 'Scripts', 'python.exe')]
const python = candidates.find((p) => existsSync(p))

if (!python) {
  console.error(
    ascii.box(color.red('No se encontró .venv en la raíz del repo.\nEjecutá scripts/setup.ps1 (o setup.sh) primero.'), {
      padding: 1,
      borderStyle: 'rounded',
    }),
  )
  process.exit(1)
}

const child = spawn(python, ['-m', 'uvicorn', 'main:app', '--host', '0.0.0.0', '--port', '8420', '--reload'], {
  cwd: join(root, 'apps', 'api'),
  stdio: 'inherit',
  env: { ...process.env, SYTHRALL_AUTH_SECRET: readAuthSecret() },
})

child.on('exit', (code) => process.exit(code ?? 0))
for (const sig of ['SIGINT', 'SIGTERM']) {
  process.on(sig, () => child.kill(sig))
}
