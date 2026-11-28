/* global window, document, getComputedStyle, innerWidth */
import assert from 'node:assert/strict'
import { inspectFocusShadow } from './helpers/focus-shadow-clipping.mjs'
import ts from 'typescript'
import { execFileSync } from 'node:child_process'
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
const directory = realpathSync(mkdtempSync(join(tmpdir(), 'fileterm-button-ui-')))
const root = fileURLToPath(new URL('../../../../', import.meta.url))
writeFileSync(
  join(directory, 'index.html'),
  '<html><head><style>html,body,#root{height:100%;margin:0}</style></head><body><div id="root"></div><script type="module" src="/fixture.tsx"></script></body></html>'
)
// Read actual call-site labels/classes so new confirmation consumers join this regression automatically.
const renderer = 'apps/tauri/src/renderer/'
const files = execFileSync('rg', ['--files', `${root}${renderer}`], { encoding: 'utf8' })
  .trim()
  .split('\n')
  .filter((file) => file.endsWith('.tsx'))
const cases = []
for (const file of files) {
  const source = readFileSync(file, 'utf8')
  const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX)
  const keys = (text = '') => [...text.matchAll(/\bt\.(\w+)/g)].map((match) => match[1])
  const add = (node, attributes) => {
    const labels = keys(attributes.confirmLabel)
    for (const label of labels)
      cases.push({
        id: `${file.slice(root.length)}:${ast.getLineAndCharacterOfPosition(node.pos).line + 1}:${label}`,
        label,
        title: keys(attributes.title)[0] || 'confirm',
        description: keys(attributes.description)[0] || 'closeConfirmQuitMsg',
        className: /^['"]/.test(attributes.className || '') ? attributes.className.slice(1, -1) : '',
        variant: (attributes.confirmVariant || '').includes('primary') ? 'primary' : 'danger'
      })
  }
  function walk(node) {
    if (
      (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) &&
      node.tagName.getText(ast) === 'ConfirmActionDialog'
    ) {
      const attrs = Object.fromEntries(
        node.attributes.properties
          .filter(ts.isJsxAttribute)
          .map((attr) => [attr.name.getText(ast), attr.initializer?.getText(ast)])
      )
      add(node, attrs)
    }
    // Layout forwards objects through ModalPortalManager rather than direct JSX props.
    if (file.endsWith('/app-modal-portals.tsx') && ts.isObjectLiteralExpression(node)) {
      const attrs = Object.fromEntries(
        node.properties
          .filter(ts.isPropertyAssignment)
          .map((prop) => [prop.name.getText(ast), prop.initializer.getText(ast)])
      )
      if (attrs.confirmLabel) add(node, attrs)
    }
    ts.forEachChild(node, walk)
  }
  walk(ast)
}
assert.ok(cases.length >= 30, 'confirmation call-site inventory must include forwarded layout bindings')
writeFileSync(
  join(directory, 'fixture.tsx'),
  `
import React from 'react';
import {createRoot} from 'react-dom/client';
import {flushSync} from 'react-dom';
import {Button} from '${root}${renderer}components/common/button/button.tsx';
import {ConfirmActionDialog} from '${root}${renderer}components/common/confirm-action-dialog/confirm-action-dialog.tsx';
import {SidebarToggle} from '${root}${renderer}components/common/sidebar-toggle/sidebar-toggle.tsx';
import {MonitoringOverlay} from '${root}${renderer}features/system/monitoring-overlay.tsx';
import {MonitoringToggle} from '${root}${renderer}features/system/monitoring-toggle.tsx';
import '${root}${renderer}features/system/system-sidebar-controls.css';
import {PathBookmarksButton} from '${root}${renderer}features/files/path-bookmarks-button.tsx';
import {FileActionModal} from '${root}${renderer}features/files/file-action-modal.tsx';
import {AiCopilotConversationList} from '${root}${renderer}features/ai/ai-copilot-conversation-list.tsx';
import {ConnectionModal} from '${root}${renderer}features/connections/connection-modal.tsx';
import {defaultForm} from '${root}${renderer}app/app-data.ts';
import {ProxyEditDialog} from '${root}${renderer}features/proxies/proxy-edit-dialog.tsx';
import {TunnelEditDialog} from '${root}${renderer}features/tunnels/tunnel-edit-dialog.tsx';
import {SettingsModal} from '${root}${renderer}features/settings/settings-modal/index.tsx';

import {RootAccessModal} from '${root}${renderer}features/files/root-access-modal.tsx';
import {BackupPasswordModal} from '${root}${renderer}features/connections/backup-password-modal.tsx';
import {SshKeyPassphraseModal} from '${root}${renderer}features/connections/ssh-key-passphrase-modal.tsx';
import {SshKeyboardInteractiveModal} from '${root}${renderer}features/connections/ssh-keyboard-interactive-modal.tsx';
import {SshHostVerificationModal} from '${root}${renderer}features/connections/ssh-host-verification-modal.tsx';
import {SmbCredentialsModal} from '${root}${renderer}features/files/smb-credentials-modal.tsx';
import {SmbSharePickerModal} from '${root}${renderer}features/files/smb-share-picker-modal.tsx';
import {FilePermissionModal} from '${root}${renderer}features/files/file-permission-modal.tsx';
import {SshKeyNoteDialog} from '${root}${renderer}features/ssh-keys/ssh-key-note-dialog.tsx';
import {ExternalOperationConfirmation} from '${root}${renderer}features/connections/external-operation-confirmation.tsx';
import {PortableUpdateDialog} from '${root}${renderer}features/common/portable-update-dialog.tsx';
import {applyThemeVariables} from '${root}${renderer}app/theme-config.ts';
import {createDefaultThemeConfig} from '${root}packages/core/src/index.ts';
import {setLocale,t} from '${root}${renderer}i18n.ts';
import '${root}${renderer}styles/index.css';
const root=createRoot(document.getElementById('root'));
const noop=()=>{};
window.calls=[];
const bookmarks=[{id:'1',scope:'local',path:'/srv/config',name:'config',type:'folder'},{id:'2',scope:'local',path:'/srv/data',name:'data',type:'folder'}];
window.fileterm={platform:'browser',retryMonitoring:()=>{window.calls.push('retry');return new Promise((resolve,reject)=>window.finishRequest=(fail=false)=>fail?reject(new Error('failed')):resolve({}))},setMonitoringEnabled:()=>{window.calls.push('toggle');return new Promise(resolve=>window.finishRequest=()=>resolve({}))},getPathBookmarks:async()=>window.bookmarkItems??bookmarks,updatePathBookmark:async(scope,action,item)=>{
window.calls.push(['bookmark',action,item.path]);
if(window.holdBookmarks){await new Promise((resolve,reject)=>window.finishBookmarks=(fail=false)=>fail?reject(new Error('Bookmark request failed')):resolve())}
const items=[...(window.bookmarkItems??bookmarks)];const index=items.findIndex(entry=>entry.id===item.id);
if(action==='add')items.push({...item,id:'added'});
else if(action==='remove')items.splice(index,1);
else {const next=index+(action==='up'?-1:1);[items[index],items[next]]=[items[next],items[index]]}
window.bookmarkItems=items;return items
}};
const fixtureApi=window.fileterm;
window.renderCase=({kind='confirm',model={},busy=false,error=false,locale='zhCN',theme='fileterm-dark',extra=false})=>{
window.fileterm=kind==='settings'?undefined:fixtureApi;
setLocale(locale);if(theme==='custom'){const config=createDefaultThemeConfig('dark');config.theme.accent='#8b5cf6';applyThemeVariables('fileterm-dark',config)}else applyThemeVariables(theme);
const shared={isSubmitting:busy,onClose:()=>window.calls.push('close'),onConfirm:()=>window.calls.push('confirm'),errorMessage:error?'Request failed. Please try again.':null};
let content;
if(kind==='confirm')content=<ConfirmActionDialog {...shared} className={model.className} confirmVariant={model.variant} title={t[model.title]} description={t[model.description]} confirmLabel={t[model.label]} extraActions={extra?<button className="confirm-action-dialog__button confirm-action-dialog__button--primary" onClick={noop}>{t.closeConfirmHide}</button>:null}/>;
else if(kind==='file-action')content=<FileActionModal {...shared} title={t[model.title||'rename']} confirmLabel={t.confirm} inputLabel={t.fileName} initialValue="config.json" hint={model.hint?t.newFileExtensionHint:undefined}/>;
else if(kind==='root')content=<RootAccessModal {...shared} defaultSshUser="Stoffel" onSubmit={input=>window.calls.push(['root',input])}/>;
else if(kind==='backup')content=<BackupPasswordModal {...shared} request={{requestId:'backup',operation:'upload',provider:'WebDAV'}} onCancel={noop} onSubmit={value=>window.calls.push(['backup',value])}/>;
else if(kind==='passphrase')content=<SshKeyPassphraseModal {...shared} request={{requestId:'key',connectionName:'Production',keyName:'id_ed25519',reason:'required'}} onCancel={noop} onSubmit={value=>window.calls.push(['passphrase',value])}/>;
else if(kind==='keyboard')content=<SshKeyboardInteractiveModal {...shared} request={{requestId:'auth',host:'server.example',port:22,connectionName:'Production',round:1,prompts:model.many?Array.from({length:20},(_,i)=>({prompt:'Verification '+i,echo:false})):[{prompt:'Verification code',echo:false}]}} onCancel={noop} onSubmit={value=>window.calls.push(['keyboard',value])}/>;
else if(kind==='host')content=<SshHostVerificationModal {...shared} request={{requestId:'host',host:'server.example',port:22,connectionName:'Production',fingerprint:'SHA256:example',knownFingerprint:'SHA256:previous'}} onReject={noop} onAcceptOnce={noop} onAcceptAndSave={noop}/>;
else if(kind==='smb')content=<SmbCredentialsModal {...shared} path="smb://server/share" onCancel={noop} onSubmit={value=>window.calls.push(['smb',value])}/>;
else if(kind==='shares')content=<SmbSharePickerModal {...shared} path="smb://server" shares={['documents','backup']} onCancel={noop} onChangeCredentials={noop} onSubmit={value=>window.calls.push(['share',value])}/>;
else if(kind==='ai-history')content=<AiCopilotConversationList conversation={null} conversationSearch="" filteredConversations={[]} isStreaming={false} loadConversation={async()=>{}} onCloseConversationList={noop} onDeleteConversation={noop} openNewConversation={noop} setConversationSearch={noop}/>;
else if(kind==='connection-editor')content=<ConnectionModal standalone mode="create" form={{...defaultForm}} setForm={noop} connectionDefaults={{}} groupOptions={[]} errorMessage={null} onClose={noop} onSubmit={noop} onTestConnection={async()=>true}/>;
else if(kind==='proxy-editor')content=<ProxyEditDialog isOpen isSubmitting={false} onClose={noop} onSave={async()=>{}} onTest={async()=>({success:true})}/>;
else if(kind==='tunnel-editor')content=<TunnelEditDialog isOpen isSubmitting={false} onClose={noop} onSave={async()=>{}}/>;
else if(kind==='settings')content=<SettingsModal theme={theme} themeConfig={createDefaultThemeConfig(theme.endsWith('light')?'light':'dark')} customThemes={[]} locale={locale} onSetTheme={noop} onSetThemeConfig={noop} onSetCustomThemes={noop} onSetLocale={noop} onOpenLogsDirectory={noop} onClose={noop}/>;
else if(kind==='permission')content=<FilePermissionModal {...shared} fileName="config.json" fileType="file" initialPermission="-rw-r--r--" supportsRecursive={false} targetPath="/srv/config.json" onSubmit={noop}/>;
else if(kind==='ssh-key')content=<SshKeyNoteDialog {...shared} mode="edit" initialNote="production" onSubmit={noop}/>;
else if(kind==='approval')content=<ExternalOperationConfirmation isSubmitting={busy} riskAcknowledged={true} onRiskAcknowledged={noop} onResolve={noop} request={{source:'cli',requestId:'test',title:'Confirm command',summary:'Execute command on production',destructive:true,details:'ls /srv',requiresRiskAcknowledgement:true}}/>;
else if(kind==='update')content=<PortableUpdateDialog onClose={noop} onOpenReleasePage={noop}/>;
else if(kind==='monitor')content=<div className="system-sidebar-layout" style={{width:280,height:440,position:'relative'}}><MonitoringToggle tabId="a" session={{connected:true,monitoring:{phase:'stopped',generation:1}}} onSnapshot={noop}/><MonitoringOverlay tabId="a" session={{connected:true,monitoring:{phase:'failed',generation:1,attempt:3,maxAttempts:3}}} collapsed={false} onSnapshot={noop}/></div>;
else if(kind==='bookmarks')content=<PathBookmarksButton scope="local" path={model.path||"/srv"} onOpen={item=>window.calls.push(['open-bookmark',item.path])}/>;
else if(kind==='sidebar')content=<SidebarToggle collapsed={false} label="Toggle sidebar" onClick={noop}/>;
else if(kind==='buttons')content=<div style={{display:'flex',gap:12,padding:24}}>{['primary','secondary','danger','ghost'].map(variant=><Button key={variant} variant={variant} loading={busy} reserveLoadingSpace={model.reserve} disabled={model.disabled} onClick={()=>window.calls.push('button')}>Action</Button>)}</div>;
flushSync(()=>root.render(<React.Fragment key={kind}>{content}</React.Fragment>));
};
window.renderCase({});
`
)
const baselineFiles = [
  'components/common/button/button.tsx',
  'components/common/button/button.css',
  'components/common/confirm-action-dialog/confirm-action-dialog.tsx',
  'components/common/confirm-action-dialog/confirm-action-dialog.css',
  'features/files/file-action-modal.tsx',
  'features/files/file-permission-modal.tsx',
  'styles/features/modal-components.css'
]
const baseline = new Map(
  baselineFiles.map((file) => [
    `${root}${renderer}${file}`,
    execFileSync('git', ['show', `HEAD:${renderer}${file}`], { cwd: root, encoding: 'utf8' })
  ])
)
for (const original of [false, true])
  await build({
    configFile: false,
    root: directory,
    base: original ? '/baseline/' : '/',
    logLevel: 'error',
    plugins: [
      ...(original
        ? [
            {
              name: 'baseline',
              enforce: 'pre',
              load(id) {
                return baseline.get(id)
              }
            }
          ]
        : []),
      react()
    ],
    resolve: {
      alias: [
        { find: /^react$/, replacement: require.resolve('react') },
        { find: /^react-dom$/, replacement: require.resolve('react-dom') },
        { find: /^react\/jsx-runtime$/, replacement: require.resolve('react/jsx-runtime') },
        { find: /^react-dom\/client$/, replacement: require.resolve('react-dom/client') }
      ]
    },
    build: { minify: false, outDir: join(directory, original ? 'baseline' : 'dist') }
  })
