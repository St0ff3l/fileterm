import { useEffect, useRef, useState, type CSSProperties, type DragEvent, type FormEvent, type MouseEvent } from 'react'
import type {
  CommandExecutionOptions,
  CommandFolder,
  CommandTemplate,
  LocalFileItem,
  RemoteFileItem,
  SessionSnapshot,
  WorkspaceTab
} from '@termdock/core'
import {
  copyText,
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
import { CommandCenter } from '../commands/CommandCenter'
import { FileContextMenu } from './FileContextMenu'
import { FileTable, LocalFileTable, PanePathBar } from './FileTables'

export function FileManager({
  activeSession,
  activeTab,
  tabs,
  commandFolders,
  commandTemplates,
  isBusy,
  localItems,
  localPath,
  onExecuteCommand,
  onOpenCommandManager,
  onOpenLocalItem,
  onOpenLocalPath,
  onOpenRemoteItem,
  onOpenRemotePath,
  onRefresh,
  onUploadFiles,
  onChooseUploadFiles,
  onDownloadFiles,
  onDropUpload
}: {
  activeSession: SessionSnapshot
  activeTab: WorkspaceTab | null
  tabs: WorkspaceTab[]
  commandFolders: CommandFolder[]
  commandTemplates: CommandTemplate[]
  isBusy: boolean
  localItems: LocalFileItem[]
  localPath: string
  onExecuteCommand(commandId: string, args: string[], options: CommandExecutionOptions, scope: 'current' | 'all-ssh'): void
  onOpenCommandManager(): void
  onOpenLocalItem(item: LocalFileItem): void
  onOpenLocalPath(path: string): void
  onOpenRemoteItem(item: RemoteFileItem): void
  onOpenRemotePath(path: string): void
  onRefresh(): void
  onUploadFiles(items: LocalFileItem[]): void
  onChooseUploadFiles(): void
  onDownloadFiles(items: RemoteFileItem[], targetDirectory?: string): void
  onDropUpload(event: DragEvent<HTMLDivElement>): void
}) {
  const [activeView, setActiveView] = useState<'file' | 'command'>('file')
  const [localPaneWidth, setLocalPaneWidth] = useState(230)
  const [localPathInput, setLocalPathInput] = useState(localPath)
  const [remotePathInput, setRemotePathInput] = useState(activeSession.remotePath)
  const [selectedLocalPaths, setSelectedLocalPaths] = useState<string[]>([])
  const [selectedRemotePaths, setSelectedRemotePaths] = useState<string[]>([])
  const [localAnchorPath, setLocalAnchorPath] = useState<string | null>(null)
  const [remoteAnchorPath, setRemoteAnchorPath] = useState<string | null>(null)
  const [contextMenu, setContextMenu] = useState<{
    pane: 'local' | 'remote'
    x: number
    y: number
    path: string
  } | null>(null)
  const splitRef = useRef<HTMLDivElement | null>(null)
  const isResizingFileSplit = useRef(false)
  const isSelectingLocal = useRef(false)
  const isSelectingRemote = useRef(false)
  const didDragSelect = useRef(false)
  const suppressNextSelectionClick = useRef(false)
  const suppressNextClearClick = useRef(false)
  const localDragSelection = useRef<{ basePaths: string[]; startPath: string } | null>(null)
  const remoteDragSelection = useRef<{ basePaths: string[]; startPath: string } | null>(null)

  useEffect(() => {
    setLocalPathInput(localPath)
    setSelectedLocalPaths((prev) => prev.filter((selectedPath) => localItems.some((item) => item.path === selectedPath)))
  }, [localItems, localPath])

  useEffect(() => {
    setRemotePathInput(activeSession.remotePath)
    setSelectedRemotePaths((prev) => prev.filter((selectedPath) => activeSession.remoteFiles.some((item) => item.path === selectedPath)))
  }, [activeSession.remoteFiles, activeSession.remotePath])

  const selectedRemoteItems = activeSession.remoteFiles.filter((item) => selectedRemotePaths.includes(item.path))
  const selectedRemoteFileItems = selectedRemoteItems.filter((item) => item.type === 'file')
  const contextLocalItem = contextMenu?.pane === 'local'
    ? localItems.find((item) => item.path === contextMenu.path) ?? null
    : null
  const contextRemoteItem = contextMenu?.pane === 'remote'
    ? activeSession.remoteFiles.find((item) => item.path === contextMenu.path) ?? null
    : null

  const submitLocalPath = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    onOpenLocalPath(localPathInput.trim() || localPath)
  }

  const submitRemotePath = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const targetPath = remotePathInput.trim() || activeSession.remotePath
    onOpenRemotePath(targetPath)
  }

  const handleRemotePaneDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault()
    event.stopPropagation()

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
    setSelectedLocalPaths(mergeUnique([
      ...session.basePaths,
      ...rangePaths(localItems, session.startPath, item.path)
    ]))
  }

  const extendRemoteDragSelection = (item: RemoteFileItem) => {
    const session = remoteDragSelection.current
    if (!isSelectingRemote.current || !session) return
    didDragSelect.current = true
    setSelectedRemotePaths(mergeUnique([
      ...session.basePaths,
      ...rangePaths(activeSession.remoteFiles, session.startPath, item.path)
    ]))
  }

  const openContextTarget = () => {
    if (contextLocalItem) {
      onOpenLocalItem(contextLocalItem)
    }
    if (contextRemoteItem) {
      onOpenRemoteItem(contextRemoteItem)
    }
    setContextMenu(null)
  }

  const copyContextPath = () => {
    const targetPath = contextLocalItem?.path ?? contextRemoteItem?.path
    if (targetPath) {
      copyText(targetPath)
    }
    setContextMenu(null)
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
    <div className="file-manager" onClick={() => setContextMenu(null)}>
      <div className="file-tabs">
        <button className={activeView === 'file' ? 'active' : ''} type="button" onClick={() => setActiveView('file')}>{t.file}</button>
        <button className={activeView === 'command' ? 'active' : ''} type="button" onClick={() => setActiveView('command')}>{t.command}</button>
        <span className="file-current-path">
          {activeView === 'file' ? activeSession.remotePath : `${t.commandQuickLaunch} (${activeTab?.sessionType === 'ssh' ? t.send : t.commandSshOnly})`}
        </span>
        {activeView === 'file' ? (
          <div className="file-tab-actions">
            <button title={t.refresh} type="button" onClick={onRefresh}><AppIcon name="refresh" /></button>
            <button title={t.downloadTo} type="button" disabled={!selectedRemoteFileItems.length} onClick={() => onDownloadFiles(selectedRemoteFileItems)}>
              <AppIcon name="download" />
            </button>
            <button title={t.upload} type="button" onClick={onChooseUploadFiles}><AppIcon name="upload" /></button>
          </div>
        ) : (
          <div className="file-tab-actions">
            <button className="flat-button compact command-manager-launch" type="button" onClick={onOpenCommandManager}>{t.commandManager}</button>
          </div>
        )}
      </div>
      {activeView === 'command' ? (
        <CommandCenter
          activeTab={activeTab}
          commandFolders={commandFolders}
          commandTemplates={commandTemplates}
          isBusy={isBusy}
          tabs={tabs}
          onExecute={onExecuteCommand}
        />
      ) : (
      <div className="file-split" ref={splitRef} style={{ '--local-pane-width': `${localPaneWidth}px` } as CSSProperties}>
        <div
          className="local-pane"
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
            className="file-table-shell"
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
          className="remote-pane"
          onClick={(event) => {
            if (event.target === event.currentTarget) {
              setSelectedRemotePaths([])
              setRemoteAnchorPath(null)
            }
          }}
          onDragOver={(event) => {
            event.preventDefault()
            event.dataTransfer.dropEffect = 'copy'
          }}
          onDrop={handleRemotePaneDrop}
        >
          <PanePathBar hint={t.dragUpload} label={t.remoteHost} value={remotePathInput} onChange={setRemotePathInput} onSubmit={submitRemotePath} />
          <div
            className="file-table-shell"
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
              rows={activeSession.remoteFiles}
              selectedPaths={selectedRemotePaths}
              onDragItem={(event, item) => {
                event.dataTransfer.effectAllowed = 'copy'
                const payload = selectedRemotePaths.includes(item.path) ? selectedRemotePaths : [item.path]
                const previewItems = activeSession.remoteFiles.filter((row) => payload.includes(row.path))
                event.dataTransfer.setData(remoteFileDragType, JSON.stringify(payload))
                setFileDragPreview(event, previewItems.map((row) => row.name))
              }}
              onOpenItem={onOpenRemoteItem}
              onContextItem={(event, item) => {
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
              onSelectItem={selectRemoteItem}
              onSelectionDragStart={(event, item) => {
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
                  rows: activeSession.remoteFiles
                }))
                setRemoteAnchorPath(startPath)
              }}
              onSelectionDragEnter={extendRemoteDragSelection}
            />
          </div>
        </div>
      </div>
      )}
      {contextMenu ? (
        <FileContextMenu
          item={contextLocalItem ?? contextRemoteItem}
          pane={contextMenu.pane}
          position={{ x: contextMenu.x, y: contextMenu.y }}
          onClose={() => setContextMenu(null)}
          onCopyPath={copyContextPath}
          onDownload={() => {
            const items = contextRemoteItem && selectedRemotePaths.includes(contextRemoteItem.path)
              ? selectedRemoteItems
              : contextRemoteItem ? [contextRemoteItem] : []
            onDownloadFiles(items)
            setContextMenu(null)
          }}
          onOpen={openContextTarget}
          onRefresh={() => {
            onRefresh()
            setContextMenu(null)
          }}
          onUpload={() => {
            if (contextLocalItem) {
              const items = selectedLocalPaths.includes(contextLocalItem.path)
                ? localItems.filter((item) => selectedLocalPaths.includes(item.path) && item.name !== '..')
                : contextLocalItem.name === '..' ? [] : [contextLocalItem]
              if (items.length) {
                onUploadFiles(items)
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
