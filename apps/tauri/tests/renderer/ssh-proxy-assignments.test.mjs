import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/features/proxies/ssh-proxy-assignments.ts', import.meta.url), 'utf8'),
    { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
  ).outputText,
  { exports }
)

const { proxyAssignmentRows, proxyAssignmentChanges } = exports
const profile = (id, fields) => ({ id, name: 'Example', type: 'ssh', ...fields })
const plain = (value) => JSON.parse(JSON.stringify(value))

test('only direct SSH configurations can be edited, including a first hop with a legacy field overridden', () => {
  const rows = proxyAssignmentRows(
    [
      profile('first', { jumpProfileId: 'ignored', jumpProfileIds: [] }),
      profile('later'),
      profile('target', { jumpProfileIds: ['first', 'later'] }),
      profile('sftp', { type: 'sftp', jumpProfileIds: ['first', 'later'] }),
      profile('legacy', { jumpProfileId: 'first' }),
      profile('alias', { authType: 'sshConfig' })
    ],
    'shared'
  )
  assert.deepEqual(plain(rows.map(({ profile, blocked }) => [profile.id, blocked])), [
    ['first', false],
    ['later', true],
    ['target', true],
    ['legacy', true]
  ])
})

test('submits only explicit changes by ID, even if a new linked connection appears meanwhile', () => {
  const rows = proxyAssignmentRows(
    [
      profile('custom', { proxy: { type: 'http', host: 'proxy', port: 1080, password: 'never send this' } }),
      profile('unlink', { proxyProfileId: 'shared' }),
      profile('new-link', { proxyProfileId: 'shared' }),
      profile('unrelated', { proxyProfileId: 'other' })
    ],
    'shared'
  )
  assert.deepEqual(
    plain(
      proxyAssignmentChanges(
        rows,
        new Map([
          ['custom', true],
          ['unlink', false]
        ])
      )
    ),
    [
      { profileId: 'custom', assigned: true, expected: { kind: 'custom', type: 'http', host: 'proxy', port: 1080 } },
      { profileId: 'unlink', assigned: false, expected: { kind: 'saved', proxyId: 'shared' } }
    ]
  )
})

test('does not assign a proxy to a connection with a selected outbound interface', () => {
  const rows = proxyAssignmentRows([profile('bound', { sshNetworkInterface: { adapterId: 'lan' } })], 'shared')
  assert.equal(rows[0].interfaceConflict, true)
  assert.equal(rows[0].blocked, true)
  assert.deepEqual(plain(proxyAssignmentChanges(rows, new Map([['bound', true]]))), [])
})
