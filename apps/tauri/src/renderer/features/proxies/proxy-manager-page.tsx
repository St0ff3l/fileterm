import { useEffect, useMemo, useRef, useState } from 'react'
import type {
  ConnectionProfile,
  CreateProxyProfileInput,
  CreateTunnelProfileInput,
  ProxyProfile,
  TunnelProfile,
  UpdateProxyProfileInput,
  UpdateTunnelProfileInput
} from '@fileterm/core'
import { t } from '../../i18n'
import { useProxyLibrary } from '../../hooks/use-proxy-library'
import { useTunnelLibrary } from '../../hooks/use-tunnel-library'
import { AppIcon } from '../common/app-icon'
import { ConfirmActionDialog } from '../common/confirm-action-dialog'
import { ContextMenu } from '../common/context-menu'
import { targetsNestedManagerControl } from '../common/manager-interactions'
import { SshProxyAssignmentDialog } from './ssh-proxy-assignment-dialog'
import { ProxyEditDialog } from './proxy-edit-dialog'
import { TunnelEditDialog } from '../tunnels/tunnel-edit-dialog'
import { SshRouteManager } from './ssh-route-manager'
import { SegmentedControl } from '../../components/common/segmented-control/segmented-control'
import './proxy-manager-page.css'

type ProtocolFilter = 'ssh' | 'http-tunnel' | 'socks5' | 'http-connect'
type ActiveFilter = 'all' | ProtocolFilter

type ManagerEntry = { kind: 'proxy'; item: ProxyProfile } | { kind: 'tunnel'; item: TunnelProfile }

const protocolOrder: ProtocolFilter[] = ['ssh', 'http-tunnel', 'socks5', 'http-connect']

function entryProtocol(entry: ManagerEntry): ProtocolFilter {
  if (entry.item.type !== 'http') return entry.item.type
  return entry.kind === 'tunnel' ? 'http-tunnel' : 'http-connect'
}

export interface ProxyManagerStats {
  totalCount: number
  proxyCount: number
  tunnelCount: number
  socks5Count: number
  httpProxyCount: number
  sshTunnelCount: number
  httpTunnelCount: number
}

