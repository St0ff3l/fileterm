import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function load(path, dependencies = {}) {
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL(path, import.meta.url), 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    {
      exports,
      require: (name) => {
        if (!(name in dependencies)) throw new Error(`unexpected dependency: ${name}`)
        return dependencies[name]
      }
    }
  )
  return exports
}
const i18n = { t: { untitledTab: 'New Tab' } }
const groups = load('../../src/renderer/features/layout/host-groups.ts')
const utils = load('../../src/renderer/hooks/workspace-tabs-utils.ts', {
  '../i18n': i18n,
  '../features/layout/host-groups': groups
})
const profileOpenGuard = load('../../src/renderer/hooks/profile-open-guard.ts')
const { buildOrderedTabs, nextTabAfterClose, reconcileTabOrder } = utils
const { createWorkspaceTabsSessionActions } = load('../../src/renderer/hooks/workspace-tabs-session-actions.ts', {
  '../app/app-utils': { homeTabKey: (id) => `home:${id}`, sessionTabKey: (id) => `session:${id}` },
  '../features/common/session-send-targets': {},
  '../i18n': i18n,
  './profile-open-guard': profileOpenGuard,
  './session-tab-memory': load('../../src/renderer/hooks/session-tab-memory.ts'),
  './workspace-tabs-utils': utils
})
const tabs = ['a', 'b', 'c', 'd'].map((id) => ({
  id,
  profileId: 'host',
  sessionType: 'ssh',
  status: 'connected',
  title: id
}))

test('the first new-session snapshot keeps the host tab bar populated before order effects run', () => {
  const entries = buildOrderedTabs(['session:b', 'session:a'], [], tabs.slice(0, 3))
  const activeKey = groups.resolveActiveHostGroupKey(entries, null, 'c')
  const group = groups.buildHostGroups(entries, []).find((entry) => entry.key === activeKey)
  assert.deepEqual(
    Array.from(group.entries, (entry) => entry.key),
    ['session:b', 'session:a', 'session:c']
  )
})

test('transfer and system pages follow the host group of the session they were opened from', () => {
  const sources = [
    { id: 'a', profileId: 'alpha', sessionType: 'ssh', status: 'connected', title: 'a' },
    { id: 'b', profileId: 'beta', sessionType: 'ssh', status: 'connected', title: 'b' }
  ]
  const local = [
    { id: 'file-transfers', kind: 'transfers', title: 'File Transfer', initialTabId: 'b' },
    { id: 'network-a', kind: 'system', view: 'network', title: 'Network', sessionTabId: 'a', sourceTabTitle: 'a' }
  ]
  const entries = buildOrderedTabs(['home:file-transfers', 'home:network-a'], local, sources)
  const keyOf = (key) =>
    groups.entryHostGroupKey(
      entries.find((entry) => entry.key === key),
      sources
    )
  assert.equal(keyOf('home:file-transfers'), 'profile:beta')
  assert.equal(keyOf('home:network-a'), 'profile:alpha')

  const orphan = buildOrderedTabs(['home:file-transfers'], [local[0]], sources)
  assert.equal(
    groups.entryHostGroupKey(
      orphan[0],
      sources.filter((tab) => tab.id !== 'b')
    ),
    groups.HOME_HOST_GROUP_KEY
  )
})

test('a pending home replacement defers the new session tab until tabOrder is rewritten', () => {
  const home = [{ id: 'home-1', kind: 'home', title: 'New Tab' }]
  const deferred = buildOrderedTabs(['home:home-1'], home, tabs, { deferMissingSessionTabs: true })
  assert.deepEqual(
    Array.from(deferred, (entry) => entry.key),
    ['home:home-1']
  )
  const replaced = buildOrderedTabs(['session:new-session'], [], [...tabs, { ...tabs[0], id: 'new-session' }], {
    deferMissingSessionTabs: true
  })
  assert.deepEqual(
    Array.from(replaced, (entry) => entry.key),
    ['session:new-session']
  )
})

test('an early snapshot cannot replace the home order before the open request completes', () => {
  const order = ['session:a', 'home:home-1']
  const keys = ['home:home-1', 'session:a', 'session:new-session']
  const pending = reconcileTabOrder(order, keys, true)
  assert.deepEqual(Array.from(pending), order)
  assert.deepEqual(Array.from(reconcileTabOrder(pending, keys, false)), [...order, 'session:new-session'])
})

