import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { SearchAddon } from '@xterm/addon-search'
import type { VerticalScrollController } from '../features/common/VerticalScrollbar'
import { getTerminalLogColorPalette, TerminalLogColorizer } from '../app/terminal-log-colorizer'
import { readClipboardText, writeClipboardText } from '../app/app-utils'
import { t } from '../i18n'
import { getConfiguredMonoFontFamily } from '../app/font-metrics'
import {
  getTerminalFontSize,
  hydrateTerminalFontSizes,
  subscribeTerminalFontSizes,
  TERMINAL_MIN_FONT_SIZE,
  TERMINAL_MAX_FONT_SIZE
} from '../app/terminal-font-size-store'
import {
  getVimVisualSelection,
  logTerminalClipboard,
  logTerminalRender,
  normalizeLocalTerminalStartupTranscript,
  splitPaneShortcutsForPlatform,
  toDisplayTerminalText,
  trimTranscript,
  TERMINAL_FIT_GUARD_ROWS,
  TERMINAL_REMOTE_GUARD_COLS,
  TERMINAL_RESIZE_PIXEL_EPSILON,
  TERMINAL_WRITE_FRAME_BUDGET,
  type SplitPaneDirection
} from './terminal-view-utils'
import { useTerminalLifecycle } from './useTerminalLifecycle'

export type TerminalViewProps = {
  profileId: string
  tabId: string
  bootText: string
  sessionType?: 'ssh' | 'ftp' | 'telnet' | 'serial' | 'local'
  connected?: boolean
  connecting?: boolean
  isActive?: boolean
  onStatus?(message: string | null): void
  onReconnect?(): void | Promise<void>
  onActivate?(): void
  onSplitPane?(direction: SplitPaneDirection): void
  onClosePane?(): void
  onCloseTab?(): void
  canClosePane?: boolean
  closedMessage?: string
  reconnectHint?: string
}

// WKWebView and WebKitGTK may keep a newly-visible keep-alive surface at zero
// size for several compositor frames while the workspace transition settles.
// Keep the retry bounded so a genuinely unavailable surface cannot create a
// permanent animation-frame loop.
const TERMINAL_ACTIVATION_RECOVERY_ATTEMPTS = 16

