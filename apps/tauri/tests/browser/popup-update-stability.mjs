/* global window, document */
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { readFileSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const { build } = require(process.env.ESBUILD_MODULE_PATH || 'esbuild')
const { chromium, webkit } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright')
const directory = mkdtempSync(join(tmpdir(), 'fileterm-popup-stability-'))
await build({
  stdin: {
    resolveDir: fileURLToPath(new URL('../../../../', import.meta.url)),
    loader: 'tsx',
    contents: `
import { Profiler, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { flushSync } from 'react-dom'
import { DropdownSelect } from './apps/tauri/src/renderer/features/common/dropdown-select'
import { SessionSendTargetPicker } from './apps/tauri/src/renderer/features/common/session-send-target-picker'
import './apps/tauri/src/renderer/styles/index.css'
function Fixture() {
  const [revision, setRevision] = useState(0)
  const [left, setLeft] = useState(30)
  const [value, setValue] = useState('one')
  window.updateFixture = () => flushSync(() => setRevision((n) => n + 1))
  window.moveFixture = () => flushSync(() => setLeft((n) => n + 100))
  window.edgeFixture = () => flushSync(() => setLeft(660))
  return <div style={{position:'absolute',left,top:40,width:320}} data-revision={revision}>
    <Profiler id="dropdown" onRender={() => window.commits.dropdown++}>
      <DropdownSelect ariaLabel="fixture dropdown" value={value} onChange={setValue}
        options={[{value:'one',label:'One'},{value:'two',label:'Two'}]} forceCustomMenu/>
    </Profiler>
    <Profiler id="targets" onRender={() => window.commits.targets++}>
      <SessionSendTargetPicker scope="selected-ssh" selectedTabIds={['tab']}
        targets={[{tabId:'tab',title:'Fixture SSH',isCurrent:true}]}
        onScopeChange={() => {}} onSelectedTabIdsChange={() => {}} popover/>
    </Profiler>
  </div>
}
window.commits = {dropdown:0,targets:0}
createRoot(document.getElementById('root')).render(<Fixture/>);
`
  },
  bundle: true,
  outfile: join(directory, 'bundle.js'),
  jsx: 'automatic',
  define: { 'process.env.NODE_ENV': JSON.stringify('development') },
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
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text())
  })
  await page.setContent(
    '<html data-platform="win32" data-theme="fileterm-light"><body><main id="root"></main></body></html>'
  )
  await page.evaluate(() => {
    window.fileterm = { platform: 'win32' }
  })
  await page.addStyleTag({ content: readFileSync(join(directory, 'bundle.css'), 'utf8') })
  await page.addScriptTag({ content: readFileSync(join(directory, 'bundle.js'), 'utf8') })
  for (const [kind, trigger, popup] of [
    ['dropdown', '.dropdown-select-trigger', '.dropdown-select-menu'],
    ['targets', '.custom-select-trigger', '.custom-select-dropdown']
  ]) {
    await page.locator(trigger).waitFor()
    await page.evaluate(async () => {
      // Initial effects and ResizeObserver can update the adaptive arrow.
      // Require startup commits to settle before measuring popup commits.
      await document.fonts.ready
      let previous = -1
      let quietFrames = 0
      for (let frame = 0; frame < 12; frame++) {
        await new Promise(window.requestAnimationFrame)
        const current = window.commits.dropdown + window.commits.targets
        quietFrames = current === previous ? quietFrames + 1 : 0
        if (quietFrames === 3) return
        previous = current
      }
      throw new Error('Popup fixture did not finish its startup updates')
    })
    await page.evaluate((kind) => (window.commits[kind] = 0), kind)
    await page.locator(trigger).click()
    const menu = page.locator(popup)
    await menu.waitFor()
    await page.evaluate(
      () => new Promise((resolve) => window.requestAnimationFrame(() => window.requestAnimationFrame(resolve)))
    )
    assert.equal(
      await page.evaluate((kind) => window.commits[kind], kind),
      1,
      `${kind}: opening/positioning must not schedule another React commit`
    )
    await page.evaluate((kind) => (window.commits[kind] = 0), kind)
    for (let update = 0; update < 50; update++) await page.evaluate(() => window.updateFixture())
    assert.equal(
      await page.evaluate((kind) => window.commits[kind], kind),
      50,
      `${kind}: equal geometry must not add commits`
    )
    const before = await menu.boundingBox()
    await page.evaluate(() => window.moveFixture())
    const after = await menu.boundingBox()
    assert.ok(Math.abs(after.x - before.x - 100) < 1, `${kind}: changed geometry must still update`)
    if (kind === 'dropdown') {
      await page.evaluate(() => window.edgeFixture())
      const edge = await menu.boundingBox()
      assert.ok(edge.x >= 8 && edge.x + edge.width <= 992, 'wide trigger menu must be bounded on its first layout')
      assert.ok(edge.width >= 320, 'trigger width must be applied before menu measurement')
    }
    // Click outside works for both shared controls.
    await page.mouse.click(900, 700)
    await menu.waitFor({ state: 'detached' })
  }
  assert.deepEqual(errors, [])
  console.log('PASS: dropdown and target picker, 100 updates with fresh arrays, bounded commits and live repositioning')
} finally {
  await browser?.close()
  rmSync(directory, { recursive: true, force: true })
}
