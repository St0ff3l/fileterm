import { registerTerminalDiagnostics } from '../xterm-adapters/terminal-diagnostics'
import { getTerminalCompatibilityOptions } from '../xterm-adapters/terminal-options'
import { Terminal, type IDisposable } from '@xterm/xterm'
import { disposeTerminal, releaseWebglAddon, type WebglAddonRuntime } from '../xterm-adapters/terminal-disposal'
import { FitAddon } from '@xterm/addon-fit'
import { SearchAddon } from '@xterm/addon-search'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import { WebLinksAddon } from '@xterm/addon-web-links'
import type { TerminalZoomOperation } from '@fileterm/core'
import { isClinkAutosuggestHelpUrl } from '../app/terminal-transcript'
import { registerTerminalEmojiWidths } from '../xterm-adapters/terminal-emoji-widths'
import { t } from '../i18n'
import {
  getConfiguredMonoFontFamily,
  getTerminalFontBoldWeight,
  getTerminalFontWeight,
  getTerminalLineHeight
} from '../app/font-metrics'
import { getTerminalRendererPreference, supportsTerminalRenderer } from '../app/terminal-renderer-preference'
import { getTerminalLogColorPalette, TerminalLogColorizer } from '../xterm-adapters/terminal-log-colorizer'
import { registerTerminalForegroundAdapter } from '../xterm-adapters/terminal-foreground-adapter'
import { registerTerminalRenderCacheLimits } from '../xterm-adapters/terminal-render-cache'
import { registerTerminalWebglGrid } from '../xterm-adapters/terminal-webgl-grid'
import { scaleTerminalFontSizeForUiZoom } from '../app/ui-zoom'
import {
  getTerminalFontSize,
  setTerminalFontSize,
  TERMINAL_DEFAULT_FONT_SIZE,
  TERMINAL_MIN_FONT_SIZE,
  TERMINAL_MAX_FONT_SIZE
} from '../app/terminal-font-size-store'
import { logTerminalZoom } from './terminal-view-utils'
import { registerTerminalBlockGlyphRenderer } from '../xterm-adapters/terminal-block-glyph-renderer'
import { registerTerminalSelectionRenderer } from '../xterm-adapters/terminal-selection-renderer'
import { registerTerminalSymbolGlyphRenderer } from '../xterm-adapters/terminal-symbol-glyph-renderer'
import { createTerminalInputWriteQueue } from './terminal-input-write-queue'
import type { TerminalLifecycleOptions, TerminalLifecycleRuntime } from './terminal-lifecycle-types'
import { DIAGNOSTIC_SCOPES, writeDiagnosticLog } from '../lib/diagnostic-log'

type TerminalRendererView = Terminal & {
  _core?: { _renderService?: { _renderer?: { value?: unknown } } }
}

function getWebglDetails(addon: WebglAddonRuntime) {
  const gl = addon._renderer?._gl
  if (!gl || gl.isContextLost()) return { gpuVendor: null, gpuRenderer: null, hardwareAcceleration: null }
  const debug = gl.getExtension('WEBGL_debug_renderer_info')
  const gpuRenderer = debug ? String(gl.getParameter(debug.UNMASKED_RENDERER_WEBGL)) : null
  const gpuVendor = debug ? String(gl.getParameter(debug.UNMASKED_VENDOR_WEBGL)) : null
  const hardwareAcceleration = gpuRenderer
    ? !/swiftshader|llvmpipe|software|warp|basic render driver/i.test(gpuRenderer)
    : null
  return { gpuVendor, gpuRenderer, hardwareAcceleration }
}

