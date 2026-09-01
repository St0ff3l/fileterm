import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { SearchAddon } from '@xterm/addon-search'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import { WebLinksAddon } from '@xterm/addon-web-links'
import type { TerminalZoomOperation } from '@fileterm/core'
import { isClinkAutosuggestHelpUrl } from '../app/terminal-transcript'
import { t } from '../i18n'
import { getConfiguredMonoFontFamily } from '../app/font-metrics'
import { getTerminalLogColorPalette, TerminalLogColorizer } from '../app/terminal-log-colorizer'
import {
  getTerminalFontSize,
  setTerminalFontSize,
  TERMINAL_DEFAULT_FONT_SIZE,
  TERMINAL_MIN_FONT_SIZE,
  TERMINAL_MAX_FONT_SIZE
} from '../app/terminal-font-size-store'
import { logTerminalZoom } from './terminal-view-utils'
import type { TerminalLifecycleOptions, TerminalLifecycleRuntime } from './terminal-lifecycle-types'

let lastFocusedTerminal: Terminal | null = null
let terminalUnderPointer: Terminal | null = null

export function clearTerminalGlobalState(terminal: Terminal) {
  if (lastFocusedTerminal === terminal) {
    lastFocusedTerminal = null
  }
  if (terminalUnderPointer === terminal) {
    terminalUnderPointer = null
  }
}

