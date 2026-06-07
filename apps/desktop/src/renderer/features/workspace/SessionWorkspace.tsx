import { useEffect, useRef, useState, type CSSProperties, type DragEvent } from 'react'
import type {
  CommandExecutionOptions,
  CommandFolder,
  CommandTemplate,
  LocalFileItem,
  RemoteFileItem,
  SessionSnapshot,
  WorkspaceTab
} from '@termdock/core'
import { TerminalView } from '../../components/TerminalView'
import { FileManager } from '../files/FileManager'

export function SessionWorkspace({
  activeTab,
  activeSession,
  tabs,
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
  onToggleRemoteFileAccessMode,
  remoteFileAccessMode,
  onRefresh,
  onUploadFiles,
  onChooseUploadFiles,
  onDownloadFiles,
  onDropUpload
}: {
  activeTab: WorkspaceTab
  activeSession: SessionSnapshot
  tabs: WorkspaceTab[]
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
  onExecuteCommand(commandId: string, args: string[], options: CommandExecutionOptions, scope: 'current' | 'all-ssh'): void
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
  onToggleRemoteFileAccessMode(): void
  remoteFileAccessMode: 'user' | 'root'
  onRefresh(): void
  onUploadFiles(items: LocalFileItem[]): void
  onChooseUploadFiles(): void
  onDownloadFiles(items: RemoteFileItem[], targetDirectory?: string): void
  onDropUpload(event: DragEvent<HTMLDivElement>): void
}) {
  const isFileOnly = activeTab.layout === 'file-only'
  const [filePanelHeight, setFilePanelHeight] = useState(218)
  const workspaceRef = useRef<HTMLElement | null>(null)
  const isResizingFilePanel = useRef(false)
  const hasUserResizedFilePanel = useRef(false)
  const isSnappedToDiskHead = useRef(false)
  const dragStateRef = useRef<{ bottom: number; height: number; snapHeight: number | null } | null>(null)
  const layoutFrameRef = useRef<number | null>(null)

  const clampFilePanelHeight = (workspaceHeight: number, nextHeight: number) => {
    const minHeight = 25 // Allow it to shrink to just the tabs row height
    const maxHeight = Math.max(minHeight, workspaceHeight - 160)
    return Math.min(maxHeight, Math.max(minHeight, nextHeight))
  }

  const syncFilePanelHeight = (mode: 'align' | 'clamp') => {
    if (isFileOnly || !workspaceRef.current || isResizingFilePanel.current) {
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
    if (isFileOnly) {
      return
    }

    const frame = window.requestAnimationFrame(() => {
      if (!hasUserResizedFilePanel.current || isSnappedToDiskHead.current) {
        syncFilePanelHeight('align')
      }
    })

    return () => window.cancelAnimationFrame(frame)
  }, [isFileOnly, activeTab.id])

  useEffect(() => {
    if (isFileOnly || !workspaceRef.current) {
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
  }, [isFileOnly, activeTab.id])

  return (
    <section
      className={`session-workspace ${isFileOnly ? 'file-only' : ''}`}
      ref={workspaceRef}
      style={{ '--file-panel-height': `${filePanelHeight}px` } as CSSProperties}
    >
      {!isFileOnly ? (
        <div className="terminal-area">
          <TerminalView
            key={activeTab.id}
            tabId={activeTab.id}
            bootText={activeSession.terminalTranscript ?? ''}
            connected={activeSession.connected === true}
          />
        </div>
      ) : null}
      {!isFileOnly ? (
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
        tabs={tabs}
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
        onToggleRemoteFileAccessMode={onToggleRemoteFileAccessMode}
        remoteFileAccessMode={remoteFileAccessMode}
        onRefresh={onRefresh}
        onUploadFiles={onUploadFiles}
        onChooseUploadFiles={onChooseUploadFiles}
        onDownloadFiles={onDownloadFiles}
        onDropUpload={onDropUpload}
      />
    </section>
  )
}
