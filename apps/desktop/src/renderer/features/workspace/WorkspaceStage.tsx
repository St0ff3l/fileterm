import type {
  CommandExecutionOptions,
  CommandFolder,
  CommandTemplate,
  ConnectionFolder,
  ConnectionProfile,
  LocalFileItem,
  RemoteFileItem,
  SessionSnapshot,
  WorkspaceTab
} from '@fileterm/core'
import type { DragEvent } from 'react'
import type { SendScope, SessionSendTarget } from '../common/session-send-targets'
import { SystemInfoWorkspace } from '../system/SystemInfoWorkspace'
import { HomeWorkspace } from './HomeWorkspace'
import { SessionWorkspace } from './SessionWorkspace'

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
  sendTargets,
  terminalDockSendScope,
  terminalDockSelectedTabIds,
  commandFolders,
  commandTemplates,
  folders,
  isBusy,
  localItems,
  localPath,
  canPasteToLocal,
  canPasteToRemote,
  clipboardStatusText,
  localCutPaths,
  remoteCutPaths,
  profiles,
  theme,
  locale,
  onCopyItems,
  onCutItems,
  onClearCutState,
  onExecuteCommand,
  onSendTerminalCommand,
  onTerminalDockSendScopeChange,
  onTerminalDockSelectedTabIdsChange,
  onOpenCommandManager,
  onChooseUploadFiles,
  onDownloadFiles,
  onDropUpload,
  onOpenLocalItem,
  onOpenLocalPath,
  onOpenProfile,
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
  onCreateCommand,
  onUpdateCommand,
  onDeleteCommand,
  onCreateCommandFolder,
  onDeleteCommandFolder,
  onUpdateCommandFolder,
  onUpdateCommandOrder,
  onSetTheme,
  onSetLocale,
  onOpenLogsDirectory,
  isSidebarCollapsed,
  isWorkspaceFocusMode,
  tabBarProps,
  isResizingSidebar,
  onResizeStart
}: {
  activeLocalTab: ActiveLocalTab
  activeHomeTabId: string | null
  activeProfile: ConnectionProfile | null
  activeSession: SessionSnapshot | null
  activeTab: WorkspaceTab | null
  sendTargets: SessionSendTarget[]
  terminalDockSendScope: SendScope
  terminalDockSelectedTabIds: string[]
  commandFolders: CommandFolder[]
  commandTemplates: CommandTemplate[]
  folders: ConnectionFolder[]
  isBusy: boolean
  localItems: LocalFileItem[]
  localPath: string
  canPasteToLocal: boolean
  canPasteToRemote: boolean
  clipboardStatusText: string | null
  localCutPaths: string[]
  remoteCutPaths: string[]
  profiles: ConnectionProfile[]
  theme: 'default-dark' | 'default-light'
  locale: 'zhCN' | 'enUS'
  onCopyItems(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onCutItems(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onClearCutState(): void
  onExecuteCommand(commandId: string, args: string[], options: CommandExecutionOptions, scope: SendScope, selectedTabIds: string[]): void
  onSendTerminalCommand(command: string): Promise<void>
  onTerminalDockSendScopeChange(scope: SendScope, rememberSelection: boolean): void
  onTerminalDockSelectedTabIdsChange(tabIds: string[], rememberSelection: boolean): void
  onOpenCommandManager(): void
  onChooseUploadFiles(): void
  onDownloadFiles(items: RemoteFileItem[], targetDirectory?: string): void
  onDropUpload(event: DragEvent<HTMLDivElement>): void
  onOpenLocalItem(item: LocalFileItem): void
  onOpenLocalPath(path: string): void
  onOpenProfile(profileId: string): void
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
  onDeleteConnection(profileId: string): void
  onCreateConnectionFolder(name: string): void
  onDeleteConnectionFolder(folderId: string): void
  onUpdateConnectionFolder(folderId: string, updates: Partial<ConnectionFolder>): void
  onUpdateConnectionOrder(id: string, newParentId: string | undefined, newOrder: number): void
  onCreateCommand(input: any): void
  onUpdateCommand(commandId: string, input: any): void
  onDeleteCommand(commandId: string): void
  onCreateCommandFolder(name: string): void
  onDeleteCommandFolder(folderId: string): void
  onUpdateCommandFolder(folderId: string, updates: Partial<CommandFolder>): void
  onUpdateCommandOrder(id: string, newParentId: string | undefined, newOrder: number): void
  onSetTheme(value: 'default-dark' | 'default-light'): void
  onSetLocale(value: 'zhCN' | 'enUS'): void
  onOpenLogsDirectory(): void
  isSidebarCollapsed: boolean
  isWorkspaceFocusMode: boolean
  tabBarProps: any
  isResizingSidebar: boolean
  onResizeStart(): void
}) {
  if (activeLocalTab?.kind === 'system') {
    return <SystemInfoWorkspace activeProfile={activeProfile} activeSession={activeSession} />
  }

  if (activeTab && activeSession && !activeLocalTab) {
    return (
      <SessionWorkspace
        key={activeTab.id}
        activeSession={activeSession}
        activeTab={activeTab}
        sendTargets={sendTargets}
        terminalDockSendScope={terminalDockSendScope}
        terminalDockSelectedTabIds={terminalDockSelectedTabIds}
        commandFolders={commandFolders}
        commandTemplates={commandTemplates}
        isBusy={isBusy}
        localItems={localItems}
        localPath={localPath}
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
        onTerminalDockSendScopeChange={onTerminalDockSendScopeChange}
        onTerminalDockSelectedTabIdsChange={onTerminalDockSelectedTabIdsChange}
        onOpenCommandManager={onOpenCommandManager}
        onChooseUploadFiles={onChooseUploadFiles}
        onDownloadFiles={onDownloadFiles}
        onDropUpload={onDropUpload}
        onOpenLocalItem={onOpenLocalItem}
        onOpenLocalPath={onOpenLocalPath}
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
      locale={locale}
      onOpen={onOpenProfile}
      onCreateConnection={onCreateConnection}
      onEditConnection={onEditConnection}
      onDeleteConnection={onDeleteConnection}
      onCreateConnectionFolder={onCreateConnectionFolder}
      onDeleteConnectionFolder={onDeleteConnectionFolder}
      onUpdateConnectionFolder={onUpdateConnectionFolder}
      onUpdateConnectionOrder={onUpdateConnectionOrder}
      onCreateCommand={onCreateCommand}
      onUpdateCommand={onUpdateCommand}
      onDeleteCommand={onDeleteCommand}
      onCreateCommandFolder={onCreateCommandFolder}
      onDeleteCommandFolder={onDeleteCommandFolder}
      onUpdateCommandFolder={onUpdateCommandFolder}
      onUpdateCommandOrder={onUpdateCommandOrder}
      onSetTheme={onSetTheme}
      onSetLocale={onSetLocale}
      onOpenLogsDirectory={onOpenLogsDirectory}
      isSidebarCollapsed={isSidebarCollapsed}
      profiles={profiles}
      tabBarProps={tabBarProps}
      isResizingSidebar={isResizingSidebar}
      onResizeStart={onResizeStart}
    />
  )
}
