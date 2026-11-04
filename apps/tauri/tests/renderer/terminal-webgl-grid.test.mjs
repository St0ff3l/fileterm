import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const atlasCache = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/xterm-adapters/terminal-webgl-atlas-cache.ts', import.meta.url), 'utf8'),
    { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
  ).outputText,
  { exports: atlasCache }
)
const textureHooks = []
let failDisplay = false
let baselineDisposals = 0
const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/xterm-adapters/terminal-webgl-grid.ts', import.meta.url), 'utf8'),
    { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
  ).outputText,
  {
    exports,
    require: (name) => {
      if (name === './terminal-webgl-atlas-cache') return atlasCache
      if (name === './terminal-webgl-graphic-glyphs') return { registerWebglGraphicGlyphs() {} }
      if (name === './terminal-webgl-text-baseline')
        return {
          createWebglTextBaseline: () => ({
            refresh() {},
            dispose() {
              baselineDisposals++
            }
          })
        }
      if (name === './terminal-webgl-display-grid')
        return {
          createWebglDisplayGrid: () => {
            if (failDisplay) throw new Error('unsupported display')
            return { refresh() {}, releaseGlyph() {}, dispose() {} }
          }
        }
      return {
        registerWebglTextureSlots: (_, glyph) => {
          const hook = {
            glyph,
            disposed: false,
            dispose() {
              this.disposed = true
            }
          }
          textureHooks.push(hook)
          return hook
        }
      }
    }
  }
)
const { registerTerminalWebglGrid } = exports

test('opaque atlas mode follows live theme alpha, rebuilds all rows on switches, and restores the original option', () => {
  const operations = []
  const background = { rgba: 0xffffffff }
  const gl = {
    canvas: { width: 800, height: 600 },
    COLOR_BUFFER_BIT: 0x4000,
    clear(mask) {
      assert.equal(mask, this.COLOR_BUFFER_BIT)
      operations.push('clear-buffer')
    },
    blendFuncSeparate() {}
  }
  const renderer = {
    _gl: gl,
    _setCanvasDevicePixelDimensions() {},
    _glyphRenderer: { value: { handleResize() {} } },
    _clearModel(glyphs) {
      assert.equal(glyphs, true)
      operations.push('clear-model')
    },
    handleCharSizeChanged() {},
    renderRows(start, end) {
      operations.push([start, end])
    }
  }
  const terminal = {
    rows: 24,
    options: { allowTransparency: true },
    _core: { _themeService: { colors: { background } }, _renderService: { _renderer: { value: renderer } } }
  }
  const adapter = registerTerminalWebglGrid(terminal)
  renderer.renderRows(8, 8)
  assert.equal(terminal.options.allowTransparency, false)
  assert.deepEqual(operations.splice(0), ['clear-model', [0, 23]])
  renderer.renderRows(8, 8)
  assert.deepEqual(operations.splice(0), [[8, 8]], 'unchanged alpha must not rebuild the atlas')
  background.rgba = 0xffffff66
  renderer.renderRows(8, 8)
  assert.equal(terminal.options.allowTransparency, true)
  assert.deepEqual(operations.splice(0), ['clear-model', 'clear-buffer', [0, 23]])
  renderer.renderRows(8, 8)
  assert.deepEqual(operations.splice(0), ['clear-buffer', [8, 8]])
  background.rgba = 0x000000ff
  renderer.renderRows(8, 8)
  assert.equal(terminal.options.allowTransparency, false)
  assert.deepEqual(operations.splice(0), ['clear-model', [0, 23]])
  adapter.dispose()
  assert.equal(terminal.options.allowTransparency, true)
})

