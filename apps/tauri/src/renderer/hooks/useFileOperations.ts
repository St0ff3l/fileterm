import { useEffect, useRef, useState, type Dispatch, type DragEvent, type SetStateAction } from 'react'
import type {
  ConnectionProfile,
  FileTermDesktopApi,
  LocalFileItem,
  PermissionChangeOptions,
  RemoteFileItem,
  SessionSnapshot,
  WorkspaceSnapshot,
  WorkspaceTab
} from '@fileterm/core'
import { withParentRow } from '../app/app-utils'
import { t, type AppLocale } from '../i18n'

const REMOTE_METHOD_ERROR_PREFIX = /Error invoking remote method '[^']+':\s*/i

export type FilePane = 'local' | 'remote'
export type FileClipboardOperation = 'copy' | 'cut'

export type FileDialogTarget = {
  pane: FilePane
  path: string
  name: string
  type: 'file' | 'folder'
}

export type FileClipboardState = {
  pane: FilePane
  operation: FileClipboardOperation
  items: FileDialogTarget[]
  tabId?: string
}

export type FileActionDialog =
  | { kind: 'new-folder'; pane: FilePane; directoryPath: string }
  | { kind: 'new-file'; pane: FilePane; directoryPath: string }
  | { kind: 'rename'; target: FileDialogTarget }
  | { kind: 'delete'; targets: FileDialogTarget[] }

export type PermissionDialogState = {
  target: FileDialogTarget & { ownerGroup?: string; permission?: string }
  supportsRecursive: boolean
}

export type RootAccessDialogState = {
  tabId: string
  sshUser?: string
  sudoUser: string
}

export type RootAccessCredentials = {
  sudoUser: string
  sudoPassword: string
}

export type FileOperationErrorDetails = {
  item?: RemoteFileItem
  targetPath?: string
}

export interface UseFileOperationsOptions {
  desktopApi?: FileTermDesktopApi
  workspace: WorkspaceSnapshot
  activeTab: WorkspaceTab | null
  activeSession: SessionSnapshot | null
  activeProfile: ConnectionProfile | null
  locale: AppLocale
  localPath: string
  localItems: LocalFileItem[]
  setLocalPath: Dispatch<SetStateAction<string>>
  setLocalItems: Dispatch<SetStateAction<LocalFileItem[]>>
  onApplySnapshot(snapshot: WorkspaceSnapshot): void
  onBusyChange(isBusy: boolean): void
  onStatusMessage(message: string): void
  formatError(scope: string, error: unknown, details?: FileOperationErrorDetails): string
  openLocalFile(item: LocalFileItem): unknown | Promise<unknown>
  openRemoteFile(tabId: string, item: RemoteFileItem, locale: AppLocale): unknown | Promise<unknown>
}

export function areClipboardItemsEqual(left: FileDialogTarget[], right: FileDialogTarget[]) {
  if (left.length !== right.length) {
    return false
  }

  for (let index = 0; index < left.length; index += 1) {
    const leftItem = left[index]
    const rightItem = right[index]
    if (
      !leftItem ||
      !rightItem ||
      leftItem.pane !== rightItem.pane ||
      leftItem.path !== rightItem.path ||
      leftItem.name !== rightItem.name ||
      leftItem.type !== rightItem.type
    ) {
      return false
    }
  }

  return true
}

export function splitNameForDuplicate(name: string, type: 'file' | 'folder') {
  if (type === 'folder') {
    return { stem: name, ext: '' }
  }

  const dotIndex = name.lastIndexOf('.')
  if (dotIndex <= 0 || dotIndex === name.length - 1) {
    return { stem: name, ext: '' }
  }

  return {
    stem: name.slice(0, dotIndex),
    ext: name.slice(dotIndex)
  }
}

