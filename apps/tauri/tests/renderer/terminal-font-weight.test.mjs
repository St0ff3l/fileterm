import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const terminalLineHeightOptions = [1, 1.05, 1.1, 1.15, 1.2, 1.25, 1.3, 1.35, 1.4, 1.45, 1.5]

function loadRendererModule(path, modules, globals = {}) {
  const exports = {}
  const source = readFileSync(new URL(`../../src/renderer/${path}`, import.meta.url), 'utf8')
  const output = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
  }).outputText

  vm.runInNewContext(output, {
    exports,
    require: (name) => modules[name] ?? {},
    ...globals
  })
  return exports
}

function fontMetricsModules(overrides = {}) {
  return {
    '@fileterm/core': {
      DEFAULT_TERMINAL_LINE_HEIGHT: 1.05,
      TERMINAL_LINE_HEIGHT_OPTIONS: terminalLineHeightOptions
    },
    './imported-fonts': { getImportedFontWeightForBold: () => null },
    '../lib/app-events': { APP_EVENT: {}, onAppEvent: () => () => {} },
    './font-stacks': { MONO_FONT_FALLBACK: 'monospace' },
    ...overrides
  }
}

test('terminal weight defaults safely and round trips supported settings independently of font size', () => {
  const properties = new Map([
    ['--app-ui-zoom', '1.5'],
    ['--font-mono', 'monospace']
  ])
  const root = { style: { setProperty: (name, value) => properties.set(name, value) } }
  const exports = loadRendererModule('app/font-metrics.ts', fontMetricsModules(), {
    document: { documentElement: root },
    getComputedStyle: () => ({ getPropertyValue: (name) => properties.get(name) ?? '' })
  })
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

test('terminal bold uses an imported bold face or falls back to synthetic bold', () => {
  const properties = new Map([
    ['--font-mono', '"Imported Mono", monospace'],
    ['--terminal-font-weight', '400']
  ])
  let importedBoldWeight = null
  const modules = fontMetricsModules({
    './imported-fonts': {
      getImportedFontWeightForBold: (family) => (family === 'Imported Mono' ? importedBoldWeight : null)
    }
  })
  const exports = loadRendererModule('app/font-metrics.ts', modules, {
    document: { documentElement: { style: { setProperty: (name, value) => properties.set(name, value) } } },
    getComputedStyle: () => ({ getPropertyValue: (name) => properties.get(name) ?? '' })
  })

  assert.equal(exports.getTerminalFontBoldWeight(), 700)
  importedBoldWeight = 600
  assert.equal(exports.getTerminalFontBoldWeight(), 600)
  importedBoldWeight = 700
  assert.equal(exports.getTerminalFontBoldWeight(), 700)
})

test('imported font CSS declares each face weight and selects the nearest bold face', () => {
  let styleElement = null
  class HTMLStyleElement {
    id = ''
    textContent = ''
  }
  const exports = loadRendererModule(
    'app/imported-fonts.ts',
    { '../lib/app-events': { APP_EVENT: {}, dispatchAppEvent() {} } },
    {
      HTMLStyleElement,
      document: {
        getElementById: () => styleElement,
        createElement: () => new HTMLStyleElement(),
        head: { appendChild: (element) => (styleElement = element) }
      },
      window: {}
    }
  )

  const font = (id, fontWeight) => ({
    id,
    family: 'Example Mono',
    fontWeight,
    format: 'ttf'
  })
  exports.registerImportedFonts([
    { font: font('regular', 400), dataUrl: 'data:font/ttf;base64,AA==' },
    { font: font('semibold', 600), dataUrl: 'data:font/ttf;base64,AA==' },
    { font: font('bold', 700), dataUrl: 'data:font/ttf;base64,AA==' }
  ])

  assert.match(styleElement.textContent, /font-weight: 400;/)
  assert.match(styleElement.textContent, /font-weight: 600;/)
  assert.match(styleElement.textContent, /font-weight: 700;/)
  assert.equal(exports.getImportedFontWeightForBold('Example Mono'), 700)
  assert.equal(exports.getImportedFontWeightForBold('Missing Mono'), null)
})

test('terminal line height only accepts the saved option values', () => {
  const properties = new Map()
  const exports = loadRendererModule('app/font-metrics.ts', fontMetricsModules(), {
    document: { documentElement: { style: { setProperty: (name, value) => properties.set(name, value) } } },
    getComputedStyle: () => ({ getPropertyValue: (name) => properties.get(name) ?? '' })
  })

  assert.equal(exports.getTerminalLineHeight(), 1.05)
  exports.setTerminalLineHeight(1.25)
  assert.equal(exports.getTerminalLineHeight(), 1.25)
  exports.setTerminalLineHeight(1.03)
  assert.equal(properties.get('--terminal-line-height'), '1.05')
  properties.set('--terminal-line-height', '1.03')
  assert.equal(exports.getTerminalLineHeight(), 1.05)
})

test('font metrics load the selected and bold faces again when terminal weight changes', () => {
  const properties = new Map()
  const loads = []
  let onMutation
  const root = { style: { setProperty: (name, value) => properties.set(name, value) } }
  const query = { addEventListener() {}, removeEventListener() {} }
  const exports = loadRendererModule('app/font-metrics.ts', fontMetricsModules(), {
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
  })
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
