export const STATUS_MESSAGE_TIMEOUT_MS = 15_000
export const REMOTE_METHOD_ERROR_PREFIX = /Error invoking remote method '[^']+':\s*/i
export const DEFAULT_SIDEBAR_WIDTH = 214
export const DEFAULT_COMMAND_LIST_WIDTH = 300
export const SIDEBAR_SNAP_THRESHOLD = 10
export const SIDEBAR_MIN_WIDTH = 190
export const SIDEBAR_MAX_WIDTH = 360
export const FILE_PANEL_PREFERENCES_KEY = 'ui.file-panel-preferences.v1'
export const DEFAULT_FILE_PANEL_RATIO = 30
export const MAX_FILE_PANEL_RATIO = 70

// Four overview cards need 4 * 200px. Include the home body/page padding and
// a small scrollbar allowance so the last card stays on the same row at the
// configured 1150px minimum window width.
const HOME_OVERVIEW_MIN_MAIN_WIDTH = 930

export function getSidebarMaxWidth(windowWidth: number, isHomeWorkspace: boolean) {
  if (!isHomeWorkspace) {
    return SIDEBAR_MAX_WIDTH
  }

  return Math.min(SIDEBAR_MAX_WIDTH, Math.max(SIDEBAR_MIN_WIDTH, windowWidth - HOME_OVERVIEW_MIN_MAIN_WIDTH))
}

export function retainOpenTabUiState<T>(state: Record<string, T>, openTabIds: Set<string>) {
  const entries = Object.entries(state)
  if (entries.every(([tabId]) => openTabIds.has(tabId))) {
    return state
  }

  return Object.fromEntries(entries.filter(([tabId]) => openTabIds.has(tabId)))
}
