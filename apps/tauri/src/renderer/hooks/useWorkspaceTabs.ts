import { useEffect, useMemo, useRef, useState, type MouseEvent } from 'react'
import type {
  ConnectionProfile,
  CommandExecutionOptions,
  FileTermDesktopApi,
  PaneFocusDirection,
  PaneNode,
  SessionSnapshot,
  WorkspaceSnapshot,
  WorkspaceTab
} from '@fileterm/core'
import {
  copyText,
  homeTabKey,
  insertTabKeyAfter,
  reorderTabKeys,
  sessionTabKey,
  settledResultsError
} from '../app/app-utils'
import { resolveSelectedTabIds, type SendScope, type SessionSendTarget } from '../features/common/session-send-targets'
import type { OrderedTabEntry, TabContextTarget } from '../features/layout/TabBar'
import { setLocale, t, type AppLocale } from '../i18n'

const MAIN_TAB_UI_STATE_KEY = 'main.tab-ui'

export type LocalTab =
  | { id: string; kind: 'home'; title: string }
  | { id: string; kind: 'system'; title: string; sessionTabId: string; sourceTabTitle: string }

export type StoredMainTabUiState = {
  localTabs: LocalTab[]
  activeLocalTabId: string | null
  nextHomeTabNumber: number
  tabOrder: string[]
  systemSidebarCollapsedByTabId: Record<string, boolean>
}

export type TerminalDockSendState = {
  scope: SendScope
  selectedTabIds: string[]
  rememberSelection: boolean
}

export type WorkspaceTabContextMenu = {
  x: number
  y: number
  target: TabContextTarget
}

export type ShortcutCloseConfirm = {
  tabId: string
  title: string
  variant: 'connecting' | 'active-session' | 'active-last-session'
}

export type WorkspaceTabContextAction =
  'copy' | 'clone' | 'connect' | 'connectAll' | 'disconnect' | 'close' | 'closeOthers' | 'closeAll'

export type WorkspaceStageKind = 'home' | 'session' | 'system'
export type WorkspaceNavigationDirection = 'up' | 'down'

type PaneBounds = {
  tabId: string
  x: number
  y: number
  width: number
  height: number
}

export type UseWorkspaceTabsOptions = {
  desktopApi?: FileTermDesktopApi
  workspace: WorkspaceSnapshot
  isMainWorkspaceWindow: boolean
  hasLoadedInitialSnapshot: boolean
  locale: AppLocale
  isBusy: boolean
  closeActiveRequestVersion: number
  newTabRequestVersion: number
  splitPaneRequest: { id: number; direction: 'row' | 'column' } | null
  paneFocusRequest: { id: number; direction: PaneFocusDirection } | null
  onSnapshot(snapshot: WorkspaceSnapshot): void
  onBusyChange(isBusy: boolean): void
  onStatusMessage(message: string | null): void
  onError(scope: string, error: unknown): void
  onCloseCurrentWindow(): void
  onRequestQuit(): void
}

function formatSystemInfoTabTitle(sourceTabTitle: string) {
  return `${t.systemInfoTabTitle} · ${sourceTabTitle || t.untitledTab}`
}

