import { startTransition, useCallback, useEffect, useRef, useState, type Dispatch, type SetStateAction } from 'react'
import {
  mergeSystemMetricsHistory,
  type FileTermDesktopApi,
  type LocalFileItem,
  type OverviewSectionId,
  type PaneFocusDirection,
  type SessionMetricsUpdate,
  type SavedTheme,
  type SshConnectionDefaults,
  type ThemeConfig,
  type TransferTask,
  type UiPreferences,
  type WorkspaceSnapshot
} from '@fileterm/core'
import { emptyState, localPreviewFiles, previewLocalPath, previewState } from '../app/app-data'
import { withParentRow } from '../app/app-utils'
import { t, type AppLocale } from '../i18n'
import { resolveRendererPlatform } from '../lib/renderer-platform'
import type { ThemeMode } from './useThemeMode'

export type WorkspaceWindowCloseRequest = {
  id: number
  isQuit: boolean
}

export type WorkspaceSplitPaneRequest = {
  id: number
  direction: 'row' | 'column'
}

export type WorkspacePaneFocusRequest = {
  id: number
  direction: PaneFocusDirection
}

export type UseWorkspaceIpcSyncOptions = {
  desktopApi?: FileTermDesktopApi
  isConnectionFormWindow: boolean
  isMainWorkspaceWindow: boolean
  isConnectionManagerWindow: boolean
  themeMode: ThemeMode
  themeConfig: ThemeConfig
  customThemes: SavedTheme[]
  locale: AppLocale
  connectionDefaults: SshConnectionDefaults
  terminalZoomLocked: boolean
  overviewShowStats: boolean
  overviewShowRecent: boolean
  overviewShowAllConnections: boolean
  overviewShowQuickActions: boolean
  overviewSectionOrder: OverviewSectionId[]
  initialUiPreferencesLoaded: boolean
  onThemeModeChange(themeMode: ThemeMode): void
  onThemeConfigChange(themeConfig: ThemeConfig): void
  onCustomThemesChange(customThemes: SavedTheme[]): void
  onLocaleChange(locale: AppLocale): void
  onConnectionDefaultsChange(value: Partial<SshConnectionDefaults>): void
  onTerminalZoomLockedChange(value: boolean): void
  onOverviewShowStatsChange(value: boolean): void
  onOverviewShowRecentChange(value: boolean): void
  onOverviewShowAllConnectionsChange(value: boolean): void
  onOverviewShowQuickActionsChange(value: boolean): void
  onOverviewSectionOrderChange(value: OverviewSectionId[]): void
  onError(scope: string, error: unknown): void
  onStatusMessage(message: string): void
}

export type UseWorkspaceIpcSyncResult = {
  workspace: WorkspaceSnapshot
  setWorkspace: Dispatch<SetStateAction<WorkspaceSnapshot>>
  applySnapshot(snapshot: WorkspaceSnapshot): void
  localPath: string
  setLocalPath: Dispatch<SetStateAction<string>>
  localItems: LocalFileItem[]
  setLocalItems: Dispatch<SetStateAction<LocalFileItem[]>>
  isLocalDirectoryLoading: boolean
  setIsLocalDirectoryLoading: Dispatch<SetStateAction<boolean>>
  hasLoadedInitialSnapshot: boolean
  isMaximized: boolean
  windowCloseRequest: WorkspaceWindowCloseRequest | null
  clearWindowCloseRequest(): void
  closeActiveRequestVersion: number
  newTabRequestVersion: number
  splitPaneRequest: WorkspaceSplitPaneRequest | null
  paneFocusRequest: WorkspacePaneFocusRequest | null
  closeCurrentWindow(): void
  requestQuitApp(): void
}

function useLatestRef<T>(value: T) {
  const ref = useRef(value)
  ref.current = value
  return ref
}

type SyncedUiPreferences = Pick<
  UiPreferences,
  | 'theme'
  | 'themeConfig'
  | 'customThemes'
  | 'locale'
  | 'connectionDefaults'
  | 'terminalZoomLocked'
  | 'overviewShowStats'
  | 'overviewShowRecent'
  | 'overviewShowAllConnections'
  | 'overviewShowQuickActions'
  | 'overviewSectionOrder'
>

