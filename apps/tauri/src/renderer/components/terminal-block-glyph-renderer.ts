import type { IBufferCell, Terminal } from '@xterm/xterm'

const BLOCK_ELEMENT_START = 0x2580
const BLOCK_ELEMENT_END = 0x259f
const BLOCK_GLYPH_CLASS = 'xterm-block-glyph'
const BLOCK_GLYPH_PLACEHOLDER_CLASS = 'xterm-block-glyph-placeholder'
const BLOCK_GLYPH_SOURCE_CLASS = 'xterm-block-glyph-source'
const BLOCK_GLYPH_TEXT_CLASS = 'xterm-block-glyph-text'

type BlockGlyphRect = { x: number; y: number; width: number; height: number }
type GlyphColor = { css: string; rgba: number }
type GlyphPalette = { foreground: GlyphColor; background: GlyphColor; ansi: GlyphColor[] }
type ThemeServiceView = { _core?: { _themeService?: { colors?: GlyphPalette } } }
type SourceBackground = { resolved: string; inline: string; priority: string }
type BufferGlyph = { codepoint: number; column: number; color: string }

function isGraphicGlyph(codepoint: number) {
  return (
    (codepoint >= BLOCK_ELEMENT_START && codepoint <= BLOCK_ELEMENT_END) ||
    codepoint === 0x2500 ||
    codepoint === 0x2501 ||
    codepoint === 0x2550
  )
}

function getGlyphForeground(cell: IBufferCell, palette: GlyphPalette, drawBoldTextInBrightColors: boolean) {
  const inverse = Boolean(cell.isInverse())
  const rgb = inverse ? cell.isBgRGB() : cell.isFgRGB()
  const indexed = inverse ? cell.isBgPalette() : cell.isFgPalette()
  let value = inverse ? cell.getBgColor() : cell.getFgColor()
  let color: GlyphColor
  if (rgb) {
    color = { css: `rgb(${value >>> 16}, ${(value >>> 8) & 255}, ${value & 255})`, rgba: (value << 8) | 255 }
  } else if (indexed) {
    if (cell.isBold() && value < 8 && drawBoldTextInBrightColors) value += 8
    color = palette.ansi[value]
  } else {
    color = inverse ? palette.background : palette.foreground
  }
  // Graphic cells bypass text contrast adjustment. A DOM span can start with
  // a space and merge later blocks, incorrectly inheriting that space's adjusted
  // color. Resolve from the buffer and live palette instead (including OSC).
  if (cell.isDim() || (inverse && !rgb && !indexed)) {
    const rgba = color.rgba >>> 0
    const alpha = cell.isDim() ? (inverse && !rgb && !indexed ? 0.5 : (rgba & 255) / 510) : 1
    return `rgba(${rgba >>> 24}, ${(rgba >>> 16) & 255}, ${(rgba >>> 8) & 255}, ${alpha})`
  }
  return color.css
}

function restoreSourceBackground(source: HTMLElement, backgrounds: WeakMap<HTMLElement, SourceBackground>) {
  const background = backgrounds.get(source)
  if (!background) return
  if (background.inline) source.style.setProperty('background-color', background.inline, background.priority)
  else source.style.removeProperty('background-color')
  source.classList.remove(BLOCK_GLYPH_SOURCE_CLASS)
  backgrounds.delete(source)
}

function snapSize(size: number, fraction: number, pixelRatio: number) {
  return Math.round(size * fraction * pixelRatio) / pixelRatio
}

