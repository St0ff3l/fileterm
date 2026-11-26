import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import * as jsxRuntime from 'react/jsx-runtime'
import ts from 'typescript'

function load(name, imports = {}) {
  const filename = new URL(`../../src/renderer/features/proxies/${name}`, import.meta.url)
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(readFileSync(filename, 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX }
    }).outputText,
    {
      exports,
      require: (id) => {
        if (!(id in imports)) throw new Error(`Unexpected dependency ${id}`)
        return imports[id]
      }
    }
  )
  return exports
}

const helpers = load('ssh-route-rules.ts')
const direct = { mode: 'fixed', route: { type: 'direct' } }
const proxy = { mode: 'fixed', route: { type: 'proxy', proxyId: 'existing' } }
const ssh = { id: 'server', type: 'ssh', name: 'Test server', tags: ['office', 'vpn'] }

test('uses effective server and tag rules, warns about ties and preserves manual routes', () => {
  const policy = {
    servers: {},
    tags: { office: { priority: 3, rule: direct }, vpn: { priority: 3, rule: proxy } },
    default: { mode: 'auto', routes: [{ type: 'direct' }] }
  }
  assert.equal(helpers.effectiveSshRule(ssh, policy).scope, 'conflict')
  assert.equal(helpers.effectiveSshRule({ ...ssh, proxyProfileId: 'existing' }, policy).scope, 'legacy')
  assert.equal(helpers.effectiveSshRule({ ...ssh, tags: ['office'] }, policy).label, 'office')
  assert.equal(helpers.effectiveSshRule({ ...ssh, tags: [] }, policy).scope, 'default')
  assert.equal(helpers.effectiveSshRule(ssh, { ...policy, servers: { server: direct } }).scope, 'server')
  assert.equal(helpers.fallbackSshRuleSource({ tags: [] }, { servers: {}, tags: {}, default: null }).kind, 'direct')
  assert.equal(helpers.fallbackSshRuleSource({ tags: [] }, policy).kind, 'global')
  const tagFallback = helpers.fallbackSshRuleSource({ tags: ['office'] }, policy)
  assert.equal(tagFallback.kind, 'tag')
  assert.equal(tagFallback.tag, 'office')
  assert.equal(helpers.fallbackSshRuleSource({ tags: [], proxyProfileId: 'existing' }, policy).kind, 'manual')
})

test('only ordinary SSH appears in tag and server policy editing', () => {
  const matches = helpers.eligibleSshRoutes([
    ssh,
    { ...ssh, type: 'sftp' },
    { ...ssh, jumpProfileIds: ['hop'] },
    { ...ssh, authType: 'sshConfig' }
  ])
  assert.equal(matches.length, 1)
  assert.equal(matches[0], ssh)
})

test('batch saves target only tagged ordinary SSH profiles without mutating the previous policy', () => {
  const baseline = { servers: { kept: proxy }, tags: {}, default: null, clients: {} }
  const next = helpers.applySshRouteEdit(baseline, { scope: 'batch', tag: 'office', rule: direct }, [
    ssh,
    { ...ssh, id: 'other', tags: ['elsewhere'] },
    { ...ssh, id: 'sftp', type: 'sftp' },
    { ...ssh, id: 'jump', jumpProfileIds: ['entry'] }
  ])
  assert.deepEqual(Object.keys(next.servers).sort(), ['kept', ssh.id].sort())
  assert.equal(next.servers[ssh.id], direct)
  assert.equal(next.servers.kept, proxy)
  assert.deepEqual(Object.keys(baseline.servers), ['kept'])
})

test('ordered route editor preserves missing references, excludes duplicates, and allows empty drafts', () => {
  const { SshRouteEditor } = load('ssh-route-editor.tsx', {
    'react/jsx-runtime': jsxRuntime,
    '../../components/common/button/button': {
      Button: ({ children, disabled, 'aria-label': ariaLabel }) =>
        createElement('button', { disabled, 'aria-label': ariaLabel }, children)
    },
    '../../components/common/panel-action-button/panel-action-button': {
      PanelActionButton: ({ children, disabled, 'aria-label': ariaLabel }) =>
        createElement('button', { disabled, 'aria-label': ariaLabel }, children)
    },
    '../common/app-icon': { AppIcon: () => null },
    '../common/dropdown-select': {
      DropdownSelect: ({ options, value }) =>
        createElement(
          'div',
          { 'data-value': value },
          options.map((option) =>
            createElement(
              'span',
              {
                key: option.value,
                'data-disabled': option.disabled ? 'yes' : 'no'
              },
              option.label
            )
          )
        )
    },
    '../../i18n': { t: new Proxy({}, { get: (_, key) => key }) },
    './ssh-route-rules': helpers
  })
  const render = (value, emptyMode = null) =>
    renderToStaticMarkup(
      createElement(SshRouteEditor, {
        value,
        emptyMode,
        options: [
          { route: { type: 'direct' }, label: 'System direct' },
          { route: { type: 'proxy', proxyId: 'available' }, label: 'Local proxy' }
        ],
        disabled: false,
        onChange() {},
        onEmptyModeChange() {}
      })
    )

  const html = render({
    mode: 'preferred',
    primary: { type: 'proxy', proxyId: 'removed' },
    fallback: [{ type: 'direct' }]
  })
  assert.match(html, /data-disabled="yes">sshRouteMissing · proxy:removed/)
  assert.match(html, /data-disabled="yes">System direct/)
  assert.match(html, /sshRoutePrimary/)
  assert.match(html, /sshRouteAdd/)

  const singleAutoHtml = render({ mode: 'auto', routes: [{ type: 'direct' }] })
  assert.match(singleAutoHtml, /aria-label="sshRouteRemove"/)
  assert.doesNotMatch(singleAutoHtml, /disabled=""[^>]*aria-label="sshRouteRemove"/)

  const emptyAutoHtml = render(null, 'auto')
  assert.match(emptyAutoHtml, /0\/4/)
  assert.match(emptyAutoHtml, /data-value=""/)
})
