import type { Terminal } from '@xterm/xterm'
import { stripClinkAutosuggestPrompt } from '../app/terminal-transcript'
import {
  formatMessage,
  localizeConnectionErrorText,
  localizeLocalTerminalText,
  localizeSerialTerminalText,
  t
} from '../i18n'

export function localizeTerminalText(value: string, options: { preserveUnknownTerminalOutput?: boolean } = {}) {
  return localizeSerialTerminalText(localizeLocalTerminalText(value), {
    ...options,
    // Terminal state summaries can contain remote/SSH text too. Preserve
    // unknown content by default; internal serial notices still fall back via
    // their explicit `[串口]` / `串口错误：` markers.
    preserveUnknownTerminalOutput: options.preserveUnknownTerminalOutput ?? true
  })
    .replaceAll('连接主机成功', t.terminalConnected)
    .replaceAll('连接主机...', t.terminalConnecting)
    .replaceAll('连接已断开', t.terminalDisconnected)
    .replaceAll('[connection closed]', t.terminalConnectionClosed)
    .replaceAll('Shell closed', t.terminalDisconnected)
    .replace(/(?:连接失败:|Connection error:)\s*([^\r\n]*)/g, (_match, error: string) => {
      const localizedError = localizeConnectionErrorText(error).replace(
        /^(?:SSH 连接失败：|FTP 连接失败：|FTP 登录失败：|Telnet 连接失败：)\s*/,
        ''
      )
      return `${t.connectionFailedPrefix}${localizedError}`
    })
    .replace(/Disconnected from\s*/g, t.disconnectedFromPrefix)
    .replace(/\bDisconnected\b/g, t.disconnected)
    .replace(/\[Telnet\] reconnect-scheduled: (\d+) (\d+)/g, (_match, seconds: string, attempt: string) =>
      formatMessage(t.telnetReconnectScheduled, { seconds, attempt })
    )
    .replace(/\[Telnet\] reconnect-limit: ([^\r\n]*)/g, (_match, error: string) =>
      formatMessage(t.telnetReconnectLimit, { error })
    )
}

export function toDisplayTerminalText(value: string) {
  // Localize fixed FileTerm notices before preserving terminal control semantics later.
  return localizeTerminalText(stripClinkAutosuggestPrompt(value), {
    preserveUnknownTerminalOutput: true
  })
}

export function splitOscPayload(payload: string) {
  const separatorIndex = payload.indexOf(';')
  if (separatorIndex === -1) {
    return null
  }

  return {
    target: payload.slice(0, separatorIndex),
    data: payload.slice(separatorIndex + 1)
  }
}

export function isOsc52TargetSupported(target: string) {
  return target === '' || /[cpsq01234567]/.test(target)
}

export function decodeBase64Utf8(value: string) {
  try {
    const normalized = value.replace(/\s+/g, '')
    const bytes = Uint8Array.from(atob(normalized), (char) => char.charCodeAt(0))
    return new TextDecoder().decode(bytes)
  } catch {
    return null
  }
}

export function encodeBase64Utf8(value: string) {
  const bytes = new TextEncoder().encode(value)
  let binary = ''
  for (const byte of bytes) {
    binary += String.fromCharCode(byte)
  }
  return btoa(binary)
}

export const TERMINAL_REMOTE_GUARD_COLS = 0
export const TERMINAL_FIT_GUARD_ROWS = 0
export const TERMINAL_RESIZE_PIXEL_EPSILON = 2
export const TERMINAL_RESIZE_SETTLE_MS = 140
// Bound one xterm parse pass without serializing the native input path.
export const TERMINAL_WRITE_CHUNK_SIZE = 16 * 1024
export const LOCAL_TERMINAL_STARTUP_TRANSCRIPT = 'Starting local shell...\r\n'
// WebView2 and WebKitGTK report high-resolution touchpad input in small pixel
// deltas, while a traditional mouse wheel commonly reports a large line/pixel
// delta in one event. Normalize and cap each event before accumulating so both
// inputs have a predictable one-notch / one-pinch-step feel.
export const TERMINAL_WHEEL_ZOOM_THRESHOLD = 12
export const TERMINAL_GESTURE_ZOOM_THRESHOLD = Math.log(1.08)

export type SplitPaneDirection = 'row' | 'column'

export function splitPaneShortcutsForPlatform(platform: string | undefined) {
  if (platform === 'darwin') {
    return { vertical: '⌘D', horizontal: '⇧⌘D', closePane: '⌘W' }
  }
  // Keep the Windows Terminal-style split keys on Windows and Linux. The
  // former Ctrl+Shift+D binding conflicts with OpenCode's delete-line action
  // and is also a plausible shortcut for other alternate-screen TUIs.
  return { vertical: 'Alt+Shift++', horizontal: 'Alt+Shift+-', closePane: 'Ctrl+Shift+W' }
}

