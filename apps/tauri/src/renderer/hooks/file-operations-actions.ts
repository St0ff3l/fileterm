import type { LocalFileItem, PermissionChangeOptions, RemoteFileItem } from '@fileterm/core'
import type { FileOperationsRuntime, FilePane } from './file-operations-types'
import { t } from '../i18n'

type FileOperationsActionContext = FileOperationsRuntime & {
  refreshCurrentPane(pane: FilePane): Promise<void>
}

export function createFileOperationsActions(context: FileOperationsActionContext) {
  const {
    desktopApi,
    activeTab,
    onApplySnapshot,
    onBusyChange,
    setFileActionDialog,
    fileActionDialog,
    setFileActionError,
    fileActionSubmittingRef,
    setIsFileActionSubmitting,
    setPermissionDialog,
    permissionDialog,
    setPermissionDialogError,
    permissionSubmittingRef,
    setIsPermissionSubmitting,
    reportOperationError,
    reportStatusError,
    ensureActiveRemoteSessionConnected,
    refreshCurrentPane
  } = context

  const runFileAction = async (action: () => Promise<void>) => {
    if (fileActionSubmittingRef.current) return
    fileActionSubmittingRef.current = true
    try {
      onBusyChange(true)
      setIsFileActionSubmitting(true)
      await action()
      setFileActionDialog(null)
      setFileActionError(null)
    } catch (error) {
      reportOperationError(setFileActionError, '文件操作', error)
    } finally {
      fileActionSubmittingRef.current = false
      setIsFileActionSubmitting(false)
      onBusyChange(false)
    }
  }

  const handleSubmitFileAction = async (rawValue: string) => {
    if (!desktopApi || !fileActionDialog || fileActionSubmittingRef.current) {
      return
    }

    const dialog = fileActionDialog
    const requiresRemoteSession =
      dialog.kind === 'rename'
        ? dialog.target.pane === 'remote'
        : dialog.kind === 'delete'
          ? dialog.targets.some((target) => target.pane === 'remote')
          : dialog.pane === 'remote'

    if (requiresRemoteSession && !ensureActiveRemoteSessionConnected(setFileActionError)) {
      return
    }

    const value = rawValue.trim()

    if (dialog.kind === 'delete') {
      await runFileAction(async () => {
        const [firstTarget] = dialog.targets
        if (!firstTarget) {
          return
        }
        if (firstTarget.pane === 'local') {
          for (const target of dialog.targets) {
            await desktopApi.deleteLocalPath(target.path)
          }
        } else if (activeTab) {
          for (const target of dialog.targets) {
            const snapshot = await desktopApi.deleteRemotePath(activeTab.id, target.path, target.type, target.isSymlink)
            onApplySnapshot(snapshot)
          }
        }
        await refreshCurrentPane(firstTarget.pane)
      })
      return
    }

    if (!value) {
      setFileActionError(t.fileNameRequired)
      return
    }

    if (dialog.kind === 'new-folder') {
      await runFileAction(async () => {
        if (dialog.pane === 'local') {
          await desktopApi.createLocalDirectory(dialog.directoryPath, value)
        } else if (activeTab) {
          const snapshot = await desktopApi.createRemoteDirectory(activeTab.id, dialog.directoryPath, value)
          onApplySnapshot(snapshot)
        }
        await refreshCurrentPane(dialog.pane)
      })
      return
    }

    if (dialog.kind === 'new-file') {
      await runFileAction(async () => {
        if (dialog.pane === 'local') {
          await desktopApi.createLocalFile(dialog.directoryPath, value)
        } else if (activeTab) {
          const snapshot = await desktopApi.createRemoteFile(activeTab.id, dialog.directoryPath, value)
          onApplySnapshot(snapshot)
        }
        await refreshCurrentPane(dialog.pane)
      })
      return
    }

    await runFileAction(async () => {
      if (dialog.target.pane === 'local') {
        await desktopApi.renameLocalPath(dialog.target.path, value)
      } else if (activeTab) {
        const snapshot = await desktopApi.renameRemotePath(activeTab.id, dialog.target.path, value)
        onApplySnapshot(snapshot)
      }
      await refreshCurrentPane(dialog.target.pane)
    })
  }

  const requestNewFolder = (pane: FilePane, directoryPath: string) => {
    if (fileActionSubmittingRef.current) return
    setFileActionError(null)
    setIsFileActionSubmitting(false)
    setFileActionDialog({ kind: 'new-folder', pane, directoryPath })
  }

  const requestNewFile = (pane: FilePane, directoryPath: string) => {
    if (fileActionSubmittingRef.current) return
    setFileActionError(null)
    setIsFileActionSubmitting(false)
    setFileActionDialog({ kind: 'new-file', pane, directoryPath })
  }

  const requestRename = (pane: FilePane, item: LocalFileItem | RemoteFileItem) => {
    if (fileActionSubmittingRef.current) return
    setFileActionError(null)
    setIsFileActionSubmitting(false)
    setFileActionDialog({
      kind: 'rename',
      target: { pane, path: item.path, name: item.name, type: item.type }
    })
  }

  const requestDelete = (pane: FilePane, items: Array<LocalFileItem | RemoteFileItem>) => {
    if (fileActionSubmittingRef.current) return
    setFileActionError(null)
    setIsFileActionSubmitting(false)
    setFileActionDialog({
      kind: 'delete',
      targets: items.map((item) => ({
        pane,
        path: item.path,
        name: item.name,
        type: item.type,
        isSymlink: item.isSymlink
      }))
    })
  }

  const dismissFileActionDialog = () => {
    if (fileActionSubmittingRef.current) return
    setFileActionDialog(null)
    setFileActionError(null)
    setIsFileActionSubmitting(false)
  }

  const requestChangePermissions = (pane: FilePane, item: LocalFileItem | RemoteFileItem) => {
    setPermissionDialogError(null)
    setPermissionDialog({
      target: {
        pane,
        path: item.path,
        name: item.name,
        type: item.type,
        permission: item.permission,
        ownerGroup: item.ownerGroup
      },
      supportsRecursive: item.type === 'folder' && (pane === 'local' || activeTab?.sessionType === 'ssh')
    })
  }

  const handleSubmitPermissions = async (options: PermissionChangeOptions) => {
    if (!desktopApi || !permissionDialog || permissionSubmittingRef.current) {
      return
    }

    if (permissionDialog.target.pane === 'remote' && !ensureActiveRemoteSessionConnected(setPermissionDialogError)) {
      return
    }

    try {
      permissionSubmittingRef.current = true
      setIsPermissionSubmitting(true)
      onBusyChange(true)
      const { target } = permissionDialog
      if (target.pane === 'local') {
        await desktopApi.changeLocalPermissions(target.path, options)
      } else if (activeTab) {
        const snapshot = await desktopApi.changeRemotePermissions(activeTab.id, target.path, options)
        onApplySnapshot(snapshot)
      }
      await refreshCurrentPane(target.pane)
      setPermissionDialog(null)
      setPermissionDialogError(null)
    } catch (error) {
      reportOperationError(setPermissionDialogError, '修改文件权限', error)
    } finally {
      permissionSubmittingRef.current = false
      setIsPermissionSubmitting(false)
      onBusyChange(false)
    }
  }

  const dismissPermissionDialog = () => {
    if (permissionSubmittingRef.current) {
      return
    }
    setPermissionDialog(null)
    setPermissionDialogError(null)
  }

  const handleQuickDelete = (pane: FilePane, items: Array<LocalFileItem | RemoteFileItem>) => {
    if (!desktopApi || pane !== 'remote' || !activeTab || !items.length || !ensureActiveRemoteSessionConnected()) {
      return
    }

    void (async () => {
      try {
        onBusyChange(true)
        for (const item of items) {
          const snapshot = await desktopApi.deleteRemotePath(activeTab.id, item.path, item.type, item.isSymlink)
          onApplySnapshot(snapshot)
        }
        await refreshCurrentPane('remote')
      } catch (error) {
        const firstItem = items[0]
        reportStatusError(
          '快速删除远程文件',
          error,
          firstItem ? { item: firstItem, targetPath: firstItem.path } : undefined
        )
      } finally {
        onBusyChange(false)
      }
    })()
  }

  return {
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
  }
}
