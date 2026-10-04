import type { Terminal } from '@xterm/xterm'

const BLOCK_ELEMENT_START = 0x2580
const BLOCK_ELEMENT_END = 0x259f
const BLOCK_GLYPH_CLASS = 'xterm-block-glyph'

type BlockGlyphRect = { x: number; y: number; width: number; height: number }

function snapSize(size: number, fraction: number, pixelRatio: number) {
  return Math.round(size * fraction * pixelRatio) / pixelRatio
}

function getBlockGlyphRects(codepoint: number, width: number, height: number, pixelRatio: number): BlockGlyphRect[] {
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
    0x259c: ['top right', 'bottom right'],
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
  columnCount: number,
  cellWidth: number,
  cellHeight: number,
  pixelRatio: number
) {
  const document = row.ownerDocument
  const screenBounds = screen.getBoundingClientRect()
  const rowBounds = row.getBoundingClientRect()
  const walker = document.createTreeWalker(row, NodeFilter.SHOW_TEXT)
  const textNodes: Text[] = []
  let current = walker.nextNode()
  while (current) {
    const text = current.textContent ?? ''
    if (/[\u2580-\u259f]/u.test(text)) {
      textNodes.push(current as Text)
    }
    current = walker.nextNode()
  }

  for (const textNode of textNodes) {
    const text = textNode.data
    const fragment = document.createDocumentFragment()
    let plainText = ''
    const blockPositions: Array<{ glyph: HTMLSpanElement; left: number; top: number }> = []
    let blockRun: Array<{ codepoint: number; column: number }> = []

    const flushPlainText = () => {
      if (plainText) {
        fragment.append(document.createTextNode(plainText))
        plainText = ''
      }
    }
    const flushBlockRun = () => {
      if (blockRun.length === 0) {
        return
      }

      const firstColumn = blockRun[0].column
      const lastColumn = blockRun[blockRun.length - 1].column
      const pixelLeft = Math.round((screenBounds.left + firstColumn * cellWidth) * pixelRatio)
      const pixelRight = Math.round((screenBounds.left + (lastColumn + 1) * cellWidth) * pixelRatio)
      const pixelTop = Math.round(rowBounds.top * pixelRatio)
      const pixelBottom = Math.round(rowBounds.bottom * pixelRatio)
      const pixelHeight = pixelBottom - pixelTop
      const pixelWidth = pixelRight - pixelLeft
      const snappedTop = pixelTop / pixelRatio
      const canvas = document.createElement('canvas')
      canvas.width = pixelWidth
      canvas.height = pixelHeight
      const context = canvas.getContext('2d', { alpha: true })
      if (!context) {
        fragment.append(
          document.createTextNode(blockRun.map(({ codepoint }) => String.fromCodePoint(codepoint)).join(''))
        )
        blockRun = []
        return
      }
      context.imageSmoothingEnabled = false
      context.fillStyle = document.defaultView?.getComputedStyle(textNode.parentElement!).color ?? 'rgb(255, 255, 255)'
      for (const { codepoint, column } of blockRun) {
        const cellLeft = Math.round((screenBounds.left + column * cellWidth) * pixelRatio) - pixelLeft
        const cellRight = Math.round((screenBounds.left + (column + 1) * cellWidth) * pixelRatio) - pixelLeft
        const rectangles = getBlockGlyphRects(codepoint, cellRight - cellLeft, pixelHeight, 1)
        for (const rectangle of rectangles) {
          context.fillRect(cellLeft + rectangle.x, rectangle.y, rectangle.width, rectangle.height)
        }
      }

      const glyph = document.createElement('span')
      glyph.className = BLOCK_GLYPH_CLASS
      glyph.setAttribute('aria-hidden', 'true')
      glyph.dataset.blockCount = String(blockRun.length)
      Object.assign(glyph.style, {
        position: 'relative',
        display: 'inline-block',
        boxSizing: 'border-box',
        width: `${blockRun.length * cellWidth}px`,
        height: `${cellHeight}px`,
        margin: '0',
        padding: '0',
        border: '0',
        verticalAlign: 'top',
        lineHeight: '0',
        letterSpacing: '0',
        backgroundColor: 'transparent',
        overflow: 'visible'
      })
      Object.assign(canvas.style, {
        position: 'absolute',
        left: '0',
        top: '0',
        display: 'block',
        width: `${pixelWidth / pixelRatio}px`,
        height: `${pixelHeight / pixelRatio}px`,
        margin: '0',
        padding: '0',
        border: '0',
        pointerEvents: 'none',
        cursor: 'inherit'
      })
      glyph.append(canvas)
      fragment.append(glyph)
      blockPositions.push({
        glyph,
        left: pixelLeft / pixelRatio,
        top: snappedTop
      })
      blockRun = []
    }

    let textOffset = 0
    for (const character of text) {
      const codepoint = character.codePointAt(0)!
      if (codepoint < BLOCK_ELEMENT_START || codepoint > BLOCK_ELEMENT_END) {
        flushBlockRun()
        plainText += character
        textOffset += character.length
        continue
      }

      flushPlainText()
      const range = document.createRange()
      range.setStart(textNode, textOffset)
      range.setEnd(textNode, textOffset + character.length)
      const glyphBounds = range.getBoundingClientRect()
      textOffset += character.length
      const column = Math.max(
        0,
        Math.min(columnCount - 1, Math.round((glyphBounds.left - screenBounds.left) / cellWidth))
      )
      const previous = blockRun[blockRun.length - 1]
      if (previous && column !== previous.column + 1) {
        flushBlockRun()
      }
      blockRun.push({ codepoint, column })
    }

    flushBlockRun()
    flushPlainText()
    textNode.replaceWith(fragment)

    for (const { glyph, left, top } of blockPositions) {
      const glyphBounds = glyph.getBoundingClientRect()
      glyph.style.transform = `translate(${left - glyphBounds.left}px, ${top - glyphBounds.top}px)`
    }
  }
}

/**
 * xterm's DOM renderer draws U+2580-U+259F from the active font. Ghostty
 * rasterizes block elements from terminal-cell geometry, so draw contiguous
 * runs into a device-pixel bitmap to keep font metrics from exposing ANSI
 * background between adjacent pieces of terminal pixel art.
 */
export function registerTerminalBlockGlyphRenderer(terminal: Terminal) {
  const renderRows = (start: number, end: number) => {
    const element = terminal.element
    const screen = element?.querySelector<HTMLElement>('.xterm-screen')
    const rows = element?.querySelector<HTMLElement>('.xterm-rows')?.children
    if (!screen || !rows?.length || terminal.cols < 1 || terminal.rows < 1) {
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
    for (let rowIndex = first; rowIndex <= last; rowIndex++) {
      const row = rows[rowIndex]
      if (row instanceof HTMLElement) {
        replaceBlockGlyphs(row, screen, terminal.cols, cellWidth, cellHeight, window.devicePixelRatio || 1)
      }
    }
  }

  const renderAllRows = () => renderRows(0, terminal.rows - 1)
  const disposables = [terminal.onRender(({ start, end }) => renderRows(start, end)), terminal.onScroll(renderAllRows)]
  renderAllRows()
  return {
    dispose() {
      for (const disposable of disposables) {
        disposable.dispose()
      }
      terminal.element?.querySelectorAll(`.${BLOCK_GLYPH_CLASS}`).forEach((glyph) => glyph.remove())
    }
  }
}
