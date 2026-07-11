import path from 'node:path'
import type { RemoteSystemPlatform } from '@fileterm/core'

const OSC_7_PREFIX = '\u001b]7;'
const OSC_7_PATTERN = /\u001b]7;file:\/\/([^\u0007\u001b]*)(?:\u0007|\u001b\\)/g
const OSC_USER_PATTERN = /\u001b]1337;RemoteUser=([^\u0007\u001b]*)(?:\u0007|\u001b\\)/g
const MAX_REPORTED_CWD_LENGTH = 4096
const MAX_REPORTED_USER_LENGTH = 256

export type RemoteShellKind = 'bash' | 'zsh' | 'fish' | 'posix'

export interface ShellStateUpdate {
  cwd?: string
  user?: string
}

export class ShellCwdTracker {
  private buffer = ''

  feed(chunk: string): ShellStateUpdate[] {
    const combined = `${this.buffer}${chunk}`
    const updates: ShellStateUpdate[] = []
    let lastCompleteEnd = 0

    OSC_7_PATTERN.lastIndex = 0
    let match7: RegExpExecArray | null
    while ((match7 = OSC_7_PATTERN.exec(combined)) !== null) {
      lastCompleteEnd = Math.max(lastCompleteEnd, match7.index + match7[0].length)
      const cwd = parseOsc7Payload(match7[1] ?? '')
      if (cwd) {
        updates.push({ cwd })
      }
    }

    OSC_USER_PATTERN.lastIndex = 0
    let matchUser: RegExpExecArray | null
    while ((matchUser = OSC_USER_PATTERN.exec(combined)) !== null) {
      lastCompleteEnd = Math.max(lastCompleteEnd, matchUser.index + matchUser[0].length)
      const user = parseRemoteUser(matchUser[1] ?? '')
      if (user) {
        updates.push({ user })
      }
    }

    if (lastCompleteEnd > 0) {
      this.buffer = combined.slice(lastCompleteEnd)
    } else {
      const marker7Start = combined.lastIndexOf(OSC_7_PREFIX)
      const markerUserStart = combined.lastIndexOf('\u001b]1337;')
      const markerStart = Math.max(marker7Start, markerUserStart)
      this.buffer = markerStart >= 0 ? combined.slice(markerStart) : combined.slice(-Math.max(OSC_7_PREFIX.length, 12))
    }

    if (this.buffer.length > 4096) {
      this.buffer = this.buffer.slice(-4096)
    }

    return updates
  }
}

export const SETUP_NEEDLE = 'test -z "${FISH_VERSION-}"'

export const SHELL_CWD_SETUP =
  'test -z "${FISH_VERSION-}" && eval \'__tdcwd() { printf "\\033]7;file://%s\\007\\033]1337;RemoteUser=%s\\007" "$(pwd -P 2>/dev/null)" "$(id -un 2>/dev/null)"; }; if [ -n "${ZSH_VERSION-}" ]; then autoload -Uz add-zsh-hook 2>/dev/null; add-zsh-hook -D precmd __tdcwd 2>/dev/null; add-zsh-hook precmd __tdcwd 2>/dev/null; elif [ -n "${BASH_VERSION-}" ]; then case "${PROMPT_COMMAND-}" in *"__tdcwd"*) ;; *) PROMPT_COMMAND="__tdcwd${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;; esac; else case "${PS1-}" in *"__tdcwd"*) ;; *) PS1="\\$(__tdcwd)${PS1-}" ;; esac; fi; __tdcwd\''

// BusyBox ash commonly uses a small interactive line-editing buffer. Keep its
// setup well below 256 bytes so the command cannot be truncated into an open
// quote/case statement and leave the terminal at a continuation prompt.
export const BUSYBOX_SHELL_CWD_SETUP =
  '__tdcwd(){ printf \'\\033]7;file://%s\\007\\033]1337;RemoteUser=%s\\007\' "$(pwd -P 2>/dev/null)" "$(id -un 2>/dev/null)";};PS1=\'$(__tdcwd)\'"${PS1-}";__tdcwd'

export function shellCwdSetupForPlatform(platform?: RemoteSystemPlatform): string | undefined {
  if (platform === 'busybox') {
    return BUSYBOX_SHELL_CWD_SETUP
  }
  if (platform === 'linux') {
    return SHELL_CWD_SETUP
  }
  return undefined
}

export function supportsPosixShellSetup(platform?: RemoteSystemPlatform): boolean {
  return shellCwdSetupForPlatform(platform) !== undefined
}

export function findSetupEchoEnd(
  text: string
): { lineStart: number; payloadEnd: number; cwd: string | null; user: string | null } | null {
  const needleIndex = text.indexOf(SETUP_NEEDLE)
  const searchStart = needleIndex >= 0 ? needleIndex : 0
  const lineStart = needleIndex >= 0 ? text.lastIndexOf('\n', needleIndex) + 1 : 0
  const searchSlice = text.slice(searchStart)

  OSC_7_PATTERN.lastIndex = 0
  let match7: RegExpExecArray | null
  let lastMatch7: RegExpExecArray | null = null
  while ((match7 = OSC_7_PATTERN.exec(searchSlice)) !== null) {
    lastMatch7 = match7
  }
  if (!lastMatch7) {
    return null
  }

  OSC_USER_PATTERN.lastIndex = 0
  let matchUser: RegExpExecArray | null
  let lastMatchUser: RegExpExecArray | null = null
  while ((matchUser = OSC_USER_PATTERN.exec(searchSlice)) !== null) {
    lastMatchUser = matchUser
  }

  const osc7End = searchStart + lastMatch7.index + lastMatch7[0].length
  const oscUserEnd = lastMatchUser ? searchStart + lastMatchUser.index + lastMatchUser[0].length : osc7End

  const payloadEnd = Math.max(osc7End, oscUserEnd)
  const cwd = parseOsc7Payload(lastMatch7[1] ?? '')
  const user = lastMatchUser ? parseRemoteUser(lastMatchUser[1] ?? '') : null

  // The controller waits for a short quiet period after this payload and drops
  // the setup echo plus the new prompt. The already visible prompt remains.
  return { lineStart, payloadEnd, cwd, user }
}

export function resolveShellFileAccess(
  loginUser: string,
  shellUser: string
): { mode: 'user' | 'root'; sudoUser?: string } {
  const normalizedLoginUser = loginUser.trim()
  const normalizedShellUser = shellUser.trim()
  if (!normalizedLoginUser || !normalizedShellUser || normalizedLoginUser === normalizedShellUser) {
    return { mode: 'user' }
  }
  return { mode: 'root', sudoUser: normalizedShellUser }
}

function parseRemoteUser(value: string): string | null {
  const user = value.trim()
  if (!user || user.length > MAX_REPORTED_USER_LENGTH || /[\u0000-\u001f\u007f]/.test(user)) {
    return null
  }
  return user
}

function parseOsc7Payload(payload: string): string | null {
  if (payload.length > MAX_REPORTED_CWD_LENGTH) {
    return null
  }

  const pathStart = payload.indexOf('/')
  if (pathStart < 0) {
    return null
  }

  const rawPath = payload.slice(pathStart)
  let decodedPath = rawPath
  try {
    decodedPath = decodeURIComponent(rawPath)
  } catch {
    // Some shells report a raw path containing a literal percent sign.
  }

  if (!decodedPath.startsWith('/')) {
    return null
  }

  return path.posix.normalize(decodedPath)
}
