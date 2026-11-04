import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function load(name, dependencies = {}, globals = {}) {
  const exports = {}
  vm.runInNewContext(
    ts.transpileModule(readFileSync(new URL(`../../src/renderer/features/files/${name}.ts`, import.meta.url), 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText,
    {
      exports,
      require: (name) => {
        assert.ok(name in dependencies, `Unexpected import ${name}`)
        return dependencies[name]
      },
      ...globals
    }
  )
  return exports
}
const model = load('remote-directory-tree-model')
const plain = (value) => JSON.parse(JSON.stringify(value))
const directory = (path) => ({ path, name: path.split('/').at(-1) || '/', type: 'folder', modified: '', size: '' })
const tick = () => new Promise(setImmediate)
function deferred() {
  let resolve, reject
  const promise = new Promise((yes, no) => {
    resolve = yes
    reject = no
  })
  return { promise, resolve, reject }
}

test('tree preserves remote names, excludes files and parent entries, and exposes the current path after listing errors', () => {
  assert.deepEqual(plain(model.remoteDirectoryAncestors('/home/a b/back\\slash/')), [
    '/',
    '/home',
    '/home/a b',
    '/home/a b/back\\slash'
  ])
  const items = [
    directory('/z'),
    directory('/a'),
    { ...directory('/file'), type: 'file' },
    directory('/..'),
    directory('/.')
  ]
  assert.deepEqual(
    plain(model.remoteDirectories(items)).map((item) => item.name),
    ['a', 'z']
  )
  const rows = model.remoteDirectoryRows({}, new Set(['/', '/home', '/home/user']), '/home/user')
  assert.deepEqual(
    plain(rows).map((row) => [row.path, row.depth, row.parent]),
    [
      ['/', 0, null],
      ['/home', 1, '/'],
      ['/home/user', 2, '/home']
    ]
  )
  const cycle = model.remoteDirectoryRows({ '/': [directory('/')] }, new Set(['/']), '/')
  assert.equal(cycle.length, 1)
})

function treeFixture(path = '/') {
  let cursor = 0
  const slots = [],
    effects = [],
    requests = []
  const react = {
    useState(initial) {
      const index = cursor++
      if (!(index in slots)) slots[index] = typeof initial === 'function' ? initial() : initial
      return [
        slots[index],
        (value) => {
          slots[index] = typeof value === 'function' ? value(slots[index]) : value
        }
      ]
    },
    useRef(initial) {
      const index = cursor++
      return (slots[index] ??= { current: initial })
    },
    useCallback(callback, deps) {
      const index = cursor++
      if (!slots[index] || deps.some((dep, i) => dep !== slots[index].deps[i])) slots[index] = { callback, deps }
      return slots[index].callback
    },
    useEffect(callback, deps) {
      const index = cursor++
      if (!slots[index] || deps.some((dep, i) => dep !== slots[index].deps[i])) {
        effects.push(() => {
          slots[index]?.cleanup?.()
          slots[index] = { deps, cleanup: callback() }
        })
      }
    }
  }
  const { useRemoteDirectoryTree } = load(
    'use-remote-directory-tree',
    { react, './remote-directory-tree-model': model },
    {
      window: {
        fileterm: {
          listRemoteDirectory: (_tab, path) => {
            const reply = deferred()
            requests.push({ path, ...reply })
            return reply.promise
          }
        }
      }
    }
  )
  let files = [],
    refresh = 0
  return {
    requests,
    render(next = {}) {
      path = next.path ?? path
      files = next.files ?? files
      refresh = next.refresh ?? refresh
      cursor = 0
      const tree = useRemoteDirectoryTree('tab', path, files, refresh)
      effects.splice(0).forEach((effect) => effect())
      return tree
    },
    unmount() {
      slots.forEach((slot) => slot?.cleanup?.())
    }
  }
}

test('tree lists ancestors without reading or changing the current directory, and deduplicates lazy expansion', async () => {
  const fixture = treeFixture('/home/user')
  fixture.render({ files: [directory('/home/user/docs')] })
  await tick()
  assert.deepEqual(
    fixture.requests.map((request) => request.path),
    ['/']
  )
  fixture.requests[0].resolve([directory('/home')])
  await tick()
  assert.equal(fixture.requests[1].path, '/home')
  fixture.requests[1].resolve([directory('/home/user')])
  await tick()
  let tree = fixture.render()
  tree.toggle('/var', true)
  tree.toggle('/var', true)
  await tick()
  assert.equal(fixture.requests.filter((request) => request.path === '/var').length, 1)
  fixture.requests.at(-1).resolve([directory('/var/log')])
  await tick()
  tree = fixture.render()
  assert.equal(tree.cache['/home/user'][0].path, '/home/user/docs')
  assert.equal(tree.cache['/var'][0].path, '/var/log')
  assert.ok(!fixture.requests.some((request) => request.path === '/home/user'))
})

test('navigation and unmount discard obsolete reads; failed nodes can be retried', async () => {
  const fixture = treeFixture()
  let tree = fixture.render()
  tree.toggle('/var', true)
  await tick()
  fixture.render({ path: '/var', files: [directory('/var/current')] })
  fixture.requests[0].resolve([directory('/var/stale')])
  await tick()
  tree = fixture.render()
  assert.equal(tree.cache['/var'][0].path, '/var/current')
  tree.toggle('/bad', true)
  await tick()
  fixture.requests.at(-1).reject(new Error('permission denied'))
  await tick()
  tree = fixture.render()
  assert.match(tree.errors['/bad'], /permission denied/)
  void tree.load('/bad', true)
  await tick()
  fixture.requests.at(-1).resolve([directory('/bad/ok')])
  await tick()
  tree = fixture.render()
  assert.equal(tree.errors['/bad'], undefined)
  tree.toggle('/slow', true)
  await tick()
  const before = fixture.render().cache
  fixture.unmount()
  fixture.requests.at(-1).resolve([directory('/slow/ignored')])
  await tick()
  assert.equal(before['/slow'], undefined)
})

test('refresh invalidates expanded node caches and reloads them without reading the current directory', async () => {
  const fixture = treeFixture()
  fixture.render().toggle('/etc', true)
  await tick()
  fixture.requests[0].resolve([directory('/etc/old')])
  await tick()
  fixture.render({ refresh: 1 })
  await tick()
  assert.deepEqual(
    fixture.requests.map((request) => request.path),
    ['/etc', '/etc']
  )
  fixture.requests[1].resolve([directory('/etc/new')])
  await tick()
  assert.equal(fixture.render().cache['/etc'][0].path, '/etc/new')
})

test('relative directory names cannot read inherited cache properties', () => {
  assert.deepEqual(plain(model.remoteDirectoryAncestors('constructor/')), ['constructor'])
  assert.equal(model.remoteDirectoryRows({}, new Set(['constructor']), 'constructor').length, 1)
})
