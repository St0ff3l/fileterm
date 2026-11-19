import type { MouseEvent } from 'react'
import type { LocalTerminalLaunchOptions, WorkspaceSnapshot, WorkspaceTab } from '@fileterm/core'
import { copyText, homeTabKey, insertTabKeyAfter, reorderTabKeys, sessionTabKey } from '../app/app-utils'
import { t } from '../i18n'
import type { TabContextTarget } from '../features/layout/tab-bar'
import { markHomeTabTransition } from '../features/workspace/home-transition'
import type { WorkspaceTabContextAction, WorkspaceTabsActionContext } from './workspace-tabs-types'
import {
  formatSessionTabTitle,
  formatSystemInfoTabTitle,
  isTabActivelyConnected,
  nextTabAfterClose
} from './workspace-tabs-utils'

type WorkspaceTabsContextActionContext = WorkspaceTabsActionContext & {
  activateSessionTab(tabId: string): Promise<void>
  closeHomeTabs(homeTabIds: string[], preferredActiveHomeId: string | null, nextSessionTabs: WorkspaceTab[]): void
  closeSessionTabById(tabId: string): Promise<WorkspaceSnapshot | null>
  closePane(paneTabId: string): Promise<void>
  applySnapshot(snapshot: WorkspaceSnapshot): void
  openLocalTerminal(options?: LocalTerminalLaunchOptions, startupCommand?: string): Promise<void>
  openProfile(profileId: string): Promise<void>
  reconnectSessionTab(tabId: string): Promise<void>
  disconnectSessionTab(tabId: string): Promise<void>
}

