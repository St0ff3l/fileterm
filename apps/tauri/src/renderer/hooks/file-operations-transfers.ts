import { useEffect, type DragEvent } from 'react'
import type { LocalFileItem, RemoteFileItem } from '@fileterm/core'
import { APP_EVENT, onAppEvent } from '../lib/app-events'
import { t } from '../i18n'
import type {
  FileOperationsRuntime,
  FilePane,
  LocalNetworkShareSource,
  RootAccessCredentials
} from './file-operations-types'
import {
  allocateTargetNames,
  extractDroppedLocalPaths,
  fileNameFromPath,
  joinLocalPath,
  joinNetworkSharePath,
  networkShareHostPath,
  normalizeNetworkSharePath
} from './file-operations-utils'

type FileOperationsNavigation = {
  openLocalDirectory(
    targetPath: string,
    options?: {
      promptForSmbCredentials?: boolean
      networkShareSource?: LocalNetworkShareSource | null
      clearNetworkShare?: boolean
    }
  ): Promise<void>
  openRemoteDirectory(tabId: string, targetPath: string, item?: RemoteFileItem): Promise<void>
  refreshCurrentPane(pane: FilePane): Promise<void>
}

export function useFileOperationsTransfers(context: FileOperationsRuntime, navigation: FileOperationsNavigation) {
  const {
    desktopApi,
    workspace,
    activeTab,
    activeSession,
    activeProfile,
    localPath,
    localNetworkShareSource,
    onApplySnapshot,
    onBusyChange,
    onStatusMessage,
    formatError,
    setFileClipboard,
    setRemoteDirectoryLoadingTabId,
    setIsWorkspaceRefreshing,
    rootAccessDialog,
    setRootAccessDialog,
    setRootAccessDialogError,
    rootAccessSubmittingRef,
    setIsRootAccessSubmitting,
    localNetworkCredentialsDialog,
    setLocalNetworkCredentialsDialog,
    setLocalNetworkCredentialsDialogError,
    localNetworkCredentialsSubmittingRef,
    setIsLocalNetworkCredentialsSubmitting,
    localNetworkShareDialog,
    setLocalNetworkShareDialog,
    setLocalNetworkShareDialogError,
    reportOperationError,
    reportStatusError,
    ensureActiveRemoteSessionConnected,
    nativeRemoteDropTargetAtRef,
    nativeDropConsumedAtRef,
    openLocalDirectory,
    openRemoteDirectory,
    refreshCurrentPane
  } = { ...context, ...navigation }

  const uploadLocalPaths = async (paths: string[]) => {
    if (!desktopApi || !activeTab || !activeSession || !ensureActiveRemoteSessionConnected()) {
      return
    }

    const uniquePaths = Array.from(new Set(paths))
    if (uniquePaths.length > 1) {
      const snapshot = await desktopApi.queueUpload(uniquePaths.map(fileNameFromPath))
      onApplySnapshot(snapshot)
    }

    for (const sourcePath of uniquePaths) {
      const snapshot = await desktopApi.uploadFile(activeTab.id, sourcePath, activeSession.remotePath)
      onApplySnapshot(snapshot)
    }
  }

  useEffect(() => {
    const handleNativeDrop = (detail: { paths: string[]; consume: () => void; position: { x: number; y: number } }) => {
      const paths = detail.paths.filter((path) => typeof path === 'string' && path.length > 0)
      const position = detail.position
      if (!paths.length || typeof position.x !== 'number' || typeof position.y !== 'number') {
        return
      }

      const targetMarkedByDragOver = Date.now() - nativeRemoteDropTargetAtRef.current < 1_500
      const ratio = window.devicePixelRatio || 1
      const targets = [
        document.elementFromPoint(position.x, position.y),
        document.elementFromPoint(position.x / ratio, position.y / ratio)
      ]
      if (!targetMarkedByDragOver && !targets.some((target) => target?.closest('.remote-pane'))) {
        return
      }
      if (typeof detail.consume === 'function') {
        detail.consume()
      }
      nativeRemoteDropTargetAtRef.current = 0
      nativeDropConsumedAtRef.current = Date.now()

      void (async () => {
        try {
          onBusyChange(true)
          await uploadLocalPaths(paths)
        } catch (error) {
          reportStatusError('上传文件', error)
        } finally {
          onBusyChange(false)
        }
      })()
    }

    return onAppEvent(APP_EVENT.tauriNativeDrop, handleNativeDrop)
  }, [activeSession, activeTab, desktopApi, onBusyChange, reportStatusError, uploadLocalPaths])

  const handleDropUpload = async (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault()
    const localPaths = extractDroppedLocalPaths(event, desktopApi)

    if (!localPaths.length || !desktopApi || !activeTab || !activeSession) {
      if (Date.now() - nativeDropConsumedAtRef.current < 1_500) {
        return
      }
      onStatusMessage(t.desktopOnlyUpload)
      return
    }

    if (!ensureActiveRemoteSessionConnected()) {
      return
    }

    try {
      onBusyChange(true)
      await uploadLocalPaths(localPaths)
    } catch (error) {
      reportStatusError('上传文件', error)
    } finally {
      onBusyChange(false)
    }
  }

  const handleUploadFiles = (items: LocalFileItem[]) => {
    if (!desktopApi) {
      return
    }

    void (async () => {
      try {
        onBusyChange(true)
        await uploadLocalPaths(items.map((item) => item.path))
      } catch (error) {
        reportStatusError('上传文件', error)
      } finally {
        onBusyChange(false)
      }
    })()
  }

  const handleChooseUploadFiles = () => {
    if (!desktopApi) {
      return
    }

    void (async () => {
      let markedBusy = false
      try {
        const filePaths = await desktopApi.selectLocalFiles(localPath)
        if (!filePaths.length) {
          return
        }

        onBusyChange(true)
        markedBusy = true
        await uploadLocalPaths(filePaths)
      } catch (error) {
        reportStatusError('上传文件', error)
      } finally {
        if (markedBusy) {
          onBusyChange(false)
        }
      }
    })()
  }

  const handleDownloadFiles = (items: RemoteFileItem[], targetDirectory?: string) => {
    if (!desktopApi || !activeTab || !ensureActiveRemoteSessionConnected()) {
      return
    }

    void (async () => {
      const downloadableItems = items.filter((row) => row.name !== '..')
      if (!downloadableItems.length) {
        return
      }

      let downloadDirectory: string | null | undefined = targetDirectory
      let markedBusy = false
      try {
        downloadDirectory ??= await desktopApi.selectLocalDirectory()
        if (!downloadDirectory) {
          return
        }

        onBusyChange(true)
        markedBusy = true
        for (const item of downloadableItems) {
          const snapshot = await desktopApi.downloadRemotePath(activeTab.id, item.path, item.type, downloadDirectory)
          onApplySnapshot(snapshot)
        }
        await openLocalDirectory(downloadDirectory)
      } catch (error) {
        reportStatusError('下载文件或目录', error, { targetPath: downloadDirectory ?? undefined })
      } finally {
        if (markedBusy) {
          onBusyChange(false)
        }
      }
    })()
  }

  const handleDownloadLocalNetworkFiles = (items: LocalFileItem[]) => {
    if (!desktopApi || !localNetworkShareSource) {
      return
    }

    void (async () => {
      const downloadableItems = items.filter((row) => row.name !== '..')
      if (!downloadableItems.length) {
        return
      }

      let downloadDirectory: string | null | undefined
      let markedBusy = false
      try {
        downloadDirectory = await desktopApi.selectLocalDirectory()
        if (!downloadDirectory) {
          return
        }

        const destinationSnapshot = await desktopApi.listLocalDirectory(downloadDirectory)
        const existingNames = destinationSnapshot.items.filter((item) => item.name !== '..').map((item) => item.name)
        const targetItems = downloadableItems.map((item) => ({
          pane: 'local' as const,
          path: item.path,
          name: item.name,
          type: item.type
        }))
        const targetNames = allocateTargetNames(targetItems, existingNames, 'copy', downloadDirectory)

        onBusyChange(true)
        markedBusy = true
        for (const [index, item] of downloadableItems.entries()) {
          await desktopApi.copyLocalPath(item.path, joinLocalPath(downloadDirectory, targetNames[index]!))
        }
      } catch (error) {
        reportStatusError('下载网络共享文件或目录', error, { targetPath: downloadDirectory ?? undefined })
      } finally {
        if (markedBusy) {
          onBusyChange(false)
        }
      }
    })()
  }

  const handleRefreshWorkspace = () => {
    if (!activeTab || !activeSession || !ensureActiveRemoteSessionConnected()) {
      return
    }

    void (async () => {
      try {
        setIsWorkspaceRefreshing(true)
        setRemoteDirectoryLoadingTabId(activeTab.id)
        setFileClipboard(null)
        await Promise.all([openLocalDirectory(localPath), openRemoteDirectory(activeTab.id, activeSession.remotePath)])
      } catch (error) {
        reportStatusError('刷新工作区', error, { targetPath: activeSession.remotePath })
      } finally {
        setIsWorkspaceRefreshing(false)
        setRemoteDirectoryLoadingTabId((current) => (current === activeTab.id ? null : current))
      }
    })()
  }

  const handleToggleRemoteFileAccessMode = () => {
    if (!desktopApi || !activeTab || activeTab.sessionType !== 'ssh' || !activeSession) {
      return
    }

    if (!ensureActiveRemoteSessionConnected()) {
      return
    }

    const nextMode = activeSession.fileAccessMode === 'root' ? 'user' : 'root'
    if (nextMode === 'root') {
      rootAccessSubmittingRef.current = false
      setRootAccessDialogError(null)
      setIsRootAccessSubmitting(false)
      setRootAccessDialog({
        tabId: activeTab.id,
        sshUser: activeProfile?.type === 'ssh' ? activeProfile.username : undefined,
        rootAccessMethod: 'sudo',
        sudoUser: 'root',
        hasSavedSudoPassword:
          activeSession.hasReusableSudoAuth ||
          (activeProfile?.type === 'ssh' && activeProfile.hasSavedSudoPassword === true),
        hasSavedSuPassword: activeProfile?.type === 'ssh' && activeProfile.hasSavedSuPassword === true
      })
      return
    }

    void (async () => {
      try {
        onBusyChange(true)
        const snapshot = await desktopApi.setRemoteFileAccessMode(activeTab.id, nextMode)
        onApplySnapshot(snapshot)
        await refreshCurrentPane('remote')
      } catch (error) {
        reportStatusError('切换到普通视角', error)
      } finally {
        onBusyChange(false)
      }
    })()
  }

  const handleToggleFollowShellCwd = () => {
    if (
      !desktopApi ||
      !activeTab ||
      activeTab.sessionType !== 'ssh' ||
      !activeSession ||
      !ensureActiveRemoteSessionConnected()
    ) {
      return
    }

    void (async () => {
      try {
        const snapshot = await desktopApi.setFollowShellCwd(activeTab.id, activeSession.followShellCwd === false)
        onApplySnapshot(snapshot)
      } catch (error) {
        reportStatusError('切换终端目录跟随', error)
      }
    })()
  }

  const handleConfirmRootAccess = ({ rootAccessMethod, sudoUser, sudoPassword }: RootAccessCredentials) => {
    if (!desktopApi || !rootAccessDialog || rootAccessSubmittingRef.current) {
      return
    }

    if (!workspace.sessions[rootAccessDialog.tabId]?.connected) {
      setRootAccessDialogError(t.remoteSessionDisconnectedAction)
      return
    }

    rootAccessSubmittingRef.current = true
    void (async () => {
      try {
        setIsRootAccessSubmitting(true)
        setRootAccessDialogError(null)
        const snapshot = await desktopApi.setRemoteFileAccessMode(rootAccessDialog.tabId, 'root', {
          rootAccessMethod,
          sudoUser,
          sudoPassword,
          useSavedPassword: sudoPassword.length === 0
        })
        onApplySnapshot(snapshot)
        await refreshCurrentPane('remote')
        setRootAccessDialog(null)
        setRootAccessDialogError(null)
      } catch (error) {
        reportOperationError(setRootAccessDialogError, '切换到 root 视角', error)
      } finally {
        rootAccessSubmittingRef.current = false
        setIsRootAccessSubmitting(false)
      }
    })()
  }

  const dismissRootAccessDialog = () => {
    if (rootAccessSubmittingRef.current) return
    setRootAccessDialog(null)
    setRootAccessDialogError(null)
    setIsRootAccessSubmitting(false)
    rootAccessSubmittingRef.current = false
  }

  const handleSubmitLocalNetworkCredentials = ({
    username: rawUsername,
    password
  }: {
    username: string
    password: string
  }) => {
    if (
      !desktopApi?.connectLocalNetworkShare ||
      !localNetworkCredentialsDialog ||
      localNetworkCredentialsSubmittingRef.current
    ) {
      return
    }

    const connectLocalNetworkShare = desktopApi.connectLocalNetworkShare
    const credentialsDialog = localNetworkCredentialsDialog
    const username = rawUsername.trim()
    if (!username || !password) {
      setLocalNetworkCredentialsDialogError(t.networkShareCredentialsFillRequired)
      return
    }

    localNetworkCredentialsSubmittingRef.current = true
    void (async () => {
      try {
        setIsLocalNetworkCredentialsSubmitting(true)
        setLocalNetworkCredentialsDialogError(null)
        const result = await connectLocalNetworkShare(credentialsDialog.path, username, password)
        if (result.kind === 'select-share') {
          setLocalNetworkShareDialog({
            path: result.path,
            username,
            password,
            shares: result.shares
          })
          setLocalNetworkCredentialsDialog(null)
          setLocalNetworkCredentialsDialogError(null)
          return
        }
        await openLocalDirectory(result.path, {
          promptForSmbCredentials: false,
          networkShareSource: {
            mountPath: result.path,
            remotePath: normalizeNetworkSharePath(credentialsDialog.path),
            hostPath: networkShareHostPath(credentialsDialog.path),
            shares: [],
            username,
            password
          }
        })
        setLocalNetworkCredentialsDialog(null)
        setLocalNetworkCredentialsDialogError(null)
      } catch (error) {
        setLocalNetworkCredentialsDialogError(formatError('连接 SMB 网络共享', error))
      } finally {
        localNetworkCredentialsSubmittingRef.current = false
        setIsLocalNetworkCredentialsSubmitting(false)
      }
    })()
  }

  const handleSubmitLocalNetworkShare = (share: string) => {
    if (
      !desktopApi?.connectLocalNetworkShare ||
      !localNetworkShareDialog ||
      localNetworkCredentialsSubmittingRef.current
    ) {
      return
    }

    if (!share) {
      setLocalNetworkShareDialogError(t.networkShareSelectRequired)
      return
    }

    const connectLocalNetworkShare = desktopApi.connectLocalNetworkShare
    const shareDialog = localNetworkShareDialog
    localNetworkCredentialsSubmittingRef.current = true
    void (async () => {
      try {
        setIsLocalNetworkCredentialsSubmitting(true)
        setLocalNetworkShareDialogError(null)
        const result = await connectLocalNetworkShare(
          shareDialog.path,
          shareDialog.username,
          shareDialog.password,
          share
        )
        if (result.kind !== 'connected') {
          throw new Error(t.networkShareSelectUnavailable)
        }
        await openLocalDirectory(result.path, {
          promptForSmbCredentials: false,
          networkShareSource: {
            mountPath: result.path,
            remotePath: joinNetworkSharePath(shareDialog.path, share),
            hostPath: normalizeNetworkSharePath(shareDialog.path),
            shares: shareDialog.shares,
            username: shareDialog.username,
            password: shareDialog.password
          }
        })
        setLocalNetworkShareDialog(null)
        setLocalNetworkShareDialogError(null)
      } catch (error) {
        setLocalNetworkShareDialogError(formatError('打开 SMB 共享文件夹', error))
      } finally {
        localNetworkCredentialsSubmittingRef.current = false
        setIsLocalNetworkCredentialsSubmitting(false)
      }
    })()
  }

  const dismissLocalNetworkCredentialsDialog = () => {
    if (localNetworkCredentialsSubmittingRef.current) return
    setLocalNetworkCredentialsDialog(null)
    setLocalNetworkCredentialsDialogError(null)
    setIsLocalNetworkCredentialsSubmitting(false)
    localNetworkCredentialsSubmittingRef.current = false
  }

  const dismissLocalNetworkShareDialog = () => {
    if (localNetworkCredentialsSubmittingRef.current) return
    setLocalNetworkShareDialog(null)
    setLocalNetworkShareDialogError(null)
    setIsLocalNetworkCredentialsSubmitting(false)
    localNetworkCredentialsSubmittingRef.current = false
  }

  const changeLocalNetworkCredentials = () => {
    if (!localNetworkShareDialog || localNetworkCredentialsSubmittingRef.current) return
    setLocalNetworkCredentialsDialog({ path: localNetworkShareDialog.path })
    setLocalNetworkCredentialsDialogError(null)
    setLocalNetworkShareDialog(null)
    setLocalNetworkShareDialogError(null)
  }

  return {
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
  }
}
