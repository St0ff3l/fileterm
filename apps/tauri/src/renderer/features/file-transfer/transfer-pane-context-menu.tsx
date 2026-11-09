import { useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import type { FileTermDesktopApi, RemoteFileItem, WorkspaceTab, WorkspaceSnapshot } from '@fileterm/core'
import { createFileOperationsActions } from '../../hooks/file-operations-actions'
import type { FileActionDialog, PermissionDialogState } from '../../hooks/file-operations-types'
import { allocateTargetNames, joinLocalPath, joinRemotePath } from '../../hooks/file-operations-utils'
import { getRemoteFileEditorBlockReason } from '../../hooks/use-file-editor'
import { getLocale, t } from '../../i18n'
import { ConfirmActionDialog } from '../common/confirm-action-dialog'
import { FileContextMenu } from '../files/file-context-menu'
import { FileActionModal } from '../files/file-action-modal'
import { FilePermissionModal } from '../files/file-permission-modal'
import { toPathBookmark } from '../files/path-bookmark-provider'
import type { TransferPaneState } from './use-transfer-pane'

export type TransferClipboard = { endpoint: string; operation: 'copy' | 'cut'; items: RemoteFileItem[] }
export type TransferPaneMenu = { x: number; y: number; items: RemoteFileItem[] }

export function TransferPaneContextMenu({
  api,
  onApplySnapshot = () => {},
  pane,
  tab,
  menu,
  busy,
  policy,
  clipboard,
  clipboardConnected,
  canCopyAcross,
  copyLabel,
  onClose,
  onBusyChange,
  onCopyAcross,
  onPasteAcross,
  onClipboardChange,
  onMutated
}: {
  api?: FileTermDesktopApi
  onApplySnapshot?(snapshot: WorkspaceSnapshot): void
  pane: TransferPaneState
  tab?: WorkspaceTab
  menu: TransferPaneMenu | null
  busy: boolean
  policy: 'skip' | 'replace'
  clipboard: TransferClipboard | null
  clipboardConnected: boolean
  canCopyAcross: boolean
  copyLabel: string
  onClose(): void
  onBusyChange(value: boolean): void
  onCopyAcross(items: RemoteFileItem[]): void
  onPasteAcross(clipboard: TransferClipboard): void
  onClipboardChange(value: TransferClipboard | null): void
  onMutated(): Promise<void>
}) {
  const local = pane.endpoint === 'local'
  const kind = local ? 'local' : 'remote'
  const ready = Boolean(api && pane.connected && !pane.loading && !pane.error && !busy && (local || tab))
  const items = menu?.items ?? []
  const item = items.length === 1 ? items[0] : null
  const [dialog, setDialog] = useState<FileActionDialog | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const submittingRef = useRef(false)
  const [permission, setPermission] = useState<PermissionDialogState | null>(null)
  const [permissionError, setPermissionError] = useState<string | null>(null)
  const [permissionSubmitting, setPermissionSubmitting] = useState(false)
  const permissionRef = useRef(false)
  const [error, setError] = useState<string | null>(null)
  const runningRef = useRef(false)
  const actions = createFileOperationsActions({
    desktopApi: api,
    activeTab: tab ?? null,
    onApplySnapshot,
    onBusyChange,
    fileActionDialog: dialog,
    setFileActionDialog: setDialog,
    setFileActionError: setActionError,
    fileActionSubmittingRef: submittingRef,
    setIsFileActionSubmitting: setSubmitting,
    permissionDialog: permission,
    setPermissionDialog: setPermission,
    setPermissionDialogError: setPermissionError,
    permissionSubmittingRef: permissionRef,
    setIsPermissionSubmitting: setPermissionSubmitting,
    reportOperationError: (setter, _scope, cause) => setter(String(cause)),
    reportStatusError: (_scope, cause) => setError(String(cause)),
    ensureActiveRemoteSessionConnected: (setter) => {
      if (pane.connected && !pane.error) return true
      const report = setter ?? setError
      report(t.fileTransferReconnect)
      return false
    },
    refreshCurrentPane: onMutated
  })
  const run = (operation: (api: FileTermDesktopApi) => Promise<void>) => {
    if (!ready || !api || runningRef.current) return
    runningRef.current = true
    onBusyChange(true)
    setError(null)
    void operation(api)
      .catch((cause: unknown) => setError(String(cause)))
      .finally(() => {
        runningRef.current = false
        onBusyChange(false)
      })
  }
  const paste = () => {
    if (!clipboard) return
    if (clipboard.endpoint !== pane.endpoint) {
      if (clipboard.operation === 'copy') onPasteAcross(clipboard)
      return
    }
    run(async (api) => {
      const names = allocateTargetNames(
        clipboard.items.map((item) => ({ ...item, pane: kind })),
        pane.items.map((item) => item.name),
        clipboard.operation,
        pane.path
      )
      for (const [index, source] of clipboard.items.entries()) {
        const destination = (local ? joinLocalPath : joinRemotePath)(pane.path, names[index])
        if (source.path === destination) continue
        if (local) {
          if (clipboard.operation === 'cut') await api.moveLocalPath(source.path, destination)
          else await api.copyLocalPath(source.path, destination)
        } else {
          onApplySnapshot(
            clipboard.operation === 'cut'
              ? await api.moveRemotePath(pane.endpoint, source.path, destination)
              : await api.copyRemotePath(pane.endpoint, source.path, destination, source.type)
          )
        }
      }
      if (clipboard.operation === 'cut') onClipboardChange(null)
      await onMutated()
    })
  }
  return (
    <>
      {menu && (
        <FileContextMenu
          disabled={!ready}
          pane={kind}
          position={menu}
          item={item ?? items[0] ?? null}
          onClose={onClose}
          transferAction={{
            label: copyLabel,
            disabled: !canCopyAcross || !items.length,
            action: () => onCopyAcross(items)
          }}
          canOpen={Boolean(item)}
          canCopy={items.length > 0}
          canCut={items.length > 0}
          canCopyPath={Boolean(item)}
          canCreate={items.length <= 1}
          canRename={Boolean(item)}
          canChangePermissions={Boolean(item)}
          canQuickDelete={!local && (tab?.sessionType === 'ssh' || tab?.sessionType === 'sftp')}
          canPaste={Boolean(
            clipboard && clipboardConnected && (clipboard.operation === 'copy' || clipboard.endpoint === pane.endpoint)
          )}
          canDownload={!local && items.length > 0}
          canUpload={!local || (canCopyAcross && items.length > 0)}
          onRefresh={() => void pane.refresh()}
          onBookmark={(item) =>
            run(async (api) => {
              await api.updatePathBookmark(
                local ? 'local' : `remote:${tab!.profileId}`,
                'add',
                toPathBookmark(local ? 'local' : `remote:${tab!.profileId}`, item)
              )
            })
          }
          onOpen={() => {
            if (item)
              run(async (api) => {
                if (item.type === 'folder') await pane.load(item.path)
                else {
                  const reason = !local && getRemoteFileEditorBlockReason(item, getLocale())
                  if (reason) throw new Error(reason)
                  await api.openFileEditorWindow({
                    source: kind,
                    path: item.path,
                    name: item.name,
                    tabId: local ? undefined : pane.endpoint,
                    encoding: 'utf-8'
                  })
                }
              })
          }}
          onCopy={() => onClipboardChange({ endpoint: pane.endpoint, operation: 'copy', items })}
          onCut={() => onClipboardChange({ endpoint: pane.endpoint, operation: 'cut', items })}
          onPaste={paste}
          onCopyPath={() => {
            if (item) run(async (api) => api.writeClipboardText(item.path))
          }}
          onNewFolder={() => actions.requestNewFolder(kind, pane.path)}
          onNewFile={() => actions.requestNewFile(kind, pane.path)}
          onRename={() => {
            if (item) actions.requestRename(kind, item)
          }}
          onDelete={() => actions.requestDelete(kind, items)}
          onDeleteFast={() => actions.requestDelete(kind, items)}
          onChangePermissions={() => {
            if (item) actions.requestChangePermissions(kind, item)
          }}
          onUpload={() => {
            if (local) onCopyAcross(items)
            else
              run(async (api) => {
                const paths = await api.selectLocalFiles()
                for (const path of paths)
                  await api.uploadFile(pane.endpoint, path, pane.path, { conflictPolicy: policy })
              })
          }}
          onDownload={() =>
            run(async (api) => {
              const destination = await api.selectLocalDirectory()
              if (destination)
                for (const item of items)
                  await api.downloadRemotePath(pane.endpoint, item.path, item.type, destination, {
                    conflictPolicy: policy
                  })
            })
          }
        />
      )}
      {createPortal(
        <>
          {dialog?.kind === 'delete' ? (
            <ConfirmActionDialog
              title={t.delete}
              confirmLabel={t.delete}
              description={`${t.deleteConfirmPrefix}${dialog.targets.length > 1 ? `${dialog.targets.length} ${t.itemsSuffix}` : (dialog.targets[0]?.name ?? '')}${t.deleteConfirmSuffix}`}
              errorMessage={actionError}
              isSubmitting={submitting}
              onClose={actions.dismissFileActionDialog}
              onConfirm={() => void actions.handleSubmitFileAction('')}
            />
          ) : dialog ? (
            <FileActionModal
              title={dialog.kind === 'rename' ? t.rename : dialog.kind === 'new-file' ? t.newFile : t.newFolder}
              confirmLabel={t.confirm}
              inputLabel={t.fileName}
              inputPlaceholder={dialog.kind === 'new-folder' ? t.folderName : t.fileName}
              initialValue={dialog.kind === 'rename' ? dialog.target.name : ''}
              hint={dialog.kind === 'new-file' ? t.newFileExtensionHint : undefined}
              errorMessage={actionError}
              isSubmitting={submitting}
              onClose={actions.dismissFileActionDialog}
              onConfirm={(value) => void actions.handleSubmitFileAction(value)}
            />
          ) : null}
          {permission && (
            <FilePermissionModal
              fileName={permission.target.name}
              fileType={permission.target.type}
              targetPath={permission.target.path}
              initialPermission={permission.target.permission}
              ownerGroup={permission.target.ownerGroup}
              supportsRecursive={permission.supportsRecursive}
              errorMessage={permissionError}
              isSubmitting={permissionSubmitting}
              onClose={actions.dismissPermissionDialog}
              onSubmit={(value) => void actions.handleSubmitPermissions(value)}
            />
          )}
          {error && (
            <ConfirmActionDialog
              title={t.fileTransferWorkspace}
              description={error}
              confirmLabel={t.confirm}
              confirmVariant="primary"
              onClose={() => setError(null)}
              onConfirm={() => setError(null)}
            />
          )}
        </>,
        document.body
      )}
    </>
  )
}
