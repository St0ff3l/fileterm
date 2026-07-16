import {
  Fragment,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type DragEvent,
  type PointerEvent as ReactPointerEvent
} from 'react'
import type { SshKeyMetadata } from '@fileterm/core'
import { AppIcon } from '../common/AppIcon'
import { ConfirmActionDialog } from '../common/ConfirmActionDialog'
import { ManagerInlineFolderRow } from '../common/ManagerInlineFolderRow'
import { managerDropClass, resolveManagerDropPosition, type ManagerDropPosition } from '../common/manager-drag'
import { useSshKeyLibrary } from '../../hooks/useSshKeyLibrary'
import { SshKeyNoteDialog } from './SshKeyNoteDialog'
import { t } from '../../i18n'
import { usePointerSortFallback, type PointerSortTarget } from '../../hooks/usePointerSortFallback'

const SSH_KEY_MANAGER_UI_STATE = 'ssh-key-manager-ui'

type SshKeyFolder = {
  id: string
  name: string
}

type SshKeyManagerUiState = {
  folders: SshKeyFolder[]
  assignments: Record<string, string>
  itemOrder?: Record<string, number>
  keyOrder?: Record<string, number>
}

type DeleteTarget = { kind: 'folder' | 'key'; id: string; name: string }
type DragItem = { kind: 'folder' | 'key'; id: string }
type DragPosition = ManagerDropPosition
type SortableItem = { kind: DragItem['kind']; id: string; fallbackOrder: number }

const ROOT_DROP_TARGET_ID = '__ssh-key-root__'

