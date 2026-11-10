import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const defaults = {
  useEmptyPassword: false,
  enableExecChannel: true,
  enableResourceMonitoring: true,
  resourceMonitoringIntervalSeconds: 1,
  resourceMonitoringMetrics: [],
  resourceMonitoringMetricOrder: [],
  reconnectMode: 'none',
  legacyAlgorithms: false
}
const exports = {}
const source = readFileSync(new URL('../../src/renderer/app/app-data.ts', import.meta.url), 'utf8')
vm.runInNewContext(
  ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
  }).outputText,
  {
    exports,
    require: (name) => {
      if (name === '@fileterm/core') {
        return {
          DEFAULT_RESOURCE_MONITORING_METRICS: [],
          DEFAULT_RESOURCE_MONITORING_METRIC_ORDER: [],
          DEFAULT_SSH_CONNECTION_DEFAULTS: defaults
        }
      }
      if (name === '../i18n') return { t: { defaultGroup: 'Default' } }
      throw new Error(`Unexpected import ${name}`)
    }
  }
)

const profile = {
  id: 'profile-1',
  type: 'ssh',
  name: 'Test server',
  host: 'example.com',
  port: 22,
  username: 'user',
  group: 'Default',
  remotePath: '.'
}

test('new connections default to local files and saved profile modes restore when editing', () => {
  assert.equal(exports.defaultForm.filePaneMode, 'local')
  assert.equal(exports.profileToForm(profile).filePaneMode, 'local')
  assert.equal(exports.profileToForm({ ...profile, filePaneMode: 'remote-tree' }).filePaneMode, 'remote-tree')
})

test('no-delete mode survives opening a saved SSH or SFTP profile for editing', () => {
  for (const type of ['ssh', 'sftp']) {
    assert.equal(exports.profileToForm({ ...profile, type, sftpNoDelete: true }).sftpNoDelete, true)
    assert.equal(exports.profileToForm({ ...profile, type }).sftpNoDelete, false)
  }
})
