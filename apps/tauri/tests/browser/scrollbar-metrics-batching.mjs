/* global window, document */
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const { build } = require(process.env.ESBUILD_MODULE_PATH || 'esbuild')
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright')
const directory = mkdtempSync(join(tmpdir(), 'fileterm-scroll-metrics-'))
await build({
  stdin: {
    resolveDir: fileURLToPath(new URL('../../../../', import.meta.url)),
    loader: 'tsx',
    contents: `
import React from 'react'
import { createRoot } from 'react-dom/client'
import { VerticalScrollbar } from './apps/tauri/src/renderer/features/common/vertical-scrollbar'
import './apps/tauri/src/renderer/styles/index.css'
let listener
window.reads = 0
window.scrollTop = 200
const controller = {
  getElement: () => document.getElementById('scroll-viewport'),
  getMetrics: (height) => { window.reads++; window.observedHeight = height; return { clientHeight: height ?? 200, scrollHeight: 1000, scrollTop: window.scrollTop } },
  scrollTo() {}, scrollBy() {}, subscribe: fn => { listener = fn; return () => {} }
}
window.updateScroll = () => { window.reads = 0; listener() }
function NativeFixture() {
  const ref = React.useRef(null)
  return <div style={{position:'relative',height:300}}><div ref={ref} id="native-scroll" style={{height:200,overflow:'auto'}}><div style={{height:1000}}/></div><VerticalScrollbar ariaLabel="native" scrollRef={ref}/></div>
}
createRoot(document.getElementById('root')).render(<><div style={{position:'relative',height:300}}><div id="scroll-viewport" style={{height:200}}><div/></div><VerticalScrollbar scrollController={controller}/></div><NativeFixture/></>)
`
  },
  bundle: true,
  outfile: join(directory, 'bundle.js'),
  jsx: 'automatic',
  define: { 'process.env.NODE_ENV': JSON.stringify('production') },
  loader: { '.svg': 'dataurl', '.ttf': 'dataurl', '.woff2': 'dataurl', '.png': 'dataurl' },
  logLevel: 'silent'
})
let browser
try {
  browser = await chromium.launch({ headless: true, channel: process.env.PLAYWRIGHT_CHANNEL || 'chrome' })
  const page = await browser.newPage()
  await page.setContent(
    '<html data-platform="darwin" data-theme="fileterm-dark"><body><div id="root"></div></body></html>'
  )
  await page.addStyleTag({ content: readFileSync(join(directory, 'bundle.css'), 'utf8') })
  await page.addScriptTag({ content: readFileSync(join(directory, 'bundle.js'), 'utf8') })
  await page.waitForFunction(
    () => document.querySelector('[role="scrollbar"]')?.getAttribute('aria-valuenow') === '200'
  )
  await page.evaluate(() => {
    window.scrollTop = 250
    window.updateScroll()
  })
  await page.waitForFunction(
    () => document.querySelector('[role="scrollbar"]')?.getAttribute('aria-valuenow') === '250'
  )
  assert.equal(await page.evaluate(() => window.reads), 1, 'one update reads scroll geometry only once')
  assert.equal(await page.evaluate(() => window.observedHeight), 200, 'controller receives the observed height')
  await page.locator('#scroll-viewport').evaluate((element) => {
    element.style.height = '300px'
  })
  await page.waitForFunction(() => window.observedHeight === 300)
  await page.waitForFunction(() => document.querySelector('[role=scrollbar]')?.getAttribute('aria-valuemax') === '700')
  const native = page.locator('#native-scroll')
  const nativeBar = page.locator('[aria-label="native"]')
  await nativeBar.waitFor()
  assert.equal(await nativeBar.getAttribute('aria-valuemax'), '800')
  await native.evaluate((element) => {
    element.style.height = '300px'
    element.scrollTop = 250
  })
  await page.waitForFunction(() => {
    const bar = document.querySelector('[aria-label="native"]')
    return bar?.getAttribute('aria-valuemax') === '700' && bar.getAttribute('aria-valuenow') === '250'
  })
  console.log('PASS: scrollbar updates reuse geometry and expose the current scroll position')
} finally {
  await browser?.close()
  rmSync(directory, { recursive: true, force: true })
}
