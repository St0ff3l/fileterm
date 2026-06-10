import { useMemo, useRef, useState, type DragEvent } from 'react'
import type { CommandFolder, CommandTemplate, CommandTemplateInput } from '@termdock/core'
import { ConfirmActionDialog } from '../common/ConfirmActionDialog'
import { t } from '../../i18n'
import { CommandEditorModal, emptyCommandForm, toCommandTemplateInput } from './CommandEditorModal'

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
  standalone = false
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
}) {
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set())
  const [draggingId, setDraggingId] = useState<string | null>(null)
  const [dragOverId, setDragOverId] = useState<string | null>(null)
  const [dragPosition, setDragPosition] = useState<'top' | 'bottom' | 'inside' | null>(null)
  const [isCreatingFolder, setIsCreatingFolder] = useState(false)
  const [newFolderName, setNewFolderName] = useState('')
  const [editorState, setEditorState] = useState<{ mode: 'create' | 'edit'; commandId?: string } | null>(null)
  const [pendingDelete, setPendingDelete] = useState<
    | { kind: 'folder'; id: string; name: string }
    | { kind: 'command'; id: string; name: string }
    | null
  >(null)
  const suppressRowClickRef = useRef(false)

  const desktopApi = window.termdock

  const stopInteractiveEvent = (event: React.SyntheticEvent) => {
    event.stopPropagation()
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
      if (item.parentId && map.has(item.parentId)) {
        const parent = map.get(item.parentId)!
        if (parent.type === 'command-folder') {
          parent.children.push(item)
        } else {
          roots.push(item)
        }
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

  const toggleFolder = (folderId: string, event?: React.MouseEvent) => {
    event?.stopPropagation()
    setExpandedFolders(prev => {
      const next = new Set(prev)
      if (next.has(folderId)) next.delete(folderId)
      else next.add(folderId)
      return next
    })
  }

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
      const children = targetNode.children
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
    // We don't have a reliable way to get parentId here easily without more state, 
    // but we can pass undefined or the user can select it in the modal.
    void desktopApi.openCommandFormWindow(mode, commandId)
  }

  const editorInitialValue = editorState?.mode === 'edit'
    ? (() => {
        const command = commandTemplates.find((item) => item.id === editorState.commandId)
        return command ? toCommandTemplateInput(command) : emptyCommandForm
      })()
    : emptyCommandForm

  const renderNode = (node: CommandTreeNode, depth: number) => {
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
          <span style={{ paddingLeft: `${depth * 20}px`, display: 'flex', alignItems: 'center', gap: '6px', overflow: 'hidden' }}>
            {isFolder && (
              <span className="folder-icon" style={{ transform: isExpanded ? 'rotate(90deg)' : 'none', transition: 'transform 0.1s', display: 'inline-block', fontSize: '10px' }}>
                ▶
              </span>
            )}
            {!isFolder && <span style={{ width: '12px', display: 'inline-block' }}></span>}
            <strong style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{node.name}</strong>
          </span>
          <span>
            {isFolder ? '--' : <code style={{ fontSize: '11px', opacity: 0.8, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', display: 'block' }}>{node.command}</code>}
          </span>
          <span className="manager-actions">
            {!isFolder && (
              <button
                className="flat-button compact"
                type="button"
                onMouseDown={stopInteractiveEvent}
                onPointerDown={stopInteractiveEvent}
                onClick={(e) => {
                  e.stopPropagation()
                  openEditorWindow('edit', node.id)
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
                  kind: isFolder ? 'folder' : 'command',
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
                <span style={{ gridColumn: '1 / -1' }}>{t.commandEmpty}</span>
              </div>
            )}
          </div>
        )}
      </div>
    )
  }

  const content = (
    <div className={`modal-card manager-modal command-manager-modal ${standalone ? 'standalone' : ''}`}>
      <div className="modal-header">
        <span>{t.commandManager}</span>
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
        <button className="primary-button" type="button" onClick={() => openEditorWindow('create')}>{t.newCommand}</button>
      </div>
      <div className="manager-table command-manager-table">
        <div className="manager-head">
          <span>{t.name}</span>
          <span>{t.commandTemplate}</span>
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
              <span></span>
            </div>
          )}
          {tree.roots.map((node) => renderNode(node, 0))}
          {tree.roots.length === 0 && !isCreatingFolder && (
            <div style={{ padding: '20px', textAlign: 'center', color: '#666' }}>
              {t.commandEmpty}
            </div>
          )}
        </div>
      </div>
    </div>
  )

  return (
    <>
      {standalone ? (
        <div className="standalone-shell">{content}</div>
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
