import type { CreateProfileInput, SshKeyImportSource } from '@fileterm/core'
import { useState } from 'react'
import { useSshKeyLibrary } from '../../hooks/use-ssh-key-library'
import { DropdownSelect } from '../common/dropdown-select'
import { StableButtonLabel } from '../common/stable-button-content'
import { SshKeyNoteDialog } from '../ssh-keys/ssh-key-note-dialog'
import { formatMessage, t } from '../../i18n'

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

  const importNewKey = async (note: string, source: SshKeyImportSource) => {
    setBusy(true)
    setNotice(null)
    try {
      const result = await importKey(note, source)
      if (result) {
        setForm((previous) => ({ ...previous, privateKeyId: result.key.id, privateKeyPath: undefined }))
        setNotice(
          result.duplicate
            ? formatMessage(t.privateKeyStatusExisting, { name: result.key.name })
            : formatMessage(t.privateKeyStatusImported, { name: result.key.name })
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
        {t.privateKeyLabel}
        <DropdownSelect
          value={form.privateKeyId ?? ''}
          options={[
            { value: '', label: t.privateKeyChooseImported },
            ...keys.map((key) => ({
              value: key.id,
              label: `${key.note ? `${key.note} · ${key.name}` : key.name} · ${shortFingerprint(key.fingerprint)}`
            }))
          ]}
          onChange={(value) => selectKey(value)}
        />
      </label>
      <button
        aria-busy={busy}
        className="primary-button ssh-private-key-action-button"
        disabled={busy}
        onClick={() => requestImport()}
        type="button"
      >
        <StableButtonLabel busy={busy} busyLabel={t.privateKeyImporting} label={t.privateKeyImportNew} />
      </button>
      {form.privateKeyPath && !form.privateKeyId ? (
        <div className="ssh-private-key-legacy">
          <span>
            {t.privateKeyLegacyPath}
            {form.privateKeyPath}
          </span>
          <button
            className="flat-button ssh-private-key-action-button"
            disabled={busy}
            onClick={() => requestImport(form.privateKeyPath)}
            type="button"
          >
            {t.privateKeyImportToManager}
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
          onSubmit={(note, source) => void importNewKey(note, source)}
        />
      ) : null}
    </div>
  )
}

function shortFingerprint(fingerprint: string) {
  return fingerprint.length > 22 ? `${fingerprint.slice(0, 12)}…${fingerprint.slice(-8)}` : fingerprint
}
