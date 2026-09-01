import { useCallback, useEffect, useMemo, useRef, useState, type Dispatch, type SetStateAction } from 'react'
import type {
  FileContentSnapshot,
  FileTermDesktopApi,
  McpAgentClientStatus,
  ResourceMonitoringMetric,
  SshConnectionDefaults,
  WorkspaceSnapshot
} from '@fileterm/core'
import { DEFAULT_COMMAND_LIST_WIDTH, DEFAULT_FILE_PANEL_RATIO, retainOpenTabUiState } from '../app/app-shell-utils'
import type { ErrorDetails } from '../app/app-error-utils'
import { useWorkspaceTabs } from './use-workspace-tabs'
import type { UseWorkspaceTabsOptions } from './workspace-tabs-types'
import { useWorkspaceModals } from './use-workspace-modals'
import { useFileEditor } from './use-file-editor'
import { useFileOperations } from './use-file-operations'
import { useSshInteractions } from './use-ssh-interactions'
import { useBackupPasswordInteractions } from './use-backup-password-interactions'
import { useSudoPasswordPrompt } from './use-sudo-password-prompt'
import type { WorkspacePaneFocusRequest, WorkspaceSplitPaneRequest } from './use-workspace-ipc-sync'
import type { AppLocale } from '../i18n'
import { DEFAULT_FILE_PANEL_SNAP_TARGET, type FilePanelSnapTarget } from '../features/workspace/file-panel-snap'

type ErrorFormatter = (scope: string, error: unknown, details?: ErrorDetails) => string
type ErrorReporter = (scope: string, error: unknown, details?: ErrorDetails) => void

export type AppWorkspaceOptions = {
  desktopApi?: FileTermDesktopApi
  workspace: WorkspaceSnapshot
  isMainWorkspaceWindow: boolean
  isConnectionFormWindow: boolean
  isFileEditorWindow: boolean
  formWindowMode: 'create' | 'edit'
  formWindowProfileId: string | null
  formWindowCommandId: string | null
  formWindowFolderId: string | null
  formWindowCommand: string
  fileEditorWindowSource: FileContentSnapshot['source'] | null
  fileEditorWindowPath: string | null
  fileEditorWindowName: string | null
  fileEditorWindowTabId: string | null
  fileEditorWindowEncoding: string
  hasLoadedInitialSnapshot: boolean
  locale: AppLocale
  connectionDefaults: SshConnectionDefaults
  filePanelRememberRatio: boolean
  hasLoadedFilePanelRatios: boolean
  filePanelHeights: Record<string, number>
  setFilePanelHeights: Dispatch<SetStateAction<Record<string, number>>>
  filePanelRatios: Record<string, number>
  setFilePanelRatios: Dispatch<SetStateAction<Record<string, number>>>
  filePanelSnapTargets: Record<string, FilePanelSnapTarget | null>
  setFilePanelSnapTargets: Dispatch<SetStateAction<Record<string, FilePanelSnapTarget | null>>>
  commandPaneWidths: Record<string, number>
  setCommandPaneWidths: Dispatch<SetStateAction<Record<string, number>>>
  workspaceFocusModes: Record<string, boolean>
  setWorkspaceFocusModes: Dispatch<SetStateAction<Record<string, boolean>>>
  workspaceViews: Record<string, 'file' | 'command' | 'tunnel'>
  setWorkspaceViews: Dispatch<SetStateAction<Record<string, 'file' | 'command' | 'tunnel'>>>
  resourceMonitoringMetrics: ResourceMonitoringMetric[]
  resourceMonitoringMetricOrder: ResourceMonitoringMetric[]
  isAiCopilotOpen: boolean
  isBusy: boolean
  setIsBusy(value: boolean): void
  localPath: string
  localItems: import('@fileterm/core').LocalFileItem[]
  setLocalPath: import('react').Dispatch<import('react').SetStateAction<string>>
  setLocalItems: import('react').Dispatch<import('react').SetStateAction<import('@fileterm/core').LocalFileItem[]>>
  setIsLocalDirectoryLoading: import('react').Dispatch<import('react').SetStateAction<boolean>>
  closeActiveRequestVersion: number
  newTabRequestVersion: number
  splitPaneRequest: WorkspaceSplitPaneRequest | null
  paneFocusRequest: WorkspacePaneFocusRequest | null
  applySnapshot(snapshot: WorkspaceSnapshot): void
  onError: ErrorReporter
  onStatusMessage(message: string): void
  formatError: ErrorFormatter
  closeCurrentWindow(): void
  requestQuitApp(): void
}

