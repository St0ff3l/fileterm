import type { ConnectionProfile, SessionSnapshot, WorkspaceTab } from '@fileterm/core'
import { useMemo, useState } from 'react'
import { t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'
import { ConfirmActionDialog } from '../common/ConfirmActionDialog'

type BackgroundSessionFilter = 'all' | 'connected' | 'connecting' | 'inactive'
type BackgroundSessionState = Exclude<BackgroundSessionFilter, 'all'> | 'error'

function resolveSessionState(tab: WorkspaceTab, session: SessionSnapshot | undefined): BackgroundSessionState {
  if (tab.status === 'connecting') {
    return 'connecting'
  }
  if (tab.status === 'error') {
    return 'error'
  }
  if (tab.status === 'connected' && session?.connected !== false) {
    return 'connected'
  }
  return 'inactive'
}

function resolveSessionAddress(profile: ConnectionProfile | undefined, session: SessionSnapshot | undefined) {
  const accessHost = session?.accessHost?.trim()
  if (accessHost) {
    return accessHost
  }

  if (profile?.type === 'serial') {
    return profile.devicePath
  }

  if (profile?.host) {
    return profile.port ? `${profile.host}:${profile.port}` : profile.host
  }

  return '—'
}

function sessionStateLabel(state: BackgroundSessionState) {
  switch (state) {
    case 'connected':
      return t.backgroundSessionConnected
    case 'connecting':
      return t.backgroundSessionConnecting
    case 'error':
      return t.backgroundSessionError
    default:
      return t.backgroundSessionInactive
  }
}

function sessionSourceLabel(source: WorkspaceTab['source']) {
  if (source === 'cli') {
    return t.sessionSourceCli
  }
  if (source === 'mcp') {
    return t.sessionSourceMcp
  }
  return '—'
}

export function BackgroundSessionsPage({
  profiles,
  tabs,
  sessions,
  onAttach,
  onClose
}: {
  profiles: ConnectionProfile[]
  tabs: WorkspaceTab[]
  sessions: Record<string, SessionSnapshot>
  onAttach(tabId: string): void | Promise<void>
  onClose(tabId: string): void | Promise<void>
}) {
  const [filter, setFilter] = useState<BackgroundSessionFilter>('all')
  const [searchQuery, setSearchQuery] = useState('')
  const [pendingClose, setPendingClose] = useState<WorkspaceTab | null>(null)
  const profileById = useMemo(() => new Map(profiles.map((profile) => [profile.id, profile])), [profiles])

  const stateByTabId = useMemo(
    () => new Map(tabs.map((tab) => [tab.id, resolveSessionState(tab, sessions[tab.id])])),
    [sessions, tabs]
  )

  const counts = useMemo(() => {
    const next = { all: tabs.length, connected: 0, connecting: 0, inactive: 0 }
    for (const state of stateByTabId.values()) {
      if (state === 'error') {
        next.inactive += 1
      } else {
        next[state] += 1
      }
    }
    return next
  }, [stateByTabId, tabs.length])

  const visibleTabs = useMemo(() => {
    const query = searchQuery.trim().toLocaleLowerCase()
    return tabs.filter((tab) => {
      const state = stateByTabId.get(tab.id) ?? 'inactive'
      const matchesFilter =
        filter === 'all' || (filter === 'inactive' ? state === 'inactive' || state === 'error' : state === filter)
      if (!matchesFilter) {
        return false
      }
      if (!query) {
        return true
      }

      const profile = profileById.get(tab.profileId)
      const address = resolveSessionAddress(profile, sessions[tab.id])
      return [tab.title, tab.id, tab.sessionType, tab.source, address, profile?.username]
        .filter(Boolean)
        .some((value) => value!.toLocaleLowerCase().includes(query))
    })
  }, [filter, profileById, searchQuery, sessions, stateByTabId, tabs])

  const filterItems: Array<{ id: BackgroundSessionFilter; label: string; count: number }> = [
    { id: 'all', label: t.allBackgroundSessions, count: counts.all },
    { id: 'connected', label: t.backgroundSessionConnected, count: counts.connected },
    { id: 'connecting', label: t.backgroundSessionConnecting, count: counts.connecting },
    { id: 'inactive', label: t.backgroundSessionInactive, count: counts.inactive }
  ]

  return (
    <section className="background-sessions-page manager-inline connection-manager-modal">
      <div className="connection-manager-header">
        <span className="connection-manager-title">
          <AppIcon name="history" size={20} />
          <span>{t.backgroundSessions}</span>
        </span>
        <label className="connection-manager-search">
          <AppIcon name="search" size={14} />
          <input
            aria-label={t.filterBackgroundSessions}
            placeholder={t.filterBackgroundSessions}
            type="search"
            value={searchQuery}
            onChange={(event) => setSearchQuery(event.target.value)}
          />
        </label>
      </div>

      <div className="connection-manager-layout background-sessions-layout">
        <aside className="connection-manager-sidebar" aria-label={t.backgroundSessionFilter}>
          {filterItems.map((item) => (
            <button
              key={item.id}
              className={`connection-manager-sidebar-item ${filter === item.id ? 'active' : ''}`}
              onClick={() => setFilter(item.id)}
              type="button"
            >
              <span className="connection-manager-sidebar-icon">
                <AppIcon
                  name={item.id === 'all' ? 'connections' : item.id === 'connected' ? 'server' : 'history'}
                  size={14}
                />
              </span>
              <span className="connection-manager-sidebar-label">{item.label}</span>
              <span className="connection-manager-sidebar-count">{item.count}</span>
            </button>
          ))}
        </aside>

        <section className="connection-manager-main">
          <div className="manager-table connection-manager-table background-sessions-table">
            <div className="manager-head">
              <span>{t.name}</span>
              <span>{t.host}</span>
              <span>{t.type}</span>
              <span>{t.sessionSource}</span>
              <span>{t.backgroundSessionStatus}</span>
              <span>{t.sessionId}</span>
              <span>{t.actions}</span>
            </div>
            <div className="manager-body connection-manager-body">
              {visibleTabs.map((tab) => {
                const profile = profileById.get(tab.profileId)
                const session = sessions[tab.id]
                const state = stateByTabId.get(tab.id) ?? 'inactive'
                const address = resolveSessionAddress(profile, session)
                const sourceLabel = sessionSourceLabel(tab.source)
                return (
                  <div
                    key={tab.id}
                    className="manager-row background-session-row"
                    onClick={() => void onAttach(tab.id)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter' || event.key === ' ') {
                        event.preventDefault()
                        void onAttach(tab.id)
                      }
                    }}
                    role="button"
                    tabIndex={0}
                    title={t.openBackgroundSession}
                  >
                    <span className="background-session-name" title={tab.title}>
                      <AppIcon name="server" size={14} />
                      <span>{tab.title}</span>
                    </span>
                    <span className="background-session-address" title={address}>
                      {address}
                    </span>
                    <span className="manager-type-badge">{(profile?.type ?? tab.sessionType).toUpperCase()}</span>
                    <span className={`background-session-source is-${tab.source ?? 'unknown'}`} title={sourceLabel}>
                      {sourceLabel}
                    </span>
                    <span className={`background-session-status is-${state}`}>
                      <span aria-hidden="true" className="background-session-status-dot" />
                      {sessionStateLabel(state)}
                    </span>
                    <span className="background-session-id" title={tab.id}>
                      {tab.id}
                    </span>
                    <span className="manager-actions background-session-actions">
                      <button
                        aria-label={`${t.openBackgroundSession}: ${tab.title}`}
                        className="manager-icon-action"
                        onClick={(event) => {
                          event.stopPropagation()
                          void onAttach(tab.id)
                        }}
                        title={t.openBackgroundSession}
                        type="button"
                      >
                        <AppIcon name="play" size={14} />
                      </button>
                      <button
                        aria-label={`${t.closeBackgroundSession}: ${tab.title}`}
                        className="manager-icon-action danger"
                        onClick={(event) => {
                          event.stopPropagation()
                          setPendingClose(tab)
                        }}
                        title={t.closeBackgroundSession}
                        type="button"
                      >
                        <AppIcon name="trash" size={14} />
                      </button>
                    </span>
                  </div>
                )
              })}
              {visibleTabs.length === 0 ? (
                <div className="background-session-empty">
                  <AppIcon name="history" size={24} />
                  <strong>
                    {searchQuery.trim() || filter !== 'all' ? t.noMatchingBackgroundSessions : t.noBackgroundSessions}
                  </strong>
                  <span>{t.noBackgroundSessionsHint}</span>
                </div>
              ) : null}
            </div>
          </div>
        </section>
      </div>

      {pendingClose ? (
        <ConfirmActionDialog
          confirmLabel={t.closeBackgroundSession}
          description={`${t.closeBackgroundSessionConfirmPrefix}${pendingClose.title}${t.closeBackgroundSessionConfirmSuffix}`}
          onClose={() => setPendingClose(null)}
          onConfirm={() => {
            const tabId = pendingClose.id
            setPendingClose(null)
            void onClose(tabId)
          }}
          title={t.closeBackgroundSession}
        />
      ) : null}
    </section>
  )
}
