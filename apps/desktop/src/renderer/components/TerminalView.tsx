import { useEffect, useRef, useState } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { SearchAddon } from '@xterm/addon-search'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import { WebLinksAddon } from '@xterm/addon-web-links'
import '@xterm/xterm/css/xterm.css'
import { copyText } from '../app/app-utils'
import { t } from '../i18n'
import { ContextMenu } from '../features/common/ContextMenu'

function localizeTerminalText(value: string) {
  return value
    .replaceAll('连接主机成功', t.terminalConnected)
    .replaceAll('连接主机...', t.terminalConnecting)
    .replaceAll('连接已断开', t.terminalDisconnected)
    .replaceAll('[connection closed]', t.terminalConnectionClosed)
    .replaceAll('Shell closed', t.terminalDisconnected)
    .replace(/连接失败:\s*/g, t.connectionFailedPrefix)
    .replace(/Connection error:\s*/g, t.connectionFailedPrefix)
    .replace(/Disconnected from\s*/g, t.disconnectedFromPrefix)
    .replace(/\bDisconnected\b/g, t.disconnected)
}

function toDisplayTerminalText(value: string) {
  // Localize fixed TermDock notices before preserving terminal control semantics later.
  return localizeTerminalText(value)
}

function splitOscPayload(payload: string) {
  const separatorIndex = payload.indexOf(';')
  if (separatorIndex === -1) {
    return null
  }

  return {
    target: payload.slice(0, separatorIndex),
    data: payload.slice(separatorIndex + 1)
  }
}

function isOsc52TargetSupported(target: string) {
  return target === ''
    || /[cpsq01234567]/.test(target)
}

function decodeBase64Utf8(value: string) {
  try {
    const normalized = value.replace(/\s+/g, '')
    const bytes = Uint8Array.from(atob(normalized), (char) => char.charCodeAt(0))
    return new TextDecoder().decode(bytes)
  } catch {
    return null
  }
}

function encodeBase64Utf8(value: string) {
  const bytes = new TextEncoder().encode(value)
  let binary = ''
  for (const byte of bytes) {
    binary += String.fromCharCode(byte)
  }
  return btoa(binary)
}

const TERMINAL_TRANSCRIPT_LIMIT = 200_000
const TERMINAL_REMOTE_GUARD_COLS = 2
const TERMINAL_FIT_GUARD_ROWS = 1
const TERMINAL_RESIZE_PIXEL_EPSILON = 2
const TERMINAL_RESIZE_SETTLE_MS = 140
const TERMINAL_RESIZE_OUTPUT_QUIET_MS = 260
const TERMINAL_SEARCH_DECORATIONS = {
  matchBackground: '#4b5563',
  matchOverviewRuler: '#9ca3af',
  activeMatchBackground: '#f3f4f6',
  activeMatchColorOverviewRuler: '#f3f4f6'
}

function trimTranscript(transcript: string) {
  if (transcript.length <= TERMINAL_TRANSCRIPT_LIMIT) {
    return transcript
  }

  return transcript.slice(transcript.length - TERMINAL_TRANSCRIPT_LIMIT)
}

function getLastVisibleTerminalLine(terminal: Terminal) {
  const buffer = terminal.buffer.active
  for (let row = buffer.length - 1; row >= 0; row -= 1) {
    const line = buffer.getLine(row)?.translateToString(false) ?? ''
    const normalized = line.trimEnd()
    if (normalized) {
      return normalized
    }
  }
  return ''
}

