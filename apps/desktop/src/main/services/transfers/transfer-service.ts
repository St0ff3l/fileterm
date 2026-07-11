import { mkdir, readdir, stat } from 'node:fs/promises'
import path from 'node:path'
import type { WebContents } from 'electron'
import {
  type FileSessionController,
  type SessionSnapshot,
  type TransferManifest,
  type TransferManifestEntry,
  type TransferProgress,
  type TransferTargetOptions,
  type TransferTask,
  type WorkspaceTab,
  type WorkspaceSnapshot
} from '@fileterm/core'
import { appWarn } from '../app-logger.js'
import { seedTransfers } from '../workspace/seed-data.js'
import { WorkspaceTransfersState } from '../workspace/workspace-transfers.js'
import {
  localTransferPartialPath,
  removeLocalFileIfExists,
  remoteTransferPartialPath,
  replaceLocalFile,
  sameTransferIdentity,
  statLocalFile
} from './transfer-file-utils.js'
import type { TransferJournal } from './transfer-journal.js'
import {
  createTransferManifest,
  isTransferManifestComplete,
  transferManifestProgress,
  updateTransferManifestEntry
} from './transfer-manifest.js'
import {
  createTransferSpeedTracker,
  directoryProgressPercent,
  formatTransferByteCount,
  rootUploadStagingPath,
  withRootUploadStagingPaths
} from './transfer-runtime-utils.js'

const TRANSFER_UPDATE_INTERVAL_MS = 200

interface TransferTabs {
  list(): WorkspaceTab[]
  getById(tabId: string): WorkspaceTab | null
}

interface TransferSessionRuntime {
  requireController(tabId: string): FileSessionController
  getController(tabId: string): FileSessionController | undefined
  get(tabId: string): SessionSnapshot | undefined
  refreshRemoteFiles(tabId: string): Promise<void>
  emitToSender(sender: WebContents, channel: string, payload: unknown): void
  emitToTab(tabId: string, channel: string, payload: unknown): void
}

export interface TransferServiceOptions {
  tabs: TransferTabs
  sessionRuntime: TransferSessionRuntime
  getSnapshot(): Promise<WorkspaceSnapshot>
  getLocale?(): 'zhCN' | 'enUS'
  transferJournal?: TransferJournal
}

export class TransferService {
  private static readonly DISCONNECTED_TRANSFER_MESSAGES = {
    zhCN: '连接已断开，可在重连后继续传输',
    enUS: 'Connection closed. Reconnect to resume the transfer.'
  } as const

  private readonly transfers = new WorkspaceTransfersState(seedTransfers)
  private readonly transferCancels = new Map<string, () => Promise<void> | void>()
  private readonly transferCanceling = new Set<string>()
  private readonly transferTabs = new Map<string, string>()
  private readonly transferUpdateTimers = new Map<string, ReturnType<typeof setTimeout>>()
  private readonly transferStopReasons = new Map<string, 'pause' | 'discard'>()
  private readonly transferRuns = new Map<string, Promise<WorkspaceSnapshot>>()
  private readonly readyPromise: Promise<void>

  constructor(private readonly options: TransferServiceOptions) {
    this.readyPromise = this.restoreTransfers()
  }

  waitUntilReady(): Promise<void> {
    return this.readyPromise
  }

  list(): TransferTask[] {
    return this.transfers.list()
  }

  getDisconnectedTransferMessage(): string {
    const locale = this.options.getLocale?.() ?? 'zhCN'
    return TransferService.DISCONNECTED_TRANSFER_MESSAGES[locale]
  }

  async shutdown(): Promise<void> {
    await this.readyPromise
    await Promise.allSettled(
      this.options.tabs.list().map((tab) => this.finalizeTransfersForTab(tab.id, '应用退出时已暂停，可手动继续'))
    )
    for (const timer of this.transferUpdateTimers.values()) {
      clearTimeout(timer)
    }
    this.transferUpdateTimers.clear()
  }

  async flushJournal(): Promise<void> {
    await this.options.transferJournal?.flush()
  }

  async queueUpload(fileNames: string[]): Promise<WorkspaceSnapshot> {
    await this.readyPromise
    this.transfers.queueUploads(fileNames)
    this.persistTransfers()
    return this.options.getSnapshot()
  }

  async cancelTransfer(transferId: string, sender: WebContents): Promise<WorkspaceSnapshot> {
    return this.discardTransfer(transferId, sender)
  }

  async pauseTransfer(transferId: string, sender: WebContents): Promise<WorkspaceSnapshot> {
    await this.readyPromise
    if (this.transferCanceling.has(transferId)) {
      return this.options.getSnapshot()
    }

    const transfer = this.ensureRootUploadStagingPaths(transferId)
    if (!transfer || (transfer.status !== 'running' && transfer.status !== 'queued')) {
      return this.options.getSnapshot()
    }
    if (!transfer.resumable) {
      return this.discardTransfer(transferId, sender)
    }

    this.transferCanceling.add(transferId)
    this.transferStopReasons.set(transferId, 'pause')
    const cancel = this.transferCancels.get(transferId)
    this.transferCancels.delete(transferId)
    this.updateTransfer(
      transferId,
      {
        status: transfer.status,
        message: '正在暂停传输...',
        speed: undefined,
        resumable: Boolean(transfer.resumable)
      },
      sender
    )

    try {
      await this.invokeTransferCancel(cancel).catch(() => undefined)
      await this.transferRuns.get(transferId)?.catch(() => undefined)
    } finally {
      this.transferCanceling.delete(transferId)
    }

    await this.updateTransfer(
      transferId,
      {
        status: 'paused',
        message: '传输已暂停，可继续',
        speed: undefined,
        resumable: Boolean(transfer.resumable)
      },
      sender
    )

    return this.options.getSnapshot()
  }

