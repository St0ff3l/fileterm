import type { DragEvent, MouseEvent } from 'react'
import type { LocalFileItem, TransferTask, WorkspaceTab } from '@termdock/core'
import { t } from '../i18n'

export const localFileDragType = 'application/x-termdock-local-file'
export const remoteFileDragType = 'application/x-termdock-remote-file'

export function isActiveTransfer(transfer: TransferTask) {
  return transfer.status === 'running' || transfer.status === 'queued'
}

export function isCompletedTransfer(transfer: TransferTask) {
  return transfer.status === 'done' || transfer.status === 'failed' || transfer.status === 'canceled'
}

export function copyText(value: string) {
  if (navigator.clipboard?.writeText) {
    void navigator.clipboard.writeText(value)
    return
  }

  const textarea = document.createElement('textarea')
  textarea.value = value
  textarea.setAttribute('readonly', '')
  textarea.style.position = 'fixed'
  textarea.style.opacity = '0'
  document.body.appendChild(textarea)
  textarea.select()
  document.execCommand('copy')
  document.body.removeChild(textarea)
}

export function hasSelectedText() {
  const selection = window.getSelection()
  if (!selection || selection.rangeCount === 0) {
    return false
  }

  return selection.toString().trim().length > 0
}

export function runningTransfers(transfers: TransferTask[]) {
  return transfers.filter(isActiveTransfer).length
}

export function homeTabKey(id: string) {
  return `home:${id}`
}

export function sessionTabKey(id: string) {
  return `session:${id}`
}

export function reorderTabKeys(keys: string[], draggingKey: string | null, targetKey: string) {
  if (!draggingKey || draggingKey === targetKey) {
    return keys
  }

  const draggingIndex = keys.indexOf(draggingKey)
  const targetIndex = keys.indexOf(targetKey)
  if (draggingIndex === -1 || targetIndex === -1) {
    return keys
  }

  const next = [...keys]
  next.splice(draggingIndex, 1)
  next.splice(targetIndex, 0, draggingKey)
  return next
}

export function insertTabKeyAfter(keys: string[], newKey: string, afterKey: string | null) {
  const withoutNewKey = keys.filter((key) => key !== newKey)
  if (!afterKey) {
    return [...withoutNewKey, newKey]
  }

  const targetIndex = withoutNewKey.indexOf(afterKey)
  if (targetIndex === -1) {
    return [...withoutNewKey, newKey]
  }

  const next = [...withoutNewKey]
  next.splice(targetIndex + 1, 0, newKey)
  return next
}

export function tabStatusClass(status: WorkspaceTab['status']) {
  if (status === 'connected') {
    return 'connected'
  }
  if (status === 'error' || status === 'closed') {
    return 'disconnected'
  }
  if (status === 'connecting') {
    return 'connecting'
  }
  return 'idle'
}

export function withParentRow(dirPath: string, items: LocalFileItem[]) {
  const parentPath = dirPath.includes('/') ? dirPath.split('/').slice(0, -1).join('/') || '/' : dirPath
  return dirPath === '/' ? items : [
    {
      path: parentPath,
      name: '..',
      type: 'folder' as const,
      modified: '',
      size: '-'
    },
    ...items
  ]
}

export function nextSelection<T extends { path: string }>({
  anchorPath,
  currentSelection,
  event,
  itemPath,
  rows
}: {
  anchorPath: string | null
  currentSelection: string[]
  event: MouseEvent<HTMLTableRowElement>
  itemPath: string
  rows: T[]
}) {
  if (event.shiftKey && anchorPath) {
    const anchorIndex = rows.findIndex((row) => row.path === anchorPath)
    const itemIndex = rows.findIndex((row) => row.path === itemPath)
    if (anchorIndex !== -1 && itemIndex !== -1) {
      const start = Math.min(anchorIndex, itemIndex)
      const end = Math.max(anchorIndex, itemIndex)
      return rows.slice(start, end + 1).map((row) => row.path)
    }
  }

  if (event.metaKey || event.ctrlKey) {
    return currentSelection.includes(itemPath)
      ? currentSelection.filter((selectedPath) => selectedPath !== itemPath)
      : [...currentSelection, itemPath]
  }

  return [itemPath]
}

export function rangePaths<T extends { path: string }>(rows: T[], startPath: string, endPath: string) {
  const startIndex = rows.findIndex((row) => row.path === startPath)
  const endIndex = rows.findIndex((row) => row.path === endPath)
  if (startIndex === -1 || endIndex === -1) {
    return endPath ? [endPath] : []
  }
  const start = Math.min(startIndex, endIndex)
  const end = Math.max(startIndex, endIndex)
  return rows.slice(start, end + 1).map((row) => row.path)
}

export function mergeUnique(values: string[]) {
  return Array.from(new Set(values))
}

export function parseDraggedPaths(payload: string) {
  try {
    const parsed = JSON.parse(payload)
    return Array.isArray(parsed) ? parsed.filter((value): value is string => typeof value === 'string') : [payload]
  } catch {
    return [payload]
  }
}

export function setFileDragPreview(event: DragEvent<HTMLElement>, names: string[]) {
  if (!names.length) {
    return
  }

  const preview = document.createElement('div')
  preview.className = 'file-drag-preview'
  const visibleNames = names.slice(0, 2)
  preview.innerHTML = `
    <span class="file-drag-preview-icon">□</span>
    <span>${escapeHtml(visibleNames.join(names.length > 1 ? ', ' : ''))}${names.length > 2 ? ` ${t.moreItemsPrefix ? `${t.moreItemsPrefix} ` : ''}${names.length} ${t.itemsSuffix}` : ''}</span>
  `
  document.body.appendChild(preview)
  event.dataTransfer.setDragImage(preview, 10, 10)
  window.setTimeout(() => preview.remove(), 0)
}

export function transferStatusText(transfer: TransferTask) {
  if (transfer.status === 'failed') {
    return transfer.direction === 'upload' ? t.uploadFailed : t.downloadFailed
  }
  if (transfer.status === 'canceled') {
    return transfer.direction === 'upload' ? t.uploadCanceled : t.downloadCanceled
  }
  if (transfer.status === 'done') {
    return transfer.direction === 'upload' ? t.uploadDone : t.downloadDone
  }
  if (transfer.status === 'queued') {
    return transfer.direction === 'upload' ? t.waitingUpload : t.waitingDownload
  }
  return transfer.direction === 'upload' ? t.uploading : t.downloading
}

function escapeHtml(value: string) {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;')
}
