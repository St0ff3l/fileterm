import { useMemo, useRef, useState, type CSSProperties } from 'react'
import type { ConnectionFolder, ConnectionProfile, SessionSnapshot, WorkspaceTab } from '@fileterm/core'
import { MaterialIcon, type MaterialIconName } from '../../components/common/material-icon/material-icon'
import { SidebarAlignedLabel } from '../../components/common/icon-label/icon-label'
import { connectionProfileSummary } from '../../app/connection-profile-summary'
import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { StatusIndicator } from '../common/status-indicator'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { buildConnectionTree, type ConnectionTreeNode } from '../connections/connection-tree'
import type { ServerManagementPage as Page } from './server-management-types'
import './server-management-sidebar.css'

function serverIcon(profile: ConnectionProfile): MaterialIconName {
  return profile.type === 'serial' ? 'usb' : 'dns'
}

export function ServerManagementSidebar({
  profiles,
  folders,
  sessions,
  workspaceTabs,
  page,
  selectedFolderId,
  activeSessionTabId,
  collapsed,
  onNavigate,
  onActivateSession
}: {
  profiles: ConnectionProfile[]
  folders: ConnectionFolder[]
  sessions: Record<string, SessionSnapshot>
  workspaceTabs: WorkspaceTab[]
  page: Page
  selectedFolderId: string | null
  activeSessionTabId: string | null
  collapsed: boolean
  onNavigate(page: Page, folderId?: string | null): void
  onActivateSession(tabId: string): void
}) {
  const connectedTabs = useMemo(
    () =>
      workspaceTabs.filter(
        (tab) => tab.sessionType !== 'local' && tab.status === 'connected' && sessions[tab.id]?.connected
      ),
    [sessions, workspaceTabs]
  )
  const tree = useMemo(() => {
    const connectedProfileIds = new Set(connectedTabs.map((tab) => tab.profileId))
    const filterConnected = (nodes: ConnectionTreeNode[]): ConnectionTreeNode[] =>
      nodes.flatMap<ConnectionTreeNode>((node) => {
        if (node.type !== 'folder') return connectedProfileIds.has(node.id) ? [node] : []
        const children = filterConnected(node.children)
        return children.length ? [{ ...node, children }] : []
      })
    return filterConnected(buildConnectionTree(profiles, folders).roots)
  }, [connectedTabs, folders, profiles])
  const treeRef = useRef<HTMLUListElement>(null)
  const tabsByProfile = useMemo(() => {
    const grouped = new Map<string, WorkspaceTab[]>()
    connectedTabs.forEach((tab) => grouped.set(tab.profileId, [...(grouped.get(tab.profileId) ?? []), tab]))
    return grouped
  }, [connectedTabs])
  const [collapsedFolders, setCollapsedFolders] = useState<Set<string>>(
    () => new Set(folders.filter((folder) => folder.isExpanded === false).map((folder) => folder.id))
  )
  const selectProfile = (profileId: string) => {
    const openTab = tabsByProfile.get(profileId)?.[0]
    if (openTab) onActivateSession(openTab.id)
  }

  const renderNode = (node: ConnectionTreeNode, depth = 0) => {
    if (node.type === 'folder') {
      const expanded = !collapsedFolders.has(node.id)
      const profileCount = countProfiles(node)
      return (
        <li className="management-tree-folder" key={node.id}>
          <div className="management-tree-row" style={{ '--tree-depth': depth } as CSSProperties}>
            <button
              aria-expanded={expanded}
              aria-label={`${expanded ? t.collapse : t.expand} ${node.name}`}
              className="management-tree-expand"
              onClick={() =>
                setCollapsedFolders((current) => {
                  const next = new Set(current)
                  if (next.has(node.id)) next.delete(node.id)
                  else next.add(node.id)
                  return next
                })
              }
              type="button"
            >
              <MaterialIcon name="chevron_right" />
            </button>
            <button
              aria-current={selectedFolderId === node.id ? 'page' : undefined}
              className={`management-tree-item ${selectedFolderId === node.id ? 'is-selected' : ''}`}
              onClick={() => onNavigate('overview', node.id)}
              title={node.name}
              type="button"
            >
              <MaterialIcon name="folder" />
              {!collapsed ? <span className="management-tree-label">{node.name}</span> : null}
              {!collapsed ? <span className="management-tree-count">{profileCount}</span> : null}
            </button>
          </div>
          {expanded ? <ul>{node.children.map((child) => renderNode(child, depth + 1))}</ul> : null}
        </li>
      )
    }

    const profileTabs = tabsByProfile.get(node.id) ?? []
    const openTab = profileTabs.find((tab) => sessions[tab.id]?.connected)
    const tabCountLabel = profileTabs.length ? `${profileTabs.length} ${t.hostRailTabCount}` : null
    const accessibleLabel = [node.name, connectionProfileSummary(node), tabCountLabel].filter(Boolean).join(' · ')
    return (
      <li className="management-tree-profile" key={node.id}>
        <button
          aria-label={accessibleLabel}
          aria-current={openTab?.id === activeSessionTabId ? 'true' : undefined}
          className={`management-tree-item ${openTab?.id === activeSessionTabId ? 'is-current' : ''}`}
          onClick={() => selectProfile(node.id)}
          style={{ '--tree-depth': depth } as CSSProperties}
          title={accessibleLabel}
          type="button"
        >
          <MaterialIcon name={serverIcon(node)} />
          {!collapsed ? (
            <span className="management-tree-profile-copy">
              <strong>{node.name}</strong>
              <small>{connectionProfileSummary(node)}</small>
            </span>
          ) : null}
          {!collapsed && profileTabs.length ? (
            <span aria-hidden="true" className="management-tree-profile-count">
              {profileTabs.length}
            </span>
          ) : null}
          {!collapsed && openTab ? <StatusIndicator aria-label={t.hostStatusConnected} status="connected" /> : null}
        </button>
      </li>
    )
  }

  const navItems: Array<{ page: Page; label: string; icon: MaterialIconName | 'history' | 'log' }> = [
    { page: 'overview', label: t.overview, icon: 'dashboard' },
    { page: 'connection-manager', label: t.connectionManager, icon: 'settings_ethernet' },
    { page: 'proxy-manager', label: t.proxyManager, icon: 'shield' },
    { page: 'command-manager', label: t.commandManager, icon: 'terminal' },
    { page: 'ssh-key-manager', label: t.sshKeyManager, icon: 'key' },
    { page: 'background-sessions', label: t.backgroundSessions, icon: 'history' },
    { page: 'mcp-call-log', label: t.mcpCallRecordsTitle, icon: 'log' },
    { page: 'settings', label: t.settings, icon: 'settings' }
  ]
  // A session tab in front means no management page is visible, so the nav
  // should not keep claiming one of its entries is current.
  const navActive = !activeSessionTabId

  return (
    <div className="server-management-sidebar-content">
      <nav aria-label={t.serverManagementNavigation} className="server-management-nav">
        {navItems.map(({ page: itemPage, label, icon }) => (
          <button
            aria-current={navActive && page === itemPage ? 'page' : undefined}
            className={`sidebar-nav-link ${navActive && page === itemPage ? 'active' : ''}`}
            key={itemPage}
            onClick={() => onNavigate(itemPage, itemPage === 'overview' ? null : selectedFolderId)}
            title={label}
            type="button"
          >
            {icon === 'history' || icon === 'log' ? <AppIcon name={icon} size={18} /> : <MaterialIcon name={icon} />}
            {!collapsed ? <SidebarAlignedLabel>{label}</SidebarAlignedLabel> : null}
          </button>
        ))}
      </nav>

      <section aria-label={t.serverConnections} className="server-management-connections">
        {!collapsed ? <h3>{t.serverConnections}</h3> : null}
        {tree.length ? (
          <div className="server-management-tree-container">
            <ul className="server-management-tree" ref={treeRef}>
              {tree.map((node) => renderNode(node))}
            </ul>
            <VerticalScrollbar ariaLabel={t.serverConnections} scrollRef={treeRef} />
          </div>
        ) : !collapsed ? (
          <p className="server-management-empty">{t.noActiveServerConnections}</p>
        ) : null}
      </section>
    </div>
  )
}

function countProfiles(node: ConnectionTreeNode): number {
  return node.type === 'folder' ? node.children.reduce((count, child) => count + countProfiles(child), 0) : 1
}
