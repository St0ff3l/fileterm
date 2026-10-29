import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
const chunkSize = 16 * 1024
const terminalResizes = []
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../src/renderer/components/terminal-view-actions.ts', import.meta.url), 'utf8').replaceAll(
      'import.meta.env.DEV',
      'false'
    ),
    { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
  ).outputText,
  {
    exports,
    queueMicrotask,
    document: { documentElement: {} },
    getComputedStyle: () => ({ getPropertyValue: () => '' }),
    window: {
      fileterm: { resizeTerminal: (...args) => terminalResizes.push(args) },
      requestAnimationFrame: () => {
        throw new Error('Output must drain even when animation frames are suspended')
      }
    },
    require: (name) => {
      if (name === 'react') return { useCallback: (callback) => callback }
      if (name === '../xterm-adapters/terminal-log-colorizer') return { getTerminalLogColorPalette: (theme) => theme }
      if (name === '../xterm-adapters/terminal-transcript-replay') return { trimTranscript: (value) => value }
      if (name === '../xterm-adapters/terminal-render-recovery')
        return { repaintTerminalRender: (terminal) => terminal.refresh(0, Math.max(0, terminal.rows - 1)) }
      if (name === './terminal-view-utils')
        return {
          TERMINAL_WRITE_CHUNK_SIZE: chunkSize,
          TERMINAL_FIT_GUARD_ROWS: 0,
          TERMINAL_REMOTE_GUARD_COLS: 0,
          TERMINAL_RESIZE_PIXEL_EPSILON: 2,
          splitPaneShortcutsForPlatform: () => ({}),
          toDisplayTerminalText: (value) => value,
          logTerminalRender: () => {}
        }
      if (
        [
          '@fileterm/core',
          '../app/app-utils',
          '../xterm-adapters/terminal-log-colorizer',
          '../xterm-adapters/terminal-vim-selection',
          '../xterm-adapters/terminal-render-recovery',
          '../app/ui-zoom',
          '../app/terminal-font-size-store',
          '../i18n'
        ].includes(name)
      )
        return {}
      throw new Error(`Unexpected import ${name}`)
    }
  }
)

function fixture() {
  const writes = []
  const callbacks = []
  const terminal = {
    reset: () => {},
    write: (text, callback) => {
      writes.push(text)
      callbacks.push(callback)
    }
  }
  const refs = {
    terminalRef: { current: terminal },
    pendingWriteRef: { current: '' },
    pendingWriteCompletionsRef: { current: [] },
    isWritingRef: { current: false },
    transcriptReplayGenerationRef: { current: 0 },
    renderedTranscriptRef: { current: '' },
    replayingTranscriptRef: { current: false },
    tabIdRef: { current: 'tab' },
    suppressHydratedChunksUntilRef: { current: 0 }
  }
  const { scheduleTerminalWrite, replaceTerminalWithTranscript } = exports.useTerminalViewActions(refs)
  return { ...refs, writes, callbacks, scheduleTerminalWrite, replaceTerminalWithTranscript }
}

test('idle terminal output reaches xterm without waiting for an animation frame', () => {
  const f = fixture()
  f.scheduleTerminalWrite('prompt> ')
  assert.deepEqual(f.writes, ['prompt> '])
  assert.equal(f.isWritingRef.current, true)
})

test('parsing completion drains bounded chunks in order with animation frames suspended', async () => {
  const f = fixture()
  const first = 'A'.repeat(chunkSize + 10)
  f.scheduleTerminalWrite(first)
  f.scheduleTerminalWrite('B')
  assert.deepEqual(f.writes, ['A'.repeat(chunkSize)])
  assert.equal(f.pendingWriteRef.current, 'A'.repeat(10) + 'B')
  f.callbacks.shift()()
  await Promise.resolve()
  assert.equal(f.writes.join(''), first + 'B')
  f.callbacks.shift()()
  assert.equal(f.pendingWriteRef.current, '')
  assert.equal(f.isWritingRef.current, false)
})

test('late parser callbacks cannot change a replacement terminal or transcript replay', () => {
  for (const replace of ['terminal', 'transcript']) {
    const f = fixture()
    f.scheduleTerminalWrite('old')
    if (replace === 'terminal') f.terminalRef.current = null
    else f.transcriptReplayGenerationRef.current++
    f.pendingWriteRef.current = 'new'
    f.callbacks.shift()()
    assert.equal(f.isWritingRef.current, true)
    assert.equal(f.pendingWriteRef.current, 'new')
    assert.deepEqual(f.writes, ['old'])
  }
})

test('credit resolves after the full batch is parsed, including combined writes', async () => {
  const f = fixture()
  let completed = 0
  const first = f.scheduleTerminalWrite('A'.repeat(chunkSize + 10)).then(() => completed++)
  const second = f.scheduleTerminalWrite('B').then(() => completed++)
  f.callbacks.shift()()
  await Promise.resolve()
  assert.equal(completed, 0)
  f.callbacks.shift()()
  await Promise.all([first, second])
  assert.equal(completed, 2)
  assert.equal(f.pendingWriteCompletionsRef.current.length, 0)
})

