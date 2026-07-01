import { randomUUID } from 'node:crypto'
import { mkdir, readdir, stat } from 'node:fs/promises'
import path from 'node:path'
import type { WebContents } from 'electron'
import {
  type CommandSendPreferences,
  type CommandExecutionOptions,
  type CommandTemplateInput,
  type CommandFolder,
  type ConnectionFolder,
  type ConnectionLibrarySnapshot,
  type ConnectionProfile,
  type CommandExecutionResult,
  type TerminalCommandHistoryEntry,
  type CreateProfileInput,
  type RemoteFileAccessOptions,
  type RemoteFileItem,
  type SshInteractionResponse,
  type SessionSnapshot,
  type PermissionChangeOptions,
  type TransferProgress,
  type TransferTargetOptions,
  type TransferTask,
  type WorkspaceSnapshot
} from '@fileterm/core'
import type { ProfileRepository } from '@fileterm/storage'
import { seedCommandFolders, seedCommandTemplates, seedProfiles, seedTransfers } from './workspace/seed-data.js'
import { WorkspaceSessionRuntime, type LiveSessionController } from './workspace/workspace-session-runtime.js'
import { WorkspaceTabsState } from './workspace/workspace-tabs.js'
import { WorkspaceTransfersState } from './workspace/workspace-transfers.js'

const TRANSFER_UPDATE_INTERVAL_MS = 200

export class WorkspaceService {
  private static readonly DISCONNECTED_TRANSFER_MESSAGES = {
    zhCN: '连接已断开，传输已终止',
    enUS: 'Connection closed, transfer terminated'
  } as const
  private readonly profileRepository: ProfileRepository
  private readonly tabs = new WorkspaceTabsState()
  private readonly transfers = new WorkspaceTransfersState(seedTransfers)
  private readonly transferCancels = new Map<string, () => Promise<void> | void>()
  private readonly transferCanceling = new Set<string>()
  private readonly transferTabs = new Map<string, string>()
  private readonly transferUpdateTimers = new Map<string, ReturnType<typeof setTimeout>>()
  private readonly privilegedAccess = new Map<string, RemoteFileAccessOptions>()
  private readonly sessionRuntime = new WorkspaceSessionRuntime({
    getSnapshot: () => this.getSnapshot(),
    updateTabStatus: (tabId, status) => {
      this.tabs.updateStatus(tabId, status)
    },
    getTabStatus: (tabId) => this.tabs.getById(tabId)?.status,
    rememberTrustedHostFingerprint: (profileId, fingerprint) => this.rememberTrustedHostFingerprint(profileId, fingerprint),
    onTabDisconnected: (tabId) => {
      this.privilegedAccess.delete(tabId)
      return this.finalizeTransfersForTab(tabId, this.getDisconnectedTransferMessage())
    }
  })

  constructor(
    profileRepository: ProfileRepository,
    private readonly options?: {
      getLocale?(): 'zhCN' | 'enUS'
    }
  ) {
    this.profileRepository = profileRepository
  }

  async shutdown() {
    for (const timer of this.transferUpdateTimers.values()) {
      clearTimeout(timer)
    }
    this.transferUpdateTimers.clear()
    await this.sessionRuntime.shutdown()
  }

  async getSnapshot(): Promise<WorkspaceSnapshot> {
    const [profiles, folders, commandFolders, commandTemplates] = await Promise.all([
      this.profileRepository.list(),
      this.profileRepository.listFolders?.() ?? Promise.resolve([]),
      this.profileRepository.listCommandFolders?.() ?? Promise.resolve([]),
      this.profileRepository.listCommandTemplates?.() ?? Promise.resolve([])
    ])

    return {
      profiles,
      folders,
      commandFolders,
      commandTemplates,
      tabs: this.tabs.list(),
      activeTabId: this.tabs.getActiveTabId(),
      transfers: this.transfers.list(),
      sessions: this.sessionRuntime.list()
    }
  }

  async getConnectionLibrary(): Promise<ConnectionLibrarySnapshot> {
    const [profiles, folders] = await Promise.all([
      this.profileRepository.list(),
      this.profileRepository.listFolders()
    ])
    return {
      profiles,
      folders
    }
  }

  bindWorkspaceSender(sender: WebContents) {
    for (const tab of this.tabs.list()) {
      this.sessionRuntime.setSender(tab.id, sender)
      void this.sessionRuntime.restoreTabData(tab.id)
    }
  }

  async createProfile(input: CreateProfileInput): Promise<WorkspaceSnapshot> {
    await this.profileRepository.create(input)
    return this.getSnapshot()
  }

  async updateProfile(profileId: string, input: CreateProfileInput): Promise<WorkspaceSnapshot> {
    await this.profileRepository.update(profileId, input)
    return this.getSnapshot()
  }

