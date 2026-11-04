import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const maps = []
class TrackedMap extends Map {
  constructor(...args) {
    super(...args)
    maps.push(this)
  }
}
const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/xterm-adapters/terminal-shared-glyph-widths.ts', import.meta.url), 'utf8'),
    {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }
  ).outputText,
  { exports, Map: TrackedMap, WeakMap }
)
const { registerSharedTerminalGlyphWidths } = exports

function createDocument() {
  const listeners = new Map()
  return {
    listeners,
    defaultView: {
      devicePixelRatio: 1,
      getComputedStyle: (node) => ({ getPropertyValue: (name) => node.styles[name] || '' })
    },
    fonts: {
      addEventListener: (name, listener) => listeners.set(name, listener),
      removeEventListener: (name, listener) => {
        assert.equal(listeners.get(name), listener)
        listeners.delete(name)
      }
    }
  }
}
function fixture(document, family = 'Mono', size = 14, measure = (chars, variant) => chars.length * size + variant) {
  let calls = 0
  let clears = 0
  const cache = {
    _measureElements: Array.from({ length: 4 }, (_, variant) => ({
      ownerDocument: document,
      closest: () => null,
      styles: {
        'font-family': family,
        'font-size': `${size}px`,
        'font-weight': variant & 1 ? '700' : '400',
        'font-style': variant & 2 ? 'italic' : 'normal'
      }
    })),
    _measure(chars, variant) {
      calls++
      return measure(chars, variant)
    },
    clear() {
      clears++
    }
  }
  const original = { measure: cache._measure, clear: cache.clear }
  const terminal = {
    element: { ownerDocument: document },
    _core: { _renderService: { _renderer: { value: { _widthCache: cache } } } }
  }
  const dispose = registerSharedTerminalGlyphWidths(terminal).dispose
  return {
    cache,
    original,
    dispose,
    get calls() {
      return calls
    },
    get clears() {
      return clears
    }
  }
}

test('matching panes share measurements, differing fonts, sizes, variants and documents do not', () => {
  const document = createDocument()
  const a = fixture(document),
    b = fixture(document)
  assert.equal(a.cache._measure('é', 0), 28)
  assert.equal(b.cache._measure('é', 0), 28)
  assert.equal(b.calls, 0)
  b.cache._measure('é', 1)
  assert.equal(b.calls, 1)
  const others = [fixture(document, 'Other'), fixture(document, 'Mono', 24), fixture(createDocument())]
  for (const other of others) {
    other.cache._measure('é', 0)
    assert.equal(other.calls, 1)
  }
  document.defaultView.devicePixelRatio = 2
  b.cache.clear()
  b.cache._measure('é', 0)
  assert.equal(b.calls, 2)
  assert.equal(b.clears, 1)
  for (const runtime of [a, b, ...others]) runtime.dispose()
  assert.equal(document.listeners.size, 0)
})

test('font load, error and explicit calibration clear invalidate memoized widths; last dispose releases listeners', () => {
  const document = createDocument()
  const a = fixture(document),
    b = fixture(document)
  a.cache._measure('中', 0)
  b.cache._measure('中', 0)
  assert.equal(b.calls, 0)
  for (const event of ['loadingdone', 'loadingerror']) {
    document.listeners.get(event)()
    a.cache._measure('中', 0)
  }
  assert.equal(a.calls, 3)
  a.cache.clear()
  a.cache._measure('中', 0)
  assert.equal(a.calls, 4)
  a.dispose()
  a.dispose()
  assert.equal(document.listeners.size, 2)
  assert.equal(a.cache._measure, a.original.measure)
  assert.equal(a.cache.clear, a.original.clear)
  b.dispose()
  assert.equal(document.listeners.size, 0)
})

test('shared widths stay bounded', () => {
  const document = createDocument()
  const runtime = fixture(document)
  const fonts = maps.at(-1)
  for (let index = 0; index < 40000; index++) {
    runtime.cache._measure(String(index), 0)
    if (index % 1000 === 0) assert.ok([...fonts.values()].reduce((sum, widths) => sum + widths.size, 0) <= 32768)
  }
  assert.ok([...fonts.values()].reduce((sum, widths) => sum + widths.size, 0) <= 32768)
  runtime.dispose()
  assert.equal(fonts.size, 0)
  assert.doesNotThrow(() => registerSharedTerminalGlyphWidths({}).dispose())
})

test('zero or invalid measurements and long combined strings are never retained', () => {
  const values = [0, NaN, Infinity, 5]
  const runtime = fixture(createDocument(), 'Mono', 14, () => values.shift() ?? 5)
  for (const expected of [0, NaN, Infinity, 5, 5]) assert.equal(runtime.cache._measure('x', 0), expected)
  assert.equal(runtime.calls, 4)
  const long = 'a' + '\u0301'.repeat(100)
  runtime.cache._measure(long, 0)
  runtime.cache._measure(long, 0)
  assert.equal(runtime.calls, 6)
  runtime.dispose()
})

test('unknown DOM renderer cache shapes leave native methods untouched', () => {
  const document = createDocument()
  for (const elements of [undefined, { length: 4 }, [null, null, null, null]]) {
    const cache = { _measureElements: elements, _measure() {}, clear() {} }
    const original = cache._measure
    const terminal = {
      element: { ownerDocument: document },
      _core: { _renderService: { _renderer: { value: { _widthCache: cache } } } }
    }
    assert.doesNotThrow(() => registerSharedTerminalGlyphWidths(terminal).dispose())
    assert.equal(cache._measure, original)
    assert.equal(document.listeners.size, 0)
  }
})
