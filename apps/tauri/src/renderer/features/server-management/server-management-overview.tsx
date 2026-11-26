import { useEffect, useMemo, useReducer, useRef, useState, type CSSProperties, type MouseEvent } from 'react'
import {
  COMMAND_BATCH_TARGET_LIMIT,
  type CommandTemplate,
  type CommandFolder,
  type ConnectionFolder,
  type ConnectionProfile,
  type SessionSnapshot,
  type SystemMetrics,
  type WorkspaceTab
} from '@fileterm/core'
import { Button } from '../../components/common/button/button'
import { DropdownSelect } from '../../components/common/dropdown-select/dropdown-select'
import { Input } from '../../components/common/input/input'
import { MaterialIcon } from '../../components/common/material-icon/material-icon'
import { SegmentedControl } from '../../components/common/segmented-control/segmented-control'
import { SidebarToggle } from '../../components/common/sidebar-toggle/sidebar-toggle'
import { deviceRoleIcon, deviceRoleLabel, resolveProfileDeviceRole } from '../../app/device-role'
import { connectionProfileSummary } from '../../app/connection-profile-summary'
import { MaskedAddressText } from '../system/system-resource-meters'
import { formatMessage, getLocale, t } from '../../i18n'
import {
  getConnectionHistory,
  getServerInsight,
  hydrateServerInsights,
  serverInsightFromMetrics,
  subscribeServerInsights,
  type ServerInsight
} from './server-insights-store'
import { AppIcon } from '../common/app-icon'
import { SelectionControl } from '../common/selection-control'
import { StatusIndicator, type StatusIndicatorStatus } from '../common/status-indicator'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { buildConnectionTree, connectionFolderOptions, flattenConnectionProfiles } from '../connections/connection-tree'
import { managerDropClass, resolveManagerDropPosition, type ManagerDropPosition } from '../common/manager-drag'
import { usePointerSortFallback, type PointerSortTarget } from '../../hooks/use-pointer-sort-fallback'
import { resolveMonitoringSource } from '../system/shared-monitoring'
import { statusForProfile, statusLabel } from './server-profile-status'
import { ServerProfileActions } from './server-profile-actions'
import { ServerManagementList } from './server-management-list'
import type { ServerListSort } from './server-management-list-model'
import { ServerCommandBatchDialog, type ServerCommandBatchDialogMode } from './server-command-batch-dialog'
import { getCommandBatchTargets } from './server-command-batch-model'
import './server-management-overview.css'
import './server-management-page-header.css'

const MAX_METRIC_AGE_MS = 120_000

