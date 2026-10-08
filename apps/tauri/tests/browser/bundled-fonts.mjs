/* global document, window */
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { createServer } from 'node:http'
import { createRequire } from 'node:module'
import { resolve, extname } from 'node:path'
import { fileURLToPath } from 'node:url'
import ts from 'typescript'

const require = createRequire(import.meta.url)
const { chromium, webkit } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright')
const dist = fileURLToPath(new URL('../../dist/', import.meta.url))
const server = createServer((request, response) => {
  if (request.url === '/') {
    response.setHeader('Content-Type', 'text/html')
    response.end('<!doctype html><html><body>Font audit</body></html>')
    return
  }
  const path = resolve(dist, '.' + request.url)
  if (!path.startsWith(dist)) {
    response.writeHead(404).end()
    return
  }
  try {
    response.setHeader('Content-Type', extname(path) === '.css' ? 'text/css' : 'font/ttf')
    response.end(readFileSync(path))
  } catch {
    response.writeHead(404).end()
  }
})
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve))
const base = `http://127.0.0.1:${server.address().port}`
const engine = process.env.PLAYWRIGHT_ENGINE === 'webkit' ? webkit : chromium
const browser = await engine.launch({ channel: process.env.PLAYWRIGHT_ENGINE === 'webkit' ? undefined : 'chrome' })
try {
  const page = await browser.newPage()
  const external = []
  page.on('request', (request) => {
    if (!request.url().startsWith(base)) external.push(request.url())
  })
  await page.goto(base)
  for (const file of readdirSync(resolve(dist, 'assets')).filter((name) => name.endsWith('.css'))) {
    await page.addStyleTag({ url: `${base}/assets/${file}` })
  }
  const stacks = ts.transpileModule(
    readFileSync(new URL('../../src/renderer/app/font-stacks.ts', import.meta.url), 'utf8'),
    {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }
  ).outputText
  await page.addScriptTag({
    content: `window.fontStacks = (() => { const exports = {}; ${stacks}; return exports })()`
  })
  const result = await page.evaluate(async () => {
    const faces = [...document.fonts]
    await Promise.all(faces.map((face) => face.load()))
    const canvas = document.createElement('canvas').getContext('2d')
    const width = (family) => {
      canvas.font = `20px ${family}`
      return canvas.measureText('Hello WWW 0123').width
    }
    const absent = 'FileTerm Missing Test Font 932731'
    const fallbackWidth = width(window.fontStacks.configuredFontStack(absent, 'code'))
    const bundledWidth = width('"JetBrains Mono"')
    const macFonts = window.fontStacks.systemFontAvailability('darwin')
    const otherFonts = ['win32', 'linux', 'browser', undefined].map(window.fontStacks.systemFontAvailability)
    const monoStack = window.fontStacks.configuredFontStack('SF Mono', 'code', 'darwin')
    const uiStack = window.fontStacks.configuredFontStack('SF Pro Text', 'ui', 'darwin')
    const systemMonoWidth = width(monoStack)
    const genericMonoWidth = width('ui-monospace')
    const systemUiWidth = width(uiStack)
    const genericUiWidth = width('system-ui')
    const quoted = window.fontStacks.configuredFontStack('Font, With "Quotes"', 'code')
    canvas.font = `20px ${quoted}`
    const validQuoted = canvas.font.includes('Font, With')
    // FontFaceSet.check alone falsely reports a missing family as available.
    const misleadingCheck = document.fonts.check(`16px "${absent}"`)
    return {
      count: faces.length,
      families: [...new Set(faces.map((face) => face.family))],
      fallbackWidth,
      bundledWidth,
      macFonts,
      otherFonts,
      monoStack,
      uiStack,
      systemMonoWidth,
      genericMonoWidth,
      systemUiWidth,
      genericUiWidth,
      misleadingCheck,
      validQuoted
    }
  })
  assert.ok(
    result.families.some((name) => name.includes('Cascadia Code')),
    'Cascadia must be in production CSS'
  )
  assert.ok(result.count >= 65, 'All bundled font faces, including Codicons, must decode')
  assert.equal(result.fallbackWidth, result.bundledWidth, 'Missing selection must use bundled mono metrics')
  assert.deepEqual(
    result.macFonts,
    { 'SF Mono': true, 'SF Pro Text': true },
    'macOS system fonts must remain selectable'
  )
  assert.ok(
    result.otherFonts.every((fonts) => !fonts['SF Mono'] && !fonts['SF Pro Text']),
    'Apple presets must be disabled on other platforms'
  )
  assert.ok(result.monoStack.startsWith('ui-monospace,'), 'SF Mono must use the system generic font entry')
  assert.ok(result.uiStack.startsWith('system-ui,'), 'SF Pro must use the system generic font entry')
  // FileTerm on macOS uses WebKit. Chrome does not resolve ui-monospace in a
  // family list consistently, and the browser build disables Apple presets.
  if (process.env.PLAYWRIGHT_ENGINE === 'webkit') {
    assert.equal(result.systemMonoWidth, result.genericMonoWidth, 'SF Mono selection must use system mono metrics')
  }
  assert.equal(result.systemUiWidth, result.genericUiWidth, 'SF Pro selection must use system UI metrics')
  assert.equal(result.validQuoted, true, 'Imported names must be safely quoted')
  assert.deepEqual(external, [], 'All fonts must load offline from the production assets')
  console.log(result)
} finally {
  await browser.close()
  await new Promise((resolve) => server.close(resolve))
}
