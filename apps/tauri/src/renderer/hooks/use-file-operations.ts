import { useEffect, useRef, useState } from 'react'
import { APP_EVENT, onAppEvent } from '../lib/app-events'
import { t } from '../i18n'
import type {
  FileActionDialog,
  FileClipboardState,
  FileOperationErrorDetails,
  FileOperationsRuntime,
  LocalNetworkCredentialsDialogState,
  LocalNetworkShareDialogState,
  LocalNetworkShareSource,
  PermissionDialogState,
  RootAccessDialogState,
  UseFileOperationsOptions
} from './file-operations-types'
import { createFileOperationsActions } from './file-operations-actions'
import { createFileOperationsNavigation } from './file-operations-navigation'
import { useFileOperationsTransfers } from './file-operations-transfers'
import { getNetworkShareDisplayPath } from './file-operations-utils'

export * from './file-operations-types'
export * from './file-operations-utils'

export function useFileOperations({
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
  onApplySnapshot,
  onBusyChange,
  onStatusMessage,
  formatError,
  openLocalFile,
  openRemoteFile
}: UseFileOperationsOptions) {
  const [remoteDirectoryLoadingTabId, setRemoteDirectoryLoadingTabId] = useState<string | null>(null)
  const [fileActionDialog, setFileActionDialog] = useState<FileActionDialog | null>(null)
  const [fileActionError, setFileActionError] = useState<string | null>(null)
  const [isFileActionSubmitting, setIsFileActionSubmitting] = useState(false)
  const fileActionSubmittingRef = useRef(false)
  const [fileClipboard, setFileClipboard] = useState<FileClipboardState | null>(null)
  const [permissionDialog, setPermissionDialog] = useState<PermissionDialogState | null>(null)
  const [permissionDialogError, setPermissionDialogError] = useState<string | null>(null)
  const [isPermissionSubmitting, setIsPermissionSubmitting] = useState(false)
  const permissionSubmittingRef = useRef(false)
  const [rootAccessDialog, setRootAccessDialog] = useState<RootAccessDialogState | null>(null)
  const [rootAccessDialogError, setRootAccessDialogError] = useState<string | null>(null)
  const [isRootAccessSubmitting, setIsRootAccessSubmitting] = useState(false)
  const rootAccessSubmittingRef = useRef(false)
  const [localNetworkCredentialsDialog, setLocalNetworkCredentialsDialog] =
    useState<LocalNetworkCredentialsDialogState | null>(null)
  const [localNetworkCredentialsDialogError, setLocalNetworkCredentialsDialogError] = useState<string | null>(null)
  const [localNetworkShareDialog, setLocalNetworkShareDialog] = useState<LocalNetworkShareDialogState | null>(null)
  const [localNetworkShareDialogError, setLocalNetworkShareDialogError] = useState<string | null>(null)
  const [localNetworkShareSource, setLocalNetworkShareSource] = useState<LocalNetworkShareSource | null>(null)
  const [isWorkspaceRefreshing, setIsWorkspaceRefreshing] = useState(false)
  const [isLocalNetworkCredentialsSubmitting, setIsLocalNetworkCredentialsSubmitting] = useState(false)
  const localNetworkCredentialsSubmittingRef = useRef(false)
  const nativeRemoteDropTargetAtRef = useRef(0)
  const nativeDropConsumedAtRef = useRef(0)

  useEffect(() => {
    const markRemoteDropTarget = () => {
      nativeRemoteDropTargetAtRef.current = Date.now()
    }

    const markNativeRemoteDropTarget = (detail: { position: { x: number; y: number } }) => {
      const { x, y } = detail.position
      if (typeof x !== 'number' || typeof y !== 'number') return
      const ratio = window.devicePixelRatio || 1
      const targets = [document.elementFromPoint(x, y), document.elementFromPoint(x / ratio, y / ratio)]
      if (targets.some((target) => target?.closest('.remote-pane'))) {
        nativeRemoteDropTargetAtRef.current = Date.now()
      }
    }

    const offRemoteDragOver = onAppEvent(APP_EVENT.tauriRemoteDragOver, markRemoteDropTarget)
    const offNativeDragOver = onAppEvent(APP_EVENT.tauriNativeDragOver, markNativeRemoteDropTarget)
    return () => {
      offRemoteDragOver()
      offNativeDragOver()
    }
  }, [])

  useEffect(() => {
    if (!fileClipboard) {
      return
    }

    const handleEscapeClearClipboard = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setFileClipboard(null)
      }
    }

    window.addEventListener('keydown', handleEscapeClearClipboard)
    return () => window.removeEventListener('keydown', handleEscapeClearClipboard)
  }, [fileClipboard])

  const reportOperationError = (
    setter: (message: string) => void,
    scope: string,
    error: unknown,
    details?: FileOperationErrorDetails
  ) => {
    console.error(`[FileTerm] ${scope}`, error)
    setter(formatError(scope, error, details))
  }

  const reportStatusError = (scope: string, error: unknown, details?: FileOperationErrorDetails) => {
    reportOperationError(onStatusMessage, scope, error, details)
  }

  const ensureActiveRemoteSessionConnected = (setter: (message: string) => void = onStatusMessage) => {
    if (!activeTab || !activeSession?.connected) {
      setter(t.remoteSessionDisconnectedAction)
      return false
    }
    return true
  }

  const runtime: FileOperationsRuntime = {
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
    onApplySnapshot,
    onBusyChange,
    onStatusMessage,
    formatError,
    openLocalFile,
    openRemoteFile,
    remoteDirectoryLoadingTabId,
    setRemoteDirectoryLoadingTabId,
    fileActionDialog,
    setFileActionDialog,
    fileActionError,
    setFileActionError,
    isFileActionSubmitting,
    setIsFileActionSubmitting,
    fileActionSubmittingRef,
    fileClipboard,
    setFileClipboard,
    permissionDialog,
    setPermissionDialog,
    permissionDialogError,
    setPermissionDialogError,
    isPermissionSubmitting,
    setIsPermissionSubmitting,
    permissionSubmittingRef,
    rootAccessDialog,
    setRootAccessDialog,
    rootAccessDialogError,
    setRootAccessDialogError,
    isRootAccessSubmitting,
    setIsRootAccessSubmitting,
    rootAccessSubmittingRef,
    localNetworkCredentialsDialog,
    setLocalNetworkCredentialsDialog,
    localNetworkCredentialsDialogError,
    setLocalNetworkCredentialsDialogError,
    localNetworkShareDialog,
    setLocalNetworkShareDialog,
    localNetworkShareDialogError,
    setLocalNetworkShareDialogError,
    localNetworkShareSource,
    setLocalNetworkShareSource,
    isWorkspaceRefreshing,
    setIsWorkspaceRefreshing,
    isLocalNetworkCredentialsSubmitting,
    setIsLocalNetworkCredentialsSubmitting,
    localNetworkCredentialsSubmittingRef,
    nativeRemoteDropTargetAtRef,
    nativeDropConsumedAtRef,
    reportOperationError,
    reportStatusError,
    ensureActiveRemoteSessionConnected
  }
  const navigation = createFileOperationsNavigation(runtime)
  const {
    canPasteIntoLocal,
    canPasteIntoRemote,
    clearCutState,
    clipboardStatusText,
    copyItems,
    cutItems,
    handleBackToLocalComputer,
    handleOpenLocalItem,
    handleOpenLocalPath,
    handleOpenRemoteItem,
    handleOpenRemotePath,
    handlePasteIntoPane,
    localCutPaths,
    openLocalDirectory,
    openRemoteDirectory,
    refreshCurrentPane,
    remoteCutPaths,
    setClipboardItems
  } = navigation
  const fileActions = createFileOperationsActions({ ...runtime, ...navigation })
  const {
    dismissFileActionDialog,
    dismissPermissionDialog,
    handleQuickDelete,
    handleSubmitFileAction,
    handleSubmitPermissions,
    requestChangePermissions,
    requestDelete,
    requestNewFile,
    requestNewFolder,
    requestRename
  } = fileActions
  const transferActions = useFileOperationsTransfers({ ...runtime, ...navigation }, navigation)
  const {
    changeLocalNetworkCredentials,
    dismissLocalNetworkCredentialsDialog,
    dismissLocalNetworkShareDialog,
    dismissRootAccessDialog,
    handleChooseUploadFiles,
    handleConfirmRootAccess,
    handleDownloadFiles,
    handleDownloadLocalNetworkFiles,
    handleDropUpload,
    handleRefreshWorkspace,
    handleSubmitLocalNetworkCredentials,
    handleSubmitLocalNetworkShare,
    handleToggleFollowShellCwd,
    handleToggleRemoteFileAccessMode,
    handleUploadFiles,
    uploadLocalPaths
  } = transferActions

  return {
    remoteDirectoryLoadingTabId,
    isRemoteDirectoryLoading: remoteDirectoryLoadingTabId === activeTab?.id,
    isWorkspaceRefreshing,
    fileClipboard,
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
    isLocalNetworkShare: Boolean(localNetworkShareSource),
    localPanePath: localNetworkShareSource ? getNetworkShareDisplayPath(localNetworkShareSource, localPath) : localPath,
    localNetworkShareSource,
    remoteFileAccessMode: activeSession?.fileAccessMode ?? 'user',
    openLocalDirectory,
    openRemoteDirectory,
    refreshCurrentPane,
    handleOpenLocalItem,
    handleOpenLocalPath,
    handleBackToLocalComputer,
    handleOpenRemoteItem,
    handleOpenRemotePath,
    setClipboardItems,
    copyItems,
    cutItems,
    clearCutState,
    handlePasteIntoPane,
    requestNewFolder,
    requestNewFile,
    requestRename,
    requestDelete,
    handleSubmitFileAction,
    dismissFileActionDialog,
    requestChangePermissions,
    handleSubmitPermissions,
    dismissPermissionDialog,
    handleQuickDelete,
    uploadLocalPaths,
    handleDropUpload,
    handleUploadFiles,
    handleChooseUploadFiles,
    handleDownloadFiles,
    handleDownloadLocalNetworkFiles,
    handleRefreshWorkspace,
    handleToggleRemoteFileAccessMode,
    handleToggleFollowShellCwd,
    handleConfirmRootAccess,
    dismissRootAccessDialog,
    handleSubmitLocalNetworkCredentials,
    dismissLocalNetworkCredentialsDialog,
    handleSubmitLocalNetworkShare,
    dismissLocalNetworkShareDialog,
    changeLocalNetworkCredentials
  }
}

export type UseFileOperationsResult = ReturnType<typeof useFileOperations>
