import type { PaneFocusDirection, PaneNode, WorkspaceSnapshot, WorkspaceTab } from '@fileterm/core'
import { t } from '../i18n'
import type { LocalTab, StoredMainTabUiState, TerminalDockSendState } from './workspace-tabs-types'
import type { OrderedTabEntry } from '../features/layout/tab-bar'
import { entryHostGroupKey } from '../features/layout/host-groups'

type PaneBounds = {
  tabId: string
  x: number
  y: number
  width: number
  height: number
}

export function formatSystemInfoTabTitle(sourceTabTitle: string) {
  return `${t.systemInfoTabTitle} · ${sourceTabTitle || t.untitledTab}`
}

export function formatSessionTabTitle(tab: WorkspaceTab) {
  if (tab.sessionType === 'local' && (!tab.title || tab.title === 'Local Terminal')) {
    return t.localTerminal
  }
  return tab.title || t.untitledTab
}

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

export function uniqueStrings(values: string[]) {
  return [...new Set(values)]
}

export function reconcileTabOrder(previousOrder: string[], allKeys: string[], pendingReplacement: boolean) {
  if (pendingReplacement) return previousOrder
  const allKeySet = new Set(allKeys)
  const kept = uniqueStrings(previousOrder.filter((key) => allKeySet.has(key)))
  const keptSet = new Set(kept)
  const missing = allKeys.filter((key) => !keptSet.has(key))
  const removedKeys = previousOrder.filter((key) => !allKeySet.has(key))
  if (missing.length && removedKeys.length) {
    let missingIdx = 0
    const replacedOrder: string[] = []
    for (const key of previousOrder) {
      if (allKeySet.has(key)) replacedOrder.push(key)
      else if (missingIdx < missing.length) replacedOrder.push(missing[missingIdx++])
    }
    const next = uniqueStrings([...replacedOrder, ...missing.slice(missingIdx)])
    return areStringArraysEqual(previousOrder, next) ? previousOrder : next
  }
  const next = [...kept, ...missing]
  return areStringArraysEqual(previousOrder, next) ? previousOrder : next
}

/** Snapshot tabs must be visible before the effect persists their order. */
export function buildOrderedTabs(
  tabOrder: string[],
  localTabs: LocalTab[],
  sessionTabs: WorkspaceTab[],
  options?: { deferMissingSessionTabs?: boolean }
): OrderedTabEntry[] {
  const entries: OrderedTabEntry[] = [
    ...localTabs.map((tab) => ({
      key: `home:${tab.id}`,
      kind: 'local' as const,
      id: tab.id,
      title: tab.title,
      tabKind: tab.kind,
      sessionTabId: tab.kind === 'system' ? tab.sessionTabId : undefined
    })),
    ...sessionTabs.map((tab) => ({ key: `session:${tab.id}`, kind: 'session' as const, tab }))
  ]
  const byKey = new Map(entries.map((entry) => [entry.key, entry]))
  const keys = options?.deferMissingSessionTabs
    ? uniqueStrings(tabOrder)
    : uniqueStrings([...tabOrder, ...byKey.keys()])
  return keys.flatMap((key) => {
    const entry = byKey.get(key)
    return entry ? [entry] : []
  })
}

/** Shows a requested tab as active until the backend snapshot confirms or replaces it. */
export function withPendingActiveTab(workspace: WorkspaceSnapshot, pendingTabId: string | null): WorkspaceSnapshot {
  if (!pendingTabId || workspace.activeTabId === pendingTabId) {
    return workspace
  }
  if (!workspace.tabs.some((tab) => tab.id === pendingTabId)) {
    return workspace
  }
  return { ...workspace, activeTabId: pendingTabId }
}

export function uniqueItemsById<T extends { id: string }>(items: T[]) {
  const seen = new Set<string>()
  return items.filter((item) => {
    if (seen.has(item.id)) {
      return false
    }
    seen.add(item.id)
    return true
  })
}

function collectPaneBounds(node: PaneNode, x: number, y: number, width: number, height: number, result: PaneBounds[]) {
  if (node.kind === 'leaf') {
    result.push({ tabId: node.tabId, x, y, width, height })
    return
  }

  const weights =
    node.weights.length === node.children.length ? node.weights : node.children.map(() => 1 / node.children.length)
  let offset = 0
  node.children.forEach((child, index) => {
    const weight = weights[index] ?? 1 / node.children.length
    if (node.direction === 'row') {
      const childWidth = width * weight
      collectPaneBounds(child, x + offset, y, childWidth, height, result)
      offset += childWidth
      return
    }

    const childHeight = height * weight
    collectPaneBounds(child, x, y + offset, width, childHeight, result)
    offset += childHeight
  })
}

export function collectPaneLeafTabIds(node: PaneNode, result: string[] = []) {
  if (node.kind === 'leaf') {
    result.push(node.tabId)
    return result
  }

  for (const child of node.children) {
    collectPaneLeafTabIds(child, result)
  }
  return result
}

