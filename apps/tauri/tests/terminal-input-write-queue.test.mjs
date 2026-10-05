import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../src/renderer/components/terminal-input-write-queue.ts', import.meta.url), 'utf8'),
    { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
  ).outputText,
  { exports }
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
