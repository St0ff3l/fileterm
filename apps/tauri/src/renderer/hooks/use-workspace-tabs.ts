import { useEffect, useMemo, useRef, useState } from 'react'
import type { ConnectionProfile, PaneFocusDirection, SessionSnapshot } from '@fileterm/core'
import { homeTabKey, sessionTabKey } from '../app/app-utils'
import type { SessionSendTarget } from '../features/common/session-send-targets'
import { setLocale, t } from '../i18n'
import type { OrderedTabEntry } from '../features/layout/tab-bar'
import type {
  LocalTab,
  ShortcutCloseConfirm,
  StoredMainTabUiState,
  TerminalDockSendState,
  WorkspaceTabsActionContext,
  UseWorkspaceTabsOptions,
  WorkspaceStageKind,
  WorkspaceTabContextMenu,
  WorkspaceNavigationDirection
} from './workspace-tabs-types'
import { createWorkspaceTabsSessionActions } from './workspace-tabs-session-actions'
import { createWorkspaceTabsContextActions } from './workspace-tabs-context-actions'
import { createWorkspaceTabsPaneActions } from './workspace-tabs-pane-actions'
import {
  areStringArraysEqual,
  collectPaneLeafTabIds,
  createDefaultTerminalDockSendState,
  createInitialMainTabUiState,
  formatSystemInfoTabTitle,
  isDefaultPlaceholderHomeTab,
  parseStoredMainTabUiState,
  resolveFallbackHomeTabId,
  uniqueItemsById,
  uniqueStrings
} from './workspace-tabs-utils'

const MAIN_TAB_UI_STATE_KEY = 'main.tab-ui'

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
          (tab) =>
            !tab.isBackground && !closingSessionTabIdSet.has(tab.id) && !tab.paneRootTabId && !leafTabIds.has(tab.id)
        )
      ),
    [closingSessionTabIdSet, workspace.tabs, leafTabIds]
  )
  const backgroundWorkspaceTabs = useMemo(
    () =>
      uniqueItemsById(
        workspace.tabs.filter(
          (tab) =>
            tab.isBackground === true &&
            !closingSessionTabIdSet.has(tab.id) &&
            !tab.paneRootTabId &&
            !leafTabIds.has(tab.id)
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

      if (missing.length > 0) {
        const removedKeys = previousOrder.filter((key) => !allKeySet.has(key))
        if (removedKeys.length > 0) {
          let missingIdx = 0
          const replacedOrder: string[] = []
          for (const key of previousOrder) {
            if (allKeySet.has(key)) {
              replacedOrder.push(key)
            } else if (missingIdx < missing.length) {
              replacedOrder.push(missing[missingIdx])
              missingIdx++
            }
          }
          const remainingMissing = missing.slice(missingIdx)
          const nextOrder = uniqueStrings([...replacedOrder, ...remainingMissing])
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

  const sessionActionContext: WorkspaceTabsActionContext = {
    desktopApi,
    workspace,
    isMainWorkspaceWindow,
    hasLoadedInitialSnapshot,
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
    onRequestQuit,
    localTabs,
    setLocalTabs,
    activeLocalTabId,
    setActiveLocalTabId,
    nextHomeTabNumber,
    setNextHomeTabNumber,
    tabOrder,
    setTabOrder,
    visibleWorkspaceTabs,
    visibleActiveSessionTabId,
    activeLocalTab,
    activeTab,
    activePaneTab,
    isHomeWorkspaceVisible,
    activeLocalTabIdForUi: effectiveActiveLocalTabId,
    hasHydratedMainTabUiState,
    activeTerminalDockSendState,
    sessionSendTargets,
    setTerminalDockSendStateByTabId,
    setClosingSessionTabIds,
    shortcutCloseConfirm,
    setShortcutCloseConfirm,
    tabContextMenu,
    setTabContextMenu,
    draggingTabKey,
    setDraggingTabKey,
    localTabsRef,
    pendingHomeReplacementKeyRef,
    pendingProfileOpenIdRef
  }
  const {
    activateSessionTab,
    applySnapshot,
    attachBackgroundSession,
    closeBackgroundSession,
    closeHomeTabs,
    closeSessionTabById,
    consumePendingProfileOpen,
    detachSessionToBackground,
    disconnectSessionTab,
    openLocalTerminal,
    openProfile,
    reconnectSessionTab,
    sendTerminalCommand,
    updateTerminalDockSendState,
    updateTerminalDockSelectedTabIds,
    updateTerminalDockSendScope
  } = createWorkspaceTabsSessionActions(sessionActionContext)

  useEffect(() => {
    consumePendingProfileOpen()
  }, [hasHydratedMainTabUiState, hasLoadedInitialSnapshot, isMainWorkspaceWindow])

  // ==========================================
  // 分屏（Split Pane）操作
  // ==========================================

  /** 基于指定 pane 新建独立 SSH session 或本地 PTY，不共享运行时。 */
  const paneActions = createWorkspaceTabsPaneActions(sessionActionContext)
  const { activatePane, closePane, focusAdjacentPane, setPaneWeights, splitCurrentPane, splitPane } = paneActions
  const contextActions = createWorkspaceTabsContextActions({
    ...sessionActionContext,
    ...paneActions,
    closeHomeTabs,
    closeSessionTabById,
    applySnapshot,
    openLocalTerminal,
    openProfile,
    reconnectSessionTab,
    disconnectSessionTab
  })
  const {
    activateHomeTab,
    addHomeTab,
    closeActiveWorkspaceItem,
    closeHomeTab,
    closeSessionTab,
    confirmShortcutClose,
    dismissShortcutCloseConfirm,
    handleTabContextAction,
    openSystemInfo,
    openTabContextMenu,
    closeTabContextMenu,
    startTabDrag,
    enterDraggedTab,
    endTabDrag
  } = contextActions

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

  useEffect(() => {
    splitCurrentPaneRef.current = splitCurrentPane
  })

  useEffect(() => {
    focusAdjacentPaneRef.current = focusAdjacentPane
  })

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
    backgroundWorkspaceTabs,
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
    openLocalTerminal,
    activateSessionTab,
    attachBackgroundSession,
    detachSessionToBackground,
    closeBackgroundSession,
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
