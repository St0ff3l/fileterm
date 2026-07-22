import { useEffect, useState } from 'react'
import { CloseButton } from '../common/CloseButton'
import { t } from '../../i18n'

function isSmbHostPath(path: string) {
  const withoutScheme = path.trim().replace(/^smb:\/\//i, '')
  return withoutScheme.split(/[\\/]+/).filter(Boolean).length === 1
}

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
  const needsShareSelection = isSmbHostPath(path)

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
        <div className="network-share-modal__header">
          <span className="network-share-modal__title">
            <span aria-hidden="true" className="material-symbols-outlined">
              folder_shared
            </span>
            <span>{t.networkShareCredentialsTitle}</span>
          </span>
          <CloseButton className="network-share-modal__close" disabled={isSubmitting} onClick={onCancel} />
        </div>

        <div className="network-share-modal__body">
          <fieldset className="ssh-fieldset network-share-fieldset">
            <legend>{t.general}</legend>
            <div className="network-share-description">
              {needsShareSelection ? t.networkShareCredentialsHostDescription : t.networkShareCredentialsDescription}
            </div>
            <div className="network-share-path-value">
              <span>{t.networkShareCredentialsPath}</span>
              <strong title={path}>{path}</strong>
            </div>
          </fieldset>

          <fieldset className="ssh-fieldset network-share-fieldset" disabled={isSubmitting}>
            <legend>{t.auth}</legend>
            <label className="file-action-field">
              <span>{t.networkShareCredentialsUsername}</span>
              <input
                autoFocus
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
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' && !isSubmitting) {
                    submit()
                  }
                }}
              />
            </label>
          </fieldset>

          {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}

          <div className="form-actions">
            <button className="flat-button" disabled={isSubmitting} onClick={onCancel} type="button">
              {t.cancel}
            </button>
            <button className="primary-button" disabled={isSubmitting} onClick={submit} type="button">
              {isSubmitting ? <span aria-hidden="true" className="button-spinner" /> : null}
              <span>{needsShareSelection ? t.networkShareCredentialsSelect : t.networkShareCredentialsConfirm}</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
