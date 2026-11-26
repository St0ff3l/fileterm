export const TOPOLOGY_STORAGE_KEY = 'fileterm:server-topology:v2'
export const LEGACY_TOPOLOGY_STORAGE_KEY = 'fileterm:server-topology:v1'
export const TOPOLOGY_LINK_NAME_LIMIT = 64
export const TOPOLOGY_MEMO_LIMIT = 2000
export const TOPOLOGY_NODE_LABEL_LIMIT = 80
export const TOPOLOGY_SERVICE_NAME_LIMIT = 64
export const TOPOLOGY_SERVICE_HOST_LIMIT = 253
export const TOPOLOGY_SERVICE_LIMIT = 16
export const TOPOLOGY_SERVICE_BOARD_LIMIT = 64
export const TOPOLOGY_SERVICE_RESULT_MAX_AGE_MS = 45_000

export const TOPOLOGY_NODE_DIMENSIONS = {
  compact: { width: 184, height: 104 },
  standard: { width: 256, height: 184 },
  expanded: { width: 344, height: 344 }
} as const

export const TOPOLOGY_NODE_SIZES = Object.keys(TOPOLOGY_NODE_DIMENSIONS) as Array<keyof typeof TOPOLOGY_NODE_DIMENSIONS>
export const TOPOLOGY_NODE_METRICS = ['ip', 'cpu', 'memory', 'disk', 'gpu', 'network', 'processes'] as const
export const DEFAULT_TOPOLOGY_NODE_METRICS: TopologyNodeMetric[] = ['cpu', 'memory']
export const TOPOLOGY_NODE_ROLES = [
  'server',
  'vm',
  'workstation',
  'firewall',
  'router',
  'switch',
  'database',
  'storage',
  'proxy',
  'custom'
] as const
export const TOPOLOGY_LINK_KINDS = ['relation', 'dependency', 'traffic'] as const
export const TOPOLOGY_LINK_DIRECTIONS = ['none', 'forward', 'reverse', 'both'] as const

const COLUMN_WIDTH = 194
const INSET = 20
const AUTO_LAYOUT_COLUMNS = 5
const MAX_POSITION = 200_000
const MAX_METRICS_BY_SIZE = { compact: 2, standard: 4, expanded: 7 } as const

export type TopologyNodeSize = keyof typeof TOPOLOGY_NODE_DIMENSIONS
export type TopologyNodeMetric = (typeof TOPOLOGY_NODE_METRICS)[number]
export type TopologyNodeRole = (typeof TOPOLOGY_NODE_ROLES)[number]
export type TopologyLinkKind = (typeof TOPOLOGY_LINK_KINDS)[number]
export type TopologyLinkDirection = (typeof TOPOLOGY_LINK_DIRECTIONS)[number]
export const DEFAULT_TOPOLOGY_NODE_METRIC_ORDER: TopologyNodeMetric[] = [...TOPOLOGY_NODE_METRICS]
export type TopologyServiceCheck = { id: string; name: string; host: string; port: number }
export type TopologyNode = {
  id: string
  x: number
  y: number
  role: TopologyNodeRole
  label?: string
  memo?: string
  routeOnly?: true
  size: TopologyNodeSize
  metrics: TopologyNodeMetric[]
  metricOrder: TopologyNodeMetric[]
  services: TopologyServiceCheck[]
}
export type TopologyLink = {
  from: string
  to: string
  name?: string
  kind: TopologyLinkKind
  direction: TopologyLinkDirection
}
export type TopologyBoard = { nodes: TopologyNode[]; links: TopologyLink[] }

export function topologyServiceProbeId(nodeId: string, serviceId: string) {
  return JSON.stringify([nodeId, serviceId])
}

export function topologyLinkMatches(link: TopologyLink, first: string, second: string) {
  return (link.from === first && link.to === second) || (link.from === second && link.to === first)
}

function validPosition(value: number) {
  return Number.isFinite(value) && value >= 0 && value <= MAX_POSITION
}

export function topologyNodeDimensions(node: Pick<TopologyNode, 'size'>, fontScale = 1) {
  const dimensions = TOPOLOGY_NODE_DIMENSIONS[node.size] ?? TOPOLOGY_NODE_DIMENSIONS.compact
  const layoutScale = Number.isFinite(fontScale) ? Math.max(1, fontScale) : 1
  return {
    ...dimensions,
    height: Math.ceil(dimensions.height * (1 + (layoutScale - 1) * 0.65))
  }
}

