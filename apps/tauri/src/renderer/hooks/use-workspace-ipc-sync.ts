import { startTransition, useCallback, useEffect, useRef, useState, type Dispatch, type SetStateAction } from 'react'
import {
  mergeSystemMetricsHistory,
  type FileTermDesktopApi,
  type LocalFileItem,
  type OverviewSectionId,
  type PaneFocusDirection,
  type ResourceMonitoringMetric,
  type RemoteFilesUpdate,
  type SessionMetricsUpdate,
  type SavedTheme,
  type ThemeConfig,
  type TransferTask,
  type WorkspaceSnapshot
} from '@fileterm/core'
import { emptyState, localPreviewFiles, previewLocalPath, previewState } from '../app/app-data'
import { withParentRow } from '../app/app-utils'
import { t, type AppLocale } from '../i18n'
import { resolveRendererPlatform } from '../lib/renderer-platform'
import type { ThemeMode } from './use-theme-mode'

function isThemeMode(val: unknown): val is ThemeMode {
  return (
    val === 'fileterm-dark' ||
    val === 'fileterm-light' ||
    val === 'codex-dark' ||
    val === 'codex-light' ||
    val === 'default-dark' ||
    val === 'default-light'
  )
}
import {
  sameSyncedUiPreferences,
  syncedUiPreferencesFrom,
  uploadFailureBanner,
  useLatestRef,
  type SyncedUiPreferences,
  type SshConnectionDefaults
} from './workspace-ipc-sync-utils'