  async deleteProfile(profileId: string): Promise<WorkspaceSnapshot> {
    await this.profileRepository.delete(profileId)
    return this.getSnapshot()
  }

  async createFolder(name: string, parentId?: string): Promise<WorkspaceSnapshot> {
    await this.profileRepository.createFolder?.(name, parentId)
    return this.getSnapshot()
  }

  async updateFolder(folderId: string, updates: Partial<ConnectionFolder>): Promise<WorkspaceSnapshot> {
    await this.profileRepository.updateFolder?.(folderId, updates)
    return this.getSnapshot()
  }

  async deleteFolder(folderId: string): Promise<WorkspaceSnapshot> {
    await this.profileRepository.deleteFolder?.(folderId)
    return this.getSnapshot()
  }

  async updateEntityOrder(id: string, newParentId: string | undefined, newOrder: number): Promise<WorkspaceSnapshot> {
    await this.profileRepository.updateOrder?.(id, newParentId, newOrder)
    return this.getSnapshot()
  }

  async createCommandFolder(name: string, parentId?: string): Promise<WorkspaceSnapshot> {
    await this.profileRepository.createCommandFolder?.(name, parentId)
    return this.getSnapshot()
  }

  async updateCommandFolder(folderId: string, updates: Partial<CommandFolder>): Promise<WorkspaceSnapshot> {
    await this.profileRepository.updateCommandFolder?.(folderId, updates)
    return this.getSnapshot()
  }

  async deleteCommandFolder(folderId: string): Promise<WorkspaceSnapshot> {
    await this.profileRepository.deleteCommandFolder?.(folderId)
    return this.getSnapshot()
  }

  async updateCommandOrder(id: string, newParentId: string | undefined, newOrder: number): Promise<WorkspaceSnapshot> {
    await this.profileRepository.updateCommandOrder?.(id, newParentId, newOrder)
    return this.getSnapshot()
  }

  async createCommandTemplate(input: CommandTemplateInput): Promise<WorkspaceSnapshot> {
    await this.profileRepository.createCommandTemplate?.(input)
    return this.getSnapshot()
  }

  async updateCommandTemplate(commandId: string, input: CommandTemplateInput): Promise<WorkspaceSnapshot> {
    await this.profileRepository.updateCommandTemplate?.(commandId, input)
    return this.getSnapshot()
  }

  async deleteCommandTemplate(commandId: string): Promise<WorkspaceSnapshot> {
    await this.profileRepository.deleteCommandTemplate?.(commandId)
    return this.getSnapshot()
  }

  async executeCommandTemplate(
    tabId: string,
    commandId: string,
    args: string[] = [],
    options?: CommandExecutionOptions
  ): Promise<CommandExecutionResult> {
    const controller = this.sessionRuntime.requireController(tabId)
    if (controller.type !== 'ssh') {
      throw new Error('只有 SSH 会话支持快捷命令')
    }

    const command = await this.profileRepository.getCommandTemplateById?.(commandId)
    if (!command) {
      throw new Error(`Command not found: ${commandId}`)
    }

    const renderedCommand = command.command.replace(/\[p#(\d+)\]/g, (_, rawIndex: string) => {
      const nextArg = args[Number(rawIndex) - 1]
      return nextArg ?? ''
    })

    const appendCarriageReturn = options?.appendCarriageReturn ?? command.appendCarriageReturn
    await controller.write(appendCarriageReturn ? `${renderedCommand}\r` : renderedCommand)

    return { renderedCommand }
  }

  async getTerminalCommandHistory(profileId: string): Promise<TerminalCommandHistoryEntry[]> {
    return this.profileRepository.getTerminalCommandHistory(profileId)
  }

  async setTerminalCommandHistory(profileId: string, entries: TerminalCommandHistoryEntry[]): Promise<void> {
    await this.profileRepository.setTerminalCommandHistory(profileId, entries)
  }

  async getCommandSendPreferences(): Promise<CommandSendPreferences> {
    return this.profileRepository.getCommandSendPreferences()
  }

  async setCommandSendPreferences(preferences: CommandSendPreferences): Promise<void> {
    await this.profileRepository.setCommandSendPreferences(preferences)
  }

