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
  commandFolders,
  commandTemplates,
  isBusy,
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
  activeTab: WorkspaceTab
  activeSession: SessionSnapshot
  tabs: WorkspaceTab[]
  localItems: LocalFileItem[]
  localPath: string
  commandFolders: CommandFolder[]
  commandTemplates: CommandTemplate[]
  isBusy: boolean
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
  const isFileOnly = activeTab.layout === 'file-only'
  const [filePanelHeight, setFilePanelHeight] = useState(218)
  const workspaceRef = useRef<HTMLElement | null>(null)
  const isResizingFilePanel = useRef(false)
  const hasAlignedFilePanel = useRef(false)

  useEffect(() => {
    if (isFileOnly) {
      return
    }

    const onMouseMove = (event: globalThis.MouseEvent) => {
      if (!isResizingFilePanel.current || !workspaceRef.current) {
        return
      }

      const rect = workspaceRef.current.getBoundingClientRect()
      const nextHeight = rect.bottom - event.clientY
      const maxHeight = Math.max(140, rect.height - 160)
      setFilePanelHeight(Math.min(maxHeight, Math.max(140, nextHeight)))
    }

    const onMouseUp = () => {
      isResizingFilePanel.current = false
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }

    window.addEventListener('mousemove', onMouseMove)
    window.addEventListener('mouseup', onMouseUp)

    return () => {
      window.removeEventListener('mousemove', onMouseMove)
      window.removeEventListener('mouseup', onMouseUp)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }
  }, [isFileOnly])

  useEffect(() => {
    hasAlignedFilePanel.current = false
  }, [activeTab.id])

  useEffect(() => {
    if (isFileOnly || hasAlignedFilePanel.current) {
      return
    }

    const frame = window.requestAnimationFrame(() => {
      const workspaceRect = workspaceRef.current?.getBoundingClientRect()
      const diskHeadRect = document.querySelector('.disk-head')?.getBoundingClientRect()

      if (!workspaceRect || !diskHeadRect) {
        return
      }

      const nextHeight = workspaceRect.bottom - diskHeadRect.top
      const maxHeight = Math.max(140, workspaceRect.height - 160)

      if (nextHeight >= 140 && nextHeight <= maxHeight) {
        setFilePanelHeight(nextHeight)
        hasAlignedFilePanel.current = true
      }
    })

    return () => window.cancelAnimationFrame(frame)
  }, [isFileOnly, activeTab.id])

  return (
    <section
      className={`session-workspace ${isFileOnly ? 'file-only' : ''}`}
      ref={workspaceRef}
      style={{ '--file-panel-height': `${filePanelHeight}px` } as CSSProperties}
    >
      {!isFileOnly ? (
        <div className="terminal-area">
          <TerminalView key={activeTab.id} tabId={activeTab.id} initialText={activeSession.terminalTranscript ?? ''} />
        </div>
      ) : null}
      {!isFileOnly ? (
        <div
          className="session-split-resizer"
          onMouseDown={() => {
            isResizingFilePanel.current = true
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
        onExecuteCommand={onExecuteCommand}
        onOpenCommandManager={onOpenCommandManager}
        onOpenLocalItem={onOpenLocalItem}
        onOpenLocalPath={onOpenLocalPath}
        onOpenRemoteItem={onOpenRemoteItem}
        onOpenRemotePath={onOpenRemotePath}
        onRefresh={onRefresh}
        onUploadFiles={onUploadFiles}
        onChooseUploadFiles={onChooseUploadFiles}
        onDownloadFiles={onDownloadFiles}
        onDropUpload={onDropUpload}
      />
    </section>
  )
}
