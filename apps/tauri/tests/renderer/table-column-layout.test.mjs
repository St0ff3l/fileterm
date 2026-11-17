import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(
      new URL('../../src/renderer/features/common/table-column-layout/column-width-storage.ts', import.meta.url),
      'utf8'
    ),
    {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }
  ).outputText,
  { exports }
)
const { columnWidthStorageKey, parseColumnWidths, readColumnWidths, writeColumnWidths } = exports
const tick = () => new Promise(setImmediate)

test('column layouts isolate profiles and stable table IDs without delimiter collisions', () => {
  assert.notEqual(columnWidthStorageKey('host-a', 'ports'), columnWidthStorageKey('host-b', 'ports'))
  assert.notEqual(columnWidthStorageKey('host-a', 'ports'), columnWidthStorageKey('host-a', 'peers'))
  assert.notEqual(columnWidthStorageKey('a:b', 'c'), columnWidthStorageKey('a', 'b:c'))
})

test('stored column widths reject malformed, stale, nonfinite and out-of-range records', () => {
  assert.deepEqual(Array.from(parseColumnWidths('[50,244.5,10000]', 3)), [50, 244.5, 10000])
  for (const raw of [null, '', 'invalid', 'null', '{}', '[60]', '[49,100]', '[60,10001]', '[60,"100"]', '[60,1e999]']) {
    assert.equal(parseColumnWidths(raw, 2), null, raw)
  }
})

test('writes remain ordered and a remounted table waits for its pending final drag before reading', async () => {
  const values = new Map()
  const gates = []
  const writes = []
  let reads = 0
  const api = {
    setUiStateItem: (key, value) =>
      new Promise((resolve) => {
        writes.push(value)
        gates.push(() => {
          values.set(key, value)
          resolve()
        })
      }),
    getUiStateItem: async (key) => {
      reads++
      return values.get(key) ?? null
    }
  }
  const first = writeColumnWidths(api, 'ordered', [100, 200])
  const second = writeColumnWidths(api, 'ordered', [180, 200])
  const restored = readColumnWidths(api, 'ordered', 2)
  await tick()
  assert.equal(writes.length, 1)
  assert.equal(reads, 0)
  gates[0]()
  await first
  await tick()
  assert.equal(writes.length, 2)
  assert.equal(reads, 0)
  gates[1]()
  await second
  assert.deepEqual(Array.from(await restored), [180, 200])
})

test('failed saves do not poison later commits and reset records restore the default layout', async () => {
  let count = 0
  let raw = null
  const api = {
    setUiStateItem: async (_key, value) => {
      if (!count++) throw Error('disk unavailable')
      raw = value
    },
    getUiStateItem: async () => raw
  }
  const first = writeColumnWidths(api, 'retry', [100, 200])
  const rejected = assert.rejects(first, /disk unavailable/)
  await writeColumnWidths(api, 'retry', [150, 200])
  await rejected
  assert.deepEqual(Array.from(await readColumnWidths(api, 'retry', 2)), [150, 200])
  await writeColumnWidths(api, 'retry', null)
  assert.equal(await readColumnWidths(api, 'retry', 2), null)
})
