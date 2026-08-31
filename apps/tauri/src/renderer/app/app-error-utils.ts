import type { RemoteFileItem } from '@fileterm/core'
import {
  localizeConnectionErrorText,
  localizeErrorScope,
  localizeLocalTerminalText,
  localizeSerialTerminalText,
  t,
  type AppLocale
} from '../i18n'
import { REMOTE_METHOD_ERROR_PREFIX } from './app-shell-utils'

export type ErrorDetails = {
  item?: RemoteFileItem
  targetPath?: string
}

function normalizeErrorMessage(err: unknown) {
  const rawMessage = err instanceof Error ? err.message : String(err)
  return localizeConnectionErrorText(
    localizeSerialTerminalText(localizeLocalTerminalText(rawMessage.replace(REMOTE_METHOD_ERROR_PREFIX, '').trim()))
  )
}

export function formatAppError(scope: string, err: unknown, locale: AppLocale, details?: ErrorDetails) {
  const message = normalizeErrorMessage(err)
  const displayScope = localizeErrorScope(scope, locale)
  const likelyDisconnectedSession =
    /会话已断开|session disconnected|session not found|remote connection closed|connection closed/i.test(message)
  const likelyConcurrentRequestIssue =
    /another one is still running|forgot to use 'await'|client is closed because user launched a task/i.test(message)
  const likelyPathIssue = /can't cd to|__NOT_DIR__|no such file|not a directory|permission denied|\b550\b/i.test(
    message
  )
  const metadata = details?.item
    ? ` (${t.permission}: ${details.item.permission || '-'}, ${t.ownerGroup}: ${details.item.ownerGroup || '-'})`
    : ''
  const pathText = details?.targetPath ? ` ${details.targetPath}` : ''

  if (likelyDisconnectedSession) {
    return t.remoteSessionDisconnectedAction
  }

  if (locale === 'zhCN') {
    if (details?.targetPath && likelyConcurrentRequestIssue) {
      return `打开远程目录${pathText}${metadata}失败：远程连接正在处理另一项请求，请稍后重试。原始错误：${message}`
    }
    if (details?.targetPath && likelyPathIssue) {
      return `无法打开远程目录${pathText}${metadata}。可能是目录不存在、不是目录，或者当前账号没有进入权限。原始错误：${message}`
    }
    return `${scope}${pathText}${metadata}失败：${message}`
  }

  if (details?.targetPath && likelyConcurrentRequestIssue) {
    return `Failed to open remote directory${pathText}${metadata}: the remote connection is still processing another request. Raw error: ${message}`
  }
  if (details?.targetPath && likelyPathIssue) {
    return `Could not open remote directory${pathText}${metadata}. It may not exist, may not be a directory, or your account may not have permission to make changes. Raw error: ${message}`
  }

  return `${displayScope}${pathText}${metadata} failed: ${message}`
}

export function reportError(
  setter: (message: string) => void,
  locale: AppLocale,
  scope: string,
  err: unknown,
  details?: ErrorDetails
) {
  console.error(`[FileTerm] ${scope}`, err)
  setter(formatAppError(scope, err, locale, details))
}
