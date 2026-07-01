import type { LocalFileItem, RemoteFileItem } from '@fileterm/core'
import { t } from '../../i18n'
import type { AppIconName } from '../common/AppIcon'

type FileRow = Pick<LocalFileItem | RemoteFileItem, 'name' | 'type'>

const archiveExtensions = new Set([
  '7z', 'bz2', 'cab', 'gz', 'iso', 'lz', 'lz4', 'rar', 'tar', 'tgz', 'txz', 'xz', 'zip', 'zst'
])

const videoExtensions = new Set([
  'avi', 'flv', 'm4v', 'mkv', 'mov', 'mp4', 'mpeg', 'mpg', 'rm', 'rmvb', 'ts', 'webm', 'wmv'
])

const audioExtensions = new Set([
  'aac', 'aiff', 'alac', 'amr', 'ape', 'flac', 'm4a', 'mid', 'midi', 'mp3', 'ogg', 'opus', 'wav', 'wma'
])

const imageExtensions = new Set([
  'avif', 'bmp', 'gif', 'heic', 'ico', 'jpeg', 'jpg', 'png', 'psd', 'svg', 'tif', 'tiff', 'webp'
])

const documentExtensions = new Set([
  'doc', 'docm', 'docx', 'md', 'mdx', 'odt', 'pages', 'pdf', 'rtf', 'txt'
])

const spreadsheetExtensions = new Set([
  'csv', 'numbers', 'ods', 'tsv', 'xls', 'xlsb', 'xlsm', 'xlsx'
])

const presentationExtensions = new Set([
  'key', 'odp', 'potx', 'pps', 'ppsx', 'ppt', 'pptm', 'pptx'
])

const codeExtensions = new Set([
  'bash', 'c', 'cc', 'cfg', 'conf', 'cpp', 'cs', 'css', 'go', 'h', 'hpp', 'htm', 'html', 'ini',
  'java', 'js', 'json', 'jsx', 'kt', 'less', 'log', 'lua', 'mjs', 'php', 'pl', 'py', 'rb', 'rs',
  'sass', 'scss', 'sh', 'sql', 'swift', 'toml', 'ts', 'tsx', 'vue', 'xml', 'yaml', 'yml', 'zsh'
])

const executableExtensions = new Set([
  'app', 'apk', 'bat', 'bin', 'cmd', 'com', 'deb', 'dmg', 'exe', 'msi', 'pkg', 'ps1', 'run', 'scr'
])

const compoundExtensions = ['tar.gz', 'tar.xz', 'tar.bz2', 'tar.zst']

function getNormalizedExtension(fileName: string) {
  const lowerName = fileName.trim().toLowerCase()
  if (!lowerName || lowerName === '.' || lowerName === '..') {
    return ''
  }

  for (const extension of compoundExtensions) {
    if (lowerName.endsWith(`.${extension}`)) {
      return extension
    }
  }

  const segments = lowerName.split('.')
  return segments.length > 1 ? segments.at(-1) ?? '' : ''
}

function getFileKindCategory(extension: string) {
  if (!extension) {
    return 'generic'
  }
  if (archiveExtensions.has(extension)) {
    return extension === 'iso' ? 'disk' : 'archive'
  }
  if (videoExtensions.has(extension)) {
    return 'video'
  }
  if (audioExtensions.has(extension)) {
    return 'audio'
  }
  if (imageExtensions.has(extension)) {
    return 'image'
  }
  if (spreadsheetExtensions.has(extension)) {
    return 'sheet'
  }
  if (presentationExtensions.has(extension)) {
    return 'slides'
  }
  if (documentExtensions.has(extension)) {
    return extension === 'pdf' ? 'pdf' : 'document'
  }
  if (codeExtensions.has(extension)) {
    return 'code'
  }
  if (executableExtensions.has(extension)) {
    return 'executable'
  }
  return 'generic'
}

export function getDisplayFileTypeLabel(row: FileRow) {
  if (row.type === 'folder') {
    return t.folder
  }

  const extension = getNormalizedExtension(row.name)
  return extension ? extension.toUpperCase() : t.file
}

export function getDisplayFileTypeSortKey(row: FileRow) {
  if (row.type === 'folder') {
    return `0:${t.folder}`
  }

  const label = getDisplayFileTypeLabel(row)
  const extension = getNormalizedExtension(row.name)
  const category = getFileKindCategory(extension)
  return `1:${category}:${label}`
}

export function getDisplayFileIconName(row: FileRow): AppIconName {
  if (row.type === 'folder') {
    return 'folder'
  }

  const extension = getNormalizedExtension(row.name)
  const category = getFileKindCategory(extension)

  switch (category) {
    case 'archive':
      return 'archive'
    case 'video':
      return 'video'
    case 'audio':
      return 'audio'
    case 'image':
      return 'image'
    case 'document':
    case 'sheet':
    case 'slides':
      return 'document'
    case 'pdf':
      return 'pdf'
    case 'code':
      return 'code'
    case 'disk':
      return 'disk'
    case 'executable':
      return 'flash'
    default:
      return 'file'
  }
}
