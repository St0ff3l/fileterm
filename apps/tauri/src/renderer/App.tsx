import {
  Suspense,
  lazy,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type FormEvent,
  type SetStateAction
} from 'react'
import {
  type CommandExecutionOptions,
  type ConnectionFormMode,
  type ConnectionImportPlan,
  type ConnectionProfile,
  type CreateProfileInput,
  type FileContentSnapshot,
  type RemoteFileItem
} from '@fileterm/core'
import { normalizeConnectionHost, validateConnectionHost } from '@fileterm/shared'
import { profileToForm } from './app/app-data'
import { settledResultsError } from './app/app-utils'
import { CommandEditorModal, emptyCommandForm, toCommandTemplateInput } from './features/commands/CommandEditorModal'
import { CommandManagerModal } from './features/commands/CommandManagerModal'
import { ConnectionManagerModal } from './features/connections/ConnectionManagerModal'
import { ConnectionFormHost } from './features/connections/ConnectionFormHost'
import { ConnectionModal } from './features/connections/ConnectionModal'
import { ConnectionImportPreviewModal } from './features/connections/ConnectionImportPreviewModal'

const FileEditorModal = lazy(() =>
  import('./features/files/FileEditorModal').then((m) => ({ default: m.FileEditorModal }))
)

function retainOpenTabUiState<T>(state: Record<string, T>, openTabIds: Set<string>) {
  const entries = Object.entries(state)
  if (entries.every(([tabId]) => openTabIds.has(tabId))) {
    return state
  }

  return Object.fromEntries(entries.filter(([tabId]) => openTabIds.has(tabId)))
}
import { CloseButton } from './features/common/CloseButton'
import { ConfirmActionDialog } from './features/common/ConfirmActionDialog'
import type { SendScope } from './features/common/session-send-targets'
import { resolveSelectedTabIds } from './features/common/session-send-targets'
import { TabBar, type TabBarProps, type TabContextTarget } from './features/layout/TabBar'
import { WindowMenubar } from './features/layout/WindowMenubar'
import { SystemSidebarShell } from './features/system/SystemSidebarShell'
import { TransferCenterHost } from './features/transfers/TransferCenterHost'
import { WorkspaceStage } from './features/workspace/WorkspaceStage'
import { useThemeMode, type ThemeMode } from './hooks/useThemeMode'
import { defaultLocale, localizeErrorScope, setLocale, t, type AppLocale } from './i18n'

import { useWorkspaceIpcSync } from './hooks/useWorkspaceIpcSync'
import { useWorkspaceTabs } from './hooks/useWorkspaceTabs'
import { useWorkspaceModals } from './hooks/useWorkspaceModals'
import { useFileOperations } from './hooks/useFileOperations'
import { useSshInteractions } from './hooks/useSshInteractions'
import { useFileEditor } from './hooks/useFileEditor'
import { useWorkspaceDataOps } from './hooks/useWorkspaceDataOps'
import { ModalPortalManager, type FileActionModalBinding } from './features/layout/ModalPortalManager'
import { StandaloneWindowFrame } from './features/layout/StandaloneWindowFrame'

const STATUS_MESSAGE_TIMEOUT_MS = 15_000
const REMOTE_METHOD_ERROR_PREFIX = /Error invoking remote method '[^']+':\s*/i
const DEFAULT_SIDEBAR_WIDTH = 214
const SIDEBAR_SNAP_THRESHOLD = 10

type ErrorDetails = {
  item?: RemoteFileItem
  targetPath?: string
}

function readInitialTheme(searchParams: URLSearchParams): ThemeMode {
  const queryTheme = searchParams.get('theme')
  if (queryTheme === 'default-light' || queryTheme === 'default-dark') {
    return queryTheme
  }
  return 'default-dark'
}

function readInitialLocale(searchParams: URLSearchParams): AppLocale {
  const queryLocale = searchParams.get('locale')
  if (queryLocale === 'enUS' || queryLocale === 'zhCN') {
    return queryLocale
  }
  return defaultLocale
}

