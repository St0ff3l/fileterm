import type {
  Dispatch,
  DragEvent,
  FormEvent,
  MouseEvent,
  MutableRefObject,
  PointerEvent,
  RefObject,
  SetStateAction
} from 'react'
import type { LocalFileItem, RemoteFileItem, SessionSnapshot } from '@fileterm/core'
import { nextSelection, WINDOWS_DRIVES_PATH } from '../../app/app-utils'
import { APP_EVENT, dispatchAppEvent } from '../../lib/app-events'
import { t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { WorkspaceLoadingState } from '../common/workspace-loading-state'
import { RemoteCapabilityPanel } from './remote-capability-panel'
import { FileTable, LocalFileTable, PaneFilterBar, PanePathBar, type RemoteFileSortState } from './file-tables'
import type { FileFilterConfig } from './file-filter'
import type { FileContextMenuState, InternalFileDragPreview, StringListSetter } from './file-manager-types'

type DragSelection = {
  basePaths: string[]
  startPath: string | null
}

export function FileManagerPanes({
  activeSession,
  canUseRemoteFiles,
  didDragSelect,
  extendLocalDragSelection,
  extendRemoteDragSelection,
  filteredLocalItems,
  filteredRemoteFiles,
  focusContainer,
  handleLocalPaneDrop,
  handleRemotePaneDrop,
  internalFileDragPreview,
  isLocalDirectoryLoading,
  isLocalNetworkShare,
  isResizingFileSplit,
  isSelectingLocal,
  isSelectingRemote,
  isSshSession,
  isViewLoading,
  isWorkspaceRefreshing,
  isWorkspaceSwitching,
  localAnchorPath,
  localDragSelection,
  localFilter,
  localItems,
  localPath,
  localPathInput,
  localScrollRef,
  localCutPaths,
  onBackToLocalComputer,
  onOpenLocalItem,
  onOpenRemoteItem,
  onToggleFollowShellCwd,
  remoteAnchorPath,
  remoteDragSelection,
  remoteFileAccessMode,
  remoteFilesUnavailableText,
  remoteFilter,
  remotePathInput,
  remoteScrollRef,
  remoteCutPaths,
  remoteSort,
  resetColumnsTrigger,
  selectLocalItem,
  selectRemoteItem,
  selectedLocalPaths,
  selectedRemotePaths,
  setContextMenu,
  setKeyboardPane,
  setLocalAnchorPath,
  setLocalFilter,
  setLocalPathInput,
  setRemoteAnchorPath,
  setRemoteFilter,
  setRemotePathInput,
  setRemoteSort,
  setSelectedLocalPaths,
  setSelectedRemotePaths,
  showLocalDirectoryLoading,
  showPaneRemoteDirectoryLoading,
  showRemoteDirectoryLoading,
  sortedRemoteRows,
  submitLocalPath,
  submitRemotePath,
  suppressNextClearClick,
  suppressNextSelectionClick,
  splitRef,
  beginInternalFileDrag
}: {
  activeSession: SessionSnapshot
  canUseRemoteFiles: boolean
  didDragSelect: MutableRefObject<boolean>
  extendLocalDragSelection(item: LocalFileItem): void
  extendRemoteDragSelection(item: RemoteFileItem): void
  filteredLocalItems: LocalFileItem[]
  filteredRemoteFiles: RemoteFileItem[]
  focusContainer(): void
  handleLocalPaneDrop(event: DragEvent<HTMLDivElement>): void
  handleRemotePaneDrop(event: DragEvent<HTMLDivElement>): void
  internalFileDragPreview: InternalFileDragPreview | null
  isLocalDirectoryLoading: boolean
  isLocalNetworkShare: boolean
  isResizingFileSplit: MutableRefObject<boolean>
  isSelectingLocal: MutableRefObject<boolean>
  isSelectingRemote: MutableRefObject<boolean>
  isSshSession: boolean
  isViewLoading: boolean
  isWorkspaceRefreshing: boolean
  isWorkspaceSwitching: boolean
  localAnchorPath: string | null
  localDragSelection: MutableRefObject<DragSelection | null>
  localFilter: FileFilterConfig
  localItems: LocalFileItem[]
  localPath: string
  localPathInput: string
  localScrollRef: RefObject<HTMLDivElement | null>
  localCutPaths: string[]
  onBackToLocalComputer(): void
  onOpenLocalItem(item: LocalFileItem): void
  onOpenRemoteItem(item: RemoteFileItem): void
  onToggleFollowShellCwd(): void
  remoteAnchorPath: string | null
  remoteDragSelection: MutableRefObject<DragSelection | null>
  remoteFileAccessMode: 'user' | 'root'
  remoteFilesUnavailableText: string
  remoteFilter: FileFilterConfig
  remotePathInput: string
  remoteScrollRef: RefObject<HTMLDivElement | null>
  remoteCutPaths: string[]
  remoteSort: RemoteFileSortState
  resetColumnsTrigger: number
  selectLocalItem(event: MouseEvent<HTMLTableRowElement>, item: LocalFileItem): void
  selectRemoteItem(event: MouseEvent<HTMLTableRowElement>, item: RemoteFileItem): void
  selectedLocalPaths: string[]
  selectedRemotePaths: string[]
  setContextMenu: Dispatch<SetStateAction<FileContextMenuState | null>>
  setKeyboardPane(value: 'local' | 'remote'): void
  setLocalAnchorPath(value: string | null): void
  setLocalFilter: Dispatch<SetStateAction<FileFilterConfig>>
  setLocalPathInput(value: string): void
  setRemoteAnchorPath(value: string | null): void
  setRemoteFilter: Dispatch<SetStateAction<FileFilterConfig>>
  setRemotePathInput(value: string): void
  setRemoteSort: Dispatch<SetStateAction<RemoteFileSortState>>
  setSelectedLocalPaths: StringListSetter
  setSelectedRemotePaths: StringListSetter
  showLocalDirectoryLoading: boolean
  showPaneRemoteDirectoryLoading: boolean
  showRemoteDirectoryLoading: boolean
  sortedRemoteRows: RemoteFileItem[]
  submitLocalPath(event: FormEvent<HTMLFormElement>): void
  submitRemotePath(event: FormEvent<HTMLFormElement>): void
  suppressNextClearClick: MutableRefObject<boolean>
  suppressNextSelectionClick: MutableRefObject<boolean>
  splitRef: RefObject<HTMLDivElement | null>
  beginInternalFileDrag(
    sourcePane: 'local' | 'remote',
    item: LocalFileItem | RemoteFileItem,
    event: PointerEvent<HTMLElement>
  ): void
}) {
  return (
    <div
      aria-busy={
        isViewLoading ||
        isWorkspaceRefreshing ||
        isWorkspaceSwitching ||
        isLocalDirectoryLoading ||
        showRemoteDirectoryLoading
      }
      className="file-split"
      ref={splitRef}
    >
      {internalFileDragPreview ? (
        <div
          aria-hidden="true"
          className="file-drag-preview"
          style={{
            left: internalFileDragPreview.x + 14,
            top: internalFileDragPreview.y + 14
          }}
        >
          <span className="file-drag-preview-icon">
            <AppIcon name={internalFileDragPreview.icon} size={14} />
          </span>
          <span>
            {internalFileDragPreview.names.slice(0, 2).join(internalFileDragPreview.names.length > 1 ? ', ' : '')}
            {internalFileDragPreview.names.length > 2
              ? ` ${t.moreItemsPrefix ? `${t.moreItemsPrefix} ` : ''}${internalFileDragPreview.names.length} ${t.itemsSuffix}`
              : ''}
          </span>
        </div>
      ) : null}
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
        <PanePathBar
          label={isLocalNetworkShare ? t.networkShare : t.localComputer}
          value={localPathInput === WINDOWS_DRIVES_PATH ? t.localComputer : localPathInput}
          onChange={setLocalPathInput}
          onSubmit={submitLocalPath}
          action={
            isLocalNetworkShare ? (
              <button
                type="button"
                className="pane-path-bar-action"
                title={t.backToThisPC}
                onClick={onBackToLocalComputer}
              >
                {t.localComputer}
              </button>
            ) : null
          }
        />
        <PaneFilterBar
          filter={localFilter}
          matchCount={filteredLocalItems.filter((i) => i.name !== '..').length}
          totalCount={localItems.filter((i) => i.name !== '..').length}
          onFilterChange={setLocalFilter}
          onClear={() => setLocalFilter((prev) => ({ ...prev, query: '' }))}
        />
        <div className="file-table-scroll-region">
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
              rows={filteredLocalItems}
              selectedPaths={selectedLocalPaths}
              onPointerDragStart={(event, item) => beginInternalFileDrag('local', item, event)}
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
                setSelectedLocalPaths(
                  nextSelection({
                    anchorPath: localAnchorPath,
                    currentSelection: selectedLocalPaths,
                    event,
                    itemPath: item.path,
                    rows: filteredLocalItems
                  })
                )
                setLocalAnchorPath(startPath)
              }}
              onSelectionDragEnter={extendLocalDragSelection}
            />
          </div>
          <VerticalScrollbar ariaLabel={t.scrollLocalFiles} scrollRef={localScrollRef} topInset={24} />
        </div>
        {showLocalDirectoryLoading ? (
          <WorkspaceLoadingState className="workspace-loading-state--overlay" label={t.loadingLocalDirectory} />
        ) : null}
      </div>
      <div
        className="file-split-resizer"
        onMouseDown={(event) => {
          event.preventDefault()
          window.getSelection()?.removeAllRanges()
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
          // Record the pane that an OS file drop is over so the bridge can
          // route its absolute paths to the correct upload target.
          dispatchAppEvent(APP_EVENT.tauriRemoteDragOver)
          if (canUseRemoteFiles) {
            event.dataTransfer.dropEffect = 'copy'
          }
        }}
        onDrop={handleRemotePaneDrop}
      >
        <div aria-busy={showRemoteDirectoryLoading} className="remote-pane-content">
          <PanePathBar
            disabled={!canUseRemoteFiles}
            hint={canUseRemoteFiles ? t.dragUpload : remoteFilesUnavailableText}
            label={t.remoteHost}
            value={remotePathInput}
            action={
              <div className="pane-path-bar-actions">
                {isSshSession ? (
                  <button
                    aria-pressed={activeSession.followShellCwd !== false}
                    className={`follow-shell-cwd-toggle ${activeSession.followShellCwd !== false ? 'is-active' : ''}`}
                    disabled={!canUseRemoteFiles}
                    onClick={onToggleFollowShellCwd}
                    title={
                      activeSession.shellCwd ? `${t.shellCwd}: ${activeSession.shellCwd}` : t.followShellCwdUnavailable
                    }
                    type="button"
                  >
                    {t.followShellCwd}
                  </button>
                ) : null}
                <RemoteCapabilityPanel capabilities={activeSession.remoteCapabilities} />
              </div>
            }
            onChange={setRemotePathInput}
            onSubmit={submitRemotePath}
          />
          <PaneFilterBar
            disabled={!canUseRemoteFiles}
            filter={remoteFilter}
            matchCount={filteredRemoteFiles.filter((i) => i.name !== '..').length}
            totalCount={activeSession.remoteFiles.filter((i) => i.name !== '..').length}
            onFilterChange={setRemoteFilter}
            onClear={() => setRemoteFilter((prev) => ({ ...prev, query: '' }))}
          />
          <div className="remote-file-table-region">
            <div
              ref={remoteScrollRef}
              className="file-table-shell remote-file-table-shell"
              onContextMenu={(event) => {
                if (!canUseRemoteFiles) return
                if (event.target !== event.currentTarget) return
                event.preventDefault()
                event.stopPropagation()
                setSelectedRemotePaths([])
                setRemoteAnchorPath(null)
                setContextMenu({ pane: 'remote', x: event.clientX, y: event.clientY, path: null })
              }}
              onMouseDown={(event) => {
                if (!canUseRemoteFiles) return
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
                emptyText={canUseRemoteFiles ? t.emptyFiles : remoteFilesUnavailableText}
                rows={sortedRemoteRows}
                sortState={remoteSort}
                selectedPaths={selectedRemotePaths}
                resetColumnsTrigger={resetColumnsTrigger}
                onToggleSort={(field) => {
                  setRemoteSort((current) =>
                    current.field === field
                      ? { field, direction: current.direction === 'asc' ? 'desc' : 'asc' }
                      : { field, direction: 'asc' }
                  )
                }}
                onPointerDragStart={(event, item) => {
                  if (canUseRemoteFiles) {
                    beginInternalFileDrag('remote', item, event)
                  }
                }}
                onOpenItem={(item) => {
                  if (canUseRemoteFiles) {
                    onOpenRemoteItem(item)
                  }
                }}
                onContextItem={(event, item) => {
                  if (!canUseRemoteFiles) return
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
                  if (canUseRemoteFiles) {
                    selectRemoteItem(event, item)
                  }
                }}
                onSelectionDragStart={(event, item) => {
                  if (!canUseRemoteFiles) return
                  setKeyboardPane('remote')
                  isSelectingRemote.current = true
                  didDragSelect.current = false
                  const startPath = event.shiftKey && remoteAnchorPath ? remoteAnchorPath : item.path
                  const basePaths = event.metaKey || event.ctrlKey ? selectedRemotePaths : []
                  remoteDragSelection.current = { basePaths, startPath }
                  suppressNextSelectionClick.current = true
                  setSelectedRemotePaths(
                    nextSelection({
                      anchorPath: remoteAnchorPath,
                      currentSelection: selectedRemotePaths,
                      event,
                      itemPath: item.path,
                      rows: sortedRemoteRows
                    })
                  )
                  setRemoteAnchorPath(startPath)
                }}
                onSelectionDragEnter={(item) => {
                  if (canUseRemoteFiles) {
                    extendRemoteDragSelection(item)
                  }
                }}
              />
            </div>
            <VerticalScrollbar ariaLabel={t.scrollRemoteFiles} scrollRef={remoteScrollRef} topInset={24} />
          </div>
          {showPaneRemoteDirectoryLoading ? (
            <WorkspaceLoadingState className="workspace-loading-state--overlay" label={t.loadingRemoteDirectory} />
          ) : null}
        </div>
      </div>
      {isViewLoading || isWorkspaceRefreshing || isWorkspaceSwitching ? (
        <WorkspaceLoadingState className="workspace-loading-state--overlay" label={t.loadingWorkspace} />
      ) : null}
    </div>
  )
}
