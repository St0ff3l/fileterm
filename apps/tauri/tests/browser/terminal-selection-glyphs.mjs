/* global window, document, requestAnimationFrame */
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const { chromium, webkit } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright')
const sharp = require(process.env.SHARP_MODULE_PATH || 'sharp')
const ts = require('typescript')
const root = new URL('../../../../', import.meta.url)
const source = new URL('../../src/renderer/components/', import.meta.url)
const engine = process.env.PLAYWRIGHT_ENGINE === 'webkit' ? webkit : chromium
const browser = await engine.launch({ headless: true, channel: process.env.PLAYWRIGHT_CHANNEL })
let reproduced = false
try {
  for (const { scale, fontSize } of [1, 2].flatMap((scale) => [14, 24].map((fontSize) => ({ scale, fontSize })))) {
    const page = await browser.newPage({ deviceScaleFactor: scale })
    await page.setContent('<div class="terminal-host" id="host"></div>')
    await page.addStyleTag({ path: fileURLToPath(new URL('node_modules/@xterm/xterm/css/xterm.css', root)) })
    await page.addStyleTag({ content: readFileSync(new URL('terminal-view.css', source), 'utf8') })
    const font = readFileSync(
      process.env.TERMINAL_TEST_FONT_PATH ||
        new URL('../assets/fonts/text/jetbrains-mono/JetBrainsMono-400.ttf', source)
    ).toString('base64')
    await page.addStyleTag({
      content: `@font-face { font-family: 'Selection Test Mono'; src: url(data:font/ttf;base64,${font}) }`
    })
    const loadedFaces = await page.evaluate(
      async () => (await document.fonts.load('24px "Selection Test Mono"')).length
    )
    assert.equal(loadedFaces, 1, 'test must load the requested font rather than silently use fallback')
    await page.addScriptTag({ path: fileURLToPath(new URL('node_modules/@xterm/xterm/lib/xterm.js', root)) })
    for (const [file, symbol] of [
      ['terminal-selection-renderer.ts', 'registerTerminalSelectionRenderer'],
      ['terminal-symbol-glyph-renderer.ts', 'registerTerminalSymbolGlyphRenderer'],
      ['terminal-block-glyph-renderer.ts', 'registerTerminalBlockGlyphRenderer']
    ]) {
      const code = ts
        .transpileModule(readFileSync(new URL(file, source), 'utf8'), {
          compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 }
        })
        .outputText.replace('export function', 'function')
      await page.addScriptTag({ content: `${code}\nwindow.${symbol} = ${symbol}` })
    }
    for (const fixed of [false, true]) {
      await page.evaluate(
        async ({ fixed, fontSize }) => {
          const host = document.getElementById('host')
          host.replaceChildren()
          const terminal = new window.Terminal({
            cols: 30,
            rows: 9,
            fontFamily: '"Selection Test Mono", "SF Mono", Menlo, monospace',
            fontSize,
            letterSpacing: 0,
            lineHeight: 1.05,
            allowProposedApi: true,
            theme: {
              foreground: '#000000',
              background: '#ffffff',
              selectionForeground: '#000000',
              selectionBackground: '#88bbbb',
              selectionInactiveBackground: '#88bbbb'
            }
          })
          terminal.open(host)
          window.testSelectionRenderer = fixed ? window.registerTerminalSelectionRenderer(terminal) : undefined
          if (fixed) window.registerTerminalSymbolGlyphRenderer(terminal)
          window.registerTerminalBlockGlyphRenderer(terminal)
          if (!fixed)
            terminal.onRender(() =>
              host.querySelectorAll('.xterm-block-glyph').forEach((canvas) => {
                canvas.style.zIndex = '0'
              })
            )
          terminal.focus()
          window.testTerminal = terminal
          await new Promise((resolve) =>
            terminal.write(
              '\x1b[?25l✻ \x1b[1mkernel\x1b[0m\r\n┌ Run scan\r\n□ Job\r\n☐ Job\r\n◻ Job\r\n▢ Job\r\n▫ Job\r\n◽ Job\r\n▘▝▖▗█',
              resolve
            )
          )
          await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
        },
        { fixed, fontSize }
      )
      const screen = page.locator('.xterm-screen')
      const metadata = await sharp(await screen.screenshot()).metadata()
      const before = await sharp(await screen.screenshot())
        .removeAlpha()
        .raw()
        .toBuffer()
      const bounds = await screen.boundingBox()
      await page.mouse.move(bounds.x + 1, bounds.y + 1)
      await page.mouse.down()
      await page.mouse.move(bounds.x + bounds.width - 1, bounds.y + bounds.height - 2, { steps: 8 })
      await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))))
      const after = await sharp(await screen.screenshot())
        .removeAlpha()
        .raw()
        .toBuffer()
      let lost = 0
      for (let i = 0; i < before.length; i += 3) {
        const x = (i / 3) % metadata.width
        const y = Math.floor(i / 3 / metadata.width)
        // Check the special glyphs, including their right-hand overhang.
        // Ordinary text can change antialiasing/merging when selected.
        if (y < (metadata.height * 8) / 9 && x > metadata.width / 20) continue
        if (
          before[i] < 10 &&
          before[i + 1] < 10 &&
          before[i + 2] < 10 &&
          (after[i] >= 80 || after[i + 1] >= 80 || after[i + 2] >= 80)
        )
          lost++
      }
      console.log(JSON.stringify({ scale, fontSize, fixed, lostGlyphPixels: lost }))
      if (fixed) assert.equal(lost, 0, 'selection must preserve all visible glyph pixels')
      else reproduced ||= lost > 0
      if (fixed) {
        const opaque = await page.evaluate(() =>
          [...document.querySelectorAll('.xterm-rows > div > .xterm-decoration-top')]
            .filter((span) => !span.classList.contains('xterm-cursor') && span.style.backgroundColor !== 'transparent')
            .map((span) => span.textContent)
        )
        assert.deepEqual(opaque, [], 'mouse dragging must not restore opaque cell backgrounds')
      }
      await page.mouse.up()
      if (fixed) {
        await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))))
        const opaque = await page.evaluate(() =>
          [...document.querySelectorAll('.xterm-rows > div > .xterm-decoration-top')]
            .filter((span) => !span.classList.contains('xterm-cursor') && span.style.backgroundColor !== 'transparent')
            .map((span) => span.textContent)
        )
        assert.deepEqual(opaque, [], 'mouseup must preserve transparent cell backgrounds')
      }
      await page.evaluate(() => window.testTerminal.clearSelection())
      if (fixed) {
        const colors = await page.evaluate(async () => {
          const terminal = window.testTerminal
          terminal.options.theme = { ...terminal.options.theme, selectionForeground: '#ff0000' }
          terminal.select(0, 8, 1)
          const paint = () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
          await paint()
          const sample = (column) => {
            const canvas = document.querySelector('.xterm-block-glyph-row[data-row-index="8"]')
            return [
              ...canvas.getContext('2d').getImageData(Math.floor((canvas.width / 30) * (column + 0.2)), 1, 1, 1).data
            ]
          }
          const selected = sample(0)
          const unselected = sample(4)
          terminal.clearSelection()
          await paint()
          return { selected, unselected, cleared: sample(0) }
        })
        assert.deepEqual(colors.selected, [255, 0, 0, 255], 'selected blocks use the selection foreground')
        assert.deepEqual(colors.unselected, [0, 0, 0, 255], 'partial selection leaves adjacent blocks intact')
        assert.deepEqual(colors.cleared, [0, 0, 0, 255], 'clearing selection restores the block foreground')
      }
      if (fixed) {
        const lifecycle = await page.evaluate(async () => {
          const terminal = window.testTerminal
          terminal.select(0, 3, 5)
          const paint = () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
          await paint()
          let renders = 0
          const subscription = terminal.onRender(() => renders++)
          await paint()
          await paint()
          const settledRenders = renders
          window.testSelectionRenderer.dispose()
          terminal.refresh(0, terminal.rows - 1)
          await paint()
          subscription.dispose()
          return {
            settledRenders,
            background: document.querySelectorAll('.xterm-rows > div')[3].querySelector('.xterm-decoration-top').style
              .backgroundColor
          }
        })
        assert.equal(lifecycle.settledRenders, 0, 'row observation must settle without a refresh loop')
        assert.notEqual(lifecycle.background, 'transparent', 'disposed adapter must stop observing row replacements')
      }
      await page.evaluate(() => window.testTerminal.dispose())
    }
    await page.close()
  }
  assert.ok(reproduced, 'control must reproduce selected glyph clipping')
} finally {
  await browser.close()
}