  async openProfile(profileId: string, sender: WebContents): Promise<WorkspaceSnapshot> {
    const profile = await this.profileRepository.getById(profileId)
    if (!profile) {
      throw new Error(`Profile not found: ${profileId}`)
    }

    const tabId = randomUUID()
    this.tabs.open(tabId, profile)
    this.sessionRuntime.setSender(tabId, sender)
    const controller = this.createController(tabId, profile)

    const snapshot: SessionSnapshot = {
      profileId: profile.id,
      accessHost: profile.host,
      summary: profile.type === 'ssh' ? '连接主机...' : controller.getSummary(),
      terminalTranscript:
        controller.type === 'ssh' ? controller.getTerminalTranscript() : undefined,
      remotePath: controller.getRemotePath(),
      shellCwd: controller.type === 'ssh' ? controller.getShellCwd() : undefined,
      followShellCwd: profile.type === 'ssh',
      remoteFiles: [],
      fileAccessMode: controller.getFileAccessMode(),
      sudoUser: profile.type === 'ssh' ? 'root' : undefined,
      hasReusableSudoAuth: controller.type === 'ssh' ? controller.hasReusableSudoAuth() : false,
      connected: false
    }

    this.sessionRuntime.set(tabId, snapshot)
    void this.sessionRuntime.connect(tabId, controller)
    void this.profileRepository.touchProfile(profileId)

    return this.getSnapshot()
  }

  async reconnectTab(tabId: string, sender: WebContents): Promise<WorkspaceSnapshot> {
    const tab = this.tabs.getById(tabId)
    if (!tab) {
      throw new Error(`Tab not found: ${tabId}`)
    }

    const currentSender = this.sessionRuntime.getSender(tabId)
    const reusableSender = currentSender && !currentSender.isDestroyed() ? currentSender : sender
    if (!reusableSender || reusableSender.isDestroyed()) {
      throw new Error(`Tab sender unavailable: ${tabId}`)
    }
    this.sessionRuntime.setSender(tabId, reusableSender)

    await this.sessionRuntime.disconnect(tabId)
    await this.finalizeTransfersForTab(tabId, this.getDisconnectedTransferMessage())

    const profile = await this.profileRepository.getById(tab.profileId)
    if (!profile) {
      throw new Error(`Profile not found: ${tab.profileId}`)
    }

    const current = this.sessionRuntime.get(tabId)
    const disconnectedTranscript = appendDisconnectedTranscript(current?.terminalTranscript)
    this.sessionRuntime.set(tabId, {
      profileId: profile.id,
      accessHost: profile.host,
      summary: current?.accessHost ? `Disconnected from ${current.accessHost}` : 'Disconnected',
      terminalTranscript: disconnectedTranscript,
      remotePath: current?.remotePath ?? profile.remotePath,
      shellCwd: current?.shellCwd,
      followShellCwd: current?.followShellCwd ?? (profile.type === 'ssh'),
      remoteFiles: [],
      fileAccessMode: 'user',
      sudoUser: current?.sudoUser ?? (profile.type === 'ssh' ? 'root' : undefined),
      hasReusableSudoAuth: false,
      connected: false,
      systemMetrics: undefined
    })

    const controller = this.createController(tabId, profile, disconnectedTranscript)
    this.sessionRuntime.set(tabId, {
      profileId: profile.id,
      accessHost: profile.host,
      summary: profile.type === 'ssh' ? '连接主机...' : `连接主机 ${profile.host}:${profile.port}...`,
      terminalTranscript:
        controller.type === 'ssh' ? controller.getTerminalTranscript() : undefined,
      remotePath: current?.remotePath ?? profile.remotePath,
      shellCwd: current?.shellCwd,
      followShellCwd: current?.followShellCwd ?? (profile.type === 'ssh'),
      remoteFiles: [],
      fileAccessMode: controller.getFileAccessMode(),
      sudoUser: current?.sudoUser ?? (profile.type === 'ssh' ? 'root' : undefined),
      hasReusableSudoAuth: controller.type === 'ssh' ? controller.hasReusableSudoAuth() : false,
      connected: false,
      systemMetrics: undefined
    })
    this.tabs.updateStatus(tabId, 'connecting')
    this.tabs.activate(tabId)

    void this.sessionRuntime.connect(tabId, controller)
    await this.sessionRuntime.emitSnapshot(reusableSender)
    return this.getSnapshot()
  }

  async activateTab(tabId: string): Promise<WorkspaceSnapshot> {
    if (!this.tabs.has(tabId)) {
      return this.getSnapshot()
    }
    this.tabs.activate(tabId)
    return this.getSnapshot()
  }

  async closeTab(tabId: string): Promise<WorkspaceSnapshot> {
    await this.sessionRuntime.teardown(tabId)
    this.privilegedAccess.delete(tabId)
    this.tabs.remove(tabId)
    return this.getSnapshot()
  }

  async disconnectTab(tabId: string): Promise<WorkspaceSnapshot> {
    const tab = this.tabs.getById(tabId)
    const current = this.sessionRuntime.get(tabId)
    if (!tab || !current) {
      throw new Error(`Tab not found: ${tabId}`)
    }

    await this.sessionRuntime.disconnect(tabId)
    await this.finalizeTransfersForTab(tabId, this.getDisconnectedTransferMessage())
    this.privilegedAccess.delete(tabId)
    const latest = this.sessionRuntime.get(tabId) ?? current
    const disconnectedTranscript = appendDisconnectedTranscript(latest.terminalTranscript)
    this.sessionRuntime.set(tabId, {
      ...latest,
      summary: latest.accessHost ? `Disconnected from ${latest.accessHost}` : 'Disconnected',
      terminalTranscript: disconnectedTranscript,
      remoteFiles: [],
      fileAccessMode: 'user',
      hasReusableSudoAuth: false,
      connected: false,
      systemMetrics: undefined
    })
    this.tabs.updateStatus(tabId, 'closed')
    await this.sessionRuntime.emitSnapshotForTab(tabId)
    return this.getSnapshot()
  }

