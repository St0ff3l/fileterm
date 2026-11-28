import type { TerminalRenderer } from '@fileterm/core'

let terminalRenderer: TerminalRenderer = 'webgl'

export function supportsTerminalRenderer(platform: string | undefined) {
  return platform === 'win32' || platform === 'darwin' || platform === 'linux' || platform === 'web'
}

export function getTerminalRendererPreference() {
  return terminalRenderer
}

export function setTerminalRendererPreference(value: TerminalRenderer) {
  terminalRenderer = value
}