const server = createServer((request, response) => {
  try {
    const path = new URL(request.url, 'http://localhost').pathname
    const isBaseline = path.startsWith('/baseline/')
    const relative = isBaseline ? path.slice('/baseline'.length) : path
    const data = readFileSync(
      join(directory, isBaseline ? 'baseline' : 'dist', relative === '/' ? 'index.html' : relative)
    )
    const type =
      {
        '.js': 'text/javascript',
        '.css': 'text/css',
        '.html': 'text/html',
        '.ttf': 'font/ttf',
        '.woff2': 'font/woff2',
        '.svg': 'image/svg+xml'
      }[extname(relative)] || 'application/octet-stream'
    response.writeHead(200, { 'Content-Type': relative === '/' ? 'text/html' : type })
    response.end(data)
  } catch {
    response.writeHead(404)
    response.end()
  }
})
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve))
let browser
try {
  browser = await chromium.launch({ headless: true, channel: process.env.PLAYWRIGHT_CHANNEL })
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } })
  const before = await browser.newPage({ viewport: { width: 1280, height: 900 } })
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  const base = `http://127.0.0.1:${server.address().port}`
  await Promise.all([page.goto(base), before.goto(base + '/baseline/')])
  await Promise.all([page.evaluate(() => document.fonts.ready), before.evaluate(() => document.fonts.ready)])
  const render = async (target, props) => {
    await target.evaluate((props) => window.renderCase(props), props)
    await target.evaluate(() => document.fonts.ready)
  }
  const geometry = (target) =>
    target.locator('.form-actions button').evaluateAll((buttons) =>
      buttons.map((button) => {
        const r = button.getBoundingClientRect(),
          s = getComputedStyle(button)
        return {
          width: r.width,
          height: r.height,
          paddingLeft: s.paddingLeft,
          paddingRight: s.paddingRight,
          radius: s.borderRadius,
          fontSize: s.fontSize,
          fontWeight: s.fontWeight,
          lineHeight: s.lineHeight
        }
      })
    )
  // Compare normal geometry against pre-change source, not just the new busy state.
  for (const locale of ['zhCN', 'enUS']) {
    for (const theme of ['fileterm-dark', 'fileterm-light', 'codex-dark', 'codex-light', 'custom']) {
      for (const model of cases) {
        const props = { model, locale, theme }
        await Promise.all([render(page, props), render(before, props)])
        assert.deepEqual(await geometry(page), await geometry(before), `idle baseline ${model.id} ${locale} ${theme}`)
        const idle = await page.locator('.form-actions').boundingBox()
        await render(page, { ...props, busy: true })
        assert.deepEqual(await page.locator('.form-actions').boundingBox(), idle, `busy layout ${model.id}`)
        assert.equal(await page.locator('.form-actions button:enabled').count(), 0)
        await page.locator('.confirm-action-dialog__button--' + model.variant).dispatchEvent('click')
        assert.equal(await page.evaluate(() => window.calls.filter((c) => c === 'confirm').length), 0)
        await render(page, props)
        assert.deepEqual(await page.locator('.form-actions').boundingBox(), idle)
      }
    }
  }
  for (const locale of ['zhCN', 'enUS'])
    for (const theme of ['fileterm-dark', 'fileterm-light', 'codex-dark', 'codex-light', 'custom'])
      for (const kind of ['file-action', 'permission']) {
        await Promise.all([render(page, { kind, locale, theme }), render(before, { kind, locale, theme })])
        assert.deepEqual(
          await geometry(page),
          await geometry(before),
          `${kind} preserves pre-migration geometry ${locale} ${theme}`
        )
      }
  for (const title of ['rename', 'newFolder', 'newFile']) {
    const props = { kind: 'file-action', model: { title, hint: title === 'newFile' } }
    await Promise.all([render(page, props), render(before, props)])
    const current = await page.locator('.file-action-modal').boundingBox()
    const previous = await before.locator('.file-action-modal').boundingBox()
    assert.ok(
      current.width <= previous.width && current.height > 190 && current.height < 360,
      'file action dialog must use moderate framed spacing'
    )
    assert.equal(
      await page.locator('.file-action-field input').evaluate((node) => getComputedStyle(node).borderRadius),
      '4px'
    )
    assert.equal(await page.getByRole('textbox', { name: '文件名', exact: true }).count(), 1)
    assert.equal(await page.locator('.file-action-modal').getByText('文件名', { exact: true }).count(), 1)
    assert.equal(
      await page.locator('.file-action-modal fieldset').count(),
      0,
      'single filename input must not have a group border'
    )
    if (process.env.FILETERM_UI_ARTIFACT_DIR)
      await page.screenshot({ path: join(process.env.FILETERM_UI_ARTIFACT_DIR, `fileterm-compact-${title}.png`) })
  }
  const bounds = (target, selector) =>
    target.locator(selector).evaluateAll((nodes) =>
      nodes.map((node) => {
        const { x, y, width, height } = node.getBoundingClientRect()
        return { x, y, width, height }
      })
    )
  for (const locale of ['zhCN', 'enUS'])
    for (const width of [1280, 420]) {
      await page.setViewportSize({ width, height: 900 })
      for (const kind of ['file-action', 'permission', 'ssh-key', 'approval', 'update']) {
        const props = { kind, locale, theme: width === 1280 ? 'fileterm-dark' : 'fileterm-light' }
        await render(page, props)
        if (process.env.FILETERM_UI_ARTIFACT_DIR)
          await page.screenshot({
            path: join(process.env.FILETERM_UI_ARTIFACT_DIR, `fileterm-${kind}-${locale}-${width}.png`)
          })
        const idle = await geometry(page)
        const positions = await bounds(page, '.form-actions button')
        await page.locator('.form-actions button').last().hover()
        await page.waitForTimeout(200)
        assert.deepEqual(await bounds(page, '.form-actions button'), positions, `${kind} hover must not move`)
        if (kind !== 'update') {
          await render(page, { ...props, busy: true })
          assert.deepEqual(await geometry(page), idle, `${kind} busy metrics`)
          assert.deepEqual(await bounds(page, '.form-actions button'), positions, `${kind} busy position`)
          assert.equal(await page.locator('.form-actions button:enabled').count(), 0)
          await render(page, { ...props, error: true })
          const withError = await geometry(page)
          await render(page, { ...props, error: true, busy: true })
          assert.deepEqual(await geometry(page), withError, `${kind} error retry widths`)
        }
        await render(page, props)
        assert.deepEqual(await geometry(page), idle)
        const fits = await page.locator('.form-actions button').evaluateAll((nodes) =>
          nodes.every((node) => {
            const r = node.getBoundingClientRect()
            return r.left >= 0 && r.right <= innerWidth && node.scrollWidth <= node.clientWidth
          })
        )
        assert.ok(fits, `${kind} buttons must fit ${locale} ${width}`)
      }
      // All confirmation labels also fit narrow windows, including exit's third action.
      if (width === 420)
        for (const model of cases) {
          await render(page, { model, locale, theme: 'fileterm-light', extra: model.label === 'closeConfirmQuit' })
          assert.ok(
            await page.locator('.form-actions button').evaluateAll((nodes) =>
              nodes.every((node) => {
                const r = node.getBoundingClientRect()
                return r.left >= 0 && r.right <= innerWidth && node.scrollWidth <= node.clientWidth
              })
            ),
            `narrow confirmation ${model.id} ${locale}`
          )
        }
    }
  await page.setViewportSize({ width: 1280, height: 900 })
  for (const theme of ['fileterm-dark', 'fileterm-light']) {
    await render(page, { kind: 'monitor', theme })
    const retry = page.locator('.monitoring-overlay button')
    const idle = await retry.boundingBox()
    await retry.click()
    await page.waitForFunction(() => document.querySelector('.monitoring-overlay button').disabled)
    assert.deepEqual(await retry.boundingBox(), idle, 'monitor retry must keep its natural width and position')
    await retry.dispatchEvent('click')
    assert.equal(await page.evaluate(() => window.calls.filter((c) => c === 'retry').length), 1)
    await page.evaluate(() => window.finishRequest(true))
    await page.locator('.monitoring-overlay [role="alert"]').waitFor()
    assert.deepEqual(await retry.boundingBox(), idle, 'failed monitor retry must keep its layout')
    await retry.click()
    assert.deepEqual(await retry.boundingBox(), idle)
    await page.evaluate(() => window.finishRequest())
    await page.waitForFunction(() => !document.querySelector('.monitoring-overlay button').disabled)
    const toggle = page.locator('.monitoring-toggle')
    const toggleBounds = await toggle.boundingBox()
    await toggle.click()
    await page.waitForFunction(() => document.querySelector('.monitoring-toggle').disabled)
    assert.deepEqual(await toggle.boundingBox(), toggleBounds)
    await page.evaluate(() => window.finishRequest())
    await page.waitForFunction(() => !document.querySelector('.monitoring-toggle').disabled)
    await render(page, { kind: 'sidebar', theme })
    await Promise.all([render(before, { kind: 'sidebar', theme })])
    assert.deepEqual(await bounds(page, '.sidebar-toggle'), await bounds(before, '.sidebar-toggle'))
    const sidebar = await page.locator('.sidebar-toggle').boundingBox()
    await page.locator('.sidebar-toggle').hover()
    await page.waitForTimeout(200)
    assert.deepEqual(await page.locator('.sidebar-toggle').boundingBox(), sidebar)
    await render(page, { kind: 'bookmarks', theme })
    await page.locator('.path-bookmarks-trigger').click()
    await page.locator('.path-bookmarks-row').first().waitFor()
    await page.locator('.path-bookmarks-dialog').evaluate((node) =>
      Promise.all(
        node
          .getAnimations({ subtree: true })
          .filter((animation) => animation.effect.getComputedTiming().iterations !== Infinity)
          .map((animation) => animation.finished.catch(() => {}))
      )
    )
    const list = await bounds(page, '.path-bookmarks-row button')
    await page.locator('.path-bookmarks-open').first().hover()
    await page.waitForTimeout(200)
    assert.deepEqual(await bounds(page, '.path-bookmarks-row button'), list, 'bookmark hover must not shift rows')
    await page.evaluate(() => (window.calls = []))
  }
  for (const theme of ['fileterm-dark', 'fileterm-light', 'codex-dark', 'codex-light', 'custom']) {
    await render(page, { kind: 'buttons', theme, model: { reserve: true } })
    await render(before, { kind: 'buttons', theme, model: { reserve: true } })
    const idle = await bounds(page, '.ft-btn')
    assert.deepEqual(idle, await bounds(before, '.ft-btn'), 'normal Button footprint must match HEAD')
    for (const button of await page.locator('.ft-btn').all()) {
      const box = await button.boundingBox()
      await button.hover()
      await page.waitForTimeout(200)
      assert.deepEqual(await button.boundingBox(), box)
    }
    await render(page, { kind: 'buttons', theme, busy: true, model: { reserve: true } })
    assert.deepEqual(await bounds(page, '.ft-btn'), idle)
    assert.equal(await page.locator('.ft-btn:enabled').count(), 0)
    await render(page, { kind: 'buttons', theme, model: { reserve: true, disabled: true } })
    assert.deepEqual(await bounds(page, '.ft-btn'), idle)
  }
  for (const locale of ['zhCN', 'enUS'])
    for (const width of [1280, 420]) {
      await page.setViewportSize({ width, height: 900 })
      for (const kind of ['root', 'backup', 'passphrase', 'keyboard', 'host', 'smb', 'shares']) {
        const props = { kind, locale, theme: width === 1280 ? 'fileterm-dark' : 'fileterm-light' }
        await render(page, props)
        const selector = kind === 'root' ? '.root-access-entry-modal' : '.form-dialog'
        const frame = await page.locator(selector).boundingBox()
        assert.ok(frame.x >= 0 && frame.x + frame.width <= width, `${kind} frame fits ${width}`)
        if (kind === 'root' && width === 1280) assert.equal(frame.width, 560)
        const controlRadii = await page
          .locator(selector + ' input:not([type=checkbox]):not([type=radio])')
          .evaluateAll((nodes) => nodes.map((node) => getComputedStyle(node).borderRadius))
        assert.ok(
          controlRadii.every((radius) => radius === '4px'),
          `${kind} has consistent field radii`
        )
        if (kind !== 'root') {
          assert.equal(
            await page
              .locator('.form-dialog > .modal-header')
              .evaluate((node) => getComputedStyle(node).borderBottomWidth),
            '1px'
          )
          assert.equal(
            await page
              .locator('.form-dialog > .form-actions')
              .evaluate((node) => getComputedStyle(node).borderTopWidth),
            '1px'
          )
        }
        const idle = await geometry(page)
        await render(page, { ...props, busy: true })
        assert.deepEqual(await geometry(page), idle, `${kind} busy button metrics`)
        assert.equal(await page.locator('.form-actions button:enabled').count(), 0)
        await render(page, { ...props, error: true })
        const errors = await page
          .locator(selector + ' .modal-error')
          .evaluateAll((nodes) => nodes.map((node) => getComputedStyle(node).borderRadius))
        assert.ok(
          errors.every((radius) => radius === '4px'),
          `${kind} error radius matches inputs`
        )
        if (kind === 'root')
          assert.equal(
            await page.locator('.root-access-modal__body').evaluate((node) => getComputedStyle(node).paddingTop),
            '16px'
          )
        if (process.env.FILETERM_UI_ARTIFACT_DIR)
          await page.screenshot({
            path: join(process.env.FILETERM_UI_ARTIFACT_DIR, `fileterm-frame-${kind}-${locale}-${width}.png`)
          })
      }
    }
  await render(page, { kind: 'keyboard', model: { many: true } })
  const footer = await page.locator('.form-dialog > .form-actions').boundingBox()
  assert.ok(footer.y + footer.height <= 900, 'long challenges must keep actions visible')
  await page.locator('.form-dialog__body').evaluate((node) => (node.scrollTop = node.scrollHeight))
  assert.deepEqual(await page.locator('.form-dialog > .form-actions').boundingBox(), footer)
  // Exercise actual form values and submission callbacks, independently of visual snapshots.
  await render(page, { kind: 'passphrase' })
  await page.locator('input[type=password]').fill('test-passphrase')
  await page.locator('.form-actions .ft-btn--primary').click()
  assert.deepEqual(await page.evaluate(() => window.calls.at(-1)), [
    'passphrase',
    { passphrase: 'test-passphrase', savePassphrase: false }
  ])
  await render(page, { kind: 'keyboard' })
  await page.locator('input').fill('123456')
  await page.locator('input').press('Enter')
  assert.deepEqual(await page.evaluate(() => window.calls.at(-1)), ['keyboard', ['123456']])
  await render(page, { kind: 'smb' })
  await page.locator('input').first().fill('test-user')
  await page.locator('input[type=password]').fill('test-password')
  await page.locator('.form-actions .ft-btn--primary').click()
  assert.deepEqual(await page.evaluate(() => window.calls.at(-1)), [
    'smb',
    { username: 'test-user', password: 'test-password' }
  ])
  await render(page, { kind: 'root' })
  await page.locator('input[type=password]').fill('test-password')
  await page.locator('.form-actions .ft-btn--primary').click()
  assert.deepEqual(await page.evaluate(() => window.calls.at(-1)), [
    'root',
    { rootAccessMethod: 'sudo', sudoUser: 'root', sudoPassword: 'test-password' }
  ])
  await render(page, { kind: 'backup' })
  await page.locator('input').first().fill('TestPassword123')
  await page.locator('input').last().fill('TestPassword123')
  await page.locator('.form-actions .ft-btn--primary').click()
  assert.deepEqual(await page.evaluate(() => window.calls.at(-1)), ['backup', 'TestPassword123'])
  for (const locale of ['zhCN', 'enUS'])
    for (const width of [1280, 420]) {
      await page.setViewportSize({ width, height: 900 })
      await render(page, { kind: 'sidebar' })
      await page.evaluate(() => {
        window.bookmarkItems = []
        window.holdBookmarks = true
        window.calls = []
      })
      const props = {
        kind: 'bookmarks',
        locale,
        theme: width === 1280 ? 'fileterm-dark' : 'fileterm-light',
        model: { path: '/srv/very-long-directory-name/another-long-directory-name' }
      }
      await render(page, props)
      await page.locator('.path-bookmarks-trigger').click()
      await page.locator('.path-bookmarks-empty').waitFor()
      await page.locator('.path-bookmarks-dialog').evaluate((node) =>
        Promise.all(
          node
            .getAnimations({ subtree: true })
            .filter((a) => a.effect.getComputedTiming().iterations !== Infinity)
            .map((a) => a.finished.catch(() => {}))
        )
      )
      assert.equal(
        await page
          .locator('.path-bookmarks-dialog > .ft-dialog__header')
          .evaluate((node) => node.getBoundingClientRect().height),
        44
      )
      assert.equal(
        await page
          .locator('.path-bookmarks-dialog .ft-dialog__title')
          .evaluate((node) => getComputedStyle(node).fontSize),
        '14px'
      )
      const dialog = await page.locator('.path-bookmarks-dialog').boundingBox()
      assert.ok(dialog.x >= 0 && dialog.x + dialog.width <= width)
      assert.equal(
        await page.locator('.path-bookmarks-dialog').evaluate((node) => node.scrollWidth <= node.clientWidth),
        true
      )
      const add = page.locator('.path-bookmarks-add-btn')
      const idle = await add.boundingBox()
      if (process.env.FILETERM_UI_ARTIFACT_DIR)
        await page.screenshot({
          path: join(process.env.FILETERM_UI_ARTIFACT_DIR, `fileterm-bookmarks-empty-${locale}-${width}.png`)
        })
      await add.click()
      await page.waitForFunction(() => typeof window.finishBookmarks === 'function')
      assert.deepEqual(await add.boundingBox(), idle, 'bookmark loading must not move the add button')
      await page.evaluate(() => window.finishBookmarks(true))
      await page.locator('.path-bookmarks-error').waitFor()
      assert.equal(
        await page.locator('.path-bookmarks-error').evaluate((node) => getComputedStyle(node).borderRadius),
        '4px'
      )
      await add.click()
      await page.evaluate(() => window.finishBookmarks())
      await page.locator('.path-bookmarks-row').waitFor()
      assert.equal((await add.boundingBox()).width, idle.width, 'already-added label must not resize button')
      assert.deepEqual(
        await page.locator('.path-bookmarks-dialog').boundingBox(),
        dialog,
        'empty/add result retain dialog dimensions'
      )
      await page.locator('.path-bookmarks-delete-btn').click()
      await page.keyboard.press('Escape')
      await page.locator('.confirm-action-dialog').waitFor({ state: 'hidden' })
      await page.locator('.path-bookmarks-dialog').waitFor()
      assert.equal(await page.evaluate(() => window.bookmarkItems.length), 1, 'cancel must keep the bookmark')
      await page.locator('.path-bookmarks-delete-btn').click()
      const confirm = page.locator('.confirm-action-dialog .ft-btn--danger')
      await confirm.click()
      await page.keyboard.press('Escape')
      assert.equal(
        await page.locator('.confirm-action-dialog').isVisible(),
        true,
        'submitting confirmation must stay open'
      )
      await page.evaluate(() => window.finishBookmarks())
      await page.locator('.path-bookmarks-empty').waitFor()
      // Exercise reorder and open without remote side effects.
      await render(page, { kind: 'sidebar' })
      await page.evaluate(() => {
        window.holdBookmarks = false
        window.bookmarkItems = [
          { id: 'a', scope: 'local', name: 'first', path: '/first', type: 'folder' },
          { id: 'b', scope: 'local', name: 'second', path: '/second', type: 'folder' }
        ]
      })
      await render(page, props)
      await page.locator('.path-bookmarks-trigger').click()
      await page.locator('.path-bookmarks-row').first().waitFor()
      await page.locator('.path-bookmarks-row').first().locator('.path-bookmarks-row-actions button').nth(1).click()
      assert.equal(await page.locator('.path-bookmarks-open-name').first().textContent(), 'second')
      await page.locator('.path-bookmarks-open').first().click()
      assert.deepEqual(await page.evaluate(() => window.calls.at(-1)), ['open-bookmark', '/second'])
      await page.locator('.path-bookmarks-trigger').click()
      await page.locator('.path-bookmarks-row').first().waitFor()
      await page.keyboard.press('Escape')
      await page.locator('.path-bookmarks-dialog').waitFor({ state: 'hidden' })
    }
  const shadowAudit = []
  for (const theme of ['fileterm-light', 'codex-light', 'fileterm-dark', 'codex-dark']) {
    await page.setViewportSize({ width: 1100, height: 760 })
    for (const kind of [
      'file-action',
      'permission',
      'ssh-key',
      'root',
      'backup',
      'passphrase',
      'keyboard',
      'smb',
      'shares',
      'connection-editor',
      'proxy-editor',
      'tunnel-editor',
      'settings',
      'ai-history'
    ]) {
      await render(page, { kind, theme })
      await page.keyboard.press('Tab')
      const fields = await page
        .locator(
          'input:not([type=hidden]):not([type=checkbox]):not([type=radio]):not([type=color]):not([type=range]), textarea, .dropdown-select-trigger'
        )
        .all()
      for (let index = 0; index < fields.length; index++) {
        const field = fields[index]
        if (!(await field.isVisible()) || !(await field.isEnabled())) continue
        await field.focus()
        await page.evaluate(() =>
          Promise.all(
            document
              .getAnimations()
              .filter((a) => a.effect.getComputedTiming().iterations !== Infinity)
              .map((a) => a.finished.catch(() => {}))
          )
        )
        if (kind === 'ai-history')
          assert.equal(
            await field.evaluate((node) => getComputedStyle(node).boxShadow),
            'none',
            'AI search paints focus only on the wrapper'
          )
        const surfaces = await field.evaluate(inspectFocusShadow)
        if (surfaces.length > 1) shadowAudit.push({ theme, kind, index, duplicateFeedback: surfaces })
        for (const surface of surfaces) if (surface.clips.length) shadowAudit.push({ theme, kind, index, ...surface })
      }
    }
  }
  if (process.env.FILETERM_UI_ARTIFACT_DIR)
    writeFileSync(
      join(process.env.FILETERM_UI_ARTIFACT_DIR, 'fileterm-shadow-audit.json'),
      JSON.stringify(shadowAudit, null, 2)
    )
  assert.deepEqual(shadowAudit, [], 'focused controls must not duplicate or clip their outer shadows')
  console.log('Focus shadow clipping audit passed: 14 real page/dialog configurations across four built-in themes.')
  await page.emulateMedia({ reducedMotion: 'reduce' })
  await render(page, { kind: 'confirm', model: cases[0], busy: true })
  assert.equal(await page.locator('.ft-btn__spinner').evaluate((node) => getComputedStyle(node).animationName), 'none')
  console.log(
    'Cross-page button/dialog checks passed:',
    cases.length,
    'call-site configurations, HEAD idle geometry, four built-in themes plus custom accent, two locales, narrow windows, actual monitoring, bookmark/sidebar and dialog components, busy/error retry and reduced motion.'
  )

  assert.deepEqual(errors, [])
} finally {
  await browser?.close()
  await new Promise((resolve) => server.close(resolve))
  rmSync(directory, { recursive: true, force: true })
}
