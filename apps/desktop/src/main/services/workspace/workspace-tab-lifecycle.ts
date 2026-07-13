import { randomUUID } from 'node:crypto'
import type { WebContents } from 'electron'
import {
  getConnectionCapabilities,
  type ConnectionProfile,
  type RemoteFileAccessOptions,
  type SessionSnapshot,
  type WorkspaceSnapshot
} from '@fileterm/core'
import type { ProfileRepository } from '@fileterm/storage'
import { appWarn } from '../app-logger.js'
import type { LiveSessionController } from './workspace-session-runtime.js'
import { WorkspaceSessionRuntime } from './workspace-session-runtime.js'
import { WorkspaceTabsState } from './workspace-tabs.js'

export interface WorkspaceTabLifecycleOptions {
  tabs: WorkspaceTabsState
  sessionRuntime: WorkspaceSessionRuntime
  profileRepository: ProfileRepository
  privilegedAccess: Map<string, RemoteFileAccessOptions>
  getSnapshot(): Promise<WorkspaceSnapshot>
  createController(tabId: string, profile: ConnectionProfile, initialTranscript?: string): LiveSessionController
  finalizeTransfersForTab(tabId: string, message: string): Promise<void>
  getDisconnectedTransferMessage(): string
}

export class WorkspaceTabLifecycleService {
  constructor(private readonly options: WorkspaceTabLifecycleOptions) {}

  bindWorkspaceSender(sender: WebContents) {
    for (const tab of this.options.tabs.list()) {
      this.options.sessionRuntime.setSender(tab.id, sender)
      void this.options.sessionRuntime.restoreTabData(tab.id)
    }
  }

  async openProfile(profileId: string, sender: WebContents): Promise<WorkspaceSnapshot> {
    const profile = await this.options.profileRepository.getById(profileId)
    if (!profile) {
      throw new Error(`Profile not found: ${profileId}`)
    }

    const tabId = randomUUID()
    this.options.tabs.open(tabId, profile)
    this.options.sessionRuntime.setSender(tabId, sender)
    const controller = this.options.createController(tabId, profile)

    this.options.sessionRuntime.set(tabId, createInitialSessionSnapshot(profile, controller))
    void this.options.sessionRuntime.connect(tabId, controller).catch((error) => {
      appWarn('[FileTerm][Session] Connection failed', error)
    })
    void this.options.profileRepository.touchProfile(profileId)

    return this.options.getSnapshot()
  }

  async reconnectTab(tabId: string, sender: WebContents): Promise<WorkspaceSnapshot> {
    const tab = this.options.tabs.getById(tabId)
    if (!tab) {
      throw new Error(`Tab not found: ${tabId}`)
    }

    const currentSender = this.options.sessionRuntime.getSender(tabId)
    const reusableSender = currentSender && !currentSender.isDestroyed() ? currentSender : sender
    if (!reusableSender || reusableSender.isDestroyed()) {
      throw new Error(`Tab sender unavailable: ${tabId}`)
    }
    this.options.sessionRuntime.setSender(tabId, reusableSender)

    await this.options.finalizeTransfersForTab(tabId, this.options.getDisconnectedTransferMessage())
    await this.options.sessionRuntime.disconnect(tabId)

    const profile = await this.options.profileRepository.getById(tab.profileId)
    if (!profile) {
      throw new Error(`Profile not found: ${tab.profileId}`)
    }

    const current = this.options.sessionRuntime.get(tabId)
    const disconnectedTranscript = appendDisconnectedTranscript(current?.terminalTranscript)
    this.options.sessionRuntime.set(tabId, {
      profileId: profile.id,
      accessHost: profile.host,
      summary: current?.accessHost ? `Disconnected from ${current.accessHost}` : 'Disconnected',
      terminalTranscript: disconnectedTranscript,
      remotePath: current?.remotePath ?? profile.remotePath,
      shellCwd: current?.shellCwd,
      followShellCwd: current?.followShellCwd ?? profile.type === 'ssh',
      remoteFiles: [],
      fileAccessMode: 'user',
      sudoUser: current?.sudoUser ?? (profile.type === 'ssh' ? 'root' : undefined),
      hasReusableSudoAuth: false,
      connected: false,
      systemMetrics: undefined
    })

    const controller = this.options.createController(tabId, profile, disconnectedTranscript)
    this.options.sessionRuntime.set(
      tabId,
      createInitialSessionSnapshot(profile, controller, {
        remotePath: current?.remotePath ?? profile.remotePath,
        shellCwd: current?.shellCwd,
        followShellCwd: current?.followShellCwd ?? profile.type === 'ssh',
        sudoUser: current?.sudoUser ?? (profile.type === 'ssh' ? 'root' : undefined)
      })
    )
    this.options.tabs.updateStatus(tabId, 'connecting')
    this.options.tabs.activate(tabId)

    void this.options.sessionRuntime.connect(tabId, controller).catch((error) => {
      appWarn('[FileTerm][Session] Reconnection failed', error)
    })
    await this.options.sessionRuntime.emitSnapshot(reusableSender)
    return this.options.getSnapshot()
  }

