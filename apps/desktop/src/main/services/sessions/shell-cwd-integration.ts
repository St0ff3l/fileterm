import path from 'node:path'

const OSC_7_PREFIX = '\u001b]7;'
const OSC_7_PATTERN = /\u001b]7;file:\/\/([^\u0007\u001b]*)(?:\u0007|\u001b\\)/g
const OSC_USER_PATTERN = /\u001b]1337;RemoteUser=([^\u0007\u001b]*)(?:\u0007|\u001b\\)/g
const MAX_REPORTED_CWD_LENGTH = 4096

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
      const user = matchUser[1]
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
      this.buffer = markerStart >= 0
        ? combined.slice(markerStart)
        : combined.slice(-Math.max(OSC_7_PREFIX.length, 12))
    }

    if (this.buffer.length > 4096) {
      this.buffer = this.buffer.slice(-4096)
    }

    return updates
  }
}

export const SETUP_NEEDLE = 'test -z "$FISH_VERSION"'

export const SHELL_CWD_SETUP = 'test -z "$FISH_VERSION" && eval \'__tdcwd() { printf "\\033]7;file://%s\\007\\033]1337;RemoteUser=%s\\007" "$(pwd -P 2>/dev/null)" "$(id -un 2>/dev/null)"; }; if [ -n "$ZSH_VERSION" ]; then autoload -Uz add-zsh-hook 2>/dev/null; add-zsh-hook -D precmd __tdcwd 2>/dev/null; add-zsh-hook precmd __tdcwd 2>/dev/null; elif [ -n "$BASH_VERSION" ]; then case "$PROMPT_COMMAND" in *"__tdcwd"*) ;; *) PROMPT_COMMAND="__tdcwd${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;; esac; else case "$PS1" in *"__tdcwd"*) ;; *) PS1="\\$(__tdcwd)$PS1" ;; esac; fi; __tdcwd\''

export function findSetupEchoEnd(text: string): { lineStart: number; payloadEnd: number; cwd: string | null; user: string | null } | null {
  const needleIndex = text.indexOf(SETUP_NEEDLE)
  if (needleIndex < 0) {
    return null
  }

  const lineStart = text.lastIndexOf('\n', needleIndex) + 1
  const searchSlice = text.slice(needleIndex)

  OSC_7_PATTERN.lastIndex = 0
  const match7 = OSC_7_PATTERN.exec(searchSlice)
  if (!match7) {
    return null
  }

  OSC_USER_PATTERN.lastIndex = 0
  const matchUser = OSC_USER_PATTERN.exec(searchSlice)

  const osc7End = needleIndex + match7.index + match7[0].length
  const oscUserEnd = matchUser ? needleIndex + matchUser.index + matchUser[0].length : osc7End
  
  const payloadEnd = Math.max(osc7End, oscUserEnd)
  const cwd = parseOsc7Payload(match7[1] ?? '')
  const user = matchUser ? matchUser[1] : null

  // Keep the real shell prompt visible. We only suppress the injected setup
  // command echo and its OSC payload, so the prompt printed after execution can
  // render naturally on first connect and later silent refreshes.
  return { lineStart, payloadEnd, cwd, user }
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
