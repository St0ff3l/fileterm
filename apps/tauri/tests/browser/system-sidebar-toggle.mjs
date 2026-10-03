// Run with ESBUILD_MODULE_PATH and PLAYWRIGHT_MODULE_PATH when tools are not installed locally.
/* global window, document */
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { readFileSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
const require = createRequire(import.meta.url)
const { build } = require(process.env.ESBUILD_MODULE_PATH || 'esbuild')
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright')
const directory = mkdtempSync(join(tmpdir(), 'fileterm-sidebar-toggle-'))
const root = fileURLToPath(new URL('../../../../', import.meta.url))
await build({
  stdin: {
    resolveDir: root,
    loader: 'tsx',
    contents: `
import React, { useState } from 'react'
import { createRoot } from 'react-dom/client'
import { SystemSidebarShell } from './apps/tauri/src/renderer/features/system/system-sidebar-shell'
import './apps/tauri/src/renderer/styles/index.css'
function Fixture({ capability, hasMetrics }) {
  const [collapsed, setCollapsed] = useState(false)
  const [width, setWidth] = useState(214)
  return <div style={{width: collapsed ? 44 : width}}>
    <SystemSidebarShell activeProfile={{type:'ssh',host:'test-host'}}
      activeTabId="tab" connectionStatus="connected" collapsed={collapsed}
      activeSession={{profileId:'profile',connected:true,capabilities:{resourceMonitoring:capability},
        systemMetrics: hasMetrics ? {cpuPercent:5,memoryPercent:10,
          networkInterfaces:['eth0'],activeNetworkInterface:'eth0',networkSamples:[],
          fileSystemRows:[{name:'/dev/root',mountPoint:'/',size:'10G',available:'5G',used:'5G',usePercent:50}]} : undefined}}
      showResourceMeters={capability !== false} visibleMetrics={['cpu','memory','disk','network']}
      isResizing={false} onMonitoringSnapshot={() => {}} onOpenSystemInfo={() => {}}
      onResizeStart={() => {}} onRestoreWidth={() => setWidth(214)} onToggleCollapsed={setCollapsed}/>
  </div>
}
const root = createRoot(document.getElementById('root'))
window.renderFixture = (capability, hasMetrics) => root.render(<Fixture key={String(capability)+hasMetrics} capability={capability} hasMetrics={hasMetrics}/>)
`
  },
  bundle: true,
  outfile: join(directory, 'bundle.js'),
  jsx: 'automatic',
  loader: { '.svg': 'dataurl', '.ttf': 'dataurl', '.woff2': 'dataurl', '.png': 'dataurl' },
  logLevel: 'silent'
})
const browser = await chromium.launch({ headless: true, channel: process.env.PLAYWRIGHT_CHANNEL })
try {
  const page = await browser.newPage({ viewport: { width: 1000, height: 760 } })
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text())
  })
  await page.setContent('<html data-theme="fileterm-light"><body><div id="root"></div></body></html>')
  await page.addStyleTag({ content: readFileSync(join(directory, 'bundle.css'), 'utf8') })
  await page.addScriptTag({ content: readFileSync(join(directory, 'bundle.js'), 'utf8') })
  for (const platform of ['darwin', 'win32', 'linux']) {
    await page.evaluate((platform) => {
      document.documentElement.dataset.platform = platform
      window.fileterm = { platform }
    }, platform)
    for (const capability of [true, false, undefined]) {
      for (const hasMetrics of [true, false]) {
        await page.evaluate(({ capability, hasMetrics }) => window.renderFixture(capability, hasMetrics), {
          capability,
          hasMetrics
        })
        const toggle = page.locator('.system-sidebar-toggle')
        await toggle.waitFor()
        for (let cycle = 0; cycle < 10; cycle++) {
          await toggle.click()
          assert.equal(await page.locator('.fs-sidebar').evaluate((el) => el.classList.contains('is-collapsed')), true)
          await toggle.click()
          assert.equal(await page.locator('.fs-sidebar').evaluate((el) => el.classList.contains('is-collapsed')), false)
          assert.equal(await page.locator('.connection-summary').count(), 1)
          assert.equal(await page.locator('.system-metrics-scroll-region').count(), capability === false ? 0 : 1)
          assert.equal(await page.locator('.disk-table').count(), capability === false ? 0 : 1)
        }
      }
    }
  }
  assert.deepEqual(errors, [])
  console.log(
    'PASS: 180 sidebar collapse/expand cycles, three platform branches, unavailable/pending metrics, no React errors'
  )
} finally {
  await browser.close()
  rmSync(directory, { recursive: true, force: true })
}
