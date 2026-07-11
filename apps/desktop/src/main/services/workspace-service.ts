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
  type TransferTargetOptions,
  type WorkspaceSessionTabEvent,
  type WorkspaceSnapshot
} from '@fileterm/core'
import type { ProfileRepository } from '@fileterm/storage'
import { seedCommandFolders, seedCommandTemplates, seedProfiles } from './workspace/seed-data.js'
import { WorkspaceSessionRuntime, type LiveSessionController } from './workspace/workspace-session-runtime.js'
import { WorkspaceTabLifecycleService } from './workspace/workspace-tab-lifecycle.js'
import { WorkspaceTabsState } from './workspace/workspace-tabs.js'
import type { TransferJournal } from './transfers/transfer-journal.js'
import { TransferService } from './transfers/transfer-service.js'
import { appWarn } from './app-logger.js'

export class WorkspaceService {
  private readonly profileRepository: ProfileRepository
  private readonly tabs = new WorkspaceTabsState()
  private readonly sessionRuntime: WorkspaceSessionRuntime
  private readonly transferService: TransferService
  private readonly tabLifecycle: WorkspaceTabLifecycleService
  private shutdownPromise?: Promise<void>
  private readonly privilegedAccess = new Map<string, RemoteFileAccessOptions>()
  private readonly handleSessionTabEvent = (event: WorkspaceSessionTabEvent) => {
    if (event.type === 'status-changed') {
      this.tabs.updateStatus(event.tabId, event.status)
      return
    }
    if (event.type === 'disconnected') {
      this.privilegedAccess.delete(event.tabId)
      void this.transferService
        .finalizeTransfersForTab(event.tabId, this.transferService.getDisconnectedTransferMessage())
        .catch((error) => {
          appWarn(`[FileTerm][Workspace] Failed to finalize transfers for disconnected tab ${event.tabId}`, error)
        })
    }
  }

  constructor(
    profileRepository: ProfileRepository,
    options?: {
      getLocale?(): 'zhCN' | 'enUS'
      transferJournal?: TransferJournal
    }
  ) {
    this.profileRepository = profileRepository
    this.sessionRuntime = new WorkspaceSessionRuntime({
      getSnapshot: () => this.getSnapshot(),
      getTabStatus: (tabId) => this.tabs.getById(tabId)?.status,
      rememberTrustedHostFingerprint: (profileId, fingerprint) =>
        this.rememberTrustedHostFingerprint(profileId, fingerprint)
    })
    this.transferService = new TransferService({
      tabs: this.tabs,
      sessionRuntime: this.sessionRuntime,
      getSnapshot: () => this.getSnapshot(),
      getLocale: () => options?.getLocale?.() ?? 'zhCN',
      transferJournal: options?.transferJournal
    })
    this.sessionRuntime.on('tab-event', this.handleSessionTabEvent)
    this.tabLifecycle = new WorkspaceTabLifecycleService({
      tabs: this.tabs,
      sessionRuntime: this.sessionRuntime,
      profileRepository: this.profileRepository,
      privilegedAccess: this.privilegedAccess,
      getSnapshot: () => this.getSnapshot(),
      createController: (tabId, profile, initialTranscript) => this.createController(tabId, profile, initialTranscript),
      finalizeTransfersForTab: (tabId, message) => this.transferService.finalizeTransfersForTab(tabId, message),
      getDisconnectedTransferMessage: () => this.transferService.getDisconnectedTransferMessage()
    })
  }

  shutdown(): Promise<void> {
    if (!this.shutdownPromise) {
      this.shutdownPromise = this.performShutdown()
    }
    return this.shutdownPromise
  }

  private async performShutdown() {
    await this.transferService.shutdown()
    this.sessionRuntime.off('tab-event', this.handleSessionTabEvent)
    await this.sessionRuntime.shutdown()
    await this.transferService.flushJournal()
  }