export function useTerminalView({
  profileId,
  tabId,
  bootText,
  sessionType = 'ssh',
  connected = false,
  connecting = false,
  isActive = true,
  onStatus,
  onReconnect,
  onSplitPane,
  onClosePane,
  onCloseTab,
  canClosePane = false,
  closedMessage = t.terminalConnectionClosed,
  reconnectHint = t.pressEnterToReconnect
}: TerminalViewProps) {
  const hydratedBootText =
    sessionType === 'local' ? normalizeLocalTerminalStartupTranscript(bootText, window.fileterm?.platform) : bootText
  const hostRef = useRef<HTMLDivElement | null>(null)
  const [terminalScrollableElement, setTerminalScrollableElement] = useState<HTMLElement | null>(null)
  const terminalRef = useRef<Terminal | null>(null)
  const fitAddonRef = useRef<FitAddon | null>(null)
  const terminalLogColorizerRef = useRef<TerminalLogColorizer | null>(null)
  const searchAddonRef = useRef<SearchAddon | null>(null)
  const findInputRef = useRef<HTMLInputElement | null>(null)
  const bootTextRef = useRef(hydratedBootText)
  const renderedTranscriptRef = useRef('')
  const pendingWriteRef = useRef('')
  const writeFrameRef = useRef<number | null>(null)
  const resizeTimerRef = useRef<number | null>(null)
  const resizeSettleTimerRef = useRef<number | null>(null)
  const pendingResizeForceRef = useRef(false)
  const pendingResizeFreezeColsRef = useRef(false)
  const isWritingRef = useRef(false)
  const suppressHydratedChunksUntilRef = useRef(0)
  const preserveVisibleBufferRef = useRef(false)
  const bootedTabs = useRef(new Set<string>())
  const wasConnectedRef = useRef(false)
  // 后端 worker 异常退出时 writeTerminal 会 reject；记录一次避免每个按键
  // 都刷一行提示，直到 terminal:state 重新同步后复位。
  const inputSendFailedRef = useRef(false)
  // xterm owns composition events internally, but a few native IMEs can still
  // produce a second `onData` event while switching input languages. Track
  // only the current composition transaction so normal repeated input and
  // paste are unaffected.
  const imeCompositionActiveRef = useRef(false)
  const imeCompositionTextRef = useRef('')
  const imeCompositionEndedAtRef = useRef(0)
  const imeCompositionDataForwardedRef = useRef(false)
  const replayingTranscriptRef = useRef(false)
  const transcriptReplayGenerationRef = useRef(0)
  // `onData` is registered once for the xterm instance.  Reading the prop
  // through a ref prevents a stale terminal-state event from a background
  // tab from swallowing keystrokes after this tab is brought back.
  const connectedRef = useRef(Boolean(connected))
  const connectingRef = useRef(Boolean(connecting))
  const isActiveRef = useRef(Boolean(isActive))
  const serialTransferBusyRef = useRef(false)
  const lastSyncedSizeRef = useRef<{ cols: number; rows: number; width: number; height: number } | null>(null)
  const lastObservedHostRectRef = useRef<{
    left: number
    top: number
    right: number
    bottom: number
    width: number
    height: number
  } | null>(null)
  const isHorizontalResizeActiveRef = useRef(false)
  const lastTerminalOutputAtRef = useRef(0)
  const awaitingCommandCompletionRef = useRef(false)
  const pendingPromptResizeRef = useRef(false)
  const profileIdRef = useRef(profileId)
  const tabIdRef = useRef(tabId)
  const onStatusRef = useRef(onStatus)
  const onReconnectRef = useRef(onReconnect)
  const closedMessageRef = useRef(closedMessage)
  const reconnectHintRef = useRef(reconnectHint)
  const onSplitPaneRef = useRef(onSplitPane)
  const onClosePaneRef = useRef(onClosePane)
  const onCloseTabRef = useRef(onCloseTab)
  const canClosePaneRef = useRef(canClosePane)
  const isReconnectingRef = useRef(false)
  const reconnectHintShownRef = useRef(false)
  const activeTerminalTabIdRef = useRef<string | null>(null)
  profileIdRef.current = profileId
  tabIdRef.current = tabId
  connectedRef.current = Boolean(connected)
  connectingRef.current = Boolean(connecting)
  isActiveRef.current = Boolean(isActive)
  onStatusRef.current = onStatus
  onReconnectRef.current = onReconnect
  closedMessageRef.current = closedMessage
  reconnectHintRef.current = reconnectHint
  onSplitPaneRef.current = onSplitPane
  onClosePaneRef.current = onClosePane
  onCloseTabRef.current = onCloseTab
  canClosePaneRef.current = canClosePane

  useEffect(() => {
    serialTransferBusyRef.current = false
    if (sessionType !== 'serial' || !window.fileterm?.onSerialTransferProgress) {
      return
    }
    return window.fileterm.onSerialTransferProgress((progress) => {
      if (progress.tabId === tabId) {
        serialTransferBusyRef.current = progress.status === 'running'
      }
    })
  }, [sessionType, tabId])
  const [hasSelection, setHasSelection] = useState(false)
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null)
  const [findOpen, setFindOpen] = useState(false)
  const findOpenRef = useRef(findOpen)
  useEffect(() => {
    findOpenRef.current = findOpen
  }, [findOpen])
  const [findQuery, setFindQuery] = useState('')
  const [findMiss, setFindMiss] = useState(false)
  const [findMatchCount, setFindMatchCount] = useState(0)
  const [activeFindIndex, setActiveFindIndex] = useState(-1)
  const [findCaseSensitive, setFindCaseSensitive] = useState(false)
  const [findRegex, setFindRegex] = useState(false)
  const [terminalZoomLocked, setTerminalZoomLocked] = useState(false)
  const terminalZoomLockedRef = useRef(false)
  const isMac = window.fileterm?.platform === 'darwin'
  const isWindowsPty = sessionType === 'local' && window.fileterm?.platform === 'win32'

  const terminalScrollController = useMemo<VerticalScrollController | null>(() => {
    if (!terminalScrollableElement) {
      return null
    }

    return {
      getElement: () => terminalScrollableElement,
      getMetrics: () => {
        const terminal = terminalRef.current
        if (!terminal) {
          return null
        }

        const rows = Math.max(1, terminal.rows)
        const clientHeight = terminalScrollableElement.clientHeight
        if (clientHeight <= 0) {
          return null
        }

        const buffer = terminal.buffer.active
        const scrollHeight = Math.max(clientHeight, (clientHeight * Math.max(rows, buffer.length)) / rows)
        const maxScrollTop = Math.max(0, scrollHeight - clientHeight)
        const scrollTop = Math.max(0, Math.min(maxScrollTop, (clientHeight * Math.max(0, buffer.viewportY)) / rows))
        return { clientHeight, scrollHeight, scrollTop }
      },
      scrollTo: (scrollTop) => {
        const terminal = terminalRef.current
        const clientHeight = terminalScrollableElement.clientHeight
        if (!terminal || clientHeight <= 0) {
          return
        }

        terminal.scrollToLine(Math.round((scrollTop / clientHeight) * Math.max(1, terminal.rows)))
      },
      scrollBy: (scrollTop) => {
        const terminal = terminalRef.current
        const clientHeight = terminalScrollableElement.clientHeight
        if (!terminal || clientHeight <= 0) {
          return
        }

        terminal.scrollLines(Math.round((scrollTop / clientHeight) * Math.max(1, terminal.rows)))
      },
      subscribe: (listener) => {
        const terminal = terminalRef.current
        if (!terminal) {
          return () => undefined
        }

        const disposables = [
          terminal.onScroll(listener),
          terminal.onResize(listener),
          terminal.onRender(listener),
          terminal.onWriteParsed(listener),
          terminal.buffer.onBufferChange(listener)
        ]
        return () => disposables.forEach((disposable) => disposable.dispose())
      }
    }
  }, [terminalScrollableElement])

  terminalZoomLockedRef.current = terminalZoomLocked

  useEffect(() => {
    const desktopApi = window.fileterm
    if (!desktopApi) {
      return
    }

    let canceled = false
    void desktopApi
      .getUiPreferences()
      .then((preferences) => {
        if (!canceled) {
          terminalZoomLockedRef.current = preferences.terminalZoomLocked
          setTerminalZoomLocked(preferences.terminalZoomLocked)
        }
      })
      .catch(() => {
        // Keep the safe default (unlocked) when the preference cannot be read.
      })

    const unsubscribe = desktopApi.onUiPreferencesChanged((preferences) => {
      if (!canceled) {
        terminalZoomLockedRef.current = preferences.terminalZoomLocked
        setTerminalZoomLocked(preferences.terminalZoomLocked)
      }
    })

    return () => {
      canceled = true
      unsubscribe()
    }
  }, [])

  useEffect(() => {
    void hydrateTerminalFontSizes()
  }, [])

  const shortcuts = {
    copy: isMac ? '⌘C' : 'Ctrl+Shift+C',
    paste: isMac ? '⌘V' : 'Ctrl+Shift+V',
    find: 'Ctrl+Shift+F',
    ...splitPaneShortcutsForPlatform(window.fileterm?.platform)
  }

  const readColor = (name: string, fallback: string) =>
    getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback

  const buildTerminalTheme = () => ({
    background: readColor('--terminal-background', readColor('--terminal-bg', '#1e1e1e')),
    foreground: readColor('--terminal-foreground', readColor('--terminal-text', '#e0e0e0')),
    cursor: readColor('--terminal-cursor', readColor('--accent-highlight', '#3b82f6')),
    cursorAccent: readColor('--terminal-cursor-accent', readColor('--terminal-bg', '#ffffff')),
    black: readColor('--terminal-black', '#1c1d22'),
    red: readColor('--terminal-red', '#e05555'),
    green: readColor('--terminal-green', readColor('--success', '#10b981')),
    yellow: readColor('--terminal-yellow', readColor('--warning', '#eab308')),
    blue: readColor('--terminal-blue', readColor('--accent-text', '#3b82f6')),
    magenta: readColor('--terminal-magenta', '#c084fc'),
    cyan: readColor('--terminal-cyan', '#22d3ee'),
    white: readColor('--terminal-white', '#e2e8f0'),
    brightBlack: readColor('--terminal-bright-black', '#64748b'),
    brightRed: readColor('--terminal-bright-red', '#f87171'),
    brightGreen: readColor('--terminal-bright-green', readColor('--success', '#34d399')),
    brightYellow: readColor('--terminal-bright-yellow', '#fde047'),
    brightBlue: readColor('--terminal-bright-blue', readColor('--text-main', '#60a5fa')),
    brightMagenta: readColor('--terminal-bright-magenta', '#e879f9'),
    brightCyan: readColor('--terminal-bright-cyan', '#67e8f9'),
    brightWhite: readColor('--terminal-bright-white', '#ffffff'),
    selectionBackground:
      findOpen && findQuery
        ? readColor('--terminal-search-active-background', readColor('--terminal-search-active-bg', '#ffd43b'))
        : readColor('--terminal-selection-background', readColor('--terminal-selection-bg', '#386bfd')),
    selectionForeground:
      findOpen && findQuery
        ? readColor('--terminal-search-active-text', '#111111')
        : readColor('--terminal-selection-foreground', readColor('--terminal-text', '#e0e0e0'))
  })

  const buildSearchDecorations = () => ({
    matchBackground: readColor(
      '--terminal-search-match-background',
      readColor('--terminal-search-match-bg', '#4b5563')
    ),
    matchOverviewRuler: readColor('--terminal-search-match-ruler', '#9ca3af'),
    activeMatchBackground: readColor(
      '--terminal-search-active-background',
      readColor('--terminal-search-active-bg', '#ffd43b')
    ),
    activeMatchBorder: readColor('--terminal-search-active-border', '#8a5a00'),
    activeMatchColorOverviewRuler: readColor('--terminal-search-active-ruler', '#f0b400')
  })

  const applyTerminalTheme = () => {
    const terminal = terminalRef.current
    if (!terminal) {
      return
    }
    terminal.options.theme = buildTerminalTheme()
    terminalLogColorizerRef.current?.setPalette(getTerminalLogColorPalette(terminal.options.theme))
    terminal.refresh(0, Math.max(terminal.rows - 1, 0))
  }

  const clearFindSelection = () => {
    searchAddonRef.current?.clearDecorations()
    const terminal = terminalRef.current
    if (terminal?.hasSelection()) {
      terminal.clearSelection()
    }
  }

  const clearEphemeralHighlight = () => {
    const terminal = terminalRef.current
    if (terminal?.hasSelection()) {
      terminal.clearSelection()
    }
    if (!findOpen) {
      searchAddonRef.current?.clearDecorations()
    }
  }

  const clearSearchDecorations = () => {
    if (!findOpenRef.current) {
      searchAddonRef.current?.clearDecorations()
    }
  }

  const closeFind = () => {
    setFindOpen(false)
    setFindQuery('')
    setFindMiss(false)
    setFindMatchCount(0)
    setActiveFindIndex(-1)
    clearFindSelection()
    terminalRef.current?.focus()
  }

  const buildSearchOptions = (incremental = false) => ({
    caseSensitive: findCaseSensitive,
    regex: findRegex,
    incremental,
    decorations: buildSearchDecorations()
  })

  const runCopy = () => {
    const terminal = terminalRef.current
    if (!terminal) {
      return
    }
    // Dock/context-menu actions can move focus to a WebView button before
    // reaching this handler. Restore the terminal first so Linux clipboard
    // providers and xterm selection stay associated with this pane.
    terminal.focus()
    const xtermSelection = terminal.getSelection()
    const vimVisualSelection = xtermSelection ? null : getVimVisualSelection(terminal, true)
    const selection = xtermSelection || vimVisualSelection?.text || ''
    if (!selection) {
      logTerminalClipboard(terminal, 'copy-skipped-empty-selection')
      return
    }
    logTerminalClipboard(terminal, 'copy-requested', {
      source: vimVisualSelection ? 'vim-visual' : 'xterm',
      vimVisualMode: vimVisualSelection?.mode
    })
    void writeClipboardText(selection).then(
      () => logTerminalClipboard(terminal, 'copy-succeeded'),
      (error: unknown) => {
        if (import.meta.env.DEV) {
          console.warn('[TerminalView][clipboard] copy-failed', error)
        }
      }
    )
  }

  const runPaste = async () => {
    const terminal = terminalRef.current
    if (!terminal) {
      return
    }
    // Focus before the asynchronous clipboard read. This is important for
    // Debian/WebKitGTK where the paste target is otherwise the Dock button or
    // the portal context-menu surface when the native read resolves.
    terminal.focus()
    try {
      const value = await readClipboardText()
      if (value) {
        terminal.focus()
        clearEphemeralHighlight()
        terminal.paste(value)
      }
    } catch {
      // Image-only/locked clipboards are a normal empty-paste case on native
      // desktops. Keep the terminal usable and avoid an unhandled rejection.
    } finally {
      terminal.focus()
    }
  }

  const searchTerminal = (query: string, direction: 1 | -1 = 1) => {
    const searchAddon = searchAddonRef.current
    if (!query) {
      setFindMiss(false)
      setFindMatchCount(0)
      setActiveFindIndex(-1)
      clearFindSelection()
      return false
    }

    if (!searchAddon) {
      setFindMiss(true)
      setFindMatchCount(0)
      setActiveFindIndex(-1)
      clearFindSelection()
      return false
    }

    try {
      const found =
        direction === -1
          ? searchAddon.findPrevious(query, buildSearchOptions())
          : searchAddon.findNext(query, buildSearchOptions())

      if (!found) {
        setFindMiss(true)
        setFindMatchCount(0)
        setActiveFindIndex(-1)
        clearFindSelection()
      }

      return found
    } catch {
      setFindMiss(true)
      setFindMatchCount(0)
      setActiveFindIndex(-1)
      clearFindSelection()
      return false
    }
  }

  const openFind = () => {
    setContextMenu(null)
    setFindOpen(true)
    setFindMiss(false)
  }

  const runFind = () => {
    openFind()
  }

  const runClear = () => {
    const terminal = terminalRef.current
    if (!terminal) {
      return
    }
    terminal.clear()
    terminal.focus()
  }

  const runSaveSessionLog = async () => {
    const desktopApi = window.fileterm
    if (!desktopApi) {
      return
    }

    try {
      const savedPath = await desktopApi.saveSessionLog(tabIdRef.current)
      if (savedPath) {
        onStatusRef.current?.(`${t.sessionLogSaved}: ${savedPath}`)
      }
    } catch (error) {
      onStatusRef.current?.(`${t.sessionLogSaveFailed}: ${error instanceof Error ? error.message : String(error)}`)
    }
  }

  const runSplitPane = (direction: SplitPaneDirection) => {
    onSplitPaneRef.current?.(direction)
  }

  const runClosePane = () => {
    onClosePaneRef.current?.()
  }

  const runCloseTab = () => {
    onCloseTabRef.current?.()
  }

  const flushPendingWrite = () => {
    writeFrameRef.current = null
    const terminal = terminalRef.current
    if (!terminal) {
      pendingWriteRef.current = ''
      return
    }

    if (!pendingWriteRef.current) {
      return
    }

    if (isWritingRef.current) {
      writeFrameRef.current = window.requestAnimationFrame(flushPendingWrite)
      return
    }

    const nextChunk = pendingWriteRef.current.slice(0, TERMINAL_WRITE_FRAME_BUDGET)
    pendingWriteRef.current = pendingWriteRef.current.slice(nextChunk.length)
    isWritingRef.current = true
    terminal.write(nextChunk, () => {
      isWritingRef.current = false
      if (pendingWriteRef.current && writeFrameRef.current === null) {
        writeFrameRef.current = window.requestAnimationFrame(flushPendingWrite)
      }
    })
  }

  const scheduleTerminalWrite = (text: string) => {
    if (!text) {
      return
    }

    pendingWriteRef.current += text
    if (writeFrameRef.current === null) {
      writeFrameRef.current = window.requestAnimationFrame(flushPendingWrite)
    }
  }

  const buildExitAlternateScreenSequence = () => '\x1b[?1049l\x1b[?1047l\x1b[?47l\x1b[?25h'

  const snapshotTerminalBuffer = (terminal: Terminal) => {
    const lines: string[] = []
    const buffer = terminal.buffer.active

    for (let row = 0; row < buffer.length; row += 1) {
      lines.push(buffer.getLine(row)?.translateToString(true) ?? '')
    }

    while (lines.length > 0 && lines[lines.length - 1] === '') {
      lines.pop()
    }

    return lines.join('\r\n')
  }

  const appendRenderedTranscript = (chunk: string) => {
    if (!chunk) {
      return
    }

    renderedTranscriptRef.current = trimTranscript(`${renderedTranscriptRef.current}${chunk}`)
  }

  const formatTerminalChunk = (terminal: Terminal | null, value: string) => {
    const displayText = toDisplayTerminalText(value)
    return displayText
  }

  const replaceTerminalWithTranscript = (terminal: Terminal, transcript: string) => {
    renderedTranscriptRef.current = trimTranscript(transcript)
    pendingWriteRef.current = ''
    if (writeFrameRef.current !== null) {
      window.cancelAnimationFrame(writeFrameRef.current)
      writeFrameRef.current = null
    }
    isWritingRef.current = false
    const replayGeneration = transcriptReplayGenerationRef.current + 1
    transcriptReplayGenerationRef.current = replayGeneration
    replayingTranscriptRef.current = true
    terminal.reset()
    const replayText = formatTerminalChunk(terminal, renderedTranscriptRef.current)
    logTerminalRender(terminal, 'transcript-replay-start', {
      tabId: tabIdRef.current,
      replayGeneration,
      transcriptLength: renderedTranscriptRef.current.length,
      replayLength: replayText.length
    })
    if (replayText) {
      terminal.write(replayText, () => {
        if (transcriptReplayGenerationRef.current === replayGeneration) {
          replayingTranscriptRef.current = false
        }
        logTerminalRender(terminal, 'transcript-replay-complete', {
          tabId: tabIdRef.current,
          replayGeneration,
          currentReplayGeneration: transcriptReplayGenerationRef.current
        })
      })
    } else {
      replayingTranscriptRef.current = false
      logTerminalRender(terminal, 'transcript-replay-complete', {
        tabId: tabIdRef.current,
        replayGeneration,
        currentReplayGeneration: transcriptReplayGenerationRef.current
      })
    }
    suppressHydratedChunksUntilRef.current = Date.now() + 1500
  }

  const shouldHydrateTranscript = (currentTranscript: string, nextTranscript: string, connected: boolean) => {
    if (!nextTranscript || nextTranscript === currentTranscript) {
      return false
    }

    if (preserveVisibleBufferRef.current && currentTranscript) {
      return false
    }

    if (!currentTranscript) {
      return true
    }

    if (connected) {
      return false
    }

    if (nextTranscript.length < currentTranscript.length) {
      return true
    }

    if (!nextTranscript.startsWith(currentTranscript)) {
      return true
    }

    return true
  }

  const syncTerminalSize = useCallback(
    (
      fitAddon: FitAddon,
      terminal: Terminal,
      options: {
        force?: boolean
        freezeCols?: boolean
        preserveVisibleBuffer?: boolean
      } = {}
    ) => {
      const { force = false, freezeCols = false } = options
      const host = hostRef.current
      if (!host) {
        return
      }

      const { width, height } = host.getBoundingClientRect()
      if (width <= 0 || height <= 0) {
        return
      }

      const proposed = fitAddon.proposeDimensions()
      if (!proposed) {
        return
      }

      const displayCols = Math.max(1, proposed.cols)
      const rows = Math.max(1, proposed.rows - TERMINAL_FIT_GUARD_ROWS)
      const previousSize = lastSyncedSizeRef.current
      const liveCols = Math.max(1, displayCols - TERMINAL_REMOTE_GUARD_COLS)
      const cols = freezeCols && previousSize ? previousSize.cols : liveCols
      if (terminal.cols !== cols || terminal.rows !== rows) {
        terminal.resize(cols, rows)
        terminal.refresh(0, Math.max(terminal.rows - 1, 0))
      }

      const nextSize = {
        cols: terminal.cols,
        rows: terminal.rows,
        width: Math.floor(width),
        height: Math.floor(height)
      }
      const remoteGridChanged =
        !previousSize || previousSize.cols !== nextSize.cols || previousSize.rows !== nextSize.rows
      if (
        !force &&
        previousSize &&
        previousSize.cols === nextSize.cols &&
        previousSize.rows === nextSize.rows &&
        Math.abs(previousSize.width - nextSize.width) <= TERMINAL_RESIZE_PIXEL_EPSILON &&
        Math.abs(previousSize.height - nextSize.height) <= TERMINAL_RESIZE_PIXEL_EPSILON
      ) {
        return
      }
      lastSyncedSizeRef.current = nextSize
      if (!remoteGridChanged) {
        return
      }

      void window.fileterm?.resizeTerminal(
        tabIdRef.current,
        nextSize.cols,
        nextSize.rows,
        nextSize.width,
        nextSize.height
      )
    },
    []
  )

  const recoverTerminalRender = useCallback(
    (reason: 'activation'): boolean => {
      // A tab can become hidden again between the two activation animation
      // frames. Do not let a stale recovery clear, resize, or focus that
      // terminal after a rapid tab switch.
      if (!isActiveRef.current) {
        return false
      }

      const terminal = terminalRef.current
      const fitAddon = fitAddonRef.current
      const host = hostRef.current
      if (!terminal || !fitAddon || !host) {
        return true
      }

      const { width, height } = host.getBoundingClientRect()
      if (width <= 0 || height <= 0) {
        logTerminalRender(terminal, 'recovery-skipped-zero-host', {
          tabId: tabIdRef.current,
          reason,
          hostWidth: Math.round(width),
          hostHeight: Math.round(height)
        })
        return true
      }

      terminal.clearTextureAtlas()
      syncTerminalSize(fitAddon, terminal, { force: true })
      if (!isActiveRef.current) {
        return false
      }
      terminal.refresh(0, Math.max(terminal.rows - 1, 0))
      terminal.focus()
      logTerminalRender(terminal, 'recovery-complete', { tabId: tabIdRef.current, reason })
      return false
    },
    [syncTerminalSize]
  )

  const applyTerminalFontSize = (fontSize: number) => {
    const terminal = terminalRef.current
    if (!terminal) {
      return false
    }

    const nextSize = Math.max(TERMINAL_MIN_FONT_SIZE, Math.min(TERMINAL_MAX_FONT_SIZE, fontSize))
    if (terminal.options.fontSize === nextSize) {
      return false
    }

    terminal.options.fontSize = nextSize
    terminal.clearTextureAtlas()
    // Font metrics change the visible grid, so keep the remote PTY's size in
    // lockstep with this terminal rather than applying WebView zoom.
    lastSyncedSizeRef.current = null
    const fitAddon = fitAddonRef.current
    if (fitAddon) {
      syncTerminalSize(fitAddon, terminal, { force: true })
    }
    return true
  }

  useTerminalLifecycle({
    isMac,
    isWindowsPty,
    isActive,
    bootText: hydratedBootText,
    hostRef,
    setTerminalScrollableElement,
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
    isActiveRef,
    serialTransferBusyRef,
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
    runSplitPane,
    runClosePane,
    runCloseTab
  })

  useEffect(() => {
    if (!isActive) {
      return
    }

    let disposed = false
    let recoveryAttempts = 0
    let recoveryFrame: number | null = null
    const attemptRecovery = () => {
      if (disposed || !isActiveRef.current) {
        return
      }

      recoveryAttempts += 1
      const shouldRetry = recoverTerminalRender('activation')
      if (shouldRetry && recoveryAttempts < TERMINAL_ACTIVATION_RECOVERY_ATTEMPTS) {
        recoveryFrame = window.requestAnimationFrame(attemptRecovery)
      }
    }

    const firstFrame = window.requestAnimationFrame(() => {
      recoveryFrame = window.requestAnimationFrame(attemptRecovery)
    })

    return () => {
      disposed = true
      window.cancelAnimationFrame(firstFrame)
      if (recoveryFrame !== null) {
        window.cancelAnimationFrame(recoveryFrame)
      }
    }
  }, [isActive, recoverTerminalRender])

  useEffect(() => {
    bootTextRef.current = hydratedBootText

    // connected 提升必须放在提前 return 之前。
    //
    // 切 tab 流程：tab A(connected) → tab B(未连接) → 切回 tab A。
    //   1. 切到 B 时 onTerminalState 的 connected=false 会把 wasConnectedRef 写成 false
    //   2. 切回 A 时，React 中间态 connected 可能短暂为 false，第 1017 行不触发
    //   3. snapshot apply 完成后 connected 变 true，effect 因 connected dep 再次触发
    //   4. 但 activeTerminalTabIdRef.current === tabId（都是 A），提前 return，
    //      wasConnectedRef.current = true 永远不执行
    //   5. A 已连接，main 不再发 terminal:state，wasConnectedRef 永久卡 false
    //   6. onData 走 !wasConnectedRef 分支吞掉所有输入——"切回来无法输入"
    //
    // 把提升逻辑提到 return 之前：只要 connected 为 true，无论是否切换 tab，
    // 都把 wasConnectedRef 从 false 提升到 true。真正的断开仍由 onTerminalState
    // (line 823) 权威设置，这里只做单向提升，不会反向覆盖。
    if (connected) {
      wasConnectedRef.current = true
    }

    if (activeTerminalTabIdRef.current === tabId) {
      return
    }

    activeTerminalTabIdRef.current = tabId
    const terminal = terminalRef.current
    const host = hostRef.current
    if (!terminal || !host) {
      return
    }

    preserveVisibleBufferRef.current = false
    awaitingCommandCompletionRef.current = false
    pendingPromptResizeRef.current = false
    replaceTerminalWithTranscript(terminal, hydratedBootText)
    lastSyncedSizeRef.current = null

    const { width, height } = host.getBoundingClientRect()
    if (width > 0 && height > 0) {
      void window.fileterm?.resizeTerminal(tabId, terminal.cols, terminal.rows, Math.floor(width), Math.floor(height))
    }
  }, [connected, hydratedBootText, tabId])

  useEffect(() => {
    const applyCurrentProfileFontSize = () => {
      if (profileIdRef.current !== profileId) {
        return
      }

      applyTerminalFontSize(getTerminalFontSize(profileId))
    }

    applyCurrentProfileFontSize()
    return subscribeTerminalFontSizes(applyCurrentProfileFontSize)
  }, [profileId])

  useEffect(() => {
    bootTextRef.current = hydratedBootText
    const terminal = terminalRef.current
    if (
      !terminal ||
      connected ||
      !shouldHydrateTranscript(renderedTranscriptRef.current, hydratedBootText, wasConnectedRef.current)
    ) {
      return
    }

    replaceTerminalWithTranscript(terminal, hydratedBootText)
  }, [connected, hydratedBootText])

  useEffect(() => {
    if (!terminalRef.current) {
      return
    }

    applyTerminalTheme()
  }, [findOpen, findQuery])

  useEffect(() => {
    const root = document.documentElement
    const observer = new MutationObserver(() => {
      applyTerminalTheme()
      const terminal = terminalRef.current
      const configuredFontFamily = getConfiguredMonoFontFamily()
      if (terminal && terminal.options.fontFamily !== configuredFontFamily) {
        terminal.options.fontFamily = configuredFontFamily
        terminal.clearTextureAtlas()
        terminal.refresh(0, Math.max(terminal.rows - 1, 0))
      }
    })

    observer.observe(root, {
      attributes: true,
      attributeFilter: ['data-theme', 'style', 'class']
    })

    return () => observer.disconnect()
  }, [findOpen, findQuery])

  useEffect(() => {
    if (!findOpen) {
      return
    }

    const frame = window.requestAnimationFrame(() => {
      findInputRef.current?.focus()
      findInputRef.current?.select()
    })

    return () => window.cancelAnimationFrame(frame)
  }, [findOpen])

  useEffect(() => {
    if (!findOpen) {
      return
    }

    if (!findQuery) {
      setFindMiss(false)
      setFindMatchCount(0)
      setActiveFindIndex(-1)
      clearFindSelection()
      return
    }

    const searchAddon = searchAddonRef.current
    if (!searchAddon) {
      setFindMiss(true)
      setFindMatchCount(0)
      setActiveFindIndex(-1)
      clearFindSelection()
      return
    }

    try {
      const found = searchAddon.findNext(findQuery, buildSearchOptions(true))
      if (!found) {
        setFindMiss(true)
        setFindMatchCount(0)
        setActiveFindIndex(-1)
        clearFindSelection()
      }
    } catch {
      setFindMiss(true)
      setFindMatchCount(0)
      setActiveFindIndex(-1)
      clearFindSelection()
    }
  }, [findCaseSensitive, findOpen, findQuery, findRegex])

  return {
    hostRef,
    terminalScrollController,
    findInputRef,
    hasSelection,
    contextMenu,
    findOpen,
    findQuery,
    findMiss,
    findMatchCount,
    activeFindIndex,
    findCaseSensitive,
    findRegex,
    shortcuts,
    setContextMenu,
    setFindQuery,
    setFindMiss,
    setActiveFindIndex,
    setFindCaseSensitive,
    setFindRegex,
    closeFind,
    searchTerminal,
    runCopy,
    runPaste,
    runFind,
    runSaveSessionLog,
    runClear,
    runSplitPane,
    runClosePane
  }
}
