import type { SessionSnapshot, WorkspaceTab } from '@fileterm/core'
import { tabStatusClass } from '../../app/app-utils'
import { t } from '../../i18n'
import type { StatusIndicatorStatus } from '../common/status-indicator'

export function statusForProfile(profileId: string, tabs: WorkspaceTab[], sessions: Record<string, SessionSnapshot>) {
  const profileTabs = tabs.filter((tab) => tab.profileId === profileId)
  const statusTab =
    profileTabs.find((tab) => sessions[tab.id]?.connected) ??
    profileTabs.find((tab) => tab.status === 'connecting') ??
    profileTabs[0]
  if (!statusTab) return null
  const normalized = sessions[statusTab.id]?.connected ? 'connected' : tabStatusClass(statusTab.status)
  const status: StatusIndicatorStatus =
    normalized === 'connected' || normalized === 'connecting' || normalized === 'disconnected' ? normalized : 'idle'
  return { status, tab: statusTab }
}

export function statusLabel(status: StatusIndicatorStatus) {
  if (status === 'connected') return t.hostStatusConnected
  if (status === 'connecting') return t.hostStatusConnecting
  // A closed session is just "not connected"; never surface "disconnected".
  return t.serverDashboardNotConnected
}
