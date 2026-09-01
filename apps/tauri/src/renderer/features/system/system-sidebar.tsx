import { useEffect, useMemo, useRef, useState } from 'react'
import type { ConnectionProfile, ResourceMonitoringMetric, SessionSnapshot } from '@fileterm/core'
import { t } from '../../i18n'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import {
  AddressLine,
  CollapsedResourceMeters,
  isEphemeralFileSystem,
  parseMemory,
  ResourceMetricCards,
  selectPrimaryFileSystem
} from './system-resource-meters'
import { NetworkMetricPanel, ProcessMetricPanel } from './system-resource-details'

export function SystemSidebar({
  activeProfile,
  activeSession,
  collapsed,
  showResourceMeters,
  visibleMetrics,
  onOpenSystemInfo,
  onToggleCollapsed
}: {
  activeProfile: ConnectionProfile | null
  activeSession: SessionSnapshot | null
  collapsed: boolean
  showResourceMeters: boolean
  visibleMetrics: ResourceMonitoringMetric[]
  onOpenSystemInfo(): void
  onToggleCollapsed(): void
}) {
  const [sortMode, setSortMode] = useState<'memory' | 'cpu' | 'command'>('cpu')
  const metrics = activeSession?.systemMetrics
  const internalIp = metrics?.ip || '-'
  const accessAddress = activeProfile?.host || activeSession?.accessHost || '-'
  const availableFileSystems = useMemo(
    () => (metrics?.fileSystemRows ?? []).filter((row) => row.mountPoint === '/' || !isEphemeralFileSystem(row)),
    [metrics?.fileSystemRows]
  )
  const rows = useMemo(() => {
    // Prefer the normalized filesystem payload that also drives the disk
    // meter. The legacy `diskRows` block can be absent in a partial stream,
    // which previously left this table with only its empty placeholders even
    // while the meter above already displayed the mounted root filesystem.
    // Unlike the meter selector, the table keeps every reported mount
    // (including tmpfs/devtmpfs rows) to match the historical layout.
    const normalizedRows = (metrics?.fileSystemRows ?? [])
      .map((row) => ({
        path: row.mountPoint || row.name,
        usage: `${row.available}/${row.size}`
      }))
      .filter((row) => row.path && row.usage !== '/')

    if (normalizedRows.length > 0) {
      return normalizedRows
    }

    // Preserve compatibility with older/partial collectors that only provide
    // the compact block.
    return (metrics?.diskRows ?? []).filter((row) => Boolean(row?.path && row?.usage && row.usage !== '/'))
  }, [metrics?.fileSystemRows, metrics?.diskRows])
  const defaultFileSystem = useMemo(() => selectPrimaryFileSystem(availableFileSystems), [availableFileSystems])
  const [selectedDiskMountPoint, setSelectedDiskMountPoint] = useState('')
  const diskScrollRef = useRef<HTMLDivElement>(null)
  const systemMetricsScrollRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    setSelectedDiskMountPoint(defaultFileSystem?.mountPoint ?? '')
  }, [activeSession?.profileId, defaultFileSystem?.mountPoint])

  useEffect(() => {
    if (availableFileSystems.some((row) => row.mountPoint === selectedDiskMountPoint)) {
      return
    }
    setSelectedDiskMountPoint(defaultFileSystem?.mountPoint ?? '')
  }, [availableFileSystems, defaultFileSystem?.mountPoint, selectedDiskMountPoint])

  useEffect(() => {
    // Reconnects reuse the sidebar DOM node, so explicitly start the resource
    // viewport at the top when the remote session changes connection state.
    if (systemMetricsScrollRef.current) {
      systemMetricsScrollRef.current.scrollTop = 0
    }
  }, [activeSession?.connected, activeSession?.profileId])

  const selectedFileSystem =
    availableFileSystems.find((row) => row.mountPoint === selectedDiskMountPoint) ?? defaultFileSystem
  const sortedProcesses = useMemo(() => {
    const procs = [...(metrics?.topProcesses ?? [])]
    if (sortMode === 'command') {
      // 按命令名字典序，便于找同名进程；同命令按 CPU 降序
      return procs
        .sort((a, b) => a.command.localeCompare(b.command) || parseFloat(b.cpu) - parseFloat(a.cpu))
        .slice(0, 40)
    }
    return procs
      .sort((a, b) => {
        if (sortMode === 'cpu') {
          return parseFloat(b.cpu) - parseFloat(a.cpu)
        }
        if (sortMode === 'memory') {
          return parseMemory(b.memory) - parseMemory(a.memory)
        }
        return 0
      })
      .slice(0, 40)
  }, [metrics?.topProcesses, sortMode])

  return (
    <div className={`system-sidebar-layout ${collapsed ? 'is-collapsed' : ''}`}>
      <button
        aria-label={collapsed ? t.showSystemSidebar : t.hideSystemSidebar}
        className={`system-sidebar-toggle ${collapsed ? 'is-collapsed' : ''}`}
        onClick={onToggleCollapsed}
        title={collapsed ? t.showSystemSidebar : t.hideSystemSidebar}
        type="button"
      >
        <svg
          className="system-sidebar-toggle-icon"
          width="14"
          height="14"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <rect x="1.5" y="1.5" width="13" height="13" rx="2.5" />
          <path d="M5.25 1.5V14.5" />
        </svg>
      </button>
      {!collapsed ? (
        <>
          <section className="sys-card">
            <div className="connection-summary">
              <AddressLine label={t.privateIp} value={internalIp} />
              <AddressLine label={t.accessAddress} value={accessAddress} />
            </div>
            <button
              className="system-title"
              data-file-panel-snap-target="system-title"
              onClick={onOpenSystemInfo}
              type="button"
            >
              {t.systemInfo}
            </button>
            <ResourceMetricCards
              availableFileSystems={availableFileSystems}
              fileSystem={selectedFileSystem}
              metrics={metrics}
              onFileSystemChange={setSelectedDiskMountPoint}
              scrollRef={systemMetricsScrollRef}
              visibleMetrics={visibleMetrics}
            />
            {visibleMetrics.includes('processes') ? (
              <ProcessMetricPanel onSortModeChange={setSortMode} rows={sortedProcesses} sortMode={sortMode} />
            ) : null}
            {visibleMetrics.includes('network') ? <NetworkMetricPanel metrics={metrics} /> : null}
          </section>
          <section className="disk-table">
            <div className="disk-head" data-file-panel-snap-target="disk-header">
              <span>{t.path}</span>
              <span>{t.availableSize}</span>
            </div>
            <div className="disk-scroll-region">
              <div className="disk-body" ref={diskScrollRef}>
                {rows.length
                  ? rows.map((row) => (
                      <div className="disk-row" key={row.path}>
                        <span>{row.path}</span>
                        <span>{row.usage}</span>
                      </div>
                    ))
                  : Array.from({ length: 8 }).map((_, i) => (
                      <div className="disk-row" key={`empty-${i}`}>
                        <span></span>
                        <span></span>
                      </div>
                    ))}
              </div>
              <VerticalScrollbar ariaLabel={t.scrollDiskList} scrollRef={diskScrollRef} />
            </div>
          </section>
        </>
      ) : showResourceMeters ? (
        <CollapsedResourceMeters fileSystem={selectedFileSystem} metrics={metrics} visibleMetrics={visibleMetrics} />
      ) : null}
    </div>
  )
}