export function createTerminalLifecycleRuntime(options: TerminalLifecycleOptions): TerminalLifecycleRuntime | null {
  const host = options.hostRef.current
  if (!host) {
    return null
  }

  const { isMac, isWindowsPty } = options
  const terminal = new Terminal({
    fontFamily: getConfiguredMonoFontFamily(),
    fontSize: getTerminalFontSize(options.profileIdRef.current),
    letterSpacing: 0.5,
    lineHeight: 1.05,
    cursorBlink: true,
    cursorStyle: 'bar',
    cursorWidth: 2,
    allowProposedApi: true,
    allowTransparency: true,
    // Vim enables mouse reporting, which disables xterm's normal selection
    // service. On macOS, Option+drag is the standard xterm escape hatch for
    // making a local selection without sending that drag to Vim.
    macOptionClickForcesSelection: true,
    reflowCursorLine: false,
    scrollback: 6000,
    // FileTerm's local Windows sessions use ConPTY. xterm.js needs this
    // hint to preserve the screen rows that ConPTY reprints during a
    // resize, especially when a kept-alive tab becomes visible again.
    windowsPty: isWindowsPty ? { backend: 'conpty' } : {},
    // Keep ED2 from moving an interactive TUI's cleared screen into the
    // scrollback. This is the behavior expected by modern TUIs and keeps the
    // viewport stable when a TUI repaints after tab/size changes.
    scrollOnEraseInDisplay: false,
    overviewRuler: { width: 0 },
    linkHandler: {
      activate: (_event, uri) => {
        if (!isClinkAutosuggestHelpUrl(uri)) {
          void window.fileterm?.openExternalUrl(uri)
        }
      }
    },
    theme: options.buildTerminalTheme()
  })
  const fitAddon = new FitAddon()
  options.fitAddonRef.current = fitAddon
  const searchAddon = new SearchAddon({ highlightLimit: 2000 })
  const unicode11Addon = new Unicode11Addon()
  const webLinksAddon = new WebLinksAddon((_event, uri) => {
    if (!isClinkAutosuggestHelpUrl(uri)) {
      void window.fileterm?.openExternalUrl(uri)
    }
  })
  terminal.loadAddon(fitAddon)
  terminal.loadAddon(searchAddon)
  terminal.loadAddon(unicode11Addon)
  terminal.loadAddon(webLinksAddon)
  terminal.unicode.activeVersion = '11'
  terminal.open(host)
  options.terminalRef.current = terminal
  options.searchAddonRef.current = searchAddon
  const terminalLogColorizer = new TerminalLogColorizer(terminal, getTerminalLogColorPalette(terminal.options.theme))
  options.terminalLogColorizerRef.current = terminalLogColorizer
  const xtermScrollableElement = host.querySelector('.xterm-scrollable-element') as HTMLElement | null
  options.setTerminalScrollableElement(xtermScrollableElement ?? terminal.element ?? null)
  const terminalTextarea = terminal.textarea

  const onCompositionStart = () => {
    options.imeCompositionActiveRef.current = true
    options.imeCompositionTextRef.current = ''
    options.imeCompositionEndedAtRef.current = 0
    options.imeCompositionDataForwardedRef.current = false
  }
  const onCompositionUpdate = (event: CompositionEvent) => {
    options.imeCompositionTextRef.current = event.data
  }
  const onCompositionEnd = (event: CompositionEvent) => {
    options.imeCompositionActiveRef.current = false
    // xterm also avoids relying on CompositionEvent.data because browsers
    // may report only the last candidate fragment there. The latest update
    // is the better signal; event.data is only a fallback for IMEs that do
    // not emit compositionupdate.
    const compositionText = options.imeCompositionTextRef.current || event.data
    options.imeCompositionTextRef.current = compositionText
    options.imeCompositionEndedAtRef.current = compositionText ? Date.now() : 0
    options.imeCompositionDataForwardedRef.current = false
  }
  terminalTextarea?.addEventListener('compositionstart', onCompositionStart)
  terminalTextarea?.addEventListener('compositionupdate', onCompositionUpdate)
  terminalTextarea?.addEventListener('compositionend', onCompositionEnd)

  const adjustTerminalFontSize = (change: number) => {
    if (options.terminalZoomLockedRef.current) {
      logTerminalZoom(terminal, 'font-size-ignored-locked', { change, tabId: options.tabIdRef.current })
      return false
    }

    const currentSize = terminal.options.fontSize ?? TERMINAL_DEFAULT_FONT_SIZE
    const nextSize = Math.max(TERMINAL_MIN_FONT_SIZE, Math.min(TERMINAL_MAX_FONT_SIZE, currentSize + change))
    logTerminalZoom(terminal, 'font-size-requested', {
      change,
      currentSize,
      nextSize,
      tabId: options.tabIdRef.current
    })
    if (nextSize === currentSize) {
      logTerminalZoom(terminal, 'font-size-unchanged-at-boundary', { tabId: options.tabIdRef.current })
      return false
    }

    setTerminalFontSize(options.profileIdRef.current, nextSize)
    options.applyTerminalFontSize(nextSize)
    logTerminalZoom(terminal, 'font-size-applied', {
      currentSize,
      nextSize: terminal.options.fontSize,
      cols: terminal.cols,
      rows: terminal.rows,
      tabId: options.tabIdRef.current
    })
    return true
  }
  const resetTerminalFontSize = () =>
    adjustTerminalFontSize(TERMINAL_DEFAULT_FONT_SIZE - (terminal.options.fontSize ?? TERMINAL_DEFAULT_FONT_SIZE))
  const applyTerminalZoom = (operation: TerminalZoomOperation, source: 'menu' | 'shortcut' | 'gesture', steps = 1) => {
    const changed =
      operation === 'reset' ? resetTerminalFontSize() : adjustTerminalFontSize((operation === 'in' ? 1 : -1) * steps)
    logTerminalZoom(terminal, `${source}-${operation}`, { changed, steps })
    return changed
  }

  const markTerminalFocused = () => {
    if (!options.isActiveRef.current) {
      return
    }
    lastFocusedTerminal = terminal
    logTerminalZoom(terminal, 'terminal-focused', { tabId: options.tabIdRef.current })
  }
  const markTerminalUnderPointer = () => {
    if (!options.isActiveRef.current) {
      return
    }
    terminalUnderPointer = terminal
    logTerminalZoom(terminal, 'terminal-pointer-entered', { tabId: options.tabIdRef.current })
  }
  const clearTerminalUnderPointer = () => {
    if (terminalUnderPointer !== terminal) {
      return
    }

    terminalUnderPointer = null
    logTerminalZoom(terminal, 'terminal-pointer-left', { tabId: options.tabIdRef.current })
  }
  terminalTextarea?.addEventListener('focus', markTerminalFocused)

  const writeReconnectHint = () => {
    if (!options.onReconnectRef.current || options.reconnectHintShownRef.current) {
      return
    }

    options.reconnectHintShownRef.current = true
    terminal.write(`\r\n${options.reconnectHintRef.current}\r\n`)
  }
  const handleTerminalWriteFailure = (error: unknown) => {
    if (String(error).includes('serial transfer active')) {
      return
    }
    if (options.inputSendFailedRef.current) {
      return
    }
    options.inputSendFailedRef.current = true
    // 一次性写入 devtools 控制台，便于和后端 app.log 里的 panic 行交叉
    // 定位（后端 panic hook 写 scope=panic，前端这里写 tab id）。
    console.warn(
      `[TerminalView] writeTerminal rejected for tab ${options.tabIdRef.current}; worker likely dead, degrading to disconnected state`,
      error
    )
    // Do not wait for the backend's terminal:state broadcast before
    // allowing Enter to use the reconnect path. A dead worker cannot
    // consume another input packet, so retaining `connected=true` here
    // would make the first retry look swallowed.
    options.connectedRef.current = false
    options.wasConnectedRef.current = false
    terminal.write(`\r\n${options.closedMessageRef.current}\r\n`)
    writeReconnectHint()
  }
  const writeTerminalInput = (data: string) => {
    window.fileterm?.writeTerminal(options.tabIdRef.current, data)?.catch(handleTerminalWriteFailure)
  }
  const requestReconnect = () => {
    if (options.wasConnectedRef.current || options.connectingRef.current || options.isReconnectingRef.current) {
      return false
    }

    const reconnect = options.onReconnectRef.current
    if (!reconnect) {
      return false
    }

    options.isReconnectingRef.current = true
    options.reconnectHintShownRef.current = false
    // Give immediate feedback before the IPC call starts. The connection
    // result may arrive seconds later, so waiting for terminal:state makes
    // a successful Enter look like a swallowed keypress.
    terminal.write(`\r\n${t.terminalReconnecting}\r\n`)
    void Promise.resolve(reconnect())
      .catch((cause) => {
        const message = cause instanceof Error ? cause.message : String(cause)
        terminal.write(`\r\n${t.connectionFailedPrefix}${message}\r\n`)
        writeReconnectHint()
      })
      .finally(() => {
        if (!options.wasConnectedRef.current) {
          options.isReconnectingRef.current = false
        }
      })
    return true
  }
  const getTerminalZoomOperation = (event: KeyboardEvent): TerminalZoomOperation | null => {
    const isZoomInKey = event.key === '+' || event.key === '=' || event.code === 'Equal' || event.code === 'NumpadAdd'
    const isZoomOutKey =
      event.key === '-' || event.key === '_' || event.code === 'Minus' || event.code === 'NumpadSubtract'
    const isZoomResetKey = event.code === 'Digit0' || event.code === 'Numpad0' || event.key === '0' || event.key === ')'

    // Keep app/window zoom out of the way of remote programs. Cmd± on macOS
    // and Ctrl+Shift± on Windows/Linux adjust only this terminal.
    const matchesZoomIn = isMac
      ? event.metaKey && !event.ctrlKey && !event.altKey && isZoomInKey
      : event.ctrlKey && event.shiftKey && !event.altKey && isZoomInKey
    const matchesZoomOut = isMac
      ? event.metaKey && !event.ctrlKey && !event.altKey && isZoomOutKey
      : event.ctrlKey && event.shiftKey && !event.altKey && isZoomOutKey
    // Keep the browser-standard Cmd+0 reset on macOS. Windows/Linux use
    // Ctrl+0: Ctrl+Shift+0 is intercepted by Windows IME/layout switching
    // before WebView2 ever forwards the physical zero key. Some keyboard
    // layouts report Shift+0 as ')', so always prefer code too.
    const matchesZoomReset = isMac
      ? event.metaKey && !event.ctrlKey && !event.altKey && isZoomResetKey
      : event.ctrlKey && !event.altKey && isZoomResetKey

    return matchesZoomReset ? 'reset' : matchesZoomIn ? 'in' : matchesZoomOut ? 'out' : null
  }

  return {
    ...options,
    terminal,
    fitAddon,
    searchAddon,
    terminalTextarea: terminalTextarea ?? null,
    terminalLogColorizer,
    adjustTerminalFontSize,
    applyTerminalZoom,
    markTerminalFocused,
    markTerminalUnderPointer,
    clearTerminalUnderPointer,
    writeReconnectHint,
    writeTerminalInput,
    requestReconnect,
    getTerminalZoomOperation,
    isLastFocusedTerminal: () => lastFocusedTerminal === terminal,
    isTerminalUnderPointer: () => terminalUnderPointer === terminal,
    clearGlobalTerminalState: () => clearTerminalGlobalState(terminal),
    disposeCore: () => {
      terminalTextarea?.removeEventListener('focus', markTerminalFocused)
      terminalTextarea?.removeEventListener('compositionstart', onCompositionStart)
      terminalTextarea?.removeEventListener('compositionupdate', onCompositionUpdate)
      terminalTextarea?.removeEventListener('compositionend', onCompositionEnd)
      terminalLogColorizer.dispose()
      options.terminalLogColorizerRef.current = null
      options.fitAddonRef.current = null
      options.searchAddonRef.current = null
      options.setTerminalScrollableElement(null)
      options.terminalRef.current = null
      clearTerminalGlobalState(terminal)
      terminal.dispose()
    }
  }
}
