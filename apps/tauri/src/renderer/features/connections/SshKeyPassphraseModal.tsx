import { useEffect, useState } from 'react'
import type { SshKeyPassphrasePromptRequest } from '@fileterm/core'
import { t } from '../../i18n'
import { CloseButton } from '../common/CloseButton'

export function SshKeyPassphraseModal({
  errorMessage,
  isSubmitting = false,
  request,
  onCancel,
  onSubmit
}: {
  errorMessage?: string | null
  isSubmitting?: boolean
  request: SshKeyPassphrasePromptRequest
  onCancel(): void
  onSubmit(input: { passphrase: string; savePassphrase: boolean }): void
}) {
  const [passphrase, setPassphrase] = useState('')
  const [savePassphrase, setSavePassphrase] = useState(false)

  useEffect(() => {
    setPassphrase('')
    setSavePassphrase(false)
  }, [request])

  const submit = () => onSubmit({ passphrase, savePassphrase })

  return (
    <div className="modal-backdrop">
      <div className="modal-card ssh-interaction-modal">
        <div className="modal-header">
          <span>{t.sshKeyPassphraseTitle}</span>
          <CloseButton disabled={isSubmitting} onClick={onCancel} />
        </div>
        <div className="root-access-description">
          {request.reason === 'invalid-saved' ? t.sshKeyPassphraseInvalidSaved : t.sshKeyPassphraseRequired}
        </div>
        <div className="root-access-meta">
          <span>{t.sshKeyPassphraseKey}</span>
          <strong>{request.keyName}</strong>
        </div>
        <label className="file-action-field">
          <span>{t.sshKeyPassphraseLabel}</span>
          <input
            autoFocus
            disabled={isSubmitting}
            type="password"
            value={passphrase}
            onChange={(event) => setPassphrase(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') submit()
            }}
          />
        </label>
        <label className="ssh-checkbox">
          <input
            checked={savePassphrase}
            disabled={isSubmitting}
            type="checkbox"
            onChange={(event) => setSavePassphrase(event.target.checked)}
          />
          <span>{t.sshKeyPassphraseSave}</span>
        </label>
        {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}
        <div className="form-actions">
          <button className="flat-button" disabled={isSubmitting} onClick={onCancel} type="button">
            {t.cancel}
          </button>
          <button className="primary-button" disabled={!passphrase || isSubmitting} onClick={submit} type="button">
            {isSubmitting ? <span aria-hidden="true" className="button-spinner" /> : null}
            <span>{t.sshKeyPassphraseContinue}</span>
          </button>
        </div>
      </div>
    </div>
  )
}
