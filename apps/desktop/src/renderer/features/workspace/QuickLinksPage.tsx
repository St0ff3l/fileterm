import type { ConnectionProfile, ConnectionFolder } from '@fileterm/core'
import { useState, useMemo } from 'react'
import { t } from '../../i18n'

type ConnectionTreeNode =
  | (ConnectionFolder & { children: ConnectionTreeNode[] })
  | (ConnectionProfile & { children?: never })

export function QuickLinksPage({
  profiles,
  folders = [],
  onOpen,
  onOpenNewConnection
}: {
  profiles: ConnectionProfile[]
  folders?: ConnectionFolder[]
  onOpen(profileId: string): void
  onOpenNewConnection(): void
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

    const handleRowClick = (e: React.MouseEvent) => {
      if (isFolder) {
        toggleFolder(node.id, e)
      } else {
        onOpen(node.id)
      }
    }

    return (
      <div key={node.id}>
        <div
          className={`flat-grid-row ${isFolder ? 'folder-row' : ''}`}
          onClick={handleRowClick}
          onKeyDown={(event) => {
            if (event.key === 'Enter' || event.key === ' ') {
              if (isFolder) {
                toggleFolder(node.id)
              } else {
                onOpen(node.id)
              }
            }
          }}
          role="button"
          tabIndex={0}
        >
          <div className={`col-icon ${isFolder ? 'folder' : 'profile'}`}>
            <span className="material-symbols-outlined">
              {isFolder ? 'folder' : 'dns'}
            </span>
          </div>
          <div className="col-name" style={{ paddingLeft: `${depth * 20}px` }}>
            {isFolder && (
              <span className={`folder-chevron material-symbols-outlined ${isExpanded ? 'expanded' : ''}`}>
                chevron_right
              </span>
            )}
            <strong>{node.name}</strong>
          </div>
          <div className="col-path">{isFolder ? '--' : (node.note || '/')}</div>
          <div className="col-user">{isFolder ? '--' : node.username}</div>
          <div className="col-type">
            <div className={`type-dot ${isFolder ? 'dot-folder' : `dot-${node.type.toLowerCase()}`}`}></div>
            <span>{isFolder ? t.homeFolderType : node.type.toUpperCase()}</span>
          </div>
          <div className="col-action">
            <button
              onClick={(e) => {
                e.stopPropagation()
                if (isFolder) {
                  toggleFolder(node.id, e)
                } else {
                  onOpen(node.id)
                }
              }}
              type="button"
            >
              <span className="material-symbols-outlined">
                {isFolder ? 'arrow_forward' : 'terminal'}
              </span>
            </button>
          </div>
        </div>
        {isFolder && isExpanded && node.children && (
          <div className="folder-children">
            {node.children.map((child) => renderNode(child, depth + 1))}
            {node.children.length === 0 && (
              <div className="flat-grid-row empty-folder-row">
                <div className="col-icon folder">
                  <span className="material-symbols-outlined">folder</span>
                </div>
                <div className="col-name" style={{ paddingLeft: `${(depth + 1) * 20}px` }}>
                  <span>{t.emptyFolder}</span>
                </div>
                <div className="col-path">--</div>
                <div className="col-user">--</div>
                <div className="col-type">
                  <div className="type-dot dot-folder"></div>
                  <span>FOLDER</span>
                </div>
                <div className="col-action"></div>
              </div>
            )}
          </div>
        )}
      </div>
    )
  }

  return (
    <div className="quick-links-page">
      <header className="page-header">
        <div className="header-meta">
          <h1 className="header-title">{t.quickConnect}</h1>
          <p className="header-subtitle">管理和连接到你配置的环境</p>
        </div>
        <button className="btn-new-connection" onClick={onOpenNewConnection} type="button">
          <span className="material-symbols-outlined">add</span>
          <span>{t.newConnection}</span>
        </button>
      </header>

      <div className="page-content">
        <div className="flat-grid">
          <div className="grid-header">
            <div></div>
            <div>名称</div>
            <div>路径</div>
            <div>用户</div>
            <div>类型</div>
            <div className="text-right">操作</div>
          </div>

          <div className="grid-rows">
            {tree.map(node => renderNode(node, 0))}
            {tree.length === 0 && (
              <div className="flat-grid-empty">
                {t.noConnections}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
