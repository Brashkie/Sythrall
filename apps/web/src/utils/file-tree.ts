// ══════════════════════════════════════════
//  Sythrall — FileList → árbol anidado
//  Convierte el FileList plano que da <input webkitdirectory> (cada File trae
//  su .webkitRelativePath, ej. "proyecto/src/index.ts") en un árbol navegable,
//  100% en el cliente — sin depender de File System Access API (Chrome/Edge
//  únicamente); webkitdirectory/mozdirectory funciona en todos los navegadores
//  modernos (Chrome, Edge, Firefox, Safari 11.1+).
// ══════════════════════════════════════════
// utils/file-tree.ts

import type { CodeFile } from '../types'

export interface FolderTreeNode {
  name: string
  path: string
  type: 'file' | 'directory'
  children?: FolderTreeNode[]
  file?: File
  /** Presente solo en hojas que ya son un CodeFile cargado en state.files. */
  codeFileId?: string
}

export function buildTreeFromFileList(files: FileList): FolderTreeNode {
  const rootName = files[0]?.webkitRelativePath?.split('/')[0] ?? 'carpeta'
  const root: FolderTreeNode = { name: rootName, path: rootName, type: 'directory', children: [] }
  const dirsByPath = new Map<string, FolderTreeNode>([[rootName, root]])

  for (const file of Array.from(files)) {
    const relPath = file.webkitRelativePath || file.name
    const segments = relPath.split('/').filter(Boolean)
    let parent = root
    let pathSoFar = segments[0] ?? rootName

    for (let i = 1; i < segments.length - 1; i++) {
      pathSoFar += `/${segments[i]}`
      let dir = dirsByPath.get(pathSoFar)
      if (!dir) {
        dir = { name: segments[i], path: pathSoFar, type: 'directory', children: [] }
        dirsByPath.set(pathSoFar, dir)
        parent.children!.push(dir)
      }
      parent = dir
    }

    const fileName = segments[segments.length - 1]
    if (fileName) {
      parent.children!.push({ name: fileName, path: relPath, type: 'file', file })
    }
  }

  sortTree(root)
  return root
}

function sortTree(node: FolderTreeNode): void {
  if (!node.children) return
  node.children.sort((a, b) => {
    if (a.type !== b.type) return a.type === 'directory' ? -1 : 1
    return a.name.localeCompare(b.name)
  })
  for (const child of node.children) sortTree(child)
}

// ── Árbol construido desde archivos YA cargados en state.files ─────────────

/**
 * Arma un árbol a partir de CodeFile[]. Usa `.path` cuando existe (viene de
 * una carpeta ya abierta o de un proyecto persistido); si no hay `.path`
 * (archivo suelto de "+ Código"/"+ Log") el archivo cuelga como hoja de
 * nivel raíz, sin carpeta contenedora.
 */
export function buildTreeFromCodeFiles(files: CodeFile[]): FolderTreeNode {
  const root: FolderTreeNode = { name: '', path: '', type: 'directory', children: [] }
  const dirsByPath = new Map<string, FolderTreeNode>([['', root]])

  for (const f of files) {
    const relPath = f.path || f.name
    const segments = relPath.split('/').filter(Boolean)
    let parent = root
    let pathSoFar = ''
    for (let i = 0; i < segments.length - 1; i++) {
      pathSoFar = pathSoFar ? `${pathSoFar}/${segments[i]}` : segments[i]!
      let dir = dirsByPath.get(pathSoFar)
      if (!dir) {
        dir = { name: segments[i]!, path: pathSoFar, type: 'directory', children: [] }
        dirsByPath.set(pathSoFar, dir)
        parent.children!.push(dir)
      }
      parent = dir
    }
    const fileName = segments[segments.length - 1] ?? f.name
    parent.children!.push({ name: fileName, path: relPath, type: 'file', codeFileId: f.id })
  }

  sortTree(root)
  return root
}

/**
 * Combina el árbol de archivos ya cargados con un árbol "pendiente" de una
 * carpeta recién elegida vía "+ Carpeta" (todavía sin leer contenido). Si un
 * path ya está cargado como CodeFile real, esa hoja gana y no se duplica.
 */
export function buildMergedTree(codeFiles: CodeFile[], pendingRoot: FolderTreeNode | null): FolderTreeNode {
  const root = buildTreeFromCodeFiles(codeFiles)
  if (!pendingRoot) return root

  const loadedPaths = new Set(codeFiles.map((f) => f.path || f.name))
  const dirsByPath = new Map<string, FolderTreeNode>([['', root]])
  ;(function index(n: FolderTreeNode): void {
    for (const c of n.children ?? []) {
      if (c.type === 'directory') {
        dirsByPath.set(c.path, c)
        index(c)
      }
    }
  })(root)

  const mergeNode = (node: FolderTreeNode, parent: FolderTreeNode): void => {
    if (node.type === 'directory') {
      let dir = dirsByPath.get(node.path)
      if (!dir) {
        dir = { name: node.name, path: node.path, type: 'directory', children: [] }
        dirsByPath.set(node.path, dir)
        parent.children!.push(dir)
      }
      for (const child of node.children ?? []) mergeNode(child, dir)
    } else {
      if (loadedPaths.has(node.path)) return
      parent.children!.push({ name: node.name, path: node.path, type: 'file', file: node.file })
    }
  }
  for (const child of pendingRoot.children ?? []) mergeNode(child, root)

  sortTree(root)
  return root
}
