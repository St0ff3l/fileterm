import type { CommandBatchTarget, SessionSnapshot, WorkspaceTab } from '@fileterm/core'

export function getCommandBatchTargets(tabs: WorkspaceTab[], sessions: Record<string, SessionSnapshot>) {
  const targets = new Map<string, CommandBatchTarget>()
  for (const tab of tabs) {
    const session = sessions[tab.id]
    if (
      tab.sessionType !== 'ssh' ||
      (tab.isBackground && tab.source) ||
      targets.has(tab.profileId) ||
      session?.profileId !== tab.profileId ||
      !session.connected ||
      !session.capabilities?.shellIntegration ||
      session.deviceMode === 'network-device' ||
      !session.aiSessionRevision
    ) {
      continue
    }
    targets.set(tab.profileId, { profileId: tab.profileId, tabId: tab.id, sessionRevision: session.aiSessionRevision })
  }
  return targets
}
