import { useMonitoringDiagnostics } from './use-monitoring-diagnostics'
import './system-sidebar-controls.css'
import { MonitoringToggle } from './monitoring-toggle'
import { MonitoringOverlay, monitoringIsObscured } from './monitoring-overlay'
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import type {
  ConnectionProfile,
  ResourceMonitoringMetric,
  SessionSnapshot,
  TabStatus,
  WorkspaceSnapshot
} from '@fileterm/core'
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
  activeTabId,
  collapsed,
  connectionStatus,
  showResourceMeters,
  visibleMetrics,
  onMonitoringSnapshot,
  onOpenSystemInfo,
  onToggleCollapsed
}: {
  activeProfile: ConnectionProfile | null
  activeSession: SessionSnapshot | null
  activeTabId: string | null
  collapsed: boolean
  connectionStatus: TabStatus | null
  showResourceMeters: boolean
  visibleMetrics: ResourceMonitoringMetric[]
  onMonitoringSnapshot(snapshot: WorkspaceSnapshot): void
  onOpenSystemInfo(): void
  onToggleCollapsed(): void
}) {
  const logMonitoringAction = useMonitoringDiagnostics({
    tabId: activeTabId,
    session: activeSession,
    collapsed,
    connectionStatus
  })
  const [sortMode, setSortMode] = useState<'memory' | 'cpu' | 'command'>('cpu')
  const obscured = monitoringIsObscured(activeSession)
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

  useLayoutEffect(() => {
    // Reconnects and kept-alive tabs reuse the sidebar DOM node. Reset both
    // before paint and on the next frame so late metric hydration cannot leave
    // the resource viewport anchored to the previous session's bottom edge.
    const resetScroll = () => {
      if (systemMetricsScrollRef.current) {
        systemMetricsScrollRef.current.scrollTop = 0
      }
    }

    resetScroll()
    const frameId = window.requestAnimationFrame(resetScroll)
    return () => window.cancelAnimationFrame(frameId)
  }, [activeSession?.connected, activeSession?.profileId, activeTabId, connectionStatus, Boolean(metrics)])

  const selectedFileSystem =
    availableFileSystems.find((row) => row.mountPoint === selectedDiskMountPoint) ?? defaultFileSystem
  const sortedProcesses = useMemo(() => {
    const procs = [...(metrics?.topProcesses ?? [])]
    if (sortMode === 'command') {
      // 使用采集端的命令排序，避免浏览器 locale 改变全局前 40 的候选范围。
      return procs
        .sort((a, b) =>
          a.commandOrder !== undefined && b.commandOrder !== undefined
            ? a.commandOrder - b.commandOrder
            : a.command.localeCompare(b.command) || parseFloat(b.cpu) - parseFloat(a.cpu)
        )
        .slice(0, 40)
    }
    return procs
      .sort((a, b) => {
        if (sortMode === 'cpu') {
          return parseFloat(b.cpu) - parseFloat(a.cpu)
        }
        if (sortMode === 'memory') {
          return (b.memoryBytes ?? parseMemory(b.memory) * 1024) - (a.memoryBytes ?? parseMemory(a.memory) * 1024)
        }
        return 0
      })
      .slice(0, 40)
  }, [metrics?.topProcesses, sortMode])

  return (
    <div className={`system-sidebar-layout ${collapsed ? 'is-collapsed' : ''}`}>
      <div className="system-sidebar-controls">
        <button
          aria-label={collapsed ? t.showSystemSidebar : t.hideSystemSidebar}
          className={`system-sidebar-toggle ${collapsed ? 'is-collapsed' : ''}`}
          onClick={() => {
            logMonitoringAction('toggle-sidebar')
            onToggleCollapsed()
          }}
          title={collapsed ? t.showSystemSidebar : t.hideSystemSidebar}
          type="button"
        >
          <span className="system-sidebar-toggle-icon" aria-hidden="true" />
        </button>
        <MonitoringToggle
          key={`toggle:${activeTabId}:${activeSession?.monitoring?.generation}`}
          onSnapshot={onMonitoringSnapshot}
          session={activeSession}
          tabId={activeTabId}
        />
      </div>
      <MonitoringOverlay
        key={`overlay:${activeTabId}:${activeSession?.monitoring?.generation}`}
        session={activeSession}
        tabId={activeTabId}
        collapsed={collapsed}
        connectionStatus={connectionStatus}
        onSnapshot={onMonitoringSnapshot}
      />
      {!collapsed ? (
        <>
          <section className="sys-card" inert={obscured}>
            <div className="connection-summary">
              <AddressLine label={t.privateIp} value={internalIp} />
              <AddressLine label={t.accessAddress} value={accessAddress} />
            </div>
            <button
              className="system-title"
              data-file-panel-snap-target="system-title"
              onClick={() => {
                logMonitoringAction('open-system-info')
                onOpenSystemInfo()
              }}
              type="button"
            >
              {t.systemInfo}
            </button>
            {showResourceMeters ? (
              <ResourceMetricCards
                availableFileSystems={availableFileSystems}
                fileSystem={selectedFileSystem}
                metrics={metrics}
                onFileSystemChange={setSelectedDiskMountPoint}
                scrollRef={systemMetricsScrollRef}
                visibleMetrics={visibleMetrics}
              />
            ) : null}
            {showResourceMeters && visibleMetrics.includes('processes') ? (
              <ProcessMetricPanel onSortModeChange={setSortMode} rows={sortedProcesses} sortMode={sortMode} />
            ) : null}
            {showResourceMeters && visibleMetrics.includes('network') ? <NetworkMetricPanel metrics={metrics} /> : null}
          </section>
          {showResourceMeters ? (
            <section className="disk-table" inert={obscured}>
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
          ) : null}
        </>
      ) : showResourceMeters ? (
        <CollapsedResourceMeters fileSystem={selectedFileSystem} metrics={metrics} visibleMetrics={visibleMetrics} />
      ) : null}
    </div>
  )
}
