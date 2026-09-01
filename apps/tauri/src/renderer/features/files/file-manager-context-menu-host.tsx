import type { LocalFileItem, RemoteFileItem } from '@fileterm/core'
import type { FileContextMenuState } from './file-manager-types'
import { FileContextMenu } from './file-context-menu'

export function FileManagerContextMenuHost({
  activeSessionRemotePath,
  canChangeContextPermissions,
  canCopyContextItems,
  canCopyContextPath,
  canCreateFromContext,
  canCutContextItems,
  canDownloadContextItems,
  canOpenContextItem,
  canPasteIntoContextPane,
  canQuickDelete,
  canRenameContextItem,
  canUploadContextItems,
  contextLocalItem,
  contextLocalSelection,
  contextMenu,
  contextRemoteItem,
  contextRemoteSelection,
  localPath,
  onChooseUploadFiles,
  onClose,
  onCopyItems,
  onCopyPath,
  onCutItems,
  onDownloadFiles,
  onDownloadLocalNetworkFiles,
  onOpenLocalItem,
  onOpenRemoteItem,
  onOpenContextTarget,
  onPasteIntoPane,
  onRefresh,
  onRequestChangePermissions,
  onRequestDelete,
  onRequestNewFile,
  onRequestNewFolder,
  onRequestQuickDelete,
  onRequestRename,
  onUploadFiles,
  singleContextItem,
  setResetColumnsTrigger
}: {
  activeSessionRemotePath: string
  canChangeContextPermissions: boolean
  canCopyContextItems: boolean
  canCopyContextPath: boolean
  canCreateFromContext: boolean
  canCutContextItems: boolean
  canDownloadContextItems: boolean
  canOpenContextItem: boolean
  canPasteIntoContextPane: boolean
  canQuickDelete: boolean
  canRenameContextItem: boolean
  canUploadContextItems: boolean
  contextLocalItem: LocalFileItem | null
  contextLocalSelection: LocalFileItem[]
  contextMenu: FileContextMenuState
  contextRemoteItem: RemoteFileItem | null
  contextRemoteSelection: RemoteFileItem[]
  localPath: string
  onChooseUploadFiles(): void
  onClose(): void
  onCopyItems(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onCopyPath(): void
  onCutItems(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onDownloadFiles(items: RemoteFileItem[]): void
  onDownloadLocalNetworkFiles(items: LocalFileItem[]): void
  onOpenLocalItem(item: LocalFileItem): void
  onOpenRemoteItem(item: RemoteFileItem): void
  onOpenContextTarget(): void
  onPasteIntoPane(pane: 'local' | 'remote'): void
  onRefresh(): void
  onRequestChangePermissions(pane: 'local' | 'remote', item: LocalFileItem | RemoteFileItem): void
  onRequestDelete(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onRequestNewFile(pane: 'local' | 'remote', directoryPath: string): void
  onRequestNewFolder(pane: 'local' | 'remote', directoryPath: string): void
  onRequestQuickDelete(pane: 'local' | 'remote', items: Array<LocalFileItem | RemoteFileItem>): void
  onRequestRename(pane: 'local' | 'remote', item: LocalFileItem | RemoteFileItem): void
  onUploadFiles(items: LocalFileItem[]): void
  setResetColumnsTrigger: (value: number | ((previous: number) => number)) => void
  singleContextItem: LocalFileItem | RemoteFileItem | null | undefined
}) {
  return (
    <FileContextMenu
      canChangePermissions={canChangeContextPermissions}
      canCopy={canCopyContextItems}
      canCopyPath={canCopyContextPath}
      canCreate={canCreateFromContext}
      canCut={canCutContextItems}
      canDownload={canDownloadContextItems}
      canOpen={canOpenContextItem}
      canPaste={canPasteIntoContextPane}
      canQuickDelete={canQuickDelete}
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
        onClose()
      }}
      onClose={() => onClose()}
      onCopy={() => {
        const items = contextMenu.pane === 'local' ? contextLocalSelection : contextRemoteSelection
        if (items.length) {
          onCopyItems(contextMenu.pane, items)
        }
        onClose()
      }}
      onCopyPath={onCopyPath}
      onCut={() => {
        const items = contextMenu.pane === 'local' ? contextLocalSelection : contextRemoteSelection
        if (items.length) {
          onCutItems(contextMenu.pane, items)
        }
        onClose()
      }}
      onDelete={() => {
        const items = contextMenu.pane === 'local' ? contextLocalSelection : contextRemoteSelection
        if (items.length) {
          onRequestDelete(contextMenu.pane, items)
        }
        onClose()
      }}
      onDeleteFast={() => {
        const items = contextMenu.pane === 'local' ? contextLocalSelection : contextRemoteSelection
        if (items.length) {
          onRequestQuickDelete(contextMenu.pane, items)
        }
        onClose()
      }}
      onDownload={() => {
        if (contextMenu.pane === 'local') {
          onDownloadLocalNetworkFiles(contextLocalSelection)
        } else {
          onDownloadFiles(contextRemoteSelection)
        }
        onClose()
      }}
      onNewFile={() => {
        onRequestNewFile(contextMenu.pane, contextMenu.pane === 'local' ? localPath : activeSessionRemotePath)
        onClose()
      }}
      onNewFolder={() => {
        onRequestNewFolder(contextMenu.pane, contextMenu.pane === 'local' ? localPath : activeSessionRemotePath)
        onClose()
      }}
      onOpen={onOpenContextTarget}
      onPaste={() => {
        onPasteIntoPane(contextMenu.pane)
        onClose()
      }}
      onRefresh={() => {
        onRefresh()
        setResetColumnsTrigger((prev) => prev + 1)
        onClose()
      }}
      onRename={() => {
        const item = singleContextItem
        if (item) {
          onRequestRename(contextMenu.pane, item)
        }
        onClose()
      }}
      onUpload={() => {
        if (contextLocalItem) {
          if (contextLocalSelection.length) {
            onUploadFiles(contextLocalSelection)
          }
        } else {
          onChooseUploadFiles()
        }
        onClose()
      }}
    />
  )
}
