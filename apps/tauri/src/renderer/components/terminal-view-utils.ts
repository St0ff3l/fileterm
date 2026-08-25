import type { Terminal } from '@xterm/xterm'
import { stripClinkAutosuggestPrompt } from '../app/terminal-transcript'
import { localizeLocalTerminalText, localizeSerialTerminalText, t } from '../i18n'

export function localizeTerminalText(value: string) {
  return localizeSerialTerminalText(localizeLocalTerminalText(value))
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

export function toDisplayTerminalText(value: string) {
  // Localize fixed FileTerm notices before preserving terminal control semantics later.
  return localizeTerminalText(stripClinkAutosuggestPrompt(value))
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

export const TERMINAL_TRANSCRIPT_LIMIT = 200_000
export const TERMINAL_REMOTE_GUARD_COLS = 0
export const TERMINAL_FIT_GUARD_ROWS = 0
export const TERMINAL_RESIZE_PIXEL_EPSILON = 2
export const TERMINAL_RESIZE_SETTLE_MS = 140
export const TERMINAL_RESIZE_OUTPUT_QUIET_MS = 260
// Bound one xterm parse pass without serializing the native input path.
export const TERMINAL_WRITE_FRAME_BUDGET = 16 * 1024
// Some IMEs can emit the committed composition once through xterm's normal
// input path and then emit the same text again when the user switches back to
// an ASCII keyboard layout. Keep this window narrow so this is not a general
// purpose input de-duplicator for fast, intentional typing.
export const TERMINAL_IME_DUPLICATE_WINDOW_MS = 75
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
  if (platform === 'win32') {
    // 与 Windows Terminal / pwsh 默认一致：Alt+Shift++ 垂直、Alt+Shift+- 水平、
    // Ctrl+Shift+W 关闭当前 pane。
    return { vertical: 'Alt+Shift++', horizontal: 'Alt+Shift+-', closePane: 'Ctrl+Shift+W' }
  }
  return { vertical: 'Ctrl+Shift+D', horizontal: 'Ctrl+Alt+Shift+D', closePane: 'Ctrl+Shift+W' }
}

export function trimTranscript(transcript: string) {
  if (transcript.length <= TERMINAL_TRANSCRIPT_LIMIT) {
    return transcript
  }

  return transcript.slice(transcript.length - TERMINAL_TRANSCRIPT_LIMIT)
}

export function getLastVisibleTerminalLine(terminal: Terminal) {
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

export function looksLikeShellPrompt(line: string) {
  if (!line) {
    return false
  }

  return [/(?:^|\s)[A-Za-z0-9_.-]+@[A-Za-z0-9_.-]+(?::[^\n]*)?[#$%>]$/, /^\[[^\]]+@[^\]]+\][#$]$/, /^[#$%>]$/].some(
    (pattern) => pattern.test(line)
  )
}

export function isFocusTrackingSequence(data: string) {
  const escape = String.fromCharCode(27)
  return data === `${escape}[I` || data === `${escape}[O`
}

export function readHydrationControlSequence(data: string, start: number, escape: string) {
  const csiPrefix = `${escape}[`
  if (data.startsWith(csiPrefix, start)) {
    for (let index = start + csiPrefix.length; index < data.length; index += 1) {
      const code = data.charCodeAt(index)
      if (code >= 0x40 && code <= 0x7e) {
        return data.slice(start, index + 1)
      }
    }
    return null
  }

  const oscPrefix = `${escape}]`
  const dcsPrefix = `${escape}P`
  const isOsc = data.startsWith(oscPrefix, start)
  const isDcs = data.startsWith(dcsPrefix, start)
  if (!isOsc && !isDcs) {
    return null
  }

  const stringStart = start + 2
  const bell = String.fromCharCode(7)
  const stringTerminator = `${escape}\\`
  const bellIndex = isOsc ? data.indexOf(bell, stringStart) : -1
  const stringTerminatorIndex = data.indexOf(stringTerminator, stringStart)
  let end = -1
  if (bellIndex >= 0 && stringTerminatorIndex >= 0) {
    end = Math.min(bellIndex + bell.length, stringTerminatorIndex + stringTerminator.length)
  } else if (bellIndex >= 0) {
    end = bellIndex + bell.length
  } else if (stringTerminatorIndex >= 0) {
    end = stringTerminatorIndex + stringTerminator.length
  }
  return end >= 0 ? data.slice(start, end) : null
}

export function isHydrationResponseSequence(sequence: string, escape: string) {
  // A transcript is a recording of PTY output, not a live terminal state.
  // When it is replayed after a tab switch, xterm can answer historical
  // terminal queries (CPR/DA/mode/color/window reports) and emit those
  // answers through onData. Forwarding the synthetic answers to the now-idle
  // shell makes it echo fragments such as `;201R` or `2RR0;276;0c`.
  const csiPrefix = `${escape}[`
  if (sequence.startsWith(csiPrefix)) {
    const csi = sequence.slice(csiPrefix.length)
    if (csi === '0n') return true
    if (/^\??\d+(?:;\d+)*R$/.test(csi)) return true
    if (/^(?:\?|>)[\d;]*c$/.test(csi)) return true
    if (/^(?:4|6|8);\d+(?:;\d+)?t$/.test(csi)) return true
    if (/^(?:\?)?\d+(?:;\d+)*\$y$/.test(csi)) return true
  }

  const oscPrefix = `${escape}]`
  if (sequence.startsWith(oscPrefix)) {
    const osc = sequence.slice(oscPrefix.length)
    const stringTerminator = `${escape}\\`
    const body = osc.endsWith(String.fromCharCode(7))
      ? osc.slice(0, -1)
      : osc.endsWith(stringTerminator)
        ? osc.slice(0, -stringTerminator.length)
        : null
    if (body !== null && /^(?:4;\d+|10|11|12);rgb:[0-9a-f]+(?:\/[0-9a-f]+){2}$/i.test(body)) {
      return true
    }
  }

  const dcsPrefix = `${escape}P`
  const dcsTerminator = `${escape}\\`
  if (sequence.startsWith(dcsPrefix) && sequence.endsWith(dcsTerminator)) {
    const dcs = sequence.slice(dcsPrefix.length, -dcsTerminator.length)
    return /^[01]\$r[\s\S]*$/.test(dcs)
  }

  return false
}

export function stripHydratedTerminalResponses(data: string) {
  const escape = String.fromCharCode(27)
  let result = ''
  let removedResponse = false
  let index = 0

  while (index < data.length) {
    if (data[index] !== escape) {
      result += data[index]
      index += 1
      continue
    }

    const sequence = readHydrationControlSequence(data, index, escape)
    if (sequence && isHydrationResponseSequence(sequence, escape)) {
      removedResponse = true
      index += sequence.length
      continue
    }

    result += data[index]
    index += 1
  }

  return removedResponse ? result : data
}

export type VimVisualSelection = {
  text: string
  mode: 'character' | 'line' | 'block'
  startRow: number
  endRow: number
}

export type HighlightedBufferRow = {
  row: number
  firstColumn: number
  lastColumn: number
}

export type PinchGestureEvent = Event & { scale?: number }

// Vim translates the mode label through gettext. These are the current UTF-8
// labels from Vim's official `src/po/*.po` catalogs, plus the legacy Central
// European forms shipped by Vim. Do not treat an arbitrary `-- <mode> --`
// message as Visual: `-- INSERT --` uses the same framing.
export const VIM_VISUAL_MODE_LABELS: Readonly<Record<VimVisualSelection['mode'], readonly string[]>> = {
  character: [
    'VISUAL',
    'VISUELE',
    'VIZUÁLNÍ',
    'VISUEL',
    'VISUELL',
    'VIDUMA',
    'VALINTA',
    'RADHARCACH',
    'VIZUÁLIS',
    'ԶՆՆԱԿԱՆ ՌԵԺԻՄ',
    'VISUALE',
    'ビジュアル',
    '비주얼',
    'VISUEEL',
    'WIZUALNY',
    'РЕЖИМ ВИЗУАЛЬНЫЙ ПОСИМВОЛЬНЫЙ',
    'VIZUÁLNE',
    'ВИЗУЕЛНО',
    'காட்சி',
    'GÖRSEL',
    'ВИБІР',
    'CHẾ ĐỘ VISUAL',
    '可视',
    '選取'
  ],
  line: [
    'VISUAL LINE',
    'VISUELE REËL',
    'VIZUÁLNÍ ŘÁDEK',
    'VISUAL LÍNIA',
    'VISUEL LINJE',
    'VISUELL ZEILE',
    'VIDUMA LINIO',
    'LÍNEA VISUAL',
    'VALINTARIVI',
    'VISUEL LIGNE',
    'LÍNE RADHARCACH',
    'VIZUÁLIS SOR',
    'ԶՆՆԱԿԱՆ ՏՈՂ',
    'VISUALE RIGA',
    'ビジュアル 行',
    '비주얼 라인',
    'VISUELL LINJE',
    'VISUELE REGEL',
    'WIZUALNY LINIOWY',
    'VISUAL/LINHA',
    'РЕЖИМ ВИЗУАЛЬНЫЙ ПОСТРОЧНЫЙ',
    'VIZUÁLNE RIADKY',
    'ВИЗУЕЛНА ЛИНИЈА',
    'VISUELL RAD',
    'காட்சி வரி',
    'GÖRSEL SATIR',
    'ВИБІР РЯДКІВ',
    'DÒNG VISUAL',
    '可视 行',
    '[行]'
  ],
  block: [
    'VISUAL BLOCK',
    'VISUELE BLOK',
    'VIZUÁLNÍ BLOK',
    'VISUAL BLOC',
    'VISUEL BLOK',
    'VISUELL BLOCK',
    'VIDUMA BLOKO',
    'BLOQUE VISUAL',
    'VALINTALOHKO',
    'VISUEL BLOC',
    'BLOC RADHARCACH',
    'VIZUÁLIS BLOKK',
    'ԶՆՆԱԿԱՆ ԲԼՈԿ',
    'VISUALE BLOCCO',
    'ビジュアル 矩形',
    '비주얼 블록',
    'VISUELL BLOKK',
    'VISUEEL BLOK',
    'WIZUALNY BLOKOWY',
    'VISUAL/BLOCO',
    'РЕЖИМ ВИЗУАЛЬНЫЙ БЛОЧНЫЙ',
    'VIZUÁLNY BLOK',
    'ВИЗУЕЛНИ БЛОК',
    'VISUELLT BLOCK',
    'விசுவல் பிளாக்',
    'GÖRSEL BLOK',
    'ВИБІР БЛОКУ',
    'KHỐI VISUAL',
    '可视 块',
    '[區塊]'
  ]
}

export function logVimVisualDiagnostic(stage: string, details: Record<string, unknown>) {
  if (!import.meta.env.DEV) {
    return
  }

  // Keep this as a string, rather than a console object, so a screenshot of
  // Web Inspector contains the actual values without having to expand rows.
  console.debug(`[TerminalView][vim-visual] ${stage} ${JSON.stringify(details)}`)
}

export function getVimVisualModeIndicator(lineText: string): VimVisualSelection['mode'] | null {
  // The terminal's own status overlay can be appended to Vim's final buffer
  // line (for example `-- VISUAL --\n 5-15 …`), so the mode marker is not
  // necessarily the entire translated row.
  const indicator = lineText.match(/--\s+(.+?)\s+--/)
  if (!indicator || indicator[1].length > 64) {
    return null
  }

  const label = indicator[1].trim().replace(/\s+/g, ' ')
  for (const mode of ['character', 'line', 'block'] as const) {
    if (VIM_VISUAL_MODE_LABELS[mode].includes(label)) {
      return mode
    }
  }
  return null
}

/**
 * Vim enables DEC mouse tracking while it is running. That deliberately makes
 * xterm stop maintaining a local selection, so `terminal.getSelection()` is
 * empty even though Vim is visibly in Visual mode. Vim renders that range with
 * either SGR inverse or a non-default background; recover the corresponding
 * buffer text as a copy-only fallback.
 *
 * This is intentionally gated by Vim's locale-independent `-- <mode> --`
 * status-line framing, the active mouse protocol, and highlighted cells around
 * the active cursor. Other TUIs with coloured panels must not become
 * accidentally copyable selections.
 */
export function getVimVisualSelection(terminal: Terminal, diagnostics = false): VimVisualSelection | null {
  const diagnose = (stage: string, details: Record<string, unknown>) => {
    if (diagnostics) {
      logVimVisualDiagnostic(stage, details)
    }
  }

  if (terminal.modes.mouseTrackingMode === 'none') {
    diagnose('mouse-tracking-disabled', {})
    return null
  }

  const buffer = terminal.buffer.active
  const viewportStart = buffer.viewportY
  const viewportEnd = Math.min(buffer.viewportY + terminal.rows, buffer.length)
  let mode: VimVisualSelection['mode'] | null = null
  const modeIndicators: Array<{ row: number; text: string; mode: VimVisualSelection['mode'] }> = []

  // Vim renders its mode indicator on the final status line. Search upward so
  // a matching string in the file contents cannot shadow the actual mode.
  for (let row = viewportEnd - 1; row >= viewportStart; row -= 1) {
    const lineText = buffer.getLine(row)?.translateToString(true) ?? ''
    const indicatorMode = getVimVisualModeIndicator(lineText)
    if (indicatorMode) {
      modeIndicators.push({ row, text: lineText, mode: indicatorMode })
      mode = indicatorMode
      break
    }
  }

  if (!mode) {
    diagnose('mode-not-found', {
      bufferType: buffer.type,
      viewportStart,
      viewportEnd,
      bottomLines: Array.from({ length: Math.min(3, viewportEnd - viewportStart) }, (_, index) => {
        const row = viewportEnd - 1 - index
        return { row, text: buffer.getLine(row)?.translateToString(true) ?? '' }
      })
    })
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
  const cursorCell = buffer.getLine(cursorRow)?.getCell(cursorColumn)
  const cursorCellWidth = cursorCell && cursorCell.getWidth() > 0 ? cursorCell.getWidth() : 1
  const cursorCellDetails = cursorCell
    ? {
        chars: cursorCell.getChars(),
        width: cursorCell.getWidth(),
        inverse: Boolean(cursorCell.isInverse()),
        backgroundDefault: cursorCell.isBgDefault(),
        foregroundDefault: cursorCell.isFgDefault()
      }
    : null
  let cursorRowIndex = highlightedRows.findIndex(
    ({ row, firstColumn, lastColumn }) => row === cursorRow && cursorColumn >= firstColumn && cursorColumn <= lastColumn
  )

  if (cursorRowIndex === -1) {
    // Vim's active Visual endpoint is drawn by xterm's cursor layer. That
    // overlay is not part of the buffer cell attributes, so the endpoint can
    // sit immediately before or after the inverse/background run even though
    // it is part of the same Vim selection.
    cursorRowIndex = highlightedRows.findIndex(
      ({ row, firstColumn, lastColumn }) =>
        row === cursorRow && (cursorColumn === firstColumn - 1 || cursorColumn === lastColumn)
    )
    if (cursorRowIndex !== -1) {
      const cursorRowHighlight = highlightedRows[cursorRowIndex]
      cursorRowHighlight.firstColumn = Math.min(cursorRowHighlight.firstColumn, cursorColumn)
      cursorRowHighlight.lastColumn = Math.max(cursorRowHighlight.lastColumn, cursorColumn + cursorCellWidth)
    }
  }

  if (cursorRowIndex === -1) {
    // A one-character Visual selection has no separately styled cells at all:
    // its only cell is covered by the cursor layer. The status marker and
    // mouse protocol already establish that this is Vim Visual mode, so copy
    // that current cell rather than incorrectly disabling Copy.
    const cursorLine = buffer.getLine(cursorRow)
    const text = cursorLine?.translateToString(false, cursorColumn, cursorColumn + cursorCellWidth) ?? ''
    diagnose('cursor-only', {
      mode,
      modeIndicators,
      cursorRow,
      cursorColumn,
      cursorCell: cursorCellDetails,
      highlightedRows,
      text
    })
    return text ? { text, mode, startRow: cursorRow, endRow: cursorRow } : null
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
  diagnose('highlighted-range', {
    mode,
    modeIndicators,
    cursorRow,
    cursorColumn,
    cursorCell: cursorCellDetails,
    highlightedRows,
    selectionRows,
    text
  })
  if (!text) {
    return null
  }

  return { text, mode, startRow: startRow.row, endRow: endRow.row }
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
