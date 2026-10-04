import type { IDisposable, Terminal } from '@xterm/xterm'

// xterm 6 stores content/foreground/background in three Uint32 words per cell.
// This is the same guarded buffer access used by TerminalLogColorizer. Keep
// changes here confined to the color bits; text, width and style stay intact.
type InternalLine = { _data: Uint32Array }
type LineView = { _line?: InternalLine }
const CELL_WORDS = 3
const COLOR_MASK = 0x03ffffff
const RGB_WHITE = 0x03ffffff
const INVERSE = 0x04000000
const COLOR_MODE_MASK = 0x03000000

/**
 * Some TUIs (including Claude Code's dark RGB theme) use literal white for
 * ordinary text. On the default canvas, treat that foreground as theme text.
 * Explicit backgrounds and inverse cells retain the program's color pairing.
 * Raw output/transcripts and xterm's parser attributes are never rewritten.
 */
export function registerTerminalForegroundAdapter(terminal: Terminal): IDisposable {
  const adaptViewport = () => {
    const buffer = terminal.buffer.active
    let changed = false
    for (let row = buffer.viewportY; row < Math.min(buffer.length, buffer.viewportY + terminal.rows); row++) {
      const line = buffer.getLine(row)
      const data = (line as LineView | undefined)?._line?._data
      if (!line || !(data instanceof Uint32Array) || data.length !== line.length * CELL_WORDS) continue

      for (let column = 0; column < line.length; column++) {
        const index = column * CELL_WORDS
        const foreground = data[index + 1]
        const background = data[index + 2]
        if ((foreground & COLOR_MASK) === RGB_WHITE && !(foreground & INVERSE) && !(background & COLOR_MODE_MASK)) {
          data[index + 1] = foreground & ~COLOR_MASK
          changed = true
        }
      }
    }
    if (changed) terminal.refresh(0, Math.max(0, terminal.rows - 1))
  }

  // Only inspect the viewport. Scrollback is adapted as it becomes visible,
  // so a busy terminal does not repeatedly scan thousands of history rows.
  const subscriptions = [
    terminal.onWriteParsed(adaptViewport),
    terminal.onScroll(adaptViewport),
    terminal.onResize(adaptViewport),
    terminal.buffer.onBufferChange(adaptViewport)
  ]
  return { dispose: () => subscriptions.forEach((subscription) => subscription.dispose()) }
}
