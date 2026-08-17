import type { IBufferLine, IDisposable, ITheme, Terminal } from '@xterm/xterm'

export type TerminalLogColorPalette = Readonly<{
  timestamp: string
  service: string
  error: string
  warning: string
  success: string
  info: string
  debug: string
  address: string
}>

type LogColorKey = keyof TerminalLogColorPalette

type InternalBufferLine = {
  length: number
  _data: Uint32Array
}

type InternalLineView = InternalBufferLine & {
  _line?: InternalBufferLine
}

type LineOriginals = {
  foreground: Uint32Array
  content: Uint32Array
  mask: Uint8Array
}

type ColorMatch = {
  start: number
  end: number
  priority: number
  rgb: number
}

type CompiledRule = {
  priority: number
  rgb: number
  pattern: RegExp
}

const CELL_INDICES = 3
const CELL_CONTENT = 0
const CELL_FOREGROUND = 1
const STYLE_MASK = 0xfc000000
const COLOR_MODE_RGB = 0x03000000
const MAX_RECOLOR_LINES_PER_WRITE = 384

// The rules intentionally stay conservative. They cover the line-oriented
// output users commonly inspect in a remote terminal while avoiding broad
// punctuation rules that would make shell prompts and tabular commands noisy.
const LOG_COLOR_RULE_DEFINITIONS: ReadonlyArray<{
  color: LogColorKey
  pattern: string
}> = [
  {
    color: 'error',
    pattern:
      '\\b(?:error|err|failed|failure|fatal|critical|exception|panic|denied|refused|unauthori[sz]ed|oom|killed|segmentation\\s+fault)\\b'
  },
  {
    color: 'warning',
    pattern: '\\b(?:warn(?:ing)?|caution|deprecated|retry|retries|backoff)\\b'
  },
  {
    color: 'success',
    pattern: '\\b(?:ok|success(?:ful)?|passed|completed|done|ready)\\b'
  },
  {
    color: 'info',
    pattern: '\\b(?:info|notice|note)\\b'
  },
  {
    color: 'debug',
    pattern: '\\b(?:debug|trace|verbose)\\b'
  },
  {
    color: 'timestamp',
    pattern:
      '(?:\\b\\d{4}[-/]\\d{1,2}[-/]\\d{1,2}[T ]\\d{2}:\\d{2}:\\d{2}(?:[.,]\\d+)?(?:Z|[+-]\\d{2}:?\\d{2})?|\\b(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\\s+\\d{1,2}\\s+\\d{2}:\\d{2}:\\d{2}(?:[.,]\\d+)?(?:\\s+\\d{4})?|(?:\\d{1,2}月\\s+)?\\d{1,2}\\s+\\d{2}:\\d{2}:\\d{2}(?:[.,]\\d+)?(?:\\s+\\d{4})?|\\b\\d{1,2}\\/\\d{1,2}\\/\\d{4}\\s+\\d{2}:\\d{2}:\\d{2})'
  },
  {
    color: 'service',
    pattern: '(?:\\b[A-Za-z][A-Za-z0-9_.@/-]*(?:\\[\\d+\\])?)(?=:\\s)'
  },
  {
    color: 'address',
    pattern:
      '(?:\\bhttps?:\\/\\/[^\\s]+|\\b(?:\\d{1,3}\\.){3}\\d{1,3}\\b|\\b(?:[0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}\\b)'
  }
]

function parseRgb(value: string): number | null {
  const normalized = value.trim()
  const short = /^#([\da-f])([\da-f])([\da-f])$/i.exec(normalized)
  const full = /^#([\da-f]{2})([\da-f]{2})([\da-f]{2})(?:[\da-f]{2})?$/i.exec(normalized)
  const components = full ? full.slice(1) : short ? short.slice(1).map((component) => component.repeat(2)) : null
  if (!components) {
    return null
  }

  return components.reduce((result, component) => (result << 8) | Number.parseInt(component, 16), 0)
}

function withRgbForeground(original: number, rgb: number): number {
  return (original & STYLE_MASK) | COLOR_MODE_RGB | (rgb & 0xffffff)
}

function getInternalLine(line: IBufferLine | undefined): InternalBufferLine | null {
  if (!line) {
    return null
  }

  const view = line as unknown as InternalLineView
  if (view._line?._data) {
    return view._line
  }
  if (view._data) {
    return view
  }
  return null
}

