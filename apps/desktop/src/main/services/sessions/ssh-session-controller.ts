import { createHash } from 'node:crypto'
import { createReadStream } from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { readFile, stat, writeFile } from 'node:fs/promises'
import { pipeline } from 'node:stream/promises'
import type { ClientChannel, ConnectConfig, FileEntry, SFTPWrapper } from 'ssh2'
import { Client } from 'ssh2'
import type {
  FileSessionController,
  PermissionChangeOptions,
  RemoteFileAccessOptions,
  RemoteFileItem,
  SshInteractionDraft,
  SshInteractionResponse,
  SystemMetrics,
  SshProfile,
  SshSessionController,
  TransferProgress
} from '@fileterm/core'
import { BaseFileSessionController } from './base-file-session-controller.js'
import { buildMetricsCommand, parentRemotePath, parseSystemMetrics, toRemoteFileItem } from './session-file-utils.js'
import {
  findSetupEchoEnd,
  SETUP_NEEDLE,
  SHELL_CWD_SETUP,
  ShellCwdTracker
} from './shell-cwd-integration.js'
import { createSshDebugLogger, isSshDebugEnabled, singleLine } from './ssh-debug-logger.js'
import { decodeBuffer, encodeText } from '../text-encoding.js'
import { appLog, appWarn } from '../app-logger.js'

export class LiveSshSessionController extends BaseFileSessionController implements SshSessionController {
  readonly type = 'ssh'
  private static readonly TRANSCRIPT_LIMIT = 200_000
  private static readonly REMOTE_FILE_READ_TIMEOUT_MS = 20_000
  private static readonly REMOTE_FILE_WRITE_TIMEOUT_MS = 20_000

  private readonly ssh = new Client()
  private readonly execSsh = new Client()
  private readonly sftpSsh = new Client()
  private readonly transferSsh = new Client()
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
  private suppressEcho = false
  private echoBuf = ''
  private shellSetupReleaseTimer?: ReturnType<typeof setTimeout>
  private fileAccessMode: 'user' | 'root' = 'user'
  private shellUser?: string
  private sudoUser = 'root'
  private sudoPassword?: string
  private awaitingSudoPasswordInput = false
  private pendingSudoPasswordInput = ''
  private metrics?: SystemMetrics
  private readonly acceptedHostFingerprints = new Set<string>()

  constructor(
    id: string,
    profile: SshProfile,
    private readonly requestInteraction: (request: SshInteractionDraft) => Promise<SshInteractionResponse>,
    private readonly rememberTrustedHostFingerprint: (fingerprint: string) => Promise<void>,
    private readonly onData: (chunk: string) => void,
    private readonly onShellCwdChange: (cwd: string) => void,
    private readonly onShellUserChange: (user: string) => void,
    private readonly onStateChange: (summary: string, transcript: string, connected: boolean) => void,
    initialTranscript?: string
  ) {
    super(id, 'ssh', profile)
    this.currentRemotePath = profile.remotePath || '.'
    this.transcript = new BoundedTextBuffer(LiveSshSessionController.TRANSCRIPT_LIMIT, initialTranscript)
    this.appendSystemMessage('连接主机...\r\n')
  }