  async activateTab(tabId: string): Promise<WorkspaceSnapshot> {
    if (!this.options.tabs.has(tabId)) {
      return this.options.getSnapshot()
    }
    this.options.tabs.activate(tabId)
    return this.options.getSnapshot()
  }

  async closeTab(tabId: string): Promise<WorkspaceSnapshot> {
    await this.options.finalizeTransfersForTab(tabId, '标签已关闭，传输已暂停，可手动继续')
    await this.options.sessionRuntime.teardown(tabId)
    this.options.privilegedAccess.delete(tabId)
    this.options.tabs.remove(tabId)
    return this.options.getSnapshot()
  }

  async disconnectTab(tabId: string): Promise<WorkspaceSnapshot> {
    const tab = this.options.tabs.getById(tabId)
    const current = this.options.sessionRuntime.get(tabId)
    if (!tab || !current) {
      throw new Error(`Tab not found: ${tabId}`)
    }

    await this.options.finalizeTransfersForTab(tabId, '连接已主动断开，传输已暂停，可手动继续')
    await this.options.sessionRuntime.disconnect(tabId)
    this.options.privilegedAccess.delete(tabId)
    const latest = this.options.sessionRuntime.get(tabId) ?? current
    this.options.sessionRuntime.set(tabId, {
      ...latest,
      summary: latest.accessHost ? `Disconnected from ${latest.accessHost}` : 'Disconnected',
      terminalTranscript: appendDisconnectedTranscript(latest.terminalTranscript),
      remoteFiles: [],
      fileAccessMode: 'user',
      hasReusableSudoAuth: false,
      connected: false,
      systemMetrics: undefined
    })
    this.options.tabs.updateStatus(tabId, 'closed')
    await this.options.sessionRuntime.emitSnapshotForTab(tabId)
    return this.options.getSnapshot()
  }
}

function createInitialSessionSnapshot(
  profile: ConnectionProfile,
  controller: LiveSessionController,
  overrides?: Partial<Pick<SessionSnapshot, 'remotePath' | 'shellCwd' | 'followShellCwd' | 'sudoUser'>>
): SessionSnapshot {
  const isFileController = controller.type === 'ssh' || controller.type === 'ftp'
  const isTerminalController = controller.type === 'ssh' || controller.type === 'telnet' || controller.type === 'serial'
  return {
    profileId: profile.id,
    accessHost: profile.type === 'serial' ? profile.devicePath : profile.host,
    summary: profile.type === 'ssh' ? '连接主机...' : controller.getSummary(),
    terminalTranscript: isTerminalController ? controller.getTerminalTranscript() : undefined,
    remotePath: overrides?.remotePath ?? (isFileController ? controller.getRemotePath() : ''),
    shellCwd: overrides?.shellCwd ?? (controller.type === 'ssh' ? controller.getShellCwd() : undefined),
    followShellCwd: overrides?.followShellCwd ?? profile.type === 'ssh',
    remoteFiles: [],
    fileAccessMode: isFileController ? controller.getFileAccessMode() : undefined,
    sudoUser: overrides?.sudoUser ?? (profile.type === 'ssh' ? 'root' : undefined),
    hasReusableSudoAuth: controller.type === 'ssh' ? controller.hasReusableSudoAuth() : false,
    connected: false,
    capabilities: getConnectionCapabilities(profile),
    reconnectMode: profile.type === 'ssh' ? (profile.reconnectMode ?? 'none') : undefined
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
