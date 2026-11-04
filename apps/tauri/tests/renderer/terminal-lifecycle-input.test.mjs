import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function fixture() {
  function load(file, dependencies = {}) {
    const exports = {}
    vm.runInNewContext(
      ts.transpileModule(readFileSync(new URL('../../src/renderer/' + file + '.ts', import.meta.url), 'utf8'), {
        compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
      }).outputText,
      { exports, performance: { now: () => 100 }, console, require: (name) => dependencies[name] }
    )
    return exports
  }
  const work = load('lib/renderer-work-log')
  const calls = []
  const written = []
  let receive
  const ref = (current) => ({ current })
  const runtime = {
    terminal: {
      attachCustomKeyEventHandler() {},
      onData(listener) {
        receive = listener
        return {
          dispose: () => {
            receive = undefined
          }
        }
      }
    },
    isActiveRef: ref(true),
    connectedRef: ref(true),
    connectingRef: ref(false),
    replayingTranscriptRef: ref(false),
    serialTransferBusyRef: ref(false),
    clearEphemeralHighlight: () => calls.push('highlight'),
    setContextMenu: () => {
      if (runtime.failUiUpdate) throw new Error('React update failure')
      calls.push('menu')
    },
    writeTerminalInput: (data) => written.push(data),
    requestReconnect: () => calls.push('reconnect')
  }
  const compatibility = {
    markDataEmitted: () => calls.push('mark-input'),
    filterCompositionData: (data) => {
      calls.push('filter-input')
      return data
    },
    dispose() {}
  }
  const handlers = load('components/terminal-lifecycle-input', {
    '../lib/renderer-work-log': work,
    '../xterm-adapters/terminal-responses': load('xterm-adapters/terminal-responses'),
    '../xterm-adapters/terminal-input-compat': { createTerminalInputCompatibility: () => compatibility },
    './terminal-view-utils': {
      isFocusTrackingSequence: (data) => data === '\x1b[I' || data === '\x1b[O',
      logTerminalClipboard() {}
    }
  }).registerTerminalInputHandlers(runtime)
  return { runtime, compatibility, calls, written, handlers, work, emit: (data) => receive(data) }
}

test('parser query replies bypass React setters and IME filters, including in background terminals', () => {
  const f = fixture()
  f.runtime.failUiUpdate = true
  f.compatibility.filterCompositionData = () => null
  const replies = ['\x1b[12;34R', '\x1b[?1;2c', '\x1b]11;rgb:0000/0000/0000\x07', '\x1bP1$r0m\x1b\\']
  replies.forEach(f.emit)
  f.runtime.isActiveRef.current = false
  replies.forEach(f.emit)
  assert.deepEqual(f.written, [...replies, ...replies])
  assert.deepEqual(f.calls, [])
  assert.equal(f.work.getRendererWorkSnapshot().phases['terminal-input'].calls, replies.length * 2)
  f.handlers.dispose()
})

test('ordinary keyboard data and focus reports keep their existing UI and active-tab rules', () => {
  const f = fixture()
  f.emit('\x1b[A')
  assert.deepEqual(f.calls, ['mark-input', 'filter-input', 'highlight', 'menu'])
  f.calls.length = 0
  f.emit('\x1b[I')
  assert.deepEqual(f.calls, ['mark-input', 'filter-input'])
  f.runtime.isActiveRef.current = false
  f.emit('private user input')
  assert.deepEqual(f.written, ['\x1b[A', '\x1b[I'])
  f.handlers.dispose()
})

test('query replies respect serial ownership, disconnection and replay, with startup ConPTY replies preserved', () => {
  const f = fixture()
  const reply = '\x1b[12;34R'
  f.runtime.serialTransferBusyRef.current = true
  f.emit(reply)
  f.runtime.serialTransferBusyRef.current = false
  f.runtime.connectedRef.current = false
  f.emit(reply)
  f.runtime.connectedRef.current = true
  f.runtime.replayingTranscriptRef.current = true
  f.emit(reply)
  assert.deepEqual(f.written, [])
  f.runtime.connectedRef.current = false
  f.runtime.connectingRef.current = true
  f.emit(reply)
  assert.deepEqual(f.written, [reply], 'the initial connecting PTY query must still be answered')
  f.handlers.dispose()
})

test('an input callback failure retains its named phase without retaining terminal input', () => {
  const f = fixture()
  f.runtime.failUiUpdate = true
  assert.throws(() => f.emit('private input'), /React update failure/)
  const snapshot = f.work.getRendererWorkSnapshot()
  assert.equal(snapshot.recentFailures[0].phase, 'terminal-input')
  assert.ok(!JSON.stringify(snapshot).includes('private'))
  f.handlers.dispose()
})