test('closing selects the right neighbour, then the left, using dragged and grouped order', () => {
  const entries = buildOrderedTabs(['session:c', 'session:a', 'session:b', 'session:d'], [], tabs)
  assert.equal(nextTabAfterClose(entries, 'session:a', false).tab.id, 'b')
  assert.equal(nextTabAfterClose(entries, 'session:d', false).tab.id, 'b')
  const mixed = buildOrderedTabs([], [], [tabs[0], { ...tabs[1], profileId: 'other' }, tabs[2]])
  assert.equal(nextTabAfterClose(mixed, 'session:a', true).tab.id, 'c')
  assert.equal(nextTabAfterClose(mixed, 'session:a', false).tab.id, 'b')
})

test('grouped closing never leaves the host group or falls back to the home tab', () => {
  const sources = [
    { id: 'a1', profileId: 'alpha', sessionType: 'ssh', status: 'connected', title: 'a1' },
    { id: 'b1', profileId: 'beta', sessionType: 'ssh', status: 'connected', title: 'b1' },
    { id: 'a2', profileId: 'alpha', sessionType: 'ssh', status: 'connected', title: 'a2' }
  ]
  const local = [
    { id: 'home-1', kind: 'home', title: 'New Tab' },
    { id: 'network-a2', kind: 'system', view: 'network', title: 'Network', sessionTabId: 'a2', sourceTabTitle: 'a2' }
  ]
  const entries = buildOrderedTabs(
    ['home:home-1', 'session:a1', 'session:b1', 'session:a2', 'home:network-a2'],
    local,
    sources
  )
  assert.equal(utils.nextTabInHostGroup(entries, 'session:a1').key, 'session:a2')
  assert.equal(utils.nextTabInHostGroup(entries, 'home:network-a2').key, 'session:a2')
  assert.equal(utils.nextTabInHostGroup(entries, 'session:a2', ['session:a2', 'home:network-a2', 'session:a1']), null)
})

test('management close selects the next tab of the same host instead of another server', async () => {
  const sessions = [
    { id: 'a1', profileId: 'alpha', sessionType: 'ssh', status: 'connected', title: 'a1' },
    { id: 'b1', profileId: 'beta', sessionType: 'ssh', status: 'connected', title: 'b1' },
    { id: 'a2', profileId: 'alpha', sessionType: 'ssh', status: 'connected', title: 'a2' }
  ]
  const calls = []
  const { state, actions } = fixture(
    {
      activateTab: async (id) => {
        calls.push(`activate:${id}`)
        return { tabs: sessions, activeTabId: id }
      },
      closeTab: async (id) => {
        calls.push(`close:${id}`)
        return { tabs: sessions.filter((tab) => tab.id !== id), activeTabId: 'a2' }
      }
    },
    null,
    'home-1',
    sessions,
    {
      serverManagementMode: true,
      workspace: { tabs: sessions, activeTabId: 'a1' },
      visibleActiveSessionTabId: 'a1'
    }
  )
  await actions.closeSessionTabById('a1')
  assert.deepEqual(calls, ['activate:a2', 'close:a1'])
  assert.equal(state.activeLocalTabId, null)
})

test('management close returns to the overview only when the host group runs out of tabs', async () => {
  const sessions = [
    { id: 'b1', profileId: 'beta', sessionType: 'ssh', status: 'connected', title: 'b1' },
    { id: 'a2', profileId: 'alpha', sessionType: 'ssh', status: 'connected', title: 'a2' }
  ]
  const calls = []
  const { state, actions } = fixture(
    {
      activateTab: async (id) => {
        calls.push(`activate:${id}`)
        return { tabs: sessions, activeTabId: id }
      },
      closeTab: async (id) => {
        calls.push(`close:${id}`)
        return { tabs: sessions.filter((tab) => tab.id !== id), activeTabId: 'b1' }
      },
      hideSessionToBackground: async (id) => {
        calls.push(`hide:${id}`)
        return {
          tabs: sessions.map((tab) => (tab.id === id ? { ...tab, isBackground: true } : tab)),
          activeTabId: 'b1'
        }
      }
    },
    null,
    'home-1',
    sessions,
    {
      serverManagementMode: true,
      workspace: { tabs: sessions, activeTabId: 'a2' },
      visibleActiveSessionTabId: 'a2'
    }
  )
  await actions.closeSessionTabById('a2')
  assert.deepEqual(calls, ['hide:a2'])
  assert.equal(state.activeLocalTabId, 'home-1')
})