  async discardTransfer(transferId: string, sender: WebContents): Promise<WorkspaceSnapshot> {
    await this.readyPromise
    const transfer = this.ensureRootUploadStagingPaths(transferId)
    if (!transfer) {
      return this.options.getSnapshot()
    }

    const isActive = transfer.status === 'running' || transfer.status === 'queued'
    if (isActive && !this.transferCanceling.has(transferId)) {
      this.transferCanceling.add(transferId)
      this.transferStopReasons.set(transferId, 'discard')
      const cancel = this.transferCancels.get(transferId)
      this.transferCancels.delete(transferId)
      this.updateTransfer(
        transferId,
        {
          status: 'canceled',
          message: '传输已取消，正在清理断点',
          speed: undefined,
          resumable: false
        },
        sender
      )
      try {
        await this.invokeTransferCancel(cancel)
        await this.transferRuns.get(transferId)?.catch(() => undefined)
      } finally {
        this.transferCanceling.delete(transferId)
      }
    }

    const cleanup = await this.removeTransferPartial(transfer)
    await this.updateTransfer(
      transferId,
      {
        status: 'canceled',
        message: cleanup.message,
        speed: undefined,
        resumable: false,
        cleanupPending: cleanup.pending
      },
      sender
    )
    this.transferStopReasons.delete(transferId)
    return this.options.getSnapshot()
  }

  async resumeTransfer(transferId: string, sender: WebContents): Promise<WorkspaceSnapshot> {
    await this.readyPromise
    if (this.transferCanceling.has(transferId)) {
      throw new Error('传输正在暂停，请稍候再继续')
    }
    const previousRun = this.transferRuns.get(transferId)
    if (previousRun) {
      await previousRun.catch(() => undefined)
    }
    const transfer = this.ensureRootUploadStagingPaths(transferId)
    if (!transfer?.resumable || !transfer.profileId) {
      throw new Error('该传输没有可用断点')
    }
    if (transfer.status === 'running' || transfer.status === 'queued') {
      return this.options.getSnapshot()
    }

    const tab = this.findConnectedTabForProfile(transfer.profileId)
    if (!tab) {
      throw new Error('请先打开并连接原传输使用的连接，再继续任务')
    }
    const controller = this.options.sessionRuntime.requireController(tab.id)
    if (transfer.fileAccessMode === 'root' && controller.getFileAccessMode() !== 'root') {
      throw new Error('该任务使用 root 文件模式创建，请先切换到 root 文件视角并完成授权')
    }

    this.transferTabs.set(transferId, tab.id)
    await this.updateTransfer(
      transferId,
      {
        tabId: tab.id,
        status: 'running',
        message: '正在检查断点...',
        speed: undefined
      },
      sender
    )
    return transfer.targetType === 'folder'
      ? this.startResumableDirectoryTransfer(transferId, tab.id, sender, true)
      : this.startResumableFileTransfer(transferId, tab.id, sender, true)
  }

  async clearTransfers(transferIds: string[]): Promise<WorkspaceSnapshot> {
    await this.readyPromise
    if (!transferIds.length) {
      return this.options.getSnapshot()
    }

    const removableIds = transferIds.filter((transferId) => {
      const transfer = this.transfers.get(transferId)
      return Boolean(
        transfer &&
        (transfer.status === 'done' || transfer.status === 'failed' || transfer.status === 'canceled') &&
        !transfer.resumable &&
        !transfer.cleanupPending
      )
    })

    if (!removableIds.length) {
      return this.options.getSnapshot()
    }

    this.transfers.removeMany(removableIds)
    removableIds.forEach((transferId) => {
      this.clearTransferUpdateTimer(transferId)
      this.transferTabs.delete(transferId)
      this.transferCancels.delete(transferId)
      this.transferCanceling.delete(transferId)
      this.transferStopReasons.delete(transferId)
    })
    this.persistTransfers()

    return this.options.getSnapshot()
  }

  async uploadFile(
    tabId: string,
    localPath: string,
    remoteDirectory: string,
    sender: WebContents,
    options?: TransferTargetOptions
  ): Promise<WorkspaceSnapshot> {
    await this.readyPromise
    const controllerForCancel = this.options.sessionRuntime.requireController(tabId)
    const targetRemotePath = path.posix.join(remoteDirectory, options?.targetName ?? path.basename(localPath))
    const localInfo = await stat(localPath)
    if (localInfo.isFile()) {
      const tab = this.options.tabs.getById(tabId)
      if (!tab) {
        throw new Error(`Tab not found: ${tabId}`)
      }
      this.assertNoConflictingTransfer(tab.profileId, 'upload', targetRemotePath)
      const transferId = this.addTransfer('upload', path.basename(localPath), tabId, sender, {
        profileId: tab.profileId,
        sessionType: controllerForCancel.type,
        fileAccessMode: controllerForCancel.getFileAccessMode(),
        targetType: 'file',
        sourcePath: localPath,
        destinationPath: targetRemotePath,
        partialPath: remoteTransferPartialPath(targetRemotePath),
        sourceIdentity: {
          size: localInfo.size,
          modifiedAt: localInfo.mtimeMs
        },
        totalBytes: localInfo.size,
        resumable: true
      })
      return this.startResumableFileTransfer(transferId, tabId, sender, false)
    }

    const tab = this.options.tabs.getById(tabId)
    if (!tab) {
      throw new Error(`Tab not found: ${tabId}`)
    }
    const entries = await this.collectLocalUploadEntries(localPath)
    const remoteRoot = targetRemotePath
    const manifest = createTransferManifest(
      [
        remoteRoot,
        ...entries.directories.map((directory) => path.posix.join(remoteRoot, ...directory.split(path.sep)))
      ],
      entries.files.map((file) => {
        const relativePath = file.relativePath.split(path.sep).join('/')
        const destinationPath = path.posix.join(remoteRoot, ...relativePath.split('/'))
        return {
          relativePath,
          sourcePath: file.fullPath,
          destinationPath,
          partialPath: remoteTransferPartialPath(destinationPath),
          sourceIdentity: {
            size: file.size,
            modifiedAt: file.modifiedAt
          }
        }
      })
    )
    const totals = transferManifestProgress(manifest)
    this.assertNoConflictingTransfer(tab.profileId, 'upload', remoteRoot)
    const transferId = this.addTransfer('upload', path.basename(localPath), tabId, sender, {
      profileId: tab.profileId,
      sessionType: controllerForCancel.type,
      fileAccessMode: controllerForCancel.getFileAccessMode(),
      targetType: 'folder',
      sourcePath: localPath,
      destinationPath: remoteRoot,
      manifest,
      transferredBytes: totals.transferredBytes,
      totalBytes: totals.totalBytes,
      resumable: true
    })
    return this.startResumableDirectoryTransfer(transferId, tabId, sender, false)
  }