  async getSnapshot(): Promise<WorkspaceSnapshot> {
    await this.transferService.waitUntilReady()
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
      transfers: this.transferService.list(),
      sessions: this.sessionRuntime.list()
    }
  }

  async getConnectionLibrary(): Promise<ConnectionLibrarySnapshot> {
    const [profiles, folders] = await Promise.all([this.profileRepository.list(), this.profileRepository.listFolders()])
    return {
      profiles,
      folders
    }
  }

  bindWorkspaceSender(sender: WebContents) {
    this.tabLifecycle.bindWorkspaceSender(sender)
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
    return this.tabLifecycle.openProfile(profileId, sender)
  }

  async reconnectTab(tabId: string, sender: WebContents): Promise<WorkspaceSnapshot> {
    return this.tabLifecycle.reconnectTab(tabId, sender)
  }

  async activateTab(tabId: string): Promise<WorkspaceSnapshot> {
    return this.tabLifecycle.activateTab(tabId)
  }

  async closeTab(tabId: string): Promise<WorkspaceSnapshot> {
    return this.tabLifecycle.closeTab(tabId)
  }

  async disconnectTab(tabId: string): Promise<WorkspaceSnapshot> {
    return this.tabLifecycle.disconnectTab(tabId)
  }

  async resolveSshInteraction(requestId: string, response: SshInteractionResponse): Promise<void> {
    this.sessionRuntime.resolveSshInteraction(requestId, response)
  }

  async rememberTrustedHostFingerprint(profileId: string, fingerprint: string): Promise<void> {
    await this.profileRepository.updateTrustedHostFingerprint?.(profileId, fingerprint)
  }

  async queueUpload(fileNames: string[]): Promise<WorkspaceSnapshot> {
    return this.transferService.queueUpload(fileNames)
  }

  async cancelTransfer(transferId: string, sender: WebContents): Promise<WorkspaceSnapshot> {
    return this.transferService.cancelTransfer(transferId, sender)
  }

  async pauseTransfer(transferId: string, sender: WebContents): Promise<WorkspaceSnapshot> {
    return this.transferService.pauseTransfer(transferId, sender)
  }

  async discardTransfer(transferId: string, sender: WebContents): Promise<WorkspaceSnapshot> {
    return this.transferService.discardTransfer(transferId, sender)
  }

  async resumeTransfer(transferId: string, sender: WebContents): Promise<WorkspaceSnapshot> {
    return this.transferService.resumeTransfer(transferId, sender)
  }

  async clearTransfers(transferIds: string[]): Promise<WorkspaceSnapshot> {
    return this.transferService.clearTransfers(transferIds)
  }

  async uploadFile(
    tabId: string,
    localPath: string,
    remoteDirectory: string,
    sender: WebContents,
    options?: TransferTargetOptions
  ): Promise<WorkspaceSnapshot> {
    return this.transferService.uploadFile(tabId, localPath, remoteDirectory, sender, options)
  }

  async downloadFile(
    tabId: string,
    remotePath: string,
    localDirectory: string,
    sender: WebContents,
    options?: TransferTargetOptions
  ): Promise<WorkspaceSnapshot> {
    return this.transferService.downloadFile(tabId, remotePath, localDirectory, sender, options)
  }

  async downloadRemotePath(
    tabId: string,
    remotePath: string,
    targetType: 'file' | 'folder',
    localDirectory: string,
    sender: WebContents,
    options?: TransferTargetOptions
  ): Promise<WorkspaceSnapshot> {
    return this.transferService.downloadRemotePath(tabId, remotePath, targetType, localDirectory, sender, options)
  }

  async readRemoteFile(tabId: string, targetPath: string, encoding?: string): Promise<string> {
    return this.sessionRuntime.runRemoteFileOperation(tabId, (controller) =>
      controller.readRemoteFile(targetPath, encoding)
    )
  }

  async writeRemoteFile(
    tabId: string,
    targetPath: string,
    content: string,
    encoding?: string
  ): Promise<WorkspaceSnapshot> {
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

  async copyRemotePath(
    tabId: string,
    targetPath: string,
    destinationPath: string,
    targetType: 'file' | 'folder'
  ): Promise<WorkspaceSnapshot> {
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

  async changeRemotePermissions(
    tabId: string,
    targetPath: string,
    options: PermissionChangeOptions
  ): Promise<WorkspaceSnapshot> {
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

  async setRemoteFileAccessMode(
    tabId: string,
    mode: 'user' | 'root',
    options?: RemoteFileAccessOptions
  ): Promise<WorkspaceSnapshot> {
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
}

export { seedCommandFolders, seedCommandTemplates, seedProfiles }
