import { APP_EVENT, dispatchAppEvent } from '../lib/app-events'
import { t } from '../i18n'
import type { WorkspaceTabsActionContext } from './workspace-tabs-types'
import { collectPaneLeafTabIds, findAdjacentPaneTabId } from './workspace-tabs-utils'

export function createWorkspaceTabsPaneActions(context: WorkspaceTabsActionContext) {
  const { desktopApi, workspace, onBusyChange, onSnapshot, onError } = context

  /** 基于指定 pane 新建独立 SSH session 或本地 PTY，不共享运行时。 */
  const splitPane = async (sourceTabId: string, direction: 'row' | 'column') => {
    if (!desktopApi) {
      return
    }

    const sourceTab = workspace.tabs.find((tab) => tab.id === sourceTabId)
    if (!sourceTab || (sourceTab.sessionType !== 'ssh' && sourceTab.sessionType !== 'local')) {
      return
    }
    try {
      onBusyChange(true)
      const snapshot = await desktopApi.splitTab(sourceTabId, direction)
      onSnapshot(snapshot)
      const rootTabId = workspace.activeTabId
      const newActivePaneId = (rootTabId && snapshot.activePaneTabIdByRoot?.[rootTabId]) ?? snapshot.activeTabId
      if (newActivePaneId) {
        window.requestAnimationFrame(() => {
          dispatchAppEvent(APP_EVENT.focusTerminal, newActivePaneId)
        })
      }
    } catch (error) {
      onError('splitPane', error)
    } finally {
      onBusyChange(false)
    }
  }

  const splitCurrentPane = async (direction: 'row' | 'column') => {
    if (!workspace.activeTabId) {
      return
    }
    const rootTabId = workspace.activeTabId
    const activePaneTabId = workspace.activePaneTabIdByRoot?.[rootTabId] ?? rootTabId
    await splitPane(activePaneTabId, direction)
  }

  const focusAdjacentPane = async (direction: Parameters<typeof findAdjacentPaneTabId>[2]) => {
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
      dispatchAppEvent(APP_EVENT.focusTerminal, targetPaneTabId)
    } catch (error) {
      onError('focusAdjacentPane', error)
    }
  }

  /** 关闭分屏中的单个 pane。 */
  const closePane = async (paneTabId: string) => {
    if (!desktopApi) {
      return
    }
    const rootTab =
      workspace.tabs.find((tab) => tab.paneRoot && collectPaneLeafTabIds(tab.paneRoot).includes(paneTabId)) ??
      workspace.tabs.find((tab) => tab.id === workspace.activeTabId)

    const rootTabId = rootTab?.id ?? workspace.activeTabId
    if (!rootTabId) {
      return
    }
    const activePaneTabId = workspace.activePaneTabIdByRoot?.[rootTabId] ?? rootTabId
    try {
      const snapshot = await desktopApi.closePane(rootTabId, paneTabId)
      onSnapshot(snapshot)
      if (paneTabId === activePaneTabId) {
        const nextRootTabId = snapshot.activeTabId ?? rootTabId
        const nextPaneTabId = snapshot.activePaneTabIdByRoot?.[nextRootTabId] ?? nextRootTabId
        window.requestAnimationFrame(() => {
          dispatchAppEvent(APP_EVENT.focusTerminal, nextPaneTabId)
        })
      }
    } catch (error) {
      onError(t.closePane, error)
    }
  }

  /** 设置分屏活跃 pane。 */
  const activatePane = async (paneTabId: string) => {
    if (!desktopApi || !workspace.activeTabId) {
      return
    }
    const rootTabId = workspace.activeTabId
    try {
      const snapshot = await desktopApi.setActivePane(rootTabId, paneTabId)
      onSnapshot(snapshot)
      window.requestAnimationFrame(() => {
        dispatchAppEvent(APP_EVENT.focusTerminal, paneTabId)
      })
    } catch (error) {
      onError('activatePane', error)
    }
  }

  /** 持久化分屏 weights。 */
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

  return { activatePane, closePane, focusAdjacentPane, setPaneWeights, splitCurrentPane, splitPane }
}
