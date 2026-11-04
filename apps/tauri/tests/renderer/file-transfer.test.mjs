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
    { exports, require: (name) => dependencies[name] ?? {} }
  )
  return exports
}
const { scopeTransfersToSession } = load('../../src/renderer/features/transfers/transfer-scope.ts')
const { buildOrderedTabs, parseStoredMainTabUiState } = load('../../src/renderer/hooks/workspace-tabs-utils.ts', {
  '../i18n': { t: { untitledTab: 'Home' } }
})

test('server copies are visible from both endpoints without leaking to unrelated sessions', () => {
  const copy = { id: 'copy', tabId: 'b', profileId: 'B', copySource: { tabId: 'a', profileId: 'A' } }
  const sessions = [
    { id: 'a', profileId: 'A' },
    { id: 'b', profileId: 'B' },
    { id: 'c', profileId: 'C' }
  ]
  assert.equal(scopeTransfersToSession([copy], 'a', 'A', sessions)[0], copy)
  assert.equal(scopeTransfersToSession([copy], 'b', 'B', sessions)[0], copy)
  assert.equal(scopeTransfersToSession([copy], 'c', 'C', sessions).length, 0)
})

test('file transfer tab survives saved UI state while invalid endpoint fields are rejected', () => {
  const tab = { id: 'file-transfers', kind: 'transfers', title: 'Files', initialTabId: 'a', initialPath: '/srv' }
  const state = parseStoredMainTabUiState(
    JSON.stringify({ localTabs: [tab], activeLocalTabId: tab.id, tabOrder: [`home:${tab.id}`] })
  )
  assert.equal(state.localTabs[0].initialPath, '/srv')
  assert.equal(buildOrderedTabs(state.tabOrder, state.localTabs, [])[0].tabKind, 'transfers')
  const invalid = parseStoredMainTabUiState(JSON.stringify({ localTabs: [{ ...tab, initialTabId: {} }] }))
  assert.equal(invalid.localTabs.length, 0)
})

test('file transfers open beside the current tab, matching system info insertion and reusing an existing tab', () => {
  const appUtils = load('../../src/renderer/app/app-utils.ts')
  const { createWorkspaceTabsContextActions } = load('../../src/renderer/hooks/workspace-tabs-context-actions.ts', {
    '../app/app-utils': appUtils,
    '../i18n': { t: { fileTransferWorkspace: 'Files' } },
    './workspace-tabs-utils': { formatSystemInfoTabTitle: (title) => title }
  })
  for (const activeLocalTabId of [null, 'system-nas']) {
    for (const method of activeLocalTabId ? ['openFileTransfers'] : ['openFileTransfers', 'openSystemInfo']) {
      const state = {
        order: ['session:ftp', 'session:nas', 'home:system-nas', 'home:tail'],
        tabs: activeLocalTabId ? [{ id: activeLocalTabId, kind: 'system', sessionTabId: 'nas' }] : [],
        active: null
      }
      const initialCount = state.tabs.length
      const actions = () =>
        createWorkspaceTabsContextActions({
          activeTab: { id: 'nas', title: 'NAS' },
          activeLocalTabId,
          localTabs: state.tabs,
          workspace: { sessions: { nas: { remotePath: '/srv' } } },
          tabActivationRequestRef: { current: 0 },
          setLocalTabs: (update) => {
            state.tabs = update(state.tabs)
          },
          setTabOrder: (update) => {
            state.order = update(state.order)
          },
          setActiveLocalTabId: (id) => {
            state.active = id
          },
          setPendingActiveTabId() {},
          onStatusMessage() {}
        })
      actions()[method]()
      const id = method === 'openFileTransfers' ? 'file-transfers' : 'system-nas'
      const anchor = activeLocalTabId ? 'home:system-nas' : 'session:nas'
      if (id !== activeLocalTabId) assert.equal(state.order[state.order.indexOf(anchor) + 1], `home:${id}`)
      assert.equal(state.active, id)
      const order = [...state.order]
      actions()[method]()
      assert.deepEqual(Array.from(state.order), order, 'reopening must reuse the tab without duplicating or moving it')
      assert.equal(state.tabs.length, initialCount + 1)
      if (method === 'openFileTransfers')
        assert.equal(state.tabs.find((tab) => tab.kind === 'transfers').initialPath, '/srv')
    }
  }
})

