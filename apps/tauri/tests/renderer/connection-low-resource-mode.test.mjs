import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(
      new URL('../../src/renderer/features/connections/connection-low-resource-mode.ts', import.meta.url),
      'utf8'
    ),
    { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
  ).outputText,
  { exports }
)
const { isLowResourceSshMode, toggleLowResourceSshMode } = exports
const defaults = { enableExecChannel: true, enableResourceMonitoring: true }

test('low-resource preset changes only this form and preserves longer timeouts and credentials', () => {
  const form = { type: 'ssh', host: 'host', password: 'secret', connectTimeoutSeconds: 120 }
  const enabled = toggleLowResourceSshMode(form, defaults)
  assert.equal(isLowResourceSshMode(enabled, defaults), true)
  assert.equal(enabled.connectTimeoutSeconds, 120)
  assert.equal(enabled.password, form.password)
  assert.equal(enabled.host, form.host)
  assert.equal(form.enableExecChannel, undefined)
  assert.equal(defaults.enableExecChannel, true)
  assert.equal(toggleLowResourceSshMode({ type: 'ssh' }, defaults).connectTimeoutSeconds, 60)
})

test('manual feature or timeout changes immediately clear the derived mode state', () => {
  const enabled = toggleLowResourceSshMode({ type: 'ssh' }, defaults)
  for (const key of ['enableExecChannel', 'enableResourceMonitoring', 'sftpEnabled']) {
    const customized = { ...enabled, [key]: true }
    assert.equal(isLowResourceSshMode(customized, defaults), false, key)
    assert.equal(isLowResourceSshMode(toggleLowResourceSshMode(customized, defaults), defaults), true)
  }
  assert.equal(isLowResourceSshMode({ ...enabled, connectTimeoutSeconds: 30 }, defaults), false)
  assert.equal(isLowResourceSshMode(JSON.parse(JSON.stringify(enabled)), defaults), true)
})

test('turning off restores default features and does not affect other protocols or network devices', () => {
  const enabled = toggleLowResourceSshMode({ type: 'ssh' }, defaults)
  const disabled = toggleLowResourceSshMode(enabled, defaults)
  assert.equal(isLowResourceSshMode(disabled, defaults), false)
  assert.equal(disabled.enableExecChannel, true)
  assert.equal(disabled.enableResourceMonitoring, true)
  assert.equal(disabled.sftpEnabled, true)
  assert.equal(disabled.connectTimeoutSeconds, 30)
  for (const form of [{ type: 'ftp' }, { type: 'ssh', deviceMode: 'network-device' }]) {
    assert.equal(toggleLowResourceSshMode(form, defaults), form)
    assert.equal(isLowResourceSshMode(form, defaults), false)
  }
})
