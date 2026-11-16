import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function harness(api) {
  const slots = []
  let index = 0
  let pending = []
  const react = {
    useRef(value) {
      const slot = index++
      return (slots[slot] ??= { current: value })
    },
    useEffect(effect, deps) {
      const slot = index++
      const previous = slots[slot]
      if (!previous || deps.some((dep, i) => dep !== previous.deps[i])) {
        pending.push(() => {
          previous?.cleanup?.()
          slots[slot] = { deps, cleanup: effect() }
        })
      }
    }
  }
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(
      readFileSync(new URL('../../src/renderer/hooks/use-connection-local-directory.ts', import.meta.url), 'utf8'),
      {
        compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
      }
    ).outputText,
    {
      exports,
      require: (name) => (name === 'react' ? react : { withParentRow: (_path, items) => items })
    }
  )
  const messages = []
  const options = {
    enabled: true,
    tabs: [{ id: 'a' }, { id: 'b' }],
    desktopApi: api,
    localPath: '/home',
    localNetworkShareSource: null,
    setLocalPath: (path) => {
      options.localPath = path
    },
    setLocalItems: () => {},
    setIsLocalDirectoryLoading: () => {},
    setLocalNetworkShareSource: () => {},
    onStatusMessage: (message) => messages.push(message),
    formatError: (scope) => scope
  }
  return {
    options,
    messages,
    async render(changes = {}) {
      Object.assign(options, changes)
      index = 0
      pending = []
      exports.useConnectionLocalDirectory(options)
      for (const effect of pending) effect()
      await new Promise(setImmediate)
    }
  }
}
const listing = (path) => ({ path: path ?? '/home', items: [] })

test('each connection opens its default and keeps the browsed directory when switching tabs', async () => {
  const calls = []
  const h = harness({
    listLocalDirectory: async (path) => {
      calls.push(path)
      return listing(path)
    }
  })
  await h.render({ tabId: 'a', defaultPath: '/a' })
  await h.render({ localPath: '/a/subdir' })
  await h.render({ tabId: 'b', defaultPath: '/b' })
  assert.equal(h.options.localPath, '/b')
  await h.render({ tabId: 'a', defaultPath: '/a' })
  assert.equal(h.options.localPath, '/a/subdir')
  assert.deepEqual(calls, ['/a', '/b', '/a/subdir'])
})

test('an unavailable configured directory reports the failure and falls back to the system default', async () => {
  const h = harness({
    listLocalDirectory: async (path) => {
      if (path) throw new Error('missing')
      return listing()
    }
  })
  await h.render({ tabId: 'a', defaultPath: '/missing' })
  assert.equal(h.options.localPath, '/home')
  assert.equal(h.messages.length, 1)
})

test('a late directory response cannot replace the newly selected connection', async () => {
  let resolveA
  const h = harness({
    listLocalDirectory: (path) =>
      path === '/a'
        ? new Promise((resolve) => {
            resolveA = resolve
          })
        : Promise.resolve(listing(path))
  })
  await h.render({ tabId: 'a', defaultPath: '/a' })
  await h.render({ tabId: 'b', defaultPath: '/b' })
  resolveA(listing('/a'))
  await new Promise(setImmediate)
  assert.equal(h.options.localPath, '/b')
})

test('editing the configured default invalidates the remembered tab path', async () => {
  const h = harness({ listLocalDirectory: async (path) => listing(path) })
  await h.render({ tabId: 'a', defaultPath: '/old' })
  await h.render({ tabId: 'a', defaultPath: '/new' })
  assert.equal(h.options.localPath, '/new')
})
