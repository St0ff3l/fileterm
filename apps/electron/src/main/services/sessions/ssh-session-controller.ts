import { createHash, randomUUID } from 'node:crypto'
import { createReadStream, createWriteStream } from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { readFile, stat } from 'node:fs/promises'
import { Readable, type Writable } from 'node:stream'
import { pipeline } from 'node:stream/promises'
import type { ClientChannel, ConnectConfig, FileEntry, SFTPWrapper, Stats } from 'ssh2'
import ssh2 from 'ssh2'
import type {
  ConnectionProfile,
  PermissionChangeOptions,
  RemoteFileAccessOptions,
  RemoteFileItem,
  RemoteFileStat,
  SshInteractionDraft,
  SshInteractionResponse,
  SystemMetrics,
  SshProfile,
  SshForwardRule,
  SshTunnelSnapshot,
  SshSessionController,
  TransferFileOptions,
  TransferProgress
} from '@fileterm/core'
import { BaseFileSessionController } from './base-file-session-controller.js'
import { parentRemotePath, toRemoteFileItem } from './session-file-utils.js'
import { collectSshSystemMetrics, type RemoteSystemPlatform } from './system-metrics/index.js'
import { probeRemoteSystemPlatform } from './system-metrics/platform-probe.js'
import type { SystemMetricsCommandOptions } from './system-metrics/types.js'
import {
  findSetupEchoEnd,
  shellCwdSetupForPlatform,
  ShellCwdTracker,
  supportsPosixShellSetup
} from './shell-cwd-integration.js'
import { createSshDebugLogger, isSshDebugEnabled, singleLine } from './ssh-debug-logger.js'
import { decodeBuffer, encodeText } from '../text-encoding.js'
import { appLog, appWarn } from '../app-logger.js'
import { createOutboundSocket } from '../network/proxy-socket-factory.js'
import { SshTunnelService } from './ssh-tunnel-service.js'

const { Client, utils } = ssh2
type Client = InstanceType<typeof Client>

export interface SshSessionClientDependencies {
  main?: Client
  exec?: Client
  sftp?: Client
  transfer?: Client
  createTransferClient?: () => Client
  resolveManagedKey?(keyId: string): Promise<{
    key: { id: string; name: string; encrypted: boolean }
    privateKey: Buffer
    savedPassphrase?: string
  }>
  setManagedKeyPassphrase?(keyId: string, passphrase: string | undefined): Promise<void>
}

export class LiveSshSessionController extends BaseFileSessionController implements SshSessionController {
  readonly type = 'ssh'
  private static readonly TRANSCRIPT_LIMIT = 200_000
  private static readonly REMOTE_FILE_READ_TIMEOUT_MS = 20_000
  private static readonly REMOTE_FILE_WRITE_TIMEOUT_MS = 20_000
  private static readonly METRICS_WARNING_INTERVAL_MS = 30_000
  private static readonly SHELL_SETUP_SETTLE_MS = 200
  private static readonly SHELL_SETUP_TIMEOUT_MS = 1200

  private readonly ssh: Client
  private readonly execSsh: Client
  private readonly sftpSsh: Client
  private transferSsh: Client
  private readonly createTransferClient: () => Client
  private readonly resolveManagedKey?: SshSessionClientDependencies['resolveManagedKey']
  private readonly setManagedKeyPassphrase?: SshSessionClientDependencies['setManagedKeyPassphrase']
  private readonly sshDebug = createSshDebugLogger(isSshDebugEnabled(), (message) => {
    this.appendSystemMessage(message)
  })
  private sftp?: SFTPWrapper
  private transferSftp?: SFTPWrapper
  private sftpUnavailableReason: string | null = null
  private sshConfig?: ConnectConfig
  private execReady = false
  private execConnectPromise?: Promise<Client>
  private hasRegisteredExecLifecycle = false
  private hasRegisteredSftpLifecycle = false
  private hasRegisteredTransferLifecycle = false
  private transferSshClosed = false
  private readonly activeTransferPipelines = new Set<{ source: Readable; destination: Writable }>()
  private shellStream?: {
    write(data: string): void
    setWindow(rows: number, cols: number, height: number, width: number): void
    end(): void
  }
  private pendingResize?: { cols: number; rows: number; width: number; height: number }
  private readonly transcript: BoundedTextBuffer
  private currentRemotePath: string
  private shellCwd?: string
  private readonly shellCwdTracker = new ShellCwdTracker()
  private cwdSetupInjected = false
  private shellPlatformProbe?: Promise<void>
  private platformProbePromise?: Promise<RemoteSystemPlatform>
  private metricsRefreshPromise?: Promise<SystemMetrics | undefined>
  private shellPlatform: RemoteSystemPlatform = 'unknown'
  private pendingShellInput: string[] = []
  private shellSetupCanceled = false
  private suppressEcho = false
  private echoBuf = ''
  private shellSetupReleaseTimer?: ReturnType<typeof setTimeout>
  private shellSetupSettleTimer?: ReturnType<typeof setTimeout>
  private shellSetupVisiblePrefixLength?: number
  private lastInjectTime = 0
  private fileAccessMode: 'user' | 'root' = 'user'
  private shellUser?: string
  private sudoUser = 'root'
  private sudoPassword?: string
  private sudoPromptWindow = ''
  private awaitingSudoPasswordInput = false
  private pendingSudoPasswordInput = ''
  private recentKeystrokes = ''
  private metrics?: SystemMetrics
  private metricsPlatform?: RemoteSystemPlatform
  private lastMetricsWarningAt = 0
  private connectionGeneration = 0
  private readonly acceptedHostFingerprints = new Set<string>()
  private tunnelService?: SshTunnelService
  private jumpSsh?: Client
  private keyboardInteractiveChallengeSeen = false

  constructor(
    id: string,
    profile: SshProfile,
    private readonly requestInteraction: (request: SshInteractionDraft) => Promise<SshInteractionResponse>,
    private readonly rememberTrustedHostFingerprint: (profileId: string, fingerprint: string) => Promise<void>,
    private readonly onData: (chunk: string) => void,
    private readonly onShellCwdChange: (cwd: string) => void,
    private readonly onShellUserChange: (user: string) => void,
    private readonly onStateChange: (summary: string, transcript: string, connected: boolean) => void,
    initialTranscript?: string,
    clients: SshSessionClientDependencies = {},
    private readonly resolveProfile?: (profileId: string) => Promise<SshProfile | ConnectionProfile | null>
  ) {
    super(id, 'ssh', profile)
    this.createTransferClient = clients.createTransferClient ?? (() => new Client())
    this.resolveManagedKey = clients.resolveManagedKey
    this.setManagedKeyPassphrase = clients.setManagedKeyPassphrase
    this.ssh = clients.main ?? new Client()
    this.execSsh = clients.exec ?? new Client()
    this.sftpSsh = clients.sftp ?? new Client()
    this.transferSsh = clients.transfer ?? this.createTransferClient()
    this.currentRemotePath = profile.remotePath || '.'
    this.transcript = new BoundedTextBuffer(LiveSshSessionController.TRANSCRIPT_LIMIT, initialTranscript)
    this.appendSystemMessage('连接主机...\r\n')
  }

  private async resolveSshAuthConfig(
    profile: SshProfile
  ): Promise<Pick<ConnectConfig, 'password' | 'privateKey' | 'passphrase' | 'agent'>> {
    if (profile.authType !== 'privateKey' || !profile.privateKeyId) {
      return resolveSshAuthConfig(profile)
    }
    if (!this.resolveManagedKey) {
      throw new Error('SSH key service unavailable')
    }

    const resolved = await this.resolveManagedKey(profile.privateKeyId)
    if (!resolved.key.encrypted) {
      return { privateKey: resolved.privateKey }
    }

    let passphrase = resolved.savedPassphrase
    let reason: 'required' | 'invalid-saved' = 'required'
    if (passphrase) {
      const parsed = utils.parseKey(resolved.privateKey, passphrase)
      if (!(parsed instanceof Error) && parsed.isPrivateKey()) {
        return { privateKey: resolved.privateKey, passphrase }
      }
      reason = 'invalid-saved'
      passphrase = undefined
      await this.setManagedKeyPassphrase?.(resolved.key.id, undefined)
    }

    const response = await this.requestInteraction({
      kind: 'key-passphrase',
      keyId: resolved.key.id,
      keyName: resolved.key.name,
      reason
    })
    if (response.kind !== 'key-passphrase' || response.canceled) {
      throw new Error('SSH key passphrase request canceled')
    }

    const parsed = utils.parseKey(resolved.privateKey, response.passphrase)
    if (parsed instanceof Error || !parsed.isPrivateKey()) {
      throw new Error('私钥口令不正确。')
    }
    if (response.savePassphrase) {
      await this.setManagedKeyPassphrase?.(resolved.key.id, response.passphrase)
    }
    return { privateKey: resolved.privateKey, passphrase: response.passphrase }
  }

