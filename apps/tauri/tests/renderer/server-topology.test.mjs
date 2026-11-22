import assert from 'node:assert/strict'
import test from 'node:test'
import {
  DEFAULT_TOPOLOGY_NODE_METRICS,
  DEFAULT_TOPOLOGY_NODE_METRIC_ORDER,
  addTopologyNode,
  addTopologyNodes,
  decodeTopologyBoard,
  moveTopologyNode,
  removeTopologyNode,
  topologyLinkMatches,
  topologyLinkEndpoints,
  updateTopologyNodeMemo
} from '../../src/renderer/features/server-management/server-topology-model.ts'

function defaultTopologyNode(id, x, y, extra = {}) {
  return {
    id,
    x,
    y,
    role: 'server',
    size: 'compact',
    metrics: [...DEFAULT_TOPOLOGY_NODE_METRICS],
    metricOrder: [...DEFAULT_TOPOLOGY_NODE_METRIC_ORDER],
    services: [],
    ...extra
  }
}

test('topology retains free positions and saves links with their endpoints', () => {
  let board = decodeTopologyBoard(null)
  board = addTopologyNode(addTopologyNode(board, 'web'), 'db')
  assert.equal(addTopologyNode(board, 'web'), board)
  assert.equal(moveTopologyNode(board, 'web', -1, 0), board)
  board = moveTopologyNode(board, 'web', 26.5, 31)
  assert.deepEqual(board.nodes[0], defaultTopologyNode('web', 26.5, 31))
  board.links.push({ from: 'web', to: 'db', kind: 'relation', direction: 'none' })
  assert.deepEqual(decodeTopologyBoard(JSON.stringify(board)), board)
  assert.ok(topologyLinkMatches(board.links[0], 'db', 'web'))
  assert.deepEqual(removeTopologyNode(board, 'web'), { nodes: [defaultTopologyNode('db', 214, 20)], links: [] })
  assert.throws(() => decodeTopologyBoard(JSON.stringify({ ...board, links: [['db', 'missing']] })))
})

test('configured nodes reserve positions without moving old cards and arrows stop outside the cards', () => {
  const pinned = { id: 'entry', x: 20, y: 20, memo: 'Keep this' }
  const initial = { nodes: [pinned], links: [] }
  const layout = addTopologyNodes(initial, ['entry', 'middle', 'target', 'target'])
  assert.deepEqual(
    layout.nodes.map(({ id }) => id),
    ['entry', 'middle', 'target']
  )
  assert.equal(layout.nodes[0], pinned)
  assert.deepEqual(topologyLinkEndpoints(pinned, layout.nodes[1], true), { x1: 204, x2: 210, y1: 72, y2: 72 })
  const positioned = { ...layout.nodes[1], routeOnly: true }
  assert.deepEqual(decodeTopologyBoard(JSON.stringify({ nodes: [pinned, positioned], links: [] })).nodes, [
    defaultTopologyNode('entry', 20, 20, { memo: 'Keep this' }),
    defaultTopologyNode('middle', 214, 20, { routeOnly: true })
  ])
})

test('legacy grid positions retain their previous screen coordinates', () => {
  const legacy = { nodes: [{ id: 'web', column: 3, row: 2 }], links: [] }
  const board = decodeTopologyBoard(JSON.stringify(legacy))
  assert.deepEqual(board.nodes, [defaultTopologyNode('web', 602, 236)])
})

test('memos stay with their node when it moves or the board reloads', () => {
  const initial = addTopologyNode(addTopologyNode(decodeTopologyBoard(null), 'web'), 'db')
  const memo = '检查证书\n周五备份数据库'
  const named = updateTopologyNodeMemo(initial, 'web', memo)
  const moved = moveTopologyNode(named, 'web', 80, 40)
  const loaded = decodeTopologyBoard(JSON.stringify(moved))
  assert.deepEqual(loaded.nodes[0], defaultTopologyNode('web', 80, 40, { memo }))
  assert.equal(loaded.nodes[1].memo, undefined)
  assert.equal(updateTopologyNodeMemo(loaded, 'web', '').nodes[0].memo, undefined)
})

test('named links survive reload, while legacy links remain unnamed', () => {
  const nodes = [
    { id: 'web', x: 0, y: 0 },
    { id: 'db', x: 200, y: 0 },
    { id: 'backup', x: 400, y: 0 }
  ]
  const links = [
    ['web', 'db', 'backup'],
    ['db', 'backup']
  ]
  const board = decodeTopologyBoard(JSON.stringify({ nodes, links }))
  assert.deepEqual(board.links, [
    { from: 'web', to: 'db', name: 'backup', kind: 'relation', direction: 'none' },
    { from: 'db', to: 'backup', kind: 'relation', direction: 'none' }
  ])
  assert.deepEqual(removeTopologyNode(board, 'backup').links, [
    { from: 'web', to: 'db', name: 'backup', kind: 'relation', direction: 'none' }
  ])
  assert.throws(() => decodeTopologyBoard(JSON.stringify({ nodes, links: [['web', 'db', 123]] })))
})
