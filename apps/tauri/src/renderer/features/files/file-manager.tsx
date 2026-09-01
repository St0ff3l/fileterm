import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type DragEvent,
  type FormEvent,
  type MouseEvent,
  type PointerEvent
} from 'react'
import type {
  CommandExecutionOptions,
  CommandFolder,
  CommandTemplate,
  CommandTemplateInput,
  LocalFileItem,
  RemoteFileItem,
  SessionSnapshot,
  WorkspaceTab
} from '@fileterm/core'
import { copyText, mergeUnique, nextSelection, rangePaths } from '../../app/app-utils'
import { t } from '../../i18n'
import { WorkspaceLoadingState } from '../common/workspace-loading-state'
import type { SendScope, SessionSendTarget } from '../common/session-send-targets'
import { CommandCenter } from '../commands/command-center'
import { SshTunnelPanel } from '../workspace/ssh-tunnel-panel'
import { FileManagerContextMenuHost } from './file-manager-context-menu-host'
import { handleFileManagerKeyboardShortcuts } from './file-manager-keyboard-shortcuts'
import { FileManagerPanes } from './file-manager-panes'
import { FileManagerToolbar } from './file-manager-toolbar'
import { useFileManagerStateSync } from './use-file-manager-state-sync'
import type { RemoteFileSortState } from './file-tables'
import { matchesFileFilter, type FileFilterConfig } from './file-filter'
import { getDragPreviewIcon, sortRemoteFiles } from './file-manager-utils'
import type { FileContextMenuState, FilePane, InternalFileDrag, InternalFileDragPreview } from './file-manager-types'

