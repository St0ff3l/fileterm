import { useEffect, type Dispatch, type SetStateAction } from 'react'
import { Terminal, type ITheme } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { SearchAddon } from '@xterm/addon-search'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import { WebLinksAddon } from '@xterm/addon-web-links'
import type { TerminalZoomOperation } from '@fileterm/core'
import { copyText } from '../app/app-utils'
import { isClinkAutosuggestHelpUrl, trimHydratedTerminalChunk } from '../app/terminal-transcript'
import { APP_EVENT, onAppEvent } from '../lib/app-events'
import { t } from '../i18n'
import { getConfiguredMonoFontFamily, observeCanvasTextMetrics } from '../app/font-metrics'
import { getTerminalLogColorPalette, TerminalLogColorizer } from '../app/terminal-log-colorizer'
import {
  getTerminalFontSize,
  setTerminalFontSize,
  TERMINAL_DEFAULT_FONT_SIZE,
  TERMINAL_MIN_FONT_SIZE,
  TERMINAL_MAX_FONT_SIZE
} from '../app/terminal-font-size-store'
import {
  decodeBase64Utf8,
  describeTerminalInput,
  encodeBase64Utf8,
  getLastVisibleTerminalLine,
  getVimVisualSelection,
  isFocusTrackingSequence,
  isOsc52TargetSupported,
  logTerminalClipboard,
  logTerminalZoom,
  looksLikeShellPrompt,
  localizeTerminalText,
  PinchGestureEvent,
  splitOscPayload,
  stripHydratedTerminalResponses,
  TERMINAL_GESTURE_ZOOM_THRESHOLD,
  TERMINAL_IME_DUPLICATE_WINDOW_MS,
  TERMINAL_RESIZE_OUTPUT_QUIET_MS,
  TERMINAL_RESIZE_PIXEL_EPSILON,
  TERMINAL_RESIZE_SETTLE_MS,
  TERMINAL_WHEEL_ZOOM_THRESHOLD
} from './terminal-view-utils'

type MutableRef<T> = { current: T }
type TerminalSize = { cols: number; rows: number; width: number; height: number }
type TerminalHostRect = { left: number; top: number; right: number; bottom: number; width: number; height: number }
type TerminalResizeOptions = {
  force?: boolean
  freezeCols?: boolean
  preserveVisibleBuffer?: boolean
}

type TerminalLifecycleOptions = {
  isMac: boolean
  isWin: boolean
  bootText: string
  hostRef: MutableRef<HTMLDivElement | null>
  setViewportElement(value: HTMLElement | null): void
  terminalRef: MutableRef<Terminal | null>
  fitAddonRef: MutableRef<FitAddon | null>
  terminalLogColorizerRef: MutableRef<TerminalLogColorizer | null>
  searchAddonRef: MutableRef<SearchAddon | null>
  bootTextRef: MutableRef<string>
  renderedTranscriptRef: MutableRef<string>
  pendingWriteRef: MutableRef<string>
  writeFrameRef: MutableRef<number | null>
  resizeTimerRef: MutableRef<number | null>
  resizeSettleTimerRef: MutableRef<number | null>
  pendingResizeForceRef: MutableRef<boolean>
  pendingResizeFreezeColsRef: MutableRef<boolean>
  isWritingRef: MutableRef<boolean>
  suppressHydratedChunksUntilRef: MutableRef<number>
  preserveVisibleBufferRef: MutableRef<boolean>
  bootedTabs: MutableRef<Set<string>>
  wasConnectedRef: MutableRef<boolean>
  inputSendFailedRef: MutableRef<boolean>
  imeCompositionActiveRef: MutableRef<boolean>
  imeCompositionTextRef: MutableRef<string>
  imeCompositionEndedAtRef: MutableRef<number>
  imeCompositionDataForwardedRef: MutableRef<boolean>
  replayingTranscriptRef: MutableRef<boolean>
  transcriptReplayGenerationRef: MutableRef<number>
  connectedRef: MutableRef<boolean>
  connectingRef: MutableRef<boolean>
  lastSyncedSizeRef: MutableRef<TerminalSize | null>
  lastObservedHostRectRef: MutableRef<TerminalHostRect | null>
  isHorizontalResizeActiveRef: MutableRef<boolean>
  lastTerminalOutputAtRef: MutableRef<number>
  awaitingCommandCompletionRef: MutableRef<boolean>
  pendingPromptResizeRef: MutableRef<boolean>
  profileIdRef: MutableRef<string>
  tabIdRef: MutableRef<string>
  onStatusRef: MutableRef<((message: string | null) => void) | undefined>
  onReconnectRef: MutableRef<(() => void | Promise<void>) | undefined>
  closedMessageRef: MutableRef<string>
  reconnectHintRef: MutableRef<string>
  canClosePaneRef: MutableRef<boolean>
  isReconnectingRef: MutableRef<boolean>
  reconnectHintShownRef: MutableRef<boolean>
  activeTerminalTabIdRef: MutableRef<string | null>
  findOpenRef: MutableRef<boolean>
  terminalZoomLockedRef: MutableRef<boolean>
  setHasSelection: Dispatch<SetStateAction<boolean>>
  setContextMenu: Dispatch<SetStateAction<{ x: number; y: number } | null>>
  setFindMatchCount: Dispatch<SetStateAction<number>>
  setActiveFindIndex: Dispatch<SetStateAction<number>>
  setFindMiss: Dispatch<SetStateAction<boolean>>
  buildTerminalTheme(): ITheme
  applyTerminalFontSize(fontSize: number): boolean
  syncTerminalSize(fitAddon: FitAddon, terminal: Terminal, options?: TerminalResizeOptions): void
  replaceTerminalWithTranscript(terminal: Terminal, transcript: string): void
  shouldHydrateTranscript(currentTranscript: string, nextTranscript: string, connected: boolean): boolean
  formatTerminalChunk(terminal: Terminal | null, value: string): string
  appendRenderedTranscript(chunk: string): void
  scheduleTerminalWrite(text: string): void
  buildExitAlternateScreenSequence(): string
  snapshotTerminalBuffer(terminal: Terminal): string
  clearSearchDecorations(): void
  clearEphemeralHighlight(): void
  runCopy(): void
  runPaste(): Promise<void>
  openFind(): void
  closeFind(): void
  runClear(): void
  runSplitPane(direction: 'row' | 'column'): void
  runClosePane(): void
  runCloseTab(): void
}

let lastFocusedTerminal: Terminal | null = null
let terminalUnderPointer: Terminal | null = null

