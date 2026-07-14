import assert from 'node:assert/strict'
import path from 'node:path'
import test from 'node:test'
import type { ConnectionProfile } from '@fileterm/core'
import {
  exportProfiles,
  previewExternalConnectionJson,
  previewSshConfig
} from '../../src/main/services/connection-config-codec.ts'

test('external connection JSON maps an SSH connection without exposing its password in preview metadata', () => {
  const [item] = previewExternalConnectionJson(
    JSON.stringify({
      name: '生产 SSH',
      host: 'server.example.test',
      port: 2222,
      user_name: 'ops',
      password: 'never-render-this',
      authentication_type: 'keyboard-interactive',
      terminal_encoding: 'GBK',
      conection_type: 'ssh',
      exec_channel_enable: false
    }),
    'fallback'
  )
  assert.equal(item?.status, 'ready')
  assert.equal(item?.input?.authType, 'keyboard-interactive')
  assert.equal(item?.input?.password, 'never-render-this')
  assert.equal(item?.unsupportedFields?.includes('password'), false)
})

test('interactive exports preserve credentials while background exports remain scrubbed', () => {
  const profile: ConnectionProfile = {
    id: 'ssh-1',
    name: 'Proxied',
    type: 'ssh',
    host: 'host.example.test',
    port: 22,
    username: 'operator',
    group: '默认',
    remotePath: '/',
    authType: 'privateKey',
    privateKeyPath: '/home/operator/.ssh/id_ed25519',
    passphrase: 'key-secret',
    password: 'login-secret',
    sftpEnabled: true,
    proxy: { type: 'http', host: 'proxy.example.test', port: 8080, username: 'proxy-user', password: 'proxy-secret' }
  }
  const backgroundExport = JSON.stringify(exportProfiles([profile], 'fileterm'))
  for (const secret of ['login-secret', 'key-secret', 'proxy-secret', 'id_ed25519'])
    assert.equal(backgroundExport.includes(secret), false)
  assert.equal(backgroundExport.includes('proxy.example.test'), true)

  const exported = exportProfiles([profile], 'fileterm', true)
  const imported = previewExternalConnectionJson(JSON.stringify(exported), 'backup')
  assert.equal(imported[0]?.input?.username, 'operator')
  assert.equal(imported[0]?.input?.password, 'login-secret')
  assert.equal(imported[0]?.input?.privateKeyPath, '/home/operator/.ssh/id_ed25519')
  assert.equal(imported[0]?.input?.passphrase, 'key-secret')

  const compatible = JSON.stringify(exportProfiles([profile], 'compatible', true))
  for (const secret of ['login-secret', 'key-secret', 'id_ed25519']) assert.equal(compatible.includes(secret), true)
})

test('SSH config parser reports wildcard entries as skipped and retains valid identities', () => {
  const items = previewSshConfig(
    'Host *\n  User ignored\nHost bastion\n  HostName jump.example.test\n  User deploy\n  Port 2201\n  IdentityFile ~/.ssh/deploy'
  )
  assert.equal(items.length, 1)
  assert.equal(items[0]?.status, 'ready')
  assert.equal(items[0]?.input?.privateKeyPath?.endsWith(path.join('.ssh', 'deploy')), true)
})

test('parses all 17 representative external connection files without relying on a default SSH port', () => {
  const fixtures = Array.from({ length: 17 }, (_, index) => ({
    name: `服务器 ${index + 1}`,
    host: `node-${index + 1}.example.test`,
    port: index % 3 === 0 ? 2200 + index : 22,
    user_name: `operator${index + 1}`,
    conection_type: 'ssh',
    authentication_type: index % 2 ? 'password' : 'keyboard-interactive'
  }))
  const items = fixtures.flatMap((fixture, index) =>
    previewExternalConnectionJson(JSON.stringify(fixture), `fixture-${index}`)
  )
  assert.equal(items.length, 17)
  assert.equal(
    items.every((item) => item.status === 'ready'),
    true
  )
  assert.equal(items[0]?.port, 2200)
})
