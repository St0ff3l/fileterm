import type { ConnectConfig } from 'ssh2'
import type { SshProfile } from '@fileterm/core'

type SshDebugScope = 'main' | 'sftp' | 'transfer-sftp' | 'exec'
type SshAuthConfig = Pick<ConnectConfig, 'password' | 'privateKey' | 'passphrase' | 'agent'>

export interface SshDebugLogger {
  readonly enabled: boolean
  handle(scope: SshDebugScope, message: string): void
  log(scope: SshDebugScope, message: string): void
  logConnectionStart(scope: SshDebugScope, profile: SshProfile, username: string, authConfig: SshAuthConfig, shouldTryKeyboard: boolean): void
  logSftpStart(username?: string, scope?: Extract<SshDebugScope, 'sftp' | 'transfer-sftp'>): void
  logKeyboardInteractive(scope: SshDebugScope, message: string): void
}

export function isSshDebugEnabled() {
  return process.env.FILETERM_SSH_DEBUG === '1'
}

export function createSshDebugLogger(
  enabled: boolean,
  append: (message: string) => void
): SshDebugLogger {
  const cache = new Set<string>()

  const log = (scope: SshDebugScope, message: string) => {
    if (!enabled) {
      return
    }
    append(`[FileTerm][SSH:${scope}] ${message}\r\n`)
  }

  return {
    enabled,
    handle(scope, message) {
      const normalized = normalizeSshDebugMessage(message)
      if (!enabled || !normalized) {
        return
      }

      const cacheKey = `${scope}:${normalized}`
      if (cache.has(cacheKey)) {
        return
      }

      cache.add(cacheKey)
      log(scope, normalized)
    },
    log,
    logConnectionStart(scope, profile, username, authConfig, shouldTryKeyboard) {
      log(
        scope,
        `开始连接 ${profile.host}:${profile.port}，profile=${profile.id}，用户 ${username || '(空)'}，密码${profile.password ? '已提供' : '未提供'}，认证 ${describeAuthAttempt(profile, authConfig, shouldTryKeyboard)}`
      )
    },
    logSftpStart(username, scope = 'sftp') {
      log(scope, `开始建立 SFTP 复用连接，用户 ${username || '(空)'}`)
    },
    logKeyboardInteractive(scope, message) {
      log(scope, message)
    }
  }
}

export function singleLine(value: string) {
  return value.replace(/\s+/g, ' ').trim()
}

function describeAuthAttempt(
  profile: SshProfile,
  authConfig: SshAuthConfig,
  shouldTryKeyboard: boolean
) {
  if (profile.authType === 'privateKey') {
    return `私钥路径 ${profile.privateKeyPath ?? '(未设置)'}`
  }

  if (profile.authType === 'password') {
    if (authConfig.password) {
      return `密码${shouldTryKeyboard ? ' + keyboard-interactive' : ''}`
    }
    return '系统 SSH 兜底'
  }

  return `系统 SSH${authConfig.agent ? ' + agent' : ''}${authConfig.privateKey ? ' + 默认私钥' : ''}`
}

function normalizeSshDebugMessage(message: string): string | null {
  const text = singleLine(message)

  if (
    /Outbound: Sending KEXINIT|Inbound: Handshake in progress|Handshake: (KEX|Host key format|C->S|S->C)/i.test(text)
  ) {
    return null
  }

  if (/Received USERAUTH_FAILURE/i.test(text)) {
    const methodsMatch = text.match(/methods left: ([^)]+)\)?$/i)
    return methodsMatch
      ? `认证失败，服务端仍允许: ${methodsMatch[1]}`
      : '认证失败，服务端拒绝当前方式'
  }

  if (/Client: none auth failed/i.test(text)) {
    return '服务端拒绝无认证探测'
  }

  if (/Client: password auth failed/i.test(text)) {
    return '密码认证失败'
  }

  if (/Client: publickey auth failed/i.test(text)) {
    return '公钥认证失败'
  }

  if (/Client: keyboard-interactive auth failed/i.test(text)) {
    return 'keyboard-interactive 认证失败'
  }

  if (/Inbound: Received USERAUTH_BANNER/i.test(text)) {
    return '收到 USERAUTH_BANNER'
  }

  if (/Inbound: Received USERAUTH_FAILURE/i.test(text)) {
    return text
  }

  if (/Outbound: Sending SERVICE_REQUEST \(ssh-userauth\)/i.test(text)) {
    return '开始请求 ssh-userauth 服务'
  }

  if (/Inbound: Received SERVICE_ACCEPT \(ssh-userauth\)/i.test(text)) {
    return '服务端接受 ssh-userauth 服务'
  }

  if (/Outbound: Sending USERAUTH_REQUEST \(none\)/i.test(text)) {
    return '发送认证探测 none'
  }

  if (/Outbound: Sending USERAUTH_REQUEST \(password\)/i.test(text)) {
    return '发送密码认证请求'
  }

  if (/Outbound: Sending USERAUTH_REQUEST \(publickey -- check\)/i.test(text)) {
    return '发送公钥可用性探测'
  }

  if (/Outbound: Sending USERAUTH_REQUEST \(publickey\)/i.test(text)) {
    return '发送公钥认证请求'
  }

  if (/Outbound: Sending USERAUTH_REQUEST \(keyboard-interactive\)/i.test(text)) {
    return '发送 keyboard-interactive 认证请求'
  }

  if (/Inbound: Received USERAUTH_INFO_REQUEST/i.test(text)) {
    return '服务端请求 keyboard-interactive 输入'
  }

  if (/Socket connected/i.test(text)) {
    return 'TCP 已连接'
  }

  if (/Socket ended/i.test(text)) {
    return 'Socket 被远端结束'
  }

  if (/Socket closed/i.test(text)) {
    return 'Socket 已关闭'
  }

  if (/Error:/i.test(text) || /All configured authentication methods failed/i.test(text)) {
    return text
  }

  return null
}
