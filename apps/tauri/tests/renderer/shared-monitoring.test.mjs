import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/features/system/shared-monitoring.ts', import.meta.url), 'utf8'),
    { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
  ).outputText,
  { exports }
)
const { resolveMonitoringSource } = exports

const session = (profileId, phase, extra = {}) => ({
  profileId,
  connected: true,
  capabilities: { resourceMonitoring: true },
  monitoring: { generation: 1, revision: 1, phase, targetKey: profileId, ...extra },
  systemMetrics: { ip: `${profileId}-${phase}` }
})

test('a shared session shows the samples of the tab collecting for its machine', () => {
  const sessions = {
    leader: session('xe9680', 'healthy'),
    follower: session('xe9680', 'shared', { sharedFromTabId: 'leader' }),
    other: session('r750xa', 'healthy')
  }
  const source = resolveMonitoringSource('follower', sessions)
  assert.equal(source.tabId, 'leader')
  assert.equal(source.session, sessions.leader)
  assert.deepEqual(Array.from(source.peers, (peer) => peer.tabId).sort(), ['follower', 'leader'])
})

test('a collecting session resolves to itself', () => {
  const sessions = { leader: session('xe9680', 'healthy') }
  const source = resolveMonitoringSource('leader', sessions)
  assert.equal(source.tabId, 'leader')
  assert.equal(source.session, sessions.leader)
})

test('a follower whose leader is gone keeps its own state until it takes over', () => {
  const sessions = {
    leader: { ...session('xe9680', 'healthy'), connected: false },
    follower: session('xe9680', 'shared', { sharedFromTabId: 'leader' })
  }
  const source = resolveMonitoringSource('follower', sessions)
  assert.equal(source.tabId, 'follower')
  assert.deepEqual(
    Array.from(source.peers, (peer) => peer.tabId),
    ['follower']
  )
})

test('no active tab resolves to nothing', () => {
  const source = resolveMonitoringSource(null, {})
  assert.equal(source.session, null)
  assert.equal(source.peers.length, 0)
})

test('editing a profile to another target does not share samples or monitor controls', () => {
  const sessions = {
    old: session('same-profile', 'healthy', { targetKey: 'old-host' }),
    current: session('same-profile', 'shared', { targetKey: 'new-host', sharedFromTabId: 'old' })
  }
  const source = resolveMonitoringSource('current', sessions)
  assert.equal(source.tabId, 'current')
  assert.deepEqual(
    Array.from(source.peers, (peer) => peer.tabId),
    ['current']
  )
})

test('a failed former leader displays the samples of its healthy replacement', () => {
  const sessions = { old: session('host', 'failed'), replacement: session('host', 'healthy') }
  const source = resolveMonitoringSource('old', sessions)
  assert.equal(source.tabId, 'replacement')
  assert.equal(source.peers.length, 2)
})

test('followers resolve a healthy replacement instead of their failed former leader', () => {
  const sessions = {
    old: session('host', 'failed'),
    follower: session('host', 'shared', { sharedFromTabId: 'old' }),
    replacement: session('host', 'healthy')
  }
  assert.equal(resolveMonitoringSource('follower', sessions).tabId, 'replacement')
})
