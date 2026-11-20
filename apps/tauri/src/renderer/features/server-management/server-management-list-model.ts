type ServerListStatus = 'connected' | 'connecting' | 'idle'

export interface ServerListRow {
  id: string
  name: string
  address: string
  protocol: string
  status: ServerListStatus
  tags: string
}

export type ServerListColumn = Exclude<keyof ServerListRow, 'id'>

export interface ServerListSort {
  column: ServerListColumn
  direction: 'ascending' | 'descending'
}

const statusOrder: Record<ServerListStatus, number> = { connected: 0, connecting: 1, idle: 2 }
const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' })

export function sortServerListRows(rows: ServerListRow[], { column, direction }: ServerListSort) {
  return [...rows].sort((left, right) => {
    const comparison =
      column === 'status'
        ? statusOrder[left.status] - statusOrder[right.status]
        : collator.compare(left[column], right[column])
    return direction === 'ascending' ? comparison : -comparison
  })
}
