import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function load(relativePath, modules = {}, globals = {}) {
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL(relativePath, import.meta.url), 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    { exports, URL, require: (name) => modules[name] ?? {}, ...globals }
  )
  return exports
}

const shared = load('../../../../packages/shared/src/connection-host.ts')
const { outboundAddressFamily, outboundInterfaceConflict, isUsedAsJumpHost } = load(
  '../../src/renderer/app/ssh-network-interface.ts',
  { '@fileterm/shared': shared }
)

test('outbound interface limits selection to supported IP literals', () => {
  assert.equal(outboundAddressFamily('192.0.2.1'), 'ipv4')
  assert.equal(outboundAddressFamily('[2001:db8::1]'), 'ipv6')
  for (const host of [
    'router.local',
    '012.0.0.1',
    '127.0.0.1',
    '0.0.0.0',
    '224.0.0.1',
    '::1',
    '::',
    '::ffff:192.0.2.1',
    'fe80::1%15',
    'fe80::1',
    'ff02::1'
  ]) {
    assert.equal(outboundAddressFamily(host), null, host)
  }
})

test('renderer shows proxy, OpenSSH, and jump conflicts without erasing the preference', () => {
  const form = { type: 'ssh', host: '192.0.2.1', sshNetworkInterface: { adapterId: 'saved' } }
  assert.equal(outboundInterfaceConflict(form, false), null)
  assert.equal(outboundInterfaceConflict({ ...form, authType: 'sshConfig' }, false), 'profile')
  assert.equal(outboundInterfaceConflict({ ...form, proxyProfileId: 'proxy' }, false), 'proxy')
  assert.equal(outboundInterfaceConflict({ ...form, jumpProfileId: 'old', jumpProfileIds: [] }, false), null)
  assert.equal(outboundInterfaceConflict({ ...form, jumpProfileIds: ['hop'] }, false), 'jump')
  assert.equal(outboundInterfaceConflict(form, true), 'jump')
  assert.equal(isUsedAsJumpHost('hop', [{ type: 'ssh', jumpProfileId: 'hop', jumpProfileIds: [] }]), false)
  assert.equal(isUsedAsJumpHost('hop', [{ type: 'ssh', jumpProfileId: 'hop' }]), true)
  assert.equal(form.sshNetworkInterface.adapterId, 'saved')
})

test('Tauri bridge enumerates adapters and passes an explicit clear to the connection test', async () => {
  const rows = [{ id: 'adapter', name: 'WLAN', online: true, ipv4: true, ipv6: false }]
  const calls = []
  const sharedApi = load('../../src/bridge/shared-api.ts', {})
  const { createTauriApi } = load(
    '../../src/bridge/tauri-api.ts',
    {
      '@tauri-apps/api/core': {
        Channel: class {},
        isTauri: () => true,
        invoke: async (...args) => {
          calls.push(args)
          return rows
        }
      },
      '@tauri-apps/api/window': { getCurrentWindow: () => ({ onDragDropEvent: async () => () => {} }) },
      './runtime-metadata': { getRuntimeMetadata: async () => ({ platform: 'win32' }) },
      './shared-api': sharedApi
    },
    { window: { addEventListener() {} } }
  )
  const api = await createTauriApi()
  assert.strictEqual(await api.listSshNetworkInterfaces(), rows)
  await api.testConnection({ sshNetworkInterface: null }, 'profile')
  assert.deepEqual(JSON.parse(JSON.stringify(calls)), [
    ['app_list_ssh_network_interfaces'],
    ['app_test_connection', { input: { sshNetworkInterface: null }, profileId: 'profile' }]
  ])
})
