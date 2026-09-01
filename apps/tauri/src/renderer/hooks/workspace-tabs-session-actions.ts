import type {
  CommandExecutionOptions,
  LocalTerminalLaunchOptions,
  WorkspaceSnapshot,
  WorkspaceTab
} from '@fileterm/core'
import { homeTabKey, sessionTabKey, settledResultsError } from '../app/app-utils'
import { resolveSelectedTabIds, type SendScope } from '../features/common/session-send-targets'
import { t } from '../i18n'
import type { WorkspaceTabsActionContext } from './workspace-tabs-types'
import { uniqueStrings } from './workspace-tabs-utils'

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

  const openProfileInCurrentWorkspace = async (profileId: string) => {
    if (!desktopApi) {
      return
    }

    const activeHomeId = context.isHomeWorkspaceVisible ? context.activeLocalTabIdForUi : null
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
    if (isMainWorkspaceWindow && (!hasLoadedInitialSnapshot || !context.hasHydratedMainTabUiState)) {
      pendingProfileOpenIdRef.current = profileId
      onBusyChange(true)
      return
    }

    await openProfileInCurrentWorkspace(profileId)
  }

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
      onBusyChange(false)
    }
  }

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

    const nextVisibleSessionTabs = visibleWorkspaceTabs.filter((tab) => tab.id !== tabId)
    const relatedLocalTabs = localTabsRef.current
      .filter((tab) => tab.kind === 'system' && tab.sessionTabId === tabId)
      .map((tab) => tab.id)

    try {
      onBusyChange(true)
      const snapshot = await desktopApi.detachSessionToBackground(tabId)
      applySnapshot(snapshot)
      if (relatedLocalTabs.length) {
        closeHomeTabs(
          relatedLocalTabs,
          activeLocalTabId && relatedLocalTabs.includes(activeLocalTabId) ? null : activeLocalTabId,
          nextVisibleSessionTabs
        )
      } else {
        setTabOrder((current) => current.filter((key) => key !== sessionTabKey(tabId)))
      }
      setActiveLocalTabId(null)
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
      const snapshot = await desktopApi.disconnectTab(tabId)
      applySnapshot(snapshot)
    } catch (error) {
      onError('断开标签页', error)
    } finally {
      onBusyChange(false)
    }
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
  }
}