/**
 * Find is an application action, so do not take Ctrl+F from the remote TUI:
 * readline, OpenCode and Gemini all use it for cursor movement. Keep the
 * application shortcut identical on every platform so Ctrl+F and
 * Ctrl+Shift+F have predictable ownership in SSH and local terminals.
 */
export function isTerminalFindShortcut(event: KeyboardEvent) {
  const matchesKey = event.key.toLowerCase() === 'f' || event.code === 'KeyF'
  return event.ctrlKey && event.shiftKey && !event.metaKey && !event.altKey && matchesKey
}

/**
 * Terminal copy/paste shortcuts intentionally differ from ordinary text
 * inputs on Linux and Windows: Ctrl+C/V belong to the remote shell, so the
 * desktop terminal convention is Ctrl+Shift+C/V. Use `code` as a fallback
 * because WebKitGTK can report a layout-dependent `key` for these physical
 * letter keys while still preserving the hardware code.
 */
export function isTerminalClipboardShortcut(event: KeyboardEvent, isMac: boolean, action: 'copy' | 'paste') {
  const expectedKey = action === 'copy' ? 'c' : 'v'
  const expectedCode = action === 'copy' ? 'KeyC' : 'KeyV'
  const matchesKey = event.key.toLowerCase() === expectedKey || event.code === expectedCode

  return isMac ? event.metaKey && !event.shiftKey && matchesKey : event.ctrlKey && event.shiftKey && matchesKey
}

/**
 * Keep local startup text stable across the reused TerminalView tab-switch
 * path. Windows deliberately omits the notice; other desktop platforms use
 * it as a race guard when the first PTY prompt arrives before hydration.
 * Remote transcripts must never use this fallback.
 */
export function normalizeLocalTerminalStartupTranscript(transcript: string, platform?: string) {
  if (platform === 'win32') {
    return transcript.startsWith(LOCAL_TERMINAL_STARTUP_TRANSCRIPT)
      ? transcript.slice(LOCAL_TERMINAL_STARTUP_TRANSCRIPT.length)
      : transcript
  }
  if (transcript.includes('Starting local shell...')) {
    return transcript
  }
  return `${LOCAL_TERMINAL_STARTUP_TRANSCRIPT}${transcript}`
}

export function isFocusTrackingSequence(data: string) {
  const escape = String.fromCharCode(27)
  return data === `${escape}[I` || data === `${escape}[O`
}

export function logTerminalClipboard(terminal: Terminal, action: string, details: Record<string, unknown> = {}) {
  if (!import.meta.env.DEV) {
    return
  }

  const selection = terminal.getSelection()
  console.debug(`[TerminalView][clipboard] ${action}`, {
    hasSelection: terminal.hasSelection(),
    mouseTrackingMode: terminal.modes.mouseTrackingMode,
    selectionLength: selection.length,
    ...details
  })
}

export function logTerminalZoom(terminal: Terminal, action: string, details: Record<string, unknown> = {}) {
  if (!import.meta.env.DEV) {
    return
  }

  console.info(`[TerminalView][zoom] ${action}`, {
    fontSize: terminal.options.fontSize,
    ...details
  })
}

export function logTerminalRender(terminal: Terminal, action: string, details: Record<string, unknown> = {}) {
  if (!import.meta.env.DEV) {
    return
  }

  const element = terminal.element
  const rect = element?.getBoundingClientRect()
  console.debug(`[TerminalView][render] ${action}`, {
    cols: terminal.cols,
    rows: terminal.rows,
    bufferType: terminal.buffer.active.type,
    bufferLength: terminal.buffer.active.length,
    viewportY: terminal.buffer.active.viewportY,
    elementConnected: Boolean(element?.isConnected),
    elementWidth: rect ? Math.round(rect.width) : 0,
    elementHeight: rect ? Math.round(rect.height) : 0,
    ...details
  })
}

export function describeTerminalInput(event: KeyboardEvent | WheelEvent) {
  const target = event.target
  return {
    type: event.type,
    target: target instanceof Element ? `${target.tagName}.${target.className}` : String(target),
    key: 'key' in event ? event.key : undefined,
    code: 'code' in event ? event.code : undefined,
    deltaY: 'deltaY' in event ? event.deltaY : undefined,
    deltaMode: 'deltaMode' in event ? event.deltaMode : undefined,
    ctrlKey: event.ctrlKey,
    shiftKey: event.shiftKey,
    metaKey: event.metaKey,
    altKey: event.altKey,
    defaultPrevented: event.defaultPrevented
  }
}
