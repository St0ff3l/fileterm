import os from 'node:os'
import path from 'node:path'
import { readFile, stat, writeFile } from 'node:fs/promises'
import type { ClientChannel, ConnectConfig, FileEntry, SFTPWrapper } from 'ssh2'
import { Client } from 'ssh2'
import type { FileSessionController, RemoteFileItem, SystemMetrics, SshProfile, SshSessionController } from '@termdock/core'
import { BaseFileSessionController } from './base-file-session-controller.js'
import { buildMetricsCommand, parentRemotePath, parseSystemMetrics, toRemoteFileItem } from './session-file-utils.js'
import { createSshDebugLogger, isSshDebugEnabled, singleLine } from './ssh-debug-logger.js'

export class LiveSshSessionController extends BaseFileSessionController implements SshSessionController {
  readonly type = 'ssh'

  private readonly ssh = new Client()
  private readonly sftpSsh = new Client()
  private readonly sshDebug = createSshDebugLogger(isSshDebugEnabled(), (message) => {
    this.appendSystemMessage(message)
  })
  private sftp?: SFTPWrapper
  private sftpUnavailableReason: string | null = null
  private sshConfig?: ConnectConfig
  private shellStream?: {
    write(data: string): void
    setWindow(rows: number, cols: number, height: number, width: number): void
    end(): void
  }
  private transcript = ''
  private currentRemotePath: string
  private metrics?: SystemMetrics

  constructor(
    id: string,
    profile: SshProfile,
    private readonly onData: (chunk: string) => void,
    private readonly onStateChange: (summary: string, transcript: string, connected: boolean) => void
  ) {
    super(id, 'ssh', profile)
    this.currentRemotePath = profile.remotePath || '.'
    this.appendSystemMessage('连接主机...\r\n')
  }

  override async connect(): Promise<void> {
    const profile = this.profile as SshProfile
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
                if (!settled) {
                  settled = true
                  reject(error)
                }
                return
              }