export function topologyNodeSizeForContent(
  size: TopologyNodeSize,
  metricCount: number,
  serviceCount: number
): TopologyNodeSize {
  const serviceRowsBySize = { compact: 1, standard: 2, expanded: 4 } as const
  const fits = (candidate: TopologyNodeSize) =>
    metricCount + Math.min(serviceCount, serviceRowsBySize[candidate]) <= MAX_METRICS_BY_SIZE[candidate]
  if (fits(size)) return size
  if (size === 'compact') return fits('standard') ? 'standard' : 'expanded'
  return 'expanded'
}

function completeTopologyMetricOrder(order: readonly TopologyNodeMetric[]): TopologyNodeMetric[] {
  const unique = [...new Set(order)]
  return [...unique, ...DEFAULT_TOPOLOGY_NODE_METRIC_ORDER.filter((metric) => !unique.includes(metric))]
}

function legacyTopologyMetricOrder(metrics: readonly TopologyNodeMetric[]): TopologyNodeMetric[] {
  const selected = new Set(metrics)
  let selectedIndex = 0
  return DEFAULT_TOPOLOGY_NODE_METRIC_ORDER.map((metric) => (selected.has(metric) ? metrics[selectedIndex++] : metric))
}

export function decodeTopologyBoard(raw: string | null, fontScale = 1): TopologyBoard {
  if (raw === null) return { nodes: [], links: [] }
  const value = JSON.parse(raw)
  if (!Array.isArray(value?.nodes) || !Array.isArray(value.links)) throw new Error('Invalid topology board')

  const ids = new Set<string>()
  const nodes: TopologyNode[] = []
  let totalServiceChecks = 0
  for (const node of value.nodes) {
    // Earlier boards stored grid cells. Resolve them to the original on-screen positions.
    const x = node?.x ?? (Number.isInteger(node?.column) ? INSET + node.column * COLUMN_WIDTH : NaN)
    const legacyRowHeight = topologyNodeDimensions({ size: 'compact' }, fontScale).height + 4
    const y = node?.y ?? (Number.isInteger(node?.row) ? INSET + node.row * legacyRowHeight : NaN)
    if (typeof node?.id !== 'string' || !node.id || ids.has(node.id) || !validPosition(x) || !validPosition(y)) {
      throw new Error('Invalid topology node')
    }
    ids.add(node.id)
    const { memo, routeOnly, label } = node
    if (memo !== undefined && (typeof memo !== 'string' || memo.length > TOPOLOGY_MEMO_LIMIT)) {
      throw new Error('Invalid topology memo')
    }
    if (label !== undefined && (typeof label !== 'string' || label.length > TOPOLOGY_NODE_LABEL_LIMIT)) {
      throw new Error('Invalid topology label')
    }
    const role = node.role ?? 'server'
    if (!TOPOLOGY_NODE_ROLES.includes(role)) throw new Error('Invalid topology role')
    const services = node.services ?? []
    const serviceIds = new Set<string>()
    let validServices = Array.isArray(services) && services.length <= TOPOLOGY_SERVICE_LIMIT
    if (validServices) {
      for (const service of services as TopologyServiceCheck[]) {
        if (
          !service ||
          typeof service !== 'object' ||
          typeof service.id !== 'string' ||
          !service.id ||
          service.id.length > 128 ||
          serviceIds.has(service.id) ||
          typeof service.name !== 'string' ||
          !service.name.trim() ||
          service.name.length > TOPOLOGY_SERVICE_NAME_LIMIT ||
          typeof service.host !== 'string' ||
          !service.host.trim() ||
          service.host.length > TOPOLOGY_SERVICE_HOST_LIMIT ||
          /\s/.test(service.host) ||
          !Number.isInteger(service.port) ||
          service.port < 1 ||
          service.port > 65535
        ) {
          validServices = false
          break
        }
        serviceIds.add(service.id)
      }
    }
    if (!validServices) throw new Error('Invalid topology service checks')
    totalServiceChecks += services.length
    if (totalServiceChecks > TOPOLOGY_SERVICE_BOARD_LIMIT) throw new Error('Invalid topology service checks')
    const requestedSize = node.size ?? 'compact'
    if (!TOPOLOGY_NODE_SIZES.includes(requestedSize)) throw new Error('Invalid topology node size')
    const metrics = node.metrics ?? DEFAULT_TOPOLOGY_NODE_METRICS
    if (
      !Array.isArray(metrics) ||
      metrics.some((metric: unknown) => !TOPOLOGY_NODE_METRICS.some((valid) => valid === metric))
    ) {
      throw new Error('Invalid topology node metrics')
    }
    const normalizedMetrics = [...new Set(metrics as TopologyNodeMetric[])]
    const requestedMetricOrder = node.metricOrder ?? legacyTopologyMetricOrder(normalizedMetrics)
    if (
      !Array.isArray(requestedMetricOrder) ||
      requestedMetricOrder.some((metric: unknown) => !TOPOLOGY_NODE_METRICS.some((valid) => valid === metric))
    ) {
      throw new Error('Invalid topology node metric order')
    }
    const metricOrder = completeTopologyMetricOrder(requestedMetricOrder as TopologyNodeMetric[])
    const enabledMetrics = new Set(normalizedMetrics)
    const orderedMetrics = metricOrder.filter((metric) => enabledMetrics.has(metric))
    const size = topologyNodeSizeForContent(requestedSize, orderedMetrics.length, services.length)
    nodes.push({
      id: node.id,
      x,
      y,
      role,
      ...(label?.trim() ? { label: label.trim() } : {}),
      ...(memo ? { memo } : {}),
      ...(routeOnly === true ? { routeOnly: true } : {}),
      size,
      metrics: orderedMetrics,
      metricOrder,
      services: services.map((service: TopologyServiceCheck) => ({
        ...service,
        name: service.name.trim(),
        host: service.host.trim()
      }))
    })
  }
  const links: TopologyLink[] = []
  for (const rawLink of value.links) {
    const link = Array.isArray(rawLink)
      ? {
          from: rawLink[0],
          to: rawLink[1],
          name: rawLink[2],
          kind: 'relation',
          direction: 'none'
        }
      : rawLink
    if (
      !link ||
      typeof link.from !== 'string' ||
      typeof link.to !== 'string' ||
      !ids.has(link.from) ||
      !ids.has(link.to) ||
      link.from === link.to ||
      !TOPOLOGY_LINK_KINDS.includes(link.kind ?? 'relation') ||
      !TOPOLOGY_LINK_DIRECTIONS.includes(link.direction ?? 'none') ||
      (link.name !== undefined && typeof link.name !== 'string') ||
      links.some((existing) => topologyLinkMatches(existing, link.from, link.to))
    ) {
      throw new Error('Invalid topology link')
    }
    const name = link.name?.trim().slice(0, TOPOLOGY_LINK_NAME_LIMIT)
    links.push({
      from: link.from,
      to: link.to,
      kind: link.kind ?? 'relation',
      direction: link.direction ?? 'none',
      ...(name ? { name } : {})
    })
  }
  return { nodes, links }
}