function collectMatches(text: string, rules: readonly CompiledRule[]): ColorMatch[] {
  const matches: ColorMatch[] = []
  for (const rule of rules) {
    rule.pattern.lastIndex = 0
    for (const match of text.matchAll(rule.pattern)) {
      const start = match.index ?? -1
      const length = match[0].length
      if (start < 0 || length === 0) {
        continue
      }
      matches.push({
        start,
        end: start + length,
        priority: rule.priority,
        rgb: rule.rgb
      })
    }
  }

  matches.sort((left, right) => left.start - right.start || left.priority - right.priority || right.end - left.end)
  const accepted: ColorMatch[] = []
  for (const match of matches) {
    if (accepted.length === 0 || match.start >= accepted[accepted.length - 1].end) {
      accepted.push(match)
    }
  }
  return accepted
}

function createPaletteFromTheme(theme: ITheme | undefined): TerminalLogColorPalette {
  const pick = (value: string | undefined, fallback: string) => value || fallback
  return {
    timestamp: pick(theme?.brightGreen, pick(theme?.green, '#39d98a')),
    service: pick(theme?.brightYellow, pick(theme?.yellow, '#e5e510')),
    error: pick(theme?.brightRed, pick(theme?.red, '#cd3131')),
    warning: pick(theme?.brightYellow, pick(theme?.yellow, '#e5e510')),
    success: pick(theme?.brightGreen, pick(theme?.green, '#39d98a')),
    info: pick(theme?.brightBlue, pick(theme?.blue, '#2472c8')),
    debug: pick(theme?.brightMagenta, pick(theme?.magenta, '#bc3fbc')),
    address: pick(theme?.brightCyan, pick(theme?.cyan, '#11a8cd'))
  }
}

export function getTerminalLogColorPalette(theme: ITheme | undefined): TerminalLogColorPalette {
  return createPaletteFromTheme(theme)
}

/**
 * Applies optional log colors after xterm has parsed a write.
 *
 * This deliberately mutates xterm's parsed cell foregrounds instead of
 * injecting SGR sequences into the stream. Remote ANSI remains intact in the
 * transcript and in copy/serialization paths, while plain journal/system log
 * lines can still get the same semantic colors as Netcatty. Alternate-screen
 * applications (top, vim, less, etc.) are left untouched.
 */
export class TerminalLogColorizer implements IDisposable {
  private readonly originalWrite: Terminal['write']
  private readonly originalReset: Terminal['reset']
  private readonly originalClear: Terminal['clear']
  private readonly originalResize: Terminal['resize']
  private readonly originals = new WeakMap<InternalBufferLine, LineOriginals>()
  private rules: CompiledRule[] = []
  private disposed = false
  private readonly terminal: Terminal

  constructor(terminal: Terminal, palette: TerminalLogColorPalette) {
    this.terminal = terminal
    this.originalWrite = terminal.write.bind(terminal)
    this.originalReset = terminal.reset.bind(terminal)
    this.originalClear = terminal.clear.bind(terminal)
    this.originalResize = terminal.resize.bind(terminal)
    this.setPalette(palette, false)
    terminal.write = this.write
    terminal.reset = this.reset
    terminal.clear = this.clear
    terminal.resize = this.resize
  }

  setPalette(palette: TerminalLogColorPalette, recolor = true): void {
    const nextRules = LOG_COLOR_RULE_DEFINITIONS.flatMap((definition, priority) => {
      const rgb = parseRgb(palette[definition.color])
      if (rgb === null) {
        return []
      }
      return [{ priority, rgb, pattern: new RegExp(definition.pattern, 'gi') }]
    })
    this.rules = nextRules
    if (recolor && !this.disposed) {
      this.recolorAll()
    }
  }

  dispose(): void {
    if (this.disposed) {
      return
    }
    this.disposed = true
    this.restoreBuffer()
    this.terminal.write = this.originalWrite
    this.terminal.reset = this.originalReset
    this.terminal.clear = this.originalClear
    this.terminal.resize = this.originalResize
  }

  private readonly write: Terminal['write'] = (data, callback) => {
    if (this.disposed) {
      this.originalWrite(data, callback)
      return
    }

    const startedInNormalBuffer = this.terminal.buffer.active.type === 'normal'
    const marker = startedInNormalBuffer ? this.terminal.registerMarker(0) : undefined
    this.originalWrite(data, () => {
      const active = this.terminal.buffer.active
      if (active.type === 'normal') {
        const endY = active.baseY + active.cursorY
        const markerY = marker && !marker.isDisposed ? marker.line : Math.max(0, endY - MAX_RECOLOR_LINES_PER_WRITE)
        this.recolorRange(markerY, endY, true)
      }
      marker?.dispose()
      callback?.()
    })
  }

  private readonly reset: Terminal['reset'] = () => {
    this.clearStoredOriginals()
    return this.originalReset()
  }

  private readonly clear: Terminal['clear'] = () => {
    this.restoreBuffer()
    this.clearStoredOriginals()
    const result = this.originalClear()
    if (this.terminal.buffer.active.type === 'normal') {
      this.terminal.refresh(0, Math.max(this.terminal.rows - 1, 0))
    }
    return result
  }

