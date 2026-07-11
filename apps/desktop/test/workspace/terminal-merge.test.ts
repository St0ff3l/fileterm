import assert from 'node:assert/strict'
import test from 'node:test'
import {
  TERMINAL_OUTPUT_FLUSH_INTERVAL_MS,
  TerminalOutputBatcher
} from '../../src/main/services/workspace/terminal-output-batcher.ts'

type EmittedChunk = {
  tabId: string
  chunk: string
}

function createHarness() {
  const emitted: EmittedChunk[] = []
  const batcher = new TerminalOutputBatcher((tabId, chunk) => {
    emitted.push({ tabId, chunk })
  })

  return { batcher, emitted }
}

test('combines high-frequency output into one emission after 16ms', (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const { batcher, emitted } = createHarness()

  assert.equal(TERMINAL_OUTPUT_FLUSH_INTERVAL_MS, 16)
  batcher.queue('tab-a', 'hello')
  batcher.queue('tab-a', ' ')
  batcher.queue('tab-a', 'world')

  context.mock.timers.tick(TERMINAL_OUTPUT_FLUSH_INTERVAL_MS - 1)
  assert.deepEqual(emitted, [])

  context.mock.timers.tick(1)
  assert.deepEqual(emitted, [{ tabId: 'tab-a', chunk: 'hello world' }])
})

test('keeps output buffers and flushes isolated per tab', (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const { batcher, emitted } = createHarness()

  batcher.queue('tab-a', 'alpha')
  batcher.queue('tab-b', 'bravo')
  batcher.queue('tab-a', '-one')
  batcher.flush('tab-a')

  assert.deepEqual(emitted, [{ tabId: 'tab-a', chunk: 'alpha-one' }])

  context.mock.timers.tick(TERMINAL_OUTPUT_FLUSH_INTERVAL_MS)
  assert.deepEqual(emitted, [
    { tabId: 'tab-a', chunk: 'alpha-one' },
    { tabId: 'tab-b', chunk: 'bravo' }
  ])
})

test('flush clears the pending timer and buffer before the next cycle', (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const { batcher, emitted } = createHarness()

  batcher.queue('tab-a', 'first')
  batcher.flush('tab-a')
  context.mock.timers.tick(TERMINAL_OUTPUT_FLUSH_INTERVAL_MS)

  assert.deepEqual(emitted, [{ tabId: 'tab-a', chunk: 'first' }])

  batcher.queue('tab-a', 'second')
  context.mock.timers.tick(TERMINAL_OUTPUT_FLUSH_INTERVAL_MS)
  assert.deepEqual(emitted, [
    { tabId: 'tab-a', chunk: 'first' },
    { tabId: 'tab-a', chunk: 'second' }
  ])
})

test('flushAll emits every pending tab once and cancels their timers', (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const { batcher, emitted } = createHarness()

  batcher.queue('tab-a', 'alpha')
  batcher.queue('tab-b', 'bravo')
  batcher.flushAll()

  assert.deepEqual(emitted, [
    { tabId: 'tab-a', chunk: 'alpha' },
    { tabId: 'tab-b', chunk: 'bravo' }
  ])

  context.mock.timers.tick(TERMINAL_OUTPUT_FLUSH_INTERVAL_MS)
  assert.equal(emitted.length, 2)
})

test('uses one timer window per tab and dispose cancels pending work', (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const { batcher, emitted } = createHarness()

  batcher.queue('tab-a', 'first')
  context.mock.timers.tick(8)
  batcher.queue('tab-a', 'second')
  context.mock.timers.tick(7)
  assert.deepEqual(emitted, [])

  context.mock.timers.tick(1)
  assert.deepEqual(emitted, [{ tabId: 'tab-a', chunk: 'firstsecond' }])

  batcher.queue('tab-b', 'discarded')
  batcher.dispose()
  batcher.dispose()
  batcher.queue('tab-c', 'ignored')
  context.mock.timers.tick(TERMINAL_OUTPUT_FLUSH_INTERVAL_MS)

  assert.deepEqual(emitted, [{ tabId: 'tab-a', chunk: 'firstsecond' }])
})
