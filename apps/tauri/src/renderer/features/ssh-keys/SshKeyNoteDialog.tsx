import { useEffect, useState } from 'react'
import type { SshKeyFileSelection, SshKeyImportSource } from '@fileterm/core'
import { AppIcon } from '../common/AppIcon'
import { ConfirmActionDialog } from '../common/ConfirmActionDialog'
import { DropdownSelect } from '../common/DropdownSelect'
import { formatMessage, t } from '../../i18n'

const MAX_PRIVATE_KEY_TEXT_LENGTH = 1024 * 1024

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
  onSubmit(note: string, source: SshKeyImportSource, folderId?: string): void
}) {
  const [note, setNote] = useState(initialNote)
  const [folderId, setFolderId] = useState(initialFolderId ?? '')
  const [inputMode, setInputMode] = useState<'file' | 'text'>('file')
  const [selectedFile, setSelectedFile] = useState<SshKeyFileSelection | null>(() =>
    initialSourcePath ? selectionFromPath(initialSourcePath) : null
  )
  const [privateKeyText, setPrivateKeyText] = useState('')
  const [isSelectingFile, setIsSelectingFile] = useState(false)
  const normalizedNote = note.trim()
  const hasImportSource = inputMode === 'file' ? Boolean(selectedFile) : Boolean(privateKeyText.trim())
  const canSubmit = Boolean(normalizedNote && (mode === 'edit' || hasImportSource))

  useEffect(() => {
    setNote(initialNote)
    setFolderId(initialFolderId ?? '')
    setInputMode('file')
    setSelectedFile(initialSourcePath ? selectionFromPath(initialSourcePath) : null)
    setPrivateKeyText('')
  }, [initialFolderId, initialNote, initialSourcePath, mode])

  const selectInputMode = (nextMode: 'file' | 'text') => {
    setInputMode(nextMode)
    if (nextMode === 'file') setPrivateKeyText('')
    else setSelectedFile(null)
  }

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
    if (!canSubmit) return
    const source: SshKeyImportSource =
      inputMode === 'file' ? { sourcePath: selectedFile?.sourcePath } : { content: privateKeyText }
    onSubmit(normalizedNote, source, folderId || undefined)
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
            <>
              <div aria-label={t.sshKeyNoteInputMode} className="ssh-key-import-dialog__source-switch" role="tablist">
                <button
                  aria-selected={inputMode === 'file'}
                  className={inputMode === 'file' ? 'is-active' : ''}
                  onClick={() => selectInputMode('file')}
                  role="tab"
                  type="button"
                >
                  <AppIcon name="folder" size={14} />
                  <span>{t.sshKeyNoteInputFile}</span>
                </button>
                <button
                  aria-selected={inputMode === 'text'}
                  className={inputMode === 'text' ? 'is-active' : ''}
                  onClick={() => selectInputMode('text')}
                  role="tab"
                  type="button"
                >
                  <AppIcon name="edit" size={14} />
                  <span>{t.sshKeyNoteInputText}</span>
                </button>
              </div>
              {inputMode === 'file' ? (
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
              ) : (
                <label className="ssh-key-note-dialog__field">
                  <span>{t.sshKeyNotePasteText}</span>
                  <textarea
                    autoComplete="off"
                    autoCorrect="off"
                    autoFocus
                    maxLength={MAX_PRIVATE_KEY_TEXT_LENGTH}
                    placeholder={t.sshKeyNoteTextPlaceholder}
                    spellCheck={false}
                    value={privateKeyText}
                    wrap="off"
                    onChange={(event) => setPrivateKeyText(event.target.value)}
                  />
                  <small>{t.sshKeyNoteTextHint}</small>
                </label>
              )}
            </>
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
