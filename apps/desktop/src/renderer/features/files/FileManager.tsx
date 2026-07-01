import { useEffect, useMemo, useRef, useState, type CSSProperties, type DragEvent, type FormEvent, type KeyboardEvent, type MouseEvent } from 'react'
import type {
  CommandExecutionOptions,
  CommandFolder,
  CommandTemplate,
  LocalFileItem,
  RemoteFileItem,
  SessionSnapshot,
  WorkspaceTab
} from '@fileterm/core'
import {
  copyText,
  hasSelectedText,
  localFileDragType,
  mergeUnique,
  nextSelection,
  parseDraggedPaths,
  rangePaths,
  remoteFileDragType,
  setFileDragPreview
} from '../../app/app-utils'
import { t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'
import type { SendScope, SessionSendTarget } from '../common/session-send-targets'
import { CommandCenter } from '../commands/CommandCenter'
import { FileContextMenu } from './FileContextMenu'
import { getDisplayFileTypeSortKey } from './file-kind'
import { FileTable, LocalFileTable, PanePathBar, type RemoteFileSortField, type RemoteFileSortState } from './FileTables'

function areStringArraysEqual(left: string[], right: string[]) {
  if (left === right) {
    return true
  }
  if (left.length !== right.length) {
    return false
  }
  for (let index = 0; index < left.length; index += 1) {
    if (left[index] !== right[index]) {
      return false
    }
  }
  return true
}

function compareText(left: string, right: string) {
  return left.localeCompare(right, undefined, { numeric: true, sensitivity: 'base' })
}

function parseSortableSize(value: string) {
  if (!value || value === '-') {
    return 0
  }

  const match = value.trim().match(/^([\d.]+)\s*([A-Za-z]+)$/)
  if (!match) {
    return 0
  }

  const amount = Number.parseFloat(match[1])
  if (!Number.isFinite(amount)) {
    return 0
  }

  const unit = match[2].toUpperCase()
  const units: Record<string, number> = {
    B: 1,
    KB: 1024,
    MB: 1024 ** 2,
    GB: 1024 ** 3,
    TB: 1024 ** 4
  }

  return amount * (units[unit] ?? 1)
}

function parseSortableTimestamp(value: string) {
  if (!value) {
    return 0
  }

  const normalized = value.replace(/\//g, '-')
  const parsed = Date.parse(normalized)
  return Number.isNaN(parsed) ? 0 : parsed
}

function compareRemoteFilesByField(
  left: RemoteFileItem,
  right: RemoteFileItem,
  sort: RemoteFileSortState
) {
  const direction = sort.direction === 'asc' ? 1 : -1

  switch (sort.field) {
    case 'size':
      return (parseSortableSize(left.size) - parseSortableSize(right.size)) * direction
    case 'type':
      return compareText(getDisplayFileTypeSortKey(left), getDisplayFileTypeSortKey(right)) * direction
    case 'modified':
      return (parseSortableTimestamp(left.modified) - parseSortableTimestamp(right.modified)) * direction
    case 'permission':
      return compareText(left.permission ?? '', right.permission ?? '') * direction
    case 'ownerGroup':
      return compareText(left.ownerGroup ?? '', right.ownerGroup ?? '') * direction
    case 'name':
    default:
      return compareText(left.name, right.name) * direction
  }
}

function sortRemoteFiles(rows: RemoteFileItem[], sort: RemoteFileSortState) {
  const parentRow = rows.find((row) => row.name === '..') ?? null
  const sortableRows = rows.filter((row) => row.name !== '..')

  sortableRows.sort((left, right) => {
    if (sort.field !== 'type' && left.type !== right.type) {
      return left.type === 'folder' ? -1 : 1
    }

    const byField = compareRemoteFilesByField(left, right, sort)
    if (byField !== 0) {
      return byField
    }

    if (left.type !== right.type) {
      return left.type === 'folder' ? -1 : 1
    }

    return compareText(left.name, right.name)
  })

  return parentRow ? [parentRow, ...sortableRows] : sortableRows
}

export function FileManager({
  activeSession,
  activeTab,
  sendTargets,
  commandFolders,
  commandTemplates,
  isBusy,
  localItems,
  localPath,
  canPasteToLocal,
  canPasteToRemote,
  clipboardStatusText,
  localCutPaths,
  remoteCutPaths,
  onExecuteCommand,
  onOpenCommandManager,
  onCopyItems,
  onCutItems,
  onClearCutState,
  onOpenLocalItem,
  onOpenLocalPath,
  onOpenRemoteItem,
  onOpenRemotePath,
  onPasteIntoPane,
  onRefresh,
  onUploadFiles,
  onChooseUploadFiles,
  onDownloadFiles,
  onDropUpload,
  onRequestChangePermissions,
  onRequestDelete,
  onRequestNewFile,
  onRequestNewFolder,
  onRequestQuickDelete,
  onRequestRename,
  onToggleFollowShellCwd,
  onToggleRemoteFileAccessMode,
  remoteFileAccessMode,
  isRemoteDirectoryLoading
}: {
  activeSession: SessionSnapshot
  activeTab: WorkspaceTab | null
  sendTargets: SessionSendTarget[]
  commandFolders: CommandFolder[]
  commandTemplates: CommandTemplate[]
  isBusy: boolean
  localItems: LocalFileItem[]
  localPath: string
  canPasteToLocal: boolean
  canPasteToRemote: boolean
  clipboardStatusText: string | null
  localCutPaths: string[]
  remoteCutPaths: string[]
  onExecuteCommand(commandId: string, args: string[], options: CommandExecutionOptions, scope: SendScope, selectedTabIds: string[]): void
  onOpenCommandManager(): void
  onCopyItems(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onCutItems(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onClearCutState(): void
  onOpenLocalItem(item: LocalFileItem): void
  onOpenLocalPath(path: string): void
  onOpenRemoteItem(item: RemoteFileItem): void
  onOpenRemotePath(path: string): void
  onPasteIntoPane(pane: 'local' | 'remote'): void
  onRefresh(): void
  onUploadFiles(items: LocalFileItem[]): void
  onChooseUploadFiles(): void
  onDownloadFiles(items: RemoteFileItem[], targetDirectory?: string): void
  onDropUpload(event: DragEvent<HTMLDivElement>): void
  onRequestChangePermissions(pane: 'local' | 'remote', item: LocalFileItem | RemoteFileItem): void
  onRequestDelete(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onRequestNewFile(pane: 'local' | 'remote', directoryPath: string): void
  onRequestNewFolder(pane: 'local' | 'remote', directoryPath: string): void
  onRequestQuickDelete(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onRequestRename(pane: 'local' | 'remote', item: LocalFileItem | RemoteFileItem): void
  onToggleFollowShellCwd(): void
  onToggleRemoteFileAccessMode(): void
  remoteFileAccessMode: 'user' | 'root'
  isRemoteDirectoryLoading: boolean
}) {
  const defaultRemoteSort = { field: 'name', direction: 'asc' } satisfies RemoteFileSortState
  const isRemoteConnected = activeSession.connected === true
  const isSshSession = activeTab?.sessionType === 'ssh'
  const showRemoteDirectoryLoading = isRemoteDirectoryLoading || activeSession.remoteFilesLoading === true
  const [activeView, setActiveView] = useState<'file' | 'command'>('file')
  const [localPaneWidth, setLocalPaneWidth] = useState(214)
  const [localPathInput, setLocalPathInput] = useState(localPath)
  const [remotePathInput, setRemotePathInput] = useState(activeSession.remotePath)
  const [remoteSort, setRemoteSort] = useState<RemoteFileSortState>(defaultRemoteSort)
  const [selectedLocalPaths, setSelectedLocalPaths] = useState<string[]>([])
  const [selectedRemotePaths, setSelectedRemotePaths] = useState<string[]>([])
  const [localAnchorPath, setLocalAnchorPath] = useState<string | null>(null)
  const [remoteAnchorPath, setRemoteAnchorPath] = useState<string | null>(null)
  const [keyboardPane, setKeyboardPane] = useState<'local' | 'remote'>('remote')
  const [contextMenu, setContextMenu] = useState<{
    pane: 'local' | 'remote'
    x: number
    y: number
    path: string | null
  } | null>(null)
  const splitRef = useRef<HTMLDivElement | null>(null)
  const containerRef = useRef<HTMLDivElement | null>(null)
  const isResizingFileSplit = useRef(false)
  const isSelectingLocal = useRef(false)
  const isSelectingRemote = useRef(false)
  const didDragSelect = useRef(false)
  const suppressNextSelectionClick = useRef(false)
  const suppressNextClearClick = useRef(false)
  const localDragSelection = useRef<{ basePaths: string[]; startPath: string | null } | null>(null)
  const remoteDragSelection = useRef<{ basePaths: string[]; startPath: string | null } | null>(null)
  const localScrollRef = useRef<HTMLDivElement | null>(null)
  const remoteScrollRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    setLocalPathInput((prev) => prev === localPath ? prev : localPath)
    setSelectedLocalPaths((prev) => {
      const next = prev.filter((selectedPath) => localItems.some((item) => item.path === selectedPath))
      return areStringArraysEqual(prev, next) ? prev : next
    })
  }, [localItems, localPath])

  useEffect(() => {
    setRemotePathInput((prev) => prev === activeSession.remotePath ? prev : activeSession.remotePath)
    setSelectedRemotePaths((prev) => {
      const next = prev.filter((selectedPath) => activeSession.remoteFiles.some((item) => item.path === selectedPath))
      return areStringArraysEqual(prev, next) ? prev : next
    })
  }, [activeSession.remoteFiles, activeSession.remotePath])

  useEffect(() => {
    if (isRemoteConnected) {
      return
    }
    setSelectedRemotePaths([])
    setRemoteAnchorPath(null)
    setContextMenu((prev) => prev?.pane === 'remote' ? null : prev)
  }, [isRemoteConnected])

  useEffect(() => {
    setRemoteSort(defaultRemoteSort)
  }, [activeTab?.id])

  const sortedRemoteRows = useMemo(() => {
    if (!isRemoteConnected) {
      return []
    }

    return sortRemoteFiles(activeSession.remoteFiles, remoteSort)
  }, [activeSession.remoteFiles, isRemoteConnected, remoteSort])

  const selectedRemoteItems = activeSession.remoteFiles.filter((item) => selectedRemotePaths.includes(item.path))
  const selectedRemoteFileItems = selectedRemoteItems.filter((item) => item.type === 'file')
  const contextLocalItem = contextMenu?.pane === 'local'
    ? localItems.find((item) => item.path === contextMenu.path) ?? null
    : null
  const contextRemoteItem = contextMenu?.pane === 'remote'
    ? activeSession.remoteFiles.find((item) => item.path === contextMenu.path) ?? null
    : null
  const contextLocalSelection = contextLocalItem && selectedLocalPaths.includes(contextLocalItem.path)
    ? localItems.filter((item) => selectedLocalPaths.includes(item.path) && item.name !== '..')
    : contextLocalItem && contextLocalItem.name !== '..' ? [contextLocalItem] : []
  const contextRemoteSelection = contextRemoteItem && selectedRemotePaths.includes(contextRemoteItem.path)
    ? selectedRemoteItems.filter((item) => item.name !== '..')
    : contextRemoteItem && contextRemoteItem.name !== '..' ? [contextRemoteItem] : []
  const contextSelectionCount = contextMenu?.pane === 'local' ? contextLocalSelection.length : contextRemoteSelection.length
  const isMultiContextSelection = contextSelectionCount > 1
  const singleContextItem = contextMenu?.pane === 'local'
    ? (contextLocalSelection.length === 1 ? contextLocalSelection[0] : contextLocalItem)
    : (contextRemoteSelection.length === 1 ? contextRemoteSelection[0] : contextRemoteItem)
  const canOpenContextItem = Boolean(singleContextItem && (contextMenu?.pane !== 'remote' || isRemoteConnected))
  const canCopyContextItems = contextSelectionCount > 0
  const canCopyContextPath = Boolean(singleContextItem && !isMultiContextSelection)
  const canCutContextItems = contextSelectionCount > 0
  const canDownloadContextItems = isRemoteConnected && contextRemoteSelection.some((item) => item.type === 'file')
  const canPasteIntoContextPane = contextMenu?.pane === 'local' ? canPasteToLocal : isRemoteConnected && canPasteToRemote
  const canUploadContextItems = isRemoteConnected && (
    Boolean(!isMultiContextSelection && contextLocalSelection.length)
    || Boolean(!isMultiContextSelection && contextMenu?.pane === 'remote')
  )
  const canCreateFromContext = !isMultiContextSelection && (contextMenu?.pane !== 'remote' || isRemoteConnected)
  const canRenameContextItem = Boolean(
    singleContextItem
    && !isMultiContextSelection
    && singleContextItem.name !== '..'
    && (contextMenu?.pane !== 'remote' || isRemoteConnected)
  )
  const canChangeContextPermissions = Boolean(
    singleContextItem
    && !isMultiContextSelection
    && singleContextItem.name !== '..'
    && (contextMenu?.pane !== 'remote' || isRemoteConnected)
  )

  const keyboardSelection = keyboardPane === 'local'
    ? localItems.filter((item) => selectedLocalPaths.includes(item.path) && item.name !== '..')
    : selectedRemoteItems.filter((item) => item.name !== '..')
  const canPasteFromKeyboard = keyboardPane === 'local' ? canPasteToLocal : canPasteToRemote

  const submitLocalPath = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    onOpenLocalPath(localPathInput.trim() || localPath)
  }

  const submitRemotePath = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!isRemoteConnected) {
      return
    }
    const targetPath = remotePathInput.trim() || activeSession.remotePath
    onOpenRemotePath(targetPath)
  }

  const handleRemotePaneDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault()
    event.stopPropagation()
    if (!isRemoteConnected) {
      return
    }

    const draggedLocalPath = event.dataTransfer.getData(localFileDragType)
    if (draggedLocalPath) {
      const draggedPaths = parseDraggedPaths(draggedLocalPath)
      const items = localItems.filter((row) => draggedPaths.includes(row.path) && row.name !== '..')
      if (items.length) {
        onUploadFiles(items)
      }
      return
    }

    onDropUpload(event)
  }

  const handleLocalPaneDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault()
    event.stopPropagation()

    const draggedRemotePayload = event.dataTransfer.getData(remoteFileDragType)
    if (!draggedRemotePayload) {
      return
    }

    if (!isRemoteConnected) {
      return
    }

    const draggedPaths = parseDraggedPaths(draggedRemotePayload)
    const items = activeSession.remoteFiles.filter((row) => draggedPaths.includes(row.path) && row.type === 'file')
    if (items.length) {
      onDownloadFiles(items, localPath)
    }
  }

  const selectLocalItem = (event: MouseEvent<HTMLTableRowElement>, item: LocalFileItem) => {
    if (suppressNextSelectionClick.current) {
      suppressNextSelectionClick.current = false
      return
    }
    const selected = nextSelection({
      anchorPath: localAnchorPath,
      currentSelection: selectedLocalPaths,
      event,
      itemPath: item.path,
      rows: localItems
    })
    setSelectedLocalPaths(selected)
    setLocalAnchorPath(item.path)
  }

  const selectRemoteItem = (event: MouseEvent<HTMLTableRowElement>, item: RemoteFileItem) => {
    if (suppressNextSelectionClick.current) {
      suppressNextSelectionClick.current = false
      return
    }
    const selected = nextSelection({
      anchorPath: remoteAnchorPath,
      currentSelection: selectedRemotePaths,
      event,
      itemPath: item.path,
      rows: activeSession.remoteFiles
    })
    setSelectedRemotePaths(selected)
    setRemoteAnchorPath(item.path)
  }

  const extendLocalDragSelection = (item: LocalFileItem) => {
    const session = localDragSelection.current
    if (!isSelectingLocal.current || !session) return
    didDragSelect.current = true
    if (!session.startPath) {
      session.startPath = item.path
      setSelectedLocalPaths(mergeUnique([...session.basePaths, item.path]))
      setLocalAnchorPath(item.path)
      return
    }
    setSelectedLocalPaths(mergeUnique([
      ...session.basePaths,
      ...rangePaths(localItems, session.startPath, item.path)
    ]))
  }

  const extendRemoteDragSelection = (item: RemoteFileItem) => {
    const session = remoteDragSelection.current
    if (!isSelectingRemote.current || !session) return
    didDragSelect.current = true
    if (!session.startPath) {
      session.startPath = item.path
      setSelectedRemotePaths(mergeUnique([...session.basePaths, item.path]))
      setRemoteAnchorPath(item.path)
      return
    }
    setSelectedRemotePaths(mergeUnique([
      ...session.basePaths,
      ...rangePaths(activeSession.remoteFiles, session.startPath, item.path)
    ]))
  }

  const openContextTarget = () => {
    if (contextMenu?.pane === 'local' && singleContextItem) {
      onOpenLocalItem(singleContextItem as LocalFileItem)
    }
    if (contextMenu?.pane === 'remote' && singleContextItem) {
      onOpenRemoteItem(singleContextItem as RemoteFileItem)
    }
    setContextMenu(null)
  }

  const copyContextPath = () => {
    const targetPath = singleContextItem?.path
    if (targetPath) {
      copyText(targetPath)
    }
    setContextMenu(null)
  }

  const focusContainer = () => {
    containerRef.current?.focus()
  }

  const handleKeyboardShortcuts = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'Escape') {
      onClearCutState()
      return
    }

    if (!(event.metaKey || event.ctrlKey) || event.altKey) {
      return
    }

    const target = event.target
    if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || (target instanceof HTMLElement && target.isContentEditable)) {
      return
    }

    if (hasSelectedText()) {
      return
    }

    const key = event.key.toLowerCase()
    if (key === 'c') {
      if (!keyboardSelection.length) {
        return
      }
      event.preventDefault()
      onCopyItems(keyboardPane, keyboardSelection)
      return
    }

    if (key === 'x') {
      if (!keyboardSelection.length) {
        return
      }
      event.preventDefault()
      onCutItems(keyboardPane, keyboardSelection)
      return
    }

    if (key === 'v') {
      if (!canPasteFromKeyboard) {
        return
      }
      event.preventDefault()
      onPasteIntoPane(keyboardPane)
    }
  }

  useEffect(() => {
    const handleMouseMove = (event: globalThis.MouseEvent) => {
      if (!isResizingFileSplit.current || !splitRef.current) return

      const rect = splitRef.current.getBoundingClientRect()
      const minLocalWidth = 180
      const minRemoteWidth = 320
      const maxLocalWidth = Math.max(minLocalWidth, rect.width - minRemoteWidth)
      const nextWidth = Math.min(maxLocalWidth, Math.max(minLocalWidth, event.clientX - rect.left))
      setLocalPaneWidth(nextWidth)
    }

    const handleMouseUp = () => {
      if (didDragSelect.current) {
        suppressNextClearClick.current = true
      }
      didDragSelect.current = false
      isSelectingLocal.current = false
      isSelectingRemote.current = false
      localDragSelection.current = null
      remoteDragSelection.current = null
      if (!isResizingFileSplit.current) return
      isResizingFileSplit.current = false
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }

    window.addEventListener('mousemove', handleMouseMove)
    window.addEventListener('mouseup', handleMouseUp)
    return () => {
      window.removeEventListener('mousemove', handleMouseMove)
      window.removeEventListener('mouseup', handleMouseUp)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }
  }, [activeSession.remoteFiles, localItems])

  return (
    <div
      ref={containerRef}
      className="file-manager"
      onClick={() => setContextMenu(null)}
      onKeyDown={handleKeyboardShortcuts}
      tabIndex={0}
      style={{ '--local-pane-width': `${localPaneWidth}px` } as CSSProperties}
    >
      <div className="file-tabs">
        <div className="file-tabs-left">
          <button className={activeView === 'file' ? 'active' : ''} type="button" onClick={() => setActiveView('file')}>{t.file}</button>
          {isSshSession ? (
            <button className={activeView === 'command' ? 'active' : ''} type="button" onClick={() => setActiveView('command')}>{t.command}</button>
          ) : null}
        </div>
        <span className={`file-current-path ${clipboardStatusText ? 'is-status-hint' : ''}`}>
          {activeView === 'file'
            ? clipboardStatusText || activeSession.remotePath
            : `${t.commandQuickLaunch} (${isSshSession ? t.send : t.commandSshOnly})`}
        </span>
        {activeView === 'file' ? (
          <div className="file-tab-actions">
            <button title={t.refresh} type="button" disabled={!isRemoteConnected} onClick={onRefresh}><AppIcon name="refresh" /></button>
            {activeTab?.sessionType === 'ssh' ? (
              <button
                aria-pressed={remoteFileAccessMode === 'root'}
                className={remoteFileAccessMode === 'root' ? 'active' : ''}
                disabled={!isRemoteConnected}
                title={`${remoteFileAccessMode === 'root' ? t.fileRootView : t.fileUserView} - ${t.fileRootViewHint}`}
                type="button"
                onClick={onToggleRemoteFileAccessMode}
              >
                {remoteFileAccessMode === 'root' ? 'root' : 'user'}
              </button>
            ) : null}
            <button title={t.downloadTo} type="button" disabled={!isRemoteConnected || !selectedRemoteFileItems.length} onClick={() => onDownloadFiles(selectedRemoteFileItems)}>
              <AppIcon name="download" />
            </button>
            <button title={t.upload} type="button" disabled={!isRemoteConnected} onClick={onChooseUploadFiles}><AppIcon name="upload" /></button>
          </div>
        ) : (
          <div className="file-tab-actions">
            <button className="flat-button compact command-manager-launch" type="button" onClick={onOpenCommandManager}>{t.commandManager}</button>
          </div>
        )}
      </div>
      {activeView === 'command' && isSshSession ? (
        <CommandCenter
          activeTab={activeTab}
          commandFolders={commandFolders}
          commandTemplates={commandTemplates}
          isBusy={isBusy}
          sendTargets={sendTargets}
          onExecute={onExecuteCommand}
          paneWidth={localPaneWidth}
          onPaneWidthChange={setLocalPaneWidth}
        />
      ) : (
      <div className="file-split" ref={splitRef}>
        <div
          className="local-pane"
          onMouseDownCapture={() => {
            setKeyboardPane('local')
            focusContainer()
          }}
          onClick={(event) => {
            if (event.target === event.currentTarget) {
              setSelectedLocalPaths([])
              setLocalAnchorPath(null)
            }
          }}
          onDragOver={(event) => {
            event.preventDefault()
            event.dataTransfer.dropEffect = 'copy'
          }}
          onDrop={handleLocalPaneDrop}
        >
          <PanePathBar label={t.localComputer} value={localPathInput} onChange={setLocalPathInput} onSubmit={submitLocalPath} />
          <div
            className="file-table-shell local-file-table-shell"
            ref={localScrollRef}
            onContextMenu={(event) => {
              if (event.target !== event.currentTarget) return
              event.preventDefault()
              event.stopPropagation()
              setSelectedLocalPaths([])
              setLocalAnchorPath(null)
              setContextMenu({ pane: 'local', x: event.clientX, y: event.clientY, path: null })
            }}
            onMouseDown={(event) => {
              if (event.target !== event.currentTarget || event.button !== 0) return
              isSelectingLocal.current = true
              didDragSelect.current = false
              localDragSelection.current = {
                basePaths: event.metaKey || event.ctrlKey ? selectedLocalPaths : [],
                startPath: null
              }
            }}
            onClick={(event) => {
              if (event.target !== event.currentTarget) return
              if (suppressNextClearClick.current) {
                suppressNextClearClick.current = false
                return
              }
              setSelectedLocalPaths([])
              setLocalAnchorPath(null)
            }}
          >
            <LocalFileTable
              scrollRef={localScrollRef}
              cutPaths={localCutPaths}
              rows={localItems}
              selectedPaths={selectedLocalPaths}
              onDragItem={(event, item) => {
                event.dataTransfer.effectAllowed = 'copy'
                const payload = selectedLocalPaths.includes(item.path)
                  ? selectedLocalPaths
                  : [item.path]
                const previewItems = localItems.filter((row) => payload.includes(row.path) && row.name !== '..')
                event.dataTransfer.setData(localFileDragType, JSON.stringify(payload))
                setFileDragPreview(event, previewItems.map((row) => row.name))
              }}
              onOpenItem={onOpenLocalItem}
              onContextItem={(event, item) => {
                event.preventDefault()
                event.stopPropagation()
                if (!selectedLocalPaths.includes(item.path)) {
                  setSelectedLocalPaths([item.path])
                  setLocalAnchorPath(item.path)
                }
                setContextMenu({ pane: 'local', x: event.clientX, y: event.clientY, path: item.path })
              }}
              onClearSelection={() => {
                if (suppressNextClearClick.current) {
                  suppressNextClearClick.current = false
                  return
                }
                setSelectedLocalPaths([])
                setLocalAnchorPath(null)
              }}
              onSelectItem={selectLocalItem}
              onSelectionDragStart={(event, item) => {
                setKeyboardPane('local')
                isSelectingLocal.current = true
                didDragSelect.current = false
                const startPath = event.shiftKey && localAnchorPath ? localAnchorPath : item.path
                const basePaths = event.metaKey || event.ctrlKey ? selectedLocalPaths : []
                localDragSelection.current = { basePaths, startPath }
                suppressNextSelectionClick.current = true
                setSelectedLocalPaths(nextSelection({
                  anchorPath: localAnchorPath,
                  currentSelection: selectedLocalPaths,
                  event,
                  itemPath: item.path,
                  rows: localItems
                }))
                setLocalAnchorPath(startPath)
              }}
              onSelectionDragEnter={extendLocalDragSelection}
            />
          </div>
        </div>
        <div
          className="file-split-resizer"
          onMouseDown={() => {
            isResizingFileSplit.current = true
            document.body.style.cursor = 'col-resize'
            document.body.style.userSelect = 'none'
          }}
          role="separator"
        />
        <div
          className="pane remote-pane"
          onMouseDownCapture={() => {
            setKeyboardPane('remote')
            focusContainer()
          }}
          onClick={(event) => {
            if (event.target === event.currentTarget) {
              setSelectedRemotePaths([])
              setRemoteAnchorPath(null)
            }
          }}
          onDragOver={(event) => {
            event.preventDefault()
            if (isRemoteConnected) {
              event.dataTransfer.dropEffect = 'copy'
            }
          }}
          onDrop={handleRemotePaneDrop}
        >
          <div
            aria-busy={showRemoteDirectoryLoading}
            className="remote-pane-content"
          >
            <PanePathBar
              disabled={!isRemoteConnected}
              hint={isRemoteConnected ? t.dragUpload : t.remoteDisconnectedDescription}
              label={t.remoteHost}
              value={remotePathInput}
              action={isSshSession ? (
                <button
                  aria-pressed={activeSession.followShellCwd !== false}
                  className={`follow-shell-cwd-toggle ${activeSession.followShellCwd !== false ? 'is-active' : ''}`}
                  disabled={!isRemoteConnected}
                  onClick={onToggleFollowShellCwd}
                  title={activeSession.shellCwd
                    ? `${t.shellCwd}: ${activeSession.shellCwd}`
                    : t.followShellCwdUnavailable}
                  type="button"
                >
                  {t.followShellCwd}
                </button>
              ) : null}
              onChange={setRemotePathInput}
              onSubmit={submitRemotePath}
            />
            <div className="remote-file-table-region">
              <div
                ref={remoteScrollRef}
                className="file-table-shell remote-file-table-shell"
                onContextMenu={(event) => {
                  if (!isRemoteConnected) return
                  if (event.target !== event.currentTarget) return
                  event.preventDefault()
                  event.stopPropagation()
                  setSelectedRemotePaths([])
                  setRemoteAnchorPath(null)
                  setContextMenu({ pane: 'remote', x: event.clientX, y: event.clientY, path: null })
                }}
                onMouseDown={(event) => {
                  if (!isRemoteConnected) return
                  if (event.target !== event.currentTarget || event.button !== 0) return
                  isSelectingRemote.current = true
                  didDragSelect.current = false
                  remoteDragSelection.current = {
                    basePaths: event.metaKey || event.ctrlKey ? selectedRemotePaths : [],
                    startPath: null
                  }
                }}
                onClick={(event) => {
                  if (event.target !== event.currentTarget) return
                  if (suppressNextClearClick.current) {
                    suppressNextClearClick.current = false
                    return
                  }
                  setSelectedRemotePaths([])
                  setRemoteAnchorPath(null)
                }}
              >
                <FileTable
                  scrollRef={remoteScrollRef}
                  cutPaths={remoteCutPaths}
                  emptyText={isRemoteConnected ? t.emptyFiles : t.remoteDisconnectedDescription}
                  rows={sortedRemoteRows}
                  sortState={remoteSort}
                  selectedPaths={selectedRemotePaths}
                  onToggleSort={(field) => {
                    setRemoteSort((current) => (
                      current.field === field
                        ? { field, direction: current.direction === 'asc' ? 'desc' : 'asc' }
                        : { field, direction: 'asc' }
                    ))
                  }}
                  onDragItem={(event, item) => {
                    if (!isRemoteConnected) return
                    event.dataTransfer.effectAllowed = 'copy'
                    const payload = selectedRemotePaths.includes(item.path) ? selectedRemotePaths : [item.path]
                    const previewItems = sortedRemoteRows.filter((row) => payload.includes(row.path))
                    event.dataTransfer.setData(remoteFileDragType, JSON.stringify(payload))
                    setFileDragPreview(event, previewItems.map((row) => row.name))
                  }}
                  onOpenItem={(item) => {
                    if (isRemoteConnected) {
                      onOpenRemoteItem(item)
                    }
                  }}
                  onContextItem={(event, item) => {
                    if (!isRemoteConnected) return
                    event.preventDefault()
                    event.stopPropagation()
                    if (!selectedRemotePaths.includes(item.path)) {
                      setSelectedRemotePaths([item.path])
                      setRemoteAnchorPath(item.path)
                    }
                    setContextMenu({ pane: 'remote', x: event.clientX, y: event.clientY, path: item.path })
                  }}
                  onClearSelection={() => {
                    if (suppressNextClearClick.current) {
                      suppressNextClearClick.current = false
                      return
                    }
                    setSelectedRemotePaths([])
                    setRemoteAnchorPath(null)
                  }}
                  onSelectItem={(event, item) => {
                    if (isRemoteConnected) {
                      selectRemoteItem(event, item)
                    }
                  }}
                  onSelectionDragStart={(event, item) => {
                    if (!isRemoteConnected) return
                    setKeyboardPane('remote')
                    isSelectingRemote.current = true
                    didDragSelect.current = false
                    const startPath = event.shiftKey && remoteAnchorPath ? remoteAnchorPath : item.path
                    const basePaths = event.metaKey || event.ctrlKey ? selectedRemotePaths : []
                    remoteDragSelection.current = { basePaths, startPath }
                    suppressNextSelectionClick.current = true
                    setSelectedRemotePaths(nextSelection({
                      anchorPath: remoteAnchorPath,
                      currentSelection: selectedRemotePaths,
                      event,
                      itemPath: item.path,
                      rows: sortedRemoteRows
                    }))
                    setRemoteAnchorPath(startPath)
                  }}
                  onSelectionDragEnter={(item) => {
                    if (isRemoteConnected) {
                      extendRemoteDragSelection(item)
                    }
                  }}
                />
              </div>
            </div>
            {showRemoteDirectoryLoading ? (
              <div
                aria-label={t.loadingRemoteDirectory}
                aria-live="polite"
                className="remote-directory-loading"
                role="status"
              >
                <span aria-hidden="true" className="remote-directory-loading-spinner" />
              </div>
            ) : null}
          </div>
        </div>
      </div>
      )}
      {contextMenu ? (
        <FileContextMenu
          canChangePermissions={canChangeContextPermissions}
          canCopy={canCopyContextItems}
          canCopyPath={canCopyContextPath}
          canCreate={canCreateFromContext}
          canCut={canCutContextItems}
          canDownload={canDownloadContextItems}
          canOpen={canOpenContextItem}
          canPaste={canPasteIntoContextPane}
          canQuickDelete={isRemoteConnected && contextMenu.pane === 'remote' && activeTab?.sessionType === 'ssh'}
          canRename={canRenameContextItem}
          canUpload={canUploadContextItems}
          item={singleContextItem ?? contextLocalItem ?? contextRemoteItem}
          pane={contextMenu.pane}
          position={{ x: contextMenu.x, y: contextMenu.y }}
          onChangePermissions={() => {
            const item = singleContextItem
            if (item) {
              onRequestChangePermissions(contextMenu.pane, item)
            }
            setContextMenu(null)
          }}
          onClose={() => setContextMenu(null)}
          onCopy={() => {
            const items = contextMenu.pane === 'local' ? contextLocalSelection : contextRemoteSelection
            if (items.length) {
              onCopyItems(contextMenu.pane, items)
            }
            setContextMenu(null)
          }}
          onCopyPath={copyContextPath}
          onCut={() => {
            const items = contextMenu.pane === 'local' ? contextLocalSelection : contextRemoteSelection
            if (items.length) {
              onCutItems(contextMenu.pane, items)
            }
            setContextMenu(null)
          }}
          onDelete={() => {
            const items = contextMenu.pane === 'local' ? contextLocalSelection : contextRemoteSelection
            if (items.length) {
              onRequestDelete(contextMenu.pane, items)
            }
            setContextMenu(null)
          }}
          onDeleteFast={() => {
            const items = contextMenu.pane === 'local' ? contextLocalSelection : contextRemoteSelection
            if (items.length) {
              onRequestQuickDelete(contextMenu.pane, items)
            }
            setContextMenu(null)
          }}
          onDownload={() => {
            const items = contextRemoteSelection
            onDownloadFiles(items)
            setContextMenu(null)
          }}
          onNewFile={() => {
            onRequestNewFile(contextMenu.pane, contextMenu.pane === 'local' ? localPath : activeSession.remotePath)
            setContextMenu(null)
          }}
          onNewFolder={() => {
            onRequestNewFolder(contextMenu.pane, contextMenu.pane === 'local' ? localPath : activeSession.remotePath)
            setContextMenu(null)
          }}
          onOpen={openContextTarget}
          onPaste={() => {
            onPasteIntoPane(contextMenu.pane)
            setContextMenu(null)
          }}
          onRefresh={() => {
            onRefresh()
            setContextMenu(null)
          }}
          onRename={() => {
            const item = singleContextItem
            if (item) {
              onRequestRename(contextMenu.pane, item)
            }
            setContextMenu(null)
          }}
          onUpload={() => {
            if (contextLocalItem) {
              if (contextLocalSelection.length) {
                onUploadFiles(contextLocalSelection)
              }
            } else {
              onChooseUploadFiles()
            }
            setContextMenu(null)
          }}
        />
      ) : null}
    </div>
  )
}
