import assert from 'node:assert/strict'
import test from 'node:test'
import {
  buildConnectionTree,
  connectionFolderOptions,
  flattenConnectionProfiles
} from '../../src/renderer/features/connections/connection-tree.ts'

test('folder choices follow the connection tree and include paths for nested folders', () => {
  const { roots, map } = buildConnectionTree(
    [
      { id: 'web', name: 'Web', type: 'ssh', parentId: 'production-web' },
      { id: 'local', name: 'Local', type: 'ssh' }
    ],
    [
      { id: 'production-web', name: 'Web', type: 'folder', parentId: 'production' },
      { id: 'empty', name: 'Empty', type: 'folder', order: 20 },
      { id: 'production', name: 'Production', type: 'folder', order: 10 }
    ]
  )

  assert.deepEqual(connectionFolderOptions(roots), [
    { value: 'production', label: 'Production' },
    { value: 'production-web', label: 'Production / Web' },
    { value: 'empty', label: 'Empty' }
  ])
  assert.deepEqual(
    flattenConnectionProfiles([map.get('production')]).map(({ id }) => id),
    ['web']
  )
})
