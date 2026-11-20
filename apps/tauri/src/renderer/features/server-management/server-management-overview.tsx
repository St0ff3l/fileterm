import { useEffect, useMemo, useReducer, useRef, useState, type CSSProperties } from 'react'
import type { ConnectionFolder, ConnectionProfile, SessionSnapshot, SystemMetrics, WorkspaceTab } from '@fileterm/core'
import { MaterialIcon } from '../../components/common/material-icon/material-icon'
import { SidebarToggle } from '../../components/common/sidebar-toggle/sidebar-toggle'
import { connectionProfileSummary } from '../../app/connection-profile-summary'
import { tabStatusClass } from '../../app/app-utils'
import { getLocale, t } from '../../i18n'
import {
  getConnectionHistory,
  getServerInsight,
  hydrateServerInsights,
  serverInsightFromMetrics,
  subscribeServerInsights,
  type ServerInsight
} from './server-insights-store'
import { StatusIndicator, type StatusIndicatorStatus } from '../common/status-indicator'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { buildConnectionTree, flattenConnectionProfiles, type ConnectionTreeNode } from '../connections/connection-tree'
import { managerDropClass, resolveManagerDropPosition, type ManagerDropPosition } from '../common/manager-drag'
import { usePointerSortFallback, type PointerSortTarget } from '../../hooks/use-pointer-sort-fallback'
import { lastSessionTabIdForProfile } from '../../hooks/session-tab-memory'
import { resolveMonitoringSource } from '../system/shared-monitoring'
import './server-management-overview.css'

const MAX_METRIC_AGE_MS = 120_000

function statusForProfile(profileId: string, tabs: WorkspaceTab[], sessions: Record<string, SessionSnapshot>) {
  const profileTabs = tabs.filter((tab) => tab.profileId === profileId)
  const statusTab =
    profileTabs.find((tab) => sessions[tab.id]?.connected) ??
    profileTabs.find((tab) => tab.status === 'connecting') ??
    profileTabs[0]
  if (!statusTab) return null
  const normalized = tabStatusClass(statusTab.status)
  const status: StatusIndicatorStatus =
    normalized === 'connected' || normalized === 'connecting' || normalized === 'disconnected' ? normalized : 'idle'
  return { status, tab: statusTab }
}

function findFolder(nodes: ConnectionTreeNode[], folderId: string): ConnectionTreeNode | undefined {
  for (const node of nodes) {
    if (node.type === 'folder' && node.id === folderId) return node
    if (node.type === 'folder') {
      const found = findFolder(node.children, folderId)
      if (found) return found
    }
  }
  return undefined
}

function liveMetrics(profileId: string, tabs: WorkspaceTab[], sessions: Record<string, SessionSnapshot>) {
  const sessionTab = tabs.find((tab) => tab.profileId === profileId && sessions[tab.id]?.connected)
  if (!sessionTab) return null
  const source = resolveMonitoringSource(sessionTab.id, sessions).session
  const metrics = source?.systemMetrics
  const sampledAt = source?.monitoring?.lastSampleAt
  if (
    !source?.connected ||
    !metrics ||
    !sampledAt ||
    !['healthy', 'shared'].includes(source.monitoring?.phase ?? '') ||
    Date.now() - sampledAt > MAX_METRIC_AGE_MS
  ) {
    return null
  }
  return metrics
}

function diskPercent(metrics: SystemMetrics | null) {
  const root = metrics?.fileSystemRows.find((row) => row.mountPoint === '/') ?? metrics?.fileSystemRows[0]
  const value = Number.parseFloat(root?.usagePercent ?? '')
  return Number.isFinite(value) ? Math.max(0, Math.min(100, value)) : null
}