  private readonly resize: Terminal['resize'] = (cols, rows) => {
    this.restoreBuffer()
    const result = this.originalResize(cols, rows)
    this.recolorAll()
    return result
  }

  private recolorAll(): void {
    const buffer = this.terminal.buffer.normal
    this.recolorRange(0, buffer.length - 1, true)
  }

  private recolorRange(startY: number, endY: number, refresh: boolean): void {
    const buffer = this.terminal.buffer.normal
    const first = Math.max(0, Math.min(startY, endY))
    const last = Math.min(buffer.length - 1, Math.max(startY, endY))
    if (last < first) {
      return
    }

    for (let y = first; y <= last; y += 1) {
      this.recolorLine(y)
    }

    if (refresh && this.terminal.buffer.active.type === 'normal') {
      const viewportY = this.terminal.buffer.active.viewportY
      const startRow = Math.max(0, first - viewportY)
      const endRow = Math.min(this.terminal.rows - 1, last - viewportY)
      if (startRow <= endRow) {
        this.terminal.refresh(startRow, endRow)
      }
    }
  }

  private recolorLine(y: number): void {
    const line = this.terminal.buffer.normal.getLine(y)
    const internal = getInternalLine(line)
    if (!line || !internal) {
      return
    }

    this.restorePhysicalLine(internal)
    const mapped = this.readLine(line)
    const matches = collectMatches(mapped.text, this.rules)
    for (const match of matches) {
      const startColumn = mapped.columns[match.start]
      const endColumn = mapped.columns[match.end - 1]
      if (startColumn === undefined || endColumn === undefined) {
        continue
      }
      this.colorPhysicalRange(internal, startColumn, endColumn + 1, match.rgb)
    }
  }

  private readLine(line: IBufferLine): { text: string; columns: number[] } {
    let text = ''
    const columns: number[] = []
    for (let column = 0; column < line.length; column += 1) {
      const chars = line.getCell(column)?.getChars() ?? ''
      if (!chars) {
        continue
      }
      text += chars
      for (let offset = 0; offset < chars.length; offset += 1) {
        columns.push(column)
      }
    }
    return { text, columns }
  }

  private colorPhysicalRange(line: InternalBufferLine, start: number, end: number, rgb: number): void {
    const originals = this.ensureOriginals(line)
    const last = Math.min(line.length, end)
    for (let column = Math.max(0, start); column < last; column += 1) {
      const dataIndex = column * CELL_INDICES
      if (dataIndex + CELL_FOREGROUND >= line._data.length) {
        continue
      }
      const content = line._data[dataIndex + CELL_CONTENT]
      const foreground = line._data[dataIndex + CELL_FOREGROUND]
      if (!originals.mask[column] || originals.content[column] !== content) {
        originals.foreground[column] = foreground
        originals.content[column] = content
        originals.mask[column] = 1
      }
      line._data[dataIndex + CELL_FOREGROUND] = withRgbForeground(originals.foreground[column], rgb)
    }
  }

  private restorePhysicalLine(line: InternalBufferLine): void {
    const originals = this.originals.get(line)
    if (!originals) {
      return
    }

    for (let column = 0; column < line.length; column += 1) {
      if (!originals.mask[column]) {
        continue
      }
      const dataIndex = column * CELL_INDICES
      if (dataIndex + CELL_FOREGROUND >= line._data.length) {
        originals.mask[column] = 0
        continue
      }
      const content = line._data[dataIndex + CELL_CONTENT]
      if (originals.content[column] === content) {
        line._data[dataIndex + CELL_FOREGROUND] = originals.foreground[column]
      }
      originals.mask[column] = 0
    }
  }

  private restoreBuffer(): void {
    const buffer = this.terminal.buffer.normal
    for (let y = 0; y < buffer.length; y += 1) {
      const internal = getInternalLine(buffer.getLine(y))
      if (internal) {
        this.restorePhysicalLine(internal)
      }
    }
  }

  private ensureOriginals(line: InternalBufferLine): LineOriginals {
    const current = this.originals.get(line)
    if (current && current.foreground.length >= line.length) {
      return current
    }

    const next: LineOriginals = {
      foreground: new Uint32Array(line.length),
      content: new Uint32Array(line.length),
      mask: new Uint8Array(line.length)
    }
    this.originals.set(line, next)
    return next
  }

  private clearStoredOriginals(): void {
    // WeakMap entries are collected with xterm's BufferLine objects. Restoring
    // before clearing is enough; a new BufferLine never reuses our snapshots.
    this.restoreBuffer()
  }
}