  async resolveSshInteraction(requestId: string, response: SshInteractionResponse): Promise<void> {
    this.sessionRuntime.resolveSshInteraction(requestId, response)
  }

  async rememberTrustedHostFingerprint(profileId: string, fingerprint: string): Promise<void> {
    await this.profileRepository.updateTrustedHostFingerprint?.(profileId, fingerprint)
  }

  async queueUpload(fileNames: string[]): Promise<WorkspaceSnapshot> {
    this.transfers.queueUploads(fileNames)
    return this.getSnapshot()
  }

  async cancelTransfer(transferId: string, sender: WebContents): Promise<WorkspaceSnapshot> {
    if (this.transferCanceling.has(transferId)) {
      return this.getSnapshot()
    }

    const transfer = this.transfers.get(transferId)
    if (!transfer || (transfer.status !== 'running' && transfer.status !== 'queued')) {
      return this.getSnapshot()
    }

    this.transferCanceling.add(transferId)
    const cancel = this.transferCancels.get(transferId)
    this.transferCancels.delete(transferId)
    await this.updateTransfer(transferId, {
      status: 'canceled',
      message: '传输已终止',
      speed: undefined
    }, sender)

    try {
      await cancel?.()
    } catch {
      // Cancel is best-effort here; the UI state should not bounce back to running.
    } finally {
      this.transferCanceling.delete(transferId)
    }

    return this.getSnapshot()
  }

  async clearTransfers(transferIds: string[]): Promise<WorkspaceSnapshot> {
    if (!transferIds.length) {
      return this.getSnapshot()
    }

    const removableIds = transferIds.filter((transferId) => {
      const transfer = this.transfers.get(transferId)
      return Boolean(transfer && transfer.status !== 'running' && transfer.status !== 'queued')
    })

    if (!removableIds.length) {
      return this.getSnapshot()
    }

    this.transfers.removeMany(removableIds)
    removableIds.forEach((transferId) => {
      this.clearTransferUpdateTimer(transferId)
      this.transferTabs.delete(transferId)
      this.transferCancels.delete(transferId)
      this.transferCanceling.delete(transferId)
    })

    return this.getSnapshot()
  }

  async uploadFile(
    tabId: string,
    localPath: string,
    remoteDirectory: string,
    sender: WebContents,
    options?: TransferTargetOptions
  ): Promise<WorkspaceSnapshot> {
    const controllerForCancel = this.sessionRuntime.requireController(tabId)
    const transferId = this.addTransfer('upload', path.basename(localPath), tabId, sender)
    const targetRemotePath = path.posix.join(remoteDirectory, options?.targetName ?? path.basename(localPath))
    const transferState = { canceled: false }
    const transferTracker = createTransferSpeedTracker()
    this.setTransferCancel(transferId, async () => {
      transferState.canceled = true
      await controllerForCancel.abortTransfer()
    })

    try {
      await this.uploadLocalEntry(controllerForCancel, localPath, remoteDirectory, transferState, (progress) => {
        if (transferState.canceled) {
          return
        }
        void this.updateTransfer(transferId, {
          progress: progress.percent,
          status: 'running',
          speed: transferTracker(progress),
          message: progress.message ?? targetRemotePath,
          transferredBytes: progress.transferredBytes,
          totalBytes: progress.totalBytes
        }, sender, 'throttled')
      }, options?.targetName)
      if (transferState.canceled) {
        return this.getSnapshot()
      }
      await this.updateTransfer(transferId, {
        progress: 100,
        status: 'done',
        speed: undefined,
        message: undefined
      }, sender)
      await this.sessionRuntime.runRemoteFileOperation(tabId, async (controller, current) => {
        const remoteFiles = await controller.listRemoteFiles()
        const latest = this.sessionRuntime.get(tabId) ?? current
        this.sessionRuntime.set(tabId, {
          ...latest,
          remotePath: controller.getRemotePath(),
          fileAccessMode: controller.getFileAccessMode(),
          hasReusableSudoAuth: controller.type === 'ssh' ? controller.hasReusableSudoAuth() : false,
          remoteFiles
        })
      })
    } catch (error) {
      if (transferState.canceled) {
        return this.getSnapshot()
      }
      await this.updateTransfer(transferId, {
        status: 'failed',
        message: error instanceof Error ? error.message : '上传失败',
        speed: undefined
      }, sender)
      throw error
    } finally {
      this.transferCancels.delete(transferId)
    }

    return this.getSnapshot()
  }

