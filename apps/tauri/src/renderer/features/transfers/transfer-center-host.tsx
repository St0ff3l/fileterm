import type {
  FileTermDesktopApi,
  TransferTask,
  WorkspaceSessionSource,
  WorkspaceSnapshot,
  WorkspaceTab
} from '@fileterm/core'
import { TransferCenter } from './transfer-center'

type JumpHostSummary = {
  name: string
  host: string
  port: number
}

export function TransferCenterHost({
  activeProfileId,
  activeTabId,
  activeTabStatus,
  activeJumpHost,
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
  activeJumpHost?: JumpHostSummary | null
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
      activeJumpHost={activeJumpHost}
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
