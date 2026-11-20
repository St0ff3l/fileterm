export const SERVER_SAVED_VIEWS_KEY = 'server-management.saved-views'
export const SERVER_STATUS_FILTERS = ['all', 'connected', 'connecting', 'idle'] as const

export interface ServerViewFilters {
  query: string
  tag: string
  status: (typeof SERVER_STATUS_FILTERS)[number]
}

export interface ServerSavedView extends ServerViewFilters {
  id: string
  name: string
}

export function decodeServerSavedViews(raw: string | null): ServerSavedView[] {
  if (raw === null) return []
  const entries = JSON.parse(raw)
  const ids = new Set<string>()
  if (
    !Array.isArray(entries) ||
    !entries.every((entry) => {
      if (
        !entry ||
        typeof entry.id !== 'string' ||
        !entry.id ||
        ids.has(entry.id) ||
        typeof entry.name !== 'string' ||
        !entry.name.trim() ||
        typeof entry.query !== 'string' ||
        typeof entry.tag !== 'string' ||
        !SERVER_STATUS_FILTERS.includes(entry.status)
      ) {
        return false
      }
      ids.add(entry.id)
      return true
    })
  ) {
    throw new Error('Invalid server saved views')
  }
  return entries
}