function areStringArraysEqual(left: string[], right: string[]) {
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

function uniqueStrings(values: string[]) {
  return [...new Set(values)]
}

function uniqueItemsById<T extends { id: string }>(items: T[]) {
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

function collectPaneLeafTabIds(node: PaneNode, result: string[] = []) {
  if (node.kind === 'leaf') {
    result.push(node.tabId)
    return result
  }

  for (const child of node.children) {
    collectPaneLeafTabIds(child, result)
  }
  return result
}

function findAdjacentPaneTabId(
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

function parseStoredMainTabUiState(raw: string | null | undefined): StoredMainTabUiState | null {
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

function createInitialMainTabUiState(enabled: boolean, stored: StoredMainTabUiState | null): StoredMainTabUiState {
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

function resolveFallbackHomeTabId(localTabs: LocalTab[], tabOrder: string[]) {
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

function isDefaultPlaceholderHomeTab(tab: LocalTab) {
  return tab.kind === 'home' && tab.id === 'home-1' && tab.title === t.untitledTab
}

function isTabActivelyConnected(tab: WorkspaceTab | null | undefined) {
  return Boolean(tab && (tab.status === 'connecting' || tab.status === 'connected'))
}

function createDefaultTerminalDockSendState(): TerminalDockSendState {
  return {
    scope: 'current',
    selectedTabIds: [],
    rememberSelection: false
  }
}

export function useWorkspaceTabs({
  desktopApi,
  workspace,
  isMainWorkspaceWindow,
  hasLoadedInitialSnapshot,
  locale,
  isBusy,
  closeActiveRequestVersion,
  newTabRequestVersion,
  splitPaneRequest,
  paneFocusRequest,
  onSnapshot,
  onBusyChange,
  onStatusMessage,
  onError,
  onCloseCurrentWindow,
  onRequestQuit
}: UseWorkspaceTabsOptions) {
  const initialMainTabUiState = createInitialMainTabUiState(isMainWorkspaceWindow, null)
  const [localTabs, setLocalTabs] = useState<LocalTab[]>(() => initialMainTabUiState.localTabs)
  const [activeLocalTabId, setActiveLocalTabId] = useState<string | null>(() => initialMainTabUiState.activeLocalTabId)
  const [nextHomeTabNumber, setNextHomeTabNumber] = useState(() => initialMainTabUiState.nextHomeTabNumber)
  const [tabOrder, setTabOrder] = useState<string[]>(() => initialMainTabUiState.tabOrder)
  const [hasHydratedMainTabUiState, setHasHydratedMainTabUiState] = useState(!isMainWorkspaceWindow)
  const [terminalDockSendStateByTabId, setTerminalDockSendStateByTabId] = useState<
    Record<string, TerminalDockSendState>
  >({})
  const [draggingTabKey, setDraggingTabKey] = useState<string | null>(null)
  const [tabContextMenu, setTabContextMenu] = useState<WorkspaceTabContextMenu | null>(null)
  const [shortcutCloseConfirm, setShortcutCloseConfirm] = useState<ShortcutCloseConfirm | null>(null)
  const [closingSessionTabIds, setClosingSessionTabIds] = useState<string[]>([])
  const [systemSidebarCollapsedByTabId, setSystemSidebarCollapsedByTabId] = useState<Record<string, boolean>>(
    () => initialMainTabUiState.systemSidebarCollapsedByTabId
  )

  const localTabsRef = useRef(localTabs)
  const pendingHomeReplacementKeyRef = useRef<string | null>(null)
  const pendingProfileOpenIdRef = useRef<string | null>(null)
  const hasSanitizedStoredPlaceholderRef = useRef(false)
  const handledCloseActiveRequestVersionRef = useRef(0)
  const handledNewTabRequestVersionRef = useRef(0)
  const handledSplitPaneRequestIdRef = useRef(0)
  const handledPaneFocusRequestIdRef = useRef(0)
  const splitCurrentPaneRef = useRef<(direction: 'row' | 'column') => Promise<void>>(async () => {})
  const focusAdjacentPaneRef = useRef<(direction: PaneFocusDirection) => Promise<void>>(async () => {})

  useEffect(() => {
    localTabsRef.current = localTabs
  }, [localTabs])

  useEffect(() => {
    if (!desktopApi?.getUiStateItem || !isMainWorkspaceWindow) {
      setHasHydratedMainTabUiState(true)
      return
    }

    const uiStateApi = desktopApi
    let canceled = false

    async function hydrateMainTabUiState() {
      try {
        const raw = await uiStateApi.getUiStateItem(MAIN_TAB_UI_STATE_KEY)
        const storedState = parseStoredMainTabUiState(raw)
        if (!storedState || canceled) {
          return
        }

        setLocalTabs(storedState.localTabs)
        setActiveLocalTabId(storedState.activeLocalTabId)
        setNextHomeTabNumber(storedState.nextHomeTabNumber)
        setTabOrder(storedState.tabOrder)
        setSystemSidebarCollapsedByTabId(storedState.systemSidebarCollapsedByTabId)
      } catch {
        // Fall back to the initial local tab state when persisted UI state cannot be read.
      } finally {
        if (!canceled) {
          setHasHydratedMainTabUiState(true)
        }
      }
    }

    void hydrateMainTabUiState()

    return () => {
      canceled = true
    }
  }, [desktopApi, isMainWorkspaceWindow])

  const closingSessionTabIdSet = useMemo(() => new Set(closingSessionTabIds), [closingSessionTabIds])
  // 分屏 leaf tab id 集合：新建的 leaf 不在 tab bar 显示；持有 paneRoot 的
  // root tab 仍需保留，不能因为它也作为树中的第一个 leaf 而被误隐藏。
  const leafTabIds = useMemo(() => {
    const ids = new Set<string>()
    for (const tab of workspace.tabs) {
      if (tab.paneRoot) {
        for (const leafTabId of collectPaneLeafTabIds(tab.paneRoot)) {
          ids.add(leafTabId)
        }
        ids.delete(tab.id)
      }
    }
    return ids
  }, [workspace.tabs])
  const visibleWorkspaceTabs = useMemo(
    () =>
      uniqueItemsById(
        workspace.tabs.filter(
          (tab) => !closingSessionTabIdSet.has(tab.id) && !tab.paneRootTabId && !leafTabIds.has(tab.id)
        )
      ),
    [closingSessionTabIdSet, workspace.tabs, leafTabIds]
  )

  useEffect(() => {
    setLocale(locale)
    setLocalTabs((previousTabs) => {
      let changed = false
      const nextTabs = previousTabs.map((tab) => {
        if (tab.kind === 'home') {
          if (tab.title === t.untitledTab) {
            return tab
          }
          changed = true
          return { ...tab, title: t.untitledTab }
        }

        const sourceTabTitle =
          visibleWorkspaceTabs.find((entry) => entry.id === tab.sessionTabId)?.title ?? tab.sourceTabTitle
        const title = formatSystemInfoTabTitle(sourceTabTitle)
        if (tab.sourceTabTitle === sourceTabTitle && tab.title === title) {
          return tab
        }
        changed = true
        return {
          ...tab,
          sourceTabTitle,
          title
        }
      })
      return changed ? nextTabs : previousTabs
    })
  }, [locale, visibleWorkspaceTabs])

  useEffect(() => {
    if (!isMainWorkspaceWindow || !hasLoadedInitialSnapshot || !hasHydratedMainTabUiState) {
      return
    }

    const allKeys = uniqueStrings([
      ...localTabs.map((tab) => homeTabKey(tab.id)),
      ...visibleWorkspaceTabs.map((tab) => sessionTabKey(tab.id))
    ])
    const allKeySet = new Set(allKeys)

    setTabOrder((previousOrder) => {
      const kept = uniqueStrings(previousOrder.filter((key) => allKeySet.has(key)))
      const keptSet = new Set(kept)
      const missing = allKeys.filter((key) => !keptSet.has(key))
      const replacementKey = pendingHomeReplacementKeyRef.current

      if (replacementKey && missing.length) {
        const replaceIndex = kept.indexOf(replacementKey)
        if (replaceIndex !== -1) {
          const next = [...kept]
          next.splice(replaceIndex, 1, missing[0])
          pendingHomeReplacementKeyRef.current = null
          const nextOrder = [...next, ...missing.slice(1)]
          return areStringArraysEqual(previousOrder, nextOrder) ? previousOrder : nextOrder
        }
      }

      const nextOrder = [...kept, ...missing]
      return areStringArraysEqual(previousOrder, nextOrder) ? previousOrder : nextOrder
    })
  }, [hasHydratedMainTabUiState, hasLoadedInitialSnapshot, isMainWorkspaceWindow, localTabs, visibleWorkspaceTabs])

  useEffect(() => {
    if (
      !isMainWorkspaceWindow ||
      !hasLoadedInitialSnapshot ||
      !hasHydratedMainTabUiState ||
      localTabs.length > 0 ||
      visibleWorkspaceTabs.length > 0
    ) {
      return
    }

    setLocalTabs([{ id: 'home-1', kind: 'home', title: t.untitledTab }])
    setActiveLocalTabId((current) => current ?? 'home-1')
    setTabOrder((previousOrder) =>
      previousOrder.includes('home:home-1') ? previousOrder : ['home:home-1', ...previousOrder]
    )
    setNextHomeTabNumber((current) => Math.max(current, 2))
  }, [
    hasHydratedMainTabUiState,
    hasLoadedInitialSnapshot,
    isMainWorkspaceWindow,
    localTabs.length,
    visibleWorkspaceTabs.length
  ])

  useEffect(() => {
    if (!isMainWorkspaceWindow || !hasLoadedInitialSnapshot || !hasHydratedMainTabUiState) {
      return
    }

    if (!hasSanitizedStoredPlaceholderRef.current) {
      hasSanitizedStoredPlaceholderRef.current = true
      const onlyPlaceholderHomeTab = localTabs.length === 1 && isDefaultPlaceholderHomeTab(localTabs[0]!)
      const hasRemoteSessions = visibleWorkspaceTabs.length > 0
      const isPlaceholderInactive = activeLocalTabId === null

      if (onlyPlaceholderHomeTab && hasRemoteSessions && isPlaceholderInactive) {
        setLocalTabs([])
        setTabOrder((previousOrder) => previousOrder.filter((key) => key !== 'home:home-1'))
        setNextHomeTabNumber(1)
        return
      }
    }

    const validSessionTabIds = new Set(visibleWorkspaceTabs.map((tab) => tab.id))
    const nextLocalTabs = localTabs.filter((tab) => tab.kind === 'home' || validSessionTabIds.has(tab.sessionTabId))
    if (nextLocalTabs.length !== localTabs.length) {
      setLocalTabs(nextLocalTabs)
    }
    setActiveLocalTabId((current) => {
      if (current && nextLocalTabs.some((tab) => tab.id === current)) {
        return current
      }
      if (visibleWorkspaceTabs.length > 0) {
        return null
      }
      return resolveFallbackHomeTabId(nextLocalTabs, tabOrder)
    })
  }, [
    activeLocalTabId,
    hasHydratedMainTabUiState,
    hasLoadedInitialSnapshot,
    isMainWorkspaceWindow,
    localTabs,
    tabOrder,
    visibleWorkspaceTabs
  ])

  useEffect(() => {
    if (!hasLoadedInitialSnapshot || !hasHydratedMainTabUiState) {
      return
    }

    if (!isMainWorkspaceWindow || !desktopApi?.setUiStateItem) {
      return
    }

    const uiStateApi = desktopApi
    void uiStateApi.setUiStateItem(
      MAIN_TAB_UI_STATE_KEY,
      JSON.stringify({
        localTabs,
        activeLocalTabId,
        nextHomeTabNumber,
        tabOrder,
        systemSidebarCollapsedByTabId
      } satisfies StoredMainTabUiState)
    )
  }, [
    activeLocalTabId,
    desktopApi,
    hasHydratedMainTabUiState,
    hasLoadedInitialSnapshot,
    isMainWorkspaceWindow,
    localTabs,
    nextHomeTabNumber,
    systemSidebarCollapsedByTabId,
    tabOrder
  ])

  useEffect(() => {
    setClosingSessionTabIds((current) => {
      const next = current.filter((tabId) => workspace.tabs.some((tab) => tab.id === tabId))
      return next.length === current.length ? current : next
    })
  }, [workspace.tabs])

  const activeLocalTab = activeLocalTabId ? (localTabs.find((tab) => tab.id === activeLocalTabId) ?? null) : null
  const visibleSessionTabOrder = uniqueStrings(tabOrder)
    .filter((key) => key.startsWith('session:'))
    .map((key) => key.slice('session:'.length))
    .filter((id) => visibleWorkspaceTabs.some((tab) => tab.id === id))
  const visibleActiveSessionTabId = activeLocalTab
    ? null
    : visibleWorkspaceTabs.some((tab) => tab.id === workspace.activeTabId)
      ? workspace.activeTabId
      : (visibleSessionTabOrder.at(-1) ?? visibleWorkspaceTabs.at(-1)?.id ?? null)
  const displayedSessionTabId = activeLocalTab
    ? activeLocalTab.kind === 'system'
      ? activeLocalTab.sessionTabId
      : null
    : visibleActiveSessionTabId
  const activeTab = displayedSessionTabId
    ? (visibleWorkspaceTabs.find((tab) => tab.id === displayedSessionTabId) ?? null)
    : null
  const activeSession = activeTab ? (workspace.sessions[activeTab.id] ?? null) : null
  const activePaneTabId = activeTab?.paneRoot
    ? (workspace.activePaneTabIdByRoot?.[activeTab.id] ?? activeTab.id)
    : activeTab?.id
  const activePaneTab = activePaneTabId ? (workspace.tabs.find((tab) => tab.id === activePaneTabId) ?? activeTab) : null
  const activePaneSession = activePaneTab ? (workspace.sessions[activePaneTab.id] ?? null) : null
  const isSystemSidebarCollapsed = activeTab ? (systemSidebarCollapsedByTabId[activeTab.id] ?? false) : false
  const setIsSystemSidebarCollapsed = (nextCollapsed: boolean) => {
    if (!activeTab) {
      return
    }

    const tabId = activeTab.id
    setSystemSidebarCollapsedByTabId((currentByTabId) => {
      const currentCollapsed = currentByTabId[tabId] ?? false
      if (currentCollapsed === nextCollapsed) {
        return currentByTabId
      }
      if (!nextCollapsed) {
        const nextByTabId = { ...currentByTabId }
        delete nextByTabId[tabId]
        return nextByTabId
      }
      return { ...currentByTabId, [tabId]: true }
    })
  }
  const workspaceStageKind: WorkspaceStageKind =
    activeLocalTab?.kind === 'system' ? 'system' : activeTab && activeSession && !activeLocalTab ? 'session' : 'home'
  const isHomeWorkspaceVisible = workspaceStageKind === 'home'
  const isActiveRemoteSessionConnected = Boolean(activeTab && activeSession?.connected)
  const showSidebar = activeTab !== null && activeSession !== null && !isHomeWorkspaceVisible
  const effectiveActiveLocalTabId =
    activeLocalTab?.id ?? (isHomeWorkspaceVisible ? resolveFallbackHomeTabId(localTabs, tabOrder) : null)
  const activeProfile = activeTab
    ? (workspace.profiles.find((profile) => profile.id === activeTab.profileId) ?? null)
    : null
  const activePaneProfile = activePaneTab
    ? (workspace.profiles.find((profile) => profile.id === activePaneTab.profileId) ?? null)
    : null
  const activeWorkspaceOrderKey = activeLocalTab
    ? homeTabKey(activeLocalTab.id)
    : activeTab
      ? sessionTabKey(activeTab.id)
      : 'empty'
  const previousWorkspaceOrderKeyRef = useRef(activeWorkspaceOrderKey)
  const workspaceNavDirectionRef = useRef<WorkspaceNavigationDirection>('down')

  const workspaceNavDirection = useMemo<WorkspaceNavigationDirection>(() => {
    const previousKey = previousWorkspaceOrderKeyRef.current
    if (previousKey === activeWorkspaceOrderKey) {
      return workspaceNavDirectionRef.current
    }
    const previousIndex = tabOrder.indexOf(previousKey)
    const nextIndex = tabOrder.indexOf(activeWorkspaceOrderKey)
    return previousIndex >= 0 && nextIndex >= 0 && nextIndex < previousIndex ? 'up' : 'down'
  }, [activeWorkspaceOrderKey, tabOrder])

  useEffect(() => {
    if (previousWorkspaceOrderKeyRef.current !== activeWorkspaceOrderKey) {
      previousWorkspaceOrderKeyRef.current = activeWorkspaceOrderKey
      workspaceNavDirectionRef.current = workspaceNavDirection
    }
  }, [activeWorkspaceOrderKey, workspaceNavDirection])

  const orderedTabs = useMemo<OrderedTabEntry[]>(
    () =>
      uniqueStrings(tabOrder)
        .map((key) => {
          if (key.startsWith('home:')) {
            const id = key.slice('home:'.length)
            const localTab = localTabs.find((tab) => tab.id === id)
            return localTab
              ? {
                  key,
                  kind: 'local' as const,
                  id: localTab.id,
                  title: localTab.title,
                  tabKind: localTab.kind
                }
              : null
          }

          const id = key.slice('session:'.length)
          const sessionTab = visibleWorkspaceTabs.find((tab) => tab.id === id)
          return sessionTab ? { key, kind: 'session' as const, tab: sessionTab } : null
        })
        .filter((item): item is OrderedTabEntry => item !== null),
    [localTabs, tabOrder, visibleWorkspaceTabs]
  )

  const sessionSendTargets = useMemo<SessionSendTarget[]>(
    () =>
      orderedTabs.flatMap((entry, index) => {
        if (entry.kind !== 'session' || entry.tab.sessionType !== 'ssh') {
          return []
        }

        const session = workspace.sessions[entry.tab.id]
        if (!session?.connected) {
          return []
        }

        return [
          {
            tabId: entry.tab.id,
            index: index + 1,
            title: entry.tab.title,
            label: `${index + 1} ${entry.tab.title}`,
            isCurrent: entry.tab.id === activeTab?.id
          }
        ]
      }),
    [activeTab?.id, orderedTabs, workspace.sessions]
  )

  const activeTerminalDockSendState = activePaneTab
    ? (terminalDockSendStateByTabId[activePaneTab.id] ?? createDefaultTerminalDockSendState())
    : createDefaultTerminalDockSendState()

  useEffect(() => {
    if (!hasLoadedInitialSnapshot) {
      return
    }

    const validTabIds = new Set(workspace.tabs.map((tab) => tab.id))
    setTerminalDockSendStateByTabId((current) => {
      const next = Object.fromEntries(Object.entries(current).filter(([tabId]) => validTabIds.has(tabId)))
      return Object.keys(next).length === Object.keys(current).length ? current : next
    })
    setSystemSidebarCollapsedByTabId((current) => {
      const next = Object.fromEntries(Object.entries(current).filter(([tabId]) => validTabIds.has(tabId)))
      return Object.keys(next).length === Object.keys(current).length ? current : next
    })
  }, [hasLoadedInitialSnapshot, visibleWorkspaceTabs, workspace.tabs])

  useEffect(() => {
    const availableTargetIds = new Set(sessionSendTargets.map((target) => target.tabId))
    setTerminalDockSendStateByTabId((current) => {
      let changed = false
      const next = Object.fromEntries(
        Object.entries(current).map(([tabId, state]) => {
          const selectedTabIds = state.selectedTabIds.filter((targetTabId) => availableTargetIds.has(targetTabId))
          if (selectedTabIds.length !== state.selectedTabIds.length) {
            changed = true
            return [tabId, { ...state, selectedTabIds }]
          }
          return [tabId, state]
        })
      )
      return changed ? next : current
    })
  }, [sessionSendTargets])

  const applySnapshot = (snapshot: WorkspaceSnapshot) => {
    setClosingSessionTabIds((current) => current.filter((tabId) => snapshot.tabs.some((tab) => tab.id === tabId)))
    onSnapshot(snapshot)
  }

  const updateTerminalDockSendState = (updater: (current: TerminalDockSendState) => TerminalDockSendState) => {
    if (!activePaneTab) {
      return
    }

    setTerminalDockSendStateByTabId((currentByTabId) => {
      const current = currentByTabId[activePaneTab.id] ?? createDefaultTerminalDockSendState()
      const next = updater(current)
      return {
        ...currentByTabId,
        [activePaneTab.id]: {
          ...next,
          selectedTabIds: next.selectedTabIds.filter((tabId) =>
            sessionSendTargets.some((target) => target.tabId === tabId)
          )
        }
      }
    })
  }

  const updateTerminalDockSendScope = (scope: SendScope, rememberSelection: boolean) => {
    updateTerminalDockSendState((current) => ({
      ...current,
      scope,
      rememberSelection,
      selectedTabIds: scope === 'selected-ssh' ? current.selectedTabIds : []
    }))
  }

  const updateTerminalDockSelectedTabIds = (selectedTabIds: string[], rememberSelection: boolean) => {
    updateTerminalDockSendState((current) => ({
      ...current,
      scope: 'selected-ssh',
      selectedTabIds,
      rememberSelection
    }))
  }

  const sendTerminalCommand = async (
    command: string,
    options?: CommandExecutionOptions,
    scope?: SendScope,
    selectedTabIds?: string[]
  ) => {
    if (!desktopApi || !activePaneTab) {
      return
    }

    const usesDockState = options === undefined && scope === undefined && selectedTabIds === undefined
    const effectiveScope = scope ?? activeTerminalDockSendState.scope
    const effectiveSelectedTabIds = selectedTabIds ?? activeTerminalDockSendState.selectedTabIds

    const targetIds = resolveSelectedTabIds(effectiveScope, activePaneTab, effectiveSelectedTabIds, sessionSendTargets)

    if (!targetIds.length) {
      onStatusMessage(t.commandNoAvailableTargets)
      return
    }

    try {
      const terminalCommand = command.replace(/\r\n|\r|\n/g, '\r')
      // 并行发送，对照 Electron 原版的 fire-and-forget 行为。
      // 顺序 await 会让一个卡住的 tab 阻塞后续所有 tab；后端已为每个
      // invoke 加了 send 超时，但并行化能进一步保证单个慢 tab 不影响
      // 其他 tab 的投递时序。allSettled 确保一个失败不影响其余。
      const payload = options?.appendCarriageReturn === false ? terminalCommand : `${terminalCommand}\r`
      const results = await Promise.allSettled(targetIds.map((tabId) => desktopApi.writeTerminal(tabId, payload)))
      const failure = settledResultsError('发送终端命令', results)
      if (failure) {
        throw failure
      }
    } catch (error) {
      onError('发送终端命令', error)
      throw error
    } finally {
      if (usesDockState && !activeTerminalDockSendState.rememberSelection && activePaneTab) {
        setTerminalDockSendStateByTabId((current) => ({
          ...current,
          [activePaneTab.id]: createDefaultTerminalDockSendState()
        }))
      }
    }
  }

  const openProfileInCurrentWorkspace = async (profileId: string) => {
    if (!desktopApi) {
      return
    }

    const activeHomeId = isHomeWorkspaceVisible ? effectiveActiveLocalTabId : null
    const replacementKey = activeHomeId ? homeTabKey(activeHomeId) : null
    pendingHomeReplacementKeyRef.current = replacementKey

    try {
      onBusyChange(true)
      const snapshot = await desktopApi.openProfile(profileId)
      applySnapshot(snapshot)
      onStatusMessage(null)
      if (activeHomeId && snapshot.activeTabId && replacementKey) {
        const nextSessionKey = sessionTabKey(snapshot.activeTabId)
        setTabOrder((current) => uniqueStrings(current.map((key) => (key === replacementKey ? nextSessionKey : key))))
        setLocalTabs((current) => current.filter((tab) => tab.id !== activeHomeId))
        pendingHomeReplacementKeyRef.current = null
      }
      setActiveLocalTabId(null)
    } catch (error) {
      pendingHomeReplacementKeyRef.current = null
      onError('打开连接', error)
    } finally {
      onBusyChange(false)
    }
  }

  const openProfile = async (profileId: string) => {
    if (isMainWorkspaceWindow && (!hasLoadedInitialSnapshot || !hasHydratedMainTabUiState)) {
      pendingProfileOpenIdRef.current = profileId
      onBusyChange(true)
      return
    }

    await openProfileInCurrentWorkspace(profileId)
  }

  useEffect(() => {
    if (!isMainWorkspaceWindow || !hasLoadedInitialSnapshot || !hasHydratedMainTabUiState) {
      return
    }

    const profileId = pendingProfileOpenIdRef.current
    if (!profileId) {
      return
    }

    pendingProfileOpenIdRef.current = null
    void openProfileInCurrentWorkspace(profileId)
  }, [hasHydratedMainTabUiState, hasLoadedInitialSnapshot, isMainWorkspaceWindow])

  const activateSessionTab = async (tabId: string) => {
    if (!desktopApi) {
      return
    }

    try {
      onBusyChange(true)
      const snapshot = await desktopApi.activateTab(tabId)
      applySnapshot(snapshot)
      setActiveLocalTabId(null)
    } catch (error) {
      onError('激活标签页', error)
    } finally {
      onBusyChange(false)
    }
  }

  const reconnectSessionTab = async (tabId: string) => {
    if (!desktopApi) {
      return
    }

    try {
      onBusyChange(true)
      const snapshot = await desktopApi.reconnectTab(tabId)
      applySnapshot(snapshot)
      setActiveLocalTabId(null)
    } catch (error) {
      onError('重新连接标签页', error)
    } finally {
      onBusyChange(false)
    }
  }

  const disconnectSessionTab = async (tabId: string) => {
    if (!desktopApi) {
      return
    }

    try {
      onBusyChange(true)
      const snapshot = await desktopApi.disconnectTab(tabId)
      applySnapshot(snapshot)
    } catch (error) {
      onError('断开标签页', error)
    } finally {
      onBusyChange(false)
    }
  }

  const closeHomeTabs = (
    homeTabIds: string[],
    preferredActiveHomeId: string | null,
    nextSessionTabs: WorkspaceTab[]
  ) => {
    let nextHomeTabs = localTabs.filter((tab) => !homeTabIds.includes(tab.id))
    let nextOrder = tabOrder.filter((key) => {
      if (key.startsWith('home:')) {
        return nextHomeTabs.some((tab) => homeTabKey(tab.id) === key)
      }
      return nextSessionTabs.some((tab) => sessionTabKey(tab.id) === key)
    })

    if (!nextHomeTabs.length && !nextSessionTabs.length) {
      nextHomeTabs = [{ id: 'home-1', kind: 'home', title: t.untitledTab }]
      preferredActiveHomeId = 'home-1'
      nextOrder = nextOrder.includes('home:home-1') ? nextOrder : ['home:home-1', ...nextOrder]
      setNextHomeTabNumber((current) => Math.max(current, 2))
    } else if (preferredActiveHomeId && !nextHomeTabs.some((tab) => tab.id === preferredActiveHomeId)) {
      preferredActiveHomeId = nextHomeTabs.at(-1)?.id ?? null
    }

    setLocalTabs(nextHomeTabs)
    setActiveLocalTabId(preferredActiveHomeId)
    setTabOrder(nextOrder)
  }

  const closeSessionTabById = async (tabId: string) => {
    if (!desktopApi) {
      return null
    }

    const nextVisibleSessionTabs = visibleWorkspaceTabs.filter((tab) => tab.id !== tabId)
    const relatedLocalTabs = localTabsRef.current
      .filter((tab) => tab.kind === 'system' && tab.sessionTabId === tabId)
      .map((tab) => tab.id)

    setClosingSessionTabIds((current) => (current.includes(tabId) ? current : [...current, tabId]))
    setTabOrder((current) => current.filter((key) => key !== sessionTabKey(tabId)))
    if (relatedLocalTabs.length) {
      closeHomeTabs(
        relatedLocalTabs,
        activeLocalTabId && relatedLocalTabs.includes(activeLocalTabId) ? null : activeLocalTabId,
        nextVisibleSessionTabs
      )
    } else if (!activeLocalTabId && workspace.activeTabId === tabId && nextVisibleSessionTabs.length === 0) {
      closeHomeTabs([], 'home-1', nextVisibleSessionTabs)
    }

    const snapshot = await desktopApi.closeTab(tabId)
    applySnapshot(snapshot)
    if (snapshot.activeTabId === null) {
      setLocalTabs((current) => (current.length ? current : [{ id: 'home-1', kind: 'home', title: t.untitledTab }]))
      setTabOrder((current) => {
        const filtered = current.filter((key) => key !== sessionTabKey(tabId))
        return filtered.some((key) => key.startsWith('home:')) ? filtered : ['home:home-1', ...filtered]
      })
      setActiveLocalTabId((current) => current ?? localTabsRef.current.at(-1)?.id ?? 'home-1')
    }
    return snapshot
  }

  // ==========================================
  // 分屏（Split Pane）操作
  // ==========================================

  /** 基于指定 pane 的 profile 新建独立 session，不共享 PTY。 */
  const splitPane = async (sourceTabId: string, direction: 'row' | 'column') => {
    if (!desktopApi) {
      return
    }
    // 只对 SSH session 分屏
    const sourceTab = workspace.tabs.find((tab) => tab.id === sourceTabId)
    if (!sourceTab || sourceTab.sessionType !== 'ssh') {
      return
    }
    try {
      onBusyChange(true)
      const snapshot = await desktopApi.splitTab(sourceTabId, direction)
      onSnapshot(snapshot)
    } catch (error) {
      onError('splitPane', error)
    } finally {
      onBusyChange(false)
    }
  }

  /**
   * 分屏当前活跃 pane：基于当前 profile 新建独立 session。
   * source tab id 优先取 activePaneTabIdByRoot[activeTabId]，退回到 activeTabId。
   */
  const splitCurrentPane = async (direction: 'row' | 'column') => {
    if (!workspace.activeTabId) {
      return
    }
    const rootTabId = workspace.activeTabId
    const activePaneTabId = workspace.activePaneTabIdByRoot?.[rootTabId] ?? rootTabId
    await splitPane(activePaneTabId, direction)
  }

  useEffect(() => {
    splitCurrentPaneRef.current = splitCurrentPane
  })

  const focusAdjacentPane = async (direction: PaneFocusDirection) => {
    if (!desktopApi || !workspace.activeTabId) {
      return
    }

    const rootTabId = workspace.activeTabId
    const rootTab = workspace.tabs.find((tab) => tab.id === rootTabId)
    const activePaneTabId = workspace.activePaneTabIdByRoot?.[rootTabId] ?? rootTabId
    if (!rootTab?.paneRoot) {
      return
    }

    const targetPaneTabId = findAdjacentPaneTabId(rootTab.paneRoot, activePaneTabId, direction)
    if (!targetPaneTabId) {
      return
    }

    try {
      const snapshot = await desktopApi.setActivePane(rootTabId, targetPaneTabId)
      onSnapshot(snapshot)
      window.dispatchEvent(new CustomEvent('fileterm:focus-terminal', { detail: targetPaneTabId }))
    } catch (error) {
      onError('focusAdjacentPane', error)
    }
  }

  useEffect(() => {
    focusAdjacentPaneRef.current = focusAdjacentPane
  })

  /** 关闭分屏中的单个 pane */
  const closePane = async (paneTabId: string) => {
    if (!desktopApi || !workspace.activeTabId) {
      return
    }
    const rootTabId = workspace.activeTabId
    const activePaneTabId = workspace.activePaneTabIdByRoot?.[rootTabId] ?? rootTabId
    try {
      const snapshot = await desktopApi.closePane(rootTabId, paneTabId)
      onSnapshot(snapshot)
      if (paneTabId === activePaneTabId) {
        const nextRootTabId = snapshot.activeTabId ?? rootTabId
        const nextPaneTabId = snapshot.activePaneTabIdByRoot?.[nextRootTabId] ?? nextRootTabId
        window.requestAnimationFrame(() => {
          window.dispatchEvent(new CustomEvent('fileterm:focus-terminal', { detail: nextPaneTabId }))
        })
      }
    } catch (error) {
      onError('closePane', error)
    }
  }

  /** 设置分屏活跃 pane */
  const activatePane = async (paneTabId: string) => {
    if (!desktopApi || !workspace.activeTabId) {
      return
    }
    const rootTabId = workspace.activeTabId
    try {
      const snapshot = await desktopApi.setActivePane(rootTabId, paneTabId)
      onSnapshot(snapshot)
    } catch (error) {
      onError('activatePane', error)
    }
  }

  /** 持久化分屏 weights */
  const setPaneWeights = async (panePath: number[], weights: number[]) => {
    if (!desktopApi || !workspace.activeTabId) {
      return
    }
    const rootTabId = workspace.activeTabId
    try {
      const snapshot = await desktopApi.setPaneWeights(rootTabId, panePath, weights)
      onSnapshot(snapshot)
    } catch (error) {
      onError('setPaneWeights', error)
    }
  }

  const closeHomeTabById = (homeTabId: string) => {
    setLocalTabs((current) => {
      const remaining = current.filter((tab) => tab.id !== homeTabId)

      if (remaining.length === 0 && visibleWorkspaceTabs.length === 0) {
        setActiveLocalTabId('home-1')
        setNextHomeTabNumber(2)
        setTabOrder((currentOrder) => {
          const filtered = currentOrder.filter((key) => key !== homeTabKey(homeTabId))
          return filtered.includes('home:home-1') ? filtered : ['home:home-1', ...filtered]
        })
        return [{ id: 'home-1', kind: 'home', title: t.untitledTab }]
      }

      if (activeLocalTabId === homeTabId) {
        setActiveLocalTabId(remaining.at(-1)?.id ?? null)
      }

      setTabOrder((currentOrder) => currentOrder.filter((key) => key !== homeTabKey(homeTabId)))
      return remaining
    })
  }

  const closeSessionTab = async (event: MouseEvent<HTMLButtonElement>, tabId: string) => {
    event.stopPropagation()
    if (!desktopApi) {
      return
    }

    const targetTab = visibleWorkspaceTabs.find((tab) => tab.id === tabId) ?? null
    if (isTabActivelyConnected(targetTab)) {
      setShortcutCloseConfirm({
        tabId,
        title: targetTab?.title ?? '',
        variant: targetTab?.status === 'connecting' ? 'connecting' : 'active-session'
      })
      return
    }

    try {
      await closeSessionTabById(tabId)
    } catch (error) {
      setClosingSessionTabIds((current) => current.filter((id) => id !== tabId))
      onError('关闭标签页', error)
    }
  }

  const activateHomeTab = (homeTabId: string) => {
    onStatusMessage(null)
    setActiveLocalTabId(homeTabId)
  }

  const addHomeTab = () => {
    const nextId = `home-${nextHomeTabNumber}`
    const nextKey = homeTabKey(nextId)

    setLocalTabs((current) => [...current, { id: nextId, kind: 'home', title: t.untitledTab }])
    setTabOrder((current) => [...current, nextKey])
    setNextHomeTabNumber((current) => current + 1)
    setActiveLocalTabId(nextId)
    onStatusMessage(null)
  }

  useEffect(() => {
    if (
      !isMainWorkspaceWindow ||
      newTabRequestVersion === 0 ||
      newTabRequestVersion === handledNewTabRequestVersionRef.current
    ) {
      return
    }

    handledNewTabRequestVersionRef.current = newTabRequestVersion
    addHomeTab()
  }, [addHomeTab, isMainWorkspaceWindow, newTabRequestVersion])

  const openSystemInfo = () => {
    if (!activeTab) {
      return
    }

    const existing = localTabs.find((tab) => tab.kind === 'system' && tab.sessionTabId === activeTab.id)
    if (existing) {
      setActiveLocalTabId(existing.id)
      onStatusMessage(null)
      return
    }

    const nextId = `system-${activeTab.id}`
    const activeOrderKey = activeLocalTabId ? homeTabKey(activeLocalTabId) : sessionTabKey(activeTab.id)
    setLocalTabs((current) => [
      ...current,
      {
        id: nextId,
        kind: 'system',
        title: formatSystemInfoTabTitle(activeTab.title),
        sessionTabId: activeTab.id,
        sourceTabTitle: activeTab.title
      }
    ])
    setTabOrder((current) => insertTabKeyAfter(current, homeTabKey(nextId), activeOrderKey))
    setActiveLocalTabId(nextId)
    onStatusMessage(null)
  }

  const closeHomeTab = (event: MouseEvent<HTMLButtonElement>, homeTabId: string) => {
    event.stopPropagation()
    closeHomeTabById(homeTabId)
  }

  const closeSessionTabs = async (tabIds: string[]) => {
    if (!desktopApi || !tabIds.length) {
      return
    }

    let lastSnapshot: WorkspaceSnapshot | null = null
    for (const tabId of tabIds) {
      lastSnapshot = await desktopApi.closeTab(tabId)
    }

    if (lastSnapshot) {
      applySnapshot(lastSnapshot)
    }
  }

  const closeActiveWorkspaceItem = async () => {
    if (!desktopApi || isBusy) {
      return
    }

    const currentActiveLocalTab = activeLocalTabId
      ? (localTabs.find((tab) => tab.id === activeLocalTabId) ?? null)
      : null
    const activeSessionTab =
      !currentActiveLocalTab && visibleActiveSessionTabId
        ? (visibleWorkspaceTabs.find((tab) => tab.id === visibleActiveSessionTabId) ?? null)
        : null
    const totalClosableItems = localTabs.length + visibleWorkspaceTabs.length

    if (currentActiveLocalTab) {
      if (totalClosableItems <= 1) {
        onCloseCurrentWindow()
        return
      }

      closeHomeTabById(currentActiveLocalTab.id)
      return
    }

    if (activeSessionTab) {
      const paneLeafIds = activeSessionTab.paneRoot ? collectPaneLeafTabIds(activeSessionTab.paneRoot) : []
      const activePaneTabId = workspace.activePaneTabIdByRoot?.[activeSessionTab.id] ?? activeSessionTab.id

      // Cmd+W follows the terminal hierarchy: close the focused pane without
      // interrupting the user with a dialog while its parent tab still has
      // other panes. Once only one surface remains, fall through to the
      // existing top-level tab confirmation flow below.
      if (paneLeafIds.length > 1 && paneLeafIds.includes(activePaneTabId)) {
        try {
          await closePane(activePaneTabId)
        } catch (error) {
          onError('关闭当前分屏', error)
        }
        return
      }

      const isLastSessionTab = visibleWorkspaceTabs.length === 1
      const needsDisconnectConfirm = isTabActivelyConnected(activeSessionTab)

      if (needsDisconnectConfirm) {
        setShortcutCloseConfirm({
          tabId: activeSessionTab.id,
          title: activeSessionTab.title,
          variant:
            activeSessionTab.status === 'connecting'
              ? 'connecting'
              : isLastSessionTab
                ? 'active-last-session'
                : 'active-session'
        })
        return
      }

      try {
        await closeSessionTabById(activeSessionTab.id)
      } catch (error) {
        setClosingSessionTabIds((current) => current.filter((id) => id !== activeSessionTab.id))
        onError('关闭当前标签页', error)
      }
      return
    }

    onRequestQuit()
  }

  useEffect(() => {
    if (
      !isMainWorkspaceWindow ||
      closeActiveRequestVersion === 0 ||
      closeActiveRequestVersion === handledCloseActiveRequestVersionRef.current
    ) {
      return
    }

    handledCloseActiveRequestVersionRef.current = closeActiveRequestVersion
    void closeActiveWorkspaceItem()
  }, [closeActiveRequestVersion, isMainWorkspaceWindow])

  useEffect(() => {
    if (!isMainWorkspaceWindow || !splitPaneRequest || splitPaneRequest.id === handledSplitPaneRequestIdRef.current) {
      return
    }

    handledSplitPaneRequestIdRef.current = splitPaneRequest.id
    void splitCurrentPaneRef.current(splitPaneRequest.direction)
  }, [splitPaneRequest, isMainWorkspaceWindow])

  useEffect(() => {
    if (!isMainWorkspaceWindow || !paneFocusRequest || paneFocusRequest.id === handledPaneFocusRequestIdRef.current) {
      return
    }

    handledPaneFocusRequestIdRef.current = paneFocusRequest.id
    void focusAdjacentPaneRef.current(paneFocusRequest.direction)
  }, [paneFocusRequest, isMainWorkspaceWindow])

  const dismissShortcutCloseConfirm = () => {
    setShortcutCloseConfirm(null)
  }

  const confirmShortcutClose = async () => {
    if (!shortcutCloseConfirm) {
      return
    }

    const { tabId } = shortcutCloseConfirm
    setShortcutCloseConfirm(null)

    try {
      await closeSessionTabById(tabId)
    } catch (error) {
      setClosingSessionTabIds((current) => current.filter((id) => id !== tabId))
      onError('关闭正在连接的标签页', error)
    }
  }

  const handleTabContextAction = async (action: WorkspaceTabContextAction) => {
    if (!tabContextMenu) {
      return
    }

    const target = tabContextMenu.target
    setTabContextMenu(null)

    if (action === 'copy') {
      copyText(target.title)
      return
    }

    if (action === 'clone') {
      if (target.kind !== 'session' || !desktopApi) {
        return
      }

      const sourceTab = visibleWorkspaceTabs.find((tab) => tab.id === target.id)
      if (!sourceTab) {
        return
      }

      try {
        onBusyChange(true)
        const snapshot = await desktopApi.openProfile(sourceTab.profileId)
        applySnapshot(snapshot)
        setActiveLocalTabId(null)
      } catch (error) {
        onError('克隆连接标签页', error)
      } finally {
        onBusyChange(false)
      }
      return
    }

    if (action === 'connect') {
      if (target.kind !== 'session') {
        return
      }
      await reconnectSessionTab(target.id)
      return
    }

    if (action === 'connectAll') {
      if (!desktopApi) {
        return
      }
      const reconnectableTabs = visibleWorkspaceTabs.filter(
        (tab) => tab.status !== 'connected' && tab.status !== 'connecting'
      )
      if (!reconnectableTabs.length) {
        return
      }
      try {
        onBusyChange(true)
        let lastSnapshot: WorkspaceSnapshot | null = null
        for (const tab of reconnectableTabs) {
          lastSnapshot = await desktopApi.reconnectTab(tab.id)
        }
        if (lastSnapshot) {
          applySnapshot(lastSnapshot)
          setActiveLocalTabId(null)
        }
      } catch (error) {
        onError('连接全部 SSH', error)
      } finally {
        onBusyChange(false)
      }
      return
    }

    if (action === 'disconnect') {
      if (target.kind !== 'session') {
        return
      }
      await disconnectSessionTab(target.id)
      return
    }

    const sessionTabsToClose =
      action === 'closeAll'
        ? visibleWorkspaceTabs.map((tab) => tab.id)
        : action === 'close'
          ? target.kind === 'session'
            ? [target.id]
            : []
          : target.kind === 'session'
            ? visibleWorkspaceTabs.filter((tab) => tab.id !== target.id).map((tab) => tab.id)
            : visibleWorkspaceTabs.map((tab) => tab.id)

    const homeTabsToClose =
      action === 'closeAll'
        ? localTabs.map((tab) => tab.id)
        : action === 'close'
          ? target.kind === 'local'
            ? [target.id]
            : []
          : target.kind === 'local'
            ? localTabs.filter((tab) => tab.id !== target.id).map((tab) => tab.id)
            : localTabs.map((tab) => tab.id)

    const remainingSessionTabs = visibleWorkspaceTabs.filter((tab) => !sessionTabsToClose.includes(tab.id))
    const preferredActiveHomeId = target.kind === 'local' && action !== 'close' ? target.id : null
    closeHomeTabs(homeTabsToClose, preferredActiveHomeId, remainingSessionTabs)

    if (!sessionTabsToClose.length) {
      return
    }

    try {
      onBusyChange(true)
      await closeSessionTabs(sessionTabsToClose)
      if (!remainingSessionTabs.length) {
        setActiveLocalTabId(
          (current) => current ?? preferredActiveHomeId ?? localTabsRef.current.at(-1)?.id ?? 'home-1'
        )
      }
    } catch (error) {
      onError('关闭标签组', error)
    } finally {
      onBusyChange(false)
    }
  }

  const openTabContextMenu = (event: MouseEvent<HTMLDivElement>, target: TabContextTarget) => {
    setTabContextMenu({ x: event.clientX, y: event.clientY, target })
  }

  const closeTabContextMenu = () => {
    setTabContextMenu(null)
  }

  const startTabDrag = (tabKey: string) => {
    setDraggingTabKey(tabKey)
  }

  const enterDraggedTab = (targetKey: string) => {
    setTabOrder((current) => reorderTabKeys(current, draggingTabKey, targetKey))
  }

  const endTabDrag = () => {
    setDraggingTabKey(null)
  }

  return {
    localTabs,
    activeLocalTabId,
    nextHomeTabNumber,
    tabOrder,
    hasHydratedMainTabUiState,
    terminalDockSendStateByTabId,
    activeTerminalDockSendState,
    draggingTabKey,
    tabContextMenu,
    shortcutCloseConfirm,
    closingSessionTabIds,
    isSystemSidebarCollapsed,
    setIsSystemSidebarCollapsed,
    visibleWorkspaceTabs,
    visibleActiveSessionTabId,
    displayedSessionTabId,
    activeLocalTab,
    activeTab,
    activeSession: activeSession as SessionSnapshot | null,
    activeProfile: activeProfile as ConnectionProfile | null,
    activePaneTab,
    activePaneSession: activePaneSession as SessionSnapshot | null,
    activePaneProfile: activePaneProfile as ConnectionProfile | null,
    workspaceStageKind,
    isHomeWorkspaceVisible,
    isActiveRemoteSessionConnected,
    showSidebar,
    effectiveActiveLocalTabId,
    activeWorkspaceOrderKey,
    workspaceNavDirection,
    orderedTabs,
    sessionSendTargets,
    openProfile,
    activateSessionTab,
    reconnectSessionTab,
    disconnectSessionTab,
    closeSessionTab,
    activateHomeTab,
    addHomeTab,
    openSystemInfo,
    closeHomeTab,
    closeActiveWorkspaceItem,
    dismissShortcutCloseConfirm,
    confirmShortcutClose,
    handleTabContextAction,
    openTabContextMenu,
    closeTabContextMenu,
    startTabDrag,
    enterDraggedTab,
    endTabDrag,
    updateTerminalDockSendState,
    updateTerminalDockSendScope,
    updateTerminalDockSelectedTabIds,
    sendTerminalCommand,
    splitPane,
    splitCurrentPane,
    closePane,
    activatePane,
    setPaneWeights
  }
}

export type UseWorkspaceTabsResult = ReturnType<typeof useWorkspaceTabs>
