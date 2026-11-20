/* global window, document */
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { build } from 'vite'
import react from '@vitejs/plugin-react'
import ts from 'typescript'

const require = createRequire(import.meta.url)
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright')
const root = fileURLToPath(new URL('../../../../', import.meta.url))
const home = join(root, 'apps/tauri/src/renderer/features/workspace/home-workspace.tsx')
const directory = mkdtempSync(join(tmpdir(), 'fileterm-home-entrance-'))
let browser
try {
  writeFileSync(
    join(directory, 'fixture.tsx'),
    `
import { createRoot } from 'react-dom/client'
import { HomeWorkspace } from ${JSON.stringify(home)}
const root = createRoot(document.getElementById('root'))
let generation = 0
window.showHome = (suppressEntrance) => root.render(<HomeWorkspace key={++generation}
  suppressEntrance={suppressEntrance} profiles={[]} commandTemplates={[]}
  commandFolders={[]} tabBarProps={{}} />)
`
  )
  await build({
    configFile: false,
    define: { 'process.env.NODE_ENV': '"production"', 'import.meta.env.DEV': 'false' },
    plugins: [
      {
        name: 'isolate-home-navigation',
        enforce: 'pre',
        resolveId(id, importer) {
          if (importer === home && id.startsWith('.')) return '\0home-child:' + id
        },
        load(id) {
          if (!id.startsWith('\0home-child:')) return
          return ts.transpileModule(
            `
export const t = new Proxy({}, { get: (_, key) => String(key) })
export const resolveRendererPlatform = (platform) => platform
export const useExternalUrl = () => ({openExternalUrl: async () => {}, externalUrlError: null})
export const OverviewPage = () => <div className="overview-page"><section data-card>Overview</section></div>
export const ConnectionManagerModal = () => <div className="manager-inline"><section data-card>Connections</section></div>
${[
  'FeedbackText',
  'AppIcon',
  'VerticalScrollbar',
  'PortableUpdateDialog',
  'ProxyManagerPage',
  'CommandManagerModal',
  'SshKeyManagerPage',
  'SettingsModal',
  'TabBar',
  'HostGroupList',
  'BackgroundSessionsPage'
]
  .map((name) => `export const ${name} = () => null`)
  .join('\n')}
`,
            { compilerOptions: { module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX } }
          ).outputText
        }
      },
      react()
    ],
    resolve: {
      alias: [
        { find: /^react$/, replacement: require.resolve('react') },
        { find: /^react\/jsx-runtime$/, replacement: require.resolve('react/jsx-runtime') },
        { find: /^react-dom\/client$/, replacement: require.resolve('react-dom/client') }
      ]
    },
    build: {
      outDir: join(directory, 'dist'),
      minify: false,
      lib: {
        entry: join(directory, 'fixture.tsx'),
        name: 'HomeEntrance',
        formats: ['iife'],
        fileName: () => 'bundle.js'
      }
    },
    logLevel: 'error'
  })
  browser = await chromium.launch({ headless: true, executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH })
  const page = await browser.newPage({ reducedMotion: 'no-preference' })
  const errors = []
  page.on('pageerror', (error) => {
    errors.push(error.message)
    console.error(error.message)
  })
  await page.setContent(
    '<div class="workspace-stage-transition is-transitioning"><div class="workspace-content-stack__item is-active"><div id="root"></div></div></div>'
  )
  for (const name of ['shell.css', 'home.css']) {
    await page.addStyleTag({ path: join(root, 'apps/tauri/src/renderer/styles/features', name) })
  }
  await page.evaluate(() => {
    window.fileterm = { platform: 'darwin', getUpdateStatus: async () => null, onUpdateStatus: () => () => {} }
    window.entrances = []
    document.addEventListener('animationstart', (event) => window.entrances.push(event.animationName))
  })
  await page.addScriptTag({ content: readFileSync(join(directory, 'dist/bundle.js'), 'utf8') })
  await page.evaluate(() => window.showHome(false))
  await page.waitForSelector('[data-card]')
  await page.waitForFunction(() => window.entrances.includes('page-card-in-up'))

  await page.evaluate(() => {
    window.entrances = []
    window.showHome(true)
  })
  await page.waitForSelector('.suppress-home-entrance')
  await page.waitForTimeout(1200)
  assert.deepEqual(
    await page.evaluate(() => window.entrances),
    [],
    'returning home must stay still beyond the old 800ms timer'
  )
  await page.getByRole('button', { name: 'overview', exact: true }).click()
  assert.equal(await page.locator('.suppress-home-entrance').count(), 1)

  await page.getByRole('button', { name: 'connectionManager', exact: true }).click()
  await page.waitForFunction(() => window.entrances.includes('page-card-in-up'))
  await page.evaluate(() => {
    window.entrances = []
  })
  await page.getByRole('button', { name: 'overview', exact: true }).click()
  await page.waitForFunction(() => window.entrances.includes('page-card-in-down'))
  assert.deepEqual(errors, [])
  console.log(
    'Initial home animates; restored home stays still beyond 800ms; sidebar navigation keeps both fly-in directions.'
  )
} finally {
  await browser?.close()
  rmSync(directory, { recursive: true, force: true })
}
