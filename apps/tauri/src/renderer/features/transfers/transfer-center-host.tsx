import type {
  FileTermDesktopApi,
  TransferTask,
  WorkspaceSessionSource,
  WorkspaceSnapshot,
  WorkspaceTab
} from '@fileterm/core'
import { TransferCenter } from './transfer-center'

export function TransferCenterHost({
  activeProfileId,
  activeTabId,
  activeTabStatus,
  activeTabSource,
  desktopApi,
  fullWidth,
  isPending,
  onHideToBackground,
  onApplySnapshot,
  onError,
  sessionTabs,
  transfers,
  visible
}: {
  activeProfileId?: string
  activeTabId: string | null
  activeTabStatus: WorkspaceTab['status'] | null
  activeTabSource: WorkspaceSessionSource | null
  desktopApi?: FileTermDesktopApi
  fullWidth: boolean
  isPending: boolean
  onHideToBackground?(tabId: string): void | Promise<void>
  onApplySnapshot(snapshot: WorkspaceSnapshot): void
  onError(scope: string, err: unknown): void
  sessionTabs: WorkspaceTab[]
  transfers: TransferTask[]
  visible: boolean
}) {
  return (
    <TransferCenter
      activeProfileId={activeProfileId}
      activeTabId={activeTabId}
      activeTabStatus={activeTabStatus}
      activeTabSource={activeTabSource}
      desktopApi={desktopApi}
      fullWidth={fullWidth}
      isPending={isPending}
      onHideToBackground={onHideToBackground}
      onApplySnapshot={onApplySnapshot}
      onError={onError}
      sessionTabs={sessionTabs.map((tab) => ({
        id: tab.id,
        profileId: tab.profileId
      }))}
      transfers={transfers}
      visible={visible}
    />
  )
}
