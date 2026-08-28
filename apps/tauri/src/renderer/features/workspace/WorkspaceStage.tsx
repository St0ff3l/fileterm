import type {
  CommandExecutionOptions,
  CommandFolder,
  CommandTemplate,
  CommandTemplateInput,
  ConnectionFolder,
  ConnectionProfile,
  McpAgentClientStatus,
  LocalFileItem,
  OverviewSectionId,
  RemoteFileItem,
  SavedTheme,
  SessionSnapshot,
  ThemeConfig,
  WorkspaceTab
} from '@fileterm/core'
import type { Dispatch, DragEvent, SetStateAction } from 'react'
import type { SendScope, SessionSendTarget } from '../common/session-send-targets'
import type { TabBarProps } from '../layout/TabBar'
import { SystemInfoWorkspace } from '../system/SystemInfoWorkspace'
import { HomeWorkspace } from './HomeWorkspace'
import { LocalTerminalWorkspace } from './LocalTerminalWorkspace'
import { SessionWorkspace } from './SessionWorkspace'
import { DEFAULT_FILE_PANEL_SNAP_TARGET, type FilePanelSnapTarget } from './file-panel-snap'

const DEFAULT_FILE_PANEL_HEIGHT = 218
const DEFAULT_FILE_PANEL_RATIO = 30
const DEFAULT_COMMAND_PANE_WIDTH = 300

type ActiveLocalTab = {
  kind: 'home' | 'system'
  sessionTabId?: string
} | null

