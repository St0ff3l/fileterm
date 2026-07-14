import type { CreateProfileInput } from '@fileterm/core'
import { useState } from 'react'
import { useSshKeyLibrary } from '../../hooks/useSshKeyLibrary'
import { SshKeyNoteDialog } from '../ssh-keys/SshKeyNoteDialog'

export function SshPrivateKeyField({
  form,
  setForm
}: {
  form: CreateProfileInput
  setForm(value: CreateProfileInput | ((previous: CreateProfileInput) => CreateProfileInput)): void
}) {
  const { keys, error, clearError, selectKeyFile, importKey } = useSshKeyLibrary()
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState<string | null>(null)
  const [pendingImport, setPendingImport] = useState<{ sourcePath?: string } | null>(null)

  const selectKey = (privateKeyId: string) => {
    setNotice(null)
    setForm((previous) => ({
      ...previous,
      privateKeyId: privateKeyId || undefined,
      privateKeyPath: privateKeyId ? undefined : previous.privateKeyPath
    }))
  }

  const requestImport = (sourcePath?: string) => {
    clearError()
    setNotice(null)
    setPendingImport({ sourcePath })
  }

  const importNewKey = async (note: string, sourcePath?: string) => {
    setBusy(true)
    setNotice(null)
    try {
      const result = await importKey(note, sourcePath)
      if (result) {
        setForm((previous) => ({ ...previous, privateKeyId: result.key.id, privateKeyPath: undefined }))
        setNotice(
          result.duplicate ? `该密钥已存在，已选中“${result.key.name}”。` : `已导入并选中“${result.key.name}”。`
        )
      }
      setPendingImport(null)
    } catch {
      // useSshKeyLibrary 已将可展示错误写入 error 状态。
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="span-2 ssh-private-key-field">
      <label>
        私钥:
        <span className="ft-select-shell">
          <select value={form.privateKeyId ?? ''} onChange={(event) => selectKey(event.target.value)}>
            <option value="">请选择已导入的密钥</option>
            {keys.map((key) => (
              <option key={key.id} value={key.id}>
                {key.note ? `${key.note} · ${key.name}` : key.name} · {shortFingerprint(key.fingerprint)}
              </option>
            ))}
          </select>
          <span aria-hidden="true" className="ft-select-shell__icon material-symbols-outlined">
            expand_more
          </span>
        </span>
      </label>
      <button
        className="primary-button compact ssh-private-key-action-button"
        disabled={busy}
        onClick={() => requestImport()}
        type="button"
      >
        {busy ? '正在导入…' : '导入新密钥'}
      </button>
      {form.privateKeyPath && !form.privateKeyId ? (
        <div className="ssh-private-key-legacy">
          <span>旧私钥路径：{form.privateKeyPath}</span>
          <button
            className="flat-button compact ssh-private-key-action-button"
            disabled={busy}
            onClick={() => requestImport(form.privateKeyPath)}
            type="button"
          >
            导入到密钥管理器
          </button>
        </div>
      ) : null}
      {notice ? <div className="ssh-private-key-notice">{notice}</div> : null}
      {error && !pendingImport ? <div className="modal-error">{error}</div> : null}
      {pendingImport ? (
        <SshKeyNoteDialog
          errorMessage={error}
          initialSourcePath={pendingImport.sourcePath}
          isSubmitting={busy}
          mode="import"
          onClose={() => {
            if (!busy) setPendingImport(null)
          }}
          onSelectFile={selectKeyFile}
          onSubmit={(note, sourcePath) => void importNewKey(note, sourcePath)}
        />
      ) : null}
    </div>
  )
}

function shortFingerprint(fingerprint: string) {
  return fingerprint.length > 22 ? `${fingerprint.slice(0, 12)}…${fingerprint.slice(-8)}` : fingerprint
}
