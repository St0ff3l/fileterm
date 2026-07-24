import { useEffect, useState } from 'react'
import type { SshKeyFileSelection } from '@fileterm/core'
import { AppIcon } from '../common/AppIcon'
import { ConfirmActionDialog } from '../common/ConfirmActionDialog'
import { DropdownSelect } from '../common/DropdownSelect'
import { formatMessage, t } from '../../i18n'

export function SshKeyNoteDialog({
  errorMessage,
  folders = [],
  initialFolderId,
  initialNote = '',
  initialSourcePath,
  isSubmitting,
  mode,
  onClose,
  onSelectFile,
  onSubmit
}: {
  errorMessage?: string | null
  folders?: Array<{ id: string; name: string }>
  initialFolderId?: string
  initialNote?: string
  initialSourcePath?: string
  isSubmitting: boolean
  mode: 'import' | 'edit'
  onClose(): void
  onSelectFile?(): Promise<SshKeyFileSelection | null>
  onSubmit(note: string, sourcePath?: string, folderId?: string): void
}) {
  const [note, setNote] = useState(initialNote)
  const [folderId, setFolderId] = useState(initialFolderId ?? '')
  const [selectedFile, setSelectedFile] = useState<SshKeyFileSelection | null>(() =>
    initialSourcePath ? selectionFromPath(initialSourcePath) : null
  )
  const [isSelectingFile, setIsSelectingFile] = useState(false)
  const normalizedNote = note.trim()
  const canSubmit = Boolean(normalizedNote && (mode === 'edit' || selectedFile))

  useEffect(() => {
    setNote(initialNote)
    setFolderId(initialFolderId ?? '')
    setSelectedFile(initialSourcePath ? selectionFromPath(initialSourcePath) : null)
  }, [initialFolderId, initialNote, initialSourcePath, mode])

  const selectFile = async () => {
    if (!onSelectFile) return
    setIsSelectingFile(true)
    try {
      const selection = await onSelectFile()
      if (selection) setSelectedFile(selection)
    } catch {
      // useSshKeyLibrary 已将可展示错误写入 error 状态。
    } finally {
      setIsSelectingFile(false)
    }
  }

  const submit = () => {
    if (canSubmit) onSubmit(normalizedNote, selectedFile?.sourcePath, folderId || undefined)
  }

  return (
    <ConfirmActionDialog
      className="ssh-key-import-dialog"
      confirmDisabled={!canSubmit || isSelectingFile}
      confirmLabel={mode === 'import' ? t.sshKeyNoteSave : t.sshKeyNoteSaveNote}
      confirmVariant="primary"
      description={
        <div className="ssh-key-import-dialog__form">
          <label className="ssh-key-note-dialog__field">
            <span>{t.sshKeyNoteLabel}</span>
            <input
              autoFocus
              maxLength={120}
              placeholder={t.sshKeyNotePlaceholder}
              value={note}
              onChange={(event) => setNote(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && canSubmit && !isSubmitting && !isSelectingFile) submit()
              }}
            />
            <small>{t.sshKeyNoteHint}</small>
          </label>
          {folders.length > 0 ? (
            <label className="ssh-key-note-dialog__field">
              <span>{t.sshKeyNoteFolder}</span>
              <DropdownSelect
                className="ssh-key-select-shell"
                value={folderId}
                options={[
                  { value: '', label: t.sshKeyNoteAllKeys },
                  ...folders.map((folder) => ({ value: folder.id, label: folder.name }))
                ]}
                onChange={(value) => setFolderId(value)}
              />
              <small>{t.sshKeyNoteFolderHint}</small>
            </label>
          ) : null}
          {mode === 'import' ? (
            <div className="ssh-key-note-dialog__field">
              <span>{t.sshKeyNoteChooseFile}</span>
              <div className="ssh-key-import-dialog__file-row">
                <div
                  className={`ssh-key-import-dialog__file-name${selectedFile ? ' has-file' : ''}`}
                  title={selectedFile?.sourcePath}
                >
                  <span aria-hidden="true" className="material-symbols-outlined">
                    description
                  </span>
                  <span>{selectedFile?.fileName ?? t.sshKeyNoteNoFile}</span>
                </div>
                <button
                  className="flat-button compact ssh-key-import-dialog__file-button"
                  disabled={isSubmitting || isSelectingFile}
                  onClick={() => void selectFile()}
                  type="button"
                >
                  {isSelectingFile ? (
                    <span aria-hidden="true" className="button-spinner" />
                  ) : (
                    <AppIcon name="folder" size={14} />
                  )}
                  <span>
                    {isSelectingFile
                      ? t.sshKeyNoteSelecting
                      : selectedFile
                        ? t.sshKeyNoteReselect
                        : t.sshKeyNoteSelectFile}
                  </span>
                </button>
              </div>
              {selectedFile?.existingKey ? (
                <div className="ssh-key-import-dialog__duplicate-notice">
                  <span aria-hidden="true" className="material-symbols-outlined">
                    info
                  </span>
                  <div>
                    <strong>{t.sshKeyNoteDuplicateTitle}</strong>
                    <span>
                      {formatMessage(t.sshKeyNoteDuplicateBody, { note: selectedFile.existingKey.note || '—' })}
                    </span>
                  </div>
                </div>
              ) : null}
              <small>{t.sshKeyNoteFileHint}</small>
            </div>
          ) : null}
        </div>
      }
      errorMessage={errorMessage}
      isSubmitting={isSubmitting}
      onClose={onClose}
      onConfirm={submit}
      title={
        <span className="ssh-key-import-dialog__title-content">
          <AppIcon name="key" size={16} />
          <span>{mode === 'import' ? t.sshKeyNoteImportTitle : t.sshKeyNoteEditTitle}</span>
        </span>
      }
    />
  )
}

function selectionFromPath(sourcePath: string): SshKeyFileSelection {
  return {
    sourcePath,
    fileName: sourcePath.split(/[\\/]/).pop() || sourcePath
  }
}
