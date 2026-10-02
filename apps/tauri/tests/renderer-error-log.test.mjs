import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function fixture(writer) {
  const calls = []
  const listeners = new Map()
  let now = 10000
  const window = {
    location: { search: '?window=file-editor&secret=do-not-log' },
    fileterm: {
      appVersion: '2.2.20',
      platform: 'win32',
      writeDiagnosticLog:
        writer ??
        ((...args) => {
          calls.push(args)
          return Promise.resolve()
        })
    },
    addEventListener: (type, listener) => listeners.set(type, listener),
    removeEventListener: (type, listener) => {
      if (listeners.get(type) === listener) listeners.delete(type)
    }
  }
  function load(path, dependencies = {}) {
    const exports = {}
    vm.runInNewContext(
      ts.transpileModule(readFileSync(new URL(path, import.meta.url), 'utf8'), {
        compilerOptions: {
          module: ts.ModuleKind.CommonJS,
          target: ts.ScriptTarget.ES2022,
          jsx: ts.JsxEmit.ReactJSX
        }
      }).outputText,
      {
        exports,
        window,
        URLSearchParams,
        TextEncoder,
        Date: { now: () => now },
        console: { error() {} },
        require: (name) => {
          if (name in dependencies) return dependencies[name]
          throw new Error(`Unexpected import ${name}`)
        }
      }
    )
    return exports
  }
  const diagnostic = load('../src/renderer/lib/diagnostic-log.ts')
  const reporter = load('../src/renderer/lib/renderer-error-log.ts', { './diagnostic-log': diagnostic })
  const boundary = load('../src/renderer/features/common/error-boundary.tsx', {
    react: { Component: class {} },
    'react/jsx-runtime': {},
    '../../i18n': { t: {} },
    '../../lib/renderer-error-log': reporter
  })
  return {
    calls,
    window,
    listeners,
    reporter,
    boundary,
    advance: () => {
      now += 5001
    }
  }
}

const details = (calls, index = 0) => {
  assert.equal(calls[index][0], 'ERROR')
  assert.equal(calls[index][1], 'renderer:error')
  return JSON.parse(calls[index][2])
}

test('the actual error boundary persists error and component stacks with runtime metadata', () => {
  const f = fixture()
  const error = new Error('Maximum update depth exceeded')
  new f.boundary.ErrorBoundary().componentDidCatch(error, { componentStack: '\n at SystemSidebar\n at App' })
  const report = details(f.calls)
  assert.equal(report.source, 'react-boundary')
  assert.equal(report.message, error.message)
  assert.equal(report.stack, error.stack)
  assert.match(report.componentStack, /SystemSidebar/)
  assert.equal(report.version, '2.2.20')
  assert.equal(report.platform, 'win32')
  assert.equal(report.window, 'file-editor')
  assert.ok(!f.calls[0][2].includes('do-not-log'))
})

test('global handlers report JS errors and promise rejections without suppressing browser handling', () => {
  const f = fixture()
  const remove = f.reporter.installRendererErrorLogging()
  const error = new Error('script failed')
  f.listeners.get('error')({ error, message: error.message })
  f.listeners.get('error')({ message: 'message-only failure' })
  f.listeners.get('error')({})
  f.listeners.get('unhandledrejection')({ reason: 'async failure' })
  f.listeners.get('unhandledrejection')({ reason: { password: 'do-not-log' } })
  assert.equal(f.calls.length, 4)
  assert.equal(details(f.calls).stack, error.stack)
  assert.equal(details(f.calls, 1).message, 'message-only failure')
  assert.equal(details(f.calls, 2).source, 'unhandled-rejection')
  assert.ok(!f.calls[3][2].includes('do-not-log'))
  remove()
  assert.equal(f.listeners.size, 0)
})

test('duplicate errors are bounded but a later recurrence or React component stack is retained', () => {
  const f = fixture()
  f.reporter.reportRendererError('window-error', 'same')
  f.reporter.reportRendererError('window-error', 'same')
  f.reporter.reportRendererError('react-boundary', 'same', 'at App')
  assert.equal(f.calls.length, 2)
  f.advance()
  f.reporter.reportRendererError('window-error', 'same')
  assert.equal(f.calls.length, 3)
  f.reporter.reportRendererError(
    'react-boundary',
    { message: 'x'.repeat(20000), stack: 's'.repeat(20000) },
    'c'.repeat(20000)
  )
  assert.ok(f.calls.at(-1)[2].length < 10000)
  assert.match(f.calls.at(-1)[2], /truncated/)
})

test('missing bridge, throwing getters, synchronous failures and rejected log writes cannot recurse', async () => {
  for (const writer of [
    () => {
      throw new Error('IPC failed')
    },
    () => Promise.reject(new Error('IPC failed'))
  ]) {
    const f = fixture(writer)
    assert.doesNotThrow(() => f.reporter.reportRendererError('window-error', new Error('original')))
    await new Promise((resolve) => setImmediate(resolve))
  }
  const f = fixture()
  assert.doesNotThrow(() =>
    f.reporter.reportRendererError('unhandled-rejection', {
      get message() {
        throw new Error('bad getter')
      }
    })
  )
  f.window.fileterm = undefined
  assert.doesNotThrow(() => f.reporter.reportRendererError('window-error', 'no bridge'))
})

test('Unicode and control-heavy errors leave room for the component stack in the 16 KiB log limit', () => {
  const f = fixture()
  f.reporter.reportRendererError(
    'react-boundary',
    { message: '\u0001'.repeat(20000), stack: '中'.repeat(20000) },
    'at App\n'.repeat(20000)
  )
  const report = details(f.calls)
  assert.match(report.componentStack, /at App/)
  assert.ok(Buffer.byteLength(f.calls[0][2], 'utf8') < 12000)
})
