import { useMemo, useState, type DragEvent, type ReactNode } from 'react'
import type { CommandFolder, CommandTemplate, CommandTemplateInput } from '@termdock/core'
import { t } from '../../i18n'
import { extractCommandParams, sortByOrder } from './command-utils'

const emptyCommandForm: CommandTemplateInput = {
  name: '',
  command: '',
  description: '',
  parentId: undefined,
  appendCarriageReturn: true
}

type FolderNode = CommandFolder & { children: FolderNode[] }

function CommandDialogShell({
  title,
  onClose,
  children
}: {
  title: string
  onClose(): void
  children: ReactNode
}) {
  return (
    <div className="modal-backdrop command-dialog-backdrop" onClick={onClose}>
      <div className="command-dialog command-editor-page" onClick={(event) => event.stopPropagation()}>
        <div className="command-dialog-titlebar">
          <div className="command-dialog-lights" aria-hidden="true">
            <span className="is-red" />
            <span className="is-muted" />
            <span className="is-green" />
          </div>
          <strong>{title}</strong>
        </div>
        <div className="command-dialog-body">
          {children}
        </div>
      </div>
    </div>
  )
}

function CommandEditorPage({
  folders,
  initialValue,
  onClose,
  onSubmit
}: {
  folders: CommandFolder[]
  initialValue: CommandTemplateInput
  onClose(): void
  onSubmit(input: CommandTemplateInput): void
}) {
  const [form, setForm] = useState<CommandTemplateInput>(initialValue)

  return (
    <CommandDialogShell title={initialValue.name ? `${t.commandEdit}-${initialValue.name}` : t.commandCreate} onClose={onClose}>
      <div className="command-editor-dialog-form">
        <div className="command-editor-grid">
          <label className="command-editor-field full">
            <span>{t.name}</span>
            <input
              type="text"
              value={form.name}
              onChange={(event) => setForm((prev) => ({ ...prev, name: event.currentTarget.value }))}
            />
          </label>
          <label className="command-editor-field">
            <span>{t.commandCategory}</span>
            <select
              value={form.parentId ?? ''}
              onChange={(event) => setForm((prev) => ({ ...prev, parentId: event.currentTarget.value || undefined }))}
            >
              <option value="">{t.commandUncategorized}</option>
              {folders.map((folder) => (
                <option key={folder.id} value={folder.id}>{folder.name}</option>
              ))}
            </select>
          </label>
          <label className="command-editor-field">
            <span>{t.note}</span>
            <input
              type="text"
              value={form.description ?? ''}
              onChange={(event) => setForm((prev) => ({ ...prev, description: event.currentTarget.value }))}
            />
          </label>
          <label className="command-editor-field full command-editor-dialog-textarea">
            <span>{t.commandTemplate}</span>
            <textarea
              rows={12}
              value={form.command}
              onChange={(event) => setForm((prev) => ({ ...prev, command: event.currentTarget.value }))}
            />
          </label>
          <div className="command-editor-field full command-editor-dialog-params">
            <span>{t.commandParamHint}</span>
            <div className="command-param-hints">
              {[1, 2, 3, 4, 5].map((index) => (
                <button
                  key={index}
                  type="button"
                  onClick={() => setForm((prev) => ({ ...prev, command: `${prev.command}[p#${index}]` }))}
                >
                  {`${t.commandParam}${index}`}
                </button>
              ))}
            </div>
            <small>{t.commandParamExplain}</small>
          </div>
          <label className="command-editor-field full command-editor-checkbox-row">
            <input
              checked={form.appendCarriageReturn ?? true}
              type="checkbox"
              onChange={(event) => setForm((prev) => ({ ...prev, appendCarriageReturn: event.currentTarget.checked }))}
            />
            <span>{t.commandAppendCr}</span>
          </label>
          <div className="command-editor-field full command-preview">
            <span>{t.commandDetectedParams}</span>
            <code>{extractCommandParams(form.command).join(', ') || '-'}</code>
          </div>
        </div>
        <div className="command-dialog-actions">
          <button
            className="flat-button compact"
            type="button"
            onClick={() => {
              if (!form.name?.trim() || !form.command?.trim()) {
                return
              }
              onSubmit({
                ...form,
                name: form.name.trim(),
                command: form.command.trim(),
                description: form.description?.trim() || undefined
              })
            }}
          >
            {t.save}
          </button>
          <button className="flat-button compact" type="button" onClick={onClose}>{t.cancel}</button>
        </div>
      </div>
    </CommandDialogShell>
  )
}

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
  const [selectedFolderId, setSelectedFolderId] = useState<string>('all')
  const [selectedCommandId, setSelectedCommandId] = useState<string | null>(commandTemplates[0]?.id ?? null)
  const [newFolderName, setNewFolderName] = useState('')
  const [editingFolderId, setEditingFolderId] = useState<string | null>(null)
  const [editingFolderName, setEditingFolderName] = useState('')
  const [draggingFolderId, setDraggingFolderId] = useState<string | null>(null)
  const [dragOverFolderId, setDragOverFolderId] = useState<string | null>(null)
  const [dragPosition, setDragPosition] = useState<'top' | 'bottom' | 'inside' | null>(null)
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set())
  const [editorState, setEditorState] = useState<{ mode: 'create' | 'edit'; commandId?: string } | null>(null)

  const folderTree = useMemo(() => {
    const roots: FolderNode[] = []
    const map = new Map<string, FolderNode>()
    const items = sortByOrder(commandFolders).map((folder) => ({ ...folder, children: [] as FolderNode[] }))

    items.forEach((folder) => {
      map.set(folder.id, folder)
    })

    items.forEach((folder) => {
      if (folder.parentId && map.has(folder.parentId)) {
        map.get(folder.parentId)?.children.push(folder)
      } else {
        roots.push(folder)
      }
    })

    const sortNodes = (nodes: FolderNode[]) => {
      nodes.sort((left, right) => (left.order ?? 0) - (right.order ?? 0))
      nodes.forEach((node) => sortNodes(node.children))
    }

    sortNodes(roots)
    return { roots, map }
  }, [commandFolders])

  const folders = useMemo(() => sortByOrder(commandFolders), [commandFolders])

  const filteredCommands = useMemo(() => {
    if (selectedFolderId === 'all') {
      return sortByOrder(commandTemplates)
    }
    if (selectedFolderId === 'ungrouped') {
      return sortByOrder(commandTemplates.filter((item) => !item.parentId))
    }
    return sortByOrder(commandTemplates.filter((item) => item.parentId === selectedFolderId))
  }, [commandTemplates, selectedFolderId])

  const selectedCommand = useMemo(
    () => commandTemplates.find((item) => item.id === selectedCommandId) ?? null,
    [commandTemplates, selectedCommandId]
  )

  const toggleFolder = (folderId: string) => {
    setExpandedFolders((prev) => {
      const next = new Set(prev)
      if (next.has(folderId)) {
        next.delete(folderId)
      } else {
        next.add(folderId)
      }
      return next
    })
  }

  const editorInitialValue = editorState?.mode === 'edit'
    ? (() => {
        const command = commandTemplates.find((item) => item.id === editorState.commandId)
        if (!command) {
          return emptyCommandForm
        }
        return {
          name: command.name,
          command: command.command,
          description: command.description ?? '',
          parentId: command.parentId,
          order: command.order,
          appendCarriageReturn: command.appendCarriageReturn
        }
      })()
    : {
        ...emptyCommandForm,
        parentId: selectedFolderId === 'all' || selectedFolderId === 'ungrouped' ? undefined : selectedFolderId
      }

  const handleDragStart = (event: DragEvent<HTMLDivElement>, folderId: string) => {
    event.stopPropagation()
    setDraggingFolderId(folderId)
    event.dataTransfer.effectAllowed = 'move'
    event.dataTransfer.setData('text/plain', folderId)
  }

  const handleDragOver = (event: DragEvent<HTMLDivElement>, targetId: string) => {
    event.preventDefault()
    event.stopPropagation()
    if (draggingFolderId === targetId) {
      return
    }

    const rect = event.currentTarget.getBoundingClientRect()
    const y = event.clientY - rect.top
    const height = rect.height
    let nextPosition: 'top' | 'bottom' | 'inside' = 'bottom'
    if (y < height * 0.25) {
      nextPosition = 'top'
    } else if (y > height * 0.75) {
      nextPosition = 'bottom'
    } else {
      nextPosition = 'inside'
    }

    setDragOverFolderId(targetId)
    setDragPosition(nextPosition)
  }

  const handleDrop = (event: DragEvent<HTMLDivElement>, targetId: string) => {
    event.preventDefault()
    event.stopPropagation()

    if (!draggingFolderId || draggingFolderId === targetId) {
      setDraggingFolderId(null)
      setDragOverFolderId(null)
      setDragPosition(null)
      return
    }

    const draggedNode = folderTree.map.get(draggingFolderId)
    const targetNode = folderTree.map.get(targetId)
    if (!draggedNode || !targetNode) {
      return
    }

    let current: FolderNode | undefined = targetNode
    while (current?.parentId) {
      if (current.parentId === draggingFolderId) {
        setDraggingFolderId(null)
        setDragOverFolderId(null)
        setDragPosition(null)
        return
      }
      current = folderTree.map.get(current.parentId)
    }

    let newParentId = targetNode.parentId
    let siblings = newParentId ? folderTree.map.get(newParentId)?.children ?? [] : folderTree.roots
    let newOrder = targetNode.order ?? 0

    if (dragPosition === 'inside') {
      newParentId = targetNode.id
      const children = targetNode.children
      newOrder = children.length ? (children[children.length - 1].order ?? 0) + 1000 : 1000
      setExpandedFolders((prev) => new Set(prev).add(targetNode.id))
    } else {
      const targetIndex = siblings.findIndex((item) => item.id === targetId)
      if (dragPosition === 'top') {
        const prev = siblings[targetIndex - 1]
        newOrder = prev ? ((prev.order ?? 0) + (targetNode.order ?? 0)) / 2 : (targetNode.order ?? 0) - 1000
      } else {
        const next = siblings[targetIndex + 1]
        newOrder = next ? ((next.order ?? 0) + (targetNode.order ?? 0)) / 2 : (targetNode.order ?? 0) + 1000
      }
    }

    onUpdateOrder(draggingFolderId, newParentId, newOrder)
    setDraggingFolderId(null)
    setDragOverFolderId(null)
    setDragPosition(null)
  }

  const renderFolderNode = (node: FolderNode, depth: number) => {
    const isExpanded = expandedFolders.has(node.id)
    const isEditing = editingFolderId === node.id
    const isDragOver = dragOverFolderId === node.id
    const dropClass = isDragOver && dragPosition ? `drop-${dragPosition}` : ''

    return (
      <div key={node.id}>
        <div
          className={`command-folder-manager-row ${dropClass} ${draggingFolderId === node.id ? 'dragging' : ''} ${selectedFolderId === node.id ? 'active' : ''}`}
          draggable
          onClick={() => setSelectedFolderId(node.id)}
          onDoubleClick={() => toggleFolder(node.id)}
          onDragStart={(event) => handleDragStart(event, node.id)}
          onDragOver={(event) => handleDragOver(event, node.id)}
          onDrop={(event) => handleDrop(event, node.id)}
          onDragEnd={() => {
            setDraggingFolderId(null)
            setDragOverFolderId(null)
            setDragPosition(null)
          }}
        >
          <div className="command-folder-manager-main" style={{ paddingLeft: `${depth * 18}px` }}>
            <button
              className="command-folder-toggle"
              type="button"
              onClick={(event) => {
                event.stopPropagation()
                toggleFolder(node.id)
              }}
            >
              {isExpanded ? '▾' : '▸'}
            </button>
            {isEditing ? (
              <input
                autoFocus
                value={editingFolderName}
                onChange={(event) => setEditingFolderName(event.currentTarget.value)}
                onBlur={() => {
                  const nextName = editingFolderName.trim()
                  if (nextName && nextName !== node.name) {
                    onUpdateFolder(node.id, { name: nextName })
                  }
                  setEditingFolderId(null)
                }}
                onKeyDown={(event) => {
                  if (event.key === 'Enter') {
                    const nextName = editingFolderName.trim()
                    if (nextName && nextName !== node.name) {
                      onUpdateFolder(node.id, { name: nextName })
                    }
                    setEditingFolderId(null)
                  }
                  if (event.key === 'Escape') {
                    setEditingFolderId(null)
                  }
                }}
              />
            ) : (
              <strong>{node.name}</strong>
            )}
          </div>
          <div className="command-folder-manager-actions">
            <button
              className="flat-button compact"
              type="button"
              onClick={(event) => {
                event.stopPropagation()
                setEditingFolderId(node.id)
                setEditingFolderName(node.name)
              }}
            >
              {t.edit}
            </button>
            <button
              className="flat-button compact danger"
              type="button"
              onClick={(event) => {
                event.stopPropagation()
                onDeleteFolder(node.id)
              }}
            >
              {t.delete}
            </button>
          </div>
        </div>
        {isExpanded ? node.children.map((child) => renderFolderNode(child, depth + 1)) : null}
      </div>
    )
  }

  const shell = (
    <div className={`modal-card manager-modal command-manager-modal ${standalone ? 'standalone' : ''}`}>
      <div className="modal-header">
        <span>{t.commandManager}</span>
        {!standalone ? <button className="icon-button" onClick={onClose} type="button">×</button> : null}
      </div>
      <div className="manager-toolbar">
        <button className="flat-button" type="button" onClick={() => setEditorState({ mode: 'create' })}>{t.newCommand}</button>
      </div>
      <div className="command-manager-panel-grid">
        <section className="command-manager-panel">
          <div className="manager-head command-manager-panel-head">
            <span>{t.commandCategory}</span>
            <span>{t.actions}</span>
          </div>
          <div className="command-manager-folder-create">
            <input
              placeholder={t.folderName}
              type="text"
              value={newFolderName}
              onChange={(event) => setNewFolderName(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') {
                  const nextName = newFolderName.trim()
                  if (!nextName) {
                    return
                  }
                  onCreateFolder(nextName)
                  setNewFolderName('')
                }
              }}
            />
            <button
              className="flat-button compact"
              type="button"
              onClick={() => {
                const nextName = newFolderName.trim()
                if (!nextName) {
                  return
                }
                onCreateFolder(nextName)
                setNewFolderName('')
              }}
            >
              {t.newFolder}
            </button>
          </div>
          <div className="command-folder-manager-list">
            <div
              className={`command-folder-manager-row root-row ${selectedFolderId === 'all' ? 'active' : ''}`}
              onClick={() => setSelectedFolderId('all')}
            >
              <div className="command-folder-manager-main">
                <strong>{t.all}</strong>
              </div>
            </div>
            <div
              className={`command-folder-manager-row root-row ${selectedFolderId === 'ungrouped' ? 'active' : ''}`}
              onClick={() => setSelectedFolderId('ungrouped')}
            >
              <div className="command-folder-manager-main">
                <strong>{t.commandUncategorized}</strong>
              </div>
            </div>
            {folderTree.roots.map((node) => renderFolderNode(node, 0))}
          </div>
        </section>

        <section className="command-manager-panel">
          <div className="manager-head command-manager-panel-head">
            <span>{t.commandList}</span>
            <span>{filteredCommands.length}</span>
          </div>
          <div className="command-manager-list-shell">
            <table className="command-table">
              <thead>
                <tr>
                  <th className="col-name">{t.name}</th>
                  <th className="col-template">{t.commandTemplate}</th>
                  <th>{t.actions}</th>
                </tr>
              </thead>
              <tbody>
                {filteredCommands.map((item) => (
                  <tr
                    key={item.id}
                    className={selectedCommandId === item.id ? 'active' : ''}
                    onClick={() => setSelectedCommandId(item.id)}
                  >
                    <td className="col-name">
                      <strong>{item.name}</strong>
                    </td>
                    <td className="col-template">
                      <code>{item.command}</code>
                    </td>
                    <td className="command-manager-actions-cell">
                      <button
                        className="flat-button compact"
                        type="button"
                        onClick={(event) => {
                          event.stopPropagation()
                          setSelectedCommandId(item.id)
                          setEditorState({ mode: 'edit', commandId: item.id })
                        }}
                      >
                        {t.edit}
                      </button>
                      <button
                        className="flat-button compact danger"
                        type="button"
                        onClick={(event) => {
                          event.stopPropagation()
                          onDeleteCommand(item.id)
                        }}
                      >
                        {t.delete}
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {!filteredCommands.length ? <div className="command-empty-state">{t.commandEmpty}</div> : null}
          </div>
        </section>
      </div>
    </div>
  )

  return (
    <>
      {standalone ? <div className="modal-shell standalone-shell">{shell}</div> : <div className="modal-shell">{shell}</div>}
      {editorState ? (
        <CommandEditorPage
          folders={folders}
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
    </>
  )
}
