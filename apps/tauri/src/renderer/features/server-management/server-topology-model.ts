export const TOPOLOGY_STORAGE_KEY = 'fileterm:server-topology:v1'
export const TOPOLOGY_NODE_WIDTH = 156
export const TOPOLOGY_NODE_HEIGHT = 60
export const TOPOLOGY_LINK_NAME_LIMIT = 64
export const TOPOLOGY_MEMO_LIMIT = 2000

const COLUMN_WIDTH = 194
const ROW_HEIGHT = 108
const INSET = 20
const AUTO_LAYOUT_COLUMNS = 5
const MAX_POSITION = 200_000

export type TopologyNode = { id: string; x: number; y: number; memo?: string }
export type TopologyLink = [first: string, second: string, name?: string]
export type TopologyBoard = { nodes: TopologyNode[]; links: TopologyLink[] }

export function topologyLinkMatches([a, b]: TopologyLink, first: string, second: string) {
  return (a === first && b === second) || (a === second && b === first)
}

function validPosition(value: number) {
  return Number.isFinite(value) && value >= 0 && value <= MAX_POSITION
}

export function decodeTopologyBoard(raw: string | null): TopologyBoard {
  if (raw === null) return { nodes: [], links: [] }
  const value = JSON.parse(raw)
  if (!Array.isArray(value?.nodes) || !Array.isArray(value.links)) throw new Error('Invalid topology board')

  const ids = new Set<string>()
  const nodes: TopologyNode[] = []
  for (const node of value.nodes) {
    // Earlier boards stored grid cells. Resolve them to the original on-screen positions.
    const x = node?.x ?? (Number.isInteger(node?.column) ? INSET + node.column * COLUMN_WIDTH : NaN)
    const y = node?.y ?? (Number.isInteger(node?.row) ? INSET + node.row * ROW_HEIGHT : NaN)
    if (typeof node?.id !== 'string' || !node.id || ids.has(node.id) || !validPosition(x) || !validPosition(y)) {
      throw new Error('Invalid topology node')
    }
    ids.add(node.id)
    const { memo } = node
    if (memo !== undefined && (typeof memo !== 'string' || memo.length > TOPOLOGY_MEMO_LIMIT)) {
      throw new Error('Invalid topology memo')
    }
    nodes.push(memo ? { id: node.id, x, y, memo } : { id: node.id, x, y })
  }
  const links: TopologyLink[] = []
  for (const link of value.links) {
    if (
      !Array.isArray(link) ||
      (link.length !== 2 && !(link.length === 3 && typeof link[2] === 'string')) ||
      !ids.has(link[0]) ||
      !ids.has(link[1]) ||
      link[0] === link[1] ||
      links.some((existing) => topologyLinkMatches(existing, link[0], link[1]))
    ) {
      throw new Error('Invalid topology link')
    }
    const name = link[2]?.trim().slice(0, TOPOLOGY_LINK_NAME_LIMIT)
    links.push(name ? [link[0], link[1], name] : [link[0], link[1]])
  }
  return { nodes, links }
}

export function addTopologyNode(board: TopologyBoard, id: string): TopologyBoard {
  if (!id || board.nodes.some((node) => node.id === id)) return board
  for (let index = 0; ; index++) {
    const x = INSET + (index % AUTO_LAYOUT_COLUMNS) * COLUMN_WIDTH
    const y = INSET + Math.floor(index / AUTO_LAYOUT_COLUMNS) * ROW_HEIGHT
    if (y > MAX_POSITION) return board
    if (
      !board.nodes.some(
        (node) => Math.abs(node.x - x) < TOPOLOGY_NODE_WIDTH && Math.abs(node.y - y) < TOPOLOGY_NODE_HEIGHT
      )
    ) {
      return { ...board, nodes: [...board.nodes, { id, x, y }] }
    }
  }
}

export function moveTopologyNode(board: TopologyBoard, id: string, x: number, y: number): TopologyBoard {
  const node = board.nodes.find((item) => item.id === id)
  if (!node || !validPosition(x) || !validPosition(y) || (node.x === x && node.y === y)) return board
  return { ...board, nodes: board.nodes.map((item) => (item === node ? { ...node, x, y } : item)) }
}

export function updateTopologyNodeMemo(board: TopologyBoard, id: string, memo: string): TopologyBoard {
  const node = board.nodes.find((item) => item.id === id)
  if (!node || memo.length > TOPOLOGY_MEMO_LIMIT || memo === (node.memo ?? '')) return board
  return { ...board, nodes: board.nodes.map((item) => (item === node ? { ...node, memo: memo || undefined } : item)) }
}

export function removeTopologyNode(board: TopologyBoard, id: string): TopologyBoard {
  return {
    nodes: board.nodes.filter((node) => node.id !== id),
    links: board.links.filter(([first, second]) => first !== id && second !== id)
  }
}
