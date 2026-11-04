import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function load(file, globals = {}, dependencies = {}) {
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL('../../src/renderer/' + file + '.ts', import.meta.url), 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    {
      exports,
      ...globals,
      require: (name) => {
        assert.ok(name in dependencies, name)
        return dependencies[name]
      }
    }
  )
  return exports
}

function workFixture() {
  let now = 0
  const work = load('lib/renderer-work-log', { performance: { now: () => now } })
  return { work, advance: (elapsed) => (now += elapsed) }
}

test('phase timing preserves this, return values, exceptions and async parsing without retaining arguments', () => {
  const { work, advance } = workFixture()
  const receiver = {
    value: 7,
    parse: work.trackRendererWork('xterm-parse', function (text) {
      assert.equal(text, 'secret terminal contents')
      advance(20)
      return this.value
    })
  }
  assert.equal(receiver.parse('secret terminal contents'), 7)
  const failure = new Error('secret failure')
  const render = work.trackRendererWork('xterm-render', () => {
    advance(30)
    throw failure
  })
  assert.throws(render, (error) => error === failure)
  const pending = new Promise(() => {})
  assert.equal(work.trackRendererWork('xterm-parse', () => pending)(), pending)
  advance(500)
  const snapshot = work.getRendererWorkSnapshot()
  assert.equal(snapshot.schemaVersion, 3)
  assert.equal(snapshot.phases['xterm-parse'].totalElapsedMs, 20, 'async waiting is not synchronous parsing time')
  assert.equal(snapshot.phases['xterm-parse'].calls, 2)
  assert.equal(snapshot.phases['xterm-render'].maxElapsedMs, 30)
  assert.equal(snapshot.recentFailures[0].phase, 'xterm-render')
  assert.equal(snapshot.activeWork.length, 0)
  assert.ok(!JSON.stringify(snapshot).includes('secret'))
  for (let index = 0; index < 20; index++) work.trackRendererWork('symbol-layout', () => advance(20))()
  assert.equal(work.getRendererWorkSnapshot().recentSlowWork.length, 6)
  work.resetRendererWork()
  assert.equal(Object.keys(work.getRendererWorkSnapshot().phases).length, 0)
  assert.equal(work.getRendererWorkSnapshot().recentSlowWork.length, 0)
  assert.equal(work.getRendererWorkSnapshot().recentFailures.length, 0)
})

test('named callback failures survive emitter unwinding without retaining errors or payloads', () => {
  const { work } = workFixture()
  let active
  const inner = work.trackRendererWork('terminal-search-results', () => {
    active = work.getRendererWorkSnapshot().activeWork.map((entry) => entry.phase)
    throw new Error('private output')
  })
  const outer = work.trackRendererWork('xterm-parse', inner)
  assert.throws(() => outer('secret payload'), /private output/)
  assert.deepEqual([...active], ['xterm-parse', 'terminal-search-results'])
  const snapshot = work.getRendererWorkSnapshot()
  assert.equal(snapshot.activeWork.length, 0)
  assert.equal(snapshot.recentFailures[0].phase, 'terminal-search-results')
  assert.ok(!JSON.stringify(snapshot).includes('private'))
  assert.ok(!JSON.stringify(snapshot).includes('secret'))
  for (let i = 0; i < 20; i++) assert.throws(() => inner())
  assert.equal(work.getRendererWorkSnapshot().recentFailures.length, 4)
  work.resetRendererWork(true)
  assert.equal(
    work.getRendererWorkSnapshot().recentFailures.length,
    4,
    'a heartbeat must not erase a pending xterm rethrow'
  )
  assert.deepEqual(Object.keys(work.getRendererWorkSnapshot().phases), [])
  work.resetRendererWork()
  assert.equal(work.getRendererWorkSnapshot().recentFailures.length, 0)
})