export function addTopologyNode(board: TopologyBoard, id: string, fontScale = 1): TopologyBoard {
  return addTopologyNodes(board, [id], fontScale)
}

export function addTopologyNodes(board: TopologyBoard, ids: Iterable<string>, fontScale = 1): TopologyBoard {
  const nodes = [...board.nodes]
  const present = new Set(nodes.map(({ id }) => id))
  const compactDimensions = topologyNodeDimensions({ size: 'compact' }, fontScale)
  const columnWidth = compactDimensions.width + 10
  const rowHeight = compactDimensions.height + 4
  let index = 0
  for (const id of ids) {
    if (!id || present.has(id)) continue
    for (;;) {
      const x = INSET + (index % AUTO_LAYOUT_COLUMNS) * columnWidth
      const y = INSET + Math.floor(index / AUTO_LAYOUT_COLUMNS) * rowHeight
      if (y > MAX_POSITION) return nodes.length === board.nodes.length ? board : { ...board, nodes }
      index++
      if (
        !nodes.some((node) => {
          const existing = topologyNodeDimensions(node, fontScale)
          const candidate = compactDimensions
          return (
            x < node.x + existing.width &&
            node.x < x + candidate.width &&
            y < node.y + existing.height &&
            node.y < y + candidate.height
          )
        })
      ) {
        nodes.push({
          id,
          x,
          y,
          role: 'server',
          size: 'compact',
          metrics: [...DEFAULT_TOPOLOGY_NODE_METRICS],
          metricOrder: [...DEFAULT_TOPOLOGY_NODE_METRIC_ORDER],
          services: []
        })
        present.add(id)
        break
      }
    }
  }
  return nodes.length === board.nodes.length ? board : { ...board, nodes }
}