  async downloadFile(
    tabId: string,
    remotePath: string,
    localDirectory: string,
    sender: WebContents,
    options?: TransferTargetOptions
  ): Promise<WorkspaceSnapshot> {
    const controllerForCancel = this.sessionRuntime.requireController(tabId)
    const localPath = path.join(localDirectory, options?.targetName ?? path.posix.basename(remotePath))
    const transferId = this.addTransfer('download', path.posix.basename(remotePath), tabId, sender)
    const transferState = { canceled: false }
    const transferTracker = createTransferSpeedTracker()
    this.setTransferCancel(transferId, async () => {
      transferState.canceled = true
      await controllerForCancel.abortTransfer()
    })

    try {
      this.ensureTransferActive(transferState)
      await controllerForCancel.downloadFile(remotePath, localPath, (progress) => {
        if (transferState.canceled) {
          return
        }
        void this.updateTransfer(transferId, {
          progress: progress.percent,
          status: 'running',
          speed: transferTracker(progress),
          message: progress.message ?? localPath,
          transferredBytes: progress.transferredBytes,
          totalBytes: progress.totalBytes
        }, sender, 'throttled')
      })
      if (transferState.canceled) {
        return this.getSnapshot()
      }
      await this.updateTransfer(transferId, {
        progress: 100,
        status: 'done',
        speed: undefined,
        message: undefined
      }, sender)
    } catch (error) {
      if (transferState.canceled) {
        return this.getSnapshot()
      }
      await this.updateTransfer(transferId, {
        status: 'failed',
        message: error instanceof Error ? error.message : '下载失败',
        speed: undefined
      }, sender)
      throw error
    } finally {
      this.transferCancels.delete(transferId)
    }

    return this.getSnapshot()
  }

  async downloadRemotePath(
    tabId: string,
    remotePath: string,
    targetType: 'file' | 'folder',
    localDirectory: string,
    sender: WebContents,
    options?: TransferTargetOptions
  ): Promise<WorkspaceSnapshot> {
    if (targetType === 'file') {
      return this.downloadFile(tabId, remotePath, localDirectory, sender, options)
    }

    const controllerForCancel = this.sessionRuntime.requireController(tabId)
    const transferName = options?.targetName ?? (path.posix.basename(remotePath) || 'folder')
    const localRootPath = path.join(localDirectory, transferName)
    const transferId = this.addTransfer('download', transferName, tabId, sender)
    const transferState = { canceled: false }
    this.setTransferCancel(transferId, async () => {
      transferState.canceled = true
      await controllerForCancel.abortTransfer()
    })

    try {
      await this.sessionRuntime.runRemoteFileOperation(tabId, async (controller) => {
        const { directories, files } = await this.collectRemoteDownloadEntries(controller, remotePath, transferState)
        await mkdir(localRootPath, { recursive: true })

        for (const directory of directories) {
          this.ensureTransferActive(transferState)
          await mkdir(path.join(localRootPath, ...directory.split('/')), { recursive: true })
        }

        if (!files.length) {
          await this.updateTransfer(transferId, {
            progress: 100,
            status: 'done',
            speed: undefined,
            message: undefined
          }, sender)
          return
        }

        let completedFiles = 0
        const totalFiles = files.length

        for (const file of files) {
          this.ensureTransferActive(transferState)
          const localFilePath = path.join(localRootPath, ...file.relativePath.split('/'))
          await mkdir(path.dirname(localFilePath), { recursive: true })
          await controller.downloadFile(file.remotePath, localFilePath, (progress) => {
            if (transferState.canceled) {
              return
            }

            const currentFraction = Math.max(0, Math.min(1, progress.percent / 100))
            const overallPercent = Math.max(
              1,
              Math.min(99, Math.round(((completedFiles + currentFraction) / totalFiles) * 100))
            )

            void this.updateTransfer(transferId, {
              progress: overallPercent,
              status: 'running',
              speed: undefined,
              message: localFilePath,
              transferredBytes: progress.transferredBytes,
              totalBytes: progress.totalBytes
            }, sender, 'throttled')
          })

          completedFiles += 1
          await this.updateTransfer(transferId, {
            progress: Math.max(1, Math.min(99, Math.round((completedFiles / totalFiles) * 100))),
            status: 'running',
            speed: undefined,
            message: localFilePath
          }, sender, 'throttled')
        }

        if (transferState.canceled) {
          return
        }

        await this.updateTransfer(transferId, {
          progress: 100,
          status: 'done',
          speed: undefined,
          message: undefined
        }, sender)
      })
    } catch (error) {
      if (transferState.canceled) {
        return this.getSnapshot()
      }
      await this.updateTransfer(transferId, {
        status: 'failed',
        message: error instanceof Error ? error.message : '下载失败',
        speed: undefined
      }, sender)
      throw error
    } finally {
      this.transferCancels.delete(transferId)
    }

    return this.getSnapshot()
  }