function ResourceRing({ label, value, detail }: { label: string; value: number | null; detail?: string }) {
  const percent = value === null || !Number.isFinite(value) ? 0 : Math.max(0, Math.min(100, value))
  return (
    <div className="server-resource-ring-block">
      <span
        aria-label={value === null ? `${label}: ${t.serverDashboardNoMetrics}` : `${label}: ${Math.round(percent)}%`}
        aria-valuemax={100}
        aria-valuemin={0}
        aria-valuenow={value === null ? undefined : Math.round(percent)}
        className={`server-resource-ring ${value === null ? 'is-empty' : ''}`}
        role="progressbar"
        style={{ '--server-resource-value': `${percent}%` } as CSSProperties}
      >
        <span>{value === null ? '—' : `${Math.round(percent)}%`}</span>
      </span>
      <strong>{label}</strong>
      {detail ? <small title={detail}>{detail}</small> : null}
    </div>
  )
}

function getResourceMetrics(metrics: SystemMetrics | null) {
  return {
    cpu: metrics?.cpuPercent ?? null,
    memory: metrics?.memoryPercent ?? null,
    disk: diskPercent(metrics),
    memoryDetail: metrics?.memoryUsage,
    diskDetail: metrics?.fileSystemRows.find((row) => row.mountPoint === '/')?.usagePercent
  }
}

function statusLabel(status: StatusIndicatorStatus) {
  if (status === 'connected') return t.hostStatusConnected
  if (status === 'connecting') return t.hostStatusConnecting
  // A closed session is just "not connected"; never surface "disconnected".
  return t.serverDashboardNotConnected
}

function formatConnectionHistoryTime(at: number) {
  try {
    return new Intl.DateTimeFormat(getLocale() === 'zhCN' ? 'zh-CN' : 'en-US', {
      dateStyle: 'medium',
      timeStyle: 'short'
    }).format(new Date(at))
  } catch {
    return new Date(at).toLocaleString()
  }
}

function formatClockTime(at: number) {
  try {
    return new Intl.DateTimeFormat(getLocale() === 'zhCN' ? 'zh-CN' : 'en-US', {
      hour: '2-digit',
      minute: '2-digit'
    }).format(new Date(at))
  } catch {
    return new Date(at).toLocaleTimeString()
  }
}

function useServerInsightStore() {
  const [, refresh] = useReducer((value: number) => value + 1, 0)

  useEffect(() => {
    void hydrateServerInsights()
    return subscribeServerInsights(refresh)
  }, [])
}

