import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function fixture() {
  let now = 0
  let callback
  let reset
  let cleared = false
  const calls = []
  const document = {
    hidden: false,
    hasFocus: () => true,
    addEventListener: (_, listener) => {
      reset = listener
    },
    removeEventListener: (_, listener) => {
      assert.equal(listener, reset)
      reset = undefined
    }
  }
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(
      readFileSync(new URL('../src/renderer/lib/renderer-performance-log.ts', import.meta.url), 'utf8'),
      {
        compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
      }
    ).outputText,
    {
      exports,
      document,
      performance: { now: () => now },
      window: {
        fileterm: { appVersion: '2.2.28', platform: 'darwin' },
        setInterval: (listener, interval) => {
          assert.equal(interval, 1000)
          callback = listener
          return 7
        },
        clearInterval: (timer) => {
          assert.equal(timer, 7)
          cleared = true
        }
      },
      require: () => ({
        DIAGNOSTIC_SCOPES: { performance: 'renderer:performance' },
        writeDiagnosticLog: (...args) => calls.push(args)
      })
    }
  )
  const dispose = exports.installRendererPerformanceLogging()
  return {
    calls,
    document,
    dispose,
    get cleared() {
      return cleared
    },
    tick: (elapsed) => {
      now += elapsed
      callback()
    },
    visibility: () => reset()
  }
}

test('recovered foreground stalls log WARN without an exception and are rate limited', () => {
  const f = fixture()
  f.tick(1000)
  assert.equal(f.calls.length, 0)
  f.tick(5000)
  assert.equal(f.calls[0][0], 'WARN')
  assert.equal(f.calls[0][1], 'renderer:performance')
  assert.equal(JSON.parse(f.calls[0][2]).elapsedMs, 5000)
  f.tick(5000)
  assert.equal(f.calls.length, 1)
  f.tick(30000)
  assert.equal(f.calls.length, 2)
  f.dispose()
  assert.equal(f.cleared, true)
})

test('hidden timer throttling and the transition back to foreground are ignored', () => {
  const f = fixture()
  f.document.hidden = true
  f.tick(60000)
  f.document.hidden = false
  f.visibility()
  f.tick(1000)
  assert.equal(f.calls.length, 0)
  f.tick(4000)
  assert.equal(f.calls.length, 1)
  f.dispose()
})
