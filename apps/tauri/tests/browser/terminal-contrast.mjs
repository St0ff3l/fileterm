/* global window, document, getComputedStyle, requestAnimationFrame */
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
  const page = await browser.newPage({ viewport: { width: 1200, height: 900 } })
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await page.setContent('<html data-platform="win32"><body></body></html>')
  await page.evaluate(() => {
    window.fileterm = { platform: 'win32' }
  })
  await page.addStyleTag({ content: readFileSync(join(directory, 'bundle.css'), 'utf8') })
  await page.addScriptTag({ content: readFileSync(join(directory, 'bundle.js'), 'utf8') })
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
    const blockColor = blockGlyph ? getComputedStyle(blockGlyph).color : null
    const blockSize = blockGlyphRect ? { width: blockGlyphRect.width, height: blockGlyphRect.height } : null
    const cellSize = { width: screenRect.width / terminal.cols, height: screenRect.height / terminal.rows }
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
    const solidBlockCanvas = solidBlockRuns[0]?.querySelector('canvas')
    const solidBlockCanvasPixels = solidBlockCanvas
      ?.getContext('2d')
      ?.getImageData(0, 0, solidBlockCanvas.width, solidBlockCanvas.height)
    let solidBlockTransparentPixelCount = 0
    if (solidBlockCanvasPixels) {
      for (let index = 3; index < solidBlockCanvasPixels.data.length; index += 4) {
        if (solidBlockCanvasPixels.data[index] !== 255) solidBlockTransparentPixelCount++
      }
    }
    const solidBlockCharacterCount = Number(solidBlockRuns[0]?.dataset.blockCount ?? 0)
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
      solidBlockTransparentPixelCount,
      solidBlockCharacterCount
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
  assert.ok(Math.abs(report.blockSize.width - report.cellSize.width) <= pixelTolerance)
  assert.ok(Math.abs(report.blockSize.height - report.cellSize.height) <= pixelTolerance)
  assert.equal(report.renderedBlockGlyphCount, 0x20, 'all Unicode block elements must render as exact cell sprites')
  assert.equal(report.renderedBlockGlyphRunCount, 1)
  assert.equal(report.solidBlockRunCount, 1, 'adjacent solid blocks must be grouped as one continuous run')
  assert.equal(report.solidBlockCharacterCount, 12)
  assert.equal(report.solidBlockCanvasCount, 1, 'adjacent full-cell glyphs must share a single sprite canvas')
  assert.equal(report.solidBlockTransparentPixelCount, 0, 'solid block rows cannot expose their ANSI background')
  assert.deepEqual(errors, [])
  console.log(
    `PASS: ${report.checks} contrast checks, 16 theme variants, all ANSI/256 colors, RGB gray/black/white, explicit and inverse backgrounds, selection/search, live custom themes; dim/hidden/block styles preserved`
  )
} finally {
  await browser?.close()
  rmSync(directory, { recursive: true, force: true })
}
