import { useMemo, useRef, useState, type CSSProperties } from 'react'
import type { ConnectionFolder, ConnectionProfile, SessionSnapshot, SystemMetrics, WorkspaceTab } from '@fileterm/core'
import { MaterialIcon } from '../../components/common/material-icon/material-icon'
import { SidebarToggle } from '../../components/common/sidebar-toggle/sidebar-toggle'
import { connectionProfileSummary } from '../../app/connection-profile-summary'
import { tabStatusClass } from '../../app/app-utils'
import { t } from '../../i18n'
import { StatusIndicator, type StatusIndicatorStatus } from '../common/status-indicator'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { buildConnectionTree, flattenConnectionProfiles, type ConnectionTreeNode } from '../connections/connection-tree'
import { managerDropClass, resolveManagerDropPosition, type ManagerDropPosition } from '../common/manager-drag'
import { usePointerSortFallback, type PointerSortTarget } from '../../hooks/use-pointer-sort-fallback'
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

export function ServerManagementOverview({
  profiles,
  folders,
  sessions,
  workspaceTabs,
  selectedFolderId,
  onSelectFolder,
  onOpenProfile,
  onConnectProfile,
  onActivateSession,
  onCreateConnection,
  onEditConnection,
  onUpdateConnectionOrder
}: {
  profiles: ConnectionProfile[]
  folders: ConnectionFolder[]
  sessions: Record<string, SessionSnapshot>
  workspaceTabs: WorkspaceTab[]
  selectedFolderId: string | null
  onSelectFolder(folderId: string | null): void
  onOpenProfile(profileId: string): void
  onConnectProfile(profileId: string): void
  onActivateSession(tabId: string): void
  onCreateConnection(): void
  onEditConnection(profile: ConnectionProfile): void
  onUpdateConnectionOrder(
    id: string,
    newParentId: string | undefined,
    newOrder: number
  ): Promise<boolean> | boolean | void
}) {
  const [query, setQuery] = useState('')
  const [selectedProfileId, setSelectedProfileId] = useState<string | null>(profiles[0]?.id ?? null)
  const [isInspectorCollapsed, setIsInspectorCollapsed] = useState(false)
  const [dragState, setDragState] = useState<{
    draggingId: string | null
    targetId: string | null
    position: ManagerDropPosition | null
  }>({ draggingId: null, targetId: null, position: null })
  const mainScrollRef = useRef<HTMLDivElement>(null)
  const inspectorScrollRef = useRef<HTMLElement>(null)
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
  const selectedProfile =
    visibleProfiles.find((profile) => profile.id === selectedProfileId) ?? visibleProfiles[0] ?? null
  const selectedStatus = selectedProfile ? statusForProfile(selectedProfile.id, workspaceTabs, sessions) : null
  const selectedSessionTab = selectedStatus?.tab ?? null
  const selectedSessionConnected = Boolean(selectedSessionTab && sessions[selectedSessionTab.id]?.connected)
  const selectedMetrics = selectedProfile ? liveMetrics(selectedProfile.id, workspaceTabs, sessions) : null
  const connectedCount = baseProfiles.filter((profile) =>
    workspaceTabs.some((tab) => tab.profileId === profile.id && sessions[tab.id]?.connected)
  ).length
  const title = selectedFolder?.type === 'folder' ? selectedFolder.name : t.serverManagementOverview

  const openWorkspace = () => {
    if (!selectedProfile) return
    if (selectedSessionTab && selectedStatus?.status !== 'disconnected') onActivateSession(selectedSessionTab.id)
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
      className={`server-management-overview ${selectedProfile ? 'has-inspector' : ''} ${isInspectorCollapsed ? 'is-inspector-collapsed' : ''}`}
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
                      {connectionStatus ? (
                        <span className={`server-dashboard-status is-${connectionStatus.status}`}>
                          <StatusIndicator aria-hidden="true" status={connectionStatus.status} />
                          {connectionStatus.status === 'connected'
                            ? t.hostStatusConnected
                            : connectionStatus.status === 'connecting'
                              ? t.hostStatusConnecting
                              : connectionStatus.status === 'disconnected'
                                ? t.hostStatusDisconnected
                                : t.hostStatusIdle}
                        </span>
                      ) : (
                        <span className="server-dashboard-status is-idle">{t.serverDashboardNotConnected}</span>
                      )}
                    </span>
                    <span className="server-dashboard-address">{connectionProfileSummary(profile)}</span>
                    <span className="server-dashboard-resources">
                      <ResourceRing label="CPU" value={resources.cpu} />
                      <ResourceRing label={t.memory} value={resources.memory} detail={resources.memoryDetail} />
                      <ResourceRing label={t.disk} value={resources.disk} detail={resources.diskDetail} />
                    </span>
                    <span className="server-dashboard-card-foot">{profile.note || profile.type.toUpperCase()}</span>
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
            {!isInspectorCollapsed ? (
              <h2>
                <MaterialIcon name="dns" />
                {t.serverDashboardDetails}
              </h2>
            ) : null}
            <SidebarToggle
              className="server-dashboard-inspector-toggle"
              collapsed={isInspectorCollapsed}
              label={isInspectorCollapsed ? t.showConnectionDetails : t.hideConnectionDetails}
              onClick={() => setIsInspectorCollapsed((current) => !current)}
            />
          </header>
          {!isInspectorCollapsed ? (
            <div className="server-dashboard-inspector-scroll">
              <aside
                aria-label={t.serverDashboardDetails}
                className="server-dashboard-inspector"
                ref={inspectorScrollRef}
              >
                <dl>
                  <div>
                    <dt>{t.host}</dt>
                    <dd>{selectedProfile.host}</dd>
                  </div>
                  <div>
                    <dt>{t.port}</dt>
                    <dd>{selectedProfile.port}</dd>
                  </div>
                  <div>
                    <dt>{t.username}</dt>
                    <dd>{selectedProfile.username || '—'}</dd>
                  </div>
                  <div>
                    <dt>{t.serverDashboardSystem}</dt>
                    <dd>{selectedMetrics?.identity.osName ?? t.serverDashboardNoMetrics}</dd>
                  </div>
                  <div>
                    <dt>{t.note}</dt>
                    <dd>{selectedProfile.note || '—'}</dd>
                  </div>
                </dl>
                {selectedMetrics ? (
                  <div className="server-dashboard-inspector-metrics">
                    <ResourceRing label="CPU" value={selectedMetrics.cpuPercent} />
                    <ResourceRing
                      label={t.memory}
                      value={selectedMetrics.memoryPercent}
                      detail={selectedMetrics.memoryUsage}
                    />
                    <ResourceRing
                      label={t.disk}
                      value={diskPercent(selectedMetrics)}
                      detail={selectedMetrics.fileSystemRows[0]?.usagePercent}
                    />
                  </div>
                ) : (
                  <p className="server-dashboard-inspector-empty">{t.serverDashboardNoMetrics}</p>
                )}
                <div className="server-dashboard-inspector-actions">
                  <button
                    className="server-dashboard-connect"
                    disabled={selectedSessionConnected || selectedStatus?.status === 'connecting'}
                    onClick={() => onConnectProfile(selectedProfile.id)}
                    type="button"
                  >
                    <MaterialIcon name="cloud_sync" />
                    {selectedSessionConnected
                      ? t.serverDashboardConnected
                      : selectedStatus?.status === 'connecting'
                        ? t.serverDashboardConnecting
                        : t.serverDashboardConnect}
                  </button>
                  <button className="server-dashboard-open" onClick={openWorkspace} type="button">
                    <MaterialIcon name="terminal" />
                    {t.serverDashboardOpenWorkspace}
                  </button>
                  <button
                    className="server-dashboard-edit"
                    onClick={() => onEditConnection(selectedProfile)}
                    type="button"
                  >
                    <MaterialIcon name="edit" />
                    {t.serverDashboardEdit}
                  </button>
                </div>
              </aside>
              <VerticalScrollbar ariaLabel={t.serverDashboardDetails} scrollRef={inspectorScrollRef} />
            </div>
          ) : null}
        </section>
      ) : null}
    </div>
  )
}
