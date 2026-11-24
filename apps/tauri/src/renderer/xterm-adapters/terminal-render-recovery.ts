import type { IDisposable, Terminal } from '@xterm/xterm'
import { getTerminalFontBoldWeight, observeCanvasTextMetrics } from '../app/font-metrics'

// WebKit can keep a newly visible surface at zero size for several compositor frames.
export const TERMINAL_ACTIVATION_RECOVERY_ATTEMPTS = 16

/** WebGL canvas resizing clears its pixels; redraw before the browser can paint that empty surface. */
export function repaintTerminalRender(terminal: Terminal) {
  // shortcut: xterm 6 WebGL renderer internals; recheck when upgrading xterm.
  const renderer = (
    terminal as Terminal & {
      _core?: {
        _renderService?: { _renderer?: { value?: { _gl?: unknown; renderRows(start: number, end: number): void } } }
      }
    }
  )._core?._renderService?._renderer?.value
  if (renderer?._gl) {
    renderer.renderRows(0, Math.max(0, terminal.rows - 1))
  } else {
    terminal.refresh(0, Math.max(0, terminal.rows - 1))
  }
}

export function recoverTerminalRenderSurface(
  terminal: Terminal,
  host: HTMLElement,
  isActive: () => boolean,
  resize: () => void,
  report: (stage: string, details: Record<string, unknown>) => void
): boolean {
  const { width, height } = host.getBoundingClientRect()
  if (width <= 0 || height <= 0) {
    report('recovery-skipped-zero-host', { hostWidth: Math.round(width), hostHeight: Math.round(height) })
    return true
  }

  terminal.clearTextureAtlas()
  resize()
  // Resizing can activate another tab; never focus a surface that became inactive.
  if (!isActive()) return false
  terminal.refresh(0, Math.max(terminal.rows - 1, 0))
  terminal.focus()
  report('recovery-complete', {})
  return false
}

export function refreshTerminalRender(terminal: Terminal) {
  terminal.clearTextureAtlas()
  terminal.refresh(0, Math.max(terminal.rows - 1, 0))
}

/** A stale WebView surface can otherwise stay black after a TUI switches buffers. */
export function registerTerminalBufferRenderRecovery(
  terminal: Terminal,
  onBufferChanged: (previousBufferType: string, nextBufferType: string) => void
): IDisposable {
  let renderedBufferType = terminal.buffer.active.type
  return terminal.onWriteParsed(() => {
    const nextBufferType = terminal.buffer.active.type
    if (nextBufferType === renderedBufferType) return

    const previousBufferType = renderedBufferType
    renderedBufferType = nextBufferType
    refreshTerminalRender(terminal)
    onBufferChanged(previousBufferType, nextBufferType)
  })
}

/** Font loading and DPI changes do not necessarily change the host's CSS box. */
export function observeTerminalFontMetrics(terminal: Terminal, onMetricsChanged: () => void) {
  let disposed = false
  const dispose = observeCanvasTextMetrics((fontFamily) => {
    terminal.options.fontWeightBold = getTerminalFontBoldWeight()
    // xterm ignores an unchanged family; pulse the whole stack to invalidate DOM glyph widths too.
    for (const family of [`${fontFamily}, ${fontFamily}`, fontFamily]) {
      if (disposed) return
      terminal.options.fontFamily = family
      refreshTerminalRender(terminal)
      onMetricsChanged()
    }
  })
  return () => {
    disposed = true
    dispose()
  }
}
