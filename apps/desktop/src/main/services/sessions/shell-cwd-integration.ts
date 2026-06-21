import path from 'node:path'

const OSC_7_PREFIX = '\u001b]7;'
const OSC_7_PATTERN = /\u001b]7;file:\/\/([^\u0007\u001b]*)(?:\u0007|\u001b\\)/g
const MAX_REPORTED_CWD_LENGTH = 4096

export type RemoteShellKind = 'bash' | 'zsh' | 'fish' | 'posix'

export class ShellCwdTracker {
  private buffer = ''

  feed(chunk: string): string[] {
    const combined = `${this.buffer}${chunk}`
    const paths: string[] = []
    let lastCompleteEnd = 0

    OSC_7_PATTERN.lastIndex = 0
    for (const match of combined.matchAll(OSC_7_PATTERN)) {
      lastCompleteEnd = (match.index ?? 0) + match[0].length
      const cwd = parseOsc7Payload(match[1] ?? '')
      if (cwd) {
        paths.push(cwd)
      }
    }

    if (lastCompleteEnd > 0) {
      this.buffer = combined.slice(lastCompleteEnd)
    } else {
      const markerStart = combined.lastIndexOf(OSC_7_PREFIX)
      this.buffer = markerStart >= 0
        ? combined.slice(markerStart)
        : combined.slice(-OSC_7_PREFIX.length)
    }

    if (this.buffer.length > 4096) {
      this.buffer = this.buffer.slice(-4096)
    }

    return paths
  }
}

export const SETUP_NEEDLE = 'test -z "$FISH_VERSION"'

export const SHELL_CWD_SETUP = 'test -z "$FISH_VERSION" && eval \'__tdcwd() { printf "\\033]7;file://%s\\007" "$(pwd -P 2>/dev/null)"; }; if [ -n "$ZSH_VERSION" ]; then autoload -Uz add-zsh-hook 2>/dev/null; add-zsh-hook -D precmd __tdcwd 2>/dev/null; add-zsh-hook precmd __tdcwd 2>/dev/null; elif [ -n "$BASH_VERSION" ]; then case "$PROMPT_COMMAND" in *"__tdcwd"*) ;; *) PROMPT_COMMAND="__tdcwd${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;; esac; else case "$PS1" in *"__tdcwd"*) ;; *) PS1="\\$(__tdcwd)$PS1" ;; esac; fi; __tdcwd\''

export function findSetupEchoEnd(text: string): { lineStart: number; osc7End: number; cwd: string | null } | null {
  const needleIndex = text.indexOf(SETUP_NEEDLE)
  if (needleIndex < 0) {
    return null
  }

  const lineStart = text.lastIndexOf('\n', needleIndex) + 1
  const searchSlice = text.slice(needleIndex)

  OSC_7_PATTERN.lastIndex = 0
  const match = OSC_7_PATTERN.exec(searchSlice)
  if (!match) {
    return null
  }

  const osc7End = needleIndex + match.index + match[0].length
  const cwd = parseOsc7Payload(match[1] ?? '')

  return { lineStart, osc7End, cwd }
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