export function ProxyManagerPage({
  profiles = [],
  onStatsChange,
  onActiveFilterChange
}: {
  profiles?: ConnectionProfile[]
  onStatsChange?(stats: ProxyManagerStats): void
  onActiveFilterChange?(label: string): void
} = {}) {
  const proxyLibrary = useProxyLibrary()
  const tunnelLibrary = useTunnelLibrary()
  const { proxies, error: proxyError, clearError: clearProxyError, saveProxy, deleteProxy, testProxy } = proxyLibrary
  const { tunnels, error: tunnelError, clearError: clearTunnelError, saveTunnel, deleteTunnel } = tunnelLibrary
  const [assignmentProxy, setAssignmentProxy] = useState<ProxyProfile | null>(null)
  const [assignmentNotice, setAssignmentNotice] = useState('')
  const [showRoutes, setShowRoutes] = useState(false)
  const [activeFilter, setActiveFilter] = useState<ActiveFilter>('all')
  const [searchQuery, setSearchQuery] = useState('')
  const [isActionsExpanded, setIsActionsExpanded] = useState(false)
  const [isProxyModalOpen, setIsProxyModalOpen] = useState(false)
  const [isTunnelModalOpen, setIsTunnelModalOpen] = useState(false)
  const [editingProxy, setEditingProxy] = useState<ProxyProfile | null>(null)
  const [editingTunnel, setEditingTunnel] = useState<TunnelProfile | null>(null)
  const [createProxyType, setCreateProxyType] = useState<'socks5' | 'http'>('socks5')
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [deletingEntry, setDeletingEntry] = useState<ManagerEntry | null>(null)
  const editorOpenInFlightRef = useRef(false)
  const [contextMenu, setContextMenu] = useState<{ entry: ManagerEntry; position: { x: number; y: number } } | null>(
    null
  )
  const [deleteError, setDeleteError] = useState<string | null>(null)
  const [testStates, setTestStates] = useState<
    Record<string, { testing: boolean; success?: boolean; latencyMs?: number }>
  >({})

  const stats = useMemo<ProxyManagerStats>(
    () => ({
      totalCount: proxies.length + tunnels.length,
      proxyCount: proxies.length,
      tunnelCount: tunnels.length,
      socks5Count: proxies.filter((proxy) => proxy.type === 'socks5').length,
      httpProxyCount: proxies.filter((proxy) => proxy.type === 'http').length,
      sshTunnelCount: tunnels.filter((tunnel) => tunnel.type === 'ssh').length,
      httpTunnelCount: tunnels.filter((tunnel) => tunnel.type === 'http').length
    }),
    [proxies, tunnels]
  )

  const entries = useMemo<ManagerEntry[]>(() => {
    const query = searchQuery.trim().toLowerCase()
    const matchesQuery = (entry: ManagerEntry) => {
      if (!query) return true
      if (entry.kind === 'proxy') {
        const proxy = entry.item
        return [proxy.name, proxy.host, proxy.port, proxy.username, proxy.type].some((value) =>
          String(value ?? '')
            .toLowerCase()
            .includes(query)
        )
      }
      const tunnel = entry.item
      return [
        tunnel.name,
        tunnel.type,
        tunnel.scriptUrl,
        ...(tunnel.forwards ?? []).flatMap((rule) => [rule.bindHost, rule.bindPort, rule.targetHost, rule.targetPort])
      ].some((value) =>
        String(value ?? '')
          .toLowerCase()
          .includes(query)
      )
    }
    return [
      ...tunnels.map((item) => ({ kind: 'tunnel' as const, item })),
      ...proxies.map((item) => ({ kind: 'proxy' as const, item }))
    ]
      .filter((entry) => (activeFilter === 'all' || activeFilter === entryProtocol(entry)) && matchesQuery(entry))
      .sort(
        (left, right) =>
          protocolOrder.indexOf(entryProtocol(left)) - protocolOrder.indexOf(entryProtocol(right)) ||
          (right.item.updatedAt ?? right.item.createdAt ?? 0) - (left.item.updatedAt ?? left.item.createdAt ?? 0)
      )
  }, [activeFilter, proxies, searchQuery, tunnels])

  const filters = {
    all: { label: t.allProxiesAndTunnels, count: stats.totalCount, icon: 'shield' },
    ssh: { label: t.sshTunnelProfiles, count: stats.sshTunnelCount, icon: 'connections' },
    'http-tunnel': { label: t.httpTunnels, count: stats.httpTunnelCount, icon: 'cloud' },
    socks5: { label: t.socks5Proxies, count: stats.socks5Count, icon: 'brand' },
    'http-connect': { label: t.httpConnectProxies, count: stats.httpProxyCount, icon: 'server' }
  } as const
  const activeFilterLabel = showRoutes ? t.sshRouteManager : filters[activeFilter].label
  const renderFilter = (filter: ActiveFilter) => (
    <FilterButton
      key={filter}
      {...filters[filter]}
      active={activeFilter === filter}
      onClick={() => setActiveFilter(filter)}
    />
  )

  useEffect(() => {
    onStatsChange?.(stats)
  }, [onStatsChange, stats])

  useEffect(() => {
    onActiveFilterChange?.(activeFilterLabel)
  }, [activeFilterLabel, onActiveFilterChange])

  const openProxyCreate = (type: 'socks5' | 'http') => {
    if (window.fileterm?.openProxyFormWindow) {
      void window.fileterm.openProxyFormWindow('create', undefined, type)
      return
    }
    setEditingProxy(null)
    setCreateProxyType(type)
    setIsProxyModalOpen(true)
  }

  const openTunnelCreate = () => {
    if (window.fileterm?.openTunnelFormWindow) {
      void window.fileterm.openTunnelFormWindow('create')
      return
    }
    setEditingTunnel(null)
    setIsTunnelModalOpen(true)
  }

  const openEntry = async (entry: ManagerEntry) => {
    if (editorOpenInFlightRef.current) return
    editorOpenInFlightRef.current = true
    try {
      if (entry.kind === 'proxy') {
        if (window.fileterm?.openProxyFormWindow) {
          await window.fileterm.openProxyFormWindow('edit', entry.item.id)
          return
        }
        setEditingProxy(entry.item)
        setIsProxyModalOpen(true)
        return
      }
      if (window.fileterm?.openTunnelFormWindow) {
        await window.fileterm.openTunnelFormWindow('edit', entry.item.id)
        return
      }
      setEditingTunnel(entry.item)
      setIsTunnelModalOpen(true)
    } catch (error) {
      console.error('Failed to open proxy or tunnel editor:', error)
    } finally {
      editorOpenInFlightRef.current = false
    }
  }

  const handleDeleteConfirm = async () => {
    if (!deletingEntry) return
    setIsSubmitting(true)
    setDeleteError(null)
    try {
      if (deletingEntry.kind === 'proxy') await deleteProxy(deletingEntry.item.id)
      else await deleteTunnel(deletingEntry.item.id)
      setDeletingEntry(null)
    } catch (cause) {
      setDeleteError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setIsSubmitting(false)
    }
  }

  const handleTestProxy = async (proxy: ProxyProfile) => {
    setTestStates((current) => ({ ...current, [proxy.id]: { testing: true } }))
    const result = await testProxy({
      name: proxy.name,
      type: proxy.type,
      host: proxy.host,
      port: proxy.port,
      username: proxy.username
    })
    setTestStates((current) => ({
      ...current,
      [proxy.id]: { testing: false, success: result.success, latencyMs: result.latencyMs }
    }))
  }

  const openAssignments = (proxy: ProxyProfile) => {
    setAssignmentNotice('')
    setAssignmentProxy(proxy)
  }

  const libraryError = proxyError || tunnelError
  const contextEntry = contextMenu?.entry ?? null
  const contextProxy = contextEntry?.kind === 'proxy' ? contextEntry.item : null

  return (
    <div className="modal-card manager-modal connection-manager-modal proxy-manager-modal manager-inline">
      <div className="connection-manager-header proxy-manager-header">
        <span className="connection-manager-title">
          <AppIcon className="proxy-manager-title-icon" name="shield" size={26} />
          <span>{t.proxyManager}</span>
        </span>
        <div className="proxy-manager-header-controls">
          <SegmentedControl
            ariaLabel={t.proxyManager}
            variant="neutral"
            value={showRoutes ? 'routes' : 'profiles'}
            options={[
              { value: 'profiles', label: t.proxyManager },
              { value: 'routes', label: t.sshRouteManager }
            ]}
            onChange={(value) => setShowRoutes(value === 'routes')}
          />
          {!showRoutes ? (
            <label className="connection-manager-search">
              <AppIcon name="search" size={14} />
              <input
                aria-label={t.filterProxies}
                placeholder={t.filterProxies}
                type="search"
                value={searchQuery}
                onChange={(event) => setSearchQuery(event.target.value)}
              />
            </label>
          ) : null}
        </div>
      </div>

      {libraryError ? (
        <div className="connection-manager-error-banner">
          <span>{libraryError}</span>
          <button
            className="flat-button compact"
            type="button"
            onClick={() => {
              clearProxyError()
              clearTunnelError()
            }}
          >
            {t.confirm}
          </button>
        </div>
      ) : null}

      {assignmentNotice ? (
        <div className="proxy-assignment-notice" role="status">
          {assignmentNotice}
        </div>
      ) : null}

      {showRoutes ? (
        <SshRouteManager profiles={profiles} proxies={proxies} />
      ) : (
        <div className="connection-manager-layout">
          <aside className="connection-manager-sidebar" aria-label="Proxy and tunnel filters">
            {renderFilter('all')}
            {renderFilter('socks5')}
            {renderFilter('http-connect')}
            {renderFilter('ssh')}
            {renderFilter('http-tunnel')}
          </aside>

          <section className="connection-manager-main">
            <div className="manager-table connection-manager-table proxy-manager-table">
              <div className="manager-head">
                <span>{t.name}</span>
                <span>{t.host}</span>
                <span>{t.port}</span>
                <span>{t.proxyAuthStatus}</span>
                <span>{t.type}</span>
                <span>{t.proxyBoundConnections}</span>
                <span>{t.actions}</span>
              </div>
              <div className="manager-body connection-manager-body">
                {entries.map((entry) => (
                  <ManagerRow
                    key={`${entry.kind}:${entry.item.id}`}
                    entry={entry}
                    testState={entry.kind === 'proxy' ? testStates[entry.item.id] : undefined}
                    onEdit={() => openEntry(entry)}
                    onManageAssignments={entry.kind === 'proxy' ? () => openAssignments(entry.item) : undefined}
                    onDelete={() => {
                      setDeletingEntry(entry)
                      setDeleteError(null)
                    }}
                    onContextMenu={(event) => {
                      event.preventDefault()
                      event.stopPropagation()
                      setContextMenu({ entry, position: { x: event.clientX, y: event.clientY } })
                    }}
                    onTest={() => {
                      if (entry.kind === 'proxy') void handleTestProxy(entry.item)
                    }}
                  />
                ))}
                {entries.length === 0 ? (
                  <div className="connection-manager-empty">
                    {searchQuery.trim() ? t.noMatchingConnections : t.noProxiesTitle}
                  </div>
                ) : null}
              </div>
            </div>

            <div className={`connection-manager-floating-drawer ${isActionsExpanded ? 'expanded' : ''}`}>
              <div className="drawer-options-wrapper">
                <button
                  className="drawer-option-btn secondary-btn"
                  type="button"
                  onClick={() => {
                    openTunnelCreate()
                    setIsActionsExpanded(false)
                  }}
                >
                  <AppIcon name="plus" size={13} />
                  <span>{t.addTunnel}</span>
                </button>
                <button
                  className="drawer-option-btn primary-btn"
                  type="button"
                  onClick={() => {
                    openProxyCreate('socks5')
                    setIsActionsExpanded(false)
                  }}
                >
                  <AppIcon name="plus" size={13} />
                  <span>{t.addProxy}</span>
                </button>
              </div>
              <button
                aria-label={t.expandActions || '操作'}
                className="drawer-trigger-btn"
                type="button"
                onClick={() => setIsActionsExpanded((expanded) => !expanded)}
              >
                <AppIcon name="plus" size={16} />
              </button>
            </div>
          </section>
        </div>
      )}

      {assignmentProxy ? (
        <SshProxyAssignmentDialog
          onClose={() => setAssignmentProxy(null)}
          onSaved={(refreshed) => {
            setAssignmentProxy(null)
            setAssignmentNotice(refreshed ? t.proxyAssignmentAppliesOnConnect : t.proxyAssignmentRefreshFailed)
          }}
          profiles={profiles}
          proxies={proxies}
          proxy={assignmentProxy}
        />
      ) : null}
      {deletingEntry ? (
        <ConfirmActionDialog
          confirmLabel={t.delete}
          confirmVariant="danger"
          description={deleteDescription(deletingEntry, deleteError)}
          isSubmitting={isSubmitting}
          onClose={() => {
            setDeletingEntry(null)
            setDeleteError(null)
          }}
          onConfirm={() => void handleDeleteConfirm()}
          title={t.delete}
        />
      ) : null}
      {contextMenu && contextEntry ? (
        <ContextMenu
          autoFocus={false}
          items={[
            ...(contextProxy
              ? [
                  {
                    label: t.proxyManageAssignments,
                    action: () => openAssignments(contextProxy)
                  },
                  {
                    label: t.testProxy,
                    disabled: testStates[contextProxy.id]?.testing,
                    action: () => void handleTestProxy(contextProxy)
                  },
                  { separator: true }
                ]
              : []),
            { label: t.edit, action: () => openEntry(contextEntry) },
            {
              label: t.delete,
              danger: true,
              action: () => {
                setDeletingEntry(contextEntry)
                setDeleteError(null)
              }
            }
          ]}
          onClose={() => setContextMenu(null)}
          position={contextMenu.position}
        />
      ) : null}

      <ProxyEditDialog
        initialProxy={editingProxy}
        initialType={createProxyType}
        isOpen={isProxyModalOpen}
        isSubmitting={isSubmitting}
        onClose={() => {
          setIsProxyModalOpen(false)
          setEditingProxy(null)
        }}
        onSave={async (input: CreateProxyProfileInput | UpdateProxyProfileInput) => {
          setIsSubmitting(true)
          try {
            await saveProxy(input)
            setIsProxyModalOpen(false)
            setEditingProxy(null)
          } finally {
            setIsSubmitting(false)
          }
        }}
        onTest={testProxy}
      />
      <TunnelEditDialog
        initialTunnel={editingTunnel}
        isOpen={isTunnelModalOpen}
        isSubmitting={isSubmitting}
        onClose={() => {
          setIsTunnelModalOpen(false)
          setEditingTunnel(null)
        }}
        onSave={async (input: CreateTunnelProfileInput | UpdateTunnelProfileInput) => {
          setIsSubmitting(true)
          try {
            await saveTunnel(input)
            setIsTunnelModalOpen(false)
            setEditingTunnel(null)
          } finally {
            setIsSubmitting(false)
          }
        }}
      />
    </div>
  )
}

