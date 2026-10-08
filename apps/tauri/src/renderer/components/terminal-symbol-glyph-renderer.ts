import type { Terminal } from '@xterm/xterm'

const SOURCE_CLASS = 'xterm-fitted-symbol-source'
type SourceStyle = { width: string; spacing: string; text: string }
type GlyphCell = { text: string; width: number }

/** DOM xterm fits a fallback glyph's advance with negative letter-spacing,
 * which does not shrink its ink. The next ANSI/selection span can then paint
 * over the right half. Fit overflowing symbols into their buffer cells before
 * any background is painted; keep the original text and cell columns intact.
 */
export function registerTerminalSymbolGlyphRenderer(terminal: Terminal) {
  const rows = terminal.element?.querySelector<HTMLElement>('.xterm-rows')
  const view = rows?.ownerDocument.defaultView
  const context = rows?.ownerDocument.createElement('canvas').getContext('2d')
  if (!rows || !view || !context) return { dispose() {} }
  const sources = new Map<HTMLElement, SourceStyle>()
  const metrics = new Map<string, { left: number; width: number }>()

  const renderRow = (row: HTMLElement, rowIndex: number) => {
    if (!/[\u0080-\u{10ffff}]/u.test(row.textContent ?? '')) return
    const line = terminal.buffer.active.getLine(terminal.buffer.active.viewportY + rowIndex)
    const cellWidth = rows.getBoundingClientRect().width / terminal.cols
    if (!line || cellWidth <= 0) return
    let column = 0
    for (const source of Array.from(row.children)) {
      if (!(source instanceof view.HTMLElement)) continue
      const text = source.textContent ?? ''
      const cells: GlyphCell[] = []
      let offset = 0
      // Map DOM text back to buffer widths, including wide and combined cells.
      // Fail closed for a joiner/decorator that supplies different text.
      while (offset < text.length && column < line.length) {
        const cell = line.getCell(column++)!
        if (cell.getWidth() === 0) continue
        const chars = cell.isInvisible() ? ' ' : cell.getChars() || ' '
        if (text.slice(offset, offset + chars.length).replace(/\u00a0/g, ' ') !== chars.replace(/\u00a0/g, ' ')) return
        cells.push({ text: text.slice(offset, offset + chars.length), width: cell.getWidth() })
        offset += chars.length
      }
      if (offset !== text.length) return
      if (source.classList.contains(SOURCE_CLASS) || !/[\p{Symbol}\p{Punctuation}]/u.test(text)) continue
      // Block elements have their own cell-geometry renderer.
      if (/[\u2580-\u259f]/u.test(text)) continue
      const columns = cells.reduce((total, cell) => total + cell.width, 0)
      if (columns === 0) continue
      const style = view.getComputedStyle(source)
      const font = `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`
      context.font = font
      const fits = cells.map((cell) => {
        if (!/[\p{Symbol}\p{Punctuation}]/u.test(cell.text)) return undefined
        const key = `${font}\n${cell.text}`
        let ink = metrics.get(key)
        if (!ink) {
          const measured = context.measureText(cell.text)
          const left = Math.min(0, -measured.actualBoundingBoxLeft)
          ink = { left, width: Math.max(measured.width, measured.actualBoundingBoxRight) - left }
          metrics.set(key, ink)
          if (metrics.size > 2048) metrics.clear()
        }
        // Even a subpixel overhang can contain an entire device-pixel edge on
        // Retina displays. Only tolerate floating-point measurement noise.
        return ink.width > cellWidth * cell.width + 0.01 ? ink : undefined
      })
      if (!fits.some(Boolean)) continue
      sources.set(source, { width: source.style.width, spacing: source.style.letterSpacing, text })
      source.classList.add(SOURCE_CLASS)
      source.style.width = `${cellWidth * columns}px`
      source.style.letterSpacing = '0px'
      const fragment = row.ownerDocument.createDocumentFragment()
      cells.forEach((cell, index) => {
        const slot = row.ownerDocument.createElement('span')
        slot.style.width = `${cellWidth * cell.width}px`
        // xterm assigns normal weight to every non-bold span, including nested
        // wrappers. Preserve the source's actual font weight in both wrappers.
        slot.style.fontWeight = 'inherit'
        const ink = fits[index]
        if (ink) {
          const glyph = row.ownerDocument.createElement('span')
          const scale = (cellWidth * cell.width) / ink.width
          glyph.textContent = cell.text
          glyph.style.fontWeight = 'inherit'
          glyph.style.transformOrigin = 'left center'
          glyph.style.transform = `translateX(${-ink.left * scale}px) scaleX(${scale})`
          slot.append(glyph)
        } else slot.textContent = cell.text
        fragment.append(slot)
      })
      source.replaceChildren(fragment)
    }
  }
  const render = (start = 0, end = rows.children.length - 1) => {
    // xterm replaces the spans on output, selection, resize and theme changes.
    for (const source of sources.keys()) if (!rows.contains(source)) sources.delete(source)
    for (let index = start; index <= end; index++) {
      const row = rows.children[index]
      if (row instanceof view.HTMLElement) renderRow(row, index)
    }
  }
  const subscription = terminal.onRender(({ start, end }) => render(start, end))
  const observer = new view.MutationObserver((records) => {
    const changed = new Set<HTMLElement>()
    for (const record of records) {
      let row = record.target instanceof view.HTMLElement ? record.target : record.target.parentElement
      while (row && row.parentElement !== rows) row = row.parentElement
      if (row) changed.add(row)
    }
    const children = Array.from(rows.children)
    for (const row of changed) {
      const index = children.indexOf(row)
      if (index >= 0) render(index, index)
    }
  })
  observer.observe(rows, { childList: true, subtree: true })
  render()
  return {
    dispose() {
      subscription.dispose()
      observer.disconnect()
      for (const [source, original] of sources) {
        source.textContent = original.text
        source.style.width = original.width
        source.style.letterSpacing = original.spacing
        source.classList.remove(SOURCE_CLASS)
      }
      sources.clear()
      metrics.clear()
    }
  }
}