function readDraggedItem(value: string): DragItem | null {
  const match = /^fileterm-ssh-key:(folder|key):(.+)$/.exec(value)
  return match ? { kind: match[1] as DragItem['kind'], id: match[2] } : null
}

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
    onActiveFolderChange?.(activeFolder?.name ?? '全部密钥')
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

  const saveFolderRename = () => {
    if (!editingFolder) return
    const name = editingFolder.name.trim()
    const current = folders.find((folder) => folder.id === editingFolder.id)
    if (
      name &&
      name !== current?.name &&
      !folders.some((folder) => folder.id !== editingFolder.id && folder.name === name)
    ) {
      persistUiState(
        folders.map((folder) => (folder.id === editingFolder.id ? { ...folder, name } : folder)),
        assignments
      )
    }
    setEditingFolder(null)
  }

  const requestDelete = (kind: DeleteTarget['kind'], id: string, name: string) => {
    setPendingDelete({ kind, id, name })
  }

  const confirmDelete = async () => {
    if (!pendingDelete) return
    setBusy(true)
    try {
      if (pendingDelete.kind === 'key') {
        await deleteKey(pendingDelete.id)
        if (assignments[pendingDelete.id]) {
          const nextAssignments = { ...assignments }
          delete nextAssignments[pendingDelete.id]
          persistUiState(folders, nextAssignments)
        }
      } else {
        const nextAssignments = { ...assignments }
        Object.keys(nextAssignments).forEach((keyId) => {
          if (nextAssignments[keyId] === pendingDelete.id) delete nextAssignments[keyId]
        })
        persistUiState(
          folders.filter((folder) => folder.id !== pendingDelete.id),
          nextAssignments
        )
        setExpandedFolderIds((current) => {
          const next = new Set(current)
          next.delete(pendingDelete.id)
          return next
        })
        if (activeFolderId === pendingDelete.id) setActiveFolderId('all')
      }
      setPendingDelete(null)
    } finally {
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

  const handleImport = async (note: string, sourcePath?: string, folderId?: string) => {
    if (!sourcePath) return
    setBusy(true)
    try {
      const result = await importKey(note, sourcePath)
      if (result) {
        const nextAssignments = { ...assignments }
        if (folderId) nextAssignments[result.key.id] = folderId
        else delete nextAssignments[result.key.id]
        persistUiState(folders, nextAssignments)
      }
      setNoteDialog(null)
    } catch {
      // useSshKeyLibrary 已将可展示错误写入 error 状态。
    } finally {
      setBusy(false)
    }
  }

  const handleEditNote = async (keyId: string, note: string, folderId?: string) => {
    setBusy(true)
    try {
      await updateNote(keyId, note)
      const nextAssignments = { ...assignments }
      if (folderId) nextAssignments[keyId] = folderId
      else delete nextAssignments[keyId]
      persistUiState(folders, nextAssignments)
      setNoteDialog(null)
    } catch {
      // useSshKeyLibrary 已将可展示错误写入 error 状态。
    } finally {
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
      <header className="connection-manager-header ssh-key-manager-header">
        <span className="connection-manager-title ssh-key-manager-title">
          <span aria-hidden="true" className="material-symbols-outlined">
            key
          </span>
          <span>密钥管理器</span>
        </span>
        <label className="connection-manager-search ssh-key-manager-search">
          <AppIcon name="search" size={14} />
          <input
            aria-label="筛选密钥"
            placeholder="筛选密钥..."
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
      </header>

      <div className="connection-manager-layout ssh-key-manager-layout">
        <aside className="connection-manager-sidebar" aria-label="密钥文件夹">
          <button
            className={`connection-manager-sidebar-item ssh-key-root-drop-target ${activeFolderId === 'all' ? 'active' : ''} ${
              dragOver?.id === ROOT_DROP_TARGET_ID ? 'drag-over' : ''
            }`}
            type="button"
            data-fileterm-sort-id={ROOT_DROP_TARGET_ID}
            data-fileterm-sort-kind="root"
            onClick={() => setActiveFolderId('all')}
            onDragOver={handleRootDragOver}
            onDragLeave={handleRootDragLeave}
            onDrop={handleRootDrop}
          >
            <span className="connection-manager-sidebar-icon">
              <AppIcon name="brand" size={14} />
            </span>
            <span className="connection-manager-sidebar-label">全部密钥</span>
            <span className="connection-manager-sidebar-count">{keys.length}</span>
          </button>

          {orderedFolders.map((folder) => (
            <button
              key={folder.id}
              className={`connection-manager-sidebar-item ${activeFolderId === folder.id ? 'active' : ''}`}
              type="button"
              data-fileterm-sort-id={folder.id}
              data-fileterm-sort-kind="folder"
              onClick={() => setActiveFolderId(folder.id)}
            >
              <span className="connection-manager-sidebar-icon">
                <AppIcon name="folder" size={14} />
              </span>
              <span className="connection-manager-sidebar-label">{folder.name}</span>
              <span className="connection-manager-sidebar-count">{folderKeyCount(folder.id)}</span>
            </button>
          ))}
        </aside>

        <section className="connection-manager-main ssh-key-manager-main">
          <div className="manager-table connection-manager-table ssh-key-manager-table">
            <div className="manager-head">
              <span>名称</span>
              <span>算法 / 指纹</span>
              <span>备注</span>
              <span>导入时间</span>
              <span>引用</span>
              <span>操作</span>
            </div>
            <div className="manager-body connection-manager-body">
              {(error || uiStateError) && !noteDialog ? (
                <div className="ssh-key-manager-error">{error || uiStateError}</div>
              ) : null}
              {isCreatingFolder && activeFolderId === 'all' ? (
                <ManagerInlineFolderRow
                  afterNameCells={['--', '--', '--', '--', null]}
                  className="ssh-key-folder-create-row"
                  placeholder={t.folderName}
                  value={newFolderName}
                  onChange={setNewFolderName}
                  onCommit={finishFolderCreation}
                  onDismiss={() => {
                    setIsCreatingFolder(false)
                    setNewFolderName('')
                  }}
                />
              ) : null}
              {activeFolderId === 'all'
                ? rootItems.map((rootItem) => {
                    if (rootItem.kind === 'key') {
                      const key = keys.find((item) => item.id === rootItem.id)
                      return key ? renderKeyRow(key) : null
                    }
                    const folder = folders.find((item) => item.id === rootItem.id)
                    if (!folder) return null
                    const folderKeys = visibleKeys.filter((key) => assignments[key.id] === folder.id)
                    const isExpanded = expandedFolderIds.has(folder.id)
                    const folderDragClass =
                      `${isFolderDragOver(folder.id)} ${dragging?.id === folder.id ? 'dragging' : ''}`.trim()
                    return (
                      <Fragment key={folder.id}>
                        <div
                          role="button"
                          tabIndex={0}
                          className={`manager-row folder-row ssh-key-folder-row ${folderDragClass}`.trim()}
                          data-fileterm-sort-id={folder.id}
                          data-fileterm-sort-kind="folder"
                          draggable={false}
                          onPointerDown={(event) => handlePointerDown(event, { kind: 'folder', id: folder.id })}
                          onDragStart={(event) => handleDragStart(event, { kind: 'folder', id: folder.id })}
                          onDragOver={(event) => handleDragOver(event, { kind: 'folder', id: folder.id })}
                          onDragLeave={(event) => {
                            event.preventDefault()
                            setDragOver(null)
                          }}
                          onDrop={(event) => handleDrop(event, { kind: 'folder', id: folder.id })}
                          onDragEnd={handleDragEnd}
                          onClick={() => {
                            if (suppressRowClickRef.current) return
                            toggleFolder(folder.id)
                          }}
                          onKeyDown={(event) => {
                            if (event.key === 'Enter' || event.key === ' ') {
                              event.preventDefault()
                              toggleFolder(folder.id)
                            }
                          }}
                        >
                          <span className="ssh-key-folder-name-cell">
                            <span
                              className="folder-icon manager-folder-toggle"
                              style={{ transform: isExpanded ? 'rotate(90deg)' : 'none' }}
                            >
                              <AppIcon name="chevron-right" size={12} />
                            </span>
                            <AppIcon name="folder" size={14} />
                            {editingFolder?.id === folder.id ? (
                              <input
                                autoFocus
                                className="manager-inline-input"
                                value={editingFolder.name}
                                onBlur={saveFolderRename}
                                onChange={(event) => setEditingFolder({ id: folder.id, name: event.target.value })}
                                onClick={(event) => event.stopPropagation()}
                                onKeyDown={(event) => {
                                  event.stopPropagation()
                                  if (event.key === 'Enter') saveFolderRename()
                                  if (event.key === 'Escape') setEditingFolder(null)
                                }}
                              />
                            ) : (
                              <strong>{folder.name}</strong>
                            )}
                          </span>
                          <span>--</span>
                          <span>--</span>
                          <span>--</span>
                          <span>--</span>
                          <span className="manager-actions ssh-key-folder-actions">
                            <button
                              aria-label={`重命名文件夹 ${folder.name}`}
                              className="manager-icon-action"
                              title="重命名文件夹"
                              type="button"
                              onMouseDown={(event) => event.stopPropagation()}
                              onPointerDown={(event) => event.stopPropagation()}
                              onClick={(event) => {
                                event.stopPropagation()
                                setEditingFolder({ id: folder.id, name: folder.name })
                              }}
                            >
                              <AppIcon name="edit" size={14} />
                            </button>
                            <button
                              aria-label={`删除文件夹 ${folder.name}`}
                              className="manager-icon-action danger"
                              title="删除文件夹"
                              type="button"
                              onMouseDown={(event) => event.stopPropagation()}
                              onPointerDown={(event) => event.stopPropagation()}
                              onClick={(event) => {
                                event.stopPropagation()
                                requestDelete('folder', folder.id, folder.name)
                              }}
                            >
                              <AppIcon name="trash" size={14} />
                            </button>
                          </span>
                        </div>
                        {isExpanded && folderKeys.length === 0 ? (
                          <div className="manager-row empty-folder ssh-key-empty-folder">
                            <span>{t.emptyFolder}</span>
                          </div>
                        ) : null}
                        {isExpanded ? folderKeys.map((key) => renderKeyRow(key, 'ssh-key-nested-row')) : null}
                      </Fragment>
                    )
                  })
                : null}
              {activeFolderId !== 'all' ? visibleKeys.map((key) => renderKeyRow(key)) : null}
              {!loading && !hasVisibleRows ? (
                <div className="connection-manager-empty ssh-key-manager-empty">
                  <span aria-hidden="true" className="material-symbols-outlined">
                    key_off
                  </span>
                  <strong>{query ? '没有匹配的密钥' : '尚未导入私钥'}</strong>
                  <span>{query ? '尝试其他搜索词。' : '新建密钥后即可在 SSH 连接中复用。'}</span>
                </div>
              ) : null}
              {loading ? <div className="connection-manager-empty">正在加载密钥列表…</div> : null}
            </div>
          </div>

          <div className={`connection-manager-floating-drawer ${isActionsExpanded ? 'expanded' : ''}`}>
            <div className="drawer-options-wrapper">
              <button
                className="drawer-option-btn secondary-btn"
                type="button"
                onClick={() => {
                  setIsCreatingFolder(true)
                  setNewFolderName('')
                  setActiveFolderId('all')
                  setIsActionsExpanded(false)
                }}
              >
                <AppIcon name="folder" size={13} />
                <span>新建文件夹</span>
              </button>
              <button className="drawer-option-btn primary-btn" type="button" onClick={openNewKeyDialog}>
                <AppIcon name="plus" size={13} />
                <span>新建密钥</span>
              </button>
            </div>
            <button
              aria-label="展开操作"
              className="drawer-trigger-btn"
              type="button"
              onClick={() => setIsActionsExpanded((expanded) => !expanded)}
            >
              <AppIcon name="plus" size={16} />
            </button>
          </div>
        </section>
      </div>

      {noteDialog ? (
        <SshKeyNoteDialog
          errorMessage={error}
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
          onSubmit={(note, sourcePath, folderId) => {
            if (noteDialog.mode === 'import') {
              void handleImport(note, sourcePath, folderId)
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
              : `确定删除 ${pendingDelete.name} 吗？此操作不会删除原始文件。`
          }
          isSubmitting={busy}
          onClose={() => {
            if (!busy) setPendingDelete(null)
          }}
          onConfirm={() => void confirmDelete()}
          title={t.delete}
        />
      ) : null}
    </section>
  )
}

function SshKeyRow({
  item,
  className,
  draggable = false,
  onDelete,
  onEdit,
  onDragStart,
  onDragOver,
  onDragLeave,
  onDrop,
  onDragEnd,
  onPointerDown
}: {
  item: SshKeyMetadata
  className?: string
  draggable?: boolean
  onDelete(): void
  onEdit(): void
  onDragStart?(event: DragEvent): void
  onDragOver?(event: DragEvent): void
  onDragLeave?(event: DragEvent): void
  onDrop?(event: DragEvent): void
  onDragEnd?(): void
  onPointerDown?(event: ReactPointerEvent): void
}) {
  return (
    <div
      className={`manager-row ssh-key-manager-row${className ? ` ${className}` : ''}`}
      draggable={draggable}
      data-fileterm-sort-id={item.id}
      data-fileterm-sort-kind="key"
      onPointerDown={onPointerDown}
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDragLeave={onDragLeave}
      onDrop={onDrop}
      onDragEnd={onDragEnd}
    >
      <span className="ssh-key-name-cell">
        <strong>{item.name}</strong>
        <small>{item.encrypted ? '已加密' : '未加密'}</small>
      </span>
      <span className="ssh-key-fingerprint-cell">
        <span>{item.algorithm}</span>
        <code title={item.fingerprint}>{shortFingerprint(item.fingerprint)}</code>
      </span>
      <span className="ssh-key-note-cell">{item.note || '—'}</span>
      <span className="ssh-key-imported-at">
        {new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(item.importedAt)}
      </span>
      <span>{item.usageCount}</span>
      <span className="manager-actions ssh-key-actions">
        <button
          aria-label="修改备注"
          className="manager-icon-action"
          title="修改备注"
          type="button"
          onMouseDown={(event) => event.stopPropagation()}
          onPointerDown={(event) => event.stopPropagation()}
          onClick={(event) => {
            event.stopPropagation()
            onEdit()
          }}
        >
          <AppIcon name="edit" size={14} />
        </button>
        <button
          aria-label="删除密钥"
          className="manager-icon-action danger"
          disabled={item.usageCount > 0}
          title={item.usageCount > 0 ? '该密钥仍被连接引用，无法删除' : '删除密钥'}
          type="button"
          onMouseDown={(event) => event.stopPropagation()}
          onPointerDown={(event) => event.stopPropagation()}
          onClick={(event) => {
            event.stopPropagation()
            onDelete()
          }}
        >
          <AppIcon name="trash" size={14} />
        </button>
      </span>
    </div>
  )
}

function isSshKeyFolder(value: unknown): value is SshKeyFolder {
  return Boolean(
    value &&
    typeof value === 'object' &&
    typeof (value as SshKeyFolder).id === 'string' &&
    typeof (value as SshKeyFolder).name === 'string'
  )
}

function createId(prefix: string) {
  return globalThis.crypto?.randomUUID?.() ?? `${prefix}-${Date.now()}`
}

function shortFingerprint(fingerprint: string) {
  return fingerprint.length > 34 ? `${fingerprint.slice(0, 18)}…${fingerprint.slice(-12)}` : fingerprint
}
