import type { ConnectionProfile, ResourceMonitoringMetric, SessionSnapshot, TabStatus } from '@fileterm/core'
import { t } from '../../i18n'
import { SystemSidebar } from './system-sidebar'

export function SystemSidebarShell({
  activeProfile,
  activeSession,
  activeTabId,
  collapsed,
  connectionStatus,
  showResourceMeters,
  visibleMetrics,
  isResizing,
  onOpenSystemInfo,
  onResizeStart,
  onRestoreWidth,
  onToggleCollapsed
}: {
  activeProfile: ConnectionProfile | null
  activeSession: SessionSnapshot | null
  activeTabId: string | null
  collapsed: boolean
  connectionStatus: TabStatus | null
  showResourceMeters: boolean
  visibleMetrics: ResourceMonitoringMetric[]
  isResizing: boolean
  onOpenSystemInfo(): void
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
        collapsed={collapsed}
        connectionStatus={connectionStatus}
        showResourceMeters={showResourceMeters}
        visibleMetrics={visibleMetrics}
        onOpenSystemInfo={onOpenSystemInfo}
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
