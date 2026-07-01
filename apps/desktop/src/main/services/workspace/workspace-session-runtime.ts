import { randomUUID } from 'node:crypto'
import type { WebContents } from 'electron'
import {
  mergeSystemMetricsHistory,
  type ConnectionProfile,
  type RemoteFileAccessOptions,
  type SessionSnapshot,
  type SystemMetrics,
  type SessionMetricsUpdate,
  type SshInteractionDraft,
  type SshInteractionRequest,
  type SshInteractionResponse,
  type WorkspaceSnapshot,
  type WorkspaceTab
} from '@fileterm/core'
import { LiveFtpSessionController, LiveSshSessionController } from '../session-controllers.js'

export type LiveSessionController = LiveSshSessionController | LiveFtpSessionController

export const REMOTE_SESSION_DISCONNECTED_MESSAGE = '会话已断开，请先重连。'

export class WorkspaceSessionRuntime {
  private static readonly TERMINAL_OUTPUT_FLUSH_INTERVAL_MS = 16
  private readonly sessions = new Map<string, SessionSnapshot>()
  private readonly liveControllers = new Map<string, LiveSessionController>()
  private readonly metricsPollers = new Map<string, ReturnType<typeof setInterval>>()
  private readonly metricsRefreshInFlight = new Set<string>()
  private readonly tabSenders = new Map<string, WebContents>()
  private readonly invalidSenders = new WeakSet<WebContents>()
  private readonly senderLifecycleListeners = new WeakSet<WebContents>()
  private readonly snapshotEmitStates = new WeakMap<WebContents, {
    inFlight?: Promise<void>
    pending: boolean
  }>()
  private readonly remoteFileOperations = new Map<string, Promise<void>>()
  private readonly terminalOutputBuffers = new Map<string, string[]>()
  private readonly terminalOutputTimers = new Map<string, ReturnType<typeof setTimeout>>()
  private readonly metricsPausedRemoteFileOperations = new Set<string>()
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
    return Object.fromEntries(
      [...this.sessions.entries()].map(([tabId, snapshot]) => [
        tabId,
        this.withLiveTerminalTranscript(tabId, snapshot)
      ])
    )
  }

  get(tabId: string) {
    const snapshot = this.sessions.get(tabId)
    return snapshot ? this.withLiveTerminalTranscript(tabId, snapshot) : undefined
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
    this.flushTerminalOutput(tabId)
    await this.liveControllers.get(tabId)?.disconnect()
    this.flushTerminalOutput(tabId)
    this.liveControllers.delete(tabId)
    this.remoteFileOperations.delete(tabId)
    this.tabSenders.delete(tabId)
    this.sessions.delete(tabId)
  }

  async disconnect(tabId: string) {
    this.stopMetricsPolling(tabId)
    this.flushTerminalOutput(tabId)
    const controller = this.liveControllers.get(tabId)
    await controller?.disconnect()
    this.flushTerminalOutput(tabId)
    const current = this.sessions.get(tabId)
    if (current && controller?.type === 'ssh') {
      this.sessions.set(tabId, {
        ...current,
        terminalTranscript: controller.getTerminalTranscript()
      })
    }
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

    for (const tabId of this.terminalOutputBuffers.keys()) {
      this.flushTerminalOutput(tabId)
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

    for (const timer of this.terminalOutputTimers.values()) {
      clearTimeout(timer)
    }
    this.terminalOutputTimers.clear()
    this.terminalOutputBuffers.clear()
  }

  requireController(tabId: string) {
    const controller = this.liveControllers.get(tabId)
    const session = this.sessions.get(tabId)
    if (!controller || !session?.connected) {
      throw new Error(REMOTE_SESSION_DISCONNECTED_MESSAGE)
    }
    return controller
  }

  getController(tabId: string) {
    return this.liveControllers.get(tabId)
  }

  hasRemoteFileOperation(tabId: string) {
    return this.remoteFileOperations.has(tabId)
  }

  hasMetricsPausedRemoteFileOperation(tabId: string) {
    return this.metricsPausedRemoteFileOperations.has(tabId)
  }

  async runRemoteFileOperation<T>(
    tabId: string,
    operation: (controller: LiveSessionController, current: SessionSnapshot) => Promise<T>,
    options?: { pauseMetrics?: boolean }
  ): Promise<T> {
    const previous = this.remoteFileOperations.get(tabId) ?? Promise.resolve()
    let releaseCurrent: () => void = () => undefined
    const currentOperation = previous.catch(() => undefined).then(() => new Promise<void>((resolve) => {
      releaseCurrent = resolve
    }))

    this.remoteFileOperations.set(tabId, currentOperation)
    await previous.catch(() => undefined)

    try {
      const controller = this.liveControllers.get(tabId)
      const current = this.sessions.get(tabId)
      if (!controller || !current?.connected) {
        throw new Error(REMOTE_SESSION_DISCONNECTED_MESSAGE)
      }
      if (options?.pauseMetrics) {
        this.metricsPausedRemoteFileOperations.add(tabId)
      }
      return await operation(controller, current)
    } finally {
      if (options?.pauseMetrics) {
        this.metricsPausedRemoteFileOperations.delete(tabId)
      }
      releaseCurrent()
      if (this.remoteFileOperations.get(tabId) === currentOperation) {
        this.remoteFileOperations.delete(tabId)
      }
    }
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
    if (this.liveControllers.has(tabId)) {
      throw new Error(`Live session controller already exists: ${tabId}`)
    }

    if (profile.type === 'ssh') {
      let sshController: LiveSshSessionController | null = null
      sshController = new LiveSshSessionController(
          tabId,
          profile,
          (request) => this.requestSshInteraction(tabId, profile, request),
          (fingerprint) => this.options.rememberTrustedHostFingerprint(profile.id, fingerprint),
          (chunk) => {
            this.queueTerminalOutput(tabId, chunk)
          },
          (cwd) => {
            void this.handleShellCwdChanged(tabId, cwd).catch(() => undefined)
          },
          () => undefined,
          (summary, transcript, connected) => {
            this.flushTerminalOutput(tabId)
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
              fileAccessMode: connected ? (sshController?.getFileAccessMode() ?? current.fileAccessMode) : 'user',
              hasReusableSudoAuth: connected ? (sshController?.hasReusableSudoAuth() ?? false) : false,
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
        shellCwd: controller.type === 'ssh' ? controller.getShellCwd() : undefined,
        followShellCwd: current.followShellCwd,
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
        const shellCwd = controller.getShellCwd()
        const sessionBeforeMetrics = this.sessions.get(tabId)
        if (
          shellCwd
          && sessionBeforeMetrics?.followShellCwd !== false
          && sessionBeforeMetrics?.remotePath !== shellCwd
        ) {
          await this.followShellCwd(tabId, shellCwd)
        }

        const systemMetrics = await controller.refreshSystemMetrics()
        const latest = this.sessions.get(tabId)
        if (latest && systemMetrics) {
          this.sessions.set(tabId, {
            ...latest,
            systemMetrics: mergeSystemMetricsHistory(undefined, systemMetrics)
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
      this.flushTerminalOutput(tabId)
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
    await this.runRemoteFileOperation(tabId, async (controller, current) => {
      const remoteFiles = await controller.listRemoteFiles()
      const latest = this.sessions.get(tabId) ?? current
      this.sessions.set(tabId, {
        ...latest,
        remotePath: controller.getRemotePath(),
        fileAccessMode: controller.getFileAccessMode(),
        hasReusableSudoAuth: controller.type === 'ssh' ? controller.hasReusableSudoAuth() : false,
        remoteFiles
      })
    }, { pauseMetrics: true })
  }

  async setFileAccessMode(tabId: string, mode: 'user' | 'root', options?: RemoteFileAccessOptions) {
    await this.runRemoteFileOperation(tabId, async (controller, current) => {
      const previousMode = controller.getFileAccessMode()
      if (previousMode === mode) {
        return
      }

      const nextSudoUser = options?.sudoUser?.trim() || current.sudoUser || 'root'

      await controller.setFileAccessMode(mode, options)
      try {
        const remoteFiles = await controller.listRemoteFiles()
        const latest = this.sessions.get(tabId) ?? current
        this.sessions.set(tabId, {
          ...latest,
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
    }, { pauseMetrics: true })
  }

  async openRemotePath(tabId: string, targetPath: string) {
    await this.setRemoteFilesLoading(tabId, true)
    try {
      await this.runRemoteFileOperation(tabId, async (controller, current) => {
        const remoteFiles = await controller.openRemotePath(targetPath)
        const latest = this.sessions.get(tabId) ?? current
        this.sessions.set(tabId, {
          ...latest,
          remotePath: controller.getRemotePath(),
          fileAccessMode: controller.getFileAccessMode(),
          hasReusableSudoAuth: controller.type === 'ssh' ? controller.hasReusableSudoAuth() : false,
          remoteFiles
        })
      }, { pauseMetrics: true })
    } finally {
      await this.setRemoteFilesLoading(tabId, false)
    }
  }

  async setFollowShellCwd(tabId: string, enabled: boolean) {
    const current = this.sessions.get(tabId)
    if (!current) {
      throw new Error(`Session not found: ${tabId}`)
    }

    this.sessions.set(tabId, {
      ...current,
      followShellCwd: enabled
    })

    if (enabled && current.shellCwd && current.shellCwd !== current.remotePath) {
      await this.followShellCwd(tabId, current.shellCwd)
    }
  }

  private async handleShellCwdChanged(tabId: string, cwd: string) {
    const current = this.sessions.get(tabId)
    if (!current || current.shellCwd === cwd) {
      return
    }

    this.sessions.set(tabId, {
      ...current,
      shellCwd: cwd
    })

    if (current.followShellCwd !== false && current.remotePath !== cwd) {
      await this.followShellCwd(tabId, cwd)
      return
    }

    await this.emitSnapshotForTab(tabId)
  }

  private async followShellCwd(tabId: string, cwd: string) {
    await this.setRemoteFilesLoading(tabId, true)
    try {
      await this.runRemoteFileOperation(tabId, async (controller, current) => {
        if (controller.type !== 'ssh') {
          return
        }
        if (current.shellCwd !== cwd || current.followShellCwd === false) {
          return
        }
        const remoteFiles = await controller.openRemotePath(cwd)
        const latest = this.sessions.get(tabId) ?? current
        if (latest.shellCwd !== cwd || latest.followShellCwd === false) {
          if (controller.getRemotePath() !== latest.remotePath) {
            await controller.openRemotePath(latest.remotePath)
          }
          return
        }
        this.sessions.set(tabId, {
          ...latest,
          remotePath: controller.getRemotePath(),
          remoteFiles
        })
      }, { pauseMetrics: true })
    } catch {
      // Cwd reporting is best-effort. Keep the terminal usable when the file view cannot read this path.
    } finally {
      await this.setRemoteFilesLoading(tabId, false)
    }
  }

  private async setRemoteFilesLoading(tabId: string, loading: boolean) {
    const current = this.sessions.get(tabId)
    if (!current || current.remoteFilesLoading === loading) {
      return
    }
    this.sessions.set(tabId, {
      ...current,
      remoteFilesLoading: loading
    })
    await this.emitSnapshotForTab(tabId)
  }

  async emitSnapshot(sender: WebContents) {
    let state = this.snapshotEmitStates.get(sender)
    if (!state) {
      state = { pending: false }
      this.snapshotEmitStates.set(sender, state)
    }

    if (state.inFlight) {
      state.pending = true
      await state.inFlight
      return
    }

    const emitAllPending = async () => {
      do {
        state!.pending = false
        await this.emitSnapshotNow(sender)
      } while (state!.pending)
    }

    state.inFlight = emitAllPending().finally(() => {
      state!.inFlight = undefined
    })
    await state.inFlight
  }

  private async emitSnapshotNow(sender: WebContents) {
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

  emitToSender(sender: WebContents, channel: string, payload: unknown) {
    if (!this.canSendToSender(sender)) {
      this.handleSenderDestroyed(sender)
      return
    }

    if (!this.trySend(sender, channel, payload)) {
      this.handleSenderDestroyed(sender)
    }
  }

  emitToTab(tabId: string, channel: string, payload: unknown) {
    this.sendToTab(tabId, channel, payload)
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
        await this.runRemoteFileOperation(tabId, async (latestController, latestSession) => {
          const remoteFiles = await latestController.listRemoteFiles()
          const freshSession = this.sessions.get(tabId) ?? latestSession
          nextSnapshot = {
            ...freshSession,
            remotePath: latestController.getRemotePath(),
            fileAccessMode: latestController.getFileAccessMode(),
            hasReusableSudoAuth: latestController.type === 'ssh' ? latestController.hasReusableSudoAuth() : false,
            remoteFiles
          }
          this.sessions.set(tabId, nextSnapshot)
        }, { pauseMetrics: true })
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
            systemMetrics: mergeSystemMetricsHistory(undefined, systemMetrics)
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
    if (this.metricsRefreshInFlight.has(tabId) || this.hasMetricsPausedRemoteFileOperation(tabId)) {
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
        systemMetrics: mergeSystemMetricsHistory(latest.systemMetrics, systemMetrics)
      })

      if (!this.canSendToSender(sender)) {
        this.handleSenderDestroyed(sender)
        return
      }

      this.emitMetrics(sender, tabId, systemMetrics, 'append')
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

    this.emitMetrics(sender, tabId, this.sessions.get(tabId)?.systemMetrics, 'replace')
  }

  private emitMetrics(
    sender: WebContents,
    tabId: string,
    systemMetrics: SystemMetrics | undefined,
    mode: SessionMetricsUpdate['mode']
  ) {
    const payload: SessionMetricsUpdate = {
      tabId,
      systemMetrics,
      mode
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

  private queueTerminalOutput(tabId: string, chunk: string) {
    const chunks = this.terminalOutputBuffers.get(tabId)
    if (chunks) {
      chunks.push(chunk)
    } else {
      this.terminalOutputBuffers.set(tabId, [chunk])
    }

    if (this.terminalOutputTimers.has(tabId)) {
      return
    }

    const timer = setTimeout(() => {
      this.terminalOutputTimers.delete(tabId)
      this.flushTerminalOutput(tabId)
    }, WorkspaceSessionRuntime.TERMINAL_OUTPUT_FLUSH_INTERVAL_MS)
    this.terminalOutputTimers.set(tabId, timer)
  }

  private flushTerminalOutput(tabId: string) {
    const timer = this.terminalOutputTimers.get(tabId)
    if (timer) {
      clearTimeout(timer)
      this.terminalOutputTimers.delete(tabId)
    }

    const chunks = this.terminalOutputBuffers.get(tabId)
    if (!chunks?.length) {
      return
    }
    this.terminalOutputBuffers.delete(tabId)

    const chunk = chunks.length === 1 ? chunks[0]! : chunks.join('')
    this.sendToTab(tabId, 'terminal:data', { tabId, chunk })
  }

  private withLiveTerminalTranscript(tabId: string, snapshot: SessionSnapshot) {
    const controller = this.liveControllers.get(tabId)
    if (controller?.type !== 'ssh') {
      return snapshot
    }

    const terminalTranscript = controller.getTerminalTranscript()
    return terminalTranscript === snapshot.terminalTranscript
      ? snapshot
      : { ...snapshot, terminalTranscript }
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
