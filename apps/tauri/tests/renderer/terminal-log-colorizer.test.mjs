import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
const diagnostics = []
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/xterm-adapters/terminal-log-colorizer.ts', import.meta.url), 'utf8'),
    {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }
  ).outputText,
  {
    exports,
    Uint32Array,
    Uint8Array,
    require: (name) => {
      if (name === '../lib/renderer-work-log') return { trackRendererWork: (_phase, work) => work }
      assert.equal(name, '../lib/diagnostic-log')
      return {
        DIAGNOSTIC_SCOPES: { performance: 'renderer:performance' },
        writeDiagnosticLog: (...args) => diagnostics.push(args)
      }
    }
  }
)

test('unchanged palettes avoid history scans while real changes recolor and dispose restores ANSI', () => {
  const cell = {}
  let reads = 0
  let cellReads = 0
  let refreshes = 0
  const line = {
    length: 4,
    _data: new Uint32Array([105, 0, 0, 110, 0, 0, 102, 0, 0, 111, 0, 0]),
    translateToString: () => 'info',
    getCell(column, target) {
      cellReads++
      assert.equal(target, cell, 'reuse a cell instead of allocating one per column')
      target.getWidth = () => 1
      target.getChars = () => String.fromCharCode(this._data[column * 3])
      return target
    }
  }
  const buffer = {
    type: 'normal',
    baseY: 0,
    cursorY: 0,
    viewportY: 0,
    length: 1,
    getNullCell: () => cell,
    getLine: () => {
      reads++
      return line
    }
  }
  const terminal = {
    buffer: { normal: buffer, active: buffer },
    rows: 1,
    write: (_, callback) => callback?.(),
    reset() {},
    clear() {},
    resize() {},
    refresh() {
      refreshes++
    },
    registerMarker: () => ({ line: 0, isDisposed: false, dispose() {} })
  }
  const keys = [
    'timestamp',
    'service',
    'error',
    'warning',
    'success',
    'info',
    'debug',
    'address',
    'number',
    'permission',
    'flag'
  ]
  const palette = Object.fromEntries(keys.map((key) => [key, '#abc']))
  const colorizer = new exports.TerminalLogColorizer(terminal, palette)
  terminal.write('info')
  assert.equal(line._data[1], 0x03aabbcc)
  assert.equal(refreshes, 1)
  for (let index = 0; index < 100; index++) terminal.write('unchanged row')
  assert.equal(cellReads, 4, 'unchanged text reuses its regex matches and cell mapping')
  assert.equal(refreshes, 1, 'unchanged semantic colors must not refresh the DOM again')
  for (const text of ['warn', 'info']) {
    line.translateToString = () => text
    for (let col = 0; col < text.length; col++) {
      line._data[col * 3] = text.charCodeAt(col)
      line._data[col * 3 + 1] = 0
    }
    terminal.write(text)
  }
  assert.equal(cellReads, 12, 'changed text invalidates cached matches and cell mapping')
  reads = 0
  for (let index = 0; index < 100; index++) colorizer.setPalette({ ...palette, info: '#aabbcc' })
  assert.equal(reads, 0)
  colorizer.setPalette({ ...palette, info: '#123456' })
  assert.equal(cellReads, 16, 'changed palette invalidates cached colors')
  assert.ok(reads > 0)
  assert.equal(line._data[1], 0x03123456)
  colorizer.setPalette({ ...palette, info: 'invalid' })
  assert.equal(line._data[1], 0, 'removing a rule restores the original foreground')
  for (let index = 0; index < 600; index++) {
    colorizer.setPalette({ ...palette, info: '#' + index.toString(16).padStart(6, '0') })
    assert.ok(colorizer.rangeCacheMisses <= 256, 'text/range cache cannot grow with the full history')
  }
  colorizer.dispose()
  assert.equal(colorizer.rangeCacheMisses, 0)
  assert.equal(line._data[1], 0)
})