function sameSyncedUiPreferences(left: SyncedUiPreferences, right: SyncedUiPreferences) {
  return (
    left.theme === right.theme &&
    JSON.stringify(left.themeConfig) === JSON.stringify(right.themeConfig) &&
    JSON.stringify(left.customThemes) === JSON.stringify(right.customThemes) &&
    left.locale === right.locale &&
    left.terminalZoomLocked === right.terminalZoomLocked &&
    left.connectionDefaults.useEmptyPassword === right.connectionDefaults.useEmptyPassword &&
    left.connectionDefaults.enableExecChannel === right.connectionDefaults.enableExecChannel &&
    left.connectionDefaults.enableResourceMonitoring === right.connectionDefaults.enableResourceMonitoring &&
    left.connectionDefaults.resourceMonitoringIntervalSeconds ===
      right.connectionDefaults.resourceMonitoringIntervalSeconds &&
    left.connectionDefaults.reconnectMode === right.connectionDefaults.reconnectMode &&
    left.connectionDefaults.legacyAlgorithms === right.connectionDefaults.legacyAlgorithms &&
    left.overviewShowStats === right.overviewShowStats &&
    left.overviewShowRecent === right.overviewShowRecent &&
    left.overviewShowAllConnections === right.overviewShowAllConnections &&
    left.overviewShowQuickActions === right.overviewShowQuickActions &&
    left.overviewSectionOrder.length === right.overviewSectionOrder.length &&
    left.overviewSectionOrder.every((sectionId, index) => sectionId === right.overviewSectionOrder[index])
  )
}

function syncedUiPreferencesFrom(preferences: UiPreferences): SyncedUiPreferences {
  return {
    theme: preferences.theme,
    themeConfig: preferences.themeConfig,
    customThemes: preferences.customThemes,
    locale: preferences.locale,
    connectionDefaults: { ...preferences.connectionDefaults },
    terminalZoomLocked: preferences.terminalZoomLocked,
    overviewShowStats: preferences.overviewShowStats,
    overviewShowRecent: preferences.overviewShowRecent,
    overviewShowAllConnections: preferences.overviewShowAllConnections,
    overviewShowQuickActions: preferences.overviewShowQuickActions,
    overviewSectionOrder: preferences.overviewSectionOrder
  }
}

function isUploadPermissionFailure(transfer: TransferTask) {
  if (transfer.direction !== 'upload' || !['failed', 'paused', 'interrupted'].includes(transfer.status)) {
    return false
  }

  return /permission[\s_-]*denied|access[\s_-]*denied|operation[\s_-]*not[\s_-]*permitted|not[\s_-]*permitted|authorization[\s_-]*failed|\b(?:eacces|eperm)\b|权限不足|没有权限|无权|拒绝访问/i.test(
    transfer.message ?? ''
  )
}

function isRootUploadCommandFailure(transfer: TransferTask) {
  if (transfer.direction !== 'upload' || !['failed', 'paused', 'interrupted'].includes(transfer.status)) {
    return false
  }

  return /root\s+(?:文件|上传|写入)|(?:^|\b)(?:su|sudo)\s+|密码|password/i.test(transfer.message ?? '')
}

function uploadFailureBanner(transfer: TransferTask) {
  if (isUploadPermissionFailure(transfer)) {
    return t.uploadPermissionDenied
  }
  if (isRootUploadCommandFailure(transfer)) {
    const detail = transfer.message?.replace(/^command error:\s*/i, '').trim()
    return detail ? `${t.uploadFailed}: ${detail}` : t.uploadFailed
  }
  return null
}

