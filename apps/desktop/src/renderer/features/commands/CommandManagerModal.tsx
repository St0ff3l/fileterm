import { useState, useMemo, useRef, useEffect, type DragEvent } from 'react'
import type { CommandFolder, CommandTemplate, CommandTemplateInput } from '@fileterm/core'
import { ConfirmActionDialog } from '../common/ConfirmActionDialog'
import { t } from '../../i18n'
import { CommandEditorModal, emptyCommandForm, toCommandTemplateInput } from './CommandEditorModal'
import { AppIcon } from '../common/AppIcon'

type CommandTreeNode =
  | (CommandFolder & { children: CommandTreeNode[] })
  | (CommandTemplate & { children?: never })

export function CommandManagerModal({
  commandFolders,
  commandTemplates,
  onClose,
  onCreateFolder,
  onDeleteFolder,
  onUpdateFolder,
  onUpdateOrder,
  onCreateCommand,
  onUpdateCommand,
  onDeleteCommand,
  standalone = false,
  inline = false,
  onActiveFolderChange
}: {
  commandFolders: CommandFolder[]
  commandTemplates: CommandTemplate[]
  onClose(): void
  onCreateFolder(name: string): void
  onDeleteFolder(folderId: string): void
  onUpdateFolder(folderId: string, updates: Partial<CommandFolder>): void
  onUpdateOrder(id: string, newParentId: string | undefined, newOrder: number): void
  onCreateCommand(input: CommandTemplateInput): void
  onUpdateCommand(commandId: string, input: CommandTemplateInput): void
  onDeleteCommand(commandId: string): void
  standalone?: boolean
  inline?: boolean
  onActiveFolderChange?(name: string): void
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
  const [editorState, setEditorState] = useState<{ mode: 'create' | 'edit'; commandId?: string } | null>(null)
  const [pendingDelete, setPendingDelete] = useState<
    | { kind: 'folder'; id: string; name: string }
    | { kind: 'command'; id: string; name: string }
    | null
  >(null)
  const suppressRowClickRef = useRef(false)

  const desktopApi = window.fileterm

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
    const items: CommandTreeNode[] = [
      ...commandTemplates.map((command, index) => ({
        ...command,
        order: typeof command.order === 'number' ? command.order : index * 1000
      })),
      ...commandFolders.map((folder, index) => ({
        ...folder,
        order: typeof folder.order === 'number' ? folder.order : (commandTemplates.length + index) * 1000,
        children: []
      }))
    ]

    const roots: CommandTreeNode[] = []
    const map = new Map<string, CommandTreeNode>()

    items.forEach(item => {
      map.set(item.id, item)
    })

    items.forEach(item => {
      const parent = item.parentId ? map.get(item.parentId) : undefined
      if (parent?.type === 'command-folder') {
        parent.children.push(item)
      } else {
        roots.push(item)
      }
    })

    const sortNodes = (nodes: CommandTreeNode[]) => {
      nodes.sort((a, b) => (a.order ?? 0) - (b.order ?? 0))
      nodes.forEach(n => {
        if (n.type === 'command-folder') sortNodes(n.children)
      })
    }
    sortNodes(roots)
    return { roots, map }
  }, [commandTemplates, commandFolders])

  const countCommandsInNodes = (nodes: CommandTreeNode[]): number => {
    return nodes.reduce((total, node) => {
      if (node.type !== 'command-folder') {
        return total + 1
      }
      return total + countCommandsInNodes(node.children)
    }, 0)
  }

  const folderNavItems = useMemo(() => {
    const items: Array<{ id: 'all' | string; name: string; count: number; depth: number }> = [
      { id: 'all', name: t.allCommands, count: commandTemplates.length, depth: 0 }
    ]

    const walkFolders = (nodes: CommandTreeNode[], depth: number) => {
      nodes.forEach((node) => {
        if (node.type !== 'command-folder') {
          return
        }
        items.push({
          id: node.id,
          name: node.name,
          count: countCommandsInNodes(node.children),
          depth
        })
        walkFolders(node.children, depth + 1)
      })
    }

    walkFolders(tree.roots, 0)
    return items
  }, [commandTemplates.length, tree.roots, t.allCommands])

  const activeFolderNode = activeFolderId === 'all' ? null : tree.map.get(activeFolderId)
  const resolvedActiveFolderId = activeFolderNode?.type === 'command-folder' ? activeFolderId : 'all'
  const activeBaseNodes = activeFolderNode?.type === 'command-folder' ? activeFolderNode.children : tree.roots

  useEffect(() => {
    const name = resolvedActiveFolderId === 'all' ? t.allCommands : activeFolderNode?.name || ''
    onActiveFolderChange?.(name)
  }, [resolvedActiveFolderId, activeFolderNode, onActiveFolderChange, t.allCommands])

  const visibleNodes = useMemo(() => {
    const query = searchQuery.trim().toLocaleLowerCase()
    if (!query) {
      return activeBaseNodes
    }

    const matches: CommandTreeNode[] = []
    const walkNodes = (nodes: CommandTreeNode[]) => {
      nodes.forEach((node) => {
        const searchableText = node.type === 'command-folder'
          ? node.name
          : [
              node.name,
              node.command,
              node.description ?? ''
            ].join(' ')

        if (searchableText.toLocaleLowerCase().includes(query)) {
          matches.push(node)
        }
        if (node.type === 'command-folder') {
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
    e.dataTransfer.setData('text/plain', id)
  }

  const handleDragOver = (e: DragEvent, targetId: string, type: 'command-folder' | 'command-template') => {
    e.preventDefault()
    e.stopPropagation()
    if (draggingId === targetId) return

    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const y = e.clientY - rect.top
    const height = rect.height

    let pos: 'top' | 'bottom' | 'inside' = 'bottom'
    if (type === 'command-folder') {
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
    let current: CommandTreeNode | undefined = targetNode
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
    let newParentId = targetParent?.type === 'command-folder' ? targetParent.id : undefined
    let siblings = targetParent?.type === 'command-folder' ? targetParent.children : tree.roots

    let newOrder = targetNode.order ?? 0

    if (dragPosition === 'inside' && targetNode.type === 'command-folder') {
      newParentId = targetNode.id
      const children = targetNode.children || []
      newOrder = children.length > 0 ? (children[children.length - 1].order ?? 0) + 1000 : 1000
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

  const openEditorWindow = (mode: 'create' | 'edit', commandId?: string) => {
    if (!desktopApi) {
      setEditorState({ mode, commandId })
      return
    }
    void desktopApi.openCommandFormWindow(mode, commandId)
  }

  const editorInitialValue = editorState?.mode === 'edit'
    ? (() => {
        const command = commandTemplates.find((item) => item.id === editorState.commandId)
        return command ? toCommandTemplateInput(command) : emptyCommandForm
      })()
    : emptyCommandForm

  const renderNode = (
    node: CommandTreeNode,
    depth: number,
    options: { includeChildren?: boolean } = {}
  ) => {
    const includeChildren = options.includeChildren ?? true
    const isFolder = node.type === 'command-folder'
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
          onDragOver={(e) => handleDragOver(e, node.id, isFolder ? 'command-folder' : 'command-template')}
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
            openEditorWindow('edit', node.id)
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
            openEditorWindow('edit', node.id)
          }}
          role="button"
          tabIndex={0}
        >
          <span className="manager-name-cell" style={{ paddingLeft: `${depth * 18}px` }}>
            {isFolder && (
              <span className="folder-icon manager-folder-toggle" style={{ transform: isExpanded ? 'rotate(90deg)' : 'none' }}>
                ▶
              </span>
            )}
            {!isFolder && <span className="manager-node-icon"><AppIcon name="brand" size={14} /></span>}
            <span className="manager-node-name">{node.name}</span>
          </span>
          <span>
            {isFolder ? '--' : <code style={{ fontSize: '11px', opacity: 0.8, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', display: 'block' }}>{node.command}</code>}
          </span>
          <span>{isFolder ? '--' : (node.description || '/')}</span>
          <span className="manager-actions">
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
                  openEditorWindow('edit', node.id)
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
                  kind: isFolder ? 'folder' : 'command',
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
                <span>{t.commandEmpty}</span>
              </div>
            )}
          </div>
        )}
      </div>
    )
  }

  const isSearching = searchQuery.trim().length > 0
  const emptyMessage = isSearching ? t.noMatchingCommands : t.commandEmpty

  const content = (
    <div className={`modal-card manager-modal connection-manager-modal command-manager-modal ${standalone ? 'standalone' : ''} ${inline ? 'manager-inline' : ''}`}>
      <div className="connection-manager-header">
        <span className="connection-manager-title">
          <span className="material-symbols-outlined">terminal</span>
          <span>{t.commandManager}</span>
        </span>
        <label className="connection-manager-search">
          <AppIcon name="search" size={14} />
          <input
            aria-label={t.filterCommands}
            placeholder={t.filterCommands}
            type="search"
            value={searchQuery}
            onChange={(event) => setSearchQuery(event.target.value)}
          />
        </label>
        {!inline && (
          <div className="connection-manager-header-actions">
            <button aria-label={t.closeTab} className="manager-close-button" onClick={onClose} title={t.closeTab} type="button">×</button>
          </div>
        )}
      </div>
      <div className="connection-manager-layout">
        <aside className="connection-manager-sidebar" aria-label={t.commandCategory}>
          {folderNavItems.map((item) => (
            <button
              key={item.id}
              className={`connection-manager-sidebar-item ${item.id === resolvedActiveFolderId ? 'active' : ''}`}
              type="button"
              onClick={() => setActiveFolderId(item.id)}
            >
              <span className="connection-manager-sidebar-icon" style={{ paddingLeft: `${item.depth * 12}px` }}>
                <AppIcon name={item.id === 'all' ? 'brand' : 'folder'} size={14} />
              </span>
              <span className="connection-manager-sidebar-label">{item.name}</span>
              <span className="connection-manager-sidebar-count">{item.count}</span>
            </button>
          ))}
        </aside>
        <section className="connection-manager-main">
          <div className="manager-table connection-manager-table command-manager-table">
            <div className="manager-head">
              <span>{t.name}</span>
              <span>{t.commandTemplate}</span>
              <span>{t.description}</span>
              <span>{t.actions}</span>
            </div>
            <div className="manager-body connection-manager-body">
              {isCreatingFolder && resolvedActiveFolderId === 'all' && (
                <div className="manager-row folder-row">
                  <span className="manager-name-cell">
                    <span className="folder-icon manager-folder-toggle">▶</span>
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
                      className="manager-inline-input"
                      placeholder={t.folderName}
                    />
                  </span>
                  <span>--</span>
                  <span>--</span>
                  <span></span>
                </div>
              )}
              {visibleNodes.map((node) => renderNode(node, 0, { includeChildren: !isSearching }))}
              {visibleNodes.length === 0 && !(isCreatingFolder && resolvedActiveFolderId === 'all') && (
                <div className="connection-manager-empty">
                  {emptyMessage}
                </div>
              )}
            </div>
          </div>
          <div className={`connection-manager-floating-drawer ${isActionsExpanded ? 'expanded' : ''}`}>
            <div className="drawer-options-wrapper">
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
                  openEditorWindow('create')
                  setIsActionsExpanded(false)
                }}
              >
                <AppIcon name="plus" size={13} />
                <span>{t.newCommand}</span>
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
          <span>{commandTemplates.length} {t.commandCountLabel}</span>
          <span className="connection-manager-footer-separator"></span>
          <span>{commandFolders.length} {t.folderCountLabel}</span>
          <span className="connection-manager-footer-spacer"></span>
          <span>{resolvedActiveFolderId === 'all' ? t.allCommands : activeFolderNode?.name}</span>
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
      {editorState ? (
        <CommandEditorModal
          folders={commandFolders}
          mode={editorState.mode}
          initialValue={editorInitialValue}
          onClose={() => setEditorState(null)}
          onSubmit={(input) => {
            if (editorState.mode === 'edit' && editorState.commandId) {
              onUpdateCommand(editorState.commandId, input)
            } else {
              onCreateCommand(input)
            }
            setEditorState(null)
          }}
        />
      ) : null}
      {pendingDelete ? (
        <ConfirmActionDialog
          confirmLabel={t.delete}
          description={`${t.deleteConfirmPrefix}${pendingDelete.name}${t.deleteConfirmSuffix}`}
          onClose={() => setPendingDelete(null)}
          onConfirm={() => {
            if (pendingDelete.kind === 'folder') {
              onDeleteFolder(pendingDelete.id)
            } else {
              onDeleteCommand(pendingDelete.id)
            }
            setPendingDelete(null)
          }}
          title={t.delete}
        />
      ) : null}
    </>
  )
}