test('glyph drawing keeps RGB blending and restores coverage alpha after upstream state resets', () => {
  let factors = []
  let draws = 0
  const gl = {
    canvas: { width: 800, height: 600 },
    SRC_ALPHA: 0x0302,
    ONE_MINUS_SRC_ALPHA: 0x0303,
    ONE: 1,
    blendFuncSeparate: (...values) => {
      factors = values
    }
  }
  const draw = () => {
    assert.deepEqual(factors, [gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA, gl.ONE, gl.ONE_MINUS_SRC_ALPHA])
    draws++
  }
  const renderer = {
    _gl: gl,
    _setCanvasDevicePixelDimensions() {},
    _glyphRenderer: { value: { handleResize() {} } },
    _clearModel() {},
    handleCharSizeChanged() {},
    renderRows: draw
  }
  const adapter = registerTerminalWebglGrid({ _core: { _renderService: { _renderer: { value: renderer } } } })
  renderer.renderRows(0, 1)
  factors = [gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA, gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA]
  renderer.renderRows(0, 1)
  assert.equal(draws, 2)
  const previous = textureHooks.at(-1)
  renderer._glyphRenderer.value = { handleResize() {} }
  renderer.renderRows(0, 1)
  assert.equal(previous.disposed, true, 'context restoration releases the old texture hook')
  assert.equal(textureHooks.at(-1).glyph, renderer._glyphRenderer.value)
  const current = textureHooks.at(-1)
  renderer.renderRows(0, 1)
  assert.equal(textureHooks.at(-1), current, 'stable frames reuse the hook')
  adapter.dispose()
  assert.equal(current.disposed, true)
  assert.equal(renderer.renderRows, draw)
})

test('fractional font measurements never replace the upstream integer atlas grid', () => {
  const gl = { drawingBufferWidth: 800, drawingBufferHeight: 600 }
  let resizes = 0
  let clears = 0
  const dimensions = { device: { char: { width: 8 }, cell: { width: 8 } } }
  const resize = (width, height) => {
    gl.drawingBufferWidth = width
    gl.drawingBufferHeight = height
  }
  const renderer = {
    _gl: gl,
    dimensions,
    _setCanvasDevicePixelDimensions: resize,
    _glyphRenderer: {
      value: {
        handleResize() {
          resizes++
        }
      }
    },
    _clearModel() {
      clears++
    },
    renderRows() {},
    handleCharSizeChanged() {}
  }
  const adapter = registerTerminalWebglGrid({
    options: { letterSpacing: 0 },
    cols: 90,
    _core: {
      _charSizeService: { width: 6.59765625 },
      _coreBrowserService: { dpr: 1.25 },
      _renderService: { _renderer: { value: renderer } }
    }
  })
  assert.equal(dimensions.device.char.width, 8)
  assert.equal(dimensions.device.cell.width, 8)
  renderer._setCanvasDevicePixelDimensions(800, 600)
  assert.equal(resizes, 0)
  renderer._setCanvasDevicePixelDimensions(801, 600)
  assert.equal(resizes, 1)
  assert.equal(clears, 1)
  adapter.dispose()
  assert.equal(renderer._setCanvasDevicePixelDimensions, resize)
})

test('a failed initial WebGL resize restores the upstream method for DOM fallback', () => {
  const original = () => {}
  const failure = new Error('renderer resize failed')
  const renderer = {
    _gl: {},
    _updateDimensions: original,
    _setCanvasDevicePixelDimensions: original,
    _glyphRenderer: { value: { handleResize() {} } },
    _clearModel() {},
    renderRows() {},
    handleCharSizeChanged() {
      throw failure
    },
    dimensions: { device: { char: {}, cell: {}, canvas: {} }, css: { cell: {}, canvas: {} } }
  }
  const measure = { width: 8.4 }
  const terminal = {
    _core: {
      _charSizeService: measure,
      _coreBrowserService: { dpr: 1.25 },
      _renderService: { _renderer: { value: renderer } }
    }
  }
  assert.throws(
    () => registerTerminalWebglGrid(terminal),
    (error) => error === failure
  )
  assert.equal(renderer._updateDimensions, original)
  assert.equal(renderer._setCanvasDevicePixelDimensions, original)
  assert.equal(measure.width, 8.4)
})