function FilterButton({
  active,
  count,
  icon,
  label,
  onClick
}: {
  active: boolean
  count: number
  icon: 'brand' | 'cloud' | 'connections' | 'server' | 'shield'
  label: string
  onClick(): void
}) {
  return (
    <button
      aria-pressed={active}
      className={`connection-manager-sidebar-item ${active ? 'active' : ''}`}
      type="button"
      onClick={onClick}
    >
      <span className="connection-manager-sidebar-icon">
        <AppIcon name={icon} size={14} />
      </span>
      <span className="connection-manager-sidebar-label">{label}</span>
      <span className="connection-manager-sidebar-count">{count}</span>
    </button>
  )
}

function ManagerRow({
  entry,
  testState,
  onEdit,
  onManageAssignments,
  onDelete,
  onContextMenu,
  onTest
}: {
  entry: ManagerEntry
  testState?: { testing: boolean; success?: boolean; latencyMs?: number }
  onEdit(): void
  onManageAssignments?(): void
  onDelete(): void
  onContextMenu(event: React.MouseEvent<HTMLDivElement>): void
  onTest(): void
}) {
  const isProxy = entry.kind === 'proxy'
  const name = entry.item.name
  let endpoint = '--'
  let portOrSetting = '--'
  let authConfigured = false
  let typeLabel = t.sshTunnelTypeShort
  let typeTitle = t.sshTunnelProfiles
  let typeClass = 'ssh'
  let boundNames: string[] = []
  if (entry.kind === 'proxy') {
    endpoint = `${entry.item.host}:${entry.item.port}`
    portOrSetting = String(entry.item.port)
    authConfigured = Boolean(entry.item.username || entry.item.hasPassword)
    typeLabel = entry.item.type === 'http' ? t.proxyTypeHttpConnectShort : t.proxyTypeSocks5Short
    typeTitle = entry.item.type === 'http' ? t.proxyProtocolHttpConnect : t.proxyProtocolSocks5
    typeClass = entry.item.type === 'http' ? 'http-connect' : 'socks5'
    boundNames = entry.item.boundConnectionNames ?? []
  } else {
    endpoint =
      entry.item.type === 'ssh'
        ? `${t.sshTunnelProfiles} · ${entry.item.forwards?.length ?? 0} ${t.tunnelForwardRules}`
        : (entry.item.scriptUrl ?? '--')
    portOrSetting = entry.item.type === 'http' ? `${entry.item.timeoutSeconds ?? 30}s` : '--'
    typeLabel = entry.item.type === 'http' ? t.httpTunnelTypeShort : t.sshTunnelTypeShort
    typeTitle = entry.item.type === 'http' ? t.httpTunnels : t.sshTunnelProfiles
    typeClass = entry.item.type === 'http' ? 'http-tunnel' : 'ssh'
    boundNames = entry.item.boundConnectionNames ?? []
  }

  return (
    <div
      className="manager-row"
      onDoubleClick={(event) => {
        if (!targetsNestedManagerControl(event)) onEdit()
      }}
      onKeyDown={(event) => {
        if (event.key !== 'Enter' || targetsNestedManagerControl(event)) return
        event.preventDefault()
        onEdit()
      }}
      onContextMenu={onContextMenu}
      role="button"
      tabIndex={0}
    >
      <span className="manager-name-cell">
        <span className="manager-node-icon">
          <AppIcon name={isProxy ? 'shield' : entry.item.type === 'ssh' ? 'connections' : 'cloud'} size={14} />
        </span>
        <span className="manager-node-name" title={name}>
          {name}
        </span>
      </span>
      <span className="proxy-manager-endpoint" title={endpoint}>
        {endpoint}
      </span>
      <span>{portOrSetting}</span>
      <span
        aria-label={authConfigured ? t.proxyAuthConfigured : t.proxyAuthNotConfigured}
        className={`proxy-auth-status ${authConfigured ? 'is-configured' : ''}`}
        title={authConfigured ? t.proxyAuthConfigured : t.proxyAuthNotConfigured}
      >
        {authConfigured ? (
          <>
            <AppIcon name="lock" size={12} />
            <span>{t.proxyAuthConfigured}</span>
          </>
        ) : (
          <span aria-hidden="true">--</span>
        )}
      </span>
      <span className={`manager-type-badge is-${typeClass}`} title={typeTitle}>
        {typeLabel}
      </span>
      {onManageAssignments ? (
        <button
          aria-label={`${t.proxyManageAssignments} · ${name}`}
          className="flat-button compact proxy-manager-bound-link"
          onClick={onManageAssignments}
          title={t.proxyManageAssignments}
          type="button"
        >
          {entry.item.usageCount ?? boundNames.length}
          <AppIcon name="connections" size={13} />
        </button>
      ) : (
        <span title={boundNames.join(', ')}>{boundNames.length || '--'}</span>
      )}
      <span className="manager-actions">
        {isProxy ? (
          <button
            aria-label={t.testProxy}
            className="manager-icon-action"
            disabled={testState?.testing}
            title={testState?.success === true ? `${testState.latencyMs ?? 0}ms` : t.testProxy}
            type="button"
            onClick={(event) => {
              event.stopPropagation()
              onTest()
            }}
          >
            {testState?.testing ? (
              <span className="button-spinner manager-action-spinner" />
            ) : testState?.success === true ? (
              <span className="proxy-row-success-dot">
                <AppIcon name="check" size={13} />
              </span>
            ) : (
              <AppIcon name="brand" size={13} />
            )}
          </button>
        ) : null}
        <button
          aria-label={t.edit}
          className="manager-icon-action"
          title={t.edit}
          type="button"
          onClick={(event) => {
            event.stopPropagation()
            onEdit()
          }}
        >
          <AppIcon name="edit" size={14} />
        </button>
        <button
          aria-label={t.delete}
          className="manager-icon-action danger"
          title={t.delete}
          type="button"
          onClick={(event) => {
            event.stopPropagation()
            onDelete()
          }}
        >
          <AppIcon name="trash" size={14} />
        </button>
      </span>
    </div>
  )
}

function deleteDescription(entry: ManagerEntry, error: string | null) {
  const boundCount = entry.item.boundConnectionNames?.length ?? 0
  const warning = entry.kind === 'proxy' ? t.deleteProxyWarning : t.deleteTunnelProfileWarning
  if (error) return `${error}\n\n${warning}`
  return boundCount > 0
    ? `确定要删除「${entry.item.name}」吗？当前正被 ${boundCount} 个连接使用。\n\n${warning}`
    : `确定要删除「${entry.item.name}」吗？\n\n${warning}`
}
