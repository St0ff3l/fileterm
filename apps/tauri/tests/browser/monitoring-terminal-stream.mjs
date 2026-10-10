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
const directory = mkdtempSync(join(tmpdir(), 'fileterm-monitoring-stream-'))
await build({
  stdin: {
    resolveDir: fileURLToPath(new URL('../../../../', import.meta.url)),
    loader: 'tsx',
    contents: `
import { createRoot } from 'react-dom/client'
import { Terminal } from '@xterm/xterm'
import { useAppShellState } from './apps/tauri/src/renderer/hooks/use-app-shell-state'
import { emptyState } from './apps/tauri/src/renderer/app/app-data'
import { SystemSidebar } from './apps/tauri/src/renderer/features/system/system-sidebar'
import { TerminalView } from './apps/tauri/src/renderer/components/terminal-view'
import { installRendererPerformanceLogging } from './apps/tauri/src/renderer/lib/renderer-performance-log'
import './apps/tauri/src/renderer/styles/index.css'
const metrics={cpuPercent:8,memoryPercent:10,swapPercent:0,networkInterfaces:['all'],activeNetworkInterface:'all',networkSamples:[],fileSystemRows:[]}
const monitoring={generation:1,revision:1,phase:'healthy',attempt:0,maxAttempts:5,intervalSeconds:1}
const session={profileId:'p',connected:true,remotePath:'/',remoteFiles:[],capabilities:{resourceMonitoring:true},monitoring,systemMetrics:metrics}
const snapshot={...emptyState,sessions:{tab:session},tabs:[{id:'tab',profileId:'p',title:'fixture',sessionType:'ssh',status:'connected',layout:'terminal-file'}],activeTabId:'tab'}
window.received=0;window.parsed=0;window.listeners={};window.performanceReports=[];
const write=Terminal.prototype.write
Terminal.prototype.write=function(data,callback){
  return write.call(this,data,()=>{window.parsed+=data.length;callback?.()})
}
const noop=()=>()=>{}
window.fileterm={platform:'darwin',getSnapshot:async()=>snapshot,
 getUiStateItem:async()=>null,setUiStateItem:async()=>{},setUiPreferences:async()=>{},
 getUiPreferences:async()=>({terminalZoomLocked:false}),
 getSecuritySettings:async()=>({lockEnabled:false,idleLockMinutes:0}),
 listImportedFonts:async()=>[],listLocalDirectory:async()=>({path:'/tmp',items:[]}),isCurrentWindowMaximized:async()=>false,
 writeDiagnosticLog:async(...args)=>{if(args[1]==='renderer:performance')window.performanceReports.push(args)},resizeTerminal:async()=>{},
 onWorkspaceSnapshot:async()=>()=>{},onUiPreferencesChanged:noop,onSecuritySettingsChanged:noop,
 onWindowMaximizedChange:noop,onWindowCloseRequest:noop,onRequestCloseActiveWorkspaceItem:noop,
 onNewTabRequest:noop,onSplitPaneRequest:noop,onFocusPaneRequest:noop,onActionApprovalRequest:noop,
 onTransferUpdate:noop,onRemoteFilesChanged:noop,onTerminalState:noop,
 onTerminalZoomRequest:noop,onTerminalGestureZoomRequest:noop,
 onSessionMetrics:(callback)=>{window.listeners.metrics=callback;return ()=>{}},
 onTerminalData:(callback)=>{window.listeners.terminal=callback;return ()=>{}}
}
window.stopPerformanceLogging=installRendererPerformanceLogging()
window.emitSample=(revision)=>{
 window.received=revision
 window.listeners.metrics({tabId:'tab',monitoring:{...monitoring,revision},mode:'append',systemMetrics:{...metrics,cpuPercent:revision%100,networkSamples:[{rx:revision,tx:revision,sampledAt:revision}]}})
}
function Fixture(){
 const shell=useAppShellState({searchParams:new URLSearchParams(),initialUiPreferences:{},desktopApi:window.fileterm,isConnectionFormWindow:false,isMainWorkspaceWindow:true,isConnectionManagerWindow:false,rendererPlatform:'darwin'})
 const current=shell.workspace.sessions.tab
 if(!current)return null
 return <div style={{display:'flex',height:700,width:1000}}>
  <output id="revision">{current.monitoring.revision}</output>
  <aside className="fs-sidebar" style={{width:214}}><SystemSidebar activeProfile={null} activeSession={current} activeTabId="tab" collapsed={false} connectionStatus="connected" showResourceMeters visibleMetrics={['cpu','network']} onMonitoringSnapshot={shell.applySnapshot} onOpenSystemInfo={()=>{}} onToggleCollapsed={()=>{}}/></aside>
  <div style={{flex:1,minWidth:0}}><TerminalView profileId="p" tabId="tab" sessionType="ssh" connected bootText="" onActivate={()=>{}} onStatus={()=>{}}/></div>
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
const browser = await engine.launch({
  headless: true,
  channel: process.env.PLAYWRIGHT_CHANNEL,
  executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH
})
try {
  const page = await browser.newPage({ viewport: { width: 1100, height: 800 } })
  page.setDefaultTimeout(10_000)
  const errors = []
  page.on('pageerror', (error) => {
    errors.push(error.message)
    console.error(error.message)
  })
  await page.setContent(
    '<html data-platform="darwin" data-theme="fileterm-dark"><body><div id="root"></div></body></html>'
  )
  await page.addStyleTag({ content: readFileSync(join(directory, 'bundle.css'), 'utf8') })
  await page.addScriptTag({ content: readFileSync(join(directory, 'bundle.js'), 'utf8') })
  await page.waitForFunction(() => window.listeners.metrics && window.listeners.terminal)
  await page.evaluate(() => window.emitSample(2))
  await page.waitForFunction(() => document.querySelector('#revision').textContent === '2')
  const paths = await page
    .locator('.network-path')
    .evaluateAll((elements) => elements.map((element) => element.getAttribute('d')))
  assert.equal(paths.length, 2)
  for (const path of paths)
    assert.equal((path.match(/M /g) ?? []).length, 1, 'initial baseline must join the first live sample')
  // Only timers deliver data: no keyboard/mouse event may wake these updates.
  await page.evaluate(() => {
    let revision = 2
    window.sampleTimer = window.setInterval(() => window.emitSample(++revision), 30)
  })
  await page.waitForFunction(() => Number(document.querySelector('#revision').textContent) >= 100)
  await page.evaluate(() => window.clearInterval(window.sampleTimer))
  await page.waitForFunction(() => Number(document.querySelector('#revision').textContent) === window.received)

  // Model an occluded/background WebView: parsing must continue without RAF.
  await page.evaluate(() => {
    window.requestAnimationFrame = () => 0
    window.expectedParsed = window.parsed
    for (let index = 0; index < 200; index++) {
      const chunk = 'stream output\r\n'.repeat(100)
      window.expectedParsed += chunk.length
      window.listeners.terminal({ tabId: 'tab', chunk })
    }
    window.emitSample(101)
  })
  await page.waitForFunction(() => window.parsed === window.expectedParsed, undefined, { polling: 50 })
  await page.waitForFunction(() => document.querySelector('#revision').textContent === '101', undefined, {
    polling: 50
  })
  const previousReports = await page.evaluate(() => window.performanceReports.length)
  await page.evaluate(() => {
    const started = performance.now()
    while (performance.now() - started < 4200) {
      // Deliberately block without throwing to exercise the recovered-stall diagnostic.
    }
  })
  await page.waitForFunction((previous) => window.performanceReports.length > previous, previousReports, {
    polling: 50
  })
  const report = await page.evaluate(() => window.performanceReports.at(-1))
  assert.equal(report[0], 'WARN')
  assert.equal(JSON.parse(report[2]).event, 'event-loop-gap')
  await page.evaluate(() => window.stopPerformanceLogging())
  assert.deepEqual(errors, [])
  console.log('PASS: autonomous metrics; output drains without RAF; recovered busy loop logs WARN without an exception')
} finally {
  await browser.close()
  rmSync(directory, { recursive: true, force: true })
}
