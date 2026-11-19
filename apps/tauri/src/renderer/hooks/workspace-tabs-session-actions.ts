import type {
  CommandExecutionOptions,
  LocalTerminalLaunchOptions,
  WorkspaceSnapshot,
  WorkspaceTab
} from '@fileterm/core'
import { homeTabKey, sessionTabKey, settledResultsError, tabStatusClass } from '../app/app-utils'
import { resolveSelectedTabIds, type SendScope } from '../features/common/session-send-targets'
import { t } from '../i18n'
import { runProfileOpenOnce } from './profile-open-guard'
import type { WorkspaceTabsActionContext } from './workspace-tabs-types'
import { nextTabAfterClose, nextTabInHostGroup, uniqueStrings } from './workspace-tabs-utils'

export function createWorkspaceTabsSessionActions(context: WorkspaceTabsActionContext) {
  const {
    desktopApi,
    workspace,
    isMainWorkspaceWindow,
    hasLoadedInitialSnapshot,
    onSnapshot,
    onBusyChange,
    onStatusMessage,
    onError,
    localTabs,
    setLocalTabs,
    activeLocalTabId,
    setActiveLocalTabId,
    setPendingActiveTabId,
    tabActivationRequestRef,
    nextHomeTabNumber,
    setNextHomeTabNumber,
    tabOrder,
    setTabOrder,
    visibleWorkspaceTabs,
    activePaneTab,
    activeTerminalDockSendState,
    sessionSendTargets,
    setTerminalDockSendStateByTabId,
    setClosingSessionTabIds,
    pendingHomeReplacementKeyRef,
    pendingProfileOpenIdRef,
    localTabsRef
  } = context

  const applySnapshot = (snapshot: WorkspaceSnapshot) => {
    setClosingSessionTabIds((current) => current.filter((tabId) => snapshot.tabs.some((tab) => tab.id === tabId)))
    onSnapshot(snapshot)
  }

  const updateTerminalDockSendState = (
    updater: (current: typeof activeTerminalDockSendState) => typeof activeTerminalDockSendState
  ) => {
    if (!activePaneTab) {
      return
    }

    setTerminalDockSendStateByTabId((currentByTabId) => {
      const current = currentByTabId[activePaneTab.id] ?? activeTerminalDockSendState
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
          [activePaneTab.id]: {
            scope: 'current',
            selectedTabIds: [],
            rememberSelection: false
          }
        }))
      }
    }
  }

  const openProfileInCurrentWorkspace = (profileId: string, keepHomeVisible = false) =>
    runProfileOpenOnce(profileId, async () => {
      if (!desktopApi) {
        return
      }

      const activeHomeId =
        context.activeLocalTab?.kind === 'home'
          ? context.activeLocalTab.id
          : context.isHomeWorkspaceVisible
            ? (context.activeLocalTabIdForUi ??
              [...context.localTabs].reverse().find((tab) => tab.kind === 'home')?.id ??
              null)
            : null
      const replacementKey = activeHomeId && !keepHomeVisible ? homeTabKey(activeHomeId) : null
      pendingHomeReplacementKeyRef.current = replacementKey

      try {
        onBusyChange(true)
        const disconnectedTab = keepHomeVisible
          ? visibleWorkspaceTabs.find(
              (tab) => tab.profileId === profileId && tabStatusClass(tab.status) === 'disconnected'
            )
          : undefined
        const snapshot = disconnectedTab
          ? await desktopApi.reconnectTab(disconnectedTab.id)
          : await desktopApi.openProfile(profileId)
        applySnapshot(snapshot)
        onStatusMessage(null)
        if (activeHomeId && snapshot.activeTabId && replacementKey) {
          const nextSessionKey = sessionTabKey(snapshot.activeTabId)
          setTabOrder((current) => {
            const replacementIndex = current.indexOf(replacementKey)
            const next = current.filter((key) => key !== replacementKey)
            if (next.includes(nextSessionKey)) return next
            if (replacementIndex < 0) return [nextSessionKey, ...next]
            return [...next.slice(0, replacementIndex), nextSessionKey, ...next.slice(replacementIndex)]
          })
          setLocalTabs((current) => current.filter((tab) => tab.id !== activeHomeId))
        }
        pendingHomeReplacementKeyRef.current = null
        if (keepHomeVisible) {
          if (activeHomeId) setActiveLocalTabId(activeHomeId)
        } else {
          setActiveLocalTabId(null)
        }
      } catch (error) {
        pendingHomeReplacementKeyRef.current = null
        onError('打开连接', error)
      } finally {
        if (replacementKey) setTabOrder((current) => [...current])
        onBusyChange(false)
      }
    })

  const openProfile = async (profileId: string) => {
    if (isMainWorkspaceWindow && (!hasLoadedInitialSnapshot || !context.hasHydratedMainTabUiState)) {
      pendingProfileOpenIdRef.current = profileId
      onBusyChange(true)
      return
    }

    await openProfileInCurrentWorkspace(profileId)
  }

  /**
   * Dashboard connect keeps the overview in front: the session runs in the
   * background so monitoring can fill in the server configuration, and only
   * the terminal action attaches it to a visible tab.
   */
  const connectProfile = (profileId: string) =>
    context.serverManagementMode
      ? runProfileOpenOnce(profileId, async () => {
          if (!desktopApi) {
            return
          }

          const disconnectedTab = visibleWorkspaceTabs.find(
            (tab) => tab.profileId === profileId && tabStatusClass(tab.status) === 'disconnected'
          )
          try {
            onBusyChange(true)
            const snapshot = disconnectedTab
              ? await desktopApi.reconnectTab(disconnectedTab.id)
              : await desktopApi.openProfileBackground(profileId)
            applySnapshot(snapshot)
            onStatusMessage(null)
          } catch (error) {
            onError('连接服务器', error)
          } finally {
            onBusyChange(false)
          }
        })
      : openProfileInCurrentWorkspace(profileId, true)

  const openLocalTerminal = async (options?: LocalTerminalLaunchOptions, startupCommand?: string) => {
    if (!desktopApi) {
      return
    }

    const activeHomeId = context.isHomeWorkspaceVisible ? context.activeLocalTabIdForUi : null
    const replacementKey = activeHomeId ? homeTabKey(activeHomeId) : null
    pendingHomeReplacementKeyRef.current = replacementKey

    try {
      onBusyChange(true)
      const snapshot = await desktopApi.openLocalTerminal(options)
      applySnapshot(snapshot)
      onStatusMessage(null)
      if (activeHomeId && snapshot.activeTabId && replacementKey) {
        const nextSessionKey = sessionTabKey(snapshot.activeTabId)
        setTabOrder((current) => uniqueStrings(current.map((key) => (key === replacementKey ? nextSessionKey : key))))
        setLocalTabs((current) => current.filter((tab) => tab.id !== activeHomeId))
      }
      pendingHomeReplacementKeyRef.current = null
      setActiveLocalTabId(null)
      if (startupCommand && snapshot.activeTabId) {
        try {
          await desktopApi.writeTerminal(snapshot.activeTabId, `${startupCommand}\r`)
        } catch (error) {
          onError('启动本地 Agent', error)
        }
      }
    } catch (error) {
      pendingHomeReplacementKeyRef.current = null
      onError('打开本地终端', error)
    } finally {
      if (replacementKey) setTabOrder((current) => [...current])
      onBusyChange(false)
    }
  }

  const activateSessionTab = async (tabId: string) => {
    if (!desktopApi) {
      return
    }

    // Background sessions have no tab yet; attaching is what opens the terminal.
    const targetTab = workspace.tabs.find((tab) => tab.id === tabId)
    if (targetTab?.isBackground) {
      const requestId = ++tabActivationRequestRef.current
      setPendingActiveTabId(tabId)
      setActiveLocalTabId(null)
      try {
        onBusyChange(true)
        const snapshot = await desktopApi.attachBackgroundSession(tabId)
        applySnapshot(snapshot)
      } catch (error) {
        if (tabActivationRequestRef.current === requestId) setActiveLocalTabId(activeLocalTabId)
        onError('打开终端', error)
      } finally {
        if (tabActivationRequestRef.current === requestId) setPendingActiveTabId(null)
        onBusyChange(false)
      }
      return
    }

    // Switch before the snapshot round trip, which carries every session's metrics history.
    const topLevelTabId = workspace.tabs.find((tab) => tab.id === tabId)?.paneRootTabId ?? tabId
    const requestId = ++tabActivationRequestRef.current
    setPendingActiveTabId(topLevelTabId)
    setActiveLocalTabId(null)
    try {
      onBusyChange(true)
      const snapshot = await desktopApi.activateTab(tabId)
      applySnapshot(snapshot)
    } catch (error) {
      if (tabActivationRequestRef.current === requestId) setActiveLocalTabId(activeLocalTabId)
      onError('激活标签页', error)
    } finally {
      if (tabActivationRequestRef.current === requestId) setPendingActiveTabId(null)
      onBusyChange(false)
    }
  }

  const attachBackgroundSession = async (tabId: string) => {
    if (!desktopApi) {
      return
    }

    const activeHomeId = context.isHomeWorkspaceVisible ? context.activeLocalTabIdForUi : null
    const replacementKey = activeHomeId ? homeTabKey(activeHomeId) : null

    try {
      onBusyChange(true)
      const snapshot = await desktopApi.attachBackgroundSession(tabId)
      applySnapshot(snapshot)
      onStatusMessage(null)
      if (activeHomeId && snapshot.activeTabId && replacementKey) {
        const nextSessionKey = sessionTabKey(snapshot.activeTabId)
        setTabOrder((current) => uniqueStrings(current.map((key) => (key === replacementKey ? nextSessionKey : key))))
        setLocalTabs((current) => current.filter((tab) => tab.id !== activeHomeId))
      }
      setActiveLocalTabId(null)
    } catch (error) {
      onError('打开后台会话', error)
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

  const detachSessionToBackground = async (tabId: string) => {
    if (!desktopApi) {
      return
    }

    const targetTab = visibleWorkspaceTabs.find((tab) => tab.id === tabId)
    if (!targetTab?.source) {
      return
    }

    const relatedLocalTabs = localTabsRef.current
      .filter((tab) => tab.kind === 'system' && tab.sessionTabId === tabId)
      .map((tab) => tab.id)

    try {
      onBusyChange(true)
      const snapshot = await desktopApi.detachSessionToBackground(tabId)
      applySnapshot(snapshot)
      const remainingLocalTabs = localTabsRef.current.filter((tab) => !relatedLocalTabs.includes(tab.id))
      const existingHome = remainingLocalTabs.find((tab) => tab.kind === 'home')
      const homeId = existingHome?.id ?? `home-${nextHomeTabNumber}`
      const request = (existingHome?.backgroundSessionsRequest ?? 0) + 1
      const home = {
        id: homeId,
        kind: 'home' as const,
        title: existingHome?.title ?? t.untitledTab,
        backgroundSessionsRequest: request
      }
      setLocalTabs(
        existingHome ? remainingLocalTabs.map((tab) => (tab.id === homeId ? home : tab)) : [...remainingLocalTabs, home]
      )
      if (!existingHome) setNextHomeTabNumber((current) => current + 1)
      setTabOrder((current) => {
        const next = current.filter(
          (key) => key !== sessionTabKey(tabId) && !relatedLocalTabs.some((id) => homeTabKey(id) === key)
        )
        return next.includes(homeTabKey(homeId)) ? next : [...next, homeTabKey(homeId)]
      })
      tabActivationRequestRef.current += 1
      setPendingActiveTabId(null)
      onStatusMessage(null)
      setActiveLocalTabId(homeId)
    } catch (error) {
      onError('隐藏后台会话', error)
    } finally {
      onBusyChange(false)
    }
  }

  const closeBackgroundSession = async (tabId: string) => {
    if (!desktopApi) {
      return
    }

    try {
      onBusyChange(true)
      const snapshot = await desktopApi.closeTab(tabId)
      applySnapshot(snapshot)
    } catch (error) {
      onError('关闭后台会话', error)
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
      // Dashboard connections only exist in the background, so dropping the
      // connection also drops the hidden session instead of leaving a stale entry.
      const isBackground = workspace.tabs.find((tab) => tab.id === tabId)?.isBackground === true
      const snapshot = isBackground ? await desktopApi.closeTab(tabId) : await desktopApi.disconnectTab(tabId)
      applySnapshot(snapshot)
    } catch (error) {
      onError('断开标签页', error)
    } finally {
      onBusyChange(false)
    }
  }

  /**
   * Server-management mode returns to the overview once the active host group
   * runs out of tabs. The overview is the home tab, which stays hidden in the
   * grouped tab bar, so make sure one exists before activating it.
   */
  const activateManagementHome = () => {
    const homeTab = localTabsRef.current.find((tab) => tab.kind === 'home')
    if (homeTab) {
      setActiveLocalTabId(homeTab.id)
      return
    }

    const homeId = `home-${nextHomeTabNumber}`
    setLocalTabs((current) =>
      current.some((tab) => tab.kind === 'home')
        ? current
        : [...current, { id: homeId, kind: 'home', title: t.untitledTab }]
    )
    setTabOrder((current) => (current.includes(homeTabKey(homeId)) ? current : [...current, homeTabKey(homeId)]))
    setNextHomeTabNumber((current) => current + 1)
    setActiveLocalTabId((current) => current ?? homeId)
  }

  const closeSessionTabById = async (tabId: string) => {
    if (!desktopApi) {
      return null
    }

    const nextVisibleSessionTabs = visibleWorkspaceTabs.filter((tab) => tab.id !== tabId)
    const relatedLocalTabs = localTabsRef.current
      .filter((tab) => tab.kind === 'system' && tab.sessionTabId === tabId)
      .map((tab) => tab.id)

    const closingActive =
      (!activeLocalTabId && context.visibleActiveSessionTabId === tabId) ||
      relatedLocalTabs.includes(activeLocalTabId ?? '')
    const closedKey = activeLocalTabId ? homeTabKey(activeLocalTabId) : sessionTabKey(tabId)
    const removedKeys = [sessionTabKey(tabId), ...relatedLocalTabs.map(homeTabKey)]
    // Grouped closing stays inside the current host group; no neighbour means
    // the group is now empty and the overview takes over.
    const next = closingActive
      ? context.serverManagementMode
        ? nextTabInHostGroup(context.orderedTabs, closedKey, removedKeys)
        : nextTabAfterClose(context.orderedTabs, closedKey, false, removedKeys)
      : null
    // Select the neighbour before closing so the backend cannot choose its last tab.
    if (next?.kind === 'session') applySnapshot(await desktopApi.activateTab(next.tab.id))

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

    if (next) setActiveLocalTabId(next.kind === 'local' ? next.id : null)
    if (context.serverManagementMode && closingActive && !next) activateManagementHome()

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

  // Preserve the existing hook's pending-open behavior without making the
  // facade know how the profile action is implemented.
  const consumePendingProfileOpen = () => {
    if (!isMainWorkspaceWindow || !hasLoadedInitialSnapshot || !context.hasHydratedMainTabUiState) {
      return
    }

    const profileId = pendingProfileOpenIdRef.current
    if (!profileId) {
      return
    }

    pendingProfileOpenIdRef.current = null
    void openProfileInCurrentWorkspace(profileId)
  }

  return {
    activateManagementHome,
    activateSessionTab,
    applySnapshot,
    attachBackgroundSession,
    closeBackgroundSession,
    closeHomeTabs,
    closeSessionTabById,
    connectProfile,
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
  }
}