test('optional recoloring failures do not interrupt xterm parsing or its completion callback', () => {
  const callbacks = []
  let disposedMarkers = 0
  let reads = 0
  const buffer = {
    type: 'normal',
    baseY: 0,
    cursorY: 0,
    viewportY: 0,
    length: 1,
    getNullCell: () => ({}),
    getLine: () => {
      reads++
      throw new Error('unavailable buffer line')
    }
  }
  const terminal = {
    buffer: { normal: buffer, active: buffer },
    rows: 1,
    write: (_text, callback) => callbacks.push(callback),
    reset() {},
    clear() {},
    resize() {},
    refresh() {},
    registerMarker: () => ({
      line: 0,
      isDisposed: false,
      dispose() {
        disposedMarkers++
      }
    })
  }
  const colorizer = new exports.TerminalLogColorizer(
    terminal,
    Object.fromEntries(
      [
        'timestamp',
        'service',
        'error',
        'warning',
        'success',
        'info',
        'debug',
        'address',
        'number',
        'permission',
        'flag'
      ].map((key) => [key, '#abc'])
    )
  )
  let parsed = 0
  terminal.write('frame', () => parsed++)
  assert.doesNotThrow(() => callbacks.shift()())
  assert.equal(parsed, 1)
  assert.equal(disposedMarkers, 1)
  terminal.write('next frame', () => parsed++)
  assert.doesNotThrow(() => callbacks.shift()())
  assert.equal(parsed, 2)
  assert.doesNotThrow(() => {
    terminal.reset()
    terminal.clear()
    terminal.resize(80, 24)
    colorizer.dispose()
  })
  assert.equal(reads, 1, 'disabled coloring must also stay bypassed during resize and cleanup')
  assert.equal(diagnostics.length, 1, 'report once without flooding diagnostics')
  assert.ok(!JSON.stringify(diagnostics).includes('frame'))
})

test('xterm enqueue rejection releases the colorizer marker before propagating the failure', () => {
  let disposedMarkers = 0
  const buffer = { type: 'normal', getNullCell: () => ({}) }
  const terminal = {
    buffer: { normal: buffer, active: buffer },
    write: () => {
      throw new Error('write data discarded')
    },
    reset() {},
    clear() {},
    resize() {},
    registerMarker: () => ({
      dispose() {
        disposedMarkers++
      }
    })
  }
  new exports.TerminalLogColorizer(
    terminal,
    Object.fromEntries(
      [
        'timestamp',
        'service',
        'error',
        'warning',
        'success',
        'info',
        'debug',
        'address',
        'number',
        'permission',
        'flag'
      ].map((key) => [key, '#abc'])
    )
  )
  assert.throws(() => terminal.write('frame'), /write data discarded/)
  assert.equal(disposedMarkers, 1)
})

test('plain agent output avoids per-cell mapping and redundant refresh; clear scans history once', () => {
  let cellReads = 0
  let lineReads = 0
  let refreshes = 0
  const line = {
    length: 80,
    _data: new Uint32Array(240),
    translateToString: () => 'Working on the requested changes',
    getCell() {
      cellReads++
      throw new Error('plain text must not map cells')
    }
  }
  const buffer = {
    type: 'normal',
    baseY: 0,
    cursorY: 0,
    viewportY: 0,
    length: 1,
    getNullCell: () => ({}),
    getLine() {
      lineReads++
      return line
    }
  }
  const terminal = {
    buffer: { normal: buffer, active: buffer },
    rows: 1,
    write: (_, callback) => callback?.(),
    reset() {},
    clear() {},
    resize() {},
    refresh() {
      refreshes++
    },
    registerMarker: () => ({ line: 0, dispose() {} })
  }
  const colorizer = new exports.TerminalLogColorizer(
    terminal,
    Object.fromEntries(
      [
        'timestamp',
        'service',
        'error',
        'warning',
        'success',
        'info',
        'debug',
        'address',
        'number',
        'permission',
        'flag'
      ].map((key) => [key, '#abc'])
    )
  )
  for (let index = 0; index < 100; index++) terminal.write('update')
  assert.equal(cellReads, 0)
  assert.equal(refreshes, 0)
  lineReads = 0
  terminal.clear()
  assert.equal(lineReads, 1)
  colorizer.dispose()
})
