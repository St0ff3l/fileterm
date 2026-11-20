import type {
  ConnectionProfile,
  ResourceMonitoringMetric,
  SessionSnapshot,
  TabStatus,
  WorkspaceSnapshot
} from '@fileterm/core'
import { t } from '../../i18n'
import { SystemSidebar } from './system-sidebar'
import type { MonitoringTarget } from './shared-monitoring'

export function SystemSidebarShell({
  activeProfile,
  activeSession,
  activeTabId,
  monitoringPeers,
  collapsed,
  connectionStatus,
  showResourceMeters,
  visibleMetrics,
  isResizing,
  toggleSide,
  onMonitoringSnapshot,
  onOpenSystemInfo,
  onOpenNetworkConnections,
  onResizeStart,
  onRestoreWidth,
  onToggleCollapsed
}: {
  activeProfile: ConnectionProfile | null
  activeSession: SessionSnapshot | null
  activeTabId: string | null
  monitoringPeers?: MonitoringTarget[]
  collapsed: boolean
  connectionStatus: TabStatus | null
  showResourceMeters: boolean
  visibleMetrics: ResourceMonitoringMetric[]
  isResizing: boolean
  /** Management mode mirrors the collapse glyph; the default layout keeps it. */
  toggleSide?: 'left' | 'right'
  onMonitoringSnapshot(snapshot: WorkspaceSnapshot): void
  onOpenSystemInfo(): void
  onOpenNetworkConnections(): void
  onResizeStart(): void
  onRestoreWidth(): void
  onToggleCollapsed(nextCollapsed: boolean): void
}) {
  return (
    <aside className={`fs-sidebar ${collapsed ? 'is-collapsed' : ''}`} style={{ position: 'relative' }}>
      <SystemSidebar
        activeProfile={activeProfile}
        activeSession={activeSession}
        activeTabId={activeTabId}
        monitoringPeers={monitoringPeers}
        collapsed={collapsed}
        connectionStatus={connectionStatus}
        showResourceMeters={showResourceMeters}
        visibleMetrics={visibleMetrics}
        onMonitoringSnapshot={onMonitoringSnapshot}
        onOpenSystemInfo={onOpenSystemInfo}
        onOpenNetworkConnections={onOpenNetworkConnections}
        toggleSide={toggleSide}
        onToggleCollapsed={() => {
          const nextCollapsed = !collapsed
          if (!nextCollapsed) {
            onRestoreWidth()
          }
          onToggleCollapsed(nextCollapsed)
        }}
      />
      {!collapsed ? (
        <div
          aria-label={t.resizeSidebar}
          className={`sidebar-resizer ${isResizing ? 'is-active' : ''}`}
          onMouseDown={(event) => {
            event.preventDefault()
            event.stopPropagation()
            onResizeStart()
          }}
          role="separator"
        />
      ) : null}
    </aside>
  )
}
