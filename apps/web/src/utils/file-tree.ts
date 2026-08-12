// ══════════════════════════════════════════
//  Sythrall — FileList → árbol anidado
//  Convierte el FileList plano que da <input webkitdirectory> (cada File trae
//  su .webkitRelativePath, ej. "proyecto/src/index.ts") en un árbol navegable,
//  100% en el cliente — sin depender de File System Access API (Chrome/Edge
//  únicamente); webkitdirectory/mozdirectory funciona en todos los navegadores
//  modernos (Chrome, Edge, Firefox, Safari 11.1+).
// ══════════════════════════════════════════
// utils/file-tree.ts

export interface FolderTreeNode {
  name: string
  path: string
  type: 'file' | 'directory'
  children?: FolderTreeNode[]
  file?: File
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
