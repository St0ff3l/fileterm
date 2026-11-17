import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/features/system/network-connections-model.ts', import.meta.url), 'utf8'),
    {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }
  ).outputText,
  { exports }
)
const { connectionsFor, trafficTotal, formatSocketBytes, socketEndpoint, socketKey } = exports
const listener = {
  protocol: 'tcp',
  localAddress: '0.0.0.0',
  localPort: 22,
  remoteAddress: '*',
  remotePort: 0,
  pid: 1,
  state: 'LISTEN',
  listening: true,
  txBytes: null,
  rxBytes: null
}
const peer = {
  ...listener,
  localAddress: '10.0.0.1',
  remoteAddress: '10.0.0.2',
  remotePort: 55000,
  pid: 2,
  state: 'ESTAB',
  listening: false,
  txBytes: 1024,
  rxBytes: 2048
}

test('wildcard listeners include child-process sockets, exclude other protocols, ports and address families', () => {
  const sockets = [
    listener,
    peer,
    { ...peer, protocol: 'udp' },
    { ...peer, localPort: 80 },
    { ...peer, localAddress: '::1' }
  ]
  assert.equal(connectionsFor(listener, sockets).length, 1)
  assert.equal(connectionsFor(listener, sockets)[0], peer)
  assert.equal(connectionsFor({ ...listener, localAddress: '127.0.0.1' }, sockets).length, 0)
  assert.equal(connectionsFor({ ...listener, localAddress: '::' }, sockets).length, 1)
})

test('missing traffic is unknown rather than zero; explicit kernel zero remains zero', () => {
  assert.equal(trafficTotal([], 'txBytes'), null)
  assert.equal(trafficTotal([peer, { ...peer, txBytes: null }], 'txBytes'), null)
  assert.equal(trafficTotal([peer, peer], 'txBytes'), 2048)
  assert.equal(trafficTotal([{ ...peer, txBytes: 0 }], 'txBytes'), 0)
  assert.equal(formatSocketBytes(null), '—')
  assert.equal(formatSocketBytes(1024), '1.0 KiB')
})

test('IPv6 endpoints are unambiguous and outgoing connections keep distinct selection identities', () => {
  assert.equal(socketEndpoint('::1', 22), '[::1]:22')
  assert.notEqual(socketKey(peer), socketKey({ ...peer, remotePort: 55001 }))
})

function loadTs(relativePath, dependencies = {}) {
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL(relativePath, import.meta.url), 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    {
      exports,
      require: (name) => {
        if (!(name in dependencies)) throw Error(`Unexpected dependency: ${name}`)
        return dependencies[name]
      }
    }
  )
  return exports
}

const groups = loadTs('../../src/renderer/features/layout/host-groups.ts')
const utils = loadTs('../../src/renderer/hooks/workspace-tabs-utils.ts', {
  '../i18n': {
    t: { networkConnections: 'Network connections', systemInfoTabTitle: 'System info', untitledTab: 'New tab' }
  },
  '../features/layout/host-groups': groups
})
const { createWorkspaceTabsContextActions } = loadTs('../../src/renderer/hooks/workspace-tabs-context-actions.ts', {
  '../app/app-utils': {
    homeTabKey: (id) => `home:${id}`,
    sessionTabKey: (id) => `session:${id}`,
    insertTabKeyAfter: (order, key, after) => {
      const result = [...order]
      result.splice(result.indexOf(after) + 1, 0, key)
      return result
    }
  },
  '../i18n': { t: {} },
  '../features/workspace/home-transition': {},
  './workspace-tabs-utils': utils
})

test('network info opens a separate host tab, reuses it, and survives stored UI state restoration', () => {
  let localTabs = [
    { id: 'system-nas', kind: 'system', title: 'System info · NAS', sessionTabId: 'nas', sourceTabTitle: 'NAS' }
  ]
  let order = ['session:nas', 'home:system-nas']
  let activeId = null
  const context = () => ({
    localTabs,
    activeTab: { id: 'nas', title: 'NAS' },
    activeLocalTabId: null,
    setLocalTabs: (update) => {
      localTabs = update(localTabs)
    },
    setTabOrder: (update) => {
      order = update(order)
    },
    tabActivationRequestRef: { current: 0 },
    setPendingActiveTabId() {},
    onStatusMessage() {},
    setActiveLocalTabId: (id) => {
      activeId = id
    }
  })
  createWorkspaceTabsContextActions(context()).openNetworkConnections()
  assert.equal(localTabs.length, 2)
  assert.equal(activeId, 'network-nas')
  assert.equal(localTabs[1].title, 'Network connections · NAS')
  assert.equal(order[1], 'home:network-nas')
  createWorkspaceTabsContextActions(context()).openNetworkConnections()
  assert.equal(localTabs.length, 2)
  const restored = utils.parseStoredMainTabUiState(
    JSON.stringify({ localTabs, tabOrder: order, activeLocalTabId: activeId })
  )
  assert.equal(restored.localTabs[1].view, 'network')
  const hostTab = { id: 'nas', profileId: 'profile-nas', sessionType: 'ssh' }
  const entries = utils.buildOrderedTabs(order, localTabs, [hostTab])
  assert.equal(groups.entryHostGroupKey(entries[1], [hostTab]), 'profile:profile-nas')
})

