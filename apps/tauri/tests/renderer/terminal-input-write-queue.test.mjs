import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/components/terminal-input-write-queue.ts', import.meta.url), 'utf8'),
    { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
  ).outputText,
  { exports, TextEncoder }
)

const { createTerminalInputWriteQueue } = exports

test('terminal input keeps event order when an earlier Tauri invoke is delayed', async () => {
  const started = []
  const completed = []
  let releaseFirst
  const firstWriteGate = new Promise((resolve) => {
    releaseFirst = resolve
  })
  const queue = createTerminalInputWriteQueue({
    getTabId: () => 'terminal-a',
    write: async (tabId, data) => {
      started.push(`${tabId}:${data}`)
      if (data === 'A') await firstWriteGate
      completed.push(data)
    }
  })

  const writes = [queue.write('A'), queue.write('b'), queue.write('7')]
  await new Promise(setImmediate)
  assert.deepEqual(started, ['terminal-a:A'])

  releaseFirst()
  await Promise.all(writes)
  assert.deepEqual(started, ['terminal-a:A', 'terminal-a:b', 'terminal-a:7'])
  assert.deepEqual(completed, ['A', 'b', '7'])
})

test('queued input retains its tab and stops after a terminal write fails', async () => {
  const writes = []
  const failures = []
  let tabId = 'terminal-a'
  let failed = false
  const queue = createTerminalInputWriteQueue({
    getTabId: () => tabId,
    shouldStop: () => failed,
    write: async (targetTabId, data) => {
      writes.push(`${targetTabId}:${data}`)
      if (data === 'x') throw new Error('synthetic write failure')
    }
  })

  const first = queue.write('x', (error) => {
    failures.push(error.message)
    failed = true
  })
  tabId = 'terminal-b'
  await Promise.all([first, queue.write('y')])

  assert.deepEqual(writes, ['terminal-a:x'])
  assert.deepEqual(failures, ['synthetic write failure'])
})

test('byte budget is shared across terminals, reserves controls and recovers after completion', async () => {
  let release
  const gate = new Promise((resolve) => (release = resolve))
  const received = []
  const failures = []
  const first = createTerminalInputWriteQueue({ getTabId: () => 'a', write: () => gate })
  const second = createTerminalInputWriteQueue({
    getTabId: () => 'b',
    write: async (_, data) => received.push(data)
  })
  const busy = (error) => failures.push(error.message)
  const pending = first.write('中'.repeat(349525)) // 1 MiB minus one byte in UTF-8.
  await second.write('文', busy)
  await second.write('\x03', busy)
  assert.deepEqual(received, ['\x03'])
  assert.equal(failures.length, 1)
  assert.match(failures[0], /backlog full/)
  release()
  await pending
  await second.write('recovered', busy)
  assert.deepEqual(received, ['\x03', 'recovered'])
})

test('oversized paste is rejected whole and disposal/failure returns input capacity', async () => {
  const received = []
  const failures = []
  const queue = createTerminalInputWriteQueue({
    getTabId: () => 'a',
    write: async (_, data) => received.push(data)
  })
  await queue.write('文'.repeat(400000), (error) => failures.push(error.message))
  assert.equal(failures.length, 1)
  assert.deepEqual(received, [])
  const pending = queue.write('x'.repeat(1024 * 1024))
  queue.dispose()
  await pending
  const next = createTerminalInputWriteQueue({
    getTabId: () => 'b',
    write: async () => {
      throw new Error('closed')
    }
  })
  await next.write('x'.repeat(1024 * 1024), () => {
    throw new Error('report failed')
  })
  await queue.write('ignored')
  const recovered = createTerminalInputWriteQueue({
    getTabId: () => 'c',
    write: async (_, data) => received.push(data)
  })
  await recovered.write('x'.repeat(1024 * 1024))
  assert.equal(received.length, 1)
})
