import { startTransition, useCallback, useEffect, useRef, useState, type Dispatch, type SetStateAction } from 'react'
import {
  mergeSystemMetricsHistory,
  type FileTermDesktopApi,
  type LocalFileItem,
  type SessionMetricsUpdate,
  type TransferTask,
  type WorkspaceSnapshot
} from '@fileterm/core'
import { emptyState, localPreviewFiles, previewLocalPath, previewState } from '../app/app-data'
import { withParentRow } from '../app/app-utils'
import { t, type AppLocale } from '../i18n'
import type { ThemeMode } from './useThemeMode'

export type WorkspaceWindowCloseRequest = {
  id: number
  isQuit: boolean
}

export type UseWorkspaceIpcSyncOptions = {
  desktopApi?: FileTermDesktopApi
  isMainWorkspaceWindow: boolean
  isConnectionManagerWindow: boolean
  themeMode: ThemeMode
  locale: AppLocale
  onThemeModeChange(themeMode: ThemeMode): void
  onLocaleChange(locale: AppLocale): void
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
  hasLoadedInitialSnapshot: boolean
  isMaximized: boolean
  windowCloseRequest: WorkspaceWindowCloseRequest | null
  clearWindowCloseRequest(): void
  closeActiveRequestVersion: number
  closeCurrentWindow(): void
  requestQuitApp(): void
}

function useLatestRef<T>(value: T) {
  const ref = useRef(value)
  ref.current = value
  return ref
}

function isUploadPermissionFailure(transfer: TransferTask) {
  if (transfer.direction !== 'upload' || !['failed', 'paused', 'interrupted'].includes(transfer.status)) {
    return false
  }

  return /permission[\s_-]*denied|access[\s_-]*denied|operation[\s_-]*not[\s_-]*permitted|not[\s_-]*permitted|authorization[\s_-]*failed|\b(?:eacces|eperm)\b|权限不足|没有权限|无权|拒绝访问/i.test(
    transfer.message ?? ''
  )
}

export function useWorkspaceIpcSync({
  desktopApi,
  isMainWorkspaceWindow,
  isConnectionManagerWindow,
  themeMode,
  locale,
  onThemeModeChange,
  onLocaleChange,
  onError,
  onStatusMessage
}: UseWorkspaceIpcSyncOptions): UseWorkspaceIpcSyncResult {
  const [workspace, setWorkspace] = useState<WorkspaceSnapshot>(emptyState)
  const [localPath, setLocalPath] = useState(previewLocalPath)
  const [localItems, setLocalItems] = useState<LocalFileItem[]>(localPreviewFiles)
  const [hasLoadedInitialSnapshot, setHasLoadedInitialSnapshot] = useState(false)
  const [isMaximized, setIsMaximized] = useState(false)
  const [windowCloseRequest, setWindowCloseRequest] = useState<WorkspaceWindowCloseRequest | null>(null)
  const [closeActiveRequestVersion, setCloseActiveRequestVersion] = useState(0)

  const desktopApiRef = useLatestRef(desktopApi)
  const onThemeModeChangeRef = useLatestRef(onThemeModeChange)
  const onLocaleChangeRef = useLatestRef(onLocaleChange)
  const onErrorRef = useLatestRef(onError)
  const onStatusMessageRef = useLatestRef(onStatusMessage)
  const nextWindowCloseRequestIdRef = useRef(0)
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
    const platform = desktopApi?.platform ?? 'browser'
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
      onThemeModeChangeRef.current(preferences.theme)
      onLocaleChangeRef.current(preferences.locale)
    })
  }, [desktopApi])

  useEffect(() => {
    if (!desktopApi) {
      return
    }

    let canceled = false
    void desktopApi.setUiPreferences({ theme: themeMode, locale }).catch((error: unknown) => {
      if (!canceled) {
        onErrorRef.current('同步界面偏好', error)
      }
    })

    return () => {
      canceled = true
    }
  }, [desktopApi, locale, themeMode])

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

    return () => {
      unsubscribeWindowClose()
      unsubscribeCloseActive()
    }
  }, [desktopApi, isMainWorkspaceWindow])

  useEffect(() => {
    let canceled = false
    setHasLoadedInitialSnapshot(false)

    if (!desktopApi) {
      setWorkspace(previewState)
      setLocalPath(previewLocalPath)
      setLocalItems(localPreviewFiles)
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
      if (isMainWorkspaceWindow && isUploadPermissionFailure(transfer)) {
        const notificationKey = `${transfer.status}:${transfer.updatedAt ?? ''}:${transfer.message ?? ''}`
        if (notifiedTransferFailuresRef.current.get(transfer.id) !== notificationKey) {
          notifiedTransferFailuresRef.current.set(transfer.id, notificationKey)
          onStatusMessageRef.current(t.uploadPermissionDenied)
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
        if (isConnectionManagerWindow) {
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
          onErrorRef.current(isConnectionManagerWindow ? '获取连接列表' : '获取工作区快照', error)
        }
      } finally {
        finishHydration()
      }
    }

    void hydrateWorkspace()

    if (isMainWorkspaceWindow) {
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
    hasLoadedInitialSnapshot,
    isMaximized,
    windowCloseRequest,
    clearWindowCloseRequest,
    closeActiveRequestVersion,
    closeCurrentWindow,
    requestQuitApp
  }
}
