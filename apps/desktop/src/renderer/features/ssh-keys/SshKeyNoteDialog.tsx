import { useEffect, useState } from 'react'
import type { SshKeyFileSelection } from '@fileterm/core'
import { ConfirmActionDialog } from '../common/ConfirmActionDialog'

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
      confirmLabel={mode === 'import' ? '保存' : '保存备注'}
      confirmVariant="primary"
      description={
        <div className="ssh-key-import-dialog__form">
          <label className="ssh-key-note-dialog__field">
            <span>备注信息</span>
            <input
              autoFocus
              maxLength={120}
              placeholder="例如：生产服务器部署密钥"
              value={note}
              onChange={(event) => setNote(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && canSubmit && !isSubmitting && !isSelectingFile) submit()
              }}
            />
            <small>备注用于在连接表单中优先识别密钥，不能为空。</small>
          </label>
          {folders.length > 0 ? (
            <label className="ssh-key-note-dialog__field">
              <span>分类文件夹</span>
              <span className="ft-select-shell ssh-key-select-shell">
                <select value={folderId} onChange={(event) => setFolderId(event.target.value)}>
                  <option value="">全部密钥</option>
                  {folders.map((folder) => (
                    <option key={folder.id} value={folder.id}>
                      {folder.name}
                    </option>
                  ))}
                </select>
                <span aria-hidden="true" className="ft-select-shell__icon material-symbols-outlined">
                  expand_more
                </span>
              </span>
              <small>可将密钥归入文件夹，也可以保留在全部密钥中。</small>
            </label>
          ) : null}
          {mode === 'import' ? (
            <div className="ssh-key-note-dialog__field">
              <span>选择密钥文件</span>
              <div className="ssh-key-import-dialog__file-row">
                <div
                  className={`ssh-key-import-dialog__file-name${selectedFile ? ' has-file' : ''}`}
                  title={selectedFile?.sourcePath}
                >
                  <span aria-hidden="true" className="material-symbols-outlined">
                    description
                  </span>
                  <span>{selectedFile?.fileName ?? '尚未选择私钥文件'}</span>
                </div>
                <button
                  className="flat-button compact"
                  disabled={isSubmitting || isSelectingFile}
                  onClick={() => void selectFile()}
                  type="button"
                >
                  {isSelectingFile ? '选择中…' : selectedFile ? '重新选择' : '选择文件'}
                </button>
              </div>
              {selectedFile?.existingKey ? (
                <div className="ssh-key-import-dialog__duplicate-notice">
                  <span aria-hidden="true" className="material-symbols-outlined">
                    info
                  </span>
                  <div>
                    <strong>该私钥已经导入</strong>
                    <span>
                      当前备注“{selectedFile.existingKey.note || '未填写'}”，保存后将直接使用已有密钥，不会重复创建。
                    </span>
                  </div>
                </div>
              ) : null}
              <small>支持所有文件；保存时会校验是否为有效的 SSH 私钥。</small>
            </div>
          ) : null}
        </div>
      }
      errorMessage={errorMessage}
      isSubmitting={isSubmitting}
      onClose={onClose}
      onConfirm={submit}
      title={mode === 'import' ? '导入 SSH 私钥' : '修改密钥备注'}
    />
  )
}

function selectionFromPath(sourcePath: string): SshKeyFileSelection {
  return {
    sourcePath,
    fileName: sourcePath.split(/[\\/]/).pop() || sourcePath
  }
}
