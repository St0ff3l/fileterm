import assert from 'node:assert/strict'
import test from 'node:test'
import { sortServerListRows } from '../../src/renderer/features/server-management/server-management-list-model.ts'

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
