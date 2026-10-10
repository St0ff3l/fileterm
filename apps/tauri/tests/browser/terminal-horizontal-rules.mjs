/* global window, document, requestAnimationFrame */
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { readFileSync, mkdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const { chromium, webkit } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright')
const sharp = require(process.env.SHARP_MODULE_PATH || 'sharp')
const ts = require('typescript')
const root = new URL('../../../../', import.meta.url)
const source = new URL('../../src/renderer/', import.meta.url)
const engine = process.env.PLAYWRIGHT_ENGINE === 'webkit' ? webkit : chromium
const browser = await engine.launch({
  headless: true,
  channel: process.env.PLAYWRIGHT_CHANNEL,
  executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH
})
try {
  for (const scale of [1, 1.25, 1.5, 2]) {
    for (const fontSize of [14, 24]) {
      const page = await browser.newPage({ deviceScaleFactor: scale })
      await page.setContent('<div class="terminal-host" id="host"></div>')
      await page.addStyleTag({ path: fileURLToPath(new URL('node_modules/@xterm/xterm/css/xterm.css', root)) })
      for (const [family, file] of [
        ['Test Mono', 'jetbrains-mono/JetBrainsMono-400.ttf'],
        ['Noto Sans SC', 'noto-sans-sc/NotoSansSC-Variable.ttf']
      ]) {
        const font = readFileSync(new URL(`assets/fonts/text/${file}`, source)).toString('base64')
        await page.addStyleTag({ content: `@font-face{font-family:'${family}';src:url(data:font/ttf;base64,${font})}` })
      }
      await page.evaluate(async () => {
        await document.fonts.load('24px "Test Mono"')
        await document.fonts.load('24px "Noto Sans SC"')
      })
      await page.addScriptTag({ path: fileURLToPath(new URL('node_modules/@xterm/xterm/lib/xterm.js', root)) })
      const code = ts
        .transpileModule(readFileSync(new URL('components/terminal-symbol-glyph-renderer.ts', source), 'utf8'), {
          compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 }
        })
        .outputText.replace('export function', 'function')
      await page.addScriptTag({ content: `${code}\nwindow.registerSymbols = registerTerminalSymbolGlyphRenderer` })
      const blockCode = ts
        .transpileModule(readFileSync(new URL('components/terminal-block-glyph-renderer.ts', source), 'utf8'), {
          compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 }
        })
        .outputText.replace('export function', 'function')
      await page.addScriptTag({ content: `${blockCode}\nwindow.registerBlocks = registerTerminalBlockGlyphRenderer` })
      const selectionCode = ts
        .transpileModule(readFileSync(new URL('components/terminal-selection-renderer.ts', source), 'utf8'), {
          compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 }
        })
        .outputText.replace('export function', 'function')
      await page.addScriptTag({
        content: `${selectionCode}\nwindow.registerSelection = registerTerminalSelectionRenderer`
      })
      for (const glyph of ['─', '━', '═']) {
        const results = []
        for (const fixed of [false, true]) {
          const geometry = await page.evaluate(
            async ({ fixed, fontSize, glyph }) => {
              const terminal = new window.Terminal({
                cols: 32,
                rows: 3,
                fontFamily: '"Test Mono", "Noto Sans SC", monospace',
                fontSize,
                letterSpacing: 0,
                lineHeight: 1.05,
                allowProposedApi: true,
                theme: { foreground: '#000000', background: '#ffffff' }
              })
              terminal.open(document.getElementById('host'))
              if (fixed) {
                window.registerSelection(terminal)
                window.registerSymbols(terminal)
                window.blocks = window.registerBlocks(terminal)
              }
              window.terminal = terminal
              // Alternating ANSI spans also expose joins between independent DOM runs.
              const rule = Array.from(
                { length: 24 },
                (_, index) => `${index % 2 ? '\x1b[38;2;0;0;0m' : '\x1b[39m'}${glyph}`
              ).join('')
              await new Promise((resolve) =>
                terminal.write(
                  `\x1b[?25l${rule}\x1b[0m\r\n${glyph.repeat(24)}\r\n▢中e\u0301${glyph.repeat(20)}`,
                  resolve
                )
              )
              await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
              const screen = terminal.element.querySelector('.xterm-screen')
              return {
                width: screen.getBoundingClientRect().width,
                height: screen.getBoundingClientRect().height,
                fitted: terminal.element.querySelectorAll('.xterm-fitted-symbol-source').length,
                text: [0, 1, 2].map((row) => terminal.buffer.active.getLine(row).translateToString(true))
              }
            },
            { fixed, fontSize, glyph }
          )
          const png = await page.locator('.xterm-screen').screenshot()
          const { data, info } = await sharp(png).removeAlpha().raw().toBuffer({ resolveWithObject: true })
          const gaps = [0, 0, 0]
          const uneven = [0, 0, 0]
          for (let row = 0; row < 3; row++) {
            let previousProfile
            for (
              let x = Math.ceil((info.width * (row === 2 ? 5 : 2)) / 32);
              x < Math.floor((info.width * 23) / 32);
              x++
            ) {
              let ink = false
              const profile = []
              for (let y = Math.ceil((info.height * row) / 3); y < Math.floor((info.height * (row + 1)) / 3); y++) {
                const offset = (y * info.width + x) * 3
                profile.push(data[offset], data[offset + 1], data[offset + 2])
                if (data[offset] < 128 && data[offset + 1] < 128 && data[offset + 2] < 128) ink = true
              }
              if (!ink) gaps[row]++
              if (previousProfile && profile.some((value, index) => value !== previousProfile[index])) uneven[row]++
              previousProfile = profile
            }
          }
          results.push({ fixed, gaps, uneven, ...geometry })
          if (process.env.SCREENSHOT_DIR) {
            mkdirSync(process.env.SCREENSHOT_DIR, { recursive: true })
            await sharp(png).toFile(
              `${process.env.SCREENSHOT_DIR}/${glyph.codePointAt(0)}-${scale}-${fontSize}-${fixed}.png`
            )
          }
          if (fixed) {
            const lifecycle = await page.evaluate(async () => {
              const terminal = window.terminal
              const originalCanvas = document.querySelector('.xterm-block-glyph-row[data-row-index="0"]')
              const paint = () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
              const hasColor = (column, color) => {
                const canvas = document.querySelector('.xterm-block-glyph-row[data-row-index="0"]')
                const pixels = canvas
                  .getContext('2d')
                  .getImageData(Math.floor(((column + 0.5) * canvas.width) / 32), 0, 1, canvas.height).data
                for (let index = 0; index < pixels.length; index += 4) {
                  if (color.every((value, offset) => pixels[index + offset] === value)) return true
                }
                return false
              }
              terminal.options.theme = { ...terminal.options.theme, selectionForeground: '#ff0000' }
              terminal.select(2, 0, 4)
              await paint()
              const selected = hasColor(3, [255, 0, 0, 255])
              const unselected = hasColor(8, [0, 0, 0, 255])
              terminal.clearSelection()
              await paint()
              const cleared = hasColor(3, [0, 0, 0, 255])
              terminal.options.theme = { ...terminal.options.theme, foreground: '#663399' }
              terminal.refresh(0, terminal.rows - 1)
              await paint()
              const themed = hasColor(2, [102, 51, 153, 255])
              const explicit = hasColor(3, [0, 0, 0, 255])
              const reused = originalCanvas === document.querySelector('.xterm-block-glyph-row[data-row-index="0"]')
              await new Promise((resolve) => terminal.write('\x1b[1;7H\x1b[K', resolve))
              await paint()
              const tail = originalCanvas
                .getContext('2d')
                .getImageData(Math.floor((8.5 * originalCanvas.width) / 32), 0, 1, originalCanvas.height).data
              const clearedTail = [...tail].every((value, index) => index % 4 !== 3 || value === 0)
              window.blocks.dispose()
              return {
                selected,
                unselected,
                cleared,
                themed,
                explicit,
                reused,
                clearedTail,
                remainingCanvases: terminal.element.querySelectorAll('.xterm-block-glyph-row').length,
                text: terminal.buffer.active.getLine(1).translateToString(true)
              }
            })
            assert.deepEqual(lifecycle, {
              selected: true,
              unselected: true,
              cleared: true,
              themed: true,
              explicit: true,
              reused: true,
              clearedTail: true,
              remainingCanvases: 0,
              text: glyph.repeat(24)
            })
          }
          await page.evaluate(() => {
            window.terminal.dispose()
            document.getElementById('host').replaceChildren()
          })
        }
        console.log(JSON.stringify({ scale, fontSize, glyph, results }))
        assert.deepEqual(
          results[1].gaps,
          results[0].gaps,
          'symbol fitting must not introduce gaps between horizontal rule cells'
        )
        assert.deepEqual(
          results[1].uneven,
          [0, 0, 0],
          'horizontal rule thickness and coverage must be uniform across joins'
        )
        assert.deepEqual(
          results[1].text,
          [glyph.repeat(24), glyph.repeat(24), `▢中e\u0301${glyph.repeat(20)}`],
          'rule text remains unchanged'
        )
      }
      await page.close()
    }
  }
} finally {
  await browser.close()
}