export function makeDuplicateName(name: string, type: 'file' | 'folder', attempt: number) {
  const { stem, ext } = splitNameForDuplicate(name, type)
  const suffix = attempt === 1 ? ' copy' : ` copy ${attempt}`
  return `${stem}${suffix}${ext}`
}

export function allocateTargetNames(
  items: FileDialogTarget[],
  existingNames: string[],
  operation: FileClipboardOperation,
  destinationPath: string
) {
  const reservedNames = new Set(existingNames)
  return items.map((item) => {
    const isSameDirectory =
      item.pane === 'remote'
        ? remoteDirname(item.path) === destinationPath
        : localDirname(item.path) === destinationPath

    let nextName = item.name

    if (operation === 'cut' && isSameDirectory) {
      reservedNames.add(nextName)
      return nextName
    }

    if (reservedNames.has(nextName) || (operation === 'copy' && isSameDirectory)) {
      let attempt = 1
      do {
        nextName = makeDuplicateName(item.name, item.type, attempt)
        attempt += 1
      } while (reservedNames.has(nextName))
    }

    reservedNames.add(nextName)
    return nextName
  })
}

export function remoteDirname(targetPath: string) {
  const normalized = targetPath.replace(/\/+$/, '') || '/'
  if (normalized === '/') {
    return '/'
  }
  const slashIndex = normalized.lastIndexOf('/')
  if (slashIndex <= 0) {
    return '/'
  }
  return normalized.slice(0, slashIndex)
}

export function joinRemotePath(directoryPath: string, name: string) {
  return directoryPath === '/' ? `/${name}` : `${directoryPath.replace(/\/+$/, '')}/${name}`
}

export function normalizeLocalPath(targetPath: string) {
  return targetPath.replace(/[\\/]+$/, '')
}

export function localDirname(targetPath: string) {
  const normalized = normalizeLocalPath(targetPath)
  if (/^[A-Za-z]:$/.test(normalized)) {
    return `${normalized}\\`
  }
  const slashIndex = Math.max(normalized.lastIndexOf('/'), normalized.lastIndexOf('\\'))
  if (slashIndex <= 0) {
    return slashIndex === 0 ? normalized.slice(0, 1) : '.'
  }
  if (slashIndex === 2 && /^[A-Za-z]:/.test(normalized)) {
    return normalized.slice(0, 3)
  }
  return normalized.slice(0, slashIndex)
}

export function joinLocalPath(directoryPath: string, name: string) {
  const separator = directoryPath.includes('\\') ? '\\' : '/'
  const normalized = normalizeLocalPath(directoryPath)
  if (normalized === separator) {
    return `${separator}${name}`
  }
  return `${normalized}${separator}${name}`
}

export function normalizeRemoteErrorMessage(error: unknown) {
  const rawMessage = error instanceof Error ? error.message : String(error)
  return rawMessage.replace(REMOTE_METHOD_ERROR_PREFIX, '').trim()
}

export function shouldPromptForRootAccess(error: unknown) {
  const message = normalizeRemoteErrorMessage(error)
  return /未检测到可复用的 sudo 授权|sudo 密码错误|sudo 密码无效|sudo 认证失败|sudo 验证超时|sudo credentials|incorrect password|authentication failure/i.test(
    message
  )
}

export function fileNameFromPath(filePath: string) {
  return filePath.split(/[/\\]/).pop() || filePath
}

