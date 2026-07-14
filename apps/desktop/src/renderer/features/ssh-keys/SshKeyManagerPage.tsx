import { Fragment, useCallback, useEffect, useMemo, useState } from 'react'
import type { SshKeyMetadata } from '@fileterm/core'
import { AppIcon } from '../common/AppIcon'
import { useSshKeyLibrary } from '../../hooks/useSshKeyLibrary'
import { SshKeyNoteDialog } from './SshKeyNoteDialog'
import { t } from '../../i18n'

const SSH_KEY_MANAGER_UI_STATE = 'ssh-key-manager-ui'

type SshKeyFolder = {
  id: string
  name: string
}

type SshKeyManagerUiState = {
  folders: SshKeyFolder[]
  assignments: Record<string, string>
}

export function SshKeyManagerPage({
  onActiveFolderChange,
  onStatsChange
}: {
  onActiveFolderChange?(name: string): void
  onStatsChange?(stats: { keyCount: number; folderCount: number }): void
}) {
  const desktopApi = window.fileterm
  const { keys, loading, error, clearError, selectKeyFile, importKey, updateNote, deleteKey } = useSshKeyLibrary()
  const [query, setQuery] = useState('')
  const [busy, setBusy] = useState(false)
  const [noteDialog, setNoteDialog] = useState<
    { mode: 'import' } | { mode: 'edit'; keyId: string; initialNote: string } | null
  >(null)
  const [folders, setFolders] = useState<SshKeyFolder[]>([])
  const [assignments, setAssignments] = useState<Record<string, string>>({})
  const [activeFolderId, setActiveFolderId] = useState<'all' | string>('all')
  const [expandedFolderIds, setExpandedFolderIds] = useState<Set<string>>(new Set())
  const [isActionsExpanded, setIsActionsExpanded] = useState(false)
  const [isCreatingFolder, setIsCreatingFolder] = useState(false)
  const [newFolderName, setNewFolderName] = useState('')

  useEffect(() => {
    let disposed = false
    void desktopApi?.getUiStateItem(SSH_KEY_MANAGER_UI_STATE).then((raw) => {
      if (disposed || !raw) return
      try {
        const parsed = JSON.parse(raw) as Partial<SshKeyManagerUiState>
        setFolders(Array.isArray(parsed.folders) ? parsed.folders.filter(isSshKeyFolder) : [])
        setAssignments(parsed.assignments && typeof parsed.assignments === 'object' ? parsed.assignments : {})
      } catch {
        // Ignore an invalid UI state item and use the empty library layout.
      }
    })
    return () => {
      disposed = true
    }
  }, [desktopApi])

  const persistUiState = useCallback(
    (nextFolders: SshKeyFolder[], nextAssignments: Record<string, string>) => {
      setFolders(nextFolders)
      setAssignments(nextAssignments)
      void desktopApi?.setUiStateItem(
        SSH_KEY_MANAGER_UI_STATE,
        JSON.stringify({ folders: nextFolders, assignments: nextAssignments } satisfies SshKeyManagerUiState)
      )
    },
    [desktopApi]
  )

  const folderKeyCount = useCallback(
    (folderId: string) => keys.filter((key) => assignments[key.id] === folderId).length,
    [assignments, keys]
  )

  const activeFolder = folders.find((folder) => folder.id === activeFolderId)
  const selectedKeys = useMemo(
    () => (activeFolderId === 'all' ? keys : keys.filter((key) => assignments[key.id] === activeFolderId)),
    [activeFolderId, assignments, keys]
  )
  const visibleKeys = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase()
    if (!normalized) return selectedKeys
    return selectedKeys.filter((key) =>
      [key.name, key.note, key.algorithm, key.fingerprint].some((value) =>
        value?.toLocaleLowerCase().includes(normalized)
      )
    )
  }, [query, selectedKeys])
  const visibleFolders = useMemo(() => {
    if (activeFolderId !== 'all') return []
    const normalized = query.trim().toLocaleLowerCase()
    return normalized ? folders.filter((folder) => folder.name.toLocaleLowerCase().includes(normalized)) : folders
  }, [activeFolderId, folders, query])
  const hasVisibleRows =
    visibleFolders.length > 0 || visibleKeys.length > 0 || (isCreatingFolder && activeFolderId === 'all')

  const toggleFolder = (folderId: string) => {
    setExpandedFolderIds((current) => {
      const next = new Set(current)
      if (next.has(folderId)) next.delete(folderId)
      else next.add(folderId)
      return next
    })
  }

  useEffect(() => {
    onActiveFolderChange?.(activeFolder?.name ?? '全部密钥')
  }, [activeFolder?.name, onActiveFolderChange])

  useEffect(() => {
    onStatsChange?.({ keyCount: keys.length, folderCount: folders.length })
  }, [folders.length, keys.length, onStatsChange])

  const finishFolderCreation = () => {
    const name = newFolderName.trim()
    setIsCreatingFolder(false)
    setNewFolderName('')
    if (!name || folders.some((folder) => folder.name === name)) return

    const folder = { id: createId('ssh-folder'), name }
    persistUiState([...folders, folder], assignments)
  }

  const renameFolder = (folder: SshKeyFolder) => {
    const name = window.prompt('重命名文件夹', folder.name)?.trim()
    if (!name || name === folder.name || folders.some((item) => item.id !== folder.id && item.name === name)) return
    persistUiState(
      folders.map((item) => (item.id === folder.id ? { ...item, name } : item)),
      assignments
    )
  }

  const removeFolder = (folder: SshKeyFolder) => {
    if (!window.confirm(`确定删除文件夹“${folder.name}”吗？文件夹里的密钥不会被删除。`)) return
    const nextAssignments = { ...assignments }
    Object.keys(nextAssignments).forEach((keyId) => {
      if (nextAssignments[keyId] === folder.id) delete nextAssignments[keyId]
    })
    persistUiState(
      folders.filter((item) => item.id !== folder.id),
      nextAssignments
    )
    setExpandedFolderIds((current) => {
      const next = new Set(current)
      next.delete(folder.id)
      return next
    })
    if (activeFolderId === folder.id) setActiveFolderId('all')
  }

  const handleImport = async (note: string, sourcePath?: string) => {
    if (!sourcePath) return
    setBusy(true)
    try {
      const result = await importKey(note, sourcePath)
      if (result && activeFolderId !== 'all') {
        persistUiState(folders, { ...assignments, [result.key.id]: activeFolderId })
      }
      setNoteDialog(null)
    } catch {
      // useSshKeyLibrary 已将可展示错误写入 error 状态。
    } finally {
      setBusy(false)
    }
  }

  const handleEditNote = async (keyId: string, note: string) => {
    setBusy(true)
    try {
      await updateNote(keyId, note)
      setNoteDialog(null)
    } catch {
      // useSshKeyLibrary 已将可展示错误写入 error 状态。
    } finally {
      setBusy(false)
    }
  }

  const handleDelete = async (keyId: string, name: string) => {
    if (!window.confirm(`确定删除密钥“${name}”吗？此操作不会删除原始文件。`)) return
    await deleteKey(keyId)
    if (assignments[keyId]) {
      const nextAssignments = { ...assignments }
      delete nextAssignments[keyId]
      persistUiState(folders, nextAssignments)
    }
  }

  const openNewKeyDialog = () => {
    clearError()
    setNoteDialog({ mode: 'import' })
    setIsActionsExpanded(false)
  }

  return (
    <section className="ssh-key-manager-page manager-inline connection-manager-modal">
      <header className="connection-manager-header ssh-key-manager-header">
        <span className="connection-manager-title ssh-key-manager-title">
          <span aria-hidden="true" className="material-symbols-outlined">
            key
          </span>
          <span>密钥管理</span>
        </span>
        <label className="connection-manager-search ssh-key-manager-search">
          <AppIcon name="search" size={14} />
          <input
            aria-label="筛选密钥"
            placeholder="筛选密钥..."
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
      </header>

      <div className="connection-manager-layout ssh-key-manager-layout">
        <aside className="connection-manager-sidebar" aria-label="密钥文件夹">
          <button
            className={`connection-manager-sidebar-item ${activeFolderId === 'all' ? 'active' : ''}`}
            type="button"
            onClick={() => setActiveFolderId('all')}
          >
            <span className="connection-manager-sidebar-icon">
              <AppIcon name="brand" size={14} />
            </span>
            <span className="connection-manager-sidebar-label">全部密钥</span>
            <span className="connection-manager-sidebar-count">{keys.length}</span>
          </button>

          {folders.map((folder) => (
            <button
              key={folder.id}
              className={`connection-manager-sidebar-item ${activeFolderId === folder.id ? 'active' : ''}`}
              type="button"
              onClick={() => setActiveFolderId(folder.id)}
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
              <span>名称</span>
              <span>算法 / 指纹</span>
              <span>备注</span>
              <span>导入时间</span>
              <span>引用</span>
              <span>操作</span>
            </div>
            <div className="manager-body connection-manager-body">
              {error && !noteDialog ? <div className="ssh-key-manager-error">{error}</div> : null}
              {isCreatingFolder && activeFolderId === 'all' ? (
                <div className="manager-row folder-row ssh-key-folder-create-row">
                  <span className="ssh-key-folder-name-cell">
                    <span className="folder-icon manager-folder-toggle">
                      <AppIcon name="chevron-right" size={12} />
                    </span>
                    <input
                      autoFocus
                      aria-label="文件夹名称"
                      className="manager-inline-input"
                      placeholder="文件夹名称"
                      type="text"
                      value={newFolderName}
                      onChange={(event) => setNewFolderName(event.target.value)}
                      onBlur={finishFolderCreation}
                      onKeyDown={(event) => {
                        if (event.key === 'Enter') finishFolderCreation()
                        if (event.key === 'Escape') {
                          setIsCreatingFolder(false)
                          setNewFolderName('')
                        }
                      }}
                    />
                  </span>
                  <span>--</span>
                  <span>--</span>
                  <span>--</span>
                  <span>--</span>
                  <span />
                </div>
              ) : null}
              {activeFolderId === 'all'
                ? visibleFolders.map((folder) => {
                    const folderKeys = visibleKeys.filter((key) => assignments[key.id] === folder.id)
                    const isExpanded = expandedFolderIds.has(folder.id)
                    return (
                      <Fragment key={folder.id}>
                        <div
                          role="button"
                          tabIndex={0}
                          className="manager-row folder-row ssh-key-folder-row"
                          onClick={() => toggleFolder(folder.id)}
                          onKeyDown={(event) => {
                            if (event.key === 'Enter' || event.key === ' ') {
                              event.preventDefault()
                              toggleFolder(folder.id)
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
                            <AppIcon name="folder" size={14} />
                            <strong>{folder.name}</strong>
                          </span>
                          <span>--</span>
                          <span>--</span>
                          <span>--</span>
                          <span>--</span>
                          <span className="manager-actions ssh-key-folder-actions">
                            <button
                              aria-label={`重命名文件夹 ${folder.name}`}
                              className="manager-icon-action"
                              title="重命名文件夹"
                              type="button"
                              onMouseDown={(event) => event.stopPropagation()}
                              onPointerDown={(event) => event.stopPropagation()}
                              onClick={(event) => {
                                event.stopPropagation()
                                renameFolder(folder)
                              }}
                            >
                              <AppIcon name="edit" size={14} />
                            </button>
                            <button
                              aria-label={`删除文件夹 ${folder.name}`}
                              className="manager-icon-action danger"
                              title="删除文件夹"
                              type="button"
                              onMouseDown={(event) => event.stopPropagation()}
                              onPointerDown={(event) => event.stopPropagation()}
                              onClick={(event) => {
                                event.stopPropagation()
                                removeFolder(folder)
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
                        {isExpanded
                          ? folderKeys.map((key) => (
                              <SshKeyRow
                                key={key.id}
                                className="ssh-key-nested-row"
                                item={key}
                                onDelete={() => void handleDelete(key.id, key.name)}
                                onEdit={() => {
                                  clearError()
                                  setNoteDialog({ mode: 'edit', keyId: key.id, initialNote: key.note ?? '' })
                                }}
                              />
                            ))
                          : null}
                      </Fragment>
                    )
                  })
                : null}
              {(activeFolderId === 'all'
                ? visibleKeys.filter((key) => !folders.some((folder) => assignments[key.id] === folder.id))
                : visibleKeys
              ).map((key) => (
                <SshKeyRow
                  key={key.id}
                  item={key}
                  onDelete={() => void handleDelete(key.id, key.name)}
                  onEdit={() => {
                    clearError()
                    setNoteDialog({ mode: 'edit', keyId: key.id, initialNote: key.note ?? '' })
                  }}
                />
              ))}
              {!loading && !hasVisibleRows ? (
                <div className="connection-manager-empty ssh-key-manager-empty">
                  <span aria-hidden="true" className="material-symbols-outlined">
                    key_off
                  </span>
                  <strong>{query ? '没有匹配的密钥' : '尚未导入私钥'}</strong>
                  <span>{query ? '尝试其他搜索词。' : '新建密钥后即可在 SSH 连接中复用。'}</span>
                </div>
              ) : null}
              {loading ? <div className="connection-manager-empty">正在加载密钥列表…</div> : null}
            </div>
          </div>

          <div className={`connection-manager-floating-drawer ${isActionsExpanded ? 'expanded' : ''}`}>
            <div className="drawer-options-wrapper">
              <button
                className="drawer-option-btn secondary-btn"
                type="button"
                onClick={() => {
                  setIsCreatingFolder(true)
                  setNewFolderName('')
                  setActiveFolderId('all')
                  setIsActionsExpanded(false)
                }}
              >
                <AppIcon name="folder" size={13} />
                <span>新建文件夹</span>
              </button>
              <button className="drawer-option-btn primary-btn" type="button" onClick={openNewKeyDialog}>
                <AppIcon name="plus" size={13} />
                <span>新建密钥</span>
              </button>
            </div>
            <button
              aria-label="展开操作"
              className="drawer-trigger-btn"
              type="button"
              onClick={() => setIsActionsExpanded((expanded) => !expanded)}
            >
              <AppIcon name="plus" size={16} />
            </button>
          </div>
        </section>
      </div>

      {noteDialog ? (
        <SshKeyNoteDialog
          errorMessage={error}
          initialNote={noteDialog.mode === 'edit' ? noteDialog.initialNote : ''}
          isSubmitting={busy}
          mode={noteDialog.mode}
          onClose={() => {
            if (!busy) setNoteDialog(null)
          }}
          onSelectFile={selectKeyFile}
          onSubmit={(note, sourcePath) => {
            if (noteDialog.mode === 'import') {
              void handleImport(note, sourcePath)
              return
            }
            void handleEditNote(noteDialog.keyId, note)
          }}
        />
      ) : null}
    </section>
  )
}

function SshKeyRow({
  item,
  className,
  onDelete,
  onEdit
}: {
  item: SshKeyMetadata
  className?: string
  onDelete(): void
  onEdit(): void
}) {
  return (
    <div className={`manager-row ssh-key-manager-row${className ? ` ${className}` : ''}`}>
      <span className="ssh-key-name-cell">
        <strong>{item.name}</strong>
        <small>{item.encrypted ? '已加密' : '未加密'}</small>
      </span>
      <span className="ssh-key-fingerprint-cell">
        <span>{item.algorithm}</span>
        <code title={item.fingerprint}>{shortFingerprint(item.fingerprint)}</code>
      </span>
      <span className="ssh-key-note-cell">{item.note || '—'}</span>
      <span className="ssh-key-imported-at">
        {new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(item.importedAt)}
      </span>
      <span>{item.usageCount}</span>
      <span className="manager-actions ssh-key-actions">
        <button aria-label="修改备注" className="manager-icon-action" title="修改备注" type="button" onClick={onEdit}>
          <AppIcon name="edit" size={14} />
        </button>
        <button
          aria-label="删除密钥"
          className="manager-icon-action danger"
          disabled={item.usageCount > 0}
          title={item.usageCount > 0 ? '该密钥仍被连接引用，无法删除' : '删除密钥'}
          type="button"
          onClick={onDelete}
        >
          <AppIcon name="trash" size={14} />
        </button>
      </span>
    </div>
  )
}

function isSshKeyFolder(value: unknown): value is SshKeyFolder {
  return Boolean(
    value &&
    typeof value === 'object' &&
    typeof (value as SshKeyFolder).id === 'string' &&
    typeof (value as SshKeyFolder).name === 'string'
  )
}

function createId(prefix: string) {
  return globalThis.crypto?.randomUUID?.() ?? `${prefix}-${Date.now()}`
}

function shortFingerprint(fingerprint: string) {
  return fingerprint.length > 34 ? `${fingerprint.slice(0, 18)}…${fingerprint.slice(-12)}` : fingerprint
}