  async downloadFile(
    tabId: string,
    remotePath: string,
    localDirectory: string,
    sender: WebContents,
    options?: TransferTargetOptions
  ): Promise<WorkspaceSnapshot> {
    await this.readyPromise
    const controller = this.options.sessionRuntime.requireController(tabId)
    const tab = this.options.tabs.getById(tabId)
    if (!tab) {
      throw new Error(`Tab not found: ${tabId}`)
    }
    const localPath = path.join(localDirectory, options?.targetName ?? path.posix.basename(remotePath))
    const remoteInfo = await controller.statRemoteFile(remotePath)
    if (!remoteInfo) {
      throw new Error(`远端文件不存在或无法读取：${remotePath}`)
    }
    this.assertNoConflictingTransfer(tab.profileId, 'download', localPath)
    const transferId = this.addTransfer('download', path.posix.basename(remotePath), tabId, sender, {
      profileId: tab.profileId,
      sessionType: controller.type,
      fileAccessMode: controller.getFileAccessMode(),
      targetType: 'file',
      sourcePath: remotePath,
      destinationPath: localPath,
      partialPath: localTransferPartialPath(localPath),
      sourceIdentity: remoteInfo,
      totalBytes: remoteInfo.size,
      resumable: true
    })
    return this.startResumableFileTransfer(transferId, tabId, sender, false)
  }

  async downloadRemotePath(
    tabId: string,
    remotePath: string,
    targetType: 'file' | 'folder',
    localDirectory: string,
    sender: WebContents,
    options?: TransferTargetOptions
  ): Promise<WorkspaceSnapshot> {
    await this.readyPromise
    if (targetType === 'file') {
      return this.downloadFile(tabId, remotePath, localDirectory, sender, options)
    }

    const controllerForCancel = this.options.sessionRuntime.requireController(tabId)
    const tab = this.options.tabs.getById(tabId)
    if (!tab) {
      throw new Error(`Tab not found: ${tabId}`)
    }
    const transferName = options?.targetName ?? (path.posix.basename(remotePath) || 'folder')
    const localRootPath = path.join(localDirectory, transferName)
    const transferState = { canceled: false }
    const previousRemotePath = controllerForCancel.getRemotePath()
    const entries = await this.collectRemoteDownloadEntries(controllerForCancel, remotePath, transferState).finally(
      async () => {
        if (controllerForCancel.getRemotePath() !== previousRemotePath) {
          await controllerForCancel.openRemotePath(previousRemotePath).catch(() => undefined)
        }
      }
    )
    const manifest = createTransferManifest(
      [localRootPath, ...entries.directories.map((directory) => path.join(localRootPath, ...directory.split('/')))],
      entries.files.map((file) => {
        const destinationPath = path.join(localRootPath, ...file.relativePath.split('/'))
        return {
          relativePath: file.relativePath,
          sourcePath: file.remotePath,
          destinationPath,
          partialPath: localTransferPartialPath(destinationPath),
          sourceIdentity: file.sourceIdentity
        }
      })
    )
    const totals = transferManifestProgress(manifest)
    this.assertNoConflictingTransfer(tab.profileId, 'download', localRootPath)
    const transferId = this.addTransfer('download', transferName, tabId, sender, {
      profileId: tab.profileId,
      sessionType: controllerForCancel.type,
      fileAccessMode: controllerForCancel.getFileAccessMode(),
      targetType: 'folder',
      sourcePath: remotePath,
      destinationPath: localRootPath,
      manifest,
      transferredBytes: totals.transferredBytes,
      totalBytes: totals.totalBytes,
      resumable: true
    })
    return this.startResumableDirectoryTransfer(transferId, tabId, sender, false)
  }

  private async collectLocalUploadEntries(
    rootPath: string,
    currentPath = rootPath
  ): Promise<{
    directories: string[]
    files: Array<{
      fullPath: string
      relativePath: string
      size: number
      modifiedAt: number
    }>
  }> {
    let entries
    try {
      entries = await readdir(currentPath, { withFileTypes: true })
    } catch (error) {
      if (currentPath !== rootPath && isSkippableLocalReadError(error)) {
        return { directories: [], files: [] }
      }
      throw error
    }
    const directories: string[] = []
    const files: Array<{ fullPath: string; relativePath: string; size: number; modifiedAt: number }> = []

    for (const entry of entries) {
      const fullPath = path.join(currentPath, entry.name)
      if (entry.isDirectory()) {
        const relativeDirectory = path.relative(rootPath, fullPath)
        directories.push(relativeDirectory)
        const nestedEntries = await this.collectLocalUploadEntries(rootPath, fullPath)
        directories.push(...nestedEntries.directories)
        files.push(...nestedEntries.files)
        continue
      }

      if (!entry.isFile()) {
        continue
      }

      let info
      try {
        info = await stat(fullPath)
      } catch (error) {
        if (isSkippableLocalReadError(error)) {
          continue
        }
        throw error
      }
      files.push({
        fullPath,
        relativePath: path.relative(rootPath, fullPath),
        size: info.size,
        modifiedAt: info.mtimeMs
      })
    }

    return { directories, files }
  }

