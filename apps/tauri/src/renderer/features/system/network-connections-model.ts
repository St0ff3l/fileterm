import type { NetworkSocket } from '@fileterm/core'

export function socketKey(socket: NetworkSocket) {
  return `${socket.protocol}|${socket.localAddress}|${socket.localPort}|${socket.pid ?? ''}|${socket.remoteAddress}|${socket.remotePort}|${socket.state}`
}

export function connectionsFor(listener: NetworkSocket, sockets: NetworkSocket[]) {
  const wildcard = ['*', '0.0.0.0', '::'].includes(listener.localAddress)
  const ipv6 = listener.localAddress.includes(':')
  return sockets.filter(
    (socket) =>
      !socket.listening &&
      socket.protocol === listener.protocol &&
      socket.localPort === listener.localPort &&
      (socket.localAddress === listener.localAddress ||
        (wildcard && (listener.localAddress === '*' || socket.localAddress.includes(':') === ipv6)))
  )
}

export function trafficTotal(sockets: NetworkSocket[], key: 'txBytes' | 'rxBytes'): number | null {
  if (!sockets.length || sockets.some((socket) => socket[key] === null)) return null
  return sockets.reduce((total, socket) => total + (socket[key] ?? 0), 0)
}

export function formatSocketBytes(value: number | null) {
  if (value === null) return '—'
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB']
  let index = 0
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024
    index++
  }
  return `${value.toFixed(index ? 1 : 0)} ${units[index]}`
}

export function socketEndpoint(address: string, port: number) {
  return `${address.includes(':') ? `[${address}]` : address}:${port}`
}

/** Keep row identities when a refresh inserts or reorders unrelated sockets. */
export function socketRowKeys(sockets: NetworkSocket[]) {
  const occurrences = new Map<string, number>()
  return sockets.map((socket) => {
    const key = socketKey(socket)
    const occurrence = occurrences.get(key) ?? 0
    occurrences.set(key, occurrence + 1)
    return `${key}|${occurrence}`
  })
}

export function socketAddressLabel(address: string, ipv4Label: string, ipv6Label: string) {
  if (address === '::') return ipv6Label
  if (address === '0.0.0.0') return ipv4Label
  return address
}

export type NetworkTableSort = { column: number; direction: 'asc' | 'desc' }

export function sortNetworkRows<T extends { sortValues: Array<string | number | null> }>(
  rows: T[],
  sort: NetworkTableSort
): T[] {
  return [...rows].sort((a, b) => {
    const left = a.sortValues[sort.column]
    const right = b.sortValues[sort.column]
    if (left == null) return right == null ? 0 : 1
    if (right == null) return -1
    const comparison =
      typeof left === 'number' && typeof right === 'number'
        ? left - right
        : String(left).localeCompare(String(right), undefined, { numeric: true })
    return sort.direction === 'asc' ? comparison : -comparison
  })
}
