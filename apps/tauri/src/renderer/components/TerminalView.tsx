import { memo, useEffect, useRef, useState } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { SearchAddon } from '@xterm/addon-search'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import { WebLinksAddon } from '@xterm/addon-web-links'
import '@xterm/xterm/css/xterm.css'
import { copyText } from '../app/app-utils'
import {
  isClinkAutosuggestHelpUrl,
  stripClinkAutosuggestPrompt,
  trimHydratedTerminalChunk
} from '../app/terminal-transcript'
import { APP_EVENT, onAppEvent } from '../lib/app-events'
import { t } from '../i18n'
import { ContextMenu } from '../features/common/ContextMenu'
import { CloseButton } from '../features/common/CloseButton'
import { AppIcon } from '../features/common/AppIcon'
import { FILETERM_MONO_FONT_FAMILY, observeCanvasTextMetrics } from '../app/font-metrics'

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
  // Localize fixed FileTerm notices before preserving terminal control semantics later.
  return localizeTerminalText(stripClinkAutosuggestPrompt(value))
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
  return target === '' || /[cpsq01234567]/.test(target)
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
const TERMINAL_FIT_GUARD_ROWS = 0
const TERMINAL_RESIZE_PIXEL_EPSILON = 2
const TERMINAL_RESIZE_SETTLE_MS = 140
const TERMINAL_RESIZE_OUTPUT_QUIET_MS = 260
// Bound one xterm parse pass without serializing the native input path.
const TERMINAL_WRITE_FRAME_BUDGET = 16 * 1024

type SplitPaneDirection = 'row' | 'column'