  override async connect(): Promise<void> {
    this.connectionGeneration += 1
    this.cwdSetupInjected = false
    this.shellPlatformProbe = undefined
    this.platformProbePromise = undefined
    this.metricsRefreshPromise = undefined
    this.shellPlatform = 'unknown'
    this.pendingShellInput = []
    this.shellSetupCanceled = false
    this.metricsPlatform = undefined
    this.lastInjectTime = 0
    this.keyboardInteractiveChallengeSeen = false
    this.clearShellSetupSuppression()

    const profile = await this.resolveConnectionProfile(this.profile as SshProfile)
    await this.connectJumpHost(profile)
    const authConfig = await this.resolveSshAuthConfig(profile)
    // keyboard-interactive is a first-class auth mode. ssh2 does not request a
    // challenge unless tryKeyboard is true, which previously caused it to fall
    // through to agent/default-key auth and report "All configured… failed".
    const shouldTryKeyboard = profile.authType === 'keyboard-interactive' || profile.authType === 'password'
    const username = profile.username || os.userInfo().username
    const baseSshConfig: ConnectConfig = {
      host: profile.host,
      port: profile.port,
      username,
      ...authConfig,
      readyTimeout: 15000,
      keepaliveInterval: 3000,
      keepaliveCountMax: 2,
      tryKeyboard: shouldTryKeyboard,
      // Prefer a stored password, then request keyboard-interactive on the
      // same SSH connection. This supports ordinary password servers as well
      // as MFA bastions that ask for an OTP after the password exchange.
      ...(profile.authType === 'keyboard-interactive'
        ? { authHandler: profile.password ? ['password', 'keyboard-interactive'] : ['keyboard-interactive'] }
        : {}),
      hostVerifier: (key: Buffer | string, verify: (accepted: boolean) => void) => {
        void this.verifyHostFingerprint(profile, key)
          .then(verify)
          .catch((error) => {
            this.sshDebug.log('main', `主机密钥校验失败: ${error instanceof Error ? error.message : String(error)}`)
            verify(false)
          })
      },
      ...(this.sshDebug.enabled ? { debug: (message: string) => this.sshDebug.handle('main', message) } : {})
    }
    const sshConfig = await this.withOutboundSocket(baseSshConfig, profile)
    this.sshConfig = sshConfig
    this.sshDebug.logConnectionStart('main', profile, username, authConfig, shouldTryKeyboard)

    await new Promise<void>((resolve, reject) => {
      let settled = false
      let connectionFailed = false

      this.ssh.removeAllListeners('banner')
      this.ssh.removeAllListeners('keyboard-interactive')
      this.ssh.on('banner', (message) => {
        this.sshDebug.log('main', `服务端横幅: ${singleLine(message)}`)
      })
      this.registerKeyboardInteractiveHandler(this.ssh, profile, 'main', (message) => {
        this.sshDebug.logKeyboardInteractive('main', message)
      })

      this.ssh
        .on('ready', () => {
          this.connected = true
          this.tunnelService = new SshTunnelService(this.ssh, (message) => this.appendSystemMessage(`${message}\r\n`))
          for (const rule of profile.forwards ?? []) this.tunnelService.register(rule)
          void Promise.all(
            (profile.forwards ?? []).filter((rule) => rule.autoStart).map((rule) => this.tunnelService?.start(rule))
          ).catch((error) =>
            this.appendSystemMessage(
              `Tunnel startup failed: ${error instanceof Error ? error.message : String(error)}\r\n`
            )
          )
          this.appendSystemMessage('连接主机成功\r\n')
          this.sshDebug.log('main', '认证完成，准备打开终端')
          this.onStateChange(this.getSummary(), this.transcript.toString(), true)
          this.ssh.shell(
            {
              term: 'xterm-256color',
              rows: this.pendingResize?.rows ?? 32,
              cols: this.pendingResize?.cols ?? 120
            },
            (error: Error | undefined, stream: ClientChannel) => {
              if (error) {
                connectionFailed = true
                this.appendSystemMessage(`终端启动失败: ${error.message}\r\n`)
                this.onStateChange(`Connection error: ${error.message}`, this.transcript.toString(), false)
                if (!settled) {
                  settled = true
                  reject(error)
                }
                return
              }

              this.shellStream = stream
              if (this.pendingResize) {
                const { cols, rows, width, height } = this.pendingResize
                stream.setWindow(rows, cols, Math.max(0, Math.floor(height)), Math.max(0, Math.floor(width)))
                this.pendingResize = undefined
              }

              stream.on('data', (chunk: Buffer) => {
                const text = chunk.toString('utf8')

                if (text.trim().length > 0) {
                  this.scheduleShellSetup(stream)
                }

                if (this.suppressEcho) {
                  this.echoBuf += text
                  const echoEnd = findSetupEchoEnd(this.echoBuf)
                  if (echoEnd) {
                    if (echoEnd.cwd && echoEnd.cwd !== this.shellCwd) {
                      this.sshDebug.log('main', `Initial shell cwd detected via injection: ${echoEnd.cwd}`)
                      this.shellCwd = echoEnd.cwd
                      this.onShellCwdChange(echoEnd.cwd)
                    }
                    if (echoEnd.user && echoEnd.user !== this.shellUser) {
                      this.sshDebug.log('main', `Initial shell user detected via injection: ${echoEnd.user}`)
                      this.handleShellUserChange(echoEnd.user)
                    }
                    this.shellSetupVisiblePrefixLength = echoEnd.lineStart
                    this.scheduleShellSetupSettle(stream)
                  }
                  if (this.echoBuf.length >= 16384) {
                    this.finishShellSetupSuppression(stream)
                  }
                  return
                }

                this.transcript.append(text)

                // 1. Feed updates (CWD and User) first
                for (const update of this.shellCwdTracker.feed(text)) {
                  if (update.cwd && update.cwd !== this.shellCwd) {
                    this.shellCwd = update.cwd
                    this.onShellCwdChange(update.cwd)
                  }
                  if (update.user && update.user !== this.shellUser) {
                    this.handleShellUserChange(update.user)
                  }
                }

                // 2. Track sudo prompt
                this.trackSudoPromptFromTerminal(text)

                // 3. Heuristic detection: if the prompt ends with '# ', it might be a root shell.
                // We inject the setup script silently to query the actual user.
                const strippedText = text.replace(/\u001b\[[0-9;?]*[A-Za-z]/g, '').trimEnd()
                const now = Date.now()
                if (supportsPosixShellSetup(this.shellPlatform) && strippedText.endsWith('#') && !this.suppressEcho) {
                  // Only inject if we think we aren't root AND it's been a while since the last injection
                  // to prevent infinite loops (because the injected script itself triggers a new prompt)
                  if (this.shellUser !== 'root' && now - this.lastInjectTime > 2000) {
                    this.lastInjectTime = now
                    this.injectShellSetup(stream)
                  }
                }

                this.onData(text)
                this.flushPendingShellInput(stream)
              })
              stream.on('close', () => {
                this.handlePrimaryDisconnect(stream)
                this.onStateChange('Shell closed', this.transcript.toString(), false)
              })

              if (!settled) {
                settled = true
                resolve()
              }
            }
          )
        })
        .on('error', (error: Error) => {
          if (
            profile.authType === 'keyboard-interactive' &&
            !this.keyboardInteractiveChallengeSeen &&
            /All configured authentication methods failed/i.test(error.message)
          ) {
            error = new Error(
              '服务器没有提供 keyboard-interactive 认证挑战。请在服务端启用 KbdInteractiveAuthentication，并确认用户名正确；若服务器仅启用 PasswordAuthentication，请改用“密码”认证。'
            )
          }
          this.handlePrimaryDisconnect()
          if (connectionFailed) {
            this.sshDebug.log('main', `忽略重复连接错误: ${error.message}`)
            return
          }
          connectionFailed = true
          this.appendSystemMessage(`连接失败: ${error.message}\r\n`)
          this.sshDebug.log('main', `连接错误: ${error.message}`)
          if (!settled) {
            settled = true
            reject(error)
          }
          this.onStateChange(`Connection error: ${error.message}`, this.transcript.toString(), false)
        })
        .on('close', () => {
          this.handlePrimaryDisconnect()
          if (connectionFailed) {
            this.sshDebug.log('main', '连接失败后收到关闭事件')
            return
          }
          this.appendSystemMessage('连接已断开\r\n')
          this.sshDebug.log('main', '连接已关闭')
          this.onStateChange('Disconnected', this.transcript.toString(), false)
        })
        .connect(sshConfig)
    })
  }

  listTunnels(): SshTunnelSnapshot[] {
    return this.tunnelService?.list() ?? []
  }

  async createTunnel(rule: SshForwardRule): Promise<SshTunnelSnapshot[]> {
    const service = this.requireTunnelService()
    service.register(rule, true)
    return service.list()
  }

  async startTunnel(ruleId: string): Promise<SshTunnelSnapshot[]> {
    const service = this.requireTunnelService()
    const rule = service.list().find((item) => item.id === ruleId)
    if (!rule) throw new Error('Tunnel was not found')
    await service.start(rule)
    return service.list()
  }

  async stopTunnel(ruleId: string): Promise<SshTunnelSnapshot[]> {
    const service = this.requireTunnelService()
    await service.stop(ruleId)
    return service.list()
  }

  async deleteTunnel(ruleId: string): Promise<SshTunnelSnapshot[]> {
    const service = this.requireTunnelService()
    await service.remove(ruleId)
    return service.list()
  }

  private requireTunnelService(): SshTunnelService {
    if (!this.connected || !this.tunnelService) throw new Error('SSH tunnel service is unavailable while disconnected')
    return this.tunnelService
  }

  private async resolveConnectionProfile(profile: SshProfile): Promise<SshProfile> {
    if (profile.authType !== 'password') {
      return profile
    }

    if (!profile.username?.trim()) {
      const response = await this.requestInteraction({
        kind: 'credentials',
        host: profile.host,
        port: profile.port,
        username: '',
        passwordRequired: true,
        reason: 'missing-username'
      })

      if (response.kind !== 'credentials' || response.canceled) {
        throw new Error('SSH 登录已取消')
      }

      return {
        ...profile,
        username: response.username.trim(),
        password: response.password
      }
    }

    if (!profile.password) {
      const hasSystemAuth = await hasSystemSshAuthAvailable()
      if (hasSystemAuth) {
        return profile
      }

      const response = await this.requestInteraction({
        kind: 'credentials',
        host: profile.host,
        port: profile.port,
        username: profile.username,
        passwordRequired: true,
        reason: 'missing-password'
      })

      if (response.kind !== 'credentials' || response.canceled) {
        throw new Error('SSH 登录已取消')
      }

      return {
        ...profile,
        username: response.username.trim(),
        password: response.password
      }
    }

    return profile
  }

  private async withOutboundSocket(config: ConnectConfig, profile: SshProfile): Promise<ConnectConfig> {
    const { sock: _sock, ...base } = config as ConnectConfig & { sock?: unknown }
    if (this.jumpSsh) {
      const sock = await this.openJumpChannel(profile.host, profile.port)
      return { ...base, sock } as ConnectConfig
    }
    if (!profile.proxy || profile.proxy.type === 'none') return base
    const sock = await createOutboundSocket(profile.host, profile.port, profile.proxy, config.readyTimeout ?? 15_000)
    return { ...base, sock } as ConnectConfig
  }

  private async connectJumpHost(profile: SshProfile) {
    if (!profile.jumpProfileId) return
    const jump = await this.resolveProfile?.(profile.jumpProfileId)
    if (!jump || jump.type !== 'ssh' || jump.id === profile.id || jump.jumpProfileId) {
      throw new Error('Jump Host must reference a different SSH profile without another Jump Host')
    }
    const client = new Client()
    const resolvedJump = await this.resolveConnectionProfile(jump)
    const auth = await resolveSshAuthConfig(resolvedJump)
    const shouldTryKeyboard = resolvedJump.authType === 'keyboard-interactive' || resolvedJump.authType === 'password'
    const baseConfig: ConnectConfig = {
      host: resolvedJump.host,
      port: resolvedJump.port,
      username: resolvedJump.username || os.userInfo().username,
      ...auth,
      readyTimeout: 15_000,
      keepaliveInterval: 3000,
      keepaliveCountMax: 2,
      tryKeyboard: shouldTryKeyboard,
      ...(resolvedJump.authType === 'keyboard-interactive'
        ? { authHandler: resolvedJump.password ? ['password', 'keyboard-interactive'] : ['keyboard-interactive'] }
        : {}),
      hostVerifier: (key: Buffer | string, verify: (accepted: boolean) => void) => {
        void this.verifyHostFingerprint(resolvedJump, key)
          .then(verify)
          .catch(() => verify(false))
      }
    }
    const jumpConfig = await this.withOutboundSocket(baseConfig, resolvedJump)
    await new Promise<void>((resolve, reject) => {
      this.registerKeyboardInteractiveHandler(client, resolvedJump, 'jump host', (message) =>
        this.sshDebug.log('main', `[Jump Host] ${message}`)
      )
      client
        .once('ready', resolve)
        .once('error', (error) => reject(new Error(`Jump Host connection failed: ${error.message}`)))
        .connect(jumpConfig)
    })
    this.jumpSsh = client
  }

  private openJumpChannel(host: string, port: number) {
    if (!this.jumpSsh) throw new Error('Jump Host is disconnected')
    return new Promise<ClientChannel>((resolve, reject) =>
      this.jumpSsh?.forwardOut('127.0.0.1', 0, host, port, (error, stream) =>
        error || !stream ? reject(error ?? new Error('Jump Host channel failed')) : resolve(stream)
      )
    )
  }

  private registerKeyboardInteractiveHandler(
    client: Client,
    profile: SshProfile,
    scope: string,
    onEvent: (message: string) => void
  ) {
    client.on('keyboard-interactive', (name, instructions, _instructionsLang, prompts, finish) => {
      this.keyboardInteractiveChallengeSeen = true
      onEvent(`收到 keyboard-interactive 认证请求，提示数 ${prompts.length}`)
      void (async () => {
        if (!prompts.length) return finish([])
        const canReusePassword =
          (profile.authType === 'password' || profile.authType === 'keyboard-interactive') &&
          prompts.length === 1 &&
          !prompts[0]?.echo &&
          /password|密码/i.test(prompts[0]?.prompt ?? '') &&
          Boolean(profile.password)
        if (canReusePassword) {
          onEvent('已自动回复首个 password 提示')
          return finish([profile.password ?? ''])
        }
        const response = await this.requestInteraction({
          kind: 'keyboard-interactive',
          host: profile.host,
          port: profile.port,
          name: name ?? '',
          instructions: instructions ?? '',
          prompts: prompts.map((prompt) => ({ prompt: prompt.prompt, echo: prompt.echo === true }))
        })
        if (
          response.kind !== 'keyboard-interactive' ||
          response.canceled ||
          response.answers.length !== prompts.length
        ) {
          throw new Error('Keyboard-interactive authentication canceled')
        }
        onEvent(`${scope} keyboard-interactive 已收到用户响应（${response.answers.length} 项）`)
        finish(response.answers)
      })().catch((error) => {
        onEvent(`${scope} keyboard-interactive 已取消: ${error instanceof Error ? error.message : String(error)}`)
        finish([])
      })
    })
  }

  private handleShellUserChange(user: string) {
    if (this.shellUser === user) return
    this.shellUser = user
    this.onShellUserChange(user)
  }

  private scheduleShellSetup(stream: { write(data: string): void }) {
    if (this.cwdSetupInjected || this.shellPlatformProbe) {
      return
    }

    const profile = this.profile as SshProfile
    if (profile.enableExecChannel === false || profile.enableResourceMonitoring === false) {
      this.cwdSetupInjected = true
      this.shellPlatform = 'unknown'
      this.sshDebug.log('main', 'Exec Channel 已禁用，跳过平台探测与 shell CWD 注入')
      this.flushPendingShellInput(stream)
      return
    }

    const probe = this.detectPlatformAndSetupShell(stream)
    this.shellPlatformProbe = probe
    const clearProbe = () => {
      if (this.shellPlatformProbe === probe) {
        this.shellPlatformProbe = undefined
      }
      this.flushPendingShellInput(stream)
    }
    void probe.then(clearProbe, (error) => {
      clearProbe()
      this.sshDebug.log('exec', `shell CWD 初始化失败: ${error instanceof Error ? error.message : String(error)}`)
    })
  }

  private async detectPlatformAndSetupShell(stream: { write(data: string): void }) {
    let platform: RemoteSystemPlatform = 'unknown'
    try {
      platform = await this.resolveRemoteSystemPlatform()
    } catch (error) {
      this.sshDebug.log(
        'exec',
        `shell 平台探测失败，已跳过 CWD 注入: ${error instanceof Error ? error.message : String(error)}`
      )
    }

    if (!this.connected || this.shellStream !== stream || this.cwdSetupInjected || this.shellSetupCanceled) {
      return
    }

    this.shellPlatform = platform
    if (platform !== 'unknown') {
      this.metricsPlatform = platform
    }
    this.cwdSetupInjected = true
    if (!supportsPosixShellSetup(platform)) {
      this.sshDebug.log('main', `远端平台为 ${platform}，跳过 POSIX shell CWD 注入`)
      return
    }

    this.injectShellSetup(stream)
  }

  private resolveRemoteSystemPlatform(): Promise<RemoteSystemPlatform> {
    if (this.metricsPlatform && this.metricsPlatform !== 'unknown') {
      return Promise.resolve(this.metricsPlatform)
    }
    if (this.platformProbePromise) {
      return this.platformProbePromise
    }

    const generation = this.connectionGeneration
    const execProbeCommand = async (command: string, options?: SystemMetricsCommandOptions, stdinPayload?: string) => {
      if (!this.connected || this.connectionGeneration !== generation) {
        throw new Error('SSH connection changed before platform probe completed')
      }

      const output = await this.execCommand(command, options, false, stdinPayload)
      if (!this.connected || this.connectionGeneration !== generation) {
        throw new Error('SSH connection changed before platform probe completed')
      }
      return output
    }
    // Establish the shared Exec SSH transport once under the probe budget.
    // Without this guard every POSIX/PowerShell/cmd candidate could wait for a
    // fresh 15-second SSH handshake after the transport itself had failed.
    const probe = this.withOperationTimeout(
      this.ensureExecConnection(),
      3000,
      'Exec SSH connection timed out during platform detection'
    ).then(() => probeRemoteSystemPlatform({ exec: execProbeCommand }))
    this.platformProbePromise = probe
    void probe.then(
      (platform) => {
        if (this.platformProbePromise !== probe) {
          return
        }
        this.platformProbePromise = undefined
        if (this.connected && this.connectionGeneration === generation) {
          this.metricsPlatform = platform
        }
      },
      () => {
        if (this.platformProbePromise === probe) {
          this.platformProbePromise = undefined
        }
      }
    )
    return probe
  }

  private injectShellSetup(stream: { write(data: string): void }) {
    const setup = shellCwdSetupForPlatform(this.shellPlatform)
    if (!setup || !this.connected || this.shellStream !== stream) {
      return
    }

    this.clearShellSetupSuppression()
    this.suppressEcho = true
    try {
      stream.write(` ${setup}\r`)
    } catch (error) {
      this.clearShellSetupSuppression()
      throw error
    }
    this.shellSetupReleaseTimer = setTimeout(() => {
      if (!this.suppressEcho) {
        return
      }
      this.finishShellSetupSuppression(stream)
    }, LiveSshSessionController.SHELL_SETUP_TIMEOUT_MS)
  }

  private scheduleShellSetupSettle(stream: { write(data: string): void }) {
    // Rebase the hard deadline after a valid marker. A marker that arrives near
    // the original deadline must not let the replacement prompt escape.
    if (this.shellSetupReleaseTimer) {
      clearTimeout(this.shellSetupReleaseTimer)
    }
    this.shellSetupReleaseTimer = setTimeout(() => {
      this.finishShellSetupSuppression(stream)
    }, LiveSshSessionController.SHELL_SETUP_TIMEOUT_MS)
    if (this.shellSetupSettleTimer) {
      clearTimeout(this.shellSetupSettleTimer)
    }
    this.shellSetupSettleTimer = setTimeout(() => {
      this.shellSetupSettleTimer = undefined
      this.finishShellSetupSuppression(stream)
    }, LiveSshSessionController.SHELL_SETUP_SETTLE_MS)
  }

  private finishShellSetupSuppression(stream: { write(data: string): void }) {
    if (!this.suppressEcho) {
      return
    }

    // Everything received after the injected command starts is internal setup
    // output until we have positively identified the pre-command prefix.  A
    // timeout must fail closed: replaying the buffer leaks the setup command on
    // fish/restricted shells and redraws another prompt.
    const visibleText =
      this.shellSetupVisiblePrefixLength === undefined ? '' : this.echoBuf.slice(0, this.shellSetupVisiblePrefixLength)
    this.clearShellSetupSuppression()
    try {
      if (visibleText) {
        this.transcript.append(visibleText)
        for (const update of this.shellCwdTracker.feed(visibleText)) {
          if (update.cwd && update.cwd !== this.shellCwd) {
            this.shellCwd = update.cwd
            this.onShellCwdChange(update.cwd)
          }
          if (update.user && update.user !== this.shellUser) {
            this.handleShellUserChange(update.user)
          }
        }
        this.trackSudoPromptFromTerminal(visibleText)
        this.onData(visibleText)
      }
    } finally {
      this.flushPendingShellInput(stream)
    }
  }

  private flushPendingShellInput(stream: { write(data: string): void }) {
    if (
      !this.connected ||
      this.shellStream !== stream ||
      !this.cwdSetupInjected ||
      this.shellPlatformProbe ||
      this.suppressEcho ||
      this.pendingShellInput.length === 0
    ) {
      return
    }

    const pendingInput = this.pendingShellInput
    this.pendingShellInput = []
    for (let index = 0; index < pendingInput.length; index += 1) {
      const data = pendingInput[index]
      try {
        stream.write(data)
        this.captureSudoPasswordInput(data)
      } catch (error) {
        this.sshDebug.log('main', `排队的终端输入写入失败: ${error instanceof Error ? error.message : String(error)}`)
        return
      }
    }
  }

  private clearShellSetupSuppression() {
    if (this.shellSetupReleaseTimer) {
      clearTimeout(this.shellSetupReleaseTimer)
      this.shellSetupReleaseTimer = undefined
    }
    if (this.shellSetupSettleTimer) {
      clearTimeout(this.shellSetupSettleTimer)
      this.shellSetupSettleTimer = undefined
    }
    this.suppressEcho = false
    this.echoBuf = ''
    this.shellSetupVisiblePrefixLength = undefined
  }

  private async verifyHostFingerprint(profile: SshProfile, key: Buffer | string): Promise<boolean> {
    const fingerprint = computeHostFingerprint(key)
    if (this.acceptedHostFingerprints.has(fingerprint)) {
      return true
    }

    if (profile.trustedHostFingerprint && profile.trustedHostFingerprint === fingerprint) {
      this.acceptedHostFingerprints.add(fingerprint)
      return true
    }

    const response = await this.requestInteraction({
      kind: 'host-verification',
      host: profile.host,
      port: profile.port,
      fingerprint,
      ...(profile.trustedHostFingerprint ? { knownFingerprint: profile.trustedHostFingerprint } : {})
    })

    if (response.kind !== 'host-verification') {
      return false
    }

    if (response.decision === 'accept-and-save') {
      await this.rememberTrustedHostFingerprint(profile.id, fingerprint)
      profile.trustedHostFingerprint = fingerprint
      ;(this.profile as SshProfile).trustedHostFingerprint = fingerprint
      this.acceptedHostFingerprints.add(fingerprint)
      return true
    }

    if (response.decision === 'accept-once') {
      this.acceptedHostFingerprints.add(fingerprint)
      return true
    }

    return false
  }

  override async disconnect(): Promise<void> {
    this.connectionGeneration += 1
    this.clearShellSetupSuppression()
    const shellStream = this.shellStream
    this.shellStream = undefined
    this.pendingShellInput = []
    this.shellPlatform = 'unknown'
    this.shellSetupCanceled = false
    this.platformProbePromise = undefined
    this.metricsRefreshPromise = undefined
    shellStream?.end()
    this.closeExecSession()
    this.closeSftpSession()
    this.closeTransferSftpSession()
    this.ssh.end()
    this.jumpSsh?.end()
    this.jumpSsh = undefined
    this.resetPrivilegedFileAccess()
    this.connected = false
  }

  getTerminalTranscript(): string {
    return this.transcript.toString()
  }

  getShellCwd(): string | undefined {
    return this.shellCwd
  }

  override getRemotePath(): string {
    return this.currentRemotePath
  }

  override getFileAccessMode(): 'user' | 'root' {
    return this.fileAccessMode
  }

  override hasReusableSudoAuth(): boolean {
    return Boolean(this.sudoPassword)
  }

  override async setFileAccessMode(mode: 'user' | 'root', options?: RemoteFileAccessOptions): Promise<void> {
    if (mode === 'root' && (this.profile as SshProfile).enableExecChannel === false) {
      throw new Error('启用 root 视角需要开启 Exec Channel')
    }

    const previousSudoUser = this.sudoUser
    const previousSudoPassword = this.sudoPassword
    const nextSudoUser = options?.sudoUser?.trim() || previousSudoUser
    const nextSudoPassword =
      options && 'sudoPassword' in options ? options.sudoPassword || undefined : previousSudoPassword
    const privilegedIdentityChanged =
      mode === 'root' && (nextSudoUser !== previousSudoUser || nextSudoPassword !== previousSudoPassword)

    if (mode === this.fileAccessMode && !privilegedIdentityChanged) {
      return
    }

    this.sudoUser = nextSudoUser
    this.sudoPassword = nextSudoPassword
    try {
      if (mode === 'root' && this.connected) {
        await this.verifyRootFileAccess()
      }
    } catch (error) {
      this.sudoUser = previousSudoUser
      this.sudoPassword = previousSudoPassword
      throw error
    }

    this.fileAccessMode = mode
    if (this.sftp && mode === 'root') {
      this.closeSftpSession()
    }
  }

  getSystemMetrics(): SystemMetrics | undefined {
    return this.metrics
  }

  async abortTransfer(): Promise<void> {
    const pipelines = [...this.activeTransferPipelines]
    await Promise.allSettled(pipelines.map(({ source, destination }) => this.stopTransferPipeline(source, destination)))
  }

  async write(data: string): Promise<void> {
    const shellStream = this.shellStream
    if (!shellStream) {
      return
    }

    if (!this.cwdSetupInjected && !this.suppressEcho) {
      this.shellSetupCanceled = true
      this.cwdSetupInjected = true
    }
    if (this.suppressEcho) {
      if (/\u0003|\u001a|\u001b/.test(data)) {
        this.pendingShellInput = []
        this.shellSetupCanceled = true
        this.clearShellSetupSuppression()
        this.captureSudoPasswordInput(data)
        shellStream.write(data)
        return
      }
      this.pendingShellInput.push(data)
      return
    }

    this.captureSudoPasswordInput(data)
    shellStream.write(data)
  }

  async resize(cols: number, rows: number, width: number, height: number): Promise<void> {
    if (this.shellStream) {
      this.shellStream.setWindow(rows, cols, Math.max(0, Math.floor(height)), Math.max(0, Math.floor(width)))
    } else {
      this.pendingResize = { cols, rows, width, height }
    }
  }

  async listRemoteFiles(): Promise<RemoteFileItem[]> {
    if (this.fileAccessMode === 'root') {
      return this.readRemoteDirectoryViaShell(this.currentRemotePath, new Error('Root file access mode enabled'))
    }

    try {
      return await this.readRemoteDirectory(this.currentRemotePath)
    } catch (error) {
      try {
        const fallbackPath = await this.resolveHomeRemotePath()
        if (fallbackPath && fallbackPath !== this.currentRemotePath) {
          this.currentRemotePath = fallbackPath
        }
        return await this.readRemoteDirectoryViaShell(this.currentRemotePath, error)
      } catch {
        const fallbackPath = await this.resolveHomeRemotePath()
        if (!fallbackPath || fallbackPath === this.currentRemotePath) {
          throw error
        }

        this.currentRemotePath = fallbackPath
        return this.readRemoteDirectoryViaShell(this.currentRemotePath, error)
      }
    }
  }

  async openRemotePath(nextPath: string): Promise<RemoteFileItem[]> {
    const previousPath = this.currentRemotePath
    this.currentRemotePath = nextPath
    try {
      if (this.fileAccessMode === 'root') {
        return await this.readRemoteDirectoryViaShell(
          this.currentRemotePath,
          new Error('Root file access mode enabled')
        )
      }

      try {
        return await this.readRemoteDirectory(this.currentRemotePath)
      } catch (error) {
        return await this.readRemoteDirectoryViaShell(this.currentRemotePath, error)
      }
    } catch (error) {
      this.currentRemotePath = previousPath
      throw error
    }
  }

  async readRemoteFile(targetPath: string, encoding = 'utf-8'): Promise<string> {
    if (this.fileAccessMode === 'root') {
      return this.readRemoteFileViaShell(targetPath, new Error('Root file access mode enabled'), encoding)
    }

    try {
      const sftp = await this.ensureSftp()
      return await this.withOperationTimeout(
        new Promise<string>((resolve, reject) => {
          sftp.readFile(targetPath, (error, data) => {
            if (error) {
              reject(error)
              return
            }
            resolve(decodeBuffer(Buffer.isBuffer(data) ? data : Buffer.from(data), encoding))
          })
        }),
        LiveSshSessionController.REMOTE_FILE_READ_TIMEOUT_MS,
        '读取远程文件超时，已重置文件通道。请重试或先下载后编辑。',
        () => this.closeSftpSession()
      )
    } catch (error) {
      return this.readRemoteFileViaShell(targetPath, error, encoding)
    }
  }

  async writeRemoteFile(targetPath: string, content: string, encoding = 'utf-8'): Promise<void> {
    if (this.fileAccessMode === 'root') {
      await this.writeRemoteFileAsPrivileged(targetPath, content, new Error('Root file access mode enabled'), encoding)
      return
    }

    try {
      const sftp = await this.ensureSftp()
      await this.ensureRemoteDirectory(path.posix.dirname(targetPath))
      const payload = encodeText(content, encoding)
      await this.withOperationTimeout(
        new Promise<void>((resolve, reject) => {
          sftp.writeFile(targetPath, payload, (error) => {
            if (error) {
              reject(error)
              return
            }
            resolve()
          })
        }),
        LiveSshSessionController.REMOTE_FILE_WRITE_TIMEOUT_MS,
        '保存远程文件超时，已重置文件通道。请重试。',
        () => this.closeSftpSession()
      )
    } catch (error) {
      await this.writeRemoteFileViaShell(targetPath, content, error, encoding)
    }
  }

  async copyRemotePath(targetPath: string, destinationPath: string, targetType: RemoteFileItem['type']): Promise<void> {
    const copyCommand = `${targetType === 'folder' ? 'cp -R' : 'cp'} -- ${shellQuote(targetPath)} ${shellQuote(destinationPath)}`
    const command = `mkdir -p ${shellQuote(path.posix.dirname(destinationPath))} && ${copyCommand}`

    if (this.fileAccessMode === 'root') {
      await this.execShellFileCommand(command, { allowNonZeroWithStdout: true }, true)
      return
    }

    await this.execCommand(`sh -lc ${shellQuote(command)}`, { allowNonZeroWithStdout: true })
  }

  async moveRemotePath(targetPath: string, destinationPath: string): Promise<void> {
    await this.renameRemotePath(targetPath, destinationPath)
  }

  async renameRemotePath(targetPath: string, nextPath: string): Promise<void> {
    if (this.fileAccessMode === 'root') {
      await this.execShellFileCommand(
        `mkdir -p ${shellQuote(path.posix.dirname(nextPath))} && mv ${shellQuote(targetPath)} ${shellQuote(nextPath)}`,
        { allowNonZeroWithStdout: true },
        true
      )
      return
    }

    try {
      const sftp = await this.ensureSftp()
      await this.ensureRemoteDirectory(path.posix.dirname(nextPath))
      await new Promise<void>((resolve, reject) => {
        sftp.rename(targetPath, nextPath, (error) => {
          if (error) {
            reject(error)
            return
          }
          resolve()
        })
      })
    } catch (error) {
      await this.execCommand(
        `sh -lc 'mkdir -p ${shellQuote(path.posix.dirname(nextPath))} && mv ${shellQuote(targetPath)} ${shellQuote(nextPath)}'`,
        { allowNonZeroWithStdout: true }
      )
      if (error && !this.sftpUnavailableReason) {
        throw error
      }
    }
  }

  async deleteRemotePath(targetPath: string, targetType: RemoteFileItem['type']): Promise<void> {
    if (this.fileAccessMode === 'root') {
      const command =
        targetType === 'folder' ? `rm -rf -- ${shellQuote(targetPath)}` : `rm -f -- ${shellQuote(targetPath)}`
      await this.execShellFileCommand(command, { allowNonZeroWithStdout: true }, true)
      return
    }

    const command =
      targetType === 'folder'
        ? `sh -lc 'rm -rf -- ${shellQuote(targetPath)}'`
        : `sh -lc 'rm -f -- ${shellQuote(targetPath)}'`
    await this.execCommand(command, { allowNonZeroWithStdout: true })
  }

  async changeRemotePermissions(targetPath: string, options: PermissionChangeOptions): Promise<void> {
    validateMode(options.mode)
    const mode = options.mode.trim()
    const recursive = Boolean(options.recursive)
    const applyTo = options.applyTo ?? 'all'

    if (this.fileAccessMode === 'root') {
      if (recursive) {
        const baseCommand =
          applyTo === 'all'
            ? `chmod -R ${shellQuote(mode)} ${shellQuote(targetPath)}`
            : applyTo === 'files'
              ? `chmod ${shellQuote(mode)} ${shellQuote(targetPath)} && find ${shellQuote(targetPath)} -type f -exec chmod ${shellQuote(mode)} {} +`
              : `chmod ${shellQuote(mode)} ${shellQuote(targetPath)} && find ${shellQuote(targetPath)} -type d -exec chmod ${shellQuote(mode)} {} +`

        await this.execShellFileCommand(baseCommand, { allowNonZeroWithStdout: true }, true)
        return
      }

      await this.execShellFileCommand(
        `chmod ${shellQuote(mode)} ${shellQuote(targetPath)}`,
        { allowNonZeroWithStdout: true },
        true
      )
      return
    }

    if (recursive) {
      const baseCommand =
        applyTo === 'all'
          ? `chmod -R ${shellQuote(mode)} ${shellQuote(targetPath)}`
          : applyTo === 'files'
            ? `chmod ${shellQuote(mode)} ${shellQuote(targetPath)} && find ${shellQuote(targetPath)} -type f -exec chmod ${shellQuote(mode)} {} +`
            : `chmod ${shellQuote(mode)} ${shellQuote(targetPath)} && find ${shellQuote(targetPath)} -type d -exec chmod ${shellQuote(mode)} {} +`

      await this.execCommand(`sh -lc ${shellQuote(baseCommand)}`, { allowNonZeroWithStdout: true })
      return
    }

    try {
      const sftp = await this.ensureSftp()
      await new Promise<void>((resolve, reject) => {
        sftp.chmod(targetPath, Number.parseInt(mode, 8), (error) => {
          if (error) {
            reject(error)
            return
          }
          resolve()
        })
      })
    } catch (error) {
      await this.execCommand(`sh -lc 'chmod ${shellQuote(mode)} ${shellQuote(targetPath)}'`, {
        allowNonZeroWithStdout: true
      })
      if (error && !this.sftpUnavailableReason) {
        throw error
      }
    }
  }

  async ensureRemoteDirectory(targetPath: string, sftpOverride?: SFTPWrapper): Promise<void> {
    const normalized = path.posix.normalize(targetPath || '.')
    if (!normalized || normalized === '.' || normalized === '/') {
      return
    }

    if (this.fileAccessMode === 'root') {
      await this.execShellFileCommand(`mkdir -p ${shellQuote(normalized)}`, { allowNonZeroWithStdout: true }, true)
      return
    }

    try {
      const sftp = sftpOverride ?? (await this.ensureSftp())
      const parts = normalized.split('/').filter(Boolean)
      let currentPath = normalized.startsWith('/') ? '/' : ''

      for (const part of parts) {
        currentPath = currentPath === '/' ? `/${part}` : currentPath ? `${currentPath}/${part}` : part

        const exists = await new Promise<boolean>((resolve) => {
          sftp.stat(currentPath, (error, stats) => {
            if (!error && stats?.isDirectory?.()) {
              resolve(true)
              return
            }
            resolve(false)
          })
        })

        if (exists) {
          continue
        }

        await new Promise<void>((resolve, reject) => {
          sftp.mkdir(currentPath, (error) => {
            if (error && !/failure/i.test(error.message)) {
              reject(error)
              return
            }
            resolve()
          })
        })
      }
    } catch (error) {
      await this.execCommand(`sh -lc 'mkdir -p ${shellQuote(normalized)}'`, { allowNonZeroWithStdout: true })
      if (error && !this.sftpUnavailableReason) {
        throw error
      }
    }
  }

  async statRemoteFile(targetPath: string): Promise<RemoteFileStat | null> {
    if (this.fileAccessMode === 'root') {
      try {
        const output = await this.execShellFileCommand(`stat -c '%s|%Y' -- ${shellQuote(targetPath)}`, undefined, true)
        const match = output.trim().match(/^(\d+)\|(\d+)/)
        return match ? { size: Number(match[1]), modifiedAt: Number(match[2]) * 1000 } : null
      } catch {
        return null
      }
    }

    const sftp = await this.ensureTransferSftp()
    return new Promise<RemoteFileStat | null>((resolve, reject) => {
      sftp.stat(targetPath, (error, attrs) => {
        if (error) {
          if (isSftpMissingError(error)) {
            resolve(null)
            return
          }
          reject(error)
          return
        }
        resolve({
          size: Math.max(attrs.size ?? 0, 0),
          modifiedAt: attrs.mtime ? attrs.mtime * 1000 : undefined
        })
      })
    })
  }

  async replaceRemoteFile(partialPath: string, destinationPath: string): Promise<void> {
    if (this.fileAccessMode === 'root') {
      await this.execShellFileCommand(
        `
set -e
if [ -L ${shellQuote(destinationPath)} ]; then
  cat -- ${shellQuote(partialPath)} > ${shellQuote(destinationPath)}
  rm -f -- ${shellQuote(partialPath)}
else
  if [ -e ${shellQuote(destinationPath)} ]; then
    chown --reference=${shellQuote(destinationPath)} -- ${shellQuote(partialPath)} 2>/dev/null || true
    chmod --reference=${shellQuote(destinationPath)} -- ${shellQuote(partialPath)} 2>/dev/null || true
  fi
  mv -f -- ${shellQuote(partialPath)} ${shellQuote(destinationPath)}
fi
`,
        { allowNonZeroWithStdout: true },
        true
      )
      return
    }

    const sftp = await this.ensureTransferSftp()
    const destinationAttrs = await sftpLstat(sftp, destinationPath)
    const partialAttrs = await sftpLstat(sftp, partialPath)
    if (
      destinationAttrs &&
      partialAttrs &&
      (destinationAttrs.isSymbolicLink() || destinationAttrs.uid !== partialAttrs.uid)
    ) {
      await pipeline(sftp.createReadStream(partialPath), sftp.createWriteStream(destinationPath, { flags: 'w' }))
      await this.verifySftpRemoteUploadSize(sftp, destinationPath, partialAttrs.size)
      await sftpCall((done) => sftp.unlink(partialPath, done))
      return
    }
    if (destinationAttrs?.mode !== undefined) {
      await sftpCall((done) => sftp.chmod(partialPath, destinationAttrs.mode & 0o7777, done)).catch(() => undefined)
    }
    try {
      await sftpCall((done) => sftp.ext_openssh_rename(partialPath, destinationPath, done))
      return
    } catch {
      // Servers without the OpenSSH extension need a reversible rename sequence.
    }

    const destination = await this.statRemoteFile(destinationPath)
    if (!destination) {
      await sftpCall((done) => sftp.rename(partialPath, destinationPath, done))
      return
    }

    const backupPath = `${destinationPath}.fileterm-backup-${randomUUID()}`
    await sftpCall((done) => sftp.rename(destinationPath, backupPath, done))
    try {
      await sftpCall((done) => sftp.rename(partialPath, destinationPath, done))
    } catch (error) {
      try {
        await sftpCall((done) => sftp.rename(backupPath, destinationPath, done))
      } catch (rollbackError) {
        throw new Error(
          `SFTP 文件替换失败，旧文件保留在 ${backupPath}：${errorMessage(error)}；回滚失败：${errorMessage(rollbackError)}`
        )
      }
      throw error
    }
    await sftpCall((done) => sftp.unlink(backupPath, done)).catch(() => undefined)
  }

  async removeRemoteFileIfExists(targetPath: string): Promise<void> {
    if (this.fileAccessMode === 'root') {
      await this.execShellFileCommand(`rm -f -- ${shellQuote(targetPath)}`, { allowNonZeroWithStdout: true }, true)
      return
    }
    const sftp = await this.ensureTransferSftp()
    await sftpCall((done) => sftp.unlink(targetPath, done)).catch((error) => {
      if (!isSftpMissingError(error)) {
        throw error
      }
    })
  }

  async uploadFile(
    localPath: string,
    remotePath: string,
    onProgress: (progress: TransferProgress) => void,
    options?: TransferFileOptions
  ): Promise<void> {
    const resumeOffset = Math.max(0, options?.resumeOffset ?? 0)
    if (this.fileAccessMode === 'root') {
      await this.uploadFileAsPrivileged(
        localPath,
        remotePath,
        onProgress,
        new Error('Root file access mode enabled'),
        resumeOffset,
        options?.signal,
        options?.stagingPath
      )
      return
    }

    await this.uploadFileAsUser(localPath, remotePath, onProgress, resumeOffset, options?.signal)
  }

  private async uploadFileAsUser(
    localPath: string,
    remotePath: string,
    onProgress: (progress: TransferProgress) => void,
    resumeOffset = 0,
    signal?: AbortSignal
  ): Promise<void> {
    let transferredBytes = 0
    try {
      const sftp = await this.ensureTransferSftp()
      const info = await stat(localPath)
      this.throwIfTransferAborted(signal)
      const total = info.size
      const progressTotal = Math.max(total, 1)
      if (resumeOffset > total) {
        throw new Error('SFTP 上传断点大于源文件，无法继续')
      }
      transferredBytes = resumeOffset
      appLog(`[FileTerm][SFTP] Upload start ${localPath} -> ${remotePath} (${formatShellBytes(total)})`)
      await this.ensureRemoteDirectory(path.posix.dirname(remotePath), sftp)
      this.throwIfTransferAborted(signal)
      if (resumeOffset < total || total === 0) {
        const localStream = createReadStream(localPath, { start: resumeOffset })
        const remoteStream = sftp.createWriteStream(remotePath, {
          flags: resumeOffset > 0 ? 'r+' : 'w',
          start: resumeOffset > 0 ? resumeOffset : undefined,
          mode: 0o644
        })
        const reportProgress = () => {
          // SFTP write streams acknowledge bytes asynchronously, so progress must follow
          // confirmed remote writes rather than optimistic local reads.
          const acknowledgedBytes = Math.min(total, resumeOffset + this.getWritableBytesWritten(remoteStream))
          if (acknowledgedBytes <= transferredBytes) {
            return
          }
          transferredBytes = acknowledgedBytes
          onProgress({
            percent: Math.min(99, Math.round((transferredBytes / progressTotal) * 100)),
            transferredBytes,
            totalBytes: total
          })
        }
        remoteStream.on('drain', reportProgress)
        remoteStream.on('finish', reportProgress)
        remoteStream.on('close', reportProgress)
        await this.runTransferPipeline(localStream, remoteStream, signal)
        reportProgress()
      }
      await this.verifySftpRemoteUploadSize(sftp, remotePath, total)
      appLog(`[FileTerm][SFTP] Upload verified ${remotePath} (${formatShellBytes(total)})`)
      onProgress({ percent: 100, transferredBytes: total, totalBytes: total })
    } catch (error) {
      if (signal?.aborted) {
        throw this.transferAbortError()
      }
      if (transferredBytes > resumeOffset || resumeOffset > 0) {
        appWarn(
          `[FileTerm][SFTP] Upload interrupted after ${formatShellBytes(transferredBytes)}: ${localPath} -> ${remotePath}`,
          error
        )
        throw new Error(`SFTP 上传已中断，已停止以避免提交不完整文件：${errorMessage(error)}`)
      }
      appWarn(
        `[FileTerm][SFTP] Upload could not start, falling back to shell stream: ${localPath} -> ${remotePath}`,
        error
      )
      await this.uploadFileViaShell(localPath, remotePath, onProgress, error, false, signal)
    }
  }

  async downloadFile(
    remotePath: string,
    localPath: string,
    onProgress: (progress: TransferProgress) => void,
    options?: TransferFileOptions
  ): Promise<void> {
    const resumeOffset = Math.max(0, options?.resumeOffset ?? 0)
    if (this.fileAccessMode === 'root') {
      await this.downloadFileViaShell(
        remotePath,
        localPath,
        onProgress,
        new Error('Root file access mode enabled'),
        resumeOffset,
        options?.signal
      )
      return
    }

    let transferredBytes = resumeOffset
    try {
      const sftp = await this.ensureTransferSftp()
      const attrs = await new Promise<{ size?: number }>((resolve, reject) => {
        sftp.stat(remotePath, (error, stats) => {
          if (error || !stats) {
            reject(error ?? new Error(`Failed to stat remote file: ${remotePath}`))
            return
          }
          resolve(stats)
        })
      })
      this.throwIfTransferAborted(options?.signal)
      const total = Math.max(attrs.size ?? 0, 0)
      const progressTotal = Math.max(total, 1)
      if (resumeOffset > total) {
        throw new Error('SFTP 下载断点大于远端文件，无法继续')
      }
      appLog(`[FileTerm][SFTP] Download start ${remotePath} -> ${localPath} (${formatShellBytes(total)})`)
      if (resumeOffset < total || total === 0) {
        const remoteStream = sftp.createReadStream(remotePath, {
          start: resumeOffset > 0 ? resumeOffset : undefined
        })
        const localStream = createWriteStream(localPath, {
          flags: resumeOffset > 0 ? 'r+' : 'w',
          start: resumeOffset > 0 ? resumeOffset : undefined
        })
        const reportProgress = () => {
          const acknowledgedBytes = Math.min(total, resumeOffset + this.getWritableBytesWritten(localStream))
          if (acknowledgedBytes <= transferredBytes) {
            return
          }
          transferredBytes = acknowledgedBytes
          onProgress({
            percent: Math.min(99, Math.round((transferredBytes / progressTotal) * 100)),
            transferredBytes,
            totalBytes: total
          })
        }
        localStream.on('drain', reportProgress)
        localStream.on('finish', reportProgress)
        localStream.on('close', reportProgress)
        await this.runTransferPipeline(remoteStream, localStream, options?.signal)
        reportProgress()
      }
      const localInfo = await stat(localPath)
      this.assertRemoteUploadSize(localPath, localInfo.size, total)
      appLog(`[FileTerm][SFTP] Download verified ${remotePath} -> ${localPath} (${formatShellBytes(total)})`)
      onProgress({ percent: 100, transferredBytes: total, totalBytes: total })
    } catch (error) {
      if (options?.signal?.aborted) {
        throw this.transferAbortError()
      }
      if (transferredBytes > resumeOffset || resumeOffset > 0) {
        appWarn(
          `[FileTerm][SFTP] Download interrupted after ${formatShellBytes(transferredBytes)}: ${remotePath} -> ${localPath}`,
          error
        )
        throw new Error(`SFTP 下载已中断，已停止以避免提交不完整文件：${errorMessage(error)}`)
      }
      appWarn(
        `[FileTerm][SFTP] Download could not start, falling back to shell stream: ${remotePath} -> ${localPath}`,
        error
      )
      await this.downloadFileViaShell(remotePath, localPath, onProgress, error, resumeOffset, options?.signal)
    }
  }

  async refreshSystemMetrics(): Promise<SystemMetrics | undefined> {
    const profile = this.profile as SshProfile
    if (profile.enableExecChannel === false || profile.enableResourceMonitoring === false) {
      this.metrics = undefined
      return undefined
    }

    if (this.metricsRefreshPromise) {
      return this.metricsRefreshPromise
    }

    const refresh = this.collectSystemMetrics()
    this.metricsRefreshPromise = refresh
    try {
      return await refresh
    } finally {
      if (this.metricsRefreshPromise === refresh) {
        this.metricsRefreshPromise = undefined
      }
    }
  }

  private async collectSystemMetrics(): Promise<SystemMetrics | undefined> {
    const startedAt = Date.now()
    const generation = this.connectionGeneration
    try {
      const platform = await this.resolveRemoteSystemPlatform()
      if (platform === 'unknown') {
        throw new Error('无法识别远端系统平台')
      }
      const result = await collectSshSystemMetrics(
        {
          exec: (command, options, stdinPayload) => this.execCommand(command, options, false, stdinPayload)
        },
        platform
      )
      if (!this.connected || this.connectionGeneration !== generation) {
        return undefined
      }
      this.metricsPlatform = result.platform
      this.metrics = result.metrics
      return this.metrics
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      this.sshDebug.log('exec', `系统信息采集失败: ${message}`)
      const now = Date.now()
      if (now - this.lastMetricsWarningAt >= LiveSshSessionController.METRICS_WARNING_INTERVAL_MS) {
        this.lastMetricsWarningAt = now
        appWarn(
          `[FileTerm][Metrics] Collection failed after ${now - startedAt}ms (platform=${this.metricsPlatform ?? 'unknown'})`,
          error
        )
      }
      return undefined
    }
  }

  pushClientNotice(message: string) {
    this.appendSystemMessage(`[FileTerm] ${message}\r\n`)
    this.onStateChange(this.getSummary(), this.transcript.toString(), this.connected)
  }

  private async ensureSftp(): Promise<SFTPWrapper> {
    if (this.sftp) {
      return this.sftp
    }

    if (!this.sshConfig) {
      throw new Error('SFTP connection not initialized')
    }

    const sshConfig = this.sshConfig
    const channelSshConfig = await this.withOutboundSocket(sshConfig, this.profile as SshProfile)
    this.registerSftpLifecycle()
    await new Promise<void>((resolve, reject) => {
      let settled = false
      this.sshDebug.logSftpStart(sshConfig.username)
      this.sftpSsh.removeAllListeners('keyboard-interactive')
      this.sftpSsh.removeAllListeners('banner')
      this.sftpSsh.on('banner', (message) => {
        this.sshDebug.log('sftp', `服务端横幅: ${singleLine(message)}`)
      })
      this.registerKeyboardInteractiveHandler(this.sftpSsh, this.profile as SshProfile, 'sftp', (message) => {
        this.sshDebug.logKeyboardInteractive('sftp', message)
      })
      const onReady = () => {
        cleanup()
        settled = true
        this.sshDebug.log('sftp', 'SFTP 认证完成')
        resolve()
      }
      const onError = (error: Error) => {
        cleanup()
        if (!settled) {
          settled = true
          this.sftpUnavailableReason = error.message
          this.sshDebug.log('sftp', `连接错误: ${error.message}`)
          reject(error)
        }
      }
      const onClose = () => {
        cleanup()
        if (!settled) {
          settled = true
          this.sftpUnavailableReason = 'SFTP SSH connection closed'
          this.sshDebug.log('sftp', '连接在握手阶段被关闭')
          reject(new Error('SFTP SSH connection closed'))
        }
      }
      const cleanup = () => {
        this.sftpSsh.off('ready', onReady)
        this.sftpSsh.off('error', onError)
        this.sftpSsh.off('close', onClose)
      }

      this.sftpSsh
        .once('ready', onReady)
        .once('error', onError)
        .once('close', onClose)
        .connect({
          ...channelSshConfig,
          keepaliveInterval: 3000,
          keepaliveCountMax: 4,
          readyTimeout: Math.max(sshConfig.readyTimeout ?? 15000, 20000),
          ...(this.sshDebug.enabled ? { debug: (message: string) => this.sshDebug.handle('sftp', message) } : {})
        })
    })

    return new Promise<SFTPWrapper>((resolve, reject) => {
      this.sftpSsh.sftp((error, sftp) => {
        if (error || !sftp) {
          this.sftpUnavailableReason = error?.message ?? 'Failed to open SFTP session'
          reject(error ?? new Error('Failed to open SFTP session'))
          return
        }
        this.sftpUnavailableReason = null
        this.attachSftpWrapperLifecycle(sftp, 'sftp')
        this.sftp = sftp
        resolve(sftp)
      })
    })
  }

  private async ensureTransferSftp(): Promise<SFTPWrapper> {
    if (this.transferSftp) {
      return this.transferSftp
    }

    if (!this.sshConfig) {
      throw new Error('Transfer SFTP connection not initialized')
    }

    if (this.transferSshClosed) {
      this.resetTransferClient(false)
    }

    const sshConfig = this.sshConfig
    const channelSshConfig = await this.withOutboundSocket(sshConfig, this.profile as SshProfile)
    this.registerTransferLifecycle()
    await new Promise<void>((resolve, reject) => {
      let settled = false
      this.transferSshClosed = false
      this.sshDebug.log(
        'transfer-sftp',
        `准备建立传输 SFTP 连接: ${sshConfig.username}@${sshConfig.host}:${sshConfig.port}`
      )
      this.transferSsh.removeAllListeners('keyboard-interactive')
      this.transferSsh.removeAllListeners('banner')
      this.transferSsh.on('banner', (message) => {
        this.sshDebug.log('transfer-sftp', `服务端横幅: ${singleLine(message)}`)
      })
      this.registerKeyboardInteractiveHandler(
        this.transferSsh,
        this.profile as SshProfile,
        'transfer-sftp',
        (message) => {
          this.sshDebug.logKeyboardInteractive('transfer-sftp', message)
        }
      )
      const onReady = () => {
        cleanup()
        settled = true
        this.sshDebug.log('transfer-sftp', '传输 SFTP 认证完成')
        resolve()
      }
      const onError = (error: Error) => {
        cleanup()
        if (!settled) {
          settled = true
          this.sshDebug.log('transfer-sftp', `连接错误: ${error.message}`)
          reject(error)
        }
      }
      const onClose = () => {
        cleanup()
        if (!settled) {
          settled = true
          this.sshDebug.log('transfer-sftp', '连接在握手阶段被关闭')
          reject(new Error('Transfer SFTP SSH connection closed'))
        }
      }
      const cleanup = () => {
        this.transferSsh.off('ready', onReady)
        this.transferSsh.off('error', onError)
        this.transferSsh.off('close', onClose)
      }

      this.transferSsh
        .once('ready', onReady)
        .once('error', onError)
        .once('close', onClose)
        .connect({
          ...channelSshConfig,
          keepaliveInterval: 3000,
          keepaliveCountMax: 4,
          readyTimeout: Math.max(sshConfig.readyTimeout ?? 15000, 20000),
          ...(this.sshDebug.enabled
            ? { debug: (message: string) => this.sshDebug.handle('transfer-sftp', message) }
            : {})
        })
    })

    return new Promise<SFTPWrapper>((resolve, reject) => {
      this.transferSsh.sftp((error, sftp) => {
        if (error || !sftp) {
          reject(error ?? new Error('Failed to open transfer SFTP session'))
          return
        }
        this.attachSftpWrapperLifecycle(sftp, 'transfer-sftp')
        this.transferSftp = sftp
        resolve(sftp)
      })
    })
  }

  private handlePrimaryDisconnect(stream?: { write(data: string): void }) {
    if (stream && this.shellStream !== stream) {
      return
    }

    this.clearShellSetupSuppression()
    this.connectionGeneration += 1
    this.pendingShellInput = []
    this.shellPlatform = 'unknown'
    this.shellSetupCanceled = false
    this.shellStream = undefined
    this.shellPlatformProbe = undefined
    this.platformProbePromise = undefined
    this.metricsRefreshPromise = undefined
    this.resetPrivilegedFileAccess()
    this.connected = false
    void this.tunnelService?.stopAll()
    this.tunnelService = undefined
    this.closeExecSession()
    this.closeSftpSession()
    this.closeTransferSftpSession()
  }

  private registerSftpLifecycle() {
    if (this.hasRegisteredSftpLifecycle) {
      return
    }

    this.hasRegisteredSftpLifecycle = true
    const markClosed = () => {
      this.sftp?.end?.()
      this.sftp = undefined
      this.sftpUnavailableReason = 'SFTP SSH connection closed'
    }

    this.sftpSsh.on('error', (error) => {
      this.sshDebug.log('sftp', `连接异常断开: ${error.message}`)
      markClosed()
    })
    this.sftpSsh.on('close', markClosed)
    this.sftpSsh.on('end', markClosed)
  }

  private registerTransferLifecycle() {
    if (this.hasRegisteredTransferLifecycle) {
      return
    }

    this.hasRegisteredTransferLifecycle = true
    const markClosed = () => {
      this.releaseTransferSftpHandle()
      this.transferSshClosed = true
    }

    this.transferSsh.on('error', (error) => {
      this.sshDebug.log('transfer-sftp', `连接异常断开: ${error.message}`)
      markClosed()
    })
    this.transferSsh.on('close', markClosed)
    this.transferSsh.on('end', markClosed)
  }

  private closeSftpSession() {
    this.sftp?.end?.()
    this.sftp = undefined
    this.sftpUnavailableReason = 'SFTP session closed'
    this.sftpSsh.end()
  }

  private closeTransferSftpSession() {
    this.releaseTransferSftpHandle()
    this.transferSsh.end()
    this.transferSshClosed = true
  }

  private releaseTransferSftpHandle() {
    this.transferSftp?.end?.()
    this.transferSftp = undefined
  }

  private resetTransferClient(endCurrent = true) {
    const currentTransferSsh = this.transferSsh
    if (endCurrent) {
      currentTransferSsh.end()
    }
    this.transferSsh = this.createTransferClient()
    this.hasRegisteredTransferLifecycle = false
    this.transferSshClosed = false
  }

  private attachSftpWrapperLifecycle(sftp: SFTPWrapper, scope: 'sftp' | 'transfer-sftp') {
    const emitter = sftp as SFTPWrapper & {
      on?(event: string, listener: (...args: unknown[]) => void): unknown
      once?(event: string, listener: (...args: unknown[]) => void): unknown
    }

    emitter.on?.('error', (error: unknown) => {
      this.sshDebug.log(scope, `SFTP wrapper error: ${errorMessage(error)}`)
    })
    emitter.on?.('end', () => {
      this.sshDebug.log(scope, 'SFTP wrapper end')
    })
    emitter.on?.('close', () => {
      this.sshDebug.log(scope, 'SFTP wrapper close')
    })
  }

  private async ensureExecConnection(): Promise<Client> {
    if (this.execReady) {
      return this.execSsh
    }

    if (this.execConnectPromise) {
      return this.execConnectPromise
    }

    if (!this.sshConfig) {
      throw new Error('Exec connection not initialized')
    }

    const sshConfig = this.sshConfig
    const channelSshConfig = await this.withOutboundSocket(sshConfig, this.profile as SshProfile)
    this.registerExecLifecycle()
    this.execConnectPromise = new Promise<Client>((resolve, reject) => {
      let settled = false
      this.sshDebug.log('exec', `准备建立 Exec 连接: ${sshConfig.username}@${sshConfig.host}:${sshConfig.port}`)
      this.execSsh.removeAllListeners('keyboard-interactive')
      this.execSsh.removeAllListeners('banner')
      this.execSsh.on('banner', (message) => {
        this.sshDebug.log('exec', `服务端横幅: ${singleLine(message)}`)
      })
      this.registerKeyboardInteractiveHandler(this.execSsh, this.profile as SshProfile, 'exec', (message) => {
        this.sshDebug.logKeyboardInteractive('exec', message)
      })
      const onReady = () => {
        cleanup()
        settled = true
        this.execReady = true
        this.execConnectPromise = undefined
        this.sshDebug.log('exec', 'Exec 认证完成')
        resolve(this.execSsh)
      }
      const onError = (error: Error) => {
        cleanup()
        this.execReady = false
        this.execConnectPromise = undefined
        if (!settled) {
          settled = true
          this.sshDebug.log('exec', `连接错误: ${error.message}`)
          reject(error)
        }
      }
      const onClose = () => {
        cleanup()
        this.execReady = false
        this.execConnectPromise = undefined
        if (!settled) {
          settled = true
          this.sshDebug.log('exec', '连接在握手阶段被关闭')
          reject(new Error('Exec SSH connection closed'))
        }
      }
      const cleanup = () => {
        this.execSsh.off('ready', onReady)
        this.execSsh.off('error', onError)
        this.execSsh.off('close', onClose)
      }

      this.execSsh
        .once('ready', onReady)
        .once('error', onError)
        .once('close', onClose)
        .connect({
          ...channelSshConfig,
          ...(this.sshDebug.enabled ? { debug: (message: string) => this.sshDebug.handle('exec', message) } : {})
        })
    })

    return this.execConnectPromise
  }

  private registerExecLifecycle() {
    if (this.hasRegisteredExecLifecycle) {
      return
    }

    this.hasRegisteredExecLifecycle = true
    const markClosed = () => {
      this.execReady = false
      this.execConnectPromise = undefined
    }
    this.execSsh.on('error', (error) => {
      this.sshDebug.log('exec', `连接异常断开: ${error.message}`)
      markClosed()
    })
    this.execSsh.on('close', markClosed)
    this.execSsh.on('end', markClosed)
  }

  private closeExecSession() {
    this.execReady = false
    this.execConnectPromise = undefined
    this.execSsh.end()
  }

  private async readRemoteDirectory(targetPath: string): Promise<RemoteFileItem[]> {
    const sftp = await this.ensureSftp()
    const entries = await new Promise<FileEntry[]>((resolve, reject) => {
      sftp.readdir(targetPath, (error, list) => {
        if (error || !list) {
          reject(error ?? new Error(`Failed to read remote directory: ${targetPath}`))
          return
        }
        resolve(list)
      })
    })

    const rows = entries
      .filter((entry) => entry.filename !== '.' && entry.filename !== '..')
      .map((entry) => toRemoteFileItem(targetPath, entry))
      .sort((left, right) => {
        if (left.type !== right.type) {
          return left.type === 'folder' ? -1 : 1
        }
        return left.name.localeCompare(right.name)
      })

    if (targetPath !== '/') {
      rows.unshift({
        path: parentRemotePath(targetPath),
        name: '..',
        type: 'folder',
        modified: '',
        size: '-',
        permission: '',
        ownerGroup: ''
      })
    }

    return rows
  }

  private async resolveHomeRemotePath(): Promise<string | null> {
    try {
      const sftp = await this.ensureSftp()
      return await new Promise<string | null>((resolve) => {
        sftp.realpath('.', (error, resolvedPath) => {
          if (error || !resolvedPath) {
            resolve(null)
            return
          }
          resolve(resolvedPath)
        })
      })
    } catch {
      const output = await this.execCommand("sh -lc 'pwd'")
      return output.trim() || null
    }
  }

  private async readRemoteFileViaShell(targetPath: string, cause: unknown, encoding = 'utf-8'): Promise<string> {
    this.ensureShellFileFallback(cause)
    const output = await this.execShellFileCommand(
      `base64 ${shellQuote(targetPath)}`,
      undefined,
      this.fileAccessMode === 'root'
    )
    return decodeBuffer(Buffer.from(output.replace(/\s+/g, ''), 'base64'), encoding)
  }

  private async writeRemoteFileViaShell(
    targetPath: string,
    content: string,
    cause: unknown,
    encoding = 'utf-8'
  ): Promise<void> {
    this.ensureShellFileFallback(cause)
    const payload = encodeText(content, encoding).toString('base64')
    await this.execShellFileCommand(
      `base64 -d > ${shellQuote(targetPath)}`,
      undefined,
      this.fileAccessMode === 'root',
      `${payload}\n`
    )
  }

  private async writeRemoteFileAsPrivileged(
    targetPath: string,
    content: string,
    cause: unknown,
    encoding = 'utf-8'
  ): Promise<void> {
    this.ensureShellFileFallback(cause)
    const tempRemotePath = await this.createTemporaryRemoteUploadPath(path.posix.basename(targetPath))
    const payload = encodeText(content, encoding).toString('base64')

    try {
      await this.execCommand(`sh -lc ${shellQuote(`base64 -d > ${tempRemotePath}`)}`, undefined, false, `${payload}\n`)
      await this.ensureRemoteDirectory(path.posix.dirname(targetPath))
      await this.execShellFileCommand(
        `mv ${shellQuote(tempRemotePath)} ${shellQuote(targetPath)}`,
        { allowNonZeroWithStdout: true },
        true
      )
    } catch (error) {
      this.scheduleTransferStagingCleanup(tempRemotePath)
      throw error
    }
  }

  private async uploadFileViaShell(
    localPath: string,
    remotePath: string,
    onProgress: (progress: TransferProgress) => void,
    cause: unknown,
    privileged = this.fileAccessMode === 'root',
    signal?: AbortSignal
  ): Promise<void> {
    this.ensureShellFileFallback(cause)
    const fileInfo = await stat(localPath)
    const total = fileInfo.size
    const progressTotal = Math.max(total, 1)
    appLog(
      `[FileTerm][SSH] Shell upload start ${localPath} -> ${remotePath} (${formatShellBytes(total)}, privileged=${privileged ? 'yes' : 'no'})`
    )
    onProgress({ percent: 1, transferredBytes: 0, totalBytes: total })
    await this.streamLocalFileToShellCommand(
      localPath,
      `cat > ${shellQuote(remotePath)}`,
      privileged,
      total,
      (transferredBytes) => {
        onProgress({
          percent: Math.min(99, Math.max(1, Math.round((transferredBytes / progressTotal) * 100))),
          transferredBytes,
          totalBytes: total
        })
      },
      signal
    )
    await this.verifyShellRemoteUploadSize(remotePath, total, privileged)
    appLog(
      `[FileTerm][SSH] Shell upload verified ${remotePath} (${formatShellBytes(total)}, privileged=${privileged ? 'yes' : 'no'})`
    )
    onProgress({ percent: 100, transferredBytes: total, totalBytes: total })
  }

  private async uploadFileAsPrivileged(
    localPath: string,
    remotePath: string,
    onProgress: (progress: TransferProgress) => void,
    cause: unknown,
    resumeOffset = 0,
    signal?: AbortSignal,
    stagingPath?: string
  ): Promise<void> {
    this.ensureShellFileFallback(cause)
    const total = (await stat(localPath)).size
    if (resumeOffset > total) {
      throw new Error('root SFTP 上传断点大于源文件，无法继续')
    }
    const tempRemotePath = stagingPath ?? (await this.createTemporaryRemoteUploadPath(path.posix.basename(remotePath)))
    appLog(`[FileTerm][SFTP] Root upload staging ${localPath}@${resumeOffset} -> ${tempRemotePath} -> ${remotePath}`)

    try {
      const sftp = await this.ensureTransferSftp()
      let stagedSize = (await this.readSftpFileSize(sftp, tempRemotePath)) ?? 0
      if (stagedSize > total) {
        appWarn(`[FileTerm][SFTP] Root staging ${tempRemotePath} is larger than its source; rebuilding it`)
        await sftpCall((done) => sftp.unlink(tempRemotePath, done)).catch((error) => {
          if (!isSftpMissingError(error)) {
            throw error
          }
        })
        stagedSize = 0
      }
      await this.uploadFileSliceAsUser(
        localPath,
        tempRemotePath,
        stagedSize,
        stagedSize,
        (progress) => {
          onProgress({
            percent: Math.min(99, Math.max(1, progress.percent === 100 ? 99 : progress.percent)),
            transferredBytes: progress.transferredBytes,
            totalBytes: progress.totalBytes,
            message: undefined
          })
        },
        signal
      )
      this.throwIfTransferAborted(signal)
      onProgress({
        percent: 99,
        transferredBytes: total,
        totalBytes: total,
        message: '正在应用 root 写入...'
      })
      await this.ensureRemoteDirectory(path.posix.dirname(remotePath))
      this.throwIfTransferAborted(signal)
      if (resumeOffset > 0) {
        await this.execShellFileCommand(
          `
set -e
current=$(stat -c %s -- ${shellQuote(remotePath)} 2>/dev/null || wc -c < ${shellQuote(remotePath)} 2>/dev/null || echo -1)
[ "$current" = ${resumeOffset} ] || { echo "resume offset changed: $current" >&2; exit 74; }
tail -c +${resumeOffset + 1} -- ${shellQuote(tempRemotePath)} >> ${shellQuote(remotePath)}
rm -f -- ${shellQuote(tempRemotePath)}
`,
          { allowNonZeroWithStdout: true },
          true
        )
      } else {
        await this.execShellFileCommand(
          `mv -f -- ${shellQuote(tempRemotePath)} ${shellQuote(remotePath)}`,
          { allowNonZeroWithStdout: true },
          true
        )
      }
      await this.verifyShellRemoteUploadSize(remotePath, total, true)
      appLog(`[FileTerm][SFTP] Root upload verified ${remotePath} (${formatShellBytes(total)})`)
      onProgress({
        percent: 100,
        transferredBytes: total,
        totalBytes: total,
        message: undefined
      })
    } catch (error) {
      if (!stagingPath) {
        this.scheduleTransferStagingCleanup(tempRemotePath)
      }
      throw error
    }
  }

  private async uploadFileSliceAsUser(
    localPath: string,
    remotePath: string,
    sourceOffset: number,
    remoteOffset: number,
    onProgress: (progress: TransferProgress) => void,
    signal?: AbortSignal
  ) {
    const sftp = await this.ensureTransferSftp()
    const total = (await stat(localPath)).size
    const progressTotal = Math.max(total, 1)
    let transferredBytes = sourceOffset
    onProgress({
      percent: Math.min(99, Math.round((transferredBytes / progressTotal) * 100)),
      transferredBytes,
      totalBytes: total
    })
    if (sourceOffset < total) {
      const localStream = createReadStream(localPath, { start: sourceOffset })
      const remoteStream = sftp.createWriteStream(remotePath, {
        flags: remoteOffset > 0 ? 'r+' : 'w',
        start: remoteOffset > 0 ? remoteOffset : undefined,
        mode: 0o600
      })
      const reportProgress = () => {
        const acknowledgedBytes = Math.min(total, sourceOffset + this.getWritableBytesWritten(remoteStream))
        if (acknowledgedBytes <= transferredBytes) {
          return
        }
        transferredBytes = acknowledgedBytes
        onProgress({
          percent: Math.min(99, Math.round((transferredBytes / progressTotal) * 100)),
          transferredBytes,
          totalBytes: total
        })
      }
      remoteStream.on('drain', reportProgress)
      remoteStream.on('finish', reportProgress)
      remoteStream.on('close', reportProgress)
      await this.runTransferPipeline(localStream, remoteStream, signal)
      reportProgress()
    } else if (total === 0 && remoteOffset === 0) {
      const emptySource = Readable.from([])
      const remoteStream = sftp.createWriteStream(remotePath, { flags: 'w', mode: 0o600 })
      await this.runTransferPipeline(emptySource, remoteStream, signal)
    }
    await this.verifySftpRemoteUploadSize(sftp, remotePath, total - (sourceOffset - remoteOffset))
    onProgress({ percent: 100, transferredBytes: total, totalBytes: total })
  }

  private async readSftpFileSize(sftp: SFTPWrapper, remotePath: string): Promise<number | null> {
    try {
      const attrs = await new Promise<{ size?: number }>((resolve, reject) => {
        sftp.stat(remotePath, (error, stats) => {
          if (error || !stats) {
            reject(error ?? new Error(`Failed to stat remote file: ${remotePath}`))
            return
          }
          resolve(stats)
        })
      })
      return typeof attrs.size === 'number' ? attrs.size : null
    } catch (error) {
      if (isSftpMissingError(error)) {
        return null
      }
      throw error
    }
  }

  private async verifySftpRemoteUploadSize(sftp: SFTPWrapper, remotePath: string, expectedSize: number): Promise<void> {
    const attrs = await new Promise<{ size?: number }>((resolve, reject) => {
      sftp.stat(remotePath, (error, stats) => {
        if (error || !stats) {
          reject(error ?? new Error(`Failed to stat remote file: ${remotePath}`))
          return
        }
        resolve(stats)
      })
    })
    this.assertRemoteUploadSize(remotePath, attrs.size, expectedSize)
  }

  private async runTransferPipeline(source: Readable, destination: Writable, signal?: AbortSignal): Promise<void> {
    const activePipeline = { source, destination }
    let rejectAbort: ((error: Error) => void) | undefined
    const abortPromise = new Promise<never>((_resolve, reject) => {
      rejectAbort = reject
    })
    const abortPipeline = () => {
      void this.stopTransferPipeline(source, destination)
        .catch(() => undefined)
        .finally(() => rejectAbort?.(this.transferAbortError()))
    }
    this.activeTransferPipelines.add(activePipeline)
    signal?.addEventListener('abort', abortPipeline, { once: true })
    let pipelinePromise: Promise<void> | undefined
    try {
      if (signal?.aborted) {
        await this.stopTransferPipeline(source, destination)
        throw this.transferAbortError()
      }
      pipelinePromise = pipeline(source, destination)
      await (signal ? Promise.race([pipelinePromise, abortPromise]) : pipelinePromise)
    } finally {
      signal?.removeEventListener('abort', abortPipeline)
      this.activeTransferPipelines.delete(activePipeline)
      void pipelinePromise?.catch(() => undefined)
    }
  }

  private async stopTransferPipeline(source: Readable, destination: Writable): Promise<void> {
    if (source.destroyed && destination.destroyed) {
      return
    }

    source.pause()
    source.unpipe(destination)

    // Let SFTP acknowledge data already handed to the writable before closing
    // its file handle. Destroying both streams at once can leave a zero-byte
    // checkpoint even though the renderer has already observed progress.
    if (!destination.destroyed && !destination.writableEnded && !destination.writableFinished) {
      await new Promise<void>((resolve) => {
        let settled = false
        const finish = () => {
          if (settled) return
          settled = true
          clearTimeout(timeout)
          destination.off('finish', finish)
          destination.off('close', finish)
          destination.off('error', finish)
          resolve()
        }
        const timeout = setTimeout(finish, 5_000)
        destination.once('finish', finish)
        destination.once('close', finish)
        destination.once('error', finish)
        destination.end()
      })
    }

    if (!source.destroyed) {
      source.destroy()
    }
    if (!destination.destroyed) {
      destination.destroy()
    }
  }

  private getWritableBytesWritten(stream: Writable): number {
    const value = (stream as Writable & { bytesWritten?: unknown }).bytesWritten
    return typeof value === 'number' && Number.isFinite(value) && value > 0 ? value : 0
  }

  private throwIfTransferAborted(signal?: AbortSignal) {
    if (signal?.aborted) {
      throw this.transferAbortError()
    }
  }

  private transferAbortError() {
    return new Error('传输已暂停')
  }

  private scheduleTransferStagingCleanup(remotePath: string) {
    void this.ensureTransferSftp()
      .then((sftp) => sftpCall((done) => sftp.unlink(remotePath, done)))
      .catch((error) => {
        if (!isSftpMissingError(error)) {
          appWarn(`[FileTerm][SFTP] Could not clean transfer staging file ${remotePath}`, error)
        }
      })
  }

  private async verifyShellRemoteUploadSize(
    remotePath: string,
    expectedSize: number,
    privileged: boolean
  ): Promise<void> {
    const output = await this.execShellFileCommand(
      `stat -c %s -- ${shellQuote(remotePath)} || wc -c < ${shellQuote(remotePath)}`,
      undefined,
      privileged
    )
    const remoteSize = parseRemoteByteSize(output)
    if (remoteSize !== undefined) {
      this.assertRemoteUploadSize(remotePath, remoteSize, expectedSize)
      return
    }

    appWarn(
      `[FileTerm][SSH] Shell upload size check returned no parseable size for ${remotePath}: ${singleLine(output) || '(empty)'}`
    )
    try {
      const sftp = await this.ensureTransferSftp()
      await this.verifySftpRemoteUploadSize(sftp, remotePath, expectedSize)
    } catch (error) {
      appWarn(`[FileTerm][SFTP] Fallback upload size check failed for ${remotePath}`, error)
      this.assertRemoteUploadSize(remotePath, undefined, expectedSize)
    }
  }

  private assertRemoteUploadSize(remotePath: string, remoteSize: number | undefined, expectedSize: number): void {
    if (remoteSize === expectedSize) {
      return
    }

    const actual = typeof remoteSize === 'number' ? formatShellBytes(remoteSize) : '未知大小'
    throw new Error(
      `传输校验失败：${path.posix.basename(remotePath)} 实际为 ${actual}，期望 ${formatShellBytes(expectedSize)}`
    )
  }

  private async downloadFileViaShell(
    remotePath: string,
    localPath: string,
    onProgress: (progress: TransferProgress) => void,
    cause: unknown,
    resumeOffset = 0,
    signal?: AbortSignal
  ): Promise<void> {
    this.ensureShellFileFallback(cause)
    const remoteIdentity = await this.statRemoteFile(remotePath)
    if (!remoteIdentity) {
      throw new Error(`远端文件不存在或无法读取：${remotePath}`)
    }
    if (resumeOffset > remoteIdentity.size) {
      throw new Error('root SFTP 下载断点大于远端文件，无法继续')
    }
    appLog(`[FileTerm][SSH] Shell download start ${remotePath}@${resumeOffset} -> ${localPath}`)
    const total = remoteIdentity.size
    await this.streamShellFileToLocal(
      remotePath,
      localPath,
      resumeOffset,
      total,
      this.fileAccessMode === 'root',
      (transferredBytes) =>
        onProgress({
          percent: Math.min(99, Math.round((transferredBytes / Math.max(total, 1)) * 100)),
          transferredBytes,
          totalBytes: total
        }),
      signal
    )
    const localInfo = await stat(localPath)
    this.assertRemoteUploadSize(localPath, localInfo.size, total)
    appLog(`[FileTerm][SSH] Shell download verified ${remotePath} -> ${localPath} (${formatShellBytes(total)})`)
    onProgress({ percent: 100, transferredBytes: total, totalBytes: total })
  }

  private async streamShellFileToLocal(
    remotePath: string,
    localPath: string,
    resumeOffset: number,
    expectedBytes: number,
    privileged: boolean,
    onProgress: (transferredBytes: number) => void,
    signal?: AbortSignal
  ): Promise<void> {
    const execClient = await this.ensureExecConnection()
    const shellCommand =
      resumeOffset > 0
        ? `tail -c +${resumeOffset + 1} -- ${shellQuote(remotePath)}`
        : `cat -- ${shellQuote(remotePath)}`
    const command = privileged
      ? this.sudoPassword
        ? `sudo -S -p '' -u ${shellQuote(this.sudoUser || 'root')} sh -lc ${shellQuote(shellCommand)}`
        : `sudo -n -u ${shellQuote(this.sudoUser || 'root')} sh -lc ${shellQuote(shellCommand)}`
      : `sh -lc ${shellQuote(shellCommand)}`

    return new Promise<void>((resolve, reject) => {
      let settled = false
      let transferredBytes = resumeOffset
      let stderr = ''
      let channel: ClientChannel | undefined
      const localStream = createWriteStream(localPath, {
        flags: resumeOffset > 0 ? 'r+' : 'w',
        start: resumeOffset > 0 ? resumeOffset : undefined
      })

      const safeReject = (error: Error) => {
        if (settled) return
        settled = true
        clearTimeout(timeoutId)
        signal?.removeEventListener('abort', abortTransfer)
        localStream.destroy()
        channel?.destroy()
        reject(error)
      }
      const safeResolve = () => {
        if (settled) return
        settled = true
        clearTimeout(timeoutId)
        signal?.removeEventListener('abort', abortTransfer)
        resolve()
      }
      const abortTransfer = () => safeReject(this.transferAbortError())
      const timeoutId = setTimeout(
        () => {
          safeReject(new Error('文件下载超时'))
        },
        10 * 60 * 1000
      )
      signal?.addEventListener('abort', abortTransfer, { once: true })
      if (signal?.aborted) {
        abortTransfer()
        return
      }

      localStream.on('error', (error) => {
        safeReject(error instanceof Error ? error : new Error(String(error)))
      })

      execClient.exec(command, { pty: false }, (error, stream) => {
        if (error) {
          safeReject(error)
          return
        }
        channel = stream
        stream.stderr.on('data', (chunk: Buffer) => {
          stderr += chunk.toString('utf8')
        })
        stream.on('data', (chunk: Buffer) => {
          transferredBytes += chunk.byteLength
          onProgress(Math.min(expectedBytes, transferredBytes))
          if (!localStream.write(chunk)) {
            stream.pause()
          }
        })
        localStream.on('drain', () => stream.resume())
        stream.on('error', (streamError: Error) => {
          safeReject(streamError instanceof Error ? streamError : new Error(String(streamError)))
        })
        stream.on('close', (code?: number) => {
          if (code && code !== 0) {
            if (privileged && /incorrect password|authentication failure|sorry, try again/i.test(stderr)) {
              this.sudoPassword = undefined
              safeReject(new Error('sudo 密码错误，请重新输入。'))
              return
            }
            safeReject(new Error(stderr.trim() || `远端文件读取失败，退出码 ${code}`))
            return
          }
          localStream.end(() => {
            if (transferredBytes !== expectedBytes) {
              safeReject(
                new Error(
                  `传输校验失败：已下载 ${formatShellBytes(transferredBytes)}，期望 ${formatShellBytes(expectedBytes)}`
                )
              )
              return
            }
            safeResolve()
          })
        })

        if (privileged && this.sudoPassword) {
          stream.end(`${this.sudoPassword}\n`)
        } else {
          stream.end()
        }
      })
    })
  }

  private async readRemoteDirectoryViaShell(targetPath: string, cause: unknown): Promise<RemoteFileItem[]> {
    this.ensureShellFileFallback(cause)
    const outputStartMarker = '__FILETERM_DIR_LIST_START__'
    const outputEndMarker = '__FILETERM_DIR_LIST_END__'
    const output = await this.execShellFileCommand(
      `
target=${shellQuote(targetPath)}
if [ ! -d "$target" ]; then
  printf "%s\\n" ${shellQuote(outputStartMarker)}
  echo "__NOT_DIR__"
  printf "%s\\n" ${shellQuote(outputEndMarker)}
  exit 1
fi
printf "%s\\n" ${shellQuote(outputStartMarker)}
cd "$target" || exit 1
for name in .* *; do
  [ "$name" = "." ] && continue
  [ "$name" = ".." ] && continue
  [ ! -e "$name" ] && continue
  kind=file
  [ -d "$name" ] && kind=folder
  stat_line=$(stat -c "%Y|%s|%A|%u|%g" -- "$name" 2>/dev/null || echo "0|0|||")
  printf "%s\t%s\t%s\n" "$name" "$kind" "$stat_line"
done
printf "%s\\n" ${shellQuote(outputEndMarker)}
`,
      undefined,
      this.fileAccessMode === 'root'
    )
    const body = extractMarkedOutput(output, outputStartMarker, outputEndMarker).trim()
    if (body === '__NOT_DIR__') {
      throw new Error(`无法打开远程目录: ${targetPath}`)
    }

    const rows = body
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => {
        const [name, type, mtime, size, permission, uid, gid] = line.split('\t').join('|').split('|')
        const fullPath = joinRemotePath(targetPath, name)
        return {
          path: fullPath,
          name,
          type: type === 'folder' ? ('folder' as const) : ('file' as const),
          modified: formatShellTimestamp(Number(mtime) || 0),
          size: type === 'folder' ? '-' : formatShellBytes(Number(size) || 0),
          permission: permission || '',
          ownerGroup: uid || gid ? `${uid || ''}/${gid || ''}` : ''
        }
      })
      .sort((left, right) => {
        if (left.type !== right.type) {
          return left.type === 'folder' ? -1 : 1
        }
        return left.name.localeCompare(right.name)
      })

    if (targetPath !== '/') {
      rows.unshift({
        path: parentRemotePath(targetPath),
        name: '..',
        type: 'folder',
        modified: '',
        size: '-',
        permission: '',
        ownerGroup: ''
      })
    }

    return rows
  }

