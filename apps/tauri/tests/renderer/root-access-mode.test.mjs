import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const source = readFileSync(new URL('../../src/renderer/hooks/file-operations-transfers.ts', import.meta.url), 'utf8')
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

function reuseFixture(reusable, execute) {
  const state = { requests: [], snapshots: [], dialogs: [], errors: [] }
  const submitting = { current: false }
  const handlers = exports.useFileOperationsTransfers(
    {
      desktopApi: {
        setRemoteFileAccessMode: async (...args) => {
          state.requests.push(args)
          return execute()
        }
      },
      activeTab: { id: 'tab', sessionType: 'ssh' },
      activeSession: { connected: true, fileAccessMode: 'user', sudoUser: 'root', hasReusableSudoAuth: reusable },
      activeProfile: { type: 'ssh', username: 'alice' },
      rootAccessSubmittingRef: submitting,
      ensureActiveRemoteSessionConnected: () => true,
      onApplySnapshot: (snapshot) => state.snapshots.push(snapshot),
      onBusyChange() {},
      setRootAccessDialog: (dialog) => state.dialogs.push(dialog),
      setRootAccessDialogError: (error) => state.errors.push(error),
      setIsRootAccessSubmitting() {},
      formatError: (_scope, error) => error.message
    },
    {}
  )
  return { state, handlers, submitting }
}

test('returning to root reuses verified session sudo credentials without reopening the dialog', async () => {
  let resolve
  const pending = new Promise((done) => {
    resolve = done
  })
  const f = reuseFixture(true, () => pending)
  f.handlers.handleToggleRemoteFileAccessMode()
  f.handlers.handleToggleRemoteFileAccessMode()
  assert.equal(f.state.requests.length, 1, 'duplicate clicks cannot start another authentication')
  const [tab, mode, options] = f.state.requests[0]
  assert.equal(tab, 'tab')
  assert.equal(mode, 'root')
  assert.equal(options.useSavedPassword, true)
  assert.equal(options.sudoPassword, undefined, 'renderer does not read cached secrets')
  const snapshot = { sessions: { tab: { fileAccessMode: 'root' } } }
  resolve(snapshot)
  await new Promise(setImmediate)
  assert.deepEqual(f.state.snapshots, [snapshot])
  assert.deepEqual(f.state.dialogs, [])
  assert.equal(f.submitting.current, false)
})

test('failed sudo reuse returns to authentication with the error and unlocks submission', async () => {
  const f = reuseFixture(true, async () => {
    throw new Error('credential expired')
  })
  f.handlers.handleToggleRemoteFileAccessMode()
  await new Promise(setImmediate)
  assert.equal(f.state.dialogs[0].tabId, 'tab')
  assert.equal(f.state.errors[0], 'credential expired')
  assert.deepEqual(f.state.snapshots, [])
  assert.equal(f.submitting.current, false)
})

test('a session without reusable authentication opens the existing sudo dialog', () => {
  const f = reuseFixture(false, () => {
    throw new Error('must not execute')
  })
  f.handlers.handleToggleRemoteFileAccessMode()
  assert.equal(f.state.requests.length, 0)
  assert.equal(f.state.dialogs[0].rootAccessMethod, 'sudo')
})