test('terminal resources are numeric, version guarded and released with exact methods restored', () => {
  const { work, advance } = workFixture()
  const diagnostics = load('xterm-adapters/terminal-diagnostics', {}, { '../lib/renderer-work-log': work })
  const parse = function (data) {
    assert.equal(this, parser)
    advance(10)
    return data
  }
  const parser = { parse }
  const renderRows = function () {
    assert.equal(this, renderer)
    advance(20)
  }
  const renderer = { renderRows }
  const renderListeners = new Set()
  const terminal = {
    cols: 80,
    element: { querySelector: () => ({}) },
    onRender(listener) {
      renderListeners.add(listener)
      return { dispose: () => renderListeners.delete(listener) }
    },
    buffer: { normal: { length: 100 }, alternate: { length: 20 } },
    _core: {
      _inputHandler: parser,
      _renderService: { _renderer: { value: renderer } },
      _writeBuffer: { _pendingData: 44 }
    }
  }
  const registration = diagnostics.registerTerminalDiagnostics(
    terminal,
    { current: 'secret' },
    { current: 'secret log' },
    { current: 'tab-safe' }
  )
  assert.equal(parser.parse('private response'), 'private response')
  renderer.renderRows()
  const snapshot = diagnostics.getTerminalResourceSnapshot()
  assert.equal(snapshot.cellStorageBytesEstimate, 120 * 80 * 12)
  assert.equal(snapshot.pendingXtermCharacters, 44)
  assert.equal(snapshot.pendingWriteCharacters, 6)
  assert.equal(snapshot.instrumentedParsers, 1)
  assert.equal(snapshot.instrumentedRenderers, 1)
  assert.equal(snapshot.byRenderer.dom, 1)
  assert.equal(snapshot.perTerminal[0].tabId, 'tab-safe')
  assert.ok(!JSON.stringify(snapshot).includes('secret'))
  const webglRenderRows = function () {
    assert.equal(this, webglRenderer)
    advance(15)
  }
  const webglRenderer = { renderRows: webglRenderRows, _gl: {} }
  terminal._core._renderService._renderer.value = webglRenderer
  registration.refreshRenderer()
  webglRenderer.renderRows()
  assert.equal(diagnostics.getTerminalResourceSnapshot().instrumentedRenderers, 1)
  assert.equal(diagnostics.getTerminalResourceSnapshot().byRenderer.webgl, 1)
  terminal._core._renderService._renderer.value = renderer
  registration.refreshRenderer()
  renderer.renderRows()
  registration.dispose()
  registration.dispose()
  assert.equal(parser.parse, parse)
  assert.equal(renderer.renderRows, renderRows)
  assert.equal(webglRenderer.renderRows, webglRenderRows)
  assert.equal(renderListeners.size, 0)
  assert.equal(diagnostics.getTerminalResourceSnapshot().terminals, 0)
  const fallback = diagnostics.registerTerminalDiagnostics({ ...terminal, _core: {} }, { current: '' }, { current: '' })
  assert.equal(diagnostics.getTerminalResourceSnapshot().unknownXtermQueues, 1)
  assert.equal(diagnostics.getTerminalResourceSnapshot().instrumentedParsers, 0)
  fallback.dispose()
  const changed = diagnostics.registerTerminalDiagnostics(terminal, { current: '' }, { current: '' })
  const replacement = () => {}
  parser.parse = replacement
  changed.dispose()
  assert.equal(parser.parse, replacement, 'another adapter replacement must not be overwritten')
})

function frameFixture({ supported = true, observeFails = false } = {}) {
  let callback
  let disconnected = false
  let slowReports = 0
  const document = { hidden: false }
  const frames = load('lib/renderer-frame-log', {
    URL,
    document,
    window: { location: { href: 'http://tauri.localhost/', origin: 'http://tauri.localhost' } },
    PerformanceObserver: class {
      static supportedEntryTypes = supported ? ['long-animation-frame'] : []
      constructor(listener) {
        callback = listener
      }
      observe(options) {
        assert.equal(options.type, 'long-animation-frame')
        if (observeFails) throw new Error('unsupported WebView')
      }
      disconnect() {
        disconnected = true
      }
    }
  }).observeRendererFrames(() => slowReports++)
  return {
    frames,
    slowReports: () => slowReports,
    document,
    emit: (...entries) => callback({ getEntries: () => entries }),
    disconnected: () => disconnected
  }
}

