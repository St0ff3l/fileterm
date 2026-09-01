import type { ConnectionProfile, ResourceMonitoringMetric, SessionSnapshot } from '@fileterm/core'
import { t } from '../../i18n'
import { SystemSidebar } from './system-sidebar'

export function SystemSidebarShell({
  activeProfile,
  activeSession,
  collapsed,
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
  collapsed: boolean
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
        collapsed={collapsed}
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
