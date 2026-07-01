import type { WorkspaceTab } from '@fileterm/core'

export type SendScope = 'current' | 'all-ssh' | 'selected-ssh'

export type SessionSendTarget = {
  tabId: string
  index: number
  title: string
  label: string
  isCurrent: boolean
}

export function summarizeSendTarget(
  scope: SendScope,
  selectedTabIds: string[],
  targets: SessionSendTarget[],
  fallback: string
) {
  if (scope === 'current') {
    return fallback
  }

  if (scope === 'all-ssh') {
    return targets.length ? targets.map((target) => String(target.index)).join(', ') : fallback
  }

  const selectedTargets = targets.filter((target) => selectedTabIds.includes(target.tabId))
  if (!selectedTargets.length) {
    return fallback
  }

  return selectedTargets.map((target) => String(target.index)).join(', ')
}

export function resolveSelectedTabIds(
  scope: SendScope,
  activeTab: WorkspaceTab | null,
  selectedTabIds: string[],
  targets: SessionSendTarget[]
) {
  if (scope === 'current') {
    return activeTab && activeTab.sessionType === 'ssh' ? [activeTab.id] : []
  }

  if (scope === 'all-ssh') {
    return targets.map((target) => target.tabId)
  }

  const availableIds = new Set(targets.map((target) => target.tabId))
  return selectedTabIds.filter((tabId) => availableIds.has(tabId))
}
