import { useEffect, useRef, useState } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
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

function isWideCodePoint(codePoint: number) {
  return (
    (codePoint >= 0x1100 && codePoint <= 0x115f)
    || (codePoint >= 0x2329 && codePoint <= 0x232a)
    || (codePoint >= 0x2e80 && codePoint <= 0xa4cf && codePoint !== 0x303f)
    || (codePoint >= 0xac00 && codePoint <= 0xd7a3)
    || (codePoint >= 0xf900 && codePoint <= 0xfaff)
    || (codePoint >= 0xfe10 && codePoint <= 0xfe19)
    || (codePoint >= 0xfe30 && codePoint <= 0xfe6f)
    || (codePoint >= 0xff00 && codePoint <= 0xff60)
    || (codePoint >= 0xffe0 && codePoint <= 0xffe6)
    || (codePoint >= 0x1f300 && codePoint <= 0x1faf6)
    || (codePoint >= 0x20000 && codePoint <= 0x3fffd)
  )
}

function getTerminalCellWidth(value: string) {
  return Array.from(value).reduce((width, char) => {
    const codePoint = char.codePointAt(0) ?? 0
    return width + (isWideCodePoint(codePoint) ? 2 : 1)
  }, 0)
}

function rowsForDisplayWidth(width: number, cols: number) {
  return Math.max(1, Math.ceil(Math.max(0, width) / Math.max(1, cols)))
}

const TERMINAL_TRANSCRIPT_LIMIT = 200_000
const TERMINAL_FIT_GUARD_ROWS = 1

function trimTranscript(transcript: string) {
  if (transcript.length <= TERMINAL_TRANSCRIPT_LIMIT) {
    return transcript
  }

  return transcript.slice(transcript.length - TERMINAL_TRANSCRIPT_LIMIT)
}

