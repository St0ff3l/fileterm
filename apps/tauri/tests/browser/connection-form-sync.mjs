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
const directory = mkdtempSync(join(tmpdir(), 'fileterm-form-sync-'))
await build({
  stdin: {
    resolveDir: fileURLToPath(new URL('../../../../', import.meta.url)),
    loader: 'tsx',
    contents: `
import { Component, useEffect } from 'react'
import { createRoot } from 'react-dom/client'
import { useWorkspaceModals } from './apps/tauri/src/renderer/hooks/use-workspace-modals'
import { ConnectionProxySection } from './apps/tauri/src/renderer/features/connections/connection-proxy-section'
import { ConnectionTunnelSection } from './apps/tauri/src/renderer/features/connections/connection-tunnel-section'
class Boundary extends Component {
  state = {error:null}
  static getDerivedStateFromError(error) {return {error}}
  render() { return this.state.error ? <div data-crash>{this.state.error.message}</div> : this.props.children }
}
function Fixture({kind}) {
  window.renderCounts[kind] = (window.renderCounts[kind] ?? 0) + 1
  const modals = useWorkspaceModals({desktopApi:window.fileterm,folders:[],profiles:[],
    connectionDefaults:{},formWindowMode:'create',formWindowProfileId:null,
    hasLoadedInitialSnapshot:false,isConnectionFormWindow:false})
  useEffect(() => modals.setForm((form) => ({...form,
    proxyProfileId:kind === 'proxy' ? 'proxy' : undefined,
    tunnelProfileId:kind === 'tunnel' ? 'tunnel' : undefined})), [kind])
  window.latestForm = modals.form
  return kind === 'proxy'
    ? <ConnectionProxySection form={modals.form} setForm={modals.updateForm}/>
    : <ConnectionTunnelSection form={modals.form} setForm={modals.updateForm}/>
}
const root = createRoot(document.getElementById('root'))
window.renderCounts = {}
window.renderFixture = (kind) => root.render(<Boundary key={kind}><Fixture kind={kind}/></Boundary>);
`
  },
  bundle: true,
  outfile: join(directory, 'bundle.js'),
  jsx: 'automatic',
  define: { 'process.env.NODE_ENV': JSON.stringify('production'), 'import.meta.env.DEV': 'false' },
  loader: { '.svg': 'dataurl', '.ttf': 'dataurl', '.woff2': 'dataurl', '.png': 'dataurl' },
  logLevel: 'silent'
})
let browser
try {
  const engine = process.env.PLAYWRIGHT_ENGINE === 'webkit' ? webkit : chromium
  browser = await engine.launch({ headless: true, channel: process.env.PLAYWRIGHT_CHANNEL })
  const page = await browser.newPage()
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text())
  })
  await page.setContent('<html><body><main id="root"></main></body></html>')
  await page.evaluate(() => {
    window.proxyPort = 1080
    window.bindPort = 8080
    window.refreshLibrary = () => {}
    window.fileterm = {
      platform: 'darwin',
      listProxyProfiles: async () => [
        { id: 'proxy', name: 'fixture proxy', type: 'socks5', host: '127.0.0.1', port: window.proxyPort }
      ],
      listTunnelProfiles: async () => [
        {
          id: 'tunnel',
          name: 'fixture tunnel',
          type: 'ssh',
          forwards: [
            {
              id: 'rule',
              kind: 'local',
              bindHost: '127.0.0.1',
              bindPort: window.bindPort,
              targetHost: 'localhost',
              targetPort: 80,
              autoStart: true
            }
          ]
        }
      ],
      onProxiesChanged: (listener) => {
        window.refreshLibrary = listener
        return () => {}
      },
      onTunnelsChanged: (listener) => {
        window.refreshLibrary = listener
        return () => {}
      }
    }
  })
  await page.addScriptTag({ content: readFileSync(join(directory, 'bundle.js'), 'utf8') })
  for (const kind of process.env.FORM_SYNC_KIND ? [process.env.FORM_SYNC_KIND] : ['proxy', 'tunnel']) {
    await page.evaluate((kind) => window.renderFixture(kind), kind)
    await page.waitForFunction(
      (kind) =>
        document.querySelector('[data-crash]') ||
        (kind === 'proxy'
          ? window.latestForm?.proxy?.port === 1080
          : window.latestForm?.forwards?.[0]?.bindPort === 8080),
      kind
    )
    assert.equal(
      await page.locator('[data-crash]').count(),
      0,
      `${kind}: form synchronization must settle without React #185`
    )
    const counts = await page.evaluate(async (kind) => {
      const frames = async () => {
        for (let frame = 0; frame < 4; frame++) await new Promise((resolve) => window.requestAnimationFrame(resolve))
      }
      await frames()
      const before = window.renderCounts[kind]
      await frames()
      return [before, window.renderCounts[kind]]
    }, kind)
    assert.equal(counts[1], counts[0], `${kind}: a populated form must stop rendering once synchronized`)
    const settledCount = counts[1]
    for (let update = 0; update < 10; update++) {
      await page.evaluate(() => window.refreshLibrary())
      await page.waitForFunction(
        (kind) =>
          kind === 'proxy' ? window.latestForm.proxy.port === 1080 : window.latestForm.forwards[0].bindPort === 8080,
        kind
      )
    }
    await page.evaluate(async () => {
      for (let frame = 0; frame < 4; frame++) await new Promise((resolve) => window.requestAnimationFrame(resolve))
    })
    assert.equal(
      await page.evaluate((kind) => window.renderCounts[kind], kind),
      settledCount,
      `${kind}: identical library refreshes must not write the parent form`
    )
    await page.evaluate((kind) => {
      if (kind === 'proxy') window.proxyPort = 1081
      else window.bindPort = 8081
      window.refreshLibrary()
    }, kind)
    await page.waitForFunction(
      (kind) =>
        kind === 'proxy' ? window.latestForm.proxy.port === 1081 : window.latestForm.forwards[0].bindPort === 8081,
      kind
    )
    assert.equal(await page.locator('[data-crash]').count(), 0)
  }
  assert.deepEqual(errors, [])
  console.log(
    'PASS: actual workspace form hook, saved proxy and tunnel sync settle, library refreshes and changed values propagate'
  )
} finally {
  await browser?.close()
  rmSync(directory, { recursive: true, force: true })
}