  private startResumableFileTransfer(transferId: string, tabId: string, sender: WebContents, resume: boolean) {
    const existing = this.transferRuns.get(transferId)
    if (existing) {
      return existing
    }
    const run = this.executeResumableFileTransfer(transferId, tabId, sender, resume)
    this.transferRuns.set(transferId, run)
    run.then(
      () => {
        if (this.transferRuns.get(transferId) === run) {
          this.transferRuns.delete(transferId)
        }
      },
      () => {
        if (this.transferRuns.get(transferId) === run) {
          this.transferRuns.delete(transferId)
        }
      }
    )
    return run
  }

  private startResumableDirectoryTransfer(transferId: string, tabId: string, sender: WebContents, resume: boolean) {
    const existing = this.transferRuns.get(transferId)
    if (existing) {
      return existing
    }
    const run = this.executeResumableDirectoryTransfer(transferId, tabId, sender, resume)
    this.transferRuns.set(transferId, run)
    run.then(
      () => {
        if (this.transferRuns.get(transferId) === run) {
          this.transferRuns.delete(transferId)
        }
      },
      () => {
        if (this.transferRuns.get(transferId) === run) {
          this.transferRuns.delete(transferId)
        }
      }
    )
    return run
  }

  private async executeResumableDirectoryTransfer(
    transferId: string,
    tabId: string,
    sender: WebContents,
    resume: boolean
  ): Promise<WorkspaceSnapshot> {
    const task = this.transfers.get(transferId)
    const controller = this.options.sessionRuntime.requireController(tabId)
    if (!task || task.targetType !== 'folder' || !task.sourcePath || !task.destinationPath || !task.manifest) {
      throw new Error('目录传输任务缺少恢复所需的 manifest')
    }

    const transferState = { canceled: false }
    const transferAbortController = new AbortController()
    const transferTracker = createTransferSpeedTracker()
    let manifest = task.manifest
    this.setTransferCancel(transferId, async () => {
      transferState.canceled = true
      transferAbortController.abort()
    })

    try {
      for (const directory of manifest.directories) {
        this.ensureTransferActive(transferState)
        if (task.direction === 'upload') {
          await controller.ensureRemoteDirectory(directory)
        } else {
          await mkdir(directory, { recursive: true })
        }
      }

      if (!resume) {
        for (const entry of manifest.files) {
          this.ensureTransferActive(transferState)
          if (task.direction === 'upload') {
            await controller.removeRemoteFileIfExists(entry.partialPath)
            if (entry.stagingPath) {
              await controller.removeRemoteFileIfExists(entry.stagingPath)
            }
          } else {
            await removeLocalFileIfExists(entry.partialPath)
          }
        }
      }

      for (const originalEntry of manifest.files) {
        this.ensureTransferActive(transferState)
        const currentEntry = manifest.files.find((entry) => entry.relativePath === originalEntry.relativePath)
        if (!currentEntry) {
          throw new Error(`目录传输 manifest 损坏：${originalEntry.relativePath}`)
        }

        const sourceIdentity =
          task.direction === 'upload'
            ? await statLocalFile(currentEntry.sourcePath)
            : await controller.statRemoteFile(currentEntry.sourcePath)
        if (!sourceIdentity || !sameTransferIdentity(sourceIdentity, currentEntry.sourceIdentity)) {
          throw new Error(`源文件已发生变化，不能继续目录断点：${currentEntry.relativePath}`)
        }

        if (currentEntry.status === 'done') {
          const destinationIdentity =
            task.direction === 'upload'
              ? await controller.statRemoteFile(currentEntry.destinationPath)
              : await statLocalFile(currentEntry.destinationPath)
          this.ensureTransferActive(transferState)
          if (destinationIdentity?.size === sourceIdentity.size) {
            continue
          }
        }

        manifest = updateTransferManifestEntry(manifest, currentEntry.relativePath, {
          status: 'running',
          transferredBytes: 0
        })
        await this.updateDirectoryTransferProgress(
          transferId,
          manifest,
          sender,
          currentEntry.relativePath,
          transferTracker,
          'immediate'
        )

        await this.executeManifestEntry(
          task.direction,
          currentEntry,
          controller,
          transferState,
          transferAbortController.signal,
          (progress) => {
            const completedBefore = transferManifestProgress(manifest).transferredBytes
            const transferredBytes = completedBefore + Math.max(0, progress.transferredBytes ?? 0)
            const totalBytes = transferManifestProgress(manifest).totalBytes
            void this.updateTransfer(
              transferId,
              {
                progress: directoryProgressPercent(manifest, currentEntry.relativePath, progress.transferredBytes ?? 0),
                status: 'running',
                speed: transferTracker({ ...progress, transferredBytes }),
                message: currentEntry.relativePath,
                transferredBytes: Math.min(totalBytes, transferredBytes),
                totalBytes,
                retryAttempt: undefined
              },
              sender,
              'throttled'
            )
          }
        )
        manifest = updateTransferManifestEntry(manifest, currentEntry.relativePath, {
          status: 'done',
          transferredBytes: sourceIdentity.size
        })
        await this.updateDirectoryTransferProgress(
          transferId,
          manifest,
          sender,
          currentEntry.relativePath,
          transferTracker,
          'immediate'
        )
      }

      if (!isTransferManifestComplete(manifest)) {
        throw new Error('目录传输未完成，manifest 中仍有待传文件')
      }
      const totals = transferManifestProgress(manifest)
      await this.updateTransfer(
        transferId,
        {
          progress: 100,
          status: 'done',
          speed: undefined,
          message: undefined,
          transferredBytes: totals.totalBytes,
          totalBytes: totals.totalBytes,
          manifest,
          resumable: false,
          retryAttempt: undefined
        },
        sender
      )

      if (task.direction === 'upload') {
        await this.options.sessionRuntime.refreshRemoteFiles(tabId).catch((error) => {
          appWarn('[FileTerm][Transfer] Directory upload completed but remote listing refresh failed', error)
        })
      }
    } catch (error) {
      if (transferState.canceled || this.transferStopReasons.has(transferId)) {
        return this.options.getSnapshot()
      }

      const runningEntry = manifest.files.find((entry) => entry.status === 'running')
      if (runningEntry) {
        const partialIdentity = await (
          task.direction === 'upload'
            ? controller.statRemoteFile(runningEntry.partialPath)
            : statLocalFile(runningEntry.partialPath)
        ).catch(() => null)
        manifest = updateTransferManifestEntry(manifest, runningEntry.relativePath, {
          status: 'pending',
          transferredBytes: Math.min(runningEntry.sourceIdentity.size, partialIdentity?.size ?? 0)
        })
      }
      const totals = transferManifestProgress(manifest)
      await this.updateTransfer(
        transferId,
        {
          progress: totals.percent,
          status: 'paused',
          message: error instanceof Error ? error.message : '目录传输失败',
          speed: undefined,
          transferredBytes: totals.transferredBytes,
          totalBytes: totals.totalBytes,
          manifest,
          resumable: true,
          retryAttempt: undefined
        },
        sender
      )
      throw error
    } finally {
      this.transferCancels.delete(transferId)
      this.transferStopReasons.delete(transferId)
      this.persistTransfers()
    }

    return this.options.getSnapshot()
  }