test('an unsupported renderer fails before installing a grid hook', () => {
  const renderer = { _updateDimensions() {} }
  const original = renderer._updateDimensions
  assert.throws(
    () => registerTerminalWebglGrid({ _core: { _renderService: { _renderer: { value: renderer } } } }),
    /Unsupported xterm WebGL grid/
  )
  assert.equal(renderer._updateDimensions, original)
})

test('clearing a shared atlas invalidates peers once and disposed or unrelated terminals are left alone', () => {
  const shared = {}
  const makeTerminal = (atlas) => {
    let clears = 0
    let redraws = 0
    const originalClear = () => {
      clears++
    }
    const renderer = {
      _gl: {},
      _charAtlas: atlas,
      _setCanvasDevicePixelDimensions() {},
      _glyphRenderer: { value: { handleResize() {} } },
      _clearModel() {
        clears++
      },
      renderRows() {},
      handleCharSizeChanged() {}
    }
    const terminal = {
      rows: 24,
      clearTextureAtlas: originalClear,
      refresh(start, end) {
        assert.equal(start, 0)
        assert.equal(end, 23)
        redraws++
      },
      _core: { _renderService: { _renderer: { value: renderer } } }
    }
    return {
      terminal,
      originalClear,
      renderer,
      adapter: registerTerminalWebglGrid(terminal),
      counts: () => ({ clears, redraws })
    }
  }
  const first = makeTerminal(shared)
  const second = makeTerminal(shared)
  const third = makeTerminal({})
  first.terminal.clearTextureAtlas()
  assert.deepEqual(first.counts(), { clears: 1, redraws: 0 })
  assert.deepEqual(second.counts(), { clears: 1, redraws: 1 })
  assert.deepEqual(third.counts(), { clears: 0, redraws: 0 })
  second.adapter.dispose()
  assert.equal(second.terminal.clearTextureAtlas, second.originalClear)
  first.terminal.clearTextureAtlas()
  assert.deepEqual(second.counts(), { clears: 1, redraws: 1 })
  third.renderer._charAtlas = shared
  first.terminal.clearTextureAtlas()
  assert.deepEqual(third.counts(), { clears: 1, redraws: 1 }, 'font changes can switch which atlas is shared')
  first.adapter.dispose()
  third.adapter.dispose()
  assert.equal(first.terminal.clearTextureAtlas, first.originalClear)
})

test('atlas reset releases redundant backing pages, reuses the largest page and retains oversized-glyph support', () => {
  const page = (size) => ({
    canvas: { width: size, height: size },
    _glyphs: [{}],
    _usedPixels: size * size,
    clear() {
      this.clears = (this.clears || 0) + 1
    }
  })
  const small = page(512),
    merged = page(4096),
    overflow = page(512)
  const removed = []
  const atlas = {
    pages: [merged, small, overflow],
    _activePages: [small],
    _overflowSizePage: overflow,
    _cacheMap: new Map([['old', {}]]),
    _cacheMapCombined: new Map([['old', {}]]),
    _didWarmUp: true,
    _requestClearModel: false,
    _onRemoveTextureAtlasCanvas: { fire: (canvas) => removed.push(canvas) }
  }
  const budget = 32 * 1024 * 1024
  assert.equal(atlasCache.isWebglAtlasCacheFull(atlas, budget), true)
  merged._usedPixels = 100
  merged.currentRow = { y: 3000, height: 20 }
  assert.equal(
    atlasCache.isWebglAtlasCacheFull(atlas, budget),
    true,
    'packing rows consume space even when glyph bounding boxes are sparse'
  )
  delete merged.currentRow
  for (let i = 0; i < 100; i++) {
    assert.equal(atlasCache.resetWebglAtlasCache(atlas), true)
    assert.equal(merged._glyphs.length, 0)
    merged._glyphs.push({})
    merged._usedPixels = 100
  }
  assert.equal(merged._glyphs.length, 1)
  assert.equal(atlas._cacheMap.size, 0)
  assert.equal(atlas._cacheMapCombined.size, 0)
  assert.equal(atlas.pages.length, 2)
  assert.equal(small.canvas.width * small.canvas.height, 0, 'release CPU backing pixels of removed pages')
  assert.equal(atlas._activePages.length, 1)
  assert.equal(atlas._activePages[0], merged)
  assert.equal(atlas.pages.includes(overflow), true)
  assert.deepEqual(removed, [small.canvas], 'notify addon listeners once for each removed page')
  assert.equal(atlas._didWarmUp, false)
  assert.equal(atlas._requestClearModel, false)
  assert.equal(
    atlasCache.isWebglAtlasCacheFull(atlas, budget),
    false,
    'retained backing pages must not trigger clearing on every frame'
  )
  assert.equal(atlasCache.isWebglAtlasCacheFull(undefined, budget), false)
  const unknown = { pages: [{}] }
  assert.equal(atlasCache.resetWebglAtlasCache(unknown), false)
})