  private async createTemporaryRemoteUploadPath(fileName: string): Promise<string> {
    const safeName = fileName.replace(/[^a-zA-Z0-9._-]/g, '_') || 'upload.bin'
    const output = await this.execCommand(`sh -lc ${shellQuote(`mktemp /tmp/fileterm-upload.XXXXXX-${safeName}`)}`)
    const tempPath = output.trim()
    if (!tempPath) {
      throw new Error('无法创建远程临时上传文件')
    }
    return tempPath
  }

  private ensureShellFileFallback(cause: unknown) {
    const profile = this.profile as SshProfile
    if (profile.enableExecChannel === false) {
      throw cause instanceof Error ? cause : new Error('SFTP unavailable and exec fallback disabled')
    }
  }

  private async execShellFileCommand(
    command: string,
    options?: { allowNonZeroWithStdout?: boolean },
    privileged = false,
    stdinPayload?: string
  ): Promise<string> {
    if (!privileged) {
      return this.execCommand(`sh -lc ${shellQuote(command)}`, options, false, stdinPayload)
    }

    const sudoUser = this.sudoUser || 'root'
    if (this.sudoPassword) {
      const stdin = `${this.sudoPassword}\n${stdinPayload ?? ''}`
      return this.execCommand(
        `sudo -S -p '' -u ${shellQuote(sudoUser)} sh -lc ${shellQuote(command)}`,
        options,
        true,
        stdin
      )
    }

    return this.execCommand(
      `sudo -n -u ${shellQuote(sudoUser)} sh -lc ${shellQuote(command)}`,
      options,
      true,
      stdinPayload
    )
  }

