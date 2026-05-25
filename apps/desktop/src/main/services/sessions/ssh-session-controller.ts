import { createHash } from 'node:crypto'
import os from 'node:os'
import path from 'node:path'
import { readFile, stat, writeFile } from 'node:fs/promises'
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
} from '@termdock/core'
import { BaseFileSessionController } from './base-file-session-controller.js'
import { buildMetricsCommand, parentRemotePath, parseSystemMetrics, toRemoteFileItem } from './session-file-utils.js'
import { createSshDebugLogger, isSshDebugEnabled, singleLine } from './ssh-debug-logger.js'
import { decodeBuffer, encodeText } from '../text-encoding.js'

export class LiveSshSessionController extends BaseFileSessionController implements SshSessionController {
  readonly type = 'ssh'
  private static readonly TRANSCRIPT_LIMIT = 200_000

  private readonly ssh = new Client()
  private readonly execSsh = new Client()
  private readonly sftpSsh = new Client()
  private readonly sshDebug = createSshDebugLogger(isSshDebugEnabled(), (message) => {
    this.appendSystemMessage(message)
  })
  private sftp?: SFTPWrapper
  private sftpUnavailableReason: string | null = null
  private sshConfig?: ConnectConfig
  private execReady = false
  private execConnectPromise?: Promise<Client>
  private hasRegisteredExecLifecycle = false
  private shellStream?: {
    write(data: string): void
    setWindow(rows: number, cols: number, height: number, width: number): void
    end(): void
  }
  private transcript = ''
  private currentRemotePath: string
  private fileAccessMode: 'user' | 'root' = 'user'
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
    private readonly onStateChange: (summary: string, transcript: string, connected: boolean) => void
  ) {
    super(id, 'ssh', profile)
    this.currentRemotePath = profile.remotePath || '.'
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
          this.onStateChange(this.getSummary(), this.transcript, true)
          this.ssh.shell(
            {
              term: 'xterm-256color',
              rows: 32,
              cols: 120
            },
            (error: Error | undefined, stream: ClientChannel) => {
              if (error) {
                connectionFailed = true
                this.appendSystemMessage(`终端启动失败: ${error.message}\r\n`)
                this.onStateChange(`Connection error: ${error.message}`, this.transcript, false)
                if (!settled) {
                  settled = true
                  reject(error)
                }
                return
              }

              this.shellStream = stream
              stream.on('data', (chunk: Buffer) => {
                const text = chunk.toString('utf8')
                this.transcript = trimTranscript(`${this.transcript}${text}`, LiveSshSessionController.TRANSCRIPT_LIMIT)
                this.trackSudoPromptFromTerminal(text)
                this.onData(text)
              })
              stream.on('close', () => {
                this.connected = false
                this.onStateChange('Shell closed', this.transcript, false)
              })

              if (!settled) {
                settled = true
                resolve()
              }
            }
          )
        })
        .on('error', (error: Error) => {
          this.connected = false
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
          this.onStateChange(`Connection error: ${error.message}`, this.transcript, false)
        })
        .on('close', () => {
          this.connected = false
          if (connectionFailed) {
            this.sshDebug.log('main', '连接失败后收到关闭事件')
            return
          }
          this.appendSystemMessage('连接已断开\r\n')
          this.sshDebug.log('main', '连接已关闭')
          this.onStateChange('Disconnected', this.transcript, false)
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
    this.shellStream?.end()
    this.closeExecSession()
    this.closeSftpSession()
    this.ssh.end()
    this.sftpSsh.end()
    this.connected = false
  }

  getTerminalTranscript(): string {
    return this.transcript
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
    this.closeSftpSession()
  }

  async write(data: string): Promise<void> {
    this.captureSudoPasswordInput(data)
    this.shellStream?.write(data)
  }

  async resize(cols: number, rows: number, width: number, height: number): Promise<void> {
    this.shellStream?.setWindow(rows, cols, Math.max(0, Math.floor(height)), Math.max(0, Math.floor(width)))
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
    this.currentRemotePath = nextPath
    if (this.fileAccessMode === 'root') {
      return this.readRemoteDirectoryViaShell(this.currentRemotePath, new Error('Root file access mode enabled'))
    }

    try {
      return await this.readRemoteDirectory(this.currentRemotePath)
    } catch (error) {
      return this.readRemoteDirectoryViaShell(this.currentRemotePath, error)
    }
  }

  async readRemoteFile(targetPath: string, encoding = 'utf-8'): Promise<string> {
    if (this.fileAccessMode === 'root') {
      return this.readRemoteFileViaShell(targetPath, new Error('Root file access mode enabled'), encoding)
    }

    try {
      const sftp = await this.ensureSftp()
      return await new Promise<string>((resolve, reject) => {
        sftp.readFile(targetPath, (error, data) => {
          if (error) {
            reject(error)
            return
          }
          resolve(decodeBuffer(Buffer.isBuffer(data) ? data : Buffer.from(data), encoding))
        })
      })
    } catch (error) {
      return this.readRemoteFileViaShell(targetPath, error, encoding)
    }
  }

  async writeRemoteFile(targetPath: string, content: string, encoding = 'utf-8'): Promise<void> {
    if (this.fileAccessMode === 'root') {
      await this.ensureRemoteDirectory(path.posix.dirname(targetPath))
      await this.writeRemoteFileViaShell(targetPath, content, new Error('Root file access mode enabled'), encoding)
      return
    }

    try {
      const sftp = await this.ensureSftp()
      await this.ensureRemoteDirectory(path.posix.dirname(targetPath))
      const payload = encodeText(content, encoding)
      await new Promise<void>((resolve, reject) => {
        sftp.writeFile(targetPath, payload, (error) => {
          if (error) {
            reject(error)
            return
          }
          resolve()
        })
      })
    } catch (error) {
      await this.writeRemoteFileViaShell(targetPath, content, error, encoding)
    }
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

  async ensureRemoteDirectory(targetPath: string): Promise<void> {
    const normalized = path.posix.normalize(targetPath || '.')
    if (!normalized || normalized === '.' || normalized === '/') {
      return
    }

    if (this.fileAccessMode === 'root') {
      await this.execShellFileCommand(`mkdir -p ${shellQuote(normalized)}`, { allowNonZeroWithStdout: true }, true)
      return
    }

    try {
      const sftp = await this.ensureSftp()
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
    try {
      const sftp = await this.ensureSftp()
      const info = await stat(localPath)
      const total = Math.max(info.size, 1)
      await this.ensureRemoteDirectory(path.posix.dirname(remotePath))
      await new Promise<void>((resolve, reject) => {
        sftp.fastPut(localPath, remotePath, {
          step: (transferred, _chunk, fileSize) => onProgress({
            percent: Math.min(99, Math.round((transferred / total) * 100)),
            transferredBytes: transferred,
            totalBytes: Math.max(fileSize || total, 1)
          })
        }, (error) => {
          if (error) {
            reject(error)
            return
          }
          onProgress({ percent: 100, transferredBytes: total, totalBytes: total })
          resolve()
        })
      })
    } catch (error) {
      await this.uploadFileViaShell(localPath, remotePath, onProgress, error)
    }
  }

  async downloadFile(remotePath: string, localPath: string, onProgress: (progress: TransferProgress) => void): Promise<void> {
    if (this.fileAccessMode === 'root') {
      await this.downloadFileViaShell(remotePath, localPath, onProgress, new Error('Root file access mode enabled'))
      return
    }

    try {
      const sftp = await this.ensureSftp()
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
      await new Promise<void>((resolve, reject) => {
        sftp.fastGet(remotePath, localPath, {
          step: (transferred, _chunk, fileSize) => onProgress({
            percent: Math.min(99, Math.round((transferred / total) * 100)),
            transferredBytes: transferred,
            totalBytes: Math.max(fileSize || total, 1)
          })
        }, (error) => {
          if (error) {
            reject(error)
            return
          }
          onProgress({ percent: 100, transferredBytes: total, totalBytes: total })
          resolve()
        })
      })
    } catch (error) {
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
    this.appendSystemMessage(`[TermDock] ${message}\r\n`)
    this.onStateChange(this.getSummary(), this.transcript, this.connected)
  }

  private async ensureSftp(): Promise<SFTPWrapper> {
    if (this.sftp) {
      return this.sftp
    }

    if (!this.sshConfig) {
      throw new Error('SFTP connection not initialized')
    }

    const sshConfig = this.sshConfig
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
        this.sftp = sftp
        resolve(sftp)
      })
    })
  }

  private closeSftpSession() {
    this.sftp?.end?.()
    this.sftp = undefined
    this.sftpSsh.end()
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

  private async uploadFileViaShell(
    localPath: string,
    remotePath: string,
    onProgress: (progress: TransferProgress) => void,
    cause: unknown,
    privileged = this.fileAccessMode === 'root'
  ): Promise<void> {
    this.ensureShellFileFallback(cause)
    const payload = await readFile(localPath)
    const total = Math.max(payload.byteLength, 1)
    onProgress({ percent: 20, transferredBytes: Math.round(total * 0.2), totalBytes: total })
    await this.execShellFileCommand(
      `base64 -d > ${shellQuote(remotePath)}`,
      undefined,
      privileged,
      `${payload.toString('base64')}\n`
    )
    onProgress({ percent: 100, transferredBytes: total, totalBytes: total })
  }

  private async uploadFileAsPrivileged(
    localPath: string,
    remotePath: string,
    onProgress: (progress: TransferProgress) => void,
    cause: unknown
  ): Promise<void> {
    this.ensureShellFileFallback(cause)
    const tempRemotePath = await this.createTemporaryRemoteUploadPath(path.posix.basename(remotePath))

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
        transferredBytes: undefined,
        totalBytes: undefined,
        message: '正在应用 root 写入...'
      })
      await this.ensureRemoteDirectory(path.posix.dirname(remotePath))
      await this.execShellFileCommand(
        `mv ${shellQuote(tempRemotePath)} ${shellQuote(remotePath)}`,
        { allowNonZeroWithStdout: true },
        true
      )
      onProgress({
        percent: 100,
        transferredBytes: undefined,
        totalBytes: undefined,
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

  private async downloadFileViaShell(
    remotePath: string,
    localPath: string,
    onProgress: (progress: TransferProgress) => void,
    cause: unknown
  ): Promise<void> {
    this.ensureShellFileFallback(cause)
    const output = await this.execShellFileCommand(`base64 ${shellQuote(remotePath)}`, undefined, this.fileAccessMode === 'root')
    const payload = Buffer.from(output.replace(/\s+/g, ''), 'base64')
    const total = Math.max(payload.byteLength, 1)
    onProgress({ percent: 80, transferredBytes: Math.round(total * 0.8), totalBytes: total })
    await writeFile(localPath, payload)
    onProgress({ percent: 100, transferredBytes: total, totalBytes: total })
  }

  private async readRemoteDirectoryViaShell(targetPath: string, cause: unknown): Promise<RemoteFileItem[]> {
    this.ensureShellFileFallback(cause)
    const output = await this.execShellFileCommand(`
target=${shellQuote(targetPath)}
if [ ! -d "$target" ]; then
  echo "__NOT_DIR__"
  exit 1
fi
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
`, undefined, this.fileAccessMode === 'root')
    if (output.trim() === '__NOT_DIR__') {
      throw new Error(`无法打开远程目录: ${targetPath}`)
    }

    const rows = output
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
    const output = await this.execCommand(`sh -lc ${shellQuote(`mktemp /tmp/termdock-upload.XXXXXX-${safeName}`)}`)
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

  private async verifyRootFileAccess(): Promise<void> {
    await this.execShellFileCommand('true', undefined, true)
  }

  private trackSudoPromptFromTerminal(text: string) {
    const normalized = text.replace(/\u001b\[[0-9;?]*[ -/]*[@-~]/g, '')
    if (/\[sudo\][^\r\n]*password for .*:|sudo[^\r\n]*密码[:：]?/i.test(normalized)) {
      this.awaitingSudoPasswordInput = true
      this.pendingSudoPasswordInput = ''
    }
  }

  private captureSudoPasswordInput(data: string) {
    if (!this.awaitingSudoPasswordInput) {
      return
    }

    for (const char of data) {
      if (char === '\u0003' || char === '\u001b') {
        this.awaitingSudoPasswordInput = false
        this.pendingSudoPasswordInput = ''
        return
      }

      if (char === '\r' || char === '\n') {
        if (this.pendingSudoPasswordInput) {
          this.sudoPassword = this.pendingSudoPasswordInput
          this.onStateChange(this.getSummary(), this.transcript, this.connected)
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
      const handleExec = (error: Error | undefined, stream: ClientChannel) => {
        if (error) {
          reject(error)
          return
        }

        let stdout = ''
        let stderr = ''

        stream.on('data', (chunk: Buffer) => {
          stdout += chunk.toString('utf8')
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
            resolve(stdout)
            return
          }
          if (code && code !== 0 && stderr.trim()) {
            if (privileged && /password is required|a password is required|no tty present|a terminal is required|sorry, you must have a tty|需要密码|必须输入密码/i.test(stderr)) {
              reject(new Error('当前这次 root 切换没有拿到可复用的 sudo 授权，请直接输入 sudo 密码后重试。终端里先执行 `sudo -i` 或 `sudo -v`，这里也不一定能复用。'))
              return
            }
            if (privileged && /incorrect password|authentication failure|3 incorrect password attempts|sorry, try again|no password was provided|密码错误|认证失败|对不起，请重试|未提供密码/i.test(stderr)) {
              reject(new Error('sudo 密码无效，请重新输入。'))
              return
            }
            reject(new Error(stderr.trim()))
            return
          }
          resolve(stdout)
        })
      }

      if (privileged && stdinPayload !== undefined) {
        execClient.exec(command, { pty: true }, handleExec)
        return
      }

      execClient.exec(command, handleExec)
    })
  }

  private appendSystemMessage(message: string) {
    this.transcript = trimTranscript(`${this.transcript}${message}`, LiveSshSessionController.TRANSCRIPT_LIMIT)
    this.onData(message)
    this.onStateChange(this.getSummary(), this.transcript, this.connected)
  }

}

function trimTranscript(transcript: string, limit: number) {
  if (transcript.length <= limit) {
    return transcript
  }

  return transcript.slice(transcript.length - limit)
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

function shellQuote(value: string) {
  return `'${value.replace(/'/g, `'\"'\"'`)}'`
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