test('transcript replacement settles discarded output without credit leaking', async () => {
  const f = fixture()
  const first = f.scheduleTerminalWrite('old')
  const second = f.scheduleTerminalWrite('older pending')
  f.replaceTerminalWithTranscript(f.terminalRef.current, 'snapshot')
  await Promise.all([first, second])
  assert.equal(f.pendingWriteCompletionsRef.current.length, 0)
  const next = f.scheduleTerminalWrite('new')
  f.callbacks.shift()()
  assert.equal(f.pendingWriteCompletionsRef.current.length, 1)
  f.callbacks.shift()()
  f.callbacks.shift()()
  await next
  assert.equal(f.pendingWriteCompletionsRef.current.length, 0)
})

test('unchanged terminal themes do not invalidate contrast caches or repaint history', () => {
  const f = fixture()
  let themes = 0
  let recolors = 0
  let refreshes = 0
  let currentTheme
  f.terminalRef.current.options = {
    get theme() {
      return currentTheme
    },
    set theme(theme) {
      currentTheme = theme
      themes++
    }
  }
  f.terminalRef.current.rows = 24
  f.terminalRef.current.refresh = () => refreshes++
  f.terminalLogColorizerRef = { current: { setPalette: () => recolors++ } }
  const { applyTerminalTheme } = exports.useTerminalViewActions(f)
  for (let index = 0; index < 100; index++) applyTerminalTheme()
  assert.equal(themes, 1)
  assert.equal(recolors, 1)
  assert.equal(refreshes, 1)
  currentTheme.foreground = '#123456'
  applyTerminalTheme()
  assert.equal(themes, 2)
  assert.equal(recolors, 2)
})

test('pixel-only resizing repaints frozen columns without sending a mismatched PTY size', () => {
  const f = fixture()
  let refreshes = 0
  Object.assign(f.terminalRef.current, { cols: 80, rows: 24, refresh: () => refreshes++ })
  f.hostRef = { current: { getBoundingClientRect: () => ({ width: 810, height: 400 }) } }
  f.lastSyncedSizeRef = { current: { cols: 80, rows: 24, width: 800, height: 400 } }
  const fit = { proposeDimensions: () => ({ cols: 81, rows: 24 }) }
  const { syncTerminalSize } = exports.useTerminalViewActions(f)
  terminalResizes.length = 0
  syncTerminalSize(fit, f.terminalRef.current, { freezeCols: true })
  assert.equal(refreshes, 1, 'a new host width needs fresh rows even before columns settle')
  assert.equal(f.terminalRef.current.cols, 80)
  assert.equal(terminalResizes.length, 0)
  syncTerminalSize(fit, f.terminalRef.current, { freezeCols: true })
  assert.equal(refreshes, 1, 'an unchanged host does not cause an idle repaint loop')
  f.terminalRef.current.resize = (cols, rows) => Object.assign(f.terminalRef.current, { cols, rows })
  syncTerminalSize(fit, f.terminalRef.current, { force: true })
  assert.equal(refreshes, 2)
  assert.equal(f.terminalRef.current.cols, 81)
  assert.deepEqual(terminalResizes, [['tab', 81, 24, 810, 400]])
})

test('observed terminal layout is reused without a second synchronous geometry read', () => {
  const f = fixture()
  Object.assign(f.terminalRef.current, { cols: 80, rows: 24, refresh() {} })
  f.hostRef = {
    current: {
      getBoundingClientRect: () => {
        throw new Error('unexpected layout read')
      }
    }
  }
  f.lastSyncedSizeRef = { current: { cols: 80, rows: 24, width: 800, height: 400 } }
  const fit = {
    proposeDimensions: () => {
      throw new Error('unexpected fit read')
    }
  }
  const { syncTerminalSize } = exports.useTerminalViewActions(f)
  terminalResizes.length = 0
  syncTerminalSize(fit, f.terminalRef.current, {
    freezeCols: true,
    observedLayout: { width: 810, height: 400, cols: 81, rows: 24 }
  })
  assert.equal(f.lastSyncedSizeRef.current.width, 810)
  assert.equal(terminalResizes.length, 0)
})

test('a rejected xterm write cannot latch the writer busy or leak later display credit', async () => {
  const f = fixture()
  const terminal = f.terminalRef.current
  const write = terminal.write
  terminal.write = () => {
    throw new Error('write data discarded')
  }
  const failed = f.scheduleTerminalWrite('lost frame')
  await assert.rejects(failed, /write data discarded/)
  assert.equal(f.isWritingRef.current, false)
  assert.equal(f.pendingWriteCompletionsRef.current.length, 0)
  assert.equal(f.pendingWriteRef.current, '')
  terminal.write = write
  const next = f.scheduleTerminalWrite('new frame')
  f.callbacks.shift()()
  await next
  assert.deepEqual(f.writes, ['new frame'])
})

test('failure while draining rejects all affected batches and leaves the next delivery usable', async () => {
  const f = fixture()
  const terminal = f.terminalRef.current
  const write = terminal.write
  const first = f.scheduleTerminalWrite('A'.repeat(chunkSize + 10))
  const second = f.scheduleTerminalWrite('B')
  terminal.write = () => {
    throw new Error('cannot enqueue')
  }
  const rejected = Promise.all([assert.rejects(first, /cannot enqueue/), assert.rejects(second, /cannot enqueue/)])
  f.callbacks.shift()()
  await rejected
  assert.equal(f.isWritingRef.current, false)
  assert.equal(f.pendingWriteRef.current, '')
  assert.equal(f.pendingWriteCompletionsRef.current.length, 0)
  terminal.write = write
  const next = f.scheduleTerminalWrite('C')
  f.callbacks.shift()()
  await next
})
