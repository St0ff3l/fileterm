import type { ConnectionProfile, ConnectionFolder } from '@fileterm/core'
import { useState, useMemo, useRef, useEffect, type DragEvent } from 'react'
import { ConfirmActionDialog } from '../common/ConfirmActionDialog'
import { t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'
import { CloseButton } from '../common/CloseButton'
import { ManagerInlineFolderRow } from '../common/ManagerInlineFolderRow'
import { managerDropClass, resolveManagerDropPosition } from '../common/manager-drag'

type ConnectionTreeNode =
  (ConnectionFolder & { children: ConnectionTreeNode[] }) | (ConnectionProfile & { children?: never })

export function ConnectionManagerModal({
  profiles,
  folders,
  onClose,
  onCreate,
  onDeleteProfile,
  onEditProfile,
  onOpenProfile,
  onCreateFolder,
  onDeleteFolder,
  onUpdateFolder,
  onUpdateOrder,
  standalone = false,
  inline = false,
  onActiveFolderChange,
  onImportConnections,
  onExportConnections
}: {
  profiles: ConnectionProfile[]
  folders: ConnectionFolder[]
  onClose(): void
  onCreate(): void
  onDeleteProfile(profileId: string): void
  onEditProfile(profile: ConnectionProfile): void
  onOpenProfile(profileId: string): void
  onCreateFolder(name: string): void
  onDeleteFolder(folderId: string): void
  onUpdateFolder(folderId: string, updates: Partial<ConnectionFolder>): void
  onUpdateOrder(id: string, newParentId: string | undefined, newOrder: number): void
  standalone?: boolean
  inline?: boolean
  onActiveFolderChange?(name: string): void
  onImportConnections?(): void
  onExportConnections?(): void
}) {
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set())
  const [activeFolderId, setActiveFolderId] = useState<'all' | string>('all')
  const [searchQuery, setSearchQuery] = useState('')
  const [draggingId, setDraggingId] = useState<string | null>(null)
  const [dragOverId, setDragOverId] = useState<string | null>(null)
  const [dragPosition, setDragPosition] = useState<'top' | 'bottom' | 'inside' | null>(null)
  const [isCreatingFolder, setIsCreatingFolder] = useState(false)
  const [isActionsExpanded, setIsActionsExpanded] = useState(false)
  const [newFolderName, setNewFolderName] = useState('')
  const [editingFolder, setEditingFolder] = useState<{ id: string; name: string } | null>(null)
  const [pendingDelete, setPendingDelete] = useState<
    { kind: 'folder'; id: string; name: string } | { kind: 'profile'; id: string; name: string } | null
  >(null)
  const suppressRowClickRef = useRef(false)

  const stopInteractiveEvent = (event: React.SyntheticEvent) => {
    event.stopPropagation()
  }

  const toggleFolder = (folderId: string, event?: React.MouseEvent) => {
    event?.stopPropagation()
    setExpandedFolders((prev) => {
      const next = new Set(prev)
      if (next.has(folderId)) next.delete(folderId)
      else next.add(folderId)
      return next
    })
  }

  const saveFolderRename = () => {
    if (!editingFolder) return
    const name = editingFolder.name.trim()
    const current = folders.find((folder) => folder.id === editingFolder.id)
    if (name && name !== current?.name) {
      onUpdateFolder(editingFolder.id, { name })
    }
    setEditingFolder(null)
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

    items.forEach((item) => {
      map.set(item.id, item)
    })

    items.forEach((item) => {
      const parent = item.parentId ? map.get(item.parentId) : undefined
      if (parent?.type === 'folder') {
        parent.children.push(item)
      } else {
        roots.push(item)
      }
    })

    const sortNodes = (nodes: ConnectionTreeNode[]) => {
      nodes.sort((a, b) => (a.order ?? 0) - (b.order ?? 0))
      nodes.forEach((n) => {
        if (n.type === 'folder') sortNodes(n.children)
      })
    }
    sortNodes(roots)
    return { roots, map }
  }, [profiles, folders])

  const countProfilesInNodes = (nodes: ConnectionTreeNode[]): number => {
    return nodes.reduce((total, node) => {
      if (node.type !== 'folder') {
        return total + 1
      }
      return total + countProfilesInNodes(node.children)
    }, 0)
  }

  const folderNavItems = useMemo(() => {
    const items: Array<{ id: 'all' | string; name: string; count: number; depth: number }> = [
      { id: 'all', name: t.allConnections, count: profiles.length, depth: 0 }
    ]

    const walkFolders = (nodes: ConnectionTreeNode[], depth: number) => {
      nodes.forEach((node) => {
        if (node.type !== 'folder') {
          return
        }
        items.push({
          id: node.id,
          name: node.name,
          count: countProfilesInNodes(node.children),
          depth
        })
        walkFolders(node.children, depth + 1)
      })
    }

    walkFolders(tree.roots, 0)
    return items
  }, [profiles.length, tree.roots, t.allConnections])

  const activeFolderNode = activeFolderId === 'all' ? null : tree.map.get(activeFolderId)
  const resolvedActiveFolderId = activeFolderNode?.type === 'folder' ? activeFolderId : 'all'
  const activeBaseNodes = activeFolderNode?.type === 'folder' ? activeFolderNode.children : tree.roots

  useEffect(() => {
    const name = resolvedActiveFolderId === 'all' ? t.allConnections : activeFolderNode?.name || ''
    onActiveFolderChange?.(name)
  }, [resolvedActiveFolderId, activeFolderNode, onActiveFolderChange, t.allConnections])

  const visibleNodes = useMemo(() => {
    const query = searchQuery.trim().toLocaleLowerCase()
    if (!query) {
      return activeBaseNodes
    }

    const matches: ConnectionTreeNode[] = []
    const walkNodes = (nodes: ConnectionTreeNode[]) => {
      nodes.forEach((node) => {
        const searchableText =
          node.type === 'folder'
            ? node.name
            : [node.name, node.host, String(node.port), node.username, node.type, node.note ?? ''].join(' ')

        if (searchableText.toLocaleLowerCase().includes(query)) {
          matches.push(node)
        }
        if (node.type === 'folder') {
          walkNodes(node.children)
        }
      })
    }

    walkNodes(activeBaseNodes)
    return matches
  }, [activeBaseNodes, searchQuery])

  const handleDragStart = (e: DragEvent, id: string) => {
    e.stopPropagation()
    suppressRowClickRef.current = true
    setDraggingId(id)
    e.dataTransfer.effectAllowed = 'move'
    // For firefox
    e.dataTransfer.setData('text/plain', id)
  }

  const handleDragOver = (e: DragEvent, targetId: string, type: 'folder' | 'profile') => {
    e.preventDefault()
    e.stopPropagation()
    if (draggingId === targetId) return

    const pos = resolveManagerDropPosition(e.currentTarget as HTMLElement, e.clientY, type === 'folder')

    setDragOverId(targetId)
    setDragPosition(pos)
  }

  const handleRootDragOver = (e: DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    if (draggingId) {
      setDragOverId('all')
      setDragPosition('inside')
    }
  }

  const handleRootDragLeave = (e: DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    if (dragOverId === 'all') {
      setDragOverId(null)
      setDragPosition(null)
    }
  }

  const handleRootDrop = (e: DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    if (draggingId) {
      const rootSiblings = tree.roots.filter((node) => node.id !== draggingId)
      const lastRoot = rootSiblings[rootSiblings.length - 1]
      onUpdateOrder(draggingId, undefined, (lastRoot?.order ?? 0) + 1000)
    }
    setDraggingId(null)
    setDragOverId(null)
    setDragPosition(null)
  }

  const handleDragLeave = (e: DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setDragOverId(null)
    setDragPosition(null)
  }

  const handleDrop = (e: DragEvent, targetId: string) => {
    e.preventDefault()
    e.stopPropagation()
    if (!draggingId || draggingId === targetId) {
      setDraggingId(null)
      setDragOverId(null)
      return
    }

    const draggedNode = tree.map.get(draggingId)
    const targetNode = tree.map.get(targetId)
    if (!draggedNode || !targetNode) return

    // Prevent dropping a folder into its own descendant
    let current: ConnectionTreeNode | undefined = targetNode
    let invalid = false
    while (current?.parentId) {
      if (current.parentId === draggingId) {
        invalid = true
        break
      }
      current = tree.map.get(current.parentId)
    }
    if (invalid) {
      setDraggingId(null)
      setDragOverId(null)
      return
    }

    const targetParent = targetNode.parentId ? tree.map.get(targetNode.parentId) : undefined
    let newParentId = targetParent?.type === 'folder' ? targetParent.id : undefined
    const siblings = targetParent?.type === 'folder' ? targetParent.children : tree.roots

    let newOrder = targetNode.order ?? 0

    if (dragPosition === 'inside' && targetNode.type === 'folder') {
      newParentId = targetNode.id
      const children = targetNode.children || []
      newOrder = children.length > 0 ? (children[children.length - 1].order ?? 0) + 1000 : 1000
      // Auto expand when dropped inside
      setExpandedFolders((prev) => new Set(prev).add(targetNode.id))
    } else {
      const targetIndex = siblings.findIndex((s) => s.id === targetId)
      if (dragPosition === 'top') {
        const prev = siblings[targetIndex - 1]
        newOrder = prev ? ((prev.order ?? 0) + (targetNode.order ?? 0)) / 2 : (targetNode.order ?? 0) - 1000
      } else if (dragPosition === 'bottom') {
        const next = siblings[targetIndex + 1]
        newOrder = next ? ((next.order ?? 0) + (targetNode.order ?? 0)) / 2 : (targetNode.order ?? 0) + 1000
      }
    }

    onUpdateOrder(draggingId, newParentId, newOrder)
    setDraggingId(null)
    setDragOverId(null)
    setDragPosition(null)
  }

  const handleDragEnd = () => {
    setDraggingId(null)
    setDragOverId(null)
    setDragPosition(null)
    window.setTimeout(() => {
      suppressRowClickRef.current = false
    }, 0)
  }

  const renderNode = (node: ConnectionTreeNode, depth: number, options: { includeChildren?: boolean } = {}) => {
    const includeChildren = options.includeChildren ?? true
    const isFolder = node.type === 'folder'
    const isExpanded = expandedFolders.has(node.id)
    const isDragOver = dragOverId === node.id
    const isDragging = draggingId === node.id

    const dropClass = managerDropClass(isDragOver, dragPosition)

    return (
      <div key={node.id}>
        <div
          className={`manager-row ${isFolder ? 'folder-row' : ''} ${dropClass} ${isDragging ? 'dragging' : ''}`}
          draggable
          onDragStart={(e) => handleDragStart(e, node.id)}
          onDragOver={(e) => handleDragOver(e, node.id, isFolder ? 'folder' : 'profile')}
          onDragLeave={handleDragLeave}
          onDrop={(e) => handleDrop(e, node.id)}
          onDragEnd={handleDragEnd}
          onDoubleClick={() => {
            if (suppressRowClickRef.current) {
              return
            }
            if (isFolder) {
              toggleFolder(node.id)
              return
            }
            onOpenProfile(node.id)
          }}
          onClick={() => {
            if (suppressRowClickRef.current) {
              return
            }
            if (isFolder) {
              toggleFolder(node.id)
            }
          }}
          onKeyDown={(event) => {
            if (event.key !== 'Enter') {
              return
            }
            event.preventDefault()
            if (isFolder) {
              toggleFolder(node.id)
              return
            }
            onOpenProfile(node.id)
          }}
          role="button"
          tabIndex={0}
        >
          <span className="manager-name-cell" style={{ paddingLeft: `${depth * 18}px` }}>
            {isFolder && (
              <span
                className="folder-icon manager-folder-toggle"
                style={{ transform: isExpanded ? 'rotate(90deg)' : 'none' }}
              >
                <AppIcon name="chevron-right" size={12} />
              </span>
            )}
            {!isFolder && (
              <span className="manager-node-icon">
                <AppIcon name="server" size={14} />
              </span>
            )}
            {isFolder && editingFolder?.id === node.id ? (
              <input
                autoFocus
                className="manager-inline-input"
                value={editingFolder.name}
                onBlur={saveFolderRename}
                onChange={(event) => setEditingFolder({ id: node.id, name: event.target.value })}
                onClick={stopInteractiveEvent}
                onKeyDown={(event) => {
                  event.stopPropagation()
                  if (event.key === 'Enter') saveFolderRename()
                  if (event.key === 'Escape') setEditingFolder(null)
                }}
              />
            ) : (
              <span className="manager-node-name">{node.name}</span>
            )}
          </span>
          <span>{isFolder ? '--' : node.host}</span>
          <span>{isFolder ? '--' : node.port}</span>
          <span>{isFolder ? '--' : node.username}</span>
          <span className={`manager-type-badge ${isFolder ? 'is-folder' : ''}`}>
            {isFolder ? t.homeFolderType : node.type.toUpperCase()}
          </span>
          <span>{isFolder ? '--' : node.note || '/'}</span>
          <span className="manager-actions">
            {!isFolder && (
              <button
                aria-label={t.connect}
                className="manager-icon-action"
                title={t.connect}
                type="button"
                onMouseDown={stopInteractiveEvent}
                onPointerDown={stopInteractiveEvent}
                onClick={(e) => {
                  e.stopPropagation()
                  onOpenProfile(node.id)
                }}
              >
                <AppIcon name="brand" size={14} />
              </button>
            )}
            {isFolder && (
              <button
                aria-label={t.rename}
                className="manager-icon-action"
                title={t.rename}
                type="button"
                onMouseDown={stopInteractiveEvent}
                onPointerDown={stopInteractiveEvent}
                onClick={(event) => {
                  event.stopPropagation()
                  setEditingFolder({ id: node.id, name: node.name })
                }}
              >
                <AppIcon name="edit" size={14} />
              </button>
            )}
            {!isFolder && (
              <button
                aria-label={t.edit}
                className="manager-icon-action"
                title={t.edit}
                type="button"
                onMouseDown={stopInteractiveEvent}
                onPointerDown={stopInteractiveEvent}
                onClick={(e) => {
                  e.stopPropagation()
                  onEditProfile(node)
                }}
              >
                <AppIcon name="edit" size={14} />
              </button>
            )}
            <button
              aria-label={t.delete}
              className="manager-icon-action danger"
              title={t.delete}
              type="button"
              onMouseDown={stopInteractiveEvent}
              onPointerDown={stopInteractiveEvent}
              onClick={(e) => {
                e.stopPropagation()
                setPendingDelete({
                  kind: isFolder ? 'folder' : 'profile',
                  id: node.id,
                  name: node.name
                })
              }}
            >
              <AppIcon name="trash" size={14} />
            </button>
          </span>
        </div>
        {includeChildren && isFolder && isExpanded && node.children && (
          <div className="folder-children">
            {node.children.map((child) => renderNode(child, depth + 1, options))}
            {node.children.length === 0 && (
              <div className="manager-row empty-folder" style={{ paddingLeft: `${(depth + 1) * 18 + 18}px` }}>
                <span>{t.emptyFolder}</span>
              </div>
            )}
          </div>
        )}
      </div>
    )
  }

  const isSearching = searchQuery.trim().length > 0
  const emptyMessage = isSearching ? t.noMatchingConnections : t.noConnections

  const content = (
    <div
      className={`modal-card manager-modal connection-manager-modal ${standalone ? 'standalone' : ''} ${inline ? 'manager-inline' : ''}`}
    >
      <div className="connection-manager-header">
        <span className="connection-manager-title">
          <span className="material-symbols-outlined">settings_ethernet</span>
          <span>{t.connectionManager}</span>
        </span>
        <label className="connection-manager-search">
          <AppIcon name="search" size={14} />
          <input
            aria-label={t.filterConnections}
            placeholder={t.filterConnections}
            type="search"
            value={searchQuery}
            onChange={(event) => setSearchQuery(event.target.value)}
          />
        </label>
        {!inline && (
          <div className="connection-manager-header-actions">
            <CloseButton onClick={onClose} />
          </div>
        )}
      </div>
      <div className="connection-manager-layout">
        <aside className="connection-manager-sidebar" aria-label={t.folder}>
          {folderNavItems.map((item) => (
            <button
              key={item.id}
              className={`connection-manager-sidebar-item ${item.id === resolvedActiveFolderId ? 'active' : ''} ${
                item.id === 'all' ? 'root-drop-target' : ''
              } ${item.id === 'all' && dragOverId === 'all' ? 'drag-over' : ''}`}
              type="button"
              onClick={() => setActiveFolderId(item.id)}
              onDragOver={item.id === 'all' ? handleRootDragOver : undefined}
              onDragLeave={item.id === 'all' ? handleRootDragLeave : undefined}
              onDrop={item.id === 'all' ? handleRootDrop : undefined}
            >
              <span className="connection-manager-sidebar-icon" style={{ paddingLeft: `${item.depth * 12}px` }}>
                <AppIcon name={item.id === 'all' ? 'connections' : 'folder'} size={14} />
              </span>
              <span className="connection-manager-sidebar-label">{item.name}</span>
              <span className="connection-manager-sidebar-count">{item.count}</span>
            </button>
          ))}
        </aside>
        <section className="connection-manager-main">
          <div className="manager-table connection-manager-table">
            <div className="manager-head">
              <span>{t.name}</span>
              <span>{t.host}</span>
              <span>{t.port}</span>
              <span>{t.userColumn}</span>
              <span>{t.type}</span>
              <span>{t.note}</span>
              <span>{t.actions}</span>
            </div>
            <div className="manager-body connection-manager-body">
              {isCreatingFolder && resolvedActiveFolderId === 'all' && (
                <ManagerInlineFolderRow
                  afterNameCells={[
                    '--',
                    '--',
                    '--',
                    <span className="manager-type-badge is-folder" key="type">
                      {t.homeFolderType}
                    </span>,
                    '--',
                    null
                  ]}
                  placeholder={t.folderName}
                  value={newFolderName}
                  onChange={setNewFolderName}
                  onCommit={onCreateFolder}
                  onDismiss={() => {
                    setIsCreatingFolder(false)
                    setNewFolderName('')
                  }}
                />
              )}
              {visibleNodes.map((node) => renderNode(node, 0, { includeChildren: !isSearching }))}
              {visibleNodes.length === 0 && !(isCreatingFolder && resolvedActiveFolderId === 'all') && (
                <div className="connection-manager-empty">{emptyMessage}</div>
              )}
            </div>
          </div>
          <div className={`connection-manager-floating-drawer ${isActionsExpanded ? 'expanded' : ''}`}>
            <div className="drawer-options-wrapper">
              {onImportConnections ? (
                <button
                  className="drawer-option-btn secondary-btn"
                  type="button"
                  onClick={() => {
                    onImportConnections()
                    setIsActionsExpanded(false)
                  }}
                >
                  <span className="material-symbols-outlined">upload_file</span>
                  <span>导入连接</span>
                </button>
              ) : null}
              {onExportConnections ? (
                <button
                  className="drawer-option-btn secondary-btn"
                  type="button"
                  onClick={() => {
                    onExportConnections()
                    setIsActionsExpanded(false)
                  }}
                >
                  <span className="material-symbols-outlined">download</span>
                  <span>导出连接</span>
                </button>
              ) : null}
              <button
                className="drawer-option-btn secondary-btn"
                type="button"
                onClick={() => {
                  setActiveFolderId('all')
                  setIsCreatingFolder(true)
                  setNewFolderName('')
                  setIsActionsExpanded(false)
                }}
              >
                <AppIcon name="folder" size={13} />
                <span>{t.newFolder}</span>
              </button>
              <button
                className="drawer-option-btn primary-btn"
                type="button"
                onClick={() => {
                  onCreate()
                  setIsActionsExpanded(false)
                }}
              >
                <AppIcon name="plus" size={13} />
                <span>{t.newConnection}</span>
              </button>
            </div>
            <button
              className="drawer-trigger-btn"
              type="button"
              onClick={() => setIsActionsExpanded(!isActionsExpanded)}
              aria-label="Expand actions"
            >
              <AppIcon name="plus" size={16} />
            </button>
          </div>
        </section>
      </div>
      {!inline && (
        <div className="connection-manager-footer">
          <span>
            {profiles.length} {t.connectionCountLabel}
          </span>
          <span className="connection-manager-footer-separator"></span>
          <span>
            {folders.length} {t.folderCountLabel}
          </span>
          <span className="connection-manager-footer-spacer"></span>
          <span>{resolvedActiveFolderId === 'all' ? t.allConnections : activeFolderNode?.name}</span>
        </div>
      )}
    </div>
  )

  return (
    <>
      {inline ? (
        content
      ) : standalone ? (
        <div className="manager-window">{content}</div>
      ) : (
        <div className="modal-backdrop">{content}</div>
      )}
      {pendingDelete ? (
        <ConfirmActionDialog
          confirmLabel={t.delete}
          description={`${t.deleteConfirmPrefix}${pendingDelete.name}${t.deleteConfirmSuffix}`}
          onClose={() => setPendingDelete(null)}
          onConfirm={() => {
            if (pendingDelete.kind === 'folder') {
              onDeleteFolder(pendingDelete.id)
            } else {
              onDeleteProfile(pendingDelete.id)
            }
            setPendingDelete(null)
          }}
          title={t.delete}
        />
      ) : null}
    </>
  )
}