test('closing the last file transfer tab requests tab closure instead of closing the window', async () => {
  const appUtils = load('../../src/renderer/app/app-utils.ts')
  const groups = load('../../src/renderer/features/layout/host-groups.ts')
  const workspaceUtils = load('../../src/renderer/hooks/workspace-tabs-utils.ts', {
    '../i18n': { t: { untitledTab: 'New Tab' } },
    '../features/layout/host-groups': groups
  })
  const { createWorkspaceTabsContextActions } = load('../../src/renderer/hooks/workspace-tabs-context-actions.ts', {
    '../app/app-utils': appUtils,
    '../i18n': { t: {} },
    '../features/workspace/home-transition': {},
    './workspace-tabs-utils': workspaceUtils
  })
  const fileTab = { id: 'file-transfers', kind: 'transfers', title: 'Files' }
  const state = { closeRequests: [], windowClosed: false }
  const actions = createWorkspaceTabsContextActions({
    desktopApi: {},
    workspace: { tabs: [] },
    isBusy: false,
    onBusyChange() {},
    onStatusMessage() {},
    onError() {},
    onCloseCurrentWindow() {
      state.windowClosed = true
    },
    onRequestQuit() {},
    localTabs: [fileTab],
    setLocalTabs() {},
    activeLocalTabId: fileTab.id,
    setActiveLocalTabId() {},
    setPendingActiveTabId() {},
    tabActivationRequestRef: { current: 0 },
    nextHomeTabNumber: 2,
    setNextHomeTabNumber() {},
    setTabOrder() {},
    visibleWorkspaceTabs: [],
    visibleActiveSessionTabId: null,
    activeTab: null,
    setClosingSessionTabIds() {},
    shortcutCloseConfirm: null,
    setShortcutCloseConfirm() {},
    tabContextMenu: null,
    setTabContextMenu() {},
    draggingTabKey: null,
    setDraggingTabKey() {},
    localTabsRef: { current: [fileTab] },
    orderedTabs: workspaceUtils.buildOrderedTabs([`home:${fileTab.id}`], [fileTab], []),
    tabBarLayout: 'single-row',
    closeHomeTabs(homeTabIds) {
      state.closeRequests.push(homeTabIds)
    },
    activateSessionTab: async () => {},
    closeSessionTabById: async () => null,
    closePane: async () => {},
    applySnapshot() {},
    reconnectSessionTab: async () => {},
    disconnectSessionTab: async () => {}
  })

  await actions.closeActiveWorkspaceItem()

  assert.equal(state.windowClosed, false)
  assert.deepEqual(Array.from(state.closeRequests[0]), [fileTab.id])
})

test('completion refreshes only the destination pane and directory, including a visible folder child', () => {
  const utilities = load('../../src/renderer/hooks/file-operations-utils.ts', {
    '../app/app-utils': { WINDOWS_DRIVES_PATH: '__drives__' }
  })
  const { shouldRefreshTransferPane } = load('../../src/renderer/features/file-transfer/transfer-refresh.ts', {
    '../../hooks/file-operations-utils': utilities
  })
  const task = {
    status: 'done',
    direction: 'copy',
    tabId: 'b',
    profileId: 'B',
    destinationPath: '/backup/file',
    copySource: { profileId: 'A' }
  }
  assert.equal(shouldRefreshTransferPane(task, 'b', '/backup', 'B'), true)
  assert.equal(shouldRefreshTransferPane(task, 'new-b', '/backup', 'B'), true)
  assert.equal(shouldRefreshTransferPane(task, 'a', '/backup', 'A'), false)
  assert.equal(shouldRefreshTransferPane(task, 'b', '/other', 'B'), false)
  assert.equal(shouldRefreshTransferPane(task, 'local', '/backup'), false)
  assert.equal(shouldRefreshTransferPane({ ...task, status: 'running' }, 'b', '/backup', 'B'), false)
  const folder = { ...task, targetType: 'folder', destinationPath: '/backup/data' }
  assert.equal(shouldRefreshTransferPane(folder, 'b', '/backup/data/sub', 'B'), true)
  assert.equal(shouldRefreshTransferPane(folder, 'b', '/backup/database', 'B'), false)
  const download = { status: 'done', direction: 'download', destinationPath: '/local/data/file' }
  assert.equal(shouldRefreshTransferPane(download, 'local', '/local/data'), true)
  assert.equal(shouldRefreshTransferPane(download, 'local', '/unrelated'), false)
  assert.equal(
    shouldRefreshTransferPane(
      { ...download, targetType: 'folder', destinationPath: '/local/data' },
      'local',
      '/local/data/sub'
    ),
    true
  )
  assert.equal(shouldRefreshTransferPane({ ...download, destinationPath: undefined }, 'local', '/local'), true)
})
