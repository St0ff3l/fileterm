import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function fixture() {
  const exports = {}
  const frames = new Map()
  let id = 0
  vm.runInNewContext(
    ts.transpileModule(
      readFileSync(new URL('../../src/renderer/components/terminal-ui-state.ts', import.meta.url), 'utf8'),
      { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
    ).outputText,
    {
      exports,
      window: {
        requestAnimationFrame: (fn) => {
          frames.set(++id, fn)
          return id
        },
        cancelAnimationFrame: (id) => frames.delete(id)
      },
      require: () => ({ trackRendererWork: (_phase, fn) => fn })
    }
  )
  const calls = []
  const findOpenRef = { current: true }
  const ui = exports.createTerminalUiState({
    findOpenRef,
    setHasSelection: (value) => calls.push(['selection', value]),
    setFindMatchCount: (value) => calls.push(['count', value]),
    setActiveFindIndex: (value) => calls.push(['index', value]),
    setFindMiss: (value) => calls.push(['miss', value])
  })
  const flush = () => {
    const pending = [...frames.values()]
    frames.clear()
    pending.forEach((fn) => fn())
  }
  return { ui, calls, frames, flush, findOpenRef }
}

test('xterm event bursts publish only the latest UI state outside the emitter stack', () => {
  const f = fixture()
  for (let index = 0; index < 1000; index++) {
    f.ui.selection(index % 2 === 0)
    f.ui.search({ resultIndex: index, resultCount: index + 1 })
  }
  assert.equal(f.calls.length, 0, 'no synchronous React setters inside the event callback')
  assert.equal(f.frames.size, 1)
  f.flush()
  assert.deepEqual(f.calls, [
    ['selection', false],
    ['count', 1000],
    ['index', 999],
    ['miss', false]
  ])
  f.ui.search({ resultIndex: -1, resultCount: 0 })
  f.flush()
  assert.deepEqual(f.calls.slice(-3), [
    ['count', 0],
    ['index', -1],
    ['miss', true]
  ])
})

test('disposing cancels pending state updates and ignores late xterm callbacks', () => {
  const f = fixture()
  f.ui.selection(true)
  f.ui.search({ resultIndex: 0, resultCount: 1 })
  f.ui.dispose()
  f.ui.selection(false)
  f.ui.search({ resultIndex: -1, resultCount: 0 })
  assert.equal(f.frames.size, 0)
  f.flush()
  assert.deepEqual(f.calls, [])
})

test('closing search before a frame cannot republish stale match counters', () => {
  const f = fixture()
  f.ui.search({ resultIndex: 4, resultCount: 5 })
  f.findOpenRef.current = false
  f.flush()
  assert.deepEqual(f.calls, [])
  f.findOpenRef.current = true
  f.ui.search({ resultIndex: 0, resultCount: 1 })
  f.flush()
  assert.deepEqual(f.calls, [
    ['count', 1],
    ['index', 0],
    ['miss', false]
  ])
})
