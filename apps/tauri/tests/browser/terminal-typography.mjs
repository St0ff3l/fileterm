// Run with Node and Playwright installed, or set PLAYWRIGHT_MODULE_PATH to its package directory.
/* global document, window, requestAnimationFrame, NodeFilter, getComputedStyle */
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright')
const root = new URL('../../../../', import.meta.url)
const source = new URL('../../src/renderer/', import.meta.url)
const font = readFileSync(new URL('assets/fonts/text/jetbrains-mono/JetBrainsMono-400.ttf', source)).toString('base64')
const css = readFileSync(new URL('components/terminal-view.css', source), 'utf8')
const browser = await chromium.launch({ channel: 'chrome', headless: true })
let reproduced = false
try {
  for (const scale of [1, 1.25, 2]) {
    const page = await browser.newPage({ deviceScaleFactor: scale })
    await page.setContent(
      `<style>@font-face {font-family: "JetBrains Mono"; src: url(data:font/ttf;base64,${font})}</style>`
    )
    await page.addStyleTag({ path: fileURLToPath(new URL('node_modules/@xterm/xterm/css/xterm.css', root)) })
    await page.addScriptTag({ path: fileURLToPath(new URL('node_modules/@xterm/xterm/lib/xterm.js', root)) })
    await page.addScriptTag({
      path: fileURLToPath(new URL('node_modules/@xterm/addon-unicode11/lib/addon-unicode11.js', root))
    })
    await page.evaluate(() => document.fonts.load('13px "JetBrains Mono"'))
    for (const fixed of [false, true]) {
      const style = await page.addStyleTag({
        content: fixed ? css : '.terminal-host .xterm { text-spacing-trim: normal; }'
      })
      for (const fontFamily of ['"JetBrains Mono", monospace', '"SF Mono", Menlo, monospace']) {
        for (const fontSize of [12, 16, 24]) {
          const result = await page.evaluate(
            async ({ fontFamily, fontSize }) => {
              const host = document.createElement('div')
              host.className = 'terminal-host'
              document.body.append(host)
              const terminal = new window.Terminal({
                cols: 80,
                rows: 12,
                fontFamily,
                fontSize,
                letterSpacing: 0.5,
                lineHeight: 1.05,
                allowProposedApi: true
              })
              terminal.loadAddon(new window.Unicode11Addon.Unicode11Addon())
              terminal.unicode.activeVersion = '11'
              terminal.open(host)
              const lines = ['│123456789│', '│123./;│', '│123。│', '│。。│', '│中文，。！？│', '│。a。b。│', '│▗▖▘▝█│']
              await new Promise((resolve) => terminal.write('\x1b[?25l' + lines.join('\r\n'), resolve))
              await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
              const positions = [...host.querySelectorAll('.xterm-rows > div')].slice(0, lines.length).map((row) => {
                const walker = document.createTreeWalker(row, NodeFilter.SHOW_TEXT)
                const bars = []
                while (walker.nextNode()) {
                  const node = walker.currentNode
                  for (let index = 0; index < node.length; index++) {
                    if (node.textContent[index] !== '│') continue
                    const range = document.createRange()
                    range.setStart(node, index)
                    range.setEnd(node, index + 1)
                    bars.push(range.getBoundingClientRect().left)
                  }
                }
                return bars.at(-1) - bars[0]
              })
              const cellWidth = positions[0] / 10
              const errors = lines.map((line, row) => {
                const bufferLine = terminal.buffer.active.getLine(row)
                let lastBarColumn = 0
                for (let column = 0; column < terminal.cols; column++) {
                  if (bufferLine.getCell(column).getChars() === '│') lastBarColumn = column
                }
                return { line, error: positions[row] - lastBarColumn * cellWidth }
              })
              const trim = getComputedStyle(
                host.querySelector('.xterm-width-cache-measure-container')
              ).getPropertyValue('text-spacing-trim')
              terminal.dispose()
              host.remove()
              return { errors, trim }
            },
            { fontFamily, fontSize }
          )
          const maxError = Math.max(...result.errors.map(({ error }) => Math.abs(error)))
          console.log(JSON.stringify({ fixed, scale, fontFamily, fontSize, maxError, ...result }))
          if (fixed) {
            assert.equal(result.trim, 'space-all', 'hidden measurement must inherit the terminal rule')
            assert.ok(maxError < 1, `terminal cell drift: ${JSON.stringify(result)}`)
          } else if (maxError > 2) reproduced = true
        }
      }
      await style.evaluate((element) => element.remove())
    }
    await page.close()
  }
  assert.ok(reproduced, 'control must reproduce punctuation drift before validating the fix')
} finally {
  await browser.close()
}
