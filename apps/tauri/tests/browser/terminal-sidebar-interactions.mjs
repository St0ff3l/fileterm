// Real TerminalView/xterm and sidebar interactions; only the desktop IPC is stubbed.
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
const directory = mkdtempSync(join(tmpdir(), 'fileterm-terminal-sidebar-'))
const root = fileURLToPath(new URL('../../../../', import.meta.url))
await build({
  stdin: {
    resolveDir: root,
    loader: 'tsx',
    contents: `
import { useState } from 'react'
import { createRoot } from 'react-dom/client'
import { flushSync } from 'react-dom'
import { TerminalView } from './apps/tauri/src/renderer/components/terminal-view'
import { SystemSidebarShell } from './apps/tauri/src/renderer/features/system/system-sidebar-shell'
import { t } from './apps/tauri/src/renderer/i18n'
import './apps/tauri/src/renderer/styles/index.css'
window.labels = { paste: t.paste }
function Fixture() {
  const [connected, setConnected] = useState(false)
  const [collapsed, setCollapsed] = useState(false)
  const [width, setWidth] = useState(214)
  const [revision, setRevision] = useState(0)
  const [capability, setCapability] = useState(true)
  window.updateFixture = (next) => flushSync(() => {
    setCapability(next)
    setRevision((value) => value + 1)
  })
  window.connectFixture = () => flushSync(() => setConnected(true))
  return <div style={{display:'flex',height:700,width:1000}}>
    <div style={{width:collapsed ? 44 : width, flexShrink:0}}>
      <SystemSidebarShell activeProfile={{type:'ssh',host:'fixture-host'}}
        activeTabId="tab" connectionStatus={connected ? 'connected' : 'connecting'} collapsed={collapsed}
        activeSession={{profileId:'profile',connected,
          capabilities:{resourceMonitoring:capability},
          monitoring:{phase:capability ? 'healthy' : 'unsupported',reason:capability ? undefined : 'target-identity-invalid',generation:1,revision},
          systemMetrics:capability ? {cpuPercent:5,memoryPercent:10,
            networkInterfaces:['eth0'],activeNetworkInterface:'eth0',networkSamples:[],
            fileSystemRows:[]} : undefined}}
        showResourceMeters={capability} visibleMetrics={['cpu','memory','disk','network']}
        isResizing={false} onMonitoringSnapshot={() => {}} onOpenSystemInfo={() => {}}
        onResizeStart={() => {}} onRestoreWidth={() => setWidth(214)} onToggleCollapsed={setCollapsed}/>
    </div>
    <div style={{flex:1,minWidth:0}}>
      <TerminalView profileId="profile" tabId="tab" sessionType="ssh" connected={connected}
        connecting={!connected} bootText={connected ? 'fixture shell\\r\\n$ ' : ''}
        onActivate={() => {}} onStatus={() => {}}/>
    </div>
  </div>
}
createRoot(document.getElementById('root')).render(<Fixture/>);
`
  },
  bundle: true,
  outfile: join(directory, 'bundle.js'),
  jsx: 'automatic',
  define: { 'process.env.NODE_ENV': JSON.stringify('production'), 'import.meta.env.DEV': 'false' },
  loader: { '.svg': 'dataurl', '.ttf': 'dataurl', '.woff2': 'dataurl', '.png': 'dataurl' },
  logLevel: 'silent'
})
const engine = process.env.PLAYWRIGHT_ENGINE === 'webkit' ? webkit : chromium
let browser
try {
  browser = await engine.launch({ headless: true, channel: process.env.PLAYWRIGHT_CHANNEL })
  const page = await browser.newPage({ viewport: { width: 1100, height: 800 } })
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('pageerror', (error) => console.error(error.message))
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text())
  })
  await page.setContent(
    '<html data-platform="darwin" data-theme="fileterm-light"><body><div id="root"></div></body></html>'
  )
  await page.evaluate(() => {
    window.writes = []
    window.clipboardReads = 0
    window.clipboardText = 'printf paste-fixture'
    window.fileterm = {
      platform: 'darwin',
      getUiPreferences: async () => ({ terminalZoomLocked: false }),
      getUiStateItem: async () => null,
      setUiStateItem: async () => {},
      onUiPreferencesChanged: () => () => {},
      onTerminalData: () => () => {},
      onTerminalState: () => () => {},
      onTerminalZoomRequest: () => () => {},
      onTerminalGestureZoomRequest: () => () => {},
      resizeTerminal: async () => {},
      writeDiagnosticLog: async () => {},
      writeTerminal: async (_tabId, data) => window.writes.push(data),
      readClipboardText: async () => {
        window.clipboardReads++
        await new Promise((resolve) => window.setTimeout(resolve, 10))
        return window.clipboardText
      }
    }
  })
  await page.addStyleTag({ content: readFileSync(join(directory, 'bundle.css'), 'utf8') })
  await page.addScriptTag({ content: readFileSync(join(directory, 'bundle.js'), 'utf8') })
  const host = page.locator('.terminal-inner')
  await page.locator('.xterm-helper-textarea').waitFor({ state: 'attached' })
  const toggle = page.locator('.system-sidebar-toggle')
  const paste = await page.evaluate(() => window.labels.paste)
  // Connect and right-click immediately, before any paste/clipboard action.
  await page.evaluate(() => window.connectFixture())
  for (const edge of [false, true]) {
    const bounds = await host.boundingBox()
    const position = edge ? { x: bounds.width - 8, y: bounds.height - 8 } : { x: 100, y: 80 }
    await host.click({ button: 'right', position })
    await page.locator('.terminal-context-menu').waitFor()
    for (let update = 0; update < 80; update++) {
      await page.evaluate(() => window.updateFixture(true))
    }
    assert.equal(await page.evaluate(() => window.clipboardReads), 0)
    assert.equal(await page.locator('.terminal-context-menu').count(), 1)
    await page.keyboard.press('Escape')
    await page.locator('.terminal-context-menu').waitFor({ state: 'detached' })
  }
  for (const capability of [true, false]) {
    await page.evaluate((value) => window.updateFixture(value), capability)
    for (let cycle = 0; cycle < 20; cycle++) {
      // Update the parent while an actual xterm context menu is open.
      await host.click({ button: 'right', position: { x: 100, y: 80 } })
      const menu = page.locator('.terminal-context-menu')
      await menu.waitFor()
      for (let update = 0; update < 5; update++) {
        await page.evaluate((value) => window.updateFixture(value), capability)
      }
      await menu.getByRole('menuitem', { name: paste, exact: false }).click()
      await menu.waitFor({ state: 'detached' })
      await page.waitForFunction((count) => window.clipboardReads === count, cycle + (capability ? 0 : 20) + 1)
      await page.waitForFunction(() => document.activeElement?.classList.contains('xterm-helper-textarea'))
      await toggle.click()
      assert.equal(await page.locator('.fs-sidebar').evaluate((el) => el.classList.contains('is-collapsed')), true)
      await toggle.click()
      assert.equal(await page.locator('.fs-sidebar').evaluate((el) => el.classList.contains('is-collapsed')), false)
      assert.equal(await page.locator('.connection-summary').count(), 1)
    }
  }
  await page.waitForFunction(() => window.writes.filter((value) => value === window.clipboardText).length === 40)
  assert.equal(await page.evaluate(() => window.clipboardReads), 40)
  assert.deepEqual(errors, [])
  console.log(
    'PASS: connect then immediately right-click without paste, production React, 40 real xterm pastes, 360 open-menu parent updates, 40 sidebar cycles, no React errors'
  )
} finally {
  await browser?.close()
  rmSync(directory, { recursive: true, force: true })
}
