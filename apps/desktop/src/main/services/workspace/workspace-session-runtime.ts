import { randomUUID } from 'node:crypto'
import type { WebContents } from 'electron'
import type {
  ConnectionProfile,
  RemoteFileAccessOptions,
  SessionSnapshot,
  SystemMetrics,
  SessionMetricsUpdate,
  SshInteractionDraft,
  SshInteractionRequest,
  SshInteractionResponse,
  WorkspaceSnapshot,
  WorkspaceTab
} from '@termdock/core'
import { LiveFtpSessionController, LiveSshSessionController } from '../session-controllers.js'

type LiveSessionController = LiveSshSessionController | LiveFtpSessionController

export class WorkspaceSessionRuntime {
  private static readonly NETWORK_HISTORY_LIMIT = 600
  private static readonly TERMINAL_TRANSCRIPT_LIMIT = 200_000
  private readonly sessions = new Map<string, SessionSnapshot>()
  private readonly liveControllers = new Map<string, LiveSessionController>()
  private readonly metricsPollers = new Map<string, ReturnType<typeof setInterval>>()
  private readonly metricsRefreshInFlight = new Set<string>()
  private readonly tabSenders = new Map<string, WebContents>()
  private readonly invalidSenders = new WeakSet<WebContents>()
  private readonly senderLifecycleListeners = new WeakSet<WebContents>()
  private readonly pendingSshInteractions = new Map<string, {
    tabId: string
    resolve(response: SshInteractionResponse): void
    reject(error: Error): void
  }>()

