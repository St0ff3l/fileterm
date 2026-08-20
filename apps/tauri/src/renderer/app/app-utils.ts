import type { DragEvent, MouseEvent } from 'react'
import type { LocalFileItem, RemoteDragImage, TransferTask, WorkspaceTab } from '@fileterm/core'
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

export function setFileDragPreview(event: DragEvent<HTMLElement>, names: string[]) {
  if (!names.length) {
    return
  }

  const preview = document.createElement('div')
  preview.className = 'file-drag-preview'
  const visibleNames = names.slice(0, 2)
  preview.innerHTML = `
    <span class="file-drag-preview-icon" aria-hidden="true">
      <svg class="app-icon" viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round">
        <g fill="currentColor" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="33.6" transform="scale(0.015625)">
          ${COPY_ICON_PATHS.map((path) => `<path d="${path}"></path>`).join('')}
        </g>
      </svg>
    </span>
    <span>${escapeHtml(visibleNames.join(names.length > 1 ? ', ' : ''))}${names.length > 2 ? ` ${t.moreItemsPrefix ? `${t.moreItemsPrefix} ` : ''}${names.length} ${t.itemsSuffix}` : ''}</span>
  `
  document.body.appendChild(preview)
  event.dataTransfer.setDragImage(preview, 10, 10)
  window.setTimeout(() => preview.remove(), 0)
}

/**
 * AppIcon `copy` 图标的路径数据（1024 视野，经 scale(0.015625) 映射到 16 视野）。
 * 拖拽 ghost 与终端悬浮窗共用同一枚复制图标。
 */
const COPY_ICON_PATHS = [
  'M761.088 715.3152a38.7072 38.7072 0 1 1 0-77.4144 37.4272 37.4272 0 0 0 37.4272-37.4272V265.0112a37.4272 37.4272 0 0 0-37.4272-37.4272H425.6256a37.4272 37.4272 0 0 0-37.4272 37.4272 38.7072 38.7072 0 1 1-77.4144 0 115.0976 115.0976 0 0 1 114.8416-114.8416h335.4624a115.0976 115.0976 0 0 1 114.8416 114.8416v335.4624a115.0976 115.0976 0 0 1-114.8416 114.8416z',
  'M589.4656 883.0976H268.1856a121.1392 121.1392 0 0 1-121.2928-121.2928v-322.56a121.1392 121.1392 0 0 1 121.2928-121.344h321.28a121.1392 121.1392 0 0 1 121.2928 121.2928v322.56c1.28 67.1232-54.1696 121.344-121.2928 121.344zM268.1856 395.3152a43.52 43.52 0 0 0-43.8784 43.8784v322.56a43.52 43.52 0 0 0 43.8784 43.8784h321.28a43.52 43.52 0 0 0 43.8784-43.8784v-322.56a43.52 43.52 0 0 0-43.8784-43.8784z'
] as const

/** Ghost 布局常量，与 session.css 中 `.native-drag-ghost` 保持一致。 */
const NATIVE_DRAG_GHOST_LAYOUT = {
  paddingX: 10,
  paddingY: 7,
  maxWidth: 360,
  radius: 5,
  borderWidth: 1,
  iconSize: 14,
  iconGap: 7,
  titleFontSize: 12,
  /** 与终端悬浮窗复制图标一致的描边粗细（AppIcon strokeWidth 2.1）。 */
  iconStrokeWidth: 2.1,
  /** 光标到 ghost 左上角的逻辑偏移（native-drag-ghost 悬浮定位）。 */
  cursorOffset: 14
} as const

/**
 * 为 Windows 原生拖出渲染 Shell 拖拽图像（DragImageBits 同款位图）。
 * 与 DOM 版 `.native-drag-ghost` 同视觉（单行图标 + 文件名，对齐 macOS）：
 * 主题色通过隐藏探针元素读取，文本与图标用 canvas 绘制，输出 PNG data
 * URL + 物理像素尺寸/偏移。
 */
