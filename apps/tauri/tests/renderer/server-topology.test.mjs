import assert from 'node:assert/strict'
import test from 'node:test'
import {
  addTopologyNode,
  addTopologyNodes,
  decodeTopologyBoard,
  moveTopologyNode,
  removeTopologyNode,
  topologyLinkMatches,
  topologyLinkEndpoints,
  updateTopologyNodeMemo
} from '../../src/renderer/features/server-management/server-topology-model.ts'

test('topology retains free positions and saves links with their endpoints', () => {
  let board = decodeTopologyBoard(null)
  board = addTopologyNode(addTopologyNode(board, 'web'), 'db')
  assert.equal(addTopologyNode(board, 'web'), board)
  assert.equal(moveTopologyNode(board, 'web', -1, 0), board)
  board = moveTopologyNode(board, 'web', 26.5, 31)
  assert.deepEqual(board.nodes[0], { id: 'web', x: 26.5, y: 31 })
  board.links.push(['web', 'db'])
  assert.deepEqual(decodeTopologyBoard(JSON.stringify(board)), board)
  assert.ok(topologyLinkMatches(board.links[0], 'db', 'web'))
  assert.deepEqual(removeTopologyNode(board, 'web'), { nodes: [{ id: 'db', x: 214, y: 20 }], links: [] })
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
  assert.deepEqual(topologyLinkEndpoints(pinned, layout.nodes[1], true), { x1: 180, x2: 210, y1: 50, y2: 50 })
  const positioned = { ...layout.nodes[1], routeOnly: true }
  assert.deepEqual(decodeTopologyBoard(JSON.stringify({ nodes: [pinned, positioned], links: [] })).nodes, [
    pinned,
    positioned
  ])
})

test('legacy grid positions retain their previous screen coordinates', () => {
  const legacy = { nodes: [{ id: 'web', column: 3, row: 2 }], links: [] }
  const board = decodeTopologyBoard(JSON.stringify(legacy))
  assert.deepEqual(board.nodes, [{ id: 'web', x: 602, y: 236 }])
})

test('memos stay with their node when it moves or the board reloads', () => {
  const initial = addTopologyNode(addTopologyNode(decodeTopologyBoard(null), 'web'), 'db')
  const memo = '检查证书\n周五备份数据库'
  const named = updateTopologyNodeMemo(initial, 'web', memo)
  const moved = moveTopologyNode(named, 'web', 80, 40)
  const loaded = decodeTopologyBoard(JSON.stringify(moved))
  assert.deepEqual(loaded.nodes[0], { id: 'web', x: 80, y: 40, memo })
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
  assert.deepEqual(board.links, links)
  assert.deepEqual(removeTopologyNode(board, 'backup').links, [['web', 'db', 'backup']])
  assert.throws(() => decodeTopologyBoard(JSON.stringify({ nodes, links: [['web', 'db', 123]] })))
})
