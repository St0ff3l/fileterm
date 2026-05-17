import type { WebContents } from 'electron'
import type {
  ConnectionProfile,
  SessionSnapshot,
  WorkspaceSnapshot,
  WorkspaceTab
} from '@termdock/core'
import { LiveFtpSessionController, LiveSshSessionController } from '../session-controllers.js'

type LiveSessionController = LiveSshSessionController | LiveFtpSessionController

export class WorkspaceSessionRuntime {
  private static readonly NETWORK_HISTORY_LIMIT = 600
  private readonly sessions = new Map<string, SessionSnapshot>()
  private readonly liveControllers = new Map<string, LiveSessionController>()
  private readonly metricsPollers = new Map<string, ReturnType<typeof setInterval>>()
  private readonly metricsRefreshInFlight = new Set<string>()
  private readonly tabSenders = new Map<string, WebContents>()

  constructor(
    private readonly options: {
      getSnapshot(): Promise<WorkspaceSnapshot>
      updateTabStatus(tabId: string, status: WorkspaceTab['status']): void
      getTabStatus(tabId: string): WorkspaceTab['status'] | undefined
    }
  ) {}

  list() {
    return Object.fromEntries(this.sessions.entries())
  }

  get(tabId: string) {
    return this.sessions.get(tabId)
  }

  set(tabId: string, snapshot: SessionSnapshot) {
    this.sessions.set(tabId, snapshot)
  }

  setSender(tabId: string, sender: WebContents) {
    this.tabSenders.set(tabId, sender)
    sender.once('destroyed', () => {
      this.handleSenderDestroyed(sender)
    })
  }

  getSender(tabId: string) {
    return this.tabSenders.get(tabId)
  }

  async teardown(tabId: string) {
    this.stopMetricsPolling(tabId)
    await this.liveControllers.get(tabId)?.disconnect()
    this.liveControllers.delete(tabId)
    this.tabSenders.delete(tabId)
    this.sessions.delete(tabId)
  }

  async disconnect(tabId: string) {
    this.stopMetricsPolling(tabId)
    await this.liveControllers.get(tabId)?.disconnect()
    this.liveControllers.delete(tabId)
  }

  requireController(tabId: string) {
    const controller = this.liveControllers.get(tabId)
    if (!controller) {
      throw new Error(`Session not found: ${tabId}`)
    }
    return controller
  }

  createController(tabId: string, profile: ConnectionProfile): LiveSessionController {
    return profile.type === 'ssh'
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
            this.options.updateTabStatus(
              tabId,
              statusFromTerminalState(summary, connected, this.options.getTabStatus(tabId))
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
      : new LiveFtpSessionController(tabId, profile)
  }

  async connect(tabId: string, controller: LiveSessionController) {
    try {
      await controller.connect()
      this.liveControllers.set(tabId, controller)

      let files = [] as Awaited<ReturnType<LiveSessionController['listRemoteFiles']>>
      let remoteFilesError: string | null = null
      try {
        files = await controller.listRemoteFiles()
      } catch (error) {
        remoteFilesError = error instanceof Error ? error.message : '远程目录读取失败'
      }

      let systemMetrics
      if (controller.type === 'ssh') {
        systemMetrics = await controller.refreshSystemMetrics()
        if (remoteFilesError) {
          controller.pushClientNotice(`SFTP 初始化失败: ${remoteFilesError}`)
        }
      }

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
        systemMetrics: systemMetrics ? this.mergeNetworkHistory(undefined, systemMetrics) : undefined
      })
      this.options.updateTabStatus(tabId, 'connected')
      if (controller.type === 'ssh') {
        const profile = controller['profile']
        if (profile.type !== 'ssh' || profile.enableExecChannel !== false) {
          this.startMetricsPolling(tabId, controller)
        }
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
      this.options.updateTabStatus(tabId, 'error')
      this.stopMetricsPolling(tabId)
    }

    void this.emitSnapshotForTab(tabId)
  }

  async refreshRemoteFiles(tabId: string) {
    const controller = this.requireController(tabId)
    const current = this.sessions.get(tabId)
    if (!current) {
      return
    }

    const remoteFiles = await controller.listRemoteFiles()
    this.sessions.set(tabId, {
      ...current,
      remotePath: controller.getRemotePath(),
      remoteFiles
    })
  }

  async openRemotePath(tabId: string, targetPath: string) {
    const controller = this.requireController(tabId)
    const current = this.sessions.get(tabId)
    if (!current) {
      throw new Error(`Session not found: ${tabId}`)
    }

    const remoteFiles = await controller.openRemotePath(targetPath)
    this.sessions.set(tabId, {
      ...current,
      remotePath: controller.getRemotePath(),
      remoteFiles
    })
  }

  async emitSnapshot(sender: WebContents) {
    if (sender.isDestroyed()) {
      this.handleSenderDestroyed(sender)
      return
    }
    sender.send('workspace:snapshot', await this.options.getSnapshot())
  }

  async emitSnapshotForTab(tabId: string) {
    const sender = this.tabSenders.get(tabId)
    if (!sender || sender.isDestroyed()) {
      this.handleSenderDestroyed(sender)
      this.tabSenders.delete(tabId)
      this.stopMetricsPolling(tabId)
      return
    }
    await this.emitSnapshot(sender)
  }

  private startMetricsPolling(tabId: string, controller: LiveSshSessionController) {
    this.stopMetricsPolling(tabId)
    const timer = setInterval(() => {
      void this.refreshMetricsForTab(tabId, controller)
    }, 1000)
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
        systemMetrics: this.mergeNetworkHistory(latest.systemMetrics, systemMetrics)
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

  private mergeNetworkHistory(
    previousMetrics: SessionSnapshot['systemMetrics'] | undefined,
    nextMetrics: NonNullable<SessionSnapshot['systemMetrics']>
  ) {
    const nextPoint = nextMetrics.networkSamples.at(-1) ?? { rx: 0, tx: 0 }
    const previousSamples = previousMetrics?.networkSamples ?? []
    const previousByInterface = previousMetrics?.networkSamplesByInterface ?? {}
    const nextByInterface = nextMetrics.networkSamplesByInterface ?? {}
    const mergedByInterface = Object.fromEntries(
      Object.entries(nextByInterface).map(([name, samples]) => {
        const nextInterfacePoint = samples.at(-1) ?? { rx: 0, tx: 0 }
        const previousInterfaceSamples = previousByInterface[name] ?? []
        return [
          name,
          [...previousInterfaceSamples, nextInterfacePoint].slice(-WorkspaceSessionRuntime.NETWORK_HISTORY_LIMIT)
        ]
      })
    )

    return {
      ...nextMetrics,
      networkSamples: [...previousSamples, nextPoint].slice(-WorkspaceSessionRuntime.NETWORK_HISTORY_LIMIT),
      networkSamplesByInterface: mergedByInterface
    }
  }
}

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
