export type FileDropPane = 'local' | 'remote'

export type FileDropTarget = {
  pane: FileDropPane
  path: string
  kind: 'folder' | 'pane'
  name?: string
}

export function getFileDropTarget(
  target: EventTarget | null,
  currentPaths: Record<FileDropPane, string>
): FileDropTarget | null {
  const element = target instanceof Element ? target : null
  if (!element) return null

  const folder = element.closest<HTMLElement>('[data-file-drop-folder-path]')
  const markedPane = element.closest<HTMLElement>('[data-file-drop-pane]')
  const pane = folder?.dataset.fileDropPane ?? markedPane?.dataset.fileDropPane
  if (pane !== 'local' && pane !== 'remote') return null

  return {
    pane,
    path: folder?.dataset.fileDropFolderPath ?? markedPane?.dataset.fileDropCurrentPath ?? currentPaths[pane],
    kind: folder ? 'folder' : 'pane',
    name: folder?.dataset.fileDropFolderName
  }
}

export function getFileDropTargetAtPoint(
  x: number,
  y: number,
  currentPaths: Record<FileDropPane, string>
): FileDropTarget | null {
  return getFileDropTarget(document.elementFromPoint(x, y), currentPaths)
}