export function buildNativeDragImage(names: string[]): RemoteDragImage | null {
  if (!names.length || typeof document === 'undefined') {
    return null
  }

  const probe = document.createElement('div')
  probe.className = 'native-drag-ghost'
  probe.style.position = 'fixed'
  probe.style.left = '-9999px'
  probe.style.top = '-9999px'
  probe.style.visibility = 'hidden'
  const icon = document.createElement('span')
  icon.className = 'file-drag-preview-icon'
  probe.appendChild(icon)
  document.body.appendChild(probe)
  const probeStyles = window.getComputedStyle(probe)
  const iconStyles = window.getComputedStyle(icon)
  const background = probeStyles.backgroundColor
  const titleColor = probeStyles.color
  const borderColor = probeStyles.borderColor
  const titleFont = `${NATIVE_DRAG_GHOST_LAYOUT.titleFontSize}px ${probeStyles.fontFamily}`
  const iconColor = iconStyles.color
  probe.remove()

  const visibleNames = names.slice(0, 2).join(names.length > 1 ? ', ' : '')
  const titleText =
    names.length > 2
      ? `${visibleNames} ${t.moreItemsPrefix ? `${t.moreItemsPrefix} ` : ''}${names.length} ${t.itemsSuffix}`.trim()
      : visibleNames

  const canvas = document.createElement('canvas')
  const context = canvas.getContext('2d')
  if (!context) {
    return null
  }

  // 先用逻辑像素排版，再按 devicePixelRatio 放大绘制。
  const scale = Math.max(1, window.devicePixelRatio || 1)
  const layoutPadding = NATIVE_DRAG_GHOST_LAYOUT.paddingX * 2
  context.font = titleFont
  const titleWidth = context.measureText(titleText).width
  const titleRowWidth = NATIVE_DRAG_GHOST_LAYOUT.iconSize + NATIVE_DRAG_GHOST_LAYOUT.iconGap + titleWidth
  const logicalWidth = Math.min(
    NATIVE_DRAG_GHOST_LAYOUT.maxWidth,
    Math.ceil(layoutPadding + titleRowWidth + NATIVE_DRAG_GHOST_LAYOUT.borderWidth * 2)
  )
  const titleRowHeight = Math.max(NATIVE_DRAG_GHOST_LAYOUT.iconSize, NATIVE_DRAG_GHOST_LAYOUT.titleFontSize + 3)
  const logicalHeight =
    NATIVE_DRAG_GHOST_LAYOUT.paddingY * 2 + titleRowHeight + NATIVE_DRAG_GHOST_LAYOUT.borderWidth * 2

  canvas.width = Math.ceil(logicalWidth * scale)
  canvas.height = Math.ceil(logicalHeight * scale)
  context.scale(scale, scale)

  const layout = NATIVE_DRAG_GHOST_LAYOUT
  // 圆角卡片背景
  context.beginPath()
  context.roundRect(
    layout.borderWidth / 2,
    layout.borderWidth / 2,
    logicalWidth - layout.borderWidth,
    logicalHeight - layout.borderWidth,
    layout.radius
  )
  context.fillStyle = background
  context.fill()
  context.lineWidth = layout.borderWidth
  context.strokeStyle = Array.isArray(borderColor) ? borderColor[0] : borderColor
  context.stroke()

  // 单行内容：复制图标 + 文件名
  const contentTop = layout.borderWidth + layout.paddingY
  const iconLeft = layout.borderWidth + layout.paddingX
  // 图标路径为 1024 视野，直接缩放到 iconSize 像素；描边宽度按
  // AppIcon 语义（strokeWidth*16 于 1024 视野）换算，与终端悬浮窗一致。
  const iconScale = layout.iconSize / 1024
  context.save()
  context.translate(iconLeft, contentTop)
  context.scale(iconScale, iconScale)
  context.fillStyle = iconColor
  context.strokeStyle = iconColor
  context.lineWidth = layout.iconStrokeWidth * 16
  context.lineCap = 'round'
  context.lineJoin = 'round'
  for (const pathData of COPY_ICON_PATHS) {
    const iconPath = new Path2D(pathData)
    context.fill(iconPath)
    context.stroke(iconPath)
  }
  context.restore()

  context.font = titleFont
  context.fillStyle = titleColor
  context.textBaseline = 'middle'
  context.fillText(
    titleText,
    iconLeft + layout.iconSize + layout.iconGap,
    contentTop + titleRowHeight / 2,
    logicalWidth - layout.borderWidth - layout.paddingX - (iconLeft + layout.iconSize + layout.iconGap)
  )

  let dataUrl: string
  try {
    dataUrl = canvas.toDataURL('image/png')
  } catch {
    return null
  }

  return {
    dataUrl,
    width: canvas.width,
    height: canvas.height,
    offsetX: Math.round(-layout.cursorOffset * scale),
    offsetY: Math.round(-layout.cursorOffset * scale)
  }
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