export function WorkspaceStage({
  activeLocalTab,
  activeHomeTabId,
  activeProfile,
  activeSession,
  activeTab,
  terminalActiveTab,
  terminalActiveSession,
  splitRootTab,
  activeView,
  onActiveViewChange,
  commandPaneWidth,
  onCommandPaneWidthChange,
  filePanelHeight,
  onFilePanelHeightChange,
  filePanelRatio,
  onFilePanelRatioCommit,
  filePanelSnapTarget,
  onFilePanelSnapTargetCommit,
  rememberFilePanelRatio,
  sendTargets,
  terminalDockSendScope,
  terminalDockSelectedTabIds,
  commandFolders,
  commandTemplates,
  folders,
  isBusy,
  localItems,
  localPath,
  localPanePath,
  isLocalNetworkShare,
  isLocalDirectoryLoading,
  isWorkspaceRefreshing,
  isWorkspaceSwitching,
  canPasteToLocal,
  canPasteToRemote,
  clipboardStatusText,
  localCutPaths,
  remoteCutPaths,
  profiles,
  theme,
  themeConfig,
  customThemes,
  locale,
  overviewShowStats,
  overviewShowRecent,
  overviewShowAllConnections,
  overviewShowQuickActions,
  overviewSectionOrder,
  onCopyItems,
  onCutItems,
  onClearCutState,
  onExecuteCommand,
  onSendTerminalCommand,
  onSaveTemporaryCommand,
  onTerminalDockSendScopeChange,
  onTerminalDockSelectedTabIdsChange,
  onOpenCommandManager,
  onChooseUploadFiles,
  onDownloadFiles,
  onDownloadLocalNetworkFiles,
  onDropUpload,
  onOpenLocalItem,
  onOpenLocalPath,
  onBackToLocalComputer,
  onOpenProfile,
  onOpenLocalTerminal,
  onReconnectLocalTerminal,
  onOpenRemoteItem,
  onOpenRemotePath,
  onPasteIntoPane,
  onRequestChangePermissions,
  onRequestDelete,
  onRequestNewFile,
  onRequestNewFolder,
  onRequestQuickDelete,
  onRequestRename,
  onToggleFollowShellCwd,
  onToggleRemoteFileAccessMode,
  remoteFileAccessMode,
  isRemoteDirectoryLoading,
  onRefresh,
  onUploadFiles,
  onCreateConnection,
  onEditConnection,
  onDeleteConnection,
  onCreateConnectionFolder,
  onDeleteConnectionFolder,
  onUpdateConnectionFolder,
  onUpdateConnectionOrder,
  onImportConnections,
  onExportConnections,
  onCreateCommand,
  onUpdateCommand,
  onDeleteCommand,
  onCreateCommandFolder,
  onDeleteCommandFolder,
  onUpdateCommandFolder,
  onUpdateCommandOrder,
  onSetTheme,
  onSetThemeConfig,
  onSetCustomThemes,
  onSetLocale,
  onOpenLogsDirectory,
  onLaunchLocalAgent,
  isSidebarCollapsed,
  isWorkspaceFocusMode,
  isWorkspaceActive = true,
  tabBarProps,
  isResizingSidebar,
  onResizeStart,
  sessions,
  activePaneTabId,
  onClosePane,
  onCloseTab,
  onSplitPane,
  onActivatePane,
  onSetPaneWeights
}: {
  activeLocalTab: ActiveLocalTab
  activeHomeTabId: string | null
  activeProfile: ConnectionProfile | null
  activeSession: SessionSnapshot | null
  activeTab: WorkspaceTab | null
  terminalActiveTab: WorkspaceTab | null
  terminalActiveSession: SessionSnapshot | null
  splitRootTab?: WorkspaceTab
  activeView: 'file' | 'command' | 'tunnel'
  onActiveViewChange(view: 'file' | 'command' | 'tunnel'): void
  commandPaneWidth: number
  onCommandPaneWidthChange(width: number): void
  filePanelHeight: number
  onFilePanelHeightChange: Dispatch<SetStateAction<number>>
  filePanelRatio: number
  onFilePanelRatioCommit(ratio: number): void
  filePanelSnapTarget: FilePanelSnapTarget | null
  onFilePanelSnapTargetCommit(target: FilePanelSnapTarget | null): void
  rememberFilePanelRatio: boolean
  sendTargets: SessionSendTarget[]
  terminalDockSendScope: SendScope
  terminalDockSelectedTabIds: string[]
  commandFolders: CommandFolder[]
  commandTemplates: CommandTemplate[]
  folders: ConnectionFolder[]
  isBusy: boolean
  localItems: LocalFileItem[]
  localPath: string
  localPanePath: string
  isLocalNetworkShare: boolean
  isLocalDirectoryLoading: boolean
  isWorkspaceRefreshing: boolean
  isWorkspaceSwitching: boolean
  canPasteToLocal: boolean
  canPasteToRemote: boolean
  clipboardStatusText: string | null
  localCutPaths: string[]
  remoteCutPaths: string[]
  profiles: ConnectionProfile[]
  theme: 'default-dark' | 'default-light'
  themeConfig: ThemeConfig
  customThemes: SavedTheme[]
  locale: 'zhCN' | 'enUS'
  overviewShowStats: boolean
  overviewShowRecent: boolean
  overviewShowAllConnections: boolean
  overviewShowQuickActions: boolean
  overviewSectionOrder: OverviewSectionId[]
  onCopyItems(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onCutItems(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onClearCutState(): void
  onExecuteCommand(
    commandId: string,
    args: string[],
    options: CommandExecutionOptions,
    scope: SendScope,
    selectedTabIds: string[]
  ): void
  onSendTerminalCommand(command: string): Promise<void>
  onSaveTemporaryCommand(command: string, appendCarriageReturn: boolean): Promise<boolean> | boolean | void
  onUpdateCommand(commandId: string, input: CommandTemplateInput): Promise<boolean> | boolean | void
  onTerminalDockSendScopeChange(scope: SendScope, rememberSelection: boolean): void
  onTerminalDockSelectedTabIdsChange(tabIds: string[], rememberSelection: boolean): void
  onOpenCommandManager(): void
  onChooseUploadFiles(): void
  onDownloadFiles(items: RemoteFileItem[], targetDirectory?: string): void
  onDownloadLocalNetworkFiles(items: LocalFileItem[]): void
  onDropUpload(event: DragEvent<HTMLDivElement>): void
  onOpenLocalItem(item: LocalFileItem): void
  onOpenLocalPath(path: string): void
  onBackToLocalComputer(): void
  onOpenProfile(profileId: string): void
  onOpenLocalTerminal(): void
  onReconnectLocalTerminal(tabId: string): Promise<void>
  onOpenRemoteItem(item: RemoteFileItem): void
  onOpenRemotePath(path: string): void
  onPasteIntoPane(pane: 'local' | 'remote'): void
  onRequestChangePermissions(pane: 'local' | 'remote', item: LocalFileItem | RemoteFileItem): void
  onRequestDelete(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onRequestNewFile(pane: 'local' | 'remote', directoryPath: string): void
  onRequestNewFolder(pane: 'local' | 'remote', directoryPath: string): void
  onRequestQuickDelete(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onRequestRename(pane: 'local' | 'remote', item: LocalFileItem | RemoteFileItem): void
  onToggleFollowShellCwd(): void
  onToggleRemoteFileAccessMode(): void
  remoteFileAccessMode: 'user' | 'root'
  isRemoteDirectoryLoading: boolean
  onRefresh(): void
  onUploadFiles(items: LocalFileItem[]): void
  onCreateConnection(): void
  onEditConnection(profile: ConnectionProfile): void
  onDeleteConnection(profileId: string): Promise<boolean> | boolean | void
  onCreateConnectionFolder(name: string): Promise<boolean> | boolean | void
  onDeleteConnectionFolder(folderId: string): Promise<boolean> | boolean | void
  onUpdateConnectionFolder(folderId: string, updates: Partial<ConnectionFolder>): Promise<boolean> | boolean | void
  onUpdateConnectionOrder(
    id: string,
    newParentId: string | undefined,
    newOrder: number
  ): Promise<boolean> | boolean | void
  onImportConnections(source?: 'files' | 'folder'): void
  onExportConnections(): void
  onCreateCommand(input: CommandTemplateInput): Promise<boolean> | boolean | void
  onUpdateCommand(commandId: string, input: CommandTemplateInput): Promise<boolean> | boolean | void
  onDeleteCommand(commandId: string): Promise<boolean> | boolean | void
  onCreateCommandFolder(name: string): Promise<boolean> | boolean | void
  onDeleteCommandFolder(folderId: string): Promise<boolean> | boolean | void
  onUpdateCommandFolder(folderId: string, updates: Partial<CommandFolder>): Promise<boolean> | boolean | void
  onUpdateCommandOrder(id: string, newParentId: string | undefined, newOrder: number): Promise<boolean> | boolean | void
  onSetTheme(value: 'default-dark' | 'default-light'): void
  onSetThemeConfig(value: ThemeConfig): void
  onSetCustomThemes(value: SavedTheme[]): void
  onSetLocale(value: 'zhCN' | 'enUS'): void
  onOpenLogsDirectory(): void
  onLaunchLocalAgent?(client: McpAgentClientStatus): void
  isSidebarCollapsed: boolean
  isWorkspaceFocusMode: boolean
  isWorkspaceActive?: boolean
  tabBarProps: Omit<TabBarProps, 'homeBrandContent'>
  isResizingSidebar: boolean
  onResizeStart(): void
  sessions: Record<string, SessionSnapshot>
  activePaneTabId?: string
  onClosePane(paneTabId: string): void
  onCloseTab(): void
  onSplitPane(paneTabId: string, direction: 'row' | 'column'): void
  onActivatePane(paneTabId: string): void
  onSetPaneWeights(panePath: number[], weights: number[]): void
}) {
  if (activeLocalTab?.kind === 'system') {
    return (
      <SystemInfoWorkspace
        activeProfile={activeProfile}
        activeSession={activeSession}
        connectionStatus={activeTab?.status ?? null}
      />
    )
  }

  if (activeTab?.sessionType === 'local' && activeSession && !activeLocalTab) {
    return (
      <LocalTerminalWorkspace
        activeSession={activeSession}
        activeTab={activeTab}
        isActive={isWorkspaceActive}
        onCloseTab={onCloseTab}
        onRestart={onReconnectLocalTerminal}
        splitRootTab={splitRootTab}
        splitPaneSessions={sessions}
        activePaneTabId={activePaneTabId}
        onClosePane={onClosePane}
        onSplitPane={onSplitPane}
        onActivatePane={onActivatePane}
        onSetPaneWeights={onSetPaneWeights}
      />
    )
  }

  if (activeTab && activeSession && !activeLocalTab) {
    return (
      <SessionWorkspace
        activeSession={activeSession}
        activeTab={activeTab}
        terminalActiveTab={terminalActiveTab ?? activeTab}
        terminalActiveSession={terminalActiveSession ?? activeSession}
        splitRootTab={splitRootTab}
        splitPaneSessions={sessions}
        activePaneTabId={activePaneTabId}
        onClosePane={onClosePane}
        onCloseTab={onCloseTab}
        onSplitPane={onSplitPane}
        onActivatePane={onActivatePane}
        onSetPaneWeights={onSetPaneWeights}
        activeView={activeView}
        onActiveViewChange={onActiveViewChange}
        commandPaneWidth={commandPaneWidth}
        onCommandPaneWidthChange={onCommandPaneWidthChange}
        filePanelHeight={filePanelHeight}
        onFilePanelHeightChange={onFilePanelHeightChange}
        filePanelRatio={filePanelRatio}
        onFilePanelRatioCommit={onFilePanelRatioCommit}
        filePanelSnapTarget={filePanelSnapTarget}
        onFilePanelSnapTargetCommit={onFilePanelSnapTargetCommit}
        rememberFilePanelRatio={rememberFilePanelRatio}
        sendTargets={sendTargets}
        terminalDockSendScope={terminalDockSendScope}
        terminalDockSelectedTabIds={terminalDockSelectedTabIds}
        commandFolders={commandFolders}
        commandTemplates={commandTemplates}
        isBusy={isBusy}
        localItems={localItems}
        localPath={localPath}
        localPanePath={localPanePath}
        isLocalNetworkShare={isLocalNetworkShare}
        isLocalDirectoryLoading={isLocalDirectoryLoading}
        isWorkspaceRefreshing={isWorkspaceRefreshing}
        isWorkspaceSwitching={isWorkspaceSwitching}
        canPasteToLocal={canPasteToLocal}
        canPasteToRemote={canPasteToRemote}
        clipboardStatusText={clipboardStatusText}
        localCutPaths={localCutPaths}
        remoteCutPaths={remoteCutPaths}
        onCopyItems={onCopyItems}
        onCutItems={onCutItems}
        onClearCutState={onClearCutState}
        onExecuteCommand={onExecuteCommand}
        onSendTerminalCommand={onSendTerminalCommand}
        onSaveTemporaryCommand={onSaveTemporaryCommand}
        onUpdateCommand={onUpdateCommand}
        onTerminalDockSendScopeChange={onTerminalDockSendScopeChange}
        onTerminalDockSelectedTabIdsChange={onTerminalDockSelectedTabIdsChange}
        onOpenCommandManager={onOpenCommandManager}
        onChooseUploadFiles={onChooseUploadFiles}
        onDownloadFiles={onDownloadFiles}
        onDownloadLocalNetworkFiles={onDownloadLocalNetworkFiles}
        onDropUpload={onDropUpload}
        onOpenLocalItem={onOpenLocalItem}
        onOpenLocalPath={onOpenLocalPath}
        onBackToLocalComputer={onBackToLocalComputer}
        onOpenRemoteItem={onOpenRemoteItem}
        onOpenRemotePath={onOpenRemotePath}
        onPasteIntoPane={onPasteIntoPane}
        onRequestChangePermissions={onRequestChangePermissions}
        onRequestDelete={onRequestDelete}
        onRequestNewFile={onRequestNewFile}
        onRequestNewFolder={onRequestNewFolder}
        onRequestQuickDelete={onRequestQuickDelete}
        onRequestRename={onRequestRename}
        onToggleFollowShellCwd={onToggleFollowShellCwd}
        onToggleRemoteFileAccessMode={onToggleRemoteFileAccessMode}
        remoteFileAccessMode={remoteFileAccessMode}
        isRemoteDirectoryLoading={isRemoteDirectoryLoading}
        onRefresh={onRefresh}
        onUploadFiles={onUploadFiles}
        isWorkspaceFocusMode={isWorkspaceFocusMode}
        isActive={isWorkspaceActive}
      />
    )
  }

  return (
    <HomeWorkspace
      key={activeHomeTabId ?? 'home-root'}
      folders={folders}
      commandFolders={commandFolders}
      commandTemplates={commandTemplates}
      theme={theme}
      themeConfig={themeConfig}
      customThemes={customThemes}
      locale={locale}
      overviewShowStats={overviewShowStats}
      overviewShowRecent={overviewShowRecent}
      overviewShowAllConnections={overviewShowAllConnections}
      overviewShowQuickActions={overviewShowQuickActions}
      overviewSectionOrder={overviewSectionOrder}
      onOpen={onOpenProfile}
      onOpenLocalTerminal={onOpenLocalTerminal}
      onCreateConnection={onCreateConnection}
      onEditConnection={onEditConnection}
      onDeleteConnection={onDeleteConnection}
      onCreateConnectionFolder={onCreateConnectionFolder}
      onDeleteConnectionFolder={onDeleteConnectionFolder}
      onUpdateConnectionFolder={onUpdateConnectionFolder}
      onUpdateConnectionOrder={onUpdateConnectionOrder}
      onImportConnections={onImportConnections}
      onExportConnections={onExportConnections}
      onCreateCommand={onCreateCommand}
      onUpdateCommand={onUpdateCommand}
      onDeleteCommand={onDeleteCommand}
      onCreateCommandFolder={onCreateCommandFolder}
      onDeleteCommandFolder={onDeleteCommandFolder}
      onUpdateCommandFolder={onUpdateCommandFolder}
      onUpdateCommandOrder={onUpdateCommandOrder}
      onSetTheme={onSetTheme}
      onSetThemeConfig={onSetThemeConfig}
      onSetCustomThemes={onSetCustomThemes}
      onSetLocale={onSetLocale}
      onOpenLogsDirectory={onOpenLogsDirectory}
      onLaunchLocalAgent={onLaunchLocalAgent}
      isSidebarCollapsed={isSidebarCollapsed}
      profiles={profiles}
      tabBarProps={tabBarProps}
      isResizingSidebar={isResizingSidebar}
      onResizeStart={onResizeStart}
    />
  )
}

type WorkspaceStageProps = Parameters<typeof WorkspaceStage>[0]

type KeepAliveWorkspaceStageProps = WorkspaceStageProps & {
  workspaceTabs: WorkspaceTab[]
  filePanelHeights: Record<string, number>
  filePanelRatios: Record<string, number>
  filePanelSnapTargets: Record<string, FilePanelSnapTarget | null>
  commandPaneWidths: Record<string, number>
  workspaceFocusModes: Record<string, boolean>
  workspaceViews: Record<string, 'file' | 'command' | 'tunnel'>
}

/**
 * Keep every top-level terminal workspace mounted while another tab is shown.
 *
 * A terminal TUI owns state that is not reconstructible from the session
 * transcript: alternate-screen contents, cursor position, selection, scrollback
 * and pending ANSI writes. The stack only changes which surface is visible;
 * the xterm instance and its PTY event subscription stay attached to its tab.
 */
export function KeepAliveWorkspaceStage({
  workspaceTabs,
  filePanelHeights,
  filePanelRatios,
  filePanelSnapTargets,
  commandPaneWidths,
  workspaceFocusModes,
  workspaceViews,
  ...stageProps
}: KeepAliveWorkspaceStageProps) {
  const sessionWorkspaceItems = workspaceTabs.map((workspaceTab) => {
    const session = stageProps.sessions[workspaceTab.id]
    if (!session) {
      return null
    }

    const isActive =
      !stageProps.activeLocalTab && stageProps.activeTab?.id === workspaceTab.id && stageProps.activeSession !== null
    const isCurrentTab = isActive
    const tabTerminal = isCurrentTab ? (stageProps.terminalActiveTab ?? workspaceTab) : workspaceTab
    const tabTerminalSession = isCurrentTab ? (stageProps.terminalActiveSession ?? session) : session
    const tabFilePanelHeight = isCurrentTab
      ? stageProps.filePanelHeight
      : (filePanelHeights[workspaceTab.id] ?? DEFAULT_FILE_PANEL_HEIGHT)
    const tabFilePanelRatio = isCurrentTab
      ? stageProps.filePanelRatio
      : stageProps.rememberFilePanelRatio
        ? (filePanelRatios[workspaceTab.profileId] ?? DEFAULT_FILE_PANEL_RATIO)
        : DEFAULT_FILE_PANEL_RATIO
    const hasStoredFilePanelRatio = Object.prototype.hasOwnProperty.call(filePanelRatios, workspaceTab.profileId)
    const hasStoredFilePanelSnapTarget = Object.prototype.hasOwnProperty.call(
      filePanelSnapTargets,
      workspaceTab.profileId
    )
    const tabFilePanelSnapTarget = isCurrentTab
      ? stageProps.filePanelSnapTarget
      : !stageProps.rememberFilePanelRatio
        ? DEFAULT_FILE_PANEL_SNAP_TARGET
        : hasStoredFilePanelSnapTarget
          ? filePanelSnapTargets[workspaceTab.profileId]
          : hasStoredFilePanelRatio
            ? null
            : DEFAULT_FILE_PANEL_SNAP_TARGET
    const tabCommandPaneWidth = isCurrentTab
      ? stageProps.commandPaneWidth
      : (commandPaneWidths[workspaceTab.id] ?? DEFAULT_COMMAND_PANE_WIDTH)
    const tabWorkspaceFocusMode = isCurrentTab
      ? stageProps.isWorkspaceFocusMode
      : (workspaceFocusModes[workspaceTab.id] ?? false)

    return (
      <div
        key={workspaceTab.id}
        className={'workspace-content-stack__item ' + (isActive ? 'is-active' : '')}
        data-workspace-tab-id={workspaceTab.id}
        aria-hidden={!isActive}
      >
        <WorkspaceStage
          {...stageProps}
          activeLocalTab={null}
          activeSession={session}
          activeTab={workspaceTab}
          terminalActiveTab={tabTerminal}
          terminalActiveSession={tabTerminalSession}
          splitRootTab={workspaceTab.paneRoot ? workspaceTab : undefined}
          activeView={isCurrentTab ? stageProps.activeView : (workspaceViews[workspaceTab.id] ?? 'file')}
          commandPaneWidth={tabCommandPaneWidth}
          filePanelHeight={tabFilePanelHeight}
          filePanelRatio={tabFilePanelRatio}
          filePanelSnapTarget={tabFilePanelSnapTarget}
          activePaneTabId={isCurrentTab ? stageProps.activePaneTabId : undefined}
          isWorkspaceSwitching={isCurrentTab ? stageProps.isWorkspaceSwitching : false}
          isWorkspaceFocusMode={tabWorkspaceFocusMode}
          isWorkspaceActive={isActive}
        />
      </div>
    )
  })

  const showBaseWorkspace = stageProps.activeLocalTab !== null || !stageProps.activeTab || !stageProps.activeSession

  return (
    <div className="workspace-content-stack">
      {showBaseWorkspace ? (
        <div className="workspace-content-stack__item is-active" aria-hidden="false">
          <WorkspaceStage {...stageProps} />
        </div>
      ) : null}
      {sessionWorkspaceItems}
    </div>
  )
}
