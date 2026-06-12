import type { ConnectionProfile, ConnectionFolder } from '@termdock/core'
import { useState, useMemo } from 'react'
import { t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'

type ConnectionTreeNode =
  | (ConnectionFolder & { children: ConnectionTreeNode[] })
  | (ConnectionProfile & { children?: never })

export function HomeWorkspace({
  profiles,
  folders = [],
  onOpen
}: {
  profiles: ConnectionProfile[]
  folders?: ConnectionFolder[]
  onOpen(profileId: string): void
}) {
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set())

  const toggleFolder = (folderId: string, event?: React.MouseEvent) => {
    event?.stopPropagation()
    setExpandedFolders(prev => {
      const next = new Set(prev)
      if (next.has(folderId)) next.delete(folderId)
      else next.add(folderId)
      return next
    })
  }

  const tree = useMemo(() => {
    const items: ConnectionTreeNode[] = [
      ...profiles.map((profile, index) => ({
        ...profile,
        order: typeof profile.order === 'number' ? profile.order : index * 1000
      })),
      ...folders.map((folder, index) => ({
        ...folder,
        order: typeof folder.order === 'number' ? folder.order : (profiles.length + index) * 1000,
        children: []
      }))
    ]

    const roots: ConnectionTreeNode[] = []
    const map = new Map<string, ConnectionTreeNode>()

    items.forEach(item => {
      map.set(item.id, item)
    })

    items.forEach(item => {
      const parent = item.parentId ? map.get(item.parentId) : undefined
      if (parent?.type === 'folder') {
        parent.children.push(item)
      } else {
        roots.push(item)
      }
    })

    const sortNodes = (nodes: ConnectionTreeNode[]) => {
      nodes.sort((a, b) => (a.order ?? 0) - (b.order ?? 0))
      nodes.forEach(n => {
        if (n.type === 'folder') sortNodes(n.children)
      })
    }
    sortNodes(roots)
    return roots
  }, [profiles, folders])

  const renderNode = (node: ConnectionTreeNode, depth: number) => {
    const isFolder = node.type === 'folder'
    const isExpanded = expandedFolders.has(node.id)

    return (
      <div key={node.id}>
        <div
          className={`quick-row ${isFolder ? 'folder-row' : ''}`}
          onClick={() => isFolder ? toggleFolder(node.id) : onOpen(node.id)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' || event.key === ' ') {
              isFolder ? toggleFolder(node.id) : onOpen(node.id)
            }
          }}
          role="button"
          tabIndex={0}
        >
          <span className="host-icon" style={{ marginLeft: `${depth * 20}px` }}>
            {isFolder ? (
              <span style={{ transform: isExpanded ? 'rotate(90deg)' : 'none', transition: 'transform 0.1s', display: 'inline-block', fontSize: '10px' }}>▶</span>
            ) : (
              <AppIcon name="server" />
            )}
          </span>
          <strong style={{ opacity: isFolder ? 0.9 : 1 }}>{node.name}</strong>
          <span>{isFolder ? '--' : (node.note || '/')}</span>
          <span>{isFolder ? '--' : node.username}</span>
          <small>{isFolder ? t.homeFolderType : node.type.toUpperCase()}</small>
        </div>
        {isFolder && isExpanded && node.children && (
          <div className="folder-children">
            {node.children.map((child) => renderNode(child, depth + 1))}
            {node.children.length === 0 && (
              <div className="quick-row empty-folder" style={{ color: '#666' }}>
                <span style={{ marginLeft: `${(depth + 1) * 20 + 20}px`, gridColumn: '1 / -1' }}>{t.emptyFolder}</span>
              </div>
            )}
          </div>
        )}
      </div>
    )
  }

  return (
    <section className="home-workspace">
      <div className="quick-panel">
        <div className="quick-header">
          <strong>{t.quickConnect}</strong>
        </div>
        <div className="quick-list">
          {tree.map(node => renderNode(node, 0))}
          {tree.length === 0 && (
             <div style={{ padding: '20px', textAlign: 'center', color: '#666' }}>{t.noConnections}</div>
          )}
        </div>
      </div>
    </section>
  )
}
