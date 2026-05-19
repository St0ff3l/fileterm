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
} from '@termdock/core'
import type { DragEvent } from 'react'
import { SystemInfoWorkspace } from '../system/SystemInfoWorkspace'
import { HomeWorkspace } from './HomeWorkspace'
import { SessionWorkspace } from './SessionWorkspace'

type ActiveLocalTab = {
  kind: 'home' | 'system'
  sessionTabId?: string
} | null

export function WorkspaceStage({
  activeLocalTab,
  activeProfile,
  activeSession,
  activeTab,
  tabs,
  commandFolders,
  commandTemplates,
  folders,
  isBusy,
  localItems,
  localPath,
  onExecuteCommand,
  onOpenCommandManager,
  profiles,
  onChooseUploadFiles,
  onDownloadFiles,
  onDropUpload,
  onOpenLocalItem,
  onOpenLocalPath,
  onOpenProfile,
  onOpenRemoteItem,
  onOpenRemotePath,
  onRefresh,
  onUploadFiles
}: {
  activeLocalTab: ActiveLocalTab
  activeProfile: ConnectionProfile | null
  activeSession: SessionSnapshot | null
  activeTab: WorkspaceTab | null
  tabs: WorkspaceTab[]
  commandFolders: CommandFolder[]
  commandTemplates: CommandTemplate[]
  folders: ConnectionFolder[]
  isBusy: boolean
  localItems: LocalFileItem[]
  localPath: string
  onExecuteCommand(commandId: string, args: string[], options: CommandExecutionOptions, scope: 'current' | 'all-ssh'): void
  onOpenCommandManager(): void
  profiles: ConnectionProfile[]
  onChooseUploadFiles(): void
  onDownloadFiles(items: RemoteFileItem[], targetDirectory?: string): void
  onDropUpload(event: DragEvent<HTMLDivElement>): void
  onOpenLocalItem(item: LocalFileItem): void
  onOpenLocalPath(path: string): void
  onOpenProfile(profileId: string): void
  onOpenRemoteItem(item: RemoteFileItem): void
  onOpenRemotePath(path: string): void
  onRefresh(): void
  onUploadFiles(items: LocalFileItem[]): void
}) {
  if (activeLocalTab?.kind === 'system') {
    return <SystemInfoWorkspace activeProfile={activeProfile} activeSession={activeSession} />
  }

  if (activeTab && activeSession && !activeLocalTab) {
    return (
      <SessionWorkspace
        activeSession={activeSession}
        activeTab={activeTab}
        tabs={tabs}
        commandFolders={commandFolders}
        commandTemplates={commandTemplates}
        isBusy={isBusy}
        localItems={localItems}
        localPath={localPath}
        onExecuteCommand={onExecuteCommand}
        onOpenCommandManager={onOpenCommandManager}
        onChooseUploadFiles={onChooseUploadFiles}
        onDownloadFiles={onDownloadFiles}
        onDropUpload={onDropUpload}
        onOpenLocalItem={onOpenLocalItem}
        onOpenLocalPath={onOpenLocalPath}
        onOpenRemoteItem={onOpenRemoteItem}
        onOpenRemotePath={onOpenRemotePath}
        onRefresh={onRefresh}
        onUploadFiles={onUploadFiles}
      />
    )
  }

  return (
    <HomeWorkspace
      folders={folders}
      onOpen={onOpenProfile}
      profiles={profiles}
    />
  )
}