  private async executeManifestEntry(
    direction: TransferTask['direction'],
    entry: TransferManifestEntry,
    controller: FileSessionController,
    transferState: { canceled: boolean },
    signal: AbortSignal,
    onProgress: (progress: TransferProgress) => void
  ) {
    this.ensureTransferActive(transferState)
    const partialIdentity =
      direction === 'upload'
        ? await controller.statRemoteFile(entry.partialPath)
        : await statLocalFile(entry.partialPath)
    const resumeOffset = partialIdentity?.size ?? 0
    this.ensureTransferActive(transferState)
    if (resumeOffset > entry.sourceIdentity.size) {
      throw new Error(`断点文件大于源文件：${entry.relativePath}`)
    }

    if (resumeOffset < entry.sourceIdentity.size || (entry.sourceIdentity.size === 0 && !partialIdentity)) {
      if (direction === 'upload') {
        await controller.uploadFile(entry.sourcePath, entry.partialPath, onProgress, {
          resumeOffset,
          signal,
          stagingPath: entry.stagingPath
        })
      } else {
        await mkdir(path.dirname(entry.partialPath), { recursive: true })
        this.ensureTransferActive(transferState)
        await controller.downloadFile(entry.sourcePath, entry.partialPath, onProgress, { resumeOffset, signal })
      }
    }

    this.ensureTransferActive(transferState)
    const completedPartial =
      direction === 'upload'
        ? await controller.statRemoteFile(entry.partialPath)
        : await statLocalFile(entry.partialPath)
    this.ensureTransferActive(transferState)
    if (!completedPartial || completedPartial.size !== entry.sourceIdentity.size) {
      throw new Error(
        `传输校验失败：${entry.relativePath} 断点大小为 ${completedPartial?.size ?? 0}，期望 ${entry.sourceIdentity.size}`
      )
    }
    if (direction === 'upload') {
      await controller.replaceRemoteFile(entry.partialPath, entry.destinationPath)
    } else {
      await replaceLocalFile(entry.partialPath, entry.destinationPath)
    }
  }

  private updateDirectoryTransferProgress(
    transferId: string,
    manifest: TransferManifest,
    sender: WebContents,
    message: string,
    transferTracker: ReturnType<typeof createTransferSpeedTracker>,
    emitMode: 'immediate' | 'throttled'
  ) {
    const totals = transferManifestProgress(manifest)
    return this.updateTransfer(
      transferId,
      {
        progress: totals.percent,
        status: 'running',
        speed: transferTracker(totals),
        message,
        transferredBytes: totals.transferredBytes,
        totalBytes: totals.totalBytes,
        manifest,
        resumable: true,
        retryAttempt: undefined
      },
      sender,
      emitMode
    )
  }

  private async transferSingleFile(
    direction: TransferTask['direction'],
    sourcePath: string,
    partialPath: string,
    sourceSize: number,
    controller: FileSessionController,
    transferState: { canceled: boolean },
    signal: AbortSignal,
    stagingPath: string | undefined,
    onProgress: (progress: TransferProgress) => void
  ) {
    this.ensureTransferActive(transferState)
    const partialIdentity =
      direction === 'upload' ? await controller.statRemoteFile(partialPath) : await statLocalFile(partialPath)
    const resumeOffset = partialIdentity?.size ?? 0
    this.ensureTransferActive(transferState)
    if (resumeOffset > sourceSize) {
      throw new Error('断点文件大于源文件，不能继续；请丢弃断点后重新传输')
    }
    if (resumeOffset === sourceSize && (sourceSize > 0 || partialIdentity)) {
      return
    }

    if (direction === 'upload') {
      await controller.uploadFile(sourcePath, partialPath, onProgress, { resumeOffset, signal, stagingPath })
    } else {
      await controller.downloadFile(sourcePath, partialPath, onProgress, { resumeOffset, signal })
    }
  }

