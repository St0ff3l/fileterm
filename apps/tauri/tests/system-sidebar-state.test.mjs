import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

// Exercise the workspace's public state/actions while isolating unrelated IPC hooks.
function fixture(options = {}) {
  const { monitoring, profile = {}, focus = false } = { monitoring: true, ...options }
  const tab = { id: 'ssh-tab', profileId: 'profile', sessionType: 'ssh' }
  let userCollapsed = false
  let focusModes = { [tab.id]: focus, other: true }
  const dependencies = {
    react: {
      useCallback: (fn) => fn,
      useMemo: (fn) => fn(),
      useRef: (value) => ({ current: value }),
      useState: (value) => [typeof value === 'function' ? value() : value, () => {}],
      useEffect: () => {}
    },
    '../lib/diagnostic-log': {},
    '../app/app-shell-utils': { DEFAULT_FILE_PANEL_RATIO: 30, DEFAULT_COMMAND_LIST_WIDTH: 250 },
    '../features/workspace/file-panel-snap': {},
    './connection-host-trust': {},
    './use-workspace-tabs': {
      useWorkspaceTabs: () => ({
        activeTab: tab,
        activeProfile: { type: 'ssh', ...profile },
        activeSession: { connected: true, capabilities: { resourceMonitoring: monitoring } },
        isSystemSidebarCollapsed: userCollapsed,
        showSidebar: true,
        setIsSystemSidebarCollapsed: (value) => {
          userCollapsed = value
        }
      })
    }
  }
  for (const [path, symbol] of [
    ['./use-workspace-modals', 'useWorkspaceModals'],
    ['./use-file-editor', 'useFileEditor'],
    ['./use-file-operations', 'useFileOperations'],
    ['./use-ssh-interactions', 'useSshInteractions'],
    ['./use-backup-password-interactions', 'useBackupPasswordInteractions'],
    ['./use-sudo-password-prompt', 'useSudoPasswordPrompt']
  ])
    dependencies[path] = { [symbol]: () => ({}) }
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL('../src/renderer/hooks/use-app-workspace.ts', import.meta.url), 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    {
      exports,
      require: (name) => {
        assert.ok(name in dependencies, `Unexpected import ${name}`)
        return dependencies[name]
      }
    }
  )
  return {
    render: () =>
      exports.useAppWorkspace({
        workspace: {},
        connectionDefaults: { enableResourceMonitoring: true },
        workspaceFocusModes: focusModes,
        workspaceViews: {},
        commandPaneWidths: {},
        filePanelHeights: {},
        filePanelRatios: {},
        filePanelSnapTargets: {},
        setWorkspaceFocusModes: (update) => {
          focusModes = update(focusModes)
        }
      }),
    focusModes: () => focusModes
  }
}

test('monitoring disabled, unsupported or pending never prevents sidebar expansion', () => {
  for (const options of [
    { monitoring: false },
    { monitoring: undefined },
    { profile: { enableResourceMonitoring: false } },
    { profile: { connectionOverrides: { enableResourceMonitoring: false } } },
    { profile: { deviceMode: 'network-device' } }
  ]) {
    const view = fixture(options)
    assert.equal(view.render().isSystemSidebarCollapsed, false)
    for (let cycle = 0; cycle < 10; cycle++) {
      view.render().setIsSystemSidebarCollapsed(true)
      assert.equal(view.render().isSystemSidebarCollapsed, true)
      view.render().setIsSystemSidebarCollapsed(false)
      assert.equal(view.render().isSystemSidebarCollapsed, false)
    }
  }
})

test('expanding the sidebar exits focus mode only for the active workspace', () => {
  const view = fixture({ focus: true })
  assert.equal(view.render().isSystemSidebarCollapsed, true)
  view.render().setIsSystemSidebarCollapsed(false)
  assert.equal(view.render().isSystemSidebarCollapsed, false)
  assert.equal(view.focusModes()['ssh-tab'], false)
  assert.equal(view.focusModes().other, true)
})