export function FileManager({
  activeSession,
  activeTab,
  activeView,
  onActiveViewChange,
  commandPaneWidth,
  onCommandPaneWidthChange,
  sendTargets,
  commandFolders,
  commandTemplates,
  isBusy,
  localItems,
  localPath,
  localPanePath,
  isLocalNetworkShare,
  isLocalDirectoryLoading,
  isWorkspaceRefreshing,
  isWorkspaceSwitching,
  canPasteToLocal,
  canPasteToRemote,
  clipboardStatusText,
  localCutPaths,
  remoteCutPaths,
  onExecuteCommand,
  onSendTerminalCommand,
  onSaveTemporaryCommand,
  onUpdateCommand,
  onOpenCommandManager,
  onCopyItems,
  onCutItems,
  onClearCutState,
  onOpenLocalItem,
  onOpenLocalPath,
  onBackToLocalComputer,
  onOpenRemoteItem,
  onOpenRemotePath,
  onPasteIntoPane,
  onRefresh,
  onUploadFiles,
  onChooseUploadFiles,
  onDownloadFiles,
  onDownloadLocalNetworkFiles,
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
  activeView: 'file' | 'command' | 'tunnel'
  onActiveViewChange(view: 'file' | 'command' | 'tunnel'): void
  commandPaneWidth: number
  onCommandPaneWidthChange(width: number): void
  isBusy: boolean
  localItems: LocalFileItem[]
  localPath: string
  localPanePath: string
  isLocalNetworkShare: boolean
  isLocalDirectoryLoading: boolean
  isWorkspaceRefreshing: boolean
  isWorkspaceSwitching: boolean
  canPasteToLocal: boolean
  canPasteToRemote: boolean
  clipboardStatusText: string | null
  localCutPaths: string[]
  remoteCutPaths: string[]
  onExecuteCommand(
    commandId: string,
    args: string[],
    options: CommandExecutionOptions,
    scope: SendScope,
    selectedTabIds: string[]
  ): void
  onSendTerminalCommand(
    command: string,
    options: CommandExecutionOptions,
    scope: SendScope,
    selectedTabIds: string[]
  ): Promise<void>
  onSaveTemporaryCommand(command: string, appendCarriageReturn: boolean): Promise<boolean> | boolean | void
  onUpdateCommand(commandId: string, input: CommandTemplateInput): Promise<boolean> | boolean | void
  onOpenCommandManager(): void
  onCopyItems(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onCutItems(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onClearCutState(): void
  onOpenLocalItem(item: LocalFileItem): void
  onOpenLocalPath(path: string): void
  onBackToLocalComputer(): void
  onOpenRemoteItem(item: RemoteFileItem): void
  onOpenRemotePath(path: string): void
  onPasteIntoPane(pane: 'local' | 'remote'): void
  onRefresh(): void
  onUploadFiles(items: LocalFileItem[]): void
  onChooseUploadFiles(): void
  onDownloadFiles(items: RemoteFileItem[], targetDirectory?: string): void
  onDownloadLocalNetworkFiles(items: LocalFileItem[]): void
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
  const canUseRemoteFiles = activeSession.connected === true && !activeSession.sftpUnavailableReason
  const remoteFilesUnavailableText = activeSession.sftpUnavailableReason ?? t.remoteDisconnectedDescription
  const isSshSession = activeTab?.sessionType === 'ssh'
  const canManageTunnels = activeSession.capabilities?.tunnels === true
  const showRemoteDirectoryLoading = isRemoteDirectoryLoading || activeSession.remoteFilesLoading === true
  const showLocalDirectoryLoading = isLocalDirectoryLoading && !isWorkspaceRefreshing
  const showPaneRemoteDirectoryLoading = showRemoteDirectoryLoading && !isWorkspaceRefreshing
  const [isViewLoading, setIsViewLoading] = useState(false)
  const [localPathInput, setLocalPathInput] = useState(localPath)
  const [remotePathInput, setRemotePathInput] = useState(activeSession.remotePath)
  const [remoteSort, setRemoteSort] = useState<RemoteFileSortState>(defaultRemoteSort)
  const [selectedLocalPaths, setSelectedLocalPaths] = useState<string[]>([])
  const [selectedRemotePaths, setSelectedRemotePaths] = useState<string[]>([])
  const [internalFileDragPreview, setInternalFileDragPreview] = useState<InternalFileDragPreview | null>(null)
  const [localAnchorPath, setLocalAnchorPath] = useState<string | null>(null)
  const [remoteAnchorPath, setRemoteAnchorPath] = useState<string | null>(null)
  const [keyboardPane, setKeyboardPane] = useState<'local' | 'remote'>('remote')
  const [resetColumnsTrigger, setResetColumnsTrigger] = useState(0)
  const [contextMenu, setContextMenu] = useState<FileContextMenuState | null>(null)
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
  const internalFileDragRef = useRef<InternalFileDrag | null>(null)
  const internalFileDragRuntimeRef = useRef({ canUseRemoteFiles, localPath, onDownloadFiles, onUploadFiles })
  internalFileDragRuntimeRef.current = { canUseRemoteFiles, localPath, onDownloadFiles, onUploadFiles }
  const localScrollRef = useRef<HTMLDivElement | null>(null)
  const remoteScrollRef = useRef<HTMLDivElement | null>(null)
  const requestedInitialLocalDirectoryRef = useRef(false)

  const switchActiveView = (nextView: 'file' | 'command' | 'tunnel') => {
    if (nextView === activeView) {
      return
    }

    setIsViewLoading(true)
    onActiveViewChange(nextView)
  }

  useFileManagerStateSync({
    localItems,
    localPath,
    localPanePath,
    activeSession,
    canUseRemoteFiles,
    activeTabId: activeTab?.id ?? null,
    activeView,
    canManageTunnels,
    isViewLoading,
    setIsViewLoading,
    setLocalPathInput,
    setSelectedLocalPaths,
    setRemotePathInput,
    setSelectedRemotePaths,
    setRemoteAnchorPath,
    setContextMenu,
    setRemoteSort,
    onActiveViewChange
  })

  useEffect(() => {
    if (
      activeView !== 'file' ||
      localItems.length > 0 ||
      isLocalDirectoryLoading ||
      requestedInitialLocalDirectoryRef.current
    ) {
      return
    }

    requestedInitialLocalDirectoryRef.current = true
    onOpenLocalPath(localPath)
  }, [activeView, isLocalDirectoryLoading, localItems.length, localPath, onOpenLocalPath])

  const [localFilter, setLocalFilter] = useState<FileFilterConfig>({ query: '', mode: 'text' })
  const [remoteFilter, setRemoteFilter] = useState<FileFilterConfig>({ query: '', mode: 'text' })

  useEffect(() => {
    setLocalFilter((prev) => (prev.query ? { ...prev, query: '' } : prev))
  }, [localPanePath])

  useEffect(() => {
    setRemoteFilter((prev) => (prev.query ? { ...prev, query: '' } : prev))
  }, [activeSession.remotePath])

  const filteredLocalItems = useMemo(() => {
    if (!localFilter.query.trim()) {
      return localItems
    }
    return localItems.filter((item) => matchesFileFilter(item.name, localFilter))
  }, [localItems, localFilter])

  const filteredRemoteFiles = useMemo(() => {
    if (!canUseRemoteFiles) {
      return []
    }
    if (!remoteFilter.query.trim()) {
      return activeSession.remoteFiles
    }
    return activeSession.remoteFiles.filter((item) => matchesFileFilter(item.name, remoteFilter))
  }, [activeSession.remoteFiles, canUseRemoteFiles, remoteFilter])

  const sortedRemoteRows = useMemo(() => {
    if (!canUseRemoteFiles) {
      return []
    }

    return sortRemoteFiles(filteredRemoteFiles, remoteSort)
  }, [canUseRemoteFiles, filteredRemoteFiles, remoteSort])

  const selectedRemoteItems = activeSession.remoteFiles.filter((item) => selectedRemotePaths.includes(item.path))
  const selectedRemoteDownloadItems = selectedRemoteItems.filter((item) => item.name !== '..')
  const contextLocalItem =
    contextMenu?.pane === 'local' ? (localItems.find((item) => item.path === contextMenu.path) ?? null) : null
  const contextRemoteItem =
    contextMenu?.pane === 'remote'
      ? (activeSession.remoteFiles.find((item) => item.path === contextMenu.path) ?? null)
      : null
  const contextLocalSelection =
    contextLocalItem && selectedLocalPaths.includes(contextLocalItem.path)
      ? localItems.filter((item) => selectedLocalPaths.includes(item.path) && item.name !== '..')
      : contextLocalItem && contextLocalItem.name !== '..'
        ? [contextLocalItem]
        : []
  const contextRemoteSelection =
    contextRemoteItem && selectedRemotePaths.includes(contextRemoteItem.path)
      ? selectedRemoteItems.filter((item) => item.name !== '..')
      : contextRemoteItem && contextRemoteItem.name !== '..'
        ? [contextRemoteItem]
        : []
  const contextSelectionCount =
    contextMenu?.pane === 'local' ? contextLocalSelection.length : contextRemoteSelection.length
  const isMultiContextSelection = contextSelectionCount > 1
  const singleContextItem =
    contextMenu?.pane === 'local'
      ? contextLocalSelection.length === 1
        ? contextLocalSelection[0]
        : contextLocalItem
      : contextRemoteSelection.length === 1
        ? contextRemoteSelection[0]
        : contextRemoteItem
  const canOpenContextItem = Boolean(singleContextItem && (contextMenu?.pane !== 'remote' || canUseRemoteFiles))
  const canCopyContextItems = contextSelectionCount > 0
  const canCopyContextPath = Boolean(singleContextItem && !isMultiContextSelection)
  const canCutContextItems = contextSelectionCount > 0
  const canDownloadContextItems =
    contextMenu?.pane === 'local'
      ? isLocalNetworkShare && contextLocalSelection.length > 0
      : canUseRemoteFiles && contextRemoteSelection.length > 0
  const canPasteIntoContextPane =
    contextMenu?.pane === 'local' ? canPasteToLocal : canUseRemoteFiles && canPasteToRemote
  const canUploadContextItems =
    canUseRemoteFiles &&
    (Boolean(!isMultiContextSelection && contextLocalSelection.length) ||
      Boolean(!isMultiContextSelection && contextMenu?.pane === 'remote'))
  const canCreateFromContext = !isMultiContextSelection && (contextMenu?.pane !== 'remote' || canUseRemoteFiles)
  const canRenameContextItem = Boolean(
    singleContextItem &&
    !isMultiContextSelection &&
    singleContextItem.name !== '..' &&
    (contextMenu?.pane !== 'remote' || canUseRemoteFiles)
  )
  const canChangeContextPermissions = Boolean(
    singleContextItem &&
    !isMultiContextSelection &&
    singleContextItem.name !== '..' &&
    (contextMenu?.pane !== 'remote' || canUseRemoteFiles)
  )

  const keyboardSelection =
    keyboardPane === 'local'
      ? localItems.filter((item) => selectedLocalPaths.includes(item.path) && item.name !== '..')
      : selectedRemoteItems.filter((item) => item.name !== '..')
  const canPasteFromKeyboard = keyboardPane === 'local' ? canPasteToLocal : canPasteToRemote

  const beginInternalFileDrag = (
    sourcePane: FilePane,
    item: LocalFileItem | RemoteFileItem,
    event: PointerEvent<HTMLElement>
  ) => {
    if (event.button !== 0 || !event.isPrimary || item.name === '..') {
      return
    }

    const rows = sourcePane === 'local' ? localItems : activeSession.remoteFiles
    const selectedPaths = sourcePane === 'local' ? selectedLocalPaths : selectedRemotePaths
    const items = (
      selectedPaths.includes(item.path) ? rows.filter((row) => selectedPaths.includes(row.path)) : [item]
    ).filter((row) => row.name !== '..')

    if (!items.length) {
      return
    }

    internalFileDragRef.current = {
      sourcePane,
      items,
      startX: event.clientX,
      startY: event.clientY,
      pointerId: event.pointerId,
      active: false
    }
  }

  useEffect(() => {
    const clearInternalFileDrag = () => {
      internalFileDragRef.current = null
      setInternalFileDragPreview(null)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }

    const handlePointerMove = (event: globalThis.PointerEvent) => {
      const drag = internalFileDragRef.current
      if (!drag || drag.pointerId !== event.pointerId) {
        return
      }

      if (event.buttons === 0) {
        clearInternalFileDrag()
        return
      }

      if (!drag.active) {
        const deltaX = event.clientX - drag.startX
        const deltaY = event.clientY - drag.startY
        if (deltaX * deltaX + deltaY * deltaY < 36) {
          return
        }
        drag.active = true
        suppressNextSelectionClick.current = true
        document.body.style.cursor = 'grabbing'
        document.body.style.userSelect = 'none'
      }

      event.preventDefault()
      setInternalFileDragPreview({
        names: drag.items.map((item) => item.name),
        icon: getDragPreviewIcon(drag.items),
        x: event.clientX,
        y: event.clientY
      })
    }

    const handlePointerUp = (event: globalThis.PointerEvent) => {
      const drag = internalFileDragRef.current
      if (!drag || drag.pointerId !== event.pointerId) {
        return
      }

      if (!drag.active) {
        clearInternalFileDrag()
        return
      }

      event.preventDefault()
      const target = document
        .elementFromPoint(event.clientX, event.clientY)
        ?.closest<HTMLElement>('.local-pane, .remote-pane')
      const targetPane: FilePane | null = target?.classList.contains('local-pane')
        ? 'local'
        : target?.classList.contains('remote-pane')
          ? 'remote'
          : null

      clearInternalFileDrag()

      const runtime = internalFileDragRuntimeRef.current
      if (targetPane === 'remote' && drag.sourcePane === 'local' && runtime.canUseRemoteFiles) {
        runtime.onUploadFiles(drag.items as LocalFileItem[])
      } else if (targetPane === 'local' && drag.sourcePane === 'remote' && runtime.canUseRemoteFiles) {
        runtime.onDownloadFiles(drag.items as RemoteFileItem[], runtime.localPath)
      }
    }

    const handlePointerCancel = () => {
      if (internalFileDragRef.current) {
        clearInternalFileDrag()
      }
    }

    window.addEventListener('pointermove', handlePointerMove, true)
    window.addEventListener('pointerup', handlePointerUp, true)
    window.addEventListener('pointercancel', handlePointerCancel, true)
    window.addEventListener('blur', handlePointerCancel)
    window.addEventListener('mouseleave', handlePointerCancel)
    return () => {
      window.removeEventListener('pointermove', handlePointerMove, true)
      window.removeEventListener('pointerup', handlePointerUp, true)
      window.removeEventListener('pointercancel', handlePointerCancel, true)
      window.removeEventListener('blur', handlePointerCancel)
      window.removeEventListener('mouseleave', handlePointerCancel)
      clearInternalFileDrag()
    }
  }, [])

  const submitLocalPath = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    onOpenLocalPath(localPathInput.trim() || localPath)
  }

  const submitRemotePath = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!canUseRemoteFiles) {
      return
    }
    const targetPath = remotePathInput.trim() || activeSession.remotePath
    onOpenRemotePath(targetPath)
  }

  const handleRemotePaneDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault()
    event.stopPropagation()
    if (!canUseRemoteFiles) {
      return
    }
    onDropUpload(event)
  }

  const handleLocalPaneDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault()
    event.stopPropagation()
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
      rows: filteredLocalItems
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
      rows: sortedRemoteRows
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
    setSelectedLocalPaths(
      mergeUnique([...session.basePaths, ...rangePaths(filteredLocalItems, session.startPath, item.path)])
    )
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
    setSelectedRemotePaths(
      mergeUnique([...session.basePaths, ...rangePaths(sortedRemoteRows, session.startPath, item.path)])
    )
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

  useEffect(() => {
    const handleMouseMove = (event: globalThis.MouseEvent) => {
      if (!isResizingFileSplit.current || !splitRef.current) return

      const rect = splitRef.current.getBoundingClientRect()
      const minLocalWidth = 180
      const minRemoteWidth = 320
      const maxLocalWidth = Math.max(minLocalWidth, rect.width - minRemoteWidth)
      const nextWidth = Math.min(maxLocalWidth, Math.max(minLocalWidth, event.clientX - rect.left))
      onCommandPaneWidthChange(nextWidth)
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
      onKeyDown={(event) =>
        handleFileManagerKeyboardShortcuts(event, {
          canPaste: canPasteFromKeyboard,
          keyboardPane,
          keyboardSelection,
          onClearCutState,
          onCopyItems,
          onCutItems,
          onPasteIntoPane
        })
      }
      tabIndex={0}
      style={{ '--local-pane-width': `${commandPaneWidth}px` } as CSSProperties}
    >
      <FileManagerToolbar
        activeSession={activeSession}
        activeTab={activeTab}
        activeView={activeView}
        canManageTunnels={canManageTunnels}
        canUseRemoteFiles={canUseRemoteFiles}
        clipboardStatusText={clipboardStatusText}
        isSshSession={isSshSession}
        onChooseUploadFiles={onChooseUploadFiles}
        onDownloadFiles={onDownloadFiles}
        onOpenCommandManager={onOpenCommandManager}
        onRefresh={onRefresh}
        onToggleRemoteFileAccessMode={onToggleRemoteFileAccessMode}
        remoteFileAccessMode={remoteFileAccessMode}
        selectedRemoteDownloadItems={selectedRemoteDownloadItems}
        setResetColumnsTrigger={setResetColumnsTrigger}
        switchActiveView={switchActiveView}
      />
      {activeView === 'tunnel' && canManageTunnels && activeTab ? (
        <div className="workspace-view-content">
          <SshTunnelPanel tabId={activeTab.id} />
          {isViewLoading || isWorkspaceSwitching ? (
            <WorkspaceLoadingState className="workspace-loading-state--overlay" />
          ) : null}
        </div>
      ) : activeView === 'command' && isSshSession ? (
        <div className="workspace-view-content">
          <CommandCenter
            activeTab={activeTab}
            commandFolders={commandFolders}
            commandTemplates={commandTemplates}
            isBusy={isBusy}
            sendTargets={sendTargets}
            onExecute={onExecuteCommand}
            onSendTerminalCommand={onSendTerminalCommand}
            onSaveTemporaryCommand={onSaveTemporaryCommand}
            onUpdateCommand={onUpdateCommand}
            paneWidth={commandPaneWidth}
            onPaneWidthChange={onCommandPaneWidthChange}
          />
          {isViewLoading || isWorkspaceSwitching ? (
            <WorkspaceLoadingState className="workspace-loading-state--overlay" />
          ) : null}
        </div>
      ) : (
        <FileManagerPanes
          activeSession={activeSession}
          beginInternalFileDrag={beginInternalFileDrag}
          canUseRemoteFiles={canUseRemoteFiles}
          didDragSelect={didDragSelect}
          extendLocalDragSelection={extendLocalDragSelection}
          extendRemoteDragSelection={extendRemoteDragSelection}
          filteredLocalItems={filteredLocalItems}
          filteredRemoteFiles={filteredRemoteFiles}
          focusContainer={focusContainer}
          handleLocalPaneDrop={handleLocalPaneDrop}
          handleRemotePaneDrop={handleRemotePaneDrop}
          internalFileDragPreview={internalFileDragPreview}
          isLocalDirectoryLoading={isLocalDirectoryLoading}
          isLocalNetworkShare={isLocalNetworkShare}
          isResizingFileSplit={isResizingFileSplit}
          isSelectingLocal={isSelectingLocal}
          isSelectingRemote={isSelectingRemote}
          isSshSession={isSshSession}
          isViewLoading={isViewLoading}
          isWorkspaceRefreshing={isWorkspaceRefreshing}
          isWorkspaceSwitching={isWorkspaceSwitching}
          localAnchorPath={localAnchorPath}
          localDragSelection={localDragSelection}
          localFilter={localFilter}
          localItems={localItems}
          localPath={localPath}
          localPathInput={localPathInput}
          localScrollRef={localScrollRef}
          localCutPaths={localCutPaths}
          onBackToLocalComputer={onBackToLocalComputer}
          onOpenLocalItem={onOpenLocalItem}
          onOpenRemoteItem={onOpenRemoteItem}
          onToggleFollowShellCwd={onToggleFollowShellCwd}
          remoteAnchorPath={remoteAnchorPath}
          remoteDragSelection={remoteDragSelection}
          remoteFileAccessMode={remoteFileAccessMode}
          remoteFilesUnavailableText={remoteFilesUnavailableText}
          remoteFilter={remoteFilter}
          remotePathInput={remotePathInput}
          remoteScrollRef={remoteScrollRef}
          remoteCutPaths={remoteCutPaths}
          remoteSort={remoteSort}
          resetColumnsTrigger={resetColumnsTrigger}
          selectLocalItem={selectLocalItem}
          selectRemoteItem={selectRemoteItem}
          selectedLocalPaths={selectedLocalPaths}
          selectedRemotePaths={selectedRemotePaths}
          setContextMenu={setContextMenu}
          setKeyboardPane={setKeyboardPane}
          setLocalAnchorPath={setLocalAnchorPath}
          setLocalFilter={setLocalFilter}
          setLocalPathInput={setLocalPathInput}
          setRemoteAnchorPath={setRemoteAnchorPath}
          setRemoteFilter={setRemoteFilter}
          setRemotePathInput={setRemotePathInput}
          setRemoteSort={setRemoteSort}
          setSelectedLocalPaths={setSelectedLocalPaths}
          setSelectedRemotePaths={setSelectedRemotePaths}
          showLocalDirectoryLoading={showLocalDirectoryLoading}
          showPaneRemoteDirectoryLoading={showPaneRemoteDirectoryLoading}
          showRemoteDirectoryLoading={showRemoteDirectoryLoading}
          sortedRemoteRows={sortedRemoteRows}
          submitLocalPath={submitLocalPath}
          submitRemotePath={submitRemotePath}
          suppressNextClearClick={suppressNextClearClick}
          suppressNextSelectionClick={suppressNextSelectionClick}
          splitRef={splitRef}
        />
      )}
      {contextMenu ? (
        <FileManagerContextMenuHost
          activeSessionRemotePath={activeSession.remotePath}
          canChangeContextPermissions={canChangeContextPermissions}
          canCopyContextItems={canCopyContextItems}
          canCopyContextPath={canCopyContextPath}
          canCreateFromContext={canCreateFromContext}
          canCutContextItems={canCutContextItems}
          canDownloadContextItems={canDownloadContextItems}
          canOpenContextItem={canOpenContextItem}
          canPasteIntoContextPane={canPasteIntoContextPane}
          canQuickDelete={canUseRemoteFiles && contextMenu.pane === 'remote' && activeTab?.sessionType === 'ssh'}
          canRenameContextItem={canRenameContextItem}
          canUploadContextItems={canUploadContextItems}
          contextLocalItem={contextLocalItem}
          contextLocalSelection={contextLocalSelection}
          contextMenu={contextMenu}
          contextRemoteItem={contextRemoteItem}
          contextRemoteSelection={contextRemoteSelection}
          localPath={localPath}
          onChooseUploadFiles={onChooseUploadFiles}
          onClose={() => setContextMenu(null)}
          onCopyItems={onCopyItems}
          onCopyPath={copyContextPath}
          onCutItems={onCutItems}
          onDownloadFiles={onDownloadFiles}
          onDownloadLocalNetworkFiles={onDownloadLocalNetworkFiles}
          onOpenLocalItem={onOpenLocalItem}
          onOpenRemoteItem={onOpenRemoteItem}
          onOpenContextTarget={openContextTarget}
          onPasteIntoPane={onPasteIntoPane}
          onRefresh={onRefresh}
          onRequestChangePermissions={onRequestChangePermissions}
          onRequestDelete={onRequestDelete}
          onRequestNewFile={onRequestNewFile}
          onRequestNewFolder={onRequestNewFolder}
          onRequestQuickDelete={onRequestQuickDelete}
          onRequestRename={onRequestRename}
          onUploadFiles={onUploadFiles}
          singleContextItem={singleContextItem}
          setResetColumnsTrigger={setResetColumnsTrigger}
        />
      ) : null}
    </div>
  )
}