function getBlockGlyphRects(codepoint: number, width: number, height: number, pixelRatio: number): BlockGlyphRect[] {
  if (codepoint === 0x2500 || codepoint === 0x2501 || codepoint === 0x2550) {
    const light = Math.max(1, Math.round(height / 16))
    const thickness = codepoint === 0x2501 ? light * 2 : light
    const y = Math.floor((height - thickness) / 2)
    // Draw full cell edges on the row bitmap, avoiding font bearings and antialiasing seams.
    return codepoint === 0x2550
      ? [
          { x: 0, y: y - light, width, height: light },
          { x: 0, y: y + light, width, height: light }
        ]
      : [{ x: 0, y, width, height: thickness }]
  }
  const halfWidth = snapSize(width, 0.5, pixelRatio)
  const halfHeight = snapSize(height, 0.5, pixelRatio)
  const full = [{ x: 0, y: 0, width, height }]
  if (codepoint === 0x2580) {
    return [{ x: 0, y: 0, width, height: halfHeight }]
  }
  if (codepoint >= 0x2581 && codepoint <= 0x2587) {
    const blockHeight = snapSize(height, (codepoint - 0x2580) / 8, pixelRatio)
    return [{ x: 0, y: height - blockHeight, width, height: blockHeight }]
  }
  if (codepoint === 0x2588) {
    return full
  }
  if (codepoint >= 0x2589 && codepoint <= 0x258f) {
    const blockWidth = snapSize(width, (0x2590 - codepoint) / 8, pixelRatio)
    return [{ x: 0, y: 0, width: blockWidth, height }]
  }
  if (codepoint === 0x2590) {
    return [{ x: width - halfWidth, y: 0, width: halfWidth, height }]
  }
  if (codepoint >= 0x2591 && codepoint <= 0x2593) {
    const patterns = [
      [{ x: 0, y: 0 }],
      [
        { x: 0, y: 0 },
        { x: halfWidth, y: halfHeight }
      ],
      [
        { x: 0, y: 0 },
        { x: halfWidth, y: 0 },
        { x: 0, y: halfHeight }
      ]
    ]
    return patterns[codepoint - 0x2591].map(({ x, y }) => ({
      x,
      y,
      width: halfWidth,
      height: halfHeight
    }))
  }
  if (codepoint === 0x2594) {
    return [{ x: 0, y: 0, width, height: snapSize(height, 0.125, pixelRatio) }]
  }
  if (codepoint === 0x2595) {
    const blockWidth = snapSize(width, 0.125, pixelRatio)
    return [{ x: width - blockWidth, y: 0, width: blockWidth, height }]
  }

  const quadrants: Record<number, Array<'top left' | 'top right' | 'bottom left' | 'bottom right'>> = {
    0x2596: ['bottom left'],
    0x2597: ['bottom right'],
    0x2598: ['top left'],
    0x2599: ['top left', 'bottom left', 'bottom right'],
    0x259a: ['top left', 'bottom right'],
    0x259b: ['top left', 'top right', 'bottom left'],
    0x259c: ['top left', 'top right', 'bottom right'],
    0x259d: ['top right'],
    0x259e: ['top right', 'bottom left'],
    0x259f: ['top right', 'bottom left', 'bottom right']
  }
  const quadrantRects: Record<'top left' | 'top right' | 'bottom left' | 'bottom right', BlockGlyphRect> = {
    'top left': { x: 0, y: 0, width: halfWidth, height: halfHeight },
    'top right': { x: width - halfWidth, y: 0, width: halfWidth, height: halfHeight },
    'bottom left': { x: 0, y: height - halfHeight, width: halfWidth, height: halfHeight },
    'bottom right': { x: width - halfWidth, y: height - halfHeight, width: halfWidth, height: halfHeight }
  }
  return (quadrants[codepoint] ?? []).map((quadrant) => quadrantRects[quadrant])
}

