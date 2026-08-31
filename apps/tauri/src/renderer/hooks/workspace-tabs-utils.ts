import type { PaneFocusDirection, PaneNode, WorkspaceTab } from '@fileterm/core'
import { t } from '../i18n'
import type { LocalTab, StoredMainTabUiState, TerminalDockSendState } from './workspace-tabs-types'

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
