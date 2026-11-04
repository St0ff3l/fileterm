import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
const sharedWidths = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/xterm-adapters/terminal-shared-glyph-widths.ts', import.meta.url), 'utf8'),
    {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }
  ).outputText,
  { exports: sharedWidths, Map, WeakMap }
)
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/xterm-adapters/terminal-render-cache.ts', import.meta.url), 'utf8'),
    {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }
  ).outputText,
  { exports, Map, Set, require: () => sharedWidths }
)
const { registerTerminalRenderCacheLimits } = exports

function contrast() {
  return {
    css: new Map(),
    color: new Map(),
    clear() {
      this.css.clear()
      this.color.clear()
    },
    setCss(bg, fg, value) {
      this.css.set(`${bg}:${fg}`, value)
    },
    setColor(bg, fg, value) {
      this.color.set(`${bg}:${fg}`, value)
    }
  }
}

test('RGB memoization stays bounded across long output, theme clear and dispose', () => {
  const cache = contrast()
  const dim = contrast()
  const methods = { clear: cache.clear, setColor: cache.setColor, setCss: cache.setCss }
  const terminal = { _core: { _themeService: { colors: { contrastCache: cache, halfContrastCache: dim } } } }
  const limits = registerTerminalRenderCacheLimits(terminal)
  for (let fg = 0; fg < 100000; fg++) {
    for (const target of [cache, dim]) {
      target.setColor(7, fg, fg)
      target.setCss(7, fg, String(fg))
      assert.ok(target.color.size <= 8192)
      assert.ok(target.css.size <= 8192)
      assert.equal(target.color.get(`7:${fg}`), fg)
      assert.equal(target.css.get(`7:${fg}`), String(fg))
    }
    if (fg % 20000 === 0) {
      cache.clear()
      dim.clear()
    }
  }
  limits.dispose()
  limits.dispose()
  for (const [name, method] of Object.entries(methods)) assert.equal(cache[name], method)
})

test('glyph width eviction preserves measuring, font cache replacement and original methods', () => {
  const widths = {
    _holey: new Map(),
    get(chars, bold, italic) {
      const key = `${chars}:${bold}:${italic}`
      if (!this._holey.has(key)) this._holey.set(key, chars.length)
      return this._holey.get(key)
    }
  }
  const get = widths.get
  const limits = registerTerminalRenderCacheLimits({
    _core: { _renderService: { _renderer: { value: { _widthCache: widths } } } }
  })
  for (let index = 0; index < 20000; index++) {
    const chars = `中文é${index}`
    assert.equal(widths.get(chars, true, false), chars.length)
    assert.ok(widths._holey.size <= 16384)
    if (index === 18000) widths._holey = new Map()
  }
  limits.dispose()
  assert.equal(widths.get, get)
})

test('unsupported renderer internals are harmless', () => {
  for (const terminal of [{}, { _core: {} }, { _core: { _themeService: { colors: { contrastCache: {} } } } }]) {
    assert.doesNotThrow(() => registerTerminalRenderCacheLimits(terminal).dispose())
  }
})