test('refresh row keys survive reordering and distinguish duplicate sockets', () => {
  const other = { ...peer, localPort: 443 }
  const before = exports.socketRowKeys([peer, other, peer])
  const after = exports.socketRowKeys([other, peer, peer])
  assert.equal(before[0], after[1])
  assert.equal(before[1], after[0])
  assert.notEqual(before[0], before[2])
})

test('wildcard address labels explain IPv6 without rewriting concrete addresses', () => {
  assert.equal(exports.socketAddressLabel('::', 'All IPv4', 'All IPv6'), 'All IPv6')
  assert.equal(exports.socketAddressLabel('0.0.0.0', 'All IPv4', 'All IPv6'), 'All IPv4')
  assert.equal(exports.socketAddressLabel('*', 'All IPv4', 'All IPv6'), '*')
  assert.equal(exports.socketAddressLabel('fe80::1', 'All IPv4', 'All IPv6'), 'fe80::1')
})

test('network header sorting compares numeric counters and addresses without sorting display strings', () => {
  const rows = [
    { key: 'large', sortValues: [443, '10.0.0.12', 2048] },
    { key: 'small', sortValues: [22, '10.0.0.2', 512] },
    { key: 'unknown', sortValues: [80, '10.0.0.3', null] }
  ]
  const keys = (column, direction) => exports.sortNetworkRows(rows, { column, direction }).map((row) => row.key)
  assert.deepEqual(Array.from(keys(0, 'asc')), ['small', 'unknown', 'large'])
  assert.deepEqual(Array.from(keys(1, 'asc')), ['small', 'unknown', 'large'])
  assert.deepEqual(Array.from(keys(2, 'desc')), ['large', 'small', 'unknown'])
  assert.deepEqual(
    rows.map((row) => row.key),
    ['large', 'small', 'unknown']
  )
})

test('only double-clicking blank space clears socket selection; refresh cannot reselect it', () => {
  const states = []
  let cursor = 0
  const jsx = (type, props) => ({ type, props })
  const module = {}
  const snapshot = { sockets: [listener, { ...listener, localPort: 80 }], source: 'ss' }
  vm.runInNewContext(
    ts.transpileModule(
      readFileSync(
        new URL('../../src/renderer/features/system/network-connections-workspace.tsx', import.meta.url),
        'utf8'
      ),
      { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX } }
    ).outputText,
    {
      exports: module,
      require: (name) => {
        if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx }
        if (name === 'react')
          return {
            useEffect() {},
            useMemo: (fn) => fn(),
            useRef: (current) => ({ current }),
            useState: (initial) => {
              const index = cursor++
              if (!(index in states)) states[index] = index === 0 ? snapshot : initial
              return [
                states[index],
                (value) => {
                  states[index] = typeof value === 'function' ? value(states[index]) : value
                }
              ]
            }
          }
        if (name === './network-connections-model') return exports
        if (name === '../../i18n') return { t: new Proxy({}, { get: (_target, key) => key }) }
        return new Proxy({}, { get: (_target, key) => key })
      }
    }
  )
  const render = () => {
    cursor = 0
    return module.NetworkConnectionsWorkspace({
      tabId: 'tab',
      connected: true,
      isActive: true,
      accessMode: 'user',
      accessUser: 'root',
      privilegeBusy: false,
      onToggleAccessMode() {}
    })
  }
  const tables = (page) => page.props.children.filter((child) => child?.type === 'NetworkSocketTable')
  let page = render()
  assert.equal(page.props.onPointerDown, undefined, 'single clicks must not clear selection')
  tables(page)[0].props.rows[1].onSelect()
  page = render()
  assert.equal(tables(page)[0].props.rows[1].selected, true)
  page.props.onDoubleClick({ target: { closest: () => ({}) } })
  assert.equal(tables(render())[0].props.rows[1].selected, true)
  page.props.onDoubleClick({ target: { closest: () => null } })
  page = render()
  assert.equal(
    tables(page)[0].props.rows.some((row) => row.selected),
    false
  )
  assert.equal(tables(page)[1].props.rows.length, 0)
  states[0] = { ...snapshot, sockets: snapshot.sockets.map((socket) => ({ ...socket })) }
  assert.equal(
    tables(render())[0].props.rows.some((row) => row.selected),
    false
  )
})
