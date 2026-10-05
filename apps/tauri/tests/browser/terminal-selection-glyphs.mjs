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
  for (const scale of [1, 2]) {
    const page = await browser.newPage({ deviceScaleFactor: scale })
    await page.setContent('<div class="terminal-host" id="host"></div>')
    await page.addStyleTag({ path: fileURLToPath(new URL('node_modules/@xterm/xterm/css/xterm.css', root)) })
    await page.addStyleTag({ content: readFileSync(new URL('terminal-view.css', source), 'utf8') })
    const font = readFileSync(new URL('../assets/fonts/text/jetbrains-mono/JetBrainsMono-400.ttf', source)).toString(
      'base64'
    )
    await page.addStyleTag({
      content: `@font-face { font-family: 'JetBrains Mono'; src: url(data:font/ttf;base64,${font}) }`
    })
    await page.evaluate(() => document.fonts.load('24px "JetBrains Mono"'))
    await page.addScriptTag({ path: fileURLToPath(new URL('node_modules/@xterm/xterm/lib/xterm.js', root)) })
    for (const [file, symbol] of [
      ['terminal-selection-renderer.ts', 'registerTerminalSelectionRenderer'],
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
      await page.evaluate(async (fixed) => {
        const host = document.getElementById('host')
        host.replaceChildren()
        const terminal = new window.Terminal({
          cols: 30,
          rows: 3,
          fontFamily: '"JetBrains Mono", Menlo, monospace',
          fontSize: 24,
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
        if (fixed) window.registerTerminalSelectionRenderer(terminal)
        window.registerTerminalBlockGlyphRenderer(terminal)
        if (!fixed)
          terminal.onRender(() =>
            host.querySelectorAll('.xterm-block-glyph').forEach((canvas) => {
              canvas.style.zIndex = '0'
            })
          )
        window.testTerminal = terminal
        await new Promise((resolve) =>
          terminal.write('\x1b[?25l✻ \x1b[1mkernel\x1b[0m\r\n┌ Run scan\r\n▘▝▖▗█', resolve)
        )
        await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
      }, fixed)
      const screen = page.locator('.xterm-screen')
      const metadata = await sharp(await screen.screenshot()).metadata()
      const before = await sharp(await screen.screenshot())
        .removeAlpha()
        .raw()
        .toBuffer()
      await page.evaluate(async () => {
        window.testTerminal.select(0, 0, 90)
        await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
      })
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
        if (y < (metadata.height * 2) / 3 && x > metadata.width / 20) continue
        if (
          before[i] < 10 &&
          before[i + 1] < 10 &&
          before[i + 2] < 10 &&
          (after[i] >= 80 || after[i + 1] >= 80 || after[i + 2] >= 80)
        )
          lost++
      }
      console.log(JSON.stringify({ scale, fixed, lostGlyphPixels: lost }))
      if (fixed) assert.equal(lost, 0, 'selection must preserve all visible glyph pixels')
      else reproduced ||= lost > 0
      if (fixed) {
        const colors = await page.evaluate(async () => {
          const terminal = window.testTerminal
          terminal.options.theme = { ...terminal.options.theme, selectionForeground: '#ff0000' }
          terminal.select(0, 2, 1)
          const paint = () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
          await paint()
          const sample = (column) => {
            const canvas = document.querySelector('.xterm-block-glyph-row[data-row-index="2"]')
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
      await page.evaluate(() => window.testTerminal.dispose())
    }
    await page.close()
  }
  assert.ok(reproduced, 'control must reproduce selected glyph clipping')
} finally {
  await browser.close()
}
