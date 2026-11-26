import type { ConnectionDeviceRole } from '@fileterm/core'

type ServerListStatus = 'connected' | 'connecting' | 'idle'

export interface ServerListRow {
  id: string
  name: string
  address: string
  protocol: string
  status: ServerListStatus
  tags: string
  tagList?: string[]
  deviceRole?: ConnectionDeviceRole
}

export type ServerListColumn = 'name' | 'address' | 'protocol' | 'status' | 'tags'

export interface ServerListSort {
  column: ServerListColumn
  direction: 'ascending' | 'descending'
}

const statusOrder: Record<ServerListStatus, number> = { connected: 0, connecting: 1, idle: 2 }
const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' })

export function nextServerListSort(sort: ServerListSort | null, column: ServerListColumn): ServerListSort | null {
  if (sort?.column !== column) return { column, direction: 'ascending' }
  return sort.direction === 'ascending' ? { column, direction: 'descending' } : null
}

export function sortServerListRows(rows: ServerListRow[], sort: ServerListSort | null) {
  if (!sort) return rows
  const { column, direction } = sort
  return [...rows].sort((left, right) => {
    const comparison =
      column === 'status'
        ? statusOrder[left.status] - statusOrder[right.status]
        : collator.compare(left[column], right[column])
    return direction === 'ascending' ? comparison : -comparison
  })
}
