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
const source = new URL('../../src/renderer/', import.meta.url)
const engine = process.env.PLAYWRIGHT_ENGINE === 'webkit' ? webkit : chromium
const browser = await engine.launch({
  channel: process.env.PLAYWRIGHT_CHANNEL,
  executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH
})
let reproduced = false
const glyphs = ['▢', '▣', '☐', '□', '✻', '┌', '→', '※']
try {
  for (const scale of [1, 1.25, 2]) {
    for (const fontSize of [14, 24]) {
      const page = await browser.newPage({ deviceScaleFactor: scale })
      await page.setContent('<div class="terminal-host" id="host"></div>')
      await page.addStyleTag({ path: fileURLToPath(new URL('node_modules/@xterm/xterm/css/xterm.css', root)) })
      await page.addStyleTag({ path: fileURLToPath(new URL('components/terminal-view.css', source)) })
      for (const [family, path, weight] of [
        [
          'Test Mono',
          process.env.TERMINAL_TEST_FONT_PATH ||
            new URL('assets/fonts/text/jetbrains-mono/JetBrainsMono-400.ttf', source),
          '400'
        ],
        [
          'Test Mono',
          process.env.TERMINAL_TEST_FONT_PATH ||
            new URL('assets/fonts/text/jetbrains-mono/JetBrainsMono-700.ttf', source),
          '700'
        ],
        ['Noto Sans SC', new URL('assets/fonts/text/noto-sans-sc/NotoSansSC-Variable.ttf', source), '100 900']
      ]) {
        const data = readFileSync(path).toString('base64')
        await page.addStyleTag({
          content: `@font-face{font-family:'${family}';font-weight:${weight};src:url(data:font/ttf;base64,${data})}`
        })
      }
      await page.evaluate(async () => {
        for (const font of ['24px "Test Mono"', '700 24px "Test Mono"', '24px "Noto Sans SC"']) {
          if (!(await document.fonts.load(font)).length) throw new Error(`Font not loaded: ${font}`)
        }
      })
      for (const path of ['@xterm/xterm/lib/xterm.js', '@xterm/addon-unicode11/lib/addon-unicode11.js']) {
        await page.addScriptTag({ path: fileURLToPath(new URL(`node_modules/${path}`, root)) })
      }
      for (const [file, symbol] of [
        ['terminal-symbol-glyph-renderer.ts', 'registerTerminalSymbolGlyphRenderer'],
        ['terminal-selection-renderer.ts', 'registerTerminalSelectionRenderer']
      ]) {
        const code = ts
          .transpileModule(readFileSync(new URL(`components/${file}`, source), 'utf8'), {
            compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 }
          })
          .outputText.replace('export function', 'function')
        await page.addScriptTag({ content: `${code}\nwindow.${symbol} = ${symbol}` })
      }
      for (const fixed of [false, true]) {
        await page.evaluate(
          ({ fixed, fontSize, glyphs }) => {
            document.getElementById('host').replaceChildren()
            const terminal = new window.Terminal({
              cols: 32,
              rows: glyphs.length,
              fontFamily: '"Test Mono", "Noto Sans SC", monospace',
              fontSize,
              lineHeight: 1.05,
              letterSpacing: 0,
              allowProposedApi: true,
              minimumContrastRatio: 4.5,
              theme: {
                foreground: '#000000',
                background: '#ffffff',
                selectionForeground: '#000000',
                selectionBackground: '#b2d9ff'
              }
            })
            terminal.loadAddon(new window.Unicode11Addon.Unicode11Addon())
            terminal.unicode.activeVersion = '11'
            terminal.open(document.getElementById('host'))
            window.testTerminal = terminal
            window.symbolRenderer = fixed ? window.registerTerminalSymbolGlyphRenderer(terminal) : undefined
            window.registerTerminalSelectionRenderer(terminal)
            terminal.focus()
            window.paint = () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
            window.writeSample = async (mode) => {
              const prefix =
                mode === 'ansi'
                  ? '\x1b[48;2;178;217;255m'
                  : mode === 'inverse'
                    ? '\x1b[38;2;178;217;255;48;2;0;0;0;7m'
                    : ''
              const sample =
                '\x1b[?25l\x1b[H' +
                glyphs
                  .map((glyph, index) => `    ${prefix}${index % 2 ? '\x1b[1m' : ''}${glyph}\x1b[22m \x1b[1mJob\x1b[0m`)
                  .join('\r\n')
              await new Promise((resolve) => terminal.write(sample, resolve))
              await window.paint()
            }
          },
          { fixed, fontSize, glyphs }
        )
        await page.evaluate(() => window.writeSample('plain'))
        const screen = page.locator('.xterm-screen')
        const baseline = await sharp(await screen.screenshot())
          .removeAlpha()
          .raw()
          .toBuffer({ resolveWithObject: true })
        const cellWidth = baseline.info.width / 32
        for (const mode of ['ansi', 'inverse', 'mouse']) {
          await page.evaluate((mode) => window.writeSample(mode), mode)
          if (mode === 'mouse') {
            const bounds = await screen.boundingBox()
            await page.mouse.move(bounds.x + 1, bounds.y + 1)
            await page.mouse.down()
            await page.mouse.move(bounds.x + bounds.width - 1, bounds.y + bounds.height - 1, { steps: 8 })
            await page.evaluate(() => window.paint())
          }
          const after = await sharp(await screen.screenshot())
            .removeAlpha()
            .raw()
            .toBuffer()
          let lost = 0
          const missing = []
          for (let i = 0; i < baseline.data.length; i += 3) {
            const x = (i / 3) % baseline.info.width
            if (x < cellWidth * 4 || x >= cellWidth * 6) continue
            if (
              baseline.data[i] < 32 &&
              baseline.data[i + 1] < 32 &&
              baseline.data[i + 2] < 32 &&
              // Background changes alter WebKit glyph antialiasing. Count ink
              // actually replaced by the blue background, not darker blue
              // pixels that still contain the stroke.
              after[i] > 178 * 0.95 &&
              after[i + 1] > 217 * 0.95 &&
              after[i + 2] > 255 * 0.95
            ) {
              // WebKit may shift an antialiased corner by one CSS pixel
              // when selection turns the run into a positioned decoration.
              // Accept neighboring ink; a clipped edge has no such stroke.
              const y = Math.floor(i / 3 / baseline.info.width)
              let nearbyInk = false
              const radius = Math.ceil(scale)
              for (let dy = -radius; dy <= radius; dy++) {
                for (let dx = -radius; dx <= radius; dx++) {
                  if (x + dx < 0 || x + dx >= baseline.info.width || y + dy < 0 || y + dy >= baseline.info.height)
                    continue
                  const pixel = ((y + dy) * baseline.info.width + x + dx) * 3
                  if (after[pixel] < 178 * 0.6 && after[pixel + 1] < 217 * 0.6 && after[pixel + 2] < 255 * 0.6)
                    nearbyInk = true
                }
              }
              if (nearbyInk) continue
              lost++
              missing.push({
                x,
                y: Math.floor(i / 3 / baseline.info.width),
                before: [...baseline.data.slice(i, i + 3)],
                after: [...after.slice(i, i + 3)]
              })
            }
          }
          console.log(JSON.stringify({ scale, fontSize, fixed, mode, lostGlyphPixels: lost }))
          if (fixed && lost) console.log(JSON.stringify({ missing }))
          if (fixed) assert.equal(lost, 0, `${mode} background must preserve the complete symbol`)
          else if (mode === 'ansi') reproduced ||= lost > 0
          if (mode === 'mouse') await page.mouse.up()
          await page.evaluate(async () => {
            window.testTerminal.clearSelection()
            await window.paint()
          })
        }
        if (fixed) {
          const mixed = await page.evaluate(async () => {
            const terminal = window.testTerminal
            const text = '中文e\u0301 ▢█Job'
            await new Promise((resolve) => terminal.write('\x1b[H\x1b[2K中文e\u0301 ▢█\x1b[1mJob\x1b[0m', resolve))
            terminal.options.fontSize = 18
            await window.paint()
            terminal.select(0, 0, 11)
            await window.paint()
            const copied = terminal.getSelection()
            terminal.clearSelection()
            await window.paint()
            const line = terminal.buffer.active.getLine(0)
            const row = terminal.element.querySelector('.xterm-rows > div')
            const label = [...row.children].find((span) => span.textContent === 'Job')
            const cellWidth =
              terminal.element.querySelector('.xterm-screen').getBoundingClientRect().width / terminal.cols
            const labelError = label
              ? Math.abs(label.getBoundingClientRect().left - row.getBoundingClientRect().left - 8 * cellWidth)
              : Infinity
            terminal.clearSelection()
            return {
              text,
              copied,
              cjkWidth: line.getCell(0).getWidth(),
              combined: line.getCell(4).getChars(),
              labelError
            }
          })
          assert.equal(mixed.copied, mixed.text, 'CJK, combining marks and graphical blocks retain exact copy text')
          assert.equal(mixed.cjkWidth, 2, 'fitting must preserve wide buffer cells')
          assert.equal(mixed.combined, 'e\u0301', 'fitting must preserve combined buffer cells')
          assert.ok(mixed.labelError < 1, 'font resize must preserve following text columns')
          await page.evaluate((fontSize) => {
            window.testTerminal.options.fontSize = fontSize
          }, fontSize)
          const result = await page.evaluate(async () => {
            await window.writeSample('ansi')
            const terminal = window.testTerminal
            const before = terminal.buffer.active.getLine(0).translateToString(true)
            const fitted = document.querySelectorAll('.xterm-fitted-symbol-source').length
            const boldMismatch = [...document.querySelectorAll('.xterm-fitted-symbol-source.xterm-bold')].some(
              (source) =>
                [...source.querySelectorAll('span')].some(
                  (child) => window.getComputedStyle(child).fontWeight !== window.getComputedStyle(source).fontWeight
                )
            )
            let mutations = 0
            const observer = new window.MutationObserver(() => mutations++)
            observer.observe(terminal.element, { childList: true, subtree: true })
            await window.paint()
            await window.paint()
            observer.disconnect()
            const settledMutations = mutations
            window.symbolRenderer.dispose()
            const remaining = document.querySelectorAll('.xterm-fitted-symbol-source').length
            terminal.refresh(0, terminal.rows - 1)
            await window.paint()
            return {
              before,
              after: terminal.buffer.active.getLine(0).translateToString(true),
              fitted,
              remaining,
              settledMutations,
              boldMismatch
            }
          })
          assert.equal(result.before, result.after, 'fitting must not alter terminal text or buffer widths')
          assert.ok(result.fitted > 0, 'real fallback symbols must exercise horizontal fitting')
          assert.equal(result.remaining, 0, 'dispose restores the original DOM text')
          assert.equal(result.boldMismatch, false, 'fitted bold symbols must retain their original font weight')
          assert.equal(result.settledMutations, 0, 'glyph fitting must settle without a mutation loop')
        }
        await page.evaluate(() => window.testTerminal.dispose())
      }
      await page.close()
    }
  }
  assert.ok(reproduced, 'control must reproduce ANSI-background clipping with the real CJK fallback')
} finally {
  await browser.close()
}