test('closing a managed terminal keeps the server connection in the background', async () => {
  const sessions = [{ id: 'a1', profileId: 'alpha', sessionType: 'ssh', status: 'connected', title: 'a1' }]
  const calls = []
  const { state, actions } = fixture(
    {
      closeTab: async (id) => {
        calls.push(`close:${id}`)
        return { tabs: [], activeTabId: null }
      },
      hideSessionToBackground: async (id) => {
        calls.push(`hide:${id}`)
        return { tabs: [{ ...sessions[0], isBackground: true }], activeTabId: null }
      }
    },
    null,
    'home-1',
    sessions,
    {
      serverManagementMode: true,
      workspace: { tabs: sessions, activeTabId: 'a1' },
      visibleActiveSessionTabId: 'a1'
    }
  )
  await actions.closeSessionTabById('a1')
  assert.deepEqual(calls, ['hide:a1'])
  assert.equal(state.closing.length, 0)
  assert.equal(state.activeLocalTabId, 'home-1')
})

test('closing one of several managed terminals still closes that session', async () => {
  const sessions = [
    { id: 'a1', profileId: 'alpha', sessionType: 'ssh', status: 'connected', title: 'a1' },
    { id: 'a2', profileId: 'alpha', sessionType: 'ssh', status: 'connected', title: 'a2' }
  ]
  const calls = []
  const { actions } = fixture(
    {
      activateTab: async (id) => {
        calls.push(`activate:${id}`)
        return { tabs: sessions, activeTabId: id }
      },
      closeTab: async (id) => {
        calls.push(`close:${id}`)
        return { tabs: sessions.filter((tab) => tab.id !== id), activeTabId: 'a2' }
      },
      hideSessionToBackground: async (id) => {
        calls.push(`hide:${id}`)
        return { tabs: sessions, activeTabId: 'a2' }
      }
    },
    null,
    'home-1',
    sessions,
    {
      serverManagementMode: true,
      workspace: { tabs: sessions, activeTabId: 'a1' },
      visibleActiveSessionTabId: 'a1'
    }
  )
  await actions.closeSessionTabById('a1')
  assert.deepEqual(calls, ['activate:a2', 'close:a1'])
})

test('management connect runs the session in the background instead of opening a tab', async () => {
  const calls = []
  const { state, actions } = fixture(
    {
      openProfile: async (id) => {
        calls.push(`open:${id}`)
        return { tabs, activeTabId: 'a' }
      },
      openProfileBackground: async (id) => {
        calls.push(`background:${id}`)
        return { tabs, activeTabId: null }
      }
    },
    'home-1',
    'home-1',
    tabs,
    { serverManagementMode: true }
  )
  await actions.connectProfile('a')
  assert.deepEqual(calls, ['background:a'])
  assert.equal(state.activeLocalTabId, 'home-1')
})

test('management terminal entry attaches a background session instead of activating it', async () => {
  const calls = []
  const backgroundTab = { ...tabs[0], id: 'bg', isBackground: true, status: 'connected' }
  const { state, actions } = fixture(
    {
      activateTab: async (id) => {
        calls.push(`activate:${id}`)
        return { tabs: [backgroundTab], activeTabId: id }
      },
      attachBackgroundSession: async (id) => {
        calls.push(`attach:${id}`)
        return { tabs: [{ ...backgroundTab, isBackground: false }], activeTabId: id }
      }
    },
    'home-1',
    'home-1',
    [backgroundTab],
    { serverManagementMode: true, workspace: { tabs: [backgroundTab], activeTabId: null } }
  )
  await actions.activateSessionTab('bg')
  assert.deepEqual(calls, ['attach:bg'])
  assert.equal(state.activeLocalTabId, null)
})