  private async streamLocalFileToShellCommand(
    localPath: string,
    command: string,
    privileged: boolean,
    expectedBytes: number,
    onProgress?: (transferredBytes: number) => void,
    signal?: AbortSignal
  ): Promise<void> {
    const execClient = await this.ensureExecConnection()
    return new Promise<void>((resolve, reject) => {
      let settled = false
      let transferredBytes = 0
      let stdout = ''
      let stderr = ''
      let readEnded = false
      const readStream = createReadStream(localPath)
      let channel: ClientChannel | undefined

      const safeResolve = () => {
        if (!settled) {
          settled = true
          clearTimeout(timeoutId)
          signal?.removeEventListener('abort', abortTransfer)
          resolve()
        }
      }

      const safeReject = (error: Error) => {
        if (!settled) {
          settled = true
          clearTimeout(timeoutId)
          signal?.removeEventListener('abort', abortTransfer)
          try {
            readStream.destroy()
          } catch {
            // ignore
          }
          try {
            channel?.destroy()
          } catch {
            // ignore
          }
          reject(error)
        }
      }

      const abortTransfer = () => safeReject(this.transferAbortError())

      const timeoutId = setTimeout(
        () => {
          safeReject(new Error('文件上传超时'))
        },
        10 * 60 * 1000
      )
      signal?.addEventListener('abort', abortTransfer, { once: true })
      if (signal?.aborted) {
        abortTransfer()
        return
      }

      const handlePrivilegeError = (message: string): boolean => {
        if (!privileged) {
          return false
        }
        if (
          /incorrect password|authentication failure|3 incorrect password attempts|sorry, try again|no password was provided|密码错误|认证失败|对不起，请重试|未提供密码/i.test(
            message
          )
        ) {
          this.sudoPassword = undefined
          safeReject(new Error('sudo 密码错误，请重新输入。'))
          return true
        }
        if (
          /password is required|a password is required|no tty present|a terminal is required|sorry, you must have a tty|需要密码|必须输入密码/i.test(
            message
          )
        ) {
          safeReject(new Error('未检测到可复用的 sudo 授权，需要提供 sudo 密码。'))
          return true
        }
        return false
      }

      const handleExec = (error: Error | undefined, stream: ClientChannel) => {
        if (error) {
          safeReject(error)
          return
        }

        channel = stream
        stream.on('data', (chunk: Buffer) => {
          stdout += chunk.toString('utf8')
          void handlePrivilegeError(stdout)
        })
        stream.stderr.on('data', (chunk: Buffer) => {
          stderr += chunk.toString('utf8')
          void handlePrivilegeError(stderr)
        })

        readStream.on('data', (chunk: Buffer) => {
          transferredBytes += chunk.byteLength
          onProgress?.(transferredBytes)
          try {
            if (!stream.write(chunk)) {
              readStream.pause()
            }
          } catch (writeError) {
            safeReject(writeError instanceof Error ? writeError : new Error(String(writeError)))
          }
        })
        stream.on('drain', () => {
          readStream.resume()
        })
        readStream.on('error', (readError) => {
          safeReject(readError instanceof Error ? readError : new Error(String(readError)))
        })
        readStream.on('end', () => {
          readEnded = true
          stream.end()
        })
        stream.on('error', (streamError: Error) => {
          safeReject(streamError instanceof Error ? streamError : new Error(String(streamError)))
        })
        stream.on('close', (code?: number) => {
          if (code && code !== 0) {
            const errMessage = stderr.trim() || stdout.trim()
            if (handlePrivilegeError(errMessage)) {
              return
            }
            safeReject(new Error(errMessage || `Command exited with code ${code}`))
            return
          }
          if (!readEnded || transferredBytes < expectedBytes) {
            safeReject(
              new Error(
                `远程写入通道提前关闭，仅发送 ${formatShellBytes(transferredBytes)} / ${formatShellBytes(expectedBytes)}`
              )
            )
            return
          }
          safeResolve()
        })

        if (privileged && this.sudoPassword) {
          stream.write(`${this.sudoPassword}\n`)
        }
      }

      try {
        const execCommand = privileged
          ? this.sudoPassword
            ? `sudo -S -p '' -u ${shellQuote(this.sudoUser || 'root')} sh -lc ${shellQuote(command)}`
            : `sudo -n -u ${shellQuote(this.sudoUser || 'root')} sh -lc ${shellQuote(command)}`
          : `sh -lc ${shellQuote(command)}`

        if (privileged && this.sudoPassword) {
          execClient.exec(execCommand, { pty: true }, handleExec)
          return
        }

        execClient.exec(execCommand, handleExec)
      } catch (execError) {
        safeReject(execError instanceof Error ? execError : new Error(String(execError)))
      }
    })
  }

