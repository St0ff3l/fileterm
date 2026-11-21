import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const INSIGHTS_KEY = 'server-management.insights.v1'
const HISTORY_KEY = 'server-management.connection-history.v1'

const metrics = {
  identity: { osName: 'Debian 13', architecture: 'x86_64', hostname: 'debian-host', kernelVersion: '6.1.0-18-amd64' },
  uptime: '3 days, 4 hours',
  uptimeSeconds: 273600,
  cpuInfoRows: [{ model: 'AMD EPYC 7B13', cores: 8 }],
  memoryBreakdown: { total: '15.62 GB' },
  fileSystemRows: [{ mountPoint: '/', used: '120 GB', size: '240 GB', usagePercent: '50%' }],
  cpuPercent: 12,
  memoryPercent: 35
}

function fixture(seed = {}) {
  const exports = {}
  const writes = []
  const reads = { ...seed }
  let nextTimerId = 0
  vm.runInNewContext(
    ts.transpileModule(
      readFileSync(
        new URL('../../src/renderer/features/server-management/server-insights-store.ts', import.meta.url),
        'utf8'
      ),
      { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
    ).outputText,
    {
      exports,
      window: {
        fileterm: {
          getUiStateItem: async (key) => reads[key] ?? null,
          setUiStateItem: async (key, value) => {
            reads[key] = value
            writes.push([key, value])
          }
        }
      },
      setTimeout: () => ++nextTimerId,
      clearTimeout: () => {}
    }
  )
  return { exports, writes, reads }
}

function tick() {
  return new Promise((resolve) => setImmediate(resolve))
}

test('records a server configuration snapshot and persists it for later sessions', async () => {
  const first = fixture()
  await first.exports.hydrateServerInsights()
  first.exports.recordServerInsight('profile-1', metrics, Date.parse('2026-11-19T12:53:00Z'))
  first.exports.flushServerInsights()
  await tick()
  await tick()

  const insight = first.exports.getServerInsight('profile-1')
  assert.equal(insight.osName, 'Debian 13')
  assert.equal(insight.architecture, 'x86_64')
  assert.equal(insight.hostname, 'debian-host')
  assert.equal(insight.kernelVersion, '6.1.0-18-amd64')
  assert.equal(insight.uptimeSeconds, 273600)
  assert.equal(insight.cpuModel, 'AMD EPYC 7B13')
  assert.equal(insight.cpuCores, 8)
  assert.equal(insight.memoryTotal, '15.62 GB')
  assert.equal(insight.diskUsed, '120 GB')
  assert.equal(insight.diskPercent, 50)

  const write = first.writes.find(([key]) => key === INSIGHTS_KEY)
  assert.ok(write, 'insight snapshot should be written to UI state')

  const second = fixture({ [INSIGHTS_KEY]: write[1] })
  await second.exports.hydrateServerInsights()
  assert.equal(second.exports.getServerInsight('profile-1').uptime, '3 days, 4 hours')
  assert.equal(second.exports.getServerInsight('profile-1').uptimeSeconds, 273600)
})

test('keeps up to 12 connection history entries, newest first', async () => {
  const store = fixture()
  await store.exports.hydrateServerInsights()
  for (let index = 0; index < 14; index++) {
    store.exports.recordConnectionEvent('profile-1', index % 2 === 0, index % 2 === 0 ? undefined : 'auth failed')
  }
  await tick()
  await tick()

  const entries = store.exports.getConnectionHistory('profile-1')
  assert.equal(entries.length, 12)
  assert.ok(entries[0].at >= entries[1].at)
  assert.equal(entries[0].ok, false)
  assert.equal(entries[0].error, 'auth failed')

  const write = store.writes.findLast(([key]) => key === HISTORY_KEY)
  assert.ok(write, 'connection history should be written to UI state')
  assert.equal(JSON.parse(write[1])['profile-1'].length, 12)
})

test('hydrates existing history and notifies subscribers on new attempts', async () => {
  const store = fixture({
    [HISTORY_KEY]: JSON.stringify({
      'profile-1': [{ id: 'seed', at: 1, ok: true }]
    })
  })
  await store.exports.hydrateServerInsights()
  assert.deepEqual(
    Array.from(store.exports.getConnectionHistory('profile-1'), (entry) => entry.id),
    ['seed']
  )

  const notifications = []
  const unsubscribe = store.exports.subscribeServerInsights(() => notifications.push('change'))
  store.exports.recordConnectionEvent('profile-1', false)
  unsubscribe()
  assert.equal(notifications.length, 1)
  await tick()
  await tick()
})

test('ignores malformed persisted JSON without throwing', async () => {
  const store = fixture({ [INSIGHTS_KEY]: '{not json', [HISTORY_KEY]: '[]' })
  await store.exports.hydrateServerInsights()
  assert.equal(store.exports.getServerInsight('profile-1'), undefined)
  assert.equal(store.exports.getConnectionHistory('profile-1').length, 0)
})
