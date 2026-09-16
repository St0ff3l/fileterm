import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

// Execute the actual navigation handler with deterministic UI/IPC boundaries.
const source = readFileSync(new URL('../src/renderer/hooks/file-operations-navigation.ts', import.meta.url), 'utf8')
const exports = {}
vm.runInNewContext(
  ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
  }).outputText,
  {
    exports,
    require: (name) => {
      if (name === '../i18n')
        return {
          t: { crossPaneCutUnsupported: 'copy first', filesCutStatus: 'cut', filesCopiedStatus: 'copy' },
          formatMessage: (text) => text
        }
      if (name === '../app/app-utils') return { withParentRow: (_path, items) => items }
      if (name === './file-operations-utils')
        return {
          allocateTargetNames: (items) => items.map((item) => item.name),
          joinLocalPath: (directory, name) => `${directory}/${name}`,
          joinRemotePath: (directory, name) => `${directory}/${name}`
        }
      throw new Error(`unexpected dependency: ${name}`)
    }
  }
)

function fixture(sourcePane, operation) {
  const calls = []
  const errors = []
  const context = {
    desktopApi: new Proxy(
      {},
      {
        get: (_target, name) => async () => {
          calls.push(name)
          return name === 'listLocalDirectory' ? { path: '/destination', items: [] } : {}
        }
      }
    ),
    workspace: { sessions: { tab: { connected: true } } },
    activeTab: { id: 'tab' },
    activeSession: { connected: true, remotePath: '/destination', remoteFiles: [] },
    localPath: '/destination',
    localItems: [],
    locale: 'enUS',
    fileClipboard: {
      pane: sourcePane,
      operation,
      tabId: 'tab',
      items: [{ path: '/source/file', name: 'file', type: 'file' }]
    },
    ensureActiveRemoteSessionConnected: () => true,
    reportStatusError: (_scope, error) => errors.push(error),
    onApplySnapshot() {},
    onBusyChange() {},
    setLocalPath() {},
    setLocalItems() {},
    setIsLocalDirectoryLoading() {},
    setLocalNetworkShareSource() {},
    setFileClipboard: () => calls.push('clearClipboard')
  }
  return { navigation: exports.createFileOperationsNavigation(context), calls, errors }
}

for (const sourcePane of ['local', 'remote']) {
  const destination = sourcePane === 'local' ? 'remote' : 'local'
  test(`${sourcePane} cross-pane cut never queues a transfer or deletes a source`, async () => {
    const { navigation, calls, errors } = fixture(sourcePane, 'cut')
    navigation.handlePasteIntoPane(destination)
    await new Promise(setImmediate)
    assert.deepEqual(calls, [])
    assert.equal(errors.length, 1)
    assert.equal(errors[0].message, 'copy first')
  })
  test(`${sourcePane} cross-pane copy queues transfer without deleting source`, async () => {
    const { navigation, calls, errors } = fixture(sourcePane, 'copy')
    navigation.handlePasteIntoPane(destination)
    await new Promise(setImmediate)
    assert.equal(errors.length, 0)
    assert.ok(calls.includes(sourcePane === 'local' ? 'uploadFile' : 'downloadRemotePath'))
    assert.ok(!calls.includes('deleteLocalPath') && !calls.includes('deleteRemotePath'))
  })
  test(`${sourcePane} same-pane cut still moves files`, async () => {
    const { navigation, calls, errors } = fixture(sourcePane, 'cut')
    navigation.handlePasteIntoPane(sourcePane)
    await new Promise(setImmediate)
    assert.equal(errors.length, 0)
    assert.ok(calls.includes(sourcePane === 'local' ? 'moveLocalPath' : 'moveRemotePath'))
    assert.ok(calls.includes('clearClipboard'))
  })
}
