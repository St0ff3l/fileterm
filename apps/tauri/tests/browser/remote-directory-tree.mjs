/* global window, document */
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { readFileSync, writeFileSync, mkdtempSync, realpathSync, rmSync } from 'node:fs'
import { createServer } from 'node:http'
import { tmpdir } from 'node:os'
import { join, extname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { build } from 'vite'
import react from '@vitejs/plugin-react'
const require = createRequire(import.meta.url)
const { chromium } = require(process.env.PLAYWRIGHT_MODULE_PATH || 'playwright')
const directory = realpathSync(mkdtempSync(join(tmpdir(), 'fileterm-directory-tree-')))
const root = fileURLToPath(new URL('../../../../', import.meta.url))
writeFileSync(
  join(directory, 'index.html'),
  '<html data-theme="fileterm-dark"><body><div id="root"></div><script type="module" src="/fixture.tsx"></script></body></html>'
)
writeFileSync(
  join(directory, 'fixture.tsx'),
  `
import React, { useState } from 'react'
import { createCodexThemeConfig, createDefaultThemeConfig, normalizeThemeConfig } from '${root}packages/core/src/index'
import { createRoot } from 'react-dom/client'
import { FileManager } from '${root}apps/tauri/src/renderer/features/files/file-manager'
import { useSettingsModalState } from '${root}apps/tauri/src/renderer/features/settings/settings-modal/controller/state'
import { useThemeSettingsController } from '${root}apps/tauri/src/renderer/features/settings/settings-modal/controller/theme'
import { applyThemeVariables, deriveThemeVariant, getThemeConfigFromTokenPreset } from '${root}apps/tauri/src/renderer/app/theme-config'
import '${root}apps/tauri/src/renderer/styles/index.css'
import '${root}apps/tauri/src/renderer/features/layout/tab-bar.css'
window.fixtureApplyConfig = (config) => applyThemeVariables('fileterm-light', config)
window.fixturePresetConfigFor = (preset,variant) => normalizeThemeConfig(preset === 'fileterm' ? createDefaultThemeConfig(variant) : preset === 'codex' ? createCodexThemeConfig(variant) : getThemeConfigFromTokenPreset(preset,variant),variant)
window.fixtureTheme = (mode) => {
 const variant = mode.endsWith('light') ? 'light' : 'dark'
 const config = mode.startsWith('iterm2-') ? getThemeConfigFromTokenPreset(mode.replace(/-(dark|light)$/,''),variant) : mode.startsWith('codex') ? createCodexThemeConfig(variant) : undefined
 applyThemeVariables(mode.startsWith('iterm2-') ? 'fileterm-'+variant : mode,config)
}
window.fixturePresetRead = (id,variant) => getThemeConfigFromTokenPreset(id,variant).theme
window.fixtureLegacyTheme = () => {
 const legacy = createDefaultThemeConfig('dark')
 legacy.codeThemeId = 'custom'
 delete legacy.theme.surfaceSidebar
 delete legacy.theme.surfaceInput
 delete legacy.theme.surfaceContextMenu
 legacy.theme.surfaceElevated = '#2e2e2e'
 legacy.theme.overrides = {'--ref-preview-dark-content':'#2a2a2a'}
 const normalized = normalizeThemeConfig(legacy)
 const light = deriveThemeVariant(normalized,'light')
 return {surface:normalized.theme.surfaceElevated,override:normalized.theme.overrides['--ref-preview-dark-content'],variant:light.variant,hasSidebar:Object.hasOwn(light.theme,'surfaceSidebar')}
}
window.fixtureSurfaceColors = (mode) => {
 const config = mode.startsWith('codex') ? createCodexThemeConfig(mode.endsWith('light') ? 'light' : 'dark') : createDefaultThemeConfig(mode.endsWith('light') ? 'light' : 'dark')
 config.codeThemeId = 'custom'
 Object.assign(config.theme, {surfaceSidebar:'#123456',surfaceInput:'#234567',surfaceContextMenu:'#345678',surfaceElevated:'#2a2a2a'})
 applyThemeVariables(mode, normalizeThemeConfig(config))
}
applyThemeVariables('fileterm-dark')
const folder = (path) => ({path,name:path.split('/').at(-1)||'/',type:'folder',size:'-',modified:''})
const file = (path) => ({...folder(path),type:'file',size:'1 KB'})
const catalog = {
  '/':[folder('/etc'),folder('/home'),folder('/restricted')],
  '/home':[folder('/home/user')], '/home/user':[folder('/home/user/logs'),file('/home/user/note.txt')],
  '/home/user/logs':[], '/etc':[folder('/etc/ssh')], '/etc/ssh':[], '/restricted':[]
}
window.fixtureTransfers = []
window.fixtureRequests = []
window.fixtureFailRestricted = true
window.fileterm = new Proxy({
  platform: new URLSearchParams(location.search).get('platform') || 'darwin',
  getUiStateItem: async (key) => sessionStorage.getItem(key),
  setUiStateItem: async (key,value) => sessionStorage.setItem(key,value),
  listRemoteDirectory: async (_tab,path) => {
    window.fixtureRequests.push(path)
    if(path==='/restricted' && window.fixtureFailRestricted) throw new Error('Permission denied')
    return catalog[path] || []
  }
}, {get:(target,key) => target[key] ?? (String(key).startsWith('on') ? () => () => {} : async () => [])})
function Fixture() {
  const [session,setSession] = useState({profileId:'preview',connected:true,remotePath:'/home/user',remoteFiles:catalog['/home/user'],capabilities:{files:true,tunnels:false}})
  const [access,setAccess] = useState('user')
  const [paneMode,setPaneMode] = useState(() => sessionStorage.getItem('fixture-pane-mode') || 'local')
  const [showPresetSync,setShowPresetSync] = useState(false)
  const navigate = (path) => setSession(current=>({...current,remotePath:path,remoteFiles:catalog[path]||[]}))
  window.fixtureNavigate = navigate
  window.fixtureConnect = (connected) => setSession(current=>({...current,connected}))
  window.fixtureAccess = setAccess
  window.fixtureStartPresetSync = () => setShowPresetSync(true)
  const noop = () => {}
  return <><main style={{height:480,margin:16,display:'grid',gridTemplateRows:'minmax(0,1fr)'}}><FileManager
    activeSession={session} activeTab={{id:'preview-tab',profileId:'preview',sessionType:'ssh'}} activeView="file"
    localPath="/Users/demo" localPanePath="/Users/demo" localItems={[folder('/Users/demo/Documents'),file('/Users/demo/local.txt')]}
    filePaneMode={paneMode} onSetProfileFilePaneMode={async (_,mode)=>{sessionStorage.setItem('fixture-pane-mode',mode);setPaneMode(mode)}}
    commandPaneWidth={240} sendTargets={[]} commandFolders={[]} commandTemplates={[]}
    isBusy={false} isLocalNetworkShare={false} isLocalDirectoryLoading={false} isWorkspaceRefreshing={false} isWorkspaceSwitching={false}
    canPasteToLocal={false} canPasteToRemote={false} clipboardStatusText={null} localCutPaths={[]} remoteCutPaths={[]}
    remoteFileAccessMode={access} isRemoteDirectoryLoading={false}
    onActiveViewChange={noop} onCommandPaneWidthChange={noop} onExecuteCommand={noop} onSendTerminalCommand={async()=>{}}
    onSaveTemporaryCommand={noop} onUpdateCommand={noop} onOpenCommandManager={noop} onCopyItems={noop} onCutItems={noop} onClearCutState={noop}
    onOpenLocalItem={noop} onOpenLocalPath={noop} onBackToLocalComputer={noop} onOpenRemoteItem={item=>navigate(item.path)} onOpenRemotePath={navigate}
    onPasteIntoPane={noop} onRefresh={()=>setSession(current=>({...current,remoteFiles:[...catalog[current.remotePath]]}))}
    onUploadFiles={items=>window.fixtureTransfers.push({direction:'upload',paths:items.map(item=>item.path)})} onChooseUploadFiles={noop} onDownloadFiles={(items,target)=>window.fixtureTransfers.push({direction:'download',paths:items.map(item=>item.path),target})} onDownloadLocalNetworkFiles={noop} onDropUpload={noop}
    onRequestChangePermissions={noop} onRequestDelete={noop} onRequestNewFile={noop} onRequestNewFolder={noop} onRequestQuickDelete={noop} onRequestRename={noop}
    onToggleFollowShellCwd={noop} onToggleRemoteFileAccessMode={noop}/></main>{showPresetSync ? <PresetSyncProbe/> : null}</>
}
function PresetSyncProbe() {
 const [themeConfig,setThemeConfig] = useState(() => {
  const preset = new URLSearchParams(location.search).get('preset')
  return preset === 'fileterm' ? createDefaultThemeConfig('light') : preset === 'codex' ? createCodexThemeConfig('light') : getThemeConfigFromTokenPreset('iterm2-gruvbox','light')
 })
 const [theme,setTheme] = useState('fileterm-light')
 const [customThemes,setCustomThemes] = useState([])
 const state = useSettingsModalState('interface')
 const controller = useThemeSettingsController({state,options:{theme,themeConfig,customThemes,onSetTheme:setTheme,onSetThemeConfig:setThemeConfig,onSetCustomThemes:setCustomThemes}})
 window.fixtureSetFont = controller.updateThemeFonts
 window.fixtureSwitchVariant = controller.switchThemeVariant
 window.fixtureSetThemeName = controller.setCustomThemeName
 window.fixtureSaveTheme = controller.saveCustomTheme
 window.fixtureSavedThemes = customThemes
 window.fixturePresetConfig = themeConfig
 window.fixturePresetSyncRerender = () => setThemeConfig(current=>({...current}))
 return null
}
createRoot(document.getElementById('root')).render(<Fixture/> )
`
)
await build({
  configFile: false,
  root: directory,
  base: './',
  plugins: [react()],
  logLevel: 'error',
  resolve: {
    alias: [
      { find: /^react$/, replacement: require.resolve('react') },
      { find: /^react\/jsx-runtime$/, replacement: require.resolve('react/jsx-runtime') },
      { find: /^react-dom\/client$/, replacement: require.resolve('react-dom/client') }
    ]
  },
  build: { minify: false, outDir: join(directory, 'dist') }
})
const server = createServer((request, response) => {
  try {
    const path = new URL(request.url, 'http://localhost').pathname
    const data = readFileSync(join(directory, 'dist', path === '/' ? 'index.html' : path))
    const type =
      {
        '.js': 'text/javascript',
        '.css': 'text/css',
        '.html': 'text/html',
        '.ttf': 'font/ttf',
        '.svg': 'image/svg+xml'
      }[extname(path)] || 'application/octet-stream'
    response.writeHead(200, { 'Content-Type': path === '/' ? 'text/html' : type })
    response.end(data)
  } catch {
    response.writeHead(404)
    response.end()
  }
})
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve))
const browser = await chromium.launch({ headless: true, channel: process.env.PLAYWRIGHT_CHANNEL })
try {
  const page = await browser.newPage({ viewport: { width: 1100, height: 530 } })
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  const address = `http://127.0.0.1:${server.address().port}`
  const mode = async (value) => {
    const native = page.locator('.file-pane-mode-select select')
    if (await native.count()) await native.selectOption(value)
    else {
      await page.locator('.file-pane-mode-select').click()
      await page
        .locator('.dropdown-select-menu button')
        .nth(value === 'local' ? 0 : 1)
        .click()
    }
  }
  const assertSidebarCorner = async (checkSettingsSidebar = false, codexMacDark = false) => {
    const colors = await page.evaluate(
      ({ checkSettingsSidebar, codexMacDark }) => {
        const probe = document.createElement('div')
        probe.className = 'fs-shell is-home-active'
        probe.style.position = 'fixed'
        probe.style.left = '-10000px'
        probe.innerHTML =
          '<div class="home-tabs-bar"><div class="fs-tabbar"><div class="titlebar-brand"></div></div></div><aside class="home-sidebar"></aside>'
        document.body.append(probe)
        const titlebarColors = [...probe.querySelectorAll('.titlebar-brand,.home-sidebar')].map(
          (node) => window.getComputedStyle(node).backgroundColor
        )
        probe.remove()

        if (!checkSettingsSidebar) return { titlebarColors, sidebarColors: null }

        const sidebarProbe = document.createElement('div')
        sidebarProbe.className = 'fs-shell is-home-active'
        sidebarProbe.style.position = 'fixed'
        sidebarProbe.style.left = '-10000px'
        sidebarProbe.style.setProperty('--theme-sidebar-background', 'rgb(48, 48, 48)')
        sidebarProbe.style.setProperty('--surface-sidebar', 'rgb(24, 24, 24)')
        sidebarProbe.innerHTML =
          '<aside class="home-sidebar"></aside><div class="settings-modal"><aside class="connection-manager-sidebar"></aside></div>'
        document.body.append(sidebarProbe)
        const sidebarColors = [
          ...sidebarProbe.querySelectorAll('.home-sidebar,.settings-modal .connection-manager-sidebar')
        ].map((node) => window.getComputedStyle(node).backgroundColor)
        sidebarProbe.remove()
        if (!codexMacDark) return { titlebarColors, sidebarColors, codexSessionRailColors: null }

        const railProbe = document.createElement('div')
        railProbe.className = 'fs-shell is-home-active'
        railProbe.style.position = 'fixed'
        railProbe.style.left = '-10000px'
        railProbe.innerHTML =
          '<div class="fs-tabbar"><div class="titlebar-brand"></div></div><aside class="fs-sidebar"><div class="system-sidebar-layout"></div></aside><div class="settings-modal"><aside class="connection-manager-sidebar"></aside></div>'
        document.body.append(railProbe)
        const railNodes = [
          ...railProbe.querySelectorAll(
            '.fs-tabbar .titlebar-brand,.fs-sidebar,.system-sidebar-layout,.settings-modal .connection-manager-sidebar'
          )
        ]
        const root = document.documentElement
        const previousFocusState = root.getAttribute('data-window-focused')
        root.removeAttribute('data-window-focused')
        const activeColors = railNodes.map((node) => window.getComputedStyle(node).backgroundColor)
        root.setAttribute('data-window-focused', 'false')
        const inactiveColors = railNodes.map((node) => window.getComputedStyle(node).backgroundColor)
        if (previousFocusState === null) root.removeAttribute('data-window-focused')
        else root.setAttribute('data-window-focused', previousFocusState)
        railProbe.remove()
        return { titlebarColors, sidebarColors, codexSessionRailColors: { activeColors, inactiveColors } }
      },
      { checkSettingsSidebar, codexMacDark }
    )
    assert.equal(colors.titlebarColors[0], colors.titlebarColors[1], JSON.stringify(colors))
    if (checkSettingsSidebar) {
      if (codexMacDark) assert.equal(colors.sidebarColors[1], 'rgb(48, 48, 48)', JSON.stringify(colors))
      else assert.equal(colors.sidebarColors[0], colors.sidebarColors[1], JSON.stringify(colors))
    }
    if (codexMacDark) {
      assert.deepEqual(colors.codexSessionRailColors.activeColors, Array(4).fill('rgb(48, 48, 48)'))
      assert.deepEqual(colors.codexSessionRailColors.inactiveColors, Array(4).fill('rgb(27, 27, 27)'))
    }
  }
  const assertThemePickerPreview = async (variant) => {
    const preview = await page.evaluate((variant) => {
      const card = document.createElement('div')
      card.className = `theme-card ${variant}`
      card.style.position = 'fixed'
      card.style.left = '-10000px'
      card.innerHTML =
        '<div class="theme-card-preview"><div class="preview-header"><span class="dot dot-close"></span><span class="dot dot-min"></span><span class="dot dot-max"></span></div><div class="preview-body"><div class="preview-sidebar"></div><div class="preview-content"></div></div></div>'
      document.body.append(card)
      const header = card.querySelector('.preview-header')
      const sidebar = card.querySelector('.preview-sidebar')
      const headerStyle = window.getComputedStyle(header)
      const lineStyle = window.getComputedStyle(header, '::after')
      const sidebarStyle = window.getComputedStyle(sidebar)
      const headerRect = header.getBoundingClientRect()
      const lastDotRight = card.querySelector('.dot-max').getBoundingClientRect().right - headerRect.left
      const result = {
        header: headerStyle.backgroundColor,
        sidebar: sidebarStyle.backgroundColor,
        divider: headerStyle.borderBottomColor,
        sidebarDivider: sidebarStyle.borderRightColor,
        line: lineStyle.backgroundColor,
        lineGeometry: [lineStyle.top, lineStyle.bottom, lineStyle.left, lineStyle.width],
        trafficLightGap: Number.parseFloat(lineStyle.left) - lastDotRight,
        sidebarWidth: sidebar.getBoundingClientRect().width
      }
      card.remove()
      return result
    }, variant)
    assert.equal(preview.header, preview.sidebar, JSON.stringify(preview))
    assert.equal(preview.divider, preview.sidebarDivider, JSON.stringify(preview))
    assert.equal(preview.line, preview.divider, JSON.stringify(preview))
    assert.deepEqual(preview.lineGeometry, ['0px', '-1px', '33px', '1px'])
    assert.equal(preview.trafficLightGap, 4, JSON.stringify(preview))
    assert.equal(preview.sidebarWidth, 34, JSON.stringify(preview))
    assert.equal(
      Number.parseFloat(preview.lineGeometry[2]) + Number.parseFloat(preview.lineGeometry[3]),
      preview.sidebarWidth
    )
  }
  const assertFlatTheme = async () => {
    const colors = await page.evaluate(() => {
      const probe = document.createElement('div')
      probe.className = 'system-sidebar-layout'
      probe.style.position = 'fixed'
      probe.style.left = '-10000px'
      probe.innerHTML =
        '<div class="process-row"></div><div class="process-row"></div><div class="disk-row"></div><div class="disk-row"></div><table class="system-table"><tbody><tr><td>a</td></tr><tr><td>b</td></tr></tbody></table>'
      document.body.append(probe)
      const rows = [...probe.querySelectorAll('.process-row,.disk-row,tr')].map(
        (node) => window.getComputedStyle(node).backgroundColor
      )
      probe.remove()
      const headers = [
        '.file-split .pane-path-bar',
        '.file-split .pane-filter-bar',
        '.remote-directory-tree-header',
        '.remote-pane th'
      ].map((selector) => window.getComputedStyle(document.querySelector(selector)).backgroundColor)
      const headerStyle = window.getComputedStyle(document.querySelector('.remote-pane th'))
      return {
        rows,
        headers,
        panel: window.getComputedStyle(document.querySelector('.remote-directory-pane')).backgroundColor,
        headerDivider: {
          borderBottomWidth: headerStyle.borderBottomWidth,
          boxShadow: headerStyle.boxShadow
        }
      }
    })
    assert.ok(
      colors.rows.every((color) => color === colors.panel),
      JSON.stringify(colors)
    )
    assert.ok(
      colors.headers.every((color) => color === colors.headers[0]),
      JSON.stringify(colors)
    )
    assert.equal(colors.headers[0], colors.panel, JSON.stringify(colors))
    assert.equal(colors.headerDivider.borderBottomWidth, '0px', JSON.stringify(colors.headerDivider))
    assert.match(colors.headerDivider.boxShadow, /\) 0px 1px 0px 0px$/, JSON.stringify(colors.headerDivider))
  }
  if (process.argv.includes('--font-only')) {
    for (const preset of ['fileterm', 'codex', 'iterm2-gruvbox']) {
      await page.goto(`${address}/?platform=darwin&preset=${preset}`)
      await page.evaluate(() => {
        const preset = new URLSearchParams(window.location.search).get('preset')
        window.fixtureTheme(preset + '-light')
        window.fixtureStartPresetSync()
      })
      await page.waitForFunction(() => typeof window.fixtureSetFont === 'function')
      await page.evaluate(() => window.fixtureSetFont({ ui: 'Inter', code: 'JetBrains Mono' }))
      await page.waitForFunction(() => window.fixturePresetConfig.theme.fonts.ui === 'Inter')
      const config = await page.evaluate(() => window.fixturePresetConfig)
      assert.equal(config.codeThemeId, preset)
      assert.equal(config.theme.fonts.code, 'JetBrains Mono')
      const result = await page.evaluate((config) => {
        const before = window.getComputedStyle(document.documentElement).getPropertyValue('--ref-surface-panel')
        window.fixtureApplyConfig(config)
        return {
          before,
          after: window.getComputedStyle(document.documentElement).getPropertyValue('--ref-surface-panel'),
          font: window.getComputedStyle(document.documentElement).getPropertyValue('--font-ui')
        }
      }, config)
      assert.equal(result.before, result.after)
      assert.match(result.font, /Inter/)
      await page.evaluate(() => window.fixtureSwitchVariant('dark'))
      await page.waitForFunction(() => window.fixturePresetConfig.variant === 'dark')
      const dark = await page.evaluate(() => window.fixturePresetConfig)
      assert.equal(dark.codeThemeId, preset)
      assert.deepEqual(dark.theme.fonts, config.theme.fonts)
      const expected = await page.evaluate((preset) => window.fixturePresetConfigFor(preset, 'dark'), preset)
      assert.equal(dark.theme.surface, expected.theme.surface, preset)
      assert.deepEqual(dark.theme.semanticColors, expected.theme.semanticColors, preset)
      assert.deepEqual(dark.theme.terminal, expected.theme.terminal, preset)
      await page.evaluate(() => window.fixtureSwitchVariant('light'))
      await page.waitForFunction(() => window.fixturePresetConfig.variant === 'light')
      assert.deepEqual((await page.evaluate(() => window.fixturePresetConfig)).theme.fonts, config.theme.fonts)
    }
    await page.evaluate(() => window.fixtureSetThemeName('Font test'))
    await page.waitForFunction(() => window.fixturePresetConfig.variant === 'light')
    await page.evaluate(() => window.fixtureSaveTheme())
    await page.waitForFunction(() => window.fixtureSavedThemes.length === 1)
    const beforeSave = await page.evaluate(() => window.fixtureSavedThemes[0])
    await page.evaluate(() => window.fixtureSetFont({ ui: 'Noto Sans SC', code: 'Cascadia Code' }))
    await page.waitForFunction(() => window.fixturePresetConfig.theme.fonts.ui === 'Noto Sans SC')
    await page.evaluate(() => window.fixtureSaveTheme())
    await page.waitForFunction(() => window.fixtureSavedThemes[0]?.variants.dark.theme.fonts.ui === 'Noto Sans SC')
    const saved = await page.evaluate(() => window.fixtureSavedThemes[0])
    assert.deepEqual(saved.variants.dark.theme.fonts, { ui: 'Noto Sans SC', code: 'Cascadia Code' })
    assert.deepEqual(saved.variants.light.theme.fonts, saved.variants.dark.theme.fonts)
    assert.equal(saved.variants.dark.theme.surface, beforeSave.variants.dark.theme.surface)
    await page.evaluate(() => window.fixtureSwitchVariant('dark'))
    await page.waitForFunction(() => window.fixturePresetConfig.variant === 'dark')
    assert.deepEqual(
      (await page.evaluate(() => window.fixturePresetConfig)).theme.fonts,
      saved.variants.dark.theme.fonts
    )
  }
  for (const platform of process.argv.includes('--font-only') ? [] : ['darwin', 'win32', 'linux']) {
    await page.goto(`${address}/?platform=${platform}`)
    await page.evaluate((platform) => (document.documentElement.dataset.platform = platform), platform)
    await page.locator('.local-pane').waitFor()
    const localName = page.locator('.local-pane .file-name-cell').filter({ hasText: 'local.txt' })
    const remoteName = page.locator('.remote-pane .file-name-cell').filter({ hasText: 'note.txt' })
    const dragTo = async (source, target) => {
      const start = await source.boundingBox()
      const end = await target.boundingBox()
      await page.mouse.move(start.x + start.width / 2, start.y + start.height / 2)
      await page.mouse.down()
      await page.mouse.move(start.x + start.width / 2 + 10, start.y + start.height / 2, { steps: 3 })
      await page.mouse.move(end.x + end.width / 2, end.y + end.height / 2, { steps: 8 })
      await page.mouse.up()
    }
    await dragTo(localName.locator('.file-row-main'), page.locator('.remote-pane'))
    assert.deepEqual(
      await page.evaluate(() => window.fixtureTransfers.splice(0)),
      [{ direction: 'upload', paths: ['/Users/demo/local.txt'] }],
      `${platform}: drag file name to upload`
    )
    await dragTo(remoteName.locator('.file-row-main'), page.locator('.local-pane'))
    assert.deepEqual(
      await page.evaluate(() => window.fixtureTransfers.splice(0)),
      [{ direction: 'download', paths: ['/home/user/note.txt'], target: '/Users/demo' }],
      `${platform}: drag file name to download`
    )
    await dragTo(localName.locator('.file-icon'), page.locator('.remote-pane'))
    assert.equal(await page.evaluate(() => window.fixtureTransfers.splice(0).length), 1)
    await dragTo(localName, page.locator('.local-pane'))
    assert.deepEqual(await page.evaluate(() => window.fixtureTransfers), [], 'same pane is a no-op')
    await localName.click()
    await page
      .locator('.local-pane .file-name-cell')
      .filter({ hasText: 'Documents' })
      .click({ modifiers: ['ControlOrMeta'] })
    await dragTo(localName, page.locator('.remote-pane'))
    assert.deepEqual(
      await page.evaluate(() => window.fixtureTransfers.splice(0)),
      [{ direction: 'upload', paths: ['/Users/demo/Documents', '/Users/demo/local.txt'] }],
      'drag selected files together'
    )
    const start = await localName.boundingBox()
    await page.mouse.move(start.x + start.width / 2, start.y + start.height / 2)
    await page.mouse.down()
    await page.mouse.move(start.x + start.width / 2 + 15, start.y + start.height / 2)
    await page.locator('.file-drag-preview').waitFor()
    await page.evaluate(() => window.dispatchEvent(new window.PointerEvent('pointercancel', { pointerId: 1 })))
    await page.locator('.file-drag-preview').waitFor({ state: 'hidden' })
    await page.mouse.up()
    assert.equal(await page.locator('.file-drag-preview').count(), 0, 'cancel clears the preview')
    assert.deepEqual(await page.evaluate(() => window.fixtureTransfers), [], 'cancel never transfers files')
    await page.evaluate(() => window.fixtureConnect(false))
    await dragTo(localName, page.locator('.remote-pane'))
    assert.deepEqual(await page.evaluate(() => window.fixtureTransfers), [], 'disconnected panes reject transfers')
    if (process.argv.includes('--drag-only')) continue
    await page.reload()
    await page.evaluate((platform) => (document.documentElement.dataset.platform = platform), platform)
    await page.locator('.local-pane').waitFor()
    assert.deepEqual(await page.evaluate(() => window.fixtureLegacyTheme()), {
      surface: '#303030',
      override: '#303030',
      variant: 'light',
      hasSidebar: false
    })
    await page.locator('.file-pane-mode-select').click()
    await page.waitForTimeout(250)
    const controlMetrics = await page.evaluate(() => {
      const control = document.querySelector('.file-pane-mode-select')
      const rect = control.getBoundingClientRect()
      const toolbar = document.querySelector('.file-tabs').getBoundingClientRect()
      const user = document.querySelector('.file-tab-actions button[aria-pressed]').getBoundingClientRect()
      const style = window.getComputedStyle(control)
      return {
        height: rect.height,
        userHeight: user.height,
        top: rect.top,
        bottom: rect.bottom,
        toolbarTop: toolbar.top,
        toolbarBottom: toolbar.bottom,
        radius: style.borderRadius,
        borderTop: style.borderTopWidth,
        borderBottom: style.borderBottomWidth,
        background: style.backgroundColor
      }
    })
    assert.equal(controlMetrics.height, controlMetrics.userHeight)
    assert.ok(
      controlMetrics.top > controlMetrics.toolbarTop && controlMetrics.bottom < controlMetrics.toolbarBottom,
      JSON.stringify(controlMetrics)
    )
    assert.notEqual(controlMetrics.radius, '0px')
    assert.equal(controlMetrics.borderTop, '1px')
    assert.equal(controlMetrics.borderBottom, '1px')
    assert.equal(controlMetrics.background, 'rgb(26, 26, 26)')
    if (platform === 'darwin') await page.screenshot({ path: '/tmp/fileterm-mode-select-open.png' })
    await page.keyboard.press('Escape')
    await mode('remote-tree')
    await page.locator('[role="treeitem"][aria-label="user"]').waitFor()
    assert.equal(await page.locator('.local-pane').count(), 0)
    const activePath = () => page.locator('.file-split .pane-path-bar input').inputValue()
    assert.equal(await activePath(), '/home/user')
    const etc = page.locator('[role="treeitem"][aria-label="etc"]')
    await etc.locator('button').click()
    await page.locator('[role="treeitem"][aria-label="ssh"]').waitFor()
    assert.equal(await activePath(), '/home/user')
    await etc.locator('.remote-directory-name').click()
    await page.waitForFunction(() => document.querySelector('.file-split .pane-path-bar input')?.value === '/etc')
    assert.equal(await activePath(), '/etc')
    const restricted = page.locator('[role="treeitem"][aria-label="restricted"]')
    await restricted.locator('button').click()
    await page.locator('.remote-directory-error').waitFor()
    await page.evaluate(() => {
      window.fixtureFailRestricted = false
    })
    await page.locator('.remote-directory-error button').click()
    await page.locator('.remote-directory-error').waitFor({ state: 'hidden' })
    const rootRow = page.locator('[role="treeitem"][aria-label="/"]')
    await rootRow.locator('button').click()
    await page.locator('.remote-directory-locate').click()
    await etc.waitFor()
    await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label') === 'etc')
    await etc.press('ArrowRight')
    await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label') === 'ssh')
    await page.keyboard.press('Enter')
    await page.waitForFunction(() => document.querySelector('.file-split .pane-path-bar input')?.value === '/etc/ssh')
    assert.equal(await activePath(), '/etc/ssh')
    await page.reload()
    await page.locator('[role="treeitem"][aria-label="user"]').waitFor()
    assert.equal(await page.locator('.local-pane').count(), 0)
    await page.evaluate(() => window.fixtureConnect(false))
    await page.locator('.remote-directory-unavailable').waitFor()
    await page.evaluate(() => window.fixtureConnect(true))
    await page.locator('[role="treeitem"][aria-label="user"]').waitFor()
    await page.evaluate(() => window.fixtureAccess('root'))
    await page.waitForFunction(() => window.fixtureRequests.filter((path) => path === '/').length >= 3)
    for (const theme of ['fileterm-dark', 'fileterm-light', 'codex-dark', 'codex-light']) {
      await page.evaluate((theme) => window.fixtureTheme(theme), theme)
      await page.waitForTimeout(250)
      await assertSidebarCorner(!theme.startsWith('fileterm-'), platform === 'darwin' && theme === 'codex-dark')
      await assertThemePickerPreview(theme.endsWith('light') ? 'light' : 'dark')
      const rowColors = await page
        .locator('.remote-pane tbody tr')
        .evaluateAll((rows) => rows.slice(0, 2).map((row) => window.getComputedStyle(row).backgroundColor))
      if (theme.startsWith('fileterm')) assert.notEqual(rowColors[0], rowColors[1])
      else {
        assert.equal(rowColors[0], rowColors[1])
        await assertFlatTheme()
        assert.equal(
          rowColors[0],
          await page.locator('.remote-directory-pane').evaluate((node) => window.getComputedStyle(node).backgroundColor)
        )
      }
      for (const width of [1100, 720]) {
        await page.setViewportSize({ width, height: 560 })
        const metrics = await page.evaluate(() => {
          const tree = document.querySelector('.remote-directory-row').getBoundingClientRect()
          const table = document
            .querySelector('.remote-pane tbody tr:not(.file-virtual-spacer)')
            .getBoundingClientRect()
          const header = document.querySelector('.file-remote-shared-header').getBoundingClientRect()
          const split = document.querySelector('.file-split').getBoundingClientRect()
          return {
            treeTop: tree.top,
            tableTop: table.top,
            treeHeight: tree.height,
            tableHeight: table.height,
            headerWidth: header.width,
            splitWidth: split.width
          }
        })
        assert.equal(metrics.treeHeight, 24)
        assert.equal(metrics.tableHeight, 24)
        assert.ok(Math.abs(metrics.treeTop - metrics.tableTop) <= 1, JSON.stringify(metrics))
        assert.equal(metrics.headerWidth, metrics.splitWidth)
      }
      await page.evaluate((theme) => window.fixtureSurfaceColors(theme), theme)
      await page.waitForTimeout(250)
      const colors = await page.evaluate(() => {
        const tokens = window.getComputedStyle(document.documentElement)
        return [
          '--ref-surface-sidebar',
          '--ref-surface-toolbar',
          '--ref-surface-input',
          '--ref-surface-context-menu',
          '--ref-surface-elevated'
        ].map((key) => tokens.getPropertyValue(key).trim())
      })
      assert.deepEqual(colors, ['#123456', '#123456', '#234567', '#345678', '#303030'])
      await page.locator('.file-pane-mode-select').click()
      await page.waitForTimeout(250)
      assert.equal(
        await page.locator('.dropdown-select-menu').evaluate((node) => window.getComputedStyle(node).backgroundColor),
        'rgb(52, 86, 120)'
      )
      await page.keyboard.press('Escape')
      await page.evaluate((theme) => window.fixtureTheme(theme), theme)
      await page.setViewportSize({ width: 1100, height: 560 })
      await page.waitForTimeout(250)
      if (platform === 'darwin') await page.screenshot({ path: `/tmp/fileterm-directory-tree-${theme}.png` })
    }
    if (platform === 'darwin') {
      for (const preset of ['dracula', 'nord', 'gruvbox', 'catppuccin', 'solarized', 'tokyonight']) {
        for (const variant of ['dark', 'light']) {
          await page.evaluate((mode) => window.fixtureTheme(mode), 'iterm2-' + preset + '-' + variant)
          await page.waitForTimeout(250)
          await assertSidebarCorner(true)
          await assertThemePickerPreview(variant)
          const rowColors = await page
            .locator('.remote-pane tbody tr')
            .evaluateAll((rows) => rows.slice(0, 2).map((row) => window.getComputedStyle(row).backgroundColor))
          assert.equal(rowColors[0], rowColors[1], preset + '-' + variant)
          await assertFlatTheme()
          if (variant === 'light') {
            const palette = await page.evaluate((id) => window.fixturePresetRead(id, 'light'), 'iterm2-' + preset)
            for (const key of ['surfaceSecondary', 'surfaceElevated', 'surfaceInput', 'surfaceContextMenu']) {
              assert.match(palette[key], /^#[\dA-F]{6}$/)
              assert.notEqual(palette[key], '#FFFFFF', preset + ' ' + key)
            }
            if (preset === 'solarized')
              assert.deepEqual(
                [palette.surfaceSecondary, palette.surfaceInput, palette.surfaceContextMenu],
                ['#F4EDDC', '#F2EAD6', '#EEE5CF']
              )
            if (preset === 'gruvbox')
              assert.deepEqual(
                [palette.surfaceSecondary, palette.surfaceInput, palette.surfaceContextMenu],
                ['#F2E8C9', '#EADFBD', '#E5D8B2']
              )
            if (preset === 'solarized' || preset === 'gruvbox')
              await page.screenshot({ path: '/tmp/fileterm-' + preset + '-light-refined.png' })
          }
          assert.equal(
            rowColors[0],
            await page
              .locator('.remote-directory-pane')
              .evaluate((node) => window.getComputedStyle(node).backgroundColor)
          )
        }
      }
      await page.evaluate(() => {
        window.fixtureTheme('iterm2-gruvbox-light')
        window.fixtureThemeAttributeMutations = []
        window.fixtureThemeAttributeObserver = new window.MutationObserver((records) => {
          window.fixtureThemeAttributeMutations.push(...records.map((record) => record.attributeName))
        })
        window.fixtureThemeAttributeObserver.observe(document.documentElement, {
          attributes: true,
          attributeFilter: ['data-theme', 'data-theme-preset']
        })
        window.fixtureStartPresetSync()
      })
      await page.waitForFunction(() => typeof window.fixturePresetSyncRerender === 'function')
      await page.waitForTimeout(100)
      await page.evaluate(() => window.fixturePresetSyncRerender())
      await page.waitForTimeout(100)
      assert.equal(
        await page.evaluate(
          () => window.fixtureThemeAttributeMutations.filter((name) => name === 'data-theme-preset').length
        ),
        2,
        'preset settings sync should read the live theme only once after a rerender'
      )
      await page.evaluate(() => window.fixtureThemeAttributeObserver.disconnect())
    }
    await page.evaluate(() => document.documentElement.style.setProperty('--folder-accent', 'rgb(160, 80, 210)'))
    assert.equal(
      await page
        .locator('.remote-directory-row > .app-icon')
        .first()
        .evaluate((node) => window.getComputedStyle(node).color),
      'rgb(160, 80, 210)'
    )
    await mode('local')
    await page.locator('.local-pane').waitFor()
  }
  assert.deepEqual(errors, [])
  console.log(
    process.argv.includes('--font-only')
      ? 'Passed: font selection preserves theme identity and saved custom fonts across light/dark switches.'
      : process.argv.includes('--drag-only')
        ? 'Passed: name/icon uploads, name downloads, multi-selection, same-pane/cancelled/disconnected drops; macOS/Windows/Linux renderer branches.'
        : 'Passed: file dragging, layout modes, lazy expansion, navigation, errors/retry, keyboard/locate, persistence, reconnect and access-mode isolation; macOS/Windows/Linux controls and dark/light themes.'
  )
} finally {
  await browser.close()
  await new Promise((resolve) => server.close(resolve))
  rmSync(directory, { recursive: true, force: true })
}
