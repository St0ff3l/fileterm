import { mkdir, readFile, rename, unlink, writeFile } from 'node:fs/promises'
import path from 'node:path'
import type { TransferManifest, TransferTask } from '@fileterm/core'

type StoredTransferJournal = {
  version: 1
  transfers: TransferTask[]
}

const EMPTY_JOURNAL: StoredTransferJournal = {
  version: 1,
  transfers: []
}

const TRANSFER_STATUSES = new Set<TransferTask['status']>([
  'queued',
  'running',
  'paused',
  'interrupted',
  'verifying',
  'finalizing',
  'done',
  'failed',
  'canceled'
])

export class TransferJournal {
  private readonly filePath: string
  private readonly tempPath: string
  private readonly backupPath: string
  private writeQueue: Promise<void> = Promise.resolve()

  constructor(baseDir: string) {
    this.filePath = path.join(baseDir, 'transfer-journal.json')
    this.tempPath = `${this.filePath}.tmp`
    this.backupPath = `${this.filePath}.bak`
  }

  async load(): Promise<TransferTask[]> {
    await mkdir(path.dirname(this.filePath), { recursive: true })
    const journal =
      (await this.readJournal(this.filePath)) ?? (await this.readJournal(this.backupPath)) ?? EMPTY_JOURNAL
    return journal.transfers.filter(isStoredTransferTask).map((transfer) => normalizeRestoredTransfer(transfer))
  }

  save(transfers: TransferTask[]): Promise<void> {
    const snapshot: StoredTransferJournal = {
      version: 1,
      transfers: transfers.slice(0, 200)
    }
    const operation = this.writeQueue.catch(() => undefined).then(() => this.writeJournal(snapshot))
    this.writeQueue = operation
    return operation
  }

  async flush(): Promise<void> {
    await this.writeQueue
  }

  private async readJournal(targetPath: string): Promise<StoredTransferJournal | null> {
    try {
      const parsed = JSON.parse(await readFile(targetPath, 'utf8')) as Partial<StoredTransferJournal>
      if (parsed.version !== 1 || !Array.isArray(parsed.transfers)) {
        return null
      }
      return {
        version: 1,
        transfers: parsed.transfers
      }
    } catch {
      return null
    }
  }

  private async writeJournal(journal: StoredTransferJournal): Promise<void> {
    await mkdir(path.dirname(this.filePath), { recursive: true })
    await writeFile(this.tempPath, JSON.stringify(journal, null, 2), 'utf8')
    await unlink(this.backupPath).catch(() => undefined)

    let movedCurrent = false
    try {
      await rename(this.filePath, this.backupPath)
      movedCurrent = true
    } catch {
      // The first write has no current journal to preserve.
    }

    try {
      await rename(this.tempPath, this.filePath)
      await unlink(this.backupPath).catch(() => undefined)
    } catch (error) {
      if (movedCurrent) {
        await rename(this.backupPath, this.filePath).catch(() => undefined)
      }
      throw error
    }
  }
}

function normalizeRestoredTransfer(transfer: TransferTask): TransferTask {
  const wasActive =
    transfer.status === 'queued' ||
    transfer.status === 'running' ||
    transfer.status === 'verifying' ||
    transfer.status === 'finalizing'

  const manifest = isValidTransferManifest(transfer.manifest)
    ? {
        ...transfer.manifest,
        files: transfer.manifest.files.map((file) => ({
          ...file,
          status: file.status === 'running' ? ('pending' as const) : file.status
        }))
      }
    : undefined
  const hasFileResumeMetadata =
    transfer.targetType === 'file' && Boolean(transfer.sourcePath && transfer.destinationPath && transfer.partialPath)
  const hasDirectoryResumeMetadata =
    transfer.targetType === 'folder' && Boolean(transfer.sourcePath && transfer.destinationPath && manifest)
  const isResumable = Boolean(
    transfer.resumable && transfer.profileId && (hasFileResumeMetadata || hasDirectoryResumeMetadata)
  )
  const wasInterrupted = transfer.status === 'interrupted' && isResumable

  return {
    ...transfer,
    manifest,
    status: wasActive ? (isResumable ? 'paused' : 'canceled') : wasInterrupted ? 'paused' : transfer.status,
    message: wasActive ? (isResumable ? '应用退出前传输未完成，可手动继续' : '应用退出前传输未完成') : transfer.message,
    speed: undefined,
    resumable: isResumable
  }
}

function isStoredTransferTask(value: unknown): value is TransferTask {
  if (!value || typeof value !== 'object') {
    return false
  }
  const task = value as Partial<TransferTask>
  return (
    typeof task.id === 'string' &&
    (task.direction === 'upload' || task.direction === 'download') &&
    typeof task.name === 'string' &&
    typeof task.progress === 'number' &&
    Number.isFinite(task.progress) &&
    task.progress >= 0 &&
    task.progress <= 100 &&
    typeof task.status === 'string' &&
    TRANSFER_STATUSES.has(task.status as TransferTask['status'])
  )
}

function isValidTransferManifest(value: unknown): value is TransferManifest {
  if (!value || typeof value !== 'object') {
    return false
  }
  const manifest = value as Partial<TransferManifest>
  return (
    manifest.version === 1 &&
    Array.isArray(manifest.directories) &&
    manifest.directories.every((entry) => typeof entry === 'string') &&
    Array.isArray(manifest.files) &&
    manifest.files.every((entry) =>
      Boolean(
        entry &&
        typeof entry.relativePath === 'string' &&
        typeof entry.sourcePath === 'string' &&
        typeof entry.destinationPath === 'string' &&
        typeof entry.partialPath === 'string' &&
        (entry.stagingPath === undefined || typeof entry.stagingPath === 'string') &&
        typeof entry.sourceIdentity?.size === 'number' &&
        Number.isFinite(entry.sourceIdentity.size) &&
        entry.sourceIdentity.size >= 0 &&
        (entry.status === 'pending' || entry.status === 'running' || entry.status === 'done') &&
        typeof entry.transferredBytes === 'number' &&
        Number.isFinite(entry.transferredBytes) &&
        entry.transferredBytes >= 0
      )
    )
  )
}
