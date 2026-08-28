import { useEffect, useState } from 'react'
import type { SshCredentialsPromptRequest } from '@fileterm/core'
import { CloseButton } from '../common/CloseButton'
import { StableButtonContent } from '../common/StableButtonContent'
import { t } from '../../i18n'

export function SshCredentialsModal({
  errorMessage,
  isSubmitting = false,
  request,
  onCancel,
  onSubmit
}: {
  errorMessage?: string | null
  isSubmitting?: boolean
  request: SshCredentialsPromptRequest
  onCancel(): void
  onSubmit(input: { username: string; password: string }): void
}) {
  const [username, setUsername] = useState(request.username ?? '')
  const [password, setPassword] = useState('')

  useEffect(() => {
    setUsername(request.username ?? '')
    setPassword('')
  }, [request])

  return (
    <div className="modal-backdrop">
      <div className="modal-card ssh-interaction-modal">
        <div className="modal-header">
          <span>{t.sshAuthPromptTitle}</span>
          <CloseButton disabled={isSubmitting} onClick={onCancel} />
        </div>

        <div className="root-access-description">{t.sshAuthPromptDescription}</div>

        <div className="root-access-meta">
          <span>{t.host}</span>
          <strong>{`${request.host}:${request.port}`}</strong>
        </div>

        <label className="file-action-field">
          <span>{t.sshAuthPromptUsername}</span>
          <input
            autoFocus
            disabled={isSubmitting}
            value={username}
            onChange={(event) => setUsername(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                onSubmit({ username, password })
              }
            }}
          />
        </label>

        <label className="file-action-field">
          <span>{t.password}</span>
          <input
            type="password"
            disabled={isSubmitting}
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                onSubmit({ username, password })
              }
            }}
          />
        </label>

        {request.passwordRequired ? <div className="file-action-hint">{t.sshAuthPromptPasswordRequired}</div> : null}
        {errorMessage ? <div className="modal-error">{errorMessage}</div> : null}

        <div className="form-actions">
          <button className="flat-button" disabled={isSubmitting} onClick={onCancel} type="button">
            {t.cancel}
          </button>
          <button
            aria-busy={isSubmitting}
            className="primary-button"
            disabled={isSubmitting}
            onClick={() => onSubmit({ username, password })}
            type="button"
          >
            <StableButtonContent busy={isSubmitting} label={t.sshAuthPromptConfirm} />
          </button>
        </div>
      </div>
    </div>
  )
}
