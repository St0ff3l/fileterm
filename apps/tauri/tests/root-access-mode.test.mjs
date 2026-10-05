import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const source = readFileSync(new URL('../src/renderer/hooks/file-operations-transfers.ts', import.meta.url), 'utf8')
const exports = {}
vm.runInNewContext(
  ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
  }).outputText,
  {
    exports,
    require: (name) => {
      if (name === 'react') return { useEffect() {} }
      if (name === '../i18n') return { t: {} }
      return {}
    }
  }
)

for (const mode of ['root', 'user']) {
  test(`switching to ${mode} consumes the backend directory without reopening the stale path`, async () => {
    const snapshots = []
    const failures = []
    let staleRefreshes = 0
    let dismissed = false
    const snapshot = { sessions: { tab: { remotePath: mode === 'root' ? '/volume2/homes/alice' : '/homes/alice' } } }
    const handlers = exports.useFileOperationsTransfers(
      {
        desktopApi: { setRemoteFileAccessMode: async () => snapshot },
        workspace: { sessions: { tab: { connected: true } } },
        activeTab: { id: 'tab', sessionType: 'ssh' },
        activeSession: { connected: true, remotePath: '/homes/alice', fileAccessMode: 'root' },
        rootAccessDialog: { tabId: 'tab' },
        rootAccessSubmittingRef: { current: false },
        ensureActiveRemoteSessionConnected: () => true,
        onApplySnapshot: (value) => snapshots.push(value),
        onBusyChange() {},
        setRootAccessDialog: (value) => {
          dismissed = value === null
        },
        setRootAccessDialogError() {},
        setIsRootAccessSubmitting() {},
        reportOperationError: (_setter, _scope, error) => failures.push(error),
        reportStatusError: (_scope, error) => failures.push(error)
      },
      {
        refreshCurrentPane: async () => {
          staleRefreshes++
        }
      }
    )
    if (mode === 'root') {
      handlers.handleConfirmRootAccess({ rootAccessMethod: 'sudo', sudoUser: 'root', sudoPassword: '' })
    } else {
      handlers.handleToggleRemoteFileAccessMode()
    }
    await new Promise(setImmediate)
    assert.deepEqual(failures, [])
    assert.deepEqual(snapshots, [snapshot])
    assert.equal(staleRefreshes, 0)
    if (mode === 'root') assert.equal(dismissed, true)
  })
}
