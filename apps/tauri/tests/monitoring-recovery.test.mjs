import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function load(path, dependencies = {}) {
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL(path, import.meta.url), 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    {
      exports,
      require: (name) => {
        if (name in dependencies) return dependencies[name]
        throw new Error(`Unexpected import ${name}`)
      }
    }
  )
  return exports
}
const core = load('../../../packages/core/src/index.ts')
const { applyMonitoringUpdate, mergeMonitoringSnapshot } = load('../src/renderer/hooks/workspace-monitoring.ts', {
  '@fileterm/core': core
})
const { buildLinePath, areSampleWindowsEqual } = load('../src/renderer/features/system/network-history.ts')
const health = (generation = 2, revision = 3, phase = 'paused') => ({
  generation,
  revision,
  phase,
  attempt: 1,
  maxAttempts: 5,
  intervalSeconds: 1
})
const metrics = { networkSamples: [{ rx: 4, tx: 7 }], diskRows: [{ path: '/', usage: '1/2' }] }
const workspace = (monitoring = health()) => ({
  sessions: { tab: { connected: true, monitoring, systemMetrics: metrics } }
})

test('a status-only event preserves the last metrics and filesystem', () => {
  const current = workspace()
  const next = applyMonitoringUpdate(current, { tabId: 'tab', monitoring: health(2, 4, 'waiting') })
  assert.equal(next.sessions.tab.systemMetrics, metrics)
  assert.equal(next.sessions.tab.monitoring.phase, 'waiting')
})

test('stale, duplicate, old-generation and unversioned updates cannot overwrite monitoring', () => {
  const current = workspace()
  for (const monitoring of [health(2, 2), health(2, 3), health(1, 99), undefined]) {
    assert.equal(applyMonitoringUpdate(current, { tabId: 'tab', monitoring, systemMetrics: {} }), current)
  }
})

test('events after close/disconnect and before new session hydration are ignored', () => {
  for (const current of [{ sessions: {} }, { sessions: { tab: { connected: false } } }, workspace(undefined)]) {
    if (current.sessions.tab?.monitoring) delete current.sessions.tab.monitoring
    assert.equal(
      applyMonitoringUpdate(current, { tabId: 'tab', monitoring: health(), systemMetrics: metrics }),
      current
    )
  }
})

test('old full snapshots preserve newer metrics but still apply disconnection', () => {
  const current = workspace(health(2, 5, 'healthy'))
  const incoming = workspace(health(2, 3, 'paused'))
  incoming.sessions.tab.connected = false
  incoming.sessions.tab.systemMetrics = undefined
  const result = mergeMonitoringSnapshot(current, incoming)
  assert.equal(result.sessions.tab.connected, false)
  assert.equal(result.sessions.tab.systemMetrics, metrics)
  assert.equal(result.sessions.tab.monitoring.revision, 5)
})

test('new session snapshot discards old generation data', () => {
  const incoming = workspace(health(3, 0, 'starting'))
  incoming.sessions.tab.systemMetrics = undefined
  const next = mergeMonitoringSnapshot(workspace(), incoming)
  assert.equal(next.sessions.tab.monitoring.generation, 3)
  assert.equal(next.sessions.tab.systemMetrics, undefined)
})

test('recovery preserves history and marks its discontinuity', () => {
  const sample = { rx: 5, tx: 8, breakBefore: true, sampledAt: 12345 }
  const next = applyMonitoringUpdate(workspace(), {
    tabId: 'tab',
    monitoring: health(2, 4, 'healthy'),
    mode: 'append',
    systemMetrics: { networkSamples: [sample] }
  })
  assert.equal(next.sessions.tab.systemMetrics.networkSamples.length, 2)
  assert.equal(next.sessions.tab.systemMetrics.networkSamples[1].breakBefore, true)
  assert.equal(next.sessions.tab.systemMetrics.diskRows, metrics.diskRows)
  const path = buildLinePath(next.sessions.tab.systemMetrics.networkSamples, 'rx', 10)
  assert.equal((path.match(/M /g) ?? []).length, 2)
  assert.equal(path.includes(' C '), false)
})

test('equal rates with a new timestamp or gap are still a new sample', () => {
  assert.equal(areSampleWindowsEqual([{ rx: 1, tx: 2, sampledAt: 1 }], [{ rx: 1, tx: 2, sampledAt: 2 }]), false)
  assert.equal(areSampleWindowsEqual([{ rx: 1, tx: 2 }], [{ rx: 1, tx: 2, breakBefore: true }]), false)
})
