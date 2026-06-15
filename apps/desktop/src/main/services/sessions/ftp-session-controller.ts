import { randomUUID } from 'node:crypto'
import { readFile, stat, unlink, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { Client as BasicFtpClient, FileInfo, FileType } from 'basic-ftp'
import type { FtpProfile, FtpSessionController, PermissionChangeOptions, RemoteFileItem, TransferProgress } from '@termdock/core'
import { BaseFileSessionController } from './base-file-session-controller.js'
import { parentRemotePath, toResolvedFtpRemoteFileItem } from './session-file-utils.js'
import { decodeBuffer, encodeText } from '../text-encoding.js'
import { appLog, appWarn } from '../app-logger.js'

export class LiveFtpSessionController extends BaseFileSessionController implements FtpSessionController {
  readonly type = 'ftp'

  private readonly ftp = new BasicFtpClient(20000)
  private readonly entryDebugInfo = new Map<string, string>()
  private readonly resolvedEntryTypes = new Map<string, RemoteFileItem['type']>()
  private readonly defaultParseList: (rawList: string) => FileInfo[]
  private listingMode: 'auto' | 'classic-list' = 'auto'
  private mlstMode: 'auto' | 'disabled' = 'auto'
  private sizeMode: 'auto' | 'disabled' = 'auto'
  private currentRemotePath: string
  private operationQueue: Promise<unknown> = Promise.resolve()

  constructor(id: string, profile: FtpProfile) {
    super(id, 'ftp', profile)
    this.currentRemotePath = profile.remotePath || '/'
    this.defaultParseList = this.ftp.parseList.bind(this.ftp)
    this.ftp.parseList = (rawList: string) => enrichFtpListing(this.defaultParseList(rawList), rawList)
  }

  override async connect(): Promise<void> {
    await this.runSerialized(async () => {
      if (this.connected) {
        return
      }

      await this.connectInternal()
    })
  }

  override async disconnect(): Promise<void> {
    await this.runSerialized(async () => {
      this.disconnectInternal()
    })
  }

  override getRemotePath(): string {
    return this.currentRemotePath
  }

  async abortTransfer(): Promise<void> {
    await this.runSerialized(async () => {
      this.disconnectInternal()
    })
  }

  async listRemoteFiles(): Promise<RemoteFileItem[]> {
    return this.runWithConnectedClient(() => this.readRemoteDirectory(this.currentRemotePath))
  }

  async openRemotePath(nextPath: string): Promise<RemoteFileItem[]> {
    return this.runWithConnectedClient(async () => {
      try {
        await this.ftp.cd(nextPath)
      } catch (error) {
        const detail = this.entryDebugInfo.get(nextPath)
        const enriched = `${error instanceof Error ? error.message : String(error)}${detail ? ` [ftp-entry: ${detail}]` : ''}`
        appWarn(`[TermDock][FTP] Failed to open remote path ${nextPath}: ${enriched}`)
        throw new Error(enriched)
      }
      this.resolvedEntryTypes.set(nextPath, 'folder')
      this.currentRemotePath = await this.ftp.pwd()
      return this.readRemoteDirectory(this.currentRemotePath)
    })
  }

  async readRemoteFile(targetPath: string, encoding = 'utf-8'): Promise<string> {
    return this.runWithConnectedClient(async () => {
      const localPath = this.tempFilePath(targetPath)
      try {
        await this.ftp.downloadTo(localPath, targetPath)
        this.resolvedEntryTypes.set(targetPath, 'file')
        return decodeBuffer(await readFile(localPath), encoding)
      } finally {
        void unlink(localPath).catch(() => undefined)
      }
    })
  }

  async writeRemoteFile(targetPath: string, content: string, encoding = 'utf-8'): Promise<void> {
    await this.runWithConnectedClient(async () => {
      const localPath = this.tempFilePath(targetPath)
      try {
        await writeFile(localPath, encodeText(content, encoding))
        await this.ensureRemoteDirectoryInternal(path.posix.dirname(targetPath))
        await this.ftp.uploadFrom(localPath, targetPath)
        this.resolvedEntryTypes.set(targetPath, 'file')
      } finally {
        void unlink(localPath).catch(() => undefined)
      }
    })
  }

  async copyRemotePath(_targetPath: string, _destinationPath: string, _targetType: RemoteFileItem['type']): Promise<void> {
    throw new Error('FTP 暂不支持服务器内复制，请改用下载后上传')
  }

  async moveRemotePath(targetPath: string, destinationPath: string): Promise<void> {
    await this.renameRemotePath(targetPath, destinationPath)
  }

  async renameRemotePath(targetPath: string, nextPath: string): Promise<void> {
    await this.runWithConnectedClient(async () => {
      await this.ftp.rename(targetPath, nextPath)
      const knownType = this.resolvedEntryTypes.get(targetPath)
      this.resolvedEntryTypes.delete(targetPath)
      if (knownType) {
        this.resolvedEntryTypes.set(nextPath, knownType)
      }
    })
  }

  async deleteRemotePath(targetPath: string, targetType: RemoteFileItem['type']): Promise<void> {
    await this.runWithConnectedClient(async () => {
      if (targetType === 'folder') {
        await this.ftp.removeDir(targetPath)
        this.resolvedEntryTypes.delete(targetPath)
        return
      }
      await this.ftp.remove(targetPath)
      this.resolvedEntryTypes.delete(targetPath)
    })
  }

  async changeRemotePermissions(targetPath: string, options: PermissionChangeOptions): Promise<void> {
    validateMode(options.mode)
    await this.runWithConnectedClient(async () => {
      if (options.recursive) {
        throw new Error('FTP 暂不支持递归修改权限')
      }
      await this.ftp.send(`SITE CHMOD ${options.mode.trim()} ${targetPath}`)
    })
  }

  async ensureRemoteDirectory(targetPath: string): Promise<void> {
    await this.runWithConnectedClient(async () => {
      await this.ensureRemoteDirectoryInternal(targetPath)
      this.resolvedEntryTypes.set(targetPath, 'folder')
    })
  }

  async uploadFile(localPath: string, remotePath: string, onProgress: (progress: TransferProgress) => void): Promise<void> {
    const info = await stat(localPath)
    const total = Math.max(info.size, 1)
    appLog(`[TermDock][FTP] Upload start ${localPath} -> ${remotePath} (${formatTransferBytes(total)})`)
    try {
      await this.runWithConnectedClient(async () => {
        this.ftp.trackProgress((progress) => {
          onProgress({
            percent: Math.min(99, Math.round((progress.bytes / total) * 100)),
            transferredBytes: progress.bytes,
            totalBytes: total
          })
        })
        try {
          await this.ensureRemoteDirectoryInternal(path.posix.dirname(remotePath))
          await this.ftp.uploadFrom(localPath, remotePath)
          await this.verifyRemoteFileSize(remotePath, total)
          this.resolvedEntryTypes.set(remotePath, 'file')
          appLog(`[TermDock][FTP] Upload verified ${remotePath} (${formatTransferBytes(total)})`)
          onProgress({ percent: 100, transferredBytes: total, totalBytes: total })
        } finally {
          this.ftp.trackProgress()
        }
      })
    } catch (error) {
      appWarn(`[TermDock][FTP] Upload failed ${localPath} -> ${remotePath}`, error)
      throw error
    }
  }

  async downloadFile(remotePath: string, localPath: string, onProgress: (progress: TransferProgress) => void): Promise<void> {
    try {
      await this.runWithConnectedClient(async () => {
        const total = Math.max(await this.ftp.size(remotePath), 1)
        appLog(`[TermDock][FTP] Download start ${remotePath} -> ${localPath} (${formatTransferBytes(total)})`)
        this.ftp.trackProgress((progress) => {
          onProgress({
            percent: Math.min(99, Math.round((progress.bytes / total) * 100)),
            transferredBytes: progress.bytes,
            totalBytes: total
          })
        })
        try {
          await this.ftp.downloadTo(localPath, remotePath)
          const localInfo = await stat(localPath)
          assertTransferSize(localPath, Math.max(localInfo.size, 1), total)
          appLog(`[TermDock][FTP] Download verified ${remotePath} -> ${localPath} (${formatTransferBytes(total)})`)
          onProgress({ percent: 100, transferredBytes: total, totalBytes: total })
        } finally {
          this.ftp.trackProgress()
        }
      })
    } catch (error) {
      appWarn(`[TermDock][FTP] Download failed ${remotePath} -> ${localPath}`, error)
      throw error
    }
  }

  private async connectInternal(): Promise<void> {
    const profile = this.profile as FtpProfile
    await this.ftp.access({
      host: profile.host,
      port: profile.port,
      user: profile.username,
      password: profile.password,
      secure: profile.secure
    })
    this.connected = true
    try {
      await this.ftp.cd(this.currentRemotePath)
      this.currentRemotePath = await this.ftp.pwd()
    } catch {
      this.currentRemotePath = profile.remotePath || '/'
    }
  }

  private disconnectInternal() {
    this.ftp.close()
    this.connected = false
  }

  private async readRemoteDirectory(targetPath: string): Promise<RemoteFileItem[]> {
    const entries = await this.listRemoteDirectoryEntries(targetPath)
    const previousPath = await this.ftp.pwd()
    const rows = entries
      .filter((entry) => entry.name !== '.' && entry.name !== '..')
    logFtpListingAlignment(targetPath, rows)
    appLog(`[TermDock][FTP] Listing remote directory ${targetPath} (${rows.length} entries)`)
    const items: RemoteFileItem[] = []

    for (const entry of rows) {
      const isDirectory = await this.resolveDirectoryFlag(targetPath, entry, previousPath)
      const item = toResolvedFtpRemoteFileItem(targetPath, entry, isDirectory)
      const debugInfo = describeFtpEntry(targetPath, entry, isDirectory)
      this.entryDebugInfo.set(item.path, debugInfo)
      if ((entry as FileInfoWithRaw).rawLine || entry.type === FileType.Unknown || isDirectory !== entry.isDirectory) {
        appLog(`[TermDock][FTP] Resolved remote entry: ${debugInfo}`)
      }
      items.push(item)
    }

    items
      .sort((left, right) => {
        if (left.type !== right.type) {
          return left.type === 'folder' ? -1 : 1
        }
        return left.name.localeCompare(right.name)
      })

    if (targetPath !== '/') {
      items.unshift({
        path: parentRemotePath(targetPath),
        name: '..',
        type: 'folder',
        modified: '',
        size: '-',
        permission: '',
        ownerGroup: ''
      })
    }

    return items
  }

  private tempFilePath(remotePath: string) {
    return path.join(os.tmpdir(), `termdock-${randomUUID()}-${path.posix.basename(remotePath) || 'remote-file'}`)
  }

  private async ensureConnected() {
    if (!this.connected) {
      await this.connectInternal()
    }
  }

  private async ensureRemoteDirectoryInternal(targetPath: string) {
    if (!targetPath || targetPath === '.') {
      return
    }

    const previousPath = await this.ftp.pwd()
    try {
      await this.ftp.ensureDir(targetPath)
    } finally {
      await this.ftp.cd(previousPath)
    }
  }

  private async runWithConnectedClient<T>(operation: () => Promise<T>): Promise<T> {
    return this.runSerialized(async () => {
      await this.ensureConnected()
      return operation()
    })
  }

  private async runSerialized<T>(operation: () => Promise<T>): Promise<T> {
    const nextOperation = this.operationQueue.catch(() => undefined).then(operation)
    this.operationQueue = nextOperation.then(() => undefined, () => undefined)
    return nextOperation
  }

  private async listRemoteDirectoryEntries(targetPath: string) {
    if (this.listingMode === 'classic-list') {
      this.setClassicListCommands()
      return this.ftp.list(targetPath)
    }

    const initialEntries = await this.ftp.list(targetPath)
    if (!shouldRetryWithClassicList(initialEntries)) {
      return initialEntries
    }

    this.listingMode = 'classic-list'
    this.mlstMode = 'disabled'
    appLog(`[TermDock][FTP] Switching listing mode to classic LIST for current session: ${targetPath}`)
    this.setClassicListCommands()
    const retriedEntries = await this.ftp.list(targetPath)
    return retriedEntries
  }

  private setClassicListCommands() {
    const ftpWithListCommands = this.ftp as BasicFtpClient & { availableListCommands?: string[] }
    ftpWithListCommands.availableListCommands = ['LIST -a', 'LIST']
  }

  private async resolveDirectoryFlag(targetPath: string, entry: FileInfo, previousPath: string) {
    const candidatePath = path.posix.join(targetPath, entry.name)
    const cachedType = this.resolvedEntryTypes.get(candidatePath)
    if (cachedType === 'folder') {
      entry.type = FileType.Directory
      return true
    }
    if (cachedType === 'file') {
      entry.type = FileType.File
      return false
    }

    if (entry.type === FileType.Directory || entry.isDirectory) {
      entry.type = FileType.Directory
      this.resolvedEntryTypes.set(candidatePath, 'folder')
      return true
    }

    if (entry.type !== FileType.Unknown) {
      entry.type = FileType.File
      this.resolvedEntryTypes.set(candidatePath, 'file')
      return false
    }

    if (this.mlstMode === 'auto') {
      const mlsdResolved = await this.tryResolveTypeWithMlst(candidatePath)
      if (mlsdResolved) {
        entry.type = mlsdResolved === 'folder' ? FileType.Directory : FileType.File
        this.resolvedEntryTypes.set(candidatePath, mlsdResolved)
        return mlsdResolved === 'folder'
      }
    }

    if (this.sizeMode === 'auto') {
      const sizeResolved = await this.tryResolveTypeWithSize(candidatePath, entry)
      if (sizeResolved) {
        entry.type = FileType.File
        this.resolvedEntryTypes.set(candidatePath, 'file')
        return false
      }
    }

    try {
      await this.ftp.cd(candidatePath)
      entry.type = FileType.Directory
      this.resolvedEntryTypes.set(candidatePath, 'folder')
      appLog(`[TermDock][FTP] Directory probe succeeded for ${candidatePath}`)
      return true
    } catch {
      entry.type = FileType.File
      this.resolvedEntryTypes.set(candidatePath, 'file')
      return false
    } finally {
      await this.ftp.cd(previousPath).catch(() => undefined)
    }
  }

  private async tryResolveTypeWithMlst(targetPath: string): Promise<RemoteFileItem['type'] | null> {
    try {
      const response = await this.ftp.sendIgnoringError(`MLST ${await this.protectFtpPath(targetPath)}`)
      if (response.code < 200 || response.code >= 300) {
        this.mlstMode = 'disabled'
        return null
      }

      const factLine = response.message
        .split(/\r?\n/)
        .map((line) => line.trim())
        .find((line) => /^\S+=\S+;/.test(line))
      if (!factLine) {
        this.mlstMode = 'disabled'
        return null
      }

      const parsed = new FileInfo(path.posix.basename(targetPath))
      enrichFromMlsdLine(parsed as FileInfoWithRaw, factLine)
      if (parsed.type === FileType.Directory) {
        appLog(`[TermDock][FTP] MLST resolved directory for ${targetPath}`)
        return 'folder'
      }
      if (parsed.type === FileType.File || parsed.type === FileType.SymbolicLink) {
        appLog(`[TermDock][FTP] MLST resolved file for ${targetPath}`)
        return 'file'
      }
      this.mlstMode = 'disabled'
      return null
    } catch {
      this.mlstMode = 'disabled'
      return null
    }
  }

  private async tryResolveTypeWithSize(targetPath: string, entry: FileInfo): Promise<boolean> {
    try {
      const response = await this.ftp.sendIgnoringError(`SIZE ${await this.protectFtpPath(targetPath)}`)
      if (response.code >= 200 && response.code < 300) {
        const size = Number.parseInt(response.message.slice(4), 10)
        if (Number.isFinite(size)) {
          entry.size = size
        }
        appLog(`[TermDock][FTP] SIZE resolved file for ${targetPath}`)
        return true
      }

      if (isUnsupportedFtpCommand(response.code)) {
        this.sizeMode = 'disabled'
      }
      return false
    } catch {
      this.sizeMode = 'disabled'
      return false
    }
  }

  private async protectFtpPath(targetPath: string) {
    const ftpWithWhitespaceGuard = this.ftp as BasicFtpClient & {
      protectWhitespace?(path: string): Promise<string>
    }
    return ftpWithWhitespaceGuard.protectWhitespace
      ? ftpWithWhitespaceGuard.protectWhitespace(targetPath)
      : targetPath
  }

  private async verifyRemoteFileSize(remotePath: string, expectedSize: number): Promise<void> {
    const remoteSize = await this.readRemoteFileSize(remotePath)
    if (remoteSize === undefined) {
      appWarn(`[TermDock][FTP] Upload size verification skipped for ${remotePath}; remote SIZE/listing did not expose a file size`)
      return
    }
    assertTransferSize(remotePath, remoteSize, expectedSize)
  }

  private async readRemoteFileSize(remotePath: string): Promise<number | undefined> {
    try {
      return Math.max(await this.ftp.size(remotePath), 0)
    } catch (error) {
      appWarn(`[TermDock][FTP] SIZE failed for ${remotePath}, trying directory listing`, error)
    }

    try {
      const entries = await this.listRemoteDirectoryEntries(path.posix.dirname(remotePath))
      const match = entries.find((entry) => entry.name === path.posix.basename(remotePath))
      return typeof match?.size === 'number' && Number.isFinite(match.size)
        ? Math.max(match.size, 0)
        : undefined
    } catch (error) {
      appWarn(`[TermDock][FTP] Directory listing size probe failed for ${remotePath}`, error)
      return undefined
    }
  }
}

function shouldRetryWithClassicList(entries: FileInfo[]) {
  if (!entries.length) {
    return false
  }

  return entries.every((entry) => {
    const rawLine = (entry as FileInfoWithRaw).rawLine?.trim()
    if (entry.type !== FileType.Unknown || !rawLine) {
      return false
    }
    return !looksLikeStructuredFtpLine(rawLine)
  })
}

function looksLikeStructuredFtpLine(rawLine: string) {
  return /^\S+=\S+;/.test(rawLine)
    || /^\d{2}-\d{2}-\d{2}\s+\d{2}:\d{2}(AM|PM)/i.test(rawLine)
    || /^[\-ldpscbD]/.test(rawLine)
}

function isUnsupportedFtpCommand(code: number) {
  return code === 500 || code === 501 || code === 502 || code === 504
}

function validateMode(mode: string) {
  if (!/^[0-7]{3,4}$/.test(mode.trim())) {
    throw new Error('权限值必须是 3 到 4 位八进制数字，例如 755')
  }
}

function describeFtpEntry(basePath: string, entry: FileInfo, isDirectory: boolean) {
  const targetPath = path.posix.join(basePath, entry.name)
  const rawType = FileType[entry.type] ?? `Unknown(${entry.type})`
  return [
    `path=${targetPath}`,
    `name=${entry.name}`,
    `rawType=${rawType}`,
    `resolvedType=${isDirectory ? 'Directory' : 'File'}`,
    `isDirectoryFlag=${entry.isDirectory ? 'true' : 'false'}`,
    `size=${entry.size}`,
    `permissions=${formatPermissionsForDebug(entry)}`,
    `owner=${entry.user || '-'}`,
    `group=${entry.group || '-'}`,
    `modified=${entry.modifiedAt?.toISOString?.() ?? (entry.rawModifiedAt || '-')}`,
    `rawLine=${(entry as FileInfoWithRaw).rawLine ?? '-'}`
  ].join(', ')
}

function formatPermissionsForDebug(entry: FileInfo) {
  if (!entry.permissions) {
    return '-'
  }

  return `u:${entry.permissions.user ?? 0},g:${entry.permissions.group ?? 0},w:${entry.permissions.world ?? 0}`
}

function assertTransferSize(targetPath: string, actualSize: number | undefined, expectedSize: number): void {
  if (actualSize === expectedSize) {
    return
  }

  const actual = typeof actualSize === 'number' ? formatTransferBytes(actualSize) : '未知大小'
  throw new Error(`传输校验失败：${path.posix.basename(targetPath)} 实际为 ${actual}，期望 ${formatTransferBytes(expectedSize)}`)
}

function formatTransferBytes(size: number) {
  if (!size) {
    return '0 B'
  }

  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let value = size
  let unitIndex = 0
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex += 1
  }

  const digits = value >= 10 || unitIndex === 0 ? 0 : 1
  return `${value.toFixed(digits)} ${units[unitIndex]}`
}

