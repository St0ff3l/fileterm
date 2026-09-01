import { useCallback, useEffect, useMemo, useRef, useState, type DragEvent } from 'react'
import type { SshKeyImportSource, SshKeyMetadata } from '@fileterm/core'
import { ConfirmActionDialog } from '../common/confirm-action-dialog'
import { managerDropClass, resolveManagerDropPosition } from '../common/manager-drag'
import { useSshKeyLibrary } from '../../hooks/use-ssh-key-library'
import { SshKeyNoteDialog } from './ssh-key-note-dialog'
import { formatMessage, t } from '../../i18n'
import { usePointerSortFallback, type PointerSortTarget } from '../../hooks/use-pointer-sort-fallback'
import { SshKeyManagerMain } from './ssh-key-manager-main'
import { SshKeyRow } from './ssh-key-row'
import {
  ROOT_DROP_TARGET_ID,
  SSH_KEY_MANAGER_UI_STATE,
  createId,
  isSshKeyFolder,
  readDraggedItem,
  type DeleteTarget,
  type DragItem,
  type DragPosition,
  type SshKeyFolder,
  type SshKeyManagerUiState,
  type SortableItem
} from './ssh-key-manager-utils'

export function SshKeyManagerPage({
  onActiveFolderChange,
  onStatsChange
}: {
  onActiveFolderChange?(name: string): void
  onStatsChange?(stats: { keyCount: number; folderCount: number }): void
}) {
  const desktopApi = window.fileterm
  const { keys, loading, error, clearError, selectKeyFile, importKey, updateNote, deleteKey } = useSshKeyLibrary()
  const [query, setQuery] = useState('')
  const [busy, setBusy] = useState(false)
  const [uiStateError, setUiStateError] = useState<string | null>(null)
  const [noteDialog, setNoteDialog] = useState<
    { mode: 'import' } | { mode: 'edit'; keyId: string; initialNote: string } | null
  >(null)
  const [folders, setFolders] = useState<SshKeyFolder[]>([])
  const [assignments, setAssignments] = useState<Record<string, string>>({})
  const [itemOrder, setItemOrder] = useState<Record<string, number>>({})
  const [activeFolderId, setActiveFolderId] = useState<'all' | string>('all')
  const [expandedFolderIds, setExpandedFolderIds] = useState<Set<string>>(new Set())
  const [editingFolder, setEditingFolder] = useState<{ id: string; name: string } | null>(null)
  const [pendingDelete, setPendingDelete] = useState<DeleteTarget | null>(null)
  const [deleteError, setDeleteError] = useState<string | null>(null)
  const [dragging, setDragging] = useState<DragItem | null>(null)
  const [dragOver, setDragOver] = useState<{ id: string; kind: DragItem['kind']; position: DragPosition } | null>(null)
  const dragStateRef = useRef<{
    dragging: DragItem | null
    dragOver: { id: string; kind: DragItem['kind']; position: DragPosition } | null
  }>({ dragging: null, dragOver: null })
  const [isActionsExpanded, setIsActionsExpanded] = useState(false)
  const [isCreatingFolder, setIsCreatingFolder] = useState(false)
  const [newFolderName, setNewFolderName] = useState('')
  const uiStateRevisionRef = useRef(0)
  const busyRef = useRef(false)

  useEffect(() => {
    let disposed = false
    const revisionAtStart = uiStateRevisionRef.current
    void desktopApi
      ?.getUiStateItem(SSH_KEY_MANAGER_UI_STATE)
      .then((raw) => {
        if (disposed || !raw || revisionAtStart !== uiStateRevisionRef.current) return
        const parsed = JSON.parse(raw) as Partial<SshKeyManagerUiState>
        const nextFolders = Array.isArray(parsed.folders) ? parsed.folders.filter(isSshKeyFolder) : []
        const nextItemOrder = parsed.itemOrder ?? parsed.keyOrder ?? {}
        setFolders(nextFolders)
        setAssignments(parsed.assignments && typeof parsed.assignments === 'object' ? parsed.assignments : {})
        setItemOrder(nextItemOrder && typeof nextItemOrder === 'object' ? nextItemOrder : {})
        setUiStateError(null)
      })
      .catch((cause: unknown) => {
        if (!disposed) setUiStateError(cause instanceof Error ? cause.message : String(cause))
      })
    return () => {
      disposed = true
    }
  }, [desktopApi])

  const persistUiState = useCallback(
    async (nextFolders: SshKeyFolder[], nextAssignments: Record<string, string>, nextItemOrder = itemOrder) => {
      uiStateRevisionRef.current += 1
      try {
        await desktopApi?.setUiStateItem(
          SSH_KEY_MANAGER_UI_STATE,
          JSON.stringify({
            folders: nextFolders,
            assignments: nextAssignments,
            itemOrder: nextItemOrder
          } satisfies SshKeyManagerUiState)
        )
        setFolders(nextFolders)
        setAssignments(nextAssignments)
        setItemOrder(nextItemOrder)
        setUiStateError(null)
        return true
      } catch (cause) {
        setUiStateError(cause instanceof Error ? cause.message : String(cause))
        return false
      }
    },
    [desktopApi, itemOrder]
  )

  const orderOf = useCallback((id: string, fallbackOrder: number) => itemOrder[id] ?? fallbackOrder, [itemOrder])

  const folderKeyCount = useCallback(
    (folderId: string) => keys.filter((key) => assignments[key.id] === folderId).length,
    [assignments, keys]
  )

  const activeFolder = folders.find((folder) => folder.id === activeFolderId)
  const selectedKeys = useMemo(
    () => (activeFolderId === 'all' ? keys : keys.filter((key) => assignments[key.id] === activeFolderId)),
    [activeFolderId, assignments, keys]
  )
  const visibleKeys = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase()
    const filtered = !normalized
      ? selectedKeys
      : selectedKeys.filter((key) =>
          [key.name, key.note, key.algorithm, key.fingerprint].some((value) =>
            value?.toLocaleLowerCase().includes(normalized)
          )
        )
    return [...filtered].sort((left, right) => {
      const leftOrder = orderOf(left.id, left.importedAt)
      const rightOrder = orderOf(right.id, right.importedAt)
      return leftOrder - rightOrder
    })
  }, [orderOf, query, selectedKeys])
  const orderedFolders = useMemo(
    () =>
      [...folders].sort((left, right) => {
        const leftIndex = folders.findIndex((folder) => folder.id === left.id)
        const rightIndex = folders.findIndex((folder) => folder.id === right.id)
        return orderOf(left.id, (leftIndex + 1) * 1000) - orderOf(right.id, (rightIndex + 1) * 1000)
      }),
    [folders, orderOf]
  )
  const visibleFolders = useMemo(() => {
    if (activeFolderId !== 'all') return []
    const normalized = query.trim().toLocaleLowerCase()
    return normalized
      ? orderedFolders.filter((folder) => folder.name.toLocaleLowerCase().includes(normalized))
      : orderedFolders
  }, [activeFolderId, orderedFolders, query])
  const rootItems = useMemo<SortableItem[]>(() => {
    const rootKeys = visibleKeys.filter((key) => !folders.some((folder) => assignments[key.id] === folder.id))
    return [
      ...visibleFolders.map((folder, index) => ({
        kind: 'folder' as const,
        id: folder.id,
        fallbackOrder: (index + 1) * 1000
      })),
      ...rootKeys.map((key) => ({ kind: 'key' as const, id: key.id, fallbackOrder: key.importedAt }))
    ].sort((left, right) => orderOf(left.id, left.fallbackOrder) - orderOf(right.id, right.fallbackOrder))
  }, [assignments, folders, orderOf, visibleFolders, visibleKeys])
  const hasVisibleRows =
    visibleFolders.length > 0 || visibleKeys.length > 0 || (isCreatingFolder && activeFolderId === 'all')
  const suppressRowClickRef = useRef(false)

  const toggleFolder = (folderId: string) => {
    setExpandedFolderIds((current) => {
      const next = new Set(current)
      if (next.has(folderId)) next.delete(folderId)
      else next.add(folderId)
      return next
    })
  }

  useEffect(() => {
    onActiveFolderChange?.(activeFolder?.name ?? t.allKeys)
  }, [activeFolder?.name, onActiveFolderChange])

  useEffect(() => {
    onStatsChange?.({ keyCount: keys.length, folderCount: folders.length })
  }, [folders.length, keys.length, onStatsChange])

  const finishFolderCreation = async (name: string) => {
    if (folders.some((folder) => folder.name === name)) return false

    const folder = { id: createId('ssh-folder'), name }
    const rootOrders = [
      ...folders.map((item, index) => orderOf(item.id, (index + 1) * 1000)),
      ...keys
        .filter((key) => !folders.some((item) => assignments[key.id] === item.id))
        .map((key) => orderOf(key.id, key.importedAt))
    ]
    return persistUiState([...folders, folder], assignments, {
      ...itemOrder,
      [folder.id]: Math.max(0, ...rootOrders) + 1000
    })
  }

  const saveFolderRename = async () => {
    if (!editingFolder) return
    const name = editingFolder.name.trim()
    const current = folders.find((folder) => folder.id === editingFolder.id)
    if (
      !name ||
      name === current?.name ||
      folders.some((folder) => folder.id !== editingFolder.id && folder.name === name)
    ) {
      setEditingFolder(null)
      return
    }
    if (busyRef.current) return

    busyRef.current = true
    setBusy(true)
    try {
      const persisted = await persistUiState(
        folders.map((folder) => (folder.id === editingFolder.id ? { ...folder, name } : folder)),
        assignments
      )
      if (persisted) setEditingFolder(null)
    } finally {
      busyRef.current = false
      setBusy(false)
    }
  }

  const requestDelete = (kind: DeleteTarget['kind'], id: string, name: string) => {
    clearError()
    setDeleteError(null)
    setPendingDelete({ kind, id, name })
  }

  const confirmDelete = async () => {
    if (!pendingDelete || busyRef.current) return
    const target = pendingDelete
    busyRef.current = true
    setBusy(true)
    setDeleteError(null)
    try {
      if (target.kind === 'key') {
        await deleteKey(target.id)
        if (assignments[target.id]) {
          const nextAssignments = { ...assignments }
          delete nextAssignments[target.id]
          await persistUiState(folders, nextAssignments)
        }
      } else {
        const nextAssignments = { ...assignments }
        Object.keys(nextAssignments).forEach((keyId) => {
          if (nextAssignments[keyId] === target.id) delete nextAssignments[keyId]
        })
        const persisted = await persistUiState(
          folders.filter((folder) => folder.id !== target.id),
          nextAssignments
        )
        if (!persisted) return
        setExpandedFolderIds((current) => {
          const next = new Set(current)
          next.delete(target.id)
          return next
        })
        if (activeFolderId === target.id) setActiveFolderId('all')
      }
      setPendingDelete(null)
    } catch (cause) {
      setDeleteError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      busyRef.current = false
      setBusy(false)
    }
  }

  const handleDragStart = (event: DragEvent, item: DragItem) => {
    event.stopPropagation()
    suppressRowClickRef.current = true
    dragStateRef.current = { dragging: item, dragOver: null }
    setDragging(item)
    event.dataTransfer.effectAllowed = 'move'
    event.dataTransfer.setData('text/plain', `fileterm-ssh-key:${item.kind}:${item.id}`)
  }

  const handleDragOver = (event: DragEvent, target: DragItem) => {
    event.preventDefault()
    event.stopPropagation()
    const activeDragging = dragStateRef.current.dragging
    if (!activeDragging || activeDragging.id === target.id) return

    const position = positionForTarget(activeDragging, target, event.currentTarget as HTMLElement, event.clientY)
    const nextDragOver = { ...target, position }
    dragStateRef.current.dragOver = nextDragOver
    setDragOver(nextDragOver)
  }

  const handleRootDragOver = (event: DragEvent) => {
    event.preventDefault()
    event.stopPropagation()
    if (dragStateRef.current.dragging?.kind === 'key') {
      const nextDragOver = { id: ROOT_DROP_TARGET_ID, kind: 'folder' as const, position: 'inside' as const }
      dragStateRef.current.dragOver = nextDragOver
      setDragOver(nextDragOver)
    }
  }

  const handleRootDragLeave = (event: DragEvent) => {
    event.preventDefault()
    event.stopPropagation()
    if (dragOver?.id === ROOT_DROP_TARGET_ID) setDragOver(null)
  }

  const clearDragState = () => {
    dragStateRef.current = { dragging: null, dragOver: null }
    setDragging(null)
    setDragOver(null)
    // Pointer-sort does not emit the native `dragend` event. Keep the click
    // generated by this pointer-up suppressed, then restore normal folder
    // expand/collapse clicks on the next event-loop turn.
    window.setTimeout(() => {
      suppressRowClickRef.current = false
    }, 0)
  }

  const sortableItemsForParent = (parentId?: string): SortableItem[] => {
    if (parentId) {
      return keys
        .filter((key) => assignments[key.id] === parentId)
        .map((key) => ({ kind: 'key' as const, id: key.id, fallbackOrder: key.importedAt }))
    }
    return [
      ...folders.map((folder, index) => ({
        kind: 'folder' as const,
        id: folder.id,
        fallbackOrder: (index + 1) * 1000
      })),
      ...keys
        .filter((key) => !folders.some((folder) => assignments[key.id] === folder.id))
        .map((key) => ({ kind: 'key' as const, id: key.id, fallbackOrder: key.importedAt }))
    ]
  }

  const reorderItems = (
    dragItem: DragItem,
    targetItem: DragItem,
    parentId: string | undefined,
    position: DragPosition
  ) => {
    const siblings = sortableItemsForParent(parentId).sort(
      (left, right) => orderOf(left.id, left.fallbackOrder) - orderOf(right.id, right.fallbackOrder)
    )
    const sourceIndex = siblings.findIndex((item) => item.id === dragItem.id && item.kind === dragItem.kind)
    const targetIndex = siblings.findIndex((item) => item.id === targetItem.id && item.kind === targetItem.kind)
    if (targetIndex < 0) return

    const source =
      sourceIndex >= 0
        ? siblings.splice(sourceIndex, 1)[0]
        : {
            kind: dragItem.kind,
            id: dragItem.id,
            fallbackOrder: keys.find((key) => key.id === dragItem.id)?.importedAt ?? Date.now()
          }
    const nextTargetIndex = siblings.findIndex((item) => item.id === targetItem.id && item.kind === targetItem.kind)
    if (!source || nextTargetIndex < 0) return
    siblings.splice(nextTargetIndex + (position === 'bottom' ? 1 : 0), 0, source)

    const nextAssignments = { ...assignments }
    if (dragItem.kind === 'key') {
      if (parentId) nextAssignments[dragItem.id] = parentId
      else delete nextAssignments[dragItem.id]
    }
    const nextItemOrder = { ...itemOrder }
    siblings.forEach((item, index) => {
      nextItemOrder[item.id] = (index + 1) * 1000
    })
    persistUiState(folders, nextAssignments, nextItemOrder)
  }

  const moveKeyToRoot = (keyId: string) => {
    const rootItems = sortableItemsForParent()
      .filter((item) => item.id !== keyId)
      .sort((left, right) => orderOf(left.id, left.fallbackOrder) - orderOf(right.id, right.fallbackOrder))
    const nextAssignments = { ...assignments }
    delete nextAssignments[keyId]
    const nextItemOrder = { ...itemOrder }
    rootItems.forEach((item, index) => {
      nextItemOrder[item.id] = (index + 1) * 1000
    })
    nextItemOrder[keyId] = (rootItems.length + 1) * 1000
    persistUiState(folders, nextAssignments, nextItemOrder)
  }

  const positionForTarget = (
    dragItem: DragItem,
    target: DragItem,
    element: HTMLElement,
    clientY: number
  ): DragPosition => {
    if (element.closest('.connection-manager-sidebar')) return 'inside'
    return resolveManagerDropPosition(element, clientY, target.kind === 'folder' && dragItem.kind === 'key')
  }

  const applyDrop = (activeDragging: DragItem, target: DragItem, position: DragPosition) => {
    if (activeDragging.id === target.id) return
    if (activeDragging.kind === 'key') {
      const draggedKey = keys.find((key) => key.id === activeDragging.id)
      if (draggedKey && target.kind === 'folder' && position === 'inside') {
        const siblingOrders = keys
          .filter((key) => key.id !== activeDragging.id && assignments[key.id] === target.id)
          .map((key) => orderOf(key.id, key.importedAt))
        const nextAssignments = { ...assignments, [activeDragging.id]: target.id }
        const nextItemOrder = { ...itemOrder, [activeDragging.id]: Math.max(0, ...siblingOrders, 0) + 1000 }
        persistUiState(folders, nextAssignments, nextItemOrder)
        setExpandedFolderIds((current) => new Set(current).add(target.id))
      } else if (draggedKey) {
        const parentId = target.kind === 'key' ? assignments[target.id] : undefined
        reorderItems(activeDragging, target, parentId, position)
      }
    } else if (activeDragging.kind === 'folder' && position !== 'inside') {
      const targetParentId = target.kind === 'key' ? assignments[target.id] : undefined
      if (!targetParentId) reorderItems(activeDragging, target, undefined, position)
    }
  }

  const handlePointerDown = usePointerSortFallback<DragItem>({
    onStart: (item) => {
      suppressRowClickRef.current = true
      dragStateRef.current = { dragging: item, dragOver: null }
      setDragging(item)
    },
    onTarget: (item, target: PointerSortTarget, clientY) => {
      if (target.id === ROOT_DROP_TARGET_ID) {
        if (item.kind === 'key') {
          const rootTarget = { id: ROOT_DROP_TARGET_ID, kind: 'folder' as const, position: 'inside' as const }
          dragStateRef.current.dragOver = rootTarget
          setDragOver(rootTarget)
        }
        return
      }
      if (target.kind !== 'folder' && target.kind !== 'key') return
      const targetItem: DragItem = { id: target.id, kind: target.kind }
      if (item.id === targetItem.id) return
      const position = positionForTarget(item, targetItem, target.element, clientY)
      const nextDragOver = { ...targetItem, position }
      dragStateRef.current.dragOver = nextDragOver
      setDragOver(nextDragOver)
    },
    onDrop: (item, target, clientY) => {
      if (target?.id === ROOT_DROP_TARGET_ID) {
        if (item.kind === 'key') moveKeyToRoot(item.id)
      } else if (target && (target.kind === 'folder' || target.kind === 'key')) {
        const targetItem: DragItem = { id: target.id, kind: target.kind }
        const position = positionForTarget(item, targetItem, target.element, clientY)
        applyDrop(item, targetItem, position)
      }
      clearDragState()
    },
    onCancel: clearDragState
  })

  const handleRootDrop = (event: DragEvent) => {
    event.preventDefault()
    event.stopPropagation()
    const activeDragging = dragStateRef.current.dragging ?? readDraggedItem(event.dataTransfer.getData('text/plain'))
    if (activeDragging?.kind === 'key') moveKeyToRoot(activeDragging.id)
    clearDragState()
  }

  const handleDrop = (event: DragEvent, target: DragItem) => {
    event.preventDefault()
    event.stopPropagation()
    const activeDragging = dragStateRef.current.dragging ?? readDraggedItem(event.dataTransfer.getData('text/plain'))
    if (!activeDragging || activeDragging.id === target.id) {
      clearDragState()
      return
    }

    let activeDragOver = dragStateRef.current.dragOver
    if (!activeDragOver || activeDragOver.id !== target.id) {
      const rect = (event.currentTarget as HTMLElement).getBoundingClientRect()
      const y = event.clientY - rect.top
      const position: DragPosition =
        target.kind === 'folder' && activeDragging.kind === 'key' && y >= rect.height * 0.25 && y <= rect.height * 0.75
          ? 'inside'
          : y < rect.height * 0.5
            ? 'top'
            : 'bottom'
      activeDragOver = { ...target, position }
    }

    applyDrop(activeDragging, target, activeDragOver.position)
    clearDragState()
  }

  const handleDragEnd = () => {
    clearDragState()
  }

  const handleImport = async (note: string, source: SshKeyImportSource, folderId?: string) => {
    if ((!source.sourcePath && !source.content) || busyRef.current) return
    busyRef.current = true
    setBusy(true)
    try {
      const result = await importKey(note, source)
      if (result) {
        const nextAssignments = { ...assignments }
        if (folderId) nextAssignments[result.key.id] = folderId
        else delete nextAssignments[result.key.id]
        await persistUiState(folders, nextAssignments)
      }
      setNoteDialog(null)
    } catch {
      // useSshKeyLibrary 已将可展示错误写入 error 状态。
    } finally {
      busyRef.current = false
      setBusy(false)
    }
  }

  const handleEditNote = async (keyId: string, note: string, folderId?: string) => {
    if (busyRef.current) return
    busyRef.current = true
    setBusy(true)
    try {
      await updateNote(keyId, note)
      const nextAssignments = { ...assignments }
      if (folderId) nextAssignments[keyId] = folderId
      else delete nextAssignments[keyId]
      await persistUiState(folders, nextAssignments)
      setNoteDialog(null)
    } catch {
      // useSshKeyLibrary 已将可展示错误写入 error 状态。
    } finally {
      busyRef.current = false
      setBusy(false)
    }
  }

  const handleDelete = (keyId: string, name: string) => {
    requestDelete('key', keyId, name)
  }

  const folderForKey = (keyId: string) => assignments[keyId] ?? ''

  const isFolderDragOver = (folderId: string) => {
    return managerDropClass(dragOver?.id === folderId, dragOver?.position ?? null)
  }

  const isKeyDragOver = (keyId: string) => {
    return managerDropClass(dragOver?.id === keyId, dragOver?.position ?? null)
  }

  const openNewKeyDialog = () => {
    clearError()
    setNoteDialog({ mode: 'import' })
    setIsActionsExpanded(false)
  }

  const renderKeyRow = (key: SshKeyMetadata, className = '') => (
    <SshKeyRow
      key={key.id}
      className={`${className} ${isKeyDragOver(key.id)}`.trim()}
      draggable={false}
      onPointerDown={(event) => handlePointerDown(event, { kind: 'key', id: key.id })}
      item={key}
      onDragStart={(event) => handleDragStart(event, { kind: 'key', id: key.id })}
      onDragOver={(event) => handleDragOver(event, { kind: 'key', id: key.id })}
      onDragLeave={(event) => {
        event.preventDefault()
        setDragOver(null)
      }}
      onDrop={(event) => handleDrop(event, { kind: 'key', id: key.id })}
      onDragEnd={handleDragEnd}
      onDelete={() => handleDelete(key.id, key.name)}
      onEdit={() => {
        clearError()
        setNoteDialog({ mode: 'edit', keyId: key.id, initialNote: key.note ?? '' })
      }}
    />
  )

  return (
    <section className="ssh-key-manager-page manager-inline connection-manager-modal">
      <SshKeyManagerMain
        query={query}
        onQueryChange={setQuery}
        activeFolderId={activeFolderId}
        onActiveFolderChange={(folderId) => setActiveFolderId(folderId)}
        dragOver={dragOver}
        keys={keys}
        orderedFolders={orderedFolders}
        folderKeyCount={folderKeyCount}
        rootItems={rootItems}
        visibleKeys={visibleKeys}
        folders={folders}
        assignments={assignments}
        expandedFolderIds={expandedFolderIds}
        dragging={dragging}
        editingFolder={editingFolder}
        busy={busy}
        isCreatingFolder={isCreatingFolder}
        newFolderName={newFolderName}
        errorMessage={noteDialog ? null : error || uiStateError}
        loading={loading}
        hasVisibleRows={hasVisibleRows}
        isActionsExpanded={isActionsExpanded}
        onRootDragOver={handleRootDragOver}
        onRootDragLeave={handleRootDragLeave}
        onRootDrop={handleRootDrop}
        onFolderPointerDown={(event, folderId) => handlePointerDown(event, { kind: 'folder', id: folderId })}
        onFolderDragStart={(event, folderId) => handleDragStart(event, { kind: 'folder', id: folderId })}
        onFolderDragOver={(event, folderId) => handleDragOver(event, { kind: 'folder', id: folderId })}
        onFolderDrop={(event, folderId) => handleDrop(event, { kind: 'folder', id: folderId })}
        onFolderDragEnd={handleDragEnd}
        onFolderToggle={(folderId) => {
          if (suppressRowClickRef.current) return
          toggleFolder(folderId)
        }}
        onFolderNameChange={(folderId, name) => setEditingFolder({ id: folderId, name })}
        onCancelFolderRename={() => setEditingFolder(null)}
        onFolderRename={() => void saveFolderRename()}
        onFolderDelete={(folder) => requestDelete('folder', folder.id, folder.name)}
        onKeyDragStart={(event, keyId) => handleDragStart(event, { kind: 'key', id: keyId })}
        onKeyDragOver={(event, keyId) => handleDragOver(event, { kind: 'key', id: keyId })}
        onKeyDragLeave={(event) => {
          event.preventDefault()
          setDragOver(null)
        }}
        onKeyDrop={(event, keyId) => handleDrop(event, { kind: 'key', id: keyId })}
        onKeyDragEnd={handleDragEnd}
        onKeyPointerDown={(event, keyId) => handlePointerDown(event, { kind: 'key', id: keyId })}
        onSetDragOver={(value) => setDragOver(value)}
        onCreateFolder={finishFolderCreation}
        onNewFolderNameChange={setNewFolderName}
        onDismissFolderCreation={() => {
          setIsCreatingFolder(false)
          setNewFolderName('')
        }}
        onCreateFolderAction={() => {
          setIsCreatingFolder(true)
          setNewFolderName('')
          setActiveFolderId('all')
          setIsActionsExpanded(false)
        }}
        onOpenNewKeyDialog={openNewKeyDialog}
        onActionsExpandedChange={() => setIsActionsExpanded((expanded) => !expanded)}
        folderDragClass={isFolderDragOver}
        renderKeyRow={renderKeyRow}
      />

      {noteDialog ? (
        <SshKeyNoteDialog
          errorMessage={error || uiStateError}
          folders={folders}
          initialFolderId={
            noteDialog.mode === 'edit' ? folderForKey(noteDialog.keyId) : activeFolderId === 'all' ? '' : activeFolderId
          }
          initialNote={noteDialog.mode === 'edit' ? noteDialog.initialNote : ''}
          isSubmitting={busy}
          mode={noteDialog.mode}
          onClose={() => {
            if (!busy) setNoteDialog(null)
          }}
          onSelectFile={selectKeyFile}
          onSubmit={(note, source, folderId) => {
            if (noteDialog.mode === 'import') {
              void handleImport(note, source, folderId)
              return
            }
            void handleEditNote(noteDialog.keyId, note, folderId)
          }}
        />
      ) : null}
      {pendingDelete ? (
        <ConfirmActionDialog
          confirmLabel={t.delete}
          description={
            pendingDelete.kind === 'folder'
              ? `${t.deleteConfirmPrefix}${pendingDelete.name}${t.deleteConfirmSuffix}`
              : formatMessage(t.deleteKeyDescription, { name: pendingDelete.name })
          }
          isSubmitting={busy}
          errorMessage={deleteError || error || uiStateError}
          onClose={() => {
            if (!busyRef.current) {
              setDeleteError(null)
              setPendingDelete(null)
            }
          }}
          onConfirm={() => void confirmDelete()}
          title={t.delete}
        />
      ) : null}
    </section>
  )
}