function splitPaneShortcutsForPlatform(platform: string | undefined) {
  if (platform === 'darwin') {
    return { vertical: '⌘D', horizontal: '⇧⌘D', closePane: '⌘W' }
  }
  if (platform === 'win32') {
    // 与 Windows Terminal / pwsh 默认一致：Alt+Shift++ 垂直、Alt+Shift+- 水平、
    // Ctrl+Shift+W 关闭当前 pane。
    return { vertical: 'Alt+Shift++', horizontal: 'Alt+Shift+-', closePane: 'Ctrl+Shift+W' }
  }
  return { vertical: 'Ctrl+Shift+D', horizontal: 'Ctrl+Alt+Shift+D', closePane: 'Ctrl+Shift+W' }
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

  return [/(?:^|\s)[A-Za-z0-9_.-]+@[A-Za-z0-9_.-]+(?::[^\n]*)?[#$%>]$/, /^\[[^\]]+@[^\]]+\][#$]$/, /^[#$%>]$/].some(
    (pattern) => pattern.test(line)
  )
}

function isFocusTrackingSequence(data: string) {
  const escape = String.fromCharCode(27)
  return data === `${escape}[I` || data === `${escape}[O`
}

type VimVisualSelection = {
  text: string
  mode: 'character' | 'line' | 'block'
  startRow: number
  endRow: number
}

type HighlightedBufferRow = {
  row: number
  firstColumn: number
  lastColumn: number
}

/**
 * Vim enables DEC mouse tracking while it is running. That deliberately makes
 * xterm stop maintaining a local selection, so `terminal.getSelection()` is
 * empty even though Vim is visibly in Visual mode. Vim renders that range with
 * either SGR inverse or a non-default background; recover the corresponding
 * buffer text as a copy-only fallback.
 *
 * This is intentionally gated by Vim's own `-- VISUAL --` mode indicator and
 * the active mouse protocol. Other TUIs with coloured panels must not become
 * accidentally copyable selections.
 */
function getVimVisualSelection(terminal: Terminal): VimVisualSelection | null {
  if (terminal.modes.mouseTrackingMode === 'none') {
    return null
  }

  const buffer = terminal.buffer.active
  const viewportStart = buffer.viewportY
  const viewportEnd = Math.min(buffer.viewportY + terminal.rows, buffer.length)
  let mode: VimVisualSelection['mode'] | null = null

  for (let row = viewportStart; row < viewportEnd; row += 1) {
    const lineText = buffer.getLine(row)?.translateToString(true) ?? ''
    if (/--\s+VISUAL\s+BLOCK\s+--/.test(lineText)) {
      mode = 'block'
      break
    }
    if (/--\s+VISUAL\s+LINE\s+--/.test(lineText)) {
      mode = 'line'
      break
    }
    if (/--\s+VISUAL\s+--/.test(lineText)) {
      mode = 'character'
    }
  }

  if (!mode) {
    return null
  }

  const highlightedRows: HighlightedBufferRow[] = []
  for (let row = viewportStart; row < viewportEnd; row += 1) {
    const line = buffer.getLine(row)
    if (!line) {
      continue
    }

    let firstColumn = -1
    let lastColumn = -1
    for (let column = 0; column < terminal.cols; column += 1) {
      const cell = line.getCell(column)
      // Wide characters have a zero-width continuation cell. It has no text
      // of its own and must not move a selection boundary one column forward.
      if (!cell || cell.getWidth() === 0) {
        continue
      }
      if (!cell.isInverse() && cell.isBgDefault()) {
        continue
      }
      if (firstColumn === -1) {
        firstColumn = column
      }
      lastColumn = column + cell.getWidth()
    }

    if (firstColumn !== -1) {
      highlightedRows.push({ row, firstColumn, lastColumn })
    }
  }

  const cursorRow = buffer.baseY + buffer.cursorY
  const cursorColumn = buffer.cursorX
  const cursorRowIndex = highlightedRows.findIndex(
    ({ row, firstColumn, lastColumn }) => row === cursorRow && cursorColumn >= firstColumn && cursorColumn <= lastColumn
  )
  if (cursorRowIndex === -1) {
    return null
  }

  // A Visual range is contiguous by row and always contains Vim's cursor.
  // Restricting the candidate to that contiguous run avoids status bars and
  // syntax groups elsewhere in the terminal buffer.
  let startIndex = cursorRowIndex
  while (startIndex > 0 && highlightedRows[startIndex - 1].row === highlightedRows[startIndex].row - 1) {
    startIndex -= 1
  }
  let endIndex = cursorRowIndex
  while (
    endIndex < highlightedRows.length - 1 &&
    highlightedRows[endIndex + 1].row === highlightedRows[endIndex].row + 1
  ) {
    endIndex += 1
  }

  const selectionRows = highlightedRows.slice(startIndex, endIndex + 1)
  const startRow = selectionRows[0]
  const endRow = selectionRows.at(-1)
  if (!endRow) {
    return null
  }

  const lines = selectionRows.map(({ row, firstColumn, lastColumn }, index) => {
    const line = buffer.getLine(row)
    if (!line) {
      return ''
    }

    if (mode === 'block') {
      return line.translateToString(true, firstColumn, lastColumn)
    }

    const selectionStart = index === 0 ? firstColumn : 0
    const selectionEnd = index === selectionRows.length - 1 ? lastColumn : terminal.cols
    return line.translateToString(true, selectionStart, selectionEnd)
  })
  const text = lines.join('\n')
  if (!text) {
    return null
  }

  return { text, mode, startRow: startRow.row, endRow: endRow.row }
}

function logTerminalClipboard(terminal: Terminal, action: string, details: Record<string, unknown> = {}) {
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

export const TerminalView = memo(function TerminalView({
  tabId,
  bootText,
  connected = false,
  connecting = false,
  isActive = true,
  onStatus,
  onReconnect,
  onActivate,
  onSplitPane,
  onClosePane,
  onCloseTab,
  canClosePane = false
}: {
  tabId: string
  bootText: string
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
  // 后端 worker 异常退出时 writeTerminal 会 reject；记录一次避免每个按键
  // 都刷一行提示，直到 terminal:state 重新同步后复位。
  const inputSendFailedRef = useRef(false)
  // `onData` is registered once for the xterm instance.  Reading the prop
  // through a ref prevents a stale terminal-state event from a background
  // tab from swallowing keystrokes after this tab is brought back.
  const connectedRef = useRef(Boolean(connected))
  const connectingRef = useRef(Boolean(connecting))
  const lastSyncedSizeRef = useRef<{ cols: number; rows: number; width: number; height: number } | null>(null)
  const lastObservedHostRectRef = useRef<{ width: number; height: number } | null>(null)
  const isHorizontalResizeActiveRef = useRef(false)
  const lastTerminalOutputAtRef = useRef(0)
  const awaitingCommandCompletionRef = useRef(false)
  const pendingPromptResizeRef = useRef(false)
  const tabIdRef = useRef(tabId)
  const onStatusRef = useRef(onStatus)
  const onReconnectRef = useRef(onReconnect)
  const onSplitPaneRef = useRef(onSplitPane)
  const onClosePaneRef = useRef(onClosePane)
  const onCloseTabRef = useRef(onCloseTab)
  const canClosePaneRef = useRef(canClosePane)
  const isReconnectingRef = useRef(false)
  const reconnectHintShownRef = useRef(false)
  const activeTerminalTabIdRef = useRef<string | null>(null)
  tabIdRef.current = tabId
  connectedRef.current = Boolean(connected)
  connectingRef.current = Boolean(connecting)
  onStatusRef.current = onStatus
  onReconnectRef.current = onReconnect
  onSplitPaneRef.current = onSplitPane
  onClosePaneRef.current = onClosePane
  onCloseTabRef.current = onCloseTab
  canClosePaneRef.current = canClosePane
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
  const isMac = window.fileterm?.platform === 'darwin'
  const isWin = window.fileterm?.platform === 'win32'

  const shortcuts = {
    copy: isMac ? '⌘C' : 'Ctrl+Shift+C',
    paste: isMac ? '⌘V' : 'Ctrl+Shift+V',
    find: isMac ? '⌘F' : 'Ctrl+F',
    ...splitPaneShortcutsForPlatform(window.fileterm?.platform)
  }

  const readColor = (name: string, fallback: string) =>
    getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback

  const buildTerminalTheme = () => ({
    background: readColor('--terminal-bg', '#1e1e1e'),
    foreground: readColor('--terminal-text', '#e0e0e0'),
    cursor: readColor('--terminal-cursor', readColor('--accent-primary', '#3b82f6')),
    cursorAccent: readColor('--terminal-cursor-accent', readColor('--terminal-bg', '#ffffff')),
    green: readColor('--success', '#39d98a'),
    brightGreen: readColor('--success', '#52f2a0'),
    blue: readColor('--accent-text', '#c8d0da'),
    brightBlue: readColor('--text-main', '#f1f5f9'),
    selectionBackground:
      findOpen && findQuery
        ? readColor('--terminal-search-active-bg', '#ffd43b')
        : readColor('--terminal-cmd-bg', 'rgba(148, 163, 184, 0.24)'),
    selectionForeground:
      findOpen && findQuery
        ? readColor('--terminal-search-active-text', '#111111')
        : readColor('--terminal-text', '#e0e0e0')
  })

  const buildSearchDecorations = () => ({
    matchBackground: readColor('--terminal-search-match-bg', '#4b5563'),
    matchOverviewRuler: readColor('--terminal-search-match-ruler', '#9ca3af'),
    activeMatchBackground: readColor('--terminal-search-active-bg', '#ffd43b'),
    activeMatchBorder: readColor('--terminal-search-active-border', '#8a5a00'),
    activeMatchColorOverviewRuler: readColor('--terminal-search-active-ruler', '#f0b400')
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
    const xtermSelection = terminal.getSelection()
    const vimVisualSelection = xtermSelection ? null : getVimVisualSelection(terminal)
    const selection = xtermSelection || vimVisualSelection?.text || ''
    if (!selection) {
      logTerminalClipboard(terminal, 'copy-skipped-empty-selection')
      return
    }
    logTerminalClipboard(terminal, 'copy-requested', {
      source: vimVisualSelection ? 'vim-visual' : 'xterm',
      vimVisualMode: vimVisualSelection?.mode
    })
    if (window.fileterm?.writeClipboardText) {
      void window.fileterm.writeClipboardText(selection).then(
        () => logTerminalClipboard(terminal, 'copy-succeeded'),
        (error: unknown) => {
          if (import.meta.env.DEV) {
            console.warn('[TerminalView][clipboard] copy-failed', error)
          }
        }
      )
    } else {
      copyText(selection)
      logTerminalClipboard(terminal, 'copy-requested-browser-fallback')
    }
    terminal.focus()
  }

  const runPaste = async () => {
    const terminal = terminalRef.current
    if (!terminal) {
      return
    }
    try {
      const value = window.fileterm?.readClipboardText
        ? await window.fileterm.readClipboardText()
        : await navigator.clipboard?.readText?.()
      if (value) {
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

    // Keep xterm and the remote PTY on the exact same column count. Readline,
    // vim, nano and progress bars all depend on that agreement for wrapping
    // and cursor-addressing. During an active horizontal resize we temporarily
    // freeze cols, then sync the true width once the drag settles.
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
  }

  useEffect(() => {
    if (!hostRef.current) {
      return
    }

    const terminal = new Terminal({
      fontFamily: FILETERM_MONO_FONT_FAMILY,
      fontSize: 12,
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
    const writeReconnectHint = () => {
      if (!onReconnectRef.current || reconnectHintShownRef.current) {
        return
      }

      reconnectHintShownRef.current = true
      terminal.write(`\r\n${t.pressEnterToReconnect}\r\n`)
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
    terminal.attachCustomKeyEventHandler((event) => {
      if (event.type !== 'keydown') {
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
        terminal.write(`\r\n${t.terminalConnectionClosed}\r\n`)
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
              const reconnectHint = onReconnectRef.current ? `\r\n${t.pressEnterToReconnect}` : ''
              if (reconnectHint) {
                reconnectHintShownRef.current = true
              }
              const disconnectedTranscript = visibleTranscript
                ? `${visibleTranscript}\r\n${t.terminalConnectionClosed}${reconnectHint}\r\n`
                : `${t.terminalConnectionClosed}${reconnectHint}\r\n`
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

      const { width, height } = host.getBoundingClientRect()
      const lastObservedRect = lastObservedHostRectRef.current
      lastObservedHostRectRef.current = { width, height }

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
      const vimVisualSelection = getVimVisualSelection(terminal)
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
      return Boolean(host && target instanceof Node && host.contains(target))
    }

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
      const key = event.key.toLowerCase()
      const matchesCopy = isMac
        ? event.metaKey && !event.shiftKey && key === 'c'
        : event.ctrlKey && event.shiftKey && key === 'c'

      if (matchesCopy && (terminal.hasSelection() || Boolean(getVimVisualSelection(terminal)))) {
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

    hostRef.current.addEventListener('mousedown', onMouseDown, true)
    hostRef.current.addEventListener('pointerdown', onPointerDown, true)
    document.addEventListener('contextmenu', onDocumentContextMenu, true)
    window.addEventListener('keydown', onKeyDown, true)
    window.addEventListener('focus', onWindowFocus)
    document.addEventListener('selectionchange', onDocumentSelectionChange)
    document.addEventListener('visibilitychange', onVisibilityChange)
    const offFocusTerminal = onAppEvent(APP_EVENT.focusTerminal, handleFocusTerminal)
    const offTerminalCopy = onAppEvent(APP_EVENT.terminalCopy, handleTerminalCopy)
    const offTerminalPaste = onAppEvent(APP_EVENT.terminalPaste, handleTerminalPaste)
    const offTerminalFind = onAppEvent(APP_EVENT.terminalFind, handleTerminalFind)

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
      disposeCanvasTextMetrics()
      resizeObserver.disconnect()
      hostRef.current?.removeEventListener('mousedown', onMouseDown, true)
      hostRef.current?.removeEventListener('pointerdown', onPointerDown, true)
      document.removeEventListener('contextmenu', onDocumentContextMenu, true)
      window.removeEventListener('keydown', onKeyDown, true)
      window.removeEventListener('focus', onWindowFocus)
      document.removeEventListener('selectionchange', onDocumentSelectionChange)
      document.removeEventListener('visibilitychange', onVisibilityChange)
      searchAddonRef.current = null
      terminalRef.current = null
      terminal.dispose()
    }
  }, [isMac])

  useEffect(() => {
    if (isActive) {
      window.requestAnimationFrame(() => {
        terminalRef.current?.focus()
      })
    }
  }, [isActive])

  useEffect(() => {
    bootTextRef.current = bootText

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
    replaceTerminalWithTranscript(terminal, bootText)
    lastSyncedSizeRef.current = null

    const { width, height } = host.getBoundingClientRect()
    if (width > 0 && height > 0) {
      void window.fileterm?.resizeTerminal(tabId, terminal.cols, terminal.rows, Math.floor(width), Math.floor(height))
    }
  }, [bootText, connected, tabId])

  useEffect(() => {
    bootTextRef.current = bootText
    const terminal = terminalRef.current
    if (
      !terminal ||
      connected ||
      !shouldHydrateTranscript(renderedTranscriptRef.current, bootText, wasConnectedRef.current)
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
    <div className="terminal-view" onFocusCapture={onActivate} onMouseDown={onActivate}>
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
          <div className="terminal-find-actions" role="group" aria-label={t.find}>
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
            <button type="button" title={t.findPrevious} onClick={() => searchTerminal(findQuery, -1)}>
              <AppIcon name="arrow-up" size={13} />
            </button>
            <button type="button" title={t.findNext} onClick={() => searchTerminal(findQuery, 1)}>
              <AppIcon name="arrow-down" size={13} />
            </button>
            <button className="terminal-find-submit" type="button" onClick={() => searchTerminal(findQuery, 1)}>
              {t.find}
            </button>
          </div>
          <CloseButton onClick={closeFind} size="compact" />
          {findMiss ? <span className="terminal-find-status">{t.findNotFound}</span> : null}
        </div>
      ) : null}
      {contextMenu ? (
        <ContextMenu
          className="terminal-context-menu"
          items={[
            { label: t.copy, shortcut: shortcuts.copy, disabled: !hasSelection, action: runCopy },
            { label: t.paste, shortcut: shortcuts.paste, action: () => void runPaste() },
            ...(onSplitPane
              ? [
                  { separator: true },
                  {
                    label: t.splitVertically,
                    shortcut: shortcuts.vertical,
                    action: () => runSplitPane('row')
                  },
                  {
                    label: t.splitHorizontally,
                    shortcut: shortcuts.horizontal,
                    action: () => runSplitPane('column')
                  },
                  // 只在分屏中且还有兄弟 pane 时显示：单 pane 时关闭等价于关 tab，
                  // 走平台关闭键（Cmd+W / Ctrl+Shift+W）的确认流程更合适。
                  ...(onClosePane && canClosePane
                    ? [
                        {
                          label: t.closePane,
                          shortcut: shortcuts.closePane,
                          action: runClosePane
                        }
                      ]
                    : [])
                ]
              : []),
            { separator: true },
            { label: t.find, shortcut: shortcuts.find, action: runFind },
            { label: t.clearScreen, action: runClear }
          ]}
          onClose={() => setContextMenu(null)}
          position={contextMenu}
        />
      ) : null}
    </div>
  )
})