export function App() {
  const searchParams = new URLSearchParams(window.location.search)
  const windowMode = searchParams.get('window') ?? 'main'
  const isConnectionManagerWindow = windowMode === 'connection-manager'
  const isCommandManagerWindow = windowMode === 'command-manager'
  const isConnectionFormWindow = windowMode === 'connection-form'
  const isCommandFormWindow = windowMode === 'command-form'
  const isFileEditorWindow = windowMode === 'file-editor'
  const isMainWorkspaceWindow =
    !isConnectionManagerWindow &&
    !isCommandManagerWindow &&
    !isConnectionFormWindow &&
    !isCommandFormWindow &&
    !isFileEditorWindow

  const formWindowMode = (searchParams.get('mode') as ConnectionFormMode | null) ?? 'create'
  const formWindowProfileId = searchParams.get('profileId')
  const formWindowCommandId = searchParams.get('commandId')
  const formWindowFolderId = searchParams.get('folderId')

  const fileEditorWindowSource = searchParams.get('source') as FileContentSnapshot['source'] | null
  const fileEditorWindowPath = searchParams.get('path')
  const fileEditorWindowName = searchParams.get('name')
  const fileEditorWindowTabId = searchParams.get('tabId')
  const fileEditorWindowEncoding = searchParams.get('encoding') ?? 'utf-8'

  const [error, setError] = useState<string | null>(null)
  const [isBusy, setIsBusy] = useState(false)
  const profileSaveInFlightRef = useRef(false)
  const [isWorkspaceTransitionActive, setIsWorkspaceTransitionActive] = useState(true)
  const [themeMode, setThemeMode] = useState<ThemeMode>(() => readInitialTheme(searchParams))
  const [locale, setLocaleState] = useState<AppLocale>(() => readInitialLocale(searchParams))
  const [isFileEditorDiscardConfirmOpen, setIsFileEditorDiscardConfirmOpen] = useState(false)
  const [connectionImportPlan, setConnectionImportPlan] = useState<ConnectionImportPlan | null>(null)

  const [sidebarWidth, setSidebarWidth] = useState(214)
  const [filePanelHeights, setFilePanelHeights] = useState<Record<string, number>>({})
  const [workspaceFocusModes, setWorkspaceFocusModes] = useState<Record<string, boolean>>({})
  const [isResizingSidebar, setIsResizingSidebar] = useState(false)

  const desktopApi = window.fileterm
  const isWindowsDesktop = desktopApi?.platform === 'win32'
  const hasRevealedStandaloneWindowRef = useRef(false)

  const openConnectionImportPreview = (source: 'files' | 'folder' = 'files') => {
    void desktopApi
      ?.previewConnectionImport(source)
      .then((plan) => plan && setConnectionImportPlan(plan))
      .catch((cause) => reportError(setError, '读取连接配置', cause))
  }

  const commitConnectionJsonPreview = async (
    selectedItemIds: string[],
    conflictStrategy: 'skip' | 'overwrite' | 'create'
  ) => {
    if (!connectionImportPlan || !desktopApi) return
    try {
      const result = await desktopApi.commitConnectionJsonImport(connectionImportPlan.id, {
        selectedItemIds,
        conflictStrategy
      })
      setConnectionImportPlan(null)
      setError(
        `连接导入：新增 ${result.imported}，覆盖 ${result.overwritten ?? 0}，跳过 ${result.skipped}，失败 ${result.failed}`
      )
    } catch (cause) {
      reportError(setError, '导入连接', cause)
    }
  }

  useThemeMode(themeMode)

  // 1. IPC Synchronization Hook
  const {
    workspace,
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
    closeCurrentWindow,
    requestQuitApp
  } = useWorkspaceIpcSync({
    desktopApi,
    isConnectionFormWindow,
    isMainWorkspaceWindow,
    isConnectionManagerWindow,
    themeMode,
    locale,
    onThemeModeChange: setThemeMode,
    onLocaleChange: (nextLocale) => {
      setLocale(nextLocale)
      setLocaleState(nextLocale)
    },
    onError: (scope, err) => reportError(setError, scope, err),
    onStatusMessage: (msg) => setError(msg)
  })

  // Child windows remain hidden until their route's first data fetch has
  // settled. This is the Tauri equivalent of Electron's `ready-to-show` and
  // prevents a blank/transparent first paint from being visible to the user.
  useEffect(() => {
    if (isMainWorkspaceWindow || !desktopApi || !hasLoadedInitialSnapshot || hasRevealedStandaloneWindowRef.current) {
      return
    }
    hasRevealedStandaloneWindowRef.current = true
    void desktopApi.showCurrentWindow().catch((cause) => reportError(setError, '显示窗口', cause))
  }, [desktopApi, hasLoadedInitialSnapshot, isMainWorkspaceWindow])

  // 2. Workspace Tabs Hook
  const {
    localTabs,
    tabContextMenu,
    shortcutCloseConfirm,
    isSystemSidebarCollapsed: isSystemSidebarUserCollapsed,
    visibleWorkspaceTabs,
    activeLocalTab,
    visibleActiveSessionTabId,
    activeTab,
    activeSession,
    addHomeTab,
    isHomeWorkspaceVisible,
    showSidebar,
    effectiveActiveLocalTabId,
    activeProfile,
    activeWorkspaceOrderKey,
    workspaceNavDirection,
    orderedTabs,
    sessionSendTargets,
    activeTerminalDockSendState,
    updateTerminalDockSendScope,
    updateTerminalDockSelectedTabIds,
    sendTerminalCommand,
    openProfile,
    activateSessionTab,
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
    openSystemInfo
  } = useWorkspaceTabs({
    desktopApi,
    workspace,
    isMainWorkspaceWindow,
    hasLoadedInitialSnapshot,
    locale,
    isBusy,
    closeActiveRequestVersion,
    onSnapshot: applySnapshot,
    onBusyChange: setIsBusy,
    onStatusMessage: (msg) => setError(msg),
    onError: (scope, err) => reportError(setError, scope, err),
    onCloseCurrentWindow: closeCurrentWindow,
    onRequestQuit: requestQuitApp
  })

  const activeFilePanelHeight = activeTab ? (filePanelHeights[activeTab.id] ?? 218) : 218
  const shouldAlignFilePanelOnMount = activeTab
    ? !Object.prototype.hasOwnProperty.call(filePanelHeights, activeTab.id)
    : false
  const activeWorkspaceFocusKey = activeTab?.id ?? effectiveActiveLocalTabId
  const isWorkspaceFocusMode = activeWorkspaceFocusKey ? (workspaceFocusModes[activeWorkspaceFocusKey] ?? false) : false
  const isResourceMonitoringAvailable =
    activeProfile?.type === 'ssh' && activeProfile.enableResourceMonitoring !== false
  const isSystemSidebarCollapsed =
    isSystemSidebarUserCollapsed || isWorkspaceFocusMode || Boolean(activeTab && !isResourceMonitoringAvailable)
  const activeTabId = activeTab?.id ?? null
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

  useEffect(() => {
    setIsWorkspaceTransitionActive(false)
    const frame = window.requestAnimationFrame(() => {
      setIsWorkspaceTransitionActive(true)
    })

    return () => window.cancelAnimationFrame(frame)
  }, [activeWorkspaceOrderKey])

  useEffect(() => {
    const openTabIds = new Set([...visibleWorkspaceTabs.map((tab) => tab.id), ...localTabs.map((tab) => tab.id)])
    setFilePanelHeights((currentHeights) => retainOpenTabUiState(currentHeights, openTabIds))
    setWorkspaceFocusModes((currentModes) => retainOpenTabUiState(currentModes, openTabIds))
  }, [localTabs, visibleWorkspaceTabs])

  // 3. Workspace Modals Hook
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
    profiles: workspace.profiles || []
  })

  // 4. File Editor Hook
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
    formatError: (scope, err, details) => formatAppError(scope, err, details),
    isFileEditorWindow,
    onApplySnapshot: applySnapshot,
    onLocalFileSaved: async () => {
      await openLocalDirectory(localPath)
    },
    onStatusMessage: (msg) => setError(msg),
    windowInput: fileEditorWindowInput
  })
  const fileEditorDirtyRef = useRef(isFileEditorDirty)
  fileEditorDirtyRef.current = isFileEditorDirty
  const fileEditorSavingRef = useRef(isFileEditorSaving)
  fileEditorSavingRef.current = isFileEditorSaving
  const pendingFileEditorCloseAfterSaveRef = useRef(false)

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
      reportError(setError, '关闭文件编辑器', err)
    })
  }

  const confirmFileEditorDiscard = () => {
    setIsFileEditorDiscardConfirmOpen(false)
    if (!desktopApi) {
      closeCurrentWindow()
      return
    }
    void desktopApi.confirmCloseCurrentFileEditor().catch((err: unknown) => {
      reportError(setError, '关闭文件编辑器', err)
    })
  }

  const cancelFileEditorDiscard = () => {
    pendingFileEditorCloseAfterSaveRef.current = false
    setIsFileEditorDiscardConfirmOpen(false)
    if (!desktopApi || !isFileEditorWindow) {
      return
    }
    void desktopApi.cancelCloseCurrentFileEditor().catch((err: unknown) => {
      reportError(setError, '取消关闭文件编辑器', err)
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
          reportError(setError, '显示文件编辑器', err)
        })
        return
      }
      if (fileEditorDirtyRef.current) {
        void desktopApi.showCurrentWindow().catch((err: unknown) => {
          reportError(setError, '显示文件编辑器', err)
        })
        setIsFileEditorDiscardConfirmOpen(true)
        return
      }
      void desktopApi.confirmCloseCurrentFileEditor().catch((err: unknown) => {
        reportError(setError, '关闭文件编辑器', err)
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
      reportError(setError, '关闭文件编辑器', err)
    })
  }, [desktopApi, isFileEditorDirty, isFileEditorSaving, isFileEditorWindow])

  // 5. File Operations Hook
  const {
    isRemoteDirectoryLoading,
    isWorkspaceRefreshing,
    canPasteIntoLocal,
    canPasteIntoRemote,
    localCutPaths,
    remoteCutPaths,
    clipboardStatusText,
    fileActionDialog,
    fileActionError,
    isFileActionSubmitting,
    permissionDialog,
    permissionDialogError,
    isPermissionSubmitting,
    rootAccessDialog,
    rootAccessDialogError,
    isRootAccessSubmitting,
    localNetworkCredentialsDialog,
    localNetworkCredentialsDialogError,
    localNetworkShareDialog,
    localNetworkShareDialogError,
    isLocalNetworkCredentialsSubmitting,
    isLocalNetworkShare,
    localPanePath,
    remoteFileAccessMode,
    openLocalDirectory,
    handleOpenLocalItem,
    handleOpenLocalPath,
    handleOpenRemoteItem,
    handleOpenRemotePath,
    copyItems,
    cutItems,
    clearCutState,
    handlePasteIntoPane,
    handleSubmitFileAction,
    requestNewFolder,
    requestNewFile,
    requestRename,
    requestDelete,
    dismissFileActionDialog,
    requestChangePermissions,
    handleSubmitPermissions,
    dismissPermissionDialog,
    handleQuickDelete,
    handleConfirmRootAccess,
    handleToggleRemoteFileAccessMode,
    handleToggleFollowShellCwd,
    handleUploadFiles,
    handleChooseUploadFiles,
    handleDownloadFiles,
    handleDownloadLocalNetworkFiles,
    handleDropUpload,
    handleRefreshWorkspace,
    dismissRootAccessDialog,
    handleSubmitLocalNetworkCredentials,
    dismissLocalNetworkCredentialsDialog,
    handleSubmitLocalNetworkShare,
    dismissLocalNetworkShareDialog,
    changeLocalNetworkCredentials
  } = useFileOperations({
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
    onStatusMessage: (msg) => setError(msg),
    formatError: (scope, err, details) => formatAppError(scope, err, details),
    openLocalFile: (item) => openLocalFile(item),
    openRemoteFile: (tabId, item, loc) => openRemoteFile(tabId, item, loc)
  })

  // 6. SSH Interactions Hook
  const {
    credentialsRequest,
    keyboardInteractiveRequest,
    hostVerificationRequest,
    keyPassphraseRequest,
    errorMessage: sshInteractionError,
    isResolving: isSshInteractionResolving,
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
    onError: (scope, err) => reportError(setError, scope, err)
  })

  // Sidebars resizing logic
  const startSidebarResize = useCallback(() => {
    window.getSelection()?.removeAllRanges()
    document.body.classList.add('is-resizing-sidebar')
    setIsResizingSidebar(true)
  }, [])

  useEffect(() => {
    if (!isResizingSidebar) {
      return
    }

    const onMouseMove = (event: globalThis.MouseEvent) => {
      const nextWidth = Math.min(360, Math.max(190, event.clientX))
      setSidebarWidth(
        Math.abs(nextWidth - DEFAULT_SIDEBAR_WIDTH) <= SIDEBAR_SNAP_THRESHOLD ? DEFAULT_SIDEBAR_WIDTH : nextWidth
      )
    }

    const onMouseUp = () => {
      window.getSelection()?.removeAllRanges()
      setIsResizingSidebar(false)
    }

    window.addEventListener('mousemove', onMouseMove)
    window.addEventListener('mouseup', onMouseUp)
    window.addEventListener('blur', onMouseUp)
    document.body.classList.add('is-resizing-sidebar')
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'

    return () => {
      window.removeEventListener('mousemove', onMouseMove)
      window.removeEventListener('mouseup', onMouseUp)
      window.removeEventListener('blur', onMouseUp)
      document.body.classList.remove('is-resizing-sidebar')
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }
  }, [isResizingSidebar])

  // Timeout for error / status bar
  useEffect(() => {
    if (!error) {
      return
    }

    const timeoutId = window.setTimeout(() => {
      setError((current) => (current === error ? null : current))
    }, STATUS_MESSAGE_TIMEOUT_MS)

    return () => {
      window.clearTimeout(timeoutId)
    }
  }, [error])

  // Bridge synchronization close window requests to modals
  useEffect(() => {
    if (!windowCloseRequest) {
      return
    }
    const hasActive = workspace.tabs.some((tab) =>
      Boolean(tab && (tab.status === 'connecting' || tab.status === 'connected'))
    )
    requestWindowCloseConfirmation(windowCloseRequest.isQuit, hasActive)
    clearWindowCloseRequest()
  }, [windowCloseRequest, workspace.tabs, requestWindowCloseConfirmation, clearWindowCloseRequest])

  const normalizeErrorMessage = (err: unknown) => {
    const rawMessage = err instanceof Error ? err.message : String(err)
    return rawMessage.replace(REMOTE_METHOD_ERROR_PREFIX, '').trim()
  }

  const formatAppError = (scope: string, err: unknown, details?: ErrorDetails) => {
    const message = normalizeErrorMessage(err)
    const displayScope = localizeErrorScope(scope, locale)
    const likelyDisconnectedSession =
      /会话已断开|session disconnected|session not found|remote connection closed|connection closed/i.test(message)
    const likelyConcurrentRequestIssue =
      /another one is still running|forgot to use 'await'|client is closed because user launched a task/i.test(message)
    const likelyPathIssue = /can't cd to|__NOT_DIR__|no such file|not a directory|permission denied|\b550\b/i.test(
      message
    )
    const metadata = details?.item
      ? ` (${t.permission}: ${details.item.permission || '-'}, ${t.ownerGroup}: ${details.item.ownerGroup || '-'})`
      : ''
    const pathText = details?.targetPath ? ` ${details.targetPath}` : ''

    if (likelyDisconnectedSession) {
      return t.remoteSessionDisconnectedAction
    }

    if (locale === 'zhCN') {
      if (details?.targetPath && likelyConcurrentRequestIssue) {
        return `打开远程目录${pathText}${metadata}失败：远程连接正在处理另一项请求，请稍后重试。原始错误：${message}`
      }
      if (details?.targetPath && likelyPathIssue) {
        return `无法打开远程目录${pathText}${metadata}。可能是目录不存在、不是目录，或者当前账号没有进入权限。原始错误：${message}`
      }
      return `${scope}${pathText}${metadata}失败：${message}`
    }

    if (details?.targetPath && likelyConcurrentRequestIssue) {
      return `Failed to open remote directory${pathText}${metadata}: the remote connection is still processing another request. Raw error: ${message}`
    }
    if (details?.targetPath && likelyPathIssue) {
      return `Could not open remote directory${pathText}${metadata}. It may not exist, may not be a directory, or your account may not have permission to make changes. Raw error: ${message}`
    }

    return `${displayScope}${pathText}${metadata} failed: ${message}`
  }

  const reportError = (setter: (message: string) => void, scope: string, err: unknown, details?: ErrorDetails) => {
    console.error(`[FileTerm] ${scope}`, err)
    setter(formatAppError(scope, err, details))
  }

  // 6. Workspace Data Operations Hook
  const {
    saveCommandTemplate,
    createCommandFolder,
    updateCommandFolder,
    updateCommandOrder,
    deleteCommandFolder,
    deleteCommandTemplate,
    createConnectionFolder,
    updateConnectionFolder,
    deleteConnectionFolder,
    updateConnectionOrder
  } = useWorkspaceDataOps({
    desktopApi: desktopApi ?? null,
    isCommandFormWindow,
    onApplySnapshot: applySnapshot,
    onBusyChange: setIsBusy,
    onError: (scope, err) => reportError(setError, scope, err),
    onCloseCurrentWindow: closeCurrentWindow
  })

  const handleSaveProfile = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (isBusy || profileSaveInFlightRef.current) {
      return
    }

    const normalizedHost = normalizeConnectionHost(form.host)
    const requiresHost = form.type !== 'serial'
    const requiresRemotePath = form.type === 'ssh' || form.type === 'ftp'

    if (
      !form.name ||
      !form.group ||
      (requiresHost && !normalizedHost) ||
      (requiresRemotePath && !form.remotePath) ||
      (form.type === 'serial' && !form.devicePath?.trim())
    ) {
      setFormError(t.fillRequired)
      return
    }

    if (requiresHost && !validateConnectionHost(normalizedHost).valid) {
      setFormError(t.invalidHost)
      return
    }

    if (form.type === 'ssh' && form.authType === 'privateKey' && !form.privateKeyId && !form.privateKeyPath) {
      setFormError(t.missingPrivateKeyPath)
      return
    }

    if (!desktopApi) {
      setFormError(t.desktopOnlyCreate)
      return
    }

    try {
      profileSaveInFlightRef.current = true
      setIsBusy(true)
      const defaultPort = form.type === 'ftp' ? 21 : form.type === 'telnet' ? 23 : form.type === 'serial' ? 0 : 22
      const finalPort = Number(form.port) || defaultPort
      const payload = { ...form, host: normalizedHost, port: finalPort }
      const snapshot = editingProfileId
        ? await desktopApi.updateProfile(editingProfileId, payload)
        : await desktopApi.createProfile(payload)
      applySnapshot(snapshot)
      if (isConnectionFormWindow) {
        closeCurrentWindow()
        return
      }
      closeConnectionForm()
    } catch (err) {
      reportError(setFormError, '保存连接', err)
    } finally {
      profileSaveInFlightRef.current = false
      setIsBusy(false)
    }
  }

  const handleDeleteProfile = async (profileId: string) => {
    if (!desktopApi) {
      setError(t.desktopOnlyDelete)
      return false
    }

    try {
      setIsBusy(true)
      const snapshot = await desktopApi.deleteProfile(profileId)
      applySnapshot(snapshot)
      return true
    } catch (err) {
      reportError(setError, '删除连接', err)
      return false
    } finally {
      setIsBusy(false)
    }
  }

  const handleClearHostFingerprint = async (profile: ConnectionProfile) => {
    if (!desktopApi || profile.type !== 'ssh') {
      return
    }

    try {
      setIsBusy(true)
      const nextInput: CreateProfileInput = {
        ...profileToForm(profile),
        trustedHostFingerprint: ''
      }
      const snapshot = await desktopApi.updateProfile(profile.id, nextInput)
      applySnapshot(snapshot)
      setError(null)
    } catch (err) {
      reportError(setError, '清除主机指纹', err)
    } finally {
      setIsBusy(false)
    }
  }

  const executeCommandTemplate = async (
    commandId: string,
    args: string[],
    options: CommandExecutionOptions,
    scope: SendScope,
    selectedTabIds: string[]
  ) => {
    if (!desktopApi) {
      return
    }

    try {
      setIsBusy(true)
      const targetIds = resolveSelectedTabIds(scope, activeTab, selectedTabIds, sessionSendTargets)
      const targetTabs = visibleWorkspaceTabs.filter((tab) => targetIds.includes(tab.id))

      // 并行发送，对照 Electron 原版的 fire-and-forget 行为。
      // 顺序 await 会让一个卡住的 tab 阻塞后续所有 tab；allSettled 确保
      // 单个失败不影响其余，后端 send 超时保证 invoke 不会永久 hang。
      const results = await Promise.allSettled(
        targetTabs.map((tab) => desktopApi.executeCommandTemplate(tab.id, commandId, args, options))
      )
      const failure = settledResultsError('执行命令模板', results)
      if (failure) {
        throw failure
      }
    } catch (err) {
      reportError(setError, '执行命令模板', err)
    } finally {
      setIsBusy(false)
    }
  }

  const openLogsDirectory = () => {
    if (!desktopApi) {
      setError(t.desktopOnlyOpenLogs)
      return
    }
    void desktopApi.openLogsDirectory().catch((err) => {
      reportError(setError, t.openLogsDirectory, err)
    })
  }

  const fileActionProps = useMemo<FileActionModalBinding>(() => {
    if (!fileActionDialog) {
      return null
    }
    if (fileActionDialog.kind === 'delete') {
      return {
        kind: 'delete',
        props: {
          confirmLabel: t.delete,
          description:
            fileActionDialog.targets.length > 1
              ? `${t.deleteConfirmPrefix}${fileActionDialog.targets.length} ${t.itemsSuffix}${t.deleteConfirmSuffix}`
              : `${t.deleteConfirmPrefix}${fileActionDialog.targets[0]?.name ?? ''}${t.deleteConfirmSuffix}`,
          errorMessage: fileActionError,
          isSubmitting: isFileActionSubmitting,
          onClose: dismissFileActionDialog,
          onConfirm: () => {
            void handleSubmitFileAction('')
          },
          title: t.delete
        }
      }
    }
    return {
      kind: 'action',
      props: {
        confirmLabel: t.confirm,
        errorMessage: fileActionError,
        hint: fileActionDialog.kind === 'new-file' ? t.newFileExtensionHint : undefined,
        initialValue: fileActionDialog.kind === 'rename' ? fileActionDialog.target.name : '',
        isSubmitting: isFileActionSubmitting,
        inputLabel: t.fileName,
        inputPlaceholder: fileActionDialog.kind === 'new-folder' ? t.folderName : t.fileName,
        onClose: dismissFileActionDialog,
        onConfirm: (val) => {
          void handleSubmitFileAction(val)
        },
        title:
          fileActionDialog.kind === 'new-folder'
            ? t.newFolder
            : fileActionDialog.kind === 'new-file'
              ? t.newFile
              : t.rename
      }
    }
  }, [fileActionDialog, fileActionError, isFileActionSubmitting, handleSubmitFileAction, dismissFileActionDialog])

  const windowCloseConfirmProps = windowCloseConfirm
    ? {
        confirmLabel: t.closeConfirmQuit,
        confirmVariant: 'danger' as const,
        description: (
          <>
            {windowCloseConfirm.hasActiveConnections ? (
              <div className="confirm-action-dialog__warning">{t.closeConfirmActiveWarn}</div>
            ) : windowCloseConfirm.isQuit ? (
              <div>{t.closeConfirmQuitMsg}</div>
            ) : null}
            {!windowCloseConfirm.isQuit ? <div>{t.closeConfirmWindowsMsg}</div> : null}
          </>
        ),
        extraActions: !windowCloseConfirm.isQuit ? (
          <button
            className="confirm-action-dialog__button confirm-action-dialog__button--primary"
            onClick={() => resolveWindowCloseConfirmation('hide')}
            type="button"
          >
            {t.closeConfirmHide}
          </button>
        ) : null,
        onClose: () => resolveWindowCloseConfirmation('cancel'),
        onConfirm: () => resolveWindowCloseConfirmation('quit'),
        title: t.closeConfirmTitle
      }
    : null

  // --- Multi-window Standalone Render Blocks ---

  if (isConnectionManagerWindow) {
    return (
      <>
        <StandaloneWindowFrame isWindows={isWindowsDesktop} showPlatformTitlebar={false} title={t.connectionManager}>
          <ConnectionManagerModal
            profiles={workspace.profiles}
            folders={workspace.folders || []}
            standalone
            onClose={closeCurrentWindow}
            onCreate={openCreateConnection}
            onDeleteProfile={handleDeleteProfile}
            onEditProfile={openEditConnection}
            onOpenProfile={(profileId) => {
              if (desktopApi) {
                void desktopApi.openProfileFromManager(profileId).catch((err: Error) => {
                  reportError(setError, '从管理器打开连接', err)
                })
                return
              }
              void openProfile(profileId)
            }}
            onCreateFolder={createConnectionFolder}
            onDeleteFolder={deleteConnectionFolder}
            onUpdateFolder={updateConnectionFolder}
            onUpdateOrder={updateConnectionOrder}
            onImportConnections={openConnectionImportPreview}
            onExportConnections={() => {
              const request = desktopApi?.exportConnections('fileterm')
              void request?.catch((error) => reportError(setError, '导出连接', error))
            }}
          />
          {showConnectionForm ? (
            <ConnectionModal
              errorMessage={formError}
              groupOptions={connectionGroupOptions}
              mode={editingProfileId ? 'edit' : 'create'}
              form={form}
              hasSavedPassword={
                editingProfileId
                  ? workspace.profiles.find((profile) => profile.id === editingProfileId)?.hasSavedPassword === true
                  : false
              }
              isSubmitting={isBusy}
              setForm={updateForm}
              onClearHostFingerprint={() => {
                const editingProfile = editingProfileId
                  ? (workspace.profiles.find((profile) => profile.id === editingProfileId) ?? null)
                  : null
                if (editingProfile) {
                  void handleClearHostFingerprint(editingProfile)
                  setForm((prev) => ({ ...prev, trustedHostFingerprint: '' }))
                }
              }}
              onSubmit={handleSaveProfile}
              onClose={closeConnectionForm}
            />
          ) : null}
        </StandaloneWindowFrame>
        {connectionImportPlan ? (
          <ConnectionImportPreviewModal
            plan={connectionImportPlan}
            onClose={() => setConnectionImportPlan(null)}
            onCommit={commitConnectionJsonPreview}
          />
        ) : null}
      </>
    )
  }

  if (isCommandManagerWindow) {
    return (
      <StandaloneWindowFrame isWindows={isWindowsDesktop} showPlatformTitlebar={false} title={t.commandManager}>
        <CommandManagerModal
          commandFolders={workspace.commandFolders || []}
          commandTemplates={workspace.commandTemplates || []}
          standalone
          onClose={closeCurrentWindow}
          onCreateFolder={createCommandFolder}
          onDeleteFolder={deleteCommandFolder}
          onUpdateFolder={updateCommandFolder}
          onUpdateOrder={updateCommandOrder}
          onCreateCommand={(input) => saveCommandTemplate(null, input)}
          onUpdateCommand={(commandId, input) => saveCommandTemplate(commandId, input)}
          onDeleteCommand={deleteCommandTemplate}
        />
      </StandaloneWindowFrame>
    )
  }

  if (isCommandFormWindow) {
    const editingCommand =
      formWindowMode === 'edit'
        ? (workspace.commandTemplates.find((item) => item.id === formWindowCommandId) ?? null)
        : null

    return (
      <StandaloneWindowFrame
        isWindows={isWindowsDesktop}
        showPlatformTitlebar={false}
        title={editingCommand ? t.commandEdit : t.commandCreate}
      >
        <CommandEditorModal
          folders={workspace.commandFolders || []}
          initialValue={
            editingCommand
              ? toCommandTemplateInput(editingCommand)
              : {
                  ...emptyCommandForm,
                  parentId: formWindowFolderId || undefined
                }
          }
          mode={editingCommand ? 'edit' : formWindowMode}
          isSubmitting={isBusy}
          standalone
          onClose={closeCurrentWindow}
          onSubmit={(input) => saveCommandTemplate(editingCommand?.id ?? null, input)}
        />
      </StandaloneWindowFrame>
    )
  }

  if (isConnectionFormWindow) {
    return (
      <StandaloneWindowFrame
        isWindows={isWindowsDesktop}
        showPlatformTitlebar={false}
        title={editingProfileId ? t.editConnection : t.newConnection}
      >
        <ConnectionFormHost
          editingProfileId={editingProfileId}
          errorMessage={formError}
          groupOptions={connectionGroupOptions}
          mode={editingProfileId ? 'edit' : formWindowMode}
          form={form}
          isSubmitting={isBusy}
          profiles={workspace.profiles}
          setForm={updateForm}
          onClearHostFingerprint={(profile) => {
            void handleClearHostFingerprint(profile)
          }}
          standalone
          onSubmit={handleSaveProfile}
          onClose={closeCurrentWindow}
        />
      </StandaloneWindowFrame>
    )
  }

  if (isFileEditorWindow && fileEditor) {
    return (
      <StandaloneWindowFrame isWindows={isWindowsDesktop} showPlatformTitlebar={false} title={fileEditor.name}>
        <Suspense fallback={<div className="standalone-shell file-editor-window">{t.updating}</div>}>
          <FileEditorModal
            errorMessage={fileEditorError}
            file={fileEditor}
            isBusy={isFileEditorBusy}
            isDirty={isFileEditorDirty}
            isSaving={isFileEditorSaving}
            onClose={requestFileEditorClose}
            onDraftChange={checkFileEditorDirty}
            onReloadWithEncoding={(encoding) => {
              void reloadFileEditorWithEncoding(encoding)
            }}
            onSave={saveFileEditor}
            standalone
            themeMode={themeMode}
          />
        </Suspense>
        {isFileEditorDiscardConfirmOpen ? (
          <ConfirmActionDialog
            confirmLabel={t.fileEditorDiscardChanges}
            description={t.fileEditorDiscardChangesDescription}
            onClose={cancelFileEditorDiscard}
            onConfirm={confirmFileEditorDiscard}
            title={t.fileEditorDiscardChangesTitle}
          />
        ) : null}
      </StandaloneWindowFrame>
    )
  }

  if (isFileEditorWindow) {
    return (
      <StandaloneWindowFrame
        isWindows={isWindowsDesktop}
        showPlatformTitlebar={false}
        title={fileEditorWindowName ?? t.appTitle}
      >
        <div className="standalone-shell file-editor-window">
          <div
            className={`modal-card file-editor-modal ${themeMode === 'default-dark' ? 'file-editor-modal--dark' : ''} standalone`}
          >
            <div className="modal-header" data-tauri-drag-region="deep">
              <div className="file-editor-title">
                <span>{fileEditorWindowSource === 'remote' ? t.editRemoteFile : t.editLocalFile}</span>
                <strong>{fileEditorWindowName ?? ''}</strong>
              </div>
              <div className="file-editor-header-actions">
                <CloseButton onClick={closeCurrentWindow} />
              </div>
            </div>
            {fileEditorError ? (
              <div className="modal-error">{fileEditorError}</div>
            ) : (
              <div className="file-editor-path">{t.updating}</div>
            )}
          </div>
        </div>
      </StandaloneWindowFrame>
    )
  }

  // --- Main Workspace Render ---

  const resolvedSidebarWidth = isSystemSidebarCollapsed ? 44 : sidebarWidth
  const brandWidth = DEFAULT_SIDEBAR_WIDTH

  const tabBarProps: Omit<TabBarProps, 'homeBrandContent'> = {
    activeHomeTabId: effectiveActiveLocalTabId,
    activeSessionTabId: visibleActiveSessionTabId,
    isWorkspaceFocusMode,
    onAddHomeTab: addHomeTab,
    onActivateHome: activateHomeTab,
    onActivateSession: (tabId: string) => {
      void activateSessionTab(tabId)
    },
    onCloseHomeTab: closeHomeTab,
    onCloseSessionTab: (event: React.MouseEvent<HTMLButtonElement>, tabId: string) => {
      void closeSessionTab(event, tabId)
    },
    onDragEnd: endTabDrag,
    onDragEnter: enterDraggedTab,
    onDragStart: startTabDrag,
    onOpenSettings: () => setShowSettings(true),
    onToggleWindowMaximize: () => {
      void desktopApi?.toggleMaximizeCurrentWindow()
    },
    onOpenTabContext: (event: React.MouseEvent<HTMLDivElement>, target: TabContextTarget) => {
      openTabContextMenu(event, target)
    },
    onToggleWorkspaceFocus: () => {
      if (!activeWorkspaceFocusKey) {
        return
      }
      const nextFocusMode = !isWorkspaceFocusMode
      setWorkspaceFocusModes((currentModes) => ({
        ...currentModes,
        [activeWorkspaceFocusKey]: nextFocusMode
      }))
      if (!nextFocusMode) {
        setSidebarWidth(214)
      }
    },
    orderedTabs
  }

  return (
    <>
      <div
        className={`fs-shell ${isWindowsDesktop ? 'has-window-menubar' : ''} ${isMaximized ? 'is-window-maximized' : ''} ${isHomeWorkspaceVisible ? 'is-home-active' : ''} ${isSystemSidebarCollapsed ? 'is-sidebar-collapsed' : ''} ${isResizingSidebar ? 'is-resizing-sidebar' : ''}`}
        style={
          {
            '--sidebar-width': `${resolvedSidebarWidth}px`,
            '--brand-width': `${brandWidth}px`
          } as CSSProperties
        }
      >
        {isWindowsDesktop ? <WindowMenubar desktopApi={desktopApi} isMaximized={isMaximized} /> : null}
        {!isHomeWorkspaceVisible && <TabBar {...tabBarProps} />}

        {showSidebar ? (
          <SystemSidebarShell
            activeProfile={activeProfile}
            activeSession={activeSession}
            collapsed={isSystemSidebarCollapsed}
            showResourceMeters={isResourceMonitoringAvailable}
            isResizing={isResizingSidebar}
            onOpenSystemInfo={openSystemInfo}
            onResizeStart={startSidebarResize}
            onRestoreWidth={() => setSidebarWidth(214)}
            onToggleCollapsed={setIsSystemSidebarCollapsed}
          />
        ) : null}

        <main className={`fs-main ${error ? 'has-status' : 'no-status'} ${showSidebar ? '' : 'full-width'}`}>
          {error ? (
            <div className="status-message" role="alert">
              <span className="status-message-text">{error}</span>
              <CloseButton aria-label={t.closeTab} onClick={() => setError(null)} size="compact" />
            </div>
          ) : null}
          <div className="workspace-stage">
            <div
              key={activeLocalTab ? activeWorkspaceOrderKey : 'session-workspace'}
              className={`workspace-stage-transition ${isWorkspaceTransitionActive ? 'is-transitioning' : ''}`}
              data-nav-direction={workspaceNavDirection}
            >
              <WorkspaceStage
                activeLocalTab={activeLocalTab}
                activeHomeTabId={effectiveActiveLocalTabId}
                activeProfile={activeProfile}
                activeSession={activeSession}
                activeTab={activeTab}
                filePanelHeight={activeFilePanelHeight}
                onFilePanelHeightChange={setActiveFilePanelHeight}
                shouldAlignFilePanelOnMount={shouldAlignFilePanelOnMount}
                sendTargets={sessionSendTargets}
                terminalDockSendScope={activeTerminalDockSendState.scope}
                terminalDockSelectedTabIds={activeTerminalDockSendState.selectedTabIds}
                commandFolders={workspace.commandFolders || []}
                commandTemplates={workspace.commandTemplates || []}
                folders={workspace.folders || []}
                isBusy={isBusy}
                localItems={localItems}
                localPath={localPath}
                localPanePath={localPanePath}
                isLocalNetworkShare={isLocalNetworkShare}
                isLocalDirectoryLoading={isLocalDirectoryLoading}
                isWorkspaceRefreshing={isWorkspaceRefreshing}
                canPasteToLocal={canPasteIntoLocal}
                canPasteToRemote={canPasteIntoRemote}
                clipboardStatusText={clipboardStatusText}
                localCutPaths={localCutPaths}
                remoteCutPaths={remoteCutPaths}
                onCopyItems={copyItems}
                onCutItems={cutItems}
                onClearCutState={clearCutState}
                onExecuteCommand={(commandId, args, options, scope, selectedTabIds) => {
                  void executeCommandTemplate(commandId, args, options, scope, selectedTabIds)
                }}
                onSendTerminalCommand={sendTerminalCommand}
                onTerminalDockSendScopeChange={(scope, rememberSelection) => {
                  updateTerminalDockSendScope(scope, rememberSelection)
                }}
                onTerminalDockSelectedTabIdsChange={(selectedTabIds, rememberSelection) => {
                  updateTerminalDockSelectedTabIds(selectedTabIds, rememberSelection)
                }}
                onOpenCommandManager={openCommandManager}
                profiles={workspace.profiles}
                onChooseUploadFiles={handleChooseUploadFiles}
                onDownloadFiles={handleDownloadFiles}
                onDownloadLocalNetworkFiles={handleDownloadLocalNetworkFiles}
                onDropUpload={handleDropUpload}
                onOpenLocalItem={handleOpenLocalItem}
                onOpenLocalPath={handleOpenLocalPath}
                onOpenProfile={openProfile}
                onOpenRemoteItem={handleOpenRemoteItem}
                onOpenRemotePath={handleOpenRemotePath}
                onPasteIntoPane={handlePasteIntoPane}
                onRequestChangePermissions={requestChangePermissions}
                onRequestDelete={requestDelete}
                onRequestNewFile={requestNewFile}
                onRequestNewFolder={requestNewFolder}
                onRequestQuickDelete={handleQuickDelete}
                onRequestRename={requestRename}
                onToggleFollowShellCwd={handleToggleFollowShellCwd}
                onToggleRemoteFileAccessMode={handleToggleRemoteFileAccessMode}
                remoteFileAccessMode={remoteFileAccessMode}
                isRemoteDirectoryLoading={isRemoteDirectoryLoading}
                onRefresh={handleRefreshWorkspace}
                onUploadFiles={handleUploadFiles}
                theme={themeMode}
                locale={locale}
                onCreateConnection={() => {
                  if (desktopApi) void desktopApi.openConnectionFormWindow('create')
                }}
                onEditConnection={openEditConnection}
                onDeleteConnection={handleDeleteProfile}
                onCreateConnectionFolder={createConnectionFolder}
                onDeleteConnectionFolder={deleteConnectionFolder}
                onUpdateConnectionFolder={updateConnectionFolder}
                onUpdateConnectionOrder={updateConnectionOrder}
                onImportConnections={openConnectionImportPreview}
                onExportConnections={() => {
                  const request = desktopApi?.exportConnections('fileterm')
                  void request?.then(() => undefined)
                }}
                onCreateCommand={(input) => saveCommandTemplate(null, input)}
                onUpdateCommand={saveCommandTemplate}
                onDeleteCommand={deleteCommandTemplate}
                onCreateCommandFolder={createCommandFolder}
                onDeleteCommandFolder={deleteCommandFolder}
                onUpdateCommandFolder={updateCommandFolder}
                onUpdateCommandOrder={updateCommandOrder}
                onSetTheme={setThemeMode}
                onSetLocale={(nextLocale) => {
                  setLocale(nextLocale)
                  setLocaleState(nextLocale)
                }}
                onOpenLogsDirectory={openLogsDirectory}
                isSidebarCollapsed={isSystemSidebarCollapsed}
                isWorkspaceFocusMode={isWorkspaceFocusMode}
                tabBarProps={tabBarProps}
                isResizingSidebar={isResizingSidebar}
                onResizeStart={startSidebarResize}
              />
            </div>
          </div>
        </main>

        <TransferCenterHost
          activeProfileId={activeTab?.profileId}
          activeTabId={activeTab?.id ?? null}
          desktopApi={desktopApi}
          fullWidth={!showSidebar}
          isPending={isBusy}
          onApplySnapshot={applySnapshot}
          onError={(scope, err) => reportError(setError, scope, err)}
          sessionTabs={visibleWorkspaceTabs}
          transfers={workspace.transfers}
          visible={!isHomeWorkspaceVisible}
        />
      </div>

      {connectionImportPlan ? (
        <ConnectionImportPreviewModal
          plan={connectionImportPlan}
          onClose={() => setConnectionImportPlan(null)}
          onCommit={commitConnectionJsonPreview}
        />
      ) : null}

      <ModalPortalManager
        commandManager={
          showCommandManager
            ? {
                commandFolders: workspace.commandFolders || [],
                commandTemplates: workspace.commandTemplates || [],
                onClose: () => setShowCommandManager(false),
                onCreateFolder: createCommandFolder,
                onDeleteFolder: deleteCommandFolder,
                onUpdateFolder: updateCommandFolder,
                onUpdateOrder: updateCommandOrder,
                onCreateCommand: (input) => saveCommandTemplate(null, input),
                onUpdateCommand: (commandId, input) => saveCommandTemplate(commandId, input),
                onDeleteCommand: deleteCommandTemplate
              }
            : null
        }
        connectionForm={
          showConnectionForm
            ? {
                editingProfileId,
                errorMessage: formError,
                groupOptions: connectionGroupOptions,
                mode: editingProfileId ? 'edit' : 'create',
                form,
                isSubmitting: isBusy,
                profiles: workspace.profiles,
                setForm: updateForm,
                onClearHostFingerprint: (profile) => {
                  void handleClearHostFingerprint(profile)
                },
                onSubmit: handleSaveProfile,
                onClose: closeConnectionForm
              }
            : null
        }
        connectionManager={
          showConnectionManager
            ? {
                profiles: workspace.profiles,
                folders: workspace.folders || [],
                onClose: () => setShowConnectionManager(false),
                onCreate: () => {
                  setShowConnectionManager(false)
                  openCreateConnection()
                },
                onDeleteProfile: handleDeleteProfile,
                onEditProfile: (profile) => {
                  setShowConnectionManager(false)
                  openEditConnection(profile)
                },
                onOpenProfile: (profileId) => {
                  setShowConnectionManager(false)
                  void openProfile(profileId)
                },
                onCreateFolder: createConnectionFolder,
                onDeleteFolder: deleteConnectionFolder,
                onUpdateFolder: updateConnectionFolder,
                onUpdateOrder: updateConnectionOrder,
                onImportConnections: openConnectionImportPreview,
                onExportConnections: () => {
                  const request = desktopApi?.exportConnections('fileterm')
                  void request?.catch((error) => reportError(setError, '导出连接', error))
                }
              }
            : null
        }
        fileAction={fileActionProps}
        fileEditor={
          fileEditor
            ? {
                errorMessage: fileEditorError,
                file: fileEditor,
                isBusy: isFileEditorBusy,
                isDirty: isFileEditorDirty,
                isSaving: isFileEditorSaving,
                onClose: closeFileEditor,
                onDraftChange: checkFileEditorDirty,
                onReloadWithEncoding: (encoding) => {
                  void reloadFileEditorWithEncoding(encoding)
                },
                onSave: saveFileEditor,
                themeMode
              }
            : null
        }
        filePermission={
          permissionDialog
            ? {
                errorMessage: permissionDialogError,
                fileName: permissionDialog.target.name,
                fileType: permissionDialog.target.type,
                initialPermission: permissionDialog.target.permission,
                isSubmitting: isPermissionSubmitting,
                onClose: dismissPermissionDialog,
                onSubmit: (options) => {
                  void handleSubmitPermissions(options)
                },
                ownerGroup: permissionDialog.target.ownerGroup,
                supportsRecursive: permissionDialog.supportsRecursive,
                targetPath: permissionDialog.target.path
              }
            : null
        }
        rootAccess={
          rootAccessDialog
            ? {
                defaultSshUser: rootAccessDialog.sshUser,
                defaultSudoUser: rootAccessDialog.sudoUser,
                errorMessage: rootAccessDialogError,
                isSubmitting: isRootAccessSubmitting,
                onClose: dismissRootAccessDialog,
                onSubmit: handleConfirmRootAccess
              }
            : null
        }
        smbCredentials={
          localNetworkCredentialsDialog
            ? {
                errorMessage: localNetworkCredentialsDialogError,
                isSubmitting: isLocalNetworkCredentialsSubmitting,
                path: localNetworkCredentialsDialog.path,
                onCancel: dismissLocalNetworkCredentialsDialog,
                onSubmit: handleSubmitLocalNetworkCredentials
              }
            : null
        }
        smbSharePicker={
          localNetworkShareDialog
            ? {
                errorMessage: localNetworkShareDialogError,
                isSubmitting: isLocalNetworkCredentialsSubmitting,
                path: localNetworkShareDialog.path,
                shares: localNetworkShareDialog.shares,
                onCancel: dismissLocalNetworkShareDialog,
                onChangeCredentials: changeLocalNetworkCredentials,
                onSubmit: handleSubmitLocalNetworkShare
              }
            : null
        }
        settings={
          showSettings
            ? {
                theme: themeMode,
                onSetTheme: setThemeMode,
                locale,
                onSetLocale: (nextLocale) => {
                  setLocale(nextLocale)
                  setLocaleState(nextLocale)
                },
                onOpenCommandManager: openCommandManagerFromSettings,
                onOpenConnectionManager: openConnectionManagerFromSettings,
                onOpenLogsDirectory: () => {
                  openLogsDirectory()
                },
                onClose: () => setShowSettings(false)
              }
            : null
        }
        shortcutCloseConfirm={
          shortcutCloseConfirm
            ? {
                confirmLabel: t.closeShortcutCloseTab,
                description: (shortcutCloseConfirm.variant === 'connecting'
                  ? t.closeShortcutConnectingDescription
                  : shortcutCloseConfirm.variant === 'active-session'
                    ? t.closeShortcutActiveDescription
                    : t.closeShortcutLastActiveDescription
                ).replace('{name}', shortcutCloseConfirm.title),
                isSubmitting: isBusy,
                onClose: dismissShortcutCloseConfirm,
                onConfirm: () => {
                  void confirmShortcutClose()
                },
                title:
                  shortcutCloseConfirm.variant === 'connecting'
                    ? t.closeShortcutConnectingTitle
                    : shortcutCloseConfirm.variant === 'active-session'
                      ? t.closeShortcutActiveTitle
                      : t.closeShortcutLastActiveTitle
              }
            : null
        }
        sshCredentials={
          credentialsRequest
            ? {
                errorMessage: sshInteractionError,
                isSubmitting: isSshInteractionResolving,
                request: credentialsRequest,
                onCancel: cancelCredentials,
                onSubmit: submitCredentials
              }
            : null
        }
        sshHostVerification={
          hostVerificationRequest
            ? {
                request: hostVerificationRequest,
                isSubmitting: isSshInteractionResolving,
                onReject: rejectHost,
                onAcceptOnce: acceptHostOnce,
                onAcceptAndSave: acceptHostAndSave
              }
            : null
        }
        sshKeyPassphrase={
          keyPassphraseRequest
            ? {
                errorMessage: sshInteractionError,
                isSubmitting: isSshInteractionResolving,
                request: keyPassphraseRequest,
                onCancel: cancelKeyPassphrase,
                onSubmit: submitKeyPassphrase
              }
            : null
        }
        sshKeyboardInteractive={
          keyboardInteractiveRequest
            ? {
                request: keyboardInteractiveRequest,
                errorMessage: sshInteractionError,
                isSubmitting: isSshInteractionResolving,
                onCancel: () => {
                  void cancelKeyboardInteractive()
                },
                onSubmit: (answers) => {
                  void submitKeyboardInteractive(answers)
                }
              }
            : null
        }
        tabContextMenu={
          tabContextMenu
            ? {
                canConnectAll: visibleWorkspaceTabs.some(
                  (tab) => tab.status !== 'connected' && tab.status !== 'connecting'
                ),
                canCloseAll: localTabs.length + visibleWorkspaceTabs.length > 0,
                canCloseCurrent:
                  tabContextMenu.target.kind === 'session' ? true : localTabs.length + visibleWorkspaceTabs.length > 1,
                canCloseOthers: localTabs.length + visibleWorkspaceTabs.length > 1,
                isSessionTab: tabContextMenu.target.kind === 'session',
                onAction: (action) => {
                  void handleTabContextAction(action)
                },
                onClose: closeTabContextMenu,
                position: { x: tabContextMenu.x, y: tabContextMenu.y },
                tabStatus: tabContextMenu.target.kind === 'session' ? tabContextMenu.target.status : null
              }
            : null
        }
        windowCloseConfirm={windowCloseConfirmProps}
      />
    </>
  )
}