  async readRemoteFile(tabId: string, targetPath: string, encoding?: string): Promise<string> {
    return this.sessionRuntime.runRemoteFileOperation(tabId, (controller) =>
      controller.readRemoteFile(targetPath, encoding)
    )
  }

  async writeRemoteFile(tabId: string, targetPath: string, content: string, encoding?: string): Promise<WorkspaceSnapshot> {
    return this.runRemoteFileMutation(tabId, async (controller) => {
      await controller.writeRemoteFile(targetPath, content, encoding)
      return controller.listRemoteFiles()
    })
  }

  async createRemoteDirectory(tabId: string, parentPath: string, name: string): Promise<WorkspaceSnapshot> {
    return this.runRemoteFileMutation(tabId, async (controller) => {
      await controller.ensureRemoteDirectory(path.posix.join(parentPath, name))
      return controller.listRemoteFiles()
    })
  }

  async createRemoteFile(tabId: string, parentPath: string, name: string): Promise<WorkspaceSnapshot> {
    return this.runRemoteFileMutation(tabId, async (controller) => {
      await controller.writeRemoteFile(path.posix.join(parentPath, name), '')
      return controller.listRemoteFiles()
    })
  }

  async copyRemotePath(tabId: string, targetPath: string, destinationPath: string, targetType: 'file' | 'folder'): Promise<WorkspaceSnapshot> {
    return this.runRemoteFileMutation(tabId, async (controller) => {
      await controller.copyRemotePath(targetPath, destinationPath, targetType)
      return controller.listRemoteFiles()
    })
  }

  async moveRemotePath(tabId: string, targetPath: string, destinationPath: string): Promise<WorkspaceSnapshot> {
    return this.runRemoteFileMutation(tabId, async (controller) => {
      await controller.moveRemotePath(targetPath, destinationPath)
      return controller.listRemoteFiles()
    })
  }

  async renameRemotePath(tabId: string, targetPath: string, newName: string): Promise<WorkspaceSnapshot> {
    return this.runRemoteFileMutation(tabId, async (controller) => {
      const nextPath = path.posix.join(path.posix.dirname(targetPath), newName)
      await controller.renameRemotePath(targetPath, nextPath)
      return controller.listRemoteFiles()
    })
  }

  async deleteRemotePath(tabId: string, targetPath: string, targetType: 'file' | 'folder'): Promise<WorkspaceSnapshot> {
    return this.runRemoteFileMutation(tabId, async (controller) => {
      await controller.deleteRemotePath(targetPath, targetType)
      return controller.listRemoteFiles()
    })
  }

  async changeRemotePermissions(tabId: string, targetPath: string, options: PermissionChangeOptions): Promise<WorkspaceSnapshot> {
    return this.runRemoteFileMutation(tabId, async (controller) => {
      await controller.changeRemotePermissions(targetPath, options)
      return controller.listRemoteFiles()
    })
  }

  async writeToTerminal(tabId: string, data: string): Promise<void> {
    const controller = this.sessionRuntime.getController(tabId)
    if (!controller || controller.type !== 'ssh') {
      return
    }
    await controller.write(data)
  }

  async resizeTerminal(tabId: string, cols: number, rows: number, width: number, height: number): Promise<void> {
    const controller = this.sessionRuntime.getController(tabId)
    if (!controller || controller.type !== 'ssh') {
      return
    }
    await controller.resize(cols, rows, width, height)
  }

  async openRemotePath(tabId: string, targetPath: string): Promise<WorkspaceSnapshot> {
    await this.sessionRuntime.openRemotePath(tabId, targetPath)
    return this.getSnapshot()
  }

  async setFollowShellCwd(tabId: string, enabled: boolean): Promise<WorkspaceSnapshot> {
    await this.sessionRuntime.setFollowShellCwd(tabId, enabled)
    return this.getSnapshot()
  }

  async setRemoteFileAccessMode(tabId: string, mode: 'user' | 'root', options?: RemoteFileAccessOptions): Promise<WorkspaceSnapshot> {
    const current = this.sessionRuntime.get(tabId)
    const resolvedOptions = mode === 'root' ? this.resolvePrivilegedAccess(tabId, current, options) : options

    await this.sessionRuntime.setFileAccessMode(tabId, mode, resolvedOptions)
    if (mode === 'root' && resolvedOptions) {
      this.privilegedAccess.set(tabId, resolvedOptions)
    }
    return this.getSnapshot()
  }