type FileInfoWithRaw = FileInfo & {
  rawLine?: string
  rawName?: string
  rawIndex?: number
}

function enrichFtpListing(files: FileInfo[], rawList: string) {
  const lines = rawList
    .split(/\r?\n/)
    .map((line) => line.trimEnd())
    .filter((line) => line.trim() !== '')
    .filter((line) => !line.startsWith('total'))
    .filter((line) => !/type=(cdir|pdir)/i.test(line))
    .map((line, index) => ({
      rawLine: line,
      rawName: extractRawEntryName(line),
      rawIndex: index
    }))
    .filter((entry) => {
      return entry.rawName !== '.' && entry.rawName !== '..'
    })

  return files.map((file, index) => {
    const enriched = file as FileInfoWithRaw
    const rawEntry = findMatchingRawLine(lines, file.name, index)
    if (!rawEntry) {
      return enriched
    }

    enriched.rawLine = rawEntry.rawLine
    enriched.rawName = rawEntry.rawName
    enriched.rawIndex = rawEntry.rawIndex
    enrichFromRawLine(enriched, rawEntry.rawLine)
    return enriched
  })
}

function findMatchingRawLine(
  lines: Array<{ rawLine: string; rawName: string; rawIndex: number }>,
  fileName: string,
  fallbackIndex: number
) {
  for (let index = fallbackIndex; index < lines.length; index += 1) {
    const line = lines[index]
    if (line.rawName === fileName) {
      return line
    }
  }

  return lines[fallbackIndex]
}