              this.shellStream = stream
              stream.on('data', (chunk: Buffer) => {
                const text = chunk.toString('utf8')
                this.transcript += text
                this.onData(text)
                this.onStateChange(this.getSummary(), this.transcript, true)
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
          this.appendSystemMessage('连接已断开\r\n')
          this.sshDebug.log('main', '连接已关闭')
          this.onStateChange('Disconnected', this.transcript, false)
        })
        .connect(sshConfig)
    })
  }

  override async disconnect(): Promise<void> {
    this.shellStream?.end()
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

  getSystemMetrics(): SystemMetrics | undefined {
    return this.metrics
  }

  async abortTransfer(): Promise<void> {
    this.closeSftpSession()
  }

  async write(data: string): Promise<void> {
    this.shellStream?.write(data)
  }

  async resize(cols: number, rows: number): Promise<void> {
    this.shellStream?.setWindow(rows, cols, rows * 16, cols * 8)
  }

  async listRemoteFiles(): Promise<RemoteFileItem[]> {
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
    try {
      return await this.readRemoteDirectory(this.currentRemotePath)
    } catch (error) {
      return this.readRemoteDirectoryViaShell(this.currentRemotePath, error)
    }
  }

  async readRemoteFile(targetPath: string): Promise<string> {
    try {
      const sftp = await this.ensureSftp()
      return await new Promise<string>((resolve, reject) => {
        sftp.readFile(targetPath, 'utf8', (error, data) => {
          if (error) {
            reject(error)
            return
          }
          resolve(typeof data === 'string' ? data : data.toString('utf8'))
        })
      })
    } catch (error) {
      return this.readRemoteFileViaShell(targetPath, error)
    }
  }

  async writeRemoteFile(targetPath: string, content: string): Promise<void> {
    try {
      const sftp = await this.ensureSftp()
      await this.ensureRemoteDirectory(path.posix.dirname(targetPath))
      await new Promise<void>((resolve, reject) => {
        sftp.writeFile(targetPath, content, 'utf8', (error) => {
          if (error) {
            reject(error)
            return
          }
          resolve()
        })
      })
    } catch (error) {
      await this.writeRemoteFileViaShell(targetPath, content, error)
    }
  }

  async ensureRemoteDirectory(targetPath: string): Promise<void> {
    const normalized = path.posix.normalize(targetPath || '.')
    if (!normalized || normalized === '.' || normalized === '/') {
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

  async uploadFile(localPath: string, remotePath: string, onProgress: (progress: number) => void): Promise<void> {
    try {
      const sftp = await this.ensureSftp()
      const info = await stat(localPath)
      const total = Math.max(info.size, 1)
      await this.ensureRemoteDirectory(path.posix.dirname(remotePath))
      await new Promise<void>((resolve, reject) => {
        sftp.fastPut(localPath, remotePath, {
          step: (transferred) => onProgress(Math.min(99, Math.round((transferred / total) * 100)))
        }, (error) => {
          if (error) {
            reject(error)
            return
          }
          onProgress(100)
          resolve()
        })
      })
    } catch (error) {
      await this.uploadFileViaShell(localPath, remotePath, onProgress, error)
    }
  }

  async downloadFile(remotePath: string, localPath: string, onProgress: (progress: number) => void): Promise<void> {
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
          step: (transferred) => onProgress(Math.min(99, Math.round((transferred / total) * 100)))
        }, (error) => {
          if (error) {
            reject(error)
            return
          }
          onProgress(100)
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
      const raw = await this.execCommand(buildMetricsCommand(), { allowNonZeroWithStdout: true })
      this.metrics = parseSystemMetrics(raw)
      return this.metrics
    } catch {
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

  private async readRemoteFileViaShell(targetPath: string, cause: unknown): Promise<string> {
    this.ensureShellFileFallback(cause)
    return this.execCommand(`sh -lc 'cat ${shellQuote(targetPath)}'`)
  }

  private async writeRemoteFileViaShell(targetPath: string, content: string, cause: unknown): Promise<void> {
    this.ensureShellFileFallback(cause)
    const payload = Buffer.from(content, 'utf8').toString('base64')
    await this.execCommand(`sh -lc 'base64 -d > ${shellQuote(targetPath)} <<'"'"'__TERMDOCK__'"'"'
${payload}
__TERMDOCK__'`)
  }

  private async uploadFileViaShell(
    localPath: string,
    remotePath: string,
    onProgress: (progress: number) => void,
    cause: unknown
  ): Promise<void> {
    this.ensureShellFileFallback(cause)
    const payload = await readFile(localPath)
    onProgress(20)
    await this.execCommand(`sh -lc 'base64 -d > ${shellQuote(remotePath)} <<'"'"'__TERMDOCK__'"'"'
${payload.toString('base64')}
__TERMDOCK__'`)
    onProgress(100)
  }

  private async downloadFileViaShell(
    remotePath: string,
    localPath: string,
    onProgress: (progress: number) => void,
    cause: unknown
  ): Promise<void> {
    this.ensureShellFileFallback(cause)
    const output = await this.execCommand(`sh -lc 'base64 ${shellQuote(remotePath)}'`)
    onProgress(80)
    await writeFile(localPath, Buffer.from(output.replace(/\s+/g, ''), 'base64'))
    onProgress(100)
  }

  private async readRemoteDirectoryViaShell(targetPath: string, cause: unknown): Promise<RemoteFileItem[]> {
    this.ensureShellFileFallback(cause)
    const output = await this.execCommand(`sh -lc '
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
  stat_line=$(stat -c "%Y|%s|%a|%u|%g" -- "$name" 2>/dev/null || echo "0|0|||")
  printf "%s\t%s\t%s\n" "$name" "$kind" "$stat_line"
done
'`)

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

  private ensureShellFileFallback(cause: unknown) {
    const profile = this.profile as SshProfile
    if (profile.enableExecChannel === false) {
      throw cause instanceof Error ? cause : new Error('SFTP unavailable and exec fallback disabled')
    }
  }

  private async execCommand(command: string, options?: { allowNonZeroWithStdout?: boolean }): Promise<string> {
    return new Promise<string>((resolve, reject) => {
      this.ssh.exec(command, (error, stream) => {
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
        stream.on('close', (code?: number) => {
          if (options?.allowNonZeroWithStdout && stdout.trim()) {
            resolve(stdout)
            return
          }
          if (code && code !== 0 && stderr.trim()) {
            reject(new Error(stderr.trim()))
            return
          }
          resolve(stdout)
        })
      })
    })
  }

  private appendSystemMessage(message: string) {
    this.transcript += message
    this.onData(message)
    this.onStateChange(this.getSummary(), this.transcript, this.connected)
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

function shellQuote(value: string) {
  return `'${value.replace(/'/g, `'\"'\"'`)}'`
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
