import path from 'node:path'
import { FileInfo, FileType } from 'basic-ftp'
import type { FileEntry } from 'ssh2'
import type { RemoteFileItem } from '@fileterm/core'

export function toRemoteFileItem(basePath: string, entry: FileEntry): RemoteFileItem {
  const fullPath = path.posix.join(basePath, entry.filename)
  const isDirectory = entry.longname.startsWith('d')
  return {
    path: fullPath,
    name: entry.filename,
    type: isDirectory ? 'folder' : 'file',
    modified: formatTimestamp(entry.attrs.mtime),
    size: isDirectory ? '-' : formatBytes(entry.attrs.size),
    permission: entry.longname.split(/\s+/)[0] ?? '',
    ownerGroup: `${entry.attrs.uid ?? 0}/${entry.attrs.gid ?? 0}`
  }
}

export function toFtpRemoteFileItem(basePath: string, entry: FileInfo): RemoteFileItem {
  return toResolvedFtpRemoteFileItem(basePath, entry, entry.type === FileType.Directory || entry.isDirectory)
}

export function toResolvedFtpRemoteFileItem(basePath: string, entry: FileInfo, isDirectory: boolean): RemoteFileItem {
  const fullPath = path.posix.join(basePath, entry.name)
  const resolvedType = isDirectory ? FileType.Directory : entry.type
  return {
    path: fullPath,
    name: entry.name,
    type: isDirectory ? 'folder' : 'file',
    modified: entry.modifiedAt ? formatDate(entry.modifiedAt) : entry.rawModifiedAt,
    size: isDirectory ? '-' : formatBytes(entry.size),
    permission: formatFtpPermissions(resolvedType, entry.permissions),
    ownerGroup: [entry.user, entry.group].filter(Boolean).join('/') || ''
  }
}

export function formatFtpPermissions(type: FileType, permissions?: FileInfo['permissions']) {
  if (!permissions) {
    return type === FileType.Directory ? 'd---------' : '----------'
  }

  return `${type === FileType.Directory ? 'd' : '-'}${formatPermissionGroup(permissions.user)}${formatPermissionGroup(permissions.group)}${formatPermissionGroup(permissions.world)}`
}

export function formatPermissionGroup(value = 0) {
  return `${value & FileInfo.UnixPermission.Read ? 'r' : '-'}${value & FileInfo.UnixPermission.Write ? 'w' : '-'}${value & FileInfo.UnixPermission.Execute ? 'x' : '-'}`
}

export function parentRemotePath(currentPath: string) {
  const normalized = currentPath.endsWith('/') && currentPath !== '/' ? currentPath.slice(0, -1) : currentPath
  const parent = path.posix.dirname(normalized)
  return parent === '.' ? '/' : parent
}

export function formatTimestamp(timestamp?: number) {
  if (!timestamp) {
    return ''
  }
  const date = new Date(timestamp * 1000)
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`
}

export function formatDate(date: Date) {
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`
}

export function formatBytes(size = 0) {
  if (!size) {
    return '0 B'
  }
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let value = size
  let unitIndex = 0
  while (value >= 1000 && unitIndex < units.length - 1) {
    value /= 1000
    unitIndex += 1
  }
  const digits = value >= 10 || unitIndex === 0 ? 0 : 1
  return `${value.toFixed(digits)} ${units[unitIndex]}`
}