test('long frames retain bounded asset attribution but omit remote paths, queries and invokers', () => {
  const f = frameFixture()
  const entry = {
    startTime: 100,
    duration: 200,
    blockingDuration: 150,
    renderStart: 220,
    styleAndLayoutStart: 250,
    scripts: [
      {
        duration: 80,
        forcedStyleAndLayoutDuration: 25,
        sourceURL: 'http://tauri.localhost/assets/index-ab12.js?token=secret',
        sourceFunctionName: 'parse',
        sourceCharPosition: 123,
        invoker: 'secret terminal'
      },
      { duration: 70, sourceURL: 'https://remote.example/private/secret.js', sourceFunctionName: 'secret' },
      { duration: 60, sourceURL: 'file:///Users/secret/source.js', sourceFunctionName: 'secret' }
    ]
  }
  f.emit(entry)
  const snapshot = f.frames.snapshot()
  assert.equal(snapshot.supported, true)
  assert.equal(snapshot.count, 1)
  assert.equal(snapshot.slowest[0].layoutToFrameEndMs, 50)
  assert.equal(snapshot.slowest[0].scripts[0].bundle, 'index-ab12.js')
  assert.equal(snapshot.slowest[0].scripts[0].sourceCharPosition, 123)
  assert.equal(snapshot.slowest[0].scripts[0].forcedStyleAndLayoutMs, 25)
  assert.ok(!JSON.stringify(snapshot).includes('secret'))
  assert.ok(!JSON.stringify(snapshot).includes('remote.example'))
  for (let index = 0; index < 10; index++)
    f.emit({ ...entry, duration: 300, scripts: Array(100).fill(entry.scripts[0]) })
  assert.equal(f.frames.snapshot().slowest.length, 4)
  assert.equal(f.frames.snapshot().slowest[0].scripts.length, 3)
  assert.equal(f.frames.snapshot().slowest[0].attributionTruncated, true)
  assert.ok(JSON.stringify(f.frames.snapshot()).length < 4000)
  assert.equal(f.slowReports(), 0)
  f.emit({ ...entry, duration: 5000 }, { ...entry, duration: 2000 })
  assert.equal(f.slowReports(), 1, 'one report per observer batch')
  for (let index = 0; index < 10; index++) f.emit(entry)
  assert.equal(f.frames.snapshot().slowest[0].durationMs, 5000, 'retain the worst stall until reset')
  f.frames.reset()
  f.document.hidden = true
  f.emit(entry)
  assert.equal(f.frames.snapshot().count, 0)
  f.frames.dispose()
  assert.equal(f.disconnected(), true)
})

test('unsupported or rejecting frame observers degrade without changing app behavior', () => {
  for (const options of [{ supported: false }, { observeFails: true }]) {
    const f = frameFixture(options)
    assert.equal(f.frames.snapshot().supported, false)
    f.frames.dispose()
  }
})

test('WebGL atlas accounting includes off-DOM canvases, deduplicates shared atlases and marks unknown or truncated data', () => {
  const { work } = workFixture()
  const diagnostics = load('xterm-adapters/terminal-diagnostics', {}, { '../lib/renderer-work-log': work })
  const atlas = { pages: [{ canvas: { width: 512, height: 512 }, glyphs: [{}, {}] }] }
  const terminal = (sharedAtlas) => ({
    cols: 80,
    buffer: { normal: { length: 50 }, alternate: { length: 0 } },
    onRender: () => ({ dispose() {} }),
    _core: { _renderService: { _renderer: { value: { _gl: {}, _charAtlas: sharedAtlas, renderRows() {} } } } }
  })
  const registrations = [atlas, atlas, undefined].map((value) =>
    diagnostics.registerTerminalDiagnostics(terminal(value), { current: '' }, { current: '' })
  )
  const snapshot = diagnostics.getTerminalResourceSnapshot()
  assert.equal(snapshot.uniqueWebglAtlasPages, 1)
  assert.equal(snapshot.uniqueWebglAtlasGlyphs, 2)
  assert.equal(snapshot.uniqueWebglAtlasBytesEstimate, 512 * 512 * 4)
  assert.equal(snapshot.unknownWebglAtlases, 1)
  assert.equal(snapshot.perTerminal[0].webglAtlasPages, 1)
  assert.equal(snapshot.perTerminal[2].webglAtlasPages, null)
  atlas.pages = Array.from({ length: 70 }, () => ({ canvas: { width: 512, height: 512 } }))
  const truncated = diagnostics.getTerminalResourceSnapshot()
  assert.equal(truncated.uniqueWebglAtlasPages, 70)
  assert.equal(truncated.uniqueWebglAtlasGlyphs, null)
  assert.equal(truncated.uniqueWebglAtlasBytesEstimate, 64 * 512 * 512 * 4)
  assert.equal(truncated.webglAtlasEstimateTruncated, true)
  registrations.forEach((registration) => registration.dispose())
  assert.equal(diagnostics.getTerminalResourceSnapshot().uniqueWebglAtlasBytesEstimate, 0)
})
