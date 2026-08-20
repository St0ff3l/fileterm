import type { DragEvent, MouseEvent } from 'react'
import type { LocalFileItem, TransferTask, WorkspaceTab } from '@fileterm/core'
import type { AppIconName } from '../features/common/AppIcon'
import { formatMessage, localizeErrorScope, t } from '../i18n'

export const localFileDragType = 'application/x-fileterm-local-file'
export const remoteFileDragType = 'application/x-fileterm-remote-file'
export const WINDOWS_DRIVES_PATH = 'fileterm://windows-drives'

export function isActiveTransfer(transfer: TransferTask) {
  return (
    transfer.status === 'running' ||
    transfer.status === 'queued' ||
    transfer.status === 'verifying' ||
    transfer.status === 'finalizing'
  )
}

export function isCompletedTransfer(transfer: TransferTask) {
  return transfer.status === 'done' || transfer.status === 'failed' || transfer.status === 'canceled'
}

export function copyText(value: string) {
  if (window.fileterm?.writeClipboardText) {
    void window.fileterm.writeClipboardText(value)
    return
  }

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

export function settledResultsError(action: string, results: PromiseSettledResult<unknown>[]): Error | null {
  const failures = results.filter((result): result is PromiseRejectedResult => result.status === 'rejected')
  if (!failures.length) {
    return null
  }
  const firstReason = failures[0]?.reason
  const detail = firstReason instanceof Error ? firstReason.message : String(firstReason ?? '')
  const summary = formatMessage(t.operationFailedSummary, {
    action: localizeErrorScope(action),
    failed: failures.length,
    total: results.length
  })
  return new Error(detail ? `${summary}；${detail}` : summary)
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

export function withParentRow(dirPath: string, items: LocalFileItem[], rootPath?: string) {
  const normalizedPath = dirPath
    .replace(/[\\/]+$/, '')
    .replace(/\\/g, '/')
    .toLocaleLowerCase()
  const normalizedRoot = rootPath
    ?.replace(/[\\/]+$/, '')
    .replace(/\\/g, '/')
    .toLocaleLowerCase()
  const windowsRoot = /^[A-Za-z]:[\\/]?$/.test(dirPath) ? WINDOWS_DRIVES_PATH : null
  const isAtRoot = normalizedRoot !== undefined && normalizedPath === normalizedRoot
  const rawParentPath = dirPath.includes('/')
    ? dirPath.split('/').slice(0, -1).join('/') || '/'
    : dirPath.includes('\\')
      ? windowsRoot || dirPath.split('\\').slice(0, -1).join('\\') || '\\'
      : dirPath
  const parentPath = /^[A-Za-z]:$/.test(rawParentPath) ? `${rawParentPath}\\` : rawParentPath
  return dirPath === '/' || dirPath === WINDOWS_DRIVES_PATH || isAtRoot
    ? items
    : [
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

const DRAG_PREVIEW_ICON_STROKE_WIDTH = 1.8

/**
 * File-list icon paths in the same 16px viewBox used by AppIcon. Keeping the
 * native PNG and HTML5 preview on these paths means drag-out uses the exact
 * same semantic icon as the file table (folder, archive, code, image, etc.).
 */
const DRAG_PREVIEW_ICON_PATHS: Partial<Record<AppIconName, readonly string[]>> = {
  folder: ['M2.5 4.5h3l1.4 1.6h6.6v5.8a1.1 1.1 0 0 1-1.1 1.1H3.6a1.1 1.1 0 0 1-1.1-1.1V5.6a1.1 1.1 0 0 1 1.1-1.1Z'],
  file: ['M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z', 'M9.5 2.5V6H13'],
  archive: [
    'M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z',
    'M9.5 2.5V6H13',
    'M8 4.1v1.1M8 6.2v1.1M8 8.3v1.1M7 10h2'
  ],
  video: [
    'M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z',
    'M9.5 2.5V6H13',
    'm6.9 7.1 3.1 1.9-3.1 1.9Z'
  ],
  audio: [
    'M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z',
    'M9.5 2.5V6H13',
    'M9.5 7.1v3.1a1 1 0 1 1-1-.9h1',
    'M9.5 7.1 7.7 7.7'
  ],
  image: [
    'M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z',
    'M9.5 2.5V6H13',
    'm6 11 1.9-2 1.4 1.4 1.7-1.8 1 1.1',
    'M7 7.2h.01'
  ],
  document: [
    'M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z',
    'M9.5 2.5V6H13',
    'M6.4 7.6h3.9M6.4 9.2h3.9M6.4 10.8h2.7'
  ],
  spreadsheet: [
    'M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z',
    'M9.5 2.5V6H13M6.2 7.7h4.6v3.8H6.2zM8.5 7.7v3.8M6.2 9.6h4.6'
  ],
  presentation: [
    'M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z',
    'M9.5 2.5V6H13M6.2 7.4h4.6v3H6.2zM8.5 10.4v1.4M7.2 11.8h2.6'
  ],
  'config-file': [
    'M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z',
    'M9.5 2.5V6H13M6.2 8h4.6M6.2 10.5h4.6M7.4 7.2v1.6M9.6 9.7v1.6'
  ],
  database: [
    'M12.5 4.2c0 1.1-2 2-4.5 2s-4.5-.9-4.5-2 2-2 4.5-2 4.5.9 4.5 2Z',
    'M3.5 4.2v3.8c0 1.1 2 2 4.5 2s4.5-.9 4.5-2V4.2M3.5 8v3.8c0 1.1 2 2 4.5 2s4.5-.9 4.5-2V8'
  ],
  'font-file': [
    'M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z',
    'M9.5 2.5V6H13M6.2 11.5 8.3 7l2.1 4.5M6.9 10h2.8'
  ],
  package: ['m8 2.2 5 2.7v6.2l-5 2.7-5-2.7V4.9l5-2.7Z', 'm3.3 5.1 4.7 2.6 4.7-2.6M8 7.7v6M5.5 3.6l4.8 2.6'],
  'terminal-file': [
    'M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z',
    'M9.5 2.5V6H13m-6.7 2 1.5 1.4-1.5 1.4M8.8 11h2'
  ],
  pdf: [
    'M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z',
    'M9.5 2.5V6H13',
    'M6.2 10.7V7.4h1.2a.9.9 0 1 1 0 1.8H6.2m2.2 1.5V7.4h.7a1.6 1.6 0 0 1 0 3.3h-.7m2-.1V7.4h1.4'
  ],
  code: [
    'M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z',
    'M9.5 2.5V6H13',
    'm7.1 8.1-1.4 1.2 1.4 1.2M9.1 8.1l1.4 1.2-1.4 1.2'
  ],
  disk: [
    'M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z',
    'M9.5 2.5V6H13',
    'M8.2 11.6a2.2 2.2 0 1 0 0-4.4 2.2 2.2 0 0 0 0 4.4Z',
    'M8.2 9.4h.01'
  ]
}

function getDragPreviewIconPaths(iconName: AppIconName) {
  return DRAG_PREVIEW_ICON_PATHS[iconName] ?? DRAG_PREVIEW_ICON_PATHS.file!
}

export function setFileDragPreview(event: DragEvent<HTMLElement>, names: string[], iconName: AppIconName = 'file') {
  if (!names.length) {
    return
  }

  const preview = document.createElement('div')
  preview.className = 'file-drag-preview'
  const visibleNames = names.slice(0, 2)
  const iconPaths = getDragPreviewIconPaths(iconName)
  preview.innerHTML = `
    <span class="file-drag-preview-icon" aria-hidden="true">
      <svg class="app-icon" viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round">
        <g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="${DRAG_PREVIEW_ICON_STROKE_WIDTH}">
          ${iconPaths.map((path) => `<path d="${path}"></path>`).join('')}
        </g>
      </svg>
    </span>
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
  if (transfer.status === 'paused') {
    return t.transferPaused
  }
  if (transfer.status === 'interrupted') {
    return t.transferInterrupted
  }
  if (transfer.status === 'verifying') {
    return t.transferVerifying
  }
  if (transfer.status === 'finalizing') {
    return t.transferFinalizing
  }
  return transfer.direction === 'upload' ? t.uploading : t.downloading
}

export function getTransferTimestamp(transfer: TransferTask) {
  return transfer.updatedAt ?? transfer.createdAt
}

export function formatTransferDateTime(timestamp?: number) {
  if (timestamp === undefined || !Number.isFinite(timestamp)) {
    return undefined
  }

  const date = new Date(timestamp)
  if (Number.isNaN(date.getTime())) {
    return undefined
  }

  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(
    date.getMinutes()
  )}:${pad(date.getSeconds())}`
}

export function formatTransferBytes(bytes?: number) {
  if (bytes === undefined || !Number.isFinite(bytes) || bytes < 0) {
    return undefined
  }
  if (bytes >= 1024 ** 4) {
    return `${(bytes / 1024 ** 4).toFixed(bytes >= 10 * 1024 ** 4 ? 0 : 1)} TB`
  }
  if (bytes >= 1024 ** 3) {
    return `${(bytes / 1024 ** 3).toFixed(bytes >= 10 * 1024 ** 3 ? 0 : 1)} GB`
  }
  if (bytes >= 1024 ** 2) {
    return `${(bytes / 1024 ** 2).toFixed(bytes >= 10 * 1024 ** 2 ? 0 : 1)} MB`
  }
  if (bytes >= 1024) {
    return `${Math.round(bytes / 1024)} KB`
  }
  return `${bytes} B`
}

function escapeHtml(value: string) {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;')
}