  private async verifyRootFileAccess(): Promise<void> {
    await this.execShellFileCommand('true', undefined, true)
  }

  private withOperationTimeout<T>(
    operation: Promise<T>,
    timeoutMs: number,
    message: string,
    onTimeout?: () => void
  ): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      let settled = false

      const settle = (handler: (value: T) => void | ((error: Error) => void), value: T | Error) => {
        if (settled) {
          return
        }
        settled = true
        clearTimeout(timeoutId)
        if (value instanceof Error) {
          ;(handler as (error: Error) => void)(value)
          return
        }
        ;(handler as (result: T) => void)(value)
      }

      const timeoutId = setTimeout(() => {
        try {
          onTimeout?.()
        } catch {
          // Best-effort reset only.
        }
        settle(reject, new Error(message))
      }, timeoutMs)

      operation.then(
        (value) => settle(resolve, value),
        (error) => settle(reject, error instanceof Error ? error : new Error(String(error)))
      )
    })
  }

  private trackSudoPromptFromTerminal(text: string) {
    const newWindow = (this.sudoPromptWindow + text).slice(-200)
    this.sudoPromptWindow = newWindow
    const normalized = newWindow.replace(/\u001b\[[0-9;?]*[ -/]*[@-~]/g, '')

    // Only test the newly appended part + a little overlap, OR just check if the pattern appears
    // at the very end of the string to avoid triggering continuously.
    // For password prompt, we want it to match when it appears at the end of the current buffer.
    if (!this.awaitingSudoPasswordInput && /(\[sudo\]|password|密码|passphrase)[^\r\n]*[:：]\s*$/i.test(normalized)) {
      this.awaitingSudoPasswordInput = true
      this.pendingSudoPasswordInput = ''

      // Attempt to recover blind-typed password from recent keystrokes
      const recentKeys = this.recentKeystrokes
      // recentKeys might look like "sudo -i\rmypassword\r" or "sudo -i\rmypassword"
      // If it contains a \r after the command, the text between the last \r and the second to last \r might be the password.
      const parts = recentKeys.split(/[\r\n]/)
      if (parts.length >= 2) {
        // If the user already hit Enter for the password, it's the second to last part.
        // If they haven't hit Enter yet, it's the last part.
        // Let's just prepopulate pendingSudoPasswordInput with the last part.
        const lastPart = parts[parts.length - 1]
        const secondToLast = parts[parts.length - 2]

        if (lastPart === '') {
          // They already hit Enter! The password is the second to last part.
          if (secondToLast && !secondToLast.includes('sudo ')) {
            this.sudoPassword = secondToLast
            this.awaitingSudoPasswordInput = false
            this.onStateChange(this.getSummary(), this.transcript.toString(), this.connected)
          }
        } else {
          // They are currently typing the password (or finished but haven't hit enter)
          if (!lastPart.includes('sudo ')) {
            this.pendingSudoPasswordInput = lastPart
          }
        }
      }
    }

    // For failure messages, check if it's freshly added by ensuring it appears at the very end.
    if (
      /(incorrect password|authentication failure|sorry, try again|密码错误|认证失败|对不起，请重试)\s*$/i.test(
        normalized
      )
    ) {
      this.sudoPassword = undefined
    }
  }

  private captureSudoPasswordInput(data: string) {
    // Keep a buffer of recent keystrokes to support blind typing recovery
    let recentKeys = this.recentKeystrokes
    recentKeys += data
    if (recentKeys.length > 200) {
      recentKeys = recentKeys.slice(-200)
    }
    this.recentKeystrokes = recentKeys

    if (!this.awaitingSudoPasswordInput) {
      return
    }

    for (const char of data) {
      if (char === '\u0003' || char === '\u001b') {
        this.awaitingSudoPasswordInput = false
        this.pendingSudoPasswordInput = ''
        this.sudoPassword = undefined
        return
      }

      if (char === '\r' || char === '\n') {
        if (this.pendingSudoPasswordInput) {
          this.sudoPassword = this.pendingSudoPasswordInput
          this.onStateChange(this.getSummary(), this.transcript.toString(), this.connected)
        }
        this.awaitingSudoPasswordInput = false
        this.pendingSudoPasswordInput = ''
        return
      }

      if (char === '\u007f' || char === '\b') {
        this.pendingSudoPasswordInput = this.pendingSudoPasswordInput.slice(0, -1)
        continue
      }

      if (char >= ' ') {
        this.pendingSudoPasswordInput += char
      }
    }
  }

  private async execCommand(
    command: string,
    options?: SystemMetricsCommandOptions,
    privileged = false,
    stdinPayload?: string
  ): Promise<string> {
    const execClient = await this.ensureExecConnection()
    return new Promise<string>((resolve, reject) => {
      let settled = false
      let streamInstance: ClientChannel | undefined

      const safeResolve = (val: string) => {
        if (!settled) {
          settled = true
          clearTimeout(timeoutId)
          resolve(val)
        }
      }

      const safeReject = (err: Error) => {
        if (!settled) {
          settled = true
          clearTimeout(timeoutId)
          reject(err)
        }
      }

      const timeoutId = setTimeout(
        () => {
          if (!settled) {
            if (streamInstance) {
              try {
                streamInstance.destroy()
              } catch {
                // ignore
              }
            }
            const timeoutError = new Error('命令执行超时')
            timeoutError.name = 'TimeoutError'
            safeReject(timeoutError)
          }
        },
        Math.max(250, options?.timeoutMs ?? 15000)
      )

      const handleExec = (error: Error | undefined, stream: ClientChannel) => {
        if (error) {
          safeReject(error)
          return
        }

        streamInstance = stream
        stream.on('error', (streamError: Error) => {
          safeReject(streamError)
        })

        let stdout = ''
        let stderr = ''

        stream.on('data', (chunk: Buffer) => {
          const chunkStr = chunk.toString('utf8')
          stdout += chunkStr

          if (privileged && stdinPayload !== undefined) {
            if (
              /incorrect password|authentication failure|3 incorrect password attempts|sorry, try again|no password was provided|密码错误|认证失败|对不起，请重试|未提供密码/i.test(
                stdout
              )
            ) {
              this.sudoPassword = undefined
              safeReject(new Error('sudo 密码错误，请重新输入。'))
              try {
                stream.destroy()
              } catch {
                // ignore
              }
              return
            }
            if (
              /password is required|a password is required|no tty present|a terminal is required|sorry, you must have a tty|需要密码|必须输入密码/i.test(
                stdout
              )
            ) {
              safeReject(new Error('未检测到可复用的 sudo 授权，需要提供 sudo 密码。'))
              try {
                stream.destroy()
              } catch {
                // ignore
              }
              return
            }
          }
        })
        stream.stderr.on('data', (chunk: Buffer) => {
          stderr += chunk.toString('utf8')
        })
        if (stdinPayload !== undefined) {
          stream.write(stdinPayload)
          stream.end()
        }
        stream.on('close', (code?: number) => {
          if (options?.allowNonZeroWithStdout && stdout.trim()) {
            safeResolve(stdout)
            return
          }

          const errMessage = stderr.trim() || (privileged && stdinPayload !== undefined ? stdout.trim() : '')

          if (code && code !== 0) {
            if (
              privileged &&
              /password is required|a password is required|no tty present|a terminal is required|sorry, you must have a tty|需要密码|必须输入密码/i.test(
                errMessage
              )
            ) {
              safeReject(new Error('未检测到可复用的 sudo 授权，需要提供 sudo 密码。'))
              return
            }
            if (
              privileged &&
              /incorrect password|authentication failure|3 incorrect password attempts|sorry, try again|no password was provided|密码错误|认证失败|对不起，请重试|未提供密码/i.test(
                errMessage
              )
            ) {
              this.sudoPassword = undefined
              safeReject(new Error('sudo 密码错误，请重新输入。'))
              return
            }
            if (errMessage) {
              safeReject(new Error(errMessage))
              return
            }
            safeReject(new Error(`Command exited with code ${code}`))
            return
          }
          safeResolve(stdout)
        })
      }

      try {
        if (privileged && stdinPayload !== undefined) {
          execClient.exec(command, { pty: true }, handleExec)
          return
        }

        execClient.exec(command, handleExec)
      } catch (err) {
        safeReject(err instanceof Error ? err : new Error(String(err)))
      }
    })
  }

  private appendSystemMessage(message: string) {
    this.transcript.append(message)
    this.onData(message)
    this.onStateChange(this.getSummary(), this.transcript.toString(), this.connected)
  }

  private resetPrivilegedFileAccess() {
    this.fileAccessMode = 'user'
    this.sudoPassword = undefined
    this.sudoPromptWindow = ''
    this.awaitingSudoPasswordInput = false
    this.pendingSudoPasswordInput = ''
    this.recentKeystrokes = ''
    this.lastInjectTime = 0
  }
}