const SNAPSHOT_LISTENER_READY_TIMEOUT_MS = 5_000

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
  filePanelRememberRatio: boolean
  resourceMonitoringMetrics: ResourceMonitoringMetric[]
  resourceMonitoringMetricOrder: ResourceMonitoringMetric[]
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
  onFilePanelRememberRatioChange(value: boolean): void
  onResourceMonitoringMetricsChange(value: ResourceMonitoringMetric[]): void
  onResourceMonitoringMetricOrderChange(value: ResourceMonitoringMetric[]): void
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
  applySnapshot(snapshot: WorkspaceSnapshot): boolean
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
  filePanelRememberRatio,
  resourceMonitoringMetrics,
  resourceMonitoringMetricOrder,
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
  onFilePanelRememberRatioChange,
  onResourceMonitoringMetricsChange,
  onResourceMonitoringMetricOrderChange,
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
  const onFilePanelRememberRatioChangeRef = useLatestRef(onFilePanelRememberRatioChange)
  const onResourceMonitoringMetricsChangeRef = useLatestRef(onResourceMonitoringMetricsChange)
  const onResourceMonitoringMetricOrderChangeRef = useLatestRef(onResourceMonitoringMetricOrderChange)
  const onOverviewShowStatsChangeRef = useLatestRef(onOverviewShowStatsChange)
  const onOverviewShowRecentChangeRef = useLatestRef(onOverviewShowRecentChange)
  const onOverviewShowAllConnectionsChangeRef = useLatestRef(onOverviewShowAllConnectionsChange)
  const onOverviewShowQuickActionsChangeRef = useLatestRef(onOverviewShowQuickActionsChange)
  const onOverviewSectionOrderChangeRef = useLatestRef(onOverviewSectionOrderChange)
  const onErrorRef = useLatestRef(onError)
  const onStatusMessageRef = useLatestRef(onStatusMessage)
  const lastPersistedUiPreferencesRef = useRef<SyncedUiPreferences | null>(null)
  const latestWorkspaceRevisionRef = useRef<number | null>(null)
  const nextWindowCloseRequestIdRef = useRef(0)
  const nextSplitPaneRequestIdRef = useRef(0)
  const nextPaneFocusRequestIdRef = useRef(0)
  const notifiedTransferFailuresRef = useRef(new Map<string, string>())

  const logWorkspaceDiagnostic = useCallback(
    (level: 'DEBUG' | 'INFO' | 'WARN' | 'ERROR', message: string) => {
      if (!desktopApi) {
        return
      }
      // Diagnostics must never make a workspace event handler fail. The
      // backend logger is best-effort and also performs secret redaction and
      // size bounding before writing app.log.
      void desktopApi.writeDiagnosticLog(level, 'renderer:workspace', message).catch(() => undefined)
    },
    [desktopApi]
  )

  const applySnapshot = useCallback(
    (snapshot: WorkspaceSnapshot): boolean => {
      const incomingRevision = snapshot.workspaceRevision
      const latestRevision = latestWorkspaceRevisionRef.current
      if (typeof incomingRevision === 'number' && latestRevision !== null && incomingRevision < latestRevision) {
        logWorkspaceDiagnostic(
          'DEBUG',
          `workspace snapshot ignored reason=stale-revision incoming_revision=${incomingRevision} latest_revision=${latestRevision}`
        )
        return false
      }
      if (typeof incomingRevision === 'number') {
        latestWorkspaceRevisionRef.current = incomingRevision
      }
      setWorkspace(snapshot)
      const disabledResourceMonitoringSessions = Object.values(snapshot.sessions).filter(
        (session) => session.capabilities?.resourceMonitoring === false
      ).length
      const activeSession = snapshot.activeTabId ? snapshot.sessions[snapshot.activeTabId] : undefined
      logWorkspaceDiagnostic(
        'INFO',
        `workspace snapshot applied workspace_revision=${typeof incomingRevision === 'number' ? incomingRevision : 'none'} sessions=${Object.keys(snapshot.sessions).length} resource_monitoring_disabled_sessions=${disabledResourceMonitoringSessions} active_tab_id=${snapshot.activeTabId ?? 'none'} active_tab_resource_monitoring_disabled=${activeSession?.capabilities?.resourceMonitoring === false} active_tab_resource_monitoring_unavailable_reason=${activeSession?.resourceMonitoringUnavailableReason ?? 'none'}`
      )
      return true
    },
    [logWorkspaceDiagnostic]
  )

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

  const applyRemoteFilesUpdate = useCallback(({ tabId, path, files }: RemoteFilesUpdate) => {
    startTransition(() => {
      setWorkspace((current) => {
        const currentSession = current.sessions[tabId]
        if (!currentSession || currentSession.remotePath !== path) {
          return current
        }

        return {
          ...current,
          sessions: {
            ...current.sessions,
            [tabId]: {
              ...currentSession,
              remoteFiles: files,
              remoteFilesLoading: false
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
      if (isThemeMode(preferences.theme)) {
        onThemeModeChangeRef.current(preferences.theme)
      }
      onThemeConfigChangeRef.current(preferences.themeConfig)
      onCustomThemesChangeRef.current(preferences.customThemes)
      if (preferences.locale === 'enUS' || preferences.locale === 'zhCN') {
        onLocaleChangeRef.current(preferences.locale)
      }
      onConnectionDefaultsChangeRef.current(preferences.connectionDefaults)
      onTerminalZoomLockedChangeRef.current(preferences.terminalZoomLocked)
      onFilePanelRememberRatioChangeRef.current(preferences.filePanelRememberRatio)
      onResourceMonitoringMetricsChangeRef.current(preferences.resourceMonitoringMetrics)
      onResourceMonitoringMetricOrderChangeRef.current(preferences.resourceMonitoringMetricOrder)
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
        if (isThemeMode(preferences.theme)) {
          onThemeModeChangeRef.current(preferences.theme)
        }
        onThemeConfigChangeRef.current(preferences.themeConfig)
        onCustomThemesChangeRef.current(preferences.customThemes)
        if (preferences.locale === 'enUS' || preferences.locale === 'zhCN') {
          onLocaleChangeRef.current(preferences.locale)
        }
        onConnectionDefaultsChangeRef.current(preferences.connectionDefaults)
        onTerminalZoomLockedChangeRef.current(preferences.terminalZoomLocked)
        onFilePanelRememberRatioChangeRef.current(preferences.filePanelRememberRatio)
        onResourceMonitoringMetricsChangeRef.current(preferences.resourceMonitoringMetrics)
        onResourceMonitoringMetricOrderChangeRef.current(preferences.resourceMonitoringMetricOrder)
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
      filePanelRememberRatio,
      resourceMonitoringMetrics,
      resourceMonitoringMetricOrder,
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
    filePanelRememberRatio,
    resourceMonitoringMetrics,
    resourceMonitoringMetricOrder,
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
    let receivedMetricsEventCount = 0
    const pendingTransfers: TransferTask[] = []
    const pendingRemoteFiles: RemoteFilesUpdate[] = []
    let unsubscribeSnapshot: (() => void) | null = null
    let snapshotListenerReadyTimer: ReturnType<typeof window.setTimeout> | null = null
    const clearSnapshotListenerReadyTimer = () => {
      if (snapshotListenerReadyTimer === null) {
        return
      }
      window.clearTimeout(snapshotListenerReadyTimer)
      snapshotListenerReadyTimer = null
    }

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
      for (const payload of pendingRemoteFiles.splice(0)) {
        applyRemoteFilesUpdate(payload)
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

    const snapshotListenerRegistration = desktopApi
      .onWorkspaceSnapshot((snapshot) => {
        if (canceled) {
          return
        }
        logWorkspaceDiagnostic(
          'DEBUG',
          `workspace snapshot event received workspace_revision=${typeof snapshot.workspaceRevision === 'number' ? snapshot.workspaceRevision : 'none'} sessions=${Object.keys(snapshot.sessions).length} active_tab_id=${snapshot.activeTabId ?? 'none'}`
        )
        receivedSnapshotEvent = applySnapshot(snapshot) || receivedSnapshotEvent
        finishHydration()
      })
      .then((unsubscribe) => {
        if (canceled) {
          unsubscribe()
          return
        }
        unsubscribeSnapshot = unsubscribe
        logWorkspaceDiagnostic('DEBUG', 'workspace snapshot listener registered')
      })
      .catch((error: unknown) => {
        if (!canceled) {
          logWorkspaceDiagnostic('WARN', `workspace snapshot listener registration failed error=${String(error)}`)
        }
      })
      .finally(clearSnapshotListenerReadyTimer)
    const snapshotListenerReady = Promise.race([
      snapshotListenerRegistration,
      new Promise<void>((resolve) => {
        snapshotListenerReadyTimer = window.setTimeout(() => {
          snapshotListenerReadyTimer = null
          if (!canceled) {
            logWorkspaceDiagnostic(
              'WARN',
              `workspace snapshot listener registration timeout timeout_ms=${SNAPSHOT_LISTENER_READY_TIMEOUT_MS}`
            )
          }
          resolve()
        }, SNAPSHOT_LISTENER_READY_TIMEOUT_MS)
      })
    ])
    const unsubscribeSessionMetrics = isMainWorkspaceWindow
      ? desktopApi.onSessionMetrics((payload) => {
          if (canceled) {
            return
          }
          receivedMetricsEventCount += 1
          const shouldLogMetricsEvent = receivedMetricsEventCount === 1 || receivedMetricsEventCount % 60 === 0
          if (shouldLogMetricsEvent) {
            logWorkspaceDiagnostic(
              'DEBUG',
              `session metrics event received count=${receivedMetricsEventCount} tab_id=${payload.tabId} mode=${payload.mode ?? 'replace'} has_system_metrics=${payload.systemMetrics !== undefined} hydrated=${hydrated}`
            )
          }
          if (!hydrated) {
            pendingMetrics.push(payload)
            logWorkspaceDiagnostic(
              'DEBUG',
              `session metrics event queued tab_id=${payload.tabId} reason=workspace-not-hydrated pending_count=${pendingMetrics.length}`
            )
            return
          }
          applySessionMetrics(payload)
          if (shouldLogMetricsEvent) {
            logWorkspaceDiagnostic(
              'DEBUG',
              `session metrics update scheduled count=${receivedMetricsEventCount} tab_id=${payload.tabId} mode=${payload.mode ?? 'replace'}`
            )
          }
        })
      : () => undefined
    const unsubscribeTransferUpdate = isMainWorkspaceWindow
      ? desktopApi.onTransferUpdate((transfer) => {
          if (canceled) {
            return
          }
          if (!hydrated) {
            pendingTransfers.push(transfer)
            return
          }
          processTransferUpdate(transfer)
        })
      : () => undefined
    const unsubscribeRemoteFilesChanged = isMainWorkspaceWindow
      ? desktopApi.onRemoteFilesChanged((payload) => {
          if (canceled) {
            return
          }
          if (!hydrated) {
            pendingRemoteFiles.push(payload)
            return
          }
          applyRemoteFilesUpdate(payload)
        })
      : () => undefined

    const hydrateWorkspace = async () => {
      try {
        // Tauri event registration is asynchronous. Wait for the snapshot
        // listener before taking the initial snapshot so a fast connection
        // cannot emit the capability=false update into an unregistered
        // WebView callback. getSnapshot below reconciles events emitted while
        // the listener handshake was in flight.
        await snapshotListenerReady
        if (canceled) {
          return
        }
        // A standalone connection editor only needs persisted profiles and
        // folders. Do not couple it to the full workspace snapshot: that
        // snapshot initializes transfer/session state first and can fail or
        // race while a child window opens, leaving the editor unable to find
        // the profile selected in the manager.
        if (isConnectionManagerWindow || isConnectionFormWindow) {
          logWorkspaceDiagnostic('DEBUG', 'workspace library hydration started')
          const snapshot = await desktopApi.getConnectionLibrary()
          if (canceled || receivedSnapshotEvent) {
            return
          }
          setWorkspace((current) => ({
            ...current,
            profiles: snapshot.profiles,
            folders: snapshot.folders
          }))
          logWorkspaceDiagnostic(
            'DEBUG',
            `workspace library hydration applied profiles=${snapshot.profiles.length} folders=${snapshot.folders.length}`
          )
          return
        }

        logWorkspaceDiagnostic('DEBUG', 'workspace snapshot hydration started')
        const snapshot = await desktopApi.getSnapshot()
        if (!canceled && !receivedSnapshotEvent) {
          applySnapshot(snapshot)
        }
      } catch (error) {
        if (!canceled && !receivedSnapshotEvent) {
          logWorkspaceDiagnostic('WARN', `workspace hydration failed error=${String(error)}`)
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
      pendingRemoteFiles.length = 0
      clearSnapshotListenerReadyTimer()
      unsubscribeSnapshot?.()
      unsubscribeSessionMetrics()
      unsubscribeTransferUpdate()
      unsubscribeRemoteFilesChanged()
    }
  }, [
    applySessionMetrics,
    applyRemoteFilesUpdate,
    applySnapshot,
    applyTransferUpdate,
    desktopApi,
    isConnectionFormWindow,
    isConnectionManagerWindow,
    isMainWorkspaceWindow,
    logWorkspaceDiagnostic
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
