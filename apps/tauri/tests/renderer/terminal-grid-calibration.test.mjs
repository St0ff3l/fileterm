import assert from 'node:assert/strict'
import test from 'node:test'
import { registerComparisonGridCalibration } from '../browser/helpers/terminal-grid-calibration.mjs'

function fixture(backend, width = 8.4, dpr = 1) {
  const measure = { width }
  const browser = { dpr }
  const terminal = { cols: 90, rows: 10, refresh() {}, options: { fontSize: 14, fontFamily: 'unchanged' } }
  let fail = false
  const renderer = {
    dimensions: { css: { cell: {}, canvas: {} }, device: { cell: {} } },
    _updateDimensions() {
      if (fail) throw new Error('measure failed')
      const width = backend === 'dom' ? measure.width * browser.dpr : Math.floor(measure.width * browser.dpr)
      this.dimensions.device.cell.width = width
      this.dimensions.css.canvas.width = Math.round((width * terminal.cols) / browser.dpr)
      this.dimensions.css.canvas.height = 180
      this.dimensions.css.cell.width = width / browser.dpr
    },
    handleCharSizeChanged() {
      this._updateDimensions()
    },
    _setDefaultSpacing() {}
  }
  terminal._core = {
    _charSizeService: measure,
    _coreBrowserService: browser,
    _renderService: { _renderer: { value: renderer } }
  }
  renderer._updateDimensions()
  return { terminal, renderer, measure, browser, setFail: () => (fail = true) }
}

test('experimental grid follows WebGL pixels without replacing the font measurement or options', () => {
  for (const dpr of [1, 1.1, 1.25, 1.5, 2]) {
    const dom = fixture('dom', 8.4, dpr)
    const gl = fixture('webgl', 8.4, dpr)
    const before = dom.renderer._updateDimensions
    const calibration = registerComparisonGridCalibration(dom.terminal, 'dom')
    registerComparisonGridCalibration(gl.terminal, 'webgl')
    assert.equal(dom.measure.width, 8.4)
    assert.ok(Math.abs(dom.renderer.dimensions.device.cell.width - gl.renderer.dimensions.device.cell.width) < 1e-9)
    assert.equal(dom.renderer.dimensions.css.cell.width, gl.renderer.dimensions.css.cell.width)
    assert.deepEqual(dom.terminal.options, { fontSize: 14, fontFamily: 'unchanged' })
    dom.measure.width = 10.2
    dom.renderer.handleCharSizeChanged()
    assert.ok(Math.abs(dom.renderer.dimensions.device.cell.width - Math.floor(10.2 * dpr)) < 1e-9)
    assert.equal(dom.measure.width, 10.2)
    calibration.dispose()
    assert.equal(dom.renderer._updateDimensions, before)
    assert.equal(dom.measure.width, 10.2)
    calibration.dispose()
  }
})

test('failed dimension updates restore the raw measurement and remove the experimental hook', () => {
  const state = fixture('dom')
  const before = state.renderer._updateDimensions
  state.setFail()
  assert.throws(() => registerComparisonGridCalibration(state.terminal, 'dom'), /measure failed/)
  assert.equal(state.measure.width, 8.4)
  assert.equal(state.renderer._updateDimensions, before)
  assert.throws(() => registerComparisonGridCalibration({}, 'dom'), /Unsupported xterm/)
})
