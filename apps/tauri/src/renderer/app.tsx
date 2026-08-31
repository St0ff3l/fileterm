import type { ConnectionFormMode, FileContentSnapshot } from '@fileterm/core'
import { useAppDataOperations } from './hooks/use-app-data-operations'
import { useAppResize } from './hooks/use-app-resize'
import { useAppShellState, type InitialUiPreferences } from './hooks/use-app-shell-state'
import { useAppWorkspace } from './hooks/use-app-workspace'
import { AppView } from './features/layout/app-view'
import { resolveRendererPlatform } from './lib/renderer-platform'

export function App({ initialUiPreferences }: { initialUiPreferences?: InitialUiPreferences } = {}) {
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
  const formWindowCommand = searchParams.get('command') ?? ''

  const fileEditorWindowSource = searchParams.get('source') as FileContentSnapshot['source'] | null
  const fileEditorWindowPath = searchParams.get('path')
  const fileEditorWindowName = searchParams.get('name')
  const fileEditorWindowTabId = searchParams.get('tabId')
  const fileEditorWindowEncoding = searchParams.get('encoding') ?? 'utf-8'

  const desktopApi = window.fileterm
  const rendererPlatform = resolveRendererPlatform(desktopApi?.platform ?? 'browser')
  const isWindowsDesktop = rendererPlatform === 'win32'
  // Linux uses the same renderer-owned chrome as Windows. macOS remains on
  // AppKit chrome because its traffic lights are intentionally native.
  const usesCustomWindowChrome = isWindowsDesktop || rendererPlatform === 'linux'

  const shell = useAppShellState({
    searchParams,
    initialUiPreferences,
    desktopApi,
    isConnectionFormWindow,
    isMainWorkspaceWindow,
    isConnectionManagerWindow,
    rendererPlatform
  })

  const workspaceState = useAppWorkspace({
    desktopApi,
    workspace: shell.workspace,
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
    hasLoadedInitialSnapshot: shell.hasLoadedInitialSnapshot,
    locale: shell.locale,
    connectionDefaults: shell.connectionDefaults,
    filePanelRememberRatio: shell.filePanelRememberRatio,
    hasLoadedFilePanelRatios: shell.hasLoadedFilePanelRatios,
    filePanelHeights: shell.filePanelHeights,
    setFilePanelHeights: shell.setFilePanelHeights,
    filePanelRatios: shell.filePanelRatios,
    setFilePanelRatios: shell.setFilePanelRatios,
    filePanelSnapTargets: shell.filePanelSnapTargets,
    setFilePanelSnapTargets: shell.setFilePanelSnapTargets,
    commandPaneWidths: shell.commandPaneWidths,
    setCommandPaneWidths: shell.setCommandPaneWidths,
    workspaceFocusModes: shell.workspaceFocusModes,
    setWorkspaceFocusModes: shell.setWorkspaceFocusModes,
    workspaceViews: shell.workspaceViews,
    setWorkspaceViews: shell.setWorkspaceViews,
    resourceMonitoringMetrics: shell.resourceMonitoringMetrics,
    resourceMonitoringMetricOrder: shell.resourceMonitoringMetricOrder,
    isAiCopilotOpen: shell.isAiCopilotOpen,
    isBusy: shell.isBusy,
    setIsBusy: shell.setIsBusy,
    localPath: shell.localPath,
    localItems: shell.localItems,
    setLocalPath: shell.setLocalPath,
    setLocalItems: shell.setLocalItems,
    setIsLocalDirectoryLoading: shell.setIsLocalDirectoryLoading,
    closeActiveRequestVersion: shell.closeActiveRequestVersion,
    newTabRequestVersion: shell.newTabRequestVersion,
    splitPaneRequest: shell.splitPaneRequest,
    paneFocusRequest: shell.paneFocusRequest,
    applySnapshot: shell.applySnapshot,
    onError: shell.reportError,
    onStatusMessage: (message: string) => shell.setError(message),
    formatError: shell.formatAppError,
    closeCurrentWindow: shell.closeCurrentWindow,
    requestQuitApp: shell.requestQuitApp
  })

  const resizeState = useAppResize({
    isHomeWorkspaceVisible: workspaceState.isHomeWorkspaceVisible,
    isResizingSidebar: shell.isResizingSidebar,
    setIsResizingSidebar: shell.setIsResizingSidebar,
    setSidebarWidth: shell.setSidebarWidth,
    isResizingAiCopilot: shell.isResizingAiCopilot,
    setIsResizingAiCopilot: shell.setIsResizingAiCopilot,
    isSystemSidebarCollapsed: workspaceState.isSystemSidebarCollapsed,
    sidebarWidth: shell.sidebarWidth,
    setAiCopilotWidth: shell.setAiCopilotWidth,
    shouldShowAiCopilot: workspaceState.shouldShowAiCopilot,
    error: shell.error,
    setError: shell.setError,
    windowCloseRequest: shell.windowCloseRequest,
    workspace: shell.workspace,
    requestWindowCloseConfirmation: workspaceState.requestWindowCloseConfirmation,
    clearWindowCloseRequest: shell.clearWindowCloseRequest
  })

  const dataOperations = useAppDataOperations({
    desktopApi,
    isCommandFormWindow,
    isConnectionFormWindow,
    form: workspaceState.form,
    setFormError: workspaceState.setFormError,
    editingProfileId: workspaceState.editingProfileId,
    isBusy: shell.isBusy,
    setIsBusy: shell.setIsBusy,
    applySnapshot: shell.applySnapshot,
    closeCurrentWindow: shell.closeCurrentWindow,
    closeConnectionForm: workspaceState.closeConnectionForm,
    setError: shell.setError,
    formatError: shell.formatAppError,
    onError: shell.reportError,
    waitForSshInteractionListener: workspaceState.waitForSshInteractionListener,
    activePaneTab: workspaceState.activePaneTab,
    visibleWorkspaceTabs: workspaceState.visibleWorkspaceTabs,
    sessionSendTargets: workspaceState.sessionSendTargets
  })

  return (
    <AppView
      model={{
        route: {
          isConnectionManagerWindow,
          isCommandManagerWindow,
          isConnectionFormWindow,
          isCommandFormWindow,
          isFileEditorWindow,
          isMainWorkspaceWindow,
          formWindowMode,
          formWindowProfileId,
          formWindowCommandId,
          formWindowFolderId,
          formWindowCommand,
          fileEditorWindowSource,
          fileEditorWindowPath,
          fileEditorWindowName,
          fileEditorWindowTabId,
          fileEditorWindowEncoding
        },
        shell,
        workspace: workspaceState,
        data: dataOperations,
        resize: resizeState,
        isWindowsDesktop,
        usesCustomWindowChrome
      }}
    />
  )
}