  private async executeResumableFileTransfer(
    transferId: string,
    tabId: string,
    sender: WebContents,
    resume: boolean
  ): Promise<WorkspaceSnapshot> {
    const task = this.transfers.get(transferId)
    const controller = this.options.sessionRuntime.requireController(tabId)
    if (!task || task.targetType !== 'file' || !task.sourcePath || !task.destinationPath || !task.partialPath) {
      throw new Error('传输任务缺少恢复所需的文件信息')
    }

    const transferState = { canceled: false }
    const transferAbortController = new AbortController()
    const transferTracker = createTransferSpeedTracker()
    let currentSourceIdentity = task.sourceIdentity
    this.setTransferCancel(transferId, async () => {
      transferState.canceled = true
      transferAbortController.abort()
    })

    try {
      const sourceIdentity =
        task.direction === 'upload'
          ? await statLocalFile(task.sourcePath)
          : await controller.statRemoteFile(task.sourcePath)
      if (!sourceIdentity) {
        throw new Error('传输源文件不存在或无法读取')
      }
      currentSourceIdentity = sourceIdentity
      if (resume && !sameTransferIdentity(sourceIdentity, task.sourceIdentity)) {
        throw new Error('源文件已发生变化，不能继续旧断点；请丢弃后重新传输')
      }

      if (!resume) {
        if (task.direction === 'upload') {
          await controller.removeRemoteFileIfExists(task.partialPath)
          if (task.stagingPath) {
            await controller.removeRemoteFileIfExists(task.stagingPath)
          }
        } else {
          await removeLocalFileIfExists(task.partialPath)
        }
      }

      const partialIdentity = resume
        ? task.direction === 'upload'
          ? await controller.statRemoteFile(task.partialPath)
          : await statLocalFile(task.partialPath)
        : null

      if (resume && !partialIdentity && task.progress >= 99 && task.transferredBytes === sourceIdentity.size) {
        const destinationIdentity =
          task.direction === 'upload'
            ? await controller.statRemoteFile(task.destinationPath)
            : await statLocalFile(task.destinationPath)
        if (destinationIdentity?.size === sourceIdentity.size) {
          await this.updateTransfer(
            transferId,
            {
              progress: 100,
              status: 'done',
              speed: undefined,
              message: undefined,
              transferredBytes: sourceIdentity.size,
              totalBytes: sourceIdentity.size,
              resumable: false,
              retryAttempt: undefined,
              cleanupPending: false
            },
            sender
          )
          return this.options.getSnapshot()
        }
      }
      const resumeOffset = partialIdentity?.size ?? 0
      if (resumeOffset > sourceIdentity.size) {
        throw new Error('断点文件大于源文件，不能继续；请丢弃断点后重新传输')
      }

      await this.updateTransfer(
        transferId,
        {
          progress: sourceIdentity.size > 0 ? Math.min(99, Math.round((resumeOffset / sourceIdentity.size) * 100)) : 0,
          status: 'running',
          speed: undefined,
          message: resumeOffset > 0 ? `从 ${formatTransferByteCount(resumeOffset)} 继续` : task.partialPath,
          transferredBytes: resumeOffset,
          totalBytes: sourceIdentity.size,
          sourceIdentity,
          resumable: true,
          retryAttempt: undefined,
          cleanupPending: false
        },
        sender
      )

      const onProgress = (progress: TransferProgress) => {
        if (transferState.canceled || this.transferStopReasons.has(transferId)) {
          return
        }
        void this.updateTransfer(
          transferId,
          {
            progress: progress.percent,
            status: 'running',
            speed: transferTracker(progress),
            message: progress.message ?? task.partialPath,
            transferredBytes: progress.transferredBytes,
            totalBytes: progress.totalBytes,
            retryAttempt: undefined
          },
          sender,
          'throttled'
        )
      }
      await this.transferSingleFile(
        task.direction,
        task.sourcePath,
        task.partialPath,
        sourceIdentity.size,
        controller,
        transferState,
        transferAbortController.signal,
        task.stagingPath,
        onProgress
      )

      if (transferState.canceled) {
        return this.options.getSnapshot()
      }

      await this.updateTransfer(
        transferId,
        {
          progress: 99,
          status: 'verifying',
          speed: undefined,
          message: '正在校验文件大小...',
          transferredBytes: sourceIdentity.size,
          totalBytes: sourceIdentity.size
        },
        sender
      )
      const completedPartial =
        task.direction === 'upload'
          ? await controller.statRemoteFile(task.partialPath)
          : await statLocalFile(task.partialPath)
      this.ensureTransferActive(transferState)
      if (!completedPartial || completedPartial.size !== sourceIdentity.size) {
        throw new Error(`传输校验失败：断点文件大小为 ${completedPartial?.size ?? 0}，期望 ${sourceIdentity.size}`)
      }

      await this.updateTransfer(
        transferId,
        {
          status: 'finalizing',
          message: '正在替换目标文件...'
        },
        sender
      )
      if (task.direction === 'upload') {
        await controller.replaceRemoteFile(task.partialPath, task.destinationPath)
      } else {
        await replaceLocalFile(task.partialPath, task.destinationPath)
      }

      await this.updateTransfer(
        transferId,
        {
          progress: 100,
          status: 'done',
          speed: undefined,
          message: undefined,
          transferredBytes: sourceIdentity.size,
          totalBytes: sourceIdentity.size,
          resumable: false,
          retryAttempt: undefined,
          cleanupPending: false
        },
        sender
      )

      if (task.direction === 'upload') {
        await this.options.sessionRuntime.refreshRemoteFiles(tabId).catch((error) => {
          appWarn('[FileTerm][Transfer] Upload completed but remote listing refresh failed', error)
        })
      }
    } catch (error) {
      if (transferState.canceled || this.transferStopReasons.has(transferId)) {
        return this.options.getSnapshot()
      }

      const partialIdentity = await (
        task.direction === 'upload' ? controller.statRemoteFile(task.partialPath) : statLocalFile(task.partialPath)
      ).catch(() => null)
      const canResume = Boolean(
        partialIdentity && currentSourceIdentity && partialIdentity.size <= currentSourceIdentity.size
      )
      await this.updateTransfer(
        transferId,
        {
          status: canResume ? 'paused' : 'failed',
          message: error instanceof Error ? error.message : '传输失败',
          speed: undefined,
          transferredBytes: partialIdentity?.size,
          totalBytes: currentSourceIdentity?.size,
          progress:
            partialIdentity && currentSourceIdentity?.size
              ? Math.min(99, Math.round((partialIdentity.size / currentSourceIdentity.size) * 100))
              : task.progress,
          resumable: canResume,
          retryAttempt: undefined
        },
        sender
      )
      throw error
    } finally {
      this.transferCancels.delete(transferId)
      this.transferStopReasons.delete(transferId)
      this.persistTransfers()
    }

    return this.options.getSnapshot()
  }