class BoundedTextBuffer {
  private static readonly CHUNK_SIZE = 4096
  private chunks: string[] = []
  private head = 0
  private length = 0

  constructor(
    private readonly limit: number,
    initialValue = ''
  ) {
    this.append(initialValue)
  }

  append(value: string) {
    if (!value) {
      return
    }

    if (value.length >= this.limit) {
      this.chunks = []
      this.head = 0
      this.length = 0
      value = value.slice(value.length - this.limit)
    }

    for (let index = 0; index < value.length; index += BoundedTextBuffer.CHUNK_SIZE) {
      const chunk = value.slice(index, index + BoundedTextBuffer.CHUNK_SIZE)
      this.chunks.push(chunk)
      this.length += chunk.length
    }

    this.trimStart()
  }

  toString() {
    if (!this.length) {
      return ''
    }
    return this.chunks.slice(this.head).join('')
  }

  private trimStart() {
    let excess = this.length - this.limit
    while (excess > 0 && this.head < this.chunks.length) {
      const first = this.chunks[this.head]!
      if (first.length <= excess) {
        excess -= first.length
        this.length -= first.length
        this.head += 1
        continue
      }

      this.chunks[this.head] = first.slice(excess)
      this.length -= excess
      excess = 0
    }

    if (this.head > 256 && this.head * 2 > this.chunks.length) {
      this.chunks = this.chunks.slice(this.head)
      this.head = 0
    }
  }
}

