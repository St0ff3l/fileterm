import { useEffect, useState } from 'react'
import { CloseButton } from '../common/CloseButton'
import { t } from '../../i18n'

export function SmbCredentialsModal({
  errorMessage,
  isSubmitting = false,
  path,
  onCancel,
  onSubmit
}: {
  errorMessage?: string | null
  isSubmitting?: boolean
  path: string
  onCancel(): void
  onSubmit(input: { username: string; password: string }): void
}) {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')

  useEffect(() => {
    setUsername('')
    setPassword('')
  }, [path])

  const submit = () => {
    onSubmit({ username, password })
  }

  return (
    <div className="modal-backdrop">
      <div className="modal-card ssh-interaction-modal network-share-credentials-modal">
        <div className="modal-header">
          <span>{t.networkShareCredentialsTitle}</span>
          <CloseButton disabled={isSubmitting} onClick={onCancel} />
        </div>

        <div className="root-access-description">{t.networkShareCredentialsDescription}</div>

        <div className="root-access-meta network-share-credentials-path">
          <span>{t.networkShareCredentialsPath}</span>
          <strong title={path}>{path}</strong>
        </div>

        <label className="file-action-field">
          <span>{t.networkShareCredentialsUsername}</span>
          <input
            autoFocus
            disabled={isSubmitting}
            value={username}
            onChange={(event) => setUsername(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !isSubmitting) {
                submit()
              }
            }}
          />
        </label>

        <label className="file-action-field">
          <span>{t.networkShareCredentialsPassword}</span>
          <input
            type="password"
            disabled={isSubmitting}
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !isSubmitting) {
                submit()
              }
            }}
          />
        </label>

        {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}

        <div className="form-actions">
          <button className="flat-button" disabled={isSubmitting} onClick={onCancel} type="button">
            {t.cancel}
          </button>
          <button className="primary-button" disabled={isSubmitting} onClick={submit} type="button">
            {isSubmitting ? <span aria-hidden="true" className="button-spinner" /> : null}
            <span>{t.networkShareCredentialsConfirm}</span>
          </button>
        </div>
      </div>
    </div>
  )
}