  private createController(tabId: string, profile: ConnectionProfile, initialTranscript?: string) {
    return this.sessionRuntime.createController(tabId, profile, initialTranscript)
  }

  private resolvePrivilegedAccess(
    tabId: string,
    current?: SessionSnapshot,
    next?: RemoteFileAccessOptions
  ): RemoteFileAccessOptions | undefined {
    const cached = this.privilegedAccess.get(tabId)
    const sudoUser = next?.sudoUser?.trim() || cached?.sudoUser?.trim() || current?.sudoUser?.trim() || 'root'
    const hasPassword = next && 'sudoPassword' in next
    const sudoPassword = hasPassword ? next.sudoPassword : cached?.sudoPassword

    return {
      sudoUser,
      ...(sudoPassword !== undefined ? { sudoPassword } : {})
    }
  }

  private async uploadLocalEntry(
    controller: ReturnType<WorkspaceService['createController']>,
    localPath: string,
    remoteDirectory: string,
    transferState: { canceled: boolean },
    onProgress: (progress: TransferProgress) => void,
    targetName?: string
  ) {
    this.ensureTransferActive(transferState)
    const info = await stat(localPath)
    if (!info.isDirectory()) {
      const remotePath = path.posix.join(remoteDirectory, targetName ?? path.basename(localPath))
      await controller.uploadFile(localPath, remotePath, onProgress)
      return
    }

    const { directories, files } = await this.collectLocalUploadEntries(localPath)
    const remoteRoot = path.posix.join(remoteDirectory, targetName ?? path.basename(localPath))
    const totalBytes = Math.max(files.reduce((sum, file) => sum + Math.max(file.size, 1), 0), 1)
    onProgress({ percent: 1, transferredBytes: 0, totalBytes })
    this.ensureTransferActive(transferState)
    await controller.ensureRemoteDirectory(remoteRoot)

    if (directories.length) {
      onProgress({ percent: 3, transferredBytes: 0, totalBytes })
    }
    for (const directory of directories) {
      this.ensureTransferActive(transferState)
      await controller.ensureRemoteDirectory(path.posix.join(remoteRoot, ...directory.split(path.sep)))
    }

    if (!files.length) {
      onProgress({ percent: 100, transferredBytes: totalBytes, totalBytes })
      return
    }

    let uploadedBytes = 0

    for (const file of files) {
      this.ensureTransferActive(transferState)
      const remotePath = path.posix.join(remoteRoot, ...file.relativePath.split(path.sep))
      await controller.uploadFile(file.fullPath, remotePath, (fileProgress) => {
        const fileBytes = Math.max(file.size, 1)
        const completedBytes = uploadedBytes + Math.min(fileBytes, fileProgress.transferredBytes ?? Math.round((fileProgress.percent / 100) * fileBytes))
        onProgress({
          percent: Math.max(5, Math.min(99, Math.round((completedBytes / totalBytes) * 100))),
          transferredBytes: completedBytes,
          totalBytes
        })
      })
      uploadedBytes += Math.max(file.size, 1)
      onProgress({
        percent: Math.max(5, Math.min(99, Math.round((uploadedBytes / totalBytes) * 100))),
        transferredBytes: uploadedBytes,
        totalBytes
      })
    }
  }