  override async connect(): Promise<void> {
    const profile = await this.resolveConnectionProfile(this.profile as SshProfile)
    const authConfig = await resolveSshAuthConfig(profile)
    const shouldTryKeyboard = profile.authType === 'password' && Boolean(profile.password)
    const username = profile.username || os.userInfo().username
    const sshConfig: ConnectConfig = {
      host: profile.host,
      port: profile.port,
      username,
      ...authConfig,
      readyTimeout: 15000,
      keepaliveInterval: 3000,
      keepaliveCountMax: 2,
      tryKeyboard: shouldTryKeyboard,
      hostVerifier: (key: Buffer | string, verify: (accepted: boolean) => void) => {
        void this.verifyHostFingerprint(profile, key)
          .then(verify)
          .catch((error) => {
            this.sshDebug.log('main', `主机密钥校验失败: ${error instanceof Error ? error.message : String(error)}`)
            verify(false)
          })
      },
      ...(this.sshDebug.enabled
        ? { debug: (message: string) => this.sshDebug.handle('main', message) }
        : {})
    }
    this.sshConfig = sshConfig
    this.sshDebug.logConnectionStart(
      'main',
      profile,
      username,
      authConfig,
      shouldTryKeyboard
    )

    await new Promise<void>((resolve, reject) => {
      let settled = false
      let connectionFailed = false

      this.ssh.removeAllListeners('banner')
      this.ssh.removeAllListeners('keyboard-interactive')
      this.ssh.on('banner', (message) => {
        this.sshDebug.log('main', `服务端横幅: ${singleLine(message)}`)
      })
      registerKeyboardInteractiveHandler(this.ssh, profile, (message) => {
        this.sshDebug.logKeyboardInteractive('main', message)
      })

      this.ssh
        .on('ready', () => {
          this.connected = true
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
                let text = chunk.toString('utf8')

                if (!this.cwdSetupInjected && text.trim().length > 0) {
                  this.cwdSetupInjected = true
                  this.injectShellSetup(stream)
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
                    
                    const beforeCmd = this.echoBuf.slice(0, echoEnd.lineStart)
                    const afterOsc7 = this.echoBuf.slice(echoEnd.payloadEnd)
                    text = beforeCmd + afterOsc7
                    this.clearShellSetupSuppression()
                  } else if (this.echoBuf.length >= 16384) {
                    text = this.echoBuf
                    this.clearShellSetupSuppression()
                  } else {
                    return
                  }
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
                const lastInjectTime = (this as any)._lastInjectTime || 0
                if (strippedText.endsWith('#') && !this.suppressEcho) {
                  // Only inject if we think we aren't root AND it's been a while since the last injection 
                  // to prevent infinite loops (because the injected script itself triggers a new prompt)
                  if (this.shellUser !== 'root' && (now - lastInjectTime > 2000)) {
                    ;(this as any)._lastInjectTime = now
                    this.injectShellSetup(stream)
                  }
                }

                this.onData(text)
              })
              stream.on('close', () => {
                this.handlePrimaryDisconnect()
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

  private handleShellUserChange(user: string) {
    if (this.shellUser === user) return
    this.shellUser = user
    this.onShellUserChange(user)
  }

  private injectShellSetup(stream: { write(data: string): void }) {
    this.clearShellSetupSuppression()
    this.suppressEcho = true
    stream.write(` ${SHELL_CWD_SETUP}\r`)
    this.shellSetupReleaseTimer = setTimeout(() => {
      if (!this.suppressEcho) {
        return
      }

      const bufferedText = this.echoBuf
      this.clearShellSetupSuppression()
      if (!bufferedText) {
        return
      }

      this.transcript.append(bufferedText)
      for (const update of this.shellCwdTracker.feed(bufferedText)) {
        if (update.cwd && update.cwd !== this.shellCwd) {
          this.shellCwd = update.cwd
          this.onShellCwdChange(update.cwd)
        }
        if (update.user && update.user !== this.shellUser) {
          this.handleShellUserChange(update.user)
        }
      }
      this.trackSudoPromptFromTerminal(bufferedText)
      this.onData(bufferedText)
    }, 1500)
  }

  private clearShellSetupSuppression() {
    if (this.shellSetupReleaseTimer) {
      clearTimeout(this.shellSetupReleaseTimer)
      this.shellSetupReleaseTimer = undefined
    }
    this.suppressEcho = false
    this.echoBuf = ''
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
      await this.rememberTrustedHostFingerprint(fingerprint)
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
    this.clearShellSetupSuppression()
    this.shellStream?.end()
    this.closeExecSession()
    this.closeSftpSession()
    this.closeTransferSftpSession()
    this.ssh.end()
    this.sftpSsh.end()
    this.transferSsh.end()
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

    if (options?.sudoUser?.trim()) {
      this.sudoUser = options.sudoUser.trim()
    }
    if (options && 'sudoPassword' in options) {
      this.sudoPassword = options.sudoPassword || undefined
    }

    if (mode === this.fileAccessMode) {
      return
    }

    if (mode === 'root' && this.connected) {
      await this.verifyRootFileAccess()
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
    this.closeTransferSftpSession()
  }

  async write(data: string): Promise<void> {
    this.captureSudoPasswordInput(data)
    this.shellStream?.write(data)
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
        return await this.readRemoteDirectoryViaShell(this.currentRemotePath, new Error('Root file access mode enabled'))
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
      await this.execCommand(`sh -lc 'mkdir -p ${shellQuote(path.posix.dirname(nextPath))} && mv ${shellQuote(targetPath)} ${shellQuote(nextPath)}'`, { allowNonZeroWithStdout: true })
      if (error && !this.sftpUnavailableReason) {
        throw error
      }
    }
  }

  async deleteRemotePath(targetPath: string, targetType: RemoteFileItem['type']): Promise<void> {
    if (this.fileAccessMode === 'root') {
      const command = targetType === 'folder'
        ? `rm -rf -- ${shellQuote(targetPath)}`
        : `rm -f -- ${shellQuote(targetPath)}`
      await this.execShellFileCommand(command, { allowNonZeroWithStdout: true }, true)
      return
    }

    const command = targetType === 'folder'
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
        const baseCommand = applyTo === 'all'
          ? `chmod -R ${shellQuote(mode)} ${shellQuote(targetPath)}`
          : applyTo === 'files'
            ? `chmod ${shellQuote(mode)} ${shellQuote(targetPath)} && find ${shellQuote(targetPath)} -type f -exec chmod ${shellQuote(mode)} {} +`
            : `chmod ${shellQuote(mode)} ${shellQuote(targetPath)} && find ${shellQuote(targetPath)} -type d -exec chmod ${shellQuote(mode)} {} +`

        await this.execShellFileCommand(baseCommand, { allowNonZeroWithStdout: true }, true)
        return
      }

      await this.execShellFileCommand(`chmod ${shellQuote(mode)} ${shellQuote(targetPath)}`, { allowNonZeroWithStdout: true }, true)
      return
    }

    if (recursive) {
      const baseCommand = applyTo === 'all'
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
      await this.execCommand(`sh -lc 'chmod ${shellQuote(mode)} ${shellQuote(targetPath)}'`, { allowNonZeroWithStdout: true })
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
      const sftp = sftpOverride ?? await this.ensureSftp()
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

  async uploadFile(localPath: string, remotePath: string, onProgress: (progress: TransferProgress) => void): Promise<void> {
    if (this.fileAccessMode === 'root') {
      await this.uploadFileAsPrivileged(localPath, remotePath, onProgress, new Error('Root file access mode enabled'))
      return
    }

    await this.uploadFileAsUser(localPath, remotePath, onProgress)
  }

  private async uploadFileAsUser(localPath: string, remotePath: string, onProgress: (progress: TransferProgress) => void): Promise<void> {
    let transferredBytes = 0
    try {
      const sftp = await this.ensureTransferSftp()
      const info = await stat(localPath)
      const total = Math.max(info.size, 1)
      appLog(`[FileTerm][SFTP] Upload start ${localPath} -> ${remotePath} (${formatShellBytes(total)})`)
      await this.ensureRemoteDirectory(path.posix.dirname(remotePath), sftp)
      const localStream = createReadStream(localPath)
      const remoteStream = sftp.createWriteStream(remotePath, {
        flags: 'w',
        mode: 0o644
      })
      localStream.on('data', (chunk) => {
        transferredBytes = Math.min(total, transferredBytes + chunk.length)
        onProgress({
          percent: Math.min(99, Math.round((transferredBytes / total) * 100)),
          transferredBytes,
          totalBytes: total
        })
      })
      await pipeline(localStream, remoteStream)
      await this.verifySftpRemoteUploadSize(sftp, remotePath, total)
      appLog(`[FileTerm][SFTP] Upload verified ${remotePath} (${formatShellBytes(total)})`)
      onProgress({ percent: 100, transferredBytes: total, totalBytes: total })
    } catch (error) {
      if (transferredBytes > 0) {
        appWarn(`[FileTerm][SFTP] Upload interrupted after ${formatShellBytes(transferredBytes)}: ${localPath} -> ${remotePath}`, error)
        throw new Error(`SFTP 上传已中断，已停止以避免提交不完整文件：${errorMessage(error)}`)
      }
      appWarn(`[FileTerm][SFTP] Upload could not start, falling back to shell stream: ${localPath} -> ${remotePath}`, error)
      await this.uploadFileViaShell(localPath, remotePath, onProgress, error, false)
    }
  }

  async downloadFile(remotePath: string, localPath: string, onProgress: (progress: TransferProgress) => void): Promise<void> {
    if (this.fileAccessMode === 'root') {
      await this.downloadFileViaShell(remotePath, localPath, onProgress, new Error('Root file access mode enabled'))
      return
    }

    let transferredBytes = 0
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
      const total = Math.max(attrs.size ?? 1, 1)
      appLog(`[FileTerm][SFTP] Download start ${remotePath} -> ${localPath} (${formatShellBytes(total)})`)
      await new Promise<void>((resolve, reject) => {
        sftp.fastGet(remotePath, localPath, {
          step: (transferred, _chunk, fileSize) => {
            transferredBytes = Math.max(transferredBytes, transferred)
            onProgress({
              percent: Math.min(99, Math.round((transferred / total) * 100)),
              transferredBytes: transferred,
              totalBytes: Math.max(fileSize || total, 1)
            })
          }
        }, (error) => {
          if (error) {
            reject(error)
            return
          }
          resolve()
        })
      })
      const localInfo = await stat(localPath)
      this.assertRemoteUploadSize(localPath, Math.max(localInfo.size, 1), total)
      appLog(`[FileTerm][SFTP] Download verified ${remotePath} -> ${localPath} (${formatShellBytes(total)})`)
      onProgress({ percent: 100, transferredBytes: total, totalBytes: total })
    } catch (error) {
      if (transferredBytes > 0) {
        appWarn(`[FileTerm][SFTP] Download interrupted after ${formatShellBytes(transferredBytes)}: ${remotePath} -> ${localPath}`, error)
        throw new Error(`SFTP 下载已中断，已停止以避免提交不完整文件：${errorMessage(error)}`)
      }
      appWarn(`[FileTerm][SFTP] Download could not start, falling back to shell stream: ${remotePath} -> ${localPath}`, error)
      await this.downloadFileViaShell(remotePath, localPath, onProgress, error)
    }
  }

  async refreshSystemMetrics(): Promise<SystemMetrics | undefined> {
    const profile = this.profile as SshProfile
    if (profile.enableExecChannel === false) {
      return this.metrics
    }

    try {
      const raw = await this.execCommand('sh', { allowNonZeroWithStdout: true }, false, `${buildMetricsCommand()}\n`)
      this.metrics = parseSystemMetrics(raw)
      return this.metrics
    } catch (error) {
      this.sshDebug.log('exec', `系统信息采集失败: ${error instanceof Error ? error.message : String(error)}`)
      return this.metrics
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
    this.registerSftpLifecycle()
    await new Promise<void>((resolve, reject) => {
      let settled = false
      this.sshDebug.logSftpStart(sshConfig.username)
      this.sftpSsh.removeAllListeners('keyboard-interactive')
      this.sftpSsh.removeAllListeners('banner')
      this.sftpSsh.on('banner', (message) => {
        this.sshDebug.log('sftp', `服务端横幅: ${singleLine(message)}`)
      })
      registerKeyboardInteractiveHandler(this.sftpSsh, this.profile as SshProfile, (message) => {
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
          ...sshConfig,
          keepaliveInterval: 3000,
          keepaliveCountMax: 4,
          readyTimeout: Math.max(sshConfig.readyTimeout ?? 15000, 20000),
          ...(this.sshDebug.enabled
            ? { debug: (message: string) => this.sshDebug.handle('sftp', message) }
            : {})
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

    const sshConfig = this.sshConfig
    this.registerTransferLifecycle()
    await new Promise<void>((resolve, reject) => {
      let settled = false
      this.sshDebug.log('transfer-sftp', `准备建立传输 SFTP 连接: ${sshConfig.username}@${sshConfig.host}:${sshConfig.port}`)
      this.transferSsh.removeAllListeners('keyboard-interactive')
      this.transferSsh.removeAllListeners('banner')
      this.transferSsh.on('banner', (message) => {
        this.sshDebug.log('transfer-sftp', `服务端横幅: ${singleLine(message)}`)
      })
      registerKeyboardInteractiveHandler(this.transferSsh, this.profile as SshProfile, (message) => {
        this.sshDebug.logKeyboardInteractive('transfer-sftp', message)
      })
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
          ...sshConfig,
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

  private handlePrimaryDisconnect() {
    this.clearShellSetupSuppression()
    this.resetPrivilegedFileAccess()
    this.connected = false
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
      this.transferSftp?.end?.()
      this.transferSftp = undefined
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
    this.transferSftp?.end?.()
    this.transferSftp = undefined
    this.transferSsh.end()
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
    this.registerExecLifecycle()
    this.execConnectPromise = new Promise<Client>((resolve, reject) => {
      let settled = false
      this.sshDebug.log('exec', `准备建立 Exec 连接: ${sshConfig.username}@${sshConfig.host}:${sshConfig.port}`)
      this.execSsh.removeAllListeners('keyboard-interactive')
      this.execSsh.removeAllListeners('banner')
      this.execSsh.on('banner', (message) => {
        this.sshDebug.log('exec', `服务端横幅: ${singleLine(message)}`)
      })
      registerKeyboardInteractiveHandler(this.execSsh, this.profile as SshProfile, (message) => {
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
          ...sshConfig,
          ...(this.sshDebug.enabled
            ? { debug: (message: string) => this.sshDebug.handle('exec', message) }
            : {})
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
    const output = await this.execShellFileCommand(`base64 ${shellQuote(targetPath)}`, undefined, this.fileAccessMode === 'root')
    return decodeBuffer(Buffer.from(output.replace(/\s+/g, ''), 'base64'), encoding)
  }

  private async writeRemoteFileViaShell(targetPath: string, content: string, cause: unknown, encoding = 'utf-8'): Promise<void> {
    this.ensureShellFileFallback(cause)
    const payload = encodeText(content, encoding).toString('base64')
    await this.execShellFileCommand(`base64 -d > ${shellQuote(targetPath)}`, undefined, this.fileAccessMode === 'root', `${payload}\n`)
  }

  private async writeRemoteFileAsPrivileged(targetPath: string, content: string, cause: unknown, encoding = 'utf-8'): Promise<void> {
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
      try {
        await this.execCommand(`sh -lc ${shellQuote(`rm -f -- ${tempRemotePath}`)}`, { allowNonZeroWithStdout: true })
      } catch {
        // Best-effort cleanup for temp editor save artifacts.
      }
      throw error
    }
  }

  private async uploadFileViaShell(
    localPath: string,
    remotePath: string,
    onProgress: (progress: TransferProgress) => void,
    cause: unknown,
    privileged = this.fileAccessMode === 'root'
  ): Promise<void> {
    this.ensureShellFileFallback(cause)
    const fileInfo = await stat(localPath)
    const total = Math.max(fileInfo.size, 1)
    appLog(`[FileTerm][SSH] Shell upload start ${localPath} -> ${remotePath} (${formatShellBytes(total)}, privileged=${privileged ? 'yes' : 'no'})`)
    onProgress({ percent: 1, transferredBytes: 0, totalBytes: total })
    await this.streamLocalFileToShellCommand(
      localPath,
      `cat > ${shellQuote(remotePath)}`,
      privileged,
      total,
      (transferredBytes) => {
        onProgress({
          percent: Math.min(99, Math.max(1, Math.round((transferredBytes / total) * 100))),
          transferredBytes,
          totalBytes: total
        })
      }
    )
    await this.verifyShellRemoteUploadSize(remotePath, total, privileged)
    appLog(`[FileTerm][SSH] Shell upload verified ${remotePath} (${formatShellBytes(total)}, privileged=${privileged ? 'yes' : 'no'})`)
    onProgress({ percent: 100, transferredBytes: total, totalBytes: total })
  }

  private async uploadFileAsPrivileged(
    localPath: string,
    remotePath: string,
    onProgress: (progress: TransferProgress) => void,
    cause: unknown
  ): Promise<void> {
    this.ensureShellFileFallback(cause)
    const total = Math.max((await stat(localPath)).size, 1)
    const tempRemotePath = await this.createTemporaryRemoteUploadPath(path.posix.basename(remotePath))
    appLog(`[FileTerm][SFTP] Root upload staging ${localPath} -> ${tempRemotePath} -> ${remotePath}`)

    try {
      await this.uploadFileAsUser(localPath, tempRemotePath, (progress) => {
        onProgress({
          percent: Math.min(99, Math.max(1, progress.percent === 100 ? 99 : progress.percent)),
          transferredBytes: progress.transferredBytes,
          totalBytes: progress.totalBytes,
          message: undefined
        })
      })
      onProgress({
        percent: 99,
        transferredBytes: total,
        totalBytes: total,
        message: '正在应用 root 写入...'
      })
      await this.ensureRemoteDirectory(path.posix.dirname(remotePath))
      await this.execShellFileCommand(
        `mv ${shellQuote(tempRemotePath)} ${shellQuote(remotePath)}`,
        { allowNonZeroWithStdout: true },
        true
      )
      await this.verifyShellRemoteUploadSize(remotePath, total, true)
      appLog(`[FileTerm][SFTP] Root upload verified ${remotePath} (${formatShellBytes(total)})`)
      onProgress({
        percent: 100,
        transferredBytes: total,
        totalBytes: total,
        message: undefined
      })
    } catch (error) {
      try {
        await this.execCommand(`sh -lc ${shellQuote(`rm -f -- ${tempRemotePath}`)}`, { allowNonZeroWithStdout: true })
      } catch {
        // Best-effort cleanup for temp upload artifacts.
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

  private async verifyShellRemoteUploadSize(remotePath: string, expectedSize: number, privileged: boolean): Promise<void> {
    const output = await this.execShellFileCommand(`stat -c %s -- ${shellQuote(remotePath)} || wc -c < ${shellQuote(remotePath)}`, undefined, privileged)
    const remoteSize = parseRemoteByteSize(output)
    if (remoteSize !== undefined) {
      this.assertRemoteUploadSize(remotePath, remoteSize, expectedSize)
      return
    }

    appWarn(`[FileTerm][SSH] Shell upload size check returned no parseable size for ${remotePath}: ${singleLine(output) || '(empty)'}`)
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
    throw new Error(`传输校验失败：${path.posix.basename(remotePath)} 实际为 ${actual}，期望 ${formatShellBytes(expectedSize)}`)
  }

  private async downloadFileViaShell(
    remotePath: string,
    localPath: string,
    onProgress: (progress: TransferProgress) => void,
    cause: unknown
  ): Promise<void> {
    this.ensureShellFileFallback(cause)
    appLog(`[FileTerm][SSH] Shell download start ${remotePath} -> ${localPath}`)
    const output = await this.execShellFileCommand(`base64 ${shellQuote(remotePath)}`, undefined, this.fileAccessMode === 'root')
    const payload = Buffer.from(output.replace(/\s+/g, ''), 'base64')
    const total = Math.max(payload.byteLength, 1)
    onProgress({ percent: 80, transferredBytes: Math.round(total * 0.8), totalBytes: total })
    await writeFile(localPath, payload)
    appLog(`[FileTerm][SSH] Shell download verified ${remotePath} -> ${localPath} (${formatShellBytes(total)})`)
    onProgress({ percent: 100, transferredBytes: total, totalBytes: total })
  }

  private async readRemoteDirectoryViaShell(targetPath: string, cause: unknown): Promise<RemoteFileItem[]> {
    this.ensureShellFileFallback(cause)
    const outputStartMarker = '__FILETERM_DIR_LIST_START__'
    const outputEndMarker = '__FILETERM_DIR_LIST_END__'
    const output = await this.execShellFileCommand(`
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
`, undefined, this.fileAccessMode === 'root')
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
          type: type === 'folder' ? 'folder' as const : 'file' as const,
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
      return this.execCommand(`sudo -S -p '' -u ${shellQuote(sudoUser)} sh -lc ${shellQuote(command)}`, options, true, stdin)
    }

    return this.execCommand(`sudo -n -u ${shellQuote(sudoUser)} sh -lc ${shellQuote(command)}`, options, true, stdinPayload)
  }

  private async streamLocalFileToShellCommand(
    localPath: string,
    command: string,
    privileged: boolean,
    expectedBytes: number,
    onProgress?: (transferredBytes: number) => void
  ): Promise<void> {
    const execClient = await this.ensureExecConnection()
    return new Promise<void>((resolve, reject) => {
      let settled = false
      let transferredBytes = 0
      let stdout = ''
      let stderr = ''
      let readEnded = false
      let readStream = createReadStream(localPath)
      let channel: ClientChannel | undefined

      const safeResolve = () => {
        if (!settled) {
          settled = true
          clearTimeout(timeoutId)
          resolve()
        }
      }

      const safeReject = (error: Error) => {
        if (!settled) {
          settled = true
          clearTimeout(timeoutId)
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

      const timeoutId = setTimeout(() => {
        safeReject(new Error('文件上传超时'))
      }, 10 * 60 * 1000)

      const handlePrivilegeError = (message: string): boolean => {
        if (!privileged) {
          return false
        }
        if (/incorrect password|authentication failure|3 incorrect password attempts|sorry, try again|no password was provided|密码错误|认证失败|对不起，请重试|未提供密码/i.test(message)) {
          this.sudoPassword = undefined
          safeReject(new Error('sudo 密码错误，请重新输入。'))
          return true
        }
        if (/password is required|a password is required|no tty present|a terminal is required|sorry, you must have a tty|需要密码|必须输入密码/i.test(message)) {
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

        readStream.on('data', (chunk: any) => {
          const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk)
          transferredBytes += buffer.byteLength
          onProgress?.(transferredBytes)
          try {
            if (!stream.write(buffer)) {
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
            safeReject(new Error(`远程写入通道提前关闭，仅发送 ${formatShellBytes(transferredBytes)} / ${formatShellBytes(expectedBytes)}`))
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
    const window = (this as any)._sudoWindow || ''
    const newWindow = (window + text).slice(-200)
    ;(this as any)._sudoWindow = newWindow
    const normalized = newWindow.replace(/\u001b\[[0-9;?]*[ -/]*[@-~]/g, '')
    
    // Only test the newly appended part + a little overlap, OR just check if the pattern appears 
    // at the very end of the string to avoid triggering continuously.
    // For password prompt, we want it to match when it appears at the end of the current buffer.
    if (!this.awaitingSudoPasswordInput && /(\[sudo\]|password|密码|passphrase)[^\r\n]*[:：]\s*$/i.test(normalized)) {
      this.awaitingSudoPasswordInput = true
      this.pendingSudoPasswordInput = ''
      
      // Attempt to recover blind-typed password from recent keystrokes
      const recentKeys = (this as any)._recentKeystrokes || ''
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
    if (/(incorrect password|authentication failure|sorry, try again|密码错误|认证失败|对不起，请重试)\s*$/i.test(normalized)) {
      this.sudoPassword = undefined
    }
  }

  private captureSudoPasswordInput(data: string) {
    // Keep a buffer of recent keystrokes to support blind typing recovery
    let recentKeys = (this as any)._recentKeystrokes || ''
    recentKeys += data
    if (recentKeys.length > 200) {
      recentKeys = recentKeys.slice(-200)
    }
    ;(this as any)._recentKeystrokes = recentKeys

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
    options?: { allowNonZeroWithStdout?: boolean },
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

      const timeoutId = setTimeout(() => {
        if (!settled) {
          if (streamInstance) {
            try {
              streamInstance.destroy()
            } catch {
              // ignore
            }
          }
          safeReject(new Error('命令执行超时'))
        }
      }, 15000)

      const handleExec = (error: Error | undefined, stream: ClientChannel) => {
        if (error) {
          safeReject(error)
          return
        }

        streamInstance = stream

        let stdout = ''
        let stderr = ''

        stream.on('data', (chunk: Buffer) => {
          const chunkStr = chunk.toString('utf8')
          stdout += chunkStr

          if (privileged && stdinPayload !== undefined) {
            if (/incorrect password|authentication failure|3 incorrect password attempts|sorry, try again|no password was provided|密码错误|认证失败|对不起，请重试|未提供密码/i.test(stdout)) {
              this.sudoPassword = undefined
              safeReject(new Error('sudo 密码错误，请重新输入。'))
              try {
                stream.destroy()
              } catch {
                // ignore
              }
              return
            }
            if (/password is required|a password is required|no tty present|a terminal is required|sorry, you must have a tty|需要密码|必须输入密码/i.test(stdout)) {
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
            if (privileged && /password is required|a password is required|no tty present|a terminal is required|sorry, you must have a tty|需要密码|必须输入密码/i.test(errMessage)) {
              safeReject(new Error('未检测到可复用的 sudo 授权，需要提供 sudo 密码。'))
              return
            }
            if (privileged && /incorrect password|authentication failure|3 incorrect password attempts|sorry, try again|no password was provided|密码错误|认证失败|对不起，请重试|未提供密码/i.test(errMessage)) {
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
    this.awaitingSudoPasswordInput = false
    this.pendingSudoPasswordInput = ''
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

async function resolveSshAuthConfig(profile: SshProfile): Promise<Pick<ConnectConfig, 'password' | 'privateKey' | 'passphrase' | 'agent'>> {
  if (profile.authType === 'password') {
    if (profile.password) {
      return {
        password: profile.password
      }
    }

    return resolveSystemSshAuthConfig(profile)
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

async function resolveSystemSshAuthConfig(profile: SshProfile): Promise<Pick<ConnectConfig, 'privateKey' | 'passphrase' | 'agent'>> {
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

function registerKeyboardInteractiveHandler(client: Client, profile: SshProfile, onEvent: (message: string) => void) {
  if (!profile.password) {
    return
  }

  client.on('keyboard-interactive', (_name, _instructions, _instructionsLang, prompts, finish) => {
    onEvent(`收到 keyboard-interactive 认证请求，提示数 ${prompts.length}`)
    if (!prompts.length) {
      finish([])
      return
    }

    onEvent(`自动回复 keyboard-interactive 提示: ${prompts.map((prompt) => singleLine(prompt.prompt)).join(' | ')}`)
    finish(prompts.map(() => profile.password ?? ''))
  })
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
  return `'${value.replace(/'/g, `'\"'\"'`)}'`
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
