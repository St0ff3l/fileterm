import type { DragEvent } from 'react'
import type { FileTermDesktopApi, LocalFileItem } from '@fileterm/core'
import { WINDOWS_DRIVES_PATH } from '../app/app-utils'
import type { FileClipboardOperation, FileDialogTarget, LocalNetworkShareSource } from './file-operations-types'

const REMOTE_METHOD_ERROR_PREFIX = /Error invoking remote method '[^']+':\s*/i
const SMB_CREDENTIALS_REQUIRED = /SMB_CREDENTIALS_REQUIRED/i

export function isSmbCredentialsRequiredError(error: unknown) {
  const message = error instanceof Error ? error.message : String(error)
  return SMB_CREDENTIALS_REQUIRED.test(message)
}

export function areClipboardItemsEqual(left: FileDialogTarget[], right: FileDialogTarget[]) {
  if (left.length !== right.length) {
    return false
  }

  for (let index = 0; index < left.length; index += 1) {
    const leftItem = left[index]
    const rightItem = right[index]
    if (
      !leftItem ||
      !rightItem ||
      leftItem.pane !== rightItem.pane ||
      leftItem.path !== rightItem.path ||
      leftItem.name !== rightItem.name ||
      leftItem.type !== rightItem.type ||
      leftItem.isSymlink !== rightItem.isSymlink
    ) {
      return false
    }
  }

  return true
}

export function splitNameForDuplicate(name: string, type: 'file' | 'folder') {
  if (type === 'folder') {
    return { stem: name, ext: '' }
  }

  const dotIndex = name.lastIndexOf('.')
  if (dotIndex <= 0 || dotIndex === name.length - 1) {
    return { stem: name, ext: '' }
  }

  return {
    stem: name.slice(0, dotIndex),
    ext: name.slice(dotIndex)
  }
}

export function makeDuplicateName(name: string, type: 'file' | 'folder', attempt: number) {
  const { stem, ext } = splitNameForDuplicate(name, type)
  const suffix = attempt === 1 ? ' copy' : ` copy ${attempt}`
  return `${stem}${suffix}${ext}`
}

export function allocateTargetNames(
  items: FileDialogTarget[],
  existingNames: string[],
  operation: FileClipboardOperation,
  destinationPath: string
) {
  const reservedNames = new Set(existingNames)
  return items.map((item) => {
    const isSameDirectory =
      item.pane === 'remote'
        ? remoteDirname(item.path) === destinationPath
        : localDirname(item.path) === destinationPath

    let nextName = item.name

    if (operation === 'cut' && isSameDirectory) {
      reservedNames.add(nextName)
      return nextName
    }

    if (reservedNames.has(nextName) || (operation === 'copy' && isSameDirectory)) {
      let attempt = 1
      do {
        nextName = makeDuplicateName(item.name, item.type, attempt)
        attempt += 1
      } while (reservedNames.has(nextName))
    }

    reservedNames.add(nextName)
    return nextName
  })
}

export function remoteDirname(targetPath: string) {
  const normalized = targetPath.replace(/\/+$/, '') || '/'
  if (normalized === '/') {
    return '/'
  }
  const slashIndex = normalized.lastIndexOf('/')
  if (slashIndex <= 0) {
    return '/'
  }
  return normalized.slice(0, slashIndex)
}

export function joinRemotePath(directoryPath: string, name: string) {
  return directoryPath === '/' ? `/${name}` : `${directoryPath.replace(/\/+$/, '')}/${name}`
}

export function normalizeLocalPath(targetPath: string) {
  return targetPath.replace(/[\\/]+$/, '')
}

export function localDirname(targetPath: string) {
  const normalized = normalizeLocalPath(targetPath)
  if (/^[A-Za-z]:$/.test(normalized)) {
    return WINDOWS_DRIVES_PATH
  }
  const slashIndex = Math.max(normalized.lastIndexOf('/'), normalized.lastIndexOf('\\'))
  if (slashIndex <= 0) {
    return slashIndex === 0 ? normalized.slice(0, 1) : '.'
  }
  if (slashIndex === 2 && /^[A-Za-z]:/.test(normalized)) {
    return normalized.slice(0, 3)
  }
  return normalized.slice(0, slashIndex)
}

export function joinLocalPath(directoryPath: string, name: string) {
  const separator = directoryPath.includes('\\') ? '\\' : '/'
  const normalized = normalizeLocalPath(directoryPath)
  if (normalized === separator) {
    return `${separator}${name}`
  }
  return `${normalized}${separator}${name}`
}

export function normalizeNetworkSharePath(targetPath: string) {
  const normalized = targetPath.trim().replace(/\/+/g, '\\')
  return normalized.replace(/^\\+/, '\\\\').replace(/\\+$/, '')
}

export function joinNetworkSharePath(directoryPath: string, name: string) {
  return `${normalizeNetworkSharePath(directoryPath)}\\${name.trim().replace(/^[\\/]+|[\\/]+$/g, '')}`
}

