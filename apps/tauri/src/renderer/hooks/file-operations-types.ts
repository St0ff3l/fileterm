import type { Dispatch, MutableRefObject, SetStateAction } from 'react'
import type {
  ConnectionProfile,
  FileTermDesktopApi,
  LocalFileItem,
  RemoteFileItem,
  SessionSnapshot,
  WorkspaceSnapshot,
  WorkspaceTab
} from '@fileterm/core'
import type { AppLocale } from '../i18n'

export type FilePane = 'local' | 'remote'
export type FileClipboardOperation = 'copy' | 'cut'

export type FileDialogTarget = {
  pane: FilePane
  path: string
  name: string
  type: 'file' | 'folder'
  isSymlink?: boolean
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
  rootAccessMethod?: 'sudo' | 'su'
  sudoUser: string
  hasSavedSudoPassword?: boolean
  hasSavedSuPassword?: boolean
}

export type LocalNetworkCredentialsDialogState = {
  path: string
}

export type LocalNetworkShareDialogState = {
  path: string
  username: string
  password: string
  shares: string[]
}

export type LocalNetworkShareSource = {
  mountPath: string
  remotePath: string
  hostPath: string
  shares: string[]
  username: string
  password: string
}

export type RootAccessCredentials = {
  rootAccessMethod: 'sudo' | 'su'
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
  setIsLocalDirectoryLoading: Dispatch<SetStateAction<boolean>>
  onApplySnapshot(snapshot: WorkspaceSnapshot): void
  onBusyChange(isBusy: boolean): void
  onStatusMessage(message: string): void
  formatError(scope: string, error: unknown, details?: FileOperationErrorDetails): string
  openLocalFile(item: LocalFileItem): unknown | Promise<unknown>
  openRemoteFile(tabId: string, item: RemoteFileItem, locale: AppLocale): unknown | Promise<unknown>
}

export type FileOperationsRuntime = UseFileOperationsOptions & {
  remoteDirectoryLoadingTabId: string | null
  setRemoteDirectoryLoadingTabId: Dispatch<SetStateAction<string | null>>
  fileActionDialog: FileActionDialog | null
  setFileActionDialog: Dispatch<SetStateAction<FileActionDialog | null>>
  fileActionError: string | null
  setFileActionError: Dispatch<SetStateAction<string | null>>
  isFileActionSubmitting: boolean
  setIsFileActionSubmitting: Dispatch<SetStateAction<boolean>>
  fileActionSubmittingRef: MutableRefObject<boolean>
  fileClipboard: FileClipboardState | null
  setFileClipboard: Dispatch<SetStateAction<FileClipboardState | null>>
  permissionDialog: PermissionDialogState | null
  setPermissionDialog: Dispatch<SetStateAction<PermissionDialogState | null>>
  permissionDialogError: string | null
  setPermissionDialogError: Dispatch<SetStateAction<string | null>>
  isPermissionSubmitting: boolean
  setIsPermissionSubmitting: Dispatch<SetStateAction<boolean>>
  permissionSubmittingRef: MutableRefObject<boolean>
  rootAccessDialog: RootAccessDialogState | null
  setRootAccessDialog: Dispatch<SetStateAction<RootAccessDialogState | null>>
  rootAccessDialogError: string | null
  setRootAccessDialogError: Dispatch<SetStateAction<string | null>>
  isRootAccessSubmitting: boolean
  setIsRootAccessSubmitting: Dispatch<SetStateAction<boolean>>
  rootAccessSubmittingRef: MutableRefObject<boolean>
  localNetworkCredentialsDialog: LocalNetworkCredentialsDialogState | null
  setLocalNetworkCredentialsDialog: Dispatch<SetStateAction<LocalNetworkCredentialsDialogState | null>>
  localNetworkCredentialsDialogError: string | null
  setLocalNetworkCredentialsDialogError: Dispatch<SetStateAction<string | null>>
  localNetworkShareDialog: LocalNetworkShareDialogState | null
  setLocalNetworkShareDialog: Dispatch<SetStateAction<LocalNetworkShareDialogState | null>>
  localNetworkShareDialogError: string | null
  setLocalNetworkShareDialogError: Dispatch<SetStateAction<string | null>>
  localNetworkShareSource: LocalNetworkShareSource | null
  setLocalNetworkShareSource: Dispatch<SetStateAction<LocalNetworkShareSource | null>>
  isWorkspaceRefreshing: boolean
  setIsWorkspaceRefreshing: Dispatch<SetStateAction<boolean>>
  isLocalNetworkCredentialsSubmitting: boolean
  setIsLocalNetworkCredentialsSubmitting: Dispatch<SetStateAction<boolean>>
  localNetworkCredentialsSubmittingRef: MutableRefObject<boolean>
  nativeRemoteDropTargetAtRef: MutableRefObject<number>
  nativeDropConsumedAtRef: MutableRefObject<number>
  reportOperationError(
    setter: (message: string) => void,
    scope: string,
    error: unknown,
    details?: FileOperationErrorDetails
  ): void
  reportStatusError(scope: string, error: unknown, details?: FileOperationErrorDetails): void
  ensureActiveRemoteSessionConnected(setter?: (message: string) => void): boolean
}