const SERVER_STATUS_FILTERS = ['all', 'connected', 'connecting', 'idle'] as const

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
  commandFolders,
  commandTemplates,
  folders,
  maskConnectionAddresses,
  sessions,
  workspaceTabs,
  focusedProfileId,
  focusedSessionTabId,
  focusRequest,
  isFocusMode,
  selectedFolderId,
  onSelectFolder,
  onConnectProfile,
  onDisconnectSession,
  onActivateSession,
  onCreateConnection,
  onEditConnection,
  onUpdateConnectionOrder,
  onToggleFocusMode
}: {
  profiles: ConnectionProfile[]
  commandFolders: CommandFolder[]
  commandTemplates: CommandTemplate[]
  folders: ConnectionFolder[]
  maskConnectionAddresses: boolean
  sessions: Record<string, SessionSnapshot>
  workspaceTabs: WorkspaceTab[]
  focusedProfileId?: string | null
  focusedSessionTabId?: string | null
  focusRequest?: number
  isFocusMode?: boolean
  selectedFolderId: string | null
  onSelectFolder(folderId: string | null): void
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
  const [statusFilter, setStatusFilter] = useState<(typeof SERVER_STATUS_FILTERS)[number]>('all')
  const [selectedTag, setSelectedTag] = useState('')
  const [view, setView] = useState<'cards' | 'list'>('cards')
  const [listSort, setListSort] = useState<ServerListSort | null>(null)
  const [checkedProfiles, setCheckedProfiles] = useState<Set<string>>(() => new Set())
  const [batchDialog, setBatchDialog] = useState<ServerCommandBatchDialogMode | null>(null)
  const [selectedProfileId, setSelectedProfileId] = useState<string | null | undefined>(undefined)
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
  const folderOptions = useMemo(() => connectionFolderOptions(tree.roots), [tree])
  const folderNode = selectedFolderId ? tree.map.get(selectedFolderId) : undefined
  const selectedFolder = folderNode?.type === 'folder' ? folderNode : undefined
  const baseProfiles = selectedFolder
    ? flattenConnectionProfiles([selectedFolder])
    : flattenConnectionProfiles(tree.roots)
  const search = query.trim().toLocaleLowerCase()
  const tags = [...new Set(profiles.flatMap((profile) => profile.tags ?? []))].sort((first, second) =>
    first.localeCompare(second)
  )
  const tagFilter = selectedTag
  if (tagFilter && !tags.includes(tagFilter)) tags.push(tagFilter)
  const statusByProfile = new Map(
    baseProfiles.map((profile) => [profile.id, statusForProfile(profile.id, workspaceTabs, sessions)])
  )
  const statusCounts = { all: baseProfiles.length, connected: 0, connecting: 0, idle: 0 }
  const filterStatus = (status: StatusIndicatorStatus | undefined) =>
    status === 'connected' || status === 'connecting' ? status : 'idle'
  const matchingProfiles = baseProfiles.filter(
    (profile) =>
      (!tagFilter || profile.tags?.includes(tagFilter)) &&
      [profile.name, profile.host, profile.note, profile.username, ...(profile.tags ?? [])].some((value) =>
        value?.toLocaleLowerCase().includes(search)
      )
  )
  statusCounts.all = matchingProfiles.length
  for (const profile of matchingProfiles) {
    statusCounts[filterStatus(statusByProfile.get(profile.id)?.status)]++
  }
  const isFiltered = Boolean(search || tagFilter) || statusFilter !== 'all'
  const visibleProfiles = matchingProfiles.filter(
    (profile) => statusFilter === 'all' || filterStatus(statusByProfile.get(profile.id)?.status) === statusFilter
  )
  const batchTargets = getCommandBatchTargets(workspaceTabs, sessions)
  const selectedTargets = visibleProfiles.flatMap((profile) => {
    const target = batchTargets.get(profile.id)
    return target && checkedProfiles.has(profile.id)
      ? [{ ...target, name: profile.name, address: connectionProfileSummary(profile) }]
      : []
  })
  const batchLimitReached = selectedTargets.length >= COMMAND_BATCH_TARGET_LIMIT
  const checkBatchProfile = (id: string, checked: boolean) => {
    const next = new Set(selectedTargets.map(({ profileId }) => profileId))
    if (checked) next.add(id)
    else next.delete(id)
    setCheckedProfiles(next)
  }
  const hasVisibleBatchTarget = visibleProfiles.some(({ id }) => batchTargets.has(id))
  const filterLabels = {
    all: t.allConnections,
    connected: t.hostStatusConnected,
    connecting: t.hostStatusConnecting,
    idle: t.serverDashboardNotConnected
  }
  const firstBaseProfileId = baseProfiles[0]?.id ?? null
  useEffect(() => {
    setSelectedProfileId((current) => {
      // An explicit clear must survive connection and metrics updates.
      if (current === null || baseProfiles.some((profile) => profile.id === current)) return current
      return firstBaseProfileId ?? undefined
    })
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
    selectedProfileId === null
      ? null
      : (visibleProfiles.find((profile) => profile.id === selectedProfileId) ?? visibleProfiles[0] ?? null)
  const selectedStatus = selectedProfile ? statusByProfile.get(selectedProfile.id) : null
  const selectedBadgeStatus: StatusIndicatorStatus =
    selectedStatus?.status === 'connected' || selectedStatus?.status === 'connecting' ? selectedStatus.status : 'idle'
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
  const title = selectedFolder ? selectedFolder.name : t.serverDashboardTitle
  const clearSelection = () => setSelectedProfileId(null)
  const clearOnBackgroundClick = (event: MouseEvent<HTMLElement>) => {
    if (event.target instanceof HTMLElement && event.target.hasAttribute('data-clear-selection')) clearSelection()
  }

  const openBatch = () => {
    if (!commandTemplates.length) {
      setBatchDialog({ type: 'prepare', message: t.serverBatchNoTemplates })
      return
    }
    if (selectedTargets.length) {
      setBatchDialog({ type: 'run', targets: selectedTargets })
      return
    }
    const canConnect = (profile: ConnectionProfile) =>
      profile.type === 'ssh' &&
      profile.deviceMode !== 'network-device' &&
      filterStatus(statusByProfile.get(profile.id)?.status) === 'idle'
    const profile = selectedProfile && canConnect(selectedProfile) ? selectedProfile : visibleProfiles.find(canConnect)
    setBatchDialog({
      type: 'prepare',
      message: hasVisibleBatchTarget ? t.serverBatchSelectFirst : t.serverBatchConnectFirst,
      connect:
        !hasVisibleBatchTarget && profile
          ? {
              label: formatMessage(t.serverBatchConnectHost, { name: profile.name }),
              onConnect: () => {
                if (statusFilter === 'idle') setStatusFilter('all')
                setSelectedProfileId(profile.id)
                onConnectProfile(profile.id)
              }
            }
          : undefined
    })
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
    <div className={`server-management-overview has-inspector ${inspectorCollapsed ? 'is-inspector-collapsed' : ''}`}>
      <div className="server-dashboard-main-scroll">
        <main
          className={`server-dashboard-main${view === 'list' ? ' is-list-view' : ''}`}
          data-clear-selection
          onClick={clearOnBackgroundClick}
          ref={mainScrollRef}
        >
          <header className="server-management-page-header" data-clear-selection>
            <h1>
              <MaterialIcon name="dashboard" />
              <span>{title}</span>
            </h1>
            <Button icon={<MaterialIcon name="add" />} onClick={onCreateConnection} variant="primary">
              {t.newConnection}
            </Button>
          </header>

          <div className="server-dashboard-toolbar" data-clear-selection>
            <div className="server-dashboard-filters" data-clear-selection>
              {folderOptions.length > 0 && (
                <DropdownSelect
                  ariaLabel={t.folder}
                  className="server-dashboard-folder-filter"
                  menuPlacement="auto"
                  menuWidth="auto"
                  onChange={(id) => onSelectFolder(id || null)}
                  options={[{ value: '', label: t.allConnections }, ...folderOptions]}
                  value={selectedFolder?.id ?? ''}
                />
              )}
              <SegmentedControl<(typeof SERVER_STATUS_FILTERS)[number]>
                ariaLabel={t.serverDashboardStatusFilter}
                className="server-dashboard-status-filter-control"
                onChange={setStatusFilter}
                options={SERVER_STATUS_FILTERS.map((status) => ({
                  value: status,
                  label: (
                    <>
                      {status !== 'all' ? <StatusIndicator aria-hidden="true" status={status} /> : null}
                      {filterLabels[status]}
                      <span className="server-dashboard-filter-count">{statusCounts[status]}</span>
                    </>
                  )
                }))}
                value={statusFilter}
                variant="neutral"
              />
              {tags.length ? (
                <DropdownSelect
                  ariaLabel={t.connectionTagFilter}
                  className="server-dashboard-tag-filter"
                  onChange={setSelectedTag}
                  options={[
                    { value: '', label: t.connectionAllTags },
                    ...tags.map((tag) => ({ value: tag, label: tag }))
                  ]}
                  value={tagFilter}
                />
              ) : null}
            </div>
            <div className="server-dashboard-search-row" data-clear-selection>
              <Input
                aria-label={t.serverDashboardSearchPlaceholder}
                className="server-dashboard-search"
                onChange={(event) => setQuery(event.target.value)}
                placeholder={t.serverDashboardSearchPlaceholder}
                prefix={<AppIcon name="search" size={14} />}
                type="search"
                value={query}
              />
              <SegmentedControl<typeof view>
                ariaLabel={t.serverDashboardView}
                onChange={setView}
                options={[
                  { value: 'cards', label: t.serverDashboardCards },
                  { value: 'list', label: t.serverDashboardList }
                ]}
                value={view}
                variant="neutral"
              />
            </div>
          </div>

          <div className="server-dashboard-batch-actions" data-clear-selection>
            <Button
              disabled={!window.fileterm || selectedTargets.length > COMMAND_BATCH_TARGET_LIMIT}
              onClick={openBatch}
              size="panel"
            >
              {t.serverBatchRun}
              {selectedTargets.length ? ` (${selectedTargets.length})` : ''}
            </Button>
            <Button disabled={!window.fileterm} onClick={() => setBatchDialog({ type: 'history' })} size="panel">
              {t.serverBatchHistory}
            </Button>
            <span>
              {hasVisibleBatchTarget
                ? formatMessage(t.serverBatchSelectionHint, { count: COMMAND_BATCH_TARGET_LIMIT })
                : t.serverBatchConnectFirst}
            </span>
          </div>

          {visibleProfiles.length && view === 'list' ? (
            <ServerManagementList
              batchEligible={(id) => batchTargets.has(id)}
              batchLimitReached={batchLimitReached}
              checkedProfiles={checkedProfiles}
              maskConnectionAddresses={maskConnectionAddresses}
              label={title}
              onCheck={checkBatchProfile}
              onClearSelection={clearSelection}
              onSelect={setSelectedProfileId}
              onSortChange={setListSort}
              onTagClick={(tag) => setSelectedTag((current) => (current === tag ? '' : tag))}
              rows={visibleProfiles.map((profile) => ({
                id: profile.id,
                name: profile.name,
                address: connectionProfileSummary(profile),
                protocol: profile.type,
                status: filterStatus(statusByProfile.get(profile.id)?.status),
                tags: (profile.tags ?? []).join(', '),
                tagList: profile.tags ?? [],
                deviceRole: profile.deviceRole
              }))}
              selectedId={selectedProfile?.id}
              sort={listSort}
            />
          ) : visibleProfiles.length ? (
            <section aria-label={title} className="server-dashboard-grid" data-clear-selection>
              {visibleProfiles.map((profile) => {
                const connectionBadgeStatus = filterStatus(statusByProfile.get(profile.id)?.status)
                const metrics = liveMetrics(profile.id, workspaceTabs, sessions)
                const resources = getResourceMetrics(metrics)
                const isSelected = selectedProfile?.id === profile.id
                const checked = checkedProfiles.has(profile.id)
                return (
                  <article
                    className={`server-dashboard-card ${isSelected ? 'is-selected' : ''} ${isFiltered ? 'is-sort-disabled' : ''} ${dragState.draggingId === profile.id ? 'is-dragging' : ''} ${managerDropClass(dragState.targetId === profile.id, dragState.position)}`}
                    data-fileterm-sort-id={isFiltered ? undefined : profile.id}
                    data-fileterm-sort-kind={isFiltered ? undefined : 'profile'}
                    key={profile.id}
                    onClick={() => setSelectedProfileId(profile.id)}
                    onPointerDown={(event) => {
                      if (!isFiltered) handlePointerSort(event, profile.id)
                    }}
                  >
                    <span className="server-dashboard-card-heading">
                      {profile.type === 'serial' ? (
                        <MaterialIcon name="usb" />
                      ) : (
                        <AppIcon
                          className="server-dashboard-card-icon"
                          name={deviceRoleIcon(profile.deviceRole)}
                          size={16}
                        />
                      )}
                      <button aria-pressed={isSelected} className="server-dashboard-card-name" type="button">
                        {profile.name}
                      </button>
                      {connectionBadgeStatus === 'idle' ? (
                        <span className="server-dashboard-status is-idle">{t.serverDashboardNotConnected}</span>
                      ) : (
                        <span className={`server-dashboard-status is-${connectionBadgeStatus}`}>
                          <StatusIndicator aria-hidden="true" status={connectionBadgeStatus} />
                          {statusLabel(connectionBadgeStatus)}
                        </span>
                      )}
                      <span className="server-dashboard-address">
                        {maskConnectionAddresses && profile.type !== 'serial' ? (
                          <MaskedAddressText value={connectionProfileSummary(profile)} />
                        ) : (
                          connectionProfileSummary(profile)
                        )}
                      </span>
                    </span>
                    <span className="server-dashboard-resources">
                      <ResourceRing label="CPU" value={resources.cpu} />
                      <ResourceRing label={t.memory} value={resources.memory} detail={resources.memoryDetail} />
                      <ResourceRing label={t.disk} value={resources.disk} detail={resources.diskDetail} />
                    </span>
                    <span className="server-dashboard-card-foot">
                      {profile.tags?.length ? (
                        <span
                          className="server-dashboard-card-tags"
                          onClick={(event) => event.stopPropagation()}
                          onPointerDown={(event) => event.stopPropagation()}
                        >
                          {profile.tags.slice(0, 3).map((tag) => (
                            <button
                              className={`server-dashboard-tag-chip${selectedTag === tag ? ' is-active' : ''}`}
                              key={tag}
                              title={tag}
                              type="button"
                              onClick={() => setSelectedTag((current) => (current === tag ? '' : tag))}
                            >
                              {tag}
                            </button>
                          ))}
                          {profile.tags.length > 3 ? (
                            <span className="server-dashboard-tag-more">+{profile.tags.length - 3}</span>
                          ) : null}
                        </span>
                      ) : profile.note ? (
                        <span className="server-dashboard-card-note" title={profile.note}>
                          {profile.note}
                        </span>
                      ) : null}
                      <span className="server-dashboard-card-type">{profile.type.toUpperCase()}</span>
                      {batchTargets.has(profile.id) && (
                        <label
                          className="server-dashboard-card-check"
                          onClick={(event) => event.stopPropagation()}
                          onPointerDown={(event) => event.stopPropagation()}
                        >
                          <SelectionControl
                            aria-label={formatMessage(t.serverBatchSelectHost, { name: profile.name })}
                            checked={checked}
                            disabled={!checked && batchLimitReached}
                            onChange={(event) => checkBatchProfile(profile.id, event.target.checked)}
                            type="checkbox"
                          />
                        </label>
                      )}
                    </span>
                  </article>
                )
              })}
            </section>
          ) : (
            <div className="server-dashboard-empty" data-clear-selection>
              <MaterialIcon name={isFiltered ? 'search' : 'dns'} />
              <p>{isFiltered ? t.serverDashboardNoResults : t.noSavedConnections}</p>
            </div>
          )}
        </main>
        <VerticalScrollbar ariaLabel={title} scrollRef={mainScrollRef} />
      </div>
      {batchDialog && (
        <ServerCommandBatchDialog
          folders={commandFolders}
          mode={batchDialog}
          onClose={() => setBatchDialog(null)}
          templates={commandTemplates}
        />
      )}

      {selectedProfile ? (
        <section className="server-dashboard-inspector-region">
          <header className="server-dashboard-inspector-header">
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
            {!inspectorCollapsed ? <h2>{t.serverDashboardDetails}</h2> : null}
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
                  <p
                    className="server-inspector-address"
                    title={
                      maskConnectionAddresses && selectedProfile.type !== 'serial'
                        ? undefined
                        : connectionProfileSummary(selectedProfile)
                    }
                  >
                    {maskConnectionAddresses && selectedProfile.type !== 'serial' ? (
                      <MaskedAddressText
                        value={`${selectedProfile.username}@${selectedProfile.host}:${selectedProfile.port}`}
                      />
                    ) : selectedProfile.type === 'serial' ? (
                      connectionProfileSummary(selectedProfile)
                    ) : (
                      `${selectedProfile.username}@${selectedProfile.host}:${selectedProfile.port}`
                    )}
                  </p>
                  <div className="server-inspector-meta">
                    <span className="server-inspector-role">
                      <AppIcon name={deviceRoleIcon(selectedProfile.deviceRole)} size={12} />
                      <span>{deviceRoleLabel(resolveProfileDeviceRole(selectedProfile))}</span>
                    </span>
                    {selectedProfile.tags?.map((tag) => (
                      <button
                        className={`server-dashboard-tag-chip${selectedTag === tag ? ' is-active' : ''}`}
                        key={tag}
                        title={tag}
                        type="button"
                        onClick={() => setSelectedTag((current) => (current === tag ? '' : tag))}
                      >
                        {tag}
                      </button>
                    ))}
                  </div>
                </div>

                <ServerProfileActions
                  profile={selectedProfile}
                  workspaceTabs={workspaceTabs}
                  sessions={sessions}
                  focusedSessionTabId={focusedSessionTabId}
                  onConnectProfile={onConnectProfile}
                  onDisconnectSession={onDisconnectSession}
                  onActivateSession={onActivateSession}
                  onEditConnection={onEditConnection}
                />

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
      ) : (
        <section className="server-dashboard-inspector-region">
          <header className="server-dashboard-inspector-header">
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
            {!inspectorCollapsed ? <h2>{t.serverDashboardDetails}</h2> : null}
          </header>
          {!inspectorCollapsed ? (
            <div className="server-dashboard-inspector-scroll">
              <aside
                aria-label={t.serverDashboardDetails}
                className="server-dashboard-inspector"
                ref={inspectorScrollRef}
              >
                <p className="server-inspector-empty server-dashboard-no-selection">
                  {t.serverDashboardSelectConnection}
                </p>
              </aside>
              <VerticalScrollbar ariaLabel={t.serverDashboardDetails} scrollRef={inspectorScrollRef} />
            </div>
          ) : null}
        </section>
      )}
    </div>
  )
}