export function networkShareHostPath(remotePath: string) {
  const host = normalizeNetworkSharePath(remotePath).split('\\').filter(Boolean)[0]
  return host ? `\\\\${host}` : normalizeNetworkSharePath(remotePath)
}

export function createNetworkShareParentRow(hostPath: string): LocalFileItem {
  return {
    path: hostPath,
    name: '..',
    type: 'folder',
    modified: '',
    size: '-'
  }
}

export function createNetworkShareRootItems(source: LocalNetworkShareSource) {
  return source.shares.map((share) => ({
    path: joinNetworkSharePath(source.hostPath, share),
    name: share,
    type: 'folder' as const,
    modified: '',
    size: '-'
  }))
}

export function isLocalPathWithin(rootPath: string, targetPath: string) {
  const normalizeForComparison = (value: string) => normalizeLocalPath(value).replace(/\\/g, '/').toLocaleLowerCase()
  const root = normalizeForComparison(rootPath)
  const target = normalizeForComparison(targetPath)
  return target === root || target.startsWith(`${root}/`)
}

export function areLocalPathsEqual(leftPath: string, rightPath: string) {
  const normalizeForComparison = (value: string) => normalizeLocalPath(value).replace(/\\/g, '/').toLocaleLowerCase()
  return normalizeForComparison(leftPath) === normalizeForComparison(rightPath)
}

export function isNetworkShareHostPath(source: LocalNetworkShareSource, targetPath: string) {
  return normalizeNetworkSharePath(targetPath).toLocaleLowerCase() === source.hostPath.toLocaleLowerCase()
}

export function isNetworkShareRootItem(source: LocalNetworkShareSource, targetPath: string) {
  const target = normalizeNetworkSharePath(targetPath).toLocaleLowerCase()
  const host = source.hostPath.toLocaleLowerCase()
  const prefix = `${host}\\`
  const relative = target.startsWith(prefix) ? target.slice(prefix.length) : ''
  return Boolean(relative) && !relative.includes('\\')
}

export function resolveNetworkSharePath(source: LocalNetworkShareSource, targetPath: string) {
  const target = normalizeNetworkSharePath(targetPath)
  const remoteRoot = normalizeNetworkSharePath(source.remotePath)
  const targetLower = target.toLocaleLowerCase()
  const remoteRootLower = remoteRoot.toLocaleLowerCase()

  if (targetLower === remoteRootLower) {
    return source.mountPath
  }

  const prefix = `${remoteRoot}\\`
  if (!targetLower.startsWith(prefix.toLocaleLowerCase())) {
    return targetPath
  }

  return target.slice(prefix.length).split('\\').filter(Boolean).reduce(joinLocalPath, source.mountPath)
}

export function getNetworkShareDisplayPath(source: LocalNetworkShareSource, localPath: string) {
  if (isNetworkShareHostPath(source, localPath)) {
    return source.hostPath
  }

  const normalizedRoot = normalizeLocalPath(source.mountPath).replaceAll('\\', '/')
  const normalizedTarget = normalizeLocalPath(localPath).replaceAll('\\', '/')
  const root = normalizedRoot.toLocaleLowerCase()
  const target = normalizedTarget.toLocaleLowerCase()
  if (target === root) {
    return source.remotePath
  }

  const prefix = `${root}/`
  if (!target.startsWith(prefix)) {
    return source.remotePath
  }

  return joinNetworkSharePath(
    source.remotePath,
    normalizedTarget.slice(normalizedRoot.length + 1).replaceAll('/', '\\')
  )
}

export function normalizeRemoteErrorMessage(error: unknown) {
  const rawMessage = error instanceof Error ? error.message : String(error)
  return rawMessage.replace(REMOTE_METHOD_ERROR_PREFIX, '').trim()
}

export function shouldPromptForRootAccess(error: unknown) {
  const message = normalizeRemoteErrorMessage(error)
  return /未检测到可复用的 sudo 授权|sudo 密码错误|sudo 密码无效|sudo 认证失败|sudo 验证超时|sudo credentials|incorrect password|authentication failure/i.test(
    message
  )
}

export function fileNameFromPath(filePath: string) {
  return filePath.split(/[/\\]/).pop() || filePath
}

export function extractDroppedLocalPaths(event: DragEvent<HTMLDivElement>, desktopApi?: FileTermDesktopApi) {
  const fileList = Array.from(event.dataTransfer.files)
  const filePaths = (
    desktopApi?.getDroppedFilePaths?.(fileList) ??
    fileList.map((file) => (file as File & { path?: string }).path).filter(Boolean)
  ).filter((filePath): filePath is string => Boolean(filePath))

  if (filePaths.length) {
    return filePaths
  }

  return Array.from(event.dataTransfer.items)
    .map((item) => item.getAsFile() as (File & { path?: string }) | null)
    .map((file) => file?.path)
    .filter((filePath): filePath is string => Boolean(filePath))
}