export function useAppWorkspace({
  desktopApi,
  workspace,
  isMainWorkspaceWindow,
  isConnectionFormWindow,
  isFileEditorWindow,
  formWindowMode,
  formWindowProfileId,
  formWindowCommandId,
  formWindowFolderId,
  formWindowCommand,
  fileEditorWindowSource,
  fileEditorWindowPath,
  fileEditorWindowName,
  fileEditorWindowTabId,
  fileEditorWindowEncoding,
  hasLoadedInitialSnapshot,
  locale,
  connectionDefaults,
  filePanelRememberRatio,
  hasLoadedFilePanelRatios,
  filePanelHeights,
  setFilePanelHeights,
  filePanelRatios,
  setFilePanelRatios,
  filePanelSnapTargets,
  setFilePanelSnapTargets,
  commandPaneWidths,
  setCommandPaneWidths,
  workspaceFocusModes,
  setWorkspaceFocusModes,
  workspaceViews,
  setWorkspaceViews,
  resourceMonitoringMetrics,
  resourceMonitoringMetricOrder,
  isAiCopilotOpen,
  isBusy,
  setIsBusy,
  localPath,
  localItems,
  setLocalPath,
  setLocalItems,
  setIsLocalDirectoryLoading,
  closeActiveRequestVersion,
  newTabRequestVersion,
  splitPaneRequest,
  paneFocusRequest,
  applySnapshot,
  onError,
  onStatusMessage,
  formatError,
  closeCurrentWindow,
  requestQuitApp
}: AppWorkspaceOptions) {
  const [isWorkspaceTransitionActive, setIsWorkspaceTransitionActive] = useState(true)
  const [isWorkspaceSwitching, setIsWorkspaceSwitching] = useState(false)
  const hasRenderedWorkspaceRef = useRef(false)

  const {
    localTabs,
    tabContextMenu,
    shortcutCloseConfirm,
    isSystemSidebarCollapsed: isSystemSidebarUserCollapsed,
    visibleWorkspaceTabs,
    backgroundWorkspaceTabs,
    activeLocalTab,
    visibleActiveSessionTabId,
    activeTab,
    addHomeTab,
    isHomeWorkspaceVisible,
    showSidebar,
    effectiveActiveLocalTabId,
    activeSession,
    activeProfile,
    activePaneTab,
    activePaneSession,
    activeWorkspaceOrderKey,
    workspaceNavDirection,
    orderedTabs,
    sessionSendTargets,
    activeTerminalDockSendState,
    updateTerminalDockSendScope,
    updateTerminalDockSelectedTabIds,
    sendTerminalCommand,
    openProfile,
    openLocalTerminal,
    activateSessionTab,
    attachBackgroundSession,
    detachSessionToBackground,
    closeBackgroundSession,
    reconnectSessionTab,
    confirmShortcutClose,
    handleTabContextAction,
    openTabContextMenu,
    closeTabContextMenu,
    startTabDrag,
    enterDraggedTab,
    endTabDrag,
    setIsSystemSidebarCollapsed,
    dismissShortcutCloseConfirm,
    activateHomeTab,
    closeHomeTab,
    closeSessionTab,
    openSystemInfo,
    splitPane,
    closePane,
    closeActiveWorkspaceItem,
    activatePane,
    setPaneWeights
  } = useWorkspaceTabs({
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
    onSnapshot: applySnapshot,
    onBusyChange: setIsBusy,
    onStatusMessage,
    onError,
    onCloseCurrentWindow: closeCurrentWindow,
    onRequestQuit: requestQuitApp
  } satisfies UseWorkspaceTabsOptions)

  const activeWorkspaceFocusKey = activeTab?.id ?? effectiveActiveLocalTabId
  const isWorkspaceFocusMode = activeWorkspaceFocusKey ? (workspaceFocusModes[activeWorkspaceFocusKey] ?? false) : false
  const activeWorkspaceView = activeTab ? (workspaceViews[activeTab.id] ?? 'file') : 'file'
  const activeCommandPaneWidth = activeTab
    ? (commandPaneWidths[activeTab.id] ?? DEFAULT_COMMAND_LIST_WIDTH)
    : DEFAULT_COMMAND_LIST_WIDTH
  const isLocalTerminalWorkspace = activeTab?.sessionType === 'local'

  const activeFilePanelHeight = activeTab ? (filePanelHeights[activeTab.id] ?? 218) : 218
  const activeFilePanelRatio = activeTab
    ? filePanelRememberRatio && hasLoadedFilePanelRatios
      ? (filePanelRatios[activeTab.profileId] ?? DEFAULT_FILE_PANEL_RATIO)
      : DEFAULT_FILE_PANEL_RATIO
    : DEFAULT_FILE_PANEL_RATIO
  const activeFilePanelSnapTarget = activeTab
    ? filePanelRememberRatio && hasLoadedFilePanelRatios
      ? Object.prototype.hasOwnProperty.call(filePanelSnapTargets, activeTab.profileId)
        ? filePanelSnapTargets[activeTab.profileId]
        : Object.prototype.hasOwnProperty.call(filePanelRatios, activeTab.profileId)
          ? null
          : DEFAULT_FILE_PANEL_SNAP_TARGET
      : DEFAULT_FILE_PANEL_SNAP_TARGET
    : null

  const activeSshResourceMonitoring =
    activeProfile?.type === 'ssh'
      ? (activeProfile.connectionOverrides?.enableResourceMonitoring ??
        activeProfile.enableResourceMonitoring ??
        connectionDefaults.enableResourceMonitoring)
      : false
  const isResourceMonitoringAvailable = Boolean(
    activeProfile?.type === 'ssh' &&
    activeProfile.deviceMode !== 'network-device' &&
    activeSshResourceMonitoring &&
    activeSession?.capabilities?.resourceMonitoring !== false
  )
  const activeSidebarMetrics =
    activeProfile?.type === 'ssh' && activeProfile.resourceMonitoringMetrics
      ? activeProfile.resourceMonitoringMetrics
      : resourceMonitoringMetrics
  const activeSidebarMetricOrder =
    activeProfile?.type === 'ssh' && activeProfile.resourceMonitoringMetricOrder
      ? activeProfile.resourceMonitoringMetricOrder
      : resourceMonitoringMetricOrder
  const shouldShowSystemSidebar = showSidebar && !isLocalTerminalWorkspace
  const isSystemSidebarCollapsed =
    isSystemSidebarUserCollapsed || isWorkspaceFocusMode || Boolean(activeTab && !isResourceMonitoringAvailable)
  const activeTabId = activeTab?.id ?? null
  const aiCopilotTargetTab = activePaneTab ?? activeTab
  const aiCopilotTargetSession = activePaneSession ?? activeSession
  const isAiCopilotAvailable = Boolean(activeTab)
  const shouldShowAiCopilot = isAiCopilotOpen && !isHomeWorkspaceVisible

  const setActiveFilePanelHeight = useCallback(
    (next: SetStateAction<number>) => {
      if (!activeTabId) {
        return
      }
      const tabId = activeTabId
      setFilePanelHeights((currentHeights) => {
        const currentHeight = currentHeights[tabId] ?? 218
        const nextHeight = typeof next === 'function' ? next(currentHeight) : next
        if (currentHeight === nextHeight) {
          return currentHeights
        }
        return { ...currentHeights, [tabId]: nextHeight }
      })
    },
    [activeTabId]
  )
  const commitActiveFilePanelRatio = useCallback(
    (nextRatio: number) => {
      if (!activeTab || !filePanelRememberRatio || !hasLoadedFilePanelRatios) return
      const ratio = Math.max(0, Math.min(70, nextRatio))
      setFilePanelRatios((currentRatios) => {
        if (currentRatios[activeTab.profileId] === ratio) return currentRatios
        return { ...currentRatios, [activeTab.profileId]: ratio }
      })
    },
    [activeTab, filePanelRememberRatio, hasLoadedFilePanelRatios]
  )
  const commitActiveFilePanelSnapTarget = useCallback(
    (nextTarget: FilePanelSnapTarget | null) => {
      if (!activeTab || !filePanelRememberRatio || !hasLoadedFilePanelRatios) return
      setFilePanelSnapTargets((currentTargets) => {
        if (
          Object.prototype.hasOwnProperty.call(currentTargets, activeTab.profileId) &&
          currentTargets[activeTab.profileId] === nextTarget
        ) {
          return currentTargets
        }
        return { ...currentTargets, [activeTab.profileId]: nextTarget }
      })
    },
    [activeTab, filePanelRememberRatio, hasLoadedFilePanelRatios]
  )
  const setActiveCommandPaneWidth = useCallback(
    (nextWidth: number) => {
      if (!activeTabId) return
      setCommandPaneWidths((currentWidths) => ({ ...currentWidths, [activeTabId]: nextWidth }))
    },
    [activeTabId]
  )

  useEffect(() => {
    const openTabIds = new Set([...visibleWorkspaceTabs.map((tab) => tab.id), ...localTabs.map((tab) => tab.id)])
    setFilePanelHeights((currentHeights) => retainOpenTabUiState(currentHeights, openTabIds))
    setCommandPaneWidths((currentWidths) => retainOpenTabUiState(currentWidths, openTabIds))
    setWorkspaceFocusModes((currentModes) => retainOpenTabUiState(currentModes, openTabIds))
    setWorkspaceViews((currentViews) => retainOpenTabUiState(currentViews, openTabIds))
  }, [localTabs, visibleWorkspaceTabs])

  const {
    closeConnectionForm,
    connectionGroupOptions,
    editingProfileId,
    form,
    formError,
    openCommandManagerFromSettings,
    openConnectionManagerFromSettings,
    openCreateConnection,
    openEditConnection,
    requestWindowCloseConfirmation,
    resolveWindowCloseConfirmation,
    setShowCommandManager,
    setShowConnectionManager,
    setShowSettings,
    showCommandManager,
    showConnectionForm,
    showConnectionManager,
    showSettings,
    updateForm,
    windowCloseConfirm,
    openCommandManager,
    setForm,
    setFormError
  } = useWorkspaceModals({
    desktopApi,
    folders: workspace.folders || [],
    formWindowMode,
    formWindowProfileId,
    hasLoadedInitialSnapshot,
    isConnectionFormWindow,
    connectionDefaults,
    profiles: workspace.profiles || []
  })

  const fileEditorWindowInput = useMemo(() => {
    if (!isFileEditorWindow || !fileEditorWindowSource || !fileEditorWindowPath || !fileEditorWindowName) {
      return null
    }
    return {
      source: fileEditorWindowSource,
      path: fileEditorWindowPath,
      name: fileEditorWindowName,
      tabId: fileEditorWindowTabId ?? undefined,
      encoding: fileEditorWindowEncoding
    }
  }, [
    fileEditorWindowName,
    fileEditorWindowPath,
    fileEditorWindowSource,
    fileEditorWindowTabId,
    fileEditorWindowEncoding,
    isFileEditorWindow
  ])
  const openLocalDirectoryRef = useRef<(targetPath: string) => Promise<void>>(async () => undefined)

  const {
    close: closeFileEditor,
    file: fileEditor,
    isBusy: isFileEditorBusy,
    isDirty: isFileEditorDirty,
    isSaving: isFileEditorSaving,
    errorMessage: fileEditorError,
    openLocalFile,
    openRemoteFile,
    reloadWithEncoding: reloadFileEditorWithEncoding,
    save: saveFileEditor,
    checkDirty: checkFileEditorDirty
  } = useFileEditor({
    activeTabId: activeTab?.id ?? null,
    desktopApi,
    formatError,
    isFileEditorWindow,
    onApplySnapshot: applySnapshot,
    onLocalFileSaved: async () => {
      await openLocalDirectoryRef.current(localPath)
    },
    onStatusMessage,
    windowInput: fileEditorWindowInput
  })
  const fileEditorDirtyRef = useRef(isFileEditorDirty)
  fileEditorDirtyRef.current = isFileEditorDirty
  const fileEditorSavingRef = useRef(isFileEditorSaving)
  fileEditorSavingRef.current = isFileEditorSaving
  const pendingFileEditorCloseAfterSaveRef = useRef(false)
  const [isFileEditorDiscardConfirmOpen, setIsFileEditorDiscardConfirmOpen] = useState(false)

  const requestFileEditorClose = () => {
    if (isFileEditorDirty) {
      setIsFileEditorDiscardConfirmOpen(true)
      return
    }
    if (!desktopApi) {
      closeCurrentWindow()
      return
    }
    void desktopApi.confirmCloseCurrentFileEditor().catch((err: unknown) => {
      onError('关闭文件编辑器', err)
    })
  }
  const confirmFileEditorDiscard = () => {
    setIsFileEditorDiscardConfirmOpen(false)
    if (!desktopApi) {
      closeCurrentWindow()
      return
    }
    void desktopApi.confirmCloseCurrentFileEditor().catch((err: unknown) => {
      onError('关闭文件编辑器', err)
    })
  }
  const cancelFileEditorDiscard = () => {
    pendingFileEditorCloseAfterSaveRef.current = false
    setIsFileEditorDiscardConfirmOpen(false)
    if (!desktopApi || !isFileEditorWindow) {
      return
    }
    void desktopApi.cancelCloseCurrentFileEditor().catch((err: unknown) => {
      onError('取消关闭文件编辑器', err)
    })
  }

  useEffect(() => {
    if (!desktopApi || !isFileEditorWindow) {
      setIsFileEditorDiscardConfirmOpen(false)
      return
    }

    return desktopApi.onFileEditorCloseRequest(() => {
      if (fileEditorSavingRef.current) {
        pendingFileEditorCloseAfterSaveRef.current = true
        void desktopApi.showCurrentWindow().catch((err: unknown) => {
          onError('显示文件编辑器', err)
        })
        return
      }
      if (fileEditorDirtyRef.current) {
        void desktopApi.showCurrentWindow().catch((err: unknown) => {
          onError('显示文件编辑器', err)
        })
        setIsFileEditorDiscardConfirmOpen(true)
        return
      }
      void desktopApi.confirmCloseCurrentFileEditor().catch((err: unknown) => {
        onError('关闭文件编辑器', err)
      })
    })
  }, [desktopApi, isFileEditorWindow])

  useEffect(() => {
    if (!desktopApi || !isFileEditorWindow || isFileEditorSaving || !pendingFileEditorCloseAfterSaveRef.current) {
      return
    }

    pendingFileEditorCloseAfterSaveRef.current = false
    if (isFileEditorDirty) {
      setIsFileEditorDiscardConfirmOpen(true)
      return
    }
    void desktopApi.confirmCloseCurrentFileEditor().catch((err: unknown) => {
      onError('关闭文件编辑器', err)
    })
  }, [desktopApi, isFileEditorDirty, isFileEditorSaving, isFileEditorWindow])

  const fileOperations = useFileOperations({
    desktopApi,
    workspace,
    activeTab,
    activeSession,
    activeProfile,
    locale,
    localPath,
    localItems,
    setLocalPath,
    setLocalItems,
    setIsLocalDirectoryLoading,
    onApplySnapshot: applySnapshot,
    onBusyChange: setIsBusy,
    onStatusMessage,
    formatError,
    openLocalFile: (item) => openLocalFile(item),
    openRemoteFile: (tabId, item, nextLocale) => openRemoteFile(tabId, item, nextLocale)
  })
  openLocalDirectoryRef.current = fileOperations.openLocalDirectory

  const {
    credentialsRequest,
    keyboardInteractiveRequest,
    hostVerificationRequest,
    keyPassphraseRequest,
    errorMessage: sshInteractionError,
    isResolving: isSshInteractionResolving,
    waitForSshInteractionListener,
    cancelCredentials,
    submitCredentials,
    cancelKeyboardInteractive,
    submitKeyboardInteractive,
    cancelKeyPassphrase,
    submitKeyPassphrase,
    rejectHost,
    acceptHostOnce,
    acceptHostAndSave
  } = useSshInteractions({
    desktopApi,
    isMainWorkspaceWindow,
    isConnectionFormWindow,
    isConnectionFormOpen: showConnectionForm,
    onError
  })

  const sshInteractionPortalProps = {
    sshCredentials: credentialsRequest
      ? {
          errorMessage: sshInteractionError,
          isSubmitting: isSshInteractionResolving,
          request: credentialsRequest,
          onCancel: cancelCredentials,
          onSubmit: submitCredentials
        }
      : null,
    sshHostVerification: hostVerificationRequest
      ? {
          request: hostVerificationRequest,
          isSubmitting: isSshInteractionResolving,
          onReject: rejectHost,
          onAcceptOnce: acceptHostOnce,
          onAcceptAndSave: acceptHostAndSave
        }
      : null,
    sshKeyPassphrase: keyPassphraseRequest
      ? {
          errorMessage: sshInteractionError,
          isSubmitting: isSshInteractionResolving,
          request: keyPassphraseRequest,
          onCancel: cancelKeyPassphrase,
          onSubmit: submitKeyPassphrase
        }
      : null,
    sshKeyboardInteractive: keyboardInteractiveRequest
      ? {
          request: keyboardInteractiveRequest,
          errorMessage: sshInteractionError,
          isSubmitting: isSshInteractionResolving,
          onCancel: () => {
            void cancelKeyboardInteractive()
          },
          onSubmit: (answers: string[]) => {
            void submitKeyboardInteractive(answers)
          }
        }
      : null
  }

  const {
    request: backupPasswordRequest,
    errorMessage: backupPasswordError,
    isResolving: isBackupPasswordResolving,
    cancel: cancelBackupPassword,
    submit: submitBackupPassword
  } = useBackupPasswordInteractions({
    desktopApi: isMainWorkspaceWindow ? desktopApi : undefined,
    onError
  })
  const {
    request: sudoPasswordRequest,
    errorMessage: sudoPasswordError,
    isResolving: isSudoPasswordResolving,
    cancel: cancelSudoPassword,
    submit: submitSudoPassword
  } = useSudoPasswordPrompt({
    desktopApi: isMainWorkspaceWindow ? desktopApi : undefined,
    onError
  })

  useEffect(() => {
    setIsWorkspaceTransitionActive(false)
    const frame = window.requestAnimationFrame(() => setIsWorkspaceTransitionActive(true))
    return () => window.cancelAnimationFrame(frame)
  }, [activeWorkspaceOrderKey])
  useEffect(() => {
    if (!hasRenderedWorkspaceRef.current) {
      hasRenderedWorkspaceRef.current = true
      return
    }
    setIsWorkspaceSwitching(true)
    const timeout = window.setTimeout(() => setIsWorkspaceSwitching(false), 240)
    return () => window.clearTimeout(timeout)
  }, [activeWorkspaceOrderKey])

  const launchLocalAgent = useCallback(
    (client: McpAgentClientStatus) => {
      const launch =
        client.id === 'claude-code'
          ? { title: 'Claude Code', command: 'claude' }
          : client.id === 'codex-cli'
            ? { title: 'Codex CLI', command: 'codex' }
            : { title: 'OpenCode', command: 'opencode' }
      void openLocalTerminal({ title: launch.title }, launch.command)
    },
    [openLocalTerminal]
  )

  return {
    ...fileOperations,
    localTabs,
    tabContextMenu,
    shortcutCloseConfirm,
    isSystemSidebarUserCollapsed,
    visibleWorkspaceTabs,
    backgroundWorkspaceTabs,
    activeLocalTab,
    visibleActiveSessionTabId,
    activeTab,
    addHomeTab,
    isHomeWorkspaceVisible,
    showSidebar,
    effectiveActiveLocalTabId,
    activeSession,
    activeProfile,
    activePaneTab,
    activePaneSession,
    activeWorkspaceOrderKey,
    workspaceNavDirection,
    orderedTabs,
    sessionSendTargets,
    activeTerminalDockSendState,
    updateTerminalDockSendScope,
    updateTerminalDockSelectedTabIds,
    sendTerminalCommand,
    openProfile,
    openLocalTerminal,
    activateSessionTab,
    attachBackgroundSession,
    detachSessionToBackground,
    closeBackgroundSession,
    reconnectSessionTab,
    confirmShortcutClose,
    handleTabContextAction,
    openTabContextMenu,
    closeTabContextMenu,
    startTabDrag,
    enterDraggedTab,
    endTabDrag,
    setIsSystemSidebarCollapsed,
    dismissShortcutCloseConfirm,
    activateHomeTab,
    closeHomeTab,
    closeSessionTab,
    openSystemInfo,
    splitPane,
    closePane,
    closeActiveWorkspaceItem,
    activatePane,
    setPaneWeights,
    activeFilePanelHeight,
    activeFilePanelRatio,
    activeFilePanelSnapTarget,
    activeWorkspaceFocusKey,
    isWorkspaceFocusMode,
    activeWorkspaceView,
    activeCommandPaneWidth,
    activeSidebarMetrics,
    activeSidebarMetricOrder,
    isResourceMonitoringAvailable,
    shouldShowSystemSidebar,
    isSystemSidebarCollapsed,
    activeTabId,
    aiCopilotTargetTab,
    aiCopilotTargetSession,
    isAiCopilotAvailable,
    shouldShowAiCopilot,
    setActiveFilePanelHeight,
    commitActiveFilePanelRatio,
    commitActiveFilePanelSnapTarget,
    setActiveCommandPaneWidth,
    isLocalTerminalWorkspace,
    connectionGroupOptions,
    editingProfileId,
    form,
    formError,
    closeConnectionForm,
    openCommandManagerFromSettings,
    openConnectionManagerFromSettings,
    openCreateConnection,
    openEditConnection,
    requestWindowCloseConfirmation,
    resolveWindowCloseConfirmation,
    setShowCommandManager,
    setShowConnectionManager,
    setShowSettings,
    showCommandManager,
    showConnectionForm,
    showConnectionManager,
    showSettings,
    updateForm,
    windowCloseConfirm,
    openCommandManager,
    setForm,
    setFormError,
    closeFileEditor,
    fileEditor,
    isFileEditorBusy,
    isFileEditorDirty,
    isFileEditorSaving,
    fileEditorError,
    reloadFileEditorWithEncoding,
    saveFileEditor,
    checkFileEditorDirty,
    requestFileEditorClose,
    confirmFileEditorDiscard,
    cancelFileEditorDiscard,
    isFileEditorDiscardConfirmOpen,
    sshInteractionPortalProps,
    backupPasswordRequest,
    backupPasswordError,
    isBackupPasswordResolving,
    cancelBackupPassword,
    submitBackupPassword,
    sudoPasswordRequest,
    sudoPasswordError,
    isSudoPasswordResolving,
    cancelSudoPassword,
    submitSudoPassword,
    waitForSshInteractionListener,
    launchLocalAgent,
    isWorkspaceTransitionActive,
    isWorkspaceSwitching
  }
}