function logFtpListingAlignment(targetPath: string, entries: FileInfo[]) {
  const parsedNames = entries.map((entry) => entry.name)
  const rawNames = entries.map((entry) => (entry as FileInfoWithRaw).rawName ?? '?')
  const mismatches = entries
    .map((entry, index) => ({
      index,
      parsedName: entry.name,
      rawName: (entry as FileInfoWithRaw).rawName ?? '?',
      rawIndex: (entry as FileInfoWithRaw).rawIndex ?? -1
    }))
    .filter((item) => item.parsedName !== item.rawName)

  if (!mismatches.length) {
    return
  }

  const mismatchSummary = mismatches
    .slice(0, 8)
    .map((item) => `#${item.index}:${item.parsedName}<-${item.rawName}@${item.rawIndex}`)
    .join(' | ')

  appWarn(`[TermDock][FTP] Listing alignment mismatch detected for ${targetPath}: ${mismatchSummary}`)
  appWarn(`[TermDock][FTP] Parsed listing sequence for ${targetPath}: ${parsedNames.join(' | ')}`)
  appWarn(`[TermDock][FTP] Raw listing sequence for ${targetPath}: ${rawNames.join(' | ')}`)
}

function extractRawEntryName(rawLine: string) {
  if (/^\S+=\S+;/.test(rawLine) || rawLine.startsWith(' ')) {
    const separatorIndex = rawLine.indexOf(' ')
    return separatorIndex >= 0 ? rawLine.slice(separatorIndex + 1).trim() : rawLine.trim()
  }

  const dosMatch = rawLine.match(/^\S+\s+\S+\s+(?:<DIR>|[0-9]+)\s+(.+)$/i)
  if (dosMatch?.[1]) {
    return dosMatch[1].trim()
  }

  const unixMatch = rawLine.match(/^[bcdelfmpSs-][rwxStTsL-]{9}\+?\s+\d+\s+(?:(?:\S+(?:\s+\S+)*)\s+)?(?:(?:\S+(?:\s+\S+)*)\s+)?(?:\d+(?:,\s*\d+)?)\s+(?:(?:\d+[-/]\d+[-/]\d+)|(?:\S{3}\s+\d{1,2})|(?:\d{1,2}\s+\S{3})|(?:\d{1,2}月\s+\d{1,2}日))\s+(?:(?:\d+(?::\d+)?)|(?:\d{4}年))\s+(.*)$/)
  if (unixMatch?.[1]) {
    return unixMatch[1].replace(/\s+->\s+.*$/, '').trim()
  }

  return rawLine.trim()
}