  private async collectLocalUploadEntries(rootPath: string, currentPath = rootPath): Promise<{
    directories: string[]
    files: Array<{
      fullPath: string
      relativePath: string
      size: number
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
    const files: Array<{ fullPath: string; relativePath: string; size: number }> = []

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
        size: info.size
      })
    }

    return { directories, files }
  }

  private async collectRemoteDownloadEntries(
    controller: ReturnType<WorkspaceService['createController']>,
    rootPath: string,
    transferState: { canceled: boolean },
    currentPath = rootPath
  ): Promise<{
    directories: string[]
    files: Array<{
      remotePath: string
      relativePath: string
    }>
  }> {
    this.ensureTransferActive(transferState)
    const entries = (await controller.openRemotePath(currentPath)).filter((entry) => entry.name !== '..')
    const directories: string[] = []
    const files: Array<{ remotePath: string; relativePath: string }> = []

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

      files.push({
        remotePath: entry.path,
        relativePath
      })
    }

    return { directories, files }
  }

  private async refreshRemoteFiles(tabId: string) {
    await this.sessionRuntime.refreshRemoteFiles(tabId)
  }

  private async runRemoteFileMutation(
    tabId: string,
    action: (controller: LiveSessionController) => Promise<RemoteFileItem[]>
  ): Promise<WorkspaceSnapshot> {
    return this.sessionRuntime.runRemoteFileOperation(tabId, async (controller, current) => {
      const remoteFiles = await action(controller)
      const latest = this.sessionRuntime.get(tabId) ?? current
      this.sessionRuntime.set(tabId, {
        ...latest,
        remotePath: controller.getRemotePath(),
        fileAccessMode: controller.getFileAccessMode(),
        hasReusableSudoAuth: controller.type === 'ssh' ? controller.hasReusableSudoAuth() : false,
        remoteFiles
      })
      return this.getSnapshot()
    })
  }

  private addTransfer(direction: 'upload' | 'download', name: string, tabId: string, sender: WebContents) {
    const transferId = this.transfers.add(direction, name)
    this.transferTabs.set(transferId, tabId)
    this.emitTransferUpdate(transferId, sender)
    return transferId
  }

  private setTransferCancel(transferId: string, cancel: () => Promise<void> | void) {
    this.transferCancels.set(transferId, cancel)
  }

  private getDisconnectedTransferMessage() {
    const locale = this.options?.getLocale?.() ?? 'zhCN'
    return WorkspaceService.DISCONNECTED_TRANSFER_MESSAGES[locale]
  }

  private finalizeTransfersForTab(tabId: string, message: string) {
    const transferIds = [...this.transferTabs.entries()]
      .filter(([, mappedTabId]) => mappedTabId === tabId)
      .map(([transferId]) => transferId)

    if (!transferIds.length) {
      return
    }

    for (const transferId of transferIds) {
      const transfer = this.transfers.get(transferId)
      if (!transfer || (transfer.status !== 'running' && transfer.status !== 'queued')) {
        continue
      }

      this.transferTabs.delete(transferId)
      this.transferCancels.delete(transferId)
      this.transferCanceling.delete(transferId)
      this.clearTransferUpdateTimer(transferId)
      const didUpdate = this.transfers.update(transferId, {
        status: 'canceled',
        speed: undefined,
        message
      })
      if (didUpdate) {
        const nextTransfer = this.transfers.get(transferId)
        if (nextTransfer) {
          this.sessionRuntime.emitToTab(tabId, 'transfer:update', nextTransfer)
        }
      }
    }
  }

  private ensureTransferActive(transferState: { canceled: boolean }) {
    if (transferState.canceled) {
      throw new Error('传输已终止')
    }
  }

  private updateTransfer(
    transferId: string,
    patch: Partial<Pick<TransferTask, 'progress' | 'status' | 'message' | 'speed' | 'transferredBytes' | 'totalBytes'>>,
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
      this.sessionRuntime.emitToSender(sender, 'transfer:update', transfer)
    }
  }
}

function appendDisconnectedTranscript(transcript?: string) {
  const base = transcript ?? ''
  if (base.endsWith('连接已断开\r\n')) {
    return base
  }

  const separator = base && !base.endsWith('\n') ? '\r\n' : ''
  return `${base}${separator}连接已断开\r\n`
}

function isSkippableLocalReadError(error: unknown) {
  return Boolean(
    error
    && typeof error === 'object'
    && 'code' in error
    && (((error as { code?: string }).code === 'EACCES') || ((error as { code?: string }).code === 'EPERM'))
  )
}

function createTransferSpeedTracker() {
  const minSampleMs = 120
  const smoothingFactor = 0.35
  let sampleStartBytes: number | undefined
  let sampleStartTimestamp: number | undefined
  let smoothedBytesPerSecond: number | undefined
  let lastSpeed: string | undefined

  return (progress: TransferProgress) => {
    if (progress.transferredBytes === undefined) {
      return lastSpeed
    }

    const now = Date.now()
    if (sampleStartBytes === undefined || sampleStartTimestamp === undefined) {
      sampleStartBytes = progress.transferredBytes
      sampleStartTimestamp = now
      return lastSpeed
    }

    const deltaBytes = progress.transferredBytes - sampleStartBytes
    const deltaMs = now - sampleStartTimestamp

    if (deltaBytes <= 0 || deltaMs < minSampleMs) {
      return lastSpeed
    }

    const instantBytesPerSecond = deltaBytes / (deltaMs / 1000)
    smoothedBytesPerSecond = smoothedBytesPerSecond === undefined
      ? instantBytesPerSecond
      : (smoothedBytesPerSecond * (1 - smoothingFactor)) + (instantBytesPerSecond * smoothingFactor)

    lastSpeed = formatTransferSpeed(smoothedBytesPerSecond)
    sampleStartBytes = progress.transferredBytes
    sampleStartTimestamp = now

    return lastSpeed
  }
}

function formatTransferSpeed(bytesPerSecond: number) {
  if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) {
    return undefined
  }

  const units = ['B/s', 'KB/s', 'MB/s', 'GB/s']
  let value = bytesPerSecond
  let unitIndex = 0

  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex += 1
  }

  const digits = value >= 100 ? 0 : value >= 10 ? 1 : 2
  return `${value.toFixed(digits)} ${units[unitIndex]}`
}

export { seedCommandFolders, seedCommandTemplates, seedProfiles }
