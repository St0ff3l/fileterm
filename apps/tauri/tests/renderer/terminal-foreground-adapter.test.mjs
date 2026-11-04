import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/xterm-adapters/terminal-foreground-adapter.ts', import.meta.url), 'utf8'),
    {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }
  ).outputText,
  { exports, Uint32Array, queueMicrotask, require: () => ({ trackRendererWork: (_phase, work) => work }) }
)

function fixture() {
  const handlers = new Map()
  let reads = 0
  let refreshes = 0
  const data = new Uint32Array([65, 0x03ffffff, 0])
  const subscribe = (event) => (callback) => {
    handlers.set(event, callback)
    return { dispose: () => handlers.delete(event) }
  }
  const buffer = {
    viewportY: 0,
    length: 1,
    getLine: () => {
      reads++
      return { length: 1, _line: { _data: data } }
    }
  }
  const adapter = exports.registerTerminalForegroundAdapter({
    rows: 1,
    buffer: { active: buffer, onBufferChange: subscribe('buffer') },
    onWriteParsed: subscribe('parsed'),
    onScroll: subscribe('scroll'),
    onResize: subscribe('resize'),
    refresh: () => refreshes++
  })
  return { adapter, handlers, data, reads: () => reads, refreshes: () => refreshes }
}

test('a parsed batch supersedes its scroll scans without waiting for animation frames', async () => {
  const f = fixture()
  for (let index = 0; index < 1000; index++) f.handlers.get('scroll')()
  assert.equal(f.reads(), 0)
  f.handlers.get('parsed')()
  assert.equal(f.data[1], 0)
  await Promise.resolve()
  assert.equal(f.reads(), 1)
  assert.equal(f.refreshes(), 1)
  f.adapter.dispose()
})

test('scroll, resize and buffer changes coalesce, and disposal cancels pending scans', async () => {
  const f = fixture()
  for (const event of ['scroll', 'resize', 'buffer']) f.handlers.get(event)()
  await Promise.resolve()
  assert.equal(f.reads(), 1)
  assert.equal(f.data[1], 0)
  f.handlers.get('scroll')()
  f.adapter.dispose()
  await Promise.resolve()
  assert.equal(f.reads(), 1)
  assert.equal(f.handlers.size, 0)
})