  private async collectRemoteDownloadEntries(
    controller: FileSessionController,
    rootPath: string,
    transferState: { canceled: boolean },
    currentPath = rootPath
  ): Promise<{
    directories: string[]
    files: Array<{
      remotePath: string
      relativePath: string
      sourceIdentity: { size: number; modifiedAt?: number }
    }>
  }> {
    this.ensureTransferActive(transferState)
    const entries = (await controller.openRemotePath(currentPath)).filter((entry) => entry.name !== '..')
    const directories: string[] = []
    const files: Array<{
      remotePath: string
      relativePath: string
      sourceIdentity: { size: number; modifiedAt?: number }
    }> = []

    for (const entry of entries) {
      this.ensureTransferActive(transferState)
      const relativePath = path.posix.relative(rootPath, entry.path)
      if (entry.type === 'folder') {
        directories.push(relativePath)
        const nestedEntries = await this.collectRemoteDownloadEntries(controller, rootPath, transferState, entry.path)
        directories.push(...nestedEntries.directories)
        files.push(...nestedEntries.files)
        continue
      }

      const sourceIdentity = await controller.statRemoteFile(entry.path)
      if (!sourceIdentity) {
        throw new Error(`无法读取远端文件信息：${entry.path}`)
      }
      files.push({
        remotePath: entry.path,
        relativePath,
        sourceIdentity
      })
    }

    return { directories, files }
  }

  private addTransfer(
    direction: 'upload' | 'download',
    name: string,
    tabId: string,
    sender: WebContents,
    details?: Partial<Omit<TransferTask, 'id' | 'direction' | 'name' | 'progress' | 'status'>>
  ) {
    const scopedDetails =
      direction === 'upload' && details?.fileAccessMode === 'root' && details.profileId
        ? {
            ...details,
            ...(details.partialPath
              ? {
                  stagingPath: rootUploadStagingPath()
                }
              : {}),
            ...(details.manifest
              ? {
                  manifest: {
                    ...details.manifest,
                    files: details.manifest.files.map((entry) => ({
                      ...entry,
                      stagingPath: rootUploadStagingPath()
                    }))
                  }
                }
              : {})
          }
        : details
    const transferId = this.transfers.add(direction, name, {
      ...scopedDetails,
      tabId
    })
    this.transferTabs.set(transferId, tabId)
    this.emitTransferUpdate(transferId, sender)
    this.persistTransfers()
    return transferId
  }

  private assertNoConflictingTransfer(
    profileId: string,
    direction: TransferTask['direction'],
    destinationPath: string
  ) {
    const conflict = this.transfers
      .list()
      .find(
        (transfer) =>
          transfer.profileId === profileId &&
          transfer.direction === direction &&
          transfer.destinationPath === destinationPath &&
          transfer.status !== 'done' &&
          transfer.status !== 'canceled' &&
          !(transfer.status === 'failed' && !transfer.resumable)
      )
    if (conflict) {
      throw new Error(`该目标已有未完成传输：${conflict.name}。请继续或丢弃原任务。`)
    }
  }

  private setTransferCancel(transferId: string, cancel: () => Promise<void> | void) {
    this.transferCancels.set(transferId, cancel)
  }

  private ensureRootUploadStagingPaths(transferId: string): TransferTask | undefined {
    const transfer = this.transfers.get(transferId)
    if (!transfer) {
      return undefined
    }
    const withStagingPaths = withRootUploadStagingPaths(transfer)
    if (withStagingPaths === transfer) {
      return transfer
    }
    this.transfers.update(transferId, {
      stagingPath: withStagingPaths.stagingPath,
      manifest: withStagingPaths.manifest
    })
    this.persistTransfers()
    return this.transfers.get(transferId)
  }

