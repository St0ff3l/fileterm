import type { LocalFileItem, RemoteFileItem } from '@fileterm/core'
import { t } from '../../i18n'
import { ContextMenu } from '../common/ContextMenu'

export function FileContextMenu({
  canChangePermissions,
  canCopy,
  canCopyPath,
  canCreate,
  canCut,
  canDownload,
  canOpen,
  canPaste,
  canQuickDelete,
  canRename,
  canUpload,
  item,
  onClose,
  onChangePermissions,
  onCopy,
  onCopyPath,
  onCut,
  onDelete,
  onDeleteFast,
  onDownload,
  onNewFile,
  onNewFolder,
  onOpen,
  onPaste,
  onRefresh,
  onRename,
  onUpload,
  pane,
  position
}: {
  canChangePermissions: boolean
  canCopy: boolean
  canCopyPath: boolean
  canCreate: boolean
  canCut: boolean
  canDownload: boolean
  canOpen: boolean
  canPaste: boolean
  canQuickDelete: boolean
  canRename: boolean
  canUpload: boolean
  item: LocalFileItem | RemoteFileItem | null
  onClose(): void
  onChangePermissions(): void
  onCopy(): void
  onCopyPath(): void
  onCut(): void
  onDelete(): void
  onDeleteFast(): void
  onDownload(): void
  onNewFile(): void
  onNewFolder(): void
  onOpen(): void
  onPaste(): void
  onRefresh(): void
  onRename(): void
  onUpload(): void
  pane: 'local' | 'remote'
  position: { x: number; y: number }
}) {
  const uploadLabel = t.uploadMore
  const canMutateItem = Boolean(item && item.name !== '..')
  const items = [
    { label: t.refresh, action: onRefresh },
    { separator: true },
    { label: t.open, disabled: !canOpen, action: onOpen },
    { label: t.copy, disabled: !canCopy, action: onCopy },
    { label: t.cut, disabled: !canCut, action: onCut },
    { label: t.paste, disabled: !canPaste, action: onPaste },
    { separator: true },
    { label: t.copyPath, disabled: !canCopyPath, action: onCopyPath },
    { label: t.download, disabled: !canDownload, action: onDownload },
    { label: uploadLabel, disabled: !canUpload, action: onUpload },
    { separator: true },
    { label: t.newFolder, disabled: !canCreate, action: onNewFolder },
    { label: t.newFile, disabled: !canCreate, action: onNewFile },
    { separator: true },
    { label: t.rename, disabled: !canRename, action: onRename },
    { label: t.delete, disabled: !canMutateItem, danger: true, action: onDelete },
    ...(canQuickDelete ? [{ label: t.quickDelete, disabled: !canMutateItem, danger: true, action: onDeleteFast }] : []),
    { separator: true },
    { label: t.permissionMore, disabled: !canChangePermissions, action: onChangePermissions }
  ]

  return (
    <ContextMenu
      className="file-context-menu"
      items={items}
      onClose={onClose}
      position={position}
    />
  )
}
