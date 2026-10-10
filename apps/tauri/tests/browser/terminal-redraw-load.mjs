/* global window, document, Element, MutationObserver */
import assert from 'node:assert/strict'
import { fileURLToPath } from 'node:url'
import { createRequire } from 'node:module'
import { readFileSync } from 'node:fs'
const root = fileURLToPath(new URL('../../../../', import.meta.url)).replace(/\/$/, '')
const require = createRequire(root + '/package.json')
const ts = require('typescript')
const { chromium, webkit } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright')
const engine = process.env.PLAYWRIGHT_ENGINE === 'webkit' ? webkit : chromium
const browser = await engine.launch({
  headless: true,
  channel: process.env.PLAYWRIGHT_CHANNEL,
  executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH
})
try {
  {
    const page = await browser.newPage({ viewport: { width: 1500, height: 1000 } })
    await page.setContent('<div id="host"></div>')
    await page.addStyleTag({ path: root + '/node_modules/@xterm/xterm/css/xterm.css' })
    for (const [name, path] of [
      ['Mono', 'jetbrains-mono/JetBrainsMono-400.ttf'],
      ['Noto Sans SC', 'noto-sans-sc/NotoSansSC-Variable.ttf']
    ]) {
      const font = readFileSync(root + '/apps/tauri/src/renderer/assets/fonts/text/' + path).toString('base64')
      await page.addStyleTag({ content: `@font-face{font-family:'${name}';src:url(data:font/ttf;base64,${font})}` })
    }
    await page.evaluate(async () => {
      await document.fonts.load('14px Mono')
      await document.fonts.load('14px "Noto Sans SC"')
    })
    await page.addScriptTag({ path: root + '/node_modules/@xterm/xterm/lib/xterm.js' })
    for (const [file, name] of [
      ['components/terminal-symbol-glyph-renderer', 'registerTerminalSymbolGlyphRenderer'],
      ['components/terminal-block-glyph-renderer', 'registerTerminalBlockGlyphRenderer'],
      ['components/terminal-selection-renderer', 'registerTerminalSelectionRenderer'],
      ['app/terminal-foreground-adapter', 'registerTerminalForegroundAdapter']
    ]) {
      let code = ts
        .transpileModule(readFileSync(root + '/apps/tauri/src/renderer/' + file + '.ts', 'utf8'), {
          compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 }
        })
        .outputText.replace(/^export /gm, '')
      await page.addScriptTag({ content: code + `\nwindow.${name}=${name};\n//# sourceURL=${file}.js` })
    }
    const result = await page.evaluate(async () => {
      const terminal = new window.Terminal({
        cols: 140,
        rows: 48,
        fontFamily: 'Mono, "Noto Sans SC", monospace',
        fontSize: 14,
        lineHeight: 1.05,
        allowProposedApi: true,
        minimumContrastRatio: 4.5,
        theme: { foreground: '#dddddd', background: '#111111' }
      })
      terminal.open(document.getElementById('host'))
      const names = {
        selection: 'registerTerminalSelectionRenderer',
        symbols: 'registerTerminalSymbolGlyphRenderer',
        blocks: 'registerTerminalBlockGlyphRenderer',
        foreground: 'registerTerminalForegroundAdapter'
      }
      for (const name of Object.values(names)) window[name](terminal)
      let renders = 0,
        mutations = 0,
        boxes = 0,
        styles = 0
      terminal.onRender(() => renders++)
      const observer = new MutationObserver((records) => (mutations += records.length))
      observer.observe(terminal.element, { childList: true, subtree: true })
      const box = Element.prototype.getBoundingClientRect
      Element.prototype.getBoundingClientRect = function () {
        boxes++
        return box.call(this)
      }
      const style = window.getComputedStyle
      window.getComputedStyle = (...args) => {
        styles++
        return style(...args)
      }
      const gaps = []
      let last = performance.now()
      const heartbeat = setInterval(() => {
        let now = performance.now()
        gaps.push(now - last)
        last = now
      }, 10)
      let parsed = 0,
        writes = 0,
        maxCanvases = 0,
        maxNodes = 0
      const started = performance.now()
      while (performance.now() - started < 60000) {
        let data = (writes % 400 === 0 ? '\x1b[?1049h' : writes % 400 === 200 ? '\x1b[?1049l' : '') + '\x1b[?25l'
        for (let row = 0; row < 46; row++) {
          data += `\x1b[${row + 1};1H`
          data +=
            row % 5 === 0
              ? '\x1b[90m' + '─'.repeat(130)
              : '\x1b[39m▢ ' +
                Array.from({ length: 12 }, (_, i) => `\x1b[${i % 2 ? 37 : 39}mTool ${i}: 中文 output ✻ `)
                  .join('')
                  .slice(0, 180)
          data += '\x1b[0m\x1b[K'
        }
        terminal.write(data, () => parsed++)
        writes++
        await new Promise((r) => setTimeout(r, 30))
        maxCanvases = Math.max(maxCanvases, terminal.element.querySelectorAll('canvas').length)
        maxNodes = Math.max(maxNodes, terminal.element.querySelectorAll('*').length)
      }
      await new Promise((r) => setTimeout(r, 250))
      const before = renders
      await new Promise((r) => setTimeout(r, 250))
      const idleRenders = renders - before
      clearInterval(heartbeat)
      gaps.sort((a, b) => a - b)
      const result = {
        writes,
        parsed,
        renders,
        idleRenders,
        mutations,
        boxes,
        styles,
        maxCanvases,
        maxNodes,
        maxGap: gaps.at(-1),
        p95Gap: gaps[Math.floor(gaps.length * 0.95)]
      }
      terminal.dispose()
      return result
    })
    assert.equal(result.parsed, result.writes, 'all writes must drain')
    assert.ok(result.writes > 500, 'sustained output must keep progressing')
    assert.equal(result.idleRenders, 0, 'adapters must settle after output stops')
    assert.ok(result.boxes < result.renders * 60 + 200, 'layout reads must be batched per row, not per ANSI span')
    assert.ok(result.maxCanvases <= 48, 'row canvases must stay bounded across normal/alternate buffers')
    assert.ok(result.maxNodes < 16000, 'visible DOM must not accumulate with output history')
    console.log(JSON.stringify(result))
    await page.close()
  }
} finally {
  await browser.close()
}
