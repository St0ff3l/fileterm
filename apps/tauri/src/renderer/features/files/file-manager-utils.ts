import type { LocalFileItem, RemoteFileItem } from '@fileterm/core'
import type { AppIconName } from '../common/app-icon'
import { getDisplayFileIconName, getDisplayFileTypeSortKey } from './file-kind'
import { parseFileModified } from './file-time'
import type { RemoteFileSortState } from './file-tables'

export function areStringArraysEqual(left: string[], right: string[]) {
  if (left === right) {
    return true
  }
  if (left.length !== right.length) {
    return false
  }
  for (let index = 0; index < left.length; index += 1) {
    if (left[index] !== right[index]) {
      return false
    }
  }
  return true
}

function compareText(left: string, right: string) {
  return left.localeCompare(right, undefined, { numeric: true, sensitivity: 'base' })
}

export function getDragPreviewIcon(items: Array<LocalFileItem | RemoteFileItem>): AppIconName {
  const firstItem = items[0]
  return firstItem ? getDisplayFileIconName(firstItem) : 'file'
}

function parseSortableSize(value: string) {
  if (!value || value === '-') {
    return 0
  }

  const match = value.trim().match(/^([\d.]+)\s*([A-Za-z]+)$/)
  if (!match) {
    return 0
  }

  const amount = Number.parseFloat(match[1])
  if (!Number.isFinite(amount)) {
    return 0
  }

  const unit = match[2].toUpperCase()
  const units: Record<string, number> = {
    B: 1,
    KB: 1024,
    MB: 1024 ** 2,
    GB: 1024 ** 3,
    TB: 1024 ** 4
  }

  return amount * (units[unit] ?? 1)
}

function compareRemoteFilesByField(left: RemoteFileItem, right: RemoteFileItem, sort: RemoteFileSortState) {
  const direction = sort.direction === 'asc' ? 1 : -1

  switch (sort.field) {
    case 'size':
      return (parseSortableSize(left.size) - parseSortableSize(right.size)) * direction
    case 'type':
      return compareText(getDisplayFileTypeSortKey(left), getDisplayFileTypeSortKey(right)) * direction
    case 'modified':
      return ((parseFileModified(left.modified) ?? 0) - (parseFileModified(right.modified) ?? 0)) * direction
    case 'permission':
      return compareText(left.permission ?? '', right.permission ?? '') * direction
    case 'ownerGroup':
      return compareText(left.ownerGroup ?? '', right.ownerGroup ?? '') * direction
    case 'name':
    default:
      return compareText(left.name, right.name) * direction
  }
}

export function sortRemoteFiles(rows: RemoteFileItem[], sort: RemoteFileSortState) {
  const parentRow = rows.find((row) => row.name === '..') ?? null
  const sortableRows = rows.filter((row) => row.name !== '..')

  sortableRows.sort((left, right) => {
    if (sort.field !== 'type' && left.type !== right.type) {
      return left.type === 'folder' ? -1 : 1
    }

    const byField = compareRemoteFilesByField(left, right, sort)
    if (byField !== 0) {
      return byField
    }

    if (left.type !== right.type) {
      return left.type === 'folder' ? -1 : 1
    }

    return compareText(left.name, right.name)
  })

  return parentRow ? [parentRow, ...sortableRows] : sortableRows
}
