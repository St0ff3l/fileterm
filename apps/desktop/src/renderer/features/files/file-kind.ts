import type { LocalFileItem, RemoteFileItem } from '@fileterm/core'
import { t } from '../../i18n'
import type { AppIconName } from '../common/AppIcon'

type FileRow = Pick<LocalFileItem | RemoteFileItem, 'name' | 'type'>

const archiveExtensions = new Set([
  '7z',
  'bz2',
  'cab',
  'gz',
  'iso',
  'lz',
  'lz4',
  'rar',
  'tar',
  'tgz',
  'txz',
  'xz',
  'zip',
  'zst'
])

const videoExtensions = new Set([
  'avi',
  'flv',
  'm4v',
  'mkv',
  'mov',
  'mp4',
  'mpeg',
  'mpg',
  'rm',
  'rmvb',
  'ts',
  'webm',
  'wmv'
])

const audioExtensions = new Set([
  'aac',
  'aiff',
  'alac',
  'amr',
  'ape',
  'flac',
  'm4a',
  'mid',
  'midi',
  'mp3',
  'ogg',
  'opus',
  'wav',
  'wma'
])

const imageExtensions = new Set([
  'avif',
  'bmp',
  'gif',
  'heic',
  'ico',
  'jpeg',
  'jpg',
  'png',
  'psd',
  'svg',
  'tif',
  'tiff',
  'webp'
])

const documentExtensions = new Set([
  'azw',
  'azw3',
  'doc',
  'docm',
  'docx',
  'epub',
  'md',
  'mdx',
  'mobi',
  'odt',
  'pages',
  'pdf',
  'rtf',
  'txt'
])

const spreadsheetExtensions = new Set(['csv', 'numbers', 'ods', 'tsv', 'xls', 'xlsb', 'xlsm', 'xlsx'])

const presentationExtensions = new Set(['key', 'odp', 'potx', 'pps', 'ppsx', 'ppt', 'pptm', 'pptx'])

const codeExtensions = new Set([
  'c',
  'cc',
  'cpp',
  'cs',
  'css',
  'go',
  'h',
  'hpp',
  'htm',
  'html',
  'java',
  'js',
  'json',
  'jsx',
  'kt',
  'less',
  'log',
  'lua',
  'mjs',
  'php',
  'pl',
  'py',
  'rb',
  'rs',
  'sass',
  'scss',
  'sql',
  'swift',
  'ts',
  'tsx',
  'vue',
  'xml'
])

const configExtensions = new Set([
  'cfg',
  'conf',
  'editorconfig',
  'env',
  'gitattributes',
  'gitignore',
  'ini',
  'lock',
  'npmrc',
  'properties',
  'toml',
  'yaml',
  'yml'
])

const databaseExtensions = new Set(['accdb', 'db', 'db3', 'mdb', 'sqlite', 'sqlite3'])

const fontExtensions = new Set(['eot', 'otf', 'ttc', 'ttf', 'woff', 'woff2'])

const packageExtensions = new Set(['apk', 'app', 'deb', 'dmg', 'msi', 'pkg'])

const scriptExtensions = new Set(['bash', 'bat', 'cmd', 'command', 'fish', 'ps1', 'sh', 'zsh'])

const executableExtensions = new Set(['bin', 'com', 'exe', 'run', 'scr', 'wasm'])

const compoundExtensions = ['tar.gz', 'tar.xz', 'tar.bz2', 'tar.zst']
const codeFileNames = new Set(['cmakelists.txt', 'dockerfile', 'jenkinsfile', 'makefile', 'rakefile'])

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
  return segments.length > 1 ? (segments.at(-1) ?? '') : ''
}

function getFileKindCategory(fileName: string, extension: string) {
  const lowerName = fileName.trim().toLowerCase()
  if (codeFileNames.has(lowerName)) {
    return 'code'
  }
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
  if (configExtensions.has(extension)) {
    return 'config'
  }
  if (databaseExtensions.has(extension)) {
    return 'database'
  }
  if (fontExtensions.has(extension)) {
    return 'font'
  }
  if (packageExtensions.has(extension)) {
    return 'package'
  }
  if (documentExtensions.has(extension)) {
    return extension === 'pdf' ? 'pdf' : 'document'
  }
  if (scriptExtensions.has(extension) || executableExtensions.has(extension)) {
    return 'executable'
  }
  if (codeExtensions.has(extension)) {
    return 'code'
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
  const category = getFileKindCategory(row.name, extension)
  return `1:${category}:${label}`
}

export function getDisplayFileIconName(row: FileRow): AppIconName {
  if (row.type === 'folder') {
    return 'folder'
  }

  const extension = getNormalizedExtension(row.name)
  const category = getFileKindCategory(row.name, extension)

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
      return 'document'
    case 'sheet':
      return 'spreadsheet'
    case 'slides':
      return 'presentation'
    case 'config':
      return 'config-file'
    case 'database':
      return 'database'
    case 'font':
      return 'font-file'
    case 'package':
      return 'package'
    case 'pdf':
      return 'pdf'
    case 'code':
      return 'code'
    case 'disk':
      return 'disk'
    case 'executable':
      return 'terminal-file'
    default:
      return 'file'
  }
}