function replaceBlockGlyphs(
  row: HTMLElement,
  screen: HTMLElement,
  rowIndex: number,
  columnCount: number,
  cellWidth: number,
  pixelRatio: number,
  bufferGlyphs: BufferGlyph[],
  sourceBackgrounds: WeakMap<HTMLElement, SourceBackground>,
  screenBounds: DOMRect,
  rowBounds: DOMRect
) {
  const document = row.ownerDocument
  const existingCanvas = screen.querySelector<HTMLCanvasElement>(`.xterm-block-glyph-row[data-row-index="${rowIndex}"]`)
  const walker = document.createTreeWalker(row, NodeFilter.SHOW_TEXT)
  const textNodes: Text[] = []
  let current = walker.nextNode()
  while (current) {
    const text = current.textContent ?? ''
    if (/[\u2500\u2501\u2550\u2580-\u259f]/u.test(text)) {
      textNodes.push(current as Text)
    }
    current = walker.nextNode()
  }

  const domGlyphs: Array<{ codepoint: number; color: string; background: string; decorated: boolean }> = []
  for (const textNode of textNodes) {
    const text = textNode.data
    const placeholder = textNode.parentElement?.classList.contains(BLOCK_GLYPH_PLACEHOLDER_CLASS)
      ? textNode.parentElement
      : null
    const colorElement = placeholder?.parentElement ?? textNode.parentElement
    const source = colorElement ?? row
    const style = document.defaultView!.getComputedStyle(source)
    if (!sourceBackgrounds.has(source)) {
      sourceBackgrounds.set(source, {
        resolved: style.backgroundColor,
        inline: source.style.getPropertyValue('background-color'),
        priority: source.style.getPropertyPriority('background-color')
      })
    }
    const background = sourceBackgrounds.get(source)!.resolved
    for (const character of text) {
      const codepoint = character.codePointAt(0)!
      if (isGraphicGlyph(codepoint)) {
        domGlyphs.push({
          codepoint,
          color: style.color,
          background,
          decorated: source.classList.contains('xterm-decoration-top')
        })
      }
    }
  }

  const glyphsMatchBuffer =
    bufferGlyphs.length === domGlyphs.length &&
    bufferGlyphs.every(({ codepoint }, index) => codepoint === domGlyphs[index].codepoint)
  if (!glyphsMatchBuffer) {
    existingCanvas?.remove()
    row.querySelectorAll<HTMLElement>(`.${BLOCK_GLYPH_SOURCE_CLASS}`).forEach((source) => {
      restoreSourceBackground(source, sourceBackgrounds)
    })
    row.querySelectorAll<HTMLElement>(`.${BLOCK_GLYPH_TEXT_CLASS}`).forEach((text) => {
      text.replaceWith(document.createTextNode(text.textContent ?? ''))
    })
    row.querySelectorAll<HTMLElement>(`.${BLOCK_GLYPH_PLACEHOLDER_CLASS}`).forEach((placeholder) => {
      placeholder.style.color = ''
    })
    return
  }

  const glyphs = bufferGlyphs.map(({ codepoint, column, color }, index) => ({
    codepoint,
    column: Math.max(0, Math.min(columnCount - 1, column)),
    color: domGlyphs[index].decorated ? domGlyphs[index].color : color,
    background: domGlyphs[index].background
  }))

  if (glyphs.length === 0) {
    existingCanvas?.remove()
    return
  }

  // Snap once in the screen's local grid. Snapping the page origin and then
  // offsetting an absolutely positioned canvas can make the browser snap its
  // compositing layer a second time at fractional window/UI zoom positions.
  const pixelTop = Math.round((rowBounds.top - screenBounds.top) * pixelRatio)
  const pixelBottom = Math.round((rowBounds.bottom - screenBounds.top) * pixelRatio)
  const pixelWidth = Math.round(screenBounds.width * pixelRatio)
  const pixelHeight = pixelBottom - pixelTop
  const canvas = existingCanvas ?? document.createElement('canvas')
  canvas.className = `${BLOCK_GLYPH_CLASS} xterm-block-glyph-row`
  canvas.dataset.rowIndex = String(rowIndex)
  canvas.dataset.blockCount = String(glyphs.length)
  canvas.setAttribute('aria-hidden', 'true')
  if (canvas.width !== pixelWidth) canvas.width = pixelWidth
  if (canvas.height !== pixelHeight) canvas.height = pixelHeight
  const context = canvas.getContext('2d', { alpha: true })
  if (!context) {
    return
  }

  context.imageSmoothingEnabled = false
  context.clearRect(0, 0, pixelWidth, pixelHeight)
  for (const { codepoint, column, color, background } of glyphs) {
    const cellLeft = Math.round(column * cellWidth * pixelRatio)
    const cellRight = Math.round((column + 1) * cellWidth * pixelRatio)
    const rectangles = getBlockGlyphRects(codepoint, cellRight - cellLeft, pixelHeight, 1)
    context.fillStyle = background
    context.fillRect(cellLeft, 0, cellRight - cellLeft, pixelHeight)
    context.fillStyle = color
    for (const rectangle of rectangles) {
      context.fillRect(cellLeft + rectangle.x, rectangle.y, rectangle.width, rectangle.height)
    }
  }

  Object.assign(canvas.style, {
    position: 'absolute',
    left: '0',
    top: `${pixelTop / pixelRatio}px`,
    display: 'block',
    width: `${pixelWidth / pixelRatio}px`,
    height: `${pixelHeight / pixelRatio}px`,
    margin: '0',
    padding: '0',
    border: '0',
    pointerEvents: 'none',
    // Selected DOM text is at level 2; graphical cells must also stay above
    // xterm's opaque selection layer (level 1).
    zIndex: '2',
    cursor: 'inherit'
  })
  return () => {
    if (!existingCanvas) {
      const selectionLayer = screen.querySelector('.xterm-selection')
      screen.insertBefore(canvas, selectionLayer ?? null)
    }

    for (const textNode of textNodes) {
      const placeholder = textNode.parentElement?.classList.contains(BLOCK_GLYPH_PLACEHOLDER_CLASS)
        ? textNode.parentElement
        : null
      const source = placeholder?.parentElement ?? textNode.parentElement
      if (!source) continue
      const background = sourceBackgrounds.get(source)!.resolved
      // Span backgrounds use font line boxes rather than snapped terminal cells.
      // Move graphical backgrounds into the same bitmap as their foreground so
      // fractional UI positions cannot expose a black border above the sprite.
      if (!source.classList.contains('xterm-cursor')) {
        source.classList.add(BLOCK_GLYPH_SOURCE_CLASS)
        source.style.backgroundColor = 'transparent'
      }
      if (placeholder) {
        placeholder.style.color = 'transparent'
        continue
      }
      const fragment = document.createDocumentFragment()
      let plainText = ''
      const flushPlainText = () => {
        if (plainText) {
          const text = document.createElement('span')
          text.className = BLOCK_GLYPH_TEXT_CLASS
          text.style.backgroundColor = background
          text.textContent = plainText
          fragment.append(text)
          plainText = ''
        }
      }
      for (const character of textNode.data) {
        const codepoint = character.codePointAt(0)!
        if (!isGraphicGlyph(codepoint)) {
          plainText += character
          continue
        }

        flushPlainText()
        const placeholder = document.createElement('span')
        placeholder.className = BLOCK_GLYPH_PLACEHOLDER_CLASS
        placeholder.textContent = character
        placeholder.style.color = 'transparent'
        fragment.append(placeholder)
      }
      flushPlainText()
      textNode.replaceWith(fragment)
    }

    row.querySelectorAll<HTMLElement>('.xterm-cursor').forEach((cursor) => {
      cursor.style.position = 'relative'
      cursor.style.zIndex = '2'
    })
  }
}