export function TerminalView({
  tabId,
  bootText,
  onStatus
}: {
  tabId: string
  bootText: string
  onStatus?(message: string | null): void
}) {
  const hostRef = useRef<HTMLDivElement | null>(null)
  const terminalRef = useRef<Terminal | null>(null)
  const findInputRef = useRef<HTMLInputElement | null>(null)
  const bootTextRef = useRef(bootText)
  const renderedTranscriptRef = useRef('')
  const pendingWriteRef = useRef('')
  const writeFrameRef = useRef<number | null>(null)
  const isWritingRef = useRef(false)
  const activeInlineRedrawRef = useRef(false)
  const inlineRedrawContentRef = useRef('')
  const inlineRedrawRowsRef = useRef(1)
  const suppressHydratedChunksUntilRef = useRef(0)
  const preserveVisibleBufferRef = useRef(false)
  const bootedTabs = useRef(new Set<string>())
  const wasConnectedRef = useRef(false)
  const lastSyncedSizeRef = useRef<{ cols: number; rows: number; width: number; height: number } | null>(null)
  const [hasSelection, setHasSelection] = useState(false)
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null)
  const [findOpen, setFindOpen] = useState(false)
  const [findQuery, setFindQuery] = useState('')
  const [findMiss, setFindMiss] = useState(false)
  const [findMatchCount, setFindMatchCount] = useState(0)
  const [activeFindIndex, setActiveFindIndex] = useState(-1)
  const isMac = window.termdock?.platform === 'darwin'

  const shortcuts = {
    copy: isMac ? '⌘C' : 'Ctrl+Shift+C',
    paste: isMac ? '⌘V' : 'Ctrl+Shift+V',
    find: isMac ? '⌘F' : 'Ctrl+F'
  }

  const readColor = (name: string, fallback: string) =>
    getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback

  const buildTerminalTheme = (highlightFind: boolean) => ({
    background: readColor('--terminal-bg', '#1e1e1e'),
    foreground: readColor('--terminal-text', '#e0e0e0'),
    cursor: readColor('--terminal-text', '#e0e0e0'),
    green: readColor('--success', '#39d98a'),
    brightGreen: readColor('--success', '#52f2a0'),
    blue: readColor('--accent-text', '#c8d0da'),
    brightBlue: readColor('--text-main', '#f1f5f9'),
    selectionBackground: readColor(
      highlightFind ? '--terminal-search-highlight' : '--terminal-cmd-bg',
      highlightFind ? 'rgba(236, 255, 71, 0.82)' : 'rgba(148, 163, 184, 0.24)'
    )
  })

  const applyTerminalTheme = () => {
    const terminal = terminalRef.current
    if (!terminal) {
      return
    }
    terminal.options.theme = buildTerminalTheme(findOpen && Boolean(findQuery))
    terminal.refresh(0, Math.max(terminal.rows - 1, 0))
  }

  const clearFindSelection = () => {
    const terminal = terminalRef.current
    if (terminal?.hasSelection()) {
      terminal.clearSelection()
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

  const collectFindMatches = (query: string) => {
    const terminal = terminalRef.current
    if (!terminal || !query) {
      return []
    }

    const normalizedQuery = query.toLocaleLowerCase()
    const buffer = terminal.buffer.active
    const matches: Array<{ row: number; column: number }> = []

    for (let row = 0; row < buffer.length; row += 1) {
      const line = buffer.getLine(row)?.translateToString(true) ?? ''
      const haystack = line.toLocaleLowerCase()
      let searchFrom = 0

      while (searchFrom <= haystack.length - normalizedQuery.length) {
        const column = haystack.indexOf(normalizedQuery, searchFrom)
        if (column === -1) {
          break
        }
        matches.push({ row, column })
        searchFrom = column + Math.max(1, normalizedQuery.length)
      }
    }

    return matches
  }

  const selectFindMatch = (query: string, index: number, matches = collectFindMatches(query)) => {
    const terminal = terminalRef.current
    if (!terminal || !query) {
      setFindMiss(false)
      setFindMatchCount(0)
      setActiveFindIndex(-1)
      clearFindSelection()
      return false
    }

    setFindMatchCount(matches.length)

    if (matches.length === 0) {
      setFindMiss(true)
      setActiveFindIndex(-1)
      clearFindSelection()
      terminal.focus()
      return false
    }

    const normalizedIndex = ((index % matches.length) + matches.length) % matches.length
    const match = matches[normalizedIndex]

    terminal.scrollToLine(match.row)
    terminal.select(match.column, match.row, query.length)
    setFindMiss(false)
    setActiveFindIndex(normalizedIndex)
    terminal.focus()
    return true
  }

  const runCopy = () => {
    const terminal = terminalRef.current
    if (!terminal) {
      return
    }
    const selection = terminal.getSelection()
    if (!selection) {
      return
    }
    copyText(selection)
    terminal.focus()
  }

  const runPaste = async () => {
    const terminal = terminalRef.current
    if (!terminal) {
      return
    }
    const value = await navigator.clipboard?.readText?.()
    if (value) {
      terminal.paste(value)
    }
    terminal.focus()
  }

  const searchTerminal = (query: string, direction: 1 | -1 = 1) => {
    if (!query) {
      setFindMiss(false)
      setFindMatchCount(0)
      setActiveFindIndex(-1)
      clearFindSelection()
      return false
    }

    const matches = collectFindMatches(query)
    if (matches.length === 0) {
      setFindMiss(true)
      setFindMatchCount(0)
      setActiveFindIndex(-1)
      clearFindSelection()
      terminalRef.current?.focus()
      return false
    }

    const nextIndex = activeFindIndex >= 0 ? activeFindIndex + direction : direction === -1 ? matches.length - 1 : 0
    return selectFindMatch(query, nextIndex, matches)
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

  const clearInlineRedrawState = () => {
    activeInlineRedrawRef.current = false
    inlineRedrawContentRef.current = ''
    inlineRedrawRowsRef.current = 1
  }

  const buildExitAlternateScreenSequence = () =>
    '\x1b[?1049l\x1b[?1047l\x1b[?47l\x1b[?25h'

  const appendRenderedTranscript = (chunk: string) => {
    if (!chunk) {
      return
    }

    renderedTranscriptRef.current = trimTranscript(`${renderedTranscriptRef.current}${chunk}`)
  }

  const formatTerminalChunk = (value: string) => toDisplayTerminalText(value)

  const replaceTerminalWithTranscript = (terminal: Terminal, transcript: string) => {
    renderedTranscriptRef.current = trimTranscript(transcript)
    pendingWriteRef.current = ''
    if (writeFrameRef.current !== null) {
      window.cancelAnimationFrame(writeFrameRef.current)
      writeFrameRef.current = null
    }
    isWritingRef.current = false
    clearInlineRedrawState()
    terminal.reset()
    terminal.write(formatTerminalChunk(renderedTranscriptRef.current))
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

  const buildInlineRedrawPrefix = (rows: number) => {
    if (rows <= 1) {
      return '\r\x1b[2K'
    }

    let prefix = '\r'
    for (let index = 1; index < rows; index += 1) {
      prefix += '\x1b[F'
    }
    for (let index = 0; index < rows; index += 1) {
      prefix += '\x1b[2K'
      if (index < rows - 1) {
        prefix += '\x1b[E'
      }
    }
    for (let index = 1; index < rows; index += 1) {
      prefix += '\x1b[F'
    }
    return prefix
  }

  const normalizeInlineRedrawChunk = (terminal: Terminal, value: string) => {
    if (!value.includes('\r')) {
      if (activeInlineRedrawRef.current) {
        inlineRedrawContentRef.current += value
        inlineRedrawRowsRef.current = rowsForDisplayWidth(
          getTerminalCellWidth(inlineRedrawContentRef.current),
          terminal.cols
        )
      }
      if (value.includes('\n')) {
        clearInlineRedrawState()
      }
      return value.replaceAll('\n', '\r\n')
    }

    let normalized = ''
    let cursor = 0

    while (cursor < value.length) {
      const carriageIndex = value.indexOf('\r', cursor)
      const newlineIndex = value.indexOf('\n', cursor)

      if (carriageIndex === -1 && newlineIndex === -1) {
        const tail = value.slice(cursor)
        normalized += tail
        if (activeInlineRedrawRef.current) {
          inlineRedrawContentRef.current += tail
          inlineRedrawRowsRef.current = rowsForDisplayWidth(
            getTerminalCellWidth(inlineRedrawContentRef.current),
            terminal.cols
          )
        }
        break
      }

      const nextBreakIndex = [carriageIndex, newlineIndex]
        .filter((index) => index !== -1)
        .sort((left, right) => left - right)[0]

      const segment = value.slice(cursor, nextBreakIndex)
      normalized += segment
      if (activeInlineRedrawRef.current) {
        inlineRedrawContentRef.current += segment
        inlineRedrawRowsRef.current = rowsForDisplayWidth(
          getTerminalCellWidth(inlineRedrawContentRef.current),
          terminal.cols
        )
      }

      if (nextBreakIndex === carriageIndex && value[nextBreakIndex + 1] === '\n') {
        normalized += '\r\n'
        clearInlineRedrawState()
        cursor = nextBreakIndex + 2
        continue
      }

      if (nextBreakIndex === carriageIndex) {
        normalized += buildInlineRedrawPrefix(inlineRedrawRowsRef.current)
        activeInlineRedrawRef.current = true
        inlineRedrawContentRef.current = ''
        inlineRedrawRowsRef.current = 1
        cursor = nextBreakIndex + 1
        continue
      }

      normalized += '\r\n'
      clearInlineRedrawState()
      cursor = nextBreakIndex + 1
    }

    return normalized
  }

  const syncTerminalSize = (fitAddon: FitAddon, terminal: Terminal) => {
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

    // xterm's fit calculation can be optimistic by a sliver in Electron split panes.
    // Keep one row of breathing room so full-screen TUIs do not draw their status
    // bars under the file panel/resizer.
    const rows = Math.max(1, proposed.rows - TERMINAL_FIT_GUARD_ROWS)
    if (terminal.cols !== proposed.cols || terminal.rows !== rows) {
      terminal.resize(proposed.cols, rows)
    }

    const nextSize = {
      cols: terminal.cols,
      rows: terminal.rows,
      width: Math.floor(width),
      height: Math.floor(height)
    }
    const previousSize = lastSyncedSizeRef.current
    if (
      previousSize
      && previousSize.cols === nextSize.cols
      && previousSize.rows === nextSize.rows
      && previousSize.width === nextSize.width
      && previousSize.height === nextSize.height
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
      allowTransparency: true,
      scrollback: 6000,
      theme: buildTerminalTheme(false)
    })
    const fitAddon = new FitAddon()
    terminal.loadAddon(fitAddon)
    terminal.open(hostRef.current)
    terminalRef.current = terminal

    syncTerminalSize(fitAddon, terminal)
    clearInlineRedrawState()

    if (bootTextRef.current) {
      replaceTerminalWithTranscript(terminal, bootTextRef.current)
    }

    const resize = () => {
      syncTerminalSize(fitAddon, terminal)
    }

    const onDataDispose = terminal.onData((data) => {
      if (terminal.hasSelection()) {
        terminal.clearSelection()
      }
      setContextMenu(null)
      void window.termdock?.writeTerminal(tabId, data)
    })

    const onSelectionDispose = terminal.onSelectionChange(() => {
      setHasSelection(terminal.hasSelection())
    })

    const offData = window.termdock?.onTerminalData(({ tabId: nextTabId, chunk }) => {
      if (nextTabId === tabId) {
        if (Date.now() < suppressHydratedChunksUntilRef.current && renderedTranscriptRef.current.endsWith(chunk)) {
          return
        }
        appendRenderedTranscript(chunk)
        scheduleTerminalWrite(formatTerminalChunk(chunk))
      }
    })

    const offState = window.termdock?.onTerminalState(({ tabId: nextTabId, summary, transcript, connected }) => {
      if (nextTabId === tabId) {
        onStatus?.(localizeTerminalText(summary))
        if (shouldHydrateTranscript(renderedTranscriptRef.current, transcript, connected)) {
          replaceTerminalWithTranscript(terminal, transcript)
        }
        if (wasConnectedRef.current && !connected) {
          preserveVisibleBufferRef.current = true
          scheduleTerminalWrite(`${buildExitAlternateScreenSequence()}\r\n${t.terminalConnectionClosed}\r\n`)
        }
        wasConnectedRef.current = connected
      }
    })

    const resizeObserver = new ResizeObserver(() => resize())
    resizeObserver.observe(hostRef.current)

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
      writeFrameRef.current = null
      isWritingRef.current = false
      pendingWriteRef.current = ''
      renderedTranscriptRef.current = ''
      suppressHydratedChunksUntilRef.current = 0
      preserveVisibleBufferRef.current = false
      lastSyncedSizeRef.current = null
      clearInlineRedrawState()
      resizeObserver.disconnect()
      hostRef.current?.removeEventListener('contextmenu', onContextMenu)
      window.removeEventListener('keydown', onKeyDown)
      terminalRef.current = null
      terminal.dispose()
    }
  }, [isMac, onStatus, tabId])

  useEffect(() => {
    bootTextRef.current = bootText
    const terminal = terminalRef.current
    if (!terminal || !shouldHydrateTranscript(renderedTranscriptRef.current, bootText, wasConnectedRef.current)) {
      return
    }

    replaceTerminalWithTranscript(terminal, bootText)
  }, [bootText])

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

    const matches = collectFindMatches(findQuery)
    setFindMatchCount(matches.length)

    if (matches.length === 0) {
      setFindMiss(true)
      setActiveFindIndex(-1)
      clearFindSelection()
      return
    }

    setFindMiss(false)
    if (activeFindIndex === -1) {
      void selectFindMatch(findQuery, 0, matches)
      return
    }
    if (activeFindIndex >= matches.length) {
      setActiveFindIndex(matches.length - 1)
    }
  }, [activeFindIndex, findOpen, findQuery])

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
