import type { ConnectionProfile, ConnectionFolder } from '@termdock/core'
import { useState, useMemo, useRef, type DragEvent } from 'react'
import { ConfirmActionDialog } from '../common/ConfirmActionDialog'
import { t } from '../../i18n'

type ConnectionTreeNode =
  | (ConnectionFolder & { children: ConnectionTreeNode[] })
  | (ConnectionProfile & { children?: never })

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
  standalone = false
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
}) {
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set())
  const [draggingId, setDraggingId] = useState<string | null>(null)
  const [dragOverId, setDragOverId] = useState<string | null>(null)
  const [dragPosition, setDragPosition] = useState<'top' | 'bottom' | 'inside' | null>(null)
  const [isCreatingFolder, setIsCreatingFolder] = useState(false)
  const [newFolderName, setNewFolderName] = useState('')
  const [pendingDelete, setPendingDelete] = useState<
    | { kind: 'folder'; id: string; name: string }
    | { kind: 'profile'; id: string; name: string }
    | null
  >(null)
  const suppressRowClickRef = useRef(false)

  const stopInteractiveEvent = (event: React.SyntheticEvent) => {
    event.stopPropagation()
  }

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
    return { roots, map }
  }, [profiles, folders])

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

    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const y = e.clientY - rect.top
    const height = rect.height

    let pos: 'top' | 'bottom' | 'inside' = 'bottom'
    if (type === 'folder') {
      if (y < height * 0.25) pos = 'top'
      else if (y > height * 0.75) pos = 'bottom'
      else pos = 'inside'
    } else {
      if (y < height * 0.5) pos = 'top'
      else pos = 'bottom'
    }

    setDragOverId(targetId)
    setDragPosition(pos)
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
    let siblings = targetParent?.type === 'folder' ? targetParent.children : tree.roots

    let newOrder = targetNode.order ?? 0

    if (dragPosition === 'inside' && targetNode.type === 'folder') {
      newParentId = targetNode.id
      const children = targetNode.children || []
      newOrder = children.length > 0 ? (children[children.length - 1].order ?? 0) + 1000 : 1000
      // Auto expand when dropped inside
      setExpandedFolders(prev => new Set(prev).add(targetNode.id))
    } else {
      const targetIndex = siblings.findIndex(s => s.id === targetId)
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

  const renderNode = (node: ConnectionTreeNode, depth: number) => {
    const isFolder = node.type === 'folder'
    const isExpanded = expandedFolders.has(node.id)
    const isDragOver = dragOverId === node.id
    const isDragging = draggingId === node.id

    let dropClass = ''
    if (isDragOver && dragPosition) {
      dropClass = `drop-${dragPosition}`
    }

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
          <span style={{ paddingLeft: `${depth * 20}px`, display: 'flex', alignItems: 'center', gap: '6px' }}>
            {isFolder && (
              <span className="folder-icon" style={{ transform: isExpanded ? 'rotate(90deg)' : 'none', transition: 'transform 0.1s', display: 'inline-block', fontSize: '10px' }}>
                ▶
              </span>
            )}
            {!isFolder && <span style={{ width: '12px', display: 'inline-block' }}></span>}
            {node.name}
          </span>
          <span>{isFolder ? '--' : node.host}</span>
          <span>{isFolder ? '--' : node.port}</span>
          <span>{isFolder ? '--' : node.username}</span>
          <span>{isFolder ? t.homeFolderType : node.type.toUpperCase()}</span>
          <span>{isFolder ? '--' : (node.note || '/')}</span>
          <span className="manager-actions">
            {!isFolder && (
              <button
                className="flat-button compact"
                type="button"
                onMouseDown={stopInteractiveEvent}
                onPointerDown={stopInteractiveEvent}
                onClick={(e) => {
                  e.stopPropagation()
                  onEditProfile(node)
                }}
              >
                {t.edit}
              </button>
            )}
            <button
              className="flat-button compact danger"
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
              {t.delete}
            </button>
          </span>
        </div>
        {isFolder && isExpanded && node.children && (
          <div className="folder-children">
            {node.children.map((child) => renderNode(child, depth + 1))}
            {node.children.length === 0 && (
              <div className="manager-row empty-folder" style={{ paddingLeft: `${(depth + 1) * 20 + 14}px`, color: '#666' }}>
                <span style={{ gridColumn: '1 / -1' }}>{t.emptyFolder}</span>
              </div>
            )}
          </div>
        )}
      </div>
    )
  }

  const content = (
    <div className={`modal-card manager-modal ${standalone ? 'standalone' : ''}`}>
      <div className="modal-header">
        <span>{t.connectionManager}</span>
        {!standalone ? <button className="icon-button" onClick={onClose} type="button">×</button> : null}
      </div>
      <div className="manager-toolbar manager-toolbar-spacious">
        <button 
          className="flat-button" 
          type="button" 
          onClick={() => {
            setIsCreatingFolder(true)
            setNewFolderName('')
          }}
        >
          {t.newFolder}
        </button>
        <button className="primary-button" type="button" onClick={onCreate}>{t.newConnection}</button>
      </div>
      <div className="manager-table">
        <div className="manager-head">
          <span>{t.name}</span>
          <span>{t.host}</span>
          <span>{t.port}</span>
          <span>{t.userColumn}</span>
          <span>{t.type}</span>
          <span>{t.note}</span>
          <span>{t.actions}</span>
        </div>
        <div className="manager-body" style={{ flex: 1, overflowY: 'auto' }}>
          {isCreatingFolder && (
            <div className="manager-row folder-row">
              <span style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                <span className="folder-icon" style={{ display: 'inline-block', fontSize: '10px' }}>▶</span>
                <input
                  type="text"
                  autoFocus
                  value={newFolderName}
                  onChange={(e) => setNewFolderName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && newFolderName.trim()) {
                      onCreateFolder(newFolderName.trim())
                      setIsCreatingFolder(false)
                    } else if (e.key === 'Escape') {
                      setIsCreatingFolder(false)
                    }
                  }}
                  onBlur={() => {
                    if (newFolderName.trim()) onCreateFolder(newFolderName.trim())
                    setIsCreatingFolder(false)
                  }}
                  style={{ background: 'var(--bg-active)', border: 'none', color: 'inherit', padding: '2px 4px', outline: 'none' }}
                  placeholder={t.folderName}
                />
              </span>
              <span>--</span>
              <span>--</span>
              <span>--</span>
              <span>{t.homeFolderType}</span>
              <span>--</span>
              <span></span>
            </div>
          )}
          {tree.roots.map((node) => renderNode(node, 0))}
          {tree.roots.length === 0 && !isCreatingFolder && (
            <div style={{ padding: '20px', textAlign: 'center', color: '#666' }}>
              {t.noConnections}
            </div>
          )}
        </div>
      </div>
    </div>
  )

  return (
    <>
      {standalone ? (
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