  constructor(
    private readonly options: {
      getSnapshot(): Promise<WorkspaceSnapshot>
      updateTabStatus(tabId: string, status: WorkspaceTab['status']): void
      getTabStatus(tabId: string): WorkspaceTab['status'] | undefined
      rememberTrustedHostFingerprint(profileId: string, fingerprint: string): Promise<void>
      onTabDisconnected?(tabId: string, summary: string): void | Promise<void>
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
    this.invalidSenders.delete(sender)
    this.tabSenders.set(tabId, sender)
    this.attachSenderLifecycleListeners(sender)

    const controller = this.liveControllers.get(tabId)
    const session = this.sessions.get(tabId)
    if (
      controller?.type === 'ssh'
      && session?.connected
      && !this.metricsPollers.has(tabId)
      && this.shouldPollMetrics(controller)
    ) {
      this.startMetricsPolling(tabId, controller)
    }
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

  async shutdown() {
    for (const tabId of this.metricsPollers.keys()) {
      this.stopMetricsPolling(tabId)
    }

    for (const [requestId, pending] of this.pendingSshInteractions.entries()) {
      this.pendingSshInteractions.delete(requestId)
      pending.reject(new Error('Workspace runtime is shutting down'))
    }

    const controllerEntries = [...this.liveControllers.entries()]
    this.liveControllers.clear()

    await Promise.allSettled(
      controllerEntries.map(async ([tabId, controller]) => {
        try {
          await controller.disconnect()
        } finally {
          this.tabSenders.delete(tabId)
        }
      })
    )
  }

  requireController(tabId: string) {
    const controller = this.liveControllers.get(tabId)
    if (!controller) {
      throw new Error(`Session not found: ${tabId}`)
    }
    return controller
  }

  getController(tabId: string) {
    return this.liveControllers.get(tabId)
  }

  resolveSshInteraction(requestId: string, response: SshInteractionResponse) {
    const pending = this.pendingSshInteractions.get(requestId)
    if (!pending) {
      throw new Error(`SSH interaction request not found: ${requestId}`)
    }

    this.pendingSshInteractions.delete(requestId)
    pending.resolve(response)
  }

  createController(tabId: string, profile: ConnectionProfile, initialTranscript?: string): LiveSessionController {
    if (profile.type === 'ssh') {
      let sshController: LiveSshSessionController | null = null
      sshController = new LiveSshSessionController(
          tabId,
          profile,
          (request) => this.requestSshInteraction(tabId, profile, request),
          (fingerprint) => this.options.rememberTrustedHostFingerprint(profile.id, fingerprint),
          (chunk) => {
            const current = this.sessions.get(tabId)
            if (current) {
              this.sessions.set(tabId, {
                ...current,
                terminalTranscript: this.appendToTranscript(current.terminalTranscript, chunk)
              })
            }
            this.sendToTab(tabId, 'terminal:data', { tabId, chunk })
          },
          (summary, transcript, connected) => {
            const current = this.sessions.get(tabId)
            if (!current) {
              return
            }
            const wasConnected = current.connected === true

            this.sessions.set(tabId, {
              ...current,
              summary,
              terminalTranscript: transcript,
              remoteFiles: connected ? current.remoteFiles : [],
              fileAccessMode: sshController?.getFileAccessMode() ?? current.fileAccessMode,
              hasReusableSudoAuth: sshController?.hasReusableSudoAuth() ?? false,
              connected,
              systemMetrics: connected ? current.systemMetrics : undefined
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
            if (wasConnected && !connected) {
              void this.options.onTabDisconnected?.(tabId, summary)
            }
            void this.emitSnapshotForTab(tabId)
          },
          initialTranscript
        )
      return sshController
    }

    return new LiveFtpSessionController(tabId, profile)
  }

  async connect(tabId: string, controller: LiveSessionController) {
    this.liveControllers.set(tabId, controller)

    try {
      await controller.connect()

      const current = this.sessions.get(tabId)
      if (!current) {
        if (this.liveControllers.get(tabId) === controller) {
          this.liveControllers.delete(tabId)
          await controller.disconnect().catch(() => undefined)
        }
        return
      }

      this.sessions.set(tabId, {
        ...current,
        summary: controller.getSummary(),
        terminalTranscript:
          controller.type === 'ssh' ? controller.getTerminalTranscript() : undefined,
        remotePath: controller.getRemotePath(),
        fileAccessMode: controller.getFileAccessMode(),
        hasReusableSudoAuth: controller.type === 'ssh' ? controller.hasReusableSudoAuth() : false,
        connected: true,
        remoteFiles: current.remoteFiles,
        systemMetrics: current.systemMetrics
      })
      this.options.updateTabStatus(tabId, 'connected')
      await this.emitSnapshotForTab(tabId)

      let remoteFilesError: string | null = null
      try {
        const files = await controller.listRemoteFiles()
        const latest = this.sessions.get(tabId)
        if (latest) {
          this.sessions.set(tabId, {
            ...latest,
            remotePath: controller.getRemotePath(),
            fileAccessMode: controller.getFileAccessMode(),
            hasReusableSudoAuth: controller.type === 'ssh' ? controller.hasReusableSudoAuth() : false,
            remoteFiles: files
          })
          this.emitMetricsForTab(tabId)
        }
      } catch (error) {
        if (controller.type === 'ssh' && controller.getFileAccessMode() === 'root' && shouldFallbackRootFileAccess(error)) {
          await controller.setFileAccessMode('user', {
            sudoUser: this.sessions.get(tabId)?.sudoUser ?? 'root',
            sudoPassword: ''
          })
          const files = await controller.listRemoteFiles()
          const latest = this.sessions.get(tabId)
          if (latest) {
            this.sessions.set(tabId, {
              ...latest,
              remotePath: controller.getRemotePath(),
              fileAccessMode: controller.getFileAccessMode(),
              hasReusableSudoAuth: controller.hasReusableSudoAuth(),
              remoteFiles: files
            })
            await this.emitSnapshotForTab(tabId)
          }
        } else {
          remoteFilesError = error instanceof Error ? error.message : '远程目录读取失败'
        }
      }

      if (controller.type === 'ssh') {
        const systemMetrics = await controller.refreshSystemMetrics()
        const latest = this.sessions.get(tabId)
        if (latest && systemMetrics) {
          this.sessions.set(tabId, {
            ...latest,
            systemMetrics: this.mergeNetworkHistory(undefined, systemMetrics)
          })
          await this.emitSnapshotForTab(tabId)
        }
        if (remoteFilesError) {
          controller.pushClientNotice(`SFTP 初始化失败: ${remoteFilesError}`)
        }
      }

      if (controller.type === 'ssh') {
        if (this.shouldPollMetrics(controller)) {
          this.startMetricsPolling(tabId, controller)
        }
      }
    } catch (error) {
      if (this.liveControllers.get(tabId) === controller) {
        this.liveControllers.delete(tabId)
      }
      const current = this.sessions.get(tabId)
      if (current) {
        const message = error instanceof Error ? error.message : '未知错误'
        const summary = `连接失败: ${message}`
        let transcript = controller.type === 'ssh'
          ? controller.getTerminalTranscript()
          : current.terminalTranscript
        if (controller.type === 'ssh') {
          if (!transcript?.includes(message)) {
            controller.pushClientNotice(summary)
            transcript = controller.getTerminalTranscript()
          }
        }
        this.sessions.set(tabId, {
          ...current,
          summary,
          terminalTranscript: transcript,
          connected: false
        })
        if (controller.type === 'ssh') {
          this.sendToTab(tabId, 'terminal:state', {
            tabId,
            summary,
            transcript: transcript ?? '',
            connected: false
          })
        }
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
      fileAccessMode: controller.getFileAccessMode(),
      remoteFiles
    })
  }

  async setFileAccessMode(tabId: string, mode: 'user' | 'root', options?: RemoteFileAccessOptions) {
    const controller = this.requireController(tabId)
    const current = this.sessions.get(tabId)
    if (!current) {
      throw new Error(`Session not found: ${tabId}`)
    }

    const previousMode = controller.getFileAccessMode()
    if (previousMode === mode) {
      return
    }

    const nextSudoUser = options?.sudoUser?.trim() || current.sudoUser || 'root'

    await controller.setFileAccessMode(mode, options)
    try {
      const remoteFiles = await controller.listRemoteFiles()
      this.sessions.set(tabId, {
        ...current,
        fileAccessMode: controller.getFileAccessMode(),
        sudoUser: nextSudoUser,
        hasReusableSudoAuth: controller.type === 'ssh' ? controller.hasReusableSudoAuth() : false,
        remotePath: controller.getRemotePath(),
        remoteFiles
      })
      await this.emitSnapshotForTab(tabId)
    } catch (error) {
      if (mode === 'root') {
        try {
          await controller.setFileAccessMode(previousMode, { sudoUser: current.sudoUser })
        } catch {
          // Keep the original error; this rollback is best-effort.
        }
      }
      throw error
    }
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
    if (!this.canSendToSender(sender)) {
      this.handleSenderDestroyed(sender)
      return
    }

    const snapshot = await this.options.getSnapshot()
    if (!this.canSendToSender(sender)) {
      this.handleSenderDestroyed(sender)
      return
    }

    const didSend = this.trySend(sender, 'workspace:snapshot', snapshot)
    if (!didSend) {
      this.handleSenderDestroyed(sender)
    }
  }

  async emitSnapshotForTab(tabId: string) {
    const sender = this.tabSenders.get(tabId)
    if (!sender || !this.canSendToSender(sender)) {
      this.handleSenderDestroyed(sender)
      this.tabSenders.delete(tabId)
      this.stopMetricsPolling(tabId)
      return
    }
    await this.emitSnapshot(sender)
  }

  async restoreTabData(tabId: string) {
    const controller = this.liveControllers.get(tabId)
    const current = this.sessions.get(tabId)
    if (!controller || !current?.connected) {
      return
    }

    let nextSnapshot = current
    let changed = false

    if (!current.remoteFiles.length) {
      try {
        const remoteFiles = await controller.listRemoteFiles()
        nextSnapshot = {
          ...nextSnapshot,
          remotePath: controller.getRemotePath(),
          fileAccessMode: controller.getFileAccessMode(),
          remoteFiles
        }
        changed = true
      } catch {
        // Keep the existing session data; this restoration is best-effort.
      }
    }

    if (controller.type === 'ssh' && !nextSnapshot.systemMetrics) {
      try {
        const systemMetrics = await controller.refreshSystemMetrics()
        if (systemMetrics) {
          nextSnapshot = {
            ...nextSnapshot,
            systemMetrics: this.mergeNetworkHistory(undefined, systemMetrics)
          }
          changed = true
        }
      } catch {
        // Ignore restoration errors; polling will keep trying after the tab is rebound.
      }
    }

    if (changed) {
      this.sessions.set(tabId, nextSnapshot)
      if (nextSnapshot.remoteFiles !== current.remoteFiles) {
        await this.emitSnapshotForTab(tabId)
        return
      }
      this.emitMetricsForTab(tabId)
    }
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

  private shouldPollMetrics(controller: LiveSshSessionController) {
    const profile = controller['profile']
    return profile.type !== 'ssh' || profile.enableExecChannel !== false
  }

  private async refreshMetricsForTab(tabId: string, controller: LiveSshSessionController) {
    if (this.metricsRefreshInFlight.has(tabId)) {
      return
    }

    const current = this.sessions.get(tabId)
    const sender = this.tabSenders.get(tabId)
    if (!current || !sender || !current.connected || !this.canSendToSender(sender)) {
      if (sender) {
        this.handleSenderDestroyed(sender)
      }
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
      const liveController = this.liveControllers.get(tabId)
      if (!latest || !latest.connected || liveController !== controller) {
        return
      }

      this.sessions.set(tabId, {
        ...latest,
        systemMetrics: this.mergeNetworkHistory(latest.systemMetrics, systemMetrics)
      })

      if (!this.canSendToSender(sender)) {
        this.handleSenderDestroyed(sender)
        return
      }

      this.emitMetrics(sender, tabId, this.sessions.get(tabId)?.systemMetrics)
    } finally {
      this.metricsRefreshInFlight.delete(tabId)
    }
  }

  private sendToTab(tabId: string, channel: string, payload: unknown) {
    const sender = this.tabSenders.get(tabId)
    if (!sender || !this.canSendToSender(sender)) {
      this.handleSenderDestroyed(sender)
      return
    }

    const didSend = this.trySend(sender, channel, payload)
    if (!didSend) {
      this.handleSenderDestroyed(sender)
    }
  }

  private emitMetricsForTab(tabId: string) {
    const sender = this.tabSenders.get(tabId)
    if (!sender || !this.canSendToSender(sender)) {
      this.handleSenderDestroyed(sender)
      return
    }

    this.emitMetrics(sender, tabId, this.sessions.get(tabId)?.systemMetrics)
  }

  private emitMetrics(sender: WebContents, tabId: string, systemMetrics: SystemMetrics | undefined) {
    const payload: SessionMetricsUpdate = {
      tabId,
      systemMetrics
    }
    const didSend = this.trySend(sender, 'workspace:sessionMetrics', payload)
    if (!didSend) {
      this.handleSenderDestroyed(sender)
    }
  }

  private canSendToSender(sender: WebContents) {
    if (this.invalidSenders.has(sender)) {
      return false
    }

    if (sender.isDestroyed()) {
      return false
    }

    try {
      const frame = sender.mainFrame
      if (!frame) {
        return false
      }
      if (typeof frame.isDestroyed === 'function' && frame.isDestroyed()) {
        return false
      }
      if ('detached' in frame && frame.detached) {
        return false
      }
    } catch {
      return false
    }

    return true
  }

  private attachSenderLifecycleListeners(sender: WebContents) {
    if (this.senderLifecycleListeners.has(sender)) {
      return
    }

    this.senderLifecycleListeners.add(sender)

    sender.once('destroyed', () => {
      this.handleSenderDestroyed(sender)
    })

    sender.on('render-process-gone', () => {
      this.handleSenderDestroyed(sender)
    })

    sender.on('did-start-navigation', (_event, _url, isInPlace, isMainFrame) => {
      if (isMainFrame && !isInPlace) {
        this.handleSenderDestroyed(sender)
      }
    })
  }

  private trySend(sender: WebContents, channel: string, payload: unknown) {
    if (this.invalidSenders.has(sender)) {
      return false
    }

    try {
      sender.send(channel, payload)
      return true
    } catch (error) {
      if (isIgnorableWebContentsSendError(error)) {
        this.invalidSenders.add(sender)
        return false
      }

      throw error
    }
  }

  private handleSenderDestroyed(sender?: WebContents) {
    if (!sender) {
      return
    }

    this.invalidSenders.add(sender)

    for (const [tabId, candidate] of this.tabSenders.entries()) {
      if (candidate === sender) {
        this.tabSenders.delete(tabId)
        this.stopMetricsPolling(tabId)
        this.rejectPendingSshInteractionsForTab(tabId, new Error('SSH interaction window was closed'))
      }
    }
  }

  private requestSshInteraction(
    tabId: string,
    profile: ConnectionProfile,
    request: SshInteractionDraft
  ): Promise<SshInteractionResponse> {
    if (profile.type !== 'ssh') {
      return Promise.reject(new Error('SSH interaction is only available for SSH profiles'))
    }

    const sender = this.tabSenders.get(tabId)
    if (!sender || !this.canSendToSender(sender)) {
      return Promise.reject(new Error('SSH interaction target window is unavailable'))
    }

    const requestId = randomUUID()
    return new Promise<SshInteractionResponse>((resolve, reject) => {
      this.pendingSshInteractions.set(requestId, {
        tabId,
        resolve,
        reject
      })

      const payload: SshInteractionRequest = {
        ...request,
        requestId,
        tabId,
        profileId: profile.id
      }

      const didSend = this.trySend(sender, 'ssh:interaction', payload)

      if (!didSend) {
        this.pendingSshInteractions.delete(requestId)
        reject(new Error('SSH interaction target window is unavailable'))
      }
    })
  }

  private rejectPendingSshInteractionsForTab(tabId: string, error: Error) {
    for (const [requestId, pending] of this.pendingSshInteractions.entries()) {
      if (pending.tabId !== tabId) {
        continue
      }

      this.pendingSshInteractions.delete(requestId)
      pending.reject(error)
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

  private appendToTranscript(current: string | undefined, chunk: string) {
    const next = `${current ?? ''}${chunk}`
    if (next.length <= WorkspaceSessionRuntime.TERMINAL_TRANSCRIPT_LIMIT) {
      return next
    }

    return next.slice(next.length - WorkspaceSessionRuntime.TERMINAL_TRANSCRIPT_LIMIT)
  }
}

function shouldFallbackRootFileAccess(error: unknown) {
  const message = error instanceof Error ? error.message : String(error)
  return /未检测到可复用的 sudo 授权|sudo 密码错误|sudo 密码无效|sudo credentials|incorrect password|authentication failure/i.test(message)
}

function isIgnorableWebContentsSendError(error: unknown) {
  if (!(error instanceof Error)) {
    return false
  }

  const errno = error as NodeJS.ErrnoException
  if (errno.code === 'EPIPE') {
    return true
  }

  return error.message.includes('Render frame was disposed')
    || error.message.includes('Object has been destroyed')
    || error.message.includes('WebContents was destroyed')
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
