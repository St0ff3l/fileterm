import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

const exports = {}
vm.runInNewContext(
  ts.transpileModule(
    readFileSync(new URL('../../src/renderer/features/layout/host-groups.ts', import.meta.url), 'utf8'),
    {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }
  ).outputText,
  { exports }
)
const { buildHostGroups, resolveActiveHostGroupKey, aggregateHostStatus } = exports

const session = (id, profileId, status = 'connected', sessionType = 'ssh') => ({
  key: `session:${id}`,
  kind: 'session',
  tab: { id, profileId, status, sessionType, title: `${profileId || 'local'}-${id}` }
})
const home = (id) => ({ key: `home:${id}`, kind: 'local', id, title: 'New Tab', tabKind: 'home' })
const system = (id, sessionTabId) => ({
  key: `home:${id}`,
  kind: 'local',
  id,
  title: 'System',
  tabKind: 'system',
  sessionTabId
})
const profiles = [
  { id: 'web', type: 'ssh', name: 'Web', host: '10.0.0.1', username: 'deploy' },
  { id: 'db', type: 'ssh', name: 'DB', host: 'db.local', username: '' }
]

test('tabs are grouped by host in first-seen order with home always first', () => {
  const tabs = [
    session('a', 'web'),
    session('b', 'db'),
    home('home-1'),
    session('c', 'web', 'error'),
    session('l', '', 'connected', 'local')
  ]
  const groups = buildHostGroups(tabs, profiles)
  assert.deepEqual(
    Array.from(groups, (group) => group.key),
    ['home', 'profile:web', 'profile:db', 'local']
  )
  const web = groups[1]
  assert.deepEqual(
    Array.from(web.entries, (entry) => entry.key),
    ['session:a', 'session:c']
  )
  assert.equal(web.title, 'Web')
  assert.equal(web.subtitle, 'deploy@10.0.0.1')
  assert.equal(web.status, 'connected')
  assert.equal(groups[2].subtitle, 'db.local')
  assert.equal(groups[0].entries.length, 1)
})

test('system info tabs join the host of the session they describe', () => {
  const tabs = [session('a', 'web'), system('sys-1', 'a'), system('sys-2', 'gone')]
  const groups = buildHostGroups(tabs, profiles)
  const groupEntryKeys = (key) => Array.from(groups.find((group) => group.key === key).entries, (entry) => entry.key)
  assert.deepEqual(groupEntryKeys('profile:web'), ['session:a', 'home:sys-1'])
  assert.deepEqual(groupEntryKeys('home'), ['home:sys-2'])
  assert.equal(resolveActiveHostGroupKey(tabs, 'sys-1', null), 'profile:web')
})

test('the active group follows the active tab', () => {
  const tabs = [home('home-1'), session('a', 'web'), session('b', 'db')]
  assert.equal(resolveActiveHostGroupKey(tabs, 'home-1', 'a'), 'home')
  assert.equal(resolveActiveHostGroupKey(tabs, null, 'b'), 'profile:db')
  assert.equal(resolveActiveHostGroupKey(tabs, null, 'missing'), 'home')
})

test('host status prefers live sessions over failed ones', () => {
  assert.equal(aggregateHostStatus(['error', 'connecting']), 'connecting')
  assert.equal(aggregateHostStatus(['closed', 'connected']), 'connected')
  assert.equal(aggregateHostStatus([]), null)
})
