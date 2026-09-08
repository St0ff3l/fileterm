import { useEffect, useMemo, useState } from 'react'
import type {
  CreateProxyProfileInput,
  CreateTunnelProfileInput,
  ProxyProfile,
  TunnelProfile,
  TunnelProfileType,
  UpdateProxyProfileInput,
  UpdateTunnelProfileInput
} from '@fileterm/core'
import { t } from '../../i18n'
import { useProxyLibrary } from '../../hooks/use-proxy-library'
import { useTunnelLibrary } from '../../hooks/use-tunnel-library'
import { AppIcon } from '../common/app-icon'
import { ConfirmActionDialog } from '../common/confirm-action-dialog'
import { ProxyEditDialog } from './proxy-edit-dialog'
import { TunnelEditDialog } from '../tunnels/tunnel-edit-dialog'

type ActiveFilter = 'all' | 'socks5' | 'http-connect' | 'ssh' | 'http-tunnel'

type ManagerEntry = { kind: 'proxy'; item: ProxyProfile } | { kind: 'tunnel'; item: TunnelProfile }

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
  onStatsChange,
  onActiveFilterChange
}: {
  onStatsChange?(stats: ProxyManagerStats): void
  onActiveFilterChange?(label: string): void
} = {}) {
  const proxyLibrary = useProxyLibrary()
  const tunnelLibrary = useTunnelLibrary()
  const { proxies, error: proxyError, clearError: clearProxyError, saveProxy, deleteProxy, testProxy } = proxyLibrary
  const { tunnels, error: tunnelError, clearError: clearTunnelError, saveTunnel, deleteTunnel } = tunnelLibrary
  const [activeFilter, setActiveFilter] = useState<ActiveFilter>('all')
  const [searchQuery, setSearchQuery] = useState('')
  const [isActionsExpanded, setIsActionsExpanded] = useState(false)
  const [isProxyModalOpen, setIsProxyModalOpen] = useState(false)
  const [isTunnelModalOpen, setIsTunnelModalOpen] = useState(false)
  const [editingProxy, setEditingProxy] = useState<ProxyProfile | null>(null)
  const [editingTunnel, setEditingTunnel] = useState<TunnelProfile | null>(null)
  const [createProxyType, setCreateProxyType] = useState<'socks5' | 'http'>('socks5')
  const [createTunnelType, setCreateTunnelType] = useState<TunnelProfileType>('ssh')
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [deletingEntry, setDeletingEntry] = useState<ManagerEntry | null>(null)
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
    const matchesFilter = (entry: ManagerEntry) => {
      if (activeFilter === 'all') return true
      if (entry.kind === 'proxy') {
        return (
          (activeFilter === 'socks5' && entry.item.type === 'socks5') ||
          (activeFilter === 'http-connect' && entry.item.type === 'http')
        )
      }
      return (
        (activeFilter === 'ssh' && entry.item.type === 'ssh') ||
        (activeFilter === 'http-tunnel' && entry.item.type === 'http')
      )
    }

    return [
      ...proxies.map((item) => ({ kind: 'proxy' as const, item })),
      ...tunnels.map((item) => ({ kind: 'tunnel' as const, item }))
    ]
      .filter((entry) => matchesFilter(entry) && matchesQuery(entry))
      .sort(
        (left, right) =>
          (right.item.updatedAt ?? right.item.createdAt ?? 0) - (left.item.updatedAt ?? left.item.createdAt ?? 0)
      )
  }, [activeFilter, proxies, searchQuery, tunnels])

  const activeFilterLabel =
    activeFilter === 'socks5'
      ? t.socks5Proxies
      : activeFilter === 'http-connect'
        ? t.httpConnectProxies
        : activeFilter === 'ssh'
          ? t.sshTunnelProfiles
          : activeFilter === 'http-tunnel'
            ? t.httpTunnels
            : t.allProxiesAndTunnels

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

  const openTunnelCreate = (type: TunnelProfileType) => {
    if (window.fileterm?.openTunnelFormWindow) {
      void window.fileterm.openTunnelFormWindow('create', undefined, type)
      return
    }
    setEditingTunnel(null)
    setCreateTunnelType(type)
    setIsTunnelModalOpen(true)
  }

  const openEntry = (entry: ManagerEntry) => {
    if (entry.kind === 'proxy') {
      if (window.fileterm?.openProxyFormWindow) {
        void window.fileterm.openProxyFormWindow('edit', entry.item.id)
        return
      }
      setEditingProxy(entry.item)
      setIsProxyModalOpen(true)
      return
    }
    if (window.fileterm?.openTunnelFormWindow) {
      void window.fileterm.openTunnelFormWindow('edit', entry.item.id)
      return
    }
    setEditingTunnel(entry.item)
    setIsTunnelModalOpen(true)
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

  const libraryError = proxyError || tunnelError

  return (
    <div className="modal-card manager-modal connection-manager-modal proxy-manager-modal manager-inline">
      <div className="connection-manager-header">
        <span className="connection-manager-title">
          <AppIcon className="proxy-manager-title-icon" name="shield" size={26} />
          <span>{t.proxyManager}</span>
        </span>
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

      <div className="connection-manager-layout">
        <aside className="connection-manager-sidebar" aria-label="Proxy and tunnel filters">
          <FilterButton
            active={activeFilter === 'all'}
            count={stats.totalCount}
            icon="shield"
            label={t.allProxiesAndTunnels}
            onClick={() => setActiveFilter('all')}
          />
          <FilterButton
            active={activeFilter === 'socks5'}
            count={stats.socks5Count}
            icon="brand"
            label={t.socks5Proxies}
            onClick={() => setActiveFilter('socks5')}
          />
          <FilterButton
            active={activeFilter === 'http-connect'}
            count={stats.httpProxyCount}
            icon="server"
            label={t.httpConnectProxies}
            onClick={() => setActiveFilter('http-connect')}
          />
          <FilterButton
            active={activeFilter === 'ssh'}
            count={stats.sshTunnelCount}
            icon="connections"
            label={t.sshTunnelProfiles}
            onClick={() => setActiveFilter('ssh')}
          />
          <FilterButton
            active={activeFilter === 'http-tunnel'}
            count={stats.httpTunnelCount}
            icon="cloud"
            label={t.httpTunnels}
            onClick={() => setActiveFilter('http-tunnel')}
          />
        </aside>

        <section className="connection-manager-content">
          <div className="manager-table connection-manager-table proxy-manager-table">
            <div className="manager-head">
              <span>{t.name}</span>
              <span>{t.host}</span>
              <span>{t.port}</span>
              <span>{t.proxyUserLabel}</span>
              <span>{t.proxyType}</span>
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
                  onDelete={() => {
                    setDeletingEntry(entry)
                    setDeleteError(null)
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
                  openTunnelCreate('http')
                  setIsActionsExpanded(false)
                }}
              >
                <AppIcon name="plus" size={13} />
                <span>{t.addHttpTunnel}</span>
              </button>
              <button
                className="drawer-option-btn secondary-btn"
                type="button"
                onClick={() => {
                  openTunnelCreate('ssh')
                  setIsActionsExpanded(false)
                }}
              >
                <AppIcon name="plus" size={13} />
                <span>{t.addSshTunnel}</span>
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
        initialType={createTunnelType}
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
    <button className={`connection-manager-sidebar-item ${active ? 'active' : ''}`} type="button" onClick={onClick}>
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
  onDelete,
  onTest
}: {
  entry: ManagerEntry
  testState?: { testing: boolean; success?: boolean; latencyMs?: number }
  onEdit(): void
  onDelete(): void
  onTest(): void
}) {
  const isProxy = entry.kind === 'proxy'
  const name = entry.item.name
  let endpoint = '--'
  let portOrSetting = '--'
  let identity = '--'
  let typeLabel = 'SSH TUNNEL'
  let typeClass = 'ssh'
  let boundNames: string[] = []
  if (entry.kind === 'proxy') {
    endpoint = `${entry.item.host}:${entry.item.port}`
    portOrSetting = String(entry.item.port)
    identity = entry.item.username || '--'
    typeLabel = entry.item.type === 'http' ? 'HTTP CONNECT' : 'SOCKS5'
    typeClass = entry.item.type === 'http' ? 'http-connect' : 'socks5'
    boundNames = entry.item.boundConnectionNames ?? []
  } else {
    endpoint =
      entry.item.type === 'ssh'
        ? `${t.sshTunnelProfiles} · ${entry.item.forwards?.length ?? 0} ${t.tunnelForwardRules}`
        : (entry.item.scriptUrl ?? '--')
    portOrSetting = entry.item.type === 'http' ? `${entry.item.timeoutSeconds ?? 30}s` : '--'
    identity = entry.item.type === 'http' && entry.item.hasToken ? t.httpTunnelToken : '--'
    typeLabel = entry.item.type === 'http' ? 'HTTP TUNNEL' : 'SSH TUNNEL'
    typeClass = entry.item.type === 'http' ? 'http-tunnel' : 'ssh'
    boundNames = entry.item.boundConnectionNames ?? []
  }

  return (
    <div className="manager-row" onClick={onEdit}>
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
      <span>{identity}</span>
      <span className={`manager-type-badge is-${typeClass}`}>{typeLabel}</span>
      <span title={boundNames.join(', ')}>{boundNames.length > 0 ? `${boundNames.length} 个连接` : '--'}</span>
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
