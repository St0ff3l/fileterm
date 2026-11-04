import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
const sizes = []
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/xterm-adapters/terminal-webgl-display-grid.ts', import.meta.url), 'utf8'),
    { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
  ).outputText,
  {
    exports,
    require: () => ({
      isGraphicGlyph: (code) => code >= 0x2500 && code <= 0x259f,
      setWebglGraphicCellSize: (_, size) => sizes.push(size && { ...size })
    })
  }
)

test('display projection preserves integer atlas bounds and restores hooks across context replacement', () => {
  const dimensions = {
    device: {
      char: { width: 8, height: 18, left: 0, top: 0 },
      cell: { width: 8, height: 18 },
      canvas: { width: 80, height: 36 }
    },
    css: { cell: { width: 6.4, height: 14.4 }, canvas: { width: 64, height: 29 } }
  }
  const terminal = {
    cols: 10,
    rows: 2,
    options: { letterSpacing: 0, lineHeight: 1 },
    element: { ownerDocument: { defaultView: { devicePixelRatio: 1.25 } } },
    _core: { _charSizeService: { width: 6.6 } }
  }
  let fail = false
  const makeGlyph = () => ({
    _atlas: {},
    _vertices: { attributes: new Float32Array(220) },
    updateCell(x, y) {
      if (fail) throw new Error('raster failed')
      const index = (y * terminal.cols + x) * 11
      this._vertices.attributes[index] = 0
      this._vertices.attributes[index + 1] = 0
    },
    _bindAtlasPageTexture() {}
  })
  const first = makeGlyph()
  const originalUpdate = first.updateCell
  const originalBind = first._bindAtlasPageTexture
  let resized = 0
  let layerDimensions
  const atlasDimensions = []
  let failAtlas = false
  const layer = {
    _deviceCellWidth: 8,
    _deviceCellHeight: 18,
    _refreshCharAtlas() {
      atlasDimensions.push([this._deviceCellWidth, this._deviceCellHeight])
      if (failAtlas) throw new Error('atlas failed')
    },
    resize(_, d) {
      layerDimensions = d
      this._deviceCellWidth = d.device.cell.width
      this._deviceCellHeight = d.device.cell.height
      this._refreshCharAtlas()
    }
  }
  const originalLayer = layer.resize
  const originalLayerAtlas = layer._refreshCharAtlas
  const rectangle = {
    _dimensions: dimensions,
    setDimensions(d) {
      this._dimensions = d
    },
    handleResize() {
      resized++
    }
  }
  const renderer = {
    dimensions,
    _gl: { canvas: { width: 83, height: 36 }, texParameteri() {} },
    _glyphRenderer: { value: first },
    _rectangleRenderer: { value: rectangle },
    _renderLayers: [layer],
    _updateDimensions() {}
  }
  const originalDimensions = renderer._updateDimensions
  const adapter = exports.createWebglDisplayGrid(terminal, renderer)
  renderer._updateDimensions()
  assert.equal(dimensions.css.cell.width, 6.6)
  assert.equal(dimensions.device.canvas.width, 83)
  assert.equal(dimensions.device.cell.width, 8)
  assert.equal(dimensions.device.char.width, 8)
  layer.resize(terminal, dimensions)
  assert.equal(layerDimensions.device.cell.width, 8.3)
  assert.deepEqual(atlasDimensions.pop(), [8, 18], 'overlay atlas creation retains integer raster bounds')
  assert.equal(layer._deviceCellWidth, 8.3, 'link overlay drawing retains projected cells')
  failAtlas = true
  assert.throws(() => layer._refreshCharAtlas(), /atlas failed/)
  assert.equal(layer._deviceCellWidth, 8.3, 'failed atlas creation restores overlay coordinates')
  failAtlas = false
  adapter.refresh()
  assert.equal(rectangle._dimensions.device.cell.width, 8.3)
  adapter.refresh()
  assert.equal(resized, 1, 'stable frames do not rebuild rectangles')
  first.updateCell(1, 0, 0x2588)
  assert.deepEqual(sizes.splice(0), [{ width: 9, height: 18 }, undefined])
  first.updateCell(1, 0, 65)
  assert.equal(sizes.length, 0, 'text uses the unchanged upstream atlas')
  renderer._gl.canvas.height = 37
  adapter.refresh()
  for (const code of [65, 0x4e2d, 0x1f600]) {
    for (let x = 0; x < terminal.cols; x++) {
      first.updateCell(x, 1, code)
      const index = (terminal.cols + x) * 11
      const projectedX = (x * renderer._gl.canvas.width) / terminal.cols + first._vertices.attributes[index]
      assert.ok(Math.abs(projectedX - Math.round(projectedX)) < 1e-6, 'text quads start on device pixels')
      const projectedY = renderer._gl.canvas.height / terminal.rows + first._vertices.attributes[index + 1]
      assert.ok(Math.abs(projectedY - Math.round(projectedY)) < 1e-6, 'text baselines start on device pixels')
    }
  }
  fail = true
  assert.throws(() => first.updateCell(0, 0, 0x2500), /raster failed/)
  assert.equal(sizes.at(-1), undefined, 'failed rasterization releases the shared size override')
  const second = makeGlyph()
  const secondUpdate = second.updateCell
  renderer._glyphRenderer.value = second
  adapter.refresh()
  assert.equal(first.updateCell, originalUpdate)
  assert.equal(first._bindAtlasPageTexture, originalBind)
  adapter.dispose()
  assert.equal(second.updateCell, secondUpdate)
  assert.equal(renderer._updateDimensions, originalDimensions)
  assert.equal(layer.resize, originalLayer)
  assert.equal(layer._refreshCharAtlas, originalLayerAtlas)
  assert.equal(rectangle._dimensions, dimensions)
})
