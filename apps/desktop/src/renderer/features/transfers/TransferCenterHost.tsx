import type { FileTermDesktopApi, TransferTask, WorkspaceSnapshot, WorkspaceTab } from '@fileterm/core'
import { TransferCenter } from './TransferCenter'

export function TransferCenterHost({
  activeProfileId,
  activeTabId,
  desktopApi,
  fullWidth,
  isPending,
  onApplySnapshot,
  onError,
  sessionTabs,
  transfers,
  visible
}: {
  activeProfileId?: string
  activeTabId: string | null
  desktopApi?: FileTermDesktopApi
  fullWidth: boolean
  isPending: boolean
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
      desktopApi={desktopApi}
      fullWidth={fullWidth}
      isPending={isPending}
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
