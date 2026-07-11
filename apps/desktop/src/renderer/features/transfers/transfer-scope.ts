import type { TransferTask } from '@fileterm/core'

export type TransferSessionTab = {
  id: string
  profileId: string
}

export function scopeTransfersToSession(
  transfers: TransferTask[],
  activeTabId: string | null | undefined,
  activeProfileId: string | undefined,
  sessionTabs: TransferSessionTab[]
): TransferTask[] {
  if (!activeTabId || !activeProfileId) {
    return []
  }

  const profileSessionTabIds = new Set(
    sessionTabs.filter((tab) => tab.profileId === activeProfileId).map((tab) => tab.id)
  )

  return transfers.filter((transfer) => {
    if (transfer.tabId === activeTabId) {
      return true
    }
    if (transfer.profileId !== activeProfileId) {
      return false
    }
    if (!transfer.tabId) {
      return true
    }
    return !profileSessionTabIds.has(transfer.tabId)
  })
}
