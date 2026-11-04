import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
vm.runInNewContext(
  ts.transpileModule(readFileSync(new URL('../../src/bridge/events/terminal-output.ts', import.meta.url), 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
  }).outputText,
  { exports, performance }
)
const tick = () => new Promise((resolve) => setImmediate(resolve))

test('delivery acknowledges only after all listeners parse and isolates failures', async () => {
  let parsed
  const waiting = new Promise((resolve) => {
    parsed = resolve
  })
  const listeners = new Set([
    () => waiting,
    () => {
      throw new Error('broken view')
    }
  ])
  const acknowledgements = []
  const diagnostics = []
  const deliver = exports.createTerminalOutputDelivery(
    listeners,
    async (id) => {
      acknowledgements.push(id)
    },
    (message) => diagnostics.push(message)
  )
  deliver({ tabId: 'ssh', chunk: 'private terminal output', deliveryId: 8 })
  await tick()
  assert.equal(acknowledgements.length, 0)
  parsed()
  await tick()
  assert.deepEqual(acknowledgements, [8])
  assert.equal(JSON.parse(diagnostics[0]).failed, true)
  assert.equal(exports.getTerminalOutputProgress().parseFailures, 1)
  assert.equal(exports.getTerminalOutputProgress().lastSuccessfulDeliveryId, undefined)
  assert.ok(!diagnostics[0].includes('private terminal output'))
  listeners.clear()
  deliver({ tabId: 'closed', chunk: '', deliveryId: 9 })
  await tick()
  assert.deepEqual(acknowledgements, [8, 9])
})

test('ten terminal listeners receive 20000 ordered batches without retained parsing work', async () => {
  const counts = Array(10).fill(0)
  const listeners = new Set(
    counts.map((_, index) => (payload) => {
      if (payload.tabId === String(index)) {
        assert.equal(payload.chunk, String(counts[index]++))
        return Promise.resolve()
      }
    })
  )
  let completed = 0
  const deliver = exports.createTerminalOutputDelivery(
    listeners,
    async () => {
      completed++
    },
    () => {
      throw new Error('Unexpected stall')
    }
  )
  for (let sequence = 0; sequence < 20000; sequence += 4) {
    for (let offset = 0; offset < 4; offset++) {
      const id = sequence + offset
      deliver({ tabId: String(id % 10), chunk: String(Math.floor(id / 10)), deliveryId: id })
    }
    await tick()
    assert.equal(completed, sequence + 4)
  }
  assert.deepEqual(counts, Array(10).fill(2000))
})

test('untracked deliveries do not request native credit', async () => {
  let completed = 0
  const listeners = new Set([
    async () => {
      completed++
    }
  ])
  let acknowledged = false
  const deliver = exports.createTerminalOutputDelivery(
    listeners,
    async () => {
      acknowledged = true
    },
    () => {}
  )
  deliver({ tabId: 'serial', chunk: 'device response' })
  await tick()
  assert.equal(completed, 1)
  assert.equal(acknowledged, false)
})

test('saturation markers are visible and counted without logging output text', async () => {
  const received = []
  const diagnostics = []
  const deliver = exports.createTerminalOutputDelivery(
    new Set([
      async (payload) => {
        received.push(payload.chunk)
      }
    ]),
    async () => {},
    (message) => diagnostics.push(message)
  )
  deliver({ tabId: 'ssh', chunk: 'private new frame', deliveryId: 1, skippedBytesBefore: 123 })
  await tick()
  assert.ok(received[0].includes('skipped 123 bytes'))
  assert.ok(received[0].endsWith('private new frame'))
  assert.equal(JSON.parse(diagnostics[0]).skippedBytesBefore, 123)
  assert.ok(!diagnostics[0].includes('private new frame'))
})

test('progress distinguishes blocked parsing, pending ACKs and rejected ACKs without payloads', async () => {
  let finishParsing
  let rejectAck
  const deliver = exports.createTerminalOutputDelivery(
    new Set([
      () =>
        new Promise((resolve) => {
          finishParsing = resolve
        })
    ]),
    () =>
      new Promise((_, reject) => {
        rejectAck = reject
      }),
    () => {}
  )
  deliver({ tabId: 'secret-host', chunk: 'secret-output', deliveryId: 42 })
  await tick()
  let progress = exports.getTerminalOutputProgress()
  assert.equal(progress.pendingBatches, 1)
  assert.equal(progress.receivedBatches, 1)
  assert.equal(progress.parsedBatches, 0)
  assert.equal(progress.lastReceivedDeliveryId, 42)
  assert.ok(!JSON.stringify(progress).includes('secret'))
  finishParsing()
  await tick()
  progress = exports.getTerminalOutputProgress()
  assert.equal(progress.pendingBatches, 0)
  assert.equal(progress.parsedBatches, 1)
  assert.equal(progress.pendingAcknowledgements, 1)
  assert.equal(progress.lastParsedDeliveryId, 42)
  assert.equal(progress.lastSuccessfulDeliveryId, 42)
  rejectAck(new Error('offline'))
  await tick()
  progress = exports.getTerminalOutputProgress()
  assert.equal(progress.pendingAcknowledgements, 0)
  assert.equal(progress.ackFailures, 1)
  assert.equal(progress.lastAcknowledgedDeliveryId, undefined)
})

test('ACK age and round-trip timing are bounded and distinguish native reply waiting from parsing', async () => {
  const isolated = {}
  let now = 100
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL('../../src/bridge/events/terminal-output.ts', import.meta.url), 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    { exports: isolated, performance: { now: () => now } }
  )
  const completions = []
  const deliver = isolated.createTerminalOutputDelivery(
    new Set([() => {}]),
    () => new Promise((resolve) => completions.push(resolve)),
    () => {}
  )
  deliver({ tabId: 'secret', chunk: 'private output', deliveryId: 1 })
  await tick()
  now = 350
  let progress = isolated.getTerminalOutputProgress()
  assert.equal(progress.pendingBatches, 0)
  assert.equal(progress.trackedPendingAcks, 1)
  assert.equal(progress.oldestTrackedAckAgeMs, 250)
  completions.shift()()
  await tick()
  progress = isolated.getTerminalOutputProgress()
  assert.equal(progress.trackedPendingAcks, 0)
  assert.equal(progress.lastAckElapsedMs, 250)
  assert.equal(progress.maxAckElapsedMs, 250)
  for (let id = 2; id < 102; id++) deliver({ tabId: 'secret', chunk: '', deliveryId: id })
  await tick()
  progress = isolated.getTerminalOutputProgress()
  assert.equal(progress.pendingAcknowledgements, 100)
  assert.equal(progress.trackedPendingAcks, 64, 'diagnostics must not create an unbounded tracker')
  assert.ok(!JSON.stringify(progress).includes('secret'))
  completions.splice(0).forEach((resolve) => resolve())
  await tick()
  assert.equal(isolated.getTerminalOutputProgress().trackedPendingAcks, 0)
})
