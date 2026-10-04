/* global window, document, getComputedStyle, requestAnimationFrame, Image */
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { readFileSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const { build } = require(process.env.ESBUILD_MODULE_PATH || 'esbuild')
const { chromium, webkit } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright')
const directory = mkdtempSync(join(tmpdir(), 'fileterm-terminal-contrast-'))
await build({
  stdin: {
    resolveDir: fileURLToPath(new URL('../../../../', import.meta.url)),
    loader: 'ts',
    contents: `
import { createDefaultThemeConfig, createCodexThemeConfig } from '@fileterm/core'
import { applyThemeVariables, getThemeConfigFromTokenPreset } from './apps/tauri/src/renderer/app/theme-config'
import { createTerminalLifecycleRuntime } from './apps/tauri/src/renderer/components/terminal-lifecycle-core'
import './apps/tauri/src/renderer/styles/index.css'
import '@xterm/xterm/css/xterm.css'
window.readTheme = (base, variant) => {
  const config = base === 'fileterm' ? createDefaultThemeConfig(variant)
    : base === 'codex' ? createCodexThemeConfig(variant)
    : getThemeConfigFromTokenPreset(base, variant)
  applyThemeVariables((base === 'codex' ? 'codex-' : 'fileterm-') + variant, config)
  return config.theme.terminal
}
window.createRuntime = (host, theme) => createTerminalLifecycleRuntime({
  hostRef: {current:host}, profileIdRef: {current:'contrast-fixture'}, tabIdRef: {current:'contrast-tab'},
  terminalRef: {current:null}, fitAddonRef: {current:null}, searchAddonRef: {current:null},
  terminalLogColorizerRef: {current:null}, isActiveRef: {current:true},
  setTerminalScrollableElement: () => {}, isMac:false, isWindowsPty:false,
  buildTerminalTheme: () => ({...theme,...theme.ansi})
})
`
  },
  bundle: true,
  outfile: join(directory, 'bundle.js'),
  define: { 'import.meta.env.DEV': 'false' },
  loader: { '.svg': 'dataurl', '.ttf': 'dataurl', '.woff2': 'dataurl', '.png': 'dataurl' },
  logLevel: 'silent'
})

let browser
try {
  const engine = process.env.PLAYWRIGHT_ENGINE === 'webkit' ? webkit : chromium
  browser = await engine.launch({ headless: true, channel: process.env.PLAYWRIGHT_CHANNEL })
  const page = await browser.newPage({
    viewport: { width: 1200, height: 900 },
    deviceScaleFactor: Number(process.env.PLAYWRIGHT_DEVICE_SCALE_FACTOR || 2)
  })
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await page.setContent('<html data-platform="win32"><body></body></html>')
  await page.evaluate(() => {
    window.fileterm = { platform: 'win32' }
  })
  await page.addStyleTag({ content: readFileSync(join(directory, 'bundle.css'), 'utf8') })
  await page.addScriptTag({ content: readFileSync(join(directory, 'bundle.js'), 'utf8') })
  await page.evaluate(() => document.fonts.ready)
  const report = await page.evaluate(async () => {
    const host = document.createElement('div')
    host.style.width = '1100px'
    document.body.append(host)
    const runtime = window.createRuntime(host, window.readTheme('codex', 'dark'))
    const terminal = runtime.terminal
    terminal.resize(80, 14)
    const configuredMinimum = terminal.options.minimumContrastRatio
    const configuredLetterSpacing = terminal.options.letterSpacing
    const write = (text) => new Promise((resolve) => terminal.write(text, resolve))
    const paint = () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
    const canvas = document.createElement('canvas')
    canvas.width = canvas.height = 1
    const context = canvas.getContext('2d', { willReadFrequently: true })
    const rgba = (color) => {
      context.clearRect(0, 0, 1, 1)
      context.fillStyle = color
      context.fillRect(0, 0, 1, 1)
      return [...context.getImageData(0, 0, 1, 1).data]
    }
    const composite = (front, back) =>
      front.slice(0, 3).map((v, i) => (v * front[3] + back[i] * (255 - front[3])) / 255)
    const luminance = (rgb) =>
      rgb
        .map((v) => v / 255)
        .map((v) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4))
        .reduce((sum, v, i) => sum + v * [0.2126, 0.7152, 0.0722][i], 0)
    const contrast = (fg, bg) =>
      (Math.max(luminance(fg), luminance(bg)) + 0.05) / (Math.min(luminance(fg), luminance(bg)) + 0.05)
    const spanAt = (row, column) => {
      let offset = 0
      for (const span of host.querySelectorAll('.xterm-rows > div')[row].children) {
        offset += span.textContent.length
        if (offset > column) return span
      }
      throw new Error('Rendered terminal cell not found')
    }
    const renderedContrast = (row, column, background) => {
      const style = getComputedStyle(spanAt(row, column))
      const bg = background ?? composite(rgba(style.backgroundColor), rgba(terminal.options.theme.background))
      return contrast(composite(rgba(style.color), [...bg, 255]), bg)
    }
    const colors = []
    for (let i = 0; i < 16; i++) colors.push({ name: 'ansi-' + i, sgr: String(i < 8 ? 30 + i : 90 + i - 8) })
    for (let i = 0; i < 256; i++) colors.push({ name: 'indexed-' + i, sgr: '38;5;' + i })
    for (const rgb of [
      '0;0;0',
      '16;16;16',
      '32;32;32',
      '102;102;102',
      '153;153;153',
      '224;224;224',
      '250;250;250',
      '255;255;255'
    ]) {
      colors.push({ name: 'rgb-' + rgb, sgr: '38;2;' + rgb })
    }
    colors.push(
      { name: 'same-rgb-dark-pair', sgr: '38;2;16;16;16;48;2;16;16;16' },
      { name: 'same-rgb-light-pair', sgr: '38;2;250;250;250;48;2;250;250;250' },
      { name: 'inverse-dark-pair', sgr: '7;38;2;16;16;16;48;2;16;16;16' },
      { name: 'inverse-light-pair', sgr: '7;38;2;250;250;250;48;2;250;250;250' },
      { name: 'bold-dark', sgr: '1;30' },
      { name: 'default', sgr: '0' }
    )
    const grid = colors.map(({ sgr }, i) => '\x1b[' + sgr + 'mA\x1b[0m ' + ((i + 1) % 32 === 0 ? '\r\n' : '')).join('')
    const themes = [
      'fileterm',
      'codex',
      ...['dracula', 'nord', 'gruvbox', 'catppuccin', 'solarized', 'tokyonight'].map((name) => 'iterm2-' + name)
    ]
    const failures = []
    let checks = 0
    // A control using the actual runtime proves the old, unprotected path.
    terminal.options.minimumContrastRatio = 1
    await write('\x1b[38;2;16;16;16;48;2;16;16;16mA')
    await paint()
    const controlRatio = renderedContrast(0, 0)
    terminal.options.minimumContrastRatio = configuredMinimum
    for (const base of themes)
      for (const variant of ['dark', 'light']) {
        const theme = window.readTheme(base, variant)
        terminal.options.theme = { ...theme, ...theme.ansi }
        for (const alternate of [false, true]) {
          terminal.reset()
          if (alternate) await write('\x1b[?1049h')
          await write(grid)
          await paint()
          for (let i = 0; i < colors.length; i++) {
            const ratio = renderedContrast(Math.floor(i / 32), (i % 32) * 2)
            checks++
            if (ratio < 4.48) failures.push({ base, variant, alternate, name: colors[i].name, ratio })
          }
        }
        terminal.reset()
        await write('needle')
        terminal.focus()
        terminal.select(0, 0, 6)
        await paint()
        const selectionRatio = renderedContrast(
          0,
          0,
          composite(rgba(theme.selectionBackground), rgba(theme.background))
        )
        const tokenSelectionRatio = contrast(
          composite(rgba(theme.selectionForeground), rgba(theme.background)),
          composite(rgba(theme.selectionBackground), rgba(theme.background))
        )
        const tokenSearchRatio = contrast(rgba(theme.search.activeMatchText), rgba(theme.search.activeMatchBackground))
        checks += 3
        for (const [name, ratio] of [
          ['selected-text', selectionRatio],
          ['selection-tokens', tokenSelectionRatio],
          ['search-tokens', tokenSearchRatio]
        ]) {
          if (ratio < 4.48) failures.push({ base, variant, name, ratio })
        }
        // SearchAddon uses the active match selection color chosen by TerminalView.
        terminal.clearSelection()
        terminal.options.theme = {
          ...theme,
          ...theme.ansi,
          selectionBackground: theme.search.activeMatchBackground,
          selectionForeground: theme.search.activeMatchText
        }
        runtime.searchAddon.findNext('needle', {
          decorations: {
            matchBackground: theme.search.matchBackground,
            matchOverviewRuler: theme.search.matchRuler,
            activeMatchBackground: theme.search.activeMatchBackground,
            activeMatchColorOverviewRuler: theme.search.activeMatchRuler
          }
        })
        await paint()
        const searchRatio = renderedContrast(0, 0, rgba(theme.search.activeMatchBackground))
        checks++
        if (searchRatio < 4.48) failures.push({ base, variant, name: 'active-search-text', ratio: searchRatio })
      }
    // Verify a hostile custom/default foreground and live theme switching on
    // existing RGB black cells. Only the rendered color may change.
    terminal.reset()
    await write('\x1b[38;2;0;0;0mA')
    const before = terminal.buffer.active.getLine(0).getCell(0).getFgColor()
    for (const background of ['#101010', '#fafafa']) {
      terminal.options.theme = { background, foreground: background }
      await paint()
      const ratio = renderedContrast(0, 0)
      checks++
      if (ratio < 4.48) failures.push({ name: 'live-custom-background', background, ratio })
    }
    const after = terminal.buffer.active.getLine(0).getCell(0).getFgColor()
    terminal.reset()
    terminal.options.theme = { background: '#101010', foreground: '#eeeeee' }
    await write('\x1b[2;38;2;16;16;16mA\x1b[0m\x1b[8mA\x1b[0m\x1b[38;2;16;16;16m█')
    await paint()
    const dim = Boolean(terminal.buffer.active.getLine(0).getCell(0).isDim())
    const hidden = Boolean(terminal.buffer.active.getLine(0).getCell(1).isInvisible())
    const blockGlyph = host.querySelector('.xterm-block-glyph')
    const blockGlyphRect = blockGlyph?.getBoundingClientRect()
    const screenRect = host.querySelector('.xterm-screen').getBoundingClientRect()
    const blockContext = blockGlyph?.getContext('2d')
    const cellSize = { width: screenRect.width / terminal.cols, height: screenRect.height / terminal.rows }
    const pixelRatio = window.devicePixelRatio || 1
    const blockPixelX =
      Math.round((screenRect.left + cellSize.width * 2.5) * pixelRatio) - Math.round(screenRect.left * pixelRatio)
    const blockPixel = blockContext?.getImageData(blockPixelX, Math.floor((blockGlyph?.height ?? 1) / 2), 1, 1).data
    const blockColor = blockPixel ? `rgb(${blockPixel[0]}, ${blockPixel[1]}, ${blockPixel[2]})` : null
    const blockSize = blockGlyphRect ? { width: blockGlyphRect.width, height: blockGlyphRect.height } : null
    terminal.reset()
    const blockElements = String.fromCodePoint(...Array.from({ length: 0x20 }, (_, index) => 0x2580 + index))
    await write('\x1b[38;2;16;16;16;48;2;16;16;16m' + blockElements)
    await paint()
    const renderedBlockGlyphRuns = [...host.querySelectorAll('.xterm-block-glyph')]
    const renderedBlockGlyphCount = renderedBlockGlyphRuns.reduce(
      (count, glyph) => count + Number(glyph.dataset.blockCount ?? 0),
      0
    )
    terminal.reset()
    await write('\x1b[38;2;215;119;87;48;2;0;0;0m' + '\u2588'.repeat(12))
    await paint()
    const solidBlockRuns = [...host.querySelectorAll('.xterm-block-glyph')]
    const solidBlockCanvas = solidBlockRuns[0]
    const solidBlockCanvasPixels = solidBlockCanvas
      ?.getContext('2d')
      ?.getImageData(0, 0, solidBlockCanvas.width, solidBlockCanvas.height)
    const solidCellRight =
      Math.round((screenRect.left + 12 * cellSize.width) * pixelRatio) - Math.round(screenRect.left * pixelRatio)
    let solidBlockGapPixelCount = 0
    if (solidBlockCanvasPixels) {
      for (let y = 0; y < solidBlockCanvas.height; y++) {
        for (let x = 0; x < solidCellRight; x++) {
          const alphaIndex = (y * solidBlockCanvas.width + x) * 4 + 3
          if (solidBlockCanvasPixels.data[alphaIndex] !== 255) solidBlockGapPixelCount++
        }
      }
    }
    const solidBlockCharacterCount = Number(solidBlockRuns[0]?.dataset.blockCount ?? 0)

    terminal.reset()
    terminal.options.theme = { background: '#2e3440', foreground: '#eeeeee' }
    await write('\x1b[38;2;215;119;87m \u2590\x1b[48;2;0;0;0m\u259b\u2588\u2588\u2588\u259b\u2588\x1b[49m')
    await paint()
    const ansiSplitCanvases = [...host.querySelectorAll('.xterm-block-glyph')]
    const ansiSplitCanvas = ansiSplitCanvases.find((canvas) => canvas.dataset.rowIndex === '0')
    const ansiSplitPixels = ansiSplitCanvas
      ?.getContext('2d')
      ?.getImageData(0, 0, ansiSplitCanvas.width, ansiSplitCanvas.height)
    const ansiSplitBoundary =
      Math.round((screenRect.left + 2 * cellSize.width) * pixelRatio) - Math.round(screenRect.left * pixelRatio)
    let ansiSplitSeamPixelCount = 0
    if (ansiSplitCanvas && ansiSplitPixels) {
      for (let y = 0; y < ansiSplitCanvas.height; y++) {
        for (const x of [ansiSplitBoundary - 1, ansiSplitBoundary]) {
          const pixelIndex = (y * ansiSplitCanvas.width + x) * 4
          if (
            ansiSplitPixels.data[pixelIndex] !== 215 ||
            ansiSplitPixels.data[pixelIndex + 1] !== 119 ||
            ansiSplitPixels.data[pixelIndex + 2] !== 87 ||
            ansiSplitPixels.data[pixelIndex + 3] !== 255
          ) {
            ansiSplitSeamPixelCount++
          }
        }
      }
    }
    const ansiSplitRawForegroundCount = new Set(
      Array.from({ length: 7 }, (_, index) =>
        terminal.buffer.active
          .getLine(0)
          .getCell(index + 1)
          .getFgColor()
      )
    ).size
    const ansiSplitLeadingColor = ansiSplitCanvas
      ?.getContext('2d')
      ?.getImageData(
        Math.round((screenRect.left + 1.8 * cellSize.width) * pixelRatio) - Math.round(screenRect.left * pixelRatio),
        Math.floor(ansiSplitCanvas.height / 4),
        1,
        1
      ).data
    const nativeGraphicBackgrounds = [...host.querySelectorAll('.xterm-block-glyph-source')].filter(
      (source) => rgba(getComputedStyle(source).backgroundColor)[3] !== 0
    ).length

    const quadrantCodepoints = Array.from({ length: 10 }, (_, index) => 0x2596 + index)
    const expectedQuadrants = [
      [false, false, true, false],
      [false, false, false, true],
      [true, false, false, false],
      [true, false, true, true],
      [true, false, false, true],
      [true, true, true, false],
      [true, true, false, true],
      [false, true, false, false],
      [false, true, true, false],
      [false, true, true, true]
    ]
    const readQuadrants = (column = 0) => {
      const quadrantCanvas = host.querySelector('.xterm-block-glyph-row[data-row-index="0"]')
      const quadrantContext = quadrantCanvas?.getContext('2d')
      if (!quadrantCanvas || !quadrantContext) return null
      const pixels = quadrantContext.getImageData(0, 0, quadrantCanvas.width, quadrantCanvas.height)
      const cellLeft =
        Math.round((screenRect.left + column * cellSize.width) * pixelRatio) - Math.round(screenRect.left * pixelRatio)
      const cellRight =
        Math.round((screenRect.left + (column + 1) * cellSize.width) * pixelRatio) -
        Math.round(screenRect.left * pixelRatio)
      const cellWidthPixels = cellRight - cellLeft
      const xSamples = [0.25, 0.75].map((fraction) => cellLeft + Math.floor(cellWidthPixels * fraction))
      const ySamples = [0.25, 0.75].map((fraction) => Math.floor(quadrantCanvas.height * fraction))
      return [
        pixels.data[(ySamples[0] * quadrantCanvas.width + xSamples[0]) * 4 + 3] === 255,
        pixels.data[(ySamples[0] * quadrantCanvas.width + xSamples[1]) * 4 + 3] === 255,
        pixels.data[(ySamples[1] * quadrantCanvas.width + xSamples[0]) * 4 + 3] === 255,
        pixels.data[(ySamples[1] * quadrantCanvas.width + xSamples[1]) * 4 + 3] === 255
      ]
    }
    let quadrantShapeMismatchCount = 0
    let animatedQuadrantFrameMismatchCount = 0
    terminal.reset()
    await write('\x1b[38;2;215;119;87m' + String.fromCodePoint(...quadrantCodepoints))
    await paint()
    for (let index = 0; index < quadrantCodepoints.length; index++) {
      if (JSON.stringify(readQuadrants(index)) !== JSON.stringify(expectedQuadrants[index])) {
        quadrantShapeMismatchCount++
      }
    }
    for (let index = 0; index < quadrantCodepoints.length; index++) {
      terminal.reset()
      await write('\x1b[38;2;215;119;87m' + String.fromCodePoint(quadrantCodepoints[index]))
      await paint()
      if (
        Number(host.querySelector('.xterm-block-glyph-row[data-row-index="0"]')?.dataset.blockCount ?? 0) !== 1 ||
        JSON.stringify(readQuadrants()) !== JSON.stringify(expectedQuadrants[index])
      ) {
        animatedQuadrantFrameMismatchCount++
      }
    }
    const spriteColorFailures = []
    const spriteColors = [
      { name: 'indexed', sgr: '38;5;173', expected: [215, 135, 95, 255] },
      { name: 'theme-ansi', sgr: '31', expected: [215, 119, 87, 255] },
      { name: 'bold-ansi', sgr: '1;31', expected: [249, 140, 112, 255] },
      { name: 'inverse', sgr: '7;38;2;0;0;0;48;2;215;119;87', expected: [215, 119, 87, 255] },
      { name: 'dim', sgr: '2;38;2;215;119;87', expected: [215, 119, 87, 128] },
      { name: 'osc-palette', sgr: '38;5;173', osc: '\x1b]4;173;rgb:d7/77/57\x1b\\', expected: [215, 119, 87, 255] }
    ]
    for (const fixture of spriteColors) {
      terminal.reset()
      terminal.options.theme = { background: '#2e3440', foreground: '#eeeeee', red: '#d77757', brightRed: '#f98c70' }
      await write((fixture.osc ?? '') + '\x1b[' + fixture.sgr + 'm█')
      await paint()
      const sprite = host.querySelector('.xterm-block-glyph-row[data-row-index="0"]')
      const pixel = sprite?.getContext('2d')?.getImageData(1, 1, 1, 1).data
      // Canvas unpremultiplication may round dim RGB channels by one unit.
      if (
        !pixel ||
        fixture.expected.some((value, index) => Math.abs(pixel[index] - value) > (fixture.name === 'dim' ? 1 : 0))
      ) {
        spriteColorFailures.push({ name: fixture.name, pixel: pixel ? [...pixel] : null })
      }
    }
    terminal.reset()
    await write('\x1b[38;2;16;16;16;48;2;16;16;16mA█B')
    await paint()
    const mixedTextBackgrounds = [0, 2].map((column) => {
      const span = spanAt(0, column)
      return getComputedStyle(span.querySelector('.xterm-block-glyph-text') ?? span).backgroundColor
    })
    runtime.disposeCore()
    return {
      configuredMinimum,
      configuredLetterSpacing,
      controlRatio,
      checks,
      failures,
      before,
      after,
      dim,
      hidden,
      blockColor,
      blockSize,
      cellSize,
      devicePixelRatio: window.devicePixelRatio,
      renderedBlockGlyphCount,
      renderedBlockGlyphRunCount: renderedBlockGlyphRuns.length,
      solidBlockRunCount: solidBlockRuns.length,
      solidBlockCanvasCount: solidBlockCanvas ? 1 : 0,
      solidBlockGapPixelCount,
      solidBlockCharacterCount,
      ansiSplitCanvasCount: ansiSplitCanvases.length,
      ansiSplitBlockCount: Number(ansiSplitCanvas?.dataset.blockCount ?? 0),
      ansiSplitSeamPixelCount,
      ansiSplitRawForegroundCount,
      ansiSplitLeadingColor: ansiSplitLeadingColor ? [...ansiSplitLeadingColor] : null,
      nativeGraphicBackgrounds,
      quadrantShapeMismatchCount,
      animatedQuadrantFrameMismatchCount,
      spriteColorFailures,
      mixedTextBackgrounds
    }
  })
  assert.equal(report.configuredMinimum, 4.5, 'the production terminal runtime must enable contrast protection')
  assert.equal(report.configuredLetterSpacing, 0, 'terminal block art must render with flush character cells')
  assert.equal(report.controlRatio, 1, 'control must reproduce same foreground/background')
  assert.deepEqual(report.failures, [], 'all normal text, selections and search matches must remain readable')
  assert.equal(report.before, 0)
  assert.equal(report.after, 0, 'contrast protection must not rewrite RGB attributes')
  assert.equal(report.dim, true)
  assert.equal(report.hidden, true)
  assert.equal(
    report.blockColor,
    'rgb(16, 16, 16)',
    'graphical block glyphs must inherit their terminal foreground color'
  )
  assert.ok(report.blockSize, 'full block glyph must use the custom cell renderer')
  const pixelTolerance = 1 / report.devicePixelRatio
  assert.ok(Math.abs(report.blockSize.width - report.cellSize.width * 80) <= pixelTolerance)
  assert.ok(Math.abs(report.blockSize.height - report.cellSize.height) <= pixelTolerance)
  assert.equal(report.renderedBlockGlyphCount, 0x20, 'all Unicode block elements must render as exact cell sprites')
  assert.equal(report.renderedBlockGlyphRunCount, 1)
  assert.equal(report.solidBlockRunCount, 1, 'adjacent solid blocks must be grouped as one continuous run')
  assert.equal(report.solidBlockCharacterCount, 12)
  assert.equal(report.solidBlockCanvasCount, 1, 'adjacent full-cell glyphs must share a single sprite canvas')
  assert.equal(report.solidBlockGapPixelCount, 0, 'solid block rows cannot expose their ANSI background')
  assert.equal(report.ansiSplitCanvasCount, 1, 'ANSI style spans on one row must share a sprite canvas')
  assert.equal(report.ansiSplitBlockCount, 7)
  assert.equal(report.ansiSplitSeamPixelCount, 0, 'contiguous block shapes across ANSI spans cannot leave a pixel seam')
  assert.equal(report.ansiSplitRawForegroundCount, 1, 'the mascot fixture uses one ANSI foreground')
  assert.deepEqual(
    report.ansiSplitLeadingColor,
    [215, 119, 87, 255],
    'spaces merged with blocks must not brighten the sprite'
  )
  assert.equal(report.nativeGraphicBackgrounds, 0, 'graphic backgrounds must share the foreground bitmap geometry')
  assert.equal(report.quadrantShapeMismatchCount, 0, 'quadrant block glyphs must match their Unicode pixel shapes')
  assert.equal(
    report.animatedQuadrantFrameMismatchCount,
    0,
    'animated quadrant updates must replace the prior sprite frame'
  )
  assert.deepEqual(
    report.spriteColorFailures,
    [],
    'sprite colors must preserve indexed, themed, bold, inverse, dim and OSC palette attributes'
  )
  assert.deepEqual(
    report.mixedTextBackgrounds,
    ['rgb(16, 16, 16)', 'rgb(16, 16, 16)'],
    'ordinary text beside block glyphs must retain its ANSI background'
  )
  assert.deepEqual(errors, [])
  const border = await page.evaluate(async () => {
    const host = document.createElement('div')
    Object.assign(host.style, { position: 'fixed', left: '40.25px', top: '40.25px', width: '500px' })
    document.body.style.backgroundColor = '#2e3440'
    document.body.append(host)
    const runtime = window.createRuntime(host, { background: '#2e3440', foreground: '#eeeeee' })
    runtime.terminal.resize(24, 5)
    await new Promise((resolve) => runtime.terminal.write('\x1b[38;2;215;119;87m ▐\x1b[48;2;0;0;0m▛███▛█', resolve))
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
    const screen = host.querySelector('.xterm-screen').getBoundingClientRect()
    const canvas = host.querySelector('.xterm-block-glyph-row').getBoundingClientRect()
    window.borderRuntime = runtime
    const ratio = window.devicePixelRatio
    return {
      x: Math.floor((screen.left + (3.5 * screen.width) / 24) * ratio),
      y: Math.floor(canvas.top * ratio)
    }
  })
  const screenshot = (await page.screenshot()).toString('base64')
  const borderPixels = await page.evaluate(
    async ({ screenshot, border }) => {
      const image = new Image()
      image.src = 'data:image/png;base64,' + screenshot
      await image.decode()
      const canvas = document.createElement('canvas')
      canvas.width = image.width
      canvas.height = image.height
      const context = canvas.getContext('2d')
      context.drawImage(image, 0, 0)
      const pixel = (y) => [...context.getImageData(border.x, y, 1, 1).data]
      const result = [-2, -1, 0, 1, 2].map((offset) => pixel(border.y + offset))
      window.borderRuntime.disposeCore()
      return result
    },
    { screenshot, border }
  )
  assert.deepEqual(borderPixels[0], [46, 52, 64, 255], 'the row above the sprite must retain the terminal background')
  assert.ok(
    borderPixels.every((pixel) => pixel[0] >= 46 && pixel[1] >= 52 && pixel[2] >= 64 && pixel[3] === 255),
    'fractional terminal positions must not expose a native black background fringe'
  )
  assert.ok(
    borderPixels.some((pixel) => JSON.stringify(pixel) === JSON.stringify([215, 119, 87, 255])),
    'the sprite top must retain its orange foreground'
  )
  console.log(
    `PASS: ${report.checks} contrast checks, 16 theme variants, all ANSI/256 colors, RGB gray/black/white, explicit and inverse backgrounds, selection/search, live custom themes; dim/hidden/block styles preserved`
  )
} finally {
  await browser?.close()
  rmSync(directory, { recursive: true, force: true })
}
