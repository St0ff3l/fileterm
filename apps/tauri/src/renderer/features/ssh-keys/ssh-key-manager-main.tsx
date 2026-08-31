import { Fragment, type DragEvent, type PointerEvent as ReactPointerEvent } from 'react'
import type { SshKeyMetadata } from '@fileterm/core'
import { formatMessage, t } from '../../i18n'
import { AppIcon } from '../common/app-icon'
import { ManagerInlineFolderRow } from '../common/manager-inline-folder-row'
import { ROOT_DROP_TARGET_ID } from './ssh-key-manager-utils'
import type { DragItem, DragOverState, RenderKeyRow, SortableItem, SshKeyFolder } from './ssh-key-manager-utils'

export function SshKeyManagerMain({
  query,
  onQueryChange,
  activeFolderId,
  onActiveFolderChange,
  dragOver,
  keys,
  orderedFolders,
  folderKeyCount,
  rootItems,
  visibleKeys,
  folders,
  assignments,
  expandedFolderIds,
  dragging,
  editingFolder,
  busy,
  isCreatingFolder,
  newFolderName,
  errorMessage,
  loading,
  hasVisibleRows,
  isActionsExpanded,
  onRootDragOver,
  onRootDragLeave,
  onRootDrop,
  onFolderPointerDown,
  onFolderDragStart,
  onFolderDragOver,
  onFolderDrop,
  onFolderDragEnd,
  onFolderToggle,
  onFolderNameChange,
  onCancelFolderRename,
  onFolderRename,
  onFolderDelete,
  onKeyDragStart,
  onKeyDragOver,
  onKeyDragLeave,
  onKeyDrop,
  onKeyDragEnd,
  onKeyPointerDown,
  onSetDragOver,
  onCreateFolder,
  onNewFolderNameChange,
  onDismissFolderCreation,
  onCreateFolderAction,
  onOpenNewKeyDialog,
  onActionsExpandedChange,
  folderDragClass,
  renderKeyRow
}: {
  query: string
  onQueryChange(value: string): void
  activeFolderId: 'all' | string
  onActiveFolderChange(folderId: string): void
  dragOver: DragOverState | null
  keys: SshKeyMetadata[]
  orderedFolders: SshKeyFolder[]
  folderKeyCount(folderId: string): number
  rootItems: SortableItem[]
  visibleKeys: SshKeyMetadata[]
  folders: SshKeyFolder[]
  assignments: Record<string, string>
  expandedFolderIds: Set<string>
  dragging: DragItem | null
  editingFolder: { id: string; name: string } | null
  busy: boolean
  isCreatingFolder: boolean
  newFolderName: string
  errorMessage: string | null
  loading: boolean
  hasVisibleRows: boolean
  isActionsExpanded: boolean
  onRootDragOver(event: DragEvent): void
  onRootDragLeave(event: DragEvent): void
  onRootDrop(event: DragEvent): void
  onFolderPointerDown(event: ReactPointerEvent, folderId: string): void
  onFolderDragStart(event: DragEvent, folderId: string): void
  onFolderDragOver(event: DragEvent, folderId: string): void
  onFolderDrop(event: DragEvent, folderId: string): void
  onFolderDragEnd(): void
  onFolderToggle(folderId: string): void
  onFolderNameChange(folderId: string, name: string): void
  onCancelFolderRename(): void
  onFolderRename(): void
  onFolderDelete(folder: SshKeyFolder): void
  onKeyDragStart(event: DragEvent, keyId: string): void
  onKeyDragOver(event: DragEvent, keyId: string): void
  onKeyDragLeave(event: DragEvent): void
  onKeyDrop(event: DragEvent, keyId: string): void
  onKeyDragEnd(): void
  onKeyPointerDown(event: ReactPointerEvent, keyId: string): void
  onSetDragOver(value: DragOverState | null): void
  onCreateFolder(name: string): boolean | void | Promise<boolean | void>
  onNewFolderNameChange(name: string): void
  onDismissFolderCreation(): void
  onCreateFolderAction(): void
  onOpenNewKeyDialog(): void
  onActionsExpandedChange(): void
  folderDragClass(folderId: string): string
  renderKeyRow: RenderKeyRow
}) {
  return (
    <>
      <header className="connection-manager-header ssh-key-manager-header">
        <span className="connection-manager-title ssh-key-manager-title">
          <span aria-hidden="true" className="material-symbols-outlined">
            key
          </span>
          <span>{t.sshKeyManager}</span>
        </span>
        <label className="connection-manager-search ssh-key-manager-search">
          <AppIcon name="search" size={14} />
          <input
            aria-label={t.filterKeys}
            placeholder={t.filterKeysPlaceholder}
            type="search"
            value={query}
            onChange={(event) => onQueryChange(event.target.value)}
          />
        </label>
      </header>

      <div className="connection-manager-layout ssh-key-manager-layout">
        <aside className="connection-manager-sidebar" aria-label={t.keyFolders}>
          <button
            className={`connection-manager-sidebar-item ssh-key-root-drop-target ${activeFolderId === 'all' ? 'active' : ''} ${
              dragOver?.id === ROOT_DROP_TARGET_ID ? 'drag-over' : ''
            }`}
            type="button"
            data-fileterm-sort-id={ROOT_DROP_TARGET_ID}
            data-fileterm-sort-kind="root"
            onClick={() => onActiveFolderChange('all')}
            onDragOver={onRootDragOver}
            onDragLeave={onRootDragLeave}
            onDrop={onRootDrop}
          >
            <span className="connection-manager-sidebar-icon">
              <AppIcon name="key" size={14} />
            </span>
            <span className="connection-manager-sidebar-label">{t.allKeys}</span>
            <span className="connection-manager-sidebar-count">{keys.length}</span>
          </button>

          {orderedFolders.map((folder) => (
            <button
              key={folder.id}
              className={`connection-manager-sidebar-item ${activeFolderId === folder.id ? 'active' : ''}`}
              type="button"
              data-fileterm-sort-id={folder.id}
              data-fileterm-sort-kind="folder"
              onClick={() => onActiveFolderChange(folder.id)}
            >
              <span className="connection-manager-sidebar-icon">
                <AppIcon name="folder" size={14} />
              </span>
              <span className="connection-manager-sidebar-label">{folder.name}</span>
              <span className="connection-manager-sidebar-count">{folderKeyCount(folder.id)}</span>
            </button>
          ))}
        </aside>

        <section className="connection-manager-main ssh-key-manager-main">
          <div className="manager-table connection-manager-table ssh-key-manager-table">
            <div className="manager-head">
              <span>{t.name}</span>
              <span>{t.keyAlgorithmFingerprint}</span>
              <span>{t.keyNote}</span>
              <span>{t.importedAt}</span>
              <span>{t.keyReferences}</span>
              <span>{t.actions}</span>
            </div>
            <div className="manager-body connection-manager-body">
              {errorMessage ? <div className="ssh-key-manager-error">{errorMessage}</div> : null}
              {isCreatingFolder && activeFolderId === 'all' ? (
                <ManagerInlineFolderRow
                  afterNameCells={['--', '--', '--', '--', null]}
                  className="ssh-key-folder-create-row"
                  placeholder={t.folderName}
                  value={newFolderName}
                  onChange={onNewFolderNameChange}
                  onCommit={onCreateFolder}
                  onDismiss={onDismissFolderCreation}
                />
              ) : null}
              {activeFolderId === 'all'
                ? rootItems.map((rootItem) => {
                    if (rootItem.kind === 'key') {
                      const key = keys.find((item) => item.id === rootItem.id)
                      return key ? renderKeyRow(key) : null
                    }
                    const folder = folders.find((item) => item.id === rootItem.id)
                    if (!folder) return null
                    const folderKeys = visibleKeys.filter((key) => assignments[key.id] === folder.id)
                    const isExpanded = expandedFolderIds.has(folder.id)
                    const folderClass =
                      `${folderDragClass(folder.id)} ${dragging?.id === folder.id ? 'dragging' : ''}`.trim()
                    return (
                      <Fragment key={folder.id}>
                        <div
                          role="button"
                          tabIndex={0}
                          className={`manager-row folder-row ssh-key-folder-row ${folderClass}`.trim()}
                          data-fileterm-sort-id={folder.id}
                          data-fileterm-sort-kind="folder"
                          draggable={false}
                          onPointerDown={(event) => onFolderPointerDown(event, folder.id)}
                          onDragStart={(event) => onFolderDragStart(event, folder.id)}
                          onDragOver={(event) => onFolderDragOver(event, folder.id)}
                          onDragLeave={(event) => {
                            event.preventDefault()
                            onSetDragOver(null)
                          }}
                          onDrop={(event) => onFolderDrop(event, folder.id)}
                          onDragEnd={onFolderDragEnd}
                          onClick={() => onFolderToggle(folder.id)}
                          onKeyDown={(event) => {
                            if (event.key === 'Enter' || event.key === ' ') {
                              event.preventDefault()
                              onFolderToggle(folder.id)
                            }
                          }}
                        >
                          <span className="ssh-key-folder-name-cell">
                            <span
                              className="folder-icon manager-folder-toggle"
                              style={{ transform: isExpanded ? 'rotate(90deg)' : 'none' }}
                            >
                              <AppIcon name="chevron-right" size={12} />
                            </span>
                            {editingFolder?.id === folder.id ? (
                              <input
                                autoFocus
                                className="manager-inline-input"
                                disabled={busy}
                                value={editingFolder.name}
                                onBlur={onFolderRename}
                                onChange={(event) => onFolderNameChange(folder.id, event.target.value)}
                                onClick={(event) => event.stopPropagation()}
                                onKeyDown={(event) => {
                                  event.stopPropagation()
                                  if (event.key === 'Enter') onFolderRename()
                                  if (event.key === 'Escape' && !busy) onCancelFolderRename()
                                }}
                              />
                            ) : (
                              <span className="manager-node-name">{folder.name}</span>
                            )}
                          </span>
                          <span>--</span>
                          <span>--</span>
                          <span>--</span>
                          <span>--</span>
                          <span className="manager-actions ssh-key-folder-actions">
                            <button
                              aria-label={formatMessage(t.renameFolderNamed, { name: folder.name })}
                              className="manager-icon-action"
                              title={t.renameFolder}
                              type="button"
                              onMouseDown={(event) => event.stopPropagation()}
                              onPointerDown={(event) => event.stopPropagation()}
                              onClick={(event) => {
                                event.stopPropagation()
                                onFolderNameChange(folder.id, folder.name)
                              }}
                            >
                              <AppIcon name="edit" size={14} />
                            </button>
                            <button
                              aria-label={formatMessage(t.deleteFolderNamed, { name: folder.name })}
                              className="manager-icon-action danger"
                              title={t.deleteFolder}
                              type="button"
                              onMouseDown={(event) => event.stopPropagation()}
                              onPointerDown={(event) => event.stopPropagation()}
                              onClick={(event) => {
                                event.stopPropagation()
                                onFolderDelete(folder)
                              }}
                            >
                              <AppIcon name="trash" size={14} />
                            </button>
                          </span>
                        </div>
                        {isExpanded && folderKeys.length === 0 ? (
                          <div className="manager-row empty-folder ssh-key-empty-folder">
                            <span>{t.emptyFolder}</span>
                          </div>
                        ) : null}
                        {isExpanded ? folderKeys.map((key) => renderKeyRow(key, 'ssh-key-nested-row')) : null}
                      </Fragment>
                    )
                  })
                : null}
              {activeFolderId !== 'all' ? visibleKeys.map((key) => renderKeyRow(key)) : null}
              {!loading && !hasVisibleRows ? (
                <div className="connection-manager-empty ssh-key-manager-empty">
                  <span aria-hidden="true" className="material-symbols-outlined">
                    key_off
                  </span>
                  <strong>{query ? t.noMatchingKeys : t.noKeysImported}</strong>
                  <span>{query ? t.tryAnotherSearch : t.noKeysHint}</span>
                </div>
              ) : null}
              {loading ? <div className="connection-manager-empty">{t.loadingKeys}</div> : null}
            </div>
          </div>

          <div className={`connection-manager-floating-drawer ${isActionsExpanded ? 'expanded' : ''}`}>
            <div className="drawer-options-wrapper">
              <button className="drawer-option-btn secondary-btn" type="button" onClick={onCreateFolderAction}>
                <AppIcon name="folder" size={13} />
                <span>{t.newFolder}</span>
              </button>
              <button className="drawer-option-btn primary-btn" type="button" onClick={onOpenNewKeyDialog}>
                <AppIcon name="plus" size={13} />
                <span>{t.newKey}</span>
              </button>
            </div>
            <button
              aria-label={t.expandActions}
              className="drawer-trigger-btn"
              type="button"
              onClick={onActionsExpandedChange}
            >
              <AppIcon name="plus" size={16} />
            </button>
          </div>
        </section>
      </div>
    </>
  )
}
