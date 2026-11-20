import assert from 'node:assert/strict'
import test from 'node:test'
import {
  buildConnectionTree,
  flattenConnectionProfiles
} from '../../src/renderer/features/connections/connection-tree.ts'
import {
  nextServerListSort,
  sortServerListRows
} from '../../src/renderer/features/server-management/server-management-list-model.ts'

const rows = [
  { id: 'a', name: 'Host 10', address: 'ops@host10', protocol: 'ssh', status: 'idle', tags: 'test' },
  { id: 'b', name: 'Host 2', address: 'ops@host2', protocol: 'telnet', status: 'connected', tags: 'prod' },
  { id: 'c', name: 'Host 2', address: 'ops@host12', protocol: 'ftp', status: 'connecting', tags: '' }
]

test('server list sorts each column in both directions without changing the card order', () => {
  const sort = (column, direction) => sortServerListRows(rows, { column, direction }).map(({ id }) => id)
  const ascending = {
    name: ['b', 'c', 'a'],
    address: ['b', 'a', 'c'],
    protocol: ['c', 'a', 'b'],
    status: ['b', 'c', 'a'],
    tags: ['c', 'b', 'a']
  }
  for (const [column, expected] of Object.entries(ascending)) {
    assert.deepEqual(sort(column, 'ascending'), expected)
    assert.deepEqual(sort(column, 'descending'), column === 'name' ? ['a', 'b', 'c'] : expected.toReversed())
  }
  assert.deepEqual(
    rows.map(({ id }) => id),
    ['a', 'b', 'c']
  )
})

test('server list defaults to saved connection order and returns to it after descending sort', () => {
  const profiles = [
    { ...rows[0], order: 20 },
    { ...rows[1], order: 30, parentId: 'folder' },
    { ...rows[2], order: 10 }
  ]
  const { roots } = buildConnectionTree(profiles, [{ id: 'folder', name: 'Folder', type: 'folder', order: 15 }])
  const saved = flattenConnectionProfiles(roots)
  const ascending = nextServerListSort(null, 'name')
  const descending = nextServerListSort(ascending, 'name')
  const restored = nextServerListSort(descending, 'name')
  assert.deepEqual(
    saved.map(({ id }) => id),
    ['c', 'b', 'a']
  )
  assert.deepEqual(
    sortServerListRows(saved, descending).map(({ id }) => id),
    ['a', 'c', 'b']
  )
  assert.equal(restored, null)
  assert.equal(sortServerListRows(saved, restored), saved)
  assert.deepEqual(nextServerListSort(descending, 'status'), { column: 'status', direction: 'ascending' })
})