export function useWorkspaceIpcSync({
  desktopApi,
  isConnectionFormWindow,
  isMainWorkspaceWindow,
  isConnectionManagerWindow,
  themeMode,
  themeConfig,
  customThemes,
  locale,
  connectionDefaults,
  terminalZoomLocked,
  overviewShowStats,
  overviewShowRecent,
  overviewShowAllConnections,
  overviewShowQuickActions,
  overviewSectionOrder,
  initialUiPreferencesLoaded,
  onThemeModeChange,
  onThemeConfigChange,
  onCustomThemesChange,
  onLocaleChange,
  onConnectionDefaultsChange,
  onTerminalZoomLockedChange,
  onOverviewShowStatsChange,
  onOverviewShowRecentChange,
  onOverviewShowAllConnectionsChange,
  onOverviewShowQuickActionsChange,
  onOverviewSectionOrderChange,
  onError,
  onStatusMessage
}: UseWorkspaceIpcSyncOptions): UseWorkspaceIpcSyncResult {
  const [workspace, setWorkspace] = useState<WorkspaceSnapshot>(emptyState)
  const [localPath, setLocalPath] = useState(previewLocalPath)
  const [localItems, setLocalItems] = useState<LocalFileItem[]>(localPreviewFiles)
  const [isLocalDirectoryLoading, setIsLocalDirectoryLoading] = useState(false)
  const [hasLoadedInitialSnapshot, setHasLoadedInitialSnapshot] = useState(false)
  const [hasHydratedUiPreferences, setHasHydratedUiPreferences] = useState(
    () => !desktopApi || initialUiPreferencesLoaded
  )
  const [canPersistUiPreferences, setCanPersistUiPreferences] = useState(() =>
    Boolean(desktopApi && initialUiPreferencesLoaded)
  )
  const [isMaximized, setIsMaximized] = useState(false)
  const [windowCloseRequest, setWindowCloseRequest] = useState<WorkspaceWindowCloseRequest | null>(null)
  const [closeActiveRequestVersion, setCloseActiveRequestVersion] = useState(0)
  const [newTabRequestVersion, setNewTabRequestVersion] = useState(0)
  const [splitPaneRequest, setSplitPaneRequest] = useState<WorkspaceSplitPaneRequest | null>(null)
  const [paneFocusRequest, setPaneFocusRequest] = useState<WorkspacePaneFocusRequest | null>(null)

  const desktopApiRef = useLatestRef(desktopApi)
  const onThemeModeChangeRef = useLatestRef(onThemeModeChange)
  const onThemeConfigChangeRef = useLatestRef(onThemeConfigChange)
  const onCustomThemesChangeRef = useLatestRef(onCustomThemesChange)
  const onLocaleChangeRef = useLatestRef(onLocaleChange)
  const onConnectionDefaultsChangeRef = useLatestRef(onConnectionDefaultsChange)
  const onTerminalZoomLockedChangeRef = useLatestRef(onTerminalZoomLockedChange)
  const onOverviewShowStatsChangeRef = useLatestRef(onOverviewShowStatsChange)
  const onOverviewShowRecentChangeRef = useLatestRef(onOverviewShowRecentChange)
  const onOverviewShowAllConnectionsChangeRef = useLatestRef(onOverviewShowAllConnectionsChange)
  const onOverviewShowQuickActionsChangeRef = useLatestRef(onOverviewShowQuickActionsChange)
  const onOverviewSectionOrderChangeRef = useLatestRef(onOverviewSectionOrderChange)
  const onErrorRef = useLatestRef(onError)
  const onStatusMessageRef = useLatestRef(onStatusMessage)
  const lastPersistedUiPreferencesRef = useRef<SyncedUiPreferences | null>(null)
  const nextWindowCloseRequestIdRef = useRef(0)
  const nextSplitPaneRequestIdRef = useRef(0)
  const nextPaneFocusRequestIdRef = useRef(0)
  const notifiedTransferFailuresRef = useRef(new Map<string, string>())

  const applySnapshot = useCallback((snapshot: WorkspaceSnapshot) => {
    setWorkspace(snapshot)
  }, [])

  const applySessionMetrics = useCallback(({ tabId, systemMetrics, mode }: SessionMetricsUpdate) => {
    startTransition(() => {
      setWorkspace((current) => {
        const currentSession = current.sessions[tabId]
        if (!currentSession) {
          return current
        }

        const nextSystemMetrics =
          systemMetrics && mode === 'append'
            ? mergeSystemMetricsHistory(currentSession.systemMetrics, systemMetrics)
            : systemMetrics

        if (currentSession.systemMetrics === nextSystemMetrics) {
          return current
        }

        return {
          ...current,
          sessions: {
            ...current.sessions,
            [tabId]: {
              ...currentSession,
              systemMetrics: nextSystemMetrics
            }
          }
        }
      })
    })
  }, [])

  const applyTransferUpdate = useCallback((transfer: TransferTask) => {
    startTransition(() => {
      setWorkspace((current) => {
        const transferIndex = current.transfers.findIndex((item) => item.id === transfer.id)
        if (transferIndex === -1) {
          return {
            ...current,
            transfers: [transfer, ...current.transfers]
          }
        }

        if (current.transfers[transferIndex] === transfer) {
          return current
        }

        const transfers = [...current.transfers]
        transfers[transferIndex] = transfer
        return {
          ...current,
          transfers
        }
      })
    })
  }, [])

  useEffect(() => {
    const platform = resolveRendererPlatform(desktopApi?.platform ?? 'browser')
    document.documentElement.dataset.platform = platform

    return () => {
      if (document.documentElement.dataset.platform === platform) {
        delete document.documentElement.dataset.platform
      }
    }
  }, [desktopApi])

  useEffect(() => {
    if (!desktopApi) {
      return
    }

    return desktopApi.onUiPreferencesChanged((preferences) => {
      lastPersistedUiPreferencesRef.current = syncedUiPreferencesFrom(preferences)
      if (preferences.theme === 'default-light' || preferences.theme === 'default-dark') {
        onThemeModeChangeRef.current(preferences.theme)
      }
      onThemeConfigChangeRef.current(preferences.themeConfig)
      onCustomThemesChangeRef.current(preferences.customThemes)
      if (preferences.locale === 'enUS' || preferences.locale === 'zhCN') {
        onLocaleChangeRef.current(preferences.locale)
      }
      onConnectionDefaultsChangeRef.current(preferences.connectionDefaults)
      onTerminalZoomLockedChangeRef.current(preferences.terminalZoomLocked)
      onOverviewShowStatsChangeRef.current(preferences.overviewShowStats)
      onOverviewShowRecentChangeRef.current(preferences.overviewShowRecent)
      onOverviewShowAllConnectionsChangeRef.current(preferences.overviewShowAllConnections)
      onOverviewShowQuickActionsChangeRef.current(preferences.overviewShowQuickActions)
      onOverviewSectionOrderChangeRef.current(preferences.overviewSectionOrder)
    })
  }, [desktopApi])

  useEffect(() => {
    if (!desktopApi || initialUiPreferencesLoaded) {
      setHasHydratedUiPreferences(true)
      setCanPersistUiPreferences(Boolean(desktopApi))
      return
    }

    let canceled = false

    void desktopApi
      .getUiPreferences()
      .then((preferences) => {
        if (canceled) {
          return
        }
        lastPersistedUiPreferencesRef.current = syncedUiPreferencesFrom(preferences)
        setCanPersistUiPreferences(true)
        if (preferences.theme === 'default-light' || preferences.theme === 'default-dark') {
          onThemeModeChangeRef.current(preferences.theme)
        }
        onThemeConfigChangeRef.current(preferences.themeConfig)
        onCustomThemesChangeRef.current(preferences.customThemes)
        if (preferences.locale === 'enUS' || preferences.locale === 'zhCN') {
          onLocaleChangeRef.current(preferences.locale)
        }
        onConnectionDefaultsChangeRef.current(preferences.connectionDefaults)
        onTerminalZoomLockedChangeRef.current(preferences.terminalZoomLocked)
        onOverviewShowStatsChangeRef.current(preferences.overviewShowStats)
        onOverviewShowRecentChangeRef.current(preferences.overviewShowRecent)
        onOverviewShowAllConnectionsChangeRef.current(preferences.overviewShowAllConnections)
        onOverviewShowQuickActionsChangeRef.current(preferences.overviewShowQuickActions)
        onOverviewSectionOrderChangeRef.current(preferences.overviewSectionOrder)
      })
      .catch((error: unknown) => {
        if (!canceled) {
          setCanPersistUiPreferences(false)
          onErrorRef.current('读取界面偏好', error)
        }
      })
      .finally(() => {
        if (!canceled) {
          setHasHydratedUiPreferences(true)
        }
      })

    return () => {
      canceled = true
    }
  }, [desktopApi, initialUiPreferencesLoaded])

  useEffect(() => {
    if (!desktopApi || !hasHydratedUiPreferences || !canPersistUiPreferences) {
      return
    }

    const nextPreferences: SyncedUiPreferences = {
      theme: themeMode,
      themeConfig,
      customThemes,
      locale,
      connectionDefaults,
      terminalZoomLocked,
      overviewShowStats,
      overviewShowRecent,
      overviewShowAllConnections,
      overviewShowQuickActions,
      overviewSectionOrder
    }
    if (
      lastPersistedUiPreferencesRef.current &&
      sameSyncedUiPreferences(lastPersistedUiPreferencesRef.current, nextPreferences)
    ) {
      return
    }

    lastPersistedUiPreferencesRef.current = nextPreferences
    let canceled = false
    void desktopApi.setUiPreferences(nextPreferences).catch((error: unknown) => {
      if (!canceled) {
        if (
          lastPersistedUiPreferencesRef.current &&
          sameSyncedUiPreferences(lastPersistedUiPreferencesRef.current, nextPreferences)
        ) {
          lastPersistedUiPreferencesRef.current = null
        }
        onErrorRef.current('同步界面偏好', error)
      }
    })

    return () => {
      canceled = true
    }
  }, [
    canPersistUiPreferences,
    connectionDefaults,
    desktopApi,
    hasHydratedUiPreferences,
    locale,
    overviewShowAllConnections,
    overviewShowRecent,
    overviewShowStats,
    overviewShowQuickActions,
    overviewSectionOrder,
    terminalZoomLocked,
    customThemes,
    themeConfig,
    themeMode
  ])

  useEffect(() => {
    if (!desktopApi || !isMainWorkspaceWindow) {
      setIsMaximized(false)
      return
    }

    let canceled = false
    let receivedMaximizedEvent = false
    const unsubscribe = desktopApi.onWindowMaximizedChange((nextIsMaximized) => {
      if (canceled) {
        return
      }
      receivedMaximizedEvent = true
      setIsMaximized(nextIsMaximized)
    })

    void desktopApi
      .isCurrentWindowMaximized()
      .then((nextIsMaximized) => {
        if (!canceled && !receivedMaximizedEvent) {
          setIsMaximized(nextIsMaximized)
        }
      })
      .catch((error: unknown) => {
        if (!canceled) {
          onErrorRef.current('读取窗口状态', error)
        }
      })

    return () => {
      canceled = true
      unsubscribe()
    }
  }, [desktopApi, isMainWorkspaceWindow])

  useEffect(() => {
    if (!desktopApi || !isMainWorkspaceWindow) {
      return
    }

    const unsubscribeWindowClose = desktopApi.onWindowCloseRequest(({ isQuit }) => {
      nextWindowCloseRequestIdRef.current += 1
      setWindowCloseRequest({
        id: nextWindowCloseRequestIdRef.current,
        isQuit
      })
    })
    const unsubscribeCloseActive = desktopApi.onRequestCloseActiveWorkspaceItem(() => {
      setCloseActiveRequestVersion((current) => current + 1)
    })
    const unsubscribeNewTab = desktopApi.onNewTabRequest(() => {
      setNewTabRequestVersion((current) => current + 1)
    })
    const unsubscribeSplitPane = desktopApi.onSplitPaneRequest((direction) => {
      nextSplitPaneRequestIdRef.current += 1
      setSplitPaneRequest({
        id: nextSplitPaneRequestIdRef.current,
        direction
      })
    })
    const unsubscribePaneFocus = desktopApi.onFocusPaneRequest((direction) => {
      nextPaneFocusRequestIdRef.current += 1
      setPaneFocusRequest({
        id: nextPaneFocusRequestIdRef.current,
        direction
      })
    })

    return () => {
      unsubscribeWindowClose()
      unsubscribeCloseActive()
      unsubscribeNewTab()
      unsubscribeSplitPane()
      unsubscribePaneFocus()
    }
  }, [desktopApi, isMainWorkspaceWindow])

  useEffect(() => {
    let canceled = false
    setHasLoadedInitialSnapshot(false)

    if (!desktopApi) {
      setWorkspace(previewState)
      setLocalPath(previewLocalPath)
      setLocalItems(localPreviewFiles)
      setIsLocalDirectoryLoading(false)
      setHasLoadedInitialSnapshot(true)
      if (isMainWorkspaceWindow) {
        onStatusMessageRef.current(t.browserPreview)
      }
      return () => {
        canceled = true
      }
    }

    let hydrated = false
    let receivedSnapshotEvent = false
    const pendingMetrics: SessionMetricsUpdate[] = []
    const pendingTransfers: TransferTask[] = []

    const processTransferUpdate = (transfer: TransferTask) => {
      const banner = uploadFailureBanner(transfer)
      if (isMainWorkspaceWindow && banner) {
        const notificationKey = `${transfer.status}:${transfer.updatedAt ?? ''}:${transfer.message ?? ''}`
        if (notifiedTransferFailuresRef.current.get(transfer.id) !== notificationKey) {
          notifiedTransferFailuresRef.current.set(transfer.id, notificationKey)
          onStatusMessageRef.current(banner)
        }
      } else if (!['failed', 'paused', 'interrupted'].includes(transfer.status)) {
        notifiedTransferFailuresRef.current.delete(transfer.id)
      }

      applyTransferUpdate(transfer)
    }

    const flushPendingUpdates = () => {
      for (const payload of pendingMetrics.splice(0)) {
        applySessionMetrics(payload)
      }
      for (const transfer of pendingTransfers.splice(0)) {
        processTransferUpdate(transfer)
      }
    }

    const finishHydration = () => {
      if (canceled || hydrated) {
        return
      }
      hydrated = true
      setHasLoadedInitialSnapshot(true)
      flushPendingUpdates()
    }

    const unsubscribeSnapshot = desktopApi.onWorkspaceSnapshot((snapshot) => {
      if (canceled) {
        return
      }
      receivedSnapshotEvent = true
      applySnapshot(snapshot)
      finishHydration()
    })
    const unsubscribeSessionMetrics = desktopApi.onSessionMetrics((payload) => {
      if (canceled) {
        return
      }
      if (!hydrated) {
        pendingMetrics.push(payload)
        return
      }
      applySessionMetrics(payload)
    })
    const unsubscribeTransferUpdate = desktopApi.onTransferUpdate((transfer) => {
      if (canceled) {
        return
      }
      if (!hydrated) {
        pendingTransfers.push(transfer)
        return
      }
      processTransferUpdate(transfer)
    })

    const hydrateWorkspace = async () => {
      try {
        // A standalone connection editor only needs persisted profiles and
        // folders. Do not couple it to the full workspace snapshot: that
        // snapshot initializes transfer/session state first and can fail or
        // race while a child window opens, leaving the editor unable to find
        // the profile selected in the manager.
        if (isConnectionManagerWindow || isConnectionFormWindow) {
          const snapshot = await desktopApi.getConnectionLibrary()
          if (canceled || receivedSnapshotEvent) {
            return
          }
          setWorkspace((current) => ({
            ...current,
            profiles: snapshot.profiles,
            folders: snapshot.folders
          }))
          return
        }

        const snapshot = await desktopApi.getSnapshot()
        if (!canceled && !receivedSnapshotEvent) {
          applySnapshot(snapshot)
        }
      } catch (error) {
        if (!canceled && !receivedSnapshotEvent) {
          onErrorRef.current(
            isConnectionManagerWindow || isConnectionFormWindow ? '获取连接列表' : '获取工作区快照',
            error
          )
        }
      } finally {
        finishHydration()
      }
    }

    void hydrateWorkspace()

    if (isMainWorkspaceWindow) {
      setIsLocalDirectoryLoading(true)
      void desktopApi
        .listLocalDirectory()
        .then(({ path, items }) => {
          if (canceled) {
            return
          }
          setLocalPath(path)
          setLocalItems(withParentRow(path, items))
        })
        .catch((error: unknown) => {
          if (!canceled) {
            onErrorRef.current('读取本机目录', error)
          }
        })
        .finally(() => {
          if (!canceled) {
            setIsLocalDirectoryLoading(false)
          }
        })
    }

    return () => {
      canceled = true
      pendingMetrics.length = 0
      pendingTransfers.length = 0
      unsubscribeSnapshot()
      unsubscribeSessionMetrics()
      unsubscribeTransferUpdate()
    }
  }, [
    applySessionMetrics,
    applySnapshot,
    applyTransferUpdate,
    desktopApi,
    isConnectionFormWindow,
    isConnectionManagerWindow,
    isMainWorkspaceWindow
  ])

  const clearWindowCloseRequest = useCallback(() => {
    setWindowCloseRequest(null)
  }, [])

  const closeCurrentWindow = useCallback(() => {
    const currentDesktopApi = desktopApiRef.current
    if (!currentDesktopApi) {
      return
    }
    void currentDesktopApi.closeCurrentWindow().catch((error: unknown) => {
      onErrorRef.current('关闭当前窗口', error)
    })
  }, [])

  const requestQuitApp = useCallback(() => {
    const currentDesktopApi = desktopApiRef.current
    if (!currentDesktopApi) {
      return
    }
    void currentDesktopApi.requestQuitApp().catch((error: unknown) => {
      onErrorRef.current('退出应用', error)
    })
  }, [])

  return {
    workspace,
    setWorkspace,
    applySnapshot,
    localPath,
    setLocalPath,
    localItems,
    setLocalItems,
    isLocalDirectoryLoading,
    setIsLocalDirectoryLoading,
    hasLoadedInitialSnapshot,
    isMaximized,
    windowCloseRequest,
    clearWindowCloseRequest,
    closeActiveRequestVersion,
    newTabRequestVersion,
    splitPaneRequest,
    paneFocusRequest,
    closeCurrentWindow,
    requestQuitApp
  }
}
