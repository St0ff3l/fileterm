import type { Terminal } from '@xterm/xterm'

type SelectionPalette = {
  selectionBackgroundOpaque: { css: string }
  selectionInactiveBackgroundOpaque: { css: string }
}
type ThemeServiceView = { _core?: { _themeService?: { colors?: SelectionPalette } } }

/** The DOM selection layer already paints the background. Repainting it on
 * every selected span covers fallback glyphs that extend beyond their cell
 * (for example ✻ and box drawing). Keep xterm's foreground/contrast decisions,
 * but let the selection layer own the background once per row.
 */
export function registerTerminalSelectionRenderer(terminal: Terminal) {
  const render = () => {
    const selection = terminal.getSelectionPosition()
    if (!selection) return
    // Guard the same xterm 6 palette boundary as the block glyph renderer.
    const colors = (terminal as unknown as ThemeServiceView)._core?._themeService?.colors
    const rows = terminal.element?.querySelector('.xterm-rows')
    if (!colors || !rows) return
    const cellWidth = rows.getBoundingClientRect().width / terminal.cols
    if (cellWidth <= 0) return
    const probe = rows.ownerDocument.createElement('span')
    const backgrounds = [colors.selectionBackgroundOpaque, colors.selectionInactiveBackgroundOpaque].map(({ css }) => {
      probe.style.backgroundColor = css
      return probe.style.backgroundColor
    })
    Array.from(rows.children).forEach((row, index) => {
      const bufferRow = terminal.buffer.active.viewportY + index
      if (bufferRow < selection.start.y || bufferRow > selection.end.y) return
      const firstColumn = bufferRow === selection.start.y ? selection.start.x : 0
      const lastColumn = bufferRow === selection.end.y ? selection.end.x : terminal.cols
      const rowLeft = row.getBoundingClientRect().left
      row.querySelectorAll<HTMLElement>(':scope > .xterm-decoration-top').forEach((span) => {
        // Search/top decorations with their own background retain it. xterm
        // marks selected text as a top decoration too, with this palette color.
        const column = Math.round((span.getBoundingClientRect().left - rowLeft) / cellWidth)
        if (
          column >= firstColumn &&
          column < lastColumn &&
          !span.classList.contains('xterm-cursor') &&
          backgrounds.includes(span.style.backgroundColor)
        ) {
          span.style.backgroundColor = 'transparent'
        }
      })
    })
  }
  const subscription = terminal.onRender(render)
  // DOM selection updates repaint rows directly without emitting onRender.
  // Request the public render cycle so both this adapter and graphical cells
  // receive the new selection, including partial selections and clearing.
  const view = terminal.element?.ownerDocument.defaultView
  let refreshFrame: number | undefined
  const refresh = () => {
    if (!view || refreshFrame !== undefined) return
    // SelectionService updates the DOM renderer on its own animation frame.
    // Wait for that frame before asking graphical overlays to repaint.
    refreshFrame = view.requestAnimationFrame(() => {
      refreshFrame = undefined
      terminal.refresh(0, Math.max(0, terminal.rows - 1))
    })
  }
  const selectionSubscription = terminal.onSelectionChange(refresh)
  // While the mouse button is held, xterm replaces row spans directly and
  // defers onSelectionChange until mouseup. Observe those replacements so a
  // later drag frame cannot restore the opaque backgrounds over glyphs.
  // Only a new selection schedules a public refresh (for graphical cells);
  // refreshing itself also replaces spans and must not create a render loop.
  let observedSelection = JSON.stringify(terminal.getSelectionPosition())
  const observer =
    view &&
    new view.MutationObserver(() => {
      render()
      const selection = JSON.stringify(terminal.getSelectionPosition())
      if (selection !== observedSelection) {
        observedSelection = selection
        refresh()
      }
    })
  const rows = terminal.element?.querySelector('.xterm-rows')
  if (rows) observer?.observe(rows, { childList: true, subtree: true })
  terminal.textarea?.addEventListener('focus', refresh)
  terminal.textarea?.addEventListener('blur', refresh)
  return {
    dispose() {
      subscription.dispose()
      selectionSubscription.dispose()
      observer?.disconnect()
      if (refreshFrame !== undefined) view?.cancelAnimationFrame(refreshFrame)
      terminal.textarea?.removeEventListener('focus', refresh)
      terminal.textarea?.removeEventListener('blur', refresh)
    }
  }
}
