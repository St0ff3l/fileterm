import { useEffect, useRef, useState, type CSSProperties, type DragEvent } from 'react'
import type {
  CommandExecutionOptions,
  CommandFolder,
  CommandTemplate,
  LocalFileItem,
  RemoteFileItem,
  SessionSnapshot,
  WorkspaceTab
} from '@fileterm/core'
import { TerminalView } from '../../components/TerminalView'
import type { SendScope, SessionSendTarget } from '../common/session-send-targets'
import { AppIcon } from '../common/AppIcon'
import { FileManager } from '../files/FileManager'
import { TerminalDock } from '../terminal/TerminalDock'
import { t } from '../../i18n'

const DEFAULT_FILE_PANEL_HEIGHT = 218

export function SessionWorkspace({
  activeTab,
  activeSession,
  sendTargets,
  terminalDockSendScope,
  terminalDockSelectedTabIds,
  localItems,
  localPath,
  canPasteToLocal,
  canPasteToRemote,
  clipboardStatusText,
  localCutPaths,
  remoteCutPaths,
  commandFolders,
  commandTemplates,
  isBusy,
  onCopyItems,
  onCutItems,
  onClearCutState,
  onExecuteCommand,
  onSendTerminalCommand,
  onTerminalDockSendScopeChange,
  onTerminalDockSelectedTabIdsChange,
  onOpenCommandManager,
  onOpenLocalItem,
  onOpenLocalPath,
  onOpenRemoteItem,
  onOpenRemotePath,
  onPasteIntoPane,
  onRequestChangePermissions,
  onRequestDelete,
  onRequestNewFile,
  onRequestNewFolder,
  onRequestQuickDelete,
  onRequestRename,
  onToggleFollowShellCwd,
  onToggleRemoteFileAccessMode,
  remoteFileAccessMode,
  isRemoteDirectoryLoading,
  onRefresh,
  onUploadFiles,
  onChooseUploadFiles,
  onDownloadFiles,
  onDropUpload,
  isWorkspaceFocusMode
}: {
  activeTab: WorkspaceTab
  activeSession: SessionSnapshot
  sendTargets: SessionSendTarget[]
  terminalDockSendScope: SendScope
  terminalDockSelectedTabIds: string[]
  localItems: LocalFileItem[]
  localPath: string
  canPasteToLocal: boolean
  canPasteToRemote: boolean
  clipboardStatusText: string | null
  localCutPaths: string[]
  remoteCutPaths: string[]
  commandFolders: CommandFolder[]
  commandTemplates: CommandTemplate[]
  isBusy: boolean
  onCopyItems(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onCutItems(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onClearCutState(): void
  onExecuteCommand(commandId: string, args: string[], options: CommandExecutionOptions, scope: SendScope, selectedTabIds: string[]): void
  onSendTerminalCommand(command: string): Promise<void>
  onTerminalDockSendScopeChange(scope: SendScope, rememberSelection: boolean): void
  onTerminalDockSelectedTabIdsChange(tabIds: string[], rememberSelection: boolean): void
  onOpenCommandManager(): void
  onOpenLocalItem(item: LocalFileItem): void
  onOpenLocalPath(path: string): void
  onOpenRemoteItem(item: RemoteFileItem): void
  onOpenRemotePath(path: string): void
  onPasteIntoPane(pane: 'local' | 'remote'): void
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
  onRefresh(): void
  onUploadFiles(items: LocalFileItem[]): void
  onChooseUploadFiles(): void
  onDownloadFiles(items: RemoteFileItem[], targetDirectory?: string): void
  onDropUpload(event: DragEvent<HTMLDivElement>): void
  isWorkspaceFocusMode: boolean
}) {
  const isFileOnly = activeTab.layout === 'file-only'
  const [filePanelHeight, setFilePanelHeight] = useState(DEFAULT_FILE_PANEL_HEIGHT)
  const [isFilePanelCollapsed, setIsFilePanelCollapsed] = useState(false)
  const workspaceRef = useRef<HTMLElement | null>(null)
  const isResizingFilePanel = useRef(false)
  const hasUserResizedFilePanel = useRef(false)
  const isSnappedToDiskHead = useRef(false)
  const dragStateRef = useRef<{ bottom: number; height: number; snapHeight: number | null } | null>(null)
  const layoutFrameRef = useRef<number | null>(null)
  const lastExpandedFilePanelHeight = useRef(DEFAULT_FILE_PANEL_HEIGHT)
  const appliedWorkspaceFocusMode = useRef<boolean | null>(null)
  const isFilePanelEffectivelyCollapsed = isFilePanelCollapsed && !isFileOnly
  const effectiveFilePanelHeight = isFilePanelEffectivelyCollapsed ? 0 : filePanelHeight

  const clampFilePanelHeight = (workspaceHeight: number, nextHeight: number) => {
    const minHeight = 25 // Allow it to shrink to just the tabs row height
    const maxHeight = Math.max(minHeight, workspaceHeight - 160)
    return Math.min(maxHeight, Math.max(minHeight, nextHeight))
  }

  const syncFilePanelHeight = (mode: 'align' | 'clamp') => {
    if (isFileOnly || isFilePanelCollapsed || !workspaceRef.current || isResizingFilePanel.current) {
      return
    }

    const workspaceRect = workspaceRef.current.getBoundingClientRect()
    if (workspaceRect.height <= 0) {
      return
    }

    if (mode === 'align') {
      const diskHeadRect = document.querySelector('.disk-head')?.getBoundingClientRect()
      if (diskHeadRect) {
        const nextHeight = workspaceRect.bottom - diskHeadRect.top
        const clampedHeight = clampFilePanelHeight(workspaceRect.height, nextHeight)
        
        setFilePanelHeight((prev) => prev === clampedHeight ? prev : clampedHeight)
        isSnappedToDiskHead.current = true
        return
      }
    }

    setFilePanelHeight((prev) => {
      const clampedHeight = clampFilePanelHeight(workspaceRect.height, prev)
      return prev === clampedHeight ? prev : clampedHeight
    })
  }

  useEffect(() => {
    if (!isFilePanelCollapsed && filePanelHeight > 0) {
      lastExpandedFilePanelHeight.current = filePanelHeight
    }
  }, [filePanelHeight, isFilePanelCollapsed])

  useEffect(() => {
    if (isFileOnly) {
      return
    }
    if (appliedWorkspaceFocusMode.current === isWorkspaceFocusMode) {
      return
    }
    appliedWorkspaceFocusMode.current = isWorkspaceFocusMode

    if (isWorkspaceFocusMode) {
      if (!isFilePanelCollapsed && filePanelHeight > 0) {
        lastExpandedFilePanelHeight.current = filePanelHeight
      }
      isResizingFilePanel.current = false
      dragStateRef.current = null
      setIsFilePanelCollapsed(true)
      return
    }

    setFilePanelHeight((prev) => prev > 0 ? prev : lastExpandedFilePanelHeight.current || DEFAULT_FILE_PANEL_HEIGHT)
    setIsFilePanelCollapsed(false)
  }, [isFileOnly, isWorkspaceFocusMode])

  useEffect(() => {
    if (isFileOnly) {
      return
    }

    let dragFrame: number | null = null

    const onMouseMove = (event: globalThis.MouseEvent) => {
      if (!isResizingFilePanel.current || !dragStateRef.current) {
        return
      }

      const { bottom, height, snapHeight } = dragStateRef.current
      let nextHeight = bottom - event.clientY

      let isSnapped = false
      if (snapHeight !== null && Math.abs(nextHeight - snapHeight) <= 10) {
        nextHeight = snapHeight
        isSnapped = true
      }
      isSnappedToDiskHead.current = isSnapped

      if (dragFrame) {
        window.cancelAnimationFrame(dragFrame)
      }

      dragFrame = window.requestAnimationFrame(() => {
        setFilePanelHeight((prev) => {
          const clamped = clampFilePanelHeight(height, nextHeight)
          return prev === clamped ? prev : clamped
        })
      })
    }

    const onMouseUp = () => {
      isResizingFilePanel.current = false
      dragStateRef.current = null
      if (dragFrame) {
        window.cancelAnimationFrame(dragFrame)
        dragFrame = null
      }
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }

    window.addEventListener('mousemove', onMouseMove)
    window.addEventListener('mouseup', onMouseUp)

    return () => {
      window.removeEventListener('mousemove', onMouseMove)
      window.removeEventListener('mouseup', onMouseUp)
      if (dragFrame) {
        window.cancelAnimationFrame(dragFrame)
      }
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }
  }, [isFileOnly])

  useEffect(() => {
    hasUserResizedFilePanel.current = false
  }, [activeTab.id])

  useEffect(() => {
    if (isFileOnly || isFilePanelCollapsed) {
      return
    }

    const frame = window.requestAnimationFrame(() => {
      if (!hasUserResizedFilePanel.current || isSnappedToDiskHead.current) {
        syncFilePanelHeight('align')
      }
    })

    return () => window.cancelAnimationFrame(frame)
  }, [isFileOnly, isFilePanelCollapsed, activeTab.id])

  useEffect(() => {
    if (isFileOnly || isFilePanelCollapsed || !workspaceRef.current) {
      return
    }

    const syncAfterLayout = () => {
      if (layoutFrameRef.current !== null) {
        window.cancelAnimationFrame(layoutFrameRef.current)
      }

      layoutFrameRef.current = window.requestAnimationFrame(() => {
        layoutFrameRef.current = null
        const mode = !hasUserResizedFilePanel.current || isSnappedToDiskHead.current ? 'align' : 'clamp'
        syncFilePanelHeight(mode)
      })
    }

    const resizeObserver = new ResizeObserver(() => {
      syncAfterLayout()
    })
    resizeObserver.observe(workspaceRef.current)

    window.addEventListener('resize', syncAfterLayout)

    return () => {
      if (layoutFrameRef.current !== null) {
        window.cancelAnimationFrame(layoutFrameRef.current)
        layoutFrameRef.current = null
      }
      resizeObserver.disconnect()
      window.removeEventListener('resize', syncAfterLayout)
    }
  }, [isFileOnly, isFilePanelCollapsed, activeTab.id])

  const handleToggleFilePanelCollapsed = () => {
    if (isFilePanelCollapsed) {
      setFilePanelHeight((prev) => prev > 0 ? prev : lastExpandedFilePanelHeight.current || DEFAULT_FILE_PANEL_HEIGHT)
      setIsFilePanelCollapsed(false)
      return
    }

    if (filePanelHeight > 0) {
      lastExpandedFilePanelHeight.current = filePanelHeight
    }
    isResizingFilePanel.current = false
    dragStateRef.current = null
    setIsFilePanelCollapsed(true)
  }

  return (
    <section
      className={`session-workspace ${isFileOnly ? 'file-only' : ''} ${isFilePanelEffectivelyCollapsed ? 'file-panel-collapsed' : ''}`}
      ref={workspaceRef}
      style={{ '--file-panel-height': `${effectiveFilePanelHeight}px` } as CSSProperties}
    >
      {!isFileOnly ? (
        <div className="terminal-area has-terminal-dock">
          <TerminalView
            tabId={activeTab.id}
            bootText={activeSession.terminalTranscript ?? ''}
            connected={activeSession.connected === true}
          />
          <TerminalDock
            activeTab={activeTab}
            connected={activeSession.connected === true}
            selectedTabIds={terminalDockSelectedTabIds}
            sendScope={terminalDockSendScope}
            sendTargets={sendTargets}
            onSelectedTabIdsChange={onTerminalDockSelectedTabIdsChange}
            onSendCommand={onSendTerminalCommand}
            onSendScopeChange={onTerminalDockSendScopeChange}
          />
        </div>
      ) : null}
      {!isFileOnly ? (
        <button
          aria-label={isFilePanelCollapsed ? t.terminalDockShowFilePanel : t.terminalDockHideFilePanel}
          aria-pressed={isFilePanelCollapsed}
          className={`file-panel-drawer-toggle ${isFilePanelCollapsed ? 'is-collapsed' : ''}`}
          title={isFilePanelCollapsed ? t.terminalDockShowFilePanel : t.terminalDockHideFilePanel}
          type="button"
          onClick={handleToggleFilePanelCollapsed}
        >
          <AppIcon name={isFilePanelCollapsed ? 'chevron-up' : 'chevron-down'} size={15} />
        </button>
      ) : null}
      {!isFileOnly && !isFilePanelCollapsed ? (
        <div
          className="session-split-resizer"
          onMouseDown={() => {
            isResizingFilePanel.current = true
            hasUserResizedFilePanel.current = true
            
            if (workspaceRef.current) {
              const rect = workspaceRef.current.getBoundingClientRect()
              const diskHeadRect = document.querySelector('.disk-head')?.getBoundingClientRect()
              dragStateRef.current = {
                bottom: rect.bottom,
                height: rect.height,
                snapHeight: diskHeadRect ? rect.bottom - diskHeadRect.top : null
              }
            }
            
            document.body.style.cursor = 'row-resize'
            document.body.style.userSelect = 'none'
          }}
          role="separator"
        />
      ) : null}
      <FileManager
        activeSession={activeSession}
        activeTab={activeTab}
        sendTargets={sendTargets}
        commandFolders={commandFolders}
        commandTemplates={commandTemplates}
        isBusy={isBusy}
        localItems={localItems}
        localPath={localPath}
        canPasteToLocal={canPasteToLocal}
        canPasteToRemote={canPasteToRemote}
        clipboardStatusText={clipboardStatusText}
        localCutPaths={localCutPaths}
        remoteCutPaths={remoteCutPaths}
        onCopyItems={onCopyItems}
        onCutItems={onCutItems}
        onClearCutState={onClearCutState}
        onExecuteCommand={onExecuteCommand}
        onOpenCommandManager={onOpenCommandManager}
        onOpenLocalItem={onOpenLocalItem}
        onOpenLocalPath={onOpenLocalPath}
        onOpenRemoteItem={onOpenRemoteItem}
        onOpenRemotePath={onOpenRemotePath}
        onPasteIntoPane={onPasteIntoPane}
        onRequestChangePermissions={onRequestChangePermissions}
        onRequestDelete={onRequestDelete}
        onRequestNewFile={onRequestNewFile}
        onRequestNewFolder={onRequestNewFolder}
        onRequestQuickDelete={onRequestQuickDelete}
        onRequestRename={onRequestRename}
        onToggleFollowShellCwd={onToggleFollowShellCwd}
        onToggleRemoteFileAccessMode={onToggleRemoteFileAccessMode}
        remoteFileAccessMode={remoteFileAccessMode}
        isRemoteDirectoryLoading={isRemoteDirectoryLoading}
        onRefresh={onRefresh}
        onUploadFiles={onUploadFiles}
        onChooseUploadFiles={onChooseUploadFiles}
        onDownloadFiles={onDownloadFiles}
        onDropUpload={onDropUpload}
      />
    </section>
  )
}
