// ══════════════════════════════════════════
//  Sythrall — Adaptador de "+ Carpeta" (cross-browser, estilo VSCode)
//  Arma el árbol de una carpeta elegida vía <input webkitdirectory> (funciona
//  en Chrome/Edge/Firefox/Safari — a diferencia de la File System Access API,
//  que es solo Chromium) y lo entrega a explorer.ts, que es el único dueño
//  de #file-tree (ver components/explorer.ts::explorerSetFolderRoot).
// ══════════════════════════════════════════
// components/file-browser.ts

import { state } from '../store/state'
import { buildTreeFromFileList } from '../utils/file-tree'
import { appendLog } from '../utils/helpers'
import { persistFilesToProject } from './app'
import { explorerSetFolderRoot } from './explorer'

export function handleFolderPick(files: FileList | null): void {
  if (!files || files.length === 0) return
  const root = buildTreeFromFileList(files)
  explorerSetFolderRoot(root)
  appendLog('info', `Carpeta explorada: ${root.name} (${files.length} archivos)`, 'fe')

  // "+ Carpeta" también guarda en el proyecto activo (o crea uno nuevo) — la
  // navegación del árbol sigue siendo local/lazy, pero el contenido ya no se
  // pierde al refrescar. Mismo camino que "+ Código", ver components/app.ts.
  if (state.backendOk) void persistFilesToProject(Array.from(files), 'folder')
}