const DEFAULT_SSH_KEY_FILES = ['id_ed25519', 'id_ecdsa', 'id_rsa', 'id_dsa']

async function resolveSshAuthConfig(
  profile: SshProfile
): Promise<Pick<ConnectConfig, 'password' | 'privateKey' | 'passphrase' | 'agent'>> {
  if (profile.authType === 'password') {
    if (profile.password) {
      return {
        password: profile.password
      }
    }

    return resolveSystemSshAuthConfig(profile)
  }

  if (profile.authType === 'keyboard-interactive') {
    // Never fall through to an agent/default key in this explicit mode, but
    // retain a saved first-factor password when the server accepts it.
    return profile.password ? { password: profile.password } : {}
  }

  if (profile.authType === 'privateKey') {
    const privateKeyPath = expandHomePath(profile.privateKeyPath)
    if (!privateKeyPath) {
      throw new Error('Missing private key path')
    }

    return {
      privateKey: await readFile(privateKeyPath, 'utf8'),
      passphrase: profile.passphrase
    }
  }

  return resolveSystemSshAuthConfig(profile)
}

async function resolveSystemSshAuthConfig(
  profile: SshProfile
): Promise<Pick<ConnectConfig, 'privateKey' | 'passphrase' | 'agent'>> {
  const agent = process.env.SSH_AUTH_SOCK
  const privateKey = await readDefaultPrivateKey()

  if (!agent && !privateKey) {
    throw new Error('No SSH agent or default private key found on this computer')
  }

  return {
    agent,
    privateKey,
    passphrase: privateKey ? profile.passphrase : undefined
  }
}

