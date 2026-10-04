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
const directory = mkdtempSync(join(tmpdir(), 'fileterm-terminal-theme-'))
await build({
  stdin: {
    resolveDir: fileURLToPath(new URL('../../../../', import.meta.url)),
    loader: 'ts',
    contents: `
import { Terminal } from '@xterm/xterm'
import { createDefaultThemeConfig, createCodexThemeConfig } from '@fileterm/core'
import { applyThemeVariables, getThemeConfigFromTokenPreset } from './apps/tauri/src/renderer/app/theme-config'
import { registerTerminalForegroundAdapter } from './apps/tauri/src/renderer/app/terminal-foreground-adapter'
import { TerminalLogColorizer, getTerminalLogColorPalette } from './apps/tauri/src/renderer/app/terminal-log-colorizer'
import './apps/tauri/src/renderer/styles/index.css'
import '@xterm/xterm/css/xterm.css'
window.Terminal = Terminal
window.adaptForeground = registerTerminalForegroundAdapter
window.colorizeLogs = (terminal) => new TerminalLogColorizer(terminal, getTerminalLogColorPalette(terminal.options.theme))
window.logPalette = getTerminalLogColorPalette
window.setTheme = (base, variant, customForeground) => {
  const config = base === 'codex' ? createCodexThemeConfig(variant)
    : base === 'fileterm' ? createDefaultThemeConfig(variant)
    : getThemeConfigFromTokenPreset(base, variant)
  if (customForeground) config.theme.terminal.foreground = customForeground
  applyThemeVariables(base === 'codex' ? 'codex-' + variant : 'fileterm-' + variant, config)
  const tokens = getComputedStyle(document.documentElement)
  const read = (name) => tokens.getPropertyValue('--terminal-' + name).trim()
  return { background: read('background'), foreground: read('foreground'),
    white: read('white'), brightWhite: read('bright-white') }
}
`
  },
  bundle: true,
  outfile: join(directory, 'bundle.js'),
  loader: { '.svg': 'dataurl', '.ttf': 'dataurl', '.woff2': 'dataurl', '.png': 'dataurl' },
  logLevel: 'silent'
})