/**
 * xterm's DOM renderer draws blocks and horizontal rules from the active font. Ghostty
 * rasterizes graphical cells from terminal-cell geometry, so draw each row into
 * one device-pixel bitmap using the terminal buffer's exact cell columns. This
 * keeps ANSI style-span boundaries and glyph bearings from shifting block art.
 */
export function registerTerminalBlockGlyphRenderer(terminal: Terminal) {
  const sourceBackgrounds = new WeakMap<HTMLElement, SourceBackground>()
  const renderRows = (start: number, end: number) => {
    const element = terminal.element
    const screen = element?.querySelector<HTMLElement>('.xterm-screen')
    const rows = element?.querySelector<HTMLElement>('.xterm-rows')?.children
    // Guard this narrow xterm 6 internal access; a future incompatible version
    // falls back to its native DOM renderer rather than guessing palette colors.
    const palette = (terminal as unknown as ThemeServiceView)._core?._themeService?.colors
    if (!screen || !rows?.length || !palette || terminal.cols < 1 || terminal.rows < 1) {
      return
    }

    const bounds = screen.getBoundingClientRect()
    const cellWidth = bounds.width / terminal.cols
    const cellHeight = bounds.height / terminal.rows
    if (cellWidth <= 0 || cellHeight <= 0) {
      return
    }

    const first = Math.max(0, start)
    const last = Math.min(end, rows.length - 1)
    // Snapshot geometry before any row canvas/wrapper mutations force layout.
    const rowBounds = Array.from(rows)
      .slice(first, last + 1)
      .map((row) => row.getBoundingClientRect())
    const updates: Array<() => void> = []
    for (let rowIndex = first; rowIndex <= last; rowIndex++) {
      const row = rows[rowIndex]
      if (row instanceof HTMLElement) {
        const buffer = terminal.buffer.active
        const bufferLine = buffer.getLine(buffer.viewportY + rowIndex)
        const bufferGlyphs: BufferGlyph[] = []
        if (bufferLine) {
          for (let column = 0; column < Math.min(terminal.cols, bufferLine.length); column++) {
            const cell = bufferLine.getCell(column)
            const codepoint = cell?.getCode() ?? 0
            if (isGraphicGlyph(codepoint)) {
              bufferGlyphs.push({
                codepoint,
                column,
                color: getGlyphForeground(cell!, palette, terminal.options.drawBoldTextInBrightColors ?? true)
              })
            }
          }
        }
        const update = replaceBlockGlyphs(
          row,
          screen,
          rowIndex,
          terminal.cols,
          cellWidth,
          window.devicePixelRatio || 1,
          bufferGlyphs,
          sourceBackgrounds,
          bounds,
          rowBounds[rowIndex - first]
        )
        if (update) updates.push(update)
      }
    }
    for (const update of updates) update()
  }

  const renderAllRows = () => renderRows(0, terminal.rows - 1)
  const disposables = [
    terminal.onRender(({ start, end }) => renderRows(start, end)),
    terminal.onScroll(renderAllRows),
    terminal.onResize(renderAllRows)
  ]
  renderAllRows()
  return {
    dispose() {
      for (const disposable of disposables) {
        disposable.dispose()
      }
      terminal.element?.querySelectorAll(`.${BLOCK_GLYPH_CLASS}`).forEach((glyph) => glyph.remove())
      terminal.element?.querySelectorAll(`.${BLOCK_GLYPH_PLACEHOLDER_CLASS}`).forEach((glyph) => {
        glyph.replaceWith(glyph.ownerDocument.createTextNode(glyph.textContent ?? ''))
      })
      terminal.element?.querySelectorAll(`.${BLOCK_GLYPH_TEXT_CLASS}`).forEach((text) => {
        text.replaceWith(text.ownerDocument.createTextNode(text.textContent ?? ''))
      })
      terminal.element?.querySelectorAll<HTMLElement>(`.${BLOCK_GLYPH_SOURCE_CLASS}`).forEach((source) => {
        restoreSourceBackground(source, sourceBackgrounds)
      })
    }
  }
}
