import { randomUUID } from 'node:crypto'
import type { WebContents } from 'electron'
import {
  createTabLayout,
  type ConnectionProfile,
  type CreateProfileInput,
  type SessionSnapshot,
  type TransferTask,
  type WorkspaceSnapshot,
  type WorkspaceTab
} from '@termdock/core'
import type { ProfileRepository } from '@termdock/storage'
import {
  LiveSshSessionController,
  MockFtpSessionController,
} from './session-controllers.js'

const seedProfiles: ConnectionProfile[] = [
  {
    id: 'profile-ssh-prod',
    type: 'ssh',
    name: 'prod-web-01',
    host: '10.0.0.21',
    port: 22,
    username: 'root',
    authType: 'privateKey',
    privateKeyPath: '~/.ssh/id_ed25519',
    group: 'Production',
    sftpEnabled: true,
    remotePath: '/srv/www'
  },
  {
    id: 'profile-ssh-nas',
    type: 'ssh',
    name: 'nas-storage',
    host: '10.0.0.44',
    port: 22,
    username: 'admin',
    authType: 'password',
    group: 'Staging',
    sftpEnabled: true,
    remotePath: '/volume1'
  },
  {
    id: 'profile-ftp-archive',
    type: 'ftp',
    name: 'archive-ftp',
    host: 'ftp.example.net',
    port: 21,
    username: 'deploy',
    secure: false,
    group: 'FTP Sites',
    remotePath: '/incoming'
  }
]

const seedTransfers: TransferTask[] = [
  { id: 'transfer-1', direction: 'upload', name: 'release.tar.gz', progress: 72, status: 'running' },
  { id: 'transfer-2', direction: 'download', name: 'backup.sql.gz', progress: 31, status: 'running' }
]

export class WorkspaceService {
  private static readonly METRICS_POLL_INTERVAL_MS = 1000

  private readonly profileRepository: ProfileRepository
  private tabs: WorkspaceTab[] = []
  private activeTabId: string | null = null
  private readonly sessions = new Map<string, SessionSnapshot>()
  private readonly liveControllers = new Map<string, LiveSshSessionController | MockFtpSessionController>()
  private readonly metricsPollers = new Map<string, ReturnType<typeof setInterval>>()
  private readonly metricsRefreshInFlight = new Set<string>()
  private readonly tabSenders = new Map<string, WebContents>()
  private readonly transfers = [...seedTransfers]

  constructor(profileRepository: ProfileRepository) {
    this.profileRepository = profileRepository
  }