export function useTerminalLifecycle({
  isMac,
  isWin,
  bootText,
  hostRef,
  setViewportElement,
  terminalRef,
  fitAddonRef,
  terminalLogColorizerRef,
  searchAddonRef,
  bootTextRef,
  renderedTranscriptRef,
  pendingWriteRef,
  writeFrameRef,
  resizeTimerRef,
  resizeSettleTimerRef,
  pendingResizeForceRef,
  pendingResizeFreezeColsRef,
  isWritingRef,
  suppressHydratedChunksUntilRef,
  preserveVisibleBufferRef,
  bootedTabs,
  wasConnectedRef,
  inputSendFailedRef,
  imeCompositionActiveRef,
  imeCompositionTextRef,
  imeCompositionEndedAtRef,
  imeCompositionDataForwardedRef,
  replayingTranscriptRef,
  transcriptReplayGenerationRef,
  connectedRef,
  connectingRef,
  lastSyncedSizeRef,
  lastObservedHostRectRef,
  isHorizontalResizeActiveRef,
  lastTerminalOutputAtRef,
  awaitingCommandCompletionRef,
  pendingPromptResizeRef,
  profileIdRef,
  tabIdRef,
  onStatusRef,
  onReconnectRef,
  closedMessageRef,
  reconnectHintRef,
  canClosePaneRef,
  isReconnectingRef,
  reconnectHintShownRef,
  activeTerminalTabIdRef,
  findOpenRef,
  terminalZoomLockedRef,
  setHasSelection,
  setContextMenu,
  setFindMatchCount,
  setActiveFindIndex,
  setFindMiss,
  buildTerminalTheme,
  applyTerminalFontSize,
  syncTerminalSize,
  replaceTerminalWithTranscript,
  shouldHydrateTranscript,
  formatTerminalChunk,
  appendRenderedTranscript,
  scheduleTerminalWrite,
  buildExitAlternateScreenSequence,
  snapshotTerminalBuffer,
  clearSearchDecorations,
  clearEphemeralHighlight,
  runCopy,
  runPaste,
  openFind,
  closeFind,
  runClear,
  runSplitPane,
  runClosePane,
  runCloseTab
}: TerminalLifecycleOptions) {
  useEffect(() => {
    if (!hostRef.current) {
      return
    }

    const terminal = new Terminal({
      fontFamily: getConfiguredMonoFontFamily(),
      fontSize: getTerminalFontSize(profileIdRef.current),
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
      overviewRuler: { width: 0 },
      linkHandler: {
        activate: (_event, uri) => {
          if (!isClinkAutosuggestHelpUrl(uri)) {
            void window.fileterm?.openExternalUrl(uri)
          }
        }
      },
      theme: buildTerminalTheme()
    })
    const fitAddon = new FitAddon()
    fitAddonRef.current = fitAddon
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
    terminal.open(hostRef.current)
    const terminalLogColorizer = new TerminalLogColorizer(terminal, getTerminalLogColorPalette(terminal.options.theme))
    terminalLogColorizerRef.current = terminalLogColorizer
    const xtermViewport = hostRef.current.querySelector('.xterm-viewport') as HTMLElement | null
    if (xtermViewport) {
      setViewportElement(xtermViewport)
    }
    const terminalTextarea = terminal.textarea
    const onCompositionStart = () => {
      imeCompositionActiveRef.current = true
      imeCompositionTextRef.current = ''
      imeCompositionEndedAtRef.current = 0
      imeCompositionDataForwardedRef.current = false
    }
    const onCompositionUpdate = (event: CompositionEvent) => {
      imeCompositionTextRef.current = event.data
    }
    const onCompositionEnd = (event: CompositionEvent) => {
      imeCompositionActiveRef.current = false
      // xterm also avoids relying on CompositionEvent.data because browsers
      // may report only the last candidate fragment there. The latest update
      // is the better signal; event.data is only a fallback for IMEs that do
      // not emit compositionupdate.
      const compositionText = imeCompositionTextRef.current || event.data
      imeCompositionTextRef.current = compositionText
      imeCompositionEndedAtRef.current = compositionText ? Date.now() : 0
      imeCompositionDataForwardedRef.current = false
    }
    terminalTextarea?.addEventListener('compositionstart', onCompositionStart)
    terminalTextarea?.addEventListener('compositionupdate', onCompositionUpdate)
    terminalTextarea?.addEventListener('compositionend', onCompositionEnd)
    const adjustTerminalFontSize = (change: number) => {
      if (terminalZoomLockedRef.current) {
        logTerminalZoom(terminal, 'font-size-ignored-locked', { change, tabId: tabIdRef.current })
        return false
      }

      const currentSize = terminal.options.fontSize ?? TERMINAL_DEFAULT_FONT_SIZE
      const nextSize = Math.max(TERMINAL_MIN_FONT_SIZE, Math.min(TERMINAL_MAX_FONT_SIZE, currentSize + change))
      logTerminalZoom(terminal, 'font-size-requested', { change, currentSize, nextSize, tabId: tabIdRef.current })
      if (nextSize === currentSize) {
        logTerminalZoom(terminal, 'font-size-unchanged-at-boundary', { tabId: tabIdRef.current })
        return false
      }

      setTerminalFontSize(profileIdRef.current, nextSize)
      applyTerminalFontSize(nextSize)
      logTerminalZoom(terminal, 'font-size-applied', {
        currentSize,
        nextSize: terminal.options.fontSize,
        cols: terminal.cols,
        rows: terminal.rows,
        tabId: tabIdRef.current
      })
      return true
    }
    const resetTerminalFontSize = () =>
      adjustTerminalFontSize(TERMINAL_DEFAULT_FONT_SIZE - (terminal.options.fontSize ?? TERMINAL_DEFAULT_FONT_SIZE))
    const applyTerminalZoom = (
      operation: TerminalZoomOperation,
      source: 'menu' | 'shortcut' | 'gesture',
      steps = 1
    ) => {
      const changed =
        operation === 'reset' ? resetTerminalFontSize() : adjustTerminalFontSize((operation === 'in' ? 1 : -1) * steps)
      logTerminalZoom(terminal, `${source}-${operation}`, { changed, steps })
      return changed
    }
    const markTerminalFocused = () => {
      lastFocusedTerminal = terminal
      logTerminalZoom(terminal, 'terminal-focused', { tabId: tabIdRef.current })
    }
    const markTerminalUnderPointer = () => {
      terminalUnderPointer = terminal
      logTerminalZoom(terminal, 'terminal-pointer-entered', { tabId: tabIdRef.current })
    }
    const clearTerminalUnderPointer = () => {
      if (terminalUnderPointer !== terminal) {
        return
      }

      terminalUnderPointer = null
      logTerminalZoom(terminal, 'terminal-pointer-left', { tabId: tabIdRef.current })
    }
    terminalTextarea?.addEventListener('focus', markTerminalFocused)
    const writeReconnectHint = () => {
      if (!onReconnectRef.current || reconnectHintShownRef.current) {
        return
      }

      reconnectHintShownRef.current = true
      terminal.write(`\r\n${reconnectHintRef.current}\r\n`)
    }
    const requestReconnect = () => {
      if (wasConnectedRef.current || connectingRef.current || isReconnectingRef.current) {
        return false
      }

      const reconnect = onReconnectRef.current
      if (!reconnect) {
        return false
      }

      isReconnectingRef.current = true
      reconnectHintShownRef.current = false
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
          if (!wasConnectedRef.current) {
            isReconnectingRef.current = false
          }
        })
      return true
    }
    const getTerminalZoomOperation = (event: KeyboardEvent): TerminalZoomOperation | null => {
      const isZoomInKey = event.key === '+' || event.key === '=' || event.code === 'Equal' || event.code === 'NumpadAdd'
      const isZoomOutKey =
        event.key === '-' || event.key === '_' || event.code === 'Minus' || event.code === 'NumpadSubtract'
      const isZoomResetKey =
        event.code === 'Digit0' || event.code === 'Numpad0' || event.key === '0' || event.key === ')'

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
    terminal.attachCustomKeyEventHandler((event) => {
      if (event.type !== 'keydown') {
        return true
      }

      // Let xterm's CompositionHelper receive IME key events. In particular,
      // keyCode 229 / Process events must not be mistaken for reconnect or
      // terminal shortcuts while a composition is being finalized.
      if (imeCompositionActiveRef.current || event.isComposing || event.keyCode === 229 || event.key === 'Process') {
        return true
      }

      const isHistoryOpen = document.body.getAttribute('data-history-open') === 'true'
      if (
        isHistoryOpen &&
        (event.key === 'ArrowUp' ||
          event.key === 'ArrowDown' ||
          event.key === 'ArrowLeft' ||
          event.key === 'ArrowRight' ||
          event.key === 'Enter' ||
          event.key === 'Escape')
      ) {
        return false
      }

      if (event.key === 'Enter' && requestReconnect()) {
        event.preventDefault()
        event.stopPropagation()
        return false
      }

      const matchesCopy = isMac
        ? event.metaKey && !event.shiftKey && event.key.toLowerCase() === 'c'
        : event.ctrlKey && event.shiftKey && event.key.toLowerCase() === 'c'
      const matchesPaste = isMac
        ? event.metaKey && !event.shiftKey && event.key.toLowerCase() === 'v'
        : event.ctrlKey && event.shiftKey && event.key.toLowerCase() === 'v'
      const matchesFind = isMac
        ? event.metaKey && !event.shiftKey && event.key.toLowerCase() === 'f'
        : event.ctrlKey && !event.shiftKey && event.key.toLowerCase() === 'f'
      const terminalZoomOperation = getTerminalZoomOperation(event)
      if (event.ctrlKey || event.metaKey || terminalZoomOperation) {
        logTerminalZoom(terminal, 'key-xterm-received', {
          ...describeTerminalInput(event),
          operation: terminalZoomOperation,
          tabId: tabIdRef.current
        })
      }

      if (matchesCopy) {
        event.preventDefault()
        event.stopPropagation()
        logTerminalClipboard(terminal, 'copy-shortcut-xterm', { key: event.key })
        runCopy()
        return false
      }

      if (matchesPaste) {
        event.preventDefault()
        event.stopPropagation()
        void runPaste()
        return false
      }

      if (matchesFind) {
        event.preventDefault()
        event.stopPropagation()
        if (findOpenRef.current) {
          closeFind()
        } else {
          openFind()
        }
        return false
      }

      if (terminalZoomOperation) {
        event.preventDefault()
        event.stopPropagation()
        applyTerminalZoom(terminalZoomOperation, 'shortcut')
        return false
      }

      const matchesClose = isMac
        ? event.metaKey && !event.shiftKey && event.key.toLowerCase() === 'w'
        : event.ctrlKey && event.shiftKey && event.key.toLowerCase() === 'w'

      if (matchesClose) {
        event.preventDefault()
        event.stopPropagation()
        if (canClosePaneRef.current) {
          runClosePane()
        } else {
          runCloseTab()
        }
        return false
      }

      const matchesSplitVertical = isMac
        ? event.metaKey && !event.shiftKey && event.key.toLowerCase() === 'd'
        : isWin
          ? (event.altKey &&
              event.shiftKey &&
              (event.key === '+' || event.key === '=' || event.code === 'Equal' || event.code === 'NumpadAdd')) ||
            (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === 'd')
          : event.ctrlKey && event.shiftKey && event.key.toLowerCase() === 'd'

      const matchesSplitHorizontal = isMac
        ? event.metaKey && event.shiftKey && event.key.toLowerCase() === 'd'
        : isWin
          ? (event.altKey &&
              event.shiftKey &&
              (event.key === '-' || event.key === '_' || event.code === 'Minus' || event.code === 'NumpadSubtract')) ||
            (event.ctrlKey && event.altKey && event.shiftKey && event.key.toLowerCase() === 'd')
          : event.ctrlKey && event.altKey && event.shiftKey && event.key.toLowerCase() === 'd'

      if (matchesSplitVertical) {
        event.preventDefault()
        event.stopPropagation()
        runSplitPane('row')
        return false
      }

      if (matchesSplitHorizontal) {
        event.preventDefault()
        event.stopPropagation()
        runSplitPane('column')
        return false
      }

      if (
        event.key === 'Control' ||
        event.key === 'Meta' ||
        event.key === 'Alt' ||
        (event.key === 'Dead' && (event.metaKey || event.ctrlKey || event.altKey))
      ) {
        return false
      }

      return true
    })
    terminalRef.current = terminal
    searchAddonRef.current = searchAddon

    const searchResultsDisposable = searchAddon.onDidChangeResults(({ resultIndex, resultCount }) => {
      setFindMatchCount(resultCount)
      setActiveFindIndex(resultIndex)
      setFindMiss(resultCount === 0)
    })

    const osc52Disposable = terminal.parser.registerOscHandler(52, async (payload) => {
      const parsed = splitOscPayload(payload)
      if (!parsed || !isOsc52TargetSupported(parsed.target)) {
        return false
      }

      if (replayingTranscriptRef.current) {
        return true
      }

      if (parsed.data === '?') {
        let clipboardText = ''
        try {
          clipboardText = window.fileterm?.readClipboardText
            ? await window.fileterm.readClipboardText()
            : ((await navigator.clipboard?.readText?.()) ?? '')
        } catch {
          clipboardText = ''
        }
        const encoded = encodeBase64Utf8(clipboardText)
        await window.fileterm?.writeTerminal(tabIdRef.current, `\u001b]52;${parsed.target || 'c'};${encoded}\u0007`)
        return true
      }

      const decoded = decodeBase64Utf8(parsed.data)
      if (decoded === null) {
        return false
      }

      if (window.fileterm?.writeClipboardText) {
        await window.fileterm.writeClipboardText(decoded)
      } else {
        copyText(decoded)
      }
      return true
    })

    syncTerminalSize(fitAddon, terminal)

    bootTextRef.current = bootText
    if (bootText) {
      replaceTerminalWithTranscript(terminal, bootText)
    }
    activeTerminalTabIdRef.current = tabIdRef.current

    const resize = (force = false, freezeCols = false, preserveVisibleBuffer = false) => {
      syncTerminalSize(fitAddon, terminal, { force, freezeCols, preserveVisibleBuffer })
    }

    const scheduleResize = (force = false, freezeCols = false, preserveVisibleBuffer = false) => {
      pendingResizeForceRef.current = pendingResizeForceRef.current || force
      pendingResizeFreezeColsRef.current = pendingResizeFreezeColsRef.current || freezeCols

      if (resizeTimerRef.current !== null) {
        window.cancelAnimationFrame(resizeTimerRef.current)
      }

      resizeTimerRef.current = window.requestAnimationFrame(() => {
        resizeTimerRef.current = null
        const shouldForce = pendingResizeForceRef.current
        const shouldFreezeCols = pendingResizeFreezeColsRef.current
        pendingResizeForceRef.current = false
        pendingResizeFreezeColsRef.current = false
        resize(shouldForce, shouldFreezeCols, preserveVisibleBuffer)
      })
    }

    // Font loading and a monitor-DPI transition occur outside ResizeObserver's
    // CSS-box model. Refresh xterm's cached glyph/canvas metrics explicitly so
    // packaged WebView2/WebKit builds cannot keep a fallback-font grid.
    const disposeCanvasTextMetrics = observeCanvasTextMetrics((fontFamily) => {
      terminal.options.fontFamily = fontFamily
      terminal.clearTextureAtlas()
      terminal.refresh(0, Math.max(terminal.rows - 1, 0))
      scheduleResize(true)
    })

    const scheduleSettledHorizontalResize = () => {
      if (resizeSettleTimerRef.current !== null) {
        window.clearTimeout(resizeSettleTimerRef.current)
      }

      resizeSettleTimerRef.current = window.setTimeout(() => {
        const quietFor = Date.now() - lastTerminalOutputAtRef.current
        if (quietFor < TERMINAL_RESIZE_OUTPUT_QUIET_MS) {
          scheduleSettledHorizontalResize()
          return
        }

        if (awaitingCommandCompletionRef.current) {
          const promptLine = getLastVisibleTerminalLine(terminal)
          if (!looksLikeShellPrompt(promptLine)) {
            resizeSettleTimerRef.current = null
            isHorizontalResizeActiveRef.current = false
            pendingPromptResizeRef.current = true
            return
          }
          awaitingCommandCompletionRef.current = false
        }

        resizeSettleTimerRef.current = null
        isHorizontalResizeActiveRef.current = false
        pendingPromptResizeRef.current = false
        window.requestAnimationFrame(() => scheduleResize(true, false, true))
      }, TERMINAL_RESIZE_SETTLE_MS)
    }

    const onDataDispose = terminal.onData((data) => {
      if (replayingTranscriptRef.current) {
        data = stripHydratedTerminalResponses(data)
      }
      if (!data) {
        return
      }

      if (imeCompositionActiveRef.current) {
        // Intermediate composition text is owned by the IME and must never
        // reach the remote PTY before compositionend.
        return
      }

      const compositionText = imeCompositionTextRef.current
      const compositionEndedAt = imeCompositionEndedAtRef.current
      const isWithinCompositionWindow =
        compositionText.length > 0 && Date.now() - compositionEndedAt <= TERMINAL_IME_DUPLICATE_WINDOW_MS
      if (isWithinCompositionWindow && data === ' ') {
        // Switching from a Chinese IME back to an ASCII layout can leave the
        // candidate-confirmation space as a standalone xterm data event. It
        // is not part of the user's terminal input in this transition.
        return
      }
      const isAsciiCompositionPayload = /^[\x20-\x7e]*$/.test(compositionText) && /^[\x20-\x7e]*$/.test(data)
      const compositionTextWithoutSpaces = compositionText.replaceAll(' ', '')
      const dataWithoutSpaces = data.replaceAll(' ', '')
      const duplicatedCompositionText = `${compositionTextWithoutSpaces}${compositionTextWithoutSpaces}`
      const isCompositionPayload =
        isWithinCompositionWindow &&
        compositionTextWithoutSpaces.length > 0 &&
        (dataWithoutSpaces === compositionTextWithoutSpaces || dataWithoutSpaces === duplicatedCompositionText)
      if (isCompositionPayload) {
        if (imeCompositionDataForwardedRef.current) {
          // A language switch can make the same committed composition arrive
          // twice. Forward the first event only.
          return
        }
        imeCompositionDataForwardedRef.current = true
        data = dataWithoutSpaces === duplicatedCompositionText ? compositionTextWithoutSpaces : dataWithoutSpaces
      } else if (
        isWithinCompositionWindow &&
        isAsciiCompositionPayload &&
        data.includes(' ') &&
        compositionTextWithoutSpaces.length > 0 &&
        data.length <= compositionText.length + 4 &&
        (dataWithoutSpaces.includes(compositionTextWithoutSpaces) ||
          compositionTextWithoutSpaces.includes(dataWithoutSpaces))
      ) {
        // The same transition may put the confirmation space inside the
        // payload (`W n`) instead of emitting it separately. Remove all
        // spaces from this short-lived IME payload so it becomes `Wn`.
        data = dataWithoutSpaces
        if (!data) {
          return
        }
      }

      // When disconnected, intercept Enter to trigger reconnect instead of
      // forwarding to the (dead) PTY. Ignore while a reconnect is in flight.
      if (!connectedRef.current) {
        if (data.includes('\r') || data.includes('\n')) requestReconnect()
        return
      }
      if (data.includes('\r') || data.includes('\n')) {
        awaitingCommandCompletionRef.current = true
      }
      // xterm.js focus tracking emits "\u001b[I" (focus in) and "\u001b[O" (focus
      // out) as data events when the webview focus changes. Closing the context
      // menu or clearing the selection on these sequences makes a right-click
      // in Vim/nano immediately disable the Copy menu item.
      const isFocusTrackingEvent = isFocusTrackingSequence(data)
      if (!isFocusTrackingEvent) {
        clearEphemeralHighlight()
        setContextMenu(null)
      } else {
        logTerminalClipboard(terminal, 'focus-tracking-preserved-selection', { data })
      }
      // 发送失败必须可见：后端 worker 死亡（panic/断连）时 send 会 reject，
      // 静默吞掉会让终端看起来"卡死且 Ctrl+C 无效"。降级为未连接状态并
      // 给出重连提示，Enter 重连路径随之可用。
      window.fileterm?.writeTerminal(tabIdRef.current, data)?.catch((error: unknown) => {
        if (inputSendFailedRef.current) {
          return
        }
        inputSendFailedRef.current = true
        // 一次性写入 devtools 控制台，便于和后端 app.log 里的 panic 行交叉
        // 定位（后端 panic hook 写 scope=panic，前端这里写 tab id）。
        console.warn(
          `[TerminalView] writeTerminal rejected for tab ${tabIdRef.current}; worker likely dead, degrading to disconnected state`,
          error
        )
        // Do not wait for the backend's terminal:state broadcast before
        // allowing Enter to use the reconnect path. A dead worker cannot
        // consume another input packet, so retaining `connected=true` here
        // would make the first retry look swallowed.
        connectedRef.current = false
        wasConnectedRef.current = false
        terminal.write(`\r\n${closedMessageRef.current}\r\n`)
        writeReconnectHint()
      })
    })

    const onSelectionDispose = terminal.onSelectionChange(() => {
      const nextHasSelection = terminal.hasSelection() || Boolean(getVimVisualSelection(terminal))
      setHasSelection(nextHasSelection)
      logTerminalClipboard(terminal, 'selection-changed', { nextHasSelection })
    })

    const offData = window.fileterm?.onTerminalData(({ tabId: nextTabId, chunk }) => {
      if (nextTabId === tabIdRef.current) {
        lastTerminalOutputAtRef.current = Date.now()
        const shouldTrimHydratedBacklog = Date.now() < suppressHydratedChunksUntilRef.current
        if (shouldTrimHydratedBacklog) {
          // IPC ordering can leave one queued batch behind an authoritative
          // snapshot. Consume that single backlog opportunity; repeated fuzzy
          // trimming could eat legitimate repeated terminal output.
          suppressHydratedChunksUntilRef.current = 0
        }
        const visibleChunk = shouldTrimHydratedBacklog
          ? trimHydratedTerminalChunk(renderedTranscriptRef.current, chunk)
          : chunk
        if (!visibleChunk) {
          return
        }
        appendRenderedTranscript(visibleChunk)
        clearSearchDecorations()
        scheduleTerminalWrite(formatTerminalChunk(terminal, visibleChunk))
        if (pendingPromptResizeRef.current) {
          scheduleSettledHorizontalResize()
        }
      }
    })

    const offState = window.fileterm?.onTerminalState(
      ({ tabId: nextTabId, summary, transcript, connected, status }) => {
        if (nextTabId === tabIdRef.current) {
          onStatusRef.current?.(localizeTerminalText(summary))
          connectedRef.current = connected
          connectingRef.current = status === 'connecting'
          const isDisconnecting = wasConnectedRef.current && !connected
          if (isDisconnecting) {
            preserveVisibleBufferRef.current = true
          }
          if (shouldHydrateTranscript(renderedTranscriptRef.current, transcript, connected)) {
            replaceTerminalWithTranscript(terminal, transcript)
          }
          if (!wasConnectedRef.current && connected) {
            lastSyncedSizeRef.current = null
            preserveVisibleBufferRef.current = false
            awaitingCommandCompletionRef.current = false
            pendingPromptResizeRef.current = false
            window.requestAnimationFrame(() => scheduleResize(true))
          }
          if (isDisconnecting) {
            terminal.write(buildExitAlternateScreenSequence(), () => {
              const visibleTranscript = snapshotTerminalBuffer(terminal)
              const reconnectHint = onReconnectRef.current ? `\r\n${reconnectHintRef.current}` : ''
              if (reconnectHint) {
                reconnectHintShownRef.current = true
              }
              const disconnectedTranscript = visibleTranscript
                ? `${visibleTranscript}\r\n${closedMessageRef.current}${reconnectHint}\r\n`
                : `${closedMessageRef.current}${reconnectHint}\r\n`
              replaceTerminalWithTranscript(terminal, disconnectedTranscript)
            })
          } else if (!connected && status === 'error') {
            // A reconnect command only starts the worker, so its Promise resolves
            // before a failed TCP/SSH handshake is known. The terminal state is
            // the authoritative failure signal for both initial and retry attempts.
            writeReconnectHint()
          }
          wasConnectedRef.current = connected
          if (connected) {
            isReconnectingRef.current = false
            inputSendFailedRef.current = false
            reconnectHintShownRef.current = false
          }
        }
      }
    )

    const resizeObserver = new ResizeObserver(() => {
      const host = hostRef.current
      if (!host) {
        return
      }

      const { left, top, right, bottom, width, height } = host.getBoundingClientRect()
      const lastObservedRect = lastObservedHostRectRef.current
      lastObservedHostRectRef.current = { left, top, right, bottom, width, height }

      const widthChanged = Boolean(
        lastObservedRect && Math.abs(lastObservedRect.width - width) > TERMINAL_RESIZE_PIXEL_EPSILON
      )

      if (widthChanged) {
        isHorizontalResizeActiveRef.current = true
        scheduleResize(false, true)
        scheduleSettledHorizontalResize()
        return
      }

      if (isHorizontalResizeActiveRef.current) {
        scheduleResize(false, true)
        scheduleSettledHorizontalResize()
        return
      }

      scheduleResize()
    })
    resizeObserver.observe(hostRef.current)

    const onWindowFocus = () => {
      window.requestAnimationFrame(() => scheduleResize(true))
    }

    const onVisibilityChange = () => {
      if (document.visibilityState === 'visible') {
        window.requestAnimationFrame(() => scheduleResize(true))
      }
    }

    const openContextMenu = (event: MouseEvent) => {
      event.preventDefault()
      // This is a document-capture handler, so prevent xterm.js and any native
      // context-menu handler from observing the canonical event after us.
      // The earlier pointerdown/mousedown capture handlers are responsible for
      // stopping xterm's mouse-reporting handlers.
      event.stopImmediatePropagation()
      // Read selection state when the menu is about to show: xterm.js finishes
      // its selection on pointerup, so at contextmenu time terminal.hasSelection()
      // is authoritative. Pointerdown/mousedown are too early.
      const vimVisualSelection = getVimVisualSelection(terminal, true)
      const nextHasSelection = terminal.hasSelection() || Boolean(vimVisualSelection)
      setHasSelection(nextHasSelection)
      logTerminalClipboard(terminal, 'context-menu-opened', {
        nextHasSelection,
        source: vimVisualSelection ? 'vim-visual' : 'xterm',
        vimVisualMode: vimVisualSelection?.mode,
        vimVisualRows: vimVisualSelection ? `${vimVisualSelection.startRow}-${vimVisualSelection.endRow}` : undefined
      })
      setContextMenu({ x: event.clientX, y: event.clientY })
    }

    const isSecondaryButton = (event: MouseEvent | PointerEvent) => {
      // macOS Ctrl+Click synthesizes a context menu with button 0 + ctrlKey.
      return event.button === 2 || (event.button === 0 && event.ctrlKey && isMac)
    }

    const isEventInsideTerminal = (event: Event) => {
      const host = hostRef.current
      const target = event.target
      if (!host) {
        return false
      }

      if (target instanceof Node && host.contains(target)) {
        return true
      }

      // In WebView2 and WebKitGTK, compositor-backed terminal canvases can
      // retarget a wheel event during capture. The composed path still retains
      // the terminal host, so use it as the platform-safe fallback.
      return event.composedPath().includes(host)
    }

    const isEventOverTerminal = (event: Event) => {
      const host = hostRef.current
      const { clientX, clientY } = event as MouseEvent
      if (!host || !Number.isFinite(clientX) || !Number.isFinite(clientY)) {
        return false
      }

      const bounds = lastObservedHostRectRef.current
      if (!bounds) {
        return false
      }

      return clientX >= bounds.left && clientX <= bounds.right && clientY >= bounds.top && clientY <= bounds.bottom
    }

    const isRootRetargetedGesture = (event: Event) => {
      const target = event.target
      return (
        target === null ||
        target === window ||
        target === document ||
        target === document.documentElement ||
        target === document.body
      )
    }

    const isTerminalGestureTarget = (event: Event) => {
      if (isEventInsideTerminal(event)) {
        return true
      }

      // WebView2, WebKitGTK and WKWebView can dispatch a compositor gesture
      // at the document root rather than the element beneath the pinch. Only
      // those root-retargeted events need the geometry/focus fallback. A
      // regular file-pane wheel event must stay on the native file scroller;
      // reading terminal bounds for every such event forces a synchronous
      // layout while a virtualized table is mounting rows.
      if (!isRootRetargetedGesture(event)) {
        return false
      }

      if (isEventOverTerminal(event)) {
        return true
      }

      const { clientX, clientY } = event as MouseEvent
      if (Number.isFinite(clientX) && Number.isFinite(clientY)) {
        return false
      }

      // The xterm textarea is the authoritative focus owner when the WebView
      // drops both the target and composed path. A click into another pane
      // updates lastFocusedTerminal before its next gesture.
      return lastFocusedTerminal === terminal && document.activeElement === terminalTextarea
    }

    const isTerminalShortcutTarget = (event: KeyboardEvent) =>
      lastFocusedTerminal === terminal && (document.activeElement === terminalTextarea || isEventInsideTerminal(event))

    const onMouseDown = (event: MouseEvent) => {
      if (!isSecondaryButton(event)) {
        return
      }
      // Vim and other TUI programs enable xterm mouse reporting. xterm.js
      // registers its own mousedown listener that calls ev.preventDefault() +
      // this.focus(), suppressing the contextmenu event and sending a mouse
      // sequence to the remote program. Stop it here so the native contextmenu
      // event still fires and our menu can open.
      event.stopImmediatePropagation()
    }

    const onPointerDown = (event: PointerEvent) => {
      if (!isSecondaryButton(event)) {
        return
      }
      // xterm.js 5.x uses Pointer Events for mouse reporting. Intercept the
      // right button in the capture phase before xterm converts it into an
      // escape sequence for Vim/nano. Do not call preventDefault(): that would
      // break xterm.js text selection. Do not set menu state here: selection is
      // not final until pointerup/contextmenu.
      event.stopImmediatePropagation()
    }

    let controlKeyActive = false
    let wheelZoomDelta = 0
    const normalizeWheelZoomDelta = (event: WheelEvent) => {
      const rawDelta = event.deltaY
      if (!Number.isFinite(rawDelta) || rawDelta === 0) {
        return 0
      }

      const delta =
        event.deltaMode === WheelEvent.DOM_DELTA_LINE
          ? rawDelta * 4
          : event.deltaMode === WheelEvent.DOM_DELTA_PAGE
            ? rawDelta * TERMINAL_WHEEL_ZOOM_THRESHOLD
            : rawDelta
      return Math.max(-TERMINAL_WHEEL_ZOOM_THRESHOLD, Math.min(TERMINAL_WHEEL_ZOOM_THRESHOLD, delta))
    }
    const consumeTerminalWheelZoom = (event: WheelEvent, source: 'window' | 'xterm') => {
      // Chromium/WebKit surface both Ctrl+wheel and trackpad pinch as a wheel
      // event with ctrlKey=true. Some Windows/Linux WebViews lose ctrlKey on
      // compositor-retargeted wheel events, so retain the key state captured
      // by the window as a fallback. Keep this terminal-scoped: unmodified
      // two-finger scrolling continues to scroll the terminal normally.
      const hasZoomModifier = isMac
        ? event.ctrlKey || event.metaKey || event.getModifierState('Control') || event.getModifierState('Meta')
        : event.ctrlKey || event.getModifierState('Control') || controlKeyActive
      if (!hasZoomModifier) {
        logTerminalZoom(terminal, `wheel-${source}-ignored-no-modifier`, {
          ...describeTerminalInput(event),
          controlKeyActive,
          tabId: tabIdRef.current
        })
        return false
      }

      const delta = normalizeWheelZoomDelta(event)
      if (delta === 0) {
        logTerminalZoom(terminal, `wheel-${source}-ignored-zero-delta`, {
          ...describeTerminalInput(event),
          tabId: tabIdRef.current
        })
        return true
      }

      wheelZoomDelta += delta
      const steps = Math.floor(Math.abs(wheelZoomDelta) / TERMINAL_WHEEL_ZOOM_THRESHOLD)
      if (steps === 0) {
        logTerminalZoom(terminal, `wheel-${source}-accumulating`, {
          ...describeTerminalInput(event),
          delta,
          wheelZoomDelta,
          tabId: tabIdRef.current
        })
        return true
      }

      const direction = wheelZoomDelta < 0 ? 1 : -1
      wheelZoomDelta -= Math.sign(wheelZoomDelta) * steps * TERMINAL_WHEEL_ZOOM_THRESHOLD
      const changed = adjustTerminalFontSize(direction * steps)
      logTerminalZoom(terminal, `wheel-${source}-applied`, {
        ...describeTerminalInput(event),
        changed,
        direction,
        steps,
        tabId: tabIdRef.current
      })
      return true
    }
    const onWheel = (event: WheelEvent) => {
      const matchesTerminal = isTerminalGestureTarget(event)
      if (!matchesTerminal) {
        return
      }

      if (!consumeTerminalWheelZoom(event, 'window')) {
        return
      }

      event.preventDefault()
      event.stopImmediatePropagation()
    }
    // xterm sees the terminal canvas' wheel event before it turns it into
    // scrollback or a remote mouse-reporting sequence. This is the primary
    // path; the window listener above remains for platform-retargeted events.
    terminal.attachCustomWheelEventHandler((event) => {
      if (!consumeTerminalWheelZoom(event, 'xterm')) {
        return true
      }

      event.preventDefault()
      event.stopImmediatePropagation()
      return false
    })

    let previousGestureScale = 1
    let gestureZoomDelta = 0
    const onGestureStart = (event: Event) => {
      if (!isTerminalGestureTarget(event)) {
        return
      }
      // WKWebView reports trackpad pinch through WebKit GestureEvents rather
      // than Chromium's ctrlKey wheel path. Cancelling it prevents WebView
      // page zoom and keeps the gesture local to this terminal.
      event.preventDefault()
      event.stopImmediatePropagation()
      previousGestureScale = (event as PinchGestureEvent).scale ?? 1
      gestureZoomDelta = 0
      logTerminalZoom(terminal, 'gesture-start', { scale: previousGestureScale })
    }
    const onGestureChange = (event: Event) => {
      if (!isTerminalGestureTarget(event)) {
        return
      }
      event.preventDefault()
      event.stopImmediatePropagation()
      const scale = (event as PinchGestureEvent).scale ?? previousGestureScale
      if (!Number.isFinite(scale) || scale <= 0 || scale === previousGestureScale) {
        return
      }

      gestureZoomDelta += Math.log(scale / previousGestureScale)
      previousGestureScale = scale
      const steps = Math.floor(Math.abs(gestureZoomDelta) / TERMINAL_GESTURE_ZOOM_THRESHOLD)
      if (steps === 0) {
        return
      }

      const direction = gestureZoomDelta > 0 ? 1 : -1
      gestureZoomDelta -= direction * steps * TERMINAL_GESTURE_ZOOM_THRESHOLD
      applyTerminalZoom(direction > 0 ? 'in' : 'out', 'gesture', steps)
      logTerminalZoom(terminal, 'gesture-change', { direction, scale, steps })
    }
    const onGestureEnd = (event: Event) => {
      if (!isTerminalGestureTarget(event)) {
        return
      }
      event.preventDefault()
      event.stopImmediatePropagation()
      logTerminalZoom(terminal, 'gesture-end')
      previousGestureScale = 1
      gestureZoomDelta = 0
    }

    const onDocumentContextMenu = (event: MouseEvent) => {
      // Final safety net for Vim/nano with mouse reporting enabled: pointerdown
      // and mousedown may be consumed by xterm's mouse handler before our
      // hostRef capture runs in some WKWebView configurations. The contextmenu
      // event is the canonical right-click event and always fires at document
      // level, so we catch it here and verify the click landed inside our
      // terminal before showing the menu.
      if (!isEventInsideTerminal(event)) {
        return
      }
      openContextMenu(event)
    }

    const onDocumentSelectionChange = () => {
      const selection = window.getSelection()
      const anchorNode = selection?.anchorNode
      const terminalHost = hostRef.current
      if (selection && !selection.isCollapsed && anchorNode && terminalHost && !terminalHost.contains(anchorNode)) {
        terminal.clearSelection()
      }
    }

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Control') {
        controlKeyActive = true
      }

      const terminalZoomOperation = getTerminalZoomOperation(event)
      if (event.ctrlKey || event.metaKey || terminalZoomOperation) {
        logTerminalZoom(terminal, 'key-window-received', {
          ...describeTerminalInput(event),
          operation: terminalZoomOperation,
          isTerminalShortcutTarget: isTerminalShortcutTarget(event),
          tabId: tabIdRef.current
        })
      }
      if (terminalZoomOperation && isTerminalShortcutTarget(event)) {
        // Tauri's WebViews can consume browser-style zoom shortcuts before
        // xterm sees them. Capture the terminal-scoped fallback at window
        // level, including Ctrl+Shift+0 on Windows/Linux.
        event.preventDefault()
        event.stopImmediatePropagation()
        applyTerminalZoom(terminalZoomOperation, 'shortcut')
        return
      }

      const key = event.key.toLowerCase()
      const matchesCopy = isMac
        ? event.metaKey && !event.shiftKey && key === 'c'
        : event.ctrlKey && event.shiftKey && key === 'c'

      if (matchesCopy && (terminal.hasSelection() || Boolean(getVimVisualSelection(terminal, true)))) {
        const target = event.target
        const editableTarget =
          target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement ? target : null
        const editableSelection = editableTarget ? editableTarget.selectionStart !== editableTarget.selectionEnd : false
        const documentSelection = window.getSelection()
        const hasDocumentSelection = Boolean(
          documentSelection && !documentSelection.isCollapsed && documentSelection.toString()
        )

        if (!editableSelection && !hasDocumentSelection) {
          event.preventDefault()
          event.stopPropagation()
          logTerminalClipboard(terminal, 'copy-shortcut-window', { key: event.key })
          runCopy()
        }
        return
      }

      if (document.activeElement === terminal.textarea && !isMac && event.ctrlKey && !event.shiftKey && key === 'l') {
        event.preventDefault()
        runClear()
      }
    }

    const onKeyUp = (event: KeyboardEvent) => {
      if (event.key === 'Control') {
        controlKeyActive = false
      }
    }
    const onWindowBlur = () => {
      // A keyup can be delivered to a different native surface after the app
      // loses focus. Do not let a stale Ctrl state turn later touchpad scrolls
      // into zoom gestures.
      controlKeyActive = false
    }

    const handleFocusTerminal = (targetTabId: string) => {
      if (targetTabId && targetTabId !== tabIdRef.current) {
        return
      }
      terminal.focus()
    }
    const handleTerminalCopy = () => {
      runCopy()
    }
    const handleTerminalPaste = () => {
      void runPaste()
    }
    const handleTerminalFind = () => {
      if (findOpenRef.current) {
        closeFind()
      } else {
        openFind()
      }
    }
    const handleTerminalZoom = (operation: TerminalZoomOperation) => {
      if (lastFocusedTerminal !== terminal) {
        logTerminalZoom(terminal, 'native-zoom-request-ignored-not-focused', {
          operation,
          tabId: tabIdRef.current
        })
        return
      }
      logTerminalZoom(terminal, 'native-zoom-request-received', { operation, tabId: tabIdRef.current })
      applyTerminalZoom(operation, 'menu')
    }
    const handleTerminalGestureZoom = (operation: TerminalZoomOperation) => {
      if (terminalUnderPointer !== terminal) {
        logTerminalZoom(terminal, 'native-gesture-zoom-ignored-not-hovered', {
          operation,
          hoveredTerminal: terminalUnderPointer === null ? null : 'another-terminal'
        })
        return
      }

      logTerminalZoom(terminal, 'native-gesture-zoom-received', { operation, tabId: tabIdRef.current })
      applyTerminalZoom(operation, 'gesture')
    }

    // xterm's textarea focus event is not consistently forwarded after a
    // compositor-backed pointer interaction. Record the active pane from the
    // host's capture phase as well, before xterm consumes the event.
    hostRef.current.addEventListener('focusin', markTerminalFocused)
    hostRef.current.addEventListener('pointerdown', markTerminalFocused, true)
    hostRef.current.addEventListener('pointerenter', markTerminalUnderPointer)
    hostRef.current.addEventListener('pointerleave', clearTerminalUnderPointer)
    hostRef.current.addEventListener('mousedown', onMouseDown, true)
    hostRef.current.addEventListener('pointerdown', onPointerDown, true)
    // Register at window capture so Windows WebView2 and Linux WebKitGTK
    // compositor events cannot bypass a listener attached below xterm's DOM.
    // onWheel scopes the event back to this terminal before consuming it.
    window.addEventListener('wheel', onWheel, { capture: true, passive: false })
    // GestureEvents can target document/body in WKWebView, so listen at the
    // window capture phase and scope the handler with isTerminalGestureTarget.
    window.addEventListener('gesturestart', onGestureStart, { capture: true, passive: false })
    window.addEventListener('gesturechange', onGestureChange, { capture: true, passive: false })
    window.addEventListener('gestureend', onGestureEnd, { capture: true, passive: false })
    document.addEventListener('contextmenu', onDocumentContextMenu, true)
    window.addEventListener('keydown', onKeyDown, true)
    window.addEventListener('keyup', onKeyUp, true)
    window.addEventListener('blur', onWindowBlur)
    window.addEventListener('focus', onWindowFocus)
    document.addEventListener('selectionchange', onDocumentSelectionChange)
    document.addEventListener('visibilitychange', onVisibilityChange)
    const offFocusTerminal = onAppEvent(APP_EVENT.focusTerminal, handleFocusTerminal)
    const offTerminalCopy = onAppEvent(APP_EVENT.terminalCopy, handleTerminalCopy)
    const offTerminalPaste = onAppEvent(APP_EVENT.terminalPaste, handleTerminalPaste)
    const offTerminalFind = onAppEvent(APP_EVENT.terminalFind, handleTerminalFind)
    const offTerminalZoom = onAppEvent(APP_EVENT.terminalZoom, handleTerminalZoom)
    const offNativeTerminalZoom = window.fileterm?.onTerminalZoomRequest(handleTerminalZoom)
    const offNativeTerminalGestureZoom = window.fileterm?.onTerminalGestureZoomRequest(handleTerminalGestureZoom)

    // Ask the main process for the actual PTY size once the terminal is mounted.
    if (!bootedTabs.current.has(tabIdRef.current)) {
      bootedTabs.current.add(tabIdRef.current)
      resize()
    }

    return () => {
      offFocusTerminal()
      offTerminalCopy()
      offTerminalPaste()
      offTerminalFind()
      offTerminalZoom()
      offNativeTerminalZoom?.()
      offNativeTerminalGestureZoom?.()
      onDataDispose.dispose()
      onSelectionDispose.dispose()
      offData?.()
      offState?.()
      if (writeFrameRef.current !== null) {
        window.cancelAnimationFrame(writeFrameRef.current)
      }
      if (resizeTimerRef.current !== null) {
        window.cancelAnimationFrame(resizeTimerRef.current)
      }
      if (resizeSettleTimerRef.current !== null) {
        window.clearTimeout(resizeSettleTimerRef.current)
      }
      writeFrameRef.current = null
      resizeTimerRef.current = null
      resizeSettleTimerRef.current = null
      pendingResizeForceRef.current = false
      pendingResizeFreezeColsRef.current = false
      isWritingRef.current = false
      pendingWriteRef.current = ''
      renderedTranscriptRef.current = ''
      suppressHydratedChunksUntilRef.current = 0
      replayingTranscriptRef.current = false
      transcriptReplayGenerationRef.current += 1
      preserveVisibleBufferRef.current = false
      lastSyncedSizeRef.current = null
      lastObservedHostRectRef.current = null
      isHorizontalResizeActiveRef.current = false
      lastTerminalOutputAtRef.current = 0
      awaitingCommandCompletionRef.current = false
      pendingPromptResizeRef.current = false
      searchResultsDisposable.dispose()
      osc52Disposable.dispose()
      disposeCanvasTextMetrics()
      resizeObserver.disconnect()
      hostRef.current?.removeEventListener('focusin', markTerminalFocused)
      hostRef.current?.removeEventListener('pointerdown', markTerminalFocused, true)
      hostRef.current?.removeEventListener('pointerenter', markTerminalUnderPointer)
      hostRef.current?.removeEventListener('pointerleave', clearTerminalUnderPointer)
      hostRef.current?.removeEventListener('mousedown', onMouseDown, true)
      hostRef.current?.removeEventListener('pointerdown', onPointerDown, true)
      window.removeEventListener('wheel', onWheel, true)
      terminalTextarea?.removeEventListener('focus', markTerminalFocused)
      terminalTextarea?.removeEventListener('compositionstart', onCompositionStart)
      terminalTextarea?.removeEventListener('compositionupdate', onCompositionUpdate)
      terminalTextarea?.removeEventListener('compositionend', onCompositionEnd)
      window.removeEventListener('gesturestart', onGestureStart, true)
      window.removeEventListener('gesturechange', onGestureChange, true)
      window.removeEventListener('gestureend', onGestureEnd, true)
      document.removeEventListener('contextmenu', onDocumentContextMenu, true)
      window.removeEventListener('keydown', onKeyDown, true)
      window.removeEventListener('keyup', onKeyUp, true)
      window.removeEventListener('blur', onWindowBlur)
      window.removeEventListener('focus', onWindowFocus)
      document.removeEventListener('selectionchange', onDocumentSelectionChange)
      document.removeEventListener('visibilitychange', onVisibilityChange)
      if (lastFocusedTerminal === terminal) {
        lastFocusedTerminal = null
      }
      if (terminalUnderPointer === terminal) {
        terminalUnderPointer = null
      }
      terminalLogColorizer.dispose()
      terminalLogColorizerRef.current = null
      searchAddonRef.current = null
      fitAddonRef.current = null
      terminalRef.current = null
      terminal.dispose()
    }
  }, [isMac])
}