  async finalizeTransfersForTab(tabId: string, message: string): Promise<void> {
    await this.readyPromise
    const transferIds = [...this.transferTabs.entries()]
      .filter(([, mappedTabId]) => mappedTabId === tabId)
      .map(([transferId]) => transferId)

    if (!transferIds.length) {
      return
    }

    const stopOperations: Array<Promise<unknown>> = []
    for (const transferId of transferIds) {
      const transfer = this.transfers.get(transferId)
      if (!transfer || !['running', 'queued', 'verifying', 'finalizing'].includes(transfer.status)) {
        continue
      }

      this.transferStopReasons.set(transferId, 'pause')
      const cancel = this.transferCancels.get(transferId)
      if (cancel) {
        stopOperations.push(this.invokeTransferCancel(cancel))
      }
      this.transferTabs.delete(transferId)
      this.transferCancels.delete(transferId)
      this.transferCanceling.delete(transferId)
      this.clearTransferUpdateTimer(transferId)
      const didUpdate = this.transfers.update(transferId, {
        status: transfer.resumable ? 'paused' : 'canceled',
        speed: undefined,
        message
      })
      if (didUpdate) {
        const nextTransfer = this.transfers.get(transferId)
        if (nextTransfer) {
          this.options.sessionRuntime.emitToTab(tabId, 'transfer:update', nextTransfer)
        }
      }
    }
    this.persistTransfers()
    await Promise.allSettled(stopOperations)
    await Promise.allSettled(
      transferIds
        .map((transferId) => this.transferRuns.get(transferId))
        .filter((run): run is Promise<WorkspaceSnapshot> => Boolean(run))
    )
    transferIds.forEach((transferId) => this.transferStopReasons.delete(transferId))
  }

  private findConnectedTabForProfile(profileId: string) {
    return this.options.tabs
      .list()
      .find(
        (candidate) =>
          candidate.profileId === profileId &&
          Boolean(this.options.sessionRuntime.getController(candidate.id)) &&
          this.options.sessionRuntime.get(candidate.id)?.connected
      )
  }

  private ensureTransferActive(transferState: { canceled: boolean }) {
    if (transferState.canceled) {
      throw new Error('传输已终止')
    }
  }

  private invokeTransferCancel(cancel?: () => Promise<void> | void): Promise<void> {
    try {
      return Promise.resolve(cancel?.())
    } catch (error) {
      return Promise.reject(error)
    }
  }

  private updateTransfer(
    transferId: string,
    patch: Partial<Omit<TransferTask, 'id' | 'direction' | 'name'>>,
    sender: WebContents,
    emitMode: 'immediate' | 'throttled' = 'immediate'
  ) {
    const changed = this.transfers.update(transferId, patch)
    if (!changed) {
      return
    }

    if (patch.status === 'done' || patch.status === 'failed' || patch.status === 'canceled') {
      this.transferTabs.delete(transferId)
    }

    if (emitMode === 'throttled') {
      this.scheduleTransferUpdate(transferId, sender)
      return
    }

    this.clearTransferUpdateTimer(transferId)
    this.emitTransferUpdate(transferId, sender)
    this.persistTransfers()
  }

  private async removeTransferPartial(transfer: TransferTask): Promise<{ message: string; pending: boolean }> {
    const partialPaths =
      transfer.manifest?.files.flatMap((entry) =>
        [entry.partialPath, entry.stagingPath].filter((value): value is string => Boolean(value))
      ) ?? [transfer.partialPath, transfer.stagingPath].filter((value): value is string => Boolean(value))
    if (!partialPaths.length) {
      return { message: '传输已取消', pending: false }
    }
    if (transfer.direction === 'download') {
      await Promise.all(partialPaths.map((partialPath) => removeLocalFileIfExists(partialPath)))
      return { message: '传输已取消，断点已删除', pending: false }
    }

    const tab = transfer.profileId ? this.findConnectedTabForProfile(transfer.profileId) : undefined
    if (!tab) {
      return {
        message: `传输已取消；${partialPaths.length} 个远端断点需要在重连后清理`,
        pending: true
      }
    }
    const controller = this.options.sessionRuntime.requireController(tab.id)
    for (const partialPath of partialPaths) {
      await controller.removeRemoteFileIfExists(partialPath)
    }
    return { message: '传输已取消，远端断点已删除', pending: false }
  }

  private async restoreTransfers(): Promise<void> {
    if (!this.options.transferJournal) {
      return
    }
    try {
      this.transfers.replaceAll((await this.options.transferJournal.load()).map(withRootUploadStagingPaths))
      await this.options.transferJournal.save(this.transfers.list())
    } catch (error) {
      appWarn('[FileTerm][Transfer] Failed to restore transfer journal', error)
    }
  }

  private persistTransfers() {
    if (!this.options.transferJournal) {
      return
    }
    void this.options.transferJournal.save(this.transfers.list()).catch((error) => {
      appWarn('[FileTerm][Transfer] Failed to persist transfer journal', error)
    })
  }

  private scheduleTransferUpdate(transferId: string, sender: WebContents) {
    if (this.transferUpdateTimers.has(transferId)) {
      return
    }

    const timer = setTimeout(() => {
      this.transferUpdateTimers.delete(transferId)
      this.emitTransferUpdate(transferId, sender)
    }, TRANSFER_UPDATE_INTERVAL_MS)
    this.transferUpdateTimers.set(transferId, timer)
  }

  private clearTransferUpdateTimer(transferId: string) {
    const timer = this.transferUpdateTimers.get(transferId)
    if (timer) {
      clearTimeout(timer)
      this.transferUpdateTimers.delete(transferId)
    }
  }

  private emitTransferUpdate(transferId: string, sender: WebContents) {
    const transfer = this.transfers.get(transferId)
    if (transfer) {
      this.options.sessionRuntime.emitToSender(sender, 'transfer:update', transfer)
    }
  }
}

function isSkippableLocalReadError(error: unknown) {
  return Boolean(
    error &&
    typeof error === 'object' &&
    'code' in error &&
    ((error as { code?: string }).code === 'EACCES' || (error as { code?: string }).code === 'EPERM')
  )
}