export function extractDroppedLocalPaths(event: DragEvent<HTMLDivElement>, desktopApi?: FileTermDesktopApi) {
  const fileList = Array.from(event.dataTransfer.files)
  const filePaths = (
    desktopApi?.getDroppedFilePaths?.(fileList) ??
    fileList.map((file) => (file as File & { path?: string }).path).filter(Boolean)
  ).filter((filePath): filePath is string => Boolean(filePath))

  if (filePaths.length) {
    return filePaths
  }

  return Array.from(event.dataTransfer.items)
    .map((item) => item.getAsFile() as (File & { path?: string }) | null)
    .map((file) => file?.path)
    .filter((filePath): filePath is string => Boolean(filePath))
}

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
  const [fileClipboard, setFileClipboard] = useState<FileClipboardState | null>(null)
  const [permissionDialog, setPermissionDialog] = useState<PermissionDialogState | null>(null)
  const [permissionDialogError, setPermissionDialogError] = useState<string | null>(null)
  const [rootAccessDialog, setRootAccessDialog] = useState<RootAccessDialogState | null>(null)
  const [rootAccessDialogError, setRootAccessDialogError] = useState<string | null>(null)
  const [isRootAccessSubmitting, setIsRootAccessSubmitting] = useState(false)
  const nativeRemoteDropTargetAtRef = useRef(0)
  const nativeDropConsumedAtRef = useRef(0)

  useEffect(() => {
    const markRemoteDropTarget = () => {
      nativeRemoteDropTargetAtRef.current = Date.now()
    }

    const markNativeRemoteDropTarget = (event: Event) => {
      const position = (event as CustomEvent<{ position?: { x?: number; y?: number } }>).detail?.position
      if (typeof position?.x !== 'number' || typeof position.y !== 'number') return
      const ratio = window.devicePixelRatio || 1
      const targets = [
        document.elementFromPoint(position.x, position.y),
        document.elementFromPoint(position.x / ratio, position.y / ratio)
      ]
      if (targets.some((target) => target?.closest('.remote-pane'))) {
        nativeRemoteDropTargetAtRef.current = Date.now()
      }
    }

    window.addEventListener('fileterm:tauri-remote-dragover', markRemoteDropTarget)
    window.addEventListener('fileterm:tauri-native-drag-over', markNativeRemoteDropTarget)
    return () => {
      window.removeEventListener('fileterm:tauri-remote-dragover', markRemoteDropTarget)
      window.removeEventListener('fileterm:tauri-native-drag-over', markNativeRemoteDropTarget)
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

  const openLocalDirectory = async (targetPath: string) => {
    if (!desktopApi) {
      setLocalPath(targetPath)
      return
    }

    const { path, items } = await desktopApi.listLocalDirectory(targetPath)
    setLocalPath(path)
    setLocalItems(withParentRow(path, items))
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
        await openLocalDirectory(item.path)
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
    operation: FileClipboardOperation,
    pane: FilePane,
    items: Array<LocalFileItem | RemoteFileItem>
  ) => {
    const normalizedItems = items
      .filter((item) => item.name !== '..')
      .map((item) => ({
        pane,
        path: item.path,
        name: item.name,
        type: item.type
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
      ? locale === 'zhCN'
        ? `已剪切 ${fileClipboard.items.length} 个文件，按 Esc 取消`
        : `Cut ${fileClipboard.items.length} files, press Esc to cancel`
      : locale === 'zhCN'
        ? `已复制 ${fileClipboard.items.length} 个文件，可在其他目录粘贴，按 Esc 取消`
        : `Copied ${fileClipboard.items.length} files, ready to paste in another folder. Press Esc to cancel`
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
          throw new Error(
            locale === 'zhCN'
              ? '暂不支持跨远程会话粘贴，请在原会话内操作或先下载到本地'
              : 'Cross-session remote paste is not supported. Paste in the original session or download locally first.'
          )
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
              const deleteSnapshot = await desktopApi.deleteRemotePath(fileClipboard.tabId!, item.path, item.type)
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

  const runFileAction = async (action: () => Promise<void>) => {
    try {
      onBusyChange(true)
      setIsFileActionSubmitting(true)
      await action()
      setFileActionDialog(null)
      setFileActionError(null)
    } catch (error) {
      reportOperationError(setFileActionError, '文件操作', error)
    } finally {
      setIsFileActionSubmitting(false)
      onBusyChange(false)
    }
  }

  const handleSubmitFileAction = async (rawValue: string) => {
    if (!desktopApi || !fileActionDialog) {
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
            const snapshot = await desktopApi.deleteRemotePath(activeTab.id, target.path, target.type)
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
    setFileActionError(null)
    setIsFileActionSubmitting(false)
    setFileActionDialog({ kind: 'new-folder', pane, directoryPath })
  }

  const requestNewFile = (pane: FilePane, directoryPath: string) => {
    setFileActionError(null)
    setIsFileActionSubmitting(false)
    setFileActionDialog({ kind: 'new-file', pane, directoryPath })
  }

  const requestRename = (pane: FilePane, item: LocalFileItem | RemoteFileItem) => {
    setFileActionError(null)
    setIsFileActionSubmitting(false)
    setFileActionDialog({
      kind: 'rename',
      target: { pane, path: item.path, name: item.name, type: item.type }
    })
  }

  const requestDelete = (pane: FilePane, items: Array<LocalFileItem | RemoteFileItem>) => {
    setFileActionError(null)
    setIsFileActionSubmitting(false)
    setFileActionDialog({
      kind: 'delete',
      targets: items.map((item) => ({ pane, path: item.path, name: item.name, type: item.type }))
    })
  }

  const dismissFileActionDialog = () => {
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
    if (!desktopApi || !permissionDialog) {
      return
    }

    if (permissionDialog.target.pane === 'remote' && !ensureActiveRemoteSessionConnected(setPermissionDialogError)) {
      return
    }

    try {
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
      onBusyChange(false)
    }
  }

  const dismissPermissionDialog = () => {
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
          const snapshot = await desktopApi.deleteRemotePath(activeTab.id, item.path, item.type)
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
    const handleNativeDrop = (event: Event) => {
      const detail = (event as CustomEvent).detail as {
        paths?: unknown
        position?: { x?: unknown; y?: unknown } | null
      }
      const paths = Array.isArray(detail?.paths)
        ? detail.paths.filter((path): path is string => typeof path === 'string' && path.length > 0)
        : []
      const position = detail?.position
      if (!paths.length || !position || typeof position.x !== 'number' || typeof position.y !== 'number') {
        return
      }

      // Finder dragover marks the remote pane explicitly. Keep both raw and
      // high-DPI-adjusted coordinate probes as a fallback: Tauri's macOS
      // position unit differs between WebKit/runtime versions.
      const targetMarkedByDragOver = Date.now() - nativeRemoteDropTargetAtRef.current < 1_500
      const ratio = window.devicePixelRatio || 1
      const targets = [
        document.elementFromPoint(position.x, position.y),
        document.elementFromPoint(position.x / ratio, position.y / ratio)
      ]
      if (!targetMarkedByDragOver && !targets.some((target) => target?.closest('.remote-pane'))) {
        return
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

    window.addEventListener('fileterm:tauri-native-drop', handleNativeDrop)
    return () => window.removeEventListener('fileterm:tauri-native-drop', handleNativeDrop)
  }, [activeSession, activeTab, desktopApi, onBusyChange, reportStatusError, uploadLocalPaths])

  const handleDropUpload = async (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault()
    const localPaths = extractDroppedLocalPaths(event, desktopApi)

    if (!localPaths.length || !desktopApi || !activeTab || !activeSession) {
      // The native event may have already started this Finder upload. Do not
      // overwrite that success path with a misleading "desktop only" notice
      // when WebKit follows it with an empty DOM FileList.
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
      const files = items.filter((row) => row.type === 'file')
      if (!files.length) {
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
        for (const item of files) {
          const snapshot = await desktopApi.downloadFile(activeTab.id, item.path, downloadDirectory)
          onApplySnapshot(snapshot)
        }
        await openLocalDirectory(downloadDirectory)
      } catch (error) {
        reportStatusError('下载文件', error, { targetPath: downloadDirectory ?? undefined })
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
        setRemoteDirectoryLoadingTabId(activeTab.id)
        setFileClipboard(null)
        await openLocalDirectory(localPath)
        await openRemoteDirectory(activeTab.id, activeSession.remotePath)
      } catch (error) {
        reportStatusError('刷新工作区', error, { targetPath: activeSession.remotePath })
      } finally {
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
      if (!activeSession.hasReusableSudoAuth) {
        setRootAccessDialogError(null)
        // 打开弹窗前重置 submitting，避免上一次提交卡死后残留的 loading
        // 状态污染新弹窗（用户报告"关闭重开连接还是卡 loading"正是此因）。
        setIsRootAccessSubmitting(false)
        setRootAccessDialog({
          tabId: activeTab.id,
          sshUser: activeProfile?.type === 'ssh' ? activeProfile.username : undefined,
          sudoUser: activeSession.sudoUser || 'root'
        })
        return
      }

      void (async () => {
        try {
          onBusyChange(true)
          setRootAccessDialogError(null)
          const snapshot = await desktopApi.setRemoteFileAccessMode(activeTab.id, nextMode)
          onApplySnapshot(snapshot)
        } catch (error) {
          if (shouldPromptForRootAccess(error)) {
            setRootAccessDialog({
              tabId: activeTab.id,
              sshUser: activeProfile?.type === 'ssh' ? activeProfile.username : undefined,
              sudoUser: activeSession.sudoUser || 'root'
            })
            reportOperationError(setRootAccessDialogError, '切换到 root 视角', error)
            return
          }
          reportStatusError('切换到 root 视角', error)
        } finally {
          onBusyChange(false)
        }
      })()
      return
    }

    void (async () => {
      try {
        onBusyChange(true)
        const snapshot = await desktopApi.setRemoteFileAccessMode(activeTab.id, nextMode)
        onApplySnapshot(snapshot)
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

  const handleConfirmRootAccess = ({ sudoUser, sudoPassword }: RootAccessCredentials) => {
    if (!desktopApi || !rootAccessDialog) {
      return
    }

    if (!workspace.sessions[rootAccessDialog.tabId]?.connected) {
      setRootAccessDialogError(t.remoteSessionDisconnectedAction)
      return
    }

    void (async () => {
      try {
        setIsRootAccessSubmitting(true)
        setRootAccessDialogError(null)
        const snapshot = await desktopApi.setRemoteFileAccessMode(rootAccessDialog.tabId, 'root', {
          sudoUser,
          sudoPassword
        })
        onApplySnapshot(snapshot)
        setRootAccessDialog(null)
        setRootAccessDialogError(null)
      } catch (error) {
        reportOperationError(setRootAccessDialogError, '切换到 root 视角', error)
      } finally {
        setIsRootAccessSubmitting(false)
      }
    })()
  }

  const dismissRootAccessDialog = () => {
    setRootAccessDialog(null)
    setRootAccessDialogError(null)
    // 必须重置 submitting：后端 sudo 验证可能因网络/超时未 reject
    // Promise，finally 不执行，loading 残留导致重开弹窗仍卡 spinner。
    // 即便后端已修复超时，dismiss 时也应主动清理，保证状态自愈。
    setIsRootAccessSubmitting(false)
  }

  return {
    remoteDirectoryLoadingTabId,
    isRemoteDirectoryLoading: remoteDirectoryLoadingTabId === activeTab?.id,
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
    rootAccessDialog,
    rootAccessDialogError,
    isRootAccessSubmitting,
    remoteFileAccessMode: activeSession?.fileAccessMode ?? 'user',
    openLocalDirectory,
    openRemoteDirectory,
    refreshCurrentPane,
    handleOpenLocalItem,
    handleOpenLocalPath,
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
    handleRefreshWorkspace,
    handleToggleRemoteFileAccessMode,
    handleToggleFollowShellCwd,
    handleConfirmRootAccess,
    dismissRootAccessDialog
  }
}

export type UseFileOperationsResult = ReturnType<typeof useFileOperations>