test('disconnecting a background session closes the hidden connection', async () => {
  const calls = []
  const backgroundTab = { ...tabs[0], id: 'bg', isBackground: true, status: 'connected' }
  const { actions } = fixture(
    {
      disconnectTab: async (id) => {
        calls.push(`disconnect:${id}`)
        return { tabs: [backgroundTab], activeTabId: null }
      },
      closeTab: async (id) => {
        calls.push(`close:${id}`)
        return { tabs: [], activeTabId: null }
      }
    },
    null,
    null,
    [backgroundTab],
    { serverManagementMode: true, workspace: { tabs: [backgroundTab], activeTabId: null } }
  )
  await actions.disconnectSessionTab('bg')
  assert.deepEqual(calls, ['close:bg'])
})

function fixture(api, activeLocalTabId = null, homeTabId = null, sessionTabs = tabs, overrides = {}) {
  const homeTabs = homeTabId ? [{ id: homeTabId, kind: 'home', title: 'New Tab' }] : []
  const state = {
    activeLocalTabId,
    localTabs: homeTabs,
    pending: null,
    closing: [],
    order: sessionTabs.map((tab) => `session:${tab.id}`),
    snapshots: [],
    errors: []
  }
  const set = (key) => (value) => {
    state[key] = typeof value === 'function' ? value(state[key]) : value
  }
  const context = {
    desktopApi: api,
    workspace: { tabs: sessionTabs, activeTabId: 'b' },
    tabBarLayout: 'host-groups',
    localTabs: homeTabs,
    localTabsRef: { current: homeTabs },
    tabOrder: state.order,
    visibleWorkspaceTabs: sessionTabs,
    visibleActiveSessionTabId: 'b',
    activeLocalTabId,
    activeLocalTab: homeTabs[0] ?? null,
    activeLocalTabIdForUi: homeTabId ?? activeLocalTabId,
    isHomeWorkspaceVisible: Boolean(homeTabId),
    isMainWorkspaceWindow: false,
    pendingHomeReplacementKeyRef: { current: null },
    orderedTabs: buildOrderedTabs(state.order, homeTabs, sessionTabs),
    tabActivationRequestRef: { current: 0 },
    setActiveLocalTabId: set('activeLocalTabId'),
    setPendingActiveTabId: set('pending'),
    setClosingSessionTabIds: set('closing'),
    setTabOrder: set('order'),
    setLocalTabs: set('localTabs'),
    nextHomeTabNumber: 2,
    setNextHomeTabNumber: () => {},
    onSnapshot: (snapshot) => state.snapshots.push(snapshot),
    onBusyChange: () => {},
    onStatusMessage: () => {},
    onError: (...args) => state.errors.push(args),
    ...overrides
  }
  return { state, actions: createWorkspaceTabsSessionActions(context) }
}

test('failed optimistic activation restores the previously selected home tab', async () => {
  const { state, actions } = fixture(
    {
      activateTab: async () => {
        throw new Error('stale tab')
      }
    },
    'home-1'
  )
  await actions.activateSessionTab('c')
  assert.equal(state.activeLocalTabId, 'home-1')
  assert.equal(state.pending, null)
  assert.equal(state.errors.length, 1)
})

test('a late failed activation cannot undo a newer successful activation', async () => {
  let fail
  const { state, actions } = fixture(
    {
      activateTab: (id) =>
        id === 'a'
          ? new Promise((_, reject) => {
              fail = reject
            })
          : Promise.resolve({ tabs, activeTabId: id })
    },
    'home-1'
  )
  const first = actions.activateSessionTab('a')
  await actions.activateSessionTab('c')
  fail(new Error('late failure'))
  await first
  assert.equal(state.activeLocalTabId, null)
  assert.equal(state.pending, null)
})

test('all session close paths activate the neighbour before backend last-tab fallback can run', async () => {
  const calls = []
  const { state, actions } = fixture({
    activateTab: async (id) => {
      calls.push(`activate:${id}`)
      return { tabs, activeTabId: id }
    },
    closeTab: async (id) => {
      calls.push(`close:${id}`)
      return { tabs: tabs.filter((tab) => tab.id !== id), activeTabId: 'c' }
    }
  })
  await actions.closeSessionTabById('b')
  assert.deepEqual(calls, ['activate:c', 'close:b'])
  assert.equal(state.snapshots.at(-1).activeTabId, 'c')
})

