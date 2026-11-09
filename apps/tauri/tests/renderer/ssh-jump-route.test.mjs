import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'
import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import * as jsxRuntime from 'react/jsx-runtime'

const exports = {}
vm.runInNewContext(
  ts.transpileModule(readFileSync(new URL('../../src/renderer/app/ssh-jump-route.ts', import.meta.url), 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
  }).outputText,
  { exports }
)
const resolve = (target, profiles) => JSON.parse(JSON.stringify(exports.resolveSshJumpRoute(target, profiles)))
const hop = (id, jumpProfileId, extra = {}) => ({
  id,
  name: id,
  type: 'ssh',
  host: id,
  port: 22,
  jumpProfileId,
  ...extra
})

test('routing mode follows the current profile fields, including asynchronously hydrated forms', () => {
  assert.equal(exports.getSshRoutingMode({}), 'direct')
  assert.equal(exports.getSshRoutingMode({ jumpProfileId: 'entry' }), 'jump')
  assert.equal(exports.getSshRoutingMode({ jumpProfileIds: ['entry', 'middle'] }), 'jump')
  assert.equal(exports.getSshRoutingMode({ jumpProfileId: 'legacy', jumpProfileIds: [] }), 'direct')
  assert.equal(exports.hasIncompleteSshJumpRoute({ jumpProfileIds: [''] }), true)
  assert.equal(exports.hasIncompleteSshJumpRoute({ jumpProfileIds: ['deleted-profile'] }), false)
})

test('routes are ordered from entry to target and old direct/single-hop profiles remain valid', () => {
  const profiles = [hop('middle', 'entry'), hop('entry')]
  assert.deepEqual(resolve({}, profiles), { jumpHosts: [], error: null })
  assert.deepEqual(
    resolve({ jumpProfileId: 'entry' }, profiles).jumpHosts.map((profile) => profile.id),
    ['entry']
  )
  assert.deepEqual(
    resolve({ id: 'target', jumpProfileId: 'middle' }, profiles).jumpHosts.map((profile) => profile.id),
    ['entry', 'middle']
  )
})

test('candidate selection rejects cycles, deleted and non-SSH references and conflicting proxy routes', () => {
  assert.equal(resolve({ id: 'target', jumpProfileId: 'target' }, []).error, 'cycle')
  assert.equal(resolve({ id: 'target', jumpProfileId: 'middle' }, [hop('middle', 'target')]).error, 'cycle')
  assert.equal(resolve({ jumpProfileId: 'missing' }, []).error, 'missing')
  for (const type of ['serial', 'telnet', 'ftp']) {
    assert.equal(resolve({ jumpProfileId: 'entry' }, [hop('entry', undefined, { type })]).error, 'type')
  }
  assert.equal(resolve({ jumpProfileId: 'entry', proxy: { type: 'http' } }, [hop('entry')]).error, 'proxy-conflict')
  assert.equal(
    resolve({ jumpProfileId: 'entry', proxyProfileId: 'saved-proxy' }, [hop('entry')]).error,
    'proxy-conflict'
  )
  assert.equal(
    resolve({ jumpProfileId: 'middle' }, [hop('middle', 'entry', { proxy: { type: 'socks5' } }), hop('entry')]).error,
    'proxy-conflict'
  )
  // An entry's outbound proxy remains valid because it has no upstream SSH hop.
  assert.equal(
    resolve({ jumpProfileId: 'entry' }, [hop('entry', undefined, { proxy: { type: 'socks5' } })]).error,
    null
  )
})

test('eight jump hosts are allowed and adding a ninth is rejected', () => {
  const profiles = Array.from({ length: 9 }, (_, index) => hop(`hop-${index}`, index ? `hop-${index - 1}` : undefined))
  assert.equal(resolve({ jumpProfileId: 'hop-7' }, profiles).jumpHosts.length, 8)
  assert.equal(resolve({ jumpProfileId: 'hop-8' }, profiles).error, 'too-long')
})

function loadComponent(path, dependencies) {
  const moduleExports = {}
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL(path, import.meta.url), 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX }
    }).outputText,
    {
      exports: moduleExports,
      require: (name) => {
        if (name === 'react/jsx-runtime') return jsxRuntime
        if (name.endsWith('.css')) return {}
        assert.ok(name in dependencies, `Unexpected dependency: ${name}`)
        return dependencies[name]
      }
    }
  )
  return moduleExports
}

test('connection settings show the complete route and explain invalid references', () => {
  const { ConnectionNetworkSection } = loadComponent(
    '../../src/renderer/features/connections/connection-network-section.tsx',
    {
      '../../i18n': { t: new Proxy({}, { get: (_target, key) => key }) },
      '../../app/ssh-jump-route': exports,
      '../../components/common/segmented-control/segmented-control': { SegmentedControl: () => null },
      '../common/app-icon': { AppIcon: () => null },
      '../common/dropdown-select': {
        DropdownSelect: ({ options }) => createElement('span', null, options.map((option) => option.label).join(' | '))
      },
      './connection-proxy-section': { ConnectionProxySection: () => null },
      './connection-tunnel-section': { ConnectionTunnelSection: () => null }
    }
  )
  const profiles = [hop('entry'), hop('middle', 'entry')]
  const props = {
    form: { type: 'ssh', name: 'target', jumpProfileId: 'middle' },
    jumpHosts: profiles,
    profiles,
    jumpRoute: resolve({ jumpProfileId: 'middle' }, profiles).jumpHosts,
    routeError: null,
    routingMode: 'jump',
    setForm() {},
    setRoutingMode() {}
  }
  const html = renderToStaticMarkup(createElement(ConnectionNetworkSection, props))
  const breadcrumb = html.match(/<ol[^>]*>.*?<\/ol>/)?.[0]
  assert.ok(breadcrumb)
  assert.ok(breadcrumb.indexOf('entry') < breadcrumb.indexOf('middle'))
  assert.ok(breadcrumb.indexOf('middle') < breadcrumb.indexOf('target'))
  assert.match(breadcrumb, /title="entry:22"/)
  assert.match(breadcrumb, /aria-label="route"/)
  const invalid = renderToStaticMarkup(
    createElement(ConnectionNetworkSection, { ...props, routeError: 'Missing jump host' })
  )
  assert.match(invalid, /role="alert">Missing jump host/)
  assert.doesNotMatch(invalid, /<ol/)
})

test('transfer status exposes every jump host and the final target', () => {
  const { TransferBar } = loadComponent('../../src/renderer/features/transfers/transfer-bar.tsx', {
    '../../i18n': { t: new Proxy({}, { get: (_target, key) => key }) },
    '../common/app-icon': { AppIcon: () => null },
    '../common/status-indicator': { StatusIndicator: () => null }
  })
  const html = renderToStaticMarkup(
    createElement(TransferBar, {
      activeCount: 0,
      activeTabId: 'tab-1',
      activeJumpHosts: [hop('hop-1'), hop('hop-2')],
      activeRouteTarget: hop('target'),
      isPending: false,
      onOpen() {}
    })
  )

  assert.match(html, /aria-label="viaJumpHost: hop-1 → hop-2 → target"/)
  assert.match(html, /title="viaJumpHost: hop-1 → hop-2 → target \(hop-1:22 → hop-2:22 → target:22\)"/)
  assert.match(html, /class="transfer-session-jump-name">hop-1 → hop-2 → target/)
  assert.match(html, /class="transfer-session-jump" tabindex="0"/)
})