function looksLikeShellPrompt(line: string) {
  if (!line) {
    return false
  }

  return [
    /(?:^|\s)[A-Za-z0-9_.-]+@[A-Za-z0-9_.-]+(?::[^\n]*)?[#$%>]$/,
    /^\[[^\]]+@[^\]]+\][#$]$/,
    /^[#$%>]$/
  ].some((pattern) => pattern.test(line))
}

export function TerminalView({
  tabId,
  bootText,
  connected = false,
  onStatus
}: {
  tabId: string
  bootText: string
  connected?: boolean
  onStatus?(message: string | null): void
}) {
  const hostRef = useRef<HTMLDivElement | null>(null)
  const terminalRef = useRef<Terminal | null>(null)
  const searchAddonRef = useRef<SearchAddon | null>(null)
  const findInputRef = useRef<HTMLInputElement | null>(null)
  const bootTextRef = useRef(bootText)
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
  const lastSyncedSizeRef = useRef<{ cols: number; rows: number; width: number; height: number } | null>(null)
  const lastObservedHostRectRef = useRef<{ width: number; height: number } | null>(null)
  const isHorizontalResizeActiveRef = useRef(false)
  const lastTerminalOutputAtRef = useRef(0)
  const awaitingCommandCompletionRef = useRef(false)
  const pendingPromptResizeRef = useRef(false)
  const [hasSelection, setHasSelection] = useState(false)
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null)
  const [findOpen, setFindOpen] = useState(false)
  const [findQuery, setFindQuery] = useState('')
  const [findMiss, setFindMiss] = useState(false)
  const [findMatchCount, setFindMatchCount] = useState(0)
  const [activeFindIndex, setActiveFindIndex] = useState(-1)
  const [findCaseSensitive, setFindCaseSensitive] = useState(false)
  const [findRegex, setFindRegex] = useState(false)
  const isMac = window.termdock?.platform === 'darwin'

  const shortcuts = {
    copy: isMac ? '⌘C' : 'Ctrl+Shift+C',
    paste: isMac ? '⌘V' : 'Ctrl+Shift+V',
    find: isMac ? '⌘F' : 'Ctrl+F'
  }

  const readColor = (name: string, fallback: string) =>
    getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback

  const buildTerminalTheme = () => ({
    background: readColor('--terminal-bg', '#1e1e1e'),
    foreground: readColor('--terminal-text', '#e0e0e0'),
    cursor: readColor('--terminal-text', '#e0e0e0'),
    green: readColor('--success', '#39d98a'),
    brightGreen: readColor('--success', '#52f2a0'),
    blue: readColor('--accent-text', '#c8d0da'),
    brightBlue: readColor('--text-main', '#f1f5f9'),
    selectionBackground: readColor(
      '--terminal-cmd-bg',
      'rgba(148, 163, 184, 0.24)'
    ),
    selectionForeground: readColor('--terminal-text', '#e0e0e0')
  })

  const applyTerminalTheme = () => {
    const terminal = terminalRef.current
    if (!terminal) {
      return
    }
    terminal.options.theme = buildTerminalTheme()
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
    decorations: TERMINAL_SEARCH_DECORATIONS
  })

  const runCopy = () => {
    const terminal = terminalRef.current
    if (!terminal) {
      return
    }
    const selection = terminal.getSelection()
    if (!selection) {
      return
    }
    if (window.termdock?.writeClipboardText) {
      void window.termdock.writeClipboardText(selection)
    } else {
      copyText(selection)
    }
    terminal.focus()
  }

  const runPaste = async () => {
    const terminal = terminalRef.current
    if (!terminal) {
      return
    }
    const value = window.termdock?.readClipboardText
      ? await window.termdock.readClipboardText()
      : await navigator.clipboard?.readText?.()
    if (value) {
      clearEphemeralHighlight()
      terminal.paste(value)
    }
    terminal.focus()
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
      const found = direction === -1
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

    const nextChunk = pendingWriteRef.current
    pendingWriteRef.current = ''
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

  const buildExitAlternateScreenSequence = () =>
    '\x1b[?1049l\x1b[?1047l\x1b[?47l\x1b[?25h'

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
    terminal.reset()
    terminal.write(formatTerminalChunk(terminal, renderedTranscriptRef.current))
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

  const syncTerminalSize = (
    fitAddon: FitAddon,
    terminal: Terminal,
    options: {
      force?: boolean
      freezeCols?: boolean
      preserveVisibleBuffer?: boolean
    } = {}
  ) => {
    const { force = false, freezeCols = false, preserveVisibleBuffer = false } = options
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

    // Keep xterm and the remote PTY on the exact same column count. Readline,
    // vim, nano and progress bars all depend on that agreement for wrapping
    // and cursor-addressing. During an active horizontal resize we temporarily
    // freeze cols, then sync the true width once the drag settles.
    const displayCols = Math.max(1, proposed.cols)
    const rows = Math.max(1, proposed.rows - TERMINAL_FIT_GUARD_ROWS)
    const previousSize = lastSyncedSizeRef.current
    const liveCols = Math.max(1, displayCols - TERMINAL_REMOTE_GUARD_COLS)
    const cols = freezeCols && previousSize
      ? previousSize.cols
      : liveCols
    const shouldPreserveVisibleBuffer = preserveVisibleBuffer && previousSize && previousSize.cols !== cols
    const visibleBufferSnapshot = shouldPreserveVisibleBuffer
      ? snapshotTerminalBuffer(terminal)
      : ''
    if (terminal.cols !== cols || terminal.rows !== rows) {
      terminal.resize(cols, rows)
      if (shouldPreserveVisibleBuffer && visibleBufferSnapshot) {
        renderedTranscriptRef.current = trimTranscript(visibleBufferSnapshot)
        pendingWriteRef.current = ''
        terminal.reset()
        terminal.write(visibleBufferSnapshot)
        suppressHydratedChunksUntilRef.current = Date.now() + 300
      }
      terminal.refresh(0, Math.max(terminal.rows - 1, 0))
    }

    const nextSize = {
      cols: terminal.cols,
      rows: terminal.rows,
      width: Math.floor(width),
      height: Math.floor(height)
    }
    if (
      !force
      && previousSize
      && previousSize.cols === nextSize.cols
      && previousSize.rows === nextSize.rows
      && Math.abs(previousSize.width - nextSize.width) <= TERMINAL_RESIZE_PIXEL_EPSILON
      && Math.abs(previousSize.height - nextSize.height) <= TERMINAL_RESIZE_PIXEL_EPSILON
    ) {
      return
    }
    lastSyncedSizeRef.current = nextSize

    void window.termdock?.resizeTerminal(
      tabId,
      nextSize.cols,
      nextSize.rows,
      nextSize.width,
      nextSize.height
    )
  }

  useEffect(() => {
    if (!hostRef.current) {
      return
    }

    const terminal = new Terminal({
      fontFamily: '"SF Mono", Menlo, Consolas, monospace',
      fontSize: 13,
      lineHeight: 1.05,
      cursorBlink: true,
      allowProposedApi: true,
      allowTransparency: true,
      reflowCursorLine: false,
      scrollback: 6000,
      theme: buildTerminalTheme()
    })
    const fitAddon = new FitAddon()
    const searchAddon = new SearchAddon({ highlightLimit: 2000 })
    const unicode11Addon = new Unicode11Addon()
    const webLinksAddon = new WebLinksAddon((_event, uri) => {
      void window.termdock?.openExternalUrl(uri)
    })
    terminal.loadAddon(fitAddon)
    terminal.loadAddon(searchAddon)
    terminal.loadAddon(unicode11Addon)
    terminal.loadAddon(webLinksAddon)
    terminal.unicode.activeVersion = '11'
    terminal.open(hostRef.current)
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

      if (parsed.data === '?') {
        const clipboardText = window.termdock?.readClipboardText
          ? await window.termdock.readClipboardText()
          : await navigator.clipboard?.readText?.() ?? ''
        const encoded = encodeBase64Utf8(clipboardText)
        await window.termdock?.writeTerminal(tabId, `\u001b]52;${parsed.target || 'c'};${encoded}\u0007`)
        return true
      }

      const decoded = decodeBase64Utf8(parsed.data)
      if (decoded === null) {
        return false
      }

      if (window.termdock?.writeClipboardText) {
        await window.termdock.writeClipboardText(decoded)
      } else {
        copyText(decoded)
      }
      return true
    })

    syncTerminalSize(fitAddon, terminal)

    if (bootTextRef.current) {
      replaceTerminalWithTranscript(terminal, bootTextRef.current)
    }

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
      if (data.includes('\r') || data.includes('\n')) {
        awaitingCommandCompletionRef.current = true
      }
      clearEphemeralHighlight()
      setContextMenu(null)
      void window.termdock?.writeTerminal(tabId, data)
    })

    const onSelectionDispose = terminal.onSelectionChange(() => {
      setHasSelection(terminal.hasSelection())
    })

    const offData = window.termdock?.onTerminalData(({ tabId: nextTabId, chunk }) => {
      if (nextTabId === tabId) {
        lastTerminalOutputAtRef.current = Date.now()
        if (Date.now() < suppressHydratedChunksUntilRef.current && renderedTranscriptRef.current.endsWith(chunk)) {
          return
        }
        appendRenderedTranscript(chunk)
        clearEphemeralHighlight()
        scheduleTerminalWrite(formatTerminalChunk(terminal, chunk))
        if (pendingPromptResizeRef.current) {
          scheduleSettledHorizontalResize()
        }
      }
    })

    const offState = window.termdock?.onTerminalState(({ tabId: nextTabId, summary, transcript, connected }) => {
      if (nextTabId === tabId) {
        onStatus?.(localizeTerminalText(summary))
        const isDisconnecting = wasConnectedRef.current && !connected
        if (isDisconnecting) {
          preserveVisibleBufferRef.current = true
        }
        if (shouldHydrateTranscript(renderedTranscriptRef.current, transcript, connected)) {
          replaceTerminalWithTranscript(terminal, transcript)
        }
        if (!wasConnectedRef.current && connected) {
          preserveVisibleBufferRef.current = false
          awaitingCommandCompletionRef.current = false
          pendingPromptResizeRef.current = false
          window.requestAnimationFrame(() => scheduleResize(true))
        }
        if (isDisconnecting) {
          terminal.write(buildExitAlternateScreenSequence(), () => {
            const visibleTranscript = snapshotTerminalBuffer(terminal)
            const disconnectedTranscript = visibleTranscript
              ? `${visibleTranscript}\r\n${t.terminalConnectionClosed}\r\n`
              : `${t.terminalConnectionClosed}\r\n`
            replaceTerminalWithTranscript(terminal, disconnectedTranscript)
          })
        }
        wasConnectedRef.current = connected
      }
    })

    const resizeObserver = new ResizeObserver(() => {
      const host = hostRef.current
      if (!host) {
        return
      }

      const { width, height } = host.getBoundingClientRect()
      const lastObservedRect = lastObservedHostRectRef.current
      lastObservedHostRectRef.current = { width, height }

      const widthChanged = Boolean(
        lastObservedRect
        && Math.abs(lastObservedRect.width - width) > TERMINAL_RESIZE_PIXEL_EPSILON
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

    const onContextMenu = (event: MouseEvent) => {
      event.preventDefault()
      setHasSelection(terminal.hasSelection())
      setContextMenu({ x: event.clientX, y: event.clientY })
    }

    const onKeyDown = (event: KeyboardEvent) => {
      if (document.activeElement !== terminal.textarea) {
        return
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

      if (matchesCopy) {
        event.preventDefault()
        runCopy()
        return
      }

      if (matchesPaste) {
        event.preventDefault()
        void runPaste()
        return
      }

      if (matchesFind) {
        event.preventDefault()
        openFind()
        return
      }

      if (!isMac && event.ctrlKey && !event.shiftKey && event.key.toLowerCase() === 'l') {
        event.preventDefault()
        runClear()
      }
    }

    hostRef.current.addEventListener('contextmenu', onContextMenu)
    window.addEventListener('keydown', onKeyDown)
    window.addEventListener('focus', onWindowFocus)
    document.addEventListener('visibilitychange', onVisibilityChange)

    // Ask the main process for the actual PTY size once the terminal is mounted.
    if (!bootedTabs.current.has(tabId)) {
      bootedTabs.current.add(tabId)
      resize()
    }

    return () => {
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
      preserveVisibleBufferRef.current = false
      lastSyncedSizeRef.current = null
      lastObservedHostRectRef.current = null
      isHorizontalResizeActiveRef.current = false
      lastTerminalOutputAtRef.current = 0
      awaitingCommandCompletionRef.current = false
      pendingPromptResizeRef.current = false
      searchResultsDisposable.dispose()
      osc52Disposable.dispose()
      resizeObserver.disconnect()
      hostRef.current?.removeEventListener('contextmenu', onContextMenu)
      window.removeEventListener('keydown', onKeyDown)
      window.removeEventListener('focus', onWindowFocus)
      document.removeEventListener('visibilitychange', onVisibilityChange)
      searchAddonRef.current = null
      terminalRef.current = null
      terminal.dispose()
    }
  }, [isMac, onStatus, tabId])

  useEffect(() => {
    bootTextRef.current = bootText
    const terminal = terminalRef.current
    if (
      !terminal
      || connected
      || !shouldHydrateTranscript(renderedTranscriptRef.current, bootText, wasConnectedRef.current)
    ) {
      return
    }

    replaceTerminalWithTranscript(terminal, bootText)
  }, [bootText, connected])

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

  return (
    <>
      <div className="terminal-host">
        <div className="terminal-inner" ref={hostRef} />
      </div>
      {findOpen ? (
        <div className="terminal-find" onClick={(event) => event.stopPropagation()}>
          <input
            ref={findInputRef}
            type="text"
            value={findQuery}
            onChange={(event) => {
              setFindQuery(event.target.value)
              setFindMiss(false)
              setActiveFindIndex(-1)
            }}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                event.preventDefault()
                searchTerminal(findQuery, event.shiftKey ? -1 : 1)
              }
              if (event.key === 'Escape') {
                event.preventDefault()
                closeFind()
              }
            }}
            placeholder={t.find}
          />
          <div className="terminal-find-count" aria-live="polite">
            {findQuery && findMatchCount > 0 ? `${Math.max(activeFindIndex + 1, 1)}/${findMatchCount}` : null}
          </div>
          <button
            type="button"
            className={findCaseSensitive ? 'is-active' : undefined}
            aria-pressed={findCaseSensitive}
            title={t.findCaseSensitive}
            onClick={() => setFindCaseSensitive((value) => !value)}
          >
            Aa
          </button>
          <button
            type="button"
            className={findRegex ? 'is-active' : undefined}
            aria-pressed={findRegex}
            title={t.findRegex}
            onClick={() => setFindRegex((value) => !value)}
          >
            .*
          </button>
          <button type="button" title={t.findPrevious} onClick={() => searchTerminal(findQuery, -1)}>↑</button>
          <button type="button" title={t.findNext} onClick={() => searchTerminal(findQuery, 1)}>↓</button>
          <button type="button" onClick={() => searchTerminal(findQuery, 1)}>{t.find}</button>
          <button type="button" onClick={closeFind}>×</button>
          {findMiss ? <span className="terminal-find-status">{t.findNotFound}</span> : null}
        </div>
      ) : null}
      {contextMenu ? (
        <ContextMenu
          className="terminal-context-menu"
          items={[
            { label: t.copy, shortcut: shortcuts.copy, disabled: !hasSelection, action: runCopy },
            { label: t.paste, shortcut: shortcuts.paste, action: () => void runPaste() },
            { separator: true },
            { label: t.find, shortcut: shortcuts.find, action: runFind },
            { label: t.clearScreen, action: runClear }
          ]}
          onClose={() => setContextMenu(null)}
          position={contextMenu}
        />
      ) : null}
    </>
  )
}
