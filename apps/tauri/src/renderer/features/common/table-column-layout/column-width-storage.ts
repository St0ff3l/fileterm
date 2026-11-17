import type { FileTermDesktopApi } from '@fileterm/core'

const pendingWrites = new Map<string, Promise<void>>()

export function columnWidthStorageKey(profileId: string, tableId: string) {
  return `fileterm.column-widths.v1:${encodeURIComponent(profileId)}:${encodeURIComponent(tableId)}`
}

export function parseColumnWidths(raw: string | null, count: number): number[] | null {
  if (!raw) return null
  try {
    const value: unknown = JSON.parse(raw)
    return Array.isArray(value) &&
      value.length === count &&
      value.every((width) => typeof width === 'number' && Number.isFinite(width) && width >= 50 && width <= 10000)
      ? value
      : null
  } catch {
    return null
  }
}

export async function readColumnWidths(api: FileTermDesktopApi, key: string, count: number) {
  // A just-unmounted table may still be committing its last drag.
  await pendingWrites.get(key)?.catch(() => undefined)
  return parseColumnWidths(await api.getUiStateItem(key), count)
}

export function writeColumnWidths(api: FileTermDesktopApi, key: string, widths: number[] | null) {
  const previous = pendingWrites.get(key) ?? Promise.resolve()
  const write = previous.catch(() => undefined).then(() => api.setUiStateItem(key, JSON.stringify(widths)))
  pendingWrites.set(key, write)
  const cleanup = () => {
    if (pendingWrites.get(key) === write) pendingWrites.delete(key)
  }
  void write.then(cleanup, cleanup)
  return write
}