let browser
try {
  const engine = process.env.PLAYWRIGHT_ENGINE === 'webkit' ? webkit : chromium
  browser = await engine.launch({ headless: true, channel: process.env.PLAYWRIGHT_CHANNEL })
  const page = await browser.newPage({ viewport: { width: 1000, height: 760 } })
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await page.setContent('<html data-platform="win32"><body></body></html>')
  await page.evaluate(() => {
    window.fileterm = { platform: 'win32' }
  })
  await page.addStyleTag({ content: readFileSync(join(directory, 'bundle.css'), 'utf8') })
  await page.addScriptTag({ content: readFileSync(join(directory, 'bundle.js'), 'utf8') })

  const result = await page.evaluate(async () => {
    const host = document.createElement('div')
    document.body.append(host)
    const terminal = new window.Terminal({ cols: 80, rows: 14, allowProposedApi: true })
    terminal.open(host)
    const write = (text) => new Promise((resolve) => terminal.write(text, resolve))
    const paint = () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
    const cell = (row, column = 0) => terminal.buffer.active.getLine(row).getCell(column)
    const samples = []
    terminal.options.theme = window.setTheme('codex', 'light')
    const heading = '这次最多使用几个 agent（含主 agent）？'
    await write('\x1b[1;38;2;255;255;255m' + heading + '\x1b[0m')
    await paint()
    const control = {
      rgbWhite: cell(0).isFgRGB() && cell(0).getFgColor() === 0xffffff,
      color: getComputedStyle(host.querySelector('.xterm-rows > div span')).color
    }
    const adapter = window.adaptForeground(terminal)
    const logColorizer = window.colorizeLogs(terminal)
    for (const alternate of [false, true]) {
      if (alternate) await write('\x1b[?1049h')
      terminal.reset()
      if (alternate) await write('\x1b[?1049h')
      // Split RGB SGR across writes, including an incomplete sequence at a
      // write boundary. xterm must continue to own parsing and attributes.
      await write('\x1b[1;38;2;255;')
      await write('255;255m' + heading + '\x1b[0m\r\n')
      await write('\x1b[37m普通白\x1b[0m\r\n\x1b[97m亮白\x1b[0m\r\n')
      await write('\x1b[38:2::255:255:255m冒号 RGB\x1b[0m\r\n')
      await write('\x1b[38;2;255;255;255;48;2;20;40;80m有背景\x1b[0m\r\n')
      await write('\x1b[7;38;2;255;255;255m反色\x1b[0m\r\n')
      await write('\x1b[38;2;153;153;153m灰色\x1b[0m\r\n')
      await write('\x1b[38;2;255;0;0m红色\x1b[0m\r\n')
      await paint()
      samples.push({
        alternate,
        buffer: terminal.buffer.active.type,
        title: terminal.buffer.active.getLine(0).translateToString(true),
        themeText: cell(0).isFgDefault(),
        bold: Boolean(cell(0).isBold()),
        colonThemeText: cell(3).isFgDefault(),
        pairedWhite: cell(4).isFgRGB() && cell(4).getFgColor() === 0xffffff,
        inverseWhite: cell(5).isFgRGB() && Boolean(cell(5).isInverse()),
        gray: cell(6).getFgColor(),
        red: cell(7).getFgColor()
      })
      // Change themes with existing text and no new output. These are the
      // actual root tokens used by TerminalView's xterm theme builder.
      for (const base of ['codex', 'fileterm', 'iterm2-nord']) {
        for (const variant of ['dark', 'light']) {
          terminal.options.theme = window.setTheme(base, variant)
          logColorizer.setPalette(window.logPalette(terminal.options.theme))
          terminal.refresh(0, terminal.rows - 1)
          await paint()
          const row = host.querySelector('.xterm-rows > div')
          const titleColor = getComputedStyle(row.querySelector('span') ?? row).color
          const probe = document.createElement('span')
          probe.style.color = terminal.options.theme.foreground
          host.append(probe)
          const foreground = getComputedStyle(probe).color
          probe.remove()
          samples.push({ base, variant, alternate, titleColor, foreground, theme: terminal.options.theme })
        }
      }
      terminal.options.theme = window.setTheme('codex', 'light', '#663399')
      logColorizer.setPalette(window.logPalette(terminal.options.theme))
      terminal.refresh(0, terminal.rows - 1)
      await paint()
      const customRow = host.querySelector('.xterm-rows > div')
      samples.push({
        base: 'custom',
        variant: 'custom',
        alternate,
        titleColor: getComputedStyle(customRow.querySelector('span') ?? customRow).color,
        foreground: 'rgb(102, 51, 153)',
        theme: terminal.options.theme
      })
      if (alternate) await write('\x1b[?1049l')
    }
    terminal.reset()
    await write('\x1b[38;2;255;255;255mhistory\x1b[0m\r\n'.repeat(50))
    terminal.scrollToTop()
    await paint()
    const history = cell(0).isFgDefault()
    adapter.dispose()
    terminal.reset()
    await write('\x1b[38;2;255;255;255mwhite after disposal')
    const disposed = cell(0).isFgRGB()
    logColorizer.dispose()
    terminal.dispose()
    return { control, samples, history, disposed }
  })

  assert.equal(result.control.rgbWhite, true, 'control must reproduce literal RGB white')
  assert.equal(result.control.color, 'rgb(255, 255, 255)', 'control must reproduce white text on Codex white canvas')
  for (const sample of result.samples) {
    if ('themeText' in sample) {
      assert.equal(sample.buffer, sample.alternate ? 'alternate' : 'normal')
      assert.equal(sample.title, '这次最多使用几个 agent（含主 agent）？')
      for (const key of ['themeText', 'bold', 'colonThemeText', 'pairedWhite', 'inverseWhite']) {
        assert.equal(sample[key], true, `${key}: ${JSON.stringify(sample)}`)
      }
      assert.equal(sample.gray, 0x999999)
      assert.equal(sample.red, 0xff0000)
    } else {
      assert.equal(
        sample.titleColor,
        sample.foreground,
        `existing heading must follow ${sample.base}/${sample.variant}`
      )
      if (sample.variant === 'light' && sample.base !== 'iterm2-nord') {
        assert.equal(sample.theme.white, '#374151')
        assert.equal(sample.theme.brightWhite, '#111827')
      }
    }
  }
  assert.equal(result.history, true, 'offscreen white history must adapt when scrolled into view')
  assert.equal(result.disposed, true, 'disposing removes all adapter listeners')
  assert.deepEqual(errors, [])
  console.log(
    'PASS: RGB/ANSI white, split/colon SGR, bold+CJK, normal/alternate screens, live themes, color pairings, history, disposal'
  )
} finally {
  await browser?.close()
  rmSync(directory, { recursive: true, force: true })
}