test('rendering recycles an oversized RGB cache before drawing without repeatedly clearing retained backing pages', () => {
  const page = { canvas: { width: 2048, height: 2048 }, _glyphs: [{}], _usedPixels: 2048 * 2048, clear() {} }
  const atlas = {
    pages: [page],
    _activePages: [page],
    _cacheMap: new Map(),
    _cacheMapCombined: new Map(),
    _didWarmUp: true,
    _requestClearModel: false
  }
  let clears = 0,
    renders = 0
  const ranges = []
  const original = (start, end) => {
    ranges.push([start, end])
    renders++
    page._glyphs.push({})
    page._usedPixels += 100
  }
  const renderer = {
    _gl: { canvas: { width: 900, height: 600 }, blendFuncSeparate() {} },
    _charAtlas: atlas,
    _setCanvasDevicePixelDimensions() {},
    _glyphRenderer: { value: { handleResize() {} } },
    _clearModel() {},
    handleCharSizeChanged() {},
    renderRows: original
  }
  const terminal = {
    rows: 24,
    clearTextureAtlas() {
      clears++
    },
    refresh() {},
    _core: { _renderService: { _renderer: { value: renderer } } }
  }
  const adapter = registerTerminalWebglGrid(terminal)
  renderer.renderRows(8, 8)
  assert.deepEqual(ranges[0], [0, 23])
  assert.equal(clears, 1)
  assert.equal(page._glyphs.length, 1)
  renderer.renderRows(0, 23)
  assert.equal(clears, 1)
  assert.equal(renders, 2)
  const largeRenderer = { ...renderer, _gl: { canvas: { width: 5000, height: 3000 } } }
  const largeTerminal = { ...terminal, _core: { _renderService: { _renderer: { value: largeRenderer } } } }
  const largeAdapter = registerTerminalWebglGrid(largeTerminal)
  page._usedPixels = 2048 * 2048
  renderer.renderRows(0, 23)
  assert.equal(clears, 1, 'a small terminal must respect the shared atlas working set of its larger peer')
  largeAdapter.dispose()
  adapter.dispose()
  assert.equal(renderer.renderRows, original)
})

test('failed display setup releases baseline listeners and texture hooks before DOM fallback', () => {
  const resize = () => {}
  const renderer = {
    _gl: {},
    _glyphRenderer: { value: { handleResize() {} } },
    _setCanvasDevicePixelDimensions: resize,
    handleCharSizeChanged() {},
    _clearModel() {},
    renderRows() {}
  }
  const disposals = baselineDisposals
  failDisplay = true
  try {
    assert.throws(
      () => registerTerminalWebglGrid({ _core: { _renderService: { _renderer: { value: renderer } } } }),
      /unsupported display/
    )
  } finally {
    failDisplay = false
  }
  assert.equal(textureHooks.at(-1).disposed, true)
  assert.equal(baselineDisposals, disposals + 1)
  assert.equal(renderer._setCanvasDevicePixelDimensions, resize)
})
