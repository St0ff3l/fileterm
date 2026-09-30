// Run with Playwright and esbuild installed, or set their *_MODULE_PATH variables.
/* global window, document */
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
const require = createRequire(import.meta.url)
const { build } = require(process.env.ESBUILD_MODULE_PATH || 'esbuild')
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright')
import { readFileSync, mkdtempSync, rmSync } from 'node:fs'
import assert from 'node:assert/strict'
const root = fileURLToPath(new URL('../../../../', import.meta.url))
const directory = mkdtempSync(join(tmpdir(), 'fileterm-monitoring-browser-'))
const entry = `
import { setLocale } from './apps/tauri/src/renderer/i18n'
import React from 'react'
import { createRoot } from 'react-dom/client'
import { SystemSidebar } from './apps/tauri/src/renderer/features/system/system-sidebar'
import './apps/tauri/src/renderer/styles/index.css'
setLocale('zhCN')
window.calls = []
window.reconnectCalls = []
window.reconnectFails = false
window.toggleCalls = []
window.toggleFails = false
window.toggleHangs = false
window.snapshots = []
window.fileterm = { reconnectTab: async (tabId) => { window.reconnectCalls.push(tabId); await new Promise(resolve => setTimeout(resolve, 300)); if(window.reconnectFails) throw new Error('failure'); window.renderMonitoring('disconnected', false, false, 'connecting'); return {tabs:[{id:tabId,status:'connecting'}]} }, setMonitoringEnabled: async (...args) => {window.toggleCalls.push(args); if(window.toggleHangs) return new Promise(() => {}); await new Promise(resolve => setTimeout(resolve, 100)); if(window.toggleFails) throw new Error('control failure'); return {phase:args[2] ? 'starting' : 'stopped'}}, retryMonitoring: async (...args) => { window.calls.push(args); await new Promise(resolve => setTimeout(resolve, 300)) } }
const root = createRoot(document.getElementById('root'))
window.renderMonitoring = (phase = 'paused', collapsed = false, connected = true, connectionStatus = connected ? 'connected' : 'error') => {
 window.view = {phase,collapsed,connected,connectionStatus}
 root.render(<SystemSidebar
 activeProfile={{type:'ssh',host:'test-host'}} activeTabId="tab" collapsed={collapsed} connectionStatus={connectionStatus} showResourceMeters={true}
 onMonitoringSnapshot={snapshot => {
 window.snapshots.push(snapshot)
 const view = window.view
 if (snapshot.phase) window.renderMonitoring(snapshot.phase, view.collapsed, view.connected, view.connectionStatus)
 else if (snapshot.tabs) window.renderMonitoring(view.phase, view.collapsed, view.connected, snapshot.tabs[0].status)
 }}
 visibleMetrics={['cpu','memory','network']} onOpenSystemInfo={() => {}} onToggleCollapsed={() => window.renderMonitoring(phase, !collapsed, connected)}
 activeSession={{connected, capabilities:{resourceMonitoring:true}, monitoring:{generation:2,revision:3,phase,attempt:1,maxAttempts:5,intervalSeconds:1,lastSampleAt:Date.now()-18000,nextRetryAt:phase==='paused'||phase==='waiting'?Date.now()+42000:undefined}, systemMetrics:{cpuPercent:5,memoryPercent:10,networkInterfaces:[],networkSamples:[{rx:1,tx:2}]}}}
/>)
}
window.renderMonitoring()
`
await build({
  stdin: { contents: entry, resolveDir: root, loader: 'tsx' },
  bundle: true,
  outfile: join(directory, 'bundle.js'),
  jsx: 'automatic',
  loader: { '.svg': 'dataurl', '.ttf': 'dataurl', '.woff2': 'dataurl', '.png': 'dataurl' },
  logLevel: 'silent'
})
const browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] })
try {
  const page = await browser.newPage({ viewport: { width: 1000, height: 760 } })
  const errors = []
  page.on('pageerror', (e) => errors.push(e.message))
  await page.setContent(
    '<html data-theme="fileterm-dark" data-platform="linux"><body style="margin:0"><main style="display:flex;height:760px"><aside id="root" style="width:250px;position:relative"></aside><textarea aria-label="terminal" style="flex:1">terminal stays usable</textarea></main></body></html>'
  )
  await page.addStyleTag({ content: readFileSync(join(directory, 'bundle.css'), 'utf8') })
  await page.addScriptTag({ content: readFileSync(join(directory, 'bundle.js'), 'utf8') })
  await page.locator('.monitoring-overlay').waitFor()

  await page.locator('.monitoring-overlay button').click()
  assert.equal(await page.evaluate(() => window.calls.length), 1)
  await page.waitForTimeout(350)
  await page.locator('textarea').fill('terminal still works')
  assert.equal(await page.locator('textarea').inputValue(), 'terminal still works')
  for (const phase of ['waiting', 'recovering', 'failed', 'starting']) {
    await page.evaluate((p) => window.renderMonitoring(p), phase)
    await page.waitForTimeout(50)
    assert.equal(await page.locator('.monitoring-overlay').getAttribute('data-phase'), phase)
    assert.equal(
      await page.locator('.monitoring-overlay button').isDisabled(),
      ['recovering', 'starting'].includes(phase)
    )
  }
  await page.evaluate(() => window.renderMonitoring('healthy'))
  await page.waitForTimeout(50)
  assert.equal(await page.locator('.monitoring-overlay').count(), 0)
  assert.equal(await page.locator('.sys-card').getAttribute('inert'), null)
  await page.evaluate(() => window.renderMonitoring('paused', true))
  await page.waitForTimeout(50)
  assert.equal(await page.locator('.monitoring-collapsed').count(), 1)
  await page.locator('.system-sidebar-toggle').click()
  await page.locator('.monitoring-overlay').waitFor()
  for (const theme of ['fileterm-light', 'fileterm-dark', 'codex-light', 'codex-dark']) {
    await page.evaluate((theme) => {
      document.documentElement.dataset.theme = theme
      document.getElementById('root').style.width = '214px'
    }, theme)
    assert.equal(
      await page.locator('.monitoring-overlay-message').evaluate((el) => el.scrollWidth <= el.clientWidth),
      true
    )
  }
  await page.evaluate(() => {
    document.documentElement.dataset.theme = 'fileterm-light'
    window.renderMonitoring('failed')
  })
  await page.waitForTimeout(50)

  await page.evaluate(() => window.renderMonitoring('disconnected'))
  await page.locator('.monitoring-overlay[data-phase="disconnected"]').waitFor()
  const before = await page.evaluate(() => window.calls.length)
  await page.getByRole('button', { name: '立即重试监控', exact: true }).click()
  assert.equal(await page.evaluate(() => window.calls.length), before + 1)
  assert.equal(await page.evaluate(() => window.reconnectCalls.length), 0)
  await page.waitForTimeout(350)
  await page.locator('.monitoring-toggle').click()
  await page.getByRole('dialog').getByRole('button', { name: '停止监控', exact: true }).click()
  await page.locator('.monitoring-overlay[data-phase="stopped"]').waitFor()
  assert.equal(await page.locator('.monitoring-overlay button').count(), 0)
  assert.equal(await page.locator('.monitoring-toggle').isDisabled(), false)
  assert.equal(await page.locator('.monitoring-toggle').getAttribute('data-stopped'), 'true')
  await page.getByRole('button', { name: '启动监控', exact: true }).click()
  await page.locator('.monitoring-overlay[data-phase="starting"]').waitFor()
  assert.equal(await page.evaluate(() => window.toggleCalls.length), 2)
  await page.evaluate(() => {
    window.reconnectFails = true
    window.renderMonitoring('disconnected', false, false)
  })
  await page.locator('.monitoring-overlay[data-phase="ssh-disconnected"]').waitFor()
  assert.equal(await page.locator('.monitoring-toggle').count(), 0)
  await page.getByRole('button', { name: '重新连接 SSH', exact: true }).click()
  await page.locator('.monitoring-overlay [role="alert"]').waitFor()
  await page.evaluate(() => {
    window.reconnectFails = false
  })
  await page.getByRole('button', { name: '重新连接 SSH', exact: true }).click()
  assert.equal(await page.locator('.monitoring-overlay button').isDisabled(), true)
  await page.locator('.monitoring-overlay[data-phase="reconnecting"]').waitFor()
  assert.equal(await page.locator('.monitoring-overlay button').isDisabled(), true)
  assert.equal(await page.evaluate(() => window.reconnectCalls.length), 2)
  assert.equal(await page.evaluate(() => window.calls.length), before + 1)
  await page.evaluate(() => window.renderMonitoring('healthy'))
  await page.locator('.monitoring-overlay').waitFor({ state: 'detached' })
  assert.equal(await page.locator('.monitoring-toggle').count(), 1)
  // Exercise both directions without a metrics event: only the command's
  // confirmed snapshot may update the UI.
  for (let cycle = 0; cycle < 3; cycle += 1) {
    await page.locator('.monitoring-toggle').click()
    await page.getByRole('dialog').getByRole('button', { name: '停止监控', exact: true }).click()
    await page.locator('.monitoring-overlay[data-phase="stopped"]').waitFor()
    await page.getByRole('button', { name: '启动监控', exact: true }).click()
    await page.locator('.monitoring-overlay[data-phase="starting"]').waitFor()
    assert.equal(await page.locator('.monitoring-toggle').isDisabled(), false)
  }
  await page.evaluate(() => {
    window.toggleFails = true
  })
  await page.locator('.monitoring-toggle').click()
  await page.getByRole('dialog').getByRole('button', { name: '停止监控', exact: true }).click()
  await page.getByRole('dialog').getByRole('alert').waitFor()
  assert.equal(
    await page.getByRole('dialog').getByRole('button', { name: '停止监控', exact: true }).isDisabled(),
    false
  )
  await page.getByRole('dialog').getByRole('button', { name: '取消', exact: true }).click()
  await page.evaluate(() => {
    window.toggleFails = false
    window.toggleHangs = true
    window.renderMonitoring('stopped')
  })
  await page.clock.install()
  await page.getByRole('button', { name: '启动监控', exact: true }).click()
  await page.clock.fastForward(21000)
  await page.locator('.monitoring-control-error').waitFor()
  assert.equal(await page.locator('.monitoring-toggle').isDisabled(), false)
  await page.evaluate(() => {
    window.toggleHangs = false
  })
  await page.clock.resume()
  // Command results must reach the workspace after the button unmounts.
  await page.evaluate(() => {
    window.renderMonitoring('stopped')
    window.fileterm.setMonitoringEnabled = async () => {
      await new Promise((resolve) => {
        window.finishToggle = resolve
      })
      return { phase: 'starting' }
    }
  })
  const snapshotCount = await page.evaluate(() => window.snapshots.length)
  await page.getByRole('button', { name: '启动监控', exact: true }).click()
  await page.locator('.system-sidebar-toggle').click()
  await page.evaluate(() => window.finishToggle())
  await page.waitForFunction((count) => window.snapshots.length === count + 1, snapshotCount)
  assert.equal(await page.locator('.monitoring-collapsed').getAttribute('title'), '正在启动监控')
  await page.locator('.system-sidebar-toggle').click()
  await page.locator('.monitoring-overlay[data-phase="starting"]').waitFor()

  // Timing out bounds UI waiting, but a late successful result is still applied.
  await page.evaluate(() => window.renderMonitoring('stopped'))
  await page.getByRole('button', { name: '启动监控', exact: true }).click()
  await page.clock.fastForward(21000)
  await page.locator('.monitoring-control-error').waitFor()
  await page.evaluate(() => window.finishToggle())
  await page.locator('.monitoring-overlay[data-phase="starting"]').waitFor()

  assert.equal(await page.locator('.monitoring-control-error').count(), 0)

  // A late stop confirmation must close its own error dialog.
  await page.evaluate(() => {
    window.renderMonitoring('healthy')
    window.fileterm.setMonitoringEnabled = async () => {
      await new Promise((resolve) => {
        window.finishStop = resolve
      })
      return { phase: 'stopped' }
    }
  })
  await page.locator('.monitoring-toggle').click()
  await page.getByRole('dialog').getByRole('button', { name: '停止监控', exact: true }).click()
  await page.clock.fastForward(21000)
  await page.getByRole('dialog').getByRole('alert').waitFor()
  await page.evaluate(() => window.finishStop())
  await page.getByRole('dialog').waitFor({ state: 'detached' })
  await page.locator('.monitoring-overlay[data-phase="stopped"]').waitFor()
  assert.equal(await page.locator('.monitoring-control-error').count(), 0)

  // An old success may update the workspace, but must not dismiss a newer error.
  await page.evaluate(() => {
    window.renderMonitoring('stopped')
    window.toggleSequence = 0
    window.fileterm.setMonitoringEnabled = async () => {
      if (++window.toggleSequence === 1) {
        await new Promise((resolve) => {
          window.finishOldToggle = resolve
        })
        return { phase: 'starting' }
      }
      throw new Error('newer request failed')
    }
  })
  await page.getByRole('button', { name: '启动监控', exact: true }).click()
  await page.clock.fastForward(21000)
  await page.locator('.monitoring-control-error').waitFor()
  await page.getByRole('button', { name: '启动监控', exact: true }).click()
  await page.waitForFunction(() => window.toggleSequence === 2)
  await page.locator('.monitoring-control-error').waitFor()
  await page.evaluate(() => window.finishOldToggle())
  await page.locator('.monitoring-overlay[data-phase="starting"]').waitFor()
  assert.equal(await page.locator('.monitoring-control-error').count(), 1)

  // A late success must not close a newly opened confirmation dialog.
  await page.evaluate(() => {
    window.renderMonitoring('stopped')
    window.fileterm.setMonitoringEnabled = async () => {
      await new Promise((resolve) => {
        window.finishBeforeDialog = resolve
      })
      return { phase: 'starting' }
    }
  })
  await page.getByRole('button', { name: '启动监控', exact: true }).click()
  await page.clock.fastForward(21000)
  await page.locator('.monitoring-control-error').waitFor()
  await page.evaluate(() => window.renderMonitoring('healthy'))
  await page.locator('.monitoring-toggle').click()
  await page.getByRole('dialog').waitFor()
  await page.evaluate(() => window.finishBeforeDialog())
  await page.locator('.monitoring-overlay[data-phase="starting"]').waitFor()
  assert.equal(await page.getByRole('dialog').count(), 1)
  await page.getByRole('dialog').getByRole('button', { name: '取消', exact: true }).click()

  // Both overlay commands recover their buttons after a missing IPC reply.
  for (const ssh of [false, true]) {
    await page.evaluate((ssh) => {
      window.renderMonitoring(ssh ? 'disconnected' : 'failed', false, !ssh)
      if (ssh) window.fileterm.reconnectTab = async () => new Promise(() => {})
      else window.fileterm.retryMonitoring = async () => new Promise(() => {})
    }, ssh)
    const button = page.locator('.monitoring-overlay button')
    await button.click()
    await page.clock.fastForward(21000)
    await page.locator('.monitoring-overlay [role="alert"]').waitFor()
    assert.equal(await button.isDisabled(), false)
    await page.evaluate((ssh) => {
      if (ssh) window.fileterm.reconnectTab = async () => ({ tabs: [{ id: 'tab', status: 'connecting' }] })
      else
        window.fileterm.retryMonitoring = async () => {
          window.calls.push(['retry-after-timeout'])
        }
    }, ssh)
    await button.click()
    if (ssh) await page.locator('.monitoring-overlay[data-phase="reconnecting"]').waitFor()
    else {
      await page.waitForFunction(() => window.calls.at(-1)?.[0] === 'retry-after-timeout')
      assert.equal(await page.locator('.monitoring-overlay [role="alert"]').count(), 0)
    }
  }
  assert.deepEqual(errors, [])
  console.log(
    'PASS: overlay recovery states, manual retry, terminal input, collapse, four themes, 214px width, disconnect'
  )
} finally {
  await browser.close()
  rmSync(directory, { recursive: true, force: true })
}