async function hasSystemSshAuthAvailable(): Promise<boolean> {
  const agent = process.env.SSH_AUTH_SOCK
  if (agent) {
    return true
  }

  const privateKey = await readDefaultPrivateKey()
  return Boolean(privateKey)
}

function expandHomePath(targetPath?: string): string | undefined {
  if (!targetPath) {
    return undefined
  }

  if (targetPath === '~') {
    return os.homedir()
  }

  if (targetPath.startsWith('~/')) {
    return path.join(os.homedir(), targetPath.slice(2))
  }

  return targetPath
}

async function readDefaultPrivateKey(): Promise<string | undefined> {
  const homeDirectory = os.homedir()

  for (const fileName of DEFAULT_SSH_KEY_FILES) {
    const candidate = path.join(homeDirectory, '.ssh', fileName)
    try {
      const candidateStats = await stat(candidate)
      if (!candidateStats.isFile()) {
        continue
      }
      return await readFile(candidate, 'utf8')
    } catch {
      continue
    }
  }

  return undefined
}

function computeHostFingerprint(key: Buffer | string) {
  const payload = Buffer.isBuffer(key) ? key : Buffer.from(key)
  return `SHA256:${createHash('sha256').update(payload).digest('base64').replace(/=+$/g, '')}`
}

function sftpCall(operation: (done: (error?: Error | null) => void) => void): Promise<void> {
  return new Promise((resolve, reject) => {
    operation((error) => {
      if (error) {
        reject(error)
        return
      }
      resolve()
    })
  })
}

function sftpLstat(sftp: SFTPWrapper, targetPath: string): Promise<Stats | null> {
  return new Promise((resolve, reject) => {
    sftp.lstat(targetPath, (error, attrs) => {
      if (error) {
        if (isSftpMissingError(error)) {
          resolve(null)
          return
        }
        reject(error)
        return
      }
      resolve(attrs)
    })
  })
}

function isSftpMissingError(error: unknown) {
  return Boolean(
    error &&
    typeof error === 'object' &&
    (('code' in error &&
      ((error as { code?: number | string }).code === 2 || (error as { code?: number | string }).code === 'ENOENT')) ||
      ('message' in error && /no such file|not found/i.test(String((error as { message?: string }).message))))
  )
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

function parseRemoteByteSize(output: string): number | undefined {
  const match = output.match(/\b\d+\b/)
  if (!match) {
    return undefined
  }

  const value = Number.parseInt(match[0], 10)
  return Number.isFinite(value) ? value : undefined
}

function shellQuote(value: string) {
  return `'${value.replace(/'/g, `'"'"'`)}'`
}

function extractMarkedOutput(output: string, startMarker: string, endMarker: string) {
  const startIndex = output.indexOf(startMarker)
  const endIndex = output.lastIndexOf(endMarker)
  if (startIndex === -1 || endIndex === -1 || endIndex < startIndex) {
    return output
  }

  return output.slice(startIndex + startMarker.length, endIndex)
}

function validateMode(mode: string) {
  if (!/^[0-7]{3,4}$/.test(mode.trim())) {
    throw new Error('权限值必须是 3 到 4 位八进制数字，例如 755')
  }
}

function joinRemotePath(basePath: string, name: string) {
  if (basePath === '/') {
    return `/${name}`
  }
  return `${basePath.replace(/\/$/, '')}/${name}`
}

function formatShellTimestamp(timestamp: number) {
  if (!timestamp) {
    return ''
  }

  const date = new Date(timestamp * 1000)
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`
}

function formatShellBytes(size: number) {
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