export function topologyLinkEndpoints(first: TopologyNode, second: TopologyNode, directed: boolean, fontScale = 1) {
  const firstSize = topologyNodeDimensions(first, fontScale)
  const secondSize = topologyNodeDimensions(second, fontScale)
  const x1 = first.x + firstSize.width / 2
  const y1 = first.y + firstSize.height / 2
  const x2 = second.x + secondSize.width / 2
  const y2 = second.y + secondSize.height / 2
  const dx = x2 - x1
  const dy = y2 - y1
  const edgeInset = (width: number, height: number) =>
    Math.min(
      Math.abs(dx) ? width / 2 / Math.abs(dx) : Infinity,
      Math.abs(dy) ? height / 2 / Math.abs(dy) : Infinity,
      0.5
    )
  if (!directed) return { x1, y1, x2, y2 }
  const startInset = edgeInset(firstSize.width, firstSize.height)
  const endInset = edgeInset(secondSize.width + 8, secondSize.height + 8)
  return {
    x1: x1 + dx * startInset,
    y1: y1 + dy * startInset,
    x2: x2 - dx * endInset,
    y2: y2 - dy * endInset
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

export function updateTopologyNodeIdentity(
  board: TopologyBoard,
  id: string,
  role: TopologyNodeRole,
  label: string
): TopologyBoard {
  const node = board.nodes.find((item) => item.id === id)
  if (!node) return board
  const nextLabel = label.slice(0, TOPOLOGY_NODE_LABEL_LIMIT) || undefined
  if (node.role === role && (node.label ?? '') === (nextLabel ?? '')) return board
  return {
    ...board,
    nodes: board.nodes.map((item) => (item === node ? { ...node, role, label: nextLabel } : item))
  }
}

export function updateTopologyNodeServices(board: TopologyBoard, id: string, services: TopologyServiceCheck[]) {
  const node = board.nodes.find((item) => item.id === id)
  if (!node || services.length > TOPOLOGY_SERVICE_LIMIT) return board
  return {
    ...board,
    nodes: board.nodes.map((item) =>
      item === node
        ? {
            ...node,
            services: [...services],
            size: topologyNodeSizeForContent(node.size, node.metrics.length, services.length)
          }
        : item
    )
  }
}

export function updateTopologyLink(board: TopologyBoard, link: TopologyLink, replacement: TopologyLink) {
  if (!board.links.includes(link)) return board
  return { ...board, links: board.links.map((item) => (item === link ? replacement : item)) }
}

export function updateTopologyNodeSize(board: TopologyBoard, id: string, size: TopologyNodeSize): TopologyBoard {
  const node = board.nodes.find((item) => item.id === id)
  if (!node) return board
  const resolvedSize = topologyNodeSizeForContent(size, node.metrics.length, node.services.length)
  if (node.size === resolvedSize) return board
  return { ...board, nodes: board.nodes.map((item) => (item === node ? { ...node, size: resolvedSize } : item)) }
}

export function updateTopologyNodeMetrics(
  board: TopologyBoard,
  id: string,
  metrics: TopologyNodeMetric[]
): TopologyBoard {
  const node = board.nodes.find((item) => item.id === id)
  if (!node) return board
  const unique = [...new Set(metrics)]
  const enabledMetrics = new Set(unique)
  const orderedMetrics = node.metricOrder.filter((metric) => enabledMetrics.has(metric))
  if (
    orderedMetrics.length === node.metrics.length &&
    orderedMetrics.every((metric, index) => metric === node.metrics[index])
  )
    return board
  const size = topologyNodeSizeForContent(node.size, orderedMetrics.length, node.services.length)
  return {
    ...board,
    nodes: board.nodes.map((item) => (item === node ? { ...node, metrics: orderedMetrics, size } : item))
  }
}

export function updateTopologyNodeMetricOrder(
  board: TopologyBoard,
  id: string,
  metricOrder: TopologyNodeMetric[]
): TopologyBoard {
  const node = board.nodes.find((item) => item.id === id)
  if (!node) return board
  const normalizedOrder = completeTopologyMetricOrder(metricOrder)
  if (normalizedOrder.every((metric, index) => metric === node.metricOrder[index])) return board
  const enabledMetrics = new Set(node.metrics)
  const metrics = normalizedOrder.filter((metric) => enabledMetrics.has(metric))
  return {
    ...board,
    nodes: board.nodes.map((item) => (item === node ? { ...node, metricOrder: normalizedOrder, metrics } : item))
  }
}

export function removeTopologyNode(board: TopologyBoard, id: string): TopologyBoard {
  return {
    nodes: board.nodes.filter((node) => node.id !== id),
    links: board.links.filter(({ from, to }) => from !== id && to !== id)
  }
}
