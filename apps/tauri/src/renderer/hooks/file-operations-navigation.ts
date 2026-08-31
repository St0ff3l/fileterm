import type { LocalFileItem, RemoteFileItem } from '@fileterm/core'
import { WINDOWS_DRIVES_PATH, withParentRow } from '../app/app-utils'
import { formatMessage, t } from '../i18n'
import type {
  FileClipboardState,
  FileOperationsRuntime,
  FilePane,
  LocalNetworkShareSource
} from './file-operations-types'
import {
  allocateTargetNames,
  areClipboardItemsEqual,
  areLocalPathsEqual,
  createNetworkShareParentRow,
  createNetworkShareRootItems,
  isLocalPathWithin,
  isNetworkShareHostPath,
  isNetworkShareRootItem,
  isSmbCredentialsRequiredError,
  joinLocalPath,
  joinRemotePath,
  resolveNetworkSharePath
} from './file-operations-utils'

type OpenLocalDirectoryOptions = {
  promptForSmbCredentials?: boolean
  networkShareSource?: LocalNetworkShareSource | null
  clearNetworkShare?: boolean
}

export function createFileOperationsNavigation(context: FileOperationsRuntime) {
  const {
    desktopApi,
    workspace,
    activeTab,
    activeSession,
    locale,
    localPath,
    localItems,
    setLocalPath,
    setLocalItems,
    setIsLocalDirectoryLoading,
    onApplySnapshot,
    onBusyChange,
    openLocalFile,
    openRemoteFile,
    formatError,
    fileClipboard,
    setFileClipboard,
    localNetworkShareSource,
    setLocalNetworkShareSource,
    setLocalNetworkCredentialsDialog,
    setLocalNetworkCredentialsDialogError,
    setRemoteDirectoryLoadingTabId,
    reportStatusError,
    ensureActiveRemoteSessionConnected
  } = context

  const openLocalDirectory = async (targetPath: string, options?: OpenLocalDirectoryOptions) => {
    if (!desktopApi) {
      setLocalPath(targetPath)
      return
    }

    setIsLocalDirectoryLoading(true)
    const clearNetworkShare = options?.clearNetworkShare === true
    const resolvedTargetPath =
      !clearNetworkShare && localNetworkShareSource
        ? resolveNetworkSharePath(localNetworkShareSource, targetPath)
        : targetPath
    const networkShareSource = clearNetworkShare
      ? null
      : (options?.networkShareSource ??
        (localNetworkShareSource &&
        (isLocalPathWithin(localNetworkShareSource.mountPath, resolvedTargetPath) ||
          isNetworkShareHostPath(localNetworkShareSource, resolvedTargetPath))
          ? localNetworkShareSource
          : null))
    setLocalNetworkShareSource(networkShareSource)

    try {
      if (
        networkShareSource &&
        networkShareSource.shares.length > 0 &&
        isNetworkShareHostPath(networkShareSource, resolvedTargetPath)
      ) {
        setLocalPath(networkShareSource.hostPath)
        setLocalItems(createNetworkShareRootItems(networkShareSource))
        return
      }

      const { path, items } = await desktopApi.listLocalDirectory(resolvedTargetPath)
      setLocalPath(path)
      const parentRow =
        networkShareSource &&
        networkShareSource.shares.length > 0 &&
        areLocalPathsEqual(networkShareSource.mountPath, path)
          ? [createNetworkShareParentRow(networkShareSource.hostPath)]
          : []
      setLocalItems([...parentRow, ...withParentRow(path, items, networkShareSource?.mountPath)])
    } catch (error) {
      if (
        options?.promptForSmbCredentials !== false &&
        desktopApi.connectLocalNetworkShare &&
        isSmbCredentialsRequiredError(error)
      ) {
        setLocalNetworkCredentialsDialog({ path: targetPath })
        setLocalNetworkCredentialsDialogError(null)
        return
      }
      throw error
    } finally {
      setIsLocalDirectoryLoading(false)
    }
  }

  const openNetworkShareItem = async (item: LocalFileItem, source: LocalNetworkShareSource) => {
    if (!desktopApi?.connectLocalNetworkShare) {
      throw new Error(t.smbFolderSwitchUnsupported)
    }

    const result = await desktopApi.connectLocalNetworkShare(item.path, source.username, source.password)
    if (result.kind !== 'connected') {
      throw new Error(t.networkShareSelectUnavailable)
    }
    await openLocalDirectory(result.path, {
      promptForSmbCredentials: false,
      networkShareSource: {
        mountPath: result.path,
        remotePath: item.path,
        hostPath: source.hostPath,
        shares: source.shares,
        username: source.username,
        password: source.password
      }
    })
  }

  const openRemoteDirectory = async (tabId: string, targetPath: string, item?: RemoteFileItem) => {
    if (!desktopApi) {
      return
    }

    if (!workspace.sessions[tabId]?.connected) {
      throw new Error(t.remoteSessionDisconnectedAction)
    }

    try {
      const snapshot = await desktopApi.openRemotePath(tabId, targetPath)
      onApplySnapshot(snapshot)
    } catch (error) {
      throw new Error(formatError('打开远程目录', error, { targetPath, item }))
    }
  }

  const refreshCurrentPane = async (pane: FilePane) => {
    if (pane === 'local') {
      await openLocalDirectory(localPath)
      return
    }

    if (activeTab && activeSession) {
      if (!activeSession.connected) {
        throw new Error(t.remoteSessionDisconnectedAction)
      }
      await openRemoteDirectory(activeTab.id, activeSession.remotePath)
    }
  }

  const handleOpenLocalItem = async (item: LocalFileItem) => {
    if (!desktopApi) {
      setLocalPath(item.path)
      return
    }

    try {
      if (item.type === 'folder') {
        if (localNetworkShareSource && isNetworkShareRootItem(localNetworkShareSource, item.path)) {
          await openNetworkShareItem(item, localNetworkShareSource)
        } else {
          await openLocalDirectory(item.path)
        }
      } else {
        await openLocalFile(item)
      }
    } catch (error) {
      reportStatusError(item.type === 'folder' ? '打开本地文件夹' : '打开本地文件', error, {
        targetPath: item.path
      })
    }
  }

  const handleOpenLocalPath = (targetPath: string) => {
    void openLocalDirectory(targetPath).catch((error: unknown) => {
      reportStatusError('打开本地路径', error, { targetPath })
    })
  }

  const handleBackToLocalComputer = () => {
    const isMac = desktopApi?.platform === 'darwin'
    const targetPath = isMac ? '' : WINDOWS_DRIVES_PATH
    void openLocalDirectory(targetPath, { clearNetworkShare: true }).catch((error: unknown) => {
      reportStatusError('打开本地路径', error, { targetPath })
    })
  }

  const handleOpenRemoteItem = (item: RemoteFileItem) => {
    if (!desktopApi || !activeTab || !ensureActiveRemoteSessionConnected()) {
      return
    }

    if (item.type === 'file') {
      void (async () => {
        try {
          await openRemoteFile(activeTab.id, item, locale)
        } catch (error) {
          reportStatusError('打开远程文件', error, { targetPath: item.path, item })
        }
      })()
      return
    }

    void (async () => {
      try {
        setRemoteDirectoryLoadingTabId(activeTab.id)
        await openRemoteDirectory(activeTab.id, item.path, item)
      } catch (error) {
        reportStatusError('打开远程文件夹', error, { targetPath: item.path, item })
      } finally {
        setRemoteDirectoryLoadingTabId((current) => (current === activeTab.id ? null : current))
      }
    })()
  }

  const handleOpenRemotePath = (targetPath: string) => {
    if (!activeTab || !ensureActiveRemoteSessionConnected()) {
      return
    }

    void (async () => {
      try {
        setRemoteDirectoryLoadingTabId(activeTab.id)
        await openRemoteDirectory(activeTab.id, targetPath)
      } catch (error) {
        reportStatusError('打开远程路径', error, { targetPath })
      } finally {
        setRemoteDirectoryLoadingTabId((current) => (current === activeTab.id ? null : current))
      }
    })()
  }

  const setClipboardItems = (
    operation: FileClipboardState['operation'],
    pane: FilePane,
    items: Array<LocalFileItem | RemoteFileItem>
  ) => {
    const normalizedItems = items
      .filter((item) => item.name !== '..')
      .map((item) => ({
        pane,
        path: item.path,
        name: item.name,
        type: item.type,
        isSymlink: item.isSymlink
      }))

    if (!normalizedItems.length) {
      return
    }

    const nextClipboard = {
      pane,
      operation,
      items: normalizedItems,
      tabId: pane === 'remote' ? activeTab?.id : undefined
    } satisfies FileClipboardState

    setFileClipboard((current) => {
      if (
        current &&
        current.pane === nextClipboard.pane &&
        current.operation === nextClipboard.operation &&
        current.tabId === nextClipboard.tabId &&
        areClipboardItemsEqual(current.items, nextClipboard.items)
      ) {
        return null
      }

      return nextClipboard
    })
  }

  const copyItems = (pane: FilePane, items: Array<LocalFileItem | RemoteFileItem>) => {
    setClipboardItems('copy', pane, items)
  }

  const cutItems = (pane: FilePane, items: Array<LocalFileItem | RemoteFileItem>) => {
    setClipboardItems('cut', pane, items)
  }

  const canPasteIntoLocal = Boolean(
    fileClipboard && (fileClipboard.pane !== 'remote' || workspace.sessions[fileClipboard.tabId ?? '']?.connected)
  )
  const canPasteIntoRemote = Boolean(
    fileClipboard &&
    activeTab &&
    activeSession?.connected &&
    (fileClipboard.pane !== 'remote' || fileClipboard.tabId === activeTab.id)
  )
  const localCutPaths =
    fileClipboard?.operation === 'cut' && fileClipboard.pane === 'local'
      ? fileClipboard.items.map((item) => item.path)
      : []
  const remoteCutPaths =
    fileClipboard?.operation === 'cut' && fileClipboard.pane === 'remote'
      ? fileClipboard.items.map((item) => item.path)
      : []
  const clipboardStatusText = fileClipboard
    ? fileClipboard.operation === 'cut'
      ? formatMessage(t.filesCutStatus, { count: fileClipboard.items.length })
      : formatMessage(t.filesCopiedStatus, { count: fileClipboard.items.length })
    : null

  const clearCutState = () => {
    setFileClipboard(null)
  }

  const handlePasteIntoPane = (pane: FilePane) => {
    if (!desktopApi || !fileClipboard) {
      return
    }

    void (async () => {
      try {
        onBusyChange(true)

        const destinationDirectory = pane === 'local' ? localPath : activeSession?.remotePath
        if (!destinationDirectory || (pane === 'remote' && !activeTab)) {
          return
        }
        if (pane === 'remote' && !ensureActiveRemoteSessionConnected()) {
          return
        }
        if (fileClipboard.pane === 'remote' && !workspace.sessions[fileClipboard.tabId ?? '']?.connected) {
          throw new Error(t.remoteSessionDisconnectedAction)
        }
        if (fileClipboard.pane === 'remote' && pane === 'remote' && fileClipboard.tabId !== activeTab?.id) {
          throw new Error(t.crossSessionPasteUnsupported)
        }

        const existingNames =
          pane === 'local'
            ? localItems.filter((item) => item.name !== '..').map((item) => item.name)
            : (activeSession?.remoteFiles ?? []).filter((item) => item.name !== '..').map((item) => item.name)
        const targetNames = allocateTargetNames(
          fileClipboard.items,
          existingNames,
          fileClipboard.operation,
          destinationDirectory
        )

        if (fileClipboard.pane === 'local' && pane === 'local') {
          for (const [index, item] of fileClipboard.items.entries()) {
            const destinationPath = joinLocalPath(destinationDirectory, targetNames[index]!)
            if (fileClipboard.operation === 'copy') {
              await desktopApi.copyLocalPath(item.path, destinationPath)
            } else {
              await desktopApi.moveLocalPath(item.path, destinationPath)
            }
          }
          await openLocalDirectory(localPath)
        } else if (fileClipboard.pane === 'local' && pane === 'remote') {
          for (const [index, item] of fileClipboard.items.entries()) {
            const snapshot = await desktopApi.uploadFile(activeTab!.id, item.path, destinationDirectory, {
              targetName: targetNames[index]
            })
            onApplySnapshot(snapshot)
            if (fileClipboard.operation === 'cut') {
              await desktopApi.deleteLocalPath(item.path)
            }
          }
          await openLocalDirectory(localPath)
          await refreshCurrentPane('remote')
        } else if (fileClipboard.pane === 'remote' && pane === 'local') {
          for (const [index, item] of fileClipboard.items.entries()) {
            const snapshot = await desktopApi.downloadRemotePath(
              fileClipboard.tabId!,
              item.path,
              item.type,
              destinationDirectory,
              { targetName: targetNames[index] }
            )
            onApplySnapshot(snapshot)
            if (fileClipboard.operation === 'cut') {
              const deleteSnapshot = await desktopApi.deleteRemotePath(
                fileClipboard.tabId!,
                item.path,
                item.type,
                item.isSymlink
              )
              onApplySnapshot(deleteSnapshot)
            }
          }
          await openLocalDirectory(localPath)
          if (fileClipboard.tabId === activeTab?.id) {
            await refreshCurrentPane('remote')
          }
        } else if (fileClipboard.pane === 'remote' && pane === 'remote') {
          for (const [index, item] of fileClipboard.items.entries()) {
            const destinationPath = joinRemotePath(destinationDirectory, targetNames[index]!)
            const snapshot =
              fileClipboard.operation === 'copy'
                ? await desktopApi.copyRemotePath(activeTab!.id, item.path, destinationPath, item.type)
                : await desktopApi.moveRemotePath(activeTab!.id, item.path, destinationPath)
            onApplySnapshot(snapshot)
          }
          await refreshCurrentPane('remote')
        }

        if (fileClipboard.operation === 'cut') {
          setFileClipboard(null)
        }
      } catch (error) {
        reportStatusError('粘贴文件', error)
      } finally {
        onBusyChange(false)
      }
    })()
  }

  return {
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
  }
}