test('opening a connection replaces the active home tab instead of adding a tab', async () => {
  const { state, actions } = fixture(
    { openProfile: async () => ({ tabs, activeTabId: 'new-session' }) },
    'home-1',
    'home-1'
  )
  state.order.unshift('home:home-1')
  await actions.openProfile('profile')
  assert.deepEqual(state.localTabs, [])
  assert.equal(state.order[0], 'session:new-session')
  assert.equal(state.activeLocalTabId, null)
})

test('failed connection opening keeps the home tab and clears the pending replacement', async () => {
  const { state, actions } = fixture(
    {
      openProfile: async () => {
        throw new Error('open failed')
      }
    },
    'home-1',
    'home-1'
  )
  state.order.unshift('home:home-1')
  await actions.openProfile('profile')
  assert.equal(state.localTabs[0].id, 'home-1')
  assert.equal(state.order[0], 'home:home-1')
  assert.equal(state.errors.length, 1)
})

for (const homeId of [null, 'home-1']) {
  test(`hiding an external session opens background sessions, ${homeId ? 'reusing the existing home' : 'creating a home tab'}`, async () => {
    const sessions = tabs.map((tab) => ({ ...tab, source: 'mcp' }))
    const snapshot = { tabs: sessions.filter((tab) => tab.id !== 'b'), activeTabId: 'c' }
    const { state, actions } = fixture({ detachSessionToBackground: async () => snapshot }, null, homeId, sessions)
    await actions.detachSessionToBackground('b')
    const home = state.localTabs.find((tab) => tab.id === state.activeLocalTabId)
    assert.equal(home.id, homeId ?? 'home-2')
    assert.equal(home.backgroundSessionsRequest, 1)
    assert.ok(state.order.includes(`home:${home.id}`))
    assert.equal(state.order.includes('session:b'), false)
    assert.equal(
      buildOrderedTabs(state.order, state.localTabs, snapshot.tabs).find((entry) => entry.id === home.id)
        .backgroundSessionsRequest,
      1
    )
    assert.equal(state.errors.length, 0)
  })
}

test('a failed hide keeps the current view and tab order', async () => {
  const sessions = tabs.map((tab) => ({ ...tab, source: 'mcp' }))
  const { state, actions } = fixture(
    {
      detachSessionToBackground: async () => {
        throw new Error('failed')
      }
    },
    null,
    null,
    sessions
  )
  const order = [...state.order]
  await actions.detachSessionToBackground('b')
  assert.equal(state.activeLocalTabId, null)
  assert.equal(state.localTabs.length, 0)
  assert.deepEqual(state.order, order)
  assert.equal(state.errors.length, 1)
})

test('only hidden, never-connected UI tabs are auto-closed after a failed connection', () => {
  const sessions = {
    a: { connected: false },
    b: { connected: false, reconnectMode: 'auto' },
    c: { connected: false },
    d: { connected: false }
  }
  const failed = [
    { ...tabs[0], status: 'error' },
    { ...tabs[1], status: 'error' },
    { ...tabs[2], status: 'error', isBackground: true },
    { ...tabs[3], status: 'error', source: 'mcp' }
  ]
  assert.equal(utils.findFailedTabToAutoClose(failed, sessions, null).id, 'a')
  assert.equal(utils.findFailedTabToAutoClose(failed, sessions, 'a'), null)
  assert.equal(utils.findFailedTabToAutoClose(failed, sessions, null, new Set(['a'])), null)
  assert.equal(utils.findFailedTabToAutoClose(failed.slice(1), sessions, null), null)
  assert.equal(
    utils.findFailedTabToAutoClose([{ ...tabs[0], status: 'connected' }], { a: { connected: true } }, null),
    null
  )
})

test('the failure detail is the last transcript line without the failure prefix', () => {
  assert.equal(
    utils.resolveSessionFailureDetail('连接主机...\r\n连接失败: SSH hop 1: timed out after 30 seconds\r\n'),
    'SSH hop 1: timed out after 30 seconds'
  )
  assert.equal(utils.resolveSessionFailureDetail(''), '')
  assert.equal(utils.resolveSessionFailureDetail(undefined), '')
})
