import type { ConnectionFormMode, FileContentSnapshot } from '@fileterm/core'
import type { useAppDataOperations } from '../../hooks/use-app-data-operations'
import type { useAppResize } from '../../hooks/use-app-resize'
import type { useAppShellState } from '../../hooks/use-app-shell-state'
import type { useAppWorkspace } from '../../hooks/use-app-workspace'

export type AppRouteState = {
  isConnectionManagerWindow: boolean
  isCommandManagerWindow: boolean
  isConnectionFormWindow: boolean
  isCommandFormWindow: boolean
  isFileEditorWindow: boolean
  isMainWorkspaceWindow: boolean
  formWindowMode: ConnectionFormMode
  formWindowProfileId: string | null
  formWindowCommandId: string | null
  formWindowFolderId: string | null
  formWindowCommand: string
  fileEditorWindowSource: FileContentSnapshot['source'] | null
  fileEditorWindowPath: string | null
  fileEditorWindowName: string | null
  fileEditorWindowTabId: string | null
  fileEditorWindowEncoding: string
}

export type AppViewModel = {
  route: AppRouteState
  shell: ReturnType<typeof useAppShellState>
  workspace: ReturnType<typeof useAppWorkspace>
  data: ReturnType<typeof useAppDataOperations>
  resize: ReturnType<typeof useAppResize>
  isWindowsDesktop: boolean
  usesCustomWindowChrome: boolean
}