export function createWorkspaceTabsContextActions(context: WorkspaceTabsContextActionContext) {
  const {
    desktopApi,
    workspace,
    isBusy,
    onBusyChange,
    onStatusMessage,
    onError,
    onCloseCurrentWindow,
    onRequestQuit,
    localTabs,
    setLocalTabs,
    activeLocalTabId,
    setActiveLocalTabId,
    setPendingActiveTabId,
    tabActivationRequestRef,
    nextHomeTabNumber,
    setNextHomeTabNumber,
    setTabOrder,
    visibleWorkspaceTabs,
    visibleActiveSessionTabId,
    activeTab,
    setClosingSessionTabIds,
    shortcutCloseConfirm: _shortcutCloseConfirm,
    setShortcutCloseConfirm,
    tabContextMenu,
    setTabContextMenu,
    draggingTabKey,
    setDraggingTabKey,
    localTabsRef,
    closeHomeTabs,
    activateSessionTab,
    closeSessionTabById,
    closePane,
    applySnapshot,
    reconnectSessionTab,
    disconnectSessionTab
  } = context

  const closeHomeTabById = (homeTabId: string) => {
    const next =
      activeLocalTabId === homeTabId ? nextTabAfterClose(context.orderedTabs, homeTabKey(homeTabId), false) : null
    closeHomeTabs(
      [homeTabId],
      activeLocalTabId === homeTabId ? (next?.kind === 'local' ? next.id : null) : activeLocalTabId,
      visibleWorkspaceTabs
    )
    if (next?.kind === 'session') void activateSessionTab(next.tab.id)
  }

  const closeSessionTab = async (event: MouseEvent<HTMLElement>, tabId: string) => {
    event.stopPropagation()
    if (!desktopApi) {
      return
    }

    const targetTab = visibleWorkspaceTabs.find((tab) => tab.id === tabId) ?? null
    if (isTabActivelyConnected(targetTab)) {
      setShortcutCloseConfirm({
        tabId,
        title: targetTab ? formatSessionTabTitle(targetTab) : '',
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
    tabActivationRequestRef.current += 1
    setPendingActiveTabId(null)
    onStatusMessage(null)
    setActiveLocalTabId(homeTabId)
  }

  const addHomeTab = () => {
    const nextId = `home-${nextHomeTabNumber}`
    const nextKey = homeTabKey(nextId)

    markHomeTabTransition(nextKey)
    setLocalTabs((current) => [...current, { id: nextId, kind: 'home', title: t.untitledTab }])
    setTabOrder((current) => [...current, nextKey])
    setNextHomeTabNumber((current) => current + 1)
    activateHomeTab(nextId)
  }

  const openFileTransfers = () => {
    const id = 'file-transfers'
    if (!localTabs.some((tab) => tab.kind === 'transfers')) {
      const activeOrderKey = activeLocalTabId
        ? homeTabKey(activeLocalTabId)
        : activeTab
          ? sessionTabKey(activeTab.id)
          : null
      setLocalTabs((current) => [
        ...current,
        {
          id,
          kind: 'transfers',
          title: t.fileTransferWorkspace,
          initialTabId: activeTab?.id,
          initialPath: workspace.sessions[activeTab?.id ?? '']?.remotePath
        }
      ])
      setTabOrder((current) => insertTabKeyAfter(current, homeTabKey(id), activeOrderKey))
    }
    activateHomeTab(id)
  }

  const openHostInfo = (view?: 'network') => {
    if (!activeTab) {
      return
    }

    const existing = localTabs.find(
      (tab) => tab.kind === 'system' && tab.sessionTabId === activeTab.id && tab.view === view
    )
    if (existing) {
      activateHomeTab(existing.id)
      return
    }

    const nextId = `${view ?? 'system'}-${activeTab.id}`
    const activeOrderKey = activeLocalTabId ? homeTabKey(activeLocalTabId) : sessionTabKey(activeTab.id)
    setLocalTabs((current) => [
      ...current,
      {
        id: nextId,
        kind: 'system',
        view,
        title: formatSystemInfoTabTitle(activeTab.title, view),
        sessionTabId: activeTab.id,
        sourceTabTitle: activeTab.title
      }
    ])
    setTabOrder((current) => insertTabKeyAfter(current, homeTabKey(nextId), activeOrderKey))
    activateHomeTab(nextId)
  }

  const openSystemInfo = () => openHostInfo()
  const openNetworkConnections = () => openHostInfo('network')

  const closeHomeTab = (event: MouseEvent<HTMLElement>, homeTabId: string) => {
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
      if (totalClosableItems <= 1 && currentActiveLocalTab.kind !== 'transfers') {
        onCloseCurrentWindow()
        return
      }

      closeHomeTabById(currentActiveLocalTab.id)
      return
    }

    if (activeSessionTab) {
      const paneLeafIds = activeSessionTab.paneRoot ? getPaneLeafIds(activeSessionTab.paneRoot) : []
      const activePaneTabId = workspace.activePaneTabIdByRoot?.[activeSessionTab.id] ?? activeSessionTab.id

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
          title: formatSessionTabTitle(activeSessionTab),
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

  const dismissShortcutCloseConfirm = () => {
    setShortcutCloseConfirm(null)
  }

  const confirmShortcutClose = async () => {
    if (!_shortcutCloseConfirm) {
      return
    }

    const { tabId } = _shortcutCloseConfirm
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
        const snapshot =
          sourceTab.sessionType === 'local'
            ? await desktopApi.openLocalTerminal()
            : await desktopApi.openProfile(sourceTab.profileId)
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
        (tab) => tab.sessionType === 'ssh' && tab.status !== 'connected' && tab.status !== 'connecting'
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

    if (action === 'saveSessionLog') {
      if (target.kind !== 'session' || !desktopApi) {
        return
      }

      try {
        const savedPath = await desktopApi.saveSessionLog(target.id)
        if (savedPath) {
          onStatusMessage(`${t.sessionLogSaved}: ${savedPath}`)
        }
      } catch (error) {
        onError(t.sessionLogSaveFailed, error)
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

    if (action === 'close') {
      if (target.kind === 'local') closeHomeTabById(target.id)
      else {
        try {
          onBusyChange(true)
          await closeSessionTabById(target.id)
        } catch (error) {
          setClosingSessionTabIds((current) => current.filter((id) => id !== target.id))
          onError('关闭标签页', error)
        } finally {
          onBusyChange(false)
        }
      }
      return
    }

    const sessionTabsToClose =
      action === 'closeAll'
        ? visibleWorkspaceTabs.map((tab) => tab.id)
        : target.kind === 'session'
          ? visibleWorkspaceTabs.filter((tab) => tab.id !== target.id).map((tab) => tab.id)
          : visibleWorkspaceTabs.map((tab) => tab.id)

    const homeTabsToClose =
      action === 'closeAll'
        ? localTabs.map((tab) => tab.id)
        : target.kind === 'local'
          ? localTabs.filter((tab) => tab.id !== target.id).map((tab) => tab.id)
          : localTabs.map((tab) => tab.id)

    const remainingSessionTabs = visibleWorkspaceTabs.filter((tab) => !sessionTabsToClose.includes(tab.id))
    const preferredActiveHomeId = target.kind === 'local' ? target.id : null
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
    activateHomeTab,
    addHomeTab,
    closeActiveWorkspaceItem,
    closeHomeTab,
    confirmShortcutClose,
    dismissShortcutCloseConfirm,
    handleTabContextAction,
    openSystemInfo,
    openNetworkConnections,
    openFileTransfers,
    openTabContextMenu,
    closeTabContextMenu,
    startTabDrag,
    enterDraggedTab,
    endTabDrag,
    closeSessionTab
  }
}

function getPaneLeafIds(node: WorkspaceTab['paneRoot'], result: string[] = []) {
  if (!node) {
    return result
  }
  if (node.kind === 'leaf') {
    result.push(node.tabId)
    return result
  }
  for (const child of node.children) {
    getPaneLeafIds(child, result)
  }
  return result
}
