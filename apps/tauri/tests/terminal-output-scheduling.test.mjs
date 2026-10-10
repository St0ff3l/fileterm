import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
const chunkSize = 16 * 1024
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
    window: {
      requestAnimationFrame: () => {
        throw new Error('Output must drain even when animation frames are suspended')
      }
    },
    require: (name) => {
      if (name === 'react') return { useCallback: (callback) => callback }
      if (name === './terminal-view-utils')
        return {
          TERMINAL_WRITE_CHUNK_SIZE: chunkSize,
          splitPaneShortcutsForPlatform: () => ({})
        }
      if (
        [
          '@fileterm/core',
          '../app/app-utils',
          '../app/terminal-log-colorizer',
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
    write: (text, callback) => {
      writes.push(text)
      callbacks.push(callback)
    }
  }
  const refs = {
    terminalRef: { current: terminal },
    pendingWriteRef: { current: '' },
    isWritingRef: { current: false },
    transcriptReplayGenerationRef: { current: 0 }
  }
  const { scheduleTerminalWrite } = exports.useTerminalViewActions(refs)
  return { ...refs, writes, callbacks, scheduleTerminalWrite }
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