export function ServerManagementOverview({
  profiles,
  folders,
  sessions,
  workspaceTabs,
  focusedProfileId,
  focusedSessionTabId,
  focusRequest,
  isFocusMode,
  selectedFolderId,
  onSelectFolder,
  onOpenProfile,
  onConnectProfile,
  onDisconnectSession,
  onActivateSession,
  onCreateConnection,
  onEditConnection,
  onUpdateConnectionOrder,
  onToggleFocusMode
}: {
  profiles: ConnectionProfile[]
  folders: ConnectionFolder[]
  sessions: Record<string, SessionSnapshot>
  workspaceTabs: WorkspaceTab[]
  focusedProfileId?: string | null
  focusedSessionTabId?: string | null
  focusRequest?: number
  isFocusMode?: boolean
  selectedFolderId: string | null
  onSelectFolder(folderId: string | null): void
  onOpenProfile(profileId: string): void
  onConnectProfile(profileId: string): void
  onDisconnectSession(tabId: string): void | Promise<void>
  onActivateSession(tabId: string): void
  onCreateConnection(): void
  onEditConnection(profile: ConnectionProfile): void
  onUpdateConnectionOrder(
    id: string,
    newParentId: string | undefined,
    newOrder: number
  ): Promise<boolean> | boolean | void
  onToggleFocusMode?(): void
}) {
  const [query, setQuery] = useState('')
  const [selectedProfileId, setSelectedProfileId] = useState<string | null>(null)
  const [isInspectorCollapsed, setIsInspectorCollapsed] = useState(false)
  // Focus mode owns the side panels, so the inspector folds away with it and the
  // expand affordance leaves focus mode instead of fighting the forced state.
  const inspectorCollapsed = isInspectorCollapsed || Boolean(isFocusMode)
  const [dragState, setDragState] = useState<{
    draggingId: string | null
    targetId: string | null
    position: ManagerDropPosition | null
  }>({ draggingId: null, targetId: null, position: null })
  const mainScrollRef = useRef<HTMLDivElement>(null)
  const inspectorScrollRef = useRef<HTMLElement>(null)
  useServerInsightStore()
  const tree = useMemo(() => buildConnectionTree(profiles, folders), [profiles, folders])
  const selectedFolder = selectedFolderId ? findFolder(tree.roots, selectedFolderId) : undefined
  const baseProfiles =
    selectedFolder?.type === 'folder'
      ? flattenConnectionProfiles([selectedFolder])
      : flattenConnectionProfiles(tree.roots)
  const search = query.trim().toLocaleLowerCase()
  const visibleProfiles = baseProfiles.filter((profile) =>
    [profile.name, profile.host, profile.note, profile.username].some((value) =>
      value?.toLocaleLowerCase().includes(search)
    )
  )
  const firstBaseProfileId = baseProfiles[0]?.id ?? null
  useEffect(() => {
    // Default to the first connection and recover when the selected one disappears.
    setSelectedProfileId((current) =>
      current && baseProfiles.some((profile) => profile.id === current) ? current : firstBaseProfileId
    )
  }, [baseProfiles, firstBaseProfileId])
  useEffect(() => {
    // Leaving focus mode restores the inspector even if it was manually
    // collapsed, so the window tools stay a predictable master toggle.
    if (!isFocusMode) setIsInspectorCollapsed(false)
  }, [isFocusMode])

  const focusedProfileRef = useRef<{ profileId: string; request: number } | null>(null)
  useEffect(() => {
    // Coming back from a terminal selects the server that session belongs to,
    // including when that server was remembered before a manual selection.
    if (!focusedProfileId) {
      return
    }
    const request = focusRequest ?? 0
    if (focusedProfileRef.current?.profileId === focusedProfileId && focusedProfileRef.current.request === request) {
      return
    }
    focusedProfileRef.current = { profileId: focusedProfileId, request }
    if (baseProfiles.some((profile) => profile.id === focusedProfileId)) {
      setSelectedProfileId(focusedProfileId)
    }
  }, [baseProfiles, focusRequest, focusedProfileId])
  const selectedProfile =
    visibleProfiles.find((profile) => profile.id === selectedProfileId) ?? visibleProfiles[0] ?? null
  const selectedStatus = selectedProfile ? statusForProfile(selectedProfile.id, workspaceTabs, sessions) : null
  const selectedBadgeStatus: StatusIndicatorStatus =
    selectedStatus?.status === 'connected' || selectedStatus?.status === 'connecting' ? selectedStatus.status : 'idle'
  const selectedSessionTab = selectedStatus?.tab ?? null
  const selectedSessionConnected = Boolean(selectedSessionTab && sessions[selectedSessionTab.id]?.connected)
  const selectedMetrics = selectedProfile ? liveMetrics(selectedProfile.id, workspaceTabs, sessions) : null
  const displayedInsight: ServerInsight | undefined = selectedMetrics
    ? serverInsightFromMetrics(selectedMetrics)
    : selectedProfile
      ? getServerInsight(selectedProfile.id)
      : undefined
  const isShowingCachedInsight = !selectedMetrics && Boolean(displayedInsight)
  const connectionHistory = selectedProfile ? getConnectionHistory(selectedProfile.id) : []
  const insightSystemName = displayedInsight?.osName
    ? [
        displayedInsight.osName,
        ...(displayedInsight.osName.trim().toLowerCase() === 'linux'
          ? [displayedInsight.hostname, displayedInsight.kernelVersion]
          : [])
      ]
        .filter((part): part is string => Boolean(part))
        .join(' ')
    : null
  const insightSystem =
    displayedInsight && insightSystemName
      ? `${insightSystemName}${
          displayedInsight.architecture && !insightSystemName.includes(displayedInsight.architecture)
            ? ` (${displayedInsight.architecture})`
            : ''
        }`
      : null
  const insightUptime = (() => {
    if (!displayedInsight) {
      return null
    }
    const seconds = displayedInsight.uptimeSeconds
    if (seconds && seconds > 0) {
      const days = Math.floor(seconds / 86400)
      const hours = Math.floor((seconds % 86400) / 3600)
      const minutes = Math.floor((seconds % 3600) / 60)
      const parts: string[] = []
      if (days > 0) {
        parts.push(`${days}${t.uptimeDayUnit}`)
      }
      if (hours > 0) {
        parts.push(`${hours}${t.uptimeHourUnit}`)
      }
      if (!days && !hours && minutes > 0) {
        parts.push(`${minutes}${t.uptimeMinuteUnit}`)
      }
      return parts.length ? parts.join(' ') : t.uptimeJustNow
    }
    return displayedInsight.uptime ?? null
  })()
  const insightCpu = displayedInsight?.cpuModel
    ? displayedInsight.cpuCores
      ? `${displayedInsight.cpuModel} (${t.serverInspectorCores.replace('{cores}', String(displayedInsight.cpuCores))})`
      : displayedInsight.cpuModel
    : null
  const insightDisk =
    displayedInsight?.diskUsed && displayedInsight.diskSize
      ? `${displayedInsight.diskUsed} / ${displayedInsight.diskSize}`
      : (displayedInsight?.diskSize ?? null)
  const insightDiskPercent = displayedInsight?.diskPercent ?? null
  const connectedCount = baseProfiles.filter((profile) =>
    workspaceTabs.some((tab) => tab.profileId === profile.id && sessions[tab.id]?.connected)
  ).length
  const title = selectedFolder?.type === 'folder' ? selectedFolder.name : t.serverManagementOverview

  const openWorkspace = () => {
    if (!selectedProfile) return
    // Reopen the tab that was last active for this server instead of the first.
    const rememberedTab = [lastSessionTabIdForProfile(selectedProfile.id), focusedSessionTabId]
      .filter((tabId): tabId is string => Boolean(tabId))
      .map((tabId) => workspaceTabs.find((tab) => tab.id === tabId && tab.profileId === selectedProfile.id))
      .find((tab) => tab && sessions[tab.id]?.connected)
    if (rememberedTab) onActivateSession(rememberedTab.id)
    else if (selectedSessionTab && selectedStatus?.status !== 'disconnected') onActivateSession(selectedSessionTab.id)
    else onOpenProfile(selectedProfile.id)
  }

  const positionForTarget = (target: PointerSortTarget, clientY: number) =>
    resolveManagerDropPosition(target.element, clientY, false)

  const applyDrop = (sourceId: string, targetId: string, position: ManagerDropPosition) => {
    if (sourceId === targetId) return
    const targetNode = tree.map.get(targetId)
    if (!targetNode) return
    const targetParent = targetNode.parentId ? tree.map.get(targetNode.parentId) : undefined
    const siblings = (targetParent?.type === 'folder' ? targetParent.children : tree.roots).filter(
      (node) => node.id !== sourceId
    )
    const targetIndex = siblings.findIndex((node) => node.id === targetId)
    if (targetIndex < 0) return

    const targetOrder = targetNode.order ?? 0
    const adjacentNode = siblings[position === 'top' ? targetIndex - 1 : targetIndex + 1]
    const newOrder = adjacentNode
      ? ((adjacentNode.order ?? 0) + targetOrder) / 2
      : targetOrder + (position === 'top' ? -1000 : 1000)
    onUpdateConnectionOrder(sourceId, targetParent?.type === 'folder' ? targetParent.id : undefined, newOrder)
  }

  const handlePointerSort = usePointerSortFallback<string>({
    onStart: (draggingId) => setDragState({ draggingId, targetId: null, position: null }),
    onTarget: (draggingId, target, clientY) => {
      const position = positionForTarget(target, clientY)
      setDragState({ draggingId, targetId: target.id, position })
    },
    onDrop: (draggingId, target, clientY) => {
      if (target) applyDrop(draggingId, target.id, positionForTarget(target, clientY))
      setDragState({ draggingId: null, targetId: null, position: null })
    },
    onCancel: () => setDragState({ draggingId: null, targetId: null, position: null })
  })

  return (
    <div
      className={`server-management-overview ${selectedProfile ? 'has-inspector' : ''} ${inspectorCollapsed ? 'is-inspector-collapsed' : ''}`}
    >
      <div className="server-dashboard-main-scroll">
        <main className="server-dashboard-main" ref={mainScrollRef}>
          <header className="server-dashboard-header">
            <div>
              <h1>
                <MaterialIcon name="dashboard" />
                {title}
              </h1>
              <p>
                {t.serverDashboardSummary
                  .replace('{total}', String(baseProfiles.length))
                  .replace('{online}', String(connectedCount))}
              </p>
            </div>
            <button className="server-dashboard-new" onClick={onCreateConnection} type="button">
              <MaterialIcon name="add" />
              <span>{t.newConnection}</span>
            </button>
          </header>

          <label className="server-dashboard-search">
            <MaterialIcon name="search" />
            <input
              aria-label={t.serverDashboardSearchPlaceholder}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t.serverDashboardSearchPlaceholder}
              type="search"
              value={query}
            />
            {selectedFolderId ? (
              <button onClick={() => onSelectFolder(null)} title={t.serverManagementOverview} type="button">
                {t.serverManagementOverview}
              </button>
            ) : null}
          </label>

          {visibleProfiles.length ? (
            <section aria-label={title} className="server-dashboard-grid">
              {visibleProfiles.map((profile) => {
                const connectionStatus = statusForProfile(profile.id, workspaceTabs, sessions)
                const connectionBadgeStatus: StatusIndicatorStatus =
                  connectionStatus?.status === 'connected' || connectionStatus?.status === 'connecting'
                    ? connectionStatus.status
                    : 'idle'
                const metrics = liveMetrics(profile.id, workspaceTabs, sessions)
                const resources = getResourceMetrics(metrics)
                const isSelected = selectedProfile?.id === profile.id
                return (
                  <article
                    aria-pressed={isSelected}
                    className={`server-dashboard-card ${isSelected ? 'is-selected' : ''} ${search ? 'is-sort-disabled' : ''} ${dragState.draggingId === profile.id ? 'is-dragging' : ''} ${managerDropClass(dragState.targetId === profile.id, dragState.position)}`}
                    data-fileterm-sort-id={search ? undefined : profile.id}
                    data-fileterm-sort-kind={search ? undefined : 'profile'}
                    key={profile.id}
                    onClick={() => setSelectedProfileId(profile.id)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter' || event.key === ' ') {
                        event.preventDefault()
                        setSelectedProfileId(profile.id)
                      }
                    }}
                    onPointerDown={(event) => {
                      if (!search) handlePointerSort(event, profile.id)
                    }}
                    role="button"
                    tabIndex={0}
                  >
                    <span className="server-dashboard-card-heading">
                      <MaterialIcon name={profile.type === 'serial' ? 'usb' : 'dns'} />
                      <strong>{profile.name}</strong>
                      {connectionBadgeStatus === 'idle' ? (
                        <span className="server-dashboard-status is-idle">{t.serverDashboardNotConnected}</span>
                      ) : (
                        <span className={`server-dashboard-status is-${connectionBadgeStatus}`}>
                          <StatusIndicator aria-hidden="true" status={connectionBadgeStatus} />
                          {statusLabel(connectionBadgeStatus)}
                        </span>
                      )}
                    </span>
                    <span className="server-dashboard-address">{connectionProfileSummary(profile)}</span>
                    <span className="server-dashboard-resources">
                      <ResourceRing label="CPU" value={resources.cpu} />
                      <ResourceRing label={t.memory} value={resources.memory} detail={resources.memoryDetail} />
                      <ResourceRing label={t.disk} value={resources.disk} detail={resources.diskDetail} />
                    </span>
                    <span className="server-dashboard-card-foot">
                      {profile.note ? (
                        <span className="server-dashboard-card-note" title={profile.note}>
                          {profile.note}
                        </span>
                      ) : null}
                      <span className="server-dashboard-card-type">{profile.type.toUpperCase()}</span>
                    </span>
                  </article>
                )
              })}
            </section>
          ) : (
            <div className="server-dashboard-empty">
              <MaterialIcon name={search ? 'search' : 'dns'} />
              <p>{search ? t.serverDashboardNoResults : t.noSavedConnections}</p>
              {!search ? (
                <button className="server-dashboard-new" onClick={onCreateConnection} type="button">
                  <MaterialIcon name="add" />
                  <span>{t.newConnection}</span>
                </button>
              ) : null}
            </div>
          )}
        </main>
        <VerticalScrollbar ariaLabel={title} scrollRef={mainScrollRef} />
      </div>

      {selectedProfile ? (
        <section className="server-dashboard-inspector-region">
          <header className="server-dashboard-inspector-header">
            {!inspectorCollapsed ? <h2>{t.serverDashboardDetails}</h2> : null}
            <SidebarToggle
              className="server-dashboard-inspector-toggle"
              collapsed={inspectorCollapsed}
              label={inspectorCollapsed ? t.showConnectionDetails : t.hideConnectionDetails}
              side="right"
              onClick={() => {
                if (isFocusMode) {
                  onToggleFocusMode?.()
                  return
                }
                setIsInspectorCollapsed((current) => !current)
              }}
            />
          </header>
          {!inspectorCollapsed ? (
            <div className="server-dashboard-inspector-scroll">
              <aside
                aria-label={t.serverDashboardDetails}
                className="server-dashboard-inspector"
                ref={inspectorScrollRef}
              >
                <div className="server-inspector-hero">
                  <div className="server-inspector-name-row">
                    <strong title={selectedProfile.name}>{selectedProfile.name}</strong>
                    <span className={`server-dashboard-status is-${selectedBadgeStatus}`}>
                      <StatusIndicator aria-hidden="true" status={selectedBadgeStatus} />
                      {statusLabel(selectedBadgeStatus)}
                    </span>
                  </div>
                  <p className="server-inspector-address" title={connectionProfileSummary(selectedProfile)}>
                    {selectedProfile.type === 'serial'
                      ? connectionProfileSummary(selectedProfile)
                      : `${selectedProfile.username}@${selectedProfile.host}:${selectedProfile.port}`}
                  </p>
                </div>

                <div className="server-inspector-actions">
                  {selectedSessionConnected && selectedSessionTab ? (
                    <button
                      className="server-inspector-action is-danger"
                      onClick={() => void onDisconnectSession(selectedSessionTab.id)}
                      type="button"
                    >
                      <MaterialIcon name="stop" />
                      {t.disconnect}
                    </button>
                  ) : (
                    <button
                      className="server-inspector-action is-primary"
                      disabled={selectedStatus?.status === 'connecting'}
                      onClick={() => onConnectProfile(selectedProfile.id)}
                      type="button"
                    >
                      <MaterialIcon name="cloud_sync" />
                      {selectedStatus?.status === 'connecting' ? t.serverDashboardConnecting : t.connect}
                    </button>
                  )}
                  <button className="server-inspector-action" onClick={openWorkspace} type="button">
                    <MaterialIcon name="terminal" />
                    {t.serverInspectorTerminal}
                  </button>
                  <button
                    className="server-inspector-action"
                    onClick={() => onEditConnection(selectedProfile)}
                    type="button"
                  >
                    <MaterialIcon name="edit" />
                    {t.edit}
                  </button>
                </div>

                <section className="server-inspector-card">
                  <header className="server-inspector-card-header">
                    <MaterialIcon name="dns" />
                    <strong>{t.serverInspectorConfig}</strong>
                    {isShowingCachedInsight && displayedInsight ? (
                      <small>
                        {t.serverInspectorLastUpdated.replace('{time}', formatClockTime(displayedInsight.updatedAt))}
                      </small>
                    ) : null}
                  </header>
                  {displayedInsight ? (
                    <dl className="server-inspector-config">
                      <div className="is-wide">
                        <dt>
                          <MaterialIcon name="dns" />
                          {t.serverInspectorSystem}
                        </dt>
                        <dd className="server-inspector-system" title={insightSystem ?? undefined}>
                          {insightSystem ?? '—'}
                        </dd>
                      </div>
                      <div>
                        <dt>
                          <MaterialIcon name="history" />
                          {t.serverInspectorUptime}
                        </dt>
                        <dd title={insightUptime ?? undefined}>{insightUptime ?? '—'}</dd>
                      </div>
                      <div className="is-wide">
                        <dt>
                          <MaterialIcon name="tune" />
                          {t.serverInspectorProcessor}
                        </dt>
                        <dd title={insightCpu ?? undefined}>{insightCpu ?? '—'}</dd>
                      </div>
                      <div>
                        <dt>
                          <MaterialIcon name="dashboard" />
                          {t.memory}
                        </dt>
                        <dd>{displayedInsight.memoryTotal ?? '—'}</dd>
                      </div>
                      <div className="is-wide">
                        <dt>
                          <MaterialIcon name="folder" />
                          {t.disk}
                        </dt>
                        <dd className="server-inspector-disk">
                          <span>{insightDisk ?? '—'}</span>
                          {insightDiskPercent === null ? null : (
                            <>
                              <span
                                aria-valuemax={100}
                                aria-valuemin={0}
                                aria-valuenow={Math.round(insightDiskPercent)}
                                className="server-inspector-disk-track"
                                role="progressbar"
                              >
                                <span style={{ width: `${Math.round(insightDiskPercent)}%` }} />
                              </span>
                              <span className="server-inspector-disk-percent">{Math.round(insightDiskPercent)}%</span>
                            </>
                          )}
                        </dd>
                      </div>
                    </dl>
                  ) : (
                    <p className="server-inspector-empty">{t.serverInspectorNoData}</p>
                  )}
                </section>

                <section className="server-inspector-card">
                  <header className="server-inspector-card-header">
                    <MaterialIcon name="history" />
                    <strong>{t.serverInspectorHistory}</strong>
                  </header>
                  {connectionHistory.length ? (
                    <ul className="server-inspector-history">
                      {connectionHistory.map((entry) => (
                        <li key={entry.id} title={entry.error}>
                          <span
                            aria-hidden="true"
                            className={`server-inspector-history-dot ${entry.ok ? 'is-success' : 'is-failure'}`}
                          />
                          <span className="server-inspector-history-time">{formatConnectionHistoryTime(entry.at)}</span>
                          <span className={`server-inspector-history-tag ${entry.ok ? 'is-success' : 'is-failure'}`}>
                            {entry.ok ? t.serverInspectorHistorySuccess : t.serverInspectorHistoryFailed}
                          </span>
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p className="server-inspector-empty">{t.serverInspectorHistoryEmpty}</p>
                  )}
                </section>
              </aside>
              <VerticalScrollbar ariaLabel={t.serverDashboardDetails} scrollRef={inspectorScrollRef} />
            </div>
          ) : null}
        </section>
      ) : null}
    </div>
  )
}
