// Run with Node and Playwright installed, or set PLAYWRIGHT_MODULE_PATH.
/* global window, document, requestAnimationFrame, FontFace */
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
import ts from 'typescript'

const require = createRequire(import.meta.url)
const { chromium, webkit } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright')
const root = new URL('../../../../', import.meta.url)
const source = new URL('../../src/renderer/', import.meta.url)
const metrics = ts.transpileModule(readFileSync(new URL('app/font-metrics.ts', source), 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
}).outputText
const events = ts.transpileModule(readFileSync(new URL('lib/app-events.ts', source), 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
}).outputText
const stacks = ts.transpileModule(readFileSync(new URL('app/font-stacks.ts', source), 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
}).outputText
const engine = process.env.PLAYWRIGHT_ENGINE === 'webkit' ? webkit : chromium
const browser = await engine.launch({
  channel: process.env.PLAYWRIGHT_ENGINE === 'webkit' ? undefined : 'chrome',
  headless: true
})
try {
  const page = await browser.newPage()
  await page.setContent('<div class="terminal-host" style="width:800px;height:400px"></div>')
  await page.addStyleTag({ path: fileURLToPath(new URL('node_modules/@xterm/xterm/css/xterm.css', root)) })
  await page.addStyleTag({ path: fileURLToPath(new URL('components/terminal-view.css', source)) })
  await page.addScriptTag({ path: fileURLToPath(new URL('node_modules/@xterm/xterm/lib/xterm.js', root)) })
  await page.addScriptTag({ path: fileURLToPath(new URL('node_modules/@xterm/addon-fit/lib/addon-fit.js', root)) })
  // Execute the production observer and event bus, not a copy of their logic.
  await page.addScriptTag({
    content: `window.fontMetrics = (() => {
      const events = (() => { const exports = {}; ${events}; return exports })();
      const stacks = (() => { const exports = {}; ${stacks}; return exports })();
      const exports = {};
      const require = (name) => name === './font-stacks' ? stacks : events;
      ${metrics}
      return { ...exports, ...stacks };
    })();`
  })
  const font = readFileSync(new URL('assets/fonts/text/outfit/Outfit-400.ttf', source)).toString('base64')
  const result = await page.evaluate(async (font) => {
    const family = '"Late, Selected Font"'
    document.documentElement.style.setProperty('--font-mono', family)
    const configuredFamily = window.fontMetrics.getConfiguredMonoFontFamily()
    const terminal = new window.Terminal({ fontFamily: configuredFamily, fontSize: 16, allowProposedApi: true })
    const fit = new window.FitAddon.FitAddon()
    terminal.loadAddon(fit)
    terminal.open(document.querySelector('.terminal-host'))
    fit.fit()
    await new Promise((resolve) => terminal.write('│123。│', resolve))
    let refreshCount = 0
    const dispose = window.fontMetrics.observeCanvasTextMetrics((fontFamily) => {
      refreshCount++
      terminal.options.fontFamily = fontFamily
      terminal.clearTextureAtlas()
      terminal.refresh(0, terminal.rows - 1)
      fit.fit()
    })
    const settle = async () => {
      for (let frame = 0; frame < 8; frame++) await new Promise(requestAnimationFrame)
    }
    await document.fonts.ready
    await settle()
    const before = terminal.cols
    const initialRefreshCount = refreshCount
    // The initial ready promise has already resolved. This face arrives later,
    // as when a user selects/imports a font after opening the terminal.
    const face = new FontFace('Late, Selected Font', `url(data:font/ttf;base64,${font})`)
    document.fonts.add(face)
    window.dispatchEvent(new Event('fileterm:imported-fonts-changed'))
    await document.fonts.load(`16px ${family}`)
    await document.fonts.ready
    await settle()
    const after = terminal.cols
    const loadedRefreshCount = refreshCount
    const restoredFamily = terminal.options.fontFamily
    const referenceHost = document.createElement('div')
    referenceHost.style.cssText = 'width:800px;height:400px'
    document.body.append(referenceHost)
    const reference = new window.Terminal({ fontFamily: configuredFamily, fontSize: 16, allowProposedApi: true })
    const referenceFit = new window.FitAddon.FitAddon()
    reference.loadAddon(referenceFit)
    reference.open(referenceHost)
    referenceFit.fit()
    const expected = reference.cols
    document.documentElement.style.setProperty('--font-mono', 'monospace')
    await settle()
    const switchedColumns = terminal.cols
    reference.options.fontFamily = window.fontMetrics.getConfiguredMonoFontFamily()
    referenceFit.fit()
    const expectedSwitchedColumns = reference.cols
    document.documentElement.style.setProperty('--font-mono', family)
    await settle()
    const restoredColumns = terminal.cols
    // Repeated import events must still refresh, without leaving an altered
    // family or delivering queued callbacks after the terminal is disposed.
    window.dispatchEvent(new Event('fileterm:imported-fonts-changed'))
    await settle()
    const repeatedRefreshCount = refreshCount
    window.dispatchEvent(new Event('fileterm:imported-fonts-changed'))
    await new Promise(requestAnimationFrame)
    dispose()
    document.fonts.dispatchEvent(new Event('loadingdone'))
    await settle()
    const disposedRefreshCount = refreshCount
    terminal.dispose()
    reference.dispose()
    referenceHost.remove()
    return {
      before,
      after,
      expected,
      switchedColumns,
      expectedSwitchedColumns,
      restoredColumns,
      restoredFamily,
      family: configuredFamily,
      initialRefreshCount,
      loadedRefreshCount,
      repeatedRefreshCount,
      disposedRefreshCount
    }
  }, font)
  assert.ok(result.initialRefreshCount > 0, 'initial metrics must be observed')
  assert.notEqual(result.before, result.expected, 'fixture must have different fallback and loaded metrics')
  assert.equal(result.after, result.expected, 'late font must match a fresh terminal grid')
  assert.equal(result.switchedColumns, result.expectedSwitchedColumns, 'font selection must resize an existing grid')
  assert.equal(result.restoredColumns, result.expected, 'switching back must restore the loaded grid')
  assert.equal(result.restoredFamily, result.family, 'quoted family containing a comma must remain intact')
  assert.ok(result.loadedRefreshCount > result.initialRefreshCount, 'later font loading must notify')
  assert.ok(result.repeatedRefreshCount > result.loadedRefreshCount, 'repeated imports must notify')
  assert.equal(result.disposedRefreshCount, result.repeatedRefreshCount, 'dispose must cancel pending notifications')
  console.log(result)
} finally {
  await browser.close()
}
