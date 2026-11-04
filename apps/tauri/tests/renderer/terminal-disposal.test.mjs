import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
const clearedTimers = []
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/xterm-adapters/terminal-disposal.ts', import.meta.url), 'utf8'),
    { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }
  ).outputText,
  { exports, clearTimeout: (timer) => clearedTimers.push(timer) }
)

test('WebGL cleanup releases its timer, cursor manager and context', () => {
  const calls = []
  const addon = {
    _renderer: {
      _contextRestorationTimeout: 42,
      _cursorBlinkStateManager: { dispose: () => calls.push('cursor') },
      _gl: {
        getExtension: (name) => {
          calls.push(name)
          return { loseContext: () => calls.push('context') }
        }
      }
    },
    dispose: () => calls.push('addon')
  }

  exports.releaseWebglAddon(addon)

  assert.deepEqual(clearedTimers, [42])
  assert.equal(addon._renderer._contextRestorationTimeout, undefined)
  assert.deepEqual(calls, ['cursor', 'addon', 'WEBGL_lose_context', 'context'])
})

test('terminal cleanup clears the hidden resize queue even if dispose throws', () => {
  const calls = []
  const terminal = {
    _core: { _renderService: { _pausedResizeTask: { _queue: { clear: () => calls.push('queue') } } } },
    dispose: () => {
      calls.push('terminal')
      throw new Error('dispose failed')
    }
  }

  assert.throws(() => exports.disposeTerminal(terminal), /dispose failed/)
  assert.deepEqual(calls, ['terminal', 'queue'])
})
