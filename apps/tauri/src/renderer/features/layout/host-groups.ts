import type { ConnectionProfile, TabStatus, WorkspaceTab } from '@fileterm/core'
import type { OrderedTabEntry } from './tab-bar'

/**
 * Tabs are grouped by the machine they belong to: the left rail lists the
 * groups and the tab bar only shows the tabs of the active group.
 *
 * - `home`: home/overview tabs (and system-info or transfer tabs whose session is gone)
 * - `local`: every local terminal
 * - `profile:<id>`: every session opened from the same connection profile
 */
export const HOME_HOST_GROUP_KEY = 'home'
export const LOCAL_HOST_GROUP_KEY = 'local'

export type HostGroupKind = 'home' | 'local' | 'profile'

export interface HostGroup {
  key: string
  kind: HostGroupKind
  profileId?: string
  /** Profile name, or the first tab's title when the profile is unknown. */
  title: string
  profileType?: ConnectionProfile['type']
  /** Host address shown as a subtitle for remote groups. */
  subtitle?: string
  entries: OrderedTabEntry[]
  status: TabStatus | null
}

export function sessionHostGroupKey(tab: Pick<WorkspaceTab, 'id' | 'profileId' | 'sessionType'>) {
  if (tab.sessionType === 'local') {
    return LOCAL_HOST_GROUP_KEY
  }
  return tab.profileId ? `profile:${tab.profileId}` : `session:${tab.id}`
}

export function entryHostGroupKey(entry: OrderedTabEntry, sessionTabs: readonly WorkspaceTab[]) {
  if (entry.kind === 'session') {
    return sessionHostGroupKey(entry.tab)
  }
  const sourceTabId =
    entry.tabKind === 'system' ? entry.sessionTabId : entry.tabKind === 'transfers' ? entry.initialTabId : undefined
  if (sourceTabId) {
    const sessionTab = sessionTabs.find((tab) => tab.id === sourceTabId)
    if (sessionTab) {
      return sessionHostGroupKey(sessionTab)
    }
  }
  return HOME_HOST_GROUP_KEY
}

const STATUS_PRIORITY: TabStatus[] = ['connected', 'connecting', 'error', 'closed', 'idle']

/** The most reassuring status wins so a host with one live session reads as connected. */
export function aggregateHostStatus(statuses: readonly TabStatus[]): TabStatus | null {
  for (const status of STATUS_PRIORITY) {
    if (statuses.includes(status)) {
      return status
    }
  }
  return null
}

function hostProfileSubtitle(profile: ConnectionProfile) {
  if (profile.type === 'serial') {
    return profile.devicePath
  }
  return profile.username ? `${profile.username}@${profile.host}` : profile.host
}

export function buildHostGroups(
  orderedTabs: readonly OrderedTabEntry[],
  profiles: readonly ConnectionProfile[]
): HostGroup[] {
  const sessionTabs = orderedTabs.flatMap((entry) => (entry.kind === 'session' ? [entry.tab] : []))
  const home: HostGroup = { key: HOME_HOST_GROUP_KEY, kind: 'home', title: '', entries: [], status: null }
  const groups = new Map<string, HostGroup>([[HOME_HOST_GROUP_KEY, home]])

  for (const entry of orderedTabs) {
    const key = entryHostGroupKey(entry, sessionTabs)
    let group = groups.get(key)
    if (!group) {
      const sessionTab =
        entry.kind === 'session' ? entry.tab : sessionTabs.find((tab) => sessionHostGroupKey(tab) === key)
      const profileId = key.startsWith('profile:') ? key.slice('profile:'.length) : undefined
      const profile = profileId ? profiles.find((candidate) => candidate.id === profileId) : undefined
      group = {
        key,
        kind: key === LOCAL_HOST_GROUP_KEY ? 'local' : 'profile',
        profileId,
        title: profile?.name || sessionTab?.title || '',
        profileType: profile?.type,
        subtitle: profile ? hostProfileSubtitle(profile) : undefined,
        entries: [],
        status: null
      }
      groups.set(key, group)
    }
    group.entries.push(entry)
  }

  for (const group of groups.values()) {
    group.status = aggregateHostStatus(
      group.entries.flatMap((entry) => (entry.kind === 'session' ? [entry.tab.status] : []))
    )
  }

  return [...groups.values()]
}

export function resolveActiveHostGroupKey(
  orderedTabs: readonly OrderedTabEntry[],
  activeHomeTabId: string | null,
  activeSessionTabId: string | null
) {
  const sessionTabs = orderedTabs.flatMap((entry) => (entry.kind === 'session' ? [entry.tab] : []))
  const activeEntry = activeHomeTabId
    ? orderedTabs.find((entry) => entry.kind === 'local' && entry.id === activeHomeTabId)
    : orderedTabs.find((entry) => entry.kind === 'session' && entry.tab.id === activeSessionTabId)
  return activeEntry ? entryHostGroupKey(activeEntry, sessionTabs) : HOME_HOST_GROUP_KEY
}