function enrichFromRawLine(file: FileInfoWithRaw, rawLine: string) {
  if (/^\S+=\S+;/.test(rawLine) || rawLine.startsWith(' ')) {
    enrichFromMlsdLine(file, rawLine)
    return
  }

  if (/^\d{2}-\d{2}-\d{2}\s+\d{2}:\d{2}(AM|PM)/i.test(rawLine)) {
    enrichFromDosLine(file, rawLine)
    return
  }

  enrichFromUnixLine(file, rawLine)
}

function enrichFromMlsdLine(file: FileInfoWithRaw, rawLine: string) {
  const separatorIndex = rawLine.indexOf(' ')
  if (separatorIndex < 0) {
    return
  }

  const facts = rawLine.slice(0, separatorIndex).split(';')
  for (const fact of facts) {
    const equalsIndex = fact.indexOf('=')
    if (equalsIndex < 0) {
      continue
    }

    const key = fact.slice(0, equalsIndex).toLowerCase()
    const value = fact.slice(equalsIndex + 1)

    if (key === 'type') {
      const normalized = value.toLowerCase()
      if (normalized === 'file') {
        file.type = FileType.File
      } else if (normalized === 'dir' || normalized === 'cdir' || normalized === 'pdir' || normalized === 'folder' || normalized === 'directory') {
        file.type = FileType.Directory
      } else if (normalized.includes('slink') || normalized.includes('symlink')) {
        file.type = FileType.SymbolicLink
      }
      continue
    }

    if (key === 'size' || key === 'sizd') {
      const size = Number.parseInt(value, 10)
      if (Number.isFinite(size)) {
        file.size = size
      }
      continue
    }

    if (key === 'unix.mode') {
      const digits = value.slice(-3)
      if (digits.length === 3) {
        file.permissions = {
          user: Number.parseInt(digits[0] ?? '0', 10),
          group: Number.parseInt(digits[1] ?? '0', 10),
          world: Number.parseInt(digits[2] ?? '0', 10)
        }
      }
    }
  }
}

function enrichFromDosLine(file: FileInfoWithRaw, rawLine: string) {
  const match = rawLine.match(/^\S+\s+\S+\s+(<DIR>|[0-9]+)\s+/i)
  if (!match) {
    return
  }

  if (match[1]?.toUpperCase() === '<DIR>') {
    file.type = FileType.Directory
    return
  }

  file.type = FileType.File
  const size = Number.parseInt(match[1] ?? '', 10)
  if (Number.isFinite(size)) {
    file.size = size
  }
}

function enrichFromUnixLine(file: FileInfoWithRaw, rawLine: string) {
  const typeToken = rawLine[0]
  if (typeToken === 'd') {
    file.type = FileType.Directory
  } else if (typeToken === 'l') {
    file.type = FileType.SymbolicLink
  } else if (typeToken === '-' || typeToken === 'f') {
    file.type = FileType.File
  }

  const parts = rawLine.trim().split(/\s+/)
  const sizeToken = parts[4]
  const size = Number.parseInt(sizeToken ?? '', 10)
  if (Number.isFinite(size)) {
    file.size = size
  }
}
