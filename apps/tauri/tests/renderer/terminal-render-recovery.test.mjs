import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function fixture() {
  let notifyMetrics
  let notifyParsed
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(
      readFileSync(new URL('../../src/renderer/xterm-adapters/terminal-render-recovery.ts', import.meta.url), 'utf8'),
      { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
    ).outputText,
    {
      exports,
      require: () => ({
        observeCanvasTextMetrics: (callback) => {
          notifyMetrics = callback
          return () => (notifyMetrics = null)
        }
      })
    }
  )
  const refreshes = []
  const terminal = {
    rows: 24,
    options: {},
    buffer: { active: { type: 'normal' } },
    clearTextureAtlas: () => refreshes.push('clear'),
    refresh: (start, end) => refreshes.push([start, end]),
    onWriteParsed: (callback) => {
      notifyParsed = callback
      return { dispose: () => (notifyParsed = null) }
    }
  }
  return {
    exports,
    terminal,
    refreshes,
    metrics: (family) => notifyMetrics?.(family),
    parsed: () => notifyParsed?.()
  }
}

test('screen switches invalidate rendering once per change and stop after disposal', () => {
  const { exports, terminal, refreshes, parsed } = fixture()
  const changes = []
  const disposable = exports.registerTerminalBufferRenderRecovery(terminal, (...args) => changes.push(args))
  parsed()
  assert.equal(refreshes.length, 0)
  terminal.buffer.active.type = 'alternate'
  parsed()
  parsed()
  terminal.buffer.active.type = 'normal'
  parsed()
  assert.deepEqual(changes, [
    ['normal', 'alternate'],
    ['alternate', 'normal']
  ])
  assert.deepEqual(refreshes, ['clear', [0, 23], 'clear', [0, 23]])
  disposable.dispose()
  terminal.buffer.active.type = 'alternate'
  parsed()
  assert.equal(changes.length, 2)
})

test('font calibration restores quoted stacks and cancels work when disposed during notification', () => {
  const { exports, terminal, metrics } = fixture()
  const families = []
  const family = '"Family, With Comma", monospace'
  let dispose = exports.observeTerminalFontMetrics(terminal, () => families.push(terminal.options.fontFamily))
  metrics(family)
  assert.deepEqual(families, [`${family}, ${family}`, family])
  assert.equal(terminal.options.fontFamily, family)
  dispose()
  metrics('Other')
  assert.equal(families.length, 2)
  dispose = exports.observeTerminalFontMetrics(terminal, () => dispose())
  metrics(family)
  assert.equal(terminal.options.fontFamily, `${family}, ${family}`, 'disposal cancels the second callback')
})

test('size repaint draws WebGL synchronously and leaves the atlas intact; DOM uses its refresh path', () => {
  const { exports, terminal, refreshes } = fixture()
  const draws = []
  terminal._core = { _renderService: { _renderer: { value: { _gl: {}, renderRows: (...args) => draws.push(args) } } } }
  exports.repaintTerminalRender(terminal)
  assert.deepEqual(draws, [[0, 23]])
  assert.deepEqual(refreshes, [])
  delete terminal._core
  exports.repaintTerminalRender(terminal)
  assert.deepEqual(refreshes, [[0, 23]])
})

test('activation recovery retries zero-sized hosts and cannot focus a tab deactivated by resize', () => {
  const { exports, terminal, refreshes } = fixture()
  let active = true
  let focused = 0
  terminal.focus = () => focused++
  const host = { getBoundingClientRect: () => ({ width: 0, height: 0 }) }
  const resize = () => (active = false)
  const recover = () =>
    exports.recoverTerminalRenderSurface(
      terminal,
      host,
      () => active,
      resize,
      () => {}
    )
  assert.equal(recover(), true)
  assert.deepEqual(refreshes, [])
  host.getBoundingClientRect = () => ({ width: 800, height: 400 })
  assert.equal(recover(), false)
  assert.deepEqual(refreshes, ['clear'])
  assert.equal(focused, 0)
})