export function findAdjacentPaneTabId(
  paneRoot: PaneNode,
  activePaneTabId: string,
  direction: PaneFocusDirection
): string | null {
  const panes: PaneBounds[] = []
  collectPaneBounds(paneRoot, 0, 0, 1, 1, panes)
  const active = panes.find((pane) => pane.tabId === activePaneTabId)
  if (!active) {
    return null
  }

  const activeCenterX = active.x + active.width / 2
  const activeCenterY = active.y + active.height / 2
  const candidates = panes
    .filter((pane) => pane.tabId !== activePaneTabId)
    .map((pane) => {
      const centerX = pane.x + pane.width / 2
      const centerY = pane.y + pane.height / 2
      const isInDirection =
        (direction === 'left' && centerX < activeCenterX) ||
        (direction === 'right' && centerX > activeCenterX) ||
        (direction === 'up' && centerY < activeCenterY) ||
        (direction === 'down' && centerY > activeCenterY)
      if (!isInDirection) {
        return null
      }

      const primaryDistance =
        direction === 'left' || direction === 'right'
          ? Math.abs(centerX - activeCenterX)
          : Math.abs(centerY - activeCenterY)
      const crossDistance =
        direction === 'left' || direction === 'right'
          ? Math.abs(centerY - activeCenterY)
          : Math.abs(centerX - activeCenterX)
      return { pane, score: primaryDistance * 2 + crossDistance }
    })
    .filter((candidate): candidate is { pane: PaneBounds; score: number } => candidate !== null)
    .sort((left, right) => left.score - right.score)

  return candidates[0]?.pane.tabId ?? null
}

export function parseStoredMainTabUiState(raw: string | null | undefined): StoredMainTabUiState | null {
  if (!raw) {
    return null
  }

  try {
    const parsed = JSON.parse(raw) as Partial<StoredMainTabUiState>
    const localTabs = uniqueItemsById(
      Array.isArray(parsed.localTabs)
        ? parsed.localTabs.filter((tab): tab is LocalTab => {
            if (!tab || typeof tab !== 'object' || typeof tab.id !== 'string' || typeof tab.title !== 'string') {
              return false
            }
            if (tab.kind === 'transfers') {
              return (
                (tab.initialTabId === undefined || typeof tab.initialTabId === 'string') &&
                (tab.initialPath === undefined || typeof tab.initialPath === 'string')
              )
            }
            if (tab.kind === 'home') {
              return true
            }
            return (
              tab.kind === 'system' &&
              typeof (tab as Extract<LocalTab, { kind: 'system' }>).sessionTabId === 'string' &&
              typeof (tab as Extract<LocalTab, { kind: 'system' }>).sourceTabTitle === 'string'
            )
          })
        : []
    )
    const tabOrder = Array.isArray(parsed.tabOrder)
      ? uniqueStrings(parsed.tabOrder.filter((entry): entry is string => typeof entry === 'string'))
      : []
    const systemSidebarCollapsedByTabId =
      parsed.systemSidebarCollapsedByTabId &&
      typeof parsed.systemSidebarCollapsedByTabId === 'object' &&
      !Array.isArray(parsed.systemSidebarCollapsedByTabId)
        ? Object.fromEntries(
            Object.entries(parsed.systemSidebarCollapsedByTabId).filter(
              ([tabId, collapsed]) => tabId.length > 0 && typeof collapsed === 'boolean'
            )
          )
        : {}

    return {
      localTabs,
      activeLocalTabId: typeof parsed.activeLocalTabId === 'string' ? parsed.activeLocalTabId : null,
      nextHomeTabNumber:
        typeof parsed.nextHomeTabNumber === 'number' && Number.isFinite(parsed.nextHomeTabNumber)
          ? Math.max(1, Math.floor(parsed.nextHomeTabNumber))
          : 1,
      tabOrder,
      systemSidebarCollapsedByTabId
    }
  } catch {
    return null
  }
}

export function createInitialMainTabUiState(
  enabled: boolean,
  stored: StoredMainTabUiState | null
): StoredMainTabUiState {
  if (!enabled) {
    return {
      localTabs: [],
      activeLocalTabId: null,
      nextHomeTabNumber: 1,
      tabOrder: [],
      systemSidebarCollapsedByTabId: {}
    }
  }

  if (stored) {
    return stored
  }

  return {
    localTabs: [{ id: 'home-1', kind: 'home', title: t.untitledTab }],
    activeLocalTabId: 'home-1',
    nextHomeTabNumber: 2,
    tabOrder: ['home:home-1'],
    systemSidebarCollapsedByTabId: {}
  }
}

export function resolveFallbackHomeTabId(localTabs: LocalTab[], tabOrder: string[]) {
  for (let index = tabOrder.length - 1; index >= 0; index -= 1) {
    const key = tabOrder[index]
    if (!key?.startsWith('home:')) {
      continue
    }
    const id = key.slice('home:'.length)
    if (localTabs.some((tab) => tab.kind === 'home' && tab.id === id)) {
      return id
    }
  }

  return [...localTabs].reverse().find((tab) => tab.kind === 'home')?.id ?? null
}

export function isDefaultPlaceholderHomeTab(tab: LocalTab) {
  return tab.kind === 'home' && tab.id === 'home-1' && tab.title === t.untitledTab
}

export function isTabActivelyConnected(tab: WorkspaceTab | null | undefined) {
  return Boolean(tab && (tab.status === 'connecting' || tab.status === 'connected'))
}

export function createDefaultTerminalDockSendState(): TerminalDockSendState {
  return {
    scope: 'current',
    selectedTabIds: [],
    rememberSelection: false
  }
}

/** Prefer the visible neighbour on the right, then the left, before leaving a host group. */
export function nextTabAfterClose(
  entries: OrderedTabEntry[],
  closedKey: string,
  grouped: boolean,
  removedKeys: string[] = [closedKey]
) {
  const closed = entries.find((entry) => entry.key === closedKey)
  if (!closed) return null
  const sessions = entries.flatMap((entry) => (entry.kind === 'session' ? [entry.tab] : []))
  const groupKey = entryHostGroupKey(closed, sessions)
  const candidates = [
    ...entries.slice(entries.indexOf(closed) + 1),
    ...entries.slice(0, entries.indexOf(closed)).reverse()
  ].filter((entry) => !removedKeys.includes(entry.key))
  return (
    (grouped ? candidates.find((entry) => entryHostGroupKey(entry, sessions) === groupKey) : undefined) ??
    candidates[0] ??
    null
  )
}
