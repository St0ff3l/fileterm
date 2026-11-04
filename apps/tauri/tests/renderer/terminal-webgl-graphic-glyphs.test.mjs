import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const draws = []
const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(
      new URL('../../src/renderer/xterm-adapters/terminal-webgl-graphic-glyphs.ts', import.meta.url),
      'utf8'
    ),
    { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
  ).outputText,
  {
    exports,
    require: () => ({
      isGraphicGlyph: (code) => code >= 0x2500 && code <= 0x259f,
      drawTerminalGraphicGlyph: (...args) => draws.push(args.slice(1))
    })
  }
)

test('graphic glyphs share DOM geometry, preserve upstream offsets/styles, and do not replace normal text', () => {
  const operations = []
  const context = {
    save: () => operations.push('save'),
    restore: () => operations.push('restore'),
    translate: (...args) => operations.push(args),
    fillText: (...args) => operations.push(['text', ...args])
  }
  const atlas = {
    _config: {
      customGlyphs: true,
      devicePixelRatio: 1.25,
      deviceCellWidth: 8,
      deviceCellHeight: 18,
      deviceCharHeight: 18
    },
    _tmpCtx: context,
    _getFromCacheMap(map, code) {
      let glyph = map.get(code)
      if (glyph === undefined) {
        glyph = this._drawToCache(code)
        map.set(code, glyph)
      }
      return glyph
    },
    _findGlyphBoundingBox: (...args) => args[4],
    _drawToCache(code, fail) {
      assert.equal(this._config.customGlyphs, code === 65)
      context.fillText(String.fromCharCode(code), 4, 22)
      if (fail) throw new Error('raster failed')
      return this._findGlyphBoundingBox(null, null, 20, false, false, 4)
    }
  }
  const { registerWebglGraphicGlyphs } = exports
  registerWebglGraphicGlyphs(atlas)
  const patched = atlas._drawToCache
  registerWebglGraphicGlyphs(atlas)
  assert.equal(atlas._drawToCache, patched, 'a shared atlas must only be wrapped once')
  assert.equal(atlas._drawToCache(0x2500), true)
  assert.deepEqual(draws.splice(0), [[0x2500, 4, 8, 18, 2]])
  assert.deepEqual(operations.splice(0), ['save', [0, 4], 'restore'])
  assert.equal(atlas._config.customGlyphs, true)
  assert.throws(() => atlas._drawToCache(0x2588, true), /raster failed/)
  assert.equal(atlas._config.customGlyphs, true)
  const cache = new Map()
  exports.setWebglGraphicCellSize(atlas, { width: 9, height: 19 })
  assert.equal(atlas._getFromCacheMap(cache, 0x2500), true)
  assert.deepEqual(draws.splice(0).at(-1), [0x2500, 4, 9, 19, 2])
  atlas._getFromCacheMap(cache, 0x2500)
  assert.equal(draws.length, 0, 'the same size reuses the glyph')
  exports.setWebglGraphicCellSize(atlas, { width: 8, height: 18 })
  atlas._getFromCacheMap(cache, 0x2500)
  assert.equal(cache.size, 2, 'floor/ceil sizes have separate cache entries')
  assert.equal(atlas._config.deviceCellWidth, 8)
  assert.equal(atlas._config.deviceCellHeight, 18)
  exports.setWebglGraphicCellSize(atlas)
  assert.equal(atlas._drawToCache(65), false)
  assert.deepEqual(operations.at(-1), ['text', 'A', 4, 22])
})

test('an unknown atlas fails before changing the raster methods', () => {
  const atlas = { _drawToCache() {} }
  const original = atlas._drawToCache
  assert.throws(() => exports.registerWebglGraphicGlyphs(atlas), /Unsupported xterm/)
  assert.equal(atlas._drawToCache, original)
})