function logTerminalRenderer(renderer: 'dom' | 'webgl', reason: string, tabId: string, addon?: WebglAddonRuntime) {
  const browser = {
    chromiumVersion: navigator.userAgent.match(/Chrome\/([\d.]+)/)?.[1] ?? null,
    edgeVersion: navigator.userAgent.match(/Edg\/([\d.]+)/)?.[1] ?? null
  }
  const gpu = addon ? getWebglDetails(addon) : null
  writeDiagnosticLog(
    renderer === 'dom' && reason !== 'preference' && reason !== 'webgl-loading' ? 'WARN' : 'INFO',
    DIAGNOSTIC_SCOPES.performance,
    JSON.stringify({ event: 'terminal-renderer', tabId, renderer, reason, browser, ...gpu })
  )
}

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
    ...getTerminalCompatibilityOptions(isWindowsPty),
    fontFamily: getConfiguredMonoFontFamily(),
    fontSize: scaleTerminalFontSizeForUiZoom(getTerminalFontSize(options.profileIdRef.current)),
    fontWeight: getTerminalFontWeight(),
    fontWeightBold: getTerminalFontBoldWeight(),
    lineHeight: getTerminalLineHeight(),
    cursorBlink: true,
    cursorStyle: 'bar',
    cursorWidth: 2,
    allowProposedApi: true,
    allowTransparency: true,
    scrollback: 6000,
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
  const emojiWidths = registerTerminalEmojiWidths(terminal)
  terminal.open(host)
  const terminalDiagnostics = registerTerminalDiagnostics(
    terminal,
    options.pendingWriteRef,
    options.renderedTranscriptRef,
    options.tabIdRef
  )
  let renderCacheLimits = registerTerminalRenderCacheLimits(terminal)
  let domRendererAdapters: IDisposable[] = []
  let webglAddon: WebglAddonRuntime | null = null
  let webglContextLoss: IDisposable | null = null
  let webglGrid: IDisposable | null = null
  let disposed = false
  const enableDomRendererAdapters = () => {
    if (domRendererAdapters.length) return
    domRendererAdapters = [
      registerTerminalSelectionRenderer(terminal),
      registerTerminalSymbolGlyphRenderer(terminal),
      registerTerminalBlockGlyphRenderer(terminal)
    ]
  }
  const disableDomRendererAdapters = () => {
    domRendererAdapters
      .splice(0)
      .reverse()
      .forEach((adapter) => adapter.dispose())
  }
  const refreshRendererAdapters = () => {
    renderCacheLimits.dispose()
    renderCacheLimits = registerTerminalRenderCacheLimits(terminal)
    terminalDiagnostics.refreshRenderer()
    terminal.refresh(0, Math.max(0, terminal.rows - 1))
  }
  const fitRendererGrid = () => {
    if (host.clientWidth > 0 && host.clientHeight > 0) fitAddon.fit()
  }
  const fallbackToDom = (reason: 'load-failed' | 'activation-failed' | 'context-loss') => {
    const addon = webglAddon
    webglAddon = null
    webglContextLoss?.dispose()
    webglContextLoss = null
    webglGrid?.dispose()
    webglGrid = null
    if (addon) {
      try {
        releaseWebglAddon(addon)
      } catch {
        // An addon teardown failure must not prevent the DOM fallback.
      }
    }
    if (disposed) return
    enableDomRendererAdapters()
    refreshRendererAdapters()
    fitRendererGrid()
    logTerminalRenderer('dom', reason, options.tabIdRef.current)
  }
  const rendererOverride = import.meta.env.DEV ? import.meta.env.VITE_FILETERM_TERMINAL_RENDERER : undefined
  const requestedRenderer =
    rendererOverride === 'dom' || rendererOverride === 'webgl' ? rendererOverride : getTerminalRendererPreference()
  const shouldUseWebgl = supportsTerminalRenderer(window.fileterm?.platform) && requestedRenderer === 'webgl'
  if (shouldUseWebgl) {
    enableDomRendererAdapters()
    logTerminalRenderer('dom', 'webgl-loading', options.tabIdRef.current)
    void import('@xterm/addon-webgl')
      .then(({ WebglAddon: WebglAddonConstructor }) => {
        if (disposed) return
        const addon = new WebglAddonConstructor() as WebglAddonRuntime
        disableDomRendererAdapters()
        try {
          terminal.loadAddon(addon)
          const activeRenderer = (terminal as TerminalRendererView)._core?._renderService?._renderer?.value
          if (activeRenderer !== addon._renderer || !addon._renderer?._gl || addon._renderer._gl.isContextLost()) {
            throw new Error('renderer-not-active')
          }
          webglAddon = addon
          webglGrid = registerTerminalWebglGrid(terminal)
          webglContextLoss = addon.onContextLoss(() => fallbackToDom('context-loss'))
          refreshRendererAdapters()
          fitRendererGrid()
          logTerminalRenderer('webgl', 'active', options.tabIdRef.current, addon)
        } catch {
          webglContextLoss?.dispose()
          webglContextLoss = null
          webglGrid?.dispose()
          webglGrid = null
          webglAddon = null
          try {
            releaseWebglAddon(addon)
          } catch {
            // The original DOM renderer remains the safe fallback if activation is partial.
          }
          enableDomRendererAdapters()
          refreshRendererAdapters()
          fitRendererGrid()
          logTerminalRenderer('dom', 'activation-failed', options.tabIdRef.current)
        }
      })
      .catch(() => fallbackToDom('load-failed'))
  } else {
    enableDomRendererAdapters()
    if (import.meta.env.DEV) logTerminalRenderer('dom', 'preference', options.tabIdRef.current)
  }
  options.terminalRef.current = terminal
  options.searchAddonRef.current = searchAddon
  const foregroundAdapter = registerTerminalForegroundAdapter(terminal)
  const terminalLogColorizer = new TerminalLogColorizer(terminal, getTerminalLogColorPalette(terminal.options.theme))
  options.terminalLogColorizerRef.current = terminalLogColorizer
  const xtermScrollableElement = host.querySelector('.xterm-scrollable-element') as HTMLElement | null
  options.setTerminalScrollableElement(xtermScrollableElement ?? terminal.element ?? null)
  const terminalTextarea = terminal.textarea

  const adjustTerminalFontSize = (change: number) => {
    if (options.terminalZoomLockedRef.current) {
      logTerminalZoom(terminal, 'font-size-ignored-locked', { change, tabId: options.tabIdRef.current })
      return false
    }

    const currentSize = getTerminalFontSize(options.profileIdRef.current)
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
    adjustTerminalFontSize(TERMINAL_DEFAULT_FONT_SIZE - getTerminalFontSize(options.profileIdRef.current))
  const applyTerminalZoom = (operation: TerminalZoomOperation, source: 'menu' | 'shortcut' | 'gesture', steps = 1) => {
    const changed =
      operation === 'reset' ? resetTerminalFontSize() : adjustTerminalFontSize((operation === 'in' ? 1 : -1) * steps)
    logTerminalZoom(terminal, `${source}-${operation}`, { changed, steps })
    return changed
  }

  const markTerminalFocused = () => {
    if (disposed || !options.isActiveRef.current) {
      return
    }
    lastFocusedTerminal = terminal
    logTerminalZoom(terminal, 'terminal-focused', { tabId: options.tabIdRef.current })
  }
  const markTerminalUnderPointer = () => {
    if (disposed || !options.isActiveRef.current) {
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
    if (disposed || !options.onReconnectRef.current || options.reconnectHintShownRef.current) {
      return
    }

    options.reconnectHintShownRef.current = true
    terminal.write(`\r\n${options.reconnectHintRef.current}\r\n`)
  }
  let lastInputBacklogWarning = -Infinity
  const handleTerminalWriteFailure = (error: unknown) => {
    if (disposed) return
    if (String(error).includes('Terminal input backlog full')) {
      if (Date.now() - lastInputBacklogWarning >= 5000) {
        lastInputBacklogWarning = Date.now()
        terminal.write('\r\n[FileTerm] Terminal input backlog full; input was not sent.\r\n')
      }
      return
    }
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
  const terminalInputWriteQueue = createTerminalInputWriteQueue({
    getTabId: () => options.tabIdRef.current,
    write: (tabId, data) => window.fileterm?.writeTerminal(tabId, data),
    shouldStop: () => options.inputSendFailedRef.current
  })
  const writeTerminalInput = (data: string, onFailure = handleTerminalWriteFailure) => {
    void terminalInputWriteQueue.write(data, onFailure)
  }
  const requestReconnect = () => {
    if (
      disposed ||
      options.wasConnectedRef.current ||
      options.connectingRef.current ||
      options.isReconnectingRef.current
    ) {
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
    void Promise.resolve()
      .then(() => {
        if (!disposed) return reconnect()
      })
      .catch((cause) => {
        if (disposed) return
        const message = cause instanceof Error ? cause.message : String(cause)
        terminal.write(`\r\n${t.connectionFailedPrefix}${message}\r\n`)
        writeReconnectHint()
      })
      .finally(() => {
        if (!disposed && !options.wasConnectedRef.current) {
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
      if (disposed) return
      disposed = true
      webglContextLoss?.dispose()
      webglContextLoss = null
      webglGrid?.dispose()
      webglGrid = null
      if (webglAddon) {
        try {
          releaseWebglAddon(webglAddon)
        } catch {
          // Terminal disposal must continue even if the experimental renderer fails to tear down.
        }
        webglAddon = null
      }
      terminalDiagnostics.dispose()
      terminalInputWriteQueue.dispose()
      terminalTextarea?.removeEventListener('focus', markTerminalFocused)
      terminalLogColorizer.dispose()
      renderCacheLimits.dispose()
      disableDomRendererAdapters()
      emojiWidths.dispose()
      foregroundAdapter.dispose()
      options.terminalLogColorizerRef.current = null
      options.fitAddonRef.current = null
      options.searchAddonRef.current = null
      options.setTerminalScrollableElement(null)
      options.terminalRef.current = null
      clearTerminalGlobalState(terminal)
      disposeTerminal(terminal)
    }
  }
}