  async getSnapshot(): Promise<WorkspaceSnapshot> {
    return {
      profiles: await this.profileRepository.list(),
      tabs: [...this.tabs],
      activeTabId: this.activeTabId,
      transfers: [...this.transfers],
      sessions: Object.fromEntries(this.sessions.entries())
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

  async openProfile(profileId: string, sender: WebContents): Promise<WorkspaceSnapshot> {
    const profile = await this.profileRepository.getById(profileId)
    if (!profile) {
      throw new Error(`Profile not found: ${profileId}`)
    }

    const tabId = randomUUID()
    const tab: WorkspaceTab = {
      id: tabId,
      profileId: profile.id,
      sessionType: profile.type,
      title: profile.name,
      layout: createTabLayout(profile),
      status: 'connecting'
    }

    this.tabs = [...this.tabs, tab]
    this.activeTabId = tabId
    this.tabSenders.set(tabId, sender)
    sender.once('destroyed', () => {
      this.handleSenderDestroyed(sender)
    })

    const controller =
      profile.type === 'ssh'
        ? new LiveSshSessionController(
            tabId,
            profile,
            (chunk) => {
              this.sendToTab(tabId, 'terminal:data', { tabId, chunk })
            },
            (summary, transcript, connected) => {
              const current = this.sessions.get(tabId)
              if (!current) {
                return
              }
              this.sessions.set(tabId, {
                ...current,
                summary,
                terminalTranscript: transcript,
                connected
              })
              this.updateTabStatus(
                tabId,
                statusFromTerminalState(summary, connected, this.tabs.find((tab) => tab.id === tabId)?.status)
              )
              this.sendToTab(tabId, 'terminal:state', {
                tabId,
                summary,
                transcript,
                connected
              })
              void this.emitSnapshotForTab(tabId)
            }
          )
        : new MockFtpSessionController(tabId, profile)

    const snapshot: SessionSnapshot = {
      profileId: profile.id,
      accessHost: profile.host,
      summary: profile.type === 'ssh' ? '连接主机...' : controller.getSummary(),
      terminalTranscript:
        controller.type === 'ssh' ? controller.getTerminalTranscript() : undefined,
      remotePath: controller.getRemotePath(),
      remoteFiles: [],
      connected: false
    }

    this.sessions.set(tabId, snapshot)

    void this.connectSession(tabId, controller)

    return this.getSnapshot()
  }

  async activateTab(tabId: string): Promise<WorkspaceSnapshot> {
    const tabExists = this.tabs.some((tab) => tab.id === tabId)
    if (!tabExists) {
      throw new Error(`Tab not found: ${tabId}`)
    }

    this.activeTabId = tabId
    return this.getSnapshot()
  }

  async closeTab(tabId: string): Promise<WorkspaceSnapshot> {
    this.stopMetricsPolling(tabId)
    await this.liveControllers.get(tabId)?.disconnect()
    this.liveControllers.delete(tabId)
    this.tabSenders.delete(tabId)
    this.tabs = this.tabs.filter((tab) => tab.id !== tabId)
    this.sessions.delete(tabId)

    if (this.activeTabId === tabId) {
      this.activeTabId = this.tabs.at(-1)?.id ?? null
    }

    return this.getSnapshot()
  }

  async queueUpload(fileNames: string[]): Promise<WorkspaceSnapshot> {
    const queued = fileNames.map((name, index) => ({
      id: randomUUID(),
      direction: 'upload' as const,
      name,
      progress: index === 0 ? 12 : 0,
      status: index === 0 ? 'running' as const : 'queued' as const
    }))

    this.transfers.unshift(...queued)
    return this.getSnapshot()
  }

  async writeToTerminal(tabId: string, data: string): Promise<void> {
    const controller = this.liveControllers.get(tabId)
    if (!controller || controller.type !== 'ssh') {
      return
    }
    await controller.write(data)
  }

  async resizeTerminal(tabId: string, cols: number, rows: number): Promise<void> {
    const controller = this.liveControllers.get(tabId)
    if (!controller || controller.type !== 'ssh') {
      return
    }
    await controller.resize(cols, rows)
  }

  async openRemotePath(tabId: string, targetPath: string): Promise<WorkspaceSnapshot> {
    const controller = this.liveControllers.get(tabId)
    const current = this.sessions.get(tabId)
    if (!controller || !current) {
      throw new Error(`Session not found: ${tabId}`)
    }

    const remoteFiles = await controller.openRemotePath(targetPath)
    this.sessions.set(tabId, {
      ...current,
      remotePath: controller.getRemotePath(),
      remoteFiles
    })

    return this.getSnapshot()
  }

  private async connectSession(
    tabId: string,
    controller: LiveSshSessionController | MockFtpSessionController
  ) {
    try {
      await controller.connect()
      this.liveControllers.set(tabId, controller)

      const files = await controller.listRemoteFiles()
      const systemMetrics =
        controller.type === 'ssh' ? await controller.refreshSystemMetrics() : undefined
      const current = this.sessions.get(tabId)
      if (!current) {
        return
      }

      this.sessions.set(tabId, {
        ...current,
        summary: controller.getSummary(),
        terminalTranscript:
          controller.type === 'ssh' ? controller.getTerminalTranscript() : undefined,
        remotePath: controller.getRemotePath(),
        remoteFiles: files,
        connected: true,
        systemMetrics
      })
      this.updateTabStatus(tabId, 'connected')
      if (controller.type === 'ssh') {
        this.startMetricsPolling(tabId, controller)
      }
    } catch (error) {
      const current = this.sessions.get(tabId)
      if (current) {
        const message = error instanceof Error ? error.message : '未知错误'
        this.sessions.set(tabId, {
          ...current,
          summary: `连接失败: ${message}`,
          terminalTranscript:
            controller.type === 'ssh' ? controller.getTerminalTranscript() : current.terminalTranscript,
          connected: false
        })
      }
      this.updateTabStatus(tabId, 'error')
      this.stopMetricsPolling(tabId)
    }

    void this.emitSnapshotForTab(tabId)
  }

  private startMetricsPolling(tabId: string, controller: LiveSshSessionController) {
    this.stopMetricsPolling(tabId)
    const timer = setInterval(() => {
      void this.refreshMetricsForTab(tabId, controller)
    }, WorkspaceService.METRICS_POLL_INTERVAL_MS)
    this.metricsPollers.set(tabId, timer)
  }

  private stopMetricsPolling(tabId: string) {
    const timer = this.metricsPollers.get(tabId)
    if (timer) {
      clearInterval(timer)
      this.metricsPollers.delete(tabId)
    }
    this.metricsRefreshInFlight.delete(tabId)
  }

  private async refreshMetricsForTab(tabId: string, controller: LiveSshSessionController) {
    if (this.metricsRefreshInFlight.has(tabId)) {
      return
    }

    const current = this.sessions.get(tabId)
    const sender = this.tabSenders.get(tabId)
    if (!current || !sender || !current.connected) {
      this.stopMetricsPolling(tabId)
      return
    }

    this.metricsRefreshInFlight.add(tabId)
    try {
      const systemMetrics = await controller.refreshSystemMetrics()
      if (!systemMetrics) {
        return
      }

      const latest = this.sessions.get(tabId)
      if (!latest) {
        return
      }

      this.sessions.set(tabId, {
        ...latest,
        systemMetrics
      })

      await this.emitSnapshot(sender)
    } finally {
      this.metricsRefreshInFlight.delete(tabId)
    }
  }

  private sendToTab(tabId: string, channel: string, payload: unknown) {
    const sender = this.tabSenders.get(tabId)
    if (!sender || sender.isDestroyed()) {
      this.handleSenderDestroyed(sender)
      this.tabSenders.delete(tabId)
      this.stopMetricsPolling(tabId)
      return
    }
    sender.send(channel, payload)
  }

  private async emitSnapshotForTab(tabId: string) {
    const sender = this.tabSenders.get(tabId)
    if (!sender || sender.isDestroyed()) {
      this.handleSenderDestroyed(sender)
      this.tabSenders.delete(tabId)
      this.stopMetricsPolling(tabId)
      return
    }
    await this.emitSnapshot(sender)
  }

  private handleSenderDestroyed(sender?: WebContents) {
    if (!sender) {
      return
    }
    for (const [tabId, candidate] of this.tabSenders.entries()) {
      if (candidate === sender) {
        this.tabSenders.delete(tabId)
        this.stopMetricsPolling(tabId)
      }
    }
  }

  private updateTabStatus(tabId: string, status: WorkspaceTab['status']) {
    this.tabs = this.tabs.map((tab) => (tab.id === tabId ? { ...tab, status } : tab))
  }

  private async emitSnapshot(sender: WebContents) {
    if (sender.isDestroyed()) {
      this.handleSenderDestroyed(sender)
      return
    }
    sender.send('workspace:snapshot', await this.getSnapshot())
  }
}

export { seedProfiles }

function statusFromTerminalState(
  summary: string,
  connected: boolean,
  currentStatus?: WorkspaceTab['status']
): WorkspaceTab['status'] {
  if (connected) {
    return 'connected'
  }

  if (currentStatus === 'error') {
    return 'error'
  }

  const normalized = summary.toLowerCase()
  if (summary.includes('失败') || normalized.includes('error')) {
    return 'error'
  }

  return 'closed'
}
