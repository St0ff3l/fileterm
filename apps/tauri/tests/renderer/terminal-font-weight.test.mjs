import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

test('terminal weight defaults safely and round trips supported settings independently of font size', () => {
  const properties = new Map([
    ['--app-ui-zoom', '1.5'],
    ['--font-mono', 'monospace']
  ])
  const root = { style: { setProperty: (name, value) => properties.set(name, value) } }
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL('../../src/renderer/app/font-metrics.ts', import.meta.url), 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    {
      exports,
      require: () => ({}),
      document: { documentElement: root },
      getComputedStyle: () => ({ getPropertyValue: (name) => properties.get(name) ?? '' })
    }
  )
  assert.equal(exports.getTerminalFontWeight(), 400)
  for (const weight of [400, 500, 600]) {
    exports.setTerminalFontWeight(weight)
    assert.equal(exports.getTerminalFontWeight(), weight)
    assert.equal(properties.get('--app-ui-zoom'), '1.5')
    assert.equal(properties.get('--font-mono'), 'monospace')
  }
  for (const invalid of ['garbage', '700', '-1', 'NaN']) {
    properties.set('--terminal-font-weight', invalid)
    assert.equal(exports.getTerminalFontWeight(), 400)
  }
})

test('font metrics load the selected and bold faces again when terminal weight changes', () => {
  const properties = new Map()
  const loads = []
  let onMutation
  const exports = {}
  const root = { style: { setProperty: (name, value) => properties.set(name, value) } }
  const query = { addEventListener() {}, removeEventListener() {} }
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL('../../src/renderer/app/font-metrics.ts', import.meta.url), 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    {
      exports,
      require: (name) =>
        name.includes('app-events')
          ? { APP_EVENT: {}, onAppEvent: () => () => {} }
          : { MONO_FONT_FALLBACK: 'monospace' },
      document: {
        documentElement: root,
        fonts: {
          load: (font) => {
            loads.push(font)
            return Promise.resolve()
          },
          ready: Promise.resolve(),
          addEventListener() {},
          removeEventListener() {}
        }
      },
      window: {
        devicePixelRatio: 1,
        matchMedia: () => query,
        addEventListener() {},
        removeEventListener() {},
        requestAnimationFrame: () => 1,
        cancelAnimationFrame() {}
      },
      MutationObserver: class {
        constructor(callback) {
          onMutation = callback
        }
        observe() {}
        disconnect() {}
      },
      getComputedStyle: () => ({ getPropertyValue: (name) => properties.get(name) ?? '' })
    }
  )
  const dispose = exports.observeCanvasTextMetrics(() => {})
  assert.ok(loads.includes('400 12px monospace'))
  assert.ok(loads.includes('700 12px monospace'))
  exports.setTerminalFontWeight(500)
  onMutation()
  assert.ok(loads.includes('500 12px monospace'))
  const count = loads.length
  onMutation()
  assert.equal(loads.length, count)
  dispose()
})
